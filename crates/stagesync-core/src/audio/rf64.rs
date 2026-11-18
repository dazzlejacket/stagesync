//! The WAV containers that can hold more than four gibibytes.
//!
//! An ordinary RIFF/WAVE file carries its two important sizes — the size of the RIFF
//! chunk and the size of the `data` chunk — in **32-bit** fields. A recording long
//! enough to fill them needs more than that, and a header whose size field has
//! wrapped produces a file that looks complete and is not. Such a file is worse than
//! no file at all, because a player will believe it.
//!
//! `RF64` (and `BW64`, which is the same container with a broadcast-extension chunk)
//! replace those two fields with the sentinel `0xFFFFFFFF` and put the true sizes in
//! a `ds64` chunk at the front of the file, where they are 64 bits wide.
//!
//! * **RF64** is the EBU specification. The file begins with the four bytes `RF64`.
//! * **BW64** is the EBU *broadcast* variant of the same container. It uses the same
//!   `RF64` signature and the same `ds64` chunk, and adds a `bext` (broadcast wave
//!   extension) chunk that carries the origin of the material.
//!
//! Both are written here by hand rather than by the RIFF writer, because the point of
//! them is the header: a library that only knows how to write 32-bit sizes cannot
//! write them at all.
//!
//! The writer is deliberately conservative:
//!
//! * the payload is written to a temporary file and moved into place at the end, so a
//!   failed export never leaves a half-written file where a finished one is expected;
//! * the `ds64` sizes are patched into the header only once the payload is complete,
//!   so the file on disk is never inconsistent with its own header;
//! * the sizes are computed from the bytes actually written, not from an estimate.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

use super::wav::WavFormat;

/// The container an audio file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// An ordinary RIFF/WAVE file, whose sizes fit in 32 bits.
    Riff,
    /// An RF64 file, whose sizes are 64 bits wide.
    Rf64,
    /// A BW64 file: RF64 with a broadcast-extension chunk.
    Bw64,
}

impl Container {
    /// Short name used in reports.
    pub fn name(self) -> &'static str {
        match self {
            Container::Riff => "riff",
            Container::Rf64 => "rf64",
            Container::Bw64 => "bw64",
        }
    }
}

/// The sentinel that marks a size as "see the `ds64` chunk".
const UNKNOWN_SIZE: u32 = u32::MAX;

/// The size of the `ds64` payload: three 64-bit sizes and a table length.
const DS64_PAYLOAD_BYTES: u64 = 28;

/// The offset of the `ds64` payload from the start of the file.
///
/// `RF64` (4) + size (4) + `WAVE` (4) + `ds64` (4) + chunk size (4) = 20.
const DS64_PAYLOAD_OFFSET: u64 = 20;

/// The size of the `bext` payload without the coding history: the fixed fields of the
/// broadcast wave extension.
///
/// description (256) + originator (32) + originator reference (32) + origination date
/// (10) + origination time (8) + time reference (8) + version (2) + UMID (64) +
/// loudness fields (10) + reserved (180).
const BEXT_FIXED_BYTES: usize = 602;

/// The number of bytes one sample of one channel occupies.
fn bytes_per_sample(format: WavFormat) -> usize {
    match format {
        WavFormat::Float32 => 4,
        WavFormat::Int16 => 2,
    }
}
/// Create the temporary file a destination is written through.
///
/// The file is created **exclusively**: the name carries the process id, a
/// per-process counter and a clock reading, and the file is opened with
/// `create_new`, so it can never be a file another writer already owns.
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
    bail!(
        "failed to create a unique temporary file beside {}",
        destination.display()
    )
}

