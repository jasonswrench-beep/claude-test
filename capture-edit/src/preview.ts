// SPDX-License-Identifier: Apache-2.0
/**
 * Saying what the edit holds, in words: a row for every clip (the list that is the foundation of
 * the timeline, story C1), a sentence for each clip, and a description of what is on screen at any
 * moment (story C4), so the result can be checked without seeing it.
 */

import { clipEndMs, clipLengthMs, findSource, modelLengthMs, type Clip, type Model, type Track } from "./model.ts";
import { describeLength, describeTime, wholeMs } from "./time.ts";
import { validate } from "./validate.ts";

/** One row of the clip list. */
export interface Row {
  readonly trackId: string;
  readonly trackName: string;
  readonly clipId: string;
  readonly name: string;
  /** What the clip holds, such as "Screen" or "Image". */
  readonly type: string;
  readonly startMs: number;
  readonly lengthMs: number;
  readonly endMs: number;
  /** In words: "ready", "muted", "needs attention" or "has a problem". */
  readonly state: string;
}

/** The column headers of the list, in order. */
export const COLUMNS = ["Name", "Track", "Type", "Start", "Length", "End", "State"] as const;

function typeName(model: Model, clip: Clip): string {
  const source = findSource(model, clip.source);
  switch (source?.kind) {
    case "screen":
      return "Screen";
    case "webcam":
      return "Webcam";
    case "microphone":
      return "Microphone";
    case "system-audio":
      return "Computer sound";
    case "image":
      return "Image";
    case "audio":
      return "Sound";
    case "video":
      return "Video";
    default:
      return "Unknown";
  }
}

/** Every clip of every track, in track order and then time order. */
export function listRows(model: Model): Row[] {
  const problems = validate(model);
  const rows: Row[] = [];
  for (const track of model.tracks) {
    for (const clip of track.clips) {
      const mine = problems.filter((p) => p.clipId === clip.id);
      const state = mine.some((p) => p.severity === "error")
        ? "has a problem"
        : mine.length > 0
          ? "needs attention"
          : clip.muted
            ? "muted"
            : "ready";
      rows.push({
        trackId: track.id,
        trackName: track.name,
        clipId: clip.id,
        name: clip.name,
        type: typeName(model, clip),
        startMs: clip.startMs,
        lengthMs: clipLengthMs(clip),
        endMs: clipEndMs(clip),
        state,
      });
    }
  }
  return rows;
}

/** A caption for the list: how much it holds. */
export function listCaption(model: Model): string {
  const clips = model.tracks.reduce((n, t) => n + t.clips.length, 0);
  const tracks = model.tracks.length;
  return `Tracks and clips: ${clips} ${clips === 1 ? "clip" : "clips"} on ${tracks} ${tracks === 1 ? "track" : "tracks"}, ${describeLength(modelLengthMs(model))} long.`;
}

/** One clip in a sentence, as it is read when it takes focus. */
export function describeClip(model: Model, clipId: string): string {
  for (const track of model.tracks) {
    const clip = track.clips.find((c) => c.id === clipId);
    if (clip === undefined) continue;
    const row = listRows(model).find((r) => r.clipId === clipId);
    const parts = [
      `${clip.name}, ${typeName(model, clip).toLowerCase()} on the ${track.name} track`,
      `starts at ${describeTime(clip.startMs)}`,
      `${describeLength(clipLengthMs(clip))} long`,
      `ends at ${describeTime(clipEndMs(clip))}`,
    ];
    if (clip.muted) parts.push("muted");
    if (clip.volumeDb !== 0) parts.push(`${Math.abs(clip.volumeDb)} decibels ${clip.volumeDb < 0 ? "quieter" : "louder"}`);
    if (clip.fadeInMs > 0) parts.push(`fades in over ${describeLength(clip.fadeInMs)}`);
    if (clip.fadeOutMs > 0) parts.push(`fades out over ${describeLength(clip.fadeOutMs)}`);
    if (row !== undefined && row.state !== "ready" && row.state !== "muted") parts.push(row.state);
    return parts.join(", ") + ".";
  }
  return `There is no clip with the id ${clipId}.`;
}

