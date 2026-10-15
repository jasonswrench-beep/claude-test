// SPDX-License-Identifier: Apache-2.0
// Choosing several rows, and what the commands do with them.
import assert from "node:assert/strict";
import { test } from "node:test";
import { findClip } from "../../capture-edit/src/index.ts";
import { EditSession, type ChangeKind } from "../src/session.ts";
import { names, open, row, select } from "./helpers.ts";

test("Shift with the arrow keys chooses a range of rows from where it started", () => {
  const s = open("Add an assignment");
  const first = s.moveSelection("next", true);
  assert.deepEqual([first.ok, first.changed, first.message], [true, true, "2 rows are chosen."]);
  assert.equal(s.selectedRow?.name, "Check the totals", "the focus is on the row the range ends at");
  assert.deepEqual(s.selectedRows.map((r) => r.name), ["Add an assignment", "Check the totals"]);
  s.moveSelection("next", true);
  assert.equal(s.selectionCount, 3);
  s.moveSelection("previous", true);
  s.moveSelection("previous", true);
  assert.deepEqual(s.selectedRows.map((r) => r.name), ["Add an assignment"], "back to where it started");
  s.moveSelection("previous", true);
  assert.deepEqual(s.selectedRows.map((r) => r.name), ["Open the gradebook", "Add an assignment"], "the range runs the other way too");
  assert.equal(s.selectedRow?.name, "Open the gradebook");
});

test("choosing a row without Shift or Ctrl goes back to just that row", () => {
  const s = open("Add an assignment");
  s.moveSelection("next", true);
  assert.equal(s.selectionCount, 2);
  s.moveSelection("next");
  assert.equal(s.selectionCount, 1);
  s.moveSelection("last", true);
  assert.ok(s.selectionCount > 1);
  s.select(row(s, "Voice, part 2").clipId);
  assert.deepEqual(s.selectedRows.map((r) => r.name), ["Voice, part 2"]);
});

test("Ctrl+Space adds a row to the choice or takes it out, and one row always stays", () => {
  const s = open("Open the gradebook");
  const add = s.toggleSelected(row(s, "Voice, part 1").clipId);
  assert.deepEqual([add.ok, add.message], [true, '"Voice, part 1" is chosen too. 2 rows are chosen.']);
  assert.equal(s.selectedRow?.name, "Voice, part 1", "the new row has the focus");
  assert.equal(s.isSelected(row(s, "Open the gradebook").clipId), true);
  const take = s.toggleSelected();
  assert.equal(take.message, '"Voice, part 1" is no longer chosen. 1 row is chosen.');
  assert.equal(s.selectedRow?.name, "Voice, part 1", "the focus stays on the row that was taken out");
  assert.equal(s.singleChoice, false, "the focus is on a row that is not chosen");
  const put = s.toggleSelected();
  assert.equal(put.message, '"Voice, part 1" is chosen too. 2 rows are chosen.');
  s.toggleSelected(row(s, "Open the gradebook").clipId);
  const last = s.toggleSelected(row(s, "Voice, part 1").clipId);
  assert.deepEqual([last.ok, last.changed, last.message], [false, false, "One row has to stay chosen."]);
});

test("Ctrl with the arrow keys moves the focus without changing the choice, so rows that are not next to each other can be chosen", () => {
  const s = open("Open the gradebook");
  const first = s.moveFocus("next");
  assert.deepEqual([first.ok, first.changed], [true, true]);
  assert.equal(s.selectedRow?.name, "Add an assignment");
  assert.deepEqual(s.selectedRows.map((r) => r.name), ["Open the gradebook"], "still the first row that is chosen");
  assert.equal(s.singleChoice, false);
  s.moveFocus("next");
  assert.equal(s.toggleSelected().message, '"Check the totals" is chosen too. 2 rows are chosen.');
  assert.deepEqual(s.selectedRows.map((r) => r.name), ["Open the gradebook", "Check the totals"], "the one between is not chosen");
  assert.equal(s.moveFocus("last").changed, true);
  assert.equal(s.moveFocus("next").message, "This is the last row.");
  assert.equal(s.moveFocus("first").changed, true);
  assert.equal(s.moveFocus("previous").message, "This is the first row.");
  assert.equal(s.focusOn("nope").ok, false);
  assert.equal(s.focusOn(row(s, "Voice, part 2").clipId).changed, true);
  assert.equal(s.selectionCount, 2);
});

