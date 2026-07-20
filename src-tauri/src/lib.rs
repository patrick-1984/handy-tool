mod actions;
mod anchor;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple_intelligence;
mod audio_feedback;
pub mod audio_toolkit;
mod backup;
pub mod cli;
mod cli_client;
mod clipboard;
mod commands;
mod helpers;
mod input;
mod llm_client;
mod managers;
mod mcp;
mod model_testing;
mod overlay;
mod portable;
mod settings;
mod shortcut;
mod signal_handle;
mod token_count;
mod transcription_coordinator;
mod tray;
mod tray_i18n;
mod typing;
mod utils;

pub use cli::CliArgs;
#[cfg(debug_assertions)]
use specta_typescript::{BigIntExportBehavior, Typescript};
use tauri_specta::{Builder, collect_commands};

use env_filter::Builder as EnvFilterBuilder;
use managers::audio::AudioRecordingManager;
use managers::history::HistoryManager;
use managers::model::ModelManager;
use managers::transcription::TranscriptionManager;
#[cfg(unix)]
use signal_hook::consts::{SIGUSR1, SIGUSR2};
#[cfg(unix)]
use signal_hook::iterator::Signals;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tauri::image::Image;
pub use transcription_coordinator::TranscriptionCoordinator;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};

use crate::settings::get_settings;

// Global atomic to store the file log level filter
// We use u8 to store the log::LevelFilter as a number
#[cfg(debug_assertions)]
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Debug as u8);
#[cfg(not(debug_assertions))]
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Info as u8);

fn level_filter_from_u8(value: u8) -> log::LevelFilter {
    match value {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        5 => log::LevelFilter::Trace,
        _ => log::LevelFilter::Trace,
    }
}

fn build_console_filter() -> env_filter::Filter {
    let mut builder = EnvFilterBuilder::new();

    match std::env::var("RUST_LOG") {
        Ok(spec) if !spec.trim().is_empty() => {
            if let Err(err) = builder.try_parse(&spec) {
                log::warn!(
                    "Ignoring invalid RUST_LOG value '{}': {}. Falling back to info-level console logging",
                    spec,
                    err
                );
                builder.filter_level(log::LevelFilter::Info);
            }
        }
        _ => {
            builder.filter_level(log::LevelFilter::Info);
        }
    }

    builder.build()
}

/// Set when a "show main window" request (single-instance relaunch, tray
/// menu click, ...) arrives before the `main` `WebviewWindow` has been built
/// yet (T-114 finding #1(d)): `tauri_plugin_single_instance`'s callback is
/// registered via `.plugin(...)` before the `.setup(...)` closure that
/// actually builds `main` (see the setup closure below), so a rapid second
/// launch can race that window into existence and previously lost its show
/// request silently (`get_webview_window("main")` returned `None`,
/// `show_main_window` just logged an error and did nothing).
/// `show_main_window` now queues the request here instead when the window
/// isn't found; the setup closure drains the flag right after building
/// `main` via [`take_pending_main_show`]. `swap` makes the drain atomic, so
/// exactly one consumer ever acts on a queued request — no lost show, no
/// double show.
static PENDING_MAIN_SHOW: AtomicBool = AtomicBool::new(false);

/// Pure swap-and-act core of the pending-show drain, split out so it's unit
/// testable without a Tauri `AppHandle`/`AtomicBool` statics. Returns `true`
/// exactly once per queued request (subsequent calls before another queue
/// return `false`), which is what lets the caller show the window without
/// ever double-showing for the same request.
fn take_pending_main_show(flag: &AtomicBool) -> bool {
    flag.swap(false, Ordering::SeqCst)
}

fn show_main_window(app: &AppHandle) {
    if let Some(main_window) = app.get_webview_window("main") {
        // On macOS, restore the Regular activation policy BEFORE showing or
        // focusing: while the app is still an Accessory, macOS can ignore the
        // focus request and leave the dock icon absent (T-220).
        #[cfg(target_os = "macos")]
        {
            if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
                log::error!("Failed to set activation policy to Regular: {}", e);
            }
        }
        // First, ensure the window is visible
        if let Err(e) = main_window.show() {
            log::error!("Failed to show window: {}", e);
        }
        // Then, bring it to the front and give it focus
        if let Err(e) = main_window.set_focus() {
            log::error!("Failed to focus window: {}", e);
        }
    } else {
        // Window doesn't exist yet — most likely the setup closure hasn't
        // built it yet (T-114 #1(d)). Queue the request; the setup closure
        // drains PENDING_MAIN_SHOW right after building `main`.
        PENDING_MAIN_SHOW.store(true, Ordering::SeqCst);
        log::warn!("Main window not found yet — queuing show request until it is built.");
        // Close the check-then-store TOCTOU gap: `main` may have been built
        // (and the setup drain may have already run and observed `false`) in
        // the window between our `get_webview_window` miss above and the
        // `store` just now — in which case that drain will never fire again
        // and our request would be lost. Re-check: if `main` now exists,
        // consume our own flag (swap is atomic, so we and the setup drain can
        // never both act on the same queued request) and show it here.
        if app.get_webview_window("main").is_some() && take_pending_main_show(&PENDING_MAIN_SHOW) {
            show_main_window(app);
        }
    }
}

