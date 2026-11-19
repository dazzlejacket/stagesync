//! `.stagesync` project persistence, relocation, staleness checks and crash recovery.
//!
//! A project file is compact JSON referencing source recordings by relative path
//! (with an absolute fallback) plus their byte length and SHA-256 digest. Large
//! audio files are **never** embedded inside project files, and cached alignment
//! results are **refused** whenever any source recording is missing or modified.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use stagesync_core::audio::sha256_file;
use stagesync_core::matching::AlignmentReport;
use stagesync_core::mixer::{
    ClippingProtection, ContainerChoice, LoudnessMatch, MixTrack, MonitorMode,
};

use crate::mixer::MixerState;
use crate::platform;
use crate::settings;
use crate::state::{AnchorFields, ExportSettings, FileEntry};

/// Current on-disk `.stagesync` schema version.
pub const PROJECT_FORMAT_VERSION: u32 = 1;

/// Preferred file extension without leading dot.
pub const PROJECT_EXTENSION: &str = "stagesync";

/// Complete saved desktop session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSyncProject {
    /// Format version (`1`).
    pub version: u32,
    /// Unix timestamp in seconds when this file was saved.
    pub saved_at_unix_secs: u64,
    /// Reference recording reference, if set.
    pub reference: Option<ProjectMediaRef>,
    /// Target recordings with per-target anchors and pending nudges.
    pub targets: Vec<ProjectTargetEntry>,
    /// Analysis toggles in effect when the report was computed.
    pub analysis: ProjectAnalysisOptions,
    /// Cached alignment report when available. Ignored on load if any media
    /// file is missing or changed on disk.
    pub report: Option<AlignmentReport>,
    /// Mixer tracks, master gain and monitor mode.
    pub mixer: ProjectMixerState,
    /// Timeline viewport, playhead, selected region, loop and voice.
    pub timeline: ProjectTimelineState,
    /// Export folder, layout, format and mix protection options.
    pub export: ProjectExportSettings,
}

/// Metadata and content digest of a referenced audio recording.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectMediaRef {
    /// Original file name for display when missing.
    pub file_name: String,
    /// Original absolute path when saved.
    pub absolute_path: PathBuf,
    /// Path relative to the `.stagesync` project file directory, if computable.
    pub relative_path: Option<PathBuf>,
    /// SHA-256 hex digest of the file bytes at save time.
    pub sha256: Option<String>,
    /// File length in bytes at save time.
    pub byte_len: Option<u64>,
    /// Cached sample rate for immediate UI display.
    pub sample_rate: Option<u32>,
    /// Cached channel count.
    pub channels: Option<usize>,
    /// Cached duration in seconds.
    pub duration_secs: Option<f64>,
    /// Cached frame count.
    pub frames: Option<u64>,
}

/// One target entry inside a `.stagesync` project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectTargetEntry {
    /// Referenced recording location and digest.
    pub media: ProjectMediaRef,
    /// User manual anchor constraint for this target.
    pub anchor: ProjectAnchor,
    /// Pending un-applied nudge in seconds.
    pub pending_nudge_secs: f64,
}

/// Serializable manual anchor constraint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct ProjectAnchor {
    pub source_time: f64,
    pub reference_time: f64,
    pub active: bool,
}

impl From<AnchorFields> for ProjectAnchor {
    fn from(value: AnchorFields) -> Self {
        Self {
            source_time: value.source_time,
            reference_time: value.reference_time,
            active: value.active,
        }
    }
}

impl From<ProjectAnchor> for AnchorFields {
    fn from(value: ProjectAnchor) -> Self {
        Self {
            source_time: value.source_time,
            reference_time: value.reference_time,
            active: value.active,
        }
    }
}

/// Serializable analysis toggles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectAnalysisOptions {
    pub use_spectral_check: bool,
    pub measure_waveform_drift: bool,
    pub force_offset_only: bool,
}

impl Default for ProjectAnalysisOptions {
    fn default() -> Self {
        Self {
            use_spectral_check: true,
            measure_waveform_drift: true,
            force_offset_only: false,
        }
    }
}

/// Serializable mixer state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMixerState {
    pub mode: MonitorMode,
    pub master_gain: f32,
    pub tracks: Vec<MixTrack>,
}

impl Default for ProjectMixerState {
    fn default() -> Self {
        Self {
            mode: MonitorMode::Both,
            master_gain: 1.0,
            tracks: Vec::new(),
        }
    }
}

