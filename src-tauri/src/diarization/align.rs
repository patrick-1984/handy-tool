//! Speaker detection, "transcribe first": the recording is transcribed once with
//! the normal engine call, then the engine's own text is handed out to the
//! diarized speakers by time.
//!
//! - Word (or token) timestamps: every word goes to the speaker it overlaps
//!   most ([`assign_words`]).
//! - Segment timestamps only (Whisper): every segment goes to its main
//!   speaker; a segment that clearly mixes two voices is transcribed again
//!   per speaker ([`assign_segments`]), a bounded local fallback.
//!
//! The text itself is cut out of the engine's full transcript rather than
//! rebuilt from the pieces, so spacing and punctuation stay exactly as the
//! engine produced them. No text is ever dropped: text not covered by any
//! speaker (speech the diarizer missed) goes to the nearest speaker.

use super::pipeline::{SAMPLE_RATE, Segment};
use super::turns;
use anyhow::Result;
use log::debug;

/// A word, token or segment of a transcript with its time span in seconds
/// from the start of the recording.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedText {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

impl TimedText {
    pub fn new(start: f32, end: f32, text: impl Into<String>) -> Self {
        Self {
            start,
            end,
            text: text.into(),
        }
    }
}

/// A Whisper segment is re-transcribed per speaker only if its main speaker
/// has less than this share of the segment's speech...
pub const SEGMENT_MAJORITY: f32 = 0.7;
/// ...and the other speakers together talk at least this long (seconds).
pub const SEGMENT_MINORITY_MIN: f32 = 1.0;
/// At most this many segments per recording are re-transcribed, so a long
/// recording full of cross-talk cannot multiply the transcription time.
pub const MAX_RETRANSCRIBED_SEGMENTS: usize = 12;
/// At most this many speaker turns inside one re-transcribed segment.
const MAX_SUB_TURNS: usize = 6;
/// A word is judged by its first this many seconds only. Word end times run
/// late: transcribe-rs ends every token where the next one starts (so a
/// turn's last word reaches across the pause into the next speaker's turn),
/// and TDT models emit trailing punctuation late. The onset is reliable.
pub const WORD_ONSET_SPAN: f32 = 0.3;
/// SentencePiece word marker.
const WORD_MARKER: char = '\u{2581}';

/// Characters that open a phrase and belong to the word after them.
fn is_opening_punctuation(c: char) -> bool {
    matches!(
        c,
        '"' | '\''
            | '“'
            | '‘'
            | '„'
            | '‚'
            | '«'
            | '‹'
            | '¿'
            | '¡'
            | '('
            | '['
            | '{'
            | '「'
            | '『'
    )
}

/// Scripts written without spaces, where every token is its own word.
fn is_unspaced_script(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF // Hiragana, Katakana
        | 0x3400..=0x4DBF // CJK extension A
        | 0x4E00..=0x9FFF // CJK unified ideographs
        | 0xF900..=0xFAFF // CJK compatibility ideographs
        | 0x0E00..=0x0E7F // Thai
    )
}

/// Whether a token or word carries speech (letters or digits), as opposed
/// to punctuation, blanks and markers only.
pub fn has_speech(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

fn clean(piece: &str) -> String {
    piece.replace(WORD_MARKER, " ").trim().to_string()
}

/// Group token rows into words, the same way the engines do: a token that
/// starts with a space or the SentencePiece marker opens a new word, any
/// other token (word pieces, punctuation) extends the current one. Tokens of
/// unspaced scripts (CJK, Thai) are words of their own. Punctuation does not
/// move a word's end: engines emit it late, after the word was spoken.
pub fn group_tokens(tokens: &[TimedText]) -> Vec<TimedText> {
    let mut words: Vec<TimedText> = Vec::new();
    let mut open = false;
    for token in tokens {
        if token.text.trim().is_empty() && !token.text.contains(WORD_MARKER) {
            // Blank/special token: closes nothing, adds nothing.
            continue;
        }
        let first = token.text.chars().next().unwrap_or(' ');
        let starts_word = !open
            || first == ' '
            || first == WORD_MARKER
            || is_unspaced_script(token.text.trim_start().chars().next().unwrap_or(' '));
        let piece = token.text.replace(WORD_MARKER, " ");
        match words.last_mut() {
            Some(word) if !starts_word => {
                word.text.push_str(&piece);
                if has_speech(&piece) {
                    word.end = word.end.max(token.end);
                }
            }
            _ => words.push(TimedText::new(token.start, token.end, piece)),
        }
        open = true;
    }
    for word in &mut words {
        word.text = word.text.trim().to_string();
    }
    words.retain(|w| !w.text.is_empty());
    words
}

/// Byte range of every piece in `text`, searched in order. A blank piece gets
/// `None`. Returns `None` if a piece cannot be found (the engine's text and
/// its timed pieces disagree).
fn locate(text: &str, pieces: &[TimedText]) -> Option<Vec<Option<(usize, usize)>>> {
    let mut cursor = 0;
    let mut out = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let needle = clean(&piece.text);
        if needle.is_empty() {
            out.push(None);
            continue;
        }
        let at = cursor + text[cursor..].find(&needle)?;
        cursor = at + needle.len();
        out.push(Some((at, cursor)));
    }
    Some(out)
}

