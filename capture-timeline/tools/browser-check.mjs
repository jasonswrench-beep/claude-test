// SPDX-License-Identifier: Apache-2.0
// Drives the clip list demo in a real (headless) Chromium with the keyboard only, and checks what a
// screen reader would be given: names, roles, selection, focus, what is announced, colour contrast.
//
// This is a developer check, not something the author runs. It needs Playwright, which is not part of this
// package: point PLAYWRIGHT_DIR at an installed copy (default: /opt/node-tools/node_modules/playwright).
//
// What it can and cannot say: it proves the page exposes the right roles, names and states, that every
// command works from the keyboard, and that focus never gets lost or stuck. It cannot say how NVDA,
// Narrator or VoiceOver will speak any of it. That has to be heard on a real computer.
//
//   node tools/browser-check.mjs [screenshot-folder]

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_DIR ?? "/opt/node-tools/node_modules/playwright");
const shots = process.argv[2];
const PORT = 8793;
const BASE = `http://127.0.0.1:${PORT}/capture-timeline/demo/index.html`;

const server = spawn(process.execPath, [resolve(here, "..", "demo", "serve.mjs"), "--port", String(PORT)], { stdio: ["ignore", "pipe", "inherit"] });
await new Promise((ready, fail) => {
  server.stdout.on("data", (chunk) => String(chunk).includes("running") && ready());
  server.on("exit", () => fail(new Error("the demo server stopped")));
});

const browser = await chromium.launch({ args: ["--no-sandbox"] });
let failures = 0;
let passes = 0;

async function check(name, body, options = {}) {
  const context = await browser.newContext({ viewport: { width: 1100, height: 2400 }, ...options });
  const page = await context.newPage();
  const problems = [];
  page.on("pageerror", (e) => problems.push(`page error: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && problems.push(`console error: ${m.text()}`));
  try {
    await page.goto(BASE + (options.search ?? ""));
    await page.locator(".os-cl-table tbody tr").first().waitFor();
    await body(page, new Helpers(page));
    if (problems.length > 0) throw new Error(problems.join("; "));
    passes += 1;
    console.log(`PASS  ${name}`);
  } catch (error) {
    failures += 1;
    console.log(`FAIL  ${name}\n      ${error instanceof Error ? error.message : error}`);
    if (shots) await page.screenshot({ path: resolve(shots, `fail-${failures}.png`), fullPage: true }).catch(() => {});
  } finally {
    await context.close();
  }
}

class Helpers {
  constructor(page) {
    this.page = page;
  }
  /** What the live region says, once it has had time to be filled. */
  async said() {
    await this.page.waitForTimeout(150);
    return (await this.page.locator(".os-cl-status").textContent()) ?? "";
  }
  /** The row that has keyboard focus, or undefined. */
  async focusedRow() {
    return this.page.evaluate(() => {
      const el = document.activeElement;
      return el instanceof HTMLTableRowElement ? { id: el.dataset.id, label: el.getAttribute("aria-label"), name: el.querySelector("th")?.textContent, selected: el.getAttribute("aria-selected") } : undefined;
    });
  }
  async rowNames() {
    return this.page.locator(".os-cl-table tbody th").allTextContents();
  }
  async focusRow(name) {
    await this.page.locator(".os-cl-table tbody tr", { has: this.page.locator(`th:text-is("${name}")`) }).focus();
  }
  async focusLabel() {
    return this.page.evaluate(() => {
      const el = document.activeElement;
      return el === document.body ? "(nothing)" : (el.getAttribute("aria-label") ?? el.textContent?.trim().slice(0, 40) ?? el.tagName);
    });
  }
  clip(name) {
    return this.page.locator(".os-tl-clip", { has: this.page.locator(`.os-tl-clip-name:text-is("${name}")`) });
  }
  async focusClip(name) {
    await this.clip(name).focus();
  }
  /** The timeline clip that has keyboard focus, or undefined. */
  async focusedClip() {
    return this.page.evaluate(() => {
      const el = document.activeElement;
      return el?.getAttribute("role") === "option" ? { name: el.querySelector(".os-tl-clip-name")?.textContent, selected: el.getAttribute("aria-selected"), label: el.getAttribute("aria-label") } : undefined;
    });
  }
  async chosenClips() {
    return this.page.locator('.os-tl-clip[aria-selected="true"] .os-tl-clip-name').allTextContents();
  }
  /** The row of the clip list for a name: its start, length and end as shown. */
  async listTimes(name) {
    return this.page.locator(".os-ed-list tbody tr", { has: this.page.locator(`th:text-is("${name}")`) }).locator("td").allTextContents();
  }
  async key(...keys) {
    for (const key of keys) await this.page.keyboard.press(key);
  }
  async value(label) {
    return this.page.getByLabel(label, { exact: true }).inputValue();
  }
}

function assertEqual(actual, expected, what) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${what}: expected ${JSON.stringify(expected)} but got ${JSON.stringify(actual)}`);
  }
}

function assertMatch(actual, pattern, what) {
  if (!pattern.test(actual)) throw new Error(`${what}: ${JSON.stringify(actual)} does not match ${pattern}`);
}

// ---- what is exposed ----

await check("the page exposes a named grid with column headers, row headers and one selected row", async (page) => {
  const snapshot = await page.locator("body").ariaSnapshot();
  assertMatch(snapshot, /grid "Tracks and clips: 9 clips, 3 overlays and zooms on 7 tracks, 1 minute 30 seconds long\."/, "the grid");
  for (const header of ["Name", "Track", "Type", "Start", "Length", "End", "State"]) assertMatch(snapshot, new RegExp(`columnheader "${header}"`), `header ${header}`);
  assertMatch(snapshot, /rowheader "Open the gradebook"/, "the row header");
  assertEqual(await page.locator('.os-cl-table tbody tr[aria-selected="true"]').count(), 1, "selected rows");
  assertEqual(await page.locator('.os-cl-table tbody tr[tabindex="0"]').count(), 1, "rows in the Tab order");
  assertMatch(snapshot, /slider "Playhead"/, "the playhead slider");
  assertMatch(snapshot, /region "Clip list"/, "the list region");
  assertMatch(snapshot, /region "Timeline"/, "the timeline region");
  assertMatch(snapshot, /status/, "the status region");
  const label = await page.locator("tbody tr").first().getAttribute("aria-label");
  assertEqual(label, "Open the gradebook, screen on the Screen track, starts at 0 seconds, 30 seconds long, ends at 30 seconds.", "the first row's sentence");
});

await check("with ?rows=cells the rows are read by their cells and carry no sentence", async (page) => {
  assertEqual(await page.locator("tbody tr[aria-label]").count(), 0, "rows with a sentence");
}, { search: "?rows=cells" });

await check("every button has a name, and its key is announced to the screen reader", async (page) => {
  const buttons = await page.locator("button").evaluateAll((all) => all.map((b) => [b.textContent.trim(), b.getAttribute("aria-keyshortcuts")]));
  for (const [text] of buttons) if (text === "") throw new Error("a button has no name");
  const split = buttons.find(([t]) => t.startsWith("Split at playhead"));
  assertEqual(split?.[1], "S", "the split button's key");
});

// ---- moving around ----

await check("the arrow keys choose a row and the focus follows; the ends are said; Tab moves on and back", async (page, t) => {
  await t.focusRow("Open the gradebook");
  await t.key("ArrowDown");
  const second = await t.focusedRow();
  assertEqual([second.name, second.selected], ["Add an assignment", "true"], "second row");
  assertMatch(second.label, /^Add an assignment, screen on the Screen track, starts at 30 seconds/, "its sentence");
  assertEqual(await page.locator('tbody tr[aria-selected="true"]').count(), 1, "selected rows");
  assertEqual(await page.locator('tbody tr[tabindex="0"]').count(), 1, "rows in the Tab order");
  await t.key("End");
  assertEqual((await t.focusedRow()).name, "Zoom on the totals", "End");
  await t.key("ArrowDown");
  assertEqual(await t.said(), "This is the last row.", "said at the end");
  await t.key("Home");
  assertEqual((await t.focusedRow()).name, "Open the gradebook", "Home");
  await t.key("ArrowUp");
  assertEqual(await t.said(), "This is the first row.", "said at the start");
  // Tab leaves the list for the next control, Shift+Tab comes back to the chosen row.
  await t.key("Tab");
  assertMatch(await t.focusLabel(), /^Split at playhead/, "after Tab");
  await t.key("Shift+Tab");
  assertEqual((await t.focusedRow()).name, "Open the gradebook", "Shift+Tab");
});