test("with the focus on a row that is not chosen, the commands act on the chosen rows, not on the row with the focus", () => {
  const s = open("Voice, part 1");
  s.moveFocus("next");
  assert.equal(s.selectedRow?.name, "Voice, part 2");
  assert.equal(s.rename("Fine").ok, true);
  assert.equal(row(s, "Fine").trackName, "Voice");
  assert.deepEqual(names(s, "Voice").slice(0, 2), ["Fine", "Voice, part 2"], "the chosen row, Voice part 1, was renamed");
  assert.equal(s.singleChoice, false);
});

test("Ctrl+A chooses every row and Escape keeps just the one with the focus", () => {
  const s = open("Add an assignment");
  assert.equal(s.selectAll().message, "All 12 rows are chosen.");
  assert.equal(s.selectionCount, 12);
  assert.equal(s.selectedRow?.name, "Add an assignment");
  assert.equal(s.selectAll().changed, false);
  assert.equal(s.keepOnlyPrimary().message, 'Only "Add an assignment" is chosen now.');
  assert.equal(s.selectionCount, 1);
  assert.deepEqual([s.keepOnlyPrimary().changed, s.keepOnlyPrimary().message], [false, "Only one row is chosen."]);
});

test("the screens are told when the choice changes, and only then", () => {
  const s = open("Add an assignment");
  const heard: ChangeKind[] = [];
  s.subscribe((k) => heard.push(k));
  s.moveSelection("next", true);
  s.toggleSelected(row(s, "Voice, part 1").clipId);
  s.selectAll();
  s.selectAll();
  s.keepOnlyPrimary();
  assert.deepEqual(heard, ["selection", "selection", "selection", "selection"]);
});

test("deleting several rows is one undo step, and says how many", () => {
  const s = open("Voice, part 1");
  s.moveSelection("next", true);
  s.moveSelection("next", true);
  const result = s.deleteSelected(false);
  assert.deepEqual([result.ok, result.message.startsWith("Deleted 3 rows, leaving the gaps.")], [true, true]);
  assert.deepEqual(names(s, "Voice"), []);
  assert.equal(s.undoLabel, "delete rows");
  assert.match(s.undo().message, /^Undid: delete rows\./);
  assert.equal(names(s, "Voice").length, 3, "one undo brings back all three");
  assert.equal(s.canUndo, false);
});

test("deleting several clips can close the gaps", () => {
  const s = open("Open the gradebook");
  s.toggleSelected(row(s, "Add an assignment").clipId);
  const result = s.deleteSelected(true);
  assert.match(result.message, /^Deleted 2 rows, closing the gaps\./);
  assert.equal(row(s, "Check the totals").startMs, 0, "the clip after both moved to the start");
});

test("muting several clips is one step; with all muted the same key unmutes them", () => {
  const s = open("Voice, part 1");
  s.moveSelection("next", true);
  assert.equal(s.toggleMute().message, "Muted 2 clips.");
  assert.deepEqual(s.selectedRows.map((r) => r.state), ["muted", "muted"]);
  assert.equal(s.undoLabel, "mute clips");
  assert.equal(s.toggleMute().message, "Unmuted 2 clips.");
  s.toggleSelected(row(s, "Voice, part 3").clipId);
  s.setSelectedMuted(true);
  s.toggleSelected(row(s, "Voice, part 3").clipId); // take it out again
  assert.equal(s.toggleMute().message, "Unmuted 2 clips.", "a mix of muted and not is muted first; here the two chosen are muted, so they are unmuted");
});

test("muting rows that are not all clips says what was left out", () => {
  const s = open("Voice, part 1");
  s.toggleSelected(row(s, "Zoom on the totals").clipId);
  assert.equal(s.setSelectedMuted(true).message, "Muted 1 clip. The other 1 chosen rows are not clips.");
  select(s, "Title: Open Settings");
  s.toggleSelected(row(s, "Zoom on the totals").clipId);
  assert.equal(s.setSelectedMuted(true).message, "None of the chosen rows is a clip, and only clips can be muted.");
});

