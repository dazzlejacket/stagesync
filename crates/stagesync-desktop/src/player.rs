//! Playback: the reference, the corrected target, and the switch between them.
//!
//! The preview is not a stand-in for the export — it is the export's own
//! arithmetic, run in real time. The target is read with the core's decoder, and
//! the drift correction is applied by the core's own polyphase resampler, so what
//! is heard in the preview is what is written to the file. A preview that
//! approximated the correction would be worse than useless: it would show a
//! synchronisation that the export does not have.
//!
//! The reference is **never** altered. It is read, resampled to the output
//! device's rate so that the two can be compared sample for sample, and played.

use std::collections::VecDeque;
use std::num::NonZero;
use std::path::Path;
use std::time::Instant;

use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer, Source};
use stagesync_core::audio::AudioDecoder;
use stagesync_core::matching::LinearModel;
use stagesync_core::resample::StreamingResampler;

/// The resampling quality, the same filter the renderer uses.
const TAPS: usize = 63;
const PHASES: usize = 1024;

/// How many frames are decoded at a time while playing.
const BLOCK: usize = 8192;

/// Which recording is being heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// The reference recording, exactly as it was recorded.
    Reference,
    /// A target recording, with the correction applied.
    Target,
}

impl Voice {
    /// The text shown for this voice.
    pub fn label(self) -> &'static str {
        match self {
            Voice::Reference => "Reference",
            Voice::Target => "Corrected target",
        }
    }
}

/// What is playing, and where it started.
struct Playing {
    voice: Voice,
    /// The position on the reference timeline the playback started at.
    start_secs: f64,
    /// When it started.
    started: Instant,
}

/// The player.
///
/// A machine without an audio device is not an error: the application still works,
/// it simply cannot make a sound, and it says so.
pub struct Player {
    /// The device, kept alive for as long as the player is.
    ///
    /// The sink owns the audio stream; dropping it closes the device. It is never
    /// read, but it must not be dropped.
    #[allow(dead_code)]
    sink: Option<MixerDeviceSink>,
    player: Option<RodioPlayer>,
    playing: Option<Playing>,
    /// Why there is no output, when there is none.
    error: Option<String>,
    /// The rate the output device runs at.
    rate: u32,
    volume: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    /// Open the audio output, if the machine has one.
    pub fn new() -> Self {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => {
                let rate = sink.config().sample_rate().get();
                let player = RodioPlayer::connect_new(sink.mixer());
                Self {
                    sink: Some(sink),
                    player: Some(player),
                    playing: None,
                    error: None,
                    rate,
                    volume: 1.0,
                }
            }
            Err(error) => Self {
                sink: None,
                player: None,
                playing: None,
                error: Some(format!("no audio output device: {error}")),
                rate: 48_000,
                volume: 1.0,
            },
        }
    }

    /// Whether the machine can make a sound.
    pub fn is_available(&self) -> bool {
        self.player.is_some()
    }

    /// Why there is no sound, when there is none.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The rate the output device runs at.
    pub fn output_rate(&self) -> u32 {
        self.rate
    }

    /// The volume, between zero and one.
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Set the volume, between zero and one.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(player) = &self.player {
            player.set_volume(self.volume);
        }
    }

    /// What is playing, if anything.
    pub fn voice(&self) -> Option<Voice> {
        self.playing.as_ref().map(|playing| playing.voice)
    }

    /// Whether something is playing.
    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// The position on the reference timeline, in seconds.
    pub fn position(&self) -> f64 {
        self.playing.as_ref().map_or(0.0, |playing| {
            playing.start_secs + playing.started.elapsed().as_secs_f64()
        })
    }

    /// Stop playing.
    pub fn stop(&mut self) {
        if let Some(player) = &self.player {
            player.stop();
            player.clear();
        }
        self.playing = None;
    }

    /// Play the reference recording from a position on the reference timeline.
    pub fn play_reference(&mut self, path: &Path, from_secs: f64) -> Result<(), anyhow::Error> {
        let rate = self.rate;
        let source = ResampledSource::open(path, from_secs.max(0.0), rate, None)?;
        self.start(Voice::Reference, from_secs.max(0.0), source)
    }

    /// Play a target recording with the correction applied, from a position on the
    /// reference timeline.
    ///
    /// The model is the one the export uses. The source is read from the position
    /// that corresponds to `from_secs` on the reference timeline, and the drift is
    /// applied by the same resampler the renderer uses.
    pub fn play_target(
        &mut self,
        path: &Path,
        model: &LinearModel,
        from_secs: f64,
    ) -> Result<(), anyhow::Error> {
        let rate = self.rate;
        let from_secs = from_secs.max(0.0);
        let source = ResampledSource::open(path, from_secs, rate, Some(model))?;
        self.start(Voice::Target, from_secs, source)
    }

    /// Put a source on the player and start the clock.
    fn start(
        &mut self,
        voice: Voice,
        from_secs: f64,
        source: ResampledSource,
    ) -> Result<(), anyhow::Error> {
        let Some(player) = &self.player else {
            return Err(anyhow::anyhow!(
                "there is no audio output device: {}",
                self.error.as_deref().unwrap_or("unknown reason")
            ));
        };
        player.stop();
        player.clear();
        player.append(source);
        player.play();
        self.playing = Some(Playing {
            voice,
            start_secs: from_secs,
            started: Instant::now(),
        });
        Ok(())
    }
}

