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

        let (sample_rate, channels, sample_format, bits_per_sample) = match &codec_params {
            CodecParameters::Audio(p) => (
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

        // A file with no sample rate, or with one so low that the analysis
        // has no band to work with, cannot be analysed. It is refused here,
        // with a message that says what is wrong, rather than being allowed
        // to reach the filterbank and fail there.
        if sample_rate == 0 {
            return Err(anyhow!(
                "{} claims a sample rate of zero, so it holds no timing information",
                path.display()
            ));
        }

        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(
                match &codec_params {
                    CodecParameters::Audio(p) => p,
                    _ => unreachable!("checked above"),
                },
                &AudioDecoderOptions::default(),
            )
            .map_err(|e| anyhow!("no decoder for {}: {}", path.display(), e))?;
        let codec_info = decoder.codec_info();
        let codec_name = human_codec_name(codec_info.short_name, codec_info.long_name);

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

    /// Move the read position to a frame, and report the frame actually reached.
    ///
    /// Decoding is sequential, so this is what makes it possible to *start* at a
    /// position rather than at the beginning of the recording: a player that has to
    /// decode two hours of material to reach the middle of a show is not a player.
    ///
    /// The frame that is reached is not always the one that was asked for. A
    /// container can only seek to a packet boundary, so the packets between the
    /// boundary and the requested frame are decoded and dropped, and the frame the
    /// read position ends up on is reported back.
    pub fn seek_to_frame(&mut self, frame: u64) -> Result<u64> {
        use symphonia::core::formats::{SeekMode, SeekTo};
        use symphonia::core::units::Timestamp;

        // Anything that was decoded but not handed over is no longer wanted.
        self.pending.clear();
        self.pending_frames = 0;
        self.eof = false;

        // A position past the end of the recording is a position at its end, not an
        // error: a playhead that runs off the end of the show should stop there.
        let frame = match self.meta.frames {
            0 => frame,
            total => frame.min(total.saturating_sub(1)),
        };

        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts: Timestamp::new(frame as i64),
                    track_id: self.track_id,
                },
            )
            .map_err(|error| anyhow!("failed to seek in the file: {error}"))?;
        // The codec's state belongs to the old position.
        self.decoder.reset();

        // The track's time base is one tick per frame for uncompressed audio, so the
        // timestamp of the seek is the frame it landed on.
        self.frames_decoded = seeked.actual_ts.get().max(0) as u64;

        // Decode forward to the requested frame, dropping what comes before it.
        while self.frames_decoded < frame {
            let before_packet = self.frames_decoded;
            self.decode_next_packet()?;
            let produced = self.frames_decoded - before_packet;
            if produced == 0 {
                // The end of the file was reached before the frame was.
                break;
            }
            let before_target = (frame - before_packet).min(produced) as usize;
            if before_target > 0 {
                for plane in self.pending.iter_mut() {
                    plane.drain(..before_target);
                }
                self.pending_frames -= before_target;
            }
        }
        Ok(self.frames_decoded)
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

/// Map Symphonia's codec short/long names to a clean human-readable label.
fn human_codec_name(short_name: &str, long_name: &str) -> String {
    match short_name.to_ascii_lowercase().as_str() {
        "pcm_f32le" | "pcm_f32be" => "PCM 32-bit float".to_string(),
        "pcm_f64le" | "pcm_f64be" => "PCM 64-bit float".to_string(),
        "pcm_s32le" | "pcm_s32be" => "PCM 32-bit".to_string(),
        "pcm_s24le" | "pcm_s24be" => "PCM 24-bit".to_string(),
        "pcm_s16le" | "pcm_s16be" => "PCM 16-bit".to_string(),
        "pcm_u8" | "pcm_s8" => "PCM 8-bit".to_string(),
        "aac" => "AAC".to_string(),
        "flac" => "FLAC".to_string(),
        "vorbis" => "Vorbis".to_string(),
        "mp3" => "MP3".to_string(),
        "opus" => "Opus".to_string(),
        "alac" => "ALAC".to_string(),
        _ if !long_name.is_empty() => long_name.to_string(),
        _ => short_name.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::wav::{WavFormat, WavWriter};

    /// A file whose every sample says which frame it is, so a seek can be checked
    /// against the truth rather than against another seek.
    fn ramp_file(path: &Path, rate: u32, frames: usize) {
        let samples: Vec<f32> = (0..frames).map(|i| (i % 1000) as f32 / 1000.0).collect();
        let mut writer = WavWriter::create(path, rate, 1, WavFormat::Float32).expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");
    }

    #[test]
    fn a_seek_lands_on_the_frame_it_asked_for() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("ramp.wav");
        let rate = 8000u32;
        let frames = 40_000usize;
        ramp_file(&path, rate, frames);

        let mut decoder = AudioDecoder::open(&path).expect("open");
        for target in [0u64, 1, 512, 12_345, 30_000] {
            let reached = decoder.seek_to_frame(target).expect("seek");
            assert!(
                reached >= target,
                "the seek to {target} landed before it, on {reached}"
            );
            let block = decoder.read_block(64).expect("read").expect("a block");
            assert_eq!(
                block.start_frame, target,
                "the block after a seek to {target} starts at {}",
                block.start_frame
            );
            let first = block.planes[0][0];
            let expected = (target % 1000) as f32 / 1000.0;
            assert!(
                (first - expected).abs() < 1e-6,
                "the first sample after a seek to {target} was {first}, not {expected}"
            );
        }
    }

    #[test]
    fn a_seek_past_the_end_of_the_file_stops_at_the_end() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("ramp.wav");
        ramp_file(&path, 8000, 1000);

        let mut decoder = AudioDecoder::open(&path).expect("open");
        let reached = decoder.seek_to_frame(100_000).expect("seek");
        assert!(
            reached <= 1000,
            "the seek past the end reached frame {reached}"
        );
    }

    #[test]
    fn codec_name_is_human_readable_not_raw_id() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("ramp.wav");
        ramp_file(&path, 8000, 1000);

        let decoder = AudioDecoder::open(&path).expect("open");
        let codec = &decoder.meta().codec;
        assert!(
            !codec.contains("audiocodecid"),
            "codec name leaked raw debug id: {codec}"
        );
        assert_eq!(codec, "PCM 32-bit float");
    }
}
