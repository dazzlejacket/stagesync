//! Background jobs: probing, analysis, export and hold-out verification.
//!
//! Every long operation runs on its own thread and reports back over a channel, so the
//! window never blocks.
//!
//! The core pipeline has no cancellation channel: [`stagesync_core::pipeline::analyze`]
//! only reports progress. The progress callback is therefore the one point at which a
//! cancellation can be observed, and it is called at every stage boundary. The worker
//! unwinds out of the call when the user cancels, so the work really does stop between
//! stages instead of running to completion in the background.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;

use stagesync_core::audio::{AudioDecoder, SourceMeta};
use stagesync_core::export::{self, Artifact};
use stagesync_core::matching::{AlignmentReport, LinearModel};
use stagesync_core::pipeline::{analyze, AnalysisConfig, CancelToken, Progress, CANCELLED};
use stagesync_core::render::{self, Layout, RenderOptions, RenderResult};
use stagesync_core::verify::{verify_corrected, HoldoutResult, VerifyConfig};

use crate::state::{stem, ExportSettings, TaskProgress};

/// Payload used to unwind out of a core call when the user cancels.
#[derive(Debug)]
pub struct Cancelled;

/// Why a background job stopped without producing a result.
pub enum JobFailure {
    /// The user cancelled the job.
    Cancelled,
    /// The job failed with this message.
    Failed(String),
}

/// What an export produced.
#[derive(Debug)]
pub struct ExportOutcome {
    /// Absolute path of every file that was written.
    pub files: Vec<PathBuf>,
    /// The source-aligned render of each source, by source index.
    pub corrected: Vec<(usize, PathBuf)>,
}

/// A message from a background worker.
pub enum JobEvent {
    /// Progress of the running job.
    Progress(TaskProgress),
    /// A file finished being probed.
    Probed {
        /// The file that was probed.
        path: PathBuf,
        /// Its metadata, when it could be read.
        meta: Option<SourceMeta>,
        /// Why it could not be read, when it could not.
        error: Option<String>,
    },
    /// The analysis finished successfully.
    AnalysisFinished(Box<AlignmentReport>),
    /// The analysis failed with this message.
    AnalysisFailed(String),
    /// The analysis was cancelled by the user.
    AnalysisCancelled,
    /// The export finished successfully.
    ExportFinished(Box<ExportOutcome>),
    /// The export failed with this message.
    ExportFailed(String),
    /// The export was cancelled by the user.
    ExportCancelled,
    /// A hold-out verification finished.
    VerificationFinished {
        /// The source it belongs to.
        source_index: usize,
        /// The measurement.
        result: Box<HoldoutResult>,
    },
    /// A hold-out verification failed.
    VerificationFailed {
        /// The source it belongs to.
        source_index: usize,
        /// Why it failed.
        message: String,
    },
    /// The peaks of a recording are ready.
    PeaksReady {
        /// The recording they belong to.
        path: PathBuf,
        /// The peaks.
        cache: Box<stagesync_core::audio::PeakCache>,
    },
    /// The peaks of a recording could not be built.
    PeaksFailed {
        /// The recording they belong to.
        path: PathBuf,
        /// Why it failed.
        message: String,
    },
}

/// Build the waveform peaks of a recording in the background.
///
/// The peaks are what the waveform display draws, and building them means reading
/// the whole recording once. That is far too slow for the window's thread, and for
/// a three-hour recording it is far too slow for the user to wait for before the
/// window opens, so it happens on a thread of its own.
pub fn spawn_peaks(tx: Sender<JobEvent>, path: PathBuf) {
    thread::spawn(move || {
        let event = match stagesync_core::audio::PeakCache::build(&path, None) {
            Ok(cache) => JobEvent::PeaksReady {
                path,
                cache: Box::new(cache),
            },
            Err(error) => JobEvent::PeaksFailed {
                path,
                message: format!("{error:#}"),
            },
        };
        let _ = tx.send(event);
    });
}

/// Install a panic hook that keeps the deliberate cancellation unwinds silent.
///
/// Without this, every cancelled run would print a panic message even though nothing
/// went wrong.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload().downcast_ref::<Cancelled>().is_some() {
            return;
        }
        previous(info);
    }));
}

/// Probe a file in the background so the file list can show its format.
///
/// Probing hashes the whole file, which is far too slow to do on the UI thread for a
/// multi-hour recording.
pub fn spawn_probe(tx: Sender<JobEvent>, path: PathBuf) {
    thread::spawn(move || {
        let event = match AudioDecoder::open(&path) {
            Ok(decoder) => JobEvent::Probed {
                path,
                meta: Some(decoder.meta().clone()),
                error: None,
            },
            Err(error) => JobEvent::Probed {
                path,
                meta: None,
                error: Some(format!("{error:#}")),
            },
        };
        let _ = tx.send(event);
    });
}

