# OpenStage Capture

Screen recording and a mid-level video editor for [OpenStage Author](https://github.com/jasonswrench-beep/openstage-author). Apache-2.0.

This repository holds the design documents and the early code for the Capture module.

## Read these first

| Document | What it is |
|---|---|
| [docs/product-brief.md](docs/product-brief.md) | What is being built and why, and every decision made so far |
| [docs/prd.md](docs/prd.md) | The requirements: user stories with "done when" checklists and accessibility checks |
| [docs/roadmap.md](docs/roadmap.md) | The build order, phase by phase, with checks for each phase |
| [docs/openstage-requests.md](docs/openstage-requests.md) | Everything Capture needs OpenStage Author to add or change, and why |
| [docs/openstage-session-handoff.md](docs/openstage-session-handoff.md) | Step-by-step tasks for the OpenStage session: the rules, roadmap and notice edits (OS-1 to OS-3) and the real-LMS and screen reader testing (OS-16, OS-17) |
| [docs/capture-protocol-spec.md](docs/capture-protocol-spec.md) | How OpenStage and the recording program talk to each other |
| [docs/capture-project-format.md](docs/capture-project-format.md) | What is stored in a capture project folder, and how a crashed recording is recovered |
| [docs/capture-edit-format.md](docs/capture-edit-format.md) | The edit file (`edit.json`): what the author has done to a recording, the editing actions, and the choices to confirm |

## The code

The edit model (tracks, clips, trim, split, join, reorder, undo and redo, in plain words) is in [`capture-edit/`](capture-edit/), written in TypeScript and tested with Node's own test runner; its file format is in [docs/capture-edit-format.md](docs/capture-edit-format.md).

The editing screens are in [`capture-timeline/`](capture-timeline/): so far the clip list (a table of everything in an edit, with keyboard commands to split, trim, delete, join, move and set exact times, each result said in words), with a demo page. It has not been heard with a screen reader yet; its README says what to listen for.

The Rust workspace is in [`capture/`](capture/). See [capture/README.md](capture/README.md) for what is in it and how to build and test it, including double-click scripts for Windows.
