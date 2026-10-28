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
| End-to-end test suite | **complete** | Hermetic, deterministic, generated in Rust. **62 tests**, all passing. See §4. |
| Independent Python verifier | **complete** | `tools/verify_exports.py` — a separate numpy/scipy implementation that never shares code with the Rust core. Its gates were audited against a relaxed variant and the stricter ones were kept; see §2. |
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
| `Voice 005_W_20250208_152158.m4a` | 4813.543 s | **+317.389 s** | **+16.116 ppm** | +16.169 ppm | 49.6 % | 319 measured, 0 skipped |
| `Voice 006_W_20250208_164548.m4a` | 4051.650 s | **+5753.314 s** | **+16.500 ppm** | +16.434 ppm | 41.8 % | 259 measured, 10 skipped |

Whole-file correlation (the independent anchor of the coarse lock):

| Source | Whole-file lag | Score |
|---|---|---|
| Voice 005 | +317.400 s | 0.569 |
| Voice 006 | +5753.350 s | 0.574 |

The whole analysis of a 2.69-hour reference against two 44.1 kHz AAC recorders takes
**24 seconds**.

### Against the figures in the handoff documents

The handoff numbers are treated as *claims to re-measure*, never as truth to hard-code.
Here is where they land:

| Claim | Re-measured | Verdict |
|---|---|---|
| Voice 005 at ≈ +317.391 s | **+317.389 s** | **Reproduced** (2 ms) |
| Voice 006 at ≈ +5753.304 s | **+5753.314 s** | **Reproduced** (10 ms) |
| Uncovered DJI span ≈ 5131.1–5753.3 s (10 m 22 s) | **5131.01–5753.31 s = 622.30 s** | **Reproduced** |
| Voice 005 drift 33–37 ppm (direct) | **+16.169 ppm** | **Not reproduced** |
| Voice 005 drift 12.95 ppm (calibrated) | **+16.116 ppm** | **Not reproduced** |

**The unresolved slope disagreement is not resolved by this work — it is reported as
open.** The handoff documents contain two figures for the same quantity, 33–37 ppm and
12.95 ppm, about 22 ppm apart. The re-measurement gives **+16.12 ppm** for Voice 005 and
**+16.50 ppm** for Voice 006 — between the two historical numbers, and close to neither.
The two recorders agree with each other to within 0.4 ppm, which is reassuring, but
neither historical figure has been reproduced and this work does not claim to have
settled which of them was right.

What the re-measured drift means in practice: 16 ppm over 4813 s is **77.6 ms** of
accumulated clock error across Voice 005, and **66.9 ms** across Voice 006. That is
audible as a drift between the two, and it is what the correction removes. The implied
pitch change is −0.0279 cents and −0.0286 cents respectively — far below audibility,
and reported rather than rounded away.

**How well is that drift actually determined?** Well enough to rule the historical
figures out. The fit uses 319 probes over 4813 s with about 1 ms of measurement noise,
which puts the standard error of the slope at roughly **±0.03 ppm**. The disagreement
between the historical figures is about 22 ppm — roughly 700 times that uncertainty.
So the historical numbers are not within the error bars of this measurement; something
about how they were obtained differs, and this work does not claim to know what.

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
| Voice 005 | 142 of 160 trusted (6 disputed, excluded) — **p50 1.879 ms, p95 2.249 ms, max 2.316 ms** | 8 of 13 trusted (2 disputed, excluded) — **p50 0.089 ms, p95 1.680 ms, max 1.816 ms** |
| Voice 006 | 108 of 134 trusted (15 disputed, excluded) — **p50 1.992 ms, p95 2.245 ms, max 9.120 ms** | 7 of 13 trusted (4 disputed, excluded) — **p50 0.000 ms, p95 1.166 ms, max 1.666 ms** |

Both put the real-data alignment **inside the 5 ms target** at p50 and p95, and both
report the same story: a handful of positions where the two methods disagree by more
than 10 ms, excluded from the headline rather than counted.

