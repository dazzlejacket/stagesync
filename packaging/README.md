# Packaging

The scripts in this directory turn a `cargo build --release` into something a
user can download and run. There is one script per platform and they all do
the same four things: build the `stagesync-desktop` binary, generate the
licence and notice bundle, assemble the result into a platform-specific shape,
and write the result into `dist/<platform>/` with a SHA-256 manifest beside
it.

None of them is a substitute for `cargo test`. They are packaging only.

## What each artefact is

| Platform | Artefact | What it is |
| --- | --- | --- |
| Windows | `stagesync-<version>-windows-x64-portable.zip` | The binary, a `README.txt` explaining the Microsoft Visual C++ runtime requirement, and the licence and notice bundle. Unpacks anywhere, installs nothing. |
| Windows | `stagesync-<version>-windows-x64-setup.exe` | An NSIS installer. Installs per-user into `%LOCALAPPDATA%\Programs\StageSync`, adds a Start-menu shortcut, registers itself in *Apps & features*, and installs the notices into `<install dir>\notices`. |
| macOS | `stagesync-<version>-macos-<arch>.app.tar.gz` | The `StageSync.app` bundle, archived. Unsigned and not notarised. |
| macOS | `stagesync-<version>-macos-<arch>.dmg` | The same bundle as a disk image. Unsigned and not notarised. |
| Linux | `stagesync-<version>-linux-x86_64.AppImage` | The application, with the X11/XCB/Wayland client libraries bundled. It does **not** bundle the C library or the graphics driver: those come from the host. Needs `FUSE` to run, which is why the tarball exists as well. |
| Linux | `stagesync-<version>-linux-x86_64.deb` | A Debian/Ubuntu package: `/usr/bin/stagesync-desktop`, a desktop entry, an icon and the notices in `/usr/share/doc/stagesync/notices`. The dependencies are read out of the built binary. The control file says `Architecture: amd64` - that is Debian's name for the port, not `x86_64`. |
| Linux | `stagesync-<version>-linux-x86_64.tar.gz` | The `AppDir` the AppImage is built from, for people who cannot or will not install FUSE. |

`<version>` is the `version` in `[workspace.package]` of the root
`Cargo.toml`. `<arch>` is `uname -m` on the machine that built it: `arm64` or
`x86_64`.

Every platform also writes `SHA256SUMS.txt` next to its artefacts, in the
`sha256sum` format, so that a release can publish one manifest that anybody
can check with `sha256sum -c`.

## What each script needs

| Script | Needs | Optional |
| --- | --- | --- |
| `windows/build.ps1` | the Rust toolchain, PowerShell 5.1 or later, Python 3 | NSIS (`makensis`) for the installer |
| `macos/build.sh` | the Rust toolchain, `tar`, Python 3 | `hdiutil` for the disk image |
| `linux/build.sh` | the Rust toolchain, `tar`, `base64`, Python 3, `binutils` | `dpkg-deb` for the deb; `appimagetool` for the AppImage, which the script downloads and checksum-verifies itself |

Python 3 is needed because the licence and notice bundle is *generated* from
`Cargo.lock` for every build - see `notices/README.md`. A missing Python is a
hard failure, not a skipped step: a binary without its notices is worse than
no binary at all.

None of the optional tools is required for the script to succeed: when one is
missing the script prints a message that says exactly which artefact was not
produced and why, and it writes the ones it can. A skip is never silent.

## The pinned toolchain

`rust-toolchain.toml` in this directory pins the Rust version a release is
built with. The packaging scripts read it and refuse to build with a different
compiler, and the workflows install exactly that version.

It lives here rather than at the repository root on purpose: a
`rust-toolchain.toml` at the root would silently change the toolchain of
every `cargo` invocation in the repository, including an engineer's own test
run, which is not the packaging scripts' business.

## Running them

From the repository root:

```powershell
# Windows
.\packaging\windows\build.ps1
.\packaging\windows\build.ps1 -Profile release -OutputDir dist/windows
```

```bash
# macOS
./packaging/macos/build.sh
bash packaging/macos/build.sh      # if the file is not marked executable
```

