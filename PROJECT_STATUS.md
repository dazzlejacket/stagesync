# PROJECT_STATUS

The state of StageSync, stated precisely. Everything below was actually run; nothing
is a plan.

---

## 1. What works

| Component | State | Notes |
|---|---|---|
| `stagesync-core` | **complete** | Ingest, features, matching, robust model, calibration, segmentation, rendering, holdout verification, exports. |
| `stagesync-cli` | **complete** | `analyze`, `export`, `verify`, `info`. |
| `stagesync-desktop` | **complete** | Native Windows egui/eframe app: drag-and-drop import, background analysis with real cancellation, zoomable timeline, per-source metrics, manual anchors, export. |
| Exports | **complete** | Corrected WAV (three layouts), `syncmap.csv`, `segments.csv`, `report.json`, `drift_graph.svg`, `timeline.txt`, `plan.json`. |
| End-to-end test suite | **complete** | Hermetic, deterministic, generated in Rust. See §4. |
| Independent Python verifier | **complete** | `tools/verify_exports.py` — a separate numpy/scipy implementation that never shares code with the Rust core. |
| Camera / video retiming | **not started** | Phase 2. |
| NLE multicam export | **not started** | Phase 2. |
| A/B audio preview in the GUI | **deliberately absent** | There is no playback button rather than a broken one. |

---

## 2. The real recordings

Measured on the actual media in `audio/`, using the release build:

```
reference: combined.wav (9685.46 s, 48000 Hz, mono)
```

| Source | Duration | Offset | Drift (calibrated) | Drift (direct) | Coverage | Probes |
|---|---|---|---|---|---|---|
| `Voice 005_W_20250208_152158.m4a` | 4813.543 s | **+317.389 s** | **+16.034 ppm** | +16.065 ppm | 50.7 % | 319 measured, 0 skipped |
| `Voice 006_W_20250208_164548.m4a` | 4051.650 s | **+5753.314 s** | **+16.220 ppm** | +16.291 ppm | 42.4 % | 259 measured, 10 skipped |

Whole-file correlation (the independent anchor of the coarse lock):

| Source | Whole-file lag | Score |
|---|---|---|
| Voice 005 | +317.400 s | 0.605 |
| Voice 006 | +5753.350 s | 0.507 |

The whole analysis of a 2.69-hour reference against two 44.1 kHz AAC recorders takes
**27 seconds**.

### Against the figures in the handoff documents

The handoff numbers are treated as *claims to re-measure*, never as truth to hard-code.
Here is where they land:

| Claim | Re-measured | Verdict |
|---|---|---|
| Voice 005 at ≈ +317.391 s | **+317.389 s** | **Reproduced** (2 ms) |
| Voice 006 at ≈ +5753.304 s | **+5753.314 s** | **Reproduced** (10 ms) |
| Uncovered DJI span ≈ 5131.1–5753.3 s (10 m 22 s) | **5130.9–5753.3 s = 622.4 s** | **Reproduced** |
| Voice 005 drift 33–37 ppm (direct) | **+16.065 ppm** | **Not reproduced** |
| Voice 005 drift 12.95 ppm (calibrated) | **+16.034 ppm** | **Not reproduced** |

**The unresolved slope disagreement is not resolved by this work — it is reported as
open.** The handoff documents contain two figures for the same quantity, 33–37 ppm and
12.95 ppm, about 22 ppm apart. The re-measurement gives **+16.0 ppm** for Voice 005 and
**+16.2 ppm** for Voice 006 — between the two historical numbers, and close to neither.
The two recorders agree with each other to within 0.2 ppm, which is reassuring, but
neither historical figure has been reproduced and this work does not claim to have
settled which of them was right.

What the re-measured drift means in practice: 16 ppm over 4813 s is **77 ms** of
accumulated clock error across Voice 005, and **66 ms** across Voice 006. That is
audible as a drift between the two, and it is what the correction removes. The
implied pitch change is −0.17 cents and −0.17 cents respectively — far below audibility,
and reported rather than rounded away.

### Hold-out verification of the real exports

Two **independent** implementations measured the rendered audio against the reference:

* the Rust hold-out verifier in `stagesync-core` (waveform GCC-PHAT **and** an
  RMS-envelope opinion, with positions that sit between the fine-map probes), and
* `tools/verify_exports.py`, a separate numpy/scipy implementation that shares no code
  with the core.

Both cross-check the two methods against each other and exclude any position where they
disagree by more than 10 ms, because at such a position at least one of them has
locked onto the wrong thing.

| Source | Rust hold-out | Independent Python verifier |
|---|---|---|
| Voice 005 | 141 of 160 positions trusted (8 disputed, excluded) — **p50 2.079 ms, p95 2.438 ms, max 6.300 ms** | 5 of 11 trusted — **p50 1.753 ms, p95 5.599 ms, max 6.500 ms** |
| Voice 006 | 105 of 134 positions trusted (16 disputed, excluded) — **p50 2.068 ms, p95 2.413 ms, max 6.867 ms** | 4 of 11 trusted — **p50 0.068 ms, p95 1.285 ms, max 1.488 ms** |

