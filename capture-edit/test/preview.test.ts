// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { COLUMNS, addClip, describeAt, describeClip, listCaption, listRows, setFades, setMuted, setVolume, splitAt, trimStart } from "../src/index.ts";
import { LOGO, ok, sample } from "./helpers.ts";

test("the clip list has a row for every clip, in words, with a caption", () => {
  const s = sample();
  const split = ok(splitAt(s.model, 30_000, s.screenTrack)).model;
  const rows = listRows(split);
  assert.equal(rows.length, 3);
  assert.deepEqual(rows.map((r) => [r.trackName, r.type, r.startMs, r.lengthMs, r.endMs, r.state]), [
    ["Screen", "Screen", 0, 30_000, 30_000, "ready"],
    ["Screen", "Screen", 30_000, 30_000, 60_000, "ready"],
    ["Audio", "Microphone", 0, 60_000, 60_000, "ready"],
  ]);
  assert.equal(listCaption(split), "Tracks and clips: 3 clips on 3 tracks, 1 minute long.");
  assert.equal(listCaption(s.model), "Tracks and clips: 2 clips on 3 tracks, 1 minute long.");
  assert.deepEqual([...COLUMNS], ["Name", "Track", "Type", "Start", "Length", "End", "State"]);
});

test("a row says when a clip is muted or needs something", () => {
  const s = sample();
  const muted = ok(setMuted(s.model, s.mic, true)).model;
  assert.equal(listRows(muted).find((r) => r.clipId === s.mic)?.state, "muted");
  const logo = ok(addClip(s.model, s.mediaTrack, { source: LOGO, startMs: 0, lengthMs: 3000 }));
  assert.equal(listRows(logo.model).find((r) => r.clipId === logo.focus?.[0])?.state, "needs attention");
});

test("a clip is described in one sentence", () => {
  const s = sample();
  const m = ok(setFades(ok(setVolume(ok(setMuted(s.model, s.mic, true)).model, s.mic, -6)).model, s.mic, 1000, 2000)).model;
  assert.equal(
    describeClip(m, s.mic),
    "Voice, microphone on the Audio track, starts at 0 seconds, 1 minute long, ends at 1 minute, muted, 6 decibels quieter, fades in over 1 second, fades out over 2 seconds.",
  );
  assert.equal(describeClip(m, "nope"), "There is no clip with the id nope.");
});

test("what is on screen at a moment is described in words, with what the rest of the editor knows", () => {
  const s = sample();
  const cut = ok(trimStart(s.model, s.screen, 10_000)).model;
  assert.equal(
    describeAt(cut, 72_000 - 60_000),
    "At 12 seconds. Screen track: Lesson, 2 seconds in. Audio track: Voice, 12 seconds in. Imported media track: nothing here, and nothing later. No overlays showing. No caption showing. Zoom: none.",
  );
  assert.match(describeAt(cut, 5000), /Screen track: nothing here; Lesson starts at 10 seconds\./);
  const full = describeAt(cut, 20_000, {
    window: "Settings",
    cursorTarget: "the Save button",
    overlays: ["Title: Welcome"],
    caption: "First, open Settings.",
    zoom: "1.5 times on the top left",
  });
  assert.match(full, /Window in front: Settings\. The cursor is over the Save button\. Overlays showing: Title: Welcome\. Caption: First, open Settings\. Zoom: 1\.5 times on the top left\.$/);
});

test("an edit with no tracks is described too", () => {
  assert.match(describeAt({ ...sample().model, tracks: [] }, 0), /The edit has no tracks yet\./);
});
