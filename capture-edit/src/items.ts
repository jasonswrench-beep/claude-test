// SPDX-License-Identifier: Apache-2.0
/**
 * Overlays and zooms: text, shapes, arrows, blur, redaction, and zoom regions (stories C5, C7, C8).
 *
 * These work like the clip actions: each gives back a new edit and a sentence, or a sentence saying
 * why not and changes nothing. After a change the item is checked, and anything that makes it unsound
 * refuses the change; things that are only likely to be hard to see or read (low contrast, tiny text)
 * are added to the sentence as a warning, so the author hears about them straight away.
 */

import { normalizeColor } from "./contrast.ts";
import {
  ALLOWED_ITEMS,
  MIN_CLIP_MS,
  findItem,
  findTrack,
  itemEndMs,
  sortItems,
  takeId,
  withItem,
  withTrack,
  type ArrowItem,
  type BlurItem,
  type Item,
  type Model,
  type Point,
  type Rect,
  type RedactItem,
  type ShapeItem,
  type TextItem,
  type ZoomItem,
} from "./model.ts";
import type { Done, Refused, Result } from "./ops.ts";
import { describeMagnification, describePlace } from "./regions.ts";
import { describeLength, describeTime, wholeMs } from "./time.ts";
import { itemKindName, validate, type Problem } from "./validate.ts";

function refuse(message: string): Refused {
  return { ok: false, message };
}

function done(model: Model, label: string, message: string, focus?: readonly string[]): Done {
  return { ok: true, model, label, message, ...(focus === undefined ? {} : { focus }) };
}

function unchanged(model: Model, message: string): Done {
  return { ok: true, model, label: "", message };
}

function quote(name: string): string {
  return `"${name}"`;
}

function capital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** The problems that an item has in a model, by their sentences. */
function problemsOf(model: Model, itemId: string): Problem[] {
  return validate(model).filter((p) => p.itemId === itemId);
}

/** Errors in the whole edit that were not there before, which a change must not introduce. */
function newErrors(before: Model, after: Model): Problem[] {
  const had = new Set(validate(before).filter((p) => p.severity === "error").map((p) => p.message));
  return validate(after).filter((p) => p.severity === "error" && !had.has(p.message));
}

/** What to add to a sentence about an item: its warnings and what is still needed, that it did not have before. */
function aboutItem(before: Model | undefined, after: Model, itemId: string): string {
  const had = new Set(before === undefined ? [] : problemsOf(before, itemId).map((p) => p.code));
  const fresh = problemsOf(after, itemId).filter((p) => p.severity !== "error" && !had.has(p.code));
  if (fresh.length === 0) return "";
  return " " + fresh.map((p) => (p.severity === "warning" ? `Warning: ${p.message}` : `Still needed: ${p.message}`)).join(" ");
}

// ---- adding ----

interface Timing {
  readonly startMs: number;
  readonly durationMs: number;
  readonly name?: string;
}

const LOWER_THIRD: Rect = { x: 500, y: 7800, width: 9000, height: 1400 };
const CENTRE: Rect = { x: 3000, y: 3000, width: 4000, height: 4000 };
const YELLOW = "#FFD400";

function place(model: Model, trackId: string, build: (id: string) => Item, wanted: string): Result {
  const track = findTrack(model, trackId);
  if (track === undefined) return refuse(`There is no track with the id ${trackId}.`);
  const taken = takeId(model, "item");
  const id = taken.id;
  const item = build(id);
  if (!ALLOWED_ITEMS[track.kind].includes(item.kind)) {
    const allowed = ALLOWED_ITEMS[track.kind];
    return refuse(
      allowed.length === 0
        ? `The ${track.name} track holds clips, not overlays or zooms.`
        : `A ${itemKindName(item)} cannot go on the ${track.name} track. That track takes ${allowed.join(", ")}.`,
    );
  }
  const next = withTrack(taken.model, { ...track, items: sortItems([...track.items, item]) });
  const errors = newErrors(model, next);
  if (errors[0] !== undefined) return refuse(errors[0].message);
  return done(
    next,
    `add ${wanted}`,
    `Added the ${itemKindName(item)} ${quote(item.name)} to the ${track.name} track at ${describeTime(item.startMs)}, ${describeLength(item.durationMs)} long.` +
      aboutItem(undefined, next, id),
    [id],
  );
}

