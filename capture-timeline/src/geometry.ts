// SPDX-License-Identifier: Apache-2.0
/**
 * The arithmetic of the timeline, with no screen: how wide a stretch of time is, which times get a mark on the ruler,
 * what time a pointer is over, and which clip the arrow keys go to. Kept apart from the drawing, as OpenStage's Timeline
 * keeps its arithmetic in `timelinemodel.ts`, so it can be tested without a browser.
 */

import type { Row } from "../../capture-edit/src/index.ts";

/** The most the timeline can be zoomed in: 400 pixels for each second. */
export const MAX_PX_PER_MS = 0.4;

/** Steps between the marks of the ruler, in milliseconds, from fine to coarse. */
const TICK_STEPS_MS = [100, 200, 500, 1000, 2000, 5000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000, 900_000, 1_800_000, 3_600_000];

/** How much time the timeline has room for: the edit, with a little room after its end. At least 10 seconds. */
export function contentMs(lengthMs: number): number {
  return Math.max(lengthMs + Math.max(2000, Math.round(lengthMs * 0.05)), 10_000);
}

/** The scale at which the whole of `contentMs` fits across `widthPx`. */
export function fitScale(widthPx: number, content: number): number {
  return Math.max(widthPx, 50) / Math.max(content, 1);
}

/** One step in or out. The result stays between `fit` (the whole edit showing) and the most zoomed in. */
export function zoomed(scale: number, fit: number, direction: "in" | "out"): number {
  const most = Math.max(MAX_PX_PER_MS, fit);
  const next = direction === "in" ? scale * 2 : scale / 2;
  return Math.min(most, Math.max(fit, next));
}

/** How much time fits across a width at a scale. */
export function visibleSpanMs(widthPx: number, scale: number): number {
  return Math.round(widthPx / scale);
}

/** The gap between marks on the ruler that keeps them at least `minGapPx` apart. */
export function tickStepMs(scale: number, minGapPx = 90): number {
  return TICK_STEPS_MS.find((step) => step * scale >= minGapPx) ?? TICK_STEPS_MS[TICK_STEPS_MS.length - 1] ?? 3_600_000;
}

/** The times that get a mark between two times (including a little either side, so marks do not pop in at the edges). */
export function ticksBetween(fromMs: number, toMs: number, step: number, lastMs: number): number[] {
  const first = Math.max(0, Math.floor(fromMs / step) * step);
  const last = Math.min(lastMs, toMs);
  const out: number[] = [];
  for (let t = first; t <= last; t += step) out.push(t);
  return out;
}

/** The time under a pointer, where `originX` is where time 0 is on the screen. Whole milliseconds, never below 0. */
export function msAtX(clientX: number, originX: number, scale: number): number {
  return Math.max(0, Math.round((clientX - originX) / scale));
}

/** Round a time to the nearest 10 milliseconds, so a drag lands on tidy numbers. */
export function tidy(ms: number): number {
  return Math.round(ms / 10) * 10;
}

export type Direction = "left" | "right" | "up" | "down" | "first" | "last";

/**
 * Where an arrow key takes the choice. Left and Right go to the clip before and after in the same track; Up and Down go to
 * the nearest track above or below that has anything on it, and to the clip there that is showing at the same time (or
 * the nearest one in time); Home and End go to the first and last in the track. `tracks` is the track ids in the order
 * they are shown, and `rows` the rows in the order of the list. Returns undefined at an edge.
 */
export function navigate(rows: readonly Row[], tracks: readonly string[], fromId: string, direction: Direction): Row | undefined {
  const from = rows.find((r) => r.clipId === fromId);
  if (from === undefined) return rows[0];
  const lane = (trackId: string): Row[] => rows.filter((r) => r.trackId === trackId);
  const mine = lane(from.trackId);
  const at = mine.findIndex((r) => r.clipId === fromId);
  switch (direction) {
    case "left":
      return mine[at - 1];
    case "right":
      return mine[at + 1];
    case "first":
      return at === 0 ? undefined : mine[0];
    case "last":
      return at === mine.length - 1 ? undefined : mine[mine.length - 1];
    case "up":
    case "down": {
      const here = tracks.indexOf(from.trackId);
      const step = direction === "up" ? -1 : 1;
      for (let i = here + step; i >= 0 && i < tracks.length; i += step) {
        const there = lane(tracks[i] ?? "");
        if (there.length === 0) continue;
        const showing = there.find((r) => r.startMs <= from.startMs && from.startMs < r.endMs);
        if (showing !== undefined) return showing;
        return [...there].sort((a, b) => Math.abs(a.startMs - from.startMs) - Math.abs(b.startMs - from.startMs))[0];
      }
      return undefined;
    }
  }
}
