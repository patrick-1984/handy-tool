use crate::settings;
use crate::settings::OverlayPosition;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// The main window's handle, noted at startup on the main thread (`hwnd()` is a
/// UI-thread getter, so it is never read from the delivery thread). 0 until then.
#[cfg(windows)]
static MAIN_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Note the main window's handle for [`is_own_helper_window`]. Call on the main
/// thread once the main window exists (startup).
pub fn note_main_window(app_handle: &AppHandle) {
    #[cfg(windows)]
    if let Some(window) = app_handle.get_webview_window("main") {
        if let Ok(hwnd) = window.hwnd() {
            MAIN_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        }
    }
    #[cfg(not(windows))]
    let _ = app_handle;
}

/// Whether `hwnd` is a window of this process other than the main window: the
/// recording pill, the live text box, the too-quiet box, the floating
/// transcript, or a menu or dialog of ours. The main window is a real target
/// (the first-start setup's "Try it" types into it); the others are not.
/// False while the main window's handle is unknown, which keeps the old
/// behaviour rather than refusing every paste.
#[cfg(windows)]
pub fn is_own_helper_window(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut pid = 0u32;
    let tid =
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut core::ffi::c_void), Some(&mut pid)) };
    helper_decision(
        hwnd,
        MAIN_HWND.load(Ordering::Relaxed),
        tid != 0 && pid == std::process::id(),
    )
}

/// The window the user worked in when the pointer came onto the pill, noted so a
/// click or a right-click menu on the pill can hand the foreground back (see
/// [`restore_focus_after_pill`]). 0 until noted; cleared when the pill hides, so
/// only a window noted while the pill is up can be brought back.
#[cfg(windows)]
static FOCUS_BEFORE_PILL: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// The pointer came onto the pill, or pressed it: note the window in front -
/// unless it already is the pill (or another helper window of ours), which keeps
/// the window noted before. Hovering never activates the pill, so the window in
/// front then is the one the user dictates into.
pub fn note_focus_before_pill() {
    #[cfg(windows)]
    if let Some(hwnd) = crate::anchor::foreground_window_id() {
        if !is_own_helper_window(hwnd) {
            FOCUS_BEFORE_PILL.store(hwnd, Ordering::Relaxed);
        }
    }
}

/// After a click or a right-click menu on the pill: if that left one of Handy's
/// helper windows in front, give the foreground back to the window noted when the
/// pointer came onto the pill, so the next paste lands where the user dictates.
///
/// Needed even for a window that does not activate on clicks: the native menu
/// (muda) calls SetForegroundWindow on the pill before TrackPopupMenu. Allowed
/// by Windows because Handy's own window is then the foreground. Does nothing
/// when the main window's handle is unknown or the noted window has closed.
pub fn restore_focus_after_pill() {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};
        let noted = FOCUS_BEFORE_PILL.load(Ordering::Relaxed);
        let ours_in_front = crate::anchor::foreground_window_id().is_some_and(is_own_helper_window);
        let hwnd = HWND(noted as *mut core::ffi::c_void);
        let alive = noted != 0 && unsafe { IsWindow(Some(hwnd)).as_bool() };
        if should_restore_focus(noted, ours_in_front, alive) {
            let restored = unsafe { SetForegroundWindow(hwnd).as_bool() };
            log::debug!("Pill used: foreground handed back (restored={restored})");
        }
    }
}

/// The rule behind [`restore_focus_after_pill`], kept pure for testing.
#[cfg_attr(not(windows), allow(dead_code))]
fn should_restore_focus(noted: isize, ours_in_front: bool, noted_alive: bool) -> bool {
    noted != 0 && ours_in_front && noted_alive
}

/// The rule behind [`is_own_helper_window`], kept pure for testing.
#[cfg_attr(not(windows), allow(dead_code))]
fn helper_decision(hwnd: isize, main_hwnd: isize, same_process: bool) -> bool {
    hwnd != 0 && main_hwnd != 0 && same_process && hwnd != main_hwnd
}

#[cfg(test)]
mod helper_window_tests {
    use super::{helper_decision, should_restore_focus};

    #[test]
    fn the_foreground_goes_back_only_when_the_pill_took_it_from_a_live_window() {
        assert!(should_restore_focus(42, true, true));
        assert!(
            !should_restore_focus(42, false, true),
            "the user's app is still in front"
        );
        assert!(
            !should_restore_focus(42, true, false),
            "the noted window has closed"
        );
        assert!(!should_restore_focus(0, true, true), "nothing noted yet");
    }

