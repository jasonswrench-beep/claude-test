// SPDX-License-Identifier: Apache-2.0
/**
 * Everything behind the clip list, with no screen: which row is selected, where the playhead is, and
 * every command the list offers (split, trim, delete, join, move, rename, set an exact time, undo).
 *
 * The screen (`panel.ts`) only shows this and passes keys to it, so every behaviour can be tested here
 * without a browser. Every command gives back a sentence in plain words, either what it did or why it
 * could not, and the screen says that sentence aloud through a live region.
 *
 * Choices made here that could have gone the other way (listed in `README.md` for the owner to veto):
 * the selection stays on the same clip after a split or a join; after a delete it goes to the row that
 * took the deleted row's place; the playhead is kept inside the edit; Delete leaves a gap and
 * Shift+Delete closes it (the edit model gives no default, so both are offered and named).
 */

import {
  deleteClip,
  deleteItem,
  describeAt,
  describeLength,
  describeRows,
  describeTime,
  findClip,
  History,
  joinWithNext,
  listCaption,
  listRows,
  modelLengthMs,
  moveItem,
  parseTime,
  renameClip,
  renameItem,
  reorderClip,
  setClipLength,
  setClipStart,
  setMuted,
  splitAt,
  stepTime,
  trimEnd,
  trimItemEnd,
  trimItemStart,
  trimStart,
  type Model,
  type Result,
  type Row,
  type StepKey,
} from "../../capture-edit/src/index.ts";

/** What a command says back: a sentence for the author, and whether it worked. */
export interface Outcome {
  readonly ok: boolean;
  /** What to tell the author, in words. */
  readonly message: string;
}

/** An outcome of moving the selection, which also says whether the selection actually moved. */
export interface Moved extends Outcome {
  readonly changed: boolean;
}

/** What changed, so the screen only redraws what it must. */
export type ChangeKind = "model" | "selection" | "playhead";

export type Where = "previous" | "next" | "first" | "last";

/** What the form under the list shows for the selected row. */
export interface Details {
  readonly id: string;
  readonly name: string;
  readonly type: string;
  readonly trackName: string;
  readonly startMs: number;
  readonly lengthMs: number;
  readonly endMs: number;
  /** A clip can be split, joined, reordered and muted; an overlay or zoom cannot. */
  readonly isClip: boolean;
  readonly muted: boolean;
}

function quote(name: string): string {
  return `"${name}"`;
}

function ok(message: string): Outcome {
  return { ok: true, message };
}

function no(message: string): Outcome {
  return { ok: false, message };
}

const NOTHING_SELECTED = "Nothing is selected. Select a row in the list first.";

export class EditSession {
  readonly #history: History;
  #playheadMs = 0;
  #selectedId: string | undefined;
  readonly #listeners = new Set<(kind: ChangeKind) => void>();
  #rowsFor: Model | undefined;
  #rows: readonly Row[] = [];
  #labelsFor: Model | undefined;
  #labels = new Map<string, string>();

  constructor(model: Model, limit?: number) {
    this.#history = new History(model, limit);
    this.#selectedId = this.rows[0]?.clipId;
  }

  // ---- what the screen shows ----

  get model(): Model {
    return this.#history.model;
  }

  /** Every clip, overlay and zoom, in track order and then time order. Worked out once per change. */
  get rows(): readonly Row[] {
    const model = this.#history.model;
    if (this.#rowsFor !== model) {
      this.#rows = listRows(model);
      this.#rowsFor = model;
    }
    return this.#rows;
  }

