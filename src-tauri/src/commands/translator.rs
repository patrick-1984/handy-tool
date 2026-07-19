//! Tauri commands for the Translator (folder-watch batch transcription).
//! Settings commands follow the repo pattern: enums arrive as `String` and
//! are parsed manually; the worker thread picks changes up on its next tick.

use crate::managers::translator::{TranslatorManager, TranslatorStatus};
use crate::settings::{TranslatorFolder, TranslatorPriority, get_settings, write_settings};
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
    index: u32,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    let folder = settings
        .translator_folders
        .get_mut(index as usize)
        .ok_or_else(|| format!("No watched folder at index {index}"))?;
    folder.enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn translator_remove_folder(app: AppHandle, index: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    if (index as usize) >= settings.translator_folders.len() {
        return Err(format!("No watched folder at index {index}"));
    }
    settings.translator_folders.remove(index as usize);
    write_settings(&app, settings);
    Ok(())
}
