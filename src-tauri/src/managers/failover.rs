//! Transcription failover: when the dictation model cannot transcribe a take
//! (its server is down, the network dropped, a request failed mid-take), the
//! take - or the chunk - is transcribed by the next model in the user's order
//! (`transcription_fallbacks`: up to two more, any engine Handy has: an online
//! one, the Custom STT server, a local model).
//!
//! - **Heartbeat.** When a take starts and fallbacks are set, an online
//!   dictation model is probed in the background (`GET {base}/models`, 3 s).
//!   A server that answers at all (any status below 500) is up; no answer is
//!   down, and the take goes straight to the first fallback instead of
//!   waiting for a request to fail.
//! - **Mid-take.** A failed request marks the dictation model down for the
//!   rest of the take, so the following chunks switch at once.
//! - The model that actually transcribed is what History shows, and the app
//!   is told (`transcription-fallback`) so it can say so.

use crate::managers::model::{EngineType, ModelManager};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{AppSettings, get_settings};
use log::{info, warn};
use serde::Serialize;
use specta::Type;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// At most this many models after the dictation model.
pub const MAX_FALLBACKS: usize = 2;

/// Told to the app when a take (or a chunk) switched model.
#[derive(Clone, Debug, Serialize, Type)]
pub struct FallbackEvent {
    /// Display name of the dictation model that failed.
    pub from: String,
    /// Display name of the model that transcribed instead.
    pub to: String,
    pub reason: String,
}

#[derive(Default)]
struct TakeState {
    /// The take this state belongs to: a number unique to each take (never
    /// reused, unlike the recording's second-resolution timestamp).
    take: u64,
    /// `Some(false)` = the dictation model is down for this take.
    primary_up: Option<bool>,
    /// Display label of the fallback that transcribed (for History).
    used_label: Option<String>,
    /// The app was told about this take's switch already.
    announced: bool,
}

static TAKE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

static STATE: Mutex<TakeState> = Mutex::new(TakeState {
    take: 0,
    primary_up: None,
    used_label: None,
    announced: false,
});

fn state() -> std::sync::MutexGuard<'static, TakeState> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

/// The fallbacks to try, in order: known, usable, not the dictation model,
/// no repeats, at most `MAX_FALLBACKS`.
pub fn fallback_chain(settings: &AppSettings, usable: impl Fn(&str) -> bool) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    for id in &settings.transcription_fallbacks {
        let id = id.trim();
        if id.is_empty() || id == settings.selected_model || chain.iter().any(|c| c == id) {
            continue;
        }
        if usable(id) {
            chain.push(id.to_string());
        }
        if chain.len() == MAX_FALLBACKS {
            break;
        }
    }
    chain
}

/// Validated list for the setting: trimmed, no blanks or repeats, at most
/// `MAX_FALLBACKS`; unknown ids refused.
pub fn validate_fallbacks(
    models: &[String],
    known: impl Fn(&str) -> bool,
) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for id in models {
        let id = id.trim();
        if id.is_empty() || out.iter().any(|o| o == id) {
            continue;
        }
        if !known(id) {
            return Err(format!("Unknown model: {id}"));
        }
        out.push(id.to_string());
    }
    if out.len() > MAX_FALLBACKS {
        return Err(format!("At most {MAX_FALLBACKS} fallback models"));
    }
    Ok(out)
}

/// The base URL a heartbeat checks for an online engine, with its key.
fn probe_target(settings: &AppSettings, engine: &EngineType) -> Option<(String, String)> {
    let (url, key) = match engine {
        EngineType::ApiWhisper => (
            &settings.api_transcription_url,
            &settings.api_transcription_key,
        ),
        EngineType::SttServer => (&settings.stt_server_url, &settings.stt_server_key),
        EngineType::OpenRouterWhisper => (
            &settings.openrouter_transcription_url,
            &settings.openrouter_transcription_key,
        ),
        _ => return None,
    };
    let url = url.trim().trim_end_matches('/');
    (!url.is_empty()).then(|| (format!("{url}/models"), key.trim().to_string()))
}

/// Is the server there? Any HTTP answer below 500 counts (a 401 or 404 still
/// means it is reachable); no answer, or a 5xx, is down.
fn probe(url: &str, key: &str) -> Result<(), String> {
    let mut req = crate::net::ureq_agent(url)
        .get(url)
        .timeout(Duration::from_secs(3));
    if !key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    match req.call() {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, _)) if code < 500 => Ok(()),
        Err(ureq::Error::Status(code, _)) => Err(format!("the server answered {code}")),
        Err(e) => Err(e.to_string()),
    }
}

/// The current take's number (capture it where the take's work is set up).
pub fn current_take() -> u64 {
    state().take
}

/// A new take: forget the last one and, when fallbacks are set and the
/// dictation model is online, check it in the background.
pub fn start_take(app: &AppHandle) {
    let take = TAKE_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    *state() = TakeState {
        take,
        ..Default::default()
    };
    let settings = get_settings(app);
    if settings.transcription_fallbacks.is_empty() {
        return;
    }
    let Some(models) = app.try_state::<Arc<ModelManager>>() else {
        return;
    };
    let Some(info) = models.get_model_info(&settings.selected_model) else {
        return;
    };
    let Some((url, key)) = probe_target(&settings, &info.engine_type) else {
        return;
    };
    let name = info.name.clone();
    std::thread::spawn(move || {
        let up = probe(&url, &key);
        let mut st = state();
        if st.take != take {
            return; // a newer take started meanwhile
        }
        match up {
            Ok(()) => {
                if st.primary_up.is_none() {
                    st.primary_up = Some(true);
                }
            }
            Err(e) => {
                warn!(
                    "Failover: {name} did not answer the heartbeat ({e}); this take uses the next model"
                );
                st.primary_up = Some(false);
            }
        }
    });
}

