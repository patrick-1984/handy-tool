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
    /// Vulkan device index to run on when `use_gpu` is true (T-212). `0` is
    /// whisper.cpp's own default (discrete GPUs are enumerated before
    /// integrated ones — see `whisper_rs::vulkan::list_devices`).
    pub gpu_device: i32,
}

impl Default for WhisperModelParams {
    fn default() -> Self {
        Self {
            use_gpu: true,
            gpu_device: 0,
        }
    }
}

/// A single GPU adapter as reported by ggml's Vulkan backend, trimmed to
/// what callers need to build a selection UI (T-212). Kept independent of
/// `whisper_rs::vulkan::VkDeviceInfo` so this crate's public API doesn't leak
/// whisper-rs-sys buffer-type internals across the crate boundary.
#[derive(Debug, Clone)]
pub struct GpuDeviceInfo {
    pub index: i32,
    pub name: String,
    pub vram_total_mb: u64,
    pub vram_free_mb: u64,
}

/// Enumerate the GPU adapters whisper.cpp's Vulkan backend can see.
///
/// Lazy by construction (only queries when called, never at load/import
/// time) and never touches a loaded model — safe to call from a settings UI
/// before any transcription has happened. Returns an empty list on macOS
/// (whisper-rs there is built with the Metal backend, which has no Vulkan
/// device registry) or if the Vulkan backend reports zero devices.
#[cfg(not(target_os = "macos"))]
pub fn list_gpu_devices() -> Vec<GpuDeviceInfo> {
    whisper_rs::vulkan::list_devices()
        .into_iter()
        .map(|d| GpuDeviceInfo {
            index: d.id,
            name: d.name,
            vram_total_mb: (d.vram.total / (1024 * 1024)) as u64,
            vram_free_mb: (d.vram.free / (1024 * 1024)) as u64,
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub fn list_gpu_devices() -> Vec<GpuDeviceInfo> {
    Vec::new()
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
        context_params.gpu_device = params.gpu_device;
        let context =
            WhisperContext::new_with_params(model_path.to_str().unwrap(), context_params)?;

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
