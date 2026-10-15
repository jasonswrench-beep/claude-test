// SPDX-License-Identifier: Apache-2.0
// Drags (many small changes, one undo step), moving to another track, volume and fades, and the unsaved marker.
import assert from "node:assert/strict";
import { test } from "node:test";
import { ROOM_PAST_END_MS } from "../../capture-edit/src/index.ts";
import { parseDecibels, type ChangeKind } from "../src/session.ts";
import { names, open, row } from "./helpers.ts";

test("dragging a clip is many small changes and one undo step, and says what the whole drag came to", () => {
  const s = open("Check the totals"); // 60 to 90 seconds, the last clip on the Screen track
  const id = s.selectedId ?? "";
  assert.equal(s.beginDrag("move", id).ok, true);
  assert.equal(s.dragging, true);
  for (const ms of [61_000, 63_500, 68_000, 70_000]) s.dragTo(ms);
  assert.equal(row(s, "Check the totals").startMs, 70_000, "the edit follows the drag while it goes on");
  assert.equal(s.canUndo, false, "an open drag is not an undo step yet");
  const result = s.endDrag();
  assert.deepEqual([result.ok, result.message], [true, 'Clip "Check the totals" now starts at 1 minute 10 seconds.']);
  assert.equal(s.dragging, false);
  assert.equal(s.undoLabel, "move clip");
  s.undo();
  assert.equal(row(s, "Check the totals").startMs, 60_000, "one undo puts the whole drag back");
  assert.equal(s.canUndo, false);
});

test("a drag stops at the limits of what the edit allows", () => {
  const s = open("Check the totals"); // cannot start before the clip before it ends, at 60 seconds
  s.beginDrag("move", s.selectedId ?? "");
  assert.equal(s.dragTo(0), 60_000);
  assert.equal(s.dragTo(10 ** 9), 90_000 + ROOM_PAST_END_MS);
  s.cancelDrag();
  const start = open("Add an assignment"); // 30 to 60 between two clips: no room to move at all
  start.beginDrag("move", start.selectedId ?? "");
  assert.equal(start.dragTo(45_000), 30_000);
  start.cancelDrag();
});

test("Escape cancels a drag: everything is put back and nothing is recorded", () => {
  const s = open("Check the totals");
  const before = s.model;
  s.beginDrag("end", s.selectedId ?? "");
  s.dragTo(75_000);
  assert.equal(row(s, "Check the totals").endMs, 75_000);
  const result = s.cancelDrag();
  assert.deepEqual([result.ok, result.message], [true, "Cancelled. Nothing was changed."]);
  assert.equal(s.model, before);
  assert.equal(s.canUndo, false);
  assert.equal(s.dragging, false);
});

test("the grips drag the start and the end", () => {
  const s = open("Add an assignment"); // 30 to 60
  const id = s.selectedId ?? "";
  s.beginDrag("start", id);
  s.dragTo(40_000);
  assert.equal(s.endDrag().message, 'Clip "Add an assignment" now starts at 40 seconds, 10 seconds shorter.');
  assert.equal(s.undoLabel, "trim clip start");
  s.beginDrag("end", id);
  s.dragTo(50_000);
  assert.equal(s.endDrag().message, 'Clip "Add an assignment" now ends at 50 seconds, 10 seconds shorter.');
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.endMs], [40_000, 50_000]);
  assert.equal(s.undoLabel, "trim clip end");
});

test("the grips of an overlay work the same way, and the drag ends on a time the edit accepts", () => {
  const s = open("Title: Open Settings"); // 12 to 16
  const id = s.selectedId ?? "";
  s.beginDrag("end", id);
  s.dragTo(11_000); // earlier than its own start: stops a tenth of a second after it
  assert.equal(s.drag?.value, 12_100);
  assert.match(s.endDrag().message, /now ends at 12\.1 seconds/);
  s.beginDrag("move", id);
  s.dragTo(30_000);
  assert.match(s.endDrag().message, /now starts at 30 seconds/);
  assert.equal(s.undoLabel, "move");
});

test("while a mouse drag is open other commands are refused, and the drag is not lost", () => {
  const s = open("Check the totals");
  s.beginDrag("move", s.selectedId ?? "");
  s.dragTo(70_000);
  const refused = s.rename("Other");
  assert.deepEqual([refused.ok, refused.message], [false, "A drag is in progress. Let go to finish it, or press Escape to cancel it."]);
  assert.equal(s.undo().ok, false);
  assert.equal(s.beginDrag("move", s.selectedId ?? "").ok, false, "only one drag at a time");
  assert.equal(s.dragging, true);
  s.endDrag();
  assert.equal(row(s, "Check the totals").startMs, 70_000);
});

