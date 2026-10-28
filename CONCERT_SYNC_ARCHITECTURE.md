# CONCERT_SYNC_ARCHITECTURE.md — recommended independent design for a Rust desktop application

**Purpose:** clean-room architecture for "MiniMax" — an open-source, concert/live-theater
oriented audio synchronizer with first-class **drift detection & correction**. Derived
from the empirical findings in `MUSICAL_SYNC_ANALYSIS.md`, from public DSP literature
(see `cleanroom/PUBLIC_DSP_REFERENCES.md`), and from reproducible black-box tests
(`cleanroom/BEHAVIOR_TESTS.md`). **No Revoice internals are used anywhere in this design.**

---

## 1. Design drivers from the measurements

| Observed reality | Architectural consequence |
|---|---|
| Offsets of **hundreds of seconds** (317 s / 5753 s) and recorder lead-ins | two-stage matcher: global fingerprint/coarse stage tolerant of ±hours, then local refine; never assume small offsets |
| Drift of **10–40 ppm, linear, sign-stable**; residual noise ±6–7 ms | default correction = single rational-rate resample; piecewise only on evidence (residual curvature test); ppm priors never trusted without an end-to-end calibration loop (§6) |
| **Missing material** (recorder off 10 min) and reference-tail overrun | segment model first-class: match *segments*, emit explicit gaps, never warp across them |
| 37 dB level mismatch, AAC-vs-float, mono, quiet floors | features must be level-normalized (local z-score) and band-aware; envelope + log-mel ensemble |
| Periodic music breaks naive correlation (false peaks at snr≈3) | confidence = peak-vs-sidelobe z + multi-probe agreement; **every** map point carries a confidence; downstream steps gate on it |
| Concat seams must be verified, not assumed | boundary probes around every detected/reference join |
| 2 h 41 m @48 kHz float (~1.9 GB) | chunked streaming feature stage; float32 envelopes (9.7 MB/h); memory budget ≤ 1 GB total |
| ppm-level pitch side-effect acceptable; WSOLA windowing not | resample-only correction preserves transients & stereo (channel-mapped) |

## 2. Pipeline (library-first, thin desktop shell)

```
┌────────────┐   ┌──────────────────┐   ┌───────────────────┐   ┌────────────────────┐
│ Ingest     │──▶│ Feature store    │──▶│ Match engine      │──▶│ Model fitter       │
│ decode →   │   │ env@100Hz, log-  │   │ coarse(global) →  │   │ Theil-Sen/RANSAC   │
│ per-track  │   │ mel@~31Hz, onset,│   │ fine(local ±2s) → │   │ + piecewise split  │
│ PCM f32    │   │ VAD/quiet flags  │   │ waveform PHAT spot│   │ + confidence gates │
└────────────┘   └──────────────────┘   └───────────────────┘   └─────────┬──────────┘
                                                                         ▼
┌────────────┐   ┌──────────────────┐   ┌───────────────────┐   ┌────────────────────┐
│ Verify     │◀──│ Renderer         │◀──│ Gap/segment plan  │◀──│ Calibration loop   │
│ residual   │   │ rational-rate    │   │ gaps preserved,   │   │ apply candidate →  │
│ map + pitch│   │ resample (+opt.  │   │ joins flagged,    │   │ re-match → fold    │
│ + LSD + export│  │ WSOLA fallback)  │   │ no force-align    │   │ slope/intercept back│
└────────────┘   └──────────────────┘   └───────────────────┘   └────────────────────┘
```

### Stage details

1. **Ingest** — `symphonia` (mp4/aac, wav, flac, caf) → mono f32 @ native rate for
   features; keep original bit-exact stream for rendering reference; channel-count/SR
   metadata table; per-file SHA-256. Never modify inputs.
2. **Features (chunked, rayon):** RMS envelope @100 Hz (z-scored, window 60 s);
   log-mel @~31 Hz for fingerprinting; spectral-flux onset envelope; energy-based quiet
   detector (structure: silence spans, recorder lead-ins). Store as `.npy`-equivalent
   memmap (≈ 4 MB/hour/channel each).
3. **Coarse global match** — two complementary paths:
   - envelope FFT cross-correlation of 60 s queries against the full reference (as
     validated here), *and*
   - chroma/landmark-hash indexing (Shazam-style; `PUBLIC_DSP_REFERENCES` §2) for
     offsets up to hours without O(T²) scans — hash inverted index of spectrogram peak
     pairs, LSH/RANSAC on (t1,t2) pairs.
   Output: candidate (t_a, t_b) pairs with scores.
4. **Robust model:** Theil–Sen (sampled) + iterative 3σ rejection → initial
   `t_b = a·t_a + c`; piecewise candidate split points from changepoint detection on
   residuals (only accept if both sides have n≥8 confident probes and slope delta
   > 3σ); explicit **gap segments** where confidence collapses on one side only.
5. **Fine map:** 60 s windows / 15 s grid, ±2 s search around model, parabolic refine
   on 10 ms grid → residual series; **GCC-PHAT waveform spots** (band-limited,
   4096-pt, energy-gated) at start/middle/end + random positions as an independent
   second opinion; disagreement > 30 ms ⇒ flag point, downgrade confidence.
6. **Confidence score (per point):** `σ-peak vs sidelobes`, `agreement(A,B)`,
   `energy margin`, fused to 0–1; report thresholds; NEVER align low-confidence spans
   — report as "uncertain" (UI: amber) and by default leave uncorrected.
7. **Calibration loop (mandatory):** candidate render → re-match against reference →
   fit residual line → fold into model → re-render (converges in 1 iteration when the
   model family is right; caught a −22 ppm discrepancy in this investigation).
