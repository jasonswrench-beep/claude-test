// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import {
  addClip,
  addTrack,
  createModel,
  findClip,
  type Clip,
  type Model,
  type Result,
  type SourceInfo,
  type SourceRef,
} from "../src/index.ts";

export const SCREEN: SourceRef = { kind: "take", take: 1, media: "screen.mp4" };
export const MIC: SourceRef = { kind: "take", take: 1, media: "mic.wav" };
export const SYSTEM: SourceRef = { kind: "take", take: 1, media: "system.wav" };
export const LOGO: SourceRef = { kind: "asset", asset: "logo.png" };
export const SHORT_VIDEO: SourceRef = { kind: "asset", asset: "intro-20s.mp4" };
export const LONG_VIDEO: SourceRef = { kind: "asset", asset: "tour-45s.mp4" };
export const MUSIC: SourceRef = { kind: "asset", asset: "music.mp3" };

export const SOURCES: readonly SourceInfo[] = [
  { ref: SCREEN, kind: "screen", name: "Screen of take 1", durationMs: 60_000 },
  { ref: MIC, kind: "microphone", name: "Microphone of take 1", durationMs: 60_000 },
  { ref: SYSTEM, kind: "system-audio", name: "Computer sound of take 1", durationMs: 60_000 },
  { ref: LOGO, kind: "image", name: "Logo", durationMs: null },
  { ref: SHORT_VIDEO, kind: "video", name: "Intro video", durationMs: 20_000, hasTextAlternative: true },
  { ref: LONG_VIDEO, kind: "video", name: "Long tour", durationMs: 45_000, hasTextAlternative: true },
  { ref: MUSIC, kind: "audio", name: "Music", durationMs: 120_000, hasTextAlternative: false },
];

/** Unwrap a result that must have worked. */
export function ok(result: Result): Extract<Result, { ok: true }> {
  assert.ok(result.ok, result.ok ? "" : `refused: ${result.message}`);
  return result;
}

/** Unwrap a result that must have been refused, and give its message. */
export function refused(result: Result): string {
  assert.equal(result.ok, false, result.ok ? `should have been refused, but said: ${result.message}` : "");
  return result.ok ? "" : result.message;
}

export interface Sample {
  readonly model: Model;
  readonly screenTrack: string;
  readonly audioTrack: string;
  readonly mediaTrack: string;
  readonly screen: string;
  readonly mic: string;
}

/** A screen track and an audio track with one 60 second clip each, and an empty imported media track. */
export function sample(): Sample {
  let model = createModel(SOURCES);
  const t1 = addTrack(model, "screen");
  const t2 = addTrack(t1.model, "audio");
  const t3 = addTrack(t2.model, "media");
  model = t3.model;
  const screenTrack = t1.focus?.[0] ?? "";
  const audioTrack = t2.focus?.[0] ?? "";
  const mediaTrack = t3.focus?.[0] ?? "";
  const a = ok(addClip(model, screenTrack, { source: SCREEN, startMs: 0, name: "Lesson" }));
  const b = ok(addClip(a.model, audioTrack, { source: MIC, startMs: 0, name: "Voice" }));
  return { model: b.model, screenTrack, audioTrack, mediaTrack, screen: a.focus?.[0] ?? "", mic: b.focus?.[0] ?? "" };
}

export function clipOf(model: Model, id: string): Clip {
  const found = findClip(model, id);
  assert.ok(found, `no clip ${id}`);
  return found.clip;
}

export function clipsOn(model: Model, trackId: string): readonly Clip[] {
  const track = model.tracks.find((t) => t.id === trackId);
  assert.ok(track, `no track ${trackId}`);
  return track.clips;
}
