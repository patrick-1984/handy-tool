// Re-export all audio components
mod decode;
mod device;
mod incremental_wav;
mod opus_chunk;
mod recorder;
mod resampler;
mod utils;
mod visualizer;

pub use decode::{SUPPORTED_EXTENSIONS, decode_audio_file};
pub use device::{CpalDeviceInfo, list_input_devices, list_output_devices};
// `repair_wav_header` is retained for recovering legacy `*.recording.wav` files
// from before the chunked-Opus migration. `IncrementalWavWriter` is no longer
// used by the recorder but kept for that one-release migration window.
pub use incremental_wav::{IncrementalWavWriter, repair_wav_header};
pub use opus_chunk::{
    ClosedChunk, OpusChunkWriter, StartParams, encode_samples_to_ogg_opus, glue_chunks,
    opus_duration_seconds, repair_truncated_opus,
};
pub use recorder::AudioRecorder;
pub use resampler::FrameResampler;
pub use utils::{pad_trailing_silence, save_wav_file};
pub use visualizer::AudioVisualiser;