function timing(t: Timing): { startMs: number; durationMs: number } {
  return { startMs: wholeMs(t.startMs), durationMs: wholeMs(t.durationMs) };
}

export interface NewText extends Timing {
  readonly text: string;
  readonly rect?: Rect;
  readonly fontSizeBp?: number;
  readonly color?: string;
  readonly background?: string | null;
  readonly bold?: boolean;
  readonly align?: "left" | "center" | "right";
}

/** Add words to the picture. By default: white on black in the lower third, 4 percent of the picture's height tall. */
export function addText(model: Model, trackId: string, request: NewText): Result {
  const text = request.text.trim();
  return place(
    model,
    trackId,
    (id): TextItem => ({
      id,
      kind: "text",
      name: request.name?.trim() || (text.length > 30 ? `${text.slice(0, 27)}...` : text) || "Text",
      ...timing(request),
      text,
      rect: request.rect ?? LOWER_THIRD,
      fontSizeBp: request.fontSizeBp ?? 400,
      color: normalizeColor(request.color ?? "#FFFFFF") ?? (request.color ?? ""),
      background:
        request.background === undefined
          ? "#000000"
          : request.background === null
            ? null
            : (normalizeColor(request.background) ?? request.background),
      bold: request.bold ?? false,
      align: request.align ?? "left",
    }),
    "text overlay",
  );
}

export interface NewShape extends Timing {
  readonly shape?: "rectangle" | "ellipse";
  readonly rect?: Rect;
  readonly outline?: string;
  readonly fill?: string | null;
  readonly lineWidthBp?: number;
  readonly altText?: string;
  readonly decorative?: boolean;
}

export function addShape(model: Model, trackId: string, request: NewShape): Result {
  return place(
    model,
    trackId,
    (id): ShapeItem => ({
      id,
      kind: "shape",
      name: request.name?.trim() || (request.shape === "ellipse" ? "Ellipse" : "Rectangle"),
      ...timing(request),
      shape: request.shape ?? "rectangle",
      rect: request.rect ?? CENTRE,
      outline: normalizeColor(request.outline ?? YELLOW) ?? (request.outline ?? ""),
      fill: request.fill === undefined || request.fill === null ? null : (normalizeColor(request.fill) ?? request.fill),
      lineWidthBp: request.lineWidthBp ?? 40,
      ...(request.altText === undefined ? {} : { altText: request.altText.trim() }),
      ...(request.decorative === undefined ? {} : { decorative: request.decorative }),
    }),
    "shape",
  );
}

export interface NewArrow extends Timing {
  readonly from: Point;
  readonly to: Point;
  readonly color?: string;
  readonly lineWidthBp?: number;
  readonly altText?: string;
  readonly decorative?: boolean;
}

export function addArrow(model: Model, trackId: string, request: NewArrow): Result {
  return place(
    model,
    trackId,
    (id): ArrowItem => ({
      id,
      kind: "arrow",
      name: request.name?.trim() || "Arrow",
      ...timing(request),
      from: request.from,
      to: request.to,
      color: normalizeColor(request.color ?? YELLOW) ?? (request.color ?? ""),
      lineWidthBp: request.lineWidthBp ?? 40,
      ...(request.altText === undefined ? {} : { altText: request.altText.trim() }),
      ...(request.decorative === undefined ? {} : { decorative: request.decorative }),
    }),
    "arrow",
  );
}

