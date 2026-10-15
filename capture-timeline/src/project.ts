// SPDX-License-Identifier: Apache-2.0
/**
 * Opening a recording: the project folder (`lesson.capture`) that OpenStage Capture writes, turned into an edit to work
 * on and a way to get at its recordings for the preview.
 *
 * Reading a folder is separated from how the folder is reached. `readProject` works on a `ProjectFolder`, which is
 * implemented twice here: on a folder the author picked with the browser's folder picker (which can also write
 * `edit.json` back into it), and on the list of files a plain "choose a folder" box gives (which cannot). Tests use a
 * folder held in memory.
 *
 * The layout it reads is in `docs/capture-project-format.md`: `capture.json`, `edit.json` when there has been an edit, and
 * `takes/take-001/take.json` for each take, with its media files beside it.
 */

import {
  editFromTakes,
  parse,
  serialize,
  type Model,
  type Problem,
  type SourceRef,
  type TakeFile,
  type TakeInfo,
} from "../../capture-edit/src/index.ts";

/** A recording folder, however it was reached. Paths are relative to the folder and use `/`. */
export interface ProjectFolder {
  readonly name: string;
  /** Whether `writeText` can work (a picked folder can; a list of files cannot). */
  readonly canWrite: boolean;
  /** The text of a file, or undefined if there is none. */
  readText(path: string): Promise<string | undefined>;
  /** The names of the folders directly inside a folder. */
  listFolders(path: string): Promise<string[]>;
  /** An address a media element can play, for a file, or undefined if there is none. */
  mediaUrl(path: string): Promise<string | undefined>;
  /** Write a text file, replacing it. Says whether it could. */
  writeText(path: string, text: string): Promise<boolean>;
  /** Let go of any addresses made by `mediaUrl`. */
  release(): void;
}

/** What opening a recording gives. */
export interface OpenedProject {
  readonly folder: ProjectFolder;
  readonly title: string;
  readonly model: Model;
  /** Whether the edit came from an `edit.json` already in the folder (else it was made from the takes). */
  readonly fromEditFile: boolean;
  /** In words: what was used, and what was left out and why. */
  readonly notes: readonly string[];
  /** The checks of the edit that came from the file, in words (not for a new edit). */
  readonly problems: readonly Problem[];
}

export type ProjectResult = { readonly ok: true; readonly project: OpenedProject } | { readonly ok: false; readonly message: string };

/** The folder name of a take: 1 is `take-001`. */
export function takeFolder(take: number): string {
  return `take-${String(take).padStart(3, "0")}`;
}

/** Where a media file of a take is, relative to the project folder. */
export function mediaPath(ref: SourceRef): string | undefined {
  return ref.kind === "take" ? `takes/${takeFolder(ref.take)}/${ref.media}` : undefined;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readTake(text: string, folderName: string): TakeInfo | string {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    return `The file take.json of ${folderName} could not be read, so that take is left out.`;
  }
  if (!isObject(data) || typeof data["id"] !== "number") return `The file take.json of ${folderName} is not a take, so that take is left out.`;
  const media: TakeFile[] = [];
  if (Array.isArray(data["media"])) {
    for (const entry of data["media"]) {
      if (isObject(entry) && typeof entry["file"] === "string" && typeof entry["kind"] === "string") media.push({ file: entry["file"], kind: entry["kind"] });
    }
  }
  return {
    take: data["id"],
    state: typeof data["state"] === "string" ? data["state"] : "unknown",
    durationMs: typeof data["durationMs"] === "number" ? data["durationMs"] : null,
    media,
  };
}

