#!/usr/bin/env bash
#
# Build the StageSync application bundle for macOS.
#
# Produces, in dist/macos:
#
#   StageSync.app                                     the application bundle
#   stagesync-<version>-macos-<arch>.app.tar.gz        the bundle, archived
#   stagesync-<version>-macos-<arch>.dmg               a disk image
#   SHA256SUMS.txt                                    the SHA-256 of the artefacts
#
# The bundle is built for the architecture of the machine this script runs on.
# There is no cross-compilation here: `uname -m` reports what was built.
#
# The bundle is *not* signed and *not* notarised. macOS Gatekeeper will
# quarantine it on first launch until the user allows it explicitly.
#
# The licence and third-party notice bundle travels inside the bundle, in
# Contents/Resources/notices, so it is in the archive and in the disk image
# as well.
#
# Requirements: the Rust toolchain, and `hdiutil` for the disk image (which is
# part of macOS). Nothing else is needed - no Xcode project, no codesign.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

app_name="StageSync"
bundle_id="org.stagesync.StageSync"
# The oldest macOS this bundle claims to run on. eframe/winit need 10.12 at
# the bare minimum; 11.0 is the oldest release that still receives security
# updates, so that is the floor the bundle declares.
minimum_macos="11.0"

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

# --- the architecture ----------------------------------------------------------

arch="$(uname -m)"
case "${arch}" in
  arm64) rust_target="aarch64-apple-darwin" ;;
  x86_64) rust_target="x86_64-apple-darwin" ;;
  *) rust_target="${arch}-apple-darwin" ;;
esac
echo "==> building the stagesync-desktop binary (macOS, ${arch})"

# --- the build -----------------------------------------------------------------

cargo build --release --locked --bin stagesync-desktop

binary="${repo_root}/target/release/stagesync-desktop"
if [ ! -f "${binary}" ]; then
  echo "error: the built binary was not found at '${binary}'" >&2
  exit 1
fi

# --- the notices bundle ----------------------------------------------------------
#
# The notices are generated from Cargo.lock, so they always describe the
# dependency graph that produced this binary.

dist_dir="${repo_root}/dist/macos"
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
  --target "${rust_target}" \
  --output "${notices_dir}"

for notice in LICENSE-MIT LICENSE-APACHE LICENSE-MPL-2.0; do
  if [ ! -f "${repo_root}/packaging/notices/${notice}" ]; then
    echo "error: the licence text '${notice}' was not found" >&2
    exit 1
  fi
  install -m 0644 "${repo_root}/packaging/notices/${notice}" "${notices_dir}/${notice}"
done

# --- the bundle ----------------------------------------------------------------

app_dir="${dist_dir}/${app_name}.app"
contents_dir="${app_dir}/Contents"

rm -rf "${app_dir}"
mkdir -p "${contents_dir}/MacOS" "${contents_dir}/Resources"

cp "${binary}" "${contents_dir}/MacOS/${app_name}"
chmod +x "${contents_dir}/MacOS/${app_name}"

# The notices travel inside the bundle, so they are in the archive and in the
# disk image as well as in the installed application.
mkdir -p "${contents_dir}/Resources/notices"
cp -R "${notices_dir}/." "${contents_dir}/Resources/notices/"

icon_key=""
icon_source="${repo_root}/assets/icon.icns"
if [ -f "${icon_source}" ]; then
  cp "${icon_source}" "${contents_dir}/Resources/icon.icns"
  icon_key="  <key>CFBundleIconFile</key>
  <string>icon</string>"
else
  echo "note: assets/icon.icns was not found; the bundle is built without an icon."
fi

cat > "${contents_dir}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>${app_name}</string>
  <key>CFBundleDisplayName</key>
  <string>${app_name}</string>
  <key>CFBundleExecutable</key>
  <string>${app_name}</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_id}</string>
${icon_key}
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>LSMinimumSystemVersion</key>
  <string>${minimum_macos}</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.music</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSHumanReadableCopyright</key>
  <string>StageSync contributors</string>
</dict>
</plist>
PLIST

# --- the archive ----------------------------------------------------------------
#
# The archive is reproducible: every file in the bundle gets the same
# modification time, the owner is fixed, and gzip is told not to write a
# timestamp of its own. macOS ships bsdtar, which has no --sort, so the
# GNU-only flags are used only where they exist - and the difference is
# reported rather than passed over in silence.

tar_version="$(tar --version 2>/dev/null || true)"
tar_is_gnu=0
case "${tar_version}" in
  *"GNU tar"*) tar_is_gnu=1 ;;
esac

find "${app_dir}" -exec touch -h -t 202001010000 {} + 2>/dev/null || \
  find "${app_dir}" -exec touch -t 202001010000 {} +

tarball="${dist_dir}/stagesync-${version}-macos-${arch}.app.tar.gz"
rm -f "${tarball}"

if [ "${tar_is_gnu}" -eq 1 ]; then
  tar --sort=name "--mtime=UTC 2020-01-01" --owner=0 --group=0 --numeric-owner \
    -cf - -C "${dist_dir}" "${app_name}.app" | gzip -n -9 > "${tarball}"
else
  echo "note: this system's tar is not GNU tar, so the archive is written without the" \
    "reproducibility flags (--sort, --mtime, --owner, --group); its bytes will differ" \
    "between machines."
  tar -cf - -C "${dist_dir}" "${app_name}.app" | gzip -n -9 > "${tarball}"
fi

# --- the disk image -------------------------------------------------------------

dmg="${dist_dir}/stagesync-${version}-macos-${arch}.dmg"
if command -v hdiutil > /dev/null 2>&1; then
  rm -f "${dmg}"
  # A UDZO disk image records its own creation time, so the image's bytes are
  # not reproducible even when its contents are. The contents are.
  hdiutil create -volname "${app_name}" -srcfolder "${app_dir}" -ov -format UDZO "${dmg}"
else
  echo "note: hdiutil was not found, so the DMG step was skipped."
  echo "      The application bundle and its .app.tar.gz were produced."
fi

# --- the checksum manifest ------------------------------------------------------

checksums="${dist_dir}/SHA256SUMS.txt"
: > "${checksums}"
for artefact in "${tarball}" "${dmg}"; do
  if [ -f "${artefact}" ]; then
    ( cd "${dist_dir}" && sha256sum "$(basename "${artefact}")" ) >> "${checksums}"
  fi
done

# --- what was written -----------------------------------------------------------

echo ""
echo "StageSync macOS packaging finished."
echo "version        : ${version}"
echo "architecture   : ${arch}"
echo "application    : ${app_dir}"
echo "info plist     : ${contents_dir}/Info.plist"
echo "notices        : ${contents_dir}/Resources/notices"
echo "archive        : ${tarball}"
if [ -f "${dmg}" ]; then
  echo "disk image     : ${dmg}"
else
  echo "disk image     : (not produced - hdiutil was not available)"
fi
echo "checksums      : ${checksums}"
echo "output directory: ${dist_dir}"
