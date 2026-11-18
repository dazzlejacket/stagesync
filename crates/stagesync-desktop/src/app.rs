//! The StageSync window.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui::{self, Color32};
use stagesync_core::audio::{PeakCache, SourceMeta};
use stagesync_core::matching::drift::DriftAssessment;
use stagesync_core::matching::{AlignmentReport, SegmentKind, SourceAlignment};
use stagesync_core::pipeline::{AnalysisConfig, ManualAnchor};
use stagesync_core::render::Layout;
use stagesync_core::verify::{residual_stats, HoldoutResult};

use crate::chart;
use crate::jobs::{self, ExportOutcome, JobEvent, JobScope, ScopedJobEvent};
use crate::platform;
use crate::player::{Player, Voice};
use crate::project::{
    self, MediaStatus, ProjectAnalysisOptions, ProjectExportSettings, ProjectMediaRef,
    ProjectMixerState, ProjectTargetEntry, ProjectTimelineState, ProjectVoice, StageSyncProject,
    PROJECT_EXTENSION,
};
use crate::settings::{self, AppSettings};
use crate::state::{
    container_label, drift_verdict, expand_dropped_paths, format_duration, format_label, is_wav,
    layout_label, loudness_label, metrics_for, protection_label, status_for, verification_for,
    AnchorFields, ExportSettings, FileEntry, TaskProgress,
};
use crate::theme::{Palette, ThemeMode};
use crate::timeline;
use crate::waveview;

/// What the user has selected in the file list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selection {
    /// The reference recording.
    Reference,
    /// One of the target recordings.
    Target(usize),
}

/// Which kind of background job is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobKind {
    Analysis,
    Export,
}

/// User action deferred pending unsaved-changes confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingAction {
    NewProject,
    PickOpenProject,
    OpenProject(PathBuf),
    CloseWindow,
}

/// Active panel tab in the lower workspace dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DockTab {
    #[default]
    Mixer,
    Inspector,
    Manual,
}

/// Deferred manual state mutation committed only when re-analysis finishes cleanly.
#[derive(Debug, Clone, Copy)]
enum PendingAnalysisCommit {
    ApplyNudge {
        target_index: usize,
        proposed_anchor: AnchorFields,
    },
    ResetToAutomatic {
        target_index: usize,
    },
}

/// A background job that is currently running.
struct RunningJob {
    kind: JobKind,
    job_id: u64,
    cancel: Arc<AtomicBool>,
}

/// The figures of one source, copied out of the report.
///
/// The copy lets the detail panel offer buttons that change the report without holding
/// a borrow of it while the buttons are drawn.
struct SourceFacts {
    name: String,
    duration: f64,
    offset_secs: Option<f64>,
    accumulated_ms: Option<f64>,
    coverage: f64,
    covered_secs: f64,
    low_secs: f64,
    uncovered_secs: f64,
    residuals: (f64, f64, f64),
    point_count: usize,
    warnings: Vec<String>,
    holdout: Option<HoldoutResult>,
    /// The verdict on the drift, when the analysis reached one.
    drift: Option<DriftAssessment>,
}

/// The StageSync application.
pub struct StageSyncApp {
    reference: Option<FileEntry>,
    targets: Vec<FileEntry>,
    selection: Option<Selection>,
    report: Option<AlignmentReport>,
    running: Option<RunningJob>,
    pending_analysis_commit: Option<PendingAnalysisCommit>,
    verifying: bool,
    verifying_job: Option<u64>,
    session_gen: u64,
    next_job_id: u64,
    latest_probe_jobs: HashMap<PathBuf, u64>,
    progress: Option<TaskProgress>,
    last_error: Option<String>,
    notice: Option<String>,
    log: Vec<String>,
    export_outcome: Option<ExportOutcome>,
    /// The manual anchor of each target recording, indexed the same way as `targets`.
    ///
    /// An anchor belongs to one recording. A recording set usually holds several
    /// files, and a single global anchor would drag every one of them onto the
    /// same correspondence, which is the very thing the anchor is for.
    anchors: Vec<AnchorFields>,
    settings: ExportSettings,
    /// Whether the independent log-mel cross-check is run during the analysis.
    use_spectral_check: bool,
    /// Whether the drift is measured on the raw waveform as well.
    measure_waveform_drift: bool,
    /// Whether the correction is offset-only, whatever the evidence says.
    force_offset_only: bool,
    /// The width of the window, as it is now.
    window_width: f32,
    /// The height of the window, as it is now.
    window_height: f32,
    /// The width the centre of the window was given by the last layout.
    ///
    /// The timeline, the anchor controls and the results all live in the
    /// centre, so the width it is given decides whether they can be read at
    /// all. It is kept so that a test can check the layout without a window.
    central_width: f32,
    /// Whether the settings have already been written for this close.
    closing_saved: bool,
    tx: Sender<ScopedJobEvent>,
    rx: Receiver<ScopedJobEvent>,
    /// Which voice the transport is set to: the reference or the corrected target.
    voice: Voice,
    /// The player: the reference, the corrected recording, and the switch between
    /// them.
    player: Player,
    /// Per-path cached RMS dBFS levels for loudness-matched mix preview.
    rms_cache: HashMap<PathBuf, f64>,
    /// Cached two-pass AutoGain trim (`(mix_hash_key, trim)`).
    autogain_cache: Option<(u64, f32)>,
    /// The waveform peaks of each recording, by path.
    ///
    /// The peaks are built on a thread of their own, because building them means
    /// reading the whole recording. A recording that is not in the map yet simply
    /// has no waveform yet.
    peaks: HashMap<PathBuf, Arc<PeakCache>>,
    /// The recordings whose peaks are being built.
    peaks_pending: usize,
    /// The visible span of the reference timeline, in seconds.
    view_start: f64,
    /// The visible span of the reference timeline, in seconds.
    view_end: f64,
    /// Where the playhead is, in reference seconds.
    playhead: f64,
    /// The region the user has selected on the timeline, in reference seconds.
    region: Option<(f64, f64)>,
    /// The region that is looped, in reference seconds.
    loop_region: Option<(f64, f64)>,
    /// The manual nudge of each target recording, in seconds.
    ///
    /// A nudge moves the **placement** of a recording and nothing else. The rate
    /// the sync engine measured is left alone, because a placement that is a few
    /// milliseconds out says nothing about the rate.
    nudges: Vec<f64>,
    /// The mixer: one track per recording, with its fader, pan, mute and solo.
    mixer: crate::mixer::MixerState,
    /// The nudge history, so that a nudge can be undone.
    nudge_history: Vec<(usize, f64)>,
    /// How many times the window has been asked to shrink to the monitor.
    size_requests: usize,
    /// Path of the currently open/saved `.stagesync` project, if any.
    project_path: Option<PathBuf>,
    /// Whether session state has unsaved changes since last project save/open.
    dirty: bool,
    /// Recently opened or saved `.stagesync` files.
    recent_projects: Vec<PathBuf>,
    /// User theme preference (`Dark`, `Light`, `System`).
    theme_mode: ThemeMode,
    /// Resolved palette for the current frame.
    palette: Palette,
    /// Last time crash-recovery autosave snapshot was written.
    last_autosave: std::time::Instant,
    /// Metadata of an uncleaned crash-recovery snapshot waiting on disk.
    recovery_available: Option<project::RecoveryMeta>,
    /// Action awaiting the Save / Don't Save / Cancel prompt.
    pending_confirm: Option<PendingAction>,
    /// Set once the user explicitly confirmed closing with unsaved changes.
    force_close: bool,
    /// Active panel tab in the lower workspace dock.
    dock_tab: DockTab,
    /// Whether the expandable message log drawer is open.
    show_log: bool,
    /// Whether the right Export & Inspector drawer is visible.
    show_export: bool,
}

impl Default for StageSyncApp {
    fn default() -> Self {
        Self::with_settings(AppSettings::load())
    }
}

impl StageSyncApp {
    /// An application that takes its preferences from the given settings.
    ///
    /// This is separate from [`Default`] so that a test can build the
    /// application without reading the settings of whoever is running it.
    fn with_settings(stored: AppSettings) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            reference: None,
            targets: Vec::new(),
            selection: None,
            report: None,
            running: None,
            pending_analysis_commit: None,
            verifying: false,
            verifying_job: None,
            session_gen: 1,
            next_job_id: 1,
            latest_probe_jobs: HashMap::new(),
            progress: None,
            last_error: None,
            notice: None,
            log: Vec::new(),
            export_outcome: None,
            anchors: Vec::new(),
            settings: ExportSettings {
                dir: String::new(),
                layout: Layout::SourceAligned,
                sample_rate: None,
                format: stagesync_core::audio::WavFormat::Float32,
                ..ExportSettings::default()
            },
            use_spectral_check: true,
            measure_waveform_drift: true,
            force_offset_only: false,
            window_width: stored.window_width,
            window_height: stored.window_height,
            central_width: 0.0,
            closing_saved: false,
            tx,
            rx,
            voice: Voice::Reference,
            player: Player::new(),
            rms_cache: HashMap::new(),
            autogain_cache: None,
            peaks: HashMap::new(),
            peaks_pending: 0,
            view_start: 0.0,
            view_end: 600.0,
            playhead: 0.0,
            region: None,
            loop_region: None,
            nudges: Vec::new(),
            nudge_history: Vec::new(),
            mixer: crate::mixer::MixerState::default(),
            size_requests: 0,
            project_path: None,
            dirty: false,
            recent_projects: stored
                .recent_projects
                .iter()
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .collect(),
            theme_mode: ThemeMode::from_name(&stored.theme),
            palette: Palette::dark(),
            last_autosave: std::time::Instant::now(),
            recovery_available: project::load_recovery_meta(),
            pending_confirm: None,
            force_close: false,
            dock_tab: DockTab::Mixer,
            show_log: false,
            show_export: true,
        };
        app.apply_settings(&stored);
        app
    }
}

impl StageSyncApp {
    /// A new, empty application.
    pub fn new() -> Self {
        Self::default()
    }

    /// Put recordings on the list, as if they had been dropped onto the window.
    ///
    /// The first one becomes the reference and the rest become targets, which is
    /// the rule the drop handler uses as well.
    pub fn open_files(&mut self, files: Vec<PathBuf>) {
        for path in files {
            self.add_path(path, false);
        }
    }

    /// Start the analysis, as if the Analyze button had been pressed.
    ///
    /// The analysis needs a reference and at least one target; without them there
    /// is nothing to do, and the window says so rather than failing silently.
    pub fn analyse_on_start(&mut self) {
        if self.reference.is_none() || self.targets.is_empty() {
            return;
        }
        self.start_analysis();
    }

    /// Take the stored settings into the window.
    ///
    /// The files of the last session are put back on the list when they are still
    /// there: a file that has since been moved or deleted is skipped, and the message
    /// panel says so.
    fn apply_settings(&mut self, stored: &AppSettings) {
        if !stored.export_dir.trim().is_empty() {
            self.settings.dir = stored.export_dir.clone();
        }
        self.settings.layout = settings::layout_from_name(&stored.layout);
        self.settings.sample_rate = stored.sample_rate;
        self.settings.format = settings::format_from_name(&stored.format);
        self.settings.loudness = settings::loudness_from_name(&stored.loudness);
        self.settings.container = settings::container_from_name(&stored.container);
        self.use_spectral_check = stored.use_spectral_check;
        self.measure_waveform_drift = stored.measure_waveform_drift;
        self.force_offset_only = stored.force_offset_only;
        self.window_width = stored.window_width;
        self.window_height = stored.window_height;
        self.restore_files(stored);
    }

    /// Put the files of the last session back on the list.
    fn restore_files(&mut self, stored: &AppSettings) {
        let mut wanted: Vec<PathBuf> = Vec::new();
        if !stored.last_reference.trim().is_empty() {
            wanted.push(PathBuf::from(stored.last_reference.trim()));
        }
        for source in &stored.last_sources {
            if !source.trim().is_empty() {
                wanted.push(PathBuf::from(source.trim()));
            }
        }
        let present: Vec<PathBuf> = wanted
            .iter()
            .filter(|path| path.is_file())
            .cloned()
            .collect();
        let missing = wanted.len() - present.len();
        if present.is_empty() {
            return;
        }
        self.add_paths(present, false);
        self.push_log(format!(
            "restored {} file(s) from the last session",
            self.reference.is_some() as usize + self.targets.len()
        ));
        if missing > 0 {
            self.push_log(format!(
                "{missing} file(s) of the last session are no longer there"
            ));
        }
    }

    /// Write the current choices to the settings file.
    ///
    /// The settings are written when something changes and when a job finishes, not
    /// on every frame: the file is read back at the start of the next run.
    fn persist_settings(&mut self) {
        let stored = AppSettings {
            export_dir: self.settings.dir.clone(),
            layout: settings::layout_name(self.settings.layout).to_string(),
            sample_rate: self.settings.sample_rate,
            format: settings::format_name(self.settings.format).to_string(),
            use_spectral_check: self.use_spectral_check,
            measure_waveform_drift: self.measure_waveform_drift,
            force_offset_only: self.force_offset_only,
            last_reference: self
                .reference
                .as_ref()
                .map(|entry| entry.path.display().to_string())
                .unwrap_or_default(),
            last_sources: self
                .targets
                .iter()
                .map(|entry| entry.path.display().to_string())
                .collect(),
            window_width: self.window_width,
            window_height: self.window_height,
            loudness: settings::loudness_name(self.settings.loudness).to_string(),
            container: settings::container_name(self.settings.container).to_string(),
            theme: self.theme_mode.name().to_string(),
            recent_projects: self
                .recent_projects
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        };
        if let Err(error) = stored.save() {
            log::warn!("could not write the settings: {error}");
        }
    }

    /// Allocate a fresh [`JobScope`] for the current session.
    fn next_scope(&mut self) -> JobScope {
        let job_id = self.next_job_id;
        self.next_job_id = self.next_job_id.wrapping_add(1).max(1);
        JobScope {
            session_gen: self.session_gen,
            job_id,
        }
    }

    /// Cancel any in-flight job without advancing `session_gen` (keeps valid per-path probes/peaks).
    fn cancel_active_jobs(&mut self) {
        if let Some(job) = self.running.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.pending_analysis_commit = None;
        self.progress = None;
        self.verifying = false;
        self.verifying_job = None;
    }

    /// Advance `session_gen` and cancel active jobs so stale workers from any prior
    /// project or reset workspace can never write into the new session.
    fn invalidate_session(&mut self) {
        self.session_gen = self.session_gen.wrapping_add(1).max(1);
        self.latest_probe_jobs.clear();
        self.rms_cache.clear();
        self.autogain_cache = None;
        self.cancel_active_jobs();
    }

    /// Whether `path` belongs to the active reference or target list.
    fn has_input_path(&self, path: &Path) -> bool {
        self.reference.as_ref().is_some_and(|e| e.path == path)
            || self.targets.iter().any(|e| e.path == path)
    }

    /// Collect all current recording paths for overwrite protection.
    fn input_paths(&self) -> Vec<PathBuf> {
        self.reference
            .iter()
            .chain(self.targets.iter())
            .map(|e| e.path.clone())
            .collect()
    }

    /// Spawn background probe + peak builds for `path` within the current session.
    fn spawn_probe_and_peaks(&mut self, path: PathBuf) {
        let probe_scope = self.next_scope();
        self.latest_probe_jobs
            .insert(path.clone(), probe_scope.job_id);
        jobs::spawn_probe_scoped(self.tx.clone(), probe_scope, path.clone());
        let peak_scope = self.next_scope();
        jobs::spawn_peaks_scoped(self.tx.clone(), peak_scope, path);
        self.peaks_pending += 1;
    }

    /// Whether a finished `AlignmentReport` still matches the active reference and target files.
    fn report_matches_session(&self, report: &AlignmentReport) -> bool {
        let Some(ref_entry) = &self.reference else {
            return false;
        };
        if ref_entry.path != report.reference.path || report.sources.len() != self.targets.len() {
            return false;
        }
        if let Some(meta) = &ref_entry.meta {
            if !meta.sha256.is_empty()
                && !report.reference.sha256.is_empty()
                && meta.sha256 != report.reference.sha256
            {
                return false;
            }
        }
        for (entry, alignment) in self.targets.iter().zip(report.sources.iter()) {
            if entry.path != alignment.source.path {
                return false;
            }
            if let Some(meta) = &entry.meta {
                if !meta.sha256.is_empty()
                    && !alignment.source.sha256.is_empty()
                    && meta.sha256 != alignment.source.sha256
                {
                    return false;
                }
            }
        }
        true
    }