/// Start the analysis of a reference against a list of sources.
///
/// Cancellation is cooperative and handled *inside* the core: the token is checked at
/// every stage boundary and inside the decoding loop, so a cancel takes effect within
/// about a second rather than at the end of a multi-hour file.
pub fn spawn_analysis(
    tx: Sender<JobEvent>,
    reference: PathBuf,
    sources: Vec<PathBuf>,
    mut config: AnalysisConfig,
    cancel: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        config.cancel = Some(CancelToken::from_flag(cancel.clone()));
        let sender = tx.clone();
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(move || {
            let mut progress = |p: Progress| {
                let _ = sender.send(JobEvent::Progress(TaskProgress {
                    label: p.stage.label().to_string(),
                    message: p.message,
                    fraction: p.fraction,
                }));
            };
            analyze(&reference, &sources, &config, &mut progress)
        }));

        let event = match outcome {
            Ok(Ok(report)) => {
                log::info!("analysis finished");
                JobEvent::AnalysisFinished(Box::new(report))
            }
            Ok(Err(error)) => {
                let text = format!("{error:#}");
                if text.contains(CANCELLED) {
                    log::info!("analysis cancelled");
                    JobEvent::AnalysisCancelled
                } else {
                    log::warn!("analysis failed: {text}");
                    JobEvent::AnalysisFailed(text)
                }
            }
            Err(payload) => {
                if payload.downcast_ref::<Cancelled>().is_some() {
                    log::info!("analysis cancelled");
                    JobEvent::AnalysisCancelled
                } else {
                    log::error!("the analysis thread panicked");
                    JobEvent::AnalysisFailed(format!(
                        "the analysis thread panicked: {}",
                        panic_text(&payload)
                    ))
                }
            }
        };
        let _ = tx.send(event);
    });
}

/// Start the export of the corrected audio and the machine-readable artefacts.
pub fn spawn_export(
    tx: Sender<JobEvent>,
    report: AlignmentReport,
    settings: ExportSettings,
    cancel: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let event = {
            let mut progress = |update: TaskProgress| {
                let _ = tx.send(JobEvent::Progress(update));
            };
            match export_report(&report, &settings, &cancel, &mut progress) {
                Ok(outcome) => {
                    log::info!("export finished");
                    JobEvent::ExportFinished(Box::new(outcome))
                }
                Err(JobFailure::Cancelled) => {
                    log::info!("export cancelled");
                    JobEvent::ExportCancelled
                }
                Err(JobFailure::Failed(message)) => {
                    log::warn!("export failed: {message}");
                    JobEvent::ExportFailed(message)
                }
            }
        };
        let _ = tx.send(event);
    });
}

/// Verify a rendered correction against the reference with the hold-out method.
///
/// This uses a different estimator (GCC-PHAT on the raw waveform) from the one that
/// fitted the model, so it is an independent check rather than a restatement.
pub fn spawn_verification(
    tx: Sender<JobEvent>,
    source_index: usize,
    corrected: PathBuf,
    reference: PathBuf,
    model: LinearModel,
    source_duration: f64,
) {
    thread::spawn(move || {
        let event = match verify_corrected(
            &corrected,
            &reference,
            &model,
            source_duration,
            &VerifyConfig::default(),
        ) {
            Ok(result) => JobEvent::VerificationFinished {
                source_index,
                result: Box::new(result),
            },
            Err(error) => JobEvent::VerificationFailed {
                source_index,
                message: format!("{error:#}"),
            },
        };
        let _ = tx.send(event);
    });
}

