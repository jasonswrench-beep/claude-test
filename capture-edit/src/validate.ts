// SPDX-License-Identifier: Apache-2.0
/**
 * Checking an edit for mistakes, in words.
 *
 * Two kinds of finding. An `error` means the edit is not sound (a clip overlaps another, a clip uses
 * more of a recording than exists) and must not be rendered. `needs-attention` means the edit is
 * sound but something OpenStage requires is still missing (an image with no alt text, an imported
 * video with no captions); the author is asked for it, and export refuses to finish until it is given
 * (stories C10 and D6).
 *
 * The same checks, with the same codes, are made by the renderer's Rust reader. The files in
 * `fixtures/` are the shared examples both must agree on.
 */

import {
  ALLOWED_ITEMS,
  ALLOWED_SOURCES,
  clipEndMs,
  clipLengthMs,
  findSource,
  MAX_IMPORTED_VIDEO_MS,
  MAX_VOLUME_DB,
  MIN_CLIP_MS,
  MIN_VOLUME_DB,
  itemEndMs,
  trackKindName,
  type Clip,
  type Item,
  type Model,
  type Track,
} from "./model.ts";
import { contrastRatio, describeRatio, isColor, neededRatio, VERY_SMALL_TEXT_BP } from "./contrast.ts";
import { isPoint, isRect } from "./regions.ts";
import { describeTime } from "./time.ts";

/**
 * `error`: the edit is not sound and must not be rendered. `needs-attention`: sound, but something
 * OpenStage requires is missing. `warning`: sound and complete, but likely to be hard to see or read.
 */
export type Severity = "error" | "needs-attention" | "warning";

export type ProblemCode =
  | "duplicate-id"
  | "bad-time"
  | "too-short"
  | "unknown-source"
  | "source-not-allowed"
  | "beyond-source"
  | "overlap"
  | "unsorted"
  | "fade-too-long"
  | "volume-range"
  | "imported-video-too-long"
  | "needs-alt-text"
  | "needs-text-alternative"
  | "item-wrong-track"
  | "bad-rect"
  | "placement-not-allowed"
  | "bad-color"
  | "bad-style"
  | "text-empty"
  | "transition-too-long"
  | "too-many-zooms"
  | "needs-description"
  | "low-contrast"
  | "text-too-small"
  | "zoom-aspect";

export interface Problem {
  readonly code: ProblemCode;
  readonly severity: Severity;
  readonly message: string;
  readonly trackId?: string;
  readonly clipId?: string;
  readonly itemId?: string;
}

function isWhole(n: number): boolean {
  return Number.isInteger(n) && n >= 0;
}

function where(track: Track, clip: Clip): string {
  return `The clip "${clip.name}" on the ${track.name} track`;
}

/** What an item is called in a sentence. */
export function itemKindName(item: Item): string {
  switch (item.kind) {
    case "text":
      return "text overlay";
    case "shape":
      return "shape";
    case "arrow":
      return "arrow";
    case "blur":
      return "blur";
    case "redact":
      return "redaction";
    case "zoom":
      return "zoom";
  }
}

/** The most zooms that may start within one second. Zooms that follow each other faster than this are too busy to follow. */
export const MAX_ZOOMS_PER_SECOND = 3;

