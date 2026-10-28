//! The StageSync window.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui::{self, Color32};
use stagesync_core::audio::SourceMeta;
use stagesync_core::matching::{AlignmentReport, SourceAlignment};
use stagesync_core::pipeline::{AnalysisConfig, ManualAnchor};
use stagesync_core::render::Layout;
use stagesync_core::verify::{residual_stats, HoldoutResult};

use crate::chart;
use crate::jobs::{self, ExportOutcome, JobEvent};
use crate::state::{
    format_duration, format_label, is_wav, layout_label, metrics_for, AnchorFields, ExportSettings,
    FileEntry, TaskProgress,
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
    drift_ppm: Option<f64>,
    accumulated_ms: Option<f64>,
    coverage: f64,
    covered_secs: f64,
    low_secs: f64,
    uncovered_secs: f64,
    residuals: (f64, f64, f64),
    point_count: usize,
    warnings: Vec<String>,
    holdout: Option<HoldoutResult>,
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
    anchor: AnchorFields,
    settings: ExportSettings,
    /// Whether the independent log-mel cross-check is run during the analysis.
    use_spectral_check: bool,
    zoom: f64,
    tx: Sender<JobEvent>,
    rx: Receiver<JobEvent>,
}

impl Default for StageSyncApp {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
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
            anchor: AnchorFields::default(),
            settings: ExportSettings {
                dir: String::new(),
                layout: Layout::SourceAligned,
                sample_rate: None,
                format: stagesync_core::audio::WavFormat::Float32,
            },
            use_spectral_check: true,
            zoom: 4.0,
            tx,
            rx,
        }
    }
}

impl StageSyncApp {
    /// A new, empty application.
    pub fn new() -> Self {
        Self::default()
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
                    self.report = Some(*report);
                    self.last_error = None;
                    self.notice = Some("Analysis finished.".to_string());
                    self.push_log("analysis finished".to_string());
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

    /// The manual anchor, when it is in effect.
    fn anchor_value(&self) -> Option<ManualAnchor> {
        if !self.anchor.active {
            return None;
        }
        Some(ManualAnchor {
            source_time: self.anchor.source_time,
            reference_time: self.anchor.reference_time,
        })
    }

    // --- Import ------------------------------------------------------------

    /// Add files to the list.
    ///
    /// The first file becomes the reference when there is none yet; every other file
    /// becomes a target. Changing the list invalidates the analysis, so the old report
    /// is dropped rather than left to describe files that are no longer there.
    fn import_paths(&mut self, paths: Vec<PathBuf>) {
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
            }
            jobs::spawn_probe(self.tx.clone(), path);
        }
        if changed {
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
        self.report = None;
        self.export_outcome = None;
        self.selection = None;
        self.notice = Some(format!("{name} was removed — run the analysis again."));
    }

    // --- Jobs --------------------------------------------------------------

