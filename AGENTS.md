# AGENTS.md — working rules for this repository

This file is the operating contract for any human or AI agent working on StageSync.
Read it before changing anything.

## What this project is

StageSync synchronises several recordings of the same concert or play against one
reference recording, measures the recorder-clock drift of each source, and renders
drift-corrected audio. It is a **clean-room, independent implementation** — see
`cleanroom/PROVENANCE.md` and `PROVENANCE.md`.

## Non-negotiable invariants

Break one of these and the result is wrong in a way that is easy to miss.

1. **The coordinate convention is fixed everywhere:**

   ```
   reference_time_seconds = scale * source_time_seconds + offset_seconds
   drift_ppm               = (scale - 1) * 1_000_000
   ```

   `scale > 1` means the source's own clock ran **slow**, so the source's timeline is
   compressed. The correction resamples with `alpha = 1/scale`, which *lengthens* the
   file and pitches it **down** by `-1200 * log2(scale)` cents. Do not "fix" this sign.

2. **Originals are read-only.** No stage may modify, rewrite, normalise or truncate an
   input file. The reference recording is never altered at all. Output always goes to a
   separate directory. `crates/stagesync-core/tests/` asserts this by hashing the inputs
   before and after a full run.

3. **The reference soundtrack is unchanged.** StageSync aligns sources *to* the
   reference; it never re-times, re-renders or re-mixes the reference.

4. **Missing material is reported, never fabricated.** A span that no recorder captured
   is an explicit `SegmentKind::UncoveredReference` / `RecorderGap`. A span where the
   match is weak is an explicit `SegmentKind::LowConfidence`. Material is **never**
   stretched, interpolated or force-matched to make a timeline look continuous. If a
   gap exists, the timeline has a hole in it.

5. **No automatic gain.** A source that is 37 dB quieter than the reference stays 37 dB
   quieter. Output preserves the source's native channel count and level, and never
   clips beyond the representable range.

6. **A drift correction is a resampling.** A ppm-scale drift is corrected with a
   deterministic polyphase Kaiser-windowed-sinc resampler. A zero-drift source is
   corrected with a constant offset and **no** resampling — do not describe a rate
   change as "no resampling".

7. **A drift figure travels with its uncertainty, and an unestablished drift is not
   applied.** A drift is a slope, and a slope fitted through points that carry a
   millisecond of noise is only good to about `noise / sqrt(Sxx)`. Every drift figure
   StageSync reports must carry the standard error that produced it, measured by
   `matching::drift::fit_with_uncertainty`. When the drift does not exceed its own
   uncertainty by a factor of three, it is **not** applied: the correction falls back
   to the offset alone and says so. Never present a slope without the precision that
   justifies it.

8. **Two estimators, and the disagreement between them is the point.** The drift is
   measured in the RMS-envelope domain and, independently, on the raw waveform at
   positions the fit did not use. Where the raw waveforms are the same signal the
   waveform measurement is far more precise and is the one applied; where they are two
   different microphones the waveforms are uncorrelated, the waveform measurement is
   rejected, and the envelope measurement stands. A disagreement between the two is
   reported, never averaged away.

9. **Honest reporting.** Never present an estimate as a measurement, a stub as a
   feature, or a plan as a result. If a number could not be measured, say so and say
   why. Disagreement between methods is reported, not averaged away.

## Clean-room boundary

* **Never** read, open, decompile, disassemble or otherwise use: `private-research/`,
  any Ghidra output, any proprietary source, any internal table, parameter set, weight
  file or extracted constant from any closed-source application.
* The permitted inputs are: the documents in `cleanroom/`, and publicly documented DSP
  literature (listed in `cleanroom/PUBLIC_DSP_REFERENCES.md`).
* Do not clone a proprietary user interface. The UI was designed from
  `cleanroom/CLEANROOM_SPEC.md`.
* Third-party dependencies are fine, but each one's licence must be compatible and must
  be recorded in `LICENSES.md`. `symphonia` is **MPL-2.0** — it is linked, not vendored,
  and a redistributor of a binary must honour the MPL's file-level copyleft.

## Layout

```
Cargo.toml                  workspace root
crates/stagesync-core/      all DSP and logic; no UI, no I/O policy
crates/stagesync-cli/       the `stagesync` command-line tool
crates/stagesync-desktop/   the `stagesync-desktop` egui/eframe application
tools/make_fixtures.py      deterministic synthetic fixture generator (ground truth known)
tools/verify_exports.py     independent numpy/scipy verifier — NOT part of the app
cleanroom/                  the pre-existing clean-room research documents
audio/                      source recordings — READ ONLY, never committed
work/, testdata/, out/      scratch and generated output — never committed
```

`stagesync-core` must not depend on `stagesync-cli` or `stagesync-desktop`.
The desktop crate must not depend on the CLI crate.

## Build, test and lint — run all four before claiming anything works

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

Report the **real exit codes**. If something fails, fix it or say exactly what is
unfinished. Do not weaken a lint rule or delete a test to make a run green unless the
test is genuinely wrong — and then say so explicitly.

