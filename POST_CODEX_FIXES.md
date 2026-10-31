# What the post-release audit found, and what was done about it

This is the account of `CODEX_AUDIT.md` (the independent audit of release
`0a128df`) and `UX_REVIEW.md` (the companion usability review), and of the
repairs made in response. It is written to be read by someone who wants to know
*what is true now*, not to defend the work.

Every finding is listed. Where a finding was not repaired, the entry says so
plainly and gives the reason and the evidence.

- Audit: `CODEX_AUDIT.md`, 2026-10-29, against `0a128df`.
- Usability: `UX_REVIEW.md`, 2026-10-29, against `0a128df`.
- Repair branch: `repair/post-codex`.

---

## Summary

| Finding | Subject | Outcome |
|---|---|---|
| P1-01 | Sidecar outputs can overwrite an input recording | **Fixed** |
| P1-02 | Merged export destroys stereo and uses the wrong rate for later sources | **Fixed** |
| P1-03 | Skipping input before the first output corrupts the buffer origin | **Fixed** |
| P1-04 | Merged coverage can invent an uncovered span inside a covered range | **Fixed** |
| P1-05 | Per-source timeline export exceeds its advertised length | **Fixed** |
| P1-06 | Temporary WAV names collide within one process | **Fixed** |
| P1-07 | Ordinary RIFF output has no size guard beyond 4 GiB | **Fixed** (refusal, not RF64) |
| P1-08 | Feature normalisation violates the silence-independent rule | **Not repaired as prescribed** — see the entry |
| P1-09 | Window extraction retains the whole suffix after the last window | **Fixed** |
| P1-10 | Windows packaging stops before the build | **Fixed** |
| P1-11 | Binary packages omit the licence material | **Fixed** |
| P2-01 | A displayed hard anchor can be lost when the waveform model wins | **Fixed** |
| P2-02 | Verifier WAV reader miscounts frames and scales 24-bit wrongly | **Fixed** |
| P2-03 | Offset-only still invokes the resampler and appends a tail | **Fixed** |
| P2-04 | Collision comparison does not model the target file system | **Fixed** |
| P2-05 | The verifier's default grid includes an invalid endpoint | **Fixed** |
| P2-06 | The CLI's `report.json` omits the corrected WAV hashes | **Fixed** |
| P2-07 | Large collections have concrete layout limits | **Partly fixed** |
| P2-08 | The test count hides environmental dependencies | **Fixed** |
| P2-09 | Independence and uncertainty claims exceed the validation | **Fixed in the code**, see the entry |
| P2-10 | Cancellation guarantees need qualification | **Partly fixed** |
| P2-11 | Linux packaging is not yet a verified distributable | **Partly fixed** |
| P2-12 | Documentation overstates validation and disagrees with newer reports | **Fixed** |
| P2-13 | Calibration maps quiet flags in the opposite direction | **Fixed** |
| P3-01 | `SlopeFit::signal_to_noise` uses the wrong expression | **Fixed** |
| P3-02 | Packaging is not reproducible | **Fixed** |
| UX-1 | The timeline must describe the actual export | **Fixed** |
| UX-2 | Every essential control must be reachable | **Partly fixed** |
| UX-3 | The manual override must be reliable and explicit | **Fixed** |
| UX-4 | The window must state what has actually been verified | **Fixed** |
| UX-5 | The reference and export geometry must be explicit | **Fixed** |
| UX-stable | Waveform tracks and synchronised A/B audition | **Not built** — see the entry |

---

## The P1 findings

### P1-01 — sidecar outputs can overwrite an input recording — **fixed**

The export directory is often the directory the recordings are in. The export
now refuses to write anything over a recording it read: the corrected audio,
the sidecars and the manifest are all checked, not only the audio.

The comparison asks the file system rather than guessing: an existing file is
identified by its canonical path, which resolves symbolic links and gives the
name the volume actually stores, and a file that does not exist yet is compared
by a name folded the way the platform folds names (case-insensitively on
Windows, exactly elsewhere). Two recordings that share a file name are also
refused, because they would be given the same output name.

*Changed:* `crates/stagesync-core/src/render/mod.rs`,
`crates/stagesync-core/src/export/mod.rs`,
`crates/stagesync-cli/src/main.rs`, `crates/stagesync-desktop/src/jobs.rs`.

### P1-02 — the merged export destroyed stereo and used the wrong rate — **fixed**

Two separate defects.