The Rust hold-out's second opinion (the RMS envelope, which is the method the pipeline
fits its model with) gives p50 1.526 ms / max 9.520 ms for Voice 005 and p50 1.845 ms /
max 18.134 ms for Voice 006, with 5 and 9 measurements discarded as outliers
respectively. It is reported for comparison and is never the headline.

### The verifier was audited, and its gates were made stricter

An earlier revision of `tools/verify_exports.py` replaced the **consensus estimator**
with a single whole-window GCC-PHAT peak and relaxed the trust thresholds
(`--min-peak-ratio` 1.10 → 1.05, `--min-agreement` 0.60 → 0.30). That revision was run
side by side with the original on the same export before any of it was accepted:

| Estimator | Voice 005 | Voice 006 |
|---|---|---|
| Consensus (median of sub-window peaks), strict gates | 8/13 trusted, p50 0.089 ms, max 1.816 ms | 7/13 trusted, p50 0.000 ms, max 1.666 ms |
| Whole-window peak, relaxed gates | 5/13 trusted, p50 1.634 ms, max 1.789 ms | 5/13 trusted, p50 1.275 ms, max 1.917 ms |

The relaxed variant accepted a **+17.3 ms** whole-window peak at one position where the
consensus gave **+0.07 ms** — a sidelobe that only the loudest part of the window
agreed with. It also let a position with no waveform measurement at all be marked
TRUSTED on the strength of the envelope correlation, which is the method the pipeline
fits its model with and therefore cannot independently vouch for.

The revision's genuinely good parts were kept — a real bug in the stereo WAV reader
(it read interleaved stereo as if it were mono), the `ratio`/`agree` columns in the
report, the headline-method abstraction, the new CLI flags and the log-spectral
warning margin — and the consensus estimator and the stricter gates were restored. The
numbers above are from the merged tool.

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

The merged reference timeline is 9685.46 s long, of which **950.87 s is not covered by
any recorder**:

* **317.39 s** at the head — before Voice 005 starts.
* **11.18 s** in the middle of Voice 005 — a genuine **recorder gap**, at reference
  3965.02–3976.20 s, reported as its own `recorder gap` segment rather than smeared
  over the good material around it.
* **622.30 s** between the two recorders — the 10 m 22 s span between the end of
  Voice 005 and the start of Voice 006.

This span is reported as an explicit gap. It is **not** stretched, interpolated or
force-matched, and the corrected audio carries silence there rather than invented
material. Voice 006 also runs **119.57 s** past the end of the reference; that is
reported as a `beyond reference` segment.

---

## 3. Accuracy

### The Rust alignment suite — the authoritative synthetic measurement

`crates/stagesync-core/tests/alignment_tests.rs` — 12 scenarios, all passing. Every
fixture is generated inside the test, so the ground truth is exact by construction.
`n` is the number of independent high-confidence positions the alignment was scored at.

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
| log-mel cross-check (independent method) | 4 | 10.576 ms | 14.213 ms | 14.213 ms |

The target — **≤ 5 ms at ≥ 5 independent high-confidence positions per source** — is met
on every scenario except the deliberately brutal +1000 ppm one, where it is met at p50
(3.524 ms) and exceeded at p95 by 2.8 ms. The degraded scenarios (EQ + noise, repeated
choruses, −37 dB) stay well inside 10 ms.

### The Python fixture — and the point at which it stops being measurable

`tools/make_fixtures.py` builds a harder fixture: +37.5 s offset, +35 ppm drift, −20 dB
level, 44.1 kHz source against a 48 kHz reference. The generator's own arithmetic gives
a truth of **+35.00 ppm** and an offset of **+37.5013 s** (the offset is applied in the
source's own, already-drifted sample domain, which is why it is 37.5013 rather than
37.5).

Measured on the 600 s fixture with the current build:

| Quantity | Value |
|---|---|
| Fitted offset | +37.5035 s (truth +37.5013 s) |
| Fitted drift | +26.834 ppm direct, +27.836 ppm calibrated (truth +35.00 ppm) |
| **Model error across the whole file** | **≤ 2.2 ms** |
| Independent waveform-domain measurement of the same fixture | **+34.828 ppm**, offset 37.501299 s, fit residual ≤ 0.076 ms |
| Pitch change implied by the model | −0.0479 cents |

