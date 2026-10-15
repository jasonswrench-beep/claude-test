// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  History,
  addClip,
  addText,
  addTrack,
  addZoom,
  moveClipToTrack,
  moveItemToTrack,
  splitAt,
  type Model,
} from "../src/index.ts";
import { MIC, ok, refused, sample } from "./helpers.ts";

function withSecondAudio(): { model: Model; second: string; s: ReturnType<typeof sample> } {
  const s = sample();
  const track = ok(addTrack(s.model, "audio"));
  return { model: track.model, second: track.focus?.[0] ?? "", s };
}

test("a clip moves to another track that takes it, keeping its time, and one undo puts it back", () => {
  const { model, second, s } = withSecondAudio();
  const h = new History(model);
  const result = h.run((m) => moveClipToTrack(m, s.mic, second));
  assert.equal(result.ok && result.message, 'Clip "Voice" moved from the Audio track to the Audio 2 track, still starting at 0 seconds.');
  assert.equal(h.model.tracks.find((t) => t.id === s.audioTrack)?.clips.length, 0);
  assert.equal(h.model.tracks.find((t) => t.id === second)?.clips[0]?.startMs, 0);
  assert.equal(h.undoLabel, "move clip to another track");
  h.undo();
  assert.equal(h.model, model, "undo gives back exactly the edit before");
});

test("a clip cannot go to a track that does not take its recording, and the reason is in words", () => {
  const s = sample();
  assert.equal(
    refused(moveClipToTrack(s.model, s.screen, s.audioTrack)),
    'Cannot move "Lesson" to the Audio track: that track takes microphone recordings, computer sound and sound files.',
  );
  assert.equal(
    refused(moveClipToTrack(s.model, s.mic, s.screenTrack)),
    'Cannot move "Voice" to the Screen track: that track takes screen recordings.',
  );
  const overlay = ok(addTrack(s.model, "overlay"));
  assert.equal(
    refused(moveClipToTrack(overlay.model, s.mic, overlay.focus?.[0] ?? "")),
    'Cannot move "Voice" to the Overlays track: that track holds overlays and zooms, not clips.',
  );
  assert.match(refused(moveClipToTrack(s.model, s.mic, "nope")), /There is no track with the id nope\./);
  assert.match(refused(moveClipToTrack(s.model, "nope", s.audioTrack)), /There is no clip with the id nope\./);
});

test("a clip cannot be moved onto a clip that is already there", () => {
  const { model, second, s } = withSecondAudio();
  const there = ok(addClip(model, second, { source: MIC, startMs: 20_000, lengthMs: 10_000, name: "Other" })).model;
  const cut = ok(splitAt(there, 30_000, s.audioTrack)).model; // Voice is now 0 to 30 and 30 to 60 on the first audio track
  const first = cut.tracks.find((t) => t.id === s.audioTrack)?.clips[0]?.id ?? "";
  assert.equal(
    refused(moveClipToTrack(cut, first, second)),
    'Cannot move "Voice" to the Audio 2 track: it would overlap the clip "Other", which runs from 20 seconds to 30 seconds there.',
  );
  const next = cut.tracks.find((t) => t.id === s.audioTrack)?.clips[1]?.id ?? "";
  assert.equal(moveClipToTrack(cut, next, second).ok, true, "the second piece, from 30 seconds, fits");
});

test("moving a clip to the track it is on changes nothing", () => {
  const s = sample();
  const result = moveClipToTrack(s.model, s.mic, s.audioTrack);
  assert.equal(result.ok && result.model, s.model);
  assert.equal(result.message, 'The clip "Voice" is already on the Audio track.');
});

test("an overlay moves to another overlay track, and a zoom only to a zoom track", () => {
  const s = sample();
  const a = ok(addTrack(s.model, "overlay"));
  const b = ok(addTrack(a.model, "overlay"));
  const z = ok(addTrack(b.model, "zoom"));
  const text = ok(addText(z.model, a.focus?.[0] ?? "", { startMs: 5000, durationMs: 3000, text: "Hi", name: "Greeting" }));
  const id = text.focus?.[0] ?? "";
  const moved = ok(moveItemToTrack(text.model, id, b.focus?.[0] ?? ""));
  assert.equal(moved.message, 'Text overlay "Greeting" moved from the Overlays track to the Overlays 2 track, still starting at 5 seconds.');
  assert.equal(moved.model.tracks.find((t) => t.id === b.focus?.[0])?.items.length, 1);
  assert.equal(moved.model.tracks.find((t) => t.id === a.focus?.[0])?.items.length, 0);
  assert.match(refused(moveItemToTrack(text.model, id, z.focus?.[0] ?? "")), /^Cannot move "Greeting" to the Zoom track: that track takes zoom\./);
  assert.match(refused(moveItemToTrack(text.model, id, s.screenTrack)), /that track holds clips, not overlays or zooms\./);
  const rect = { x: 2000, y: 2000, width: 5000, height: 5000 };
  const zoom = ok(addZoom(z.model, z.focus?.[0] ?? "", { startMs: 1000, durationMs: 3000, rect, description: "part", name: "Close up" }));
  assert.match(refused(moveItemToTrack(zoom.model, zoom.focus?.[0] ?? "", a.focus?.[0] ?? "")), /^Cannot move "Close up" to the Overlays track: that track takes text, shape, arrow, blur, redact\./);
  assert.equal(moveItemToTrack(text.model, id, a.focus?.[0] ?? "").ok && moveItemToTrack(text.model, id, a.focus?.[0] ?? "").message, 'Text overlay "Greeting" is already on the Overlays track.');
});

test("a zoom cannot be moved into a stretch of another zoom track that already has a zoom", () => {
  const s = sample();
  const a = ok(addTrack(s.model, "zoom"));
  const b = ok(addTrack(a.model, "zoom"));
  const rect = { x: 2000, y: 2000, width: 5000, height: 5000 };
  const one = ok(addZoom(b.model, a.focus?.[0] ?? "", { startMs: 10_000, durationMs: 5000, rect, description: "part", name: "First" }));
  const two = ok(addZoom(one.model, b.focus?.[0] ?? "", { startMs: 12_000, durationMs: 5000, rect, description: "part", name: "Second" }));
  const result = moveItemToTrack(two.model, two.focus?.[0] ?? "", a.focus?.[0] ?? "");
  assert.equal(result.ok, false);
});
