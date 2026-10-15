# OpenStage Capture (the recording program)

This folder holds the code for **OpenStage Capture**: the program that records the screen, microphone, and system sound for OpenStage Author. It is free and open source under the Apache License 2.0.

The program is called `openstage-capture`. It has **no window of its own**. OpenStage starts it, tells it what to do, and shows the author what it reports. This keeps the recorder small, safe, and easy to test, and it means the recording controls can be built to be fully accessible inside OpenStage.

## Where this stands (honest status)

**Phase 1, the "walking skeleton," is built:** the whole path from "start recording" to "recover a crashed recording" works, using a built-in **fake** source of picture and sound. **Phase 2, real capture on Windows, is under way.**

| Run on the owner's Windows computer, and working | Built, not yet run on Windows | Not built yet |
|---|---|---|
| The conversation with OpenStage (the protocol) | Repairing the sound files after a crash (tested here with made-up files) | Click-to-choose window picker, hiding chosen windows from the recording |
| The capture project folder and its crash-safe files | Recording a webcam into its own file (`--camera`) | Cursor and click effects, suggested zooms (they will read the interaction log) |
| Countdown, record, pause, resume, stop, markers, start again | Turning a recording or an edit into a finished MP4 (`render`) | The editor panels inside OpenStage |
| Killing the program mid-recording and repairing it | Noting the cursor, clicks, windows, clicked item names and keys (`--interactions`, off unless asked), and reading back and deleting that log (`interactions`) | Captions, audio description, step-by-step output |
| Capturing a real screen, and encoding it as H.264 video | Warning when the microphone hears the speakers (an echo); tested only on made-up sound | |
| A crash during a video recording loses about a second | Long-recording numbers (skipped pictures, processor and memory use, sound against picture), the timing test, and the crash test of the real recorder with sound | Placing sound and picture by their capture times, if the long test shows it is needed |
| The real recorder with a real screen: pause leaves no gap, the saved video reads back at the right length | Starting the screen, sound and video encoder during the countdown, to remove the 1.3 second pause after it | |
| Listening to the microphone and to the computer's own sound; finding which sound output carries the sound | | |
| Recording sound with the screen: mixed into the MP4 and kept apart as `mic.wav` and `system.wav` (the owner reported it worked; the numbers were not seen) | | |
| License and rule checks | | |

**The edit model is separate.** The logic for editing a recording (tracks, clips, trim, split, join, undo) is TypeScript in [`../capture-edit/`](../capture-edit/), with 136 of its own tests; the Rust side here reads and checks the file it writes (`edit.json`), and 4 of the tests above check that both agree on shared examples.

**What "tested" means here.** The 658 automated tests pass on Linux (a Windows computer runs 660, because two tests only exist on Windows). On 2026-10-13 the first 65 were also run on the owner's Windows computer and all passed, including a test that starts the real program, kills it in the middle of a recording, and repairs the recording afterwards. The 593 tests added since then have passed on Linux only. The real Windows parts (screen capture, video, crash safety, the recorder, the microphone and the computer's sound) were run on the owner's computer through the test scripts listed below, and passed. Sound in the recording (the AAC sound track inside the MP4 and the sound files) was also run there, and the owner reported that it worked; the detailed numbers were not seen, and how closely sound and picture stay together has not been measured.

## Phase 2 has started: screen capture

The first Windows-only piece is in `crates/openstage-capture-windows`. It finds your displays and captures one with Windows Graphics Capture (the same Windows service the Snipping Tool uses), then reports in plain words how steadily pictures arrived. It does not record to a project yet.

**The screen capture test has passed on the owner's computer** (Windows 11 build 26200, two displays, about 31 pictures per second while something moved). Whether the saved picture looked right to the eye has not been confirmed yet.

**The video test has passed on the owner's computer** (240 pictures at 30 per second, none skipped, about 51 percent of one processor core, with a graphics card encoder available). Whether the saved video looked right to the eye has not been confirmed yet.

**The crash safety test has passed on the owner's computer:** the recording ran 8.1 seconds, the program was stopped without warning, and 7.0 seconds survived and played with no repair (1.1 seconds lost, inside the 3 second target). So a recording is saved as one standard MP4 written by Windows itself.

