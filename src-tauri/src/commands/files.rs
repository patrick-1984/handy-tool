//! Transcribe a file (the Files page). The file is decoded, cut at the pauses
//! into pieces of at most ~40 s (the watched folders' segmenter) and each piece
//! is transcribed once with the model picked for files - no live text and no
//! second pass, so the text is final as it comes. The original file is only
//! ever read. The text can be saved as a .txt in Handy's folder (or a chosen
//! one) or next to the recording, and a copy of the audio kept in that folder;
//! every result goes into a log shown on the page.

use crate::audio_toolkit::audio::{SUPPORTED_EXTENSIONS, decode_audio_file};
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::managers::translator::split_speech_segments;
use crate::settings::{AppSettings, FileTextSave, get_settings, write_settings};
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

const SAMPLE_HZ: f64 = 16_000.0;
const LOG_FILE: &str = "transcribed_files.json";

/// One transcribed file, as kept in the log.
#[derive(Clone, Serialize, Deserialize, Type)]
pub struct FileTranscription {
    /// When it finished, in milliseconds since 1970; also its id.
    pub id: i64,
    pub file_name: String,
    pub source_path: String,
    pub duration_secs: f64,
    pub model: String,
    pub text: String,
    /// Where the .txt was saved, when "Save the text as a file" was on.
    pub text_path: Option<String>,
    /// Where the audio was copied, when "Keep a copy of the audio" was on.
    pub audio_path: Option<String>,
    /// Why saving the text or copying the audio failed, if it did.
    pub save_error: Option<String>,
}

/// The file being transcribed, or how the last one ended.
#[derive(Clone, Serialize, Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FileJob {
    Decoding {
        file_name: String,
    },
    Transcribing {
        file_name: String,
        done_secs: f64,
        total_secs: f64,
    },
    Done {
        entry: FileTranscription,
    },
    Failed {
        file_name: String,
        error: String,
    },
    Cancelled {
        file_name: String,
    },
}

static JOB: Lazy<Mutex<Option<FileJob>>> = Lazy::new(|| Mutex::new(None));
static CANCEL: AtomicBool = AtomicBool::new(false);
/// Serializes read-modify-write of the log file.
static LOG_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn publish(app: &AppHandle, job: FileJob) {
    *JOB.lock().unwrap_or_else(|p| p.into_inner()) = Some(job.clone());
    let _ = app.emit("file-job", job);
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// The chosen folder, or `{app_data}/files`.
fn files_folder(app: &AppHandle, settings: &AppSettings) -> Result<PathBuf, String> {
    let chosen = settings.files_folder.trim();
    if !chosen.is_empty() {
        return Ok(PathBuf::from(chosen));
    }
    crate::portable::resolve_app_data_dir(app).map(|d| d.join("files"))
}

/// A name for the .txt (and, with `with_audio`, the audio copy) that
/// overwrites nothing: `talk`, then `talk (2)`, `talk (3)`...
fn unique_stem(folder: &Path, source: &Path, with_audio: bool) -> String {
    let base = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "audio".into());
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base} ({n})")
            }
        })
        .find(|stem| {
            !folder.join(format!("{stem}.txt")).exists()
                && !(with_audio && folder.join(format!("{stem}.{ext}")).exists())
        })
        .expect("an unused name")
}