/// The text owned by each piece: from its start (plus any opening
/// punctuation right before it) to the start of the next located piece. The
/// first located piece also owns everything before it, so concatenating the
/// slices gives back `text` exactly. Unlocated pieces own nothing.
fn owned_slices<'a>(text: &'a str, spans: &[Option<(usize, usize)>]) -> Vec<&'a str> {
    let mut cuts: Vec<Option<usize>> = vec![None; spans.len()];
    let mut prev_end: Option<usize> = None;
    for (i, span) in spans.iter().enumerate() {
        let Some((start, end)) = *span else {
            continue;
        };
        let cut = match prev_end {
            None => 0,
            Some(floor) => {
                let mut cut = start;
                while let Some(c) = text[floor..cut].chars().next_back() {
                    if !is_opening_punctuation(c) {
                        break;
                    }
                    cut -= c.len_utf8();
                }
                cut
            }
        };
        cuts[i] = Some(cut);
        prev_end = Some(end);
    }
    let mut out = vec![""; spans.len()];
    let mut next_cut = text.len();
    for i in (0..spans.len()).rev() {
        if let Some(cut) = cuts[i] {
            out[i] = &text[cut..next_cut];
            next_cut = cut;
        }
    }
    out
}

/// Per-speaker overlap (seconds) of `[start, end)` with the diarized speech.
fn overlap_by_speaker(start: f32, end: f32, segments: &[Segment]) -> Vec<(usize, f32)> {
    let mut out: Vec<(usize, f32)> = Vec::new();
    for seg in segments {
        let overlap = end.min(seg.end) - start.max(seg.start);
        if overlap <= 0.0 {
            continue;
        }
        match out.iter_mut().find(|(s, _)| *s == seg.speaker) {
            Some((_, total)) => *total += overlap,
            None => out.push((seg.speaker, overlap)),
        }
    }
    out
}

/// Speaker of the diarized speech nearest to `t` (seconds).
fn nearest_speaker(t: f32, segments: &[Segment]) -> Option<usize> {
    segments
        .iter()
        .map(|s| {
            let distance = if t < s.start {
                s.start - t
            } else if t > s.end {
                t - s.end
            } else {
                0.0
            };
            (distance, s.speaker)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, speaker)| speaker)
}

/// The speaker a timed piece belongs to: most overlap, ties going to the
/// previous piece's speaker; a piece outside all diarized speech goes to the
/// nearest speaker. `None` only if there is no diarized speech at all.
pub(super) fn speaker_for(
    start: f32,
    end: f32,
    segments: &[Segment],
    prev: Option<usize>,
) -> Option<usize> {
    let mid = (start + end) / 2.0;
    // Zero-width rows (point-in-time tokens) are judged by their midpoint.
    let (start, end) = if end - start < 0.02 {
        (mid - 0.01, mid + 0.01)
    } else {
        (start, end)
    };
    let overlaps = overlap_by_speaker(start, end, segments);
    let best = overlaps.iter().map(|(_, o)| *o).fold(0.0f32, f32::max);
    if best <= 0.0 {
        return nearest_speaker(mid, segments);
    }
    let tied: Vec<usize> = overlaps
        .iter()
        .filter(|(_, o)| best - o < 1e-3)
        .map(|(s, _)| *s)
        .collect();
    match prev {
        Some(p) if tied.contains(&p) => Some(p),
        _ => tied.first().copied(),
    }
}

/// The speaker of a word, judged by its onset ([`WORD_ONSET_SPAN`]).
pub(super) fn word_speaker(
    word: &TimedText,
    segments: &[Segment],
    prev: Option<usize>,
) -> Option<usize> {
    let end = word.end.min(word.start + WORD_ONSET_SPAN).max(word.start);
    speaker_for(word.start, end, segments, prev)
}

/// Append `text` to the transcript as said by `speaker`, joining it with the
/// previous chunk if that one is by the same speaker.
fn push_chunk(chunks: &mut Vec<(usize, String)>, speaker: usize, text: &str) {
    match chunks.last_mut() {
        Some((last, block)) if *last == speaker => block.push_str(text),
        _ => chunks.push((speaker, text.to_string())),
    }
}

