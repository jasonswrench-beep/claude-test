// SPDX-License-Identifier: Apache-2.0
/**
 * The clip list on screen (stories C1 and C2): a table of every clip, overlay and zoom that is fully
 * operable from the keyboard and says what it does aloud, with the playhead, the commands, and a form
 * for exact times.
 *
 * This file is only the screen. What each command does, and the sentence that says so, is in
 * `session.ts`, which is tested without a browser. Plain DOM, no libraries, written the way OpenStage's
 * Timeline panel is, so the two can be joined later.
 *
 * Keyboard (also listed on the page, from the same table that makes the buttons, so the two cannot drift):
 *   the list      Up and Down choose a row, Home and End go to the first and last row. Left and Right move the
 *                 playhead 0.1 second (Shift: 1 second), Page Up and Page Down 5 seconds. Enter or F2 edits the row.
 *   the playhead  as a slider: the same keys, with Home and End going to the start and end of the edit.
 *   from either   S split, Shift+S split every track, B trim start, E trim end, J join, M mute, D describe, Delete
 *                 (Shift+Delete closes the gap), Alt+Up and Alt+Down move the clip, Ctrl+Z undo, Ctrl+Y redo.
 *
 * Nothing is ever disabled (a disabled button leaves the Tab order and takes focus with it); a command that
 * cannot be done says why instead. Focus never gets stuck: Tab and Shift+Tab always move on, and Escape in a
 * text box puts it back on the list.
 *
 * Not yet checked with NVDA, Narrator or VoiceOver. How a screen reader reads the list, and whether the
 * sentence on each row (`aria-label`) or the cells read better, is what the first test with one has to settle.
 */

import { COLUMNS, formatTimecode, type Row, type StepKey } from "../../capture-edit/src/index.ts";
import type { EditSession, Outcome } from "./session.ts";

export interface PanelOptions {
  /** Also tell this function every message (for example OpenStage's own announcer). */
  announce?: (message: string) => void;
  /**
   * How a row is named for a screen reader: `sentence` reads the whole row as a sentence ("Add an assignment,
   * screen on the Screen track, starts at 30 seconds, ..."), `cells` leaves the cells to be read with their column
   * headers. Not known yet which is better with each screen reader, so both are kept.
   */
  rows?: "sentence" | "cells";
  /** The heading above the list. */
  title?: string;
}

export interface ClipListPanel {
  readonly element: HTMLElement;
  /** Put keyboard focus on the selected row. */
  focus(): void;
  /** Say something: shown, and read aloud by a screen reader. */
  announce(message: string): void;
  /** Take the panel off the page. */
  destroy(): void;
}

type Attrs = Record<string, string | number | boolean | undefined>;

function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, ...children: (Node | string)[]): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === undefined || value === false) continue;
    element.setAttribute(name, value === true ? "" : String(value));
  }
  element.append(...children);
  return element;
}

let instances = 0;

/** What the screen offers for each command: the button, its key, and what it does. */
interface Command {
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

const COMMANDS: readonly Command[] = [
  { id: "split", label: "Split at playhead", key: "S", aria: "S", match: (e) => isKey(e, "s"), run: (s) => s.splitSelected() },
  {
    id: "split-all",
    label: "Split every track at playhead",
    key: "Shift+S",
    aria: "Shift+S",
    match: (e) => isKey(e, "s", true),
    run: (s) => s.splitEveryTrack(),
  },
  { id: "trim-start", label: "Trim start to playhead", key: "B", aria: "B", match: (e) => isKey(e, "b"), run: (s) => s.trimStartToPlayhead() },
  { id: "trim-end", label: "Trim end to playhead", key: "E", aria: "E", match: (e) => isKey(e, "e"), run: (s) => s.trimEndToPlayhead() },
  {
    id: "delete-gap",
    label: "Delete, leave a gap",
    key: "Delete",
    aria: "Delete",
    match: (e) => (e.key === "Delete" || e.key === "Backspace") && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey,
    run: (s) => s.deleteSelected(false),
  },
  {
    id: "delete-close",
    label: "Delete, close the gap",
    key: "Shift+Delete",
    aria: "Shift+Delete",
    match: (e) => (e.key === "Delete" || e.key === "Backspace") && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey,
    run: (s) => s.deleteSelected(true),
  },
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

const PLAYHEAD_KEYS: Readonly<Record<string, StepKey>> = {
  ArrowLeft: "Left",
  ArrowRight: "Right",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Home: "Home",
  End: "End",
};

function isTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLTextAreaElement || target.isContentEditable) return true;
  return target instanceof HTMLInputElement && !["checkbox", "radio", "button", "submit"].includes(target.type);
}

