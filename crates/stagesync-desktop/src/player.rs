//! Playback: the reference, the corrected target, the switch between them, and the
//! mix.
//!
//! The preview is not a stand-in for the export — it is the export's own
//! arithmetic, run in real time. The target is read with the core's decoder, and
//! the drift correction is applied by the core's own polyphase resampler, so what
//! is heard in the preview is what is written to the file. A preview that
//! approximated the correction would be worse than useless: it would show a
//! synchronisation that the export does not have.
//!
//! The **mix** is the same: several recordings are read at once, each through its
//! own position map, and mixed with the core's own pan matrices and gains. The
//! preview is streaming, so a three-hour concert costs a few tens of megabytes
//! rather than gigabytes.
//!
//! The reference is **never** altered. It is read, resampled to the output
//! device's rate so that the two can be compared sample for sample, and played.

use std::collections::VecDeque;
use std::num::NonZero;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rodio::{DeviceSinkBuilder, Player as RodioPlayer, Source};
use stagesync_core::audio::AudioDecoder;
use stagesync_core::matching::LinearModel;
use stagesync_core::mixer::{ClippingProtection, MixTrack, PanMatrix};
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
    /// The mix of several recordings.
    Mix,
}

impl Voice {
    /// The text shown for this voice.
    pub fn label(self) -> &'static str {
        match self {
            Voice::Reference => "Reference",
            Voice::Target => "Corrected target",
            Voice::Mix => "Mix",
        }
    }
}

/// One recording of the mix, with the way it is placed in the field.
pub struct MixMember {
    /// The recording to read.
    pub path: std::path::PathBuf,
    /// The correction to apply.
    pub model: LinearModel,
    /// The fader, as a linear gain.
    pub gain: f32,
    /// The pan position: `-1` is hard left, `+1` is hard right.
    pub pan: f32,
    /// The law used to pan a mono recording.
    pub pan_law: stagesync_core::mixer::PanLaw,
    /// How a stereo recording is panned.
    pub stereo_pan: stagesync_core::mixer::StereoPan,
}

impl MixMember {
    /// A member of the mix, at its own level and in the centre.
    pub fn new(path: std::path::PathBuf, model: LinearModel) -> Self {
        Self {
            path,
            model,
            gain: 1.0,
            pan: 0.0,
            pan_law: stagesync_core::mixer::PanLaw::EqualPower,
            stereo_pan: stagesync_core::mixer::StereoPan::Balance,
        }
    }

    /// The member described by a track of the mixer's settings.
    pub fn from_track(path: std::path::PathBuf, model: LinearModel, track: &MixTrack) -> Self {
        Self {
            path,
            model,
            gain: track.gain * track.trim,
            pan: track.pan,
            pan_law: track.pan_law,
            stereo_pan: track.stereo_pan,
        }
    }
}

/// Live mixer parameters updated atomically from the UI while playing a mix.
#[derive(Debug, Clone)]
pub struct LiveTrackUpdate {
    /// Combined effective linear gain (trim * fader, zero when muted or inactive by solo).
    pub gain: f32,
    /// Pan position in `[-1.0, 1.0]`.
    pub pan: f32,
    /// Mono pan law.
    pub pan_law: stagesync_core::mixer::PanLaw,
    /// Stereo pan mode.
    pub stereo_pan: stagesync_core::mixer::StereoPan,
}

#[derive(Debug, Clone, Default)]
struct LiveMixUpdate {
    master_gain: f32,
    protection: ClippingProtection,
    autogain_trim: f32,
    tracks: Vec<LiveTrackUpdate>,
}

type LiveMixShared = Arc<(AtomicU64, Mutex<LiveMixUpdate>)>;

/// Output backend abstraction so audio device work runs off the UI thread and can be
/// substituted with stalled or simulated devices in tests.
pub trait AudioBackend: Send + 'static {
    /// Output device sample rate in Hz.
    fn sample_rate(&self) -> u32;
    /// Whether an output device is open and ready to accept audio.
    fn is_available(&self) -> bool;
    /// Error from opening the device, if unavailable.
    fn init_error(&self) -> Option<&str>;
    /// Start playing `source` at `volume`, replacing any previous stream.
    fn play(
        &mut self,
        source: Box<dyn Source<Item = f32> + Send>,
        volume: f32,
    ) -> Result<(), String>;
    /// Halt playback immediately without blocking indefinitely on device drain.
    fn stop(&mut self);
    /// Adjust master monitor volume (`0.0..=1.0`).
    fn set_volume(&mut self, volume: f32);
    /// Return any runtime device error detected by the backend.
    fn poll_error(&mut self) -> Option<String> {
        None
    }
}

/// Real `rodio` audio backend that creates a fresh `RodioPlayer` per stream and never
/// calls `RodioPlayer::clear()` (which blocks in `sleep_until_end()` if a device stalls).
struct RodioBackend {
    sink: Option<rodio::MixerDeviceSink>,
    player: Option<RodioPlayer>,
    rate: u32,
    init_error: Option<String>,
}

impl RodioBackend {
    fn open_default() -> Self {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let rate = sink.config().sample_rate().get();
                Self {
                    sink: Some(sink),
                    player: None,
                    rate,
                    init_error: None,
                }
            }
            Err(error) => Self {
                sink: None,
                player: None,
                rate: 48_000,
                init_error: Some(format!("no audio output device: {error}")),
            },
        }
    }
}

impl AudioBackend for RodioBackend {
    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn is_available(&self) -> bool {
        self.sink.is_some()
    }

