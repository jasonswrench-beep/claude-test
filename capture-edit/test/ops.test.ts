// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addClip,
  addTrack,
  clipEndMs,
  clipLengthMs,
  deleteClip,
  deleteRange,
  joinWithNext,
  renameClip,
  reorderClip,
  setClipLength,
  setClipStart,
  setFades,
  setImageText,
  setMuted,
  setVolume,
  splitAt,
  trimEnd,
  trimStart,
  validate,
} from "../src/index.ts";
import { LOGO, LONG_VIDEO, MUSIC, SCREEN, SHORT_VIDEO, clipOf, clipsOn, ok, refused, sample } from "./helpers.ts";

// ---- adding ----

test("a recording goes onto a track whole, and the author is told where", () => {
  const s = sample();
  assert.equal(clipsOn(s.model, s.screenTrack).length, 1);
  const lesson = clipOf(s.model, s.screen);
  assert.deepEqual([lesson.startMs, lesson.sourceStartMs, lesson.sourceEndMs], [0, 0, 60_000]);
  const again = ok(addClip(s.model, s.mediaTrack, { source: SHORT_VIDEO, startMs: 5000 }));
  assert.equal(again.message, 'Added the clip "Intro video" to the Imported media track at 5 seconds, 20 seconds long.');
});

test("a clip cannot go on a track that does not take that kind of source", () => {
  const s = sample();
  const message = refused(addClip(s.model, s.screenTrack, { source: LOGO, startMs: 0 }));
  assert.match(message, /Logo cannot go on the Screen track/);
});

test("a clip cannot overlap another on the same track", () => {
  const s = sample();
  const message = refused(addClip(s.model, s.screenTrack, { source: SCREEN, startMs: 30_000, lengthMs: 5000 }));
  assert.match(message, /overlap the clip "Lesson", which runs from 0 seconds to 1 minute/);
});

test("a clip cannot use more of a recording than there is", () => {
  const s = sample();
  const t = addTrack(s.model, "screen");
  const message = refused(addClip(t.model, t.focus?.[0] ?? "", { source: SCREEN, startMs: 0, sourceStartMs: 50_000, sourceEndMs: 70_000 }));
  assert.match(message, /only 1 minute long, so a clip cannot use up to 1 minute 10 seconds of it/);
});

test("a still image is shown for 5 seconds unless said, and asks for alt text", () => {
  const s = sample();
  const added = ok(addClip(s.model, s.mediaTrack, { source: LOGO, startMs: 0 }));
  const clip = clipOf(added.model, added.focus?.[0] ?? "");
  assert.equal(clipLengthMs(clip), 5000);
  assert.match(added.message, /Still needed: alt text for the image, or mark it as decorative\./);
  const described = ok(addClip(s.model, s.mediaTrack, { source: LOGO, startMs: 0, lengthMs: 3000, altText: "The company logo" }));
  assert.doesNotMatch(described.message, /Still needed/);
  assert.equal(clipLengthMs(clipOf(described.model, described.focus?.[0] ?? "")), 3000);
});

test("imported video can be at most 30 seconds, imported sound can be any length", () => {
  const s = sample();
  assert.match(refused(addClip(s.model, s.mediaTrack, { source: LONG_VIDEO, startMs: 0 })), /at most 30 seconds/);
  ok(addClip(s.model, s.mediaTrack, { source: LONG_VIDEO, startMs: 0, lengthMs: 30_000 }));
  assert.match(refused(addClip(s.model, s.mediaTrack, { source: LONG_VIDEO, startMs: 0, lengthMs: 31_000 })), /at most 30 seconds/);
  const music = ok(addClip(s.model, s.mediaTrack, { source: MUSIC, startMs: 0 }));
  assert.match(music.message, /Still needed: captions, a script or a transcript for it\./);
});

// ---- split ----

test("splitting at the playhead makes two clips and says so in the words of the requirements", () => {
  const s = sample();
  const r = ok(splitAt(s.model, 72_400 - 60_000, s.screenTrack));
  assert.equal(r.message, "Clip split at 12.4 seconds, now two clips.");
  assert.equal(r.label, "split clip");
  const [a, b] = clipsOn(r.model, s.screenTrack);
  assert.ok(a && b);
  assert.deepEqual([a.startMs, clipEndMs(a), a.sourceStartMs, a.sourceEndMs], [0, 12_400, 0, 12_400]);
  assert.deepEqual([b.startMs, clipEndMs(b), b.sourceStartMs, b.sourceEndMs], [12_400, 60_000, 12_400, 60_000]);
  assert.notEqual(a.id, b.id);
  assert.equal(a.name, b.name);
  assert.deepEqual(r.focus, [a.id, b.id]);
});

