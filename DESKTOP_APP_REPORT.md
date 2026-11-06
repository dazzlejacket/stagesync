# The desktop application — what was built, what was verified, and what was not

This document describes the work on the StageSync desktop application: the
waveform timeline, the playback, the manual corrections, and the tests that
prove they work. Everything below was run on this machine; the figures are the
figures the programs printed.

The short-clip false-alignment defect that came first is described in
`PROJECT_STATUS.md` §11 and in `crates/stagesync-core/tests/short_source_tests.rs`.

---

## 1. What the window does now

| Requirement | What it does | Where it lives |
|---|---|---|
| Real waveforms | The reference is drawn on top, each target below it, **placed by the alignment itself** — the picture is the model, not an approximation of it. | `src/waveview.rs` |
| Timeline | Zoom with the wheel (around the cursor), `Start` / `-` / `+` / `Fit`, drag to select a region, Shift+drag to pan, a ruler, and the playhead. | `src/app.rs` (`timeline_section`) |
| Overlaps, gaps, uncertainty | A strip under every lane: green where the material is matched, amber where the match is weak, red where the reference has material nobody recorded. | `src/waveview.rs` |
| Playback | The reference, or the corrected target, through the machine's own audio device. | `src/player.rs` |
| A/B at the same position | The two buttons share one position on the reference timeline, so switching between them does not move the playhead. | `src/app.rs` (`transport_section`) |
| Loop | Select a region, press *Loop selection*, and the playback starts again at the beginning of the region when it reaches the end. | `src/app.rs` (`follow_the_playhead`) |
| Jump to a suspected problem | *Places to look at* lists every gap and every uncertain span, each with a button that moves the playhead there. | `src/app.rs` (`problems_section`) |
| Manual fixes | Nudge by ±1, ±10 or ±100 ms; the placement moves and **the rate does not**. Undo, and *Reset to automatic*. | `src/app.rs` (`nudge_section`) |
| Re-checking after an edit | A nudge can be turned into a manual anchor; the analysis runs again over the anchored recording and the result is reported again. | `src/app.rs`, `src/jobs.rs` |
| No overwhelming dashboard | Three panels: the file list, the timeline with the transport, and the export controls. | `src/app.rs` (`layout`) |

### The command line

`stagesync-desktop` takes the same files as the window, and analyses them without
the window having to be opened:

```
stagesync-desktop [--analyse] [FILE…]
```

The first file is the reference. `--help` prints the usage; an unknown switch is
an error rather than a shrug. The switch exists so that the application can be
driven from a script and so that a populated window can be reproduced.

---

## 2. The waveform cache

The window never holds a recording's samples. `stagesync-core`'s new
`audio::peaks` module streams the file once and keeps, for every bucket of
samples, the lowest and the highest sample in it:

* the finest level covers **512 frames** (about eleven milliseconds at 48 kHz);
* each level above it is **four times coarser**, until at most 32 buckets are
  left;
* the file is read in blocks and each block is folded into the peaks and
  dropped, so the memory does not depend on the length of the recording, and a
  cancelled build stops within a block.

For the real three-hour reference (`combined.wav`, 464,902,080 frames at 48 kHz,
mono) that is **nine levels and 1,209,639 buckets — 9.7 MB of peak data, against
1.85 GB of samples**. A stereo recording of the same length costs the same in
buckets and twice as much in floats.

The level that answers a question is chosen by the span: the finest level that
covers the span with no more buckets than the window has pixels. Zooming in
therefore costs no further reading of the file.

`audio::decode::seek_to_frame` was added at the same time, so that playback can
start in the middle of a recording without decoding everything before it.

---

## 3. The playback

`rodio` is used for the **device** and for nothing else: the decoding is done by
`stagesync-core`, which already reads every format the pipeline supports, and the
correction is done by the core's own polyphase Kaiser-windowed-sinc resampler —
the same filter the export uses. The preview and the export therefore agree,
instead of the preview being a second, different approximation.

* The reference is played as it is.
* The corrected target is played through the model: the position in the recording
  comes from the model, and the rate correction is the model's own scale.
* The output is produced at the device's own sample rate; the rate conversion and
  the drift correction are folded into one filter
  (`alpha = in_rate / (out_rate * scale)`).
* A machine with no audio device is handled: the window says so and keeps working.

---

## 4. The manual corrections

A nudge moves the **placement** of a recording and nothing else. The rate is
never changed by hand: the drift is a measurement, and a measurement that a user
can quietly overwrite is not a measurement.

* The nudge is shown on the picture at once, before it is applied.
* *Apply and re-check* turns the nudge into a **manual anchor** on that recording
  and runs the analysis again, so the placement is re-established from the
  material rather than assumed. The rate stays as it was measured.
* *Undo* takes back the last nudge; *Reset to automatic* throws the manual
  corrections away and goes back to what the matcher found.

---

## 5. What was verified, and how

### The real recordings

The window was run against the real material in `audio/`, with the release build:

| Source | Offset | Drift | Accumulated | Coverage |
|---|---|---|---|---|
| `Voice 005_W_20250208_152158.m4a` | **+317.389 s** | **+16.06 ± 0.14 ppm** | +77.3 ms | 49.6 % |
| `Voice 006_W_20250208_164548.m4a` | **+5753.314 s** | **+16.39 ± 0.15 ppm** | +66.4 ms | 41.8 % |

These are the same figures the documentation already gave, which is the point:
the desktop application did not change the measurement.

The screenshot in `work/shot16.png` shows the populated window: the reference
lane on top, the Voice 005 lane below it with its waveform starting at 317 s —
exactly where the model says the recording begins — and the Voice 006 lane empty
because its material begins at 5753 s, far outside the visible 600 s.

### The buttons

`egui_kittest` drives the **real widgets**: it finds a button by its label and
clicks it, the way a mouse would. The test
`the_buttons_of_the_window_do_what_they_say` clicks **Analyze**, **Fit**, the
zoom-out button and **Play**, and checks that each of them did something: the
analysis ran and the window said so, *Fit* put the whole show on the screen, the
zoom changed the span, and *Play* either started the sound or said why it could
not.

This is how a real defect was found: the button that removes a recording was
labelled `✖` (U+2716), and egui's bundled font has no glyph for it, so the button
was an empty box. It is now `×`, which the font has.

### The whole workflow

`the_window_aligns_a_recording_and_exports_the_correction` drives the window's
own path end to end on a generated recording: the files are put on the list, the
analysis runs on its own thread and comes back over the channel, the alignment is
checked against the truth, the nudge is applied and undone, the export writes the
corrected audio and the artefacts, and the two recordings are **byte-for-byte
unchanged**.

### The test suite

| Suite | Tests |
|---|---|
| `stagesync-core` (all) | 145 |
| `stagesync-desktop` | 38 |
| `stagesync-cli` | 21 |

---

## 6. What was **not** verified

* **Physical mouse clicks on the screen.** This machine has no interactive
  desktop: `SendInput` returns nothing and posted mouse messages never reach
  egui. The buttons were therefore clicked through egui's own input harness,
  which is the same code path a mouse takes, but the literal act of moving a
  mouse on this display was not performed. Anything that depends on the window
  manager rather than on egui — a native drag-and-drop from the file manager, a
  right-click menu — is therefore **unverified**.
* **macOS and Linux.** The code is one codebase and the CI is configured for
  Windows, macOS (x64 and arm64) and Linux, but only the Windows build and the
  whole test suite were run here. The Linux build needs `libasound2-dev`, which
  has been added to the CI's package list because `cpal` links ALSA.
* **A machine with no audio device.** The application handles it and says so; it
  was not exercised on one.