    fn init_error(&self) -> Option<&str> {
        self.init_error.as_deref()
    }

    fn play(
        &mut self,
        source: Box<dyn Source<Item = f32> + Send>,
        volume: f32,
    ) -> Result<(), String> {
        let Some(sink) = &self.sink else {
            return Err(self
                .init_error
                .clone()
                .unwrap_or_else(|| "no audio output device".to_string()));
        };
        if let Some(prev) = self.player.take() {
            prev.stop();
            drop(prev);
        }
        let player = RodioPlayer::connect_new(sink.mixer());
        player.set_volume(volume);
        player.append(source);
        player.play();
        self.player = Some(player);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(prev) = self.player.take() {
            prev.stop();
            drop(prev);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        if let Some(player) = &self.player {
            player.set_volume(volume);
        }
    }
}

/// Source wrapper enabling wait-free UI cancellation, accurate consumed-frame counting,
/// and reliable end-of-stream / drop completion tracking.
struct StoppableSource {
    inner: Box<dyn Source<Item = f32> + Send>,
    channels: u16,
    sample_in_frame: u16,
    cancelled: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    frames_played: Arc<AtomicU64>,
}

impl StoppableSource {
    fn wrap(
        inner: Box<dyn Source<Item = f32> + Send>,
        cancelled: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
        frames_played: Arc<AtomicU64>,
    ) -> Self {
        let channels = inner.channels().get().max(1);
        Self {
            inner,
            channels,
            sample_in_frame: 0,
            cancelled,
            finished,
            frames_played,
        }
    }
}

impl Iterator for StoppableSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.cancelled.load(Ordering::Acquire) {
            self.finished.store(true, Ordering::Release);
            return None;
        }
        match self.inner.next() {
            Some(sample) => {
                self.sample_in_frame += 1;
                if self.sample_in_frame >= self.channels {
                    self.sample_in_frame = 0;
                    self.frames_played.fetch_add(1, Ordering::Relaxed);
                }
                Some(sample)
            }
            None => {
                self.finished.store(true, Ordering::Release);
                None
            }
        }
    }
}

impl Source for StoppableSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

impl Drop for StoppableSource {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::Release);
    }
}

enum BackendCmd {
    Play {
        cmd_id: u64,
        source: Box<dyn Source<Item = f32> + Send>,
        volume: f32,
    },
    Stop {
        cmd_id: u64,
    },
    SetVolume {
        cmd_id: u64,
        volume: f32,
    },
    Shutdown,
}

/// What is playing, and where it started.
struct Playing {
    voice: Voice,
    /// The position on the reference timeline the playback started at.
    start_secs: f64,
    /// Atomic frames already consumed by the audio output backend.
    frames_played: Arc<AtomicU64>,
    /// Wait-free cancellation flag read on the audio thread.
    cancelled: Arc<AtomicBool>,
    /// Set to `true` once the audio source exhausts or is stopped/dropped.
    finished: Arc<AtomicBool>,
    /// Shared live-update state when playing `Voice::Mix`.
    live_mix: Option<LiveMixShared>,
}

/// The player.
///
/// Audio device control executes on a dedicated background worker so stop, voice switch,
/// seek, and loop jumps never block the GUI thread even if an audio driver stalls.
pub struct Player {
    cmd_tx: Option<Sender<BackendCmd>>,
    available: bool,
    playing: Option<Playing>,
    /// Why there is no output, when there is none or a backend failure occurred.
    error: Option<String>,
    /// Asynchronous runtime error published by the backend worker thread.
    runtime_error: Arc<Mutex<Option<String>>>,
    next_cmd_id: u64,
    last_sent_cmd: Option<(u64, Instant)>,
    ack_cmd: Arc<AtomicU64>,
    stall_reported: bool,
    /// The rate the output device runs at.
    rate: u32,
    volume: f32,
    /// The loudest sample the mix has produced, shared with the playing source.
    mix_peak: Arc<AtomicU32>,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(playing) = self.playing.take() {
            playing.cancelled.store(true, Ordering::Release);
        }
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(BackendCmd::Shutdown);
        }
    }
}

impl Player {
    /// Open the audio output, if the machine has one.
    pub fn new() -> Self {
        Self::spawn_with_factory(RodioBackend::open_default)
    }

    /// Build a `Player` backed by a custom [`AudioBackend`] (used for stalled/mock device tests).
    pub fn with_backend<B: AudioBackend>(backend: B) -> Self {
        Self::spawn_with_factory(move || backend)
    }

    fn spawn_with_factory<B: AudioBackend, F: FnOnce() -> B + Send + 'static>(factory: F) -> Self {
        let (init_tx, init_rx) = mpsc::channel::<(bool, u32, Option<String>)>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<BackendCmd>();
        let ack_cmd = Arc::new(AtomicU64::new(0));
        let runtime_error = Arc::new(Mutex::new(None));
        let ack_worker = Arc::clone(&ack_cmd);
        let err_worker = Arc::clone(&runtime_error);

        thread_spawn_worker(factory, init_tx, cmd_rx, ack_worker, err_worker);

        let (available, rate, error) = init_rx.recv_timeout(Duration::from_secs(2)).unwrap_or((
            false,
            48_000,
            Some("audio output initialisation timed out".to_string()),
        ));

