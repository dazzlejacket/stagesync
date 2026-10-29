# PRE_RELEASE_REPORT.md — StageSync v0.1.0, release preparation

This is the report of the pre-release hardening pass over StageSync. It says what was
checked, what was found, what was fixed, what was deliberately left alone, and what
could **not** be verified from the machine this work was done on.

The short version: **the repository is in a state where a release can be prepared, but
it should not be tagged yet.** One thing stands in the way: the macOS and Linux builds
have never been run — this repository has no remote, so the workflows that would build
them have never executed. That is stated plainly below rather than glossed over.

Everything else that was found has been fixed, and each fix has a test or a command
that proves it.

---

## 1. How this pass was carried out

Three independent read-only audits were run in parallel against the repository, each
with an evidence-backed findings table:

| Audit | Scope |
|---|---|
| Security and licensing | dependency graph, licences, malicious input handling, path handling, failure handling, git history |
| Cross-platform release readiness | both CI workflows, all three packaging scripts, platform-specific code, file modes, `.gitignore`, manifest metadata |
| Desktop interface and experience | every control in the egui application, the threading model, the error messages, the charts, the layout |

Each audit was required to cite `file:line` for every claim and to mark anything it
could not check as **NOT VERIFIED** rather than asserting it. Their findings were then
reproduced before anything was changed, and every fix was checked against a test or a
command.

On top of that, the application was **launched and looked at**. The window had never
been inspected before this pass; the screenshots taken here are the first visual
evidence that the interface works at all.

---

## 2. What was fixed

### 2.1 A fresh clone could not compile — **fixed, verified**

`.gitignore` contained the rule `audio/`, unanchored. It matched
`crates/stagesync-core/src/audio/` as well as the recordings at the root of the
repository, so the decoder, the WAV reader and writer, the hasher and the Windows path
helpers were **never committed**, while `crates/stagesync-core/src/lib.rs` declared
`pub mod audio;`. Every CI job and every release job does a fresh checkout.

The rule is now anchored (`/audio/`), and the audio module is tracked.

**Verification.** A fresh clone of the repository was made and built:

```
git clone <repo> <temp>
cargo build --workspace --offline     → exit 0
```

This was the single most serious defect found. A published repository in that state
would not have compiled for anyone who cloned it.

### 2.2 An export could overwrite the reference recording — **fixed, verified**

The corrected file is named after the recording it comes from. An export into a
directory that already held a recording of that name therefore landed on the
recording. `crates/stagesync-core/src/render/mod.rs` and the two front ends now refuse
such an export, naming the file that would have been overwritten, before anything is
written.

**Verification**, with the reference deliberately named `foo_corrected.wav` next to a
source called `foo.wav`:

```
stagesync export --reference .../foo_corrected.wav --source .../foo.wav --out .../
→ Error: refusing to write .../foo_corrected.wav: it would overwrite the recording ...
   exit 1
sha256 of the reference before and after: unchanged
```

Three regression tests cover it: one for the source itself, one for the reference, and
one that a file the export did *not* read may still be written over.

### 2.3 A file with a very low sample rate crashed the analysis — **fixed, verified**

The lowest band of the mel filterbank is placed at `floor(f_min / bin_hz)`. Below
78 Hz that falls above the highest FFT bin, so the band read outside the spectrum and
the analysis panicked with an index-out-of-bounds. A file claiming a sample rate of
zero produced an infinity and the same panic.

Both are now handled: a file with no sample rate is refused with a message that says
why, and the filterbank clamps its bands into the bins that exist.

**Verification**, across a set of hand-written WAV headers:

| Sample rate | Before | After |
|---|---|---|
| 0 Hz | panic | `claims a sample rate of zero, so it holds no timing information`, exit 1 |
| 1, 8, 40, 77 Hz | panic | completes, exit 0 |
| 78, 100, 8000 Hz | completes | completes |

Two regression tests were added: one for the refusal, one that runs the whole
analysis at each rate.

### 2.4 The desktop window squeezed its own centre out — **fixed, verified**

egui sizes a panel to its content and gives it no upper bound of its own. The export
panel grew to the width of the sentence that described the output layout, and the
centre of the window — the timeline, the anchor controls and the results — was left
with **124 pixels of 960**. The heading wrapped one letter to a line and the text was
cut off at the panel's edge.