    /// Apply everything the background workers have reported since the last frame.
    fn drain_events(&mut self) {
        while let Ok(ScopedJobEvent { scope, event }) = self.rx.try_recv() {
            if scope.session_gen != self.session_gen {
                continue;
            }
            match event {
                JobEvent::Progress(progress) => {
                    if self
                        .running
                        .as_ref()
                        .is_some_and(|job| job.job_id == scope.job_id)
                    {
                        self.progress = Some(progress);
                    }
                }
                JobEvent::Probed { path, meta, error } => {
                    let is_latest = self
                        .latest_probe_jobs
                        .get(&path)
                        .is_some_and(|id| *id == scope.job_id);
                    if is_latest && self.has_input_path(&path) {
                        self.apply_probe(path, meta, error);
                    }
                }
                JobEvent::AnalysisFinished(report) => {
                    let expected = self.running.as_ref().is_some_and(|job| {
                        job.kind == JobKind::Analysis && job.job_id == scope.job_id
                    });
                    if !expected {
                        continue;
                    }
                    self.running = None;
                    self.progress = None;
                    // The report describes the recordings that were in the list
                    // when the analysis started. If the list has changed since,
                    // its indices or paths no longer line up with the file list, and
                    // showing it would put one recording's figures next to
                    // another recording's name.
                    if !self.report_matches_session(&report) {
                        self.pending_analysis_commit = None;
                        self.report = None;
                        self.last_error = Some(
                            "The file list changed while the analysis was running, so its result \
                             was discarded. Run the analysis again."
                                .to_string(),
                        );
                        self.push_log(
                            "analysis result discarded: the file list changed".to_string(),
                        );
                        continue;
                    }
                    if let Some(commit) = self.pending_analysis_commit.take() {
                        match commit {
                            PendingAnalysisCommit::ApplyNudge {
                                target_index,
                                proposed_anchor,
                            } => {
                                if self.anchors.len() <= target_index {
                                    self.anchors
                                        .resize(target_index + 1, AnchorFields::default());
                                }
                                self.anchors[target_index] = proposed_anchor;
                                if let Some(n) = self.nudges.get_mut(target_index) {
                                    *n = 0.0;
                                }
                                self.nudge_history
                                    .retain(|(other, _)| *other != target_index);
                            }
                            PendingAnalysisCommit::ResetToAutomatic { target_index } => {
                                if let Some(anchor) = self.anchors.get_mut(target_index) {
                                    anchor.active = false;
                                }
                                if let Some(n) = self.nudges.get_mut(target_index) {
                                    *n = 0.0;
                                }
                                self.nudge_history
                                    .retain(|(other, _)| *other != target_index);
                            }
                        }
                    }
                    if self.view_start == 0.0 && (self.view_end - 600.0).abs() < 1e-6 {
                        self.view_end = report.timeline_duration.max(1.0);
                    }
                    self.report = Some(*report);
                    self.autogain_cache = None;
                    self.mark_dirty();
                    self.last_error = None;
                    self.notice = Some("Analysis finished.".to_string());
                    self.push_log("analysis finished".to_string());
                    self.persist_settings();
                }
                JobEvent::AnalysisFailed(message) => {
                    let expected = self.running.as_ref().is_some_and(|job| {
                        job.kind == JobKind::Analysis && job.job_id == scope.job_id
                    });
                    if !expected {
                        continue;
                    }
                    self.pending_analysis_commit = None;
                    self.running = None;
                    self.progress = None;
                    self.last_error = Some(format!("The analysis failed: {message}"));
                    self.push_log(format!("analysis failed: {message}"));
                }
                JobEvent::AnalysisCancelled => {
                    let expected = self.running.as_ref().is_some_and(|job| {
                        job.kind == JobKind::Analysis && job.job_id == scope.job_id
                    });
                    if !expected {
                        continue;
                    }
                    self.pending_analysis_commit = None;
                    self.running = None;
                    self.progress = None;
                    self.last_error = None;
                    self.notice = Some(
                        "The analysis was cancelled. The previous result was kept.".to_string(),
                    );
                    self.push_log("analysis cancelled".to_string());
                }
                JobEvent::ExportFinished(outcome) => {
                    let expected = self.running.as_ref().is_some_and(|job| {
                        job.kind == JobKind::Export && job.job_id == scope.job_id
                    });
                    if !expected {
                        continue;
                    }
                    self.running = None;
                    self.progress = None;
                    self.export_outcome = Some(*outcome);
                    self.last_error = None;
                    self.notice = Some("Export finished.".to_string());
                    self.push_log("export finished".to_string());
                    self.persist_settings();
                }
                JobEvent::ExportFailed(message) => {
                    let expected = self.running.as_ref().is_some_and(|job| {
                        job.kind == JobKind::Export && job.job_id == scope.job_id
                    });
                    if !expected {
                        continue;
                    }
                    self.running = None;
                    self.progress = None;
                    self.last_error = Some(format!("The export failed: {message}"));
                    self.push_log(format!("export failed: {message}"));
                }
                JobEvent::ExportCancelled => {
                    let expected = self.running.as_ref().is_some_and(|job| {
                        job.kind == JobKind::Export && job.job_id == scope.job_id
                    });
                    if !expected {
                        continue;
                    }
                    self.running = None;
                    self.progress = None;
                    self.last_error = None;
                    self.notice = Some(
                        "The export was cancelled. Files written so far were kept.".to_string(),
                    );
                    self.push_log("export cancelled".to_string());
                }
                JobEvent::VerificationFinished {
                    source_index,
                    result,
                } => {
                    if !self.verifying || self.verifying_job != Some(scope.job_id) {
                        continue;
                    }
                    self.verifying = false;
                    self.verifying_job = None;
                    if let Some(alignment) = self
                        .report
                        .as_mut()
                        .and_then(|report| report.sources.get_mut(source_index))
                    {
                        alignment.holdout = Some(*result);
                    }
                    self.notice = Some("Hold-out verification finished.".to_string());
                    self.push_log("hold-out verification finished".to_string());
                }
                JobEvent::VerificationFailed {
                    source_index,
                    message,
                } => {
                    if !self.verifying || self.verifying_job != Some(scope.job_id) {
                        continue;
                    }
                    self.verifying = false;
                    self.verifying_job = None;
                    self.last_error = Some(format!("The verification failed: {message}"));
                    self.push_log(format!(
                        "verification of source {source_index} failed: {message}"
                    ));
                }
                JobEvent::PeaksReady { path, cache } => {
                    if self.has_input_path(&path) {
                        self.peaks_pending = self.peaks_pending.saturating_sub(1);
                        self.peaks.insert(path, Arc::new(*cache));
                    }
                }
                JobEvent::PeaksFailed { path, message } => {
                    if self.has_input_path(&path) {
                        self.peaks_pending = self.peaks_pending.saturating_sub(1);
                        self.push_log(format!(
                            "the waveform of {} could not be built: {message}",
                            path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.display().to_string())
                        ));
                    }
                }
            }
        }
    }

    /// Store the result of a background probe.
    fn apply_probe(&mut self, path: PathBuf, meta: Option<SourceMeta>, error: Option<String>) {
        let mut invalidated_ref = false;
        let mut invalidated_target: Option<usize> = None;
        let check_entry = |entry: &mut FileEntry,
                           is_ref: bool,
                           view_end: &mut f64,
                           report_none: bool,
                           view_start: f64|
         -> bool {
            let mut modified = false;
            if let Some(m) = &meta {
                entry.missing = false;
                let byte_mismatch = entry.expected_byte_len.is_some_and(|exp| exp != m.byte_len);
                let hash_mismatch = entry
                    .expected_sha256
                    .as_ref()
                    .is_some_and(|exp| exp != &m.sha256);
                if byte_mismatch || hash_mismatch {
                    entry.modified_warning = Some(
                        "File contents differ from saved project — re-run analysis.".to_string(),
                    );
                    modified = true;
                } else {
                    entry.modified_warning = None;
                }
                if is_ref
                    && report_none
                    && view_start == 0.0
                    && (*view_end - 600.0).abs() < 1e-6
                    && m.duration_secs > 0.0
                {
                    *view_end = m.duration_secs.max(1.0);
                }
            } else if error.is_some() && !entry.path.is_file() {
                entry.missing = true;
            }
            entry.meta = meta.clone();
            entry.error = error.clone();
            modified
        };

        let report_none = self.report.is_none();
        let view_start = self.view_start;
        let mut view_end = self.view_end;
        if let Some(entry) = self.reference.as_mut().filter(|entry| entry.path == path) {
            invalidated_ref = check_entry(entry, true, &mut view_end, report_none, view_start);
            self.view_end = view_end;
        } else if let Some((idx, entry)) = self
            .targets
            .iter_mut()
            .enumerate()
            .find(|(_, entry)| entry.path == path)
        {
            if check_entry(entry, false, &mut view_end, report_none, view_start) {
                invalidated_target = Some(idx);
            }
        }

        if invalidated_ref {
            self.cancel_active_jobs();
            self.rms_cache.remove(&path);
            self.autogain_cache = None;
            self.report = None;
            self.export_outcome = None;
            self.anchors.fill(AnchorFields::default());
            self.nudges.fill(0.0);
            self.nudge_history.clear();
            self.notice = Some(
                "Cached alignment, manual anchors, and pending nudges were cleared because the reference changed on disk."
                    .to_string(),
            );
        } else if let Some(idx) = invalidated_target {
            self.cancel_active_jobs();
            self.rms_cache.remove(&path);
            self.autogain_cache = None;
            self.report = None;
            self.export_outcome = None;
            if let Some(anchor) = self.anchors.get_mut(idx) {
                *anchor = AnchorFields::default();
            }
            if let Some(nudge) = self.nudges.get_mut(idx) {
                *nudge = 0.0;
            }
            self.nudge_history.retain(|(other, _)| *other != idx);
            self.notice = Some(
                "Cached alignment, manual anchor, and pending nudge were cleared because a source recording changed on disk."
                    .to_string(),
            );
        }
    }

    /// Remember a message for the log panel.
    fn push_log(&mut self, message: String) {
        self.log.push(message);
        if self.log.len() > 400 {
            self.log.remove(0);
        }
    }

    /// The index of the selected target, when one is selected.
    fn selected_target_index(&self) -> Option<usize> {
        match self.selection {
            Some(Selection::Target(index)) if index < self.targets.len() => Some(index),
            _ => None,
        }
    }

    /// The manual anchor of the target at `index`, when it is in effect.
    fn anchor_value(&self, index: usize) -> Option<ManualAnchor> {
        let anchor = self.anchors.get(index)?;
        if !anchor.active {
            return None;
        }
        Some(ManualAnchor {
            source_time: anchor.source_time,
            reference_time: anchor.reference_time,
        })
    }

    /// The key that identifies a recording for the purposes of an anchor.
    ///
    /// The content hash identifies the recording exactly and survives the file
    /// being moved. The full path is used until the background probe has
    /// finished and the hash is known.
    ///
    /// A file name is deliberately *not* used: two recordings in different
    /// folders share one, and an anchor meant for one of them must not be
    /// applied to the other.
    fn anchor_key(&self, entry: &FileEntry) -> String {
        entry
            .meta
            .as_ref()
            .map(|meta| meta.sha256.clone())
            .unwrap_or_else(|| entry.path.to_string_lossy().to_string())
    }

    // --- Import ------------------------------------------------------------

    /// Add files to the list.
    ///
    /// The first file becomes the reference when there is none yet; every other file
    /// becomes a target. Changing the list invalidates the analysis, so the old report
    /// is dropped rather than left to describe files that are no longer there.
    fn import_paths(&mut self, paths: Vec<PathBuf>) {
        self.add_paths(paths, true);
    }

    /// Add files to the list, and say so when the list changed.
    ///
    /// `announce` is false while the files of the last session are put back, because
    /// the user did not do anything then.
    fn add_paths(&mut self, paths: Vec<PathBuf>, announce: bool) {
        let mut changed = false;
        for path in paths {
            if self
                .reference
                .as_ref()
                .is_some_and(|entry| entry.path == path)
            {
                continue;
            }
            if self.targets.iter().any(|entry| entry.path == path) {
                continue;
            }
            changed = true;
            if self.reference.is_none() {
                self.reference = Some(FileEntry::new(path.clone()));
                self.set_default_export_dir(&path);
            } else {
                self.targets.push(FileEntry::new(path.clone()));
                self.anchors.push(AnchorFields::default());
                self.nudges.push(0.0);
                self.mixer.sync_to(self.targets.len());
            }
            self.spawn_probe_and_peaks(path);
        }
        if changed && announce {
            self.cancel_active_jobs();
            self.report = None;
            self.export_outcome = None;
            self.last_error = None;
            self.mark_dirty();
            self.notice = Some("The file list changed — run the analysis again.".to_string());
        }
    }

    /// Add one recording to the list.
    ///
    /// `announce` is false while the files of the last session are put back, because
    /// the user did not do anything then.
    fn add_path(&mut self, path: PathBuf, announce: bool) {
        self.add_paths(vec![path], announce);
    }

    /// Point the export at a folder next to the reference, unless the user chose one.
    fn set_default_export_dir(&mut self, reference: &std::path::Path) {
        if !self.settings.dir.trim().is_empty() {
            return;
        }
        let parent = reference.parent().unwrap_or(std::path::Path::new("."));
        let folder = parent.join("stagesync_export");
        self.settings.dir = folder.display().to_string();
    }

    /// Ask the operating system for the recordings to align.
    fn browse_for_files(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Select the reference recording and the target recordings")
            .add_filter(
                "Audio",
                &[
                    "wav", "flac", "m4a", "mp3", "ogg", "oga", "aac", "opus", "wma",
                ],
            )
            .pick_files();
        if let Some(files) = picked {
            self.import_paths(files);
        }
    }

    /// Ask the operating system for the export folder.
    fn browse_for_folder(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Select the export folder")
            .pick_folder();
        if let Some(folder) = picked {
            self.settings.dir = folder.display().to_string();
        }
    }

    /// Drop the reference recording.
    fn clear_reference(&mut self) {
        self.cancel_active_jobs();
        self.reference = None;
        self.report = None;
        self.export_outcome = None;
        self.selection = None;
        self.anchors.fill(AnchorFields::default());
        self.nudges.fill(0.0);
        self.nudge_history.clear();
        self.mixer.sync_to(self.targets.len());
        self.mark_dirty();
        self.notice = Some("The reference was cleared.".to_string());
    }

    /// Drop one target recording.
    fn remove_target(&mut self, index: usize) {
        if index >= self.targets.len() {
            return;
        }
        self.cancel_active_jobs();
        let name = self.targets[index].name();
        self.targets.remove(index);
        if index < self.anchors.len() {
            self.anchors.remove(index);
        }
        if index < self.nudges.len() {
            self.nudges.remove(index);
        }
        self.nudge_history = self
            .nudge_history
            .drain(..)
            .filter_map(|(other, prev)| match other.cmp(&index) {
                std::cmp::Ordering::Less => Some((other, prev)),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some((other - 1, prev)),
            })
            .collect();
        self.mixer.remove_target(index);
        self.report = None;
        self.export_outcome = None;
        self.selection = None;
        self.mark_dirty();
        self.notice = Some(format!("{name} was removed — run the analysis again."));
    }

    /// Make one of the target recordings the reference.
    ///
    /// The first file added becomes the reference by default, which is rarely
    /// what the user meant when they dropped a folder: the first file in a
    /// directory listing is whichever one the file system happened to list
    /// first. This makes the choice explicit and deliberate.
    ///
    /// The recording that was the reference becomes a target, so nothing is
    /// lost, and the results are dropped because they describe the old
    /// reference.
    fn make_reference(&mut self, index: usize) {
        if index >= self.targets.len() {
            return;
        }
        self.cancel_active_jobs();
        let entry = self.targets.remove(index);
        // The promoted recording's own anchor goes with it. An anchor belongs to
        // the recording it was set for, and the reference is not a target: leaving
        // it behind would move the *old* reference instead.
        if index < self.anchors.len() {
            self.anchors.remove(index);
        }
        if index < self.nudges.len() {
            self.nudges.remove(index);
        }
        self.nudge_history = self
            .nudge_history
            .drain(..)
            .filter_map(|(other, prev)| match other.cmp(&index) {
                std::cmp::Ordering::Less => Some((other, prev)),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some((other - 1, prev)),
            })
            .collect();
        self.mixer.remove_target(index);
        let name = entry.name();
        let previous = self.reference.take();
        self.reference = Some(entry);
        if let Some(previous) = previous {
            // The old reference becomes a target, with no anchor of its own.
            self.targets.push(previous);
            self.anchors.push(AnchorFields::default());
            self.nudges.push(0.0);
        }
        self.mixer.sync_to(self.targets.len());
        self.report = None;
        self.export_outcome = None;
        self.selection = Some(Selection::Reference);
        self.mark_dirty();
        self.notice = Some(format!(
            "{name} is now the reference — run the analysis again."
        ));
    }

    // --- Projects, Relocate & Crash Recovery -------------------------------

    /// Whether the session has unsaved modifications.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Path of the currently open/saved `.stagesync` file, if any.
    pub fn project_path(&self) -> Option<&Path> {
        self.project_path.as_deref()
    }

    /// Whether an alignment report is currently populated.
    pub fn has_report(&self) -> bool {
        self.report.is_some()
    }

    /// Whether all imported recordings have finished building peak caches.
    pub fn has_all_peaks(&self) -> bool {
        self.peaks_pending == 0
            && self.peaks.len() >= (self.reference.is_some() as usize + self.targets.len())
    }

    /// Mark the active project as having unsaved changes.
    fn mark_dirty(&mut self) {
        if self.reference.is_some() || !self.targets.is_empty() || self.project_path.is_some() {
            self.dirty = true;
        }
    }

    /// Snapshot the current session into a serializable `.stagesync` project.
    pub fn to_project(&self, project_path: Option<&Path>) -> StageSyncProject {
        let project_dir = project_path
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty());

        let reference = self
            .reference
            .as_ref()
            .map(|entry| ProjectMediaRef::from_entry(entry, project_dir));

        let targets = self
            .targets
            .iter()
            .enumerate()
            .map(|(i, entry)| ProjectTargetEntry {
                media: ProjectMediaRef::from_entry(entry, project_dir),
                anchor: self.anchors.get(i).copied().unwrap_or_default().into(),
                pending_nudge_secs: self.nudges.get(i).copied().unwrap_or(0.0),
            })
            .collect();

        let voice = match self.voice {
            Voice::Reference => ProjectVoice::Reference,
            Voice::Target => ProjectVoice::Target,
            Voice::Mix => ProjectVoice::Mix,
        };

        StageSyncProject {
            version: project::PROJECT_FORMAT_VERSION,
            saved_at_unix_secs: project::unix_now_secs(),
            reference,
            targets,
            analysis: ProjectAnalysisOptions {
                use_spectral_check: self.use_spectral_check,
                measure_waveform_drift: self.measure_waveform_drift,
                force_offset_only: self.force_offset_only,
            },
            report: self.report.clone(),
            mixer: ProjectMixerState::from(&self.mixer),
            timeline: ProjectTimelineState {
                view_start: self.view_start,
                view_end: self.view_end,
                playhead: self.playhead,
                region: self.region,
                loop_region: self.loop_region,
                voice,
            },
            export: ProjectExportSettings::from(&self.settings),
        }
    }

    /// Save the active session atomically to `path` and update recent projects.
    pub fn save_project_to(&mut self, path: &Path) -> Result<(), anyhow::Error> {
        project::ensure_safe_project_path(path, &self.input_paths())?;
        let snapshot = self.to_project(Some(path));
        snapshot.save_to(path)?;
        self.project_path = Some(path.to_path_buf());
        self.dirty = false;
        self.record_recent_project(path);
        project::clear_recovery();
        self.recovery_available = None;
        self.notice = Some(format!("Project saved to {}", platform::display_path(path)));
        self.push_log(format!("saved project {}", platform::display_path(path)));
        Ok(())
    }

    /// Open a `.stagesync` project from disk, verifying source hashes and missing files.
    pub fn open_project_path(&mut self, path: &Path) -> Result<(), anyhow::Error> {
        let loaded = StageSyncProject::load_from(path)?;
        self.apply_loaded_project(loaded, Some(path.to_path_buf()));
        self.record_recent_project(path);
        Ok(())
    }

    fn apply_loaded_project(
        &mut self,
        loaded: project::LoadedProject,
        source_path: Option<PathBuf>,
    ) {
        self.invalidate_session();
        self.player.stop();
        self.peaks.clear();
        self.peaks_pending = 0;
        self.export_outcome = None;
        self.selection = None;
        self.nudge_history.clear();
        self.last_error = None;

        let proj = loaded.project;
        self.use_spectral_check = proj.analysis.use_spectral_check;
        self.measure_waveform_drift = proj.analysis.measure_waveform_drift;
        self.force_offset_only = proj.analysis.force_offset_only;

        let ref_modified = loaded
            .reference
            .as_ref()
            .is_some_and(|r| matches!(r.status, MediaStatus::ModifiedOnDisk { .. }));

        self.reference = loaded.reference.as_ref().map(entry_from_resolved);
        self.targets = loaded.targets.iter().map(entry_from_resolved).collect();

        // Preserve manual anchors/nudges for missing files so relinking can restore
        // the session verbatim, but clear stale manual edits when audio bytes on
        // disk actually changed (or when the reference itself changed).
        self.anchors = proj
            .targets
            .iter()
            .zip(loaded.targets.iter())
            .map(|(t, resolved)| {
                if ref_modified || matches!(resolved.status, MediaStatus::ModifiedOnDisk { .. }) {
                    AnchorFields::default()
                } else {
                    AnchorFields::from(t.anchor)
                }
            })
            .collect();
        self.nudges = proj
            .targets
            .iter()
            .zip(loaded.targets.iter())
            .map(|(t, resolved)| {
                if ref_modified || matches!(resolved.status, MediaStatus::ModifiedOnDisk { .. }) {
                    0.0
                } else {
                    t.pending_nudge_secs
                }
            })
            .collect();

        self.mixer.mode = proj.mixer.mode;
        self.mixer.master_gain = proj.mixer.master_gain;
        self.mixer.tracks = proj.mixer.tracks;
        self.mixer.sync_to(self.targets.len());

        self.settings = proj.export.to_export_settings(&self.mixer);

        self.view_start = proj.timeline.view_start;
        self.view_end = proj.timeline.view_end;
        self.playhead = proj.timeline.playhead;
        self.region = proj.timeline.region;
        self.loop_region = proj.timeline.loop_region;
        self.voice = match proj.timeline.voice {
            ProjectVoice::Reference => Voice::Reference,
            ProjectVoice::Target => Voice::Target,
            ProjectVoice::Mix => Voice::Mix,
        };

        self.report = loaded.valid_report;
        self.project_path = source_path.clone();
        self.dirty = false;

        // Kick off background metadata verification + streaming PeakCache builds for present files.
        let ref_probe_path = self
            .reference
            .as_ref()
            .filter(|r| !r.missing)
            .map(|r| r.path.clone());
        if let Some(p) = ref_probe_path {
            self.spawn_probe_and_peaks(p);
        }
        let target_probe_paths: Vec<PathBuf> = self
            .targets
            .iter()
            .filter(|t| !t.missing)
            .map(|t| t.path.clone())
            .collect();
        for p in target_probe_paths {
            self.spawn_probe_and_peaks(p);
        }

        for msg in &loaded.notices {
            self.push_log(msg.clone());
        }
        if !loaded.notices.is_empty() {
            self.last_error = Some(loaded.notices.join(" · "));
        } else if let Some(p) = &source_path {
            self.notice = Some(format!("Opened project {}", platform::display_path(p)));
        }
    }

    /// Reset to a blank project.
    pub fn new_project(&mut self) {
        self.invalidate_session();
        self.player.stop();
        self.reference = None;
        self.targets.clear();
        self.anchors.clear();
        self.nudges.clear();
        self.nudge_history.clear();
        self.mixer = crate::mixer::MixerState::default();
        self.report = None;
        self.export_outcome = None;
        self.selection = None;
        self.peaks.clear();
        self.peaks_pending = 0;
        self.view_start = 0.0;
        self.view_end = 600.0;
        self.playhead = 0.0;
        self.region = None;
        self.loop_region = None;
        self.project_path = None;
        self.dirty = false;
        self.last_error = None;
        self.notice = Some("Started a new project.".to_string());
        project::clear_recovery();
        self.recovery_available = None;
    }

    /// Relocate a missing or moved reference recording to `new_path`.
    pub fn relocate_reference(&mut self, new_path: PathBuf) {
        let Some(entry) = self.reference.as_mut() else {
            return;
        };
        let changed = relocate_entry(entry, new_path.clone());
        self.cancel_active_jobs();
        if changed {
            self.report = None;
            self.export_outcome = None;
            self.anchors.fill(AnchorFields::default());
            self.nudges.fill(0.0);
            self.nudge_history.clear();
        } else if let Some(report) = self.report.as_mut() {
            report.reference.path = new_path.clone();
        }
        self.rms_cache.remove(&new_path);
        self.autogain_cache = None;
        self.spawn_probe_and_peaks(new_path);
        self.mark_dirty();
        self.last_error = None;
        self.notice = Some(if changed {
            "Reference relocated to a different file — run Auto Sync again.".to_string()
        } else {
            "Reference relocated and digest verified.".to_string()
        });
    }

    /// Relocate a missing or moved target recording to `new_path`.
    pub fn relocate_target(&mut self, index: usize, new_path: PathBuf) {
        let Some(entry) = self.targets.get_mut(index) else {
            return;
        };
        let changed = relocate_entry(entry, new_path.clone());
        self.cancel_active_jobs();
        if changed {
            self.report = None;
            self.export_outcome = None;
            if let Some(anchor) = self.anchors.get_mut(index) {
                *anchor = AnchorFields::default();
            }
            if let Some(nudge) = self.nudges.get_mut(index) {
                *nudge = 0.0;
            }
            self.nudge_history.retain(|(other, _)| *other != index);
        } else if let Some(alignment) = self
            .report
            .as_mut()
            .and_then(|report| report.sources.get_mut(index))
        {
            alignment.source.path = new_path.clone();
        }
        self.rms_cache.remove(&new_path);
        self.autogain_cache = None;
        self.spawn_probe_and_peaks(new_path);
        self.mark_dirty();
        self.last_error = None;
        self.notice = Some(if changed {
            "Target relocated to a different file — run Auto Sync again.".to_string()
        } else {
            "Target relocated and digest verified.".to_string()
        });
    }

    fn record_recent_project(&mut self, path: &Path) {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|c| c.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        };
        self.recent_projects.retain(|existing| existing != &abs);
        self.recent_projects.insert(0, abs);
        self.recent_projects.truncate(10);
        self.persist_settings();
    }

    fn maybe_autosave(&mut self) {
        if !self.dirty || self.running.is_some() {
            return;
        }
        if self.last_autosave.elapsed().as_secs() < 20 {
            return;
        }
        self.last_autosave = std::time::Instant::now();
        let snapshot = self.to_project(self.project_path.as_deref());
        if let Err(err) =
            project::write_recovery(&snapshot, self.project_path.as_deref(), self.dirty)
        {
            log::warn!("autosave failed: {err:#}");
        }
    }

    fn request_action(&mut self, action: PendingAction) {
        if self.dirty {
            self.pending_confirm = Some(action);
        } else {
            self.execute_pending_action(action);
        }
    }

    fn execute_pending_action(&mut self, action: PendingAction) {
        match action {
            PendingAction::NewProject => self.new_project(),
            PendingAction::PickOpenProject => self.browse_open_project(),
            PendingAction::OpenProject(path) => {
                if let Err(err) = self.open_project_path(&path) {
                    self.last_error = Some(format!("Failed to open project: {err:#}"));
                }
            }
            PendingAction::CloseWindow => {
                self.force_close = true;
            }
        }
    }

    fn browse_open_project(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Open StageSync project")
            .add_filter("StageSync Project", &[PROJECT_EXTENSION])
            .pick_file();
        if let Some(path) = picked {
            if let Err(err) = self.open_project_path(&path) {
                self.last_error = Some(format!("Failed to open project: {err:#}"));
            }
        }
    }

    fn browse_save_project(&mut self, save_as: bool) -> bool {
        if !save_as {
            if let Some(path) = self.project_path.clone() {
                return match self.save_project_to(&path) {
                    Ok(()) => true,
                    Err(err) => {
                        self.last_error = Some(format!("Failed to save project: {err:#}"));
                        false
                    }
                };
            }
        }
        let default_name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("concert.stagesync");
        let picked = rfd::FileDialog::new()
            .set_title("Save StageSync project")
            .add_filter("StageSync Project", &[PROJECT_EXTENSION])
            .set_file_name(default_name)
            .save_file();
        if let Some(mut path) = picked {
            if path.extension().is_none() {
                path.set_extension(PROJECT_EXTENSION);
            }
            match self.save_project_to(&path) {
                Ok(()) => return true,
                Err(err) => {
                    self.last_error = Some(format!("Failed to save project: {err:#}"));
                }
            }
        }
        false
    }

    fn browse_relocate_file(&self, title: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title(title)
            .add_filter(
                "Audio",
                &[
                    "wav", "flac", "m4a", "mp3", "ogg", "oga", "aac", "opus", "wma",
                ],
            )
            .pick_file()
    }

    // --- Jobs --------------------------------------------------------------

    /// Start the analysis of the reference against the targets.
    fn start_analysis(&mut self) {
        if self.running.is_some() {
            // Starting a second job would orphan the first one's cancel flag,
            // so the user would no longer be able to stop it.
            self.last_error = Some(
                "A job is already running. Wait for it to finish, or cancel it first.".to_string(),
            );
            return;
        }
        let Some(reference) = self.reference.as_ref().map(|entry| entry.path.clone()) else {
            self.last_error = Some("Choose a reference recording first.".to_string());
            return;
        };
        let sources: Vec<PathBuf> = self
            .targets
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        if sources.is_empty() {
            self.last_error = Some("Choose at least one target recording first.".to_string());
            return;
        }

        let mut config = AnalysisConfig {
            compute_log_mel: self.use_spectral_check,
            measure_waveform_drift: self.measure_waveform_drift,
            force_offset_only: self.force_offset_only,
            ..AnalysisConfig::default()
        };
        // Each anchor belongs to the recording it was set for, so one recording
        // can be pinned without dragging the others onto the same point.
        //
        // The anchor is keyed by the recording's identity — its content hash
        // when the probe has finished, its full path otherwise — and never by
        // its file name: two recordings called `song.wav` in different folders
        // are two different recordings.
        config.manual_anchors = self
            .targets
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let anchor = match self.pending_analysis_commit {
                    Some(PendingAnalysisCommit::ApplyNudge {
                        target_index,
                        proposed_anchor,
                    }) if target_index == index => ManualAnchor {
                        source_time: proposed_anchor.source_time,
                        reference_time: proposed_anchor.reference_time,
                    },
                    Some(PendingAnalysisCommit::ResetToAutomatic { target_index })
                        if target_index == index =>
                    {
                        return None;
                    }
                    _ => self.anchor_value(index)?,
                };
                Some((self.anchor_key(entry), anchor))
            })
            .collect();
        if !config.manual_anchors.is_empty() {
            // The automatic lock is not to be trusted, so widen the fine search.
            config.fine.search_secs = 10.0;
        }

        let scope = self.next_scope();
        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(RunningJob {
            kind: JobKind::Analysis,
            job_id: scope.job_id,
            cancel: cancel.clone(),
        });
        self.progress = None;
        self.last_error = None;
        self.notice = None;
        self.push_log("analysis started".to_string());
        jobs::spawn_analysis_scoped(self.tx.clone(), scope, reference, sources, config, cancel);
    }

    /// Whether target `index` has a manual anchor toggle or time edit not yet applied by `Apply anchor`.
    fn has_unapplied_anchor(&self, index: usize) -> bool {
        let Some(anchor) = self.anchors.get(index) else {
            return false;
        };
        let Some(alignment) = self.report.as_ref().and_then(|r| r.sources.get(index)) else {
            return anchor.active;
        };
        match (anchor.active, alignment.override_state) {
            (false, None) => false,
            (true, Some(ov)) => {
                (ov.anchor_source_time - anchor.source_time).abs() > 1e-5
                    || (ov.anchor_reference_time - anchor.reference_time).abs() > 1e-5
            }
            _ => true,
        }
    }

    /// Name of the first target with an un-applied manual anchor edit, if any.
    fn first_unapplied_anchor_target(&self) -> Option<String> {
        (0..self.targets.len())
            .find(|&i| self.has_unapplied_anchor(i))
            .map(|i| self.targets[i].name())
    }

    /// Whether any target has a non-zero un-applied manual nudge.
    fn has_pending_nudges(&self) -> bool {
        self.nudges.iter().any(|n| n.abs() > 1e-9)
    }

    /// Build a snapshot of `report` with every pending manual nudge baked into its
    /// linear model, `TimeMap`, shifted segments, merged timeline, and `OverrideState`,
    /// so exports match the live preview sample-for-sample.
    fn effective_report_for_export(&self, report: &AlignmentReport) -> AlignmentReport {
        let mut out = report.clone();
        let mut any_nudged = false;
        for (i, alignment) in out.sources.iter_mut().enumerate() {
            let nudge = self.nudges.get(i).copied().unwrap_or(0.0);
            if nudge.abs() <= 1e-12 {
                continue;
            }
            let Some(old_model) = alignment.model() else {
                continue;
            };
            any_nudged = true;
            let new_offset = old_model.offset + nudge;
            let new_model = stagesync_core::matching::LinearModel::new(old_model.scale, new_offset);
            if let Some(d) = alignment.drift.as_mut() {
                d.model = new_model;
            }
            alignment.calibrated_model = Some(new_model);
            if alignment.direct_model.is_some() && alignment.drift.is_none() {
                alignment.direct_model = Some(new_model);
            }
            for seg in &mut alignment.map.segments {
                seg.reference_start += nudge;
            }
            for seg in &mut alignment.segments {
                if !matches!(
                    seg.kind,
                    SegmentKind::UncoveredReference | SegmentKind::BeyondReference
                ) {
                    seg.reference_start += nudge;
                    seg.reference_end += nudge;
                }
            }
            for pt in &mut alignment.points {
                pt.residual_ms =
                    (pt.reference_time - new_model.reference_time(pt.source_time)) * 1000.0;
            }
            let auto_offset = alignment
                .override_state
                .map(|o| o.automatic_offset)
                .unwrap_or(old_model.offset);
            alignment.override_state = Some(stagesync_core::matching::OverrideState {
                anchor_source_time: 0.0,
                anchor_reference_time: new_offset,
                automatic_offset: auto_offset,
                applied_offset: new_offset,
                shift_ms: (new_offset - auto_offset) * 1000.0,
            });
            alignment.holdout = None;
            alignment.warnings.push(format!(
                "Exported with an un-applied manual nudge of {:+.1} ms; alignment was not re-measured at this offset.",
                nudge * 1000.0
            ));
        }
        if any_nudged {
            let per_source: Vec<Vec<stagesync_core::matching::Segment>> =
                out.sources.iter().map(|s| s.segments.clone()).collect();
            out.timeline = stagesync_core::matching::segments::merge_timeline(
                &per_source,
                out.timeline_duration,
            );
        }
        out
    }

    /// Start the export of the corrected audio and the artefacts.
    fn start_export(&mut self) {
        if self.running.is_some() {
            self.last_error = Some(
                "A job is already running. Wait for it to finish, or cancel it first.".to_string(),
            );
            return;
        }
        let Some(base_report) = self.report.clone() else {
            self.last_error = Some("Run the analysis before exporting.".to_string());
            return;
        };
        if let Some(target_name) = self.first_unapplied_anchor_target() {
            self.last_error = Some(format!(
                "Manual anchor for {target_name} was edited but not applied. Click \"Apply anchor\" before exporting."
            ));
            return;
        }
        if self.settings.dir.trim().is_empty() {
            self.last_error = Some("Choose an export folder first.".to_string());
            return;
        }
        let report = self.effective_report_for_export(&base_report);
        let scope = self.next_scope();
        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(RunningJob {
            kind: JobKind::Export,
            job_id: scope.job_id,
            cancel: cancel.clone(),
        });
        self.progress = None;
        self.last_error = None;
        self.notice = None;
        self.push_log(format!("export started into {}", self.settings.dir));
        // The mixer's settings travel with the export, so that the file the window
        // writes is the mix the user can hear.
        self.settings.mix = self.mixer.settings();
        jobs::spawn_export_scoped(
            self.tx.clone(),
            scope,
            report,
            self.settings.clone(),
            cancel,
        );
    }

    /// Verify a rendered correction against the reference, independently of the fit.
    fn start_verification(&mut self, index: usize) {
        let model_and_duration = self.report.as_ref().and_then(|report| {
            let alignment = report.sources.get(index)?;
            let model = alignment.model()?;
            let nudge = self.nudges.get(index).copied().unwrap_or(0.0);
            Some((
                stagesync_core::matching::LinearModel::new(model.scale, model.offset + nudge),
                alignment.source.duration_secs,
            ))
        });
        let Some((model, duration)) = model_and_duration else {
            self.last_error =
                Some("This source has no alignment, so there is nothing to verify.".to_string());
            return;
        };
        let corrected = self.export_outcome.as_ref().and_then(|outcome| {
            outcome
                .corrected
                .iter()
                .find(|(source, _)| *source == index)
                .map(|(_, path)| path.clone())
        });
        let Some(corrected) = corrected else {
            self.last_error = Some(
                "Export the corrected audio first: the hold-out check compares the rendered file."
                    .to_string(),
            );
            return;
        };
        let Some(reference) = self.reference.as_ref().map(|entry| entry.path.clone()) else {
            return;
        };
        if !is_wav(&reference) {
            self.last_error = Some(
                "The hold-out check reads the reference as a WAV file, which this one is not."
                    .to_string(),
            );
            return;
        }

        let scope = self.next_scope();
        self.verifying = true;
        self.verifying_job = Some(scope.job_id);
        self.last_error = None;
        self.push_log(format!("verifying source {index}"));
        jobs::spawn_verification_scoped(
            self.tx.clone(),
            scope,
            index,
            corrected,
            reference,
            model,
            duration,
        );
    }

    // --- Panels ------------------------------------------------------------

    /// Switch theme mode cleanly (`Dark`, `Light`, `System`) and persist preference.
    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
        self.palette = match mode {
            ThemeMode::Light => Palette::light(),
            _ => Palette::dark(),
        };
        self.persist_settings();
    }

    /// Select a target recording by index for inspection / screenshots / shortcuts.
    pub fn select_target(&mut self, index: usize) {
        if index < self.targets.len() {
            self.selection = Some(Selection::Target(index));
        }
    }

    /// Drain pending background jobs (`Probed`, `PeaksReady`, `AnalysisFinished`, `ExportFinished`).
    pub fn poll_background(&mut self) {
        self.drain_events();
    }

    /// Compact dual-row studio header: workflow & project controls on top, transport & zoom below.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let busy = self.running.is_some() || self.verifying;
        let can_analyze = self.reference.is_some() && !self.targets.is_empty() && !busy;
        let mut start = false;
        let mut cancel = false;
        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Project  (Ctrl+N)").clicked() {
                    ui.close();
                    self.request_action(PendingAction::NewProject);
                }
                if ui.button("Open Project…  (Ctrl+O)").clicked() {
                    ui.close();
                    self.request_action(PendingAction::PickOpenProject);
                }
                ui.menu_button("Recent Projects", |ui| {
                    if self.recent_projects.is_empty() {
                        ui.label("No recent projects");
                    } else {
                        for recent in self.recent_projects.clone() {
                            let label = recent
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| platform::display_path(&recent));
                            if ui
                                .button(label)
                                .on_hover_text(platform::display_path(&recent))
                                .clicked()
                            {
                                ui.close();
                                self.request_action(PendingAction::OpenProject(recent));
                            }
                        }
                    }
                });
                ui.separator();
                if ui.button("Save Project  (Ctrl+S)").clicked() {
                    ui.close();
                    self.browse_save_project(false);
                }
                if ui.button("Save Project As…  (Ctrl+Shift+S)").clicked() {
                    ui.close();
                    self.browse_save_project(true);
                }
            });

            if ui
                .button("Add files…")
                .on_hover_text("Import reference and target audio files")
                .clicked()
            {
                self.browse_for_files();
            }

            let proj_label = match &self.project_path {
                Some(p) => p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled".to_string()),
                None => "Untitled".to_string(),
            };
            ui.colored_label(
                if self.dirty {
                    self.palette.accent
                } else {
                    self.palette.text_muted
                },
                if self.dirty {
                    format!("{proj_label}*")
                } else {
                    proj_label
                },
            );

            ui.separator();

            let analyze_btn = egui::Button::new(egui::RichText::new("Analyze").strong().color(
                if can_analyze {
                    Color32::WHITE
                } else {
                    self.palette.text_muted
                },
            ))
            .fill(if can_analyze {
                self.palette.accent
            } else {
                self.palette.bg_surface
            });
            start = ui
                .add_enabled(can_analyze, analyze_btn)
                .on_hover_text("Run coarse + fine sync & clock drift measurement")
                .clicked();

            if self.running.is_some() {
                let tip = match self.running.as_ref().map(|job| job.kind) {
                    Some(JobKind::Export) => {
                        "Cancel remaining exports (the file currently writing cannot stop mid-file)"
                    }
                    _ => "Stop the running analysis",
                };
                cancel = ui.button("Cancel").on_hover_text(tip).clicked();
            } else if self.verifying {
                ui.colored_label(self.palette.accent, "Verifying…");
            }

            ui.menu_button("Options", |ui| {
                changed |= ui
                    .checkbox(&mut self.use_spectral_check, "Spectral cross-check")
                    .on_hover_text(
                        "Run independent log-mel cross-check for confidence corroboration.",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.measure_waveform_drift, "Waveform drift check")
                    .on_hover_text("Measure raw waveform GCC-PHAT drift at hold-out positions.")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.force_offset_only, "Offset only (no resampling)")
                    .on_hover_text(
                        "Align constant offset only and keep source playback rate unchanged.",
                    )
                    .changed();
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(self.show_export, "Export panel")
                    .on_hover_text("Toggle Export inspector drawer")
                    .clicked()
                {
                    self.show_export = !self.show_export;
                }
                if ui
                    .selectable_label(self.show_log, format!("Log ({})", self.log.len()))
                    .on_hover_text("Toggle message log drawer")
                    .clicked()
                {
                    self.show_log = !self.show_log;
                }
                let theme_icon = if self.palette.dark {
                    crate::theme::Icon::Moon
                } else {
                    crate::theme::Icon::Sun
                };
                if crate::theme::small_icon_button(
                    ui,
                    theme_icon,
                    "Toggle theme",
                    &format!(
                        "Theme: {} (click to cycle Dark / Light / System)",
                        self.theme_mode.label()
                    ),
                )
                .clicked()
                {
                    self.theme_mode = match self.theme_mode {
                        ThemeMode::Dark => ThemeMode::Light,
                        ThemeMode::Light => ThemeMode::System,
                        ThemeMode::System => ThemeMode::Dark,
                    };
                    self.persist_settings();
                }
                ui.colored_label(self.palette.text_muted, self.theme_mode.label());
            });
        });

        ui.separator();

        // Row 2: Studio Transport, A/B Switch, Timecode, and Timeline Zoom/Fit
        let duration = self
            .report
            .as_ref()
            .map(|report| report.timeline_duration.max(1.0))
            .or_else(|| {
                self.reference
                    .as_ref()
                    .and_then(|r| r.meta.as_ref())
                    .map(|m| m.duration_secs.max(1.0))
            })
            .unwrap_or(600.0);

        let playhead_anchor = self.playhead;
        let muted_color = self.palette.text_muted;
        let draw_view_controls =
            |ui: &mut egui::Ui, view_start: &mut f64, view_end: &mut f64| -> Option<f64> {
                let mut seek_to = None;
                if ui
                    .button("Fit")
                    .on_hover_text("Show whole timeline (F)")
                    .clicked()
                {
                    *view_start = 0.0;
                    *view_end = duration;
                }
                if ui.button("+").on_hover_text("Zoom in (+)").clicked() {
                    let span = (*view_end - *view_start).max(1e-3);
                    let new_span = (span * 0.5).clamp(0.05, 24.0 * 3600.0);
                    let ratio = ((playhead_anchor - *view_start) / span).clamp(-4.0, 4.0);
                    *view_start = playhead_anchor - ratio * new_span;
                    *view_end = *view_start + new_span;
                }
                if ui.button("-").on_hover_text("Zoom out (-)").clicked() {
                    let span = (*view_end - *view_start).max(1e-3);
                    let new_span = (span * 2.0).clamp(0.05, 24.0 * 3600.0);
                    let ratio = ((playhead_anchor - *view_start) / span).clamp(-4.0, 4.0);
                    *view_start = playhead_anchor - ratio * new_span;
                    *view_end = *view_start + new_span;
                }
                if ui
                    .button("Start")
                    .on_hover_text("Jump to 0:00:00 (Home)")
                    .clicked()
                {
                    seek_to = Some(0.0);
                    *view_start = 0.0;
                }
                ui.colored_label(
                    muted_color,
                    format!(
                        "{:.1}s / {:.1}s",
                        (*view_end - *view_start).max(0.0),
                        duration
                    ),
                );
                seek_to
            };

        let compact_transport = ui.available_width() < 1180.0;
        let mut requested_seek: Option<f64> = None;

        ui.horizontal_wrapped(|ui| {
            let playing = self.player.is_playing();
            let play_btn =
                egui::Button::new(if playing { "Stop" } else { "Play" }).fill(if playing {
                    self.palette.playhead.gamma_multiply(0.22)
                } else {
                    self.palette.bg_surface
                });
            if ui
                .add(play_btn)
                .on_hover_text("Play / Stop at playhead (Space)")
                .clicked()
            {
                if playing {
                    self.player.stop();
                } else {
                    self.play_voice(self.voice);
                }
            }

            ui.separator();
            for (v, label, tip) in [
                (Voice::Reference, "Reference", "Play original reference (1)"),
                (
                    Voice::Target,
                    "Corrected target",
                    "Play selected drift-corrected target (2)",
                ),
                (Voice::Mix, "Mix", "Play multi-track mix (3)"),
            ] {
                if ui
                    .selectable_label(self.voice == v, label)
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.play_voice(v);
                }
            }

            ui.separator();
            if ui
                .add_enabled(self.region.is_some(), egui::Button::new("Loop selection"))
                .on_hover_text("Loop selected span (L)")
                .clicked()
            {
                self.loop_region = self.region;
                self.mark_dirty();
            }
            if ui
                .add_enabled(self.loop_region.is_some(), egui::Button::new("Clear loop"))
                .clicked()
            {
                self.loop_region = None;
                self.mark_dirty();
            }

            let mut vol = (self.player.volume() * 100.0).round();
            if ui
                .add(
                    egui::Slider::new(&mut vol, 0.0..=100.0)
                        .suffix("%")
                        .show_value(false)
                        .text("Vol"),
                )
                .on_hover_text(format!("Preview monitor level: {vol:.0}%"))
                .changed()
            {
                self.player.set_volume(vol / 100.0);
            }

            ui.separator();
            ui.monospace(format_timecode(self.playhead));

            if !compact_transport {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(s) =
                        draw_view_controls(ui, &mut self.view_start, &mut self.view_end)
                    {
                        requested_seek = Some(s);
                    }
                });
            }
        });

        if compact_transport {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(s) =
                        draw_view_controls(ui, &mut self.view_start, &mut self.view_end)
                    {
                        requested_seek = Some(s);
                    }
                });
            });
        }
        if let Some(pos) = requested_seek {
            self.seek_playhead(pos);
        }

        if changed {
            self.autogain_cache = None;
            self.mark_dirty();
            self.persist_settings();
            if self.player.is_playing() && self.player.voice() == Some(crate::player::Voice::Mix) {
                let (live_members, autogain_trim) = self.resolve_preview_mix(false);
                self.player.update_live_mix(
                    self.mixer.master_gain,
                    self.settings.protection,
                    autogain_trim,
                    &live_members,
                );
            }
        }
        if start {
            self.start_analysis();
        }
        if cancel {
            if let Some(job) = &self.running {
                job.cancel.store(true, Ordering::Relaxed);
                self.push_log("cancel requested".to_string());
            }
        }
    }

    /// The file list (vertically scrollable sidebar).
    fn file_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Sources");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.reference.is_some() && self.running.is_none(),
                        egui::Button::new("Clear ref"),
                    )
                    .clicked()
                {
                    self.clear_reference();
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("sources_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.colored_label(self.palette.text_muted, "REFERENCE");
                if let Some(entry) = self.reference.clone() {
                    let selected = self.selection == Some(Selection::Reference);
                    egui::Frame::new()
                        .fill(if selected {
                            self.palette.bg_lane_selected
                        } else {
                            self.palette.bg_surface
                        })
                        .stroke(eframe::egui::Stroke::new(
                            1.0,
                            if selected {
                                self.palette.accent
                            } else {
                                self.palette.border_subtle
                            },
                        ))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            if ui.selectable_label(selected, entry.name()).clicked() {
                                self.selection = Some(Selection::Reference);
                            }
                            ui.colored_label(
                                self.palette.text_faint,
                                platform::elide_path(&entry.path, PATH_CHARS),
                            );
                            match &entry.meta {
                                Some(meta) => {
                                    ui.label(format!(
                                        "{} Hz · {} ch · {} · {}",
                                        meta.sample_rate,
                                        meta.channels,
                                        format_duration(meta.duration_secs),
                                        meta.codec
                                    ));
                                }
                                None => {
                                    if entry.error.is_none() && !entry.missing {
                                        ui.colored_label(self.palette.text_muted, "reading…");
                                    }
                                }
                            }
                            if let Some(error) = &entry.error {
                                ui.colored_label(self.palette.error, format!("cannot read: {error}"));
                            }
                            if entry.missing {
                                ui.horizontal(|ui| {
                                    ui.colored_label(self.palette.error, "Missing file");
                                    if ui.button("Relocate…").clicked() {
                                        if let Some(new_path) = self
                                            .browse_relocate_file("Locate missing reference recording")
                                        {
                                            self.relocate_reference(new_path);
                                        }
                                    }
                                });
                            } else if let Some(warn) = &entry.modified_warning {
                                ui.colored_label(self.palette.warn, warn.as_str());
                            }
                        });
                } else {
                    wrapped(
                        ui,
                        "Drop audio files or a folder anywhere, or click Add files… (first becomes Reference).",
                    );
                }

                ui.add_space(6.0);
                ui.colored_label(
                    self.palette.text_muted,
                    format!("TARGETS ({})", self.targets.len()),
                );
                if self.targets.is_empty() {
                    ui.colored_label(
                        self.palette.text_muted,
                        "None yet — drop or add recordings to align.",
                    );
                }

                let mut select: Option<Selection> = None;
                let mut remove: Option<usize> = None;
                let mut promote: Option<usize> = None;
                for index in 0..self.targets.len() {
                    let entry = self.targets[index].clone();
                    let metrics = self
                        .report
                        .as_ref()
                        .map(|report| metrics_for(report, index));
                    let selected = self.selection == Some(Selection::Target(index));
                    egui::Frame::new()
                        .fill(if selected {
                            self.palette.bg_lane_selected
                        } else {
                            self.palette.bg_surface
                        })
                        .stroke(eframe::egui::Stroke::new(
                            1.0,
                            if selected {
                                self.palette.accent
                            } else {
                                self.palette.border_subtle
                            },
                        ))
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            if ui
                                .selectable_label(
                                    selected,
                                    egui::RichText::new(entry.name()).strong(),
                                )
                                .clicked()
                            {
                                select = Some(Selection::Target(index));
                            }
                            ui.colored_label(
                                self.palette.text_faint,
                                platform::elide_path(&entry.path, PATH_CHARS),
                            );
                            match &entry.meta {
                                Some(meta) => {
                                    ui.label(format!(
                                        "{} Hz · {} ch · {} · {}",
                                        meta.sample_rate,
                                        meta.channels,
                                        format_duration(meta.duration_secs),
                                        meta.codec
                                    ));
                                }
                                None => {
                                    if entry.error.is_none() && !entry.missing {
                                        ui.colored_label(self.palette.text_muted, "reading…");
                                    }
                                }
                            }
                            if let Some(error) = &entry.error {
                                ui.colored_label(self.palette.error, format!("cannot read: {error}"));
                            }
                            if entry.missing {
                                ui.horizontal(|ui| {
                                    ui.colored_label(self.palette.error, "Missing file");
                                    if ui
                                        .push_id(("relocate_target", index), |ui| {
                                            ui.button("Relocate…")
                                        })
                                        .inner
                                        .clicked()
                                    {
                                        if let Some(new_path) = self.browse_relocate_file(&format!(
                                            "Locate missing recording {}",
                                            entry.name()
                                        )) {
                                            self.relocate_target(index, new_path);
                                        }
                                    }
                                });
                            } else if let Some(warn) = &entry.modified_warning {
                                ui.colored_label(self.palette.warn, warn.as_str());
                            }
                            if let Some(metrics) = metrics {
                                wrapped(ui, metrics_line(&metrics));
                            }
                            let pending_nudge = self.nudges.get(index).copied().unwrap_or(0.0);
                            if pending_nudge.abs() > 1e-9 {
                                ui.colored_label(
                                    self.palette.warn,
                                    format!(
                                        "Pending nudge {:+.1} ms (export applies; click Apply to re-check)",
                                        pending_nudge * 1000.0
                                    ),
                                );
                            }
                            if self.has_unapplied_anchor(index) {
                                ui.colored_label(
                                    self.palette.error,
                                    "Unapplied manual anchor — click Apply anchor",
                                );
                            }
                            let alignment = self.report.as_ref().and_then(|r| r.sources.get(index));
                            let status = status_for(self.report.as_ref(), index);
                            let verification = verification_for(self.report.as_ref(), index);
                            if alignment.is_none() {
                                ui.colored_label(self.palette.text_muted, "Not analysed");
                            } else if alignment.and_then(|a| a.model()).is_none() {
                                ui.colored_label(self.palette.warn, "No reliable match");
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.colored_label(self.palette.text_muted, status.label());
                                    if self.export_outcome.is_some()
                                        || alignment.and_then(|a| a.holdout.as_ref()).is_some()
                                    {
                                        ui.separator();
                                        ui.colored_label(
                                            self.palette.text_muted,
                                            verification.label(),
                                        );
                                    }
                                });
                            }
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_enabled(
                                                self.running.is_none(),
                                                egui::Button::new("×"),
                                            )
                                            .on_hover_text("Remove recording from list")
                                            .clicked()
                                        {
                                            remove = Some(index);
                                        }
                                        if ui
                                            .add_enabled(
                                                self.running.is_none(),
                                                egui::Button::new("Make reference"),
                                            )
                                            .on_hover_text("Use this recording as the reference")
                                            .clicked()
                                        {
                                            promote = Some(index);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }

                if let Some(selection) = select {
                    self.selection = Some(selection);
                }
                if let Some(index) = remove {
                    self.remove_target(index);
                }
                if let Some(index) = promote {
                    self.make_reference(index);
                }
            });
    }

    /// The export controls and the list of files that were written (vertically scrollable).
    fn export_panel(&mut self, ui: &mut egui::Ui) {
        ui.strong("Export");
        ui.separator();
        let mut browse = false;
        let mut changed = false;

        egui::ScrollArea::vertical()
            .id_salt("export_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Folder");
                    let width =
                        (ui.available_width() - ui.spacing().item_spacing.x - 70.0).max(80.0);
                    changed |= ui
                        .add_sized(
                            [width, ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(&mut self.settings.dir)
                                .desired_width(f32::INFINITY)
                                .hint_text("choose a folder"),
                        )
                        .changed();
                    if ui.button("Browse…").clicked() {
                        browse = true;
                    }
                });
                if browse {
                    self.browse_for_folder();
                    changed = true;
                }

                egui::ComboBox::from_label("Layout")
                    .selected_text(layout_label(self.settings.layout))
                    .show_ui(ui, |ui| {
                        for (label, layout) in [
                            ("Source aligned", Layout::SourceAligned),
                            (
                                "Reference timeline, per source",
                                Layout::ReferenceTimelinePerSource,
                            ),
                            (
                                "Reference timeline, merged",
                                Layout::ReferenceTimelineMerged,
                            ),
                            ("Mixdown", Layout::Mixdown),
                            ("Synchronised stems", Layout::MixStems),
                        ] {
                            changed |= ui
                                .selectable_value(&mut self.settings.layout, layout, label)
                                .changed();
                        }
                    });
                wrapped(
                    ui,
                    match self.settings.layout {
                        Layout::SourceAligned => {
                            "One corrected file per source, keeping its own length and sample rate."
                        }
                        Layout::ReferenceTimelinePerSource => {
                            "One reference-length file per source; gaps written as silence."
                        }
                        Layout::ReferenceTimelineMerged => {
                            "One reference-length file combining all sources in list order."
                        }
                        Layout::Mixdown => "One stereo mixdown rendered from the Mixer settings.",
                        Layout::MixStems => {
                            "One reference-length stem per enabled mixer recording."
                        }
                    },
                );

                let mut override_rate = self.settings.sample_rate.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut override_rate, "Output sample rate")
                        .changed()
                    {
                        self.settings.sample_rate = if override_rate { Some(48_000) } else { None };
                        changed = true;
                    }
                    if let Some(rate) = self.settings.sample_rate.as_mut() {
                        changed |= ui
                            .add(
                                egui::DragValue::new(rate)
                                    .speed(100.0)
                                    .range(8_000..=192_000),
                            )
                            .changed();
                        ui.label("Hz");
                    }
                });

                egui::ComboBox::from_label("Format")
                    .selected_text(format_label(self.settings.format))
                    .show_ui(ui, |ui| {
                        for format in [
                            stagesync_core::audio::WavFormat::Float32,
                            stagesync_core::audio::WavFormat::Int16,
                        ] {
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.format,
                                    format,
                                    format_label(format),
                                )
                                .changed();
                        }
                    });

                if self.settings.layout.is_mix() {
                    egui::ComboBox::from_label("Clipping")
                        .selected_text(protection_label(self.settings.protection))
                        .show_ui(ui, |ui| {
                            for protection in [
                                stagesync_core::mixer::ClippingProtection::None,
                                stagesync_core::mixer::ClippingProtection::SoftClip {
                                    threshold: 0.9,
                                },
                                stagesync_core::mixer::ClippingProtection::AutoGain {
                                    target_dbfs: -1.0,
                                },
                            ] {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.protection,
                                        protection,
                                        protection_label(protection),
                                    )
                                    .changed();
                            }
                        });
                    wrapped(
                        ui,
                        match self.settings.protection {
                            stagesync_core::mixer::ClippingProtection::None => {
                                "32-bit float keeps peaks above 0 dBFS unclipped."
                            }
                            stagesync_core::mixer::ClippingProtection::SoftClip { .. } => {
                                "Smoothly compresses samples above threshold."
                            }
                            stagesync_core::mixer::ClippingProtection::AutoGain { .. } => {
                                "Two-pass master trim landing peak on -1.0 dBFS."
                            }
                        },
                    );

                    egui::ComboBox::from_label("Loudness")
                        .selected_text(loudness_label(self.settings.loudness))
                        .show_ui(ui, |ui| {
                            for loudness in [
                                stagesync_core::mixer::LoudnessMatch::None,
                                stagesync_core::mixer::LoudnessMatch::MatchToLoudest,
                            ] {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.loudness,
                                        loudness,
                                        loudness_label(loudness),
                                    )
                                    .changed();
                            }
                        });

                    egui::ComboBox::from_label("Container")
                        .selected_text(container_label(self.settings.container))
                        .show_ui(ui, |ui| {
                            for container in [
                                stagesync_core::mixer::ContainerChoice::Auto,
                                stagesync_core::mixer::ContainerChoice::Riff,
                                stagesync_core::mixer::ContainerChoice::Rf64,
                                stagesync_core::mixer::ContainerChoice::Bw64,
                            ] {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.container,
                                        container,
                                        container_label(container),
                                    )
                                    .changed();
                            }
                        });
                }

                ui.add_space(6.0);
                if let Some(target_name) = self.first_unapplied_anchor_target() {
                    ui.colored_label(
                        self.palette.error,
                        format!(
                            "Unapplied manual anchor on {target_name}: click \"Apply anchor\" before exporting."
                        ),
                    );
                } else if self.has_pending_nudges() {
                    ui.colored_label(
                        self.palette.warn,
                        "Pending manual nudge(s) will be baked into exported audio and plan.json.",
                    );
                }
                let can_export = self.report.is_some()
                    && !self.settings.dir.trim().is_empty()
                    && self.running.is_none()
                    && !self.verifying
                    && self.first_unapplied_anchor_target().is_none();
                let exp_btn = egui::Button::new(egui::RichText::new("Export").strong().color(
                    if can_export {
                        Color32::WHITE
                    } else {
                        self.palette.text_muted
                    },
                ))
                .fill(if can_export {
                    self.palette.accent
                } else {
                    self.palette.bg_surface
                });
                if ui.add_enabled(can_export, exp_btn).clicked() {
                    self.start_export();
                }

                if let Some(outcome) = &self.export_outcome {
                    ui.separator();
                    ui.strong(format!("Files written ({})", outcome.files.len()));
                    for path in &outcome.files {
                        ui.colored_label(
                            self.palette.text_muted,
                            platform::elide_path(path, PATH_CHARS),
                        );
                    }
                }
            });

        if changed {
            self.autogain_cache = None;
            self.mark_dirty();
            self.persist_settings();
            if self.player.is_playing() && self.player.voice() == Some(crate::player::Voice::Mix) {
                let (live_members, autogain_trim) = self.resolve_preview_mix(false);
                self.player.update_live_mix(
                    self.mixer.master_gain,
                    self.settings.protection,
                    autogain_trim,
                    &live_members,
                );
            }
        }
    }

    /// Compact studio status bar + expandable message log drawer.
    fn log_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.progress {
                Some(progress) => {
                    ui.colored_label(self.palette.accent, progress.label.clone());
                    ui.add(
                        egui::ProgressBar::new(progress.fraction.clamp(0.0, 1.0) as f32)
                            .desired_width(160.0),
                    );
                    ui.colored_label(self.palette.text_muted, progress.message.clone());
                }
                None => {
                    if self.verifying {
                        ui.colored_label(self.palette.accent, "Verifying rendered correction…");
                    } else if self.peaks_pending > 0 {
                        ui.colored_label(
                            self.palette.text_muted,
                            format!("Building waveforms ({} remaining)…", self.peaks_pending),
                        );
                    } else if let Some(notice) = &self.notice {
                        ui.label(notice.as_str());
                    } else {
                        let count = self.reference.is_some() as usize + self.targets.len();
                        ui.colored_label(
                            self.palette.text_muted,
                            format!(
                                "{count} recording(s), {} waveform(s) cached",
                                self.peaks.len()
                            ),
                        );
                    }
                }
            }
            if let Some(report) = self.report.as_ref() {
                let low = report
                    .timeline
                    .iter()
                    .filter(|s| s.kind == SegmentKind::LowConfidence)
                    .count();
                let gaps = report
                    .timeline
                    .iter()
                    .filter(|s| s.kind == SegmentKind::UncoveredReference)
                    .count();
                ui.separator();
                ui.colored_label(
                    self.palette.text_muted,
                    format!(
                        "{:.1} s · {gaps} gap(s), {low} low confidence",
                        report.timeline_duration
                    ),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(self.palette.text_faint, platform::describe_platform());
            });
        });

        if self.show_log {
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("log")
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.colored_label(self.palette.text_muted, line.as_str());
                    }
                });
        }
    }

    /// The main workspace: alerts, front-and-center stereo waveform timeline, and tabbed lower dock.
    fn central(&mut self, ui: &mut egui::Ui) {
        self.central_width = ui.available_width();
        if let Some(meta) = self.recovery_available.clone() {
            let warn = self.palette.warn;
            egui::Frame::new()
                .fill(warn.gamma_multiply(0.14))
                .stroke(eframe::egui::Stroke::new(1.0, warn))
                .corner_radius(3.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let label = meta
                            .active_project_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| {
                                format!("Recover unsaved session from {}?", n.to_string_lossy())
                            })
                            .unwrap_or_else(|| {
                                "An unsaved autosave recovery snapshot was found.".to_string()
                            });
                        ui.colored_label(warn, label);
                        if ui.button("Recover Session").clicked() {
                            match project::load_recovery_project() {
                                Ok((loaded, rmeta)) => {
                                    let target = rmeta.and_then(|m| m.active_project_path);
                                    self.apply_loaded_project(loaded, target);
                                    self.dirty = true;
                                    self.recovery_available = None;
                                }
                                Err(err) => {
                                    self.last_error =
                                        Some(format!("Could not recover session: {err:#}"));
                                    self.recovery_available = None;
                                }
                            }
                        }
                        if ui.button("Discard").clicked() {
                            project::clear_recovery();
                            self.recovery_available = None;
                        }
                    });
                });
            ui.add_space(6.0);
        }
        if let Some(error) = self.last_error.clone() {
            let foreground = self.palette.error;
            egui::Frame::new()
                .fill(foreground.gamma_multiply(0.14))
                .stroke(eframe::egui::Stroke::new(1.0, foreground))
                .corner_radius(3.0)
                .inner_margin(7.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(foreground, error);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Dismiss").clicked() {
                                self.last_error = None;
                            }
                        });
                    });
                });
            ui.add_space(6.0);
        }

        // Front-and-center Timeline.
        self.timeline_section(ui);
        ui.add_space(6.0);
        ui.separator();

        // Studio Dock Tabs
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.dock_tab, DockTab::Mixer, "Mixer");
            ui.selectable_value(&mut self.dock_tab, DockTab::Inspector, "Inspector & Charts");
            let issues = self.problems().len();
            let manual_label = if issues > 0 {
                format!("Issues & Manual Anchor ({issues})")
            } else {
                "Issues & Manual Anchor".to_string()
            };
            ui.selectable_value(&mut self.dock_tab, DockTab::Manual, manual_label);
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("central_dock_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| match self.dock_tab {
                DockTab::Mixer => {
                    self.mixer_section(ui);
                }
                DockTab::Inspector => {
                    self.detail_section(ui);
                }
                DockTab::Manual => {
                    self.problems_section(ui);
                    ui.add_space(6.0);
                    self.nudge_section(ui);
                    ui.add_space(6.0);
                    self.anchor_section(ui);
                }
            });
    }

    /// The mixer: one row per recording, with its fader, pan, mute, solo and meter.
    ///
    /// The panel sits directly under the timeline, because that is where the user
    /// looks when they ask what the mix will sound like.
    fn mixer_section(&mut self, ui: &mut egui::Ui) {
        // The mixer always has one track per recording, whether or not the analysis
        // has run: a recording that has no alignment yet still deserves a fader.
        self.mixer.sync_to(self.targets.len());

        // The level the playing mix has actually reached, as opposed to the level
        // it is predicted to reach.
        self.mixer.live_peak = match self.player.voice() {
            Some(crate::player::Voice::Mix) => {
                let peak = self.player.mix_peak();
                if peak > 0.0 {
                    Some(peak)
                } else {
                    None
                }
            }
            _ => None,
        };

        let infos: Vec<crate::mixer::TrackInfo> = self
            .targets
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let peak = self
                    .peaks
                    .get(&entry.path)
                    .map(|cache| crate::mixer::peak_of(cache.as_ref()));
                let aligned = self
                    .report
                    .as_ref()
                    .and_then(|report| report.sources.get(index))
                    .map(|alignment| alignment.model().is_some())
                    .unwrap_or(false);
                // The channel count comes from the analysis when there is one, and
                // from the recording's own metadata otherwise.
                let channels = self
                    .report
                    .as_ref()
                    .and_then(|report| report.sources.get(index))
                    .map(|alignment| alignment.source.channels)
                    .or_else(|| entry.meta.as_ref().map(|meta| meta.channels))
                    .unwrap_or(1);
                crate::mixer::TrackInfo {
                    name: entry.name(),
                    peak,
                    aligned,
                    channels: channels.max(1),
                }
            })
            .collect();

        let before_mix = serde_json::to_string(&self.mixer.settings()).ok();
        crate::mixer::panel(ui, &mut self.mixer, &infos);
        let after_mix = serde_json::to_string(&self.mixer.settings()).ok();
        if before_mix != after_mix {
            self.autogain_cache = None;
            self.mark_dirty();
        }
        if self.player.is_playing() && self.player.voice() == Some(crate::player::Voice::Mix) {
            let (live_members, autogain_trim) = self.resolve_preview_mix(false);
            self.player.update_live_mix(
                self.mixer.master_gain,
                self.settings.protection,
                autogain_trim,
                &live_members,
            );
        }
    }

    /// The waveform timeline: full-show overview minimap + stereo lanes + coverage legend.
    fn timeline_section(&mut self, ui: &mut egui::Ui) {
        let duration = self
            .report
            .as_ref()
            .map(|report| report.timeline_duration.max(1.0))
            .or_else(|| {
                self.reference
                    .as_ref()
                    .and_then(|r| r.meta.as_ref())
                    .map(|m| m.duration_secs.max(1.0))
            })
            .unwrap_or(600.0);

        ui.horizontal_wrapped(|ui| {
            ui.strong("Timeline");
            ui.separator();
            timeline::paint_legend(ui, &self.palette);
            if let Some((start, end)) = self.region {
                ui.separator();
                ui.colored_label(
                    self.palette.accent,
                    format!("Sel: {:.3}s – {:.3}s ({:.3}s)", start, end, end - start),
                );
            }
        });

        let width = ui.available_width().max(160.0);

        // Overview minimap strip for full-concert situational awareness & scrubbing.
        let (ov_response, ov_painter) = ui.allocate_painter(
            egui::Vec2::new(width, waveview::OVERVIEW_HEIGHT),
            egui::Sense::click_and_drag(),
        );
        {
            let view = self.waveform_view();
            waveview::paint_overview(
                &ov_painter,
                ov_response.rect,
                &view,
                duration,
                &self.palette,
            );
        }
        if ov_response.clicked() || ov_response.dragged() {
            if let Some(pos) = ov_response.interact_pointer_pos() {
                let frac = ((pos.x - ov_response.rect.left()) / ov_response.rect.width().max(1.0))
                    .clamp(0.0, 1.0) as f64;
                let center = frac * duration;
                let half_span = ((self.view_end - self.view_start) * 0.5).max(0.5);
                self.view_start = (center - half_span).max(0.0);
                self.view_end = self.view_start + half_span * 2.0;
                if ov_response.clicked() {
                    self.seek_playhead(center);
                }
            }
        }

        ui.add_space(3.0);

        let lane_height = waveview::height(self.targets.len());
        let max_tl_height = (ui.available_height() * 0.56).max(200.0);
        egui::ScrollArea::vertical()
            .id_salt("timeline_lanes_scroll")
            .max_height(max_tl_height)
            .show(ui, |ui| {
                let (response, painter) = ui.allocate_painter(
                    egui::Vec2::new(width, lane_height),
                    egui::Sense::click_and_drag(),
                );
                {
                    let view = self.waveform_view();
                    waveview::paint(&painter, response.rect, &view, &self.palette);
                }
                self.handle_timeline_input(ui, &response);
            });
    }

    /// What the waveform timeline needs to draw itself.
    fn waveform_view(&self) -> waveview::WaveformView<'_> {
        let reference_peaks = self
            .reference
            .as_ref()
            .and_then(|entry| self.peaks.get(&entry.path))
            .map(|cache| cache.as_ref());

        let coverage = self
            .report
            .as_ref()
            .map(|report| {
                report
                    .timeline
                    .iter()
                    .map(|span| (span.reference_start, span.reference_end, span.kind))
                    .collect()
            })
            .unwrap_or_default();

        let mut targets = Vec::new();
        for (index, entry) in self.targets.iter().enumerate() {
            let alignment = self
                .report
                .as_ref()
                .and_then(|report| report.sources.get(index));
            let model = alignment.and_then(|alignment| alignment.model());
            let segments = alignment
                .map(|alignment| {
                    alignment
                        .segments
                        .iter()
                        .map(|span| (span.reference_start, span.reference_end, span.kind))
                        .collect()
                })
                .unwrap_or_default();
            targets.push(waveview::TargetLane {
                name: entry.name(),
                peaks: self.peaks.get(&entry.path).map(|cache| cache.as_ref()),
                // A pending nudge moves the picture immediately, so that the user
                // can see what the adjustment does before it is applied.
                model: model.map(|model| {
                    stagesync_core::matching::LinearModel::new(
                        model.scale,
                        model.offset + self.nudges.get(index).copied().unwrap_or(0.0),
                    )
                }),
                segments,
                selected: self.selected_target_index() == Some(index),
                pending_nudge_secs: self.nudges.get(index).copied().unwrap_or(0.0),
                unapplied_anchor: self.has_unapplied_anchor(index),
            });
        }

        waveview::WaveformView {
            start_secs: self.view_start,
            end_secs: self.view_end,
            reference: reference_peaks,
            coverage,
            targets,
            playhead_secs: self.playhead,
            selection: self.region,
            loop_region: self.loop_region,
            playing: None,
        }
    }

    /// What the user does to the timeline: move the playhead, select a region,
    /// pan, and zoom smoothly under the cursor.
    fn handle_timeline_input(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        let rect = response.rect;
        let shift = ui.input(|input| input.modifiers.shift);
        let pointer = response.interact_pointer_pos();

        if let Some(hover) = response.hover_pos().or(pointer) {
            if rect.contains(hover) {
                let (scroll_y, scroll_x) =
                    ui.input(|input| (input.smooth_scroll_delta.y, input.smooth_scroll_delta.x));
                if scroll_y.abs() > 0.01 {
                    let anchor = waveview::time_at(rect, hover.x, self.view_start, self.view_end);
                    self.zoom_view(if scroll_y > 0.0 { 1.0 } else { -1.0 }, anchor);
                }
                if scroll_x.abs() > 0.01 {
                    let span = (self.view_end - self.view_start).max(1e-6);
                    let per_pixel = span / rect.width().max(1.0) as f64;
                    let moved = scroll_x as f64 * per_pixel;
                    self.view_start -= moved;
                    self.view_end -= moved;
                }
            }
        }

        if let Some(pointer) = pointer {
            if response.clicked() {
                let target_pos =
                    waveview::time_at(rect, pointer.x, self.view_start, self.view_end).max(0.0);
                self.seek_playhead(target_pos);
                self.region = None;
            }
        }

        if response.dragged() {
            if shift {
                // Panning: the view follows the pointer.
                let span = (self.view_end - self.view_start).max(1e-6);
                let per_pixel = span / rect.width().max(1.0) as f64;
                let moved = response.drag_delta().x as f64 * per_pixel;
                self.view_start -= moved;
                self.view_end -= moved;
            } else if let (Some(pointer), Some(origin)) =
                (pointer, ui.input(|input| input.pointer.press_origin()))
            {
                let (start, end) =
                    waveview::drag_span(rect, origin.x, pointer.x, self.view_start, self.view_end);
                self.region = Some((start.max(0.0), end.max(0.0)));
            }
        }
    }

    /// Zoom the view in or out around a point on the timeline.
    fn zoom_view(&mut self, direction: f32, anchor: f64) {
        let span = (self.view_end - self.view_start).max(1e-3);
        let factor = if direction > 0.0 { 0.5 } else { 2.0 };
        let new_span = (span * factor).clamp(0.05, 24.0 * 3600.0);
        let ratio = ((anchor - self.view_start) / span).clamp(-4.0, 4.0);
        self.view_start = anchor - ratio * new_span;
        self.view_end = self.view_start + new_span;
    }

    /// Play one of the voices from the playhead.
    fn play_voice(&mut self, voice: Voice) {
        self.voice = voice;
        let position = self.playhead;
        let result = match voice {
            Voice::Reference => {
                let Some(path) = self.reference.as_ref().map(|entry| entry.path.clone()) else {
                    self.last_error = Some("There is no reference recording.".to_string());
                    return;
                };
                self.player.play_reference(&path, position)
            }
            Voice::Target => {
                let Some(index) = self.selected_target_index() else {
                    self.last_error =
                        Some("Select a target recording in the file list to hear it.".to_string());
                    return;
                };
                let Some(alignment) = self
                    .report
                    .as_ref()
                    .and_then(|report| report.sources.get(index))
                else {
                    self.last_error = Some("Run the analysis before listening to a target.".into());
                    return;
                };
                let Some(model) = alignment.model() else {
                    self.last_error = Some(
                        "This recording has no alignment, so there is nothing to play.".to_string(),
                    );
                    return;
                };
                let path = alignment.source.path.clone();
                let model = stagesync_core::matching::LinearModel::new(
                    model.scale,
                    model.offset + self.nudges.get(index).copied().unwrap_or(0.0),
                );
                self.player.play_target(&path, &model, position)
            }
            Voice::Mix => {
                if self.mixer.mode == stagesync_core::mixer::MonitorMode::ReferenceOnly {
                    // The reference is monitored on its own. It is never a track of
                    // the mix, so "the mix" in this mode is the reference alone.
                    let Some(path) = self.reference.as_ref().map(|entry| entry.path.clone()) else {
                        self.last_error = Some("There is no reference recording.".to_string());
                        return;
                    };
                    self.player.play_reference(&path, position)
                } else {
                    let members = self.mix_members();
                    if members.is_empty() {
                        self.last_error = Some(
                            "There is nothing to mix: run the analysis and choose the \
                             recordings in the mixer."
                                .to_string(),
                        );
                        return;
                    }
                    let master = self.mixer.master_gain;
                    let protection = self.settings.protection;
                    let trim = self.preview_autogain_trim();
                    self.player
                        .play_mix(&members, master, protection, trim, position)
                }
            }
        };
        match result {
            Ok(()) => {
                self.last_error = None;
                self.notice = Some(format!("Playing the {}.", voice.label().to_lowercase()));
            }
            Err(error) => {
                self.player.stop();
                self.last_error = Some(format!("Playback failed: {error:#}"));
            }
        }
    }

    /// Compute cached per-track RMS loudness adjustments matching [`stagesync_core::mixer::loudness_gains`].
    fn loudness_gains_for_mixer(&mut self) -> Vec<f64> {
        let mut gains = vec![0.0f64; self.mixer.tracks.len()];
        if self.settings.loudness == stagesync_core::mixer::LoudnessMatch::None {
            return gains;
        }
        let Some(report) = self.report.as_ref() else {
            return gains;
        };
        let mut levels: Vec<(usize, f64)> = Vec::new();
        for track in &self.mixer.tracks {
            let Some(alignment) = report.sources.get(track.source_index) else {
                continue;
            };
            let level = if let Some(&cached) = self.rms_cache.get(&alignment.source.path) {
                cached
            } else if let Ok(measured) =
                stagesync_core::mixer::measure_rms_dbfs(&alignment.source.path)
            {
                self.rms_cache
                    .insert(alignment.source.path.clone(), measured);
                measured
            } else {
                continue;
            };
            levels.push((track.source_index, level));
        }
        let offsets = stagesync_core::mixer::loudness_offsets_db(&levels);
        for (idx, track) in self.mixer.tracks.iter().enumerate() {
            if let Some((_, db)) = offsets.iter().find(|(s, _)| *s == track.source_index) {
                gains[idx] = *db;
            }
        }
        gains
    }

    /// Fingerprint of active mixer, loudness, protection, and nudge parameters for caching AutoGain.
    fn mix_cache_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.mixer.master_gain.to_bits().hash(&mut hasher);
        self.mixer.mode.label().hash(&mut hasher);
        std::mem::discriminant(&self.settings.loudness).hash(&mut hasher);
        std::mem::discriminant(&self.settings.protection).hash(&mut hasher);
        self.settings
            .protection
            .target_dbfs()
            .to_bits()
            .hash(&mut hasher);
        self.settings.sample_rate.hash(&mut hasher);
        for t in &self.mixer.tracks {
            t.source_index.hash(&mut hasher);
            t.enabled.hash(&mut hasher);
            t.mute.hash(&mut hasher);
            t.solo.hash(&mut hasher);
            t.gain.to_bits().hash(&mut hasher);
            t.trim.to_bits().hash(&mut hasher);
            t.pan.to_bits().hash(&mut hasher);
            std::mem::discriminant(&t.pan_law).hash(&mut hasher);
            std::mem::discriminant(&t.stereo_pan).hash(&mut hasher);
        }
        for n in &self.nudges {
            n.to_bits().hash(&mut hasher);
        }
        if let Some(report) = &self.report {
            report.timeline_duration.to_bits().hash(&mut hasher);
            for s in &report.sources {
                s.source.sha256.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// Compute the two-pass `AutoGain` trim for preview using the exact exported mix arithmetic.
    fn preview_autogain_trim(&mut self) -> f32 {
        if !matches!(
            self.settings.protection,
            stagesync_core::mixer::ClippingProtection::AutoGain { .. }
        ) {
            return 1.0;
        }
        let key = self.mix_cache_key();
        if let Some((cached_key, trim)) = self.autogain_cache {
            if cached_key == key {
                return trim;
            }
        }
        let Some(base_report) = self.report.as_ref() else {
            return 1.0;
        };
        let eff_report = self.effective_report_for_export(base_report);
        let loudness_db = self.loudness_gains_for_mixer();
        let mix_settings = self.mixer.settings();
        let mix_opts = stagesync_core::mixer::MixOptions {
            sample_rate: Some(
                self.settings
                    .sample_rate
                    .unwrap_or_else(|| self.player.output_rate())
                    .max(1),
            ),
            channels: 2,
            format: self.settings.format,
            protection: self.settings.protection,
            loudness: self.settings.loudness,
            container: self.settings.container,
            block_frames: 65_536,
            ..Default::default()
        };
        if let Ok((trim, _)) = stagesync_core::mixer::measure_autogain_trim(
            &eff_report,
            &mix_settings,
            &mix_opts,
            &loudness_db,
        ) {
            self.autogain_cache = Some((key, trim));
            trim
        } else {
            1.0
        }
    }

    /// Build mix members and optional auto-gain trim (`compute_autogain = false` during slider drags).
    fn resolve_preview_mix(
        &mut self,
        compute_autogain: bool,
    ) -> (Vec<crate::player::MixMember>, f32) {
        let members = self.mix_members();
        let trim = if compute_autogain {
            self.preview_autogain_trim()
        } else if matches!(
            self.settings.protection,
            stagesync_core::mixer::ClippingProtection::AutoGain { .. }
        ) {
            let key = self.mix_cache_key();
            self.autogain_cache
                .and_then(|(k, t)| (k == key).then_some(t))
                .unwrap_or(1.0)
        } else {
            1.0
        };
        (members, trim)
    }

    /// The recordings the mixer plays, with their corrections and RMS loudness matching.
    ///
    /// The reference is never among them: it is the thing the other recordings were
    /// aligned to, and mixing it in would double every one of them.
    fn mix_members(&mut self) -> Vec<crate::player::MixMember> {
        let loudness_db = self.loudness_gains_for_mixer();
        let Some(report) = self.report.as_ref() else {
            return Vec::new();
        };
        let monitored = self.mixer.monitored();
        let solo_active = monitored.iter().any(|t| t.enabled && t.solo);
        let mut members = Vec::with_capacity(monitored.len());
        for track in monitored {
            let Some(alignment) = report.sources.get(track.source_index) else {
                continue;
            };
            let Some(model) = alignment.model() else {
                continue;
            };
            // A pending nudge moves the recording immediately, so that the mix is
            // heard the way the picture shows it.
            let model = stagesync_core::matching::LinearModel::new(
                model.scale,
                model.offset + self.nudges.get(track.source_index).copied().unwrap_or(0.0),
            );
            let track_loudness_db = self
                .mixer
                .tracks
                .iter()
                .position(|t| t.source_index == track.source_index)
                .and_then(|pos| loudness_db.get(pos).copied())
                .unwrap_or(0.0);
            let mut member =
                crate::player::MixMember::from_track(alignment.source.path.clone(), model, &track);
            member.gain = track.effective_gain(solo_active)
                * stagesync_core::mixer::db_to_linear(track_loudness_db);
            members.push(member);
        }
        members
    }

    /// The places worth listening to.
    ///
    /// The gaps in the coverage and the spans the matcher was not sure about are
    /// where a recording is missing or where the alignment is doubtful. They are
    /// listed with a button that takes the playhead there, so that "something is
    /// wrong with this show" becomes a place the user can go.
    fn problems_section(&mut self, ui: &mut egui::Ui) {
        let problems = self.problems();
        if problems.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(format!("Places to look at ({})", problems.len()))
            .default_open(false)
            .show(ui, |ui| {
                for (label, position) in problems.iter().take(20) {
                    ui.horizontal(|ui| {
                        if ui.button(label.clone()).clicked() {
                            self.jump_to(*position);
                        }
                    });
                }
            });
    }

    /// The places worth looking at, and where they are.
    fn problems(&self) -> Vec<(String, f64)> {
        let mut out: Vec<(String, f64)> = Vec::new();
        let Some(report) = self.report.as_ref() else {
            return out;
        };
        for span in &report.timeline {
            if span.kind == SegmentKind::UncoveredReference {
                out.push((
                    format!("Nothing covers {:.1} s", span.reference_start),
                    span.reference_start,
                ));
            }
        }
        for alignment in &report.sources {
            for span in &alignment.segments {
                if span.kind == SegmentKind::LowConfidence {
                    out.push((
                        format!(
                            "{}: uncertain at {:.1} s",
                            alignment.source.file_name, span.reference_start
                        ),
                        span.reference_start,
                    ));
                }
            }
        }
        out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Move the playhead to a place, bring it into view, and seek active playback.
    fn jump_to(&mut self, position: f64) {
        self.seek_playhead(position);
        let span = (self.view_end - self.view_start).max(1e-3);
        self.view_start = self.playhead - span * 0.5;
        self.view_end = self.view_start + span;
    }

    /// The manual adjustment of the selected recording's placement.
    ///
    /// A nudge moves the **placement** and nothing else. The rate the sync engine
    /// measured is left alone, because a recording that is a few milliseconds out
    /// says nothing about its clock.
    fn nudge_section(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.selected_target_index() else {
            return;
        };
        let name = self.targets[index].name();
        let can_run =
            self.reference.is_some() && !self.targets.is_empty() && self.running.is_none();

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Nudge {name}"));
            for (label, delta) in [
                ("-100 ms", -0.100),
                ("-10 ms", -0.010),
                ("-1 ms", -0.001),
                ("+1 ms", 0.001),
                ("+10 ms", 0.010),
                ("+100 ms", 0.100),
            ] {
                if ui.button(label).clicked() {
                    self.nudge(index, delta);
                }
            }
        });

        let applied = self.nudges.get(index).copied().unwrap_or(0.0);
        if applied.abs() > 1e-9 {
            wrapped(
                ui,
                format!(
                    "The placement is moved {:+.1} ms. The rate is not changed. Press Apply to \
                     re-measure and re-check the recording at its new position.",
                    applied * 1000.0
                ),
            );
        }

        let mut apply = false;
        let mut reset = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_run && applied.abs() > 1e-9,
                    egui::Button::new("Apply and re-check"),
                )
                .clicked()
            {
                apply = true;
            }
            if ui
                .add_enabled(
                    can_run && !self.nudge_history.is_empty(),
                    egui::Button::new("Undo"),
                )
                .clicked()
            {
                self.undo_nudge();
            }
            if ui
                .add_enabled(can_run, egui::Button::new("Reset to automatic"))
                .clicked()
            {
                reset = true;
            }
        });

        if apply {
            self.apply_nudge(index);
        }
        if reset {
            self.reset_to_automatic(index);
        }
    }

    /// Move one recording's placement by a number of milliseconds.
    fn nudge(&mut self, index: usize, delta: f64) {
        let current = self.nudges.get(index).copied().unwrap_or(0.0);
        if self.nudges.len() <= index {
            self.nudges.resize(index + 1, 0.0);
        }
        self.nudges[index] = current + delta;
        self.nudge_history.push((index, current));
        self.mark_dirty();
    }

    /// Take back the last nudge.
    fn undo_nudge(&mut self) {
        if let Some((index, previous)) = self.nudge_history.pop() {
            if let Some(value) = self.nudges.get_mut(index) {
                *value = previous;
                self.mark_dirty();
            }
        }
    }

    /// Apply a pending nudge and re-measure the recording at its new position.
    ///
    /// The nudge becomes a manual anchor: the model is forced through the new
    /// point, and the rate the sync engine measured is kept. The analysis then
    /// runs again, so the recording is re-checked rather than merely moved.
    fn apply_nudge(&mut self, index: usize) {
        let Some(alignment) = self
            .report
            .as_ref()
            .and_then(|report| report.sources.get(index))
        else {
            return;
        };
        let Some(model) = alignment.model() else {
            return;
        };
        let nudge = self.nudges.get(index).copied().unwrap_or(0.0);
        let proposed_anchor = AnchorFields {
            source_time: 0.0,
            reference_time: model.offset + nudge,
            active: true,
        };
        // Stage the anchor & zero-nudge commit until AnalysisFinished succeeds so a
        // cancelled or failed re-analysis never drops the user's pending nudge.
        self.pending_analysis_commit = Some(PendingAnalysisCommit::ApplyNudge {
            target_index: index,
            proposed_anchor,
        });
        self.push_log(format!(
            "{} moved by {:+.1} ms; re-measuring",
            self.targets[index].name(),
            nudge * 1000.0
        ));
        self.start_analysis();
    }

    /// Drop the manual adjustment of one recording and go back to the automatic
    /// result.
    fn reset_to_automatic(&mut self, index: usize) {
        self.pending_analysis_commit = Some(PendingAnalysisCommit::ResetToAutomatic {
            target_index: index,
        });
        self.push_log(format!(
            "{} resetting to the automatic match",
            self.targets[index].name()
        ));
        self.start_analysis();
    }

    /// The manual anchor controls.
    ///
    /// The anchor belongs to the selected target recording. It is a hard
    /// constraint on that recording alone: the model of the anchored recording
    /// is forced through the point that is given, and the other recordings keep
    /// whatever the automatic match found for them.
    fn anchor_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Manual anchor")
            .default_open(true)
            .show(ui, |ui| {
                let Some(index) = self.selected_target_index() else {
                    wrapped(
                        ui,
                        "Use this when the automatic match fails. Select a target recording \
                         in the file list to anchor it: the anchor is a hard constraint on that \
                         recording alone, and the other recordings keep the alignment the \
                         automatic match found for them.",
                    );
                    return;
                };
                let name = self.targets[index].name();
                let can_run =
                    self.reference.is_some() && !self.targets.is_empty() && self.running.is_none();
                wrapped(
                    ui,
                    format!(
                        "Use this when the automatic match fails. The anchor is an override on \
                         {name}: the model of that recording is forced through the point you give, \
                         whatever the match found, and the other recordings are left alone. The \
                         rate the match measured is kept; only the placement moves."
                    ),
                );

                // The anchor is copied out before the controls are drawn: the
                // buttons below start a new analysis, and that needs the whole
                // application, not just this one field.
                let Some(anchor) = self.anchors.get(index) else {
                    return;
                };
                let mut active = anchor.active;
                let mut source_time = anchor.source_time;
                let mut reference_time = anchor.reference_time;

                ui.checkbox(&mut active, "Use this anchor for this recording");
                ui.horizontal_wrapped(|ui| {
                    let src_lbl = ui.label("Source time");
                    ui.add(
                        egui::DragValue::new(&mut source_time)
                            .speed(0.05)
                            .range(0.0..=1_000_000.0),
                    )
                    .labelled_by(src_lbl.id);
                    ui.label("s");
                    let ref_lbl = ui.label("Reference time");
                    ui.add(
                        egui::DragValue::new(&mut reference_time)
                            .speed(0.05)
                            .range(0.0..=1_000_000.0),
                    )
                    .labelled_by(ref_lbl.id);
                    ui.label("s");
                });

                // What the last analysis actually did with the anchor. The user has
                // to be able to see whether it was applied, and whether it moved the
                // placement away from the match.
                if let Some(state) = self
                    .report
                    .as_ref()
                    .and_then(|report| report.sources.get(index))
                    .and_then(|alignment| alignment.override_state)
                {
                    wrapped(
                        ui,
                        format!(
                            "Applied: the placement was moved {:+.1} ms relative to the automatic \
                             match (which placed it at {:+.3} s).",
                            state.shift_ms, state.automatic_offset
                        ),
                    );
                }

                let mut apply = false;
                let mut clear = false;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_run, egui::Button::new("Apply anchor"))
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui
                        .add_enabled(active, egui::Button::new("Reset to automatic"))
                        .clicked()
                    {
                        clear = true;
                    }
                    if active {
                        ui.label("the anchor is in effect");
                    }
                });

                if let Some(anchor) = self.anchors.get_mut(index) {
                    anchor.source_time = source_time;
                    anchor.reference_time = reference_time;
                    if clear {
                        anchor.active = false;
                    }
                }
                if apply {
                    if let Some(anchor) = self.anchors.get_mut(index) {
                        anchor.active = true;
                    }
                    self.start_analysis();
                } else if clear {
                    self.start_analysis();
                }
            });
    }

    /// The details of the selected target, and the drift charts.
    fn detail_section(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.selected_target_index() else {
            wrapped(
                ui,
                "Select a target recording in the file list to see how it was aligned.",
            );
            return;
        };

        let (facts, points, model) = {
            let Some(report) = self.report.as_ref() else {
                ui.label("Run the analysis to see the alignment details.");
                return;
            };
            let Some(alignment) = report.sources.get(index) else {
                ui.label("This target has no alignment result.");
                return;
            };
            (
                facts_for(alignment),
                alignment.points.clone(),
                alignment.model(),
            )
        };

        let corrected = self.export_outcome.as_ref().and_then(|outcome| {
            outcome
                .corrected
                .iter()
                .find(|(source, _)| *source == index)
                .map(|(_, path)| path.clone())
        });
        let reference_is_wav = self
            .reference
            .as_ref()
            .is_some_and(|entry| is_wav(&entry.path));
        let can_verify = corrected.is_some() && reference_is_wav && !self.verifying;
        let mut verify_clicked = false;
        let palette = self.palette;

        let render_facts = |ui: &mut egui::Ui, verify_clicked: &mut bool| {
            ui.strong(facts.name.as_str());
            ui.add_space(2.0);

            egui::Grid::new("source_facts")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Offset at t0");
                    ui.label(match facts.offset_secs {
                        Some(offset) => format!("{offset:+.4} s"),
                        None => "no alignment".to_string(),
                    });
                    ui.end_row();

                    ui.label("Drift");
                    // The drift is shown with the uncertainty that produced it, or
                    // it is not shown at all: a rate without its precision is a
                    // number, not a measurement. A source whose drift was not
                    // established says so, rather than showing a figure of zero.
                    ui.label(match &facts.drift {
                        Some(assessment) => match assessment.status {
                            stagesync_core::matching::drift::DriftStatus::Established => format!(
                                "{:+.3} ± {:.3} ppm",
                                assessment.drift_ppm, assessment.slope_se_ppm
                            ),
                            stagesync_core::matching::drift::DriftStatus::OffsetOnly => {
                                "not applied — only the offset was corrected".to_string()
                            }
                        },
                        None => "no alignment".to_string(),
                    });
                    ui.end_row();

                    ui.label("Accumulated drift");
                    ui.label(match facts.accumulated_ms {
                        Some(ms) => format!("{ms:+.2} ms"),
                        None => "no alignment".to_string(),
                    });
                    ui.end_row();

                    ui.label("Coverage");
                    ui.label(format!(
                        "{:.1}%  ({} ok · {} low · {} gap)",
                        facts.coverage * 100.0,
                        format_duration(facts.covered_secs),
                        format_duration(facts.low_secs),
                        format_duration(facts.uncovered_secs)
                    ));
                    ui.end_row();

                    ui.label("Residual p50/p95/max");
                    ui.label(format!(
                        "{:+.2} / {:+.2} / {:.2} ms ({} pts)",
                        facts.residuals.0, facts.residuals.1, facts.residuals.2, facts.point_count
                    ));
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.strong("Drift verdict");
            match drift_verdict(facts.drift.as_ref()) {
                Some(verdict) => {
                    egui::Grid::new("drift_verdict")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Status");
                            ui.label(verdict.status.clone());
                            ui.end_row();

                            ui.label("Measured by");
                            ui.label(verdict.method.clone());
                            ui.end_row();

                            ui.label("Applied drift");
                            ui.label(verdict.applied.clone());
                            ui.end_row();
                        });

                    if verdict.envelope.is_some() || verdict.waveform.is_some() {
                        egui::Grid::new("drift_methods")
                            .num_columns(4)
                            .show(ui, |ui| {
                                ui.strong("Estimator");
                                ui.strong("Drift");
                                ui.strong("RMS");
                                ui.strong("Points");
                                ui.end_row();
                                if let Some(envelope) = &verdict.envelope {
                                    ui.label("envelope");
                                    ui.label(envelope.drift.clone());
                                    ui.label(format!("{:.2} ms", envelope.residual_rms_ms));
                                    ui.label(envelope.points_used.to_string());
                                    ui.end_row();
                                }
                                if let Some(waveform) = &verdict.waveform {
                                    ui.label("waveform (gcc_phat)");
                                    ui.label(waveform.drift.clone());
                                    ui.label(format!("{:.2} ms", waveform.residual_rms_ms));
                                    ui.label(waveform.points_used.to_string());
                                    ui.end_row();
                                }
                            });
                        if let Some(correlation) = verdict
                            .waveform
                            .as_ref()
                            .and_then(|waveform| waveform.mean_correlation)
                        {
                            ui.label(format!("mean raw-waveform correlation: {correlation:.3}"));
                        }
                    }

                    for note in &verdict.notes {
                        ui.colored_label(weak_text(ui), note.as_str());
                    }
                }
                None => {
                    ui.label(
                        "No drift verdict: the analysis found no correspondence between this \
                         recording and the reference, so there is nothing to report.",
                    );
                }
            }

            if !facts.warnings.is_empty() {
                ui.label("Warnings");
                for warning in &facts.warnings {
                    ui.colored_label(ui.visuals().warn_fg_color, warning.as_str());
                }
            }

            if let Some(holdout) = &facts.holdout {
                ui.label("Hold-out verification (waveform GCC-PHAT, independent of the fit)");
                egui::Grid::new("holdout_facts")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Residual p50 / p95 / max");
                        ui.label(format!(
                            "{:+.3} / {:+.3} / {:.3} ms",
                            holdout.p50_ms, holdout.p95_ms, holdout.max_ms
                        ));
                        ui.end_row();
                        ui.label("Counted points");
                        ui.label(format!("{} of {}", holdout.counted, holdout.points.len()));
                        ui.end_row();
                        ui.label("Pitch change");
                        ui.label(format!("{:+.4} cents", holdout.pitch_cents));
                        ui.end_row();
                        if let Some(drift_error) = holdout.drift_error_ppm {
                            let uncertainty = holdout
                                .drift_error_se_ppm
                                .map(|se| format!(" ± {se:.3} ppm"))
                                .unwrap_or_default();
                            ui.label("How wrong the applied drift is");
                            ui.label(format!("{drift_error:+.3} ppm{uncertainty}"));
                            ui.end_row();
                        }
                    });
                for note in &holdout.notes {
                    ui.colored_label(ui.visuals().warn_fg_color, note.as_str());
                }
            }

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        can_verify,
                        egui::Button::new("Verify the corrected audio (hold-out)"),
                    )
                    .clicked()
                {
                    *verify_clicked = true;
                }
                if corrected.is_none() {
                    ui.label("export the corrected audio to enable the hold-out check");
                } else if !reference_is_wav {
                    ui.label("the hold-out check needs a WAV reference");
                }
            });
        };

        if ui.available_width() >= 760.0 {
            ui.columns(2, |cols| {
                render_facts(&mut cols[0], &mut verify_clicked);
                chart::paint_lag_plot(&mut cols[1], &points, model, facts.duration, &palette);
                chart::paint_residual_plot(&mut cols[1], &points, facts.duration, &palette);
                chart::paint_drift_plot(&mut cols[1], &points, model, facts.duration, &palette);
            });
        } else {
            render_facts(ui, &mut verify_clicked);
            ui.add_space(6.0);
            chart::paint_lag_plot(ui, &points, model, facts.duration, &palette);
            chart::paint_residual_plot(ui, &points, facts.duration, &palette);
            chart::paint_drift_plot(ui, &points, model, facts.duration, &palette);
        }

        if verify_clicked {
            self.start_verification(index);
        }
    }
}

