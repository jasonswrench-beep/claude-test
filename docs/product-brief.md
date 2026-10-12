# OpenStage Capture — Product Brief

*(formerly the working title "OpenScreen"; see [Naming](#13-naming))*

**Status:** Draft v0.5 — for review
**License:** Apache-2.0 (hard constraint; see [Licensing & dependency policy](#7-licensing--dependency-policy))
**Context:** A module of **OpenStage**, a Tauri v2 desktop app (Rust shell, plain TypeScript + HTML interface). Delivered as a **separate capture process** that OpenStage's interface drives.
**Accessibility:** First-class requirement, for the tool *and* for everything it produces (see [section 6](#6-accessibility-requirements))

> **Provenance note.** Items marked *(relayed)* come from OpenStage's scoping document, capability matrix, or notes as reported to this brief. The author of this brief has no access to the OpenStage repository and has not read those files. Facts about what OpenStage's code does today are listed in [section 12](#12-open-questions) as things to confirm in the repo, not decisions.

> **Plain-language guide.** This brief uses a few technical terms; the [glossary](#15-plain-language-glossary) at the end explains them all. The two that matter most:
> - **Release 1** (the industry calls it the **MVP**, "minimum viable product") is the first version people can use for real, start to finish: record, trim, add captions, drop it into a course. It does the whole job, but not every advanced feature.
> - **Release 2** (the industry calls it **v1**) is the next, fuller version with the more advanced features.
>
> Where this brief says "MVP" it means Release 1, and where it says "v1" it means Release 2.
>
> **Decisions that need you** are collected in [section 12](#12-open-questions), in plain terms, each with a recommended default.

## Decisions so far

| Decision | Value |
|---|---|
| License | Apache-2.0 |
| Host project | OpenStage: Tauri v2, Rust shell, TypeScript + plain HTML UI (WebView2 on Windows), no UI framework |
| Engine language | Rust |
| Process model | Separate process with **no interface of its own**. OpenStage's UI and global hotkeys drive it; JSON on stdout; same pattern OpenStage already uses for `publish.ts`, `narrate.py`, and `validate.py` |
| Interface | Native HTML controls inside OpenStage. No separate GUI toolkit |
| Platforms | **Windows first** (the only test hardware available). macOS and Linux later, when testers exist |
| Editing | **Camtasia-style multi-track timeline confirmed** (owner: screen recording needs a more complex timeline). *When* it arrives, Release 1 or Release 2, is [open question 1](#12-open-questions) |
| Publishing | **One publisher only: OpenStage's.** Any capture path hands assets to it; OpenStage Capture never ships its own exporter. v1 targets **SCORM 1.2 only**; SCORM 2004 and xAPI are Later *(relayed)* |
| Narration | Text-first. Audio generation is **download on first use, never bundled** in the installer. Nothing breaks if it is never downloaded |
| Speech | Local only, permissive licenses only |
| Accessibility bar | WCAG 2.2 AA for output; ATAG 2.0 Part A for the editor (this brief proposes adding Part B). **No conformance report yet; publish an accessibility statement** |
| Name | **OpenStage Capture** (confirmed by owner; crate prefix `openstage-capture`). Replaces the working title "OpenScreen" |

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

## 4. Reference products — what we take and what we skip

| Capability | Camtasia | Captivate | Storyline | Our stance |
|---|---|---|---|---|
| Screen + audio + webcam capture | ✔ | ✔ | ✔ (step-by-step) | **MVP** |
| Editing | multi-track timeline | basic | basic | **Wanted: Camtasia-style multi-track timeline.** MVP builds the accessible track/clip model and a clip list with trim/split; the full multi-track UI follows in v1 (open question 1) |
| Editable cursor, click effects | ✔ | ✔ | partial | **MVP**, enabled by separate interaction data |
| Auto zoom/pan | ✔ (SmartFocus) | ✔ | — | **v1** |
| Callouts, annotations, shapes | ✔ | ✔ | ✔ | **v1**, reusing OpenStage's canvas where possible |
| Captions / transcript | ✔ | ✔ | ✔ | **MVP** from script; speech-to-text **v1** |
| Auto step-by-step slides from clicks | — | ✔ | ✔ | **v1** |
| Demo / Practice / Assessment modes | — | ✔ | ✔ | **v1** |
| SCORM / xAPI export | partial (quizzes) | ✔ | ✔ | **One publisher (OpenStage's).** v1: SCORM 1.2 only. SCORM 2004 and xAPI: Later |
| Audio description track | ? | ? | ? | **v1** *(competitor support not verified)* |
| Branching, variables, full authoring suite | — | ✔ | ✔ | **Non-goal**; that is OpenStage |
| Templates, asset library, themes | ✔ | ✔ | ✔ | Later |

## 5. Goals and non-goals

**Goals**
1. Reliable capture of screen (full / window / region), system audio, microphone, and webcam on Windows, with a documented capture-to-file path.
2. Record interaction events in a **documented, open project format** (versioned JSON + media files) that other tools can read and write.
3. Editing inside OpenStage's interface, fully operable by keyboard and screen reader, sufficient to finish a tutorial: trim, split, reorder, annotate, on a Camtasia-style multi-track timeline.
4. Output as MP4/WebM video and as slide-per-step content. **There is exactly one publisher (OpenStage's).** Any capture path hands assets to it, so editor preview and published output cannot drift apart.
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
- Building a speech-to-text engine into the MVP.

## 6. Accessibility requirements

Accessibility has two halves, and [ATAG 2.0](https://www.w3.org/TR/ATAG/) (W3C Authoring Tool Accessibility Guidelines) names both: **Part A** covers making the authoring tool itself accessible, and **Part B** covers making it support and promote accessible content. ATAG 2.0 references WCAG 2.0, so apply it with **WCAG 2.2 AA** as the success-criteria baseline.

**Targets:** WCAG 2.2 AA for the editing UI and all generated output; ATAG 2.0 Part A (OpenStage's existing bar) **plus Part B** for this module, because a recorder authors content. **An accessibility statement now; an Accessibility Conformance Report (ACR/VPAT-style) only after real screen-reader testing exists.**

### 6.1 The tool must be accessible (ATAG Part A)

The recorder and editor are OpenStage HTML, so they inherit OpenStage's accessibility rules and patterns: native controls, a polite live region announcing each action, dialogs that close with Escape and return focus, keyboard-driven canvas, and tables with captions and headers. **Those patterns are written down but have not yet been tested with a real screen reader** (see 6.3). Recording adds requirements of its own:

- **Recorder panel** uses native buttons with accessible names and states; no drag-only or hover-only operations; no keyboard traps.
- **Global hotkeys** to start/pause/stop, registered by OpenStage (Tauri has a global-shortcut plugin; verify), configurable, and working while another application has focus. They must **not collide with screen-reader modifier keys** (Insert and Caps Lock on NVDA, JAWS, and Narrator).
- **Feedback when OpenStage isn't focused.** Once recording starts, the user is in another application. OpenStage's live region may not be announced then (unverified), so the capture process plays distinct audio cues for start, pause, resume, stop, and error, and OpenStage shows the same state when it regains focus. Validate with NVDA and Narrator.
- **Recorder UI excluded from the capture** (for example, via Windows window display-affinity settings; verify with WebView2 windows) so controls never appear in the video.
- **Timeline without a mouse.** The multi-track timeline is the hardest custom widget in the product and the riskiest commitment under ATAG Part A. Build the **structural model first**: tracks → clips → properties, exposed as a `<table>` or tree with captions and headers, numeric timecode inputs, and a keyboard model that mirrors the canvas (arrow keys move, Alt+arrows resize or trim, Page Up/Down select). The visual timeline and any drag editing are a view over that model, never the only path. The visual timeline does not ship until the structural model is keyboard-complete and screen-reader-verified.
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
- **Use OpenStage's course player and publisher; never build a second one.** Practice and Assessment modes use keyboard-operable hotspots and text-entry checks. **No click-only interactions.** Time limits adjustable or removable (WCAG 2.2.1). Correct language, headings, landmarks, and reading order.
  - **Known gap to fix first:** OpenStage's player uses a fixed-size stage that fails WCAG's reflow criterion (1.4.10). Recordings placed in a course inherit that until it is fixed.
- **Optional sign-language video track** as a first-class track type.
- **Accessibility checker.** A pre-export report that lists missing captions, missing alt text, contrast failures, flashing risk, and unlabeled hotspots, with jump-to-fix. Warnings are prominent and defaults favor accessibility, but the author is not blocked from exporting.

### 6.3 Accessibility in the process

- **First deliverable: a baseline screen-reader pass on OpenStage's existing editor patterns** (the ones the recorder will reuse) with NVDA and Narrator on Windows. JAWS if a license is available. VoiceOver when someone with a Mac can test. So far only scripted keyboard and structure checks have been run.
- Accessibility acceptance criteria on every user story; no feature merges without keyboard and screen-reader behavior defined. OpenStage's acceptance tests already require keyboard-only operation and NVDA and VoiceOver verification for the editor and published output *(relayed)*; keep the test evidence, because it is the raw material for a real ACR later.
- **Automated:** axe-core (or equivalent) on every generated HTML package in CI.
- **Manual:** scripted passes with real assistive technology on the core flows (record → trim → caption → insert into course) before each release.
- **User testing** with people who use assistive technology, from the first usable build.
- **Publish an accessibility statement now, not a conformance report.** It states what is targeted, what has actually been tested, what is known to be incomplete, and how to report a problem. Update it as testing happens. Keep a public accessibility issues list.

### 6.4 Compliance context

- **Written bar vs. this brief.** OpenStage's notes state the course bar as WCAG 2.2 AA in one place and 2.1 AA in another. This brief targets **2.2 AA**; it includes the 2.1 AA requirements apart from the obsolete 4.1.1 Parsing criterion. Reconcile the notes.
- **Two separate claims; do not conflate them.** **WCAG 2.2 AA** is the claim about the published course, what learners experience, and most vendors document it. **ATAG 2.0 Part A** is the claim about the authoring tool, what authors experience, and it is much less commonly documented. The ATAG claim is the one worth documenting properly over time.
- **No VPAT/ACR yet.** A conformance report written without testing is a false claim that institutions will rely on when adopting the tool. Publish the accessibility statement (6.3) and build the evidence as the acceptance tests run.
- **When an ACR will be asked for.** A free tool used by an individual instructor rarely reaches procurement. Expect a request once a department, campus, or system adopts or recommends it institutionally, and treat it as near-certain if the project takes grant or institutional funding, because documentation obligations can arrive attached to the money. This is a landscape, not legal advice.
- **US public institutions.** The DOJ's ADA Title II rule sets **WCAG 2.1 AA** for web content and mobile apps of state and local governments, which includes public universities. An April 2026 interim final rule moved the compliance dates to **April 26, 2027** (entities serving 50,000 or more people) and **April 26, 2028** (smaller entities and special districts). This comes from law-firm and university summaries ([Plante Moran](https://plantemoran.com/explore-our-thinking/insight/2026/04/doj-extends-ada-title-ii-web-accessibility-compliance-deadlines), [Duane Morris](https://www.duanemorris.com/alerts/doj_extends_ada_title_ii_digital_accessibility_deadlines_one_year_0426.html), [University of Arizona](https://accessibility.arizona.edu/news/doj-issues-interim-final-rule-title-ii-digital-accessibility-compliance-dates)), not the Federal Register, and the rule may have changed since. Because 2.2 AA is a superset, meeting this brief's target meets that standard. Which date applies to a given institution is for its accessibility officer or ADA coordinator, not this brief.
- **Section 508 and EN 301 549** also cover authoring tools; per OpenStage's notes, ATAG Part A fits that. WCAG 2.2 AA covers the web-content levels of 508, EN 301 549, and AODA.

## 7. Licensing & dependency policy

The project is **Apache-2.0** and will be part of OpenStage, so every dependency must be one that downstream users can ship under Apache-2.0 without relicensing the whole work.

| Category | Policy | Examples |
|---|---|---|
| **Allowed** | Apache-2.0, MIT, BSD-2/3, ISC, zlib, Unlicense/CC0, dual "MIT OR Apache-2.0" (the Rust norm) | Tauri, windows-rs, Opus, libvpx, libaom, SVT-AV1, OpenH264, Kokoro-82M (code and weights Apache-2.0) |
| **Case-by-case** | MPL-2.0 (file-level copyleft; keep modifications isolated); LGPL **only** if dynamically linked, replaceable, and not required for core function | FFmpeg LGPL builds, GStreamer plugins |
| **Prohibited** | GPL, AGPL, SSPL, any LGPL static linking, "source-available" / non-commercial / proprietary-with-conditions licenses | FFmpeg `--enable-gpl` builds, x264, x265, OBS Studio code, `phonemizer-fork` (GPLv3+), espeak-ng (GPL) |

Practical consequences:

- **Do not bundle FFmpeg.** Prefer **OS-native encoders and capture APIs** (Media Foundation on Windows) and permissive codecs (OpenH264, VP9, AV1, Opus). If FFmpeg is ever optional, it must be a user-supplied, dynamically loaded, LGPL build, never a default dependency.
- **FFmpeg guard before any integration.** `cargo-deny` does not see native libraries. Add a CI check that fails the build if any FFmpeg build configuration contains `--enable-gpl` or `--enable-nonfree`, and flags `--enable-version3` for review, **before** any code or build configuration from another project is brought in. A GPL FFmpeg build entering through an imported build configuration would silently relicense the combined work.
- **Reusing code from other projects, even permissively licensed ones:** attribution in `NOTICE`, a license check on the specific files, and no imported build configuration until it has been audited. Never copy build configurations wholesale.
- **Codec patents are separate from copyright licenses.** Apache-2.0 grants patent rights only for the project's own contributions. H.264/AAC (and HEVC) carry third-party patent pools. Using the OS encoder or Cisco's pre-built OpenH264 binary shifts this to the platform vendor; VP9/AV1/Opus avoid it. **Needs legal review before shipping.**
- **Housekeeping:** SPDX header on every source file, a `NOTICE` file for third-party attributions, `cargo-deny` license checks in CI, and a decision on **DCO vs. CLA** (use OpenStage's policy if it has one).
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

Per OpenStage's notes, the Python environment that runs Kokoro narration includes `phonemizer-fork` (GPLv3 or later) and the espeak-ng speech engine (also GPL). Download-on-first-use fixes installer size and weight licensing, but **it does not by itself resolve these**:

- **Today it is fine:** OpenStage doesn't bundle them and talks to them as a separate program the user installed from upstream.
- **Mirroring the weights on our own release is fine** (Apache-2.0). **Mirroring or auto-downloading the GPL components from our own release would make the project a distributor of GPL code**, with source-offer and notice obligations.

**In plain terms.** Narration relies on two helper tools that turn written words into speech sounds. Both are published under the **GPL**, a "share-alike" license: anyone who *hands out* the tools must also publish their source code and follow the GPL's rules, and mixing them into a product can pull that whole product under the GPL. OpenStage is Apache-2.0, which is more relaxed, so the two need to be kept cleanly apart. There are three ways to do that:

| Option | What it means in practice | Good | Not so good |
|---|---|---|---|
| **1. Keep them separate; the user installs them** *(today's arrangement)* | OpenStage never includes or hosts the helper tools. A user who wants spoken narration installs them once from their official source, and OpenStage uses them if they are there. Everything except spoken narration works without them. | No legal risk. No extra work. Nothing changes. | An extra setup step for anyone who wants narration, which is a real hurdle on locked-down campus computers. |
| **2. Replace them with tools under a relaxed license** | Find substitutes under Apache, MIT, or BSD licenses, so narration can be downloaded and offered as freely as the voice model itself. | Smoothest for users. No GPL anywhere. | Someone has to find and test a replacement for each language, and pronunciation quality might drop. It is not known whether a good enough replacement exists. |
| **3. Include them and follow the GPL's rules** | OpenStage hands out the helper tools itself, publishes their source code, and keeps their license notices, with the tools staying separate programs. | Smoothest for users, with no search for substitutes. | Needs a lawyer's sign-off, brings ongoing obligations, and some institutions' software policies may balk at GPL code in the package. |

**Recommendation:** choose **option 1 now** and look into **option 2** in the background. Skip option 3 unless a lawyer says it is fine. The decision is **not urgent**: it is only needed before the first installer is released. This is a decision for OpenStage's owner, ideally with legal advice, and is not legal advice itself. The separate-process design keeps GPL code out of OpenStage Capture's own binary and source, but it does not remove the redistribution obligations of anything shipped alongside.

## 8. Scope by milestone

Windows first. Accessibility work is spread across every milestone rather than saved for the end, because the hardest parts (the process protocol's audio cues, the track/clip model's keyboard behavior, and the project-format tracks) are architectural.

**Release 1 (the MVP) — "record, trim, drop into a course"**
- Capture on Windows: screen/window/region + mic + system audio + webcam
- Interaction logging: cursor position, clicks, key events, active window; opt-in with a clear on-screen and audible indicator
- **Process protocol v0**: `record`, `render`, `probe`, `recover`; JSON-lines events; graceful stop; crash-safe output (see section 9)
- **Project format v0** (documented) with caption, transcript, alt-text, and language fields from day one
- **Track/clip model** with its keyboard-operable table view: the accessible foundation the multi-track timeline will sit on
- **OpenStage integration:** recorder panel in native HTML, global hotkeys, audible state cues, clip list with trim/split, insert as a video asset in a slide with its caption track
- **Script-first captions** through OpenStage's existing narration route; WebVTT/SRT import and export
- Cursor re-styling and click highlights at render time (with flash safeguards)
- Export MP4 (Media Foundation) and WebM (VP9/Opus)
- Baseline screen-reader pass on the reused OpenStage patterns (6.3)
- Accessibility statement published

**Release 2 (v1) — "teach with it"**
- **Camtasia-style multi-track timeline UI** over the MVP track/clip model, keyboard-complete and screen-reader-verified before it ships *(recommended placement; see open question 1)*
- Auto step segmentation → slide-per-step content in OpenStage's slide model, with step text and alt text pre-filled from UI Automation
- Auto zoom/pan; callouts, text, arrows, blur/redact
- Demo / Practice / Assessment output with keyboard-operable hotspots and text-entry checks, through OpenStage's player and publisher
- **Publishing: SCORM 1.2 only**, through the single publisher. A recording-bearing course that reports completion correctly in a real LMS is the must-pass acceptance test, because a course that fails to report completion correctly is worse than no course *(relayed)*
- **Local speech-to-text** for human-recorded audio (model and library audited for license first)
- Audio description track (script → speech)
- Accessibility checker pane
- macOS (needs a tester and hardware), then Linux (PipeWire + xdg-desktop-portal)

**Later**
- SCORM 2004 (earns its place by separating completion status from success status, once 1.2 is proven in a real LMS), xAPI and cmi5 (need a learning record store we do not control, so shipping them early would be a checkbox, not a capability)
- Sign-language track, accessible PDF/HTML step guides, templates and themes, noise removal, localization, plugin API.

## 9. Architecture and technical considerations

### 9.1 Process model and protocol *(proposal, to fit OpenStage's existing conventions)*

OpenStage already launches `publish.ts`, `narrate.py`, and `validate.py` as separate processes: one job at a time, JSON back on standard output, and a cancel that stops the whole process tree. The capture process follows the same pattern, with two differences because a recording is long-running and interactive, not a batch job:

- **Events stream as JSON lines on stdout**: a first line with the protocol version, then `state` (idle/countdown/recording/paused/stopped), elapsed time, throttled audio levels, warnings, errors, and a final `result` with the project path. Logs go to stderr.
- **Control comes in as JSON lines on stdin**: `pause`, `resume`, `stop`, `marker`. **`stop` is graceful** (finalize the file); **cancel is the existing kill-the-tree** behavior. Because cancel is abrupt, **recording must be crash-safe**: write in segments (for example fragmented MP4) with an append-only `events.jsonl`, flush regularly, and offer `recover` to repair an interrupted project. A killed recording should lose seconds, not the take.
- **Why a separate process:** it holds its own OS permission prompts (screen capture, microphone, accessibility), a crash can't take the editor down, and anything with a stricter license stays out of OpenStage's own code.
- **Protocol as contract.** OpenStage's `CLAUDE.md` describes plugin contracts that are intended but not built *(relayed)*. This protocol, versioned and documented, could become that contract.
- **Launch mechanism:** Tauri can bundle and launch external binaries ("sidecars"); check whether that fits how OpenStage launches its other tools.

### 9.2 Capture on Windows (first target)

- Windows Graphics Capture for screen/window; WASAPI loopback for system audio, WASAPI for the microphone
- UI Automation for the element under each click (name and role); optional, best-effort, permission-aware, and **opt-in per recording** because it can expose on-screen text
- Media Foundation for H.264/AAC encoding, with VP9/Opus as the portable fallback
- macOS: ScreenCaptureKit (needs Screen Recording and Accessibility permissions). Linux: PipeWire via xdg-desktop-portal; Wayland restricts global input capture, so interaction logging there may be limited

### 9.3 Project format and timeline scope

A folder or zip of `project.json` (versioned schema) + media segments + `events.jsonl`, with caption, audio-description, transcript, alt-text, and language tracks defined in the schema from v0. No proprietary container.

**Two different timelines; keep them separate.** OpenStage's format specification defines slide timing as basic object appear/disappear timing, explicitly not keyframe animation *(relayed; the spec has not been read for this brief)*. A Camtasia-style multi-track timeline needs more: tracks of video, audio, captions, callouts, and zoom/pan over time. The recommended boundary: **the media timeline belongs to OpenStage Capture's project format**, and OpenStage Capture renders it to a video asset or slide-per-step content that is handed to the single publisher. OpenStage's slide timing stays basic and its spec does not change. If the spec must change, that is OpenStage's decision.

**Proposed crate layout** (to align with OpenStage's workspace conventions): `openstage-capture` (the engine) · `openstage-capture-project` · `openstage-capture-timeline` · `openstage-capture-render` · `openstage-capture-protocol` · `openstage-capture-a11y` (caption/alt-text model, checker) · `openstage-capture-cli`

### 9.4 Hard problems to design early

1. A/V sync across screen, system audio, mic, and webcam clocks
2. Variable-frame-rate screen capture vs. constant-rate editing
3. Crash-safe, recoverable recording (9.1)
4. Long (60+ minute) recordings
5. Privacy: password fields, notifications, and sensitive windows (exclusion/redaction at capture time)
6. Hotkeys that never collide with assistive technology (6.1)
7. Audible, reliable state feedback while another application has focus (6.1)
8. An accessible multi-track timeline (6.1)

## 10. Success criteria

- A first-time user records a short tutorial and drops it, captioned, into a course in under 10 minutes.
- **A user can complete record → trim → caption → insert with the keyboard alone, and separately with a screen reader alone (NVDA and Narrator), with no critical blockers found in manual testing on Windows.**
- **A recording interrupted by a killed process recovers all but the last few seconds.**
- **A recording-bearing course published as SCORM 1.2 reports completion correctly in a real LMS.**
- **With the narration weights absent, projects still open, render, and publish; only audio generation is unavailable.**
- **Zero serious or critical axe findings on generated HTML**, plus a passing manual keyboard and screen-reader review of the player (after the reflow gap is fixed).
- Capture overhead stays below ~10% CPU on a mid-range laptop at 1080p30.
- A/V drift under 40 ms over a 60-minute recording.
- The dependency license check, including the FFmpeg build-configuration guard, passes in CI on every commit.
- An accessibility statement is published; an ACR only after real screen-reader testing exists.

## 11. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| **No real screen-reader testing of OpenStage exists yet** | The patterns this module reuses may not work as intended | Baseline NVDA/Narrator pass before building on them (6.3) |
| **Multi-track timeline is hard to make keyboard- and screen-reader-operable** | Authors with disabilities locked out of the headline feature; ATAG Part A claim at risk | Structural model first; visual timeline is a view over it; does not ship until verified |
| Timeline scope collides with OpenStage's basic slide-timing spec *(relayed)* | Pressure to change OpenStage's format | Keep the media timeline in OpenStage Capture's project format; render to assets (9.3) |
| A GPL FFmpeg build enters via imported build configuration | Silently relicenses the combined work | CI guard on FFmpeg build flags before any integration (section 7) |
| The host name "OpenStage" is also an existing phone product line | Search noise; possible trademark friction | Trademark and search check before the name goes public (section 13) |
| Cancel kills the process tree mid-recording | Lost or corrupt recordings | Crash-safe segments, append-only events, `recover` command |
| Global hotkeys collide with screen-reader keys | Authors can't start or stop recording | Configurable hotkeys; test against NVDA, JAWS, Narrator defaults |
| Live-region announcements may not fire while another app has focus | Screen-reader users don't know recording state | Audio cues from the capture process; verify with real AT |
| Player fails reflow (1.4.10) | Recordings inherit a WCAG failure | Fix in OpenStage before claiming 2.2 AA for recording-bearing courses |
| GPL components in the narration environment | Redistribution obligations if mirrored or bundled | Keep upstream-sourced and user-installed, or replace; decide before any installer (section 7) |
| Downloaded narration weights tampered with or corrupted | Unverified blob runs inference | SHA-256 verification before load |
| Managed institutional machines block downloads | Narration unavailable to a core audience | Documented manual placement path; text-first narration keeps everything else working |
| Windows-only hardware | macOS/Linux and VoiceOver unverified | Windows first; recruit a Mac tester before promising macOS |
| Permissive-only encoder options are thinner than FFmpeg's | Output format/quality limits | OS encoders; VP9/AV1 as the portable fallback |
| Codec patent exposure (H.264/AAC) | Legal | Legal review; default to OS encoders / OpenH264 |
| No speech-to-text yet | Human-recorded audio can't be auto-captioned | Script-first captions in MVP; audited local STT in v1 |
| Auto-generated captions and alt text are wrong | Inaccessible content that looks accessible | Always editable; flagged unreviewed until approved |
| Conformance claims made without testing | Institutions rely on a false claim | Accessibility statement only until real testing exists |
| Scope creep toward a full authoring suite | Never ships | Hold the non-goals line; OpenStage is the authoring suite |

## 12. Open questions

*Resolved: name (OpenStage Capture); the Camtasia-style timeline is confirmed as wanted; single publisher; SCORM 1.2 only for v1; narration delivery; no VPAT yet, accessibility statement instead.*

### Facts to confirm in the OpenStage repo (not decisions)

These are answered by reading files, not by deciding. They go to whoever has the repository.

- **What does `publish.ts` output today?** The decided targets (SCORM 1.2 for v1) are what it *must* do; what it *does* is unverified. If it does not exist yet, the honest answer is "nothing yet."
- **How does the format specification define the timeline?** Confirm it matches the "basic appear/disappear timing, not keyframe animation" description, so the separation in 9.3 holds.
- **Which written WCAG bar is canonical in OpenStage's notes, 2.1 or 2.2?** (This brief assumes 2.2 AA.)
- **Is there any FFmpeg build configuration** in OpenStage, or anywhere being drawn on, and what flags does it use?
- **Does a Tauri sidecar fit** how OpenStage launches its other tools?
- **Has the player's reflow gap (1.4.10) been fixed or scoped?**

### Decisions

Each decision has a recommended default, so "go with your recommendation" is a valid answer.

1. **When should the Camtasia-style timeline arrive?** You have confirmed you want it. The only question is whether it comes in Release 1 or Release 2.
   - **Option A: in Release 1.** The first usable version includes the full multi-track timeline. *Good:* you get the headline feature from day one. *Not so good:* a longer wait before anything is usable, and the timeline is the hardest feature to make fully usable with a keyboard and screen reader, so building it first is the riskiest path.
   - **Option B (recommended): in Release 2.** Release 1 has a simpler list-style editor: a table of clips you can trim, split, and reorder, built on the same foundations as the timeline. The full Camtasia-style timeline arrives in Release 2. *Good:* something usable sooner, and the timeline gets built properly on a proven base. *Not so good:* a longer wait for the timeline itself.
   - If the timeline is what makes the product worth releasing at all, choose A and accept a longer first release.
2. **Is any code being copied from the existing OpenScreen project (the Electron and TypeScript screen recorder), or was only the name borrowed?** Yes or no. *Assumed no.* If yes, we need to credit it in a `NOTICE` file and check what it brings with it before anything is copied (section 7).
3. **The GPL helper tools for narration:** option 1, 2, or 3 in section 7. *Recommended: option 1 now, and look into option 2.* Not urgent; needed only before the first installer.
4. **Compliance deadlines and reports.** Ask your campus accessibility officer or ADA coordinator which deadline applies and whether anyone will ask for a formal accessibility conformance report. Only your institution can answer this.
5. **Testing help.** Is there anyone with a Mac who can test with VoiceOver (Apple's screen reader), and do you have access to JAWS (a paid Windows screen reader)? Without them, testing claims are limited to Windows with NVDA and Narrator.
6. **Automatic captions from a recorded voice.** Needed once authors record their own voice rather than writing a script. It requires a speech-recognition tool that runs on the user's own computer under a relaxed license. *Not needed for Release 1, so no decision is needed yet.*
7. **Video file formats.** The most common format (H.264/AAC, the kind most MP4 files use) involves patent licensing fees. Windows can produce it with its own built-in support, which shifts that to Microsoft. The alternative is patent-free formats such as WebM, which fewer devices play. *Recommended: use Windows's built-in support and also offer WebM, after a legal check.*
8. **Rules for outside contributors.** When other people contribute code, do they sign an agreement (a CLA) or just certify with each contribution that they have the right to share it (a DCO)? If OpenStage already has a policy, this project follows it.

## 13. Naming

**Decided: OpenStage Capture** (by the owner). It replaces the working title "OpenScreen".

**Why the working title was dropped.** "OpenScreen" is already the name of an existing, MIT-licensed open-source screen recorder (an Electron and TypeScript alternative to Screen Studio, per its documentation and directory listings; its repository was not examined for this brief) and of a W3C standard, the Open Screen Protocol. Sharing the name would have caused search confusion and attribution questions, and might have suggested a relationship to that project that does not exist.

**Checks done on the new name.** The crate names `openstage-capture` and `openstage_capture` are unregistered on crates.io. This does not cover GitHub, npm, or trademarks.

**One thing to know about the host name.** "OpenStage" is also the name of an existing line of IP desk phones sold by Unify, under a trademark licensed from Siemens (per Unify's own wiki). That is a different industry from e-learning software, so confusion may be limited, but searches will be noisy and a trademark and search check is worth doing before the name goes public. This is not legal advice.

## 14. Proposed next steps

1. **Baseline screen-reader pass** on OpenStage's existing editor patterns (NVDA and Narrator). It gates the whole approach.
2. **Add the FFmpeg build-flag CI guard** (section 7) before any code or build configuration from another project is integrated.
3. **Confirm the repo facts** in section 12 by reading the files, starting with `publish.ts` and the format specification.
4. **Settle open questions 1–3** (when the timeline arrives; any code reuse from the existing OpenScreen project; the GPL helper tools).
5. **Write the process protocol v0** as a one-page spec: JSON-lines events, stdin control, graceful stop vs. kill, crash-safe segments, `recover`.
6. **Windows capture spike:** screen + mic + system audio + cursor events + UI Automation element info, written to the draft project format; then kill the process mid-recording and recover.
7. **Render spike:** that project to MP4 (Media Foundation) and WebM with only allowed dependencies; confirm `cargo-deny` and the FFmpeg guard pass.
8. **Write and publish the accessibility statement**, and decide the narration-environment plan before any installer ships.
9. Convert this brief into a PRD with user stories, each carrying accessibility acceptance criteria.

## 15. Plain-language glossary

| Term | What it means |
|---|---|
| **Release 1 / MVP** | "Minimum viable product": the first version that does the whole job end to end and can be used for real. |
| **Release 2 / v1** | The next, fuller version, with the more advanced features. |
| **Capture engine** | The part that records the screen, sound, and mouse and keyboard activity. |
| **Separate process** | A program that runs on its own, apart from OpenStage. If it crashes, OpenStage keeps running. |
| **Crash-safe** | The recording is saved in small pieces as it goes, so a crash loses seconds, not the whole take. |
| **Timeline** | The strip in editors like Camtasia where video, audio, captions, and effects are arranged over time in layers. |
| **Track / clip** | A track is one layer (for example the screen video, or the narration). A clip is one piece on a track. |
| **Project format** | The way a recording and all its edits are saved in files, so other tools can read them. |
| **Publisher** | The part of OpenStage that turns a finished course into a package a learning system can run. |
| **LMS** | Learning management system: the website where learners take courses (for example Canvas or Moodle). |
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