/** Every problem in an edit, in the order tracks and clips appear. */
export function validate(model: Model): Problem[] {
  const problems: Problem[] = [];
  const add = (p: Problem) => problems.push(p);

  const seen = new Set<string>();
  const duplicate = (id: string, what: string, trackId?: string) => {
    if (seen.has(id)) {
      add({
        code: "duplicate-id",
        severity: "error",
        message: `Two ${what}s have the id ${id}. Every id must be different.`,
        ...(trackId === undefined ? {} : { trackId }),
      });
    }
    seen.add(id);
  };

  for (const track of model.tracks) {
    duplicate(track.id, "track");
    checkItems(model, track, problems, duplicate);
    let previous: Clip | undefined;
    for (const clip of track.clips) {
      duplicate(clip.id, "clip", track.id);
      const here = { trackId: track.id, clipId: clip.id };
      const label = where(track, clip);
      if (clip.placement !== undefined && !isRect(clip.placement)) {
        add({ code: "bad-rect", severity: "error", message: `${label} has a place on the picture that is not a rectangle inside it.`, ...here });
      }

      if (![clip.startMs, clip.sourceStartMs, clip.sourceEndMs, clip.fadeInMs, clip.fadeOutMs].every(isWhole)) {
        add({
          code: "bad-time",
          severity: "error",
          message: `${label} has a time that is not a whole number of milliseconds, or is below zero.`,
          ...here,
        });
        previous = clip;
        continue;
      }
      if (clipLengthMs(clip) < MIN_CLIP_MS) {
        add({
          code: "too-short",
          severity: "error",
          message: `${label} is ${describeTime(Math.max(0, clipLengthMs(clip)))} long. A clip must be at least ${describeTime(MIN_CLIP_MS)}.`,
          ...here,
        });
      }

      const source = findSource(model, clip.source);
      if (source === undefined) {
        add({
          code: "unknown-source",
          severity: "error",
          message: `${label} uses a recording or file that this edit does not know about.`,
          ...here,
        });
      } else {
        if (clip.placement !== undefined && ["microphone", "system-audio", "audio"].includes(source.kind)) {
          add({
            code: "placement-not-allowed",
            severity: "error",
            message: `${label} holds ${source.name}, which is sound only, so it cannot have a place on the picture. Take the place away.`,
            ...here,
          });
        }
        if (!ALLOWED_SOURCES[track.kind].includes(source.kind)) {
          add({
            code: "source-not-allowed",
            severity: "error",
            message: `${label} holds ${source.name}, which cannot go on a ${trackKindName(track.kind).toLowerCase()} track.`,
            ...here,
          });
        }
        if (source.durationMs !== null && clip.sourceEndMs > source.durationMs) {
          add({
            code: "beyond-source",
            severity: "error",
            message: `${label} uses up to ${describeTime(clip.sourceEndMs)} of ${source.name}, which is only ${describeTime(source.durationMs)} long.`,
            ...here,
          });
        }
        if (source.ref.kind === "asset" && source.kind === "video" && clipLengthMs(clip) > MAX_IMPORTED_VIDEO_MS) {
          add({
            code: "imported-video-too-long",
            severity: "error",
            message: `${label} is ${describeTime(clipLengthMs(clip))} long. Imported video clips can be at most ${describeTime(MAX_IMPORTED_VIDEO_MS)}.`,
            ...here,
          });
        }
        if (source.kind === "image" && clip.decorative !== true && (clip.altText ?? "").trim() === "") {
          add({
            code: "needs-alt-text",
            severity: "needs-attention",
            message: `${label} is an image with no alt text. Describe it, or mark it as decorative.`,
            ...here,
          });
        }
        if (
          source.ref.kind === "asset" &&
          (source.kind === "video" || source.kind === "audio") &&
          source.hasTextAlternative !== true
        ) {
          add({
            code: "needs-text-alternative",
            severity: "needs-attention",
            message:
              source.kind === "video"
                ? `${label} is an imported video with no captions or narration script. Add one.`
                : `${label} is imported sound with no captions, script or transcript. Add one, or describe it if it is wordless music.`,
            ...here,
          });
        }
      }

      if (clip.fadeInMs + clip.fadeOutMs > clipLengthMs(clip)) {
        add({
          code: "fade-too-long",
          severity: "error",
          message: `${label} fades in for ${describeTime(clip.fadeInMs)} and out for ${describeTime(clip.fadeOutMs)}, which is longer than the clip.`,
          ...here,
        });
      }
      if (!(clip.volumeDb >= MIN_VOLUME_DB && clip.volumeDb <= MAX_VOLUME_DB) || !Number.isInteger(clip.volumeDb)) {
        add({
          code: "volume-range",
          severity: "error",
          message: `${label} has its volume changed by ${clip.volumeDb} decibels. Use a whole number from ${MIN_VOLUME_DB} to ${MAX_VOLUME_DB}.`,
          ...here,
        });
      }

      if (previous !== undefined) {
        if (clip.startMs < previous.startMs) {
          add({
            code: "unsorted",
            severity: "error",
            message: `${label} comes after a clip that starts later. Clips must be in order of start.`,
            ...here,
          });
        } else if (clip.startMs < clipEndMs(previous)) {
          add({
            code: "overlap",
            severity: "error",
            message: `${label} starts at ${describeTime(clip.startMs)}, before the clip "${previous.name}" before it ends at ${describeTime(clipEndMs(previous))}.`,
            ...here,
          });
        }
      }
      previous = clip;
    }
  }
  return problems;
}

