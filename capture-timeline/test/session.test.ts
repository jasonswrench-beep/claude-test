// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { addClip, addTrack, createModel, splitAt } from "../../capture-edit/src/index.ts";
import { SAMPLE_SOURCES, sampleModel } from "../src/sample.ts";
import { EditSession, type ChangeKind } from "../src/session.ts";
import { names, open, row, select, solo } from "./helpers.ts";

// ---- the list and the selection ----

test("the list starts on its first row and describes what it holds", () => {
  const s = open();
  assert.equal(s.rows.length, 12);
  assert.equal(s.selectedRow?.name, "Open the gradebook");
  assert.equal(s.caption, "Tracks and clips: 9 clips, 3 overlays and zooms on 5 tracks, 1 minute 30 seconds long.");
  assert.equal(s.lengthMs, 90_000);
  assert.equal(s.playheadMs, 0);
  assert.equal(s.canUndo, false);
  assert.equal(s.canRedo, false);
});

test("every row is read as a sentence", () => {
  const s = open();
  assert.equal(s.describeRow(row(s, "Add an assignment").clipId), "Add an assignment, screen on the Screen track, starts at 30 seconds, 30 seconds long, ends at 1 minute.");
  assert.match(s.describeRow(row(s, "Title: Open Settings").clipId), /^Title: Open Settings, text overlay on the Overlays track, starts at 12 seconds, 4 seconds long, ends at 16 seconds, says: Open Settings/);
  assert.equal(s.describeRow("nope"), "There is no row with the id nope.");
  // and the sentence follows the edit, it is not kept from before
  s.select(row(s, "Add an assignment").clipId);
  s.setPlayhead(40_000);
  s.trimStartToPlayhead();
  assert.equal(s.describeRow(row(s, "Add an assignment").clipId), "Add an assignment, screen on the Screen track, starts at 40 seconds, 20 seconds long, ends at 1 minute.");
});

test("the arrow keys move the selection one row, and Home and End go to the ends", () => {
  const s = open();
  const first = s.selectedId;
  const moved = s.moveSelection("next");
  assert.equal(moved.changed, true);
  assert.equal(s.selectedRow?.name, "Add an assignment");
  assert.equal(moved.message, s.describeRow(row(s, "Add an assignment").clipId));
  s.moveSelection("previous");
  assert.equal(s.selectedId, first);
  const atTop = s.moveSelection("previous");
  assert.deepEqual([atTop.changed, atTop.message], [false, "This is the first row."]);
  assert.equal(s.moveSelection("last").changed, true);
  assert.equal(s.selectedRow?.name, "Zoom on the totals");
  const atEnd = s.moveSelection("next");
  assert.deepEqual([atEnd.changed, atEnd.message], [false, "This is the last row."]);
  s.moveSelection("first");
  assert.equal(s.selectedId, first);
});

test("selecting a row that does not exist changes nothing", () => {
  const s = open();
  const before = s.selectedId;
  const result = s.select("nope");
  assert.deepEqual([result.ok, result.changed, result.message], [false, false, "There is no row with the id nope."]);
  assert.equal(s.selectedId, before);
});

test("an empty edit says so, and every command that needs a row says to select one", () => {
  const s = new EditSession(createModel());
  assert.equal(s.selectedId, undefined);
  assert.equal(s.details, undefined);
  assert.deepEqual(s.moveSelection("next"), { ok: false, message: "The list is empty.", changed: false });
  const nothing = "Nothing is selected. Select a row in the list first.";
  for (const result of [
    s.splitSelected(),
    s.trimStartToPlayhead(),
    s.trimEndToPlayhead(),
    s.deleteSelected(false),
    s.joinSelectedWithNext(),
    s.moveSelected("later"),
    s.toggleMute(),
    s.rename("x"),
    s.setStartText("1"),
    s.setLengthText("1"),
    s.setEndText("1"),
    s.playheadToSelectedStart(),
    s.playheadToSelectedEnd(),
    s.describeSelected(),
  ]) {
    assert.deepEqual(result, { ok: false, message: nothing });
  }
});

