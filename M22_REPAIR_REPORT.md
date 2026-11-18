# StageSync M22 Repair Report

**Branch:** `gemini_m22_bug_fixes` (fast-forwarded from audited `ae07b28` before applying repairs)  
**Target HEAD after repair pass:** see commit log below (`M22_REPAIR_REPORT.md` added at HEAD)  
**Overall Verdict:** **FIXED for Windows source/build/test workflows (private beta candidate on Windows); UNVERIFIED for NSIS setup executable, macOS builds, and Linux builds on this host.**

---

## 1. Required Quality Gates & Real-Concert Verification

| Gate / Check | Command | Exit Code | Result |
|---|---|---:|---|
| Formatting | `cargo fmt --check` | 0 | Pass cleanly |
| Lints (`-D warnings`) | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | Pass cleanly |
| Release build | `cargo build --release` | 0 | Built `stagesync.exe` & `stagesync-desktop.exe` |
| Cold parallel tests | `CARGO_TARGET_DIR=out/cold-test-target cargo test --workspace` | 0 | **282 passed, 0 failed, 0 ignored** (184 core + 98 desktop) |
| Real concert CLI export | `.\target\release\stagesync.exe export` (`combined.wav` + `Voice 005` + `Voice 006` -> `out\real-concert-check`) | 0 | Finished in 59.8 s; Voice 005 (`+16.061 ppm`, `142/160` hold-out trusted, `p50 +2.126 ms`, `max 6.948 ms`); Voice 006 (`+16.386 ppm`, `104/134` hold-out trusted, `p50 +2.180 ms`, `max 8.680 ms`) |
| Independent Python verifier (`Voice 005`) | `verify_exports.py --no-pitch` (`offset=317.3893364060632`, `scale=1.0000160608159037`) | 0 | `23/30` trusted (1 disputed, 6 untrusted), `|residual| p50 0.000 ms`, `p95 0.074 ms`, `max 1.901 ms` |
| Independent Python verifier (`Voice 006`) | `verify_exports.py --no-pitch` (`offset=5753.314004933`, `scale=1.000016385581`) | 0 | `16/24` trusted (3 disputed, 5 untrusted), `|residual| p50 0.061 ms`, `p95 0.390 ms`, `max 1.183 ms` |
| Original read-only media SHA-256 check | `Get-FileHash "C:\Users\grego\Documents\Codex Projects\SynkerClean\audio\*" -Algorithm SHA256` | 0 | Identical before & after: `combined.wav` (`048B29F3...`), `Voice 005` (`ED9A8E17...`), `Voice 006` (`DC57EAF5...`) |
| Windows packaging (`WindowsApps` first on `PATH`) | `.\packaging\windows\build.ps1 -Profile release -OutputDir out/win-pkg-test` + `check_nsi.py` | 0 | Portable ZIP `stagesync-0.1.0-windows-x64-portable.zip` built (`SHA-256: fa77c67e85ea4edc13fbe408c85e918c5f9054e67c37dc8aab698c3d113b7a3d`), 513 package notices generated; `check_nsi.py`: no problems found; NSIS skipped (`makensis` absent) |

### Exact Cold-Run Test Counts by Crate & Binary
- **`stagesync` CLI:** `0` unit tests (`src/main.rs`)
- **`stagesync-core` (184 total):** `42` unit + `12` `alignment_tests` + `6` `drift_tests` + `18` `dsp_tests` + `4` `gap_alignment_tests` + `6` `merged_timeline_tests` + `28` `mixer_tests` + `21` `repair_regressions` + `21` (`render_tests` / verification suite) + `12` + `9` `short_source_tests` + `5` = **184 passed, 0 failed, 0 ignored**
- **`stagesync-desktop` (98 total):** `62` lib unit + `6` `mixer_workflow` + `16` `platform_tests` + `7` `project_workflow` + `1` `ui_themes_and_capture` + `6` `workflow` = **98 passed, 0 failed, 0 ignored**

---

## 2. Codex Audit Findings Mapping

### P1 — Release Blockers

