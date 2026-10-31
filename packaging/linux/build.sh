#!/usr/bin/env bash
#
# Build the StageSync Linux artefacts.
#
# Produces, in dist/linux:
#
#   AppDir/                                    the AppImage source directory
#   stagesync-<version>-linux-x86_64.AppImage  the application image
#   stagesync-<version>-linux-x86_64.deb       a Debian/Ubuntu package
#   stagesync-<version>-linux-x86_64.tar.gz    the AppDir, archived
#   SHA256SUMS.txt                             the SHA-256 of the artefacts
#
# What the AppImage is and is not:
#
#   It bundles the application and the X11/XCB/Wayland client libraries the
#   binary needs, so it runs on a host that has none of them. It does NOT
#   bundle the C library, the graphics driver or the kernel interface: those
#   have to come from the host, and saying otherwise would be a claim the
#   artefact cannot support. The Debian package declares the same libraries
#   as real dependencies instead, which is what a package manager needs.
#
# Requirements: the Rust toolchain, and for the optional steps `dpkg-deb` (for
# the deb) and a pinned, checksum-verified `appimagetool` (for the AppImage).
# When one of them is missing the script says so and carries on with the
# artefacts it can make; the skip is always reported, never silent.
#
# Nothing is downloaded and executed without its checksum being verified
# first. The appimagetool version and its SHA-256 are pinned in
# packaging/linux/appimagetool.sha256.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

# The name used in the artefact file names, and the Debian architecture
# identifier. They are different names for the same machine: "x86_64" is the
# name of the CPU, "amd64" is the name Debian gives the port. A control file
# that says `Architecture: x86_64` is rejected by dpkg.
file_arch="x86_64"
deb_arch="amd64"

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

# --- the pinned toolchain --------------------------------------------------------
#
# A release is built with one known compiler, not with whatever "stable" is
# this week. The pin lives under packaging/ so that it does not change the
# toolchain of every cargo invocation in the repository.

