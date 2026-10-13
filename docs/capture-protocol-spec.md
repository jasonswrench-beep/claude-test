# OpenStage Capture: Protocol Specification

**Status:** v0.1 (matches the code in `capture/crates/openstage-capture-protocol`)
**Protocol version string:** `0.1`

This page describes how OpenStage talks to the recording program, `openstage-capture`. The program has no window of its own. OpenStage starts it, tells it what to do, and shows the author what it reports. If this page and the code ever disagree, that is a bug; fix both in the same change.

## 1. The shape of the conversation

- **One JSON object per line, in both directions.** A line ends with a newline (`\n`). A carriage return before it (`\r\n`) is also accepted.
- **Events** go from the program to OpenStage, on the program's **standard output**.
- **Commands** go from OpenStage to the program, on the program's **standard input**.
- **Logs and diagnostics** go to **standard error**. Standard output carries events and nothing else.
- **Longest line:** 64 KiB (65,536 bytes), in both directions. A longer command is ignored and answered with a warning. The program never keeps an over-long line in memory.
- **Text encoding:** UTF-8.
- **Names:** field names use camelCase. Event and command names are camelCase too.

### Rules that let the protocol grow

1. The **first line** the program prints is always a `hello` event. A reader checks `protocol` before trusting anything else.
2. A reader **must ignore event types it does not know** and **fields it does not know**.
3. The program **ignores extra fields** in commands. A command it does not understand gets a `bad-command` warning, never a crash.
4. While recording, the **last line** is a `result` event, and then the program exits.
5. A change that older readers could misread bumps the protocol version.

## 2. Starting the program

```
openstage-capture probe
openstage-capture record --project <folder> [options]
openstage-capture recover --project <folder>
openstage-capture audio-devices [--plain]                           (Windows only; lists the microphones and sound outputs)
openstage-capture audio-test [options]                           (Windows only; listens to the microphone and the computer's sound)
openstage-capture find-output [--seconds N] [--plain]                (Windows only; finds which sound output is carrying sound)
openstage-capture video-info --file <video>                          (Windows only; says how long a saved video is)
openstage-capture screen-test [options]                          (Windows only; tries screen capture)
openstage-capture video-test [options]                           (Windows only; tries making an MP4 of the screen)
openstage-capture video-crash-test [options]                     (Windows only; records, kills the program, checks what survived)
openstage-capture render --project <folder> --out <file>        (not built yet)
```

### Exit codes

| Code | Meaning |
|---|---|
| 0 | It worked. |
| 1 | Something went wrong (a `fatal` error event was printed first). |
| 2 | Not available: an unknown source, a command not built yet (`render`), or `screen-test` on a computer that is not running Windows. |

### Options for `audio-test`

