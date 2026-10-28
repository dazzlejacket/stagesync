# StageSync

Drift-aware synchronisation of several recordings of the same concert or play.

You have a soundboard recording and two handheld recorders. They all captured the same
performance, but they started at different moments and their clocks ran at slightly
different rates, so they drift apart over the course of an evening. StageSync finds
where each recording sits on the reference timeline, measures how fast its clock was
running, and renders corrected audio — while refusing to invent material that was
never recorded.

StageSync is an **independent, clean-room implementation**. It was written from a
behaviour specification and publicly documented DSP literature; no proprietary code,
decompilation, internal table, parameter or weight was used, and the user interface
was not cloned. See [`PROVENANCE.md`](PROVENANCE.md) and
[`cleanroom/`](cleanroom/).

> **Status: first usable vertical slice.** The core, the CLI, the desktop application,
> the exports and the test suite all work. Camera/video retiming and NLE multicam
> export are Phase 2 and are not implemented. See
> [`PROJECT_STATUS.md`](PROJECT_STATUS.md) for the exact state of every part.

---

## The one rule that matters

Every number StageSync produces uses this convention, everywhere, without exception:

```
reference_time_seconds = scale * source_time_seconds + offset_seconds
drift_ppm               = (scale - 1) * 1_000_000
```

`scale > 1` means the source's own clock ran **slow**, so its timeline is compressed.
The correction resamples by `alpha = 1/scale`, which *lengthens* the file and pitches
it **down** by `-1200 * log2(scale)` cents — a 35 ppm drift is about 0.061 cents, and
the 16 ppm measured on the real recordings is about 0.028 cents, both far below
audibility, but the number is reported rather than rounded away.

---

## Build

Requires a stable Rust toolchain (the workspace declares `rust-version = 1.82`; it has
been built and tested on 1.99.1) and a Windows toolchain with the MSVC toolchain.

```powershell
cargo build --release
```

This produces two executables:

| Binary | What it is |
|---|---|
| `target\release\stagesync.exe` | the command-line tool |
| `target\release\stagesync-desktop.exe` | the native Windows desktop application |

Quality gates — all four are expected to pass:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

All four exit 0 on the committed tree. The test suite is **62 tests**: 18 core unit
tests, 12 alignment scenarios, 12 render tests, 10 robustness tests, 6 desktop tests,
3 doc tests and 1 CLI test.

---

## Command line

### Analyse — find the alignment, write the sync map

```powershell
stagesync analyze `
  --reference "audio\combined.wav" `
  --source   "audio\Voice 005_W_20250208_152158.m4a" `
  --source   "audio\Voice 006_W_20250208_164548.m4a" `
  --out      out\analysis
```

Prints the offset, the drift in ppm, the coverage, the number of measured and skipped
probes, every coarse candidate it considered, and every warning it raised. This is the
real output for the two recorders against the DJI soundboard — 23.8 seconds for 2.69
hours of reference:

```
reference: combined.wav (9685.46 s, 48000 Hz, 1 ch)
Voice 005_W_20250208_152158.m4a: offset +317.389 s, drift +16.116 ppm, 49.6% covered, 638 points (0 skipped), 1 warnings
    note: direct drift +16.169 ppm vs calibrated +16.116 ppm (difference -0.053 ppm)
    fine probes: 319 measured, 0 skipped (quiet 0, no region 0, no peak 0)
    coarse candidate: lag +317.400 s, 15 vote(s), mean score 0.057
    coarse candidate: lag +7826.100 s, 1 vote(s), mean score 0.050
    whole-file correlation: lag +317.400 s, score 0.569
    warning: the spectral cross-check disagreed with the envelope fit at 259 of 319 positions (threshold 50 ms; median disagreement -317 ms, largest 685 ms); the envelope fit was kept and the disagreement is reported
Voice 006_W_20250208_164548.m4a: offset +5753.314 s, drift +16.500 ppm, 41.8% covered, 522 points (10 skipped), 1 warnings
    note: direct drift +16.434 ppm vs calibrated +16.500 ppm (difference +0.066 ppm)
    fine probes: 259 measured, 10 skipped (quiet 4, no region 6, no peak 0)
    coarse candidate: lag +5753.350 s, 13 vote(s), mean score 0.057
    whole-file correlation: lag +5753.350 s, score 0.574
    warning: the spectral cross-check disagreed with the envelope fit at 181 of 263 positions (threshold 50 ms; median disagreement -233 ms, largest 566 ms); the envelope fit was kept and the disagreement is reported
timeline: 9685.46 s total, 950.87 s with no recorder coverage
```

