// SPDX-License-Identifier: Apache-2.0
// Makes a small recording folder for the browser check: three seconds of test picture and sound in the place
// OpenStage Capture keeps them. The picture files are WebM, not MP4, because the browser used for the check cannot play
// MP4 (H.264); they are given the names the recorder gives (screen.mp4) so the folder reads like a real one.
//
//   node tools/make-fixture.mjs <folder>        (needs ffmpeg; says so and stops if it is not there)

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const out = process.argv[2];
if (out === undefined) {
  console.error("Say which folder to make, for example: node tools/make-fixture.mjs /tmp/lesson.capture");
  process.exit(1);
}
const take = join(out, "takes", "take-001");
mkdirSync(take, { recursive: true });

function ffmpeg(args) {
  const run = spawnSync("ffmpeg", ["-y", "-loglevel", "error", ...args], { encoding: "utf8" });
  if (run.error !== undefined || run.status !== 0) {
    console.error(`ffmpeg could not make a test file: ${run.error?.message ?? run.stderr}`);
    process.exit(2);
  }
}

// 3 seconds. The screen is the standard test picture, the webcam a plain colour, each sound a different note.
ffmpeg(["-f", "lavfi", "-i", "testsrc=size=640x360:rate=15", "-t", "3", "-c:v", "libvpx", "-b:v", "300k", "-f", "webm", join(take, "screen.mp4")]);
ffmpeg(["-f", "lavfi", "-i", "color=c=orange:size=320x180:rate=15", "-t", "3", "-c:v", "libvpx", "-b:v", "100k", "-f", "webm", join(take, "webcam.mp4")]);
ffmpeg(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=16000", "-t", "3", "-c:a", "pcm_s16le", join(take, "mic.wav")]);
ffmpeg(["-f", "lavfi", "-i", "sine=frequency=660:sample_rate=16000", "-t", "3", "-c:a", "pcm_s16le", join(take, "system.wav")]);

writeFileSync(join(out, "capture.json"), JSON.stringify({ format: "openstage-capture-project", schemaVersion: "0.2.5", id: "cap-test", title: "Test lesson", language: "en-US", created: "2026-10-15T12:00:00Z", tracks: {}, edit: null }, null, 2));
writeFileSync(
  join(take, "take.json"),
  JSON.stringify({
    id: 1,
    state: "complete",
    durationMs: 3000,
    media: [
      { file: "screen.mp4", kind: "screen", container: "fragmented-mp4" },
      { file: "mic.wav", kind: "microphone", container: "wav" },
      { file: "system.wav", kind: "system-audio", container: "wav" },
      { file: "webcam.mp4", kind: "webcam", container: "fragmented-mp4" },
    ],
  }),
);
console.log(`Made ${out}`);
