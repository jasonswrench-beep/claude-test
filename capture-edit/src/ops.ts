// SPDX-License-Identifier: Apache-2.0
/**
 * The editing actions: add, split, trim, move, delete, join, reorder, and the clip settings.
 *
 * Every action takes a model and gives back either a new model with a sentence for the author to
 * hear ("Clip split at 1 minute 12.4 seconds, now two clips"), or a sentence saying why it cannot be
 * done. Nothing is ever changed in place, and a refused action changes nothing. Each result also
 * carries a short `label` ("split clip") that undo and redo announce.
 *
 * An action that would change nothing (a key pressed at the end of its range) gives back the same
 * model it was given, so the undo history can skip it.
 *
 * Choices made here that could have gone the other way (flagged in `docs/capture-edit-format.md`):
 * trimming a clip leaves a gap rather than moving the clips after it; a split piece keeps its clip's
 * name; deleting asks the author whether to close the gap; reordering keeps the gaps between clips
 * where they were.
 */

import {
  ALLOWED_SOURCES,
  clipEndMs,
  clipLengthMs,
  findClip,
  findSource,
  findTrack,
  MAX_IMPORTED_VIDEO_MS,
  MAX_VOLUME_DB,
  MIN_CLIP_MS,
  MIN_VOLUME_DB,
  sameSource,
  sortClips,
  takeId,
  trackKindName,
  withClip,
  withTrack,
  type Clip,
  type Model,
  type SourceInfo,
  type SourceRef,
  type Track,
  type TrackKind,
} from "./model.ts";
import { describeLength, describeTime, wholeMs } from "./time.ts";
import { validate } from "./validate.ts";

export interface Done {
  readonly ok: true;
  readonly model: Model;
  /** What to tell the author. */
  readonly message: string;
  /** A few words for undo and redo to announce, such as "split clip". Empty when nothing changed. */
  readonly label: string;
  /** The clips (or track) the author's attention should go to afterwards. */
  readonly focus?: readonly string[];
}

export interface Refused {
  readonly ok: false;
  readonly message: string;
}

export type Result = Done | Refused;

function refuse(message: string): Refused {
  return { ok: false, message };
}

function done(model: Model, label: string, message: string, focus?: readonly string[]): Done {
  return { ok: true, model, label, message, ...(focus === undefined ? {} : { focus }) };
}

/** The same model, because nothing needed to change. */
function unchanged(model: Model, message: string): Done {
  return { ok: true, model, label: "", message };
}

function quote(name: string): string {
  return `"${name}"`;
}

function locate(model: Model, clipId: string) {
  const found = findClip(model, clipId);
  return found ?? refuse(`There is no clip with the id ${clipId}.`);
}

function clamp01(clip: Clip): Clip {
  const length = clipLengthMs(clip);
  const fadeIn = Math.min(clip.fadeInMs, length);
  const fadeOut = Math.min(clip.fadeOutMs, length - fadeIn);
  return fadeIn === clip.fadeInMs && fadeOut === clip.fadeOutMs ? clip : { ...clip, fadeInMs: fadeIn, fadeOutMs: fadeOut };
}

// ---- tracks ----

/** Add a track. The first of a kind is named for the kind; later ones are numbered. */
export function addTrack(model: Model, kind: TrackKind, name?: string): Done {
  const taken = takeId(model, "track");
  const same = model.tracks.filter((t) => t.kind === kind).length;
  const chosen = name?.trim() || (same === 0 ? trackKindName(kind) : `${trackKindName(kind)} ${same + 1}`);
  const track: Track = { id: taken.id, kind, name: chosen, clips: [], items: [] };
  return done({ ...taken.model, tracks: [...taken.model.tracks, track] }, "add track", `Added the track ${quote(chosen)}.`, [
    track.id,
  ]);
}

export function renameTrack(model: Model, trackId: string, name: string): Result {
  const track = findTrack(model, trackId);
  if (track === undefined) return refuse(`There is no track with the id ${trackId}.`);
  const next = name.trim();
  if (next === "") return refuse("A track needs a name.");
  if (next === track.name) return unchanged(model, `The track is already called ${quote(next)}.`);
  return done(withTrack(model, { ...track, name: next }), "rename track", `The track ${quote(track.name)} is now called ${quote(next)}.`, [trackId]);
}