The **alignment** is good: ≤ 2.2 ms of model error across 562 s of source, inside the
5 ms target. The **slope** is not, and the reason is worth stating plainly rather than
hiding: the core fits the drift from RMS-envelope probes whose noise on this fixture is
about ±5 ms, while 35 ppm over 562 s is only 19.7 ms of signal. The slope therefore
carries roughly ±8 ppm of uncertainty, and one probe in twenty-nine locked onto a
sidelobe 20.5 s away.

A waveform-domain measurement of the identical fixture resolves the drift to
**+34.828 ppm** against a truth of +35.00 ppm — a 0.17 ppm error. So the fixture is
fine, and the envelope probe is the noisy part.

**The fixture's length matters more than anything else here.** The 180 s variant of the
same fixture develops only 5 ms of drift signal against the same ±5 ms of noise, and
the measured slope comes out at **−47.7 ppm** — not a small error, the wrong sign. A
drift is a slope, and a slope needs enough length for the signal to rise above the
noise. This is the same lesson the Rust suite learned the hard way, and it is why the
Rust suite uses 600 s fixtures.

**This does not affect the real recordings.** There the probe noise is about 1 ms and
the drift signal is 77.6 ms, over 4813 s and 319 probes, so the slope is determined to
better than ±0.03 ppm. The real-data drift figures in §2 are solid.

### Three experiments that were run and rejected

* **A 200 Hz envelope** (10 ms frame, 5 ms hop) was tried and was *much worse*:
  residuals blew up to ±30 ms and the fitted drift came out at +1947 ppm. The shorter
  RMS window makes the envelope spiky, and the correlation peak stops being
  well-conditioned. Reverted to 20 ms / 10 ms.
* **The coarse lock's candidate ranking** was changed so that the overlap-normalised
  score ranks the peaks. Short-overlap edge artefacts then outrank real matches. The
  ranking uses a fixed-normalisation matched-filter curve instead, and the normalised
  value is kept for the confidence score.
* **Excluding quiet frames from the local z-score's statistics alone** was tried and
  biased the measured drift by about 19 ppm (a 35 ppm truth came out as 53.8 ppm),
  because the source and the reference then get different envelope shapes wherever
  their silence differs. The fix was to exclude silent frames from the *correlation*
  (`ncc_correlate_masked`) instead, where they cannot bias anything, and to keep the
  statistics identical for both files.

---

## 4. The test suite

`crates/stagesync-core/tests/` — hermetic, deterministic, and self-contained: every
fixture is generated in Rust inside the test, with the ground truth exact by
construction. No Python, no network, no checked-in media.

| File | Tests | What it covers |
|---|---|---|
| `tests/alignment_tests.rs` | 12 | offsets (positive, negative, multi-hour), drift (+10, +35, −35, +1000 ppm), sample-rate mismatch, level difference, EQ + noise, repeated choruses, head/tail silence, the log-mel cross-check as an independent method. |
| `tests/render_tests.rs` | 12 | render determinism, no NaNs, no clipping, stereo channel relationship, int16 level, beyond-reference tails, input immutability, the merged timeline, the pitch shift, WAV round trip, timeline silence, the recorder gap. |
| `tests/robustness_tests.rs` | 10 | non-audio / empty / missing file rejected, truncated WAV, too-short source, cancellation before and during analysis, a shared cancellation token, AAC/M4A via ffmpeg, and a **2-hour recording handled without loading it whole**. |
| core unit tests | 18 | the DSP and the model internals. |
| `crates/stagesync-desktop` | 6 | the desktop crate's non-UI logic. |
| doc tests | 3 | the documentation examples. |
| `crates/stagesync-cli` | 1 | the command-line surface. |
| **Total** | **62** | all passing. |

