// SPDX-License-Identifier: Apache-2.0
// Many random editing sessions: whatever is done, the edit stays sound, every sentence is clean,
// and undo and redo walk back and forth exactly.
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  History,
  addClip,
  addTrack,
  clipEndMs,
  deleteClip,
  deleteRange,
  describeAt,
  joinWithNext,
  parse,
  reorderClip,
  serialize,
  setClipLength,
  setClipStart,
  setFades,
  setMuted,
  setVolume,
  splitAt,
  trimEnd,
  trimStart,
  validate,
  type Model,
  type Result,
} from "../src/index.ts";
import { LOGO, MIC, SCREEN, SHORT_VIDEO, SOURCES, SYSTEM } from "./helpers.ts";
import { createModel } from "../src/index.ts";

/** A small, repeatable random number generator. */
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

function start(): { model: Model; ids: string[] } {
  let m = createModel(SOURCES);
  const ids: string[] = [];
  for (const kind of ["screen", "audio", "audio", "media"] as const) {
    const t = addTrack(m, kind);
    m = t.model;
    ids.push(t.focus?.[0] ?? "");
  }
  m = (addClip(m, ids[0] ?? "", { source: SCREEN, startMs: 0 }) as Extract<Result, { ok: true }>).model;
  m = (addClip(m, ids[1] ?? "", { source: MIC, startMs: 0 }) as Extract<Result, { ok: true }>).model;
  m = (addClip(m, ids[2] ?? "", { source: SYSTEM, startMs: 0 }) as Extract<Result, { ok: true }>).model;
  return { model: m, ids };
}

const BAD_WORDS = new RegExp("undefined|NaN|Infinity|null|\\[object|" + String.fromCharCode(0x2014));

test("random editing sessions keep the edit sound, the sentences clean, and undo exact", () => {
  let successes = 0;
  let refusals = 0;
  for (let seed = 1; seed <= 150; seed++) {
    const rnd = random(seed);
    const pick = (n: number) => Math.floor(rnd() * n);
    const { model: first, ids } = start();
    const history = new History(first, 500);
    const original = history.model;
    for (let step = 0; step < 40; step++) {
      const model = history.model;
      const clips = model.tracks.flatMap((t) => t.clips);
      const clip = clips[pick(Math.max(1, clips.length))];
      const clipId = clip?.id ?? "clip-none";
      const time = pick(65_000);
      const kind = pick(14);
      const closeGap = rnd() < 0.5;
      const run = (): Result => {
        switch (kind) {
          case 0:
            return splitAt(model, time);
          case 1:
            return splitAt(model, time, ids[pick(ids.length)]);
          case 2:
            return trimStart(model, clipId, time);
          case 3:
            return trimEnd(model, clipId, time);
          case 4:
            return setClipStart(model, clipId, time);
          case 5:
            return setClipLength(model, clipId, pick(30_000));
          case 6:
            return deleteClip(model, clipId, closeGap);
          case 7:
            return deleteRange(model, time, time + pick(15_000), closeGap);
          case 8:
            return joinWithNext(model, clipId);
          case 9:
            return reorderClip(model, clipId, pick(5));
          case 10:
            return setFades(model, clipId, pick(3000), pick(3000));
          case 11:
            return setVolume(model, clipId, pick(61) - 40);
          case 12:
            return setMuted(model, clipId, rnd() < 0.5);
          default:
            return addClip(model, ids[3] ?? "", { source: rnd() < 0.5 ? LOGO : SHORT_VIDEO, startMs: pick(60_000), lengthMs: 100 + pick(8000), decorative: true });
        }
      };
      const result = history.run(run);
      assert.equal(typeof result.message, "string");
      assert.ok(result.message.length > 0, "every result has a sentence");
      assert.doesNotMatch(result.message, BAD_WORDS, `seed ${seed} step ${step}: ${result.message}`);
      if (!result.ok) {
        refusals += 1;
        assert.equal(history.model, model, "a refused action changes nothing");
        continue;
      }
      successes += 1;
      const errors = validate(history.model).filter((p) => p.severity === "error");
      assert.deepEqual(errors, [], `seed ${seed} step ${step} (${result.label}): ${result.message}`);
      for (const track of history.model.tracks) {
        for (let i = 1; i < track.clips.length; i++) {
          const a = track.clips[i - 1];
          const b = track.clips[i];
          assert.ok(a && b && clipEndMs(a) <= b.startMs, `clips overlap on ${track.name}`);
        }
      }
    }
    // The description of the picture never breaks either.
    assert.doesNotMatch(describeAt(history.model, pick(70_000)), BAD_WORDS);
    // Saved and read back, it is the same edit.
    const text = serialize(history.model);
    const back = parse(text);
    assert.ok(back.ok && JSON.stringify(back.model) === JSON.stringify(history.model), `seed ${seed}: save and reopen`);
    assert.equal(back.ok && serialize(back.model), text);
    // Undo everything, then redo everything.
    const final = history.model;
    while (history.undo().ok) {
      /* undoing */
    }
    assert.deepEqual(history.model, original, `seed ${seed}: undo reaches the start`);
    while (history.redo().ok) {
      /* redoing */
    }
    assert.deepEqual(history.model, final, `seed ${seed}: redo reaches the end`);
  }
  assert.ok(successes > 1500, `enough actions worked to mean something: ${successes}`);
  assert.ok(refusals > 300, `enough were refused to mean something: ${refusals}`);
});
