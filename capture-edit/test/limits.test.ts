// SPDX-License-Identifier: Apache-2.0
// How far each clip, overlay and zoom can go. The limits come from asking the editing actions, so they must
// agree with them everywhere: the limit is accepted, one step beyond is refused.
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ROOM_PAST_END_MS,
  addClip,
  addText,
  addTrack,
  addZoom,
  clipEndMs,
  createModel,
  deleteClip,
  deleteRange,
  itemEndMs,
  limitsOf,
  moveItem,
  setClipStart,
  splitAt,
  trimEnd,
  trimItemEnd,
  trimItemStart,
  trimStart,
  MIN_CLIP_MS,
  modelLengthMs,
  type Model,
  type Result,
} from "../src/index.ts";
import { MIC, SCREEN, SOURCES, ok, sample } from "./helpers.ts";

test("a clip between two others can only move within the gap, and its grips stop at its neighbours and its recording", () => {
  const s = sample();
  const cut = ok(splitAt(s.model, 30_000, s.screenTrack)).model; // Lesson 0 to 30, and a second piece 30 to 60
  const [a, b] = cut.tracks[0]?.clips ?? [];
  assert.ok(a && b);
  const top = 60_000 + ROOM_PAST_END_MS;
  // The first piece has no room to move (the second starts where it ends), but can be trimmed or shown earlier or later.
  assert.deepEqual(limitsOf(cut, a.id), {
    kind: "clip",
    move: { min: 0, max: 0 },
    trimStart: { min: 0, max: 29_900 },
    trimEnd: { min: 100, max: 30_000 },
  });
  // The second piece may move later, as far as there is room; its start cannot go before the first piece ends; it cannot be
  // made longer than its recording.
  assert.deepEqual(limitsOf(cut, b.id), {
    kind: "clip",
    move: { min: 30_000, max: top },
    trimStart: { min: 30_000, max: 59_900 },
    trimEnd: { min: 30_100, max: 60_000 },
  });
});

test("after trimming there is room to move and to grow again", () => {
  const s = sample();
  const trimmed = ok(trimEnd(ok(trimStart(s.model, s.screen, 10_000)).model, s.screen, 40_000)).model; // 10 to 40 of a 60 second recording
  const limits = limitsOf(trimmed, s.screen);
  assert.deepEqual(limits?.trimStart, { min: 0, max: 39_900 }, "can show all the way back to the start of the recording");
  assert.deepEqual(limits?.trimEnd, { min: 10_100, max: 60_000 }, "can show the rest of the recording");
  assert.equal(limits?.move.min, 0);
});

test("an overlay can go anywhere in time, and its grips keep it at least a tenth of a second long", () => {
  const s = sample();
  const track = ok(addTrack(s.model, "overlay"));
  const text = ok(addText(track.model, track.focus?.[0] ?? "", { startMs: 10_000, durationMs: 5000, text: "Hello" }));
  const id = text.focus?.[0] ?? "";
  const top = Math.max(60_000, modelLengthMs(text.model)) + ROOM_PAST_END_MS;
  assert.deepEqual(limitsOf(text.model, id), {
    kind: "item",
    move: { min: 0, max: top },
    trimStart: { min: 0, max: 14_900 },
    trimEnd: { min: 10_100, max: top },
  });
});

test("a zoom cannot be taken into the zoom before it, nor past the one after it", () => {
  const s = sample();
  const track = ok(addTrack(s.model, "zoom"));
  const rect = { x: 2000, y: 2000, width: 5000, height: 5000 };
  let m = track.model;
  const make = (startMs: number) => {
    const r = ok(addZoom(m, track.focus?.[0] ?? "", { startMs, durationMs: 4000, rect, description: "a part" }));
    m = r.model;
    return r.focus?.[0] ?? "";
  };
  make(5000);
  const middle = make(20_000);
  make(40_000);
  const limits = limitsOf(m, middle);
  assert.deepEqual(limits?.move, { min: 9000, max: 36_000 }, "from where the first ends to where the last starts, less its own length");
  // A zoom needs a second to move in and out (half a second each), so it cannot be made shorter than that.
  assert.deepEqual(limits?.trimStart, { min: 9000, max: 23_000 });
  assert.deepEqual(limits?.trimEnd, { min: 21_000, max: 40_000 });
});

test("there are no limits for something that is not there", () => {
  assert.equal(limitsOf(sample().model, "nope"), undefined);
});

// ---- the limits and the actions always agree ----

