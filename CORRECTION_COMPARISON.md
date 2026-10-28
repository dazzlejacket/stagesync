# CORRECTION_COMPARISON.md — synchronization correction methods evaluated

**Context:** same material as `MUSICAL_SYNC_ANALYSIS.md` (Voice005/006 watch vs
DJI `combined.wav`, reference = DJI timeline; drift ≈ 10–40 ppm linear; one 10 min 22 s
uncovered span; no in-file interruptions). Goal: least-invasive method that keeps the
musical artifact-free. All numbers measured, not estimated, unless marked.

## Method comparison

| # | Method | Measured accuracy (end-to-end vs DJI) | Artifacts / cost | Verdict |
|---|---|---|---|---|
| 1 | **Constant offset only** (align at first match, no rate change) | 0 ms at t=0 by construction; **+62 ms error by end of Voice005** (12.95 ppm × 4813 s; +169 ms if the +35 ppm direct-slope interpretation holds). Mid-file error at 40 min ≈ 29–75 ms | none (pure edit/offset) | **INSUFFICIENT** for >30 min material: residual error reaches frame-level amplitudes (a 62 ms slip makes tight unison hits visibly late; risky for A/V re-sync targets of ±20 ms) |
| 2 | **Linear clock-drift correction by high-quality resampling** (polyphase windowed-sinc, rate a, placement b) — **CHOSEN** | **−1.9…+3.5 ms** residuals across both files' full covered spans (10 probe positions, ±4 ms bound); before→after excerpt GCC-PHAT: ≤ +13.5 ms (envelope ≤ ±10 ms quantum) | pitch changes by ≤ **0.14 ¢** (= rate error, inaudible; measured at 3 positions); no window/overlap artifacts possible (no frame processing); HF preserved (windowed-sinc, no decimation stages); runtime ≈ 2–4 min per 80 min file (chunked) | ✅ **RECOMMENDED** — accurate, minimal, verifiable |
| 3 | **Piecewise-linear / smoothly varying drift correction** | Would fit the −22…−35 ppm methodology bracket or any slow nonlinearity; measured quadratic term of the direct model = **7.3e-12 s⁻² ≈ null** ⇒ no nonlinearity above the ±6–7 ms noise floor. A per-segment (e.g., per DJI concat join) rate model changes nothing measurable vs single-slope (verified: residual line flat through joins 1401/2800/4199/5399 s) | identical to #2 per segment; extra parameters risk over-fitting measurement noise | **NOT NEEDED** for these data; keep as fallback if a future recording shows residual curvature > noise |
| 4 | **Constrained DTW** (genuinely nonlinear alignment / interruptions) | No within-file nonlinearity exists to correct (§ analysis). The only interruption is *missing material* (watch005→006 gap, DJI tail) — DTW **cannot invent absent audio**; forcing alignment there is forbidden by the brief. DTW would only add rubber-sheet warping on already-flat residuals | warping artifacts risk (local smear of transients/vibrato), higher CPU/memory, harder to verify determinism | **NOT APPLIED** (no evidence of need; would degrade transparency). Recommended in architecture only for gap-crossing *stitching* with explicit unmatched-segment reporting |
| 5 | **Pitch-preserving time stretching** (WSOLA/phase vocoder for the drift) | Unnecessary: required rate change is 10–40 ppm; resampling's pitch side-effect (≤ 0.24 ¢ even at 40 ppm) is orders of magnitude below audibility, while WSOLA would impose window-length granularity (~±5–40 ms) — **worse than the ±1–4 ms achieved by resampling** — plus overlap-add phasiness on sustained vocals and transient smearing at short windows | windowing artifacts (phasiness, transient double-images) on ~150 min of vocals | **REJECTED**: strictly worse accuracy AND artifacts for ppm-scale corrections. Reserve for ≥0.5 % corrections where pitch side-effects would become audible (not present here) |

## Special cases handled explicitly

- **Missing 10 min 22 s (dji 5131–5753 s):** no correction is applied; the corrected
  Voice005 simply ends at dji 5130.9 s and the corrected Voice006 starts at 5752.3 s —
  the gap remains an *explicit gap* (never force-aligned; flagged in the drift map).
- **Watch006 tail beyond DJI end (~60–119 s):** preserved in the corrected file; flagged
  as "beyond reference" (no reference exists to verify against).
- **Pre-show region (dji < ~500 s):** low-confidence matches (quiet); correction is
  model-driven there (linear extrapolation), which is safe precisely because the model
  is linear.

## Recommended approach (final)

1. Coarse→fine envelope matching + robust linear fit (as implemented).
2. **End-to-end calibration loop**: generate the candidate correction, re-measure the
   corrected-vs-reference residual line, and fold its slope/intercept back into the
   model (this caught and removed the −22 ppm discrepancy; acceptance = residual flat,
   not a ppm prior).
3. Single linear polyphase resample with exact rational rate + integer placement;
   sample rate preserved; chunked with filter margins; deterministic.
4. Verify at ≥5 positions per file (beginning/middle/end) + waveform GCC-PHAT spot
   checks + pitch sanity; publish residuals (done: ≤ ±4 ms, ≤ 0.14 ¢).

## What was deliberately NOT done

- No DTW over unmatched sections; no crossfades faking continuity across the 10 min gap;
- no sample-rate conversions (stays 44.1 kHz in, 44.1 kHz out; reference 48 kHz never
  resampled either — comparisons resample *copies* for measurement only);
- no spectral/waveform modification beyond time-mapping (bit-depth preserved: int16 → int16).
