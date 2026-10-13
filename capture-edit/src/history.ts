// SPDX-License-Identifier: Apache-2.0
/**
 * Undo and redo (story C11).
 *
 * The history keeps the edit and every earlier state of it. Because the model is never changed in
 * place, an earlier state is just the model as it was, so undo cannot go wrong. A drag with the mouse
 * (or a held key) is many small actions but one undo step: `beginGroup`, then the actions, then
 * `endGroup`; Escape calls `cancelGroup`, which puts everything back and records nothing.
 */

import type { Model } from "./model.ts";
import type { Result } from "./ops.ts";
import { describeChange } from "./preview.ts";

interface Step {
  readonly label: string;
  readonly before: Model;
  readonly after: Model;
}

interface Group {
  readonly label: string | undefined;
  readonly before: Model;
  lastLabel: string;
  lastMessage: string;
}

export interface HistoryResult {
  readonly ok: boolean;
  /** What to tell the author. */
  readonly message: string;
}

export class History {
  #model: Model;
  #undo: Step[] = [];
  #redo: Step[] = [];
  #group: Group | undefined;
  readonly #limit: number;

  constructor(model: Model, limit = 200) {
    this.#model = model;
    this.#limit = Math.max(1, limit);
  }

  /** The edit as it is now. */
  get model(): Model {
    return this.#model;
  }

  get canUndo(): boolean {
    return this.#undo.length > 0 && this.#group === undefined;
  }

  get canRedo(): boolean {
    return this.#redo.length > 0 && this.#group === undefined;
  }

  /** What undo would undo, such as "split clip", or undefined. */
  get undoLabel(): string | undefined {
    return this.#undo.at(-1)?.label;
  }

  get redoLabel(): string | undefined {
    return this.#redo.at(-1)?.label;
  }

  get grouping(): boolean {
    return this.#group !== undefined;
  }

  /** Do an action. If it worked and changed something it becomes one undo step (or part of the open group). */
  run(action: (model: Model) => Result): Result {
    const result = action(this.#model);
    if (!result.ok || result.model === this.#model) return result;
    const before = this.#model;
    this.#model = result.model;
    if (this.#group !== undefined) {
      this.#group.lastLabel = result.label;
      this.#group.lastMessage = result.message;
    } else {
      this.#push({ label: result.label, before, after: result.model });
    }
    return result;
  }

  #push(step: Step): void {
    this.#undo.push(step);
    if (this.#undo.length > this.#limit) this.#undo.shift();
    this.#redo = [];
  }

  /** Start a group: everything done until `endGroup` is one undo step. */
  beginGroup(label?: string): HistoryResult {
    if (this.#group !== undefined) return { ok: false, message: "A drag is already in progress." };
    this.#group = { label, before: this.#model, lastLabel: label ?? "", lastMessage: "" };
    return { ok: true, message: "" };
  }

  /**
   * Finish the group. Its message says what the whole drag came to when that is simple (one clip's
   * start, end or place changed), otherwise the last thing it did. Nothing is recorded if it changed nothing.
   */
  endGroup(): HistoryResult {
    const group = this.#group;
    if (group === undefined) return { ok: false, message: "No drag is in progress." };
    this.#group = undefined;
    if (group.before !== this.#model) {
      this.#push({ label: group.label ?? group.lastLabel, before: group.before, after: this.#model });
    }
    return { ok: true, message: describeChange(group.before, this.#model) ?? group.lastMessage };
  }

  /** Put everything back as it was when the group began, and record nothing (Escape during a drag). */
  cancelGroup(): HistoryResult {
    const group = this.#group;
    if (group === undefined) return { ok: false, message: "No drag is in progress." };
    this.#group = undefined;
    this.#model = group.before;
    return { ok: true, message: "Cancelled. Nothing was changed." };
  }

  undo(): HistoryResult {
    if (this.#group !== undefined) return { ok: false, message: "Finish or cancel the drag first." };
    const step = this.#undo.pop();
    if (step === undefined) return { ok: false, message: "Nothing to undo." };
    this.#redo.push(step);
    this.#model = step.before;
    return { ok: true, message: `Undid: ${step.label}.` };
  }

  redo(): HistoryResult {
    if (this.#group !== undefined) return { ok: false, message: "Finish or cancel the drag first." };
    const step = this.#redo.pop();
    if (step === undefined) return { ok: false, message: "Nothing to redo." };
    this.#undo.push(step);
    this.#model = step.after;
    return { ok: true, message: `Redid: ${step.label}.` };
  }
}
