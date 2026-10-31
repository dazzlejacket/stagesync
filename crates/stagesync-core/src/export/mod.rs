//! Exports: the machine-readable sync map, the run report and the drift graph.
//!
//! All artefacts are deterministic: the same inputs and configuration produce
//! byte-identical files, which is what makes the pipeline testable end to end.
//!
//! An export is written into a **staging directory** and moved into place at
//! the end, so a failure leaves nothing behind but the staging directory, which
//! is removed. The last file written is the manifest, which lists every file
//! the export produced together with its SHA-256: "the export is byte-identical"
//! is then a statement that can be checked from the export itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub mod graph;

use crate::matching::model::LinearModel;
use crate::matching::{AlignmentReport, SegmentKind, SourceAlignment};

/// The names of the sidecar files every export writes.
///
/// They are checked against the recordings that were read, because the export
/// directory is often the directory the recordings are in, and a recording
/// called `syncmap.csv` would be overwritten by the sidecar of the same name.
///
/// The list lives here, rather than being spelled out at each write site, so
/// that the *complete* set of files an export produces is known in one place.
pub const SIDECAR_FILES: &[&str] = &[
    "syncmap.csv",
    "segments.csv",
    "drift_graph.svg",
    "timeline.txt",
    "plan.json",
    "report.json",
    "manifest.json",
];

/// The paths the sidecar files of an export would be written to.
///
/// These are the files an *analysis-only* export writes — no corrected audio at
/// all — and they are checked against the recordings just as the audio is.
pub fn sidecar_outputs(out: &Path) -> Vec<PathBuf> {
    SIDECAR_FILES.iter().map(|name| out.join(name)).collect()
}

/// One file of the export manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// The file name, relative to the export directory.
    pub name: String,
    /// What the file is.
    pub kind: String,
    /// Size in bytes.
    pub bytes: u64,
    /// SHA-256 of the file.
    pub sha256: String,
}

/// The complete inventory of an export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    /// Tool name.
    pub tool: String,
    /// Tool version.
    pub version: String,
    /// The coordinate convention.
    pub coordinate_system: String,
    /// Every file the export produced.
    pub files: Vec<ManifestEntry>,
    /// A note about the manifest itself.
    pub note: String,
}

