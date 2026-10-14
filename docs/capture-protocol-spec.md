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
openstage-capture timing-test [options]                            (Windows only; measures how late picture and sound arrive)
openstage-capture recorder-crash-test [options]                    (Windows only; kills the real recorder mid-recording, repairs, checks every file)
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
| `--output` | `default` | Which sound output to listen to, chosen the same way. Windows listens to one output at a time, so these test commands keep to the output they started on. |
| `--plain` | off | Print sentences for a person instead of JSON events. |

The checks, in order: `audio-devices` (every microphone and sound output, numbered, with the default marked), `microphone-start`, `system-audio-start` (each names the device found, or says why listening could not start, with advice for the usual cause), `microphone` and `system-audio` (the loudest moment in words, or "heard nothing" with advice), `audio-clock` (the microphone's sound card clock compared with the computer's: fine within 250 milliseconds), `balance` (only when both sounds were heard: how loud the voice and the computer's sound are next to each other while each is making sound, in the mix inside the video, that is after any `--mic-gain-db` and `--system-gain-db`; it also says what the difference was as heard. It fails with the setting to try when the computer's sound is as loud as the voice or within 3 decibels of it), `echo` (only when both sounds were heard: whether the microphone's loudness follows the computer's, which means it is hearing the speakers; it fails with advice about headphones when an echo is likely, and says plainly when there was too little computer sound to tell), and `audio-saved`. The saved files hold what a recording would hold: each track as long as the real time, with silence filled in where Windows delivered nothing (it sends nothing for a quiet sound output). Exit codes are as for `screen-test`.

### `audio-devices`

Lists the microphones and sound outputs the computer has, numbered within their kind in the order Windows lists them, with the default marked, as one `check` named `audio-devices`. The numbers are what `--microphone` and `--output` take. Exit codes are as for `screen-test`. `probe` also lists them, as `microphone:N` and `output:N`, with `available` true on Windows and a note giving the options to record them (`record --source screen --microphone N`).

A name Windows will not give is not hidden: it is shown as `microphone 1 (its name could not be read: <reason>)` (or `sound output N`), so the numbering and the choice still work. Windows is asked for three different name properties before giving up.

### `find-output`

Windows listens to one sound output at a time, and a computer often has many (speakers, headphones, monitors, virtual devices). `find-output` listens to every sound output at the same time for `--seconds` (default 6, between 1 and 120) while something audible plays, and reports which of them carried sound. It saves nothing. The checks, in order: `audio-devices`, `outputs-tried` (how many outputs it could listen to; ones that cannot be listened to, often virtual, are named but are not a failure unless none could be listened to), and `sound-found` (passes and names each output that carried sound, with its number and level, and says "To record it, choose sound output number N" when exactly one did; fails and asks for something to be playing when none did). Exit codes are as for `screen-test`; it exits 1 when no output carried sound.

### `timing-test`

Measures, for `--seconds` (default 10), how late the picture and the sound reach the program after Windows captured them, and whether the picture and sound clocks agree. Options: `--display` (1), `--microphone` and `--output` (as for `audio-test`), `--plain`. It saves nothing. Windows stamps every picture and every packet of sound with the moment it was captured, on the computer's high resolution clock; the program stamps the moment each arrived. The difference is the "lateness". The checks: `timing-video`, `timing-microphone`, `timing-system-audio` (each says how many readings, the usual lateness, the range, and the lateness that 95 out of 100 readings stayed within; a lateness that is negative or longer than a second means the stamps are not on the computer's clock, and that stream fails with a plain message), `timing-video-stamps` (only if some pictures had no capture time), and `timing-sync`, one for the microphone and one for the computer's sound: how much later or earlier than the picture it belongs with that sound arrives. In a recording placed by arrival that is how far out of step they would be. Passing is within 40 milliseconds. It fails with advice (place picture and sound by their capture times) otherwise. Exit codes are as for `screen-test`.

### `recorder-crash-test`