/// Like [`push_chunk`] for a separately transcribed piece: makes sure it is
/// separated from the text before it by a space.
fn push_separate(chunks: &mut Vec<(usize, String)>, speaker: usize, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if let Some((last, block)) = chunks.last_mut()
        && *last == speaker
    {
        if !block.ends_with(char::is_whitespace) {
            block.push(' ');
        }
        block.push_str(text);
        return;
    }
    chunks.push((speaker, text.to_string()));
}

fn finish(chunks: Vec<(usize, String)>) -> Vec<(usize, String)> {
    chunks
        .into_iter()
        .map(|(s, t)| (s, t.trim().to_string()))
        .filter(|(_, t)| !t.is_empty())
        .collect()
}

/// Where the timed pieces can be found in the text. If they don't match the
/// engine's text (it was normalised differently), the text is rebuilt from
/// the pieces instead, so nothing is lost either way.
fn text_and_spans(text: &str, pieces: &[TimedText]) -> (String, Vec<Option<(usize, usize)>>) {
    if let Some(spans) = locate(text, pieces) {
        return (text.to_string(), spans);
    }
    debug!("Timed transcript pieces do not match its text; rebuilding the text from them");
    let rebuilt = pieces
        .iter()
        .map(|p| clean(&p.text))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let spans = locate(&rebuilt, pieces).unwrap_or_else(|| vec![None; pieces.len()]);
    (rebuilt, spans)
}

/// Words are only checked for a late clock if there are at least this many
/// speech onsets to compare...
const MIN_LAG_ONSETS: usize = 3;
/// ...and corrected only if the words start this much later than the speech
/// (seconds, median over onsets). Offline models stay well below it (0.1–0.2 s
/// measured, mostly the segmentation's own early onsets); streaming models
/// run in batch report words when they emit them (0.5 s measured).
const MIN_LAG: f32 = 0.3;
/// Normal delay between a diarized onset and the first word's timestamp,
/// left in place when correcting.
const NORMAL_LAG: f32 = 0.1;
/// A pause before speech this long (seconds) makes its start an onset.
const ONSET_GAP: f32 = 0.3;

/// How late (seconds) the word timestamps run behind the diarized speech:
/// the median delay from each speech onset (speech after a pause) to the
/// first word starting there, if it is clearly late; otherwise 0.
pub fn word_lag(words: &[TimedText], segments: &[Segment]) -> f32 {
    let mut sorted: Vec<&Segment> = segments.iter().collect();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut delays = Vec::new();
    let mut speech_end = f32::NEG_INFINITY;
    for seg in sorted {
        if seg.start - speech_end >= ONSET_GAP {
            let first = words
                .iter()
                .map(|w| w.start)
                .filter(|&t| t >= seg.start - 0.15 && t <= seg.start + 1.5)
                .min_by(f32::total_cmp);
            if let Some(t) = first {
                delays.push(t - seg.start);
            }
        }
        speech_end = speech_end.max(seg.end);
    }
    if delays.len() < MIN_LAG_ONSETS {
        return 0.0;
    }
    delays.sort_by(f32::total_cmp);
    let median = delays[delays.len() / 2];
    if median >= MIN_LAG {
        median - NORMAL_LAG
    } else {
        0.0
    }
}

/// Word-level alignment: every word goes to the speaker its onset overlaps
/// most ([`word_speaker`]), and runs of words by the same speaker become one
/// block. Returns `(speaker, text)` blocks in order (speaker ids as in
/// `segments`). Word timestamps that run clearly late (see [`word_lag`]) are
/// moved back first.
pub fn assign_words(text: &str, words: &[TimedText], segments: &[Segment]) -> Vec<(usize, String)> {
    let lag = word_lag(words, segments);
    let shifted: Vec<TimedText>;
    let words = if lag > 0.0 {
        debug!("Word timestamps run {:.2}s late; correcting", lag);
        shifted = words
            .iter()
            .map(|w| TimedText::new(w.start - lag, w.end - lag, w.text.clone()))
            .collect();
        &shifted[..]
    } else {
        words
    };
    let (text, spans) = text_and_spans(text, words);
    let slices = owned_slices(&text, &spans);
    let mut chunks: Vec<(usize, String)> = Vec::new();
    let mut prev = None;
    let mut located = false;
    for (word, slice) in words.iter().zip(slices) {
        if slice.is_empty() {
            continue;
        }
        located = true;
        let Some(speaker) = word_speaker(word, segments, prev) else {
            return vec![(0, text.trim().to_string())];
        };
        prev = Some(speaker);
        push_chunk(&mut chunks, speaker, slice);
    }
    if !located && !text.trim().is_empty() {
        // No located piece (all blank): keep the text with one speaker.
        let speaker = segments.first().map_or(0, |s| s.speaker);
        chunks.push((speaker, text.clone()));
    }
    finish(chunks)
}