/// Render the corrected audio and write every machine-readable artefact.
///
/// This is the whole export step. The window drives it on a background thread and the
/// integration tests drive it directly.
///
/// `cancel` is checked between output files: a render that has already started is
/// allowed to finish, because stopping it half way would leave a truncated WAV behind.
pub fn export_report(
    report: &AlignmentReport,
    settings: &ExportSettings,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(TaskProgress),
) -> Result<ExportOutcome, JobFailure> {
    let out = PathBuf::from(settings.dir.trim());
    std::fs::create_dir_all(&out).map_err(|error| {
        JobFailure::Failed(format!(
            "failed to create the output directory {}: {error}",
            out.display()
        ))
    })?;

    let mut options = RenderOptions {
        layout: settings.layout,
        sample_rate: settings.sample_rate,
        format: settings.format,
        ..RenderOptions::default()
    };
    if options.layout != Layout::SourceAligned && options.sample_rate.is_none() {
        options.sample_rate = Some(report.reference.sample_rate);
    }

    let mut renders: Vec<RenderResult> = Vec::new();
    let mut corrected: Vec<(usize, PathBuf)> = Vec::new();
    let mut step = 0usize;

    // The number of files the chosen layout will produce, so the progress bar is honest.
    let expected = match options.layout {
        Layout::SourceAligned | Layout::ReferenceTimelinePerSource => report.sources.len(),
        Layout::ReferenceTimelineMerged => 1,
        Layout::Mixdown => 1,
        Layout::MixStems => settings.mix.tracks.len().max(1),
    };

    // The recordings the mixer plays. The reference is never among them.
    let selected: Vec<usize> = settings
        .mix
        .tracks
        .iter()
        .filter(|track| track.enabled && !track.mute)
        .map(|track| track.source_index)
        .collect();

    // The corrected files are named after the recordings they come from, so an
    // export into a directory that already holds a recording of that name
    // would land on the recording itself. The originals are read-only and the
    // reference is never altered, so this is refused before anything is
    // written.
    let inputs: Vec<PathBuf> = std::iter::once(report.reference.path.clone())
        .chain(report.sources.iter().map(|a| a.source.path.clone()))
        .collect();
    let planned = render::complete_outputs(
        &out,
        &planned_outputs(&out, report, options.layout, &selected),
    );
    // Two recordings that share a file name are given the same output name, and
    // the second write would silently replace the first.
    render::ensure_outputs_are_distinct(&planned)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
    // Every file the export writes is checked against the recordings, the
    // sidecars included: the export directory is often the directory the
    // recordings are in.
    render::ensure_outputs_avoid_inputs(&planned, &inputs)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;

    // The export is written into a staging directory and moved into place in one
    // step at the end, so a failure leaves the output directory exactly as it was
    // rather than half an export next to the recordings.
    let staged = export::StagedExport::begin(&out)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;

    match options.layout {
        Layout::SourceAligned => {
            for (index, alignment) in report.sources.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(JobFailure::Cancelled);
                }
                let Some(model) = alignment.model() else {
                    continue;
                };
                step += 1;
                report_progress(
                    &mut *on_progress,
                    step,
                    expected,
                    format!("rendering {}", alignment.source.file_name),
                );
                let path = staged.path(&format!(
                    "{}_corrected.wav",
                    stem(&alignment.source.file_name)
                ));
                render::prepare_output(&path)
                    .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
                let result =
                    render::render_source_aligned(&alignment.source.path, &model, &path, &options)
                        .map_err(|error| {
                            JobFailure::Failed(format!(
                                "failed to render {}: {error:#}",
                                path.display()
                            ))
                        })?;
                // The file is written into the staging directory and moved into place at the
                // end, so the path handed back is the final one.
                let final_path = out.join(
                    result
                        .path
                        .file_name()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| result.path.clone()),
                );
                corrected.push((index, absolute(&final_path)));
                renders.push(result);
            }
        }
        Layout::ReferenceTimelinePerSource => {
            for alignment in report.sources.iter() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(JobFailure::Cancelled);
                }
                let Some(model) = alignment.model() else {
                    continue;
                };
                step += 1;
                report_progress(
                    &mut *on_progress,
                    step,
                    expected,
                    format!("placing {}", alignment.source.file_name),
                );
                let path = staged.path(&format!(
                    "{}_timeline.wav",
                    stem(&alignment.source.file_name)
                ));
                render::prepare_output(&path)
                    .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
                let result = render::render_reference_timeline(
                    &alignment.source.path,
                    &model,
                    report.timeline_duration,
                    &path,
                    &options,
                )
                .map_err(|error| {
                    JobFailure::Failed(format!("failed to render {}: {error:#}", path.display()))
                })?;
                renders.push(result);
            }
        }
        Layout::ReferenceTimelineMerged => {
            if cancel.load(Ordering::Relaxed) {
                return Err(JobFailure::Cancelled);
            }
            if !report
                .sources
                .iter()
                .any(|alignment| alignment.model().is_some())
            {
                return Err(JobFailure::Failed(
                    "no source could be aligned, so no timeline was rendered".to_string(),
                ));
            }
            step += 1;
            report_progress(
                &mut *on_progress,
                step,
                expected,
                "merging the timeline".to_string(),
            );
            let path = staged.path("timeline_merged.wav");
            render::prepare_output(&path)
                .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
            // The merged timeline is rendered from the report's own ownership plan, so
            // the audio and the picture of it cannot disagree.
            let result =
                render::render_merged_timeline(report, &path, &options).map_err(|error| {
                    JobFailure::Failed(format!("failed to render {}: {error:#}", path.display()))
                })?;
            renders.push(result);
        }
        Layout::Mixdown => {
            if cancel.load(Ordering::Relaxed) {
                return Err(JobFailure::Cancelled);
            }
            if selected.is_empty() {
                return Err(JobFailure::Failed(
                    "no recording was chosen for the mix".to_string(),
                ));
            }
            step += 1;
            report_progress(
                &mut *on_progress,
                step,
                expected,
                "mixing the recordings".to_string(),
            );
            let path = staged.path("mixdown.wav");
            render::prepare_output(&path)
                .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
            let mix_options = stagesync_core::mixer::MixOptions {
                sample_rate: options.sample_rate,
                channels: 2,
                format: options.format,
                protection: settings.protection,
                ..stagesync_core::mixer::MixOptions::default()
            };
            let result =
                stagesync_core::mixer::render_mix(report, &settings.mix, &mix_options, &path)
                    .map_err(|error| {
                        JobFailure::Failed(format!(
                            "failed to render {}: {error:#}",
                            path.display()
                        ))
                    })?;
            for note in &result.notes {
                log::info!("the mix: {note}");
            }
            renders.push(RenderResult {
                path: result.path.clone(),
                sample_rate: result.sample_rate,
                channels: result.channels,
                frames: result.frames,
                source_frames: result.frames,
                silence_frames: 0,
                effective_rate: 1.0,
                pitch_shift_cents: 0.0,
                resampled: false,
            });
        }
        Layout::MixStems => {
            // One synchronised file per chosen recording, without the mix.
            for index in &selected {
                if cancel.load(Ordering::Relaxed) {
                    return Err(JobFailure::Cancelled);
                }
                let Some(alignment) = report.sources.get(*index) else {
                    continue;
                };
                let Some(model) = alignment.model() else {
                    continue;
                };
                step += 1;
                report_progress(
                    &mut *on_progress,
                    step,
                    expected,
                    format!("placing {}", alignment.source.file_name),
                );
                let path = staged.path(&format!("{}_stem.wav", stem(&alignment.source.file_name)));
                render::prepare_output(&path)
                    .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
                let result = render::render_reference_timeline(
                    &alignment.source.path,
                    &model,
                    report.timeline_duration,
                    &path,
                    &options,
                )
                .map_err(|error| {
                    JobFailure::Failed(format!("failed to render {}: {error:#}", path.display()))
                })?;
                renders.push(result);
            }
        }
    }

    // The cancel flag is read once more here, after the last file has been
    // written. A render that is already under way cannot be stopped, so the
    // user is told that the export was cancelled rather than being shown a
    // finished export that they asked to stop.
    if cancel.load(Ordering::Relaxed) {
        return Err(JobFailure::Cancelled);
    }

    let mut artefacts: Vec<Artifact> = Vec::new();

    // Every artefact is described by the path it will have once the export is
    // committed, and by the hash of the bytes that were actually written.
    let describe = |kind: &str, written: &Path| -> Result<Artifact, JobFailure> {
        let final_path = staged.final_path(written);
        artifact(kind, written, &final_path)
    };

    let syncmap = staged.path("syncmap.csv");
    export::write_syncmap_csv(&syncmap, report)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
    artefacts.push(describe("syncmap", &syncmap)?);

    let segments = staged.path("segments.csv");
    export::write_segments_csv(&segments, report)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
    artefacts.push(describe("segments", &segments)?);

    let graph = staged.path("drift_graph.svg");
    export::write_drift_graph(&graph, report)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
    artefacts.push(describe("drift_graph", &graph)?);

    let summary = staged.path("timeline.txt");
    export::write_timeline_summary(&summary, report)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
    artefacts.push(describe("timeline_summary", &summary)?);

    let plan = staged.path("plan.json");
    let plan_json = serde_json::to_string_pretty(report).map_err(|error| {
        JobFailure::Failed(format!("failed to serialise the analysis: {error}"))
    })?;
    std::fs::write(&plan, plan_json).map_err(|error| {
        JobFailure::Failed(format!("failed to write {}: {error}", plan.display()))
    })?;
    artefacts.push(describe("plan", &plan)?);

    for result in &renders {
        artefacts.push(describe("corrected_audio", &result.path)?);
    }

    let report_path = staged.path("report.json");
    export::write_report_json(&report_path, report, artefacts.clone())
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
    artefacts.push(describe("report", &report_path)?);

    // The manifest is written after the report, so that it can list it. It
    // lists every file of the export with its hash, which is what makes "the
    // export is byte-identical" a statement that can be checked from the export.
    let manifest_path = staged.path("manifest.json");
    let staging_dir = manifest_path
        .parent()
        .ok_or_else(|| JobFailure::Failed("the staging directory has no parent".to_string()))?;
    let mut kinds: Vec<(String, String)> = vec![
        ("syncmap.csv".into(), "syncmap".into()),
        ("segments.csv".into(), "segments".into()),
        ("drift_graph.svg".into(), "drift_graph".into()),
        ("timeline.txt".into(), "timeline_summary".into()),
        ("plan.json".into(), "plan".into()),
        ("report.json".into(), "report".into()),
    ];
    for result in &renders {
        if let Some(name) = result.path.file_name() {
            kinds.push((name.to_string_lossy().to_string(), "corrected_audio".into()));
        }
    }
    export::write_manifest_json(&manifest_path, staging_dir, &kinds)
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;
    artefacts.push(describe("manifest", &manifest_path)?);

    // The artefacts are written, and the cancel flag is read once more: a
    // cancel that arrived while the artefacts were being written still means
    // the user asked for the export to stop.
    if cancel.load(Ordering::Relaxed) {
        return Err(JobFailure::Cancelled);
    }

    // Everything is written: move the export into place in one step.
    staged
        .commit()
        .map_err(|error| JobFailure::Failed(format!("{error:#}")))?;

    let files = artefacts
        .iter()
        .map(|a| {
            let name = a
                .path
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| a.path.clone());
            absolute(&out.join(name))
        })
        .collect();

    on_progress(TaskProgress {
        label: "Exporting".to_string(),
        message: "all artefacts written".to_string(),
        fraction: 1.0,
    });

    Ok(ExportOutcome { files, corrected })
}