test("what the form shows: a clip can be muted, an overlay is not a clip", () => {
  const s = open("Voice, part 1");
  assert.deepEqual(s.details, {
    id: row(s, "Voice, part 1").clipId,
    name: "Voice, part 1",
    type: "Microphone",
    trackId: row(s, "Voice, part 1").trackId,
    trackName: "Voice",
    startMs: 0,
    lengthMs: 30_000,
    endMs: 30_000,
    isClip: true,
    muted: false,
    volumeDb: 0,
    fadeInMs: 0,
    fadeOutMs: 0,
  });
  select(s, "Zoom on the totals");
  assert.equal(s.details?.isClip, false);
  assert.equal(s.details?.type, "Zoom");
});

// ---- the playhead ----

test("the playhead steps by a tenth of a second, a second with Shift, five seconds with the Page keys", () => {
  const s = open();
  assert.equal(s.stepPlayhead("Right").message, 'Playhead at 0.1 seconds, 0.1 seconds into "Open the gradebook".');
  assert.equal(s.playheadMs, 100);
  s.stepPlayhead("Right", true);
  assert.equal(s.playheadMs, 1100);
  s.stepPlayhead("PageUp");
  assert.equal(s.playheadMs, 6100);
  s.stepPlayhead("PageDown");
  s.stepPlayhead("Left");
  assert.equal(s.playheadMs, 1000);
  assert.equal(s.stepPlayhead("End").message, 'Playhead at 1 minute 30 seconds, 1 minute after "Open the gradebook".');
  assert.equal(s.playheadMs, 90_000);
  assert.equal(s.stepPlayhead("Home").message, 'Playhead at 0 seconds, at the start of "Open the gradebook".');
});

test("the playhead stops at the ends and says so", () => {
  const s = open();
  assert.equal(s.stepPlayhead("Left").message, "The playhead is at the start.");
  assert.equal(s.stepPlayhead("PageDown").message, "The playhead is at the start.");
  s.stepPlayhead("End");
  assert.equal(s.stepPlayhead("Right").message, "The playhead is at the end, 1 minute 30 seconds.");
  assert.equal(s.stepPlayhead("End").message, "The playhead is at the end, 1 minute 30 seconds.");
  assert.equal(new EditSession(createModel()).stepPlayhead("Right").message, "The edit is empty, so the playhead stays at 0 seconds.");
});

test("the playhead says where it is in relation to the selected row", () => {
  const s = open("Add an assignment"); // 30 to 60 seconds
  assert.equal(s.setPlayhead(10_000).message, 'Playhead at 10 seconds, 20 seconds before "Add an assignment".');
  assert.equal(s.setPlayhead(30_000).message, 'Playhead at 30 seconds, at the start of "Add an assignment".');
  assert.equal(s.setPlayhead(42_500).message, 'Playhead at 42.5 seconds, 12.5 seconds into "Add an assignment".');
  assert.equal(s.setPlayhead(60_000).message, 'Playhead at 1 minute, at the end of "Add an assignment".');
  assert.equal(s.setPlayhead(75_000).message, 'Playhead at 1 minute 15 seconds, 15 seconds after "Add an assignment".');
});

test("a time can be typed for the playhead, and a wrong one is explained", () => {
  const s = open();
  assert.equal(s.setPlayheadText("1:12.4").ok, true);
  assert.equal(s.playheadMs, 72_400);
  assert.equal(s.setPlayheadText("12").ok, true);
  assert.equal(s.playheadMs, 12_000);
  const bad = s.setPlayheadText("soon");
  assert.equal(bad.ok, false);
  assert.match(bad.message, /"soon" is not a time\. Type a time such as 72\.4/);
  assert.equal(s.playheadMs, 12_000);
  const past = s.setPlayheadText("5:00");
  assert.equal(past.ok, true);
  assert.equal(s.playheadMs, 90_000);
  assert.match(past.message, /^The edit is only 1 minute 30 seconds long, so the playhead went to the end\. Playhead at 1 minute 30 seconds/);
});

test("the playhead can jump to the start or end of the selected row", () => {
  const s = open("Add an assignment");
  s.playheadToSelectedEnd();
  assert.equal(s.playheadMs, 60_000);
  s.playheadToSelectedStart();
  assert.equal(s.playheadMs, 30_000);
});

// ---- splitting ----

