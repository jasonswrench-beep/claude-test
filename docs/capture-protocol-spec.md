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
openstage-capture render --project <folder> --out <file>        (not built yet)
```

### Exit codes

| Code | Meaning |
|---|---|
| 0 | It worked. |
| 1 | Something went wrong (a `fatal` error event was printed first). |
| 2 | Not available: an unknown source, or a command not built yet (`render`). |

### Options for `record`

| Option | Default | Meaning |
|---|---|---|
| `--project` | required | The capture project folder. Created if it does not exist. |
| `--source` | `synthetic` | Where picture and sound come from. Only `synthetic` (a test source) exists so far. |
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
The answer to the `probe` command. Lists each source and whether it can be used on this computer. `note` is plain language for people.
```json
{"event":"probe","platform":"windows","sources":[{"id":"synthetic","available":true,"note":"A built-in test source."}]}
```

### `state`
The recorder changed state, or is repeating its state (about once a second, so a late reader catches up).
```json
{"event":"state","state":"recording","elapsedMs":1500,"take":1}
```
`state` is one of `idle`, `countdown`, `recording`, `paused`, `stopping`, `stopped`, `error`. `elapsedMs` counts recording time only: **time spent paused is not counted**. During the countdown it is 0. `take` is the take number (starting at 1).

### `level`
A sound level, sent while recording. `source` is `mic` or `system`. `peak` runs from 0.0 (silence) to 1.0 (loudest). `text` is a plain-language description for people who cannot see a meter: `silent`, `quiet`, `good`, `loud`, or `too loud`.
```json
{"event":"level","source":"mic","peak":0.5,"text":"good","elapsedMs":10}
```

### `warning`
Something worth knowing that did not stop the recording.
```json
{"event":"warning","code":"not-paused","message":"Resume only works while paused."}
```
Codes in use: `bad-command`, `not-recording`, `not-paused`, `recover-problem`.

### `error`
Something went wrong. If `fatal` is true, the program is stopping, and it still tries to leave every file complete.
```json
{"event":"error","code":"write-failed","message":"The disk is full.","fatal":true}
```
Codes in use: `write-failed`, `source-unavailable`, `project`, `not-implemented`.

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

### `result`
The last line of a `record` or `recover` run.
```json
{"event":"result","ok":true,"project":"C:\\Courses\\lesson.capture","take":1,"state":"complete","durationMs":12000,"segments":6}
```
`state` is the final state of the take: `complete`, `recovered`, `error` (then `ok` is false), or `none` (the program stopped before any take was started, or `recover` found nothing to repair).

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
