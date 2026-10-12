# OpenScreen — Product Brief

*(working title; see [Naming](#13-naming))*

**Status:** Draft v0.3 — for review
**License:** Apache-2.0 (hard constraint; see [Licensing & dependency policy](#7-licensing--dependency-policy))
**Context:** A module of **OpenStage**, a Tauri v2 desktop app (Rust shell, plain TypeScript + HTML interface). Delivered as a **separate capture process** that OpenStage's interface drives.
**Accessibility:** First-class requirement, for the tool *and* for everything it produces (see [section 6](#6-accessibility-requirements))

## Decisions so far

| Decision | Value |
|---|---|
| License | Apache-2.0 |
| Host project | OpenStage: Tauri v2, Rust shell, TypeScript + plain HTML UI (WebView2 on Windows), no UI framework |
| Engine language | Rust |
| Process model | Separate process with **no interface of its own**. OpenStage's UI and global hotkeys drive it; JSON on stdout; same pattern OpenStage already uses for `publish.ts`, `narrate.py`, and `validate.py` |
| Interface | Native HTML controls inside OpenStage. No separate GUI toolkit |
| Platforms | **Windows first** (the only test hardware available). macOS and Linux later, when testers exist |
| Accessibility bar | WCAG 2.2 AA for output; ATAG 2.0 Part A for the editor (this brief proposes adding Part B for this module) |
| Speech | Local only, permissive licenses only |
| Regulation | None recorded in OpenStage; see [6.4](#64-compliance-context) |
| Name | OpenScreen (working title, unconfirmed) |

---

## 1. Summary

An open-source, accessible screen recorder for people who make **instructional video and software training**. It covers the ground Camtasia holds for video and Adobe Captivate / Articulate Storyline hold for e-learning software simulations. The capture engine is a permissively licensed Rust process; the recorder and editing experience live inside OpenStage's accessible HTML interface.

Three ideas drive the design:

1. **Record the screen and the user's interactions as separate, editable data.** Video plus cursor, clicks, keystrokes, active window, and (where the OS allows) the UI element that was clicked, rather than flattened pixels. That enables re-styled cursors, automatic zoom-and-pan, auto-generated step text, and conversion of a recording into an interactive "show me / let me try / test me" simulation.
2. **Accessibility is built into the data model, not bolted on.** Captions, transcripts, audio description, alt text, and language are first-class tracks in the project format. The same UI-element data that powers step captions also powers alt text and accessible step-by-step guides.
3. **Script-first narration.** OpenStage already turns a narration script into speech and captions locally. A recording can use the same route: write the script, record the demo, and get narration and captions from the script with no speech-to-text. Speech-to-text is only needed when the author records their own voice.

## 2. Problem and opportunity

- Camtasia is excellent for video but closed, paid, and not embeddable.
- Captivate and Storyline do software simulations well but are heavy authoring suites; screen capture is a means to an end.
- Open-source recorders (OBS and similar) capture well but have no editing, no interaction data, and no e-learning output. The most capable ones are GPL, so they cannot be embedded in an Apache-2.0 project.
- Editors in this category are generally built around mouse drag-and-drop, which is a barrier for authors who use a keyboard, switch device, voice control, or screen reader. Accessible output is usually left to the author to assemble by hand.
- **Gap:** a permissively licensed, embeddable engine that records interaction data, is usable by authors with disabilities, and helps every author produce accessible video and training.

## 3. Target users and jobs-to-be-done

| User | Job |
|---|---|
| **OpenStage course authors** | Show a software procedure inside a course, with narration, captions, and a practice mode |
| **Instructional designers / L&D** | Turn a software walkthrough into an accessible module |
| **Educators / course creators** | Record a lecture or tutorial, trim it, add callouts, export captioned video |
| **Software support / docs teams** | Produce short how-to clips and step-by-step guides that stay easy to update when the UI changes |
| **Authors with disabilities** | Do all of the above with keyboard, screen reader, voice control, or switch access |
| **Learners using assistive technology** | Watch, practice, and be assessed with the same quality of experience as other learners |

## 4. Reference products — what we take and what we skip

| Capability | Camtasia | Captivate | Storyline | Our stance |
|---|---|---|---|---|
| Screen + audio + webcam capture | ✔ | ✔ | ✔ (step-by-step) | **MVP** |
| Editing | multi-track timeline | basic | basic | **MVP**: clip list with trim/split in OpenStage. Full multi-track timeline only if wanted (open question 1) |
| Editable cursor, click effects | ✔ | ✔ | partial | **MVP**, enabled by separate interaction data |
| Auto zoom/pan | ✔ (SmartFocus) | ✔ | — | **v1** |
| Callouts, annotations, shapes | ✔ | ✔ | ✔ | **v1**, reusing OpenStage's canvas where possible |
| Captions / transcript | ✔ | ✔ | ✔ | **MVP** from script; speech-to-text **v1** |
| Auto step-by-step slides from clicks | — | ✔ | ✔ | **v1** |
| Demo / Practice / Assessment modes | — | ✔ | ✔ | **v1** |
| SCORM / xAPI / HTML5 export | partial (quizzes) | ✔ | ✔ | **Reuse OpenStage's publisher.** OpenScreen supplies assets and step data (open question 2) |
| Audio description track | ? | ? | ? | **v1** *(competitor support not verified)* |
| Branching, variables, full authoring suite | — | ✔ | ✔ | **Non-goal**; that is OpenStage |
| Templates, asset library, themes | ✔ | ✔ | ✔ | Later |

## 5. Goals and non-goals

**Goals**
1. Reliable capture of screen (full / window / region), system audio, microphone, and webcam on Windows, with a documented capture-to-file path.
2. Record interaction events in a **documented, open project format** (versioned JSON + media files) that other tools can read and write.
3. Editing inside OpenStage's interface, fully operable by keyboard and screen reader, sufficient to finish a tutorial: trim, split, reorder, annotate.
4. Output as MP4/WebM video and as slide-per-step content that OpenStage's existing publisher can ship.
5. **Accessible by construction:** meet the targets in section 6 for both the editing experience and the output.
6. **Engine only.** No interface of its own; usable headless and through a documented, versioned process protocol.
7. 100% Apache-2.0-compatible dependency tree.

**Non-goals (for now)**
- A GUI of its own, or any native GUI toolkit.
- Matching Camtasia's effects/transitions catalog or Storyline's full interaction model.
- Live streaming or multi-camera production (OBS's territory).
- Cloud hosting, review/collab workflows, or an LMS.
- Mobile-device and browser-based capture.
- Building a speech-to-text engine into the MVP.

## 6. Accessibility requirements

Accessibility has two halves, and [ATAG 2.0](https://www.w3.org/TR/ATAG/) (W3C Authoring Tool Accessibility Guidelines) names both: **Part A** covers making the authoring tool itself accessible, and **Part B** covers making it support and promote accessible content. ATAG 2.0 references WCAG 2.0, so apply it with **WCAG 2.2 AA** as the success-criteria baseline.

**Targets:** WCAG 2.2 AA for the editing UI and all generated output; ATAG 2.0 Part A (OpenStage's existing bar) **plus Part B** for this module, because a recorder authors content; a published Accessibility Conformance Report (ACR/VPAT-style) with each release once real screen-reader testing exists.

### 6.1 The tool must be accessible (ATAG Part A)

The recorder and editor are OpenStage HTML, so they inherit OpenStage's accessibility rules and patterns: native controls, a polite live region announcing each action, dialogs that close with Escape and return focus, keyboard-driven canvas, and tables with captions and headers. **Those patterns are written down but have not yet been tested with a real screen reader** (see 6.3). Recording adds requirements of its own:

- **Recorder panel** uses native buttons with accessible names and states; no drag-only or hover-only operations; no keyboard traps.
- **Global hotkeys** to start/pause/stop, registered by OpenStage (Tauri has a global-shortcut plugin; verify), configurable, and working while another application has focus. They must **not collide with screen-reader modifier keys** (Insert and Caps Lock on NVDA, JAWS, and Narrator).
- **Feedback when OpenStage isn't focused.** Once recording starts, the user is in another application. OpenStage's live region may not be announced then (unverified), so the capture process plays distinct audio cues for start, pause, resume, stop, and error, and OpenStage shows the same state when it regains focus. Validate with NVDA and Narrator.
- **Recorder UI excluded from the capture** (for example, via Windows window display-affinity settings; verify with WebView2 windows) so controls never appear in the video.
- **Clip editor without a mouse.** A `<table>` clip list with caption and headers, numeric timecode inputs, and a keyboard model that mirrors the canvas (arrow keys move, Alt+arrows resize or trim, Page Up/Down select). Drag editing, if added, is a convenience on top.
- **Described preview.** A text readout of what is on screen at the playhead (window, cursor target, active callouts), so the visual result isn't verifiable only by sight.
- **Voice control compatibility.** Accessible names contain the visible label text (WCAG 2.5.3).
- **Respect OS settings:** forced-colors and high-contrast modes, text scaling and zoom, reduced motion, status never conveyed by color alone, minimum target sizes (WCAG 2.2 2.5.8).
- **Assistive technology is recordable.** Screen-reader speech (via system audio) and focus indicators must be captured, since authors often need to demonstrate AT workflows.

### 6.2 The output must be accessible, and the tool must make that the easy path (ATAG Part B)

- **Captions as a first-class track.** Timed text, language-tagged, edited alongside audio, exported as sidecar (WebVTT/SRT). **From the narration script by default**, which is how OpenStage works today; from speech-to-text for human-recorded audio in v1. Always human-editable and flagged "auto-generated, unreviewed" until approved.
- **Transcripts and step-by-step text** generated from the same data as captions.
- **Audio description track** (WCAG 1.2.3 / 1.2.5): written as text and voiced through the same script-to-speech route; support extended description (pausing video) in interactive output.
- **Alt text for non-text content** (callouts, shapes, images, hotspots), with a prompt rather than silent omission. Pre-fill from the Windows UI Automation tree where available, for example "Button: Save".
- **Motion and flashing safeguards.** Zooms, click effects, and transitions must not exceed three flashes per second (WCAG 2.3.1); respect reduced motion in the player.
- **Contrast and legibility checks** for callouts, captions, cursor highlights, and annotations (WCAG 1.4.3 / 1.4.11).
- **Use OpenStage's course player and publisher rather than building a second one.** Practice and Assessment modes use keyboard-operable hotspots and text-entry checks. **No click-only interactions.** Time limits adjustable or removable (WCAG 2.2.1). Correct language, headings, landmarks, and reading order.
  - **Known gap to fix first:** OpenStage's player uses a fixed-size stage that fails WCAG's reflow criterion (1.4.10). Recordings placed in a course inherit that until it is fixed.
- **Optional sign-language video track** as a first-class track type.
- **Accessibility checker.** A pre-export report that lists missing captions, missing alt text, contrast failures, flashing risk, and unlabeled hotspots, with jump-to-fix. Warnings are prominent and defaults favor accessibility, but the author is not blocked from exporting.

### 6.3 Accessibility in the process

- **First deliverable: a baseline screen-reader pass on OpenStage's existing editor patterns** (the ones the recorder will reuse) with NVDA and Narrator on Windows. JAWS if a license is available. VoiceOver when someone with a Mac can test. So far only scripted keyboard and structure checks have been run.
- Accessibility acceptance criteria on every user story; no feature merges without keyboard and screen-reader behavior defined.
- **Automated:** axe-core (or equivalent) on every generated HTML package in CI.
- **Manual:** scripted passes with real assistive technology on the core flows (record → trim → caption → insert into course) before each release.
- **User testing** with people who use assistive technology, from the first usable build.
- Publish an ACR with each release and keep a public accessibility issues list.

### 6.4 Compliance context

- **Written bar vs. this brief.** OpenStage's notes state the course bar as WCAG 2.2 AA in one place and 2.1 AA in another. This brief targets **2.2 AA**; it includes the 2.1 AA requirements apart from the obsolete 4.1.1 Parsing criterion. Reconcile the notes (open question 4).
- **US public institutions.** The DOJ's ADA Title II rule sets **WCAG 2.1 AA** for web content and mobile apps of state and local governments, which includes public universities. An April 2026 interim final rule moved the compliance dates to **April 26, 2027** (entities serving 50,000 or more people) and **April 26, 2028** (smaller entities and special districts). This comes from law-firm and university summaries ([Plante Moran](https://plantemoran.com/explore-our-thinking/insight/2026/04/doj-extends-ada-title-ii-web-accessibility-compliance-deadlines), [Duane Morris](https://www.duanemorris.com/alerts/doj_extends_ada_title_ii_digital_accessibility_deadlines_one_year_0426.html), [University of Arizona](https://accessibility.arizona.edu/news/doj-issues-interim-final-rule-title-ii-digital-accessibility-compliance-dates)), not the Federal Register, and the rule may have changed since. Whether it applies to a given institution is for that institution's accessibility office and counsel, not this brief.
- **Section 508 and EN 301 549** also cover authoring tools; per OpenStage's notes, ATAG Part A fits that. WCAG 2.2 AA covers the web-content levels of 508, EN 301 549, and AODA.
- **An ACR/VPAT requires real screen-reader testing** (6.3). Whether anyone needs one is an open question.

## 7. Licensing & dependency policy

The project is **Apache-2.0** and will be part of OpenStage, so every dependency must be one that downstream users can ship under Apache-2.0 without relicensing the whole work.

| Category | Policy | Examples |
|---|---|---|
| **Allowed** | Apache-2.0, MIT, BSD-2/3, ISC, zlib, Unlicense/CC0, dual "MIT OR Apache-2.0" (the Rust norm) | Tauri, windows-rs, Opus, libvpx, libaom, SVT-AV1, OpenH264, Kokoro-82M (code and weights Apache-2.0, per OpenStage's notes) |
| **Case-by-case** | MPL-2.0 (file-level copyleft; keep modifications isolated); LGPL **only** if dynamically linked, replaceable, and not required for core function | FFmpeg LGPL builds, GStreamer plugins |
| **Prohibited** | GPL, AGPL, SSPL, any LGPL static linking, "source-available" / non-commercial / proprietary-with-conditions licenses | FFmpeg `--enable-gpl` builds, x264, x265, OBS Studio code, `phonemizer-fork` (GPLv3+), espeak-ng (GPL) |

Practical consequences:

- **Do not bundle FFmpeg.** Prefer **OS-native encoders and capture APIs** (Media Foundation on Windows) and permissive codecs (OpenH264, VP9, AV1, Opus). If FFmpeg is ever optional, it must be a user-supplied, dynamically loaded, LGPL build, never a default dependency.
- **Codec patents are separate from copyright licenses.** Apache-2.0 grants patent rights only for the project's own contributions. H.264/AAC (and HEVC) carry third-party patent pools. Using the OS encoder or Cisco's pre-built OpenH264 binary shifts this to the platform vendor; VP9/AV1/Opus avoid it. **Needs legal review before shipping.**
- **Housekeeping:** SPDX header on every source file, a `NOTICE` file for third-party attributions, `cargo-deny` license checks in CI, and a decision on **DCO vs. CLA** (use OpenStage's policy if it has one).
- **Bundled assets** (fonts, icons, cursor sets, sample media, default caption fonts) meet the same bar. Many are CC-BY-SA or SIL OFL; check each.

### Licensing issue to settle before any installer ships

Per OpenStage's notes, the Python environment that runs Kokoro narration includes `phonemizer-fork` (GPLv3 or later) and the espeak-ng speech engine (also GPL). **Today that is fine:** OpenStage doesn't bundle them and talks to them as a separate program the user installed. **An installer that bundles that environment would redistribute GPL code.** OpenScreen should reuse the narration route for audio description and script-first captions, so it inherits the issue. Options to weigh:

1. Keep narration as an optional, user-installed external dependency, documented as such (the current arrangement).
2. Replace the GPL phonemization components with permissive ones. Evaluate candidates for license and quality in each supported language; verify before relying on any.
3. Ship it bundled as a separate program under GPL terms (source offer, notices) with legal sign-off.

This is a decision for OpenStage's owner with legal advice, not something this brief can settle. The separate-process design keeps GPL code out of OpenScreen's own binary and source, but it does not remove the redistribution obligations of anything bundled alongside.

## 8. Scope by milestone

Windows first. Accessibility work is spread across every milestone rather than saved for the end, because the hardest parts (the process protocol's audio cues, the clip editor's keyboard model, and the project-format tracks) are architectural.

**MVP (v0.1) — "record, trim, drop into a course"**
- Capture on Windows: screen/window/region + mic + system audio + webcam
- Interaction logging: cursor position, clicks, key events, active window; opt-in with a clear on-screen and audible indicator
- **Process protocol v0**: `record`, `render`, `probe`, `recover`; JSON-lines events; graceful stop; crash-safe output (see section 9)
- **Project format v0** (documented) with caption, transcript, alt-text, and language fields from day one
- **OpenStage integration:** recorder panel in native HTML, global hotkeys, audible state cues, clip list with trim/split, insert as a video asset in a slide with its caption track
- **Script-first narration and captions** through OpenStage's existing narration route; WebVTT/SRT import and export
- Cursor re-styling and click highlights at render time (with flash safeguards)
- Export MP4 (Media Foundation) and WebM (VP9/Opus)
- Baseline screen-reader pass on the reused OpenStage patterns (6.3)

**v1 — "teach with it"**
- Auto step segmentation → slide-per-step content in OpenStage's slide model, with step text and alt text pre-filled from UI Automation
- Auto zoom/pan; callouts, text, arrows, blur/redact
- Demo / Practice / Assessment output with keyboard-operable hotspots and text-entry checks, through OpenStage's player and publisher
- **Local speech-to-text** for human-recorded audio (model and library audited for license first)
- Audio description track (script → speech)
- Accessibility checker pane
- macOS (needs a tester and hardware), then Linux (PipeWire + xdg-desktop-portal)
- Multi-track timeline, if decided

**Later**
- Sign-language track, accessible PDF/HTML step guides, templates and themes, noise removal, localization, plugin API.

## 9. Architecture and technical considerations

### 9.1 Process model and protocol *(proposal, to fit OpenStage's existing conventions)*

OpenStage already launches `publish.ts`, `narrate.py`, and `validate.py` as separate processes: one job at a time, JSON back on standard output, and a cancel that stops the whole process tree. The capture process follows the same pattern, with two differences because a recording is long-running and interactive, not a batch job:

- **Events stream as JSON lines on stdout**: a first line with the protocol version, then `state` (idle/countdown/recording/paused/stopped), elapsed time, throttled audio levels, warnings, errors, and a final `result` with the project path. Logs go to stderr.
- **Control comes in as JSON lines on stdin**: `pause`, `resume`, `stop`, `marker`. **`stop` is graceful** (finalize the file); **cancel is the existing kill-the-tree** behavior. Because cancel is abrupt, **recording must be crash-safe**: write in segments (for example fragmented MP4) with an append-only `events.jsonl`, flush regularly, and offer `recover` to repair an interrupted project. A killed recording should lose seconds, not the take.
- **Why a separate process:** it holds its own OS permission prompts (screen capture, microphone, accessibility), a crash can't take the editor down, and anything with a stricter license stays out of OpenStage's own code.
- **Protocol as contract.** OpenStage's `CLAUDE.md` describes plugin contracts that are intended but not built. This protocol, versioned and documented, could become that contract.
- **Launch mechanism:** Tauri can bundle and launch external binaries ("sidecars"); check whether that fits how OpenStage launches its other tools.

### 9.2 Capture on Windows (first target)

- Windows Graphics Capture for screen/window; WASAPI loopback for system audio, WASAPI for the microphone
- UI Automation for the element under each click (name and role); optional, best-effort, permission-aware, and **opt-in per recording** because it can expose on-screen text
- Media Foundation for H.264/AAC encoding, with VP9/Opus as the portable fallback
- macOS: ScreenCaptureKit (needs Screen Recording and Accessibility permissions). Linux: PipeWire via xdg-desktop-portal; Wayland restricts global input capture, so interaction logging there may be limited

### 9.3 Project format

A folder or zip of `project.json` (versioned schema) + media segments + `events.jsonl`, with caption, audio-description, transcript, alt-text, and language tracks defined in the schema from v0. No proprietary container.

**Proposed crate layout** (to align with OpenStage's workspace conventions): `openscreen-capture` · `openscreen-project` · `openscreen-render` · `openscreen-protocol` · `openscreen-a11y` (caption/alt-text model, checker) · `openscreen-cli`

### 9.4 Hard problems to design early

1. A/V sync across screen, system audio, mic, and webcam clocks
2. Variable-frame-rate screen capture vs. constant-rate editing
3. Crash-safe, recoverable recording (9.1)
4. Long (60+ minute) recordings
5. Privacy: password fields, notifications, and sensitive windows (exclusion/redaction at capture time)
6. Hotkeys that never collide with assistive technology (6.1)
7. Audible, reliable state feedback while another application has focus (6.1)

## 10. Success criteria

- A first-time user records a short tutorial and drops it, captioned, into a course in under 10 minutes.
- **A user can complete record → trim → caption → insert with the keyboard alone, and separately with a screen reader alone (NVDA and Narrator), with no critical blockers found in manual testing on Windows.**
- **A recording interrupted by a killed process recovers all but the last few seconds.**
- **Zero serious or critical axe findings on generated HTML**, plus a passing manual keyboard and screen-reader review of the player (after the reflow gap is fixed).
- Capture overhead stays below ~10% CPU on a mid-range laptop at 1080p30.
- A/V drift under 40 ms over a 60-minute recording.
- The dependency license check passes in CI on every commit.
- An ACR is published once real screen-reader testing exists.

## 11. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| **No real screen-reader testing of OpenStage exists yet** | The patterns this module reuses may not work as intended | Baseline NVDA/Narrator pass before building on them (6.3) |
| Cancel kills the process tree mid-recording | Lost or corrupt recordings | Crash-safe segments, append-only events, `recover` command |
| Global hotkeys collide with screen-reader keys | Authors can't start or stop recording | Configurable hotkeys; test against NVDA, JAWS, Narrator defaults |
| Live-region announcements may not fire while another app has focus | Screen-reader users don't know recording state | Audio cues from the capture process; verify with real AT |
| Player fails reflow (1.4.10) | Recordings inherit a WCAG failure | Fix in OpenStage before claiming 2.2 AA for recording-bearing courses |
| GPL components in the narration environment | Redistribution obligations if bundled in an installer | Decide before any installer ships (section 7) |
| Windows-only hardware | macOS/Linux and VoiceOver unverified | Windows first; recruit a Mac tester before promising macOS |
| Permissive-only encoder options are thinner than FFmpeg's | Output format/quality limits | OS encoders; VP9/AV1 as the portable fallback |
| Codec patent exposure (H.264/AAC) | Legal | Legal review; default to OS encoders / OpenH264 |
| No speech-to-text yet | Human-recorded audio can't be auto-captioned | Script-first captions in MVP; audited local STT in v1 |
| Auto-generated captions and alt text are wrong | Inaccessible content that looks accessible | Always editable; flagged unreviewed until approved |
| Scope creep toward a full authoring suite or video editor | Never ships | Hold the non-goals line; OpenStage is the authoring suite |

## 12. Open questions

*Resolved since v0.2: OpenStage's stack and UI approach, integration shape (separate process), local-and-permissive speech, and the recorded accessibility bar.*

1. **Editing model.** Where do recordings land: (a) a video asset on a slide, (b) slide-per-step simulation content, or (c) a standalone project with its own multi-track timeline? *Recommendation:* (a) and (b) for MVP and v1, reusing OpenStage's accessible canvas; defer (c), since an accessible multi-track timeline is the riskiest piece. Does "like Camtasia" mean the timeline matters to you, or the Captivate/Storyline step-recording model?
2. **Publisher.** Which formats does `publish.ts` produce today (HTML5, SCORM, xAPI)? OpenScreen should supply assets and step data to it rather than duplicate it.
3. **GPL narration environment.** How will narration ship in an installer? (Section 7; needed before any installer.)
4. **Which written bar is canonical?** OpenStage's notes say WCAG 2.2 AA and WCAG 2.1 AA in different places. *Recommendation:* 2.2 AA, plus ATAG Part B for this module.
5. **Institutional or legal requirement?** Only you can settle this. Does an accessibility office, grant, or customer require WCAG 2.1 AA under ADA Title II, a VPAT, or something else?
6. **Test access.** Is a Mac and VoiceOver tester available, and is a JAWS license?
7. **Speech-to-text.** Needed once authors record their own voice. Which local, permissive model and library, after an audit?
8. **Codec position.** Comfortable relying on OS-provided H.264/AAC, or royalty-free codecs only?
9. **Contribution model.** DCO or CLA, or OpenStage's existing policy?
10. **Name.** See below.

## 13. Naming

"OpenScreen" works as a working title, but it has a collision: the W3C Second Screen Working Group publishes the **Open Screen Protocol**, so searches and docs will be noisy. (I believe Chromium's implementation of it also lives in a repository named `openscreen`; I did not verify that.) The crate names `openscreen`, `open-screen`, and `openstage` were all unregistered on crates.io when checked.

*Recommendation:* since this ships inside OpenStage, prefer an in-family name such as **OpenStage Capture** or **OpenStage Screen** (crate prefix `openstage-`), which avoids the collision and makes the relationship obvious. Keep "OpenScreen" as the working title until you decide. Do a trademark and search check before the name goes public.

## 14. Proposed next steps

1. **Baseline screen-reader pass** on OpenStage's existing editor patterns (NVDA and Narrator). It gates the whole approach.
2. **Write the process protocol v0** as a one-page spec: JSON-lines events, stdin control, graceful stop vs. kill, crash-safe segments, `recover`.
3. **Windows capture spike:** screen + mic + system audio + cursor events + UI Automation element info, written to the draft project format; then kill the process mid-recording and recover.
4. **Render spike:** that project to MP4 (Media Foundation) and WebM with only allowed dependencies; confirm `cargo-deny` passes.
5. **Decide the narration-environment plan** before any installer ships.
6. Answer open questions 1, 2, and 5.
7. Convert this brief into a PRD with user stories, each carrying accessibility acceptance criteria.
