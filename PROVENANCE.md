# PROVENANCE.md — how StageSync was built, and what it is made of

This document is the provenance record for **StageSync**, an open-source, drift-aware
audio synchronisation tool written in Rust.

It answers four questions:

1. Was StageSync built from someone else's code? (§1, §2, §5)
2. What third-party code does it use, at which versions, under which licences? (§3)
3. Are those licences compatible with StageSync's own? (§4)
4. How can a reviewer check all of this for themselves? (§6, §7)

It is written to be readable by someone who has never seen the project. Where a fact
cannot be established from the repository alone, it is marked as such rather than
asserted.

---

## 1. The clean-room statement

### 1.1 What StageSync is

StageSync takes two or more recordings of the same performance — typically a soundboard
or FOH recording used as the **reference**, and one or more on-stage or phone
recordings used as **sources** — and works out how far each source is out of time
relative to the reference, including how that offset drifts over the length of the
recording. It then applies that correction, optionally renders the corrected sources
onto the reference timeline, and reports what it found.

It is a working implementation of that idea. It is not a description of a working
implementation, and it makes no claim about being suitable for any particular kind of
production work.

### 1.2 What was used as input

Only three categories of material informed the implementation:

1. **The project's own clean-room documents**, in the `cleanroom/` directory:
   - `cleanroom/CLEANROOM_SPEC.md` — a behaviour specification: the user-visible
     requirements, parameters and acceptance criteria, expressed as observable
     requirements rather than as a description of any particular implementation.
   - `cleanroom/BEHAVIOR_TESTS.md` — a black-box validation protocol: how to
     synthesise test signals, how to measure them, and what to measure.
   - `cleanroom/PROVENANCE.md` — the research-side boundary audit that describes where
     each class of information in the clean-room set came from, and what was
     deliberately excluded from it.
   - `cleanroom/PUBLIC_DSP_REFERENCES.md` — an enumerated list of published DSP
     literature (papers, books, standards) that the implementation may and does draw on.
2. **Publicly documented, independently published DSP literature**, exactly as
   enumerated in `cleanroom/PUBLIC_DSP_REFERENCES.md`. This includes, for example, the
   original Sakoe–Chiba dynamic-time-warping paper, Theil–Sen and RANSAC regression
   methods, the GCC-PHAT delay-estimation method, Crochiere–Rabiner multirate
   resampling, and ITU-R BS.1770 loudness measurement. These are *methods*, not code:
   StageSync's implementations of them were written from the publications.
3. **Third-party open-source libraries**, obtained from crates.io and PyPI under their
   own licences, listed in full in §3.

### 1.3 What was not used

The following were **not** consulted, copied, or used, in whole or in part:

- **No proprietary source code.** No source from the proprietary application that
  StageSync is functionally comparable to (the "reference product", identified in the
  `cleanroom/` documents as Synchro Arts' *Revoice Pro* 5.1.x) — or from any other
  closed-source project — was read, adapted, translated, paraphrased into code, or
  otherwise used.
- **No decompilation or disassembly.** No decompiler output, pseudocode listing,
  Ghidra project, function-address table, control-flow graph, or assembly reading of
  any proprietary binary was consulted. No decompiler or reverse-engineering tool
  output of any kind exists in this repository. The `cleanroom/` documents contain no
  such material either; `cleanroom/PROVENANCE.md` §5 records an explicit grep-based
  self-audit that the clean-room set contains no decompiled code, no engine-internal
  identifiers, and no proprietary numeric constants.
- **No reverse-engineered internal tables, parameter tables, or weight sets.** No
  internal structure layout, no internal parameter field name or offset, no numeric
  tuning constant taken from an engine, no model file, no weight vector, no binary
  blob, and no extracted constant was used. The only values taken from the reference
  product are **user-visible configuration values** — preset names, and parameter
  names and defaults — read from the preset files the product ships for its own users
  to read. That is the one class of reference-product-derived material the clean-room
  set legitimately contains, and `cleanroom/PROVENANCE.md` §2.1 records it as such.
- **No proprietary binaries.** No reference-product binary, or any part of one, is
  vendored, embedded, bundled, linked, or redistributed by this repository. The only
  binaries in the repository are StageSync's own build output (see `.gitignore`).
- **No cloned user interface.** The reference product's screens, dialogs, menus,
  icons, images, string tables, and layout were not copied. The command-line
  interface and the desktop GUI were designed from the behaviour described in
  `cleanroom/CLEANROOM_SPEC.md` §2 and §6 and from ordinary expectations of what such
  a tool should offer. Where the reference product's *observable workflow* is
  described in the clean-room specification — for example, that a user selects a
  guide track and one or more dub tracks, and that the product reports specific
  conditions when a signal or a pitch cannot be found — StageSync implements the
  capability, not the reference product's presentation of it.

### 1.4 An important qualification

The `cleanroom/` documents describe a research engagement that *did* include
reverse-engineering work, performed in a **separate, restricted workspace**
(`../private-research/`, outside this repository). The point of the clean-room
boundary was to keep that material away from the implementation: `cleanroom/PROVENANCE.md`
§3 lists the classes of RE-derived fact that were deliberately excluded from the
clean-room set, and §6 instructs the implementer to ignore the private workspace
entirely.

StageSync's implementation was written from the clean-room set plus public literature
only. **This repository contains none of the excluded material.** §7 is explicit about
what that does and does not let a reviewer conclude.

---

## 2. The independent origin of each component

Every functional part of StageSync was written for this project. No component below
is a port, a fork, a transliteration, or a reimplementation of someone else's code.
The "Method" column names the published literature the component is *based on*, where
one applies — a method published in a paper is not somebody's code.

| # | Component | Where it lives | What it does | Method / basis | Origin |
|---|---|---|---|---|---|
| 1 | **RMS-envelope feature with local z-score** | `crates/stagesync-core/src/features/envelope.rs` | Computes a dB-scaled RMS envelope, then locally z-scores it, so a 37 dB level difference between a soundboard and a phone recording becomes a constant offset rather than a change of shape. | Standard envelope-following and local standardisation. | Written for this project. |
| 2 | **Log-mel front end** | `crates/stagesync-core/src/features/mel.rs` | Log-mel spectrogram used for disambiguation of repetitive material and as an independent feature family for verification. | Mel filterbank as in Davis & Mermelstein (1980) / Slaney (1998); the FFT itself comes from `rustfft`. | Written for this project (filterbank and feature definition); FFT primitive is `rustfft`. |
| 3 | **Coarse global lock** | `crates/stagesync-core/src/matching/coarse.rs` | Finds *where* in the reference a source recording lives, across hours rather than seconds, by correlating many long non-quiet windows and requiring the candidate offsets to agree. | Normalised cross-correlation over the envelope. | Written for this project. |
| 4 | **Fine probes** | `crates/stagesync-core/src/matching/fine.rs` | Measures the exact correspondence point by point along the recording, attaching a confidence and an explicit reason when confidence is limited. | Cross-correlation peak picking with parabolic interpolation. | Written for this project. |
| 5 | **Robust regression (Theil–Sen / RANSAC / MAD)** | `crates/stagesync-core/src/matching/regression.rs` | Fits the drift line while tolerating a sizeable minority of wrong probe points, and reports the median absolute deviation so the fit's own quality can be judged. | Theil–Sen estimator (1964); RANSAC (Fischler & Bolles, 1981); MAD as a robust scale. | Written for this project. |
| 6 | **Piecewise time map** | `crates/stagesync-core/src/matching/model.rs` | The time model `reference_time = scale × source_time + offset`, which becomes several joined linear segments when the evidence shows the clock rate changed *during* the recording. | Linear clock model; piecewise extension. | Written for this project. |
| 7 | **Polyphase Kaiser-windowed-sinc resampler** | `crates/stagesync-core/src/resample/mod.rs` | Applies a small rate change while keeping the material transparent: a classic Crochiere–Rabiner polyphase interpolator with a Kaiser-windowed sinc (β = 8.6, ≈90 dB stop-band rejection) and an I₀ Bessel function computed in-tree. | Crochiere & Rabiner (1983); Kaiser window (1974). | Written for this project — no `libsamplerate`, SoX, `rubato` or similar is used. |
| 8 | **Segmentation and coverage model** | `crates/stagesync-core/src/matching/segments.rs` | Describes what is actually covered, what is missing and what is merely uncertain. Missing material is reported as a gap and never stretched, interpolated or force-aligned. | — | Written for this project. |
| 9 | **GCC-PHAT holdout verifier** | `crates/stagesync-core/src/verify.rs`, with the primitive in `crates/stagesync-core/src/matching/dsp.rs` | Validates the *rendered* result using a different method from the one used to fit the model: waveform GCC-PHAT at hold-out positions disjoint from the fitting probes, reported as p50 / p95 / max so one bad position cannot hide behind an average. | GCC-PHAT (Knapp & Carter, 1976; also Orfanidis). FFT primitive is `rustfft`. | Written for this project. |
| 10 | **Drift-graph writer (SVG)** | `crates/stagesync-core/src/export/graph.rs` | Writes the drift graph as plain SVG. | — | Written for this project, **specifically to avoid taking a plotting dependency**; see §5. |
| 11 | **Export formats (CSV / JSON / sync map / run report)** | `crates/stagesync-core/src/export/mod.rs` | Deterministic machine-readable artefacts; the same inputs and configuration produce byte-identical files. | — | Written for this project. |
| 12 | **WAV writer and reader** | `crates/stagesync-core/src/audio/wav.rs` | Streaming write and random-access read of WAV, preserving native channel count and native level (no normalisation, no automatic gain). | The WAV container encoding/decoding itself is provided by the `hound` crate. | StageSync's own streaming and level-preservation logic; container codec is `hound`. |
| 13 | **SHA-256 file hashing** | `crates/stagesync-core/src/audio/hash.rs` | Streams a file through a 1 MiB buffer to prove the originals were not modified, without loading it into memory. | SHA-256 primitive is provided by the `sha2` crate. | Written for this project; primitive is `sha2`. |
| 14 | **Rendering (three layouts)** | `crates/stagesync-core/src/render/mod.rs` | Turns the time model into corrected audio: source-aligned, reference-timeline-per-source, and a merged reference timeline. | — | Written for this project. |
| 15 | **Calibration loop** | `crates/stagesync-core/src/calibration.rs` | Closes the gap between *fitting* a line to measured correspondences and *applying* it, by re-measuring the corrected output rather than asking the estimator to confirm itself. | — | Written for this project. |
| 16 | **Analysis pipeline** | `crates/stagesync-core/src/pipeline.rs` | Stages the work so the CLI and the GUI can report progress and cancel cleanly. | — | Written for this project. |
| 17 | **Command-line interface** | `crates/stagesync-cli/src/main.rs` | `stagesync analyze`, `export`, `verify`, `info`. Argument parsing is by `clap`; the command surface and behaviour are StageSync's own. | — | Written for this project. |
| 18 | **Desktop GUI** | `crates/stagesync-desktop/` (`app.rs`, `chart.rs`, `timeline.rs`, `jobs.rs`, `state.rs`) | Native Windows window presenting the results, with long operations on background threads. The window is a thin shell over `stagesync-core`. Widget toolkit is `eframe`/`egui`; file dialogs are `rfd`. | — | Written for this project. |
| 19 | **Python tooling** | `tools/make_fixtures.py`, `tools/verify_exports.py` | Generation of test fixtures and independent verification of exported artefacts. | — | Written for this project; see §3.3. |

The three crates in the workspace are `stagesync-core` (the library, where all the
measurement lives), `stagesync-cli` (the binary named `stagesync`) and
`stagesync-desktop` (the native Windows application). All three are original work.

---

## 3. The dependency table

### 3.1 How the list was produced

Everything in this section was read out of the repository, not recalled:

- Exact versions come from `Cargo.lock` (format version 4), which is the authoritative
  resolved dependency graph for this workspace.
- Licence identifiers come from the `license` field of each dependency's own
  `Cargo.toml` manifest as published on crates.io.
- The "kind" column is derived from `cargo tree`, run against the
  `x86_64-pc-windows-msvc` target, with the `normal`, `build` and `dev` edges
  separated.

**Counts.** `Cargo.lock` pins **492** third-party packages (plus the three
first-party workspace members). A Windows x64 build compiles **242** of them
along the normal (runtime) edge, of which **12** are proc-macro crates that run at
compile time and are not linked into the shipped executable. **1** package
(`tempfile`) is a dev-dependency and is used only by tests. **No** package is reached
through a `[build-dependencies]` edge — the project has no build scripts and none of
its dependencies pull one in on this target. The remaining packages are present in
`Cargo.lock` because Cargo resolves for every platform it supports: they are for
Android, iOS, macOS, Linux (Wayland/X11), Redox, WebAssembly, or for other CPU
architectures, or they are reachable only through dev-dependencies.

### 3.2 Direct dependencies

These are the dependencies declared in `Cargo.toml` and the three crate manifests.

| Crate | Version | Licence | Kind | What StageSync uses it for |
|---|---|---|---|---|
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 | runtime | Error type for application-level failures (context chaining) in all three crates. |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | runtime | Derive-based error types for the library's own error enums. |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | runtime | Serialisation framework for the JSON report, the sync map and the CLI's parameter files. |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | runtime | JSON encoding and decoding for those artefacts. |
| `rayon` | 1.12.0 | MIT OR Apache-2.0 | runtime | Data-parallel execution across CPU cores during analysis and rendering. |
| `rustfft` | 6.4.1 | MIT OR Apache-2.0 | runtime | FFT used by the log-mel front end, the correlation primitives and the GCC-PHAT verifier. |
| `hound` | 3.5.1 | Apache-2.0 | runtime | WAV container encoding and decoding, behind StageSync's own streaming writer/reader. |
| `symphonia` | 0.6.1 | **MPL-2.0** | runtime | Decoding of compressed input formats (AAC in MP4/M4A, FLAC) and WAV/PCM passthrough. See §4 and `LICENSES.md` §4. |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | runtime | SHA-256 primitive for the file-hash audit trail. |
| `clap` | 4.6.7 | MIT OR Apache-2.0 | runtime | Command-line argument parsing for the `stagesync` binary. |
| `log` | 0.4.34 | MIT OR Apache-2.0 | runtime | Logging facade used across the workspace. |
| `env_logger` | 0.11.11 | MIT OR Apache-2.0 | runtime | Logging backend that writes `log` records to stderr. |
| `eframe` | 0.37.0 | MIT OR Apache-2.0 | runtime | `egui` application framework: the windowing and widget toolkit of the desktop application. |
| `rfd` | 0.17.2 | MIT | runtime | Native file-open and file-save dialogs for the desktop application. |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 | **test-only** (dev-dependency) | Temporary files in the test suite only. Not compiled into either shipped binary. |

`stagesync-core` additionally depends on `stagesync-cli` and `stagesync-desktop` do on
`stagesync-core` — first-party crates, not third-party dependencies.

Two licences in this table are **not** the usual dual MIT/Apache-2.0: `hound` is
**Apache-2.0 only**, and `rfd` is **MIT only**. Both are compatible with StageSync's
own licence (§4). `symphonia` is **MPL-2.0**, which is a weak copyleft licence and is
the one dependency that imposes an extra obligation on a redistributor; it is treated
in detail in `LICENSES.md` §4.

### 3.3 Python tooling dependencies

Python is used by the `tools/` scripts only. **The shipped application never invokes
Python, and neither of the two binaries has any runtime dependency on it.** The
scripts are developer tools: they generate test fixtures and they independently
verify exported artefacts.

| Package | Version | Licence | Kind | What it is used for |
|---|---|---|---|---|
| `numpy` | 2.5.4 | BSD-3-Clause | tooling (never shipped) | Array arithmetic in the fixture generator and the independent verifier. |
| `scipy` | 1.18.1 | BSD-3-Clause | tooling (never shipped) | `scipy.signal.resample_poly` and signal primitives in the independent verifier. |

The standard library modules used (`argparse`, `json`, `math`, `sys`, `dataclasses`,
`fractions`, `pathlib`, `typing`) are part of CPython and carry the Python Software
Foundation License.

The versions above are the versions present in the development environment in which
this document was written. They are **not** pinned by a lock file or a
`requirements.txt` in this repository, because nothing that ships depends on them.
A reviewer should treat them as "what was used to produce the checked-in fixtures",
not as a declared constraint.

### 3.4 Complete transitive dependency list

Every package pinned in `Cargo.lock`, with its exact version, its licence identifier
as declared in its own manifest, its build kind, and a one-line description taken
from that same manifest.

**Kind** means: `runtime` — compiled into the shipped Windows binary; `runtime (direct)`
— listed in §3.2; `proc-macro` — compile-time code generation, not linked into the
shipped binary; `test-only` — dev-dependency, not shipped; `not built for Windows
x64` — present in `Cargo.lock` because Cargo resolves for all platforms, but not
compiled for a Windows x64 target.

| Crate | Version | Licence | Kind | Description (from the crate's own manifest) |
|---|---|---|---|---|
| `ab_glyph` | `0.2.32` | `Apache-2.0` | other platform | API for loading, scaling, positioning and rasterizing OpenType font glyphs. |
| `ab_glyph_rasterizer` | `0.1.10` | `Apache-2.0` | other platform | Coverage rasterization for lines, quadratic & cubic beziers |
| `accesskit` | `0.25.2` | `MIT OR Apache-2.0` | runtime | UI accessibility infrastructure across platforms |
| `accesskit_atspi_common` | `0.21.1` | `MIT OR Apache-2.0` | other platform | AccessKit UI accessibility infrastructure: core AT-SPI translation layer |
| `accesskit_consumer` | `0.39.2` | `MIT OR Apache-2.0` | runtime | AccessKit consumer library (internal) |
| `accesskit_ios` | `0.2.2` | `MIT OR Apache-2.0` | other platform | AccessKit UI accessibility infrastructure: iOS adapter |
| `accesskit_macos` | `0.27.2` | `MIT OR Apache-2.0` | other platform | AccessKit UI accessibility infrastructure: macOS adapter |
| `accesskit_unix` | `0.24.1` | `MIT OR Apache-2.0` | other platform | AccessKit UI accessibility infrastructure: Linux adapter |
| `accesskit_windows` | `0.35.2` | `MIT OR Apache-2.0` | runtime | AccessKit UI accessibility infrastructure: Windows adapter |
| `accesskit_winit` | `0.34.2` | `Apache-2.0` | runtime | AccessKit UI accessibility infrastructure: winit adapter |
| `adler2` | `2.0.1` | `0BSD OR MIT OR Apache-2.0` | runtime | A simple clean-room implementation of the Adler-32 checksum |
| `ahash` | `0.8.12` | `MIT OR Apache-2.0` | runtime | A non-cryptographic hash function using AES-NI for high performance |
| `aho-corasick` | `1.1.5` | `Unlicense OR MIT` | runtime | Fast multiple substring searching. |
| `allocator-api2` | `0.2.21` | `MIT OR Apache-2.0` | runtime | Mirror of Rust's allocator API |
| `android_system_properties` | `0.1.6` | `MIT OR Apache-2.0` | other platform | Minimal Android system properties wrapper |
| `android-activity` | `0.6.1` | `MIT OR Apache-2.0` | other platform | Glue for building Rust applications on Android with NativeActivity or GameActivity |
| `android-properties` | `0.2.2` | `MIT` | other platform | Rust-based Android properties wrapper |
| `anstream` | `1.0.0` | `MIT OR Apache-2.0` | runtime | IO stream adapters for writing colored text that will gracefully degrade according to your terminal's capabilities. |
| `anstyle` | `1.0.14` | `MIT OR Apache-2.0` | runtime | ANSI text styling |
| `anstyle-parse` | `1.0.0` | `MIT OR Apache-2.0` | runtime | Parse ANSI Style Escapes |
| `anstyle-query` | `1.1.5` | `MIT OR Apache-2.0` | runtime | Look up colored console capabilities |
| `anstyle-wincon` | `3.0.11` | `MIT OR Apache-2.0` | runtime | Styling legacy Windows terminals |
| `anyhow` | `1.0.104` | `MIT OR Apache-2.0` | runtime (direct) | Flexible concrete Error type built on std::error::Error |
| `arboard` | `3.7.0` | `MIT OR Apache-2.0` | runtime | Image and text handling for the OS clipboard. |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | other platform | Macros to take array references of slices |
| `arrayvec` | `0.7.8` | `MIT OR Apache-2.0` | runtime | A vector with fixed capacity, backed by an array (it can be stored on the stack too). Implements fixed capacity ArrayVec and ArrayString. |
| `ash` | `0.38.0+1.3.281` | `MIT OR Apache-2.0` | runtime | Vulkan bindings for Rust |
| `as-raw-xcb-connection` | `1.0.1` | `MIT OR Apache-2.0` | other platform | Trait to facilitate interoperatibility with libxcb C API |
| `async-broadcast` | `0.7.2` | `MIT OR Apache-2.0` | other platform | Async broadcast channels |
| `async-channel` | `2.5.0` | `Apache-2.0 OR MIT` | other platform | Async multi-producer multi-consumer channel |
| `async-executor` | `1.14.0` | `Apache-2.0 OR MIT` | other platform | Async executor |
| `async-io` | `2.6.0` | `Apache-2.0 OR MIT` | other platform | Async I/O and timers |
| `async-lock` | `3.4.2` | `Apache-2.0 OR MIT` | other platform | Async synchronization primitives |
| `async-process` | `2.5.0` | `Apache-2.0 OR MIT` | other platform | Async interface for working with processes |
| `async-recursion` | `1.2.0` | `MIT OR Apache-2.0` | other platform | Recursion for async functions |
| `async-signal` | `0.2.14` | `Apache-2.0 OR MIT` | other platform | Async signal handling |
| `async-task` | `4.7.1` | `Apache-2.0 OR MIT` | other platform | Task abstraction for building executors |
| `async-trait` | `0.1.92` | `MIT OR Apache-2.0` | other platform | Type erasure for async trait methods |
| `atomic-waker` | `1.1.2` | `Apache-2.0 OR MIT` | other platform | A synchronization primitive for task wakeup |
| `atspi` | `0.29.0` | `Apache-2.0 OR MIT` | other platform | Pure-Rust, zbus-based AT-SPI2 protocol implementation. |
| `atspi-common` | `0.13.0` | `Apache-2.0 OR MIT` | other platform | Primitive types used for sending and receiving Linux accessibility events. |
| `atspi-proxies` | `0.13.0` | `Apache-2.0 OR MIT` | other platform | AT-SPI2 proxies to query or manipulate UI objects |
| `autocfg` | `1.5.1` | `Apache-2.0 OR MIT` | other platform | Automatic cfg for Rust compiler features |
| `bitflags` | `1.3.2` | `MIT/Apache-2.0` | other platform | A macro to generate structures which behave like bitflags. |
| `bitflags` | `2.13.2` | `MIT OR Apache-2.0` | runtime | A macro to generate structures which behave like bitflags. |
| `bit-set` | `0.10.0` | `Apache-2.0 OR MIT` | runtime | A set of bits |
| `bit-vec` | `0.9.1` | `Apache-2.0 OR MIT` | runtime | A vector of bits |
| `block2` | `0.5.1` | `MIT` | other platform | Apple's C language extension of blocks |
| `block2` | `0.6.2` | `MIT` | other platform | Apple's C language extension of blocks |
| `block-buffer` | `0.10.4` | `MIT OR Apache-2.0` | runtime | Buffer type for block processing of data |
| `blocking` | `1.7.0` | `Apache-2.0 OR MIT` | other platform | A thread pool for isolating blocking I/O in async programs |
| `bumpalo` | `3.20.3` | `MIT OR Apache-2.0` | other platform | A fast bump allocation arena for Rust. |
| `bytemuck` | `1.25.2` | `Zlib OR Apache-2.0 OR MIT` | runtime | A crate for mucking around with piles of bytes. |
| `bytemuck_derive` | `1.12.1` | `Zlib OR Apache-2.0 OR MIT` | proc-macro | derive proc-macros for `bytemuck` |
| `byteorder-lite` | `0.1.0` | `Unlicense OR MIT` | runtime | Library for reading/writing numbers in big-endian and little-endian. |
| `bytes` | `1.12.1` | `MIT` | other platform | Types and traits for working with bytes |
| `calloop` | `0.13.0` | `MIT` | other platform | A callback-based event loop |
| `calloop` | `0.14.5` | `MIT` | other platform | A callback-based event loop |
| `calloop-wayland-source` | `0.3.0` | `MIT` | other platform | A wayland-rs client event source for callloop |
| `calloop-wayland-source` | `0.4.1` | `MIT` | other platform | A wayland-rs client event source for callloop |
| `cc` | `1.10.0` | `MIT OR Apache-2.0` | other platform | A build-time dependency for Cargo build scripts to assist in invoking the native C compiler to compile native C code into a static archive to be linked into Rust code. |
| `cfg_aliases` | `0.2.2` | `MIT` | other platform | A tiny utility to help save you a lot of effort with long winded `#[cfg()]` checks. |
| `cfg-if` | `1.0.5` | `MIT OR Apache-2.0` | runtime | A macro to ergonomically define an item depending on a large number of #[cfg] parameters. Structured like an if-else chain, the first matching branch is the item that gets emitted. |
| `cgl` | `0.3.2` | `MIT / Apache-2.0` | other platform | Rust bindings for CGL on Mac |
| `clap` | `4.6.7` | `MIT OR Apache-2.0` | runtime (direct) | A simple to use, efficient, and full-featured Command Line Argument Parser |
| `clap_builder` | `4.6.7` | `MIT OR Apache-2.0` | runtime | A simple to use, efficient, and full-featured Command Line Argument Parser |
| `clap_derive` | `4.6.7` | `MIT OR Apache-2.0` | proc-macro | Parse command line argument by defining a struct, derive crate. |
| `clap_lex` | `1.1.1` | `MIT OR Apache-2.0` | runtime | Minimal, flexible command line parser |
| `clipboard-win` | `5.4.3` | `BSL-1.0` | runtime | Provides simple way to interact with Windows clipboard. |
| `codespan-reporting` | `0.13.1` | `Apache-2.0` | runtime | Beautiful diagnostic reporting for text-based programming languages |
| `color` | `0.3.3` | `Apache-2.0 OR MIT` | runtime | A library for representing and manipulating colors |
| `colorchoice` | `1.0.5` | `MIT OR Apache-2.0` | runtime | Global override of color control |
| `combine` | `4.6.9` | `MIT` | other platform | Fast parser combinators on arbitrary streams with zero-copy support. |
| `concurrent-queue` | `2.5.0` | `Apache-2.0 OR MIT` | other platform | Concurrent multi-producer multi-consumer queue |
| `core-foundation` | `0.9.4` | `MIT OR Apache-2.0` | other platform | Bindings to Core Foundation for macOS |
| `core-foundation-sys` | `0.8.7` | `MIT OR Apache-2.0` | other platform | Bindings to Core Foundation for macOS |
| `core-graphics` | `0.23.2` | `MIT OR Apache-2.0` | other platform | Bindings to Core Graphics for macOS |
| `core-graphics-types` | `0.1.3` | `MIT OR Apache-2.0` | other platform | Bindings for some fundamental Core Graphics types |
| `cpufeatures` | `0.2.17` | `MIT OR Apache-2.0` | runtime | Lightweight runtime CPU feature detection for aarch64, loongarch64, and x86/x86_64 targets,  with no_std support and support for mobile targets including Android and iOS |
| `crc32fast` | `1.5.3` | `MIT OR Apache-2.0` | runtime | Fast, SIMD-accelerated CRC32 (IEEE) checksum computation |
| `crossbeam-deque` | `0.8.8` | `MIT OR Apache-2.0` | runtime | Concurrent work-stealing deque |
| `crossbeam-epoch` | `0.9.21` | `MIT OR Apache-2.0` | runtime | Epoch-based garbage collection |
| `crossbeam-utils` | `0.8.23` | `MIT OR Apache-2.0` | runtime | Utilities for concurrent programming |
| `crunchy` | `0.2.4` | `MIT` | other platform | Crunchy unroller: deterministically unroll constant loops |
| `crypto-common` | `0.1.7` | `MIT OR Apache-2.0` | runtime | Common cryptographic traits |
| `cursor-icon` | `1.2.0` | `MIT OR Apache-2.0 OR Zlib` | runtime | Cross platform cursor icon type |
| `defmt` | `1.2.0` | `MIT OR Apache-2.0` | other platform | A highly efficient logging framework that targets resource-constrained devices, like microcontrollers |
| `defmt-macros` | `1.2.0` | `MIT OR Apache-2.0` | other platform | defmt macros |
| `defmt-parser` | `1.1.0` | `MIT OR Apache-2.0` | other platform | Parsing library for defmt format strings |
| `digest` | `0.10.7` | `MIT OR Apache-2.0` | runtime | Traits for cryptographic hash functions and message authentication codes |
| `dispatch` | `0.2.0` | `MIT` | other platform | Rust wrapper for Apple's Grand Central Dispatch. |
| `dispatch2` | `0.3.1` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings and wrappers for Apple's Grand Central Dispatch (GCD) |
| `displaydoc` | `0.2.7` | `MIT OR Apache-2.0` | proc-macro | A derive macro for implementing the display Trait via a doc comment and string interpolation |
| `dlib` | `0.5.3` | `MIT` | other platform | Helper macros for handling manually loading optional system libraries. |
| `document-features` | `0.2.12` | `MIT OR Apache-2.0` | proc-macro | Extract documentation for the feature flags from comments in Cargo.toml |
| `downcast-rs` | `1.2.1` | `MIT/Apache-2.0` | other platform | Trait object downcasting support using only safe Rust. It supports type parameters, associated types, and type constraints. |
| `dpi` | `0.1.2` | `Apache-2.0 AND MIT` | runtime | Types for handling UI scaling |
| `ecolor` | `0.37.0` | `MIT OR Apache-2.0` | runtime | Color structs and color conversion utilities |
| `eframe` | `0.37.0` | `MIT OR Apache-2.0` | runtime (direct) | egui framework - write GUI apps that compiles to web and/or natively |
| `egui` | `0.37.0` | `MIT OR Apache-2.0` | runtime | An easy-to-use immediate mode GUI that runs on both web and native |
| `egui_glow` | `0.37.0` | `MIT OR Apache-2.0` | other platform | Bindings for using egui natively using the glow library |
| `egui_system_font_provider` | `0.37.0` | `MIT OR Apache-2.0` | runtime | Load system fonts on demand for characters the egui fonts lack |
| `egui-wgpu` | `0.37.0` | `MIT OR Apache-2.0` | runtime | Bindings for using egui natively using the wgpu library |
| `egui-winit` | `0.37.0` | `MIT OR Apache-2.0` | runtime | Bindings for using egui with winit |
| `either` | `1.19.0` | `MIT OR Apache-2.0` | runtime | The enum `Either` with variants `Left` and `Right` is a general purpose sum type with two cases. |
| `emath` | `0.37.0` | `MIT OR Apache-2.0` | runtime | Minimal 2D math library for GUI work |
| `endi` | `1.1.1` | `MIT` | other platform | A simple endian-handling library |
| `enumflags2` | `0.7.12` | `MIT OR Apache-2.0` | other platform | Enum-based bit flags |
| `enumflags2_derive` | `0.7.12` | `MIT OR Apache-2.0` | other platform | Do not use directly, use the reexport in the `enumflags2` crate. This allows for better compatibility across versions. |
| `env_filter` | `2.0.0` | `MIT OR Apache-2.0` | runtime | Filter log events using environment variables |
| `env_logger` | `0.11.11` | `MIT OR Apache-2.0` | runtime (direct) | A logging implementation for `log` which is configured via an environment variable. |
| `epaint` | `0.37.0` | `MIT OR Apache-2.0` | runtime | Minimal 2D graphics library for GUI work |
| `epaint_default_fonts` | `0.37.0` | `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` | runtime | Default fonts for use in epaint / egui |
| `equivalent` | `1.0.3` | `Apache-2.0 OR MIT` | runtime | Traits for key comparison in maps. |
| `errno` | `0.3.14` | `MIT OR Apache-2.0` | other platform | Cross-platform interface to the `errno` variable. |
| `error-code` | `3.4.0` | `BSL-1.0` | runtime | Error code |
| `euclid` | `0.22.14` | `MIT OR Apache-2.0` | runtime | Geometry primitives |
| `event-listener` | `5.4.2` | `Apache-2.0 OR MIT` | other platform | Notify async tasks or threads |
| `event-listener-strategy` | `0.5.4` | `Apache-2.0 OR MIT` | other platform | Block or poll on event_listener easily |
| `extended` | `0.1.0` | `MIT` | runtime | Extended precision 80-bit floating-point numbers (f80). |
| `fastrand` | `2.5.0` | `Apache-2.0 OR MIT` | other platform | A simple and fast random number generator |
| `fax` | `0.2.7` | `MIT` | other platform | Decoder and Encoder for CCITT Group 3 and 4 bi-level image encodings used by fax machines TIFF and PDF. |
| `fdeflate` | `0.3.7` | `MIT OR Apache-2.0` | runtime | Fast specialized deflate implementation |
| `fearless_simd` | `0.4.1` | `Apache-2.0 OR MIT` | runtime | Safer and easier SIMD |
| `find-msvc-tools` | `0.1.15` | `MIT OR Apache-2.0` | other platform | Find windows-specific tools, read MSVC versions from the registry and from COM interfaces |
| `fixedbitset` | `0.5.7` | `MIT OR Apache-2.0` | other platform | FixedBitSet is a simple bitset collection |
| `flate2` | `1.1.10` | `MIT OR Apache-2.0` | runtime | DEFLATE compression and decompression exposed as Read/BufRead/Write streams. Supports miniz_oxide and multiple zlib implementations. Supports zlib, gzip, and raw deflate streams. |
| `foldhash` | `0.1.5` | `Zlib` | other platform | A fast, non-cryptographic, minimally DoS-resistant hashing algorithm. |
| `foldhash` | `0.2.0` | `Zlib` | runtime | A fast, non-cryptographic, minimally DoS-resistant hashing algorithm. |
| `fontique` | `0.11.1` | `Apache-2.0 OR MIT` | runtime | Font enumeration and fallback. |
| `font-types` | `0.12.7` | `MIT OR Apache-2.0` | runtime | Scalar types used in fonts. |
| `foreign-types` | `0.5.0` | `MIT/Apache-2.0` | other platform | A framework for Rust wrappers over C APIs |
| `foreign-types-macros` | `0.2.4` | `MIT/Apache-2.0` | other platform | An internal crate used by foreign-types |
| `foreign-types-shared` | `0.3.1` | `MIT/Apache-2.0` | other platform | An internal crate used by foreign-types |
| `form_urlencoded` | `1.2.2` | `MIT OR Apache-2.0` | runtime | Parser and serializer for the application/x-www-form-urlencoded syntax, as used by HTML forms. |
| `futures-core` | `0.3.34` | `MIT OR Apache-2.0` | other platform | The core traits and types in for the `futures` library. |
| `futures-io` | `0.3.34` | `MIT OR Apache-2.0` | other platform | The `AsyncRead`, `AsyncWrite`, `AsyncSeek`, and `AsyncBufRead` traits for the futures-rs library. |
| `futures-lite` | `2.6.1` | `Apache-2.0 OR MIT` | other platform | Futures, streams, and async I/O combinators |
| `futures-macro` | `0.3.34` | `MIT OR Apache-2.0` | other platform | The futures-rs procedural macro implementations. |
| `futures-task` | `0.3.34` | `MIT OR Apache-2.0` | other platform | Tools for working with tasks. |
| `futures-util` | `0.3.34` | `MIT OR Apache-2.0` | other platform | Common utilities and extension traits for the futures-rs library. |
| `generic-array` | `0.14.7` | `MIT` | runtime | Generic types implementing functionality of arrays |
| `gethostname` | `1.1.0` | `Apache-2.0` | other platform | gethostname for all platforms |
| `getrandom` | `0.3.4` | `MIT OR Apache-2.0` | other platform | A small cross-platform library for retrieving random data from system source |
| `getrandom` | `0.4.3` | `MIT OR Apache-2.0` | other platform | A small cross-platform library for retrieving random data from system source |
| `gl_generator` | `0.14.0` | `Apache-2.0` | other platform | Code generators for creating bindings to the Khronos OpenGL APIs. |
| `glifo` | `0.3.0` | `Apache-2.0 OR MIT` | runtime | Glifo provides APIs for efficiently rendering text. |
| `glow` | `0.17.0` | `MIT OR Apache-2.0 OR Zlib` | runtime | GL on Whatever: a set of bindings to run GL (Open GL, OpenGL ES, and WebGL) anywhere, and avoid target-specific code. |
| `glutin` | `0.32.3` | `Apache-2.0` | other platform | Cross-platform OpenGL context provider. |
| `glutin_egl_sys` | `0.7.1` | `Apache-2.0` | other platform | The egl bindings for glutin |
| `glutin_glx_sys` | `0.6.1` | `Apache-2.0` | other platform | The glx bindings for glutin |
| `glutin_wgl_sys` | `0.6.1` | `Apache-2.0` | runtime | The wgl bindings for glutin |
| `glutin-winit` | `0.5.0` | `MIT` | other platform | Glutin bootstrapping helpers with winit |
| `gpu-allocator` | `0.28.0` | `MIT OR Apache-2.0` | runtime | Memory allocator for GPU memory in Vulkan and DirectX 12 |
| `guillotiere` | `0.7.0` | `MIT/Apache-2.0` | runtime | A dynamic 2D texture atlas allocator with fast deallocation. |
| `half` | `2.7.1` | `MIT OR Apache-2.0` | runtime | Half-precision floating point f16 and bf16 types for Rust implementing the IEEE 754-2008 standard binary16 and bfloat16 types. |
| `harfrust` | `0.12.0` | `MIT` | runtime | A complete HarfBuzz shaping algorithm port to Rust. |
| `hashbrown` | `0.15.5` | `MIT OR Apache-2.0` | other platform | A Rust port of Google's SwissTable hash map |
| `hashbrown` | `0.16.1` | `MIT OR Apache-2.0` | runtime | A Rust port of Google's SwissTable hash map |
| `hashbrown` | `0.17.1` | `MIT OR Apache-2.0` | runtime | A Rust port of Google's SwissTable hash map |
| `heck` | `0.5.0` | `MIT OR Apache-2.0` | runtime | heck is a case conversion library. |
| `hermit-abi` | `0.5.3` | `MIT OR Apache-2.0` | other platform | Hermit system calls definitions. |
| `hex` | `0.4.3` | `MIT OR Apache-2.0` | other platform | Encoding and decoding data into/from hexadecimal representation. |
| `hound` | `3.5.1` | `Apache-2.0` | runtime (direct) | A wav encoding and decoding library |
| `icu_collections` | `2.4.0` | `Unicode-3.0` | runtime | Collection of API for use in ICU libraries. |
| `icu_locale_core` | `2.4.0` | `Unicode-3.0` | runtime | API for managing Unicode Language and Locale Identifiers |
| `icu_normalizer` | `2.4.0` | `Unicode-3.0` | runtime | API for normalizing text into Unicode Normalization Forms |
| `icu_normalizer_data` | `2.4.0` | `Unicode-3.0` | runtime | Data for the icu_normalizer crate |
| `icu_properties` | `2.4.0` | `Unicode-3.0` | runtime | Definitions for Unicode properties |
| `icu_properties_data` | `2.4.0` | `Unicode-3.0` | runtime | Data for the icu_properties crate |
| `icu_provider` | `2.4.0` | `Unicode-3.0` | runtime | Trait and struct definitions for the ICU data provider |
| `idna` | `1.1.0` | `MIT OR Apache-2.0` | runtime | IDNA (Internationalizing Domain Names in Applications) and Punycode. |
| `idna_adapter` | `1.2.2` | `Apache-2.0 OR MIT` | runtime | Back end adapter for idna |
| `image` | `0.25.10` | `MIT OR Apache-2.0` | runtime | Imaging library. Provides basic image processing and encoders/decoders for common image formats. |
| `indexmap` | `2.14.2` | `Apache-2.0 OR MIT` | runtime | A hash table with consistent order and fast iteration. |
| `is_terminal_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | runtime | Polyfill for `is_terminal` stdlib feature for use with older MSRVs |
| `itertools` | `0.15.0` | `MIT OR Apache-2.0` | runtime | Extra iterator adaptors, iterator methods, free functions, and macros. |
| `itoa` | `1.0.18` | `MIT OR Apache-2.0` | runtime | Fast integer primitive to string conversion |
| `jiff` | `0.2.38` | `Unlicense OR MIT` | runtime | A date-time library that encourages you to jump into the pit of success.  This library is heavily inspired by the Temporal project. |
| `jiff-core` | `0.1.1` | `Unlicense OR MIT` | runtime | Low level datetime primitives for the Jiff library. |
| `jiff-static` | `0.2.38` | `Unlicense OR MIT` | other platform | Create static TimeZone values for Jiff (useful in core-only environments). |
| `jni` | `0.22.4` | `MIT OR Apache-2.0` | other platform | Rust bindings to the JNI |
| `jni-macros` | `0.22.4` | `MIT OR Apache-2.0` | other platform | Procedural macros for the jni crate |
| `jni-sys` | `0.3.1` | `MIT OR Apache-2.0` | other platform | Rust definitions corresponding to jni.h |
| `jni-sys` | `0.4.1` | `MIT OR Apache-2.0` | other platform | Rust definitions corresponding to jni.h |
| `jni-sys-macros` | `0.4.1` | `MIT OR Apache-2.0` | other platform | Macros for jni-sys crate |
| `jobserver` | `0.1.35` | `MIT OR Apache-2.0` | other platform | An implementation of the GNU Make jobserver for Rust. |
| `js-sys` | `0.3.106` | `MIT OR Apache-2.0` | other platform | Bindings for all JS global objects and functions in all JS environments like Node.js and browsers, built on `#[wasm_bindgen]` using the `wasm-bindgen` crate. |
| `khronos_api` | `3.1.0` | `Apache-2.0` | other platform | The Khronos XML API Registry, exposed as byte string constants. |
| `khronos-egl` | `6.0.0` | `MIT/Apache-2.0` | runtime | Rust bindings for EGL |
| `kurbo` | `0.13.1` | `Apache-2.0 OR MIT` | runtime | A 2D curves library |
| `lazy_static` | `1.5.1` | `MIT OR Apache-2.0` | runtime | A macro for declaring lazily evaluated statics in Rust. |
| `libc` | `0.2.191` | `MIT OR Apache-2.0` | runtime | Raw FFI bindings to platform libraries like libc. |
| `libloading` | `0.8.9` | `ISC` | runtime | Bindings around the platform's dynamic library loading primitives with greatly improved memory safety. |
| `libm` | `0.2.16` | `MIT` | runtime | libm in pure Rust |
| `libredox` | `0.1.25` | `MIT` | other platform | Redox stable ABI |
| `linebender_resource_handle` | `0.1.1` | `Apache-2.0 OR MIT` | runtime | Raw resource handle for immutable blobs with dynamic lifetimes. |
| `linux-raw-sys` | `0.12.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | other platform | Generated bindings for Linux's userspace API |
| `linux-raw-sys` | `0.4.15` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | other platform | Generated bindings for Linux's userspace API |
| `litemap` | `0.8.3` | `Unicode-3.0` | runtime | A key-value Map implementation based on a flat, sorted Vec. |
| `litrs` | `1.0.1` | `MIT OR Apache-2.0` | runtime | Parse and inspect Rust literals (i.e. tokens in the Rust programming language representing fixed values). Particularly useful for proc macros, but can also be used outside of a proc-macro context. |
| `lock_api` | `0.4.14` | `MIT OR Apache-2.0` | runtime | Wrappers to create fully-featured Mutex and RwLock types. Compatible with no_std. |
| `log` | `0.4.34` | `MIT OR Apache-2.0` | runtime (direct) | A lightweight logging facade for Rust |
| `memchr` | `2.8.3` | `Unlicense OR MIT` | runtime | Provides extremely fast (uses SIMD on x86_64, aarch64 and wasm32) routines for 1, 2 or 3 byte search and single substring search. |
| `memmap2` | `0.9.12` | `MIT OR Apache-2.0` | runtime | Cross-platform Rust API for memory-mapped file IO |
| `memoffset` | `0.9.1` | `MIT` | other platform | offset_of functionality for Rust structs. |
| `miniz_oxide` | `0.8.9` | `MIT OR Zlib OR Apache-2.0` | runtime | DEFLATE compression and decompression library rewritten in Rust based on miniz |
| `miniz_oxide` | `0.9.1` | `MIT OR Zlib OR Apache-2.0` | runtime | DEFLATE compression and decompression library rewritten in Rust based on miniz |
| `moxcms` | `0.8.1` | `BSD-3-Clause OR Apache-2.0` | runtime | Simple Color Management in Rust |
| `naga` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Shader translator and validator. Part of the wgpu project |
| `naga-types` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Common types for naga and wgpu |
| `ndk` | `0.9.0` | `MIT OR Apache-2.0` | other platform | Safe Rust bindings to the Android NDK |
| `ndk-context` | `0.1.1` | `MIT OR Apache-2.0` | other platform | Handles for accessing Android APIs |
| `ndk-sys` | `0.6.0+11769913` | `MIT OR Apache-2.0` | other platform | FFI bindings for the Android NDK |
| `nohash-hasher` | `0.2.0` | `Apache-2.0 OR MIT` | runtime | An implementation of `std::hash::Hasher` which does not hash at all. |
| `nom` | `8.0.0` | `MIT` | other platform | A byte-oriented, zero-copy, parser combinators library |
| `num_enum` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | other platform | Procedural macros to make inter-operation between primitives and enums easier. |
| `num_enum_derive` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | other platform | Internal implementation details for ::num_enum (Procedural macros to make inter-operation between primitives and enums easier) |
| `num-complex` | `0.4.6` | `MIT OR Apache-2.0` | runtime | Complex numbers implementation for Rust |
| `num-integer` | `0.1.47` | `MIT OR Apache-2.0` | runtime | Integer traits and functions |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | runtime | Numeric traits for generic mathematics |
| `objc2` | `0.5.3` | `MIT` | other platform | Objective-C interface and runtime bindings |
| `objc2` | `0.6.5` | `MIT` | other platform | Objective-C interface and runtime bindings |
| `objc2-app-kit` | `0.2.2` | `MIT` | other platform | Bindings to the AppKit framework |
| `objc2-app-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the AppKit framework |
| `objc2-cloud-kit` | `0.2.2` | `MIT` | other platform | Bindings to the CloudKit framework |
| `objc2-contacts` | `0.2.2` | `MIT` | other platform | Bindings to the Contacts framework |
| `objc2-core-data` | `0.2.2` | `MIT` | other platform | Bindings to the CoreData framework |
| `objc2-core-foundation` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the CoreFoundation framework |
| `objc2-core-graphics` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the CoreGraphics framework |
| `objc2-core-image` | `0.2.2` | `MIT` | other platform | Bindings to the CoreImage framework |
| `objc2-core-location` | `0.2.2` | `MIT` | other platform | Bindings to the CoreLocation framework |
| `objc2-core-text` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the CoreText framework |
| `objc2-encode` | `4.1.0` | `MIT` | other platform | Objective-C type-encoding representation and parsing |
| `objc2-foundation` | `0.2.2` | `MIT` | other platform | Bindings to the Foundation framework |
| `objc2-foundation` | `0.3.2` | `MIT` | other platform | Bindings to the Foundation framework |
| `objc2-io-surface` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the IOSurface framework |
| `objc2-link-presentation` | `0.2.2` | `MIT` | other platform | Bindings to the LinkPresentation framework |
| `objc2-metal` | `0.2.2` | `MIT` | other platform | Bindings to the Metal framework |
| `objc2-metal` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the Metal framework |
| `objc2-quartz-core` | `0.2.2` | `MIT` | other platform | Bindings to the QuartzCore/CoreAnimation framework |
| `objc2-quartz-core` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the QuartzCore/CoreAnimation framework |
| `objc2-symbols` | `0.2.2` | `MIT` | other platform | Bindings to the Symbols framework |
| `objc2-ui-kit` | `0.2.2` | `MIT` | other platform | Bindings to the UIKit framework |
| `objc2-ui-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | other platform | Bindings to the UIKit framework |
| `objc2-uniform-type-identifiers` | `0.2.2` | `MIT` | other platform | Bindings to the UniformTypeIdentifiers framework |
| `objc2-user-notifications` | `0.2.2` | `MIT` | other platform | Bindings to the UserNotifications framework |
| `objc-sys` | `0.3.5` | `MIT` | other platform | Raw bindings to the Objective-C runtime and ABI |
| `once_cell` | `1.21.4` | `MIT OR Apache-2.0` | runtime | Single assignment cells and lazy values. |
| `once_cell_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | runtime | Polyfill for `OnceCell` stdlib feature for use with older MSRVs |
| `orbclient` | `0.3.55` | `MIT` | other platform | The Orbital Client Library |
| `ordered-float` | `5.5.1` | `MIT` | runtime | Wrappers for total ordering on floats |
| `ordered-stream` | `0.2.0` | `MIT OR Apache-2.0` | other platform | Streams that are ordered relative to external events |
| `os_pipe` | `1.2.3` | `MIT` | other platform | a cross-platform library for opening OS pipes |
| `owned_ttf_parser` | `0.25.1` | `Apache-2.0` | other platform | ttf-parser plus support for owned data |
| `parking` | `2.2.1` | `Apache-2.0 OR MIT` | other platform | Thread parking and unparking |
| `parking_lot` | `0.12.5` | `MIT OR Apache-2.0` | runtime | More compact and efficient implementations of the standard synchronization primitives. |
| `parking_lot_core` | `0.9.12` | `MIT OR Apache-2.0` | runtime | An advanced API for creating custom synchronization primitives. |
| `parlance` | `0.1.1` | `Apache-2.0 OR MIT` | runtime | Fundamental text property types |
| `peniko` | `0.6.1` | `Apache-2.0 OR MIT` | runtime | Primitive types for styling vector graphics |
| `percent-encoding` | `2.3.2` | `MIT OR Apache-2.0` | runtime | Percent encoding and decoding |
| `petgraph` | `0.8.3` | `MIT OR Apache-2.0` | other platform | Graph data structure library. Provides graph types and graph algorithms. |
| `phf` | `0.14.0` | `MIT` | other platform | Runtime support for perfect hash function data structures |
| `phf_generator` | `0.14.0` | `MIT` | other platform | PHF generation logic |
| `phf_macros` | `0.14.0` | `MIT` | other platform | Macros to generate types in the phf crate |
| `phf_shared` | `0.14.0` | `MIT` | other platform | Support code shared by PHF libraries |
| `pin-project` | `1.1.13` | `Apache-2.0 OR MIT` | other platform | A crate for safe and ergonomic pin-projection. |
| `pin-project-internal` | `1.1.13` | `Apache-2.0 OR MIT` | other platform | Implementation detail of the `pin-project` crate. |
| `pin-project-lite` | `0.2.17` | `Apache-2.0 OR MIT` | runtime | A lightweight version of pin-project written with declarative macros. |
| `piper` | `0.2.5` | `MIT OR Apache-2.0` | other platform | An asynchronous single-consumer single-producer pipe for bytes. |
| `pkg-config` | `0.3.34` | `MIT OR Apache-2.0` | other platform | A library to run the pkg-config system tool at build time in order to be used in Cargo build scripts. |
| `plain` | `0.2.3` | `MIT/Apache-2.0` | other platform | A small Rust library that allows users to reinterpret data of certain types safely. |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | runtime | PNG decoding and encoding library in pure Rust |
| `polling` | `3.11.0` | `Apache-2.0 OR MIT` | other platform | Portable interface to epoll, kqueue, event ports, and IOCP |
| `poll-promise` | `0.3.0` | `MIT OR Apache-2.0` | runtime | Poll the result of an async operation in a game or immediate mode GUI. |
| `pollster` | `0.4.0` | `Apache-2.0/MIT` | other platform | Synchronously block the thread until a future completes |
| `pollster` | `1.0.1` | `Apache-2.0/MIT` | runtime | Synchronously block the thread until a future completes |
| `polycool` | `0.4.0` | `MIT OR Apache-2.0` | runtime | Polynomial root-finding |
| `portable-atomic` | `1.15.0` | `Apache-2.0 OR MIT` | other platform | Portable atomic types including support for 128-bit atomics, atomic float, etc. |
| `portable-atomic-util` | `0.2.8` | `Apache-2.0 OR MIT` | other platform | Synchronization primitives built with portable-atomic. |
| `potential_utf` | `0.1.6` | `Unicode-3.0` | runtime | Unvalidated string and character types |
| `presser` | `0.3.1` | `MIT OR Apache-2.0` | runtime | A crate to help you copy things into raw buffers without invoking spooky action at a distance (undefined behavior). |
| `primal-check` | `0.3.4` | `MIT OR Apache-2.0` | runtime | Fast standalone primality testing. |
| `proc-macro2` | `1.0.107` | `MIT OR Apache-2.0` | runtime | A substitute implementation of the compiler's `proc_macro` API to decouple token-based libraries from the procedural macro use case. |
| `proc-macro-crate` | `3.5.0` | `MIT OR Apache-2.0` | other platform | Replacement for crate (macro_rules keyword) in proc-macros |
| `profiling` | `1.0.18` | `MIT OR Apache-2.0` | runtime | This crate provides a very thin abstraction over other profiler crates. |
| `pxfm` | `0.1.30` | `BSD-3-Clause OR Apache-2.0` | runtime | Fast and accurate math |
| `quick-error` | `2.0.1` | `MIT/Apache-2.0` | other platform | A macro which makes error types pleasant to write. |
| `quick-xml` | `0.41.0` | `MIT` | other platform | High performance xml reader and writer |
| `quote` | `1.0.47` | `MIT OR Apache-2.0` | runtime | Quasi-quoting macro quote!(...) |
| `range-alloc` | `0.1.5` | `MIT OR Apache-2.0` | runtime | Generic range allocator |
| `raw-window-handle` | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` | runtime | Interoperability library for Rust Windowing applications. |
| `raw-window-metal` | `1.1.0` | `MIT OR Apache-2.0` | other platform | Interop library between Metal and raw-window-handle |
| `rayon` | `1.12.0` | `MIT OR Apache-2.0` | runtime (direct) | Simple work-stealing parallelism for Rust |
| `rayon-core` | `1.13.0` | `MIT OR Apache-2.0` | runtime | Core APIs for Rayon |
| `read-fonts` | `0.41.0` | `MIT OR Apache-2.0` | runtime | Reading OpenType font files. |
| `redox_syscall` | `0.4.1` | `MIT` | other platform | A Rust library to access raw Redox system calls |
| `redox_syscall` | `0.5.18` | `MIT` | other platform | A Rust library to access raw Redox system calls |
| `redox_syscall` | `0.9.5` | `MIT` | other platform | A Rust library to access raw Redox system calls |
| `r-efi` | `5.3.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | other platform | UEFI Reference Specification Protocol Constants and Definitions |
| `r-efi` | `6.0.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | other platform | UEFI Reference Specification Protocol Constants and Definitions |
| `regex` | `1.13.1` | `MIT OR Apache-2.0` | runtime | An implementation of regular expressions for Rust. This implementation uses finite automata and guarantees linear time matching on all inputs. |
| `regex-automata` | `0.4.18` | `MIT OR Apache-2.0` | runtime | Automata construction and matching using regular expressions. |
| `regex-lite` | `0.1.9` | `MIT OR Apache-2.0` | runtime | A lightweight regex engine that optimizes for binary size and compilation time. |
| `regex-syntax` | `0.8.11` | `MIT OR Apache-2.0` | runtime | A regular expression parser. |
| `renderdoc-sys` | `1.1.0` | `MIT OR Apache-2.0` | runtime | Low-level bindings to the RenderDoc API |
| `rfd` | `0.17.2` | `MIT` | runtime (direct) | Rusty File Dialog |
| `roxmltree` | `0.21.2` | `MIT OR Apache-2.0` | other platform | Represent an XML as a read-only tree. |
| `rustc_version` | `0.4.1` | `MIT OR Apache-2.0` | other platform | A library for querying the version of a installed rustc compiler |
| `rustc-hash` | `1.1.0` | `Apache-2.0/MIT` | runtime | speed, non-cryptographic hash used in rustc |
| `rustc-hash` | `2.1.3` | `Apache-2.0 OR MIT` | runtime | A speedy, non-cryptographic hashing algorithm used by rustc |
| `rustfft` | `6.4.1` | `MIT OR Apache-2.0` | runtime (direct) | High-performance FFT library written in pure Rust. |
| `rustix` | `0.38.44` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | other platform | Safe Rust bindings to POSIX/Unix/Linux/Winsock-like syscalls |
| `rustix` | `1.1.6` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | other platform | Safe Rust bindings to POSIX/Unix/Linux/Winsock-like syscalls |
| `rustversion` | `1.0.23` | `MIT OR Apache-2.0` | other platform | Conditional compilation according to rustc compiler version |
| `same-file` | `1.0.6` | `Unlicense/MIT` | other platform | A simple crate for determining whether two file paths point to the same file. |
| `scoped-tls` | `1.0.1` | `MIT/Apache-2.0` | other platform | Library implementation of the standard library's old `scoped_thread_local!` macro for providing scoped access to thread local storage (TLS) so any type can be stored into TLS. |
| `scopeguard` | `1.2.0` | `MIT OR Apache-2.0` | runtime | A RAII scope guard that will run a given closure when it goes out of scope, even if the code between panics (assuming unwinding panic).  Defines the macros `defer!`, `defer_on_unwind!`, `defer_on_success!` as shorthands for guards with one of the implemented strategies. |
| `sctk-adwaita` | `0.10.1` | `MIT` | other platform | Adwaita-like SCTK Frame |
| `self_cell` | `1.3.0` | `Apache-2.0 OR GPL-2.0-only` | runtime | Safe-to-use proc-macro-free self-referential structs in stable Rust. |
| `semver` | `1.0.28` | `MIT OR Apache-2.0` | other platform | Parser and evaluator for Cargo's flavor of Semantic Versioning |
| `send_wrapper` | `0.6.0` | `MIT/Apache-2.0` | other platform | This Rust library implements a wrapper type called SendWrapper which allows you to move around non-Send types between threads, as long as you access the contained value only from within the original thread. You also have to make sure that the wrapper is dropped from within the original thread. If any of these constraints is violated, a panic occurs. |
| `serde` | `1.0.229` | `MIT OR Apache-2.0` | runtime (direct) | A generic serialization/deserialization framework |
| `serde_core` | `1.0.229` | `MIT OR Apache-2.0` | runtime | Serde traits only, with no support for derive -- use the `serde` crate instead |
| `serde_derive` | `1.0.229` | `MIT OR Apache-2.0` | proc-macro | Macros 1.1 implementation of #[derive(Serialize, Deserialize)] |
| `serde_json` | `1.0.154` | `MIT OR Apache-2.0` | runtime (direct) | A JSON serialization file format |
| `serde_repr` | `0.1.21` | `MIT OR Apache-2.0` | other platform | Derive Serialize and Deserialize that delegates to the underlying repr of a C-like enum. |
| `sha2` | `0.10.9` | `MIT OR Apache-2.0` | runtime (direct) | Pure Rust implementation of the SHA-2 hash function family including SHA-224, SHA-256, SHA-384, and SHA-512. |
| `shlex` | `2.0.1` | `MIT OR Apache-2.0` | other platform | Split a string into shell words, like Python's shlex. |
| `signal-hook-registry` | `1.4.8` | `MIT OR Apache-2.0` | other platform | Backend crate for signal-hook |
| `simd_cesu8` | `1.2.0` | `Apache-2.0 OR MIT` | other platform | An extremely fast, SIMD accelerated, encoding and decoding library for CESU-8 and Modified UTF-8. |
| `simd-adler32` | `0.3.10` | `MIT` | runtime | A SIMD-accelerated Adler-32 hash algorithm implementation. |
| `simdutf8` | `0.1.5` | `MIT OR Apache-2.0` | other platform | SIMD-accelerated UTF-8 validation. |
| `siphasher` | `1.0.4` | `MIT OR Apache-2.0` | other platform | SipHash-2-4 and SipHash-1-3 keyed hash functions in pure Rust, with 64-bit and 128-bit output and no_std support |
| `skrifa` | `0.44.0` | `MIT OR Apache-2.0` | runtime | Metadata reader and glyph scaler for OpenType fonts. |
| `slab` | `0.4.12` | `MIT` | other platform | Pre-allocated storage for a uniform data type |
| `slotmap` | `1.1.1` | `Zlib` | other platform | Slotmap data structure |
| `smallvec` | `1.16.4` | `MIT OR Apache-2.0` | runtime | 'Small vector' optimization: store up to a small number of items on the stack |
| `smithay-client-toolkit` | `0.19.2` | `MIT` | other platform | Toolkit for making client wayland applications. |
| `smithay-client-toolkit` | `0.20.0` | `MIT` | other platform | Toolkit for making client wayland applications. |
| `smithay-clipboard` | `0.7.3` | `MIT` | other platform | Provides access to the wayland clipboard for client applications. |
| `smol_str` | `0.2.2` | `MIT OR Apache-2.0` | runtime | small-string optimized string type with O(1) clone |
| `spirv` | `0.4.0+sdk-1.4.341.0` | `Apache-2.0` | runtime | Rust definition of SPIR-V structs and enums |
| `stable_deref_trait` | `1.2.1` | `MIT OR Apache-2.0` | runtime | An unsafe marker trait for types like Box and Rc that dereference to a stable address even when moved, and hence can be used with libraries such as owning_ref and rental. |
| `static_assertions` | `1.1.0` | `MIT OR Apache-2.0` | runtime | Compile-time assertions to ensure that invariants are met. |
| `strength_reduce` | `0.2.4` | `MIT OR Apache-2.0` | runtime | Faster integer division and modulus operations |
| `strict-num` | `0.1.1` | `MIT` | other platform | A collection of bounded numeric types |
| `strsim` | `0.11.1` | `MIT` | runtime | Implementations of string similarity metrics. Includes Hamming, Levenshtein, OSA, Damerau-Levenshtein, Jaro, Jaro-Winkler, and Sørensen-Dice. |
| `symphonia` | `0.6.1` | `MPL-2.0` | runtime (direct) | Pure Rust media container and audio decoding library. |
| `symphonia-bundle-flac` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust FLAC demuxer and decoder (a part of project Symphonia). |
| `symphonia-codec-aac` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust AAC decoder (a part of project Symphonia). |
| `symphonia-codec-adpcm` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust ADPCM audio decoder (a part of project Symphonia). |
| `symphonia-codec-pcm` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust PCM audio decoder (a part of project Symphonia). |
| `symphonia-codec-vorbis` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust Vorbis decoder (a part of project Symphonia). |
| `symphonia-common` | `0.6.1` | `MPL-2.0` | runtime | Project Symphonia utilities for codecs and formats. |
| `symphonia-core` | `0.6.1` | `MPL-2.0` | runtime | Project Symphonia shared structs, traits, and features. |
| `symphonia-format-isomp4` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust ISO/MP4 demuxer (a part of project Symphonia). |
| `symphonia-format-mkv` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust MKV/WebM demuxer (a part of project Symphonia). |
| `symphonia-format-ogg` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust OGG demuxer (a part of project Symphonia). |
| `symphonia-format-riff` | `0.6.1` | `MPL-2.0` | runtime | Pure Rust RIFF demuxer (a part of project Symphonia). |
| `symphonia-metadata` | `0.6.1` | `MPL-2.0` | runtime | Project Symphonia multimedia tag and metadata readers. |
| `syn` | `2.0.119` | `MIT OR Apache-2.0` | runtime | Parser for Rust source code |
| `syn` | `3.0.9` | `MIT OR Apache-2.0` | runtime | Parser for Rust source code |
| `synstructure` | `0.14.0` | `MIT` | runtime | Helper methods and macros for custom derives |
| `sys-locale` | `0.3.2` | `MIT OR Apache-2.0` | runtime | Small and lightweight library to obtain the active system locale |
| `tempfile` | `3.27.0` | `MIT OR Apache-2.0` | test-only (direct) | A library for managing temporary files and directories. |
| `termcolor` | `1.4.1` | `Unlicense OR MIT` | other platform | A simple cross platform library for writing colored text to a terminal. |
| `thiserror` | `1.0.69` | `MIT OR Apache-2.0` | other platform | derive(Error) |
| `thiserror` | `2.0.21` | `MIT OR Apache-2.0` | runtime (direct) | derive(Error) |
| `thiserror-impl` | `1.0.69` | `MIT OR Apache-2.0` | other platform | Implementation detail of the `thiserror` crate |
| `thiserror-impl` | `2.0.21` | `MIT OR Apache-2.0` | proc-macro | Implementation detail of the `thiserror` crate |
| `tiff` | `0.11.3` | `MIT` | other platform | TIFF decoding and encoding library in pure Rust |
| `tiny-skia` | `0.11.4` | `BSD-3-Clause` | other platform | A tiny Skia subset ported to Rust. |
| `tiny-skia-path` | `0.11.4` | `BSD-3-Clause` | other platform | A tiny-skia Bezier path implementation |
| `tinystr` | `0.8.5` | `Unicode-3.0` | runtime | A small ASCII-only bounded length string representation. |
| `tokio` | `1.54.1` | `MIT` | other platform | An event-driven, non-blocking I/O platform for writing asynchronous I/O backed applications. |
| `toml_datetime` | `1.1.2+spec-1.1.0` | `MIT OR Apache-2.0` | other platform | A TOML-compatible datetime type |
| `toml_edit` | `0.25.17+spec-1.1.0` | `MIT OR Apache-2.0` | other platform | Yet another format-preserving TOML parser. |
| `toml_parser` | `1.1.5+spec-1.1.0` | `MIT OR Apache-2.0` | other platform | Yet another format-preserving TOML parser. |
| `tracing` | `0.1.44` | `MIT` | runtime | Application-level tracing for Rust. |
| `tracing-attributes` | `0.1.31` | `MIT` | other platform | Procedural macro attributes for automatically instrumenting functions. |
| `tracing-core` | `0.1.36` | `MIT` | runtime | Core primitives for application-level tracing. |
| `transpose` | `0.2.3` | `MIT OR Apache-2.0` | runtime | Utility for transposing multi-dimensional data |
| `tree_magic_mini` | `3.2.2` | `MIT` | other platform | Determines the MIME type of a file by traversing a filetype tree. |
| `ttf-parser` | `0.25.1` | `MIT OR Apache-2.0` | other platform | A high-level, safe, zero-allocation font parser for TrueType, OpenType, and AAT. |
| `type-map` | `0.5.1` | `MIT/Apache-2.0` | runtime | Provides a typemap container with FxHashMap |
| `typenum` | `1.20.1` | `MIT OR Apache-2.0` | runtime | Typenum is a Rust library for type-level numbers evaluated at     compile time. It currently supports bits, unsigned integers, and signed     integers. It also provides a type-level array of type-level numbers, but its     implementation is incomplete. |
| `uds_windows` | `1.2.1` | `MIT` | other platform | Unix Domain Sockets for Windows! |
| `unicode-general-category` | `1.1.0` | `Apache-2.0` | runtime | Fast lookup of the Unicode General Category property for char |
| `unicode-ident` | `1.0.27` | `(MIT OR Apache-2.0) AND Unicode-3.0` | runtime | Determine whether characters have the XID_Start or XID_Continue properties according to Unicode Standard Annex #31 |
| `unicode-properties` | `0.1.4` | `MIT/Apache-2.0` | runtime | Query character Unicode properties according to UAX #44 and UTR #51. |
| `unicode-script` | `0.5.8` | `MIT OR Apache-2.0` | runtime | This crate exposes the Unicode `Script` and `Script_Extension` properties from [UAX #24](http://www.unicode.org/reports/tr24/) |
| `unicode-segmentation` | `1.13.3` | `MIT OR Apache-2.0` | runtime | This crate provides Grapheme Cluster, Word and Sentence boundaries according to Unicode Standard Annex #29 rules. |
| `unicode-width` | `0.2.3` | `MIT OR Apache-2.0` | runtime | Determine displayed width of `char` and `str` types according to Unicode Standard Annex #11 rules. |
| `url` | `2.5.8` | `MIT OR Apache-2.0` | runtime | URL library for Rust, based on the WHATWG URL Standard |
| `utf8_iter` | `1.0.4` | `Apache-2.0 OR MIT` | runtime | Iterator by char over potentially-invalid UTF-8 in &[u8] |
| `utf8parse` | `0.2.2` | `Apache-2.0 OR MIT` | runtime | Table-driven UTF-8 parser |
| `uuid` | `1.29.0` | `Apache-2.0 OR MIT` | runtime | A library to generate and parse UUIDs. |
| `vello_common` | `0.2.0` | `Apache-2.0 OR MIT` | runtime | Core data structures and utilities shared across the Vello rendering, including geometry processing and tiling logic. |
| `vello_cpu` | `0.2.0` | `Apache-2.0 OR MIT` | runtime | A CPU-based renderer for Vello, optimized for SIMD and multithreaded execution. |
| `version_check` | `0.9.5` | `MIT/Apache-2.0` | other platform | Tiny crate to check the version of the installed/running rustc. |
| `walkdir` | `2.5.0` | `Unlicense/MIT` | other platform | Recursively walk a directory. |
| `wasip2` | `1.0.4+wasi-0.2.12` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | other platform | WASIp2 API bindings for Rust |
| `wasm-bindgen` | `0.2.129` | `MIT OR Apache-2.0` | other platform | Easy support for interacting between JS and Rust. |
| `wasm-bindgen-futures` | `0.4.79` | `MIT OR Apache-2.0` | other platform | Bridging the gap between Rust Futures and JavaScript Promises |
| `wasm-bindgen-macro` | `0.2.129` | `MIT OR Apache-2.0` | other platform | Definition of the `#[wasm_bindgen]` attribute, an internal dependency |
| `wasm-bindgen-macro-support` | `0.2.129` | `MIT OR Apache-2.0` | other platform | Implementation APIs for the `#[wasm_bindgen]` attribute |
| `wasm-bindgen-shared` | `0.2.129` | `MIT OR Apache-2.0` | other platform | Shared support between wasm-bindgen and wasm-bindgen cli, an internal dependency. |
| `wayland-backend` | `0.3.17` | `MIT` | other platform | Low-level bindings to the Wayland protocol |
| `wayland-client` | `0.31.15` | `MIT` | other platform | Bindings to the standard C implementation of the wayland protocol, client side. |
| `wayland-csd-frame` | `0.3.0` | `MIT` | other platform | Common trait and types for wayland CSD interop |
| `wayland-cursor` | `0.31.14` | `MIT` | other platform | Bindings to libwayland-cursor. |
| `wayland-protocols` | `0.32.13` | `MIT` | other platform | Generated API for the officials wayland protocol extensions |
| `wayland-protocols-experimental` | `20250721.0.1` | `MIT` | other platform | Generated API for experimental wayland protocol extensions |
| `wayland-protocols-misc` | `0.3.12` | `MIT` | other platform | Generated API for misc and deprecated wayland protocol extensions |
| `wayland-protocols-plasma` | `0.3.12` | `MIT` | other platform | Generated API for the Plasma wayland protocol extensions |
| `wayland-protocols-wlr` | `0.3.12` | `MIT` | other platform | Generated API for the WLR wayland protocol extensions |
| `wayland-scanner` | `0.31.11` | `MIT` | other platform | Wayland Scanner for generating rust APIs from XML wayland protocol files. |
| `wayland-sys` | `0.31.11` | `MIT` | other platform | FFI bindings to the various libwayland-*.so libraries. You should only need this crate if you are working on custom wayland protocol extensions. Look at the crate wayland-client for usable bindings. |
| `webbrowser` | `1.2.5` | `MIT OR Apache-2.0` | runtime | Open URLs in web browsers available on a platform |
| `web-sys` | `0.3.106` | `MIT OR Apache-2.0` | other platform | Bindings for all Web APIs, a procedurally generated crate from WebIDL |
| `web-time` | `1.1.0` | `MIT OR Apache-2.0` | runtime | Drop-in replacement for std::time for Wasm in browsers |
| `weezl` | `0.1.12` | `MIT OR Apache-2.0` | other platform | Fast LZW compression and decompression. |
| `wgpu` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Cross-platform, safe, pure-rust graphics API |
| `wgpu-core` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Core implementation logic of wgpu, the cross-platform, safe, pure-rust graphics API |
| `wgpu-core-deps-apple` | `30.0.1` | `MIT OR Apache-2.0` | other platform | Feature unification helper crate for Apple platforms |
| `wgpu-core-deps-emscripten` | `30.0.1` | `MIT OR Apache-2.0` | other platform | Feature unification helper crate for the Emscripten platform |
| `wgpu-core-deps-wasm` | `30.0.1` | `MIT OR Apache-2.0` | other platform | Feature unification helper crate for the WebAssembly platform |
| `wgpu-core-deps-windows-linux-android` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Feature unification helper crate for the Windows/Linux/Android platforms |
| `wgpu-hal` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Hardware abstraction layer for wgpu, the cross-platform, safe, pure-rust graphics API |
| `wgpu-naga-bridge` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Conversions between naga and wgpu-types. Part of the wgpu project |
| `wgpu-types` | `30.0.1` | `MIT OR Apache-2.0` | runtime | Common types and utilities for wgpu, the cross-platform, safe, pure-rust graphics API |
| `winapi-util` | `0.1.11` | `Unlicense OR MIT` | other platform | A dumping ground for high level safe wrappers over windows-sys. |
| `windows` | `0.62.2` | `MIT OR Apache-2.0` | runtime | Rust for Windows |
| `windows_aarch64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | other platform | Import lib for Windows |
| `windows_aarch64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | other platform | Import lib for Windows |
| `windows_i686_gnu` | `0.52.6` | `MIT OR Apache-2.0` | other platform | Import lib for Windows |
| `windows_i686_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | other platform | Import lib for Windows |
| `windows_i686_msvc` | `0.52.6` | `MIT OR Apache-2.0` | other platform | Import lib for Windows |
| `windows_x86_64_gnu` | `0.52.6` | `MIT OR Apache-2.0` | other platform | Import lib for Windows |
| `windows_x86_64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | other platform | Import lib for Windows |
| `windows_x86_64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | runtime | Import lib for Windows |
| `windows-collections` | `0.3.2` | `MIT OR Apache-2.0` | runtime | Windows collection types |
| `windows-core` | `0.62.2` | `MIT OR Apache-2.0` | runtime | Core type support for COM and Windows |
| `windows-future` | `0.3.2` | `MIT OR Apache-2.0` | runtime | Windows async types |
| `windows-implement` | `0.60.2` | `MIT OR Apache-2.0` | proc-macro | The implement macro for the Windows crates |
| `windows-interface` | `0.59.3` | `MIT OR Apache-2.0` | proc-macro | The interface macro for the Windows crates |
| `windows-link` | `0.2.1` | `MIT OR Apache-2.0` | runtime | Linking for Windows |
| `windows-numerics` | `0.3.1` | `MIT OR Apache-2.0` | runtime | Windows numeric types |
| `windows-result` | `0.4.1` | `MIT OR Apache-2.0` | runtime | Windows error handling |
| `windows-strings` | `0.5.1` | `MIT OR Apache-2.0` | runtime | Windows string types |
| `windows-sys` | `0.52.0` | `MIT OR Apache-2.0` | runtime | Rust for Windows |
| `windows-sys` | `0.59.0` | `MIT OR Apache-2.0` | other platform | Rust for Windows |
| `windows-sys` | `0.61.2` | `MIT OR Apache-2.0` | runtime | Rust for Windows |
| `windows-targets` | `0.52.6` | `MIT OR Apache-2.0` | runtime | Import libs for Windows |
| `windows-threading` | `0.2.1` | `MIT OR Apache-2.0` | runtime | Windows threading |
| `winit` | `0.30.13` | `Apache-2.0` | runtime | Cross-platform window creation library. |
| `winnow` | `1.0.4` | `MIT` | other platform | A byte-oriented, zero-copy, parser combinators library |
| `wit-bindgen` | `0.57.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | other platform | Rust bindings generator and runtime support for WIT and the component model. Used when compiling Rust programs to the component model. |
| `wl-clipboard-rs` | `0.9.5` | `MIT OR Apache-2.0` | other platform | Access to the Wayland clipboard for terminal and other window-less applications. |
| `writeable` | `0.6.4` | `Unicode-3.0` | runtime | A more efficient alternative to fmt::Display |
| `x11-dl` | `2.21.0` | `MIT` | other platform | X11 library bindings for Rust |
| `x11rb` | `0.13.2` | `MIT OR Apache-2.0` | other platform | Rust bindings to X11 |
| `x11rb-protocol` | `0.13.2` | `MIT OR Apache-2.0` | other platform | Rust bindings to X11 |
| `xcursor` | `0.3.11` | `MIT` | other platform | A library for loading XCursor themes |
| `xkbcommon-dl` | `0.4.2` | `MIT` | other platform | Dynamically loaded xkbcommon and xkbcommon-x11 Rust bindings. |
| `xkeysym` | `0.2.1` | `MIT OR Apache-2.0 OR Zlib` | other platform | A library for working with X11 keysyms |
| `xml-rs` | `0.8.29` | `MIT` | other platform | An XML library in pure Rust |
| `yeslogic-fontconfig-sys` | `6.0.1` | `MIT` | other platform | Raw bindings to Fontconfig without a vendored C library |
| `yoke` | `0.8.4` | `Unicode-3.0` | runtime | Abstraction allowing borrowed data to be carried along with the backing data it borrows from |
| `yoke-derive` | `0.8.4` | `Unicode-3.0` | proc-macro | Custom derive for the yoke crate |
| `zbus` | `5.19.0` | `MIT` | other platform | API for D-Bus communication |
| `zbus_macros` | `5.19.0` | `MIT` | other platform | proc-macros for zbus |
| `zbus_names` | `4.3.5` | `MIT` | other platform | A collection of D-Bus bus names types (deprecated: merged into zbus) |
| `zbus_xml` | `5.2.1` | `MIT` | other platform | API to handle D-Bus introspection XML |
| `zbus-lockstep` | `0.5.2` | `MIT` | other platform | Keep types in lockstep with DBus XML definitions |
| `zbus-lockstep-macros` | `0.5.2` | `MIT` | other platform | Macros to keep types in lockstep with DBus XML definitions |
| `zcheapstr` | `1.1.0` | `MIT` | other platform | An immutable, cheaply cloneable string type with borrowed, static and shared variants |
| `zerocopy` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | runtime | Zerocopy makes zero-cost memory manipulation effortless. We write "unsafe" so you don't have to. |
| `zerocopy-derive` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | proc-macro | Custom derive for traits from the zerocopy crate |
| `zerofrom` | `0.1.8` | `Unicode-3.0` | runtime | ZeroFrom trait for constructing |
| `zerofrom-derive` | `0.1.8` | `Unicode-3.0` | proc-macro | Custom derive for the zerofrom crate |
| `zerotrie` | `0.2.5` | `Unicode-3.0` | runtime | A data structure that efficiently maps strings to integers |
| `zerovec` | `0.11.9` | `Unicode-3.0` | runtime | Zero-copy vector backed by a byte array |
| `zerovec-derive` | `0.11.7` | `Unicode-3.0` | proc-macro | Custom derive for the zerovec crate |
| `zlib-rs` | `0.6.8` | `Zlib` | other platform | A memory-safe zlib implementation written in rust |
| `zmij` | `1.0.23` | `MIT` | runtime | A double-to-string conversion algorithm based on Schubfach and xjb |
| `zune-core` | `0.5.3` | `MIT OR Apache-2.0 OR Zlib` | other platform | Core utilities for image processing in the zune family of crates |
| `zune-jpeg` | `0.5.15` | `MIT OR Apache-2.0 OR Zlib` | other platform | A fast, correct and safe jpeg decoder |
| `zvariant` | `5.15.1` | `MIT` | other platform | D-Bus & GVariant encoding & decoding (deprecated: merged into zbus) |
| `zvariant_derive` | `5.15.1` | `MIT` | other platform | D-Bus & GVariant encoding & decoding (deprecated: merged into zbus) |
| `zvariant_utils` | `4.2.0` | `MIT` | other platform | Various utilities used internally by the zvariant crate. |

### 3.5 A note on the "description" column

For the transitive crates the description is the crate author's own one-line summary
from the crate's `Cargo.toml`, reproduced verbatim. It describes what the crate *is*,
not what StageSync uses it for. StageSync's own use of each crate is limited to the
direct dependencies in §3.2; the transitive crates are pulled in by those and are
used in whatever way their own upstream design dictates.

---

## 4. Dependency-licence compatibility

### 4.1 StageSync's own licence

StageSync is released under **MIT OR Apache-2.0** — a dual licence. This is declared
in `Cargo.toml` as `license = "MIT OR Apache-2.0"` at the workspace level, and is
inherited by all three crates. The full texts are in `LICENSE-MIT` and
`LICENSE-APACHE`; `LICENSES.md` explains how to choose between them.

The dual licence lets a recipient pick whichever of the two permissive licences suits
them. A commercial user with an Apache-2.0 policy can take Apache-2.0; everyone else
can take MIT. Neither choice imposes copyleft on StageSync's own code.

### 4.2 Why every dependency's licence is compatible

Every dependency in `Cargo.lock` is under a licence that permits StageSync to be
redistributed under MIT or Apache-2.0. The families that appear, and the argument for
each:

| Licence family | Packages | Why it is compatible |
|---|---|---|
| MIT, Apache-2.0, dual MIT/Apache-2.0 (both orderings, plus the `/` and `,` spellings some manifests use) | The large majority | Permissive. Requires only that the copyright notice and licence text travel with the source and that the licence text appears in binary distributions. StageSync satisfies both, and §4.3 explains why the non-MIT/Apache cases below are compatible too. |
| BSD-2-Clause, BSD-3-Clause, ISC, 0BSD, Zlib, BSL-1.0, Unlicense | `zerocopy` / `zerocopy-derive`, `tiny-skia` / `tiny-skia-path`, `moxcms`, `pxfm`, `num_enum` / `num_enum_derive`, `arrayref`, `libloading`, `adler2`, `clipboard-win`, `error-code`, `dispatch2`, `bytemuck` / `bytemuck_derive`, `foldhash` (×2), `slotmap`, `zlib-rs`, `jiff` / `jiff-core` / `jiff-static`, `memchr`, `aho-corasick`, `byteorder-lite`, `termcolor`, `winapi-util`, `same-file`, `walkdir`, and the `objc2-*` family | Permissive, in the same family as MIT. The only substantive condition is the notice-retention clause, which StageSync's third-party notices satisfy. |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | `linux-raw-sys` (×2), `rustix` (×2), `wasip2`, `wit-bindgen` | The LLVM exception removes the copyleft effect of Apache-2.0 for the covered files, leaving a permissive licence with a patent grant. Take the MIT or plain-Apache-2.0 option. |
| Unicode-3.0 (alone, or together with MIT/Apache-2.0) | `icu_collections`, `icu_locale_core`, `icu_normalizer`, `icu_normalizer_data`, `icu_properties`, `icu_properties_data`, `icu_provider`, `litemap`, `potential_utf`, `tinystr`, `unicode-ident`, `writeable`, `yoke`, `yoke-derive`, `zerofrom`, `zerofrom-derive`, `zerotrie`, `zerovec`, `zerovec-derive` | An OSI-approved permissive licence with a warranty disclaimer and a notice-retention condition. `unicode-ident` additionally declares `(MIT OR Apache-2.0) AND Unicode-3.0`. Compatible. |
| OFL-1.1 and Ubuntu-font-1.0 (in combination with MIT/Apache-2.0) | `epaint_default_fonts` | Font-licence conditions. They require that the font files keep their copyright and patent notices, that any derivative of the font keep the same licence, and that the fonts are not sold on their own. StageSync embeds these fonts as unmodified data inside the GUI and does not modify or redistribute them separately, so the conditions are met. |
| LGPL-2.1-or-later | `r-efi` (both 5.3.0 and 6.0.0) | These crates are declared `MIT OR Apache-2.0 OR LGPL-2.1-or-later`. StageSync takes the MIT/Apache-2.0 option, so the LGPL terms do not apply to them. |
| Apache-2.0 OR GPL-2.0-only | `self_cell` 1.3.0 | StageSync takes the Apache-2.0 option; the GPL option is not used. |
| Apache-2.0 AND MIT | `dpi` 0.1.2 | Both licences apply and both are permissive, so both notices must be reproduced. Compatible. |
| **MPL-2.0** | `symphonia` 0.6.1 and all twelve `symphonia-*` sub-crates | Weak, **file-level** copyleft. See below. |

### 4.3 The one dependency that needs attention: symphonia (MPL-2.0)

`symphonia` 0.6.1 and its sub-crates (`symphonia-core`, `symphonia-common`,
`symphonia-metadata`, `symphonia-codec-pcm`, `symphonia-codec-aac`,
`symphonia-codec-adpcm`, `symphonia-codec-vorbis`, `symphonia-bundle-flac`,
`symphonia-format-riff`, `symphonia-format-isomp4`, `symphonia-format-mkv`,
`symphonia-format-ogg`) are licensed **MPL-2.0**.

MPL-2.0 is a file-level copyleft licence. Its effect is narrow and is worth stating
precisely:

- The **MPL-covered files stay under MPL-2.0.** They may be combined with files under
  other terms — StageSync's own Rust code and its MIT/Apache-2.0 licence — and the
  combined work may be distributed under those other terms. MPL-2.0 does **not** reach
  StageSync's own source files.
- A recipient of a binary that contains the MPL-covered code must be able to obtain
  the **Corresponding Source** of those MPL-covered files. In practice: ship the
  source of the `symphonia` crates, or point recipients at the exact published
  versions on crates.io.
- If StageSync ever **modified** an MPL-covered file, that modified file would have
  to be published under MPL-2.0. StageSync does not modify `symphonia`; it is used as
  an unmodified dependency. (StageSync's own patch set against it, if any were ever
  needed, would be a separate matter to assess.)
- The licence notices must be kept intact.

Because Rust links statically by default, the `symphonia` object code is compiled
*into* `stagesync.exe` and `stagesync-desktop.exe` rather than shipped as a separate
DLL. This does not change the obligation — the source of the MPL-covered files still
has to be made available to recipients — but it does mean there is no "symphonia.dll"
to point at when writing the notices. `LICENSES.md` §4 sets out the file-level
copyleft implication in detail, including what a redistributor of a Windows binary
should actually do, and `LICENSES.md` §5 gives the checklist.

### 4.4 Summary

Every dependency is under a permissive licence or a weak, file-level copyleft licence
whose copyleft does not extend to StageSync's own code. StageSync's dual
MIT/Apache-2.0 licence is compatible with all of them. The single obligation that
needs active attention when shipping a binary — making the `symphonia` source
available and keeping the MPL notices — is documented in `LICENSES.md`.

---

## 5. The parts that are original

The following are hand-written for this project, with **no third-party code behind
them** — no library implements these, and none was consulted to write them:

1. **The SVG drift-graph writer** (`crates/stagesync-core/src/export/graph.rs`).
   This was written **by hand specifically to avoid taking a plotting dependency** —
   a plotting library would have been the obvious choice, and was rejected to keep
   the dependency footprint of the project small, to keep the output deterministic
   byte-for-byte, and because the same data is re-drawn interactively by the desktop
   application. It emits plain SVG text.
2. **The desktop chart painters** (`crates/stagesync-desktop/src/chart.rs`).
   The lag and residual plots are drawn directly by `egui` painting code: "no
   plotting library is used", so that the axes and their units stay explicit.
3. **The reference-timeline painter** (`crates/stagesync-desktop/src/timeline.rs`).
   A span of the reference that no recorder captured is drawn as an empty, hatched
   band — never as continuous material.
4. **The polyphase Kaiser-windowed-sinc resampler**
   (`crates/stagesync-core/src/resample/mod.rs`), including the Kaiser window and the
   I₀ Bessel function used by it. The alternatives named in
   `cleanroom/PUBLIC_DSP_REFERENCES.md` §8 — `libsamplerate` (GPL-2), SoX's
   resamplers, `rubato` — were **not** used.
5. **The robust regression estimators** (`crates/stagesync-core/src/matching/regression.rs`):
   Theil–Sen, RANSAC, and MAD-based outlier rejection, written from the published
   methods.
6. **The coarse global lock** (`crates/stagesync-core/src/matching/coarse.rs`) and
   **the fine probes** (`crates/stagesync-core/src/matching/fine.rs`).
7. **The RMS-envelope feature and its local z-score**
   (`crates/stagesync-core/src/features/envelope.rs`) and **the log-mel front end**
   (`crates/stagesync-core/src/features/mel.rs`).
8. **The correlation primitives, including GCC-PHAT**
   (`crates/stagesync-core/src/matching/dsp.rs`).
9. **The segmentation and coverage model**
   (`crates/stagesync-core/src/matching/segments.rs`) and **the calibration loop**
   (`crates/stagesync-core/src/calibration.rs`).
10. **The rendering layouts** (`crates/stagesync-core/src/render/mod.rs`).
11. **The export formats and the deterministic sync-map / run-report structures**
    (`crates/stagesync-core/src/export/mod.rs`).
12. **The command-line interface** (`crates/stagesync-cli/src/main.rs`) and the
    **desktop application** (`crates/stagesync-desktop/`), including the background
    job runner.
13. **The Python tooling** (`tools/make_fixtures.py`, `tools/verify_exports.py`) —
    written for this project, and deliberately an *independent* implementation rather
    than a re-use of the Rust code, so that verification is not the estimator
    confirming itself.

Third-party code **is** used underneath some of these, and it is worth being precise
about where. The FFT in the log-mel front end, the correlation primitives and the
GCC-PHAT verifier comes from `rustfft`. The WAV container codec comes from `hound`.
The SHA-256 primitive comes from `sha2`. Argument parsing comes from `clap`. The
window and widget toolkit come from `eframe`/`egui`, and the file dialogs from `rfd`.
None of these libraries supplies the algorithms StageSync is about.

---

## 6. How to verify this claim

A reviewer can check the provenance trail without taking anyone's word for it. Here
is the procedure, in order.

### 6.1 Check the dependency list against the lock file

```text
cargo tree --locked --offline --target x86_64-pc-windows-msvc --edges normal --prefix none --workspace --all-features
```

The result must agree with §3.4 of this document and with `LICENSES.md` Appendix A.
The exact versions are pinned in `Cargo.lock`; running `cargo tree` without
`--locked` would be a mistake, because it may resolve newer versions.

To reproduce the licence column, read the `license` field from each dependency's own
manifest. For any crate already in the local cargo registry cache, the manifest is at:

```text
%USERPROFILE%\.cargo\registry\src\<registry-hash>\<crate>-<version>\Cargo.toml
```

The same information is published at `https://crates.io/crates/<crate>` and in the
crate's upstream repository.

To confirm the build/runtime split, run `cargo tree` with `--edges normal`,
`--edges build` and `--edges dev` separately. On this target the `build` edge is
empty and the `dev` edge contains only `tempfile`.

### 6.2 Check the licence files against the dependencies

- `LICENSE-MIT` is the standard MIT licence text with the copyright line
  `Copyright (c) 2026 The StageSync contributors`.
- `LICENSE-APACHE` is the Apache License 2.0 in full, including the appendix
  boilerplate. It is byte-for-byte the file published by the Apache Software
  Foundation at <https://www.apache.org/licenses/LICENSE-2.0.txt> (SHA-256
  `CFC7749B96F63BD31C3C42B5C471BF756814053E847C10F3EB003417BC523D30`), with
  line endings normalised to LF to match the rest of the repository. To check:

  ```text
  curl -sS https://www.apache.org/licenses/LICENSE-2.0.txt | sha256sum
  ```

  and compare against the hash above, after normalising line endings.
- `LICENSES.md` Appendix A lists every pinned package with its licence and where the
  licence text lives. Spot-check a sample: the thirteen `symphonia-*` crates must all
  read `MPL-2.0`, `hound` must read `Apache-2.0` (not the dual), and `rfd` must read
  `MIT` (not the dual).

### 6.3 Look for the absence of vendored binaries and decompiler output

- The repository contains no `.exe`, `.dll`, `.lib`, `.pdb`, `.a`, `.so` or other
  vendored executable binary anywhere outside `target/`. The `target/` directory is
  build output and is excluded by `.gitignore`, as are `*.exe`, `*.dll`, `*.pdb`,
  `*.rlib`, `*.rmeta`, `__pycache__/` and `*.pyc`. Recorded media is excluded too:
  `audio/`, `*.wav`, `*.m4a` and the rest of the media extensions are ignored, and
  `work/` (scratch output) is ignored in full. The `*.wav` files in `testdata/` are
  therefore working material, not source-controlled fixtures.
- No dependency ships a third-party **runtime** binary that StageSync
  redistributes. A few crate packages contain link-time artefacts — the
  `windows_*` crates ship MSVC import libraries and GNU archives that stub out the
  Windows system libraries, and `libloading` ships two DLLs used by its own test
  suite — but none of these is linked into, or copied alongside, the StageSync
  executables.
- The `cleanroom/` documents contain no decompiled code, pseudocode, function
  addresses, internal engine identifiers or proprietary numeric constants. This was
  checked by the research side before handoff and is recorded in
  `cleanroom/PROVENANCE.md` §5; a reviewer can repeat it by grepping `cleanroom/`
  for decompiler-style function labels, hex address lists and internal schema names.
- The `cleanroom/` set refers to a private reverse-engineering workspace at
  `../private-research/`. **That directory is not part of this repository.** If a
  reviewer has access to the wider filesystem, its absence is itself checkable.
- The `cleanroom/` documents do name the reference product, and they do record the
  SHA-256 of the reference binary and of the reverse-engineering toolchain. That is
  provenance *about* the research, not content derived from it: no decompiled code,
  no internal constant and no weight or table is present.

### 6.4 Check that the implementation matches the specification

- `crates/stagesync-core/tests/dsp_tests.rs` pins the meaning of the library's
  conventions — which way a lag points, how a time map is applied, what the
  resampler does to amplitude and phase — and can be run with `cargo test`.
- `crates/stagesync-desktop/tests/workflow.rs` checks the parts of the desktop
  application that do not need a window, including that every artefact the window
  promises to produce is really written.
- `tools/verify_exports.py` independently re-measures exported artefacts using a
  separate implementation (NumPy/SciPy) rather than the Rust code, which is the
  strongest available check that the pipeline is not merely self-consistent.

### 6.5 Check that the UI was not cloned

Compare the StageSync CLI surface (`stagesync analyze | export | verify | info`) and
the desktop window layout against the reference product's own. The command set and
the screen structure were designed from `cleanroom/CLEANROOM_SPEC.md`; they are not
a copy of the reference product's menus, dialogs or shortcuts.

---

## 7. Known limitations of the clean-room claim

This section is deliberately candid. A clean-room claim is a claim about *process*,
and a process cannot be fully audited from an artefact alone.

1. **The repository can show what is absent, not what was never read.** This
   repository contains no decompiled code, no proprietary source, no vendored binary
   and no reverse-engineered table. That is verifiable. It is **not** verifiable from
   the repository that no such material was ever *opened* by the person writing the
   code. The clean-room boundary is a matter of process and of trust in the people
   who ran it, and the repository is evidence for it, not proof of it.
2. **The upstream research engagement did include reverse engineering.** The
   `cleanroom/` documents describe a research workspace that decompiled the reference
   product with Ghidra, and `cleanroom/PROVENANCE.md` §3 enumerates the classes of
   fact that were derived that way and were deliberately kept out of the clean-room
   set. StageSync contains none of that material, but the existence of the upstream
   engagement means the claim is a statement about a *boundary*, not about a
   project that was never touched by reverse engineering.
3. **Some user-visible values originate in the reference product's shipped
   configuration.** Preset names, parameter names and parameter defaults were read
   from the preset files the reference product ships. These are user-level
   configuration data, not implementation, and the clean-room documents treat them as
   a legitimate input. They are nonetheless the only reference-product-derived values
   in the specification, and a reader comparing StageSync's defaults against the
   reference product's presets will find them similar by design.
4. **The published literature is a shared input.** The DSP methods StageSync
   implements — DTW-style alignment, GCC-PHAT, Theil–Sen, RANSAC, Crochiere–Rabiner
   resampling, Kaiser windows — are public. An independent implementer working from
   the same publications would produce a functionally similar tool. Convergent
   similarity is therefore expected and is not evidence of copying.
5. **The behaviour specification was written by someone who observed the reference
   product.** `cleanroom/CLEANROOM_SPEC.md` and `cleanroom/BEHAVIOR_TESTS.md` are
   black-box observations of the reference product's user-visible behaviour. The
   specification is therefore an accurate description of *what the product does*,
   and by construction a specification of observable behaviour carries some
   information about the original implementation's observable decisions — which is
   precisely why the clean-room method draws the line at user-visible behaviour and
   internals.
6. **Some licence facts are self-reported by upstream projects.** The licence
   identifiers in §3 and in `LICENSES.md` were read from each dependency's published
   `Cargo.toml`. They are authoritative for the versions pinned here, but they are
   upstream's own statements about its own code, not an independent legal audit.
7. **The Python tooling versions are not pinned.** As noted in §3.3, the NumPy and
   SciPy versions are not constrained by a lock file, and the fixture WAV files in
   `testdata/` are working material rather than version-pinned artefacts. They can
   be regenerated with `tools/make_fixtures.py`, which is the reproducible route.

---

## 8. Where to read more

| Document | What it covers |
|---|---|
| `LICENSES.md` | StageSync's own licence, a per-dependency licence table, plain-English notes on MIT / Apache-2.0 / MPL-2.0, the `symphonia` MPL-2.0 obligations, and the third-party notices checklist for anyone shipping a binary. |
| `LICENSE-MIT` | The MIT licence text. |
| `LICENSE-APACHE` | The Apache License 2.0 text, in full. |
| `cleanroom/CLEANROOM_SPEC.md` | The behaviour specification the implementation was written from. |
| `cleanroom/BEHAVIOR_TESTS.md` | The black-box validation protocol and the measurement definitions. |
| `cleanroom/PROVENANCE.md` | The research-side boundary audit: where each class of information in the clean-room set came from. |
| `cleanroom/PUBLIC_DSP_REFERENCES.md` | The published DSP literature the implementation draws on. |
| `Cargo.toml`, `Cargo.lock` | The authoritative dependency list and its exact resolved versions. |
