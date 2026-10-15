// SPDX-License-Identifier: Apache-2.0
/**
 * Saving an edit as `edit.json` and reading it back.
 *
 * The text is written in one fixed order, with two-space indents and a closing new line, so saving
 * the same edit twice gives the same bytes, and a saved, closed and reopened edit is unchanged
 * (story B4). Reading is strict: a file that is not an edit, or has a version this program does not
 * know, is refused with a plain message instead of being half understood.
 */

import {
  EDIT_FORMAT,
  EDIT_SCHEMA_VERSION,
  sortClips,
  sortItems,
  type Clip,
  type Item,
  type Model,
  type Point,
  type Rect,
  type SourceInfo,
  type SourceRef,
  type Track,
  type TrackKind,
} from "./model.ts";
import { errorsOf, validate, type Problem } from "./validate.ts";

function sourceRefJson(ref: SourceRef): Record<string, unknown> {
  return ref.kind === "take"
    ? { kind: "take", take: ref.take, media: ref.media }
    : { kind: "asset", asset: ref.asset };
}

function clipJson(c: Clip): Record<string, unknown> {
  const out: Record<string, unknown> = {
    id: c.id,
    name: c.name,
    source: sourceRefJson(c.source),
    sourceStartMs: c.sourceStartMs,
    sourceEndMs: c.sourceEndMs,
    startMs: c.startMs,
    muted: c.muted,
    volumeDb: c.volumeDb,
    fadeInMs: c.fadeInMs,
    fadeOutMs: c.fadeOutMs,
  };
  if (c.placement !== undefined) out["placement"] = rectJson(c.placement);
  if (c.altText !== undefined) out["altText"] = c.altText;
  if (c.decorative !== undefined) out["decorative"] = c.decorative;
  return out;
}

function rectJson(r: Rect): Record<string, unknown> {
  return { x: r.x, y: r.y, width: r.width, height: r.height };
}

function pointJson(p: Point): Record<string, unknown> {
  return { x: p.x, y: p.y };
}

function itemJson(i: Item): Record<string, unknown> {
  const head = { id: i.id, kind: i.kind, name: i.name, startMs: i.startMs, durationMs: i.durationMs };
  const alt = (x: { altText?: string; decorative?: boolean }): Record<string, unknown> => ({
    ...(x.altText === undefined ? {} : { altText: x.altText }),
    ...(x.decorative === undefined ? {} : { decorative: x.decorative }),
  });
  switch (i.kind) {
    case "text":
      return {
        ...head,
        text: i.text,
        rect: rectJson(i.rect),
        fontSizeBp: i.fontSizeBp,
        color: i.color,
        background: i.background,
        bold: i.bold,
        align: i.align,
      };
    case "shape":
      return {
        ...head,
        shape: i.shape,
        rect: rectJson(i.rect),
        outline: i.outline,
        fill: i.fill,
        lineWidthBp: i.lineWidthBp,
        ...alt(i),
      };
    case "arrow":
      return { ...head, from: pointJson(i.from), to: pointJson(i.to), color: i.color, lineWidthBp: i.lineWidthBp, ...alt(i) };
    case "blur":
      return { ...head, rect: rectJson(i.rect) };
    case "redact":
      return { ...head, rect: rectJson(i.rect), color: i.color };
    case "zoom":
      return { ...head, rect: rectJson(i.rect), transitionMs: i.transitionMs, description: i.description };
  }
}

function sourceJson(s: SourceInfo): Record<string, unknown> {
  const out: Record<string, unknown> = { ref: sourceRefJson(s.ref), kind: s.kind, name: s.name, durationMs: s.durationMs };
  if (s.hasTextAlternative !== undefined) out["hasTextAlternative"] = s.hasTextAlternative;
  return out;
}

/** The edit as the text of `edit.json`. */
export function serialize(model: Model): string {
  const doc = {
    format: model.format,
    schemaVersion: model.schemaVersion,
    nextId: model.nextId,
    sources: model.sources.map(sourceJson),
    tracks: model.tracks.map((t) => ({
      id: t.id,
      kind: t.kind,
      name: t.name,
      clips: t.clips.map(clipJson),
      items: t.items.map(itemJson),
    })),
  };
  return JSON.stringify(doc, null, 2) + "\n";
}

export type ParseResult =
  | { ok: true; model: Model; problems: Problem[] }
  | { ok: false; message: string };

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

const SOURCE_KINDS = ["screen", "webcam", "microphone", "system-audio", "image", "audio", "video"] as const;
const TRACK_KINDS: readonly TrackKind[] = ["screen", "webcam", "audio", "media", "overlay", "zoom"];

