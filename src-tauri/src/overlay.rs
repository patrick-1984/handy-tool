use crate::settings;
use crate::settings::OverlayPosition;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// Generation counter incremented on every show, checked by delayed hide threads.
/// If the generation changed between spawning and waking, the hide is stale and skipped.
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(not(target_os = "macos"))]
use log::debug;

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindowBuilder;

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{CollectionBehavior, PanelBuilder, PanelLevel, tauri_panel};

#[cfg(target_os = "linux")]
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
#[cfg(target_os = "linux")]
use std::env;

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

const OVERLAY_WIDTH: f64 = 204.0;
const OVERLAY_HEIGHT: f64 = 36.0;

#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 46.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 15.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 40.0;

#[cfg(target_os = "linux")]
fn update_gtk_layer_shell_anchors(overlay_window: &tauri::webview::WebviewWindow) {
    let window_clone = overlay_window.clone();
    let _ = overlay_window.run_on_main_thread(move || {
        // Try to get the GTK window from the Tauri webview
        if let Ok(gtk_window) = window_clone.gtk_window() {
            let settings = settings::get_settings(window_clone.app_handle());
            match settings.overlay_position {
                OverlayPosition::Top => {
                    gtk_window.set_anchor(Edge::Top, true);
                    gtk_window.set_anchor(Edge::Bottom, false);
                }
                OverlayPosition::Bottom | OverlayPosition::None => {
                    gtk_window.set_anchor(Edge::Bottom, true);
                    gtk_window.set_anchor(Edge::Top, false);
                }
            }
        }
    });
}

/// Initializes GTK layer shell for Linux overlay window
/// Returns true if layer shell was successfully initialized, false otherwise
#[cfg(target_os = "linux")]
fn init_gtk_layer_shell(overlay_window: &tauri::webview::WebviewWindow) -> bool {
    // On KDE Wayland, layer-shell init has shown protocol instability.
    // Fall back to regular always-on-top overlay behavior (as in v0.7.1).
    let is_wayland = env::var("WAYLAND_DISPLAY").is_ok()
        || env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false);
    let is_kde = env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_uppercase().contains("KDE"))
        .unwrap_or(false)
        || env::var("KDE_SESSION_VERSION").is_ok();
    if is_wayland && is_kde {
        debug!("Skipping GTK layer shell init on KDE Wayland");
        return false;
    }

    if !gtk_layer_shell::is_supported() {
        return false;
    }

    // Try to get the GTK window from the Tauri webview
    if let Ok(gtk_window) = overlay_window.gtk_window() {
        // Initialize layer shell
        gtk_window.init_layer_shell();
        gtk_window.set_layer(Layer::Overlay);
        gtk_window.set_keyboard_mode(KeyboardMode::None);
        gtk_window.set_exclusive_zone(0);

        update_gtk_layer_shell_anchors(overlay_window);

        return true;
    }
    false
}