function checkItems(
  _model: Model,
  track: Track,
  problems: Problem[],
  duplicate: (id: string, what: string, trackId?: string) => void,
): void {
  const add = (p: Problem) => problems.push(p);
  let previous: Item | undefined;
  const starts: number[] = [];
  for (const item of track.items) {
    duplicate(item.id, "item", track.id);
    const here = { trackId: track.id, itemId: item.id };
    const label = `The ${itemKindName(item)} "${item.name}" on the ${track.name} track`;

    if (!ALLOWED_ITEMS[track.kind].includes(item.kind)) {
      add({
        code: "item-wrong-track",
        severity: "error",
        message: `${label} cannot go on a ${trackKindName(track.kind).toLowerCase()} track.`,
        ...here,
      });
    }
    if (!isWhole(item.startMs) || !isWhole(item.durationMs)) {
      add({
        code: "bad-time",
        severity: "error",
        message: `${label} has a time that is not a whole number of milliseconds, or is below zero.`,
        ...here,
      });
      previous = item;
      continue;
    }
    if (item.durationMs < MIN_CLIP_MS) {
      add({
        code: "too-short",
        severity: "error",
        message: `${label} is ${describeTime(item.durationMs)} long. It must be at least ${describeTime(MIN_CLIP_MS)}.`,
        ...here,
      });
    }

    const colors: string[] = [];
    switch (item.kind) {
      case "text":
        colors.push(item.color, ...(item.background === null ? [] : [item.background]));
        break;
      case "shape":
        colors.push(item.outline, ...(item.fill === null ? [] : [item.fill]));
        break;
      case "arrow":
        colors.push(item.color);
        break;
      case "redact":
        colors.push(item.color);
        break;
      default:
        break;
    }
    if (colors.some((c) => !isColor(c))) {
      add({ code: "bad-color", severity: "error", message: `${label} has a colour that is not written like #FFFFFF.`, ...here });
    }

    if (item.kind === "arrow") {
      if (!isPoint(item.from) || !isPoint(item.to)) {
        add({ code: "bad-rect", severity: "error", message: `${label} starts or ends outside the picture.`, ...here });
      }
    } else if (!isRect(item.rect)) {
      add({ code: "bad-rect", severity: "error", message: `${label} is not a rectangle inside the picture.`, ...here });
    }

    switch (item.kind) {
      case "text": {
        if (item.text.trim() === "") {
          add({ code: "text-empty", severity: "error", message: `${label} has no text.`, ...here });
        }
        if (!Number.isInteger(item.fontSizeBp) || item.fontSizeBp < 50 || item.fontSizeBp > 3000) {
          add({ code: "bad-style", severity: "error", message: `${label} has a text size outside the range that can be drawn.`, ...here });
        } else if (item.fontSizeBp < VERY_SMALL_TEXT_BP) {
          add({
            code: "text-too-small",
            severity: "warning",
            message: `${label} is very small; viewers may not be able to read it. Make the text larger.`,
            ...here,
          });
        }
        if (item.background !== null && isColor(item.color) && isColor(item.background)) {
          const ratio = contrastRatio(item.color, item.background);
          const needed = neededRatio(item.fontSizeBp, item.bold);
          if (ratio < needed) {
            add({
              code: "low-contrast",
              severity: "warning",
              message: `${label} is hard to read: its colour and its background have a contrast of ${describeRatio(ratio)}, and ${describeRatio(needed)} is needed. Choose colours that differ more.`,
              ...here,
            });
          }
        }
        break;
      }
      case "shape":
      case "arrow": {
        if (!Number.isInteger(item.lineWidthBp) || item.lineWidthBp < 1 || item.lineWidthBp > 500) {
          add({ code: "bad-style", severity: "error", message: `${label} has a line thickness outside the range that can be drawn.`, ...here });
        }
        if (item.decorative !== true && (item.altText ?? "").trim() === "") {
          add({
            code: "needs-alt-text",
            severity: "needs-attention",
            message: `${label} has no alt text. Say what it points out, or mark it as decorative.`,
            ...here,
          });
        }
        break;
      }
      case "zoom": {
        if (!Number.isInteger(item.transitionMs) || item.transitionMs < 0 || item.transitionMs * 2 > item.durationMs) {
          add({
            code: "transition-too-long",
            severity: "error",
            message: `${label} moves in and out for ${describeTime(Math.max(0, item.transitionMs))} each, which does not fit in ${describeTime(item.durationMs)}.`,
            ...here,
          });
        }
        if (item.description.trim() === "") {
          add({
            code: "needs-description",
            severity: "needs-attention",
            message: `${label} has no description. Say what it shows, so the zoom is not only a picture.`,
            ...here,
          });
        }
        if (isRect(item.rect) && Math.abs(item.rect.width - item.rect.height) > 100) {
          add({
            code: "zoom-aspect",
            severity: "warning",
            message: `${label} is not the same shape as the picture, so the zoomed view will have empty bars at its sides or top and bottom.`,
            ...here,
          });
        }
        starts.push(item.startMs);
        break;
      }
      default:
        break;
    }

    if (previous !== undefined) {
      if (item.startMs < previous.startMs) {
        add({
          code: "unsorted",
          severity: "error",
          message: `${label} comes after an item that starts later. Items must be in order of start.`,
          ...here,
        });
      } else if (track.kind === "zoom" && item.startMs < itemEndMs(previous)) {
        add({
          code: "overlap",
          severity: "error",
          message: `${label} starts at ${describeTime(item.startMs)}, before the zoom "${previous.name}" before it ends at ${describeTime(itemEndMs(previous))}.`,
          ...here,
        });
      }
    }
    previous = item;
  }
  // Too many zooms in a short time is hard to follow.
  const sorted = [...starts].sort((a, b) => a - b);
  for (let i = 0; i + MAX_ZOOMS_PER_SECOND < sorted.length; i++) {
    const first = sorted[i];
    const last = sorted[i + MAX_ZOOMS_PER_SECOND];
    if (first !== undefined && last !== undefined && last - first < 1000) {
      add({
        code: "too-many-zooms",
        severity: "error",
        message: `More than ${MAX_ZOOMS_PER_SECOND} zooms start within one second, around ${describeTime(first)}. That is too fast to follow.`,
        trackId: track.id,
      });
      break;
    }
  }
}

/** Only the problems that make an edit unsound. */
export function errorsOf(problems: readonly Problem[]): Problem[] {
  return problems.filter((p) => p.severity === "error");
}