/// Write the export manifest.
///
/// The inventory is taken from the export directory itself rather than from a
/// list maintained by hand, so a file that was written and not remembered is
/// still listed. The manifest cannot contain its own hash, and says so.
pub fn write_manifest_json(path: &Path, out_dir: &Path, kinds: &[(String, String)]) -> Result<()> {
    let mut files: Vec<ManifestEntry> = Vec::new();
    for entry in std::fs::read_dir(out_dir)
        .with_context(|| format!("failed to read the export directory: {}", out_dir.display()))?
    {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if Path::new(&name) == path.file_name().unwrap_or_default() {
            continue;
        }
        let kind = kinds
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, kind)| kind.clone())
            .unwrap_or_else(|| "file".to_string());
        files.push(ManifestEntry {
            name,
            kind,
            bytes: metadata.len(),
            sha256: crate::audio::hash::sha256_file(&entry.path())?,
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let manifest = ExportManifest {
        tool: crate::APP_NAME.to_string(),
        version: crate::VERSION.to_string(),
        coordinate_system: COORDINATE_SYSTEM.to_string(),
        files,
        note: "The manifest cannot contain its own hash; every other file of the export is \
               listed above with its SHA-256."
            .to_string(),
    };
    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(path, json)
        .with_context(|| format!("failed to write the export manifest: {}", path.display()))?;
    Ok(())
}

/// An export that is written in a staging directory and moved into place.
///
/// The corrected files are named after the recordings they come from, so an
/// export that fails half way through would otherwise leave a partial file in
/// the output directory — next to the recordings, when the output directory is
/// the one the recordings are in. Staging the whole export and moving it in one
/// step means a failure leaves the output directory as it was.
pub struct StagedExport {
    staging: PathBuf,
    target: PathBuf,
    committed: bool,
}

impl StagedExport {
    /// Create the staging directory.
    ///
    /// The staging directory is a sibling of the output directory, so that
    /// moving the files into place is a rename within one directory rather than
    /// a copy across two.
    pub fn begin(target: &Path) -> Result<Self> {
        let target = if target.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            target.to_path_buf()
        };
        let parent = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&parent).with_context(|| {
            format!(
                "failed to create the output directory: {}",
                parent.display()
            )
        })?;
        std::fs::create_dir_all(&target).with_context(|| {
            format!(
                "failed to create the output directory: {}",
                target.display()
            )
        })?;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let staging = parent.join(format!(
            ".stagesync-export-{}-{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&staging).with_context(|| {
            format!(
                "failed to create the staging directory: {}",
                staging.display()
            )
        })?;
        Ok(Self {
            staging,
            target,
            committed: false,
        })
    }

    /// The path a file of the export should be written to.
    pub fn path(&self, name: &str) -> PathBuf {
        self.staging.join(name)
    }

    /// The directory the export will end up in.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Move every staged file into the output directory.
    pub fn commit(mut self) -> Result<()> {
        for entry in std::fs::read_dir(&self.staging).with_context(|| {
            format!(
                "failed to read the staging directory: {}",
                self.staging.display()
            )
        })? {
            let entry = entry?;
            let destination = self.target.join(entry.file_name());
            std::fs::rename(entry.path(), &destination).with_context(|| {
                format!(
                    "failed to move {} into the output directory",
                    entry.file_name().to_string_lossy()
                )
            })?;
        }
        let _ = std::fs::remove_dir_all(&self.staging);
        self.committed = true;
        Ok(())
    }
}

impl Drop for StagedExport {
    fn drop(&mut self) {
        if !self.committed {
            // The export did not finish: nothing of it is left behind.
            let _ = std::fs::remove_dir_all(&self.staging);
        }
    }
}

/// The coordinate convention used by every artefact.
pub const COORDINATE_SYSTEM: &str =
    "reference_time_seconds = scale * source_time_seconds + offset_seconds; drift_ppm = (scale - 1) * 1_000_000";

/// A model, serialised with its derived figures spelled out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSummary {
    /// Time scale.
    pub scale: f64,
    /// Offset in seconds.
    pub offset_seconds: f64,
    /// Drift in parts per million.
    pub drift_ppm: f64,
    /// Drift accumulated over the whole source, in milliseconds.
    pub accumulated_drift_ms: f64,
    /// Reference time of source time zero.
    pub reference_time_at_source_zero: f64,
}

impl ModelSummary {
    /// Summarise a linear model over a given source duration.
    pub fn new(model: &LinearModel, source_duration: f64) -> Self {
        Self {
            scale: model.scale,
            offset_seconds: model.offset,
            drift_ppm: model.drift_ppm(),
            accumulated_drift_ms: model.accumulated_drift_ms(0.0, source_duration),
            reference_time_at_source_zero: model.offset,
        }
    }
}

/// The complete run report written to `report.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    /// Tool name.
    pub tool: String,
    /// Tool version.
    pub version: String,
    /// The coordinate convention.
    pub coordinate_system: String,
    /// The alignment analysis.
    pub analysis: AlignmentReport,
    /// Paths of the artefacts produced by this run.
    pub artifacts: Vec<Artifact>,
    /// Extra notes.
    pub notes: Vec<String>,
}

/// A produced artefact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// What the artefact is.
    pub kind: String,
    /// Where it was written.
    pub path: PathBuf,
    /// Size in bytes.
    pub bytes: u64,
    /// SHA-256 of the file, when it could be computed.
    ///
    /// This is what makes the "run it twice and the artefacts are identical"
    /// claim checkable rather than a matter of opinion: two runs that agree
    /// produce the same hash here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Write the point-by-point sync map as CSV.
