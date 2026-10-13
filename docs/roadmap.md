# OpenStage Capture: Build Roadmap

**Status:** Draft v0.1
**Built from:** the [product brief](product-brief.md), the [requirements document](prd.md) (46+ stories), and [what Capture needs from OpenStage](openstage-requests.md)
**Scope:** Release 1, Windows 11 and later

> **How to read this.** The roadmap gives an *order*, not dates, because nobody can honestly date work that depends on testing on a real Windows machine and on changes inside OpenStage. Each phase says what gets built, which requirement stories it covers, where it can be tested, and what has to be true before the next phase starts (the *exit check*). Story numbers (A1, C3, and so on) refer to the requirements document.

## 1. Ground rules

1. **Walking skeleton first.** The first thing built is a thin, working path from "start recording" to "recover a crashed recording," with a fake (synthetic) source of sound and picture. Real capture is then added to a skeleton that already works and is already tested.
2. **Accessibility is built in every phase,** not saved for the end. Every phase's exit check includes the keyboard and screen reader checks for what it built.
3. **Test where it can be tested.** Pure logic (protocol, project format, crash recovery, edit model) is built and tested in the cloud build environment. Anything that touches Windows (screen capture, sound devices, encoders, hotkeys, screen readers) is written there but **tested on the owner's Windows machine** with double-click scripts, and results are recorded.
4. **Nothing is claimed that was not tested** (R9 in the requirements document).
5. **Checkpoints, not promises.** At marked points the owner can change course. The two main ones: if OpenStage's prerequisites prove large, revisit what Release 1 includes; if speech recognition fails its audit, it leaves Release 1.
6. **License and rules checks run on every change** from the first line of code (G4).

## 2. Who does what

| Who | Does |
|---|---|
| **Claude (the Capture author)** | Writes the Capture code, tests, specs, and scripts. Writes and type-checks Windows code but cannot run it. Writes the editor panels once given write access to the OpenStage repository. |
| **The owner** | Runs Windows tests by double-clicking the provided scripts and reports what happened. Decides the open questions. Grants repository access when ready. |
| **The OpenStage session** | Makes the changes listed in [openstage-requests.md](openstage-requests.md) (CLAUDE.md edits, roadmap, shell, player, format). |

## 3. Where the code lives

- **Now:** in this repository under `capture/`, as a Cargo workspace (Rust) with these crates: `openstage-capture-protocol`, `openstage-capture-project`, `openstage-capture` (the engine), and `openstage-capture-cli` (the `openstage-capture` program).
- **Later:** when the owner agrees, the `capture/` folder moves into the OpenStage repository, and the editor panels (TypeScript) are added to `editor/src/`.

