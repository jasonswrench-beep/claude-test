# OpenStage Capture: Project Format

**Status:** v0.1 (matches the code in `capture/crates/openstage-capture-project`)
**Format name:** `openstage-capture-project`
**Schema version:** `0.1.0`

A recording lives in its own folder, named `something.capture`, **beside** the course (the `.osproj`), never inside it. This matters: OpenStage's publisher zips the project into `source/`, and raw footage and the interaction log (which can contain private information) must never ship to learners. Only the exported, edited video becomes part of the course.

## 1. Layout

```
lesson.capture/
  capture.json                  the manifest (small, rarely changes)
  takes/
    take-001/
      take.json                 the only record of this take's state
      events.jsonl              what the author did while recording (append-only)
      segments/
        seg-000001.seg          complete pieces of the recording
        seg-000002.seg
        seg-000003.part         a piece still being written (or left by a crash)
    take-002/
      ...
```

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
  "sources": ["synthetic"],
  "target": null,
  "flushMs": 1000,
  "durationMs": 12000,
  "maxLossMs": null,
  "segments": 6
}
```

| `state` | Meaning |
|---|---|
| `recording` | Being recorded now, **or the program stopped before finishing** (a crash). `recover` repairs these. |
| `complete` | Finished cleanly. |
| `recovered` | Repaired after an interruption. `maxLossMs` says the most that could have been lost. |
| `setAside` | The author chose Start again. The take is **kept, never erased**. |

`take.json` is written atomically (to a temporary file, then renamed), so a crash never leaves half a file. It is the **only** place a take's state is recorded. Take folders are numbered `take-001`, `take-002`, and so on; numbers are never reused.

## 4. Segments

Recording is written in short pieces called segments (2 seconds each by default). Each piece is complete and checksummed on its own, so a crash costs seconds, not the take.

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

## 5. The interaction log: `events.jsonl`

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

## 6. Rules this format keeps

- Nothing the author recorded is erased by the software. Start again sets a take aside.
- A crash costs at most `flushMs` of recording.
- A newer, unknown schema is refused with a plain message, never half-read.
- Raw footage and the interaction log stay out of published courses.
- Narration is text. Nothing in this format stores narration as audio only.
