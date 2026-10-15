# The editing screens (clip list)

The first screen of the OpenStage Capture editor: the **clip list** (stories C1 and C2). It is a table of everything in an edit, with the keyboard commands to split, trim, delete, join, move, rename and set exact times, and it says aloud what each command did. The visual multi-track timeline (story C3) is built on top of this, on the same commands, and comes next.

It sits on the edit model in [`../capture-edit/`](../capture-edit/) and does not duplicate any of its rules: what a command does, and the sentence that says so, come from there.

**Nothing in this package has been heard with a screen reader.** It has been tested by scripts, and checked in a real browser by a script that reads the same information a screen reader is given (roles, names, states, focus). How NVDA, Narrator, JAWS or VoiceOver actually speak it is not known until someone listens. The checklist for that is below.

## Try it (Windows)

Double-click **`capture\OPEN TIMELINE DEMO.bat`**. It opens a page in your browser with a made-up lesson (a screen recording cut into three clips, the voice and webcam cut the same way, two overlays and a zoom). It records nothing, saves nothing and changes nothing on your computer; the page is served only to your own computer and stops when you close the window. It needs Node.js 22.13 or newer.

Things to try, with the keyboard only:

1. Press **Tab** until the list takes focus, then **Down** and **Up** to choose a row. Each row is read as one sentence.
2. Press **Right** a few times. The playhead moves a tenth of a second at a time (**Shift+Right** a second, **Page Up** five seconds).
3. Choose a screen clip, put the playhead inside it, and press **S**. The clip is cut in two.
4. Press **Delete** (leaves a gap) or **Shift+Delete** (closes the gap). Press **Ctrl+Z** to get it back.
5. Press **Enter** on a row to go to its name and times. Type `1:12.4`, `72.4` or `1m12.4s` into a time box and press Enter. A time that cannot work is explained in words.
6. Open **Keyboard shortcuts** under the list for every key.

"Start again with the sample lesson" puts everything back. "Save a copy of this edit" downloads the current edit as `edit.json` (the real edit file), so you can send it to me.

## What it does

* A table (`grid`) with a caption, column headers, and one row for every clip, overlay and zoom: Name, Track, Type, Start, Length, End, State.
* Up and Down choose a row (Home and End go to the first and last). The table is one stop in the Tab order, not one per row.
* A playhead, as a slider and as a box where an exact time can be typed.
* Commands that work from the keyboard and as buttons: split at the playhead (the selected clip, or every track), trim the start or end to the playhead, delete (leave a gap, or close it), join with the next clip, move earlier or later, mute, undo, redo, and "describe what is at the playhead" (story C4, on request).
* A form for the selected row: name, start, length, end, muted.
* Every result is a plain sentence, shown in a status line and read aloud through a polite live region.
* Undo and redo, one step per command.

## What it does not do yet

Dragging; choosing several rows at once; moving a clip to another track; volume and fade boxes; loading a real recording and saving to disk (the demo can only hand back a copy); playing the video; and the visual timeline, which is the next piece.

## Accessibility: what changed, in the terms the project asks for

**Keyboard access.** Every command is available from the keyboard and as a button. The list is one Tab stop; Up and Down move inside it. Nothing is disabled (a disabled button leaves the Tab order, and takes keyboard focus with it); a command that cannot be done says why instead. Letter keys (S, B, E, J, M, D) work only when the list or the playhead has focus, never inside a text box, so they cannot get in the way of typing. Ctrl+Z inside a text box undoes the typing, not the edit.

**No keyboard trap.** Tab and Shift+Tab always move on. Escape in a text box puts the old text back, and if there is nothing to put back, returns to the list. When the last row is deleted, focus goes to the "no clips" message instead of being lost.

**Focus order.** Heading, status line, playhead (slider, time box, two buttons), the list, the command buttons, the form for the selected row, the keyboard shortcuts. After a command that redraws the list, focus is put back on the chosen row if it was on the list; if it was on a button it stays on the button.

**Screen reader output.** The list is a `grid`; each row has its sentence as its name ("Add an assignment, screen on the Screen track, starts at 30 seconds, 30 seconds long, ends at 1 minute."), and its cells keep the column headers. The playhead is a `slider` whose spoken value says where it is and how that relates to the chosen row ("1 minute 12.4 seconds, 12.4 seconds into "Add an assignment""). Results go to a `status` live region, filled about 60 ms after the change so that a screen reader's own announcement of a focus change is not talked over. The slider's value is read as it changes, so the status line is not used for it as well. A value that is refused is marked `aria-invalid`, with the reason beside it.

