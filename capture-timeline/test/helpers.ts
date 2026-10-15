// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { addClip, addTrack, createModel, type Model } from "../../capture-edit/src/index.ts";
import { sampleModel, SAMPLE_SOURCES } from "../src/sample.ts";
import { EditSession } from "../src/session.ts";

/** A session on the sample lesson, with the row called `name` chosen. */
export function open(name?: string): EditSession {
  const session = new EditSession(sampleModel());
  if (name !== undefined) select(session, name);
  return session;
}

export function select(session: EditSession, name: string): void {
  const row = session.rows.find((r) => r.name === name);
  assert.ok(row, `no row called ${name}`);
  assert.ok(session.select(row.clipId).ok);
}

export function row(session: EditSession, name: string) {
  const found = session.rows.find((r) => r.name === name);
  assert.ok(found, `no row called ${name}`);
  return found;
}

export function names(session: EditSession, track?: string): string[] {
  return session.rows.filter((r) => track === undefined || r.trackName === track).map((r) => r.name);
}

/** An edit with one 10 second clip on a screen track, and nothing else. */
export function solo(): Model {
  let model = createModel(SAMPLE_SOURCES);
  const track = addTrack(model, "screen");
  model = track.model;
  const added = addClip(model, track.focus?.[0] ?? "", {
    source: { kind: "take", take: 1, media: "screen.mp4" },
    startMs: 0,
    lengthMs: 10_000,
    name: "Only clip",
  });
  assert.ok(added.ok);
  return added.model;
}
