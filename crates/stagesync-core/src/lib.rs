//! **StageSync core** — drift-aware synchronisation of multiple recordings of the same
//! performance.
//!
//! The crate is deliberately split so that every stage is independently testable:
//!
//! * [`audio`]     — probing, decoding and hashing of source media (never mutated).
//! * [`features`]  — rate-agnostic time-domain and spectral descriptors.
//! * [`matching`]  — coarse global lock, fine alignment, robust drift model, segmentation.
//! * [`resample`]  — deterministic polyphase windowed-sinc variable-rate resampler.
//! * [`render`]    — correction rendering (source-aligned and reference-timeline layouts).
//! * [`calibration`] — end-to-end calibration loop (measure → correct → re-measure).
//! * [`verify`]    — independent holdout validation (waveform GCC-PHAT).
//! * [`export`]    — WAV / CSV / JSON / graph artefacts.
//!
//! ## Coordinate system
//!
//! Every model in this crate uses one convention, defined once and used everywhere:
//!
//! ```text
//! reference_time_seconds = scale * source_time_seconds + offset_seconds
//! drift_ppm               = (scale - 1) * 1_000_000
//! ```
//!
//! `scale > 1` means the source clock runs *fast* (its material is compressed in time
//! relative to the reference), `scale < 1` means it runs slow. `offset` is the reference
//! time that corresponds to source time zero.

pub mod audio;
pub mod calibration;
pub mod export;
pub mod features;
pub mod matching;
pub mod pipeline;
pub mod render;
pub mod resample;
pub mod verify;

pub use audio::{AudioDecoder, SourceMeta};
pub use matching::{
    AlignmentReport, Confidence, LinearModel, MatchPoint, Segment, SourceAlignment, TimeMap,
};

/// Crate version string, surfaced in every exported report.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Name of the application, surfaced in every exported report.
pub const APP_NAME: &str = "StageSync";