/** Blur a region so it cannot be read. Said plainly: a blur in the exported video cannot be undone by the viewer. */
export function addBlur(model: Model, trackId: string, request: Timing & { readonly rect: Rect }): Result {
  const result = place(
    model,
    trackId,
    (id): BlurItem => ({ id, kind: "blur", name: request.name?.trim() || "Blur", ...timing(request), rect: request.rect }),
    "blur",
  );
  return result.ok ? { ...result, message: `${result.message} Once the video is exported, nobody can undo this blur.` } : result;
}

export function addRedact(model: Model, trackId: string, request: Timing & { readonly rect: Rect; readonly color?: string }): Result {
  const result = place(
    model,
    trackId,
    (id): RedactItem => ({
      id,
      kind: "redact",
      name: request.name?.trim() || "Redaction",
      ...timing(request),
      rect: request.rect,
      color: normalizeColor(request.color ?? "#000000") ?? (request.color ?? ""),
    }),
    "redaction",
  );
  return result.ok ? { ...result, message: `${result.message} Once the video is exported, nobody can undo this redaction.` } : result;
}

/** How long a zoom takes to move in, and to move out, when nothing else is said: half a second. */
export const DEFAULT_ZOOM_TRANSITION_MS = 500;

export interface NewZoom extends Timing {
  readonly rect: Rect;
  readonly transitionMs?: number;
  readonly description?: string;
}

export function addZoom(model: Model, trackId: string, request: NewZoom): Result {
  return place(
    model,
    trackId,
    (id): ZoomItem => ({
      id,
      kind: "zoom",
      name: request.name?.trim() || "Zoom",
      ...timing(request),
      rect: request.rect,
      transitionMs: wholeMs(request.transitionMs ?? DEFAULT_ZOOM_TRANSITION_MS),
      description: (request.description ?? "").trim(),
    }),
    "zoom",
  );
}

// ---- changing ----

function locate(model: Model, itemId: string) {
  return findItem(model, itemId) ?? refuse(`There is no overlay or zoom with the id ${itemId}.`);
}

function describeItemName(item: Item): string {
  return `${capital(itemKindName(item))} ${quote(item.name)}`;
}

/** Replace an item with a changed copy, refusing if that makes the edit unsound. */
function change(
  model: Model,
  itemId: string,
  label: string,
  make: (item: Item) => Item | string,
  say: (item: Item, before: Item) => string,
): Result {
  const found = locate(model, itemId);
  if ("ok" in found) return found;
  const changed = make(found.item);
  if (typeof changed === "string") return refuse(changed);
  if (JSON.stringify(changed) === JSON.stringify(found.item)) return unchanged(model, `${describeItemName(found.item)} is already like that.`);
  const next = withItem(model, found.track.id, changed);
  const errors = newErrors(model, next);
  if (errors[0] !== undefined) return refuse(errors[0].message);
  return done(next, label, say(changed, found.item) + aboutItem(model, next, itemId), [itemId]);
}

export function renameItem(model: Model, itemId: string, name: string): Result {
  const next = name.trim();
  if (next === "") return refuse("A name cannot be empty.");
  return change(model, itemId, "rename", (i) => ({ ...i, name: next }), (_i, before) => `${describeItemName(before)} is now called ${quote(next)}.`);
}

/** Move an overlay or zoom to start at a time; its length stays. */
export function moveItem(model: Model, itemId: string, toMs: number): Result {
  const at = wholeMs(toMs);
  return change(model, itemId, "move", (i) => ({ ...i, startMs: at }), (i) => `${describeItemName(i)} now starts at ${describeTime(at)}.`);
}

