// SPDX-License-Identifier: Apache-2.0
/**
 * The clip list on screen (stories C1 and C2): a table of every clip, overlay and zoom that is fully operable from
 * the keyboard and says what it does aloud, with the playhead, the commands, and a form for exact values.
 *
 * This file is only the screen. What each command does, and the sentence that says so, is in `session.ts`, which is
 * tested without a browser. Plain DOM, no libraries, written the way OpenStage's Timeline panel is, so the two can be
 * joined later. The timeline (`timeline-panel.ts`) shows the same session; the two always make the same changes.
 *
 * Keyboard (also listed on the page, from the same table that makes the buttons, so the two cannot drift):
 *   the list      Up and Down choose a row, Home and End go to the first and last row; with Shift they choose a range.
 *                 Ctrl+Space adds a row to the choice or takes it out, Ctrl+A chooses every row, Escape keeps just
 *                 the row you are on. Left and Right move the playhead 0.1 second (Shift: 1 second), Page Up and
 *                 Page Down 5 seconds. Enter or F2 edits the row.
 *   the playhead  as a slider: the same keys, with Home and End going to the start and end of the edit.
 *   from either   the commands in `commands.ts`: S, Shift+S, B, E, J, M, D, Delete, Shift+Delete, Alt+Up, Alt+Down,
 *                 Ctrl+Z, Ctrl+Y.
 *
 * Nothing is ever disabled (a disabled button leaves the Tab order and takes focus with it); a command that cannot
 * be done says why instead. Focus never gets stuck: Tab and Shift+Tab always move on, and Escape in a text box puts
 * it back on the list.
 *
 * Not yet checked with NVDA, Narrator or VoiceOver. How a screen reader reads the list, and whether the sentence on
 * each row (`aria-label`) or the cells read better, is what the first test with one has to settle.
 */

import { COLUMNS, formatTimecode, type Row, type StepKey } from "../../capture-edit/src/index.ts";
import { Announcer } from "./announcer.ts";
import { COMMANDS, commandFor, isTextField } from "./commands.ts";
import { h, nextId, type Attrs } from "./dom.ts";
import type { EditSession, Outcome } from "./session.ts";

export interface PanelOptions {
  /** Where messages are said. Without it the panel makes its own line at the top. */
  announcer?: Announcer;
  /** Also tell this function every message, when the panel makes its own announcer. */
  announce?: (message: string) => void;
  /**
   * How a row is named for a screen reader: `sentence` reads the whole row as a sentence ("Add an assignment,
   * screen on the Screen track, starts at 30 seconds, ..."), `cells` leaves the cells to be read with their column
   * headers. Not known yet which is better with each screen reader, so both are kept.
   */
  rows?: "sentence" | "cells";
  /** The heading above the list. */
  title?: string;
  /** Show the playhead slider and time box (default). The timeline has its own, so a page with both leaves these out. */
  playhead?: boolean;
}

export interface ClipListPanel {
  readonly element: HTMLElement;
  /** Put keyboard focus on the chosen row. */
  focus(): void;
  /** Put keyboard focus on the first box of the form for the chosen row. */
  focusDetails(): void;
  /** Say something: shown, and read aloud by a screen reader. */
  announce(message: string): void;
  /** Take the panel off the page. */
  destroy(): void;
}

const PLAYHEAD_KEYS: Readonly<Record<string, StepKey>> = {
  ArrowLeft: "Left",
  ArrowRight: "Right",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Home: "Home",
  End: "End",
};

