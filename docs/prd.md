# OpenStage Capture: Requirements (PRD)

**Status:** Draft v0.3, with the owner's answers on the proposed details recorded (section 9), for review
**Source:** the [product brief](product-brief.md) (v0.13). The brief records the reasoning and the decisions. This document turns them into requirements that a person can test.
**Scope:** Release 1, Windows only
**License:** Apache-2.0

> **Plain-language guide.** A *user story* says who wants what and why. Under each story, **Done when** is the checklist a tester uses, and **Accessibility checks** are extra tests that are never optional. Details marked **[P]** are *proposals*: sensible starting values that still need your confirmation. Details marked *(confirmed)* have been answered by the owner. Section 9 shows which is which.

## 1. Purpose

OpenStage Capture is a module of OpenStage Author. It lets an author record their screen, edit the recording in a mid-level video editor (like Camtasia), add captions, narration, and text overlays, and turn a recording into step-by-step training. OpenStage's own publisher then ships the result as part of a course. Everything is designed so that authors and learners who use assistive technology get the same quality of experience as everyone else.

## 2. Who it is for

| Person | What they need |
|---|---|
| **Course author** | Show how to use software inside a course, with narration, captions, and a practice mode |
| **Author who uses assistive technology** | Do everything above using only a keyboard, a screen reader, voice control, or a switch |
| **Learner** | Watch, practice, and be assessed, including learners who use assistive technology |
| **Maintainer** (the owner and collaborators) | Build, test, and release it without breaking OpenStage's rules |

## 3. How to read this document

- Stories are grouped into **epics** (A to H). Every story is part of Release 1. The suggested build order is in the brief, section 8.
- "OpenStage" means OpenStage Author. "The capture program" means `openstage-capture.exe`, the separate program that records.
- Story numbers (A1, C3, and so on) are stable, so tests and notes can refer to them.

## 4. Rules that apply to every story

| # | Rule |
|---|---|
| **R1** | **Accessibility baseline.** Every feature is fully usable by keyboard and with a screen reader. No keyboard traps; every dialog and panel closes by keyboard. Focus is visible and in a logical order. A control's accessible name contains its visible label. Windows high-contrast and forced-colors modes, text scaling, and reduced motion are respected. Nothing is conveyed by color alone. A feature that fails this is not finished. |
| **R2** | **Required fields.** Every object Capture adds to a course carries `altText`, `role`, `tabOrder`, and `decorative`, never optional and never silently filled with empty values. |
| **R3** | **Text has an audio route.** Any text the learner is meant to read can be given narration, and the editor makes that easy. Where a new kind of text has no audio route yet, the change summary says so. |
| **R4** | **Format and docs move together.** A change to the project format updates `docs/format-spec.md` in the same commit, bumps the schema version, and ships a migration. |
| **R5** | **Say what could affect accessibility.** Any change that could affect keyboard access, focus order, or screen reader output says so in its summary. |
| **R6** | **Private by default.** Raw recordings and interaction logs never ship inside a published course unless the author explicitly opts in. Interaction capture is opt-in. Nothing leaves the computer. |
| **R7** | **License rules.** Only Apache-2.0-compatible dependencies. No GPL. No bundled FFmpeg. Credit in `NOTICE`. Checked automatically (G4). |
| **R8** | **Plain language and project style.** Screens and help use plain words. Prose written for the project avoids em dashes. Anything that needs running is delivered as a double-clickable script. |
| **R9** | **Honest claims.** Only claim what was actually tested (which screen readers, which LMSs, which Windows versions). Unreviewed automatic text is labeled as such. |
| **R10** | **Keep the evidence.** Keep test results (keyboard-only runs, screen reader runs, LMS runs). They are the raw material for a real conformance report later. |

## 5. Stories

### Epic A: Recording

#### A1. Choose what to record
**As an** author, **I want** to record the full screen, one window, or a region, **so that** learners see only what matters.

**Done when**
- I can pick any connected display, any open window, or a region.
- A region can be set by typing numbers (position and size) as well as by dragging.
- The chosen target is shown in text before I start, for example "Window: Notepad" or "Region: 1280 by 720 at 100, 80".
- If the chosen window closes mid-recording, the recording stops safely with a clear message and keeps everything recorded so far.

**Accessibility checks**
- Choosing a target works by keyboard and screen reader (a named list; numeric entry for regions). Nothing requires drawing with a mouse.
- The chosen target is announced.

#### A2. Choose sound and camera
**As an** author, **I want** to choose my microphone, system audio, and webcam, **so that** I capture exactly the sound and picture I intend.

**Done when**
- Microphone, system audio, and webcam can each be turned on or off, in any combination.
- Level indicators have a text equivalent (for example "Microphone level: good"), not only a moving bar.
- If a device is missing or in use, a plain message says so, and I can choose to record without it.
- System audio includes screen reader speech when it is playing, so authors can demonstrate assistive technology workflows.