Listens to the default microphone and to what the computer is playing (the default sound output, including a screen reader's speech) and reports each finding as a `check` event. It records no picture and does not make a capture project. Sound is requested from Windows as 16-bit stereo at 48,000 samples per second; Windows converts from what the device really uses.

| Option | Default | Meaning |
|---|---|---|
| `--seconds` | 8 | How long to listen (between 1 and 600). |
| `--out` | `audio-test-output` | Folder for `mic.wav`, `system.wav`, and `both.wav`. They contain your voice and what the computer played, so they are private. |
| `--microphone` | `default` | Which microphone: its number from `audio-devices`, part of its name (capital letters do not matter), or `default`. |
| `--output` | `default` | Which sound output to listen to, chosen the same way. Windows listens to one output at a time. |
| `--plain` | off | Print sentences for a person instead of JSON events. |

The checks, in order: `audio-devices` (every microphone and sound output, numbered, with the default marked), `microphone-start`, `system-audio-start` (each names the device found, or says why listening could not start, with advice for the usual cause), `microphone` and `system-audio` (the loudest moment in words, or "heard nothing" with advice), `audio-clock` (the microphone's sound card clock compared with the computer's: fine within 250 milliseconds), `balance` (only when both sounds were heard: how loud the voice and the computer's sound are next to each other while each is making sound; it fails with a number to try when the computer's sound is as loud as the voice or louder), `echo` (only when both sounds were heard: whether the microphone's loudness follows the computer's, which means it is hearing the speakers; it fails with advice about headphones when an echo is likely, and says plainly when there was too little computer sound to tell), and `audio-saved`. The saved files hold what a recording would hold: each track as long as the real time, with silence filled in where Windows delivered nothing (it sends nothing for a quiet sound output). Exit codes are as for `screen-test`.

### `audio-devices`

Lists the microphones and sound outputs the computer has, numbered within their kind in the order Windows lists them, with the default marked, as one `check` named `audio-devices`. The numbers are what `--microphone` and `--output` take. Exit codes are as for `screen-test`. `probe` also lists them, as `microphone:N` and `output:N`, with `available` true on Windows and a note giving the options to record them (`record --source screen --microphone N`).

A name Windows will not give is not hidden: it is shown as `microphone 1 (its name could not be read: <reason>)` (or `sound output N`), so the numbering and the choice still work. Windows is asked for three different name properties before giving up.

### `find-output`

Windows listens to one sound output at a time, and a computer often has many (speakers, headphones, monitors, virtual devices). `find-output` listens to every sound output at the same time for `--seconds` (default 6, between 1 and 120) while something audible plays, and reports which of them carried sound. It saves nothing. The checks, in order: `audio-devices`, `outputs-tried` (how many outputs it could listen to; ones that cannot be listened to, often virtual, are named but are not a failure unless none could be listened to), and `sound-found` (passes and names each output that carried sound, with its number and level, and says "To record it, choose sound output number N" when exactly one did; fails and asks for something to be playing when none did). Exit codes are as for `screen-test`; it exits 1 when no output carried sound.

### `video-info`

Opens a saved video the way a player would and reports how many pictures and seconds it holds, as one `check` named `video-info`. Exit codes are as for `screen-test`.

### Options for `screen-test`

Tries screen capture on this computer and reports each finding as a `check` event. It does not make a capture project.

| Option | Default | Meaning |
|---|---|---|
| `--display` | 1 | Which display to capture. Display 1 is the main display; the others follow left to right. |
| `--seconds` | 5 | How long to capture (between 0.5 and 600). |
| `--out` | `screen-test-output` | Folder for `first.bmp` and `last.bmp`, the first and last pictures. They show the screen, so they are private. |
| `--with-cursor` | off | Draw the mouse pointer in the pictures. Off by default because the pointer is recorded separately, so it can be enlarged or highlighted when editing. |
| `--plain` | off | Print sentences for a person (`OK: ...` or `PROBLEM: ...`) instead of JSON events. No `hello` line is printed in this mode. |

The checks, in order: `windows-version`, `capture-support`, `displays`, `screen-capture`, `frame-rate`, `pictures-saved`. It stops at the first check that makes the rest meaningless (for example, no screen capture support). Exit code 0 means every check was fine, 1 means at least one found a problem.

Windows sends a new picture only when something on the screen changes, so a still screen gives few pictures. The `frame-rate` check therefore needs something moving on screen, and says so when it reports a low rate.

### Options for `video-test`

Records a short MP4 video of one display with Windows' own video tools and reports each finding as a `check` event. It records no sound and does not make a capture project.

| Option | Default | Meaning |
|---|---|---|
| `--display` | 1 | Which display to record (1 is the main display). |
| `--seconds` | 8 | How long to record (between 1 and 600). |
| `--fps` | 30 | Pictures per second in the video. The screen only sends a picture when something changes, so the newest picture is repeated to keep the rate steady. |
| `--bitrate-kbps` | 8000 | Video quality, in kilobits per second. |
| `--fragmented` | off | Write a crash-safe fragmented MP4 (`test_fragmented.mp4`): a setup box and then short, self-contained fragments, about one a second, so a file cut off by a crash still plays up to the last whole fragment. |
| `--out` | `video-test-output` | Folder for `test.mp4` (or `test_fragmented.mp4`). The video shows the screen, so it is private. |
| `--plain` | off | Print sentences for a person instead of JSON events. No `hello` line is printed in this mode. |

The checks, in order: `media-foundation`, `h264-encoder`, `video-encode`, `video-readback` (Windows opens the finished file the way a player would and counts the pictures; they must match what was written, within 5 percent), `cpu-use`, `mp4-file`. It stops at the first check that makes the rest meaningless. `video-encode` is not fine if more than 10 percent of the beats were skipped because the computer could not keep up. `cpu-use` is not fine above 10 percent of the computer's total processing capacity (the product's target for 1080p at 30 pictures per second). Exit code 0 means every check was fine, 1 means at least one found a problem, 2 means the computer is not running Windows.

### Options for `video-crash-test`

Tests crash safety on this computer. It starts a recording of one display as a fragmented MP4 (the same as `video-test --fragmented`), waits until the file appears, lets it record, then **kills the recording program without warning** (the way a crash or a power cut would), and looks at what is left. Only the test's own recording process is stopped.

| Option | Default | Meaning |
|---|---|---|
| `--display` | 1 | Which display to record. |
| `--seconds-before-kill` | 8 | How long to let it record before the kill (between 2 and 60). |
| `--out` | `video-crash-output` | Folder for `test_fragmented.mp4`. It shows the screen, so it is private. |
| `--plain` | off | Print sentences for a person instead of JSON events. |

