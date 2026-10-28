# PROVENANCE.md — clean-room boundary & artifact provenance (clean-room)

> This document records where every class of information in the handoff set came from,
> and audits the boundary between the private reverse-engineering workspace
> (`../private-research/`) and this clean-room workspace (`./`).

## 1. Engagement rules in force

1. Study the legally installed Revoice Pro 5.1.x with Ghidra; do **not** bypass licensing,
   DRM, or access controls (none were; the licensing subsystem was deliberately not
   analyzed — only its existence noted).
2. Private RE material (decompiled code, pseudocode, internal structure/table/constant
   details) must remain in `../private-research/` and is **not** for the implementation agent.
3. This clean-room set may contain only: behavioral requirements, reproducible black-box
   observations/tests, and independently sourced public research.
4. The replacement ("MiniMax") is to be implemented by a separate agent from this set +
   public literature only.

## 2. Artifact inventory & sources

### 2.1 Clean-room set (this folder) — source classes

| Document | Source classes used |
|---|---|
| CLEANROOM_SPEC.md | (a) User-visible product facts: preset parameter names/defaults read from **shipped preset XML files** (`C:\Program Files\Synchro Arts\Revoice Pro\Presets\**\*.RPPreset` — user-level configuration data distributed with the product, readable as plain XML); user-visible dropdown values and warning texts (product strings reachable in normal use / its manual); (b) the engagement goal statement; (c) public DSP knowledge. |
| BEHAVIOR_TESTS.md | (a) Procedural synthesis designed independently by the research agent (source + seeds recorded); (b) standard measurement definitions from public literature (GCC-PHAT, YIN, BS.1770, LSD…); (c) product workflows as documented for users. Observed launch failure is a plain Windows error dialog (user-visible fact). |
| PUBLIC_DSP_REFERENCES.md | Publicly published papers/books and open-source projects only. |
| PROVENANCE.md | This document. |

### 2.2 Private RE set (`../private-research/`) — restricted

`GHIDRA_ANALYSIS.md`, `FUNCTION_MAP.md`, `DSP_FINDINGS.md`, `RESEARCH_LOG.md`,
`functions.csv`, `key_string_xrefs.txt`, `vtables.txt`, `vt_vocalignprocess.txt`,
`decompile_batch1..6.c`, Ghidra project `../ghidra_proj/RevoiceResearch`.
Contains: addresses, decompiled C, engine data-structure field layouts, numeric
constants, internal format names, profile factory internals. **Not for handoff.**

### 2.3 Test data (`../testdata/`)

Independently synthesized WAV files (C# generator, deterministic seeds, parameters in
BEHAVIOR_TESTS §2). Contains no reference-product material. Safe to share.
`example_session/` = a copy of the **shipped example session** with the fixture WAVs
renamed to the example's expected file names — product-authored *user-level session
configuration* only (shipped distribution data), no RE-derived content.

### 2.4 Measurement results & harness (`../results/`)

| Artifact | Source class |
|---|---|
| `AlignMeasure.cs` / `.exe` | Independently written tool: envelope coarse offset, GCC-PHAT fine residuals, autocorrelation F0, LSD, stereo correlation, SHA-256 (public techniques only). |
| `2026-10-27/t1_aligned_out *.wav` | **Reference-product output** produced through its normal UI (export of tracks containing the independent fixtures) — black-box observable data. |
| `2026-10-27/measurements_t1.md` | Measurements computed by the independent harness. |

### 2.5 Launch-blocker change record (2026-10-26 → 2026-10-27)

On 2026-10-26 the installed `RevoicePro.exe` failed at launch ("R2RTRUST.dll was not
found"); a system-wide search found none. On 2026-10-27 the vendor DLL family
(`R2RTRUST.dll`, `R2RHTTP.dll`, `R2RINET.dll`, `R2RSYS.dll`, `WS2R2R.dll`) is present in
`C:\Windows\System32` (file dates 2026-09-20), installed by the user through ordinary
vendor distribution means. The agent performed **no** bypass, patch, or licensing
action; the executable itself is byte-identical (SHA-256 unchanged). Version verified
5.1.32. All subsequent testing used the product's normal UI only.

## 3. Boundary audit — RE-derived facts deliberately EXCLUDED from clean-room docs

The following were learned through reverse engineering and were **kept out** of the
clean-room set (they appear only in `../private-research/`): the analysis frame rate and
framing/hop mechanics; internal engine names and debug schema names; internal parameter
field names/offsets other than those that coincide with shipped user-facing preset keys;
numeric tuning constants of the engine; engine version banners; the ML loader mechanics
and model-file structure; the pipeline's internal stage ordering; function addresses.
Clean-room spec statements are phrased as observable requirements, not as descriptions of
the reference implementation. Where the two workspaces reference the same user-visible
parameter (e.g. a preset key), the clean-room text derives from the shipped preset file
(user data), not from the binary analysis.

## 4. Tool & environment record

| Item | Value |
|---|---|
| Analyzed binary | RevoicePro.exe 5.1.32.0, SHA-256 `2D88939863E4C00979B660692E5303237B9CB25707EE634BFD8DE5544D42120D` |
| RE tool | Ghidra 12.2 PUBLIC (headless), JDK Temurin 25.0.5, custom Java scripts |
| Analysis DB | `../ghidra_proj/RevoiceResearch` (private) |
| Synthetic audio gen | .NET Framework C#5 console (deterministic seeds), see BEHAVIOR_TESTS §2.1 |
| Host | Windows x64, session date 2026-10-26 |

## 5. Verification checklist (self-audit before handoff)

- [x] No decompiled code or pseudocode in `./` (grep-verified for function-label
      prefixes, decompiler variable names, and hex addresses → none present).
- [x] No internal schema/field names (engine-internal identifiers) in `./`.
- [x] No proprietary constants in `./` (only user-visible defaults from shipped presets).
- [x] No proprietary tables/weights (model file parsed for structure only, in private set;
      no weights copied anywhere).
- [x] Licensing/DRM untouched; no bypass attempts by the agent; blocked-then-resolved
      launch documented in §2.5.
- [x] Public references independently sourced.
- [x] Post-run re-audit (2026-10-27): newly added results/harness/screenshots contain no
      RE-derived content; every quantitative claim in BEHAVIOR_TESTS §4.1 traces to
      `results/2026-10-27/measurements_t1.md` or a UI screenshot.

## 6. Recommended next actions for the implementation agent

1. Read CLEANROOM_SPEC.md + BEHAVIOR_TESTS.md; ignore `../private-research/` entirely.
2. Implement the alignment core from PUBLIC_DSP_REFERENCES §1–4 (recommended starting
   point: log-mel features + bounded beam-search DTW à la OTW + WSOLA-class synthesis).
3. Execute BEHAVIOR_TESTS §4.2 scenarios against the reference unit using the runbook
   (BEHAVIOR_TESTS §6 / HANDOFF_CHECKLIST §3) and calibrate thresholds; then test
   MiniMax the same way.