impl From<&MixerState> for ProjectMixerState {
    fn from(mixer: &MixerState) -> Self {
        Self {
            mode: mixer.mode,
            master_gain: mixer.master_gain,
            tracks: mixer.tracks.clone(),
        }
    }
}

/// Playback voice stored in timeline state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ProjectVoice {
    #[default]
    Reference,
    Target,
    Mix,
}

/// Serializable timeline viewport and transport cursor state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectTimelineState {
    pub view_start: f64,
    pub view_end: f64,
    pub playhead: f64,
    pub region: Option<(f64, f64)>,
    pub loop_region: Option<(f64, f64)>,
    pub voice: ProjectVoice,
}

impl Default for ProjectTimelineState {
    fn default() -> Self {
        Self {
            view_start: 0.0,
            view_end: 600.0,
            playhead: 0.0,
            region: None,
            loop_region: None,
            voice: ProjectVoice::Reference,
        }
    }
}

/// Serializable export preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectExportSettings {
    pub dir: String,
    pub layout: String,
    pub sample_rate: Option<u32>,
    pub format: String,
    pub protection: ClippingProtection,
    pub loudness: LoudnessMatch,
    pub container: ContainerChoice,
}

impl From<&ExportSettings> for ProjectExportSettings {
    fn from(value: &ExportSettings) -> Self {
        Self {
            dir: value.dir.clone(),
            layout: settings::layout_name(value.layout).to_string(),
            sample_rate: value.sample_rate,
            format: settings::format_name(value.format).to_string(),
            protection: value.protection,
            loudness: value.loudness,
            container: value.container,
        }
    }
}

impl ProjectExportSettings {
    /// Convert back into runtime `ExportSettings`.
    pub fn to_export_settings(&self, mixer: &MixerState) -> ExportSettings {
        ExportSettings {
            dir: self.dir.clone(),
            layout: settings::layout_from_name(&self.layout),
            sample_rate: self.sample_rate,
            format: settings::format_from_name(&self.format),
            mix: mixer.settings(),
            protection: self.protection,
            loudness: self.loudness,
            container: self.container,
        }
    }
}

/// Verification status of one referenced recording when loaded from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaStatus {
    /// Located on disk and bytes match saved size + SHA-256 (or no digest was recorded).
    Verified,
    /// Located via relative path after moving the project folder.
    RelocatedByRelativePath,
    /// File was found on disk, but size or SHA-256 changed since the project was saved.
    ModifiedOnDisk { detail: String },
    /// File could not be found at relative or saved absolute path.
    Missing,
}

/// Result of resolving one media reference on load.
#[derive(Debug, Clone)]
pub struct ResolvedMedia {
    /// Path to use in the UI (resolved existing path, or saved absolute path if missing).
    pub resolved_path: PathBuf,
    /// Verification status against disk.
    pub status: MediaStatus,
    /// Original saved reference for relocation checks / display.
    pub saved: ProjectMediaRef,
}

/// Loaded and verified project ready to hydrate `StageSyncApp`.
#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub project: StageSyncProject,
    pub reference: Option<ResolvedMedia>,
    pub targets: Vec<ResolvedMedia>,
    /// Verified report (`None` if missing, stale due to changed media, or invalidated).
    pub valid_report: Option<AlignmentReport>,
    /// Human-readable notices/warnings raised during load & integrity verification.
    pub notices: Vec<String>,
}

/// Crash-recovery metadata saved alongside `recovery.stagesync`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryMeta {
    pub active_project_path: Option<PathBuf>,
    pub saved_at_unix_secs: u64,
    pub dirty: bool,
}

impl ProjectMediaRef {
    /// Capture location, metadata and SHA-256 for a `FileEntry` relative to `project_dir`.
    pub fn from_entry(entry: &FileEntry, project_dir: Option<&Path>) -> Self {
        let abs = make_absolute(&entry.path);
        let rel = project_dir.and_then(|base| relative_path(&abs, base));

        // Reuse probed metadata when available; compute SHA-256 and byte length
        // directly from disk if missing and file still exists so staleness checks
        // always have a cryptographic anchor.
        let (sha256, byte_len, sample_rate, channels, duration_secs, frames) =
            if let Some(meta) = &entry.meta {
                (
                    Some(meta.sha256.clone()),
                    Some(meta.byte_len),
                    Some(meta.sample_rate),
                    Some(meta.channels),
                    Some(meta.duration_secs),
                    Some(meta.frames),
                )
            } else if abs.is_file() {
                let bytes = fs::metadata(&abs)
                    .ok()
                    .map(|m| m.len())
                    .or(entry.expected_byte_len);
                let digest = sha256_file(&abs)
                    .ok()
                    .or_else(|| entry.expected_sha256.clone());
                (digest, bytes, None, None, None, None)
            } else {
                // Keep the expected identity when a file is currently missing so a
                // save/re-snapshot never wipes out relink verification.
                (
                    entry.expected_sha256.clone(),
                    entry.expected_byte_len,
                    None,
                    None,
                    None,
                    None,
                )
            };

        Self {
            file_name: entry.name(),
            absolute_path: abs,
            relative_path: rel,
            sha256,
            byte_len,
            sample_rate,
            channels,
            duration_secs,
            frames,
        }
    }
}

