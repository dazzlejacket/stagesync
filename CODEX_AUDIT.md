# StageSync independent release audit

Audit date: 2026-10-29. Audited HEAD: `0a128df` (16 reachable commits). Windows x64; Rust 1.99.1; Python 3.14.7, NumPy 2.5.4, SciPy 1.18.1.

## Release decision

**C — hold the present binaries privately. Do not ship stable 1.0 or the current beta binaries.** There are reproducible failures affecting input preservation, stereo content, playback rate, negative-offset rendering, and coverage reporting. These are more consequential than interface polish or unexecuted macOS CI.

**B — a clearly labeled Windows beta is a reasonable next milestone after the safety/correctness blockers below are fixed and independently retested.** A waveform redesign need not delay that beta. Stable 1.0 should include usable inspection, reliable overrides, representative native platform testing, and broader rendered-output validation.

The existing Rust engine has useful, reproducible alignment capability. This audit does **not** recommend replacing it. It recommends repairing specific implementation paths and strengthening independent checks.

| Area | Judgment at audited HEAD |
|---|---|
| Offset/drift estimation | Promising, with reproducible synthetic and real results; normalization and validation assumptions still need correction/evidence |
| Audio export safety/correctness | **Blocked** by reproduced defects, including an input-overwrite path |
| GUI | Technical preview; sufficient foundation for beta after essential fixes; insufficient inspection/control for stable 1.0 |
| Windows executable | Builds; installer pipeline **blocked**; live GUI not independently inspected in this audit |
| macOS Intel / Apple Silicon | Configured but untested; no native support claim justified |
| Linux x86_64 | Configured but untested; packaging/runtime concerns remain |
| Publication hygiene | No recordings/binaries/recognized credential patterns found in reachable history; binary notices missing; advisory scan unverified |

### Evidence labels

**E** means experimentally reproduced on this checkout. **S** means a source/configuration finding, with runtime impact not experimentally established on every affected platform. **D** means a product/design recommendation. P1 blocks the affected release path; P2 should be scheduled before stable and may block beta where stated; P3 is lower priority. **No P0 finding was established.** Absence of P0 is not approval to release.

## Scope and preservation

Read the requested repository documents, manifests/lockfile, core matching/features/calibration/render/decode/verification implementation, desktop state/jobs/layout/timeline/platform code, tests, workflows and packaging scripts. Inspected current HEAD, all reachable commit names and all 177 unique permitted historical blobs. No remote is configured. Starting working tree was clean.

No application source, tests, manifests, lockfile, workflows or packaging configuration were edited. No commit, push, upload or publication occurred. Reports are uncommitted. Experiments, fresh source archive/build and generated media are under ignored `work/codex-audit/`; a separate scratch Cargo harness exercises public APIs. Private research, decompiled code and proprietary internals were not opened or used.

The computer-use skill was read and the release desktop launch was attempted. Computer Use was interrupted by physical Escape. After the user authorized a retry, the helper again returned the Escape interruption. No further UI inputs were issued. Thus **there is no new live screenshot, DPI test or successful interactive import/export walkthrough in this audit**. Previous screenshots and developer assertions are not treated as live evidence. See UX_REVIEW.md.

## P1 findings

### P1-01 — sidecar outputs can overwrite an input recording [E]

References: `crates/stagesync-desktop/src/jobs.rs:303`, `:426`, `:459`; `crates/stagesync-cli/src/main.rs:273`, `:676`; `crates/stagesync-core/src/export/mod.rs:133`, `:180`.

Only planned audio filenames are checked against inputs. CSV, JSON, SVG and text files are later written directly with truncating writes. In the harness, a valid synthetic WAV was copied to `probes/syncmap.csv` and supplied as the report's reference path. `export_report` returned success and changed that input into CSV. No real recording was touched. A sidecar symlink/hardlink to a normal input also deserves explicit coverage because sidecar writes follow existing files.

The actual CLI reproduced this without a constructed report: copied the synthetic ramp WAV to `work/codex-audit/cli-sidecar-probe/syncmap.csv`, then ran `stagesync analyze --reference work/codex-audit/cli-sidecar-probe/syncmap.csv --source work/codex-audit/probes/ramp.wav --out work/codex-audit/cli-sidecar-probe`. Exit **0**; the reference file's first line became the syncmap CSV header. Evidence: `work/codex-audit/cli-sidecar-probe.log`.

**Fix:** build one complete output manifest, including every sidecar and temporary destination; validate it against every input before writing. Use safe file-identity/path resolution, refuse unexpected aliases, and stage sidecars too. Apply the same protection to analyze-only exports. Test preserved input hashes for every artifact filename, not just WAV names.

### P1-02 — merged export destroys stereo and uses the wrong rate for later sources [E]

References: `crates/stagesync-core/src/render/mod.rs:347`, `:394`, `:447`, `:455`.