**Accessibility checks**
- Device pickers are labeled native controls.
- Level readouts are announced politely and at a sensible rate, not continuously.
- A webcam preview is optional and has a text status.

#### A3. Start, pause, resume, and stop with the keyboard
**As an** author, **I want** to control recording with hotkeys that work in any program, **so that** I can record software without switching windows or using the mouse.

**Done when**
- Start, pause and resume, and stop each have a hotkey that works while another program has focus. Pause and resume share one key, which announces the new state.
- Hotkeys are configurable. The settings screen warns if a choice uses a screen reader's modifier key (Insert or Caps Lock).
- **Default hotkeys [P]:** Ctrl+Shift+F9 to start, Ctrl+Shift+F10 to pause or resume, Ctrl+Shift+F11 to stop, and Ctrl+Shift+F12 to start again (A10). During the capture spike they are tested against the default keys of NVDA, JAWS, and Narrator, Windows itself, and common programs, and changed if any collide.
- An optional countdown runs before recording starts: 3 seconds by default, adjustable, and it can be turned off *(confirmed)*.
- Stop finishes cleanly: every file is finalized and I am told where the recording was saved.
- The same four actions are also buttons in the recorder panel.

**Accessibility checks**
- All four actions work from the keyboard, in the panel and by hotkey, with NVDA and Narrator running.
- No action needs a pointer.

#### A4. Know the recording state without sight
**As an** author who cannot see the screen, **I want** clear sounds and text for what the recorder is doing, **so that** I always know whether I am recording.

**Done when**
- Countdown, start, pause, resume, stop, and error each have a distinct sound. The sounds are short, have adjustable volume, are on by default, and can be turned off *(confirmed)*.
- The state is also shown as text in the recorder panel, announced to assistive technology when OpenStage has focus, and shown again when OpenStage regains focus.
- Elapsed time can be asked for and read; it is not announced constantly.
- Errors (disk full, device lost) are both sounded and written as text.

**Accessibility checks**
- Sound is never the only signal. Nothing flashes more than three times a second. Color is never the only indicator.
- With NVDA and Narrator running and another program in front, start and stop confirmations are still perceivable. This is the specific risk to test, because OpenStage's announcements may not be heard when its window is not focused.

#### A5. Keep the recorder out of the video
**As an** author, **I want** the recorder's own controls kept out of the picture, **so that** learners never see them.

**Done when**
- The recorder panel, the countdown, and the OpenStage window are not in the recorded picture, unless I choose to record OpenStage itself.
- I can see which windows are excluded.

**Accessibility checks**
- The excluded state is announced and can be changed by keyboard.
- **Depends on** a Windows capability that must be verified to work with OpenStage's web-based window.

#### A6. Record interactions (opt-in)
**As an** author, **I want** to record where I click, what I type, and which window is active, **so that** Capture can suggest zooms, steps, and step text.

**Done when**
- Interaction recording is off until I turn it on for a given recording.
- When on, it records cursor position, clicks, key presses, the active window's title, and, where Windows provides it, the name and role of the clicked element.
- Text typed into password fields is never recorded.
- A persistent indicator shows that interaction recording is on, and it is also announced and sounded when recording starts.
- I can review what was captured and delete the interaction log.

**Accessibility checks**
- The opt-in explanation is plain language and keyboard accessible.
- The indicator is not visual only.

#### A7. Protect private content
**As an** author, **I want** private information kept out of the recording, **so that** I do not expose passwords or personal details.

**Done when**
- I can choose windows to exclude. Excluded windows appear blank or blurred in the recording.
- Notification pop-ups are kept out of the recording by default, and I can change that *(confirmed)*.
- A short privacy checklist appears before the first recording and can be reopened.
- Afterward, any region can still be blurred or blacked out (see C8).

**Accessibility checks**
- The checklist and exclusion settings are fully keyboard accessible.

#### A8. Survive a crash
**As an** author, **I want** a recording that is not lost if something stops, **so that** a crash costs me seconds, not the whole take.

**Done when**
- The recording is saved in small pieces as it goes.
- If the capture program or the computer stops mid-recording, the next time I open the project I am offered recovery and get back everything except the last few seconds (at most 10 seconds) *(confirmed)*.
- OpenStage's existing Cancel (which stops the program abruptly) never leaves a broken file: the result is either recoverable or cleanly discarded.
- Discarding a recording is a separate, explicit action with a confirmation. (Starting again, A10, is its own explicit action and sets the take aside instead of erasing it.)
- Test: end the program during a recording and recover it.

**Accessibility checks**
- The recovery offer and the discard confirmation are accessible dialogs, closable by keyboard.