**The real recorder has passed on the owner's computer** with a scripted session on a real screen (countdown, markers, a 2 second pause that left no gap, stop), producing `takes/take-001/screen.mp4` that Windows read back at exactly the recording's length. It starts about 1.3 seconds after the countdown ends, which will be fixed. `TEST RECORD SCREEN.bat` runs it.

**Sound capture has passed on the owner's computer:** the microphone (a Shure MV7, chosen from a list of 8) and the computer's own sound (output 9 of 11, found with `FIND SOUND OUTPUT.bat`) were heard together, and the clocks agreed. With speakers instead of headphones, the microphone also hears the speakers and the mixed sound has an echo. That is why the recorder keeps the microphone and the computer's sound as **separate files** as well as the mix, so the editor can fix it later.

**Sound in the recording has been run on the owner's computer, which reported that it worked.** `record --source screen` records the screen, the chosen microphone and the chosen sound output. Each take holds `screen.mp4` (picture plus the two sounds mixed, as AAC) and `mic.wav` and `system.wav` (each sound alone). Choose the devices with `--microphone` and `--output` (a number from `audio-devices`, part of a name, `default`, or `none`). The recorder also makes a short sound for each step (countdown, start, pause, resume, stop, error), never recorded, loud as `--cue-volume` says and off with `--cues off`; these were heard on the owner's Windows computer with `TEST CUES.bat` (all six audible; a seventh sound, which says that what you do is being noted, was added afterwards and has not been heard yet); how they behave inside a real recording has not been reported yet. `--region 100,200,1280,720` records only that rectangle of the display (from the left, from the top, width, height). Left as `default`, the computer's sound follows Windows if it moves to another output during the recording (headphones plugged in or out); a number or name is kept to. Following is built and tested with fake outputs but has not yet been run on Windows.

**Balancing the sound.** The microphone and the computer's sound are saved on their own as `mic.wav` and `system.wav`, exactly as heard and never changed, so they can be balanced later in the editor. The mix inside the MP4 is a plain mix, which can be changed with `--mic-gain-db` and `--system-gain-db` (for example `--system-gain-db -10` makes the computer's sound quieter in the mix; about 6 decibels is half the volume). After a recording the program says how far apart the two were in loudness and what to try. Windows' own MP4 writer holds only one sound stream, which is why the separate tracks are WAV files and not extra tracks inside the MP4. `TEST RECORD SCREEN.bat` tries it.

**What you do with the mouse and keyboard can be noted, but only if you ask.** `record --source screen --interactions no-keys` keeps a private log, in the take's own `events.jsonl`, of where the cursor goes, each click, the title and place of the window in front and the name and kind of the item you click (for example the button Save); `--interactions on` also keeps the keys you press. The default, `off`, keeps nothing. You are told in words and with a distinct four-note sound as each countdown begins, every state report while recording says so, nothing is noted while paused, and with `no-keys` the program does not even install a keyboard hook. Typed keys are never written for a password field, and a letter, digit or symbol is written only when Windows says the keyboard is in an ordinary text field (when it cannot tell, none are written). `openstage-capture interactions --project <folder>` reads the log back in words (the keys are counted, and printed only with `--list`) and `--delete --yes` deletes it, keeping your own markers. This is built and tested here with made-up input only; the part that talks to Windows (the mouse and keyboard hooks and the accessibility information) has never been run, and `TEST INTERACTIONS.bat` is how it gets run. There is no on-screen mark yet; the indicator is the sound, the spoken and written announcement, and the `watching` flag in each state report, which OpenStage will show.

## What to do on Windows (no typing needed)