`SourceCursor` averages channels to mono and the merged writer duplicates that mono into the first source's channel count. A stereo input with left +0.25 and right -0.25 becomes silence in both output channels. This loses content, not merely metadata. Geometry for every source uses `first_meta.sample_rate`. With an 8 kHz first source and 16 kHz second source, a 1000 Hz tone in the second plays at approximately **500 Hz**, despite scale=1.

**Fix:** maintain independent per-channel cursors driven by the source's own sample rate and a shared position map. Define an explicit channel-layout policy for heterogeneous sources; reject unsupported combinations rather than silently folding them. Add independently read merged stereo and mixed-rate tests. Existing stereo tests exercise source-aligned rendering and miss this path.

### P1-03 — skipping input before the first output corrupts the resampler's absolute buffer origin [E]

Reference: `crates/stagesync-core/src/resample/mod.rs:244`–`:248`; affected callers include `crates/stagesync-core/src/render/mod.rs:267`, `:449`.

The code drains `min(drop, buffer.len())` samples but advances `buffer_start` by the larger, unclamped `drop`. When the next required sample lies well beyond the first decoded block, later input is mislabeled with future indices. A three-second 8 kHz ramp, rendered with offset -2 seconds and 1024-frame blocks, gives **0.082625** at reference 0.1 seconds instead of **0.7**. This is actual wrong audio. Negative offsets and merged cursors first opened late are affected candidates.

**Fix:** advance the absolute origin only by samples actually removed; preserve exact input-index accounting through skipped blocks. Test large negative offsets, late cursor activation, overlapping-priority sources, varied block lengths and rendered samples against independent truth.

### P1-04 — merged coverage can invent an uncovered span inside a fully covered range [E]

Reference: `crates/stagesync-core/src/matching/segments.rs:512`–`:568`.

The merge sorts intervals by start, appends overlaps and assigns `cursor=end`, even when that moves the cursor backward. Source 0 covering [0,10] plus source 1 covering [2,3] produces both overlapping matched spans **and an uncovered [3,10] span**. The first source actually covers all ten seconds. Source priority is not resolved by this algorithm despite its comment.

The renderer separately chooses owners from whole-file affine ranges (`render/mod.rs:398`–`:453`), not the report's trusted/gap segments. Consequently a higher-priority source's internal recorder gap can hide usable material from another source, and the display/report need not describe the audio that is written.

**Fix:** sweep all interval boundaries, resolve each elementary span by documented priority and usable coverage, and derive display/export ownership from the same validated plan. Test nested overlaps, priority reversals, internal gaps covered by another recorder, and exact rendered ownership.

### P1-05 — per-source timeline export exceeds its advertised length [E]

Reference: `crates/stagesync-core/src/render/mod.rs:258`–`:294`.

`content_end` is calculated and discarded. Every resampled sample and the filter tail is written; only short outputs are padded, long outputs are never bounded. One second of 8 kHz input at offset +0.5 on a one-second reference produces **12,032 frames**, rather than 8,000. Real Voice006 extends beyond the reference, so this is a relevant geometry, not an artificial impossible input.

**Fix:** bound timeline output to the requested reference frame range, including the flush. Keep beyond-reference material in the source-aligned layout, as documented. Test both layouts explicitly so preserving source material does not defeat timeline length guarantees.

### P1-06 — temporary WAV names collide within one process and invalidate atomicity [E]

References: `crates/stagesync-core/src/audio/wav.rs:61`, `:98`–`:104`, `:203`–`:233`; `crates/stagesync-core/tests/common/mod.rs:371`–`:384`.

Temporary names contain destination and PID only; creation truncates an existing temporary file. Two simultaneous writers to one destination share the same file. Deterministic harness sequence: open both, write 0.25 with the first and 0.75 with the second, finalize first (success), finalize second (missing temporary file). The first successful destination now contains **0.75**, not its own samples. Thus an unsuccessful writer can affect another writer's committed output.

The cold parallel suite independently reproduced this interaction: eight alignment tests failed while racing to create the same cached reference fixture. The cache releases its mutex before generating/writing. A serial suite passed all 133 tests.

**Fix:** unique, exclusively created temporary files; avoid PID-only naming; protect fixture creation with one generation owner and per-run temporary storage. Test simultaneous same-destination writers, failure after another writer commits, and cold parallel test runs.

### P1-07 — ordinary RIFF output has no size guard for recordings beyond 4 GiB [S]

References: `crates/stagesync-core/src/audio/wav.rs:91`–`:105`, `:154`–`:193`; `Cargo.lock` pins hound 3.5.1. Inspected dependency `hound-3.5.1/src/write.rs:175`, `:435`, `:492`.

The underlying writer counts data bytes in `u32` with unchecked release arithmetic. StageSync neither preflights the final size nor uses RF64/splitting. Three hours of stereo float at 96 kHz is **8,294,400,000 data bytes**, beyond RIFF capacity. Release arithmetic can wrap and finalize an invalid size/header; debug arithmetic can panic. A completed renamed file is therefore not necessarily a valid complete long recording. This audit did not allocate/write a multi-gigabyte overflow fixture.

