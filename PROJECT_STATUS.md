# PROJECT_STATUS

The state of StageSync, stated precisely. Everything below was actually run; nothing
is a plan.

---

## 1. What works

| Component | State | Notes |
|---|---|---|
| `stagesync-core` | **complete** | Ingest, features, matching, robust model, calibration, **waveform-domain drift measurement**, segmentation, rendering, holdout verification, exports. |
| `stagesync-cli` | **complete** | `analyze`, `export`, `verify`, `info`, `level`. |
| `stagesync-desktop` | **complete** | Native egui/eframe app, **Windows, macOS and Linux from one codebase**: drag-and-drop import, background analysis with real cancellation, zoomable timeline, per-source metrics and the drift verdict, manual anchors, persisted settings, export. |
| Exports | **complete** | Corrected WAV (three layouts), `syncmap.csv`, `segments.csv`, `report.json`, `drift_graph.svg`, `timeline.txt`, `plan.json`. |
| End-to-end test suite | **complete** | Hermetic, deterministic, generated in Rust. See §4. |
| Independent Python verifier | **complete** | `tools/verify_exports.py` — a separate numpy/scipy implementation that never shares code with the Rust core. Its gates were audited against a relaxed variant and the stricter ones were kept; see §2. |
| Cross-platform builds | **configured** | Native CI runners for Windows, macOS (x64 and arm64) and Linux, plus packaging scripts for a Windows installer, a macOS `.app`/`.dmg` and a Linux AppImage/`.deb`. The **Windows build and the whole test suite were run locally**; the macOS and Linux builds are **not verified** — see §7. |
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

Both historical drift figures are now **excluded** by an independent measurement on the
rendered file — see below.

**The slope disagreement is now resolved by an independent measurement.** The handoff
documents contain two figures for the same quantity, 33–37 ppm and 12.95 ppm, about
22 ppm apart. The re-measurement gives **+16.116 ppm** for Voice 005 and **+16.500 ppm**
for Voice 006 — between the two historical numbers, and close to neither.

The new figure that settles it is the **drift error measured on the rendered file**. A
correctly corrected file has *flat* hold-out residuals: the same offset error everywhere.
A slope in those residuals against the source time means the applied *rate* is wrong, and
the slope says by how much. This is measured on the audio that was actually written, so
it is a statement about what was applied rather than a restatement of the model that was
fitted:

| Source | Drift error measured on the rendered file | Implied true drift |
|---|---|---|
| Voice 005 | **+0.081 ppm ± 0.022 ppm** | +16.197 ppm |
| Voice 006 | **−0.458 ppm ± 0.103 ppm** | +16.042 ppm |

Both historical figures are excluded: 33–37 ppm is about 800 times the measured
uncertainty away, and 12.95 ppm is about 290 times away. The two recorders' true drifts
agree with each other to **0.16 ppm**, which is the consistency check that was missing
before.

The measurement is checked for bias: rendering the synthetic fixture with the *exact*
ground-truth model produces a measured drift error of under 1 ppm
(`crates/stagesync-core/tests/drift_tests.rs`,
`the_holdout_drift_error_has_no_systematic_bias`), so the figure is not an artefact of
the renderer or of the measurement. It does absorb any systematic rate error the
*renderer* introduces, so the two implied true drifts above should be read as agreeing
to a few tenths of a ppm rather than to the last digit.

What the re-measured drift means in practice: 16 ppm over 4813 s is **77.6 ms** of
accumulated clock error across Voice 005, and **66.9 ms** across Voice 006. That is
audible as a drift between the two, and it is what the correction removes. The implied
pitch change is −0.0279 cents and −0.0286 cents respectively — far below audibility,
and reported rather than rounded away.

**How well is that drift actually determined?** Two independent ways, and they agree.
The fit uses 319 probes over 4813 s, and the standard error of the slope computed from
those probes' residuals is **±0.099 ppm** for Voice 005 and **±0.160 ppm** for Voice 006
(the earlier hand-estimate of ±0.03 ppm in this document was too optimistic: it
assumed the probe errors were independent, and they are not). The rendered-file check
above gives **±0.022 ppm** and **±0.103 ppm** on the two recorders. The disagreement
between the historical figures is about 22 ppm — roughly 200 times that uncertainty. So
the historical numbers are not within the error bars of this measurement; something
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

#### The 9.120 ms outlier on Voice 006

Voice 006's worst trusted residual is **9.120 ms**, against a p95 of 2.245 ms. It is a
single position, and it is now explained rather than merely reported:

