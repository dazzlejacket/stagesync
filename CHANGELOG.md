# Changelog

All notable changes to StageSync are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The release that follows this one. Everything below has been done since the last
commit of the initial working version.

### Fixed

- **A fresh clone could not compile.** The `.gitignore` rule `audio/` was unanchored and
  matched `crates/stagesync-core/src/audio/` as well as the recordings at the root of the
  repository, so the decoder, the WAV reader and writer, the hasher and the Windows path
  helpers were never committed while `lib.rs` declared `pub mod audio;`. The rule is now
  anchored to the root of the repository, and the audio module is tracked.
- **An export could overwrite the reference recording.** The corrected file is named after
  the recording it comes from, so an export into a directory that already held a recording
  of that name landed on that recording. The export is now refused, with a message that
  says which file would have been overwritten, before anything is written.
- **A file with a very low sample rate crashed the analysis.** At 77 Hz and below the
  lowest band of the mel filterbank fell above the highest FFT bin and the band read
  outside the spectrum. A file that claims a sample rate of zero is now refused with a
  clear message, and the filterbank clamps its bands into the bins that exist.
- **The desktop window squeezed its own centre out.** egui sizes a panel to its content
  and gives it no upper bound of its own, so the export panel grew to the width of the
  sentence that described the layout and the centre — the timeline, the anchor controls
  and the results — was left with a ninth of the window, with the heading wrapping one
  letter to a line. The side panels are now capped, the long sentences wrap, and a path
  is elided in the middle rather than pushing the panel wider.
- **The manual anchor was described wrongly and behaved wrongly.** The window said the
  anchor was applied to every recording, which is exactly what the core had been changed
  to stop doing. An anchor now belongs to the selected recording alone, and the other
  recordings keep the alignment the automatic match found for them.
- **Some sources were shown as having a drift of exactly zero.** A source whose drift was
  not established is now shown as *not applied — only the offset was corrected*, and every
  drift that is shown carries the uncertainty that produced it.
- **A job could be started twice.** The anchor buttons started an analysis without
  checking whether one was already running, which orphaned the first job's cancel flag.
  The controls are now disabled while a job is in flight, and an analysis whose file list
  changed while it ran is discarded rather than shown against the wrong recording.
- **A cancelled export could still report success.** The cancel flag is now read after
  the last file is written, so a cancel that arrived during the export is reported as a
  cancellation.
- **An export that failed part-way through left a half-written WAV file behind.** The
  samples are now written to a temporary file beside the destination and moved into place
  when the file is finished, so a file at the destination is always a complete one. A
  write that is abandoned leaves the destination untouched.
- **Two recordings with the same file name were exported onto each other.** Their
  corrected files share a name, so the second render replaced the first and one of the
  two exports was lost. An export is now refused when two of its outputs name the same
  file, and the message says what to do about it.
- **Several source files carried mojibake.** Some text had been decoded as Windows-1252
  and re-encoded as UTF-8, so an em dash became `aEUR a-quote` and a sigma became
  `A-circumflex A-hyphen`. The command line printed the garbage to the terminal.

### Changed

- The declared minimum Rust version is now 1.95. It was 1.82, which the build could not
  honour: `eframe` and `egui` 0.37 declare `rust-version = 1.95`, `wgpu` 30 needs 1.87,
  and `symphonia` 0.6 and `clap` 4.6 need 1.85.
- The continuous integration job for Intel macOS now asks for the `macos-15-intel` runner.
  The `macos-13` label it used has been retired by GitHub, so the job could not have run.
- The release workflow now runs the test suite before it builds anything, and the
  packaging scripts build with `--locked`, so a release cannot be cut from a dependency
  set that differs from the one that was tested.
- The desktop application uses the colours of the system theme, so its warnings, notes
  and error messages stay legible on a light desktop as well as a dark one.
- The drift plot's legend is drawn at the right-hand end of the title row, where a long
  title no longer sits on top of it.
- `thiserror` is no longer declared: the core has no typed errors to apply it to.

### Added

- A regression test that lays the desktop window out head-lessly, at any window size, and
  checks that the centre keeps a usable share of it. It fails on the layout that shipped.
- Regression tests for the export that would overwrite a recording, for the file with no
  sample rate, and for the very low sample rates.
- `CHANGELOG.md`, `CONTRIBUTING.md` and `SECURITY.md`, and issue templates.

### Known limitations

These are not fixed. They are stated here so that nobody is surprised by them.

- The macOS and Linux builds and the packaging steps are **configured but unverified**.
  This repository has no GitHub remote, so the workflows that would build them have never
  run. The Intel macOS runner label has been changed to the current one, but nothing has
  been built on it.
- The independent Python verifier confirmed the first real export to about 1.8 ms at the
  positions it trusted, and **declined to give a figure at all** for the second one,
  because too few positions could be trusted. The verifier's gates were not loosened to
  make it produce a number.
- The application asks for at least 960×640 *logical* points. A display of 1440×900
  physical pixels at a 200 % scale offers only 720×450 logical points, so on such a
  display the window is smaller than the application asks for.