/// Format timeline position cleanly in `H:MM:SS.mmm` for studio transport readout.
fn format_timecode(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "--:--:--.---".to_string();
    }
    let ms_total = (seconds * 1000.0).round() as u64;
    let ms = ms_total % 1000;
    let total_secs = ms_total / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    format!("{hours}:{mins:02}:{secs:02}.{ms:03}")
}

/// Construct a `FileEntry` from a resolved `.stagesync` project reference.
fn entry_from_resolved(resolved: &project::ResolvedMedia) -> FileEntry {
    let mut entry = FileEntry::new(resolved.resolved_path.clone());
    entry.expected_sha256 = resolved.saved.sha256.clone();
    entry.expected_byte_len = resolved.saved.byte_len;
    match &resolved.status {
        MediaStatus::Verified | MediaStatus::RelocatedByRelativePath => {}
        MediaStatus::Missing => {
            entry.missing = true;
            entry.error = Some("file not found on disk".to_string());
        }
        MediaStatus::ModifiedOnDisk { detail } => {
            entry.modified_warning = Some(detail.clone());
        }
    }
    entry
}

/// Update `entry` after the user locates it on disk, returning `true` if bytes changed.
fn relocate_entry(entry: &mut FileEntry, new_path: PathBuf) -> bool {
    let changed = match (&entry.expected_byte_len, &entry.expected_sha256) {
        (Some(exp_len), Some(exp_hash)) => {
            let len_ok = std::fs::metadata(&new_path)
                .map(|m| m.len() == *exp_len)
                .unwrap_or(false);
            let hash_ok = len_ok
                && stagesync_core::audio::sha256_file(&new_path)
                    .map(|h| h == *exp_hash)
                    .unwrap_or(false);
            !hash_ok
        }
        _ => true,
    };
    entry.path = new_path;
    entry.missing = false;
    entry.error = None;
    entry.meta = None;
    entry.modified_warning = if changed {
        Some("Relocated file has different audio bytes — re-run Auto Sync.".to_string())
    } else {
        None
    };
    changed
}

