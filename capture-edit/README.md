# OpenStage Capture: the edit model

The editing logic of OpenStage Capture, without a screen: tracks and clips laid out in time, and the
actions that change them (add, split, trim, move, delete, join, reorder, volume and fades), undo and
redo, the clip list, and a description of what is on screen at any moment. Every action says what it
did in plain words, because the editor must work without a mouse and with a screen reader.

Licence: Apache-2.0. TypeScript, no runtime dependencies.

The file format and the choices made are in [`../docs/capture-edit-format.md`](../docs/capture-edit-format.md).
The same file is read and checked by the Rust renderer side
(`../capture/crates/openstage-capture-project/src/edit.rs`); the examples in `fixtures/` are read by both and
must give the same findings.

## What is in it

| File | What it holds |
|---|---|
| `src/model.ts` | The data: tracks, clips, sources, and small helpers |
| `src/time.ts` | Times in words ("1 minute 12.4 seconds"), typed times, the keyboard steps |
| `src/ops.ts` | The editing actions on clips |
| `src/items.ts` | The actions on overlays and zooms: text, shapes, arrows, blur, redaction, zoom |
| `src/contrast.ts`, `src/regions.ts` | Colours and contrast; rectangles on the picture and saying where they are in words |
| `src/history.ts` | Undo and redo, with a drag as one step |
| `src/validate.ts` | The checks, with codes shared with the Rust reader |
| `src/serialize.ts` | Saving and reading `edit.json` |
| `src/preview.ts` | The clip list, a sentence for a clip, the described preview |
| `test/` | The tests, including 150 random editing sessions that must stay sound and undo exactly |
| `fixtures/` | Shared examples and `expected.json`, read by the TypeScript and the Rust tests |

## Running the tests

Needs Node 22.6 or later (it runs TypeScript directly, so there is no build step) and, for the type
check, the `typescript` and `@types/node` packages.

```
node --test "test/*.test.ts"      # the tests
tsc -p .                          # the type check
```

Nothing here needs Windows. A person who does not use a terminal does not need to run it: it is checked
before each delivery, and its results are reported in `docs/roadmap.md`.