/** Show the clip list in `container`, working on `session`. */
export function mountClipList(container: HTMLElement, session: EditSession, options: PanelOptions = {}): ClipListPanel {
  const id = nextId("os-cl");
  const sentences = (options.rows ?? "sentence") === "sentence";
  const ownAnnouncer = options.announcer === undefined;
  const announcer = options.announcer ?? new Announcer(options.announce);
  const showPlayhead = options.playhead !== false;

  // ---- the parts ----

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
  const table = h(
    "table",
    { class: "os-cl-table", role: "grid", "aria-multiselectable": "true", "aria-labelledby": `${id}-caption`, "aria-describedby": `${id}-list-hint` },
    caption,
    h("thead", {}, headRow),
    body,
  );
  const empty = h("p", { class: "os-cl-empty", tabindex: -1 }, "There are no clips yet. Record something, or add a clip, to start editing.");

  const buttons = new Map<string, HTMLButtonElement>();
  const commandGroup = h("div", { class: "os-cl-commands", role: "group", "aria-labelledby": `${id}-commands-title` });
  for (const command of COMMANDS) {
    const button = h("button", { type: "button", class: "os-cl-button", "aria-keyshortcuts": command.aria }, command.label, h("kbd", { class: "os-cl-key", "aria-hidden": "true" }, command.key));
    button.addEventListener("click", () => run(command.run(session), command.readout === true));
    buttons.set(command.id, button);
    commandGroup.append(button);
  }

  // The form: exact values for the row with the focus.
  const formTitle = h("h3", { id: `${id}-form-title`, class: "os-cl-subtitle" }, "Chosen row");
  const timeHint = `${id}-time-hint`;
  const fields = {
    name: h("input", { id: `${id}-name`, type: "text", class: "os-cl-input", autocomplete: "off" }),
    start: h("input", { id: `${id}-start`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": timeHint }),
    length: h("input", { id: `${id}-length`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": timeHint }),
    end: h("input", { id: `${id}-end`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": timeHint }),
    volume: h("input", { id: `${id}-volume`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": `${id}-volume-hint` }),
    fadeIn: h("input", { id: `${id}-fadeIn`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": timeHint }),
    fadeOut: h("input", { id: `${id}-fadeOut`, type: "text", class: "os-cl-input os-cl-time", autocomplete: "off", spellcheck: "false", "aria-describedby": timeHint }),
  };
  const trackSelect = h("select", { id: `${id}-track`, class: "os-cl-input" });
  const muted = h("input", { id: `${id}-muted`, type: "checkbox" });
  const mutedLabel = h("label", { class: "os-cl-check", for: `${id}-muted` }, muted, " Muted");
  const formError = h("p", { class: "os-cl-error", id: `${id}-error` });
  const field = (key: keyof typeof fields, label: string) => h("div", { class: "os-cl-field" }, h("label", { for: `${id}-${key}` }, label), fields[key]);
  const soundFields = h(
    "div",
    { class: "os-cl-fields os-cl-sound" },
    field("volume", "Volume (decibels)"),
    field("fadeIn", "Fade in"),
    field("fadeOut", "Fade out"),
    h("p", { class: "os-cl-hint", id: `${id}-volume-hint` }, "Volume: a whole number from -40 to 20. 0 is as recorded; -6 is about half as loud."),
  );
  const timeFields = h("div", { class: "os-cl-fields" }, field("name", "Name"), field("start", "Start"), field("length", "Length"), field("end", "End"));
  const trackField = h("div", { class: "os-cl-field" }, h("label", { for: `${id}-track` }, "Track"), trackSelect);
  const manyNote = h("p", { class: "os-cl-hint", id: `${id}-many` });
  const form = h(
    "form",
    { class: "os-cl-form", "aria-labelledby": `${id}-form-title`, novalidate: true },
    formTitle,
    manyNote,
    timeFields,
    h("div", { class: "os-cl-fields" }, trackField),
    soundFields,
    mutedLabel,
    formError,
  );

  const readout = h("p", { class: "os-cl-readout", id: `${id}-readout` });

  const helpRows = [
    ["Up and Down arrows", "Choose a row. Home and End go to the first and last row. With Shift, choose a range of rows."],
    ["Ctrl with the arrow keys", "Move without changing which rows are chosen. Then Ctrl+Space adds the row you are on to the choice, or takes it out, so rows that are not next to each other can be chosen."],
    ["Ctrl+Space", "Add the row you are on to the choice, or take it out."],
    ["Ctrl+A", "Choose every row."],
    ["Escape", "Keep just the row you are on."],
    ["Left and Right arrows", "Move the playhead 0.1 second. With Shift, 1 second. Page Up and Page Down move it 5 seconds."],
    ["Enter or F2", "Edit the row you are on: go to its name and times."],
    ...COMMANDS.map((c) => [c.key, c.label] as const),
    ["Escape, in a text box", "Put the text back as it was, or if it is unchanged, return to the list."],
    ["Tab", "Always moves on to the next part. Nothing here keeps keyboard focus."],
  ] as const;
  const help = h(
    "details",
    { class: "os-cl-help" },
    h("summary", {}, "Keyboard shortcuts for the list"),
    h("p", {}, "The list and the playhead take these keys. Letters work when the list or the playhead has focus, not inside a text box. Split, trim, delete and mute act on every chosen row; the other commands on one."),
    h("dl", {}, ...helpRows.flatMap(([keys, what]) => [h("dt", {}, keys), h("dd", {}, what)])),
  );

  const playheadGroup = h(
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
  );
  // The time hint is used by the form's time boxes too, so it is always on the page, even when the playhead is left out.
  const timeHintOnly = h("p", { class: "os-cl-hint", id: timeHint }, "Type a time such as 72.4 (seconds), 1:12.4 (minutes and seconds) or 1m12.4s, then press Enter or leave the box.");

  const root = h(
    "section",
    { class: "os-cl", "aria-labelledby": `${id}-title` },
    h("h2", { id: `${id}-title`, class: "os-cl-title" }, options.title ?? "Clip list"),
    ownAnnouncer ? announcer.element : "",
    showPlayhead ? playheadGroup : "",
    h("p", { class: "os-cl-hint", id: `${id}-list-hint` }, "Use the Up and Down arrow keys to choose a row. The keys are listed under Keyboard shortcuts."),
    h("div", { class: "os-cl-tablewrap" }, table, empty),
    h("h3", { id: `${id}-commands-title`, class: "os-cl-subtitle" }, "Edit the chosen rows"),
    commandGroup,
    form,
    showPlayhead ? "" : timeHintOnly,
    readout,
    help,
  );
  container.append(root);

  // ---- keeping the screen in step with the session ----

  const rowElements = new Map<string, HTMLTableRowElement>();
  let lastMark = 0;

  function say(message: string): void {
    announcer.say(message);
  }

  function run(outcome: Outcome, keep = false): void {
    say(outcome.message);
    if (keep && outcome.ok) readout.textContent = outcome.message;
  }

  function cell(tag: "td" | "th", text: string, extra: Attrs = {}): HTMLElement {
    return h(tag, extra, text);
  }

  function buildRow(row: Row): HTMLTableRowElement {
    const primary = row.clipId === session.selectedId;
    return h(
      "tr",
      {
        role: "row",
        tabindex: primary ? 0 : -1,
        "aria-selected": session.isSelected(row.clipId) ? "true" : "false",
        class: primary ? "os-cl-primary" : undefined,
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
      const primary = rowId === session.selectedId;
      tr.tabIndex = primary ? 0 : -1;
      tr.setAttribute("aria-selected", session.isSelected(rowId) ? "true" : "false");
      tr.classList.toggle("os-cl-primary", primary);
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
    const count = session.selectionCount;
    form.hidden = details === undefined;
    if (details === undefined) return;
    if (count > 1 || !session.singleChoice) {
      formTitle.textContent = count > 1 ? `${count} rows chosen` : "Chosen row";
      manyNote.textContent =
        count > 1
          ? `${count} rows are chosen, so there is no form. Split, trim, delete and mute act on all of them. Press Escape to keep just ${details.name}, and edit its name and times here.`
          : `${details.name} has the focus but is not the chosen row, so there is no form. Press Ctrl+Space to choose it, or Escape to keep just it.`;
      manyNote.hidden = false;
      timeFields.hidden = true;
      trackField.hidden = true;
      soundFields.hidden = true;
      mutedLabel.hidden = true;
      return;
    }
    manyNote.hidden = true;
    timeFields.hidden = false;
    trackField.hidden = false;
    formTitle.textContent = `Chosen row: ${details.name}`;
    setField(fields.name, details.name);
    setField(fields.start, formatTimecode(details.startMs));
    setField(fields.length, formatTimecode(details.lengthMs));
    setField(fields.end, formatTimecode(details.endMs));
    soundFields.hidden = !details.isClip;
    mutedLabel.hidden = !details.isClip;
    setField(fields.volume, String(details.volumeDb));
    setField(fields.fadeIn, formatTimecode(details.fadeInMs));
    setField(fields.fadeOut, formatTimecode(details.fadeOutMs));
    muted.checked = details.muted;
    // The Track box offers the tracks that take this row; it is rebuilt only when the choices change.
    const choices = session.trackChoices;
    const signature = choices.map((c) => `${c.id}:${c.name}:${c.current}`).join("|");
    if (trackSelect.dataset["choices"] !== signature) {
      trackSelect.dataset["choices"] = signature;
      trackSelect.replaceChildren(...choices.map((c) => h("option", { value: c.id, ...(c.current ? { selected: true } : {}) }, c.name)));
    }
    const current = choices.find((c) => c.current);
    if (current !== undefined) trackSelect.value = current.id;
    if (!Object.values(fields).some((f) => f.hasAttribute("aria-invalid"))) formError.textContent = "";
    const muteButton = buttons.get("mute");
    if (muteButton?.firstChild) muteButton.firstChild.textContent = details.isClip ? (details.muted ? "Unmute clip" : "Mute clip") : "Mute or unmute clip";
  }

  function syncPlayhead(): void {
    const length = session.lengthMs;
    const at = session.playheadMs;
    if (!session.playing) {
      slider.setAttribute("aria-valuemax", String(length));
      slider.setAttribute("aria-valuenow", String(at));
      slider.setAttribute("aria-valuetext", session.playheadText);
    }
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
    } else if (kind === "playhead" || kind === "transport") {
      // While it plays, the rows are marked a few times a second, not sixty.
      const now = performance.now();
      if (kind === "playhead" && session.playing && now - lastMark < 250) return;
      lastMark = now;
      syncPlayhead();
      markPlayhead();
    }
    // `drag` (a change under a drag that is not shown here) and `saved` leave the list alone.
  });

  // ---- keys ----

  /** The commands that work from the list and from the playhead. Returns whether the key was one of them. */
  function commandKey(e: KeyboardEvent): boolean {
    const command = commandFor(e);
    if (command === undefined) return false;
    e.preventDefault();
    run(command.run(session), command.readout === true);
    return true;
  }

  body.addEventListener("keydown", (e) => {
    if (e.defaultPrevented) return;
    const key = e.key;
    const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
    if (plain) {
      const rowMove = { ArrowUp: "previous", ArrowDown: "next", Home: "first", End: "last" } as const;
      if (Object.hasOwn(rowMove, key)) {
        e.preventDefault();
        const moved = session.moveSelection(rowMove[key as keyof typeof rowMove], e.shiftKey);
        focusSelected();
        if (!moved.changed) say(moved.message);
        else if (e.shiftKey) say(moved.message);
        return;
      }
      if (key === "ArrowLeft" || key === "ArrowRight" || key === "PageUp" || key === "PageDown") {
        e.preventDefault();
        run(session.stepPlayhead(PLAYHEAD_KEYS[key] ?? "Right", e.shiftKey));
        return;
      }
      if (key === "Enter" || key === "F2") {
        e.preventDefault();
        focusDetails();
        return;
      }
      if (key === "Escape") {
        const moved = session.keepOnlyPrimary();
        if (moved.changed) {
          e.preventDefault();
          say(moved.message);
        }
        return;
      }
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) {
      const focusMove = { ArrowUp: "previous", ArrowDown: "next", Home: "first", End: "last" } as const;
      if (Object.hasOwn(focusMove, key)) {
        e.preventDefault();
        const moved = session.moveFocus(focusMove[key as keyof typeof focusMove]);
        focusSelected();
        if (!moved.changed) say(moved.message);
        return;
      }
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey) {
      if (key.toLowerCase() === "a") {
        e.preventDefault();
        say(session.selectAll().message);
        return;
      }
      if (key === " ") {
        e.preventDefault();
        say(session.toggleSelected().message);
        return;
      }
    }
    commandKey(e);
  });

  body.addEventListener("focusin", (e) => {
    const tr = (e.target as HTMLElement).closest<HTMLTableRowElement>("tr[data-id]");
    const rowId = tr?.dataset["id"];
    // Focus arriving by a click or Tab on a row that is not the focused one makes it the only chosen row. Arrow keys,
    // Ctrl+Space and the like have already made their choice before they move the focus, so they are not undone here.
    if (rowId !== undefined && rowId !== session.selectedId) session.select(rowId);
  });

  // Ctrl or Shift with the mouse adds to the choice or chooses a range; it is done before the focus moves, so the
  // focus arriving does not undo it.
  body.addEventListener("mousedown", (e) => {
    const rowId = (e.target as HTMLElement).closest<HTMLTableRowElement>("tr[data-id]")?.dataset["id"];
    if (rowId === undefined) return;
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      say(session.toggleSelected(rowId).message);
      focusSelected();
    } else if (e.shiftKey) {
      e.preventDefault();
      say(session.selectRange(rowId).message);
      focusSelected();
    }
  });

  // A plain click on a row that already has the focus, with several chosen, makes it the only one.
  body.addEventListener("click", (e) => {
    const rowId = (e.target as HTMLElement).closest<HTMLTableRowElement>("tr[data-id]")?.dataset["id"];
    if (rowId !== undefined && !e.ctrlKey && !e.metaKey && !e.shiftKey && session.selectionCount > 1) session.select(rowId);
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
    const command = commandFor(e);
    if (command !== undefined && (command.id === "undo" || command.id === "redo")) {
      e.preventDefault();
      run(command.run(session));
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
  commitOnLeave(fields.volume, (t) => session.setVolumeText(t), () => String(session.details?.volumeDb ?? 0));
  commitOnLeave(fields.fadeIn, (t) => session.setFadeInText(t), () => formatTimecode(session.details?.fadeInMs ?? 0));
  commitOnLeave(fields.fadeOut, (t) => session.setFadeOutText(t), () => formatTimecode(session.details?.fadeOutMs ?? 0));
  commitOnLeave(timeInput, (t) => session.setPlayheadText(t), () => formatTimecode(session.playheadMs));
  muted.addEventListener("change", () => run(session.setSelectedMuted(muted.checked)));
  trackSelect.addEventListener("change", () => {
    const outcome = session.moveSelectedToTrack(trackSelect.value);
    say(outcome.message);
    if (!outcome.ok) syncForm();
  });
  trackSelect.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      focusSelected();
    }
  });
  form.addEventListener("submit", (e) => e.preventDefault());

  function focusDetails(): void {
    if (form.hidden || timeFields.hidden) {
      focusSelected();
      return;
    }
    fields.name.focus();
    fields.name.select();
  }

  // ---- first draw ----
  renderRows();
  syncForm();
  syncPlayhead();

  return {
    element: root,
    focus: focusSelected,
    focusDetails,
    announce: say,
    destroy(): void {
      stop();
      if (ownAnnouncer) announcer.destroy();
      root.remove();
    },
  };
}