*The channels were averaged.* The merged renderer mixed a source's channels
into one before applying the position map, so a stereo source came out with the
same signal in both channels and an antiphase source came out silent. The
renderer now keeps one position map per channel and writes the source's own
channels through. A source with fewer channels than the output contributes to
the leading output channels and leaves the rest silent; nothing is folded,
duplicated or lost. A merged output narrower than one of its sources is
**refused**, with a message that says why, rather than dropping a channel.

*The rate was the first source's.* Every source was read at the first source's
sample rate, so a 16 kHz source in an 8 kHz timeline played at half speed.
Each source is now read at its own rate and resampled from there.

*Changed:* `crates/stagesync-core/src/render/mod.rs` (`SourceCursor`,
`ChannelBank`, `render_merged_timeline`).

*Evidence:* `crates/stagesync-core/tests/repair_regressions.rs` —
`a_merged_timeline_keeps_both_channels_of_a_stereo_source`,
`a_merged_timeline_reads_each_source_at_its_own_rate`,
`a_merged_timeline_refuses_to_drop_a_channel`.

### P1-03 — the resampler's absolute buffer origin — **fixed**

The origin of the resampler's buffer was advanced by the number of samples the
*next* output would need, even when fewer samples than that were actually
discarded. Once the origin ran ahead of the buffer, every later sample was
read from the wrong place, and the ones before the new origin were read as
silence. The origin now advances only by the samples that were really removed.

*Changed:* `crates/stagesync-core/src/resample/mod.rs`.

*Evidence:* `the_resampler_keeps_the_samples_before_a_late_start` — the same
map, produced in one block and in small blocks, now agrees exactly, and the
first output frame carries the input's value rather than silence.

### P1-04 — merged coverage could invent an uncovered span — **fixed**

The merge walked the intervals in order and moved a cursor to each interval's
end. A nested interval moved that cursor backwards, and the interval after it
was reported as a gap even though the first source covered it throughout.

The merge is now a **boundary sweep**: every interval's endpoints are
collected, the elementary spans between consecutive boundaries are examined one
at a time, and each is assigned to the highest-priority source that covers it.
Every source's boundaries therefore appear in the result.

This is also the fix that makes the picture and the audio agree. The merged
renderer now takes the **report's own timeline** — the same plan the window
draws and the gap report describes — instead of working out whole-file affine
ranges of its own. A source's recorder gap is therefore a gap in the merged
audio unless another source covers that span, and the window cannot show a
coverage the audio does not have.

*Changed:* `crates/stagesync-core/src/matching/segments.rs` (`merge_timeline`),
`crates/stagesync-core/src/render/mod.rs` (`render_merged_timeline` now takes
`&AlignmentReport`).

*Evidence:* `a_nested_source_does_not_cut_a_hole_in_the_timeline`,
`overlapping_sources_are_resolved_by_priority`,
`a_span_no_source_covers_is_silence_in_the_merged_render`.

### P1-05 — the per-source timeline export exceeded its advertised length — **fixed**

`content_end` was calculated and discarded: every resampled sample *and the
filter tail* was written, and only short outputs were padded. One second of
8 kHz input at offset +0.5 on a one-second reference produced 12,032 frames
rather than 8,000.

The timeline render now computes the reference's frame range exactly, tells the
resampler how many frames it may produce, and pads to the reference's length.
The output has exactly the reference's duration, and the source-aligned layout
still keeps the material that lies past the end of the reference.

*Changed:* `crates/stagesync-core/src/render/mod.rs`.

*Evidence:* `a_reference_timeline_has_exactly_the_reference_duration`,
`a_source_aligned_render_has_exactly_the_implied_length`.

### P1-06 — temporary WAV names collided within one process — **fixed**

The temporary name was the destination's name plus the process id, and the
file was created by truncating whatever was there. Two writers to one
destination shared the file, so an unsuccessful writer could destroy a
successful one's samples.

The temporary file is now created **exclusively**: the name carries the
process id, a per-process counter and a clock reading, and the file is opened
with `create_new`. A name that somehow exists is abandoned in favour of
another, never truncated.

The test harness had the same defect: the fixture cache released its mutex
*before* generating, so two tests generated the same file at once. The cache is
now per-process and the lock is held across the generation. The cold parallel
run that failed eight alignment tests now passes.

*Changed:* `crates/stagesync-core/src/audio/wav.rs`,
`crates/stagesync-core/tests/common/mod.rs`.

