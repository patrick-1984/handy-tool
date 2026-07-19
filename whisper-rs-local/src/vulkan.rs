use crate::common_logging::generic_error;
use std::ffi::CStr;
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;
use whisper_rs_sys::{
    ggml_backend_vk_get_device_count, ggml_backend_vk_get_device_description,
    ggml_backend_vk_get_device_memory,
};

#[derive(Debug, Clone)]
pub struct VKVram {
    pub free: usize,
    pub total: usize,
}

/// Human-readable device information
#[derive(Debug, Clone)]
pub struct VkDeviceInfo {
    pub id: i32,
    pub name: String,
    pub vram: VKVram,
}

/// Process-wide lock serializing every interaction this module has with
/// ggml's Vulkan backend (adversarial review finding 4, T-212 follow-up).
///
/// `ggml-vulkan.cpp` guards its one-time backend init with an
/// *unsynchronized* global flag that it marks complete before the device
/// vectors it populates are fully written. If `list_devices()` runs on one
/// thread while a model load is initializing the same backend on another,
/// the enumeration can observe partial state (a data race), or the
/// in-progress init can be disturbed by a concurrent query. `list_devices()`
/// below takes this lock for its full FFI section.
///
/// Note this module cannot, by itself, close the race against a Whisper GPU
/// model load: that load happens in `whisper_ctx.rs`'s
/// `WhisperContext::new_with_params`, which does not (and — per this pass's
/// file ownership — could not be changed to) take this same lock. See
/// `with_vulkan_lock` below and `managers/transcription.rs` in the `handy`
/// crate (which mirrors an equivalent lock at the one layer it owns, since
/// `whisper-rs` is not a direct dependency of that crate and this static
/// cannot be shared across the crate boundary without adding one — see
/// `tickets/T-212-gpu-selection.md` for the full note).
///
/// Not reentrant: never call `list_devices()` (or [`with_vulkan_lock`])
/// while already holding this lock on the same thread — `std::sync::Mutex`
/// is not reentrant and a nested acquisition deadlocks.
pub static VULKAN_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` while holding [`VULKAN_LOCK`]. Exposed so any future caller
/// within this crate that talks to the Vulkan backend can serialize against
/// `list_devices()` without duplicating the lock.
///
/// Recovers from a poisoned lock (a previous holder panicked while holding
/// it) instead of propagating the poison — refusing to serialize further
/// Vulkan access because of one earlier panic would be worse than
/// proceeding with a possibly-stale `()` guard.
pub fn with_vulkan_lock<R>(f: impl FnOnce() -> R) -> R {
    let _guard = VULKAN_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f()
}

/// Enumerate every physical GPU ggml can see.
///
/// Note: integrated GPUs are returned *after* discrete ones, mirroring
/// ggml's C logic.
///
/// Takes [`VULKAN_LOCK`] for the full call (adversarial review finding 4a)
/// so two concurrent `list_devices()` calls can't race each other's FFI
/// section. Enumerates name + memory only — deliberately does NOT call
/// `ggml_backend_vk_buffer_type`, which fully initializes the device; a
/// list-devices call is meant to be a cheap, side-effect-free query
/// (finding 4c). Callers that need a buffer type for a chosen device should
/// request it explicitly at load time, not get it pre-computed for every
/// adapter on every enumeration.
///
/// The FFI calls are additionally run behind `catch_unwind` (finding 4b): if
/// something below panics (or otherwise unwinds), it's contained here and
/// turned into an empty list rather than crossing back out of this
/// function. This is defense in depth, not a guarantee — a genuine C++
/// exception unwinding through the `extern "C"` boundary into
/// `ggml_backend_vk_get_device_count`/`_description`/`_memory` is technically
/// UB regardless of `catch_unwind` on the Rust side, and in a `panic =
/// "abort"` build (this workspace's release profile) a Rust-level panic
/// aborts the process before `catch_unwind` ever runs — it only has an
/// effect in unwind-strategy builds (dev builds, `cargo test`). ggml's
/// Vulkan backend is expected to report driver failures via return
/// values/asserts rather than throwing, but should that assumption ever be
/// wrong in a build where unwinding is possible, this keeps the failure
/// from silently corrupting caller state instead of visibly propagating.
pub fn list_devices() -> Vec<VkDeviceInfo> {
    let _guard = VULKAN_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        let n = ggml_backend_vk_get_device_count();
        (0..n)
            .map(|id| {
                // 256 bytes is plenty (spec says 128 is enough)
                let mut tmp: [libc::c_char; 256] = [0; 256];
                ggml_backend_vk_get_device_description(id as c_int, tmp.as_mut_ptr(), tmp.len());
                let mut free = 0usize;
                let mut total = 0usize;
                ggml_backend_vk_get_device_memory(id, &mut free, &mut total);
                VkDeviceInfo {
                    id,
                    name: CStr::from_ptr(tmp.as_ptr()).to_string_lossy().into_owned(),
                    vram: VKVram { free, total },
                }
            })
            .collect::<Vec<_>>()
    }));

    result.unwrap_or_else(|_| {
        generic_error!(
            "ggml Vulkan device enumeration panicked/unwound; returning an empty device list"
        );
        Vec::new()
    })
}

#[cfg(test)]
mod vulkan_tests {
    use super::*;

    #[test]
    fn enumerate_must_not_panic() {
        let _ = list_devices();
    }

    #[test]
    fn sane_device_info() {
        let gpus = list_devices();
        let mut seen = std::collections::HashSet::new();

        for dev in &gpus {
            assert!(seen.insert(dev.id), "duplicated id {}", dev.id);
            assert!(!dev.name.trim().is_empty(), "GPU {} has empty name", dev.id);
            assert!(
                dev.vram.total >= dev.vram.free,
                "GPU {} total < free",
                dev.id
            );
        }
    }

    #[test]
    fn with_vulkan_lock_runs_closure_and_returns_value() {
        let v = with_vulkan_lock(|| 42);
        assert_eq!(v, 42);
    }

    #[test]
    fn list_devices_releases_lock_before_returning() {
        // Regression guard for finding 4a: list_devices() must not hold
        // VULKAN_LOCK past its own return, or any caller that needs the
        // lock afterward (e.g. a model load) would deadlock/stall forever.
        let _ = list_devices();
        assert!(
            VULKAN_LOCK.try_lock().is_ok(),
            "list_devices() must release VULKAN_LOCK before returning"
        );
    }

    #[test]
    fn with_vulkan_lock_recovers_from_poison() {
        // A panic while the lock is held (e.g. from some future caller of
        // with_vulkan_lock) poisons the Mutex. Later callers must still be
        // able to proceed rather than propagate the poison forever.
        let poisoned = catch_unwind(AssertUnwindSafe(|| {
            with_vulkan_lock(|| panic!("simulated panic while holding the vulkan lock"));
        }));
        assert!(poisoned.is_err());

        let v = with_vulkan_lock(|| 7);
        assert_eq!(v, 7);
    }
}