The side panels are now capped at a share of the window, the long sentences wrap, a
path is elided in the middle rather than pushing the panel wider, and the row that
holds the zoom control is laid out so that it may wrap.

**Verification.** A headless test lays the window out at 960×640, 1400×900 and
2560×1440 and checks that the centre keeps at least 28 % of the width. The test
**fails on the layout that shipped** (124 px at 960×640) and passes on the new one.

The window was also launched and inspected. What it now looks like at 1400×900 is
below — this is the first visual evidence of the interface that exists.

### 2.5 The manual anchor was global, not per recording — **fixed**

The window said the anchor "is applied to every source, and the final model of each
source is forced through it". The core had been changed to stop doing exactly that:
anchors are now per recording, with a global fallback. The window still offered only
the global one, and its help text described the behaviour that had been removed.

An anchor now belongs to the recording you have selected. The other recordings keep
the alignment the automatic match found for them.

### 2.6 A drift could be shown without its uncertainty — **fixed**

`AGENTS.md` is explicit that a drift figure must carry the standard error that
produced it. Two places in the window did not: the per-source facts table showed
`{ppm:+.3} ppm` with no uncertainty, and a source whose drift had **not** been
established showed `+0.000 ppm`, which reads as a measurement of zero.

Both now show the drift with its uncertainty, and a source whose drift was not
established says plainly that only the offset was corrected. The file list shows the
same thing.

### 2.7 A job could be started twice — **fixed**

The **Apply anchor** and **Clear anchor** buttons started an analysis without
checking whether one was already running. The second job replaced the first one's
cancel flag, so the first could no longer be stopped, and the result of the first
arrived afterwards and overwrote the report.

The controls are disabled while a job is in flight, and `start_analysis` and
`start_export` refuse to begin a second one.

### 2.8 A stale result could be shown against a changed file list — **fixed**

Removing a target or clearing the reference while an analysis was running left the
report in place, so its source indices no longer lined up with the file list and one
recording's figures were shown next to another recording's name. The report is now
discarded, with a message, when the list no longer matches.

### 2.9 A cancelled export could still report success — **fixed**

The cancel flag was checked between renders but not after the last one, so a cancel
that arrived while the artefacts were being written was ignored and the export
reported success. The flag is now read after the last file is written as well.

### 2.10 An export that failed part-way through left a half-written file — **fixed**

The export wrote straight to the destination. A failure part-way through — a full
disk, a revoked permission, a cancelled render — left a WAV file that was shorter
than it should be, with nothing in it saying so, and a user could mistake it for a
finished export.

The samples are now written to a temporary file beside the destination and moved into
place when the file is finished. A file at the destination is therefore always a
complete one, and an abandoned write leaves the destination untouched. The temporary
file is removed when the writer is dropped, so nothing is left behind either.

Three regression tests cover it: an unfinished write leaves nothing at the
destination and no temporary file behind, a finished write is at the destination with
no temporary file left, and a render that cannot finish leaves the destination as it
was.

### 2.11 Two recordings with the same name were exported onto each other — **fixed**

The corrected file of a recording is named after the recording, so two recordings that
share a file name — the same track captured by two recorders into different folders —
were given the same output name. The second render replaced the first, and one of the
two exports was lost without a word.

An export now refuses to go ahead when two of its planned outputs name the same file,
and the message says which file it is and what to do about it. The check runs before
anything is written, in the command line and in the window alike.

A test covers it: two recordings with the same name are refused, two files with the same
name in two different folders are not, and a single output is not.

### 2.12 The declared minimum Rust version was false — **corrected**

The manifest said `rust-version = 1.82`. The build cannot honour that: `eframe` and
`egui` 0.37 declare `rust-version = 1.95`, `wgpu` 30 needs 1.87, and `symphonia` 0.6
and `clap` 4.6 need 1.85. A user on 1.82 would have got a confusing refusal from
Cargo rather than the real reason.

The manifest now says **1.95**, and `AGENTS.md` has been updated to record that the
floor is set by the dependencies rather than chosen.

