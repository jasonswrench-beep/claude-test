// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  History,
  addArrow,
  addBlur,
  addClip,
  addRedact,
  addShape,
  addText,
  addTrack,
  addZoom,
  contrastRatio,
  deleteItem,
  describeAt,
  describeClip,
  describeMagnification,
  describePlace,
  findItem,
  isLargeText,
  listCaption,
  listRows,
  moveItem,
  normalizeColor,
  parse,
  rectFromPixels,
  rectToPixels,
  renameItem,
  serialize,
  setArrowEnds,
  setItemAltText,
  setItemRect,
  setShapeStyle,
  setText,
  setTextStyle,
  setZoom,
  trimItemEnd,
  trimItemStart,
  validate,
  type Item,
  type Model,
  type Rect,
} from "../src/index.ts";
import { LOGO, ok, refused, sample, SCREEN } from "./helpers.ts";

interface WithOverlays {
  readonly model: Model;
  readonly overlays: string;
  readonly zooms: string;
  readonly screenTrack: string;
}

function withTracks(): WithOverlays {
  const s = sample();
  const o = addTrack(s.model, "overlay");
  const z = addTrack(o.model, "zoom");
  return { model: z.model, overlays: o.focus?.[0] ?? "", zooms: z.focus?.[0] ?? "", screenTrack: s.screenTrack };
}

const HALF: Rect = { x: 2500, y: 2500, width: 5000, height: 5000 };

function itemOf(model: Model, id: string): Item {
  const found = findItem(model, id);
  assert.ok(found, `no item ${id}`);
  return found.item;
}

// ---- colour and contrast ----

test("contrast is measured the way the web content guidelines measure it", () => {
  assert.equal(Math.round(contrastRatio("#000000", "#FFFFFF") * 10) / 10, 21);
  assert.equal(contrastRatio("#336699", "#336699"), 1);
  // 4.5 to 1 is the line for ordinary text. #767676 on white just passes; #777777 just fails.
  assert.ok(contrastRatio("#767676", "#FFFFFF") >= 4.5);
  assert.ok(contrastRatio("#777777", "#FFFFFF") < 4.5);
  assert.equal(contrastRatio("#123456", "#FEDCBA"), contrastRatio("#FEDCBA", "#123456"), "order does not matter");
});

test("large text needs less contrast than small text, and bold counts as large sooner", () => {
  assert.equal(isLargeText(221, false), false);
  assert.equal(isLargeText(222, false), true);
  assert.equal(isLargeText(172, true), false);
  assert.equal(isLargeText(173, true), true);
});

test("colours are written one way", () => {
  assert.equal(normalizeColor("ffd400"), "#FFD400");
  assert.equal(normalizeColor(" #ffd400 "), "#FFD400");
  assert.equal(normalizeColor("yellow"), undefined);
  assert.equal(normalizeColor("#FFF"), undefined);
});

// ---- text ----

test("text goes on the overlay track with a readable look by default", () => {
  const t = withTracks();
  const r = ok(addText(t.model, t.overlays, { startMs: 5000, durationMs: 3000, text: "Welcome to the course" }));
  assert.equal(r.message, 'Added the text overlay "Welcome to the course" to the Overlays track at 5 seconds, 3 seconds long.');
  const item = itemOf(r.model, r.focus?.[0] ?? "");
  assert.equal(item.kind, "text");
  if (item.kind === "text") {
    assert.deepEqual([item.color, item.background, item.fontSizeBp, item.bold, item.align], ["#FFFFFF", "#000000", 400, false, "left"]);
  }
  assert.deepEqual(validate(r.model).filter((p) => p.severity !== "warning"), []);
});