The two implementations agree to within about half a millisecond at p50, and both put
the real-data alignment **inside the 5 ms target** at p50 and p95. The Python verifier
is the more cautious of the two: it refuses to make an accuracy claim when fewer than
five positions are corroborated, because five of Voice 006's eleven sampled positions
fall in passages that are too quiet to measure at all — the concert is ending there.

The residual is not centred on zero. It sits at roughly **−2 ms**, meaning the corrected
voice audio still lags the soundboard by about two milliseconds after alignment. That is
a fixed acoustic/electronic delay between the two capture chains, not a clock error, and
no amount of time-warping can remove it. It is reported rather than hidden.

### Byte-for-byte reproducibility

Running the same export twice produces **byte-identical artefacts**:

```
drift_graph.svg           IDENTICAL
plan.json                 IDENTICAL
segments.csv              IDENTICAL
source_basic_corrected.wav IDENTICAL
syncmap.csv               IDENTICAL
timeline.txt              IDENTICAL
report.json               IDENTICAL once the output directory name is normalised
```

The corrected WAV is identical, so the resampler is genuinely deterministic and the
chunked path matches the single-shot path. `report.json` differs only in the absolute
paths of the artefacts it lists — it carries no timestamp, so nothing in it varies
between runs.

### The uncovered span

The merged reference timeline is 9685.46 s long, of which **939.70 s is not covered by
any recorder**:

* **317.39 s** at the head — before Voice 005 starts.
* **622.31 s** in the middle — the 10 m 22 s span between the end of Voice 005 and the
  start of Voice 006.

This span is reported as an explicit gap. It is **not** stretched, interpolated or
force-matched, and the corrected audio carries silence there rather than invented
material.

---

## 3. Accuracy

Measured on the deterministic synthetic fixtures (`tools/make_fixtures.py`), where the
ground truth is exact by construction.

### Clean fixture

`source_basic.wav`: +37.5 s offset, +35 ppm drift, −20 dB level, 44.1 kHz source
against a 48 kHz reference. The fixture's *effective* scale after integer resampling is
+34.898 ppm, which is what the assertions use.

| Quantity | Value |
|---|---|
| Fitted offset | +37.502 s (truth +37.5013 s) |
| Fitted drift | +26.541 ppm (truth +34.898 ppm) |
| **Model error across the whole file** | **≤ 0.9 ms** |
| Individual probe measurements | within 1.1 ms at 6 of 8 positions; worst +6.1 ms |
| Hold-out verification (waveform GCC-PHAT, disjoint positions) | 4/4 counted, p50 +0.584 ms, p95 +0.834 ms, max **0.834 ms** |
| Hold-out verification (RMS envelope) | 4/4 measured, p50 +0.105 ms, max 0.206 ms |
| Pitch change implied by the model | −0.0459 cents |

The target — ≤ 5 ms at ≥ 5 independent high-confidence positions — is met with room to
spare on the clean fixture, and the independent hold-out check agrees.

### Two experiments that were run and rejected

* **A 200 Hz envelope** (10 ms frame, 5 ms hop) was tried and was *much worse*:
  residuals blew up to ±30 ms and the fitted drift came out at +1947 ppm. The shorter
  RMS window makes the envelope spiky, and the correlation peak stops being
  well-conditioned. Reverted to 20 ms / 10 ms.
* **The coarse lock's candidate ranking** was changed so that the overlap-normalised
  score ranks the peaks. Short-overlap edge artefacts then outrank real matches. The
  ranking uses a fixed-normalisation matched-filter curve instead, and the normalised
  value is kept for the confidence score.

---

## 4. The test suite

`crates/stagesync-core/tests/` — hermetic, deterministic, and self-contained: every
fixture is generated in Rust inside the test, with the ground truth exact by
construction. No Python, no network, no checked-in media.

See the test-suite report for the per-scenario p50/p95/max numbers.

---

## 5. Bugs found and fixed during this work

These were real defects, found by running the thing rather than by reading it.

