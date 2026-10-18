//! Commands for the Custom STT server settings and for adding Parakeet
//! models from Hugging Face.

use crate::managers::hf_parakeet::{self, HfModelSummary, HfRepoCheck};
use crate::managers::model::{ModelInfo, ModelManager};
use crate::managers::stt_server::{self, SttServerInfo};
use std::sync::Arc;
use tauri::State;

/// The STT server's models and backend (`GET {url}/models`): the settings'
/// Test button and model lists. Takes the address being typed, so it can be
/// tried before it is saved.
#[tauri::command]
#[specta::specta]
pub async fn get_stt_server_info(url: String, key: String) -> Result<SttServerInfo, String> {
    if url.trim().is_empty() {
        return Err("Enter the server address first".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || stt_server::list_models(&url, &key))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Parakeet ONNX repos on Hugging Face (most downloaded first).
#[tauri::command]
#[specta::specta]
pub async fn search_hf_parakeet_models(query: String) -> Result<Vec<HfModelSummary>, String> {
    hf_parakeet::search(&query).await.map_err(|e| e.to_string())
}

/// Whether Handy can use a repo, and what it would download.
#[tauri::command]
#[specta::specta]
pub async fn check_hf_parakeet_model(repo: String) -> Result<HfRepoCheck, String> {
    let repo = repo.trim().to_string();
    let (_, tree) = hf_parakeet::tree(&repo).await.map_err(|e| e.to_string())?;
    Ok(hf_parakeet::check_tree(&repo, &tree))
}

/// Adds a compatible repo as a model (the frontend then downloads it like any
/// other model).
#[tauri::command]
#[specta::specta]
pub async fn add_hf_parakeet_model(
    model_manager: State<'_, Arc<ModelManager>>,
    repo: String,
) -> Result<ModelInfo, String> {
    let repo = repo.trim().to_string();
    let (revision, tree) = hf_parakeet::tree(&repo).await.map_err(|e| e.to_string())?;
    let files = hf_parakeet::pick_files(&tree)
        .map_err(|reason| format!("Handy cannot use {repo}: {reason}"))?;
    model_manager
        .add_hf_parakeet_model(&repo, &revision, files)
        .map_err(|e| e.to_string())
}