test("a time in words of more than a minute reads as in the requirements", () => {
  let s = sample();
  s = { ...s, model: ok(trimEnd(s.model, s.screen, 60_000)).model };
  const t = addTrack(s.model, "screen");
  const long = ok(addClip(t.model, t.focus?.[0] ?? "", { source: SCREEN, startMs: 0, lengthMs: 60_000 }));
  assert.equal(ok(splitAt(long.model, 40_000, t.focus?.[0] ?? "")).message, "Clip split at 40 seconds, now two clips.");
});

test("splitting without a track cuts every clip under the playhead, so picture and sound stay together", () => {
  const s = sample();
  const r = ok(splitAt(s.model, 30_000));
  assert.equal(r.message, "Split 2 clips at 30 seconds; each is now two clips.");
  assert.equal(clipsOn(r.model, s.screenTrack).length, 2);
  assert.equal(clipsOn(r.model, s.audioTrack).length, 2);
});

test("a split at the very start or end of a clip, or in empty space, is refused in words", () => {
  const s = sample();
  assert.match(refused(splitAt(s.model, 0, s.screenTrack)), /Nothing to split at 0 seconds/);
  assert.match(refused(splitAt(s.model, 60_000, s.screenTrack)), /Nothing to split at 1 minute/);
  assert.match(refused(splitAt(s.model, 90_000)), /no clip has this time inside it/);
  assert.match(refused(splitAt(s.model, 30_000, "track-999")), /no track with the id track-999/);
});

test("a split that would leave a sliver is refused", () => {
  const s = sample();
  assert.match(refused(splitAt(s.model, 50, s.screenTrack)), /one piece would be shorter than 0.1 seconds/);
  assert.match(refused(splitAt(s.model, 59_950, s.screenTrack)), /shorter than 0.1 seconds/);
  ok(splitAt(s.model, 100, s.screenTrack));
});

test("a split keeps the fades at the outer ends only", () => {
  const s = sample();
  const faded = ok(setFades(s.model, s.mic, 2000, 3000));
  const r = ok(splitAt(faded.model, 30_000, s.audioTrack));
  const [a, b] = clipsOn(r.model, s.audioTrack);
  assert.ok(a && b);
  assert.deepEqual([a.fadeInMs, a.fadeOutMs, b.fadeInMs, b.fadeOutMs], [2000, 0, 0, 3000]);
});

// ---- trim ----

test("moving the start grip later cuts off the beginning and keeps the end where it was", () => {
  const s = sample();
  const r = ok(trimStart(s.model, s.screen, 5000));
  const clip = clipOf(r.model, s.screen);
  assert.deepEqual([clip.startMs, clipEndMs(clip), clip.sourceStartMs, clip.sourceEndMs], [5000, 60_000, 5000, 60_000]);
  assert.equal(r.message, 'Clip "Lesson" now starts at 5 seconds, 5 seconds shorter.');
  assert.equal(r.label, "trim clip start");
});

test("moving the start grip earlier shows more of the recording, but only as far as there is", () => {
  const s = sample();
  const cut = ok(trimStart(s.model, s.screen, 5000)).model;
  const back = ok(trimStart(cut, s.screen, 2000));
  assert.equal(back.message, 'Clip "Lesson" now starts at 2 seconds, 3 seconds longer.');
  const further = ok(trimStart(back.model, s.screen, 0));
  assert.equal(clipOf(further.model, s.screen).sourceStartMs, 0);
  // A clip that was moved later has nothing of the recording before its first moment to show.
  const shorter = ok(trimEnd(s.model, s.screen, 20_000)).model;
  const moved = ok(setClipStart(shorter, s.screen, 15_000)).model;
  assert.match(refused(trimStart(moved, s.screen, 10_000)), /earlier than 15 seconds: the recording has nothing before that/);
});

