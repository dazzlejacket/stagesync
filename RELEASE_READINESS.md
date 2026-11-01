# Release readiness

This is what has actually been *run*, what has only been *statically checked*,
and what has not been run at all. It is written to be read by someone deciding
whether to publish, so it keeps those three apart and does not round a partial
result up into a complete one.

- Audit: `CODEX_AUDIT.md`, 2026-10-29, against release `0a128df`.
- Usability: `UX_REVIEW.md`, 2026-10-29, against the same release.
- Repairs: `POST_CODEX_FIXES.md` — what was done about each finding.
- Evidence table: the last section of this file.

The build machine for every native result below is **Windows 11 x64
(10.0.26300)** with **Rust 1.99.1** — the version pinned in
`packaging/rust-toolchain.toml`.

---

## 1. What is verified

### 1.1 The four gates

Run from the repository root, in this order, on a cold fixture cache:

| Gate | Command | Result |
|---|---|---|
| Formatting | `cargo fmt --all -- --check` | clean, exit 0 |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | clean, exit 0 |
| Tests | `cargo test --workspace` | **172 passed, 0 failed**, exit 0 |
| Release build | `cargo build --release --workspace` | exit 0, no warnings |

The test run is genuinely cold: each test process keeps its own fixture
directory under the operating system's temporary directory and holds the
generation lock *across* the generation, so two processes cannot build the same
fixture at the same time. The run was made in parallel and serially; both are
green.

`cargo fmt` and `cargo clippy` are both run with `-D warnings` — no lint rule
was weakened and no test was removed to make a run green.

The release workflow now repeats both of the first two gates itself, because a
release can be cut from a tag that never went through CI, and it installs
`ffmpeg` explicitly — the AAC test now fails loudly without it.

### 1.2 The real recordings

The two concert recordings in `audio/` were aligned against `combined.wav`
through the repaired pipeline and exported. The export was then verified by
`tools/verify_exports.py`, which is an **independent** measurement: it does not
read the model the pipeline fitted, it measures the corrected file against the
reference and reports the residual.

For `Voice_006_W_20250208_164548_corrected.wav` against `audio/combined.wav`:

| Figure | Value |
|---|---|
| Model scale | `1.000016500036` |
| Model offset | `5753.313995859` s |
| Drift implied by the model | `16.500` ppm |
| Trusted positions | **18 of 24** |
| \|residual\| over trusted positions | p50 **0.030 ms**, p95 **0.990 ms**, max **1.916 ms** |
| Envelope estimator, for comparison | p50 −0.464 ms, p95 3.425 ms, max 7.836 ms |
| Disputed positions | 2 (methods disagreed by more than 10 ms) |
| Confidence | high |

The two disputed positions are **excluded** from the headline statistics rather
than folded into a percentile, and they are listed by name in the verifier's own
report.

The pitch figures are reported the way the audit asked for them: the model
implies **−0.0286 cents**, and the autocorrelation tracker measured a mean
absolute difference of 15.76 cents over the four positions where it could
resolve a pitch at all. The tracker cannot resolve a change of a few
hundredths of a cent, so it **bounds** the pitch change and is not a measurement
of it. Two positions disagreed by more than 100 cents; those are tracking
artefacts on polyphonic material and are excluded from the figure above.

The verifier's trust thresholds were **not** touched by the repair, and they
are the difference between a measurement and a guess:

| Gate | Value |
|---|---|
| `--min-peak-ratio` | `1.10` |
| `--min-agreement` | `0.60` |
| required trusted positions | `5` |

These are byte-identical to the values in the audited release.

### 1.3 The default measurement grid

The verifier's default grid is now **predetermined**: the positions are derived
from the geometry of the two files alone — a uniform grid over the interior the
two files share, with a floor of twelve positions — and are chosen *before* any
position is measured. A position is never selected because it happened to
measure well.

The denser grid exposes positions that fail. On the export above, four of the
twenty-four positions are untrusted and two are disputed; they are reported as
failures, not hidden.

---

## 2. What is checked but not run end to end

### 2.1 The Windows installer

`packaging/windows/installer.nsi` is corrected — the `InstallDir` line used the
bare `$STAGESYNC_INSTALL_DIR` form, which asks for a runtime *variable* instead
of the compile-time define, so the install directory silently came out empty —
and it is checked by `packaging/windows/check_nsi.py`, a static checker written
for exactly that class of mistake.

**Not verified:** `makensis` is **not installed on this machine**, so the NSIS
installer has never been compiled. The script is correct by inspection and by
static check, but no compiler has seen it. **One CI run with NSIS is required
before a release.**

The portable ZIP *was* built, by running `packaging/windows/build.ps1` to
completion on this machine. Running it twice produced a byte-identical archive,
which is the reproducibility claim (P3-02) actually demonstrated.

