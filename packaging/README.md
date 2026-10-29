# Packaging

The scripts in this directory turn a `cargo build --release` into something a
user can download and run. There is one script per platform and they all do
the same three things: build the `stagesync-desktop` binary, assemble it into
a platform-specific shape, and write the result into `dist/<platform>/`.

None of them is a substitute for `cargo test`. They are packaging only.

## What each artefact is

| Platform | Artefact | What it is |
| --- | --- | --- |
| Windows | `stagesync-<version>-windows-x64-portable.zip` | The binary plus a `README.txt` explaining the Microsoft Visual C++ runtime requirement. Unpacks anywhere, installs nothing. |
| Windows | `stagesync-<version>-windows-x64-setup.exe` | An NSIS installer. Installs per-user into `%LOCALAPPDATA%\Programs\StageSync`, adds a Start-menu shortcut, registers itself in *Apps & features*. |
| macOS | `stagesync-<version>-macos-<arch>.app.tar.gz` | The `StageSync.app` bundle, archived. Unsigned and not notarised. |
| macOS | `stagesync-<version>-macos-<arch>.dmg` | The same bundle as a disk image. Unsigned and not notarised. |
| Linux | `stagesync-<version>-linux-x86_64.AppImage` | A self-contained application. Needs `FUSE` to run, which is why the tarball exists as well. |
| Linux | `stagesync-<version>-linux-x86_64.deb` | A Debian/Ubuntu package: `/usr/bin/stagesync-desktop`, a desktop entry and, when the icon exists, a 256x256 icon. |
| Linux | `stagesync-<version>-linux-x86_64.tar.gz` | The `AppDir` the AppImage is built from, for people who cannot or will not install FUSE. |

`<version>` is the `version` in `[workspace.package]` of the root
`Cargo.toml`. `<arch>` is `uname -m` on the machine that built it: `arm64` or
`x86_64`.

## What each script needs

| Script | Needs | Optional |
| --- | --- | --- |
| `windows/build.ps1` | the Rust toolchain, PowerShell 5.1 or later | NSIS (`makensis`) for the installer |
| `macos/build.sh` | the Rust toolchain, `tar` | `hdiutil` for the disk image |
| `linux/build.sh` | the Rust toolchain, `tar`, `base64` | `appimagetool` for the AppImage, `dpkg-deb` for the deb |

None of the optional tools is required for the script to succeed: when one is
missing the script prints a message that says exactly which artefact was not
produced and why, and it writes the ones it can. A skip is never silent.

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

`windows/build.ps1` takes `-Profile` (the cargo profile, `release` by default)
and `-OutputDir` (`dist/windows` by default). The two shell scripts take no
arguments; they always write into `dist/macos` and `dist/linux`.

The scripts build into the repository's own `target/` directory, so a
packaging run and a `cargo test` run cannot happen at the same time - they
would fight over the cargo lock.

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
the version, the install directory, the built binary and the portable ZIP as
`-D` defines:

```
makensis /DSTAGESYNC_VERSION=0.1.0 /DSTAGESYNC_VERSION_NUM=0.1.0.0 \
         /DSTAGESYNC_INSTALL_DIR=C:\Users\you\AppData\Local\Programs\StageSync \
         /DSTAGESYNC_EXE=...\target\release\stagesync-desktop.exe \
         /DSTAGESYNC_PORTABLE_ZIP=...\stagesync-0.1.0-windows-x64-portable.zip \
         packaging\windows\installer.nsi
```

Every define has a default in the script, so it also compiles by hand.

## Signing

Nothing here is signed. The Windows installer is unsigned, and the macOS
bundle is unsigned and not notarised, so Gatekeeper will quarantine it on
first launch. That is stated in the release notes rather than left for the
user to discover.