/** Read a recording folder: its title, its takes, and the edit (the saved one, or a new one made from the takes). */
export async function readProject(folder: ProjectFolder): Promise<ProjectResult> {
  const manifestText = await folder.readText("capture.json");
  if (manifestText === undefined) {
    return { ok: false, message: `${folder.name} is not a recording made by OpenStage Capture: it has no capture.json. Choose the folder that ends in .capture.` };
  }
  let manifest: unknown;
  try {
    manifest = JSON.parse(manifestText);
  } catch {
    return { ok: false, message: `The file capture.json in ${folder.name} could not be read, so the folder cannot be opened.` };
  }
  if (!isObject(manifest) || manifest["format"] !== "openstage-capture-project") {
    return { ok: false, message: `${folder.name} is not a recording made by OpenStage Capture: capture.json is for something else.` };
  }
  const title = typeof manifest["title"] === "string" && manifest["title"] !== "" ? manifest["title"] : folder.name;

  const notes: string[] = [];
  const takes: TakeInfo[] = [];
  for (const name of (await folder.listFolders("takes")).sort()) {
    if (!/^take-\d+$/.test(name)) continue;
    const text = await folder.readText(`takes/${name}/take.json`);
    if (text === undefined) {
      notes.push(`There is no take.json in ${name}, so that take is left out.`);
      continue;
    }
    const take = readTake(text, name);
    if (typeof take === "string") notes.push(take);
    else if (take.media.length === 0 && take.state !== "recording" && take.state !== "setAside") {
      notes.push(`Take ${take.take} is a test take made of segments, which cannot be played or edited here, so it is left out.`);
    } else takes.push(take);
  }

  const saved = await folder.readText("edit.json");
  if (saved !== undefined) {
    const parsed = parse(saved);
    if (parsed.ok) {
      notes.push("Opened the edit that was saved in this recording.");
      return { ok: true, project: { folder, title, model: parsed.model, fromEditFile: true, notes, problems: parsed.problems } };
    }
    notes.push(`The saved edit could not be read (${parsed.message}), so a new edit was made from the takes.`);
  }
  const made = editFromTakes(takes);
  return { ok: true, project: { folder, title, model: made.model, fromEditFile: false, notes: [...notes, ...made.notes], problems: [] } };
}

/** The edit as the text to write into `edit.json`. */
export function editText(model: Model): string {
  return serialize(model);
}

// ---- a folder held in memory (for tests) ----

/** A recording folder made from a map of path to text. Media files are not kept; `mediaUrl` gives `blob:` addresses it is told of. */
export function memoryFolder(name: string, files: Readonly<Record<string, string>>, media: Readonly<Record<string, string>> = {}): ProjectFolder & { readonly written: Map<string, string> } {
  const written = new Map<string, string>();
  return {
    name,
    canWrite: true,
    written,
    readText: async (path) => written.get(path) ?? files[path],
    listFolders: async (path) => {
      const prefix = path === "" ? "" : `${path}/`;
      const seen = new Set<string>();
      for (const key of Object.keys(files)) {
        if (key.startsWith(prefix) && key.slice(prefix.length).includes("/")) seen.add(key.slice(prefix.length).split("/")[0] ?? "");
      }
      return [...seen];
    },
    mediaUrl: async (path) => media[path],
    writeText: async (path, text) => {
      written.set(path, text);
      return true;
    },
    release: () => undefined,
  };
}

// ---- a folder picked with the browser's folder picker (it can be written to) ----

interface DirectoryHandle {
  readonly name: string;
  getFileHandle(name: string, options?: { create?: boolean }): Promise<{ getFile(): Promise<File>; createWritable(): Promise<{ write(data: string): Promise<void>; close(): Promise<void> }> }>;
  getDirectoryHandle(name: string): Promise<DirectoryHandle>;
  values(): AsyncIterable<{ readonly kind: "file" | "directory"; readonly name: string }>;
}

/** Walk down a path of folders, or undefined if one is missing. */
async function walk(root: DirectoryHandle, parts: readonly string[]): Promise<DirectoryHandle | undefined> {
  let here = root;
  for (const part of parts) {
    try {
      here = await here.getDirectoryHandle(part);
    } catch {
      return undefined;
    }
  }
  return here;
}