test("split cuts the selected clip at the playhead and stays on the first piece", () => {
  const s = open("Open the gradebook");
  const id = s.selectedId;
  s.setPlayhead(10_000);
  const result = s.splitSelected();
  assert.deepEqual([result.ok, result.message], [true, "Clip split at 10 seconds, now two clips."]);
  assert.equal(s.selectedId, id);
  assert.deepEqual(
    s.rows.filter((r) => r.trackName === "Screen").map((r) => [r.name, r.startMs, r.lengthMs]),
    [
      ["Open the gradebook", 0, 10_000],
      ["Open the gradebook", 10_000, 20_000],
      ["Add an assignment", 30_000, 30_000],
      ["Check the totals", 60_000, 30_000],
    ],
  );
  assert.equal(s.canUndo, true);
  assert.equal(s.undoLabel, "split clip");
});

test("split leaves the other tracks alone, and says why when the playhead is not inside the clip", () => {
  const s = open("Add an assignment"); // 30 to 60 seconds
  s.setPlayhead(10_000);
  const before = s.model;
  const result = s.splitSelected();
  assert.equal(result.ok, false);
  assert.equal(result.message, 'Cannot split "Add an assignment" at 10 seconds: it runs from 30 seconds to 1 minute, so the playhead has to be inside it.');
  s.setPlayhead(30_000);
  assert.match(s.splitSelected().message, /^Cannot split "Add an assignment" at 30 seconds/, "at the very start there is nothing to cut off");
  assert.equal(s.model, before);
  assert.equal(s.canUndo, false);
  s.setPlayhead(45_000);
  assert.equal(s.splitSelected().ok, true);
  assert.equal(names(s, "Voice").length, 3, "the voice track was not cut");
});

test("split every track cuts picture and sound together", () => {
  const s = open();
  s.setPlayhead(45_000);
  const result = s.splitEveryTrack();
  assert.deepEqual([result.ok, result.message], [true, "Split 3 clips at 45 seconds; each is now two clips."]);
  assert.equal(s.rows.filter((r) => r.type !== "Text overlay" && r.type !== "Arrow" && r.type !== "Zoom").length, 12);
  s.setPlayhead(50_000);
  s.splitEveryTrack();
  assert.equal(s.canUndo, true);
  s.undo();
  assert.equal(s.rows.length, 15, "one undo undoes the whole split of all three tracks");
});

test("an overlay or zoom cannot be split, and the answer says what it is", () => {
  const s = open("Title: Open Settings");
  s.setPlayhead(13_000);
  const result = s.splitSelected();
  assert.deepEqual([result.ok, result.message], [false, '"Title: Open Settings" is a text overlay, not a clip. Only clips can be split.']);
  select(s, "Arrow to the Add button");
  assert.equal(s.splitSelected().message, '"Arrow to the Add button" is an arrow, not a clip. Only clips can be split.');
});

// ---- trimming ----

test("trimming moves the start or the end of a clip to the playhead", () => {
  const s = open("Add an assignment"); // 30 to 60 seconds
  s.setPlayhead(40_000);
  assert.equal(s.trimStartToPlayhead().message, 'Clip "Add an assignment" now starts at 40 seconds, 10 seconds shorter.');
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.endMs], [40_000, 60_000]);
  s.setPlayhead(50_000);
  assert.equal(s.trimEndToPlayhead().message, 'Clip "Add an assignment" now ends at 50 seconds, 10 seconds shorter.');
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.endMs], [40_000, 50_000]);
  assert.equal(s.undoLabel, "trim clip end");
});

test("trimming an overlay or zoom moves its start or end the same way", () => {
  const s = open("Title: Open Settings"); // 12 to 16 seconds
  s.setPlayhead(13_000);
  assert.match(s.trimStartToPlayhead().message, /now starts at 13 seconds and ends at 16 seconds/);
  s.setPlayhead(15_000);
  assert.match(s.trimEndToPlayhead().message, /now ends at 15 seconds, lasting 2 seconds/);
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.lengthMs], [13_000, 2000]);
});

test("a trim the clip cannot take is refused in words and changes nothing", () => {
  const s = open("Add an assignment"); // 30 to 60 seconds
  s.setPlayhead(30_050);
  const before = s.model;
  const result = s.trimEndToPlayhead();
  assert.equal(result.ok, false);
  assert.match(result.message, /shorter than 0\.1 seconds|at least 0\.1 seconds/);
  assert.equal(s.model, before);
});

// ---- deleting ----