/// Refuse to save a project without the `.stagesync` extension or over any input recording.
pub fn ensure_safe_project_path(path: &Path, inputs: &[PathBuf]) -> Result<()> {
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(PROJECT_EXTENSION));
    if !ext_ok {
        bail!(
            "project files must use the .{PROJECT_EXTENSION} extension (got {})",
            path.display()
        );
    }
    if !inputs.is_empty() {
        stagesync_core::render::ensure_output_does_not_overwrite(path, inputs).with_context(
            || {
                format!(
                    "refusing to overwrite an input recording with project file {}",
                    path.display()
                )
            },
        )?;
    }
    Ok(())
}

impl StageSyncProject {
    /// Collect all input paths referenced by this project (relative paths resolved against `base_dir`).
    fn referenced_paths(&self, base_dir: Option<&Path>) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut push_ref = |r: &ProjectMediaRef| {
            out.push(r.absolute_path.clone());
            if let (Some(dir), Some(rel)) = (base_dir, &r.relative_path) {
                out.push(clean_path(&dir.join(rel)));
            }
        };
        if let Some(r) = &self.reference {
            push_ref(r);
        }
        for t in &self.targets {
            push_ref(&t.media);
        }
        out
    }

    /// Save the project atomically to `path` (`.tmp` write + atomic rename).
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let inputs = self.referenced_paths(path.parent());
        ensure_safe_project_path(path, &inputs)?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create directory {}", parent.display()))?;
            }
        }
        let json = serde_json::to_string_pretty(self).context("serialize StageSync project")?;
        atomic_write_bytes(path, json.as_bytes())
    }

    /// Load a `.stagesync` file, resolve paths, and verify media integrity.
    pub fn load_from(path: &Path) -> Result<LoadedProject> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("read project file {}", path.display()))?;
        let project: StageSyncProject = serde_json::from_str(&raw)
            .with_context(|| format!("parse project file {}", path.display()))?;

        if project.version != PROJECT_FORMAT_VERSION {
            bail!(
                "unsupported .stagesync project version {} (expected {})",
                project.version,
                PROJECT_FORMAT_VERSION
            );
        }

        let project_dir = path.parent().filter(|p| !p.as_os_str().is_empty());
        let mut notices = Vec::new();
        let mut stale_reasons = Vec::new();

        let reference = project
            .reference
            .as_ref()
            .map(|r| resolve_media(project_dir, r));
        if let Some(ref_res) = &reference {
            record_media_notices("Reference", ref_res, &mut notices, &mut stale_reasons);
        }

        let mut targets = Vec::with_capacity(project.targets.len());
        for target in &project.targets {
            let resolved = resolve_media(project_dir, &target.media);
            record_media_notices(
                &target.media.file_name,
                &resolved,
                &mut notices,
                &mut stale_reasons,
            );
            targets.push(resolved);
        }

        // Never reuse cached analysis results if any recording is missing, modified,
        // or if the cached report's embedded digests disagree with the resolved files.
        let mut valid_report = project.report.clone();
        if let Some(report) = valid_report.as_mut() {
            if report.sources.len() != project.targets.len() {
                valid_report = None;
                notices.push(
                    "Saved alignment report did not match target count and was discarded."
                        .to_string(),
                );
            } else if !stale_reasons.is_empty() {
                valid_report = None;
                notices.push(format!(
                    "Cached sync results were invalidated because source audio changed or is missing ({}). Re-run Auto Sync once files are ready.",
                    stale_reasons.join("; ")
                ));
            } else {
                let mut report_mismatch: Option<String> = None;
                if let Some(ref_res) = &reference {
                    if let Some(saved_hash) = &ref_res.saved.sha256 {
                        if !report.reference.sha256.is_empty()
                            && &report.reference.sha256 != saved_hash
                        {
                            report_mismatch = Some(format!(
                                "reference report digest does not match {}",
                                ref_res.saved.file_name
                            ));
                        }
                    }
                }
                if report_mismatch.is_none() {
                    for (alignment, target_res) in report.sources.iter().zip(targets.iter()) {
                        if let Some(saved_hash) = &target_res.saved.sha256 {
                            if !alignment.source.sha256.is_empty()
                                && &alignment.source.sha256 != saved_hash
                            {
                                report_mismatch = Some(format!(
                                    "cached alignment digest does not match {}",
                                    target_res.saved.file_name
                                ));
                                break;
                            }
                        }
                    }
                }
                if let Some(reason) = report_mismatch {
                    valid_report = None;
                    notices.push(format!(
                        "Cached sync results were invalidated ({reason}). Re-run Auto Sync."
                    ));
                } else {
                    // Rebind cached report paths to the currently resolved project files so moved
                    // or copied project folders never read from stale original directories.
                    if let Some(ref_res) = &reference {
                        report.reference.path = ref_res.resolved_path.clone();
                    }
                    for (alignment, target_res) in report.sources.iter_mut().zip(targets.iter()) {
                        alignment.source.path = target_res.resolved_path.clone();
                    }
                }
            }
        }

        Ok(LoadedProject {
            project,
            reference,
            targets,
            valid_report,
            notices,
        })
    }
}

