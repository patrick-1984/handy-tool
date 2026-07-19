//! Whisper speech recognition engine implementation.
//!
//! This module provides a Whisper-based transcription engine that uses
//! OpenAI's Whisper model for speech-to-text conversion. Whisper models
//! are provided as single GGML format files.

use crate::{TranscriptionEngine, TranscriptionResult, TranscriptionSegment};
use std::path::{Path, PathBuf};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Parameters for configuring Whisper model loading.
#[derive(Debug, Clone)]
pub struct WhisperModelParams {
    pub use_gpu: bool,
}

impl Default for WhisperModelParams {
    fn default() -> Self {
        Self { use_gpu: true }
    }
}

/// Parameters for configuring Whisper inference behavior.
#[derive(Debug, Clone)]
pub struct WhisperInferenceParams {
    /// Target language for transcription (e.g., "en", "es", "fr").
    /// If None, Whisper will auto-detect the language.
    pub language: Option<String>,

    /// Whether to translate the transcription to English.
    pub translate: bool,

    /// Whether to print special tokens in the output
    pub print_special: bool,

    /// Whether to print progress information during transcription
    pub print_progress: bool,

    /// Whether to print results in real-time as they're generated
    pub print_realtime: bool,

    /// Whether to include timestamp information in the output
    pub print_timestamps: bool,

    /// Whether to suppress blank/empty segments in the output
    pub suppress_blank: bool,

    /// Whether to suppress non-speech tokens
    pub suppress_non_speech_tokens: bool,

    /// Threshold for detecting silence/no-speech segments (0.0-1.0).
    pub no_speech_thold: f32,

    /// Initial prompt to provide context to the model.
    pub initial_prompt: Option<String>,
}

impl Default for WhisperInferenceParams {
    fn default() -> Self {
        Self {
            language: None,
            translate: false,
            print_special: false,
            print_progress: false,
            print_realtime: false,
            print_timestamps: false,
            suppress_blank: true,
            suppress_non_speech_tokens: true,
            no_speech_thold: 0.2,
            initial_prompt: None,
        }
    }
}

/// Whisper speech recognition engine.
pub struct WhisperEngine {
    loaded_model_path: Option<PathBuf>,
    state: Option<whisper_rs::WhisperState>,
    context: Option<whisper_rs::WhisperContext>,
}

impl Default for WhisperEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            loaded_model_path: None,
            state: None,
            context: None,
        }
    }
}

impl Drop for WhisperEngine {
    fn drop(&mut self) {
        self.unload_model();
    }
}

impl TranscriptionEngine for WhisperEngine {
    type InferenceParams = WhisperInferenceParams;
    type ModelParams = WhisperModelParams;

    fn load_model_with_params(
        &mut self,
        model_path: &Path,
        params: Self::ModelParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut context_params = WhisperContextParameters::default();
        context_params.use_gpu = params.use_gpu;
        let context = WhisperContext::new_with_params(
            model_path.to_str().unwrap(),
            context_params,
        )?;

        let state = context.create_state()?;

        self.context = Some(context);
        self.state = Some(state);

        self.loaded_model_path = Some(model_path.to_path_buf());
        Ok(())
    }

    fn unload_model(&mut self) {
        self.loaded_model_path = None;
        self.state = None;
        self.context = None;
    }

    fn transcribe_samples(
        &mut self,
        samples: Vec<f32>,
        params: Option<Self::InferenceParams>,
    ) -> Result<TranscriptionResult, Box<dyn std::error::Error>> {
        let state = self
            .state
            .as_mut()
            .ok_or("Model not loaded. Call load_model() first.")?;

        let whisper_params = params.unwrap_or_default();

        let mut full_params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 3,
            patience: -1.0,
        });
        full_params.set_language(whisper_params.language.as_deref());
        full_params.set_translate(whisper_params.translate);
        full_params.set_print_special(whisper_params.print_special);
        full_params.set_print_progress(whisper_params.print_progress);
        full_params.set_print_realtime(whisper_params.print_realtime);
        full_params.set_print_timestamps(whisper_params.print_timestamps);
        full_params.set_suppress_blank(whisper_params.suppress_blank);
        full_params.set_suppress_nst(whisper_params.suppress_non_speech_tokens);
        full_params.set_no_speech_thold(whisper_params.no_speech_thold);

        if let Some(ref prompt) = whisper_params.initial_prompt {
            full_params.set_initial_prompt(prompt);
        }

        state.full(full_params, &samples)?;

        let num_segments = state.full_n_segments();

        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                let text = segment.to_str_lossy()
                    .map_err(|e| format!("Failed to get segment text: {}", e))?
                    .to_string();
                let start = segment.start_timestamp() as f32 / 100.0;
                let end = segment.end_timestamp() as f32 / 100.0;

                segments.push(TranscriptionSegment {
                    start,
                    end,
                    text: text.clone(),
                });
                full_text.push_str(&text);
            }
        }

        Ok(TranscriptionResult {
            text: full_text.trim().to_string(),
            segments: Some(segments),
        })
    }
}
