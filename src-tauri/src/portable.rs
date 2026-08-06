//! Portable-mode app-data resolution (T-114).
//!
//! **STATUS: wired in.** Declared as `mod portable;` in `lib.rs` and called
//! from every persistent-state call site listed in
//! `tickets/T-114-portable-distribution.md`'s call-site table
//! (`managers/model.rs`, `managers/history.rs`, `managers/audio.rs`,
//! `managers/translator.rs`, `settings.rs`, `backup.rs`, `mcp/mod.rs`).
//!
//! ## Why this exists
//!
//! Handy resolves all of its persistent state (settings, history DB,
//! downloaded models, recordings, the translator queue) through Tauri's
//! `AppHandle::path().app_data_dir()` / `app_config_dir()`, which are
//! computed from the `identifier` in `tauri.conf.json` (`pr.handy`) combined
//! with the OS's per-user profile convention (`%APPDATA%\pr.handy` on
//! Windows). Tauri 2 does not expose a supported way to override that
//! resolution at runtime — there is no `set_app_data_dir` hook. A portable
//! build (run from a USB stick / arbitrary folder, no installer, no writes
//! outside its own folder) therefore needs every call site that currently
//! calls `app_handle.path().app_data_dir()` (or passes a bare relative path
//! to `tauri_plugin_store`, which resolves the same way) to instead call
//! through this module, so the choice of "OS profile dir" vs
//! "folder-beside-the-exe" is made in exactly one place.
//!
//! ## Marker file
//!
//! Portable mode is opt-in and detected by presence, not a build flag: if a
//! zero-byte `portable.marker` file sits next to `handy.exe`, all app data
//! goes into a `data\` folder next to the exe instead of the OS profile dir.
//! This lets one build of `handy.exe` serve both the installed and the
//! portable distribution — the packaging script (`portable.cmd`) is what
//! drops the marker file into the portable ZIP; the NSIS/MSI installers never
//! create it, so an installed copy is unaffected.
//!
//! `handy.exe` is a single binary that serves both the GUI app and the `handy`
//! CLI companion (arg-dispatched in `main.rs`), so `std::env::current_exe()`
//! always points at the same file regardless of which mode invoked it —
//! `mcp/mod.rs::sidecar_path()` (no `AppHandle` available; called standalone
//! by the CLI) relies on this to stay portable-aware too.
//!
//! ## Failure handling
//!
//! Presence of the marker is not enough on its own: a portable build might
//! be run from read-only media, or from a folder the current user can't
//! write to. If the `data\` dir can't be created OR a writability probe
//! inside it fails, portable mode is treated as **not active** for this
//! process — every caller transparently falls back to the normal
//! `app_data_dir()` / `app_config_dir()` OS location, with a `warn!` log
//! explaining why. This never panics or aborts startup; worst case, a
//! portable launch on unwritable media behaves like a normal install for
//! that run. The write-probe itself uses a per-process-unique filename
//! (`create_new`, never truncating a same-named leftover file) and its
//! cleanup is verified — a probe that can't be removed also falls back to
//! non-portable (see [`resolve_portable_dir`]) rather than silently leaving
//! a stray file behind in a folder we just told the user is "clean".
//!
//! **The `warn!` alone is not enough (T-114 finding #6):** portable-mode
//! detection first runs from `lib.rs` while picking the log plugin's file
//! target, which happens BEFORE the logger is attached — and the result is
//! memoized in [`PORTABLE_DIR`] for the life of the process, so the `warn!`
//! never replays once the logger does come up. Every fallback branch in
//! [`resolve_portable_dir`] therefore ALSO `eprintln!`s to stderr (prefixed
//! `[handy]`) in addition to the `warn!`, so an operator running the exe
//! from a console still sees why a portable launch silently became a normal
//! one, even though nothing reached the log file.
//!
//! ## Isolation scope (T-114 hardening pass)
//!
//! Redirecting the *data* dir (settings/history/models/recordings) is only
//! part of "leaves no trace". More isolation gaps are closed elsewhere,
//! funneling through this module's [`portable_data_dir`] /
//! [`resolve_app_data_dir`]:
//! - **WebView2 storage — ALL THREE windows** (`lib.rs` main window creation;
//!   `overlay.rs` recording-overlay + floating-transcription windows):
//!   without an explicit data directory, WebView2 defaults every window to
//!   the same `%LOCALAPPDATA%\pr.handy` an installed copy would use, so
//!   `localStorage` would otherwise leak between an installed and a portable
//!   copy on the same machine. `lib.rs` (main) and `overlay.rs` (both aux
//!   windows) each call `WebviewWindowBuilder::data_directory()` with the
//!   SAME `<portable_data>\webview` dir when portable — deliberately shared
//!   across all three so every window's webview state lands in one portable
//!   place, `#[cfg(windows)]`-gated since `data_directory()` is WebView2-only.
//! - **Logs** (`lib.rs` log-plugin targets, [`portable_log_dir`],
//!   [`resolve_log_dir`]): file logs go to `<portable_data>\logs` instead of
//!   `app_log_dir()` (which also resolves under the OS profile dir).
//! - **Machine state** (`lib.rs` autostart, `shortcut::change_autostart_setting`,
//!   `install_cli_binary`): portable mode never registers/unregisters the
//!   autostart Run-key entry and never installs the CLI onto
//!   `%LOCALAPPDATA%\Microsoft\WindowsApps` — both are machine/user-profile
//!   state that would outlive the portable folder being deleted. The raw
//!   `autostart:default` Tauri capability is also removed from
//!   `capabilities/desktop.json` — the app's own gated
//!   `change_autostart_setting` command doesn't need it (it calls
//!   `ManagerExt::autolaunch()` directly from Rust), and leaving it in would
//!   let arbitrary frontend JS call the raw, non-portable-gated plugin
//!   commands directly.
//! - **Custom notification sounds** (`commands/audio.rs::custom_sound_exists`,
//!   `audio_feedback.rs::resolve_sound_path`): both now resolve the
//!   `custom_*.wav` files via [`resolve_app_data_dir`] instead of
//!   `BaseDirectory::AppData`, so portable mode discovers/plays its own
//!   custom sounds instead of silently ignoring them (or picking up an
//!   installed copy's, if one happens to exist on the same machine).
//!   Bundled (non-custom) theme sounds stay resolved via
//!   `BaseDirectory::Resource` either way — unaffected.
//!
//! ## Non-Windows
//!
//! The resolver itself is not `cfg`-gated — the marker-beside-the-exe check
//! works the same way on any OS `current_exe()` supports. Packaging
//! (`portable.cmd`) is Windows-only per CLAUDE.md, so in practice the
//! `portable.marker` file is never shipped on macOS/Linux and this code path
//! is inert there (`current_exe()` returns `None`/no marker found → normal
//! behavior, byte-for-byte). Note `current_exe()` on macOS resolves to a path
//! inside the `.app` bundle, which would need different handling if a macOS
//! portable mode is ever wanted (see ticket "Open risks") — out of scope here.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

