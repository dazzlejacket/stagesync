# The repair of `CODEX_REPAIR_VERIFICATION.md`

This is the account of the second audit — `CODEX_REPAIR_VERIFICATION.md`, of
release `0a128df` and the branch `repair/post-codex` — and of the repairs made in
response. It is written for someone who wants to know *what is true now*.

Every finding of that audit is listed, in the priority order the audit set. Where
a finding was not repaired, the entry says so plainly and gives the reason and
the evidence. Two findings the audit itself did not report were found while
repairing it; they are in [§7](#7-two-defects-the-audit-did-not-report).

- Audit: `CODEX_REPAIR_VERIFICATION.md`, against `900dc06`.
- Repair branch: `repair/post-codex`.
- The real recordings: `audio/combined.wav` (48 kHz, 9685.460 s),
  `audio/Voice 005_W_20250208_152158.m4a` (44.1 kHz AAC, 4813.543 s),
  `audio/Voice 006_W_20250208_164548.m4a` (44.1 kHz AAC, 4051.650 s).

---

## Summary

| Finding | Subject | Outcome |
|---|---|---|
| P1 | The merged export lost 48 % of the material and displaced it by 15–29 s | **Fixed** |
| P2 | The log-mel cross-check converted frame indices with the wrong rate | **Fixed** |
| P2-13 | The calibration mapped the quiet flags in the opposite direction | **Fixed** |
| P3-01 | `SlopeFit::signal_to_noise` measured a rate, not a clock scale | **Fixed** |
| P2-08 | The AAC test reported success when `ffmpeg` failed | **Fixed** |
| P4 | `report.json` recorded staging paths and swallowed hash errors | **Fixed** |
| P4 | The path-comparison claims overstated what is folded | **Corrected** |
| P4 | The release workflow had no `fmt` or `clippy` gate | **Fixed** |
| P5 | The quiet threshold depends on whether the recording has a gap | **Measured; not changed** |
| P5 | The 90 s / 60 s gap case: 0, 1–2 or 10 trusted probes? | **Reconciled** — see [§6](#6-the-90-second-source-with-a-60-second-gap) |
| P5 | `AGENTS.md`'s normalisation rule did not describe the implementation | **Corrected** |
| — | The coarse lock was defeated by a recorder gap | **Found and fixed** — see [§7](#7-two-defects-the-audit-did-not-report) |

---

## 1. P1 — the merged export lost half its material

### The finding

`CODEX_REPAIR_VERIFICATION.md` reported that `timeline-merged` produced a file
whose material was 48 % missing and displaced by 15 to 29 seconds, and
attributed it to `b58bd8e`.

### What was actually wrong

The merged renderer walks each source's material once, span by span, and hands
each span to the caller. The cursor that remembers where a source has got to
advanced its position by the frames it **skipped** — the ones before the
requested span — and not by the frames it **handed over**.

Every span after the first therefore began from a stale origin: the renderer
read from further along the recording than the plan asked for, threw away
material it had already given away, and computed the next span's skip from the
wrong place. The export kept about half the material it was supposed to contain
and put it fifteen to twenty-nine seconds away.

Three further faults were in the same code and are fixed with it:

* the processors were created from the file's **declared** channel count rather
  than from the number of planes the decoder actually produced, so a file whose
  header disagreed with its content was rendered with the wrong number of
  channels;
* `pending_len` reported the **maximum** of the channels' pending lengths rather
  than the minimum, so a short read on one channel was hidden;
* the merged renderer reported `resampled: true` for every source, even for a
  source whose correction was a whole-sample copy.

### What was done

*Changed:* `crates/stagesync-core/src/render/mod.rs` (`SourceCursor`,
`render_merged_timeline`).

*Evidence:* `crates/stagesync-core/tests/merged_timeline_tests.rs` — six tests
that place every span where the plan says, read a source again at a later span,
honour a source's exact covered range, keep a source's own channel count, and
report `resampled: false` for a correction that copies.

The tests were checked against the code as it stood: reintroducing the defect
makes `a_merged_timeline_places_every_span_where_the_plan_says` fail at frame
320000 — a displacement of exactly twenty seconds — and
`a_merged_timeline_reads_a_source_again_at_a_later_span` fail at frame 240000.

### The verification on the real recordings

The merged export of the two watch recordings against `combined.wav`:

* 8734.6 s of audio and 950.9 s of silence placeholder;
* the file is exactly 464 902 080 frames long — the reference's own length;
* every one of the three uncovered spans is digital silence;
* 72 windows checked by an independent verifier (`work/pr108/verify_merged.py`,
  numpy and scipy, its own RIFF reader, the sources decoded with `ffmpeg`):
  **worst lag 0.0 ms, worst correlation 0.997**;
* the plan's spans sum to the length of the timeline.

The inputs were hashed before and after the run and are byte-identical.

### The source-aligned export

The other layout was exported as well, and its own **hold-out** check — which
measures the *rendered* file against the reference at positions the fit did not
use — is the evidence that the correction is real:

| | `Voice 005` | `Voice 006` |
|---|---|---|
| corrected length | 4813.62 s | 4051.72 s |
| drift applied | +16.061 ppm | +16.386 ppm |
| pitch change implied by the model | −0.0278 cents | −0.0284 cents |
| hold-out positions trusted | 142 of 160 | 104 of 134 |
| hold-out residual | p50 +2.126 ms, p95 +2.372 ms, max 6.948 ms | p50 +2.180 ms, p95 +2.386 ms, max 8.680 ms |
| drift the hold-out says was applied | out by +0.173 ppm ± 0.035 ppm | out by −0.368 ppm ± 0.095 ppm |

The residuals are measured on the rendered file, where they are flat if the
rate is right and a slope if it is wrong. A drift that was out by 20 ppm would
leave a residual of about 100 ms at the end of a file this long; the measured
residuals change by 0.173 ppm and 0.368 ppm over the whole file.

The disputed positions — 7 of 160 and 18 of 134 — are **excluded** from the
headline statistics rather than folded into a percentile, and they are named in
the report.

---

## 2. P2 — the log-mel cross-check converted frame indices with the wrong rate

### The finding

The spectral cross-check disagreed with the envelope fit on 259 of 319 positions
for `Voice 005` and on 181 of 263 for `Voice 006`, with a median disagreement of
−317 ms and a maximum of 685 ms.

### What was actually wrong

The log-mel extractor reported a **nominal** frame rate of `1000 / hop_ms`, and
the cross-check used it to convert frame indices into times. The nominal rate
and the real rate differ whenever the hop does not divide a whole number of
samples:

| file | sample rate | hop | nominal | actual | difference |
|---|---:|---:|---:|---:|---:|
| `combined.wav` | 48 000 Hz | 32 samples | 31.25 ms | 31.25 ms | 0 ppm |
| `Voice 005` | 44 100 Hz | 32 samples | 32.00 ms | 32.0017 ms | −143 ppm |

The cross-check walked the source's mel frames `probe += hop_secs * rate` with
the **nominal** rate, so it drifted 143.7 ppm behind the truth; and because the
cross-check reports the reference time **of the frame it probed**, its reference
times were short by 143.7 ppm × t — 0 ms at 15 s, 32 ms at 90 s, 320 ms at
2265 s and 544 ms at 3765 s.

The reference's half of the conversion was already right, because a 48 kHz file's
nominal rate and actual rate are the same.

### What was done

*Changed:*

* `crates/stagesync-core/src/features/mel.rs` — the extractor now keeps the
  sample rate it was given, and `LogMel::rate_hz` reports the **actual** rate,
  `sample_rate / hop_len`;
* `crates/stagesync-core/src/matching/fine.rs` (`spectral_offsets`) — each
  file's frame indices are converted with **that file's own** rate:
  `reference_time = (r0 + lag_frames + half) / reference_rate` and
  `source_time = p / source_rate`, the time of the frame actually probed rather
  than the fractional ideal probe.

The clock-drift sign convention and the confidence gates were not touched.

*Evidence:* `crates/stagesync-core/tests/spectral_timing_tests.rs` — five tests.
The decisive one builds a `LogMel` by hand and checks the frame arithmetic in
both directions (48 kHz reference against a 44.1 kHz source, and the
reverse). Against the code as it stood it fails with *"the reference time is
132.9920 s instead of 132.9732 s"*.

### The verification on the real recordings

| | before | after |
|---|---:|---:|
| `Voice 005`: spectral positions disagreeing | 259 / 319 | **0** |
| `Voice 005`: median point-by-point difference | 319.3 ms | **11.8 ms** |
| `Voice 005`: positions differing by more than 50 ms | 92 % | **0 %** |
| `Voice 006`: spectral positions disagreeing | 181 / 263 | **0** |
| `Voice 006`: median point-by-point difference | 233.2 ms | **11.3 ms** |
| `Voice 006`: positions differing by more than 50 ms | 90 % | **0 %** |

The drift figures and the offsets did not move.

---

## 3. P3 — three small findings

### P2-13 — the calibration mapped the quiet flags in the opposite direction

The resampler reads input at `alpha * output_index`, and the quiet flags used
`output_index / alpha`; for a non-unity scale those select different source
positions. The flags now use the same coordinate as the samples.

*Changed:* `crates/stagesync-core/src/calibration.rs`.

### P3-01 — `SlopeFit::signal_to_noise` measured the wrong thing

It computed `abs(slope) / SE`, which is the signal-to-noise of a **rate**, rather
than `abs(slope - 1) / SE`, which is the signal-to-noise of a **clock scale**.
The helper is now correct. The release decisions were already using the right
expression and did not change.

*Changed:* `crates/stagesync-core/src/matching/drift.rs`.

### P2-08 — the AAC test reported success when `ffmpeg` failed

The test returned quietly when the encode did not happen, so the compressed path
was counted as covered when it had never been checked. It now **fails loudly**,
with `SKIP-UNVERIFIED` and the reason, when `ffmpeg` is missing, fails, or
produces a file of one kibibyte or less.

*Changed:* `crates/stagesync-core/tests/robustness_tests.rs`.

**Three things had to be settled before the test could pass at all**, and each
is a finding of its own:

1. **ffmpeg's AAC encoder refuses the project's own WAV files.** The float WAV
   writer emits `WAVE_FORMAT_EXTENSIBLE` with a **zero channel mask**, which
   the specification leaves unspecified; ffmpeg's demuxer therefore reports the
   layout as unknown, and its AAC encoder refuses a stream whose layout it does
   not know — it fails with *"Unsupported channel layout"*. Its other encoders
   (MP3, FLAC, PCM) accept the same file. The test now states the layout on the
   command line (`-channel_layout mono`), which is what the file's data says.
   The writer was **not** changed; see
   [§8](#8-findings-that-were-not-repaired).
2. **The AAC encoder only accepts a fixed list of sample rates.** A source at
   7200 Hz therefore had to be resampled by the encoder before it was encoded,
   and that resampling degraded the envelope enough that three of the five
   envelope probes locked onto the wrong place while the independent spectral
   check stayed correct. The compressed test's source is therefore written at
   44.1 kHz, which the encoder accepts as it is, so the encode changes nothing
   but the compression. The other tests in the file keep their 7200 Hz source.
3. **An AAC encoder primes the stream with 1024 samples.** The decoded
   recording is 1024 samples (23 ms at 44.1 kHz) later than the file it was
   made from, and the decoder does not take those samples back out. The test's
   assertion is therefore what its name says — *the compressed recording is
   matched like an uncompressed one* — and it is made by comparing the two
   analyses directly: the difference between them has to be **one constant**
   (the codec's priming delay) and nothing else. Measured: a median of
   −20.9 ms with a spread of under 2 ms.

---

## 4. P4 — the report, the claims and the gates

### `report.json` recorded the staging paths

The report recorded the path of the **temporary** file each artefact was written
to, so every path in it named a file that no longer existed. A hash error or a
size error was logged and then discarded, and the export was reported as
successful anyway.

*Changed:*

* `crates/stagesync-core/src/export/mod.rs` — `StagedExport::final_path` maps a
  staged artefact to the path it will occupy once committed;
* `crates/stagesync-cli/src/main.rs` and
  `crates/stagesync-desktop/src/jobs.rs` — the artefact carries the **final**
  path, and a hash or metadata error **aborts the export** with its context
  rather than being logged and dropped.

`Artifact::sha256` stays an `Option` for JSON compatibility, but it is now
always populated.

### The path-comparison claims overstated what is folded

The comments claimed that the comparison folds case **and** Unicode
normalisation. It does not: it folds case on Windows and exactly elsewhere, and
it uses the canonical path for a file that exists. Unicode normalisation is
**not** folded, and folding it would be wrong on Windows, where NTFS does not
normalise. The claims were corrected rather than the behaviour changed.

*Changed:* `crates/stagesync-core/src/render/mod.rs`.

### The release workflow had no `fmt` or `clippy` gate

A release can be cut from a tag that never went through CI, so the release
workflow now repeats both gates itself: `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings`. `rustfmt` and `clippy`
were added to the toolchain components, and `ffmpeg` is installed explicitly —
the AAC test now fails loudly without it.

*Changed:* `.github/workflows/release.yml`, `.github/workflows/ci.yml`.

---

## 5. P5 — the noise-floor coupling: measured, and deliberately not changed

The quiet threshold is a fixed percentile of the frame-level distribution. A
recording that contains digital silence therefore gets a completely different
threshold from the same recording without it, because the percentile lands
inside the silence instead of on the recorder's own noise floor. On the audit's
own fixture the difference is **121.5 dB**:

```
quiet threshold: with a gap -155.51 dB, without -34.00 dB, a difference of 121.50 dB
```

The audit recommended estimating the floor from the frames that are not digital
silence, so that the two recordings would get comparable thresholds.

**That change was not made.** The coupling is real, and it was measured, but it
does not move the correspondence:

| | coarse offset error | trusted probes | probe errors |
|---|---:|---:|---|
| the recording with a gap | **+0.000 ms** | 1 | −0.021 ms |
| the same recording without the gap | **+0.000 ms** | 2 | +0.295 ms, +5.000 ms |
| the gapped recording, threshold moved by hand to the ungapped one's | **+0.000 ms** | 1 | −0.034 ms |

The third row is the coupling isolated: the *same* file, correlated with the
*same* reference, with only the threshold moved. The alignment does not change.

Changing the rule would have moved the quiet threshold of **every** recording
that contains any digital silence — including the two real watch recordings,
whose thresholds are currently −174 dB because their AAC decoders produce exact
zeros — without improving the correspondence by any measurable amount. That is
a change with a risk and no measured benefit, so it was not made.

*Evidence:* `crates/stagesync-core/tests/gap_alignment_tests.rs` —
`the_quiet_threshold_depends_on_whether_the_recording_has_a_gap` records the
coupling as it stands, and `the_alignment_survives_the_noise_floor_coupling`
shows that the alignment does not depend on it.

---

## 6. The 90-second source with a 60-second gap

The audit's §3.7 proposed a test that asserts *"at least 10 probes are trusted"*
for this case, and in the same paragraph said that the current implementation
delivers *"1–2 trusted probes at ~0.1 ms"*. The two statements contradict each
other, and the audit's own table measured **1**.

**Ten trusted probes is arithmetically impossible for this fixture.** The fine
stage probes every 15 s over a window 30 s wide, so a 90-second source has at
most four probe positions in total, and a source of which 60 s is a gap has room
for one. No implementation can produce ten.

The audit's figure of *1–2 trusted probes at ~0.1 ms* was measured by its own
harness, which searched its fine probes around the **ground-truth** model. The
**pipeline** searches around the **coarse lock's** model — and that model was
wrong, which is the defect in [§7](#7-two-defects-the-audit-did-not-report).
So the audit's harness and the pipeline disagreed, and the audit's table did not
carry a coarse-lock column for the short cases.

Measured on the audit's own fixtures, before and after the repair:

| fixture | coarse offset error before | after | trusted probes after | p50 after |
|---|---:|---:|---:|---:|
| 90 s, no gap | 0 ms | 0 ms | 4 | 0.208 ms |
| 95 s, 15 s gap | **+87.5 s** | **0 ms** | 3 | 0.009 ms |
| 90 s, 30 s gap | **+144 s** | **0 ms** | 2 | 0.043 ms |
| 90 s, 60 s gap | **+10 s** | **0 ms** | 1 | 0.111 ms |
| 90 s, 75 s gap | **+41.5 s** | **0 ms** | 1 | 0.121 ms |
| 690 s, 90 s gap | 0 ms | 0 ms | 22 | 0.011 ms |
| 840 s, no gap | 0 ms | 0 ms | 25 | 0.233 ms |

The trusted-probe counts and the probe errors now reproduce the audit's own
harness numbers exactly.

*Evidence:* `crates/stagesync-core/tests/gap_alignment_tests.rs` — four tests.
Against the code as it stood, three of the four fail: the coarse lock came out
**106 000 ms** from the truth, the gapped recording produced **no** trusted
probe at all, and the gap-placement test found no model.

`AGENTS.md`'s normalisation rule was also corrected: it described a rule the
implementation does not follow, and the implementation's rule — the statistics
are taken from the signal frames only, *because* that makes the normalised
shape independent of the silence beside it — is the one that was measured to
work.

---

## 7. Two defects the audit did not report

### The coarse lock's whole-file correlation was not masked

The coarse stage correlates the whole source against the whole reference to
break the tie between the windows. That correlation was **not** masked, while
every window correlation was. A recorder gap therefore contributed frames
carrying no information to the coarse stage's most reliable measurement, and
correlated them against the reference's music.

The consequence was not a small error. On the audit's own fixtures the coarse
lock was **87.5 s, 144 s, 10 s and 41.5 s** away from the truth, and the fine
stage — which searches around the coarse lock's model — inherited the error.

### The coarse stage ranked its peaks by the wrong criterion

The peaks were ranked by the matched-filter score with a **global**
normalisation. That score is linear in the reference's amplitude, so it is
largest where the reference happens to be most dynamic rather than where the two
envelopes agree best. On the 30-second-gap fixture, the overlap-normalised
value at the true lag was **0.7455** against **0.6550** at the wrong lag it
chose, while the global score said the opposite.

`AGENTS.md` records that ranking by the overlap-normalised score lets
short-overlap edge artefacts outrank real matches. That is true of the **fine**
stage, whose correlation is against a region barely longer than its own window,
and it is why `find_peaks` ranks the way it does. It is not true of the coarse
stage, whose window is correlated against the whole reference and whose overlap
is bounded by the same reliability rule. The two stages now use different
criteria, each the one that was measured to be right for it.

*Changed:*

* `crates/stagesync-core/src/matching/dsp.rs` — `find_peaks_by_ncc`, the
  coarse stage's ranking, with the same reliability rule `find_peaks` uses;
* `crates/stagesync-core/src/matching/coarse.rs` — the whole-file correlation
  is masked, and both the windows and the whole file are ranked by
  `find_peaks_by_ncc`.

**The real recordings are unaffected.** The coarse lock's model is
byte-identical before and after the change — the same 317.400 s and 5753.350 s,
with the same 15 and 13 votes — and so are the fine probes and the direct
model. Only the *candidate list* in the report changes.

---

## 8. Findings that were not repaired

1. **The robust fit cannot reject a gross outlier when only three or four
   probes are trusted.** Theil–Sen's estimator takes the median of the pairwise
   slopes; with four points and one outlier, three of the six slopes are the
   outlier's and the median of the sorted list lands on one of them. Measured:
   a probe 11.5 s out was trusted with a confidence of 0.507, and the resulting
   offset was 3.8 s wrong. The **drift** was correctly refused — the fit's
   signal-to-noise was 1.9, below the factor of three — but the **offset** was
   not protected. This is a defect in its own right, in the small-sample
   regime; it was not repaired here because it is a separate policy question
   (how many probes a line fit may rest on) and the brief did not ask for it.
2. **The fine stage never probes the last half-window of a source.** The probe
   loop requires the whole 30 s window to lie inside the source, so material in
   the final 15 s of a recording is never probed — which is why the 90 s / 60 s
   gap case has one trusted probe and not two.
3. **The float WAV writer emits `WAVE_FORMAT_EXTENSIBLE` with a zero channel
   mask.** Most tools read the file correctly; ffmpeg's AAC encoder does not.
   The writer was left alone because changing it changes the bytes of every
   export, and the interop problem is narrow and now documented and worked
   around in the test.
4. **The noise-floor coupling** — [§5](#5-p5--the-noise-floor-coupling-measured-and-deliberately-not-changed).
5. **The envelope's ten-millisecond hop does not divide evenly at every sample
   rate** (at 22 050 Hz a hop is 220.5 samples). The envelope's rate is the
   nominal 100 Hz by construction and both sides of a comparison share it, so
   the matcher is consistent; but a frame index from an envelope must never be
   converted with a different rate, which is the mistake P2 was about.

---

## 9. The gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | **pass** — exit 0 |
| `cargo test --workspace` | **pass** — 172 passed, 0 failed, exit 0 |
| `cargo build --release --workspace` | **pass** — exit 0, no warnings |

The test count was 157 before this pass and 172 after it; the fifteen new tests
are the three suites added here (six for the merged timeline, five for the
log-mel timing and four for the recorder gap). No lint rule was weakened and no
test was removed to make a run green.

The inputs were hashed before and after every run and are byte-identical:

```
048B29F3B02F3B61D20E61BDB0B4E24FCCF7D4B209609B45212340939A836026  combined.wav
ED9A8E1775E3814EC25E29314F17B82445BCB00DD9BB9E4D0FB7386EBB9D3C0B  Voice 005_W_20250208_152158.m4a
DC57EAF59B9452DCC1C94427818AC698A2593CA6421F62D47417C93921A209D9  Voice 006_W_20250208_164548.m4a
```