8. **Renderer:**
   - ppm-scale rate change: **rational polyphase windowed-sinc** (continued-fraction
     ratio of the measured rate, chunked with filter-margin overlap; preserves SR,
     channels, bit depth; deterministic; SIMD via `realfft`/`rustfft` + `rubato` for
     on-the-fly variants);
   - optional (flagged): WSOLA via `soundtouch`-style engine only when |rate−1| > 0.5 %;
   - gaps: cut/stitch with explicit gap markers (EBU-style transparent, no crossfade
     invention); optional short equal-power crossfades (2–5 ms) at *existing* seams only;
   - stereo: identical time-map applied to both channels (shared resampler state).
9. **Verify & reports:** residual map at ≥5 positions/file + full fine map, LSD on
   static segments, pitch-delta, before/after excerpts, CSV + graph + JSON summary.

## 3. Rust crate sketch

| Concern | Crate |
|---|---|
| decode (wav/flac/aiff/mp4/m4a) | `symphonia` (+ `hound` for fast wav) |
| FFT | `rustfft` / `realfft` (real I/O) |
| ndarray / memmap | `ndarray`, `memmap2` |
| resampling | custom rational polyphase on `rustfft` convolutions, or `rubato` (adaptive) / `dasp_sample` interpolation fallback |
| robust regression | small in-crate Theil–Sen + RANSAC (no heavy deps) |
| hashing/fingerprint | `rustc-hash` FNV/xxhash for landmark hashing; `sled`/CSV for index |
| plotting (PNG/SVG) | `plotters` (headless backend) |
| UI (MVP) | `egui`/`eframe` (waveform via `egui_plot`), file ops `rfd`; heavy lifting stays in the core lib so CLI & GUI share code |
| CLI | `clap`; batch YAML/JSON job files |
| parallelism | `rayon` (feature/chunk embarrassingly parallel) |
| determinism/testing | fixed seeds; golden-file fixtures from `testdata/` + excerpts of this musical; `proptest` for monotonicity/invariants |

## 4. Core data model (clean-room concepts only)

```rust
struct Source { id, path, sha256, sample_rate, channels, duration }
struct SegmentMap {                 // per source pair
  matches: Vec<Match{ t_a, t_b, lag, confidence, method }>,
  model: Option<LinearModel{ a, c }>,          // or Piecewise{ breakpoints }
  gaps: Vec<Gap{ in_ref_range, reason }>,      // missing on A or B side
  verdict: Confidence{ per_point, aggregate },
}
struct CorrectionPlan { segments: Vec<RatePlan{ rate_rational, place_offset }>, gaps: Vec<Cut> }
struct VerificationReport { residuals_ms, pitch_cents, lsd_db, excerpts }
```

## 5. UX for trust (concert-specific)

- Timeline visualization: reference ruler with corrected material overlay, **color =
  confidence** (green ≥0.8 / amber 0.5–0.8 / red <0.5), gaps drawn as hatched ranges
  with plain-language cause ("recorder stopped — 10 min missing").
- Always show: estimated offset at t0, drift in ppm and in accumulated ms, residual
  stats before/after, and the calibration-iteration history (auditable).
- Exports: corrected file + `syncmap.csv` + `report.json` + optional before/after
  excerpts — everything this investigation produced is the template.

## 6. Verification/testing strategy (inherited)

- Unit: envelope/xcorr/PHAT/regression against fixtures with known synthetic ground
  truth (drift injected at ±10/35/1000 ppm, offsets ±2 h, gaps, level+EQ shifts).
- Integration: the 7 synthetic scenarios in `cleanroom/BEHAVIOR_TESTS.md` §4.2 plus the
  real-musical regression set (hashes in MUSICAL_SYNC_ANALYSIS §1).
- Property tests: monotone match maps; correction idempotence (correct∘unCorrect ≈ 0);
  determinism (bit-identical reruns).
- **Golden acceptance:** end-to-end corrected-vs-reference residuals ≤ ±5 ms at
  ≥5 positions per source (as demonstrated), pitch delta ≤ 0.25 ¢, no force-aligned
  gaps, LSD unchanged on static segments.

## 7. Performance targets (from measured costs)

- Feature stage ≈ 0.5 s per hour of audio single-core (measured envelope build) →
  full 2 h 41 m pair < 5 s; coarse FFT pass ≈ 6 s per 40 probes; fine map ≈ 5 s;
  rendering ≈ 2–4 min per 80 min mono file (polyphase) ⇒ **whole concert ≈ 5–8 min
  on a laptop**, memory < 1 GB (float32, chunked).

## 8. MVP roadmap

1. **M1 (core):** ingest → envelope/log-mel → coarse+fine match → linear model →
   calibration loop → resample render → verify report → CSV/PNG (this repo's Python
   pipeline is the executable spec).
2. **M2:** landmark-hash global index (hour-scale offsets without scanning), gap
   segmentation, piecewise-on-evidence, excerpts.
3. **M3:** GUI (load pair, waveform + confidence overlay, plan preview, one-click
   render/export), WSOLA fallback flag, batch mode.
4. **M4:** Revoice/PluralEyes black-box benchmark harness (once their export confounds
   are resolved manually) for published comparison tables.

## 9. Explicit non-goals

- No proprietary formats/code/constants; no copied implementation details; benchmark
  comparisons only from observable black-box behavior (`cleanroom/` set).
- No aggressive vocal-alignment mode by default (concert sync must remain transparent);
  warp/DTW tools remain opt-in with clear artifact warnings.