test("text that is hard to read, or too small, is added with a warning in the sentence", () => {
  const t = withTracks();
  const low = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "Hello", fontSizeBp: 200, color: "#777777", background: "#808080" }));
  assert.match(low.message, /Warning: The text overlay "Hello" on the Overlays track is hard to read: .* contrast of 1\.\d to 1, and 4\.5 to 1 is needed/);
  const small = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "Tiny", fontSizeBp: 120 }));
  assert.match(small.message, /Warning: .* is very small/);
  const bigLow = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "Big", fontSizeBp: 300, color: "#767676", background: "#FFFFFF" }));
  assert.doesNotMatch(bigLow.message, /Warning/, "4.5 to 1 passes");
  const large = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "Large", fontSizeBp: 300, color: "#999999", background: "#FFFFFF" }));
  assert.match(large.message, /and 3\.0 to 1 is needed|contrast of 2\.\d to 1, and 3\.0 to 1/, "large text is judged against 3 to 1");
  const none = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "No background", color: "#999999", background: null }));
  assert.doesNotMatch(none.message, /Warning/, "without a background of its own there is nothing to compare");
});

test("text with nothing to say, a bad colour or a rectangle off the picture is refused", () => {
  const t = withTracks();
  assert.match(refused(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "   " })), /has no text/);
  assert.match(refused(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "x", color: "red" })), /colour that is not written like #FFFFFF/);
  assert.match(refused(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "x", rect: { x: 9000, y: 0, width: 2000, height: 500 } })), /not a rectangle inside the picture/);
  assert.match(refused(addText(t.model, t.overlays, { startMs: 0, durationMs: 50, text: "x" })), /at least 0.1 seconds/);
  assert.match(refused(addText(t.model, "track-404", { startMs: 0, durationMs: 2000, text: "x" })), /no track with the id track-404/);
});

test("overlays may overlap each other in time, but only the right kind goes on each track", () => {
  const t = withTracks();
  const a = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 5000, text: "One" }));
  const b = ok(addText(a.model, t.overlays, { startMs: 2000, durationMs: 5000, text: "Two", rect: { x: 500, y: 500, width: 4000, height: 1000 } }));
  assert.equal(findItem(b.model, a.focus?.[0] ?? "")?.track.items.length, 2);
  assert.match(refused(addText(b.model, t.zooms, { startMs: 0, durationMs: 2000, text: "x" })), /cannot go on the Zoom track\. That track takes zoom/);
  assert.match(refused(addZoom(b.model, t.overlays, { startMs: 0, durationMs: 2000, rect: HALF })), /cannot go on the Overlays track/);
  assert.match(refused(addText(b.model, t.screenTrack, { startMs: 0, durationMs: 2000, text: "x" })), /holds clips, not overlays or zooms/);
  assert.match(refused(addClip(b.model, t.overlays, { source: SCREEN, startMs: 0 })), /cannot go on the Overlays track/);
});

// ---- shapes, arrows, blur, redaction ----

test("a shape or arrow asks for alt text or to be marked decorative", () => {
  const t = withTracks();
  const shape = ok(addShape(t.model, t.overlays, { startMs: 0, durationMs: 3000 }));
  assert.match(shape.message, /Still needed: .* has no alt text\. Say what it points out, or mark it as decorative\./);
  const id = shape.focus?.[0] ?? "";
  const described = ok(setItemAltText(shape.model, id, { altText: "The Save button" }));
  assert.equal(described.message, 'Shape "Rectangle" now has alt text: The Save button');
  assert.equal(validate(described.model).filter((p) => p.itemId === id).length, 0);
  const deco = ok(setItemAltText(shape.model, id, { decorative: true }));
  assert.equal(deco.message, 'Shape "Rectangle" is marked as decorative.');

  const arrow = ok(addArrow(t.model, t.overlays, { startMs: 0, durationMs: 3000, from: { x: 1000, y: 1000 }, to: { x: 4000, y: 4000 }, decorative: true }));
  assert.doesNotMatch(arrow.message, /Still needed/);
  assert.match(refused(addArrow(t.model, t.overlays, { startMs: 0, durationMs: 3000, from: { x: -1, y: 0 }, to: { x: 4000, y: 4000 } })), /starts or ends outside the picture/);
  const moved = ok(setArrowEnds(arrow.model, arrow.focus?.[0] ?? "", { x: 2000, y: 2000 }, { x: 8000, y: 8000 }));
  assert.match(moved.message, /now points from the top left of the picture to the bottom right\./);
  assert.match(refused(setArrowEnds(shape.model, id, { x: 0, y: 0 }, { x: 1, y: 1 })), /is not an arrow/);
  assert.match(refused(setItemRect(arrow.model, arrow.focus?.[0] ?? "", HALF)), /An arrow has two ends/);
});

