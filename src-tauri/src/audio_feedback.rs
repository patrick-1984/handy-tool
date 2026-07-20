use crate::settings::SoundTheme;
use crate::settings::{self, AppSettings};
use cpal::traits::{DeviceTrait, HostTrait};
use log::{debug, error, warn};
use rodio::OutputStreamBuilder;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::thread;
use tauri::{AppHandle, Manager};

pub enum SoundType {
    Start,
    Stop,
}

fn resolve_sound_path(
    app: &AppHandle,
    settings: &AppSettings,
    sound_type: SoundType,
) -> Option<PathBuf> {
    let sound_file = get_sound_path(settings, sound_type);
    // T-114 finding #5: custom sounds are user data (dropped in the app-data
    // dir by the user), not a bundled resource — route them through the
    // portable-aware resolver so a portable launch plays/discovers its own
    // `data\custom_*.wav` instead of always resolving BaseDirectory::AppData
    // (which is %APPDATA%\pr.handy regardless of portable mode, so portable
    // custom sounds would be silently ignored and an installed copy's custom
    // sounds would leak into a portable run on the same machine). Built-in
    // theme sounds stay bundled resources either way — unaffected.
    if is_custom_sound(settings) {
        return crate::portable::resolve_app_data_dir(app)
            .ok()
            .map(|dir| dir.join(sound_file));
    }
    app.path()
        .resolve(&sound_file, tauri::path::BaseDirectory::Resource)
        .ok()
}

/// Pure branch-selection helper backing `resolve_sound_path`'s
/// custom-vs-bundled-resource decision (T-114 finding #5), split out so it's
/// unit testable without an `AppHandle`.
fn is_custom_sound(settings: &AppSettings) -> bool {
    settings.sound_theme == SoundTheme::Custom
}

fn get_sound_path(settings: &AppSettings, sound_type: SoundType) -> String {
    match (settings.sound_theme, sound_type) {
        (SoundTheme::Custom, SoundType::Start) => "custom_start.wav".to_string(),
        (SoundTheme::Custom, SoundType::Stop) => "custom_stop.wav".to_string(),
        (_, SoundType::Start) => settings.sound_theme.to_start_path(),
        (_, SoundType::Stop) => settings.sound_theme.to_stop_path(),
    }
}

pub fn play_feedback_sound(app: &AppHandle, sound_type: SoundType) {
    let settings = settings::get_settings(app);
    if !settings.audio_feedback {
        return;
    }
    if let Some(path) = resolve_sound_path(app, &settings, sound_type) {
        play_sound_async(app, path);
    }
}

pub fn play_feedback_sound_blocking(app: &AppHandle, sound_type: SoundType) {
    let settings = settings::get_settings(app);
    if !settings.audio_feedback {
        return;
    }
    if let Some(path) = resolve_sound_path(app, &settings, sound_type) {
        play_sound_blocking(app, &path);
    }
}

/// Short synthesized double-beep for "busy — press ignored" feedback (a
/// transcribe press while the previous take is still processing). No sound
/// asset needed; respects the audio feedback toggle and volume.
pub fn play_busy_beep(app: &AppHandle) {
    let settings = settings::get_settings(app);
    if !settings.audio_feedback {
        return;
    }
    let volume = settings.audio_feedback_volume;
    let selected_device = settings.selected_output_device.clone();
    thread::spawn(move || {
        use rodio::source::{SineWave, Source};
        use std::time::Duration;
        let Ok(stream_builder) = stream_builder_for_device(selected_device) else {
            return;
        };
        let Ok(stream_handle) = stream_builder.open_stream() else {
            return;
        };
        let sink = rodio::Sink::connect_new(stream_handle.mixer());
        // Two quick low tones read as "not now" without resembling start/stop.
        for _ in 0..2 {
            sink.append(
                SineWave::new(320.0)
                    .take_duration(Duration::from_millis(70))
                    .amplify(0.30 * volume),
            );
            sink.append(
                SineWave::new(1.0)
                    .take_duration(Duration::from_millis(45))
                    .amplify(0.0),
            );
        }
        sink.sleep_until_end();
    });
}

pub fn play_test_sound(app: &AppHandle, sound_type: SoundType) {
    let settings = settings::get_settings(app);
    if let Some(path) = resolve_sound_path(app, &settings, sound_type) {
        play_sound_blocking(app, &path);
    }
}

fn play_sound_async(app: &AppHandle, path: PathBuf) {
    let app_handle = app.clone();
    thread::spawn(move || {
        if let Err(e) = play_sound_at_path(&app_handle, path.as_path()) {
            error!("Failed to play sound '{}': {}", path.display(), e);
        }
    });
}

