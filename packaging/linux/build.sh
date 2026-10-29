#!/usr/bin/env bash
#
# Build the StageSync Linux artefacts.
#
# Produces, in dist/linux:
#
#   AppDir/                                   the AppImage source directory
#   stagesync-<version>-linux-x86_64.AppImage  a self-contained application
#   stagesync-<version>-linux-x86_64.deb        a Debian/Ubuntu package
#   stagesync-<version>-linux-x86_64.tar.gz     the AppDir, archived
#
# Requirements: the Rust toolchain, and for the optional steps `appimagetool`
# (for the AppImage) and `dpkg-deb` (for the deb). When one of them is
# missing the script says so and carries on with the artefacts it can make;
# the skip is always reported, never silent.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

arch="x86_64"
package_name="stagesync"
maintainer="StageSync contributors <stagesync@users.noreply.github.com>"

# --- the version ---------------------------------------------------------------

version="$(awk '
  /^\[workspace\.package\]/ { in_section = 1; next }
  /^\[/ { in_section = 0 }
  in_section && /^version[[:space:]]*=/ {
    gsub(/"/, "", $3)
    print $3
    exit
  }
' Cargo.toml)"

if [ -z "${version}" ]; then
  echo "error: the workspace version could not be read from Cargo.toml" >&2
  exit 1
fi

# --- the build -----------------------------------------------------------------

echo "==> building the stagesync-desktop binary (linux, ${arch})"
cargo build --release --locked --bin stagesync-desktop

binary="${repo_root}/target/release/stagesync-desktop"
if [ ! -f "${binary}" ]; then
  echo "error: the built binary was not found at '${binary}'" >&2
  exit 1
fi

dist_dir="${repo_root}/dist/linux"
app_dir="${dist_dir}/AppDir"

rm -rf "${app_dir}"
mkdir -p "${app_dir}/assets"

# --- the AppDir ----------------------------------------------------------------

install -m 0755 "${binary}" "${app_dir}/stagesync-desktop"
install -m 0644 "${repo_root}/packaging/linux/stagesync.desktop" "${app_dir}/stagesync.desktop"

cat > "${app_dir}/AppRun" <<'APPRUN'
#!/bin/sh
# The entry point of the StageSync AppDir. It starts the application from
# wherever the AppImage happens to be mounted.
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/stagesync-desktop" "$@"
APPRUN
chmod +x "${app_dir}/AppRun"

repo_icon="${repo_root}/assets/icon.png"
placeholder_icon="${dist_dir}/placeholder-icon.png"
if [ -f "${repo_icon}" ]; then
  install -m 0644 "${repo_icon}" "${app_dir}/assets/icon.png"
  install -m 0644 "${repo_icon}" "${app_dir}/stagesync.png"
  icon_source="assets/icon.png"
else
  echo "note: assets/icon.png was not found."
  echo "      appimagetool refuses to build an AppImage without an icon, so a generated"
  echo "      placeholder icon is written to AppDir/stagesync.png. It is not artwork:"
  echo "      replace assets/icon.png in the repository and build again for a real icon."
  icon_source="a generated placeholder"
  base64 -d > "${placeholder_icon}" <<'PLACEHOLDER_ICON'
iVBORw0KGgoAAAANSUhEUgAAAQAAAAEACAIAAADTED8xAAADHUlEQVR42u3TMQ0AIAxE0QrqwIR/
DR2qBwcd2Agv+Qru8iLXlr4tTCAAJAAkACQAJAAkACQAJAAkACQAJAAkACQAJAAkACQAJAAkACQA
JAAkACQAJAAkACQAJAAkACQAJAAkACQAJAAkACQAJAAkACQAJAAkACQAJAAkACQAJAAkACQAJACu
quqn8yAAAAgAAAQAAAIAAAEAgAAAQAAAAIAAAEAAACAAABAAAAgAAAQAAAAAAAAAAAAAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAgBMBAEAAACAAABAAAAgAAAQAAABYAQAABAAAAgAAAQCAAABAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAQCAAABAAAAgAAAQAAAAIAAAEAAACAAA
BAAAAgAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADgRAAAEAAACAAABAAAAkACQACY
QABIAEgASABIAEgASABIAEgASABIAEgASABIAEgASABIAEgASABIAEgASABIAEgASABIAEgASABI
AEgASABIAEgASABIAEgASABIAEgASABIAEgASABIAEgASEMHOPZNA7nEdnMAAAAASUVORK5CYII=
PLACEHOLDER_ICON
  install -m 0644 "${placeholder_icon}" "${app_dir}/stagesync.png"
fi

# --- the AppImage --------------------------------------------------------------

appimage="${dist_dir}/stagesync-${version}-linux-${arch}.AppImage"
if command -v appimagetool > /dev/null 2>&1; then
  rm -f "${appimage}"
  echo "==> building the AppImage (icon: ${icon_source})"
  ARCH="${arch}" appimagetool "${app_dir}" "${appimage}"
else
  echo "note: appimagetool was not found on PATH, so the AppImage step was skipped."
  echo "      The AppDir tarball was produced instead: ${dist_dir}/stagesync-${version}-linux-${arch}.tar.gz"
fi

# --- the AppDir tarball --------------------------------------------------------
#
# GNU tar is asked for a reproducible archive: the entries are sorted, every
# timestamp is pinned and the owner is fixed.

tar_version="$(tar --version 2>/dev/null || true)"
tar_is_gnu=0
case "${tar_version}" in
  *"GNU tar"*) tar_is_gnu=1 ;;
esac

tarball="${dist_dir}/stagesync-${version}-linux-${arch}.tar.gz"
rm -f "${tarball}"
if [ "${tar_is_gnu}" -eq 1 ]; then
  tar --sort=name "--mtime=UTC 2020-01-01" --owner=0 --group=0 --numeric-owner \
    -czf "${tarball}" -C "${dist_dir}" "AppDir"
else
  echo "note: this system's tar is not GNU tar, so the archive is written without the" \
    "reproducibility flags (--sort, --mtime, --owner, --group); its bytes will differ" \
    "between machines."
  tar -czf "${tarball}" -C "${dist_dir}" "AppDir"
fi

# --- the Debian package --------------------------------------------------------

deb_root="${dist_dir}/deb"
rm -rf "${deb_root}"
mkdir -p "${deb_root}/DEBIAN"
mkdir -p "${deb_root}/usr/bin"
mkdir -p "${deb_root}/usr/share/applications"

install -m 0755 "${binary}" "${deb_root}/usr/bin/stagesync-desktop"
install -m 0644 "${repo_root}/packaging/linux/stagesync.desktop" \
  "${deb_root}/usr/share/applications/stagesync.desktop"

if [ -f "${repo_icon}" ]; then
  mkdir -p "${deb_root}/usr/share/icons/hicolor/256x256/apps"
  install -m 0644 "${repo_icon}" \
    "${deb_root}/usr/share/icons/hicolor/256x256/apps/stagesync.png"
else
  # The desktop entry names the icon `stagesync`, so the package has to carry
  # one. Without it the launcher shows a broken icon on every desktop.
  mkdir -p "${deb_root}/usr/share/icons/hicolor/256x256/apps"
  install -m 0644 "${placeholder_icon}" \
    "${deb_root}/usr/share/icons/hicolor/256x256/apps/stagesync.png"
fi

cat > "${deb_root}/DEBIAN/control" <<CONTROL
Package: ${package_name}
Version: ${version}
Architecture: ${arch}
Section: audio
Priority: optional
Maintainer: ${maintainer}
Depends: libc6, libgcc-s1, libx11-6, libxkbcommon0, libx11-xcb1, libwayland-client0, libgl1 | libegl1
Description: Drift-aware multi-recorder audio synchronisation
 StageSync aligns several recordings of the same concert or play against
 one reference recording, measures the clock drift of each recorder, and
 renders drift-corrected audio.
CONTROL

deb="${dist_dir}/stagesync-${version}-linux-${arch}.deb"
if command -v dpkg-deb > /dev/null 2>&1; then
  rm -f "${deb}"
  echo "==> building the Debian package"
  dpkg-deb --build --root-owner-group "${deb_root}" "${deb}"
else
  echo "note: dpkg-deb was not found, so the Debian package step was skipped."
fi

# --- what was written ----------------------------------------------------------

echo ""
echo "StageSync Linux packaging finished."
echo "version         : ${version}"
echo "architecture    : ${arch}"
echo "app directory   : ${app_dir}"
echo "appdir tarball  : ${tarball}"
if [ -f "${appimage}" ]; then
  echo "appimage        : ${appimage}"
else
  echo "appimage        : (not produced - appimagetool was not available)"
fi
if [ -f "${deb}" ]; then
  echo "debian package  : ${deb}"
else
  echo "debian package  : (not produced - dpkg-deb was not available)"
fi
echo "output directory: ${dist_dir}"