/// Saves the text where `save` says and copies the audio if the setting
/// says so. Returns the two paths and the first error.
fn save_outputs(
    app: &AppHandle,
    settings: &AppSettings,
    source: &Path,
    text: &str,
    save: FileTextSave,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut error = None;
    // The audio copy, and the text when it goes there too, share one name in
    // Handy's folder.
    let handy = if settings.file_keep_audio || save == FileTextSave::HandyFolder {
        match files_folder(app, settings).and_then(|f| {
            std::fs::create_dir_all(&f)
                .map(|_| f)
                .map_err(|e| e.to_string())
        }) {
            Ok(folder) => {
                let stem = unique_stem(&folder, source, true);
                Some((folder, stem))
            }
            Err(e) => {
                error = Some(e);
                None
            }
        }
    } else {
        None
    };
    let text_out = match save {
        FileTextSave::HandyFolder => handy
            .as_ref()
            .map(|(folder, stem)| folder.join(format!("{stem}.txt"))),
        FileTextSave::NextToFile => source
            .parent()
            .map(|dir| dir.join(format!("{}.txt", unique_stem(dir, source, false)))),
        FileTextSave::Ask | FileTextSave::DontSave => None,
    };
    let mut text_path = None;
    if let Some(out) = text_out {
        match std::fs::write(&out, text) {
            Ok(()) => text_path = Some(out.display().to_string()),
            Err(e) => {
                error.get_or_insert(format!("{}: {e}", out.display()));
            }
        }
    }
    let mut audio_path = None;
    if let (true, Some((folder, stem))) = (settings.file_keep_audio, &handy) {
        let mut out = folder.join(stem);
        if let Some(ext) = source.extension() {
            out.set_extension(ext);
        }
        match std::fs::copy(source, &out) {
            Ok(_) => audio_path = Some(out.display().to_string()),
            Err(e) => {
                error.get_or_insert(format!("{}: {e}", out.display()));
            }
        }
    }
    (text_path, audio_path, error)
}

fn log_path(app: &AppHandle) -> Result<PathBuf, String> {
    crate::portable::resolve_app_data_dir(app).map(|d| d.join(LOG_FILE))
}

