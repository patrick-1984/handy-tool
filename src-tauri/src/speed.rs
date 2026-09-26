//! How fast this PC is transcribing right now compared with its own normal, for
//! the recording overlay's speed warning.
//!
//! "Normal" is the median of this model's last few transcriptions on this PC,
//! kept on disk so it survives restarts. Short clips (live previews) and long
//! ones (chunks, final passes) are kept apart: a long clip costs less per second
//! of audio, so mixing them would make every short one look slow. The overlay
//! only hears about it when the PC is clearly slower than usual.

use log::debug;
use once_cell::sync::Lazy;
use std::collections::BTreeMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

/// Measurements kept per model and clip length; "normal" is their median.
const HISTORY: usize = 20;
/// Too few measurements say nothing about normal yet.
const MIN_HISTORY: usize = 5;
/// Below this share of normal speed the overlay shows the warning.
const SLOW_PERCENT: u32 = 70;
/// Clips at least this long are "long" (chunks and final passes).
const LONG_CLIP_SECS: f64 = 10.0;
const FILE_NAME: &str = "transcription_speed.json";

/// Seconds of processing per second of audio, oldest first, per "model|short"
/// or "model|long". Loaded from disk on first use.
static HISTORY_BY_KEY: Lazy<Mutex<Option<BTreeMap<String, Vec<f64>>>>> =
    Lazy::new(|| Mutex::new(None));

fn load(app: &AppHandle) -> BTreeMap<String, Vec<f64>> {
    crate::portable::resolve_app_data_dir(app)
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join(FILE_NAME)).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, history: &BTreeMap<String, Vec<f64>>) {
    let Ok(dir) = crate::portable::resolve_app_data_dir(app) else {
        return;
    };
    match serde_json::to_string(history) {
        Ok(text) => {
            if let Err(e) = std::fs::write(dir.join(FILE_NAME), text) {
                debug!("Could not save transcription speeds: {e}");
            }
        }
        Err(e) => debug!("Could not encode transcription speeds: {e}"),
    }
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    sorted[sorted.len() / 2]
}

/// How this measurement compares with normal, in percent (100 = normal speed,
/// 50 = twice as slow). None while there is no normal to compare with yet.
fn percent_of_normal(history: &[f64], measured: f64) -> Option<u32> {
    if history.len() < MIN_HISTORY || measured <= 0.0 {
        return None;
    }
    Some((median(history) / measured * 100.0).round() as u32)
}

/// Record one finished transcription and tell the overlay whether the PC is
/// running slow: `Some(percent)` below SLOW_PERCENT of normal, otherwise None.
pub fn record(app: &AppHandle, model: &str, audio_secs: f64, secs_per_audio_sec: f64) {
    let length = if audio_secs >= LONG_CLIP_SECS {
        "long"
    } else {
        "short"
    };
    let key = format!("{model}|{length}");
    let slow = {
        let Ok(mut guard) = HISTORY_BY_KEY.lock() else {
            return;
        };
        let all = guard.get_or_insert_with(|| load(app));
        let history = all.entry(key).or_default();
        // Compared before it joins the history, so a slow spell does not
        // immediately become the new normal.
        let percent = percent_of_normal(history, secs_per_audio_sec);
        history.push(secs_per_audio_sec);
        if history.len() > HISTORY {
            history.remove(0);
        }
        save(app, all);
        percent.filter(|p| *p < SLOW_PERCENT)
    };
    let _ = app.emit("transcription-speed", slow);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_verdict_until_there_is_a_normal() {
        assert_eq!(percent_of_normal(&[0.2, 0.2, 0.2, 0.2], 0.4), None);
    }

    #[test]
    fn twice_as_slow_as_the_median_is_fifty_percent() {
        let history = [0.2, 0.19, 0.21, 0.2, 0.9, 0.2];
        assert_eq!(percent_of_normal(&history, 0.4), Some(50));
        assert_eq!(percent_of_normal(&history, 0.2), Some(100));
    }
}
