use chrono::{
    Duration as ChronoDuration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc,
};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::{Mutex as AsyncMutex, Notify};

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const SCHEDULER_POLL: Duration = Duration::from_secs(60);
const RELEASES_URL: &str = "https://github.com/patrick-1984/handy-tool/releases";

#[derive(Debug, Clone, Serialize, Type)]
pub struct UpdaterStatus {
    pub state: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub progress_percent: Option<u8>,
    pub waiting_for_idle: bool,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub last_checked_at: Option<String>,
    pub releases_url: String,
}

impl UpdaterStatus {
    fn state(state: &str) -> Self {
        Self {
            state: state.to_string(),
            version: None,
            notes: None,
            downloaded_bytes: None,
            total_bytes: None,
            progress_percent: None,
            waiting_for_idle: false,
            error_code: None,
            error_detail: None,
            last_checked_at: None,
            releases_url: RELEASES_URL.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct SchedulerState {
    schedule_date: Option<String>,
    scheduled_offset_minutes: i16,
    schedule_signature: String,
    last_scheduled_attempt_date: Option<String>,
    last_scheduled_attempt_at: Option<String>,
    last_successful_check_at: Option<String>,
    last_seen_version: Option<String>,
    pending_update_from: Option<String>,
    pending_update_to: Option<String>,
    pending_update_at: Option<String>,
}

#[derive(Clone)]
struct PreparedUpdate {
    update: Update,
    bytes: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct UpdateManager {
    inner: Arc<UpdateManagerInner>,
}

struct UpdateManagerInner {
    app: AppHandle,
    status: Arc<Mutex<UpdaterStatus>>,
    prepared: AsyncMutex<Option<PreparedUpdate>>,
    operation: AsyncMutex<()>,
    last_operation_finished: Mutex<Option<Instant>>,
    schedule: Mutex<SchedulerState>,
    notify: Notify,
}

impl UpdateManager {
    pub fn new(app: AppHandle) -> Self {
        let schedule = load_scheduler_state(&app);
        let manager = Self {
            inner: Arc::new(UpdateManagerInner {
                app,
                status: Arc::new(Mutex::new(UpdaterStatus::state("idle"))),
                prepared: AsyncMutex::new(None),
                operation: AsyncMutex::new(()),
                last_operation_finished: Mutex::new(None),
                schedule: Mutex::new(schedule),
                notify: Notify::new(),
            }),
        };
        manager.clear_completed_attempt_marker();
        manager
    }

    pub fn start(&self) {
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                manager.scheduler_tick().await;
                tokio::select! {
                    _ = tokio::time::sleep(SCHEDULER_POLL) => {},
                    _ = manager.inner.notify.notified() => {},
                }
            }
        });
    }

    pub fn notify_schedule_changed(&self) {
        self.inner.notify.notify_one();
    }

    fn status(&self) -> UpdaterStatus {
        self.inner
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn publish(&self, status: UpdaterStatus) {
        *self
            .inner
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = status.clone();
        let _ = self.inner.app.emit("updater-status", status);
    }

    fn fail(&self, code: &str, detail: impl ToString) -> String {
        let mut detail = detail.to_string();
        if detail.len() > 400 {
            detail.truncate(400);
        }
        warn!("Updater {code}: {detail}");
        let mut status = UpdaterStatus::state("failed");
        status.error_code = Some(code.to_string());
        status.error_detail = Some(detail.clone());
        status.last_checked_at = self
            .inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last_successful_check_at
            .clone();
        self.publish(status);
        detail
    }

    async fn check(
        &self,
        automatic: bool,
        install_in_window: Option<NaiveDateTime>,
    ) -> Result<UpdaterStatus, String> {
        let _operation = self.inner.operation.lock().await;

        // A manual click arriving on the heels of the scheduler shares its
        // result instead of issuing a second request or racing an install.
        if self
            .inner
            .last_operation_finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some_and(|finished| finished.elapsed() < Duration::from_secs(5))
        {
            return Ok(self.status());
        }

        if !automatic {
            self.publish(UpdaterStatus::state("checking"));
        }
        let updater = match self
            .inner
            .app
            .updater_builder()
            .timeout(CHECK_TIMEOUT)
            .on_before_exit(crate::mcp::stop)
            .build()
        {
            Ok(updater) => updater,
            Err(error) if automatic => {
                let detail = error.to_string();
                warn!("Scheduled updater configuration error: {detail}");
                return Err(detail);
            }
            Err(error) => return Err(self.fail("configuration", error)),
        };

        let checked = match updater.check().await {
            Ok(update) => Ok(update),
            Err(error) if automatic => {
                let detail = error.to_string();
                warn!("Scheduled update check failed: {detail}");
                Err(detail)
            }
            Err(error) => Err(self.fail("check_failed", error)),
        };

        let now = Utc::now().to_rfc3339();
        match checked {
            Ok(Some(mut update)) => {
                update.timeout = Some(DOWNLOAD_TIMEOUT);
                let version = update.version.clone();
                let notes = update.body.clone();
                *self.inner.prepared.lock().await = Some(PreparedUpdate {
                    update,
                    bytes: None,
                });
                self.update_successful_check(&now, Some(&version), !automatic);
                let mut status = UpdaterStatus::state("available");
                status.version = Some(version);
                status.notes = notes;
                status.last_checked_at = Some(now);
                self.publish(status);

                if automatic && install_in_window.is_some() {
                    let settings = crate::settings::get_settings(&self.inner.app);
                    if settings.automatic_update_checks
                        && settings.automatic_silent_updates
                        && in_place_updates_supported()
                    {
                        if let Err(error) = self
                            .download_and_install_locked(true, install_in_window)
                            .await
                        {
                            warn!("Scheduled update deferred or failed: {error}");
                        }
                    }
                }
            }
            Ok(None) => {
                *self.inner.prepared.lock().await = None;
                self.update_successful_check(&now, None, !automatic);
                let mut status = UpdaterStatus::state("idle");
                status.last_checked_at = Some(now);
                self.publish(status);
            }
            Err(error) => {
                *self
                    .inner
                    .last_operation_finished
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
                return Err(error);
            }
        }
        *self
            .inner
            .last_operation_finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
        Ok(self.status())
    }

    async fn install(&self) -> Result<UpdaterStatus, String> {
        let _operation = self.inner.operation.lock().await;
        self.download_and_install_locked(false, None).await?;
        Ok(self.status())
    }

    async fn download_and_install_locked(
        &self,
        scheduled: bool,
        window_end: Option<NaiveDateTime>,
    ) -> Result<(), String> {
        if crate::portable::portable_marker_present() {
            let mut status = UpdaterStatus::state("unsupported");
            status.error_code = Some("portable_mode".to_string());
            status.error_detail = Some(
                "Portable copies cannot be updated in place. Download the portable release instead."
                    .to_string(),
            );
            self.publish(status);
            return Err("In-place updates are disabled in portable mode".to_string());
        }

        #[cfg(not(windows))]
        {
            let mut status = UpdaterStatus::state("unsupported");
            status.error_code = Some("nsis_only".to_string());
            status.error_detail =
                Some("Silent in-place updates currently use the Windows NSIS channel.".to_string());
            self.publish(status);
            return Err("Silent updates are currently Windows-only".to_string());
        }

        let prepared = self
            .inner
            .prepared
            .lock()
            .await
            .clone()
            .ok_or_else(|| self.fail("no_update", "Check for updates before installing"))?;
        let version = prepared.update.version.clone();
        let notes = prepared.update.body.clone();

        let bytes = if let Some(bytes) = prepared.bytes {
            bytes
        } else {
            let mut status = UpdaterStatus::state("downloading");
            status.version = Some(version.clone());
            status.notes = notes.clone();
            status.downloaded_bytes = Some(0);
            status.progress_percent = Some(0);
            self.publish(status);

            let status_slot = self.inner.status.clone();
            let app = self.inner.app.clone();
            let progress_version = version.clone();
            let progress_notes = notes.clone();
            let mut downloaded = 0u64;
            let mut last_emit = Instant::now() - Duration::from_secs(1);
            let mut last_percent = 0u8;
            let bytes = prepared
                .update
                .download(
                    move |chunk, total| {
                        downloaded = downloaded.saturating_add(chunk as u64);
                        let percent = total
                            .filter(|total| *total > 0)
                            .map(|total| ((downloaded.saturating_mul(100) / total).min(100)) as u8);
                        let changed_percent = percent.is_some_and(|value| value > last_percent);
                        if changed_percent || last_emit.elapsed() >= Duration::from_millis(250) {
                            if let Some(value) = percent {
                                last_percent = value;
                            }
                            last_emit = Instant::now();
                            let mut progress = UpdaterStatus::state("downloading");
                            progress.version = Some(progress_version.clone());
                            progress.notes = progress_notes.clone();
                            progress.downloaded_bytes = Some(downloaded);
                            progress.total_bytes = total;
                            progress.progress_percent = percent;
                            *status_slot
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                progress.clone();
                            let _ = app.emit("updater-status", progress);
                        }
                    },
                    || {},
                )
                .await
                .map_err(|error| self.fail("download_failed", error))?;

            let mut ready = UpdaterStatus::state("ready_to_restart");
            ready.version = Some(version.clone());
            ready.notes = notes.clone();
            ready.downloaded_bytes = Some(bytes.len() as u64);
            ready.progress_percent = Some(100);
            self.publish(ready);
            *self.inner.prepared.lock().await = Some(PreparedUpdate {
                update: prepared.update.clone(),
                bytes: Some(bytes.clone()),
            });
            bytes
        };

        loop {
            if scheduled && window_end.is_some_and(|end| Local::now().naive_local() > end) {
                info!("Silent-update window closed while waiting for an idle pipeline; deferring");
                return Ok(());
            }

            let coordinator = self.inner.app.state::<crate::TranscriptionCoordinator>();
            if coordinator.try_reserve_for_update() {
                // The reservation is authoritative; this second read is the
                // mandatory immediate pre-install belt-and-suspenders check.
                if crate::transcription_coordinator::pipeline_stage()
                    == crate::transcription_coordinator::STAGE_IDLE
                    && crate::transcription_coordinator::update_pending()
                {
                    break;
                }
                coordinator.release_update_reservation();
            }

            let mut waiting = UpdaterStatus::state("ready_to_restart");
            waiting.version = Some(version.clone());
            waiting.notes = notes.clone();
            waiting.waiting_for_idle = true;
            self.publish(waiting);
            tokio::time::sleep(if scheduled {
                Duration::from_secs(15)
            } else {
                Duration::from_secs(1)
            })
            .await;
        }

        self.persist_attempt_marker(&version);
        let mut installing = UpdaterStatus::state("installing");
        installing.version = Some(version);
        self.publish(installing);

        if crate::transcription_coordinator::pipeline_stage()
            != crate::transcription_coordinator::STAGE_IDLE
            || !crate::transcription_coordinator::update_pending()
        {
            self.inner
                .app
                .state::<crate::TranscriptionCoordinator>()
                .release_update_reservation();
            return Err(self.fail(
                "pipeline_became_busy",
                "The transcription pipeline was no longer idle immediately before installation",
            ));
        }

        // On Windows this launches NSIS with Tauri's silent update/restart
        // arguments and exits the current process only after launch succeeds.
        if let Err(error) = prepared.update.install(&bytes) {
            self.inner
                .app
                .state::<crate::TranscriptionCoordinator>()
                .release_update_reservation();
            return Err(self.fail("install_failed", error));
        }
        Ok(())
    }

    async fn scheduler_tick(&self) {
        let settings = crate::settings::get_settings(&self.inner.app);
        if !settings.automatic_update_checks {
            if self.inner.prepared.lock().await.is_none() {
                self.publish(UpdaterStatus::state("disabled"));
            }
            return;
        }

        let now = Local::now().naive_local();
        let center_minutes =
            parse_center_minutes(&settings.silent_update_time_local).unwrap_or(240);
        let jitter = settings.silent_update_jitter_minutes.min(180) as i64;
        let (schedule_date, window_start, window_end) = window_for_now(now, center_minutes, jitter);
        let signature = format!("{}:{}", settings.silent_update_time_local, jitter);
        let date_key = schedule_date.to_string();
        let offset = self.ensure_daily_offset(&date_key, &signature, jitter as i16);
        let scheduled_at = resolve_local_time(
            center_for_date(schedule_date, center_minutes) + ChronoDuration::minutes(offset as i64),
        );

        let already_attempted = self
            .inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last_scheduled_attempt_date
            .as_deref()
            == Some(date_key.as_str());

        if now >= window_start && now <= window_end && now >= scheduled_at {
            if already_attempted {
                // A manual check earlier today satisfies the network check,
                // but a discovered update must still be applied in its
                // allowed silent window.
                if settings.automatic_silent_updates
                    && in_place_updates_supported()
                    && self.inner.prepared.lock().await.is_some()
                {
                    let _operation = self.inner.operation.lock().await;
                    let _ = self
                        .download_and_install_locked(true, Some(window_end))
                        .await;
                }
                return;
            }
            self.mark_scheduled_attempt(&date_key);
            let install_window = (settings.automatic_silent_updates
                && in_place_updates_supported())
            .then_some(window_end);
            let _ = self.check(true, install_window).await;
            return;
        }

        // If the app was asleep or closed through the whole window, do one
        // stale check after it returns, but never install outside the window.
        let todays_end = resolve_local_time(
            center_for_date(now.date(), center_minutes) + ChronoDuration::minutes(jitter),
        );
        if now > todays_end && !already_attempted && self.last_successful_check_is_stale() {
            let today_key = now.date().to_string();
            self.mark_scheduled_attempt(&today_key);
            let _ = self.check(true, None).await;
        }
    }

    fn ensure_daily_offset(&self, date: &str, signature: &str, jitter: i16) -> i16 {
        let mut state = self
            .inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.schedule_date.as_deref() != Some(date) || state.schedule_signature != signature {
            let span = (jitter as i32 * 2 + 1).max(1) as u32;
            let bytes = uuid::Uuid::new_v4().into_bytes();
            let sample = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            state.scheduled_offset_minutes = (sample % span) as i16 - jitter;
            state.schedule_date = Some(date.to_string());
            state.schedule_signature = signature.to_string();
            persist_scheduler_state(&self.inner.app, &state);
        }
        state.scheduled_offset_minutes
    }

    fn mark_scheduled_attempt(&self, date: &str) {
        let mut state = self
            .inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.last_scheduled_attempt_date = Some(date.to_string());
        state.last_scheduled_attempt_at = Some(Utc::now().to_rfc3339());
        // Persist before network access: a crash or persistent proxy failure
        // cannot turn into a retry loop.
        persist_scheduler_state(&self.inner.app, &state);
    }

    fn update_successful_check(&self, at: &str, version: Option<&str>, manual: bool) {
        let mut state = self
            .inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.last_successful_check_at = Some(at.to_string());
        if manual {
            state.last_scheduled_attempt_date = Some(Local::now().date_naive().to_string());
        }
        if let Some(version) = version {
            state.last_seen_version = Some(version.to_string());
        }
        persist_scheduler_state(&self.inner.app, &state);
    }

    fn last_successful_check_is_stale(&self) -> bool {
        self.inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last_successful_check_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .is_none_or(|last| {
                Utc::now().signed_duration_since(last.with_timezone(&Utc))
                    >= ChronoDuration::hours(24)
            })
    }

    fn persist_attempt_marker(&self, to_version: &str) {
        let mut state = self
            .inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending_update_from = Some(self.inner.app.package_info().version.to_string());
        state.pending_update_to = Some(to_version.to_string());
        state.pending_update_at = Some(Utc::now().to_rfc3339());
        persist_scheduler_state(&self.inner.app, &state);
    }

    fn clear_completed_attempt_marker(&self) {
        let current = self.inner.app.package_info().version.to_string();
        let mut state = self
            .inner
            .schedule
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.pending_update_to.as_deref() == Some(current.as_str()) {
            state.pending_update_from = None;
            state.pending_update_to = None;
            state.pending_update_at = None;
            persist_scheduler_state(&self.inner.app, &state);
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_updater_status(manager: tauri::State<'_, UpdateManager>) -> UpdaterStatus {
    manager.status()
}

#[tauri::command]
#[specta::specta]
pub async fn check_for_updates(
    manager: tauri::State<'_, UpdateManager>,
) -> Result<UpdaterStatus, String> {
    manager.check(false, None).await
}

#[tauri::command]
#[specta::specta]
pub async fn install_available_update(
    manager: tauri::State<'_, UpdateManager>,
) -> Result<UpdaterStatus, String> {
    manager.install().await
}

fn parse_center_minutes(value: &str) -> Option<u32> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}

fn in_place_updates_supported() -> bool {
    cfg!(windows) && !crate::portable::portable_marker_present()
}

fn center_for_date(date: NaiveDate, center_minutes: u32) -> NaiveDateTime {
    date.and_hms_opt(center_minutes / 60, center_minutes % 60, 0)
        .expect("validated local update time")
}

fn resolve_local_time(mut candidate: NaiveDateTime) -> NaiveDateTime {
    // A configured wall-clock minute can disappear during the spring DST jump.
    // Advance to the first real local minute. During the autumn repeat, the
    // persisted daily attempt marker ensures the wall-clock minute runs once.
    for _ in 0..=24 * 60 {
        match Local.from_local_datetime(&candidate) {
            LocalResult::Single(_) | LocalResult::Ambiguous(_, _) => return candidate,
            LocalResult::None => candidate += ChronoDuration::minutes(1),
        }
    }

    candidate
}

fn window_for_now(
    now: NaiveDateTime,
    center_minutes: u32,
    jitter: i64,
) -> (NaiveDate, NaiveDateTime, NaiveDateTime) {
    for delta in [-1i64, 0, 1] {
        let date = now.date() + ChronoDuration::days(delta);
        let center = center_for_date(date, center_minutes);
        let start = resolve_local_time(center - ChronoDuration::minutes(jitter));
        let end = resolve_local_time(center + ChronoDuration::minutes(jitter));
        if now >= start && now <= end {
            return (date, start, end);
        }
    }
    let date = now.date();
    let center = center_for_date(date, center_minutes);
    (
        date,
        resolve_local_time(center - ChronoDuration::minutes(jitter)),
        resolve_local_time(center + ChronoDuration::minutes(jitter)),
    )
}

fn scheduler_state_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    crate::portable::resolve_app_data_dir(app)
        .ok()
        .map(|dir| dir.join("updater_state.json"))
}

fn load_scheduler_state(app: &AppHandle) -> SchedulerState {
    scheduler_state_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn persist_scheduler_state(app: &AppHandle, state: &SchedulerState) {
    let Some(path) = scheduler_state_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            warn!("Could not create updater-state directory: {error}");
            return;
        }
    }
    match serde_json::to_vec_pretty(state) {
        Ok(json) => {
            if let Err(error) = std::fs::write(path, json) {
                warn!("Could not persist updater schedule state: {error}");
            }
        }
        Err(error) => warn!("Could not serialize updater schedule state: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_center_time() {
        assert_eq!(parse_center_minutes("04:00"), Some(240));
        assert_eq!(parse_center_minutes("23:59"), Some(1439));
        assert_eq!(parse_center_minutes("24:00"), None);
    }

    #[test]
    fn window_handles_midnight() {
        let now = NaiveDate::from_ymd_opt(2026, 8, 6)
            .unwrap()
            .and_hms_opt(23, 50, 0)
            .unwrap();
        let (date, start, end) = window_for_now(now, 10, 30);
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 8, 7).unwrap());
        assert!(now >= start && now <= end);
    }
}