1. Double-click **`BUILD CAPTURE.bat`**. It checks that Rust is installed (and explains how to install it if not), builds the program, and says plainly whether it worked.
2. Double-click **`TEST CAPTURE.bat`**. It runs every test, then makes a short test recording, and says PASS or FAIL.
3. Double-click **`TEST SCREEN CAPTURE.bat`**. It watches your main screen for 8 seconds (keep a window moving), saves two still pictures of it, opens one, and says PASS or lists what went wrong. The pictures show your screen, so they stay on your computer and you do not need to send them.
4. Double-click **`TEST VIDEO CAPTURE.bat`**. It records an 8 second MP4 video of your main screen (no sound yet), opens it, and says PASS or lists what went wrong. The video shows your screen, so it stays on your computer and you do not need to send it.
5. Double-click **`TEST VIDEO CRASH.bat`**. It records your screen, stops the program without warning (only its own recording process), then repairs the video and tells you how many seconds survived.
6. Double-click **`TEST RECORD SCREEN.bat`**. It asks which microphone and sound output to use, then runs the real recorder through a scripted session (countdown, marker, pause, resume, marker, stop) on your main screen with sound, opens the video it made, and says PASS or lists what went wrong. Play some sound and say a sentence while it runs. The recording shows your screen and holds your voice, so it stays on your computer.
7. Double-click **`FIND SOUND OUTPUT.bat`** while something is playing (a video or some music). It listens to every sound output at the same time for 6 seconds and says which output carries the sound and its number. Nothing is saved.
8. Double-click **`TEST AUDIO CAPTURE.bat`**. It lists your microphones and sound outputs and asks which to use (press Enter for the defaults), then listens to your microphone and to the sound your computer is playing for 8 seconds (play something, and say a sentence), saves what it heard as sound files, plays them, and says PASS or lists what went wrong. When it asks which sound output to use, give the number `FIND SOUND OUTPUT.bat` named. The files contain your voice, so they stay on your computer.
9. Double-click **`TEST SYNC TIMING.bat`**. For 10 seconds it measures how late the picture, your microphone and the computer's sound reach the recorder, and whether Windows' time stamps on them can be trusted. Keep a window moving, play some sound and talk. Nothing is saved.
10. Double-click **`TEST RECORDER CRASH.bat`**. It starts the real recorder with sound, stops it without warning the way a crash would, repairs the recording, and checks the video, the sound in it, and the separate sound files. The test recording is real, so it stays on your computer.
11. Double-click **`TEST LONG RECORDING.bat`**. It records for as long as you choose (10 minutes is a good first try; 60 is the full test), pauses in the middle, and then reports whether the program kept up, whether it grew without limit, and whether sound and picture stayed together. Keep Windows from going to sleep first, or answer Y to the question about being away: the script then keeps Windows awake, plays a gentle looping sound and shows a moving window by itself, so you can leave it for the whole test.
12. Double-click **`TEST OUTPUT SWITCH.bat`**. It records for 70 seconds while you switch the sound to another output twice (plug in headphones, or pick another output in Windows), and reports whether the recording followed the sound. Start some music first.
13. Double-click **`TEST CUES.bat`**. It plays the recorder's own sounds (countdown, start, pause, resume, stop, error) one at a time, saying in words what each is first. Tell me whether you can hear them all and tell them apart.
14. Double-click **`CHOOSE REGION.bat`**. The screen goes dim and you choose the part to record by dragging a rectangle with handles on its corners and sides, or by typing four numbers in a small window (which works with the keyboard alone and a screen reader). It records nothing and saves your choice.
15. Double-click **`TEST REGION.bat`**. It records 8 seconds of just a rectangle of one display (the one you chose, or four numbers you type), shows a thin orange frame just outside it while it records, and opens the video so you can see that only that rectangle is in it.
16. Double-click **`TEST WINDOW.bat`**. It lists the open windows with a number for each, records 10 seconds of just the one you choose (nothing else on the screen is in the video, even if another window is in front of it), and opens the video. Try dragging another window over it, and resizing it.
17. Double-click **`TEST WEBCAM.bat`**. It lists your cameras with a number for each, records about 9 seconds of the screen, your voice and the camera you choose (with a pause in the middle), and opens the webcam video, which is saved as its own file beside the screen video. Look at the webcam video and tell me whether you are the right way up, not mirrored, and smooth. It has never been run on a real camera.
18. Double-click **`TEST EXPORT.bat`**. It records about 12 seconds of your screen and voice (and a webcam if you choose one), then turns the recording into a finished MP4 with a title, a red box, a yellow ellipse, a green arrow, a blurred patch, a blacked-out patch, a zoom and the webcam in the corner, and opens it. Look at the video and tell me what you see. It has never been run on Windows.
19. Double-click **`TEST INTERACTIONS.bat`**. It records about half a minute of your screen while keeping the private interaction log, tells you what to do (move the mouse, click a few things, type a few words in Notepad, try a password box, click while paused), then prints in words everything it noted and asks whether to delete it. It is the first time this part has run on Windows. Do not type anything private: the keys go into the log and onto the screen. The log stays on your computer.
20. Double-click **`CHECK MY COMPUTER.bat`**. It looks at your computer and says, in plain words, whether it is ready to record: the Windows version, screen capture, the video and sound parts MP4 needs, your microphone and its permission, and free disk space. It records nothing.

