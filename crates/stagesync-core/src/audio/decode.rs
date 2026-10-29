//! Streaming audio decoding via Symphonia.
//!
//! Supports the formats a concert workflow actually needs:
//!
//! * WAV (PCM 16/24/32-bit integer **and** 32-bit float) — soundboard / field recorders,
//! * AAC-LC in MP4/M4A — watch and phone recorders,
//! * FLAC — lossless captures.
//!
//! Decoding is strictly **sequential** and **bounded-memory**: the caller pulls blocks of
//! frames, and the decoder never materialises the whole file. A 2 h 41 m float32 mono
//! recording (1.86 GB) is processed with a few tens of MiB of resident memory.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::{AudioDecoder as SymphoniaAudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use super::hash::sha256_file;

/// Static description of an ingested source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMeta {
    /// Absolute path the file was read from.
    pub path: PathBuf,
    /// File name only (for display).
    pub file_name: String,
    /// SHA-256 of the original bytes; used to prove the original was not modified.
    pub sha256: String,
    /// Size of the original file in bytes.
    pub byte_len: u64,
    /// Human-readable codec name (e.g. `"pcm_s32le"`, `"aac"`).
    pub codec: String,
    /// Nominal sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: usize,
    /// Decoded length in seconds.
    pub duration_secs: f64,
    /// Decoded length in frames (per channel).
    pub frames: u64,
    /// Original sample format, when known (e.g. `"f32"`, `"s16"`).
    pub sample_format: Option<String>,
    /// Original bit depth, when known.
    pub bits_per_sample: Option<u32>,
    /// True when the source is a floating-point WAV (levels are absolute, not dBFS-referenced).
    pub float_format: bool,
}

/// A block of decoded audio frames, one plane per channel.
///
/// All planes have identical length. The frame index of the first frame is carried
/// explicitly so that callers can map decoded audio back onto a timeline even if they
/// consume blocks out of order.
#[derive(Debug, Clone)]
pub struct DecodedBlock {
    /// Frame index (per channel) of the first frame in this block.
    pub start_frame: u64,
    /// Per-channel sample planes.
    pub planes: Vec<Vec<f32>>,
}

impl DecodedBlock {
    /// Number of channels.
    pub fn channels(&self) -> usize {
        self.planes.len()
    }

    /// Number of frames in this block.
    pub fn frames(&self) -> usize {
        self.planes.first().map_or(0, |p| p.len())
    }

    /// Mono mixdown (arithmetic mean across channels).
    ///
    /// For mono sources this is a copy. For multi-channel sources it is the mean of the
    /// channels, which preserves relative level between the channels without biasing the
    /// envelope towards any single one.
    pub fn mono(&self) -> Vec<f32> {
        let n = self.frames();
        let mut out = vec![0.0f32; n];
        if self.planes.is_empty() {
            return out;
        }
        let inv = 1.0 / self.planes.len() as f32;
        for plane in &self.planes {
            for (i, &s) in plane.iter().enumerate() {
                out[i] += s * inv;
            }
        }
        out
    }
}

/// A sequential, bounded-memory audio decoder.
pub struct AudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaAudioDecoder>,
    track_id: u32,
    meta: SourceMeta,
    frames_decoded: u64,
    /// Accumulated planes for the block currently being assembled.
    pending: Vec<Vec<f32>>,
    pending_frames: usize,
    eof: bool,
    /// Frames the container claims it holds (may be `None` for streaming formats).
    expected_frames: Option<u64>,
}

