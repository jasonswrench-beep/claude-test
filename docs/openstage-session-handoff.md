# Tasks for the OpenStage session: OS-1, OS-2, OS-3, OS-16, OS-17

**Written:** 2026-10-13, against the OpenStage Author repository at commit `23d53e5` (line numbers below are from that commit; search for the quoted text if they have moved).
**For:** a Claude Code session working in the OpenStage Author repository. It has the repository. It does **not** have the conversation that produced this file, so everything it needs is here.
**From:** the owner (Jason) and the author of OpenStage Capture.
**Parent document:** [openstage-requests.md](openstage-requests.md) lists all 17 things Capture needs from OpenStage. This file takes the five that are documents and testing, not code, and spells each one out step by step. The other twelve (OS-4 to OS-15) are not asked for here and should not be started from this file.

## Read this first

**What Capture is.** OpenStage Capture is a separate Windows-first module: a recording program (`openstage-capture`, written in Rust), a mid-level editor for recordings (trim, split, text overlays, zoom, like Camtasia), and accessible step-by-step and software-simulation output. It will become part of OpenStage Author. Its code and plans live today in the repository `jasonswrench-beep/claude-test` (code in `capture/`, plans in `docs/`). It does not change how OpenStage works until the owner agrees to each step.

**What is asked here.** Three small document edits (OS-1, OS-2, OS-3) and two verification jobs (OS-16, OS-17). Nothing here changes the player, the format, the validator, or the editor code. If you find that a task seems to need such a change, stop and ask the owner.

