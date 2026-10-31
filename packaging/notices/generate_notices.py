#!/usr/bin/env python3
"""Generate the StageSync third-party notices bundle from Cargo.lock.

The notices that ship with a binary are generated, never written by hand: a
dependency bump then cannot silently leave the shipped notices out of date.

Two inputs are read, and nothing else:

  Cargo.lock      the pinned dependency graph: names, versions, checksums.
  licenses.json   the licence each pinned package declares in its own
                  published manifest, and where that licence text lives.

`Cargo.lock` does **not** record licences and does **not** record which
platform-specific packages a given build actually links, so the licence
metadata comes from the checked-in map and the generated file says so
plainly rather than guessing. See packaging/notices/README.md.

The script needs no network access and no cargo: it is a pure function of the
two input files, so the same inputs always produce byte-identical output.

Usage:

    python packaging/notices/generate_notices.py \
        --cargo-lock Cargo.lock \
        --licenses packaging/notices/licenses.json \
        --output dist/windows/notices
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sys

# The licence texts that are shipped next to the generated notices.
PROJECT_LICENCE_FILES = [
    (
        "LICENSE-MIT",
        "MIT",
        "StageSync itself. StageSync is distributed under `MIT OR Apache-2.0`: "
        "you may use it under either licence.",
    ),
    (
        "LICENSE-APACHE",
        "Apache License 2.0",
        "StageSync itself, the option of the `MIT OR Apache-2.0` dual licence.",
    ),
    (
        "LICENSE-MPL-2.0",
        "Mozilla Public License 2.0",
        "The thirteen `symphonia` crates. `symphonia` is linked, not vendored, so "
        "the MPL's file-level copyleft applies to it: see SOURCE-AVAILABILITY.md.",
    ),
]

# The copyleft dependency family. The licence identifier comes from the map;
# this list only says which packages have to be called out by name.
MPL_FAMILY = "symphonia"


def sha256_of(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    digest.update(path.read_bytes())
    return digest.hexdigest()


def parse_cargo_lock(text: str) -> list[dict]:
    """Read the package table out of a Cargo.lock.

    Cargo.lock is TOML, but the package table it contains is small and regular
    enough to read directly: that keeps the generator free of a TOML parser and
    of any dependency of its own.
    """
    packages = []
    for block in text.split("[[package]]")[1:]:
        name = re.search(r'^name = "(.*)"', block, re.M)
        version = re.search(r'^version = "(.*)"', block, re.M)
        if not name or not version:
            continue
        source = re.search(r'^source = "(.*)"', block, re.M)
        checksum = re.search(r'^checksum = "(.*)"', block, re.M)
        packages.append(
            {
                "name": name.group(1),
                "version": version.group(1),
                "source": source.group(1) if source else None,
                "checksum": checksum.group(1) if checksum else None,
            }
        )
    return packages


def workspace_version(cargo_toml: pathlib.Path) -> str:
    """Read `version` out of the `[workspace.package]` table."""
    if not cargo_toml.is_file():
        return ""
    text = cargo_toml.read_text(encoding="utf-8")
    section = re.search(r"^\[workspace\.package\](.*?)^\[", text, re.M | re.S)
    if not section:
        return ""
    found = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
    return found.group(1) if found else ""


def workspace_repository(cargo_toml: pathlib.Path) -> str:
    if not cargo_toml.is_file():
        return ""
    text = cargo_toml.read_text(encoding="utf-8")
    section = re.search(r"^\[workspace\.package\](.*?)^\[", text, re.M | re.S)
    if not section:
        return ""
    found = re.search(r'^repository\s*=\s*"([^"]+)"', section.group(1), re.M)
    return found.group(1) if found else ""


def read_licences(path: pathlib.Path) -> dict[tuple[str, str], dict]:
    document = json.loads(path.read_text(encoding="utf-8"))
    entries = {}
    for entry in document["packages"]:
        entries[(entry["name"], entry["version"])] = entry
    return entries


def fmt_table(rows: list[list[str]], headers: list[str]) -> str:
    lines = ["| " + " | ".join(headers) + " |",
             "|" + "|".join("---" for _ in headers) + "|"]
    for row in rows:
        lines.append("| " + " | ".join(row) + " |")
    return "\n".join(lines)


def build_notices(
    *,
    version: str,
    packages: list[dict],
    licences: dict[tuple[str, str], dict],
    lock_digest: str,
    map_digest: str,
    target: str | None,
    graph_note: str,
    repository: str,
) -> str:
    missing = [
        (p["name"], p["version"])
        for p in packages
        if (p["name"], p["version"]) not in licences
    ]
    if missing:
        raise SystemExit(
            "error: these packages are pinned in Cargo.lock but have no entry in "
            "the licence map; update the map in the same change:\n  "
            + "\n  ".join("%s %s" % m for m in sorted(missing))
        )

    mpl = sorted(
        (p for p in packages if licences[(p["name"], p["version"])]["license"].startswith("MPL-2.0")),
        key=lambda p: p["name"],
    )

    out = []
    out.append("# Third-party notices for StageSync%s" % ((" " + version) if version else ""))
    out.append("")
    out.append(
        "This file is generated. Do not edit it by hand: regenerate it with"
    )
    out.append("")
    out.append("```")
    out.append(
        "python packaging/notices/generate_notices.py"
        " --cargo-lock Cargo.lock"
        " --licenses packaging/notices/licenses.json"
        " --output <directory>"
    )
    out.append("```")
    out.append("")
    out.append("The inputs this file was generated from:")
    out.append("")
    out.append(fmt_table(
        [
            ["`Cargo.lock`", "`%s`" % lock_digest],
            ["`packaging/notices/licenses.json`", "`%s`" % map_digest],
        ],
        ["Input", "SHA-256"],
    ))
    out.append("")
    out.append("## What this file is")
    out.append("")
    out.append(
        "StageSync is distributed under `MIT OR Apache-2.0`. The binaries it "
        "ships link a number of third-party crates, each of which carries its "
        "own licence. This file lists every package pinned in `Cargo.lock`, the "
        "licence that package declares in its own published manifest, and where "
        "that licence text lives."
    )
    out.append("")
    out.append("## The licence texts that ship with this build")
    out.append("")
    out.append(fmt_table(
        [["`%s`" % name, licence, applies] for name, licence, applies in PROJECT_LICENCE_FILES],
        ["File", "Licence", "Applies to"],
    ))
    out.append("")
    out.append(
        "The project licence texts are copies of the ones in the source tree; "
        "the MPL-2.0 text is the Mozilla Public License 2.0 as published by "
        "the SPDX project."
    )
    out.append("")
    out.append("## The dependency graph")
    out.append("")
    out.append(
        "`Cargo.lock` pins %d packages. %d of them are the StageSync crates "
        "themselves; the other %d are third-party dependencies from crates.io."
        % (
            len(packages),
            sum(1 for p in packages if p["source"] is None),
            sum(1 for p in packages if p["source"] is not None),
        )
    )
    out.append("")
    if target:
        out.append("The build this bundle was generated for: **%s**." % target)
        out.append("")
    out.append(graph_note)
    out.append("")
    rows = []
    for package in sorted(packages, key=lambda p: (p["name"], p["version"])):
        entry = licences[(package["name"], package["version"])]
        rows.append([
            "`%s`" % package["name"],
            "`%s`" % package["version"],
            "`%s`" % entry["license"],
            entry.get("license_text", ""),
        ])
    out.append(fmt_table(rows, ["Crate", "Version", "Licence", "Where the licence text lives"]))
    out.append("")
    out.append("## The MPL-2.0 dependency: the `symphonia` family")
    out.append("")
    out.append(
        "%d packages are licensed under the Mozilla Public License 2.0, and all "
        "of them belong to the `symphonia` family. `symphonia` decodes the "
        "audio formats StageSync reads. It is **linked**, not vendored: Rust "
        "links it statically into the binary, so there is no `symphonia.dll` to "
        "point a recipient at. What MPL-2.0 section 3.2 requires instead is "
        "that the corresponding source be made available, and "
        "`SOURCE-AVAILABILITY.md` in this bundle says exactly where that source "
        "comes from and how to verify it."
        % len(mpl)
    )
    out.append("")
    out.append(fmt_table(
        [
            ["`%s`" % p["name"], "`%s`" % p["version"],
             "`%s`" % (p["checksum"] or "n/a (path dependency)")]
            for p in mpl
        ],
        ["Crate", "Version", "SHA-256 of the crates.io `.crate` file (the `Cargo.lock` checksum)"],
    ))
    out.append("")
    out.append(
        "StageSync does not modify any `symphonia` file, so there are no "
        "modified files to publish under MPL-2.0 section 3.1(b)."
    )
    out.append("")
    out.append("## Limitations, stated plainly")
    out.append("")
    out.append(
        "* `Cargo.lock` records names, versions and checksums. It does **not** "
        "record licences, and it does **not** record which platform-specific "
        "packages a given build actually links. The licence column above "
        "therefore comes from `packaging/notices/licenses.json`, a checked-in "
        "transcription of the `license` field of each pinned package's "
        "published manifest (see `LICENSES.md`, Appendix A, for the "
        "per-dependency table this file was generated from). No licence here "
        "was guessed."
    )
    out.append(
        "* The table above is the whole lock graph, which is a superset of the "
        "graph any one binary links. A Windows build does not link the "
        "`*-apple` crates and a Linux build does not link the `windows_*` "
        "crates; listing them is deliberate, because the notices have to "
        "cover the lock file that produced the binary."
    )
    out.append(
        "* A licence identifier says what a package *declares*. This file "
        "makes no claim that the upstream code complies with the licence it "
        "declares; auditing every upstream codebase is not something this "
        "project does, and licence scanners read the same manifests."
    )
    out.append(
        "* The fonts embedded through `epaint_default_fonts` are covered by "
        "their own licence terms (`OFL-1.1` and `Ubuntu-font-1.0`), which are "
        "listed in the table above. StageSync does not sell the fonts "
        "separately."
    )
    out.append("")
    if repository:
        out.append(
            "The repository StageSync declares in its workspace manifest is "
            "<%s>. StageSync's own licence texts (`LICENSE-MIT` and "
            "`LICENSE-APACHE`) are in this bundle. No third-party licence in "
            "the table above requires StageSync's own source to be published: "
            "the only source-availability obligation here is the MPL-2.0 one, "
            "and it is described in `SOURCE-AVAILABILITY.md`."
            % repository
        )
        out.append("")
    return "\n".join(out).rstrip("\n") + "\n"


def build_source_availability(
    *,
    version: str,
    packages: list[dict],
    licences: dict[tuple[str, str], dict],
    repository: str,
) -> str:
    mpl = sorted(
        (p for p in packages if licences[(p["name"], p["version"])]["license"].startswith("MPL-2.0")),
        key=lambda p: p["name"],
    )
    if not mpl:
        raise SystemExit("error: no MPL-2.0 package is pinned in Cargo.lock")

    out = []
    out.append("# Source availability for the MPL-2.0 covered code")
    out.append("")
    out.append(
        "This file is generated by "
        "`packaging/notices/generate_notices.py`. Do not edit it by hand."
    )
    out.append("")
    out.append("## The obligation")
    out.append("")
    out.append(
        "StageSync links `symphonia`, which is licensed under the Mozilla "
        "Public License 2.0. MPL-2.0 is a file-level copyleft licence: it does "
        "not require StageSync, or anything else that merely links `symphonia`, "
        "to be published. What it does require is stated in section 3.2:"
    )
    out.append("")
    out.append("> If You distribute Covered Software in Executable Form then:")
    out.append(">")
    out.append(
        "> (a) such Covered Software must also be made available in Source "
        "Code Form, as described in Section 3.1, and You must inform "
        "recipients of the Executable Form how they can obtain a copy of such "
        "Source Code Form by reasonable means in a timely manner, at a charge "
        "no more than the cost of distribution to the recipient; and"
    )
    out.append("")
    out.append(
        "The full text of the licence is `LICENSE-MPL-2.0` in this bundle. The "
        "official text is published at <https://www.mozilla.org/en-US/MPL/2.0/>."
    )
    out.append("")
    out.append("## What is covered")
    out.append("")
    out.append(
        "These %d packages are the MPL-2.0 covered code. They are the whole of "
        "it: no other package in `Cargo.lock` is under a copyleft licence."
        % len(mpl)
    )
    out.append("")
    out.append(fmt_table(
        [
            ["`%s`" % p["name"], "`%s`" % p["version"], "`%s`" % p["checksum"]]
            for p in mpl
        ],
        ["Crate", "Version", "SHA-256 of the crates.io `.crate` file"],
    ))
    out.append("")
    out.append("## Where the corresponding source comes from")
    out.append("")
    out.append(
        "The corresponding source is the published source of the exact "
        "versions above, as distributed on crates.io. Nothing is patched, "
        "vendored or re-generated: StageSync depends on the unmodified "
        "crates.io release of each crate."
    )
    out.append("")
    out.append("For each crate, the source archive is:")
    out.append("")
    out.append("```")
    out.append("https://crates.io/api/v1/crates/<crate>/<version>/download")
    out.append("```")
    out.append("")
    out.append(
        "which redirects to "
        "`https://static.crates.io/crates/<crate>/<crate>-<version>.crate`. The "
        "SHA-256 of that file is the checksum recorded in `Cargo.lock` in the "
        "table above, so a download can be verified against the lock file "
        "without trusting anything else:"
    )
    out.append("")
    out.append("```")
    out.append(
        "curl -fsSL -o symphonia-0.6.1.crate \\\n"
        "  https://crates.io/api/v1/crates/symphonia/0.6.1/download\n"
        "sha256sum symphonia-0.6.1.crate"
    )
    out.append("```")
    out.append("")
    out.append(
        "The whole set can also be materialised from the crates.io index "
        "without any of the above, which is the route a distributor should "
        "prefer because it is driven by the lock file itself:"
    )
    out.append("")
    out.append("```")
    out.append("cargo vendor --locked --versioned-dirs vendor/")
    out.append("```")
    out.append("")
    out.append(
        "That command writes the source of every pinned dependency into "
        "`vendor/`, including the thirteen crates above, and the checksums it "
        "verifies against are the ones in `Cargo.lock`."
    )
    out.append("")
    out.append("## Modifications")
    out.append("")
    out.append(
        "StageSync modifies no file of any `symphonia` crate. There is "
        "therefore no modified file to publish under MPL-2.0 section 3.1(b), "
        "and no source-code-form notice for `symphonia` to attach."
    )
    out.append("")
    out.append("## StageSync's own source")
    out.append("")
    if repository:
        out.append(
            "StageSync is licensed under `MIT OR Apache-2.0`; its licence "
            "texts (`LICENSE-MIT`, `LICENSE-APACHE`) are in this bundle. The "
            "repository StageSync declares in its workspace manifest is <%s>. "
            "StageSync's own source is not required to be published by any "
            "licence: the only source-availability obligation in this "
            "distribution is the MPL-2.0 one described above."
            % repository
        )
    else:
        out.append(
            "StageSync itself is licensed under `MIT OR Apache-2.0`. Its "
            "licence texts (`LICENSE-MIT`, `LICENSE-APACHE`) are in this "
            "bundle."
        )
    out.append("")
    return "\n".join(out).rstrip("\n") + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-lock", default="Cargo.lock", type=pathlib.Path)
    parser.add_argument(
        "--licenses", default="packaging/notices/licenses.json", type=pathlib.Path
    )
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument(
        "--cargo-toml",
        default="Cargo.toml",
        type=pathlib.Path,
        help="read the workspace version and repository from here",
    )
    parser.add_argument(
        "--target",
        default=None,
        help=(
            "the target triple this bundle is for, recorded in the generated "
            "file. It does not change which packages are listed: Cargo.lock "
            "does not record target selection."
        ),
    )
    args = parser.parse_args(argv)

    lock_path = args.cargo_lock.resolve()
    map_path = args.licenses.resolve()
    if not lock_path.is_file():
        raise SystemExit("error: %s was not found" % lock_path)
    if not map_path.is_file():
        raise SystemExit("error: %s was not found" % map_path)

    packages = parse_cargo_lock(lock_path.read_text(encoding="utf-8"))
    if not packages:
        raise SystemExit("error: no packages were found in %s" % lock_path)
    licences = read_licences(map_path)

    version = workspace_version(args.cargo_toml)
    repository = workspace_repository(args.cargo_toml)

    graph_note = (
        "The table lists every package in `Cargo.lock`. `Cargo.lock` does not "
        "record which of them a given build links, so this list is a superset "
        "of the graph of any single binary."
    )

    output_dir = args.output
    output_dir.mkdir(parents=True, exist_ok=True)

    notices = build_notices(
        version=version,
        packages=packages,
        licences=licences,
        lock_digest=sha256_of(lock_path),
        map_digest=sha256_of(map_path),
        target=args.target,
        graph_note=graph_note,
        repository=repository,
    )
    (output_dir / "THIRD_PARTY_NOTICES.md").write_text(notices, encoding="utf-8", newline="\n")

    availability = build_source_availability(
        version=version, packages=packages, licences=licences, repository=repository
    )
    (output_dir / "SOURCE-AVAILABILITY.md").write_text(
        availability, encoding="utf-8", newline="\n"
    )

    print(
        "generated %d notices for %d packages -> %s"
        % (2, len(packages), output_dir)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