**Architecture decision (made on the owner's delegation; the owner can veto).** *Edit operations* (trim, split, join, reorder, undo) live in **TypeScript** in the editor, like the existing Timeline model, and are saved as an edit decision list in the capture project. The Rust side **reads and validates** that list and **renders** it. This avoids sending every keystroke of an edit across a process boundary and matches OpenStage's rule that editor logic is TypeScript.

## 4. Phases

### Phase 0: Foundations
**Goal.** Rules, specs, and checks exist before real code does.

| Work | Stories |
|---|---|
| Repository layout and Cargo workspace | all |
| Rule checker: license allowlist, FFmpeg build-flag guard, license headers, no bundled FFmpeg | G4 |
| Protocol spec v0.1 (one page) | G1 |
| Project format spec v0.1 | B1 |
| Requests to OpenStage written | (this roadmap) |
| Draft accessibility statement | H1 |
| *OpenStage side:* CLAUDE.md edits (OS-1), roadmap (OS-2), NOTICE (OS-3) | R7 |
| *Owner and OpenStage:* baseline screen reader pass (OS-17), real LMS tests (OS-16) start | prerequisites |

**Exit check.** The rule checker passes on every change. Both specs exist and match the code.

### Phase 1: Walking skeleton (built first)
**Goal.** A program that starts, "records" a synthetic source in crash-safe pieces, takes commands, speaks the protocol, survives being killed, and recovers.

| Work | Stories |
|---|---|
| Protocol crate: events, commands, one JSON object per line, version line, size limits | G1 |
| Project crate: capture folder, manifest, takes, crash-safe segments, append-only event log | B1, A8 |
| Engine: session state machine (countdown, record, pause, resume, stop, restart, marker), levels with text equivalents, safe finish on write errors | A3, A4, A8, A10, G1 |
| Start again: set a take aside, never erase it | A10 |
| `recover`: salvage a killed recording, losing at most the flush interval | A8 |
| CLI: `record`, `probe`, `recover` (`render` reports "not yet available") | G1 |
| Windows scripts: build and test by double-click | tests on Windows |

**Where tested.** Cloud build environment (automated, including a real kill-and-recover test), then the owner runs the same tests on Windows.

**Exit check (checkpoint CP-1).** All tests pass here and on the owner's Windows machine, including killing the program mid-recording and recovering with at most the flush interval lost (the requirement allows 10 seconds).

### Phase 2: Real capture on Windows
**Goal.** Replace the synthetic source with the real thing.

| Work | Stories |
|---|---|
| Screen, window, and region capture (Windows Graphics Capture), with numeric region entry | A1 |
| Microphone, system audio (including screen reader speech), and webcam | A2 |
| Keep the recorder and notification pop-ups out of the picture | A5, A7 |
| Interaction log: cursor, clicks, keys, active window, and clicked element names (UI Automation), opt-in, never password text | A6 |
| Exclude chosen windows | A7 |
| Setup checks: permissions, Windows 11 and later, media components for MP4 | A9 |
| Audio cues for every state, made by the capture program itself | A4 |

**Where tested.** The owner's Windows machine with provided scripts; results recorded.

**Exit check (checkpoint CP-2).** A 60-minute test recording shows audio and video drift under 40 ms and under about 10% CPU at 1080p30 on a mid-range laptop; a killed recording recovers; the recorder never appears in the picture; interaction capture is off unless turned on.

### Phase 3: Render and export
**Goal.** Turn a capture project and its edit decisions into a finished MP4.

| Work | Stories |
|---|---|
| Edit decision list: schema, validation, and a rendering plan | B3, C-series (render side) |
| Frame composition: cursor effects, click effects, zoom and pan, text and shape overlays, blur and redact | C5 to C8 |
| "Reduce motion" export (cuts instead of animated zooms) | C5 |
| MP4 (H.264 and AAC) through Windows media encoders; atomic export; cancel | F1 |
| Render once, store as an asset; republishing is byte-identical | B3 |

**Where tested.** Composition logic here (golden frames); encoding on Windows.

**Exit check.** An exported MP4 plays in current browsers; cancelling leaves no partial file; the same project exports identically twice.

### Phase 4: The editing model
**Goal.** The edit logic, without a screen.

| Work | Stories |
|---|---|
| Track and clip model, edit operations (trim, split, delete, join, reorder; gap or ripple delete), undo and redo | C1, C2, C11, B4 |
| Timeline arithmetic extending OpenStage's `timelinemodel.ts` (same keyboard steps, same plain-words announcements) | C3 |
| Imported media rules (image, audio, short clips of 30 seconds or less) | C10 |
| Described-preview text for any playhead position | C4 |

**Where built.** TypeScript with Node's test runner, in this repository first.

**Exit check.** Every operation is covered by tests; every operation has a plain-words result string; nothing is mouse-only.

### Phase 5: OpenStage integration and the editor panels
**Needs.** Write access to the OpenStage repository (a branch with a pull request), and OS-4 to OS-7.

| Work | Stories |
|---|---|
| Streaming launch path, global hotkeys, program lookup, video preview | G2, G3, A3 |
| Recorder panel in plain HTML with live state | A3, A4, A10 |
| Clip list (table) and the multi-track timeline | C1, C3 |
| Described preview, text overlays, shapes, zoom and cursor controls | C4 to C9 |
| Insert into a course; the problem list shows validator findings | F2 |
| Accessibility checker pane | F4 |

**Exit check (checkpoint CP-3).** Record, trim, add an overlay, export, and insert into a course using only the keyboard, then again with a screen reader (NVDA and Narrator). The timeline does not ship until its keyboard model is complete and screen-reader-verified.

### Phase 6: Accessible content
| Work | Stories |
|---|---|
| Captions from the narration script, caption track editing, WebVTT and SRT import and export, readability warnings | D1, D2 |
| Transcripts and step text; alt text pre-filled from clicked elements, marked unreviewed | D4, D6 |
| Audio for text the learner reads | D7 |
| Audio description authoring (player side: OS-9) | D5 |
| Speech recognition: audit Whisper's license and quality; plug in through the "captions from media" contract | D3 |

**Fallback rule.** If no speech recognition tool passes the audit, D3 leaves Release 1. The rest stays.

### Phase 7: Steps and interactive training
**Needs.** OS-10 (text entry), OS-11 (scoring), OS-12, OS-13, OS-14.

| Work | Stories |
|---|---|
| Step segmentation from the interaction log; step slides with required fields | E1 |
| Demo mode (watch) | E2 |
| Practice mode (try it), keyboard-operable hotspots, typing steps | E3 |
| Assessment mode (test me), scoring through SCORM 1.2 | E4 |

**Checkpoint CP-4.** If more than one OpenStage prerequisite proves large, the owner can ship Demo and click-only Practice first.

### Phase 8: Hardening and release
| Work | Stories |
|---|---|
| Release gate: all OpenStage gate items plus Capture items, in Brightspace, Moodle, and Canvas | F3, H2 |
| Full manual keyboard and screen reader passes; automated accessibility checks | H2 |
| Performance, A/V drift, and recovery tests on real hardware | H2 |
| Final accessibility statement; plain-language help and shortcut list | H1, H3 |
| Final license, patent, and trademark checks | R7 |

## 5. The parallel track: what OpenStage must do

These run alongside Phases 2 to 7 (see [openstage-requests.md](openstage-requests.md)).

| Request | Unblocks | Phase needed by |
|---|---|---|
| OS-1 to OS-3: rules, roadmap, notice | everything | Phase 0 |
| OS-8: responsive output (longest) | Release 1 accessibility claim | Phase 8 (start early) |
| OS-4 to OS-7: shell | recorder panel | Phase 5 |
| OS-9: audio description in the player | D5 | Phase 6 |
| OS-10 to OS-14: text entry, scoring, attempts, hotspot, provenance | Practice and Assessment | Phase 7 |
| OS-15: keep capture material out of `source/` | B2 safety net | Phase 8 |
| OS-16, OS-17: real LMS, screen reader baseline | trust in the foundation | Phase 0 to 5 |

## 6. Coverage check

Every requirement story appears below, so none is forgotten.

| Phase | Stories |
|---|---|
| 0 | G4, H1 (draft) |
| 1 | A8, A10 (protocol side), B1, G1, G3 (program side) |
| 2 | A1, A2, A4 (cues), A5, A6, A7, A9 |
| 3 | B3, C5 to C8 (render side), F1 |
| 4 | B4, C1, C2, C4 (text), C9, C10, C11 |
| 5 | A3, A4, A10 (panels), C1, C3, C4 to C9 (controls), F2, F4, G2, G3 (shell side) |
| 6 | D1 to D7 |
| 7 | E1 to E4 |
| 8 | B2 (test), F3, H1 (final), H2, H3 |

## 7. The first build

Phases 0 and 1 are built first. They need nothing from the OpenStage repository, can be fully tested in the cloud build environment, and settle the shape of everything after them. Phase 2 comes next; it needs the owner's Windows machine for testing.

## 8. Progress

Updated as work lands. "Tested here" means the automated tests pass in the cloud build environment (Linux). Nothing is called working on Windows until the owner has run the Windows scripts and reported the result.

| Phase | State |
|---|---|
| 0 Foundations | Mostly built. Done: workspace, rule checker, [protocol spec](capture-protocol-spec.md), [project format](capture-project-format.md), [requests to OpenStage](openstage-requests.md), automatic-checks file. **Not done:** draft accessibility statement; the OpenStage-side items (OS-1 to OS-3, OS-16, OS-17). |
| 1 Walking skeleton | Built and tested. **Checkpoint CP-1 passed on 2026-10-13:** the owner ran `TEST CAPTURE.bat` on Windows and all 65 tests passed there, including the real kill-and-recover test, and a short test recording ran and finished cleanly. (The Windows version was not recorded in the log; see the note below.) A later fix and its test bring the total to 66; they have passed here but have not yet been re-run on Windows. |
| 2 to 8 | Not started. |

Two real bugs were found in Phase 1 and fixed, each with a regression test that fails without the fix:

1. A time subtraction could overflow when handling a command took longer than the recorder's own step (found by the integration tests in the cloud).
2. After a recording stopped, the `stopped` state named the *next* take (take 2) instead of the one just finished (take 1). It was spotted by reading the owner's first Windows log, and would have made a screen reader announce the wrong take number.

**Note on Windows version.** The log records that the platform is Windows but not whether it is Windows 11. Release 1 targets Windows 11 and later. `TEST CAPTURE.bat` now saves the Windows version at the top of `test-log.txt` (this change has not yet been run on Windows).
