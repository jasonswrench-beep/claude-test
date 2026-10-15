// SPDX-License-Identifier: Apache-2.0
/**
 * Saying what the edit holds, in words: a row for every clip (the list that is the foundation of
 * the timeline, story C1), a sentence for each clip, and a description of what is on screen at any
 * moment (story C4), so the result can be checked without seeing it.
 */

import {
  clipEndMs,
  clipLengthMs,
  findItem,
  findSource,
  itemEndMs,
  modelLengthMs,
  type Clip,
  type Item,
  type Model,
  type Track,
} from "./model.ts";
import { describeMagnification, describePlace } from "./regions.ts";
import { describeLength, describeTime, wholeMs } from "./time.ts";
import { itemKindName, validate } from "./validate.ts";

/** One row of the clip list. */
export interface Row {
  readonly trackId: string;
  readonly trackName: string;
  /** The id of the clip, or of the overlay or zoom (the list holds both). */
  readonly clipId: string;
  readonly name: string;
  /** What the clip holds, such as "Screen" or "Image". */
  readonly type: string;
  readonly startMs: number;
  readonly lengthMs: number;
  readonly endMs: number;
  /** In words: "ready", "muted", "needs attention", "has a warning" or "has a problem". */
  readonly state: string;
}

function stateOf(problems: readonly { severity: string }[], muted: boolean): string {
  return problems.some((p) => p.severity === "error")
    ? "has a problem"
    : problems.some((p) => p.severity === "needs-attention")
      ? "needs attention"
      : problems.some((p) => p.severity === "warning")
        ? "has a warning"
        : muted
          ? "muted"
          : "ready";
}

function itemTypeName(item: Item): string {
  const name = itemKindName(item);
  return name.charAt(0).toUpperCase() + name.slice(1);
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
      const state = stateOf(mine, clip.muted);
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
    for (const item of track.items) {
      rows.push({
        trackId: track.id,
        trackName: track.name,
        clipId: item.id,
        name: item.name,
        type: itemTypeName(item),
        startMs: item.startMs,
        lengthMs: item.durationMs,
        endMs: itemEndMs(item),
        state: stateOf(
          problems.filter((p) => p.itemId === item.id),
          false,
        ),
      });
    }
  }
  return rows;
}

/** A caption for the list: how much it holds. */
export function listCaption(model: Model): string {
  const clips = model.tracks.reduce((n, t) => n + t.clips.length, 0);
  const items = model.tracks.reduce((n, t) => n + t.items.length, 0);
  const tracks = model.tracks.length;
  const extra = items > 0 ? `, ${items} ${items === 1 ? "overlay or zoom" : "overlays and zooms"}` : "";
  return `Tracks and clips: ${clips} ${clips === 1 ? "clip" : "clips"}${extra} on ${tracks} ${tracks === 1 ? "track" : "tracks"}, ${describeLength(modelLengthMs(model))} long.`;
}

function clipSentence(model: Model, track: Track, clip: Clip, state: string | undefined): string {
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
  if (state !== undefined && state !== "ready" && state !== "muted") parts.push(state);
  return parts.join(", ") + ".";
}

/** One clip in a sentence, as it is read when it takes focus. */
export function describeClip(model: Model, clipId: string): string {
  for (const track of model.tracks) {
    const clip = track.clips.find((c) => c.id === clipId);
    if (clip === undefined) continue;
    const row = listRows(model).find((r) => r.clipId === clipId);
    return clipSentence(model, track, clip, row?.state);
  }
  const found = findItem(model, clipId);
  if (found !== undefined) return describeItem(found.item, found.track.name, model);
  return `There is no clip with the id ${clipId}.`;
}

/**
 * A sentence for every clip, overlay and zoom, by id: the same sentences as `describeClip`, worked out in one
 * pass over the edit's checks instead of one pass for each. A long recording cut into hundreds of clips
 * needs this to keep the list quick.
 */
export function describeRows(model: Model): Map<string, string> {
  const problems = validate(model);
  const sentences = new Map<string, string>();
  for (const track of model.tracks) {
    for (const clip of track.clips) {
      const state = stateOf(
        problems.filter((p) => p.clipId === clip.id),
        clip.muted,
      );
      sentences.set(clip.id, clipSentence(model, track, clip, state));
    }
    for (const item of track.items) {
      const state = stateOf(
        problems.filter((p) => p.itemId === item.id),
        false,
      );
      sentences.set(item.id, itemSentence(item, track.name, state));
    }
  }
  return sentences;
}

/** An overlay or zoom in one sentence, as it is read when it takes focus. */
export function describeItem(item: Item, trackName: string, model?: Model): string {
  const state = model === undefined ? undefined : stateOf(validate(model).filter((p) => p.itemId === item.id), false);
  return itemSentence(item, trackName, state);
}

function itemSentence(item: Item, trackName: string, state: string | undefined): string {
  const parts = [
    `${item.name}, ${itemKindName(item)} on the ${trackName} track`,
    `starts at ${describeTime(item.startMs)}`,
    `${describeLength(item.durationMs)} long`,
    `ends at ${describeTime(itemEndMs(item))}`,
  ];
  switch (item.kind) {
    case "text":
      parts.push(`says: ${item.text}`, `in ${describePlace(item.rect)} of the picture`);
      break;
    case "zoom":
      parts.push(`${describeMagnification(item.rect)} on ${describePlace(item.rect)} of the picture`);
      if (item.description !== "") parts.push(item.description);
      break;
    case "shape":
    case "arrow":
      if (item.decorative === true) parts.push("decorative");
      else if (item.altText !== undefined && item.altText !== "") parts.push(item.altText);
      break;
    default:
      parts.push(`over ${describePlace(item.rect)} of the picture`);
  }
  if (state !== undefined && state !== "ready") parts.push(state);
  return parts.join(", ") + ".";
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

  // Overlays and the zoom come from the edit itself; the rest of the editor may add to them.
  const showing: string[] = [];
  let zoom: string | undefined;
  for (const track of model.tracks) {
    for (const item of track.items) {
      if (!(item.startMs <= at && at < itemEndMs(item))) continue;
      if (item.kind === "zoom") {
        zoom = `${describeMagnification(item.rect)} on ${describePlace(item.rect)} of the picture${item.description === "" ? "" : `, showing ${item.description}`}`;
      } else if (item.kind === "text") {
        showing.push(`text "${item.text}"`);
      } else if (item.kind === "blur") {
        showing.push(`a blur over ${describePlace(item.rect)} of the picture`);
      } else if (item.kind === "redact") {
        showing.push(`a redaction over ${describePlace(item.rect)} of the picture`);
      } else {
        showing.push(`${itemKindName(item)}${item.decorative === true || item.altText === undefined || item.altText === "" ? "" : `: ${item.altText}`}`);
      }
    }
  }
  showing.push(...(context.overlays ?? []));
  sentences.push(showing.length === 0 ? "No overlays showing." : `Overlays showing: ${showing.join("; ")}.`);
  sentences.push(context.caption === undefined ? "No caption showing." : `Caption: ${context.caption}`);
  const zoomText = context.zoom ?? zoom;
  sentences.push(zoomText === undefined ? "Zoom: none." : `Zoom: ${zoomText}.`);
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
