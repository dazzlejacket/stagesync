# Building StageSync

StageSync is a Rust workspace with three crates:

| Crate | Binary | What it is |
| --- | --- | --- |
| `crates/stagesync-core` | – | The library: audio ingest, features, matching, drift estimation, segmentation, rendering. |
| `crates/stagesync-cli` | `stagesync` | The command-line tool. |
| `crates/stagesync-desktop` | `stagesync-desktop` | The egui/eframe desktop application. |

Both binaries declare an explicit `[[bin]]` name and path, so
`cargo build --release --bin stagesync-desktop` produces
`target/release/stagesync-desktop` (with a `.exe` suffix on Windows).

---

## 1. Prerequisites

### The Rust toolchain

The workspace asks for **Rust 1.95** (`rust-version` in the root
`Cargo.toml`). Any newer stable toolchain works; `stable` is what CI uses.
The floor is set by the dependencies rather than chosen: `eframe` and `egui`
0.37 declare `rust-version = 1.95`, `wgpu` 30 needs 1.87, and `symphonia` 0.6
and `clap` 4.6 need 1.85. A manifest that claimed a lower floor would produce a
confusing refusal from Cargo rather than the real reason.

The two components the gates need are `rustfmt` and `clippy`:

```bash
rustup toolchain install stable --component rustfmt --component clippy
rustup default stable
```

### Windows (x64)

* **Rust**: the `stable-x86_64-pc-windows-msvc` toolchain.
* **MSVC build tools**: the *Visual Studio Build Tools* (or Visual Studio
  2022) with the *Desktop development with C++* workload. The `x86_64-pc-windows-msvc`
  toolchain links the Microsoft C runtime dynamically, so the build needs
  `link.exe` and the Windows SDK from that workload. Install it with the
  Visual Studio installer or with the Build Tools bootstrapper; the
  `rustup` toolchain alone is not enough.
* **NSIS** (optional, for the installer): from <https://nsis.sourceforge.io/Main.html>.
  It puts `makensis.exe` on `PATH` and installs into
  `C:\Program Files (x86)\NSIS`.
* **PowerShell**: 5.1 is enough; the packaging script also runs unchanged
  under PowerShell 7 (`pwsh`).

### macOS (arm64 and x64)

* **Rust**: the `stable-aarch64-apple-darwin` or `stable-x86_64-apple-darwin`
  toolchain, matching the machine.
* **Xcode Command Line Tools**: `xcode-select --install`. The desktop
  application links against the system frameworks; nothing else from Xcode is
  needed.
* **hdiutil**: part of macOS. It is used for the disk image and is skipped
  with a message if it is missing.

### Linux (x86_64)

* **Rust**: the `stable-x86_64-unknown-linux-gnu` toolchain.
* **System libraries**: `eframe` (winit + wgpu) links against the GL, X11,
  XCB, XKB and Wayland client libraries, so their development headers have to
  be installed before the desktop crate can be built:

  ```bash
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends \
    libgl1-mesa-dev \
    libegl1-mesa-dev \
    libx11-dev \
    libxcb1-dev \
    libxcb-randr0-dev \
    libxcb-shm0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    libxkbcommon-x11-dev \
    libwayland-dev \
    libssl-dev \
    pkg-config
  ```

  The rest of the workspace has no system dependencies at all: `symphonia`
  decodes in pure Rust.
* **Optional, for packaging**:
  * `appimagetool` for the AppImage, from
    <https://github.com/AppImage/AppImageKit/releases>. It also needs
    `squashfs-tools` (`mksquashfs`) on the machine.
  * `dpkg-deb` for the `.deb` (part of the `dpkg` package).

---

## 2. The four gates

These four commands are the project's contract (see `AGENTS.md`). Run all
four before claiming that anything works, and report the real exit codes.

```bash
cargo fmt --all -- --check                              # 1. formatting
cargo clippy --workspace --all-targets -- -D warnings   # 2. lints
cargo test --workspace                                   # 3. tests
cargo build --release --workspace                        # 4. release binaries
```