/// Push the resolved appearance theme into the overlay/floating windows.
///
/// Unlike the main window (which resolves + applies `app_theme` itself via
/// React in App.tsx, including tracking OS `prefers-color-scheme` changes
/// live when System is selected), the recording-overlay and
/// floating-transcription windows have no settings store of their own — they
/// only listen for a handful of narrow, single-purpose events (T-204). Rather
/// than widen their event contracts, we stamp `data-theme` directly onto each
/// window's `document.documentElement` from the Rust side: `System` clears
/// the attribute so each window's own `@media (prefers-color-scheme: dark)`
/// CSS keeps tracking the OS live (no JS needed for that case), while
/// `Light`/`Dark` set an explicit override that wins in both directions per
/// the `:root[data-theme="..."]` rules in RecordingOverlay.css /
/// FloatingTranscription.css.
///
/// Called once at startup (right after both aux windows are created, using
/// the persisted setting) and again on every `change_app_theme_setting` call
/// so the change applies immediately without restarting.
/// Builder `initialization_script` for a forced theme: runs before the page's
/// own scripts on EVERY navigation, closing the load race where an `eval`
/// issued right after `build()` lands on the pre-navigation document and is
/// lost. Empty for `System` (no attribute — the window's own
/// `prefers-color-scheme` CSS tracks the OS). Enum-derived: no user string
/// ever reaches the script.
pub(crate) fn theme_init_script(theme: settings::Theme) -> &'static str {
    match theme {
        settings::Theme::Light => "document.documentElement.setAttribute('data-theme','light');",
        settings::Theme::Dark => "document.documentElement.setAttribute('data-theme','dark');",
        settings::Theme::System => "",
    }
}

pub(crate) fn apply_theme_to_aux_windows(app: &AppHandle, theme: settings::Theme) {
    let js = match theme {
        settings::Theme::Light => "document.documentElement.setAttribute('data-theme','light');",
        settings::Theme::Dark => "document.documentElement.setAttribute('data-theme','dark');",
        settings::Theme::System => "document.documentElement.removeAttribute('data-theme');",
    };
    for label in ["recording_overlay", "floating_transcription"] {
        if let Some(window) = app.get_webview_window(label) {
            if let Err(e) = window.eval(js) {
                log::warn!("Failed to apply theme to '{}' window: {}", label, e);
            }
        }
    }
}

