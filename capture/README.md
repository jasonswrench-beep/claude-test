# OpenStage Capture (the recording program)

This folder holds the code for **OpenStage Capture**: the program that records the screen, microphone, and system sound for OpenStage Author. It is free and open source under the Apache License 2.0.

The program is called `openstage-capture`. It has **no window of its own**. OpenStage starts it, tells it what to do, and shows the author what it reports. This keeps the recorder small, safe, and easy to test, and it means the recording controls can be built to be fully accessible inside OpenStage.

## Where this stands (honest status)

**Phase 1, the "walking skeleton," is built.** That means the whole path from "start recording" to "recover a crashed recording" works, using a built-in **fake** source of picture and sound.

| Works and is tested | Not built yet |
|---|---|
| The conversation with OpenStage (the protocol) | Recording a real screen, window, or region into a project |
| The capture project folder and its crash-safe files | Recording your real microphone, system sound, or webcam |
| Countdown, record, pause, resume, stop, markers | Cursor and click effects, interaction capture |
| Start again (the old take is kept, never erased) | Turning a recording into an MP4 (`render`) |
| Killing the program mid-recording and repairing it | The editor panels inside OpenStage |
| Sound levels with plain-language text | Captions, audio description, step-by-step output |
| License and rule checks | Encoding the pictures into video (the next slice) |

**What "tested" means here.** The 99 automated tests pass on Linux. On 2026-10-13 the first 65 were also run on the owner's Windows computer and all passed, including a test that starts the real program, kills it in the middle of a recording, and repairs the recording afterwards. The 34 tests added since then (the take-number fix, the screen capture checks, and the video checks) have passed on Linux and have not yet been run on Windows. On Windows, the fake test source and the real screen capture test (`screen-test`) have been run. Making video, recording sound, and recording a real screen into a project have not, so none of that is claimed to work.

## Phase 2 has started: screen capture

The first Windows-only piece is in `crates/openstage-capture-windows`. It finds your displays and captures one with Windows Graphics Capture (the same Windows service the Snipping Tool uses), then reports in plain words how steadily pictures arrived. It does not record to a project yet.

**The screen capture test has passed on the owner's computer** (Windows 11 build 26200, two displays, about 31 pictures per second while something moved). Whether the saved picture looked right to the eye has not been confirmed yet.

**The video test has never been run.** `TEST VIDEO CAPTURE.bat` records an 8 second MP4 of the main screen through Windows' own video tools and reports processor use against the 10 percent target. Its Windows-only code has been checked by the Rust compiler and linter for Windows, and everything around it is tested here with fakes, but whether Windows' encoders behave on your computer is not known until you run it.

## What to do on Windows (no typing needed)

1. Double-click **`BUILD CAPTURE.bat`**. It checks that Rust is installed (and explains how to install it if not), builds the program, and says plainly whether it worked.
2. Double-click **`TEST CAPTURE.bat`**. It runs every test, then makes a short test recording, and says PASS or FAIL.
3. Double-click **`TEST SCREEN CAPTURE.bat`**. It watches your main screen for 8 seconds (keep a window moving), saves two still pictures of it, opens one, and says PASS or lists what went wrong. The pictures show your screen, so they stay on your computer and you do not need to send them.

**Tip when updating:** extract each new ZIP into a **new, empty folder** rather than over an old one. Extracting over an old folder can skip some files and leave a mix of old and new code, which shows up as a build error that says something was "not found" or a "variant" is missing. (The old folder's build files are not needed; the first build in the new folder just takes a few minutes.)

4. Double-click **`TEST VIDEO CAPTURE.bat`**. It records an 8 second MP4 video of your main screen (no sound yet), opens it, and says PASS or lists what went wrong. The video shows your screen, so it stays on your computer and you do not need to send it.

If any of them fails, send the log file it names (`build-log.txt`, `test-log.txt`, `screen-test-log.txt`, or `video-test-log.txt`, saved in this folder).

## How the pieces fit

```
capture/
  crates/
    openstage-capture-protocol/   the messages between OpenStage and the program
    openstage-capture-project/    the project folder, takes, crash-safe segments, recovery
    openstage-capture/            the engine: the recording state machine and sources
    openstage-capture-windows/    screen capture for Windows, and the checks that prove it works
    openstage-capture-cli/        the program itself (openstage-capture)
  tools/check_rules.py            license, FFmpeg, header, and writing-style checks
  ci/capture.yml                  automatic checks, to be copied into .github/workflows
  THIRD-PARTY.md                  every library built in, with its license (generated)
```

The two format documents are the source of truth. Change them in the same commit as any code that reads or writes the format.

- [Protocol specification](../docs/capture-protocol-spec.md)
- [Project format](../docs/capture-project-format.md)
- [Product brief](../docs/product-brief.md), [requirements](../docs/prd.md), [roadmap](../docs/roadmap.md)

## The program

```
openstage-capture probe                       say what can be recorded on this computer
openstage-capture record --project <folder>   record, taking one-line JSON commands on standard input
openstage-capture recover --project <folder>  repair a recording that was interrupted
openstage-capture screen-test                 try capturing the screen (Windows only)
openstage-capture video-test                  try making an MP4 of the screen (Windows only)
openstage-capture render ...                  not built yet (Phase 3)
```

Events come out on standard output as one JSON object per line, and commands go in on standard input the same way:

```
> {"cmd":"pause"}
< {"event":"state","state":"paused","elapsedMs":4000,"take":1}
```

## For developers

You need Rust (stable) and Python 3.

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -I tools/check_rules.py
```

`tools/check_rules.py` fails the build if a dependency has a license outside the allowlist, if FFmpeg is added as a dependency, if a build flag turns on GPL or non-free code, if a source file lacks its license header, or if prose uses em dashes (a house style rule). `python -I tools/check_rules.py --write-third-party` regenerates `THIRD-PARTY.md`.

## Rules that shape the code

- **A crash costs seconds, not the recording.** Data is written in small checksummed pieces and forced to disk every second.
- **Nothing is erased by the software.** Start again sets a take aside.
- **No GPL, and no bundled FFmpeg.** Everything built in must be permissively licensed.
- **Raw recordings stay out of published courses.** The capture folder lives beside the course, never inside it.
- **Accessibility is built in from the start.** Every state and sound level has a text equivalent, so nothing depends on seeing a screen.

## License

Apache License 2.0. See [`../LICENSE`](../LICENSE).
