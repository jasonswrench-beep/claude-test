# OpenStage Capture: Edit Format and Edit Model

**Status:** v0.1 (matches the code in `capture-edit/` and `capture/crates/openstage-capture-project/src/edit.rs`)
**Format name:** `openstage-capture-edit`
**Schema version:** `0.1.0`

This is the file that records what the author has done to a recording: which parts of the recording
are kept, where they sit on a timeline, and how loud they are. It is called an *edit decision list*. The
recording itself is never changed: trims, splits and the rest are only these numbers (story B4).

It is written by the editor (TypeScript, `capture-edit/`, which is built and tested without a screen) and
read by the renderer (Rust), which turns it into a finished MP4 in Phase 3. The roadmap's architecture
decision explains why the editing lives in TypeScript and the rendering in Rust.

## 1. Where it lives

`edit.json` sits in the capture project folder, beside `capture.json`. If there is no `edit.json`, nothing
has been edited yet. The `edit` field of `capture.json` stays reserved and unused: the file's presence is
the truth, so the two can never disagree. Saving writes to a temporary file and renames it, like every
other file in the project.

## 2. The file

```json
{
  "format": "openstage-capture-edit",
  "schemaVersion": "0.1.0",
  "nextId": 6,
  "sources": [
    { "ref": { "kind": "take", "take": 1, "media": "screen.mp4" }, "kind": "screen",
      "name": "Screen of take 1", "durationMs": 60000 },
    { "ref": { "kind": "asset", "asset": "logo.png" }, "kind": "image",
      "name": "Logo", "durationMs": null }
  ],
  "tracks": [
    { "id": "track-1", "kind": "screen", "name": "Screen", "clips": [
      { "id": "clip-3", "name": "Lesson",
        "source": { "kind": "take", "take": 1, "media": "screen.mp4" },
        "sourceStartMs": 0, "sourceEndMs": 20000, "startMs": 0,
        "muted": false, "volumeDb": 0, "fadeInMs": 0, "fadeOutMs": 0 } ] }
  ]
}
```

Everything is in **whole milliseconds**. Properties are always written in the same order with two-space
indents and a closing new line, so saving the same edit twice gives the same bytes (story B4).

