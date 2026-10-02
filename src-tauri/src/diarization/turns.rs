//! From diarization segments to speaker turns (for the per-turn fallback and
//! for re-transcribing mixed Whisper segments), and from labelled text to the
//! final transcript.

use super::pipeline::{SAMPLE_RATE, Segment};

/// Same-speaker segments closer than this (seconds) become one turn.
const MERGE_GAP: f32 = 1.0;
/// Turns shorter than this (seconds) are folded into a neighbour: too short to
/// transcribe reliably, and usually a backchannel or a mis-assigned word.
const MIN_TURN: f32 = 1.0;
/// How far (seconds) a turn reaches into the silence on each side: up to the
/// middle of the gap to the neighbouring turn, but no further than this. Gaps
/// up to twice this long are covered completely, so words the segmentation
/// missed or clipped are still transcribed (the per-turn fallback only
/// transcribes turn audio), without handing the engine long stretches of
/// silence.
pub(super) const GAP_FILL: f32 = 2.0;

/// A stretch of audio attributed to one speaker, in samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    pub start: usize,
    pub end: usize,
    pub speaker: usize,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    start: f32,
    end: f32,
    speaker: usize,
}

fn merge_same_speaker(spans: &mut Vec<Span>) {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        match out.last_mut() {
            Some(last) if last.speaker == span.speaker && span.start - last.end <= MERGE_GAP => {
                last.end = last.end.max(span.end);
            }
            _ => out.push(span),
        }
    }
    *spans = out;
}

/// Build non-overlapping speaker turns covering the diarized speech and the
/// pauses around it (see [`GAP_FILL`]).
pub fn build_turns(segments: &[Segment], num_samples: usize) -> Vec<Turn> {
    let mut sorted: Vec<Segment> = segments.to_vec();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));

    // Sequential pass: merge same-speaker neighbours, and resolve overlapping
    // speech by letting the earlier turn keep the shared audio (a segment
    // entirely inside another speaker's turn is absorbed by it).
    let mut spans: Vec<Span> = Vec::new();
    for seg in sorted {
        let mut span = Span {
            start: seg.start,
            end: seg.end,
            speaker: seg.speaker,
        };
        if let Some(last) = spans.last_mut() {
            if last.speaker == span.speaker && span.start - last.end <= MERGE_GAP {
                last.end = last.end.max(span.end);
                continue;
            }
            if span.start < last.end {
                if span.end <= last.end {
                    continue;
                }
                span.start = last.end;
            }
        }
        spans.push(span);
    }

    // Fold short turns into the nearer neighbour, shortest first, until every
    // turn is long enough (or only one is left).
    while spans.len() > 1 {
        let Some((i, _)) = spans
            .iter()
            .enumerate()
            .filter(|(_, s)| s.end - s.start < MIN_TURN)
            .min_by(|(_, a), (_, b)| (a.end - a.start).total_cmp(&(b.end - b.start)))
        else {
            break;
        };
        let gap_prev = (i > 0).then(|| spans[i].start - spans[i - 1].end);
        let gap_next = (i + 1 < spans.len()).then(|| spans[i + 1].start - spans[i].end);
        let short = spans.remove(i);
        match (gap_prev, gap_next) {
            (Some(p), Some(n)) if n < p => spans[i].start = short.start,
            (Some(_), _) => spans[i - 1].end = spans[i - 1].end.max(short.end),
            (None, _) => spans[i].start = short.start,
        }
        merge_same_speaker(&mut spans);
    }

    let to_sample = |t: f32| ((t.max(0.0) * SAMPLE_RATE as f32) as usize).min(num_samples);
    (0..spans.len())
        .map(|i| {
            // Extend into silence only, at most up to the middle of the gap
            // to the neighbouring turn, so turns never share audio.
            let lo = if i > 0 {
                (spans[i - 1].end + spans[i].start) / 2.0
            } else {
                0.0
            };
            let hi = if i + 1 < spans.len() {
                (spans[i].end + spans[i + 1].start) / 2.0
            } else {
                f32::INFINITY
            };
            let start = (spans[i].start - GAP_FILL).max(lo.min(spans[i].start));
            let end = (spans[i].end + GAP_FILL).min(hi.max(spans[i].end));
            Turn {
                start: to_sample(start),
                end: to_sample(end),
                speaker: spans[i].speaker,
            }
        })
        .filter(|t| t.end > t.start)
        .collect()
}

