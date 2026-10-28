# BEHAVIOR_TESTS.md — black-box behavioral validation protocol (clean-room)

> **CLEAN-ROOM DOCUMENT.** Contains only: specifications of independently generated test
> signals, procedures observable through the product's normal UI, and measurement
> definitions implemented with independent open-source tooling. No reference-product
> internals are assumed or described.

## 0. Status of this round (updated 2026-10-27)

| Item | Status |
|---|---|
| Synthetic signal library | ✅ generated (`../testdata/`, see §2; generator spec §2.1) |
| Measurement harness | ✅ implemented — `../results/AlignMeasure.cs` (coarse → fine protocol, §3.1) |
| Live tests vs Revoice Pro | ⚠️ **PARTIALLY EXECUTED.** Launch blocker resolved: the missing vendor DLLs (`R2RTRUST.dll` + siblings) are now present in `C:\Windows\System32` (installed by the user via vendor distribution; no circumvention performed by the agent). Product version verified: **5.1.32** (file version resource; SHA-256 `2D889398…42120D`). App starts, loads sessions, renders, and exports. |
| T1 fixed-offset end-to-end | ✅ **EXECUTED (2 runs: T1, T1b)** — full pipeline completed; artifacts + measurements in `../results/2026-10-27/` (§4.1). Absolute accuracy figure withheld pending fixed-span export (§0.1-1). |
| T1b fixed-span re-export | ⚠️ export attempted; "Export Selected Playback Range" button inert under automation — needs one manual click, then re-measure |
| T2–T8 | ⛔ not yet executed — exact procedures in §4.2 (automatable, see runbook §6) |
| Tests against MiniMax | ⏳ pending implementation |

### 0.1 Important observed caveats (verified, not assumed)

1. **Export origin confound**: plain **Save** of the export dialog produces one file per
   enabled track whose start/end follow each track's own content (verified 2026-10-27:
   first-energy onsets of the T1b trio were 560 ms / 0 ms / 5110 ms and durations
   16.067 s / 15.907 s / 7.014 s). Inter-file lag measurements therefore mix
   *alignment error* with *export-crop offset* and must not be quoted as accuracy
   figures. **Fixed-span procedure**: the dialog's "Export Selected Playback Range"
   button is the intended path (range = blue ruler selection); under automation
   (`BM_CLICK`, multiple attempts) it produced no file while leaving the dialog open —
   status: **needs manual UI verification** (one click by hand) — until confirmed, treat
   cross-file absolute lags as uncalibrated.
2. **Default export sample rate ≠ session rate**: session audio at 48 kHz exported at
   **44.1 kHz** by default (the "Save as type" combo default) — an observable resampling
   stage. Set the type combo to 48 kHz WAV for bit-clean comparisons.
3. Short spans (<3 s) make envelope-based **coarse** offset ambiguous on quasi-periodic
   syllable patterns; the fine stage (§3.1) is authoritative and must be given a coarse
   window wide enough (±300 ms minimum, or ±2 s for unaligned comparisons).
4. **"New Process" validation is silent**: incomplete fields ⇒ create action produces no
   effect and no message (observed repeatedly); complete fields ⇒ process + auto-render
   (observed once all four requirements were satisfied: type, range source, preset,
   tracks list).

## 1. Principles

- Signals are synthesized procedurally (documented seeds) — no copyrighted audio.
- All measurements use independent tools (C# harness source shipped; numpy/scipy/
  pyloudnorm may substitute).
- Every test records: input hashes, preset/parameter values, export format, product
  version/build, and measurement outputs, into `results/<date>/`.

## 2. Test signal library (already generated)

All files: WAV PCM16, 48 kHz (long pair stereo), in `../testdata/`.

