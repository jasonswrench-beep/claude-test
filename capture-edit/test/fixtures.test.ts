// SPDX-License-Identifier: Apache-2.0
// The shared examples: every reader of an edit file (this one, and the Rust one that the renderer
// uses) must find the same thing in each, so the two can never quietly disagree about what is valid.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { parse } from "../src/index.ts";

const dir = new URL("../fixtures/", import.meta.url);
const expected = JSON.parse(readFileSync(new URL("expected.json", dir), "utf8")) as {
  fixtures: Record<string, { readable: boolean; problems: string[] }>;
};

test("every fixture is described in expected.json and the other way round", () => {
  const files = readdirSync(dir).filter((f) => f.endsWith(".json") && f !== "expected.json").sort();
  assert.deepEqual(files, Object.keys(expected.fixtures).sort());
});

for (const [name, want] of Object.entries(expected.fixtures)) {
  test(`fixture ${name}`, () => {
    const result = parse(readFileSync(new URL(name, dir), "utf8"));
    assert.equal(result.ok, want.readable, result.ok ? "read, but should not have" : result.message);
    if (result.ok) {
      const found = result.problems.map((p) => `${p.code}:${p.severity}`).sort();
      assert.deepEqual(found, [...want.problems].sort());
    }
  });
}