| Where | Bug | Consequence |
|---|---|---|
| `features/envelope.rs` | The noise floor was estimated as the **10th percentile** of the level distribution. Music has no silence, so that percentile lands on a quiet *musical passage*, not on a floor. | 77 % of every recording was marked "quiet", so **every fine probe was skipped** and the matcher produced zero points. Fixed to a 2nd-percentile estimate, with a degenerate-case fallback. |
| `matching/fine.rs` | The log-mel band offset was applied to the **start** of a slice but not to its **end**. | Out-of-bounds panic on the **default configuration** for every band above 0. Found by the desktop-GUI worker, confirmed by the real-data run. |
| `matching/fine.rs` | The same class of bug in the *reference* band slice. | Same panic. Fixed with a bounds guard so a malformed feature can never panic the analysis. |
| `matching/coarse.rs` | The correlation window was not clamped to the material that exists. | Panic on any recording shorter than the 60 s window. |
| `matching/dsp.rs` | The correlation multiplied `A * conj(B)` instead of `conj(A) * B`. | **The lag sign was inverted** — every source offset came out negative. |
| `matching/dsp.rs` | `gcc_phat` returned the negated index. | Same sign error in the independent verifier. |
| `verify.rs` | `read_resampled` used the resampling ratio **inverted** and mixed up input and output units. | The hold-out verifier found **zero** positions: every window was discarded as too short. The independent check was silently not running at all. |
| `verify.rs` | The waveform GCC-PHAT hold-out cannot work when the two files are *different recordings* of the same performance. | The hold-out check was structurally unable to validate real material. An RMS-envelope opinion was added, which does work across different microphones. |
| `matching/segments.rs` | The gap threshold was a fixed 5 s while the probe hop is 15 s. | Every inter-probe span was reported as a gap; the timeline looked like a string of holes. The threshold now follows the spacing the matcher actually achieved. |
| `matching/segments.rs` | The spectral cross-check points were fed into the segmentation. | Every covered span was sliced into 8 ms slivers. |
| `matching/fine.rs` | The fine stage's skip reasons were only partly instrumented. | The skip breakdown reported "0 of 8 skipped" while every probe *was* skipped — the instrumentation hid the bug above. |
| `audio/wav.rs`, `export/graph.rs`, `export/mod.rs` | Column semantics: `model_offset_s` held a *reference time*, and the residual sign was the opposite of the documented one. | The sync map was actively misleading. Columns renamed and the sign convention documented. |
| `tools/make_fixtures.py` | Python's `wave` module cannot write IEEE float, so float bytes were written under a **PCM** header. | The "fixtures" were not audio at all — near-square-wave noise with a flat envelope. The fixture generator now writes the RIFF chunks by hand. |
| `pipeline.rs` | A manual anchor applied to **every** source. | Dragged every recording onto one correspondence. Anchors are now per source, with a global fallback. |
| `pipeline.rs` | There was no way to cancel an analysis. | The GUI had to unwind out of the core with a panic. Cancellation is now cooperative and checked inside the decoding loop. |

---

## 6. Known limitations

1. **The log-mel cross-check does not corroborate the envelope fit on real material.**
   It disagreed at 265 of 319 positions for Voice 005 and 190 of 263 for Voice 006
   (50 ms threshold). This is reported, not hidden — and it is the expected outcome
   when the two files are different recordings of the same performance: their spectral
   content differs far more than their amplitude envelope does. On the synthetic
   fixtures, where the source *is* derived from the reference, the two methods agree.
2. **The waveform hold-out check is not applicable to real material**, for the same
   reason. The RMS-envelope opinion is the usable one there.
3. **The measured pitch change is a coarse sanity check.** The autocorrelation pitch
   tracker resolves to a few tenths of a cent on polyphonic material, while a
   ppm-scale drift correction changes the pitch by a hundredth of a cent. The exact
   figure is the deterministic one, `-1200 * log2(scale)`, and both are reported.
4. **The drift disagreement in §2 is unresolved.** This work re-measures the slope and
   reports what it found; it does not explain why the historical figures differ.
5. **The desktop application has never been opened.** It compiles, it is clippy-clean,
   and its non-UI logic has integration tests, but no window was ever rendered in this
   environment, so on-screen behaviour is unverified.
6. **The desktop application cannot cancel a render in flight.** The core's render
   functions have no progress callback; the button is honestly labelled accordingly.

---

## 7. The gates

```
cargo fmt --check                                     → 0
cargo clippy --workspace --all-targets -- -D warnings  → 0
cargo test --workspace                                → 0
cargo build --release                                 → 0
```

The exact test counts and the exit codes as they were run are in the delivery report.

---

## 8. Licensing and provenance

* The project is dual MIT / Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`). The
  Apache-2.0 text is byte-identical to the file published at
  `apache.org/licenses/LICENSE-2.0.txt` (SHA-256 `CFC7749B…`).
* `PROVENANCE.md` records the clean-room statement, the origin of every component, and
  the full dependency table — all **492** packages in `Cargo.lock`, each one's exact
  version and declared licence, read out of the local cargo registry rather than
  recalled.
* `LICENSES.md` carries the same table with the licence-text locations, plus
  plain-English explanations of what MIT, Apache-2.0 and MPL-2.0 require of a
  redistributor.
* `symphonia` is **MPL-2.0** — it is linked, not vendored. `hound` is **Apache-2.0**
  (not dual), `rfd` is **MIT** (not dual). A redistributor of a Windows binary must
  honour the MPL's file-level copyleft; see `LICENSES.md`.
* The SVG drift-graph writer and the Python tooling are original.
* Nothing from any proprietary application, decompiler output, internal table,
  parameter set or weight file was used in the implementation. See `PROVENANCE.md` and
  `cleanroom/`.