| Field | Meaning |
|---|---|
| `format`, `schemaVersion` | A reader refuses a file whose `format` is not this one, and one whose version does not start with `0.1.`. The message says so in words. |
| `nextId` | The number the next track or clip id is made from (`track-4`, `clip-9`). Ids are never reused. |
| `sources` | Every file the clips may use, with what it is and how long it is. `ref` is either a file of a recording (`take` and `media`, as listed in that take's `take.json`) or a file from the course's assets (`asset`). `kind` is `screen`, `webcam`, `microphone`, `system-audio`, `image`, `audio` or `video`. `durationMs` is its natural length, or `null` for a still image, which can be shown for any length. `hasTextAlternative` (imported video and sound only) says whether it has captions, a narration script or a transcript. |
| `tracks` | In the order shown. `kind` is `screen`, `webcam`, `audio` or `media` (imported images, sound and short video). A track takes only some kinds of source: screen takes `screen`; webcam takes `webcam`; audio takes `microphone`, `system-audio` and `audio`; media takes `image`, `video` and `audio`. |
| clip `source`, `sourceStartMs`, `sourceEndMs` | The part of the source that is used, in the source's own time. A still image always has `sourceStartMs` 0 and a `sourceEndMs` equal to how long it is shown. |
| clip `startMs` | Where on the timeline the clip begins. It ends `sourceEndMs - sourceStartMs` later. A track's clips are in order of start and never overlap. |
| clip `muted`, `volumeDb`, `fadeInMs`, `fadeOutMs` | Basic sound settings (story C9): off, louder or quieter in whole decibels from -40 to 20 (about 6 decibels is double or half), and fades. The fades together cannot be longer than the clip. |
| clip `altText`, `decorative` | For an image: what it shows, or that it is decorative. One of them is required (story C10). |

## 3. What is checked

Every reader makes the same checks and names them with the same codes. `error` means the edit is not
sound and must not be rendered. `needs-attention` means it is sound but something OpenStage requires is
still missing; the author is asked for it, and export refuses to finish until it is given.

| Code | Severity | Meaning |
|---|---|---|
| `duplicate-id` | error | Two tracks or clips have the same id. |
| `bad-time` | error | A time is not a whole number of milliseconds, or is below zero. |
| `too-short` | error | A clip is under 0.1 seconds long. |
| `unknown-source` | error | A clip uses a file that `sources` does not list. |
| `source-not-allowed` | error | A clip's file cannot go on that kind of track. |
| `beyond-source` | error | A clip uses more of a file than exists. |
| `overlap` | error | A clip starts before the one before it ends. |
| `unsorted` | error | A clip is listed after one that starts later. |
| `fade-too-long` | error | The fades together are longer than the clip. |
| `volume-range` | error | The volume change is not a whole number from -40 to 20. |
| `imported-video-too-long` | error | An imported video clip is over 30 seconds (story C10). |
| `needs-alt-text` | needs-attention | An image has no alt text and is not marked decorative. |
| `needs-text-alternative` | needs-attention | An imported video or sound has no captions, script or transcript. |

The examples in `capture-edit/fixtures/` (one per code, plus files that cannot be read at all) are shared:
`expected.json` says what each must produce, and both the TypeScript tests and the Rust tests read them,
so the two readers cannot quietly disagree about what is valid.

## 4. The editing actions

Each action takes the edit and gives back a new edit with a sentence for the author, or a sentence saying
why it cannot be done and changes nothing. Nothing is changed in place. Each result also carries a short
label that undo and redo announce.

| Action | What it does | Says, for example |
|---|---|---|
| add track, rename track | Adds a track; later ones of a kind are numbered. | Added the track "Audio 2". |
| add clip | Puts a source on a track at a time. Refuses an unfitting source, an overlap, more than the source holds, or imported video over 30 seconds. For an image or sound without its text it says what is still needed. | Added the clip "Logo" to the Imported media track at 5 seconds, 3 seconds long. Still needed: alt text for the image, or mark it as decorative. |
| split at a time | Cuts the clip under the time, on one track or on every track at once, so picture and sound stay together. Neither piece may be under 0.1 seconds. Fades stay at the outer ends only. | Clip split at 1 minute 12.4 seconds, now two clips. |
| trim start, trim end | The start and end grips. The other end stays; the clip's length changes. Going outward shows more of the recording, only as far as there is and as far as the neighbouring clip allows. | Clip "Lesson" now starts at 5 seconds, 5 seconds shorter. |
| move, set length | Exact values from the clip list. A moved clip may not run into its neighbours. | Clip "Lesson" now starts at 12 seconds. |
| delete | Removes a clip. The author chooses, every time, whether to leave the gap or close it. | Clip "Lesson" deleted, leaving a gap of 10 seconds. |
| remove a stretch | Cuts a stretch of time out of every track (or chosen tracks), splitting clips at its edges, so a mistake goes from picture and sound together. Again the author chooses whether to close the gap. | Removed 5 seconds from 10 seconds to 15 seconds on 2 tracks, and moved everything after it earlier to close the gap. |
| join | Joins a clip with the next when they are the same file, touch, and continue one into the other. Otherwise says which of those is false. | Joined 2 clips into one clip of 1 minute. |
| reorder | Moves a clip to another place in its track's order and lays the clips out again, one after another, keeping the gaps where they were. | Clip "Lesson" moved to number 1 of 3; the clips on the Screen track were laid out again in the new order. |
| rename, mute, volume, fades, alt text | Settings of a clip. | Clip "Voice" is now 6 decibels quieter than recorded. |

An action that would change nothing (a key pressed at the end of its range) gives back the same edit, so
the undo history skips it.

**Keyboard steps** are the same as OpenStage's existing Timeline: Left and Right Arrow move a tenth of a
second, with Shift a second; Page Up and Page Down five seconds; Home and End go to the ends of the
range. Times typed into fields are understood as `72.4` (seconds), `1:12.4`, `1:02:05` or `1m12.4s`, and a
mistake is explained with examples. **Times are said in words:** `1 minute 12.4 seconds`.

**Undo and redo** (story C11) walk back and forth through the actions, saying "Undid: split clip." and
"Redid: split clip." A drag (a mouse drag or a held key) is many small actions but one undo step: begin a
group, act, end the group; the end says what the whole drag came to ("Clip "Lesson" now starts at 34
seconds, 4 seconds shorter."). Escape cancels the group and records nothing. The last 200 steps are kept.

**The clip list** (story C1) has a row for every clip with the columns Name, Track, Type, Start, Length,
End and State, a caption ("Tracks and clips: 3 clips on 3 tracks, 1 minute long."), and a sentence for each
clip. **The described preview** (story C4) says what is on screen and in the sound at a moment, using what
the rest of the editor knows (the window in front, what the cursor is over, overlays, caption, zoom).

## 5. Choices that could have gone the other way

Made on the owner's delegation; each can be changed.

1. **Trimming leaves a gap** rather than moving the clips after it. Only *delete* asks the author whether to close the gap, as the requirements say.
2. **A split piece keeps its clip's name.** Pieces are told apart by their start in the list.
3. **Reordering keeps the gaps** between clips where they were, and does not touch other tracks (so it can move one track out of step with another; the editor should say so).
4. **Imported video is limited to 30 seconds of the clip used**, not of the file. Imported sound has no limit.
5. **A still image is shown for 5 seconds** unless another length is given.
6. **No clip is shorter than 0.1 seconds**, matching the 0.1 second grip step.
7. **Overlapping clips on one track are not allowed.** Overlap (a picture-in-picture, a title over the screen) is done with another track.
8. **Whole milliseconds, no frame grid.** The renderer places clips on its own frame rate.

## 6. Not here yet

Text overlays, callouts, shapes, blur and redaction (C7, C8), zoom and pan (C5), cursor effects (C6, which come from the recorded events at render time), caption tracks (D1), and speed changes. Each will be added as a new kind of track or item with its own version bump and, if needed, a migration; a reader refuses a version it does not know rather than half understand it.

## 7. Versions

`0.1.0` is the first version. Any change to the format bumps the version and ships a migration.