/** Move an overlay or zoom to another track that takes it, keeping its time. */
export function moveItemToTrack(model: Model, itemId: string, trackId: string): Result {
  const found = locate(model, itemId);
  if ("ok" in found) return found;
  const { track: from, item } = found;
  const to = findTrack(model, trackId);
  if (to === undefined) return refuse(`There is no track with the id ${trackId}.`);
  if (to.id === from.id) return unchanged(model, `${describeItemName(item)} is already on the ${from.name} track.`);
  if (!ALLOWED_ITEMS[to.kind].includes(item.kind)) {
    const allowed = ALLOWED_ITEMS[to.kind];
    return refuse(
      `Cannot move ${quote(item.name)} to the ${to.name} track: ${allowed.length === 0 ? "that track holds clips, not overlays or zooms" : `that track takes ${allowed.join(", ")}`}.`,
    );
  }
  const moved = withTrack(model, { ...from, items: from.items.filter((i) => i.id !== item.id) });
  const next = withTrack(moved, { ...to, items: sortItems([...to.items, item]) });
  const errors = newErrors(model, next);
  if (errors[0] !== undefined) return refuse(errors[0].message);
  return done(
    next,
    "move to another track",
    `${describeItemName(item)} moved from the ${from.name} track to the ${to.name} track, still starting at ${describeTime(item.startMs)}.` + aboutItem(model, next, itemId),
    [itemId],
  );
}

/** Move the start grip: the end stays where it is. */
export function trimItemStart(model: Model, itemId: string, toMs: number): Result {
  const at = wholeMs(toMs);
  return change(
    model,
    itemId,
    "trim start",
    (i) => {
      const end = itemEndMs(i);
      if (at > end - MIN_CLIP_MS) return `Cannot start it later than ${describeTime(end - MIN_CLIP_MS)}: it must stay at least ${describeLength(MIN_CLIP_MS)} long.`;
      return { ...i, startMs: at, durationMs: end - at };
    },
    (i) => `${describeItemName(i)} now starts at ${describeTime(at)} and ends at ${describeTime(itemEndMs(i))}.`,
  );
}

/** Move the end grip: the start stays where it is. */
export function trimItemEnd(model: Model, itemId: string, toMs: number): Result {
  const at = wholeMs(toMs);
  return change(
    model,
    itemId,
    "trim end",
    (i) => {
      if (at < i.startMs + MIN_CLIP_MS) return `Cannot end it earlier than ${describeTime(i.startMs + MIN_CLIP_MS)}: it must stay at least ${describeLength(MIN_CLIP_MS)} long.`;
      return { ...i, durationMs: at - i.startMs };
    },
    (i) => `${describeItemName(i)} now ends at ${describeTime(at)}, lasting ${describeLength(i.durationMs)}.`,
  );
}

export function deleteItem(model: Model, itemId: string): Result {
  const found = locate(model, itemId);
  if ("ok" in found) return found;
  const next = withTrack(model, { ...found.track, items: found.track.items.filter((i) => i.id !== itemId) });
  return done(next, "delete", `${describeItemName(found.item)} deleted.`, [found.track.id]);
}

/** Set the rectangle of a text overlay, shape, blur, redaction or zoom (numeric entry; story C5 and C7). */
export function setItemRect(model: Model, itemId: string, rect: Rect): Result {
  return change(
    model,
    itemId,
    "set position",
    (i) => (i.kind === "arrow" ? "An arrow has two ends, not a rectangle. Set its ends instead." : { ...i, rect }),
    (i) => (i.kind === "zoom" ? `${describeItemName(i)} now shows the ${describePlace(rect).replace(/^the /, "")} of the picture at ${describeMagnification(rect)}.` : `${describeItemName(i)} is now in ${describePlace(rect)} of the picture.`),
  );
}

export function setArrowEnds(model: Model, itemId: string, from: Point, to: Point): Result {
  return change(
    model,
    itemId,
    "set arrow",
    (i) => (i.kind === "arrow" ? { ...i, from, to } : `${describeItemName(i)} is not an arrow.`),
    (i) => `${describeItemName(i)} now points from ${describePlace({ x: from.x, y: from.y, width: 0, height: 0 })} of the picture to ${describePlace({ x: to.x, y: to.y, width: 0, height: 0 })}.`,
  );
}

