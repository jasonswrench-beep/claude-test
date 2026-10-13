// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { validate, type Clip, type Model, type Track } from "../src/index.ts";
import { LONG_VIDEO, MIC, SCREEN, LOGO, MUSIC, sample } from "./helpers.ts";

function withClips(model: Model, trackId: string, change: (clips: readonly Clip[]) => readonly Clip[]): Model {
  return { ...model, tracks: model.tracks.map((t): Track => (t.id === trackId ? { ...t, clips: change(t.clips) } : t)) };
}

function codes(model: Model): string[] {
  return validate(model).map((p) => p.code);
}

test("a fresh sample has no problems", () => {
  assert.deepEqual(validate(sample().model), []);
});

test("each kind of mistake is found and named in words", () => {
  const s = sample();
  const lesson = s.model.tracks[0]?.clips[0];
  assert.ok(lesson);
  const bad = (change: Partial<Clip>, expected: string, pattern?: RegExp) => {
    const m = withClips(s.model, s.screenTrack, (clips) => clips.map((c) => ({ ...c, ...change })));
    const found = validate(m).filter((p) => p.code === expected);
    assert.ok(found.length > 0, `${expected} not found for ${JSON.stringify(change)}`);
    if (pattern !== undefined) assert.match(found[0]?.message ?? "", pattern);
  };
  bad({ startMs: -1 }, "bad-time", /not a whole number of milliseconds/);
  bad({ startMs: 1.5 }, "bad-time");
  bad({ sourceEndMs: 50 }, "too-short", /at least 0.1 seconds/);
  bad({ sourceEndMs: 61_000 }, "beyond-source", /uses up to 1 minute 1 second of Screen of take 1, which is only 1 minute long/);
  bad({ source: { kind: "take", take: 9, media: "nothing.mp4" } }, "unknown-source");
  bad({ source: MIC }, "source-not-allowed", /cannot go on a screen track/);
  bad({ fadeInMs: 40_000, fadeOutMs: 30_000 }, "fade-too-long", /longer than the clip/);
  bad({ volumeDb: 30 }, "volume-range", /from -40 to 20/);
  bad({ volumeDb: 2.5 }, "volume-range");
});

test("overlap and order mistakes are found", () => {
  const s = sample();
  const lesson = s.model.tracks[0]?.clips[0];
  assert.ok(lesson);
  const second: Clip = { ...lesson, id: "clip-90", startMs: 30_000, sourceStartMs: 0, sourceEndMs: 10_000 };
  assert.deepEqual(codes(withClips(s.model, s.screenTrack, (c) => [...c, second])), ["overlap"]);
  assert.deepEqual(codes(withClips(s.model, s.screenTrack, (c) => [second, ...c])), ["unsorted"]);
});

test("duplicate ids are found", () => {
  const s = sample();
  const lesson = s.model.tracks[0]?.clips[0];
  assert.ok(lesson);
  const twin: Clip = { ...lesson, startMs: 70_000, sourceStartMs: 0, sourceEndMs: 5000 };
  assert.ok(codes(withClips(s.model, s.screenTrack, (c) => [...c, twin])).includes("duplicate-id"));
  const dup = { ...s.model, tracks: [...s.model.tracks, { ...s.model.tracks[0]!, clips: [] }] };
  assert.ok(codes(dup).includes("duplicate-id"));
});

test("an imported video over 30 seconds is an error; an image or sound without its text is a request", () => {
  const s = sample();
  const base = (source: Clip["source"], length: number, extra: Partial<Clip> = {}): Clip => ({
    id: "clip-50",
    name: "Imported",
    source,
    sourceStartMs: 0,
    sourceEndMs: length,
    startMs: 0,
    muted: false,
    volumeDb: 0,
    fadeInMs: 0,
    fadeOutMs: 0,
    ...extra,
  });
  const onMedia = (c: Clip) => validate(withClips(s.model, s.mediaTrack, () => [c]));
  const long = onMedia(base(LONG_VIDEO, 31_000)).find((p) => p.code === "imported-video-too-long");
  assert.equal(long?.severity, "error");
  assert.deepEqual(onMedia(base(LONG_VIDEO, 30_000)), []);
  const alt = onMedia(base(LOGO, 3000)).find((p) => p.code === "needs-alt-text");
  assert.equal(alt?.severity, "needs-attention");
  assert.deepEqual(onMedia(base(LOGO, 3000, { decorative: true })), []);
  assert.deepEqual(onMedia(base(LOGO, 3000, { altText: "A logo" })), []);
  assert.equal(onMedia(base(LOGO, 3000, { altText: "   " })).length, 1, "blank alt text is no alt text");
  const music = onMedia(base(MUSIC, 10_000)).find((p) => p.code === "needs-text-alternative");
  assert.match(music?.message ?? "", /wordless music/);
  assert.ok(s.screenTrack && SCREEN);
});
