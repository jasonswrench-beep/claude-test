# OpenStage Capture: Project Format

**Status:** v0.3 (matches the code in `capture/crates/openstage-capture-project`)
**Format name:** `openstage-capture-project`
**Schema version:** `0.2.2` (projects at `0.1.x`, `0.2.0` and `0.2.1` still open unchanged: see section 7)

A recording lives in its own folder, named `something.capture`, **beside** the course (the `.osproj`), never inside it. This matters: OpenStage's publisher zips the project into `source/`, and raw footage and the interaction log (which can contain private information) must never ship to learners. Only the exported, edited video becomes part of the course.

## 1. Layout

```
lesson.capture/
  capture.json                  the manifest (small, rarely changes)
  takes/
    take-001/                   a real screen recording
      take.json                 the only record of this take's state; lists its media files
      events.jsonl              what the author did while recording (append-only)
      screen.mp4                the picture and the mixed sound, as one crash-safe fragmented MP4
      mic.wav                   the microphone alone (when a microphone was recorded)
      system.wav                the computer's own sound alone (when it was recorded)
    take-002/                   a take of the built-in test source (used by the tests)
      take.json
      events.jsonl
      segments/
        seg-000001.seg          complete, checksummed pieces of the recording
        seg-000002.seg
        seg-000003.part         a piece still being written (or left by a crash)
```

A take holds its media in **one of two ways**. A real screen recording is one `screen.mp4` file written by Windows' own video tools, plus one separate `.wav` sound file for each sound that was recorded. The built-in test source (which makes fake picture and sound so the recorder can be tested anywhere) uses the segment files described in section 4. `take.json` says which: a take with a `media` list is a file take; a take without one is a segment take.

## 2. The manifest: `capture.json`

```json
{
  "format": "openstage-capture-project",
  "schemaVersion": "0.1.0",
  "id": "cap-...",
  "title": "Lesson 1",
  "language": "en-US",
  "created": "2026-10-13T12:00:00Z",
  "tracks": {
    "captions": [],
    "transcript": null,
    "audioDescription": [],
    "altText": []
  },
  "edit": null
}
```

| Field | Meaning |
|---|---|
| `format` | Always `openstage-capture-project`. A different value is refused. |
| `schemaVersion` | `0.1.x` files are readable by this version. Anything else is refused with a plain message. Every change to the format bumps this and ships a migration. |
| `id` | A unique id for the project. |
| `title`, `language` | Set when the project is made. `language` is a BCP 47 tag, for captions and speech. |
| `created` | UTC time, ISO 8601. |
| `tracks` | The accessibility tracks. They exist from version one so captions, transcript, audio description, and alt text are never an afterthought. Their contents are defined in later versions; in 0.1 they are empty. |
| `edit` | Reserved for the edit decision list (trims, splits, overlays, zooms). `null` until editing exists. |

**The manifest never lists takes.** The folder `takes/` is the list. This keeps a crash from leaving two records that disagree.

## 3. A take: `take.json`

```json
{
  "id": 1,
  "state": "complete",
  "startedAt": "2026-10-13T12:00:00Z",
  "endedAt": "2026-10-13T12:00:12Z",
  "setAsideAt": null,
  "sources": ["screen:1"],
  "target": null,
  "flushMs": 2000,
  "durationMs": 12000,
  "maxLossMs": null,
  "segments": 3,
  "media": [
    { "file": "screen.mp4", "kind": "screen", "container": "fragmented-mp4",
      "mix": { "microphoneGainDb": 0, "systemGainDb": -10 } },
    { "file": "mic.wav", "kind": "microphone", "container": "wav" },
    { "file": "system.wav", "kind": "system-audio", "container": "wav" }
  ]
}
```

| `state` | Meaning |
|---|---|
| `recording` | Being recorded now, **or the program stopped before finishing** (a crash). `recover` repairs these. |
| `complete` | Finished cleanly. |
| `recovered` | Repaired after an interruption. `maxLossMs` says the most that could have been lost. |
| `setAside` | The author chose Start again. The take is **kept, never erased**. |