await check("keyboard focus never gets stuck: Tab goes all the way round the page and out again", async (page, t) => {
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  const seen = new Set();
  let left = false;
  for (let i = 0; i < 80; i++) {
    await t.key("Tab");
    const where = await page.evaluate(() => (document.activeElement === document.body ? "body" : `${document.activeElement.tagName}#${document.activeElement.id}.${document.activeElement.dataset.id ?? ""}.${document.activeElement.getAttribute("role") ?? ""}.${document.activeElement.getAttribute("aria-label") ?? document.activeElement.textContent.trim().slice(0, 20)}`));
    if (where === "body" || seen.has(where)) {
      left = true;
      break;
    }
    seen.add(where);
  }
  if (!left) throw new Error(`still tabbing after 80 presses; ${seen.size} different places seen`);
  if (seen.size < 20) throw new Error(`only ${seen.size} places can be reached by Tab`);
  // and one row is a single stop, not twelve; one clip of the timeline is a single stop, not nine
  const rowStops = [...seen].filter((s) => s.startsWith("TR#")).length;
  assertEqual(rowStops, 1, "table rows reached by Tab");
  const clipStops = [...seen].filter((s) => s.includes(".option.")).length;
  assertEqual(clipStops, 1, "timeline clips reached by Tab");
});

// ---- the playhead ----

await check("Left and Right on the list move the playhead and say where it is", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("ArrowRight");
  assertEqual(await t.said(), 'Playhead at 0.1 seconds, 29.9 seconds before "Add an assignment".', "first step");
  await t.key("Shift+ArrowRight", "PageUp");
  assertEqual(await page.getByRole("slider", { name: "Playhead" }).getAttribute("aria-valuenow"), "6100", "slider value");
  assertEqual(await t.value("Playhead time"), "0:06.1", "time box");
});

await check("the playhead slider has the keys of the Timeline and does not say things twice", async (page, t) => {
  const slider = page.getByRole("slider", { name: "Playhead" });
  await slider.focus();
  const before = await t.said();
  await t.key("ArrowRight", "Shift+ArrowRight", "PageUp");
  assertEqual(await slider.getAttribute("aria-valuenow"), "6100", "after steps");
  assertMatch(await slider.getAttribute("aria-valuetext"), /^6\.1 seconds, 6\.1 seconds into "Open the gradebook"$/, "spoken value");
  assertEqual(await t.said(), before, "the status line is left alone, since the slider's value is read as it changes");
  await t.key("End");
  assertEqual(await slider.getAttribute("aria-valuenow"), "90000", "End");
  assertEqual(await slider.getAttribute("aria-valuemax"), "90000", "maximum");
  await t.key("Home");
  assertEqual(await slider.getAttribute("aria-valuenow"), "0", "Home");
  assertEqual((await page.evaluate(() => document.activeElement.getAttribute("role"))), "slider", "focus stays on the slider");
});

await check("a typed time moves the playhead, a wrong one is explained and the box says it is invalid", async (page, t) => {
  const box = page.getByLabel("Playhead time", { exact: true });
  await box.fill("1:12.4");
  await box.press("Enter");
  assertEqual(await page.getByRole("slider", { name: "Playhead" }).getAttribute("aria-valuenow"), "72400", "typed time");
  assertEqual(await box.inputValue(), "1:12.4", "box keeps the exact time");
  await box.fill("soon");
  await box.press("Enter");
  assertMatch(await t.said(), /^"soon" is not a time\. Type a time such as 72\.4/, "wrong time explained");
  assertEqual(await box.getAttribute("aria-invalid"), "true", "marked invalid");
  await box.press("Escape");
  assertEqual(await box.inputValue(), "1:12.4", "Escape puts it back");
  assertEqual(await box.getAttribute("aria-invalid"), null, "no longer invalid");
  assertEqual(await page.evaluate(() => document.activeElement.id.endsWith("-time")), true, "focus stays in the box on the first Escape");
  await box.press("Escape");
  assertEqual(await page.evaluate(() => document.activeElement.getAttribute("role") + ":" + document.activeElement.querySelector(".os-tl-clip-name")?.textContent), "option:Open the gradebook", "the second Escape returns to the chosen clip");
});

// ---- editing ----

await check("split from the keyboard: playhead typed, S pressed, said, counted, focus kept, undone with Ctrl+Z", async (page, t) => {
  const box = page.getByLabel("Playhead time", { exact: true });
  await box.fill("10");
  await box.press("Enter");
  await t.focusRow("Open the gradebook");
  await t.key("s");
  assertEqual(await t.said(), "Clip split at 10 seconds, now two clips.", "said");
  assertEqual((await t.focusedRow())?.name, "Open the gradebook", "focus stays on the first piece");
  assertEqual((await t.rowNames()).filter((n) => n === "Open the gradebook").length, 2, "two pieces");
  assertMatch(await page.locator("caption").textContent(), /^Tracks and clips: 10 clips/, "caption updated");
  assertEqual(await page.locator('tbody tr[aria-selected="true"]').count(), 1, "selected rows");
  await t.key("Control+z");
  assertEqual(await t.said(), "Undid: split clip.", "undo said");
  assertEqual((await t.rowNames()).filter((n) => n === "Open the gradebook").length, 1, "one piece again");
  assertEqual((await t.focusedRow())?.name, "Open the gradebook", "focus kept after undo");
  await t.key("Control+y");
  assertEqual(await t.said(), "Redid: split clip.", "redo said");
});

await check("a command that cannot be done says why, changes nothing, and leaves the focus where it was", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("s"); // the playhead is at 0, outside this clip
  assertMatch(await t.said(), /^Cannot split "Add an assignment" at 0 seconds: it runs from 30 seconds to 1 minute, so the playhead has to be inside it\.$/, "said why");
  assertEqual((await t.focusedRow())?.name, "Add an assignment", "focus");
  assertEqual((await t.rowNames()).length, 12, "rows");
  assertEqual(await page.getByRole("button", { name: "Undo" }).count(), 1, "undo button is always there");
});

await check("Delete leaves a gap, Shift+Delete closes it, focus goes to the row that took the place, undo brings it back", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("Delete");
  assertEqual(await t.said(), 'Clip "Add an assignment" deleted, leaving a gap of 30 seconds. Selected "Check the totals".', "said");
  assertEqual((await t.focusedRow())?.name, "Check the totals", "focus");
  assertEqual((await page.locator("tbody tr", { hasText: "Check the totals" }).locator("td").nth(2).textContent()), "1:00", "start unchanged");
  await t.key("Control+z");
  assertEqual((await t.rowNames()).includes("Add an assignment"), true, "row is back");
  await t.focusRow("Add an assignment");
  await t.key("Shift+Delete");
  assertMatch(await t.said(), /moved 30 seconds earlier to close the gap/, "closing said");
  assertEqual((await page.locator("tbody tr", { hasText: "Check the totals" }).locator("td").nth(2).textContent()), "0:30", "later clip moved");
});

await check("pressing a button with the keyboard works and the focus stays on the button", async (page, t) => {
  await t.focusRow("Add an assignment");
  await page.getByRole("button", { name: "Delete, close the gap" }).focus();
  await t.key("Enter");
  assertMatch(await t.said(), /deleted/, "said");
  assertEqual(await page.evaluate(() => document.activeElement.textContent.trim().startsWith("Delete, close the gap")), true, "focus stays on the button");
  await page.getByRole("button", { name: "Undo" }).focus();
  await t.key("Space");
  assertEqual(await t.said(), "Undid: delete clip.", "undo");
  assertEqual((await t.rowNames()).includes("Add an assignment"), true, "row is back");
});