    #[test]
    fn only_our_other_windows_count_as_helpers() {
        assert!(
            helper_decision(7, 5, true),
            "the pill, a window of ours that is not main"
        );
        assert!(
            !helper_decision(5, 5, true),
            "the main window is a real target"
        );
        assert!(!helper_decision(7, 5, false), "another app's window");
        assert!(
            !helper_decision(7, 0, true),
            "main unknown: keep the old behaviour"
        );
        assert!(!helper_decision(0, 5, true), "no foreground window");
    }
}

/// A take's state (recording, paused, transcribing, a microphone problem...) is on
/// the pill, from the start of its show until the pill hides. Unlike
/// `OVERLAY_VISIBLE` it is set before the show's window work, and a sound source
/// notice never sets it.
static TAKE_ON_PILL: AtomicBool = AtomicBool::new(false);

/// Generation counter incremented on every show, checked by delayed hide threads.
/// If the generation changed between spawning and waking, the hide is stale and skipped.
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);

/// The pill showed a "Transcribing N%" figure since it was last shown: its hide
/// waits a moment longer for the figure to run up to 100% first.
static PROGRESS_SHOWN: AtomicBool = AtomicBool::new(false);

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

/// The pill at its normal size; the Pill size setting scales it (the page zooms).
const BASE_OVERLAY_WIDTH: f64 = 188.0;
const BASE_OVERLAY_HEIGHT: f64 = 36.0;

/// Transparent room round the pill inside its window: the progress light runs
/// centred on the pill's border (half of it outside), with its glow.
const OVERLAY_MARGIN: f64 = 10.0;

/// The pill's size for the current settings.
fn overlay_size(settings: &settings::AppSettings) -> (f64, f64) {
    let scale = settings.pill_scale_factor();
    (BASE_OVERLAY_WIDTH * scale, BASE_OVERLAY_HEIGHT * scale)
}

/// The pill's window: the pill plus its margin on every side.
fn overlay_window_size(settings: &settings::AppSettings) -> (f64, f64) {
    let (width, height) = overlay_size(settings);
    let margin = OVERLAY_MARGIN * settings.pill_scale_factor();
    (width + 2.0 * margin, height + 2.0 * margin)
}

/// Where the pill's window goes so the pill itself sits at its usual place.
fn overlay_window_position(app_handle: &AppHandle) -> Option<(f64, f64)> {
    let (x, y) = calculate_overlay_position(app_handle)?;
    let margin = OVERLAY_MARGIN * settings::get_settings(app_handle).pill_scale_factor();
    Some((x - margin, y - margin))
}

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
    // Manual escape hatch: some compositors report layer-shell as "supported"
    // yet render the overlay incorrectly. HANDY_NO_GTK_LAYER_SHELL=1 (or
    // true/yes, case-insensitive) forces the plain always-on-top fallback.
    // Unset/0/false/empty leaves behavior unchanged.
    let force_disabled = env::var("HANDY_NO_GTK_LAYER_SHELL")
        .map(|v| {
            let v = v.trim();
            !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
        })
        .unwrap_or(false);
    if force_disabled {
        debug!("HANDY_NO_GTK_LAYER_SHELL set — skipping GTK layer shell init");
        return false;
    }

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
        let (width, height) = overlay_size(&settings);

        let x = work_area_x + (work_area_width - width) / 2.0;
        let y = match settings.overlay_position {
            OverlayPosition::Top => work_area_y + OVERLAY_TOP_OFFSET,
            OverlayPosition::Bottom | OverlayPosition::None => {
                work_area_y + work_area_height - height - OVERLAY_BOTTOM_OFFSET
            }
        };

        return Some((x, y));
    }
    None
}