// ---- adding clips ----

export interface NewClip {
  readonly source: SourceRef;
  /** Where on the timeline it goes. */
  readonly startMs: number;
  readonly name?: string;
  /** The part of the source to use. Defaults to all of it. */
  readonly sourceStartMs?: number;
  readonly sourceEndMs?: number;
  /** How long to use, from the source's start. For a still image this is how long it is shown (5 seconds if not said). */
  readonly lengthMs?: number;
  readonly altText?: string;
  readonly decorative?: boolean;
}

/** How long a still image is shown when nothing else is said. */
export const DEFAULT_STILL_MS = 5000;

function promptFor(code: string): string {
  switch (code) {
    case "needs-alt-text":
      return "alt text for the image, or mark it as decorative";
    case "needs-text-alternative":
      return "captions, a script or a transcript for it";
    default:
      return code;
  }
}

/** Put a clip on a track. */
export function addClip(model: Model, trackId: string, request: NewClip): Result {
  const track = findTrack(model, trackId);
  if (track === undefined) return refuse(`There is no track with the id ${trackId}.`);
  const source: SourceInfo | undefined = findSource(model, request.source);
  if (source === undefined) return refuse("That recording or file is not part of this edit.");
  if (!ALLOWED_SOURCES[track.kind].includes(source.kind)) {
    return refuse(`${source.name} cannot go on the ${track.name} track. That track takes ${ALLOWED_SOURCES[track.kind].join(", ")}.`);
  }
  const startMs = wholeMs(request.startMs);
  let sourceStart = wholeMs(request.sourceStartMs ?? 0);
  let sourceEnd: number;
  if (request.sourceEndMs !== undefined) sourceEnd = wholeMs(request.sourceEndMs);
  else if (request.lengthMs !== undefined) sourceEnd = sourceStart + wholeMs(request.lengthMs);
  else sourceEnd = source.durationMs ?? sourceStart + DEFAULT_STILL_MS;
  if (source.durationMs === null) {
    // A still has no time of its own: it is shown from 0 for as long as asked.
    sourceEnd = sourceEnd - sourceStart;
    sourceStart = 0;
  }
  if (sourceEnd - sourceStart < MIN_CLIP_MS) {
    return refuse(`A clip must be at least ${describeLength(MIN_CLIP_MS)} long.`);
  }
  if (source.durationMs !== null && sourceEnd > source.durationMs) {
    return refuse(`${source.name} is only ${describeLength(source.durationMs)} long, so a clip cannot use up to ${describeTime(sourceEnd)} of it.`);
  }
  if (source.ref.kind === "asset" && source.kind === "video" && sourceEnd - sourceStart > MAX_IMPORTED_VIDEO_MS) {
    return refuse(
      `That video would be ${describeLength(sourceEnd - sourceStart)} long. Imported video clips can be at most ${describeLength(MAX_IMPORTED_VIDEO_MS)}. Use a shorter part of it.`,
    );
  }
  const end = startMs + (sourceEnd - sourceStart);
  const clash = track.clips.find((c) => startMs < clipEndMs(c) && c.startMs < end);
  if (clash !== undefined) {
    return refuse(
      `That would overlap the clip ${quote(clash.name)}, which runs from ${describeTime(clash.startMs)} to ${describeTime(clipEndMs(clash))}. Choose a free place on the ${track.name} track.`,
    );
  }
  const taken = takeId(model, "clip");
  const clip: Clip = {
    id: taken.id,
    name: request.name?.trim() || source.name,
    source: request.source,
    sourceStartMs: sourceStart,
    sourceEndMs: sourceEnd,
    startMs,
    muted: false,
    volumeDb: 0,
    fadeInMs: 0,
    fadeOutMs: 0,
    ...(request.altText === undefined ? {} : { altText: request.altText }),
    ...(request.decorative === undefined ? {} : { decorative: request.decorative }),
  };
  const next = withTrack(taken.model, { ...track, clips: sortClips([...track.clips, clip]) });
  const needs = validate(next)
    .filter((p) => p.clipId === clip.id && p.severity === "needs-attention")
    .map((p) => promptFor(p.code));
  return done(
    next,
    "add clip",
    `Added the clip ${quote(clip.name)} to the ${track.name} track at ${describeTime(startMs)}, ${describeLength(end - startMs)} long.` +
      (needs.length > 0 ? ` Still needed: ${needs.join("; ")}.` : ""),
    [clip.id],
  );
}

