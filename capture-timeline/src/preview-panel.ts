// SPDX-License-Identifier: Apache-2.0
/**
 * The preview (story C4's picture, and playing the edit): the picture of the edit at the playhead, with the pictures,
 * overlays and zoom the edit asks for, and the sound, and a way to play it.
 *
 * What plays follows the exporter (see `momentAt` in the edit model): a screen, webcam, video or image clip gives
 * picture only, a microphone, computer-sound or sound-file clip gives sound, the zoom moves along the same curve, and
 * blur, blackout, shapes and arrows zoom with the picture while text and the webcam do not. This is a way to check the
 * edit, not the finished video: the exported MP4 is what counts, and text is drawn with the browser's fonts here.
 *
 * The playhead is the one clock. While it plays, the playhead moves along with the wall clock and each recording is kept
 * near where it should be (it is moved only when it has drifted by more than 0.3 second, so it does not stutter).
 * Recordings that are not available (the demo's made-up lesson has none) are shown as labelled placeholders, so the
 * overlays and zoom can still be seen, and playing still moves the playhead.
 *
 * For a screen reader the picture has no content of its own: what is showing is said on request (the Describe button, or
 * D), which is the same text the timeline's Describe gives. While playing, nothing is announced except starting, pausing
 * and reaching the end.
 */

import {
  FULL,
  describeTime,
  formatTimecode,
  momentAt,
  type Item,
  type Moment,
  type PlayingClip,
  type Rect,
  type SourceRef,
} from "../../capture-edit/src/index.ts";
import { Announcer } from "./announcer.ts";
import { h, nextId } from "./dom.ts";
import type { MediaLibrary } from "./project.ts";
import type { EditSession, Outcome } from "./session.ts";

export interface PreviewOptions {
  /** Where messages are said. Without it the panel makes its own line at the top. */
  announcer?: Announcer;
  title?: string;
}

export interface PreviewPanel {
  readonly element: HTMLElement;
  readonly playing: boolean;
  /** Play from the playhead, or pause. */
  toggle(): Outcome;
  /** Put keyboard focus on the Play button. */
  focus(): void;
  /** Use another set of recordings (after opening a recording). */
  setLibrary(library: MediaLibrary): void;
  destroy(): void;
}

/** How far a recording may drift from the playhead before it is moved, in seconds. */
const DRIFT_S = 0.3;

interface Slot {
  readonly key: string;
  readonly ref: SourceRef;
  readonly kind: "video" | "audio";
  el: HTMLVideoElement | HTMLAudioElement | undefined;
  /** `loading` until the library has answered; `missing` if it has nothing for this recording. */
  state: "loading" | "ready" | "missing";
  gain: GainNode | undefined;
}

function keyOf(ref: SourceRef): string {
  return ref.kind === "take" ? `take-${ref.take}-${ref.media}` : `asset-${ref.asset}`;
}

function percent(bp: number): string {
  return `${bp / 100}%`;
}

function place(el: HTMLElement, rect: Rect): void {
  el.style.left = percent(rect.x);
  el.style.top = percent(rect.y);
  el.style.width = percent(rect.width);
  el.style.height = percent(rect.height);
}

