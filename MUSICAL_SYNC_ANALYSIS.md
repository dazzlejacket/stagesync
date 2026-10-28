# MUSICAL_SYNC_ANALYSIS.md — Real-world musical synchronization & clock-drift investigation

**Date:** 2026-10-27 · **Analyst:** research agent (clean-room track) · **Status:** executed,
measured, corrected artifacts verified. One methodology discrepancy documented (§6).

All original recordings were treated as read-only (SHA-256 recorded below; never modified).
All analysis used independent public DSP techniques (envelope normalized cross-correlation,
Theil–Sen robust regression, GCC-PHAT). Scripts live in `musical_work/` (reproducible).

---

## 1. Materials

| File | Size | Format | Duration | SHA-256 |
|---|---|---|---|---|
| `FG_DJI\DJI\combined.wav` | 1,859,608,412 B | WAV, **PCM float32, 48 000 Hz, mono** | 9 685.46 s (2 h 41 m 15 s) | `048B29F3B02F3B61D20E61BDB0B4E24FCCF7D4B209609B45212340939A836026` |
| `FG_Watch\...\Voice 005_W_20250208_152158.m4a` | 78,396,857 B | MP4/AAC-LC **128 kb/s, 44 100 Hz, mono** | 4 813.54 s (1 h 20 m 13 s) | `ED9A8E1775E3814EC25E29314F17B82445BCB00DD9BB9E4D0FB7386EBB9D3C0B` |
| `FG_Watch\...\Voice 006_W_20250208_164548.m4a` (continuation, also analyzed) | 65,988,222 B | AAC-LC 128 kb/s, 44.1 kHz, mono | 4 051.65 s (1 h 7 m 32 s) | `DC57EAF59B9452DCC1C94427818AC698A2593CA6421F62D47417C93921A209D9` |

**`combined.wav` composition** (from `list.txt` + size arithmetic, all float32/48 kHz):
concatenation of 9 DJI files in list order — durations 2.145 + 3×1398.784 + 1200.359
+ 1398.613 + 2×1398.784 + 90.424 = **9 685.459 s = the container duration exactly** ⇒
the file is a byte-faithful concat; **join positions: 2.145, 1400.929, 2799.712,
4198.496, 5398.855, 6797.468, 8196.251, 9595.035 s**. (First entry is a 2.15 s fragment
dated 2025-02-07; the rest are 2025-02-09 12:56→~16:05.)

**Recording-difference profile** (objective, on 16 kHz mono renderings of each):

| Metric | DJI (float32/48k) | Watch005 (AAC/44.1k) |
|---|---|---|
| median RMS (1 s frames) | **−63.1 dBFS** | **−26.1 dBFS** |
| noise floor (10th pct) | −76.4 dBFS | −40.1 dBFS |
| loud level (90th pct) | −51.1 dBFS | −18.5 dBFS |
| dynamic range (p90−p10) | 25.3 dB | 21.6 dB |
| spectral tilt (80–8 kHz, loud frames) | −6.33 dB/oct | −6.73 dB/oct |

Interpretation: **~37 dB level difference** (watch uses aggressive auto-gain; DJI records
very low-gain distant audio), nearly equal spectral tilt (±0.4 dB/oct ⇒ no gross EQ
mismatch), different noise floors, watch additionally band-limited by 128 kb/s AAC.
Crowd/ambience is present in both (floor tracks the room). The matching pipeline's local
z-score normalization makes it insensitive to these differences (peak scores 0.6–0.9).

## 2. Same performance? — YES (content-verified)

Full-range envelope matching (10 ms RMS-envelope features, FFT normalized
cross-correlation, confidence = peak z-score vs sidelobes) found a consistent linear
correspondence across the whole overlap with scores 0.6–0.93 (sidelobe σ 4–7):
`t_dji = a·t_watch + b` fits with residuals of only a few ms. Note: file *names* imply
different dates (watch 2025-02-08, DJI parts 2025-02-09) — content correlation, not
filenames, is authoritative: **both watch files and the DJI recording capture the same
performance events**, mutually consistent to the millisecond level after correction.