### A computer where Rust is installed but is not on the PATH

Some computers (a work computer where you are not an administrator, for example) have Rust installed but deliberately not on the PATH, so a plain window says "cargo is not found". Every script in this folder now copes with that, without installing anything and without administrator rights:

* `rust-setup.bat` is called first by every script. If `cargo` is already found, it does nothing. If not, it looks for the helper `C:\Dev\tools\rust-env.ps1` (set the variable `OPENSTAGE_RUST_ENV` to the full path of a helper somewhere else), runs it in PowerShell, and copies the settings it makes (the PATH, INCLUDE and LIB, and a few more) into the script's own window. That lasts only until the script ends. If there is no helper it tries `.cargo\bin` in your user folder. If that fails too, it prints what PowerShell said, so you can send it to me.
* `cargo-run.bat` runs cargo for every build and test and writes everything cargo says to the script's log file. It tries **without downloading anything** first. Rust keeps the libraries it has already fetched in `.cargo\registry` in your user folder, and a computer that has all of them never downloads. If some are missing, it says so and **asks before downloading them**, once, from crates.io. Type D and press Enter to allow it; anything else stops, with nothing downloaded.
* `tools/check_rules.py` now fails if any script calls cargo on its own or forgets `rust-setup.bat`, so a new script cannot quietly skip this.

This was written without that computer and has never been run on it. If the first script says it could not find Rust, or asks to download, send me what the window says.

**Where to extract it:** somewhere that is not synced by OneDrive, such as a folder under `C:\Dev`. The build makes thousands of files (several gigabytes), and OneDrive syncing them slows the build and can lock files in the middle of it.

**Tip when updating:** extract each new ZIP into a **new, empty folder** rather than over an old one. Extracting over an old folder can skip some files and leave a mix of old and new code, which shows up as a build error that says something was "not found" or a "variant" is missing. (The old folder's build files are not needed; the first build in the new folder just takes a few minutes.)

If any of them fails, send the log file it names (`build-log.txt`, `test-log.txt`, `screen-test-log.txt`, `video-test-log.txt`, `video-crash-log.txt`, `record-test-log.txt`, `find-output-log.txt`, `audio-test-log.txt`, `timing-test-log.txt`, `recorder-crash-log.txt`, `choose-region-log.txt`, `window-test-log.txt`, `webcam-test-log.txt`, `export-test-log.txt`, `export-log.txt`, `hands-free-log.txt`, `interactions-test-log.txt`, `interactions-test-list.txt`, or `long-test-log.txt`, saved in this folder).

## How the pieces fit

```
capture/
  crates/
    openstage-capture-protocol/   the messages between OpenStage and the program
    openstage-capture-project/    the project folder, takes, crash-safe segments, recovery
    openstage-capture/            the engine: the recording state machine and sources
    openstage-capture-windows/    screen capture for Windows, and the checks that prove it works
    openstage-capture-render/     the plan for turning an edit into a finished video: what shows at each frame, how zooms move, how sound is mixed (pure arithmetic, no video)
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
openstage-capture record --project <folder>   record (--source screen on Windows), taking one-line JSON commands on standard input
openstage-capture recover --project <folder>  repair a recording that was interrupted
openstage-capture screen-test                 try capturing the screen (Windows only)
openstage-capture video-test                  try making an MP4 of the screen (Windows only)
openstage-capture video-info --file <video>   say how long a saved video is (Windows only)
openstage-capture audio-devices               list the microphones and sound outputs (Windows only)
openstage-capture audio-test                  listen to the microphone and the computer's sound (Windows only)
openstage-capture timing-test                 measure how late picture and sound arrive (Windows only)
openstage-capture recorder-crash-test         kill the real recorder mid-recording and check what survives (Windows only)
openstage-capture find-output                 find which sound output is carrying sound (Windows only)
openstage-capture video-crash-test            record, stop the program without warning, check what survived (Windows only)
openstage-capture render ...                  turn a recording or an edit into a finished MP4 (Windows only)
openstage-capture interactions --project <folder>   read back, in words, what was noted of the mouse and keyboard, and delete it
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