/// Send one progress notification for the export.
fn report_progress(
    on_progress: &mut dyn FnMut(TaskProgress),
    step: usize,
    expected: usize,
    message: String,
) {
    let fraction = if expected == 0 {
        1.0
    } else {
        (step as f64 / expected as f64).min(1.0)
    };
    on_progress(TaskProgress {
        label: "Exporting".to_string(),
        message,
        fraction,
    });
}

/// Describe a written file.
///
/// The path that is recorded is the one the file will have once the export is
/// committed, not the one it has while it is still in the staging directory.
/// A file that cannot be measured or hashed is an error: a report that says
/// "the artefacts are identical" has to be able to prove it.
fn artifact(kind: &str, written: &Path, final_path: &Path) -> Result<Artifact, JobFailure> {
    let bytes = std::fs::metadata(written)
        .map_err(|error| {
            JobFailure::Failed(format!(
                "failed to read the size of {}: {error}",
                written.display()
            ))
        })?
        .len();
    let sha256 = stagesync_core::audio::hash::sha256_file(written).map_err(|error| {
        JobFailure::Failed(format!("failed to hash {}: {error}", written.display()))
    })?;
    Ok(Artifact {
        kind: kind.to_string(),
        path: final_path.to_path_buf(),
        bytes,
        sha256: Some(sha256),
    })
}