/// How many characters of a path are shown before the middle is elided.
///
/// The monospace font of the file list is about seven pixels wide, and the
/// file panel is at most 420 pixels across, so this keeps a path on one line
/// without letting it push the panel wider than its cap.
const PATH_CHARS: usize = 48;

/// The colour of secondary text, which follows the theme.
fn weak_text(ui: &egui::Ui) -> Color32 {
    ui.visuals()
        .weak_text_color
        .unwrap_or_else(|| ui.visuals().text_color())
}

/// A label that wraps at the edge of its panel instead of running past it.
///
/// A plain `ui.label` never wraps, and egui sizes a panel to its content, so
/// an unwrapped sentence makes the panel as wide as the sentence. The wrap is
/// made a little narrower than the panel, because the panel's own frame puts
/// its text a few pixels inside its edge and a word that ran over that edge
/// was cut in half.
fn wrapped(ui: &mut egui::Ui, text: impl Into<String>) {
    let panel_width = ui.max_rect().width();
    ui.set_max_width((panel_width - 12.0).max(40.0));
    ui.add(egui::Label::new(text.into()).wrap());
    ui.set_max_width(panel_width);
}

/// Read the figures of one source out of its alignment.
fn facts_for(alignment: &SourceAlignment) -> SourceFacts {
    let model = alignment.model();
    let residuals: Vec<f64> = alignment
        .points
        .iter()
        .map(|point| point.residual_ms)
        .collect();
    SourceFacts {
        name: alignment.source.file_name.clone(),
        duration: alignment.source.duration_secs,
        offset_secs: model.map(|model| model.offset),
        accumulated_ms: model
            .map(|model| model.accumulated_drift_ms(0.0, alignment.source.duration_secs)),
        coverage: alignment.coverage.fraction,
        covered_secs: alignment.coverage.covered_seconds,
        low_secs: alignment.coverage.low_confidence_seconds,
        uncovered_secs: alignment.coverage.uncovered_seconds,
        residuals: residual_stats(&residuals),
        point_count: alignment.points.len(),
        warnings: alignment.warnings.clone(),
        holdout: alignment.holdout.clone(),
        drift: alignment.drift.clone(),
    }
}