/// A streaming writer for RF64 and BW64 files.
///
/// The writer accepts planar blocks exactly as the RIFF writer does, so the caller
/// does not have to know which container it is producing.
pub struct Rf64Writer {
    /// The file being written.
    writer: BufWriter<File>,
    /// Where the finished file belongs.
    destination: PathBuf,
    /// The file being written.
    temporary: PathBuf,
    /// The number of channels of the output.
    channels: usize,
    /// The output sample format.
    format: WavFormat,
    /// The output sample rate.
    sample_rate: u32,
    /// The number of payload bytes written so far.
    data_bytes: u64,
    /// The number of frames written so far.
    frames: u64,
    /// Whether the file has been moved into place.
    committed: bool,
}

impl Rf64Writer {
    /// Create an RF64 file.
    pub fn create(
        path: &Path,
        sample_rate: u32,
        channels: usize,
        format: WavFormat,
        variant: Container,
    ) -> Result<Self> {
        let (temporary, file) = create_temporary(path)?;
        Self::with_file(
            file,
            temporary,
            path.to_path_buf(),
            sample_rate,
            channels,
            format,
            variant,
        )
    }

    /// Write into a file that has already been created.
    ///
    /// The caller owns the temporary file's name, so that one writer can serve both
    /// containers without two different notions of "the file being written".
    pub fn with_file(
        file: File,
        temporary: PathBuf,
        destination: PathBuf,
        sample_rate: u32,
        channels: usize,
        format: WavFormat,
        variant: Container,
    ) -> Result<Self> {
        anyhow::ensure!(channels > 0, "cannot write a WAV with zero channels");
        anyhow::ensure!(
            sample_rate > 0,
            "cannot write a WAV with a zero sample rate"
        );
        anyhow::ensure!(
            matches!(variant, Container::Rf64 | Container::Bw64),
            "the RF64 writer only writes RF64 and BW64 files"
        );

        let writer = BufWriter::with_capacity(1 << 20, file);
        let mut this = Self {
            writer,
            destination,
            temporary,
            channels,
            format,
            sample_rate,
            data_bytes: 0,
            frames: 0,
            committed: false,
        };
        this.write_header(variant)?;
        Ok(this)
    }

    /// The number of channels of the output file.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The number of frames written so far.
    pub fn frames_written(&self) -> u64 {
        self.frames
    }

    /// Write the header: the `RF64` signature, the `ds64` chunk, the `bext` chunk of
    /// a BW64 file, the `fmt ` chunk and the `data` chunk header.
    fn write_header(&mut self, variant: Container) -> Result<()> {
        let mut head: Vec<u8> = Vec::with_capacity(256);

        // The RF64 signature, the sentinel size and the form type.
        head.extend_from_slice(b"RF64");
        head.extend_from_slice(&UNKNOWN_SIZE.to_le_bytes());
        head.extend_from_slice(b"WAVE");

        // The `ds64` chunk, whose payload is patched once the file is complete.
        head.extend_from_slice(b"ds64");
        head.extend_from_slice(&(DS64_PAYLOAD_BYTES as u32).to_le_bytes());
        head.extend_from_slice(&0u64.to_le_bytes()); // riff size
        head.extend_from_slice(&0u64.to_le_bytes()); // data size
        head.extend_from_slice(&0u64.to_le_bytes()); // sample count
        head.extend_from_slice(&0u32.to_le_bytes()); // table length

        if variant == Container::Bw64 {
            write_bext(&mut head);
        }

        // The `fmt ` chunk.
        let bits = match self.format {
            WavFormat::Float32 => 32u16,
            WavFormat::Int16 => 16u16,
        };
        let tag: u16 = match self.format {
            WavFormat::Float32 => 3, // WAVE_FORMAT_IEEE_FLOAT
            WavFormat::Int16 => 1,   // WAVE_FORMAT_PCM
        };
        let block_align = self.channels as u16 * bits / 8;
        let byte_rate = self.sample_rate * block_align as u32;
        let mut fmt = Vec::with_capacity(18);
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&(self.channels as u16).to_le_bytes());
        fmt.extend_from_slice(&self.sample_rate.to_le_bytes());
        fmt.extend_from_slice(&byte_rate.to_le_bytes());
        fmt.extend_from_slice(&block_align.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        if self.format == WavFormat::Float32 {
            // A non-PCM format carries an extension size, which is zero here.
            fmt.extend_from_slice(&0u16.to_le_bytes());
        }
        head.extend_from_slice(b"fmt ");
        head.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        head.extend_from_slice(&fmt);

        // The `data` chunk header. Its size is the sentinel; the true size is in the
        // `ds64` chunk.
        head.extend_from_slice(b"data");
        head.extend_from_slice(&UNKNOWN_SIZE.to_le_bytes());

        self.writer.write_all(&head).with_context(|| {
            format!(
                "failed to write the WAV header: {}",
                self.destination.display()
            )
        })
    }

