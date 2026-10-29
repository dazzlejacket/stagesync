# CODEX_HANDOFF.md — the state of StageSync for whoever picks it up next

This is a handoff, not a status report for its own sake. It tells you what the work
was, what state the repository is in, what is still open, and where to start.

Read [`AGENTS.md`](AGENTS.md) first. It is the operating contract, and it records a
list of mistakes that have already been made here, each with the reason it went
wrong. Most of them are easy to make again.

---

## 1. What StageSync is

A drift-aware synchroniser for several recordings of the same concert or play. It
finds where each recording sits on the reference timeline, measures how fast that
recorder's clock was running, and renders corrected audio — while refusing to invent
material that was never recorded.

Rust 2021, one shared codebase for Windows, macOS and Linux:

```
crates/stagesync-core/     all the DSP and the logic; no user interface
crates/stagesync-cli/      the `stagesync` command-line tool
crates/stagesync-desktop/  the `stagesync-desktop` egui application
tools/make_fixtures.py     a development aid for synthetic fixtures
tools/verify_exports.py    an independent numpy/scipy verifier — NOT part of the app
cleanroom/                 the pre-existing clean-room research documents
```

The desktop crate must not depend on the CLI crate, and the core must not depend on
either.

---

## 2. The state of the repository right now

**The four gates pass on Windows 11 x64 with Rust 1.99.1**, and all 133 tests pass.
Every change of this pass is committed, and a fresh clone of the result builds and passes
the same suite.

The release preparation is **not finished**. One thing stands in the way of tagging:

1. **The macOS and Linux builds have never run.** The repository has no GitHub remote,
   so the workflows that would build them have never executed. The Intel runner label
   has been corrected (`macos-13` → `macos-15-intel`), but nothing has been built on
   it.

The full account is in [`PRE_RELEASE_REPORT.md`](PRE_RELEASE_REPORT.md). Read that
rather than this file if you want the detail.

---

## 3. What was done in this pass

### Fixed, with a test or a command that proves it

- **The audio module was not tracked.** `.gitignore` had an unanchored `audio/` rule
  that matched `crates/stagesync-core/src/audio/`, so a fresh clone could not compile.
  A fresh clone now builds.
- **An export could overwrite the reference recording.** Now refused, with a message
  naming the file, before anything is written.
- **A sample rate of 77 Hz or below crashed the analysis.** Now handled: a file with
  no sample rate is refused with a clear message, and the filterbank clamps its bands.
- **The desktop window squeezed its own centre out** — the export panel grew to the
  width of a sentence and the centre was left with 124 pixels of 960. The side panels
  are capped, the long text wraps, and paths are elided in the middle.
- **The manual anchor was global**, dragging every recording onto one correspondence.
  It now belongs to the selected recording alone.
- **A drift could be shown without its uncertainty**, and a source whose drift was not
  established showed `+0.000 ppm`. Both are fixed.
- **A second job could be started**, orphaning the first one's cancel flag. Fixed.
- **A stale result could be shown against a changed file list.** Fixed.
- **A cancelled export could report success.** Fixed.
- **An export that failed part-way through left a half-written file at the
  destination.** The samples now go to a temporary file beside the destination and are
  moved into place when the file is finished, so a file at the destination is always a
  complete one.
- **Two recordings with the same file name were exported onto each other.** Their
  corrected files share a name, so the second render replaced the first. An export is
  now refused when two of its outputs name the same file.
- **The declared minimum Rust version was false** (`1.82`; the dependencies need
  1.95). Corrected, and `AGENTS.md` updated to record why.
- **Text in eight source files had been destroyed** by a Windows-1252 mis-decode,
  including a line the CLI printed to the terminal. Repaired.
- **The Intel macOS runner label had been retired.** Corrected.
- **The release workflow ran no tests**, the packaging built without `--locked`, the
  shell scripts were not executable, the `.deb` declared no libraries and carried no
  icon, `MACOSX_DEPLOYMENT_TARGET` was unset, `workflow_dispatch` could build an empty
  ref, `thiserror` was declared and unused. All fixed.

### Verified

- The four gates, on Windows 11 x64.
- The real recordings: 32.7 s to analyse, 103.8 s to export, with the same figures the
  project has always reported.
- The independent verifier: **Voice 005's export is accurate to 1.8 ms** at the
  positions it trusted. **Voice 006's export was not verified** — the verifier
  refused to give a figure because too few positions could be trusted, and its gates
  were not loosened.
