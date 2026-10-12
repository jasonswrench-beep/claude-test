# OpenScreen — Product Brief

*(working title; see [Naming](#13-naming))*

**Status:** Draft v0.2 — for review
**License:** Apache-2.0 (hard constraint; see [Licensing & dependency policy](#7-licensing--dependency-policy))
**Context:** A module of **OpenStage**, a Rust project. Desktop application.
**Accessibility:** First-class requirement, for the tool *and* for everything it produces (see [section 6](#6-accessibility-requirements))

## Decisions so far

| Decision | Value |
|---|---|
| License | Apache-2.0 |
| Host project | OpenStage |
| Implementation language | Rust |
| Delivery | Desktop application (browser-based capture is out) |
| Accessibility | Release gate, not a backlog item |
| Name | OpenScreen (working title, unconfirmed) |

---

## 1. Summary

An open-source, accessible screen recorder and lightweight editor for people who make **instructional video and software training**. It covers the ground Camtasia holds for video and Adobe Captivate / Articulate Storyline hold for e-learning software simulations, as a permissively licensed Rust module that ships inside OpenStage.

Two ideas drive the design:

1. **Record the screen and the user's interactions as separate, editable data.** Video plus cursor, clicks, keystrokes, active window, and (where the OS allows) the UI element that was clicked, rather than flattened pixels. That enables re-styled cursors, automatic zoom-and-pan, auto-generated step text, and conversion of a recording into an interactive "show me / let me try / test me" simulation.
2. **Accessibility is built into the data model, not bolted on.** Captions, transcripts, audio description, alt text, and language are first-class tracks in the project format. The same UI-element data that powers step captions also powers alt text and accessible step-by-step guides.

## 2. Problem and opportunity

- Camtasia is excellent for video but closed, paid, and not embeddable.
- Captivate and Storyline do software simulations well but are heavy authoring suites; screen capture is a means to an end.
- Open-source recorders (OBS and similar) capture well but have no timeline editing, no interaction data, and no e-learning output. The most capable ones are GPL, so they cannot be embedded in an Apache-2.0 project.
- Timeline editors in this category are generally built around mouse drag-and-drop, which is a barrier for authors who use a keyboard, switch device, voice control, or screen reader. Accessible output is usually left to the author to assemble by hand.
- **Gap:** a permissively licensed, embeddable engine that records interaction data, is usable by authors with disabilities, and helps every author produce accessible video and training.

## 3. Target users and jobs-to-be-done

| User | Job |
|---|---|
| **Instructional designers / L&D** | Turn a software walkthrough into an accessible SCORM/xAPI module with narration, captions, and a practice mode |
| **Educators / course creators** | Record a lecture or tutorial, trim it, add callouts, export captioned video |
| **Software support / docs teams** | Produce short how-to clips and step-by-step guides that stay easy to update when the UI changes |
| **Authors with disabilities** | Do all of the above with keyboard, screen reader, voice control, or switch access |
| **Learners using assistive technology** | Watch, practice, and be assessed with the same quality of experience as other learners |
| **OpenStage** | Embed recording and editing as a capability inside a larger product |

## 4. Reference products — what we take and what we skip

| Capability | Camtasia | Captivate | Storyline | Our stance |
|---|---|---|---|---|
| Screen + audio + webcam capture | ✔ | ✔ | ✔ (step-by-step) | **MVP** |
| Multi-track timeline editor | ✔ | basic | basic | **MVP** (simple: trim, split, multi-track), keyboard-complete |
| Editable cursor, click effects | ✔ | ✔ | partial | **MVP**, enabled by separate interaction data |
| Auto zoom/pan | ✔ (SmartFocus) | ✔ | — | **v1** |
| Callouts, annotations, shapes | ✔ | ✔ | ✔ | **v1** |
| Captions / transcript | ✔ | ✔ | ✔ | **MVP data model, v1 generation** |
| Auto step-by-step slides from clicks | — | ✔ | ✔ | **v1** |
| Demo / Practice / Assessment modes | — | ✔ | ✔ | **v1** |
| SCORM / xAPI / cmi5 / HTML5 export | partial (quizzes) | ✔ | ✔ | **v1** |
| Audio description track | ? | ? | ? | **v1** *(competitor support not verified)* |
| Branching, variables, full authoring suite | — | ✔ | ✔ | **Non-goal**, hand off to OpenStage |
| Templates, asset library, themes | ✔ | ✔ | ✔ | Later |

## 5. Goals and non-goals

**Goals**
1. Reliable capture of screen (full / window / region), system audio, microphone, and webcam, with a documented capture-to-file path.
2. Record interaction events in a **documented, open project format** (versioned JSON + media files) that other tools can read and write.
3. A usable timeline editor, enough to finish a tutorial without leaving the tool, fully operable without a mouse.
4. Export to MP4/WebM and to interactive HTML5 / SCORM / xAPI.
5. **Accessible by construction:** meet the targets in section 6 for both the application and its output.
6. **Library-first:** the engine is usable headless and from other Rust codebases; the UI is a thin layer over it.
7. 100% Apache-2.0-compatible dependency tree.

**Non-goals (for now)**
- Matching Camtasia's effects/transitions catalog or Storyline's full interaction model.
- Live streaming or multi-camera production (OBS's territory).
- Cloud hosting, review/collab workflows, or an LMS.
- Mobile-device capture.
- Browser-based capture.

## 6. Accessibility requirements

Accessibility has two halves, and **ATAG 2.0** (W3C Authoring Tool Accessibility Guidelines) is the framework that names both: **Part A** covers making the authoring tool itself accessible, and **Part B** covers making it support and promote accessible content. ATAG 2.0 references WCAG 2.0, so we apply it with **WCAG 2.2 AA** as the success-criteria baseline.

**Targets:** WCAG 2.2 AA for the application UI and all generated output; ATAG 2.0 Part A and Part B at AA; alignment with Section 508 and EN 301 549; a published Accessibility Conformance Report (VPAT-style) with each release.

### 6.1 The tool must be accessible (ATAG Part A)

- **Keyboard-complete.** Every function reachable and operable by keyboard alone; no drag-only operations; visible focus; logical focus order; customizable shortcuts.
- **Screen reader support** on Windows (NVDA, Narrator, JAWS), macOS (VoiceOver), and Linux (Orca). Every control has an accessible name, role, state, and value; state changes are announced.
- **Voice control compatibility.** Accessible names contain the visible label text (WCAG 2.5.3) so Dragon, Voice Control, and Voice Access can target controls.
- **Timeline without a mouse.** The timeline is the hardest custom widget in the product. It needs a structural, non-visual model: tracks → clips → properties, navigable as a tree or list, with commands for move, trim, split, and numeric timecode entry. Drag editing is a convenience on top of that model, not the only path.
- **Recording is operable and perceivable without sight.** Global hotkeys to start/pause/stop; recording state, countdown, and errors announced to assistive technology and available as audio cues; tray/menu-bar controls accessible; no time-limited UI interactions during setup.
- **Respect OS settings:** high-contrast and forced-colors modes, text scaling and display zoom, reduced motion, color-independent status indicators, minimum target sizes (WCAG 2.2 2.5.8).
- **Authors who are blind can verify their output.** A described preview (text readout of what is on screen at the playhead: window, cursor target, active callouts) so the visual result isn't visible to sighted authors only.
- **Capture of assistive technology.** Screen-reader speech and focus indicators must be recordable, since authors often need to demonstrate AT workflows.

### 6.2 The output must be accessible, and the tool must make that the easy path (ATAG Part B)

- **Captions as a first-class track.** Timed text, language-tagged, edited alongside audio, exported as sidecar (WebVTT/SRT) and embedded where the container supports it. Generated via pluggable speech-to-text, always human-editable.
- **Transcripts and step-by-step text** generated from the recording, from the same data as captions.
- **Audio description track** (WCAG 1.2.5 / 1.2.3): text-described with TTS or recorded; support for extended description (pausing video) in interactive output.
- **Alt text for non-text content** (callouts, shapes, images, hotspots), with a prompt rather than silent omission. Pre-fill from the OS accessibility tree where available (UI Automation on Windows, AX on macOS, AT-SPI on Linux), for example "Button: Save".
- **Motion and flashing safeguards.** Zooms, click effects, and transitions must not exceed three flashes per second (WCAG 2.3.1); respect reduced-motion in the interactive player; warn on large-area flash.
- **Contrast and legibility checks** for callouts, captions, cursor highlights, and annotations (WCAG 1.4.3 / 1.4.11), with warnings in the editor.
- **Accessible player and packages (HTML5 / SCORM / xAPI):**
  - keyboard- and screen-reader-operable controls with visible focus
  - captions styling, playback speed, transcript, audio description toggle
  - **Practice and Assessment modes use keyboard-operable hotspots and text-entry checks. No click-only interactions.**
  - adjustable or removable time limits (WCAG 2.2.1)
  - correct language, headings, landmarks, and reading order
- **Optional sign-language video track** (picture-in-picture) as a first-class track type.
- **Accessibility checker.** A pre-export report, in the style of an office-suite checker, that lists missing captions, missing alt text, contrast failures, flashing risk, and unlabeled hotspots, with jump-to-fix. Warnings are prominent and defaults favor accessibility, but the author is not blocked from exporting.

### 6.3 Accessibility in the process

- Accessibility acceptance criteria on every user story; no feature merges without keyboard and screen-reader behavior defined.
- **Automated:** axe-core (or equivalent) on every generated HTML/SCORM package in CI; unit tests on the accessibility tree where the UI toolkit allows.
- **Manual:** scripted passes with NVDA, Narrator, VoiceOver, and Orca on the core flows (record → trim → caption → export) before each release.
- **User testing** with people who use assistive technology, from the first usable build.
- Publish an ACR with each release and keep a public accessibility issues list.

## 7. Licensing & dependency policy

The project is **Apache-2.0** and will be part of OpenStage, so every dependency must be one that downstream users can ship under Apache-2.0 without relicensing the whole work.

| Category | Policy | Examples |
|---|---|---|
| **Allowed** | Apache-2.0, MIT, BSD-2/3, ISC, zlib, Unlicense/CC0, dual "MIT OR Apache-2.0" (the Rust norm) | AccessKit*, Skia, wgpu, windows-rs, Opus, libvpx, libaom, SVT-AV1, OpenH264 |
| **Case-by-case** | MPL-2.0 (file-level copyleft; keep modifications isolated); LGPL **only** if dynamically linked, replaceable, and not required for core function | GStreamer plugins (LGPL), FFmpeg LGPL builds, Qt (LGPL tier; weigh against permissive UI options) |
| **Prohibited** | GPL, AGPL, SSPL, any LGPL static linking, "source-available" / non-commercial / proprietary-with-conditions licenses | FFmpeg `--enable-gpl` builds, x264, x265, OBS Studio code, Qt GPL/commercial tiers, **Slint** (offered as royalty-free proprietary / GPLv3 / commercial, none of which fit) |

\* AccessKit is reported as `MIT OR Apache-2.0` by a third-party package listing; confirm against its repository before depending on it.

Practical consequences:

- **Do not bundle FFmpeg.** Prefer **OS-native encoders and capture APIs** (Media Foundation, VideoToolbox / AVFoundation, VA-API) and permissive codecs (OpenH264, VP9, AV1, Opus). If FFmpeg is ever optional, it must be a user-supplied, dynamically loaded, LGPL build, never a default dependency.
- **Codec patents are separate from copyright licenses.** Apache-2.0 grants patent rights only for the project's own contributions. H.264/AAC (and HEVC) carry third-party patent pools. Using the OS encoder or Cisco's pre-built OpenH264 binary shifts this to the platform vendor; VP9/AV1/Opus avoid it. **Needs legal review before shipping.**
- **Housekeeping:** SPDX header on every source file, a `NOTICE` file for third-party attributions, `cargo-deny` license checks in CI, and a decision on **DCO vs. CLA**.
- **Bundled assets** (fonts, icons, cursor sets, sample media, default caption fonts) meet the same bar. Many are CC-BY-SA or SIL OFL; check each. Bundled fonts must also support the languages we caption in.

## 8. Scope by milestone

Accessibility work is spread across every milestone rather than saved for the end, because the hardest parts (the timeline's structural model and the project-format tracks) are architectural.

**MVP (v0.1) — "record, trim, export"**
- Capture: screen/window/region + mic + system audio + webcam (Windows and macOS)
- Interaction logging: cursor position, clicks, key events, active window; opt-in with a clear on-screen and audible indicator
- Project format v0 (documented) with **caption, transcript, alt-text, and language fields from day one**
- Minimal timeline (trim, split, delete, reorder, multi-track) with the **structural, keyboard-complete model** and numeric timecode entry
- **Accessible recorder controls**: global hotkeys, announced state, accessible tray/menu-bar
- Manual caption authoring and import/export of WebVTT/SRT
- Cursor re-styling and click highlights at render time (with flash safeguards)
- Export MP4 (OS encoder) and WebM (VP9/Opus)
- Headless CLI/API: `record`, `render`
- **Toolkit decision made by accessibility spike** (see section 9)

**v1 — "teach with it"**
- Speech-to-text caption generation (pluggable), transcript view
- Auto zoom/pan; callouts, text, arrows, blur/redact
- Auto step segmentation → slide-per-click view, with step text and alt text pre-filled from OS accessibility trees
- Audio description track (TTS or recorded)
- Demo / Practice / Assessment output with keyboard-operable hotspots and text-entry checks
- Accessibility checker pane
- Export: accessible HTML5 package, SCORM 1.2 / 2004, xAPI, cmi5
- Linux capture (PipeWire + xdg-desktop-portal)

**Later**
- Sign-language track, accessible PDF/HTML step guides, templates and themes, noise removal, AI narration, localization, plugin API.

## 9. Technical considerations

**Capture APIs (per platform)**, reached from Rust through platform bindings, each checked against the license policy:
- Windows: Windows Graphics Capture (+ WASAPI loopback for system audio)
- macOS: ScreenCaptureKit (+ AVFoundation); requires Screen Recording and Accessibility permissions
- Linux: PipeWire via xdg-desktop-portal. Wayland deliberately restricts global input capture, so interaction logging on Linux may be limited; plan for a degraded mode.

**Interaction data from accessibility APIs.** UI Automation (Windows), AX (macOS), and AT-SPI (Linux) tell us *which element* was clicked and its name and role. That powers step captions, alt text, and accessible guides, and is optional, best-effort, and permission-gated. It must degrade gracefully, and users must be able to turn it off.

**Proposed crate layout** *(to align with OpenStage's workspace conventions)*:
`openscreen-capture` · `openscreen-project` (format + schema) · `openscreen-timeline` · `openscreen-render` · `openscreen-export` · `openscreen-a11y` (checker, caption/alt-text model) · `openscreen-cli` · `openscreen-ui` (OpenStage integration)

**The UI toolkit is the biggest accessibility decision.** On the web, the browser supplies the accessibility tree. In a native Rust app, the toolkit must. The shared Rust building block is **AccessKit**, which gives toolkits a common accessibility tree and platform adapters for screen readers. Reported screen-reader coverage across Rust toolkits is uneven, and I could not verify per-toolkit status from current sources.

*Recommendation:* if OpenStage already has a UI framework, use it and invest in closing its accessibility gaps through AccessKit. If it does not, **do not choose a toolkit on looks or features.** Run a timeboxed spike: build the same small window (labeled controls, a list, a text field, and a custom timeline-like widget) in each candidate and test with NVDA, Narrator, and VoiceOver before committing. A webview-based shell is a legitimate candidate on accessibility grounds alone, at the cost of a heavier runtime and a split between Rust and web code.

**Project format:** a folder or zip of `project.json` (versioned schema) + media + `events.jsonl`, with caption, audio-description, transcript, alt-text, and language tracks defined in the schema from v0. No proprietary container.

**Hard problems to design early**
1. A/V sync across screen, system audio, mic, and webcam clocks
2. Variable-frame-rate screen capture vs. constant-rate editing
3. Timeline performance with long (60+ minute) recordings
4. Privacy: password fields, notifications, and sensitive windows (exclusion/redaction at capture time); accessibility-tree capture can expose text, so it is opt-in per recording
5. An accessible custom timeline widget (see 6.1)
6. Keeping the interactive player accessible across the SCORM/xAPI variants

## 10. Success criteria

- A first-time user records and exports a 5-minute tutorial in under 10 minutes.
- **A user can complete record → trim → caption → export with the keyboard alone, and separately with a screen reader alone, with no critical blockers found in manual testing on Windows and macOS.**
- **Zero serious or critical axe findings on generated HTML/SCORM packages, plus a passing manual keyboard and screen-reader review of the player.**
- An Accessibility Conformance Report is published with each release.
- Capture overhead stays below ~10% CPU on a mid-range laptop at 1080p30.
- A/V drift under 40 ms over a 60-minute recording.
- A recorded walkthrough converts to a working SCORM package that passes in SCORM Cloud.
- The dependency license check passes in CI on every commit.

## 11. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Native Rust GUI accessibility is uneven across toolkits | Tool is not usable with a screen reader | Toolkit chosen by accessibility spike; AccessKit; test with real AT from the first build |
| The timeline is a custom visual widget | Keyboard and screen-reader users locked out of editing | Structural model first, drag UI second; numeric entry; described preview |
| Permissive-only encoder options are thinner than FFmpeg's | Output format/quality limits | Lean on OS encoders; VP9/AV1 as the portable fallback; document the optional LGPL path |
| Codec patent exposure (H.264/AAC) | Legal | Legal review; default to OS encoders / OpenH264 |
| OS permission flows (macOS Screen Recording / Accessibility) are themselves hard to complete with AT | Authors blocked at first run | Accessible onboarding that explains each permission; test the flow with VoiceOver |
| Wayland limits and platform parity | Degraded UX, Linux gaps | Explicit Linux degraded mode; Windows and macOS first |
| Auto-generated captions and alt text are wrong | Inaccessible content that looks accessible | Always editable; flagged "auto-generated, unreviewed" until a human approves |
| Scope creep toward a full authoring suite | Never ships | Hold the non-goals line; hand off to OpenStage |

## 12. Open questions

1. **What is OpenStage, and what is its UI approach?** (Toolkit, rendering, any existing accessibility work.) This is now the top question, because it determines the accessibility strategy and the integration shape.
2. **Integration shape:** crates linked into OpenStage, a plugin loaded at runtime, or a separate process with an IPC boundary? (Capture in a separate process is attractive for permissions and crash isolation.)
3. **Primary audience for v1:** e-learning authors (SCORM/xAPI first) or general tutorial/video creators (editing first)? This reorders the v1 list.
4. **Platforms for MVP:** Windows + macOS assumed. Is Linux required earlier?
5. **Codec position:** is the project comfortable relying on OS-provided H.264/AAC, or must output be royalty-free codecs only?
6. **Speech-to-text and TTS:** must these run locally with permissive licenses (model weights often carry licenses separate from the code that runs them), or may they call external services? Captions and audio description make this a core dependency, not an extra.
7. **Accessibility targets:** is WCAG 2.2 AA plus ATAG 2.0 the right bar, or is there a contractual or regulatory requirement (Section 508, EN 301 549, ADA, AODA) to track explicitly?
8. **Contribution model:** DCO or CLA?
9. **Name:** see below.

## 13. Naming

"OpenScreen" works as a working title, but it has a collision: the W3C Second Screen Working Group publishes the **Open Screen Protocol**, so searches and docs will be noisy. (I believe Chromium's implementation of it also lives in a repository named `openscreen`; I did not verify that.) The crate names `openscreen`, `open-screen`, and `openstage` were all unregistered on crates.io when checked.

*Recommendation:* since this ships inside OpenStage, prefer an in-family name such as **OpenStage Capture** or **OpenStage Screen** (crate prefix `openstage-`), which avoids the collision and makes the relationship obvious. Keep "OpenScreen" as the working title until you decide. Do a trademark and search check before the name goes public.

## 14. Proposed next steps

1. Answer open questions 1–3; they gate everything else.
2. **Accessibility spike:** the same small test window (labeled controls, list, text field, timeline-like custom widget) in each candidate UI approach, tested with NVDA, Narrator, and VoiceOver.
3. Capture spike: 30 seconds of screen + mic + system audio + cursor events on Windows and macOS, written to the draft project format.
4. Render spike: render that project to MP4 and WebM using only allowed dependencies; confirm the license check passes.
5. Write the project-format spec (v0, including caption/transcript/alt-text/AD tracks) and a `cargo-deny` allowlist.
6. Convert this brief into a PRD with user stories, each carrying accessibility acceptance criteria.