Starts the real recorder (`record --source screen`) with the chosen microphone and sound output, records through a 2 second pause, kills it without warning after `--seconds-before-kill` (default 12, at least 8), then repairs the project the way `recover` does and looks at every file. Options: `--display`, `--microphone`, `--output` (a number, part of a name, `default`, or `none`), `--out` (default `recorder-crash-output`), `--plain`. The recording is real, so the folder is private. The recorder's own diagnostic notes are saved in `recorder-diagnostics.txt` there. The checks: `crash-test` (what was chosen, then how long it recorded), `crash-recover` (the take was recovered; anything repair reported), `crash-video` (Windows reads the repaired video), `crash-video-sound` (it has a sound track), `crash-microphone-file` and `crash-system-file` (each WAV file plays), `crash-loss` (the shortest saved file against the recording time: fine within 3 seconds, the product allows 10), and `crash-in-step` (the saved files within 2.5 seconds of each other in length). Exit codes are as for `screen-test`.

### `cue-test`

Plays the recorder's own cue sounds one at a time, in the order a recording uses them, and says in words what each is before it sounds. Options: `--volume` (0 to 100, default 70), `--out` (default `cue-test-output`: each sound is also saved there as a small WAV file, so it can be opened in any player; this works on every system), `--plain`. The checks: `cue-volume`, then for each of `cue-countdown`, `cue-start`, `cue-pause`, `cue-resume`, `cue-stop` and `cue-error` a sentence ("Next: the start sound. It is three rising notes... It lasts 340 milliseconds.") sent before the sound, and `cue-files`. A sound Windows would not play is a failed check (`cue-<name>-played`). Exit codes are as for `screen-test`; on a system that is not Windows the files are still written and the exit code is 2.

### `setup-check`