```bash
# Linux
./packaging/linux/build.sh
bash packaging/linux/build.sh      # if the file is not marked executable
```

`windows/build.ps1` takes `-Profile` (the cargo profile, `release` by default),
`-OutputDir` (`dist/windows` by default) and `-SkipToolchainCheck`. The two
shell scripts take no arguments; they always write into `dist/macos` and
`dist/linux`.

The scripts build into the repository's own `target/` directory, so a
packaging run and a `cargo test` run cannot happen at the same time - they
would fight over the cargo lock. `windows/build.ps1` honours
`CARGO_TARGET_DIR` if it is set, so a build can share a prepared dependency
cache.

## Reproducibility

Two runs of the same script on the same inputs produce the same bytes:

* the Windows ZIP is written entry by entry, in a fixed order, with a fixed
  timestamp - 2020-01-01T00:00:00Z, or `$env:SOURCE_DATE_EPOCH` when that is
  set - instead of the file's own modification time;
* the Linux and macOS archives pin every file's timestamp, sort the entries
  and write the gzip stream without a timestamp of its own (`gzip -n`);
* the notices are generated from `Cargo.lock` and `licenses.json`, and
  contain no timestamp either;
* every platform writes a SHA-256 manifest of its artefacts.

The macOS disk image is the exception: `hdiutil` records its own creation
time, so the `.dmg`'s bytes differ between runs even when its contents do
not. The `.app.tar.gz` next to it is reproducible.

## Icons

The repository does not contain an `assets/` directory yet, so:

* the macOS bundle is built without an `icon.icns`;
* the `.deb` is built without an icon;
* the AppDir gets a **generated placeholder** `stagesync.png`, because
  `appimagetool` refuses to build an AppImage without an icon. The script
  says so when it happens.

Put a real `assets/icon.png` (256x256 or larger) and `assets/icon.icns` in the
repository and the scripts pick them up with no change.

## The NSIS script

`windows/installer.nsi` is compiled by `makensis`. The build script passes it
the version, the install directory, the built binary, the portable ZIP and the
notices directory as `-D` defines:

```
makensis /DSTAGESYNC_VERSION=0.1.0 /DSTAGESYNC_VERSION_NUM=0.1.0.0 \
         /DSTAGESYNC_INSTALL_DIR=C:/Users/you/AppData/Local/Programs/StageSync \
         /DSTAGESYNC_EXE=.../target/release/stagesync-desktop.exe \
         /DSTAGESYNC_PORTABLE_ZIP=.../stagesync-0.1.0-windows-x64-portable.zip \
         /DSTAGESYNC_NOTICES_DIR=.../dist/windows/portable/notices \
         packaging\windows\installer.nsi
```

Every define has a default in the script, so it also compiles by hand. The
paths are written with forward slashes on purpose: NSIS resolves a backslash
inside a string as an escape sequence, so `"$INSTDIR\notices"` is a newline
and not a directory called `notices`.

`windows/check_nsi.py` checks the script for that class of mistake, for
`${NAME}` references to defines that do not exist, for `$NAME` references to
defines that do (the bare `$` form asks for a runtime variable, which is
empty), and for defines that are never used. It needs no NSIS installation:

```
python packaging/windows/check_nsi.py packaging/windows/installer.nsi
```

## The appimagetool

The Linux script builds the AppImage with `appimagetool`, pinned to the
release `1.9.1` and verified against the SHA-256 in
`linux/appimagetool.sha256` before it is executed. The tool used to come from
the `continuous` release of AppImageKit, which is a moving target: the bytes
under that URL change without notice, so a build that fetched it ran whatever
happened to be published last, with nothing to compare it against.

If the checksum cannot be found - neither in the file nor in the environment
variable `APPIMAGETOOL_SHA256` - the script stops without downloading or
running anything.

## Signing

Nothing here is signed. The Windows installer is unsigned, and the macOS
bundle is unsigned and not notarised, so Gatekeeper will quarantine it on
first launch. That is stated in the release notes rather than left for the
user to discover.