/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    let position = overlay_window_position(app_handle);

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
    .inner_size(
        overlay_window_size(&settings::get_settings(app_handle)).0,
        overlay_window_size(&settings::get_settings(app_handle)).1,
    )
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
    if let Some((x, y)) = overlay_window_position(app_handle) {
        // PanelBuilder creates a Tauri window then converts it to NSPanel.
        // The window remains registered, so get_webview_window() still works.
        match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
            .url(WebviewUrl::App("src/overlay/index.html".into()))
            .title("Recording")
            .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: overlay_window_size(&settings::get_settings(app_handle)).0,
                height: overlay_window_size(&settings::get_settings(app_handle)).1,
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
    // First, so a sound source notice from here on joins this take's pill instead
    // of showing (and later hiding) a pill of its own.
    TAKE_ON_PILL.store(true, Ordering::SeqCst);
    // The main window follows the take too (the setup's Try it), whether or
    // not the pill is shown. `emit_to` only queues the event.
    let _ = app_handle.emit_to("main", "take-state", state);

    // Bump generation so any pending delayed-hide thread becomes stale - also
    // with no pill (position None), or an old hide would end the new take in
    // the main window.
    OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst);

    // Check if overlay should be shown based on position setting
    let settings = settings::get_settings(app_handle);
    if settings.overlay_position == OverlayPosition::None {
        return;
    }
    if state != "transcribing" && state != "processing" {
        PROGRESS_SHOWN.store(false, Ordering::Relaxed);
    }

    update_overlay_position(app_handle);

    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Mark visible ONLY if the native show actually succeeded, so the audio
        // worker's emit_levels doesn't queue frames to a window that never
        // appeared (T-306: it gates on this flag, never on a sync window getter).
        if overlay_window.show().is_ok() {
            OVERLAY_VISIBLE.store(true, Ordering::Relaxed);
        }

        // On Windows, aggressively re-assert "topmost" in the native Z-order after showing
        #[cfg(target_os = "windows")]
        force_overlay_topmost(&overlay_window);

        let _ = overlay_window.emit("show-overlay", state);
    }
}

/// Shows the recording overlay window with fade-in animation
pub fn show_recording_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "recording");
    show_live_text_window(app_handle);
}

/// Shows the transcribing overlay window
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "transcribing");
}

/// Shows the processing overlay window
pub fn show_processing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "processing");
}

/// Shows the overlay's paused state (the take is open but takes in no audio)
pub fn show_paused_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "paused");
}

/// Tells the user why a take could not start (`state` is one of the overlay's
/// microphone-problem states: no microphone, blocked, error), then hides itself. A
/// new show in the meantime (e.g. a mic was plugged in and the shortcut pressed
/// again) bumps the generation, which cancels the pending hide.
pub fn show_microphone_problem_overlay(app_handle: &AppHandle, state: &str) {
    show_overlay_state(app_handle, state);
    let generation = OVERLAY_GENERATION.load(Ordering::SeqCst);
    let app_handle = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        if OVERLAY_GENERATION.load(Ordering::SeqCst) == generation {
            hide_recording_overlay(&app_handle);
        }
    });
}

/// What the pill says after the sound source changed from the shortcut or its menu.
#[derive(Clone, serde::Serialize)]
struct CaptureSourceNotice {
    source: settings::CaptureSource,
    /// The take in progress keeps its source; the new one starts with the next take.
    next_take: bool,
}

/// How long the pill shows a sound source change.
const CAPTURE_NOTICE_MS: u64 = 1_600;

/// Say on the pill which sound source takes record now (the Cycle Sound Source
/// shortcut, the pill's right-click menu). During a take the pill shows it over the
/// sound bars for a moment; with no take on screen the pill appears just for it and
/// hides again. Nothing with Overlay Position: None.
pub fn show_capture_source_notice(
    app_handle: &AppHandle,
    source: settings::CaptureSource,
    next_take: bool,
) {
    if settings::get_settings(app_handle).overlay_position == OverlayPosition::None {
        return;
    }
    let notice = CaptureSourceNotice { source, next_take };
    if TAKE_ON_PILL.load(Ordering::SeqCst) {
        let _ = app_handle.emit_to("recording_overlay", "capture-source-notice", notice);
        return;
    }
    // Not through show_overlay_state: that tells the main window a take started.
    // No lock against a take starting right now (its show runs on the main thread,
    // which this one waits on): a take shown in the meantime keeps its state on
    // the pill (the page ignores a later "notice") and the hide below skips it.
    OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst);
    let generation = OVERLAY_GENERATION.load(Ordering::SeqCst);
    update_overlay_position(app_handle);
    let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") else {
        return;
    };
    if overlay_window.show().is_ok() {
        OVERLAY_VISIBLE.store(true, Ordering::Relaxed);
    }
    #[cfg(target_os = "windows")]
    force_overlay_topmost(&overlay_window);
    let _ = overlay_window.emit("show-overlay", "notice");
    let _ = overlay_window.emit("capture-source-notice", notice);
    // A take shown in the meantime (or another notice) bumps the generation and
    // keeps the pill.
    let app_handle = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(CAPTURE_NOTICE_MS));
        if OVERLAY_GENERATION.load(Ordering::SeqCst) == generation
            && !TAKE_ON_PILL.load(Ordering::SeqCst)
        {
            hide_recording_overlay(&app_handle);
        }
    });
}

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        {
            update_gtk_layer_shell_anchors(&overlay_window);
        }

        // The Pill size setting may have changed since the last take.
        let (width, height) = overlay_window_size(&settings::get_settings(app_handle));
        let _ = overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        if let Some((x, y)) = overlay_window_position(app_handle) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }
}