test("split cuts each chosen clip the playhead is inside, as one step, and leaves the rest alone", () => {
  const s = open("Open the gradebook"); // 0 to 30
  s.toggleSelected(row(s, "Voice, part 2").clipId); // 30 to 60, so the playhead at 20 is not inside it
  s.toggleSelected(row(s, "Voice, part 1").clipId); // 0 to 30
  s.setPlayhead(20_000);
  const result = s.splitSelected();
  assert.equal(result.message, "Split 2 clips at 20 seconds; each is now two clips. The other 1 chosen rows were left alone.");
  assert.equal(s.rows.length, 14);
  assert.equal(s.undoLabel, "split clips");
  s.undo();
  assert.equal(s.rows.length, 12, "one undo");
});

test("split with several chosen, none of them under the playhead, says so and changes nothing", () => {
  const s = open("Open the gradebook");
  s.toggleSelected(row(s, "Voice, part 2").clipId);
  s.setPlayhead(45_000);
  const before = s.model;
  const result = s.splitSelected();
  assert.equal(result.ok, true, "Voice part 2 is under the playhead at 45 seconds");
  s.undo();
  s.setPlayhead(80_000);
  const none = s.splitSelected();
  assert.deepEqual([none.ok, none.message], [false, "None of the 2 chosen rows can be split at 1 minute 20 seconds: the playhead is not inside any of the clips."]);
  assert.equal(s.model, before);
});

test("trimming several rows to the playhead moves all their starts", () => {
  const s = open("Add an assignment"); // 30 to 60
  s.toggleSelected(row(s, "Voice, part 2").clipId); // 30 to 60
  s.setPlayhead(40_000);
  const result = s.trimStartToPlayhead();
  assert.equal(result.message, "Moved the start of 2 rows to 40 seconds.");
  assert.deepEqual(s.selectedRows.map((r) => [r.name, r.startMs]), [["Add an assignment", 40_000], ["Voice, part 2", 40_000]]);
  s.setPlayhead(50_000);
  s.trimEndToPlayhead();
  assert.deepEqual(s.selectedRows.map((r) => r.endMs), [50_000, 50_000]);
});

test("the commands that need one row say so when several are chosen", () => {
  const s = open("Add an assignment");
  s.moveSelection("next", true);
  const why = "This works on one row at a time, and 2 rows are chosen. Press Escape to keep just the one you are on.";
  for (const result of [s.joinSelectedWithNext(), s.moveSelected("later"), s.rename("x"), s.setStartText("1"), s.setLengthText("1"), s.setEndText("1"), s.setVolumeText("1"), s.setFadeInText("1"), s.moveSelectedToTrack("track-1")]) {
    assert.deepEqual(result, { ok: false, message: why });
  }
  s.keepOnlyPrimary();
  assert.equal(s.rename("Fine").ok, true);
});

test("the choice is kept sound when rows go away: undo of a split drops the piece that was chosen", () => {
  const s = open("Open the gradebook");
  s.setPlayhead(10_000);
  s.splitSelected();
  s.moveSelection("next", true); // both pieces chosen
  assert.equal(s.selectionCount, 2);
  s.undo();
  assert.equal(s.selectionCount, 1);
  assert.ok(s.selectedRow);
  assert.equal(findClip(s.model, s.selectedId ?? "")?.clip.name, "Open the gradebook");
});

test("when the row with the focus goes away, the focus goes to the chosen row nearest to it", () => {
  const s = open("Voice, part 2");
  s.setPlayhead(40_000);
  s.splitSelected(); // Voice part 2 becomes two pieces
  const pieces = s.rows.filter((r) => r.name === "Voice, part 2");
  assert.equal(pieces.length, 2);
  s.select(row(s, "Open the gradebook").clipId);
  s.toggleSelected(row(s, "Voice, part 3").clipId);
  s.toggleSelected(pieces[1]?.clipId); // the second piece has the focus, and is chosen
  assert.equal(s.selectionCount, 3);
  s.undo(); // the second piece is gone
  assert.equal(s.selectedRow?.name, "Voice, part 3", "not the first row of the choice, but the nearest one");
  assert.equal(s.selectionCount, 2);
});

test("the sentence for the row with the focus says how many are chosen", () => {
  const s = open("Add an assignment");
  assert.doesNotMatch(s.describeSelected().message, /chosen/);
  s.moveSelection("next", true);
  assert.match(s.describeSelected().message, /^2 rows are chosen\. The one with the focus: Check the totals,/);
});

test("an empty edit has nothing to choose", () => {
  const s = new EditSession(open().model);
  s.deleteSelected(false);
  assert.ok(s.rows.length > 0);
});
