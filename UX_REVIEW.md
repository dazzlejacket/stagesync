# StageSync UX review

2026-10-29; HEAD `0a128df`. Companion to CODEX_AUDIT.md. **S** = implementation inspection; **E** = reproduced behavior; **D** = design recommendation.

## Product judgment

**The current GUI is not ready for stable public release. It can become a useful technical beta without a wholesale redesign.** Fix export correctness/safety first. For beta, fix navigation, override behavior and truthful status. For stable 1.0, add waveform-based inspection and synchronized A/B audition to make automatic results understandable and checkable.

A conventional waveform timeline is warranted, but dragging clips into sync must remain optional. The product should stay **IMPORT → AUTO SYNC → INSPECT → OPTIONAL CORRECT → EXPORT**. Keep the Rust engine and its measurement/reporting architecture.

## What was actually inspected

Read the actual app/state/jobs/timeline/platform/settings implementation and workflow/layout tests. A real release launch was attempted through the computer-use skill. Physical Escape stopped Computer Use; after the user authorized retry, the helper still returned the interruption. No new live capture or successful interactive session was obtained. Do not mistake source inspection, a responsive process, previous screenshots or a headless width assertion for an observed GUI workflow. Multiple live window sizes, DPI, themes, keyboard/screen-reader behavior and first-time-user success are **not verified** here.

The existing layout test passed in the serial 133-test run. It creates an empty/default app at 960×640, 1400×900 and 2560×1440 and asserts center width. It does not exercise loaded recordings, populated results, scrolling, text clipping, dialogs, playback or actual DPI (`crates/stagesync-desktop/src/app.rs:1540`–`:1585`).

## Present workflow and answers to the owner's questions

| Question | What exists now / judgment |
|---|---|
| 1. Automatic shared placement or offsets only? | **S:** Analysis calculates models and paints automatically positioned segment lanes on the reference time axis. It also exports corrected sources or timeline layouts. It is more than a table of offsets, but not an audio clip editor. |
| 2. Starts, ends, overlaps and gaps visible? | **S:** One merged coverage lane plus one lane per recorder, a ruler and colored/hatched segment kinds. There is no dedicated reference waveform track. **E:** nested overlaps can produce false uncovered spans; merged output ownership can differ from displayed segment ownership. Fix this before relying on the picture. |
| 3. Three-hour timeline understandable? | **S:** Horizontal scrolling and a logarithmic zoom exist. Fit cannot fit three hours into an ordinary center panel because its minimum 0.2 px/s requires 2,160 logical pixels. No waveform transients or overview navigator support close inspection. |
| 4. Would waveforms materially help? | **D:** Yes: users can inspect starts/ends, transients, recorder holes and repeated sections in context. Waveform appearance alone must not be called independent proof of clock drift. |
| 5. Zoomable tracks without a DAW? | **D:** Recommended. Read-only peak tracks, common ruler, overview, selected-region details and audition are enough; no mixer, plugin host, destructive editing or elaborate arrangement system is needed. |
| 6. Manual dragging/nudging? | **S/E:** There is no clip dragging/nudging. Numeric source/reference anchor fields exist. The final waveform model can discard that claimed hard constraint (50 ms reproduction in CODEX_AUDIT P2-01). **D:** make a reversible offset nudge/anchor fallback after repairing this contract. |
| 7. Listen together / switch reference-target? | **S:** No audio playback, cursor, transport, A/B preview or synchronized listening. There is no decorative fake Play button. External listening is needed today. |
| 8. Synchronized vs uncertain vs unmatched? | **S:** Matched, low confidence, uncovered reference, recorder gap and beyond-reference have colors/labels; holes are hatched. This is a useful foundation. No click interaction on individual painted spans (`Sense::hover`) or warning-to-region navigation exists. Correct false coverage first. |
| 9. Are measurements understandable? | **S/D:** Detailed offset, drift ± SE, methods, residuals, warnings and charts are exposed. The offset-only drift verdict is clearer than an unexplained zero. Nevertheless ppm, GCC-PHAT, residual RMS and percentage coverage dominate; normal users need a short outcome in milliseconds plus expandable evidence. A 49.6% coverage figure describes reference coverage, not 49.6% sync quality. |
| 10. First-time import/sync/export without docs? | **S:** Add files/drop folder → Analyze → Export is available; the first imported file becomes reference. **Not observed:** novice success. Reference selection is implicit, export geometry needs explanation, inspection is passive and controls/details can overflow. A documented path is not a usability test. |