test("a shape can be restyled and given a bad colour is refused", () => {
  const t = withTracks();
  const shape = ok(addShape(t.model, t.overlays, { startMs: 0, durationMs: 3000, decorative: true }));
  const id = shape.focus?.[0] ?? "";
  const style = ok(setShapeStyle(shape.model, id, { shape: "ellipse", outline: "#ff0000", fill: null, lineWidthBp: 60 }));
  const item = itemOf(style.model, id);
  assert.equal(item.kind === "shape" && item.shape, "ellipse");
  assert.equal(item.kind === "shape" && item.outline, "#FF0000");
  assert.match(refused(setShapeStyle(shape.model, id, { outline: "blue" })), /not a colour/);
  assert.match(refused(setShapeStyle(shape.model, id, { lineWidthBp: 9000 })), /line thickness/);
});

test("blur and redaction say that the viewer cannot undo them", () => {
  const t = withTracks();
  const blur = ok(addBlur(t.model, t.overlays, { startMs: 1000, durationMs: 4000, rect: HALF }));
  assert.match(blur.message, /Added the blur "Blur" .* Once the video is exported, nobody can undo this blur\.$/);
  const redact = ok(addRedact(t.model, t.overlays, { startMs: 1000, durationMs: 4000, rect: HALF, color: "#222222" }));
  assert.match(redact.message, /Added the redaction "Redaction" .* nobody can undo this redaction\.$/);
  assert.deepEqual(validate(redact.model), [], "neither needs alt text");
});

// ---- zoom ----

test("a zoom moves in over half a second by default, and asks what it shows", () => {
  const t = withTracks();
  const r = ok(addZoom(t.model, t.zooms, { startMs: 10_000, durationMs: 4000, rect: HALF }));
  const zoom = itemOf(r.model, r.focus?.[0] ?? "");
  assert.equal(zoom.kind === "zoom" && zoom.transitionMs, 500);
  assert.match(r.message, /Still needed: .* has no description\. Say what it shows/);
  const described = ok(setZoom(r.model, zoom.id, { description: "The settings menu" }));
  assert.equal(validate(described.model).filter((p) => p.itemId === zoom.id).length, 0);
  assert.equal(described.message, 'Zoom "Zoom" moves in and out over 0.5 seconds each.');
});

test("a zoom is described by how much and where", () => {
  assert.equal(describeMagnification(HALF), "2 times");
  assert.equal(describeMagnification({ x: 0, y: 0, width: 6667, height: 6667 }), "1.5 times");
  assert.equal(describeMagnification({ x: 0, y: 0, width: 10_000, height: 10_000 }), "1 time");
  assert.equal(describePlace(HALF), "the middle");
  assert.equal(describePlace({ x: 0, y: 0, width: 5000, height: 5000 }), "the top left");
  assert.equal(describePlace({ x: 5000, y: 6000, width: 5000, height: 4000 }), "the bottom right");
  assert.equal(describePlace({ x: 3500, y: 0, width: 3000, height: 2000 }), "the top");
  assert.equal(describePlace({ x: 0, y: 4000, width: 2000, height: 2000 }), "the left");
});

