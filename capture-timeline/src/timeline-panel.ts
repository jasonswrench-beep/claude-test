// SPDX-License-Identifier: Apache-2.0
/**
 * The timeline on screen (story C3): the tracks and their clips drawn along a time axis, with a playhead, a zoom, and
 * clips that can be chosen, moved and trimmed by keyboard or by mouse. It shows the same session as the clip list
 * (`panel.ts`), so the two always make the same changes: nothing here is possible only with the mouse.
 *
 * How it is built for a screen reader and for a keyboard:
 *  - The clips are one `listbox` of `option`s, grouped by track (`group`), and the whole of it is one Tab stop: Left and
 *    Right choose the clip before or after in the track, Up and Down the track above or below, Home and End the first
 *    and last in the track. Each option is named by the same sentence as its row in the clip list.
 *  - The ruler is the playhead, a `slider`. Its spoken value says where it is and how that relates to the chosen clip.
 *  - The chosen clip has two real sliders, its start grip and its end grip (as in OpenStage's Timeline): Left and Right
 *    move a tenth of a second, with Shift a second, Page Up and Page Down five seconds, Home and End go to the ends of what
 *    the edit allows. A key held down is one undo step; Escape puts it back.
 *  - A mouse drag moves a clip (and, downwards or upwards onto another track that takes it, moves it to that track) or a
 *    grip. The whole drag is one undo step and Escape cancels it.
 *  - Zoom has buttons and keys; only the marks in view are drawn, so an hour-long edit stays quick.
 *
 * Not yet checked with NVDA, Narrator or VoiceOver. Whether a listbox of clips, grouped by track, reads well, and whether
 * the sliders announce clearly, is what the first test with one has to settle. The timeline must not ship until it has.
 */

import {
  describeLength,
  describeTime,
  formatTimecode,
  limitsOf,
  trackKindName,
  type Limits,
  type Row,
  type StepKey,
  type TrackKind,
} from "../../capture-edit/src/index.ts";
import { Announcer } from "./announcer.ts";
import { COMMANDS, commandFor, isTextField } from "./commands.ts";
import { h, nextId } from "./dom.ts";
import { contentMs, fitScale, msAtX, navigate, tickStepMs, ticksBetween, tidy, visibleSpanMs, zoomed, type Direction } from "./geometry.ts";
import type { DragKind, EditSession, Outcome } from "./session.ts";

export interface TimelineOptions {
  /** Where messages are said. Without it the panel makes its own line at the top. */
  announcer?: Announcer;
  /** Also tell this function every message, when the panel makes its own announcer. */
  announce?: (message: string) => void;
  /** The heading above the timeline. */
  title?: string;
  /** Called when the author asks to edit the chosen clip's details (Enter or F2). */
  onEdit?: () => void;
}

export interface TimelinePanel {
  readonly element: HTMLElement;
  /** Put keyboard focus on the chosen clip. */
  focus(): void;
  announce(message: string): void;
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

const NAVIGATION: Readonly<Record<string, Direction>> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
  Home: "first",
  End: "last",
};

/** The keys that move the playhead from the clips: a tenth of a second, a second with Shift, five seconds. */
const PLAYHEAD_FROM_CLIPS: Readonly<Record<string, readonly [StepKey, boolean]>> = {
  ",": ["Left", false],
  ".": ["Right", false],
  "<": ["Left", true],
  ">": ["Right", true],
  PageUp: ["PageUp", false],
  PageDown: ["PageDown", false],
};

const KEY_DRAG_IDLE_MS = 500;

const TRACK_KINDS: readonly TrackKind[] = ["screen", "webcam", "audio", "media", "overlay", "zoom"];