function readRef(v: unknown, what: string): SourceRef {
  if (!isObject(v)) throw new Error(`${what} has no source.`);
  if (v["kind"] === "take" && typeof v["take"] === "number" && typeof v["media"] === "string") {
    return { kind: "take", take: v["take"], media: v["media"] };
  }
  if (v["kind"] === "asset" && typeof v["asset"] === "string") return { kind: "asset", asset: v["asset"] };
  throw new Error(`${what} has a source that is neither a take's file nor an asset.`);
}

function num(v: unknown, what: string): number {
  if (typeof v !== "number") throw new Error(`${what} is missing or is not a number.`);
  return v;
}

function str(v: unknown, what: string): string {
  if (typeof v !== "string") throw new Error(`${what} is missing or is not text.`);
  return v;
}

function readClip(v: unknown, where: string): Clip {
  if (!isObject(v)) throw new Error(`${where} has a clip that is not an object.`);
  const id = str(v["id"], `${where}: a clip's id`);
  const w = `The clip ${id}`;
  const base = {
    id,
    name: str(v["name"], `${w}: its name`),
    source: readRef(v["source"], w),
    sourceStartMs: num(v["sourceStartMs"], `${w}: sourceStartMs`),
    sourceEndMs: num(v["sourceEndMs"], `${w}: sourceEndMs`),
    startMs: num(v["startMs"], `${w}: startMs`),
    muted: v["muted"] === true,
    volumeDb: num(v["volumeDb"], `${w}: volumeDb`),
    fadeInMs: num(v["fadeInMs"], `${w}: fadeInMs`),
    fadeOutMs: num(v["fadeOutMs"], `${w}: fadeOutMs`),
  };
  return {
    ...base,
    // Version 0.3 added a place on the frame; an older file has none, which is how it was drawn before.
    ...(isObject(v["placement"]) ? { placement: readRect(v["placement"], w) } : {}),
    ...(typeof v["altText"] === "string" ? { altText: v["altText"] } : {}),
    ...(typeof v["decorative"] === "boolean" ? { decorative: v["decorative"] } : {}),
  };
}

function readRect(v: unknown, what: string): Rect {
  if (!isObject(v)) throw new Error(`${what} has no rectangle.`);
  return { x: num(v["x"], `${what}: x`), y: num(v["y"], `${what}: y`), width: num(v["width"], `${what}: width`), height: num(v["height"], `${what}: height`) };
}

function readPoint(v: unknown, what: string): Point {
  if (!isObject(v)) throw new Error(`${what} has no point.`);
  return { x: num(v["x"], `${what}: x`), y: num(v["y"], `${what}: y`) };
}

function readItem(v: unknown, where: string): Item {
  if (!isObject(v)) throw new Error(`${where} has an item that is not an object.`);
  const id = str(v["id"], `${where}: an item's id`);
  const w = `The item ${id}`;
  const head = {
    id,
    name: str(v["name"], `${w}: its name`),
    startMs: num(v["startMs"], `${w}: startMs`),
    durationMs: num(v["durationMs"], `${w}: durationMs`),
  };
  const alt = {
    ...(typeof v["altText"] === "string" ? { altText: v["altText"] } : {}),
    ...(typeof v["decorative"] === "boolean" ? { decorative: v["decorative"] } : {}),
  };
  switch (v["kind"]) {
    case "text": {
      const background = v["background"] === null ? null : str(v["background"], `${w}: background`);
      const align = v["align"];
      if (align !== "left" && align !== "center" && align !== "right") throw new Error(`${w}: its alignment is not left, center or right.`);
      return {
        ...head,
        kind: "text",
        text: str(v["text"], `${w}: its text`),
        rect: readRect(v["rect"], w),
        fontSizeBp: num(v["fontSizeBp"], `${w}: fontSizeBp`),
        color: str(v["color"], `${w}: its colour`),
        background,
        bold: v["bold"] === true,
        align,
      };
    }
    case "shape": {
      const shape = v["shape"];
      if (shape !== "rectangle" && shape !== "ellipse") throw new Error(`${w}: its shape is not a rectangle or an ellipse.`);
      return {
        ...head,
        kind: "shape",
        shape,
        rect: readRect(v["rect"], w),
        outline: str(v["outline"], `${w}: its outline colour`),
        fill: v["fill"] === null ? null : str(v["fill"], `${w}: its fill`),
        lineWidthBp: num(v["lineWidthBp"], `${w}: lineWidthBp`),
        ...alt,
      };
    }
    case "arrow":
      return {
        ...head,
        kind: "arrow",
        from: readPoint(v["from"], w),
        to: readPoint(v["to"], w),
        color: str(v["color"], `${w}: its colour`),
        lineWidthBp: num(v["lineWidthBp"], `${w}: lineWidthBp`),
        ...alt,
      };
    case "blur":
      return { ...head, kind: "blur", rect: readRect(v["rect"], w) };
    case "redact":
      return { ...head, kind: "redact", rect: readRect(v["rect"], w), color: str(v["color"], `${w}: its colour`) };
    case "zoom":
      return {
        ...head,
        kind: "zoom",
        rect: readRect(v["rect"], w),
        transitionMs: num(v["transitionMs"], `${w}: transitionMs`),
        description: str(v["description"], `${w}: its description`),
      };
    default:
      throw new Error(`${w} has a kind this program does not know: ${String(v["kind"])}.`);
  }
}