// ---- splitting ----

/**
 * Split at a time. With a track, splits the clip on that track at that time. Without one, splits every
 * clip that the time falls inside, on every track, so picture and sound are cut together.
 */
export function splitAt(model: Model, timeMs: number, trackId?: string): Result {
  const at = wholeMs(timeMs);
  const tracks = trackId === undefined ? model.tracks : model.tracks.filter((t) => t.id === trackId);
  if (trackId !== undefined && tracks.length === 0) return refuse(`There is no track with the id ${trackId}.`);

  let next = model;
  const focus: string[] = [];
  let count = 0;
  let tooShort: string | undefined;
  for (const track of tracks) {
    const clip = track.clips.find((c) => c.startMs < at && at < clipEndMs(c));
    if (clip === undefined) continue;
    const left = at - clip.startMs;
    const right = clipEndMs(clip) - at;
    if (left < MIN_CLIP_MS || right < MIN_CLIP_MS) {
      tooShort = `Cannot split ${quote(clip.name)} at ${describeTime(at)}: one piece would be shorter than ${describeLength(MIN_CLIP_MS)}.`;
      continue;
    }
    const taken = takeId(next, "clip");
    const sourceAt = clip.sourceStartMs + left;
    const first = clamp01({ ...clip, sourceEndMs: sourceAt, fadeOutMs: 0 });
    const second = clamp01({ ...clip, id: taken.id, sourceStartMs: sourceAt, startMs: at, fadeInMs: 0 });
    const live = findTrack(taken.model, track.id) ?? track;
    next = withTrack(taken.model, { ...live, clips: sortClips([...live.clips.filter((c) => c.id !== clip.id), first, second]) });
    focus.push(first.id, second.id);
    count += 1;
  }
  if (count === 0) {
    return refuse(
      tooShort ??
        (trackId === undefined
          ? `Nothing to split at ${describeTime(at)}: no clip has this time inside it.`
          : `Nothing to split at ${describeTime(at)}: the ${tracks[0]?.name ?? ""} track has no clip with this time inside it.`),
    );
  }
  return done(
    next,
    count === 1 ? "split clip" : "split clips",
    count === 1
      ? `Clip split at ${describeTime(at)}, now two clips.`
      : `Split ${count} clips at ${describeTime(at)}; each is now two clips.`,
    focus,
  );
}

// ---- trimming and moving ----

function neighbours(track: Track, index: number): { previous: Clip | undefined; next: Clip | undefined } {
  return { previous: track.clips[index - 1], next: track.clips[index + 1] };
}

/** How much longer or shorter, in words. */
function changeBy(deltaMs: number): string {
  return deltaMs < 0 ? `${describeLength(-deltaMs)} shorter` : `${describeLength(deltaMs)} longer`;
}

/**
 * Move a clip's start (the start grip): the clip's end stays where it is, and its length changes.
 * Going later cuts off the beginning; going earlier shows more of the recording, if there is more.
 * The clips around it do not move.
 */
