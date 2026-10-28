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

7. **Honest reporting.** Never present an estimate as a measurement, a stub as a
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

* Rust 2021 edition, `rust-version = 1.82` in the workspace manifest. Do not raise it.
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
* **Using a fixed coarse-window hop.** Short recordings end up with a single window,
  which can never be corroborated. Spread the windows evenly instead.
* **A fixed gap threshold in the segmenter.** With a 15 s probe hop every inter-probe
  span then looks like a gap. The threshold has to follow the spacing the matcher
  actually achieved.
* **A correlation that multiplies `A * conj(B)`** inverts the lag sign. The convention
  is `a[n] ~= b[n + lag]`, positive lag means `b` runs late.
* **Test signals made of harmonically related pure tones.** They share a common period
  and create exact aliases. Use inharmonic partials plus noise.

## Git

Local commits are fine. **Do not** push, publish a release, or upload any recording
without explicit permission from the user. `.gitignore` already excludes `audio/`,
`*.wav`, `*.m4a`, `target/`, `out/`, `work/`, `testdata/` and credential files — keep
it that way, and check `git status` before every commit.