Looks at the computer and says whether it is ready to record (story A9). It records nothing, asks Windows for no permission, and sends nothing anywhere. Options: `--dir` (default the current folder: where recordings will be kept, whose drive's free space is reported), `--plain`. The checks, in order: `windows-version` (Windows 11 or later), `screen-capture` (Windows' screen capture is available) and `displays`, `video-encoder` (an H.264 encoder exists, hardware or software) and `sound-encoder` (an AAC encoder exists); both of these name the Media Feature Pack when missing, because some "N" editions of Windows come without Windows' media parts; `microphone` and `sound-output` (found, with the defaults named; a missing one is reported but is not a failure, because a recording can be made without a voice or without the computer's sound), `microphone-access` (fails when the Windows privacy settings block microphone use for desktop programs, with the setting to change; passes with a note when Windows does not say) and `disk-space` (fails below 2 gigabytes free; otherwise says about how many hours that is, at about 5 gigabytes an hour). Exit codes are as for `screen-test`. `CHECK MY COMPUTER.bat` runs it.

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
| `--region` | none | With `--source screen`: record only part of the display, as four whole numbers in pixels from the display's top left corner: how far from the left, how far from the top, the width and the height, for example `100,200,1280,720` (commas or spaces). It is checked before the countdown: a region that cannot be read, reaches past the display, or is smaller than 64 by 64 is refused with `bad-option` and exit code 2, with the reason in words. An odd width or height is made one pixel smaller (H.264 needs an even size) and a `region` check says so. The part is cut out of each picture before it is encoded. `take.json` records it (see the project format, `region`). |
| `--fps` | 30 | With `--source screen`: pictures per second in the video. |
| `--bitrate-kbps` | 8000 | With `--source screen`: video quality, in kilobits per second. |
| `--microphone` | `default` | With `--source screen`: which microphone to record: its number from `audio-devices`, part of its name (capital letters do not matter), `default`, or `none` to record no microphone. |
| `--mic-gain-db` | 0 | With `--source screen`: make the microphone louder (positive) or quieter (negative) by this many whole decibels in the mix inside the video (from -40 to 20; about 6 decibels is double or half the volume). `mic.wav` is never changed. |
| `--strict` | off | Count the summary findings (see `run-*` below) as part of passing or failing, for long test recordings. Without it they are only reported. |
| `--system-gain-db` | 0 | The same for the computer's sound, for example `-10` to make it quieter. `system.wav` is never changed. A number outside the range is refused with `bad-option` and exit code 2. |
| `--output` | `default` | With `--source screen`: whose sound to record: the sound that a sound output is playing (the computer's own sound, including a screen reader's speech). Chosen the same way; `none` records none of it. Left as `default`, the recording follows Windows when it moves the sound to another output during the take (headphones plugged in or out): it asks Windows every 250 ms which output is the default and moves when two looks agree, or at once when the output being listened to breaks, ending the take only if no other output appears within 5 seconds. A number or part of a name is kept to, with an `output-changed` warning afterwards if Windows moved the sound. |
| `--cues` | `on` | Whether the recorder makes a short sound for each step, so an author who cannot see the screen always knows what it is doing (story A4). `on` or `off`; anything else is refused with `bad-option` and exit code 2. See "The cue sounds" below. OpenStage may pass `off` if it plays sounds of its own. |
| `--cue-volume` | 70 | How loud the cue sounds are, 0 (silent) to 100. A number above 100 is refused. |
| `--plain` | off | Print sentences for a person instead of JSON events. No `hello` line is printed in this mode. |
| `--demo-script` | none | Instead of reading commands on standard input, follow a script such as `pause@7000;resume@9000;marker:look here@11000;stop@13000`: commands with times in milliseconds from the start of the session (the countdown included). For demos and tests. |
| `--countdown-ms` | 3000 | Countdown before recording starts. 0 turns it off. With `--source screen` the countdown is also when the program gets ready: it starts the screen capture and the sound and wakes the video encoder (by making and throwing away a small file in the computer's temporary folder), so that recording starts when the countdown ends. A problem with the screen or a sound device is therefore reported when the countdown starts, before any take exists. With no countdown, all of that happens at the start of the take instead. |
| `--segment-ms` | 2000 | Recording time per segment file. |
| `--flush-ms` | 1000 | How often data is forced to disk. A crash loses at most this much. |
| `--duration-ms` | none | Stop by itself after this much recording time (tests and demos). |
| `--ignore-stdin-eof` | off | Keep recording if standard input closes. By default, closing standard input stops the recording cleanly. |
| `--title` | empty | Title stored in a new project. |
| `--language` | `en-US` | Language stored in a new project. |

### The cue sounds

With `--cues on` (the default) the recorder makes a short sound for each step, itself, because OpenStage's own announcements may not be heard while another program is in front. Each is under 400 milliseconds and plays through the computer's default sound output at `--cue-volume`. They are told apart by shape, not only by pitch:

| Sound | What it is | What it sounds like | Length |
|---|---|---|---|
| countdown | once a second from the start of the countdown | one very short high beep | 50 ms |
| start | the countdown is over; recording begins when it ends | three rising notes, the last longer | 340 ms |
| pause | recording is paused | two falling notes | 240 ms |
| resume | recording carries on | two rising notes | 240 ms |
| stop | the recording is finished and closed | three falling notes, the last low and long | 340 ms |
| error | something went wrong and the recording ended | two low, rough buzzes | 340 ms |

Rules the program keeps:

* **No cue is ever played while recording time is running**, so none is recorded. The countdown beeps play during the countdown; the start sound plays at the end of the countdown and recording begins only after it has ended (plus 60 ms); the pause sound plays once the recording is paused; the resume sound plays while still paused and recording carries on only after it has ended; the stop and error sounds play after every file has been closed. Whatever the microphone and the computer's sound captured meanwhile is thrown away, as at any start-up.
* Waiting for the start and resume sounds is not recording time. `--countdown-ms` is the time before the start sound, so recording begins about 0.4 seconds after the countdown ends. With `--cues off` there is no such wait.
* Starting again (`restart`) counts down again, with beeps, once the old take is closed. A marker makes no sound. Stopping during the countdown plays the stop sound and no start sound.
* A cue that cannot be played never stops a recording. The `state` events remain the primary signal; the sounds are an addition, and `cue-test` lets a person hear them without recording.
* Before the countdown, `record --source screen` sends one `check` named `cues` saying that the sounds are on and at what volume.

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
Codes in use: `bad-command`, `not-recording`, `not-paused`, `recover-problem`, and `slow-save` (saving to the disk took 100 milliseconds or more just now, with how long; a slow disk can make the video stutter), and `track-silent` (a sound track of a finished recording holds only silence: sent after the recording, with advice that fits the microphone or the computer's sound), `output-changed` (after a recording with the computer's sound on an output chosen by number or name: Windows now sends sound to a different sound output than the one chosen, so the rest of the sound may be missing; not sent when the output was left as the default, which is followed), `sound-went-quiet` (the computer's sound was heard but then silent for the last minute or more of the recording, with when it went quiet; this is how a move to an output that was not listened to looks), `echo-likely` (the microphone seems to be hearing the speakers), `echo-unchecked` (the two files could not be read to look for one), `balance-off` (the computer's sound is as loud as the voice or louder, with the change to try), and `balance-unchecked`.

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
One finding from a test or setup check, in plain language. Sent by `screen-test`, `video-test`, `video-crash-test`, `audio-test`, `find-output`, `timing-test`, `recorder-crash-test`, and `video-info`; and by `record --source screen`, which sends `region` before the countdown when `--region` was given (which part of the display, and anything that was changed, in words), `cues` (when the sounds are on), `sound-devices` before the countdown (which microphone and which sound output were chosen, in words) and, just before `result`, the readback checks: `video-size` (with a region: the pictures in the saved video must be exactly the region's size), `video-readback` (Windows opens the saved video the way a player would and the length it reads must match the recording time to within 0.7 seconds), `audio-readback` (the same for the video's sound track, sent only when sound was recorded), and `microphone-file` and `system-sound-file` (the separate sound files hold sound as long as the recording, to within 0.3 seconds, and the loudest moment is said in words), then `balance` and `echo` (the same two checks as in `audio-test`, from the two saved files; it never changes the exit code, because an echo is for the author to know, not a fault of the recorder). Last come the summary findings, from counters the recorder keeps while it works: `run-summary` (length, pictures written, steps of the steady 30 pictures a second that had to be skipped because the program was late, pictures Windows threw away while recording, how many it delivered in all and the most that were waiting at one look; fails above 1 percent skipped), `run-cpu` (processor use as a share of one core and of this computer, with a projection for a 4-core laptop; fails above 10 percent of this computer), `run-stalls` (how often the program went more than 100 ms without looking at the screen and the sound, the longest wait, and for the worst few what the program had just been busy with (the picture, the sound, saving to disk, and of the sound's time how much was writing sound into the video file); fails if any wait was longer than 250 ms or there was more than one wait over 100 ms a minute), `run-memory` (memory at the start and the most used; fails if it grew by more than 300 megabytes, which would be a leak), and `run-sound-microphone` and `run-sound-system` (how far each sound card ran from the computer's clock, what the program corrected, and at worst how much later or earlier than its picture the sound played, and when; fails beyond 40 milliseconds. The computer's sound is judged only on playing late: a quiet computer sends no sound at all, which looks the same as sound arriving early. When the recording followed Windows to another sound output, `run-sound-system` also lists each move with its time, and it fails if a move could not be made). These only decide the exit code with `--strict`. While recording, the program also writes a line about memory and processor use to standard error once a minute. With `--source screen` the program looks at the screen and the sound every 5 milliseconds (the default for other sources is 20): a steady 30 pictures a second needs a look at least every 33 milliseconds, and Windows' sleeping is coarse enough that a 20 millisecond wait can become 30 or 45. The setup checks of the recorder will use it too.
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
