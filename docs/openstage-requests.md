# What OpenStage Capture needs from OpenStage Author

**Status:** Draft v0.1, for the owner and the OpenStage session
**Source:** the [product brief](product-brief.md) (v0.17) and the [requirements document](prd.md) (v0.5), checked against the OpenStage Author repository (commit 23d53e5)
**Purpose:** OpenStage Capture is a separate program, but a few things can only be changed inside OpenStage Author itself. This file lists every one of them: what is needed, why, where it lives, and how to tell it is done. Nothing here should be built without the owner's agreement, as CLAUDE.md requires for anything beyond the editor's first slice.

> **Step-by-step version of five of these.** OS-1, OS-2, OS-3, OS-16, and OS-17 are written out in full, with exact text to change, in [openstage-session-handoff.md](openstage-session-handoff.md). Give that file to the OpenStage session.

> **How to read this file.** Each request has an ID (OS-1, OS-2, and so on) so conversations can point at it. **Blocks Release 1** means Release 1 cannot ship without it. **Needed** means a Release 1 feature depends on it. **Recommended** means it is a safety improvement. "Size" is a rough guess by the author of this file, who has read the code but has not built any of it. Treat it as a hint, not an estimate.

## Summary

| ID | Request | Kind | Size |
|---|---|---|---|
| OS-1 | Record three exceptions in CLAUDE.md | Rules | S |
| OS-2 | Update the roadmap | Rules | S |
| OS-3 | Document the narration engine's GPL parts in `NOTICE` | Rules | S |
| OS-4 | A streaming launch path in the Rust shell | Shell | M |
| OS-5 | Global hotkeys (a capability in the Tauri shell) | Shell | S |
| OS-6 | Previewing large local video in the editor | Shell | M |
| OS-7 | An `OPENSTAGE_CAPTURE` setting | Shell | S |
| OS-8 | Responsive output (reflow, WCAG 1.4.10) | Player and format | L |
| OS-9 | Audio description in the player | Player and format | M |
| OS-10 | A text-entry input | Player and format | M |
| OS-11 | Scoring from steps | Player and format | M |
| OS-12 | Counting attempts (adding to a variable) | Format | S |
| OS-13 | Define the `hotspot` object | Format | S |
| OS-14 | Capture provenance on generated slides | Format | S |
| OS-15 | Keep capture material out of `source/` | Publisher | S |
| OS-16 | Real LMS verification | Testing | M |
| OS-17 | A baseline screen reader pass | Testing | M |

**Suggested order.** OS-1 to OS-3 first (small, unblock everything). Start OS-8 early because it is the longest. OS-4 to OS-7 before the editor panels are built. OS-9 to OS-14 can follow in any order but before the features that need them (the "Needed by" lines say which). OS-16 and OS-17 should happen as soon as possible, because they test the ground everything else stands on.

## Group 1: Rules and documents

### OS-1. Record three exceptions in CLAUDE.md
**Kind:** Blocks Release 1 (a rule that the work must agree with) | **Size:** S

**What.** Change three lines in `CLAUDE.md` so they allow the Capture module. Suggested wording:
- Replace "Not a video editor." with: *"Not a full or professional video editor. The Capture module includes a mid-level editor for recordings: trimming, splitting, text overlays, and zoom, in the manner of Camtasia."*
- Replace "Cross-platform from the start: Windows, macOS and Linux all matter." with: *"Cross-platform from the start, except the Capture module, which is Windows first."*
- Replace "TypeScript throughout." with: *"TypeScript throughout, except the Capture program, which is Rust because screen capture needs native code."*

**Why.** The owner accepted all three exceptions. Without these edits the project's own rules and the Capture work disagree, and a future session could reasonably refuse to follow the Capture work.

**Done when.** The three lines are changed and committed.

### OS-2. Update the roadmap
**Kind:** Blocks Release 1 | **Size:** S