/// Stretch `turns` so together they cover all `num_samples` of the audio:
/// the first starts at 0, the last ends at the end, and neighbours meet in
/// the middle of every pause. The per-turn fallback transcribes only turn
/// audio, so without this words the diarizer missed in a long pause, or
/// before the first or after the last turn, would be lost.
pub fn cover_audio(mut turns: Vec<Turn>, num_samples: usize) -> Vec<Turn> {
    for i in 1..turns.len() {
        let mid = (turns[i - 1].end + turns[i].start) / 2;
        turns[i - 1].end = mid;
        turns[i].start = mid;
    }
    if let Some(first) = turns.first_mut() {
        first.start = 0;
    }
    if let Some(last) = turns.last_mut() {
        last.end = num_samples;
    }
    turns.retain(|t| t.end > t.start);
    turns
}

/// Turns longer than this (seconds) are split for per-turn transcription:
/// engines without long-form support get their audio in pieces they handle
/// well, and Whisper keeps to about one 30 s window per call.
pub const MAX_TURN: f32 = 30.0;
/// A long turn is cut in the quietest moment between this many seconds after
/// its start and [`MAX_TURN`].
const SPLIT_SEARCH_FROM: f32 = 15.0;
/// Length of the quiet stretch looked for, and the hop between candidates.
const SPLIT_WINDOW: usize = SAMPLE_RATE * 3 / 10;
const SPLIT_HOP: usize = SAMPLE_RATE / 10;

/// Split turns longer than [`MAX_TURN`] at the quietest point (a pause,
/// usually) found in the last part of every 30 s stretch.
pub fn split_long_turns(turns: Vec<Turn>, samples: &[f32]) -> Vec<Turn> {
    let max = (MAX_TURN * SAMPLE_RATE as f32) as usize;
    let search_from = (SPLIT_SEARCH_FROM * SAMPLE_RATE as f32) as usize;
    let mut out = Vec::with_capacity(turns.len());
    for turn in turns {
        let mut start = turn.start;
        let end = turn.end.min(samples.len());
        while end - start > max {
            let mut best = (f32::INFINITY, start + max);
            let mut at = start + search_from;
            while at + SPLIT_WINDOW <= start + max {
                let energy: f32 = samples[at..at + SPLIT_WINDOW].iter().map(|v| v * v).sum();
                // Ties go to the later point: fewer, longer pieces.
                if energy <= best.0 {
                    best = (energy, at + SPLIT_WINDOW / 2);
                }
                at += SPLIT_HOP;
            }
            out.push(Turn {
                start,
                end: best.1,
                speaker: turn.speaker,
            });
            start = best.1;
        }
        out.push(Turn {
            start,
            end: end.max(start),
            speaker: turn.speaker,
        });
    }
    out.retain(|t| t.end > t.start);
    out
}

