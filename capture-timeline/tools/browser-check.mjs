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

import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
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
  const context = await browser.newContext({ viewport: { width: 1100, height: 1000 }, ...options });
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
  assertMatch(snapshot, /grid "Tracks and clips: 9 clips, 3 overlays and zooms on 5 tracks, 1 minute 30 seconds long\."/, "the grid");
  for (const header of ["Name", "Track", "Type", "Start", "Length", "End", "State"]) assertMatch(snapshot, new RegExp(`columnheader "${header}"`), `header ${header}`);
  assertMatch(snapshot, /rowheader "Open the gradebook"/, "the row header");
  assertEqual(await page.locator('.os-cl-table tbody tr[aria-selected="true"]').count(), 1, "selected rows");
  assertEqual(await page.locator('.os-cl-table tbody tr[tabindex="0"]').count(), 1, "rows in the Tab order");
  assertMatch(snapshot, /slider "Playhead"/, "the playhead slider");
  assertMatch(snapshot, /region "Clips"/, "the region");
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
    const where = await page.evaluate(() => (document.activeElement === document.body ? "body" : `${document.activeElement.tagName}#${document.activeElement.id}.${document.activeElement.dataset.id ?? ""}.${document.activeElement.textContent.trim().slice(0, 20)}`));
    if (where === "body" || seen.has(where)) {
      left = true;
      break;
    }
    seen.add(where);
  }
  if (!left) throw new Error(`still tabbing after 80 presses; ${seen.size} different places seen`);
  if (seen.size < 25) throw new Error(`only ${seen.size} places can be reached by Tab`);
  // and one row is a single stop, not twelve
  const rowStops = [...seen].filter((s) => s.startsWith("TR#")).length;
  assertEqual(rowStops, 1, "table rows reached by Tab");
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
  assertEqual((await t.focusedRow())?.name, "Open the gradebook", "the second Escape returns to the list");
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
  assertMatch(await page.locator(".os-cl-error").textContent(), /would run into/, "error shown");
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
  assertEqual(await page.locator(".os-cl-empty").isVisible(), true, "empty message shown");
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
  assertMatch(await page.locator(".os-cl-readout").textContent(), /Overlays showing: text "Open Settings"\./, "kept as text");
});

await check("Start again gives back the sample, with focus on the list", async (page, t) => {
  await t.focusRow("Add an assignment");
  await t.key("Delete");
  await page.getByRole("button", { name: "Start again with the sample lesson" }).click();
  assertEqual((await t.rowNames()).length, 12, "rows");
  assertEqual((await t.focusedRow())?.name, "Open the gradebook", "focus on the first row");
  assertEqual(await page.locator(".os-cl").count(), 1, "one panel, not two");
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