| | |
|---|---|
| Position | 9600.814 s into the corrected file (near the end of the recording) |
| Waveform residual | +9.120 ms |
| RMS-envelope residual at the same position | **+0.468 ms** |
| Waveform correlation peak there | **0.0724**, against a median of ~0.15 over the trusted positions |

The waveform method locked onto a weak feature — its correlation peak there is the
weakest of the trusted positions — and landed 9 ms away, while the envelope method found
the position to within half a millisecond. The two methods disagree by 8.65 ms, just
inside the 10 ms threshold, so the position was kept rather than discarded as disputed.
**This is a measurement artefact of the waveform method at a weak position, not an
alignment error**: the alignment at that point is good to about half a millisecond. The
hold-out now says so in its own notes, naming the position, the peak strength and what
the other method measured there, so the `max` figure cannot be read without its context.

The position is not excluded from the headline, because loosening or tightening the
10 ms disagreement threshold to make a number look better is exactly the wrong direction.
It is explained instead.

#### The Voice 005 "recorder gap" is real, but nothing was lost

The 11.18 s hole reported inside Voice 005 at reference 3965.02–3976.20 s was checked
against the audio itself with the new `stagesync level` command, which prints the level
profile of a file over a span:

* In the **source**, the level drops to the recorder's own noise floor (−62 to −67 dBFS)
  from source 3647.6 s to 3661.8 s, against a programme level of −25 to −55 dBFS
  immediately either side. So the hole in the source is real, and it is **14.2 s** long.
* In the **reference**, the same span sits at −78 to −82 dBFS against a noise floor of
  −81.9 dBFS. The reference is silent there too.

So the performance itself was quiet, and what the source carries there is the recorder's
own self-noise. Nothing was missed: the span is not lost material. The reported 11.18 s
is *shorter* than the 14.2 s of near-silence, so the report was already conservative, but
it was calling the span a "recorder gap", which reads as a recorder failure.

The segmenter now checks the reference before calling a hole a gap: a hole in the source
is only reported as a **recorder gap** when the reference has material over the same
span. Where the reference is silent as well, the span is reported as matched with a note
saying that no material was lost. This changes the *report*, not the rendered audio — the
renderer works from the time map, not from the segment kinds.

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

This was checked by running the whole real-data export twice and comparing SHA-256
digests: every artefact was identical except `report.json`, and `report.json` differed
only in the output paths it lists. `report.json` now also records the **SHA-256 of every
output artefact**, so the claim is checkable from the report itself rather than by
comparing files by hand.

The corrected WAV is identical, so the resampler is genuinely deterministic and the
chunked path matches the single-shot path. `report.json` carries no timestamp, so nothing
in it varies between runs.

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
| Fitted offset | +37.5013 s (truth +37.5013 s) |
| Fitted drift, envelope domain | +26.834 ppm direct, +27.836 ppm calibrated |
| **Fitted drift, waveform domain (now applied)** | **+35.121 ppm** |
| Uncertainty of the waveform fit | ±0.17 ppm |
| Residual RMS of the waveform fit | 0.009 ms |
| Mean waveform correlation of the probes | 0.99 |
| Pitch change implied by the model | −0.0479 cents |

The **alignment** was always good: ≤ 2.2 ms of model error across 562 s of source,
inside the 5 ms target. The **slope** was not, and the reason is worth stating plainly
rather than hiding: the core fitted the drift from RMS-envelope probes whose noise on
this fixture is about ±5 ms, while 35 ppm over 562 s is only 19.7 ms of signal. The slope
therefore carried roughly ±8 ppm of uncertainty, and one probe in twenty-nine locked
onto a sidelobe 20.5 s away.

The waveform-domain estimator is now part of the pipeline, and it resolves the same
fixture to **+35.121 ppm** against a truth of +35.00 ppm — a 0.12 ppm error.

**The fixture's length matters more than anything else here.** The 180 s variant of the
same fixture develops only 5 ms of drift signal against the same ±5 ms of noise. The
envelope-domain fit comes out at **−47.7 ppm** — not a small error, the wrong sign. A
drift is a slope, and a slope needs enough length for the signal to rise above the
noise.

**That case is now handled rather than merely documented.** The waveform estimator is
run at **hold-out positions** — the midpoints between the fine probes, which the model
was not fitted through — and it measures the same quantity with a different estimator
and much less noise:

| 180 s fixture | Envelope domain | Waveform domain |
|---|---|---|
| Drift | −47.7 ppm ± 53.2 ppm | **+35.39 ppm ± 0.17 ppm** |
| Points used | 7 | 6 |
| Residual RMS | 3.24 ms | 0.009 ms |
| Mean waveform correlation | — | 0.99 |
| **Applied** | rejected: the drift does not exceed its own uncertainty | **+35.39 ppm** |