/// Number of distinct speakers across `turns`.
pub fn speaker_count(turns: &[Turn]) -> usize {
    let mut seen: Vec<usize> = turns.iter().map(|t| t.speaker).collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

/// Join per-turn transcripts as `[Person N]: text` paragraphs, numbering
/// speakers by first appearance. Blank turns are skipped and consecutive
/// turns of the same speaker are joined. If only one speaker said anything,
/// the plain text is returned without a label.
pub fn format_transcript(turns: &[(usize, String)]) -> String {
    let mut paragraphs: Vec<(usize, String)> = Vec::new();
    for (speaker, text) in turns {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        match paragraphs.last_mut() {
            Some((last, para)) if last == speaker => {
                para.push(' ');
                para.push_str(text);
            }
            _ => paragraphs.push((*speaker, text.to_string())),
        }
    }

    let mut order: Vec<usize> = Vec::new();
    for (speaker, _) in &paragraphs {
        if !order.contains(speaker) {
            order.push(*speaker);
        }
    }
    if order.len() < 2 {
        return paragraphs
            .into_iter()
            .map(|(_, p)| p)
            .collect::<Vec<_>>()
            .join(" ");
    }

    paragraphs
        .iter()
        .map(|(speaker, text)| {
            let n = order.iter().position(|s| s == speaker).unwrap_or(0) + 1;
            format!("[Person {}]: {}", n, text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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

    fn secs(turns: &[Turn]) -> Vec<(f32, f32, usize)> {
        turns
            .iter()
            .map(|t| {
                (
                    t.start as f32 / SAMPLE_RATE as f32,
                    t.end as f32 / SAMPLE_RATE as f32,
                    t.speaker,
                )
            })
            .collect()
    }

    fn approx(a: &[(f32, f32, usize)], b: &[(f32, f32, usize)]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| (x.0 - y.0).abs() < 1e-3 && (x.1 - y.1).abs() < 1e-3 && x.2 == y.2)
    }

    const N: usize = 60 * SAMPLE_RATE;

    #[test]
    fn merges_same_speaker_within_a_second_and_fills_gaps() {
        let turns = build_turns(&[seg(1.0, 4.0, 0), seg(4.8, 7.0, 0), seg(9.0, 12.0, 1)], N);
        let got = secs(&turns);
        assert!(approx(&got, &[(0.0, 8.0, 0), (8.0, 14.0, 1)]), "{got:?}");
    }

    #[test]
    fn keeps_same_speaker_apart_over_a_long_gap() {
        let turns = build_turns(&[seg(1.0, 4.0, 0), seg(6.0, 9.0, 0)], N);
        assert_eq!(turns.len(), 2);
    }

    #[test]
    fn absorbs_short_turns_into_nearest_neighbour() {
        // A short B blip close to the following A turn joins it, and the two
        // A turns then merge into one.
        let turns = build_turns(&[seg(0.0, 5.0, 0), seg(5.9, 6.4, 1), seg(6.5, 10.0, 0)], N);
        let got = secs(&turns);
        assert!(approx(&got, &[(0.0, 12.0, 0)]), "{got:?}");
    }

    #[test]
    fn overlap_goes_to_the_earlier_speaker() {
        let turns = build_turns(
            &[
                seg(0.0, 5.0, 0),
                seg(4.0, 9.0, 1),  // starts inside A → clipped to 5.0
                seg(6.0, 7.0, 0),  // inside B → absorbed
                seg(9.5, 12.0, 0), // back to A
            ],
            N,
        );
        let got = secs(&turns);
        assert!(
            approx(&got, &[(0.0, 5.0, 0), (5.0, 9.25, 1), (9.25, 14.0, 0)]),
            "{got:?}"
        );
    }

    #[test]
    fn gap_fill_never_overlaps_the_neighbour() {
        // 0.2 s gap: each side gets half of it.
        let turns = build_turns(&[seg(0.0, 5.0, 0), seg(5.2, 9.0, 1)], N);
        let got = secs(&turns);
        assert!(approx(&got, &[(0.0, 5.1, 0), (5.1, 11.0, 1)]), "{got:?}");
        assert!(turns[0].end <= turns[1].start);
    }

    #[test]
    fn gap_fill_is_clamped_to_the_audio() {
        let turns = build_turns(&[seg(0.05, 3.0, 0), seg(4.0, 5.95, 1)], 6 * SAMPLE_RATE);
        let got = secs(&turns);
        assert!(approx(&got, &[(0.0, 3.5, 0), (3.5, 6.0, 1)]), "{got:?}");
    }

    #[test]
    fn gap_fill_is_capped_in_long_pauses() {
        let turns = build_turns(&[seg(1.0, 3.0, 0), seg(10.0, 12.0, 1)], N);
        let got = secs(&turns);
        assert!(approx(&got, &[(0.0, 5.0, 0), (8.0, 14.0, 1)]), "{got:?}");
    }

    #[test]
    fn short_reply_between_turns_is_still_transcribed() {
        // A, 0.6 s pause, B says "No." (0.25 s), 0.6 s pause, A again.
        let covers = |turns: &[Turn], from: f32, to: f32| {
            let (from, to) = ((from * 16_000.0) as usize, (to * 16_000.0) as usize);
            (from..to).all(|i| turns.iter().any(|t| t.start <= i && i < t.end))
        };
        // The reply as its own (short) segment: folded into a neighbour.
        let turns = build_turns(
            &[seg(0.5, 5.0, 0), seg(5.6, 5.85, 1), seg(6.45, 10.0, 0)],
            N,
        );
        assert!(covers(&turns, 5.6, 5.85), "{:?}", secs(&turns));
        // The reply missed by segmentation: the pause is covered anyway.
        let turns = build_turns(&[seg(0.5, 5.0, 0), seg(6.45, 10.0, 0)], N);
        assert_eq!(turns.len(), 2);
        assert!(covers(&turns, 5.0, 6.45), "{:?}", secs(&turns));
    }

    #[test]
    fn single_short_turn_survives() {
        let turns = build_turns(&[seg(1.0, 1.5, 0)], N);
        assert_eq!(turns.len(), 1);
        assert_eq!(speaker_count(&turns), 1);
    }

    #[test]
    fn covered_turns_span_the_whole_audio() {
        let sr = SAMPLE_RATE;
        let turns = build_turns(&[seg(5.0, 8.0, 0), seg(20.0, 24.0, 1)], 40 * sr);
        // Turns reach only GAP_FILL into the long pauses...
        assert!(turns[0].start > 0 && turns[1].end < 40 * sr);
        let covered = cover_audio(turns, 40 * sr);
        // ...covered, they meet in the middle and reach both ends.
        assert_eq!(covered[0].start, 0);
        assert_eq!(covered[0].end, covered[1].start);
        assert_eq!(covered[1].end, 40 * sr);
        assert!(cover_audio(Vec::new(), 10).is_empty());
    }

    #[test]
    fn long_turns_split_at_pauses() {
        // 70 s of "speech" with pauses at 22 s and 50 s.
        let mut samples = vec![0.5f32; 70 * SAMPLE_RATE];
        for pause in [22.0f32, 50.0] {
            let from = (pause * SAMPLE_RATE as f32) as usize;
            samples[from..from + SAMPLE_RATE / 2].fill(0.0);
        }
        let turns = split_long_turns(
            vec![
                Turn {
                    start: 0,
                    end: 70 * SAMPLE_RATE,
                    speaker: 2,
                },
                Turn {
                    start: 70 * SAMPLE_RATE,
                    end: 70 * SAMPLE_RATE,
                    speaker: 1,
                },
            ],
            &samples,
        );
        let got = secs(&turns);
        assert_eq!(got.len(), 3, "{got:?}");
        assert!((got[0].1 - 22.25).abs() < 0.2, "{got:?}");
        assert!((got[1].1 - 50.25).abs() < 0.2, "{got:?}");
        assert!(got.iter().all(|t| t.2 == 2 && t.1 - t.0 <= MAX_TURN));
        assert_eq!(turns[0].end, turns[1].start);
        assert_eq!(turns[2].end, 70 * SAMPLE_RATE);
    }

    #[test]
    fn long_turn_without_pause_is_cut_at_the_limit() {
        let samples = vec![0.5f32; 65 * SAMPLE_RATE];
        let turns = split_long_turns(
            vec![Turn {
                start: 0,
                end: 65 * SAMPLE_RATE,
                speaker: 0,
            }],
            &samples,
        );
        assert_eq!(turns.len(), 3);
        assert!(turns.iter().all(|t| t.end - t.start <= 30 * SAMPLE_RATE));
    }

    #[test]
    fn formats_labels_in_order_of_first_appearance() {
        let text = format_transcript(&[
            (3, "Hello there.".into()),
            (1, "Hi!".into()),
            (3, "  ".into()),
            (3, "How are you?".into()),
            (1, "Fine.".into()),
            (1, "Thanks.".into()),
        ]);
        assert_eq!(
            text,
            "[Person 1]: Hello there.\n\n[Person 2]: Hi!\n\n[Person 1]: How are you?\n\n[Person 2]: Fine. Thanks."
        );
    }

    #[test]
    fn one_speaker_with_text_gets_no_label() {
        let text = format_transcript(&[
            (0, "First part.".into()),
            (1, "".into()),
            (0, "Second part.".into()),
        ]);
        assert_eq!(text, "First part. Second part.");
        assert_eq!(format_transcript(&[]), "");
    }
}
