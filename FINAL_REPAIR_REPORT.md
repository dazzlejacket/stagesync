# StageSync final beta repair report

2026-11-19. Local branch `codex/final-beta-repair`, based on audited commit
`58e526cd5a00ce474bc0339a524b2146aca5afd1`. The original workspace was its ancestor
`32b5f0e`; it was fast-forwarded before repair. No push, release, CI dispatch, or
recording upload. No new product features or dependencies.

**F1-F9 repairs are complete and all four local quality gates pass.** Native
platform/package/device limitations below remain explicitly unverified.

The audit was read from `work/final-beta-audit/CODEX_FINAL_BETA_AUDIT.md`.
Its fixtures and source archive were preserved. New execution evidence is under
`work/final-repair/`; media and scratch evidence remain ignored by Git.

## Repairs and reproduction evidence

| Finding | Baseline reproduction | Repair and regression |
|---|---|---|
| F1: Save staging destroys an input | New hermetic test failed: saving removed the loaded `.collision.stagesync.<pid>.tmp` WAV. `baseline-save.log`, Rust exit 101. | Staging uses `create_new`, retries occupied names, writes only the exclusively owned file, calls `sync_all`, then renames. Unix additionally syncs the parent directory. The original filename regression passes; direct/alias/extension guards remain. |
| F2: false short placements | Exact default seeds 13 and 29 both failed the new refusal tests; models +225.65 s and +17.65 s. `baseline-core.log`, exit 101. | Require stronger position corroboration; retain the existing 0.90 near-exact whole-file route with independent fine-position support. A single weak long-clip probe is insufficient. Weak or gapped fits receive explicitly reported supplemental placement probes over surviving material, without changing the quiet mask, fit, uncertainty, or drift holdouts. Default cases 13/29 are refused. Existing successful short/gap/hour-offset tests are retained. |
| F3: wrong envelope clock/drift | Exact case 43 reproduced -2104.880985 +/-135.456223 ppm and a render 405.866 ms short. Both default and forced-envelope tests failed. | Envelope rate is `sample_rate / rounded_hop_len`. Correlators put unequal per-file feature clocks onto a common physical-time grid; calibration and segment times retain actual rates. Rejected drift falls back to the median of positions supporting the accepted placement, rather than keeping an uncertain slope's biased intercept. Regressions include 11.025/22.05 kHz, rendered duration and independent rendered-waveform checks. |
| F4: AutoGain freezes UI | Original optimized private resolver: 41.5292832 s on the concert. `baseline-scan.log`. | Immutable report/mixer snapshot, cancellable background RMS/peak measurement, fingerprint checks before/after, session and full mix key on result acceptance. Repaints/status continue; Stop cancels. No processed audio starts until preparation succeeds. |
| F5: live edits bypass protection | Original probe: trim 0.09002995 became 1; sampled peak became 7.6920414. `baseline-live.log`. | A cache miss pauses audition and schedules a new measurement; unknown/error trim never becomes unity. Live-edit regression checks silence/pause while pending, final samples against separately rendered export output, protected peak <=1, and changed-media error handling. Monitor-specific peak measurement uses the actual monitored tracks. |
| F6: Linux checksum self-move | The same regression against the untouched audited build script exits 1 on GNU `mv` naming the same file twice. `linux-regression-baseline.log`. Initial sandboxed MSYS launch failed before running commands; an approved external-sandbox run reproduced the actual error. | Removed redundant move. `packaging/linux/test_checksums.sh` executes the production checksum section, checks all three artifacts and the tar-only path with `sha256sum -c`; exit 0 (`linux-delivery.log`). Added to native Linux CI. Bash syntax check exits 0. Native Linux packaging is not claimed. |
| F7: synchronous source preparation | Original public-player construction: combined.wav 1.3090162 s. `baseline-longplay.log`. | Hash/open/seek/source construction runs on the audio actor. Stop/switch cancels queued/in-flight preparation; hashing and decode loops observe cancellation. Full hashes are retained, and app auditions check expected analyzed content before playback. The two-second acknowledgement diagnostic now reports pending preparation/output instead of asserting a device disconnect while hashing may still be running. A deliberately blocked preparation regression proves prompt submission/Stop and no stale playback/error delivery. |
| F8: fractional onset differs from export | New complete-prefix comparison failed at onset: 0.8 versus 0.9087543. `baseline-fractional.log`, exit 101. | Preserve fractional phase after the rounded silent delay, read filter pre-roll on seeks, and limit output to the covered endpoint. All 24 mono/stereo offset/seek cases compare complete prefixes with timeline exports at <1e-5 sample difference. |
| F9: ambiguous capture query | Unchanged screenshot-enabled test failed on multiple `0:00:` labels. `baseline-capture.log`, exit 101. | Unique visible `Playhead` transport label/query. Screenshot-enabled test passes with compact rectangle and overflow assertions intact. Fresh dark/light/inspector GPU images were inspected. |