fn initialize_core_logic(app_handle: &AppHandle) {
    // Note: Enigo (keyboard/mouse simulation) is NOT initialized here.
    // The frontend is responsible for calling the `initialize_enigo` command
    // after onboarding completes. This avoids triggering permission dialogs
    // on macOS before the user is ready.

    // Initialize the managers
    let recording_manager = Arc::new(
        AudioRecordingManager::new(app_handle).expect("Failed to initialize recording manager"),
    );
    let model_manager =
        Arc::new(ModelManager::new(app_handle).expect("Failed to initialize model manager"));
    let transcription_manager = Arc::new(
        TranscriptionManager::new(app_handle, model_manager.clone())
            .expect("Failed to initialize transcription manager"),
    );
    let history_manager =
        Arc::new(HistoryManager::new(app_handle).expect("Failed to initialize history manager"));

    // Recover any recordings interrupted by a crash on a previous run.
    if let Err(e) = history_manager.reconcile_orphan_recordings() {
        log::warn!("Failed to reconcile interrupted recordings: {}", e);
    }

    // Add managers to Tauri's managed state
    app_handle.manage(recording_manager.clone());
    app_handle.manage(model_manager.clone());
    app_handle.manage(transcription_manager.clone());
    app_handle.manage(history_manager.clone());

    // Translator (folder-watch batch transcription). Started after the
    // transcription manager is managed — its worker resolves it from state.
    let translator_manager = crate::managers::translator::TranslatorManager::new(app_handle);
    app_handle.manage(translator_manager);

    // Jumper persistence: re-resolve saved slot identities against the
    // windows that exist now (opt-in; unresolved slots retry lazily).
    crate::anchor::restore_persisted_slots(app_handle);

    // Note: Shortcuts are NOT initialized here.
    // The frontend is responsible for calling the `initialize_shortcuts` command
    // after permissions are confirmed (on macOS) or after onboarding completes.
    // This matches the pattern used for Enigo initialization.

    #[cfg(unix)]
    let signals = Signals::new(&[SIGUSR1, SIGUSR2]).unwrap();
    // Set up signal handlers for toggling transcription
    #[cfg(unix)]
    signal_handle::setup_signal_handler(app_handle.clone(), signals);

    // Apply macOS Accessory policy if starting hidden
    #[cfg(target_os = "macos")]
    {
        let settings = settings::get_settings(app_handle);
        if settings.start_hidden {
            let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }
    // Get the current theme to set the appropriate initial icon
    let initial_theme = tray::get_current_theme(app_handle);

    // Choose the appropriate initial icon based on theme
    let initial_icon_path = tray::get_icon_path(initial_theme, tray::TrayIconState::Idle);

    let tray = TrayIconBuilder::new()
        .icon(
            Image::from_path(
                app_handle
                    .path()
                    .resolve(initial_icon_path, tauri::path::BaseDirectory::Resource)
                    .unwrap(),
            )
            .unwrap(),
        )
        .tooltip("Handy Tool")
        .show_menu_on_left_click(true)
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                show_main_window(app);
            }
            "copy_last_transcript" => {
                tray::copy_last_transcript(app);
            }
            "unload_model" => {
                let transcription_manager = app.state::<Arc<TranscriptionManager>>();
                if !transcription_manager.is_model_loaded() {
                    log::warn!("No model is currently loaded.");
                    return;
                }
                match transcription_manager.unload_model() {
                    Ok(()) => log::info!("Model unloaded via tray."),
                    Err(e) => log::error!("Failed to unload model via tray: {}", e),
                }
            }
            "cancel" => {
                use crate::utils::cancel_current_operation;

                // Use centralized cancellation that handles all operations
                cancel_current_operation(app);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app_handle)
        .unwrap();
    app_handle.manage(tray);

    // Initialize tray menu with idle state
    utils::update_tray_menu(app_handle, &utils::TrayIconState::Idle, None);

    // Apply show_tray_icon setting
    let settings = settings::get_settings(app_handle);
    if !settings.show_tray_icon {
        tray::set_tray_visibility(app_handle, false);
    }

    // Refresh tray menu when model state changes
    let app_handle_for_listener = app_handle.clone();
    app_handle.listen("model-state-changed", move |_| {
        tray::update_tray_menu(&app_handle_for_listener, &tray::TrayIconState::Idle, None);
    });

    // Get the autostart manager and configure based on user setting.
    // Portable mode NEVER touches this (T-114 gap #3): the autostart
    // Run-key entry is machine/user-profile state that outlives the
    // portable folder, and a fresh portable profile's `autostart_enabled`
    // default (false) would otherwise DISABLE an installed copy's autostart
    // entry the very first time a portable copy runs on the same machine.
    let settings = settings::get_settings(&app_handle);

    if portable::portable_data_dir().is_some() {
        log::info!(
            "Portable mode: skipping autostart registration (the Run-key entry, if any, is left untouched)"
        );
    } else {
        let autostart_manager = app_handle.autolaunch();
        if settings.autostart_enabled {
            // Enable autostart if user has opted in
            let _ = autostart_manager.enable();
        } else {
            // Disable autostart if user has opted out
            let _ = autostart_manager.disable();
        }
    }

    // Create the recording overlay window (hidden by default)
    utils::create_recording_overlay(app_handle);

    // Create the floating transcription window (hidden by default)
    overlay::create_floating_transcription_window(app_handle);

    // Stamp the persisted appearance theme onto both aux windows now that
    // they exist (T-204). `settings` here was fetched above for the
    // show_tray_icon/autostart checks and still reflects app_theme.
    apply_theme_to_aux_windows(app_handle, settings.app_theme);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(cli_args: CliArgs) {
    // Parse console logging directives from RUST_LOG, falling back to info-level logging
    // when the variable is unset
    let console_filter = build_console_filter();

    let specta_builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        shortcut::change_binding,
        shortcut::reset_binding,
        shortcut::change_ptt_setting,
        shortcut::change_audio_feedback_setting,
        shortcut::change_audio_feedback_volume_setting,
        shortcut::change_sound_theme_setting,
        shortcut::change_start_hidden_setting,
        shortcut::change_autostart_setting,
        shortcut::change_translate_to_english_setting,
        shortcut::change_selected_language_setting,
        shortcut::change_overlay_position_setting,
        shortcut::change_app_theme_setting,
        shortcut::change_debug_mode_setting,
        shortcut::change_word_correction_threshold_setting,
        shortcut::change_paste_method_setting,
        shortcut::change_paste_method_ptt_setting,
        shortcut::get_available_typing_tools,
        shortcut::change_typing_tool_setting,
        shortcut::change_external_script_path_setting,
        shortcut::change_clipboard_handling_setting,
        shortcut::change_auto_submit_setting,
        shortcut::change_auto_submit_key_setting,
        shortcut::change_submit_paste_method_setting,
        shortcut::change_submit_key_setting,
        shortcut::change_submit_idle_behavior_setting,
        shortcut::change_submit_clipboard_handling_setting,
        shortcut::change_clipboard_restore_delay_setting,
        shortcut::change_submit_clipboard_restore_delay_setting,
        shortcut::get_shortcut_registration_failures,
        shortcut::change_jumper_persist_setting,
        shortcut::change_anchor_action_setting,
        shortcut::change_anchor_action_slot_setting,
        shortcut::change_jumper_track_setting,
        shortcut::change_jumper_track_slot_setting,
        shortcut::change_jumper_save_cursor_setting,
        shortcut::change_jumper_cursor_mode_setting,
        shortcut::change_translator_model_unload_timeout,
        shortcut::change_translator_model_unload_custom_seconds,
        shortcut::change_return_focus_setting,
        anchor::get_anchor_status,
        anchor::clear_anchor,
        anchor::get_jump_slots,
        anchor::clear_jump_slot,
        anchor::jump_to_slot,
        commands::translator::get_translator_status,
        commands::translator::change_translator_enabled,
        commands::translator::change_translator_priority,
        commands::translator::change_translator_model,
        commands::translator::translator_add_folder,
        commands::translator::translator_set_folder_enabled,
        commands::translator::translator_remove_folder,
        shortcut::change_post_process_enabled_setting,
        shortcut::change_experimental_enabled_setting,
        shortcut::change_transcription_mode_setting,
        shortcut::change_transcription_mode_ptt_setting,
        shortcut::change_transcribe_gpu_device_setting,
        shortcut::change_api_transcription_url_setting,
        shortcut::change_api_transcription_key_setting,
        shortcut::change_api_transcription_model_setting,
        shortcut::set_openrouter_transcription_provider_ref,
        shortcut::change_openrouter_transcription_model_setting,
        shortcut::change_openrouter_transcription_route_setting,
        shortcut::change_openrouter_transcription_audio_format_setting,
        shortcut::update_model_test_library,
        shortcut::set_post_process_provider_ref,
        shortcut::change_post_process_temperature_setting,
        shortcut::add_post_process_prompt,
        shortcut::update_post_process_prompt,
        shortcut::delete_post_process_prompt,
        shortcut::set_post_process_selected_prompt,
        shortcut::update_custom_words,
        shortcut::suspend_binding,
        shortcut::resume_binding,
        shortcut::change_mute_while_recording_setting,
        shortcut::change_append_trailing_space_setting,
        shortcut::change_crash_resilient_recording_setting,
        shortcut::change_app_language_setting,
        shortcut::change_typing_start_delay_setting,
        shortcut::change_typing_key_delay_setting,
        shortcut::change_keyboard_implementation_setting,
        shortcut::get_keyboard_implementation,
        shortcut::change_post_process_disable_thinking_setting,
        shortcut::change_show_tray_icon_setting,
        shortcut::handy_keys::start_handy_keys_recording,
        shortcut::handy_keys::stop_handy_keys_recording,
        commands::cancel_operation,
        commands::get_app_dir_path,
        commands::get_app_settings,
        commands::get_default_settings,
        commands::get_log_dir_path,
        commands::set_log_level,
        commands::open_recordings_folder,
        commands::open_log_dir,
        commands::open_app_data_dir,
        commands::count_tokens,
        commands::check_apple_intelligence_available,
        commands::check_flm_available,
        commands::open_floating_transcription,
        commands::close_floating_transcription,
        commands::initialize_enigo,
        commands::initialize_shortcuts,
        typing::set_typing_text,
        typing::start_typing,
        typing::cancel_typing,
        token_count::count_tokens_via_provider,
        token_count::count_tokens_all_providers,
        token_count::cancel_token_count_sweep,
        token_count::list_provider_models,
        token_count::list_openrouter_transcription_models,
        token_count::read_text_file_for_count,
        model_testing::run_model_test,
        model_testing::cancel_model_test,
        model_testing::write_text_file,
        model_testing::fetch_openrouter_model_prices,
        mcp::get_mcp_status,
        mcp::set_mcp_enabled,
        mcp::change_mcp_port,
        mcp::regenerate_mcp_token,
        mcp::install_cli,
        shortcut::update_llm_providers,
        commands::models::get_available_models,
        commands::models::get_model_info,
        commands::models::download_model,
        commands::models::delete_model,
        commands::models::cancel_download,
        commands::models::set_active_model,
        commands::models::get_current_model,
        commands::models::get_transcription_model_status,
        commands::models::is_model_loading,
        commands::models::has_any_models_available,
        commands::models::has_any_models_or_downloads,
        commands::models::list_gpu_devices,
        commands::audio::update_microphone_mode,
        commands::audio::get_microphone_mode,
        commands::audio::get_available_microphones,
        commands::audio::set_selected_microphone,
        commands::audio::get_selected_microphone,
        commands::audio::get_available_output_devices,
        commands::audio::set_selected_output_device,
        commands::audio::get_selected_output_device,
        commands::audio::play_test_sound,
        commands::audio::check_custom_sounds,
        commands::audio::set_clamshell_microphone,
        commands::audio::get_clamshell_microphone,
        commands::audio::is_recording,
        commands::transcription::set_model_unload_timeout,
        commands::transcription::set_model_unload_custom_seconds,
        commands::transcription::get_model_load_status,
        commands::transcription::unload_model_manually,
        commands::history::get_history_entries,
        commands::history::backfill_history_durations,
        commands::history::toggle_history_entry_saved,
        commands::history::get_audio_file_path,
        commands::history::delete_history_entry,
        commands::history::update_history_limit,
        commands::history::update_recording_retention_period,
        backup::create_backup,
        backup::restore_backup,
        backup::restart_app,
        helpers::clamshell::is_laptop,
    ]);

    #[cfg(debug_assertions)] // <- Only export on non-release builds
    specta_builder
        .export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    // Portable-aware (T-114 gap #2): file logs must land under
    // `<portable_data>\logs`, not the OS profile log dir, when portable mode
    // is active. This target list is assembled before an `AppHandle` exists
    // (we're still building `tauri::Builder`), so it goes through the
    // `AppHandle`-free `portable_log_dir()` rather than
    // `portable::resolve_log_dir()` (which `commands::get_log_dir_path` /
    // `open_log_dir` use once an `AppHandle` is available, resolving to the
    // SAME path). `None` here (normal, installed run) falls back to
    // `TargetKind::LogDir`, byte-identical to before this change.
    let file_log_target = match portable::portable_log_dir() {
        Some(path) => Target::new(TargetKind::Folder {
            path,
            file_name: Some("handy".into()),
        }),
        None => Target::new(TargetKind::LogDir {
            file_name: Some("handy".into()),
        }),
    }
    .filter(|metadata| {
        let file_level = FILE_LOG_LEVEL.load(Ordering::Relaxed);
        metadata.level() <= level_filter_from_u8(file_level)
    });

    let builder = tauri::Builder::default()
        .device_event_filter(tauri::DeviceEventFilter::Always)
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            LogBuilder::new()
                .level(log::LevelFilter::Trace) // Set to most verbose level globally
                .max_file_size(500_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .clear_targets()
                .targets([
                    // Console output respects RUST_LOG environment variable
                    Target::new(TargetKind::Stdout).filter({
                        let console_filter = console_filter.clone();
                        move |metadata| console_filter.enabled(metadata)
                    }),
                    // File logs respect the user's settings (stored in FILE_LOG_LEVEL atomic)
                    file_log_target,
                ])
                .build(),
        );

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--toggle-transcription") {
                signal_handle::send_transcription_input(app, "transcribe", "CLI");
            } else if args.iter().any(|a| a == "--toggle-post-process") {
                signal_handle::send_transcription_input(app, "transcribe_with_post_process", "CLI");
            } else if args.iter().any(|a| a == "--cancel") {
                crate::utils::cancel_current_operation(app);
            } else if args.iter().any(|a| a == "--start-hidden") {
                // A second instance launched purely to (auto-)start hidden — e.g.
                // the CLI probing for the server. Don't surface the main window.
            } else {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(cli_args.clone())
        .manage(typing::TypingState::new())
        .manage(token_count::TokenCountState::new())
        .manage(model_testing::ModelTestState::new())
        .setup(move |app| {
            let mut settings = get_settings(&app.handle());

            // CLI --debug flag overrides debug_mode and log level (runtime-only, not persisted)
            if cli_args.debug {
                settings.debug_mode = true;
                settings.log_level = settings::LogLevel::Trace;
            }

            let tauri_log_level: tauri_plugin_log::LogLevel = settings.log_level.into();
            let file_log_level: log::Level = tauri_log_level.into();
            // Store the file log level in the atomic for the filter to use
            FILE_LOG_LEVEL.store(file_log_level.to_level_filter() as u8, Ordering::Relaxed);
            let app_handle = app.handle().clone();
            app.manage(TranscriptionCoordinator::new(app_handle.clone()));

            // Create the main window ourselves (T-114 gap #1: WebView2
            // storage isolation). tauri.conf.json declares "main" with
            // `"create": false` so Tauri's own pre-setup window-creation
            // loop (`app.rs`'s internal `setup()`, which runs BEFORE this
            // closure) skips it — that loop is the only place Tauri
            // auto-builds windows from config, and by the time our closure
            // ran it would already be too late to inject a data directory
            // into an already-built webview. Building it here from the
            // exact same `WindowConfig` keeps every other attribute (title,
            // size, visibility, ...) identical to the config; the only
            // addition is an explicit `.data_directory()` when portable mode
            // is active, so WebView2's localStorage/cache/IndexedDB/cookies
            // live under `<portable_data>\webview` instead of defaulting to
            // `%LOCALAPPDATA%\pr.handy` — the same folder an INSTALLED copy
            // uses, which is exactly the leak this closes (a portable and an
            // installed copy on the same machine would otherwise share
            // localStorage). Non-portable runs are unaffected:
            // `portable_data_dir()` is `None`, so this is byte-identical to
            // the previous auto-created window (same config, same
            // attributes, just built a few lines earlier).
            let main_window_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .expect("main window must be declared in tauri.conf.json");
            #[cfg_attr(not(windows), allow(unused_mut))]
            let mut main_window_builder =
                tauri::WebviewWindowBuilder::from_config(&app_handle, &main_window_config)
                    .expect("failed to build main window builder from config");
            #[cfg(windows)]
            {
                // data_directory() is WebView2-only (WKWebView on macOS has
                // no equivalent — see portable.rs module docs); portable
                // mode is Windows-only in practice (portable.cmd is
                // Windows-only per CLAUDE.md), so this is cfg-gated rather
                // than relying on portable_data_dir() staying inert
                // elsewhere.
                if let Some(portable_dir) = portable::portable_data_dir() {
                    main_window_builder =
                        main_window_builder.data_directory(portable_dir.join("webview"));
                }
            }
            main_window_builder
                .build()
                .expect("failed to create main window");

            // T-114 finding #1(d): the single-instance callback (registered
            // above via `.plugin(...)`, which runs before this closure) can
            // fire while `main` didn't exist yet and queue itself in
            // PENDING_MAIN_SHOW instead of being silently dropped. Drain it
            // now that `main` exists. `take_pending_main_show` uses `swap`,
            // so this can only fire once per queued request.
            if take_pending_main_show(&PENDING_MAIN_SHOW) {
                show_main_window(&app_handle);
            }

            initialize_core_logic(&app_handle);

            // Start the localhost MCP/CLI server if enabled, and best-effort
            // install the `handy` CLI onto PATH (so the app "ships" the CLI).
            if settings.mcp_server_enabled {
                if settings.mcp_server_token.is_empty() {
                    let token = uuid::Uuid::new_v4().to_string();
                    settings.mcp_server_token = token.clone();
                    // Persist ONLY the token from a fresh read so the runtime-only
                    // `--debug` overrides above are not written to disk.
                    let mut persisted = get_settings(&app_handle);
                    persisted.mcp_server_token = token;
                    settings::write_settings(&app_handle, persisted);
                }
                if let Err(e) = mcp::start(
                    app_handle.clone(),
                    settings.mcp_server_port,
                    settings.mcp_server_token.clone(),
                ) {
                    log::error!("Failed to start MCP/CLI server: {}", e);
                }
            }
            install_cli_if_needed();

            // Backfill duration for pre-existing history rows that lack it (older
            // recordings), off the main thread so startup isn't blocked.
            if let Some(hm) = app_handle.try_state::<Arc<HistoryManager>>() {
                let hm = hm.inner().clone();
                std::thread::spawn(move || {
                    if let Err(e) = hm.backfill_missing_durations() {
                        log::warn!("History duration backfill failed: {}", e);
                    }
                });
            }

            // Hide tray icon if --no-tray was passed
            if cli_args.no_tray {
                tray::set_tray_visibility(&app_handle, false);
            }

            // Show main window only if not starting hidden
            // CLI --start-hidden flag overrides the setting
            let should_hide = settings.start_hidden || cli_args.start_hidden;
            if !should_hide {
                if let Some(main_window) = app_handle.get_webview_window("main") {
                    main_window.show().unwrap();
                    main_window.set_focus().unwrap();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Hide pre-created windows instead of destroying them
                if window.label() == "floating_transcription"
                    || window.label() == "recording_overlay"
                {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                // Let other non-main windows close normally
                if window.label() != "main" {
                    return;
                }
                let settings = get_settings(&window.app_handle());
                let cli = window.app_handle().state::<CliArgs>();
                // If tray icon is hidden (via setting or --no-tray flag), quit the app
                if !settings.show_tray_icon || cli.no_tray {
                    window.app_handle().exit(0);
                    return;
                }
                api.prevent_close();
                let _res = window.hide();
                #[cfg(target_os = "macos")]
                {
                    let res = window
                        .app_handle()
                        .set_activation_policy(tauri::ActivationPolicy::Accessory);
                    if let Err(e) = res {
                        log::error!("Failed to set activation policy: {}", e);
                    }
                }
            }
            tauri::WindowEvent::ThemeChanged(theme) => {
                log::info!("Theme changed to: {:?}", theme);
                // Update tray icon to match new theme, maintaining idle state
                utils::change_tray_icon(&window.app_handle(), utils::TrayIconState::Idle);
            }
            _ => {}
        })
        .invoke_handler(specta_builder.invoke_handler())
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // Stop the MCP/CLI server and clean up its discovery sidecar on exit.
            if let tauri::RunEvent::Exit = event {
                mcp::stop();
            }
        });
}