test("a key held on a grip is one drag that ends by itself when any other command comes", () => {
  const s = open("Add an assignment");
  const id = s.selectedId ?? "";
  s.beginDrag("end", id, "keys");
  for (let i = 0; i < 5; i++) assert.equal(s.dragStep("Left").ok, true);
  s.dragStep("Left", true);
  assert.equal(s.selectedRow?.endMs, 60_000 - 500 - 1000);
  assert.equal(s.canUndo, false);
  s.moveSelection("next"); // any other command settles the drag
  assert.equal(s.dragging, false);
  assert.equal(s.undoLabel, "trim clip end");
  s.undo();
  assert.equal(row(s, "Add an assignment").endMs, 60_000, "all six steps were one undo");
  assert.equal(s.canUndo, false);
});

test("a key that cannot go further says so in words", () => {
  const s = open("Add an assignment"); // 30 to 60, with no room on either side to move
  const id = s.selectedId ?? "";
  s.beginDrag("end", id, "keys");
  assert.equal(s.dragStep("Right").message, "It cannot end any later: the latest it can end is 1 minute.");
  s.dragStep("Home");
  assert.equal(s.selectedRow?.endMs, 30_100);
  assert.equal(s.dragStep("Left").message, "It cannot end any earlier: the earliest it can end is 30.1 seconds.");
  s.cancelDrag();
  s.beginDrag("start", id, "keys");
  assert.equal(s.dragStep("PageDown").message, "It cannot start any earlier: the earliest it can start is 30 seconds.");
  s.cancelDrag();
});

test("the screens are told about a drag as it goes, and again when it ends or is cancelled", () => {
  const s = open("Check the totals");
  const heard: ChangeKind[] = [];
  s.subscribe((k) => heard.push(k));
  s.beginDrag("move", s.selectedId ?? "");
  s.dragTo(61_000);
  s.dragTo(61_000); // the same place: nothing changes
  s.dragTo(62_000);
  s.endDrag();
  assert.deepEqual(heard, ["drag", "drag", "model"]);
  heard.length = 0;
  s.beginDrag("move", s.selectedId ?? "");
  s.dragTo(80_000);
  s.cancelDrag();
  assert.deepEqual(heard, ["drag", "model"]);
});

test("a drag that changed nothing records nothing", () => {
  const s = open("Check the totals");
  s.beginDrag("move", s.selectedId ?? "");
  s.dragTo(60_000);
  const result = s.endDrag();
  assert.equal(result.message, "Nothing was changed.");
  assert.equal(s.canUndo, false);
});

test("a drag that does not exist cannot begin", () => {
  const s = open();
  assert.deepEqual(s.beginDrag("move", "nope"), { ok: false, message: "There is no row with the id nope." });
  assert.equal(s.endDrag().ok, false);
  assert.equal(s.cancelDrag().ok, false);
  assert.equal(s.dragTo(1000), undefined);
  assert.equal(s.dragStep("Left").ok, false);
});

// ---- moving to another track ----

test("a clip can be dropped on another track that takes it, in the same undo step as the drag", () => {
  const s = open("Voice, part 3");
  assert.equal(s.addTrack("audio").message, 'Added the track "Audio 2".');
  const second = s.model.tracks.at(-1)?.id ?? "";
  const id = row(s, "Voice, part 3").clipId;
  s.select(id);
  s.beginDrag("move", id);
  s.dragTo(75_000);
  const result = s.endDrag({ dropTrackId: second });
  assert.equal(result.ok, true);
  assert.match(result.message, /^Clip "Voice, part 3" now starts at 1 minute 15 seconds\. Clip "Voice, part 3" moved from the Voice track to the Audio 2 track, still starting at 1 minute 15 seconds\./);
  assert.equal(row(s, "Voice, part 3").trackName, "Audio 2");
  s.undo();
  assert.deepEqual([row(s, "Voice, part 3").trackName, row(s, "Voice, part 3").startMs], ["Voice", 60_000], "one undo puts back both the move and the track");
});

test("a drop on a track that does not take the clip says why and leaves it where it was", () => {
  const s = open("Voice, part 3");
  const screen = s.model.tracks.find((t) => t.kind === "screen")?.id ?? "";
  s.beginDrag("move", s.selectedId ?? "");
  s.dragTo(70_000);
  const result = s.endDrag({ dropTrackId: screen });
  assert.match(result.message, /Cannot move "Voice, part 3" to the Screen track: that track takes screen recordings\./);
  assert.equal(row(s, "Voice, part 3").trackName, "Voice");
  assert.equal(row(s, "Voice, part 3").startMs, 70_000, "the sideways part of the drag still counts");
  assert.equal(result.ok, true);
  s.beginDrag("move", s.selectedId ?? "");
  const nothing = s.endDrag({ dropTrackId: screen });
  assert.equal(nothing.ok, false, "a drop that changes nothing and is refused is a refusal");
});

