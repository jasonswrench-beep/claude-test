// SPDX-License-Identifier: Apache-2.0
/**
 * The editing screens together: the timeline above and the clip list below, on one session, sharing one line of
 * announcements. F6 and Shift+F6 move between the two (region cycling, as OpenStage's own panels do).
 */

import { Announcer } from "./announcer.ts";
import { h } from "./dom.ts";
import { mountClipList, type ClipListPanel } from "./panel.ts";
import { EMPTY_LIBRARY, type MediaLibrary } from "./project.ts";
import { mountPreview, type PreviewPanel } from "./preview-panel.ts";
import type { EditSession } from "./session.ts";
import { mountTimeline, type TimelinePanel } from "./timeline-panel.ts";

export interface EditorOptions {
  /** Also tell this function every message. */
  announce?: (message: string) => void;
  /** How the rows of the clip list are named for a screen reader. */
  rows?: "sentence" | "cells";
  /** The recordings the preview plays. Without it the preview shows placeholders. */
  library?: MediaLibrary;
}

export interface Editor {
  readonly element: HTMLElement;
  readonly preview: PreviewPanel;
  readonly timeline: TimelinePanel;
  readonly list: ClipListPanel;
  readonly announcer: Announcer;
  /** Put keyboard focus on the timeline. */
  focus(): void;
  announce(message: string): void;
  destroy(): void;
}

export function mountEditor(container: HTMLElement, session: EditSession, options: EditorOptions = {}): Editor {
  const announcer = new Announcer(options.announce);
  const previewHost = h("div", { class: "os-ed-preview" });
  const timelineHost = h("div", { class: "os-ed-timeline" });
  const listHost = h("div", { class: "os-ed-list" });
  const root = h("div", { class: "os-ed" }, announcer.element, previewHost, timelineHost, listHost);
  container.append(root);

  const list = mountClipList(listHost, session, { announcer, playhead: false, ...(options.rows === undefined ? {} : { rows: options.rows }), title: "Clip list" });
  const timeline = mountTimeline(timelineHost, session, { announcer, onEdit: () => list.focusDetails() });
  const preview = mountPreview(previewHost, session, options.library ?? EMPTY_LIBRARY, { announcer });

  const regions = [
    { name: "Preview", element: preview.element, focus: () => preview.focus() },
    { name: "Timeline", element: timeline.element, focus: () => timeline.focus() },
    { name: "Clip list", element: list.element, focus: () => list.focus() },
  ];

  // F6 and Shift+F6 move between the regions, and say which one they arrived in.
  root.addEventListener("keydown", (e) => {
    if (e.key !== "F6" || e.ctrlKey || e.metaKey || e.altKey) return;
    e.preventDefault();
    const here = regions.findIndex((r) => r.element.contains(document.activeElement));
    const next = regions[(here + (e.shiftKey ? -1 : 1) + regions.length) % regions.length];
    if (next === undefined) return;
    next.focus();
    announcer.say(`${next.name}.`);
  });

  return {
    element: root,
    preview,
    timeline,
    list,
    announcer,
    focus: () => timeline.focus(),
    announce: (message) => announcer.say(message),
    destroy(): void {
      preview.destroy();
      timeline.destroy();
      list.destroy();
      announcer.destroy();
      root.remove();
    },
  };
}
