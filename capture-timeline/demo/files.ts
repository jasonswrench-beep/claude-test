// SPDX-License-Identifier: Apache-2.0
// Opening and saving for the demo page: an edit file, and a recording folder.
//
// Where the browser has the file pickers (Chrome and Edge), they are used, so a file or folder opened can be saved back to
// the same place. Where it does not, an ordinary "choose a file" box is used for opening, and saving hands back a download.
// `?pickers=input` on the page address turns the pickers off, which is how the browser check reaches the fallback.

import { fromDirectoryHandle, fromFileList, type ProjectFolder } from "../src/project.ts";

interface FileHandle {
  readonly name: string;
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(data: string): Promise<void>; close(): Promise<void> }>;
}

interface Pickers {
  showOpenFilePicker?: (options: unknown) => Promise<FileHandle[]>;
  showSaveFilePicker?: (options: unknown) => Promise<FileHandle>;
  showDirectoryPicker?: (options: unknown) => Promise<Parameters<typeof fromDirectoryHandle>[0]>;
}

const pickers = (): Pickers => (new URLSearchParams(location.search).get("pickers") === "input" ? {} : (window as unknown as Pickers));

const JSON_TYPES = [{ description: "Edit file", accept: { "application/json": [".json"] } }];

/** An edit file that was opened, and where it can be saved back to, if that is known. */
export interface OpenedFile {
  readonly name: string;
  readonly text: string;
  readonly handle: FileHandle | undefined;
}

function cancelled(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

/** Ask for an edit file. Undefined if the author backed out. Must be called straight from a click. */
export async function chooseEditFile(): Promise<OpenedFile | undefined> {
  const open = pickers().showOpenFilePicker;
  if (open !== undefined) {
    try {
      const [handle] = await open.call(window, { types: JSON_TYPES, multiple: false });
      if (handle === undefined) return undefined;
      return { name: handle.name, text: await (await handle.getFile()).text(), handle };
    } catch (error) {
      if (cancelled(error)) return undefined;
      throw error;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json";
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (file === undefined) resolve(undefined);
      else void file.text().then((text) => resolve({ name: file.name, text, handle: undefined }));
    });
    input.addEventListener("cancel", () => resolve(undefined));
    input.click();
  });
}

/** Ask for a recording folder (the one that ends in .capture). Undefined if the author backed out. */
export async function chooseRecordingFolder(): Promise<ProjectFolder | undefined> {
  const pick = pickers().showDirectoryPicker;
  if (pick !== undefined) {
    try {
      return fromDirectoryHandle(await pick.call(window, { mode: "readwrite" }));
    } catch (error) {
      if (cancelled(error)) return undefined;
      throw error;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.setAttribute("webkitdirectory", "");
    input.multiple = true;
    input.addEventListener("change", () => resolve(input.files === null ? undefined : fromFileList(input.files)));
    input.addEventListener("cancel", () => resolve(undefined));
    input.click();
  });
}

/** What saving did: where it went, in words. */
export interface Saved {
  readonly where: string;
  readonly handle: FileHandle | undefined;
}

function download(name: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}

/**
 * Save the text of an edit. Into the file it was opened from if there is one, else to a place the author chooses, else as
 * a download. `asCopy` always chooses a place (or downloads). Undefined if the author backed out.
 */
export async function saveEditFile(text: string, handle: FileHandle | undefined, asCopy: boolean): Promise<Saved | undefined> {
  const save = pickers().showSaveFilePicker;
  let target = asCopy ? undefined : handle;
  if (target === undefined && save !== undefined) {
    try {
      target = await save.call(window, { suggestedName: "edit.json", types: JSON_TYPES });
    } catch (error) {
      if (cancelled(error)) return undefined;
      throw error;
    }
  }
  if (target === undefined) {
    download("edit.json", text);
    return { where: "edit.json in your downloads", handle: undefined };
  }
  const writable = await target.createWritable();
  await writable.write(text);
  await writable.close();
  return { where: target.name, handle: target };
}
