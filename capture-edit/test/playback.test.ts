// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  FULL,
  addArrow,
  addText,
  addTrack,
  addZoom,
  editFromTakes,
  gainAt,
  momentAt,
  setFades,
  setMuted,
  setVolume,
  smoothstep,
  splitAt,
  trimStart,
} from "../src/index.ts";
import { ok, sample } from "./helpers.ts";

test("at a moment, the picture and the sound that play are named, with where in each recording they are", () => {
  const s = sample();
  const cut = ok(trimStart(s.model, s.screen, 10_000)).model; // screen shows 10 to 60 of the recording, from 10 seconds in
  const m = momentAt(cut, 20_000);
  assert.deepEqual(m.pictures.map((p) => [p.name, p.sourceMs, p.kind]), [["Lesson", 20_000, "screen"]]);
  assert.deepEqual(m.sounds.map((p) => [p.name, p.sourceMs, p.kind, p.gain]), [["Voice", 20_000, "microphone", 1]]);
  assert.deepEqual(m.zoom, { rect: { x: 0, y: 0, width: FULL, height: FULL }, moving: false });
  assert.deepEqual(m.overlays, []);
});

test("a clip that was cut from the front plays from the right place in its recording", () => {
  const s = sample();
  const cut = ok(trimStart(s.model, s.screen, 10_000)).model;
  assert.equal(momentAt(cut, 5000).pictures.length, 0, "nothing shows before the clip starts");
  const first = momentAt(cut, 10_000).pictures[0];
  assert.equal(first?.sourceMs, 10_000, "the first moment is 10 seconds into the recording");
  const later = ok(splitAt(cut, 30_000, s.screenTrack)).model;
  assert.equal(momentAt(later, 30_000).pictures[0]?.sourceMs, 30_000);
  assert.equal(momentAt(later, 59_999).pictures[0]?.sourceMs, 59_999);
  assert.equal(momentAt(later, 60_000).pictures.length, 0, "a clip is not playing at the moment it ends");
});

test("the exporter's rules: a screen clip gives no sound, a sound clip gives no picture", () => {
  const s = sample();
  const m = momentAt(s.model, 1000);
  assert.deepEqual(m.pictures.map((p) => p.kind), ["screen"]);
  assert.deepEqual(m.sounds.map((p) => p.kind), ["microphone"]);
});

test("volume, mute and fades set how loud a sound is, in straight lines", () => {
  const s = sample();
  let m = ok(setVolume(s.model, s.mic, -6)).model;
  const clip = m.tracks.find((t) => t.id === s.audioTrack)?.clips[0];
  assert.ok(clip);
  assert.ok(Math.abs(gainAt(clip, 30_000) - 10 ** (-6 / 20)) < 1e-9);
  m = ok(setFades(s.model, s.mic, 2000, 4000)).model;
  const faded = m.tracks.find((t) => t.id === s.audioTrack)?.clips[0];
  assert.ok(faded);
  assert.equal(gainAt(faded, 0), 0);
  assert.equal(gainAt(faded, 1000), 0.5);
  assert.equal(gainAt(faded, 2000), 1);
  assert.equal(gainAt(faded, 58_000), 0.5);
  assert.equal(gainAt(faded, 59_000), 0.25);
  assert.equal(momentAt(m, 0).sounds.length, 0, "silent sounds are left out");
  assert.equal(momentAt(m, 1000).sounds[0]?.gain, 0.5);
  const muted = ok(setMuted(s.model, s.mic, true)).model;
  assert.equal(momentAt(muted, 1000).sounds.length, 0);
});

test("the overlays showing are the ones whose time it is, in the order they are drawn", () => {
  const s = sample();
  const track = ok(addTrack(s.model, "overlay"));
  const t = track.focus?.[0] ?? "";
  const one = ok(addText(track.model, t, { startMs: 5000, durationMs: 5000, text: "First" }));
  const two = ok(addArrow(one.model, t, { startMs: 8000, durationMs: 4000, from: { x: 1000, y: 1000 }, to: { x: 4000, y: 4000 }, altText: "points" }));
  assert.deepEqual(momentAt(two.model, 4999).overlays, []);
  assert.deepEqual(momentAt(two.model, 6000).overlays.map((o) => o.kind), ["text"]);
  assert.deepEqual(momentAt(two.model, 9000).overlays.map((o) => o.kind), ["text", "arrow"]);
  assert.deepEqual(momentAt(two.model, 10_000).overlays.map((o) => o.kind), ["arrow"]);
});

test("a zoom moves in along the exporter's curve, holds, and moves out; with reduced motion it cuts", () => {
  const s = sample();
  const track = ok(addTrack(s.model, "zoom"));
  const rect = { x: 2500, y: 2500, width: 5000, height: 5000 };
  const m = ok(addZoom(track.model, track.focus?.[0] ?? "", { startMs: 10_000, durationMs: 6000, rect, description: "part", transitionMs: 1000 })).model;
  const view = (ms: number, reduceMotion = false) => momentAt(m, ms, { reduceMotion }).zoom;
  assert.deepEqual(view(9999), { rect: { x: 0, y: 0, width: FULL, height: FULL }, moving: false });
  assert.deepEqual(view(10_000), { rect: { x: 0, y: 0, width: FULL, height: FULL }, moving: true });
  const half = smoothstep(0.5);
  assert.equal(half, 0.5);
  assert.deepEqual(view(10_500), { rect: { x: 1250, y: 1250, width: 7500, height: 7500 }, moving: true });
  assert.deepEqual(view(11_000), { rect, moving: false });
  assert.deepEqual(view(14_999), { rect, moving: false });
  assert.equal(view(15_500).moving, true);
  assert.deepEqual(view(16_000), { rect: { x: 0, y: 0, width: FULL, height: FULL }, moving: false });
  assert.deepEqual(view(10_100, true), { rect, moving: false }, "reduced motion cuts straight in");
  assert.ok(Math.abs(smoothstep(0.25) - 0.15625) < 1e-12);
  assert.equal(smoothstep(-1), 0);
  assert.equal(smoothstep(2), 1);
});

test("the screen and the webcam show together, in track order, and the two sounds play together", () => {
  const { model } = editFromTakes([
    {
      take: 1,
      state: "complete",
      durationMs: 10_000,
      media: [
        { file: "screen.mp4", kind: "screen" },
        { file: "webcam.mp4", kind: "webcam" },
        { file: "mic.wav", kind: "microphone" },
        { file: "system.wav", kind: "system-audio" },
      ],
    },
  ]);
  const m = momentAt(model, 4000);
  assert.deepEqual(m.pictures.map((p) => [p.name, p.sourceMs]), [["Screen of take 1", 4000], ["Webcam of take 1", 4000]]);
  assert.deepEqual(m.sounds.map((p) => p.name), ["Microphone of take 1", "Computer sound of take 1"]);
});