/// Resolve one media reference against the project directory and verify its digest.
pub fn resolve_media(project_dir: Option<&Path>, saved: &ProjectMediaRef) -> ResolvedMedia {
    let mut candidate_rel: Option<PathBuf> = None;
    if let (Some(dir), Some(rel)) = (project_dir, &saved.relative_path) {
        let joined = clean_path(&dir.join(rel));
        if joined.is_file() {
            candidate_rel = Some(joined);
        }
    }

    let candidate_abs = saved
        .absolute_path
        .is_file()
        .then(|| saved.absolute_path.clone());

    let (resolved_path, via_relative) = if let Some(rel_path) = candidate_rel {
        // Prefer the path relative to the `.stagesync` file so copied/moved project
        // folders stay self-contained instead of reading stale originals.
        let differed = candidate_abs
            .as_ref()
            .map(|abs| abs != &rel_path)
            .unwrap_or(true);
        (rel_path, differed)
    } else if let Some(abs_path) = candidate_abs {
        (abs_path, false)
    } else {
        return ResolvedMedia {
            resolved_path: saved.absolute_path.clone(),
            status: MediaStatus::Missing,
            saved: saved.clone(),
        };
    };

    let status = verify_resolved_file(&resolved_path, saved, via_relative);
    ResolvedMedia {
        resolved_path,
        status,
        saved: saved.clone(),
    }
}

/// Verify an existing file's size and SHA-256 against the saved snapshot.
pub fn verify_resolved_file(
    path: &Path,
    saved: &ProjectMediaRef,
    via_relative: bool,
) -> MediaStatus {
    if let Some(expected_len) = saved.byte_len {
        match fs::metadata(path) {
            Ok(meta) if meta.len() != expected_len => {
                return MediaStatus::ModifiedOnDisk {
                    detail: format!(
                        "file size changed ({} bytes vs expected {} bytes)",
                        meta.len(),
                        expected_len
                    ),
                };
            }
            Err(err) => {
                return MediaStatus::ModifiedOnDisk {
                    detail: format!("could not stat file: {err}"),
                };
            }
            Ok(_) => {}
        }
    }

    if let Some(expected_hash) = &saved.sha256 {
        match sha256_file(path) {
            Ok(actual_hash) if &actual_hash != expected_hash => {
                return MediaStatus::ModifiedOnDisk {
                    detail: "SHA-256 content hash changed on disk".to_string(),
                };
            }
            Err(err) => {
                return MediaStatus::ModifiedOnDisk {
                    detail: format!("could not hash file: {err}"),
                };
            }
            Ok(_) => {}
        }
    }

    if via_relative {
        MediaStatus::RelocatedByRelativePath
    } else {
        MediaStatus::Verified
    }
}

