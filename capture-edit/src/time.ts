// SPDX-License-Identifier: Apache-2.0
/**
 * Time, in whole milliseconds, and how it is said and typed.
 *
 * Everything in the edit model is measured in whole milliseconds, so there is never a rounding
 * surprise and two programs always agree. This file turns a time into words ("1 minute 12.4
 * seconds") for announcements, turns what an author types into a time, and holds the keyboard
 * steps of the timeline, which are the same as OpenStage's existing Timeline: Left and Right Arrow
 * move a tenth of a second, Shift a second, Page Up and Page Down five seconds, Home and End go to
 * the ends.
 */

/** The step of the arrow keys, in milliseconds. */
export const STEP_FINE_MS = 100;
/** The step of Shift with an arrow key. */
export const STEP_SECOND_MS = 1000;
/** The step of Page Up and Page Down. */
export const STEP_COARSE_MS = 5000;

/** Make a number a whole, non-negative count of milliseconds. */
export function wholeMs(value: number): number {
  return Math.max(0, Math.round(value));
}

function plural(count: number, one: string): string {
  return `${count} ${one}${count === 1 ? "" : "s"}`;
}

/** The seconds part, with up to three decimals and no trailing zeros: 12.4 -> "12.4", 12 -> "12". */
function secondsText(ms: number): string {
  const text = (ms / 1000).toFixed(3).replace(/\.?0+$/, "");
  return text === "" ? "0" : text;
}

/**
 * A time in words, for an announcement.
 * 0 -> "0 seconds"; 1000 -> "1 second"; 72400 -> "1 minute 12.4 seconds"; 3725000 -> "1 hour 2 minutes 5 seconds".
 */
export function describeTime(ms: number): string {
  const total = wholeMs(ms);
  const hours = Math.floor(total / 3_600_000);
  const minutes = Math.floor(total / 60_000) % 60;
  const restMs = total % 60_000;
  const parts: string[] = [];
  if (hours > 0) parts.push(plural(hours, "hour"));
  if (minutes > 0) parts.push(plural(minutes, "minute"));
  if (restMs > 0 || parts.length === 0) {
    const seconds = secondsText(restMs);
    parts.push(`${seconds} ${seconds === "1" ? "second" : "seconds"}`);
  }
  return parts.join(" ");
}

/** A length of time in words, the same as a time. Kept separate so the meaning is clear where it is used. */
export function describeLength(ms: number): string {
  return describeTime(ms);
}

/** A time as a timecode for a field: 72400 -> "1:12.4", 3725000 -> "1:02:05", 500 -> "0:00.5". */
export function formatTimecode(ms: number): string {
  const total = wholeMs(ms);
  const hours = Math.floor(total / 3_600_000);
  const minutes = Math.floor(total / 60_000) % 60;
  const restMs = total % 60_000;
  const whole = Math.floor(restMs / 1000);
  const fraction = restMs % 1000 === 0 ? "" : "." + String(restMs % 1000).padStart(3, "0").replace(/0+$/, "");
  const ss = String(whole).padStart(2, "0") + fraction;
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${ss}` : `${minutes}:${ss}`;
}

export type TimeParse = { ok: true; ms: number } | { ok: false; message: string };

const HOW_TO_TYPE =
  "Type a time such as 72.4 (seconds), 1:12.4 (minutes and seconds), 1:02:05 (hours, minutes, seconds), or 1m12.4s.";

/**
 * What an author typed into a time field. Accepts seconds ("72.4"), minutes and seconds ("1:12.4"),
 * hours ("1:02:05") and the spoken form ("1m12.4s", "2m", "90s"). Says in words what was wrong.
 */
export function parseTime(text: string): TimeParse {
  const t = text.trim().toLowerCase();
  if (t === "") return { ok: false, message: `Nothing was typed. ${HOW_TO_TYPE}` };
  const number = String.raw`(\d+(?:\.\d+)?)`;
  // 1h2m5.5s, 1m12.4s, 90s, 2m
  const spoken = new RegExp(`^(?:${number}h)?(?:${number}m)?(?:${number}s)?$`).exec(t);
  if (spoken && (spoken[1] !== undefined || spoken[2] !== undefined || spoken[3] !== undefined)) {
    const h = Number(spoken[1] ?? 0);
    const m = Number(spoken[2] ?? 0);
    const s = Number(spoken[3] ?? 0);
    return { ok: true, ms: wholeMs((h * 3600 + m * 60 + s) * 1000) };
  }
  // 72.4, 1:12.4, 1:02:05
  if (/^\d+(?::\d{1,2}){0,2}(?:\.\d+)?$/.test(t)) {
    const [whole = "", fraction = ""] = t.split(".") as [string, string?];
    const parts = whole.split(":").map(Number);
    let seconds = 0;
    for (const p of parts) seconds = seconds * 60 + p;
    const ms = seconds * 1000 + (fraction === "" ? 0 : Math.round(Number("0." + fraction) * 1000));
    if (parts.length > 1 && parts.slice(1).some((p) => p >= 60)) {
      return { ok: false, message: `"${text.trim()}" has 60 or more minutes or seconds in a part. ${HOW_TO_TYPE}` };
    }
    return { ok: true, ms: wholeMs(ms) };
  }
  return { ok: false, message: `"${text.trim()}" is not a time. ${HOW_TO_TYPE}` };
}

/** The keys that move a grip or the playhead. */
export type StepKey = "Left" | "Right" | "PageUp" | "PageDown" | "Home" | "End";

/** The modifier keys held with it. */
export interface StepModifiers {
  shift?: boolean;
}

/**
 * Where a grip or the playhead goes when a key is pressed, with the same steps as OpenStage's Timeline.
 * `from` is where it is now, `min` and `max` are the ends it may reach. The result never leaves them.
 * Left and Right move 0.1 second, with Shift 1 second; Page Up and Page Down move 5 seconds; Home
 * and End go to `min` and `max`. Right and Page Up go later; Left and Page Down go earlier.
 */
export function stepTime(key: StepKey, from: number, min: number, max: number, modifiers: StepModifiers = {}): number {
  let target: number;
  switch (key) {
    case "Home":
      target = min;
      break;
    case "End":
      target = max;
      break;
    case "Left":
      target = from - (modifiers.shift ? STEP_SECOND_MS : STEP_FINE_MS);
      break;
    case "Right":
      target = from + (modifiers.shift ? STEP_SECOND_MS : STEP_FINE_MS);
      break;
    case "PageUp":
      target = from + STEP_COARSE_MS;
      break;
    case "PageDown":
      target = from - STEP_COARSE_MS;
      break;
  }
  return Math.min(Math.max(wholeMs(target), min), max);
}