**What.** In `ROADMAP.md`:
1. Move "Responsive output" out of "Later" into an active phase (see OS-8).
2. Replace "Screen recording. Software simulation authoring. Step capture." in "Later" with a new phase called "Capture module", linking to the Capture roadmap (`docs/roadmap.md`).
3. Add these as roadmap items: audio description in the player (OS-9), a text-entry input (OS-10), scoring from steps (OS-11), and, as a request only, **timing that follows a video or an audio clip** (see below).
4. Keep "Whisper for imported media" and note that the Capture module will audit it first for speech recognition.

**Why.** The roadmap is where OpenStage records what it will build and in what order. Several Capture features depend on items that are currently "Later" or missing.

**Timing that follows a video (a request, not a requirement).** The roadmap says "Not built: timing that follows a video or an audio clip." If OpenStage could time an object to a video's own clock, Capture's callouts and step text could be real, accessible OpenStage objects timed to the video, instead of being baked into the video picture where screen readers cannot see them. Capture works without it, so this is not on the critical path.

**Done when.** The roadmap shows the items above.

### OS-3. Document the narration engine's GPL parts in `NOTICE`
**Kind:** Recommended | **Size:** S

**What.** Add a short paragraph to `NOTICE` (and the README's narration section) saying: spoken narration needs a speech engine that the user installs separately; that engine may include components under the GPL, such as `phonemizer-fork` and espeak-ng; OpenStage does not include, bundle, or host them.

**Why.** The repository lists Kokoro-82M (Apache-2.0) in `NOTICE` but says nothing about the user-supplied engine's GPL parts, and neither `phonemizer` nor `espeak` is mentioned anywhere in the repository. Today that is legally fine because OpenStage never ships them. Writing the arrangement down keeps it fine and stops a later installer from bundling them by accident. The owner chose to keep this arrangement (option 1).

**Done when.** `NOTICE` and the README say it.

## Group 2: The Rust shell

All of these are in `editor/src-tauri/`.

### OS-4. A streaming launch path
**Kind:** Blocks Release 1 | **Size:** M | **Needed by:** PRD stories A3, A4, A8, A10, G2

**What.** Add a second way to run a helper program, alongside the existing `run_job` in `lib.rs`:
- Keep the program's **standard input open**, so the shell can send one-line JSON commands (`pause`, `resume`, `stop`, `restart`, `marker`).
- Read its **standard output line by line while it runs**, and pass each line to the page as an event (for example with Tauri's event system), so the recorder panel can show live state.
- Give it **its own slot**, separate from the "preview or publish" slot (`JOB_BUSY`). While a recording is in progress, heavy jobs (publish, preview, narration, export) wait until the author pauses or stops, with a plain message. Editing and saving stay available.
- **Stop is graceful first:** send `stop`, wait up to 10 seconds for the program to finish and print its result, and only then fall back to `kill_tree`.
- Keep the existing behavior of the other jobs unchanged.

**Why.** The current path closes standard input (`Stdio::null()`), waits for the program to end (`wait_with_output`), and cancels by killing the whole process tree. That suits a publish run. It cannot run a recording that lasts an hour and needs pause and stop commands and live status. Killing a recording abruptly is the one thing the capture program is built to survive, but it should be the fallback, not the normal way to stop.

**Protocol.** The capture program speaks a small documented protocol (`docs/capture-protocol-spec.md`): one JSON object per line in each direction, the first output line states the protocol version, and the last is a `result`. The shell should treat unknown event types as ignorable.

**Done when.** From the editor, a test program can be started, sent `pause` and `stop`, and its events appear in the page as they happen. A second request is refused politely while one runs. Cancel still works for publish. The existing publish and preview behavior is unchanged.

**Accessibility.** State changes must reach the page's polite live region and the recorder panel, not only a visual indicator.

### OS-5. Global hotkeys
**Kind:** Blocks Release 1 | **Size:** S | **Needed by:** A3, A10

**What.** Add Tauri's global-shortcut plugin (verify the exact crate name, version, and permission names in Tauri v2's documentation) and allow it in `capabilities/default.json`, which today permits only dialogs, opening links, and setting or closing the window.

**Why.** Recording means the author is working in another program, so start, pause, resume, stop, and start again must work while OpenStage is not the focused window.

**Done when.** A hotkey registered from the page fires while another program has focus, can be changed by the author, and is unregistered when OpenStage closes. The settings screen can warn when a chosen combination uses a screen reader's modifier key (Insert or Caps Lock).

**Accessibility.** Defaults must not collide with NVDA, JAWS, or Narrator keys or with Windows. The proposed defaults are Ctrl+Shift+F9 (start), F10 (pause or resume), F11 (stop), and F12 (start again), to be collision-tested.

### OS-6. Previewing large local video in the editor
**Kind:** Needed | **Size:** M | **Needed by:** C3, C4, F2

**What.** Decide and implement how the editor plays a local video file (potentially hundreds of megabytes) without loading it all into memory. Today the window's security policy (`tauri.conf.json`) allows media only from the app itself, `blob:`, and one sound site, and narration audio is read into memory and played from a blob.

**Why.** Previewing an hour of recording, or even a few minutes, from an in-memory blob will be slow or fail. Tauri has a way to serve local files to the page (its asset protocol); using it means widening the policy carefully, limited to the project folders.

**Done when.** A multi-hundred-megabyte MP4 plays in the editor with seeking, and the policy allows it only from the open project's folders.

### OS-7. An `OPENSTAGE_CAPTURE` setting
**Kind:** Needed | **Size:** S | **Needed by:** G3

**What.** Add an `OPENSTAGE_CAPTURE` setting that gives the location of `openstage-capture.exe`, in the same style as `OPENSTAGE_NODE`, `OPENSTAGE_PYTHON`, and `OPENSTAGE_KOKORO_PYTHON`. If it is unset, look for the program the way other tools are found. If it is missing, show a plain message that says what to do.

**Why.** It keeps the capture program findable the same way as every other helper, and keeps the `OPENSTAGE_*` naming convention that CLAUDE.md says not to rename.

**Done when.** Setting it changes which program is used, and a missing program gives a clear message.

## Group 3: The player and the format

Every change here follows the project's own rules: update `docs/format-spec.md` in the same commit, bump `schemaVersion`, ship a migration, and keep the editor and the player on the same renderer.

### OS-8. Responsive output (reflow)
**Kind:** Blocks Release 1 (owner decision: fix first) | **Size:** L | **Needed by:** the whole Release 1 accessibility claim

**What.** Make the published course meet WCAG 2.2 AA 1.4.10 (Reflow): content works at a width of 320 CSS pixels (and at 400% zoom on a 1280-pixel window) without scrolling in two directions, with text re-wrapping instead of shrinking. Today the player scales the whole stage down to fit (`fitStage` in `player.ts`) and never reflows, and `format-spec.md` calls this "the open responsive-output decision."

**Why.** The owner decided that responsive output should be fixed before Release 1. Recordings placed in a course inherit the gap until then, and the accessibility statement must otherwise list it as a known failure.

**Design notes for OpenStage to weigh (not decided here).**
- Every object already has a required reading order (`tabOrder`), which might drive a stacked layout at narrow widths.
- WCAG excepts parts of content that need a two-dimensional layout to be usable or meaningful (videos, large images, screenshots with hotspots are candidates). Which parts of OpenStage's stage count is a judgment for an accessibility specialist; the owner chose to fix the gap rather than rely on the exception.
- The editor must let the author see and adjust the narrow layout, and it must use the same renderer as the player.

**Done when.** A published course passes the reflow check at 320 CSS pixels wide and at large text sizes, in the editor preview and in the published output, with no loss of content or function.

**Checkpoint.** If this proves much larger than expected, the owner can revisit the decision and ship with the gap stated plainly in the accessibility statement.

### OS-9. Audio description in the player
**Kind:** Needed | **Size:** M | **Needed by:** D5

**What.** Add audio description to the `video` object and the player:
- **The control:** a clearly labeled "Audio description" button beside the video. It opens an accessible pop-up that follows OpenStage's dialog rules (focus moves in, Escape closes it, focus returns, no keyboard trap).
- **In the pop-up the learner can:** read the description, listen to it, or **play the video with the description voiced in time**, pausing the video where the description needs more time (extended description).
- **The data (proposal):** a `video` object gains an optional list of description cues, each with a time, the description text, and the usual voice settings (`voice`, `speed`, `lang`). Description audio is generated from the text with the existing narration route and cached by content hash, like narration. Text stays the source of truth.

**Why.** WCAG 1.2.5 (Level AA) asks for audio description that is synchronized with prerecorded video, and a text-only alternative satisfies only Level A (1.2.3). If everything important on screen is already said in the narration, no extra description is needed, so the control appears only where the author has added description. The player currently plays video with the browser's own controls (`format-spec.md`), so this is new player work.

**Done when.** A video with description cues shows the button; the pop-up works by keyboard and with a screen reader; play-with-description plays the cues at the right times and pauses the video when a cue needs more time; a video with no cues shows no button; the validator checks the new fields.

### OS-10. A text-entry input
**Kind:** Needed | **Size:** M | **Needed by:** E3 (typing steps)

**What.** Add a free-text kind to the `input` object, next to checkbox, radio, drop-down, list, Likert, and semantic differential. Proposal: `kind: "text"` with a required `prompt`, a `variable` (string), an optional length limit, and, for answer checking, a choice to ignore capital letters and surrounding spaces and the ability to accept several correct answers. It is drawn as a native text field with a real label, and the answer can be compared in a trigger condition (`equals` already exists for strings).

**Why.** Practice mode asks the learner to do what the original recording did, including typing into a field. OpenStage has no free-text input today (`format-spec.md`, "Inputs and surveys"), so typing steps cannot be built.

**Done when.** A text input saves into a string variable, can be used in a condition, reports to the LMS when set to report, and is announced properly by NVDA and Narrator (visible label, clear error messages for wrong answers, no time limit).

**Accessibility.** WCAG 3.3.1 (error identification) and 3.3.2 (labels or instructions). Wrong-answer feedback must be announced, and must not rely on color alone.

### OS-11. Scoring from steps
**Kind:** Needed | **Size:** M | **Needed by:** E4 (assessment mode)

**What.** Let `hotspot` steps and text-entry steps contribute points to the course score, the results box, and SCORM reporting. Proposal: the same style of `scoring` field that `quizQuestion` has (`points`, `attemptsAllowed`), on hotspot and text-input objects.

**Why.** Today the score and the results box come only from `quizQuestion` objects, and the only question type is multiple choice (`player/src/runtime.ts`). Without a new source, Assessment mode cannot be scored.

**Constraint to design around.** SCORM 1.2 allows 4096 characters of `suspend_data` (`scorm.ts`). A course with many scored steps must store step results compactly, or a learner who leaves and returns will lose state. OpenStage's release gate says this resume behavior is load-bearing.

**Done when.** Passing and failing attempts of a scored step course report the correct score through SCORM 1.2; completion and success remain separate states; a learner who leaves and returns resumes with step results intact.

### OS-12. Counting attempts
**Kind:** Needed | **Size:** S | **Needed by:** E3

**What.** Confirm whether `setVariable` can add to a number (for example, add 1 to an attempts counter). The spec says that with no `value` it sets the variable to true, and the conditions can compare numbers (`lessThan` and so on). If it cannot add, add an optional `operation` (such as `add`).

**Why.** Practice mode gives a hint after a wrong try and reveals the answer after a set number of tries. That needs a counter.

**Done when.** A trigger can increase a number variable by a given amount, and the spec says so.

### OS-13. Define the `hotspot` object
**Kind:** Needed | **Size:** S | **Needed by:** E3

**What.** `format-spec.md` says the `hotspot` content is "not yet defined, any object accepted." Define it: a transparent, focusable region placed over a picture, with a required accessible name, a visible focus indicator, and a minimum target size.

**Why.** Practice steps place a hotspot over the target area of a screenshot. A defined hotspot gives every step the same keyboard and screen reader behavior.

**Done when.** A hotspot is reachable with Tab, activated with Enter or Space, announced with its name and role, has a visible focus indicator, and meets WCAG 2.5.8 (target size) and 2.4.7 (focus visible). The validator checks it.

### OS-14. Capture provenance on generated slides
**Kind:** Needed | **Size:** S | **Needed by:** E1

**What.** Let objects and slides that Capture generates carry a `provenance` entry naming the capture project and step, the way imported objects carry `sourceDeck`, `slideIndex`, and `shapeId` (for example `sourceCapture` and `stepId`). Editing such an object must not drop its provenance.

**Why.** Capture can regenerate step slides if the recording changes. Provenance lets a re-sync keep the author's edits, the same reasoning OpenStage uses for imported PowerPoint slides.

**Done when.** The format accepts the new provenance fields, the editor preserves them on edit, and the spec and migration are updated.

## Group 4: The publisher

### OS-15. Keep capture material out of `source/`
**Kind:** Recommended | **Size:** S | **Needed by:** B2

**What.** `player/publish.ts` adds everything in the project folder except `.cache/` to `source/` in the SCORM zip. Make the exclusion list configurable, and have the validator warn if recognizable raw capture material (such as an `events.jsonl` file or a capture folder) sits inside the project.

**Why.** Capture's own design keeps raw footage and interaction logs (keystrokes, window titles) in a separate folder outside the `.osproj`, so nothing needs to change today. This request is a safety net: if anyone places capture material inside a project, publishing would hand it to every learner.

**Done when.** A project containing a file named `events.jsonl` produces a validator warning, and the exclusion list can be set in `project.json`.

## Group 5: Verification that is already on your roadmap

### OS-16. Real LMS verification
**Kind:** Blocks Release 1 | **Size:** M

**What.** Run OpenStage's SCORM 1.2 output in Brightspace and Moodle (available to the owner now) and in Canvas (access on the way), using test courses. Check launch without error, correct scores for passing and failing attempts, completion and success as separate states, and returning a learner to their exact place.

**Why.** It is the unchecked item on `ROADMAP.md` (Phase 1) and the first item in `CLAUDE.md`'s release gate. SCORM 1.2 has only been checked against a stand-in. Capture's must-pass test builds on it, so any problem should be found before recordings are added.

**Done when.** The roadmap item is checked, with notes on each LMS.

### OS-17. A baseline screen reader pass
**Kind:** Blocks Release 1 | **Size:** M

**What.** Test the existing editor patterns that Capture will reuse with NVDA and Narrator on Windows: the canvas, the Properties panel, dialogs, the live region, the Timing list, and especially the **Timeline panel** with its slider grips. Record what works and what does not.

**Why.** The multi-track timeline extends the Timeline panel's design, and the recorder panel and clip list reuse OpenStage's patterns. None of it has been tested with a real screen reader (`ROADMAP.md`: "ATAG Part A pass on everything above, including NVDA" is unchecked). Building on untested patterns risks rebuilding them later.

**Done when.** A short written result exists for each pattern, and problems are fixed or listed.

## What this file does not ask for

- No change to slide timing (`appear`, `start`, `visibleFor`, `duration`). The media timeline lives in Capture's own project format.
- No second renderer, no second publisher, and no second player.
- No change to how narration works. Captions still come from the narration script, and speech recognition is never run when a script exists.
- No video editing inside OpenStage itself. The mid-level video editor is part of the Capture module.

## One more thing: the editor panels

Capture needs new panels in the editor (a recorder panel, a clip list, a timeline, a step review list, and a checker pane), written in TypeScript with plain HTML like the rest of the editor. The Capture author will write them. Placing them in `editor/src/` needs write access to the OpenStage repository (a branch, with a pull request for the owner to review). The owner decides whether and when to grant it.