        Self {
            cmd_tx: Some(cmd_tx),
            available,
            playing: None,
            error,
            runtime_error,
            next_cmd_id: 1,
            last_sent_cmd: None,
            ack_cmd,
            stall_reported: false,
            rate: rate.max(1),
            volume: 1.0,
            mix_peak: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Whether the machine can make a sound.
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// Why there is no sound, when there is none.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Take and clear any runtime error reported asynchronously by the audio backend.
    pub fn take_error(&mut self) -> Option<String> {
        let err = self.runtime_error.lock().ok().and_then(|mut g| g.take());
        if let Some(ref e) = err {
            self.error = Some(e.clone());
        }
        err
    }

    /// The rate the output device runs at.
    pub fn output_rate(&self) -> u32 {
        self.rate
    }

    /// The volume, between zero and one.
    pub fn volume(&self) -> f32 {
        self.volume
    }

    fn send_cmd(&mut self, build: impl FnOnce(u64) -> BackendCmd) {
        let Some(tx) = &self.cmd_tx else {
            return;
        };
        let id = self.next_cmd_id;
        self.next_cmd_id += 1;
        if tx.send(build(id)).is_ok() {
            self.last_sent_cmd = Some((id, Instant::now()));
            self.stall_reported = false;
        }
    }

    /// Set the volume, between zero and one.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        let vol = self.volume;
        self.send_cmd(|cmd_id| BackendCmd::SetVolume {
            cmd_id,
            volume: vol,
        });
    }

    /// What is playing, if anything.
    pub fn voice(&self) -> Option<Voice> {
        self.playing.as_ref().map(|playing| playing.voice)
    }

    /// Whether something is playing right now.
    pub fn is_playing(&self) -> bool {
        let Some(playing) = &self.playing else {
            return false;
        };
        !playing.cancelled.load(Ordering::Acquire) && !playing.finished.load(Ordering::Acquire)
    }

    /// Reconcile local state with the audio backend; returns `true` if playback just ended.
    pub fn poll(&mut self) -> bool {
        if let Some((sent_id, sent_at)) = self.last_sent_cmd {
            if self.ack_cmd.load(Ordering::Acquire) >= sent_id {
                self.last_sent_cmd = None;
                self.stall_reported = false;
            } else if !self.stall_reported && sent_at.elapsed() >= Duration::from_secs(2) {
                self.stall_reported = true;
                if let Ok(mut guard) = self.runtime_error.lock() {
                    if guard.is_none() {
                        *guard = Some("audio output device stalled or disconnected".to_string());
                    }
                }
            }
        }
        if self.playing.is_some() && !self.is_playing() {
            self.playing = None;
            return true;
        }
        false
    }

    /// Push live master/track mix parameter updates without restarting the stream.
    pub fn update_live_mix(
        &self,
        master_gain: f32,
        protection: ClippingProtection,
        autogain_trim: f32,
        members: &[MixMember],
    ) {
        let Some(playing) = &self.playing else {
            return;
        };
        let Some(shared) = &playing.live_mix else {
            return;
        };
        if let Ok(mut guard) = shared.1.lock() {
            guard.master_gain = master_gain;
            guard.protection = protection;
            guard.autogain_trim = autogain_trim;
            guard.tracks = members
                .iter()
                .map(|m| LiveTrackUpdate {
                    gain: m.gain,
                    pan: m.pan,
                    pan_law: m.pan_law,
                    stereo_pan: m.stereo_pan,
                })
                .collect();
            shared.0.fetch_add(1, Ordering::Release);
        }
    }

    /// The loudest sample the mix that is playing has produced.
    ///
    /// It is zero when nothing is playing.
    pub fn mix_peak(&self) -> f32 {
        f32::from_bits(self.mix_peak.load(Ordering::Relaxed))
    }

    /// The position on the reference timeline, in seconds, derived from actual audio
    /// frames consumed by the output device rather than a free-running wall clock.
    pub fn position(&self) -> f64 {
        self.playing.as_ref().map_or(0.0, |playing| {
            let frames = playing.frames_played.load(Ordering::Relaxed);
            playing.start_secs + frames as f64 / self.rate.max(1) as f64
        })
    }

    /// Stop playing immediately without blocking on the audio device.
    pub fn stop(&mut self) {
        if let Some(playing) = self.playing.take() {
            playing.cancelled.store(true, Ordering::Release);
        }
        self.send_cmd(|cmd_id| BackendCmd::Stop { cmd_id });
    }

    /// Play the reference recording from a position on the reference timeline.
    pub fn play_reference(&mut self, path: &Path, from_secs: f64) -> Result<(), anyhow::Error> {
        let rate = self.rate;
        let source = ResampledSource::open(path, from_secs.max(0.0), rate, None)?;
        self.start(Voice::Reference, from_secs.max(0.0), Box::new(source), None)
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
        self.start(Voice::Target, from_secs, Box::new(source), None)
    }