No existing test was deleted or loosened. NCC, peak-separation, waveform-correlation,
3-sigma drift, and Python verifier trust thresholds were not reduced. No verifier
source was changed. Temporary test experiments and unsuccessful runs are retained
in the evidence directory; they are not represented as passes.

## Final required quality gates

| Exact command | Real exit | Seconds |
|---|---:|---:|
| `cargo fmt --check` | 0 | 0.7994743 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 3.1384130 |
| `cargo test --workspace` | 0 | 362.2399582 |
| `cargo build --release` | 0 | 40.2465962 |

**296 passed, zero failed, zero ignored.** Core: 194; desktop: 102.
Final release CLI SHA-256 matches the executable used for the final concert
export (`delivery-cli-identity.json`); the last diagnostic wording change affects
only the desktop.

Toolchain: Cargo/Rust 1.99.1. Commands run at the workspace root, with ordinary
parallel tests. Authoritative logs: `delivery-gate-{fmt,clippy,tests,build}.log`
and `delivery-gates.json`.

Earlier unsuccessful gate attempts remain in `gates.json`, `final-gates.json`,
`verified-gates.json`, `release-gates.json`, and `completion-gates.json`: actual regression failures during
repair; one Clippy identical-branches error; Windows linker/file locks caused by
an older test or export still using an executable. A scratch verification
driver initially compared the second export with the first export's manifest;
its failure is superseded by matching-directory manifest checks, all eight
entries verified. The final all-suite run before the last diagnostic wording
change also passed all 296 tests (`final-verification-gates.json`). Those attempts were rerun;
no lint rule or test assertion was suppressed. The final export uses a copied
CLI executable so it cannot lock the build's executable.

## Alignment counterexamples and broad replay

The exact preserved default WAVs were also read directly by the fresh core, not
just regenerated for tests. Cases 13/29 are refused; case 43 applies
+150.919063 +/-3.374624 ppm, offset +11.333442494 s, against +150 ppm truth.
The rendered file has 1,984,800 frames at 11,025 Hz: 180.027210884 s, only
0.210884 ms longer than construction truth. Preserved-media hashes match before and
after (`preserved-final-hashes.json`).

Forced envelope fallback measures +155.110144 +/-9.349153 ppm at 11.025 kHz and
+157.229710 +/-22.087783 ppm at 22.05 kHz. Rendered-waveform comparisons use the
known reference samples and fixture truth, not the fit's own residuals; sampled
lags are <=1.25 ms. Duration error must remain <20 ms in the hermetic tests.

The final 1,024-case replay (`torture-pass7.log`, `torture-pass7/cases.jsonl`)
completed with **234 applied, 790 refused, zero wrong applications, zero unrelated
acceptances**, exit 0, 131.2967181 s. As in the audit, only 32 bulk cases use all
defaults; 992 disable spectral/waveform checks. Default cases 13/29/43 have their
own tests and preserved-media runs. Related applications must have <=100 ms
endpoint error; independent performances/noise/repeated-only material must be
refused. Unlike the audit collector, the repair replay asserts zero failures.
Supplemental corroboration requires sufficient overlap, independent coarse-window support (except when recorder silence removed windows), three trustworthy positions and agreement across the measured positions. It cannot create a global lock out of dense probes around one clipped edge coincidence. Earlier replay failures led to retained regressions 506, 956, 977, 241 and 1003.

The exact older 95-second/15-second-hole recording measures offset
+9.999991312 s (error -0.008688 ms); the retained 30-second-hole recording measures
+9.999927138 s (error -0.072862 ms). The original noise-floor, gap-placement,
robust-fit and median-offset regressions remain enforced.

## Playback responsiveness and processing

An optimized append-only copy of the current app's actual resolver, using the
concert report and a mock output backend, records:

- Enqueue: **0.0013185 s**, no unprocessed members, trim 0 while pending.
- Stop/cancel: **0.0000042 s**, cancellation flag set.
- Background completion: **38.7810166 s**, valid trim 1, no error.

`responsiveness.log` contains the assertions and evidence. This measures UI-call
latency and background preparation, not physical speaker onset. A separate public
player probe submits combined.wav in 19.3 microseconds and the M4As in 1.0/0.5
microseconds (`longplay-final.log`); new submissions cancel old preparation.
The device's actual consumed frames still drive the playhead.

