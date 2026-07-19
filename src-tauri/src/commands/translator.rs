//! Tauri commands for the Translator (folder-watch batch transcription).
//! Settings commands follow the repo pattern: enums arrive as `String` and
//! are parsed manually; the worker thread picks changes up on its next tick.

use crate::managers::translator::{TranslatorManager, TranslatorStatus};
use crate::settings::{
    TranslatorFolder, TranslatorPriority, get_settings, update_settings, write_settings,
};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub fn get_translator_status(
    translator: State<'_, Arc<TranslatorManager>>,
) -> Result<TranslatorStatus, String> {
    Ok(translator.status())
}

#[tauri::command]
#[specta::specta]
pub fn change_translator_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.translator_enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_translator_priority(app: AppHandle, priority: String) -> Result<(), String> {
    let parsed = match priority.as_str() {
        "live_first" => TranslatorPriority::LiveFirst,
        "folder_first" => TranslatorPriority::FolderFirst,
        "fifo" => TranslatorPriority::Fifo,
        other => return Err(format!("Unknown translator priority: {other}")),
    };
    let mut settings = get_settings(&app);
    settings.translator_priority = parsed;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_translator_model(
    app: AppHandle,
    model_manager: State<'_, Arc<crate::managers::model::ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    let trimmed = model_id.trim().to_string();
    if !trimmed.is_empty() {
        let info = model_manager
            .get_model_info(&trimmed)
            .ok_or_else(|| format!("Unknown model: {trimmed}"))?;
        if !info.is_downloaded {
            return Err(format!("Model not downloaded: {trimmed}"));
        }
    }
    let mut settings = get_settings(&app);
    settings.translator_model = trimmed;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn translator_add_folder(app: AppHandle, path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Folder path is empty".into());
    }
    if !std::path::Path::new(trimmed).is_dir() {
        return Err(format!("Not a folder: {trimmed}"));
    }
    let mut settings = get_settings(&app);
    if settings
        .translator_folders
        .iter()
        .any(|f| f.path.eq_ignore_ascii_case(trimmed))
    {
        return Err("This folder is already being watched".into());
    }
    settings.translator_folders.push(TranslatorFolder {
        path: trimmed.to_string(),
        enabled: true,
    });
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn translator_set_folder_enabled(
    app: AppHandle,
    path: String,
    enabled: bool,
) -> Result<(), String> {
    // T-109 — the first production migration of T-111's `update_settings`
    // helper (see tickets/T-111-*.md). This command and
    // `translator_remove_folder` below used to each run their own bare
    // get_settings/mutate/write_settings read-modify-write with no
    // cross-call coordination: toggling folder A's enabled flag while
    // removing folder B ran concurrently could each read the SAME
    // pre-mutation settings and write back a version missing the other
    // call's change — a toggle could resurrect a folder just removed, or a
    // removal could silently discard a concurrent toggle. Routing both
    // through `update_settings` serializes them (and every other migrated
    // writer) under one process-wide lock, so both mutations always land.
    let mut found = false;
    update_settings(&app, |settings| {
        if let Some(folder) = settings
            .translator_folders
            .iter_mut()
            .find(|f| f.path == path)
        {
            folder.enabled = enabled;
            found = true;
        }
    });
    if !found {
        return Err(format!("No watched folder: {path}"));
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn translator_remove_folder(app: AppHandle, path: String) -> Result<(), String> {
    // T-109: see `translator_set_folder_enabled` above — same race, same fix.
    let mut removed = false;
    update_settings(&app, |settings| {
        let before = settings.translator_folders.len();
        settings.translator_folders.retain(|f| f.path != path);
        removed = settings.translator_folders.len() != before;
    });
    if !removed {
        return Err(format!("No watched folder: {path}"));
    }
    Ok(())
}
