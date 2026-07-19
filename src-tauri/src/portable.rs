//! Portable-mode app-data resolution (T-114).
//!
//! **STATUS: scaffold only, NOT wired into the app yet.** This file is not
//! declared as `mod portable;` in `lib.rs`, so it does not currently compile
//! into the binary. It exists as a ready-to-integrate reference for the
//! follow-up that wires portable mode into the real manager code (owned by a
//! different round per T-114 — see `tickets/T-114-portable-distribution.md`
//! for the exact call sites and line numbers to edit).
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
//! ## Integration checklist (not done by this file alone)
//!
//! 1. Add `mod portable;` to `lib.rs` (after `mod overlay;`, before `mod
//!    settings;`).
//! 2. Replace every `app_handle.path().app_data_dir()` / `self.app_handle
//!    .path().app_data_dir()` call in `managers/model.rs`,
//!    `managers/history.rs`, `managers/audio.rs`, and `managers/translator.rs`
//!    with `crate::portable::resolve_app_data_dir(app_handle)`.
//! 3. Replace the three `app.store(SETTINGS_STORE_PATH)` call sites in
//!    `settings.rs` with `app.store(crate::portable::settings_store_path(app))`.
//! 4. `backup.rs` also calls `app.path().app_data_dir()` directly (for backup
//!    archive assembly) and should be switched too, so Backup/Restore stays
//!    consistent between portable and installed builds.
//! 5. Optional/secondary: `mcp/mod.rs::sidecar_path()` uses `dirs::config_dir()`
//!    directly (it has no `AppHandle` — the CLI binary calls it standalone)
//!    and is NOT covered by this module. Left as OS-profile-only for now; the
//!    MCP discovery sidecar is a small, re-creatable pointer file, not user
//!    data, so this is a reasonable v1 gap (see ticket for detail).
//!
//! None of the edits above are made by this commit — see CLAUDE.md's
//! per-round file ownership: this round owns only new standalone files, not
//! existing `.rs` sources (`settings.rs`, `lib.rs`, the `managers/*.rs`
//! files belong to other rounds).

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// If a `portable.marker` file exists next to the running executable,
/// returns the `data` folder that should hold ALL app state (settings,
/// history, models, recordings, translator queue) for this run. Returns
/// `None` for a normal (installed) run, in which case callers should fall
/// back to Tauri's own `app_data_dir()` / `app_config_dir()`.
///
/// The `data` directory is created on first access if it doesn't exist yet
/// (mirrors the `fs::create_dir_all` Handy already does for
/// `{app_data}/models` and friends).
pub fn portable_data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    if !exe_dir.join("portable.marker").is_file() {
        return None;
    }
    let data_dir = exe_dir.join("data");
    if !data_dir.exists() {
        let _ = std::fs::create_dir_all(&data_dir);
    }
    Some(data_dir)
}

/// Drop-in replacement for `app_handle.path().app_data_dir()` that redirects
/// to the portable `data\` folder when `portable.marker` is present beside
/// the exe, and otherwise defers to Tauri's normal OS-profile resolution.
///
/// Intended call sites (see the integration checklist above): every place in
/// `managers/model.rs`, `managers/history.rs`, `managers/audio.rs`,
/// `managers/translator.rs`, and `backup.rs` that currently calls
/// `app_handle.path().app_data_dir()` directly.
pub fn resolve_app_data_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = portable_data_dir() {
        return Ok(dir);
    }
    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))
}

/// Drop-in replacement for the bare `SETTINGS_STORE_PATH` string currently
/// passed to `tauri_plugin_store`'s `StoreExt::store()` in `settings.rs`.
/// `tauri_plugin_store` resolves a *relative* path against the app's config
/// dir itself, so a relative path can't be redirected from the caller's
/// side — this returns an *absolute* path instead (which the store plugin
/// uses as-is) when portable mode is active, and the original relative name
/// otherwise (preserving today's installed-app behavior byte-for-byte).
pub fn settings_store_path(_app: &AppHandle, relative_name: &str) -> PathBuf {
    match portable_data_dir() {
        Some(dir) => dir.join(relative_name),
        None => PathBuf::from(relative_name),
    }
}