impl AudioDecoder {
    /// Probe and open a media file for sequential decoding.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("failed to open audio file: {}", path.display()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| anyhow!("failed to probe audio file {}: {}", path.display(), e))?;

        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| anyhow!("no audio track found in {}", path.display()))?;
        let track_id = track.id;
        let expected_frames = track.num_frames.filter(|&n| n > 0);

        let codec_params = track
            .codec_params
            .clone()
            .ok_or_else(|| anyhow!("audio track in {} has no codec parameters", path.display()))?;

        let (codec_name, sample_rate, channels, sample_format, bits_per_sample) =
            match &codec_params {
                CodecParameters::Audio(p) => (
                    format!("{:?}", p.codec).to_lowercase(),
                    p.sample_rate.unwrap_or(0),
                    p.channels.as_ref().map_or(0, |c| c.count()),
                    p.sample_format.map(|f| format!("{f:?}").to_lowercase()),
                    p.bits_per_sample,
                ),
                _ => {
                    return Err(anyhow!(
                        "unsupported (non-audio) track in {}",
                        path.display()
                    ))
                }
            };

        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(
                match &codec_params {
                    CodecParameters::Audio(p) => p,
                    _ => unreachable!("checked above"),
                },
                &AudioDecoderOptions::default(),
            )
            .map_err(|e| anyhow!("no decoder for {}: {}", path.display(), e))?;

        let byte_len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let sha256 = sha256_file(path)?;

        let float_format = matches!(sample_format.as_deref(), Some("f32") | Some("f64"));
        let mut meta = SourceMeta {
            path: path.to_path_buf(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
            sha256,
            byte_len,
            codec: codec_name,
            sample_rate,
            channels,
            duration_secs: 0.0,
            frames: expected_frames.unwrap_or(0),
            sample_format,
            bits_per_sample,
            float_format,
        };

        // Prefer the container's frame count when it is available; it is more reliable than
        // anything we can infer from bit-rate.
        if let Some(n) = expected_frames {
            if sample_rate > 0 {
                meta.duration_secs = n as f64 / sample_rate as f64;
            }
        }

        Ok(Self {
            format,
            decoder,
            track_id,
            meta,
            frames_decoded: 0,
            pending: Vec::new(),
            pending_frames: 0,
            eof: false,
            expected_frames,
        })
    }

    /// Metadata for the open file.
    pub fn meta(&self) -> &SourceMeta {
        &self.meta
    }

    /// Number of frames decoded so far.
    pub fn frames_decoded(&self) -> u64 {
        self.frames_decoded
    }

    /// Read the next block of decoded audio, accumulating until at least `min_frames`
    /// frames are available.
    ///
    /// Returns `None` at end of stream. The returned block is guaranteed to be non-empty.
    pub fn read_block(&mut self, min_frames: usize) -> Result<Option<DecodedBlock>> {
        let min_frames = min_frames.max(1);
        while self.pending_frames < min_frames && !self.eof {
            self.decode_next_packet()?;
        }
        if self.pending_frames == 0 {
            return Ok(None);
        }
        let start_frame = self.frames_decoded - self.pending_frames as u64;
        let planes = std::mem::take(&mut self.pending);
        self.pending_frames = 0;
        Ok(Some(DecodedBlock {
            start_frame,
            planes,
        }))
    }

    /// Decode packets until the stream ends, handing each decoded block to `sink`.
    ///
    /// This is the workhorse for the feature-extraction stage: it keeps memory bounded and
    /// avoids the caller having to manage block boundaries.
    pub fn for_each_block<F>(&mut self, min_frames: usize, mut sink: F) -> Result<u64>
    where
        F: FnMut(&DecodedBlock) -> Result<()>,
    {
        let mut total = 0u64;
        while let Some(block) = self.read_block(min_frames)? {
            total += block.frames() as u64;
            sink(&block)?;
        }
        Ok(total)
    }

    fn decode_next_packet(&mut self) -> Result<()> {
        if self.eof {
            return Ok(());
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.eof = true;
                    break;
                }
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    self.eof = true;
                    break;
                }
                Err(e) => {
                    return Err(anyhow!("error reading audio packet: {}", e));
                }
            };

            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    // Convert out of the decoder's borrow before touching `self` again.
                    let planes = planes_of(&buf);
                    self.extend(planes);
                    // One packet, one call. The loop is only for skipping packets that
                    // belong to another track and for recovering from a corrupt packet.
                    // Decoding on without stopping here would pull the *whole file*
                    // into memory in a single call, which is exactly what the caller
                    // asked not to happen.
                    break;
                }
                Err(SymphoniaError::DecodeError(_)) | Err(SymphoniaError::ResetRequired) => {
                    // A single corrupt packet must not abort a two-hour file: reset the
                    // decoder and continue with the next packet.
                    self.decoder.reset();
                    continue;
                }
                Err(e) => {
                    return Err(anyhow!("error decoding audio packet: {}", e));
                }
            }
        }
        Ok(())
    }

    fn extend(&mut self, planes: Vec<Vec<f32>>) {
        let frames = planes.first().map_or(0, |p| p.len());
        if frames == 0 {
            return;
        }
        if self.pending.is_empty() {
            self.pending = vec![Vec::new(); planes.len()];
        }
        for (dst, src) in self.pending.iter_mut().zip(planes.iter()) {
            dst.extend_from_slice(src);
        }
        self.pending_frames += frames;
        self.frames_decoded += frames as u64;
    }

    /// Finalise metadata once the stream has been consumed.
    pub fn finish(&mut self) {
        if self.meta.duration_secs == 0.0 && self.meta.sample_rate > 0 {
            self.meta.duration_secs = self.frames_decoded as f64 / self.meta.sample_rate as f64;
            self.meta.frames = self.frames_decoded;
        }
        let _ = self.expected_frames;
    }
}

/// Convert a decoded buffer into per-channel `f32` planes.
fn planes_of(buf: &GenericAudioBufferRef<'_>) -> Vec<Vec<f32>> {
    let frames = buf.frames();
    let planes = buf.num_planes();
    if frames == 0 || planes == 0 {
        return Vec::new();
    }
    let mut out: Vec<Vec<f32>> = vec![Vec::with_capacity(frames); planes];
    buf.copy_to_vecs_planar(&mut out);
    out
}