#### A9. First-run setup and permissions
**As an** author, **I want** a gentle first-time setup, **so that** Windows permission prompts do not leave me stuck.

**Done when**
- Before Windows asks for screen, microphone, or camera access, a plain explanation says what is coming and why.
- Setup checks whether this Windows installation has the media components MP4 export needs, and says what to install if not. (Some "N" editions of Windows may lack them; verify.)
- Windows 11 and later are supported. Earlier versions are not *(confirmed)*.
- Optional parts (such as the webcam) never block recording if unavailable.

**Accessibility checks**
- The whole flow, including Windows' own prompts, is completed with the keyboard alone and with NVDA and Narrator.

#### A10. Start again when a take goes wrong
**As an** author, **I want** to throw away a recording that went wrong and begin again with one action, **so that** a slip does not cost me a long detour.

**Done when**
- While recording or paused, one action (a hotkey, default Ctrl+Shift+F12 **[P]**, or a button in the recorder panel) stops this take, sets it aside, and starts a new recording with the same target, sound, camera, and options. The countdown, if on, runs first.
- It also works right after I stop: a "Record again" button starts a new take with the same settings, and I choose whether the earlier take is kept or set aside.
- **Nothing is erased.** A take set aside goes to a "Discarded takes" list inside the capture project, where I can play it or restore it. Only emptying that list deletes anything, and that asks for confirmation. **[P: this is the recommended way to make "start again" safe.]**
- There is no confirmation prompt when starting again. That keeps it fast and means it never pulls focus away from the program being recorded. Nothing is lost, so none is needed.
- A sound and a text message announce it, for example "Take set aside. Starting again."
- Discarded takes are never published (B2).

**Accessibility checks**
- Works by hotkey and by keyboard in the panel, with NVDA and Narrator running. No step has a time limit. The Discarded takes list is a labeled table that works by keyboard.

### Epic B: Projects and files

#### B1. A capture project folder
**As a** maintainer, **I want** each recording stored in its own documented folder, **so that** other tools can read it and edits stay non-destructive.

**Done when**
- Each recording has its own folder beside the course (named `name.capture`, *confirmed*), holding the raw media in pieces, the interaction log, the edit decisions (the media timeline), and tracks for captions, transcript, audio description, alt text, and language.
- The format is open (JSON plus media files, no proprietary container), documented, and versioned. Every change bumps the version and ships a migration.
- Media are referenced, not embedded.

**Accessibility checks**
- The schema includes the required accessibility fields from the first version.

#### B2. Raw material never ships
**As an** author, **I want** my raw footage and logs kept out of published courses, **so that** learners never receive my keystrokes or window titles.

**Done when**
- The capture project lives outside the course's `.osproj` folder.
- A published course (the web folder and the SCORM zip) contains no raw recordings, interaction logs, or window titles. Test: unzip a published package and look.
- If I explicitly opt in to including them, a plain warning says what will be included.

**Accessibility checks**
- The opt-in is a keyboard-accessible, plain-language dialog.

#### B3. Render once, store as an asset
**As an** author, **I want** my finished video stored once in the course, **so that** publishing is predictable.

