# OpenStage Capture: Product Brief

*(formerly the working title "OpenScreen"; see [Naming](#13-naming))*

**Status:** v0.17, checked against the OpenStage Author repository; requirements drafted in [prd.md](prd.md), with open items in section 12
**License:** Apache-2.0 (hard constraint; see [Licensing & dependency policy](#7-licensing--dependency-policy))
**Context:** A module of **OpenStage Author** ("OpenStage" for short), a Tauri v2 desktop app (Rust shell, plain TypeScript + HTML interface) at [github.com/jasonswrench-beep/openstage-author](https://github.com/jasonswrench-beep/openstage-author). Delivered as a **separate capture process** that OpenStage's interface drives.
**Accessibility:** First-class requirement, for the tool *and* for everything it produces (see [section 6](#6-accessibility-requirements))

> **Provenance note.** Items marked *(confirmed)* were read from the OpenStage Author repository (`jasonswrench-beep/openstage-author`, commit 23d53e5 of 2026-10-12, read-only) by the author of this brief, with file references. Items marked *(relayed)* come from OpenStage's scoping document, capability matrix, or notes as reported to this brief; those documents are not in the repository and have not been seen. Where this brief and the repository's own rules differ, section 12 says so and asks.

> **Plain-language guide.** This brief uses a few technical terms; the [glossary](#15-plain-language-glossary) at the end explains them all.
>
> **The plan is one big first release, called Release 1,** containing everything major, rather than a small first version followed by a bigger one. (The industry would call a small first version an "MVP"; that is not the plan here.) A short list of things is deliberately left until *after* Release 1; see [section 8](#8-scope-one-big-first-release).
>
> **Most decisions are now made** and recorded in the table below. The few items still open are in [section 12](#12-open-questions).

## Decisions so far

| Decision | Value |
|---|---|
| License | Apache-2.0 |
| Host project | **OpenStage Author** ("OpenStage" for short): Tauri v2, Rust shell, TypeScript + plain HTML UI (WebView2 on Windows), no UI framework; Apache-2.0 *(confirmed)* |
| Engine language | Rust |
| Process model | Separate process with **no interface of its own**. OpenStage's UI and global hotkeys drive it; JSON on stdout; the same launch pattern OpenStage already uses for `publish.ts`, `narrate.py`, and `validate.py`, extended so a live recording can be controlled (9.1) |
| Interface | Native HTML controls inside OpenStage. No separate GUI toolkit |
| Platforms | **Windows 11 and later only for Release 1.** macOS and JAWS testing only if a Mac user and a JAWS license become available later; Linux is not planned |
| Editing | **A mid-level video editor** (owner: basic video editing, text overlays, and zooming into parts, like Camtasia), with a **Camtasia-style multi-track timeline, in Release 1**. Images, audio, and short video clips from outside the recording are accepted (owner: option B). Not a full or professional editor; section 5 lists what is in and out |
| Publishing | **One publisher only: OpenStage's.** Any capture path hands assets to it; OpenStage Capture never ships its own exporter. Release 1 targets **SCORM 1.2 only**. Today the publisher produces a folder of plain web files and a SCORM 1.2 zip; SCORM 2004 and xAPI are on OpenStage's roadmap as later *(confirmed)* |
| Narration | Text-first. Audio generation is **download on first use, never bundled** in the installer. Nothing breaks if it is never downloaded |
| Speech | Local only, permissive licenses only |
| Accessibility bar | WCAG 2.2 AA for output; ATAG 2.0 Part A for the editor (this brief proposes adding Part B). **No conformance report yet; publish an accessibility statement.** Reflow (1.4.10) in OpenStage's player is a known gap today and **will be fixed before Release 1 ships** (owner decision), see 6.2 |
| Name | **OpenStage Capture** (confirmed by owner; crate prefix `openstage-capture`). Replaces the working title "OpenScreen" |
| Release plan | **One big first release (Release 1)** with everything major; section 8 says what is in and out |
| Prerequisite | **Responsive output in OpenStage's player is fixed first** (owner: "ideally, fix responsive output first"). Release 1 does not ship until it is done. It is OpenStage Author work, outside this module (section 8) |
| Rule exceptions | **Accepted (owner)** for the Capture module: Windows first, a mid-level video editor, and a Rust capture program. To be recorded in OpenStage's CLAUDE.md by the owner or the OpenStage session (section 12, item 7) |
| Video output | **MP4** (H.264 video, AAC audio) using Windows's built-in encoders. WebM is not in Release 1. A legal check on patents is still advised before shipping |
| Narration helper tools (GPL) | **Option 1, for now:** they stay separate and the user installs them; OpenStage never includes or hosts them |
| Code from other projects | **None** copied from the existing OpenScreen project (owner: cleaner). Rules for any future outside code stay in section 7 |
| Compliance deadline | **None** at present (owner). The accessibility targets are quality goals, not deadlines |
| Contribution rules | **None** for now; development is by the owner and collaborators only. Revisit before accepting outside contributors |

---

## 1. Summary

An open-source, accessible screen recorder for people who make **instructional video and software training**. It covers the ground Camtasia holds for video and Adobe Captivate / Articulate Storyline hold for e-learning software simulations. The capture engine is a permissively licensed Rust process; the recorder and editing experience live inside OpenStage's accessible HTML interface.

Three ideas drive the design:

1. **Record the screen and the user's interactions as separate, editable data.** Video plus cursor, clicks, keystrokes, active window, and (where the OS allows) the UI element that was clicked, rather than flattened pixels. That enables re-styled cursors, automatic zoom-and-pan, auto-generated step text, and conversion of a recording into an interactive "show me / let me try / test me" simulation.
2. **Accessibility is built into the data model, not bolted on.** Captions, transcripts, audio description, alt text, and language are first-class tracks in the project format. The same UI-element data that powers step captions also powers alt text and accessible step-by-step guides.
3. **Script-first narration.** OpenStage already turns a narration script into speech and captions locally, and stores narration as text. A recording can use the same route: write the script, record the demo, and get captions from the script with no speech-to-text. Speech-to-text is only needed when the author records their own voice.

## 2. Problem and opportunity

- Camtasia is excellent for video but closed, paid, and not embeddable.
- Captivate and Storyline do software simulations well but are heavy authoring suites; screen capture is a means to an end.
- Open-source recorders capture well but typically lack accessible editing, interaction data, and e-learning output. The most capable ones are GPL, so they cannot be embedded in an Apache-2.0 project.
- Timeline editors in this category are generally built around mouse drag-and-drop, which is a barrier for authors who use a keyboard, switch device, voice control, or screen reader. Accessible output is usually left to the author to assemble by hand.
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

## 4. Reference products: what we take and what we skip

| Capability | Camtasia | Captivate | Storyline | Our stance |
|---|---|---|---|---|
| Screen + audio + webcam capture | ✔ | ✔ | ✔ (step-by-step) | **Release 1** |
| Editing | multi-track timeline | basic | basic | **Release 1: a mid-level editor with a Camtasia-style multi-track timeline**, built on an accessible track/clip model with a keyboard-operable table view as its foundation |
| Editable cursor, click effects | ✔ | ✔ | partial | **Release 1**, enabled by separate interaction data |
| Auto zoom/pan | ✔ (SmartFocus) | ✔ | no | **Release 1** |
| Callouts, annotations, shapes | ✔ | ✔ | ✔ | **Release 1.** In slide-per-step content they are ordinary OpenStage objects (real text, timed with appear/disappear). Inside a video they are rendered into the MP4, with text equivalents alongside (9.3) |
| Captions / transcript | ✔ | ✔ | ✔ | **Release 1**: from the narration script, and from speech-to-text for human-recorded audio (fallback rule in section 8) |
| Auto step-by-step slides from clicks | no | ✔ | ✔ | **Release 1** |
| Demo / Practice / Assessment modes | no | ✔ | ✔ | **Release 1** |
| SCORM / xAPI export | partial (quizzes) | ✔ | ✔ | **One publisher (OpenStage's).** Release 1: SCORM 1.2 only. SCORM 2004 and xAPI: after Release 1 |
| Audio description track | ? | ? | ? | **Release 1** *(competitor support not verified)* |
| Branching, variables, full authoring suite | no | ✔ | ✔ | **Non-goal**; that is OpenStage |
| Templates, asset library, themes | ✔ | ✔ | ✔ | Later |

## 5. Goals and non-goals

**Goals**
1. Reliable capture of screen (full / window / region), system audio, microphone, and webcam on Windows, with a documented capture-to-file path.
2. Record interaction events in a **documented, open project format** (versioned JSON + media files) that other tools can read and write.
3. Editing inside OpenStage's interface, fully operable by keyboard and screen reader, sufficient to finish a tutorial: trim, split, reorder, annotate, on a Camtasia-style multi-track timeline.
4. Output as MP4 video and as slide-per-step content. **There is exactly one publisher (OpenStage's).** Any capture path hands assets to it, so editor preview and published output cannot drift apart.
5. **Accessible by construction:** meet the targets in section 6 for both the editing experience and the output.
6. **Engine only.** No interface of its own; usable headless and through a documented, versioned process protocol.
7. 100% Apache-2.0-compatible dependency tree.

**Non-goals (for now)**
- A GUI of its own, or any native GUI toolkit.
- **A second exporter**, or any SCORM/xAPI writer outside OpenStage's publisher.
- Matching Camtasia's effects/transitions catalog or Storyline's full interaction model.
- Live streaming or multi-camera production (OBS's territory).
- Cloud hosting, review/collab workflows, or an LMS.
- Mobile-device and browser-based capture.
- **A professional-grade video editor.** The editor is mid-level, as described in "What 'mid-level video editor' means" below. Color correction, green screen, multi-camera editing, a large effects and transitions catalog, motion graphics, and advanced audio mixing are not planned.
- Building a speech-recognition engine from scratch (an existing local, permissively licensed one will be chosen instead).

### What "mid-level video editor" means

The owner's words: "a middle-video editor. Some basic video editing and text overlays and the ability to zoom into parts. Again, like Camtasia." In plain terms, that is more than a trimmer and well short of a professional editing suite.

**Included in Release 1**
- **Cutting:** trim, split, delete, join, and reorder clips, all non-destructive and undoable.
- **Multi-track timeline:** screen, audio, webcam, captions, and overlays on separate tracks.
- **Text overlays:** titles, labels, lower thirds, and callouts, with real text equivalents and the required alt text fields (6.2).
- **Zoom and pan into parts of the screen:** set by hand, or suggested automatically from where the user clicked.
- **Cursor effects:** restyle the cursor, highlight it, and show clicks (with flash safeguards).
- **Shapes, arrows, and blur or redact regions.**
- **Audio basics:** volume, mute, and fade in and out.
- **Captions:** edited on their own track alongside the audio.
- **Media from outside the recording:** images, audio (such as music or a voice-over), and short video clips on the timeline, the way Camtasia's media bin works.

**Not planned for now,** so the scope stays mid-level: color correction and filters, green screen, multi-camera editing, a large effects and transitions catalog, motion graphics and animation curves beyond zoom and pan, and advanced audio mixing or noise removal. Small additions such as playback speed changes, freeze frames, or simple fades between clips can be added later if wanted.

**Decided (owner): media from outside the recording is accepted** (option B). Imports should come from the course's existing asset library where possible, which already records each file's source, creator, and licence and feeds the credits page *(confirmed)*. OpenStage's validator rules then apply to imported media *(confirmed, `validator-spec.md` and `tools/validate.py`)*: an image needs alt text or must be marked decorative; a video needs a caption file or a narration script; and an audio clip needs captions, a script, or a transcript. That last rule applies to any audio object, so wordless background music still needs a transcript, and a short description of the music should satisfy it (to be confirmed when built).

## 6. Accessibility requirements

Accessibility has two halves, and [ATAG 2.0](https://www.w3.org/TR/ATAG/) (W3C Authoring Tool Accessibility Guidelines) names both: **Part A** covers making the authoring tool itself accessible, and **Part B** covers making it support and promote accessible content. ATAG 2.0 references WCAG 2.0, so apply it with **WCAG 2.2 AA** as the success-criteria baseline.

**Targets:** WCAG 2.2 AA for the editing UI and all generated output (**reflow, 1.4.10, in OpenStage's player is a known gap today and is to be fixed before Release 1 ships; see 6.2**); ATAG 2.0 Part A (OpenStage's existing bar) **plus Part B** for this module, because a recorder authors content. **An accessibility statement now; an Accessibility Conformance Report (ACR/VPAT-style) only after real screen-reader testing exists.**

### 6.1 The tool must be accessible (ATAG Part A)

The recorder and editor are OpenStage HTML, so they inherit OpenStage's accessibility rules and patterns: native controls, a polite live region announcing each action, dialogs that close with Escape and return focus, keyboard-driven canvas, and tables with captions and headers. **Those patterns are written down but have not yet been tested with a real screen reader** (see 6.3). Recording adds requirements of its own:

- **Recorder panel** uses native buttons with accessible names and states; no drag-only or hover-only operations; no keyboard traps.
- **Global hotkeys** to start/pause/stop, registered by OpenStage, configurable, and working while another application has focus. They must **not collide with screen-reader modifier keys** (Insert and Caps Lock on NVDA, JAWS, and Narrator). OpenStage Author does not do this yet: its window permissions allow only dialogs, opening links, and setting or closing the window (`capabilities/default.json`), so a global-shortcut capability would have to be added *(confirmed)*.
- **Feedback when OpenStage isn't focused.** Once recording starts, the user is in another application. OpenStage's live region may not be announced then (unverified), so the capture process plays distinct audio cues for start, pause, resume, stop, and error, and OpenStage shows the same state when it regains focus. Validate with NVDA and Narrator.
- **Recorder UI excluded from the capture** (for example, via Windows window display-affinity settings; verify with WebView2 windows) so controls never appear in the video.
- **Timeline without a mouse.** The multi-track timeline is the hardest custom widget in the product and the riskiest commitment under ATAG Part A, **but OpenStage Author has already built the pattern to extend.** Its Timeline panel draws each object as a row with a bar. Each row has two grips that are real ARIA sliders (Left/Right Arrow move a tenth of a second, Shift a second, Page Up/Down five, Home/End to the ends), announced in words. Bars can also be dragged, Escape undoes a drag, and a Timing list with fields makes exactly the same changes, so nothing is mouse-only. The panel's arithmetic lives in a separately tested model (`timelinemodel.ts`) and the panel only draws it (`timelinepanel.ts`) *(confirmed)*. The multi-track timeline should extend that same pattern: tracks and clips as rows, the same keyboard grammar, and a fields-based list making the same changes. Like the rest of the editor it has **not yet been checked with a real screen reader** (6.3). The visual timeline does not ship until its keyboard model is complete and screen-reader-verified.
- **Described preview.** A text readout of what is on screen at the playhead (window, cursor target, active callouts), so the visual result isn't verifiable only by sight.
- **Voice control compatibility.** Accessible names contain the visible label text (WCAG 2.5.3).
- **Respect OS settings:** forced-colors and high-contrast modes, text scaling and zoom, reduced motion, status never conveyed by color alone, minimum target sizes (WCAG 2.2 2.5.8).
- **Assistive technology is recordable.** Screen-reader speech (via system audio) and focus indicators must be captured, since authors often need to demonstrate AT workflows.

### 6.2 The output must be accessible, and the tool must make that the easy path (ATAG Part B)

- **Captions as a first-class track.** Timed text, language-tagged, edited alongside audio, exported as sidecar (WebVTT/SRT). **From the narration script by default**: OpenStage's rules say captions and transcripts for narrated objects are derived from the script, and speech-to-text is never run on narration audio when a script exists *(confirmed, CLAUDE.md)*. Speech-to-text applies only to human-recorded audio with no script, through OpenStage's planned "captions from media" plugin contract (see the fallback rule in section 8). The roadmap also lists "Whisper for imported media" under later work, and any such model needs the same license audit. Always human-editable and flagged "auto-generated, unreviewed" until approved.
- **Transcripts and step-by-step text** generated from the same data as captions.
- **Follow OpenStage's schema rules for everything Capture adds.** Every object carries `altText`, `role`, `tabOrder`, and `decorative`, never optional and never filled silently with empty defaults *(confirmed, CLAUDE.md)*. Auto-generated alt text is allowed because it is not empty, but it is flagged unreviewed.
- **Text the learner reads needs an audio route.** OpenStage's rule: if it is text the learner is meant to read, audio should be available for it. Step text, callout text, and captions that Capture introduces must be narratable from a script, and the editor should make that easy; say so wherever a new kind of text has no audio route yet *(confirmed, CLAUDE.md)*.
- **A video object must have a caption file or a narration script.** The format and validator already require it (`format-spec.md`, `schema/object.schema.json`), so a recording cannot be inserted without one *(confirmed)*.
- **Audio description track** (WCAG 1.2.3 / 1.2.5): written as text and voiced through the same script-to-speech route; support extended description (pausing video) in interactive output. Delivery to learners is a pop-up that opens when the learner asks for it, where they can read the description, listen to it, or play the video with the description voiced in time (owner decisions; see the PRD, story D5).
- **Alt text for non-text content** (callouts, shapes, images, hotspots), with a prompt rather than silent omission. Pre-fill from the Windows UI Automation tree where available, for example "Button: Save".
- **Motion and flashing safeguards.** Zooms, click effects, and transitions must not exceed three flashes per second (WCAG 2.3.1); respect reduced motion in the player.
- **Contrast and legibility checks** for callouts, captions, cursor highlights, and annotations (WCAG 1.4.3 / 1.4.11).
- **Use OpenStage's course player and publisher; never build a second one.** Practice and Assessment modes use keyboard-operable hotspots and text-entry checks. **No click-only interactions.** Time limits adjustable or removable (WCAG 2.2.1). Correct language, headings, landmarks, and reading order.
  - **Known gap, owned by OpenStage, to be fixed first:** the player scales the whole stage down to fit narrow windows (and never above 100%), but it cannot reflow. On a phone the stage is a shrunken picture of a desktop slide, with tiny text and no re-wrapping at large text sizes or high zoom. That fails WCAG 2.2 AA 1.4.10 (Reflow). OpenStage's own specification lists it as a known limit, and "responsive output" is on its roadmap as later work *(confirmed)*. **Decided (owner): fix it first.** Release 1 does not ship until responsive output exists (see Prerequisites in section 8). Until then, the accessibility statement lists the gap plainly, and recordings placed in a course inherit it. WCAG excepts parts of content that need a two-dimensional layout to be usable or meaningful, and some explainers list video and presentations as examples, but whether that covers OpenStage's stage is a judgment for an accessibility specialist; the owner chose to fix the gap rather than rely on the exception.
- **Optional sign-language video track** as a first-class track type.
- **Accessibility checker.** A pre-export report that lists missing captions, missing alt text, contrast failures, flashing risk, and unlabeled hotspots, with jump-to-fix. Warnings are prominent and defaults favor accessibility, but the author is not blocked from exporting.

### 6.3 Accessibility in the process

- **First deliverable: a baseline screen-reader pass on OpenStage's existing editor patterns** (the ones the recorder will reuse) with NVDA and Narrator on Windows. JAWS and VoiceOver are out of scope for now (no JAWS license or Mac tester yet), and the accessibility statement will say so. So far only scripted keyboard and structure checks have been run.
- Accessibility acceptance criteria on every user story; no feature merges without keyboard and screen-reader behavior defined. OpenStage's release gate already requires keyboard-only operation of the whole editor and published course, and NVDA and VoiceOver verification *(confirmed, CLAUDE.md)*; the ATAG Part A pass "including NVDA" is still unchecked on its roadmap. Keep the test evidence, because it is the raw material for a real ACR later. VoiceOver verification waits until a Mac tester exists.
- **Automated:** axe-core (or equivalent) on every generated HTML package in CI.
- **Manual:** scripted passes with real assistive technology on the core flows (record → trim → caption → insert into course) before each release.
- **User testing** with people who use assistive technology, from the first usable build.
- **Publish an accessibility statement now, not a conformance report.** It states what is targeted, what has actually been tested, what is known to be incomplete, and how to report a problem. Update it as testing happens. Keep a public accessibility issues list.

### 6.4 Compliance context

- **No compliance deadline today (owner).** The targets in this section are quality goals, not deadlines. The context below is kept for institutions that may adopt the tool later.
- **Written bar.** OpenStage Author's rules state WCAG 2.2 AA for published output and ATAG Part A for the editor *(confirmed, CLAUDE.md)*; "2.1" appears nowhere in the repository. This brief targets **2.2 AA** and proposes adding ATAG Part B for this module. WCAG 2.2 AA includes the 2.1 AA requirements apart from the obsolete 4.1.1 Parsing criterion.
- **Two separate claims; do not conflate them.** **WCAG 2.2 AA** is the claim about the published course, what learners experience, and most vendors document it. **ATAG 2.0 Part A** is the claim about the authoring tool, what authors experience, and it is much less commonly documented. The ATAG claim is the one worth documenting properly over time.
- **No VPAT/ACR yet.** A conformance report written without testing is a false claim that institutions will rely on when adopting the tool. Publish the accessibility statement (6.3) and build the evidence as the acceptance tests run.
- **When an ACR will be asked for.** A free tool used by an individual instructor rarely reaches procurement. Expect a request once a department, campus, or system adopts or recommends it institutionally, and treat it as near-certain if the project takes grant or institutional funding, because documentation obligations can arrive attached to the money. This is a landscape, not legal advice.
- **For reference, US public institutions.** The DOJ's ADA Title II rule sets **WCAG 2.1 AA** for web content and mobile apps of state and local governments, which includes public universities. An April 2026 interim final rule moved the compliance dates to **April 26, 2027** (entities serving 50,000 or more people) and **April 26, 2028** (smaller entities and special districts). This comes from law-firm and university summaries ([Plante Moran](https://plantemoran.com/explore-our-thinking/insight/2026/04/doj-extends-ada-title-ii-web-accessibility-compliance-deadlines), [Duane Morris](https://www.duanemorris.com/alerts/doj_extends_ada_title_ii_digital_accessibility_deadlines_one_year_0426.html), [University of Arizona](https://accessibility.arizona.edu/news/doj-issues-interim-final-rule-title-ii-digital-accessibility-compliance-dates)), not the Federal Register, and the rule may have changed since. Because 2.2 AA is a superset, meeting this brief's target meets that standard. Which date applies to a given institution is for its accessibility officer or ADA coordinator, not this brief.
- **Section 508 and EN 301 549** also cover authoring tools; per OpenStage's notes, ATAG Part A fits that. WCAG 2.2 AA covers the web-content levels of 508, EN 301 549, and AODA.

## 7. Licensing & dependency policy

The project is **Apache-2.0** and will be part of OpenStage, so every dependency must be one that downstream users can ship under Apache-2.0 without relicensing the whole work.

| Category | Policy | Examples |
|---|---|---|
| **Allowed** | Apache-2.0, MIT, BSD-2/3, ISC, zlib, Unlicense/CC0, dual "MIT OR Apache-2.0" (the Rust norm) | Tauri, windows-rs, Opus, libvpx, libaom, SVT-AV1, OpenH264, Kokoro-82M (code and weights Apache-2.0) |
| **Case-by-case** | MPL-2.0 (file-level copyleft; keep modifications isolated); LGPL **only** if dynamically linked, replaceable, and not required for core function | FFmpeg LGPL builds, GStreamer plugins |
| **Prohibited** | GPL, AGPL, SSPL, any LGPL static linking, "source-available" / non-commercial / proprietary-with-conditions licenses | FFmpeg `--enable-gpl` builds, x264, x265, OBS Studio code, `phonemizer-fork` (GPLv3+), espeak-ng (GPL) |

Practical consequences:

- **Do not bundle FFmpeg.** Release 1 produces MP4 (H.264 video, AAC audio) with **Windows's built-in encoders** (Media Foundation), so FFmpeg is not needed. If FFmpeg is ever optional, it must be a user-supplied, dynamically loaded, LGPL build, never a default dependency.
- **FFmpeg guard before any integration.** `cargo-deny` does not see native libraries. Add a CI check that fails the build if any FFmpeg build configuration contains `--enable-gpl` or `--enable-nonfree`, and flags `--enable-version3` for review, **before** any code or build configuration from another project is brought in. A GPL FFmpeg build entering through an imported build configuration would silently relicense the combined work.
- **Reusing code from other projects, even permissively licensed ones** (the owner has decided that no code is copied from the existing OpenScreen project): attribution in `NOTICE`, a license check on the specific files, and no imported build configuration until it has been audited. Never copy build configurations wholesale.
- **Codec patents are separate from copyright licenses.** Apache-2.0 grants patent rights only for the project's own contributions. H.264/AAC (and HEVC) carry third-party patent pools. Using Windows's built-in encoder shifts this to Microsoft; patent-free formats (VP9, AV1, Opus) avoid it. **The owner has chosen MP4 (H.264/AAC) as the output; a legal review is still advised before shipping.** Also check early that the Windows editions you support include the media components (the "N" editions of Windows may need Microsoft's Media Feature Pack; verify).
- **Housekeeping:** SPDX header on every source file, a `NOTICE` file for third-party attributions, `cargo-deny` license checks in CI, and no contribution agreement while development is limited to the owner and collaborators (Apache-2.0 already says contributions submitted to the project come under the same license unless stated otherwise; set rules before accepting outside contributors).
- **Bundled assets** (fonts, icons, cursor sets, sample media, default caption fonts) meet the same bar. Many are CC-BY-SA or SIL OFL; check each.

### Narration delivery *(decided; relayed)*

**Narration never ships in the installer.** Installer size matters for a tool people try casually, and the narration model (about 340 MB per OpenStage's notes; not independently verified) would roughly triple the download for a feature many users will never touch. Instead:

- **Download on first use**, never bundled.
- **Mirror the weights on a release asset of our own** rather than fetching from a third-party host at runtime. Apache-2.0 permits redistribution with the license and attribution carried along, and it removes dependence on someone else's uptime and rate limits.
- **Verify a SHA-256 hash before loading.** An unverified multi-hundred-megabyte blob that then runs inference is not acceptable.
- **Cache in the per-user application data directory**, never in Program Files: no administrator rights, and it survives app updates.
- **Document a manual placement path** (download the file separately, drop it in a named folder). Managed institutional machines frequently block application downloads, and higher education is full of them.
- **Show the Apache-2.0 license and the training-data disclosure at the download prompt.** Kokoro's model card says it was trained on public-domain audio, permissively licensed audio, and **synthetic audio from closed third-party TTS models**, and it names two CC BY datasets that require attribution. Verify the disclosure text against the current model card before publishing it. **Ship the Kokoro NOTICE attribution with the installer whether or not the weights ever arrive.**
- **Graceful degradation.** If the user never downloads it, nothing breaks. Narration is stored as text in the project format, so projects open, render, and publish normally, and captions still generate from the script. Only audio generation is unavailable. This follows directly from the text-first narration decision, so it is not a blocking dependency.

### The GPL question this does *not* settle

OpenStage Author already works the way option 1 below describes *(confirmed)*. The editor never makes audio itself: `tools/narrate.py` does, in the speech engine's own Python, which the user points to with the `OPENSTAGE_KOKORO_PYTHON` setting. With no engine set up, the editor says so plainly and notes that transcripts and captions still work. The roadmap lists "Installer and first-run model download" as not yet built, and nothing is packaged: the editor runs from source.

Per the OpenStage session's check of the owner's computer *(relayed; the repository does not mention it)*, the Python environment that runs Kokoro narration includes `phonemizer-fork` (GPLv3 or later) and the espeak-ng speech engine (also GPL). Download-on-first-use fixes installer size and weight licensing, but **it does not by itself resolve these**:

- **Today it is fine:** OpenStage doesn't bundle them and talks to them as a separate program the user installed.
- **Mirroring the weights on our own release is fine** (Apache-2.0). **Mirroring or auto-downloading the GPL components from our own release would make the project a distributor of GPL code**, with source-offer and notice obligations.

**In plain terms.** Narration relies on two helper tools that turn written words into speech sounds. Both are published under the **GPL**, a "share-alike" license: anyone who *hands out* the tools must also publish their source code and follow the GPL's rules, and mixing them into a product can pull that whole product under the GPL. OpenStage is Apache-2.0, which is more relaxed, so the two need to be kept cleanly apart. There are three ways to do that:

| Option | What it means in practice | Good | Not so good |
|---|---|---|---|
| **1. Keep them separate; the user installs them** *(today's arrangement)* | OpenStage never includes or hosts the helper tools. A user who wants spoken narration installs them once from their official source, and OpenStage uses them if they are there. Everything except spoken narration works without them. | No legal risk. No extra work. Nothing changes. | An extra setup step for anyone who wants narration, which is a real hurdle on locked-down campus computers. |
| **2. Replace them with tools under a relaxed license** | Find substitutes under Apache, MIT, or BSD licenses, so narration can be downloaded and offered as freely as the voice model itself. | Smoothest for users. No GPL anywhere. | Someone has to find and test a replacement for each language, and pronunciation quality might drop. It is not known whether a good enough replacement exists. |
| **3. Include them and follow the GPL's rules** | OpenStage hands out the helper tools itself, publishes their source code, and keeps their license notices, with the tools staying separate programs. | Smoothest for users, with no search for substitutes. | Needs a lawyer's sign-off, brings ongoing obligations, and some institutions' software policies may balk at GPL code in the package. |

**Decided (owner): option 1, for now.** Option 2 stays available as a later improvement; option 3 is off the table unless a lawyer says it is fine. Revisit before any installer ships anything that includes these tools. This is a decision for OpenStage's owner, ideally with legal advice, and is not legal advice itself. The separate-process design keeps GPL code out of OpenStage Capture's own binary and source, but it does not remove the redistribution obligations of anything shipped alongside.

**One small documentation gap.** The repository's `NOTICE` lists Kokoro-82M (Apache-2.0) but says nothing about the user-supplied engine's GPL components, and neither `phonemizer` nor `espeak` is mentioned anywhere in the repository *(confirmed)*. Under option 1, a sentence in `NOTICE` or the README saying that spoken narration needs a user-supplied engine that may include GPL components OpenStage does not ship would make the arrangement explicit. That edit belongs in the OpenStage repository.

## 8. Scope: one big first release

The owner would rather have everything major ready to ship at once, so this brief plans **a single first release, Release 1**, for **Windows only**. Accessibility work runs through all of it rather than being saved for the end, because the hardest parts (the audio cues, the timeline's keyboard behavior, and the project-format tracks) are architectural.

**The trade-off, stated plainly:** a bigger first release takes longer to reach anyone, and the timeline, the interactive step-by-step content, and the captioning all have to reach the same accessibility standard before it ships. To manage that, the work is **built in a suggested order** (below) even though it **ships as one release**.

### Prerequisites outside this module

Release 1 does not ship until these OpenStage Author items are done. They are OpenStage work, not Capture work, and their schedule is OpenStage's to set.

1. **Responsive output in the published player** *(owner decision: fix first)*. Today the stage is a fixed size scaled to fit the window; text cannot re-wrap at narrow widths or large text sizes, which fails WCAG 2.2 AA 1.4.10 (Reflow). The roadmap lists "Responsive output" under Later and the format spec calls it "the open responsive-output decision", so it must move up the roadmap. It will touch the format (a schema version bump with a migration), the shared renderer, and the editor's stage. This is the largest prerequisite and a real design question for a freeform canvas. One possible starting point, for OpenStage to weigh: every object already has a required reading order (`tabOrder`), which might drive a stacked layout at narrow widths. That is a design question for OpenStage, not decided here.
2. **SCORM 1.2 verified in a real LMS** (Brightspace and Moodle, then Canvas). This is already open on OpenStage's own roadmap and release gate.
3. **The streaming launch path** in OpenStage's Rust shell (section 9.1). Built as part of this project and agreed with the owner first.
4. **A baseline screen-reader pass** on the editor patterns Capture reuses (6.3).
5. **Player support for audio description** in OpenStage's course player: the pop-up, playing the video with the description voiced in time, and pausing for extended description. Today the player uses the browser's own controls for video.
6. **A text-entry input** in OpenStage (a free-text field that saves into a variable), for typing steps in Practice mode. OpenStage has no free-text input today *(confirmed)*.
7. **Scoring from steps** in OpenStage, so Practice and Assessment steps can contribute to the score and SCORM reporting. Today only multiple-choice quiz questions feed the score *(confirmed)*.

Not prerequisites: timing that follows a video (a roadmap request only, open item 8), and macOS, Linux, JAWS, or VoiceOver work.

**Checkpoint.** Responsive output could turn out to be much larger than expected. If it does, the owner can revisit the decision and ship Release 1 with the gap stated plainly in the accessibility statement. That would be the owner's call, made with the facts at that point. The same applies to prerequisites 5 to 7: if more than one proves large, the owner can revisit what Release 1 includes.

### Release 1 includes

**Recording**
- Capture on Windows: screen/window/region + microphone + system audio + webcam
- Interaction logging: cursor position, clicks, key events, active window; opt-in, with a clear on-screen and audible indicator
- Recorder panel in native HTML, global hotkeys, audible state cues, and a Start again action that sets a bad take aside (nothing is erased)
- Process protocol and crash-safe recording with a `recover` command (section 9)

**Editing**
- Track/clip model with a keyboard-operable table view
- **Camtasia-style multi-track timeline**, extending OpenStage's existing Timeline panel pattern (6.1), keyboard-complete and screen-reader-verified before it ships
- Trim, split, delete, reorder, multi-track
- Images, audio, and short video clips from outside the recording on the timeline, drawn from the course's asset library; each keeps its source and licence record and carries the alt text, captions, or transcript the validator requires
- Cursor re-styling and click highlights at render time (with flash safeguards)
- Zoom and pan into parts of the screen (by hand, or suggested from clicks); text overlays, callouts, arrows, blur/redact

**Accessible content**
- Captions from the narration script, and WebVTT/SRT import and export
- **Local speech-to-text** for human-recorded audio (see the fallback rule below)
- Transcripts, alt text, audio description track (script → speech), delivered in a pop-up the learner opens, including playing the video with the description voiced in time
- Accessibility checker pane

**Interactive training**
- Auto step segmentation → slide-per-step content in OpenStage's slide model, with step text and alt text pre-filled from UI Automation
- Demo / Practice / Assessment output with keyboard-operable hotspots and text-entry checks, through OpenStage's player and publisher

**Output and publishing**
- Export **MP4** (H.264 video, AAC audio) through Windows's built-in encoders
- Insert as a video asset or slide-per-step content in an OpenStage course
- **Publishing: SCORM 1.2 only**, through the single publisher. Today OpenStage's publisher produces a folder of plain web files that runs in any browser, and a SCORM 1.2 zip *(confirmed)*. OpenStage's own release gate is that the package launches in Moodle, Canvas, and Brightspace without error, that passing and failing attempts report the correct score, and that a learner who leaves returns to their exact position *(confirmed, CLAUDE.md)*. A recording-bearing course meeting that gate is the must-pass acceptance test, because a course that fails to report completion correctly is worse than no course *(relayed)*. The owner has Brightspace and Moodle and is getting Canvas access (item 9). OpenStage's SCORM 1.2 output has so far been checked only against a strict stand-in LMS, never a real one, so this test also checks the existing publisher *(confirmed)*

**Quality and honesty**
- Project format v0 (documented) with caption, transcript, alt-text, and language fields from day one
- Baseline screen-reader pass on the reused OpenStage patterns; full manual pass with NVDA and Narrator before release (6.3)
- Accessibility statement published

**Fallback rule for speech-to-text.** This is the one Release 1 item whose feasibility isn't yet known. If no local, permissively licensed speech-recognition tool passes the license and quality audit, Release 1 ships with script-first captions only, and speech recognition moves to the next release. It must not delay everything else.

### Suggested build order inside Release 1 *(not separate releases)*

1. **Foundations:** baseline screen-reader pass on OpenStage's patterns; license and FFmpeg CI guards; process-protocol spec; project format.
2. **Recording:** reliable capture with crash recovery; hotkeys and audio cues.
3. **Editing foundation:** track/clip model and keyboard table view; MP4 rendering.
4. **Timeline:** the visual multi-track timeline over that model.
5. **Learning features:** step segmentation, zoom and callouts, practice modes; captions, speech-to-text, audio description; accessibility checker.
6. **Publishing and hardening:** SCORM 1.2 in a real LMS; full manual screen-reader pass; accessibility statement.

Responsive output (prerequisite 1) can proceed in parallel with steps 1 to 5, but Release 1 cannot ship before it is finished.

### After Release 1 *(not scheduled)*

- **macOS** (when a Mac user is available) and **Linux**; **JAWS** and **VoiceOver** testing
- SCORM 2004 (it separates completion status from success status; do it once 1.2 is proven in a real LMS), xAPI and cmi5 (they need a learning record store we do not control, so shipping them early would be a checkbox, not a capability)
- WebM output, sign-language track, accessible PDF/HTML step guides, templates and themes, noise removal, localization, plugin API

## 9. Architecture and technical considerations

### 9.1 Process model and protocol *(proposal, to fit OpenStage's existing conventions)*

**How OpenStage Author launches helpers today** *(confirmed, `editor/src-tauri/src/lib.rs` and `tauri.conf.json`)*:
- The Rust shell starts helper programs directly with Rust's standard process launcher. It does **not** use Tauri's bundled-program ("sidecar") feature and has no shell plugin. It finds programs by searching upward from the app for the repository's files, because everything runs from source today (the config has `bundle.active: false`). Locations can be overridden with `OPENSTAGE_*` settings (`OPENSTAGE_NODE`, `OPENSTAGE_PYTHON`, `OPENSTAGE_KOKORO_PYTHON`).
- **One job at a time.** A single "preview or publish" slot; a second request is refused until the first finishes or is cancelled.
- **Wait, then read.** Standard input is closed. The shell waits for the program to finish, collects all of its output, and takes the last line starting with `{` as the JSON result.
- **Cancel is a hard stop.** It kills the program and everything it started (`taskkill /T /F` on Windows).

**What this means for recording.** A recording is long-running and interactive, so the existing launch path cannot run it as it stands. OpenStage Author's Rust shell needs a **second, streaming launch path**:
- keep standard input open, to send `pause`, `resume`, `stop`, and `marker`
- read standard output line by line while the program runs, to show live state and pass it to the page
- its own slot, so a recording doesn't block (or get blocked by) preview and publish, with the interplay defined
- a **graceful stop first** (finalize the file), with the hard kill only as the fallback
- the window's content-security policy currently allows media only from the app itself, `blob:`, and one sound site, so previewing local video will need an agreed change

This is a modest but real change inside OpenStage Author, not in the Capture program. CLAUDE.md says anything beyond the editor's first slice is agreed with the owner first, so it should be.

**The capture program** follows the established pattern, with two differences because a recording is long-running and interactive:
- **Events stream as JSON lines on stdout**: a first line with the protocol version, then `state` (idle/countdown/recording/paused/stopped), elapsed time, throttled audio levels, warnings, errors, and a final `result` with the project path. Logs go to stderr.
- **Control comes in as JSON lines on stdin**: `pause`, `resume`, `stop`, `restart`, `marker`. **`stop` is graceful** (finalize the file); **cancel is the existing kill-the-tree** behavior. Because cancel is abrupt, **recording must be crash-safe**: write in segments (for example fragmented MP4) with an append-only `events.jsonl`, flush regularly, and offer `recover` to repair an interrupted project. A killed recording should lose seconds, not the take.
- **Why a separate process:** it holds its own OS permission prompts (screen capture, microphone, accessibility), a crash can't take the editor down, and anything with a stricter license stays out of OpenStage's own code.
- **Naming follows OpenStage's conventions** *(confirmed, CLAUDE.md)*: the program is `openstage-capture.exe` (as `openstage-editor.exe`), and an `OPENSTAGE_CAPTURE` setting overrides its location. The project keeps the short `OPENSTAGE_*` prefix on purpose.
- **Protocol as contract.** CLAUDE.md names three plugin contracts (audio from text, captions from media, image from prompt) as a rule, but the roadmap shows no general plugin API built yet *(confirmed)*. This protocol, versioned and documented, could become the pattern for them.
- **Installer.** Nothing is packaged yet; the roadmap lists "Installer and first-run model download" as not built. How the capture program ships (bundled with the app or alongside it) is decided when the installer is designed.

### 9.2 Capture on Windows (first target)

- Windows Graphics Capture for screen/window; WASAPI loopback for system audio, WASAPI for the microphone
- UI Automation for the element under each click (name and role); optional, best-effort, permission-aware, and **opt-in per recording** because it can expose on-screen text
- Media Foundation for H.264/AAC (MP4) encoding. WebM (VP9/Opus) is not in Release 1; it could be added later as a patent-free option
- macOS: ScreenCaptureKit (needs Screen Recording and Accessibility permissions). Linux: PipeWire via xdg-desktop-portal; Wayland restricts global input capture, so interaction logging there may be limited

### 9.3 Project format and timeline scope

**Two projects, kept apart.** The course is an OpenStage project folder (`my-course.osproj/`, holding `project.json`, `slides/`, `assets/video/`, `assets/captions/`, and so on *(confirmed, `format-spec.md`)*). A recording lives in its own **capture project** folder beside it (for example `my-recording.capture/`), holding a versioned `project.json`, the raw media segments, `events.jsonl`, and the edit decisions (the media timeline), with caption, audio-description, transcript, alt-text, and language tracks defined in the schema from the start. No proprietary container.

**What crosses into the course.** Only finished things: the rendered MP4 goes to the course's `assets/video/`, its WebVTT captions to `assets/captions/`, and for step-by-step content, ordinary slide files whose objects carry the required fields. A `video` object already exists in the format: it plays a file from `assets/video/` with the browser's own controls, takes a `captions` path and an optional `transcript`, and can fire a `mediaComplete` trigger when it finishes *(confirmed)*.

**The raw capture project must not live inside the `.osproj`,** for three reasons:
- **Privacy.** The SCORM zip adds everything in the project folder except `.cache/` to `source/` (`publish.ts`, around lines 209 to 224 *(confirmed)*). Raw footage and an interaction log (keystrokes, window titles, on-screen element names) inside the project would be handed to every learner. Published packages must exclude them unless the author explicitly opts in.
- **It is not a cache.** `.cache/` is regenerable and excluded; raw capture is the source for re-editing, so it can't be treated as cache.
- **Size.** Raw recordings are large; they would balloon the package and the git history, since a project's assets are committed.

If a capture folder were ever placed inside a project, the publisher would need a small, agreed change to exclude it.

**Rendered once, stored as an asset.** OpenStage's release gate includes "a project saved, closed, reopened and republished is byte-identical" *(confirmed, CLAUDE.md)*. So the MP4 is rendered when the author exports, stored as an asset, and referenced; it is never regenerated at publish time.

**Two different timelines; keep them separate.** OpenStage's format specification describes timing only as "appears at / disappears at", in milliseconds, per object *(confirmed)*: `appear` (counted from the slide or layer opening, or from the object's own narration starting), `start` (the delay after that), `visibleFor` (how long it stays, or forever), and an optional slide `duration`. A separate `animation` field covers one entrance and one exit per object, each one of none, fade, four slide directions, or zoom, lasting 0.1 to 2 seconds. There are no keyframes, motion paths, easing curves, looping, or emphasis effects; those were left out on purpose, and slide transitions and motion paths are not built. The roadmap says plainly: "Not built: timing that follows a video or an audio clip" (ROADMAP.md:40). A Camtasia-style multi-track timeline needs far more: tracks of video, audio, captions, callouts, and zoom/pan over time. The recommended boundary: **the media timeline belongs to OpenStage Capture's project format**, and OpenStage Capture renders it to a video asset or slide-per-step content that is handed to the single publisher. OpenStage's slide timing stays basic and its spec does not change. If the spec must change, that is OpenStage's decision.

**What follows from that boundary**
- **Slide-per-step content** can use ordinary OpenStage objects (text, callouts, arrows) timed with appear/disappear. That is real text on the page, so it is accessible in the normal way.
- **Inside a video,** anything that follows the video's own clock (auto zoom/pan, moving callouts, cursor highlights, blur) has to be rendered into the MP4, because OpenStage cannot yet time objects to a video's clock. Effects baked into video are invisible to screen readers, so their meaning must travel as text alongside it: the transcript, the audio description, and alt text. That is already planned (6.2), and this is why it is required rather than optional. If OpenStage later adds timing that follows a video, callouts could be real objects with real text instead, which is better for accessibility (open item 8).

**Proposed crate layout** (to align with OpenStage's workspace conventions): `openstage-capture` (the engine) · `openstage-capture-project` · `openstage-capture-timeline` · `openstage-capture-render` · `openstage-capture-protocol` · `openstage-capture-a11y` (caption/alt-text model, checker) · `openstage-capture-cli`

### 9.4 Hard problems to design early

1. A/V sync across screen, system audio, mic, and webcam clocks
2. Variable-frame-rate screen capture vs. constant-rate editing
3. Crash-safe, recoverable recording (9.1)
4. Long (60+ minute) recordings
5. Privacy: password fields, notifications, and sensitive windows (exclusion/redaction at capture time), and keeping raw recordings and interaction logs out of published packages (9.3)
6. Hotkeys that never collide with assistive technology (6.1)
7. Audible, reliable state feedback while another application has focus (6.1)
8. An accessible multi-track timeline (6.1)

## 10. Success criteria

- A first-time user records a short tutorial and drops it, captioned, into a course in under 10 minutes.
- **A user can complete record → trim → caption → insert with the keyboard alone, and separately with a screen reader alone (NVDA and Narrator), with no critical blockers found in manual testing on Windows.**
- **A recording interrupted by a killed process recovers all but the last few seconds.**
- **A recording-bearing course published as SCORM 1.2 reports completion correctly in a real LMS,** tested in Brightspace and Moodle, and in Canvas once access arrives (OpenStage's release gate names all three).
- **A published course package contains no raw recordings, keystroke logs, or window logs** unless the author explicitly chose to include them.
- **A course with a Capture video, saved, closed, reopened, and republished, is byte-identical** (OpenStage's own release gate), because the MP4 is rendered once and stored as an asset.
- **With the narration weights absent, projects still open, render, and publish; only audio generation is unavailable.**
- **Zero serious or critical axe findings on generated HTML**, plus a passing manual keyboard and screen-reader review of the player (after responsive output ships).
- **Published courses, including ones with recordings, pass the reflow check (WCAG 2.2 AA 1.4.10)** at 320 CSS pixels wide and at large text sizes.
- Capture overhead stays below ~10% CPU on a mid-range laptop at 1080p30.
- A/V drift under 40 ms over a 60-minute recording.
- The dependency license check, including the FFmpeg build-configuration guard, passes in CI on every commit.
- An accessibility statement is published; an ACR only after real screen-reader testing exists.

## 11. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| **No real screen-reader testing of OpenStage exists yet** | The patterns this module reuses may not work as intended | Baseline NVDA/Narrator pass before building on them (6.3) |
| **Multi-track timeline is hard to make keyboard- and screen-reader-operable** | Authors with disabilities locked out of the headline feature; ATAG Part A claim at risk | Structural model first; visual timeline is a view over it; does not ship until verified |
| **Capture departs from OpenStage Author's own rules** (video editor, cross-platform, TypeScript) *(confirmed)* | Work done against the host project's stated scope until CLAUDE.md says otherwise | The owner has accepted all three exceptions; record them in CLAUDE.md before building |
| **The existing launcher can't run a live recording** *(confirmed)* | No pause/stop control or live state; a recording would block preview and publish | Add a streaming launch path with its own slot and a graceful stop (9.1) |
| Media from outside the recording brings licensing and accessibility obligations | Unlicensed music or images; uncaptioned video; missing alt text | Draw imports from the course asset library (source, creator, and licence recorded); the validator's rules apply; prompt for missing alt text, captions, or transcripts |
| Audio description needs new features in OpenStage's course player (pop-up, described playback, pausing for extended description) | More OpenStage work in front of Release 1 | Scope it early alongside responsive output; the owner can revisit the decision if it proves large |
| Practice and Assessment modes need new OpenStage features (a text-entry input, scoring from steps) *(confirmed)* | More OpenStage format and player work in front of Release 1 | Scope them early with the other prerequisites; the checkpoint in section 8 applies |
| **One big first release** | A longer wait before anything ships; more to test at once; schedule risk | Build in the suggested order (section 8); freeze the Release 1 list; apply the speech-to-text fallback rule |
| **OpenStage's SCORM 1.2 has never run in a real LMS** *(confirmed)* | Release 1's headline test could fail for reasons outside this module | Test OpenStage's current output in Brightspace and Moodle first (Canvas when access arrives), before building on it |
| Raw recordings or interaction logs shipped inside published packages | Learners receive keystrokes, window titles, and raw footage | Exclude by default; explicit opt-in; checked in the acceptance tests (section 10) |
| Timeline scope collides with OpenStage's basic slide-timing spec *(confirmed)* | Pressure to change OpenStage's format | Keep the media timeline in OpenStage Capture's project format; render to assets (9.3) |
| A GPL FFmpeg build enters via imported build configuration | Silently relicenses the combined work | CI guard on FFmpeg build flags before any integration (section 7) |
| The host name "OpenStage" is also an existing phone product line | Search noise; possible trademark friction | Trademark and search check before the name goes public (section 13) |
| Cancel kills the process tree mid-recording | Lost or corrupt recordings | Crash-safe segments, append-only events, `recover` command |
| Global hotkeys collide with screen-reader keys | Authors can't start or stop recording | Configurable hotkeys; test against NVDA, JAWS, Narrator defaults |
| Live-region announcements may not fire while another app has focus | Screen-reader users don't know recording state | Audio cues from the capture process; verify with real AT |
| Player fails reflow (1.4.10); responsive output is a Release 1 prerequisite *(owner decision)* | Release 1 cannot ship before OpenStage has responsive output, and it may be much larger than expected | Track it on OpenStage's roadmap; keep the checkpoint in section 8 so the owner can revisit the decision; list the gap in the accessibility statement until it is fixed |
| GPL components in the narration environment | Redistribution obligations if mirrored or bundled | Keep upstream-sourced and user-installed, or replace; decide before any installer (section 7) |
| Downloaded narration weights tampered with or corrupted | Unverified blob runs inference | SHA-256 verification before load |
| Managed institutional machines block downloads | Narration unavailable to a core audience | Documented manual placement path; text-first narration keeps everything else working |
| Windows-only hardware | macOS, Linux, VoiceOver, and JAWS unverified | Windows only for Release 1; make no claims for platforms or screen readers that were not tested |
| Windows's built-in encoders are the only MP4 route | Output depends on Windows media components (some Windows editions may lack them) | Check supported Windows editions early; other encoders can be added later under the license rules |
| Codec patent exposure (H.264/AAC in MP4) | Legal | Use Windows's built-in encoder; legal check before shipping |
| Speech-to-text may not pass the license and quality audit | Human-recorded audio can't be auto-captioned | Script-first captions always work; the fallback rule in section 8 moves speech recognition out of Release 1 rather than delaying it |
| Auto-generated captions and alt text are wrong | Inaccessible content that looks accessible | Always editable; flagged unreviewed until approved |
| Conformance claims made without testing | Institutions rely on a false claim | Accessibility statement only until real testing exists |
| Scope creep toward a full authoring suite | Never ships | Hold the non-goals line; OpenStage is the authoring suite |

## 12. Open questions

*Resolved: all three differences from CLAUDE.md are accepted as exceptions (Windows first, a mid-level video editor, a Rust capture program); media from outside the recording is accepted (option B); the editor is mid-level (basic editing, text overlays, zoom, like Camtasia); reflow (fix responsive output first, as a Release 1 prerequisite); the owner will add the timing-follows-video request to OpenStage's roadmap; name (OpenStage Capture); the Camtasia-style timeline is in Release 1; one big first release; Release 1 scope confirmed; no code from the existing OpenScreen project; GPL helper tools: option 1; MP4 output; no compliance deadline; no contribution rules; Brightspace and Moodle as real-LMS test targets, with Canvas access on the way; no VPAT yet (accessibility statement instead); and every fact previously listed to confirm in OpenStage's files (below).*

### Confirmed from the OpenStage Author repository

Read by the author of this brief from `jasonswrench-beep/openstage-author` (read-only, commit 23d53e5). The first three were also reported earlier by the OpenStage session.

- **Publishing.** `publish.ts` produces two things and no other package format: a folder of plain web files (`index.html`, `player.js`, `player.css`, `assets/`, and generated `narration/` audio) that runs in any browser, untracked if no LMS is present; and a SCORM 1.2 zip, `<name>-scorm12.zip`, holding those files plus `imsmanifest.xml` and the editable project under `source/`. The manifest marks the course as graded when it has a quiz. There is no SCORM 2004, xAPI, cmi5, or Tin Can code; SCORM 2004 and xAPI appear only on the roadmap under Later (ROADMAP.md:55). SCORM 1.2 has been checked only against a strict stand-in LMS, never a real one (ROADMAP.md:13).
- **SCORM 1.2 status.** SCORM 1.2 has a single status field; the player keeps completion and success separate internally and merges them at the last moment when writing `lesson_status` (`scorm.ts`).
- **Timing.** Appear/disappear only, in milliseconds, plus one entrance and one exit animation per object. "Not built: timing that follows a video or an audio clip" (ROADMAP.md:40). Details in 9.3.
- **Reflow.** Still open. `format-spec.md` lists it as a known limit that falls short of WCAG 2.2 AA 1.4.10 and says responsive output is deferred; the roadmap lists "Responsive output" under Later. The player does scale the stage down to fit narrow windows (`player.ts`, `fitStage`) but cannot reflow, and its stylesheet has no width-based media rules (only reduced motion, forced colors, and print).
- **Written accessibility bar.** WCAG 2.2 AA for published output and ATAG Part A for the editor, with every feature keyboard operable and screen-reader usable (CLAUDE.md). "WCAG 2.1" appears nowhere in the repository; the 2.1 figure belongs to the ADA rule (6.4).
- **FFmpeg.** None anywhere in the repository.
- **Helper launching.** Direct, one job at a time, hard-kill cancel; no Tauri sidecar. Details and consequences in 9.1.
- **Screen recording is already on the roadmap.** "Screen recording," "Software simulation authoring," and "Step capture" are listed under Later (ROADMAP.md:55), with "Whisper for imported media" and "Responsive output".
- **A keyboard-operable Timeline already exists** for object timing (ROADMAP.md:40, `docs/editor.md`, `timelinemodel.ts`, `timelinepanel.ts`); see 6.1.
- **Release gate** (CLAUDE.md): the SCORM package launches in Moodle, Canvas, and Brightspace without error; passing and failing attempts report the correct score; completion and success are separate states; a learner who leaves returns to their exact position; the whole editor and course are keyboard operable; both are usable with NVDA and VoiceOver; generated captions match the script and are correctly timed; a saved, closed, reopened, and republished project is byte-identical. Roadmap items still unchecked: tests in Moodle, Canvas, and Brightspace; the ATAG Part A pass including NVDA; the installer.
- **Narration engine.** Found through `OPENSTAGE_KOKORO_PYTHON`; with none, the editor says so and transcripts and captions still work. `NOTICE` lists Kokoro-82M (Apache-2.0); the GPL components are not mentioned anywhere in the repository (section 7).
- **Project rules that shape this module** (CLAUDE.md): narration is text and audio is a cache; media are referenced assets, not embedded; the editor and player share one renderer; local models sit behind three plugin contracts; and three scope statements that this brief departs from (item 7).

### Items still open

1. ~~Confirm the Release 1 list.~~ **Done:** confirmed by the owner. Section 8 stands as written.
2. **Pick a speech-recognition tool.** This is research, not a decision for you. It would plug in through OpenStage's planned "captions from media" contract, and "Whisper for imported media" is already on OpenStage's roadmap, so start with that candidate (believed to be MIT-licensed; to be verified in the audit). Candidates are checked for license and quality; the fallback rule in section 8 applies if none passes.
3. **Legal check on MP4 patents** before anything ships. H.264 and AAC involve patent licensing; Windows's built-in encoder handles this on the platform side, but get it confirmed.
4. **Trademark and search check on the names** ("OpenStage" and "OpenStage Capture") before they go public (section 13).
5. ~~The reflow gap in OpenStage's player: ship with it, or pull the fix forward?~~ **Decided (owner):** fix responsive output first. It is now a Release 1 prerequisite (section 8), with a checkpoint if it proves much larger than expected. Moving "Responsive output" from Later into OpenStage's active roadmap is an OpenStage edit for you or your OpenStage session.
6. ~~A real LMS to test with.~~ **Done:** the owner has access to Brightspace and Moodle, so OpenStage's SCORM 1.2 output and Release 1 can be tested in both. Use a test or sandbox course, not a live course with real learners.
7. ~~Where this module departs from OpenStage Author's own rules.~~ **Resolved (owner):** all three are accepted as exceptions for the Capture module.
   - **Cross-platform from the start:** Windows only for now.
   - **"Not a video editor":** the Capture module includes a mid-level video editor (section 5).
   - **"TypeScript throughout":** the capture program is Rust, because screen capture needs native Windows code. The repository already uses Rust (the shell) and Python (the tools).
   - Each is a one-line edit to CLAUDE.md in the OpenStage repository, which only you or your OpenStage session should make (this session has read-only access). Suggested wording: *"Not a full or professional video editor. The Capture module includes a mid-level editor for recordings: trimming, splitting, text overlays, and zoom, in the manner of Camtasia."* / *"Cross-platform from the start, except the Capture module, which is Windows first."* / *"TypeScript throughout, except the Capture program, which is Rust because screen capture needs native code."*
8. ~~Ask for timing that follows a video.~~ **Done:** the owner will add it to OpenStage's roadmap as a request. It is not a Release 1 requirement; Capture works without it (effects rendered in, with text equivalents).
9. ~~Canvas.~~ **In progress:** the owner is getting access to Canvas. OpenStage's release gate names Moodle, Canvas, and Brightspace, so Canvas joins Brightspace and Moodle as a test target once access arrives. Until then, the accessibility statement and release notes say Canvas was not tested.
10. ~~Media from outside the recording.~~ **Decided (owner): option B.** The editor also accepts images, audio, and short video clips, drawn from the course's asset library, with the validator's rules applying to each (section 5).

### Revisit when…

| If this happens | Then |
|---|---|
| A Mac user becomes available | Add macOS capture and VoiceOver testing |
| A JAWS license becomes available | Add JAWS testing |
| Canvas access arrives | Add Canvas to the SCORM test pass (OpenStage's release gate names it) |
| Responsive output proves much larger than expected | The owner revisits the decision (checkpoint in section 8) |
| The tool is adopted by a department, campus, or system, or takes grant or institutional funding | Expect requests for an accessibility conformance report and possibly deadlines; build on the test evidence already collected |
| Outside contributors want to help | Set contribution rules (section 7) before accepting their code |
| Anyone wants an installer that includes the narration tools | Revisit the GPL decision (section 7) before it ships |

## 13. Naming

**Decided: OpenStage Capture** (by the owner). It replaces the working title "OpenScreen".

**Why the working title was dropped.** "OpenScreen" is already the name of an existing, MIT-licensed open-source screen recorder (an Electron and TypeScript alternative to Screen Studio, per its documentation and directory listings; its repository was not examined for this brief) and of a W3C standard, the Open Screen Protocol. Sharing the name would have caused search confusion and attribution questions, and might have suggested a relationship to that project that does not exist.

**No code is being reused** from the existing OpenScreen project (owner decision: cleaner).

**Conventions to follow** *(confirmed, CLAUDE.md)*. The product is OpenStage Author (website openstageauthor.com), and "OpenStage" is the accepted short form. The `.osproj` extension, `openstage-editor.exe`, the `OPENSTAGE_*` settings, and the `os-` CSS prefix keep the short name on purpose. OpenStage Capture follows suit: `openstage-capture` for the program and crates, and `OPENSTAGE_CAPTURE` for its location setting. The crate names `openstage-capture` and `openstage_capture` were unregistered on crates.io when checked (this does not cover GitHub, npm, or trademarks).

**One thing to know about the host name.** "OpenStage" is also the name of an existing line of IP desk phones sold by Unify, under a trademark licensed from Siemens (per Unify's own wiki). That is a different industry from e-learning software, so confusion may be limited, but searches will be noisy and a trademark and search check is worth doing before the name goes public. This is not legal advice.

## 14. Proposed next steps

1. ~~Confirm the Release 1 list.~~ **Done:** confirmed by the owner (section 8).
2. **Baseline screen-reader pass** on OpenStage Author's existing editor patterns (NVDA and Narrator), including its Timeline panel, since the multi-track timeline will extend it. It gates the whole approach.
3. **Record the three CLAUDE.md changes** the owner has accepted (Windows first, a mid-level video editor, a Rust capture program) by having the OpenStage session make the edits (suggested wording in item 7).
4. **Plan responsive output** in OpenStage Author (prerequisite 1, section 8): move it from Later into the active roadmap and scope it (format change, shared renderer, editor stage), with the checkpoint in mind. The owner and the OpenStage session lead this.
5. **Add the roadmap request** for timing that follows a video (item 8).
6. **Put the rule checks in place** from the first line of code: license checking and the FFmpeg build-flag guard (section 7).
7. **Test OpenStage's current SCORM 1.2 output in Brightspace and Moodle** (available now), then in Canvas when access arrives, using test courses rather than live ones. Release 1's must-pass test depends on it, and it has only been checked against a stand-in so far.
8. **Design the streaming launch path** in OpenStage's Rust shell (section 9.1), agreed with the owner first.
9. **Write the process protocol** as a one-page spec: JSON-lines events, stdin control, graceful stop vs. kill, crash-safe segments, `recover`.
10. **Windows capture spike:** screen + mic + system audio + cursor events + UI Automation element info, written to a capture project folder; then kill the process mid-recording and recover.
11. **MP4 render spike:** render that project to MP4 through Media Foundation, confirm the license checks pass, and check which Windows editions have the needed media components.
12. **Audit speech-recognition candidates** (start with Whisper) for license and quality, to decide whether they fit Release 1 or trigger the fallback rule.
13. **Draft the accessibility statement,** including the reflow gap until it is fixed.
14. **When the brief moves into the OpenStage repository,** apply that project's writing rules and working style (for example, no em dashes in prose).
15. ~~Convert this brief into a PRD.~~ **Done:** first draft in [prd.md](prd.md), with user stories, "done when" checklists, and accessibility checks.

## 15. Plain-language glossary

| Term | What it means |
|---|---|
| **Release 1** | The first public release. Planned as one big release with everything major, rather than a small first version (what the industry calls an "MVP") followed by a bigger one. |
| **MP4 (H.264/AAC)** | The most widely playable video file type. Its video and audio formats involve patent licensing, which Windows's built-in encoder covers on the platform side. |
| **Capture engine** | The part that records the screen, sound, and mouse and keyboard activity. |
| **Separate process** | A program that runs on its own, apart from OpenStage. If it crashes, OpenStage keeps running. |
| **Crash-safe** | The recording is saved in small pieces as it goes, so a crash loses seconds, not the whole take. |
| **Timeline** | The strip in editors like Camtasia where video, audio, captions, and effects are arranged over time in layers. |
| **Track / clip** | A track is one layer (for example the screen video, or the narration). A clip is one piece on a track. |
| **Project format** | The way a recording and all its edits are saved in files, so other tools can read them. |
| **Publisher** | The part of OpenStage that turns a finished course into a package a learning system can run. |
| **LMS** | Learning management system: the website where learners take courses (for example Brightspace or Moodle). |
| **SCORM 1.2** | The long-standing standard a course uses to report to an LMS that a learner finished. Widest compatibility. |
| **xAPI** | A newer reporting standard that also needs a separate "learning record store" to be useful. |
| **Screen reader** | Software that reads the screen aloud for blind and low-vision users (NVDA, JAWS, Narrator, VoiceOver). |
| **WCAG 2.2 AA** | The international yardstick for accessible web content. This is the target for courses and recordings. |
| **ATAG 2.0** | The matching yardstick for tools that create content. Part A: the tool itself is accessible. Part B: the tool helps authors make accessible content. |
| **Accessibility statement** | An honest public description of what is targeted, what has been tested, and what is known to be incomplete. |
| **ACR / VPAT** | A formal, tested report of how well a product meets accessibility standards. Needs real testing first. |
| **Apache-2.0** | A relaxed open-source license: anyone may use, change, and share the code with few conditions. |
| **GPL** | A "share-alike" license: anyone who hands out code that includes GPL parts must share their source and follow GPL's rules. |
| **NOTICE file** | A file that credits other people's work that the project uses. |
| **CI check** | An automatic test that runs every time the code changes, for example to catch a forbidden license. |
| **Bundling / mirroring** | Bundling: packing a file inside the installer. Mirroring: hosting our own copy of a file for download. |
| **Reflow** | Text and layout re-wrapping to fit a narrow screen or a large text size, instead of just shrinking the whole page into a smaller picture. |
| **Stand-in LMS** | A pretend learning system built for testing. Useful, but it can't catch every way a real LMS behaves differently. |
| **Repository (repo)** | A project's folder on GitHub, with all its files and a saved history of every change. |
| **Sidecar** | A Tauri feature for bundling a helper program inside an app. OpenStage Author does not use it today. |
