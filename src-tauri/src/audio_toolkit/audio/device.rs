use cpal::traits::{DeviceTrait, HostTrait};

pub struct CpalDeviceInfo {
    pub index: String,
    pub name: String,
    pub is_default: bool,
    pub device: cpal::Device,
}

pub fn list_input_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.input_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".into());

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default,
            device,
        });
    }

    Ok(out)
}

pub fn list_output_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host.default_output_device().and_then(|d| d.name().ok());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.output_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".into());

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default,
            device,
        });
    }

    Ok(out)
}

/// How a system-audio (loopback) capture source failed to resolve.
///
/// Every variant names the device the user asked for, because the whole point is
/// that a silently-wrong capture source is indistinguishable from a working one:
/// it produces a perfectly successful recording of nothing.
#[derive(Debug, Clone)]
pub enum CaptureSourceError {
    /// The configured endpoint is not among the active render endpoints.
    NotFound {
        requested: String,
    },
    /// Two or more active endpoints share that friendly name. Picking the first is
    /// exactly the unnoticeable-wrong-source failure this type exists to prevent.
    Ambiguous {
        requested: String,
        matches: usize,
    },
    /// No default playback endpoint at all (no speakers, no headset).
    NoDefaultPlayback,
    Enumeration(String),
}

impl std::fmt::Display for CaptureSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { requested } => write!(
                f,
                "Playback device '{requested}' was not found. Pick another system-audio source in Settings."
            ),
            Self::Ambiguous { requested, matches } => write!(
                f,
                "{matches} playback devices are named '{requested}'. Rename one in Windows Sound settings so Handy can tell them apart."
            ),
            Self::NoDefaultPlayback => write!(f, "No playback device is available to capture."),
            Self::Enumeration(e) => write!(f, "Could not list playback devices: {e}"),
        }
    }
}

impl std::error::Error for CaptureSourceError {}

/// Resolve the configured system-audio source to a render endpoint.
///
/// `None` means "follow the current default playback device", which is what makes a
/// Bluetooth A2DP<->HFP switch survivable: Windows moves the DEFAULT endpoint when a
/// call starts, so re-resolving per take follows it, whereas a pinned endpoint goes
/// silent. Resolution is deliberately strict - a miss or an ambiguous name is an
/// error, never a silent fallback to something that happens to be nearby.
pub fn resolve_system_audio_device(name: Option<&str>) -> Result<cpal::Device, CaptureSourceError> {
    let devices =
        list_output_devices().map_err(|e| CaptureSourceError::Enumeration(e.to_string()))?;

    let Some(want) = name else {
        return devices
            .into_iter()
            .find(|d| d.is_default)
            .map(|d| d.device)
            .ok_or(CaptureSourceError::NoDefaultPlayback);
    };

    let matches: Vec<_> = devices.into_iter().filter(|d| d.name == want).collect();
    match matches.len() {
        0 => Err(CaptureSourceError::NotFound {
            requested: want.to_string(),
        }),
        1 => Ok(matches.into_iter().next().unwrap().device),
        n => Err(CaptureSourceError::Ambiguous {
            requested: want.to_string(),
            matches: n,
        }),
    }
}