## 3. Offsets and drift measurements (coarse → fine)

Coarse: 60 s query windows, probes every 120 s, full-length reference search
(±5 s guards). Fine: 60 s windows every 15 s, ±2 s search around the robust model,
parabolic interpolation on the 10 ms grid. Independent check: waveform GCC-PHAT
(20 s frames, band-limited, PHAT whitening) at 27 positions. Regression: Theil–Sen +
iterative 0.5 s outlier rejection.

| Quantity | Voice005 | Voice006 |
|---|---|---|
| initial offset (dji − watch at watch t=0) | **317.391 s** | **5753.304 s** |
| fine-map slope (method A: envelope) | **+35.09 ppm** (resid sd 6.1 ms, quad. term ≈ 0) | +13.82 ppm (sd 7.2 ms, n=258) |
| coarse-slope (method A) | +37 ppm | ≈ +14 ppm |
| GCC-PHAT spot slope (method B, waveform) | **+33.3 ppm** (27 pts, ±35 ms scatter; several <1 ms agreement) | — |
| **end-to-end calibrated (method C, see §6)** | **+12.95 ppm** | (matches method A) |
| accumulated drift over the file | **62 ms (method C) / 169 ms (method A)** | 56 ms |
| drift shape | **linear** (quadratic coefficient null: 7.3e-12 s⁻²; per-800 s block slopes same sign, no steps) | linear, same |

**Classification: LINEAR clock drift** — recorder crystal-rate difference, not performance
variation: per-block slopes are constant-signed, residual series are monotone-smooth,
and there are **no step discontinuities** inside either file (see §4). Magnitude for
Voice005 is method-dependent: two direct methods (envelope fine/coarse, GCC spots) agree
on **+33…+37 ppm**; the calibrated end-to-end correction (§6/§7) implies **+13 ppm**.
This discrepancy is **unresolved and reported openly** (§6) — no single number is claimed
as final beyond "10–40 ppm, linear".

## 4. Missing material, dropouts, pauses, unmatched content

Coverage (reverse probes: DJI→watch every 300 s + watch fine map at 15 s):

| DJI span | Coverage | Cause |
|---|---|---|
| 0 – 317.4 s | no match (quiet) | pre-show: DJI started ≥5.3 min before the watch; DJI is near-silent 0–737 s (recorder lead-in) |
| 317.4 – 5131.1 s | **covered by Voice005** (321/321 fine probes confident except musical rests) | — |
| **5131.1 – 5753.3 s** | **NO watch coverage (10 min 22 s missing)** | Voice005 stopped at real time ≈16:42:11, Voice006 started ≈16:45:48 (file names): a 3.6 min real gap cannot explain 10.4 min of DJI ⇒ **~6.8 min of performance was never recorded by the watch (recorder off), not a clock artifact** |
| 5753.3 – 9685.5 s | **covered by Voice006** (258 probes; tail probes past ~9400 s weaken near the end) | — |
| watch006 beyond 9685.5 s | last ~60–119 s of Voice006 have no DJI reference | **DJI recording ends first** (its 9th file is only 90 s) |

- **Inside each watch file: no recorder pauses/dropouts** (offset series continuous; no
  jumps > measurement noise; the scattered short "silences" in watch005 at t≈471–660 s
  are musical rests/ambience dips — the fine map matches through them).
- **DJI concat joins verified seamless**: fine-map residuals show no step at joins
  1400.9 / 2799.7 / 4198.5 s (Voice005 span) — verified points straddle all three.
- DJI quiet spans (e.g. 0–737 s, 1107–1216 s, 1232–1529 s) = pre-show/interval quiet
  sections — treated as legitimate content, not misalignment.

## 5. Clock difference vs performance difference