export function trimStart(model: Model, clipId: string, toMs: number): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const { track, clip, clipIndex } = found;
  const target = wholeMs(toMs);
  const source = findSource(model, clip.source);
  const still = source?.durationMs === null;
  const { previous } = neighbours(track, clipIndex);
  const earliestBySource = still ? 0 : clip.startMs - clip.sourceStartMs;
  const earliest = Math.max(previous === undefined ? 0 : clipEndMs(previous), earliestBySource);
  const latest = clipEndMs(clip) - MIN_CLIP_MS;
  if (target === clip.startMs) return unchanged(model, `The clip ${quote(clip.name)} already starts at ${describeTime(target)}.`);
  if (target < earliest) {
    const why =
      previous !== undefined && clipEndMs(previous) >= earliestBySource
        ? `the clip ${quote(previous.name)} before it ends at ${describeTime(clipEndMs(previous))}`
        : "the recording has nothing before that";
    return refuse(`Cannot start ${quote(clip.name)} earlier than ${describeTime(earliest)}: ${why}.`);
  }
  if (target > latest) {
    return refuse(`Cannot start ${quote(clip.name)} later than ${describeTime(latest)}: it must stay at least ${describeLength(MIN_CLIP_MS)} long.`);
  }
  const delta = target - clip.startMs;
  const moved: Clip = still
    ? { ...clip, startMs: target, sourceEndMs: clip.sourceEndMs - delta }
    : { ...clip, startMs: target, sourceStartMs: clip.sourceStartMs + delta };
  return done(
    withClip(model, track.id, clamp01(moved)),
    "trim clip start",
    `Clip ${quote(clip.name)} now starts at ${describeTime(target)}, ${changeBy(-delta)}.`,
    [clip.id],
  );
}

/** Move a clip's end (the end grip): the clip's start stays where it is, and its length changes. */
export function trimEnd(model: Model, clipId: string, toMs: number): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const { track, clip, clipIndex } = found;
  const target = wholeMs(toMs);
  const source = findSource(model, clip.source);
  const { next } = neighbours(track, clipIndex);
  const latestBySource =
    source === undefined || source.durationMs === null ? Number.POSITIVE_INFINITY : clip.startMs + (source.durationMs - clip.sourceStartMs);
  const latest = Math.min(next === undefined ? Number.POSITIVE_INFINITY : next.startMs, latestBySource);
  const earliest = clip.startMs + MIN_CLIP_MS;
  const end = clipEndMs(clip);
  if (target === end) return unchanged(model, `The clip ${quote(clip.name)} already ends at ${describeTime(target)}.`);
  if (target < earliest) {
    return refuse(`Cannot end ${quote(clip.name)} earlier than ${describeTime(earliest)}: it must stay at least ${describeLength(MIN_CLIP_MS)} long.`);
  }
  if (target > latest) {
    const why =
      next !== undefined && next.startMs <= latestBySource
        ? `the clip ${quote(next.name)} after it starts at ${describeTime(next.startMs)}`
        : "the recording ends there";
    return refuse(`Cannot end ${quote(clip.name)} later than ${describeTime(latest)}: ${why}.`);
  }
  const delta = target - end;
  return done(
    withClip(model, track.id, clamp01({ ...clip, sourceEndMs: clip.sourceEndMs + delta })),
    "trim clip end",
    `Clip ${quote(clip.name)} now ends at ${describeTime(target)}, ${changeBy(delta)}.`,
    [clip.id],
  );
}

/** Set a clip's length exactly (a field in the clip list). The start stays; the end moves. */
export function setClipLength(model: Model, clipId: string, lengthMs: number): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const result = trimEnd(model, clipId, found.clip.startMs + wholeMs(lengthMs));
  if (!result.ok || result.model === model) {
    return result.ok ? unchanged(model, `The clip ${quote(found.clip.name)} is already ${describeLength(clipLengthMs(found.clip))} long.`) : result;
  }
  return { ...result, label: "set clip length", message: `Clip ${quote(found.clip.name)} is now ${describeLength(wholeMs(lengthMs))} long, ending at ${describeTime(found.clip.startMs + wholeMs(lengthMs))}.` };
}

/** Move a whole clip so it starts at a time (a field in the clip list, or a drag). It may not run into its neighbours. */
export function setClipStart(model: Model, clipId: string, toMs: number): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const { track, clip, clipIndex } = found;
  const target = wholeMs(toMs);
  const length = clipLengthMs(clip);
  const { previous, next } = neighbours(track, clipIndex);
  const earliest = previous === undefined ? 0 : clipEndMs(previous);
  const latest = next === undefined ? Number.POSITIVE_INFINITY : next.startMs - length;
  if (target === clip.startMs) return unchanged(model, `The clip ${quote(clip.name)} already starts at ${describeTime(target)}.`);
  if (target < earliest) {
    return refuse(
      previous === undefined
        ? `Cannot start ${quote(clip.name)} before the beginning.`
        : `Cannot start ${quote(clip.name)} earlier than ${describeTime(earliest)}: it would run into the clip ${quote(previous.name)}, which ends then.`,
    );
  }
  if (target > latest) {
    return refuse(`Cannot start ${quote(clip.name)} later than ${describeTime(latest)}: it would run into the clip ${quote(next?.name ?? "")}, which starts at ${describeTime(next?.startMs ?? 0)}.`);
  }
  return done(withClip(model, track.id, { ...clip, startMs: target }), "move clip", `Clip ${quote(clip.name)} now starts at ${describeTime(target)}.`, [clip.id]);
}