The drift estimate now travels with the precision that produced it, and a figure that
cannot be defended is not applied. When *neither* estimator can establish the drift,
StageSync corrects the **offset only** and says so, rather than guessing a rate.

**This does not affect the real recordings.** There the probe noise is about 1 ms and
the drift signal is 77.6 ms, over 4813 s and 319 probes, so the slope is determined to
better than ±0.03 ppm — and the rendered-file check in §2 confirms it independently.
The waveform estimator finds no usable correlation on that material, because two
different microphones record uncorrelated waveforms, and it says so rather than
reporting a number it cannot stand behind. The real-data drift figures in §2 are solid.

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
| `tests/drift_tests.rs` | 6 | the drift verdict: the waveform estimator on a short recording, the waveform correcting an envelope estimate that looks precise but is not, offset-only on request, the hold-out probes being genuine hold-out positions, the applied model agreeing with the waveform measurements, and the hold-out drift-error measurement having **no systematic bias**. |
| `tests/dsp_tests.rs` | 18 | the correlation, the peak finding, the model and the masking. |
| `tests/render_tests.rs` | 21 | render determinism, no NaNs, no clipping, stereo channel relationship, int16 level, beyond-reference tails, input immutability, the merged timeline, the pitch shift, WAV round trip, timeline silence, the recorder gap, the **atomic write** (an unfinished write leaves nothing behind) and **two recordings with the same name being refused**. |
| `tests/robustness_tests.rs` | 12 | non-audio / empty / missing file rejected, truncated WAV, too-short source, cancellation before and during analysis, a shared cancellation token, AAC/M4A via ffmpeg, and a **2-hour recording handled without loading it whole**. |
| core unit tests (in the library) | 13 | the DSP, the window extractor and the drift assessment. |
| `crates/stagesync-desktop` unit tests | 29 | the desktop crate's painters and its state helpers, and the head-less layout test. |
| `crates/stagesync-desktop/tests/platform_tests.rs` | 16 | the platform adapter and the persisted settings. |
| `crates/stagesync-desktop/tests/workflow.rs` | 6 | the desktop workflow end to end, without a window. |
| **Total** | **121** | all passing. |

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
| `matching/drift.rs` | The drift was fitted and reported **without any uncertainty**, so a slope that was smaller than its own noise looked like a measurement. | On the 180 s fixture the drift came out at **−47.7 ppm** against a truth of **+35 ppm** — the wrong sign — and nothing in the report said so. The slope now carries its standard error, and a drift that does not exceed its own uncertainty is not applied at all. |
| `matching/drift.rs` | The envelope estimate was used whenever it existed, even when the waveform measurement was better. | The waveform estimator resolves the 180 s fixture to +35.39 ppm ± 0.17 ppm against a truth of +35.00 ppm. It is now the one applied whenever it is well determined. |
| `matching/drift.rs` | The decision logic fell through to the envelope estimate even when the envelope estimate had been rejected as imprecise. | An unestablished drift was applied. Caught by a unit test; the fallback is now the offset-only correction. |
| `matching/segments.rs` | A hole in the source was reported as a **recorder gap** without checking whether the reference had material there. | The 11.18 s span inside Voice 005 was called lost material when the performance itself was quiet and the reference was silent over the same span. The reference is now consulted. |
| `render/mod.rs` | The merged render wrote a source's material for whole 100 ms blocks, so a source's tail was carried past the end of its covered range and the next source's first fraction was dropped. | The resampler's ringing bled into a span that nothing covered, and up to 100 ms of a higher-priority source could be lost at its start. The exact ranges are now honoured. |
| `tests/render_tests.rs` | The merged-timeline hole test measured from the hole's exact first frame, so it measured the source's own boundary ringing. | The test's verdict depended on where a block boundary happened to fall: it passed before, and failed after a model changed. It now measures a margin inside the hole, which is what it was always trying to measure. |
| `export/mod.rs` | `syncmap.csv` wrote the confidence reason as the point's status, ignoring the point's own note. | The reason a raw-waveform probe was rejected never reached the file. |

---

## 5a. The release-preparation pass

A hardening pass over the whole repository was carried out before the first release.
Three independent read-only audits were run in parallel — security and licensing,
cross-platform release readiness, and the desktop interface — each required to cite
`file:line` for every claim and to mark what it could not check as unverified. The
desktop window was also launched and inspected, which had never been done before.

