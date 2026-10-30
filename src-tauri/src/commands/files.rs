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
/// Speaker requests of takes already delivered, still running (their result
/// exists only in that thread until it lands in History): the updater waits.
pub static BACKGROUND_SPEAKERS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);
/// Serializes read-modify-write of the log file.
static LOG_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// True while a file is being decoded or transcribed. The updater waits for it:
/// until the file is done, its text exists only in this worker's memory.
/// History's "Transcribe again" counts too (its text is not saved yet either).
pub fn file_job_running() -> bool {
    RETRANSCRIBING.load(Ordering::SeqCst)
        || BACKGROUND_SPEAKERS.load(Ordering::SeqCst) > 0
        || matches!(
            *JOB.lock().unwrap_or_else(|p| p.into_inner()),
            Some(FileJob::Decoding { .. } | FileJob::Transcribing { .. })
        )
}

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

/// Where the audio copy goes: built exactly as `unique_stem` checked it.
/// `set_extension()` would replace a dot inside the stem ("meeting.2026" ->
/// "meeting.m4a") and copy over a file that was never checked.
fn audio_copy_path(folder: &Path, stem: &str, source: &Path) -> PathBuf {
    match source.extension() {
        Some(ext) => folder.join(format!("{stem}.{}", ext.to_string_lossy())),
        None => folder.join(stem),
    }
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
        let out = audio_copy_path(folder, stem, source);
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
/// switching the dictation model mid-file just moves the rest there. FLM and
/// the online engines need no slot and go straight to their server.
fn transcribe_piece(
    app: &AppHandle,
    tm: &TranscriptionManager,
    model: &str,
    external: bool,
    piece: Vec<f32>,
) -> Result<String, String> {
    transcribe_piece_with(app, tm, model, external, piece, false)
}

/// `lock_held`: the caller already holds CHUNK_TRANSCRIBE_LOCK (a take's
/// failover) - do not take it again.
fn transcribe_piece_with(
    app: &AppHandle,
    tm: &TranscriptionManager,
    model: &str,
    external: bool,
    piece: Vec<f32>,
    lock_held: bool,
) -> Result<String, String> {
    let serial = || {
        (!lock_held).then(|| {
            crate::actions::CHUNK_TRANSCRIBE_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner())
        })
    };
    let is_dictation = get_settings(app).selected_model == model;
    let load_error = |e: anyhow::Error| format!("could not load the model: {e}");
    // FLM (the NPU server) runs beside whatever dictation uses: straight to the
    // server, never through an engine slot (and not under the chunk lock - the
    // NPU does not compete with the GPU/CPU engines). Also when FLM IS the
    // dictation model: dictation may switch away mid-file, and only dictation's
    // own takes drive the overlay's percentage.
    if tm.is_flm_model(model) {
        return tm
            .transcribe_flm_direct(model, piece)
            .map_err(|e| e.to_string());
    }
    // An online engine (API, OpenRouter) is a stateless request: straight to
    // its endpoint, beside whatever dictation uses (and its cost is not billed
    // to a take being dictated).
    if external {
        return tm
            .transcribe_online_direct(model, piece)
            .map_err(|e| e.to_string());
    }
    if is_dictation {
        // Loaded on first use, and again if it was unloaded (idle timeout).
        if tm.get_current_model().as_deref() != Some(model) {
            tm.load_model(model).map_err(load_error)?;
        }
        // The local engine takes one job at a time: a take being dictated
        // waits for this piece at most.
        let _serial = serial();
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
        let _serial = serial();
        match tm.transcribe_translator_expecting(model, piece.clone()) {
            Ok(text) => return Ok(text),
            Err(e) => error = e.to_string(),
        }
    }
    Err(error)
}