| Field | Meaning |
|---|---|
| `sources` | What was recorded: `screen:1` is Display 1; `synthetic` is the test source. |
| `flushMs` | The most recording a crash can cost with this kind of media. Segment takes force data to disk this often (1000 by default). For `fragmented-mp4` takes it is 2000: Windows closes a fragment about every second, and a measured crash on Windows 11 lost 1.1 seconds. |
| `durationMs` | How long the recording ran, not counting time spent paused. For a recovered file take it is an **estimate** (about one second per whole fragment); the exact length is read when the video is opened. |
| `segments` | How many media files the take holds (segment files, or video files). |
| `media` | New in 0.2. The take's media files, named relative to the take's folder, each with a `kind` and a `container`. Kinds: `screen` (in a `fragmented-mp4`; its optional `mix` says how the sound inside it was mixed, see section 4: a setup box and then short, self-contained fragments, so a file cut off by a crash plays up to the last whole fragment), `microphone` and `system-audio` (each a `wav`, new in 0.2.1: see section 4). Absent in older projects. A reader ignores kinds it does not know. |

`take.json` is written atomically (to a temporary file, then renamed), so a crash never leaves half a file. It is the **only** place a take's state is recorded. Take folders are numbered `take-001`, `take-002`, and so on; numbers are never reused.

## 4. Screen recordings: one crash-safe MP4

A real screen recording is one file, `screen.mp4`, written by Windows' Media Foundation as a **fragmented MP4**: the setup box (`ftyp`, `moov`) is written first, then a short fragment (a `moof` box describing it and an `mdat` box holding the video) about once a second. Every fragment is complete on its own, so a file that stops in the middle of a fragment still plays up to the one before.

The picture is H.264 at a steady 30 pictures per second (the newest picture of the screen repeats while the screen does not change). The video's clock is **recording time**: pausing stops it, so the video has no gap where a pause was.

### Sound

Sound is recorded as 16-bit stereo at 48,000 samples per second and is stored **twice**:

1. **Mixed, inside `screen.mp4`**, as one AAC sound track (128 kilobits per second) beside the picture. This is what every player plays. It is the microphone and the computer's sound added together.
2. **Separately, as WAV files**: `mic.wav` (the microphone alone) and `system.wav` (the computer's own sound alone, including a screen reader's speech). The editor needs them apart, for example to remove an echo when the microphone also heard the speakers. A take lists a WAV file only if that sound was recorded.

The mix inside the video can be adjusted without touching the originals: `microphoneGainDb` and `systemGainDb` (whole decibels from -40 to 20, 0 meaning as heard; about 6 decibels is half or double the volume) are applied only to the mix, and `take.json` records them in the video's `mix`. `mic.wav` and `system.wav` always hold the sound exactly as heard, so the editor can start again from them and set the balance by ear. Windows' own MP4 writer accepts only one sound stream per file (one picture, one sound), which is why the two sounds are kept apart as WAV files and not as two tracks inside the MP4.

Every sound track is as long as the recording, to within a few hundredths of a second. The recording clock stands still while paused, so sound heard during a pause is thrown away, and so is any sound heard while the recorder was still starting up. If the computer plays nothing, Windows sends no sound at all for that time; the recorder fills the gap with silence so the track keeps its length.

A WAV file normally records its length only when it is finished, so one cut off by a crash would claim to be empty. These are written differently: about once a second the recorder writes the sound, forces it to disk, and only then updates the length in the file's header. So the header is never ahead of the sound, and a crash costs at most about a second of it. `recover` also cuts a half-written sound frame off the end and corrects the header, and leaves a file that is already fine byte for byte as it was.

A WAV file counts its length in 32 bits, which is a little under 4 GiB: about 6 hours of this sound. The recorder stops with a plain message rather than writing a file it could not describe.

If a sound device stops in the middle of a recording (a microphone is unplugged, a headset is switched), the take ends with a plain message and everything recorded until then is kept.

### Recovery

`openstage-capture recover --project <folder>` does this for every take in state `recording` that lists an MP4 file:

1. Look at the outermost boxes of the file. Find the last safe point: after the setup box, or after a whole fragment (`moof` and its complete `mdat`).
2. Cut the file back to that point. Only an unfinished last fragment, or a few stray bytes, are ever removed. A file that is already clean is left alone.
3. A file that does not exist is reported. A file with no usable start (not even the setup box) is **left exactly as it is** and reported.
4. Repair each `wav` sound file the take lists: cut a half-written sound frame off the end and correct the header's lengths. A file that is already fine is left byte for byte as it was; one that is missing or not a sound file this program wrote is reported and left alone.
5. Trim a half-written final line from `events.jsonl`.
6. Write `take.json` with state `recovered`, an estimated `durationMs`, and `maxLossMs`.

