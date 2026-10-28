# CLEANROOM_SPEC.md — "MiniMax" precise audio alignment (clean-room handoff)

> **CLEAN-ROOM DOCUMENT — safe for independent implementation.**
> Contains only: (a) user-visible product behavior and parameters of the reference
> product (Revoice Pro 5.1.x, as shipped in its preset files/manual), (b) black-box test
> requirements, (c) engineering requirements derived from the stated goal
> ("precise audio alignment"). No reverse-engineered internals appear here — see
> PROVENANCE.md for the boundary audit.

## 0. Objective

Build an open-source replacement focused on **precise audio alignment**: given a
**Guide** (reference) audio signal and a **Dub/Target** (to be aligned) signal of the
same material, produce a time-warped, optionally pitch/level-adjusted version of the
Dub that matches the Guide's timing with high precision and no audible artifacts.
The reference product's user-facing behavior defines the target UX; the implementation
must be developed from this spec, public literature (PUBLIC_DSP_REFERENCES.md), and the
black-box test results (BEHAVIOR_TESTS.md) only.

## 1. Product context observed (user-visible facts only)

- The reference product aligns a Guide to a Dub using named presets such as
  "Tight - Timing", "Slightly Loose - Timing", "Dialogue - Align Timing",
  "…+ Pitch Match", "…+ Tune Harmony" (shipped preset names).
- Exposed adjustment parameters (from shipped preset files, i.e. user data):
  `TightnessOfAlignment`, `TightnessOfPitch`, `TightnessOfLevel`,
  `StrengthOfPitchCorrection`, `OffsetOfPitchCorrection`, `FormantsEditing`,
  `TargetShiftOfFormants`, `LevelGainOffset`, `AttackTimeOfLevel` (default 0.01 s),
  `DecayTimeOfLevel` (default 0.03 s), `StrengthOfLevelCorrection`,
  `UseDubForAverageLevel`, `SensitivityToTransients` (default 72),
  `MaxTimeShift` (default 5.0 s), `SoftLimiting`, `TuningStrength` (default 100),
  `Variation` (default 10), `FilterLength` (default 49), `RenderQuality` (default 10),
  `ProtectTransients`, `UseDubAsRef`, `UsePitchInAlignment`, `SmoothPitch`,
  `AdvancedPitch`, `NoteSeparator`, `PitchEditingMode`, `SelectivePitchTransferMode`,
  `DisplayFrequency`, plus pitch analysis band limits
  (`GuideLowestFreq`/`GuideHighestFreq`/`DubLowestFreq`/`DubHighestFreq`,
  default 60–850 Hz).
- Flexibility presets offered to users: **Lowest / Low / Normal / High Flexibility** and
  **Normal Flexibility + Pitch** (user-facing dropdown values).
- Users can place **waypoints** (manual Guide↔Dub anchor pairs), including
  "Fixed Range" anchors; vibrato areas are protected from manual warp edits.
- The product warns users when: no signal is detected in guide/dub audio; no pitch is
  detected in guide/dub audio; the start of the dub signal cannot be found; a region is
  longer than 30000 s; the background before the Guide is longer than before the Dub
  ("may result in the Aligned Dub starting too early").
- Processing is per region ("Process Control Block"); users are advised to include
  ~0.25 s of background audio before the aligned material, with slightly more
  pre-roll on the Dub than the Guide.

## 2. Functional requirements (MiniMax)

| ID | Requirement | Acceptance |
|---|---|---|
| F1 | Ingest Guide and Dub audio (any sample rate 8–192 kHz, mono or multichannel; channels aligned independently unless stereo-linking enabled) | Loads all formats the target DAW ecosystem provides (WAV/AIFF/CAF minimum) |
| F2 | Automatic global offset lock: find the Dub's temporal correspondence to the Guide start without user input | Correct lock on BEHAVIOR_TESTS T1–T6 |
| F3 | Automatic monotone time-warping of the Dub onto the Guide timeline (timing-only mode) | Drift/local-deviation metrics per BEHAVIOR_TESTS |
| F4 | Optional pitch-contour transfer from Guide to Dub with strength/tightness controls and semitone offset | Measured F0 of output approaches Guide contour proportionally to strength |
| F5 | Optional level (loudness-envelope) transfer with attack/decay times and strength | Envelope correlation improves vs input; no pumping beyond configured attack/decay |
| F6 | Manual waypoints force exact local correspondence at user anchors | With anchors set, output passes exactly through anchors |
| F7 | Flexibility profiles bound how much local compression/expansion is allowed (Lowest…High) | Bounded stretch ratio per profile (values calibrated by BEHAVIOR_TESTS T3) |
| F8 | User-settable maximum timing shift (`MaxTimeShift`, default 5 s): beyond it, prefer copying over shifting | T2 with offsets > max |
| F9 | Transient handling: onsets preserved/protected (`ProtectTransients`, `SensitivityToTransients`) | Onset deviation metric; no pre-echo/smearing at onsets |
| F10 | Silence/missing-content policy: silence in Dub vs Guide resolves gracefully (map silence↔silence; keep lengths consistent) | T4 |
| F11 | Robust pre-flight analysis: report (as product does) when guide/dub lacks signal or pitch | Diagnostics implemented; tested in T4/T5 |
| F12 | Render multi-threaded, block-based; support 2-hour inputs in bounded memory (streaming or chunked) | T7 memory/time gates |
| F13 | Export aligned audio bit-exact-repeatable (same input+params ⇒ same output) | Hash-identical reruns |
| F14 | Stereo preservation: for stereo Dub, maintain inter-channel phase relationships (linked warp decisions) | L/R alignment curves identical in linked mode; no image shift |