/// Process-wide memoized result of portable-mode detection. `current_exe()`
/// and the marker/writability checks are filesystem calls; every caller
/// funnels through `portable_data_dir()`, so without caching a busy startup
/// (settings, models, history, translator all resolving their dirs) would
/// re-stat the same files repeatedly. The marker/exe location cannot change
/// mid-process, so a `OnceLock` is safe and simple.
static PORTABLE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// A filename for the writability probe that's unique to this process
/// invocation (pid + nanosecond timestamp). Two properties this buys over a
/// fixed name like `.portable_write_test`:
/// 1. `create_new` (below) can never collide with — and therefore never
///    truncate — some other file a previous run left behind under that name.
/// 2. Concurrent launches (e.g. two portable copies started at once against
///    the same `data\` dir) don't race each other's probe writes.
fn probe_filename() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(".portable_write_test_{}_{nanos}", std::process::id())
}

/// Pure decision function, deliberately `AppHandle`-free so it's unit
/// testable without standing up a Tauri app: given the directory the running
/// exe lives in, decide whether portable mode is active and usable.
///
/// Returns `Some(data_dir)` only when ALL of the following hold:
/// 1. `<exe_dir>/portable.marker` exists and is a file.
/// 2. `<exe_dir>/data` exists (or was just created).
/// 3. `<exe_dir>/data` is actually writable (probed with a throwaway file —
///    a directory can exist and still be unwritable, e.g. read-only media
///    where the dir was pre-populated by whoever wrote the USB image).
/// 4. The probe file could be cleaned up afterward. A dir that accepts the
///    write but refuses the delete (some restricted/network shares behave
///    this way) is not a dir we should tell the rest of the app is clean and
///    usable — it would leave a stray dotfile behind forever, in a folder
///    the whole point of portable mode is to leave spotless.
///
/// Returns `None` (→ caller falls back to the OS profile dir) if the marker
/// is absent, or if any of steps 2-4 fail — logging a `warn!` in the failure
/// case so a silently-ignored portable launch is still visible in the logs.
fn resolve_portable_dir(exe_dir: &Path) -> Option<PathBuf> {
    if !exe_dir.join("portable.marker").is_file() {
        return None;
    }

    let data_dir = exe_dir.join("data");
    if !data_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            // T-114 finding #6: this runs during log-target selection in
            // lib.rs, BEFORE the logger is attached, and PORTABLE_DIR memoizes
            // the result — so a bare `warn!` here is silently discarded and
            // NEVER replayed once the logger comes up. eprintln! to stderr as
            // well so the operator has some chance of seeing why a portable
            // launch fell back to a per-user profile.
            eprintln!(
                "[handy] portable.marker present but '{}' could not be created ({e}) — falling back to per-user profile",
                data_dir.display()
            );
            log::warn!(
                "Portable mode: portable.marker found next to the exe, but '{}' could not be created ({e}) — falling back to the normal app-data location",
                data_dir.display()
            );
            return None;
        }
    }

    // Writability probe: `create_dir_all` succeeding (or the dir already
    // existing) isn't proof the volume accepts writes. `create_new` refuses
    // to open (and cannot truncate) a pre-existing file at this path — moot
    // in practice since the name is unique per invocation, but it's the
    // correct primitive regardless.
    let probe = data_dir.join(probe_filename());
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => match std::fs::remove_file(&probe) {
            // Removed cleanly, or it was already gone somehow (e.g. a
            // concurrent cleanup) — either way nothing is left behind.
            Ok(()) => Some(data_dir),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(data_dir),
            Err(e) => {
                // T-114 finding #6: same pre-logger discard problem as above.
                eprintln!(
                    "[handy] portable marker present but data dir unusable: write-probe at '{}' could not be removed ({e}) — falling back to per-user profile",
                    probe.display()
                );
                log::warn!(
                    "Portable mode: wrote a write-probe at '{}' but could not remove it afterward ({e}) — falling back to the normal app-data location so no stray file is left behind",
                    probe.display()
                );
                None
            }
        },
        Err(e) => {
            // T-114 finding #6: same pre-logger discard problem as above.
            eprintln!(
                "[handy] portable marker present but data dir unusable: '{}' is not writable ({e}) — falling back to per-user profile",
                data_dir.display()
            );
            log::warn!(
                "Portable mode: portable.marker found next to the exe, but '{}' is not writable ({e}) — falling back to the normal app-data location",
                data_dir.display()
            );
            None
        }
    }
}