**Fix:** RF64 or deterministic splitting, or fail before rendering if the maximum RIFF chunk size would be exceeded. Enforce bounds incrementally for unknown-length inputs. Test boundary arithmetic with a small virtual writer and validate a large-file integration case on the release build.

### P1-08 — feature normalization violates the documented silence-independent rule [E/S]

References: `crates/stagesync-core/src/features/envelope.rs:213`, `:300`–`:366`; repository AGENTS.md, previously failed normalization rule.

The implementation computes each local mean/variance from that file's non-quiet frames when at least eight remain. AGENTS.md expressly prohibits making normalization depend on silence placement. A deterministic public-function probe with identical values and different quiet masks changed the normalized **active** frames by as much as **0.71803236**. Masking the correlation later does not undo changes already made to neighboring active samples.

This proves a feature-shape dependence and contract violation. It does **not**, by itself, measure a wrong drift on the concert files or quantify the bias for every microphone pair. Existing passing offset/drift tests do not resolve that concern.

**Fix:** reconcile implementation with the normalization rule; preserve comparable statistics and exclude quiet samples at correlation. Add ground-truth asymmetric-silence and changing-microphone tests with accumulated rendered-time error and uncertainty coverage. Do not silently loosen confidence or drift gates.

### P1-09 — waveform window extraction retains the entire suffix after its last requested window [S]

Reference: `crates/stagesync-core/src/audio/windows.rs:99`, `:125`–`:155`.

Once `next == plan.len()`, the buffer is no longer trimmed, yet decoding/resampling and appending continue until EOF. A short source near the beginning of a long reference can therefore retain almost the entire remaining reference at analysis rate. At the default 8 kHz mono f32 rate, an unused three-hour suffix is approximately **346 MB**, excluding other buffers. An early-window synthetic call succeeds, but process RSS growth was not isolated experimentally; the lifetime/control-flow defect is established statically.

**Fix:** stop decoding once all requested windows have been satisfied, with correct EOF/filter-margin handling. Test equal window requests on increasingly long suffixes, measuring peak memory and decoded work. The existing two-hour fixture places its source late in the reference and does not stress this case.

### P1-10 — Windows packaging stops before the build [E]

Reference: `packaging/windows/build.ps1:47`–`:49`; `.github/workflows/release.yml:95`–`:146`.

The availability check executes `cargo build --version`. Cargo rejects that argument. Running `./packaging/windows/build.ps1 -OutputDir work/codex-audit/windows-package` exited **1**, incorrectly reporting that Cargo is unavailable. The release Windows job uses this script and cannot package successfully as written.

**Fix:** use `cargo --version` or resolve the executable. Then actually compile/install/uninstall the NSIS artifact on a clean Windows machine. Also inspect `installer.nsi:60`: `InstallDir "$STAGESYNC_INSTALL_DIR"` does not use the `${STAGESYNC_INSTALL_DIR}` define syntax. This latter concern was not tested with NSIS because packaging failed first.

### P1-11 — binary packages omit the license/notices material required by the repository's own checklist [S]

References: `LICENSES.md:848`–`:896`; `packaging/windows/build.ps1:113`–`:153`; `packaging/windows/installer.nsi:104`; `packaging/macos/build.sh:74`; `packaging/linux/build.sh:63`, `:150`.

The scripts copy the executable, selected icons/desktop files and Windows README/ZIP. They do not assemble the project licenses, third-party copyright/license notices, embedded-font notices or Symphonia source-availability information. A large dependency table in the source tree is useful but is not a notices bundle shipped with the binaries.