pinned_toolchain="$(awk '
  /^[[:space:]]*channel[[:space:]]*=/ {
    gsub(/"/, "", $3)
    print $3
    exit
  }
' packaging/rust-toolchain.toml 2>/dev/null || true)"

if [ -n "${pinned_toolchain}" ]; then
  installed_toolchain="$(rustc --version | awk '{print $2}')"
  if [ "${installed_toolchain}" != "${pinned_toolchain}" ]; then
    echo "error: the pinned Rust toolchain is ${pinned_toolchain} but ${installed_toolchain} is installed." >&2
    echo "       Install it with 'rustup toolchain install ${pinned_toolchain}' and try again." >&2
    exit 1
  fi
  echo "==> toolchain: rustc ${installed_toolchain} (the pinned version)"
fi

# --- the build -----------------------------------------------------------------

echo "==> building the stagesync-desktop binary (linux, ${file_arch})"
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

# --- the notices bundle ----------------------------------------------------------
#
# The notices are generated from Cargo.lock, so they always describe the
# dependency graph that produced this binary.

notices_dir="${dist_dir}/notices"
rm -rf "${notices_dir}"
mkdir -p "${notices_dir}"

python="${STAGESYNC_PYTHON:-}"
if [ -z "${python}" ]; then
  for candidate in python3 python; do
    if ! command -v "${candidate}" > /dev/null 2>&1; then
      continue
    fi
    # The candidate has to be an interpreter, not a stub that only pretends:
    # on some systems `python3` is a package-manager shim that prints an
    # error and exits non-zero.
    if "${candidate}" --version 2> /dev/null | grep -q '^Python 3\.'; then
      python="${candidate}"
      break
    fi
  done
fi
if [ -z "${python}" ]; then
  echo "error: Python 3 was not found, and the licence and notice bundle cannot be" >&2
  echo "       generated without it. Install Python 3 and try again." >&2
  exit 1
fi

echo "==> generating the licence and notice bundle"
"${python}" "${repo_root}/packaging/notices/generate_notices.py" \
  --cargo-lock "${repo_root}/Cargo.lock" \
  --licenses "${repo_root}/packaging/notices/licenses.json" \
  --cargo-toml "${repo_root}/Cargo.toml" \
  --target "x86_64-unknown-linux-gnu" \
  --output "${notices_dir}"

for notice in LICENSE-MIT LICENSE-APACHE LICENSE-MPL-2.0; do
  if [ ! -f "${repo_root}/packaging/notices/${notice}" ]; then
    echo "error: the licence text '${notice}' was not found" >&2
    exit 1
  fi
  install -m 0644 "${repo_root}/packaging/notices/${notice}" "${notices_dir}/${notice}"
done

# --- the native runtime dependencies of the built binary --------------------------
#
# The package dependencies are read out of the binary that was just built, not
# out of a hand-written list that drifts. A shared library the binary needs
# that is not in the table below stops the build: a package that silently
# omits a dependency is a package that does not install.

soname_to_package() {
  case "$1" in
    libc.so.6|libm.so.6|libdl.so.2|libpthread.so.0|librt.so.1|libresolv.so.2|libutil.so.1) echo libc6 ;;
    libgcc_s.so.1) echo libgcc-s1 ;;
    libstdc++.so.6) echo libstdc++6 ;;
    libatomic.so.1) echo libatomic1 ;;
    libX11.so.6) echo libx11-6 ;;
    libX11-xcb.so.1) echo libx11-xcb1 ;;
    libXext.so.6) echo libxext6 ;;
    libXi.so.6) echo libxi6 ;;
    libXrandr.so.2) echo libxrandr2 ;;
    libXrender.so.1) echo libxrender1 ;;
    libXcursor.so.1) echo libxcursor1 ;;
    libXinerama.so.1) echo libxinerama1 ;;
    libXfixes.so.3) echo libxfixes3 ;;
    libXtst.so.6) echo libxtst6 ;;
    libXxf86vm.so.1) echo libxxf86vm1 ;;
    libxcb.so.1) echo libxcb1 ;;
    libxcb-dri2.so.0) echo libxcb-dri2-0 ;;
    libxcb-dri3.so.0) echo libxcb-dri3-0 ;;
    libxcb-glx.so.0) echo libxcb-glx0 ;;
    libxcb-present.so.0) echo libxcb-present0 ;;
    libxcb-randr.so.0) echo libxcb-randr0 ;;
    libxcb-render.so.0) echo libxcb-render0 ;;
    libxcb-shape.so.0) echo libxcb-shape0 ;;
    libxcb-shm.so.0) echo libxcb-shm0 ;;
    libxcb-sync.so.0) echo libxcb-sync0 ;;
    libxcb-xfixes.so.0) echo libxcb-xfixes0 ;;
    libxcb-xkb.so.0) echo libxcb-xkb1 ;;
    libxkbcommon.so.0) echo libxkbcommon0 ;;
    libxkbcommon-x11.so.0) echo libxkbcommon-x11-0 ;;
    libwayland-client.so.0) echo libwayland-client0 ;;
    libwayland-cursor.so.0) echo libwayland-cursor0 ;;
    libwayland-egl.so.1) echo libwayland-egl1 ;;
    libwayland-server.so.0) echo libwayland-server0 ;;
    libEGL.so.1) echo libegl1 ;;
    libEGL.so.1.*) echo libegl1 ;;
    libGL.so.1) echo libgl1 ;;
    libGLX.so.0) echo libglx0 ;;
    libGLdispatch.so.0) echo libglvnd0 ;;
    libgbm.so.1) echo libgbm1 ;;
    libdrm.so.2) echo libdrm2 ;;
    libasound.so.2) echo libasound2 ;;
    libudev.so.1) echo libudev1 ;;
    libexpat.so.1) echo libexpat1 ;;
    libfontconfig.so.1) echo libfontconfig1 ;;
    libfreetype.so.6) echo libfreetype6 ;;
    libpng16.so.16) echo libpng16-16 ;;
    libharfbuzz.so.0) echo libharfbuzz0b ;;
    libpango-1.0.so.0) echo libpango-1.0-0 ;;
    libpangocairo-1.0.so.0) echo libpango-1.0-0 ;;
    libcairo.so.2) echo libcairo2 ;;
    libgio-2.0.so.0) echo libglib2.0-0 ;;
    libglib-2.0.so.0) echo libglib2.0-0 ;;
    libgobject-2.0.so.0) echo libglib2.0-0 ;;
    *) echo "" ;;
  esac
}

read_needed_sonames() {
  if command -v objdump > /dev/null 2>&1; then
    objdump -p "${binary}" | awk '/NEEDED/ { print $2 }'
  elif command -v readelf > /dev/null 2>&1; then
    readelf -d "${binary}" | sed -n 's/.*NEEDED.*\[\(.*\)\]/\1/p'
  else
    echo "error: neither objdump nor readelf was found, so the runtime dependencies" >&2
    echo "       of the built binary cannot be read. Install binutils and try again." >&2
    exit 1
  fi
}