The static/loudness/protection sample-equivalence regression now waits for the
asynchronous result and retains its sample tolerances. It additionally exercises
a hot live fader/master change, paused preparation, exact protected output, and
changed-media failure. Session/Stop tests reject stale prepared results.

## Real concert exports and originals

The final frozen-source release CLI exported both sources against the reference with
no historical plan reused: **exit 0, 194.9703605 s**. `real-export-verified-run.json` records the real exit
and elapsed time; `verified-cli-hash.json` identifies the copied executable. The
copied executable prevents Windows from locking the build target during export.

| Source | Offset seconds | Drift +/-SE ppm | Python trusted | Absolute residual p50 / p95 / max ms |
|---|---:|---:|---:|---|
| Voice 005 | +317.389336406 | +16.060816 +/-0.136747 | 23/30 | 0.000 / 0.074 / 1.901 |
| Voice 006 | +5753.314004933 | +16.385581 +/-0.153791 | 16/24 | 0.061 / 0.390 / 1.183 |

The unchanged `tools/verify_exports.py`, default sampling/trust gates, `--no-pitch`,
reports **status success** for both, exit 0. Voice 005 retains 1 disputed / 6
untrusted positions; Voice 006 retains 3 disputed / 5 untrusted. These positions
are excluded from headlines, not hidden. Evidence: `verify-verified-{0,1}.{json,md}`,
`verifier-verified-exits.json`. All eight manifest entries are independently
rehashed and byte counts checked (`manifest-verified-check.json`).

Final corrected WAVs are byte-identical to the first fresh exports, and the final
plan's timeline and drift assessments are exactly identical. This also checks
that the final sparse-evidence repairs did not change the established concert
models. `final-real-check.json` records these assertions.

The independent merged render is 464,902,080 mono float32 frames at 48 kHz:
exactly 9,685.46 s, the unchanged reference duration. The unchanged audit checking
algorithm sampled **527 positions**, minimum NCC **0.999551388**, maximum lag
**0.000 ms** at its 8 kHz check resolution. Three uncovered spans: **45,639,062
interior samples checked, peak exactly zero**. This is sampled placement and gap
interior verification, not a claim about every covered sample or boundary. The final plan and corrected WAVs match those used for this merged
check exactly. Evidence: `realbench.log`, `merged-check.log`, `merged-verification.json`.

Original SHA-256 values before/after export:

| Original | SHA-256 |
|---|---|
| combined.wav | `048b29f3b02f3b61d20e61bdb0b4e24fccf7d4b209609b45212340939a836026` |
| Voice 005 | `ed9a8e1775e3814ec25e29314f17b82445bcb00dd9bb9e4d0fb7386ebb9d3c0b` |
| Voice 006 | `dc57eaf59b9452dcc1c94427818ac698a2593ca6421f62d47417c93921a209d9` |

`originals-before.json` and `originals-after.json` compare identically; the final
frozen-source export is followed by `originals-verified-after.json` and the same
three hash assertions. Originals
were read only; all outputs are in a separate ignored scratch directory.

## Remaining limitations

- Linux checksum generation is exercised under Git Bash/GNU tools on Windows.
  WSL is not installed. Native Linux builds, AppImage/deb installation, ALSA,
  X11/Wayland, and hosted CI execution remain unverified. No macOS package or
  NSIS installer was built/installed during this repair.
- No new sustained native listening, physical hot-unplug, screen-reader, or
  crash/power-loss recovery journey is claimed. GPU/widget tests and mock/device
  regression coverage are not equivalent to every driver or desktop environment.
- Existing export cancellation remains stage-bounded; this repair does not claim
  that an in-progress whole-file export can be interrupted inside every renderer.
- Protected preview still needs a whole-timeline background measurement, which can
  take tens of seconds. Live edits pause and recompute; they do not audition an
  unmeasured protected mix. OS-level stalled reads can delay a worker even though
  the UI and Stop remain nonblocking.
- Automatic matching is deliberately conservative. Refusals include some related
  short/degraded clips; users may need an explicit manual anchor. The finite
  replay is not a proof against every possible false correspondence.
- File/directory sync improves Save durability; physical power-loss behavior was
  not tested. Concurrent hostile media replacement during/after hashing, network
  filesystem semantics, and exhaustive filesystem aliases remain outside this
  verification. Exclusive staging prevents pre-existing siblings from truncation.
- Some synthetic spectral cross-check positions still disagree. Their warnings
  remain reported; estimates are not averaged or forced to agree.
- Pitch tracking on polyphonic concert material cannot resolve the model's few
  hundredths of a cent; no independently measured pitch-accuracy claim is made.