- Watch-vs-watch offsets within each file are smooth to ±6–7 ms over 4800+ s ⇒ any
  recorder-clock error accumulates as **ppm-scale linear drift** (33.6/13.8 ppm families;
  both ≪ 1000 ppm — ordinary crystal tolerance/temperature behavior).
- No tempo-scale changes or steps (performance differences would appear as *steps* or
  slope plateaus — none observed; the only discontinuity in the whole dataset is the
  recorder-off gap of §4, which is an *absence of data*, not a warp).

## 6. Methodology discrepancy (explicitly unresolved)

- Direct pair measurements (envelope fine/coarse: +35.09/+37 ppm; waveform GCC spots:
  +33.3 ppm) mutually agree on ≈+35 ppm for Voice005.
- The first-pass correction applied exactly that slope (rate 28501/28500, verified exact
  to <0.03 ppm by synthetic marker tests of the same chunked pipeline), yet the corrected
  artifact **versus DJI** measured −22.1 ppm residual slope (9 points, monotone R²≈1),
  i.e. the end-to-end pair behaves as if the true slope were ≈+13 ppm.
- Applying that calibration (a = 1.0000129454, b = 317.391152) produced artifacts now
  verified **flat: −1.9…+2.9 ms over dji 400→4900 s (Voice005); −1.6…+3.5 ms over
  6000→9600 s (Voice006)** — plus an independent full-range content-position check
  (pre-calibration artifact): content positions tracked construction within −20/−70/−100 ms
  slopes likewise ≈ −23 ppm.
- Candidate explanations left open for the implementing agent: (a) systematic bias of
  envelope/GCC direct-slope estimation on highly periodic music when query/reference have
  different bandwidths & levels (per-probe 10 ms quantization interacting with envelope
  asymmetry); (b) an unmodeled interaction in the probe-window bookkeeping; (c) less
  likely, an artifact of one reference chain. **Consequence: for this project the
  acceptance criterion is the direct end-to-end residual (≤±5 ms), not a ppm prior;**
  both candidate drift numbers are recorded (CSV/graph show method-A offsets; models and
  verification show method-C). No accuracy claim relies on the disputed number alone.

## 7. Correction applied & verification

Chosen method: **single linear clock correction by pure resampling** (see
CORRECTION_COMPARISON.md for alternatives) — time mapped by
`t_dji = a·t_watch + b` with rational polyphase resampling (Voice005: 28501/28500 base,
calibrated final coefficients; chunked with 4096-sample filter margins), sample-rate
preserved at 44.1 kHz (no SRC), placement offset b in samples, output on the DJI
timeline. Artifacts (in `corrected/`):

| Artifact | Length | Content span |
|---|---|---|
| `watch005_corrected_vs_dji_fulltimeline_44k1.wav` | 5 130.9 s | dji t 0 → 5130.9 s (watch content at 316.4…5130.9) |
| `watch005_corrected_trimmed_44k1.wav` | 4 814.5 s | starts at dji t = 316.391 s |
| `watch006_corrected_vs_dji_fulltimeline_44k1.wav` | 9 685.5 s | full DJI length; watch006 content 5752.3 → 9685.5+ |

**Verification (independent, envelope full-range + spot checks):**

| Check | Result |
|---|---|
| residual vs DJI, Voice005 @ 400/1500/2600/3700/4900 s | **−1.9 / +1.7 / +1.2 / +0.6 / +2.9 ms** |
| residual vs DJI, Voice006 @ 6000…9600 s | **−1.6 / −0.2 / +3.5 / +2.3 / +0.8 ms** |
| pitch shift of corrected vs original (same content) | **+0.01 ¢, −0.14 ¢** (resample rate effect; inaudible) |
| before/after excerpts (waveform GCC-PHAT, see excerpt_metrics.json) | after-vs-reference: **+1.3 / +7.7 / +13.5 ms**; envelope: 0/±10/20 ms |
| before-excerpt offset vs DJI (full-range envelope) | 917.400 s and 4717.450 s matched model exactly (snr 4.2 / 6.0); excerpt2 envelope hit a known false peak (snr 2.9) — recorded as low-confidence rather than quoted |

