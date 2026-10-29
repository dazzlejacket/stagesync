//! Streaming WAV writer and random-access WAV reader.
//!
//! Output preserves the **native channel count** of the source and the **native level**:
//! no normalisation, no automatic gain, no clipping beyond the representable range.
//! The sample rate of the output is chosen explicitly by the caller (the target file's
//! nominal rate for source-aligned exports, the reference rate for timeline exports).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::hash::sha256_file;

/// Output sample encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavFormat {
    /// 32-bit IEEE float — lossless for material that was already float.
    Float32,
    /// 16-bit signed PCM.
    Int16,
}

impl WavFormat {
    /// Short name used in reports.
    pub fn name(self) -> &'static str {
        match self {
            WavFormat::Float32 => "f32",
            WavFormat::Int16 => "i16",
        }
    }
}

/// Streaming WAV writer that accepts blocks of planar frames.
///
/// The samples are written to a temporary file beside the destination, and that
/// file is moved into place when the writer is finalised. A file at the
/// destination is therefore always a complete one: a render that fails half way
/// through leaves the destination untouched, and the temporary file is removed
/// when the writer is dropped.
pub struct WavWriter {
    /// The writer itself, taken out when the file is finalised.
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    format: WavFormat,
    channels: usize,
    sample_rate: u32,
    frames_written: u64,
    /// Where the finished file belongs.
    destination: PathBuf,
    /// The file being written to.
    temporary: PathBuf,
    /// Whether the file has been moved into place.
    committed: bool,
}

/// The name of the temporary file a destination is written through.
///
/// The name is hidden, and carries the process id, so that two exports running
/// at the same time do not write over one another's temporary file.
fn temporary_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{name}.{}.stagesync-part", std::process::id()))
}

impl WavWriter {
    /// Create a new WAV file.
    pub fn create(
        path: &Path,
        sample_rate: u32,
        channels: usize,
        format: WavFormat,
    ) -> Result<Self> {
        anyhow::ensure!(channels > 0, "cannot write a WAV with zero channels");
        anyhow::ensure!(
            sample_rate > 0,
            "cannot write a WAV with a zero sample rate"
        );
        let spec = hound::WavSpec {
            channels: channels as u16,
            sample_rate,
            bits_per_sample: match format {
                WavFormat::Float32 => 32,
                WavFormat::Int16 => 16,
            },
            sample_format: match format {
                WavFormat::Float32 => hound::SampleFormat::Float,
                WavFormat::Int16 => hound::SampleFormat::Int,
            },
        };
        let temporary = temporary_path(path);
        // The destination is named in the error rather than the temporary file: the
        // temporary name is a hidden, process-specific one that the user did not ask
        // for and cannot act on.
        let file = File::create(&temporary)
            .with_context(|| format!("failed to create WAV file: {}", path.display()))?;
        let writer = hound::WavWriter::new(BufWriter::with_capacity(1 << 20, file), spec)
            .with_context(|| format!("failed to initialise WAV file: {}", path.display()))?;
        Ok(Self {
            writer: Some(writer),
            format,
            channels,
            sample_rate,
            frames_written: 0,
            destination: path.to_path_buf(),
            temporary,
            committed: false,
        })
    }

    /// The writer, while the file is still being written.
    fn writer(&mut self) -> Result<&mut hound::WavWriter<BufWriter<File>>> {
        self.writer
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("the WAV file has already been finalised"))
    }

    /// Sample rate of the output file.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channel count of the output file.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Number of frames written so far.
    pub fn frames_written(&self) -> u64 {
        self.frames_written
    }

    /// Write one planar block (one plane per channel). All planes must be equal length.
    ///
    /// The planes may be anything that borrows as a slice of samples, so a caller does
    /// not have to copy its samples into a `Vec` just to write them.
    pub fn write_planar<P: AsRef<[f32]>>(&mut self, planes: &[P]) -> Result<()> {
        anyhow::ensure!(
            planes.len() == self.channels,
            "plane count {} does not match channel count {}",
            planes.len(),
            self.channels
        );
        let n = planes.first().map_or(0, |p| p.as_ref().len());
        for p in planes {
            anyhow::ensure!(
                p.as_ref().len() == n,
                "planar blocks must have equal length"
            );
        }
        match self.format {
            WavFormat::Float32 => {
                for i in 0..n {
                    for p in planes {
                        self.writer()?.write_sample(p.as_ref()[i])?;
                    }
                }
            }
            WavFormat::Int16 => {
                for i in 0..n {
                    for p in planes {
                        let v = (p.as_ref()[i] * 32767.0).round();
                        let v = v.clamp(-32768.0, 32767.0);
                        self.writer()?.write_sample(v as i16)?;
                    }
                }
            }
        }
        self.frames_written += n as u64;
        Ok(())
    }

    /// Write `frames` frames of digital silence.
    pub fn write_silence(&mut self, frames: usize) -> Result<()> {
        match self.format {
            WavFormat::Float32 => {
                for _ in 0..frames {
                    for _ in 0..self.channels {
                        self.writer()?.write_sample(0.0f32)?;
                    }
                }
            }
            WavFormat::Int16 => {
                for _ in 0..frames {
                    for _ in 0..self.channels {
                        self.writer()?.write_sample(0i16)?;
                    }
                }
            }
        }
        self.frames_written += frames as u64;
        Ok(())
    }

    /// Finalise the WAV header, flush it to disk, and move the file into place.
    pub fn finalize(mut self) -> Result<()> {
        let writer = self
            .writer
            .take()
            .ok_or_else(|| anyhow::anyhow!("the WAV file has already been finalised"))?;
        writer.finalize().with_context(|| {
            format!(
                "failed to finalise WAV file: {}",
                self.destination.display()
            )
        })?;
        std::fs::rename(&self.temporary, &self.destination).with_context(|| {
            format!(
                "failed to move the finished WAV file into place at {}",
                self.destination.display()
            )
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for WavWriter {
    /// Remove the temporary file when the writer was not finalised.
    ///
    /// The file at the destination is left alone, so a failed write never
    /// replaces a good export with a broken one.
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.temporary);
        }
    }
}