## What should stay

- Automatic analysis as the primary action; no required manual alignment.
- The reference remains unchanged and visible as the time authority.
- Per-source results and explicit uncovered/uncertain spans.
- Two estimator opinions, measured uncertainty, rejected measurements and no fabricated continuity.
- Background work and honest export cancellation labeling.
- Separate source-aligned and reference-timeline export choices, once actual lengths/content match their labels.

## Must fix before beta

### 1. Make the timeline describe the actual export [E]

CODEX_AUDIT P1-02 through P1-05 identify audio/coverage errors. Their UX consequences are severe: a plausible green timeline can accompany folded stereo, wrong playback speed, false gaps or the wrong output length. Use one validated coverage/owner plan for display and rendering. Show the chosen source at overlaps and distinguish actual missing material from weak matching and performance silence.

### 2. Make every essential control reachable [S/D]

References: `crates/stagesync-desktop/src/app.rs:714`, `:965`, `:1018`–`:1045`, `:1511`; `src/main.rs:24`.

Compute Fit from available viewport width with a minimum low enough for the full recording. Provide vertical scrolling for file lists and result content, responsive/collapsible side panels and a wrapping toolbar. Test populated sessions at 720×450 logical points, 960×640, 1400×900 and 2560×1440, at 100/150/200% scaling where the machine supports it. Do not change system display settings without separate authorization. Preserve keyboard access to Auto Sync, Cancel, the selected source and Export.

### 3. Repair or temporarily remove the unreliable manual override [E/D]

References: `crates/stagesync-core/src/pipeline.rs:381`, `:533`–`:568`; `crates/stagesync-desktop/src/app.rs:1050`–`:1146`.

The current hard-anchor wording is false when the waveform fit wins. Make a final override an explicit, persisted layer on the automatic model. Show conflicts instead of silently ignoring an anchor. Key it by recording identity, not basename. Offer **Reset to automatic**. Every edit invalidates prior rendered verification and output links until the updated render is checked. If this cannot be made reliable before beta, disable the anchor control with a truthful limitation notice.

### 4. State what has actually been verified [S/D]

Display a concise status for each target: **Auto aligned**, **Needs inspection**, **No reliable match**, **Override applied**, **Export not verified**, or **Export verified at N sampled positions**. Keep analysis confidence separate from rendered-export verification. Show **Insufficient data** prominently rather than leading with 0.000 ms when no trusted measurements exist. Disputed positions must remain visible, not disappear behind a low p95. Label partially completed exports and files left after cancellation. Connect report/hash results to the exact export generation.

### 5. Make the reference and export geometry explicit [S/D]

The first dropped file becomes reference (`app.rs:390`–`:426`). Add a deliberate reference affordance, such as Set as reference on an imported row, with clear invalidation of prior results. Do not assume directory enumeration supplies the intended guide first.

Explain source-aligned export in practical terms: it changes recorder rate but does not add timeline-leading silence; use the reported placement offset. Explain reference-timeline output as a common zero point and reference duration, with silence placeholders. Show a short planned-output summary before Export. Keep sample rate and file format under advanced export options with sensible defaults.

These are essential clarity/reliability fixes, not requests for a new editing application.

## Recommended stable-release inspection layout [D]

Use three regions:

1. **Compact header:** reference identity, Auto Sync, progress/cancel and overall “2 aligned · 1 needs inspection” state. Advanced estimator switches in a disclosure, with their consequences explained.
2. **Main timeline:** reference peak waveform at top; one automatically positioned waveform row per target; pinned track names; common time ruler; colored match/uncertainty bands; hatched true gaps; a compact full-performance overview. Keep a merged-owner view available as an overlay or additional lane.
3. **Selection inspector / export drawer:** selected target or warning region, plain-language outcome, position/drift impact in milliseconds, verification sample count/status and expandable numerical evidence. Export controls can collapse until needed.

