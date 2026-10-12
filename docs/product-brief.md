# Screen Recording & Authoring Tool — Product Brief

**Status:** Draft v0.1 — for review
**License:** Apache-2.0 (hard constraint; see [Licensing & dependency policy](#6-licensing--dependency-policy))
**Context:** Intended to become a component of a larger, separate project

---

## 1. Summary

An open-source screen recorder and lightweight editor aimed at people who make **instructional video and software training** — the space Camtasia occupies for video, and Adobe Captivate / Articulate Storyline occupy for e-learning software simulations.

The core idea: **record the screen and the user's interactions as separate, editable data** (video + cursor, clicks, keystrokes, window/UI-element info), not as flattened pixels. That one decision enables the features that make those products valuable — re-styled cursors, automatic zoom-and-pan, auto-generated step captions, and one-click conversion of a recording into an interactive "show me / let me try / test me" simulation.

## 2. Problem and opportunity

- Camtasia is excellent for video but is closed, paid, and not embeddable.
- Captivate and Storyline do software simulations well but are heavy, expensive, authoring-suite-first tools; their screen capture is a means to an end.
- Open-source recorders (OBS and similar) capture well but have no timeline editing, no interaction data, and no e-learning output; and the most capable ones are GPL, so they can't be embedded in an Apache-2.0 project.
- **Gap:** a permissively licensed, embeddable engine that records interaction data and outputs both video and interactive training.

## 3. Target users and jobs-to-be-done

| User | Job |
|---|---|
| **Instructional designers / L&D** | Turn a software walkthrough into a SCORM/xAPI module with narration, captions, and a practice mode |
| **Educators / course creators** | Record a lecture or tutorial, trim it, add callouts, export to MP4 |
| **Software support / docs teams** | Produce short how-to clips and step-by-step guides that stay easy to update when the UI changes |
| **Host project (TBD)** | Embed recording/editing as a capability inside a larger product |

## 4. Reference products — what we take and what we skip

| Capability | Camtasia | Captivate | Storyline | Our stance |
|---|---|---|---|---|
| Screen + audio + webcam capture | ✔ | ✔ | ✔ (Step-by-step) | **MVP** |
| Multi-track timeline editor | ✔ | basic | basic | **MVP** (simple: trim, split, multi-track) |
| Editable cursor, click effects | ✔ | ✔ | partial | **MVP** — enabled by separate interaction data |
| Auto zoom/pan | ✔ (SmartFocus) | ✔ | — | **v1** |
| Callouts, annotations, shapes | ✔ | ✔ | ✔ | **v1** |
| Captions / transcript | ✔ | ✔ | ✔ | **v1** (pluggable speech-to-text) |
| Auto step-by-step slides from clicks | — | ✔ | ✔ | **v1** |
| Demo / Practice / Assessment modes | — | ✔ | ✔ | **v1** |
| SCORM / xAPI / cmi5 / HTML5 export | partial (quizzes) | ✔ | ✔ | **v1** |
| Branching, variables, full authoring suite | — | ✔ | ✔ | **Non-goal** — hand off to the host project |
| Templates, asset library, themes | ✔ | ✔ | ✔ | Later |

## 5. Goals and non-goals

**Goals**
1. Reliable capture of screen (full / window / region), system audio, microphone, and webcam, with a documented capture-to-file path.
2. Record interaction events in a **documented, open project format** (versioned JSON + media files) so other tools can read and write it.
3. A usable timeline editor — enough to finish a tutorial without leaving the tool.
4. Export to MP4/WebM and to interactive HTML5 / SCORM / xAPI.
5. **Library-first:** the engine is usable headless and from other codebases; any UI is a thin shell over it.
6. 100% Apache-2.0-compatible dependency tree.

**Non-goals (for now)**
- Matching Camtasia's effects/transitions catalog or Storyline's full interaction model.
- Live streaming or multi-camera production (OBS's territory).
- Cloud hosting, review/collab workflows, or an LMS.
- Mobile-device capture.

## 6. Licensing & dependency policy

The project is **Apache-2.0** and will be part of another project, so every dependency must be one that downstream users can ship under Apache-2.0 without licensing the whole work differently.

| Category | Policy | Examples |
|---|---|---|
| **Allowed** | Apache-2.0, MIT, BSD-2/3, ISC, zlib, Unlicense/CC0 | Skia, wgpu, Opus, libvpx, libaom, SVT-AV1, OpenH264, Tauri |
| **Case-by-case** | MPL-2.0 (file-level copyleft; keep modifications isolated), LGPL **only** if dynamically linked, replaceable, and not required for core function | GStreamer plugins (LGPL), FFmpeg LGPL builds, Qt (LGPL tier; weigh against permissive UI options) |
| **Prohibited** | GPL, AGPL, SSPL, any LGPL static linking, "source-available" / non-commercial licenses | FFmpeg `--enable-gpl` builds, x264, x265, OBS Studio code, Qt GPL/commercial tiers |

Practical consequences:

- **Do not bundle FFmpeg.** Prefer **OS-native encoders and capture APIs** (Media Foundation, VideoToolbox / AVFoundation, VA-API) and permissive codecs (OpenH264, VP9, AV1, Opus). If FFmpeg is ever optional, it must be a user-supplied, dynamically loaded, LGPL build — never a default dependency.
- **Codec patents are separate from copyright licenses.** Apache-2.0 grants patent rights only for the project's own contributions. H.264/AAC (and HEVC) carry third-party patent pools. Using the OS encoder or Cisco's pre-built OpenH264 binary shifts this to the platform vendor; VP9/AV1/Opus avoid it. **Needs legal review before shipping.**
- **Housekeeping:** SPDX header on every source file, a `NOTICE` file for third-party attributions, a dependency-license check in CI (e.g. `cargo-deny` / `license-checker`), and a decision on **DCO vs. CLA** for contributions.
- **Bundled assets** (fonts, icons, sample media, cursor sets) must meet the same bar — many are CC-BY-SA or SIL OFL; check each.

## 7. Scope by milestone

**MVP (v0.1) — "record, trim, export"**
- Capture: screen/window/region + mic + system audio + webcam (Windows and macOS)
- Interaction logging: cursor position, clicks, key events, active window (opt-in, with clear on-screen indicator)
- Project format v0 (documented) and a minimal timeline: trim, split, delete, reorder, multi-track
- Cursor re-styling and click highlights at render time
- Export MP4 (OS encoder) and WebM (VP9/Opus)
- Headless CLI/API: `record`, `render`

**v1 — "teach with it"**
- Auto zoom/pan from interaction data; callouts, text, arrows, blur/redact
- Captions via pluggable speech-to-text; subtitle export (SRT/VTT)
- Auto step segmentation → slide-per-click view
- Demo / Practice / Assessment output modes with hotspot and text-entry checks
- Export: HTML5 package, SCORM 1.2 / 2004, xAPI, cmi5
- Linux capture (PipeWire + xdg-desktop-portal)

**Later**
- Templates and themes, noise removal, background removal for webcam, AI narration (TTS), localization, plugin API.

## 8. Technical considerations

**Capture APIs (per platform)**
- Windows: Windows Graphics Capture (+ WASAPI loopback for system audio)
- macOS: ScreenCaptureKit (+ AVFoundation); requires Screen Recording / Accessibility permissions
- Linux: PipeWire via xdg-desktop-portal. Wayland deliberately restricts global input capture, so interaction logging on Linux may be limited — plan for degraded mode.

**Working architecture assumption** *(to confirm against the host project's stack)*
- **Core engine in Rust** — capture, project model, timeline, render/export. Permissive ecosystem, native performance, easy C ABI for embedding.
- **UI shell in TypeScript** (web tech, e.g. Tauri, which is Apache-2.0/MIT) over the engine's API.
- **Renderer on a GPU abstraction** (wgpu) so editor preview and export share one code path.
- **Project format:** a folder or zip of `project.json` (versioned schema) + media + `events.jsonl`; no proprietary container.

**Hard problems to design early**
1. A/V sync across screen, system audio, mic, and webcam clocks
2. Variable-frame-rate screen capture vs. constant-rate editing
3. Timeline performance with long (60+ min) recordings
4. Privacy: password fields, notifications, and sensitive windows (exclusion/redaction at capture time)
5. Mapping clicks to UI elements (Accessibility APIs) for step captions — optional, best-effort

## 9. Success criteria

- A first-time user records and exports a 5-minute tutorial in under 10 minutes.
- Capture overhead stays below ~10% CPU on a mid-range laptop at 1080p30.
- A/V drift under 40 ms over a 60-minute recording.
- A recorded walkthrough converts to a working SCORM package that passes in SCORM Cloud.
- The dependency license check passes in CI on every commit.

## 10. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Permissive-only encoder options are thinner than FFmpeg's | Output format/quality limits | Lean on OS encoders; VP9/AV1 as the portable fallback; document the optional LGPL path |
| Codec patent exposure (H.264/AAC) | Legal | Legal review; default to OS encoders / OpenH264 |
| OS permission and Wayland limits | Degraded UX, Linux gaps | Onboarding flow for permissions; explicit Linux degraded mode |
| Scope creep toward a full authoring suite | Never ships | Hold the non-goals line; hand off to the host project |
| Unknown host-project constraints | Rework | Resolve open questions 1–3 before architecture is locked |

## 11. Open questions

1. **What is the host project, and what is its stack/runtime?** (Desktop app, web app, library, plugin?) This decides the engine language and UI approach.
2. **Desktop-native or browser-based capture?** Browser `getDisplayMedia` is far easier to embed but cannot record system-wide cursor/click data or run outside the tab/window grant.
3. **Primary audience for v1:** e-learning authors (SCORM/xAPI first) or general tutorial/video creators (editing first)? This reorders the v1 list.
4. **Platforms for MVP:** Windows + macOS assumed — is Linux required earlier?
5. **Codec position:** is the project comfortable relying on OS-provided H.264/AAC, or must output be royalty-free codecs only?
6. **Contribution model:** DCO or CLA?
7. **Speech-to-text / TTS:** must these be local and permissively licensed (model weights often carry licenses separate from the code that runs them), or may they call external services?
8. **Name** — working title needed.

## 12. Proposed next steps

1. Answer open questions 1–3 (they gate everything else).
2. Spike: capture 30 s of screen + mic + system audio + cursor events on Windows and macOS, written to the draft project format.
3. Spike: render that project to MP4 and WebM using only allowed dependencies; confirm the license check passes.
4. Write the project-format spec (v0) and a dependency-license allowlist file.
5. Convert this brief into a PRD with user stories and acceptance criteria for the MVP.
