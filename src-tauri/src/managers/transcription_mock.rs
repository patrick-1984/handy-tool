// CI-only mock TranscriptionManager - avoids whisper/Vulkan dependencies.
// This file is copied over transcription.rs during CI tests.
// Existing tests don't exercise transcription, so this is safe.

use crate::managers::model::ModelManager;
use anyhow::Result;
use serde::Serialize;
use std::sync::Arc;
use tauri::AppHandle;

/// Mirrors the real module's Vulkan-serialization helper so callers
/// (`commands/models.rs::list_gpu_devices`) still resolve under the CI mock.
/// No real Vulkan work happens in CI, so this just runs the closure.
pub fn with_vulkan_op_lock<R>(f: impl FnOnce() -> R) -> R {
    f()
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct TranscriptionManager {
    #[allow(dead_code)]
    app_handle: AppHandle,
}

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, _model_manager: Arc<ModelManager>) -> Result<Self> {
        Ok(Self {
            app_handle: app_handle.clone(),
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        false
    }

    pub fn unload_model(&self) -> Result<()> {
        Ok(())
    }

    pub fn maybe_unload_immediately(&self, _context: &str) {}

    pub fn load_model(&self, _model_id: &str) -> Result<()> {
        Ok(())
    }

    pub fn next_external_select_gen(&self) -> u64 {
        0
    }

    pub fn reload_external_model_if_latest(&self, _model_id: &str, _select_gen: u64) -> Result<()> {
        Ok(())
    }

    pub fn initiate_model_load(&self) {}

    pub fn get_current_model(&self) -> Option<String> {
        None
    }

    pub fn transcribe(&self, _audio: Vec<f32>) -> Result<String> {
        Ok(String::new())
    }

    pub fn transcribe_expecting(&self, _expected_model: &str, _audio: Vec<f32>) -> Result<String> {
        Ok(String::new())
    }

    pub fn set_live_transcribing(&self, _active: bool) {}

    pub fn set_batch_transcribing(&self, _active: bool) {}
}