///
/// Columns:
///
/// | column | meaning |
/// |---|---|
/// | `source_file` | the source recording |
/// | `t_source_s` | position in the source, in seconds |
/// | `t_reference_s` | the measured position in the reference, in seconds |
/// | `measured_lag_s` | `t_reference_s - t_source_s`, the measured offset at this point |
/// | `model_lag_s` | the lag the final model predicts at `t_source_s` |
/// | `residual_ms` | `measured_lag_s - model_lag_s` in milliseconds; positive means the
///   measurement sits *later* in the reference than the model says |
/// | `confidence_score` | fused confidence in `[0, 1]` |
/// | `method` | which stage produced the measurement |
/// | `status_or_note` | why the point is not fully trusted, or `clean match` |
///
/// The coordinate convention is `reference_time = scale * source_time + offset`.
pub fn write_syncmap_csv(path: &Path, report: &AlignmentReport) -> Result<()> {
    let mut csv = String::new();
    csv.push_str(
        "source_file,t_source_s,t_reference_s,measured_lag_s,model_lag_s,residual_ms,confidence_score,method,status_or_note\n",
    );
    for source in &report.sources {
        let model = source.model();
        for p in &source.points {
            let model_lag =
                model.map_or(p.lag, |m| m.reference_time(p.source_time) - p.source_time);
            let residual_ms = (p.lag - model_lag) * 1000.0;
            // A point's own note says what happened to it; the confidence reason is
            // the fallback for the stages that do not have one.
            let status = if p.note.is_empty() {
                p.confidence.reason.describe()
            } else {
                p.note.as_str()
            };
            csv.push_str(&format!(
                "{},{:.6},{:.6},{:.6},{:.6},{:.3},{:.4},{},{}\n",
                csv_escape(&source.source.file_name),
                p.source_time,
                p.reference_time,
                p.lag,
                model_lag,
                residual_ms,
                p.confidence.score,
                p.method.name(),
                csv_escape(status),
            ));
        }
    }
    std::fs::write(path, csv)
        .with_context(|| format!("failed to write sync map: {}", path.display()))?;
    Ok(())
}

/// Write the segment (gap and coverage) list as CSV.
pub fn write_segments_csv(path: &Path, report: &AlignmentReport) -> Result<()> {
    let mut csv = String::new();
    csv.push_str("source_file,kind,source_start_s,source_end_s,reference_start_s,reference_end_s,duration_s,confidence,note\n");
    for source in &report.sources {
        for s in &source.segments {
            csv.push_str(&format!(
                "{},{},{},{},{:.3},{:.3},{:.3},{:.4},{}\n",
                csv_escape(&source.source.file_name),
                s.kind.label(),
                s.source_start
                    .map(|v| format!("{v:.3}"))
                    .unwrap_or_default(),
                s.source_end.map(|v| format!("{v:.3}")).unwrap_or_default(),
                s.reference_start,
                s.reference_end,
                s.duration(),
                s.confidence.score,
                csv_escape(&s.note),
            ));
        }
    }
    std::fs::write(path, csv)
        .with_context(|| format!("failed to write segment list: {}", path.display()))?;
    Ok(())
}

/// Write the JSON run report.
pub fn write_report_json(
    path: &Path,
    report: &AlignmentReport,
    artifacts: Vec<Artifact>,
) -> Result<()> {
    let run = RunReport {
        tool: crate::APP_NAME.to_string(),
        version: crate::VERSION.to_string(),
        coordinate_system: COORDINATE_SYSTEM.to_string(),
        analysis: report.clone(),
        artifacts,
        notes: Vec::new(),
    };
    let json = serde_json::to_string_pretty(&run)?;
    std::fs::write(path, json)
        .with_context(|| format!("failed to write report: {}", path.display()))?;
    Ok(())
}

