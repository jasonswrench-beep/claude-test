// SPDX-License-Identifier: Apache-2.0
/**
 * The edit model: tracks and clips laid out in time.
 *
 * This is plain data. Nothing here draws anything or touches a file; the editor shows it, the
 * operations in `ops.ts` change it (always by making a new model, never by changing the old one,
 * which is what makes undo simple), and the renderer reads it back from `edit.json`.
 *
 * A clip points at a stretch of a recording (or of an imported file) and says where on the timeline
 * it sits. The recording itself is never changed: trimming, splitting and the rest only change
 * these numbers. All times are whole milliseconds.
 */

export const EDIT_FORMAT = "openstage-capture-edit";
export const EDIT_SCHEMA_VERSION = "0.1.0";

/** The shortest a clip may be, in milliseconds. */
export const MIN_CLIP_MS = 100;
/** Imported video clips can be at most this long (story C10). */
export const MAX_IMPORTED_VIDEO_MS = 30_000;
/** Volume of a clip in the mix, in decibels. The same range as the recorder's mix. */
export const MIN_VOLUME_DB = -40;
export const MAX_VOLUME_DB = 20;

/** What a track holds. */
export type TrackKind = "screen" | "webcam" | "audio" | "media";

/** What a source file is. */
export type SourceKind = "screen" | "webcam" | "microphone" | "system-audio" | "image" | "audio" | "video";

/** Where a clip's picture or sound comes from: a file of a recording, or a file from the course's assets. */
export type SourceRef =
  | { readonly kind: "take"; readonly take: number; readonly media: string }
  | { readonly kind: "asset"; readonly asset: string };

/** What the model knows about a source file. */
export interface SourceInfo {
  readonly ref: SourceRef;
  readonly kind: SourceKind;
  /** What to call it in words, such as "Screen of take 1". */
  readonly name: string;
  /** Its natural length. `null` for a still image, which can be shown for any length. */
  readonly durationMs: number | null;
  /**
   * For imported video and audio: whether it has captions, a narration script or a transcript
   * (OpenStage's rules need one; story C10). Not used for anything else.
   */
  readonly hasTextAlternative?: boolean;
}

/** One stretch of a source, placed on a track. */
export interface Clip {
  readonly id: string;
  readonly name: string;
  readonly source: SourceRef;
  /** The part of the source that is used, in the source's own time. */
  readonly sourceStartMs: number;
  readonly sourceEndMs: number;
  /** Where on the timeline the clip begins. It ends `sourceEndMs - sourceStartMs` later. */
  readonly startMs: number;
  readonly muted: boolean;
  /** Louder (positive) or quieter (negative), in decibels. */
  readonly volumeDb: number;
  readonly fadeInMs: number;
  readonly fadeOutMs: number;
  /** For an image: what it shows, in words, or `decorative` instead. */
  readonly altText?: string;
  readonly decorative?: boolean;
}

export interface Track {
  readonly id: string;
  readonly kind: TrackKind;
  readonly name: string;
  /** Always in order of start, never overlapping. */
  readonly clips: readonly Clip[];
}

export interface Model {
  readonly format: typeof EDIT_FORMAT;
  readonly schemaVersion: string;
  /** The number the next track or clip id is made from. */
  readonly nextId: number;
  readonly sources: readonly SourceInfo[];
  readonly tracks: readonly Track[];
}

/** A new, empty edit, knowing which source files it may use. */
export function createModel(sources: readonly SourceInfo[] = []): Model {
  return { format: EDIT_FORMAT, schemaVersion: EDIT_SCHEMA_VERSION, nextId: 1, sources, tracks: [] };
}

/** Which kinds of source each kind of track may hold. */
export const ALLOWED_SOURCES: Readonly<Record<TrackKind, readonly SourceKind[]>> = {
  screen: ["screen"],
  webcam: ["webcam"],
  audio: ["microphone", "system-audio", "audio"],
  media: ["image", "video", "audio"],
};

/** What a kind of track is called in words. */
export function trackKindName(kind: TrackKind): string {
  switch (kind) {
    case "screen":
      return "Screen";
    case "webcam":
      return "Webcam";
    case "audio":
      return "Audio";
    case "media":
      return "Imported media";
  }
}

export function sameSource(a: SourceRef, b: SourceRef): boolean {
  if (a.kind === "take" && b.kind === "take") return a.take === b.take && a.media === b.media;
  if (a.kind === "asset" && b.kind === "asset") return a.asset === b.asset;
  return false;
}

export function findSource(model: Model, ref: SourceRef): SourceInfo | undefined {
  return model.sources.find((s) => sameSource(s.ref, ref));
}

export function clipLengthMs(clip: Clip): number {
  return clip.sourceEndMs - clip.sourceStartMs;
}

export function clipEndMs(clip: Clip): number {
  return clip.startMs + clipLengthMs(clip);
}

export interface ClipLocation {
  readonly track: Track;
  readonly trackIndex: number;
  readonly clip: Clip;
  readonly clipIndex: number;
}

export function findClip(model: Model, clipId: string): ClipLocation | undefined {
  for (let trackIndex = 0; trackIndex < model.tracks.length; trackIndex++) {
    const track = model.tracks[trackIndex];
    if (track === undefined) continue;
    const clipIndex = track.clips.findIndex((c) => c.id === clipId);
    const clip = track.clips[clipIndex];
    if (clip !== undefined) return { track, trackIndex, clip, clipIndex };
  }
  return undefined;
}

export function findTrack(model: Model, trackId: string): Track | undefined {
  return model.tracks.find((t) => t.id === trackId);
}

/** How long the whole edit is: where the last clip ends. */
export function modelLengthMs(model: Model): number {
  let end = 0;
  for (const track of model.tracks) for (const clip of track.clips) end = Math.max(end, clipEndMs(clip));
  return end;
}

/** A new model with one track replaced. */
export function withTrack(model: Model, track: Track): Model {
  return { ...model, tracks: model.tracks.map((t) => (t.id === track.id ? track : t)) };
}

/** A new model with one clip replaced; the track's clips are kept in order of start. */
export function withClip(model: Model, trackId: string, clip: Clip): Model {
  const track = findTrack(model, trackId);
  if (track === undefined) return model;
  const clips = track.clips.map((c) => (c.id === clip.id ? clip : c));
  return withTrack(model, { ...track, clips: sortClips(clips) });
}

export function sortClips(clips: readonly Clip[]): readonly Clip[] {
  return [...clips].sort((a, b) => a.startMs - b.startMs || a.id.localeCompare(b.id));
}

/** Make an id such as "clip-4" and the model that remembers it was used. */
export function takeId(model: Model, prefix: "track" | "clip"): { id: string; model: Model } {
  return { id: `${prefix}-${model.nextId}`, model: { ...model, nextId: model.nextId + 1 } };
}
