// SPDX-License-Identifier: Apache-2.0
// The demo page: the preview, the timeline and the clip list on a made-up lesson, with opening and saving. Nothing here is
// recorded, and nothing is saved unless you choose Save.

import { addTrack, describeLength, listCaption, parse, serialize, modelLengthMs, type Model } from "../../capture-edit/src/index.ts";
import { mountEditor, type Editor } from "../src/editor.ts";
import { EMPTY_LIBRARY, libraryOf, readProject, type ProjectFolder } from "../src/project.ts";
import { longSampleModel, sampleModel } from "../src/sample.ts";
import { EditSession } from "../src/session.ts";
import { chooseEditFile, chooseRecordingFolder, saveEditFile, type OpenedFile } from "./files.ts";

const host = document.getElementById("clips");
if (host === null) throw new Error("The demo page has no place for the editor.");
const params = new URLSearchParams(location.search);

// ?rows=cells reads each row by its cells instead of by one sentence, so the two can be compared with a screen reader.
const rows = params.get("rows") === "cells" ? "cells" : "sentence";
const rowsNote = document.getElementById("demo-note-url");
if (rowsNote !== null) {
  rowsNote.textContent =
    rows === "cells"
      ? "Rows are being read cell by cell (the page address ends in ?rows=cells). Remove that to hear each row as one sentence."
      : "Rows are being read as one sentence each. To compare, add ?rows=cells to the end of the page address.";
}

/** The sample lesson with two empty tracks, so there is somewhere to move a clip or an overlay to. */
function demoModel(): Model {
  if (params.get("long") === "1") return longSampleModel();
  return addTrack(addTrack(sampleModel(), "overlay").model, "audio", "Audio 2").model;
}

interface Current {
  session: EditSession;
  editor: Editor;
  /** What is open, in words, for the line under the buttons. */
  label: string;
  folder: ProjectFolder | undefined;
  handle: OpenedFile["handle"];
}

let current: Current | undefined;
const fileState = document.getElementById("file-state");

function showState(): void {
  if (current === undefined) return;
  const dirty = current.session.dirty;
  if (fileState !== null) fileState.textContent = `${current.label} ${dirty ? "There are changes that are not saved." : "There are no unsaved changes."}`;
  document.title = `${dirty ? "* " : ""}${current.label.split(".")[0] ?? "Editor"} - OpenStage Capture`;
}

/** Put an edit on the screen in place of whatever was there. */
function load(model: Model, label: string, say: string, options: { folder?: ProjectFolder; handle?: OpenedFile["handle"]; focus?: boolean } = {}): void {
  if (current !== undefined) {
    current.editor.destroy();
    current.folder?.release();
  }
  const session = new EditSession(model);
  const editor = mountEditor(host as HTMLElement, session, { rows, library: options.folder === undefined ? EMPTY_LIBRARY : libraryOf(options.folder) });
  current = { session, editor, label, folder: options.folder, handle: options.handle };
  session.subscribe((kind) => {
    if (kind === "model" || kind === "saved") showState();
  });
  showState();
  editor.announce(say);
  if (options.focus !== false) editor.focus();
}

function say(message: string): void {
  current?.editor.announce(message);
}

function problemsNote(problems: readonly { message: string }[]): string {
  const first = problems[0];
  return first === undefined ? "" : ` ${problems.length === 1 ? "One thing needs a look" : `${problems.length} things need a look`}: ${first.message}`;
}

// ---- the buttons ----

document.getElementById("start-again")?.addEventListener("click", () => {
  load(demoModel(), "The made-up lesson.", "Started again with the sample lesson.");
});

document.getElementById("open-file")?.addEventListener("click", () => {
  void (async () => {
    try {
      const file = await chooseEditFile();
      if (file === undefined) return;
      const parsed = parse(file.text);
      if (!parsed.ok) {
        say(`${file.name} was not opened: ${parsed.message}`);
        return;
      }
      load(parsed.model, `${file.name}.`, `Opened ${file.name}. ${listCaption(parsed.model)}${problemsNote(parsed.problems)}`, { handle: file.handle });
    } catch (error) {
      say(`The file could not be opened: ${error instanceof Error ? error.message : String(error)}`);
    }
  })();
});

document.getElementById("open-folder")?.addEventListener("click", () => {
  void (async () => {
    try {
      const folder = await chooseRecordingFolder();
      if (folder === undefined) return;
      const result = await readProject(folder);
      if (!result.ok) {
        folder.release();
        say(result.message);
        return;
      }
      const { project } = result;
      const note = project.notes.join(" ");
      load(project.model, `The recording ${project.title}.`, `Opened the recording ${project.title}, ${describeLength(modelLengthMs(project.model))} long. ${listCaption(project.model)} ${note}${problemsNote(project.problems)}`.replace(/\s+/g, " ").trim(), { folder: project.folder });
    } catch (error) {
      const refused = error instanceof DOMException && (error.name === "SecurityError" || error.name === "NotAllowedError");
      say(
        refused
          ? "The browser would not let this page use that folder. Choose the folder that ends in .capture itself, not a folder that holds other things such as Documents or Downloads."
          : `The recording could not be opened: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  })();
});

async function save(asCopy: boolean): Promise<void> {
  if (current === undefined) return;
  const { session } = current;
  const text = serialize(session.model);
  try {
    if (!asCopy && current.folder?.canWrite === true && (await current.folder.writeText("edit.json", text))) {
      session.markSaved();
      say(`Saved edit.json in ${current.folder.name}.`);
      return;
    }
    const saved = await saveEditFile(text, asCopy ? undefined : current.handle, asCopy);
    if (saved === undefined) {
      say("Saving was cancelled. Nothing was saved.");
      return;
    }
    if (!asCopy) {
      current.handle = saved.handle ?? current.handle;
      session.markSaved();
    }
    say(`${asCopy ? "A copy was saved as" : "Saved as"} ${saved.where}.`);
  } catch (error) {
    say(`It could not be saved: ${error instanceof Error ? error.message : String(error)}`);
  }
}

document.getElementById("save")?.addEventListener("click", () => void save(false));
document.getElementById("save-copy")?.addEventListener("click", () => void save(true));

// Ctrl+S saves, wherever the focus is.
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "s") {
    e.preventDefault();
    void save(false);
  }
});

// Leaving the page with changes that are not saved asks first.
window.addEventListener("beforeunload", (e) => {
  if (current?.session.dirty === true) e.preventDefault();
});

// The page opens on the made-up lesson, without moving the focus.
load(demoModel(), "The made-up lesson.", "Ready. This is a made-up lesson. Choose a clip with the arrow keys.", { focus: false });