fn record_media_notices(
    label: &str,
    resolved: &ResolvedMedia,
    notices: &mut Vec<String>,
    stale_reasons: &mut Vec<String>,
) {
    match &resolved.status {
        MediaStatus::Verified => {}
        MediaStatus::RelocatedByRelativePath => {
            notices.push(format!(
                "{label}: resolved via project-relative path {}",
                resolved.resolved_path.display()
            ));
        }
        MediaStatus::ModifiedOnDisk { detail } => {
            notices.push(format!("{label}: {detail}"));
            stale_reasons.push(format!("{label} modified"));
        }
        MediaStatus::Missing => {
            notices.push(format!(
                "{label}: missing at {}",
                resolved.saved.absolute_path.display()
            ));
            stale_reasons.push(format!("{label} missing"));
        }
    }
}

/// Atomic file write using a temporary sibling file and `fs::rename`.
pub fn atomic_write_bytes(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create parent dir {}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");
    // Exclusive creation is essential: a pre-existing sibling can itself be a
    // loaded recording (or an alias of one). Never truncate it, even on a retry.
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let (tmp, mut file) = loop {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(".{file_name}.{}.{serial}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => break (tmp, file),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err).context("create exclusive project staging file"),
        }
    };
    let result = (|| -> Result<()> {
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    Ok(())
}

/// Directory for autosave crash-recovery snapshots.
pub fn autosave_dir() -> PathBuf {
    platform::config_dir().join("autosave")
}

/// Path to the crash-recovery `.stagesync` snapshot.
pub fn recovery_project_file() -> PathBuf {
    autosave_dir().join("recovery.stagesync")
}

/// Path to the crash-recovery metadata sidecar.
pub fn recovery_meta_file() -> PathBuf {
    autosave_dir().join("recovery_meta.json")
}

/// Write an atomic crash-recovery snapshot and metadata sidecar.
pub fn write_recovery(
    project: &StageSyncProject,
    active_project_path: Option<&Path>,
    dirty: bool,
) -> Result<()> {
    let dir = autosave_dir();
    fs::create_dir_all(&dir)?;
    project.save_to(&recovery_project_file())?;
    let meta = RecoveryMeta {
        active_project_path: active_project_path.map(PathBuf::from),
        saved_at_unix_secs: unix_now_secs(),
        dirty,
    };
    let meta_json = serde_json::to_string_pretty(&meta)?;
    atomic_write_bytes(&recovery_meta_file(), meta_json.as_bytes())?;
    Ok(())
}

/// Inspect whether a non-empty crash-recovery snapshot is waiting on disk.
pub fn load_recovery_meta() -> Option<RecoveryMeta> {
    let proj_path = recovery_project_file();
    if !proj_path.is_file() {
        return None;
    }
    let meta_raw = fs::read_to_string(recovery_meta_file()).ok()?;
    serde_json::from_str::<RecoveryMeta>(&meta_raw).ok()
}

/// Load the saved crash-recovery project.
pub fn load_recovery_project() -> Result<(LoadedProject, Option<RecoveryMeta>)> {
    let meta = load_recovery_meta();
    let loaded = StageSyncProject::load_from(&recovery_project_file())?;
    Ok((loaded, meta))
}

/// Delete crash-recovery files (called on clean shutdown or user discard).
pub fn clear_recovery() {
    let _ = fs::remove_file(recovery_project_file());
    let _ = fs::remove_file(recovery_meta_file());
}

/// Current Unix timestamp in seconds.
pub fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Normalize `.` and `..` components purely lexically without requiring the path to exist.
pub fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(comp);
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn make_absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        clean_path(path)
    } else if let Ok(cwd) = std::env::current_dir() {
        clean_path(&cwd.join(path))
    } else {
        path.to_path_buf()
    }
}