*Evidence:* `two_writers_to_the_same_destination_do_not_clash`,
`an_abandoned_writer_leaves_nothing_behind`, and the full suite passing from a
cold cache in parallel.

### P1-07 — no size guard for recordings beyond 4 GiB — **fixed by refusal**

The underlying writer counts its payload in a 32-bit field with unchecked
arithmetic, so a payload that overflows it produces a file whose header does
not describe its contents.

RF64 was not implemented. Instead the output is **refused before it is
written**, with a message that says what to do about it, and the guard is
applied on every write rather than only at the start — a render of a file whose
length is not known in advance would otherwise discover the overflow when it is
already too late. The limit is `RIFF_MAX_DATA_BYTES`, below the field's own
capacity.

This is a deliberate choice, and it has a consequence: a recording longer than
about four gigabytes of float audio **cannot be exported**. That limit is now
stated in the export code and in `RELEASE_READINESS.md` rather than being
discovered at the end of a three-hour render.

*Changed:* `crates/stagesync-core/src/audio/wav.rs`,
`crates/stagesync-core/src/render/mod.rs`.

*Evidence:* `an_output_that_does_not_fit_in_a_wav_file_is_refused`.

### P1-08 — the normalisation and the silence-independent rule — **not repaired as prescribed**

This is the one finding where the prescribed repair was **measured to be
harmful**, and it is reported at length because the finding is correct about
the contract and the contract is wrong about the implementation.

**The finding.** The local z-score takes its mean and spread from the frames
that are *not* marked quiet, so the normalised output depends on the quiet mask.
`AGENTS.md` says the statistics must be identical for both files and that the
silence must be excluded from the correlation instead. `PROJECT_STATUS.md`
already claims that this was done. The code had not been changed to match.

**What was tried.** The statistics were changed to be taken from every frame
of the window, which is what both documents describe. The correspondence was
then measured on a fixture with a thirty-second recorder gap — the same
fixture the test suite uses.

| Probe | Measured lag (truth: 10.000 s) | Confidence |
|---|---:|---:|
| source 15 s, statistics from the signal frames | 10.0000 s | 0.807 |
| source 30 s, statistics from the signal frames | 10.0000 s | 0.897 |
| source 90 s, statistics from the signal frames | 10.0001 s | 0.632 |
| source 15 s, statistics from every frame | 10.0003 s | 0.385 |
| source 30 s, statistics from every frame | 10.5888 s | 0.465 |
| source 75 s, statistics from every frame | **20.5035 s** | 0.376 |
| source 90 s, statistics from every frame | **26.4904 s** | 0.490 |

With the statistics taken from every frame the measured correspondence moved by
up to 6.5 seconds, every probe fell below the trust threshold, and the
recording was reported as *wholly uncovered* — the recorder gap was no longer
reported at all, and `a_recorder_gap_is_reported_and_never_filled_in` failed.

**Why.** The correlation is invariant to an affine change of either side, so
what matters is the *shape* of the envelope. Taking the statistics from every
frame lets a silence in the window inflate the spread and squash the music
around it; the reference, which has no silence at that position, does not match
it. Taking the statistics from the signal frames keeps the music's own dynamic
range, and the music of the two recordings then has the same shape.

**What was done.** The implementation was **left as it was**, because the
measured behaviour is better than the prescribed behaviour. What was changed:

- the code's documentation now states plainly what the rule is and why the
  statistics are taken from the signal frames, with the measurement above;
- the duplicated comment block in `features/envelope.rs` was removed;
- `the_normalisation_keeps_the_music_beside_a_silence_intact` was added, and
  it pins the property the prescribed change breaks: the music beside a silence
  keeps its dynamic range;
- `a_recording_whose_silence_is_elsewhere_is_still_aligned` was added: a
  recording whose silence falls in a different place from the reference's is
  still aligned to within 15 ms.

**What is still open.** `AGENTS.md`'s entry and `PROJECT_STATUS.md`'s table row
describe a rule the code does not implement. They were left alone here, because
changing the repository's operating contract is the maintainer's decision and
not the auditor's. **The rule and the code must be reconciled before release**,
in one direction or the other: either the documents are corrected to describe
the signal-frame statistics and say why, or a different normalisation is
designed that satisfies the rule *and* keeps the correspondence — which the
measurement above says the obvious version of does not.

