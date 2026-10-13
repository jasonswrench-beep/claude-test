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
  type Clip,
  type Model,
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
  if (c.altText !== undefined) out["altText"] = c.altText;
  if (c.decorative !== undefined) out["decorative"] = c.decorative;
  return out;
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
    tracks: model.tracks.map((t) => ({ id: t.id, kind: t.kind, name: t.name, clips: t.clips.map(clipJson) })),
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
const TRACK_KINDS: readonly TrackKind[] = ["screen", "webcam", "audio", "media"];

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
    ...(typeof v["altText"] === "string" ? { altText: v["altText"] } : {}),
    ...(typeof v["decorative"] === "boolean" ? { decorative: v["decorative"] } : {}),
  };
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
  if (typeof version !== "string" || !version.startsWith("0.1.")) {
    return {
      ok: false,
      message: `This edit file is version ${String(version)}, and this program reads version 0.1 only (it is ${EDIT_SCHEMA_VERSION}). It was probably made by a newer or older program.`,
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
      return { id, kind: kind as TrackKind, name: str(t["name"], `The track ${id}: its name`), clips };
    });
    const model: Model = {
      format: EDIT_FORMAT,
      schemaVersion: version,
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
  return { ...model, tracks: model.tracks.map((t) => ({ ...t, clips: sortClips(t.clips) })) };
}