/// How the speakers share one segment of the transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum SegmentPlan {
    /// The whole segment goes to this speaker.
    Keep(usize),
    /// Mixed speakers: transcribe these sample ranges separately.
    Split(Vec<turns::Turn>),
}

/// Decide what to do with one timed segment (see [`SEGMENT_MAJORITY`]).
pub fn plan_segment(
    seg: &TimedText,
    segments: &[Segment],
    num_samples: usize,
    prev: Option<usize>,
) -> Option<SegmentPlan> {
    let majority = speaker_for(seg.start, seg.end, segments, prev)?;
    let overlaps = overlap_by_speaker(seg.start, seg.end, segments);
    let total: f32 = overlaps.iter().map(|(_, o)| o).sum();
    let main = overlaps
        .iter()
        .find(|(s, _)| *s == majority)
        .map_or(0.0, |(_, o)| *o);
    if total <= 0.0 || main / total >= SEGMENT_MAJORITY || total - main < SEGMENT_MINORITY_MIN {
        return Some(SegmentPlan::Keep(majority));
    }

    // Speaker turns inside the segment, built the same way as for whole
    // recordings (short bits folded into neighbours, gaps shared out).
    let to_sample = |t: f32| ((t.max(0.0) * SAMPLE_RATE as f32) as usize).min(num_samples);
    let (from, to) = (to_sample(seg.start), to_sample(seg.end));
    if to <= from {
        return Some(SegmentPlan::Keep(majority));
    }
    let clipped: Vec<Segment> = segments
        .iter()
        .filter(|s| s.end > seg.start && s.start < seg.end)
        .map(|s| Segment {
            start: s.start.max(seg.start) - seg.start,
            end: s.end.min(seg.end) - seg.start,
            speaker: s.speaker,
        })
        .collect();
    let mut sub: Vec<turns::Turn> = turns::build_turns(&clipped, to - from)
        .into_iter()
        .map(|t| turns::Turn {
            start: t.start + from,
            end: t.end + from,
            speaker: t.speaker,
        })
        .collect();
    if turns::speaker_count(&sub) < 2 || sub.len() > MAX_SUB_TURNS {
        return Some(SegmentPlan::Keep(majority));
    }
    // The sub-turns replace the segment's whole text, so they must cover all
    // of its audio: turns only reach a limited way into silence, and words
    // the diarizer missed there would be lost. Stretch the outer turns to the
    // segment's edges and meet in the middle of every pause.
    for i in 1..sub.len() {
        let mid = (sub[i - 1].end + sub[i].start) / 2;
        sub[i - 1].end = mid;
        sub[i].start = mid;
    }
    if let Some(first) = sub.first_mut() {
        first.start = from;
    }
    if let Some(last) = sub.last_mut() {
        last.end = to;
    }
    Some(SegmentPlan::Split(sub))
}

/// Segment-level alignment (engines without word timestamps, e.g. Whisper).
/// Each segment goes to its main speaker; a segment that mixes voices (see
/// [`SEGMENT_MAJORITY`]) is transcribed again per speaker turn with
/// `retranscribe`, up to [`MAX_RETRANSCRIBED_SEGMENTS`] times per recording.
/// An error
/// from `retranscribe` aborts the alignment.
pub fn assign_segments(
    text: &str,
    pieces: &[TimedText],
    segments: &[Segment],
    samples: &[f32],
    retranscribe: &mut dyn FnMut(&[f32]) -> Result<String>,
) -> Result<Vec<(usize, String)>> {
    let (text, spans) = text_and_spans(text, pieces);
    let slices = owned_slices(&text, &spans);
    let mut chunks: Vec<(usize, String)> = Vec::new();
    let mut prev = None;
    let mut budget = MAX_RETRANSCRIBED_SEGMENTS;
    let mut located = false;
    for (piece, slice) in pieces.iter().zip(slices) {
        if slice.is_empty() {
            continue;
        }
        located = true;
        let Some(plan) = plan_segment(piece, segments, samples.len(), prev) else {
            return Ok(vec![(0, text.trim().to_string())]);
        };
        match plan {
            SegmentPlan::Keep(speaker) => {
                prev = Some(speaker);
                push_chunk(&mut chunks, speaker, slice);
            }
            SegmentPlan::Split(sub) if budget > 0 => {
                budget -= 1;
                let mut parts = Vec::with_capacity(sub.len());
                for turn in &sub {
                    parts.push((turn.speaker, retranscribe(&samples[turn.start..turn.end])?));
                }
                if parts.iter().all(|(_, t)| t.trim().is_empty()) {
                    // Nothing came back: keep the original text.
                    let speaker = speaker_for(piece.start, piece.end, segments, prev).unwrap_or(0);
                    prev = Some(speaker);
                    push_chunk(&mut chunks, speaker, slice);
                    continue;
                }
                debug!(
                    "Re-transcribed a mixed-speaker segment ({:.1}–{:.1}s) as {} turns",
                    piece.start,
                    piece.end,
                    parts.len()
                );
                for (speaker, part) in parts {
                    push_separate(&mut chunks, speaker, &part);
                    prev = Some(speaker);
                }
                // Keep the separation from whatever follows.
                if let Some((_, block)) = chunks.last_mut() {
                    block.push(' ');
                }
            }
            SegmentPlan::Split(_) => {
                let speaker = speaker_for(piece.start, piece.end, segments, prev).unwrap_or(0);
                prev = Some(speaker);
                push_chunk(&mut chunks, speaker, slice);
            }
        }
    }
    if !located && !text.trim().is_empty() {
        let speaker = segments.first().map_or(0, |s| s.speaker);
        chunks.push((speaker, text.clone()));
    }
    Ok(finish(chunks))
}