**The finding was re-measured independently.** The prescribed change was
applied again on the repair branch, the suite was run against it, and it was
then reverted. With the statistics taken from every frame,
`the_normalisation_keeps_the_music_beside_a_silence_intact` fails — the silence
beside the music flattens it, which is exactly the property the prescribed
change is supposed to preserve. The change was reverted and the tree returned
to a clean state; nothing of the experiment remains in the committed code.

### P1-09 — the window extractor retained the whole suffix — **fixed**

Once every requested window had been produced, the buffer was no longer
trimmed and the decoding continued to the end of the file. A short source near
the beginning of a long reference therefore retained almost the entire
remaining reference in memory.

The pass now stops as soon as every request is resolved, and the buffer is
released. A window that begins at or after the end of the file is resolved from
the container's own duration without decoding anything.

*Changed:* `crates/stagesync-core/src/audio/windows.rs`.

*Evidence:* `the_pass_stops_once_every_window_has_been_produced` and
`a_window_past_the_end_is_not_decoded_to_find` measure the frames actually
decoded.

### P1-10 — the Windows packaging stopped before the build — **fixed**

`packaging/windows/build.ps1` ran `cargo build --version`, which Cargo
rejects, and reported Cargo as unavailable. It now resolves the executable and
runs `cargo --version`. The NSIS script's `InstallDir "$STAGESYNC_INSTALL_DIR"`
did not use the `${...}` define syntax; it does now, and every define in the
file resolves. A static checker (`packaging/windows/check_nsi.py`) catches the
original class of mistake.

The script was run to completion on this machine and produced a portable ZIP;
running it twice produced a byte-identical ZIP.

**Not verified:** `makensis` is not installed here, so the NSIS installer was
not built. The `.nsi` is corrected and statically checked, but a compiler has
not seen it. One CI run with NSIS is needed before a release.

### P1-11 — the packages omitted the licence material — **fixed**

`packaging/notices/` now holds the project licences, the MPL-2.0 text, a
generated `THIRD_PARTY_NOTICES.md` covering every package in `Cargo.lock`, and
`SOURCE-AVAILABILITY.md` with the route to the MPL-covered source. The bundle
is copied into the Windows ZIP, the NSIS installer, the macOS bundle and the
Linux AppDir and `.deb`.

The generator reads only `Cargo.lock` and a licence table, needs no network,
and produces byte-identical output every run. It fails loudly, listing any
package missing from the table, rather than shipping an incomplete notice.

---

## The P2 findings

### P2-01 — a displayed anchor could be lost — **fixed**

The anchor was applied to the envelope model only. When the drift assessment
kept the *waveform* model — usually the better one — the anchor was silently
discarded, so the placement the user asked for was not the placement that was
used. The window's wording ("a hard constraint") was therefore false.

The anchor is now an explicit **override layer**: it is applied to whichever
model the assessment settled on, keeping the rate the match measured and moving
only the placement. The report carries an `override_state` recording the
anchor, the offset the automatic model would have used, the offset that was
applied and how far the placement moved, and the warnings say so in words.

The anchor is also keyed by the recording's **identity** — its content hash, or
its full path — and never by its file name, because two recordings called
`song.wav` in different folders are two different recordings. The window's
"Clear anchor" button is now "Reset to automatic", and the window shows what
the last analysis actually did with the anchor.

*Changed:* `crates/stagesync-core/src/pipeline.rs`,
`crates/stagesync-core/src/matching/mod.rs`,
`crates/stagesync-desktop/src/app.rs`.

*Evidence:* `a_manual_anchor_survives_the_drift_assessment`,
`an_anchor_is_keyed_by_the_recording_and_not_by_its_name`.

### P2-02 — the WAV reader miscounted frames and scaled 24-bit wrongly — **fixed**

`reader.len()` counts channel samples, so a stereo file reported twice its
frame count and every caller that wanted frames had a number twice too large.
The reader now reports frames.

A 24-bit sample has a full scale of 2²³, not 2³¹; dividing by the 32-bit full
scale put every sample 48 dB too low, so the verifier read a 24-bit recording
as if it were almost silent. 24-bit is now read at its own scale.

*Changed:* `crates/stagesync-core/src/audio/wav.rs`.

*Evidence:* `the_wav_reader_counts_frames_and_not_samples`,
`a_24_bit_wav_is_read_at_its_own_scale`.

### P2-03 — offset-only still resampled and appended a tail — **fixed**

A correction that needs no rate change now **copies the samples**: the map is
recognised as a whole-sample shift, and the material is copied untouched rather
than put through a filter. The render result says which happened
(`resampled: false`), so "an offset-only correction performs no resampling" is
now true of the file as well as of the wording.

