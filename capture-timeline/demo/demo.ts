// SPDX-License-Identifier: Apache-2.0
// The demo page: the clip list on a made-up lesson. Nothing here is saved or recorded.

import { serialize } from "../../capture-edit/src/index.ts";
import { mountClipList, type ClipListPanel } from "../src/panel.ts";
import { sampleModel } from "../src/sample.ts";
import { EditSession } from "../src/session.ts";

const host = document.getElementById("clips");
if (host === null) throw new Error("The demo page has no place for the clip list.");

// ?rows=cells reads each row by its cells instead of by one sentence, so the two can be compared with a screen reader.
const rows = new URLSearchParams(location.search).get("rows") === "cells" ? "cells" : "sentence";
document.getElementById("demo-note-url")!.textContent =
  rows === "cells"
    ? "Rows are being read cell by cell (the page address ends in ?rows=cells). Remove that to hear each row as one sentence."
    : "Rows are being read as one sentence each. To compare, add ?rows=cells to the end of the page address.";

let session = new EditSession(sampleModel());
let panel: ClipListPanel = mountClipList(host, session, { rows });

document.getElementById("start-again")?.addEventListener("click", () => {
  panel.destroy();
  session = new EditSession(sampleModel());
  panel = mountClipList(host, session, { rows });
  panel.announce("Started again with the sample lesson.");
  panel.focus();
});

document.getElementById("save-copy")?.addEventListener("click", () => {
  const url = URL.createObjectURL(new Blob([serialize(session.model)], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = "edit.json";
  link.click();
  URL.revokeObjectURL(url);
  panel.announce("A copy of the edit was saved as edit.json in your downloads.");
});