needed_sonames="$(read_needed_sonames | sort -u)"
if [ -z "${needed_sonames}" ]; then
  echo "error: no shared library dependencies were read from '${binary}'." >&2
  exit 1
fi

runtime_packages=""
unknown_sonames=""
for soname in ${needed_sonames}; do
  package="$(soname_to_package "${soname}")"
  if [ -z "${package}" ]; then
    unknown_sonames="${unknown_sonames} ${soname}"
    continue
  fi
  case " ${runtime_packages} " in
    *" ${package} "*) ;;
    *) runtime_packages="${runtime_packages} ${package}" ;;
  esac
done

if [ -n "${unknown_sonames}" ]; then
  echo "error: the built binary needs shared libraries whose Debian package is" >&2
  echo "       not known: ${unknown_sonames}" >&2
  echo "       Add them to the soname_to_package() table in this script and build" >&2
  echo "       again. A package that silently omits a dependency is a package" >&2
  echo "       that does not install." >&2
  exit 1
fi

echo "==> the binary needs: ${needed_sonames}"
echo "==> the Debian dependencies are: ${runtime_packages}"

# --- the AppDir ----------------------------------------------------------------

install -m 0755 "${binary}" "${app_dir}/stagesync-desktop"
install -m 0644 "${repo_root}/packaging/linux/stagesync.desktop" "${app_dir}/stagesync.desktop"
mkdir -p "${app_dir}/usr/share/doc/stagesync"
cp -R "${notices_dir}" "${app_dir}/usr/share/doc/stagesync/notices"

# The client libraries are bundled so that the image runs on a host that has
# none of them. The C library, the graphics driver and the kernel interface
# are deliberately left to the host: a bundled C library breaks on an older
# host, and a bundled graphics driver breaks on every host.
bundled_dir="${app_dir}/usr/lib"
bundled_count=0
excluded_sonames="libc.so.6 libm.so.6 libdl.so.2 libpthread.so.0 librt.so.1 libresolv.so.2 libgcc_s.so.1 libGL.so.1 libGLX.so.0 libGLdispatch.so.0 libEGL.so.1 ld-linux-x86-64.so.2 linux-vdso.so.1"

if command -v ldd > /dev/null 2>&1; then
  mkdir -p "${bundled_dir}"
  while read -r line; do
    case "${line}" in
      *"=>"*) resolved="$(printf '%s' "${line}" | sed -n 's/.*=> \([^ ]*\).*/\1/p')" ;;
      *) resolved="" ;;
    esac
    [ -n "${resolved}" ] || continue
    [ -f "${resolved}" ] || continue
    soname="$(basename "${resolved}")"
    skip=0
    for excluded in ${excluded_sonames}; do
      if [ "${soname}" = "${excluded}" ]; then
        skip=1
        break
      fi
    done
    [ "${skip}" -eq 0 ] || continue
    if [ ! -f "${bundled_dir}/${soname}" ]; then
      install -m 0755 "${resolved}" "${bundled_dir}/${soname}"
      bundled_count=$((bundled_count + 1))
    fi
  done < <(ldd "${binary}" 2>/dev/null || true)
  echo "==> bundled ${bundled_count} client librar(y|ies) into the AppDir"
  echo "    not bundled (the host provides them): ${excluded_sonames}"
else
  echo "note: ldd was not found, so no client libraries were bundled into the AppDir."
fi

cat > "${app_dir}/AppRun" <<'APPRUN'
#!/bin/sh
# The entry point of the StageSync AppDir. It starts the application from
# wherever the AppImage happens to be mounted.
#
# The bundled client libraries are found through LD_LIBRARY_PATH. The C
# library and the graphics driver are the host's: they are not bundled, and
# the image does not pretend otherwise.
HERE="$(dirname "$(readlink -f "$0")")"
if [ -d "${HERE}/usr/lib" ]; then
  LD_LIBRARY_PATH="${HERE}/usr/lib${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
  export LD_LIBRARY_PATH
fi
exec "${HERE}/stagesync-desktop" "$@"
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