/// Run a CLI companion command (headless client to the running app's server).
/// Returns the process exit code.
pub fn run_cli(cmd: cli::Commands) -> i32 {
    cli_client::run(cmd)
}

/// Destination for the installed `handy` CLI: a per-user directory already on
/// PATH (Windows: `%LOCALAPPDATA%\Microsoft\WindowsApps`; otherwise `~/.local/bin`).
pub fn cli_install_path() -> Result<std::path::PathBuf, String> {
    #[cfg(windows)]
    {
        let base = dirs::data_local_dir().ok_or("no LOCALAPPDATA directory")?;
        Ok(base.join("Microsoft").join("WindowsApps").join("handy.exe"))
    }
    #[cfg(not(windows))]
    {
        let home = dirs::home_dir().ok_or("no home directory")?;
        Ok(home.join(".local").join("bin").join("handy"))
    }
}

/// Path of the installed GUI app (Windows NSIS per-user install dir). Used by
/// the PATH CLI copy to forward bare launches — the CLI copy lives outside the
/// install dir and has no `resources\`, so the GUI cannot start from there.
/// Checks the current "Handy Tool" install dir first, then the pre-rebrand
/// "Handy" dir (the binary is named handy.exe in both — `mainBinaryName`).
#[cfg(windows)]
pub fn installed_app_path() -> Option<std::path::PathBuf> {
    let base = dirs::data_local_dir()?;
    for dir in ["Handy Tool", "Handy"] {
        let p = base.join(dir).join("handy.exe");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Copy this executable to the CLI install path so `handy` is available on PATH.
/// Writes to a temp sibling then atomically renames, so a running `handy` CLI's
/// binary is never truncated in place.
///
/// Portable-gated (T-114 gap #3): `cli_install_path()` points at
/// `%LOCALAPPDATA%\Microsoft\WindowsApps` (Windows) / `~/.local/bin` — both
/// machine/user-profile locations outside the portable folder, and outliving
/// it. This is the single choke point for ALL three ways a CLI install can be
/// triggered — the `mcp::install_cli` Tauri command (Settings UI), the
/// headless `handy install-cli` companion command (`cli_client.rs`), and the
/// startup self-refresh below (`install_cli_if_needed`) — so gating here
/// covers all of them without needing to touch `cli_client.rs`.
pub fn install_cli_binary() -> Result<String, String> {
    if portable::portable_data_dir().is_some() {
        return Err(
            "CLI install is disabled in portable mode (it would write outside this folder, to a machine/user-profile PATH location)"
                .to_string(),
        );
    }
    let src = std::env::current_exe().map_err(|e| e.to_string())?;
    let dest = cli_install_path()?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir failed: {}", e))?;
    }
    let tmp = dest.with_extension("new");
    std::fs::copy(&src, &tmp).map_err(|e| format!("copy failed: {}", e))?;
    std::fs::rename(&tmp, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("install failed: {}", e)
    })?;
    Ok(dest.to_string_lossy().to_string())
}