export function mountPreview(container: HTMLElement, session: EditSession, initial: MediaLibrary, options: PreviewOptions = {}): PreviewPanel {
  const id = nextId("os-pv");
  const ownAnnouncer = options.announcer === undefined;
  const announcer = options.announcer ?? new Announcer();
  let library = initial;

  // ---- the parts ----

  const pictureLayer = h("div", { class: "os-pv-layer os-pv-zoomed" });
  const fixedLayer = h("div", { class: "os-pv-layer" });
  const stage = h("div", { class: "os-pv-stage", role: "img" }, pictureLayer, fixedLayer);
  const timeText = h("span", { class: "os-pv-time" });
  const playButton = h("button", { type: "button", class: "os-cl-button", "aria-keyshortcuts": "Space" }, "Play", h("kbd", { class: "os-cl-key", "aria-hidden": "true" }, "Space"));
  const describeButton = h("button", { type: "button", class: "os-cl-button", "aria-keyshortcuts": "D" }, "Describe what is showing", h("kbd", { class: "os-cl-key", "aria-hidden": "true" }, "D"));
  const reduce = h("input", { id: `${id}-reduce`, type: "checkbox" });
  reduce.checked = typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const reduceLabel = h("label", { class: "os-cl-check", for: `${id}-reduce` }, reduce, " Cut zooms instead of moving (reduced motion)");
  const readout = h("p", { class: "os-cl-readout" });
  // The sound has no picture. Its players are kept here, out of sight and out of the way of a screen reader.
  const soundHost = h("div", { hidden: true, "aria-hidden": "true" });
  const root = h(
    "section",
    { class: "os-cl os-pv", "aria-labelledby": `${id}-title` },
    h("h2", { id: `${id}-title`, class: "os-cl-title" }, options.title ?? "Preview"),
    ownAnnouncer ? announcer.element : "",
    h("div", { class: "os-pv-controls", role: "group", "aria-label": "Preview controls" }, playButton, describeButton, timeText, reduceLabel),
    h("figure", { class: "os-pv-figure" }, stage),
    readout,
    soundHost,
  );
  container.append(root);

  // ---- the recordings ----

  const slots = new Map<string, Slot>();
  let audio: AudioContext | undefined;

  function slotFor(ref: SourceRef, kind: "video" | "audio"): Slot {
    const key = keyOf(ref);
    let slot = slots.get(key);
    if (slot !== undefined) return slot;
    slot = { key, ref, kind, el: undefined, state: "loading", gain: undefined };
    slots.set(key, slot);
    const made = slot;
    void library.url(ref).then((url) => {
      if (slots.get(key) !== made) return; // the library was changed meanwhile
      if (url === undefined) {
        made.state = "missing";
      } else {
        const el = kind === "video" ? h("video", { class: "os-pv-media", playsinline: true, preload: "auto" }) : h("audio", { preload: "auto" });
        if (el instanceof HTMLVideoElement) el.muted = true; // the exporter does not use a screen recording's own sound
        el.src = url;
        if (el instanceof HTMLAudioElement) soundHost.append(el);
        made.el = el;
        made.state = "ready";
        connectAudio(made);
      }
      if (!playing) show(session.playheadMs);
    });
    return slot;
  }

  /** Sound goes through a gain, so a clip can be louder than recorded (the volume goes up to 20 decibels). */
  function connectAudio(slot: Slot): void {
    if (audio === undefined || slot.kind !== "audio" || slot.el === undefined || slot.gain !== undefined) return;
    const gain = audio.createGain();
    audio.createMediaElementSource(slot.el as HTMLAudioElement).connect(gain).connect(audio.destination);
    slot.gain = gain;
  }

  function startAudio(): void {
    if (audio === undefined) {
      const Context = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (Context !== undefined) audio = new Context();
    }
    void audio?.resume();
    for (const slot of slots.values()) connectAudio(slot);
  }

  function discard(): void {
    for (const slot of slots.values()) {
      slot.el?.pause();
      slot.el?.removeAttribute("src");
      slot.el?.load();
    }
    slots.clear();
  }

  // ---- drawing a moment ----

  const boxes = new Map<string, HTMLElement>(); // a picture box for each picture clip showing
  let overlaySignature = "";
  const overlayEls: HTMLElement[] = [];
  let shownText = "";

  function placeholder(clip: PlayingClip, slot: Slot): HTMLElement {
    const kind = clip.trackKind;
    return h(
      "div",
      { class: `os-pv-placeholder os-pv-${kind}` },
      h("strong", {}, clip.name),
      h("span", {}, slot.state === "loading" ? "Loading the recording" : "No recording is loaded for this, so this stands in for it"),
    );
  }

  function pictureBox(clip: PlayingClip, onlyPicture: boolean): HTMLElement {
    let box = boxes.get(clip.clipId);
    if (box === undefined) {
      box = h("div", { class: `os-pv-box os-pv-box-${clip.trackKind}`, "data-clip": clip.clipId });
      boxes.set(clip.clipId, box);
    }
    if (clip.trackKind === "webcam" && clip.placement === undefined && !onlyPicture) {
      box.style.left = "";
      box.style.top = "";
      box.style.width = "";
      box.style.height = "";
      box.classList.add("os-pv-corner");
    } else {
      box.classList.remove("os-pv-corner");
      place(box, clip.placement ?? { x: 0, y: 0, width: FULL, height: FULL });
    }
    const slot = slotFor(clip.source, "video");
    const wanted = slot.el ?? placeholder(clip, slot);
    if (box.firstChild !== wanted) box.replaceChildren(wanted);
    return box;
  }

  function overlayElement(item: Item): HTMLElement | undefined {
    switch (item.kind) {
      case "zoom":
        return undefined;
      case "text": {
        const el = h("div", { class: "os-pv-text", "data-item": item.id }, item.text);
        place(el, item.rect);
        el.style.fontSize = `${item.fontSizeBp / 100}cqh`;
        el.style.color = item.color;
        el.style.background = item.background ?? "transparent";
        el.style.fontWeight = item.bold ? "700" : "400";
        el.style.textAlign = item.align;
        return el;
      }
      case "shape": {
        const el = h("div", { class: "os-pv-shape", "data-item": item.id });
        place(el, item.rect);
        el.style.border = `${item.lineWidthBp / 100}cqh solid ${item.outline}`;
        el.style.background = item.fill ?? "transparent";
        if (item.shape === "ellipse") el.style.borderRadius = "50%";
        return el;
      }
      case "blur": {
        const el = h("div", { class: "os-pv-blur", "data-item": item.id });
        place(el, item.rect);
        return el;
      }
      case "redact": {
        const el = h("div", { class: "os-pv-redact", "data-item": item.id });
        place(el, item.rect);
        el.style.background = item.color;
        return el;
      }
      case "arrow": {
        const ns = "http://www.w3.org/2000/svg";
        const svg = document.createElementNS(ns, "svg");
        svg.setAttribute("class", "os-pv-arrow");
        svg.setAttribute("viewBox", `0 0 ${FULL} ${FULL}`);
        svg.setAttribute("preserveAspectRatio", "none");
        const { from, to } = item;
        const angle = Math.atan2(to.y - from.y, to.x - from.x);
        const head = Math.max(item.lineWidthBp * 4, 250);
        const left = [to.x - head * Math.cos(angle - 0.45), to.y - head * Math.sin(angle - 0.45)];
        const right = [to.x - head * Math.cos(angle + 0.45), to.y - head * Math.sin(angle + 0.45)];
        const line = document.createElementNS(ns, "line");
        for (const [name, value] of [["x1", from.x], ["y1", from.y], ["x2", to.x], ["y2", to.y], ["stroke", item.color], ["stroke-width", item.lineWidthBp * 1.2]] as const) line.setAttribute(name, String(value));
        const tip = document.createElementNS(ns, "polygon");
        tip.setAttribute("points", `${to.x},${to.y} ${left.join(",")} ${right.join(",")}`);
        tip.setAttribute("fill", item.color);
        svg.append(line, tip);
        return svg as unknown as HTMLElement;
      }
    }
  }

  function showOverlays(moment: Moment): void {
    const signature = JSON.stringify(moment.overlays);
    if (signature === overlaySignature) return;
    overlaySignature = signature;
    for (const el of overlayEls) el.remove();
    overlayEls.length = 0;
    for (const item of moment.overlays) {
      const el = overlayElement(item);
      if (el === undefined) continue;
      overlayEls.push(el);
      // Text stays where it was written; the rest belongs to the picture and moves and grows with the zoom.
      (item.kind === "text" ? fixedLayer : pictureLayer).append(el);
    }
  }

  function showZoom(moment: Moment): void {
    const r = moment.zoom.rect;
    if (r.width >= FULL && r.height >= FULL) {
      pictureLayer.style.transform = "";
      return;
    }
    const s = Math.min(FULL / r.width, FULL / r.height);
    const tx = 50 - (s * (r.x + r.width / 2) * 100) / FULL;
    const ty = 50 - (s * (r.y + r.height / 2) * 100) / FULL;
    pictureLayer.style.transform = `translate(${tx}%, ${ty}%) scale(${s})`;
  }

  /** Keep a recording near where it should be, and playing or paused as asked. */
  function keep(slot: Slot, sourceMs: number, playingNow: boolean): void {
    const el = slot.el;
    if (el === undefined) return;
    const want = sourceMs / 1000;
    if (!playingNow || Math.abs(el.currentTime - want) > DRIFT_S) {
      try {
        el.currentTime = want;
      } catch {
        /* not ready to be moved yet */
      }
    }
    if (playingNow) {
      if (el.paused) void el.play().catch(() => undefined);
    } else if (!el.paused) {
      el.pause();
    }
  }

  function arrange(layer: HTMLElement, wanted: readonly HTMLElement[]): void {
    if (!wanted.every((el, i) => layer.children[i] === el)) layer.prepend(...wanted);
  }

  /** Show the edit at a time: the pictures, the overlays and the zoom, and keep the recordings where they belong. */
  function show(timeMs: number): void {
    const moment = momentAt(session.model, timeMs, { reduceMotion: reduce.checked });
    const others = moment.pictures.filter((p) => p.trackKind !== "webcam").length;
    const wanted = new Set<string>();
    const main: HTMLElement[] = [];
    const corner: HTMLElement[] = [];
    for (const clip of moment.pictures) {
      const box = pictureBox(clip, others === 0);
      wanted.add(clip.clipId);
      (clip.trackKind === "webcam" ? corner : main).push(box);
    }
    for (const [clipId, box] of boxes) {
      if (!wanted.has(clipId)) {
        box.remove();
        boxes.delete(clipId);
      }
    }
    // The main pictures and what belongs to them zoom; the webcam does not; text is last, on top. A picture is only moved
    // when the order really changes: taking a video out of the page and putting it back pauses it.
    arrange(pictureLayer, main);
    arrange(fixedLayer, corner);
    showOverlays(moment);
    showZoom(moment);
    const total = session.lengthMs;
    timeText.textContent = `${formatTimecode(moment.timeMs)} of ${formatTimecode(total)}`;
    const label = `Preview of the edit at ${describeTime(moment.timeMs)}. Use the Describe button for what is showing.`;
    if (!playing && label !== shownText) {
      shownText = label;
      stage.setAttribute("aria-label", label);
    }

    // The recordings.
    const active = new Set<string>();
    for (const clip of moment.pictures) {
      const slot = slotFor(clip.source, "video");
      active.add(slot.key);
      keep(slot, clip.sourceMs, playing);
    }
    for (const clip of moment.sounds) {
      const slot = slotFor(clip.source, "audio");
      active.add(slot.key);
      keep(slot, clip.sourceMs, playing);
      if (slot.gain !== undefined) slot.gain.gain.value = clip.gain;
      else if (slot.el !== undefined) slot.el.volume = Math.min(1, clip.gain);
      if (slot.el !== undefined) slot.el.dataset["gain"] = clip.gain.toFixed(3); // visible to a test, and to anyone looking
    }
    for (const slot of slots.values()) if (!active.has(slot.key) && slot.el !== undefined && !slot.el.paused) slot.el.pause();
  }

  // ---- playing ----

  let playing = false;
  let startedAtMs = 0;
  let startedAtWall = 0;
  let ticking = false;
  let frame: number | undefined;

  function setButton(): void {
    const label = playButton.firstChild;
    if (label !== null) label.textContent = playing ? "Pause" : "Play";
  }

  function tick(now: number): void {
    if (!playing) return;
    const end = session.lengthMs;
    const t = startedAtMs + (now - startedAtWall);
    ticking = true;
    session.setPlayhead(Math.min(t, end));
    ticking = false;
    if (t >= end) {
      stopPlaying();
      show(end);
      announcer.say(`Reached the end, ${describeTime(end)}.`);
      return;
    }
    show(t);
    frame = window.requestAnimationFrame(tick);
  }

  function stopPlaying(): void {
    playing = false;
    if (frame !== undefined) window.cancelAnimationFrame(frame);
    frame = undefined;
    for (const slot of slots.values()) slot.el?.pause();
    setButton();
    session.setPlaying(false);
  }

  function toggle(): Outcome {
    if (playing) {
      stopPlaying();
      show(session.playheadMs);
      return { ok: true, message: `Paused at ${describeTime(session.playheadMs)}.` };
    }
    if (session.lengthMs === 0) return { ok: false, message: "There is nothing to play: the edit is empty." };
    const restarted = session.playheadMs >= session.lengthMs;
    if (restarted) session.setPlayhead(0);
    startAudio();
    playing = true;
    startedAtMs = session.playheadMs;
    startedAtWall = performance.now();
    setButton();
    session.setPlaying(true);
    frame = window.requestAnimationFrame(tick);
    return { ok: true, message: `Playing from ${describeTime(startedAtMs)}${restarted ? ", the start, because the playhead was at the end" : ""}.` };
  }

  playButton.addEventListener("click", () => announcer.say(toggle().message));
  describeButton.addEventListener("click", () => {
    const outcome = session.describePlayhead();
    readout.textContent = outcome.message;
    announcer.say(outcome.message);
  });
  reduce.addEventListener("change", () => {
    if (!playing) show(session.playheadMs);
  });
  session.setTransport({ toggle });

  const stop = session.subscribe((kind) => {
    if (kind === "playhead") {
      if (ticking) return;
      if (playing) {
        // The author moved the playhead while it was playing: carry on from there.
        startedAtMs = session.playheadMs;
        startedAtWall = performance.now();
      } else show(session.playheadMs);
    } else if (kind === "model" && !playing) {
      show(session.playheadMs);
    }
  });

  show(session.playheadMs);
  setButton();

  return {
    element: root,
    get playing() {
      return playing;
    },
    toggle,
    focus: () => playButton.focus(),
    setLibrary(next) {
      if (playing) stopPlaying();
      discard();
      for (const box of boxes.values()) box.remove();
      boxes.clear();
      library = next;
      show(session.playheadMs);
    },
    destroy(): void {
      stop();
      if (playing) stopPlaying();
      discard();
      session.setTransport(undefined);
      void audio?.close();
      if (ownAnnouncer) announcer.destroy();
      root.remove();
    },
  };
}