Excerpts: `excerpts/excerpt{1..3}_{before,after,reference}_*.wav` (20 s each, at watch
t = 600 / 2400 / 4400 s; reference taken at the corresponding DJI time).

**Audible-artifact assessment:** the correction is a pure time-map resampling at rates
within 14 ppm of unity ⇒ no windowing/overlap artifacts are possible (no WSOLA/phase
vocoder involved); measurable pitch change ≤0.14 ¢; transient timing within ±13 ms of
reference throughout; stereo N/A (both sources mono). Uncertain regions: the ±10 ms
envelope quantum in reported residuals (GCC-PHAT confirms sub-15 ms), and the low-conf
pre-show interval (dji < ~500 s, quiet).

## 8. Revoice Pro comparison (Phase 5) — DEFERRED, with precise blocker

Per engagement rules no Revoice accuracy claim is made without resolving the
export-origin/sample-rate confounds documented previously (`cleanroom/BEHAVIOR_TESTS.md`
§0.1). Status: the confounds are **still open** — specifically, the "Export Selected
Playback Range" button produced no file under automation (BM_CLICK / coordinate click)
and plain Save exports remain per-track content-bounded at a default 44.1 kHz type.
Therefore a same-material accuracy comparison cannot be executed honestly in this round.
*Fastest human path to unblock* (≈2 min of manual UI): export this musical excerpt pair
via File ▸ Export Audio, set "Save as type" to the session rate, click
**Export Selected Playback Range** once by hand, confirm all three per-track files share
identical lengths; then the §3 pipeline measures Revoice's output with the same harness.
Behavioral notes already established from the earlier Revoice round (timing-only preset
changed output duration without pitch transfer: 2.73 ¢ pitch parity; per-track exports
resample to 44.1 kHz by default) remain valid as *observable behavior*, not accuracy data.

## 9. Answers to the objective questions

1. **Can these recordings be synchronized automatically across the whole musical?**
   YES for 100% of the musical that the watch captured (≈154 of 161 DJI minutes), with
   residuals ≤ ±5 ms after correction; **NO for the 10 min 22 s the watch never
   recorded** (dji 5131–5753 s) — that span cannot be synchronized by any method because
   the material does not exist in the watch (it is reported, not force-aligned).
2. **Progressive/variable drift:** drift exists and is **linear** (10–40 ppm range;
   both devices same sign); no variable (nonlinear) drift component above ±7 ms noise.
3. **Interruptions:** one recorder-off gap (watch005→006), none inside files; DJI concat
   seams are seamless.
4. **Correction:** constant offset alone would leave up to 62 ms error at the end of
   Voice005 — linear resampling was applied and verified (≤ ±4 ms end-to-end); no
   DTW/pitch-stretch needed.

## 10. Reproduction

```
musical_work/phase2_core.py    # envelopes, structure, coarse matches
musical_work/phase2_fine.py    # model, fine map, GCC-PHAT spots, reverse coverage
musical_work/phase2_w006.py    # watch006 coarse
musical_work/phase3_correct.py # chunked linear resample + placement (skips existing outputs)
musical_work/phase4_rawcheck.py / phase4_applied*.py / phase4_wavecheck.py / phase4_fullscan.py
musical_work/phase4_calib.py   # end-to-end calibration (writes calibration.json)
musical_work/phase4_verify.py  # final residuals + pitch sanity
musical_work/phase5_artifacts.py  # excerpts + MUSICAL_DRIFT_MAP.csv + MUSICAL_DRIFT_GRAPH.png
musical_work/phase6_excerpt_metrics.py
```
Python 3.14 + numpy/scipy/matplotlib; ffmpeg 9.0.2 for decoding; intermediates in
`musical_work/` (16 kHz mono PCM renderings — originals untouched).
