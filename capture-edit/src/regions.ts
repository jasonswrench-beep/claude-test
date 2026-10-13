// SPDX-License-Identifier: Apache-2.0
/**
 * Rectangles on the picture, and saying where they are in words (stories C5 and C7).
 *
 * A rectangle is stored in ten-thousandths of the picture's width and height, so it means the same
 * thing at any size of video and is always a whole number. Authors type pixels; these turn pixels into
 * that and back, given the size of the picture.
 */

import { FULL, type Point, type Rect } from "./model.ts";

/** Whether a rectangle is whole numbers, has some size, and lies inside the picture. */
export function isRect(r: Rect): boolean {
  return (
    [r.x, r.y, r.width, r.height].every(Number.isInteger) &&
    r.x >= 0 &&
    r.y >= 0 &&
    r.width > 0 &&
    r.height > 0 &&
    r.x + r.width <= FULL &&
    r.y + r.height <= FULL
  );
}

export function isPoint(p: Point): boolean {
  return Number.isInteger(p.x) && Number.isInteger(p.y) && p.x >= 0 && p.x <= FULL && p.y >= 0 && p.y <= FULL;
}

export interface Pixels {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface Canvas {
  readonly width: number;
  readonly height: number;
}

/** A rectangle given in pixels, for a picture of this size. */
export function rectFromPixels(px: Pixels, canvas: Canvas): Rect {
  return {
    x: Math.round((px.x * FULL) / canvas.width),
    y: Math.round((px.y * FULL) / canvas.height),
    width: Math.round((px.width * FULL) / canvas.width),
    height: Math.round((px.height * FULL) / canvas.height),
  };
}

/** A rectangle in pixels, for a picture of this size. */
export function rectToPixels(r: Rect, canvas: Canvas): Pixels {
  return {
    x: Math.round((r.x * canvas.width) / FULL),
    y: Math.round((r.y * canvas.height) / FULL),
    width: Math.round((r.width * canvas.width) / FULL),
    height: Math.round((r.height * canvas.height) / FULL),
  };
}

/** Where a rectangle is, in words: "the top left", "the middle", "the bottom". */
export function describePlace(r: Rect): string {
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  const across = cx < FULL / 3 ? "left" : cx > (FULL * 2) / 3 ? "right" : "middle";
  const down = cy < FULL / 3 ? "top" : cy > (FULL * 2) / 3 ? "bottom" : "middle";
  if (across === "middle" && down === "middle") return "the middle";
  if (across === "middle") return `the ${down}`;
  if (down === "middle") return `the ${across}`;
  return `the ${down} ${across}`;
}

/** How many times bigger the part is shown than it is on the whole picture, by width. */
export function magnification(r: Rect): number {
  return FULL / r.width;
}

/** The magnification in words: "2 times", "1.5 times". */
export function describeMagnification(r: Rect): string {
  const m = Math.round(magnification(r) * 10) / 10;
  const text = Number.isInteger(m) ? String(m) : m.toFixed(1);
  return `${text} ${text === "1" ? "time" : "times"}`;
}

/** A rectangle as a share of the picture, in words: "a quarter of its width and half of its height". */
export function describeSize(r: Rect): string {
  const share = (v: number) => `${Math.round(v / 100)} percent`;
  return `${share(r.width)} of its width and ${share(r.height)} of its height`;
}
