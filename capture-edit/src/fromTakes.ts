// SPDX-License-Identifier: Apache-2.0
/**
 * Starting an edit from what was recorded: one source for each file of each finished take, a track for each kind of
 * recording, and one clip for each file, takes one after another. This is what the first edit of a project is, so
 * there is something to cut.
 *
 * The takes are read from the project folder by the program that opens it (`take.json` of each take); this file only
 * turns what they say into an edit, so it can be tested without a folder.
 */

import { addClip, addTrack, type Result } from "./ops.ts";
import { createModel, type Model, type SourceInfo, type SourceKind, type TrackKind } from "./model.ts";
import { describeLength } from "./time.ts";

/** One media file of a take, as `take.json` lists it. */
export interface TakeFile {
  readonly file: string;
  /** `screen`, `webcam`, `microphone`, `system-audio`, `interactions` and anything newer. */
  readonly kind: string;
}

/** The parts of a take that an edit needs. */
export interface TakeInfo {
  /** The take's number: 1 for `take-001`. */
  readonly take: number;
  /** `complete`, `recovered`, `recording` or `setAside`. */
  readonly state: string;
  readonly durationMs: number | null;
  readonly media: readonly TakeFile[];
}

export interface EditFromTakes {
  readonly model: Model;
  /** In words: what was left out and why. Empty when everything was used. */
  readonly notes: readonly string[];
}

interface Lane {
  readonly source: SourceKind;
  readonly track: TrackKind;
  readonly trackName: string;
  readonly sourceName: string;
}

/** The kinds of recording an edit uses, in the order their tracks are shown. */
const LANES: readonly Lane[] = [
  { source: "screen", track: "screen", trackName: "Screen", sourceName: "Screen" },
  { source: "webcam", track: "webcam", trackName: "Webcam", sourceName: "Webcam" },
  { source: "microphone", track: "audio", trackName: "Microphone", sourceName: "Microphone" },
  { source: "system-audio", track: "audio", trackName: "Computer sound", sourceName: "Computer sound" },
];

/** Make an edit from takes. Takes that are not finished, or were set aside, are left out and said so. */
export function editFromTakes(takes: readonly TakeInfo[]): EditFromTakes {
  const notes: string[] = [];
  const usable: TakeInfo[] = [];
  for (const take of [...takes].sort((a, b) => a.take - b.take)) {
    if (take.state === "recording") {
      notes.push(`Take ${take.take} was not finished (it was still recording, or the program stopped), so it is not in the edit. Recover it first.`);
    } else if (take.state === "setAside") {
      notes.push(`Take ${take.take} was set aside (Start again), so it is not in the edit.`);
    } else if (take.state !== "complete" && take.state !== "recovered") {
      notes.push(`Take ${take.take} is in a state this program does not know (${take.state}), so it is not in the edit.`);
    } else if (take.durationMs === null || take.durationMs <= 0) {
      notes.push(`Take ${take.take} has no length recorded, so it is not in the edit.`);
    } else if (!take.media.some((m) => LANES.some((l) => l.source === m.kind))) {
      notes.push(`Take ${take.take} has no picture or sound that can be edited yet, so it is not in the edit.`);
    } else {
      usable.push(take);
    }
  }

  // The sources: every file of every usable take.
  const sources: SourceInfo[] = [];
  for (const take of usable) {
    for (const file of take.media) {
      const lane = LANES.find((l) => l.source === file.kind);
      if (lane === undefined || take.durationMs === null) continue;
      sources.push({
        ref: { kind: "take", take: take.take, media: file.file },
        kind: lane.source,
        name: `${lane.sourceName} of take ${take.take}`,
        durationMs: take.durationMs,
      });
    }
  }

  let model = createModel(sources);
  // A track for each kind of recording that any take has, in a fixed order.
  const tracks = new Map<string, string>();
  for (const lane of LANES) {
    if (!sources.some((s) => s.kind === lane.source)) continue;
    const added = addTrack(model, lane.track, lane.trackName);
    model = added.model;
    tracks.set(lane.source, added.focus?.[0] ?? "");
  }

  // One clip for each file, with the takes laid one after another.
  let at = 0;
  for (const take of usable) {
    if (take.durationMs === null) continue;
    for (const file of take.media) {
      const lane = LANES.find((l) => l.source === file.kind);
      const track = lane === undefined ? undefined : tracks.get(lane.source);
      if (lane === undefined || track === undefined) continue;
      const result: Result = addClip(model, track, {
        source: { kind: "take", take: take.take, media: file.file },
        startMs: at,
        name: `${lane.sourceName} of take ${take.take}`,
      });
      if (result.ok) model = result.model;
      else notes.push(`${lane.sourceName} of take ${take.take} could not be added: ${result.message}`);
    }
    at += take.durationMs;
  }

  if (usable.length === 0) notes.push("There is nothing recorded yet that can be edited.");
  else if (usable.length > 1) notes.push(`${usable.length} takes were laid one after another, ${describeLength(at)} in all.`);
  return { model, notes };
}