| File | Spec | Purpose |
|---|---|---|
| `g_main.wav` | 8 s synthetic vocal-like phrase: 12 syllables, harmonic stack (9 harmonics, 1/n), vibrato 5.4 Hz ±18 cents, per-syllable notes from {220…440} Hz set, breath noise; seed 4242 | Guide reference |
| `d_offset.wav` | Guide delayed +250 ms, −6 dB, one-pole LP (a=0.35), white noise −30 dB | T1 fixed offset + device mismatch |
| `d_drift.wav` | Guide time-scaled to 100.5 % duration (≈ +40 ms progressive drift) | T2 progressive drift |
| `d_local.wav` | Syllable at 2.6–3.2 s shifted +80 ms; rest identical | T3 local timing error |
| `d_gap.wav` | Syllable at 2.0–2.5 s replaced by digital silence | T4 missing content |
| `d_retake.wav` | Re-synthesized performance (seed 9999, ±4 cent note offsets, vibrato 6.1 Hz ±24 cents), LP a=0.25, −9 dB, noise +0.03 | T5 different take/device, EQ+level+noise |
| `long_guide.wav` / `long_dub.wav` | 2 h stereo: 2.5 s chord pads (roots 110–196 Hz, per-segment random) + broadband percussive hit each segment; dub = +300 ms offset and +0.05 % drift, −3.1 dB | T6 scale/performance; T7 drift over hours |

### 2.1 Generator parameters (reproducibility)

Deterministic C# synthesis (source: `testdata/SynthGen.cs`); RNG seeds as tabled;
syllable onsets `t0=0.4 s`, durations 0.35–0.6 s, gaps 0.12–0.27 s; envelope attack
15 ms, release 40 ms, decay to 0.7 level; normalized to 0.7 FS.

## 3. Measurement definitions (independent tools)

### 3.1 Two-stage timing measurement (FIXED PROTOCOL — supersedes the old ±100 ms window)

- **Stage A — coarse**: 10 ms RMS-envelope of both signals; integer-lag normalized
  cross-correlation; search window **±5 s for unaligned pairs, ±300 ms for aligned
  pairs**; output initial offset t0 in seconds.
- **Stage B — fine**: GCC-PHAT on 4096-sample Hann frames, lag search
  **t0 ± 120 ms**; per-frame local residual τi; parabolic peak interpolation.
  Report residual distribution (mean/median/sd/p95/max), and a robust linear fit of
  τi vs time (drift, µs/s = ppm). *The old protocol used only the ±120 ms window and
  therefore could not detect the +250 ms T1 offset; Stage A exists to prevent that.*
- **Pitch**: independent autocorrelation F0 (70–800 Hz, 10 ms hop, voicing gate by
  correlation > 0.45 and energy > 1e-6) on Guide vs aligned-out; mean |cents| on
  jointly-voiced frames. (librosa.pyin acceptable alternative.)
- **Level**: ITU-R BS.1770 integrated loudness (pyloudnorm) + envelope correlation
  (attack 0.01 s / decay 0.03 s followers) between Guide and output.
- **Artifacts**: log-spectral distance (LSD, 2048-pt FFT, 50 % overlap, log-power,
  after Stage-A time alignment) on quasi-static segments; spectral-flux onset deviation;
  pre-echo energy before onsets vs Guide.
- **Stereo**: L/R correlation coefficient and its variation over time (image stability).
- **Resource**: wall-clock, peak RSS, output SHA-256 on rerun (determinism).

Harness: `../results/AlignMeasure.cs` implements §3.1 (WAV reader, envelope coarse,
GCC-PHAT fine, F0, LSD, stereo correlation, SHA-256) with no external dependencies.

## 4. Test procedures

### 4.0 Common workflow (verified end-to-end 2026-10-27)

All steps through the product's normal UI (automation toolkit: `../HANDOFF_CHECKLIST.md` §3):

1. Launch `RevoicePro.exe "<session>.rvp"` (or File▸Open) with a session containing one
   Guide and one Dub audio track (T1 used `testdata/example_session/` — a copy of the
   shipped example with the fixture WAVs renamed to `LEAD VOX 1.wav`/`LEAD VOX 2.wav`;
   the session resolves audio by file name next to the session file).