# --- the appimagetool -------------------------------------------------------------
#
# The tool is pinned to one release and its SHA-256 is verified before it is
# executed. The `continuous` release it used to come from is a moving target:
# the bytes under that URL change without notice, so a build that downloads it
# is a build that runs whatever was published last. If the checksum cannot be
# obtained the script stops; it never runs an unverified binary.

appimagetool_version="1.9.1"
appimagetool_url="https://github.com/AppImage/appimagetool/releases/download/${appimagetool_version}/appimagetool-${file_arch}.AppImage"
appimagetool_checksum_file="${repo_root}/packaging/linux/appimagetool.sha256"

appimagetool_sha256="${APPIMAGETOOL_SHA256:-}"
if [ -z "${appimagetool_sha256}" ] && [ -f "${appimagetool_checksum_file}" ]; then
  appimagetool_sha256="$(awk 'NF && $1 !~ /^#/ { print $1; exit }' "${appimagetool_checksum_file}")"
fi
if [ -z "${appimagetool_sha256}" ]; then
  echo "error: no SHA-256 for appimagetool ${appimagetool_version} is available." >&2
  echo "       Put it in ${appimagetool_checksum_file} or set APPIMAGETOOL_SHA256." >&2
  echo "       The tool is not downloaded and not executed without one." >&2
  exit 1
fi

cache_dir="${STAGESYNC_CACHE_DIR:-${HOME}/.cache/stagesync}"
appimagetool_cache="${cache_dir}/appimagetool-${appimagetool_version}-${file_arch}.AppImage"
appimagetool_extract="${cache_dir}/appimagetool-${appimagetool_version}-${file_arch}"

appimage="${dist_dir}/stagesync-${version}-linux-${file_arch}.AppImage"

if command -v appimagetool > /dev/null 2>&1; then
  # A tool on PATH is used as it is: it is the distribution's own tool, and
  # its integrity is the distribution's business, not this script's.
  echo "==> building the AppImage with the appimagetool on PATH (icon: ${icon_source})"
  rm -f "${appimage}"
  ARCH="${file_arch}" appimagetool "${app_dir}" "${appimage}"
elif command -v curl > /dev/null 2>&1 && command -v sha256sum > /dev/null 2>&1; then
  echo "==> fetching appimagetool ${appimagetool_version}"
  mkdir -p "${cache_dir}"
  rm -f "${appimagetool_cache}"
  curl -fsSL --retry 3 -o "${appimagetool_cache}" "${appimagetool_url}"

  echo "${appimagetool_sha256}  ${appimagetool_cache}" | sha256sum --check --status - || {
    echo "error: the SHA-256 of the downloaded appimagetool does not match the pin." >&2
    echo "       expected: ${appimagetool_sha256}" >&2
    echo "       actual:   $(sha256sum "${appimagetool_cache}" | awk '{print $1}')" >&2
    echo "       The file was not executed." >&2
    exit 1
  }
  echo "==> the appimagetool checksum matches the pin"

  # The GitHub runner has no FUSE, so the AppImage cannot be mounted. It is
  # extracted instead, and its own AppRun is used to build the image.
  rm -rf "${appimagetool_extract}"
  mkdir -p "${appimagetool_extract}"
  ( cd "${appimagetool_extract}" && "${appimagetool_cache}" --appimage-extract > /dev/null )

  echo "==> building the AppImage (icon: ${icon_source})"
  rm -f "${appimage}"
  ARCH="${file_arch}" "${appimagetool_extract}/squashfs-root/AppRun" "${app_dir}" "${appimage}"
else
  echo "note: neither appimagetool nor curl and sha256sum were found, so the" >&2
  echo "      AppImage step was skipped." >&2
  echo "      The AppDir tarball was produced instead: ${dist_dir}/stagesync-${version}-linux-${file_arch}.tar.gz" >&2
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

# Every file in the AppDir gets the same modification time, so that the
# archive does not depend on when the build ran.
find "${app_dir}" -exec touch -h -t 202001010000 {} + 2>/dev/null || \
  find "${app_dir}" -exec touch -t 202001010000 {} +

tarball="${dist_dir}/stagesync-${version}-linux-${file_arch}.tar.gz"
rm -f "${tarball}"
if [ "${tar_is_gnu}" -eq 1 ]; then
  tar --sort=name "--mtime=UTC 2020-01-01" --owner=0 --group=0 --numeric-owner \
    -cf - -C "${dist_dir}" "AppDir" | gzip -n -9 > "${tarball}"