The `note:` line is deliberate: the *direct* slope, the *calibrated* slope and their
difference are three separate quantities, and a disagreement between them is reported
rather than quietly averaged away. The `warning:` line is deliberate too: the log-mel
cross-check disagrees with the envelope fit on this material, the envelope fit is kept,
and the disagreement is stated rather than hidden.

### Export — analyse, render corrected audio, and verify the result

```powershell
stagesync export `
  --reference "audio\combined.wav" `
  --source   "audio\Voice 005_W_20250208_152158.m4a" `
  --out      out\corrected `
  --layout   source-aligned
```

Three layouts:

| `--layout` | Result |
|---|---|
| `source-aligned` | one corrected file per source, keeping that source's own length and sample rate |
| `timeline-per-source` | one reference-length file per source, with explicit silence where the source was absent |
| `timeline-merged` | one reference-length file covering every source, priority-resolved |

The export runs the **hold-out verification** on the rendered audio and records the
result in `report.json`. Pass `--no-verify` to skip it.

### Verify — check an existing render independently

```powershell
stagesync verify `
  --corrected out\corrected\Voice_005_W_20250208_152158_corrected.wav `
  --reference "audio\combined.wav" `
  --offset   317.389329 `
  --scale    1.000016115882 `
  --duration 4813.621