/// Refresh an already-installed CLI copy on startup when stale (size differs),
/// so it tracks app updates. Never bootstrap-installs: putting `handy` on PATH
/// is only done by the explicit "Install CLI" action in settings — a PATH copy
/// the user didn't ask for turns a typed `handy` into a dead console launch.
///
/// Portable mode skips this entirely (T-114 gap #3) — belt-and-suspenders
/// with the gate inside `install_cli_binary()`: this also avoids a wasted
/// `stat` of a machine-profile path and a spurious `warn!` on every portable
/// startup when a differently-sized CLI copy happens to already exist there
/// (e.g. left by an installed copy on the same machine).
fn install_cli_if_needed() {
    if portable::portable_data_dir().is_some() {
        return;
    }
    let Ok(dest) = cli_install_path() else { return };
    let src = std::env::current_exe().ok();
    let need = match (
        src.as_ref().and_then(|s| std::fs::metadata(s).ok()),
        std::fs::metadata(&dest).ok(),
    ) {
        (Some(s), Some(d)) => s.len() != d.len(),
        _ => false,
    };
    if need {
        if let Err(e) = install_cli_binary() {
            log::warn!("Could not install handy CLI to PATH: {}", e);
        } else {
            log::info!("Installed handy CLI to {}", dest.to_string_lossy());
        }
    }
}

