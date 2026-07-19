use crate::managers::model::{ModelInfo, ModelManager};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, write_settings};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// One Vulkan GPU adapter offered to the frontend's device-selection dropdown
/// (T-212). Trimmed down from `transcribe_rs::engines::whisper::GpuDeviceInfo`
/// — the UI only needs an id to persist and a label to show.
#[derive(Serialize, Clone, Debug, Type)]
pub struct GpuDeviceOption {
    pub index: i32,
    pub name: String,
    pub vram_total_mb: u64,
}

/// List the GPU adapters whisper.cpp's Vulkan backend can see, for the
/// transcription GPU-device selector. Enumeration is lazy (queries the
/// Vulkan backend on demand) and safe to call at any time — it never touches
/// a loaded model. Returns an empty list on macOS (Metal backend, no Vulkan
/// device registry) or if no adapters are found.
///
/// Adversarial review finding 4 (T-212 follow-up): runs under
/// `crate::managers::transcription::with_vulkan_op_lock` so this enumeration
/// can never overlap the GPU Whisper model-load path in
/// `managers/transcription.rs` — see that lock's doc comment for why
/// enumeration racing a load is unsafe.
#[tauri::command]
#[specta::specta]
pub async fn list_gpu_devices() -> Result<Vec<GpuDeviceOption>, String> {
    Ok(crate::managers::transcription::with_vulkan_op_lock(|| {
        transcribe_rs::engines::whisper::list_gpu_devices()
            .into_iter()
            .map(|d| GpuDeviceOption {
                index: d.index,
                name: d.name,
                vram_total_mb: d.vram_total_mb,
            })
            .collect()
    }))
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(model_manager.get_available_models())
}

#[tauri::command]
#[specta::specta]
pub async fn get_model_info(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<Option<ModelInfo>, String> {
    Ok(model_manager.get_model_info(&model_id))
}

#[tauri::command]
#[specta::specta]
pub async fn download_model(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .download_model(&model_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    // If deleting the active model, unload it and clear the setting
    let settings = get_settings(&app_handle);
    if settings.selected_model == model_id {
        transcription_manager
            .unload_model()
            .map_err(|e| format!("Failed to unload model: {}", e))?;

        let mut settings = get_settings(&app_handle);
        settings.selected_model = String::new();
        write_settings(&app_handle, settings);
    }

    model_manager
        .delete_model(&model_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_active_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    // Check if model exists and is available
    let model_info = model_manager
        .get_model_info(&model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    if !model_info.is_downloaded {
        return Err(format!("Model not downloaded: {}", model_id));
    }

    // Clicking the already-selected, already-loaded model must be a no-op —
    // reloading it churns CPU/GPU for seconds and drops the warm engine.
    // External engines are exempt: their liveness isn't captured by
    // current_model_id (a dead FLM subprocess still reports loaded), and
    // reselecting is the user's natural "restart it" gesture.
    if !model_info.engine_type.is_external() {
        let settings = get_settings(&app_handle);
        if settings.selected_model == model_id
            && transcription_manager.get_current_model().as_deref() == Some(model_id.as_str())
        {
            log::debug!(
                "Model '{}' is already active and loaded; skipping",
                model_id
            );
            return Ok(());
        }
    }

    // External/API engines are configured separately (Advanced > Transcription)
    // and validate/load lazily at transcription time. Selecting one must persist
    // even before it's configured — otherwise it's a select-then-configure
    // deadlock — so we save the selection first and treat an eager-load failure
    // (usually "not configured yet") as a non-fatal, event-surfaced warning.
    let is_external = model_info.engine_type.is_external();

    if is_external {
        let mut settings = get_settings(&app_handle);
        settings.selected_model = model_id.clone();
        write_settings(&app_handle, settings);

        // Warm up in the BACKGROUND: FLM's start_serve can block for minutes
        // on first use (it downloads its model), and doing that inline froze
        // the UI on "switching". Selection is already persisted; a failed
        // eager load only means lazy loading at first use.
        let tm = transcription_manager.inner().clone();
        let id = model_id.clone();
        std::thread::spawn(move || {
            if let Err(e) = tm.load_model(&id) {
                log::warn!("Selected '{}' but it is not ready yet: {}", id, e);
            }
        });
        return Ok(());
    }

    // Local models: load first (surfaces a broken model as an error), then persist.
    transcription_manager
        .load_model(&model_id)
        .map_err(|e| e.to_string())?;

    let mut settings = get_settings(&app_handle);
    settings.selected_model = model_id.clone();
    write_settings(&app_handle, settings);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_current_model(app_handle: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app_handle);
    Ok(settings.selected_model)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcription_model_status(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<Option<String>, String> {
    Ok(transcription_manager.get_current_model())
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_loading(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<bool, String> {
    // Check if transcription manager has a loaded model
    let current_model = transcription_manager.get_current_model();
    Ok(current_model.is_none())
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_available(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let models = model_manager.get_available_models();
    Ok(models.iter().any(|m| m.is_downloaded))
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_or_downloads(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let models = model_manager.get_available_models();
    // Return true if any models are downloaded OR if any downloads are in progress
    Ok(models.iter().any(|m| m.is_downloaded))
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .cancel_download(&model_id)
        .map_err(|e| e.to_string())
}