    /// Write one planar block (one plane per channel).
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
        let mut buffer: Vec<u8> =
            Vec::with_capacity(n * self.channels * bytes_per_sample(self.format));
        match self.format {
            WavFormat::Float32 => {
                for i in 0..n {
                    for p in planes {
                        buffer.extend_from_slice(&p.as_ref()[i].to_le_bytes());
                    }
                }
            }
            WavFormat::Int16 => {
                for i in 0..n {
                    for p in planes {
                        let v = (p.as_ref()[i] * 32767.0).round().clamp(-32768.0, 32767.0);
                        buffer.extend_from_slice(&(v as i16).to_le_bytes());
                    }
                }
            }
        }
        self.writer
            .write_all(&buffer)
            .with_context(|| format!("failed to write audio: {}", self.destination.display()))?;
        self.data_bytes += buffer.len() as u64;
        self.frames += n as u64;
        Ok(())
    }

    /// Write `frames` frames of digital silence.
    pub fn write_silence(&mut self, frames: usize) -> Result<()> {
        let sample: Vec<u8> = match self.format {
            WavFormat::Float32 => 0f32.to_le_bytes().to_vec(),
            WavFormat::Int16 => 0i16.to_le_bytes().to_vec(),
        };
        let mut buffer = Vec::with_capacity(frames * self.channels * sample.len());
        for _ in 0..frames {
            for _ in 0..self.channels {
                buffer.extend_from_slice(&sample);
            }
        }
        self.writer
            .write_all(&buffer)
            .with_context(|| format!("failed to write silence: {}", self.destination.display()))?;
        self.data_bytes += buffer.len() as u64;
        self.frames += frames as u64;
        Ok(())
    }

    /// Finalise the header, flush it to disk, and move the file into place.
    pub fn finalize(mut self) -> Result<()> {
        self.writer.flush().with_context(|| {
            format!(
                "failed to flush the WAV file: {}",
                self.destination.display()
            )
        })?;
        let file_size = self.writer.get_ref().metadata()?.len();

        // Patch the `ds64` sizes. The file is opened again for writing, and the three
        // fields are replaced with the values the finished file actually has.
        let mut file = OpenOptions::new()
            .write(true)
            .open(self.temporary.as_path())
            .with_context(|| {
                format!(
                    "failed to open the WAV file for its header: {}",
                    self.temporary.display()
                )
            })?;
        let riff_size = file_size.saturating_sub(8);
        file.seek(SeekFrom::Start(DS64_PAYLOAD_OFFSET))?;
        file.write_all(&riff_size.to_le_bytes())?;
        file.write_all(&self.data_bytes.to_le_bytes())?;
        file.write_all(&self.frames.to_le_bytes())?;
        file.flush()?;
        drop(file);

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

impl Drop for Rf64Writer {
    /// Remove the temporary file when the writer was not finalised.
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.temporary);
        }
    }
}