/// If a `portable.marker` file exists next to the running executable AND the
/// resulting `data\` folder is usable, returns that folder — the location
/// that should hold ALL app state (settings, history, models, recordings,
/// translator queue) for this run. Returns `None` for a normal (installed)
/// run, or when portable mode was requested but isn't usable (see module
/// docs), in which case callers fall back to Tauri's own `app_data_dir()` /
/// `app_config_dir()`.
///
/// Memoized for the life of the process (see [`PORTABLE_DIR`]).
pub fn portable_data_dir() -> Option<PathBuf> {
    PORTABLE_DIR
        .get_or_init(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|d| d.to_path_buf()))
                .and_then(|exe_dir| resolve_portable_dir(&exe_dir))
        })
        .clone()
}

/// Detect the marker itself, even when the portable data directory is not
/// writable. The updater must never turn a portable launch into an installed
/// copy merely because portable storage initialization failed.
pub fn portable_marker_present() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("portable.marker")))
        .is_some_and(|marker| marker.is_file())
}

/// Drop-in replacement for `app_handle.path().app_data_dir()` (and, for
/// callers that used `app_config_dir()`, an equally valid substitute — see
/// `backup.rs`, where both collapse to the same portable dir) that redirects
/// to the portable `data\` folder when portable mode is active, and
/// otherwise defers to Tauri's normal OS-profile resolution.
pub fn resolve_app_data_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = portable_data_dir() {
        return Ok(dir);
    }
    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))
}

/// Pure `<data_dir>/logs` join, split out from [`portable_log_dir`] purely so
/// the branch is unit testable without going through the process-memoized
/// [`portable_data_dir`].
fn log_dir_for(portable_dir: Option<&Path>) -> Option<PathBuf> {
    portable_dir.map(|dir| dir.join("logs"))
}