/**
 * Read the text of an `edit.json`. Refuses anything that is not an edit of a version this program
 * knows (0.1.x). A file that reads fine but has mistakes comes back with its problems, so the
 * author can be told; whether to go on is up to the caller (see `errorsOf`).
 */
export function parse(text: string): ParseResult {
  let doc: unknown;
  try {
    doc = JSON.parse(text);
  } catch (e) {
    return { ok: false, message: `This is not a readable edit file: ${(e as Error).message}` };
  }
  if (!isObject(doc) || doc["format"] !== EDIT_FORMAT) {
    return { ok: false, message: `This is not an OpenStage Capture edit file: its format should be "${EDIT_FORMAT}".` };
  }
  const version = doc["schemaVersion"];
  if (typeof version !== "string" || !(version.startsWith("0.1.") || version.startsWith("0.2.") || version.startsWith("0.3."))) {
    return {
      ok: false,
      message: `This edit file is version ${String(version)}, and this program reads versions 0.1, 0.2 and 0.3 (it writes ${EDIT_SCHEMA_VERSION}). It was probably made by a newer program.`,
    };
  }
  try {
    const sources = (Array.isArray(doc["sources"]) ? doc["sources"] : []).map((s: unknown): SourceInfo => {
      if (!isObject(s)) throw new Error("A source is not an object.");
      const kind = s["kind"];
      if (!SOURCE_KINDS.includes(kind as (typeof SOURCE_KINDS)[number])) {
        throw new Error(`A source has a kind this program does not know: ${String(kind)}.`);
      }
      const duration = s["durationMs"];
      return {
        ref: readRef(s["ref"], "A source"),
        kind: kind as SourceInfo["kind"],
        name: str(s["name"], "A source's name"),
        durationMs: duration === null ? null : num(duration, "A source's durationMs"),
        ...(typeof s["hasTextAlternative"] === "boolean" ? { hasTextAlternative: s["hasTextAlternative"] } : {}),
      };
    });
    const tracks = (Array.isArray(doc["tracks"]) ? doc["tracks"] : []).map((t: unknown): Track => {
      if (!isObject(t)) throw new Error("A track is not an object.");
      const kind = t["kind"];
      if (!TRACK_KINDS.includes(kind as TrackKind)) {
        throw new Error(`A track has a kind this program does not know: ${String(kind)}.`);
      }
      const id = str(t["id"], "A track's id");
      const clips = (Array.isArray(t["clips"]) ? t["clips"] : []).map((c: unknown) => readClip(c, `The track ${id}`));
      // Version 0.1 had no items: a missing list is an empty one (this is the whole migration from 0.1 to 0.2).
      const items = (Array.isArray(t["items"]) ? t["items"] : []).map((i: unknown) => readItem(i, `The track ${id}`));
      return { id, kind: kind as TrackKind, name: str(t["name"], `The track ${id}: its name`), clips, items };
    });
    const model: Model = {
      format: EDIT_FORMAT,
      // An older file is brought up to the current version; saving it writes the current version.
      schemaVersion: EDIT_SCHEMA_VERSION,
      nextId: num(doc["nextId"], "nextId"),
      sources,
      tracks,
    };
    return { ok: true, model, problems: validate(model) };
  } catch (e) {
    return { ok: false, message: `This edit file cannot be read: ${(e as Error).message}` };
  }
}

/** Whether a parsed edit can be edited and rendered: it read fine and has no errors (needs-attention is allowed). */
export function isSound(result: ParseResult): boolean {
  return result.ok && errorsOf(result.problems).length === 0;
}

/** Put every track's clips in order of start. Used when an edit is built by hand. */
export function normalize(model: Model): Model {
  return { ...model, tracks: model.tracks.map((t) => ({ ...t, clips: sortClips(t.clips), items: sortItems(t.items) })) };
}