// ---- deleting ----

/**
 * Delete a clip. `closeGap` is the author's choice, so it has no default: false leaves an empty
 * stretch where the clip was; true moves the later clips on the same track earlier by the clip's length.
 */
export function deleteClip(model: Model, clipId: string, closeGap: boolean): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const { track, clip } = found;
  const length = clipLengthMs(clip);
  const rest = track.clips.filter((c) => c.id !== clip.id);
  const later = rest.filter((c) => c.startMs >= clip.startMs);
  const clips = closeGap ? rest.map((c) => (c.startMs >= clip.startMs ? { ...c, startMs: c.startMs - length } : c)) : rest;
  const message = closeGap
    ? later.length === 0
      ? `Clip ${quote(clip.name)} deleted. Nothing came after it.`
      : `Clip ${quote(clip.name)} deleted, and the ${later.length === 1 ? "clip" : `${later.length} clips`} after it moved ${describeLength(length)} earlier to close the gap.`
    : `Clip ${quote(clip.name)} deleted, leaving a gap of ${describeLength(length)}.`;
  return done(withTrack(model, { ...track, clips }), "delete clip", message, [track.id]);
}

/**
 * Remove a stretch of time, from `fromMs` to `toMs`, from every track (or the tracks named), so a
 * mistake is cut out of the picture and the sound together. Clips that cross an edge are cut there.
 * `closeGap` is the author's choice, so it has no default: true moves everything after the stretch
 * earlier; false leaves it empty.
 */
export function deleteRange(model: Model, fromMs: number, toMs: number, closeGap: boolean, trackIds?: readonly string[]): Result {
  const from = wholeMs(fromMs);
  const to = wholeMs(toMs);
  if (to <= from) return refuse(`The end, ${describeTime(to)}, must be after the start, ${describeTime(from)}.`);
  const targets = trackIds === undefined ? model.tracks : model.tracks.filter((t) => trackIds.includes(t.id));
  if (targets.length === 0) return refuse("There is no track to remove time from.");
  const removed = to - from;

  let next = model;
  let touched = 0;
  const touchedTracks = new Set<string>();
  for (const track of targets) {
    const pieces: Clip[] = [];
    for (const clip of track.clips) {
      const end = clipEndMs(clip);
      if (end <= from || clip.startMs >= to) {
        pieces.push(clip);
        continue;
      }
      touched += 1;
      touchedTracks.add(track.id);
      const keepsLeft = clip.startMs < from;
      const keepsRight = end > to;
      const leftLength = from - clip.startMs;
      const rightLength = end - to;
      if ((keepsLeft && leftLength < MIN_CLIP_MS) || (keepsRight && rightLength < MIN_CLIP_MS)) {
        return refuse(
          `Cannot remove from ${describeTime(from)} to ${describeTime(to)}: it would leave a piece of ${quote(clip.name)} shorter than ${describeLength(MIN_CLIP_MS)}. Move an edge a little.`,
        );
      }
      if (keepsLeft) {
        pieces.push(clamp01({ ...clip, sourceEndMs: clip.sourceStartMs + leftLength, ...(keepsRight ? { fadeOutMs: 0 } : {}) }));
      }
      if (keepsRight) {
        const taken = takeId(next, "clip");
        next = taken.model;
        const sourceAt = clip.sourceStartMs + (to - clip.startMs);
        const right: Clip = {
          ...clip,
          id: keepsLeft ? taken.id : clip.id,
          sourceStartMs: sourceAt,
          startMs: to,
          ...(keepsLeft ? { fadeInMs: 0 } : {}),
        };
        pieces.push(clamp01(right));
      }
    }
    const shifted = closeGap ? pieces.map((c) => (c.startMs >= to ? { ...c, startMs: c.startMs - removed } : c)) : pieces;
    next = withTrack(next, { ...(findTrack(next, track.id) ?? track), clips: sortClips(shifted) });
  }
  if (touched === 0) return refuse(`Nothing to remove between ${describeTime(from)} and ${describeTime(to)}: no clip is there.`);
  const trackWords = touchedTracks.size === 1 ? "1 track" : `${touchedTracks.size} tracks`;
  return done(
    next,
    "remove time",
    `Removed ${describeLength(removed)} from ${describeTime(from)} to ${describeTime(to)} on ${trackWords}` +
      (closeGap ? ", and moved everything after it earlier to close the gap." : ", leaving a gap."),
    [...touchedTracks],
  );
}