/// Write the drift graph as an SVG document.
///
/// The graph shows the measured offset against source time with the fitted model line,
/// and below it the residual against that model, which is where drift appears as a slope.
pub fn write_drift_graph(path: &Path, report: &AlignmentReport) -> Result<()> {
    let svg = graph::render_drift_graph(report);
    std::fs::write(path, svg)
        .with_context(|| format!("failed to write drift graph: {}", path.display()))?;
    Ok(())
}
/// Write the merged timeline as a human-readable text summary.
pub fn write_timeline_summary(path: &Path, report: &AlignmentReport) -> Result<()> {
    let mut text = String::new();
    text.push_str("StageSync timeline summary\n");
    text.push_str("=========================\n\n");
    text.push_str(&format!(
        "reference: {} ({:.2} s, {} Hz, {} ch)\n",
        report.reference.file_name,
        report.reference.duration_secs,
        report.reference.sample_rate,
        report.reference.channels
    ));
    text.push_str(&format!(
        "timeline length: {:.2} s\n\n",
        report.timeline_duration
    ));
    for span in &report.timeline {
        let owner = match span.source_index {
            Some(i) => report
                .sources
                .get(i)
                .map(|s| s.source.file_name.clone())
                .unwrap_or_default(),
            None => "-".to_string(),
        };
        text.push_str(&format!(
            "{:>10.2} -> {:>10.2}  {:<26} {:<28} {}\n",
            span.reference_start,
            span.reference_end,
            span.kind.label(),
            owner,
            span.note
        ));
    }
    text.push('\n');
    for source in &report.sources {
        let model = source.model();
        text.push_str(&format!("--- {} ---\n", source.source.file_name));
        if let Some(m) = model {
            text.push_str(&format!(
                "  scale {:.9}  offset {:.4} s  drift {:+.3} ppm  accumulated {:+.1} ms\n",
                m.scale,
                m.offset,
                m.drift_ppm(),
                m.accumulated_drift_ms(0.0, source.source.duration_secs)
            ));
        }
        if let Some(d) = source.direct_model {
            if let Some(c) = source.calibrated_model {
                if (d.scale - c.scale).abs() > 1e-12 {
                    text.push_str(&format!(
                        "  note: direct drift {:+.3} ppm and calibrated drift {:+.3} ppm disagree by {:+.3} ppm\n",
                        d.drift_ppm(),
                        c.drift_ppm(),
                        c.drift_ppm() - d.drift_ppm()
                    ));
                }
            }
        }
        text.push_str(&format!(
            "  coverage: {:.1}% ({:.1} s covered, {:.1} s uncertain, {:.1} s uncovered)\n",
            source.coverage.fraction * 100.0,
            source.coverage.covered_seconds,
            source.coverage.low_confidence_seconds,
            source.coverage.uncovered_seconds
        ));
        for w in &source.warnings {
            text.push_str(&format!("  warning: {w}\n"));
        }
        text.push('\n');
    }
    std::fs::write(path, text)
        .with_context(|| format!("failed to write timeline summary: {}", path.display()))?;
    Ok(())
}

/// Escape a value for CSV output.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Collect the segment kinds present in a report (used by the GUI legend).
pub fn segment_kinds(report: &AlignmentReport) -> Vec<SegmentKind> {
    let mut kinds = Vec::new();
    for span in &report.timeline {
        if !kinds.contains(&span.kind) {
            kinds.push(span.kind);
        }
    }
    kinds
}

/// Summarise one source alignment for display.
pub fn summarise_source(source: &SourceAlignment) -> String {
    let model = source.model();
    match model {
        Some(m) => format!(
            "{}: offset {:.3} s, drift {:+.2} ppm, {:.1}% covered",
            source.source.file_name,
            m.offset,
            m.drift_ppm(),
            source.coverage.fraction * 100.0
        ),
        None => format!("{}: no reliable alignment", source.source.file_name),
    }
}