export function mountTimeline(container: HTMLElement, session: EditSession, options: TimelineOptions = {}): TimelinePanel {
  const id = nextId("os-tl");
  const ownAnnouncer = options.announcer === undefined;
  const announcer = options.announcer ?? new Announcer(options.announce);

  // ---- the parts ----

  const ruler = h("div", {
    class: "os-tl-ruler",
    role: "slider",
    tabindex: 0,
    "aria-label": "Playhead",
    "aria-valuemin": 0,
    "aria-orientation": "horizontal",
    "aria-describedby": `${id}-ruler-hint`,
  });
  const corner = h("div", { class: "os-tl-corner", "aria-hidden": "true" }, "Time");
  const lanesEl = h("div", { class: "os-tl-lanes", role: "listbox", "aria-label": "Clips on the timeline", "aria-multiselectable": "true", "aria-describedby": `${id}-lanes-hint` });
  const gripsEl = h("div", { class: "os-tl-grips" });
  const playline = h("div", { class: "os-tl-playline", "aria-hidden": "true" });
  const canvas = h("div", { class: "os-tl-canvas" }, h("div", { class: "os-tl-rulerrow" }, corner, ruler), lanesEl, gripsEl, playline);
  const scroller = h("div", { class: "os-tl-scroll" }, canvas);
  const empty = h("p", { class: "os-cl-empty", tabindex: -1 }, "There are no clips yet. Record something, or add a clip, to start editing.");

  const grips = {
    start: h("div", { class: "os-tl-grip os-tl-grip-start", role: "slider", tabindex: 0, "data-grip": "start", "aria-orientation": "horizontal" }),
    end: h("div", { class: "os-tl-grip os-tl-grip-end", role: "slider", tabindex: 0, "data-grip": "end", "aria-orientation": "horizontal" }),
  };
  gripsEl.append(grips.start, grips.end);

  const timeInput = h("input", { id: `${id}-time`, type: "text", class: "os-cl-input", autocomplete: "off", spellcheck: "false", "aria-describedby": `${id}-time-hint` });
  const error = h("p", { class: "os-cl-error", id: `${id}-error` });
  const kindSelect = h("select", { id: `${id}-kind`, class: "os-cl-input" }, ...TRACK_KINDS.map((k) => h("option", { value: k }, trackKindName(k))));
  const readout = h("p", { class: "os-cl-readout" });

  const button = (label: string, action: () => void, extra: string = ""): HTMLButtonElement => {
    const b = h("button", { type: "button", class: "os-cl-button" }, label, extra === "" ? "" : h("kbd", { class: "os-cl-key", "aria-hidden": "true" }, extra));
    b.addEventListener("click", action);
    return b;
  };
  const zoomInButton = button("Zoom in", () => zoomBy("in"), "+");
  const zoomOutButton = button("Zoom out", () => zoomBy("out"), "-");
  const fitButton = button("Show the whole edit", () => zoomFit(), "0");
  const describeButton = button("Describe what is at the playhead", () => run(session.describePlayhead(), true), "D");
  const addTrackButton = button("Add track", () => run(session.addTrack(kindSelect.value as TrackKind)));

  const helpRows = [
    ["Left and Right arrows", "Choose the clip before or after, in the same track. With Shift, choose a range of clips."],
    ["Up and Down arrows", "Choose the clip showing at the same time in the track above or below."],
    ["Home and End", "Choose the first or last clip in the track."],
    ["Ctrl with the arrow keys", "Move without changing which clips are chosen. Then Ctrl+Space adds the clip you are on to the choice, or takes it out, so clips that are not next to each other can be chosen."],
    ["Ctrl+Space", "Add the clip you are on to the choice, or take it out."],
    ["Ctrl+A", "Choose every clip."],
    ["Escape", "Keep just the clip you are on."],
    ["Comma and period", "Move the playhead 0.1 second. With Shift (less than and greater than signs), 1 second. Page Up and Page Down move it 5 seconds."],
    ["Plus, minus and 0", "Zoom in, zoom out, and show the whole edit."],
    ["Enter or F2", "Edit the clip you are on: go to its name and times in the clip list."],
    ["On a start or end grip", "Left and Right move it 0.1 second, with Shift 1 second, Page Up and Page Down 5 seconds, Home and End to the ends of what is allowed. Escape puts it back."],
    ["On the playhead", "The same keys move the playhead. Home and End go to the start and end of the edit."],
    ...COMMANDS.map((c) => [c.key, c.label] as const),
    ["Mouse", "Drag a clip to move it, or onto another track that takes it to move it there. Drag a start or end grip to trim. Click the ruler or an empty part of a track to put the playhead there. Escape cancels a drag; one drag is one undo step."],
  ] as const;
  const help = h(
    "details",
    { class: "os-cl-help" },
    h("summary", {}, "Keyboard shortcuts for the timeline"),
    h("p", {}, "Letters work when a clip, a grip or the playhead has focus, not inside a text box. Split, trim, delete and mute act on every chosen clip."),
    h("dl", {}, ...helpRows.flatMap(([keys, what]) => [h("dt", {}, keys), h("dd", {}, what)])),
  );

  const controls = h(
    "div",
    { class: "os-tl-controls", role: "group", "aria-label": "Playhead, zoom and tracks" },
    h("div", { class: "os-cl-field os-cl-field-inline" }, h("label", { for: `${id}-time` }, "Playhead time"), timeInput),
    h("div", { class: "os-cl-row-buttons" }, zoomInButton, zoomOutButton, fitButton, describeButton),
    h("div", { class: "os-tl-addtrack" }, h("div", { class: "os-cl-field" }, h("label", { for: `${id}-kind` }, "Kind of track to add"), kindSelect), addTrackButton),
  );

  const root = h(
    "section",
    { class: "os-cl os-tl", "aria-labelledby": `${id}-title` },
    h("h2", { id: `${id}-title`, class: "os-cl-title" }, options.title ?? "Timeline"),
    ownAnnouncer ? announcer.element : "",
    controls,
    h("p", { class: "os-cl-hint", id: `${id}-time-hint` }, "Type a time such as 72.4 (seconds), 1:12.4 (minutes and seconds) or 1m12.4s, then press Enter."),
    error,
    h("p", { class: "os-cl-hint", id: `${id}-ruler-hint` }, "The ruler is the playhead: Left and Right move it, Home and End go to the start and end."),
    h("p", { class: "os-cl-hint", id: `${id}-lanes-hint` }, "Left and Right choose the clip before or after, Up and Down the track above or below. The keys are listed under Keyboard shortcuts."),
    h("div", { class: "os-tl-frame" }, scroller, empty),
    readout,
    help,
  );
  container.append(root);

  // ---- state and drawing ----

  const optionEls = new Map<string, HTMLElement>();
  let scale = 0.05; // pixels for each millisecond
  let fitting = true; // following the whole edit, until the author zooms in
  let contentPx = 0;
  let keyDragTimer: number | undefined;

  const say = (message: string): void => announcer.say(message);

  function run(outcome: Outcome, keep = false): void {
    say(outcome.message);
    if (keep && outcome.ok) readout.textContent = outcome.message;
  }

  /** The width in pixels of the gutter that holds the track names: where time 0 is, counted from the canvas's left edge. */
  function gutterPx(): number {
    return ruler.offsetLeft;
  }

  /** Where time 0 is on the screen. */
  function originX(): number {
    return ruler.getBoundingClientRect().left;
  }

  function fit(): number {
    return fitScale(Math.max(scroller.clientWidth - gutterPx() - 16, 200), contentMs(session.lengthMs));
  }

  function noun(kind: TrackKind, count: number): string {
    const clips = kind !== "overlay" && kind !== "zoom";
    return `${count} ${clips ? (count === 1 ? "clip" : "clips") : count === 1 ? "item" : "items"}`;
  }

  function place(el: HTMLElement, row: Row): void {
    el.style.left = `${row.startMs * scale}px`;
    el.style.width = `${Math.max(4, row.lengthMs * scale)}px`;
  }

  function buildOption(row: Row, index: number, count: number): HTMLElement {
    const primary = row.clipId === session.selectedId;
    const sentence = session.describeRow(row.clipId);
    const el = h(
      "div",
      {
        role: "option",
        class: `os-tl-clip${primary ? " os-tl-primary" : ""}${row.state === "muted" ? " os-tl-muted" : ""}`,
        tabindex: primary ? 0 : -1,
        "aria-selected": session.isSelected(row.clipId) ? "true" : "false",
        "aria-label": sentence,
        "aria-posinset": index + 1,
        "aria-setsize": count,
        "data-id": row.clipId,
        title: sentence,
      },
      h("span", { class: "os-tl-clip-name" }, row.name),
    );
    if (row.state !== "ready") el.append(h("span", { class: "os-tl-badge" }, row.state));
    place(el, row);
    return el;
  }

  function renderLanes(): void {
    const hadFocus = lanesEl.contains(document.activeElement) || document.activeElement === empty;
    const rows = session.rows;
    optionEls.clear();
    const lanes = session.model.tracks.map((track) => {
      const mine = rows.filter((r) => r.trackId === track.id);
      const trackEl = h("div", { class: "os-tl-track", "data-track": track.id });
      trackEl.style.width = `${contentPx}px`;
      mine.forEach((row, i) => {
        const el = buildOption(row, i, mine.length);
        optionEls.set(row.clipId, el);
        trackEl.append(el);
      });
      return h(
        "div",
        { class: "os-tl-lane", role: "group", "aria-label": `${track.name} track, ${mine.length === 0 ? "empty" : noun(track.kind, mine.length)}`, "data-track": track.id, "data-kind": track.kind },
        h("span", { class: "os-tl-lane-name", "aria-hidden": "true" }, track.name, h("span", { class: "os-tl-lane-kind" }, trackKindName(track.kind))),
        trackEl,
      );
    });
    lanesEl.replaceChildren(...lanes);
    lanesEl.hidden = rows.length === 0;
    empty.hidden = rows.length !== 0;
    if (hadFocus) focusPrimary();
  }

  function renderTicks(): void {
    const from = Math.max(0, scroller.scrollLeft / scale - 2000);
    const to = (scroller.scrollLeft + scroller.clientWidth) / scale + 2000;
    const step = tickStepMs(scale);
    const marks = ticksBetween(from, to, step, contentPx / scale).map((ms) => {
      const mark = h("span", { class: "os-tl-tick", "aria-hidden": "true" }, formatTimecode(ms));
      mark.style.left = `${ms * scale}px`;
      return mark;
    });
    ruler.replaceChildren(...marks);
  }

  function laneIndexOf(trackId: string): number {
    return session.model.tracks.findIndex((t) => t.id === trackId);
  }

  function positionGrip(grip: HTMLElement, ms: number, lane: number): void {
    grip.style.left = `calc(var(--os-tl-gutter) + ${ms * scale}px)`;
    grip.style.top = `calc(var(--os-tl-ruler-h) + ${lane} * var(--os-tl-lane-h))`;
  }

  /** The two grips of the chosen clip: their places, and what a screen reader says for them. */
  function renderGrips(limits?: Limits): void {
    const row = session.selectedRow;
    gripsEl.hidden = row === undefined || !session.singleChoice || lanesEl.hidden;
    if (row === undefined || gripsEl.hidden) return;
    const found = limits ?? limitsOf(session.model, row.clipId);
    const lane = laneIndexOf(row.trackId);
    positionGrip(grips.start, row.startMs, lane);
    positionGrip(grips.end, row.endMs, lane);
    grips.start.setAttribute("aria-label", `Start of "${row.name}"`);
    grips.end.setAttribute("aria-label", `End of "${row.name}"`);
    if (found !== undefined) {
      grips.start.setAttribute("aria-valuemin", String(found.trimStart.min));
      grips.start.setAttribute("aria-valuemax", String(found.trimStart.max));
      grips.end.setAttribute("aria-valuemin", String(found.trimEnd.min));
      grips.end.setAttribute("aria-valuemax", String(found.trimEnd.max));
    }
    updateGripValues(row);
  }

  function updateGripValues(row: Row): void {
    grips.start.setAttribute("aria-valuenow", String(row.startMs));
    grips.start.setAttribute("aria-valuetext", `starts at ${describeTime(row.startMs)}, ${describeLength(row.lengthMs)} long`);
    grips.end.setAttribute("aria-valuenow", String(row.endMs));
    grips.end.setAttribute("aria-valuetext", `ends at ${describeTime(row.endMs)}, ${describeLength(row.lengthMs)} long`);
    const lane = laneIndexOf(row.trackId);
    positionGrip(grips.start, row.startMs, lane);
    positionGrip(grips.end, row.endMs, lane);
  }

  function syncPlayhead(): void {
    const at = session.playheadMs;
    playline.style.left = `calc(var(--os-tl-gutter) + ${at * scale}px)`;
    const shown = formatTimecode(at);
    const editing = document.activeElement === timeInput && timeInput.value !== (timeInput.dataset["shown"] ?? timeInput.value);
    timeInput.dataset["shown"] = shown;
    if (!editing) timeInput.value = shown;
    // While it plays, the slider's value is not changed sixty times a second for a screen reader to announce: it is told
    // where the playhead is when it stops.
    if (!session.playing) {
      ruler.setAttribute("aria-valuemax", String(session.lengthMs));
      ruler.setAttribute("aria-valuenow", String(at));
      ruler.setAttribute("aria-valuetext", session.playheadText);
    }
    keepPlayheadInView();
  }

  /** If the playhead has gone out of view (a key moved it, or it is playing), scroll so it is in view again. */
  function keepPlayheadInView(): void {
    const x = session.playheadMs * scale;
    const left = scroller.scrollLeft;
    const room = scroller.clientWidth - gutterPx();
    if (x < left) scroller.scrollLeft = Math.max(0, x - 40);
    else if (x > left + room) scroller.scrollLeft = session.playing ? x - room * 0.15 : x - room + 40; // turn the page while playing
  }

  function layout(): void {
    if (fitting) scale = fit();
    contentPx = contentMs(session.lengthMs) * scale;
    canvas.style.width = `calc(var(--os-tl-gutter) + ${contentPx}px)`;
    ruler.style.width = `${contentPx}px`;
    renderLanes();
    renderTicks();
    renderGrips();
    syncPlayhead();
  }

  function renderSelection(): void {
    for (const [rowId, el] of optionEls) {
      const primary = rowId === session.selectedId;
      el.tabIndex = primary ? 0 : -1;
      el.setAttribute("aria-selected", session.isSelected(rowId) ? "true" : "false");
      el.classList.toggle("os-tl-primary", primary);
    }
    renderGrips();
    syncPlayhead();
  }

  function focusPrimary(): void {
    const el = session.selectedId === undefined ? undefined : optionEls.get(session.selectedId);
    if (el !== undefined) el.focus();
    else empty.focus();
  }

  const stop = session.subscribe((kind) => {
    if (kind === "model") layout();
    else if (kind === "selection") renderSelection();
    else if (kind === "playhead" || kind === "transport") syncPlayhead();
    else if (kind === "drag") {
      const drag = session.drag;
      const row = drag === undefined ? undefined : session.rows.find((r) => r.clipId === drag.id);
      if (drag === undefined || row === undefined) return;
      const el = optionEls.get(drag.id);
      if (el !== undefined) place(el, row); // its sentence is made again when the drag ends
      updateGripValues(row);
    }
  });

  // ---- zoom ----

  /** Change the scale, keeping the playhead where it is on the screen. */
  function setScale(next: number): void {
    const at = session.playheadMs;
    const x = at * scale - scroller.scrollLeft;
    scale = next;
    fitting = Math.abs(next - fit()) < 1e-12;
    layout();
    scroller.scrollLeft = Math.max(0, at * scale - x);
    renderTicks();
  }

  function span(): string {
    return describeLength(visibleSpanMs(scroller.clientWidth - gutterPx(), scale));
  }

  function zoomBy(direction: "in" | "out"): void {
    const before = scale;
    const next = zoomed(scale, fit(), direction);
    if (next === before) {
      say(direction === "in" ? `Already as far in as it goes. About ${span()} fit across the timeline.` : `The whole edit is already showing, ${span()} across.`);
      return;
    }
    setScale(next);
    say(`Zoomed ${direction}. About ${span()} fit across the timeline.`);
  }

  function zoomFit(): void {
    fitting = true;
    scale = fit();
    layout();
    scroller.scrollLeft = 0;
    renderTicks();
    say(`Showing the whole edit, ${span()} across.`);
  }

  // ---- keys ----

  function commandKey(e: KeyboardEvent): boolean {
    const command = commandFor(e);
    if (command === undefined) return false;
    e.preventDefault();
    run(command.run(session), command.readout === true);
    return true;
  }

  const EDGE: Readonly<Record<Direction, string>> = {
    left: "This is the first clip on this track.",
    right: "This is the last clip on this track.",
    first: "This is the first clip on this track.",
    last: "This is the last clip on this track.",
    up: "There is no track above this one with anything on it.",
    down: "There is no track below this one with anything on it.",
  };

  function go(direction: Direction, extend: boolean): void {
    const from = session.selectedId;
    if (from === undefined) return;
    const target = navigate(session.rows, session.model.tracks.map((t) => t.id), from, direction);
    if (target === undefined) {
      say(EDGE[direction]);
      return;
    }
    const moved = extend && (direction === "left" || direction === "right") ? session.selectRange(target.clipId) : session.select(target.clipId);
    focusPrimary();
    if (extend && moved.changed) say(moved.message);
  }

  /** Focus arriving by Tab or by a program on a clip that is not the one with the focus makes it the only chosen one. */
  lanesEl.addEventListener("focusin", (e) => {
    const rowId = (e.target as HTMLElement).closest<HTMLElement>(".os-tl-clip")?.dataset["id"];
    if (rowId !== undefined && rowId !== session.selectedId) session.select(rowId);
  });

  lanesEl.addEventListener("keydown", (e) => {
    if (e.defaultPrevented) return;
    const key = e.key;
    const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && Object.hasOwn(NAVIGATION, key)) {
      // Ctrl with an arrow key moves without changing the choice; Ctrl+Space then adds the clip or takes it out.
      e.preventDefault();
      const from = session.selectedId;
      const target = from === undefined ? undefined : navigate(session.rows, session.model.tracks.map((t) => t.id), from, NAVIGATION[key] ?? "right");
      if (target === undefined) {
        say(EDGE[NAVIGATION[key] ?? "right"]);
        return;
      }
      session.focusOn(target.clipId);
      focusPrimary();
      return;
    }
    if (plain) {
      if (Object.hasOwn(NAVIGATION, key)) {
        e.preventDefault();
        go(NAVIGATION[key] ?? "right", e.shiftKey);
        return;
      }
      const step = PLAYHEAD_FROM_CLIPS[key];
      if (step !== undefined) {
        e.preventDefault();
        run(session.stepPlayhead(step[0], step[1] || e.shiftKey));
        return;
      }
      if (key === "+" || key === "=") {
        e.preventDefault();
        zoomBy("in");
        return;
      }
      if (key === "-" || key === "_") {
        e.preventDefault();
        zoomBy("out");
        return;
      }
      if (key === "0") {
        e.preventDefault();
        zoomFit();
        return;
      }
      if (key === "Enter" || key === "F2") {
        e.preventDefault();
        options.onEdit?.();
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

  // The playhead, with the keys of the Timeline. Its value is read as it changes, so the line of text is not used for it.
  ruler.addEventListener("keydown", (e) => {
    if (e.defaultPrevented) return;
    if (!e.ctrlKey && !e.metaKey && !e.altKey && Object.hasOwn(PLAYHEAD_KEYS, e.key)) {
      e.preventDefault();
      session.stepPlayhead(PLAYHEAD_KEYS[e.key] ?? "Right", e.shiftKey);
      return;
    }
    commandKey(e);
  });

  // The grips. A key held down is one drag, which ends by itself when the keys stop, or on any other command.
  function finishKeyDrag(): void {
    if (keyDragTimer !== undefined) {
      window.clearTimeout(keyDragTimer);
      keyDragTimer = undefined;
    }
    if (session.dragging && session.drag?.via === "keys") session.endDrag();
  }

  for (const kind of ["start", "end"] as const) {
    const grip = grips[kind];
    grip.addEventListener("keydown", (e) => {
      if (e.defaultPrevented) return;
      // Pressing Shift on its way to Shift+Right is not another command, and does not end the step.
      if (e.key === "Shift" || e.key === "Control" || e.key === "Alt" || e.key === "Meta") return;
      if (e.key === "Escape" && session.dragging && session.drag?.via === "keys") {
        e.preventDefault();
        if (keyDragTimer !== undefined) window.clearTimeout(keyDragTimer);
        keyDragTimer = undefined;
        run(session.cancelDrag());
        return;
      }
      if (!e.ctrlKey && !e.metaKey && !e.altKey && Object.hasOwn(PLAYHEAD_KEYS, e.key)) {
        e.preventDefault();
        const rowId = session.selectedId;
        if (rowId === undefined) return;
        if (!session.dragging) {
          const began = session.beginDrag(kind, rowId, "keys");
          if (!began.ok) {
            say(began.message);
            return;
          }
        }
        const stepped = session.dragStep(PLAYHEAD_KEYS[e.key] ?? "Right", e.shiftKey);
        // The grip's value is read as it changes; a sentence is only needed when it cannot go further.
        if (stepped.message !== "") say(stepped.message);
        if (keyDragTimer !== undefined) window.clearTimeout(keyDragTimer);
        keyDragTimer = window.setTimeout(finishKeyDrag, KEY_DRAG_IDLE_MS);
        return;
      }
      finishKeyDrag();
      commandKey(e);
    });
    grip.addEventListener("blur", finishKeyDrag);
  }

  // Undo and redo work from anywhere in the panel except a text box, where they undo the typing.
  root.addEventListener("keydown", (e) => {
    if (e.defaultPrevented || isTextField(e.target)) return;
    const command = commandFor(e);
    if (command !== undefined && (command.id === "undo" || command.id === "redo")) {
      e.preventDefault();
      finishKeyDrag();
      run(command.run(session));
    }
  });

  // ---- the mouse ----

  function autoscroll(clientX: number): void {
    const box = scroller.getBoundingClientRect();
    if (clientX > box.right - 40) scroller.scrollLeft += 24;
    else if (clientX < box.left + gutterPx() + 40) scroller.scrollLeft -= 24;
  }

  /** Follow the pointer until it is let go, calling `move` for each movement. `done` says how it ended. */
  function track(move: (ev: PointerEvent) => void, done: (cancelled: boolean) => void): void {
    const finish = (cancelled: boolean): void => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", onKey, true);
      done(cancelled);
    };
    const up = (): void => finish(false);
    const cancel = (): void => finish(true);
    // Escape calls off a drag from the keyboard, so a drag is never something that has to be finished with the mouse.
    const onKey = (ev: KeyboardEvent): void => {
      if (ev.key !== "Escape") return;
      ev.preventDefault();
      ev.stopPropagation();
      finish(true);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", onKey, true);
  }

  /** Drag a clip (`move`) or one of its grips (`start`, `end`). Nothing begins until the pointer has really moved. */
  function armDrag(kind: DragKind, rowId: string, e: PointerEvent): void {
    const startX = e.clientX;
    const startY = e.clientY;
    const first = session.rows.find((r) => r.clipId === rowId);
    if (first === undefined) return;
    const grab = msAtX(e.clientX, originX(), scale) - first.startMs;
    const from = first.trackId;
    let began = false;
    let drop: string | undefined;
    let lane: HTMLElement | undefined;
    const el = optionEls.get(rowId);

    const showDrop = (target: HTMLElement | undefined): void => {
      if (lane !== undefined && lane !== target) lane.classList.remove("os-tl-drop-ok", "os-tl-drop-no");
      lane = target;
      if (target === undefined) return;
      const ok = session.trackChoices.some((c) => c.id === target.dataset["track"] && !c.current);
      target.classList.toggle("os-tl-drop-ok", ok);
      target.classList.toggle("os-tl-drop-no", !ok);
    };

    track(
      (ev) => {
        if (!began) {
          if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < (kind === "move" ? 4 : 2)) return;
          if (session.selectionCount > 1) session.select(rowId);
          const result = session.beginDrag(kind, rowId);
          if (!result.ok) {
            say(result.message);
            return;
          }
          began = true;
          el?.classList.add("os-tl-dragging");
        }
        const ms = msAtX(ev.clientX, originX(), scale);
        session.dragTo(tidy(kind === "move" ? ms - grab : ms));
        autoscroll(ev.clientX);
        if (kind === "move") {
          const under = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>(".os-tl-lane") ?? undefined;
          const trackId = under?.dataset["track"];
          drop = trackId !== undefined && trackId !== from ? trackId : undefined;
          showDrop(drop === undefined ? undefined : under);
        }
      },
      (cancelled) => {
        el?.classList.remove("os-tl-dragging");
        showDrop(undefined);
        if (!began) {
          // Only a click: a click on one of several chosen clips makes it the only one.
          if (kind === "move" && !cancelled && session.selectionCount > 1) session.select(rowId);
          return;
        }
        run(cancelled ? session.cancelDrag() : session.endDrag(drop === undefined ? {} : { dropTrackId: drop }));
        focusPrimary();
      },
    );
  }

  lanesEl.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    const rowId = target.closest<HTMLElement>(".os-tl-clip")?.dataset["id"];
    if (rowId === undefined) {
      // An empty part of a track puts the playhead there.
      if (target.classList.contains("os-tl-track")) {
        e.preventDefault();
        scrub(e);
      }
      return;
    }
    e.preventDefault(); // no text selection; the focus is put where it belongs below
    if (e.ctrlKey || e.metaKey) {
      say(session.toggleSelected(rowId).message);
      focusPrimary();
      return;
    }
    if (e.shiftKey) {
      say(session.selectRange(rowId).message);
      focusPrimary();
      return;
    }
    if (!session.isSelected(rowId)) session.select(rowId);
    focusPrimary();
    armDrag("move", rowId, e);
  });

  for (const kind of ["start", "end"] as const) {
    grips[kind].addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      const rowId = session.selectedId;
      if (rowId === undefined) return;
      e.preventDefault();
      grips[kind].focus();
      armDrag(kind, rowId, e);
    });
  }

  /** Put the playhead where the pointer is, and keep following it until it is let go. */
  function scrub(e: PointerEvent): void {
    session.setPlayhead(msAtX(e.clientX, originX(), scale));
    track(
      (ev) => {
        session.setPlayhead(msAtX(ev.clientX, originX(), scale));
        autoscroll(ev.clientX);
      },
      () => undefined,
    );
  }

  ruler.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    ruler.focus();
    scrub(e);
  });

  scroller.addEventListener("scroll", () => {
    window.requestAnimationFrame(renderTicks);
  });

  // ---- the playhead time and the track kind ----

  let tried: string | undefined;
  const commitTime = (): void => {
    const text = timeInput.value;
    if (text === formatTimecode(session.playheadMs) || text === tried) return;
    const outcome = session.setPlayheadText(text);
    if (outcome.ok) {
      tried = undefined;
      timeInput.removeAttribute("aria-invalid");
      timeInput.removeAttribute("aria-errormessage");
      error.textContent = "";
      timeInput.value = formatTimecode(session.playheadMs);
    } else {
      tried = text;
      timeInput.setAttribute("aria-invalid", "true");
      timeInput.setAttribute("aria-errormessage", error.id);
      error.textContent = outcome.message;
    }
    say(outcome.message);
  };
  timeInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commitTime();
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (timeInput.value !== formatTimecode(session.playheadMs)) {
        timeInput.value = formatTimecode(session.playheadMs);
        tried = undefined;
        timeInput.removeAttribute("aria-invalid");
        error.textContent = "";
        say("Put back as it was.");
      } else {
        focusPrimary();
      }
    }
  });
  timeInput.addEventListener("change", commitTime);
  timeInput.addEventListener("input", () => {
    tried = undefined;
  });
  kindSelect.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      focusPrimary();
    }
  });

  // ---- size ----

  const observer = new ResizeObserver(() => {
    if (fitting) layout();
    else renderTicks();
  });
  observer.observe(scroller);

  // ---- first draw ----
  layout();

  return {
    element: root,
    focus: focusPrimary,
    announce: say,
    destroy(): void {
      stop();
      observer.disconnect();
      if (keyDragTimer !== undefined) window.clearTimeout(keyDragTimer);
      if (ownAnnouncer) announcer.destroy();
      root.remove();
    },
  };
}
