#!/usr/bin/env bash
#
# Build the StageSync application bundle for macOS.
#
# Produces, in dist/macos:
#
#   StageSync.app                                     the application bundle
#   stagesync-<version>-macos-<arch>.app.tar.gz        the bundle, archived
#   stagesync-<version>-macos-<arch>.dmg               a disk image
#
# The bundle is built for the architecture of the machine this script runs on.
# There is no cross-compilation here: `uname -m` reports what was built.
#
# The bundle is *not* signed and *not* notarised. macOS Gatekeeper will
# quarantine it on first launch until the user allows it explicitly.
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

# --- the architecture ----------------------------------------------------------

arch="$(uname -m)"
echo "==> building the stagesync-desktop binary (macOS, ${arch})"

# --- the build -----------------------------------------------------------------

cargo build --release --bin stagesync-desktop

binary="${repo_root}/target/release/stagesync-desktop"
if [ ! -f "${binary}" ]; then
  echo "error: the built binary was not found at '${binary}'" >&2
  exit 1
fi

# --- the bundle ----------------------------------------------------------------

dist_dir="${repo_root}/dist/macos"
app_dir="${dist_dir}/${app_name}.app"
contents_dir="${app_dir}/Contents"

rm -rf "${app_dir}"
mkdir -p "${contents_dir}/MacOS" "${contents_dir}/Resources"

cp "${binary}" "${contents_dir}/MacOS/${app_name}"
chmod +x "${contents_dir}/MacOS/${app_name}"

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
# GNU tar is asked for a reproducible archive: the entries are sorted, every
# timestamp is pinned and the owner is fixed. macOS ships bsdtar, which has no
# --sort, so the flags are used only where they exist - and the difference is
# reported rather than passed over in silence.

tar_version="$(tar --version 2>/dev/null || true)"
tar_is_gnu=0
case "${tar_version}" in
  *"GNU tar"*) tar_is_gnu=1 ;;
esac

tarball="${dist_dir}/stagesync-${version}-macos-${arch}.app.tar.gz"
rm -f "${tarball}"

if [ "${tar_is_gnu}" -eq 1 ]; then
  tar --sort=name "--mtime=UTC 2020-01-01" --owner=0 --group=0 --numeric-owner \
    -czf "${tarball}" -C "${dist_dir}" "${app_name}.app"
else
  echo "note: this system's tar is not GNU tar, so the archive is written without the" \
    "reproducibility flags (--sort, --mtime, --owner, --group); its bytes will differ" \
    "between machines."
  tar -czf "${tarball}" -C "${dist_dir}" "${app_name}.app"
fi

# --- the disk image -------------------------------------------------------------

dmg="${dist_dir}/stagesync-${version}-macos-${arch}.dmg"
if command -v hdiutil > /dev/null 2>&1; then
  rm -f "${dmg}"
  hdiutil create -volname "${app_name}" -srcfolder "${app_dir}" -ov -format UDZO "${dmg}"
else
  echo "note: hdiutil was not found, so the DMG step was skipped."
  echo "      The application bundle and its .app.tar.gz were produced."
fi

# --- what was written -----------------------------------------------------------

echo ""
echo "StageSync macOS packaging finished."
echo "version        : ${version}"
echo "architecture   : ${arch}"
echo "application    : ${app_dir}"
echo "info plist     : ${contents_dir}/Info.plist"
echo "archive        : ${tarball}"
if [ -f "${dmg}" ]; then
  echo "disk image     : ${dmg}"
else
  echo "disk image     : (not produced - hdiutil was not available)"
fi
echo "output directory: ${dist_dir}"
