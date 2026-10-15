// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addClip,
  isSound,
  parse,
  serialize,
  setFades,
  setImageText,
  splitAt,
  EDIT_SCHEMA_VERSION,
  setPlacement,
  type Model,
} from "../src/index.ts";
import { LOGO, MUSIC, ok, sample } from "./helpers.ts";

function busy(): Model {
  const s = sample();
  let m = ok(splitAt(s.model, 20_000)).model;
  m = ok(setFades(m, s.mic, 500, 0)).model;
  const logo = ok(addClip(m, s.mediaTrack, { source: LOGO, startMs: 3000, lengthMs: 4000 }));
  m = ok(setImageText(logo.model, logo.focus?.[0] ?? "", { altText: 'A "quoted" logo, with a comma' })).model;
  m = ok(addClip(m, s.mediaTrack, { source: MUSIC, startMs: 10_000, lengthMs: 30_000 })).model;
  return m;
}

test("an edit saved and read back is the same edit, and saving again gives the same bytes", () => {
  const model = busy();
  const text = serialize(model);
  const back = parse(text);
  assert.ok(back.ok);
  assert.deepEqual(back.model, model);
  assert.equal(serialize(back.model), text);
  assert.ok(text.endsWith("}\n"));
});

test("the saved text lists things in one fixed order and leaves out what is not set", () => {
  const s = sample();
  const text = serialize(s.model);
  const first = text.indexOf('"format"');
  assert.ok(first >= 0 && first < text.indexOf('"schemaVersion"') && text.indexOf('"schemaVersion"') < text.indexOf('"nextId"'));
  assert.ok(!text.includes("altText") && !text.includes("decorative"));
  assert.ok(text.includes(`"schemaVersion": "${EDIT_SCHEMA_VERSION}"`));
});

test("an edit with only needs-attention problems is still sound", () => {
  const s = sample();
  const logo = ok(addClip(s.model, s.mediaTrack, { source: LOGO, startMs: 0, lengthMs: 3000 }));
  const back = parse(serialize(logo.model));
  assert.ok(back.ok);
  assert.equal(back.problems.length, 1);
  assert.equal(back.problems[0]?.severity, "needs-attention");
  assert.ok(isSound(back));
});

test("a file that is not an edit, or is a version this program does not read, is refused in words", () => {
  const cases: Array<[string, RegExp]> = [
    ["not json at all", /not a readable edit file/],
    ["[]", /not an OpenStage Capture edit file/],
    ['{"format":"something-else"}', /not an OpenStage Capture edit file/],
    ['{"format":"openstage-capture-edit","schemaVersion":"0.4.0"}', /version 0.4.0, and this program reads versions 0.1, 0.2 and 0.3/],
    ['{"format":"openstage-capture-edit"}', /version undefined/],
    ['{"format":"openstage-capture-edit","schemaVersion":"0.1.0","nextId":1,"sources":[],"tracks":[{"id":"t","kind":"hologram","name":"x","clips":[]}]}', /does not know: hologram/],
    ['{"format":"openstage-capture-edit","schemaVersion":"0.1.0","nextId":"x","sources":[],"tracks":[]}', /nextId is missing or is not a number/],
    ['{"format":"openstage-capture-edit","schemaVersion":"0.1.0","nextId":1,"sources":[],"tracks":[{"id":"t","kind":"audio","name":"x","clips":[{"id":"c"}]}]}', /The clip c: its name is missing/],
  ];
  for (const [text, pattern] of cases) {
    const r = parse(text);
    assert.equal(r.ok, false, text);
    if (!r.ok) assert.match(r.message, pattern, text);
  }
});

test("a file with mistakes in it reads, and brings its problems with it", () => {
  const s = sample();
  const text = serialize(s.model).replace('"startMs": 0,\n          "muted": false', '"startMs": -5,\n          "muted": false');
  const r = parse(text);
  assert.ok(r.ok);
  assert.ok(r.problems.some((p) => p.code === "bad-time"));
  assert.equal(isSound(r), false);
});

test("a picture's place is saved after its fades, is read back, and an older file without one reads as before", () => {
  const s = sample();
  const placed = ok(setPlacement(s.model, s.screen, { x: 7500, y: 7000, width: 2200, height: 2200 }));
  const text = serialize(placed.model);
  assert.match(text, /"fadeOutMs": 0,\n\s+"placement": \{\n\s+"x": 7500,/);
  const back = parse(text);
  assert.ok(back.ok);
  assert.deepEqual(back.model, placed.model);
  assert.equal(serialize(back.model), text, "saving again gives the same bytes");
  const without = parse(serialize(s.model));
  assert.ok(without.ok);
  assert.ok(without.model.tracks.every((t) => t.clips.every((c) => c.placement === undefined)));
  assert.doesNotMatch(serialize(s.model), /placement/);
});