/// Write a broadcast wave extension chunk.
///
/// The chunk records where the material came from. The `time reference` field is left
/// at zero: it counts the samples since midnight, which is a property of a broadcast
/// schedule rather than of a file, and inventing a value would be a false statement.
fn write_bext(head: &mut Vec<u8>) {
    let mut payload = vec![0u8; BEXT_FIXED_BYTES];
    let description = b"StageSync mixdown";
    payload[..description.len()].copy_from_slice(description);
    let originator = b"StageSync";
    payload[256..256 + originator.len()].copy_from_slice(originator);
    // Version 2 of the extension.
    payload[256 + 32 + 32 + 10 + 8 + 8..256 + 32 + 32 + 10 + 8 + 8 + 2]
        .copy_from_slice(&2u16.to_le_bytes());
    let history = b"StageSync mixdown, written as BW64\0";
    payload.extend_from_slice(history);
    // A chunk's payload is padded to an even length, so the padding byte is part of
    // the chunk. A file whose chunks are not padded is a file whose reader walks off
    // the end of the last chunk and into the wrong place.
    if payload.len() % 2 == 1 {
        payload.push(0);
    }

    head.extend_from_slice(b"bext");
    head.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    head.extend_from_slice(&payload);
}

/// The sample encoding of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleEncoding {
    /// 32-bit IEEE float.
    Float32,
    /// 16-bit signed PCM.
    Int16,
    /// 32-bit signed PCM.
    Int32,
}

/// A random-access reader for RIFF, RF64 and BW64 files.
///
/// The reader is written here rather than taken from the RIFF library because that
/// library does not know what an `ds64` chunk is, and a file written by this crate in
/// the RF64 container has to be readable by this crate.
pub struct ContainerReader {
    reader: BufReader<File>,
    /// The output sample rate.
    sample_rate: u32,
    /// The number of channels.
    channels: usize,
    /// The length of the file in frames.
    frames: u64,
    /// The sample encoding.
    encoding: SampleEncoding,
    /// The offset of the first sample of the `data` chunk.
    data_offset: u64,
    /// The number of bytes one frame occupies.
    frame_bytes: u64,
}

impl ContainerReader {
    /// Open a WAV file of any of the three containers.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("failed to open WAV file: {}", path.display()))?;
        let mut reader = BufReader::with_capacity(1 << 20, file);
        let file_len = reader.get_ref().metadata()?.len();

        let mut magic = [0u8; 12];
        reader
            .read_exact(&mut magic)
            .with_context(|| format!("failed to read the WAV header: {}", path.display()))?;
        let is_rf64 = &magic[0..4] == b"RF64";
        if !is_rf64 && &magic[0..4] != b"RIFF" {
            bail!("{} is not a WAV file", path.display());
        }
        if &magic[8..12] != b"WAVE" {
            bail!("{} is not a WAVE file", path.display());
        }

        // The `ds64` chunk, when there is one, carries the true sizes.
        let mut data_size: Option<u64> = None;
        let mut sample_count: Option<u64> = None;
        if is_rf64 {
            let mut id = [0u8; 4];
            reader.read_exact(&mut id)?;
            if &id != b"ds64" {
                bail!("{} is an RF64 file with no ds64 chunk", path.display());
            }
            let mut size = [0u8; 4];
            reader.read_exact(&mut size)?;
            let size = u32::from_le_bytes(size) as u64;
            let mut payload = vec![0u8; size as usize];
            reader.read_exact(&mut payload)?;
            if payload.len() < DS64_PAYLOAD_BYTES as usize {
                bail!("{} has a ds64 chunk that is too short", path.display());
            }
            data_size = Some(u64::from_le_bytes(payload[8..16].try_into().unwrap()));
            sample_count = Some(u64::from_le_bytes(payload[16..24].try_into().unwrap()));
        }

        // The chunks: `fmt ` describes the material, `data` holds it.
        let mut sample_rate = 0u32;
        let mut channels = 0usize;
        let mut encoding = SampleEncoding::Float32;

