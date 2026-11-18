# StageSync M22 — Implementation & Release Hardening Summary

This document records the changes implemented, bugs repaired, tests added, and platform packaging checks completed in Milestone M22 (`stagesync_m22_release_hardening`).

---

## 1. Real `.stagesync` Project Persistence (`bffb590`)

Implemented in `crates/stagesync-desktop/src/project.rs` and wired into `app.rs`, `state.rs`, and `main.rs`:

- **Portable, Non-Blotting Project Format (`StageSyncProject` v1)**: Serializes reference/target media references, cached metadata, `AlignmentReport`, manual anchors, fine nudges & undo history, full mixer state (faders, trims, pans, pan laws, mute/solo, monitoring mode, master gain), timeline viewport & loop span, active selection, and export settings as JSON without ever embedding audio samples.
- **Relative + Fallback Path Resolution**: Stores both relative paths (from the `.stagesync` file's parent directory) and original absolute paths so moved project folders resolve media cleanly.
- **File Integrity & Staleness Protection**: Records `byte_len` and SHA-256 digests per media entry. Opening a `.stagesync` project verifies each file via `verify_resolved_file`:
  - Missing files are flagged (`FileEntry::missing`), clear stale cached results, and offer an in-panel **Relocate…** workflow (`relocate_reference` / `relocate_target`) without altering original files.
  - Modified files (size or SHA-256 mismatch) emit an explicit warning (`FileEntry::modified_warning`) and discard cached alignment results/anchors/nudges rather than trusting stale data.
- **Atomic Saves & Recovery**: Writes `.stagesync` files and crash recovery snapshots (`recovery.stagesync`) via staging `.tmp` files and atomic rename/replace.
- **Workflow Integration**: `File` menu (`New`, `Open…`, `Open Recent`, `Save`, `Save As…`), dirty state indicator (`*`) in title/toolbar, unsaved-changes confirmation modal (`Save` / `Don't Save` / `Cancel`), crash recovery banner on startup when a non-empty recovery snapshot exists, `.stagesync` CLI argument support in `main.rs`, and `.stagesync` drag-and-drop loading.

---

## 2. M19 / M20 / M21 Audit & Bug Fixes (`beb649f`, `206153e`)

- **M19 Short-Clip Reliability (`crates/stagesync-core/src/matching/fine.rs`)**: Added adaptive shorter window/hop sizing (`short_win`, `short_hop`) for 8–30 s recordings so short valid clips align reliably while sub-8 s fragments continue to report insufficient material.
- **M20 Playback & Seeking (`crates/stagesync-desktop/src/player.rs`)**:
  - Fixed `is_playing()` to account for `player.empty()` so finished playback resets cleanly to Stopped/Paused.
  - Added `poll()` in frame loop to finalize finished playback states promptly.
  - Fixed `ResampledSource::open` when seeking at or past EOF (`total_frames > 0 && position >= total_frames as f64` yields finished/silence instead of repeating the tail block).
  - Added lock-free generation-stamped `LiveMixShared` / `LiveTrackUpdate` so mixer level, trim, pan, mute, solo, and master gain changes update live A/B/Mix playback without restarting audio streams.
- **M21 Mixer & Export Consistency (`mixer.rs`, `settings.rs`, `jobs.rs`, `state.rs`, `app.rs`)**:
  - Fixed `predicted_output` matrix norm to use the sum of absolute row gains (`|g00| + |g01|`, `|g10| + |g11|`).
  - Fixed per-track level meter to apply `track.effective_gain(solo_active)`.
  - Fixed `mix_members()` double-multiplication of `trim` gain.
  - Persisted and wired `LoudnessMatch` and `ContainerChoice` through `ExportSettings` -> `AppSettings` -> `MixOptions`.
  - Fixed `remove_target`, `make_reference`, and `clear_reference` to keep `nudges`, `nudge_history`, and `mixer.tracks` indices synchronized, using `entry.meta` channel counts when mixer rows are initialized before analysis.
  - Fixed `metrics_line` in `app.rs` so offset-only aligned targets (`drift_ppm = Some(0.0)`, `drift_se_ppm = None`) display their measured offset, drift-not-applied status, and coverage instead of `"no alignment yet"`.

---

## 3. Desktop UI Redesign, Light/Dark/System Themes & Workflow (`206153e` + follow-up polish)

- **Theme System (`crates/stagesync-desktop/src/theme.rs`)**: Added `ThemeMode::{Dark, Light, System}` with cohesive `Palette` color tokens across widgets, waveform lanes, segment legends, charts (`chart.rs`), timeline legend (`timeline.rs`), and mixer meters (`mixer.rs`), persisted in settings and switchable live from the header or `Options` menu. Fixed `Visuals::widgets.active.fg_stroke` so `strong_text_color()` maintains crisp dark-ink contrast in Light mode, and eliminated `override_text_color` clobbering `weak_text_color` and semantic warning/error colors.
- **Compact Studio Workspace (`app.rs`, `waveview.rs`)**:
  - Two-row studio top header (project/menu/analysis/theme/panel toggles + transport, A/B/Mix mode, loop controls, master volume, `H:MM:SS.mmm` timecode, `Start / - / + / Fit` zoom controls).
  - Scrollable `Sources` left sidebar and collapsible `Export` right sidebar, with full-width track names and non-crowded action rows.
  - Front-and-center Stereo L/R waveform timeline with `OVERVIEW_HEIGHT` minimap, loop/selection region rendering, cursor-centered wheel zoom, and horizontal pan.
  - Tabbed lower dock (`Mixer`, `Inspector & Charts` with 2-column metrics + inline diagnostic plots on wide panes, `Issues & Manual Anchor`) and collapsible status/log bar.
  - Vector icons drawn with `Painter` (`Icon`, `small_icon_button`) so buttons never show missing Unicode font boxes.
  - Keyboard shortcuts (`Space`, `1/2/3`, `Tab`, `L`, `F`, `Home`, `+`/`-`, `Ctrl+N/O/S/Shift+S/E/Z`, `Alt+Left/Right`).
  - Accessible labelling on manual anchor `DragValue` spin buttons (`labelled_by`).

---

## 4. Packaging, Third-Party Notices & Verification Summary

- **Notices (`packaging/notices/`)**: Updated `licenses.json` with exact metadata for all 19 additional locked packages (including `egui_kittest`, `kittest`, `rodio`, `cpal`, `objc2-*`, `alsa*`, `toml*`, `num-*`) and regenerated `THIRD_PARTY_NOTICES.md` and `SOURCE-AVAILABILITY.md` (`513` packages).
- **Windows Packaging (`packaging/windows/`)**:
  - `check_nsi.py packaging/windows/installer.nsi` passed with no problems.
  - `packaging/windows/build.ps1` built release `stagesync-desktop.exe` (`rustc 1.99.1`), bundled `notices/`, generated `dist/windows/stagesync-0.1.0-windows-x64-portable.zip` and `dist/windows/SHA256SUMS.txt`.
  - **Untested on this host**: `makensis` (NSIS installer compilation), macOS (`x86_64` / `aarch64` `.dmg`), and Linux (`.AppImage` / `.deb`).
- **Real GUI Screenshots (`wgpu` via `egui_kittest`)**:
  - [`docs/screenshots/stagesync_dark_mixer.png`](docs/screenshots/stagesync_dark_mixer.png)
  - [`docs/screenshots/stagesync_light_mixer.png`](docs/screenshots/stagesync_light_mixer.png)
  - [`docs/screenshots/stagesync_dark_inspector.png`](docs/screenshots/stagesync_dark_inspector.png)
- **Test Coverage Added**:
  - `crates/stagesync-core/tests/short_source_tests.rs`: short 12 s / 20 s clip alignment verification.
  - `crates/stagesync-desktop/src/player.rs`: EOF seek silence & live mix parameter updates without stream reopen.
  - `crates/stagesync-desktop/src/app.rs`: target removal & reference promotion index alignment across nudges/mixer.
  - `crates/stagesync-desktop/tests/project_workflow.rs`: `.stagesync` round-trip, modification detection, file relocation, export, and crash recovery snapshot tests.
  - `crates/stagesync-desktop/tests/ui_themes_and_capture.rs`: populated UI Dark/Light switching, Inspector/Issues tab switching, responsive `960x640` layout check, accessibility check, and optional `wgpu` screenshot capture (`STAGESYNC_SCREENSHOT_DIR`).
