// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { serialize } from "../../capture-edit/src/index.ts";
import { fromDirectoryHandle, libraryOf, mediaPath, memoryFolder, readProject, takeFolder } from "../src/project.ts";
import { sampleModel } from "../src/sample.ts";

const manifest = JSON.stringify({ format: "openstage-capture-project", schemaVersion: "0.2.5", id: "cap-1", title: "Lesson 1", language: "en-US", tracks: {}, edit: null });

function take(id: number, state = "complete", durationMs: number | null = 12_000, media = [{ file: "screen.mp4", kind: "screen" }, { file: "mic.wav", kind: "microphone" }]) {
  return JSON.stringify({ id, state, durationMs, media });
}

test("a recording folder becomes an edit made from its takes, with the title it was given", async () => {
  const folder = memoryFolder("lesson.capture", {
    "capture.json": manifest,
    "takes/take-001/take.json": take(1),
    "takes/take-002/take.json": take(2, "recovered", 8000),
  });
  const result = await readProject(folder);
  assert.ok(result.ok);
  assert.equal(result.project.title, "Lesson 1");
  assert.equal(result.project.fromEditFile, false);
  assert.deepEqual(result.project.model.tracks.map((t) => t.name), ["Screen", "Microphone"]);
  assert.deepEqual(result.project.model.tracks[0]?.clips.map((c) => [c.name, c.startMs]), [["Screen of take 1", 0], ["Screen of take 2", 12_000]]);
  assert.deepEqual(result.project.notes, ["2 takes were laid one after another, 20 seconds in all."]);
});

test("an edit saved in the folder is opened as it was, with what is still wrong with it", async () => {
  const saved = serialize(sampleModel());
  const folder = memoryFolder("lesson.capture", { "capture.json": manifest, "edit.json": saved, "takes/take-001/take.json": take(1) });
  const result = await readProject(folder);
  assert.ok(result.ok);
  assert.equal(result.project.fromEditFile, true);
  assert.deepEqual(result.project.notes, ["Opened the edit that was saved in this recording."]);
  assert.equal(result.project.model.tracks.length, 5);
});

test("an edit that cannot be read is said so and a new one is made from the takes", async () => {
  const folder = memoryFolder("lesson.capture", { "capture.json": manifest, "edit.json": "{not json", "takes/take-001/take.json": take(1) });
  const result = await readProject(folder);
  assert.ok(result.ok);
  assert.equal(result.project.fromEditFile, false);
  assert.match(result.project.notes[0] ?? "", /^The saved edit could not be read \(.+\), so a new edit was made from the takes\.$/);
  assert.equal(result.project.model.tracks.length, 2);
});

test("a folder that is not a recording is refused in words", async () => {
  const none = await readProject(memoryFolder("holiday photos", {}));
  assert.deepEqual(none, { ok: false, message: "holiday photos is not a recording made by OpenStage Capture: it has no capture.json. Choose the folder that ends in .capture." });
  const broken = await readProject(memoryFolder("x.capture", { "capture.json": "{" }));
  assert.deepEqual(broken, { ok: false, message: "The file capture.json in x.capture could not be read, so the folder cannot be opened." });
  const other = await readProject(memoryFolder("x.capture", { "capture.json": JSON.stringify({ format: "something-else" }) }));
  assert.deepEqual(other, { ok: false, message: "x.capture is not a recording made by OpenStage Capture: capture.json is for something else." });
});

test("takes that cannot be used are left out, each with its reason", async () => {
  const folder = memoryFolder("lesson.capture", {
    "capture.json": manifest,
    "takes/take-001/take.json": take(1),
    "takes/take-002/take.json": "{oops",
    "takes/take-003/events.jsonl": "",
    "takes/take-004/take.json": JSON.stringify({ id: 4, state: "complete", durationMs: 5000 }),
    "takes/take-005/take.json": take(5, "recording"),
    "takes/notes/take.json": take(9),
  });
  const result = await readProject(folder);
  assert.ok(result.ok);
  assert.deepEqual(result.project.notes, [
    "The file take.json of take-002 could not be read, so that take is left out.",
    "There is no take.json in take-003, so that take is left out.",
    "Take 4 is a test take made of segments, which cannot be played or edited here, so it is left out.",
    "Take 5 was not finished (it was still recording, or the program stopped), so it is not in the edit. Recover it first.",
  ]);
  assert.equal(result.project.model.tracks[0]?.clips.length, 1);
});

test("a folder with no title is called by its name", async () => {
  const folder = memoryFolder("lesson.capture", { "capture.json": JSON.stringify({ format: "openstage-capture-project" }) });
  const result = await readProject(folder);
  assert.ok(result.ok);
  assert.equal(result.project.title, "lesson.capture");
  assert.deepEqual(result.project.notes, ["There is nothing recorded yet that can be edited."]);
});