    /// Put a source on the player backend and start tracking consumed frames.
    fn start(
        &mut self,
        voice: Voice,
        from_secs: f64,
        source: Box<dyn Source<Item = f32> + Send>,
        live_mix: Option<LiveMixShared>,
    ) -> Result<(), anyhow::Error> {
        if !self.available {
            return Err(anyhow::anyhow!(
                "there is no audio output device: {}",
                self.error.as_deref().unwrap_or("unknown reason")
            ));
        }
        if let Some(prev) = self.playing.take() {
            prev.cancelled.store(true, Ordering::Release);
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let frames_played = Arc::new(AtomicU64::new(0));
        let wrapped = StoppableSource::wrap(
            source,
            Arc::clone(&cancelled),
            Arc::clone(&finished),
            Arc::clone(&frames_played),
        );
        let volume = self.volume;
        self.send_cmd(move |cmd_id| BackendCmd::Play {
            cmd_id,
            source: Box::new(wrapped),
            volume,
        });
        self.playing = Some(Playing {
            voice,
            start_secs: from_secs,
            frames_played,
            cancelled,
            finished,
            live_mix,
        });
        Ok(())
    }

    /// Play several recordings at once, mixed with the chosen clipping protection and trim.
    pub fn play_mix(
        &mut self,
        members: &[MixMember],
        master_gain: f32,
        protection: ClippingProtection,
        autogain_trim: f32,
        from_secs: f64,
    ) -> Result<(), anyhow::Error> {
        let rate = self.rate;
        let from_secs = from_secs.max(0.0);
        let published_peak = Arc::clone(&self.mix_peak);
        published_peak.store(0.0f32.to_bits(), Ordering::Relaxed);
        let live_mix: LiveMixShared = Arc::new((
            AtomicU64::new(0),
            Mutex::new(LiveMixUpdate {
                master_gain,
                protection,
                autogain_trim,
                tracks: members
                    .iter()
                    .map(|m| LiveTrackUpdate {
                        gain: m.gain,
                        pan: m.pan,
                        pan_law: m.pan_law,
                        stereo_pan: m.stereo_pan,
                    })
                    .collect(),
            }),
        ));
        let source = MixSource::open_with_options(
            members,
            master_gain,
            protection,
            autogain_trim,
            from_secs,
            rate,
            published_peak,
            Some(Arc::clone(&live_mix)),
        )?;
        self.start(Voice::Mix, from_secs, Box::new(source), Some(live_mix))
    }
}

fn thread_spawn_worker<B: AudioBackend, F: FnOnce() -> B + Send + 'static>(
    factory: F,
    init_tx: Sender<(bool, u32, Option<String>)>,
    cmd_rx: Receiver<BackendCmd>,
    ack_cmd: Arc<AtomicU64>,
    runtime_error: Arc<Mutex<Option<String>>>,
) {
    let _ = std::thread::Builder::new()
        .name("stagesync-audio".to_string())
        .spawn(move || {
            let mut backend = factory();
            let _ = init_tx.send((
                backend.is_available(),
                backend.sample_rate().max(1),
                backend.init_error().map(str::to_string),
            ));

            while let Ok(first) = cmd_rx.recv() {
                // Coalesce queued commands so rapid seeks / A-B switches skip stale streams.
                let mut latest_cmd = Some(first);
                let mut latest_volume: Option<(u64, f32)> = None;
                let mut max_cmd_id = 0u64;
                let record_id = |cmd: &BackendCmd, max_id: &mut u64| match cmd {
                    BackendCmd::Play { cmd_id, .. }
                    | BackendCmd::Stop { cmd_id }
                    | BackendCmd::SetVolume { cmd_id, .. } => {
                        *max_id = (*max_id).max(*cmd_id);
                    }
                    BackendCmd::Shutdown => {}
                };
                if let Some(ref c) = latest_cmd {
                    record_id(c, &mut max_cmd_id);
                    if let BackendCmd::SetVolume { cmd_id, volume } = c {
                        latest_volume = Some((*cmd_id, *volume));
                        latest_cmd = None;
                    }
                }
                while let Ok(next_cmd) = cmd_rx.try_recv() {
                    record_id(&next_cmd, &mut max_cmd_id);
                    match next_cmd {
                        BackendCmd::Shutdown => {
                            backend.stop();
                            return;
                        }
                        BackendCmd::SetVolume { cmd_id, volume } => {
                            latest_volume = Some((cmd_id, volume));
                        }
                        other => {
                            latest_cmd = Some(other);
                        }
                    }
                }

                if let Some((_, vol)) = latest_volume {
                    backend.set_volume(vol);
                }
                if let Some(cmd) = latest_cmd {
                    match cmd {
                        BackendCmd::Play { source, volume, .. } => {
                            if let Err(err) = backend.play(source, volume) {
                                if let Ok(mut guard) = runtime_error.lock() {
                                    *guard = Some(err);
                                }
                            }
                        }
                        BackendCmd::Stop { .. } => {
                            backend.stop();
                        }
                        BackendCmd::SetVolume { volume, .. } => {
                            backend.set_volume(volume);
                        }
                        BackendCmd::Shutdown => {
                            backend.stop();
                            return;
                        }
                    }
                }
                if let Some(err) = backend.poll_error() {
                    if let Ok(mut guard) = runtime_error.lock() {
                        *guard = Some(err);
                    }
                }
                if max_cmd_id > 0 {
                    ack_cmd.fetch_max(max_cmd_id, Ordering::Release);
                }
            }
            backend.stop();
        });
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
    /// The number of silent frames before the recording's material begins.
    ///
    /// A recording that starts later on the reference timeline than the position
    /// the playback started at is silent until its own start.
    delay: u64,
}