test("deleting can leave a gap, and the selection goes to the row that took the place", () => {
  const s = open("Add an assignment");
  const result = s.deleteSelected(false);
  assert.deepEqual([result.ok, result.message], [true, 'Clip "Add an assignment" deleted, leaving a gap of 30 seconds. Selected "Check the totals".']);
  assert.equal(s.selectedRow?.name, "Check the totals");
  assert.equal(row(s, "Check the totals").startMs, 60_000, "nothing moved");
  assert.equal(s.undoLabel, "delete clip");
});

test("deleting can close the gap, which moves the later clips earlier", () => {
  const s = open("Add an assignment");
  const result = s.deleteSelected(true);
  assert.equal(result.message, 'Clip "Add an assignment" deleted, and the clip after it moved 30 seconds earlier to close the gap. Selected "Check the totals".');
  assert.equal(row(s, "Check the totals").startMs, 30_000);
});

test("deleting the last row selects the one before it", () => {
  const s = open("Zoom on the totals");
  const result = s.deleteSelected(false);
  assert.equal(result.message, 'Zoom "Zoom on the totals" deleted. Selected "Arrow to the Add button".');
  assert.equal(s.selectedRow?.name, "Arrow to the Add button");
});

test("deleting an overlay works the same whichever way the gap is asked for", () => {
  for (const closeGap of [false, true]) {
    const s = open("Title: Open Settings");
    assert.equal(s.deleteSelected(closeGap).ok, true);
    assert.equal(s.rows.length, 11);
  }
});

test("deleting the only row leaves an empty list, and the playhead cannot stay past the end", () => {
  const s = new EditSession(solo());
  s.setPlayhead(10_000);
  const result = s.deleteSelected(false);
  assert.equal(result.message, 'Clip "Only clip" deleted, leaving a gap of 10 seconds. The list is now empty. The playhead moved back to the end, 0 seconds.');
  assert.equal(s.selectedId, undefined);
  assert.equal(s.rows.length, 0);
  assert.equal(s.playheadMs, 0);
  assert.equal(s.undo().ok, true);
  assert.equal(s.rows.length, 1);
});

// ---- joining and moving ----

test("clips that were split can be joined again", () => {
  const s = open("Open the gradebook");
  s.setPlayhead(10_000);
  s.splitSelected();
  const id = s.selectedId;
  const result = s.joinSelectedWithNext();
  assert.deepEqual([result.ok, result.message], [true, "Joined 2 clips into one clip of 30 seconds."]);
  assert.equal(s.selectedId, id);
  assert.equal(s.rows.length, 12);
});

test("a join that cannot be done says why", () => {
  const s = open("Check the totals");
  assert.match(s.joinSelectedWithNext().message, /last clip on the Screen track/);
  select(s, "Zoom on the totals");
  assert.equal(s.joinSelectedWithNext().message, '"Zoom on the totals" is a zoom, not a clip. Only clips can be joined.');
});

test("a clip moves one place earlier or later, and the ends say so", () => {
  const s = open("Add an assignment");
  const result = s.moveSelected("earlier");
  assert.equal(result.ok, true);
  assert.deepEqual(names(s, "Screen"), ["Add an assignment", "Open the gradebook", "Check the totals"]);
  assert.equal(row(s, "Add an assignment").startMs, 0);
  assert.equal(s.moveSelected("earlier").message, '"Add an assignment" is already the first clip on the Screen track.');
  select(s, "Check the totals");
  assert.equal(s.moveSelected("later").message, '"Check the totals" is already the last clip on the Screen track.');
  assert.equal(s.moveSelected("earlier").ok, true);
  select(s, "Title: Open Settings");
  assert.equal(s.moveSelected("later").message, '"Title: Open Settings" is a text overlay, not a clip. Overlays and zooms are placed by their start time, so they have no order to change.');
});

// ---- the form ----

test("muting a clip, and unmuting it", () => {
  const s = open("Voice, part 1");
  assert.equal(s.toggleMute().message, 'Clip "Voice, part 1" is now muted.');
  assert.equal(s.details?.muted, true);
  assert.equal(s.selectedRow?.state, "muted");
  assert.equal(s.toggleMute().message, 'Clip "Voice, part 1" is now not muted.');
  assert.equal(s.details?.muted, false);
  assert.equal(s.setSelectedMuted(false).message, 'Clip "Voice, part 1" is already not muted.');
  assert.equal(s.undoLabel, "unmute clip");
  select(s, "Zoom on the totals");
  assert.equal(s.toggleMute().message, '"Zoom on the totals" is a zoom, not a clip. Only clips can be muted.');
});