export function setText(model: Model, itemId: string, text: string): Result {
  return change(
    model,
    itemId,
    "set text",
    (i) => (i.kind === "text" ? { ...i, text: text.trim() } : `${describeItemName(i)} is not a text overlay.`),
    (i) => `${describeItemName(i)} now says: ${i.kind === "text" ? i.text : ""}`,
  );
}

export interface TextStyle {
  readonly fontSizeBp?: number;
  readonly color?: string;
  readonly background?: string | null;
  readonly bold?: boolean;
  readonly align?: "left" | "center" | "right";
}

export function setTextStyle(model: Model, itemId: string, style: TextStyle): Result {
  return change(
    model,
    itemId,
    "set text style",
    (i) => {
      if (i.kind !== "text") return `${describeItemName(i)} is not a text overlay.`;
      const color = style.color === undefined ? i.color : normalizeColor(style.color);
      const background = style.background === undefined ? i.background : style.background === null ? null : normalizeColor(style.background);
      if (color === undefined || background === undefined) return "That is not a colour. Write it like #FFFFFF.";
      return {
        ...i,
        fontSizeBp: style.fontSizeBp ?? i.fontSizeBp,
        color,
        background,
        bold: style.bold ?? i.bold,
        align: style.align ?? i.align,
      };
    },
    (i) => `${describeItemName(i)} has a new look.`,
  );
}

export interface ShapeStyle {
  readonly shape?: "rectangle" | "ellipse";
  readonly outline?: string;
  readonly fill?: string | null;
  readonly lineWidthBp?: number;
}

export function setShapeStyle(model: Model, itemId: string, style: ShapeStyle): Result {
  return change(
    model,
    itemId,
    "set shape style",
    (i) => {
      if (i.kind !== "shape") return `${describeItemName(i)} is not a shape.`;
      const outline = style.outline === undefined ? i.outline : normalizeColor(style.outline);
      const fill = style.fill === undefined ? i.fill : style.fill === null ? null : normalizeColor(style.fill);
      if (outline === undefined || fill === undefined) return "That is not a colour. Write it like #FFFFFF.";
      return { ...i, shape: style.shape ?? i.shape, outline, fill, lineWidthBp: style.lineWidthBp ?? i.lineWidthBp };
    },
    (i) => `${describeItemName(i)} has a new look.`,
  );
}

export interface ZoomSettings {
  readonly transitionMs?: number;
  readonly description?: string;
}

export function setZoom(model: Model, itemId: string, settings: ZoomSettings): Result {
  return change(
    model,
    itemId,
    "set zoom",
    (i) =>
      i.kind !== "zoom"
        ? `${describeItemName(i)} is not a zoom.`
        : {
            ...i,
            transitionMs: settings.transitionMs === undefined ? i.transitionMs : wholeMs(settings.transitionMs),
            description: settings.description === undefined ? i.description : settings.description.trim(),
          },
    (i) => `${describeItemName(i)} moves in and out over ${describeLength(i.kind === "zoom" ? i.transitionMs : 0)} each.`,
  );
}

/** What a shape or arrow points out, in words, or that it is only decoration. */
export function setItemAltText(model: Model, itemId: string, text: { altText?: string; decorative?: boolean }): Result {
  return change(
    model,
    itemId,
    "set alt text",
    (i) => {
      if (i.kind !== "shape" && i.kind !== "arrow") return `${describeItemName(i)} does not take alt text.`;
      const { altText: _a, decorative: _d, ...bare } = i;
      const altText = text.altText === undefined ? i.altText : text.altText.trim();
      const decorative = text.decorative ?? i.decorative;
      return {
        ...bare,
        ...(altText === undefined || altText === "" ? {} : { altText }),
        ...(decorative === undefined ? {} : { decorative }),
      } as Item;
    },
    (i) =>
      (i.kind === "shape" || i.kind === "arrow") && i.decorative === true
        ? `${describeItemName(i)} is marked as decorative.`
        : `${describeItemName(i)} now has alt text: ${(i.kind === "shape" || i.kind === "arrow" ? i.altText : "") ?? ""}`,
  );
}