/// The measured figures of one target, as one line of text.
fn metrics_line(metrics: &crate::state::TargetMetrics) -> String {
    match (
        metrics.offset_secs,
        metrics.drift_ppm,
        metrics.drift_se_ppm,
        metrics.accumulated_drift_ms,
    ) {
        (Some(offset), Some(ppm), Some(se), Some(accumulated)) => format!(
            "offset {offset:+.3} s · drift {ppm:+.2} ± {se:.2} ppm · accumulated {accumulated:+.1} ms · coverage {:.1}%",
            metrics.coverage.unwrap_or(0.0) * 100.0
        ),
        // A drift without an uncertainty is not shown: it would be a number
        // with nothing behind it.
        (Some(offset), None, _, Some(accumulated))
        | (Some(offset), _, None, Some(accumulated)) => format!(
            "offset {offset:+.3} s · drift not applied · accumulated {accumulated:+.1} ms · coverage {:.1}%",
            metrics.coverage.unwrap_or(0.0) * 100.0
        ),
        (Some(offset), None, _, None) | (Some(offset), _, None, None) => format!(
            "offset {offset:+.3} s · drift not applied · coverage {:.1}%",
            metrics.coverage.unwrap_or(0.0) * 100.0
        ),
        _ => "no alignment yet".to_string(),
    }
}

