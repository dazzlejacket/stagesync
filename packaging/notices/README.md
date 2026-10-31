# The StageSync notices bundle

This directory holds the licence and notice material that ships with every
StageSync binary: the portable Windows ZIP, the NSIS installer, the macOS
application bundle and disk image, the Linux AppImage and the Debian package.

`LICENSES.md` in the repository root is a *source-tree* document: a per-crate
licence table with an explanation of what each licence obliges a distributor
to do. It is not a notices bundle, and it is not what a recipient of a binary
receives. This directory is.

## What is in the bundle

| File | What it is |
|---|---|
| `LICENSE-MIT` | A copy of the MIT licence from the repository root. |
| `LICENSE-APACHE` | A copy of the Apache License 2.0 from the repository root. |
| `LICENSE-MPL-2.0` | The Mozilla Public License 2.0, the licence of the `symphonia` crates. |
| `THIRD_PARTY_NOTICES.md` | **Generated.** Every package pinned in `Cargo.lock`, the licence it declares, and where that licence text lives. |
| `SOURCE-AVAILABILITY.md` | **Generated.** The MPL-2.0 section 3.2 obligation, the exact `symphonia` versions, and where their corresponding source comes from. |
| `licenses.json` | The licence map the generator reads: the `license` field of each pinned package's published manifest, transcribed from `LICENSES.md` Appendix A. |
| `generate_notices.py` | The generator. |

The two generated files are checked in so that the bundle can be read without
running anything. They are a snapshot, not the source of truth: the packaging
scripts regenerate them for every build, and the copies that ship are always
the fresh ones. A stale checked-in copy would show up as a different SHA-256
in the "inputs this file was generated from" table at the top of the file.

The copies are copies: `LICENSE-MIT` and `LICENSE-APACHE` stay in the
repository root as well, because that is where the source tree's own licence
documents live.

## Regenerating the notices

```text
python packaging/notices/generate_notices.py \
    --cargo-lock Cargo.lock \
    --licenses packaging/notices/licenses.json \
    --output dist/windows/notices
```

The generator is a pure function of its two inputs. It needs no network
access, no cargo and no third-party Python package, and it embeds no
timestamp, so the same inputs always produce byte-identical output. The
packaging scripts call it for every build, so the notices in a release always
match the `Cargo.lock` that produced the binary.

## The one thing `Cargo.lock` does not carry

`Cargo.lock` records package names, versions and checksums. It does **not**
record licences, and it does **not** record which platform-specific packages a
given build actually links. The generator therefore needs `licenses.json`, and
`THIRD_PARTY_NOTICES.md` says so in its own "Limitations, stated plainly"
section rather than presenting a guess as a fact.

`licenses.json` is a checked-in transcription of the `license` field of each
pinned package's published manifest. `LICENSES.md` Appendix A was read from
those manifests, and this file is the machine-readable form of that table.

**When a dependency is added, upgraded or removed, `licenses.json` has to be
updated in the same change.** The generator fails loudly, listing the packages
it has no licence for, rather than shipping a notice with a hole in it:

```text
error: these packages are pinned in Cargo.lock but have no entry in the
licence map; update the map in the same change:
  some-crate 1.2.3
```

## The MPL-2.0 dependency

`symphonia` (and its twelve sub-crates) is licensed under MPL-2.0. It is
*linked*, not vendored, so there is no `symphonia.dll` for a recipient to ask
about; what MPL-2.0 section 3.2 requires is that the corresponding source be
made available and that recipients be told how to obtain it.
`SOURCE-AVAILABILITY.md` gives the exact route: the crates.io source archive
of each pinned version, the SHA-256 that `Cargo.lock` records for it (which is
the SHA-256 of that archive), and the `cargo vendor` command that materialises
the lot from the lock file.

StageSync modifies no `symphonia` file, so there is no modified file to
publish under MPL-2.0 section 3.1(b).