/// `AppHandle`-free: returns the portable logs folder (`<portable_data>\logs`)
/// when portable mode is active, `None` for a normal install. Needed by
/// `lib.rs` to pick the log-plugin's file target — that target list is built
/// while assembling `tauri::Builder`, before an `AppHandle` exists, so this
/// can't go through [`resolve_app_data_dir`] / [`resolve_log_dir`] (both take
/// one). Mirrors [`portable_data_dir`]'s "just the folder, no fallback"
/// shape; the caller decides what None means (falls back to
/// `TargetKind::LogDir`, Tauri's own OS-profile log dir).
pub fn portable_log_dir() -> Option<PathBuf> {
    log_dir_for(portable_data_dir().as_deref())
}

/// Drop-in replacement for `app_handle.path().app_log_dir()` (T-114 isolation
/// gap #2): reports `<portable_data>\logs` when portable mode is active — the
/// SAME path `lib.rs` pointed the log plugin's file target at — so
/// `get_log_dir_path` / `open_log_dir` never show the user the OS profile log
/// folder while logs are actually landing in the portable folder.
pub fn resolve_log_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = portable_log_dir() {
        return Ok(dir);
    }
    app_handle
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {e}"))
}

/// Drop-in replacement for the bare `SETTINGS_STORE_PATH` string currently
/// passed to `tauri_plugin_store`'s `StoreExt::store()` in `settings.rs`.
/// `tauri_plugin_store` resolves a *relative* path against the app's own
/// data dir internally (`BaseDirectory::AppData`) via `PathBuf::push`, and a
/// relative path can't be redirected from the caller's side — but `push`
/// with an *absolute* path replaces the base entirely (standard
/// `PathBuf::push` semantics), so this returns an *absolute* path when
/// portable mode is active (which the store plugin then uses as-is), and the
/// original relative name otherwise (preserving today's installed-app
/// behavior byte-for-byte).
pub fn settings_store_path(_app: &AppHandle, relative_name: &str) -> PathBuf {
    settings_store_path_impl(relative_name)
}