impl eframe::App for StageSyncApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.layout(ui);
    }
}

impl StageSyncApp {
    /// Draw the whole window.
    ///
    /// The panels are laid out from the root UI that eframe hands over, so the
    /// file list, the centre and the export controls divide the window between
    /// them. This is separate from [`eframe::App::ui`] so that the layout can
    /// be laid out head-lessly, at any window size, in a test.
    fn layout(&mut self, ui: &mut egui::Ui) {
        self.palette = crate::theme::apply_theme(ui.ctx(), self.theme_mode.is_dark(ui.ctx()));
        self.drain_events();
        self.follow_the_playhead();
        self.keep_window_on_screen(ui);

        // Files dropped onto the window are the primary way to add recordings. A
        // dropped folder is opened and the recordings inside it are taken.
        let dropped: Vec<PathBuf> = ui.ctx().input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            let is_single_project = dropped.len() == 1
                && dropped[0]
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case(PROJECT_EXTENSION));
            if is_single_project {
                self.request_action(PendingAction::OpenProject(dropped[0].clone()));
            } else {
                let expanded = expand_dropped_paths(&dropped);
                if expanded.is_empty() {
                    self.last_error =
                        Some("There are no recordings in what was dropped.".to_string());
                } else {
                    self.import_paths(expanded);
                }
            }
        }

        // The size of the window is remembered for the next run.
        let size = ui
            .ctx()
            .input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
        if let Some(size) = size {
            if size.x > 0.0 && size.y > 0.0 {
                self.window_width = size.x;
                self.window_height = size.y;
            }
        }
        if ui.ctx().input(|input| input.viewport().close_requested()) {
            if self.dirty && !self.force_close {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if self.pending_confirm.is_none() {
                    self.pending_confirm = Some(PendingAction::CloseWindow);
                }
            } else if !self.closing_saved {
                self.closing_saved = true;
                self.persist_settings();
                project::clear_recovery();
            }
        }

        self.maybe_autosave();
        self.handle_shortcuts(ui.ctx());

        let window = ui.available_width();
        let files_max = (window * 0.28).clamp(210.0, 340.0);
        let export_max = (window * 0.27).clamp(210.0, 320.0);

        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::side_top_panel(&ui.ctx().global_style())
                    .fill(self.palette.bg_panel)
                    .inner_margin(egui::Margin::symmetric(8, 5)),
            )
            .show(ui, |ui| self.toolbar(ui));

        let bottom_panel = egui::Panel::bottom("messages").resizable(self.show_log);
        let bottom_panel = if self.show_log {
            bottom_panel.default_size(135.0)
        } else {
            bottom_panel.exact_size(26.0)
        };
        bottom_panel.show(ui, |ui| self.log_panel(ui));

        egui::Panel::left("files")
            .resizable(true)
            .default_size(280.0_f32.min(files_max))
            .max_size(files_max)
            .show(ui, |ui| self.file_panel(ui));

        if self.show_export {
            egui::Panel::right("export")
                .resizable(true)
                .default_size(270.0_f32.min(export_max))
                .max_size(export_max)
                .show(ui, |ui| self.export_panel(ui));
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ui.ctx().global_style())
                    .fill(self.palette.bg_canvas)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ui, |ui| self.central(ui));

        self.draw_pending_confirm(ui.ctx());

        if self.force_close {
            self.dirty = false;
            project::clear_recovery();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if self.running.is_some() || self.peaks_pending > 0 || self.verifying {
            ui.ctx().request_repaint();
        } else if self.player.is_playing() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
    }

    /// Global keyboard shortcuts (disabled while typing into a text input).
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.pending_confirm.is_some() {
            return;
        }
        let duration = self
            .report
            .as_ref()
            .map(|report| report.timeline_duration.max(1.0))
            .unwrap_or(600.0);

        let cmd = ctx.input(|i| i.modifiers.command);
        let shift = ctx.input(|i| i.modifiers.shift);
        let alt = ctx.input(|i| i.modifiers.alt);

        if cmd && ctx.input(|i| i.key_pressed(egui::Key::N)) {
            self.request_action(PendingAction::NewProject);
        } else if cmd && ctx.input(|i| i.key_pressed(egui::Key::O)) {
            self.request_action(PendingAction::PickOpenProject);
        } else if cmd && shift && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            self.browse_save_project(true);
        } else if cmd && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            self.browse_save_project(false);
        } else if cmd && ctx.input(|i| i.key_pressed(egui::Key::E)) {
            self.start_export();
        } else if cmd && ctx.input(|i| i.key_pressed(egui::Key::Z)) {
            self.undo_nudge();
        } else if alt && ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            if let Some(idx) = self.selected_target_index() {
                self.nudge(idx, -0.005);
            }
        } else if alt && ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            if let Some(idx) = self.selected_target_index() {
                self.nudge(idx, 0.005);
            }
        } else if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            if self.player.is_playing() {
                self.player.stop();
            } else {
                self.play_voice(self.voice);
            }
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
            self.play_voice(Voice::Reference);
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
            self.play_voice(Voice::Target);
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
            self.play_voice(Voice::Mix);
        } else if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
            let next = match self.voice {
                Voice::Reference => Voice::Target,
                _ => Voice::Reference,
            };
            self.play_voice(next);
        } else if ctx.input(|i| i.key_pressed(egui::Key::L)) {
            if self.loop_region.is_some() {
                self.loop_region = None;
            } else if self.region.is_some() {
                self.loop_region = self.region;
            }
        } else if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.view_start = 0.0;
            self.view_end = duration;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Home)) {
            self.seek_playhead(0.0);
            self.view_start = 0.0;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals))
        {
            self.zoom_view(1.0, self.playhead);
        } else if ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
            self.zoom_view(-1.0, self.playhead);
        }
    }

    /// Keep the window inside the monitor it is on.
    ///
    /// A window that is larger than the screen is not shown in full: the window
    /// manager clips it, the layout is still computed for the whole window, and
    /// the panels that lie outside the screen are simply not visible. The user
    /// is left with a window that looks broken rather than one that is too big.
    ///
    /// The window is therefore asked to shrink to the monitor when it does not
    /// fit. The request is made a few times only: a window manager that ignores
    /// the request must not turn into an endless stream of them.
    fn keep_window_on_screen(&mut self, ui: &egui::Ui) {
        const MARGIN: f32 = 24.0;
        const ATTEMPTS: usize = 8;

        let viewport = ui.ctx().input(|input| input.viewport().clone());
        let (Some(monitor), Some(inner)) = (viewport.monitor_size, viewport.inner_rect) else {
            return;
        };
        if monitor.x < 200.0 || monitor.y < 200.0 {
            return;
        }
        let size = inner.size();
        let max = egui::vec2(monitor.x - MARGIN, monitor.y - MARGIN);
        if size.x <= max.x && size.y <= max.y {
            return;
        }
        if self.size_requests >= ATTEMPTS {
            return;
        }
        self.size_requests += 1;
        let wanted = egui::vec2(size.x.min(max.x), size.y.min(max.y));
        log::info!("the window is {size:?} and the monitor is {monitor:?}; asking for {wanted:?}");
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(wanted));
    }

    /// Seek the playhead and restart active audio playback at the new position.
    fn seek_playhead(&mut self, position: f64) {
        self.playhead = position.max(0.0);
        if self.player.is_playing() {
            let voice = self.voice;
            self.play_voice(voice);
        }
    }

    /// Move the playhead with the sound, surface backend stalls/errors, and keep a loop going.
    ///
    /// The playhead is the position on the reference timeline that is being heard.
    /// It follows the player, and when a loop is set the playback starts again at
    /// the beginning of the region as soon as it reaches the end of it.
    fn follow_the_playhead(&mut self) {
        if let Some(err) = self.player.take_error() {
            self.last_error = Some(err.clone());
            self.push_log(format!("audio error: {err}"));
        }
        self.player.poll();
        if !self.player.is_playing() {
            return;
        }
        let position = self.player.position();
        self.playhead = position;
        if let Some((start, end)) = self.loop_region {
            if position >= end && end > start {
                let voice = self.voice;
                self.play_voice_at(voice, start);
            }
        }
    }

    /// Play one of the two voices from a given position.
    fn play_voice_at(&mut self, voice: Voice, position: f64) {
        self.playhead = position;
        self.play_voice(voice);
    }

    fn draw_pending_confirm(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_confirm.clone() else {
            return;
        };
        let mut choice: Option<Option<bool>> = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Save changes to the current StageSync project before continuing?");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(Some(true));
                    }
                    if ui.button("Don't Save").clicked() {
                        choice = Some(Some(false));
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(None);
                    }
                });
            });
        if let Some(decision) = choice {
            match decision {
                Some(true) => {
                    if self.browse_save_project(false) {
                        self.pending_confirm = None;
                        self.execute_pending_action(action);
                    }
                }
                Some(false) => {
                    self.pending_confirm = None;
                    self.execute_pending_action(action);
                }
                None => {
                    self.pending_confirm = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lay the window out at a given size and report the width the centre got.
    ///
    /// The window is laid out without a real one: egui is handed a screen the
    /// size of the window and the layout runs over it, so the result is the
    /// layout the user would see.
    fn central_width_at(width: f32, height: f32) -> f32 {
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };
        let mut app = StageSyncApp::with_settings(AppSettings::default());
        let output = context.run_ui(input, |ui| app.layout(ui));
        // There is no renderer here to hand the textures to.
        output.drop_without_applying_deltas();
        app.central_width
    }

    /// The centre of the window holds the timeline, the anchor and the results.
    ///
    /// egui gives a panel no upper bound of its own, so the export panel once
    /// grew to the width of the sentence that describes the layout and the
    /// centre — the timeline, the anchor controls and the results — was left
    /// with a fifth of the window, with the heading wrapping one letter to a
    /// line. The centre must keep a real share of the window at every size the
    /// application allows.
    #[test]
    fn the_centre_keeps_a_usable_share_of_the_window() {
        for (width, height) in [
            (960.0, 640.0),   // the smallest window the application allows
            (1400.0, 900.0),  // the default
            (2560.0, 1440.0), // a large display
        ] {
            let central = central_width_at(width, height);
            assert!(
                central >= width * 0.28,
                "at {width}x{height} the centre was only {central:.0} px wide"
            );
        }
    }

    /// Promoting a target to the reference must not move anybody's anchor.
    ///
    /// The targets and their anchors are two lists that are read by the same
    /// index, so anything that removes one of them has to remove the other.
    /// Promoting a target used to push the promoted recording's own anchor onto
    /// the recording that became a target instead — the anchor of one file
    /// ended up on another — and it left the two lists different lengths, so
    /// every later anchor was read against the wrong recording.
    #[test]
    fn promoting_a_target_to_the_reference_keeps_the_anchors_with_their_files() {
        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.reference = Some(FileEntry::new(PathBuf::from("reference.wav")));
        app.targets = vec![
            FileEntry::new(PathBuf::from("first.wav")),
            FileEntry::new(PathBuf::from("second.wav")),
        ];
        app.anchors = vec![
            AnchorFields {
                source_time: 1.0,
                reference_time: 11.0,
                active: true,
            },
            AnchorFields {
                source_time: 2.0,
                reference_time: 12.0,
                active: true,
            },
        ];

        // Promote the *second* target.
        app.make_reference(1);

        assert_eq!(
            app.reference.as_ref().map(|e| e.path.clone()),
            Some(PathBuf::from("second.wav")),
            "the wrong recording became the reference"
        );
        assert_eq!(app.targets.len(), 2, "the old reference was lost");
        assert_eq!(
            app.targets[0].path,
            PathBuf::from("first.wav"),
            "the first target moved"
        );
        assert_eq!(
            app.targets[1].path,
            PathBuf::from("reference.wav"),
            "the old reference did not become the last target"
        );

        // The two lists stay the same length, and the anchor that is left
        // belongs to the recording that is still a target.
        assert_eq!(
            app.anchors.len(),
            app.targets.len(),
            "the anchors and the targets are different lengths"
        );
        let anchor = app
            .anchor_value(0)
            .expect("the first target keeps its anchor");
        assert_eq!((anchor.source_time, anchor.reference_time), (1.0, 11.0));

        // The recording that became the reference had an anchor of its own; it
        // went with it, rather than landing on the recording that took its
        // place.
        assert!(
            app.anchor_value(1).is_none(),
            "the promoted recording's anchor was left behind on another file"
        );
    }

    /// A nudge moves the placement, and an undo takes it back.
    ///
    /// The nudge is a movement of the recording, not a change of its rate, so the
    /// rate is not part of what the nudge touches at all.
    #[test]
    fn a_nudge_moves_the_placement_and_an_undo_takes_it_back() {
        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.targets = vec![FileEntry::new(PathBuf::from("target.wav"))];

        app.nudge(0, 0.010);
        assert!(
            (app.nudges[0] - 0.010).abs() < 1e-12,
            "the nudge was {}",
            app.nudges[0]
        );

        app.nudge(0, -0.004);
        assert!(
            (app.nudges[0] - 0.006).abs() < 1e-12,
            "the second nudge was {}",
            app.nudges[0]
        );

        app.undo_nudge();
        assert!(
            (app.nudges[0] - 0.010).abs() < 1e-12,
            "the undo left {}",
            app.nudges[0]
        );

        app.undo_nudge();
        assert!(
            app.nudges[0].abs() < 1e-12,
            "the second undo left {}",
            app.nudges[0]
        );
    }

    /// Zooming has to keep the material under the cursor where it was.
    ///
    /// A zoom that moves the anchor is a zoom that takes the listener's eye away
    /// from the thing they were looking at.
    #[test]
    fn zooming_keeps_the_anchor_under_the_cursor() {
        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.view_start = 100.0;
        app.view_end = 200.0;
        let anchor = 150.0;

        app.zoom_view(1.0, anchor);
        let span = app.view_end - app.view_start;
        assert!((span - 50.0).abs() < 1e-9, "the span is {span}");
        let at_anchor = app.view_start + (anchor - app.view_start) / span * span;
        assert!(
            (at_anchor - anchor).abs() < 1e-9,
            "the anchor moved to {at_anchor}"
        );

        app.zoom_view(-1.0, anchor);
        let span = app.view_end - app.view_start;
        assert!((span - 100.0).abs() < 1e-9, "the span is {span}");
    }

    /// The view may not zoom past the ends of the recording.
    #[test]
    fn the_zoom_is_bounded() {
        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.view_start = 0.0;
        app.view_end = 100.0;

        // The smallest span is 0.05 s. The span that comes back is the
        // difference of two numbers near fifty seconds, so the last few bits
        // are lost in the subtraction; the bound is checked with a tolerance
        // that is far below anything a person could see.
        for _ in 0..40 {
            app.zoom_view(1.0, 50.0);
        }
        let span = app.view_end - app.view_start;
        assert!(span >= 0.05 - 1e-9, "the span shrank to {span}");

        for _ in 0..40 {
            app.zoom_view(-1.0, 50.0);
        }
        let span = app.view_end - app.view_start;
        assert!(span <= 24.0 * 3600.0 + 1e-9, "the span grew to {span}");
    }

    // --- The whole workflow, end to end -----------------------------------

    /// A deterministic pseudo-musical performance.
    ///
    /// The signal has no exactly repeated passage: a repeating block would make
    /// several positions of the recording genuinely alike, which would test the
    /// fixture rather than the matcher. The chord progression, the dynamics and
    /// the partials are therefore drawn from a fixed seed, so the fixture is the
    /// same on every run but never repeats itself inside the recording.
    fn performance(duration_secs: f64, rate: u32, seed: u64) -> Vec<f32> {
        let n = (duration_secs * rate as f64).round() as usize;
        let mut state = seed | 1;
        let mut next = || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545F491_4F6CDD1D)
        };
        let uniform = |next: &mut dyn FnMut() -> u64| ((next() >> 40) as f64) / (1u64 << 24) as f64;
        let mut rng = || uniform(&mut next);

        const CHORD_SECS: f64 = 2.3;
        const BEAT_SECS: f64 = 0.5;
        const SECTION_SECS: f64 = 17.0;

        let roots: Vec<f64> = (0..256).map(|_| 110.0 + 330.0 * rng()).collect();
        let weights: Vec<f64> = (0..256).map(|_| 0.35 + 0.65 * rng()).collect();
        let gains: Vec<f64> = (0..64).map(|_| 0.20 + 0.80 * rng()).collect();

        let mut out = vec![0.0f32; n];
        for (i, slot) in out.iter_mut().enumerate() {
            let t = i as f64 / rate as f64;
            let chord = (t / CHORD_SECS).floor() as usize % roots.len();
            let root = roots[chord];
            let f = root * (1.0 + 0.0007 * chord as f64);
            // Inharmonic partials: harmonically related pure tones share a
            // common period and create exact correlation aliases.
            let mut v = 0.0f64;
            for (h, amp) in [(1.0f64, 0.50), (1.503, 0.30), (2.117, 0.18)] {
                v += amp * (std::f64::consts::PI * 2.0 * f * h * t).sin();
            }
            // A slowly gliding tone gives the envelope long-term structure.
            let glide = 220.0 + 90.0 * (0.5 + 0.5 * (std::f64::consts::PI * 2.0 * t / 23.0).sin());
            v += 0.16 * (std::f64::consts::PI * 2.0 * glide * t).sin();
            let section = (t / SECTION_SECS).floor() as usize % gains.len();
            *slot = (v * weights[chord] * gains[section]) as f32;
        }

        // Percussive bursts: these give the envelope sharp, well-localised
        // features.
        let mut beat = 0usize;
        while beat as f64 * BEAT_SECS < duration_secs {
            let start = (beat as f64 * BEAT_SECS * rate as f64).round() as usize;
            if start >= n {
                break;
            }
            let len = ((0.06 * rate as f64).round() as usize).min(n - start);
            for j in 0..len {
                let env = (1.0 - j as f32 / len as f32).powi(2);
                out[start + j] += env * 0.42 * ((rng() * 2.0 - 1.0) as f32);
            }
            beat += 1;
        }

        // A low noise floor, so the material is never digitally silent.
        for v in out.iter_mut() {
            *v += 0.0015 * ((rng() * 2.0 - 1.0) as f32);
        }

        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-9);
        for v in out.iter_mut() {
            *v = *v / peak * 0.7;
        }
        out
    }

    /// A recording of the same performance, starting at an exact place in the
    /// reference and running at a slightly different rate.
    fn make_source(
        reference: &[f32],
        ref_rate: u32,
        out_rate: u32,
        scale: f64,
        offset: f64,
    ) -> Vec<f32> {
        let duration = (reference.len() as f64 / ref_rate as f64 - offset) / scale;
        let n = (duration * out_rate as f64).round() as usize;
        let mut out = vec![0.0f32; n];
        for (j, slot) in out.iter_mut().enumerate() {
            let source_time = j as f64 / out_rate as f64;
            let pos = ((scale * source_time + offset) * ref_rate as f64).round() as i64;
            if pos < 0 || pos as usize >= reference.len() {
                continue;
            }
            *slot = reference[pos as usize];
        }
        out
    }

    /// Write a mono WAV file.
    fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) {
        let mut writer = stagesync_core::audio::WavWriter::create(
            path,
            rate,
            1,
            stagesync_core::audio::WavFormat::Float32,
        )
        .expect("create WAV writer");
        writer
            .write_planar(std::slice::from_ref(&samples))
            .expect("write samples");
        writer.finalize().expect("finalise WAV");
    }

    /// The size and the modification time of a file, as a cheap fingerprint.
    fn fingerprint(path: &std::path::Path) -> (u64, std::time::SystemTime) {
        let meta = std::fs::metadata(path).expect("the file is there");
        (meta.len(), meta.modified().expect("the file has a time"))
    }

    /// Wait for a background job to report back.
    fn wait_for<T>(app: &mut StageSyncApp, done: impl Fn(&StageSyncApp) -> Option<T>) -> Option<T> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        loop {
            app.drain_events();
            if let Some(value) = done(app) {
                return Some(value);
            }
            if std::time::Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// The whole workflow, from an imported recording to an exported correction.
    ///
    /// This drives the application the way the window does: the files are put on
    /// the list, the analysis runs on its own thread, the result comes back over
    /// the channel, and the export writes the corrected audio and the artefacts.
    /// The point is not that each step works on its own — the core has its own
    /// tests for that — but that the window's own path from the file list to the
    /// finished export works, and that the recordings are left untouched.
    #[test]
    fn the_window_aligns_a_recording_and_exports_the_correction() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        // The export finishes by writing the settings, and the settings must not
        // be those of whoever is running the test.
        let _settings = SettingsGuard::new(&dir);
        let reference_rate = 48_000u32;
        let reference_secs = 40.0f64;
        let signal = performance(reference_secs, reference_rate, 11);
        let reference_path = dir.path().join("reference.wav");
        write_wav(&reference_path, &signal, reference_rate);

        // The target: the same performance, starting three seconds into the
        // reference, recorded at another rate and running slightly fast.
        let scale = 1.0 + 30e-6;
        let offset = 3.0f64;
        let target_rate = 44_100u32;
        let target = make_source(&signal, reference_rate, target_rate, scale, offset);
        let target_path = dir.path().join("target.wav");
        write_wav(&target_path, &target, target_rate);

        let reference_before = fingerprint(&reference_path);
        let target_before = fingerprint(&target_path);

        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.open_files(vec![reference_path.clone(), target_path.clone()]);
        app.analyse_on_start();

        let report =
            wait_for(&mut app, |app| app.report.clone()).expect("the analysis finished in time");
        assert!(
            app.last_error.is_none(),
            "the analysis reported an error: {:?}",
            app.last_error
        );
        let alignment = &report.sources[0];
        let model = alignment
            .model()
            .expect("the recording was aligned to the reference");
        assert!(
            (model.offset - offset).abs() < 0.05,
            "the offset was {} s and the truth is {offset} s",
            model.offset
        );
        assert!(
            (model.scale - scale).abs() < 200e-6,
            "the rate was {} and the truth is {scale}",
            model.scale
        );

        // The nudge moves the placement and nothing else.
        app.selection = Some(Selection::Target(0));
        app.nudge(0, 0.010);
        assert!((app.nudges[0] - 0.010).abs() < 1e-12);
        app.undo_nudge();
        assert!(app.nudges[0].abs() < 1e-12);

        // The export writes the corrected audio and the artefacts.
        let out = dir.path().join("export");
        app.settings.dir = out.display().to_string();
        app.start_export();
        let outcome = wait_for(&mut app, |app| {
            app.export_outcome
                .as_ref()
                .map(|outcome| (outcome.files.clone(), outcome.corrected.clone()))
        })
        .expect("the export finished in time");
        assert!(
            app.last_error.is_none(),
            "the export reported an error: {:?}",
            app.last_error
        );
        let (files, corrected_files) = outcome;
        assert!(!files.is_empty(), "the export wrote nothing");
        for file in &files {
            let meta = std::fs::metadata(file).expect("the file is there");
            assert!(meta.len() > 0, "{} is empty", file.display());
        }
        let (_, corrected) = corrected_files
            .first()
            .expect("the corrected audio was written");
        assert!(corrected.is_file(), "{} is missing", corrected.display());

        // The recordings are read-only: the export never touches them.
        assert_eq!(
            reference_before,
            fingerprint(&reference_path),
            "the reference was changed by the export"
        );
        assert_eq!(
            target_before,
            fingerprint(&target_path),
            "the target was changed by the export"
        );
    }

    // --- The buttons of the window ----------------------------------------

    /// The lock that keeps the two tests that touch the settings apart.
    ///
    /// The environment variable that says where the settings live is
    /// process-wide, so only one test may point it somewhere else at a time.
    static SETTINGS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A guard that points the settings at a temporary folder.
    ///
    /// The window writes its settings when a job finishes. The tests run the
    /// window's own code, so without this the settings of whoever is running
    /// the tests would be overwritten with the test's temporary paths — which
    /// is exactly what happened once.
    struct SettingsGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<String>,
    }

    impl SettingsGuard {
        /// Point the settings at a folder inside the given temporary directory.
        fn new(dir: &tempfile::TempDir) -> Self {
            let lock = SETTINGS_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = std::env::var(crate::platform::CONFIG_DIR_ENV).ok();
            std::env::set_var(crate::platform::CONFIG_DIR_ENV, dir.path());
            SettingsGuard {
                _lock: lock,
                previous,
            }
        }
    }

    impl Drop for SettingsGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(crate::platform::CONFIG_DIR_ENV, value),
                None => std::env::remove_var(crate::platform::CONFIG_DIR_ENV),
            }
        }
    }

    /// The buttons of the window do what they say.
    ///
    /// The test drives the real widgets: it finds the button by its label and
    /// clicks it, exactly as a mouse would. A button that is only painted — that
    /// does nothing when it is pressed — fails this test.
    ///
    /// The settings are pointed at a temporary folder for the duration of the
    /// test, so that the window cannot write over the settings of whoever is
    /// running it.
    #[test]
    fn the_buttons_of_the_window_do_what_they_say() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let _settings = SettingsGuard::new(&dir);

        the_buttons_of_the_window_do_what_they_say_inner(&dir)
            .expect("the buttons did not do what they say");
    }

    /// The body of the button test, kept apart so that the environment variable
    /// is always put back.
    fn the_buttons_of_the_window_do_what_they_say_inner(dir: &tempfile::TempDir) -> Option<()> {
        use egui_kittest::kittest::Queryable as _;

        // A reference and a target, so that the analysis has something to do.
        let reference_rate = 48_000u32;
        let signal = performance(30.0, reference_rate, 3);
        let reference_path = dir.path().join("reference.wav");
        write_wav(&reference_path, &signal, reference_rate);
        let target = make_source(&signal, reference_rate, 44_100, 1.0, 2.0);
        let target_path = dir.path().join("target.wav");
        write_wav(&target_path, &target, 44_100);

        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            // The window keeps asking for a repaint while it is drawing the
            // waveforms and running a job, so the harness is allowed as many
            // steps as the test needs.
            .with_max_steps(10_000)
            .build_eframe(|_cc| StageSyncApp::with_settings(AppSettings::default()));
        harness.run();

        // The files are put on the list, as if they had been dropped onto the
        // window.
        harness
            .state_mut()
            .open_files(vec![reference_path.clone(), target_path.clone()]);
        harness.run();

        // The Analyze button starts the analysis.
        harness
            .query_by_label("Analyze")
            .expect("the window has an Analyze button")
            .click();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        while harness.state().report.is_none() && std::time::Instant::now() < deadline {
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        harness.state().report.as_ref()?;
        assert!(
            harness.state().last_error.is_none(),
            "the analysis reported an error: {:?}",
            harness.state().last_error
        );
        harness.run();

        // The result is on the screen.
        assert!(
            harness
                .query_by_label_contains("Analysis finished.")
                .is_some(),
            "the window did not say that the analysis had finished"
        );

        // The Fit button puts the whole show on the screen.
        let duration = harness
            .state()
            .report
            .as_ref()
            .expect("the report is there")
            .timeline_duration;
        harness
            .query_by_label("Fit")
            .expect("the window has a Fit button")
            .click();
        harness.run();
        assert!(
            harness.state().view_start.abs() < 1e-6,
            "the view starts at {} s",
            harness.state().view_start
        );
        assert!(
            (harness.state().view_end - duration).abs() < 0.5,
            "the view ends at {} s and the show is {duration} s long",
            harness.state().view_end
        );

        // The zoom-out button widens the view.
        let fitted = harness.state().view_end - harness.state().view_start;
        harness
            .query_by_label("-")
            .expect("the window has a zoom-out button")
            .click();
        harness.run();
        let zoomed = harness.state().view_end - harness.state().view_start;
        assert!(
            zoomed > fitted,
            "the zoom-out button did not widen the view: {fitted} s became {zoomed} s"
        );

        // The Play button either starts the sound or says why it cannot. A
        // machine without an audio device is not a failure of the button.
        harness
            .query_by_label("Play")
            .expect("the window has a Play button")
            .click();
        harness.run();
        let app = harness.state();
        let playing = app.player.is_playing();
        let complained = app.last_error.is_some();
        assert!(playing || complained, "the Play button did nothing at all");

        Some(())
    }

    #[test]
    fn remove_target_and_make_reference_keep_nudges_and_mixer_aligned() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let paths: Vec<PathBuf> = ["ref.wav", "t0.wav", "t1.wav", "t2.wav"]
            .iter()
            .map(|name| {
                let p = dir.path().join(name);
                std::fs::write(&p, b"").unwrap();
                p
            })
            .collect();

        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.open_files(paths.clone());
        assert_eq!(app.targets.len(), 3);
        app.mixer.sync_to(3);
        app.mixer.tracks[0].gain = 0.25;
        app.mixer.tracks[1].gain = 0.50;
        app.mixer.tracks[2].gain = 0.75;
        app.nudges = vec![0.01, 0.02, 0.03];
        app.nudge_history = vec![(0, 0.0), (1, 0.0), (2, 0.0)];

        app.remove_target(1);
        assert_eq!(app.targets.len(), 2);
        assert_eq!(app.nudges, vec![0.01, 0.03]);
        assert_eq!(app.nudge_history, vec![(0, 0.0), (1, 0.0)]);
        assert_eq!(app.mixer.tracks.len(), 2);
        assert!((app.mixer.tracks[0].gain - 0.25).abs() < 1e-6);
        assert!((app.mixer.tracks[1].gain - 0.75).abs() < 1e-6);
        assert_eq!(app.mixer.tracks[1].source_index, 1);

        app.make_reference(0);
        assert_eq!(app.reference.as_ref().unwrap().path, paths[1]);
        assert_eq!(app.targets.len(), 2);
        assert_eq!(app.targets[0].path, paths[3]);
        assert_eq!(app.targets[1].path, paths[0]);
        assert_eq!(app.nudges, vec![0.03, 0.0]);
        assert!((app.mixer.tracks[0].gain - 0.75).abs() < 1e-6);
        assert!((app.mixer.tracks[1].gain - 1.0).abs() < 1e-6);
    }

    #[test]
    fn p1_04_stale_export_verification_and_probe_events_are_ignored() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let _settings = SettingsGuard::new(&dir);
        let p_ref = dir.path().join("ref.wav");
        let p_tgt = dir.path().join("tgt.wav");
        std::fs::write(&p_ref, b"REF").unwrap();
        std::fs::write(&p_tgt, b"TGT").unwrap();

        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.open_files(vec![p_ref.clone(), p_tgt.clone()]);
        let old_scope = app.next_scope();

        // Simulate switching to a new blank session (or another project).
        app.new_project();
        let wrong_job = JobScope {
            session_gen: app.session_gen,
            job_id: old_scope.job_id + 999,
        };

        // Inject stale events from old session_gen AND wrong job_id in current session_gen.
        let _ = app.tx.send(ScopedJobEvent {
            scope: old_scope,
            event: JobEvent::ExportFinished(Box::new(ExportOutcome {
                files: vec![dir.path().join("stale.wav")],
                corrected: vec![],
            })),
        });
        let _ = app.tx.send(ScopedJobEvent {
            scope: old_scope,
            event: JobEvent::VerificationFailed {
                source_index: 0,
                message: "stale verify".to_string(),
            },
        });
        let _ = app.tx.send(ScopedJobEvent {
            scope: wrong_job,
            event: JobEvent::VerificationFailed {
                source_index: 0,
                message: "wrong job verify".to_string(),
            },
        });
        let _ = app.tx.send(ScopedJobEvent {
            scope: wrong_job,
            event: JobEvent::Probed {
                path: p_tgt.clone(),
                meta: None,
                error: Some("stale probe error".to_string()),
            },
        });

        app.drain_events();

        assert!(app.export_outcome.is_none());
        assert!(app.last_error.is_none());
        assert!(!app.is_dirty());
        assert!(!app.verifying);
    }

    #[test]
    fn p1_06_pending_nudge_exports_in_audio_and_plan_and_unapplied_anchor_blocks_export() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let _settings = SettingsGuard::new(&dir);
        let rate = 8_000u32;
        let signal = performance(25.0, rate, 7);
        let ref_path = dir.path().join("ref.wav");
        write_wav(&ref_path, &signal, rate);
        // Target is delayed by +1.0 s relative to reference.
        let src_offset = 1.0;
        let tgt_samples: Vec<f32> = signal[rate as usize..].to_vec();
        let tgt_path = dir.path().join("tgt.wav");
        write_wav(&tgt_path, &tgt_samples, rate);

        let config = stagesync_core::pipeline::AnalysisConfig {
            compute_log_mel: false,
            measure_waveform_drift: false,
            ..stagesync_core::pipeline::AnalysisConfig::default()
        };
        let mut noop = |_: stagesync_core::pipeline::Progress| {};
        let report = stagesync_core::pipeline::analyze(
            &ref_path,
            std::slice::from_ref(&tgt_path),
            &config,
            &mut noop,
        )
        .expect("analyze");
        let orig_offset = report.sources[0].model().expect("matched").offset;

        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.open_files(vec![ref_path.clone(), tgt_path.clone()]);
        app.report = Some(report);

        // Apply a +0.500 s pending nudge WITHOUT running "Apply and re-check", and export.
        let nudge_secs = 0.500;
        app.nudge(0, nudge_secs);
        let out_dir = dir.path().join("out_nudge");
        app.settings.dir = out_dir.display().to_string();
        app.settings.layout = Layout::ReferenceTimelinePerSource;
        app.start_export();
        let outcome = wait_for(&mut app, |a| {
            a.export_outcome.as_ref().map(|o| o.files.clone())
        })
        .expect("export finishes");
        assert!(
            app.last_error.is_none(),
            "export failed: {:?}",
            app.last_error
        );

        let plan_json = std::fs::read_to_string(out_dir.join("plan.json")).expect("read plan.json");
        let exported_report: AlignmentReport =
            serde_json::from_str(&plan_json).expect("parse plan.json");
        let exported_offset = exported_report.sources[0]
            .model()
            .expect("exported model")
            .offset;
        assert!(
            (exported_offset - (orig_offset + nudge_secs)).abs() < 1e-4,
            "pending nudge was dropped on export: got {exported_offset}, expected {}",
            orig_offset + nudge_secs
        );
        // Verify rendered timeline WAV is silent before orig_offset + nudge_secs and active after.
        let timeline_wav = out_dir.join("tgt_timeline.wav");
        let mut reader = stagesync_core::audio::WavReader::open(&timeline_wav).expect("open wav");
        let pre_nudge = reader
            .read_frames((orig_offset * rate as f64) as u64 + 400, 1600)
            .expect("read pre-nudge frames");
        let pre_peak = pre_nudge[0].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            pre_peak < 1e-3,
            "expected silence in nudged gap [{orig_offset}..{}], got peak {pre_peak}",
            orig_offset + nudge_secs
        );
        let post_nudge = reader
            .read_frames(
                ((orig_offset + nudge_secs) * rate as f64) as u64 + 800,
                3200,
            )
            .expect("read post-nudge frames");
        let post_peak = post_nudge[0].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            post_peak > 0.05,
            "expected shifted audio after orig_offset + nudge_secs, got peak {post_peak}"
        );

        // Unapplied manual anchor edit must block export instead of silently exporting stale alignment.
        app.anchors[0].active = true;
        app.anchors[0].source_time = 2.0;
        app.anchors[0].reference_time = 5.0;
        let blocked_dir = dir.path().join("blocked_out");
        app.settings.dir = blocked_dir.display().to_string();
        app.export_outcome = None;
        app.start_export();
        assert!(
            app.running.is_none(),
            "export started with unapplied anchor"
        );
        assert!(
            app.last_error
                .as_deref()
                .is_some_and(|e| e.contains("Apply anchor")),
            "expected Apply anchor refusal error, got {:?}",
            app.last_error
        );
        app.anchors[0].active = false;

        // Staging Apply & Cancel preserves pending nudge and prior anchor intact.
        app.apply_nudge(0);
        if let Some(job) = app.running.as_ref() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.running.is_some() && std::time::Instant::now() < deadline {
            app.drain_events();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            (app.nudges[0] - nudge_secs).abs() < 1e-12,
            "cancelled apply_nudge wiped out pending nudge: {}",
            app.nudges[0]
        );
        assert!(
            !app.anchors[0].active,
            "cancelled apply_nudge mutated anchor prematurely"
        );
        let _ = outcome;
        let _ = src_offset;
    }

    #[test]
    fn p2_02_app_mix_preview_matches_exported_mixdown_across_loudness_pan_and_protection() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let _settings = SettingsGuard::new(&dir);
        let rate = 8_000u32;

        // Reference + 2 sources at different levels (one quiet 0.125, one loud 0.9) and different rates/pans.
        let n = rate as usize;
        let ref_samples = vec![0.0f32; n];
        let s1: Vec<f32> = (0..n)
            .map(|i| 0.9 * (2.0 * std::f32::consts::PI * 190.0 * i as f32 / rate as f32).sin())
            .collect();
        let s2: Vec<f32> = (0..n)
            .map(|i| 0.125 * (2.0 * std::f32::consts::PI * 310.0 * i as f32 / rate as f32).sin())
            .collect();
        let ref_path = dir.path().join("ref.wav");
        let p1 = dir.path().join("t1.wav");
        let p2 = dir.path().join("t2.wav");
        write_wav(&ref_path, &ref_samples, rate);
        write_wav(&p1, &s1, rate);
        write_wav(&p2, &s2, rate);

        let meta_ref = stagesync_core::audio::AudioDecoder::open(&ref_path)
            .unwrap()
            .meta()
            .clone();
        let meta1 = stagesync_core::audio::AudioDecoder::open(&p1)
            .unwrap()
            .meta()
            .clone();
        let meta2 = stagesync_core::audio::AudioDecoder::open(&p2)
            .unwrap()
            .meta()
            .clone();

        let m = stagesync_core::matching::LinearModel::new(1.0, 0.0);
        let make_align = |meta: SourceMeta| stagesync_core::matching::SourceAlignment {
            source: meta,
            map: stagesync_core::matching::TimeMap::from_linear(m, 1.0),
            coarse: None,
            fine_measured: 1,
            fine_skipped: 0,
            fine_skip_reasons: Default::default(),
            direct_model: Some(m),
            calibrated_model: Some(m),
            drift: None,
            points: vec![],
            segments: vec![],
            coverage: stagesync_core::matching::Coverage {
                covered_seconds: 1.0,
                low_confidence_seconds: 0.0,
                uncovered_seconds: 0.0,
                fraction: 1.0,
            },
            holdout: None,
            override_state: None,
            warnings: vec![],
        };
        let report = AlignmentReport {
            reference: meta_ref,
            sources: vec![make_align(meta1), make_align(meta2)],
            timeline: vec![],
            timeline_duration: 1.0,
        };

        let mut app = StageSyncApp::with_settings(AppSettings::default());
        app.open_files(vec![ref_path, p1, p2]);
        app.report = Some(report.clone());
        app.mixer.mode = stagesync_core::mixer::MonitorMode::Both;
        app.mixer.master_gain = 1.25;
        app.mixer.tracks[0].pan = -0.5;
        app.mixer.tracks[1].pan = 0.6;
        app.settings.sample_rate = Some(rate);
        app.settings.format = stagesync_core::audio::WavFormat::Float32;
        app.settings.loudness = stagesync_core::mixer::LoudnessMatch::MatchToLoudest;

        for (tag, protection) in [
            ("none_hot", stagesync_core::mixer::ClippingProtection::None),
            (
                "soft_clip",
                stagesync_core::mixer::ClippingProtection::SoftClip { threshold: 0.9 },
            ),
            (
                "auto_gain",
                stagesync_core::mixer::ClippingProtection::AutoGain { target_dbfs: -1.0 },
            ),
        ] {
            app.settings.protection = protection;
            app.autogain_cache = None;
            let (members, autogain_trim) = app.resolve_preview_mix(true);
            let preview = crate::player::collect_preview_mix_frames(
                &members,
                app.mixer.master_gain,
                app.settings.protection,
                autogain_trim,
                0.0,
                rate,
                n,
            )
            .expect("preview mix frames");

            let out_wav = dir.path().join(format!("mix_{tag}.wav"));
            let mix_settings = app.mixer.settings();
            let mix_opts = stagesync_core::mixer::MixOptions {
                sample_rate: Some(rate),
                channels: 2,
                format: stagesync_core::audio::WavFormat::Float32,
                protection,
                loudness: app.settings.loudness,
                container: stagesync_core::mixer::ContainerChoice::Auto,
                block_frames: 4096,
                ..Default::default()
            };
            let res =
                stagesync_core::mixer::render_mix(&report, &mix_settings, &mix_opts, &out_wav)
                    .expect("render_mix");
            let mut reader = stagesync_core::audio::WavReader::open(&out_wav).expect("open mix");
            let exported = reader.read_frames(0, n).expect("read mix frames");

            let mut max_diff = 0.0f32;
            let mut max_peak = 0.0f32;
            for i in 128..(n - 128) {
                let dl = (preview[i][0] - exported[0][i]).abs();
                let dr = (preview[i][1] - exported[1][i]).abs();
                max_diff = max_diff.max(dl).max(dr);
                max_peak = max_peak.max(preview[i][0].abs()).max(preview[i][1].abs());
            }
            assert!(
                max_diff < 1e-4,
                "[{tag}] preview vs export max sample divergence {max_diff} exceeded 1e-4"
            );
            if matches!(protection, stagesync_core::mixer::ClippingProtection::None) {
                assert!(
                    max_peak > 1.0 && res.peak > 1.0,
                    "[{tag}] expected unclipped Float32 peak > 1.0, got preview={max_peak}, export={}",
                    res.peak
                );
            } else {
                assert!(
                    max_peak <= 1.0 + 1e-4,
                    "[{tag}] clipping protection exceeded 1.0: preview={max_peak}"
                );
            }
        }
    }
}
