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
export const EDIT_SCHEMA_VERSION = "0.2.0";

/** The shortest a clip may be, in milliseconds. */
export const MIN_CLIP_MS = 100;
/** Imported video clips can be at most this long (story C10). */
export const MAX_IMPORTED_VIDEO_MS = 30_000;
/** Volume of a clip in the mix, in decibels. The same range as the recorder's mix. */
export const MIN_VOLUME_DB = -40;
export const MAX_VOLUME_DB = 20;

/** What a track holds. Screen, webcam, audio and media tracks hold clips; overlay and zoom tracks hold items. */
export type TrackKind = "screen" | "webcam" | "audio" | "media" | "overlay" | "zoom";

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

/** The whole width or height of the picture, in the units rectangles are measured in: ten-thousandths. */
export const FULL = 10_000;

/** A rectangle on the picture, in ten-thousandths of its width and height from its top left corner. */
export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** A point on the picture, in the same units. */
export interface Point {
  readonly x: number;
  readonly y: number;
}

/** What a text overlay, shape or zoom looks like, wherever it is. Colours are "#RRGGBB". */
interface ItemBase {
  readonly id: string;
  readonly name: string;
  /** Where on the timeline it begins, and how long it lasts. */
  readonly startMs: number;
  readonly durationMs: number;
}

/** Words on the picture: a title, a label, a lower third, a callout (story C7). */
export interface TextItem extends ItemBase {
  readonly kind: "text";
  readonly text: string;
  readonly rect: Rect;
  /** The height of a line of text, in ten-thousandths of the picture's height. */
  readonly fontSizeBp: number;
  readonly color: string;
  /** The colour behind the text, or null for none. */
  readonly background: string | null;
  readonly bold: boolean;
  readonly align: "left" | "center" | "right";
}

/** A rectangle or ellipse that draws the eye (story C8). */
export interface ShapeItem extends ItemBase {
  readonly kind: "shape";
  readonly shape: "rectangle" | "ellipse";
  readonly rect: Rect;
  readonly outline: string;
  readonly fill: string | null;
  /** Line thickness, in ten-thousandths of the picture's height. */
  readonly lineWidthBp: number;
  readonly altText?: string;
  readonly decorative?: boolean;
}

/** An arrow from one point to another. */
export interface ArrowItem extends ItemBase {
  readonly kind: "arrow";
  readonly from: Point;
  readonly to: Point;
  readonly color: string;
  readonly lineWidthBp: number;
  readonly altText?: string;
  readonly decorative?: boolean;
}

/** A region made unreadable by blurring. */
export interface BlurItem extends ItemBase {
  readonly kind: "blur";
  readonly rect: Rect;
}

/** A region covered by a solid colour. */
export interface RedactItem extends ItemBase {
  readonly kind: "redact";
  readonly rect: Rect;
  readonly color: string;
}

/** A moment of zooming into part of the picture (story C5). */
export interface ZoomItem extends ItemBase {
  readonly kind: "zoom";
  /** The part of the picture that fills the frame while zoomed. */
  readonly rect: Rect;
  /** How long the move in, and the move out, each take. */
  readonly transitionMs: number;
  /** What the zoom shows, in words, so it is not only a picture (stories D4, D5). */
  readonly description: string;
}

export type Item = TextItem | ShapeItem | ArrowItem | BlurItem | RedactItem | ZoomItem;
export type ItemKind = Item["kind"];

export interface Track {
  readonly id: string;
  readonly kind: TrackKind;
  readonly name: string;
  /** For screen, webcam, audio and media tracks. Always in order of start, never overlapping. */
  readonly clips: readonly Clip[];
  /** For overlay and zoom tracks. In order of start. Overlays may overlap; zooms may not. */
  readonly items: readonly Item[];
}

/** Which kinds of item each kind of track holds. */
export const ALLOWED_ITEMS: Readonly<Record<TrackKind, readonly ItemKind[]>> = {
  screen: [],
  webcam: [],
  audio: [],
  media: [],
  overlay: ["text", "shape", "arrow", "blur", "redact"],
  zoom: ["zoom"],
};

export function itemEndMs(item: Item): number {
  return item.startMs + item.durationMs;
}

export function sortItems(items: readonly Item[]): readonly Item[] {
  return [...items].sort((a, b) => a.startMs - b.startMs || a.id.localeCompare(b.id));
}

export interface ItemLocation {
  readonly track: Track;
  readonly item: Item;
}

export function findItem(model: Model, itemId: string): ItemLocation | undefined {
  for (const track of model.tracks) {
    const item = track.items.find((i) => i.id === itemId);
    if (item !== undefined) return { track, item };
  }
  return undefined;
}

/** A new model with one item replaced; the track's items are kept in order of start. */
export function withItem(model: Model, trackId: string, item: Item): Model {
  const track = findTrack(model, trackId);
  if (track === undefined) return model;
  return withTrack(model, { ...track, items: sortItems(track.items.map((i) => (i.id === item.id ? item : i))) });
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
  overlay: [],
  zoom: [],
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
    case "overlay":
      return "Overlays";
    case "zoom":
      return "Zoom";
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

/** How long the whole edit is: where the last clip ends. Overlays and zooms do not make it longer. */
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
export function takeId(model: Model, prefix: "track" | "clip" | "item"): { id: string; model: Model } {
  return { id: `${prefix}-${model.nextId}`, model: { ...model, nextId: model.nextId + 1 } };
}