test("where a take's files are, and the library hands out each address once", async () => {
  assert.equal(takeFolder(7), "take-007");
  assert.equal(mediaPath({ kind: "take", take: 1, media: "screen.mp4" }), "takes/take-001/screen.mp4");
  assert.equal(mediaPath({ kind: "asset", asset: "logo.png" }), undefined);
  let asked = 0;
  const folder = { ...memoryFolder("x", {}), mediaUrl: async (path: string) => (asked += 1, `blob:${path}`) };
  const library = libraryOf(folder);
  assert.equal(await library.url({ kind: "take", take: 1, media: "screen.mp4" }), "blob:takes/take-001/screen.mp4");
  assert.equal(await library.url({ kind: "take", take: 1, media: "screen.mp4" }), "blob:takes/take-001/screen.mp4");
  assert.equal(asked, 1);
  assert.equal(await library.url({ kind: "asset", asset: "logo.png" }), undefined);
});

test("writing the edit back into the folder", async () => {
  const folder = memoryFolder("lesson.capture", { "capture.json": manifest });
  assert.equal(await folder.writeText("edit.json", "{}"), true);
  assert.equal(await folder.readText("edit.json"), "{}");
});

// ---- a folder picked with the browser's folder picker ----

interface FakeFile {
  text: string;
}
class FakeDir {
  readonly name: string;
  readonly files = new Map<string, FakeFile>();
  readonly dirs = new Map<string, FakeDir>();
  constructor(name: string) {
    this.name = name;
  }
  async getFileHandle(name: string, options?: { create?: boolean }) {
    let file = this.files.get(name);
    if (file === undefined) {
      if (options?.create !== true) throw new DOMException("not found", "NotFoundError");
      file = { text: "" };
      this.files.set(name, file);
    }
    const made = file;
    return {
      name,
      getFile: async () => new File([made.text], name),
      createWritable: async () => ({
        write: async (data: string) => {
          made.text = data;
        },
        close: async () => undefined,
      }),
    };
  }
  async getDirectoryHandle(name: string): Promise<FakeDir> {
    const dir = this.dirs.get(name);
    if (dir === undefined) throw new DOMException("not found", "NotFoundError");
    return dir;
  }
  async *values() {
    for (const name of this.files.keys()) yield { kind: "file" as const, name };
    for (const name of this.dirs.keys()) yield { kind: "directory" as const, name };
  }
}

function fakeProject(): FakeDir {
  const root = new FakeDir("lesson.capture");
  root.files.set("capture.json", { text: manifest });
  const takes = new FakeDir("takes");
  const one = new FakeDir("take-001");
  one.files.set("take.json", { text: take(1) });
  one.files.set("screen.mp4", { text: "pretend picture" });
  takes.dirs.set("take-001", one);
  root.dirs.set("takes", takes);
  return root;
}

test("a folder from the folder picker is read, and its recordings are reached", async () => {
  const folder = fromDirectoryHandle(fakeProject());
  assert.equal(folder.name, "lesson.capture");
  assert.equal(folder.canWrite, true);
  assert.deepEqual(await folder.listFolders("takes"), ["take-001"]);
  assert.deepEqual(await folder.listFolders("nope"), []);
  assert.equal(await folder.readText("takes/take-001/take.json"), take(1));
  assert.equal(await folder.readText("takes/take-009/take.json"), undefined);
  assert.equal(await folder.readText("edit.json"), undefined);
  const url = await folder.mediaUrl("takes/take-001/screen.mp4");
  assert.match(url ?? "", /^blob:/);
  assert.equal(await folder.mediaUrl("takes/take-001/missing.mp4"), undefined);
  folder.release();
  const result = await readProject(folder);
  assert.ok(result.ok);
  assert.equal(result.project.title, "Lesson 1");
});

test("the edit is written into the folder, making edit.json if there is none, and replacing it if there is", async () => {
  const root = fakeProject();
  const folder = fromDirectoryHandle(root);
  assert.equal(await folder.writeText("edit.json", "first"), true);
  assert.equal(root.files.get("edit.json")?.text, "first", "made when there was none");
  assert.equal(await folder.writeText("edit.json", "second"), true);
  assert.equal(root.files.get("edit.json")?.text, "second", "replaced");
  assert.equal(await folder.writeText("takes/take-001/notes.txt", "inside a take"), true);
  assert.equal(await folder.writeText("nowhere/edit.json", "x"), false, "a folder that is not there cannot be written to");
});