| Gate | What it means | What it catches |
| --- | --- | --- |
| `cargo fmt --all -- --check` | The formatting is exactly what `rustfmt` would produce. It changes nothing; it only reports. | A diff that would fail on the next person's machine. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Every crate, every target (lib, bins, tests, examples) compiles without a lint warning. `-D warnings` turns a warning into an error. | The usual Rust foot-guns: needless clones, wrong iterator adaptors, suspicious `unwrap`. |
| `cargo test --workspace` | The whole test suite passes. The tests are hermetic: they generate their own fixtures in a temporary directory and depend on nothing outside the test - no Python, no network, no checked-in media. | A change that broke the DSP, the drift estimate or the export. |
| `cargo build --release --workspace` | The optimised binaries build with the release profile (`opt-level = 3`, thin LTO, 4 codegen units). | A build that only works in debug, or a feature that does not compile under the release profile. |

A gate that fails is fixed or reported. It is not weakened, and the test is
not deleted to make the run green.

---

## 3. Building the release binaries

On every platform, from the repository root:

```bash
cargo build --release --workspace
```

That produces both binaries:

```
target/release/stagesync            target/release/stagesync.exe
target/release/stagesync-desktop    target/release/stagesync-desktop.exe
```

To build only the desktop application:

```bash
cargo build --release --bin stagesync-desktop
```

To build only the command-line tool:

```bash
cargo build --release --bin stagesync
```

The build is deterministic: the same input produces the same output, and
nothing in the pipeline holds a whole file in memory.

---

## 4. The packaging artefacts

The packaging scripts live in `packaging/` and are described in
[`packaging/README.md`](packaging/README.md). They all build into the
repository's own `target/` directory, so a packaging run and a `cargo test`
run cannot happen at the same time.

### Windows

```powershell
.\packaging\windows\build.ps1
.\packaging\windows\build.ps1 -Profile release -OutputDir dist\windows
```

Produces, in `dist\windows`:

| Artefact | Needs |
| --- | --- |
| `stagesync-<version>-windows-x64-portable.zip` | nothing beyond the Rust toolchain |
| `stagesync-<version>-windows-x64-setup.exe` | NSIS (`makensis`) |

When `makensis` is not on `PATH` the script says so, skips the installer and
still writes the ZIP. It exits 0: a missing installer is a missing artefact,
not a failed build.

The installer is **per-user**: it installs into
`%LOCALAPPDATA%\Programs\StageSync`, asks for no administrator rights, adds
a Start-menu shortcut and an optional desktop shortcut, and registers itself
in *Apps & features*.

The binary needs the **Microsoft Visual C++ runtime** on the machine, because
the MSVC build links the C runtime dynamically. The `README.txt` inside the
portable ZIP says so and gives the download link.

### macOS

```bash
./packaging/macos/build.sh
```

Produces, in `dist/macos`:

| Artefact | Needs |
| --- | --- |
| `StageSync.app` | nothing beyond the Rust toolchain |
| `stagesync-<version>-macos-<arch>.app.tar.gz` | nothing beyond the Rust toolchain |
| `stagesync-<version>-macos-<arch>.dmg` | `hdiutil` |

`<arch>` is `uname -m` of the machine that built it. The bundle is
**unsigned and not notarised**, so Gatekeeper quarantines it on first launch
until the user allows it explicitly.

### Linux

```bash
./packaging/linux/build.sh
```

Produces, in `dist/linux`:

| Artefact | Needs |
| --- | --- |
| `AppDir/` | nothing beyond the Rust toolchain |
| `stagesync-<version>-linux-x86_64.tar.gz` | nothing beyond the Rust toolchain |
| `stagesync-<version>-linux-x86_64.AppImage` | `appimagetool` (and `mksquashfs`) |
| `stagesync-<version>-linux-x86_64.deb` | `dpkg-deb` |

A missing `appimagetool` or `dpkg-deb` is reported in the script's output and
the remaining artefacts are still written.

The `.deb` installs `/usr/bin/stagesync-desktop` and a desktop entry, and
depends only on `libc6` and `libgcc-s1` - the application is not linked
against GTK.

---

## 5. Where CI puts the artefacts

Two workflows live in `.github/workflows/`:

| Workflow | Trigger | What it does |
| --- | --- | --- |
| `ci.yml` | every push and pull request | Runs the four gates on `windows-latest`, `macos-latest` (arm64), `macos-13` (x64) and `ubuntu-latest`, and uploads the release binaries as workflow artefacts. |
| `release.yml` | a `v*` tag, or `workflow_dispatch` | Builds the packaged artefacts on the same four native runners and attaches them to the GitHub release for the tag. |

Every job runs on a **native** runner. Nothing is cross-compiled, so a green
Linux job is a statement about Linux and a green macOS job is a statement
about macOS.