test("moving the start grip stops at the clip before it and at the minimum length", () => {
  const s = sample();
  const split = ok(splitAt(s.model, 30_000, s.screenTrack)).model;
  const [a, b] = clipsOn(split, s.screenTrack);
  assert.ok(a && b);
  const trimmed = ok(trimStart(split, b.id, 40_000)).model;
  assert.match(refused(trimStart(trimmed, b.id, 20_000)), /earlier than 30 seconds: the clip "Lesson" before it ends at 30 seconds/);
  assert.match(refused(trimStart(split, b.id, 59_950)), /later than 59.9 seconds: it must stay at least 0.1 seconds long/);
});

test("moving the end grip works both ways and stops at the end of the recording", () => {
  const s = sample();
  const shorter = ok(trimEnd(s.model, s.screen, 45_000));
  assert.equal(shorter.message, 'Clip "Lesson" now ends at 45 seconds, 15 seconds shorter.');
  const longer = ok(trimEnd(shorter.model, s.screen, 50_000));
  assert.equal(longer.message, 'Clip "Lesson" now ends at 50 seconds, 5 seconds longer.');
  assert.match(refused(trimEnd(shorter.model, s.screen, 61_000)), /later than 1 minute: the recording ends there/);
});

test("moving the end grip stops at the clip after it", () => {
  const s = sample();
  const split = ok(splitAt(s.model, 30_000, s.screenTrack)).model;
  const [a, b] = clipsOn(split, s.screenTrack);
  assert.ok(a && b);
  const gap = ok(trimStart(split, b.id, 35_000)).model;
  assert.match(refused(trimEnd(gap, a.id, 36_000)), /later than 35 seconds: the clip "Lesson" after it starts at 35 seconds/);
  assert.equal(ok(trimEnd(gap, a.id, 35_000)).message, 'Clip "Lesson" now ends at 35 seconds, 5 seconds longer.');
});

test("a trim that changes nothing gives back the same model", () => {
  const s = sample();
  const r = ok(trimEnd(s.model, s.screen, 60_000));
  assert.equal(r.model, s.model);
  assert.equal(r.label, "");
  assert.equal(r.message, 'The clip "Lesson" already ends at 1 minute.');
});

test("trimming a still image changes how long it is shown", () => {
  const s = sample();
  const added = ok(addClip(s.model, s.mediaTrack, { source: LOGO, startMs: 10_000, lengthMs: 5000, decorative: true }));
  const id = added.focus?.[0] ?? "";
  const longer = ok(trimEnd(added.model, id, 20_000));
  assert.equal(clipLengthMs(clipOf(longer.model, id)), 10_000);
  const earlier = ok(trimStart(longer.model, id, 8000));
  const c = clipOf(earlier.model, id);
  assert.deepEqual([c.startMs, clipEndMs(c), c.sourceStartMs], [8000, 20_000, 0]);
});

test("a set length is the same as moving the end", () => {
  const s = sample();
  const r = ok(setClipLength(s.model, s.screen, 40_000));
  assert.equal(clipEndMs(clipOf(r.model, s.screen)), 40_000);
  assert.equal(r.message, 'Clip "Lesson" is now 40 seconds long, ending at 40 seconds.');
  assert.match(refused(setClipLength(s.model, s.screen, 0)), /Cannot end/);
});

test("trimming brings the fades down to fit", () => {
  const s = sample();
  const faded = ok(setFades(s.model, s.mic, 20_000, 30_000)).model;
  const r = ok(trimEnd(faded, s.mic, 10_000));
  const c = clipOf(r.model, s.mic);
  assert.deepEqual([c.fadeInMs, c.fadeOutMs], [10_000, 0]);
  assert.equal(validate(r.model).filter((p) => p.severity === "error").length, 0);
});

// ---- move ----

test("a clip can be moved to a new start if it does not run into its neighbours", () => {
  const s = sample();
  const shorter = ok(trimEnd(s.model, s.screen, 20_000)).model;
  const moved = ok(setClipStart(shorter, s.screen, 15_000));
  assert.equal(moved.message, 'Clip "Lesson" now starts at 15 seconds.');
  const t = addTrack(moved.model, "screen");
  assert.ok(t.focus);
  const two = ok(splitAt(shorter, 10_000, s.screenTrack)).model;
  const [a, b] = clipsOn(two, s.screenTrack);
  assert.ok(a && b);
  const gap = ok(trimStart(two, b.id, 12_000)).model;
  assert.match(refused(setClipStart(gap, a.id, 5000)), /later than 2 seconds: it would run into the clip "Lesson", which starts at 12 seconds/);
  assert.match(refused(setClipStart(gap, b.id, 5000)), /earlier than 10 seconds: it would run into the clip "Lesson", which ends then/);
});