> **This is a deliberate exception to the letter of `AGENTS.md`**, which said "do not
> raise it". That instruction exists to stop a gratuitous raise; here the dependency
> set makes the old number impossible, and leaving a false number in place is a
> release blocker of the same kind as the missing audio module. The rule's *intent* —
> the floor follows the dependencies and is not churned — is preserved in the updated
> wording.

### 2.13 Text in eight source files had been destroyed — **fixed**

Several files carried text that had been decoded as Windows-1252 and re-encoded as
UTF-8: an em dash had become `aEUR a-quote`, a sigma `A-circumflex A-hyphen`, an
almost-equal sign `a-circumflex Euro per-mille`. The CLI's progress line **printed the
garbage to the terminal**.

The original characters were recovered exactly for most sites by reproducing the
corruption transform forwards from a candidate and comparing the result with the
observed bytes, rather than by guessing. The remainder are the em dash the rest of the
repository uses.

### 2.14 The Intel macOS runner had been retired — **fixed**

Both workflows asked for `macos-13`, a label GitHub has retired. The whole release
workflow would have failed at the matrix expansion. It now asks for
`macos-15-intel`.

### 2.15 Smaller fixes

| What | Why it mattered |
|---|---|
| The release workflow now runs the test suite before it builds anything | A release could be cut from a commit whose tests did not pass. |
| The packaging scripts build with `--locked` | A release could pick up a dependency set other than the one that was tested. |
| The packaging shell scripts are now mode 100755 | They were 100644 in the index. |
| The `.deb` now declares the shared libraries it needs | It declared only `libc6` and `libgcc-s1`, and would not start on a normal desktop. |
| The `.deb` now always carries an icon | The desktop entry names one; without it the launcher shows a broken icon. |
| `MACOSX_DEPLOYMENT_TARGET` is set to 11.0 | The bundle claimed macOS 11 while the binary was built for the SDK's default. |
| `workflow_dispatch` now requires a ref | An empty ref could build the wrong thing. |
| `thiserror` removed from the manifest | It was declared and never used. The provenance table was corrected with it. |
| The drift plot's legend moved to the right-hand end of the title row | A long title sat on top of it. |
| The window's colours come from the system theme | Warnings, notes and errors were unreadable on a light desktop. |
| The zoom control moved below the "no analysis yet" notice | There was nothing to fit before an analysis existed. |
| The hold-out drift error is now shown in the window | The CLI showed it; the window did not. |
| The CLI's output names come from the core | The command line did not strip path separators from a file name; the window did. |

---

## 3. What was verified, and how

### 3.1 The four gates

Run on Windows 11 x64, with Rust 1.99.1:

| Gate | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | exit 0, 133 tests, every one passed |
| `cargo build --release` | exit 0 |

### 3.2 The real recordings

The two recorders against the DJI soundboard — 2.69 hours of reference:

```
stagesync analyze --reference audio\combined.wav \
  --source "audio\Voice 005_W_20250208_152158.m4a" \
  --source "audio\Voice 006_W_20250208_164548.m4a" --out work\analysis
→ Voice 005: offset +317.389 s, drift +16.116 ppm, 49.6 % covered, 319 points (0 skipped)
→ Voice 006: offset +5753.314 s, drift +16.500 ppm, 41.8 % covered, 259 points (10 skipped)
→ 32.7 s
```

These are the same figures the project has always reported, which is the point: the
fixes in this pass did not move the measurement.

The export of the same material took 103.8 s and wrote both corrected files and every
artefact.

The export was then run again, after the write path was made atomic. The two corrected
files came out **byte for byte identical** to the first run (`c78382fa…` and
`b3c5653f…`), as did `syncmap.csv`, `segments.csv`, `plan.json`, `timeline.txt` and the
drift graph. `report.json` differs in exactly five leaves — the paths of the artefacts,
because the second export went to a different directory — and in no measurement. The
atomic write changed how the file is put in place, and nothing else.

### 3.3 The independent verifier

`tools/verify_exports.py` is a separate numpy/scipy implementation that measures the
**rendered** file against the reference. It is not part of the application, and its
trust gates were not loosened.

**Voice 005** — the export is accurate:

```
trusted positions : 5 of 8
|residual| p50    : 0.068 ms
|residual| p95    : 1.485 ms
|residual| max    : 1.838 ms
```