/// Forces a window to be topmost using Win32 API (Windows only)
/// This is more reliable than Tauri's set_always_on_top which can be overridden
#[cfg(target_os = "windows")]
fn force_overlay_topmost(overlay_window: &tauri::webview::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
    };

    // Clone because run_on_main_thread takes 'static
    let overlay_clone = overlay_window.clone();

    // Make sure the Win32 call happens on the UI thread
    let _ = overlay_clone.clone().run_on_main_thread(move || {
        if let Ok(hwnd) = overlay_clone.hwnd() {
            unsafe {
                // Force Z-order: make this window topmost without changing size/pos or stealing focus
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    });
}

fn calculate_overlay_position(app_handle: &AppHandle) -> Option<(f64, f64)> {
    if let Some(monitor) = app_handle.primary_monitor().ok().flatten() {
        let work_area = monitor.work_area();
        let scale = monitor.scale_factor();
        let work_area_width = work_area.size.width as f64 / scale;
        let work_area_height = work_area.size.height as f64 / scale;
        let work_area_x = work_area.position.x as f64 / scale;
        let work_area_y = work_area.position.y as f64 / scale;

        let settings = settings::get_settings(app_handle);

        let x = work_area_x + (work_area_width - OVERLAY_WIDTH) / 2.0;
        let y = match settings.overlay_position {
            OverlayPosition::Top => work_area_y + OVERLAY_TOP_OFFSET,
            OverlayPosition::Bottom | OverlayPosition::None => {
                work_area_y + work_area_height - OVERLAY_HEIGHT - OVERLAY_BOTTOM_OFFSET
            }
        };

        return Some((x, y));
    }
    None
}

/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    let position = calculate_overlay_position(app_handle);

    // On Linux (Wayland), monitor detection often fails, but we don't need exact coordinates
    // for Layer Shell as we use anchors. On other platforms, we require a position.
    #[cfg(not(target_os = "linux"))]
    if position.is_none() {
        debug!("Failed to determine overlay position, not creating overlay window");
        return;
    }

    // Stamp a forced theme before the page's scripts run — an eval issued
    // right after build() can land on the pre-navigation document (T-204).
    let theme_js = crate::theme_init_script(crate::settings::get_settings(app_handle).app_theme);
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        "recording_overlay",
        tauri::WebviewUrl::App("src/overlay/index.html".into()),
    )
    .title("Recording")
    .resizable(false)
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .accept_first_mouse(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focused(false)
    .visible(false);

    if !theme_js.is_empty() {
        builder = builder.initialization_script(theme_js);
    }

    if let Some((x, y)) = position {
        builder = builder.position(x, y);
    }

    // T-114 finding #1: without an explicit data directory, WebView2
    // defaults this window's storage to %LOCALAPPDATA%\pr.handy — the same
    // profile dir an installed copy uses — even in portable mode. Mirror the
    // main window's fix (lib.rs setup closure): share the SAME
    // `<portable_data>\webview` dir so all of Handy's webview state (main +
    // both aux windows) lands in one portable place. Non-portable/non-Windows
    // behavior is unaffected (`portable_data_dir()` is `None`).
    #[cfg(windows)]
    {
        if let Some(portable_dir) = crate::portable::portable_data_dir() {
            builder = builder.data_directory(portable_dir.join("webview"));
        }
    }

    match builder.build() {
        Ok(_window) => {
            #[cfg(target_os = "linux")]
            {
                // Try to initialize GTK layer shell, ignore errors if compositor doesn't support it
                if init_gtk_layer_shell(&_window) {
                    debug!("GTK layer shell initialized for overlay window");
                } else {
                    debug!("GTK layer shell not available, falling back to regular window");
                }
            }

            debug!("Recording overlay window created successfully (hidden)");
        }
        Err(e) => {
            debug!("Failed to create recording overlay window: {}", e);
        }
    }
}

/// Creates the recording overlay panel and keeps it hidden by default (macOS)
#[cfg(target_os = "macos")]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    if let Some((x, y)) = calculate_overlay_position(app_handle) {
        // PanelBuilder creates a Tauri window then converts it to NSPanel.
        // The window remains registered, so get_webview_window() still works.
        match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
            .url(WebviewUrl::App("src/overlay/index.html".into()))
            .title("Recording")
            .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            }))
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .corner_radius(0.0)
            .with_window(|w| w.decorations(false).transparent(true))
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary(),
            )
            .build()
        {
            Ok(panel) => {
                let _ = panel.hide();
            }
            Err(e) => {
                log::error!("Failed to create recording overlay panel: {}", e);
            }
        }
    }
}

fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    // Check if overlay should be shown based on position setting
    let settings = settings::get_settings(app_handle);
    if settings.overlay_position == OverlayPosition::None {
        return;
    }

    // Bump generation so any pending delayed-hide thread becomes stale
    OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst);

    update_overlay_position(app_handle);

    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.show();

        // On Windows, aggressively re-assert "topmost" in the native Z-order after showing
        #[cfg(target_os = "windows")]
        force_overlay_topmost(&overlay_window);

        let _ = overlay_window.emit("show-overlay", state);
    }
}

/// Shows the recording overlay window with fade-in animation
pub fn show_recording_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "recording");
}

/// Shows the transcribing overlay window
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "transcribing");
}

/// Shows the processing overlay window
pub fn show_processing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "processing");
}

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        {
            update_gtk_layer_shell_anchors(&overlay_window);
        }

        if let Some((x, y)) = calculate_overlay_position(app_handle) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }
}

/// Hides the recording overlay window with fade-out animation
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    // Always hide the overlay regardless of settings - if setting was changed while recording,
    // we still want to hide it properly
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Emit event to trigger fade-out animation
        let _ = overlay_window.emit("hide-overlay", ());
        // Hide the window after a short delay to allow animation to complete.
        // Capture the current generation so we can skip the hide if a new show happened.
        let generation = OVERLAY_GENERATION.load(Ordering::SeqCst);
        let window_clone = overlay_window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            // Only hide if no new show has occurred since we were spawned
            if OVERLAY_GENERATION.load(Ordering::SeqCst) == generation {
                let _ = window_clone.hide();
            }
        });
    }
}

