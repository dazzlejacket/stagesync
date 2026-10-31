//! The StageSync window.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui::{self, Color32};
use stagesync_core::audio::SourceMeta;
use stagesync_core::matching::drift::DriftAssessment;
use stagesync_core::matching::{AlignmentReport, SourceAlignment};
use stagesync_core::pipeline::{AnalysisConfig, ManualAnchor};
use stagesync_core::render::Layout;
use stagesync_core::verify::{residual_stats, HoldoutResult};

use crate::chart;
use crate::jobs::{self, ExportOutcome, JobEvent};
use crate::platform;
use crate::settings::{self, AppSettings};
use crate::state::{
    drift_verdict, expand_dropped_paths, format_duration, format_label, is_wav, layout_label,
    metrics_for, status_for, verification_for, AnchorFields, ExportSettings, FileEntry,
    TaskProgress,
};
use crate::timeline;

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
    zoom: f64,
    tx: Sender<JobEvent>,
    rx: Receiver<JobEvent>,
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
            },
            use_spectral_check: true,
            measure_waveform_drift: true,
            force_offset_only: false,
            window_width: stored.window_width,
            window_height: stored.window_height,
            central_width: 0.0,
            closing_saved: false,
            zoom: 4.0,
            tx,
            rx,
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
                    self.report = Some(*report);
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
            }
        }
    }

    /// Store the result of a background probe.
    fn apply_probe(&mut self, path: PathBuf, meta: Option<SourceMeta>, error: Option<String>) {
        if let Some(entry) = self.reference.as_mut().filter(|entry| entry.path == path) {
            entry.meta = meta;
            entry.error = error;
            return;
        }
        if let Some(entry) = self.targets.iter_mut().find(|entry| entry.path == path) {
            entry.meta = meta;
            entry.error = error;
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
            }
            jobs::spawn_probe(self.tx.clone(), path);
        }
        if changed && announce {
            self.report = None;
            self.export_outcome = None;
            self.last_error = None;
            self.notice = Some("The file list changed — run the analysis again.".to_string());
        }
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
        self.report = None;
        self.export_outcome = None;
        self.selection = None;
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
        let name = entry.name();
        let previous = self.reference.take();
        self.reference = Some(entry);
        if let Some(previous) = previous {
            // The old reference becomes a target, with no anchor of its own.
            self.targets.push(previous);
            self.anchors.push(AnchorFields::default());
        }
        self.report = None;
        self.export_outcome = None;
        self.selection = Some(Selection::Reference);
        self.notice = Some(format!(
            "{name} is now the reference — run the analysis again."
        ));
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

        ui.horizontal(|ui| {
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
                    }
                }
            }
        });

        if changed {
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
                    .add_enabled(self.running.is_none(), egui::Button::new("✖"))
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
        self.anchor_section(ui);
        ui.add_space(8.0);
        self.detail_section(ui);
    }

    /// The zoomable timeline.
    fn timeline_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Reference timeline");

        let Some(report) = self.report.as_ref() else {
            wrapped(
                ui,
                "No analysis yet: add a reference and at least one target, then press Analyze.",
            );
            return;
        };

        let mut fit = false;
        // The row is laid out so that it may wrap: a row that is wider than
        // the panel sets the panel's minimum width, and everything drawn after
        // it is then laid out too wide and cut off at the panel's edge.
        ui.horizontal_wrapped(|ui| {
            ui.label("Zoom");
            ui.add(
                egui::Slider::new(&mut self.zoom, 0.2..=500.0)
                    .suffix(" px/s")
                    .logarithmic(true),
            );
            if ui.button("Fit").clicked() {
                fit = true;
            }
        });

        let duration = report.timeline_duration.max(1.0);
        if fit {
            let width = ui.available_width().max(120.0);
            self.zoom = (width as f64 / duration).clamp(0.2, 500.0);
        }

        let scale = self.zoom as f32;
        let width = ((duration as f32) * scale).max(160.0);
        let height = timeline::timeline_height(report.sources.len());

        timeline::paint_legend(ui);
        ui.add_space(4.0);

        egui::ScrollArea::horizontal()
            .id_salt("timeline_scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let (response, painter) =
                    ui.allocate_painter(egui::Vec2::new(width, height), egui::Sense::hover());
                timeline::paint_timeline(&painter, response.rect, report, scale);
            });
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
        self.drain_events();

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
            let expanded = expand_dropped_paths(&dropped);
            if expanded.is_empty() {
                self.last_error = Some("There are no recordings in what was dropped.".to_string());
            } else {
                self.import_paths(expanded);
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
        if ui.ctx().input(|input| input.viewport().close_requested()) && !self.closing_saved {
            // The window is about to close: the settings are written once, so the
            // file is not rewritten on every frame of the closing.
            self.closing_saved = true;
            self.persist_settings();
        }

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

        if self.running.is_some() {
            // Keep the progress display moving while a job is running.
            ui.ctx().request_repaint();
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
}