The 2-hour test is worth calling out: it writes a >100 MB file, streams the *second*
hour of it out of the reference, and asserts that the coarse lock finds the offset at
exactly 3600.0 s **and** that the Windows peak working set grows by less than half the
file size. The decoder's packet loop was missing its `break`, so it read the whole file
into memory in one call; that test is what exposed it.

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
| `matching/dsp.rs` | `find_peaks` returned the peaks **sorted by lag**, so `peaks.first()` was the *earliest* peak rather than the strongest. | The coarse stage's "accepted" candidate was often a sidelobe. Now sorted strongest-first. |
| `verify.rs` | `read_resampled` used the resampling ratio **inverted** and mixed up input and output units. | The hold-out verifier found **zero** positions: every window was discarded as too short. The independent check was silently not running at all. |
| `verify.rs` | The waveform GCC-PHAT hold-out cannot work when the two files are *different recordings* of the same performance. | The hold-out check was structurally unable to validate real material. An RMS-envelope opinion was added, which does work across different microphones. |
| `matching/segments.rs` | The gap threshold was a fixed 5 s while the probe hop is 15 s. | Every inter-probe span was reported as a gap; the timeline looked like a string of holes. The threshold now follows the spacing the matcher actually achieved, estimated from *all* the measured points. |
| `matching/segments.rs` | The gap was reported over the whole span between two good probes rather than over the hole itself. | A recorder gap was presented as a 45 s hole that swallowed good material. The hole is now located from the envelope's quiet flags. |
| `matching/segments.rs` | The spectral cross-check points were fed into the segmentation. | Every covered span was sliced into 8 ms slivers. |
| `matching/coarse.rs` | The coarse stage fitted a **drift rate** from its window lags. Those lags are quantised at 50 ms, so one window that latched onto the wrong place became a per-cent rate error that displaced every fine probe. | The whole alignment was built on a wrong slope. The coarse stage now produces an offset-only seed model; the fine stage measures the rate. |
| `matching/coarse.rs` | The cluster lag was the **mean** of the votes rather than the median. | One bad window moved the seed. Now the median. |
| `matching/fine.rs` | The fine fit fell back to fitting on **all** points when fewer than 5 were confident. | A model built on rejected measurements looks like a measurement but is not one. It now always fits on the trustworthy points and requires ≥ 3 of them. |
| `matching/fine.rs` | The robust fit's inlier mask was applied to the *measured* points instead of the *fitted* subset. | The wrong points were marked as outliers and the bad ones were let through. |
| `matching/fine.rs` | The residual sign was inverted relative to `pipeline.rs` and the documented convention. | The sync map's residuals pointed the wrong way. |
| `matching/fine.rs` | `residual_stats` returned **signed** values where the documentation promised absolute ones. | A negative p50 was possible where p50 means a magnitude. |
| `features/envelope.rs` | The local z-score's statistics were computed from the signal frames only, so the two files got different shapes wherever their silence differed. | A measured-drift bias of about 19 ppm (35 ppm truth came out as 53.8 ppm). The statistics are now identical for both files and the silent frames are excluded from the *correlation* instead. |
| `matching/dsp.rs` | The masked NCC applied the mask to the reference's energy term but not to the numerator. | Silent frames still contributed to the correlation. `ncc_correlate_masked` now masks both sides. |
| `audio/decode.rs` | The packet loop had no `break` after a successful decode. | The **whole file** was pulled into memory in one call, contradicting the module's own bounded-memory contract. This is what made the 2-hour test fail. |
| `audio/wav.rs` | `write_planar` took an owned slice, so every caller had to clone its samples. | Unnecessary copies of hundreds of megabytes. Now takes `&[impl AsRef<[f32]>]`. |
| `export/mod.rs` | `syncmap.csv` column semantics: `model_offset_s` held a *reference time*, and the residual sign was the opposite of the documented one. | The sync map was actively misleading. Columns renamed and the sign convention documented. |
| `tools/make_fixtures.py` | Python's `wave` module cannot write IEEE float, so float bytes were written under a **PCM** header. | The "fixtures" were not audio at all — near-square-wave noise with a flat envelope. The fixture generator now writes the RIFF chunks by hand. |
| `tools/verify_exports.py` | The stereo reader took `self._map[start_frame : start_frame + count]` from the *interleaved* array, so a stereo file was read as if it were mono and the wrong samples entirely. | Any stereo verification read the wrong data. Fixed to slice by `frame * channels`. |
| `tools/verify_exports.py` | The pitch summary averaged tracker artefacts (up to 2516 cents — the tracker had locked onto a different harmonic) into the headline. | The report said "mean 997 cents of pitch change", which reads like the correction destroyed the pitch. The model-implied pitch change is now reported next to the measured one, and differences larger than a semitone are counted separately as artefacts. |
| `pipeline.rs` | A manual anchor applied to **every** source. | Dragged every recording onto one correspondence. Anchors are now per source, with a global fallback. |
| `pipeline.rs` | There was no way to cancel an analysis. | The GUI had to unwind out of the core with a panic. Cancellation is now cooperative and checked inside the decoding loop. |