## Coding conventions

* Rust 2021 edition, `rust-version` in the workspace manifest. The floor is set by the
  dependencies rather than chosen: `eframe` and `egui` 0.37 declare `rust-version = 1.95`,
  `wgpu` 30 needs 1.87, and `symphonia` 0.6 and `clap` 4.6 need 1.85. Do not raise it above
  what the dependencies need, and do not let the manifest claim a floor the build cannot
  keep — a number that is too low produces a confusing refusal from Cargo rather than the
  real reason.
* `anyhow` at the edges, `thiserror` for typed errors in library internals.
* Public items are documented with `///` comments that explain **why**, not what.
* Prefer a clear, deterministic algorithm over a clever one. Every stage must be
  reproducible: the same input must produce byte-identical output.
* Streaming is mandatory for anything that touches the whole file. A 3-hour recording
  must not be held in memory. `AudioDecoder::for_each_block` is the workhorse.
* Tests must be hermetic: generate their own fixtures, use `tempfile::TempDir`, and
  depend on nothing outside the test — no Python, no network, no checked-in media.
* Never leave a decorative control in the UI. Every visible button must do the thing
  it says.

## Things that have already gone wrong — do not repeat them

* **Writing float WAVs with Python's `wave` module.** It cannot write IEEE float; the
  float bytes end up under a PCM header and decode to near-square-wave noise with a
  flat envelope. `tools/make_fixtures.py` writes the RIFF chunks by hand.
* **Estimating a noise floor as the 10th percentile of the level distribution.** Music
  has no silence, so that percentile lands on a quiet *musical passage* and marks most
  of the recording as "quiet" — which silently skips every fine probe. Use a low
  percentile (2%) so the estimate lands on real silence or on the recorder's hiss.
* **Ranking correlation peaks by the overlap-normalised score.** Short-overlap edge
  artefacts then outrank real matches. Rank on a fixed-normalisation curve and keep the
  normalised value for the confidence score.
* **Returning correlation peaks sorted by lag.** The caller then takes the *earliest*
  peak as the best match instead of the strongest. `find_peaks` returns them strongest
  first.
* **Using a fixed coarse-window hop.** Short recordings end up with a single window,
  which can never be corroborated. Spread the windows evenly instead.
* **A fixed gap threshold in the segmenter.** With a 15 s probe hop every inter-probe
  span then looks like a gap. The threshold has to follow the spacing the matcher
  actually achieved — and that spacing has to come from *all* the measured points, not
  from the trusted ones, or a hole raises the threshold that was meant to detect it.
* **Fitting a drift rate from the coarse stage's window lags.** Those lags are read off
  a 20 Hz decimated envelope, so they are quantised at 50 ms; one window that latched
  onto the wrong place turns into a per-cent rate error that then displaces every fine
  probe. The coarse stage finds *where* the recording is. The fine stage, with hundreds
  of probes, measures *how fast its clock ran*.
* **Making the envelope's normalisation depend on where the silence is.** The property
  the matcher depends on is that *the normalised shape of the music does not change when
  silence is added or removed nearby*. The local z-score's statistics are therefore taken
  from the **signal frames only** — precisely because that makes the shape
  silence-invariant: a silence added next to the music does not change which frames are
  signal frames in the window, so it does not change the window's mean or spread, so it
  does not change the normalised values of the music at all. The silent frames are
  excluded from the *correlation* (`ncc_correlate_masked`), where they cannot bias
  anything. Taking the statistics from every frame instead is the opposite of what the
  rule asks for: it makes the normalised shape depend on how much silence happens to be
  in the window, and it was measured to be worse — every probe of a gapped recording
  became untrustworthy.
* **A correlation that multiplies `A * conj(B)`** inverts the lag sign. The convention
  is `a[n] ~= b[n + lag]`, positive lag means `b` runs late.
* **Decoding a packet without stopping.** A decode loop that keeps pulling packets after
  a successful decode reads the *whole file* into memory in one call, whatever the
  caller asked for. Break after each packet.
* **Test signals made of harmonically related pure tones.** They share a common period
  and create exact aliases. Use inharmonic partials plus noise.
* **Asserting a drift to a few ppm on a short fixture.** A drift is a slope, and a slope
  fitted through points that carry a millisecond of noise is only good to about ±20 ppm
  over three minutes. Use a fixture long enough for the precision the assertion demands,
  or express the tolerance in the accumulated milliseconds, which is what matters.
* **Applying the robust fit's inlier mask to the wrong list.** The mask is indexed by the
  *fitted* points; applying it to every measured point marks the wrong ones as outliers
  and lets the bad ones through.
* **Trusting a whole-window correlation peak on its own.** On real music the tallest peak
  of a 20 s correlation curve is often a sidelobe that only the loudest part of the window
  agrees with. Take the **median of the overlapping sub-window peaks**, and use the
  fraction of sub-windows that agree with that consensus as the reliability gate. On the
  real recordings the whole-window estimate gave +17.3 ms where the consensus gave
  +0.07 ms at the same position.