test("zooms cannot overlap, cannot be too many at once, and must fit their own moves", () => {
  const t = withTracks();
  const a = ok(addZoom(t.model, t.zooms, { startMs: 10_000, durationMs: 4000, rect: HALF }));
  assert.match(refused(addZoom(a.model, t.zooms, { startMs: 12_000, durationMs: 4000, rect: HALF })), /starts at 12 seconds, before the zoom "Zoom" before it ends at 14 seconds/);
  assert.match(refused(addZoom(t.model, t.zooms, { startMs: 0, durationMs: 800, rect: HALF })), /moves in and out for 0.5 seconds each, which does not fit in 0.8 seconds/);
  ok(addZoom(t.model, t.zooms, { startMs: 0, durationMs: 800, rect: HALF, transitionMs: 400 }));
  // Four zooms starting within one second is too many.
  let m = t.model;
  for (let i = 0; i < 3; i++) m = ok(addZoom(m, t.zooms, { startMs: i * 300, durationMs: 200, rect: HALF, transitionMs: 100 })).model;
  assert.match(refused(addZoom(m, t.zooms, { startMs: 900, durationMs: 200, rect: HALF, transitionMs: 100 })), /More than 3 zooms start within one second/);
  ok(addZoom(m, t.zooms, { startMs: 1000, durationMs: 200, rect: HALF, transitionMs: 100 }));
});

test("a zoom that is not the shape of the picture gets a warning about empty bars", () => {
  const t = withTracks();
  const r = ok(addZoom(t.model, t.zooms, { startMs: 0, durationMs: 4000, rect: { x: 1000, y: 1000, width: 4000, height: 2000 } }));
  assert.match(r.message, /Warning: .* not the same shape as the picture/);
});

// ---- changing ----

test("an overlay can be moved, trimmed, renamed, repositioned and deleted, each in words", () => {
  const t = withTracks();
  const added = ok(addText(t.model, t.overlays, { startMs: 5000, durationMs: 3000, text: "Welcome", name: "Title" }));
  const id = added.focus?.[0] ?? "";
  assert.equal(ok(moveItem(added.model, id, 6000)).message, 'Text overlay "Title" now starts at 6 seconds.');
  assert.equal(ok(trimItemStart(added.model, id, 6000)).message, 'Text overlay "Title" now starts at 6 seconds and ends at 8 seconds.');
  assert.equal(ok(trimItemEnd(added.model, id, 10_000)).message, 'Text overlay "Title" now ends at 10 seconds, lasting 5 seconds.');
  assert.match(refused(trimItemStart(added.model, id, 7950)), /later than 7.9 seconds/);
  assert.match(refused(trimItemEnd(added.model, id, 5050)), /earlier than 5.1 seconds/);
  assert.equal(ok(renameItem(added.model, id, "Heading")).message, 'Text overlay "Title" is now called "Heading".');
  assert.equal(ok(setText(added.model, id, "Welcome back")).message, 'Text overlay "Title" now says: Welcome back');
  assert.match(ok(setItemRect(added.model, id, { x: 0, y: 0, width: 5000, height: 1000 })).message, /is now in the top left of the picture/);
  assert.match(refused(setItemRect(added.model, id, { x: 9000, y: 0, width: 5000, height: 1000 })), /not a rectangle inside the picture/);
  assert.equal(ok(deleteItem(added.model, id)).message, 'Text overlay "Title" deleted.');
  assert.equal(findItem(ok(deleteItem(added.model, id)).model, id), undefined);
  assert.equal(ok(moveItem(added.model, id, 5000)).model, added.model, "no change gives back the same edit");
  assert.match(refused(moveItem(added.model, "item-404", 1)), /no overlay or zoom with the id item-404/);
  const shape = ok(addShape(t.model, t.overlays, { startMs: 0, durationMs: 2000, decorative: true }));
  assert.match(refused(setText(shape.model, shape.focus?.[0] ?? "", "x")), /is not a text overlay/);
});