test("the Track box offers the tracks that take the row, and moving works from the form", () => {
  const s = open("Voice, part 1");
  s.addTrack("audio");
  s.addTrack("overlay");
  s.select(row(s, "Voice, part 1").clipId);
  assert.deepEqual(s.trackChoices.map((c) => [c.name, c.current]), [["Voice", true], ["Audio 2", false]]);
  const other = s.trackChoices.find((c) => !c.current)?.id ?? "";
  assert.match(s.moveSelectedToTrack(other).message, /moved from the Voice track to the Audio 2 track/);
  assert.equal(row(s, "Voice, part 1").trackName, "Audio 2");
  assert.equal(s.trackChoices.find((c) => c.current)?.name, "Audio 2");
  select2(s, "Title: Open Settings");
  assert.deepEqual(s.trackChoices.map((c) => c.name), ["Overlays", "Overlays 2"]);
  select2(s, "Zoom on the totals");
  assert.deepEqual(s.trackChoices.map((c) => c.name), ["Zoom"]);
  select2(s, "Open the gradebook");
  assert.deepEqual(s.trackChoices.map((c) => c.name), ["Screen"]);
});

function select2(s: ReturnType<typeof open>, name: string): void {
  s.select(row(s, name).clipId);
}

// ---- volume and fades ----

test("a volume can be typed in several ways, and a wrong one is explained", () => {
  assert.deepEqual(parseDecibels("-6"), { ok: true, db: -6 });
  assert.deepEqual(parseDecibels("+3"), { ok: true, db: 3 });
  assert.deepEqual(parseDecibels(" 3 dB "), { ok: true, db: 3 });
  assert.deepEqual(parseDecibels("-12 decibels"), { ok: true, db: -12 });
  assert.deepEqual(parseDecibels("0"), { ok: true, db: 0 });
  assert.deepEqual(parseDecibels("6.0"), { ok: true, db: 6 });
  const how = "Type a whole number of decibels from -40 to 20, such as -6 (quieter) or 3 (louder). 0 leaves the sound as recorded.";
  assert.deepEqual(parseDecibels("loud"), { ok: false, message: `"loud" is not a number of decibels. ${how}` });
  assert.deepEqual(parseDecibels("2.5"), { ok: false, message: `The volume is changed in whole decibels, so "2.5" is not allowed. ${how}` });
  assert.deepEqual(parseDecibels(""), { ok: false, message: `Nothing was typed. ${how}` });
});

test("volume and fades are set from the form, and say what they did", () => {
  const s = open("Voice, part 1");
  assert.equal(s.setVolumeText("-6").message, 'Clip "Voice, part 1" is now 6 decibels quieter than recorded.');
  assert.equal(s.details?.volumeDb, -6);
  assert.match(s.selectedRow ? s.describeRow(s.selectedId ?? "") : "", /6 decibels quieter/);
  assert.equal(s.setVolumeText("50").ok, false, "past the top of the range");
  assert.match(s.setVolumeText("50").message, /whole number of decibels from -40 to 20/);
  assert.equal(s.setFadeInText("2").ok, true);
  assert.equal(s.setFadeOutText("1.5").ok, true);
  assert.deepEqual([s.details?.fadeInMs, s.details?.fadeOutMs], [2000, 1500]);
  assert.match(s.describeRow(s.selectedId ?? ""), /fades in over 2 seconds, fades out over 1\.5 seconds/);
  const tooLong = s.setFadeInText("40");
  assert.equal(tooLong.ok, false);
  assert.deepEqual([s.details?.fadeInMs, s.details?.fadeOutMs], [2000, 1500], "a refused fade changes nothing");
  assert.equal(s.setFadeInText("soon").ok, false);
  assert.equal(s.setFadeOutText("0").ok, true, "0 turns a fade off");
  assert.equal(s.details?.fadeOutMs, 0);
});

test("overlays and zooms have no volume or fades", () => {
  const s = open("Title: Open Settings");
  assert.equal(s.setVolumeText("-6").message, '"Title: Open Settings" is a text overlay, not a clip. Only clips have a volume.');
  assert.equal(s.setFadeInText("1").message, '"Title: Open Settings" is a text overlay, not a clip. Only clips have fades.');
});

// ---- unsaved changes ----

test("the edit knows when it has unsaved changes, and undoing back to the saved edit clears that", () => {
  const s = open("Open the gradebook");
  const heard: ChangeKind[] = [];
  s.subscribe((k) => heard.push(k));
  assert.equal(s.dirty, false);
  s.rename("Intro");
  assert.equal(s.dirty, true);
  s.markSaved();
  assert.equal(s.dirty, false);
  assert.ok(heard.includes("saved"));
  s.rename("Welcome");
  assert.equal(s.dirty, true);
  s.undo();
  assert.equal(s.dirty, false, "back to what was saved");
  s.undo();
  assert.equal(s.dirty, true, "before what was saved");
  s.redo();
  assert.equal(s.dirty, false);
  assert.deepEqual(names(s, "Screen").slice(0, 1), ["Intro"]);
});

test("a drag in progress counts as a change, and a cancelled one does not", () => {
  const s = open("Check the totals");
  s.beginDrag("move", s.selectedId ?? "");
  s.dragTo(70_000);
  assert.equal(s.dirty, true);
  s.cancelDrag();
  assert.equal(s.dirty, false);
});
