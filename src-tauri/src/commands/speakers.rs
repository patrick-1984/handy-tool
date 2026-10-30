//! Speakers on this PC: the speakers model's card (status, download, delete).

use crate::diarize::{self, LocalSpeakersStatus};
use tauri::AppHandle;

#[tauri::command]
#[specta::specta]
pub fn get_local_speakers_status(app: AppHandle) -> LocalSpeakersStatus {
    diarize::status(&app)
}

/// Downloads the speakers model; progress comes as `local-speakers-status`.
#[tauri::command]
#[specta::specta]
pub async fn download_local_speakers_model(app: AppHandle) -> Result<(), String> {
    diarize::download(&app).await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn cancel_local_speakers_download() {
    diarize::cancel_download();
}

#[tauri::command]
#[specta::specta]
pub fn delete_local_speakers_model(app: AppHandle) -> Result<(), String> {
    diarize::delete(&app).map_err(|e| e.to_string())
}
