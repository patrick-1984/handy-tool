use crate::managers::model::{EngineType, ModelInfo, ModelManager};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{AppSettings, get_settings, write_settings};
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

    // External/API engines are configured separately (Advanced > Providers)
    // and validate/load lazily at transcription time. Selecting one must persist
    // even before it's configured — otherwise it's a select-then-configure
    // deadlock — so we save the selection first and treat an eager-load failure
    // (usually "not configured yet") as a non-fatal, event-surfaced warning.
    let is_external = model_info.engine_type.is_external();

    if is_external {
        let mut settings = get_settings(&app_handle);
        settings.selected_model = model_id.clone();
        write_settings(&app_handle, settings);

        // Warm up in the BACKGROUND. Use `reload_external_model_if_latest` (NOT
        // `initiate_model_load`) so RE-selecting an external model force-reloads
        // it — that's the intended recovery when e.g. an FLM subprocess died
        // (initiate_model_load's model-id preflight would no-op on a dead-but-
        // "loaded" engine). It is single-flight (`load_flight`) and its FLM arm
        // stops the old child before spawning a new one, so this can't race
        // recording start or the Translator into a duplicate `flm serve`. The
        // generation guard makes rapid re-selections latest-intent-wins: a
        // superseded warm-up bails instead of restarting a freshly-loaded FLM.
        // A failure still surfaces via load_model's `loading_failed` event.
        let tm = transcription_manager.inner().clone();
        let id = model_id.clone();
        let select_gen = tm.next_external_select_gen();
        std::thread::spawn(move || {
            if let Err(e) = tm.reload_external_model_if_latest(&id, select_gen) {
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

/// T-106: is this registry entry a REAL, usable transcription path right now?
///
/// Local engines (Whisper/Parakeet/Moonshine/SenseVoice) are usable once
/// `is_downloaded` is true — the registry probes the filesystem for these
/// (see `update_download_status` in `managers/model.rs`).
///
/// External engines (`EngineType::is_external()`) are a DIFFERENT story: the
/// registry always sets `is_downloaded: true` for API/OpenRouter (they have
/// no on-disk artifact — see the doc comment on `EngineType::is_external`),
/// and FLM's entry is inserted when its executable path is present. The model
/// manager filters that entry after Windows Application Control marks it blocked.
/// So is_downloaded alone cannot distinguish
/// "the user configured this" from "this engine merely exists" — that's
/// exactly the T-106 bug (a fresh install with an unconfigured API/OpenRouter
/// entry skipped onboarding into a broken state). Require each external
/// engine's own minimum usable configuration instead:
/// - `ApiWhisper`: a non-empty endpoint URL (the API key is legitimately
///   optional for some OpenAI-compatible servers, e.g. local FLM/faster-
///   whisper-server without auth).
/// - `OpenRouterWhisper`: a configured provider reference that resolves to a
///   registered `llm_providers` entry with a non-empty API key (the
///   transcription model itself may be left unset — the engine falls back to
///   `openai/whisper-large-v3`, see CLAUDE.md).
/// - FlmWhisper: model-manager queries filter out a policy-blocked FLM before
///   this helper sees it; is_downloaded is true for the remaining entry.
fn is_model_usable(model: &ModelInfo, settings: &AppSettings) -> bool {
    match model.engine_type {
        EngineType::ApiWhisper => !settings.api_transcription_url.trim().is_empty(),
        EngineType::OpenRouterWhisper => {
            // T-308: usable when the dedicated URL + API key are set (no longer
            // tied to an `llm_providers` entry). Model is optional (defaults).
            !settings.openrouter_transcription_url.trim().is_empty()
                && !settings.openrouter_transcription_key.trim().is_empty()
        }
        #[cfg(not(target_os = "macos"))]
        EngineType::FlmWhisper => model.is_downloaded,
        _ => model.is_downloaded,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_available(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let settings = get_settings(&app_handle);
    let models = model_manager.get_available_models();
    Ok(models.iter().any(|m| is_model_usable(m, &settings)))
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_or_downloads(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let settings = get_settings(&app_handle);
    let models = model_manager.get_available_models();
    // Return true if any models are usable OR if any downloads are in progress
    Ok(models
        .iter()
        .any(|m| is_model_usable(m, &settings) || m.is_downloading))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    /// Minimal `ModelInfo` builder for the fields `is_model_usable` cares
    /// about; the rest are cosmetic (name/description/scores/etc.).
    fn model(engine_type: EngineType, is_downloaded: bool) -> ModelInfo {
        ModelInfo {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            description: String::new(),
            filename: String::new(),
            url: None,
            size_mb: 0,
            is_downloaded,
            is_downloading: false,
            partial_size: 0,
            is_directory: false,
            engine_type,
            accuracy_score: 0.0,
            speed_score: 0.0,
            supports_translation: false,
            is_recommended: false,
            supported_languages: Vec::new(),
            is_custom: false,
        }
    }

    #[test]
    fn local_engine_usable_only_when_downloaded() {
        let settings = get_default_settings();
        assert!(!is_model_usable(
            &model(EngineType::Whisper, false),
            &settings
        ));
        assert!(is_model_usable(
            &model(EngineType::Whisper, true),
            &settings
        ));
    }

    #[test]
    fn api_engine_requires_url_regardless_of_registry_is_downloaded() {
        let mut settings = get_default_settings();
        // Registry always marks api-whisper as `is_downloaded: true` even
        // when nothing is configured — this is exactly the T-106 bug.
        let m = model(EngineType::ApiWhisper, true);

        settings.api_transcription_url = String::new();
        assert!(
            !is_model_usable(&m, &settings),
            "unconfigured API engine must not count as usable"
        );

        settings.api_transcription_url = "   ".to_string();
        assert!(
            !is_model_usable(&m, &settings),
            "whitespace-only URL must not count as usable"
        );

        settings.api_transcription_url = "http://localhost:8080".to_string();
        assert!(is_model_usable(&m, &settings));
    }

    #[test]
    fn openrouter_engine_requires_url_and_api_key() {
        // T-308: OpenRouter transcription now uses dedicated URL + key fields
        // (decoupled from the llm_providers registry).
        let mut settings = get_default_settings();
        let m = model(EngineType::OpenRouterWhisper, true);

        // Fresh defaults ship the OpenRouter URL but no key → not usable.
        settings.openrouter_transcription_key = String::new();
        assert!(!is_model_usable(&m, &settings));

        // URL + key present → usable (model defaults to openai/whisper-large-v3).
        settings.openrouter_transcription_url = "https://openrouter.ai/api/v1".to_string();
        settings.openrouter_transcription_key = "sk-test".to_string();
        assert!(is_model_usable(&m, &settings));

        // Whitespace-only URL must not count as usable even with a key.
        settings.openrouter_transcription_url = "   ".to_string();
        assert!(!is_model_usable(&m, &settings));
    }

    #[test]
    fn flm_engine_trusts_registry_is_downloaded() {
        // Model-manager queries filter a known policy-blocked FLM entry before
        // usability checks; the remaining registry entry is authoritative.
        #[cfg(not(target_os = "macos"))]
        {
            let settings = get_default_settings();
            assert!(is_model_usable(
                &model(EngineType::FlmWhisper, true),
                &settings
            ));
        }
    }
}