**Fix:** generate and package notices for the actual target dependency graph, include project licenses and the MPL text, and provide an accurate route to corresponding MPL-covered source. Verify package contents on all targets. MPL 2.0 §3.2 describes executable-form/source-availability obligations; see the [official license](https://www.mozilla.org/en-US/MPL/2.0/). No blanket incompatibility of MPL linking with this project was found; this is a distribution-completeness finding, not a recommendation to remove Symphonia.

## P2 findings

### P2-01 — a displayed hard manual anchor can be lost when the waveform model wins [E]

References: `crates/stagesync-core/src/pipeline.rs:381`, `:533`–`:568`; `crates/stagesync-core/src/matching/drift.rs:417`; `crates/stagesync-desktop/src/app.rs:1050`–`:1128`.

The anchor adjusts the calibrated envelope model before `assess`; the selected waveform fit supplies a different intercept afterward. Synthetic correspondence with an explicit anchor source=30, reference=35.05105 yielded final reference **35.0010600743**, method Waveform: almost **50 ms away from the hard constraint**. This is not an argument that the deliberately shifted anchor was acoustically right; it proves the UI's override contract is not honored. Offset-only mode also computes its offset anew from automatic points. Anchors are keyed by basename, so duplicate basenames can share one correspondence accidentally.

Fix before beta or disable/label the override: apply a documented final constraint policy, key anchors by recording identity, preserve the automatic baseline separately, show conflicts with evidence, invalidate prior export verification, and verify the overridden render.

### P2-02 — verifier WAV reader miscounts stereo frames and scales 24-bit PCM incorrectly [E]

References: `crates/stagesync-core/src/audio/wav.rs:275`, `:306`–`:339`.

`reader.len()` counts channel samples; an 8,000-frame stereo file reports 16,000 frames. PCM wider than 16 bits uses a fixed 2^31 divisor; a 24-bit half-scale sample reads **0.001953125**, not 0.5 (48.16 dB error). Beyond-EOF reads can therefore turn into invented zero padding in stereo validation, and 24-bit verification has distorted level/energy thresholds. Main rendering decodes via Symphonia; this finding is specifically about the random-access verifier/reader path.

Fix using frame duration and the actual valid bit width. Test 8/16/24/32-bit PCM, float, stereo and EOF bounds against an independent reader.

### P2-03 — offset-only still invokes the sinc resampler and appends a tail [E]

References: `crates/stagesync-core/src/render/mod.rs:179`–`:217`; `crates/stagesync-core/src/resample/mod.rs:195`–`:250`; README.md:142, :349.

No unity-map bypass exists. An 8,000-frame unity-rate/zero-offset input produces **8,032 frames**. Source/timeline channel banks never set the resampler output limit. This contradicts both exact-duration expectations and the promise that offset-only performs no resampling. A same-rate sample-copy path is necessary; a user-requested sample-rate conversion still requires resampling and must be described honestly. Test decoded sample equality and exact frame counts, not just two repeated renders against one another.

### P2-04 — collision comparison does not model target-filesystem equivalence [S]

References: `crates/stagesync-core/src/render/mod.rs:554`–`:584`; `crates/stagesync-desktop/src/jobs.rs:536`.

Windows comparisons use ASCII-only case folding. macOS uses byte-sensitive equality even on a case-insensitive filesystem; no Unicode normalization handling exists. Case variants/non-ASCII variants can pass the guard then name one file on the target volume. Parent resolution also falls back on unresolved paths after errors. Direct Windows ASCII duplicate-name protection exists and should be retained. Native case/normalization and alias tests are needed; those platforms were not experimentally exercised here.

### P2-05 — independent verifier's default grid includes an invalid endpoint and is too sparse for Voice006 [E/S]

Reference: `tools/verify_exports.py:724`–`:737`.

When the reference ends first, `hi=reference.duration-offset` is not reduced by the reference window half-width. The default last Voice006 window ends **10.250 seconds beyond the reference**. Two more positions have ambiguous waveform consensus and two have waveform/envelope disagreement. The default run consequently cannot establish accuracy. See the full assessment below.

Fix the overlap-window bounds and provide a predetermined denser sampling policy, retaining all reliability gates and reporting every rejection. Do not lower min-agreement, peak-ratio or the required trusted count. Passing a sampled region does not justify declaring a disputed position correct.

### P2-06 — CLI report.json omits corrected WAV hashes [E]

Reference: `crates/stagesync-cli/src/main.rs:697`–`:705`.

The JSON report is serialized before rendered artifacts are appended. Fresh real-export report.json listed five sidecars and no corrected audio. Those five hashes matched their actual files, but the report cannot authenticate either WAV. The desktop exporter does append renders before serializing (`jobs.rs:455`), so CLI and GUI differ. Fix ordering and require hash/metadata errors to surface rather than silently producing missing hashes or zero sizes. Include the final verification result for the exact hashed audio.

### P2-07 — long-performance inspection and large collections have concrete layout limits [S/D]

References: `crates/stagesync-desktop/src/app.rs:714`, `:965`, `:1018`–`:1045`, `:1511`–`:1522`; `crates/stagesync-desktop/src/main.rs:24`.

Fit clamps zoom to at least 0.2 px/s. A three-hour performance therefore needs **2,160 logical pixels** regardless of the available center width; it cannot fit a normal window. Files and central details have no vertical ScrollArea around their full content, unlike the message log/export-file list. Long collections and detailed results can become inaccessible. The 960×640 minimum is also too large for the previously reported 720×450 logical display. Live behavior was not captured during this audit, so these are implementation-derived findings.

Fix Fit to honor the viewport, make side/central content reachable, adapt the toolbar/panels to small windows, and expose validation status plainly. A zoomable waveform inspection track is recommended for stable release, not a wholesale application rewrite or a prerequisite to beta. UX_REVIEW.md gives the prioritized design.

### P2-08 — test count hides environmental dependencies and correlated assertions [E/S]

References: `crates/stagesync-core/tests/common/mod.rs:363`–`:384`; `crates/stagesync-core/tests/robustness_tests.rs:187`–`:198`, `:282`–`:355`; `crates/stagesync-core/tests/alignment_tests.rs:127`; `crates/stagesync-core/tests/render_tests.rs:23`.

Fixtures are cached in a shared OS temp directory across runs, contrary to the claimed per-test hermetic TempDir approach, and cold parallel generation races. The AAC test launches external ffmpeg and returns success if encoding fails. In this audit ffmpeg printed encoder errors, yet the AAC test was reported **ok**. Thus 133 passing does not mean all 133 claimed behaviors were exercised. The memory test subtracts process lifetime peaks; prior tests can mask its growth, and non-Windows memory assertions are skipped. Many accuracy tests score fine-envelope measurement positions rather than an independently decoded final render. The two-hour offset fixture is not a multi-hour drift-correction accuracy test.

Fix fixture isolation/synchronization, make optional external coverage explicit, and add cold CI plus independent rendered truth. Important missing cases include mixed-rate merged stereo, nested overlaps, large negative offsets, RIFF limits, truly excised media/time discontinuities, changing microphone behavior and uncertainty coverage over long baselines.

### P2-09 — independence and uncertainty claims exceed the validation actually implemented [S]

References: `crates/stagesync-core/src/verify.rs:8`–`:13`, `:491`–`:502`; `crates/stagesync-core/src/pipeline.rs:695`–`:751`; `crates/stagesync-core/src/matching/drift.rs:84`–`:133`; `crates/stagesync-core/src/matching/fine.rs:45`–`:49`.

Distinct center points do not make disjoint windows: default fine windows are 30 seconds wide at 15-second hops; waveform probes are six-second windows between those centers. The 20-second rendered verification windows overlap fitted material too; when the waveform estimator determines the applied slope, later GCC-PHAT checks also share estimator code and signal. The Python implementation remains a valuable separate check, but neither language independence nor shifted centers makes all sample evidence statistically independent.

The classical OLS standard error assumes independent errors, while fine windows overlap. The code documents this as a lower bound; product claims should do the same. Test uncertainty coverage with repeated independently generated microphone/noise conditions and use reserved time blocks or an explicitly justified covariance-aware policy. Do not change the established scale/drift sign convention.

### P2-10 — cancellation/responsiveness guarantees need qualification and stronger job identity [S]

References: `crates/stagesync-desktop/src/jobs.rs:109`, `:245`, `:408`; `crates/stagesync-desktop/src/app.rs:318`–`:334`, `:426`, `:504`, `:1059`, `:1345`, `:1524`; `crates/stagesync-core/src/audio/decode.rs:187`.

Export cancellation is checked between files and after the last file; it cannot stop a multi-hour render already underway. This limitation is visibly labeled. Atomic temporary files now make cooperative block cancellation feasible. Verification has no cancellation token. Probing creates one thread per file and hashes the full file; large imports have no concurrency cap. Hashing at AudioDecoder::open is outside the decode cancellation checkpoints. Progress is a stage/file count, not decoded/rendered work, and can show 100% at the beginning of the last expensive stage.

Verification events carry only source index, not report/export generation. Some edit/anchor controls guard `running` but not `verifying`; stale verification can be attached to a changed report. The explicit repaint request also covers `running` only, not probing/verification. These are static race/repaint risks; no live freeze was demonstrated. Fix with bounded workers, block cancellation, event generation identities and worker-triggered repaint notifications.

### P2-11 — Linux packaging is not yet a verified distributable [S]

References: `packaging/linux/build.sh:23`, `:63`–`:71`, `:169`–`:173`; `.github/workflows/release.yml:210`, `:263`–`:277`.

Debian control uses Architecture `x86_64`, whereas the Debian architecture identifier is **amd64** ([Debian's port description](https://www.debian.org/ports/amd64/)). Use distinct package and AppImage architecture names and test ordinary installation without force options. AppDir carries the executable/launcher/icon, not a bundled dependency set; calling it self-contained is unproved. `ubuntu-latest` supplies an unspecified moving libc/runtime baseline, and the unversioned libc dependency does not establish compatibility with older distributions. Use ldd/readelf and clean supported-distro launch tests; generate package dependencies from the built binary where appropriate.

The workflow downloads and executes appimagetool from moving `continuous` without a verified checksum. Pin the tool/release and checksum, audit FUSE-less extraction, and run the real AppImage. No Linux installer or runtime test was executed here.

### P2-12 — current documentation overstates validation and disagrees with newer reports [S]

References: PROJECT_STATUS.md:9–:16 and its real-data verification section; BUILDING.md's verified/unverified and runner sections; CODEX_HANDOFF.md's release checklist; README.md:142, :349; `crates/stagesync-core/src/export/mod.rs:228`–`:249`.

Older status sections claim both real exports independently verified and explain away a weak waveform outlier as definitively an artifact. The latest pre-release report correctly states default Voice006 INSUFFICIENT DATA. A weak waveform/envelope disagreement does not establish which method is right at that exact location. This audit's denser grid adds new evidence, but it does not vindicate that categorical claim. BUILDING still mentions retired macos-13 and GUI unviewed, while newer reports say otherwise; the handoff says make export atomic after stating it was done. Plain-text summaries also print drift figures without their accompanying uncertainty, though the JSON assessment carries it.

Rewrite the publication-facing claims from one dated evidence table, explicitly distinguish configured/native-tested/runtime-tested, and include uncertainty/status anywhere drift is presented. Preserve disputed-position details rather than promoting envelope agreement to ground truth.

### P2-13 — calibration maps quiet flags in the opposite direction from its audio samples [S]

Reference: `crates/stagesync-core/src/calibration.rs:184`–`:216`.

The waveform/envelope resampler reads input at `alpha * output_index`, with alpha=1/scale, but quiet flags use `output_index / alpha`. For non-unity scale these select different source positions. The subsequent dB condition can hide some effects, but the flags still do not represent the same coordinate. Fix the map and test silence boundaries under positive/negative/high drift. No distinct real-recording error was attributed to this path in this audit.

## P3 findings

**P3-01 [S]:** `crates/stagesync-core/src/matching/drift.rs:65` computes SlopeFit.signal_to_noise using `abs(slope)/SE`, rather than `abs(slope-1)/SE` for a clock scale. Current release decisions use MethodFigures.signal_to_noise (the correct drift/SE expression), so this is a misleading public helper rather than a demonstrated current gating failure. Correct or narrow its semantics and test it.

**P3-02 [S/D]:** packaging is not reproducible as a release process: floating stable toolchain, moving runner images/actions/tool downloads, timestamp-sensitive Windows ZIP/macOS archives, and no release checksums manifest. Deterministic audio exports and reproducible software packages are separate claims. Pin inputs and publish checksums after the functional/package blockers are resolved.

## DSP assessment and real-recording independent verification

The core coordinate convention is consistent in the tested main paths: reference=scale*source+offset, drift=(scale-1)*1e6, renderer step=source_rate/(output_rate*scale), pitch=-1200*log2(scale). No sign change is warranted. Correlation uses conj(A)*B; candidate ranking uses fixed-normalization strength and peaks remain strongest-first. Coarse slope is held at unity; fine robust inlier indices are mapped back through the confident subset; segmentation spacing uses all measured points. These historical fixes are present. The decoder breaks after a decoded packet, avoiding the previous whole-file-in-one-call bug.

Drift decisions carry the OLS-derived SE, default to 3× significance, prefer an eligible waveform measurement, and reject low raw correlation. Calibration disagreement is retained and contributes to envelope uncertainty. These are sound safeguards, subject to the normalization/covariance and render defects above. A single affine map is what the actual pipeline emits; existence of a piecewise TimeMap type is not evidence that recording edits or clock jumps are handled automatically.

### Fresh measurements

Commands were run against the original read-only audio, writing only audit scratch output. Separate analysis: **33.8 s**. Fresh CLI export including its repeated analysis, rendering, Rust checks and artifacts: **96.558 s**, exit 0; analysis within that command was **32.5 s**. The remaining approximately 64 seconds includes rendering, Rust verification and artifact I/O; it is **not** an isolated renderer benchmark. Concurrent tests/compilation affected workload, so these are observations, not controlled performance guarantees.

| Source | Applied offset | Applied drift ± SE | Drift method |
|---|---:|---:|---|
| Voice005 | +317.389329056 s | +16.115881830 ± 0.099071586 ppm | Envelope; waveform rejected |
| Voice006 | +5753.313995859 s | +16.500035972 ± 0.159987491 ppm | Envelope; waveform rejected |

No historical ppm value was hard-coded into analysis. Exact figures above came from newly generated plan.json. SHA-256 of all three original inputs after export matched the hashes captured during analysis. Both fresh corrected WAVs matched the pre-existing final_export3 WAV hashes byte-for-byte. This establishes reproducibility against those prior local artifacts; a second fresh render was not needed for that comparison. Five CLI-reported sidecar hashes matched their files; WAV hashes were independently checked because CLI report.json omits them (P2-06).

| Verification on fresh corrected audio | Trusted | Result over trusted positions |
|---|---:|---|
| Voice005, Python default unchanged gates | 5/8 | p50 0.068 ms; p95 1.485 ms; max 1.838 ms |
| Voice006, Python default unchanged gates | 3/8 | **INSUFFICIENT DATA**; no overall accuracy claim |
| Voice006, predetermined 24-position interior grid, identical gates | 18/24 | p50 0.030 ms; p95 0.990 ms; max 1.916 ms; 2 disputed, 4 untrusted, 0 errors |
| Voice005, Rust rendered holdout | 142/160 | p50 1.879 ms; p95 2.249 ms; max 2.316 ms |
| Voice006, Rust rendered holdout | 108/134 | p50 1.992 ms; p95 2.245 ms; max 9.120 ms |

Python default Voice006 failures break down as follows: two waveform-consensus agreement fractions 0.56 and 0.44 below the unchanged 0.60 gate; two waveform/envelope disagreements of 26.34 and 28.40 ms; one reference-end window error. At those disputed positions the waveform reports approximately zero residual while the envelope is displaced. That points to estimator disagreement, not proof of a bad render or proof the envelope is right. The 24-position grid was selected uniformly **before measurement** over [20 seconds, common-overlap-end minus 20 seconds], not cherry-picked from successful positions. All gates and required trusted count remained unchanged.

**Judgment:** the old default run was insufficient independent evidence, not demonstrated synchronization failure. The new dense-grid result supplies independent sampled validation of Voice006 and removes the need to characterize that entire file as wholly unverified. It does not establish accuracy in rejected/unsampled regions, independently measure a true drift to the last decimal, or prove all microphone conditions. This issue alone would permit an honestly labeled beta; the reproduced export defects currently prevent that beta.

Detailed evidence: `work/codex-audit/voice005-default.{json,md,log}`, `voice006-default.{json,md,log}`, `voice006-grid24.{json,md,log}`, `real-analysis/`, `real-export/`, and `harness.log`.

## Atomic export and failure-path assessment

Atomic currently means **individual WAV visibility after successful finalize/rename**, not a transaction for a whole export. Sidecars are direct writes. If source N fails, earlier WAVs remain. If a rerun fails, a prior report can remain beside a mixture of old/new audio. Cancellation after a render can leave a complete WAV without a new completion report; the GUI explicitly says files written so far were kept. No common transaction identifier or atomic completion manifest establishes that a directory is one complete run.

Experiments: an abandoned ordinary writer preserved an existing destination byte-for-byte; normal replacement of an unrelated existing destination succeeded on this Windows system; simultaneous writers failed the integrity test described above. Read-path/render errors are propagated, and hound.finalize explicitly flushes the underlying BufWriter and returns flush errors. No real disk-full, ACL-denial, process-kill or power-loss test was performed. Do not claim those scenarios passed.

There is no sync_all or parent-directory durability step, and Drop cleanup cannot run after process termination. PID-based `.stagesync-part` files may remain after a crash. Initialization failure before the wrapper owns the temporary path also has no wrapper cleanup. Platform-specific rename/replacement behavior on macOS/Linux was not executed; same-directory placement is a useful foundation but not proof of universal crash durability.

Recommended beta behavior: stage a run in a distinct directory, preserve existing outputs on failure, publish a final atomic completion manifest covering exact hashes, and make canceled/partial runs unmistakable. Validate all inputs/output aliases first. Add safe scratch error injection for write, flush, rename, and interrupted runs.

## Build/test evidence

| Actual command/run | Exit | Evidence/result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | No formatting differences |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | Passed |
| `cargo build --release --workspace` | 0 | Passed |
| `cargo test --workspace`, default sandbox TEMP | 1 | Four core window tests failed on temp-file rename Access denied; stopped before rest of suite |
| `cargo test --workspace`, TEMP/TMP=`work/codex-audit`, empty fixture cache | 101 | Core unit tests passed; alignment suite 4 passed / 8 failed, concurrent shared-temporary rename errors |
| `cargo test --workspace -- --test-threads=1`, separate repository scratch TEMP/TMP | 0 | **133 passed**; AAC test silently skipped after external encoding errors |
| `cargo test --workspace`, same now-populated fixture cache, normal parallel execution | 0 | **133 passed**; warming the cache avoids the cold generation race; AAC encoding still skipped |
| `cargo build --manifest-path work/codex-audit/fresh-source/Cargo.toml --workspace --offline --locked --target-dir work/codex-audit/fresh-target` | 0 | Fresh HEAD archive, empty build directory, all workspace crates built; 51.14 s |
| Scratch API harness, final run | 0 | Reproductions above; an earlier run exited 101 parsing an older local plan schema, then was pointed at the new analysis plan |
| Windows packaging script | 1 | Invalid Cargo availability check |
| Real analysis / real export | 0 / 0 | Measured figures above |
| Python verifier default005/default006/grid24 | 0 / 0 / 0 | Exit 0 includes insufficient-data reporting; inspect the result status, not only exit code |
| `cargo audit --version`; `cargo deny --version` | unavailable | Both commands absent; no lockfile-wide advisory/license-policy scan completed |

Serial synthetic coverage actually executed includes clean/negative/multi-hour offsets; +10/+35/-35/+1000 ppm drift; low level; sample-rate differences; EQ/noise; repeated material; head/tail silence; recorder gaps; input immutability; deterministic source rendering; stereo source-aligned rendering; cancellation; and long-file analysis. These tests reuse the project's fixtures and some production reader/verification code. Independent public-API probes exposed failures those tests miss. This audit did not complete a new exhaustive cross-product of three-hour drift, clock discontinuities, changing microphones and hostile malformed containers; these remain explicit validation gaps.

## Cross-platform readiness

| Target / artifact | State | Evidence and remaining gate |
|---|---|---|
| Windows x64 development binaries | **VERIFIED build**, qualified tests | Local release build and serial 133-test run; default cold parallel gate fails; GUI launch attempt only |
| Windows 10/11 clean-machine usability | **CONFIGURED BUT UNTESTED** | No clean Windows 10/11 installation/runtime matrix or successful interactive GUI session in this audit |
| Windows portable ZIP / NSIS | **BLOCKED** | Packaging script fails immediately; NSIS itself not exercised; CRT prerequisite external |
| macOS Intel | **CONFIGURED BUT UNTESTED** | Native macos-15-intel jobs exist; no executed job or Intel launch test |
| macOS Apple Silicon | **CONFIGURED BUT UNTESTED** | Native macos-latest job exists; verify produced Mach-O architecture and deployment target on runner |
| macOS .app/.dmg | **CONFIGURED BUT UNTESTED** | Bundle layout/Info.plist provided; unsigned/not notarized; no install/Gatekeeper/runtime evidence |
| Linux executable | **CONFIGURED BUT UNTESTED** | Ubuntu runner/build libraries configured; native build/runtime not executed here |
| Linux AppImage | **CONFIGURED BUT UNTESTED**, distribution claims blocked | No native bundle/launch result; unpinned tool and unbundled runtime issues |
| Linux .deb | **BLOCKED for ordinary release readiness** | Wrong architecture identifier and no native install/runtime evidence |

The current Intel label is not inherently broken: the [official runner image inventory](https://github.com/actions/runner-images) lists macos-15-intel. macos-latest is a moving arm64 label in the current inventory. Native labels are a sensible architecture strategy; names alone do not validate binaries. Add architecture assertions and test the oldest advertised OS/runtime, not merely the current hosted runner.

CI config covers four native build/test targets and uploads desktop plus CLI binaries. Packaging scripts build the desktop only. Release workflow tests on Linux before packaging, but the packaging jobs themselves do not each execute the native suite. They depend on a separate CI workflow having run successfully; no explicit release dependency on those four CI results exists. The current Windows packaging failure alone prevents the complete release graph from finishing.

## Security, licensing and history

Full reachable history scan: 16 commits, 89 distinct paths, 177 unique permitted blobs; largest blob approximately 128 KB (Cargo.lock). No tracked audio, executable, library, archive, private-research/decompile path, or recognized private-key/GitHub-token/AWS-key/OpenAI-key pattern was found. The scan checked selected patterns, not every possible secret encoding. A generic personal-home path example exists in packaging/README.md. Repository and generated reports also contain local/provenance path information; redact support reports before public sharing.

Clean-room boundary documents disclose their process limits; source-tree absence cannot prove what previous authors never saw. No conclusion of proprietary-code contamination was established. This audit did not inspect forbidden materials to make that judgment.

The manifests keep core independent of desktop/CLI and desktop independent of CLI. Cargo.lock pins registry dependencies and checksums. License inventories acknowledge Symphonia's MPL-2.0. Binary-notice assembly is still absent. No new dependency was added to application manifests.

RustSec exposure: **NOT VERIFIED**. cargo-audit and cargo-deny are absent. The [RustSec database](https://rustsec.org/advisories/) is available online, but a web search is not a complete scan of this lockfile, so no vulnerability-free claim is made. Install/use pinned audit tooling in CI with a current database, recording unavailable-data failures rather than treating them as clean.

Decoder robustness tests cover some empty/truncated/invalid media, not fuzzing or arbitrary malicious containers. DecodeError/ResetRequired packets are skipped without recording a timing discontinuity (`audio/decode.rs:310`); packet-loss preservation needs targeted timestamp tests. Finite/range validation for arbitrary public render models and extreme metadata also deserves hardening. No exploit was demonstrated.

## Prioritized handoff for MiniMax

1. **Protect all inputs and all outputs**, including sidecars; make complete/partial runs identifiable; use unique exclusive temporary files. Fix the cold parallel test race without weakening tests.
2. **Repair rendered audio:** per-source sample rates/channels in merged output; resampler skip-origin accounting; exact output limits/unity bypass; RIFF size safety. Reproduce the audit harness results as focused regression tests with independent decoding.
3. **Unify coverage and render ownership:** boundary sweep with nested overlaps, genuine recorder holes and explicit priority. Verify rendered samples agree with the timeline.
4. **Honor overrides and DSP contracts:** final manual-anchor policy, silence-independent normalization, quiet-flag coordinate map; preserve uncertainty and estimator disagreement. Add rendered ground-truth adverse cases.
5. **Fix packaging and notices:** Cargo availability check, NSIS definition, Debian amd64, notices/source-availability bundle, pinned AppImage tool and supported runtime baseline. Then execute native CI/install/launch checks. No automatic push/release is authorized by this report.
6. **Make beta inspection usable:** correct Fit, scroll/adapt panels, show verification insufficient/disputed/partial states and the actual export geometry. Add waveforms/A-B inspection incrementally before stable 1.0; retain automatic-first operation.

Acceptance requires rerunning the four gates with actual exit codes, cold isolated parallel fixtures, and independent verification of the final rendered/hash-identified artifacts. Do not lower verifier gates or replace the working Rust estimator to erase these findings.
