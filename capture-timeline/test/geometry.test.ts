// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  MAX_PX_PER_MS,
  contentMs,
  fitScale,
  msAtX,
  navigate,
  tickStepMs,
  ticksBetween,
  tidy,
  visibleSpanMs,
  zoomed,
} from "../src/geometry.ts";
import { open } from "./helpers.ts";

test("the timeline has room for the edit and a little more, and never less than ten seconds", () => {
  assert.equal(contentMs(0), 10_000);
  assert.equal(contentMs(5000), 10_000);
  assert.equal(contentMs(90_000), 94_500);
  assert.equal(contentMs(3_600_000), 3_780_000);
});

test("zooming goes in and out by half or double and stops at the whole edit and at the most zoomed in", () => {
  const fit = fitScale(940, 94_500);
  assert.ok(Math.abs(fit - 940 / 94_500) < 1e-12);
  assert.equal(zoomed(fit, fit, "out"), fit, "already showing the whole edit");
  const once = zoomed(fit, fit, "in");
  assert.ok(Math.abs(once - fit * 2) < 1e-12);
  let scale = fit;
  for (let i = 0; i < 20; i++) scale = zoomed(scale, fit, "in");
  assert.equal(scale, MAX_PX_PER_MS);
  assert.equal(zoomed(scale, fit, "in"), MAX_PX_PER_MS);
  for (let i = 0; i < 20; i++) scale = zoomed(scale, fit, "out");
  assert.equal(scale, fit);
  // a very short edit would fit at more than the most zoomed in: that is its fit and its most
  const tiny = fitScale(940, 1000);
  assert.ok(tiny > MAX_PX_PER_MS);
  assert.equal(zoomed(tiny, tiny, "in"), tiny);
});

test("how much time fits across the screen", () => {
  assert.equal(visibleSpanMs(1000, 0.1), 10_000);
  assert.equal(visibleSpanMs(940, 940 / 94_500), 94_500);
});

test("the ruler marks are at least 90 pixels apart, at tidy steps", () => {
  assert.equal(tickStepMs(0.4), 500, "400 pixels a second: every half second");
  assert.equal(tickStepMs(0.1), 1000);
  assert.equal(tickStepMs(0.01), 10_000);
  assert.equal(tickStepMs(0.001), 120_000);
  assert.equal(tickStepMs(0.0000001), 3_600_000);
  assert.equal(tickStepMs(0.01, 40), 5000);
  for (const scale of [0.4, 0.3, 0.1, 0.05, 0.01, 0.003, 0.001]) assert.ok(tickStepMs(scale) * scale >= 90 || tickStepMs(scale) === 3_600_000);
});

test("only the marks in view are made, from a step boundary at or before the left edge", () => {
  assert.deepEqual(ticksBetween(2300, 6100, 1000, 90_000), [2000, 3000, 4000, 5000, 6000]);
  assert.deepEqual(ticksBetween(0, 2500, 1000, 90_000), [0, 1000, 2000]);
  assert.deepEqual(ticksBetween(88_500, 95_000, 1000, 90_000), [88_000, 89_000, 90_000], "none past the end of the timeline");
  assert.deepEqual(ticksBetween(-500, 1500, 500, 90_000), [0, 500, 1000, 1500]);
});

test("the time under the pointer, from where time 0 is on the screen", () => {
  assert.equal(msAtX(300, 100, 0.1), 2000);
  assert.equal(msAtX(50, 100, 0.1), 0, "left of the start is the start");
  assert.equal(msAtX(100.04, 100, 0.1), 0);
  assert.equal(tidy(1234), 1230);
  assert.equal(tidy(1235), 1240);
});

test("Left and Right go to the clip before and after in the same track, and stop at the ends", () => {
  const s = open();
  const tracks = s.model.tracks.map((t) => t.id);
  const id = (name: string) => s.rows.find((r) => r.name === name)?.clipId ?? "";
  assert.equal(navigate(s.rows, tracks, id("Add an assignment"), "right")?.name, "Check the totals");
  assert.equal(navigate(s.rows, tracks, id("Add an assignment"), "left")?.name, "Open the gradebook");
  assert.equal(navigate(s.rows, tracks, id("Open the gradebook"), "left"), undefined);
  assert.equal(navigate(s.rows, tracks, id("Check the totals"), "right"), undefined);
  assert.equal(navigate(s.rows, tracks, id("Add an assignment"), "first")?.name, "Open the gradebook");
  assert.equal(navigate(s.rows, tracks, id("Add an assignment"), "last")?.name, "Check the totals");
  assert.equal(navigate(s.rows, tracks, id("Open the gradebook"), "first"), undefined);
  assert.equal(navigate(s.rows, tracks, id("Check the totals"), "last"), undefined);
});

test("Up and Down go to the clip showing at the same time in the next track that has anything", () => {
  const s = open();
  const tracks = s.model.tracks.map((t) => t.id); // Screen, Voice, Webcam, Overlays, Zoom
  const id = (name: string) => s.rows.find((r) => r.name === name)?.clipId ?? "";
  assert.equal(navigate(s.rows, tracks, id("Add an assignment"), "down")?.name, "Voice, part 2");
  assert.equal(navigate(s.rows, tracks, id("Voice, part 2"), "down")?.name, "Presenter, part 2");
  assert.equal(navigate(s.rows, tracks, id("Voice, part 2"), "up")?.name, "Add an assignment");
  assert.equal(navigate(s.rows, tracks, id("Open the gradebook"), "up"), undefined);
  // the overlays: nothing is showing at 30 seconds, so the nearest in time is chosen (the arrow at 36 s is nearer than the title at 12 s)
  assert.equal(navigate(s.rows, tracks, id("Presenter, part 2"), "down")?.name, "Arrow to the Add button");
  assert.equal(navigate(s.rows, tracks, id("Arrow to the Add button"), "down")?.name, "Zoom on the totals");
  assert.equal(navigate(s.rows, tracks, id("Zoom on the totals"), "down"), undefined);
});

test("an empty track is skipped, and a row that is not there goes to the first", () => {
  const s = open();
  s.addTrack("media");
  const tracks = s.model.tracks.map((t) => t.id);
  const id = (name: string) => s.rows.find((r) => r.name === name)?.clipId ?? "";
  assert.equal(navigate(s.rows, tracks, id("Zoom on the totals"), "up")?.name, "Arrow to the Add button");
  assert.equal(navigate(s.rows, tracks, "nope", "down")?.name, "Open the gradebook");
});