// ---- joining and reordering ----

/** Join a clip with the one after it, when they are the same recording and continue one into the other. */
export function joinWithNext(model: Model, clipId: string): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const { track, clip, clipIndex } = found;
  const second = track.clips[clipIndex + 1];
  if (second === undefined) return refuse(`Cannot join ${quote(clip.name)} with the next clip: it is the last clip on the ${track.name} track.`);
  if (!sameSource(clip.source, second.source)) {
    return refuse(`Cannot join ${quote(clip.name)} and ${quote(second.name)}: they come from different recordings or files.`);
  }
  if (clipEndMs(clip) !== second.startMs) {
    return refuse(
      `Cannot join ${quote(clip.name)} and ${quote(second.name)}: there is a gap of ${describeLength(second.startMs - clipEndMs(clip))} between them. Close the gap first.`,
    );
  }
  const source = findSource(model, clip.source);
  const still = source?.durationMs === null;
  if (!still && clip.sourceEndMs !== second.sourceStartMs) {
    return refuse(
      `Cannot join ${quote(clip.name)} and ${quote(second.name)}: part of the recording between them was cut out, so they do not continue one into the other.`,
    );
  }
  const joined: Clip = clamp01({
    ...clip,
    sourceEndMs: still ? clip.sourceEndMs + clipLengthMs(second) : second.sourceEndMs,
    fadeOutMs: second.fadeOutMs,
  });
  const clips = sortClips([...track.clips.filter((c) => c.id !== clip.id && c.id !== second.id), joined]);
  return done(
    withTrack(model, { ...track, clips }),
    "join clips",
    `Joined 2 clips into one clip of ${describeLength(clipLengthMs(joined))}.`,
    [joined.id],
  );
}

/**
 * Change a clip's place in the order of its track. `toIndex` is 0 for first. The clips are laid out
 * again in the new order, one after another, with the gaps between them kept in the same places, so
 * the track takes up the same stretch of time.
 */
export function reorderClip(model: Model, clipId: string, toIndex: number): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const { track, clip, clipIndex } = found;
  const count = track.clips.length;
  const index = Math.min(Math.max(Math.trunc(toIndex), 0), count - 1);
  if (index === clipIndex) return unchanged(model, `The clip ${quote(clip.name)} is already number ${index + 1} of ${count}.`);
  const gaps: number[] = [];
  for (let i = 0; i + 1 < count; i++) {
    const a = track.clips[i];
    const b = track.clips[i + 1];
    if (a !== undefined && b !== undefined) gaps.push(b.startMs - clipEndMs(a));
  }
  const order = track.clips.filter((c) => c.id !== clip.id);
  order.splice(index, 0, clip);
  let cursor = track.clips[0]?.startMs ?? 0;
  const clips = order.map((c, i) => {
    const placed = { ...c, startMs: cursor };
    cursor += clipLengthMs(c) + (gaps[i] ?? 0);
    return placed;
  });
  return done(
    withTrack(model, { ...track, clips }),
    "reorder clips",
    `Clip ${quote(clip.name)} moved to number ${index + 1} of ${count}; the clips on the ${track.name} track were laid out again in the new order.`,
    [clip.id],
  );
}

// ---- clip settings ----