## 3. Non-functional requirements

- **Audio quality**: no audible artifacts on sustained vocals at 44.1/48 kHz: log-spectral
  distance on static (unwarped) segments ≤ calibration threshold; no aliasing above the
  configured analysis band; pitch-shifted material retains formant character
  (`FormantsEditing` default on).
- **Precision**: on controlled synthetic tests, estimated local timing error of output
  vs Guide ≤ 2 ms (RMS) for clean signals; ≤ 10 ms for degraded (noise/EQ) signals.
  (Calibrate against the reference unit's measured performance when available.)
- **Determinism**: fully deterministic pipeline (fixed seeds, no time-dependent behavior).
- **Performance**: processing slower than real time ×2 or better on a modern 8-core CPU
  for 48 kHz mono; 2-hour stereo file completes without exceeding memory budget.
- **Licensing-clean implementation**: only permissively licensed or original components
  (or GPL components if MiniMax is GPL); no code from the reference product.

## 4. Parameter model (defaults mirror shipped user data)

```
alignment.tightness        : 0..100   (default per "Tight/Slightly Loose" presets; calibrate)
alignment.flexibility      : enum lowest|low|normal|high  (default normal)
alignment.use_pitch_feature: bool     (default false; "Normal Flexibility + Pitch" analog)
alignment.max_time_shift_s : float    (default 5.0)
alignment.sensitivity_transients: 0..100 (default 72)
alignment.protect_transients: bool   (default false)
pitch.transfer_strength    : 0..100
pitch.correction_offset_semitones : float
pitch.tuning_strength      : 0..100   (default 100)
pitch.variation_pct        : float    (default 10)
pitch.analysis_band_hz     : [60, 850] default
pitch.smooth               : bool
level.transfer_strength    : 0..100
level.attack_s             : default 0.01
level.decay_s              : default 0.03
level.smoothing_filter_taps: default 49
level.use_dub_average      : bool
common.render_quality      : int      (default 10; semantics calibrated by test)
```

## 5. Reference architecture suggestions (public techniques only)

- Frame-level feature extraction (log-mel / MFCC / chroma hybrids; see
  PUBLIC_DSP_REFERENCES.md §2–3) on both signals.
- Monotone alignment path via **DTW family** with pruned/beam search and user anchors
  (§4), robustified with saturation/clipping of frame costs (soft-DTW or explicit
  cost caps) to survive noise/EQ mismatch.
- Warp map → time-varying synthesis: WSOLA-class or phase-vocoder time-stretching with
  transient-detection gating (§5), pitch transfer via PSOLA/harmonic-percussive methods (§6).
- Envelope followers + smoothed gain application for level transfer (§7).
- Independent pitch tracker (YIN/pYIN/CREPE) for pitch features and presence checks (§3).

## 6. Interface expectations

- CLI + library API: `minimax align --guide G --dub D --out O [--params JSON]`,
  returning per-region alignment statistics (mean/max deviation, stretch range used).
- Machine-readable run report (JSON) equivalent for test automation.

## 7. Observed reference-product behavior (black-box, 2026-10-27, v5.1.32)

Statements below are externally observable facts from the executed T1 run; they are
requirements inputs for MiniMax, not implementation knowledge.

- **Session model**: audio lives in regions on named tracks; alignment is configured as
  a per-region "process" with three visible pieces: the Guide input region, the Dub
  input region, and a newly created **output track suffixed `[OUT]`** carrying the
  rendered result as audio.
- **Configuration flow**: roles are assigned from a Tracks menu (guide singular, dubs
  plural); the process dialog then requires: process type (default
  "Match Timing / Pitch / Level"), a **time source** (observable options: "No Source",
  "Selected Playback Range" — a blue ruler range drawn by dragging in the strip under
  the ruler), a preset (hierarchical: Recent/Factory/User/Shared; factory list includes
  "Default", "11 Slightly Loose- Timing [Default]", "21 Tight- Timing",
  "31 Dialogue- Align Timing" family, plus pitch-match variants), and a track list.
- **Process creation is validated silently**: with incomplete fields the create action
  produces no effect and no message; with complete fields it succeeds without a
  confirmation step.
- **Rendering** is automatic after creation (output waveform appears within seconds;
  no separate render command observed for this size of material).
- **Export** (File ▸ Export Audio…) yields one WAV per enabled track named
  `<base> <track>.wav`; a status line reports
  "Export Starting at: … Ending at: …"; track enablement via an "Enable All" button;
  "No tracks selected" is shown as validation text.
- **Export sample rate defaults to 44.1 kHz** even when the session audio is 48 kHz
  (observable resampling; type combo offers other rates).
- **Time modification is audible/measurable**: in T1 (timing-only preset) the exported
  output duration changed from 1.835 s (input) to 1.900 s (+3.5 %) while pitch remained
  within **2.73 ¢ mean absolute error** of the guide on jointly-voiced frames — i.e.
  timing edits occurred without pitch transfer under a timing-only preset.
- **Spectral character**: LSD between output and guide ≈ LSD between unaligned dub and
  guide (1.14 vs 1.17 dB) ⇒ the timing edit did not impose large spectral change.
- **Session file**: `.rvp` files are binary and reference audio by file name (relative
  resolution works); sessions reopen with roles, ranges, and selected preset intact
  (the process dialog remembered "21 Tight- Timing" across reopen).
- **Input robustness requirements** (from product's own user-visible diagnostics, to be
  matched): reports when no signal is detected in guide or dub, when no pitch is
  detected, when the dub start cannot be found, and refuses regions > 30000 s.

## 8. Out of scope

DRM/licensing of any kind; cloning proprietary UI; installer behavior; network features.
