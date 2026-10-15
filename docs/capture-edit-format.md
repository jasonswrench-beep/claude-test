# OpenStage Capture: Edit Format and Edit Model

**Status:** v0.3 (matches the code in `capture-edit/` and `capture/crates/openstage-capture-project/src/edit.rs`)
**Format name:** `openstage-capture-edit`
**Schema version:** `0.3.0` (files at `0.1.x` and `0.2.x` still open and are brought up to date: see section 7)

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
| `format`, `schemaVersion` | A reader refuses a file whose `format` is not this one, and one whose version does not start with `0.1.` or `0.2.`. The message says so in words. |
| `nextId` | The number the next track or clip id is made from (`track-4`, `clip-9`). Ids are never reused. |
| `sources` | Every file the clips may use, with what it is and how long it is. `ref` is either a file of a recording (`take` and `media`, as listed in that take's `take.json`) or a file from the course's assets (`asset`). `kind` is `screen`, `webcam`, `microphone`, `system-audio`, `image`, `audio` or `video`. `durationMs` is its natural length, or `null` for a still image, which can be shown for any length. `hasTextAlternative` (imported video and sound only) says whether it has captions, a narration script or a transcript. |
| `tracks` | In the order shown. `kind` is `screen`, `webcam`, `audio` or `media` (imported images, sound and short video), which hold `clips`; or `overlay` or `zoom`, which hold `items` (new in 0.2: see section 3). A clip track takes only some kinds of source: screen takes `screen`; webcam takes `webcam`; audio takes `microphone`, `system-audio` and `audio`; media takes `image`, `video` and `audio`. |
| clip `source`, `sourceStartMs`, `sourceEndMs` | The part of the source that is used, in the source's own time. A still image always has `sourceStartMs` 0 and a `sourceEndMs` equal to how long it is shown. |
| clip `startMs` | Where on the timeline the clip begins. It ends `sourceEndMs - sourceStartMs` later. A track's clips are in order of start and never overlap. |
| clip `muted`, `volumeDb`, `fadeInMs`, `fadeOutMs` | Basic sound settings (story C9): off, louder or quieter in whole decibels from -40 to 20 (about 6 decibels is double or half), and fades. The fades together cannot be longer than the clip. |
| clip `placement` | New in 0.3. Where the picture sits in the finished video: a rectangle on the frame, in the same units as an overlay's (ten-thousandths of the frame's width and height from its top left corner). Optional. Without it a screen, image or video fills the frame and a webcam is a small picture in the bottom right corner (or fills the frame when there is no other picture at that moment). A picture is never stretched: it is made as large as fits the rectangle and centred in it. Only a clip that has a picture can have one: a place on sound is an error (`placement-not-allowed`), and a place that is not a rectangle inside the picture is `bad-rect`. |
| clip `altText`, `decorative` | For an image: what it shows, or that it is decorative. One of them is required (story C10). |

### Overlays and zooms (new in 0.2)

An `overlay` track holds text, shapes, arrows, blur and redaction; a `zoom` track holds zooms. They are not cut from a recording, so they are `items`, not clips: each has an `id`, a `kind`, a `name`, a `startMs` and a `durationMs`. Overlays on one track may overlap in time (later ones are drawn on top); zooms may not.

Positions are **rectangles in ten-thousandths of the picture's width and height**, measured from its top left corner (`{"x": 2500, "y": 2500, "width": 5000, "height": 5000}` is the middle quarter). That means the same thing at any video size and is always a whole number. Authors type pixels; the editor converts. Colours are written `#RRGGBB`.