// ---- delete ----

test("deleting can leave a gap or close it, and the author chooses", () => {
  const s = sample();
  const split = ok(splitAt(s.model, 10_000, s.screenTrack)).model;
  const two = ok(splitAt(split, 20_000, s.screenTrack)).model;
  const [a, b, c] = clipsOn(two, s.screenTrack);
  assert.ok(a && b && c);
  const gap = ok(deleteClip(two, b.id, false));
  assert.equal(gap.message, 'Clip "Lesson" deleted, leaving a gap of 10 seconds.');
  assert.deepEqual(clipsOn(gap.model, s.screenTrack).map((x) => x.startMs), [0, 20_000]);

  const closed = ok(deleteClip(two, b.id, true));
  assert.equal(closed.message, 'Clip "Lesson" deleted, and the clip after it moved 10 seconds earlier to close the gap.');
  assert.deepEqual(clipsOn(closed.model, s.screenTrack).map((x) => x.startMs), [0, 10_000]);
  assert.equal(closed.label, "delete clip");

  const last = ok(deleteClip(two, c.id, true));
  assert.equal(last.message, 'Clip "Lesson" deleted. Nothing came after it.');
});

test("closing the gap leaves other tracks alone", () => {
  const s = sample();
  const split = ok(splitAt(s.model, 10_000, s.screenTrack)).model;
  const [a] = clipsOn(split, s.screenTrack);
  assert.ok(a);
  const r = ok(deleteClip(split, a.id, true));
  assert.equal(clipsOn(r.model, s.audioTrack)[0]?.startMs, 0);
  assert.equal(clipsOn(r.model, s.audioTrack)[0]?.sourceEndMs, 60_000);
});

// ---- remove a stretch ----

test("a stretch of time can be cut out of every track at once, closing the gap", () => {
  const s = sample();
  const r = ok(deleteRange(s.model, 10_000, 15_000, true));
  assert.equal(r.message, "Removed 5 seconds from 10 seconds to 15 seconds on 2 tracks, and moved everything after it earlier to close the gap.");
  for (const track of [s.screenTrack, s.audioTrack]) {
    const [a, b] = clipsOn(r.model, track);
    assert.ok(a && b);
    assert.deepEqual([a.startMs, clipEndMs(a), a.sourceStartMs, a.sourceEndMs], [0, 10_000, 0, 10_000]);
    assert.deepEqual([b.startMs, clipEndMs(b), b.sourceStartMs, b.sourceEndMs], [10_000, 55_000, 15_000, 60_000]);
  }
  assert.equal(validate(r.model).filter((p) => p.severity === "error").length, 0);
});

test("a stretch can be cut out leaving a gap, or from chosen tracks only", () => {
  const s = sample();
  const gap = ok(deleteRange(s.model, 10_000, 15_000, false, [s.audioTrack]));
  assert.match(gap.message, /on 1 track, leaving a gap\.$/);
  assert.equal(clipsOn(gap.model, s.screenTrack).length, 1, "the screen track is untouched");
  const [, b] = clipsOn(gap.model, s.audioTrack);
  assert.deepEqual([b?.startMs, b?.sourceStartMs], [15_000, 15_000]);
});

test("a stretch at the start or end of a clip trims it instead of splitting it", () => {
  const s = sample();
  const front = ok(deleteRange(s.model, 0, 5000, false, [s.screenTrack]));
  assert.deepEqual(clipsOn(front.model, s.screenTrack).map((c) => [c.startMs, c.sourceStartMs, c.sourceEndMs]), [[5000, 5000, 60_000]]);
  const back = ok(deleteRange(s.model, 55_000, 70_000, true, [s.screenTrack]));
  assert.deepEqual(clipsOn(back.model, s.screenTrack).map((c) => [c.startMs, c.sourceEndMs]), [[0, 55_000]]);
});

