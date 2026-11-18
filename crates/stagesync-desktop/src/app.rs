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
use crate::jobs::{self, ExportOutcome, JobEvent};
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

/// A background job that is currently running.
struct RunningJob {
    kind: JobKind,
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
    verifying: bool,
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
    tx: Sender<JobEvent>,
    rx: Receiver<JobEvent>,
    /// Which voice the transport is set to: the reference or the corrected target.
    voice: Voice,
    /// The player: the reference, the corrected recording, and the switch between
    /// them.
    player: Player,
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
            verifying: false,
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

    /// Apply everything the background workers have reported since the last frame.
    fn drain_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                JobEvent::Progress(progress) => self.progress = Some(progress),
                JobEvent::Probed { path, meta, error } => self.apply_probe(path, meta, error),
                JobEvent::AnalysisFinished(report) => {
                    self.running = None;
                    self.progress = None;
                    // The report describes the recordings that were in the list
                    // when the analysis started. If the list has changed since,
                    // its indices no longer line up with the file list, and
                    // showing it would put one recording's figures next to
                    // another recording's name.
                    if report.sources.len() != self.targets.len() {
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
                    if self.view_start == 0.0 && (self.view_end - 600.0).abs() < 1e-6 {
                        self.view_end = report.timeline_duration.max(1.0);
                    }
                    self.report = Some(*report);
                    self.mark_dirty();
                    self.last_error = None;
                    self.notice = Some("Analysis finished.".to_string());
                    self.push_log("analysis finished".to_string());
                    self.persist_settings();
                }
                JobEvent::AnalysisFailed(message) => {
                    self.running = None;
                    self.progress = None;
                    self.last_error = Some(format!("The analysis failed: {message}"));
                    self.push_log(format!("analysis failed: {message}"));
                }
                JobEvent::AnalysisCancelled => {
                    self.running = None;
                    self.progress = None;
                    self.last_error = None;
                    self.notice = Some(
                        "The analysis was cancelled. The previous result was kept.".to_string(),
                    );
                    self.push_log("analysis cancelled".to_string());
                }
                JobEvent::ExportFinished(outcome) => {
                    self.running = None;
                    self.progress = None;
                    self.export_outcome = Some(*outcome);
                    self.last_error = None;
                    self.notice = Some("Export finished.".to_string());
                    self.push_log("export finished".to_string());
                    self.persist_settings();
                }
                JobEvent::ExportFailed(message) => {
                    self.running = None;
                    self.progress = None;
                    self.last_error = Some(format!("The export failed: {message}"));
                    self.push_log(format!("export failed: {message}"));
                }
                JobEvent::ExportCancelled => {
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
                    self.verifying = false;
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
                    self.verifying = false;
                    self.last_error = Some(format!("The verification failed: {message}"));
                    self.push_log(format!(
                        "verification of source {source_index} failed: {message}"
                    ));
                }
                JobEvent::PeaksReady { path, cache } => {
                    self.peaks_pending = self.peaks_pending.saturating_sub(1);
                    self.peaks.insert(path, Arc::new(*cache));
                }
                JobEvent::PeaksFailed { path, message } => {
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

    /// Store the result of a background probe.
    fn apply_probe(&mut self, path: PathBuf, meta: Option<SourceMeta>, error: Option<String>) {
        let mut invalidated_by_modification = false;
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
            invalidated_by_modification =
                check_entry(entry, true, &mut view_end, report_none, view_start);
            self.view_end = view_end;
        } else if let Some(entry) = self.targets.iter_mut().find(|entry| entry.path == path) {
            invalidated_by_modification =
                check_entry(entry, false, &mut view_end, report_none, view_start);
        }

        if invalidated_by_modification && self.report.is_some() {
            self.report = None;
            self.export_outcome = None;
            self.notice = Some(
                "Cached alignment was cleared because a recording changed on disk.".to_string(),
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
            jobs::spawn_probe(self.tx.clone(), path.clone());
            // The waveform is built at the same time: it needs the whole recording,
            // and the user wants to see it as soon as it is there.
            jobs::spawn_peaks(self.tx.clone(), path);
            self.peaks_pending += 1;
        }
        if changed && announce {
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
        self.reference = None;
        self.report = None;
        self.export_outcome = None;
        self.selection = None;
        self.nudges.clear();
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

        self.reference = loaded.reference.as_ref().map(entry_from_resolved);
        self.targets = loaded.targets.iter().map(entry_from_resolved).collect();
        self.anchors = proj
            .targets
            .iter()
            .map(|t| AnchorFields::from(t.anchor))
            .collect();
        self.nudges = proj.targets.iter().map(|t| t.pending_nudge_secs).collect();

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
        if let Some(ref_entry) = &self.reference {
            if !ref_entry.missing {
                jobs::spawn_probe(self.tx.clone(), ref_entry.path.clone());
                jobs::spawn_peaks(self.tx.clone(), ref_entry.path.clone());
                self.peaks_pending += 1;
            }
        }
        for tgt in &self.targets {
            if !tgt.missing {
                jobs::spawn_probe(self.tx.clone(), tgt.path.clone());
                jobs::spawn_peaks(self.tx.clone(), tgt.path.clone());
                self.peaks_pending += 1;
            }
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
        let Some(ref mut entry) = self.reference else {
            return;
        };
        let changed = relocate_entry(entry, new_path.clone());
        if changed && self.report.is_some() {
            self.report = None;
            self.export_outcome = None;
        }
        jobs::spawn_probe(self.tx.clone(), new_path.clone());
        jobs::spawn_peaks(self.tx.clone(), new_path);
        self.peaks_pending += 1;
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
        if changed && self.report.is_some() {
            self.report = None;
            self.export_outcome = None;
        }
        jobs::spawn_probe(self.tx.clone(), new_path.clone());
        jobs::spawn_peaks(self.tx.clone(), new_path);
        self.peaks_pending += 1;
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
                let anchor = self.anchor_value(index)?;
                Some((self.anchor_key(entry), anchor))
            })
            .collect();
        if !config.manual_anchors.is_empty() {
            // The automatic lock is not to be trusted, so widen the fine search.
            config.fine.search_secs = 10.0;
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(RunningJob {
            kind: JobKind::Analysis,
            cancel: cancel.clone(),
        });
        self.progress = None;
        self.last_error = None;
        self.notice = None;
        self.push_log("analysis started".to_string());
        jobs::spawn_analysis(self.tx.clone(), reference, sources, config, cancel);
    }

    /// Start the export of the corrected audio and the artefacts.
    fn start_export(&mut self) {
        if self.running.is_some() {
            self.last_error = Some(
                "A job is already running. Wait for it to finish, or cancel it first.".to_string(),
            );
            return;
        }
        let Some(report) = self.report.clone() else {
            self.last_error = Some("Run the analysis before exporting.".to_string());
            return;
        };
        if self.settings.dir.trim().is_empty() {
            self.last_error = Some("Choose an export folder first.".to_string());
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(RunningJob {
            kind: JobKind::Export,
            cancel: cancel.clone(),
        });
        self.progress = None;
        self.last_error = None;
        self.notice = None;
        self.push_log(format!("export started into {}", self.settings.dir));
        // The mixer's settings travel with the export, so that the file the window
        // writes is the mix the user can hear.
        self.settings.mix = self.mixer.settings();
        jobs::spawn_export(self.tx.clone(), report, self.settings.clone(), cancel);
    }

    /// Verify a rendered correction against the reference, independently of the fit.
    fn start_verification(&mut self, index: usize) {
        let model_and_duration = self.report.as_ref().and_then(|report| {
            let alignment = report.sources.get(index)?;
            let model = alignment.model()?;
            Some((model, alignment.source.duration_secs))
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

        self.verifying = true;
        self.last_error = None;
        self.push_log(format!("verifying source {index}"));
        jobs::spawn_verification(
            self.tx.clone(),
            index,
            corrected,
            reference,
            model,
            duration,
        );
    }

    // --- Panels ------------------------------------------------------------

    /// The toolbar with the run controls and the progress display.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let busy = self.running.is_some() || self.verifying;
        let can_analyze = self.reference.is_some() && !self.targets.is_empty() && !busy;
        let mut start = false;
        let mut cancel = false;
        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Project").clicked() {
                    ui.close();
                    self.request_action(PendingAction::NewProject);
                }
                if ui.button("Open Project…").clicked() {
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
                if ui.button("Save Project").clicked() {
                    ui.close();
                    self.browse_save_project(false);
                }
                if ui.button("Save Project As…").clicked() {
                    ui.close();
                    self.browse_save_project(true);
                }
            });
            let proj_label = match &self.project_path {
                Some(p) => p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled".to_string()),
                None => "Untitled".to_string(),
            };
            ui.label(if self.dirty {
                format!("{proj_label}*")
            } else {
                proj_label
            });
            ui.separator();
            start = ui
                .add_enabled(can_analyze, egui::Button::new("Analyze"))
                .clicked();
            if self.running.is_some() {
                let label = match self.running.as_ref().map(|job| job.kind) {
                    Some(JobKind::Export) => "Cancel (the file being written cannot be stopped)",
                    _ => "Cancel",
                };
                cancel = ui.button(label).clicked();
            } else if self.verifying {
                ui.label("Verifying…");
            }
            ui.separator();
            changed |= ui
                .checkbox(&mut self.use_spectral_check, "Spectral cross-check")
                .on_hover_text(
                    "Run the independent log-mel cross-check while analysing. It is slower and \
                     gives a second opinion on every match; turn it off if it is not usable.",
                )
                .changed();
            changed |= ui
                .checkbox(&mut self.measure_waveform_drift, "Waveform drift check")
                .on_hover_text(
                    "Measure the drift on the raw waveform as an independent second opinion, at \
                     positions the fit did not use. It costs an extra pass over the audio and it \
                     is the only way to tell a measured drift from a guessed one.",
                )
                .changed();
            changed |= ui
                .checkbox(&mut self.force_offset_only, "Offset only")
                .on_hover_text(
                    "Correct the position but leave the rate alone. Use this when the recordings \
                     must not be resampled.",
                )
                .changed();
            ui.separator();
            match &self.progress {
                Some(progress) => {
                    ui.label(progress.label.clone());
                    ui.add(egui::ProgressBar::new(
                        progress.fraction.clamp(0.0, 1.0) as f32
                    ));
                    ui.label(progress.message.clone());
                }
                None => {
                    if self.verifying {
                        ui.label("Verifying the rendered correction…");
                    } else if self.peaks_pending > 0 {
                        ui.label(format!(
                            "Drawing the waveforms… ({} left)",
                            self.peaks_pending
                        ));
                    } else {
                        // The state of the session, in one line: what is loaded and
                        // what has been worked out about it.
                        let mut info = format!(
                            "{} recording(s), {} waveform(s)",
                            self.reference.is_some() as usize + self.targets.len(),
                            self.peaks.len()
                        );
                        if let Some(report) = self.report.as_ref() {
                            info.push_str(&format!(
                                " · aligned in {:.1} s",
                                report
                                    .timeline
                                    .last()
                                    .map_or(0.0, |span| span.reference_end)
                            ));
                            let low = report
                                .timeline
                                .iter()
                                .filter(|span| span.kind == SegmentKind::LowConfidence)
                                .count();
                            let gaps = report
                                .timeline
                                .iter()
                                .filter(|span| span.kind == SegmentKind::UncoveredReference)
                                .count();
                            info.push_str(&format!(" · {gaps} gap(s), {low} uncertain"));
                        }
                        ui.label(info);
                    }
                }
            }
        });

        if changed {
            self.mark_dirty();
            self.persist_settings();
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

    /// The file list.
    fn file_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Files");
        wrapped(
            ui,
            "Drop audio files or a folder onto the window, or use Add files\u{2026}",
        );
        ui.horizontal(|ui| {
            if ui.button("Add files…").clicked() {
                self.browse_for_files();
            }
            if ui
                .add_enabled(
                    self.reference.is_some() && self.running.is_none(),
                    egui::Button::new("Clear reference"),
                )
                .clicked()
            {
                self.clear_reference();
            }
        });
        ui.separator();

        ui.strong("Reference");
        if let Some(entry) = self.reference.clone() {
            let selected = self.selection == Some(Selection::Reference);
            if ui.selectable_label(selected, entry.name()).clicked() {
                self.selection = Some(Selection::Reference);
            }
            ui.monospace(platform::elide_path(&entry.path, PATH_CHARS));
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
                    if entry.error.is_none() {
                        ui.label("reading…");
                    }
                }
            }
            if let Some(error) = &entry.error {
                ui.colored_label(ui.visuals().error_fg_color, format!("cannot read: {error}"));
            }
            if entry.missing {
                ui.horizontal(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, "Missing file");
                    if ui.button("Relocate…").clicked() {
                        if let Some(new_path) =
                            self.browse_relocate_file("Locate missing reference recording")
                        {
                            self.relocate_reference(new_path);
                        }
                    }
                });
            } else if let Some(warn) = &entry.modified_warning {
                ui.colored_label(ui.visuals().warn_fg_color, warn.as_str());
            }
        } else {
            ui.label("none — the first file you add becomes the reference");
        }
        ui.separator();

        ui.strong("Targets");
        if self.targets.is_empty() {
            ui.label("none — add the recordings you want to align");
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
            ui.horizontal(|ui| {
                let selected = self.selection == Some(Selection::Target(index));
                if ui.selectable_label(selected, entry.name()).clicked() {
                    select = Some(Selection::Target(index));
                }
                if ui
                    .add_enabled(self.running.is_none(), egui::Button::new("Make reference"))
                    .clicked()
                {
                    promote = Some(index);
                }
                if ui
                    // The multiplication sign, not the heavy ballot x: the font
                    // that egui ships has no glyph for U+2716, and a button
                    // without a glyph is an empty box.
                    .add_enabled(self.running.is_none(), egui::Button::new("×"))
                    .clicked()
                {
                    remove = Some(index);
                }
            });
            ui.monospace(platform::elide_path(&entry.path, PATH_CHARS));
            match &entry.meta {
                Some(meta) => {
                    ui.label(format!(
                        "{} Hz · {} ch · {}",
                        meta.sample_rate,
                        meta.channels,
                        format_duration(meta.duration_secs)
                    ));
                }
                None => {
                    if entry.error.is_none() {
                        ui.label("reading…");
                    }
                }
            }
            if let Some(error) = &entry.error {
                ui.colored_label(ui.visuals().error_fg_color, format!("cannot read: {error}"));
            }
            if entry.missing {
                ui.horizontal(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, "Missing file");
                    if ui
                        .push_id(("relocate_target", index), |ui| ui.button("Relocate…"))
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
                ui.colored_label(ui.visuals().warn_fg_color, warn.as_str());
            }
            if let Some(metrics) = metrics {
                wrapped(ui, metrics_line(&metrics));
            }
            // The status says what is actually known, and the verification says
            // what was checked in the exported audio. They are different things,
            // and the user needs both.
            let status = status_for(self.report.as_ref(), index);
            let verification = verification_for(self.report.as_ref(), index);
            ui.horizontal_wrapped(|ui| {
                ui.label(status.label());
                ui.separator();
                ui.label(verification.label());
            });
            ui.separator();
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
    }

    /// The export controls and the list of files that were written.
    fn export_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Export");
        let mut browse = false;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Folder");
            // The field fills the space the panel has left, rather than asking
            // for an infinite width: a text box that grows with the window
            // would push the panel wider than its cap.
            let width = (ui.available_width() - ui.spacing().item_spacing.x - 70.0).max(80.0);
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
        wrapped(ui, match self.settings.layout {
            Layout::SourceAligned => {
                "One corrected file per source, keeping its own length and sample rate."
            }
            Layout::ReferenceTimelinePerSource => {
                "One reference-length file per source; the gaps are written as silence."
            }
            Layout::ReferenceTimelineMerged => {
                "One reference-length file with every source placed; overlaps follow the order above."
            }
            Layout::Mixdown => {
                "One mixed file: the recordings the mixer plays, on the reference timeline."
            }
            Layout::MixStems => {
                "One reference-length file per recording the mixer plays, without the mix."
            }
        });

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
                        .selectable_value(&mut self.settings.format, format, format_label(format))
                        .changed();
                }
            });

        // The clipping protection only applies to the mixing layouts: the other
        // layouts write one recording at a time, and a recording is never turned
        // down without being asked to be.
        if self.settings.layout.is_mix() {
            egui::ComboBox::from_label("Clipping")
                .selected_text(protection_label(self.settings.protection))
                .show_ui(ui, |ui| {
                    for protection in [
                        stagesync_core::mixer::ClippingProtection::None,
                        stagesync_core::mixer::ClippingProtection::SoftClip { threshold: 0.9 },
                        stagesync_core::mixer::ClippingProtection::AutoGain { target_dbfs: -1.0 },
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
                        "Nothing is done. A 32-bit float file holds a peak above 0 dBFS, so it \
                         is written as it is."
                    }
                    stagesync_core::mixer::ClippingProtection::SoftClip { .. } => {
                        "Samples above the threshold are compressed smoothly. This changes the \
                         material, and the number of samples it touched is reported."
                    }
                    stagesync_core::mixer::ClippingProtection::AutoGain { .. } => {
                        "The mix is measured and one master gain is applied so that the \
                         loudest sample lands on the target. The recordings keep their \
                         relative levels."
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
        let can_export = self.report.is_some()
            && !self.settings.dir.trim().is_empty()
            && self.running.is_none()
            && !self.verifying;
        if ui
            .add_enabled(can_export, egui::Button::new("Export"))
            .clicked()
        {
            self.start_export();
        }

        if let Some(outcome) = &self.export_outcome {
            ui.separator();
            ui.strong("Files written");
            egui::ScrollArea::vertical()
                .id_salt("written_files")
                .show(ui, |ui| {
                    for path in &outcome.files {
                        ui.monospace(platform::elide_path(path, PATH_CHARS));
                    }
                });
        }

        if changed {
            self.mark_dirty();
            self.persist_settings();
        }
    }

    /// The message log.
    fn log_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Messages");
            ui.separator();
            ui.colored_label(weak_text(ui), platform::describe_platform());
        });
        egui::ScrollArea::vertical()
            .id_salt("log")
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.log {
                    ui.label(line.as_str());
                }
            });
    }

    /// The main area: the timeline, the anchor controls and the source details.
    fn central(&mut self, ui: &mut egui::Ui) {
        self.central_width = ui.available_width();
        if let Some(meta) = self.recovery_available.clone() {
            let warn = ui.visuals().warn_fg_color;
            egui::Frame::new()
                .fill(warn.gamma_multiply(0.16))
                .corner_radius(4.0)
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
            // The colours come from the theme, so the message is legible both
            // on a light desktop and on a dark one.
            let foreground = ui.visuals().error_fg_color;
            egui::Frame::new()
                .fill(foreground.gamma_multiply(0.18))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.colored_label(foreground, error);
                });
            ui.add_space(6.0);
        }
        if let Some(notice) = self.notice.clone() {
            let foreground = ui.visuals().text_color();
            egui::Frame::new()
                .fill(foreground.gamma_multiply(0.18))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.colored_label(foreground, notice);
                });
            ui.add_space(6.0);
        }

        self.timeline_section(ui);
        ui.add_space(8.0);
        self.mixer_section(ui);
        ui.add_space(8.0);
        self.transport_section(ui);
        ui.add_space(4.0);
        self.problems_section(ui);
        ui.add_space(8.0);
        self.nudge_section(ui);
        ui.add_space(8.0);
        self.anchor_section(ui);
        ui.add_space(8.0);
        self.detail_section(ui);
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

        crate::mixer::panel(ui, &mut self.mixer, &infos);
        if self.player.is_playing() && self.player.voice() == Some(crate::player::Voice::Mix) {
            let live_members = self.mix_members();
            self.player
                .update_live_mix(self.mixer.master_gain, &live_members);
        }
    }

    /// The waveform timeline: the reference on top, the recordings below it.
    ///
    /// The picture *is* the alignment. Every recording is drawn where the sync
    /// engine put it, so a recording that is out of place is something the user
    /// can see rather than something they have to be told about. The waveforms
    /// come from the core's peak cache, which is built by streaming the file, so
    /// a three-hour recording costs a few megabytes rather than gigabytes.
    fn timeline_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Timeline");

        let duration = self
            .report
            .as_ref()
            .map(|report| report.timeline_duration.max(1.0))
            .unwrap_or(600.0);

        let mut fit = false;
        let mut zoom = 0.0f32;
        // The row is laid out so that it may wrap: a row that is wider than the
        // panel sets the panel's minimum width, and everything drawn after it is
        // then laid out too wide and cut off at the panel's edge.
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Start")
                .on_hover_text("Go to the beginning of the show")
                .clicked()
            {
                self.playhead = 0.0;
                self.view_start = 0.0;
            }
            if ui.button("-").on_hover_text("Zoom out").clicked() {
                zoom -= 1.0;
            }
            if ui.button("+").on_hover_text("Zoom in").clicked() {
                zoom += 1.0;
            }
            if ui
                .button("Fit")
                .on_hover_text("Show the whole show")
                .clicked()
            {
                fit = true;
            }
            ui.label(format!(
                "{:.0} s of {:.0} s on screen",
                (self.view_end - self.view_start).max(0.0),
                duration
            ));
        });

        if fit {
            self.view_start = 0.0;
            self.view_end = duration;
        }
        if zoom != 0.0 {
            self.zoom_view(zoom.signum(), self.playhead);
        }

        timeline::paint_legend(ui);
        ui.add_space(4.0);

        let width = ui.available_width().max(160.0);
        let height = waveview::height(self.targets.len());
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(width, height),
            egui::Sense::click_and_drag(),
        );
        {
            let view = self.waveform_view();
            waveview::paint(&painter, response.rect, &view);
        }
        self.handle_timeline_input(ui, &response);
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
    /// pan, and zoom.
    fn handle_timeline_input(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        let rect = response.rect;
        let shift = ui.input(|input| input.modifiers.shift);
        let pointer = response.interact_pointer_pos();

        if let Some(pointer) = pointer {
            // The wheel zooms around the pointer, so the material under the cursor
            // stays under the cursor.
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 && rect.contains(pointer) {
                let anchor = waveview::time_at(rect, pointer.x, self.view_start, self.view_end);
                self.zoom_view(if scroll > 0.0 { -1.0 } else { 1.0 }, anchor);
            }
            if response.clicked() {
                self.playhead =
                    waveview::time_at(rect, pointer.x, self.view_start, self.view_end).max(0.0);
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

    /// The transport: what is playing, and what the user can do about it.
    fn transport_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let playing = self.player.is_playing();
            if ui
                .button(if playing { "Stop" } else { "Play" })
                .on_hover_text("Play from the playhead")
                .clicked()
            {
                if playing {
                    self.player.stop();
                } else {
                    self.play_voice(self.voice);
                }
            }

            ui.separator();

            // The A/B switch: the two voices are heard at the same position on
            // the timeline, so the difference between them is the difference the
            // correction makes.
            if ui
                .selectable_label(self.voice == Voice::Reference, "Reference")
                .on_hover_text("Play the reference recording, exactly as it was recorded")
                .clicked()
            {
                self.play_voice(Voice::Reference);
            }
            if ui
                .selectable_label(self.voice == Voice::Target, "Corrected target")
                .on_hover_text("Play the selected recording with the correction applied")
                .clicked()
            {
                self.play_voice(Voice::Target);
            }
            if ui
                .selectable_label(self.voice == Voice::Mix, "Mix")
                .on_hover_text("Play the recordings the mixer has selected, mixed together")
                .clicked()
            {
                self.play_voice(Voice::Mix);
            }

            ui.separator();

            if ui
                .add_enabled(self.region.is_some(), egui::Button::new("Loop selection"))
                .on_hover_text("Play the selected region over and over")
                .clicked()
            {
                self.loop_region = self.region;
            }
            if ui
                .add_enabled(self.loop_region.is_some(), egui::Button::new("Clear loop"))
                .clicked()
            {
                self.loop_region = None;
            }

            ui.separator();

            ui.label(format!("Position {}", format_duration(self.playhead)));
        });

        if let Some(error) = self.player.error() {
            wrapped(ui, format!("There is no sound: {error}"));
        }
        if let (Some((start, end)), Some(kind)) = (self.region, self.selection_kind()) {
            let _ = kind;
            wrapped(
                ui,
                format!(
                    "Selected {start:.3} s to {end:.3} s ({:.3} s long).",
                    end - start
                ),
            );
        }
    }

    /// What kind of material the selection covers, when it covers one kind.
    fn selection_kind(&self) -> Option<SegmentKind> {
        let (start, end) = self.region?;
        let mut kind = None;
        for span in self.report.as_ref()?.timeline.iter() {
            if span.reference_start <= start && span.reference_end >= end {
                kind = Some(span.kind);
            }
        }
        kind
    }

    /// Play one of the two voices, from the playhead.
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
                    self.player.play_mix(&members, master, position)
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

    /// The recordings the mixer plays, with their corrections.
    ///
    /// The reference is never among them: it is the thing the other recordings were
    /// aligned to, and mixing it in would double every one of them.
    fn mix_members(&self) -> Vec<crate::player::MixMember> {
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
            let mut member =
                crate::player::MixMember::from_track(alignment.source.path.clone(), model, &track);
            member.gain = track.effective_gain(solo_active);
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

    /// Move the playhead to a place, and bring it into view.
    fn jump_to(&mut self, position: f64) {
        self.playhead = position.max(0.0);
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
        let reference_time = model.offset + nudge;
        if self.anchors.len() <= index {
            self.anchors.resize(index + 1, AnchorFields::default());
        }
        self.anchors[index] = AnchorFields {
            source_time: 0.0,
            reference_time,
            active: true,
        };
        // The nudge is now part of the model, so it must not be added a second
        // time.
        if let Some(value) = self.nudges.get_mut(index) {
            *value = 0.0;
        }
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
        if let Some(value) = self.nudges.get_mut(index) {
            *value = 0.0;
        }
        self.nudge_history.retain(|(other, _)| *other != index);
        if let Some(anchor) = self.anchors.get_mut(index) {
            anchor.active = false;
        }
        self.push_log(format!(
            "{} was reset to the automatic match",
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
                    ui.label("Source time");
                    ui.add(
                        egui::DragValue::new(&mut source_time)
                            .speed(0.05)
                            .range(0.0..=1_000_000.0),
                    );
                    ui.label("s");
                    ui.label("Reference time");
                    ui.add(
                        egui::DragValue::new(&mut reference_time)
                            .speed(0.05)
                            .range(0.0..=1_000_000.0),
                    );
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

        ui.heading(facts.name.as_str());

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

                ui.label("Accumulated drift over the source");
                ui.label(match facts.accumulated_ms {
                    Some(ms) => format!("{ms:+.2} ms"),
                    None => "no alignment".to_string(),
                });
                ui.end_row();

                ui.label("Coverage");
                ui.label(format!(
                    "{:.1}%  ({} covered, {} uncertain, {} uncovered)",
                    facts.coverage * 100.0,
                    format_duration(facts.covered_secs),
                    format_duration(facts.low_secs),
                    format_duration(facts.uncovered_secs)
                ));
                ui.end_row();

                ui.label("Residual p50 / p95 / max");
                ui.label(format!(
                    "{:+.2} / {:+.2} / {:.2} ms  ({} match points)",
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
                            ui.strong("Residual RMS");
                            ui.strong("Measurements");
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
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_verify,
                    egui::Button::new("Verify the corrected audio (hold-out)"),
                )
                .clicked()
            {
                self.start_verification(index);
            }
            if corrected.is_none() {
                ui.label("export the corrected audio to enable the hold-out check");
            } else if !reference_is_wav {
                ui.label("the hold-out check needs a WAV reference");
            }
        });

        ui.add_space(6.0);
        chart::paint_lag_plot(ui, &points, model, facts.duration);
        chart::paint_residual_plot(ui, &points, facts.duration);
        chart::paint_drift_plot(ui, &points, model, facts.duration);
    }
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
        (Some(offset), None, _, Some(accumulated)) => format!(
            "offset {offset:+.3} s · drift not applied · accumulated {accumulated:+.1} ms · coverage {:.1}%",
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

        // egui sizes a panel to its content and gives it no upper bound of its
        // own, so a long file name or a wide text box would otherwise squeeze
        // the centre — the timeline, the anchor controls and the results — out
        // of existence. The side panels are capped so that the centre always
        // keeps a usable share of the window, at any window size.
        let window = ui.available_width();
        let files_max = (window * 0.34).clamp(220.0, 420.0);
        let export_max = (window * 0.30).clamp(220.0, 360.0);

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("messages")
            .resizable(true)
            .default_size(110.0)
            .show(ui, |ui| self.log_panel(ui));
        egui::Panel::left("files")
            .resizable(true)
            .default_size(400.0_f32.min(files_max))
            .max_size(files_max)
            .show(ui, |ui| self.file_panel(ui));
        egui::Panel::right("export")
            .resizable(true)
            .default_size(340.0_f32.min(export_max))
            .max_size(export_max)
            .show(ui, |ui| self.export_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.central(ui));

        self.draw_pending_confirm(ui.ctx());

        if self.force_close {
            self.dirty = false;
            project::clear_recovery();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if self.running.is_some() || self.peaks_pending > 0 {
            // Keep the progress display moving while a job is running, and keep
            // the waveforms coming in as they are built.
            ui.ctx().request_repaint();
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

    /// Move the playhead with the sound, and keep a loop going.
    ///
    /// The playhead is the position on the reference timeline that is being heard.
    /// It follows the player, and when a loop is set the playback starts again at
    /// the beginning of the region as soon as it reaches the end of it.
    fn follow_the_playhead(&mut self) {
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
}