It is safe to run again. In a real test on Windows 11 the recording ran 8.1 seconds, the program was killed without warning, and 7.0 seconds survived and played without any repair.

## 5. Segments (the test source)

Segment takes are written in short pieces called segments (2 seconds each by default). Each piece is complete and checksummed on its own, so a crash costs seconds, not the take.

### Files

- `seg-000001.part`: being written.
- `seg-000001.seg`: complete. The `.part` file is renamed to `.seg` only after it is flushed to disk.

### Contents

A segment file is:

```
8 bytes   magic: "OSCSEG0\n"
then zero or more records:
  4 bytes   payload length   (unsigned, little-endian)
  4 bytes   CRC-32 of payload (unsigned, little-endian)
  8 bytes   media time in ms (unsigned, little-endian)
  n bytes   payload
```

The first byte of each payload says what kind of data it is: `1` picture, `2` microphone sound, `3` system sound (including screen reader speech). The rest is the data for that kind. (Release 1 will define the data formats when real capture arrives; version 0.1 stores the test source's data.)

### Flushing

Data is forced to disk every `flushMs` (1000 ms by default). **A crash loses at most that much.** The requirement allows 10 seconds; this is stricter.

### Recovery

`openstage-capture recover --project <folder>` does this for every take in state `recording`:

1. For each `.part` file, keep every complete record whose checksum is right; stop at the first damaged or cut-off record; cut the file off there and rename it to `.seg`. A `.part` file that holds only the 8-byte start marker and no records is removed (it held no data). A `.part` file that does not start with the marker is **kept** and renamed to `seg-NNNNNN.part.unreadable`, and the author is told in plain language.
2. Count the complete segments and read the recording length from the last one.
3. Remove a half-written final line from `events.jsonl`.
4. Write `take.json` with state `recovered`, the length, and `maxLossMs`.

It is safe to run again: takes that are already fine are left alone. A take folder with a missing or unreadable `take.json` is rebuilt from the recording itself, with a plain-language note. **Nothing is ever deleted except a segment file that held no data at all.** The cut-off end of a damaged segment is the only recorded data that can be dropped, and `maxLossMs` states how much that can be.

## 6. The interaction log: `events.jsonl`

One JSON object per line, only ever appended to. `t` is milliseconds of recording time.

```json
{"t":1200,"type":"click","x":640,"y":480,"button":"left"}
{"t":1210,"type":"element","name":"Save","role":"button"}
{"t":3400,"type":"window","title":"Untitled - Notepad","process":"notepad.exe"}
```

| `type` | Fields |
|---|---|
| `cursorMove` | `x`, `y` |
| `click` | `x`, `y`, `button` |
| `key` | `key`, `modifiers`. **Never written for password fields.** |
| `window` | `title`, `process` |
| `element` | `name`, `role` of the clicked item, from Windows' accessibility information |
| `marker` | `label` |

Interaction capture is **off unless the author turns it on** (this is a requirement for the real Windows capture, Phase 2; in version 0.1 the only thing written is `marker`). A line that cannot be read, such as one cut short by a crash, is skipped when reading and trimmed by `recover`. The log is the raw material for step lists, alt text, and interactive steps; it is not published.

## 7. Versions

`0.2.0` added one optional field (`media` in `take.json`). `0.2.1` adds two kinds of media to that list (`microphone` and `system-audio`, in `wav` files) and puts the mixed sound inside `screen.mp4`. `0.2.2` adds the optional `mix` on the video's entry in that list. Projects at `0.1.x`, `0.2.0` and `0.2.1` open unchanged, so no migration is needed; the version is bumped so that a program can tell which projects can hold sound files. (A program that predates `0.2.1` opens such a project but does not know about its sound files; `recover` from that program would leave them as they are.) A project with any other version is refused with a plain message.

## 8. Rules this format keeps

- Nothing the author recorded is erased by the software. Start again sets a take aside.
- A crash costs at most `flushMs` of recording (1 second for segment takes, 2 for screen recordings, sound files included).
- A newer, unknown schema is refused with a plain message, never half-read.
- Raw footage and the interaction log stay out of published courses.
- Narration is text. Nothing in this format stores narration as audio only.