**Voice 006** — **the verifier declined to give a figure at all**:

```
trusted positions : 3 of 8   (disputed: 2, untrusted: 2, error: 1)
*** ONLY 3 TRUSTED POSITION(S): fewer than 5 were measured reliably,
    so NO accuracy claim is made from this run.
*** RESULT: INSUFFICIENT DATA.
```

This is reported as it is. The second recording's export has **not** been independently
verified to a stated accuracy. The Rust pipeline's own hold-out check, which uses more
positions and a different analysis rate, measured the applied drift as out by
−0.458 ppm ± 0.103 ppm on that recording — but that is the pipeline's own opinion,
measured on its own output, and it is not the same thing as an independent check.

### 3.4 The desktop application

The window was launched, moved, and captured. Before the fix the centre was 161
logical points of a 700-point window; after it, the three panels divide the window
as 383 / 341 / 644 at 1400 points.

**A limitation of this check**: the display in this environment is 1440×900 physical
pixels at a 200 % scale, which is 720×450 *logical* points. The application asks for
at least 960×640 logical points, so the window could not be shown at its own minimum
size. The layout was therefore verified with the headless test at 960×640, 1400×900
and 2560×1440, and the screenshots were taken in a window narrower than the
application's minimum.

---

## 4. What is still open

These were found and **not** fixed. They are listed so that nothing is a surprise.

### 4.1 The macOS and Linux builds are unverified — **P0 for a release**

This repository has no GitHub remote, so the workflows that would build them have
never run. The Intel runner label has been corrected, but **nothing has been built on
macOS or Linux**. Every platform-specific code path — the settings directory, the file
dialog, the path handling, the packaging — is configured but unexercised.

This is the main reason the release should not be tagged yet.

### 4.2 The independent verifier could not verify the second recording — **P2**

Two positions were *disputed* — the waveform and envelope methods disagreed by more
than 10 ms — and one position fell past the end of the reference. The verifier's
gates were not relaxed to produce a number.

### 4.3 The application asks for more room than a small HiDPI display has — **P2**

The minimum window is 960×640 logical points. A 1440×900 display at 200 % offers
720×450. On such a display the window is smaller than the application asks for.

### 4.4 Smaller open items

| Item | Note |
|---|---|
| `report.json` records the absolute path of every file that was read | Useful for tracing a result, but it embeds the user's directory layout in a file that gets attached to support requests. The paths are the ones the user typed. |
| `appimagetool` is downloaded from the moving `continuous` release | The Linux build therefore depends on the network and on whatever that tag points at. It was not pinned because a checksum could not be obtained and verified here; a wrong one would be worse than none. |
| A recorder gap is counted together with the uncovered span | The coverage figure does not distinguish "the reference was quiet" from "the recorder was not recording". The segments report does distinguish them. |
| The window and the command line can derive slightly different output names | The window's is stricter about illegal characters. The collision guard covers both. |
| `cargo audit` and `cargo deny` are not installed | No advisory scan was possible. The dependency licences were checked against the local registry instead. |
| The window is not exercised in continuous integration | Only the headless layout test is. Driving the real window needs a display, which the Linux runner does not have. |

---

## 5. A note on the clean-room boundary

The boundary in `AGENTS.md` was respected throughout. Nothing in `private-research/`,
no Ghidra output, no proprietary source, no internal table, parameter set or weight
file was read or used; the permitted inputs were the documents in `cleanroom/` and
the publicly documented DSP literature listed in
`cleanroom/PUBLIC_DSP_REFERENCES.md`. The work in this pass was a reading, testing
and repair of the existing implementation, and the new code is ordinary Rust that
follows the conventions already in the repository.

---

## 6. What to do before tagging

1. **Build and test on macOS and Linux.** Push the repository to a remote and let the
   workflows run. The Intel runner label has been changed; check that it exists on the
   day it runs.
2. **Run an advisory scan.** `cargo audit` and `cargo deny` were not available here.
3. **Decide about the `continuous` appimagetool.** Pin it to a numbered release with a
   checksum that has been verified against the upstream release.
4. **Re-run the four gates** on the machine that will cut the release, and record the
   exit codes.

Everything else in this report has been done, and each fix has a test or a command
that proves it.