/// A recording, read from a position and resampled to the output rate.
///
/// The resampling is the core's own deterministic polyphase filter, so the preview
/// and the export are the same filter applied to the same material.
struct ResampledSource {
    decoder: AudioDecoder,
    /// One resampler per channel.
    resamplers: Vec<StreamingResampler>,
    /// The interleaved output, waiting to be played.
    ready: VecDeque<f32>,
    /// How many channels the source has.
    channels: u16,
    /// The rate the output runs at.
    rate: u32,
    /// Whether the recording has been read to its end.
    finished: bool,
}

impl ResampledSource {
    /// Open a recording at a position.
    ///
    /// `model` is the correction to apply. `None` means the recording is played as
    /// it is — that is the reference, which is never altered.
    ///
    /// The position is a position on the **reference** timeline. For a corrected
    /// target it is turned into a position in the recording by the model.
    fn open(
        path: &Path,
        from_secs: f64,
        out_rate: u32,
        model: Option<&LinearModel>,
    ) -> Result<Self, anyhow::Error> {
        let mut decoder = AudioDecoder::open(path)?;
        let in_rate = decoder.meta().sample_rate;
        let channels = decoder.meta().channels.max(1);

        // Where in the recording to start reading, and how much the rate changes.
        //
        // The output advances along the reference timeline at one second per second,
        // whatever the recording's own rate is. For the reference that is simply
        // its own rate; for a corrected target the drift stretches the material, so
        // the recording is read more slowly than it is written.
        let (start_frame, alpha) = match model {
            None => (
                (from_secs * in_rate as f64).round() as u64,
                in_rate as f64 / out_rate as f64,
            ),
            Some(model) => {
                // The source time that corresponds to the requested reference time.
                let source_time = (from_secs - model.offset) / model.scale;
                let source_time = source_time.max(0.0);
                (
                    (source_time * in_rate as f64).round() as u64,
                    in_rate as f64 / (out_rate as f64 * model.scale),
                )
            }
        };

        decoder.seek_to_frame(start_frame)?;

        let resamplers = (0..channels)
            .map(|_| StreamingResampler::new(alpha, 0.0, TAPS, PHASES))
            .collect();

        Ok(Self {
            decoder,
            resamplers,
            ready: VecDeque::new(),
            channels: channels as u16,
            rate: out_rate,
            finished: false,
        })
    }

    /// Decode and resample until there is something to play.
    ///
    /// Returns `false` when the recording has nothing more to give.
    fn fill(&mut self) -> bool {
        if self.finished {
            return false;
        }
        let block = match self.decoder.read_block(BLOCK) {
            Ok(Some(block)) => block,
            Ok(None) => {
                self.finished = true;
                return self.flush();
            }
            Err(error) => {
                log::warn!("playback stopped: {error:#}");
                self.finished = true;
                return self.flush();
            }
        };

        let mut produced: Vec<Vec<f32>> = Vec::with_capacity(self.resamplers.len());
        for (channel, resampler) in self.resamplers.iter_mut().enumerate() {
            let plane = block
                .planes
                .get(channel)
                .map(|p| p.as_slice())
                .unwrap_or(&[]);
            produced.push(resampler.push(plane));
        }
        self.interleave(&produced);
        !self.ready.is_empty()
    }