| Finding | Summary | Status | Fix & Files Changed | Test / Verification Evidence |
|---|---|---|---|---|
| **P1-01** | Moved project verifies moved audio files but retains old report paths and exports stale audio | **FIXED** | `crates/stagesync-desktop/src/project.rs` (`StageSyncProject::load_from` validates report SHA-256 & rebinds `report.reference.path` and `report.sources[i].source.path` to `resolved_path`), `crates/stagesync-desktop/src/app.rs` (`relocate_reference`, `relocate_target`), `crates/stagesync-desktop/src/jobs.rs` (`verify_meta_unchanged` re-checks byte length & SHA-256 right before render in `export_report`) | Commit `103fe78`; `project_workflow::p1_01_moved_project_rebinds_report_paths_and_exports_moved_audio`, `project_workflow::p1_01_export_refuses_tampered_audio_before_render` |
| **P1-02** | Project Save can overwrite an input recording or write non-`.stagesync` path | **FIXED** | `crates/stagesync-desktop/src/project.rs` (`ensure_safe_project_path` enforces `.stagesync` extension and delegates to `stagesync_core::render::ensure_output_does_not_overwrite`), called in `StageSyncProject::save_to` and `crates/stagesync-desktop/src/app.rs` (`save_project_to`) | Commit `103fe78`; `project_workflow::p1_02_project_save_refuses_to_overwrite_input_recording_or_wrong_extension` (tests direct WAV, `sub/../` relative alias, and `.json` rejection) |
| **P1-03** | Short clips (<60s) & unrelated signals receive grossly wrong applied offsets | **FIXED** | `crates/stagesync-core/src/matching/dsp.rs` (`pearson_correlate_masked`), `coarse.rs` (sub-window splitting for 8–60 s clips, `full_range_ncc`, refusing whole-file self-corroboration on single-window clips), `fine.rs` (`short_clip` `< win + hop` window scaling), `pipeline.rs` (`has_credible_evidence` refuses unanchored short/uncorroborated locks with `matched: false` & `model() == None`) | Commits `7e99fed`, `8e332bd`, `5c469bb`; `short_source_tests::short_twelve_second_excerpt_does_not_apply_grossly_wrong_offset`, `short_source_tests::unrelated_twenty_second_signal_is_refused`, plus existing `very_short_twelve_second_and_twenty_second_clips_are_aligned` & `app::tests::the_window_aligns_a_recording_and_exports_the_correction` |
| **P1-04** | Opening/switching projects does not isolate background analysis, export, verification, or probe results | **FIXED** | `crates/stagesync-desktop/src/jobs.rs` (`JobScope { session_gen, job_id }`, `ScopedJobEvent`, `spawn_*_scoped`), `crates/stagesync-desktop/src/app.rs` (`invalidate_session`, `cancel_active_jobs`, `report_matches_session`, generation & `job_id` gating in `drain_events`) | Commit `103fe78`; `project_workflow::p1_04_opening_project_b_isolates_background_analysis_from_project_a`, `app::tests::p1_04_stale_export_verification_and_probe_events_are_ignored` |
| **P1-05** | Corrected-target solo playback skips leading silence prefix (`ResampledSource::Iterator::next`) | **FIXED** | `crates/stagesync-desktop/src/player.rs` (`ResampledSource::fill` emits leading silence frames into `self.ready` so `Iterator::next()` and `MixSource`'s `next_frame()` share the exact delay path) | Commit `3569bf9`; `player::tests::p1_05_resampled_source_iterator_emits_leading_silence_before_target_start` |
| **P1-06** | Export silently ignores pending nudge; Apply wipes pending state before completion; unapplied anchor edits ignored on export | **FIXED** | `crates/stagesync-desktop/src/app.rs` (`effective_report_for_export` bakes pending nudges into model/`TimeMap`/segments/`merge_timeline`/`OverrideState`, clears stale `holdout`, and adds honest warning; `has_unapplied_anchor` blocks export when anchor edited without Apply; `PendingAnalysisCommit` stages Apply/Reset commits until `AnalysisFinished` succeeds) | Commit `3569bf9`; `app::tests::p1_06_pending_nudge_exports_in_audio_and_plan_and_unapplied_anchor_blocks_export` (verifies `plan.json`, rendered WAV samples, unapplied anchor refusal, and Cancel preservation) |
| **P1-07** | Stop / A-B / seek / loop can block native UI indefinitely on `rodio` backend; wall-clock playhead drift | **FIXED** | `crates/stagesync-desktop/src/player.rs` (`AudioBackend` trait + worker actor thread, `RodioBackend` creates fresh `RodioPlayer` per stream without calling blocking `clear()`, `StoppableSource` provides wait-free `cancelled` atomic + consumed `frames_played` clock), `crates/stagesync-desktop/src/app.rs` (`seek_playhead` restarts live audio on timeline/overview/Start/Home/jump, `follow_the_playhead` surfaces backend stalls via `take_error()`, `request_repaint_after(33ms)` while playing) | Commit `3569bf9`; `player::tests::p1_07_stalled_backend_never_blocks_stop_seek_or_voice_switch`, `player::tests::p1_07_position_follows_consumed_frames_not_wall_clock`, `player::tests::live_audio_device_stop_and_switch_do_not_freeze` |

### P2 & P3 — Quality, Mixer, UI & Packaging Findings

| Finding | Summary | Status | Fix & Files Changed | Test / Verification Evidence |
|---|---|---|---|---|
| **P2-01** | Changed audio keeps stale anchors/nudges; saving missing target wipes saved SHA-256 and byte length | **FIXED** | `crates/stagesync-desktop/src/project.rs` (`ProjectMediaRef::from_entry` preserves `expected_sha256` & `expected_byte_len` when file missing on disk), `crates/stagesync-desktop/src/app.rs` (`apply_loaded_project`, `apply_probe`, `relocate_reference`, `relocate_target`, `clear_reference` invalidate stale target or all anchors/nudges on content mismatch) | Commit `103fe78`; `project_workflow::p2_01_changed_or_missing_media_invalidate_manual_state_and_preserve_identity` |
| **P2-02** | Protected & loudness-matched exports disagree with preview (`render_mix` also ignored `settings.master_gain`) | **FIXED** | `crates/stagesync-core/src/mixer/mod.rs` (`render_mix` applies `settings.master_gain`; exported `loudness_offsets_db`, `loudness_gains`, `measure_autogain_trim`), `crates/stagesync-desktop/src/player.rs` (`MixSource` & `Player::play_mix` / `update_live_mix` apply `ClippingProtection` & `autogain_trim`), `crates/stagesync-desktop/src/app.rs` (`rms_cache` + `autogain_cache` + `mix_members` RMS loudness matching) | Commit `1601a50`; `mixer_tests::render_mix_applies_master_gain_with_none_soft_clip_and_auto_gain`, `app::tests::p2_02_app_mix_preview_matches_exported_mixdown_across_loudness_pan_and_protection` (verified sample diff `< 1e-4` across `Float32`, `SoftClip`, `AutoGain`, `MatchToLoudest`, `pan`, `master_gain`) |
| **P2-03** | Windows packaging aborts on Microsoft Store `WindowsApps` Python stub before trying real Python | **FIXED** | `packaging/windows/build.ps1` (`Test-PythonExecutable` + `Resolve-PythonCommand` checks `STAGESYNC_PYTHON`, tries all PATH candidates with bounded process timeout + closed stdin, skips `WindowsApps` reparse stubs unless needed as fallback, supports `py -3`, checks per-user `LocalAppData\Programs\Python`) | Commit `7b76c9e`; verified `build.ps1 -Profile release -OutputDir out/win-pkg-test` with `WindowsApps` placed first on `PATH` -> exit `0` |
| **P2-04** | Compact `960×640` transport overlaps playhead timecode and view span label | **FIXED** | `crates/stagesync-desktop/src/app.rs` (moves view/zoom controls to a clean second row below transport when `available_width() < 1180.0`) | Commit `2b9764b`; `ui_themes_and_capture::populated_ui_supports_dark_light_tabs_and_responsive_sizes` asserts non-intersecting `tc_rect`, `span_rect`, `fit_rect` within `<= 960.0` |
| **P3-01** | Multi-hour concert ruler collides labels (capped at 300s); unaligned tracks trigger false `+3.21 dBFS` clipping warnings | **FIXED** | `crates/stagesync-desktop/src/waveview.rs` (`tick_step` extends to 4 h + dynamic doubling enforcing `>= 84 px` gap), `crates/stagesync-desktop/src/mixer.rs` (`predicted_output` ignores `!info.aligned`, clarifies `"Predicted output (upper bound)"` & `"File peak"`) | Commit `2b9764b`; `waveview::tests::concert_ruler_labels_keep_minimum_pixel_separation`, `mixer::tests::unaligned_tracks_are_excluded_from_predicted_output` |
| **UI Cleanup** | Raw `audiocodecid(284)` codec string; repeated `Needs inspection / Export not verified`; subtle pending manual edits | **FIXED** | `crates/stagesync-core/src/audio/decode.rs` (`human_codec_name` displays e.g. `"PCM 32-bit float"`, `"AAC"`), `crates/stagesync-desktop/src/app.rs` & `waveview.rs` (shows codec on target card, stops repeating `"Export not verified"` on unaligned/unexported cards, adds prominent badges on card, lane caption, and Export panel for pending nudges & unapplied anchors) | Commits `7e99fed`, `2b9764b`; `decode::tests::codec_name_is_human_readable_not_raw_id` |

### Remaining `UNVERIFIED` Scope (Environmental / Out-of-Host Coverage)

| Area | Status | Reason / What Was Checked Instead |
|---|---|---|
| **Windows NSIS Installer (`.exe`)** | **UNVERIFIED** | `makensis` is not installed on this Windows host. Static check `check_nsi.py packaging/windows/installer.nsi` passed (`no problems found`); runtime `.exe` installer build, install/uninstall, and file associations require NSIS or the GitHub Actions `windows-2022` runner. |
| **Windows clean-machine launch & VC++ CRT** | **UNVERIFIED** | Verified portable ZIP structure, binary, `README.txt`, and 5 notice files on build host; launching on a clean Windows VM without VC++ Redistributable was not performed here. |
| **macOS Intel & Apple Silicon builds** | **UNVERIFIED** | Inspected `packaging/macos/build.sh` and `.github/workflows/{ci,release}.yml`; no macOS host available to test native `.app` / `.dmg` build, Gatekeeper behavior, or CoreAudio playback. |
| **Linux AppImage / `.deb` builds** | **UNVERIFIED** | Inspected `packaging/linux/build.sh` and `.github/workflows/{ci,release}.yml`; no Linux host available to test `dpkg-deb`, `appimagetool`, X11/Wayland, or ALSA playback. |
| **GitHub Actions CI / Release runs** | **UNVERIFIED** | `.github/workflows/ci.yml` and `.github/workflows/release.yml` reviewed (no pushes allowed in this pass). |
| **UI screen-reader & crash/power-loss recovery** | **UNVERIFIED** | `egui_kittest` accessibility labels & crash-recovery round-trip unit tests pass (`project_workflow::crash_recovery_snapshot_saves_and_clears`), but live Narrator/NVDA navigation and physical SIGKILL recovery were not manually executed. |
| **AutoGain 3-hour background responsiveness** | **NOTE / MINOR UX** | `resolve_preview_mix` computes `measure_autogain_trim` once (and caches by parameter fingerprint) when pressing Play in `ClippingProtection::AutoGain`. Short/medium clips start immediately; on multi-hour concerts, the very first `AutoGain` Play press runs the peak scan synchronously on the UI thread before caching (normal `None` and `SoftClip` monitoring have zero scan latency). |

---

## 3. Commits Created on `gemini_m22_bug_fixes`

- `7e99fed` `fix(core): prevent false short-clip matches (P1-03) and humanize codec labels`
- `8e332bd` `fix(core): refine short-clip fine probe window threshold (<45s)`
- `103fe78` `fix(desktop): project safety, moved path rebinding, and job isolation (P1-01/P1-02/P1-04/P2-01)`
- `3569bf9` `fix(desktop): non-blocking audio backend, leading silence, pending nudge export & anchor safety (P1-05/P1-06/P1-07)`
- `1601a50` `fix(mixer): apply master_gain in render_mix and align preview/export mixing (P2-02)`
- `2b9764b` `fix(ui): resolve 960x640 overlap, concert ruler spacing, false clipping alerts & noisy badges (P2-04/P3-01)`
- `7b76c9e` `fix(packaging): skip broken WindowsApps stubs and probe Python cleanly (P2-03)`
- `5c469bb` `chore: satisfy clippy -D warnings on pipeline and desktop tests`
- *(final commit adding `M22_REPAIR_REPORT.md`)*
