#!/usr/bin/env bash
# Exercise the production checksum section without a GUI build or package tools.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dist_dir="$(mktemp -d)"
cleanup() {
  rm -f -- "${dist_dir}/test.AppImage" "${dist_dir}/test.deb" \
    "${dist_dir}/test.tar.gz" "${dist_dir}/SHA256SUMS.txt"
  rmdir -- "${dist_dir}"
}
trap cleanup EXIT
appimage="${dist_dir}/test.AppImage"
deb="${dist_dir}/test.deb"
tarball="${dist_dir}/test.tar.gz"
printf 'app image fixture' > "${appimage}"
printf 'deb fixture' > "${deb}"
printf 'tar fixture' > "${tarball}"
# Run the actual build-script section so a failure at the end cannot hide
# behind successful compilation or artifact creation.
section="$(sed -n '/^checksums=/,/^# --- what was written/p' "${script_dir}/build.sh")"
test -n "${section}"
eval "${section}"
test "$(wc -l < "${dist_dir}/SHA256SUMS.txt")" -eq 3
(cd "${dist_dir}" && sha256sum -c SHA256SUMS.txt)
# The optional-package path must work with only the mandatory tarball too.
rm -- "${appimage}" "${deb}"
eval "${section}"
test "$(wc -l < "${dist_dir}/SHA256SUMS.txt")" -eq 1
(cd "${dist_dir}" && sha256sum -c SHA256SUMS.txt)