test("a clip or an overlay can be renamed, and an empty name is refused", () => {
  const s = open("Open the gradebook");
  assert.equal(s.rename("Welcome").message, 'Clip "Open the gradebook" is now called "Welcome".');
  assert.equal(s.selectedRow?.name, "Welcome");
  assert.deepEqual([s.rename("   ").ok, s.rename("   ").message], [false, "A clip needs a name."]);
  select(s, "Title: Open Settings");
  assert.match(s.rename("Heading").message, /is now called "Heading"/);
  assert.equal(s.selectedRow?.name, "Heading");
  assert.equal(s.rename("").ok, false);
});

test("exact times typed into the form move a clip or an overlay", () => {
  const s = open("Title: Open Settings"); // 12 s, 4 s long
  assert.equal(s.setStartText("20").message.startsWith('Text overlay "Title: Open Settings" now starts at 20 seconds'), true);
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.lengthMs], [20_000, 4000]);
  assert.match(s.setLengthText("2.5").message, /now ends at 22\.5 seconds, lasting 2\.5 seconds/);
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.lengthMs], [20_000, 2500]);
  assert.match(s.setEndText("0:30").message, /now ends at 30 seconds, lasting 10 seconds/);
  assert.equal(s.selectedRow?.endMs, 30_000);
});

test("exact times typed into the form change a clip", () => {
  const s = open("Check the totals"); // 60 to 90 seconds, the end of the recording
  assert.match(s.setLengthText("10").message, /^Clip "Check the totals" is now 10 seconds long, ending at 1 minute 10 seconds/);
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.endMs], [60_000, 70_000]);
  assert.match(s.setStartText("1:15").message, /^Clip "Check the totals" now starts at 1 minute 15 seconds/);
  assert.deepEqual([s.selectedRow?.startMs, s.selectedRow?.endMs], [75_000, 85_000]);
  assert.match(s.setEndText("1:20").message, /^Clip "Check the totals" now ends at 1 minute 20 seconds/);
  assert.equal(s.selectedRow?.endMs, 80_000);
});

test("a time the clips cannot take is refused in words, and a time that is not a time is explained", () => {
  const s = open("Add an assignment"); // 30 to 60 seconds, between two other clips
  const before = s.model;
  const early = s.setStartText("10");
  assert.equal(early.ok, false);
  assert.match(early.message, /would run into the clip "Open the gradebook"/);
  const late = s.setLengthText("45"); // would run into the next clip
  assert.equal(late.ok, false);
  const words = s.setStartText("whenever");
  assert.equal(words.ok, false);
  assert.match(words.message, /^"whenever" is not a time\. Type a time such as 72\.4 \(seconds\)/);
  assert.equal(s.setEndText("").ok, false);
  assert.equal(s.model, before);
  assert.equal(s.canUndo, false);
});

test("typing what is already there changes nothing and is not an undo step", () => {
  const s = open("Add an assignment");
  const result = s.setStartText("30");
  assert.deepEqual([result.ok, result.message], [true, 'The clip "Add an assignment" already starts at 30 seconds.']);
  assert.equal(s.canUndo, false);
  assert.equal(s.rename("Add an assignment").message, 'The clip is already called "Add an assignment".');
  assert.equal(s.canUndo, false);
});

// ---- undo and redo ----

test("undo and redo say what they did, and each command is one step", () => {
  const s = open("Open the gradebook");
  assert.deepEqual([s.undo().ok, s.undo().message], [false, "Nothing to undo."]);
  assert.deepEqual([s.redo().ok, s.redo().message], [false, "Nothing to redo."]);
  s.setPlayhead(10_000);
  s.splitSelected();
  s.rename("Intro");
  assert.equal(s.undo().message, "Undid: rename clip.");
  assert.equal(s.selectedRow?.name, "Open the gradebook");
  assert.equal(s.redoLabel, "rename clip");
  assert.equal(s.undo().message, "Undid: split clip.");
  assert.equal(s.rows.length, 12);
  assert.equal(s.redo().message, "Redid: split clip.");
  assert.equal(s.rows.length, 13);
  assert.equal(s.redo().message, "Redid: rename clip.");
  assert.equal(s.selectedRow?.name, "Intro");
});

