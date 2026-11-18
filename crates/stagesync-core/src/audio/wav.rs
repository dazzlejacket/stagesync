//! Streaming WAV writer and random-access WAV reader.
//!
//! Output preserves the **native channel count** of the source and the **native level**:
//! no normalisation, no automatic gain, no clipping beyond the representable range.
//! The sample rate of the output is chosen explicitly by the caller (the target file's
//! nominal rate for source-aligned exports, the reference rate for timeline exports).

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use super::hash::sha256_file;
use super::rf64::{Container, Rf64Writer};

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

    /// The number of bytes one sample of one channel occupies in the file.
    pub fn bytes_per_sample(self) -> u64 {
        match self {
            WavFormat::Float32 => 4,
            WavFormat::Int16 => 2,
        }
    }
}

/// The largest payload an ordinary RIFF/WAVE file can carry.
///
/// The RIFF chunk size and the `data` chunk size are both 32-bit fields, so
/// the audio payload cannot exceed `u32::MAX`. A margin is left for the
/// header hound writes after the payload, so the limit is strictly below the
/// field's own capacity.
///
/// A file larger than this cannot be written as an ordinary RIFF file at all.
/// Rather than emitting a header whose size field has wrapped — which produces
/// a file that looks complete and is not — the writer refuses the output.
pub const RIFF_MAX_DATA_BYTES: u64 = u32::MAX as u64 - 4096;

/// Refuse an output that cannot be represented as an ordinary RIFF/WAVE file.
///
/// The check is arithmetic only: it never allocates or writes anything, so it
/// can be run before a long render begins.
pub fn ensure_wav_size_fits(frames: u64, channels: usize, format: WavFormat) -> Result<()> {
    let bytes = frames
        .saturating_mul(channels as u64)
        .saturating_mul(format.bytes_per_sample());
    if bytes > RIFF_MAX_DATA_BYTES {
        anyhow::bail!(
            "the output would need {} bytes of audio, which is more than an ordinary \
             RIFF/WAVE file can hold ({} bytes). Export a shorter span, or convert the \
             file to RF64 first.",
            bytes,
            RIFF_MAX_DATA_BYTES
        );
    }
    Ok(())
}

/// The writer a file is written through.
///
/// The two are not interchangeable: the RIFF writer hands its header to the RIFF
/// library, while the RF64 writer writes the header itself, because the whole
/// point of RF64 is a header the RIFF library cannot produce.
enum WriterInner {
    /// An ordinary RIFF/WAVE file.
    Riff(hound::WavWriter<BufWriter<File>>),
    /// An RF64 or BW64 file.
    Extended(Rf64Writer),
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
    writer: Option<WriterInner>,
    format: WavFormat,
    channels: usize,
    sample_rate: u32,
    frames_written: u64,
    /// Bytes of audio payload written so far, for the RIFF size guard.
    data_bytes: u64,
    /// The container the file is written in.
    container: Container,
    /// Where the finished file belongs.
    destination: PathBuf,
    /// The file being written to.
    temporary: PathBuf,
    /// Whether the file has been moved into place.
    committed: bool,
}

/// Create the temporary file a destination is written through.
///
/// The temporary file is created **exclusively**: the name carries the process
/// id, a per-process counter and a clock reading, and the file is opened with
/// `create_new`, so it can never be a file another writer already owns. A
/// name that somehow already exists is abandoned in favour of another, rather
/// than truncated — truncating it would destroy the other writer's samples and
/// let an unsuccessful export corrupt a successful one.
fn create_temporary(destination: &Path) -> Result<(PathBuf, File)> {
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for attempt in 0..64u32 {
        let candidate = parent.join(format!(".{name}.{pid}.{attempt}.{nanos}.stagesync-part"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to create a temporary file beside {}",
                        destination.display()
                    )
                })
            }
        }
    }
    anyhow::bail!(
        "failed to create a unique temporary file beside {}",
        destination.display()
    )
}

impl WavWriter {
    /// Create a new WAV file.
    ///
    /// The file is an ordinary RIFF/WAVE file, whose sizes fit in 32 bits. A file
    /// whose payload would not fit is refused by [`ensure_wav_size_fits`] rather than
    /// written with a size field that has wrapped.
    pub fn create(
        path: &Path,
        sample_rate: u32,
        channels: usize,
        format: WavFormat,
    ) -> Result<Self> {
        Self::create_with_container(path, sample_rate, channels, format, Container::Riff)
    }