test("changing a text look warns about contrast straight away", () => {
  const t = withTracks();
  const added = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 3000, text: "Hi" }));
  const id = added.focus?.[0] ?? "";
  const bad = ok(setTextStyle(added.model, id, { color: "#888888", background: "#999999" }));
  assert.match(bad.message, /has a new look\. Warning: .* hard to read/);
  const fixed = ok(setTextStyle(bad.model, id, { color: "#000000", background: "#FFFFFF" }));
  assert.doesNotMatch(fixed.message, /Warning/);
  assert.match(refused(setTextStyle(added.model, id, { color: "pink" })), /not a colour/);
  const blur = ok(addBlur(t.model, t.overlays, { startMs: 0, durationMs: 2000, rect: HALF }));
  assert.match(refused(setTextStyle(blur.model, blur.focus?.[0] ?? "", { bold: true })), /is not a text overlay/);
});

test("a change that would make the edit unsound is refused and changes nothing", () => {
  const t = withTracks();
  const a = ok(addZoom(t.model, t.zooms, { startMs: 0, durationMs: 4000, rect: HALF, description: "a" }));
  const b = ok(addZoom(a.model, t.zooms, { startMs: 5000, durationMs: 4000, rect: HALF, description: "b" }));
  assert.match(refused(moveItem(b.model, b.focus?.[0] ?? "", 2000)), /before the zoom/);
  assert.match(refused(trimItemStart(b.model, b.focus?.[0] ?? "", 3000)), /before the zoom/);
});

// ---- the list and the preview ----

test("overlays and zooms are rows in the list, and in what is on screen", () => {
  const t = withTracks();
  let m = ok(addText(t.model, t.overlays, { startMs: 5000, durationMs: 3000, text: "Welcome" })).model;
  m = ok(addZoom(m, t.zooms, { startMs: 6000, durationMs: 4000, rect: { x: 0, y: 0, width: 5000, height: 5000 }, description: "The menu bar" })).model;
  m = ok(addBlur(m, t.overlays, { startMs: 6500, durationMs: 1000, rect: HALF })).model;
  const rows = listRows(m);
  assert.deepEqual(rows.filter((r) => r.trackName !== "Screen" && r.trackName !== "Audio").map((r) => [r.type, r.trackName, r.startMs, r.lengthMs, r.state]), [
    ["Text overlay", "Overlays", 5000, 3000, "ready"],
    ["Blur", "Overlays", 6500, 1000, "ready"],
    ["Zoom", "Zoom", 6000, 4000, "ready"],
  ]);
  assert.equal(listCaption(m), "Tracks and clips: 2 clips, 3 overlays and zooms on 5 tracks, 1 minute long.");
  const at = describeAt(m, 7000);
  assert.match(at, /Overlays showing: text "Welcome"; a blur over the middle of the picture\./);
  assert.match(at, /Zoom: 2 times on the top left of the picture, showing The menu bar\./);
  assert.match(describeAt(m, 1000), /No overlays showing\. No caption showing\. Zoom: none\./);
  const textId = rows.find((r) => r.type === "Text overlay")?.clipId ?? "";
  assert.equal(describeClip(m, textId), 'Welcome, text overlay on the Overlays track, starts at 5 seconds, 3 seconds long, ends at 8 seconds, says: Welcome, in the lower part.'.replace("the lower part", "the bottom of the picture"));
});

test("a row says when an overlay needs something or has a warning", () => {
  const t = withTracks();
  const shape = ok(addShape(t.model, t.overlays, { startMs: 0, durationMs: 2000 }));
  assert.equal(listRows(shape.model).find((r) => r.clipId === shape.focus?.[0])?.state, "needs attention");
  const text = ok(addText(t.model, t.overlays, { startMs: 0, durationMs: 2000, text: "x", fontSizeBp: 100 }));
  assert.equal(listRows(text.model).find((r) => r.clipId === text.focus?.[0])?.state, "has a warning");
});

// ---- saving ----