Use min/max peak envelopes in a multiresolution cache, streamed from read-only inputs. Include recording hash, channel selection, decode/cache format and resolution in cache identity; bound memory/disk usage and support cancellation. Preserve stereo/channel visibility rather than arbitrarily averaging antiphase content to silence. Rendering a three-hour waveform must not decode the entire file into UI memory.

Zoom around the cursor/selection; pan horizontally; Fit all, Fit selection and jump-to-start/end/gap. A few overview pixels can represent minutes; inspection zoom should reveal transients at millisecond scale without suggesting that pixel precision establishes measurement uncertainty. Add timecode and accessible text/patterns so color is not the only signal.

### Low-confidence region interaction

Click a warning or colored span to select that exact reference/source interval, zoom with context and show why it was uncertain: weak peak, repeated section, disagreement, extrapolation, recorder gap or no reference overlap. List the affected duration and whether it will export as source material or a silence placeholder. Offer Audition, Reanalyze with an anchor, and Reset to automatic when implemented. Do not present an unexplained percentage as a probability of correctness.

### Playback and verification

Add a shared cursor and region audition with reference/selected-target A/B at the same reference time. Use the actual corrected mapping/output so audition does not restate a different preview path. Keep native levels and make any optional listening attenuation explicit; do not introduce automatic gain into exports. A controlled two-source listening mode may help detect flams, but mixing should not hide independent levels or imply that different microphones must have identical waveforms.

Playback is a user confidence aid, not a replacement for independent rendered-output verification. Close transients should have jump/loop controls. No requirement for video retiming, a DAW mixer, effect plugins or destructive cuts is implied.

### Optional correction model

Start with one advanced **Offset nudge** in milliseconds and one source/reference anchor. An optional horizontal drag can edit that same offset, with explicit fine-step keyboard nudges. Preserve the automatic slope by default. Do not let a casual drag silently refit recorder drift. Show automatic placement, override amount and resulting placement separately; require re-render/reverification for an accuracy statement.

Keep undo/reset and an explicit override log in the project/report. Multiple anchors or piecewise correction should follow only when supported by actual measured discontinuities and implemented verification; a generic piecewise type does not make that feature finished. Never bridge missing material to satisfy an anchor.

## Suggested implementation order

| Order | Work | Acceptance evidence |
|---|---|---|
| 1 | Repair input/output safety and renderer/coverage defects | Independent decoded samples, exact lengths/channels/rates, preserved input hashes, cold parallel gates |
| 2 | Fit/scroll/responsive layout, explicit reference, verification/partial states | Live populated Windows sessions at practical sizes/scales; keyboard walkthrough; no unreachable actions |
| 3 | Reliable final override/reset and generation tracking | Automatic result preserved; exact anchor honored or clearly rejected; stale verification cannot attach |
| 4 | Streaming read-only waveform overview and zoom tracks | Three-hour memory/cancellation test, correct channel handling, common ruler, warning-to-region navigation |
| 5 | Mapped A/B audition and region cursor/loop | Transient check against exported audio; no export gain changes; safe cancellation/error recovery |
| 6 | Optional nudge/drag and polish | Reversible edits, explicit milliseconds, reverified output; short novice study on import → export |

**Beta boundary:** steps 1–3, truthful limitations and Windows package/runtime validation. Waveforms/A-B can be announced as absent. **Stable boundary:** demonstrated inspection/verification workflow, steps 4–5 or equivalent compelling usability evidence, native platform validation, and all correctness blockers resolved. Optional dragging, richer visual styling, session refinements and extra shortcuts can follow later.

## MiniMax handoff

Keep automatic alignment central. First make the audio, coverage and reports agree. Then make a long populated session navigable, make overrides truthful, and show whether the actual export was verified. Add bounded waveform tracks and synchronized A/B inspection incrementally. Do not spend the next pass cloning another product's UI or building a DAW while these release blockers remain.