/** A folder from `showDirectoryPicker()` (Chrome and Edge). */
export function fromDirectoryHandle(root: DirectoryHandle): ProjectFolder {
  const urls: string[] = [];
  const split = (path: string): { dir: string[]; file: string } => {
    const parts = path.split("/").filter((p) => p !== "");
    return { dir: parts.slice(0, -1), file: parts[parts.length - 1] ?? "" };
  };
  return {
    name: root.name,
    canWrite: true,
    async readText(path) {
      const { dir, file } = split(path);
      const folder = await walk(root, dir);
      if (folder === undefined) return undefined;
      try {
        return await (await (await folder.getFileHandle(file)).getFile()).text();
      } catch {
        return undefined;
      }
    },
    async listFolders(path) {
      const folder = await walk(root, path.split("/").filter((p) => p !== ""));
      if (folder === undefined) return [];
      const names: string[] = [];
      for await (const entry of folder.values()) if (entry.kind === "directory") names.push(entry.name);
      return names;
    },
    async mediaUrl(path) {
      const { dir, file } = split(path);
      const folder = await walk(root, dir);
      if (folder === undefined) return undefined;
      try {
        const url = URL.createObjectURL(await (await folder.getFileHandle(file)).getFile());
        urls.push(url);
        return url;
      } catch {
        return undefined;
      }
    },
    async writeText(path, text) {
      const { dir, file } = split(path);
      const folder = await walk(root, dir);
      if (folder === undefined) return false;
      try {
        // `create` makes edit.json the first time; without it a folder with no edit yet could not be written to.
        const writable = await (await folder.getFileHandle(file, { create: true })).createWritable();
        await writable.write(text);
        await writable.close();
        return true;
      } catch {
        return false;
      }
    },
    release() {
      for (const url of urls.splice(0)) URL.revokeObjectURL(url);
    },
  };
}

// ---- a folder from a "choose a folder" box (it cannot be written to) ----

/** A folder from a file box that was given a whole folder (`webkitdirectory`). `edit.json` cannot be written back into it. */
export function fromFileList(files: FileList | readonly File[]): ProjectFolder | undefined {
  const all = [...files];
  const first = all[0];
  if (first === undefined) return undefined;
  const root = first.webkitRelativePath.split("/")[0] ?? "";
  const byPath = new Map<string, File>();
  for (const file of all) {
    const parts = file.webkitRelativePath.split("/");
    byPath.set(parts.slice(1).join("/"), file);
  }
  const urls: string[] = [];
  return {
    name: root === "" ? "the folder" : root,
    canWrite: false,
    readText: async (path) => byPath.get(path)?.text(),
    listFolders: async (path) => {
      const prefix = path === "" ? "" : `${path}/`;
      const seen = new Set<string>();
      for (const key of byPath.keys()) if (key.startsWith(prefix) && key.slice(prefix.length).includes("/")) seen.add(key.slice(prefix.length).split("/")[0] ?? "");
      return [...seen];
    },
    mediaUrl: async (path) => {
      const file = byPath.get(path);
      if (file === undefined) return undefined;
      const url = URL.createObjectURL(file);
      urls.push(url);
      return url;
    },
    writeText: async () => false,
    release: () => {
      for (const url of urls.splice(0)) URL.revokeObjectURL(url);
    },
  };
}

/** The files of a recording that the preview can play, by the address of each. */
export interface MediaLibrary {
  /** An address for a recording, or undefined if there is none (the demo's made-up recordings have none). */
  url(ref: SourceRef): Promise<string | undefined>;
}

/** A library with nothing in it: every picture and sound is shown as a placeholder. */
export const EMPTY_LIBRARY: MediaLibrary = { url: async () => undefined };

/** The media of an open recording. */
export function libraryOf(folder: ProjectFolder): MediaLibrary {
  const cache = new Map<string, Promise<string | undefined>>();
  return {
    url(ref) {
      const path = mediaPath(ref);
      if (path === undefined) return Promise.resolve(undefined);
      let found = cache.get(path);
      if (found === undefined) {
        found = folder.mediaUrl(path);
        cache.set(path, found);
      }
      return found;
    },
  };
}