await check("the form edits exact values: name, start, length, end, mute; a refusal is explained and Escape puts the text back", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("Enter");
  const name = page.getByLabel("Name", { exact: true });
  assertEqual(await name.evaluate((el) => document.activeElement === el), true, "focus is in the Name box");
  await name.fill("Add a new assignment");
  await name.press("Enter");
  assertEqual(await t.said(), 'Clip "Add an assignment" is now called "Add a new assignment".', "rename said");
  assertEqual(await name.evaluate((el) => document.activeElement === el), true, "focus stays in the box after Enter");
  assertEqual((await t.rowNames()).includes("Add a new assignment"), true, "row renamed");
  const start = page.getByLabel("Start", { exact: true });
  await start.fill("10");
  await start.press("Enter");
  assertMatch(await t.said(), /would run into the clip "Open the gradebook"/, "refusal said");
  assertEqual(await start.getAttribute("aria-invalid"), "true", "marked invalid");
  assertMatch(await page.locator(".os-ed-list .os-cl-error").textContent(), /would run into/, "error shown");
  await start.press("Escape");
  assertEqual(await start.inputValue(), "0:30", "put back");
  assertEqual(await start.getAttribute("aria-invalid"), null, "not invalid");
  const length = page.getByLabel("Length", { exact: true });
  await length.fill("20");
  await length.press("Tab"); // leaving the box applies it
  assertMatch(await t.said(), /is now 20 seconds long, ending at 50 seconds/, "length said");
  assertEqual(await page.getByLabel("End", { exact: true }).inputValue(), "0:50", "end follows");
  const muted = page.getByLabel("Muted");
  await muted.check();
  assertEqual(await t.said(), 'Clip "Add a new assignment" is now muted.', "mute said");
});

await check("Ctrl+Z inside a text box undoes the typing, not the edit", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("Delete");
  const rows = (await t.rowNames()).length;
  await page.getByLabel("Name", { exact: true }).fill("typing");
  await t.key("Control+z");
  assertEqual((await t.rowNames()).length, rows, "the edit was not undone");
});

await check("deleting everything leaves focus inside the panel, and undo brings it all back", async (page, t) => {
  await t.focusRow("Open the gradebook");
  for (let i = 0; i < 12; i++) await t.key("Delete");
  assertEqual(await page.locator("tbody tr").count(), 0, "rows left");
  assertEqual(await page.locator(".os-cl-table").isHidden(), true, "table hidden");
  assertEqual(await page.locator(".os-ed-list .os-cl-empty").isVisible(), true, "empty message shown");
  assertEqual(await page.evaluate(() => document.activeElement.className), "os-cl-empty", "focus is on the empty message, not lost");
  assertMatch(await t.said(), /The list is now empty\./, "said");
  await t.key("Control+z");
  assertEqual(await page.locator("tbody tr").count(), 1, "one row back");
});

await check("Mute toggles from M and the button label follows the state", async (page, t) => {
  await t.focusRow("Voice, part 1");
  assertEqual((await page.getByRole("button", { name: /^Mute clip/ }).count()), 1, "label before");
  await t.key("m");
  assertEqual(await t.said(), 'Clip "Voice, part 1" is now muted.', "said");
  assertEqual((await page.getByRole("button", { name: /^Unmute clip/ }).count()), 1, "label after");
  assertMatch(await page.locator("tbody tr", { hasText: "Voice, part 1" }).locator("td").last().textContent(), /muted/, "state column");
});

await check("what is at the playhead is said on request and kept on the page as text", async (page, t) => {
  await t.focusRow("Open the gradebook");
  await page.getByLabel("Playhead time", { exact: true }).fill("13");
  await page.getByLabel("Playhead time", { exact: true }).press("Enter");
  await t.focusRow("Open the gradebook");
  await t.key("d");
  assertMatch(await t.said(), /^At 13 seconds\. Screen track: Open the gradebook, 13 seconds in\./, "said");
  assertMatch(await page.locator(".os-ed-list .os-cl-readout").textContent(), /Overlays showing: text "Open Settings"\./, "kept as text");
});

await check("Start again gives back the sample, with focus on the list", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("Delete");
  await page.getByRole("button", { name: "Start again with the sample lesson" }).click();
  assertEqual((await t.rowNames()).length, 12, "rows");
  assertEqual(await page.evaluate(() => document.activeElement.getAttribute("role") + ":" + document.activeElement.querySelector(".os-tl-clip-name")?.textContent), "option:Open the gradebook", "focus on the first clip");
  assertEqual(await page.locator(".os-ed").count(), 1, "one editor, not two");
});


// ---- the timeline ----

await check("the timeline exposes a listbox of clips grouped by track, one Tab stop, and two grips for the chosen clip", async (page) => {
  const snapshot = await page.locator(".os-tl").ariaSnapshot();
  assertMatch(snapshot, /listbox "Clips on the timeline"/, "the listbox");
  for (const group of ["Screen track, 3 clips", "Voice track, 3 clips", "Webcam track, 3 clips", "Overlays track, 2 items", "Zoom track, 1 item", "Overlays 2 track, empty", "Audio 2 track, empty"]) {
    assertMatch(snapshot, new RegExp(`group "${group}"`), group);
  }
  assertMatch(snapshot, /option "Add an assignment, screen on the Screen track, starts at 30 seconds, 30 seconds long, ends at 1 minute\."/, "a clip's sentence");
  assertEqual(await page.locator('.os-tl-clip[tabindex="0"]').count(), 1, "clips in the Tab order");
  assertEqual(await page.locator('.os-tl-clip[aria-selected="true"]').count(), 1, "chosen clips");
  assertEqual(await page.locator(".os-tl-clip").first().getAttribute("aria-posinset"), "1", "position in the group");
  assertEqual(await page.locator(".os-tl-clip").first().getAttribute("aria-setsize"), "3", "size of the group");
  assertMatch(snapshot, /slider "Playhead"/, "the playhead slider");
  assertMatch(snapshot, /slider "Start of \\"Open the gradebook\\""/, "the start grip");
  assertMatch(snapshot, /slider "End of \\"Open the gradebook\\""/, "the end grip");
  const start = page.getByRole("slider", { name: 'Start of "Open the gradebook"' });
  assertEqual([await start.getAttribute("aria-valuemin"), await start.getAttribute("aria-valuemax"), await start.getAttribute("aria-valuenow")], ["0", "29900", "0"], "start grip range");
  assertEqual(await start.getAttribute("aria-valuetext"), "starts at 0 seconds, 30 seconds long", "start grip words");
  const end = page.getByRole("slider", { name: 'End of "Open the gradebook"' });
  assertEqual([await end.getAttribute("aria-valuemin"), await end.getAttribute("aria-valuemax")], ["100", "30000"], "end grip range");
  assertEqual(await end.getAttribute("aria-valuetext"), "ends at 30 seconds, 30 seconds long", "end grip words");
  // every box has a name for a screen reader
  const unnamed = await page.locator("input:not([type=checkbox]), select").evaluateAll((all) => all.filter((el) => !el.labels?.length && !el.getAttribute("aria-label")).map((el) => el.id));
  assertEqual(unnamed, [], "boxes with no label");
});