impl ResampledSource {
    /// Open a recording at a position.
    ///
    /// `model` is the correction to apply. `None` means the recording is played as
    /// it is — that is the reference, which is never altered.
    ///
    /// The position is a position on the **reference** timeline. For a corrected
    /// target it is turned into a position in the recording by the model.
    ///
    /// The recording is placed on the reference timeline exactly as the export places
    /// it: a recording that has not started yet at the given position contributes
    /// silence until the position its model says it starts at, and the material is
    /// read from the fraction of a frame the model puts it at.
    fn open(
        path: &Path,
        from_secs: f64,
        out_rate: u32,
        model: Option<&LinearModel>,
    ) -> Result<Self, anyhow::Error> {
        let mut decoder = AudioDecoder::open(path)?;
        let in_rate = decoder.meta().sample_rate;
        let channels = decoder.meta().channels.max(1);

        // Where the recording's material sits on the reference timeline, and how the
        // rate changes.
        //
        // The output advances along the reference timeline at one second per second,
        // whatever the recording's own rate is. For the reference that is simply
        // its own rate; for a corrected target the drift stretches the material, so
        // the recording is read more slowly than it is written.
        let (alpha, gamma) = match model {
            None => (in_rate as f64 / out_rate as f64, 0.0),
            Some(model) => (
                in_rate as f64 / (out_rate as f64 * model.scale),
                -model.offset * in_rate as f64 / model.scale,
            ),
        };

        // The position in the recording that the given reference position falls on.
        let position = alpha * (from_secs * out_rate as f64) + gamma;
        let total_frames = decoder.meta().frames;
        if total_frames > 0 && position >= total_frames as f64 {
            // Seeking past EOF must yield silence immediately rather than clamping to
            // the final packet and repeating tail samples.
            let resamplers = (0..channels)
                .map(|_| StreamingResampler::new(alpha, 0.0, TAPS, PHASES))
                .collect();
            return Ok(Self {
                decoder,
                resamplers,
                ready: VecDeque::new(),
                channels: channels as u16,
                rate: out_rate,
                finished: true,
                delay: 0,
            });
        }
        let (start_frame, fraction, delay) = match model {
            None => {
                let start = position.floor().max(0.0);
                (start as u64, position - start, 0)
            }
            Some(model) => {
                if position >= 0.0 {
                    let start = position.floor();
                    (start as u64, position - start, 0)
                } else {
                    // The recording has not started yet at this position: it stays
                    // silent until the position its model gives it.
                    let start_at = model.offset;
                    let delay = ((start_at - from_secs).max(0.0) * out_rate as f64).round() as u64;
                    (0, 0.0, delay)
                }
            }
        };

        decoder.seek_to_frame(start_frame)?;

        // The resampler is given the fraction of the frame the material starts on,
        // so the preview reads the same position the export does.
        let resamplers = (0..channels)
            .map(|_| StreamingResampler::new(alpha, fraction, TAPS, PHASES))
            .collect();

        Ok(Self {
            decoder,
            resamplers,
            ready: VecDeque::new(),
            channels: channels as u16,
            rate: out_rate,
            finished: false,
            delay,
        })
    }