/// The absolute path of a file, resolved against the file system when possible.
///
/// Windows' `canonicalize` answers with a verbatim path (`\\?\C:\...`). The prefix is
/// removed, because the paths are shown to the user and pasted into other tools.
fn absolute(path: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(path)
        .unwrap_or_else(|_| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()));
    crate::platform::strip_verbatim_prefix(&resolved)
}

/// The message carried by a panic payload.
fn panic_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "unknown cause".to_string()
    }
}

/// The files an export would write for the given layout.
///
/// The names are worked out before anything is written, so that an export
/// which would land on one of the recordings can be refused before the first
/// file is touched.
fn planned_outputs(
    out: &Path,
    report: &AlignmentReport,
    layout: Layout,
    selected: &[usize],
) -> Vec<PathBuf> {
    match layout {
        Layout::SourceAligned => report
            .sources
            .iter()
            .map(|a| out.join(format!("{}_corrected.wav", stem(&a.source.file_name))))
            .collect(),
        Layout::ReferenceTimelinePerSource => report
            .sources
            .iter()
            .map(|a| out.join(format!("{}_timeline.wav", stem(&a.source.file_name))))
            .collect(),
        Layout::ReferenceTimelineMerged => vec![out.join("timeline_merged.wav")],
        Layout::Mixdown => vec![out.join("mixdown.wav")],
        Layout::MixStems => selected
            .iter()
            .filter_map(|index| report.sources.get(*index))
            .map(|a| out.join(format!("{}_stem.wav", stem(&a.source.file_name))))
            .collect(),
    }
}
