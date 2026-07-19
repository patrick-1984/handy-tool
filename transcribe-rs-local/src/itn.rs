//! Inverse Text Normalization (ITN) post-processing.
//!
//! Converts spoken-form text (e.g. "twenty three dollars") into written-form
//! (e.g. "$23") using rules from the `nemo-text-processing` crate.

use crate::TranscriptionResult;
use nemo_text_processing::normalize_sentence;

/// Apply inverse text normalization to a transcription result.
///
/// Normalizes `result.text` and, if present, each segment's text.
///
/// # Example
///
/// ```ignore
/// use transcribe_rs::itn::apply_itn;
///
/// let mut result = engine.transcribe_file(&path, None)?;
/// apply_itn(&mut result);
/// println!("{}", result.text); // written-form output
/// ```
pub fn apply_itn(result: &mut TranscriptionResult) {
    result.text = normalize_sentence(&result.text);
    if let Some(segments) = &mut result.segments {
        for segment in segments.iter_mut() {
            segment.text = normalize_sentence(&segment.text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TranscriptionSegment;

    #[test]
    fn test_apply_itn_normalizes_text_and_segments() {
        let mut result = TranscriptionResult {
            text: "twenty three dollars".to_string(),
            segments: Some(vec![
                TranscriptionSegment {
                    start: 0.0,
                    end: 1.0,
                    text: "twenty three dollars".to_string(),
                },
                TranscriptionSegment {
                    start: 1.0,
                    end: 2.0,
                    text: "one hundred fifty two".to_string(),
                },
            ]),
        };

        apply_itn(&mut result);

        assert_eq!(result.text, "$23");
        let segments = result.segments.unwrap();
        assert_eq!(segments[0].text, "$23");
        assert_eq!(segments[1].text, "152");
    }

    #[test]
    fn test_apply_itn_no_segments() {
        let mut result = TranscriptionResult {
            text: "twenty three dollars".to_string(),
            segments: None,
        };

        apply_itn(&mut result);

        assert_eq!(result.text, "$23");
        assert!(result.segments.is_none());
    }
}
