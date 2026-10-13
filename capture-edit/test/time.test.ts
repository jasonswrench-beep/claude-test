// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { describeTime, formatTimecode, parseTime, stepTime, wholeMs } from "../src/time.ts";

test("a time is said in words, with the units in the singular when there is one", () => {
  assert.equal(describeTime(0), "0 seconds");
  assert.equal(describeTime(1000), "1 second");
  assert.equal(describeTime(2000), "2 seconds");
  assert.equal(describeTime(500), "0.5 seconds");
  assert.equal(describeTime(12_400), "12.4 seconds");
  assert.equal(describeTime(60_000), "1 minute");
  assert.equal(describeTime(61_000), "1 minute 1 second");
  assert.equal(describeTime(72_400), "1 minute 12.4 seconds");
  assert.equal(describeTime(120_000), "2 minutes");
  assert.equal(describeTime(3_725_000), "1 hour 2 minutes 5 seconds");
  assert.equal(describeTime(3_600_000), "1 hour");
  assert.equal(describeTime(72_435), "1 minute 12.435 seconds");
});

test("a time that is not whole or not positive is made whole", () => {
  assert.equal(wholeMs(12.6), 13);
  assert.equal(wholeMs(-5), 0);
  assert.equal(describeTime(-100), "0 seconds");
  assert.equal(describeTime(1000.4), "1 second");
});

test("a timecode for a field reads back as the same time", () => {
  for (const ms of [0, 500, 12_400, 61_000, 72_400, 599_999, 3_725_000, 3_725_050]) {
    const text = formatTimecode(ms);
    const back = parseTime(text);
    assert.deepEqual(back, { ok: true, ms }, `${ms} -> ${text}`);
  }
  assert.equal(formatTimecode(72_400), "1:12.4");
  assert.equal(formatTimecode(500), "0:00.5");
  assert.equal(formatTimecode(3_725_000), "1:02:05");
});

test("what an author types is understood in the usual ways", () => {
  const ms = (text: string) => {
    const r = parseTime(text);
    assert.ok(r.ok, `${text}: ${r.ok ? "" : r.message}`);
    return r.ms;
  };
  assert.equal(ms("72.4"), 72_400);
  assert.equal(ms("90"), 90_000);
  assert.equal(ms(" 1:12.4 "), 72_400);
  assert.equal(ms("1:02:05"), 3_725_000);
  assert.equal(ms("1m12.4s"), 72_400);
  assert.equal(ms("2m"), 120_000);
  assert.equal(ms("90s"), 90_000);
  assert.equal(ms("1h2m5s"), 3_725_000);
  assert.equal(ms("0"), 0);
});

test("what cannot be understood is explained with examples", () => {
  for (const text of ["", "   ", "abc", "1:75", "1:2:3:4", "-5", "1.2.3", "12 seconds"]) {
    const r = parseTime(text);
    assert.equal(r.ok, false, text);
    if (!r.ok) assert.match(r.message, /72\.4|1:12\.4/, text);
  }
});

test("the keys move by the same steps as OpenStage's Timeline", () => {
  const at = (key: Parameters<typeof stepTime>[0], shift = false) => stepTime(key, 10_000, 0, 60_000, { shift });
  assert.equal(at("Right"), 10_100);
  assert.equal(at("Left"), 9_900);
  assert.equal(at("Right", true), 11_000);
  assert.equal(at("Left", true), 9_000);
  assert.equal(at("PageUp"), 15_000);
  assert.equal(at("PageDown"), 5_000);
  assert.equal(at("Home"), 0);
  assert.equal(at("End"), 60_000);
});

test("a step never goes past either end", () => {
  assert.equal(stepTime("Left", 50, 0, 1000), 0);
  assert.equal(stepTime("PageDown", 3000, 0, 60_000), 0);
  assert.equal(stepTime("Right", 950, 0, 1000), 1000);
  assert.equal(stepTime("PageUp", 58_000, 0, 60_000), 60_000);
  assert.equal(stepTime("Right", 5000, 5000, 5000), 5000);
  assert.equal(stepTime("Home", 9000, 2000, 8000), 2000);
});