The checks, in order: `crash-test` (the recording ran and was killed), `crash-scan` (how many complete fragments the file holds and whether it ends cleanly), `crash-read-as-is` (whether Windows can play the file exactly as the crash left it), `crash-repair` (cutting off only the unfinished last piece), `crash-read-after-repair`, and `crash-loss` (how many seconds of recording were lost; fine if no more than 3, while the product allows at most 10). The recorded time is measured from the moment the recording program signals that recording began (it creates a small marker file, `recording-started.txt`, in the output folder), to the kill, so it is accurate to about a tenth of a second. Exit codes are as for `screen-test`.

### Options for `record`

| Option | Default | Meaning |
|---|---|---|
| `--project` | required | The capture project folder. Created if it does not exist. |
| `--source` | `synthetic` | Where picture and sound come from: `screen` (your real screen, with sound, Windows only) or `synthetic` (a built-in test source that makes fake picture and sound and works anywhere). |
| `--display` | 1 | With `--source screen`: which display (1 is the main display). |
| `--fps` | 30 | With `--source screen`: pictures per second in the video. |
| `--bitrate-kbps` | 8000 | With `--source screen`: video quality, in kilobits per second. |
| `--microphone` | `default` | With `--source screen`: which microphone to record: its number from `audio-devices`, part of its name (capital letters do not matter), `default`, or `none` to record no microphone. |
| `--mic-gain-db` | 0 | With `--source screen`: make the microphone louder (positive) or quieter (negative) by this many whole decibels in the mix inside the video (from -40 to 20; about 6 decibels is double or half the volume). `mic.wav` is never changed. |
| `--system-gain-db` | 0 | The same for the computer's sound, for example `-10` to make it quieter. `system.wav` is never changed. A number outside the range is refused with `bad-option` and exit code 2. |
| `--output` | `default` | With `--source screen`: whose sound to record: the sound that a sound output is playing (the computer's own sound, including a screen reader's speech). Chosen the same way; `none` records none of it. Windows listens to one output at a time. |
| `--plain` | off | Print sentences for a person instead of JSON events. No `hello` line is printed in this mode. |
| `--demo-script` | none | Instead of reading commands on standard input, follow a script such as `pause@7000;resume@9000;marker:look here@11000;stop@13000`: commands with times in milliseconds from the start of the session (the countdown included). For demos and tests. |
| `--countdown-ms` | 3000 | Countdown before recording starts. 0 turns it off. |
| `--segment-ms` | 2000 | Recording time per segment file. |
| `--flush-ms` | 1000 | How often data is forced to disk. A crash loses at most this much. |
| `--duration-ms` | none | Stop by itself after this much recording time (tests and demos). |
| `--ignore-stdin-eof` | off | Keep recording if standard input closes. By default, closing standard input stops the recording cleanly. |
| `--title` | empty | Title stored in a new project. |
| `--language` | `en-US` | Language stored in a new project. |

## 3. Events (program to OpenStage)

Every event has an `event` field that names its type.

### `hello`
Always the first line.
```json
{"event":"hello","protocol":"0.1","program":"openstage-capture","programVersion":"0.1.0","platform":"windows"}
```

### `probe`
The answer to the `probe` command. Lists each source and whether it can be used on this computer. `note` is plain language for people. On Windows, each real display is listed as `display:1`, `display:2`, and so on, with `available` true and a note giving the command to record it (`record --source screen --display N`).
```json
{"event":"probe","platform":"windows","sources":[{"id":"synthetic","available":true,"note":"A built-in test source."}]}
```

### `state`
The recorder changed state, or is repeating its state (about once a second, so a late reader catches up).
```json
{"event":"state","state":"recording","elapsedMs":1500,"take":1}
```
`state` is one of `idle`, `countdown`, `recording`, `paused`, `stopping`, `stopped`, `error`. `elapsedMs` counts recording time only: **time spent paused is not counted**. During the countdown it is 0. `take` is the take number (starting at 1) that the report belongs to: during a countdown it is the take about to begin, and in `stopped` and `error` it is the take just finished. It is 0 only if the program stopped before any take began.

