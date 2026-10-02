//! Commands for speaker detection ("Make note with speakers"): the speaker
//! models and speaker-labelled transcripts of History entries.

use crate::diarization::{
    self, DiarizationFailed, SpeakerTranscriber, TimedTranscript, TimingSupport,
    models::{self, SpeakerModelStatus},
};
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use anyhow::anyhow;
use log::warn;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Error codes returned to the frontend, which shows them translated.
const ERR_MODELS_MISSING: &str = "speakers_models_missing";
const ERR_NO_RECORDING: &str = "speakers_no_recording";
const ERR_NO_SPEECH: &str = "speakers_no_speech";
const ERR_NO_MODEL: &str = "speakers_no_model";
const ERR_FAILED: &str = "speakers_failed";

/// Whether the speaker models are present, downloading, or failed.
#[tauri::command]
#[specta::specta]
pub fn get_speaker_model_status(app: AppHandle) -> SpeakerModelStatus {
    models::status(&app)
}

/// Download (or resume) the speaker models. Progress arrives as
/// `speaker-model-status` events; returns once the download has finished.
#[tauri::command]
#[specta::specta]
pub async fn download_speaker_models(app: AppHandle) -> Result<(), String> {
    models::download(&app).await.map_err(|e| e.to_string())
}

/// Delete the downloaded speaker models to free space; they can be
/// downloaded again at any time. Refused with `speakers_models_in_use`
/// while "Make note with speakers" runs and `speakers_models_downloading`
/// during a download.
#[tauri::command]
#[specta::specta]
pub fn delete_speaker_models(app: AppHandle) -> Result<(), String> {
    models::delete(&app).map_err(|e| e.to_string())
}

/// The selected transcription model, through the main engine slot with the
/// same locks as the Files page: loaded on first use (and again after an
/// idle unload), local engine calls serialized with dictation by
/// `CHUNK_TRANSCRIBE_LOCK`, and every call checked against the model.
struct SelectedModel<'a> {
    tm: &'a TranscriptionManager,
    model: String,
    external: bool,
}

impl SelectedModel<'_> {
    fn ensure_loaded(&self) -> anyhow::Result<()> {
        if self.tm.get_current_model().as_deref() != Some(self.model.as_str()) {
            self.tm
                .load_model(&self.model)
                .map_err(|e| anyhow!("could not load the model: {e}"))?;
        }
        Ok(())
    }

    fn run<T>(&self, call: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
        self.ensure_loaded()?;
        if self.external {
            return call();
        }
        let _serial = crate::actions::CHUNK_TRANSCRIBE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        call()
    }
}

impl SpeakerTranscriber for SelectedModel<'_> {
    fn timing_support(&self) -> TimingSupport {
        self.tm.timing_support()
    }

    fn transcribe_timed(&self, audio: &[f32]) -> anyhow::Result<TimedTranscript> {
        self.run(|| {
            self.tm
                .transcribe_with_timing_expecting(&self.model, audio.to_vec())
        })
    }

    fn transcribe(&self, audio: &[f32]) -> anyhow::Result<String> {
        self.run(|| self.tm.transcribe_expecting(&self.model, audio.to_vec()))
    }
}

/// Transcribe a History entry's recording with speaker labels
/// (`[Person N]: …`) for a note with speakers, with the selected
/// transcription model. With one speaker the text comes back without
/// labels. The entry's stored text is left as it is.
///
/// Runs one at a time: a second call waits for the first. The entry is
/// starred, as for any note, so the retention cleanup keeps it.
///
/// Error codes: `speakers_models_missing`, `speakers_no_recording`,
/// `speakers_no_speech`, `speakers_no_model`, `speakers_failed` (the speaker
/// models failed); transcription errors are returned as they come.
#[tauri::command]
#[specta::specta]
pub async fn transcribe_history_entry_with_speakers(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    id: i64,
) -> Result<String, String> {
    let _in_use = models::lock_for_use().await;
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    if entry.audio_purged_at.is_some() || entry.file_name.is_empty() || !audio_path.is_file() {
        return Err(ERR_NO_RECORDING.to_string());
    }
    let Some(paths) = models::installed_paths(&app) else {
        return Err(ERR_MODELS_MISSING.to_string());
    };
    let model = get_settings(&app).selected_model;
    let Some(model_info) = model_manager
        .get_model_info(&model)
        .filter(|m| m.is_downloaded)
    else {
        return Err(ERR_NO_MODEL.to_string());
    };

    // Star the entry before the slow part, as generate_note does, so the
    // retention cleanup can't delete it while its note is being made.
    if !history_manager
        .mark_entry_saved(id)
        .map_err(|e| e.to_string())?
    {
        return Err(format!("History entry {} not found", id));
    }

    let tm = Arc::clone(&transcription_manager);
    let external = model_info.engine_type.is_external();
    let text = tauri::async_runtime::spawn_blocking(move || {
        let samples = crate::audio_toolkit::audio::decode_audio_file(&audio_path)
            .map_err(|e| anyhow!("Failed to load audio: {e}"))?;
        if samples.is_empty() {
            return Ok(String::new());
        }
        // Keep the model loaded across all the engine calls.
        let hold = tm.hold_loaded_model();
        let selected = SelectedModel {
            tm: &tm,
            model,
            external,
        };
        let result = selected
            .ensure_loaded()
            .and_then(|()| diarization::transcribe_with_speakers(&paths, &selected, &samples));
        drop(hold);
        tm.maybe_unload_immediately("transcription with speakers");
        result
    })
    .await
    .map_err(|e| format!("Transcription task panicked: {}", e))?
    .map_err(|e| {
        warn!("Transcription with speakers failed: {:#}", e);
        if e.is::<DiarizationFailed>() {
            ERR_FAILED.to_string()
        } else {
            e.to_string()
        }
    })?;

    if text.trim().is_empty() {
        return Err(ERR_NO_SPEECH.to_string());
    }
    Ok(text)
}
