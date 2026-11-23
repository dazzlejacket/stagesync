# Third-party notices for StageSync 0.1.0

This file is generated. Do not edit it by hand: regenerate it with

```
python packaging/notices/generate_notices.py --cargo-lock Cargo.lock --licenses packaging/notices/licenses.json --output <directory>
```

The inputs this file was generated from:

| Input | SHA-256 |
|---|---|
| `Cargo.lock` | `9d0fde41beffaab9d6b93350eb8fd2718066d241689be4d23194c7b79f50be79` |
| `packaging/notices/licenses.json` | `965fda1ecc95ecb6209cb22e37d49cdc22e20d3bf93244f80b0fecdf67aa8e2f` |

## What this file is

StageSync is distributed under `MIT OR Apache-2.0`. The binaries it ships link a number of third-party crates, each of which carries its own licence. This file lists every package pinned in `Cargo.lock`, the licence that package declares in its own published manifest, and where that licence text lives.

## The licence texts that ship with this build

| File | Licence | Applies to |
|---|---|---|
| `LICENSE-MIT` | MIT | StageSync itself. StageSync is distributed under `MIT OR Apache-2.0`: you may use it under either licence. |
| `LICENSE-APACHE` | Apache License 2.0 | StageSync itself, the option of the `MIT OR Apache-2.0` dual licence. |
| `LICENSE-MPL-2.0` | Mozilla Public License 2.0 | The thirteen `symphonia` crates. `symphonia` is linked, not vendored, so the MPL's file-level copyleft applies to it: see SOURCE-AVAILABILITY.md. |

The project licence texts are copies of the ones in the source tree; the MPL-2.0 text is the Mozilla Public License 2.0 as published by the SPDX project.

## The dependency graph

`Cargo.lock` pins 513 packages. 3 of them are the StageSync crates themselves; the other 510 are third-party dependencies from crates.io.

The table lists every package in `Cargo.lock`. `Cargo.lock` does not record which of them a given build links, so this list is a superset of the graph of any single binary.