export function renameClip(model: Model, clipId: string, name: string): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const next = name.trim();
  if (next === "") return refuse("A clip needs a name.");
  if (next === found.clip.name) return unchanged(model, `The clip is already called ${quote(next)}.`);
  return done(withClip(model, found.track.id, { ...found.clip, name: next }), "rename clip", `Clip ${quote(found.clip.name)} is now called ${quote(next)}.`, [clipId]);
}

export function setMuted(model: Model, clipId: string, muted: boolean): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  if (found.clip.muted === muted) return unchanged(model, `Clip ${quote(found.clip.name)} is already ${muted ? "muted" : "not muted"}.`);
  return done(withClip(model, found.track.id, { ...found.clip, muted }), muted ? "mute clip" : "unmute clip", `Clip ${quote(found.clip.name)} is now ${muted ? "muted" : "not muted"}.`, [clipId]);
}

/** Louder (positive) or quieter (negative), in whole decibels. About 6 decibels is double or half the volume. */
export function setVolume(model: Model, clipId: string, volumeDb: number): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  if (!Number.isInteger(volumeDb) || volumeDb < MIN_VOLUME_DB || volumeDb > MAX_VOLUME_DB) {
    return refuse(`The volume can be changed by a whole number of decibels from ${MIN_VOLUME_DB} to ${MAX_VOLUME_DB}; 0 leaves it as recorded.`);
  }
  if (found.clip.volumeDb === volumeDb) return unchanged(model, `Clip ${quote(found.clip.name)} is already at ${volumeDb} decibels.`);
  const how = volumeDb === 0 ? "back to as recorded" : `${Math.abs(volumeDb)} decibels ${volumeDb < 0 ? "quieter" : "louder"} than recorded`;
  return done(withClip(model, found.track.id, { ...found.clip, volumeDb }), "set volume", `Clip ${quote(found.clip.name)} is now ${how}.`, [clipId]);
}

/** Fade the sound in at the start and out at the end. Together they cannot be longer than the clip. */
export function setFades(model: Model, clipId: string, fadeInMs: number, fadeOutMs: number): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const fadeIn = wholeMs(fadeInMs);
  const fadeOut = wholeMs(fadeOutMs);
  const length = clipLengthMs(found.clip);
  if (fadeIn + fadeOut > length) {
    return refuse(`The fades add up to ${describeLength(fadeIn + fadeOut)}, but the clip is only ${describeLength(length)} long.`);
  }
  if (fadeIn === found.clip.fadeInMs && fadeOut === found.clip.fadeOutMs) return unchanged(model, "The fades are already like that.");
  return done(
    withClip(model, found.track.id, { ...found.clip, fadeInMs: fadeIn, fadeOutMs: fadeOut }),
    "set fades",
    `Clip ${quote(found.clip.name)} fades in over ${describeLength(fadeIn)} and out over ${describeLength(fadeOut)}.`,
    [clipId],
  );
}

/** What an image shows, in words, or that it is decorative. Only images have it. */
export function setImageText(model: Model, clipId: string, text: { altText?: string; decorative?: boolean }): Result {
  const found = locate(model, clipId);
  if ("ok" in found) return found;
  const source = findSource(model, found.clip.source);
  if (source?.kind !== "image") return refuse(`Clip ${quote(found.clip.name)} is not an image, so it has no alt text.`);
  const altText = text.altText === undefined ? found.clip.altText : text.altText.trim();
  const decorative = text.decorative ?? found.clip.decorative;
  const { altText: _oldAlt, decorative: _oldDecorative, ...bare } = found.clip;
  const base: Clip = {
    ...bare,
    ...(altText === undefined || altText === "" ? {} : { altText }),
    ...(decorative === undefined ? {} : { decorative }),
  };
  return done(
    withClip(model, found.track.id, base),
    "set alt text",
    decorative === true
      ? `Image ${quote(found.clip.name)} is marked as decorative.`
      : altText !== undefined && altText !== ""
        ? `Image ${quote(found.clip.name)} now has alt text: ${altText}`
        : `Image ${quote(found.clip.name)} still needs alt text, or to be marked as decorative.`,
    [clipId],
  );
}