**Looking at it.** The main text and controls (the page, status line, hints, headers, rows, buttons, key labels, boxes, error text) have at least 4.5:1 contrast in light and dark colours (checked by script). Windows high contrast (forced colours) uses the system colours. Text can be enlarged to 200% at 320 pixels wide without the page scrolling sideways (the table scrolls inside its own box). Nothing moves, so there is no motion to turn off. The chosen row and the rows the playhead is inside are marked by shape (a thick left edge, a dashed line), not by colour alone.

**Text that is meant to be read, and audio.** The status sentences are the only text this screen adds. It has no narration of its own yet; a screen reader reads them. A way to give them audio when the editor is part of OpenStage is not built.

## Please listen to this with a screen reader

What to check, and what to tell me. The first item matters most, because it decides how rows are read.

1. **Rows.** In the list, press Down. Is the whole row sentence read, once? Then add `?rows=cells` to the end of the page address, reload, and press Down again: which is clearer, the sentence or the cells with their column names? (If NVDA only reads the cells with no names, say so.)
2. **Playhead.** Press Right on the list. Is "Playhead at ..." read once? On the slider itself, press Right: is the value read once, not twice?
3. **A command.** Choose a clip, put the playhead inside it, press S. Is "Clip split at ..." read in full, or is it cut off by the row being read again?
4. **Delete.** Press Delete. Are the result and the newly chosen row both heard, in a sensible order?
5. **A wrong time.** Type `soon` into a time box and press Enter. Is the reason read, and is the box announced as invalid?
6. **Modes.** Does NVDA switch to focus mode when you reach the list and the slider by themselves, or do the arrow keys have to be passed on by hand?
7. **High contrast.** Turn on a Windows contrast theme (left Alt+left Shift+Print Screen). Can you still see which row is chosen, and where the playhead is?

## Choices that could have gone the other way

I made these so the work could go on. Each is one small change to reverse; tell me if you want any different.

1. **Exact values are edited in a form under the list, not inside the table's cells.** Editing inside cells is the usual spreadsheet way, but screen readers handle it much less reliably. Cost: one extra step (press Enter on a row).
2. **The list moves by row, not by cell.** Left and Right move the playhead instead of moving between cells.
3. **Each row is read as one sentence** rather than as cells. `?rows=cells` shows the other way, for comparing.
4. **Delete leaves a gap; Shift+Delete closes it.** The requirements say the author chooses every time, and the edit model gives no default, so both are named buttons. Which key does which is my choice. Undo brings either back.
5. **Single letters as shortcuts** (S split, Shift+S split every track, B trim the beginning, E trim the end, J join, M mute, D describe). They work only in the list and on the playhead, not in text boxes.
6. **Split acts on the chosen clip only, and refuses if the playhead is not inside it** ("it runs from 30 seconds to 1 minute, so the playhead has to be inside it"). Shift+S splits every track at the playhead instead, so picture and sound are cut together.
7. **After a split the choice stays on the first piece. After a delete it goes to the row that took the place** (or the one before, if it was the last).
8. **The playhead stays inside the edit.** A time typed past the end goes to the end, and says so. If an edit gets shorter, the playhead moves back with it.
9. **A typed time is applied when you press Enter or leave the box.** Leaving the box applies it even if you did not press Enter.
10. **Nothing is disabled.** A command that cannot be done says why.

## For developers

```
src/session.ts     the logic: selection, playhead, every command and its sentence. No screen; tested with Node alone.
src/panel.ts       the screen: plain DOM, no libraries. Only shows the session and passes keys to it.
src/clip-list.css  styles (colour tokens, dark mode, forced colours, 200% text)
src/sample.ts      the made-up lesson for the demo and the tests
demo/              the demo page, and serve.mjs, which takes the types out of the TypeScript as the browser asks for it
tools/browser-check.mjs   a developer check in real Chromium (needs Playwright; not part of the package)
test/              Node's own test runner
```

```
npx tsc -p .                       type check (TypeScript 5.8 or newer, @types/node)
node --test "test/*.test.ts"       the tests
node tools/browser-check.mjs       the browser check: PLAYWRIGHT_DIR=<folder of an installed playwright>
node demo/serve.mjs --open         the demo
```

No runtime dependencies. TypeScript with erasable syntax only (it runs under Node's type stripping and in the demo). The panel is written the way OpenStage's own Timeline panel is, so the two can be joined when the editor is part of OpenStage.