| Crate | Version | Licence | Where the licence text lives |
|---|---|---|---|
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
| `alsa` | `0.11.0` | `Apache-2.0/MIT` | SPDX identifier only; upstream repository: <https://github.com/diwic/alsa-rs> |
| `alsa-sys` | `0.4.0` | `MIT` | SPDX identifier only; upstream repository: <https://github.com/diwic/alsa-sys> |
| `android-activity` | `0.6.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE`, `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-mobile/android-activity> |
| `android-properties` | `0.2.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/miklelappo/android-properties> |
| `android_system_properties` | `0.1.6` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/nical/android_system_properties> |
| `anstream` | `1.0.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle` | `1.0.14` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle-parse` | `1.0.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle-query` | `1.1.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anstyle-wincon` | `3.0.11` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-cli/anstyle.git> |
| `anyhow` | `1.0.104` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/anyhow> |
| `arboard` | `3.7.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE.txt`, `LICENSE-MIT.txt` &mdash; upstream repository: <https://github.com/1Password/arboard> |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/droundy/arrayref> |
| `arrayvec` | `0.7.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bluss/arrayvec> |
| `as-raw-xcb-connection` | `1.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/psychon/as-raw-xcb-connection> |
| `ash` | `0.38.0+1.3.281` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/ash-rs/ash> |
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
| `bit-set` | `0.10.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/contain-rs/bit-set> |
| `bit-vec` | `0.9.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/contain-rs/bit-vec> |
| `bitflags` | `1.3.2` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bitflags/bitflags> |
| `bitflags` | `2.13.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bitflags/bitflags> |
| `block-buffer` | `0.10.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/utils> |
| `block2` | `0.5.1` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `block2` | `0.6.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
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
| `cfg-if` | `1.0.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/cfg-if> |
| `cfg_aliases` | `0.2.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/katharostech/cfg_aliases> |
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
| `coreaudio-rs` | `0.14.2` | `MIT/Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/RustAudio/coreaudio-rs.git> |
| `cpal` | `0.18.2` | `Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/RustAudio/cpal> |
| `cpufeatures` | `0.2.17` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/utils> |
| `crc32fast` | `1.5.3` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/srijs/rust-crc32fast> |
| `crossbeam-deque` | `0.8.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/crossbeam-rs/crossbeam> |
| `crossbeam-epoch` | `0.9.21` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/crossbeam-rs/crossbeam> |
| `crossbeam-utils` | `0.8.23` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/crossbeam-rs/crossbeam> |
| `crunchy` | `0.2.4` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/eira-fransham/crunchy> |
| `crypto-common` | `0.1.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/traits> |
| `cursor-icon` | `1.2.0` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` &mdash; upstream repository: <https://github.com/rust-windowing/cursor-icon> |
| `dasp_sample` | `0.11.0` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/rustaudio/sample.git> |
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
| `egui-wgpu` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui-wgpu> |
| `egui-winit` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui-winit> |
| `egui_glow` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui_glow> |
| `egui_kittest` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/emilk/egui> |
| `egui_system_font_provider` | `0.37.0` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/emilk/egui/tree/main/crates/egui_system_font_provider> |
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
| `font-types` | `0.12.7` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/googlefonts/fontations> |
| `fontique` | `0.11.1` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/parley> |
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
| `glutin-winit` | `0.5.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `glutin_egl_sys` | `0.7.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `glutin_glx_sys` | `0.6.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
| `glutin_wgl_sys` | `0.6.1` | `Apache-2.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/rust-windowing/glutin> |
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
| `khronos-egl` | `6.0.0` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/timothee-haudebourg/khronos-egl> |
| `khronos_api` | `3.1.0` | `Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/brendanzab/gl-rs/> |
| `kittest` | `0.5.0` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/rerun-io/kittest> |
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
| `mach2` | `0.6.0` | `BSD-2-Clause OR MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/JohnTitor/mach2> |
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
| `num-bigint` | `0.4.8` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/rust-num/num-bigint> |
| `num-complex` | `0.4.6` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-num/num-complex> |
| `num-derive` | `0.4.2` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/rust-num/num-derive> |
| `num-integer` | `0.1.47` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-num/num-integer> |
| `num-rational` | `0.4.2` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/rust-num/num-rational> |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-num/num-traits> |
| `num_enum` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/illicitonion/num_enum> |
| `num_enum_derive` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/illicitonion/num_enum> |
| `objc-sys` | `0.3.5` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2` | `0.5.3` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2` | `0.6.5` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-app-kit` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-app-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-audio-toolbox` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-avf-audio` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-cloud-kit` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-contacts` | `0.2.2` | `MIT` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-audio` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; upstream repository: <https://github.com/madsmtm/objc2> |
| `objc2-core-audio-types` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | SPDX identifier only; upstream repository: <https://github.com/madsmtm/objc2> |
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
| `poll-promise` | `0.3.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/EmbarkStudios/poll-promise> |
| `polling` | `3.11.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/smol-rs/polling> |
| `pollster` | `0.4.0` | `Apache-2.0/MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/zesterer/pollster> |
| `pollster` | `1.0.1` | `Apache-2.0/MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/zesterer/pollster> |
| `polycool` | `0.4.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/linebender/kurbo> |
| `portable-atomic` | `1.15.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/taiki-e/portable-atomic> |
| `portable-atomic-util` | `0.2.8` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/taiki-e/portable-atomic-util> |
| `potential_utf` | `0.1.6` | `Unicode-3.0` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/unicode-org/icu4x> |
| `presser` | `0.3.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/EmbarkStudios/presser> |
| `primal-check` | `0.3.4` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/huonw/primal> |
| `proc-macro-crate` | `3.5.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/bkchr/proc-macro-crate> |
| `proc-macro2` | `1.0.107` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/proc-macro2> |
| `profiling` | `1.0.18` | `MIT OR Apache-2.0` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/aclysma/profiling> |
| `pxfm` | `0.1.30` | `BSD-3-Clause OR Apache-2.0` | Published crate package: `LICENSE-APACHE.md`, `LICENSE.md` &mdash; upstream repository: <https://github.com/awxkee/pxfm> |
| `quick-error` | `2.0.1` | `MIT/Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <http://github.com/tailhook/quick-error> |
| `quick-xml` | `0.41.0` | `MIT` | Published crate package: `LICENSE-MIT.md` &mdash; upstream repository: <https://github.com/tafia/quick-xml> |
| `quote` | `1.0.47` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/dtolnay/quote> |
| `r-efi` | `5.3.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/r-efi/r-efi> |
| `r-efi` | `6.0.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | SPDX identifier only; licence text is in the upstream repository: <https://github.com/r-efi/r-efi> |
| `range-alloc` | `0.1.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE.APACHE`, `LICENSE.MIT` &mdash; upstream repository: <https://github.com/gfx-rs/range-alloc> |
| `raw-window-handle` | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` | Published crate package: `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` &mdash; upstream repository: <https://github.com/rust-windowing/raw-window-handle> |
| `raw-window-metal` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-windowing/raw-window-metal> |
| `rayon` | `1.12.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rayon-rs/rayon> |
| `rayon-core` | `1.13.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rayon-rs/rayon> |
| `read-fonts` | `0.41.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/googlefonts/fontations> |
| `redox_syscall` | `0.4.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/syscall> |
| `redox_syscall` | `0.5.18` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/syscall> |
| `redox_syscall` | `0.9.5` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://gitlab.redox-os.org/redox-os/kernel> |
| `regex` | `1.13.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `regex-automata` | `0.4.18` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `regex-lite` | `0.1.9` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `regex-syntax` | `0.8.11` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/regex> |
| `renderdoc-sys` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/ebkalderon/renderdoc-rs> |
| `rfd` | `0.17.2` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/PolyMeilex/rfd> |
| `rodio` | `0.23.0` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/RustAudio/rodio> |
| `roxmltree` | `0.21.2` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RazrFalcon/roxmltree> |
| `rustc-hash` | `1.1.0` | `Apache-2.0/MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang-nursery/rustc-hash> |
| `rustc-hash` | `2.1.3` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/rust-lang/rustc-hash> |
| `rustc_version` | `0.4.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/djc/rustc-version-rs> |
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
| `serde_spanned` | `1.1.3` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/toml-rs/toml> |
| `sha2` | `0.10.9` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/RustCrypto/hashes> |
| `shlex` | `2.0.1` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/comex/rust-shlex> |
| `signal-hook-registry` | `1.4.8` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/vorner/signal-hook> |
| `simd-adler32` | `0.3.10` | `MIT` | Published crate package: `LICENSE.md` &mdash; upstream repository: <https://github.com/mcountryman/simd-adler32> |
| `simd_cesu8` | `1.2.0` | `Apache-2.0 OR MIT` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/seancroach/simd_cesu8> |
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
| `stagesync-cli` | `0.1.0` | `MIT OR Apache-2.0` | The StageSync workspace licence: LICENSE-MIT and LICENSE-APACHE in the source tree. |
| `stagesync-core` | `0.1.0` | `MIT OR Apache-2.0` | The StageSync workspace licence: LICENSE-MIT and LICENSE-APACHE in the source tree. |
| `stagesync-desktop` | `0.1.0` | `MIT OR Apache-2.0` | The StageSync workspace licence: LICENSE-MIT and LICENSE-APACHE in the source tree. |
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
| `toml` | `1.2.1+spec-1.1.0` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/toml-rs/toml> |
| `toml_datetime` | `1.1.3+spec-1.1.0` | `MIT OR Apache-2.0` | SPDX identifier only; upstream repository: <https://github.com/toml-rs/toml> |
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
| `web-sys` | `0.3.106` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys> |
| `web-time` | `1.1.0` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/daxpedda/web-time> |
| `webbrowser` | `1.2.5` | `MIT OR Apache-2.0` | Published crate package: `LICENSE-APACHE`, `LICENSE-MIT` &mdash; upstream repository: <https://github.com/amodm/webbrowser-rs> |
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
| `windows_aarch64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_aarch64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_i686_gnu` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_i686_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_i686_msvc` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_gnu` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | Published crate package: `license-apache-2.0`, `license-mit` &mdash; upstream repository: <https://github.com/microsoft/windows-rs> |
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
| `zbus-lockstep` | `0.5.2` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/luukvanderduim/zbus-lockstep> |
| `zbus-lockstep-macros` | `0.5.2` | `MIT` | Published crate package: `LICENSE-MIT` &mdash; upstream repository: <https://github.com/luukvanderduim/zbus-lockstep> |
| `zbus_macros` | `5.19.0` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zbus_names` | `4.3.5` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
| `zbus_xml` | `5.2.1` | `MIT` | Published crate package: `LICENSE` &mdash; upstream repository: <https://github.com/z-galaxy/zbus/> |
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

## The MPL-2.0 dependency: the `symphonia` family

13 packages are licensed under the Mozilla Public License 2.0, and all of them belong to the `symphonia` family. `symphonia` decodes the audio formats StageSync reads. It is **linked**, not vendored: Rust links it statically into the binary, so there is no `symphonia.dll` to point a recipient at. What MPL-2.0 section 3.2 requires instead is that the corresponding source be made available, and `SOURCE-AVAILABILITY.md` in this bundle says exactly where that source comes from and how to verify it.

| Crate | Version | SHA-256 of the crates.io `.crate` file (the `Cargo.lock` checksum) |
|---|---|---|
| `symphonia` | `0.6.1` | `a7edef6a96b696d4e0cab5ee9ebb7ca155ed95f30a6b45bbb8b97d2727f02424` |
| `symphonia-bundle-flac` | `0.6.1` | `6405d5c34ff6f8f7ca08a4101efe66d21e180f7322ef9359900a0e55698a7892` |
| `symphonia-codec-aac` | `0.6.1` | `f5bf8e39552d34a3c4c98333370e62f48c92456d2e814f273b5c3ad7c4a5f45c` |
| `symphonia-codec-adpcm` | `0.6.1` | `445932ecb0c59362fde9c082dd63a75380650c5077fdb8b80b8cac850442a74c` |
| `symphonia-codec-pcm` | `0.6.1` | `e04ba75686acbe43542fdd374571195f0530c0b7785ca25cc6840e9c6c4b6eea` |
| `symphonia-codec-vorbis` | `0.6.1` | `73d90b4fcf796137cc683c538282804ff9629f8ad9dbfd881fcbba331ac4e986` |
| `symphonia-common` | `0.6.1` | `2acc3fcc18ec9b8cdd48614e259c4cf0d27b71d41e5d9b120b42c5adab12d7c4` |
| `symphonia-core` | `0.6.1` | `01c412864d599d4750d0c3d684d7e093ec05e5309681ef5252cc1096a437f6e0` |
| `symphonia-format-isomp4` | `0.6.1` | `0e681a70e1870d34e02abf1dbc51e4267c3f1827801474e8870be8c689fc4dc3` |
| `symphonia-format-mkv` | `0.6.1` | `d015c5c0558864665894b3f4cbd95e10abb01b9c868e751c72670f326a56360e` |
| `symphonia-format-ogg` | `0.6.1` | `0b5495e7f7e3c7035328d82b6d6e377eef289bb0c4105bdeb557fc93a833f994` |
| `symphonia-format-riff` | `0.6.1` | `1ff70929083a8c1a5f6cd7c904b6071c7914ad04739b510c2f7239dfc9b7dabe` |
| `symphonia-metadata` | `0.6.1` | `83713a97705d77bdef7cdbc0768fd6e5a54e4cd7e48d60a806ae85639e2c87c6` |

StageSync does not modify any `symphonia` file, so there are no modified files to publish under MPL-2.0 section 3.1(b).

## Limitations, stated plainly

* `Cargo.lock` records names, versions and checksums. It does **not** record licences, and it does **not** record which platform-specific packages a given build actually links. The licence column above therefore comes from `packaging/notices/licenses.json`, a checked-in transcription of the `license` field of each pinned package's published manifest (see `LICENSES.md`, Appendix A, for the per-dependency table this file was generated from). No licence here was guessed.
* The table above is the whole lock graph, which is a superset of the graph any one binary links. A Windows build does not link the `*-apple` crates and a Linux build does not link the `windows_*` crates; listing them is deliberate, because the notices have to cover the lock file that produced the binary.
* A licence identifier says what a package *declares*. This file makes no claim that the upstream code complies with the licence it declares; auditing every upstream codebase is not something this project does, and licence scanners read the same manifests.
* The fonts embedded through `epaint_default_fonts` are covered by their own licence terms (`OFL-1.1` and `Ubuntu-font-1.0`), which are listed in the table above. StageSync does not sell the fonts separately.

The repository StageSync declares in its workspace manifest is <https://github.com/dazzlejacket/stagesync>. StageSync's own licence texts (`LICENSE-MIT` and `LICENSE-APACHE`) are in this bundle. No third-party licence in the table above requires StageSync's own source to be published: the only source-availability obligation here is the MPL-2.0 one, and it is described in `SOURCE-AVAILABILITY.md`.