* **Letting the envelope correlation vouch for a position.** The envelope NCC is the
  method the Rust pipeline fits its model with, so a position it alone can confirm is
  self-confirming, not verified. The independent waveform measurement is the only thing
  that can mark a position trusted.
* **Loosening a verifier's gates to make a run green.** The trust thresholds in
  `tools/verify_exports.py` (`--min-peak-ratio`, `--min-agreement`) are the difference
  between a measurement and a guess. Changing them to admit more positions is the wrong
  direction; fix the estimator instead. Both variants were run side by side on the same
  export and both sets of numbers are in `PROJECT_STATUS.md`.
* **Averaging tracker artefacts into a headline figure.** An autocorrelation pitch
  tracker on polyphonic material locks onto whichever harmonic it likes best, so
  individual pitch differences reach thousands of cents. Report the pitch change the
  *model* implies next to the measured one, count the differences that are too large to
  be pitch measurements separately, and say plainly that the tracker cannot resolve a
  correction of a few hundredths of a cent.
* **Reporting a drift without the uncertainty that produced it.** The envelope probes
  resolve the *position* to about a millisecond, but a drift of a few tens of parts per
  million over a few minutes is itself only a few milliseconds — so on a short
  recording the envelope's slope is a number without a measurement behind it. It came
  out at −47.7 ppm against a truth of +35 ppm, with a standard error of 53 ppm. The
  figure was not wrong because the fit was careless; it was wrong because nobody
  asked how well it was determined. Always fit the slope *with* its uncertainty, and
  compare the two.
* **Preferring the envelope drift when a waveform measurement is available.** The
  waveform measurement is a different estimator with different noise, and it is an
  order of magnitude more precise when the two files really are the same signal. Use
  it. Use the envelope when the waveform measurement is not available or not
  meaningful — and say which one was used.
* **Trusting a waveform measurement on two different microphones.** Two recorders of
  the same performance have uncorrelated waveforms but strongly correlated envelopes.
  The normalised correlation at the measured lag is what tells the two cases apart; a
  waveform figure whose mean correlation is low is not a measurement of anything, and
  it must be rejected rather than reported.
* **Probing the waveform at the positions the model was fitted through.** That only
  restates the model. Probe at the midpoints between the fine probes, which the fit
  did not use, and interpolate the search centre from the *measured* neighbouring
  probes rather than from the model — a model with the wrong slope would otherwise
  drag the search onto the wrong part of the recording.
* **Calling a hole in the source a recorder gap when the reference is silent there
  too.** A hole is only *lost material* if the reference has material over the same
  span. When the performance itself was quiet, the source carries its recorder's
  self-noise and nothing was missed. Check the reference before calling it a gap.
* **Writing a source's material past the end of its covered range.** The merged
  render works in blocks, and a block boundary that falls near the end of a source
  will otherwise carry that source's resampler ringing into a span that nothing
  covers — and lose the first fraction of the next source. Honour the exact ranges.
* **A test whose verdict depends on where a block boundary happens to fall.** The
  merged-timeline hole test passed only because the hole boundary landed on a block
  edge; change the model slightly and the source's own boundary ringing landed inside
  the checked region and the test failed. A test that measures the source's last
  samples is measuring the boundary, not the hole. Measure a margin inside the hole.
* **Reading a drift error off the model's own residuals.** The drift error has to be
  measured on the **rendered** file, where the residuals are flat if the rate is
  right and slope if it is wrong. Measured against the model that produced them, the
  residuals are flat by construction and the figure is meaningless.
* **Converting one file's frame indices with the other file's rate.** The log-mel hop
  is rounded to whole samples, so a 44.1 kHz file's frames are 32.0017 ms apart and a
  48 kHz file's are 32 ms apart: the two nominal rates are identical and the two
  actual rates are 143 ppm apart. A frame index must be converted with the rate of
  the file it indexes, and the rate must be the **actual** one
  (`sample_rate / hop_len`), not `1000 / hop_ms`. Getting this wrong made the spectral
  cross-check report reference times that were short by 143 ppm — 690 ms by the end of
  an eighty-minute recording — and made it disagree with the envelope fit on material
  where the two estimators were in fact agreeing.
* **Advancing a cursor by the frames it handed over.** The merged timeline's cursor
  walks each source's material once and hands it over span by span. Its position has
  to advance by exactly the frames that leave it — the ones discarded because they lie
  before the requested span, and the ones handed to the caller. A cursor that advanced
  only for the frames it *skipped* computed the next span's skip from a stale origin,
  threw away material it had already given away, and read from further along the
  recording than the plan asked for. The merged export kept about half the material it
  was supposed to contain and placed it fifteen to twenty-nine seconds away, silently.

## Git

Local commits are fine. **Do not** push, publish a release, or upload any recording
without explicit permission from the user. `.gitignore` already excludes `audio/`,
`*.wav`, `*.m4a`, `target/`, `out/`, `work/`, `testdata/` and credential files — keep
it that way, and check `git status` before every commit.