2. Dismiss the "Quick Start" dialog (title-bar X).
3. Set roles: select the guide track's region area, then **Tracks ▸ Match: Use Selected
   Track as Guide**; select the dub region, then **Tracks ▸ Match: Use Selected Tracks
   as Dubs**. (Each role command re-opens/updates the "New Process" dialog; role
   commands also change which track the dialog's **Add** button adds.)
4. Define the time range: **drag in the thin strip under the ruler starting on the
   right-hand hint area ("CLICK AND DRAG TO INSERT A PROCESS")** leftwards across the
   content — a blue playback-range bar appears in the ruler. (Drag on the *left* part of
   that strip zooms instead — do not start there.)
5. In the **New Process** dialog verify: Selected Process = "Match Timing / Pitch /
   Level" (APT), Initial Input Start & End = **Selected Playback Range** with times,
   Presets = desired factory preset (blue button ▸ Factory Presets ▸ e.g.
   "21 Tight- Timing"), and **Select Tracks** contains the needed tracks (click **Add**
   after each role command; the dialog rebuilds its controls when roles change — always
   re-read control handles if driving programmatically).
6. Click **Create New Process**. Observable result: process regions appear
   ("Match:…-Guide" blue, "Match:…-Dub" orange) and a new **"<track> [OUT]"** track
   with the rendered aligned waveform.
7. Export: **File ▸ Export Audio…** → in the dialog click **Enable All**, set the file
   name, then click **Save** → produces one WAV per track named
   `<name> <track name>.wav`. For fixed-span comparisons the
   **Export Selected Playback Range** button must be used with the blue range set to
   identical times — this button did not respond to automation (§0.1-1): confirm it
   with one manual click before relying on it.
8. Measure with the harness (§3).

### 4.1 T1 / T1b — fixed offset (+250 ms, EQ/level/noise) ✅ executed (2 runs)

- Setup: `example_session` with `g_main.wav`→`LEAD VOX 1.wav`,
  `d_offset.wav`→`LEAD VOX 2.wav`; process type APT "Match Timing / Pitch / Level";
  range source = Selected Playback Range (~2 s blue range first run; full-content span
  second run); preset **21 Tight- Timing**; process created and auto-rendered
  (dialog→visible output < 15 s; an output track `<track> [OUT]` is created).
- Exports: `results/2026-10-27/t1_aligned_out *.wav` (run 1) and
  `t1b_fixed *.wav` (run 2, all enabled tracks). Measurements:
  `measurements_t1.md`, `measurements_t1b.md`.

| Metric | Run 1 (T1) | Run 2 (T1b) |
|---|---|---|
| guide / dub export | 48 kHz mono, 1.835 s / 1.835 s | 48 kHz mono, 16.067 s / 15.907 s |
| aligned export | **44.1 kHz** mono, **1.900 s** (≠ inputs) | **44.1 kHz** mono, **7.014 s** (≠ inputs) |
| duration change (aligned vs dub) | +3.5 % on the processed span | output content length differs ⇒ time modification active; also reveals non-fixed-span exports (§0.1-1) |
| coarse Stage-A (dub vs guide) | +0.090 s (ambiguous, §0.1-3) | +0.090 s (same) |
| fine Stage-B residuals (dub vs guide) | mean −5.7 ms, sd 11.6, p95 8.2, max 19.6 | mean −8.9 ms, sd 15.0, p95 9.6, max 26.5 |
| fine Stage-B residuals (aligned vs guide) | mean −88.7 ms, sd 9.6 | mean −104.5 ms, sd 10.4 |
| inter-file origin check (first energy) | not measured | **560 ms / 0 ms / 5110 ms ⇒ origins differ ⇒ absolute lags uncalibrated (§0.1-1)** |
| pitch abs error (aligned vs guide) | **2.73 ¢** (140 frames) | **2.67 ¢** (140 frames) |
| LSD (aligned vs guide / dub baseline) | 1.14 / 1.17 dB | 1.14 / 1.14 dB |

**Verdicts (honest):**
- ✅ Pipeline fully works: roles → range → dialog → create → auto-render → export.
- ✅ Time modification demonstrated (output duration ≠ input duration under a
  timing-only preset).
- ✅ Pitch untouched by timing-only mode: ≈2.7¢ mean error (both runs).
- ✅ No gross spectral change: LSD(aligned,guide) ≈ LSD(dub,guide).
- ✅ Local content mapping is *stable* (aligned residual sd ≈ 10 ms across 151 frames).
- ⛔ **Absolute alignment accuracy (≤2 ms claim) NOT YET MEASURABLE** with plain-Save
  exports because origins differ per track; the range-export path must be confirmed
  (§0.1-1) and T1b re-run before any accuracy number is quoted. The −88/−104 ms figures
  are recorded as *file-coordinate content lags with confounded origins*, explicitly
  **not** alignment errors.

### 4.2 Remaining scenarios — exact procedures (not yet executed)

Identical to §4.0 with these substitutions and measurements:

| ID | Guide/Dub pair | Preset/params | Measurements | Pass bands (initial) |
|---|---|---|---|---|
| T1b | same as T1 | same, **fixed-span re-export** (blue range pinned to identical times; type combo 48 kHz) | Stage A must report ≈+0.250 s unaligned and ≈0 s/≤2 ms aligned; §3 metrics | offset error ≤ 2 ms; no artifacts |
| T2 | `g_main`/`d_drift` | Tight; then High Flexibility | Stage-A slope before ≈ +0.5 %; residual drift after; stretch bounds | residual ≤ 0.005 % |
| T3 | `g_main`/`d_local` | Tight / Normal / Loose | local τi excursion at 2.6–3.2 s; collateral elsewhere | corrected ≤ 5 ms; collateral ≤ 2 ms |
| T4 | `g_main`/`d_gap` | default | silence mapping, diagnostics (product shows "no signal…" style warnings only when applicable), no clicks (LSD on silence ≈ 0, no NaN) | artifact-free |
| T5 | `g_main`/`d_retake` | Normal+Pitch; Timing-only | timing under EQ/level/noise; pitch error monotonic in transfer strength | timing ≤ 10 ms |
| T6 | **`long_guide`/`long_dub` (2 h stereo)** | Tight | Stage-A across 2 h (chunked), wall-clock, peak RSS, output duration sanity, artifacts | drift ≤ 10 ms over 2 h; RSS ≤ 4 GB; RTF ≤ 2 |
| T7 | T1 re-run | identical | SHA-256 equality of exports | bit-identical |
| T8 | stereo long pair | default | L/R correlation stability of output | Δ ≤ 0.02 |

Presets comparison protocol: repeat T1b with at least "11 Slightly Loose- Timing
[Default]", "21 Tight- Timing", "31 Dialogue- Align Timing" and tabulate the §3 metrics
side by side (observable differences in τi distribution and stretch activity).

## 5. Result log template

```
date | product build | test ID | params | stageA offset pre/post (ms) | τi mean/max (ms) |
drift (ppm) | pitch err (c) | LSD (dB) | onset dev (ms) | loudness Δ (LU) |
wall-clock | RSS | export SR | notes
```
Filled rows for T1: see `results/2026-10-27/measurements_t1.md` + §4.1 table.

## 6. Automation runbook (verified mechanics)

The product ignores injected keyboard input entirely; mouse input and window messages
work. Verified primitives (details in `HANDOFF_CHECKLIST.md` §3):

- Open top menus: post `WM_SYSCOMMAND/SC_KEYMENU` with the mnemonic char to the frame.
- Read menus: full-screen capture; menu row pitch ≈ 44 px from top ≈ y86.
- Click menu items and canvas: `SetCursorPos(png/2)` then button down/up (coordinate
  space of captures is 2× the cursor space on this machine).
- Dialogs are modeless; standard controls respond to `BM_CLICK`/`WM_SETTEXT`;
  custom-painted controls need coordinate clicks (verify with `WindowFromPoint`).
- Detect windows/dialogs by enumerating the process's `#32770` windows.

## 7. Safety/compliance

- Uses only legitimately purchased/installed software, exercised through normal UI.
- No reverse-engineered knowledge is required to run any step here.