/// Hides the recording overlay window with fade-out animation
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    // Stop the audio worker from emitting levels immediately (T-306), before any
    // window work below — the actual native hide is delayed for the fade-out.
    OVERLAY_VISIBLE.store(false, Ordering::Relaxed);
    TAKE_ON_PILL.store(false, Ordering::SeqCst);
    #[cfg(windows)]
    FOCUS_BEFORE_PILL.store(0, Ordering::Relaxed);
    let _ = app_handle.emit_to("main", "take-state", "idle");
    if QUIET_HINT_SHOWN.swap(false, Ordering::Relaxed) {
        set_quiet_hint_visible(app_handle, false);
    }
    // Always hide the overlay regardless of settings - if setting was changed while recording,
    // we still want to hide it properly
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Emit event to trigger fade-out animation
        let _ = overlay_window.emit("hide-overlay", ());
        // Hide the window after a short delay to allow animation to complete.
        // Capture the current generation so we can skip the hide if a new show happened.
        let generation = OVERLAY_GENERATION.load(Ordering::SeqCst);
        let window_clone = overlay_window.clone();
        let live_text = app_handle.get_webview_window(LIVE_TEXT_LABEL);
        if let Some(w) = &live_text {
            let _ = w.emit("live-text-hide", ());
        }
        // A "Transcribing N%" figure first runs up to 100% (the pill does that
        // on hide-overlay), then fades.
        let fade_ms = if PROGRESS_SHOWN.swap(false, Ordering::Relaxed) {
            650
        } else {
            300
        };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(fade_ms));
            // Only hide if no new show has occurred since we were spawned
            if OVERLAY_GENERATION.load(Ordering::SeqCst) == generation {
                let _ = window_clone.hide();
            }
            // The live text box keeps a finished take's final text on screen a
            // moment longer, and text still being typed in until it is done (it
            // fades itself, at most ~13.5 s); its window is click-through, so
            // staying up costs nothing.
            std::thread::sleep(std::time::Duration::from_millis(14_000 - fade_ms));
            if OVERLAY_GENERATION.load(Ordering::SeqCst) == generation {
                if let Some(w) = live_text {
                    let _ = w.hide();
                }
            }
        });
    }
}

/* ─────────────────────────── Live Text Box ─────────────────────────────── */

const LIVE_TEXT_LABEL: &str = "live_text";
const LIVE_TEXT_WIDTH: f64 = 460.0;
/// "Last words" is one line next to the pill: 40 px at the normal 15 px text.
fn live_text_one_line_height(settings: &settings::AppSettings) -> f64 {
    (settings.live_text_font_size.clamp(11, 28) as f64 * 8.0 / 3.0).round()
}
/// "Whole text" grows from one line towards the screen's middle, up to the
/// chosen number of lines (Box height) but never taller than this share of the
/// work area. The window is click-through and transparent, so the room it keeps
/// costs nothing.
const LIVE_TEXT_MAX_SHARE: f64 = 0.5;
const LIVE_TEXT_GAP: f64 = 8.0;

#[derive(Clone, serde::Serialize)]
struct LiveTextShow {
    mode: crate::settings::LiveTextMode,
    /// Fade out a few seconds after the text stops changing.
    fade: bool,
    /// The box sits below the pill (pill at the top of the screen) instead of above.
    below_pill: bool,
    /// Text size in logical pixels.
    font_size: u32,
}