    /// Decode and resample until there is something to play.
    ///
    /// Returns `false` when the recording has nothing more to give.
    fn fill(&mut self) -> bool {
        if self.delay > 0 {
            // Emit leading reference-timeline silence inside fill() so both
            // Iterator::next (solo playback) and next_frame (mix) align identically.
            let emit_frames = self.delay.min(BLOCK as u64) as usize;
            self.delay -= emit_frames as u64;
            let zeros = emit_frames * self.channels as usize;
            self.ready.reserve(zeros);
            for _ in 0..zeros {
                self.ready.push_back(0.0);
            }
            return true;
        }
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

    /// The next frame of the recording's own channels.
    ///
    /// `false` means the recording has nothing more to give. The mixer treats that
    /// as silence, so a recording that ends leaves no click behind.
    fn next_frame(&mut self, out: &mut [f32]) -> bool {
        let channels = self.channels as usize;
        loop {
            if self.ready.len() >= channels {
                for sample in out.iter_mut().take(channels) {
                    *sample = self.ready.pop_front().unwrap_or(0.0);
                }
                return true;
            }
            if !self.fill() {
                return false;
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

/// One recording of the mix, read at the output's rate.
struct MixMemberSource {
    /// The recording itself.
    source: ResampledSource,
    /// Native channels of this recording.
    channels: usize,
    /// The matrix that places the recording in the field.
    matrix: PanMatrix,
    /// The fader.
    gain: f32,
    /// The recording's current frame, in its own channels.
    frame: Vec<f32>,
}

/// Several recordings, mixed, played at once.
///
/// The mix is the core's own arithmetic: each recording is read through its own
/// position map — the same drift correction the export applies — and the pan
/// matrices and gains are the core's own.
struct MixSource {
    /// The recordings of the mix.
    members: Vec<MixMemberSource>,
    /// The interleaved stereo output, waiting to be played.
    ready: VecDeque<f32>,
    /// The rate the output runs at.
    rate: u32,
    /// The master fader.
    master_gain: f32,
    /// Export clipping protection applied identically in preview.
    protection: ClippingProtection,
    /// Two-pass automatic gain reduction trim (`1.0` unless AutoGain measured a trim).
    autogain_trim: f32,
    /// The loudest sample the mix has produced.
    peak: f32,
    /// How many samples above 0 dBFS the mix has produced.
    overs: u64,
    /// Where the peak is published for the interface to read.
    published_peak: Arc<AtomicU32>,
    /// Shared live-update handle from `Player`.
    live_mix: Option<LiveMixShared>,
    /// Last applied live-update generation.
    applied_version: u64,
}

impl MixSource {
    /// Open the recordings of the mix at a position on the reference timeline.
    #[allow(dead_code)]
    fn open(
        members: &[MixMember],
        master_gain: f32,
        from_secs: f64,
        out_rate: u32,
        published_peak: Arc<AtomicU32>,
    ) -> Result<Self, anyhow::Error> {
        Self::open_with_options(
            members,
            master_gain,
            ClippingProtection::None,
            1.0,
            from_secs,
            out_rate,
            published_peak,
            None,
        )
    }

    #[allow(dead_code)]
    fn open_with_live(
        members: &[MixMember],
        master_gain: f32,
        from_secs: f64,
        out_rate: u32,
        published_peak: Arc<AtomicU32>,
        live_mix: Option<LiveMixShared>,
    ) -> Result<Self, anyhow::Error> {
        Self::open_with_options(
            members,
            master_gain,
            ClippingProtection::None,
            1.0,
            from_secs,
            out_rate,
            published_peak,
            live_mix,
        )
    }

    /// Open the mix with full clipping protection, auto-gain trim, and optional live updates.
    #[allow(clippy::too_many_arguments)]
    fn open_with_options(
        members: &[MixMember],
        master_gain: f32,
        protection: ClippingProtection,
        autogain_trim: f32,
        from_secs: f64,
        out_rate: u32,
        published_peak: Arc<AtomicU32>,
        live_mix: Option<LiveMixShared>,
    ) -> Result<Self, anyhow::Error> {
        let mut sources = Vec::with_capacity(members.len());
        for member in members {
            let source =
                ResampledSource::open(&member.path, from_secs, out_rate, Some(&member.model))?;
            // The matrix is the core's own, so the preview and the export place the
            // recording in the field in exactly the same way.
            let channels = source.channels as usize;
            let matrix =
                PanMatrix::for_source(channels, member.pan, member.pan_law, member.stereo_pan);
            sources.push(MixMemberSource {
                source,
                channels,
                matrix,
                gain: member.gain,
                frame: vec![0.0; channels.max(1)],
            });
        }
        let applied_version = live_mix
            .as_ref()
            .map(|s| s.0.load(Ordering::Acquire))
            .unwrap_or(0);
        Ok(Self {
            members: sources,
            ready: VecDeque::new(),
            rate: out_rate,
            master_gain,
            protection,
            autogain_trim,
            peak: 0.0,
            overs: 0,
            published_peak,
            live_mix,
            applied_version,
        })
    }

    /// Produce the next output frame.
    fn fill(&mut self) -> bool {
        if let Some(shared) = &self.live_mix {
            let ver = shared.0.load(Ordering::Acquire);
            if ver != self.applied_version {
                if let Ok(guard) = shared.1.try_lock() {
                    self.master_gain = guard.master_gain;
                    self.protection = guard.protection;
                    self.autogain_trim = guard.autogain_trim;
                    for (member, upd) in self.members.iter_mut().zip(guard.tracks.iter()) {
                        member.gain = upd.gain;
                        member.matrix = PanMatrix::for_source(
                            member.channels,
                            upd.pan,
                            upd.pan_law,
                            upd.stereo_pan,
                        );
                    }
                    self.applied_version = ver;
                }
            }
        }
        let mut left = 0f32;
        let mut right = 0f32;
        let mut any = false;

        for member in self.members.iter_mut() {
            if !member.source.next_frame(&mut member.frame) {
                // The recording has ended: it contributes nothing more, and the
                // silence it leaves is exactly silence, so there is no click.
                continue;
            }
            any = true;
            let mut out = [0f32; 2];
            member.matrix.apply(&member.frame, &mut out);
            left += out[0] * member.gain;
            right += out[1] * member.gain;
        }
        if !any {
            return false;
        }

        let effective_master = self.master_gain * self.autogain_trim;
        let left = self.protection.apply(left, effective_master);
        let right = self.protection.apply(right, effective_master);
        self.peak = self.peak.max(left.abs()).max(right.abs());
        // The peak is published as it grows, so the interface can show what the
        // mix is actually doing rather than what it is expected to do.
        self.published_peak
            .store(self.peak.to_bits(), Ordering::Relaxed);
        if left.abs() > 1.0 {
            self.overs += 1;
        }
        if right.abs() > 1.0 {
            self.overs += 1;
        }
        self.ready.push_back(left);
        self.ready.push_back(right);
        true
    }
}

/// Render stereo frames from [`MixSource`] using the exact real-time preview pipeline
/// (used to verify sample-level parity with exported mixes in unit tests).
#[cfg(test)]
pub(crate) fn collect_preview_mix_frames(
    members: &[MixMember],
    master_gain: f32,
    protection: ClippingProtection,
    autogain_trim: f32,
    from_secs: f64,
    rate: u32,
    max_frames: usize,
) -> Result<Vec<[f32; 2]>, anyhow::Error> {
    let mut source = MixSource::open_with_options(
        members,
        master_gain,
        protection,
        autogain_trim,
        from_secs,
        rate,
        Arc::new(AtomicU32::new(0)),
        None,
    )?;
    let mut frames = Vec::with_capacity(max_frames);
    for _ in 0..max_frames {
        let Some(l) = source.next() else {
            break;
        };
        let r = source.next().unwrap_or(0.0);
        frames.push([l, r]);
    }
    Ok(frames)
}

impl Iterator for MixSource {
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

impl Source for MixSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(2).expect("two is not zero")
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(self.rate).unwrap_or(NonZero::new(48_000).expect("48000 is not zero"))
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
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

    /// A recording of a constant value.
    fn constant(path: &Path, value: f32, frames: usize, rate: u32) {
        let samples = vec![value; frames];
        let mut writer = WavWriter::create(path, rate, 1, WavFormat::Float32).expect("create");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write");
        writer.finalize().expect("finalise");
    }

    #[test]
    fn two_recordings_are_played_at_once_and_mixed() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000u32;
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        constant(&a, 0.25, rate as usize, rate);
        constant(&b, 0.25, rate as usize, rate);

        let members = vec![
            MixMember::new(a, LinearModel::new(1.0, 0.0)),
            MixMember::new(b, LinearModel::new(1.0, 0.0)),
        ];
        let peak_cell = Arc::new(AtomicU32::new(0));
        let mut source =
            MixSource::open(&members, 1.0, 0.0, rate, Arc::clone(&peak_cell)).expect("open");

        // The first frame of the mix, read as a stereo frame.
        let left = source.next().expect("a sample");
        let right = source.next().expect("a sample");
        // Each recording is panned to the centre with the equal-power law, so each
        // contributes 0.25 * 0.7071 and the mix is the sum.
        let expected = 2.0 * 0.25 * 0.5f32.sqrt();
        assert!(
            (left - expected).abs() < 1e-5,
            "the mix was {left} and it should be {expected}"
        );
        assert!(
            (right - expected).abs() < 1e-5,
            "the mix was {right} and it should be {expected}"
        );
        // The peak of the mix is published for the interface to read.
        let published = f32::from_bits(peak_cell.load(Ordering::Relaxed));
        assert!(published >= left.abs());
    }

    #[test]
    fn a_panned_recording_is_played_on_one_side_only() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000u32;
        let a = dir.path().join("a.wav");
        constant(&a, 0.5, rate as usize, rate);

        let mut member = MixMember::new(a, LinearModel::new(1.0, 0.0));
        member.pan = -1.0;
        let mut source =
            MixSource::open(&[member], 1.0, 0.0, rate, Arc::new(AtomicU32::new(0))).expect("open");

        let left = source.next().expect("a sample");
        let right = source.next().expect("a sample");
        assert!(
            (left - 0.5).abs() < 1e-5,
            "the left channel was {left} and it should be 0.5"
        );
        assert!(
            right.abs() < 1e-6,
            "the right channel was not silent: {right}"
        );
    }

    #[test]
    fn a_recording_that_starts_late_is_silent_until_its_own_start() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000u32;
        let a = dir.path().join("a.wav");
        constant(&a, 0.5, rate as usize, rate);

        // The recording belongs half a second into the reference timeline. Played
        // from the beginning of the timeline it must be silent until then: the
        // preview places the recording where its model says it is, exactly as the
        // export does.
        let offset = 0.5;
        let members = vec![MixMember::new(a, LinearModel::new(1.0, offset))];
        let mut source =
            MixSource::open(&members, 1.0, 0.0, rate, Arc::new(AtomicU32::new(0))).expect("open");

        let start = (offset * rate as f64) as usize;
        for frame in 0..start {
            let left = source.next().expect("a sample");
            let right = source.next().expect("a sample");
            assert!(
                left.abs() < 1e-6 && right.abs() < 1e-6,
                "the recording was heard at frame {frame}, before its own start at {start}: {left}"
            );
        }
        let left = source.next().expect("a sample");
        let right = source.next().expect("a sample");
        let expected = 0.5 * 0.5f32.sqrt();
        assert!(
            (left - expected).abs() < 1e-5,
            "the recording was not at its own start: {left} instead of {expected}"
        );
        assert!(
            (right - expected).abs() < 1e-5,
            "the recording was not at its own start: {right} instead of {expected}"
        );
    }

    #[test]
    fn a_recording_that_ends_leaves_silence_and_no_click() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000u32;
        let a = dir.path().join("a.wav");
        // A quarter of a second of material.
        constant(&a, 0.5, rate as usize / 4, rate);

        let members = vec![MixMember::new(a, LinearModel::new(1.0, 0.0))];
        let mut source =
            MixSource::open(&members, 1.0, 0.0, rate, Arc::new(AtomicU32::new(0))).expect("open");

        let mut last: Option<f32> = None;
        let mut samples = 0usize;
        for sample in source.by_ref() {
            // Every sample before the end is the same: there is no click when the
            // recording stops.
            if let Some(previous) = last {
                if samples < rate as usize / 4 - 2 {
                    assert!(
                        (sample - previous).abs() < 1e-6,
                        "the mix jumped at sample {samples}: {previous} then {sample}"
                    );
                }
            }
            last = Some(sample);
            samples += 1;
        }
        assert!(samples > rate as usize / 4, "the recording was cut short");
    }

    #[test]
    fn seeking_past_eof_produces_silence_not_tail_repeat() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("short.wav");
        let rate = 8000u32;
        tone(&path, rate, 1.0, 220.0);

        let source = ResampledSource::open(&path, 5.0, rate, None).expect("open");
        assert_eq!(source.count(), 0, "seeking past EOF must emit 0 samples");
    }