        loop {
            let mut id = [0u8; 4];
            if reader.read_exact(&mut id).is_err() {
                break;
            }
            let mut size_bytes = [0u8; 4];
            if reader.read_exact(&mut size_bytes).is_err() {
                break;
            }
            let mut size = u32::from_le_bytes(size_bytes) as u64;
            if size == UNKNOWN_SIZE as u64 {
                size = u64::MAX;
            }
            let position = reader.stream_position()?;

            match &id {
                b"fmt " => {
                    let mut fmt = vec![0u8; size.min(40) as usize];
                    reader.read_exact(&mut fmt)?;
                    if fmt.len() >= 16 {
                        let mut tag = u16::from_le_bytes(fmt[0..2].try_into().unwrap());
                        channels = u16::from_le_bytes(fmt[2..4].try_into().unwrap()) as usize;
                        sample_rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
                        let bits = u16::from_le_bytes(fmt[14..16].try_into().unwrap());
                        // `WAVE_FORMAT_EXTENSIBLE` carries the real format tag in the
                        // first two bytes of its sub-format GUID. The RIFF writer uses
                        // it for every file of more than sixteen bits, which includes
                        // every 32-bit float file.
                        if tag == 0xFFFE && fmt.len() >= 40 {
                            tag = u16::from_le_bytes(fmt[24..26].try_into().unwrap());
                        }
                        encoding = match (tag, bits) {
                            (3, 32) => SampleEncoding::Float32,
                            (1, 16) => SampleEncoding::Int16,
                            (1, 32) => SampleEncoding::Int32,
                            _ => bail!("{} uses an unsupported sample format", path.display()),
                        };
                    }
                }
                b"data" => {
                    // The payload size is the chunk size, unless the file is shorter
                    // than the chunk claims — a truncated file must not be read past
                    // its end.
                    let available = file_len.saturating_sub(position);
                    let payload = match data_size {
                        Some(size) => size.min(available),
                        None => {
                            if size == u64::MAX {
                                available
                            } else {
                                size.min(available)
                            }
                        }
                    };
                    let frame_bytes = match encoding {
                        SampleEncoding::Float32 => 4,
                        SampleEncoding::Int16 => 2,
                        SampleEncoding::Int32 => 4,
                    } * channels.max(1) as u64;
                    let frames = match frame_bytes {
                        0 => 0,
                        bytes => payload / bytes,
                    };
                    // Skip the payload.
                    reader.seek(SeekFrom::Start(position + payload))?;
                    let frames = match sample_count {
                        Some(count) => count.min(frames),
                        None => frames,
                    };
                    return Ok(Self {
                        reader,
                        sample_rate,
                        channels,
                        frames,
                        encoding,
                        data_offset: position,
                        frame_bytes,
                    });
                }
                _ => {}
            }

            // Move to the next chunk: the payload, padded to an even length.
            let padded = if size == u64::MAX {
                break;
            } else {
                size + (size & 1)
            };
            reader.seek(SeekFrom::Start(position + padded))?;
        }