fn play_sound_blocking(app: &AppHandle, path: &Path) {
    if let Err(e) = play_sound_at_path(app, path) {
        error!("Failed to play sound '{}': {}", path.display(), e);
    }
}

fn play_sound_at_path(app: &AppHandle, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let settings = settings::get_settings(app);
    let volume = settings.audio_feedback_volume;
    let selected_device = settings.selected_output_device.clone();
    play_audio_file(path, selected_device, volume)
}

/// Build an output stream for the configured device, falling back to the
/// system default. Shared by file playback and the synthesized busy beep so
/// both honor the user's output-device selection.
fn stream_builder_for_device(
    selected_device: Option<String>,
) -> Result<OutputStreamBuilder, Box<dyn std::error::Error>> {
    let stream_builder = if let Some(device_name) = selected_device {
        if device_name == "Default" {
            debug!("Using default device");
            OutputStreamBuilder::from_default_device()?
        } else {
            let host = crate::audio_toolkit::get_cpal_host();
            let devices = host.output_devices()?;

            let mut found_device = None;
            for device in devices {
                if device.name()? == device_name {
                    found_device = Some(device);
                    break;
                }
            }

            match found_device {
                Some(device) => OutputStreamBuilder::from_device(device)?,
                None => {
                    warn!("Device '{}' not found, using default device", device_name);
                    OutputStreamBuilder::from_default_device()?
                }
            }
        }
    } else {
        debug!("Using default device");
        OutputStreamBuilder::from_default_device()?
    };
    Ok(stream_builder)
}

fn play_audio_file(
    path: &std::path::Path,
    selected_device: Option<String>,
    volume: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let stream_builder = stream_builder_for_device(selected_device)?;

    let stream_handle = stream_builder.open_stream()?;
    let mixer = stream_handle.mixer();

    let file = File::open(path)?;
    let buf_reader = BufReader::new(file);

    let sink = rodio::play(mixer, buf_reader)?;
    sink.set_volume(volume);
    sink.sleep_until_end();

    Ok(())
}

#[cfg(test)]
mod sound_path_tests {
    use super::*;

    // T-114 finding #5: custom sounds must route through the portable-aware
    // resolver, bundled themes must not. These tests exercise the pure
    // branch-selection decision (`is_custom_sound`) and the file-name half
    // of `get_sound_path` directly, without needing an `AppHandle` — the
    // `AppHandle`-dependent half of `resolve_sound_path` (actually calling
    // `resolve_app_data_dir` / `BaseDirectory::Resource`) isn't unit
    // testable without standing up a Tauri app, so it's covered by the
    // portable.rs resolver tests instead.

    #[test]
    fn custom_theme_is_flagged_custom_sound() {
        let mut settings = settings::get_default_settings();
        settings.sound_theme = SoundTheme::Custom;
        assert!(is_custom_sound(&settings));
    }

    #[test]
    fn builtin_themes_are_not_custom_sound() {
        let mut settings = settings::get_default_settings();
        settings.sound_theme = SoundTheme::Marimba;
        assert!(!is_custom_sound(&settings));

        settings.sound_theme = SoundTheme::Pop;
        assert!(!is_custom_sound(&settings));
    }

    #[test]
    fn custom_sound_file_names_have_no_resource_prefix() {
        // Custom sounds are joined directly onto the resolved app-data dir
        // (dir.join(sound_file)) — unlike the bundled "resources/..." paths
        // below, they must NOT carry a "resources/" prefix, or they'd
        // resolve to <data_dir>/resources/custom_start.wav instead of the
        // flat <data_dir>/custom_start.wav the rest of the app (and
        // commands/audio.rs::custom_sound_exists) expects.
        let mut settings = settings::get_default_settings();
        settings.sound_theme = SoundTheme::Custom;
        assert_eq!(
            get_sound_path(&settings, SoundType::Start),
            "custom_start.wav"
        );
        assert_eq!(
            get_sound_path(&settings, SoundType::Stop),
            "custom_stop.wav"
        );
    }

    #[test]
    fn builtin_theme_sound_paths_keep_resources_prefix() {
        let mut settings = settings::get_default_settings();
        settings.sound_theme = SoundTheme::Marimba;
        assert_eq!(
            get_sound_path(&settings, SoundType::Start),
            "resources/marimba_start.wav"
        );
        assert_eq!(
            get_sound_path(&settings, SoundType::Stop),
            "resources/marimba_stop.wav"
        );
    }
}