test("overlays and zooms are saved and read back exactly, and saving again gives the same bytes", () => {
  const t = withTracks();
  let m = ok(addText(t.model, t.overlays, { startMs: 5000, durationMs: 3000, text: 'Say "hello", then go', align: "center", bold: true, background: null })).model;
  m = ok(addShape(m, t.overlays, { startMs: 0, durationMs: 2000, shape: "ellipse", fill: "#00FF00", altText: "The logo" })).model;
  m = ok(addArrow(m, t.overlays, { startMs: 0, durationMs: 2000, from: { x: 1, y: 2 }, to: { x: 3, y: 4 }, decorative: true })).model;
  m = ok(addBlur(m, t.overlays, { startMs: 100, durationMs: 200, rect: HALF })).model;
  m = ok(addRedact(m, t.overlays, { startMs: 100, durationMs: 200, rect: HALF })).model;
  m = ok(addZoom(m, t.zooms, { startMs: 0, durationMs: 3000, rect: HALF, description: "The menu" })).model;
  const text = serialize(m);
  const back = parse(text);
  assert.ok(back.ok);
  assert.deepEqual(back.model, m);
  assert.equal(serialize(back.model), text);
});

test("an edit from version 0.1, which had no overlays or zooms, opens and is brought up to date", () => {
  const old = JSON.parse(serialize(sample().model)) as { schemaVersion: string; tracks: Array<{ items?: unknown }> };
  old.schemaVersion = "0.1.0";
  for (const track of old.tracks) delete track.items;
  const back = parse(JSON.stringify(old));
  assert.ok(back.ok);
  assert.equal(back.model.schemaVersion, "0.2.0");
  assert.ok(back.model.tracks.every((tr) => tr.items.length === 0));
  assert.equal(back.problems.length, 0);
});

test("an item of a kind this program does not know is refused, not half understood", () => {
  const raw = JSON.parse(serialize(withTracks().model)) as { tracks: Array<{ kind: string; items: unknown[] }> };
  const overlay = raw.tracks.find((tr) => tr.kind === "overlay");
  assert.ok(overlay);
  overlay.items.push({ id: "item-9", kind: "hologram", name: "x", startMs: 0, durationMs: 1000 });
  const r = parse(JSON.stringify(raw));
  assert.equal(r.ok, false);
  if (!r.ok) assert.match(r.message, /kind this program does not know: hologram/);
});

// ---- pixels ----

test("numbers typed in pixels become positions that mean the same at any size", () => {
  const canvas = { width: 1920, height: 1080 };
  const rect = rectFromPixels({ x: 480, y: 270, width: 960, height: 540 }, canvas);
  assert.deepEqual(rect, { x: 2500, y: 2500, width: 5000, height: 5000 });
  assert.deepEqual(rectToPixels(rect, canvas), { x: 480, y: 270, width: 960, height: 540 });
  assert.deepEqual(rectToPixels(rect, { width: 1280, height: 720 }), { x: 320, y: 180, width: 640, height: 360 });
});

// ---- undo ----

test("adding and changing overlays can be undone and redone", () => {
  const t = withTracks();
  const history = new History(t.model);
  history.run((m) => addText(m, t.overlays, { startMs: 0, durationMs: 2000, text: "One" }));
  history.run((m) => addZoom(m, t.zooms, { startMs: 0, durationMs: 4000, rect: HALF }));
  const id = history.model.tracks.find((tr) => tr.kind === "overlay")?.items[0]?.id ?? "";
  history.run((m) => moveItem(m, id, 3000));
  assert.deepEqual(history.undo(), { ok: true, message: "Undid: move." });
  assert.deepEqual(history.undo(), { ok: true, message: "Undid: add zoom." });
  assert.deepEqual(history.undo(), { ok: true, message: "Undid: add text overlay." });
  assert.equal(history.model, t.model);
  assert.deepEqual(history.redo(), { ok: true, message: "Redid: add text overlay." });
  assert.ok(LOGO);
});