/// Compute a relative path from directory `base` to `target` when both share a root/prefix.
pub fn relative_path(target: &Path, base: &Path) -> Option<PathBuf> {
    let target = clean_path(target);
    let base = clean_path(base);
    let t_comps: Vec<Component<'_>> = target.components().collect();
    let b_comps: Vec<Component<'_>> = base.components().collect();

    // Ensure identical drive/prefix/root on Windows/Unix.
    if t_comps.first() != b_comps.first() {
        return None;
    }

    let mut common = 0usize;
    while common < t_comps.len() && common < b_comps.len() && t_comps[common] == b_comps[common] {
        common += 1;
    }
    if common == 0 {
        return None;
    }

    let mut rel = PathBuf::new();
    for _ in common..b_comps.len() {
        rel.push("..");
    }
    for comp in &t_comps[common..] {
        rel.push(comp);
    }
    Some(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_computes_sibling_and_nested_moves() {
        let proj_dir = Path::new("/tmp/concert/edits");
        let audio = Path::new("/tmp/concert/audio/mic1.wav");
        let rel = relative_path(audio, proj_dir).expect("relative path");
        assert_eq!(rel, PathBuf::from("../audio/mic1.wav"));
        let joined = clean_path(&proj_dir.join(&rel));
        assert_eq!(joined, PathBuf::from("/tmp/concert/audio/mic1.wav"));
    }

    #[test]
    fn project_save_and_load_detects_missing_and_modified_files() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let ref_path = dir.path().join("ref.wav");
        let tgt_path = dir.path().join("tgt.wav");
        fs::write(&ref_path, b"REF_BYTES_V1").unwrap();
        fs::write(&tgt_path, b"TGT_BYTES_V1").unwrap();

        let proj_path = dir.path().join("show.stagesync");
        let proj_dir = proj_path.parent();
        let ref_entry = FileEntry::new(ref_path.clone());
        let tgt_entry = FileEntry::new(tgt_path.clone());

        let project = StageSyncProject {
            version: PROJECT_FORMAT_VERSION,
            saved_at_unix_secs: 12345,
            reference: Some(ProjectMediaRef::from_entry(&ref_entry, proj_dir)),
            targets: vec![ProjectTargetEntry {
                media: ProjectMediaRef::from_entry(&tgt_entry, proj_dir),
                anchor: ProjectAnchor {
                    source_time: 1.0,
                    reference_time: 2.5,
                    active: true,
                },
                pending_nudge_secs: 0.015,
            }],
            analysis: ProjectAnalysisOptions::default(),
            report: None,
            mixer: ProjectMixerState::default(),
            timeline: ProjectTimelineState::default(),
            export: ProjectExportSettings::from(&ExportSettings::default()),
        };

        project.save_to(&proj_path).expect("save project");
        assert!(!dir.path().join(".show.stagesync").exists());

        let loaded = StageSyncProject::load_from(&proj_path).expect("load project");
        assert_eq!(
            loaded.reference.as_ref().unwrap().status,
            MediaStatus::Verified
        );
        assert_eq!(loaded.targets[0].status, MediaStatus::Verified);

        // Modifying target bytes triggers ModifiedOnDisk.
        fs::write(&tgt_path, b"TGT_BYTES_V2_MODIFIED").unwrap();
        let modified = StageSyncProject::load_from(&proj_path).expect("reload");
        assert!(matches!(
            modified.targets[0].status,
            MediaStatus::ModifiedOnDisk { .. }
        ));

        // Deleting target triggers Missing.
        fs::remove_file(&tgt_path).unwrap();
        let missing = StageSyncProject::load_from(&proj_path).expect("reload");
        assert_eq!(missing.targets[0].status, MediaStatus::Missing);
    }

    #[test]
    fn moved_project_folder_resolves_via_relative_path() {
        let root = tempfile::TempDir::new().expect("temp dir");
        let orig = root.path().join("orig");
        let moved = root.path().join("moved");
        fs::create_dir_all(&orig).unwrap();
        fs::create_dir_all(&moved).unwrap();

        let ref_orig = orig.join("ref.wav");
        fs::write(&ref_orig, b"AUDIO_DATA_123").unwrap();
        let proj_orig = orig.join("mix.stagesync");

        let project = StageSyncProject {
            version: PROJECT_FORMAT_VERSION,
            saved_at_unix_secs: 1,
            reference: Some(ProjectMediaRef::from_entry(
                &FileEntry::new(ref_orig.clone()),
                proj_orig.parent(),
            )),
            targets: Vec::new(),
            analysis: ProjectAnalysisOptions::default(),
            report: None,
            mixer: ProjectMixerState::default(),
            timeline: ProjectTimelineState::default(),
            export: ProjectExportSettings::from(&ExportSettings::default()),
        };
        project.save_to(&proj_orig).unwrap();

        // Copy project and audio into `moved`, remove `orig`.
        let ref_moved = moved.join("ref.wav");
        let proj_moved = moved.join("mix.stagesync");
        fs::copy(&ref_orig, &ref_moved).unwrap();
        fs::copy(&proj_orig, &proj_moved).unwrap();
        fs::remove_dir_all(&orig).unwrap();

        let loaded = StageSyncProject::load_from(&proj_moved).expect("load moved");
        let r = loaded.reference.expect("ref resolved");
        assert_eq!(r.resolved_path, ref_moved);
        assert_eq!(r.status, MediaStatus::RelocatedByRelativePath);
    }
}