/* ──────────────────── Floating Transcription Window ────────────────────── */

const FLOATING_LABEL: &str = "floating_transcription";

/// Pre-creates the floating transcription window (hidden) at startup.
/// This avoids creating a WebView2 instance on-demand which can block the
/// main thread and freeze all IPC on Windows.
#[cfg(not(target_os = "macos"))]
pub fn create_floating_transcription_window(app_handle: &AppHandle) {
    // Same load-race guard as the overlay: stamp a forced theme before the
    // page's own scripts run (T-204).
    let theme_js = crate::theme_init_script(crate::settings::get_settings(app_handle).app_theme);
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        FLOATING_LABEL,
        tauri::WebviewUrl::App("src/floating/index.html".into()),
    )
    .title("Live Transcription")
    .inner_size(800.0, 300.0)
    .min_inner_size(400.0, 150.0)
    .resizable(true)
    .decorations(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false);
    if !theme_js.is_empty() {
        builder = builder.initialization_script(theme_js);
    }

    // T-114 finding #1: same portable-webview-dir fix as the recording
    // overlay above and the main window (lib.rs) — share
    // `<portable_data>\webview` so this window's WebView2 storage doesn't
    // leak into %LOCALAPPDATA%\pr.handy in portable mode.
    #[cfg(windows)]
    {
        if let Some(portable_dir) = crate::portable::portable_data_dir() {
            builder = builder.data_directory(portable_dir.join("webview"));
        }
    }

    match builder.build() {
        Ok(_) => {
            log::debug!("Floating transcription window pre-created (hidden)");
        }
        Err(e) => {
            log::error!("Failed to pre-create floating transcription window: {}", e);
        }
    }
}

#[cfg(target_os = "macos")]
pub fn create_floating_transcription_window(app_handle: &AppHandle) {
    match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, FLOATING_LABEL)
        .url(WebviewUrl::App("src/floating/index.html".into()))
        .title("Live Transcription")
        .size(tauri::Size::Logical(tauri::LogicalSize {
            width: 800.0,
            height: 300.0,
        }))
        .level(PanelLevel::Floating)
        .has_shadow(true)
        .transparent(false)
        .no_activate(false)
        .with_window(|w| {
            w.resizable(true)
                .decorations(true)
                .min_inner_size(400.0, 150.0)
        })
        .collection_behavior(CollectionBehavior::new().can_join_all_spaces())
        .build()
    {
        Ok(_) => {
            log::debug!("Floating transcription panel pre-created (hidden)");
        }
        Err(e) => {
            log::error!("Failed to pre-create floating transcription panel: {}", e);
        }
    }
}

/// Shows the floating transcription window (already pre-created).
pub fn show_floating_transcription_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window(FLOATING_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        log::warn!("Floating transcription window not found — was it pre-created?");
    }
}

/// Hides the floating transcription window.
pub fn close_floating_transcription_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window(FLOATING_LABEL) {
        let _ = window.hide();
    }
}

/// Milliseconds of the last `mic-level` emit, for rate limiting (~30 FPS).
static LAST_MIC_LEVEL_EMIT_MS: AtomicU64 = AtomicU64::new(0);
const MIC_LEVEL_EMIT_INTERVAL_MS: u64 = 33;

/// Forwards mic spectrum levels to the recording overlay window.
///
/// The overlay is the only `mic-level` consumer, so delivery targets it via
/// `emit_to` instead of an app-wide broadcast, is rate-limited to ~30 events
/// per second, and is skipped entirely while the overlay is disabled
/// (`OverlayPosition::None`, the Linux default) or hidden.
pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    // Rate limit first — it's the cheapest check.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_MIC_LEVEL_EMIT_MS.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < MIC_LEVEL_EMIT_INTERVAL_MS {
        return;
    }
    // Advance the stamp BEFORE the settings read so a disabled/hidden overlay
    // (which returns early below) still pays get_settings at most ~30×/s
    // instead of on every audio callback.
    LAST_MIC_LEVEL_EMIT_MS.store(now_ms, Ordering::Relaxed);

    // Reads current settings each rate-limited tick, so the gate reacts
    // quickly when the overlay setting changes.
    if settings::get_settings(app_handle).overlay_position == OverlayPosition::None {
        return;
    }

    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // No consumer is watching while the overlay is hidden.
        if !overlay_window.is_visible().unwrap_or(true) {
            return;
        }
        let _ = app_handle.emit_to("recording_overlay", "mic-level", levels);
    }
}