| Kind | Fields besides the common ones |
|---|---|
| `text` (story C7) | `text`, `rect`, `fontSizeBp` (height of a line, in ten-thousandths of the picture's height), `color`, `background` (a colour or `null`), `bold`, `align` (`left`, `center`, `right`). The text is also part of the transcript. |
| `shape` (C8) | `shape` (`rectangle` or `ellipse`), `rect`, `outline`, `fill` (or `null`), `lineWidthBp`, and `altText` or `decorative`. |
| `arrow` (C8) | `from` and `to` (points `{x, y}`), `color`, `lineWidthBp`, and `altText` or `decorative`. |
| `blur` (C8) | `rect`. A blur in the exported video cannot be undone by the viewer, and the editor says so. |
| `redact` (C8) | `rect`, `color`. The same. |
| `zoom` (C5) | `rect` (the part that fills the frame), `transitionMs` (how long moving in and moving out each take; 500 by default), `description` (what it shows, in words, so a zoom is not only a picture). |

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
| `item-wrong-track` | error | An overlay or zoom is on the wrong kind of track. |
| `bad-rect` | error | A rectangle or point (including a clip's `placement`) is not whole numbers inside the picture, or has no size. |
| `placement-not-allowed` | error | A clip that holds only sound has a `placement`. |
| `bad-color` | error | A colour is not written `#RRGGBB`. |
| `bad-style` | error | A text size or line thickness is outside what can be drawn. |
| `text-empty` | error | A text overlay has no text. |
| `transition-too-long` | error | A zoom's moves in and out do not fit in its length. |
| `too-many-zooms` | error | More than 3 zooms start within one second. |
| `needs-alt-text` | needs-attention | (also) A shape or arrow has no alt text and is not marked decorative. |
| `needs-description` | needs-attention | A zoom has no description. |
| `low-contrast` | warning | Text and its own background differ by less than 4.5 to 1 (3 to 1 for large text, 24 pixels or more on a 1080 pixel picture, or 18.7 if bold). Text with no background is not checked: there is nothing to compare it with. |
| `text-too-small` | warning | Text is under about 16 pixels on a 1080 pixel picture. |
| `zoom-aspect` | warning | A zoom's rectangle is not the shape of the picture, so the zoomed view would have bars. |

(`bad-time`, `too-short`, `unsorted` and `overlap` apply to items as well as clips; `duplicate-id` applies to tracks, clips and items alike.)

A `warning` is advice: the edit is sound and complete, but the thing is likely to be hard to see or read. The editor adds it to the sentence it says when the change is made, so the author hears about it straight away.

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
| add text, shape, arrow, blur, redaction, zoom | Puts an overlay or zoom on its track. Refuses an unfitting track, a bad colour or rectangle, an overlapping or too-busy zoom. Adds warnings and what is still needed to the sentence. | Added the text overlay "Welcome" to the Overlays track at 5 seconds, 3 seconds long. Warning: ... is hard to read: ... contrast of 1.1 to 1, and 3.0 to 1 is needed. |
| move, trim start, trim end, delete, set position, set text, set look, set zoom, alt text | Change an overlay or zoom. A change that would make the edit unsound is refused and changes nothing. | Zoom "Zoom" now shows the top left of the picture at 2 times. |

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
9. **Positions are fractions of the picture, not pixels**, so an edit does not depend on the video's size. The cost is that a pixel position typed by an author is rounded to the nearest ten-thousandth (about 0.2 of a pixel at 1080p).
10. **Overlays may overlap in time; zooms may not**, and no more than 3 zooms may start within one second. This is a safe-motion limit chosen to match the requirement that animations never flash more than three times a second. Zooms are also not allowed to be shorter than their own moves.
11. **Contrast is checked against the overlay's own background only**, and the thresholds for large and very small text assume a picture 1080 pixels high.
12. **Blur and redaction need no alt text**; shapes and arrows do (or must be marked decorative). A text overlay's own text serves as its text equivalent.

## 5b. What the renderer does with it

The renderer (`capture/crates/openstage-capture-render` plans and draws; `capture/crates/openstage-capture-windows` reads the recordings, draws the text and writes the MP4) first checks the edit and refuses one with errors, with the first problem in words; unless told otherwise it also refuses one that is sound but still has things marked `needs-attention`, so an export does not finish until the author has given the alt text or descriptions that are missing. Warnings do not stop it. The length of the video is where the last clip ends. Frame `n` is shown at `n / fps` seconds, in whole microseconds. Sound comes from the microphone, computer-sound and imported-audio clips, with their volume, mute and straight-line fades, added together and limited so a loud sum is clipped, never wrapped into noise. **Not yet:** the sound inside an imported video clip is not mixed.

**How a frame is drawn**, bottom to top:

1. **The main picture** (the screen, or an imported image or video, in track order): fitted into the frame, or into the clip's `placement`, and zoomed. It is never stretched; if its shape differs from its box it is made as large as fits and centred, with black either side.
2. **What belongs to the picture** (blur, blackout, shapes, arrows), in the order the edit lists them (later on top). Their positions are fractions of the frame, and they **move and grow with the zoom**, so a blurred or blacked-out box always covers the same part of the picture whatever the zoom has done. A box that ends in the middle of a pixel is made bigger to take in the whole pixel, never smaller. A blur is strong enough to hide small print but is not a secret-keeper: a password or a name that must not be recovered should be blacked out.
3. **The webcam** (a clip on a `webcam` track): in the bottom right corner at about 22 percent of the frame's width by default, or where its `placement` says; not zoomed; above shapes and arrows. When no other picture is showing at that moment, a webcam with no `placement` fills the frame.
4. **Text**, above everything and **not zoomed**, so titles and labels stay the same size and in the same place while the picture moves. The text is drawn by Windows' own text drawing, so it has the fonts and language rules of the computer. It is laid out in its box with a little room round it (a third of a line's height), broken into lines at spaces, and cut off where it does not fit; the export then says that the text overlay does not fit its box, because words that are cut off cannot be read.

A zoom moves in over `transitionMs` along a smooth curve (slow at the start and end), holds, and moves out the same way; when the author asks for **reduced motion**, it cuts straight to the part and straight back with no movement. A zoom rectangle that is not the shape of the frame stretches the picture (the `zoom-aspect` warning says so). The video is the size of the first picture on the screen track unless another size is asked for.

Drawing uses only whole numbers and the arithmetic every computer does the same way, so a frame drawn twice from the same pictures is the same bytes.

**Choices that could go either way, for the owner to veto:** (1) blur, blackout, shapes and arrows zoom with the picture but text does not (the alternative is that everything zooms together, which can push a title out of the frame); (2) the webcam is not zoomed and is drawn above shapes but below text; (3) a webcam with no place is a corner picture, or the whole frame when it is alone; (4) positions are fractions of the frame, not of the picture inside it, which makes no difference unless the picture is a different shape from the frame.

## 6. Not here yet

Cursor effects (C6, which come from the recorded events at render time), suggested zooms from clicks (C5, which needs the interaction log), caption tracks (D1), zoom that follows a moving point, text that animates, and speed changes. Each will be added as a new kind of track or item with its own version bump and, if needed, a migration; a reader refuses a version it does not know rather than half understand it.

## 7. Versions

`0.1.0` was the first version. `0.2.0` adds the `overlay` and `zoom` track kinds and their `items` (a track now always has an `items` list, empty for clip tracks). The migration from `0.1` is to give every track an empty `items` list, which the reader does when it opens an old file; saving then writes `0.2.0`. A reader refuses any other version with a plain message. `0.3.0` adds an optional `placement` to a clip (where its picture sits in the finished video). A file without one is drawn as before, so the migration from `0.1` or `0.2` is to change nothing but the version, which the reader does when it opens an old file; saving then writes `0.3.0`. Any change to the format bumps the version and ships a migration.