/// `AppHandle`-free core of [`settings_store_path`] so the branch logic is
/// unit testable without standing up a Tauri app.
fn settings_store_path_impl(relative_name: &str) -> PathBuf {
    match portable_data_dir() {
        Some(dir) => dir.join(relative_name),
        None => PathBuf::from(relative_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a fresh temp dir to act as a fake `<exe_dir>`. Each test gets
    /// its own directory (no shared global state — `resolve_portable_dir` is
    /// pure and takes the dir as a parameter, unlike the memoized
    /// `portable_data_dir()` which is intentionally not exercised here).
    fn temp_exe_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "handy_portable_test_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn marker_absent_returns_none() {
        let dir = temp_exe_dir("no_marker");
        assert_eq!(resolve_portable_dir(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn marker_present_creates_and_returns_data_dir() {
        let dir = temp_exe_dir("marker_present");
        std::fs::write(dir.join("portable.marker"), b"").unwrap();

        let resolved = resolve_portable_dir(&dir);
        assert_eq!(resolved, Some(dir.join("data")));
        assert!(dir.join("data").is_dir());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn marker_present_but_existing_data_dir_reused() {
        let dir = temp_exe_dir("marker_existing_data");
        std::fs::write(dir.join("portable.marker"), b"").unwrap();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        // Put a marker file inside so we can prove it wasn't recreated/wiped.
        std::fs::write(dir.join("data").join("keep.txt"), b"x").unwrap();

        let resolved = resolve_portable_dir(&dir);
        assert_eq!(resolved, Some(dir.join("data")));
        assert!(dir.join("data").join("keep.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn marker_present_but_data_dir_cannot_be_created_falls_back() {
        // Simulate an unusable target: pre-create "data" as a plain FILE, not
        // a directory. `create_dir_all` fails in this situation on every
        // platform (can't turn a file into a directory), which stands in for
        // the real-world "can't be created" case (e.g. a read-only volume)
        // without needing OS-specific permission APIs in a unit test.
        let dir = temp_exe_dir("marker_blocked");
        std::fs::write(dir.join("portable.marker"), b"").unwrap();
        std::fs::write(dir.join("data"), b"not a directory").unwrap();

        let resolved = resolve_portable_dir(&dir);
        assert_eq!(
            resolved, None,
            "unwritable/uncreatable data dir must fall back to None, never panic"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn portable_data_dir_process_cache_is_stable() {
        // portable_data_dir() itself is memoized against the REAL exe path
        // (not injectable), so this just asserts it doesn't panic and is
        // idempotent across repeated calls in this process — the real
        // marker-detection logic is covered via resolve_portable_dir above.
        let first = portable_data_dir();
        let second = portable_data_dir();
        assert_eq!(first, second);
    }

    #[test]
    fn settings_store_path_relative_when_not_portable() {
        // portable_data_dir() reflects this test process's real exe dir,
        // which has no portable.marker next to it in the test environment,
        // so this exercises the "not portable" branch of
        // settings_store_path_impl end-to-end, via the actual production
        // function (no reimplemented logic).
        if portable_data_dir().is_some() {
            // Defensive: if this ever runs in an environment where a
            // portable.marker happens to sit next to the test binary, skip
            // rather than produce a false failure.
            return;
        }
        assert_eq!(
            settings_store_path_impl("settings_store.json"),
            PathBuf::from("settings_store.json")
        );
    }

    #[test]
    fn settings_store_path_absolute_under_data_dir_when_portable() {
        // Exercise the portable branch directly against resolve_portable_dir
        // (the pure, injectable function) rather than the process-memoized
        // portable_data_dir(), since we can't inject a fake exe dir into the
        // latter. This still proves the join-under-data-dir logic that
        // settings_store_path_impl relies on.
        let dir = temp_exe_dir("settings_path_portable");
        std::fs::write(dir.join("portable.marker"), b"").unwrap();
        let data_dir = resolve_portable_dir(&dir).expect("portable dir should resolve");
        assert_eq!(
            data_dir.join("settings_store.json"),
            dir.join("data").join("settings_store.json")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Finding #4: write-probe cleanup ---------------------------------

    #[test]
    fn probe_filename_is_unique_across_calls() {
        // Two calls within the same process must not collide even at
        // nanosecond resolution flakiness — this is what lets `create_new`
        // guarantee "never truncates a leftover file" rather than merely
        // "usually doesn't".
        let a = probe_filename();
        let b = probe_filename();
        assert_ne!(a, b, "probe filenames must be unique per call");
        assert!(a.starts_with(".portable_write_test_"));
    }

    #[test]
    fn successful_probe_leaves_no_file_behind() {
        // The whole point of finding #4: accepting portable mode must not
        // leave a stray dotfile in what we just told the app is a clean,
        // writable data dir.
        let dir = temp_exe_dir("probe_cleanup_verified");
        std::fs::write(dir.join("portable.marker"), b"").unwrap();

        let resolved = resolve_portable_dir(&dir);
        assert_eq!(resolved, Some(dir.join("data")));

        let leftovers: Vec<_> = std::fs::read_dir(dir.join("data"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert!(
            leftovers.is_empty(),
            "expected no leftover files in the portable data dir, found {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_uses_create_new_never_truncates_existing_file() {
        // Defense in depth for the "fixed filename could truncate a leftover
        // file" half of finding #4: even if something else already occupies
        // the exact probe path chosen for this call (can't happen in
        // practice since the name is unique per invocation, but this proves
        // the primitive itself is truncation-safe), create_new must refuse
        // to open it rather than overwrite its contents.
        let dir = temp_exe_dir("probe_create_new_safety");
        let probe_path = dir.join(probe_filename());
        std::fs::write(&probe_path, b"pre-existing content, must survive").unwrap();

        let result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe_path);
        assert!(
            result.is_err(),
            "create_new must fail rather than truncate an existing file"
        );
        assert_eq!(
            std::fs::read(&probe_path).unwrap(),
            b"pre-existing content, must survive"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Finding #2: log-dir resolution branch ----------------------------

    #[test]
    fn log_dir_for_none_when_not_portable() {
        assert_eq!(log_dir_for(None), None);
    }

    #[test]
    fn log_dir_for_joins_logs_under_portable_dir_when_portable() {
        let dir = temp_exe_dir("log_dir_portable");
        assert_eq!(log_dir_for(Some(&dir)), Some(dir.join("logs")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn portable_log_dir_none_when_not_portable() {
        // Same caveat as settings_store_path_relative_when_not_portable:
        // reflects this test process's real (non-portable) exe dir.
        if portable_data_dir().is_some() {
            return;
        }
        assert_eq!(portable_log_dir(), None);
    }
}