else
  echo "note: this system's tar is not GNU tar, so the archive is written without the" \
    "reproducibility flags (--sort, --mtime, --owner, --group); its bytes will differ" \
    "between machines."
  tar -cf - -C "${dist_dir}" "AppDir" | gzip -n -9 > "${tarball}"
fi

# --- the Debian package --------------------------------------------------------

deb_root="${dist_dir}/deb"
rm -rf "${deb_root}"
mkdir -p "${deb_root}/DEBIAN"
mkdir -p "${deb_root}/usr/bin"
mkdir -p "${deb_root}/usr/share/applications"
mkdir -p "${deb_root}/usr/share/doc/stagesync"

install -m 0755 "${binary}" "${deb_root}/usr/bin/stagesync-desktop"
install -m 0644 "${repo_root}/packaging/linux/stagesync.desktop" \
  "${deb_root}/usr/share/applications/stagesync.desktop"
cp -R "${notices_dir}" "${deb_root}/usr/share/doc/stagesync/notices"

# The Debian convention: a copyright file in the documentation directory.
{
  echo "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
  echo "Upstream-Name: StageSync"
  echo "Source: https://github.com/stagesync/stagesync"
  echo ""
  echo "Files: *"
  echo "Copyright: 2026 The StageSync contributors"
  echo "License: MIT or Apache-2.0"
  echo " The full licence texts are in the notices directory of this package:"
  echo " /usr/share/doc/stagesync/notices/LICENSE-MIT"
  echo " /usr/share/doc/stagesync/notices/LICENSE-APACHE"
  echo ""
  echo "Files: usr/share/doc/stagesync/notices/*"
  echo "Copyright: 2026 The StageSync contributors and the dependencies listed in"
  echo " THIRD_PARTY_NOTICES.md"
  echo "License: MPL-2.0"
  echo " The symphonia crates are licensed under the Mozilla Public License 2.0."
  echo " Their source is available as described in"
  echo " /usr/share/doc/stagesync/notices/SOURCE-AVAILABILITY.md"
} > "${deb_root}/usr/share/doc/stagesync/copyright"

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

# The dependencies come from the built binary, so they cannot drift away from
# what it actually links.
depends="$(printf '%s\n' ${runtime_packages} | sort -u | tr '\n' ',' | sed 's/,$//')"

cat > "${deb_root}/DEBIAN/control" <<CONTROL
Package: ${package_name}
Version: ${version}
Architecture: ${deb_arch}
Section: audio
Priority: optional
Maintainer: ${maintainer}
Depends: ${depends}
Description: Drift-aware multi-recorder audio synchronisation
 StageSync aligns several recordings of the same concert or play against
 one reference recording, measures the clock drift of each recorder, and
 renders drift-corrected audio.
 .
 The licence and third-party notice bundle is installed in
 /usr/share/doc/stagesync/notices.
CONTROL

deb="${dist_dir}/stagesync-${version}-linux-${file_arch}.deb"
if command -v dpkg-deb > /dev/null 2>&1; then
  rm -f "${deb}"
  echo "==> building the Debian package"
  dpkg-deb --build --root-owner-group "${deb_root}" "${deb}"
else
  echo "note: dpkg-deb was not found, so the Debian package step was skipped."
fi

# --- the checksum manifest ------------------------------------------------------

checksums="${dist_dir}/SHA256SUMS.txt"
: > "${checksums}"
for artefact in "${appimage}" "${deb}" "${tarball}"; do
  if [ -f "${artefact}" ]; then
    # The manifest names the files as they are in the directory, so that
    # `sha256sum -c SHA256SUMS.txt` works when it is run there.
    ( cd "${dist_dir}" && sha256sum "$(basename "${artefact}")" ) >> "${checksums}"
  fi
done
if [ -s "${checksums}" ]; then
  mv "${checksums}" "${dist_dir}/SHA256SUMS.txt"
fi

# --- what was written ----------------------------------------------------------

echo ""
echo "StageSync Linux packaging finished."
echo "version         : ${version}"
echo "architecture    : ${file_arch} (Debian architecture: ${deb_arch})"
echo "app directory   : ${app_dir}"
echo "appdir tarball  : ${tarball}"
echo "notices         : ${notices_dir}"
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
echo "checksums       : ${dist_dir}/SHA256SUMS.txt"
echo "output directory: ${dist_dir}"