    /// Finish the stream, producing the tail of the filter.
    fn flush(&mut self) -> bool {
        let mut produced: Vec<Vec<f32>> = Vec::with_capacity(self.resamplers.len());
        for resampler in self.resamplers.iter_mut() {
            produced.push(resampler.flush());
        }
        self.interleave(&produced);
        !self.ready.is_empty()
    }

    /// Interleave the per-channel output into the stream the device expects.
    fn interleave(&mut self, produced: &[Vec<f32>]) {
        let frames = produced.iter().map(|plane| plane.len()).min().unwrap_or(0);
        for frame in 0..frames {
            for plane in produced {
                if let Some(sample) = plane.get(frame) {
                    self.ready.push_back(*sample);
                }
            }
        }
    }
}

impl Iterator for ResampledSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        loop {
            if let Some(sample) = self.ready.pop_front() {
                return Some(sample);
            }
            if !self.fill() {
                return None;
            }
        }
    }
}

impl Source for ResampledSource {
    fn current_span_len(&self) -> Option<usize> {
        // The number of channels and the rate never change.
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(self.channels).unwrap_or(NonZero::new(1).expect("one is not zero"))
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(self.rate).unwrap_or(NonZero::new(48_000).expect("48000 is not zero"))
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        // The length of what is left depends on where the recording ends, which is
        // not known until it has been read.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagesync_core::audio::{WavFormat, WavWriter};

    /// A tone at a known rate, written to a temporary file.
    fn tone(path: &Path, rate: u32, secs: f64, freq: f64) {
        let n = (secs * rate as f64).round() as usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (2.0 * std::f64::consts::PI * freq * t as f64).sin() as f32 * 0.5
            })
            .collect();
        let mut writer = WavWriter::create(path, rate, 1, WavFormat::Float32).expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");
    }

    #[test]
    fn a_corrected_source_produces_the_right_number_of_samples() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("tone.wav");
        let rate = 8000u32;
        tone(&path, rate, 2.0, 220.0);

        // A drift of +1000 ppm: the source's clock ran slow, so the material is
        // stretched by 1000 ppm when it is corrected.
        let model = LinearModel::new(1.001, 0.0);
        let mut source = ResampledSource::open(&path, 0.0, rate, Some(&model)).expect("open");
        let produced = source.by_ref().count();
        let expected = (2.0 * rate as f64 * model.scale).round() as usize;
        // The filter's tail adds a few samples.
        assert!(
            (produced as i64 - expected as i64).abs() < 200,
            "the corrected source produced {produced} samples, and the correction needs {expected}"
        );
    }

    #[test]
    fn the_reference_is_played_as_it_is() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("tone.wav");
        let rate = 8000u32;
        tone(&path, rate, 1.0, 220.0);

        let mut source = ResampledSource::open(&path, 0.0, rate, None).expect("open");
        let produced = source.by_ref().count();
        let expected = rate as usize;
        assert!(
            (produced as i64 - expected as i64).abs() < 200,
            "the reference produced {produced} samples, and it holds {expected}"
        );
    }

    #[test]
    fn a_position_in_the_middle_of_the_recording_is_where_the_playback_starts() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("tone.wav");
        let rate = 8000u32;
        // A ramp, so the position of the first sample says where it came from.
        let n = rate as usize;
        let samples: Vec<f32> = (0..n).map(|i| i as f32 / n as f32).collect();
        let mut writer = WavWriter::create(&path, rate, 1, WavFormat::Float32).expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");

        let mut source = ResampledSource::open(&path, 0.5, rate, None).expect("open");
        let first = source.next().expect("a sample");
        assert!(
            (first - 0.5).abs() < 0.01,
            "the first sample was {first}, and half way through the ramp it should be about 0.5"
        );
    }
}
