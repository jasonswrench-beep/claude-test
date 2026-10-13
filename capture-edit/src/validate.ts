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
  ALLOWED_SOURCES,
  clipEndMs,
  clipLengthMs,
  findSource,
  MAX_IMPORTED_VIDEO_MS,
  MAX_VOLUME_DB,
  MIN_CLIP_MS,
  MIN_VOLUME_DB,
  trackKindName,
  type Clip,
  type Model,
  type Track,
} from "./model.ts";
import { describeTime } from "./time.ts";

export type Severity = "error" | "needs-attention";

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
  | "needs-text-alternative";

export interface Problem {
  readonly code: ProblemCode;
  readonly severity: Severity;
  readonly message: string;
  readonly trackId?: string;
  readonly clipId?: string;
}

function isWhole(n: number): boolean {
  return Number.isInteger(n) && n >= 0;
}

function where(track: Track, clip: Clip): string {
  return `The clip "${clip.name}" on the ${track.name} track`;
}

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
    let previous: Clip | undefined;
    for (const clip of track.clips) {
      duplicate(clip.id, "clip", track.id);
      const here = { trackId: track.id, clipId: clip.id };
      const label = where(track, clip);

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

/** Only the problems that make an edit unsound. */
export function errorsOf(problems: readonly Problem[]): Problem[] {
  return problems.filter((p) => p.severity === "error");
}