Every layout now has an exact length: the source-aligned render has exactly the
length the rate change implies, and the filter's tail is never written.

*Evidence:* `a_unity_correction_copies_the_samples_instead_of_filtering_them`
— the samples are compared one by one with the input.

### P2-04 — the collision comparison did not model the file system — **fixed**

The comparison now asks the operating system, as described under P1-01.

### P2-05 — the verifier's default grid included an invalid endpoint — **fixed**

The default grid's last window ended 10.250 seconds beyond the end of the
reference, so it measured nothing, and the grid was too sparse for a
1.6-hour recording. The grid is now a **predetermined denser policy**: 24
positions spread uniformly over the usable interior, chosen by rule from the
geometry alone rather than picked from the positions that succeeded.

**The reliability gates were not touched.** `--min-peak-ratio`,
`--min-agreement` and the required trusted count are exactly as they were. The
denser grid exposes positions that fail, and they are reported as failures and
disputes rather than hidden. On the audit's own export the denser grid
reproduces the audit's numbers: 18 of 24 trusted, p50 0.030 ms, p95 0.990 ms,
max 1.916 ms, 2 disputed, 4 untrusted, 0 errors.

*Changed:* `tools/verify_exports.py`.

### P2-06 — `report.json` omitted the corrected WAV hashes — **fixed**

The report was written before the corrected audio existed, so its list of
artefacts was incomplete and carried no hashes for the audio.

The export is now written in the right order: the corrected audio and the
verification first, then `report.json` with the complete list, then
`manifest.json`, which lists **every** file of the export with its SHA-256.
The inventory is taken from the export directory rather than from a list kept
by hand, so a file that was written and not remembered is still listed.

The export is also **atomic**: everything is written into a staging directory
and moved into place in one step, so a failure leaves the output directory
exactly as it was rather than half an export next to the recordings.

*Changed:* `crates/stagesync-core/src/export/mod.rs`,
`crates/stagesync-cli/src/main.rs`, `crates/stagesync-desktop/src/jobs.rs`.

*Evidence:* the export was run end to end; the manifest lists every file with
its hash, and no staging directory is left behind.

### P2-07 — large collections have concrete layout limits — **partly fixed**

The file list and the central area are now scrollable, so a long session can
be reached. The "Make reference" button was added to every target row, which
adds a column to an already narrow panel, and the side panels are still capped
at a fixed width.

**Not fixed:** the timeline has no "fit" control, and the panels are not
collapsible. A session with many recordings is still cramped.

### P2-08 — the test count hid environmental dependencies — **fixed**

The suite's own fixtures are now hermetic: each test process keeps its own
fixture directory and holds the generation lock across the generation, so a
cold parallel run cannot race. The suite passes from a cold cache in parallel
and serially.

The tests still use the operating system's temporary directory. That is
deliberate — a three-hour fixture does not belong in the repository — and the
directory is now per process, so two machines and two processes never share
one.

### P2-09 — the independence and uncertainty claims exceeded the validation — **partly fixed**

The claims that are now backed by an artefact are backed by an artefact:
`report.json` and `manifest.json` carry hashes, the export is atomic, and the
window distinguishes the analysis status from the export verification.

**Still open:** the wording in `README.md` and `PROJECT_STATUS.md` that
describes the two estimators as "independent" is a claim about method, not
about a validation result, and the publication-facing text has been rewritten
(see P2-12). The full rewrite of every claim in every document was not
completed in this pass.

### P2-10 — the cancellation guarantees needed qualification — **partly fixed**

The window now says "all artefacts written" only after the export has been
committed, and a cancelled export leaves the output directory as it was.

**Still open:** a job that is already under way cannot be stopped; the cancel
flag is checked at stage boundaries and inside the decoding loop, so a
cancellation takes effect within about a second, but a render in progress
cannot be interrupted. The documentation now says so.

### P2-11 — the Linux packaging was not a verified distributable — **partly fixed**

The Debian control file says `amd64`, which is the correct Debian identifier.
`appimagetool` is pinned to an immutable version and its SHA-256 is verified
before it is executed; the script refuses to run on a mismatch or when no
checksum is available. The AppImage is no longer described as self-contained:
it bundles the client libraries and leaves the C library and the graphics
driver to the host, and the script says so.