- The desktop window was launched and looked at, for the first time.

### Added

- A headless layout regression test (`app::tests`), which fails on the layout that
  shipped.
- Regression tests for the export collision, the missing sample rate and the very low
  sample rates.
- `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md` and issue templates.

---

## 4. Where to start

### If you are cutting the release

1. Push to a remote and let both workflows run. Check that `macos-15-intel` is a live
   runner label on the day.
2. Make the export atomic first — see §5.
3. Run `cargo audit` and `cargo deny`. They were not installed here, so no advisory
   scan was done.
4. Decide about the `continuous` appimagetool download in `packaging/linux/build.sh`.
   Pin it to a numbered release with a checksum you have verified.
5. Run the four gates on the machine that will cut the release and record the exit
   codes.

### If you are working on the alignment or the drift

Read these before you touch anything:

- `crates/stagesync-core/src/matching/drift.rs` — the drift fit and its uncertainty.
  `fit_with_uncertainty` is the only way a drift is produced.
- `crates/stagesync-core/src/matching/waveform.rs` — the raw-waveform second opinion,
  and the hold-out probing that keeps it honest.
- `crates/stagesync-core/src/pipeline.rs` — how the two estimators are compared and
  which one is applied.

The rules that matter: a drift is a slope, and a slope fitted through points that
carry a millisecond of noise is only good to about `noise / sqrt(Sxx)`. Every drift
figure must carry the uncertainty that produced it, and when the drift does not exceed
its own uncertainty by a factor of three it is not applied at all. A disagreement
between the two estimators is reported, never averaged away.

The hold-out probing must stay where it is: at the midpoints between the fine probes,
which the fit did not use, with the search centre interpolated from the *measured*
neighbouring probes rather than from the model.

### If you are working on the desktop application

- `crates/stagesync-desktop/src/app.rs` is the whole window. The layout is split into
  `layout()`, which takes a `&mut egui::Ui`, so that it can be laid out head-lessly.
  Keep it that way: the layout regression test depends on it.
- `crates/stagesync-desktop/src/platform.rs` holds every operating-system difference.
  Do not spread one into the rest of the crate.
- The long-running work runs on a worker thread; the window only polls for results.
  Do not move any of it onto the interface thread.

---

## 5. The open items, in the order they are worth doing

1. **Build on macOS and Linux.** Nothing else in this list matters as much.
2. **Run an advisory scan**, and pin `appimagetool`.
3. **Decide what `report.json` should record about paths.** It currently records the
   absolute path of every file that was read, which is useful and also embeds the
   user's directory layout in a file that gets attached to support requests.
4. **Separate a recorder gap from an uncovered reference in the coverage figure.** The
   segments report distinguishes them; the summary figure does not.
5. **Drive the real window in continuous integration.** Only the headless layout test
   is exercised there.

---

## 6. The things that are easy to get wrong

These are in `AGENTS.md` as well, and every one of them has happened here.

- The coordinate convention is fixed everywhere:
  `reference_time = scale * source_time + offset`, `drift_ppm = (scale - 1) * 1e6`.
  `scale > 1` means the source's clock ran **slow**. Do not "fix" the sign.
- The originals are read-only, and the reference is never altered at all.
- Missing material is reported, never fabricated. If there is a gap, the timeline has a
  hole in it.
- No automatic gain. A source that is 37 dB quieter stays 37 dB quieter.
- A drift correction is a resampling. A zero-drift source is corrected with an offset
  and **no** resampling.
- A drift figure travels with its uncertainty, and an unestablished drift is not
  applied.
- The two estimators' disagreement is the point; it is reported, never averaged away.
- Never present an estimate as a measurement, a stub as a feature, or a plan as a
  result.
- A test whose verdict depends on where a block boundary happens to fall is testing
  the block size, not the hole.
- Do not loosen a verifier's trust gate to make a run green. The gates are the
  difference between a measurement and a guess.
- `audio/` in `.gitignore` must stay anchored to the root of the repository. See §3.

---

## 7. How to check your work

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

All four, with their real exit codes, before you claim that anything works. If
something fails, fix it or say exactly what is unfinished. Do not weaken a lint rule
or delete a test to make a run green unless the test is genuinely wrong — and then
say so explicitly.

The test suite is hermetic: it generates its own fixtures, uses a temporary
directory, and depends on nothing outside the test. `tools/make_fixtures.py` is a
development aid and is **not** part of the application; the Rust suite does not call
it, and it must not.