/// The box's window size for the current settings: the chosen width, and one
/// line or room to grow into.
fn live_text_size(app_handle: &AppHandle, settings: &settings::AppSettings) -> (f64, f64) {
    let width = settings.live_text_width as f64;
    if settings.live_text_mode == crate::settings::LiveTextMode::LastWords {
        return (width, live_text_one_line_height(settings));
    }
    let work_area_height = app_handle
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.work_area().size.height as f64 / m.scale_factor())
        .unwrap_or(1000.0);
    (
        width,
        // The lines at the chosen text size (line height 1.55) plus the box's
        // padding and border.
        (settings.live_text_lines.clamp(2, 30) as f64
            * settings.live_text_font_size.clamp(11, 28) as f64
            * 1.55
            + 26.0)
            .ceil()
            .min(work_area_height * LIVE_TEXT_MAX_SHARE),
    )
}

/// Centred on the pill, just above it (or below it when the pill is at the top).
fn live_text_position(
    app_handle: &AppHandle,
    below_pill: bool,
    (width, height): (f64, f64),
) -> Option<(f64, f64)> {
    let (x, y) = calculate_overlay_position(app_handle)?;
    let (pill_width, pill_height) = overlay_size(&settings::get_settings(app_handle));
    let lx = x + pill_width / 2.0 - width / 2.0;
    let ly = if below_pill {
        y + pill_height + LIVE_TEXT_GAP
    } else {
        y - height - LIVE_TEXT_GAP
    };
    Some((lx, ly))
}

/// Pre-creates the live text box (hidden): a click-through, never-focused strip
/// next to the recording pill that shows the take's live text.
#[cfg(not(target_os = "macos"))]
pub fn create_live_text_window(app_handle: &AppHandle) {
    // Linux Wayland: without layer shell the compositor places (and may focus) a
    // plain window wherever it likes, so there is no live text box there.
    #[cfg(target_os = "linux")]
    if crate::utils::is_wayland() {
        debug!("Wayland: no live text box window (not placeable without layer shell)");
        return;
    }
    let theme_js = crate::theme_init_script(crate::settings::get_settings(app_handle).app_theme);
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        LIVE_TEXT_LABEL,
        tauri::WebviewUrl::App("src/livetext/index.html".into()),
    )
    .title("Live text")
    .resizable(false)
    .inner_size(
        LIVE_TEXT_WIDTH,
        live_text_one_line_height(&settings::get_settings(app_handle)),
    )
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focused(false)
    .focusable(false)
    .visible(false);

    if !theme_js.is_empty() {
        builder = builder.initialization_script(theme_js);
    }
    // Same portable webview data dir as the other windows (T-114 finding #1).
    #[cfg(windows)]
    {
        if let Some(portable_dir) = crate::portable::portable_data_dir() {
            builder = builder.data_directory(portable_dir.join("webview"));
        }
    }

    match builder.build() {
        Ok(window) => {
            // Clicks pass through to whatever is underneath.
            let _ = window.set_ignore_cursor_events(true);
            debug!("Live text window created (hidden)");
        }
        Err(e) => debug!("Failed to create live text window: {}", e),
    }
}

/// macOS: the recording overlay is an NSPanel there; no live text box yet.
#[cfg(target_os = "macos")]
pub fn create_live_text_window(_app_handle: &AppHandle) {}

/// Shows the live text box next to the pill for the take, when it is switched on.
pub fn show_live_text_window(app_handle: &AppHandle) {
    let settings = settings::get_settings(app_handle);
    if !settings.live_text_box_enabled || settings.overlay_position == OverlayPosition::None {
        return;
    }
    present_live_text_window(app_handle, &settings);
}

/// "Show the text as it's transcribed": a take without the live text box gets the
/// box at stop, for its transcript to be typed into as it comes in. Returns
/// whether the box is shown for that.
pub fn show_live_text_after_stop(app_handle: &AppHandle) -> bool {
    let settings = settings::get_settings(app_handle);
    if !settings.live_text_after_stop
        || settings.live_text_box_enabled
        || settings.overlay_position == OverlayPosition::None
    {
        return false;
    }
    present_live_text_window(app_handle, &settings);
    true
}