/// The whole of `samples` with `model` (any engine), cut at its pauses into
/// pieces as for a file. Used by a take's failover (`lock_held` = the caller
/// holds CHUNK_TRANSCRIBE_LOCK). A local model other than the dictation one
/// runs in the second slot and is released afterwards.
pub fn transcribe_samples_with(
    app: &AppHandle,
    tm: &TranscriptionManager,
    model: &str,
    samples: &[f32],
    lock_held: bool,
) -> Result<String, String> {
    let info = app
        .state::<Arc<ModelManager>>()
        .get_model_info(model)
        .filter(|m| m.is_downloaded)
        .ok_or_else(|| format!("{model} is not available"))?;
    let external = info.engine_type.is_external();
    // An online engine takes the whole take at once (one request); a local
    // engine reads it in pieces.
    let result = if external && !tm.is_flm_model(model) {
        transcribe_piece_with(app, tm, model, true, samples.to_vec(), lock_held)
    } else {
        let mut parts = Vec::new();
        let mut failed = None;
        for range in split_speech_segments(samples) {
            match transcribe_piece_with(
                app,
                tm,
                model,
                external,
                samples[range].to_vec(),
                lock_held,
            ) {
                Ok(text) => {
                    let text = text.trim();
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(parts.join(" ")),
        }
    };
    release_model(app, model);
    result
}

/// Whether the watched folders can use `model`: a downloaded model; an online
/// one (API, OpenRouter) only when picked for the folders themselves or when
/// it is the dictation model - the same rule the Files page shows.
pub fn usable_for_folders(settings: &AppSettings, models: &ModelManager, model: &str) -> bool {
    use crate::managers::model::EngineType;
    models.get_model_info(model).is_some_and(|info| {
        let online = matches!(
            info.engine_type,
            EngineType::ApiWhisper | EngineType::OpenRouterWhisper | EngineType::SttServer
        );
        info.is_downloaded
            && (!online
                || model == settings.translator_model.trim()
                || model == settings.selected_model)
    })
}

/// A model loaded into the second slot only for a file doesn't stay in memory.
fn release_model(app: &AppHandle, model: &str) {
    let settings = get_settings(app);
    let models = app.state::<Arc<ModelManager>>();
    let watched_folders_use_it = settings.translator_enabled
        && settings.watched_folder_model(|m| usable_for_folders(&settings, &models, m)) == model;
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
        // Checked under the JOB lock: the updater reserves first and then reads
        // file_job_running(), which takes this lock, so a file either starts
        // before that read (and the update waits) or sees the reservation here.
        if crate::transcription_coordinator::update_pending() {
            return Err("an update is being installed; try again after the restart".into());
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
    // Kept for the spawn-failure path below: the closure takes the originals.
    let (app_on_error, name_on_error) = (app.clone(), file_name.clone());
    let spawned = std::thread::Builder::new()
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
        });
    if let Err(e) = spawned {
        // JOB already says Decoding: leaving it there would refuse every later
        // file and hold back the updater (file_job_running) for good.
        let error = format!("could not start the file transcription: {e}");
        publish(
            &app_on_error,
            FileJob::Failed {
                file_name: name_on_error,
                error: error.clone(),
            },
        );
        return Err(error);
    }
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

/// One History re-transcription at a time.
static RETRANSCRIBING: AtomicBool = AtomicBool::new(false);

/// Clears RETRANSCRIBING however the job ends.
struct RetranscribeGuard;

impl Drop for RetranscribeGuard {
    fn drop(&mut self) {
        RETRANSCRIBING.store(false, Ordering::SeqCst);
    }
}

/// History's "Transcribe again": reads a take's recording again with the
/// dictation model and replaces the row's transcript - for a take recorded
/// without a model, or whose transcription failed. Nothing is pasted or
/// post-processed; the recording is only read. Returns the new text.
#[tauri::command]
#[specta::specta]
pub async fn retranscribe_history_entry(app: AppHandle, id: i64) -> Result<String, String> {
    let history = Arc::clone(
        app.state::<Arc<crate::managers::history::HistoryManager>>()
            .inner(),
    );
    let entry = history
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("the history entry no longer exists")?;
    let model = get_settings(&app).selected_model;
    let model_info = app
        .state::<Arc<ModelManager>>()
        .get_model_info(&model)
        .filter(|m| m.is_downloaded)
        .ok_or("no speech model is ready - download one first")?;
    let files = history
        .recording_files_for(&entry.file_name)
        .map_err(|e| e.to_string())?;
    // Claim first, then look for an update: the updater claims UPDATE_PENDING
    // first and then reads file_job_running() (which sees this flag) - both
    // SeqCst, so at least one of the two sees the other and backs off.
    if RETRANSCRIBING.swap(true, Ordering::SeqCst) {
        return Err("another recording is being transcribed".into());
    }
    let guard = RetranscribeGuard;
    if crate::transcription_coordinator::update_pending() {
        return Err("an update is being installed; try again after the restart".into());
    }

    let app_job = app.clone();
    let model_job = model.clone();
    let (text, diarized) = tauri::async_runtime::spawn_blocking(
        move || -> Result<(String, Option<String>), String> {
            let mut samples = Vec::new();
            for file in &files {
                samples.extend(decode_audio_file(file).map_err(|e| e.to_string())?);
            }
            let external = model_info.engine_type.is_external();
            let tm = app_job.state::<Arc<TranscriptionManager>>();
            info!(
                "History: transcribing entry {id} again with {model_job} ({:.0} s)",
                samples.len() as f64 / SAMPLE_HZ
            );
            // A Custom STT server reads the whole recording at once, so it can
            // tell the speakers apart across it.
            if matches!(
                model_info.engine_type,
                crate::managers::model::EngineType::SttServer
            ) {
                let result = tm.stt_server_take(&samples).map_err(|e| e.to_string())?;
                return Ok((result.text, result.diarized));
            }
            let mut parts = Vec::new();
            for range in split_speech_segments(&samples) {
                let piece = samples[range].to_vec();
                let text = transcribe_piece(&app_job, &tm, &model_job, external, piece)?;
                let text = text.trim();
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
            release_model(&app_job, &model_job);
            Ok((parts.join(" "), None))
        },
    )
    .await
    .map_err(|e| e.to_string())??;

    let label = app
        .state::<Arc<ModelManager>>()
        .get_model_info(&model)
        .map(|m| m.name);
    // The row and the take's text file change together (see
    // HistoryManager::replace_transcription).
    history
        .replace_transcription(id, &text, label, diarized.as_deref())
        .map_err(|e| e.to_string())?;
    // Held until the text is in the database, so an update waits for it.
    drop(guard);
    info!(
        "History: entry {id} transcribed again ({} chars)",
        text.len()
    );
    Ok(text)
}

/// What "Transcribe again with speakers" did.
#[derive(Serialize, Clone, Debug, Type)]
pub struct SpeakersResult {
    /// The new plain text.
    pub text: String,
    /// The server told two or more speakers apart (else: one voice, no
    /// speaker text).
    pub speakers: bool,
}

/// Why the speakers request cannot run, as `CODE` or `CODE|detail` - the app
/// turns it into a message with a way to fix it:
/// `STT_SERVER_NOT_SET`, `STT_SPEAKERS_MODEL_NOT_SET`,
/// `STT_SERVER_UNREACHABLE|<error>`, `STT_SPEAKERS_MODEL_MISSING|<model>|<models it has>`.
fn speakers_preflight(
    settings: &AppSettings,
    info: Result<crate::managers::stt_server::SttServerInfo, String>,
) -> Result<(), String> {
    let model = settings.stt_server_speakers_model.trim();
    if model.is_empty() {
        return Err("STT_SPEAKERS_MODEL_NOT_SET".into());
    }
    let info = info.map_err(|e| format!("STT_SERVER_UNREACHABLE|{e}"))?;
    // A server that lists models must list this one.
    if !info.models.is_empty() && !info.models.iter().any(|m| m.id == model) {
        let available = info
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("STT_SPEAKERS_MODEL_MISSING|{model}|{available}"));
    }
    Ok(())
}

/// History's "Transcribe again with speakers": the recording, whole, to the
/// Custom STT server's speakers model (whatever the dictation model is). The
/// row gets the plain text and the text with speakers.
#[tauri::command]
#[specta::specta]
pub async fn retranscribe_history_entry_speakers(
    app: AppHandle,
    id: i64,
) -> Result<SpeakersResult, String> {
    let settings = get_settings(&app);
    if settings.stt_server_url.trim().is_empty() {
        return Err("STT_SERVER_NOT_SET".into());
    }
    let (url, key) = (
        settings.stt_server_url.clone(),
        settings.stt_server_key.clone(),
    );
    let info = tauri::async_runtime::spawn_blocking(move || {
        crate::managers::stt_server::list_models(&url, &key).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    speakers_preflight(&settings, info)?;

    let history = Arc::clone(
        app.state::<Arc<crate::managers::history::HistoryManager>>()
            .inner(),
    );
    let entry = history
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("the history entry no longer exists")?;
    let files = history
        .recording_files_for(&entry.file_name)
        .map_err(|e| e.to_string())?;
    if RETRANSCRIBING.swap(true, Ordering::SeqCst) {
        return Err("another recording is being transcribed".into());
    }
    let guard = RetranscribeGuard;
    if crate::transcription_coordinator::update_pending() {
        return Err("an update is being installed; try again after the restart".into());
    }
    let app_job = app.clone();
    let settings_job = settings.clone();
    let result = tauri::async_runtime::spawn_blocking(
        move || -> Result<crate::managers::stt_server::SttServerResult, String> {
            let mut samples = Vec::new();
            for file in &files {
                samples.extend(decode_audio_file(file).map_err(|e| e.to_string())?);
            }
            info!(
                "History: transcribing entry {id} again with speakers ({:.0} s)",
                samples.len() as f64 / SAMPLE_HZ
            );
            app_job
                .state::<Arc<TranscriptionManager>>()
                .stt_server_speakers_direct(&settings_job, &samples)
                .map_err(|e| e.to_string())
        },
    )
    .await
    .map_err(|e| e.to_string())??;
    let label = Some(format!(
        "{} — STT server",
        settings.stt_server_speakers_model.trim()
    ));
    history
        .replace_transcription(id, &result.text, label, result.diarized.as_deref())
        .map_err(|e| e.to_string())?;
    drop(guard);
    info!(
        "History: entry {id} transcribed again with speakers ({} chars, speakers: {})",
        result.text.len(),
        result.diarized.is_some()
    );
    Ok(SpeakersResult {
        speakers: result.diarized.is_some(),
        text: result.text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speakers_preflight_names_what_is_missing() {
        use crate::managers::stt_server::{SttServerInfo, SttServerModel};
        let mut s = crate::settings::get_default_settings();
        let info = |ids: &[&str]| -> Result<SttServerInfo, String> {
            Ok(SttServerInfo {
                models: ids
                    .iter()
                    .map(|id| SttServerModel {
                        id: id.to_string(),
                        description: String::new(),
                    })
                    .collect(),
                backend: None,
            })
        };
        assert_eq!(
            speakers_preflight(&s, info(&["transcribe", "transcribe-diarize-openai"])),
            Ok(())
        );
        // A server that does not list models is trusted.
        assert_eq!(speakers_preflight(&s, info(&[])), Ok(()));
        assert_eq!(
            speakers_preflight(&s, info(&["transcribe"])),
            Err("STT_SPEAKERS_MODEL_MISSING|transcribe-diarize-openai|transcribe".into())
        );
        assert_eq!(
            speakers_preflight(&s, Err("refused".into())),
            Err("STT_SERVER_UNREACHABLE|refused".into())
        );
        s.stt_server_speakers_model = " ".into();
        assert_eq!(
            speakers_preflight(&s, info(&[])),
            Err("STT_SPEAKERS_MODEL_NOT_SET".into())
        );
    }

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

    #[test]
    fn the_audio_copy_goes_to_the_name_that_was_checked() {
        let dir = tempfile::TempDir::new().unwrap();
        // An unrelated recording whose name a dot in the stem could collapse to.
        std::fs::write(dir.path().join("meeting.m4a"), "keep me").unwrap();
        let source = Path::new("C:/phone/meeting.2026.m4a");
        let stem = unique_stem(dir.path(), source, true);
        assert_eq!(stem, "meeting.2026");
        let out = audio_copy_path(dir.path(), &stem, source);
        assert_eq!(out, dir.path().join("meeting.2026.m4a"));
        assert!(
            !out.exists(),
            "the copy must go to a name unique_stem found free"
        );
        assert_eq!(
            audio_copy_path(dir.path(), "notes", Path::new("C:/x/notes")),
            dir.path().join("notes")
        );
    }
}