/** What other parts of the editor know about the picture at a moment. Each is optional. */
export interface PreviewContext {
  /** The window that was in front, from the interaction log. */
  readonly window?: string;
  /** What the cursor was over, if known. */
  readonly cursorTarget?: string;
  /** The zoom at that moment, in words. */
  readonly zoom?: string;
  /** The text overlays and callouts showing. */
  readonly overlays?: readonly string[];
  /** The caption showing. */
  readonly caption?: string;
}

function activeClip(track: Track, at: number): Clip | undefined {
  return track.clips.find((c) => c.startMs <= at && at < clipEndMs(c));
}

/**
 * What is on screen and in the sound at a moment, in words (story C4). Read politely when asked for,
 * not announced all the time.
 */
export function describeAt(model: Model, timeMs: number, context: PreviewContext = {}): string {
  const at = wholeMs(timeMs);
  const sentences: string[] = [`At ${describeTime(at)}.`];
  if (model.tracks.length === 0) sentences.push("The edit has no tracks yet.");
  for (const track of model.tracks) {
    const clip = activeClip(track, at);
    const name = `${track.name} track`;
    if (clip !== undefined) {
      const into = describeLength(at - clip.startMs);
      const extras = [
        clip.muted ? "muted" : undefined,
        clip.volumeDb !== 0 ? `${Math.abs(clip.volumeDb)} decibels ${clip.volumeDb < 0 ? "quieter" : "louder"}` : undefined,
      ].filter((x): x is string => x !== undefined);
      sentences.push(`${name}: ${clip.name}, ${into} in${extras.length > 0 ? ", " + extras.join(", ") : ""}.`);
    } else {
      const next = track.clips.find((c) => c.startMs > at);
      sentences.push(
        next === undefined
          ? `${name}: nothing here, and nothing later.`
          : `${name}: nothing here; ${next.name} starts at ${describeTime(next.startMs)}.`,
      );
    }
  }
  if (context.window !== undefined) sentences.push(`Window in front: ${context.window}.`);
  if (context.cursorTarget !== undefined) sentences.push(`The cursor is over ${context.cursorTarget}.`);
  sentences.push(
    context.overlays === undefined || context.overlays.length === 0
      ? "No overlays showing."
      : `Overlays showing: ${context.overlays.join("; ")}.`,
  );
  sentences.push(context.caption === undefined ? "No caption showing." : `Caption: ${context.caption}`);
  sentences.push(context.zoom === undefined ? "Zoom: none." : `Zoom: ${context.zoom}.`);
  return sentences.join(" ");
}


function quote(name: string): string {
  return `"${name}"`;
}

/**
 * The net result of a drag, in words, when exactly one clip changed (the usual drag of a grip or a
 * clip). `undefined` when it cannot be said simply, in which case the last action's own sentence is used.
 */
export function describeChange(before: Model, after: Model): string | undefined {
  const changed: Array<{ was: Clip; now: Clip }> = [];
  for (const track of after.tracks) {
    for (const now of track.clips) {
      const was = before.tracks.find((t) => t.id === track.id)?.clips.find((c) => c.id === now.id);
      if (was === undefined) return undefined;
      if (JSON.stringify(was) !== JSON.stringify(now)) changed.push({ was, now });
    }
  }
  const countBefore = before.tracks.reduce((n, t) => n + t.clips.length, 0);
  const countAfter = after.tracks.reduce((n, t) => n + t.clips.length, 0);
  if (changed.length !== 1 || countBefore !== countAfter) return undefined;
  const { was, now } = changed[0] ?? {};
  if (was === undefined || now === undefined) return undefined;
  const by = (ms: number) => (ms < 0 ? `${describeLength(-ms)} shorter` : `${describeLength(ms)} longer`);
  const startMoved = now.startMs !== was.startMs;
  const endMoved = clipEndMs(now) !== clipEndMs(was);
  if (startMoved && !endMoved) {
    return `Clip ${quote(now.name)} now starts at ${describeTime(now.startMs)}, ${by(clipEndMs(now) - now.startMs - (clipEndMs(was) - was.startMs))}.`;
  }
  if (!startMoved && endMoved) {
    return `Clip ${quote(now.name)} now ends at ${describeTime(clipEndMs(now))}, ${by(clipEndMs(now) - clipEndMs(was))}.`;
  }
  if (startMoved && endMoved && clipLengthMs(now) === clipLengthMs(was)) {
    return `Clip ${quote(now.name)} now starts at ${describeTime(now.startMs)}.`;
  }
  return undefined;
}