fn present_live_text_window(app_handle: &AppHandle, settings: &settings::AppSettings) {
    let below_pill = settings.overlay_position == OverlayPosition::Top;
    if let Some(window) = app_handle.get_webview_window(LIVE_TEXT_LABEL) {
        let (width, height) = live_text_size(app_handle, &settings);
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        if let Some((x, y)) = live_text_position(app_handle, below_pill, (width, height)) {
            let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
        let _ = window.emit(
            "live-text-show",
            LiveTextShow {
                mode: settings.live_text_mode,
                fade: settings.live_text_fade,
                below_pill,
                font_size: settings.live_text_font_size.clamp(11, 28),
            },
        );
        if window.show().is_ok() {
            #[cfg(target_os = "windows")]
            force_overlay_topmost(&window);
        }
    }
}

/// Applies a live-text setting change (T button, settings page) to a take in
/// progress: show or hide the box right away.
pub fn refresh_live_text_window(app_handle: &AppHandle) {
    let enabled = settings::get_settings(app_handle).live_text_box_enabled;
    if enabled && OVERLAY_VISIBLE.load(Ordering::Relaxed) {
        show_live_text_window(app_handle);
    } else if let Some(window) = app_handle.get_webview_window(LIVE_TEXT_LABEL) {
        let _ = window.emit("live-text-hide", ());
        let _ = window.hide();
    }
}

/* ─────────────────────────── "Too quiet" box ───────────────────────────── */

const QUIET_HINT_LABEL: &str = "quiet_hint";
/// Room for the box in any language; the window is transparent and click-through.
const QUIET_HINT_WIDTH: f64 = 320.0;
// The box fills its window, so it sits exactly QUIET_HINT_GAP under the pill.
const QUIET_HINT_HEIGHT: f64 = 22.0;
const QUIET_HINT_GAP: f64 = 6.0;

/// This take shows "Too quiet" in its own box rather than in the pill (set per take).
static QUIET_HINT_BOX: AtomicBool = AtomicBool::new(false);
/// The box is up.
static QUIET_HINT_SHOWN: AtomicBool = AtomicBool::new(false);

/// Set once the too-quiet box exists. It never does on macOS (no such window yet)
/// or on Linux Wayland, and not when building it failed: the pill shows the hint.
static QUIET_HINT_READY: AtomicBool = AtomicBool::new(false);

/// Per take: "Too quiet" in its own box (where that window exists) or in the pill.
pub fn set_quiet_hint_box(on: bool) {
    QUIET_HINT_BOX.store(
        on && QUIET_HINT_READY.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
}

/// Pre-creates the "Too quiet" box (hidden): a click-through, never-focused strip
/// just under the recording pill - in the gap above the taskbar when the pill
/// is at the bottom, so it never covers the live text box.
#[cfg(not(target_os = "macos"))]
pub fn create_quiet_hint_window(app_handle: &AppHandle) {
    // Linux Wayland: as for the live text box; the pill shows the hint itself.
    #[cfg(target_os = "linux")]
    if crate::utils::is_wayland() {
        debug!("Wayland: no too-quiet box window; the pill shows the hint");
        return;
    }
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        QUIET_HINT_LABEL,
        tauri::WebviewUrl::App("src/quiethint/index.html".into()),
    )
    .title("Too quiet")
    .resizable(false)
    .inner_size(QUIET_HINT_WIDTH, QUIET_HINT_HEIGHT)
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focused(false)
    .focusable(false)
    .visible(false);
    // Same portable webview data dir as the other windows (T-114 finding #1).
    #[cfg(windows)]
    {
        if let Some(portable_dir) = crate::portable::portable_data_dir() {
            builder = builder.data_directory(portable_dir.join("webview"));
        }
    }
    match builder.build() {
        Ok(window) => {
            let _ = window.set_ignore_cursor_events(true);
            QUIET_HINT_READY.store(true, Ordering::Relaxed);
            debug!("Too-quiet box created (hidden)");
        }
        Err(e) => debug!("Failed to create the too-quiet box: {}", e),
    }
}

/// macOS: the recording overlay is an NSPanel there; the pill shows the hint itself.
#[cfg(target_os = "macos")]
pub fn create_quiet_hint_window(_app_handle: &AppHandle) {}

/// Shows or hides the box. Called from the audio worker, so the work runs on a
/// thread of its own: placing the box asks the main loop for the monitor,
/// which must never block the worker (T-306).
fn set_quiet_hint_visible(app_handle: &AppHandle, show: bool) {
    let app = app_handle.clone();
    std::thread::spawn(move || {
        let Some(window) = app.get_webview_window(QUIET_HINT_LABEL) else {
            return;
        };
        if show {
            let settings = settings::get_settings(&app);
            let scale = settings.pill_scale_factor();
            let (pill_width, pill_height) = overlay_size(&settings);
            let (width, height) = (QUIET_HINT_WIDTH * scale, QUIET_HINT_HEIGHT * scale);
            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
            if let Some((x, y)) = calculate_overlay_position(&app) {
                let x = x + pill_width / 2.0 - width / 2.0;
                let y = y + pill_height + QUIET_HINT_GAP;
                let _ =
                    window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
            }
            let _ = window.emit("quiet-hint", true);
            if window.show().is_ok() {
                #[cfg(target_os = "windows")]
                force_overlay_topmost(&window);
            }
            if QUIET_HINT_SHOWN.load(Ordering::Relaxed) {
                return;
            }
            // Hidden again while this was being shown.
        }
        let _ = window.emit("quiet-hint", false);
        // Let it fade, unless it came back in the meantime.
        std::thread::sleep(std::time::Duration::from_millis(250));
        if !QUIET_HINT_SHOWN.load(Ordering::Relaxed) {
            let _ = window.hide();
        }
    });
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

/// Desired recording-overlay visibility, tracked process-locally (T-306). The
/// audio worker's `emit_levels` MUST NOT call Tauri's synchronous
/// `Window::is_visible()` getter: that getter posts a message to the main event
/// loop and blocks on the reply, so if the main thread is simultaneously inside
/// `cancel_current_operation` joining the recorder worker (which is what runs
/// `emit_levels`), the two block on each other and the whole app hangs (the
/// recording overlay freezes mid-frame). This flag lets the worker gate emits
/// with a lock-free atomic instead. Set true when an overlay state is shown,
/// false the instant a hide is requested.
static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);

/// One `mic-level` update: the spectrum, and whether the microphone is live
/// (false while a cold one is still warming up - the pill says "Starting mic").
#[derive(Clone, serde::Serialize)]
struct MicLevel<'a> {
    levels: &'a [f32],
    live: bool,
    /// Speech too quiet to be kept was heard just now: the pill says so.
    too_quiet: bool,
}