**Done when**
- Exporting renders the video once, stores it in the course's `assets/video/`, and stores its captions in `assets/captions/`.
- Publishing never re-renders. A course saved, closed, reopened, and republished is byte-identical (OpenStage's own release gate).
- Re-exporting after edits replaces the video cleanly and leaves no stale copies.

#### B4. Close it and come back
**As an** author, **I want** to close a project and continue later, **so that** long edits are safe.

**Done when**
- After save, close, and reopen, all tracks, clips, overlays, and settings are intact.
- Editing is non-destructive: the original recording is never changed. Trims, splits, and effects are decisions stored on top of it.
- If the capture folder is missing or moved, a clear message appears, and the course still opens and publishes using the last exported video.

### Epic C: Editing (mid-level)

#### C1. See tracks and clips as a list
**As an** author, **I want** a list of every track and clip with all their settings, **so that** I can edit without a mouse.

**Done when**
- A table lists each track and clip with its name, type, start, length, and state, with a caption and column headers.
- Every property can be changed with a field, including exact timecodes.
- This list is the foundation of the timeline (C3): both make exactly the same changes.

**Accessibility checks**
- Arrow keys move between rows. Selection and focus are announced. No keyboard trap.
- Verified with NVDA and Narrator.

#### C2. Trim, split, delete, join, and reorder
**As an** author, **I want** the basic cutting tools, **so that** I can tidy a recording.

**Done when**
- I can split at the playhead, trim a clip's start or end, delete a clip, join adjacent clips, and reorder clips.
- Deleting can either leave a gap or close it, and the author chooses *(confirmed)*.
- Results are announced in words, for example "Clip split at 1 minute 12.4 seconds, now two clips".
- Every action can be undone and redone (see C11).

**Accessibility checks**
- Each action has a documented keyboard route. No mouse-only action.

#### C3. Multi-track timeline (Camtasia-style)
**As an** author, **I want** a timeline with separate tracks for screen, sound, webcam, captions, and overlays, **so that** I can arrange everything over time.

**Done when**
- A visual timeline shows tracks and clips along a time axis, with a playhead and a way to zoom the timeline itself.
- Each clip has start and end grips that are real sliders, following OpenStage's existing Timeline: Left and Right Arrow move a tenth of a second, Shift a second, Page Up and Page Down five seconds, Home and End to the ends. Changes are announced in words.
- Mouse dragging also works. Escape cancels a drag. A whole drag is one undo.
- The timeline takes part in F6 region cycling, like OpenStage's existing Timeline.
- The list (C1) and the timeline always make the same changes. Nothing is possible only with the mouse.
- **The timeline does not ship until its keyboard model is complete and verified with NVDA and Narrator.**
- A 60-minute recording stays usable **[P: editing actions respond without noticeable delay]**.

**Accessibility checks**
- Works in forced-colors mode and at larger text sizes. Nothing conveyed by color alone. Reduced motion respected.

#### C4. Know what is on screen without seeing it
**As an** author who cannot see the screen, **I want** a text description of the picture at any point, **so that** I can check my work.

**Done when**
- A readout, available on request, says what is at the playhead: which window, what the cursor is on (if known), which overlays and captions are showing, and the current zoom.
- It updates when the playhead moves and is reachable by keyboard.
- It is read politely when asked for, not announced constantly.

#### C5. Zoom and pan into parts of the screen
**As an** author, **I want** to zoom into the part that matters, **so that** learners can see small details.

**Done when**
- I can add a zoom to a region for a span of time, with a smooth transition, half a second by default *(confirmed)*.
- Capture can suggest zooms from where I clicked. I accept, edit, or reject each one.
- A zoom region can be set with numbers as well as by dragging.
- Zoom animations never flash more than three times a second.
- Because a learner cannot turn off motion that is part of a video, export offers a **reduce motion** choice that uses cuts instead of animated zooms *(confirmed)*.
- What the zoom shows is also available as text (see D4 and D5).

**Accessibility checks**
- Everything is possible by keyboard and numeric entry.

#### C6. Cursor effects
**As an** author, **I want** to restyle and highlight the cursor and show clicks, **so that** learners can follow it.

**Done when**
- I can change the cursor's size and style, add a highlight, and show clicks with an effect such as a ring.
- Effects are applied when the video is rendered, from the recorded events, so they can change without re-recording.
- If interaction recording was off, these options explain why they are unavailable.
- Effects never flash more than three times a second, and a warning appears if a highlight has weak contrast.

**Accessibility checks**
- Options have text names; a preview is available.

#### C7. Text overlays and callouts
**As an** author, **I want** to put titles, labels, lower thirds, and callouts on the video, **so that** I can point things out.

**Done when**
- I can add text overlays and set their size, color, background, and position (by numbers or by dragging).
- A contrast warning appears when text is hard to read, and a size warning when it is very small.
- The overlay's text is also part of the transcript (D4), so it is not video-only.
- Each overlay has its alt text or is marked decorative (R2).
- Overlay text can be given narration (R3).

**Accessibility checks**
- Fully editable by keyboard.

#### C8. Shapes, arrows, blur, and redact
**As an** author, **I want** to draw attention to things and hide private details, **so that** the video is clear and safe.

**Done when**
- I can add shapes and arrows, and blur or black out a region for a span of time.
- I am told that blur and redaction in the exported video cannot be undone by the viewer.
- Each shape or arrow has alt text or is marked decorative.

**Accessibility checks**
- Creating, moving, and timing these works by keyboard and numeric entry.

#### C9. Audio basics
**As an** author, **I want** to adjust volume and fades, **so that** the sound is comfortable.

**Done when**
- I can set a clip's volume, mute it, and fade it in and out.
- Level displays have a text equivalent.
- Noise removal and advanced mixing are not in Release 1.

#### C10. Bring in outside media
**As an** author, **I want** to add images, music, voice-overs, and short video clips, **so that** I can build a complete tutorial.

**Done when**
- I can add images, audio, and short video clips from the course's asset library onto the timeline. "Short" means 30 seconds or less *(confirmed)*.
- Each file keeps its source, creator, and licence in the course's asset record and appears on the credits page.
- At import, I am prompted for what OpenStage's rules require: an image needs alt text or must be marked decorative; a video needs a caption file or a narration script; an audio clip needs captions, a script, or a transcript. For wordless background music, a short description counts as the transcript (confirm when built).
- No filters or professional effects are added.

**Accessibility checks**
- The import and prompts are keyboard accessible; prompts come at import, not only at export.

#### C11. Undo and redo
**As an** author, **I want** to undo and redo any edit, **so that** I can experiment safely.

**Done when**
- Every editing action can be undone and redone with the standard keys.
- A mouse drag is one undo step.
- Undo and redo are announced, for example "Undid: split clip".

### Epic D: Captions, narration, and text equivalents

#### D1. Captions from a script
**As an** author, **I want** captions made from my narration script, **so that** they match exactly and need no speech recognition.

**Done when**
- Captions are derived from the script, word for word, and timed to the narration audio when it exists.
- If the audio has not been made yet, captions can still be produced with estimated timing, clearly marked, following OpenStage's existing fallback.
- Speech recognition is never run when a script exists.
- Captions and transcripts still work with no speech engine installed.

**Accessibility checks**
- Captions are readable (size, contrast, line length) and the language is set.

#### D2. Edit captions; import and export
**As an** author, **I want** to edit captions and move them in and out as standard files, **so that** I can fix or reuse them.

**Done when**
- Captions have their own track, with editable text and timing by fields.
- WebVTT and SRT files can be imported and exported. Each track has a language.
- Warnings (never blockers) appear for captions that are hard to read **[P: based on common caption guidance]**: more than 2 lines; more than 42 characters on a line; faster than about 160 words per minute (roughly 20 characters per second); or on screen for less than 1 second. The numbers are adjustable. WCAG sets no numbers for this. These come from published caption guides (DCMP, Netflix, and BBC) as summarized by secondary sources, so the official guides should be checked before the numbers are finalized.
- A caption I edited by hand is marked, and regenerating never overwrites it without asking.

#### D3. Captions from a recorded voice
**As an** author who recorded my own voice, **I want** captions generated automatically, **so that** I do not type them all.

**Done when**
- Captions can be generated locally from recorded audio, with nothing sent off the computer.
- The result is labeled "auto-generated, unreviewed" until I approve it.
- It plugs in through OpenStage's planned "captions from media" contract. No model-specific code enters the format or the renderer.
- The first tool audited is Whisper, which is already on OpenStage's roadmap. Its license is believed to be MIT and must be verified *(confirmed order)*.
- **Fallback rule:** if no speech recognition tool passes the license and quality audit, this story leaves Release 1. D1 and D2 stay.

**Accessibility checks**
- The review flow works by keyboard, and the unreviewed state is announced.

#### D4. Transcripts and step text
**As a** learner, **I want** a transcript next to the video, **so that** I can read what is said and shown.

**Done when**
- A transcript is generated from the captions, the step text, and overlay text, and stored in the video object's transcript field.
- It includes text descriptions of important on-screen events that the audio does not say, such as zooms and callouts **[P: drawn from step text and audio description]**.
- Learners can read it beside the video.

#### D5. Audio description
**As a** learner who cannot see the screen, **I want** the important visuals described aloud, **so that** I understand the video.

**Done when**
- The author can write short descriptions at points in time. They are voiced through the same script-to-speech route as narration, or recorded.
- Not every video needs description. If everything important on screen is already said in the narration, no extra description is needed. The checker (F4) flags videos with important visual-only content so the author can add description where it matters.
- **Delivery (owner: a pop-up when the learner asks for it).** A clearly labeled "Audio description" button sits by the video and opens an accessible pop-up that follows OpenStage's dialog rules: focus moves in, Escape closes it, focus returns, no keyboard trap. In the pop-up the learner can read the description, listen to it, or play the video with the description voiced in time with it, pausing the video where needed (extended description) *(confirmed)*.
- **Why the third option:** WCAG 1.2.5 asks for audio description that is synchronized with prerecorded video, and a text-only alternative satisfies only the Level A criterion (1.2.3). The owner confirmed that play-with-description is in Release 1.
- **Depends on** OpenStage's player. A video object currently plays with the browser's own controls *(confirmed)*, so the pop-up, the described playback, and pausing for extended description are new player features in OpenStage (section 6, item 6).

**Accessibility checks**
- Reviewed by someone who uses a screen reader. The pop-up passes the dialog checks (focus, Escape, no keyboard trap).

#### D6. Alt text and required fields for everything Capture adds
**As an** author, **I want** help filling the required accessibility fields, **so that** nothing is left empty.

**Done when**
- Every object Capture adds has `altText`, `role`, `tabOrder`, and `decorative` (R2).
- Alt text can be pre-filled from the clicked element's name and role (for example "Button: Save"). Pre-filled text is marked unreviewed until I approve it.
- The accessibility checker (F4) lists everything still unreviewed.

#### D7. Audio for text the learner reads
**As an** author, **I want** to give step text, callouts, and overlays an audio route, **so that** the course works for someone who cannot read the screen.

**Done when**
- Each of those can be given narration from a script with one action.
- With no speech engine set up, the editor says so plainly, and everything else keeps working.

### Epic E: Interactive training

#### E1. Turn a recording into step-by-step slides
**As an** author, **I want** a recording turned into a series of steps, **so that** I can build a simulation without starting from scratch.

**Done when**
- From the interaction log, Capture proposes steps (one per click, typed entry, or window change). I can merge, split, delete, and reorder them.
- Each step becomes an ordinary OpenStage slide with a screenshot, step text (pre-filled from element names when available), and a callout on the target.
- The text on these slides is real text, with required fields filled (R2).
- It works without element names: I type the step text myself.
- Each screenshot has alt text describing the step.

**Accessibility checks**
- The step review list is keyboard accessible, with a clear reading order.

#### E2. Demo mode (watch)
**As a** learner, **I want** to watch the procedure step by step, **so that** I see how it is done.

**Done when**
- Steps advance with narration and captions, and the learner can go next, back, or pause.
- Reduced motion is respected.
- It uses OpenStage's player. There is no second player.

#### E3. Practice mode (try it)
**As a** learner, **I want** to try each step myself, **so that** I learn by doing.

**Done when**
- Each step's target is a hotspot that works by mouse, keyboard, or touch. Typing steps use a text input.
- The learner gets clear feedback on right and wrong, a hint, and a "show me" option.
- There are no click-only interactions. Every hotspot is reachable with Tab and activated with Enter or Space, with a name and role from the step.
- Feedback is announced to screen readers.
- There are no time limits by default. If any exist, they can be adjusted or removed (WCAG 2.2.1).

**Depends on** OpenStage's interaction support. Its roadmap lists hotspot, text-entry, and scenario interactions as not built. Practice may be buildable from existing pieces (hotspot objects, inputs, layers, and the rules editor) or may need new interaction types. **To be confirmed in design.**

#### E4. Assessment mode (test me)
**As an** instructor, **I want** a scored version, **so that** I can check that learners can do the task.

**Done when**
- The same steps run without hints and are scored.
- Passing and failing attempts both report the correct score through OpenStage's quiz and score system.
- Course completion and assessment success stay separate states, as OpenStage already requires.
- Time limits, if any, are adjustable or removable.

### Epic F: Export, checking, and publishing

#### F1. Export MP4
**As an** author, **I want** an MP4, **so that** the video plays everywhere.

**Done when**
- Export produces MP4 with H.264 video and AAC audio, using Windows' built-in encoders, plus a separate caption file.
- The video plays in current browsers and in the course player.
- Progress is shown as text, export can be cancelled, and a cancelled export never leaves a partial file in the course.
- If the Windows edition lacks the encoders, a clear message says what to install.

**Accessibility checks**
- Progress is announced politely. Cancel works by keyboard.

#### F2. Insert it into a course
**As an** author, **I want** one action to put the video in a slide, **so that** I do not assemble it by hand.

**Done when**
- A single action adds the exported video as a `video` object with its captions and transcript filled in, and prompts for any required field (R2). For step-by-step content, it adds the slides at the position I choose.
- OpenStage's validator passes with no "time-based media needs captions" error.
- Course rules can use the video's "finished playing" event.
- Any validator findings appear in the editor's problem list.

#### F3. Publish through the one publisher
**As an** author, **I want** my course published the normal way, **so that** nothing drifts apart.

**Done when**
- A course containing Capture output publishes through OpenStage's existing publish step to both outputs: a folder of web files and a SCORM 1.2 zip. Capture has no exporter of its own.
- Preview and published output look and behave the same (one renderer).
- No raw capture data is in either output (B2).
- **Must pass:** the package launches in Brightspace and Moodle (and in Canvas once access exists) without error, reports completion and score correctly for passing and failing attempts, and returns a learner who leaves to their exact place.

**Depends on** the real-LMS verification of OpenStage's existing SCORM 1.2 output (section 6).

#### F4. Check before you finish
**As an** author, **I want** a checker that lists accessibility problems, **so that** I can fix them before publishing.

**Done when**
- A pane lists: missing captions, missing or unreviewed alt text, unreviewed automatic text, weak contrast, flashing risk, unlabeled hotspots, text with no audio route, and any raw data set to be included.
- Each item takes me straight to what needs fixing.
- Warnings are prominent but never block exporting. The author decides.
- The same list is available as plain text for a screen reader.
- The checker never claims the course "conforms". It reports what it found.

### Epic G: Integration with OpenStage Author

#### G1. A program with no interface and a documented protocol
**As a** maintainer, **I want** the capture program to have no interface of its own, **so that** OpenStage's accessible interface is the only one.

**Done when**
- The program is `openstage-capture.exe`, with commands to record, render, probe the system, and recover.
- Its first output line states the protocol version. It then reports events as lines of JSON: state, elapsed time, throttled audio levels, warnings, errors, and a final result with the project path. Logs go to a separate channel.
- It accepts pause, resume, stop, restart (set this take aside and start a new one with the same settings), and marker commands.
- The protocol is documented in a short spec and versioned.
- It can run on its own from a script, so it can be tested without OpenStage.

#### G2. A streaming launch path in OpenStage's shell
**As a** maintainer, **I want** OpenStage to control a live recording, **so that** pause and stop work and the page shows live state.

**Done when**
- The shell can start the program while keeping its input open and reading its output as it arrives, and can show the state in the page.
- A recording has its own slot, separate from preview and publish. While a recording is in progress, heavy jobs (publishing, previewing the course, making narration audio, and exporting video) wait until I pause or stop, with a plain message, because they would slow the recording and can cause dropped frames. Everything else, such as editing slides and saving, stays available **[P: the owner left this open; this is the recommended default]**.
- Stop is graceful first. The abrupt kill is the fallback only after a wait **[P: 10 seconds]**.
- The window's security policy allows previewing local video without opening it to other sources.
- This change is agreed with the owner first, as CLAUDE.md requires.

**Accessibility checks**
- State changes reach the page's polite live region and the recorder panel.

#### G3. Find the program
**As a** maintainer, **I want** the program found the same way as OpenStage's other tools, **so that** it is easy to run.

**Done when**
- An `OPENSTAGE_CAPTURE` setting overrides its location. Otherwise it is found like the other tools while running from source.
- If it cannot be found, a plain message says what to do.
- How it ships with an installer is decided together with OpenStage's installer, which is not built yet.

#### G4. Rule checks from the first line
**As a** maintainer, **I want** the license rules checked automatically, **so that** a forbidden dependency cannot slip in.

**Done when**
- Every change is checked for disallowed licenses.
- The check fails if any FFmpeg build uses `--enable-gpl` or `--enable-nonfree`, and flags `--enable-version3` for review.
- Source files carry license headers, and `NOTICE` credits third-party work.
- No code from another project is added without a `NOTICE` entry and a license check of those files.

### Epic H: Honesty and release readiness

#### H1. Publish an accessibility statement
**As an** adopter, **I want** an honest statement of what is tested, **so that** I can decide whether to rely on the tool.

**Done when**
- A plain-language statement says what is targeted (WCAG 2.2 AA for output; ATAG 2.0 Part A, plus Part B for this module, for the editor), what was actually tested (NVDA and Narrator on Windows; which LMSs), what is known to be incomplete, and how to report a problem.
- It is updated at every release. It claims nothing that was not tested.
- It names the reflow gap until OpenStage's responsive output ships, and VoiceOver and JAWS as untested.

#### H2. Release gate
**As the** owner, **I want** one checklist that decides whether Release 1 ships, **so that** nothing slips through.

**Done when** all of the following hold, with Capture content in the course:
- The SCORM package launches without error in Brightspace and Moodle (and Canvas once available); passing and failing attempts report the right score; completion and success are separate; a learner who leaves returns to their exact place.
- The whole editor and the whole published course work by keyboard alone, and with NVDA and Narrator. (VoiceOver is not yet tested, and the statement says so.)
- Generated captions match the narration script exactly and are correctly timed.
- A saved, closed, reopened, and republished course is byte-identical.
- A recording stopped by killing the program is recovered.
- A published package contains no raw recordings or logs.
- Published courses pass the reflow check (WCAG 1.4.10) at 320 CSS pixels wide and at large text sizes, once responsive output ships.
- Automated accessibility checks on the generated pages show no serious or critical findings.
- The license checks pass.

#### H3. Help in plain language
**As an** author, **I want** built-in help, **so that** I can learn every feature and its keys.

**Done when**
- Help describes each feature and its keyboard keys in plain words.
- All shortcuts are listed in one place, reachable by keyboard.

## 6. Prerequisites and dependencies

Release 1 does not ship until these are done. Items 1, 2, 4, and 6 are OpenStage work outside this module.

| # | Prerequisite | Why |
|---|---|---|
| 1 | **Responsive output in the published player** (reflow, WCAG 1.4.10) | Owner decision: fix first. With a checkpoint if it proves much larger than expected (brief, section 8). |
| 2 | **OpenStage's SCORM 1.2 verified in a real LMS** (Brightspace and Moodle, then Canvas) | It has only run against a stand-in. F3 and H2 depend on it. |
| 3 | **The streaming launch path** (G2) | Recording cannot run on the current one-job-at-a-time launcher. |
| 4 | **A baseline screen reader pass** on the OpenStage patterns Capture reuses | No real screen reader testing exists yet. |
| 5 | **CLAUDE.md updated** for the three accepted exceptions (Windows first, mid-level video editor, Rust capture program) | Keeps the project's own rules and the work in agreement. |
| 6 | **Player support for audio description:** the pop-up, playing the video with the description voiced in time, and pausing for extended description | D5 needs it. The player currently plays video with the browser's own controls. |

**Not prerequisites:** timing that follows a video (a roadmap request), and macOS, Linux, JAWS, or VoiceOver work.

## 7. Non-functional requirements

| Area | Requirement |
|---|---|
| **Performance** | Capture overhead below about 10% CPU on a mid-range laptop at 1080p30. Audio and video drift under 40 ms over a 60-minute recording. |
| **Reliability** | A killed recording loses only the last few seconds (A8). A cancelled export never leaves a partial file. A failed edit never damages the original recording. |
| **Compatibility** | Windows 11 and later only *(confirmed)*. Editions are checked by the capture spike and tested (A9). MP4 plays in current browsers. |
| **Privacy and security** | Everything stays on the computer. Interaction capture is opt-in. Raw material never ships (R6). The program reads only what the author chose to record. Inputs to the program are validated, as OpenStage already does for other tools. |
| **Licensing** | As in R7 and the brief, section 7. Narration engine components that are under the GPL stay separate and user-installed, and are never bundled or mirrored. |
| **Usability** | A first-time author records a short tutorial and puts it, captioned, into a course in under 10 minutes. |
| **Maintainability** | The format is documented and versioned; protocol and format changes ship with their documentation. |

## 8. Out of scope (after Release 1, not scheduled)

macOS and Linux; JAWS and VoiceOver testing; SCORM 2004; xAPI and cmi5; WebM output; a sign-language video track; accessible PDF or HTML step guides; templates and themes; noise removal; localization; a plugin API; color correction, green screen, multi-camera editing, a large effects catalog, and motion graphics.

## 9. Details: confirmed and still proposed

The owner answered the fourteen proposed details and asked for one more feature (item 15). This table shows where each stands.

| # | Detail | Status |
|---|---|---|
| 1 | Countdown before recording (A3) | **Confirmed:** 3 seconds, adjustable, or off |
| 2 | Default hotkeys (A3) | **Confirmed that defaults are needed. Values proposed:** Ctrl+Shift+F9 start, F10 pause or resume, F11 stop, F12 start again; tested for collisions in the capture spike |
| 3 | Sounds (A4) | **Confirmed:** short, adjustable volume, on by default, can be turned off |
| 4 | Notification pop-ups (A7) | **Confirmed:** excluded by default |
| 5 | Maximum recovery loss (A8) | **Confirmed: 10 seconds** (changed from the proposed 5) |
| 6 | Capture folder name (B1) | **Confirmed:** `name.capture` |
| 7 | Delete leaves a gap or closes it (C2) | **Confirmed:** the author chooses |
| 8 | What "short" means for outside video clips (C10) | **Confirmed:** 30 seconds or less |
| 9 | Zoom transition and reduce-motion export (C5) | **Confirmed:** half a second; reduce-motion choice included |
| 10 | Caption limits (D2) | **Proposed from best-practice research:** at most 2 lines; at most 42 characters a line; about 160 words per minute (roughly 20 characters per second); at least 1 second on screen; warnings only; adjustable. Check the official guides before finalizing |
| 11 | How audio description reaches learners (D5) | **Confirmed:** a pop-up when the learner asks for it, with read, listen, and play-with-description options (needed to meet WCAG 1.2.5 at Level AA) |
| 12 | What may run during a recording; the stop wait (G2) | **Left open by the owner.** Recommended default: heavy jobs wait until pause or stop; everything else stays available. The 10-second stop wait stands unless changed |
| 13 | Supported Windows versions (A9, section 7) | **Confirmed:** Windows 11 and later |
| 14 | Speech recognition tool to audit first (D3) | **Confirmed:** Whisper first (license to be verified) |
| 15 | Start again (A10) | **Requested by the owner:** "start again if the first recording was messed up". **Proposed details:** default hotkey Ctrl+Shift+F12; the old take is set aside in a Discarded takes list, not erased; no confirmation prompt |

**Still to settle**
- The hotkey values, after collision testing (item 2).
- The caption numbers, after checking the official guides (item 10).
- The details of Start again, especially the Discarded takes list (item 15).
- Whether the recommended default for jobs during a recording is right (item 12).
- The editing performance target for 60-minute recordings (C3).

## 10. Traceability to the brief

| The brief's Release 1 group | Covered by |
|---|---|
| Recording | A1 to A10, G1 to G3 |
| Editing | B4, C1 to C11 |
| Accessible content | D1 to D7, F4 |
| Interactive training | E1 to E4 |
| Output and publishing | B3, F1 to F3 |
| Quality and honesty | B1, B2, G4, H1 to H3 |
| Prerequisites | Section 6 |