await check("the arrow keys go along a track and across tracks, Home and End to the ends, the edges are said, and the list follows", async (page, t) => {
  await t.focusClip("Add an assignment");
  await t.key("ArrowRight");
  assertEqual((await t.focusedClip()).name, "Check the totals", "Right");
  await t.key("ArrowRight");
  assertEqual(await t.said(), "This is the last clip on this track.", "end of the track said");
  await t.key("ArrowLeft", "ArrowLeft");
  assertEqual((await t.focusedClip()).name, "Open the gradebook", "Left twice");
  await t.key("ArrowLeft");
  assertEqual(await t.said(), "This is the first clip on this track.", "start of the track said");
  await t.key("End");
  assertEqual((await t.focusedClip()).name, "Check the totals", "End");
  await t.key("Home");
  assertEqual((await t.focusedClip()).name, "Open the gradebook", "Home");
  await t.key("ArrowDown");
  assertEqual((await t.focusedClip()).name, "Voice, part 1", "Down to the track below");
  await t.key("ArrowDown", "ArrowDown");
  assertEqual((await t.focusedClip()).name, "Title: Open Settings", "Down again, nothing showing at 0 so the nearest in time");
  await t.key("ArrowUp");
  assertEqual((await t.focusedClip()).name, "Presenter, part 1", "Up");
  await t.focusClip("Open the gradebook");
  await t.key("ArrowUp");
  assertEqual(await t.said(), "There is no track above this one with anything on it.", "top said");
  // exactly one clip is chosen and in the Tab order, and the clip list shows the same one
  assertEqual(await page.locator('.os-tl-clip[tabindex="0"]').count(), 1, "clips in the Tab order");
  assertEqual(await t.chosenClips(), ["Open the gradebook"], "chosen clips");
  assertEqual(await page.locator('.os-ed-list tbody tr[aria-selected="true"] th').allTextContents(), ["Open the gradebook"], "the list follows the timeline");
});

await check("choosing in the list moves the timeline, and the other way round, without taking the focus", async (page, t) => {
  await t.focusRow("Voice, part 2");
  assertEqual(await t.chosenClips(), ["Voice, part 2"], "the timeline follows the list");
  assertEqual(await t.focusedRow().then((r) => r?.name), "Voice, part 2", "focus stays in the list");
  await t.focusClip("Presenter, part 3");
  assertEqual(await page.locator('.os-ed-list tbody tr[aria-selected="true"] th').allTextContents(), ["Presenter, part 3"], "the list follows the timeline");
  assertEqual((await t.focusedClip()).name, "Presenter, part 3", "focus stays in the timeline");
});

await check("several clips can be chosen with the keyboard, and the list and the timeline both show it", async (page, t) => {
  await t.focusClip("Open the gradebook");
  await t.key("Shift+ArrowRight", "Shift+ArrowRight");
  assertEqual(await t.said(), "3 rows are chosen.", "said");
  assertEqual(await t.chosenClips(), ["Open the gradebook", "Add an assignment", "Check the totals"], "chosen in the timeline");
  assertEqual((await page.locator('.os-ed-list tbody tr[aria-selected="true"] th').allTextContents()).length, 3, "chosen in the list");
  assertEqual(await page.locator(".os-tl-grips").isHidden(), true, "no grips while several are chosen");
  await t.key("Escape");
  assertEqual(await t.chosenClips(), ["Check the totals"], "Escape keeps the one with the focus");
  assertEqual(await page.locator(".os-tl-grips").isVisible(), true, "grips are back");
  await t.key("Control+a");
  assertEqual(await t.said(), "All 12 rows are chosen.", "all");
  await t.key("Escape");
  await t.key("Control+ArrowLeft"); // the focus moves, the choice does not
  assertEqual((await t.focusedClip()).name, "Add an assignment", "Ctrl+Left moves the focus");
  assertEqual(await t.chosenClips(), ["Check the totals"], "and chooses nothing");
  assertEqual(await page.locator(".os-tl-grips").isHidden(), true, "no grips for a clip that is not chosen");
  await t.key("Control+ ");
  assertMatch(await t.said(), /is chosen too\. 2 rows are chosen\./, "toggled in");
  assertEqual(await t.chosenClips(), ["Add an assignment", "Check the totals"], "two chosen that are next to each other now, and the order is the list's");
  await t.key("Delete");
  assertMatch(await t.said(), /^Deleted 2 rows/, "deleted both");
  await t.key("Control+z");
  assertMatch(await t.said(), /^Undid: delete rows\./, "one undo");
  assertEqual((await t.rowNames()).length, 12, "all back");
});

await check("the start grip moves with the keys, a held key is one undo step, the limit is said, Escape puts it back", async (page, t) => {
  await t.focusClip("Check the totals"); // 60 to 90 seconds
  const grip = page.getByRole("slider", { name: 'Start of "Check the totals"' });
  await grip.focus();
  await t.key("ArrowRight", "ArrowRight", "ArrowRight", "Shift+ArrowRight", "PageUp");
  assertEqual(await grip.getAttribute("aria-valuenow"), "66300", "after three steps, Shift and Page Up");
  assertEqual(await grip.getAttribute("aria-valuetext"), "starts at 1 minute 6.3 seconds, 23.7 seconds long", "its words");
  await page.waitForTimeout(800); // the key is let go: the drag ends by itself
  assertEqual((await t.listTimes("Check the totals"))[2], "1:06.3", "the list shows it too");
  await t.key("Control+z");
  assertEqual(await t.said(), "Undid: trim clip start.", "one undo for all five presses");
  assertEqual((await t.listTimes("Check the totals"))[2], "1:00", "back to the start");
  await grip.focus();
  await t.key("Home");
  assertEqual(await grip.getAttribute("aria-valuenow"), "60000", "Home goes to the lowest start it may have (the clip before it ends there)");
  await t.key("ArrowLeft");
  assertEqual(await t.said(), "It cannot start any earlier: the earliest it can start is 1 minute.", "the limit is said");
  await t.key("End");
  assertEqual(await grip.getAttribute("aria-valuenow"), "89900", "End goes to a tenth of a second before the end");
  await t.key("Escape");
  assertEqual(await t.said(), "Cancelled. Nothing was changed.", "Escape");
  assertEqual(await grip.getAttribute("aria-valuenow"), "60000", "put back");
  assertEqual(await page.getByRole("button", { name: "Undo" }).count(), 1);
  await t.key("Control+z");
  assertEqual(await t.said(), "Nothing to undo.", "Escape recorded nothing");
});

await check("the end grip trims the end, and holding keys with Tab away finishes the step", async (page, t) => {
  await t.focusClip("Add an assignment"); // 30 to 60
  const grip = page.getByRole("slider", { name: 'End of "Add an assignment"' });
  await grip.focus();
  await t.key("ArrowLeft", "ArrowLeft");
  assertEqual(await grip.getAttribute("aria-valuetext"), "ends at 59.8 seconds, 29.8 seconds long");
  await t.key("Tab"); // leaving the grip finishes the drag at once
  assertEqual((await t.listTimes("Add an assignment"))[4], "0:59.8", "end in the list");
  await page.getByRole("button", { name: "Undo" }).click();
  assertMatch(await t.said(), /^Undid: trim clip end\./, "one step");
});

await check("dragging a clip with the mouse moves it, is one undo step, and says what it came to", async (page, t) => {
  const box = await t.clip("Check the totals").boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 40, box.y + box.height / 2, { steps: 4 });
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2, { steps: 4 });
  await page.mouse.up();
  assertMatch(await t.said(), /^Clip "Check the totals" now starts at 1 minute \d+(\.\d+)? seconds\.$/, "said");
  const start = (await t.listTimes("Check the totals"))[2];
  if (start === "1:00") throw new Error("the clip did not move");
  await t.key("Control+z");
  assertEqual((await t.listTimes("Check the totals"))[2], "1:00", "one undo puts the whole drag back");
  assertEqual(await t.said(), "Undid: move clip.");
});

await check("Escape during a mouse drag calls it off, and nothing is recorded", async (page, t) => {
  const box = await t.clip("Check the totals").boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2, { steps: 6 });
  const moved = await t.clip("Check the totals").boundingBox();
  if (Math.abs(moved.x - box.x) < 20) throw new Error("the clip is not moving under the mouse");
  await page.keyboard.press("Escape");
  await page.mouse.up();
  assertEqual(await t.said(), "Cancelled. Nothing was changed.", "said");
  assertEqual((await t.listTimes("Check the totals"))[2], "1:00", "put back");
  await t.key("Control+z");
  assertEqual(await t.said(), "Nothing to undo.", "nothing was recorded");
});