/// Label of the fallback that transcribed this take, if any (History).
pub fn used_label() -> Option<String> {
    state().used_label.clone()
}

fn model_name(app: &AppHandle, id: &str) -> String {
    app.try_state::<Arc<ModelManager>>()
        .and_then(|m| m.get_model_info(id).map(|i| i.name))
        .unwrap_or_else(|| id.to_string())
}

/// Transcribe `samples` with the dictation model, or - when it is down or
/// fails - with the fallbacks in order. The caller already holds the engine
/// lock (CHUNK_TRANSCRIBE_LOCK), as for `tm.transcribe`.
pub fn transcribe(
    app: &AppHandle,
    tm: &TranscriptionManager,
    take: u64,
    whole_take: bool,
    samples: Vec<f32>,
) -> anyhow::Result<String> {
    // `take` = the take this work belongs to (captured when it was set up),
    // so a chunk of an abandoned take that finishes later never touches the
    // next one: every write below checks it is still the current take.
    let settings = get_settings(app);
    let primary = settings.selected_model.clone();
    let primary_is_stt = app
        .try_state::<Arc<ModelManager>>()
        .and_then(|m| m.get_model_info(&primary))
        .is_some_and(|i| matches!(i.engine_type, EngineType::SttServer));
    // The dictation model's own call. On the STT server a take also brings
    // its speaker text, kept for this take only.
    // `whole_take`: the whole recording (single request, deferred); a piece of
    // a chunked take asks for plain text only (its speakers come from one
    // whole-take request after stop).
    let run_primary = |samples: Vec<f32>| -> anyhow::Result<String> {
        if primary_is_stt && whole_take {
            let result = tm.stt_server_take_expecting(&primary, samples)?;
            if state().take == take {
                crate::managers::stt_server::set_session_diarized(take, result.diarized);
            }
            Ok(result.text)
        } else {
            tm.transcribe(samples)
        }
    };
    let chain = match app.try_state::<Arc<ModelManager>>() {
        Some(models) => fallback_chain(&settings, |id| {
            crate::commands::models::model_usable(&models, &settings, id)
        }),
        None => Vec::new(),
    };
    if chain.is_empty() {
        return run_primary(samples);
    }
    // A whole-take STT request goes to the speakers model, which can be up
    // while the plain model (that marked the primary down) is not: try it.
    let primary_down = !(primary_is_stt && whole_take) && {
        let st = state();
        st.take == take && st.primary_up == Some(false)
    };
    let reason = if primary_down {
        "it did not answer at the start of the take".to_string()
    } else {
        match run_primary(samples.clone()) {
            Ok(text) => return Ok(text),
            Err(e) => {
                warn!("Failover: {primary} failed ({e}); trying the next model");
                let mut st = state();
                if st.take == take {
                    st.primary_up = Some(false);
                }
                e.to_string()
            }
        }
    };
    let mut last_error = anyhow::anyhow!("{reason}");
    for model in chain {
        match crate::commands::files::transcribe_samples_with(app, tm, &model, &samples, true) {
            Ok(text) => {
                let to = model_name(app, &model);
                info!("Failover: transcribed with {to} instead of {primary}");
                let announce = {
                    let mut st = state();
                    if st.take != take {
                        false
                    } else {
                        st.used_label = Some(format!("{to} — fallback"));
                        !std::mem::replace(&mut st.announced, true)
                    }
                };
                if announce {
                    let _ = app.emit(
                        "transcription-fallback",
                        FallbackEvent {
                            from: model_name(app, &primary),
                            to,
                            reason: reason.clone(),
                        },
                    );
                }
                return Ok(text);
            }
            Err(e) => {
                warn!("Failover: {model} failed too ({e})");
                last_error = anyhow::anyhow!("{e}");
            }
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    #[test]
    fn the_chain_skips_the_dictation_model_repeats_and_unusable_models() {
        let mut s = get_default_settings();
        s.selected_model = "api-whisper".into();
        s.transcription_fallbacks = vec![
            "api-whisper".into(),
            "stt-server".into(),
            "stt-server".into(),
            "broken".into(),
            "parakeet-tdt-0.6b-v3".into(),
            "small".into(),
        ];
        let chain = fallback_chain(&s, |id| id != "broken");
        assert_eq!(chain, vec!["stt-server", "parakeet-tdt-0.6b-v3"]);
        s.transcription_fallbacks.clear();
        assert!(fallback_chain(&s, |_| true).is_empty());
    }

    #[test]
    fn the_setting_is_validated() {
        let known = |id: &str| id != "nope";
        assert_eq!(
            validate_fallbacks(&[" a ".into(), "a".into(), "".into(), "b".into()], known).unwrap(),
            vec!["a", "b"]
        );
        assert!(validate_fallbacks(&["nope".into()], known).is_err());
        assert!(validate_fallbacks(&["a".into(), "b".into(), "c".into()], known).is_err());
    }

    #[test]
    fn heartbeat_targets_online_engines_only() {
        let mut s = get_default_settings();
        s.stt_server_url = "http://pc:8000/v1/".into();
        s.stt_server_key = "k".into();
        assert_eq!(
            probe_target(&s, &EngineType::SttServer),
            Some(("http://pc:8000/v1/models".into(), "k".into()))
        );
        s.api_transcription_url = String::new();
        assert_eq!(probe_target(&s, &EngineType::ApiWhisper), None);
        assert_eq!(probe_target(&s, &EngineType::Parakeet), None);
    }
}