  /** A sentence saying how much the list holds, for its caption. */
  get caption(): string {
    return listCaption(this.#history.model);
  }

  /** How long the edit is, in milliseconds. */
  get lengthMs(): number {
    return modelLengthMs(this.#history.model);
  }

  get playheadMs(): number {
    return this.#playheadMs;
  }

  get selectedId(): string | undefined {
    return this.#selectedId;
  }

  get selectedRow(): Row | undefined {
    return this.rows.find((r) => r.clipId === this.#selectedId);
  }

  /** The numbers and words the form under the list shows. */
  get details(): Details | undefined {
    const row = this.selectedRow;
    if (row === undefined) return undefined;
    const found = findClip(this.#history.model, row.clipId);
    return {
      id: row.clipId,
      name: row.name,
      type: row.type,
      trackName: row.trackName,
      startMs: row.startMs,
      lengthMs: row.lengthMs,
      endMs: row.endMs,
      isClip: found !== undefined,
      muted: found?.clip.muted ?? false,
    };
  }

  get canUndo(): boolean {
    return this.#history.canUndo;
  }

  get canRedo(): boolean {
    return this.#history.canRedo;
  }

  get undoLabel(): string | undefined {
    return this.#history.undoLabel;
  }

  get redoLabel(): string | undefined {
    return this.#history.redoLabel;
  }

  /** Be told when something changes. Returns a function that stops the telling. */
  subscribe(listener: (kind: ChangeKind) => void): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  #emit(kind: ChangeKind): void {
    for (const listener of [...this.#listeners]) listener(kind);
  }

  /** One row in a sentence, as it is read when the row takes focus. Every row's sentence is worked out in one pass. */
  describeRow(id: string): string {
    const model = this.#history.model;
    if (this.#labelsFor !== model) {
      this.#labels = describeRows(model);
      this.#labelsFor = model;
    }
    return this.#labels.get(id) ?? `There is no row with the id ${id}.`;
  }

  /** The selected row in a sentence (on request). */
  describeSelected(): Outcome {
    const id = this.#selectedId;
    return id === undefined ? no(NOTHING_SELECTED) : ok(this.describeRow(id));
  }

  /** What is on screen and in the sound at the playhead, in words (story C4, on request). */
  describePlayhead(): Outcome {
    return ok(describeAt(this.#history.model, this.#playheadMs));
  }

  // ---- selection ----

  /** Select a row by its id. */
  select(id: string): Moved {
    const row = this.rows.find((r) => r.clipId === id);
    if (row === undefined) return { ok: false, message: `There is no row with the id ${id}.`, changed: false };
    const changed = this.#selectedId !== id;
    this.#selectedId = id;
    if (changed) this.#emit("selection");
    return { ok: true, message: this.describeRow(id), changed };
  }

  /** Move the selection up, down, or to the first or last row. */
  moveSelection(where: Where): Moved {
    const rows = this.rows;
    if (rows.length === 0) return { ok: false, message: "The list is empty.", changed: false };
    const at = rows.findIndex((r) => r.clipId === this.#selectedId);
    const last = rows.length - 1;
    let to: number;
    switch (where) {
      case "first":
        to = 0;
        break;
      case "last":
        to = last;
        break;
      case "previous":
        to = at <= 0 ? 0 : at - 1;
        break;
      case "next":
        to = at < 0 ? 0 : Math.min(at + 1, last);
        break;
    }
    const target = rows[to];
    if (target === undefined) return { ok: false, message: "The list is empty.", changed: false };
    if (at === to) {
      const edge = to === 0 ? "first" : "last";
      return { ok: true, message: rows.length === 1 ? "This is the only row." : `This is the ${edge} row.`, changed: false };
    }
    return this.select(target.clipId);
  }

  // ---- the playhead ----

  /**
   * Where the playhead is, in words, and how that relates to the selected row, such as
   * `12.4 seconds, 2.4 seconds into "Lesson"`. This is what a screen reader says for the playhead slider.
   */
  get playheadText(): string {
    const at = this.#playheadMs;
    const base = describeTime(at);
    const row = this.selectedRow;
    if (row === undefined) return base;
    const name = quote(row.name);
    if (at < row.startMs) return `${base}, ${describeLength(row.startMs - at)} before ${name}`;
    if (at === row.startMs) return `${base}, at the start of ${name}`;
    if (at < row.endMs) return `${base}, ${describeLength(at - row.startMs)} into ${name}`;
    if (at === row.endMs) return `${base}, at the end of ${name}`;
    return `${base}, ${describeLength(at - row.endMs)} after ${name}`;
  }

  #playheadSentence(): string {
    return `Playhead at ${this.playheadText}.`;
  }

  #movePlayhead(to: number): void {
    if (to !== this.#playheadMs) {
      this.#playheadMs = to;
      this.#emit("playhead");
    }
  }

  /** Move the playhead with the keys of the Timeline: 0.1 second, with Shift 1 second, Page keys 5 seconds, Home and End. */
  stepPlayhead(key: StepKey, shift = false): Outcome {
    const from = this.#playheadMs;
    const to = stepTime(key, from, 0, this.lengthMs, { shift });
    this.#movePlayhead(to);
    if (to === from) {
      if (this.lengthMs === 0) return ok("The edit is empty, so the playhead stays at 0 seconds.");
      return ok(to === 0 ? "The playhead is at the start." : `The playhead is at the end, ${describeTime(to)}.`);
    }
    return ok(this.#playheadSentence());
  }

  /** Put the playhead at a time. A time beyond the end goes to the end, and says so. */
  setPlayhead(ms: number): Outcome {
    const length = this.lengthMs;
    const target = Math.max(0, Math.round(ms));
    const to = Math.min(target, length);
    this.#movePlayhead(to);
    return ok(target > length ? `The edit is only ${describeLength(length)} long, so the playhead went to the end. ${this.#playheadSentence()}` : this.#playheadSentence());
  }

  /** Put the playhead at what the author typed ("72.4", "1:12.4", "1m12.4s"). */
  setPlayheadText(text: string): Outcome {
    const parsed = parseTime(text);
    return parsed.ok ? this.setPlayhead(parsed.ms) : no(parsed.message);
  }

  /** Move the playhead to where the selected row starts. */
  playheadToSelectedStart(): Outcome {
    const row = this.selectedRow;
    if (row === undefined) return no(NOTHING_SELECTED);
    this.#movePlayhead(Math.min(row.startMs, this.lengthMs));
    return ok(this.#playheadSentence());
  }

  /** Move the playhead to where the selected row ends. */
  playheadToSelectedEnd(): Outcome {
    const row = this.selectedRow;
    if (row === undefined) return no(NOTHING_SELECTED);
    this.#movePlayhead(Math.min(row.endMs, this.lengthMs));
    return ok(this.#playheadSentence());
  }

  // ---- changing the edit ----

  /**
   * Run an editing action. If it changed the edit, keep the selection and the playhead sound and tell the
   * screen. The edit model's own sentence is what the author hears.
   */
  #run(action: (model: Model) => Result): Outcome {
    const before = this.#history.model;
    const index = this.rows.findIndex((r) => r.clipId === this.#selectedId);
    const result = this.#history.run(action);
    if (!result.ok) return no(result.message);
    if (this.#history.model === before) return ok(result.message);
    return ok(result.message + this.#afterChange(index));
  }

  /** After the edit changed: keep the selection on a row that exists and the playhead inside the edit. */
  #afterChange(previousIndex: number): string {
    const rows = this.rows;
    let extra = "";
    if (!rows.some((r) => r.clipId === this.#selectedId)) {
      const next = rows[Math.min(Math.max(previousIndex, 0), rows.length - 1)];
      this.#selectedId = next?.clipId;
      extra += next === undefined ? " The list is now empty." : ` Selected ${quote(next.name)}.`;
    }
    const length = this.lengthMs;
    if (this.#playheadMs > length) {
      this.#playheadMs = length;
      extra += ` The playhead moved back to the end, ${describeTime(length)}.`;
    }
    this.#emit("model");
    return extra;
  }

  /** The selected row, or a sentence saying nothing is selected. */
  #selected(): Row | Outcome {
    return this.selectedRow ?? no(NOTHING_SELECTED);
  }

  #isClip(row: Row): boolean {
    return findClip(this.#history.model, row.clipId) !== undefined;
  }

  #notAClip(row: Row, what: string): Outcome {
    return no(`${quote(row.name)} is ${/^[aeiou]/i.test(row.type) ? "an" : "a"} ${row.type.toLowerCase()}, not a clip. ${what}`);
  }

  /** Split the selected clip at the playhead. */
  splitSelected(): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    if (!this.#isClip(row)) return this.#notAClip(row, "Only clips can be split.");
    const at = this.#playheadMs;
    if (!(row.startMs < at && at < row.endMs)) {
      return no(
        `Cannot split ${quote(row.name)} at ${describeTime(at)}: it runs from ${describeTime(row.startMs)} to ${describeTime(row.endMs)}, so the playhead has to be inside it.`,
      );
    }
    return this.#run((m) => splitAt(m, at, row.trackId));
  }

  /** Split every clip that the playhead is inside, on every track, so picture and sound are cut together. */
  splitEveryTrack(): Outcome {
    return this.#run((m) => splitAt(m, this.#playheadMs));
  }

  /** Move the start of the selected row to the playhead. */
  trimStartToPlayhead(): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    const at = this.#playheadMs;
    return this.#run((m) => (this.#isClip(row) ? trimStart(m, row.clipId, at) : trimItemStart(m, row.clipId, at)));
  }

  /** Move the end of the selected row to the playhead. */
  trimEndToPlayhead(): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    const at = this.#playheadMs;
    return this.#run((m) => (this.#isClip(row) ? trimEnd(m, row.clipId, at) : trimItemEnd(m, row.clipId, at)));
  }

  /** Delete the selected row. For a clip the author chooses: leave a gap, or move the later clips earlier to close it. */
  deleteSelected(closeGap: boolean): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    return this.#run((m) => (this.#isClip(row) ? deleteClip(m, row.clipId, closeGap) : deleteItem(m, row.clipId)));
  }

  /** Join the selected clip with the one after it. */
  joinSelectedWithNext(): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    if (!this.#isClip(row)) return this.#notAClip(row, "Only clips can be joined.");
    return this.#run((m) => joinWithNext(m, row.clipId));
  }

  /** Move the selected clip one place earlier or later in the order of its track. */
  moveSelected(direction: "earlier" | "later"): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    if (!this.#isClip(row)) return this.#notAClip(row, "Overlays and zooms are placed by their start time, so they have no order to change.");
    const found = findClip(this.#history.model, row.clipId);
    if (found === undefined) return no(`There is no clip with the id ${row.clipId}.`);
    const to = found.clipIndex + (direction === "earlier" ? -1 : 1);
    if (to < 0) return no(`${quote(row.name)} is already the first clip on the ${row.trackName} track.`);
    if (to >= found.track.clips.length) return no(`${quote(row.name)} is already the last clip on the ${row.trackName} track.`);
    return this.#run((m) => reorderClip(m, row.clipId, to));
  }

  /** Mute or unmute the selected clip. */
  setSelectedMuted(muted: boolean): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    if (!this.#isClip(row)) return this.#notAClip(row, "Only clips can be muted.");
    return this.#run((m) => setMuted(m, row.clipId, muted));
  }

  /** Mute the selected clip if it is sound on, unmute it if it is muted. */
  toggleMute(): Outcome {
    const details = this.details;
    return details === undefined ? no(NOTHING_SELECTED) : this.setSelectedMuted(!details.muted);
  }

  // ---- the form: exact values ----

  /** Give the selected row a new name. */
  rename(text: string): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    return this.#run((m) => (this.#isClip(row) ? renameClip(m, row.clipId, text) : renameItem(m, row.clipId, text)));
  }

  /** Set where the selected row starts, from what the author typed. A clip moves whole; its length stays. */
  setStartText(text: string): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    const parsed = parseTime(text);
    if (!parsed.ok) return no(parsed.message);
    return this.#run((m) => (this.#isClip(row) ? setClipStart(m, row.clipId, parsed.ms) : moveItem(m, row.clipId, parsed.ms)));
  }

  /** Set how long the selected row lasts, from what the author typed. The start stays; the end moves. */
  setLengthText(text: string): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    const parsed = parseTime(text);
    if (!parsed.ok) return no(parsed.message);
    return this.#run((m) =>
      this.#isClip(row) ? setClipLength(m, row.clipId, parsed.ms) : trimItemEnd(m, row.clipId, row.startMs + parsed.ms),
    );
  }

  /** Set where the selected row ends, from what the author typed. The start stays; the length changes. */
  setEndText(text: string): Outcome {
    const row = this.#selected();
    if ("ok" in row) return row;
    const parsed = parseTime(text);
    if (!parsed.ok) return no(parsed.message);
    return this.#run((m) => (this.#isClip(row) ? trimEnd(m, row.clipId, parsed.ms) : trimItemEnd(m, row.clipId, parsed.ms)));
  }

  // ---- history ----

  undo(): Outcome {
    const before = this.#history.model;
    const index = this.rows.findIndex((r) => r.clipId === this.#selectedId);
    const result = this.#history.undo();
    if (!result.ok) return no(result.message);
    return ok(result.message + (this.#history.model === before ? "" : this.#afterChange(index)));
  }

  redo(): Outcome {
    const before = this.#history.model;
    const index = this.rows.findIndex((r) => r.clipId === this.#selectedId);
    const result = this.#history.redo();
    if (!result.ok) return no(result.message);
    return ok(result.message + (this.#history.model === before ? "" : this.#afterChange(index)));
  }
}
