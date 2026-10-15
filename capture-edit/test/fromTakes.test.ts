// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { editFromTakes, listRows, parse, serialize, validate, type TakeInfo } from "../src/index.ts";

const take1: TakeInfo = {
  take: 1,
  state: "complete",
  durationMs: 12_000,
  media: [
    { file: "screen.mp4", kind: "screen" },
    { file: "mic.wav", kind: "microphone" },
    { file: "system.wav", kind: "system-audio" },
    { file: "webcam.mp4", kind: "webcam" },
    { file: "events.jsonl", kind: "interactions" },
  ],
};

test("a finished take becomes a track and a clip for each kind of recording, all starting together", () => {
  const { model, notes } = editFromTakes([take1]);
  assert.deepEqual(notes, []);
  assert.deepEqual(
    listRows(model).map((r) => [r.trackName, r.name, r.type, r.startMs, r.lengthMs, r.state]),
    [
      ["Screen", "Screen of take 1", "Screen", 0, 12_000, "ready"],
      ["Webcam", "Webcam of take 1", "Webcam", 0, 12_000, "ready"],
      ["Microphone", "Microphone of take 1", "Microphone", 0, 12_000, "ready"],
      ["Computer sound", "Computer sound of take 1", "Computer sound", 0, 12_000, "ready"],
    ],
  );
  assert.deepEqual(model.tracks.map((t) => t.kind), ["screen", "webcam", "audio", "audio"]);
  assert.deepEqual(validate(model), []);
  assert.deepEqual(
    model.sources.map((s) => [s.ref, s.durationMs]),
    [
      [{ kind: "take", take: 1, media: "screen.mp4" }, 12_000],
      [{ kind: "take", take: 1, media: "mic.wav" }, 12_000],
      [{ kind: "take", take: 1, media: "system.wav" }, 12_000],
      [{ kind: "take", take: 1, media: "webcam.mp4" }, 12_000],
    ],
  );
});

test("only the tracks that were recorded are made, and the edit saves and reads back the same", () => {
  const { model } = editFromTakes([{ ...take1, media: [{ file: "screen.mp4", kind: "screen" }, { file: "mic.wav", kind: "microphone" }] }]);
  assert.deepEqual(model.tracks.map((t) => t.name), ["Screen", "Microphone"]);
  const back = parse(serialize(model));
  assert.ok(back.ok);
  assert.deepEqual(back.model, model);
});

test("several takes are laid one after another", () => {
  const second: TakeInfo = { take: 2, state: "recovered", durationMs: 8000, media: [{ file: "screen.mp4", kind: "screen" }, { file: "mic.wav", kind: "microphone" }] };
  const { model, notes } = editFromTakes([second, take1]);
  assert.deepEqual(notes, ["2 takes were laid one after another, 20 seconds in all."]);
  const screen = model.tracks[0]?.clips.map((c) => [c.name, c.startMs, c.sourceEndMs]);
  assert.deepEqual(screen, [["Screen of take 1", 0, 12_000], ["Screen of take 2", 12_000, 8000]]);
  assert.deepEqual(validate(model), []);
});

test("a take that is not finished, or was set aside, or has no length, is left out and the reason is said", () => {
  const { model, notes } = editFromTakes([
    take1,
    { ...take1, take: 2, state: "recording" },
    { ...take1, take: 3, state: "setAside" },
    { ...take1, take: 4, durationMs: null },
    { ...take1, take: 5, state: "mystery" },
    { take: 6, state: "complete", durationMs: 5000, media: [{ file: "events.jsonl", kind: "interactions" }] },
  ]);
  assert.equal(model.tracks[0]?.clips.length, 1);
  assert.deepEqual(notes, [
    "Take 2 was not finished (it was still recording, or the program stopped), so it is not in the edit. Recover it first.",
    "Take 3 was set aside (Start again), so it is not in the edit.",
    "Take 4 has no length recorded, so it is not in the edit.",
    "Take 5 is in a state this program does not know (mystery), so it is not in the edit.",
    "Take 6 has no picture or sound that can be edited yet, so it is not in the edit.",
  ]);
});

test("with nothing to edit the edit is empty and says so", () => {
  const { model, notes } = editFromTakes([]);
  assert.equal(model.tracks.length, 0);
  assert.deepEqual(notes, ["There is nothing recorded yet that can be edited."]);
});