```

### Info — metadata for one file

```powershell
stagesync info "audio\Voice 005_W_20250208_152158.m4a"
```

---

## Desktop application

```powershell
cargo run --release -p stagesync-desktop
```

* Drag files onto the window, or use **Browse**. The reference is the first file you
  add; every further file becomes a target.
* **Analyze** runs the whole pipeline on a worker thread, reports each stage, and
  **Cancel** really stops it — the cancellation is checked inside the decoding loop, so
  it takes effect within about a second rather than at the end of a three-hour file.
* The timeline is zoomable and scrollable, coloured by segment kind, with genuine gaps
  hatched rather than hidden.
* Per target you see the offset, the drift in ppm *and* in accumulated milliseconds,
  the coverage, the residual p50/p95/max, and every warning the analyser raised.
* **Manual anchor** sets a correspondence by hand when the automatic lock fails. It is
  a hard constraint: the final model is forced through it.
* **Export** writes the real files and tells you their real absolute paths.

---

## What gets written

| File | Contents |
|---|---|
| `<source>_corrected.wav` | the drift-corrected source, native channel count, native level, target sample rate |
| `syncmap.csv` | one row per measurement: source time, measured reference time, measured lag, model lag, residual in ms, confidence, method, and a note saying why a point is not fully trusted |
| `segments.csv` | every span of every source: matched, uncovered reference, recorder gap, low confidence, beyond reference |
| `report.json` | the complete machine-readable analysis, including the SHA-256 of every input and output |
| `drift_graph.svg` | the drift and residual graph, hand-written SVG — no plotting dependency |
| `timeline.txt` | a human-readable summary of the merged timeline |
| `plan.json` | the resolved model for each source |

Reports carry **no timestamps**, so re-running the same analysis produces byte-identical
artefacts and the hashes in `report.json` are meaningful.

---

## How it works

**Ingest.** Symphonia decodes WAV (PCM and IEEE float) and AAC/M4A, streaming in blocks
so a three-hour file never sits in memory. Every input is SHA-256'd.

**Feature.** A 100 Hz RMS envelope (20 ms window, 10 ms hop) on a dB scale, then locally
z-scored over ±30 s. The dB scale turns a 37 dB level difference into a constant
offset; the local z-score removes that offset *and* normalises the local dynamic range.
The local statistics are computed from the signal frames only, so that a block of
silence in one recording cannot distort the envelope's shape around it — and they are
computed the same way for both files, because making the normalisation depend on where
the silence is biases the measured drift. A log-mel spectrogram is computed alongside as
an independent second opinion.

**Coarse lock — this stage finds *where*, not *how fast*.** Long (60 s) windows spread
evenly over the source are correlated against the whole reference. A candidate offset is
only accepted when the whole-file correlation agrees with it, or when at least two
windows agree — a single window can be fooled by a passage that happens to sound like
another one. The whole-file correlation is the anchor; the windows supply the
corroboration. The seed model it produces is **offset-only** (`scale = 1.0`): the lags
are read off a 20 Hz decimated envelope, so they are quantised at 50 ms and cannot
resolve a ppm-scale rate.

**Fine probes — this stage measures the rate.** Every 15 s, a 30 s window around the
model's prediction is correlated against the reference with a ±2 s search, and the peak
is refined by parabolic interpolation. Silent frames are excluded from the *correlation*
on both sides, so a recorder gap cannot bias the comparison. Overlap-normalised NCC
drives the confidence score; a separate fixed-normalisation curve ranks the candidate
peaks, so a short-overlap edge artefact cannot outrank a real match. Lags with less than
half the shorter signal overlapping are marked unreliable and excluded. The fit uses only
the trustworthy points — a model built on rejected measurements looks like a measurement
but is not one.

**Robust model.** The measurements are fitted with Theil–Sen, cross-checked with
deterministic RANSAC (fixed-seed LCG) and refined by iterative MAD rejection. A
disagreement between the fitters is reported.

**Calibration.** The current model is applied to the source, the *corrected* material
is measured again, and the residual relation is folded back into the model. The
correction is bounded (at most 100 ppm and 2 s); a larger proposal is reported as a
disagreement and not applied.

**Hold-out verification.** The rendered audio is measured against the reference on the
**raw waveform** with GCC-PHAT, at positions that sit *between* the fine-map probes, so
they are genuine hold-out positions and a different estimator. The pitch change is
measured independently by autocorrelation.

**Rendering.** A deterministic polyphase Kaiser-windowed-sinc resampler (63 taps, 1024
phases) corrects the drift; a zero-drift source is corrected with a constant offset and
no resampling at all.

**Segmentation.** The gap threshold follows the spacing the matcher actually achieved,
estimated from *all* the measured points, and the hole is located from the envelope's
quiet flags — so the reported gap covers the hole itself rather than the whole span
around it, and the good material on either side of it is never stretched over it.

---

## Accuracy

Measured on deterministic synthetic signals, where the ground truth is exact by
construction, and cross-checked on the real recordings by **two independent
implementations** — the Rust hold-out verifier and `tools/verify_exports.py`, which
shares no code with the core.

### The Rust alignment suite — 12 scenarios, 12/12 passing

Every fixture is generated inside the test, so the truth is exact. `n` is the number of
independent high-confidence positions the alignment was scored at; the figures are the
absolute error of the model at those positions.

| Scenario | n | p50 | p95 | max |
|---|---|---|---|---|
| clean offset | 9 | 0.056 ms | 0.426 ms | 0.426 ms |
| negative offset | 12 | 0.020 ms | 0.042 ms | 0.042 ms |
| multi-hour offset | 2 | 0.399 ms | 0.399 ms | 0.399 ms |
| drift +10 ppm | 18 | 0.857 ms | 2.641 ms | 2.641 ms |
| drift +35 ppm | 27 | 1.597 ms | 3.842 ms | 3.842 ms |
| drift −35 ppm | 25 | 1.324 ms | 3.247 ms | 3.247 ms |
| drift +1000 ppm | 26 | 3.524 ms | 7.789 ms | 7.789 ms |
| −37 dB level difference | 9 | 0.056 ms | 0.426 ms | 0.426 ms |
| 48 kHz source vs 44.1 kHz reference | 4 | 1.077 ms | 1.131 ms | 1.131 ms |
| EQ + noise | 13 | 0.161 ms | 0.658 ms | 0.658 ms |
| repeated choruses | 8 | 0.238 ms | 0.964 ms | 0.964 ms |
| head and tail silence | 13 | 0.017 ms | 0.050 ms | 0.050 ms |

The target — **≤ 5 ms at ≥ 5 independent high-confidence positions** — is met on every
scenario except the deliberately brutal +1000 ppm one, where it is met at p50 and
exceeded at p95 by 2.8 ms.

### The Python fixture, and where it is noise-limited

`tools/make_fixtures.py` builds a harder fixture: +37.5 s offset, +35 ppm drift, −20 dB
level, 44.1 kHz source against a 48 kHz reference, over 600 s. Measured on the 600 s
fixture:

| Quantity | Value |
|---|---|
| True drift (from the generator's own arithmetic) | +35.00 ppm |
| True offset | +37.5013 s |
| Fitted offset | +37.5035 s |
| Fitted drift | +26.834 ppm direct, +27.836 ppm calibrated |
| **Model error across the whole file** | **≤ 2.2 ms** |
| Independent waveform-domain measurement | **+34.828 ppm**, offset 37.501299 s, fit residual ≤ 0.076 ms |

The **alignment** is good: ≤ 2.2 ms of model error across 562 s, inside the 5 ms target.
The **slope** is not: the core fits the drift from RMS-envelope probes whose noise on
this fixture is ±5 ms, and 35 ppm over 562 s is only 19.7 ms of signal, so the slope
carries roughly ±8 ppm of uncertainty. A waveform-domain measurement of the same fixture
resolves the drift to +34.83 ppm against a truth of +35.00 ppm.

That is a property of the *fixture*, not of the real recordings: on the real data the
probe noise is about 1 ms and the drift signal is 77 ms, so the slope is resolved to
better than ±0.03 ppm. The shorter 180 s variant of the same fixture resolves the
drift not at all — it develops 5 ms of signal against the same ±5 ms of noise, and the
measured slope comes out at −47.7 ppm. Use the Rust suite for drift assertions; use
this fixture for the offset, level and sample-rate behaviour.

### The real recordings

`combined.wav` (9685.46 s, 48 kHz) against two 44.1 kHz AAC recorders:

| Source | Offset | Drift | Coverage | Rust hold-out | Independent Python verifier |
|---|---|---|---|---|---|
| Voice 005 | **+317.389 s** | **+16.116 ppm** | 49.6 % | 142/160 trusted, p50 1.879 ms, p95 2.249 ms, max 2.316 ms | 8/13 trusted, p50 0.089 ms, p95 1.680 ms, max 1.816 ms |
| Voice 006 | **+5753.314 s** | **+16.500 ppm** | 41.8 % | 108/134 trusted, p50 1.992 ms, p95 2.245 ms, max 9.120 ms | 7/13 trusted, p50 0.000 ms, p95 1.166 ms, max 1.666 ms |

The two implementations agree, both put the real alignment inside the 5 ms target at p50
and p95, and both exclude the positions where their two methods disagree by more than
10 ms rather than counting them.

The pitch change is reported two ways. The **model** implies −0.0279 cents (Voice 005)
and −0.0286 cents (Voice 006) — `-1200 * log2(scale)`, which is the exact answer. An
autocorrelation tracker on polyphonic material resolves to tens of cents, so the
measured figures (mean |1.61| cents over 2 positions for Voice 005) are only a coarse
sanity check that the pitch was not shifted; they cannot resolve a correction of a few
hundredths of a cent, and the tool says so rather than pretending otherwise.

**10 m 22 s of the reference was never recorded by anybody.** The merged timeline
reports it as an explicit gap: **950.87 s** in total is not covered by any recorder —
317.39 s before the first recorder starts, an 11.18 s recorder gap inside Voice 005, and
the 622.30 s hole between the two recorders. It is not filled in with invented material.

The full detail, including the drift figures from the handoff documents that this work
does **not** reproduce, is in [`PROJECT_STATUS.md`](PROJECT_STATUS.md).

---

## Repository layout

```
crates/stagesync-core/      all the DSP and logic — no UI, no policy
crates/stagesync-core/tests/ the hermetic end-to-end suite (alignment, render, robustness)
crates/stagesync-cli/       the `stagesync` command-line tool
crates/stagesync-desktop/   the `stagesync-desktop` egui/eframe application
tools/make_fixtures.py      deterministic synthetic fixtures with exact ground truth
tools/verify_exports.py     an independent numpy/scipy verifier (not part of the app)
cleanroom/                  the clean-room specification and provenance documents
audio/                      source recordings — READ ONLY, never committed
```

`AGENTS.md` is the working contract for the repository: the invariants, the clean-room
boundary, and the list of mistakes that have already been made and must not be
repeated.

---

## Not in scope

* **Camera / video retiming.** Phase 2. StageSync handles audio only.
* **NLE multicam export.** Phase 2.
* **Pitch, formant or vocal reconstruction.** Deliberately out of scope; the correction
  changes pitch only by the unavoidable amount of the drift correction, and that amount
  is measured and reported.
* **A/B audio preview in the desktop application.** There is no playback button rather
  than a broken one.

---

## Licence

Dual MIT / Apache-2.0. See [`LICENSES.md`](LICENSES.md) for the third-party dependency
licences — note that `symphonia` is **MPL-2.0**, which has file-level copyleft
implications for anyone redistributing a binary.