### 2.2 The Linux and macOS packages

The Debian control file carries `amd64`, which is the correct Debian
architecture identifier. `appimagetool` is pinned to an immutable version and
its SHA-256 is verified **before** it is executed; the script refuses to run on
a mismatch, and refuses to run at all when no checksum is available. The
AppImage is no longer described as self-contained — it bundles the client
libraries and leaves the C library and the graphics driver to the host.

**Not verified:** these packages were **not built on their own platforms**. This
machine is Windows. The scripts were exercised with stubbed platform tools, so
the `.deb`, the AppImage and the `.dmg` remain unproven end to end. Each needs
one build on its own platform.

---

## 3. What is not done

### 3.1 The waveform tracks and the A/B audition (UX-stable)

The recommended stable-release inspection layout — bounded waveform tracks with
a shared cursor, and a reference/selected-target A/B audition at the same
reference time — **was not built**.

The reasons are honest ones. The audit records that the window was never
observed interactively, and the correction work was the part that was
verifiably wrong. Building a waveform viewer and an audition path that nobody
has watched work would add a large surface area in exchange for nothing. The
review's own guidance is that this belongs after the correctness blockers are
resolved.

The pieces the audition would need are in place: the corrected mapping is
applied by the renderer, the merged timeline is exactly the reference's length,
and the export is verified independently of the fit. What is missing is the
window around them.

### 3.2 The normalisation rule (P1-08) — **closed**

This was the one audit finding whose prescribed repair was **measured to be
harmful**, and it was the one open item that blocked a release. It is now
closed.

`AGENTS.md` requires the envelope's local statistics to be *identical for both
files*, with the silent frames excluded from the correlation instead. The code
takes the statistics from the **signal** frames, and the silent frames *are*
excluded from the correlation.

The prescribed change was implemented and measured on the fixture with a
thirty-second recorder gap. With the statistics taken from every frame the
measured correspondence moved by up to **6.5 seconds**, every probe fell below
the trust threshold, the recording was reported as *wholly uncovered*, and the
recorder gap stopped being reported at all.

The reason is that the correlation is invariant to an affine change of either
side, so what matters is the *shape* of the music. Taking the statistics from
the signal frames keeps the music's own dynamic range; taking them from every
frame lets a silence in the window inflate the spread and squash the music
around it, and the reference — which has no silence at that position — no longer
matches it.

The implementation was therefore **left as it was**, and the properties it
depends on are now pinned by tests
(`the_normalisation_keeps_the_music_beside_a_silence_intact`,
`a_recording_whose_silence_is_elsewhere_is_still_aligned`).

The finding was **re-measured independently** on the repair branch: the
prescribed change was applied again, the suite was run against it, and it was
reverted. With the statistics taken from every frame,
`the_normalisation_keeps_the_music_beside_a_silence_intact` fails — the
silence beside the music flattens it, which is the very property the
prescribed change is supposed to preserve. The experiment was reverted and
nothing of it remains in the committed code.

**The contract and the code now agree.** `AGENTS.md` states the property that
is actually wanted — *the normalised shape of the music must not change when
silence is added or removed nearby* — and says that the local statistics are
taken from the signal frames only *because* that makes the shape
silence-invariant. The contract no longer describes an implementation that does
not exist, so the item no longer blocks a release.

### 3.2a The second audit's findings (P1–P5) — **closed, with four exceptions**

`CODEX_REPAIR_VERIFICATION.md` was a second, independent audit. Its findings
and the repairs are in `FINAL_REPAIR_HANDOFF.md`. The four exceptions — items
that were measured, understood and deliberately not changed — are:

1. **The robust fit cannot reject a gross outlier when only three or four probes
   are trusted.** A probe 11.5 s out was trusted with a confidence of 0.507 and
   the resulting offset was 3.8 s wrong. The drift was correctly refused (the
   fit's signal-to-noise was 1.9, below the factor of three), but the offset was
   not protected. A source with only a handful of trusted probes is the
   vulnerable case; the two real recordings have hundreds.
2. **The fine stage never probes the last half-window of a source**, so material
   in the final fifteen seconds of a recording is never probed.
3. **The float WAV writer emits `WAVE_FORMAT_EXTENSIBLE` with a zero channel
   mask.** Most tools read the file correctly; ffmpeg's AAC encoder refuses it.
   The writer was left alone because changing it changes the bytes of every
   export.
4. **The noise-floor coupling** — the quiet threshold is 121.5 dB lower for a
   recording that contains digital silence. It was measured, and it does not
   move the correspondence, so the rule was not changed.

None of the four affects the two real recordings: their offsets are exact to
the millisecond, their drift is measured on hundreds of probes, and their hold-
out verification agrees with the model to within a fraction of a millisecond.

### 3.3 The 4 GiB output limit (P1-07)

RF64 was not implemented. The output is **refused before it is written**, with
a message that says what to do about it, and the guard is applied on every write
rather than only at the start.

The consequence is a real limit, and it is stated here rather than discovered at
the end of a three-hour render: **a recording longer than about four gigabytes
of float audio cannot be exported.** A concert recording of that length needs
either RF64 support or a shorter export window.

### 3.4 The remaining partly-repaired findings

| Finding | What is still open |
|---|---|
| P2-07 | The file list and the central area scroll, but there is no "fit" control for the timeline and the panels are not collapsible. A session with many recordings is still cramped. |
| P2-09 | The claims that are now backed by an artefact are backed by an artefact. The wording that describes the two estimators as "independent" is a claim about *method*, not about a validation result, and a full rewrite of every claim in every document was not completed. |
| P2-10 | A cancelled export leaves the output directory as it was. A job that is already under way cannot be stopped: the flag is checked at stage boundaries and inside the decoding loop, so a cancellation takes effect within about a second, but a render in progress cannot be interrupted. |
| P2-11 | See section 2.2 — the packages are not built on their own platforms. |
| UX-2 | The layout test exercises an empty session at three sizes. The responsiveness at 720×450 and at 150 % and 200 % scaling was not measured, and the window has never been observed interactively. |
| UX-5 | The export geometry is explained in the code and in `README.md`, but the window does not yet show it next to the export controls. |

---

## 4. What has to happen before publishing

1. **Run the Windows installer build once on a machine with NSIS.** The `.nsi`
   has been corrected and statically checked, but no compiler has seen it.
2. **Build the Linux and macOS packages on their own platforms.** The AppImage,
   the `.deb` and the `.dmg` are unproven end to end.
3. **Decide P1-08 — done.** `AGENTS.md` and the code were reconciled in the
   direction the measurement supports: the contract now describes the
   signal-frame statistics and says why. See §3.2.
4. **Observe the window.** No one has watched the application run. The GUI
   changes are substantial, and the layout has only been exercised by a test
   against an empty session.
5. **Decide on UX-stable.** Either build the waveform tracks and the A/B
   audition, or record the decision to ship without them.
6. **Note the 4 GiB limit** in the release notes, or implement RF64.
7. **Decide on the four exceptions in §3.2a.** They are measured and
   understood; each is a policy question rather than a defect, and the
   maintainer should decide whether they are acceptable for this release.

---

## 5. Evidence table

Every row states what kind of evidence it is. "Native" means it was run on the
platform it applies to; "stubbed" means the script was exercised with the
platform's tools replaced by stubs, so the *script* is proved but the *artefact*
is not; "static" means the artefact was checked without being produced.

| Item | Evidence | Kind |
|---|---|---|
| Formatting | `cargo fmt --all -- --check`, exit 0 | native |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings`, exit 0 | native |
| Tests | `cargo test --workspace`, 172 passed, 0 failed, cold cache, parallel and serial | native |
| Release build | `cargo build --release --workspace`, exit 0 | native |
| The merged timeline | exported from the two watch recordings and verified independently: the file is exactly the reference's length, every uncovered span is digital silence, 72 windows at a worst lag of **0.0 ms** | native |
| The log-mel cross-check | the spectral cross-check disagrees with the envelope fit at **no** position on either real recording; the median point-by-point difference is 11.8 ms and 11.3 ms | native |
| A recorder gap | the coarse lock on the audit's own short fixtures: an offset error of 0 ms, against 87.5 s, 144 s, 10 s and 41.5 s before the repair | native |
| Real-material alignment and export | `Voice_006` against `audio/combined.wav`, exported and verified independently | native |
| Independent verification | `tools/verify_exports.py`, 18 of 24 trusted, p50 0.030 ms | native |
| Verifier trust thresholds | `--min-peak-ratio 1.10`, `--min-agreement 0.60`, `min_trusted 5` — unchanged from the audited release | static |
| Windows portable ZIP | `packaging/windows/build.ps1` run to completion; two runs byte-identical | native |
| Windows NSIS installer | `packaging/windows/check_nsi.py` passes; `makensis` not installed | static |
| Licence bundle | `packaging/notices/` generated from `Cargo.lock`; byte-identical on repeat runs; fails loudly on an unmapped package | native |
| Linux `.deb` and AppImage | `packaging/linux/build.sh` exercised with stubbed platform tools | stubbed |
| macOS `.dmg` | `packaging/macos/build.sh` exercised with stubbed platform tools | stubbed |
| The window, interactively | never observed | **none** |
| The waveform tracks and A/B audition | not built | **none** |

The evidence files themselves — the audit's logs, the real export and the
verifier's reports — are under `work/codex-audit/` and `work/release-evidence/`.
`work/` is scratch space and is not committed.
