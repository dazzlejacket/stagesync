# HANDOFF_CHECKLIST.md — MiniMax clean-room handoff (clean-room)

> Status board for the implementation agent. Everything here is externally observable
> behavior, independent tooling, or pointers into the other clean-room documents.
> `../private-research/` remains restricted.

## 1. Deliverable status

| Deliverable | State |
|---|---|
| Ghidra analysis (private) | ✅ complete — see `../private-research/` (not for this agent) |
| CLEANROOM_SPEC.md | ✅ updated with observed behavior (§7) from live T1 run |
| BEHAVIOR_TESTS.md | ✅ protocol fixed (coarse→fine), T1 executed, T2–T8 specified |
| PUBLIC_DSP_REFERENCES.md | ✅ stable |
| PROVENANCE.md | ✅ updated incl. launch-blocker change record (§2.5) |
| Measurement harness | ✅ `../results/AlignMeasure.cs` (compiles with .NET Framework C#5) |
| T1 fixed-offset run | ✅ executed end-to-end; artifacts in `../results/2026-10-27/` |
| T2 drift, T3 local, T4 gap, T5 retake, T6 2-hour, T7 determinism, T8 stereo | ⛔ pending — procedures locked in BEHAVIOR_TESTS §4.2 |
| Preset-comparison runs | ⛔ pending — protocol in BEHAVIOR_TESTS §4.2 |
| MiniMax implementation | ⛔ explicitly out of scope for this phase |

## 2. Environment facts (verified)

- Product: Revoice Pro **5.1.32**, `C:\Program Files\Synchro Arts\Revoice Pro\RevoicePro.exe`,
  SHA-256 `2D88939863E4C00979B660692E5303237B9CB25707EE634BFD8DE5544D42120D`.
- Launch: works. The 2026-10-26 blocker (missing `R2RTRUST.dll`) is resolved — vendor
  DLLs now in `C:\Windows\System32`; no bypass was used (PROVENANCE §2.5).
- Fixtures: `testdata/*.wav` (7 files) + `testdata/example_session/` (shipped example
  session wired to fixture file names). Two-hour pair present
  (`long_guide.wav`, `long_dub.wav`, 1.38 GB each).
- Screen: 1440×900 logical; **cursor coordinates = capture-pixel/2** (verify per session
  with the hover-highlight test before batches).

## 3. Verified UI automation runbook (reference)

Product ignores injected keystrokes; mouse events and window messages work.
Toolkit scripts live in the agent's temp workspace (`ClickAny.ps1`, `Pipe.ps1`,
`MenuOp/MenuMaster.ps1`, `DlgClick.ps1`, `HoverDrag.ps1`, `PresetChain.ps1`,
`Snap2.ps1`, `MoveDlg.ps1`) — behavior summary:

1. **Launch**: `Start-Process RevoicePro.exe -ArgumentList '"<session.rvp>"'`; wait ~20 s;
   close the "Quick Start" `#32770` dialog with `PostMessage(WM_CLOSE)`.
2. **Find windows**: `EnumWindows` on the process; main = title `Revoice Pro - …`;
   dialogs = class `#32770` with their titles; never use `FindWindow` by title (flaky).
3. **Open a top menu**: `PostMessage(frame, WM_SYSCOMMAND, SC_KEYMENU, mnemonicChar)`
   (`F`/`E`/`T`/`W`/`H`). Menus ignore keyboard otherwise. If it fails: no dialog may be
   open (dialogs suppress menu commands), no stale popup may exist, and the frame may
   need 1 s after role commands. **Never destroy popups with WM_CLOSE** (wedges menu
   tracking) — dismiss with `WM_KEYDOWN/UP VK_ESCAPE` posted to the popup, or a canvas
   click.
4. **Read a menu**: full-screen `CopyFromScreen` capture; menu top ≈ y86, row pitch
   ≈44 px; label x ≈ 180+.
5. **Click any PNG-space point (x,y)**: `SetCursorPos(x/2, y/2)`, 300–450 ms settle,
   `mouse_event` left down/250 ms/up. **Verify with `WindowFromPoint` first** whenever
   correctness matters (dialogs are modeless; the main window can sit above them).
6. **Dialogs**: enumerate `EnumChildWindows` *fresh before each use* (the New Process
   dialog rebuilds its child controls when roles change — stale handles silently fail).
   Standard controls: `SendMessage(BM_CLICK)`, `WM_SETTEXT/WM_GETTEXT` (Unicode
   overloads with explicit `EntryPoint="SendMessageW"`). Custom `AfxWnd140su` controls:
   coordinate clicks only.
7. **Create-process chain** (BEHAVIOR_TESTS §4.0): roles via Tracks menu items
   (Guide = row y698, Dubs = y742 in PNG space) → range drag starting on the hint zone
   (right side of the under-ruler strip; left side is a zoom gesture) → New Process
   dialog: Presets bar (blue, PNG ~(709,467)) ▸ Factory ▸ item (row y = 519 + 47·index)
   → **Add** (first `96x28` AfxWnd) after each role command → **Create New Process**
   (class `Button`; `SendMessage(BM_CLICK)`; if it seems ignored, verify all fields and
   that no field regressed after child rebuilds).
8. **Export** (BEHAVIOR_TESTS §4.0-7): File menu ▸ Export row **y550** (not 506) →
   dialog: `Enable All` button → `WM_SETTEXT` full path into the filename `Edit`
   (children: `ComboBoxEx32`→`ComboBox`→`Edit 225x12`) → `BM_CLICK` **`&Save`** →
   files appear as `<base> <track>.wav`.
9. **Perf/log evidence**: keep one screenshot per state transition; hash every export.

## 4. Honest gaps / next execution order

1. **Fixed-span export** (BEHAVIOR_TESTS §0.1-1): click **Export Selected Playback Range**
   *manually* once to confirm it exports the blue range; if it works, automate it
   (coordinate click may be needed — the button ignores `BM_CLICK`), re-run T1b, and only
   then quote absolute alignment-accuracy numbers.
2. **T6/T8 two-hour stereo concert run** (priority per engagement): import
   `long_guide/long_dub` into a fresh session copy, same chain, capture wall-clock +
   peak RSS (`Get-Process`), export, chunked Stage-A drift measurement. Not started —
   session fixtures for it are in `testdata/`.
3. T2 → T3 → T5 → T4 → T7 in order; preset comparison (3 presets × T1b).
4. Update the result table in BEHAVIOR_TESTS §4/§5 with each real row; never leave a
   placeholder that reads like a measurement.

## 5. Compliance notes

- No licensing circumvention at any point; product used as delivered.
- Private RE findings (addresses, internals) must never be copied from
  `../private-research/` into implementation-facing material.
- All fixture audio is procedurally generated (seeds documented).