test("a stretch that would leave a sliver, or that has no clip in it, is refused", () => {
  const s = sample();
  assert.match(refused(deleteRange(s.model, 10_000, 59_950, true)), /shorter than 0.1 seconds/);
  assert.match(refused(deleteRange(s.model, 70_000, 80_000, true)), /Nothing to remove between 1 minute 10 seconds and 1 minute 20 seconds/);
  assert.match(refused(deleteRange(s.model, 5000, 5000, true)), /must be after the start/);
});

// ---- join ----

test("two clips that continue one into the other can be joined back", () => {
  const s = sample();
  const split = ok(splitAt(s.model, 30_000, s.screenTrack)).model;
  const [a] = clipsOn(split, s.screenTrack);
  assert.ok(a);
  const r = ok(joinWithNext(split, a.id));
  assert.equal(r.message, "Joined 2 clips into one clip of 1 minute.");
  assert.deepEqual(clipsOn(r.model, s.screenTrack).map((c) => [c.startMs, c.sourceStartMs, c.sourceEndMs]), [[0, 0, 60_000]]);
  assert.equal(r.label, "join clips");
});

test("clips that do not continue are not joined, and the reason is given", () => {
  const s = sample();
  const split = ok(splitAt(s.model, 30_000, s.screenTrack)).model;
  const [a, b] = clipsOn(split, s.screenTrack);
  assert.ok(a && b);
  const gap = ok(trimStart(split, b.id, 32_000)).model;
  assert.match(refused(joinWithNext(gap, a.id)), /there is a gap of 2 seconds between them/);
  const closed = ok(setClipStart(gap, b.id, 30_000)).model;
  assert.match(refused(joinWithNext(closed, a.id)), /part of the recording between them was cut out/);
  assert.match(refused(joinWithNext(split, b.id)), /it is the last clip on the Screen track/);
});

test("clips from different recordings are not joined", () => {
  const s = sample();
  const t = ok(addClip(s.model, s.mediaTrack, { source: SHORT_VIDEO, startMs: 0, lengthMs: 5000 }));
  const u = ok(addClip(t.model, s.mediaTrack, { source: MUSIC, startMs: 5000, lengthMs: 5000 }));
  assert.match(refused(joinWithNext(u.model, t.focus?.[0] ?? "")), /come from different recordings or files/);
});

test("two stills of the same image join into one longer one", () => {
  const s = sample();
  const a = ok(addClip(s.model, s.mediaTrack, { source: LOGO, startMs: 0, lengthMs: 4000, decorative: true }));
  const b = ok(addClip(a.model, s.mediaTrack, { source: LOGO, startMs: 4000, lengthMs: 3000, decorative: true }));
  const r = ok(joinWithNext(b.model, a.focus?.[0] ?? ""));
  assert.equal(clipLengthMs(clipOf(r.model, r.focus?.[0] ?? "")), 7000);
});

// ---- reorder ----

test("clips can be reordered; they are laid out again in the new order with the gaps kept", () => {
  const s = sample();
  let m = ok(splitAt(s.model, 10_000, s.screenTrack)).model;
  m = ok(splitAt(m, 30_000, s.screenTrack)).model;
  const [a, b, c] = clipsOn(m, s.screenTrack);
  assert.ok(a && b && c);
  m = ok(trimStart(m, c.id, 35_000)).model; // a 5 second gap before the last clip
  const r = ok(reorderClip(m, c.id, 0));
  assert.match(r.message, /moved to number 1 of 3/);
  const placed = clipsOn(r.model, s.screenTrack);
  assert.deepEqual(placed.map((x) => x.id), [c.id, a.id, b.id]);
  // Lengths are 25, 10 and 20 seconds now; the gaps stay where they were: none after the first slot, 5 seconds after the second.
  assert.deepEqual(placed.map((x) => x.startMs), [0, 25_000, 40_000]);
});

test("reordering keeps the track as long as it was", () => {
  const s = sample();
  let m = ok(splitAt(s.model, 10_000, s.screenTrack)).model;
  m = ok(splitAt(m, 30_000, s.screenTrack)).model;
  const before = clipsOn(m, s.screenTrack);
  const last = before.at(-1);
  assert.ok(last);
  const r = ok(reorderClip(m, before[0]?.id ?? "", 2));
  const after = clipsOn(r.model, s.screenTrack);
  assert.equal(clipEndMs(after.at(-1) ?? last), clipEndMs(last));
  assert.equal(validate(r.model).filter((p) => p.severity === "error").length, 0);
});