### `level`
A sound level, sent while recording about twice a second: the loudest moment since the last one. `source` is `mic` (the microphone) or `system` (the computer's own sound). `peak` runs from 0.0 (silence) to 1.0 (loudest). `text` is a plain-language description for people who cannot see a meter: `silent`, `quiet`, `good`, `loud`, or `too loud`.
```json
{"event":"level","source":"mic","peak":0.5,"text":"good","elapsedMs":10}
```

### `warning`
Something worth knowing that did not stop the recording.
```json
{"event":"warning","code":"not-paused","message":"Resume only works while paused."}
```
Codes in use: `bad-command`, `not-recording`, `not-paused`, `recover-problem`, and `track-silent` (a sound track of a finished recording holds only silence: sent after the recording, with advice that fits the microphone or the computer's sound), `echo-likely` (the microphone seems to be hearing the speakers), `echo-unchecked` (the two files could not be read to look for one), `balance-off` (the computer's sound is as loud as the voice or louder, with the change to try), and `balance-unchecked`.

### `error`
Something went wrong. If `fatal` is true, the program is stopping, and it still tries to leave every file complete.
```json
{"event":"error","code":"write-failed","message":"The disk is full.","fatal":true}
```
Codes in use: `write-failed` (a file could not be written), `media-failed` (the screen or the video encoder failed; the message says why), `source-unavailable`, `bad-demo-script`, `project`, `not-implemented`.

### `marker`
A marker was added at this point of the recording.
```json
{"event":"marker","label":"oops","elapsedMs":4200}
```

### `takeSetAside`
The author started again. The old take is **set aside, not erased**, and a new one begins.
```json
{"event":"takeSetAside","take":1,"nextTake":2}
```

### `recovered`
`recover` repaired an interrupted take. `maxLossMs` is the most recording time the repair could have lost (the flush interval).
```json
{"event":"recovered","take":1,"durationMs":4000,"maxLossMs":1000,"segments":3}
```

### `check`
One finding from a test or setup check, in plain language. Sent by `screen-test`, `video-test`, `video-crash-test`, `audio-test`, `find-output`, and `video-info`; and by `record --source screen`, which sends `sound-devices` before the countdown (which microphone and which sound output were chosen, in words) and, just before `result`, the readback checks: `video-readback` (Windows opens the saved video the way a player would and the length it reads must match the recording time to within 0.7 seconds), `audio-readback` (the same for the video's sound track, sent only when sound was recorded), and `microphone-file` and `system-sound-file` (the separate sound files hold sound as long as the recording, to within 0.3 seconds, and the loudest moment is said in words), then `balance` and `echo` (the same two checks as in `audio-test`, from the two saved files; it never changes the exit code, because an echo is for the author to know, not a fault of the recorder). The setup checks of the recorder will use it too.
```json
{"event":"check","check":"screen-capture","ok":true,"message":"Received 151 pictures in 5.0 seconds from Display 1, 1920 by 1080, main display. Each picture is 1920 by 1080 pixels."}
```
`check` names what was checked, `ok` says whether it was fine, and `message` says what that means for a person.

### `result`
The last line of a `record` or `recover` run.
```json
{"event":"result","ok":true,"project":"C:\\Courses\\lesson.capture","take":1,"state":"complete","durationMs":12000,"segments":6}
```
`segments` is how many media files the take holds. `state` is the final state of the take: `complete`, `recovered`, `error` (then `ok` is false), or `none` (the program stopped before any take was started, or `recover` found nothing to repair).

## 4. Commands (OpenStage to program)

Every command has a `cmd` field.

| Command | Example | What it does |
|---|---|---|
| `pause` | `{"cmd":"pause"}` | Stop recording time without ending the take. Only valid while recording. |
| `resume` | `{"cmd":"resume"}` | Continue after a pause. Only valid while paused. |
| `stop` | `{"cmd":"stop"}` | Finish cleanly: make every file complete, then print `result`. |
| `restart` | `{"cmd":"restart"}` | Start again: set this take aside (never erased) and begin a new one with the same settings. Only valid while recording. |
| `marker` | `{"cmd":"marker","label":"oops"}` | Add a marker. `label` is optional. Only valid while recording. |

A command used in the wrong state gets a `warning`, and nothing else changes.

## 5. What happens when things go wrong

| Situation | What the program does |
|---|---|
| OpenStage closes standard input (it quit or crashed) | Stops cleanly and finishes the take. (Unless `--ignore-stdin-eof`.) |
| Nobody is reading standard output any more | Finishes the take cleanly and exits. |
| A write to disk fails (disk full, drive removed) | Prints a fatal `write-failed` error, completes what it can, exits with code 1. |
| The program is killed or the computer loses power | The take stays in state `recording`. `recover` repairs it later and loses at most `--flush-ms` of recording. |
| A command line is empty | Ignored. |
| A command line is not valid, or too long | A `bad-command` warning. Recording continues. |

## 6. What OpenStage must do

- Start the program with standard input and standard output **piped**, and read standard output **as it arrives**, not only at the end. (This is request OS-4 in [openstage-requests.md](openstage-requests.md).)
- Read standard error too, or send it to a log, so the pipe never fills.
- Never rely on the order of fields in a line.
- Show every state change and level in text, not only visually (the recorder panel is built that way).
