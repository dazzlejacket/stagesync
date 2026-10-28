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
it **down** by `-1200 * log2(scale)` cents — a 35 ppm drift is about 0.17 cents, which
is inaudible, but the number is reported rather than rounded away.

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
probes, every coarse candidate it considered, and every warning it raised:

```
source_basic.wav: offset +37.502 s, drift +26.541 ppm, 79.2% covered, 8 points (0 skipped), 0 warnings
    note: direct drift +31.404 ppm vs calibrated +26.541 ppm (difference -4.863 ppm)
    fine probes: 8 measured, 0 skipped (quiet 0, no region 0, no peak 0)
    coarse candidate: lag +37.500 s, 1 vote(s), mean score 0.374
    whole-file correlation: lag +37.500 s, score 0.628
```

The `note:` line is deliberate: the *direct* slope, the *calibrated* slope and their
difference are three separate quantities, and a disagreement between them is reported
rather than quietly averaged away.

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
  --corrected out\corrected\Voice_005_corrected.wav `
  --reference "audio\combined.wav" `
  --offset   317.391 `
  --duration 4813.5
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
A log-mel spectrogram is computed alongside as an independent second opinion.

**Coarse lock.** Long (60 s) windows spread evenly over the source are correlated
against the whole reference. A candidate offset is only accepted when the whole-file
correlation agrees with it, or when at least two windows agree — a single window can be
fooled by a passage that happens to sound like another one. The whole-file correlation
is the anchor; the windows supply the corroboration and the drift.

**Fine probes.** Every 15 s, a 30 s window around the model's prediction is correlated
against the reference with a ±2 s search, and the peak is refined by parabolic
interpolation. Overlap-normalised NCC drives the confidence score; a separate
fixed-normalisation curve ranks the candidate peaks, so a short-overlap edge artefact
cannot outrank a real match. Lags with less than half the shorter signal overlapping
are marked unreliable and excluded.

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

---

## Accuracy

Measured on the deterministic synthetic fixtures, where the ground truth is exact by
construction, and cross-checked on the real recordings by **two independent
implementations** — the Rust hold-out verifier and `tools/verify_exports.py`, which
shares no code with the core.

### Synthetic fixtures

`source_basic.wav`: +37.5 s offset, +35 ppm drift, −20 dB level, 44.1 kHz source
against a 48 kHz reference. The fixture's *effective* scale after integer resampling is
+34.898 ppm, which is what the assertions use.

| Quantity | Value |
|---|---|
| Fitted offset | +37.502 s (truth +37.5013 s) |
| Fitted drift | +26.541 ppm (truth +34.898 ppm) |
| **Model error across the whole file** | **≤ 0.9 ms** |
| Individual probe measurements | within 1.1 ms at 6 of 8 positions; worst +6.1 ms |
| Hold-out check (waveform GCC-PHAT, disjoint positions) | 4/4 trusted, p50 0.584 ms, p95 0.834 ms, max **0.834 ms** |
| Independent Python verifier | 7/8 trusted, p50 0.000 ms, p95 0.000 ms, max **0.885 ms** |
| Pitch change implied by the model | −0.0459 cents |

### The real recordings

`combined.wav` (9685.46 s, 48 kHz) against two 44.1 kHz AAC recorders:

| Source | Offset | Drift | Coverage | Rust hold-out | Independent Python verifier |
|---|---|---|---|---|---|
| Voice 005 | **+317.389 s** | **+16.034 ppm** | 50.7 % | p50 2.079 ms, p95 2.438 ms, max 6.300 ms | p50 1.753 ms, p95 5.599 ms, max 6.500 ms |
| Voice 006 | **+5753.314 s** | **+16.220 ppm** | 42.4 % | p50 2.068 ms, p95 2.413 ms, max 6.867 ms | p50 0.068 ms, p95 1.285 ms, max 1.488 ms |

The two implementations agree to within about half a millisecond at p50, and both put
the real alignment inside the 5 ms target. The residual is centred near −2 ms rather
than zero: after alignment the voice audio still lags the soundboard by about two
milliseconds. That is a fixed delay in the capture chain, not a clock error, and no
amount of time-warping removes it.

**10 m 22 s of the reference was never recorded by anybody.** The merged timeline
reports it as an explicit gap (939.70 s uncovered in total: 317.39 s before the first
recorder starts, plus the 622.31 s hole between the two). It is not filled in with
invented material.

The full detail, including the drift figures from the handoff documents that this work
does **not** reproduce, is in [`PROJECT_STATUS.md`](PROJECT_STATUS.md).

---

## Repository layout

```
crates/stagesync-core/      all the DSP and logic — no UI, no policy
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