/// The integer sample width of a WAV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleKind {
    Float,
    Int8,
    Int16,
    Int32,
}

/// A random-access WAV reader, used by the verification stage.
///
/// Only WAV is supported here on purpose: the holdout verifier must be able to jump to
/// arbitrary positions in a multi-hour file without decoding it from the start.
pub struct WavReader {
    reader: hound::WavReader<BufReader<File>>,
    sample_rate: u32,
    channels: usize,
    frames: u64,
    kind: SampleKind,
}

impl WavReader {
    /// Open a WAV file for random access.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("failed to open WAV file: {}", path.display()))?;
        let reader = hound::WavReader::new(BufReader::with_capacity(1 << 20, file))
            .with_context(|| format!("failed to read WAV header: {}", path.display()))?;
        let spec = reader.spec();
        let kind = match spec.sample_format {
            hound::SampleFormat::Float => SampleKind::Float,
            hound::SampleFormat::Int => match spec.bits_per_sample {
                8 => SampleKind::Int8,
                16 => SampleKind::Int16,
                _ => SampleKind::Int32,
            },
        };
        Ok(Self {
            frames: reader.len() as u64,
            sample_rate: spec.sample_rate,
            channels: spec.channels as usize,
            reader,
            kind,
        })
    }

    /// Sample rate of the file.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channel count of the file.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Length of the file in frames.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Read `count` frames starting at `start`, as per-channel planes.
    ///
    /// Frames outside the file are returned as silence.
    pub fn read_frames(&mut self, start: u64, count: usize) -> Result<Vec<Vec<f32>>> {
        let mut planes = vec![Vec::with_capacity(count); self.channels];
        if count == 0 {
            return Ok(planes);
        }
        let total = self.frames;
        if start >= total {
            for p in planes.iter_mut() {
                p.resize(count, 0.0);
            }
            return Ok(planes);
        }
        let available = ((total - start) as usize).min(count);
        self.reader.seek(start as u32)?;

        // Read the interleaved samples, then split them into planes.
        let interleaved: Vec<f32> = match self.kind {
            SampleKind::Float => self
                .reader
                .samples::<f32>()
                .take(available * self.channels)
                .collect::<Result<Vec<f32>, hound::Error>>()?,
            SampleKind::Int8 => self
                .reader
                .samples::<i8>()
                .take(available * self.channels)
                .map(|v| v.map(|x| x as f32 / 128.0))
                .collect::<Result<Vec<f32>, hound::Error>>()?,
            SampleKind::Int16 => self
                .reader
                .samples::<i16>()
                .take(available * self.channels)
                .map(|v| v.map(|x| x as f32 / 32768.0))
                .collect::<Result<Vec<f32>, hound::Error>>()?,
            SampleKind::Int32 => self
                .reader
                .samples::<i32>()
                .take(available * self.channels)
                .map(|v| v.map(|x| x as f32 / 2_147_483_648.0))
                .collect::<Result<Vec<f32>, hound::Error>>()?,
        };

        for i in 0..available {
            for (c, plane) in planes.iter_mut().enumerate() {
                let idx = i * self.channels + c;
                if idx < interleaved.len() {
                    plane.push(interleaved[idx]);
                }
            }
        }
        for p in planes.iter_mut() {
            p.resize(count, 0.0);
        }
        Ok(planes)
    }

    /// Read a mono mixdown of `count` frames starting at `start`.
    pub fn read_mono(&mut self, start: u64, count: usize) -> Result<Vec<f32>> {
        let planes = self.read_frames(start, count)?;
        let mut out = vec![0.0f32; count];
        if planes.is_empty() {
            return Ok(out);
        }
        let inv = 1.0 / planes.len() as f32;
        for plane in &planes {
            for (i, &s) in plane.iter().enumerate() {
                out[i] += s * inv;
            }
        }
        Ok(out)
    }
}

/// SHA-256 of an already-written output file, used for reproducibility checks.
pub fn hash_output(path: &Path) -> Result<String> {
    sha256_file(path)
}