    #[test]
    fn live_mix_parameters_update_gain_without_reopening() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000u32;
        let a = dir.path().join("a.wav");
        constant(&a, 0.4, rate as usize, rate);

        let members = vec![MixMember::new(a, LinearModel::new(1.0, 0.0))];
        let shared: LiveMixShared = Arc::new((
            AtomicU64::new(0),
            Mutex::new(LiveMixUpdate {
                master_gain: 1.0,
                protection: ClippingProtection::None,
                autogain_trim: 1.0,
                tracks: vec![LiveTrackUpdate {
                    gain: 1.0,
                    pan: 0.0,
                    pan_law: stagesync_core::mixer::PanLaw::EqualPower,
                    stereo_pan: stagesync_core::mixer::StereoPan::Balance,
                }],
            }),
        ));
        let mut source = MixSource::open_with_live(
            &members,
            1.0,
            0.0,
            rate,
            Arc::new(AtomicU32::new(0)),
            Some(Arc::clone(&shared)),
        )
        .expect("open");
        let initial = source.next().expect("sample");
        assert!((initial - 0.4 * 0.5f32.sqrt()).abs() < 1e-4);
        let _ = source.next(); // consume R

        {
            let mut guard = shared.1.lock().unwrap();
            guard.master_gain = 0.5;
            guard.tracks[0].gain = 0.5;
            shared.0.fetch_add(1, Ordering::Release);
        }
        let updated = source.next().expect("sample");
        assert!(
            (updated - 0.1 * 0.5f32.sqrt()).abs() < 1e-4,
            "live update should reduce sample to 0.25x, got {updated}"
        );
    }

    #[test]
    fn p1_05_resampled_source_iterator_emits_leading_silence_before_target_start() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let rate = 8000u32;
        let a = dir.path().join("target.wav");
        constant(&a, 0.5, rate as usize, rate);

        let offset = 0.25;
        let model = LinearModel::new(1.0, offset);
        let mut source = ResampledSource::open(&a, 0.0, rate, Some(&model)).expect("open");

        let expected_silence = (offset * rate as f64).round() as usize;
        for frame in 0..expected_silence {
            let sample = source.next().expect("leading silence sample");
            assert!(
                sample.abs() < 1e-6,
                "ResampledSource skipped leading silence at frame {frame}: got {sample}"
            );
        }
        let first_audio = source.next().expect("audio sample after delay");
        assert!(
            (first_audio - 0.5).abs() < 0.05,
            "expected ~0.5 after delay, got {first_audio}"
        );
    }

    #[test]
    fn p1_07_stalled_backend_never_blocks_stop_seek_or_voice_switch() {
        struct StalledBackend {
            gate: Arc<Mutex<()>>,
        }
        impl AudioBackend for StalledBackend {
            fn sample_rate(&self) -> u32 {
                8_000
            }
            fn is_available(&self) -> bool {
                true
            }
            fn init_error(&self) -> Option<&str> {
                None
            }
            fn play(
                &mut self,
                _source: Box<dyn Source<Item = f32> + Send>,
                _volume: f32,
            ) -> Result<(), String> {
                let _hold = self.gate.lock();
                Ok(())
            }
            fn stop(&mut self) {
                let _hold = self.gate.lock();
            }
            fn set_volume(&mut self, _volume: f32) {}
        }

        let dir = tempfile::TempDir::new().expect("temp dir");
        let a = dir.path().join("a.wav");
        tone(&a, 8_000, 2.0, 220.0);

        let gate = Arc::new(Mutex::new(()));
        let held = gate.lock().unwrap();
        let mut player = Player::with_backend(StalledBackend {
            gate: Arc::clone(&gate),
        });

        let started = Instant::now();
        player
            .play_reference(&a, 0.0)
            .expect("play succeeds non-blocking");
        assert!(player.is_playing());
        // Switch voice & stop while the backend is completely deadlocked on `gate`.
        player
            .play_reference(&a, 0.5)
            .expect("seek/switch non-blocking");
        player.stop();
        assert!(!player.is_playing());
        assert!(
            started.elapsed() < Duration::from_millis(150),
            "stop/switch blocked GUI thread for {:?}",
            started.elapsed()
        );
        drop(held);
    }

    #[test]
    fn p1_07_position_follows_consumed_frames_not_wall_clock() {
        struct PullingBackend {
            held: Arc<Mutex<Option<Box<dyn Source<Item = f32> + Send>>>>,
        }
        impl AudioBackend for PullingBackend {
            fn sample_rate(&self) -> u32 {
                8_000
            }
            fn is_available(&self) -> bool {
                true
            }
            fn init_error(&self) -> Option<&str> {
                None
            }
            fn play(
                &mut self,
                mut source: Box<dyn Source<Item = f32> + Send>,
                _volume: f32,
            ) -> Result<(), String> {
                // Consume exactly 2,000 mono frames (= 0.25 s at 8 kHz), then hold without draining.
                for _ in 0..2_000 {
                    let _ = source.next();
                }
                *self.held.lock().unwrap() = Some(source);
                Ok(())
            }
            fn stop(&mut self) {
                *self.held.lock().unwrap() = None;
            }
            fn set_volume(&mut self, _volume: f32) {}
        }

        let dir = tempfile::TempDir::new().expect("temp dir");
        let a = dir.path().join("a.wav");
        tone(&a, 8_000, 2.0, 220.0);

        let held = Arc::new(Mutex::new(None));
        let mut player = Player::with_backend(PullingBackend {
            held: Arc::clone(&held),
        });
        player.play_reference(&a, 1.0).expect("play");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && player.position() < 1.24 {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Sleep 120 ms with zero additional frames consumed: position MUST stay pinned at 1.25 s.
        std::thread::sleep(Duration::from_millis(120));
        let pos = player.position();
        assert!(
            (pos - 1.25).abs() < 1e-3,
            "position drifted away from consumed frames (got {pos}, expected 1.25)"
        );
        player.stop();
    }

    #[test]
    fn live_audio_device_stop_and_switch_do_not_freeze() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let a = dir.path().join("a.wav");
        tone(&a, 48_000, 2.0, 220.0);

        let mut player = Player::new();
        if !player.is_available() {
            return;
        }
        let t0 = Instant::now();
        player.play_reference(&a, 0.0).expect("start play");
        std::thread::sleep(Duration::from_millis(40));
        player.play_reference(&a, 0.5).expect("voice/seek switch");
        std::thread::sleep(Duration::from_millis(20));
        player.stop();
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "live device switch/stop took too long: {:?}",
            t0.elapsed()
        );
        assert!(!player.is_playing());
    }
}
