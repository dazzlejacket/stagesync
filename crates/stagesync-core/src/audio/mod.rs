//! Audio ingest: probing, streaming decode and hashing.
//!
//! Source media is **never** modified. Decoding produces temporary `f32` PCM in memory
//! only; the original byte stream is left untouched on disk.

pub mod decode;
pub mod hash;
pub mod peaks;
pub mod wav;
pub mod windows;

pub use decode::{AudioDecoder, DecodedBlock, SourceMeta};
pub use hash::sha256_file;
pub use peaks::{PeakCache, PeakLevel, PeakWindow};
pub use wav::{ensure_wav_size_fits, WavFormat, WavReader, WavWriter, RIFF_MAX_DATA_BYTES};
pub use windows::{extract_windows, WindowRequest};