function random(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

test("on random edits, every limit is accepted by the action and one step beyond it is refused", () => {
  let checked = 0;
  for (let seed = 1; seed <= 60; seed++) {
    const rnd = random(seed);
    const pick = (n: number) => Math.floor(rnd() * n);
    let m: Model = createModel(SOURCES);
    const screen = addTrack(m, "screen");
    const audio = addTrack(screen.model, "audio");
    const overlay = addTrack(audio.model, "overlay");
    const zoom = addTrack(overlay.model, "zoom");
    m = zoom.model;
    m = ok(addClip(m, screen.focus?.[0] ?? "", { source: SCREEN, startMs: 0 })).model;
    m = ok(addClip(m, audio.focus?.[0] ?? "", { source: MIC, startMs: 0 })).model;
    const rect = { x: 2000, y: 2000, width: 5000, height: 5000 };
    for (let step = 0; step < 24; step++) {
      const time = pick(65_000);
      const clips = m.tracks.flatMap((t) => t.clips);
      const id = clips[pick(Math.max(1, clips.length))]?.id ?? "";
      const tries: Result[] = [
        splitAt(m, time),
        trimStart(m, id, time),
        trimEnd(m, id, time),
        setClipStart(m, id, time),
        deleteClip(m, id, rnd() < 0.5),
        deleteRange(m, time, time + pick(10_000), rnd() < 0.5),
        addText(m, overlay.focus?.[0] ?? "", { startMs: pick(60_000), durationMs: 100 + pick(6000), text: "Hi" }),
        addZoom(m, zoom.focus?.[0] ?? "", { startMs: pick(60_000), durationMs: 1000 + pick(6000), rect, description: "part", transitionMs: pick(500) }),
      ];
      const result = tries[pick(tries.length)];
      if (result?.ok) m = result.model;
    }
    const top = Math.max(modelLengthMs(m), 0) + ROOM_PAST_END_MS;
    for (const track of m.tracks) {
      for (const thing of [...track.clips, ...track.items]) {
        const id = thing.id;
        const limits = limitsOf(m, id);
        assert.ok(limits, `seed ${seed}: limits for ${id}`);
        const isClip = limits.kind === "clip";
        const start = thing.startMs;
        const end = "durationMs" in thing ? itemEndMs(thing) : clipEndMs(thing);
        const topFor = Math.max(top, end + ROOM_PAST_END_MS);
        const move = (ms: number) => (isClip ? setClipStart(m, id, ms) : moveItem(m, id, ms));
        const startTo = (ms: number) => (isClip ? trimStart(m, id, ms) : trimItemStart(m, id, ms));
        const endTo = (ms: number) => (isClip ? trimEnd(m, id, ms) : trimItemEnd(m, id, ms));
        const where = `seed ${seed}, ${id}, from ${start} to ${end}`;
        // the range holds where it is now
        for (const [name, range, at] of [
          ["move", limits.move, start],
          ["trim start", limits.trimStart, start],
          ["trim end", limits.trimEnd, end],
        ] as const) {
          assert.ok(range.min <= at && at <= range.max, `${where}: ${name} range ${range.min} to ${range.max} must hold ${at}`);
        }
        // both ends are accepted
        for (const [name, action, range] of [
          ["move", move, limits.move],
          ["trim start", startTo, limits.trimStart],
          ["trim end", endTo, limits.trimEnd],
        ] as const) {
          assert.equal(action(range.min).ok, true, `${where}: ${name} to its lowest, ${range.min}, is accepted`);
          assert.equal(action(range.max).ok, true, `${where}: ${name} to its highest, ${range.max}, is accepted`);
          checked += 2;
        }
        // one step beyond is refused, unless the range ends because nothing stops it
        const refused = (what: string, result: Result) => assert.equal(result.ok, false, `${where}: ${what} should be refused`);
        if (limits.move.min > 0) refused(`move to ${limits.move.min - 1}`, move(limits.move.min - 1));
        if (limits.move.max < topFor) refused(`move to ${limits.move.max + 1}`, move(limits.move.max + 1));
        if (limits.trimStart.min > 0) refused(`start to ${limits.trimStart.min - 1}`, startTo(limits.trimStart.min - 1));
        if (limits.trimStart.max < end - MIN_CLIP_MS) refused(`start to ${limits.trimStart.max + 1}`, startTo(limits.trimStart.max + 1));
        if (limits.trimEnd.min > start + MIN_CLIP_MS) refused(`end to ${limits.trimEnd.min - 1}`, endTo(limits.trimEnd.min - 1));
        if (limits.trimEnd.max < topFor) refused(`end to ${limits.trimEnd.max + 1}`, endTo(limits.trimEnd.max + 1));
        checked += 6;
      }
    }
  }
  assert.ok(checked > 3000, `only ${checked} checks were made`);
});