    /// Start the analysis of the reference against the targets.
    fn start_analysis(&mut self) {
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
            manual_anchor: self.anchor_value(),
            ..AnalysisConfig::default()
        };
        if config.manual_anchor.is_some() {
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
            let model = alignment.calibrated_model.or(alignment.direct_model)?;
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
        let busy = self.running.is_some();
        let can_analyze = self.reference.is_some() && !self.targets.is_empty() && !busy;
        let mut start = false;
        let mut cancel = false;

        ui.horizontal(|ui| {
            start = ui
                .add_enabled(can_analyze, egui::Button::new("Analyze"))
                .clicked();
            if busy {
                let label = match self.running.as_ref().map(|job| job.kind) {
                    Some(JobKind::Export) => "Cancel (after this file)",
                    _ => "Cancel",
                };
                cancel = ui.button(label).clicked();
            }
            ui.separator();
            ui.checkbox(&mut self.use_spectral_check, "Spectral cross-check")
                .on_hover_text(
                    "Run the independent log-mel cross-check while analysing. It is slower and \
                     gives a second opinion on every match; turn it off if it is not usable.",
                );
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
        ui.label("Drop audio files onto the window, or use Add files…");
        ui.horizontal(|ui| {
            if ui.button("Add files…").clicked() {
                self.browse_for_files();
            }
            if ui
                .add_enabled(
                    self.reference.is_some(),
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
            ui.monospace(entry.path.display().to_string());
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
                ui.colored_label(
                    Color32::from_rgb(235, 120, 110),
                    format!("cannot read: {error}"),
                );
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
                if ui.small_button("✖").clicked() {
                    remove = Some(index);
                }
            });
            ui.monospace(entry.path.display().to_string());
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
                ui.colored_label(
                    Color32::from_rgb(235, 120, 110),
                    format!("cannot read: {error}"),
                );
            }
            if let Some(metrics) = metrics {
                ui.label(metrics_line(&metrics));
            }
            ui.separator();
        }

        if let Some(selection) = select {
            self.selection = Some(selection);
        }
        if let Some(index) = remove {
            self.remove_target(index);
        }
    }

    /// The export controls and the list of files that were written.
    fn export_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Export");
        let mut browse = false;
        ui.horizontal(|ui| {
            ui.label("Folder");
            ui.add(
                egui::TextEdit::singleline(&mut self.settings.dir)
                    .desired_width(f32::INFINITY)
                    .hint_text("choose a folder"),
            );
            if ui.button("Browse…").clicked() {
                browse = true;
            }
        });
        if browse {
            self.browse_for_folder();
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
                    ui.selectable_value(&mut self.settings.layout, layout, label);
                }
            });
        ui.label(match self.settings.layout {
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
            }
            if let Some(rate) = self.settings.sample_rate.as_mut() {
                ui.add(
                    egui::DragValue::new(rate)
                        .speed(100.0)
                        .range(8_000..=192_000),
                );
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
                    ui.selectable_value(&mut self.settings.format, format, format_label(format));
                }
            });

        ui.add_space(6.0);
        let can_export =
            self.report.is_some() && !self.settings.dir.trim().is_empty() && self.running.is_none();
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
                        ui.monospace(path.display().to_string());
                    }
                });
        }
    }

    /// The message log.
    fn log_panel(&mut self, ui: &mut egui::Ui) {
        ui.strong("Messages");
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
        if let Some(error) = self.last_error.clone() {
            egui::Frame::new()
                .fill(Color32::from_rgb(64, 22, 22))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.colored_label(Color32::from_rgb(245, 140, 130), error);
                });
            ui.add_space(6.0);
        }
        if let Some(notice) = self.notice.clone() {
            egui::Frame::new()
                .fill(Color32::from_rgb(30, 48, 66))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.colored_label(Color32::from_rgb(150, 205, 240), notice);
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

        let mut fit = false;
        egui::Grid::new("timeline_controls")
            .num_columns(3)
            .show(ui, |ui| {
                ui.label("Zoom");
                ui.add(
                    egui::Slider::new(&mut self.zoom, 0.2..=500.0)
                        .suffix(" px/s")
                        .logarithmic(true),
                );
                if ui.button("Fit").clicked() {
                    fit = true;
                }
                ui.end_row();
            });

        let Some(report) = self.report.as_ref() else {
            ui.label(
                "No analysis yet: add a reference and at least one target, then press Analyze.",
            );
            return;
        };

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
    fn anchor_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Manual anchor")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    "Use this when the automatic match fails. The anchor is a hard constraint: \
                     it is applied to every source, and the final model of each source is \
                     forced through it.",
                );
                ui.horizontal(|ui| {
                    ui.label("Source time");
                    ui.add(
                        egui::DragValue::new(&mut self.anchor.source_time)
                            .speed(0.05)
                            .range(0.0..=1_000_000.0),
                    );
                    ui.label("s");
                    ui.label("Reference time");
                    ui.add(
                        egui::DragValue::new(&mut self.anchor.reference_time)
                            .speed(0.05)
                            .range(0.0..=1_000_000.0),
                    );
                    ui.label("s");
                });

                let can_run = self.reference.is_some() && !self.targets.is_empty();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_run, egui::Button::new("Apply anchor"))
                        .clicked()
                    {
                        self.anchor.active = true;
                        self.start_analysis();
                    }
                    if ui
                        .add_enabled(self.anchor.active, egui::Button::new("Clear anchor"))
                        .clicked()
                    {
                        self.anchor.active = false;
                        self.start_analysis();
                    }
                    if self.anchor.active {
                        ui.label("the anchor is in effect");
                    }
                });
            });
    }

    /// The details of the selected target, and the drift charts.
    fn detail_section(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.selected_target_index() else {
            ui.label("Select a target recording in the file list to see how it was aligned.");
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
                alignment.calibrated_model.or(alignment.direct_model),
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
                ui.label(match facts.drift_ppm {
                    Some(ppm) => format!("{ppm:+.3} ppm"),
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

        if !facts.warnings.is_empty() {
            ui.label("Warnings");
            for warning in &facts.warnings {
                ui.colored_label(Color32::from_rgb(235, 190, 120), warning.as_str());
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
                });
            for note in &holdout.notes {
                ui.colored_label(Color32::from_rgb(235, 190, 120), note.as_str());
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
    }
}

/// Read the figures of one source out of its alignment.
fn facts_for(alignment: &SourceAlignment) -> SourceFacts {
    let model = alignment.calibrated_model.or(alignment.direct_model);
    let residuals: Vec<f64> = alignment
        .points
        .iter()
        .map(|point| point.residual_ms)
        .collect();
    SourceFacts {
        name: alignment.source.file_name.clone(),
        duration: alignment.source.duration_secs,
        offset_secs: model.map(|model| model.offset),
        drift_ppm: model.map(|model| model.drift_ppm()),
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
    }
}

/// The measured figures of one target, as one line of text.
fn metrics_line(metrics: &crate::state::TargetMetrics) -> String {
    match (metrics.offset_secs, metrics.drift_ppm, metrics.accumulated_drift_ms) {
        (Some(offset), Some(ppm), Some(accumulated)) => format!(
            "offset {offset:+.3} s · drift {ppm:+.2} ppm · accumulated {accumulated:+.1} ms · coverage {:.1}%",
            metrics.coverage.unwrap_or(0.0) * 100.0
        ),
        _ => "no alignment yet".to_string(),
    }
}

impl eframe::App for StageSyncApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_events();

        // Files dropped onto the window are the primary way to add recordings.
        let dropped: Vec<PathBuf> = ui.ctx().input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.import_paths(dropped);
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("messages")
            .resizable(true)
            .default_size(110.0)
            .show(ui, |ui| self.log_panel(ui));
        egui::Panel::left("files")
            .resizable(true)
            .default_size(400.0)
            .show(ui, |ui| self.file_panel(ui));
        egui::Panel::right("export")
            .resizable(true)
            .default_size(340.0)
            .show(ui, |ui| self.export_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.central(ui));

        if self.running.is_some() {
            // Keep the progress display moving while a job is running.
            ui.ctx().request_repaint();
        }
    }
}
