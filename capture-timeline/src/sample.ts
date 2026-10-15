// SPDX-License-Identifier: Apache-2.0
/**
 * A small made-up lesson edit, for the demo page and the tests: a 90 second screen recording cut into
 * three clips, the voice over it cut the same way, a webcam clip, two overlays and a zoom. Nothing here
 * is a real recording; the sources only have names and lengths.
 */

import {
  addArrow,
  addClip,
  addText,
  addTrack,
  addZoom,
  createModel,
  renameClip,
  splitAt,
  type Model,
  type Result,
  type SourceInfo,
  type SourceRef,
} from "../../capture-edit/src/index.ts";

const SCREEN: SourceRef = { kind: "take", take: 1, media: "screen.mp4" };
const MIC: SourceRef = { kind: "take", take: 1, media: "mic.wav" };
const WEBCAM: SourceRef = { kind: "take", take: 1, media: "webcam.mp4" };

export const SAMPLE_SOURCES: readonly SourceInfo[] = [
  { ref: SCREEN, kind: "screen", name: "Screen of take 1", durationMs: 90_000 },
  { ref: MIC, kind: "microphone", name: "Microphone of take 1", durationMs: 90_000 },
  { ref: WEBCAM, kind: "webcam", name: "Webcam of take 1", durationMs: 90_000 },
];

function must(result: Result): Extract<Result, { ok: true }> {
  if (!result.ok) throw new Error(`The sample edit could not be built: ${result.message}`);
  return result;
}

/** The sample edit. Built with the edit model's own actions, so it is always sound. */
export function sampleModel(): Model {
  let model = createModel(SAMPLE_SOURCES);
  const screen = must(addTrack(model, "screen"));
  const voice = must(addTrack(screen.model, "audio", "Voice"));
  const webcam = must(addTrack(voice.model, "webcam"));
  const overlay = must(addTrack(webcam.model, "overlay"));
  const zoom = must(addTrack(overlay.model, "zoom"));
  model = zoom.model;
  const [screenId = "", voiceId = "", webcamId = "", overlayId = "", zoomId = ""] = [screen, voice, webcam, overlay, zoom].map(
    (r) => r.focus?.[0] ?? "",
  );

  model = must(addClip(model, screenId, { source: SCREEN, startMs: 0, name: "Lesson screen" })).model;
  model = must(addClip(model, voiceId, { source: MIC, startMs: 0, name: "Voice" })).model;
  model = must(addClip(model, webcamId, { source: WEBCAM, startMs: 0, name: "Presenter" })).model;

  // Cut picture and sound together into three parts (30 and 60 seconds), then give the parts names.
  model = must(splitAt(model, 30_000)).model;
  model = must(splitAt(model, 60_000)).model;
  const names: Record<string, readonly string[]> = {
    [screenId]: ["Open the gradebook", "Add an assignment", "Check the totals"],
    [voiceId]: ["Voice, part 1", "Voice, part 2", "Voice, part 3"],
    [webcamId]: ["Presenter, part 1", "Presenter, part 2", "Presenter, part 3"],
  };
  for (const track of model.tracks) {
    const wanted = names[track.id];
    if (wanted === undefined) continue;
    const ids = track.clips.map((c) => c.id);
    for (const [i, id] of ids.entries()) {
      const name = wanted[i];
      if (name !== undefined) model = must(renameClip(model, id, name)).model;
    }
  }

  model = must(addText(model, overlayId, { startMs: 12_000, durationMs: 4_000, text: "Open Settings", name: "Title: Open Settings" })).model;
  model = must(
    addArrow(model, overlayId, {
      startMs: 36_000,
      durationMs: 5_000,
      name: "Arrow to the Add button",
      from: { x: 2000, y: 6500 },
      to: { x: 4800, y: 4200 },
      altText: "An arrow pointing to the Add button",
    }),
  ).model;
  model = must(
    addZoom(model, zoomId, {
      startMs: 70_000,
      durationMs: 8_000,
      rect: { x: 2500, y: 2500, width: 5000, height: 5000 },
      description: "the totals column",
      name: "Zoom on the totals",
    }),
  ).model;
  return model;
}

/**
 * A one hour recording cut into 720 clips (the screen and the voice, 360 pieces of 10 seconds each), to see that the
 * screens stay quick on a long edit. Open the demo with `?long=1`.
 */
export function longSampleModel(): Model {
  const screen: SourceRef = { kind: "take", take: 1, media: "screen.mp4" };
  const mic: SourceRef = { kind: "take", take: 1, media: "mic.wav" };
  let model = createModel([
    { ref: screen, kind: "screen", name: "Screen of take 1", durationMs: 3_600_000 },
    { ref: mic, kind: "microphone", name: "Microphone of take 1", durationMs: 3_600_000 },
  ]);
  const a = must(addTrack(model, "screen"));
  const b = must(addTrack(a.model, "audio", "Voice"));
  model = b.model;
  model = must(addClip(model, a.focus?.[0] ?? "", { source: screen, startMs: 0, name: "Screen" })).model;
  model = must(addClip(model, b.focus?.[0] ?? "", { source: mic, startMs: 0, name: "Voice" })).model;
  for (let i = 1; i < 360; i++) model = must(splitAt(model, i * 10_000)).model;
  return model;
}