    /// Create a new WAV file in a named container.
    ///
    /// [`Container::Rf64`] and [`Container::Bw64`] hold a payload of any size: their
    /// header carries the true sizes in 64-bit fields. They are the only way to write
    /// a recording that is longer than an ordinary WAV file can describe.
    pub fn create_with_container(
        path: &Path,
        sample_rate: u32,
        channels: usize,
        format: WavFormat,
        container: Container,
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
        let (temporary, file) = create_temporary(path)?;
        // The destination is named in the error rather than the temporary file: the
        // temporary name is a hidden, process-specific one that the user did not ask
        // for and cannot act on.
        let inner = match container {
            Container::Riff => {
                let writer = hound::WavWriter::new(BufWriter::with_capacity(1 << 20, file), spec)
                    .with_context(|| {
                    format!("failed to initialise WAV file: {}", path.display())
                })?;
                WriterInner::Riff(writer)
            }
            Container::Rf64 | Container::Bw64 => WriterInner::Extended(Rf64Writer::with_file(
                file,
                temporary.clone(),
                path.to_path_buf(),
                sample_rate,
                channels,
                format,
                container,
            )?),
        };
        Ok(Self {
            writer: Some(inner),
            format,
            channels,
            sample_rate,
            frames_written: 0,
            data_bytes: 0,
            container,
            destination: path.to_path_buf(),
            temporary,
            committed: false,
        })
    }

    /// The container the file is written in.
    pub fn container(&self) -> Container {
        self.container
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

    /// Check that `frames` more frames still fit in an ordinary RIFF file.
    ///
    /// The guard is applied on every write, not only at the start, because a
    /// render of a file whose length is not known in advance would otherwise
    /// discover the overflow when it is already too late to do anything about
    /// it. The underlying writer counts its payload in a 32-bit field with
    /// unchecked arithmetic, so a payload that overflows that field produces a
    /// file whose header does not describe its contents.
    fn reserve_data_bytes(&mut self, frames: usize) -> Result<()> {
        let bytes = (frames as u64)
            .saturating_mul(self.channels as u64)
            .saturating_mul(self.format.bytes_per_sample());
        let total = self.data_bytes.saturating_add(bytes);
        if total > RIFF_MAX_DATA_BYTES && self.container == Container::Riff {
            anyhow::bail!(
                "the output reached {} bytes of audio, which is more than an ordinary \
                 RIFF/WAVE file can hold ({} bytes). Write the file in the RF64 container \
                 instead.",
                total,
                RIFF_MAX_DATA_BYTES
            );
        }
        self.data_bytes = total;
        Ok(())
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
        self.reserve_data_bytes(n)?;
        match self.writer.as_mut() {
            Some(WriterInner::Riff(writer)) => match self.format {
                WavFormat::Float32 => {
                    for i in 0..n {
                        for p in planes {
                            writer.write_sample(p.as_ref()[i])?;
                        }
                    }
                }
                WavFormat::Int16 => {
                    for i in 0..n {
                        for p in planes {
                            let v = (p.as_ref()[i] * 32767.0).round();
                            let v = v.clamp(-32768.0, 32767.0);
                            writer.write_sample(v as i16)?;
                        }
                    }
                }
            },
            Some(WriterInner::Extended(writer)) => writer.write_planar(planes)?,
            None => anyhow::bail!("the WAV file has already been finalised"),
        }
        self.frames_written += n as u64;
        Ok(())
    }

    /// Write `frames` frames of digital silence.
    pub fn write_silence(&mut self, frames: usize) -> Result<()> {
        self.reserve_data_bytes(frames)?;
        match self.writer.as_mut() {
            Some(WriterInner::Riff(writer)) => match self.format {
                WavFormat::Float32 => {
                    for _ in 0..frames {
                        for _ in 0..self.channels {
                            writer.write_sample(0.0f32)?;
                        }
                    }
                }
                WavFormat::Int16 => {
                    for _ in 0..frames {
                        for _ in 0..self.channels {
                            writer.write_sample(0i16)?;
                        }
                    }
                }
            },
            Some(WriterInner::Extended(writer)) => writer.write_silence(frames)?,
            None => anyhow::bail!("the WAV file has already been finalised"),
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
        match writer {
            WriterInner::Riff(writer) => {
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
            }
            WriterInner::Extended(writer) => {
                // The extended writer owns the temporary file and moves it into place
                // itself, because it has to patch the header before the file is
                // complete.
                writer.finalize().with_context(|| {
                    format!(
                        "failed to finalise WAV file: {}",
                        self.destination.display()
                    )
                })?;
            }
        }
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
    Int24,
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
                24 => SampleKind::Int24,
                _ => SampleKind::Int32,
            },
        };
        // hound reports the length in *samples*, and a sample is one channel of
        // one frame. A stereo file therefore reports twice its frame count, and
        // every caller that wanted frames was silently working with a number
        // twice too large.
        let frames = (reader.len() as u64) / (spec.channels as u64).max(1);
        Ok(Self {
            frames,
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
            // A 24-bit sample has a full scale of 2^23, not 2^31. Dividing it by
            // the 32-bit full scale put every sample 48 dB too low, which made
            // the level and energy checks of the verifier read a 24-bit file as
            // if it were almost silent.
            SampleKind::Int24 => self
                .reader
                .samples::<i32>()
                .take(available * self.channels)
                .map(|v| v.map(|x| x as f32 / 8_388_608.0))
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