**Rules you must follow while doing this** (they come from this repository's own `CLAUDE.md`):
- No em dashes in prose written for the project. Use a comma, a colon, parentheses, or two sentences. (This also applies to anything you add to `NOTICE`, `ROADMAP.md`, `README.md`, and the new files below.)
- Explain what changed in plain language. The owner is not a programmer.
- Anything the owner needs to run must be a double-clickable `.bat` script, not a command to copy into a terminal.
- When a decision could reasonably go either way, say so and ask. Four such decisions are listed in the last section.
- Say so explicitly if a change could affect keyboard access, focus order, or screen reader output. (None of the document edits do.)
- Do not rename `OPENSTAGE_*`, `.osproj`, `openstage-editor.exe`, or the `os-` CSS prefix.
- Do not claim anything is tested that has not been. You cannot run a real LMS or a real screen reader yourself. For OS-16 and OS-17 your job is to prepare, guide the owner, record results exactly, and act on them. Never write "passed" for something nobody ran.

**Suggested order.** OS-1, OS-2, OS-3 in one sitting (a few minutes each). Then prepare OS-16 and OS-17 and hand them to the owner. Make one commit per request so each can be reviewed alone.

| ID | What | Files | Who does it |
|---|---|---|---|
| OS-1 | Three exceptions in the project rules | `CLAUDE.md` | You |
| OS-2 | Roadmap updates | `ROADMAP.md` | You |
| OS-3 | Say in writing that the narration engine has GPL parts the user installs | `NOTICE`, `README.md`, `ROADMAP.md` | You |
| OS-16 | Test the SCORM 1.2 output in real LMSs | new `examples/lms-test-lesson.osproj/LMS-RESULTS.md`, `ROADMAP.md` | You prepare; the owner runs the tests; you record and act on results |
| OS-17 | Screen reader pass on the editor patterns Capture will reuse | new `docs/screen-reader-pass.md`, `ROADMAP.md` | You prepare; the owner or a helper runs it; you record and act on results |

---

## OS-1. Record three exceptions in CLAUDE.md

**Status of the decision.** The owner has agreed to all three exceptions (asked and answered "Yes" in the Capture planning conversation, before 2026-10-13). `CLAUDE.md` says anything beyond the editor's first slice is agreed with the user first; this is that agreement. If you want confirmation before editing the rules file, ask the owner one line: "Capture needs three exceptions in CLAUDE.md. Confirm?"

**Why.** Without these edits the project's own rules forbid the Capture work: it is a video editor, it is Windows first, and it is written in Rust. A later session could rightly refuse to follow the Capture plans.

**Edit 1.** In `CLAUDE.md`, section "What this is" (line 17).

Find exactly:
```
Not a template tool. Not an LMS. Not a cloud platform. Not a video editor.
```
Replace with exactly:
```
Not a template tool. Not an LMS. Not a cloud platform. Not a full or professional video editor.
The Capture module includes a mid-level editor for recordings: trimming, splitting, text overlays,
and zoom, in the manner of Camtasia.
```

**Edit 2 and 3.** In `CLAUDE.md`, section "Architecture" (lines 67 and 68, one bullet wrapped over two lines).

Find exactly:
```
- TypeScript throughout. Electron or Tauri shell. Cross-platform from the start:
  Windows, macOS and Linux all matter.
```
Replace with exactly:
```
- TypeScript throughout, except the Capture program, which is Rust because screen capture
  needs native code. Electron or Tauri shell. Cross-platform from the start, except the
  Capture module, which is Windows first.
```

**Do not** change anything else in `CLAUDE.md`. In particular do not touch the "Out of scope" list or the release gate. Leave the existing em dash in the "Out of scope" heading alone; it is not part of this task.

**Done when.** The two blocks above are changed, nothing else in the file differs (check with `git diff`), and the change is committed. Suggested message: "Allow the Capture module: mid-level video editor, Windows first, Rust program".

**Tell the owner** (plain language): the rules file now allows a mid-level recording editor, a Windows-first module, and one Rust program; nothing else in the rules changed.

---

## OS-2. Update the roadmap

**Why.** `ROADMAP.md` is where OpenStage records what it will build and in what order. Several Capture features depend on items that are in "Later" or missing, and "Later" lists screen recording as a distant idea, which is no longer true.

Make these four edits to `ROADMAP.md`.

**Edit 1. Move "Responsive output" out of "Later" into an active phase.** The owner decided: fix responsive output first, before Capture ships. It is a published-output accessibility gap (WCAG 2.2 1.4.10, Reflow), so **this file recommends Phase 1** ("Format and player"). That is a judgment call; see "Decisions to ask the owner" at the end. Add this line to Phase 1, directly after the line `- [ ] Tested in Moodle, Canvas and Brightspace, including resume` (line 14):
```
- [ ] Responsive output: the published course reflows at 320 CSS pixels wide and at 400 percent zoom without scrolling in two directions (WCAG 2.2, 1.4.10 Reflow). Today the player scales the whole stage to fit and never reflows, so this is a known failure. The owner decided it is fixed before the Capture module ships.
```

**Edit 2. Replace the "Later" paragraph.** Find exactly (lines 55 to 57):
```
SCORM 2004 and xAPI. Whisper for imported media. Screen recording. Software simulation
authoring. Step capture. Localisation. QTI and LTI. Responsive output. A CC0 character
and icon library. Reusable components. Bring-your-own-key AI assistance.
```
Replace with exactly:
```
SCORM 2004 and xAPI. Whisper for imported media (the Capture module will audit it first
for speech recognition). Localisation. QTI and LTI. A CC0 character and icon library.
Reusable components. Bring-your-own-key AI assistance.
```
(Removed: "Screen recording. Software simulation authoring. Step capture." which become Phase 4 below, and "Responsive output." which moves to Phase 1. "Whisper for imported media" stays.)

**Edit 3. Add a new phase before "## Later".** Insert this whole block between the end of "Phase 3" (after the `- [x] Branching` line) and the "## Later" heading. The phase number is a suggestion; use the next free number.
```
## Phase 4: Capture module (Windows first)

Screen recording, a mid-level video editor for recordings (trimming, splitting, text overlays and
zoom), and accessible step-by-step and software-simulation output. The recorder is a separate
program (`openstage-capture`, written in Rust) that the editor starts; the editor panels are
TypeScript. Its own plan is in the repository jasonswrench-beep/claude-test (docs/roadmap.md);
update this link when the Capture code moves into this repository. What it needs from OpenStage,
besides the shell work listed in that repository's docs/openstage-requests.md:

- [ ] Audio description for video in the player: an "Audio description" button beside a video that opens an accessible pop-up where the learner can read the description, listen to it, or play the video with the description voiced in time. Shown only where the author has added description. The text is the source and the audio is generated from it, like narration.
- [ ] A text-entry input, so a learner can type an answer into a labelled field and a rule can check it
- [ ] Scoring from steps: hotspot and text-entry steps can add points to the score, the results box and SCORM, with step results stored compactly enough for SCORM 1.2's 4096-character suspend data
- [ ] Timing that follows a video or an audio clip. A request only: Capture works without it, but with it Capture's step text could be real, accessible objects timed to the video instead of being drawn into the picture.
```

**Edit 4. Keep "Whisper for imported media".** Already done by Edit 2. Do not remove it.

**Do not** tick any box, and do not remove any existing item.

**Done when.** `ROADMAP.md` shows all four edits, `git diff` shows only those changes, and the change is committed. Suggested message: "Roadmap: responsive output now active; add the Capture module phase".

---

## OS-3. Write down that the narration engine has GPL parts the user installs

**Why.** `NOTICE` lists Kokoro-82M (Apache-2.0). The speech engine that makes the audio is installed separately by the user (see `docs/editor.md`, "The audio is made on this computer by the speech engine (Kokoro), never in the editor itself", and the `OPENSTAGE_KOKORO_PYTHON` setting). That engine can include components under the GNU General Public License, such as `phonemizer-fork` and `espeak-ng`. Nothing in this repository mentions them. Today that is fine because OpenStage never ships them. Writing the arrangement down keeps it fine and stops a later installer from bundling them by accident. The owner chose this arrangement: the engine stays user-installed.

**Before you write it,** confirm from the code that it is true: that OpenStage runs the engine as a separate program (a separate Python process), and that no GPL component is copied into this repository or into the files the installer would ship. If either is not true, do not add the text; tell the owner what you found.

**Edit 1. `NOTICE`.** After the Kokoro-82M entry (after line 22, `  Apache License 2.0`) and before "Any further bundled model or library..." add exactly:
```

Speech engine (installed separately by the user; not included, bundled or hosted by OpenStage Author)
  Spoken narration is made by a speech engine that the user installs on their own computer
  and that OpenStage Author runs as a separate program. That engine may include components
  under the GNU General Public License, such as phonemizer-fork and espeak-ng. OpenStage
  Author does not include, bundle, or host them. They keep their own licences.
```

**Edit 2. `README.md`.** In the "Licence" section, directly after the sentence ending "is Apache 2.0." (line 61), add a new paragraph:
```

Spoken narration also needs a speech engine that you install separately. Parts of that
engine may be under the GNU General Public License (for example phonemizer-fork and
espeak-ng). OpenStage Author does not include, bundle, or host them.
```
(The README has no section called "narration"; the Licence section is where Kokoro is already mentioned, so that is where this goes.)

**Edit 3. `ROADMAP.md`.** Find the installer item (line 44):
```
- [ ] Installer and first-run model download (nothing is packaged yet; the editor is built from source)
```
Replace with:
```
- [ ] Installer and first-run model download (nothing is packaged yet; the editor is built from source). The installer must not bundle GPL components such as espeak-ng or phonemizer-fork; see NOTICE.
```

**Not a legal opinion.** This text states facts about how the software is arranged. It is not legal advice. The owner has a final legal check on their list before release.

**Done when.** `NOTICE`, `README.md`, and `ROADMAP.md` say it, `git diff` shows only those changes, and the change is committed. Suggested message: "Document that the narration engine's GPL parts are user-installed".

---

## OS-16. Test the SCORM 1.2 output in real LMSs

**Why.** `ROADMAP.md` line 14 (`- [ ] Tested in Moodle, Canvas and Brightspace, including resume`) is unchecked, and the first four items in the release gate in `CLAUDE.md` depend on it. Today SCORM 1.2 has been checked only against a stand-in LMS. Capture's most important test (a learner leaves and returns to the exact place) builds on this, so any problem should be found before recordings are added.

**What already exists.** `examples/lms-test-lesson.osproj/` is a 24-slide test lesson with a full written test plan, `LMS-TEST-PLAN.md`, containing 12 numbered tests, an answer key, and "What to send back". `PUBLISH LMS TEST LESSON.bat` rebuilds the package as `dist\lms-test-lesson-scorm12.zip`. The plan says most of it "has now been tried once in a real Brightspace", but **no results are recorded anywhere in the repository.** Ask the owner for any notes or screenshots from that first run.

**Who can do what.** The owner has Brightspace and Moodle now. Canvas access is coming; the owner will say when. You cannot run an LMS. You prepare the results file, guide the owner one test at a time in plain language, record what they report exactly, and fix real faults.

**Steps.**

1. Ask the owner to double-click `PUBLISH LMS TEST LESSON.bat` and confirm that `dist\lms-test-lesson-scorm12.zip` was made. (It needs Node.js and Python; the script says what to install if they are missing. Narration is made with Kokoro if found; the lesson still publishes without audio.)
2. Create the results file `examples/lms-test-lesson.osproj/LMS-RESULTS.md` using the template at the end of this section. Do not fill in any result yourself.
3. For **each LMS** (Brightspace, Moodle, Canvas), the owner uploads the zip as a SCORM 1.2 package and uses a **test student, not instructor preview** (most LMSs record nothing in preview). Do not guess menu names for Moodle or Canvas: ask the owner for a screenshot if they cannot find the upload option, and write down what worked.
4. **Required tests** for each LMS, from `LMS-TEST-PLAN.md`. They match the four SCORM items in the release gate:
   - **Test 1**, launch and look: opens with no error.
   - **Test 2**, a failing attempt: expect failed, score 60.
   - **Test 3**, a passing attempt at the edge (80 percent exactly, after a retry on question 1): expect passed, score 80.
   - **Test 4**, resume: close the lesson on question 3, reopen. Expect it opens on question 3, answers 1 and 2 are kept and locked, the survey answers and extra notes on slide 9 are kept, and finishing gives passed, score 100.
5. **Also worth doing** if the owner has time: Test 6 (keyboard only, and whether Tab can leave the LMS frame and return), Test 7 (NVDA, if available), Test 8 (the embedded YouTube video), Test 10 (downloads), and Tests 11 and 12 (timing and animation). Tests 5 and 9 are lower priority.
6. For every test, record **exactly what the LMS shows**: the words it uses for status, the score, and anything that looked wrong, with a screenshot if possible. Record failures as results; they are the point.
7. If a test shows a real fault in OpenStage, fix it following the project rules (spec in the same commit, a regression test that fails without the fix), and re-run that test.
8. **Roadmap.** Change line 14 only as results come in. While any LMS is untested or failing, write it as `[~]` with a short note naming each LMS and its state, for example `[~] Tested in Moodle, Canvas and Brightspace, including resume (Brightspace: passed tests 1 to 4; Moodle: not yet; Canvas: not yet)`. Tick `[x]` only when all three have passed tests 1 to 4. Never tick it on the strength of the stand-in LMS.

**One thing to raise with the owner.** The release gate says "Course completion and assessment success are tracked as separate states". `LMS-TEST-PLAN.md` itself notes that SCORM 1.2 has only one status, so a learner who finishes but fails is reported as failed, never as completed. Those two statements pull against each other. Do not decide it alone: ask the owner how the gate should read for SCORM 1.2 (for example, "completion and success are both correct as far as SCORM 1.2 can express them, and are fully separate in SCORM 2004 later"), and change the gate wording only with their approval. This matters to Capture because its first release publishes through SCORM 1.2 only.

**Template for `LMS-RESULTS.md`** (create it with every result cell empty):
```
# Real LMS results for the test lesson

Package tested: dist\lms-test-lesson-scorm12.zip, built from commit ______ on ______.
Plan: LMS-TEST-PLAN.md. Use a test student, not instructor preview.

## Brightspace
LMS version: ______   Date: ______   Tester: ______

| Test | Result (passed / failed / not run) | What the LMS showed (status words, score) | Problems, screenshots |
|---|---|---|---|
| 1 Launch and look | | | |
| 2 Failing attempt (expect failed, 60) | | | |
| 3 Passing at the edge (expect passed, 80) | | | |
| 4 Resume (expect Question 3, answers kept, passed 100) | | | |
| 6 Keyboard only, including leaving the frame | | | |
| 7 Screen reader (NVDA) | | | |
| 8 Embedded video | | | |
| 10 Downloads | | | |
| 11 and 12 Timing and animation | | | |

## Moodle
(same table)

## Canvas
(same table)

## Summary
Which LMSs have passed tests 1 to 4: ______
Faults found and where they were fixed: ______
```

**Done when.** `LMS-RESULTS.md` has real results for each LMS, tests 1 to 4 have passed in all three (or the failures are listed and being fixed), and `ROADMAP.md` line 14 reflects exactly that.

---

## OS-17. A baseline screen reader pass on the editor

**Why.** Capture's recorder panel, clip list, and multi-track timeline are built by reusing OpenStage's editor patterns, above all the **Timeline panel** with its slider grips. `ROADMAP.md` line 43 (`- [ ] ATAG Part A pass on everything above, including NVDA`) is unchecked and `docs/editor.md` says "Screen reader testing with NVDA is still to be done." Building new panels on untested patterns risks rebuilding them later. The release gate also requires "the entire editor operable by keyboard alone" and "usable with NVDA and VoiceOver". This pass covers Windows (NVDA and Narrator) only; VoiceOver is out of scope until there is a Mac build.

**Who can do what.** You cannot run a screen reader. You write the test script and results file, the owner (or someone who uses a screen reader every day, which will find far more than a first-time tester; ask the owner who) runs it, and you record the findings and fix or list the problems. Software: NVDA (free, from nvaccess.org) and Narrator (built into Windows; Windows key + Ctrl + Enter turns it on). Run the editor with `OPEN EDITOR.bat`; on Windows the editor is a WebView2 (Chromium) window.

**Steps.**

1. Create `docs/screen-reader-pass.md` containing, for each pattern below: what to do, what a good result sounds like, and an empty results line. Write it in plain language with numbered steps, no jargon.
2. Hand it to the owner or helper. Ask them to run each pattern with NVDA first, then Narrator, and to record **exactly what the screen reader said**, not a summary.
3. Record the results in the same file. For each problem write: the pattern, the screen reader and version, what was said, what should have been said, and how bad it is (blocks a task / awkward / cosmetic).
4. Fix what you can under the project rules, with tests, and list the rest. Keep the roadmap item at `[~]` with a note ("NVDA and Narrator pass done for: ...; open problems: ...") until the whole editor has been covered. Do not tick it on the strength of this baseline alone.

**The patterns to cover, and what good looks like.** The facts below come from `docs/editor.md` and the code; check them against the current code before you write the script.

| # | Pattern | Where it lives | What to do | A good result |
|---|---|---|---|---|
| 1 | **Panels and region cycling** | whole editor | Press F6 and Shift+F6 repeatedly | Each panel is named when focus enters it; the Timeline panel is included while it is showing; focus is never lost |
| 2 | **The Slides, Objects and Problems lists** | `app.ts` | Up and Down, Home and End, Enter or Space; Alt+Up and Alt+Down to move a slide or change reading order; Delete | Each item's name, position and selected state is spoken; reordering is announced; delete says it can be undone |
| 3 | **The canvas by keyboard** | `canvas.ts` | Page Down and Page Up to choose an object; arrow keys to move 1 pixel, Shift+arrows 10; Alt+arrows to resize; Escape | Selection, moves and resizes are announced in the polite live region; the slide itself is `inert` so the reader is not walked through it |
| 4 | **The Properties panel** | `app.ts` | Tab through the fields for a text object, a shape, an image; change values | Every field has a spoken label; changing a value keeps focus and cursor in place; unavailable controls (`aria-disabled`) can still be reached and say they are unavailable |
| 5 | **Dialogs** | `newproject.ts`, `layoutpicker.ts`, `themepicker.ts`, `choicepicker.ts`, `textprompt.ts`, `narrationdialog.ts`, `publishdialog.ts`, `assetsdialog.ts` (native `<dialog>`), and the Timing and Variables dialogs | Open each; Tab around; press Escape; for Narration and Publish, start and cancel a job | Focus moves in; Tab stays inside; Escape closes; focus returns to the button that opened it; progress and cancel are announced; no keyboard trap |
| 6 | **The live region** | `app.ts` (`role="status"`, `aria-live="polite"`) | Move, resize, undo, redo, delete, save; run Narration | Each announcement is heard once, in plain words, and does not interrupt typing |
| 7 | **The Timing list** | `timingdialog.ts` | Open it from the Timing box; change a time with the keyboard | It reads as a table or list with named fields; each change is announced |
| 8 | **The Timeline panel** (most important) | `timelinepanel.ts`, `timelinemodel.ts` | Press the Timeline button. Move through the rows. On a grip, press Left and Right (a tenth of a second), Shift+arrow (a second), Page Up and Page Down (five seconds), Home and End; on the second grip, End makes the object stay | Each row is a group named with the object and its timing in words; each grip is announced as a slider named "<name>: when it appears" or "when it goes" with a value in words (for example "0.3 seconds after the slide opens") that updates after every key; the Properties panel follows the change; nothing is spoken twice |

For pattern 8, record specifically: whether NVDA needs focus mode to operate the grips and whether it switches by itself; whether Narrator reads the value text (`aria-valuetext`) or only the number; and whether the row's group label is read when focus moves between rows. These are the answers Capture needs before it builds a multi-track timeline.

**Do not** test the published player here (that is Test 7 of the LMS plan in OS-16), the Mac, or touch devices.

**Done when.** `docs/screen-reader-pass.md` holds NVDA and Narrator results for all eight patterns, every problem is fixed or listed with its severity, and `ROADMAP.md` line 43 carries a note saying exactly what was covered.

---

## Report back

When you finish, give the owner a short plain-language summary and, if the Capture author asks, send the same to the Capture repository:

- For each of OS-1, OS-2, OS-3: done or not, with the commit.
- For OS-16: per LMS, which tests were run and what happened; faults found; the current roadmap line.
- For OS-17: per pattern and screen reader, what worked; problems and their severity; anything Capture should do differently in its recorder panel or timeline because of what you found.
- Anything you did not do and why.

## Decisions to ask the owner (do not pick silently)

1. **Where does "Responsive output" go in the roadmap?** This file recommends Phase 1 (it is a published-output accessibility gap). Another phase may fit better. If unsure, ask.
2. **How should the release gate read for SCORM 1.2?** "Completion and success are separate states" cannot be fully true in SCORM 1.2, which has one status. See OS-16.
3. **Who runs the screen reader pass (OS-17)?** An experienced daily screen reader user will find far more than a first-time tester.
4. **Are there notes or screenshots from the first Brightspace run?** They should be added to `LMS-RESULTS.md` rather than lost.

## What this file does not ask for

- No change to the player, the format, the validator, `schemaVersion`, or the editor code.
- No work on OS-4 to OS-15 (the shell, responsive output, audio description, text entry, scoring, attempts, hotspot, provenance, publisher exclusions). Those need the owner's go-ahead, one at a time, and are described in [openstage-requests.md](openstage-requests.md).
- No bundling of any speech engine component, and no legal conclusions.