fn read_log(app: &AppHandle) -> Vec<FileTranscription> {
    log_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_log(app: &AppHandle, entries: &[FileTranscription]) -> Result<(), String> {
    let path = log_path(app)?;
    let bytes = serde_json::to_vec(entries).map_err(|e| e.to_string())?;
    // Atomic: a crash mid-write must not lose the whole log.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| e.to_string())
}

/// One piece through the right engine slot, checked per piece. The dictation
/// model uses the shared slot; any other local model loads into the second
/// slot beside it (the watched folders' one), so dictation stays ready -
/// switching the dictation model mid-file just moves the rest there.
fn transcribe_piece(
    app: &AppHandle,
    tm: &TranscriptionManager,
    model: &str,
    external: bool,
    piece: Vec<f32>,
) -> Result<String, String> {
    let is_dictation = get_settings(app).selected_model == model;
    let load_error = |e: anyhow::Error| format!("could not load the model: {e}");
    if is_dictation || external {
        if !is_dictation {
            // An online engine has no second slot.
            return Err("the speech model was changed while the file was being transcribed".into());
        }
        // Loaded on first use, and again if it was unloaded (idle timeout).
        if tm.get_current_model().as_deref() != Some(model) {
            tm.load_model(model).map_err(load_error)?;
        }
        if external {
            return tm
                .transcribe_expecting(model, piece)
                .map_err(|e| e.to_string());
        }
        // The local engine takes one job at a time: a take being dictated
        // waits for this piece at most.
        let _serial = crate::actions::CHUNK_TRANSCRIBE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        return tm
            .transcribe_expecting(model, piece)
            .map_err(|e| e.to_string());
    }
    // The watched folders can load their own model into this slot between
    // two pieces: load ours back and try once more.
    let mut error = String::new();
    for _ in 0..2 {
        if tm.get_current_translator_model().as_deref() != Some(model) {
            tm.load_translator_model(model).map_err(load_error)?;
        }
        let _serial = crate::actions::CHUNK_TRANSCRIBE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        match tm.transcribe_translator_expecting(model, piece.clone()) {
            Ok(text) => return Ok(text),
            Err(e) => error = e.to_string(),
        }
    }
    Err(error)
}

/// A model loaded into the second slot only for a file doesn't stay in memory.
fn release_model(app: &AppHandle, model: &str) {
    let settings = get_settings(app);
    let watched_folders_use_it = settings.translator_enabled && settings.translator_model == model;
    let tm = app.state::<Arc<TranscriptionManager>>();
    if !watched_folders_use_it && tm.get_current_translator_model().as_deref() == Some(model) {
        let _ = tm.unload_translator_model();
    }
}

/// Decodes, transcribes piece by piece and saves. `Ok(None)` = cancelled.
fn transcribe_whole(
    app: &AppHandle,
    path: &Path,
    file_name: &str,
    model: &str,
    save: FileTextSave,
) -> Result<Option<FileTranscription>, String> {
    let settings = get_settings(app);
    let model_info = app.state::<Arc<ModelManager>>().get_model_info(model);
    let Some(model_info) = model_info.filter(|m| m.is_downloaded) else {
        return Err("the model isn't downloaded".into());
    };
    let external = model_info.engine_type.is_external();
    let samples = decode_audio_file(path).map_err(|e| e.to_string())?;
    let total_secs = samples.len() as f64 / SAMPLE_HZ;
    let segments = split_speech_segments(&samples);
    info!(
        "Files: transcribing {} ({total_secs:.0}s, {} pieces)",
        path.display(),
        segments.len()
    );
    publish(
        app,
        FileJob::Transcribing {
            file_name: file_name.to_string(),
            done_secs: 0.0,
            total_secs,
        },
    );

    let tm = app.state::<Arc<TranscriptionManager>>();
    let mut parts: Vec<String> = Vec::new();
    for range in segments {
        if CANCEL.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let piece = samples[range.clone()].to_vec();
        let text = transcribe_piece(app, &tm, model, external, piece)?;
        let text = text.trim();
        if !text.is_empty() {
            parts.push(text.to_string());
        }
        publish(
            app,
            FileJob::Transcribing {
                file_name: file_name.to_string(),
                done_secs: range.end as f64 / SAMPLE_HZ,
                total_secs,
            },
        );
    }
    if CANCEL.load(Ordering::SeqCst) {
        return Ok(None);
    }

    let text = parts.join(" ");
    let (text_path, audio_path, save_error) = save_outputs(app, &settings, path, &text, save);
    if let Some(e) = &save_error {
        warn!("Files: could not save the outputs: {e}");
    }
    let entry = FileTranscription {
        id: chrono::Utc::now().timestamp_millis(),
        file_name: file_name.to_string(),
        source_path: path.display().to_string(),
        duration_secs: total_secs,
        model: model_info.name.clone(),
        text,
        text_path,
        audio_path,
        save_error,
    };
    {
        let _guard = LOG_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut log = read_log(app);
        log.insert(0, entry.clone());
        if let Err(e) = write_log(app, &log) {
            warn!("Files: could not write the log: {e}");
        }
    }
    info!(
        "Files: finished {} ({} chars)",
        path.display(),
        entry.text.len()
    );
    Ok(Some(entry))
}

/// Starts transcribing `path` with `model_id` in the background, the text
/// going where `save` says; progress and the result arrive as `file-job`
/// events.
#[tauri::command]
#[specta::specta]
pub fn transcribe_file(
    app: AppHandle,
    path: String,
    model_id: String,
    save: FileTextSave,
) -> Result<(), String> {
    if save == FileTextSave::Ask {
        return Err("where to save the text was not chosen".into());
    }
    let model_ready = app
        .state::<Arc<ModelManager>>()
        .get_model_info(&model_id)
        .is_some_and(|m| m.is_downloaded);
    if !model_ready {
        return Err("download the model first".into());
    }
    let path = PathBuf::from(path.trim());
    if !path.is_file() {
        return Err(format!("not a file: {}", path.display()));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!("this file type (.{ext}) can't be read"));
    }
    let file_name = file_name_of(&path);
    {
        let mut job = JOB.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(
            *job,
            Some(FileJob::Decoding { .. } | FileJob::Transcribing { .. })
        ) {
            return Err("a file is already being transcribed".into());
        }
        *job = Some(FileJob::Decoding {
            file_name: file_name.clone(),
        });
    }
    CANCEL.store(false, Ordering::SeqCst);
    let _ = app.emit(
        "file-job",
        FileJob::Decoding {
            file_name: file_name.clone(),
        },
    );
    std::thread::Builder::new()
        .name("file-transcribe".into())
        .spawn(move || {
            let result = transcribe_whole(&app, &path, &file_name, &model_id, save);
            release_model(&app, &model_id);
            let job = match result {
                Ok(Some(entry)) => FileJob::Done { entry },
                Ok(None) => FileJob::Cancelled { file_name },
                Err(error) => {
                    warn!("Files: {} failed: {error}", path.display());
                    FileJob::Failed { file_name, error }
                }
            };
            publish(&app, job);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The file being transcribed, or how the last one ended (None before the first).
#[tauri::command]
#[specta::specta]
pub fn get_file_job() -> Option<FileJob> {
    JOB.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Stops the file being transcribed after the piece in progress.
#[tauri::command]
#[specta::specta]
pub fn cancel_file_job() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Transcribed files, newest first.
#[tauri::command]
#[specta::specta]
pub fn get_file_transcriptions(app: AppHandle) -> Vec<FileTranscription> {
    let _guard = LOG_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    read_log(&app)
}

/// Removes an entry from the log (its saved .txt and audio copy stay).
#[tauri::command]
#[specta::specta]
pub fn delete_file_transcription(app: AppHandle, id: i64) -> Result<(), String> {
    let _guard = LOG_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut log = read_log(&app);
    log.retain(|e| e.id != id);
    write_log(&app, &log)
}

/// The folder the text and audio copies go to (the default one when none is chosen).
#[tauri::command]
#[specta::specta]
pub fn get_files_folder(app: AppHandle) -> Result<String, String> {
    files_folder(&app, &get_settings(&app)).map(|p| p.display().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn open_files_folder(app: AppHandle) -> Result<(), String> {
    let folder = files_folder(&app, &get_settings(&app))?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(folder.display().to_string(), None::<String>)
        .map_err(|e| format!("Failed to open the folder: {e}"))
}

#[tauri::command]
#[specta::specta]
pub fn change_file_text_save_setting(app: AppHandle, save: FileTextSave) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.file_text_save = save;
    write_settings(&app, settings);
    Ok(())
}

/// The model picked for files (remembered). Empty = the dictation model.
#[tauri::command]
#[specta::specta]
pub fn change_file_model_setting(
    app: AppHandle,
    model_manager: tauri::State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    if !model_id.is_empty() && model_manager.get_model_info(&model_id).is_none() {
        return Err(format!("Unknown model: {model_id}"));
    }
    let mut settings = get_settings(&app);
    settings.file_model = model_id;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_file_keep_audio_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.file_keep_audio = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// Empty = back to Handy's own folder.
#[tauri::command]
#[specta::specta]
pub fn change_files_folder_setting(app: AppHandle, path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if !trimmed.is_empty() && !Path::new(trimmed).is_dir() {
        return Err(format!("Not a folder: {trimmed}"));
    }
    let mut settings = get_settings(&app);
    settings.files_folder = trimmed.to_string();
    write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_stem_never_reuses_a_taken_name() {
        let dir = tempfile::TempDir::new().unwrap();
        let source = Path::new("C:/phone/talk.m4a");
        assert_eq!(unique_stem(dir.path(), source, true), "talk");
        std::fs::write(dir.path().join("talk.txt"), "x").unwrap();
        assert_eq!(unique_stem(dir.path(), source, true), "talk (2)");
        std::fs::write(dir.path().join("talk (2).m4a"), "x").unwrap();
        assert_eq!(unique_stem(dir.path(), source, true), "talk (3)");
        // Next to the recording the audio itself is there: only the .txt counts.
        assert_eq!(unique_stem(dir.path(), source, false), "talk (2)");
        std::fs::write(dir.path().join("talk (2).txt"), "x").unwrap();
        assert_eq!(unique_stem(dir.path(), source, false), "talk (3)");
    }
}
