// SPDX-License-Identifier: Apache-2.0
/**
 * How far a clip, overlay or zoom can be moved or trimmed: the ends of a slider, and the place a drag stops.
 *
 * These are not worked out a second time. Each limit is found by asking the editing action itself whether it
 * would be accepted, so what the screen offers and what the edit model allows cannot disagree: a grip can always
 * be taken to the end of its slider, and one step beyond is always refused.
 *
 * What can be done is an unbroken stretch around where the thing is now (it may not run into a neighbour, nor past
 * the end of its recording), so the farthest acceptable time is found by halving the distance.
 */

import { findClip, findItem, modelLengthMs, MIN_CLIP_MS, clipEndMs, itemEndMs, type Model } from "./model.ts";
import { moveItem, trimItemEnd, trimItemStart } from "./items.ts";
import { setClipStart, trimEnd, trimStart, type Result } from "./ops.ts";

/** How far past the end of the edit something may be moved or made longer, when nothing else stops it: 10 minutes. */
export const ROOM_PAST_END_MS = 600_000;

export interface Range {
  readonly min: number;
  readonly max: number;
}

export interface Limits {
  readonly kind: "clip" | "item";
  /** Where the whole thing may start, with its length kept (dragging it). */
  readonly move: Range;
  /** Where its start may be (the start grip). The end stays. */
  readonly trimStart: Range;
  /** Where its end may be (the end grip). The start stays. */
  readonly trimEnd: Range;
}

/** The farthest time from `accepted` towards `bound` that `works` accepts. `accepted` itself must work. */
function farthest(accepted: number, bound: number, works: (ms: number) => boolean): number {
  if (bound === accepted || works(bound)) return bound;
  let good = accepted;
  let bad = bound;
  while (Math.abs(bad - good) > 1) {
    const middle = Math.trunc((good + bad) / 2);
    if (works(middle)) good = middle;
    else bad = middle;
  }
  return good;
}

const fine = (result: Result): boolean => result.ok;

/** The limits of a clip, overlay or zoom by its id, or undefined if there is none. */
export function limitsOf(model: Model, id: string): Limits | undefined {
  const clip = findClip(model, id)?.clip;
  const item = clip === undefined ? findItem(model, id)?.item : undefined;
  const start = clip?.startMs ?? item?.startMs;
  const end = clip === undefined ? (item === undefined ? undefined : itemEndMs(item)) : clipEndMs(clip);
  if (start === undefined || end === undefined) return undefined;
  const top = Math.max(modelLengthMs(model), end) + ROOM_PAST_END_MS;

  const moveTo = (ms: number) => fine(clip !== undefined ? setClipStart(model, id, ms) : moveItem(model, id, ms));
  const startTo = (ms: number) => fine(clip !== undefined ? trimStart(model, id, ms) : trimItemStart(model, id, ms));
  const endTo = (ms: number) => fine(clip !== undefined ? trimEnd(model, id, ms) : trimItemEnd(model, id, ms));

  // A zoom may not run into the zoom before or after it on its track. Moving a zoom whole could jump clean over its
  // neighbour (the list is kept in order of start, so that is sound), but a slider or a drag should not: it stops at
  // the neighbour. So the search is kept between the neighbours, where what is accepted is one unbroken stretch.
  let before = 0;
  let nextStart: number | undefined;
  if (item !== undefined) {
    const track = model.tracks.find((t) => t.items.some((i) => i.id === id));
    if (track?.kind === "zoom") {
      const others = track.items.filter((i) => i.id !== id);
      const previous = others.filter((i) => i.startMs < start).at(-1);
      const next = others.find((i) => i.startMs > start);
      if (previous !== undefined) before = Math.min(itemEndMs(previous), start);
      if (next !== undefined) nextStart = next.startMs;
    }
  }

  const latestStart = end - MIN_CLIP_MS;
  const earliestEnd = start + MIN_CLIP_MS;
  const highestStart = nextStart === undefined ? top : Math.max(start, nextStart - (end - start));
  const highestEnd = nextStart === undefined ? top : Math.max(end, nextStart);
  return {
    kind: clip !== undefined ? "clip" : "item",
    move: { min: farthest(start, before, moveTo), max: farthest(start, highestStart, moveTo) },
    trimStart: { min: farthest(start, before, startTo), max: farthest(start, latestStart, startTo) },
    trimEnd: { min: farthest(end, earliestEnd, endTo), max: farthest(end, highestEnd, endTo) },
  };
}