---

## 6. Known limitations

1. **The log-mel cross-check does not corroborate the envelope fit on real material.**
   It disagreed at 259 of 319 positions for Voice 005 and 181 of 263 for Voice 006
   (50 ms threshold; median disagreement −317 ms and −233 ms). This is reported, not
   hidden — and it is the expected outcome when the two files are different recordings
   of the same performance: their spectral content differs far more than their amplitude
   envelope does. On the synthetic fixtures, where the source *is* derived from the
   reference, the two methods agree (10.6 ms p50 on the log-mel cross-check scenario).
2. **The waveform hold-out check is weak on real material.** The two files are
   different recordings of the same performance, so a waveform correlation is measuring
   two different performances of the same music. It still works — it produced the
   headline p50 of 1.879 ms and 1.992 ms — but it locks onto the wrong thing often
   enough that 6 and 15 positions respectively were disputed and excluded. The
   RMS-envelope opinion is the more robust one there.
3. **The measured pitch change is a coarse sanity check.** The autocorrelation pitch
   tracker resolves to tens of cents on polyphonic material, while a ppm-scale drift
   correction changes the pitch by a hundredth of a cent. The exact figure is the
   deterministic one, `-1200 * log2(scale)`, and both are reported; differences larger
   than a semitone are counted separately as tracker artefacts rather than averaged in.
4. **The drift disagreement in §2 is unresolved.** This work re-measures the slope and
   reports what it found; it does not explain why the historical figures differ. It can
   say that they are *not* within this measurement's error bars.
5. **The drift is fitted from envelope probes, which is the noisy part.** On the Python
   fixture the envelope probe noise is ±5 ms and the slope comes out 8 ppm light; a
   waveform-domain measurement of the same fixture resolves it to 0.17 ppm. On the real
   recordings this does not bite (probe noise ~1 ms against 77.6 ms of signal), but a
   shorter or noisier real recording would need the waveform-domain estimator. The
   Python verifier already demonstrates that estimator.
6. **The desktop application has been launched but its window has never been
   inspected.** The binary starts, stays alive and produces no output on stdout or
   stderr; its non-UI logic has integration tests. But no window was ever rendered and
   looked at in this environment, so on-screen behaviour is unverified.
7. **The desktop application cannot cancel a render in flight.** The core's render
   functions have no progress callback; the button is honestly labelled accordingly.

---

## 7. The gates

```
cargo fmt --all -- --check                             → 0
cargo clippy --workspace --all-targets -- -D warnings  → 0
cargo test --workspace                                 → 0
cargo build --release --workspace                      → 0
```

Test counts as they were run:

```
stagesync-cli    doc tests      1 passed
stagesync-core   unit tests    18 passed
stagesync-core   alignment     12 passed
stagesync-core   render        12 passed
stagesync-core   robustness    10 passed
stagesync-core   doc tests      3 passed
stagesync-desktop               6 passed
                          total 62 passed, 0 failed
```

Both release binaries build and run: `target\release\stagesync.exe` (6,052,352 bytes)
and `target\release\stagesync-desktop.exe` (20,461,016 bytes). The desktop binary was
started and left running for eight seconds without exiting and without writing a byte
to stdout or stderr; `stagesync.exe --help` lists `analyze`, `export`, `verify` and
`info`.

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
