// SPDX-License-Identifier: Apache-2.0
// Playing: the session only asks whatever plays the edit to start or stop, and says when it has.
import assert from "node:assert/strict";
import { test } from "node:test";
import { type ChangeKind, type Outcome } from "../src/session.ts";
import { open } from "./helpers.ts";

test("with nothing to play the edit, the play command says so", () => {
  const s = open();
  assert.deepEqual(s.togglePlay(), { ok: false, message: "There is nothing here to play the edit with." });
  assert.equal(s.playing, false);
});

test("the play command asks the player, and the screens are told when it starts and stops", () => {
  const s = open();
  const heard: ChangeKind[] = [];
  s.subscribe((k) => heard.push(k));
  let asked = 0;
  s.setTransport({
    toggle(): Outcome {
      asked += 1;
      s.setPlaying(!s.playing);
      return { ok: true, message: s.playing ? "Playing." : "Paused." };
    },
  });
  assert.equal(s.togglePlay().message, "Playing.");
  assert.equal(s.playing, true);
  assert.equal(s.togglePlay().message, "Paused.");
  assert.equal(s.playing, false);
  assert.equal(asked, 2);
  assert.deepEqual(heard, ["transport", "transport"]);
  s.setPlaying(false);
  assert.deepEqual(heard, ["transport", "transport"], "nothing is said when nothing changed");
  s.setTransport(undefined);
  assert.equal(s.togglePlay().ok, false);
});

test("editing and moving the playhead carry on while it plays", () => {
  const s = open("Open the gradebook");
  s.setTransport({ toggle: () => ({ ok: true, message: "" }) });
  s.setPlaying(true);
  s.setPlayhead(10_000);
  assert.equal(s.splitSelected().ok, true);
  assert.equal(s.rename("Intro").ok, true);
  assert.equal(s.playing, true);
});
