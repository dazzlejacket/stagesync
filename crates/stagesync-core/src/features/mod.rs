//! Frame-level audio features.
//!
//! All features are defined in **physical units** (seconds, hertz) rather than in samples,
//! so that a 44.1 kHz recording and a 48 kHz recording of the same performance produce
//! directly comparable feature sequences. A change of sample rate is therefore never
//! mistaken for clock drift.

pub mod envelope;
pub mod mel;

pub use envelope::{Envelope, EnvelopeConfig, EnvelopeExtractor};
pub use mel::{LogMel, LogMelConfig};