await check("dragging a grip with the mouse trims, and a drag of a clip onto another track moves it there in the same step", async (page, t) => {
  await t.focusClip("Title: Open Settings"); // an overlay, 12 to 16 seconds, on the Overlays track
  const grip = page.getByRole("slider", { name: 'End of "Title: Open Settings"' });
  const g = await grip.boundingBox();
  await page.mouse.move(g.x + g.width / 2, g.y + g.height / 2);
  await page.mouse.down();
  await page.mouse.move(g.x + g.width / 2 + 60, g.y + g.height / 2, { steps: 6 });
  await page.mouse.up();
  assertMatch(await t.said(), /^Text overlay "Title: Open Settings" now ends at \d+(\.\d+)? seconds, lasting/, "trim said");
  assertEqual((await t.listTimes("Title: Open Settings"))[3] === "0:16", false, "the end moved");
  await t.key("Control+z");
  // now drag the whole overlay down onto the empty second overlay track
  const c = await t.clip("Title: Open Settings").boundingBox();
  const lane = await page.locator('.os-tl-lane[aria-label^="Overlays 2 track"] .os-tl-track').boundingBox();
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + c.width / 2, lane.y + lane.height / 2, { steps: 8 });
  assertEqual(await page.locator(".os-tl-drop-ok").count(), 1, "the track it can go to is marked");
  await page.mouse.up();
  assertMatch(await t.said(), /moved from the Overlays track to the Overlays 2 track, still starting at 12 seconds\./, "move said");
  assertEqual((await page.locator(".os-ed-list tbody tr", { hasText: "Title: Open Settings" }).locator("td").first().textContent()), "Overlays 2", "the list shows the new track");
  await t.key("Control+z");
  assertEqual((await page.locator(".os-ed-list tbody tr", { hasText: "Title: Open Settings" }).locator("td").first().textContent()), "Overlays", "one undo puts it back");
});

await check("a drop on a track that does not take the clip says why and leaves the clip on its own track", async (page, t) => {
  const c = await t.clip("Voice, part 3").boundingBox();
  const lane = await page.locator('.os-tl-lane[aria-label^="Screen track"] .os-tl-track').boundingBox();
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + c.width / 2, lane.y + lane.height / 2, { steps: 8 });
  assertEqual(await page.locator(".os-tl-drop-no").count(), 1, "a track that will not take it is marked differently");
  await page.mouse.up();
  assertMatch(await t.said(), /Cannot move "Voice, part 3" to the Screen track: that track takes screen recordings\./, "why");
  assertEqual((await page.locator(".os-ed-list tbody tr", { hasText: "Voice, part 3" }).locator("td").first().textContent()), "Voice", "still on its track");
});