        bail!("{} has no data chunk", path.display())
    }

    /// The sample rate of the file.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The number of channels of the file.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The length of the file in frames.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The sample encoding of the file.
    pub fn encoding(&self) -> SampleEncoding {
        self.encoding
    }

    /// Read `count` frames starting at `start`, as per-channel planes.
    ///
    /// Frames outside the file are returned as silence.
    pub fn read_frames(&mut self, start: u64, count: usize) -> Result<Vec<Vec<f32>>> {
        let mut planes = vec![Vec::with_capacity(count); self.channels.max(1)];
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
        let offset = self.data_offset + start * self.frame_bytes;
        self.reader.seek(SeekFrom::Start(offset))?;

        let mut raw = vec![0u8; available * self.channels.max(1) * 4];
        let wanted = match self.encoding {
            SampleEncoding::Float32 => available * self.channels.max(1) * 4,
            SampleEncoding::Int16 => available * self.channels.max(1) * 2,
            SampleEncoding::Int32 => available * self.channels.max(1) * 4,
        };
        self.reader.read_exact(&mut raw[..wanted])?;

        let interleaved: Vec<f32> = match self.encoding {
            SampleEncoding::Float32 => raw[..wanted]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect(),
            SampleEncoding::Int16 => raw[..wanted]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| i16::from_le_bytes(*c) as f32 / 32768.0)
                .collect(),
            SampleEncoding::Int32 => raw[..wanted]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| i32::from_le_bytes(*c) as f32 / 2_147_483_648.0)
                .collect(),
        };

        for i in 0..available {
            for (c, plane) in planes.iter_mut().enumerate() {
                let index = i * self.channels.max(1) + c;
                if index < interleaved.len() {
                    plane.push(interleaved[index]);
                }
            }
        }
        for p in planes.iter_mut() {
            p.resize(count, 0.0);
        }
        Ok(planes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::wav::WavWriter;

    #[test]
    fn an_rf64_file_round_trips_its_samples() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("big.wav");
        let left: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0) - 0.5).collect();
        let right: Vec<f32> = left.iter().map(|v| -v).collect();

        let mut writer = Rf64Writer::create(&path, 48_000, 2, WavFormat::Float32, Container::Rf64)
            .expect("create");
        writer.write_planar(&[&left, &right]).expect("write");
        writer.finalize().expect("finalise");

        let mut reader = ContainerReader::open(&path).expect("open");
        assert_eq!(reader.channels(), 2);
        assert_eq!(reader.sample_rate(), 48_000);
        assert_eq!(reader.frames(), 1000);
        let planes = reader.read_frames(0, 1000).expect("read");
        assert_eq!(planes.len(), 2);
        assert!((planes[0][500] - left[500]).abs() < 1e-6);
        assert!((planes[1][500] - right[500]).abs() < 1e-6);
    }

    #[test]
    fn the_header_of_an_rf64_file_describes_the_file() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("big.wav");
        let samples: Vec<f32> = vec![0.25; 100];
        let mut writer = Rf64Writer::create(&path, 44_100, 1, WavFormat::Float32, Container::Rf64)
            .expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");

        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(&bytes[0..4], b"RF64");
        // The 32-bit size fields are the sentinel, never a wrapped value.
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            u32::MAX
        );
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"ds64");
        // The `ds64` payload holds the true sizes.
        let riff_size = u64::from_le_bytes(bytes[20..28].try_into().unwrap());
        let data_size = u64::from_le_bytes(bytes[28..36].try_into().unwrap());
        let sample_count = u64::from_le_bytes(bytes[36..44].try_into().unwrap());
        assert_eq!(data_size, 400);
        assert_eq!(sample_count, 100);
        assert_eq!(riff_size, bytes.len() as u64 - 8);
    }

    #[test]
    fn a_bw64_file_carries_the_broadcast_extension() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("broadcast.wav");
        let samples: Vec<f32> = vec![0.5; 10];
        let mut writer = Rf64Writer::create(&path, 48_000, 1, WavFormat::Float32, Container::Bw64)
            .expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");

        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(&bytes[0..4], b"RF64");
        // The `bext` chunk sits between the `ds64` chunk and the `fmt ` chunk.
        assert_eq!(&bytes[48..52], b"bext");
        let reader = ContainerReader::open(&path).expect("open");
        assert_eq!(reader.frames(), 10);
    }

    #[test]
    fn an_ordinary_riff_file_is_read_by_the_same_reader() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("plain.wav");
        let samples: Vec<f32> = vec![0.125; 64];
        let mut writer = WavWriter::create(&path, 48_000, 1, WavFormat::Float32).expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");

        let mut reader = ContainerReader::open(&path).expect("open");
        assert_eq!(reader.frames(), 64);
        let planes = reader.read_frames(0, 64).expect("read");
        assert!((planes[0][10] - 0.125).abs() < 1e-6);
    }
}