#[cfg(test)]
mod pending_main_show_tests {
    use super::*;

    // T-114 finding #1(d): the pending-show flag exists so a "show main
    // window" request arriving before `main` is built (single-instance
    // relaunch racing the setup closure) is queued rather than dropped, and
    // is drained exactly once so it's never double-shown either. These
    // tests exercise the pure swap-and-act helper directly against a local
    // `AtomicBool` (never the process-wide `PENDING_MAIN_SHOW` static, so
    // tests can't interfere with each other when run in parallel).

    #[test]
    fn queued_request_is_taken_exactly_once() {
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::SeqCst);

        assert!(
            take_pending_main_show(&flag),
            "a queued request must be taken"
        );
        assert!(
            !take_pending_main_show(&flag),
            "a second drain must not re-fire the same request (no double-show)"
        );
    }

    #[test]
    fn no_queued_request_is_a_no_op() {
        let flag = AtomicBool::new(false);
        assert!(
            !take_pending_main_show(&flag),
            "draining with nothing queued must not claim a show"
        );
    }

    #[test]
    fn requeue_after_drain_is_taken_again() {
        // A second, later show request (e.g. another relaunch) after the
        // first was already drained must still be honored — the flag isn't
        // permanently "spent" after one use.
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::SeqCst);
        assert!(take_pending_main_show(&flag));

        flag.store(true, Ordering::SeqCst);
        assert!(take_pending_main_show(&flag));
    }

    #[test]
    fn store_after_drain_is_recovered_by_a_second_consumer() {
        // Models the TOCTOU interleaving (T-114 #1(d) re-verify): the setup
        // drain runs and finds nothing (store hadn't landed yet), THEN the
        // producer stores its request. A single drain would lose it. The
        // producer's own post-store re-check is the second consumer — and
        // because `take` is an atomic swap, exactly ONE of {setup drain,
        // producer re-check} ever returns true for a given queued request.
        let flag = AtomicBool::new(false);

        // 1. setup drain runs first — nothing queued yet.
        assert!(!take_pending_main_show(&flag), "drain sees nothing");
        // 2. producer stores its request AFTER that drain (the lost-wakeup gap).
        flag.store(true, Ordering::SeqCst);
        // 3. producer's post-store re-check consumes it — the show is NOT lost.
        assert!(
            take_pending_main_show(&flag),
            "the post-store re-check must recover a request that landed after the drain"
        );
        // And it's still exactly-once: neither consumer can act on it again.
        assert!(
            !take_pending_main_show(&flag),
            "no double-show after recovery"
        );
    }
}