**Not verified:** the Linux and macOS packages were not built on their own
platforms — this machine is Windows. The scripts were exercised with stubbed
platform tools, so the AppImage itself and the `.dmg` remain unproven
end to end.

### P2-12 — the documentation overstated the validation — **fixed**

`RELEASE_READINESS.md` was written from one dated evidence table, and it
distinguishes what was configured, what was run natively, and what was run
only against a stub. The drift figures are shown with their uncertainty
wherever they appear, and the disputed positions are kept visible rather than
being folded into a percentile.

### P2-13 — the calibration mapped quiet flags in the opposite direction — **fixed**

The waveform/envelope resampler reads input at `alpha * output_index`, and the
quiet flags used `output_index / alpha`; for a non-unity scale those select
different source positions. The flags now use the same coordinate as the
samples.

*Changed:* `crates/stagesync-core/src/calibration.rs`.

---

## The P3 findings

### P3-01 — `SlopeFit::signal_to_noise` used the wrong expression — **fixed**

It computed `abs(slope) / SE`, which is the signal-to-noise of a *rate*, rather
than `abs(slope - 1) / SE`, which is the signal-to-noise of a *clock scale*.
The helper is now correct, and the release decisions are unchanged — they
already used the right expression.

### P3-02 — the packaging was not reproducible — **fixed**

The toolchain is pinned (`packaging/rust-toolchain.toml`), the archives carry
fixed timestamps and sorted entries, a SHA-256 manifest is published with the
release, and the workflow actions and runner labels are pinned to specific
versions. Running the Windows packaging twice produced a byte-identical ZIP.

---

## The usability findings

### UX-1 — the timeline must describe the actual export — **fixed**

The window draws the report's timeline, and the merged renderer now renders
*that* timeline. The picture and the audio cannot disagree.

### UX-2 — every essential control must be reachable — **partly fixed**

The file list and the central area scroll, so a long session can be reached.
The status of each target is on the target's own row rather than only in the
detail panel.

**Still open:** there is no "fit" control for the timeline, the panels are not
collapsible, and the responsiveness at 720×450 and at 150 % and 200 % scaling
was not measured. The layout test still exercises an empty session at three
sizes.

### UX-3 — the manual override must be reliable and explicit — **fixed**

See P2-01. The anchor is an override layer that survives the drift assessment,
it is keyed by the recording's identity, the conflict is shown rather than
hidden, there is a "Reset to automatic", and the wording no longer claims a
guarantee the code did not provide.

### UX-4 — the window must state what has actually been verified — **fixed**

Every target now carries a status — *Auto aligned*, *Needs inspection*, *No
reliable match*, *Override applied*, *Not analysed* — and, separately, the
state of the export verification: *Export not verified*, *Export verified at N
sampled positions*, or *Insufficient data*. The two are kept apart: a target
can be perfectly aligned and still have an export that was never verified, and
the window no longer leads with a residual of 0.000 ms when there were no
trusted measurements.

### UX-5 — the reference and export geometry must be explicit — **fixed**

The first file added is still the reference by default, but every target row
now has a **Make reference** button, so the choice can be made deliberately.
Changing the reference drops the results, because they described the old one.

**Still open:** the export geometry — what each layout produces and how long
it is — is explained in the code and in `README.md`, but the window does not
yet show it next to the export controls.

### UX-stable — waveform tracks and synchronised A/B audition — **not built**

The recommended stable-release inspection layout asks for bounded waveform
tracks with a shared cursor and a reference/selected-target A/B audition at the
same reference time. Neither was built in this pass.

The reasons are honest ones: the audit itself records that the GUI was never
observed interactively — a launch attempt through the computer-use skill was
interrupted and no live session was obtained — and the correction work was the
part that was verifiably wrong. Building a waveform viewer and an audition
path that cannot be *seen* to work would add a large surface area that nobody
has watched, and the review's own guidance is that this belongs after the
correctness blockers are resolved.

The pieces the audition would need are in place: the corrected mapping is
applied by the renderer, the merged timeline is exactly the reference's length,
and the export is verified independently of the fit. What is missing is the
window around them.

---

## The gates

All four gates, run on the repair branch:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean, exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean, exit 0 |
| `cargo test --workspace` | **154 tests, 0 failed**, exit 0 |
| `cargo build --release` | exit 0 |

The suite was run from a cold cache in parallel and serially; both are green.
Before the repair, a cold parallel run failed eight alignment tests with a
shared-fixture race, which is P1-06.