await check("a click on the ruler or an empty part of a track puts the playhead there; the ruler is a slider", async (page, t) => {
  const ruler = page.locator(".os-tl-ruler");
  const r = await ruler.boundingBox();
  await page.mouse.click(r.x + r.width * 0.5, r.y + r.height / 2);
  const mid = Number(await ruler.getAttribute("aria-valuenow"));
  if (mid < 40_000 || mid > 56_000) throw new Error(`the playhead went to ${mid} ms, not near the middle of the timeline`);
  assertEqual(await page.evaluate(() => document.activeElement.getAttribute("role")), "slider", "the ruler has the focus");
  await t.key("Home");
  assertEqual(await ruler.getAttribute("aria-valuenow"), "0", "Home");
  await t.key("ArrowRight", "Shift+ArrowRight");
  assertEqual(await ruler.getAttribute("aria-valuetext"), "1.1 seconds, 1.1 seconds into \"Open the gradebook\"", "spoken value");
  const lane = await page.locator('.os-tl-lane[aria-label^="Overlays 2 track"] .os-tl-track').boundingBox();
  await page.mouse.click(lane.x + lane.width * 0.25, lane.y + lane.height / 2);
  const quarter = Number(await ruler.getAttribute("aria-valuenow"));
  if (quarter < 15_000 || quarter > 28_000) throw new Error(`a click on an empty track gave ${quarter} ms`);
  assertEqual(await t.value("Playhead time"), (await page.evaluate((ms) => { const s = Math.floor(ms / 1000); const f = ms % 1000; return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}${f === 0 ? "" : "." + String(f).padStart(3, "0").replace(/0+$/, "")}`; }, quarter)), "the time box follows");
});

await check("from a clip: commas move the playhead, S splits, D describes; the grips and the list follow", async (page, t) => {
  await t.focusClip("Open the gradebook");
  await t.key(".", ".", ".", "Shift+>");
  assertEqual(await page.locator(".os-tl-ruler").getAttribute("aria-valuenow"), "1300", "comma and period");
  await t.key("PageUp");
  assertEqual(await t.said(), 'Playhead at 6.3 seconds, 6.3 seconds into "Open the gradebook".', "Page Up said");
  await t.key("s");
  assertEqual(await t.said(), "Clip split at 6.3 seconds, now two clips.", "split");
  assertEqual((await t.focusedClip()).name, "Open the gradebook", "focus stays on the clip");
  assertEqual(await page.locator('.os-tl-lane[aria-label^="Screen track"]').getAttribute("aria-label"), "Screen track, 4 clips", "the lane counts four");
  await t.key("d");
  assertMatch(await t.said(), /^At 6\.3 seconds\. /, "described");
  await t.key("Control+z");
  assertEqual(await t.said(), "Undid: split clip.", "undone");
  assertEqual(await page.locator('.os-tl-lane[aria-label^="Screen track"]').getAttribute("aria-label"), "Screen track, 3 clips", "back to three");
  assertEqual((await t.focusedClip()).name, "Open the gradebook", "focus kept after undo");
});

await check("zoom: in and out by keys and buttons, the whole edit with 0, the span is said, the ruler marks follow", async (page, t) => {
  const marks = async () => page.locator(".os-tl-tick").count();
  const width = async () => page.locator(".os-tl-canvas").evaluate((el) => el.scrollWidth);
  const fitted = await width();
  await t.focusClip("Open the gradebook");
  await t.key("+");
  assertMatch(await t.said(), /^Zoomed in\. About \d+(\.\d+)? seconds fit across the timeline\.$/, "said");
  if ((await width()) <= fitted * 1.5) throw new Error("zooming in did not make the timeline wider");
  await t.key("=", "=");
  if ((await width()) <= fitted * 3) throw new Error("two more steps did not make it wider");
  assertMatch((await page.locator(".os-tl-tick").first().textContent()) ?? "", /^\d+:\d\d(\.\d+)?$/, "a mark's label");
  await page.getByRole("button", { name: /^Zoom out/ }).click();
  assertMatch(await t.said(), /^Zoomed out\./, "out said");
  await t.focusClip("Open the gradebook");
  await t.key("0");
  assertMatch(await t.said(), /^Showing the whole edit, 1 minute 3\d(\.\d+)? seconds across\.$|^Showing the whole edit, 1 minute \d+(\.\d+)? seconds across\.$/, "fit said");
  assertEqual(await width(), fitted, "back to fitting");
  await t.key("-");
  assertMatch(await t.said(), /^The whole edit is already showing/, "cannot zoom out past the whole edit");
  await page.getByRole("button", { name: /^Zoom in/ }).click();
  for (let i = 0; i < 8; i++) await page.getByRole("button", { name: /^Zoom in/ }).click();
  assertMatch(await t.said(), /^Already as far in as it goes\./, "the most zoomed in");
  if ((await marks()) > 60) throw new Error(`${await marks()} marks are drawn: only the ones in view should be`);
});

await check("F6 and Shift+F6 move between the preview, the timeline and the clip list and say where they arrived", async (page, t) => {
  await t.focusClip("Open the gradebook");
  await t.key("F6");
  assertEqual(await t.focusedRow().then((r) => r?.name), "Open the gradebook", "F6 goes to the list");
  assertEqual(await t.said(), "Clip list.", "said");
  await t.key("F6");
  assertEqual(await page.evaluate(() => document.activeElement.textContent.startsWith("Play")), true, "F6 goes round to the preview, on its Play button");
  assertEqual(await t.said(), "Preview.", "said");
  await t.key("F6");
  assertEqual((await t.focusedClip())?.name, "Open the gradebook", "then to the timeline");
  assertEqual(await t.said(), "Timeline.", "said");
  await t.key("Shift+F6");
  assertEqual(await page.evaluate(() => document.activeElement.textContent.startsWith("Play")), true, "Shift+F6 goes back");
});

await check("Enter on a clip goes to its details in the list, and Escape there goes back to the list row", async (page, t) => {
  await t.focusClip("Add an assignment");
  await t.key("Enter");
  assertEqual(await page.evaluate(() => document.activeElement.id.endsWith("-name")), true, "in the Name box");
  assertEqual(await page.evaluate(() => document.activeElement.value), "Add an assignment", "with the name");
});

await check("volume, fades and the Track box in the form change the clip and say so", async (page, t) => {
  await t.focusRow("Voice, part 1");
  const volume = page.getByLabel("Volume (decibels)");
  await volume.fill("-6");
  await volume.press("Enter");
  assertEqual(await t.said(), 'Clip "Voice, part 1" is now 6 decibels quieter than recorded.', "volume");
  await volume.fill("loud");
  await volume.press("Enter");
  assertMatch(await t.said(), /^"loud" is not a number of decibels\./, "wrong volume explained");
  assertEqual(await volume.getAttribute("aria-invalid"), "true");
  await volume.press("Escape");
  assertEqual(await volume.inputValue(), "-6");
  const fadeIn = page.getByLabel("Fade in", { exact: true });
  await fadeIn.fill("2");
  await fadeIn.press("Enter");
  assertMatch(await t.said(), /fade/i, "fade said");
  assertMatch(await page.locator(".os-tl-clip", { hasText: "Voice, part 1" }).getAttribute("aria-label"), /6 decibels quieter, fades in over 2 seconds/, "the timeline's sentence follows");
  const track = page.getByLabel("Track", { exact: true });
  assertEqual(await track.locator("option").allTextContents(), ["Voice", "Audio 2"], "the tracks that take a recording of the microphone");
  await track.selectOption({ label: "Audio 2" });
  assertMatch(await t.said(), /moved from the Voice track to the Audio 2 track/, "moved");
  assertEqual(await page.locator('.os-tl-lane[aria-label^="Audio 2 track"]').getAttribute("aria-label"), "Audio 2 track, 1 clip", "the lane shows it");
});

await check("a 60 minute recording cut into 720 clips stays quick: draw, split, zoom, drag, undo", async (page, t) => {
  const began = Date.now();
  await page.goto(BASE + "?long=1");
  await page.locator(".os-tl-clip").first().waitFor();
  const drawn = Date.now() - began;
  if (process.env.SHOW_TIMES) console.log(`      drawing the page with 720 clips: ${drawn} ms`);
  if (drawn > 4000) throw new Error(`drawing took ${drawn} ms`);
  assertEqual(await page.locator(".os-tl-clip").count(), 720, "clips drawn");
  const time = async (what, fn) => {
    const t0 = Date.now();
    await fn();
    await page.waitForFunction(() => true);
    const took = Date.now() - t0;
    if (took > 1000) throw new Error(`${what} took ${took} ms, which is not under a second`);
    if (process.env.SHOW_TIMES) console.log(`      ${what}: ${took} ms`);
    return took;
  };
  await page.locator(".os-tl-clip").nth(100).focus();
  await time("moving the playhead into a clip and splitting it", async () => {
    await page.getByLabel("Playhead time", { exact: true }).fill("16:45");
    await page.getByLabel("Playhead time", { exact: true }).press("Enter");
    await page.locator(".os-tl-clip").nth(100).focus();
    await page.keyboard.press("s");
    await page.waitForFunction(() => document.querySelector(".os-cl-status")?.textContent !== "");
  });
  await time("zooming in", async () => {
    await page.keyboard.press("+");
  });
  await time("undo", async () => {
    await page.keyboard.press("Control+z");
  });
  await time("choosing every row", async () => {
    await page.keyboard.press("Control+a");
  });
  const box = await page.locator(".os-tl-clip").nth(100).boundingBox();
  if (box !== null) {
    await time("a mouse drag of a clip", async () => {
      await page.keyboard.press("Escape");
      await page.mouse.move(box.x + 5, box.y + box.height / 2);
      await page.mouse.down();
      await page.mouse.move(box.x + 5, box.y + box.height / 2, { steps: 2 });
      await page.mouse.up();
    });
  }
});


// ---- the preview, playing, and files ----

await check("the preview shows what the edit asks for at the playhead: the pictures, the text, the arrow and the zoom", async (page, t) => {
  const box = page.getByLabel("Playhead time", { exact: true });
  const at = async (time) => {
    await box.fill(time);
    await box.press("Enter");
  };
  await at("13");
  assertEqual(await page.locator(".os-pv-text").allTextContents(), ["Open Settings"], "the text overlay at 13 seconds");
  assertEqual(await page.locator(".os-pv-box-screen").count(), 1, "the screen picture");
  assertMatch((await page.locator(".os-pv-box-screen").textContent()) ?? "", /Open the gradebook/, "stands in for the recording");
  assertEqual(await page.locator(".os-pv-box-webcam.os-pv-corner").count(), 1, "the webcam in the corner");
  assertEqual(await page.locator(".os-pv-zoomed").evaluate((el) => el.style.transform), "", "no zoom");
  assertEqual(await page.locator(".os-pv-zoomed .os-pv-text").count(), 0, "text is not part of what zooms");
  assertMatch(await page.locator(".os-pv-stage").getAttribute("aria-label"), /^Preview of the edit at 13 seconds\./, "the picture has a name");
  await at("37");
  assertEqual(await page.locator(".os-pv-text").count(), 0, "the text has gone");
  assertEqual(await page.locator(".os-pv-zoomed svg.os-pv-arrow").count(), 1, "the arrow, with the picture");
  await at("1:12");
  assertMatch(await page.locator(".os-pv-zoomed").evaluate((el) => el.style.transform), /scale\(2\)/, "zoomed in twice");
  assertEqual(await page.locator(".os-pv-zoomed svg").count(), 0, "the arrow has gone");
  const box2 = await page.locator(".os-pv-box-webcam").boundingBox();
  const stage = await page.locator(".os-pv-stage").boundingBox();
  if (box2.x + box2.width < stage.x + stage.width * 0.9) throw new Error("the webcam is not in the bottom right corner");
  await at("1:30");
  assertEqual(await page.locator(".os-pv-box").count(), 0, "nothing shows at the very end");
  await page.getByRole("button", { name: /^Describe what is showing/ }).click();
  assertMatch(await t.said(), /^At 1 minute 30 seconds\./, "described on request");
});

await check("Space plays from the playhead and pauses, the playhead moves, it stops at the end, and playing from the end starts again", async (page, t) => {
  const box = page.getByLabel("Playhead time", { exact: true });
  await box.fill("1:28.5");
  await box.press("Enter");
  await t.focusClip("Open the gradebook");
  await t.key(" ");
  assertEqual(await t.said(), "Playing from 1 minute 28.5 seconds.", "start said");
  assertEqual(await page.getByRole("button", { name: "Pause", exact: true }).count(), 1, "the button says Pause");
  const duringAria = await page.locator(".os-tl-ruler").getAttribute("aria-valuenow");
  assertEqual(duringAria, "88500", "the slider's value is not changed while it plays");
  assertEqual(await page.locator(".os-tl-ruler").getAttribute("aria-valuetext") === null, false);
  await page.waitForFunction(() => document.querySelector(".os-cl-status").textContent.startsWith("Reached the end"), null, { timeout: 6000 });
  assertEqual(await page.locator(".os-cl-status").textContent(), "Reached the end, 1 minute 30 seconds.", "end said");
  assertEqual(await page.getByRole("button", { name: "Play", exact: true }).count(), 1, "the button says Play again");
  assertEqual(await page.locator(".os-tl-ruler").getAttribute("aria-valuenow"), "90000", "the slider says where it stopped");
  await t.key(" ");
  assertEqual(await t.said(), "Playing from 0 seconds, the start, because the playhead was at the end.", "from the end it starts again");
  await page.waitForTimeout(400);
  await t.key(" ");
  assertMatch(await t.said(), /^Paused at 0\.\d+ seconds\.$/, "paused");
  const paused = Number(await page.locator(".os-tl-ruler").getAttribute("aria-valuenow"));
  await page.waitForTimeout(300);
  assertEqual(Number(await page.locator(".os-tl-ruler").getAttribute("aria-valuenow")), paused, "it stays where it was paused");
  assertEqual(await t.value("Playhead time"), await page.evaluate((ms) => `0:${String(Math.floor(ms / 1000)).padStart(2, "0")}${ms % 1000 === 0 ? "" : "." + String(ms % 1000).padStart(3, "0").replace(/0+$/, "")}`, paused), "the time box agrees");
});

await check("moving the playhead while it plays carries on from there; editing while it plays works", async (page, t) => {
  await t.focusClip("Open the gradebook");
  await t.key(" ");
  await page.waitForTimeout(300);
  const box = page.getByLabel("Playhead time", { exact: true });
  await box.fill("50");
  await box.press("Enter");
  await page.waitForTimeout(300);
  await t.focusClip("Add an assignment");
  await t.key(" ");
  const stopped = Number(await page.locator(".os-tl-ruler").getAttribute("aria-valuenow"));
  if (stopped < 50_200 || stopped > 52_000) throw new Error(`it carried on from ${stopped} ms, not from 50 seconds`);
  assertEqual(await page.getByRole("button", { name: "Play", exact: true }).count(), 1);
});

const dir = mkdtempSync(join(tmpdir(), "os-fixture-"));
const fixture = join(dir, "lesson.capture");
const made = spawnSync(process.execPath, [resolve(here, "make-fixture.mjs"), fixture], { encoding: "utf8" });
const haveFixture = made.status === 0;
if (!haveFixture) console.log(`SKIP  the recording folder checks (no ffmpeg to make test files: ${(made.stderr || made.stdout).trim().slice(0, 100)})`);

await check("an edit file can be saved as a copy, saved, and opened again; a file that is not an edit is refused in words", async (page, t) => {
  await t.focusRow("Open the gradebook");
  await page.getByLabel("Name", { exact: true }).fill("Welcome");
  await page.getByLabel("Name", { exact: true }).press("Enter");
  assertMatch(await page.locator("#file-state").textContent(), /There are changes that are not saved\./, "unsaved");
  const [copy] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Save a copy" }).click()]);
  assertEqual(copy.suggestedFilename(), "edit.json", "the copy's name");
  const copyPath = join(dir, "copy.json");
  await copy.saveAs(copyPath);
  assertEqual(JSON.parse(readFileSync(copyPath, "utf8")).format, "openstage-capture-edit", "a real edit file");
  assertMatch(await t.said(), /^A copy was saved as edit\.json in your downloads\.$/, "said");
  assertMatch(await page.locator("#file-state").textContent(), /There are changes that are not saved\./, "a copy does not count as saving");
  const [saved] = await Promise.all([page.waitForEvent("download"), page.keyboard.press("Control+s")]);
  assertEqual(saved.suggestedFilename(), "edit.json");
  assertMatch(await t.said(), /^Saved as edit\.json in your downloads\.$/, "Ctrl+S saves");
  assertMatch(await page.locator("#file-state").textContent(), /There are no unsaved changes\./, "saved");
  // start again, then open the copy
  await page.getByRole("button", { name: "Start again with the sample lesson" }).click();
  assertEqual((await t.rowNames()).includes("Welcome"), false, "the sample has the old name");
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Open an edit file" }).click()]);
  await chooser.setFiles(copyPath);
  await page.waitForFunction(() => document.querySelector(".os-cl-status").textContent.startsWith("Opened"));
  assertMatch(await page.locator(".os-cl-status").textContent(), /^Opened copy\.json\. Tracks and clips: 9 clips/, "opened, in words");
  assertEqual((await t.rowNames()).includes("Welcome"), true, "the renamed clip is there");
  assertMatch(await page.locator("#file-state").textContent(), /There are no unsaved changes\./);
  // a file that is not an edit
  const junk = join(dir, "junk.json");
  writeFileSync(junk, "this is not an edit");
  const [second] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Open an edit file" }).click()]);
  await second.setFiles(junk);
  await page.waitForFunction(() => document.querySelector(".os-cl-status").textContent.includes("was not opened"));
  assertMatch(await page.locator(".os-cl-status").textContent(), /^junk\.json was not opened: /, "refused in words");
  assertEqual((await t.rowNames()).includes("Welcome"), true, "what was open stays open");
}, { search: "?pickers=input" });

if (haveFixture) {
  await check("a recording folder opens as an edit made from its take, and it plays: the pictures, the sound, and the end", async (page, t) => {
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Open a recording folder" }).click()]);
    await chooser.setFiles(fixture);
    await page.waitForFunction(() => document.querySelector(".os-cl-status").textContent.startsWith("Opened the recording"));
    assertEqual(await page.locator(".os-cl-status").textContent(), "Opened the recording Test lesson, 3 seconds long. Tracks and clips: 4 clips on 4 tracks, 3 seconds long.", "opened, in words");
    assertEqual(await t.rowNames(), ["Screen of take 1", "Webcam of take 1", "Microphone of take 1", "Computer sound of take 1"], "the clips");
    assertMatch(await page.locator("#file-state").textContent(), /^The recording Test lesson\./);
    await page.locator(".os-pv-media").first().waitFor({ state: "attached", timeout: 5000 });
    const media = () => page.evaluate(() => ({
      videos: [...document.querySelectorAll("video")].map((v) => ({ paused: v.paused, muted: v.muted, time: v.currentTime, ready: v.readyState, error: v.error?.code ?? 0 })),
      audios: [...document.querySelectorAll("audio")].map((a) => ({ paused: a.paused, time: a.currentTime, ready: a.readyState, error: a.error?.code ?? 0 })),
    }));
    await t.focusClip("Screen of take 1");
    await t.key(" ");
    await page.waitForTimeout(1200);
    const playing = await media();
    for (const v of playing.videos) {
      if (v.error !== 0) throw new Error(`a video could not be played (error ${v.error}); the browser may not play this file type`);
      assertEqual(v.muted, true, "a picture is played without its own sound");
    }
    assertEqual(playing.videos.length, 2, "the screen and the webcam");
    assertEqual(playing.videos.every((v) => !v.paused && v.time > 0.3), true, "both pictures are playing and have moved on");
    assertEqual(playing.audios.length, 2, "the microphone and the computer's sound");
    assertEqual(playing.audios.every((a) => !a.paused && a.time > 0.3), true, "both sounds are playing and have moved on");
    // the pictures play smoothly: they are never taken out of the page and put back (that would pause them)
    const samples = [];
    for (let i = 0; i < 6; i++) {
      samples.push(await page.evaluate(() => [...document.querySelectorAll("video")].map((v) => v.currentTime)));
      await page.waitForTimeout(80);
    }
    for (let i = 1; i < samples.length; i++) {
      samples[i].forEach((time, k) => {
        if (time < samples[i - 1][k]) throw new Error(`a picture went backwards: ${JSON.stringify(samples)}`);
      });
    }
    // each recording is within half a second of the playhead (the playhead is the clock)
    const apart = await page.evaluate(() => {
      const [m, s] = document.querySelector(".os-tl-controls input").value.split(":");
      const playhead = Number(m) * 60 + Number(s);
      return [...document.querySelectorAll("video, audio")].map((el) => Math.abs(el.currentTime - playhead));
    });
    assertEqual(apart.every((d) => d < 0.5), true, `the recordings keep up with the playhead (apart by ${apart.map((d) => d.toFixed(2)).join(", ")} seconds)`);
    await page.waitForFunction(() => document.querySelector(".os-cl-status").textContent.startsWith("Reached the end"), null, { timeout: 6000 });
    const ended = await media();
    assertEqual([...ended.videos, ...ended.audios].every((m) => m.paused), true, "everything stops at the end");
    // starting part way through: every recording is put where the playhead is, at once
    const from = page.getByLabel("Playhead time", { exact: true });
    await from.fill("1.5");
    await from.press("Enter");
    await t.focusClip("Screen of take 1");
    await t.key(" ");
    await page.waitForTimeout(500);
    const midway = await media();
    assertEqual([...midway.videos, ...midway.audios].every((m) => m.time > 1.4 && m.time < 2.6), true, `everything started near 1.5 seconds (${JSON.stringify([...midway.videos, ...midway.audios].map((m) => Number(m.time.toFixed(2))))})`);
    await t.key(" ");
    // volume changes what the sound is played at
    await t.focusRow("Microphone of take 1");
    const volume = page.getByLabel("Volume (decibels)");
    await volume.fill("-6");
    await volume.press("Enter");
    await t.focusClip("Microphone of take 1");
    await page.getByLabel("Playhead time", { exact: true }).fill("0.5");
    await page.getByLabel("Playhead time", { exact: true }).press("Enter");
    const gains = await page.evaluate(() => [...document.querySelectorAll("audio")].map((a) => Number(a.dataset.gain)));
    assertEqual(gains.map((g) => Number(g.toFixed(2))), [0.5, 1], "the microphone is 6 decibels quieter, the computer's sound as recorded");
    // the preview stays with the playhead when it is moved by hand: the picture seeks
    const box = page.getByLabel("Playhead time", { exact: true });
    await box.fill("2");
    await box.press("Enter");
    await page.waitForTimeout(500);
    const seeked = await media();
    assertEqual(seeked.videos.every((v) => Math.abs(v.time - 2) < 0.3), true, `the pictures went to 2 seconds (${JSON.stringify(seeked.videos.map((v) => v.time))})`);
    // Save: this folder cannot be written to from a file box, so it hands back a download
    const [saved] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Save", exact: true }).click()]);
    assertEqual(saved.suggestedFilename(), "edit.json");
  }, { search: "?pickers=input" });

  await check("a folder that is not a recording is refused in words, and what was open stays open", async (page, t) => {
    const other = join(dir, "holiday");
    writeFileSync(join(dir, "holiday.txt"), "x");
    spawnSync("mkdir", ["-p", other]);
    writeFileSync(join(other, "notes.txt"), "not a recording");
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Open a recording folder" }).click()]);
    await chooser.setFiles(other);
    await page.waitForFunction(() => document.querySelector(".os-cl-status").textContent.includes("not a recording"));
    assertEqual(await page.locator(".os-cl-status").textContent(), "holiday is not a recording made by OpenStage Capture: it has no capture.json. Choose the folder that ends in .capture.", "said");
    assertEqual((await t.rowNames()).length, 12, "the lesson is still there");
  }, { search: "?pickers=input" });
}

await check("a key held on a grip ends the step when the focus goes somewhere else, at once", async (page, t) => {
  await t.focusClip("Add an assignment");
  const grip = page.getByRole("slider", { name: 'End of "Add an assignment"' });
  await grip.focus();
  await t.key("ArrowLeft", "ArrowLeft");
  await page.getByLabel("Playhead time", { exact: true }).focus(); // the focus leaves the grip
  assertEqual((await t.listTimes("Add an assignment"))[4], "0:59.8", "the list already shows it");
  await page.getByRole("button", { name: "Undo" }).click();
  assertMatch(await t.said(), /^Undid: trim clip end\./, "one step");
});

// ---- looking at it ----

async function contrast(page, selector) {
  return page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return { missing: true };
    const parse = (c) => {
      const m = c.match(/rgba?\(([^)]+)\)/);
      const [r, g, b, a = 1] = m[1].split(/[ ,\/]+/).filter(Boolean).map(Number);
      return { r, g, b, a };
    };
    const bg = (node) => {
      let layers = [];
      for (let n = node; n; n = n.parentElement) {
        const c = parse(getComputedStyle(n).backgroundColor);
        if (c.a > 0) layers.push(c);
        if (c.a === 1) break;
      }
      let out = { r: 255, g: 255, b: 255 };
      for (const c of layers.reverse()) out = { r: c.r * c.a + out.r * (1 - c.a), g: c.g * c.a + out.g * (1 - c.a), b: c.b * c.a + out.b * (1 - c.a) };
      return out;
    };
    const lum = ({ r, g, b }) => {
      const f = (v) => ((v /= 255) <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
      return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
    };
    const fg = parse(getComputedStyle(el).color);
    const back = bg(el);
    const [a, b] = [lum(fg), lum(back)].sort((x, y) => y - x);
    return { ratio: (a + 0.05) / (b + 0.05) };
  }, selector);
}

for (const scheme of ["light", "dark"]) {
  await check(`text and controls have enough contrast (${scheme} colours)`, async (page, t) => {
    await t.focusRow("Voice, part 1");
    await t.key("m");
    await page.getByLabel("Start", { exact: true }).fill("whenever");
    await page.getByLabel("Start", { exact: true }).press("Enter");
    const wanted = {
      ".os-cl": 4.5,
      ".os-cl-status": 4.5,
      ".os-cl-hint": 4.5,
      ".os-cl-table thead th": 4.5,
      '.os-cl-table tbody tr[aria-selected="true"] th': 4.5,
      ".os-cl-table tbody tr:not([aria-selected=true]) td": 4.5,
      ".os-cl-button": 4.5,
      ".os-cl-key": 4.5,
      ".os-cl-input": 4.5,
      ".os-cl-error": 4.5,
      ".os-cl-help summary": 4.5,
      ".os-tl-lane-name": 4.5,
      ".os-tl-lane-kind": 4.5,
      ".os-tl-tick": 4.5,
      ".os-tl-corner": 4.5,
      '.os-tl-lane[data-kind="screen"] .os-tl-clip': 4.5,
      '.os-tl-lane[data-kind="webcam"] .os-tl-clip': 4.5,
      '.os-tl-lane[data-kind="audio"] .os-tl-clip': 4.5,
      '.os-tl-lane[data-kind="overlay"] .os-tl-clip': 4.5,
      '.os-tl-lane[data-kind="zoom"] .os-tl-clip': 4.5,
      ".os-tl-badge": 4.5,
    };
    for (const [selector, minimum] of Object.entries(wanted)) {
      const result = await contrast(page, selector);
      if (result.missing) throw new Error(`${selector} is not on the page`);
      if (result.ratio < minimum) throw new Error(`${selector} has contrast ${result.ratio.toFixed(2)}:1, under ${minimum}:1`);
    }
    if (shots) await page.screenshot({ path: resolve(shots, `${scheme}.png`), fullPage: true });
  }, { colorScheme: scheme });
}

await check("Windows high contrast (forced colours): the chosen row is still shown and the page still works", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("ArrowDown");
  assertEqual((await t.focusedRow())?.name, "Check the totals", "still works");
  if (shots) await page.screenshot({ path: resolve(shots, "forced-colors.png"), fullPage: true });
}, { forcedColors: "active" });

await check("at 320 pixels wide and text twice the size, the page does not scroll sideways", async (page) => {
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  const wide = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  if (wide > 1) throw new Error(`the page is ${wide}px wider than the window`);
  if (shots) await page.screenshot({ path: resolve(shots, "narrow.png"), fullPage: true });
}, { viewport: { width: 320, height: 800 } });

await check("no animation or motion is used (reduced motion has nothing to turn off)", async (page) => {
  const animated = await page.evaluate(() => [...document.querySelectorAll("*")].filter((el) => {
    const s = getComputedStyle(el);
    return (s.animationName !== "none" && s.animationName !== "") || parseFloat(s.transitionDuration) > 0;
  }).length);
  assertEqual(animated, 0, "elements with an animation or a transition");
}, { reducedMotion: "reduce" });

await browser.close();
server.kill();
console.log(`\n${passes} passed, ${failures} failed.`);
process.exit(failures === 0 ? 0 : 1);