/** Show the clip list in `container`, working on `session`. */
export function mountClipList(container: HTMLElement, session: EditSession, options: PanelOptions = {}): ClipListPanel {
  const id = `os-cl-${++instances}`;
  const sentences = (options.rows ?? "sentence") === "sentence";

  // ---- the parts ----
  const status = h(
    "p",
    { class: "os-cl-status", role: "status", "aria-live": "polite", "aria-atomic": "true" },
    "Ready. Choose a row with the Up and Down arrow keys.",
  );
  let statusTimer: number | undefined;

  const slider = h("div", {
    class: "os-cl-slider",
    role: "slider",
    tabindex: 0,
    "aria-label": "Playhead",
    "aria-valuemin": 0,
    "aria-orientation": "horizontal",
  });
  const sliderFill = h("span", { class: "os-cl-slider-fill", "aria-hidden": "true" });
  slider.append(sliderFill);
  const timeInput = h("input", {
    id: `${id}-time`,
    type: "text",
    class: "os-cl-input",
    autocomplete: "off",
    spellcheck: "false",
    "aria-describedby": `${id}-time-hint`,
  });

  const caption = h("caption", { id: `${id}-caption` });
  const headRow = h("tr", {}, ...COLUMNS.map((name) => h("th", { scope: "col" }, name)));
  const body = h("tbody");
  const table = h("table", { class: "os-cl-table", role: "grid", "aria-labelledby": `${id}-caption`, "aria-describedby": `${id}-list-hint` }, caption, h("thead", {}, headRow), body);
  const empty = h("p", { class: "os-cl-empty", tabindex: -1 }, "There are no clips yet. Record something, or add a clip, to start editing.");

  const buttons = new Map<string, HTMLButtonElement>();
  const commandGroup = h("div", { class: "os-cl-commands", role: "group", "aria-labelledby": `${id}-commands-title` });
  for (const command of COMMANDS) {
    const button = h("button", { type: "button", class: "os-cl-button", "aria-keyshortcuts": command.aria }, command.label, h("kbd", { class: "os-cl-key", "aria-hidden": "true" }, command.key));
    button.addEventListener("click", () => run(command.run(session), command.readout === true));
    buttons.set(command.id, button);
    commandGroup.append(button);
  }
  // The clip list's own keys appear in the list of keys but are not buttons.

  // The form: exact values for the selected row.
  const formTitle = h("h3", { id: `${id}-form-title`, class: "os-cl-subtitle" }, "Selected row");
  const fields = {
    name: h("input", { id: `${id}-name`, type: "text", class: "os-cl-input", autocomplete: "off" }),
    start: h("input", { id: `${id}-start`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": `${id}-time-hint` }),
    length: h("input", { id: `${id}-length`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": `${id}-time-hint` }),
    end: h("input", { id: `${id}-end`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": `${id}-time-hint` }),
  };
  const muted = h("input", { id: `${id}-muted`, type: "checkbox" });
  const mutedLabel = h("label", { class: "os-cl-check", for: `${id}-muted` }, muted, " Muted");
  const formError = h("p", { class: "os-cl-error", id: `${id}-error` });
  const form = h(
    "form",
    { class: "os-cl-form", "aria-labelledby": `${id}-form-title`, novalidate: true },
    formTitle,
    h("div", { class: "os-cl-fields" }, ...(
      [
        ["name", "Name"],
        ["start", "Start"],
        ["length", "Length"],
        ["end", "End"],
      ] as const
    ).map(([key, label]) => h("div", { class: "os-cl-field" }, h("label", { for: `${id}-${key}` }, label), fields[key]))),
    mutedLabel,
    formError,
  );

  const readout = h("p", { class: "os-cl-readout", id: `${id}-readout` });

  const helpRows = [
    ["Up and Down arrows", "Choose a row. Home and End go to the first and last row."],
    ["Left and Right arrows", "Move the playhead 0.1 second. With Shift, 1 second. Page Up and Page Down move it 5 seconds."],
    ["Enter or F2", "Edit the chosen row: go to its name and times."],
    ...COMMANDS.map((c) => [c.key, c.label] as const),
    ["Escape, in a text box", "Put the text back as it was, or if it is unchanged, return to the list."],
    ["Tab", "Always moves on to the next part. Nothing here keeps keyboard focus."],
  ] as const;
  const help = h(
    "details",
    { class: "os-cl-help" },
    h("summary", {}, "Keyboard shortcuts"),
    h("p", {}, "The list and the playhead take these keys. Letters work when the list or the playhead has focus, not inside a text box."),
    h("dl", {}, ...helpRows.flatMap(([keys, what]) => [h("dt", {}, keys), h("dd", {}, what)])),
  );

  const root = h(
    "section",
    { class: "os-cl", "aria-labelledby": `${id}-title` },
    h("h2", { id: `${id}-title`, class: "os-cl-title" }, options.title ?? "Clips"),
    status,
    h(
      "div",
      { class: "os-cl-playhead", role: "group", "aria-labelledby": `${id}-playhead-title` },
      h("h3", { id: `${id}-playhead-title`, class: "os-cl-subtitle" }, "Playhead"),
      slider,
      h("div", { class: "os-cl-field os-cl-field-inline" }, h("label", { for: `${id}-time` }, "Playhead time"), timeInput),
      h("p", { class: "os-cl-hint", id: `${id}-time-hint` }, "Type a time such as 72.4 (seconds), 1:12.4 (minutes and seconds) or 1m12.4s, then press Enter."),
      h(
        "div",
        { class: "os-cl-row-buttons" },
        ...(
          [
            ["Playhead to start of row", () => session.playheadToSelectedStart()],
            ["Playhead to end of row", () => session.playheadToSelectedEnd()],
          ] as const
        ).map(([label, action]) => {
          const button = h("button", { type: "button", class: "os-cl-button" }, label);
          button.addEventListener("click", () => run(action()));
          return button;
        }),
      ),
    ),
    h("p", { class: "os-cl-hint", id: `${id}-list-hint` }, "Use the Up and Down arrow keys to choose a row. The keys are listed under Keyboard shortcuts."),
    h("div", { class: "os-cl-tablewrap" }, table, empty),
    h("h3", { id: `${id}-commands-title`, class: "os-cl-subtitle" }, "Edit the chosen row"),
    commandGroup,
    form,
    readout,
    help,
  );
  container.append(root);

  // ---- keeping the screen in step with the session ----

  const rowElements = new Map<string, HTMLTableRowElement>();

  function say(message: string): void {
    if (statusTimer !== undefined) window.clearTimeout(statusTimer);
    // Cleared first, and filled a moment later, so the same sentence said twice is still read, and so it comes
    // after whatever a screen reader says for a change of focus rather than being talked over by it.
    status.textContent = "";
    statusTimer = window.setTimeout(() => {
      status.textContent = message;
      statusTimer = undefined;
    }, 60);
    options.announce?.(message);
  }

  function run(outcome: Outcome, keep = false): void {
    say(outcome.message);
    if (keep && outcome.ok) readout.textContent = outcome.message;
  }

  function cell(tag: "td" | "th", text: string, extra: Attrs = {}): HTMLElement {
    return h(tag, extra, text);
  }

  function buildRow(row: Row): HTMLTableRowElement {
    const selected = row.clipId === session.selectedId;
    const tr = h(
      "tr",
      {
        role: "row",
        tabindex: selected ? 0 : -1,
        "aria-selected": selected ? "true" : "false",
        "data-id": row.clipId,
        ...(sentences ? { "aria-label": session.describeRow(row.clipId) } : {}),
      },
      cell("th", row.name, { scope: "row" }),
      cell("td", row.trackName),
      cell("td", row.type),
      cell("td", formatTimecode(row.startMs), { class: "os-cl-time" }),
      cell("td", formatTimecode(row.lengthMs), { class: "os-cl-time" }),
      cell("td", formatTimecode(row.endMs), { class: "os-cl-time" }),
      cell("td", row.state, row.state === "ready" || row.state === "muted" ? {} : { class: "os-cl-attention" }),
    );
    return tr;
  }

  function renderRows(): void {
    const hadFocus = body.contains(document.activeElement) || document.activeElement === empty;
    rowElements.clear();
    const rows = session.rows;
    const built = rows.map((row) => {
      const tr = buildRow(row);
      rowElements.set(row.clipId, tr);
      return tr;
    });
    body.replaceChildren(...built);
    caption.textContent = session.caption;
    table.hidden = rows.length === 0;
    empty.hidden = rows.length !== 0;
    markPlayhead();
    if (hadFocus) focusSelected();
  }

  /** Rows the playhead is inside get a marker (a dashed edge, not only a colour). */
  function markPlayhead(): void {
    const at = session.playheadMs;
    for (const row of session.rows) {
      rowElements.get(row.clipId)?.classList.toggle("os-cl-at-playhead", row.startMs <= at && at < row.endMs);
    }
  }

  function renderSelection(): void {
    for (const [rowId, tr] of rowElements) {
      const selected = rowId === session.selectedId;
      tr.tabIndex = selected ? 0 : -1;
      tr.setAttribute("aria-selected", selected ? "true" : "false");
    }
    syncForm();
    syncPlayhead();
  }

  function setField(input: HTMLInputElement, value: string): void {
    const editing = document.activeElement === input && input.value !== (input.dataset["shown"] ?? input.value);
    input.dataset["shown"] = value;
    if (!editing) {
      input.value = value;
      input.removeAttribute("aria-invalid");
      input.removeAttribute("aria-errormessage");
    }
  }

  function syncForm(): void {
    const details = session.details;
    form.hidden = details === undefined;
    if (details === undefined) return;
    formTitle.textContent = `Selected row: ${details.name}`;
    setField(fields.name, details.name);
    setField(fields.start, formatTimecode(details.startMs));
    setField(fields.length, formatTimecode(details.lengthMs));
    setField(fields.end, formatTimecode(details.endMs));
    mutedLabel.hidden = !details.isClip;
    muted.checked = details.muted;
    if (!Object.values(fields).some((f) => f.hasAttribute("aria-invalid"))) formError.textContent = "";
    const muteButton = buttons.get("mute");
    if (muteButton?.firstChild) muteButton.firstChild.textContent = details.isClip ? (details.muted ? "Unmute clip" : "Mute clip") : "Mute or unmute clip";
  }

  function syncPlayhead(): void {
    const length = session.lengthMs;
    const at = session.playheadMs;
    slider.setAttribute("aria-valuemax", String(length));
    slider.setAttribute("aria-valuenow", String(at));
    slider.setAttribute("aria-valuetext", session.playheadText);
    sliderFill.style.width = length === 0 ? "0%" : `${(at / length) * 100}%`;
    setField(timeInput, formatTimecode(at));
  }

  function focusSelected(): void {
    const selected = session.selectedId === undefined ? undefined : rowElements.get(session.selectedId);
    if (selected !== undefined) selected.focus();
    else empty.focus();
  }

  const stop = session.subscribe((kind) => {
    if (kind === "model") {
      renderRows();
      syncForm();
      syncPlayhead();
    } else if (kind === "selection") {
      renderSelection();
      markPlayhead();
    } else {
      syncPlayhead();
      markPlayhead();
    }
  });

  // ---- keys ----

  /** The commands that work from the list and from the playhead. Returns whether the key was one of them. */
  function commandKey(e: KeyboardEvent): boolean {
    for (const command of COMMANDS) {
      if (command.match(e)) {
        e.preventDefault();
        run(command.run(session), command.readout === true);
        return true;
      }
    }
    return false;
  }

  body.addEventListener("keydown", (e) => {
    if (e.defaultPrevented) return;
    const key = e.key;
    if (!e.ctrlKey && !e.metaKey && !e.altKey) {
      const rowMove = { ArrowUp: "previous", ArrowDown: "next", Home: "first", End: "last" } as const;
      if (Object.hasOwn(rowMove, key)) {
        e.preventDefault();
        const moved = session.moveSelection(rowMove[key as keyof typeof rowMove]);
        focusSelected();
        if (!moved.changed) say(moved.message);
        return;
      }
      if (key === "ArrowLeft" || key === "ArrowRight" || key === "PageUp" || key === "PageDown") {
        e.preventDefault();
        run(session.stepPlayhead(PLAYHEAD_KEYS[key] ?? "Right", e.shiftKey));
        return;
      }
      if (key === "Enter" || key === "F2") {
        e.preventDefault();
        fields.name.focus();
        fields.name.select();
        return;
      }
    }
    commandKey(e);
  });

  body.addEventListener("focusin", (e) => {
    const tr = (e.target as HTMLElement).closest<HTMLTableRowElement>("tr[data-id]");
    const rowId = tr?.dataset["id"];
    if (rowId !== undefined && rowId !== session.selectedId) session.select(rowId);
  });

  slider.addEventListener("keydown", (e) => {
    if (e.defaultPrevented) return;
    if (!e.ctrlKey && !e.metaKey && !e.altKey && Object.hasOwn(PLAYHEAD_KEYS, e.key)) {
      e.preventDefault();
      // The slider's own value is read by the screen reader as it changes, so this is not said again.
      session.stepPlayhead(PLAYHEAD_KEYS[e.key] ?? "Right", e.shiftKey);
      return;
    }
    commandKey(e);
  });

  // Undo and redo work from anywhere in the panel except a text box, where they undo the typing.
  root.addEventListener("keydown", (e) => {
    if (e.defaultPrevented || isTextField(e.target)) return;
    const undoOrRedo = COMMANDS.filter((c) => c.id === "undo" || c.id === "redo").find((c) => c.match(e));
    if (undoOrRedo !== undefined) {
      e.preventDefault();
      run(undoOrRedo.run(session));
    }
  });

  // ---- the form and the playhead time ----

  /** Wire a text box so Enter or leaving it applies what was typed, and Escape puts the old text back. */
  function commitOnLeave(input: HTMLInputElement, apply: (text: string) => Outcome, current: () => string): void {
    let tried: string | undefined;
    const commit = (): void => {
      const text = input.value;
      if (text === current() || text === tried) return;
      const outcome = apply(text);
      if (outcome.ok) {
        tried = undefined;
        input.removeAttribute("aria-invalid");
        formError.textContent = "";
        input.removeAttribute("aria-errormessage");
        // The sentence is said once; the box shows the exact value now held.
        input.value = current();
      } else {
        tried = text;
        input.setAttribute("aria-invalid", "true");
        input.setAttribute("aria-errormessage", formError.id);
        formError.textContent = outcome.message;
      }
      say(outcome.message);
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        commit();
      } else if (e.key === "Escape") {
        e.preventDefault();
        if (input.value !== current()) {
          input.value = current();
          tried = undefined;
          input.removeAttribute("aria-invalid");
          formError.textContent = "";
          say("Put back as it was.");
        } else {
          focusSelected();
        }
      }
    });
    input.addEventListener("change", commit);
    input.addEventListener("input", () => {
      tried = undefined;
    });
  }

  commitOnLeave(fields.name, (t) => session.rename(t), () => session.details?.name ?? "");
  commitOnLeave(fields.start, (t) => session.setStartText(t), () => formatTimecode(session.details?.startMs ?? 0));
  commitOnLeave(fields.length, (t) => session.setLengthText(t), () => formatTimecode(session.details?.lengthMs ?? 0));
  commitOnLeave(fields.end, (t) => session.setEndText(t), () => formatTimecode(session.details?.endMs ?? 0));
  commitOnLeave(timeInput, (t) => session.setPlayheadText(t), () => formatTimecode(session.playheadMs));
  muted.addEventListener("change", () => run(session.setSelectedMuted(muted.checked)));
  form.addEventListener("submit", (e) => e.preventDefault());

  // ---- first draw ----
  renderRows();
  syncForm();
  syncPlayhead();

  return {
    element: root,
    focus: focusSelected,
    announce: say,
    destroy(): void {
      stop();
      if (statusTimer !== undefined) window.clearTimeout(statusTimer);
      root.remove();
    },
  };
}