/// Number of distinct speakers among blocks.
pub fn block_speakers(blocks: &[(usize, String)]) -> usize {
    let mut seen: Vec<usize> = blocks.iter().map(|(s, _)| *s).collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f32, end: f32, speaker: usize) -> Segment {
        Segment {
            start,
            end,
            speaker,
        }
    }

    fn w(start: f32, end: f32, text: &str) -> TimedText {
        TimedText::new(start, end, text)
    }

    #[test]
    fn words_split_at_a_speaker_change() {
        let text = "Hello there, how are you? Fine, thanks.";
        let words = [
            w(0.0, 0.4, "Hello"),
            w(0.4, 0.8, "there,"),
            w(0.9, 1.1, "how"),
            w(1.1, 1.3, "are"),
            w(1.3, 1.6, "you?"),
            w(2.0, 2.4, "Fine,"),
            w(2.4, 2.9, "thanks."),
        ];
        let segments = [seg(0.0, 1.7, 3), seg(1.9, 3.0, 1)];
        assert_eq!(
            assign_words(text, &words, &segments),
            vec![
                (3, "Hello there, how are you?".to_string()),
                (1, "Fine, thanks.".to_string())
            ]
        );
    }

    #[test]
    fn word_straddling_a_change_goes_by_its_onset() {
        let text = "one two three";
        let segments = [seg(0.0, 1.0, 0), seg(1.0, 2.0, 1)];
        // The first 0.3 s of "two" (0.9–1.2) overlap B more than A.
        let words = [w(0.0, 0.5, "one"), w(0.9, 1.5, "two"), w(1.5, 2.0, "three")];
        assert_eq!(
            assign_words(text, &words, &segments),
            vec![(0, "one".to_string()), (1, "two three".to_string())]
        );
        // Starting well inside A, it stays with A even if its (unreliable)
        // end reaches far into B.
        let words = [w(0.0, 0.5, "one"), w(0.7, 1.6, "two"), w(1.6, 2.0, "three")];
        assert_eq!(
            assign_words(text, &words, &segments),
            vec![(0, "one two".to_string()), (1, "three".to_string())]
        );
    }

    #[test]
    fn uncovered_words_go_to_the_nearest_speaker() {
        let text = "Before. Inside A. Gap word. Inside B. After.";
        let words = [
            w(0.0, 0.5, "Before."),
            w(1.0, 1.5, "Inside"),
            w(1.5, 1.9, "A."),
            w(2.4, 2.6, "Gap"),
            w(2.6, 2.9, "word."),
            w(5.0, 5.5, "Inside"),
            w(5.5, 5.9, "B."),
            w(7.5, 7.9, "After."),
        ];
        // Diarization covered only 1–2 s (A) and 4.5–6 s (B).
        let segments = [seg(1.0, 2.0, 0), seg(4.5, 6.0, 1)];
        let blocks = assign_words(text, &words, &segments);
        assert_eq!(
            blocks,
            vec![
                (0, "Before. Inside A. Gap word.".to_string()),
                (1, "Inside B. After.".to_string())
            ]
        );
        // Nothing dropped.
        let joined: Vec<String> = blocks.into_iter().map(|(_, t)| t).collect();
        assert_eq!(joined.join(" "), text);
    }

    #[test]
    fn text_far_from_all_speech_is_kept() {
        // Words long after the last diarized speech still go to the nearest
        // speaker: no word is ever dropped.
        let text = "Hello there. Hi. Thank you.";
        let words = [
            w(0.2, 0.5, "Hello"),
            w(0.5, 0.9, "there."),
            w(1.6, 1.9, "Hi."),
            w(20.0, 20.3, "Thank"),
            w(20.3, 20.6, "you."),
        ];
        let segments = [seg(0.0, 1.0, 0), seg(1.5, 2.0, 1)];
        assert_eq!(
            assign_words(text, &words, &segments),
            vec![
                (0, "Hello there.".to_string()),
                (1, "Hi. Thank you.".to_string())
            ]
        );
        // Same for a Whisper segment.
        let pieces = [w(0.0, 2.0, " Hello there."), w(15.0, 20.0, " Thank you.")];
        let segments = [seg(0.0, 2.0, 0)];
        let blocks = assign_segments(
            " Hello there. Thank you.",
            &pieces,
            &segments,
            &vec![0.0; 21 * SAMPLE_RATE],
            &mut |_| unreachable!(),
        )
        .unwrap();
        assert_eq!(blocks, vec![(0, "Hello there. Thank you.".to_string())]);
    }

    #[test]
    fn late_punctuation_does_not_move_a_word_to_the_next_speaker() {
        // TDT models emit "?" well after the word: " now"@19.28–19.36 and
        // "?"@20.00–20.16. The speaker changes at 19.67.
        let tokens = [
            w(18.9, 19.2, " right"),
            w(19.28, 19.36, " now"),
            w(20.0, 20.16, "?"),
            w(20.3, 20.6, " Yes"),
            w(20.6, 20.7, "."),
        ];
        let words = group_tokens(&tokens);
        assert_eq!(words[1], w(19.28, 19.36, "now?"));
        let segments = [seg(18.0, 19.67, 0), seg(19.67, 21.0, 1)];
        assert_eq!(
            assign_words("right now? Yes.", &words, &segments),
            vec![(0, "right now?".to_string()), (1, "Yes.".to_string())]
        );
        // Library word rows with the punctuation folded into the end are
        // judged by their onset.
        let words = [
            w(18.9, 19.2, "right"),
            w(19.28, 20.16, "now?"),
            w(20.3, 20.7, "Yes."),
        ];
        assert_eq!(
            assign_words("right now? Yes.", &words, &segments),
            vec![(0, "right now?".to_string()), (1, "Yes.".to_string())]
        );
    }

    #[test]
    fn turn_final_word_ending_at_the_next_word_stays_with_its_speaker() {
        // transcribe-rs ends every token where the next one starts, so the
        // last word of a turn spans the pause: "now" 19.28–22.16.
        let words = [
            w(18.9, 19.28, "right"),
            w(19.28, 22.16, "now."),
            w(22.16, 22.5, "Thursday"),
            w(22.5, 23.0, "works."),
        ];
        // Speaker 2's diarized speech starts before their first word, so the
        // stretched "now." overlaps speaker 2 (0.66 s) more than 0 (0.32 s).
        let segments = [seg(18.5, 19.6, 0), seg(21.5, 23.2, 2)];
        assert_eq!(
            assign_words("right now. Thursday works.", &words, &segments),
            vec![
                (0, "right now.".to_string()),
                (2, "Thursday works.".to_string())
            ]
        );
    }

    #[test]
    fn punctuation_and_spacing_come_from_the_engine_text() {
        // Words without their punctuation (as some engines report them), an
        // opening quote that belongs to the next speaker, and a double space.
        let text = "Is it ready?  \"Yes,\" she said — almost.";
        let words = [
            w(0.0, 0.2, "Is"),
            w(0.2, 0.4, "it"),
            w(0.4, 0.8, "ready"),
            w(1.5, 1.8, "Yes"),
            w(1.8, 2.0, "she"),
            w(2.0, 2.2, "said"),
            w(2.4, 2.8, "almost"),
        ];
        let segments = [seg(0.0, 1.0, 0), seg(1.4, 3.0, 1)];
        assert_eq!(
            assign_words(text, &words, &segments),
            vec![
                (0, "Is it ready?".to_string()),
                (1, "\"Yes,\" she said — almost.".to_string())
            ]
        );
    }

    #[test]
    fn late_word_timestamps_are_corrected() {
        // Three utterances; the words are reported 0.6 s late, so without the
        // correction the last word of each would go to the next speaker.
        let segments = [seg(0.5, 2.0, 0), seg(2.5, 4.0, 1), seg(4.5, 6.0, 0)];
        let words = [
            w(1.1, 1.5, "One"),
            w(1.6, 2.0, "two"),
            w(2.2, 2.6, "three."),
            w(3.1, 3.5, "Four"),
            w(3.6, 4.0, "five"),
            w(4.2, 4.6, "six."),
            w(5.1, 5.5, "Seven"),
            w(5.6, 6.2, "eight."),
        ];
        assert!((word_lag(&words, &segments) - 0.5).abs() < 1e-4);
        assert_eq!(
            assign_words(
                "One two three. Four five six. Seven eight.",
                &words,
                &segments
            ),
            vec![
                (0, "One two three.".to_string()),
                (1, "Four five six.".to_string()),
                (0, "Seven eight.".to_string())
            ]
        );
        // On-time words (0.1 s after each onset) are left alone.
        let on_time: Vec<TimedText> = words
            .iter()
            .map(|x| TimedText::new(x.start - 0.5, x.end - 0.5, x.text.clone()))
            .collect();
        assert_eq!(word_lag(&on_time, &segments), 0.0);
    }

    #[test]
    fn mismatching_words_rebuild_the_text() {
        let text = "completely different text";
        let words = [w(0.0, 0.5, "hello"), w(1.5, 2.0, "world")];
        let segments = [seg(0.0, 1.0, 0), seg(1.0, 2.0, 1)];
        assert_eq!(
            assign_words(text, &words, &segments),
            vec![(0, "hello".to_string()), (1, "world".to_string())]
        );
    }

    #[test]
    fn unspaced_scripts_split_per_token() {
        let text = "你好。我很好。";
        let words = group_tokens(&[
            w(0.0, 0.3, "你"),
            w(0.3, 0.6, "好"),
            w(0.6, 0.7, "。"),
            w(1.5, 1.8, "我"),
            w(1.8, 2.0, "很"),
            w(2.0, 2.2, "好"),
            w(2.2, 2.3, "。"),
        ]);
        assert_eq!(words.len(), 5);
        let segments = [seg(0.0, 1.0, 0), seg(1.2, 2.5, 1)];
        assert_eq!(
            assign_words(text, &words, &segments),
            vec![(0, "你好。".to_string()), (1, "我很好。".to_string())]
        );
    }

    #[test]
    fn tokens_group_into_words() {
        let tokens = [
            w(0.0, 0.1, "\u{2581}Hel"),
            w(0.1, 0.2, "lo"),
            w(0.2, 0.25, ","),
            w(0.3, 0.5, " world"),
            w(0.5, 0.5, ""),
            w(0.5, 0.6, "!"),
        ];
        // Punctuation joins the word's text but not its time span.
        assert_eq!(
            group_tokens(&tokens),
            vec![w(0.0, 0.2, "Hello,"), w(0.3, 0.5, "world!")]
        );
    }

    #[test]
    fn zero_width_tokens_use_their_midpoint() {
        let segments = [seg(0.0, 1.0, 0), seg(1.0, 2.0, 1)];
        assert_eq!(speaker_for(1.5, 1.5, &segments, None), Some(1));
        assert_eq!(speaker_for(0.5, 0.5, &segments, Some(1)), Some(0));
        assert_eq!(speaker_for(0.5, 0.5, &[], None), None);
    }

    #[test]
    fn segment_majority_keeps_the_main_speaker() {
        let n = 20 * SAMPLE_RATE;
        // 4 s segment: A 3.4 s, B 0.6 s → 85% → keep A.
        let piece = w(0.0, 4.0, "x");
        let segments = [seg(0.0, 3.4, 0), seg(3.4, 4.0, 1)];
        assert_eq!(
            plan_segment(&piece, &segments, n, None),
            Some(SegmentPlan::Keep(0))
        );
        // 1.5 s segment, 50/50 but the minority is under 1 s → keep.
        let piece = w(0.0, 1.5, "x");
        let segments = [seg(0.0, 0.8, 0), seg(0.8, 1.5, 1)];
        assert!(matches!(
            plan_segment(&piece, &segments, n, None),
            Some(SegmentPlan::Keep(_))
        ));
        // 6 s segment, A 3.5 s / B 2.5 s → split into two turns.
        let piece = w(2.0, 8.0, "x");
        let segments = [seg(2.0, 5.5, 0), seg(5.5, 8.0, 1)];
        let Some(SegmentPlan::Split(sub)) = plan_segment(&piece, &segments, n, None) else {
            panic!("expected a split");
        };
        assert_eq!(sub.len(), 2);
        assert_eq!((sub[0].speaker, sub[1].speaker), (0, 1));
        assert_eq!(sub[0].start, 2 * SAMPLE_RATE);
        assert_eq!(sub[1].end, 8 * SAMPLE_RATE);
        assert_eq!(sub[0].end, sub[1].start);
    }

    #[test]
    fn split_segments_are_retranscribed_in_full() {
        // Whisper segment 0–10 s; the diarizer heard A only at 4–6 s and B at
        // 6–10 s. The words in 0–2 s (more than GAP_FILL before A) must still
        // be transcribed again, or they would vanish with the segment's text.
        let n = 12 * SAMPLE_RATE;
        let piece = w(0.0, 10.0, " Early words. Middle. Late.");
        let segments = [seg(4.0, 6.0, 0), seg(6.0, 10.0, 1)];
        let Some(SegmentPlan::Split(sub)) = plan_segment(&piece, &segments, n, None) else {
            panic!("expected a split");
        };
        assert_eq!(sub.first().unwrap().start, 0);
        assert_eq!(sub.last().unwrap().end, 10 * SAMPLE_RATE);
        assert!(sub.windows(2).all(|p| p[0].end == p[1].start));

        // A long pause inside the segment is covered too.
        let piece = w(0.0, 12.0, "x");
        let segments = [seg(0.0, 2.0, 0), seg(9.0, 12.0, 1)];
        let Some(SegmentPlan::Split(sub)) = plan_segment(&piece, &segments, n, None) else {
            panic!("expected a split");
        };
        assert_eq!(sub.len(), 2);
        assert_eq!(sub[0].start, 0);
        assert_eq!(sub[0].end, sub[1].start);
        assert_eq!(sub[0].end, (5.5 * SAMPLE_RATE as f32) as usize);
        assert_eq!(sub[1].end, 12 * SAMPLE_RATE);

        // Through assign_segments: the re-transcribed ranges tile the segment.
        let samples = vec![0.0f32; n];
        let pieces = [w(0.0, 10.0, " Early words. Middle. Late.")];
        let segments = [seg(4.0, 6.0, 0), seg(6.0, 10.0, 1)];
        let base = samples.as_ptr() as usize;
        let mut ranges = Vec::new();
        assign_segments(
            " Early words. Middle. Late.",
            &pieces,
            &segments,
            &samples,
            &mut |audio| {
                let start = (audio.as_ptr() as usize - base) / std::mem::size_of::<f32>();
                ranges.push((start, start + audio.len()));
                Ok("t".to_string())
            },
        )
        .unwrap();
        assert_eq!(ranges.first().unwrap().0, 0);
        assert_eq!(ranges.last().unwrap().1, 10 * SAMPLE_RATE);
        assert!(ranges.windows(2).all(|p| p[0].1 == p[1].0));
    }

    #[test]
    fn segments_are_assigned_and_mixed_ones_retranscribed() {
        let samples = vec![0.0f32; 20 * SAMPLE_RATE];
        let text = " Good morning everyone. Hi! Hello, thanks. Let's start.";
        let pieces = [
            w(0.0, 3.0, " Good morning everyone."),
            // Mixed: B 3–5.5 s, A 5.5–8 s.
            w(3.0, 8.0, " Hi! Hello, thanks."),
            w(8.0, 10.0, " Let's start."),
        ];
        let segments = [seg(0.0, 3.0, 0), seg(3.0, 5.5, 1), seg(5.5, 10.0, 0)];
        let mut calls = Vec::new();
        let blocks = assign_segments(text, &pieces, &segments, &samples, &mut |audio| {
            calls.push(audio.len());
            Ok(if calls.len() == 1 {
                " Hi!"
            } else {
                "Hello, thanks."
            }
            .to_string())
        })
        .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            blocks,
            vec![
                (0, "Good morning everyone.".to_string()),
                (1, "Hi!".to_string()),
                (0, "Hello, thanks. Let's start.".to_string())
            ]
        );
    }

    #[test]
    fn retranscription_is_bounded() {
        let n = 400 * SAMPLE_RATE;
        let samples = vec![0.0f32; n];
        // 30 mixed segments, 10 s each, 5 s per speaker.
        let mut pieces = Vec::new();
        let mut segments = Vec::new();
        let mut text = String::new();
        for i in 0..30 {
            let t = i as f32 * 10.0;
            pieces.push(w(t, t + 10.0, &format!(" s{i}.")));
            text.push_str(&format!(" s{i}."));
            segments.push(seg(t, t + 5.0, 0));
            segments.push(seg(t + 5.0, t + 10.0, 1));
        }
        let mut calls = 0;
        let blocks = assign_segments(&text, &pieces, &segments, &samples, &mut |_| {
            calls += 1;
            Ok("r".to_string())
        })
        .unwrap();
        assert_eq!(calls, 2 * MAX_RETRANSCRIBED_SEGMENTS);
        // The segments over budget are kept whole.
        assert!(blocks.iter().any(|(_, t)| t.contains("s29.")));
    }

    #[test]
    fn retranscription_errors_abort() {
        let samples = vec![0.0f32; 10 * SAMPLE_RATE];
        let pieces = [w(0.0, 6.0, "x")];
        let segments = [seg(0.0, 3.0, 0), seg(3.0, 6.0, 1)];
        let result = assign_segments("x", &pieces, &segments, &samples, &mut |_| {
            Err(anyhow::anyhow!("engine failed"))
        });
        assert!(result.is_err());
    }
}