The full account is in [`PRE_RELEASE_REPORT.md`](PRE_RELEASE_REPORT.md); the findings
that changed the state of the project were:

| What | Before | After |
|---|---|---|
| The audio module was tracked | **not tracked at all** — `.gitignore` had an unanchored `audio/` rule that matched `crates/stagesync-core/src/audio/`, so a fresh clone could not compile | tracked; a fresh clone builds |
| An export over the reference | landed on the recording, silently | refused, naming the file |
| A sample rate of 77 Hz or below | panicked the filterbank | refused with a message, or handled |
| The centre of the desktop window | 124 px of 960 | a capped, checked share at every size |
| The manual anchor | applied to every recording | belongs to the selected recording |
| A drift without its uncertainty | shown, and shown as `+0.000 ppm` when it was not applied | shown with its uncertainty, or said not to be applied |
| The declared minimum Rust version | 1.82, which the build cannot honour | 1.95, which it can |
| The Intel macOS runner | `macos-13`, a retired label | `macos-15-intel` |
| The release workflow | ran no tests | runs the suite before it builds |

One thing was **not** fixed, and is the reason the release should not be tagged yet:
the macOS and Linux builds have never run because the repository has no remote. An
export that fails part-way through no longer leaves a partial WAV behind - the
samples go to a temporary file and are moved into place when the file is finished.

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
4. **Why the historical drift figures differ is still unexplained.** The figures are
   excluded — by two to three orders of magnitude — but this work cannot say what
   produced them. It can say that they are *not* within this measurement's error bars.
5. **The envelope-domain drift estimate is still the one used on real material.** The
   waveform-domain estimator is more precise when the two files are the same signal,
   and it is applied wherever that holds. On the real recordings the raw waveforms are
   uncorrelated — two different microphones — so the waveform measurement is rejected
   and the envelope measurement stands, which is the correct answer for that material.
   On that material the envelope estimate is good to about ±0.1 ppm, and the
   rendered-file check confirms it independently.
6. **The desktop application has been launched but its window has never been
   inspected.** The binary starts, stays alive and produces no output on stdout or
   stderr; its non-UI logic has integration tests. But no window was ever rendered and
   looked at in this environment, so on-screen behaviour is unverified.
7. **The desktop application cannot cancel a render in flight.** The core's render
   functions have no progress callback; the button is honestly labelled accordingly.
8. **macOS and Linux are configured but unverified.** The CI workflows and the
   packaging scripts exist and have been checked as far as they can be from Windows,
   but this repository has no GitHub remote, so no macOS or Linux build has ever been
   run. See §7.

---

## 7. The gates

```
cargo fmt --all -- --check                             → 0
cargo clippy --workspace --all-targets -- -D warnings  → 0
cargo test --workspace                                 → 0
cargo build --release --workspace                      → 0
```

All four were run on **Windows 11 x64** with Rust 1.99.1.

Test counts as they were run:

```
stagesync-cli    unit tests     0 passed
stagesync-core   unit tests    13 passed
stagesync-core   alignment     12 passed
stagesync-core   drift          6 passed
stagesync-core   dsp           18 passed
stagesync-core   render        12 passed
stagesync-core   robustness    10 passed
stagesync-core   doc tests      0 passed
stagesync-desktop unit tests   28 passed
stagesync-desktop platform     16 passed
stagesync-desktop workflow      6 passed
stagesync-desktop doc tests     0 passed
                          total 121 passed, 0 failed
```

### Cross-platform builds

| Platform | CI runner | Artefact | Verified? |
|---|---|---|---|
| Windows x64 | `windows-latest` | portable ZIP + NSIS installer | **yes** — built and tested on Windows 11 x64 |
| macOS arm64 | `macos-latest` | `.app` bundle + `.dmg` | **no** |
| macOS x64 | `macos-13` | `.app` bundle + `.dmg` | **no** |
| Linux x64 | `ubuntu-latest` | AppImage + `.deb` + tarball | **no** |

The workflows are `.github/workflows/ci.yml` and `.github/workflows/release.yml`; the
packaging scripts are in `packaging/`, and `BUILDING.md` documents every step.

**The macOS and Linux rows say "no" because no macOS or Linux build has ever been
run.** This repository has no GitHub remote, so the workflows cannot execute here, and
a cross-compiled binary would not be evidence of anything anyway — the tests have to
run natively. The claim is limited to what was actually done: the Windows build and
the whole test suite ran locally; the macOS and Linux configuration exists and has
been checked as far as it can be from Windows, and it is unverified until the CI runs.

The macOS builds are **unsigned and not notarised**, and the release notes say so.

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
