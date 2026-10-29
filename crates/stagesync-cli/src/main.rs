//! **StageSync** — command-line interface.
//!
//! ```text
//! stagesync analyze --reference REF --source A [--source B] --out DIR
//! stagesync export  --reference REF --source A [--source B] --out DIR [--layout ...]
//! stagesync verify  --corrected FILE --reference FILE --offset SEC --scale F --duration SEC
//! stagesync info    FILE
//! ```
//!
//! `analyze` and `export` write the same machine-readable artefacts, so a run can be
//! inspected before any audio is rendered.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use stagesync_core::audio::{AudioDecoder, WavFormat};
use stagesync_core::export::{self, Artifact};
use stagesync_core::matching::AlignmentReport;
use stagesync_core::pipeline::{analyze, AnalysisConfig, ManualAnchor};
use stagesync_core::render::{self, Layout, RenderOptions, RenderResult};
use stagesync_core::verify::{verify_corrected, HoldoutResult, VerifyConfig};

#[derive(Parser, Debug)]
#[command(
    name = "stagesync",
    version,
    about = "Drift-aware synchronisation of multiple recordings of the same performance",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Analyse a reference against one or more sources and write the sync map.
    Analyze {
        /// Reference (guide) recording.
        #[arg(long, value_name = "FILE")]
        reference: PathBuf,
        /// A source recording to align. May be repeated.
        #[arg(long, value_name = "FILE", required = true)]
        source: Vec<PathBuf>,
        /// Directory for the artefacts.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        /// Skip the independent log-mel cross-check (faster, less evidence).
        #[arg(long)]
        no_log_mel: bool,
        /// Skip the raw-waveform drift measurement.
        ///
        /// The waveform measurement is an independent second opinion on the drift,
        /// taken at positions the fit did not use. Turning it off makes the drift rest
        /// on the envelope measurement alone.
        #[arg(long)]
        no_waveform_drift: bool,
        /// Correct the position but leave the rate alone.
        ///
        /// Nothing is resampled. The report still records what the drift evidence was.
        #[arg(long)]
        offset_only: bool,
        /// Manual anchor: source time in seconds.
        #[arg(long, value_name = "SEC", requires = "anchor_reference")]
        anchor_source: Option<f64>,
        /// Manual anchor: reference time in seconds.
        #[arg(long, value_name = "SEC", requires = "anchor_source")]
        anchor_reference: Option<f64>,
    },
    /// Analyse, then render corrected audio.
    Export {
        /// Reference (guide) recording.
        #[arg(long, value_name = "FILE")]
        reference: PathBuf,
        /// A source recording to align. May be repeated.
        #[arg(long, value_name = "FILE", required = true)]
        source: Vec<PathBuf>,
        /// Directory for the artefacts.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        /// Output layout.
        #[arg(long, value_enum, default_value_t = LayoutArg::SourceAligned)]
        layout: LayoutArg,
        /// Output sample rate (default: the source rate, or the reference rate for timelines).
        #[arg(long, value_name = "HZ")]
        sample_rate: Option<u32>,
        /// Output sample format.
        #[arg(long, value_enum, default_value_t = FormatArg::Float32)]
        format: FormatArg,
        /// Skip the independent log-mel cross-check.
        #[arg(long)]
        no_log_mel: bool,
        /// Skip the raw-waveform drift measurement.
        #[arg(long)]
        no_waveform_drift: bool,
        /// Correct the position but leave the rate alone.
        #[arg(long)]
        offset_only: bool,
        /// Skip the hold-out verification of the rendered audio.
        #[arg(long)]
        no_verify: bool,
    },
    /// Independently verify a rendered correction against the reference.
    Verify {
        /// The rendered (source-aligned) WAV.
        #[arg(long, value_name = "FILE")]
        corrected: PathBuf,
        /// The reference WAV.
        #[arg(long, value_name = "FILE")]
        reference: PathBuf,
        /// Placement offset of the corrected file on the reference timeline, in seconds.
        #[arg(long, value_name = "SEC")]
        offset: f64,
        /// Time scale that was applied.
        #[arg(long, value_name = "F", default_value_t = 1.0)]
        scale: f64,
        /// Duration of the corrected source, in seconds.
        #[arg(long, value_name = "SEC")]
        duration: f64,
        /// Rate of the waveform comparison, in Hz.
        #[arg(long, value_name = "HZ", default_value_t = 16_000)]
        analysis_rate: u32,
    },
    /// Print metadata for an audio file.
    Info {
        /// The file to inspect.
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// Print the level profile of an audio file over a span.
    ///
    /// This is the tool for checking whether a reported gap is really a gap: it shows
    /// the level of the recording itself, so a hole in the coverage can be confirmed
    /// against the audio rather than taken on trust.
    Level {
        /// The file to inspect.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Start of the span, in seconds (default: the start of the file).
        #[arg(long, value_name = "SEC")]
        from: Option<f64>,
        /// End of the span, in seconds (default: the end of the file).
        #[arg(long, value_name = "SEC")]
        to: Option<f64>,
        /// Length of each block in the profile, in milliseconds.
        #[arg(long, value_name = "MS", default_value_t = 250.0)]
        block_ms: f64,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum LayoutArg {
    /// One corrected file per source, keeping the source's own length and rate.
    SourceAligned,
    /// One reference-length timeline per source, silence where the source is absent.
    TimelinePerSource,
    /// One merged reference-length timeline covering every source.
    TimelineMerged,
}

impl From<LayoutArg> for Layout {
    fn from(value: LayoutArg) -> Self {
        match value {
            LayoutArg::SourceAligned => Layout::SourceAligned,
            LayoutArg::TimelinePerSource => Layout::ReferenceTimelinePerSource,
            LayoutArg::TimelineMerged => Layout::ReferenceTimelineMerged,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum FormatArg {
    /// 32-bit float.
    Float32,
    /// 16-bit signed PCM.
    Int16,
}

impl From<FormatArg> for WavFormat {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Float32 => WavFormat::Float32,
            FormatArg::Int16 => WavFormat::Int16,
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze {
            reference,
            source,
            out,
            no_log_mel,
            no_waveform_drift,
            offset_only,
            anchor_source,
            anchor_reference,
        } => {
            let anchor = match (anchor_source, anchor_reference) {
                (Some(s), Some(r)) => Some(ManualAnchor {
                    source_time: s,
                    reference_time: r,
                }),
                _ => None,
            };
            let report = run_analysis(
                &reference,
                &source,
                no_log_mel,
                no_waveform_drift,
                offset_only,
                anchor,
            )?;
            let artefacts = write_artefacts(&out, &report, &[], &[])?;
            println!("{}", summarise(&report));
            for a in &artefacts {
                println!("wrote {}", a.path.display());
            }
            Ok(())
        }
        Command::Export {
            reference,
            source,
            out,
            layout,
            sample_rate,
            format,
            no_log_mel,
            no_waveform_drift,
            offset_only,
            no_verify,
        } => {
            let mut report = run_analysis(
                &reference,
                &source,
                no_log_mel,
                no_waveform_drift,
                offset_only,
                None,
            )?;
            let mut options = RenderOptions {
                layout: layout.into(),
                sample_rate,
                format: format.into(),
                ..RenderOptions::default()
            };
            if options.layout != Layout::SourceAligned && options.sample_rate.is_none() {
                options.sample_rate = Some(report.reference.sample_rate);
            }

            std::fs::create_dir_all(&out)
                .with_context(|| format!("failed to create output directory: {}", out.display()))?;

            let mut results: Vec<RenderResult> = Vec::new();
            let mut holdouts: Vec<(usize, HoldoutResult)> = Vec::new();
            match options.layout {
                Layout::SourceAligned => {
                    for (index, alignment) in report.sources.iter().enumerate() {
                        let Some(model) = alignment.model() else {
                            eprintln!(
                                "skipping {}: no reliable alignment was established",
                                alignment.source.file_name
                            );
                            continue;
                        };
                        let path = out.join(format!(
                            "{}_corrected.wav",
                            stem(&alignment.source.file_name)
                        ));
                        render::prepare_output(&path)?;
                        let result = render::render_source_aligned(
                            &alignment.source.path,
                            &model,
                            &path,
                            &options,
                        )
                        .with_context(|| format!("failed to render {}", path.display()))?;
                        println!(
                            "rendered {} -> {} ({:.2} s, drift {:+.3} ppm, pitch {:+.4} cents)",
                            alignment.source.file_name,
                            path.display(),
                            result.frames as f64 / result.sample_rate as f64,
                            model.drift_ppm(),
                            result.pitch_shift_cents
                        );

                        // The hold-out verification runs on the *rendered* file with a
                        // different method (waveform GCC-PHAT at disjoint positions), so
                        // it is an independent check rather than the fitter confirming
                        // itself. It is recorded in report.json.
                        if !no_verify {
                            match verify_corrected(
                                &path,
                                &reference,
                                &model,
                                alignment.source.duration_secs,
                                &VerifyConfig {
                                    expected_pitch_cents: Some(model.pitch_shift_cents()),
                                    ..VerifyConfig::default()
                                },
                            ) {
                                Ok(result) => {
                                    println!(
                                        "  hold-out check (waveform GCC-PHAT): {} of {} positions trusted ({} disputed, excluded), residual p50 {:+.3} ms, p95 {:+.3} ms, max {:.3} ms",
                                        result.counted,
                                        result.points.len(),
                                        result.disputed,
                                        result.p50_ms,
                                        result.p95_ms,
                                        result.max_ms
                                    );
                                    println!(
                                        "  hold-out check (RMS envelope):     {} of {} points measured ({} outliers discarded), residual p50 {:+.3} ms, p95 {:+.3} ms, max {:.3} ms",
                                        result.envelope_counted,
                                        result.points.len(),
                                        result.envelope_rejected,
                                        result.envelope_p50_ms,
                                        result.envelope_p95_ms,
                                        result.envelope_max_ms
                                    );
                                    println!(
                                        "  measured pitch change: {:+.4} cents",
                                        result.pitch_cents
                                    );
                                    for note in &result.notes {
                                        println!("  note: {note}");
                                    }
                                    holdouts.push((index, result));
                                }
                                Err(error) => {
                                    eprintln!(
                                        "hold-out verification of {} failed: {error:#}",
                                        path.display()
                                    );
                                }
                            }
                        }
                        results.push(result);
                    }
                }
                Layout::ReferenceTimelinePerSource => {
                    for alignment in report.sources.iter() {
                        let Some(model) = alignment.model() else {
                            continue;
                        };
                        let path = out.join(format!(
                            "{}_timeline.wav",
                            stem(&alignment.source.file_name)
                        ));
                        render::prepare_output(&path)?;
                        let result = render::render_reference_timeline(
                            &alignment.source.path,
                            &model,
                            report.timeline_duration,
                            &path,
                            &options,
                        )
                        .with_context(|| format!("failed to render {}", path.display()))?;
                        println!(
                            "rendered timeline {} -> {} ({:.1} s of audio, {:.1} s of silence placeholder)",
                            alignment.source.file_name,
                            path.display(),
                            result.source_frames as f64 / result.sample_rate as f64,
                            result.silence_frames as f64 / result.sample_rate as f64
                        );
                        results.push(result);
                    }
                }
                Layout::ReferenceTimelineMerged => {
                    let sources: Vec<(PathBuf, stagesync_core::matching::LinearModel)> = report
                        .sources
                        .iter()
                        .filter_map(|a| a.model().map(|m| (a.source.path.clone(), m)))
                        .collect();
                    if sources.is_empty() {
                        anyhow::bail!("no source could be aligned, so no timeline was rendered");
                    }
                    let path = out.join("timeline_merged.wav");
                    render::prepare_output(&path)?;
                    let result = render::render_merged_timeline(
                        &sources,
                        report.timeline_duration,
                        &path,
                        &options,
                    )?;
                    println!(
                        "rendered merged timeline -> {} ({:.1} s of audio, {:.1} s of silence placeholder)",
                        path.display(),
                        result.source_frames as f64 / result.sample_rate as f64,
                        result.silence_frames as f64 / result.sample_rate as f64
                    );
                    results.push(result);
                }
            }

            // Record the hold-out verification in the report so that it ships with the
            // artefacts rather than only on the terminal.
            for (index, holdout) in holdouts {
                if let Some(alignment) = report.sources.get_mut(index) {
                    alignment.holdout = Some(holdout);
                }
            }

            let artefacts = write_artefacts(&out, &report, &results, &[])?;
            for a in &artefacts {
                println!("wrote {}", a.path.display());
            }
            Ok(())
        }
        Command::Verify {
            corrected,
            reference,
            offset,
            scale,
            duration,
            analysis_rate,
        } => {
            let model = stagesync_core::matching::LinearModel::new(scale, offset);
            let config = VerifyConfig {
                analysis_rate,
                expected_pitch_cents: Some(model.pitch_shift_cents()),
                ..VerifyConfig::default()
            };
            let result = verify_corrected(&corrected, &reference, &model, duration, &config)?;
            println!("hold-out verification of {}", corrected.display());
            println!(
                "  trusted positions : {} of {} ({} disputed, excluded)",
                result.counted,
                result.points.len(),
                result.disputed
            );
            println!("  residual p50     : {:+.3} ms", result.p50_ms);
            println!("  residual p95     : {:+.3} ms", result.p95_ms);
            println!("  residual max     : {:+.3} ms", result.max_ms);
            println!(
                "  unfiltered p50/p95/max: {:+.3} / {:+.3} / {:.3} ms ({} points)",
                result.raw_p50_ms, result.raw_p95_ms, result.raw_max_ms, result.raw_counted
            );
            println!("  pitch change     : {:+.4} cents", result.pitch_cents);
            match (result.drift_error_ppm, result.drift_error_se_ppm) {
                (Some(error), Some(error_se)) => println!(
                    "  drift error      : {:+.3} ppm ± {:.3} ppm (from the slope of the hold-out residuals)",
                    error, error_se
                ),
                _ => println!(
                    "  drift error      : not established (too few mutually corroborated positions)"
                ),
            }
            println!(
                "  envelope p50/p95/max: {:+.3} / {:+.3} / {:.3} ms ({} points, {} outliers discarded)",
                result.envelope_p50_ms,
                result.envelope_p95_ms,
                result.envelope_max_ms,
                result.envelope_counted,
                result.envelope_rejected
            );
            for p in &result.points {
                println!(
                    "    t={:>9.3} s  residual {:+8.3} ms  peak {:.4}  z {:.2}  env {:+8.3} ms{}",
                    p.reference_time,
                    p.residual_ms,
                    p.phat_peak,
                    p.phat_zscore,
                    p.envelope_residual_ms.unwrap_or(f64::NAN),
                    if p.counted { "" } else { "   (not counted)" }
                );
            }
            for n in &result.notes {
                println!("  note: {n}");
            }
            Ok(())
        }
        Command::Info { file } => {
            let decoder = AudioDecoder::open(&file)?;
            let meta = decoder.meta();
            println!("file        : {}", meta.path.display());
            println!("sha256      : {}", meta.sha256);
            println!("size        : {} bytes", meta.byte_len);
            println!("codec       : {}", meta.codec);
            println!("sample rate : {} Hz", meta.sample_rate);
            println!("channels    : {}", meta.channels);
            println!("duration    : {:.3} s", meta.duration_secs);
            println!("frames      : {}", meta.frames);
            println!(
                "sample fmt  : {}{}",
                meta.sample_format.clone().unwrap_or_else(|| "?".into()),
                if meta.float_format { " (float)" } else { "" }
            );
            Ok(())
        }
        Command::Level {
            file,
            from,
            to,
            block_ms,
        } => {
            print_level_profile(&file, from, to, block_ms)?;
            Ok(())
        }
    }
}

/// Print the level profile of a file over a span.
///
/// The blocks are printed with their level relative to the loudest block, so a hole
/// in a recording is visible as a run of blocks that sit at the noise floor rather
/// than at the programme level.
fn print_level_profile(
    file: &Path,
    from: Option<f64>,
    to: Option<f64>,
    block_ms: f64,
) -> Result<()> {
    let block_ms = block_ms.max(1.0);
    let mut decoder = AudioDecoder::open(file)?;
    let meta = decoder.meta().clone();
    let start = from.unwrap_or(0.0).max(0.0);
    let end = to.unwrap_or(meta.duration_secs).min(meta.duration_secs);
    anyhow::ensure!(
        end > start,
        "the requested span is empty: {start:.3} s to {end:.3} s"
    );

    let block_frames = ((meta.sample_rate as f64) * block_ms / 1000.0).round() as usize;
    anyhow::ensure!(block_frames > 0, "the block length is too short to measure");

    let mut levels: Vec<(f64, f64)> = Vec::new();
    let mut position = 0.0f64;
    let mut block: Vec<f32> = Vec::with_capacity(block_frames);
    decoder.for_each_block(block_frames.max(1024), |decoded| {
        for sample in decoded.mono() {
            block.push(sample);
            if block.len() >= block_frames {
                let sum: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
                let rms = (sum / block.len() as f64).sqrt();
                levels.push((position, rms));
                position += block_ms / 1000.0;
                block.clear();
            }
        }
        Ok(())
    })?;
    if !block.is_empty() {
        let sum: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
        let rms = (sum / block.len() as f64).sqrt();
        levels.push((position, rms));
    }

    let peak = levels.iter().map(|(_, r)| *r).fold(0.0f64, f64::max);
    let mean = if levels.is_empty() {
        0.0
    } else {
        levels.iter().map(|(_, r)| *r).sum::<f64>() / levels.len() as f64
    };
    // A low percentile, not the tenth: music has no silence, so a low percentile lands
    // on the recorder's own noise floor rather than on a quiet passage.
    let mut sorted: Vec<f64> = levels.iter().map(|(_, r)| *r).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = if sorted.is_empty() {
        0.0
    } else {
        sorted[(sorted.len() as f64 * 0.02) as usize % sorted.len().max(1)]
    };

    println!(
        "file    : {} ({:.3} s, {} Hz, {} ch)",
        meta.file_name, meta.duration_secs, meta.sample_rate, meta.channels
    );
    println!("span    : {start:.3} s to {end:.3} s");
    println!("block   : {block_ms:.0} ms, {} blocks", levels.len());
    println!(
        "level   : peak {:.1} dBFS, mean {:.1} dBFS, noise floor (2nd percentile) {:.1} dBFS",
        to_db(peak),
        to_db(mean),
        to_db(floor)
    );
    println!("profile :");
    for (time, rms) in &levels {
        if *time < start || *time > end {
            continue;
        }
        let db = to_db(*rms);
        let width = ((db + 60.0).max(0.0) / 60.0 * 60.0).round() as usize;
        println!(
            "  {:>10.3} s  {:>7.1} dBFS  |{:<60}|",
            time,
            db,
            "#".repeat(width)
        );
    }
    Ok(())
}

/// A level in decibels relative to full scale.
fn to_db(amplitude: f64) -> f64 {
    if amplitude <= 0.0 {
        -120.0
    } else {
        20.0 * amplitude.log10()
    }
}

/// Run the analysis with progress reporting.
fn run_analysis(
    reference: &Path,
    sources: &[PathBuf],
    no_log_mel: bool,
    no_waveform_drift: bool,
    offset_only: bool,
    anchor: Option<ManualAnchor>,
) -> Result<AlignmentReport> {
    let mut config = AnalysisConfig {
        compute_log_mel: !no_log_mel,
        measure_waveform_drift: !no_waveform_drift,
        force_offset_only: offset_only,
        manual_anchor: anchor,
        ..AnalysisConfig::default()
    };
    // A manual anchor means the automatic lock is not to be trusted, so widen the fine search.
    if anchor.is_some() {
        config.fine.search_secs = 10.0;
    }

    let started = Instant::now();
    let mut progress = |p: stagesync_core::pipeline::Progress| {
        println!(
            "[{:>5.1}%] {} — {}",
            p.fraction * 100.0,
            p.stage.label(),
            p.message
        );
    };
    let report = analyze(reference, sources, &config, &mut progress)?;
    println!(
        "analysis finished in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    Ok(report)
}

/// Write every machine-readable artefact.
fn write_artefacts(
    out: &Path,
    report: &AlignmentReport,
    renders: &[RenderResult],
    extra: &[Artifact],
) -> Result<Vec<Artifact>> {
    std::fs::create_dir_all(out)
        .with_context(|| format!("failed to create output directory: {}", out.display()))?;

    let mut artefacts: Vec<Artifact> = Vec::new();

    let syncmap = out.join("syncmap.csv");
    export::write_syncmap_csv(&syncmap, report)?;
    artefacts.push(artifact("syncmap", &syncmap)?);

    let segments = out.join("segments.csv");
    export::write_segments_csv(&segments, report)?;
    artefacts.push(artifact("segments", &segments)?);

    let graph = out.join("drift_graph.svg");
    export::write_drift_graph(&graph, report)?;
    artefacts.push(artifact("drift_graph", &graph)?);

    let summary = out.join("timeline.txt");
    export::write_timeline_summary(&summary, report)?;
    artefacts.push(artifact("timeline_summary", &summary)?);

    let plan = out.join("plan.json");
    let plan_json = serde_json::to_string_pretty(report)?;
    std::fs::write(&plan, plan_json)?;
    artefacts.push(artifact("plan", &plan)?);

    let report_path = out.join("report.json");
    export::write_report_json(&report_path, report, artefacts.clone())?;
    artefacts.push(artifact("report", &report_path)?);

    for r in renders {
        artefacts.push(artifact("corrected_audio", &r.path)?);
    }
    artefacts.extend(extra.iter().cloned());
    Ok(artefacts)
}

/// Describe a written file.
///
/// The SHA-256 is recorded as well as the size, so that "the artefacts are
/// byte-identical between two runs" is a checkable statement rather than a
/// claim: the hash of each output is in `report.json`.
fn artifact(kind: &str, path: &Path) -> Result<Artifact> {
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(Artifact {
        kind: kind.to_string(),
        path: path.to_path_buf(),
        bytes,
        sha256: stagesync_core::audio::hash::sha256_file(path).ok(),
    })
}

/// A short human-readable summary.
fn summarise(report: &AlignmentReport) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "reference: {} ({:.2} s, {} Hz, {} ch)\n",
        report.reference.file_name,
        report.reference.duration_secs,
        report.reference.sample_rate,
        report.reference.channels
    ));
    for source in &report.sources {
        let model = source.model();
        match model {
            Some(m) => text.push_str(&format!(
                "{}: offset {:+.3} s, drift {:+.3} ppm, {:.1}% covered, {} points ({} skipped), {} warnings\n",
                source.source.file_name,
                m.offset,
                m.drift_ppm(),
                source.coverage.fraction * 100.0,
                source.points.len(),
                source.fine_skipped,
                source.warnings.len()
            )),
            None => text.push_str(&format!(
                "{}: no reliable alignment\n",
                source.source.file_name
            )),
        }
        if let (Some(d), Some(c)) = (source.direct_model, source.calibrated_model) {
            if (d.scale - c.scale).abs() > 1e-12 {
                text.push_str(&format!(
                    "    note: direct drift {:+.3} ppm vs calibrated {:+.3} ppm (difference {:+.3} ppm)\n",
                    d.drift_ppm(),
                    c.drift_ppm(),
                    c.drift_ppm() - d.drift_ppm()
                ));
            }
        }
        text.push_str(&format!(
            "    fine probes: {} measured, {} skipped (quiet {}, no region {}, no peak {})\n",
            source.fine_measured,
            source.fine_skipped,
            source.fine_skip_reasons.low_energy,
            source.fine_skip_reasons.no_region,
            source.fine_skip_reasons.no_peak
        ));
        if let Some(assessment) = &source.drift {
            text.push_str(&drift_verdict(assessment));
        }
        if let Some(lock) = &source.coarse {
            for c in lock.candidates.iter().take(5) {
                text.push_str(&format!(
                    "    coarse candidate: lag {:+.3} s, {} vote(s), mean score {:.3}\n",
                    c.lag_secs, c.votes, c.mean_score
                ));
            }
            if let Some(whole) = lock.full_range_lag_secs {
                text.push_str(&format!(
                    "    whole-file correlation: lag {:+.3} s, score {:.3}\n",
                    whole, lock.full_range_score
                ));
            }
        }
        for w in &source.warnings {
            text.push_str(&format!("    warning: {w}\n"));
        }
    }
    let uncovered: f64 = report
        .timeline
        .iter()
        .filter(|s| s.kind == stagesync_core::matching::SegmentKind::UncoveredReference)
        .map(|s| s.reference_end - s.reference_start)
        .sum();
    text.push_str(&format!(
        "timeline: {:.2} s total, {:.2} s with no recorder coverage\n",
        report.timeline_duration, uncovered
    ));
    text
}

/// Describe the drift verdict in plain language.
///
/// The verdict is the part of the report that says whether the drift was *measured* or
/// only guessed, so it is spelled out rather than reduced to a number.
fn drift_verdict(assessment: &stagesync_core::matching::DriftAssessment) -> String {
    use stagesync_core::matching::{DriftMethod, DriftStatus};

    let mut text = String::new();
    match assessment.status {
        DriftStatus::OffsetOnly => {
            text.push_str(
                "    drift: NOT established — only the offset was corrected (no rate change)\n",
            );
        }
        DriftStatus::Established => {
            text.push_str(&format!(
                "    drift: established by the {} method, {:+.3} ppm ± {:.3} ppm, \
                 applied over {} measurement(s)\n",
                assessment.method.label(),
                assessment.drift_ppm,
                assessment.slope_se_ppm,
                assessment.points_used
            ));
        }
    }

    for (label, figures) in [
        ("envelope", assessment.envelope.as_ref()),
        ("waveform", assessment.waveform.as_ref()),
    ] {
        let Some(f) = figures else { continue };
        let correlation = match f.mean_correlation {
            Some(c) => format!(", mean waveform correlation {c:.2}"),
            None => String::new(),
        };
        text.push_str(&format!(
            "      {label} estimate: {:+.3} ppm ± {:.3} ppm, residual {:.2} ms, {} measurement(s){correlation}\n",
            f.drift_ppm, f.slope_se_ppm, f.residual_rms_ms, f.points_used
        ));
    }

    if assessment.method == DriftMethod::OffsetOnly {
        text.push_str("      the drift evidence was not precise enough to apply a rate change\n");
    }
    for note in &assessment.notes {
        text.push_str(&format!("      note: {note}\n"));
    }
    text
}

/// File name without extension.
fn stem(name: &str) -> String {
    let base = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    base.replace(' ', "_")
}