/// Forwards mic spectrum levels to the recording overlay window.
///
/// The overlay is the only `mic-level` consumer, so delivery targets it via
/// `emit_to` instead of an app-wide broadcast, is rate-limited to ~30 events
/// per second, and is skipped entirely while the overlay is disabled
/// (`OverlayPosition::None`, the Linux default) or hidden.
pub fn emit_levels(app_handle: &AppHandle, levels: &[f32], live: bool, too_quiet: bool) {
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

    // No consumer is watching while the overlay is hidden. Gate on the
    // process-local flag — NEVER on `Window::is_visible()` here: this runs on
    // the audio worker thread and that getter blocks on the main event loop,
    // which deadlocks against `cancel_current_operation` joining this very
    // worker on the main thread (T-306). `emit_to` only queues the event, so it
    // is safe to call from the worker without touching the window handle.
    if !OVERLAY_VISIBLE.load(Ordering::Relaxed) {
        return;
    }
    // "Too quiet" in a box of its own: the pill keeps its sound bars.
    let in_box = QUIET_HINT_BOX.load(Ordering::Relaxed);
    if in_box && QUIET_HINT_SHOWN.swap(too_quiet, Ordering::Relaxed) != too_quiet {
        set_quiet_hint_visible(app_handle, too_quiet);
    }
    let _ = app_handle.emit_to(
        "recording_overlay",
        "mic-level",
        MicLevel {
            levels,
            live,
            too_quiet: too_quiet && !in_box,
        },
    );
}

/// Sends the overlay how far the take's transcription is (percent). Like
/// `emit_levels`, skipped while the overlay is hidden (same T-306 flag).
pub fn emit_transcription_progress(app_handle: &AppHandle, percent: u8) {
    // The main window too (the setup's Try it), even with the pill hidden. A
    // name of its own: a window's listen() hears every emit_to target, so the
    // pill must not get this copy (nor the main window the pill's).
    let _ = app_handle.emit_to("main", "take-progress", percent);
    if !OVERLAY_VISIBLE.load(Ordering::Relaxed) {
        return;
    }
    PROGRESS_SHOWN.store(true, Ordering::Relaxed);
    let _ = app_handle.emit_to("recording_overlay", "transcription-progress", percent);
}
