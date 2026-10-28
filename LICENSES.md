# LICENSES.md — StageSync's licence, and the licences of everything it depends on

This document covers four things:

1. **StageSync's own licence**, and how to choose between the two options. (§1)
2. **The licences of every dependency**, with the complete per-crate table in
   Appendix A. (§2, §3)
3. **What MIT, Apache-2.0 and MPL-2.0 actually require of you**, in plain English,
   including the one dependency that is not permissively licensed:
   [`symphonia` 0.6.1, which is MPL-2.0]. (§4)
4. **A checklist for whoever ships a binary.** (§5)

It is written for a person who has to ship a Windows build and wants to know what
obligations they are taking on. It is not legal advice.

---

## 1. StageSync's own licence

StageSync is licensed under **MIT OR Apache-2.0** — that is, you may use it under
*either* licence, and you may choose the one that suits you.

This is declared once, at the workspace level, in `Cargo.toml`:

```toml
[workspace.package]
license = "MIT OR Apache-2.0"
```

and is inherited by all three crates (`stagesync-core`, `stagesync-cli`,
`stagesync-desktop`). It is also stated in the manifest of every published crate in
this repository, which is what package managers and licence scanners read.

The full licence texts are in the repository root:

| File | Text |
|---|---|
| [`LICENSE-MIT`](LICENSE-MIT) | The MIT Licence, with the copyright line `Copyright (c) 2026 The StageSync contributors`. |
| [`LICENSE-APACHE`](LICENSE-APACHE) | The Apache License, Version 2.0, in full, including the appendix boilerplate. Byte-for-byte the file published by the Apache Software Foundation at <https://www.apache.org/licenses/LICENSE-2.0.txt>, with line endings normalised to LF to match the rest of this repository. |

### 1.1 Choosing between them

Both are permissive. Neither obliges you to open-source your own work, and neither
requires you to pay anything or to licence your own code compatibly.

Pick **MIT** if you want the shortest possible licence text and a single, simple
obligation.

Pick **Apache-2.0** if your organisation has a policy about patent grants, or wants
the explicit patent-licence and patent-litigation-termination terms in §3 of the
licence. Apache-2.0 also contains an express patent grant from each contributor,
whereas MIT contains no patent clause at all. If that matters to you, take
Apache-2.0.

Either way, your only obligations are to keep the copyright notice and the licence
text with the source, and to include the licence text when you distribute a binary.
See §4.1 and §4.2.

---

## 2. Dependency licences at a glance

The complete per-dependency table is **Appendix A** (§3). The summary:

| Licence | Packages | What it means for you |
|---|---|---|
| MIT / Apache-2.0 dual, in either order, plus the `/` and `,` spellings some manifests use | 289 | Permissive. Keep the notices. |
| MIT only | 98 | Permissive. Keep the notices. |
| Apache-2.0 only | 16 | Permissive. Keep the notices. Note `hound` is one of these. |
| Unicode-3.0 | 18 | Permissive (OSI-approved). Keep the notices. |
| MPL-2.0 | 13 | **Weak, file-level copyleft.** All thirteen are the `symphonia` family. See §4.3 and §5. |
| Zlib OR Apache-2.0 OR MIT | 11 | Permissive. Take the MIT or Apache-2.0 option. |
| Unlicense OR MIT | 8 | Permissive. Take the MIT option. |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 6 | The LLVM exception removes Apache-2.0's copyleft effect. Take the MIT or plain-Apache-2.0 option. |
| MIT OR Apache-2.0 OR Zlib | 6 | Permissive. Take the MIT or Apache-2.0 option. |
| Zlib | 4 | Permissive. Keep the notices. |
| BSD-3-Clause (alone, or OR MIT/Apache-2.0, or OR Apache-2.0) | 6 | Permissive. Keep the notices. |
| BSD-2-Clause OR Apache-2.0 OR MIT | 2 | Permissive. Take the MIT or Apache-2.0 option. |
| BSD-2-Clause | 1 | Permissive. Keep the notices. |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | 2 | LGPL is a weak copyleft, but you do not have to take it: take the MIT/Apache-2.0 option. (`r-efi` 5.3.0 and 6.0.0.) |
| BSL-1.0 | 2 | Permissive. Keep the notices. |
| Unlicense/MIT | 2 | Permissive. Take the MIT option. |
| MIT OR Zlib OR Apache-2.0 | 2 | Permissive. Take the MIT or Apache-2.0 option. |
| ISC | 1 | Permissive. Keep the notices. |
| 0BSD OR MIT OR Apache-2.0 | 1 | Permissive. Take the MIT or Apache-2.0 option. |
| Apache-2.0 OR GPL-2.0-only | 1 | Take the Apache-2.0 option. (`self_cell` 1.3.0.) |
| Apache-2.0 AND MIT | 1 | Both notices apply. (`dpi` 0.1.2.) |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1 | Both notices apply. (`unicode-ident` 1.0.27.) |
| (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | 1 | Font licence conditions apply on top. (`epaint_default_fonts` 0.37.0.) See §4.5. |
| **Total** | **492** | |

**Nothing in the dependency graph is under a licence that would require StageSync, or
anything you build with it, to be published as source code.** The single copyleft
dependency, `symphonia`, is MPL-2.0, which is weak *file-level* copyleft: it does
not reach your own files, but it does oblige you to make its source available (§4.3).

### 2.1 What "verified" means in this document

Every one of the 492 packages pinned in `Cargo.lock` declares a licence in the
`license` field of its published `Cargo.toml` manifest, and all 492 were read from
the local cargo registry cache. **No licence in Appendix A is marked "unverified",
because there is no package in the graph for which the licence could not be stated
from the manifest.**

That is a narrower claim than it may sound, and it is worth being precise:

- **What is verified:** the licence each upstream project *declares* for the exact
  version pinned here.
- **What is not verified:** that the upstream code actually complies with the
  licence it declares. Auditing 492 third-party codebases is not something this
  repository can do, and no claim of that kind is made. This is the normal state of
  affairs for every project that depends on crates.io packages; licence scanners
  read the same manifests and reach the same conclusion.

If you need the *actual* licence text for a specific crate, Appendix A tells you
exactly where to find it: either in the published crate package (430 of the 492
packages ship a `LICENSE`-style file — the table names the exact file) or, for the
remaining 62, in the upstream repository, which the table also links.

---

## 3. Appendix A — per-dependency licence table

Every package pinned in `Cargo.lock`, with the exact resolved version, the licence
identifier declared in that package's own manifest, and where the licence text
lives.

To regenerate or re-check this table:

```text
cargo tree --locked --offline --target x86_64-pc-windows-msvc --edges normal --prefix none --workspace
```

and read the `license` field from each crate's `Cargo.toml` in
`%USERPROFILE%\.cargo\registry\src\<registry-hash>\<crate>-<version>\Cargo.toml`,
or from `https://crates.io/crates/<crate>`.

| Crate | Version | Licence | Where the licence text lives |
|---|---|---|---|---|
| `ab_glyph` | `0.2.32` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/alexheretic/ab-glyph> |
| `ab_glyph_rasterizer` | `0.1.10` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/alexheretic/ab-glyph> |
| `accesskit` | `0.25.2` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `accesskit_atspi_common` | `0.21.1` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `accesskit_consumer` | `0.39.2` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `accesskit_ios` | `0.2.2` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `accesskit_macos` | `0.27.2` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `accesskit_unix` | `0.24.1` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `accesskit_windows` | `0.35.2` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `accesskit_winit` | `0.34.2` | `Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/AccessKit/accesskit> |
| `adler2` | `2.0.1` | `0BSD OR MIT OR Apache-2.0` | Published crate package: `LICENSE-0BSD`, `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/oyvindln/adler2> |
| `ahash` | `0.8.12` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/tkaitchuck/ahash> |
| `aho-corasick` | `1.1.5` | `Unlicense OR MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/aho-corasick> |
| `allocator-api2` | `0.2.21` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/zakarumych/allocator-api2> |
| `android_system_properties` | `0.1.6` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/nical/android_system_properties> |
| `android-activity` | `0.6.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE`, `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-mobile/android-activity> |
| `android-properties` | `0.2.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/miklelappo/android-properties> |
| `anstream` | `1.0.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle` | `1.0.14` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle-parse` | `1.0.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle-query` | `1.1.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle-wincon` | `3.0.11` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anyhow` | `1.0.104` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/anyhow> |
| `arboard` | `3.7.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE.txt`, `LICENSE-MIT.txt` &mdash; upstream repository: <https://github.com/1Password/arboard> |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/droundy/arrayref> |
| `arrayvec` | `0.7.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bluss/arrayvec> |
| `ash` | `0.38.0+1.3.281` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/ash-rs/ash> |
| `as-raw-xcb-connection` | `1.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/psychon/as-raw-xcb-connection> |
| `async-broadcast` | `0.7.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-broadcast> |
| `async-channel` | `2.5.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-channel> |
| `async-executor` | `1.14.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-executor> |
| `async-io` | `2.6.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-io> |
| `async-lock` | `3.4.2` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-lock> |
| `async-process` | `2.5.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-process> |
| `async-recursion` | `1.2.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dcchut/async-recursion> |
| `async-signal` | `0.2.14` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-signal> |
| `async-task` | `4.7.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/async-task> |
| `async-trait` | `0.1.92` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/async-trait> |
| `atomic-waker` | `1.1.2` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-THIRD-PARTY` &mdash; upstream repository: <https://github.com/smol-rs/atomic-waker> |
| `atspi` | `0.29.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE2.txt`, `LICENSE-MIT.txt` &mdash; upstream repository: <https://github.com/odilia-app/atspi> |
| `atspi-common` | `0.13.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE2.txt`, `LICENSE-MIT.txt` &mdash; upstream repository: <https://github.com/odilia-app/atspi> |
| `atspi-proxies` | `0.13.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE2.txt`, `LICENSE-MIT.txt` &mdash; upstream repository: <https://github.com/odilia-app/atspi> |
| `autocfg` | `1.5.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/cuviper/autocfg> |
| `bitflags` | `1.3.2` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bitflags/bitflags> |
| `bitflags` | `2.13.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bitflags/bitflags> |
| `bit-set` | `0.10.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/contain-rs/bit-set> |
| `bit-vec` | `0.9.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/contain-rs/bit-vec> |
| `block2` | `0.5.1` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `block2` | `0.6.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `block-buffer` | `0.10.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/utils> |
| `blocking` | `1.7.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/blocking> |
| `bumpalo` | `3.20.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/fitzgen/bumpalo> |
| `bytemuck` | `1.25.2` | `Zlib OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/Lokathor/bytemuck> |
| `bytemuck_derive` | `1.12.1` | `Zlib OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/Lokathor/bytemuck> |
| `byteorder-lite` | `0.1.0` | `Unlicense OR MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/image-rs/byteorder-lite> |
| `bytes` | `1.12.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/tokio-rs/bytes> |
| `calloop` | `0.13.0` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/Smithay/calloop> |
| `calloop` | `0.14.5` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/Smithay/calloop> |
| `calloop-wayland-source` | `0.3.0` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/calloop-wayland-source> |
| `calloop-wayland-source` | `0.4.1` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/calloop-wayland-source> |
| `cc` | `1.10.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/cc-rs> |
| `cfg_aliases` | `0.2.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/katharostech/cfg_aliases> |
| `cfg-if` | `1.0.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/cfg-if> |
| `cgl` | `0.3.2` | `MIT / Apache-2.0` | Published crate package: `COPYING`, `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/cgl-rs> |
| `clap` | `4.6.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/clap-rs/clap> |
| `clap_builder` | `4.6.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/clap-rs/clap> |
| `clap_derive` | `4.6.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/clap-rs/clap> |
| `clap_lex` | `1.1.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/clap-rs/clap> |
| `clipboard-win` | `5.4.3` | `BSL-1.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/DoumanAsh/clipboard-win> |
| `codespan-reporting` | `0.13.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/brendanzab/codespan> |
| `color` | `0.3.3` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/color> |
| `colorchoice` | `1.0.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `combine` | `4.6.9` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/Marwes/combine> |
| `concurrent-queue` | `2.5.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/concurrent-queue> |
| `core-foundation` | `0.9.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/core-foundation-rs> |
| `core-foundation-sys` | `0.8.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/core-foundation-rs> |
| `core-graphics` | `0.23.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/core-foundation-rs> |
| `core-graphics-types` | `0.1.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/core-foundation-rs> |
| `cpufeatures` | `0.2.17` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/utils> |
| `crc32fast` | `1.5.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/srijs/rust-crc32fast> |
| `crossbeam-deque` | `0.8.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/crossbeam-rs/crossbeam> |
| `crossbeam-epoch` | `0.9.21` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/crossbeam-rs/crossbeam> |
| `crossbeam-utils` | `0.8.23` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/crossbeam-rs/crossbeam> |
| `crunchy` | `0.2.4` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/eira-fransham/crunchy> |
| `crypto-common` | `0.1.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/traits> |
| `cursor-icon` | `1.2.0` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/rust-windowing/cursor-icon> |
| `defmt` | `1.2.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/knurling-rs/defmt> |
| `defmt-macros` | `1.2.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/knurling-rs/defmt> |
| `defmt-parser` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/knurling-rs/defmt> |
| `digest` | `0.10.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/traits> |
| `dispatch` | `0.2.0` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <http://github.com/SSheldon/rust-dispatch> |
| `dispatch2` | `0.3.1` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `displaydoc` | `0.2.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/yaahc/displaydoc> |
| `dlib` | `0.5.3` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/elinorbgr/dlib> |
| `document-features` | `0.2.12` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/slint-ui/document-features> |
| `downcast-rs` | `1.2.1` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/marcianx/downcast-rs> |
| `dpi` | `0.1.2` | `Apache-2.0 AND MIT` | Published crate package: `LICENSE`, `LICENSE-LIBM-MIT` &mdash; upstream repository: <https://github.com/rust-windowing/winit> |
| `ecolor` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui> |
| `eframe` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/eframe> |
| `egui` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui> |
| `egui_glow` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui_glow> |
| `egui_system_font_provider` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui_system_font_provider> |
| `egui-wgpu` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui-wgpu> |
| `egui-winit` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui-winit> |
| `either` | `1.19.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rayon-rs/either> |
| `emath` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/emath> |
| `endi` | `1.1.1` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/zeenix/endi> |
| `enumflags2` | `0.7.12` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/meithecatte/enumflags2> |
| `enumflags2_derive` | `0.7.12` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/meithecatte/enumflags2> |
| `env_filter` | `2.0.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/env_logger> |
| `env_logger` | `0.11.11` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/env_logger> |
| `epaint` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/epaint> |
| `epaint_default_fonts` | `0.37.0` | `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/epaint_default_fonts> |
| `equivalent` | `1.0.3` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/indexmap-rs/equivalent> |
| `errno` | `0.3.14` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/lambda-fairy/rust-errno> |
| `error-code` | `3.4.0` | `BSL-1.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/DoumanAsh/error-code> |
| `euclid` | `0.22.14` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/euclid> |
| `event-listener` | `5.4.2` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/event-listener> |
| `event-listener-strategy` | `0.5.4` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/event-listener-strategy> |
| `extended` | `0.1.0` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/depp/extended-rs> |
| `fastrand` | `2.5.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/fastrand> |
| `fax` | `0.2.7` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdf-rs/fax> |
| `fdeflate` | `0.3.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/image-rs/fdeflate> |
| `fearless_simd` | `0.4.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/fearless_simd> |
| `find-msvc-tools` | `0.1.15` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/cc-rs> |
| `fixedbitset` | `0.5.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/petgraph/fixedbitset> |
| `flate2` | `1.1.10` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/flate2-rs> |
| `foldhash` | `0.1.5` | `Zlib` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/orlp/foldhash> |
| `foldhash` | `0.2.0` | `Zlib` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/orlp/foldhash> |
| `fontique` | `0.11.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/parley> |
| `font-types` | `0.12.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/googlefonts/fontations> |
| `foreign-types` | `0.5.0` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/sfackler/foreign-types> |
| `foreign-types-macros` | `0.2.4` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/sfackler/foreign-types> |
| `foreign-types-shared` | `0.3.1` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/sfackler/foreign-types> |
| `form_urlencoded` | `1.2.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/rust-url> |
| `futures-core` | `0.3.34` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/futures-rs> |
| `futures-io` | `0.3.34` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/futures-rs> |
| `futures-lite` | `2.6.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-THIRD-PARTY` &mdash; upstream repository: <https://github.com/smol-rs/futures-lite> |
| `futures-macro` | `0.3.34` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/futures-rs> |
| `futures-task` | `0.3.34` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/futures-rs> |
| `futures-util` | `0.3.34` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/futures-rs> |
| `generic-array` | `0.14.7` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/fizyk20/generic-array.git> |
| `gethostname` | `1.1.0` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://codeberg.org/swsnr/gethostname.rs.git> |
| `getrandom` | `0.3.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-random/getrandom> |
| `getrandom` | `0.4.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-random/getrandom> |
| `gl_generator` | `0.14.0` | `Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/brendanzab/gl-rs/> |
| `glifo` | `0.3.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/vello> |
| `glow` | `0.17.0` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/grovesNL/glow> |
| `glutin` | `0.32.3` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `glutin_egl_sys` | `0.7.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `glutin_glx_sys` | `0.6.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `glutin_wgl_sys` | `0.6.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `glutin-winit` | `0.5.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `gpu-allocator` | `0.28.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/Traverse-Research/gpu-allocator> |
| `guillotiere` | `0.7.0` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/nical/guillotiere> |
| `half` | `2.7.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/VoidStarKat/half-rs> |
| `harfrust` | `0.12.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/harfbuzz/harfrust> |
| `hashbrown` | `0.15.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/hashbrown> |
| `hashbrown` | `0.16.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/hashbrown> |
| `hashbrown` | `0.17.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/hashbrown> |
| `heck` | `0.5.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/withoutboats/heck> |
| `hermit-abi` | `0.5.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/hermit-os/hermit-rs> |
| `hex` | `0.4.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/KokaKiwi/rust-hex> |
| `hound` | `3.5.1` | `Apache-2.0` | Published crate package: `license` &mdash; upstream repository: <https://github.com/ruuda/hound> |
| `icu_collections` | `2.4.0` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `icu_locale_core` | `2.4.0` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `icu_normalizer` | `2.4.0` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `icu_normalizer_data` | `2.4.0` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `icu_properties` | `2.4.0` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `icu_properties_data` | `2.4.0` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `icu_provider` | `2.4.0` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `idna` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/rust-url/> |
| `idna_adapter` | `1.2.2` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/hsivonen/idna_adapter> |
| `image` | `0.25.10` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/image-rs/image> |
| `indexmap` | `2.14.2` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/indexmap-rs/indexmap> |
| `is_terminal_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/polyfill-rs/is_terminal_polyfill> |
| `itertools` | `0.15.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-itertools/itertools> |
| `itoa` | `1.0.18` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/itoa> |
| `jiff` | `0.2.38` | `Unlicense OR MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/jiff> |
| `jiff-core` | `0.1.1` | `Unlicense OR MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/jiff> |
| `jiff-static` | `0.2.38` | `Unlicense OR MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/jiff> |
| `jni` | `0.22.4` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/jni-rs/jni-rs> |
| `jni-macros` | `0.22.4` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/jni-rs/jni-rs> |
| `jni-sys` | `0.3.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/jni-rs/jni-sys> |
| `jni-sys` | `0.4.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/jni-rs/jni-sys> |
| `jni-sys-macros` | `0.4.1` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/jni-rs/jni-sys> |
| `jobserver` | `0.1.35` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/jobserver-rs> |
| `js-sys` | `0.3.106` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys> |
| `khronos_api` | `3.1.0` | `Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/brendanzab/gl-rs/> |
| `khronos-egl` | `6.0.0` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/timothee-haudebourg/khronos-egl> |
| `kurbo` | `0.13.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/kurbo> |
| `lazy_static` | `1.5.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang-nursery/lazy-static.rs> |
| `libc` | `0.2.191` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/libc> |
| `libloading` | `0.8.9` | `ISC` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/nagisa/rust_libloading/> |
| `libm` | `0.2.16` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/rust-lang/compiler-builtins> |
| `libredox` | `0.1.25` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/libredox.git> |
| `linebender_resource_handle` | `0.1.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/raw_resource_handle> |
| `linux-raw-sys` | `0.12.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-Apache-2.0_WITH_LLVM-exception`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/sunfishcode/linux-raw-sys> |
| `linux-raw-sys` | `0.4.15` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-Apache-2.0_WITH_LLVM-exception`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/sunfishcode/linux-raw-sys> |
| `litemap` | `0.8.3` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `litrs` | `1.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/LukasKalbertodt/litrs> |
| `lock_api` | `0.4.14` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/Amanieu/parking_lot> |
| `log` | `0.4.34` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/log> |
| `memchr` | `2.8.3` | `Unlicense OR MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/memchr> |
| `memmap2` | `0.9.12` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RazrFalcon/memmap2-rs> |
| `memoffset` | `0.9.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/Gilnaa/memoffset> |
| `miniz_oxide` | `0.8.9` | `MIT OR Zlib OR Apache-2.0` | Published crate package: `LICENSE`, `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` &mdash; upstream repository: <https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide> |
| `miniz_oxide` | `0.9.1` | `MIT OR Zlib OR Apache-2.0` | Published crate package: `LICENSE`, `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` &mdash; upstream repository: <https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide> |
| `moxcms` | `0.8.1` | `BSD-3-Clause OR Apache-2.0` | Published crate package: `LICENSE-APACHE.md`, `LICENSE.md` &mdash; upstream repository: <https://github.com/awxkee/moxcms.git> |
| `naga` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `naga-types` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `ndk` | `0.9.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/rust-mobile/ndk> |
| `ndk-context` | `0.1.1` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/rust-windowing/android-ndk-rs> |
| `ndk-sys` | `0.6.0+11769913` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/rust-mobile/ndk> |
| `nohash-hasher` | `0.2.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/paritytech/nohash-hasher> |
| `nom` | `8.0.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-bakery/nom> |
| `num_enum` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/illicitonion/num_enum> |
| `num_enum_derive` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/illicitonion/num_enum> |
| `num-complex` | `0.4.6` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-num/num-complex> |
| `num-integer` | `0.1.47` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-num/num-integer> |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-num/num-traits> |
| `objc2` | `0.5.3` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2` | `0.6.5` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-app-kit` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-app-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-cloud-kit` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-contacts` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-data` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-foundation` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-graphics` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-image` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-location` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-text` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-encode` | `4.1.0` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-foundation` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-foundation` | `0.3.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-io-surface` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-link-presentation` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-metal` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-metal` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-quartz-core` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-quartz-core` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-symbols` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-ui-kit` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-ui-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-uniform-type-identifiers` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-user-notifications` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc-sys` | `0.3.5` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `once_cell` | `1.21.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/matklad/once_cell> |
| `once_cell_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/polyfill-rs/once_cell_polyfill> |
| `orbclient` | `0.3.55` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/orbclient> |
| `ordered-float` | `5.5.1` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/reem/rust-ordered-float> |
| `ordered-stream` | `0.2.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/danieldg/ordered-stream> |
| `os_pipe` | `1.2.3` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/oconnor663/os_pipe.rs> |
| `owned_ttf_parser` | `0.25.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/alexheretic/owned-ttf-parser> |
| `parking` | `2.2.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-THIRD-PARTY` &mdash; upstream repository: <https://github.com/smol-rs/parking> |
| `parking_lot` | `0.12.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/Amanieu/parking_lot> |
| `parking_lot_core` | `0.9.12` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/Amanieu/parking_lot> |
| `parlance` | `0.1.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/parley> |
| `peniko` | `0.6.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/peniko> |
| `percent-encoding` | `2.3.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/rust-url/> |
| `petgraph` | `0.8.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/petgraph/petgraph> |
| `phf` | `0.14.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-phf/rust-phf> |
| `phf_generator` | `0.14.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-phf/rust-phf> |
| `phf_macros` | `0.14.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-phf/rust-phf> |
| `phf_shared` | `0.14.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-phf/rust-phf> |
| `pin-project` | `1.1.13` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/taiki-e/pin-project> |
| `pin-project-internal` | `1.1.13` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/taiki-e/pin-project> |
| `pin-project-lite` | `0.2.17` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/taiki-e/pin-project-lite> |
| `piper` | `0.2.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/piper> |
| `pkg-config` | `0.3.34` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/pkg-config-rs> |
| `plain` | `0.2.3` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/randomites/plain> |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/image-rs/image-png> |
| `polling` | `3.11.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/polling> |
| `poll-promise` | `0.3.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/EmbarkStudios/poll-promise> |
| `pollster` | `0.4.0` | `Apache-2.0/MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/zesterer/pollster> |
| `pollster` | `1.0.1` | `Apache-2.0/MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/zesterer/pollster> |
| `polycool` | `0.4.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/kurbo> |
| `portable-atomic` | `1.15.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/taiki-e/portable-atomic> |
| `portable-atomic-util` | `0.2.8` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/taiki-e/portable-atomic-util> |
| `potential_utf` | `0.1.6` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `presser` | `0.3.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/EmbarkStudios/presser> |
| `primal-check` | `0.3.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/huonw/primal> |
| `proc-macro2` | `1.0.107` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/proc-macro2> |
| `proc-macro-crate` | `3.5.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bkchr/proc-macro-crate> |
| `profiling` | `1.0.18` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/aclysma/profiling> |
| `pxfm` | `0.1.30` | `BSD-3-Clause OR Apache-2.0` | Published crate package: `LICENSE-APACHE.md`, `LICENSE.md` &mdash; upstream repository: <https://github.com/awxkee/pxfm> |
| `quick-error` | `2.0.1` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <http://github.com/tailhook/quick-error> |
| `quick-xml` | `0.41.0` | `MIT` | Published crate package: `LICENSE-MIT.md` &mdash; upstream repository: <https://github.com/tafia/quick-xml> |
| `quote` | `1.0.47` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/quote> |
| `range-alloc` | `0.1.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/range-alloc> |
| `raw-window-handle` | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` &mdash; upstream repository: <https://github.com/rust-windowing/raw-window-handle> |
| `raw-window-metal` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-windowing/raw-window-metal> |
| `rayon` | `1.12.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rayon-rs/rayon> |
| `rayon-core` | `1.13.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rayon-rs/rayon> |
| `read-fonts` | `0.41.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/googlefonts/fontations> |
| `redox_syscall` | `0.4.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/syscall> |
| `redox_syscall` | `0.5.18` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/syscall> |
| `redox_syscall` | `0.9.5` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/kernel> |
| `r-efi` | `5.3.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/r-efi/r-efi> |
| `r-efi` | `6.0.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/r-efi/r-efi> |
| `regex` | `1.13.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `regex-automata` | `0.4.18` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `regex-lite` | `0.1.9` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `regex-syntax` | `0.8.11` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `renderdoc-sys` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/ebkalderon/renderdoc-rs> |
| `rfd` | `0.17.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/PolyMeilex/rfd> |
| `roxmltree` | `0.21.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RazrFalcon/roxmltree> |
| `rustc_version` | `0.4.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/djc/rustc-version-rs> |
| `rustc-hash` | `1.1.0` | `Apache-2.0/MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang-nursery/rustc-hash> |
| `rustc-hash` | `2.1.3` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/rustc-hash> |
| `rustfft` | `6.4.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/ejmahler/RustFFT> |
| `rustix` | `0.38.44` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-Apache-2.0_WITH_LLVM-exception`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bytecodealliance/rustix> |
| `rustix` | `1.1.6` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-Apache-2.0_WITH_LLVM-exception`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bytecodealliance/rustix> |
| `rustversion` | `1.0.23` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/rustversion> |
| `same-file` | `1.0.6` | `Unlicense/MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/same-file> |
| `scoped-tls` | `1.0.1` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/alexcrichton/scoped-tls> |
| `scopeguard` | `1.2.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bluss/scopeguard> |
| `sctk-adwaita` | `0.10.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/PolyMeilex/sctk-adwaita> |
| `self_cell` | `1.3.0` | `Apache-2.0 OR GPL-2.0-only` | Published crate package: `LICENSE-APACHE`, `LICENSE-GPLv2` &mdash; upstream repository: <https://github.com/Voultapher/self_cell> |
| `semver` | `1.0.28` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/semver> |
| `send_wrapper` | `0.6.0` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE.txt`, `LICENSE-MIT.txt` &mdash; upstream repository: <https://github.com/thk1/send_wrapper> |
| `serde` | `1.0.229` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/serde-rs/serde> |
| `serde_core` | `1.0.229` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/serde-rs/serde> |
| `serde_derive` | `1.0.229` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/serde-rs/serde> |
| `serde_json` | `1.0.154` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/serde-rs/json> |
| `serde_repr` | `0.1.21` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/serde-repr> |
| `sha2` | `0.10.9` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/hashes> |
| `shlex` | `2.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/comex/rust-shlex> |
| `signal-hook-registry` | `1.4.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/vorner/signal-hook> |
| `simd_cesu8` | `1.2.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/seancroach/simd_cesu8> |
| `simd-adler32` | `0.3.10` | `MIT` | Published crate package: `LICENSE.md` &mdash; upstream repository: <https://github.com/mcountryman/simd-adler32> |
| `simdutf8` | `0.1.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-Apache`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rusticstuff/simdutf8> |
| `siphasher` | `1.0.4` | `MIT OR Apache-2.0` | Published crate package: `COPYING`, `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/jedisct1/rust-siphash> |
| `skrifa` | `0.44.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/googlefonts/fontations> |
| `slab` | `0.4.12` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/tokio-rs/slab> |
| `slotmap` | `1.1.1` | `Zlib` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/orlp/slotmap> |
| `smallvec` | `1.16.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/rust-smallvec> |
| `smithay-client-toolkit` | `0.19.2` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/client-toolkit> |
| `smithay-client-toolkit` | `0.20.0` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/client-toolkit> |
| `smithay-clipboard` | `0.7.3` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/smithay/smithay-clipboard> |
| `smol_str` | `0.2.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-analyzer/smol_str> |
| `spirv` | `0.4.0+sdk-1.4.341.0` | `Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/gfx-rs/rspirv> |
| `stable_deref_trait` | `1.2.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/storyyeller/stable_deref_trait> |
| `static_assertions` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/nvzqz/static-assertions-rs> |
| `strength_reduce` | `0.2.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <http://github.com/ejmahler/strength_reduce> |
| `strict-num` | `0.1.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/RazrFalcon/strict-num> |
| `strsim` | `0.11.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rapidfuzz/strsim-rs> |
| `symphonia` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-bundle-flac` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-codec-aac` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-codec-adpcm` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-codec-pcm` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-codec-vorbis` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-common` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-core` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-format-isomp4` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-format-mkv` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-format-ogg` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-format-riff` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `symphonia-metadata` | `0.6.1` | `MPL-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/pdeljanov/Symphonia> |
| `syn` | `2.0.119` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/syn> |
| `syn` | `3.0.9` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/syn> |
| `synstructure` | `0.14.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/mystor/synstructure> |
| `sys-locale` | `0.3.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/1Password/sys-locale> |
| `tempfile` | `3.27.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/Stebalien/tempfile> |
| `termcolor` | `1.4.1` | `Unlicense OR MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/termcolor> |
| `thiserror` | `1.0.69` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/thiserror> |
| `thiserror` | `2.0.21` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/thiserror> |
| `thiserror-impl` | `1.0.69` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/thiserror> |
| `thiserror-impl` | `2.0.21` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/thiserror> |
| `tiff` | `0.11.3` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/image-rs/image-tiff> |
| `tiny-skia` | `0.11.4` | `BSD-3-Clause` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/RazrFalcon/tiny-skia> |
| `tiny-skia-path` | `0.11.4` | `BSD-3-Clause` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/RazrFalcon/tiny-skia/tree/master/path> |
| `tinystr` | `0.8.5` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `tokio` | `1.54.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/tokio-rs/tokio> |
| `toml_datetime` | `1.1.2+spec-1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/toml-rs/toml> |
| `toml_edit` | `0.25.17+spec-1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/toml-rs/toml> |
| `toml_parser` | `1.1.5+spec-1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/toml-rs/toml> |
| `tracing` | `0.1.44` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/tokio-rs/tracing> |
| `tracing-attributes` | `0.1.31` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/tokio-rs/tracing> |
| `tracing-core` | `0.1.36` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/tokio-rs/tracing> |
| `transpose` | `0.2.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/ejmahler/transpose> |
| `tree_magic_mini` | `3.2.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/mbrubeck/tree_magic/> |
| `ttf-parser` | `0.25.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/harfbuzz/ttf-parser> |
| `type-map` | `0.5.1` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/kardeiz/type-map> |
| `typenum` | `1.20.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE`, `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/paholg/typenum> |
| `uds_windows` | `1.2.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/haraldh/rust_uds_windows> |
| `unicode-general-category` | `1.1.0` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/yeslogic/unicode-general-category> |
| `unicode-ident` | `1.0.27` | `(MIT OR Apache-2.0) AND Unicode-3.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-UNICODE` &mdash; upstream repository: <https://github.com/dtolnay/unicode-ident> |
| `unicode-properties` | `0.1.4` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/unicode-rs/unicode-properties> |
| `unicode-script` | `0.5.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/unicode-rs/unicode-script> |
| `unicode-segmentation` | `1.13.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/unicode-rs/unicode-segmentation> |
| `unicode-width` | `0.2.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/unicode-rs/unicode-width> |
| `url` | `2.5.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/servo/rust-url> |
| `utf8_iter` | `1.0.4` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/hsivonen/utf8_iter> |
| `utf8parse` | `0.2.2` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/alacritty/vte> |
| `uuid` | `1.29.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/uuid-rs/uuid> |
| `vello_common` | `0.2.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/vello> |
| `vello_cpu` | `0.2.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/vello> |
| `version_check` | `0.9.5` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/SergioBenitez/version_check> |
| `walkdir` | `2.5.0` | `Unlicense/MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/walkdir> |
| `wasip2` | `1.0.4+wasi-0.2.12` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-Apache-2.0_WITH_LLVM-exception`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bytecodealliance/wasi-rs> |
| `wasm-bindgen` | `0.2.129` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen> |
| `wasm-bindgen-futures` | `0.4.79` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures> |
| `wasm-bindgen-macro` | `0.2.129` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro> |
| `wasm-bindgen-macro-support` | `0.2.129` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support> |
| `wasm-bindgen-shared` | `0.2.129` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared> |
| `wayland-backend` | `0.3.17` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-client` | `0.31.15` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-csd-frame` | `0.3.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/wayland-csd-frame> |
| `wayland-cursor` | `0.31.14` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-protocols` | `0.32.13` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-experimental` | `20250721.0.1` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-misc` | `0.3.12` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-plasma` | `0.3.12` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-wlr` | `0.3.12` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-scanner` | `0.31.11` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `wayland-sys` | `0.31.11` | `MIT` | Published crate package: `LICENSE.txt` &mdash; upstream repository: <https://github.com/smithay/wayland-rs> |
| `webbrowser` | `1.2.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/amodm/webbrowser-rs> |
| `web-sys` | `0.3.106` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys> |
| `web-time` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/daxpedda/web-time> |
| `weezl` | `0.1.12` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/image-rs/weezl> |
| `wgpu` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-core` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-core-deps-apple` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-core-deps-emscripten` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-core-deps-wasm` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-core-deps-windows-linux-android` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-hal` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-naga-bridge` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `wgpu-types` | `30.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/wgpu> |
| `winapi-util` | `0.1.11` | `Unlicense OR MIT` | Published crate package: `COPYING`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/BurntSushi/winapi-util> |
| `windows` | `0.62.2` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_aarch64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_aarch64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_i686_gnu` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_i686_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_i686_msvc` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_gnu` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-collections` | `0.3.2` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-core` | `0.62.2` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-future` | `0.3.2` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-implement` | `0.60.2` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-interface` | `0.59.3` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-link` | `0.2.1` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-numerics` | `0.3.1` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-result` | `0.4.1` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-strings` | `0.5.1` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-sys` | `0.52.0` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-sys` | `0.59.0` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-sys` | `0.61.2` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-targets` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows-threading` | `0.2.1` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `winit` | `0.30.13` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/winit> |
| `winnow` | `1.0.4` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/winnow-rs/winnow> |
| `wit-bindgen` | `0.57.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-Apache-2.0_WITH_LLVM-exception`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bytecodealliance/wit-bindgen> |
| `wl-clipboard-rs` | `0.9.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/YaLTeR/wl-clipboard-rs> |
| `writeable` | `0.6.4` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `x11-dl` | `2.21.0` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/AltF02/x11-rs.git> |
| `x11rb` | `0.13.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/psychon/x11rb> |
| `x11rb-protocol` | `0.13.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/psychon/x11rb> |
| `xcursor` | `0.3.11` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/esposm03/xcursor-rs> |
| `xkbcommon-dl` | `0.4.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/xkbcommon-dl> |
| `xkeysym` | `0.2.1` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/notgull/xkeysym> |
| `xml-rs` | `0.8.29` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/kornelski/xml-rs> |
| `yeslogic-fontconfig-sys` | `6.0.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/yeslogic/fontconfig-rs> |
| `yoke` | `0.8.4` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `yoke-derive` | `0.8.4` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `zbus` | `5.19.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zbus_macros` | `5.19.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zbus_names` | `4.3.5` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zbus_xml` | `5.2.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zbus-lockstep` | `0.5.2` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/luukvanderduim/zbus-lockstep> |
| `zbus-lockstep-macros` | `0.5.2` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/luukvanderduim/zbus-lockstep> |
| `zcheapstr` | `1.1.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zcheapstr/> |
| `zerocopy` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/google/zerocopy> |
| `zerocopy-derive` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/google/zerocopy> |
| `zerofrom` | `0.1.8` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `zerofrom-derive` | `0.1.8` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `zerotrie` | `0.2.5` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `zerovec` | `0.11.9` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `zerovec-derive` | `0.11.7` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `zlib-rs` | `0.6.8` | `Zlib` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/trifectatechfoundation/zlib-rs> |
| `zmij` | `1.0.23` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/zmij> |
| `zune-core` | `0.5.3` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/etemesi254/zune-image> |
| `zune-jpeg` | `0.5.15` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg> |
| `zvariant` | `5.15.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zvariant_derive` | `5.15.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zvariant_utils` | `4.2.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |

**Python tooling.** The `tools/` scripts use `numpy` 2.5.4 and `scipy` 1.18.1, both
**BSD-3-Clause**. The standard library is under the Python Software Foundation
License. Python is used for fixture generation and independent verification only and
is never used by the shipped application, so it is not part of any binary you ship.
These versions are not pinned by a lock file in this repository, because nothing
that ships depends on them. Treat them as "the versions present in the development
environment where this document was written", not as a declared constraint.

---

## 4. What the licences require of a redistributor

In plain English. "You" below means whoever distributes StageSync — as source, as a
binary, or both.

### 4.1 MIT

MIT requires two things and nothing more:

1. **Keep the copyright notice and the permission notice** with all copies or
   substantial portions of the software.
2. **Do not misrepresent the warranty.** MIT says the software is provided "as is",
   without warranty of any kind.

There is **no** obligation to publish your own source code, and **no** patent grant
and **no** patent-retaliation clause. If you rely on the MIT text for StageSync
itself, you must include `LICENSE-MIT` with your distribution.

For the MIT-licensed dependencies, the practical requirement is to reproduce their
copyright and permission notices in your third-party notices file. There is no
source-code obligation.

### 4.2 Apache-2.0

Apache-2.0 requires more than MIT, but all of it is paperwork:

1. **Give recipients a copy of the licence.** A copy of Apache-2.0 must accompany
   the work. (§4(a))
2. **Mark modified files.** If you change a file, it must carry a prominent notice
   saying you changed it. (§4(b))
3. **Retain existing notices.** Keep all copyright, patent, trademark and attribution
   notices from the original source that pertain to parts of the work you
   distribute. (§4(c))
4. **Honour `NOTICE` files.** If the work includes a `NOTICE` file, your
   distribution must include a readable copy of the attribution notices it
   contains — in a `NOTICE` file, in the documentation, or in a display. You may add
   your own attribution notices but you may not alter or remove the existing ones.
   (§4(d))
5. **You may licence your own contributions differently.** You may add your own
   copyright statement to your modifications and licence those modifications under
   other terms, provided the overall use and distribution still complies.

Apache-2.0 also grants a **perpetual, worldwide, non-exclusive, no-charge,
royalty-free, irrevocable patent licence** from each contributor, with a
patent-litigation-termination clause (§3). This is the substantive difference from
MIT and the usual reason to prefer Apache-2.0.

### 4.3 MPL-2.0 — and specifically `symphonia` 0.6.1

**This is the one dependency in StageSync that is not permissively licensed, and it
is the one that needs your attention.**

`symphonia` 0.6.1 and its twelve sub-crates — `symphonia-core`,
`symphonia-common`, `symphonia-metadata`, `symphonia-codec-pcm`,
`symphonia-codec-aac`, `symphonia-codec-adpcm`, `symphonia-codec-vorbis`,
`symphonia-bundle-flac`, `symphonia-format-riff`, `symphonia-format-isomp4`,
`symphonia-format-mkv` and `symphonia-format-ogg` — are all licensed **MPL-2.0**.
StageSync uses `symphonia` to decode compressed input formats (AAC in MP4/M4A, and
FLAC) alongside WAV/PCM.

#### What "file-level copyleft" means

MPL-2.0 is a *weak, file-level* copyleft licence. The distinction from a strong
copyleft licence such as the GPL is the whole point, and it is worth being exact:

- **The MPL-covered files stay under MPL-2.0.** You may not relicense them.
- **Everything else is yours.** MPL-2.0 has no "the whole work must be open source"
  rule. It recognises the concept of a **"Larger Work"** (§1.9): a work that combines
  the MPL-covered software with other material, where the MPL-covered software is
  not the principal component. The Larger Work may be distributed under terms of your
  choosing — MIT, Apache-2.0, or a proprietary licence. **Your own code is not
  dragged into MPL-2.0, and neither is the StageSync code you are redistributing.**
- **`symphonia` does not declare the "Incompatible With Secondary Licenses" clause**
  (§1.10), so there is no additional restriction on how you combine it.

#### The obligations

- **§3.2 — make the Corresponding Source available.** If you distribute the MPL-covered
  software in Executable Form, you must make the Corresponding Source of the
  MPL-covered files available to the recipients of your binary, at a charge no more
  than the cost of distribution to them, and you must tell those recipients that the
  covered software is licensed under MPL-2.0 and how to obtain that source.
- **§3.3 — modifications.** If you modify an MPL-covered file, you must make the
  modified version available under MPL-2.0. StageSync does not modify `symphonia`;
  it is used as an unmodified dependency. If you fork or patch it downstream, this
  becomes your obligation.
- **§3.4 — notices stay intact.** You may add your own notices, but you may not
  alter or remove the existing ones.

#### Static linking versus dynamic linking — and what actually applies here

The brief for this project anticipated the dynamic-linking case, so it is worth
addressing both, and being clear about which one StageSync is actually in.

- **If the MPL-covered code is linked dynamically** — shipped as a separate DLL that
  your application loads at runtime — the application is the Larger Work. It can be
  distributed under MIT, Apache-2.0 or a proprietary licence. The DLL itself
  remains under MPL-2.0, its notices must stay intact, and you must still make the
  Corresponding Source of the MPL-covered files available to recipients. This is
  the arrangement MPL-2.0 was designed for: the file-level copyleft keeps the
  copyleft from spreading out of the covered files and into your application.
- **If the MPL-covered code is linked statically** — compiled directly into your
  executable — the analysis is the same in its essentials. The MPL-covered object
  code is part of your binary, so the Larger Work analysis applies, and the
  MPL-covered files remain under MPL-2.0 while the rest of the binary can be
  licensed as you choose. The §3.2 source-availability obligation still applies:
  recipients of your binary must be able to get the source of the MPL-covered
  files.

**Which case is StageSync in? The static one.** Rust links libraries statically by
default, and `symphonia` is an ordinary Rust library dependency, not a `cdylib`. Its
object code is compiled *into* `stagesync.exe` and `stagesync-desktop.exe`. There is
no `symphonia.dll` in the distribution to point a notice at.

This matters practically in exactly one way: **your notices file cannot say "see
`symphonia.dll` for the MPL-covered component".** You must instead point recipients
at the *source* of the `symphonia` crates, at the exact version you used.

#### What a redistributor of a Windows binary should actually do

Concretely, for a StageSync Windows build:

1. **Ship the Corresponding Source of the thirteen `symphonia` 0.6.1 crates.**
   The simplest compliant way is to include the crates.io source tarballs for
   `symphonia` 0.6.1 and each sub-crate you link, for example in a
   `third-party/symphonia/` directory alongside the binary, and point to that
   directory in your notices. These tarballs *are* the Corresponding Source:
   `symphonia` is pure Rust and ships its complete source in the published crate.
2. **Include the MPL-2.0 licence text** and reproduce the `symphonia` notices —
   the copyright notice and the statement that the code is licensed under MPL-2.0 —
   in your third-party notices file.
3. **Do not strip the notices.** MPL-2.0 §3.4 forbids altering or removing them.
4. **If you modify `symphonia`**, publish the modified files under MPL-2.0 and
   offer them alongside the binary.
5. **If you would rather not carry source**, the alternative is to keep `symphonia`
   out of the build — for example by dropping the AAC/FLAC decode features and
   accepting WAV/PCM only. This is a real option, since `symphonia`'s features are
   selected in `crates/stagesync-core/Cargo.toml`. It is a product decision, not a
   legal requirement.

A written offer is the traditional mechanism for meeting a source-availability
obligation, and MPL-2.0 §3.2 permits charging no more than the cost of
distribution. In practice, for a binary of this size, **including the source
tarballs is simpler, cheaper and more robust than an offer**, and it is what §5
recommends.

### 4.4 The permissive licences, briefly

- **BSD-2-Clause / BSD-3-Clause** — permissive. The only substantive condition is
  that the copyright notice, the conditions and the disclaimer are reproduced. The
  third clause of BSD-3-Clause additionally forbids using the names of the
  contributors to endorse derived products without permission; that clause is about
  endorsement, and it does not restrict redistribution.
- **ISC** — permissive. Reproduce the notice.
- **Zlib** — permissive. Reproduce the notice; do not use the author's name to
  endorse a product.
- **0BSD** — a "no conditions" permissive licence; it removes even the notice
  requirement.
- **Unlicense** — a public-domain dedication, with a fallback MIT grant. Take the
  MIT option if you prefer.
- **BSL-1.0** (Boost Software Licence) — permissive, with an OSI-style
  non-endorsement clause.
- **Apache-2.0 WITH LLVM-exception** — the LLVM exception removes Apache-2.0's
  copyleft effect for the covered files, leaving a permissive licence with a patent
  grant.

### 4.5 The one font-licence exception: `epaint_default_fonts`

`epaint_default_fonts` 0.37.0 declares
`(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. The `AND` means all three
apply: the crate's own code is MIT/Apache-2.0, and the font files it embeds are
under the SIL Open Font License 1.1 and the Ubuntu Font Licence 1.0.

What those require:

- The font files must keep their copyright and patent notices.
- Any derivative of the font must be released under the same licence — StageSync
  does not modify the fonts.
- The fonts may not be sold on their own, and the OFL's Reserved Font Name
  conditions apply if a Reserved Font Name is used.

StageSync embeds these fonts as **unmodified data** inside the GUI and does not
redistribute them as a separate product, so these conditions are satisfied. The
practical requirement is to keep the font notices in your third-party notices file.
A subtlety worth noting: if you were to *rename* a Reserved Font Name, you would
have to rename it in the notices too — but StageSync does not rename anything.

### 4.6 What this means for StageSync's own dual licence

Every dependency in the graph is under a licence that permits StageSync to be
redistributed under MIT or Apache-2.0, and every dependency's licence permits
combination with code under those terms. The MPL-2.0 dependency is compatible in
exactly this sense: it leaves your own code and your own licensing choice untouched,
and imposes only a source-availability obligation on the recipient of a binary.

`PROVENANCE.md` §4 sets out the same analysis from the compatibility side, with the
per-licence-family reasoning.

---

## 5. Third-party notices checklist

For whoever ships a StageSync binary. Work through this list; it is ordered by
what is most easily forgotten.

### 5.1 StageSync's own licence

- [ ] Include `LICENSE-MIT` and `LICENSE-APACHE` with the distribution.
- [ ] State, in the README or the About box, that StageSync is available under
      MIT **or** Apache-2.0, and which of the two you are distributing it under.
- [ ] Keep the copyright notice `Copyright (c) 2026 The StageSync contributors`
      intact.

### 5.2 Third-party notices file

- [ ] Ship a `THIRD-PARTY-NOTICES.txt` (or equivalent) alongside the binary.
- [ ] List **every** package in Appendix A that is compiled into the shipped
      binary — that is, every row marked `runtime`, `runtime (direct)` or
      `proc-macro` in `PROVENANCE.md` §3.4 — with its name, exact version and
      licence.
- [ ] Reproduce each dependency's **copyright notice** and its licence text or a
      pointer to it.
- [ ] Include the licence text of each distinct licence used, not just the
      identifiers: MIT, Apache-2.0, Zlib, BSD-3-Clause, BSD-2-Clause, ISC, 0BSD,
      Unlicense, BSL-1.0, Unicode-3.0, MPL-2.0, OFL-1.1, Ubuntu-font-1.0, and the
      LLVM exception as applicable.
- [ ] Keep the `NOTICE` files of any Apache-2.0-licensed dependency that has one,
      reproducing the attribution notices they contain. (Check each dependency's
      repository for a `NOTICE` file — Appendix A points at the repository but does
      not list `NOTICE` files, because whether a given crate has one is a property
      of its repository, not of its manifest.)
- [ ] Where a dependency's licence is dual (MIT/Apache-2.0), it is sufficient to
      reproduce both texts and say the dependency is available under either.

### 5.3 The MPL-2.0 dependency — `symphonia` 0.6.1

- [ ] Include the **MPL-2.0 licence text** in your distribution.
- [ ] Reproduce the `symphonia` copyright notice and state that the `symphonia`
      crates are licensed under MPL-2.0.
- [ ] **Ship the Corresponding Source of the thirteen `symphonia` 0.6.1 crates** —
      the crates.io source tarballs for `symphonia` and each sub-crate you link —
      and tell recipients where they are. Remember that because Rust links
      statically there is **no `symphonia.dll`** to refer to; the source *is* the
      thing you must point at.
- [ ] If you have modified any `symphonia` file, publish those modifications under
      MPL-2.0 and offer them with the binary.
- [ ] Do not alter or remove the upstream notices (§3.4).

### 5.4 Font and other data licences

- [ ] Keep the copyright and patent notices for the fonts embedded via
      `epaint_default_fonts` (`OFL-1.1` and `Ubuntu-font-1.0`), and state that the
      fonts are not sold separately.
- [ ] Keep the notices for the `Unicode-3.0`-licensed crates
      (`unicode-ident`, `icu_*`, `yoke`, `zerovec` and friends).

### 5.5 Things that are *not* required, so you do not go looking for them

- [ ] No source-code disclosure obligation for your own work: MIT and Apache-2.0
      are both permissive, and MPL-2.0's copyleft does not reach your files.
- [ ] No obligation to licence your own application under MIT or Apache-2.0.
- [ ] No third-party **runtime** binary is redistributed by StageSync. A few
      dependencies do ship link-time artefacts inside their crate packages: the
      `windows_*` crates ship MSVC import libraries (`.lib`) and GNU archives
      (`.a`) that are *stubs* referring to the Windows system libraries, and
      `libloading` ships two small DLLs (`nagisa32.dll`, `nagisa64.dll`) that are
      fixtures for that crate's own test suite. None of these is linked into
      `stagesync.exe` or `stagesync-desktop.exe`, and none is copied into a
      StageSync distribution. The only libraries the shipped binaries actually
      call are Windows system libraries (kernel32, user32 and so on), which are
      licensed by Microsoft and are covered by your Windows distribution terms,
      not by this notices file.
- [ ] No obligation to ship the Python tooling, or to mention numpy and scipy in
      a binary notices file — they are not used by the shipped application. (Do
      list them if you distribute the `tools/` scripts.)

### 5.6 Reproducibility

- [ ] Generate the notices file from `Cargo.lock` rather than by hand, so that a
      dependency bump cannot silently leave the notices out of date. Appendix A
      and `PROVENANCE.md` §3.4 are generated from the lock file in exactly this way.

---

## 6. Quick reference

| Question | Answer |
|---|---|
| What licence is StageSync under? | MIT **or** Apache-2.0, at your option. |
| Where are the licence texts? | `LICENSE-MIT` and `LICENSE-APACHE` in the repository root. |
| Is any dependency copyleft over the whole work? | No. |
| Is any dependency copyleft at all? | Yes — `symphonia` 0.6.1 and its twelve sub-crates, under MPL-2.0, which is file-level. |
| What must I do about `symphonia`? | Ship the source of the `symphonia` crates, include the MPL-2.0 text, keep the notices. See §4.3. |
| Must I open-source my own application? | No. |
| Must I ship any third-party source code? | Yes — the source of the `symphonia` crates, and only those. |
| Where do I find the licence of a specific crate? | Appendix A, or the crate's published `Cargo.toml`, or its upstream repository. |
