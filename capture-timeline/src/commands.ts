// SPDX-License-Identifier: Apache-2.0
/**
 * The editing commands and their keys, in one table that both screens (the clip list and the timeline) use to make
 * their buttons, to read keys, and to write their lists of shortcuts, so the two cannot drift apart.
 */

import type { EditSession, Outcome } from "./session.ts";

export interface Command {
  readonly id: string;
  readonly label: string;
  /** How the key is written for people. */
  readonly key: string;
  /** How the key is written for `aria-keyshortcuts`. */
  readonly aria: string;
  readonly match: (e: KeyboardEvent) => boolean;
  readonly run: (session: EditSession) => Outcome;
  /** The answer is long, so it is also kept as text on the page to be read at leisure. */
  readonly readout?: true;
}

const isKey = (e: KeyboardEvent, key: string, shift = false): boolean =>
  e.key.toLowerCase() === key && e.shiftKey === shift && !e.ctrlKey && !e.metaKey && !e.altKey;

const plainDelete = (e: KeyboardEvent): boolean =>
  (e.key === "Delete" || e.key === "Backspace") && !e.ctrlKey && !e.metaKey && !e.altKey;

export const COMMANDS: readonly Command[] = [
  { id: "split", label: "Split at playhead", key: "S", aria: "S", match: (e) => isKey(e, "s"), run: (s) => s.splitSelected() },
  { id: "split-all", label: "Split every track at playhead", key: "Shift+S", aria: "Shift+S", match: (e) => isKey(e, "s", true), run: (s) => s.splitEveryTrack() },
  { id: "trim-start", label: "Trim start to playhead", key: "B", aria: "B", match: (e) => isKey(e, "b"), run: (s) => s.trimStartToPlayhead() },
  { id: "trim-end", label: "Trim end to playhead", key: "E", aria: "E", match: (e) => isKey(e, "e"), run: (s) => s.trimEndToPlayhead() },
  { id: "delete-gap", label: "Delete, leave a gap", key: "Delete", aria: "Delete", match: (e) => plainDelete(e) && !e.shiftKey, run: (s) => s.deleteSelected(false) },
  { id: "delete-close", label: "Delete, close the gap", key: "Shift+Delete", aria: "Shift+Delete", match: (e) => plainDelete(e) && e.shiftKey, run: (s) => s.deleteSelected(true) },
  { id: "join", label: "Join with next clip", key: "J", aria: "J", match: (e) => isKey(e, "j"), run: (s) => s.joinSelectedWithNext() },
  {
    id: "earlier",
    label: "Move clip earlier",
    key: "Alt+Up",
    aria: "Alt+ArrowUp",
    match: (e) => e.key === "ArrowUp" && e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey,
    run: (s) => s.moveSelected("earlier"),
  },
  {
    id: "later",
    label: "Move clip later",
    key: "Alt+Down",
    aria: "Alt+ArrowDown",
    match: (e) => e.key === "ArrowDown" && e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey,
    run: (s) => s.moveSelected("later"),
  },
  { id: "mute", label: "Mute or unmute clip", key: "M", aria: "M", match: (e) => isKey(e, "m"), run: (s) => s.toggleMute() },
  {
    id: "play",
    label: "Play or pause",
    key: "Space",
    aria: "Space",
    match: (e) => e.key === " " && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey,
    run: (s) => s.togglePlay(),
  },
  { id: "describe", label: "Describe what is at the playhead", key: "D", aria: "D", match: (e) => isKey(e, "d"), run: (s) => s.describePlayhead(), readout: true },
  {
    id: "undo",
    label: "Undo",
    key: "Ctrl+Z",
    aria: "Control+Z",
    match: (e) => (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "z",
    run: (s) => s.undo(),
  },
  {
    id: "redo",
    label: "Redo",
    key: "Ctrl+Y",
    aria: "Control+Y",
    match: (e) => (e.ctrlKey || e.metaKey) && !e.altKey && ((!e.shiftKey && e.key.toLowerCase() === "y") || (e.shiftKey && e.key.toLowerCase() === "z")),
    run: (s) => s.redo(),
  },
];

/** The command a key press is, if it is one. */
export function commandFor(e: KeyboardEvent): Command | undefined {
  return COMMANDS.find((c) => c.match(e));
}

/** Is this key press in a text box (where letters are typing, and Ctrl+Z is the box's own undo)? */
export function isTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement || target.isContentEditable) return true;
  return target instanceof HTMLInputElement && !["checkbox", "radio", "button", "submit"].includes(target.type);
}
