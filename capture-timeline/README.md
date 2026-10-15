# The editing screens: preview, timeline and clip list

The editor of OpenStage Capture, so far as a package of its own with a demo page. It has three screens on one edit, which always make the same changes:

* **The clip list** (stories C1 and C2): a table of everything in an edit, with keyboard commands to split, trim, delete, join, move, rename and set exact values, each result said in words.
* **The timeline** (story C3): the tracks and their clips along a time axis, with a playhead, a zoom, start and end grips that are real sliders, and mouse dragging. Escape cancels a drag, and a whole drag is one undo step.
* **The preview** (story C4's picture, and playing): the picture of the edit at the playhead, with the overlays and the zoom, and the sound, and a way to play it.

It sits on the edit model in [`../capture-edit/`](../capture-edit/) and does not duplicate any of its rules: what a command does, the sentence that says so, how far a grip can go, and what plays at a moment all come from there.

**Nothing in this package has been heard with a screen reader, and the timeline must not ship until it has been (story C3).** It has been tested by scripts, and checked in a real browser by a script that reads the same information a screen reader is given (roles, names, states, focus) and drives everything with the keyboard and the mouse. How NVDA, Narrator, JAWS or VoiceOver actually speak it is not known until someone listens. The checklist for that is below.

## Try it (Windows)

Double-click **`capture\OPEN TIMELINE DEMO.bat`**. It opens a page in your browser with a made-up lesson (a screen recording cut into three clips, the voice and webcam cut the same way, two overlays and a zoom). It records nothing, and changes nothing on your computer unless you choose Save; the page is served only to your own computer and stops when you close the window. It needs Node.js 22.13 or newer, and a recent Edge or Chrome for the best results.

Things to try, with the keyboard only:

1. Press **Tab** until the timeline takes focus. **Left** and **Right** choose the clip before or after in the track; **Up** and **Down** choose the clip in the track above or below.
2. Press **.** (period) a few times to move the playhead 0.1 second (**,** goes back; **Page Up** and **Page Down** move it 5 seconds). Press **S** to split the chosen clip at the playhead.
3. Press **Tab** to reach the clip's **start grip** and **end grip**. Press **Right** a few times: it moves 0.1 second each time. Wait half a second, then press **Ctrl+Z**: the whole run of presses is undone as one step. **Escape** puts a grip back.
4. Press **Space** to play. The preview shows the pictures and overlays, and the playhead moves. **Space** again pauses. (The made-up lesson has no recordings, so the pictures are labelled stand-ins.)
5. **Shift+Right** adds the next clip to the choice; **Ctrl+Right** moves without changing the choice, and **Ctrl+Space** adds or removes the clip you are on. **Delete** deletes every chosen clip, and one **Ctrl+Z** brings them all back.
6. **F6** moves between the preview, the timeline and the clip list. **Enter** on a clip goes to its name, times, track, volume and fades in the clip list.
7. With the mouse: drag a clip to move it; drag it onto another track that takes it to move it there; drag a grip to trim; click the ruler to put the playhead there. **Escape** during a drag calls it off.

**Open an edit file** opens an `edit.json`. **Open a recording folder** opens a folder made by OpenStage Capture (the one that ends in `.capture`): if it holds an `edit.json` that is opened, otherwise a first edit is made from its takes, and the preview plays the real recordings. **Save** (or Ctrl+S) writes the edit into the recording folder, or into the file that was opened; where the browser has no file pickers it hands back a download instead. **Save a copy** always asks where.

Add `?rows=cells` to the page address to have the clip list's rows read by their cells instead of as sentences (to compare with a screen reader). Add `?long=1` for a one hour recording cut into 720 clips, to see that it stays quick.

## What it does

* **The clip list.** A table (`grid`) with a caption, column headers, and one row for every clip, overlay and zoom: Name, Track, Type, Start, Length, End, State. A form for the row you are on: name, start, length, end, track, volume, fade in, fade out, muted. Rows can be chosen several at a time.
* **The timeline.** A track for each of the edit's tracks, with each clip a block along a ruler. The playhead is a slider on the ruler. Zoom in and out (up to 400 pixels for each second) or show the whole edit; only the marks in view are drawn, so an hour-long recording stays quick. An empty track can be added, to move clips onto.
* **The commands**, from the keyboard in both the timeline and the list, and as buttons in the list: split at the playhead (the chosen clip, or every track), trim the start or end to the playhead, delete (leave a gap, or close it), join with the next clip, move earlier or later in the track, mute, undo, redo, play and pause, and "describe what is at the playhead" (story C4, on request). Split, trim, delete and mute act on every chosen clip as one undo step; the others ask for one.
* **The preview.** The pictures in track order (the webcam in the bottom right corner unless it says where), blur, blackout, shapes and arrows that zoom with the picture, text that does not, and the zoom moving along the exporter's curve (or cutting, for reduced motion). The sound of every microphone, computer-sound and sound-file clip, at the volume and fades set. A screen recording's own sound is not played, because the exporter does not use it.
* **Moving to another track.** A drag onto another track, or the Track box in the form, moves a clip, overlay or zoom to a track that takes it, keeping its time. A track that does not take it, or a clip already there, is explained in words.
* **Files.** Open and save an edit file, and open a recording folder. Unsaved changes are shown and asked about before leaving the page.
* Every result is a plain sentence, shown in one line at the top and read aloud through a polite live region that the three screens share.

## What it does not do yet

Snapping a drag to other clips or the playhead; dragging several clips at once (a drag moves the one you drag); choosing a stretch of time to remove; renaming or deleting a track; changing an overlay's text, place or look, or a zoom's region (the edit model can; there is no screen for it yet); the captions track; effects from the cursor and clicks; an export button (the exporter is `openstage-capture render --project <folder> --out lesson.mp4`, which reads the `edit.json` that Save writes into the recording folder); saving into the project automatically; and joining this to OpenStage's own panels and F6 cycling. Not yet tried: playing a real `screen.mp4` made by the recorder (the check plays WebM files, because the browser it uses cannot play MP4), on Windows, in Edge.

## Accessibility: what changed, in the terms the project asks for

**Keyboard access.** Every command is available from the keyboard and as a button, and every change that can be made with the mouse can be made without it: a drag by a key held on a grip or by the Start and Length boxes, a move to another track by the Track box, a range of clips by Shift and the arrow keys, clips that are not next to each other by Ctrl and the arrow keys with Ctrl+Space, the playhead by its keys or its box, the zoom by its keys or buttons. Nothing is disabled (a disabled button leaves the Tab order, and takes keyboard focus with it); a command that cannot be done says why instead. Letter keys (S, B, E, J, M, D) and Space work only when a clip, a row, a grip or the playhead has focus, never inside a text box, so they cannot get in the way of typing. Ctrl+Z inside a text box undoes the typing, not the edit.

**No keyboard trap.** Tab and Shift+Tab always move on. Escape in a text box puts the old text back, and if there is nothing to put back, returns to the list or the timeline. Escape on a grip, or during a mouse drag, cancels it. When the last row is deleted, focus goes to the "no clips" message instead of being lost. A mouse drag never has to be finished with the mouse.

**Focus order.** The line of announcements, the preview (its controls), the timeline (its controls, the playhead, the clips as one stop, then the start and end grips of the chosen clip), the clip list (the table as one stop, the command buttons, the form), the keyboard shortcuts. After a command that redraws a screen, focus is put back on the clip or row that had it. A key held on a grip ends the step when the focus leaves the grip.

**Screen reader output.**
* The list is a `grid`; each row has its sentence as its name ("Add an assignment, screen on the Screen track, starts at 30 seconds, 30 seconds long, ends at 1 minute."), and its cells keep the column headers. (`?rows=cells` reads the cells instead.)
* The timeline's clips are one `listbox` of `option`s grouped by track (`group`, named "Screen track, 3 clips"), each named by the same sentence and with its position in the track. The chosen clips are `aria-selected`.
* The playhead is a `slider` whose spoken value says where it is and how that relates to the chosen clip ("1 minute 12.4 seconds, 12.4 seconds into "Add an assignment""). The start and end grips are `slider`s named "Start of "Add an assignment"" with a spoken value ("starts at 30 seconds, 30 seconds long"), and with the range the edit allows as their minimum and maximum.
* Results go to a `status` live region, filled about 60 ms after the change so that a screen reader's own announcement of a change of focus is not talked over. A slider's value is read as it changes, so the line is not used for it as well. A value that is refused is marked `aria-invalid`, with the reason beside it.
* **While it plays, nothing is announced** except starting ("Playing from 1 minute 28.5 seconds."), pausing, and reaching the end, and the playhead slider's value is not changed sixty times a second; it is told where the playhead is when it stops. The picture has the name "Preview of the edit at 13 seconds"; what is showing is said on request (D, or the Describe button), and kept on the page as text.

**Looking at it.** The main text and controls (the page, the line of announcements, hints, headers, rows, buttons, key labels, boxes, error text, the track names, the clips of the five kinds of track in the demo, the marks of the ruler) have at least 4.5:1 contrast in light and dark colours (checked by script). Windows high contrast (forced colours) uses the system colours. Text can be enlarged to 200% at 320 pixels wide without the page scrolling sideways (the table and the timeline scroll inside their own boxes). A track is never told apart by colour alone: its name and kind are written at the start of its lane. The chosen clips have a thick outline, the one with the focus a thicker edge; muted clips are hatched and say "muted"; the playhead is a line. A zoom in the preview can be turned into a cut ("reduced motion"), which starts on if the computer asks for reduced motion. Nothing else moves except playing, which the author starts and can pause (Space), and no sound plays until they do.

**Text that is meant to be read, and audio.** The sentences this adds are read by a screen reader. It has no narration of its own yet. The picture of the preview has its description on request, but no audio description track is made from it yet. A way to give the sentences audio when the editor is part of OpenStage is not built.

## Please listen to this with a screen reader

What to check, and what to tell me. The first two matter most, because they decide how the screens are read.

1. **Rows.** In the clip list, press Down. Is the whole row sentence read, once? Then add `?rows=cells` to the end of the page address, reload, and press Down again: which is clearer, the sentence or the cells with their column names?
2. **The timeline's clips.** Tab to the timeline and press Right and Down. Is each clip read as its sentence, with its track and its place ("2 of 3")? Does NVDA stay in focus mode on the clips, so the arrow keys work, or do they have to be passed on by hand?
3. **The grips.** Tab to a grip and press Right three times. Is the value read each time ("starts at 30.1 seconds ...") and only once?
4. **Playing.** Press Space. Is "Playing from ..." read, and then quiet until you press Space again ("Paused at ...")? Does the slider say where the playhead is after it stops?
5. **A command.** Choose a clip, put the playhead inside it, press S. Is "Clip split at ..." read in full, or is it cut off by the clip being read again?
6. **Delete several.** Shift+Right to choose three clips, press Delete. Are the result and the newly chosen clip both heard, in a sensible order?
7. **A wrong time.** Type `soon` into a time box and press Enter. Is the reason read, and is the box announced as invalid?
8. **F6.** Does it say where it arrived, and is the preview's Play button a sensible first stop?
9. **Opening.** Open a recording folder. Is what was opened, and what was left out, said clearly?
10. **High contrast.** Turn on a Windows contrast theme (left Alt+left Shift+Print Screen). Can you still see which clips are chosen, the grips, and where the playhead is?

## Choices that could have gone the other way

I made these so the work could go on. Each is one small change to reverse; tell me if you want any different.

**The clip list** (from the first part)

1. **Exact values are edited in a form under the list, not inside the table's cells.** Screen readers handle editing inside cells much less reliably. Cost: one extra step (press Enter on a row).
2. **The list moves by row, not by cell.** Left and Right move the playhead instead of moving between cells.
3. **Each row is read as one sentence** rather than as cells. `?rows=cells` shows the other way, for comparing.
4. **Delete leaves a gap; Shift+Delete closes it.** The requirements say the author chooses every time, so both are named buttons. Which key does which is my choice. Undo brings either back.
5. **Single letters as shortcuts** (S split, Shift+S split every track, B trim the beginning, E trim the end, J join, M mute, D describe, Space play), only when a clip, row, grip or the playhead has focus.
6. **Split acts on the chosen clip only, and refuses if the playhead is not inside it.** Shift+S splits every track at the playhead instead, so picture and sound are cut together.
7. **After a split the choice stays on the first piece. After a delete it goes to the row that took the place** (or the one before, if it was the last).
8. **The playhead stays inside the edit.** A time typed past the end goes to the end, and says so.
9. **A typed time is applied when you press Enter or leave the box.**
10. **Nothing is disabled.** A command that cannot be done says why.

**The timeline and choosing several**

11. **The clips are one listbox, grouped by track, with one Tab stop**, so ten tracks of clips are not hundreds of stops. Left and Right go along a track, Up and Down across tracks.
12. **Only the chosen clip has grips** (two more Tab stops), not every clip. Every clip's start and end can also be set in the form, or dragged.
13. **A key held on a grip is one undo step.** It ends half a second after the last key press, or at once on any other command or when the focus leaves the grip.
14. **The playhead keys from the clips are comma and period** (0.1 second, with Shift 1 second) and Page Up and Page Down (5 seconds), because Left and Right choose clips. On the ruler itself and in the list, the playhead keys are the arrow keys as in OpenStage's Timeline.
15. **Ctrl with the arrow keys moves without changing the choice, and Ctrl+Space adds or removes the clip you are on**, the usual way to choose clips that are not next to each other. Shift with the arrow keys chooses a range. A choice always keeps at least one row.
16. **With several chosen, split, trim, delete and mute act on all of them as one undo step; everything else asks for one.** Moving several clips together by dragging is not built.
17. **A drag lands on tidy times** (the nearest 0.01 second) and is kept inside what the edit allows (not into its neighbour, not past its recording). There is no snapping yet. A zoom stops at the zoom before and after it, though the edit model would let it jump over them.
18. **A drag that ends over another track moves the clip there, in the same undo step**, and if that track does not take it, says why and leaves it on its own track. Where nothing else stops it, a clip can be moved or made longer up to 10 minutes past the end of the edit.
19. **Zoom goes by halves and doubles**, from the whole edit to 400 pixels a second, and keeps the playhead where it is on the screen.

**The preview, playing, and files**

20. **The recordings' own sound is not played from a screen or webcam clip**, because the exporter does not use it: sound comes from the microphone and computer-sound clips. If you would rather the preview used the screen recording's mix, the exporter has to change too.
21. **Playing is the playhead's clock**: each recording is moved only when it has drifted more than 0.3 second, so it does not stutter. Playing from the end starts again from the start.
22. **While it plays, a screen reader is not told the playhead's value**; it is told when it stops. The rows of the list are marked a few times a second, not sixty.
23. **Opening a recording folder that has an `edit.json` opens that edit** and does not add takes made since. One that has none gets a first edit made from its finished takes, laid one after another, with a track each for the screen, the webcam, the microphone and the computer's sound.
24. **Save writes `edit.json` into the recording folder** (where the browser lets a page write to a folder it was given, as Edge and Chrome do), otherwise into the file that was opened, otherwise a download. A copy always asks.
25. **The text in the preview is drawn with the browser's fonts**, so line breaks can differ a little from the exported video. The exported video is what counts.

## For developers

```
src/session.ts          the logic: choice, playhead, every command and its sentence, drags, playing. No screen; tested with Node alone.
src/geometry.ts         the timeline's arithmetic: scale, zoom steps, ruler marks, pointer to time, arrow-key navigation
src/commands.ts         the commands and their keys, in one table that both screens use
src/panel.ts            the clip list on screen: plain DOM, no libraries
src/timeline-panel.ts   the timeline on screen
src/preview-panel.ts    the preview and playing
src/editor.ts           the three together, with F6
src/announcer.ts        the one line of announcements they share
src/project.ts          opening a recording folder: read capture.json and the takes, make the edit, reach the media
src/sample.ts           the made-up lesson for the demo and the tests, and the one hour edit
src/*.css               styles (colour tokens, dark mode, forced colours, 200% text)
demo/                   the demo page; serve.mjs takes the types out of the TypeScript as the browser asks for it; files.ts opens and saves
tools/browser-check.mjs a developer check in real Chromium (needs Playwright; not part of the package); make-fixture.mjs makes a test recording (needs ffmpeg)
test/                   Node's own test runner
```

```
npx tsc -p .                       type check (TypeScript 5.8 or newer, @types/node)
node --test "test/*.test.ts"       the tests
node tools/browser-check.mjs       the browser check: PLAYWRIGHT_DIR=<folder of an installed playwright>
node demo/serve.mjs --open         the demo
```

No runtime dependencies. TypeScript with erasable syntax only (it runs under Node's type stripping and in the demo). The panels are written the way OpenStage's own Timeline panel is, so they can be joined when the editor is part of OpenStage. The folder and file pickers of Chrome and Edge are used where they exist (`?pickers=input` turns them off); the check reaches only the fallback, so the pickers themselves are untested.