test("a clip already in its place is reported, not changed", () => {
  const s = sample();
  const r = ok(reorderClip(s.model, s.screen, 0));
  assert.equal(r.model, s.model);
  assert.equal(r.message, 'The clip "Lesson" is already number 1 of 1.');
  const high = ok(reorderClip(s.model, s.screen, 99));
  assert.equal(high.model, s.model, "an index past the end counts as the last place");
});

// ---- settings ----

test("a clip can be renamed, muted, made louder or quieter, and faded, each in words", () => {
  const s = sample();
  assert.equal(ok(renameClip(s.model, s.mic, "  My voice ")).message, 'Clip "Voice" is now called "My voice".');
  assert.match(refused(renameClip(s.model, s.mic, "   ")), /needs a name/);
  assert.equal(ok(setMuted(s.model, s.mic, true)).message, 'Clip "Voice" is now muted.');
  assert.equal(ok(setMuted(s.model, s.mic, false)).model, s.model);
  assert.equal(ok(setVolume(s.model, s.mic, -6)).message, 'Clip "Voice" is now 6 decibels quieter than recorded.');
  assert.equal(ok(setVolume(s.model, s.mic, 3)).message, 'Clip "Voice" is now 3 decibels louder than recorded.');
  assert.match(refused(setVolume(s.model, s.mic, 25)), /from -40 to 20/);
  assert.match(refused(setVolume(s.model, s.mic, 1.5)), /whole number/);
  assert.equal(ok(setFades(s.model, s.mic, 500, 1500)).message, 'Clip "Voice" fades in over 0.5 seconds and out over 1.5 seconds.');
  assert.match(refused(setFades(s.model, s.mic, 40_000, 30_000)), /add up to 1 minute 10 seconds, but the clip is only 1 minute long/);
});

test("alt text can be given to an image or it can be marked decorative, and only images have it", () => {
  const s = sample();
  const added = ok(addClip(s.model, s.mediaTrack, { source: LOGO, startMs: 0 }));
  const id = added.focus?.[0] ?? "";
  assert.equal(validate(added.model).filter((p) => p.code === "needs-alt-text").length, 1);
  const text = ok(setImageText(added.model, id, { altText: " A blue circle with a white O " }));
  assert.equal(text.message, "Image \"Logo\" now has alt text: A blue circle with a white O");
  assert.equal(validate(text.model).filter((p) => p.code === "needs-alt-text").length, 0);
  const deco = ok(setImageText(added.model, id, { decorative: true }));
  assert.equal(deco.message, 'Image "Logo" is marked as decorative.');
  assert.equal(validate(deco.model).filter((p) => p.code === "needs-alt-text").length, 0);
  const blank = ok(setImageText(text.model, id, { altText: "   " }));
  assert.match(blank.message, /still needs alt text/);
  assert.match(refused(setImageText(s.model, s.screen, { altText: "x" })), /is not an image/);
});

test("an unknown clip is always said so in words", () => {
  const s = sample();
  for (const result of [splitAt(s.model, 1000, "x"), trimStart(s.model, "x", 1), trimEnd(s.model, "x", 1), deleteClip(s.model, "x", true), joinWithNext(s.model, "x"), reorderClip(s.model, "x", 0), renameClip(s.model, "x", "y"), setMuted(s.model, "x", true), setVolume(s.model, "x", 0), setFades(s.model, "x", 0, 0), setClipStart(s.model, "x", 0), setClipLength(s.model, "x", 1)]) {
    assert.equal(result.ok, false);
  }
  assert.match(refused(trimStart(s.model, "clip-404", 1)), /There is no clip with the id clip-404/);
});

test("nothing an action does changes the model it was given", () => {
  const s = sample();
  const frozen = structuredClone(s.model);
  splitAt(s.model, 10_000);
  trimStart(s.model, s.screen, 5000);
  deleteClip(s.model, s.screen, true);
  deleteRange(s.model, 1000, 2000, true);
  reorderClip(s.model, s.screen, 3);
  assert.deepEqual(s.model, frozen);
});

test("MUSIC and tracks: a second track of a kind is numbered", () => {
  const s = sample();
  const t = addTrack(s.model, "audio");
  assert.equal(t.message, 'Added the track "Audio 2".');
  assert.equal(addTrack(s.model, "audio", "Narration").message, 'Added the track "Narration".');
});