The workflow artefacts are named after the platform:

| Artefact name | Contents |
| --- | --- |
| `stagesync-windows-x86_64` | `stagesync-desktop.exe`, `stagesync.exe` |
| `stagesync-macos-arm64` | `stagesync-desktop`, `stagesync` |
| `stagesync-macos-x86_64` | `stagesync-desktop`, `stagesync` |
| `stagesync-linux-x86_64` | `stagesync-desktop`, `stagesync` |

The release attaches the packaged files to the release page:

| Platform | Release asset |
| --- | --- |
| Windows x64 | `stagesync-<version>-windows-x64-setup.exe` |
| Windows x64 | `stagesync-<version>-windows-x64-portable.zip` |
| macOS arm64 | `stagesync-<version>-macos-arm64.app.tar.gz`, `stagesync-<version>-macos-arm64.dmg` |
| macOS x64 | `stagesync-<version>-macos-x86_64.app.tar.gz`, `stagesync-<version>-macos-x86_64.dmg` |
| Linux x86_64 | `stagesync-<version>-linux-x86_64.AppImage` |
| Linux x86_64 | `stagesync-<version>-linux-x86_64.deb` |
| Linux x86_64 | `stagesync-<version>-linux-x86_64.tar.gz` |

To release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The release workflow can also be started by hand (*Actions* -> *Release* ->
*Run workflow*). Its `ref` input is the git ref to build; it defaults to the
ref the workflow was started from. The release is then tagged with that same
value, so a manual run with `ref: v0.1.0` adds the artefacts to the existing
`v0.1.0` release, while a manual run with a branch name creates a release
tagged with that branch name. Nothing in the workflow is allowed to fail
quietly: if a packaging step cannot run, the artefact is missing from the
release rather than quietly empty.

---

## 6. What is verified and what is not

This section is the honest one. Read it before trusting any platform claim.

**Verified.** The Windows x64 build and the full test suite were executed on
a Windows 11 x64 development machine: the four gates in §2 all exited 0, and
`PROJECT_STATUS.md` §7 records the per-crate test counts as they were run.
The desktop binary was launched there and stayed alive without writing to
stdout or stderr. Its *window* was never rendered and looked at, so its
on-screen behaviour is still unverified - that is stated in
`PROJECT_STATUS.md` §6 as well.

**Not verified.** The macOS and Linux **builds and packaging steps are
configured but were not executed**. The repository had no GitHub remote at
the time of writing, so neither `ci.yml` nor `release.yml` has ever run, and
nothing about macOS or Linux may be claimed on their account. They are
unverified until the CI runs them.

That covers, concretely:

* the Linux `apt` package set in §1 - a well-known set for eframe 0.37 and
  wgpu on Ubuntu 22.04 and later, but not one that has been exercised here;
* the macOS bundle, the disk image and the `Info.plist`;
* the NSIS installer, which has never been compiled - `makensis` was not
  available on the development machine;
* the AppImage and the `.deb`;
* the CI workflows themselves, as configuration.

Two more things are worth stating plainly:

* The `macos-13` runner label is the Intel macOS runner. GitHub retires
  runner images from time to time; if that label disappears, use the current
  Intel label (`macos-15-intel`) instead. Nothing else in the job changes.
* `appimagetool` is distributed as an AppImage and the GitHub runner has no
  FUSE, so the release workflow extracts it and puts its own `AppRun` on
  `PATH` under the name `appimagetool`. That path has not been exercised.

When a workflow has run, this section should be rewritten with what actually
happened - including the failures.

---

## 7. Dependencies and licences

Every third-party dependency and its licence is recorded in `LICENSES.md`.
To list the dependencies that actually go into the binaries:

```bash
cargo tree --locked --offline --edges normal --prefix none --workspace
```

* `--locked` refuses to change `Cargo.lock`, so the list matches what is
  committed.
* `--offline` keeps cargo from reaching the network, so the output cannot be
  changed by a newer release being published in the meantime.
* `--edges normal` drops the build-dependency and dev-dependency edges, so
  only what is linked into the shipped binaries is listed.
* `--prefix none` drops the `crate v1.2.3 (registry)` decoration, so the
  output is a plain list of crate names to check against `LICENSES.md`.

Note that `symphonia` is **MPL-2.0**: it is linked, not vendored. Anyone who
redistributes a binary of this project has to honour the MPL's file-level
copyleft. See `LICENSES.md`.
