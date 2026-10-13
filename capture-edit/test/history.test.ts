// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { History, deleteClip, setClipStart, splitAt, trimEnd, trimStart, type Model } from "../src/index.ts";
import { ok, refused, sample } from "./helpers.ts";

function start() {
  const s = sample();
  return { s, history: new History(s.model) };
}

test("undo and redo go back and forth through what was done, and say so", () => {
  const { s, history } = start();
  const original = history.model;
  assert.equal(history.canUndo, false);
  assert.equal(history.undo().message, "Nothing to undo.");

  history.run((m) => splitAt(m, 30_000, s.screenTrack));
  const split = history.model;
  history.run((m) => trimEnd(m, s.mic, 40_000));
  assert.equal(history.undoLabel, "trim clip end");

  assert.deepEqual(history.undo(), { ok: true, message: "Undid: trim clip end." });
  assert.equal(history.undoLabel, "split clip");
  assert.equal(history.redoLabel, "trim clip end");
  assert.deepEqual(history.undo(), { ok: true, message: "Undid: split clip." });
  assert.equal(history.model, original);

  assert.deepEqual(history.redo(), { ok: true, message: "Redid: split clip." });
  assert.equal(history.model, split);
  history.redo();
  assert.equal(history.canRedo, false);
  assert.equal(history.redo().message, "Nothing to redo.");
});

test("doing something new after an undo clears what could have been redone", () => {
  const { s, history } = start();
  history.run((m) => splitAt(m, 30_000, s.screenTrack));
  history.undo();
  assert.equal(history.canRedo, true);
  history.run((m) => trimEnd(m, s.mic, 40_000));
  assert.equal(history.canRedo, false);
});

test("a refused action and an action that changes nothing leave no undo step", () => {
  const { s, history } = start();
  const refusal = history.run((m) => splitAt(m, 90_000, s.screenTrack));
  assert.equal(refusal.ok, false);
  const nothing = history.run((m) => trimEnd(m, s.screen, 60_000));
  assert.equal(nothing.ok, true);
  assert.equal(history.canUndo, false);
});

test("a whole drag is one undo step, and its message is the last thing it did", () => {
  const { s, history } = start();
  const original = history.model;
  history.run((m) => splitAt(m, 30_000, s.screenTrack));
  const afterSplit = history.model;
  const second = history.model.tracks[0]?.clips[1];
  assert.ok(second);

  assert.equal(history.beginGroup("move clip").ok, true);
  for (const to of [31_000, 32_000, 33_000, 34_000]) history.run((m) => trimStart(m, second.id, to));
  assert.equal(history.undo().ok, false, "undo waits until the drag ends");
  const end = history.endGroup();
  assert.equal(end.message, 'Clip "Lesson" now starts at 34 seconds, 4 seconds shorter.', "the whole drag, not its last small step");

  assert.equal(history.undoLabel, "move clip");
  history.undo();
  assert.equal(history.model, afterSplit, "one undo undoes the whole drag");
  history.undo();
  assert.equal(history.model, original);
});

test("escape during a drag puts everything back and records nothing", () => {
  const { s, history } = start();
  history.run((m) => splitAt(m, 30_000, s.screenTrack));
  const before = history.model;
  const second = before.tracks[0]?.clips[1];
  assert.ok(second);
  history.beginGroup("trim clip start");
  history.run((m) => trimStart(m, second.id, 40_000));
  assert.notEqual(history.model, before);
  assert.deepEqual(history.cancelGroup(), { ok: true, message: "Cancelled. Nothing was changed." });
  assert.equal(history.model, before);
  assert.equal(history.undoLabel, "split clip", "no step was added for the drag");
});

test("a drag that changed nothing records nothing", () => {
  const { history } = start();
  history.beginGroup("move clip");
  const end = history.endGroup();
  assert.equal(end.ok, true);
  assert.equal(history.canUndo, false);
  assert.equal(history.endGroup().ok, false);
  assert.equal(history.cancelGroup().ok, false);
});

test("only a limited number of steps are kept, the oldest forgotten first", () => {
  const s = sample();
  const history = new History(s.model, 3);
  for (let i = 1; i <= 5; i++) history.run((m) => splitAt(m, i * 5000, s.screenTrack));
  let undone = 0;
  while (history.undo().ok) undone += 1;
  assert.equal(undone, 3);
});

test("undoing everything gives back the starting model and redoing everything the final one, after a long mixed session", () => {
  const { s, history } = start();
  const original = history.model;
  const steps: Array<(m: Model) => ReturnType<typeof splitAt>> = [
    (m) => splitAt(m, 10_000),
    (m) => splitAt(m, 40_000),
    (m) => trimEnd(m, m.tracks[0]?.clips[0]?.id ?? "", 8000),
    (m) => deleteClip(m, m.tracks[0]?.clips[1]?.id ?? "", true),
    (m) => setClipStart(m, m.tracks[1]?.clips[2]?.id ?? "", 41_000),
  ];
  let done = 0;
  for (const step of steps) if (history.run(step).ok) done += 1;
  assert.ok(done >= 4);
  const finalModel: Model = history.model;
  while (history.undo().ok) {
    /* back to the start */
  }
  assert.deepEqual(history.model, original);
  while (history.redo().ok) {
    /* forward again */
  }
  assert.deepEqual(history.model, finalModel);
  assert.ok(s);
  assert.match(refused(splitAt(history.model, 999_999)), /Nothing to split/);
});