test("undoing something that removes the selected row moves the selection to a row that exists", () => {
  const s = open("Open the gradebook");
  s.setPlayhead(10_000);
  s.splitSelected();
  s.moveSelection("next"); // the second piece, which the split made
  const second = s.selectedId;
  assert.notEqual(second, undefined);
  const result = s.undo();
  assert.equal(result.ok, true);
  assert.match(result.message, /^Undid: split clip\. Selected "/);
  assert.notEqual(s.selectedId, second);
  assert.ok(s.selectedRow, "the selection is on a row that exists");
});

test("the playhead is kept inside the edit when undo makes it shorter", () => {
  const s = new EditSession(solo()); // one clip, 10 seconds
  assert.match(s.setLengthText("20").message, /is now 20 seconds long/);
  s.stepPlayhead("End");
  assert.equal(s.playheadMs, 20_000);
  const result = s.undo();
  assert.equal(result.message, "Undid: set clip length. The playhead moved back to the end, 10 seconds.");
  assert.equal(s.playheadMs, 10_000);
  s.redo();
  assert.equal(s.lengthMs, 20_000);
});

// ---- telling the screen ----

test("the screen is told what kind of thing changed, and only when something did", () => {
  const s = open("Open the gradebook");
  const heard: ChangeKind[] = [];
  const stop = s.subscribe((kind) => heard.push(kind));
  s.moveSelection("next");
  s.moveSelection("next");
  s.moveSelection("last");
  s.moveSelection("last"); // already there
  s.stepPlayhead("Right");
  s.stepPlayhead("Home");
  s.stepPlayhead("Home"); // already there
  assert.deepEqual(heard, ["selection", "selection", "selection", "playhead", "playhead"]);
  heard.length = 0;
  s.splitSelected(); // the playhead is at 0 and the zoom is not a clip: refused
  assert.deepEqual(heard, []);
  select(s, "Add an assignment");
  heard.length = 0;
  s.setPlayhead(45_000);
  s.splitSelected();
  s.rename("Add an assignment"); // the same name: nothing changed
  s.undo();
  assert.deepEqual(heard, ["playhead", "model", "model"]);
  stop();
  s.redo();
  assert.deepEqual(heard, ["playhead", "model", "model"], "nothing is said after the listener is removed");
});

// ---- readouts on request ----

test("what is at the playhead is described on request, and the selected row too", () => {
  const s = open("Open the gradebook");
  s.setPlayhead(13_000);
  const here = s.describePlayhead();
  assert.equal(here.ok, true);
  assert.match(here.message, /^At 13 seconds\. Screen track: Open the gradebook, 13 seconds in\./);
  assert.match(here.message, /Overlays showing: text "Open Settings"\./);
  assert.equal(s.describeSelected().message, s.describeRow(s.selectedId ?? ""));
});

// ---- long recordings ----

test("an hour-long recording cut into 720 clips stays quick", () => {
  const SCREEN = { kind: "take", take: 1, media: "screen.mp4" } as const;
  const MIC = { kind: "take", take: 1, media: "mic.wav" } as const;
  let model = createModel([
    { ref: SCREEN, kind: "screen", name: "Screen", durationMs: 3_600_000 },
    { ref: MIC, kind: "microphone", name: "Microphone", durationMs: 3_600_000 },
  ]);
  const screen = addTrack(model, "screen");
  const voice = addTrack(screen.model, "audio");
  model = voice.model;
  for (const [track, source] of [
    [screen.focus?.[0] ?? "", SCREEN],
    [voice.focus?.[0] ?? "", MIC],
  ] as const) {
    const added = addClip(model, track, { source, startMs: 0 });
    assert.ok(added.ok);
    model = added.model;
  }
  for (let i = 1; i < 360; i++) {
    const cut = splitAt(model, i * 10_000);
    assert.ok(cut.ok);
    model = cut.model;
  }
  const started = performance.now();
  const s = new EditSession(model);
  assert.equal(s.rows.length, 720);
  for (const r of s.rows) s.describeRow(r.clipId);
  const middle = s.rows[200];
  assert.ok(middle);
  s.select(middle.clipId);
  s.setPlayhead(middle.startMs + 5000);
  assert.equal(s.splitSelected().ok, true);
  assert.equal(s.deleteSelected(true).ok, true);
  assert.equal(s.undo().ok, true);
  assert.equal(s.undo().ok, true);
  for (const r of s.rows) s.describeRow(r.clipId);
  const took = performance.now() - started;
  assert.ok(took < 1000, `took ${Math.round(took)} ms, which is not under a second`);
});
