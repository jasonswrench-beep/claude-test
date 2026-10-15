// SPDX-License-Identifier: Apache-2.0
//! Noting what the author does while recording (story A6): where the cursor goes, the clicks, the keys, which window
//! is in front, and the name of the thing that was clicked.
//!
//! This is private information, so everything here is built around three rules:
//!
//! * It is **off** unless the author turns it on for a recording, and when it is on the author is told in words, with
//!   a sound, and in every state report (see [`announcement`]).
//! * Typed characters are **never** written for a password field, and also not when the recorder cannot tell what
//!   kind of field has the keyboard. Doubt means silence. See [`InteractionFilter`].
//! * It goes only into the take's own `events.jsonl` (see `docs/capture-project-format.md`), which is never published,
//!   and the author can read it in words and delete it (`interaction_review` in `openstage-capture-project`).
//!
//! What Windows does (hooks that tell this program about every click and key, and the accessibility information that
//! names the clicked item) is in `win_interactions.rs`. What is decided here, the same everywhere and tested without
//! a Windows computer, is: how often the cursor is written, which keys are written, what a position means, and
//! how Windows' clock is matched to recording time.

use openstage_capture_project::{LogBounds, LogEvent};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The cursor is written at most this often (a picture lasts 33 ms at 30 a second), and its last position always is.
pub const MOVE_EVERY_MS: u64 = 33;
/// A change to the window in front (or its title) is written at most this often, and the last one always is.
pub const WINDOW_EVERY_MS: u64 = 250;

// ---------------------------------------------------------------------------------------------------------------
// What is recorded

/// How much of what the author does is noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Nothing. This is what happens unless the author asks.
    Off,
    /// The cursor, the clicks, the windows and the names of clicked things, but not one key.
    NoKeys,
    /// All of that and the keys pressed, except in password fields.
    All,
}

impl Level {
    /// The words `--interactions` takes: `off`, `no-keys` or `on`.
    pub fn parse(text: &str) -> Result<Level, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Level::Off),
            "no-keys" => Ok(Level::NoKeys),
            "on" => Ok(Level::All),
            other => Err(format!(
                "--interactions must be off, no-keys or on, not \"{other}\". Off (the default) notes nothing; no-keys notes the cursor, clicks and windows but no keys; on notes keys too."
            )),
        }
    }

    pub fn is_on(self) -> bool {
        self != Level::Off
    }

    /// Whether the keys pressed are noted.
    pub fn keys(self) -> bool {
        self == Level::All
    }
}

/// The sentence said when a recording that notes what the author does is about to begin, and again with each take.
pub fn announcement(level: Level) -> String {
    match level {
        Level::Off => String::new(),
        Level::NoKeys => "Interaction recording is ON: the cursor, clicks, windows and names of clicked items are being noted. Keys are not. Nothing is noted while paused.".to_string(),
        Level::All => "Interaction recording is ON: the cursor, clicks, windows, names of clicked items and keys are being noted. Keys typed in password fields are never noted. Nothing is noted while paused.".to_string(),
    }
}

/// The explanation given when the author asks for it, before anything is recorded. Plain words, and it says what
/// it cannot promise.
pub fn explanation(level: Level) -> String {
    let what = if level.keys() {
        "where the cursor is, each mouse click, the title and place of the window in front, the name and kind of the item you click (such as the button Save), and the keys you press"
    } else {
        "where the cursor is, each mouse click, the title and place of the window in front, and the name and kind of the item you click (such as the button Save). No keys are noted"
    };
    let mut text = format!(
        "Interaction recording is on, because you asked for it. While recording time runs, the recorder writes a private log of {what}. It is the file events.jsonl in the take's folder. It stays on this computer, is never put into a published course, and is what lets the editor suggest zooms and write step lists. Nothing is noted while recording is paused. To read what was kept, or to delete it, use the interactions command: openstage-capture interactions --project <the recording folder> --take <number>."
    );
    if level.keys() {
        text.push_str(
            " Keys are never written for a password field. Windows decides what a password field is, and some programs do not say: when the recorder cannot tell what kind of field has the keyboard, it writes no letters, digits or symbols at all, only keys such as Enter and shortcuts such as Ctrl+S. If you will type anything private, use --interactions no-keys instead.",
        );
    }
    text
}

// ---------------------------------------------------------------------------------------------------------------
// Keys

/// Which of the four modifier keys were held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Mods {
    /// The names written to the log, always in the same order.
    pub fn names(self) -> Vec<String> {
        let mut out = Vec::new();
        for (held, name) in [(self.ctrl, "ctrl"), (self.alt, "alt"), (self.shift, "shift"), (self.win, "win")] {
            if held {
                out.push(name.to_string());
            }
        }
        out
    }

    /// Whether this is a shortcut (Ctrl+S, Alt+F4, Win+D) rather than a character being typed. Ctrl and Alt held
    /// together is how many keyboards make the third symbol on a key (AltGr), so that is a character.
    fn is_shortcut(self) -> bool {
        self.win || (self.ctrl != self.alt)
    }
}

/// Whether a virtual key code is Shift, Ctrl, Alt or the Windows key (either side). They are never written as keys of their own;
/// they are written as the modifiers of the key pressed with them.
pub fn is_modifier_key(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

/// Whether a key types a character: a letter, a digit, a space, a symbol or the number pad's digits and operators.
pub fn is_character_key(vk: u32) -> bool {
    matches!(vk, 0x20 | 0x30..=0x39 | 0x41..=0x5A | 0x60..=0x6F | 0xBA..=0xC0 | 0xDB..=0xDE)
}

/// The name written for a key. It names the key, not the character: letters are lower case and the symbols are named
/// as on a US keyboard (a `semicolon` is the key to the right of L), and whether Shift was held is in the modifiers.
/// `None` for keys that are not worth noting (volume, browser buttons) or not known.
pub fn key_name(vk: u32) -> Option<String> {
    let fixed = match vk {
        0x08 => "backspace",
        0x09 => "tab",
        0x0D => "enter",
        0x13 => "pause",
        0x14 => "caps-lock",
        0x1B => "escape",
        0x20 => "space",
        0x21 => "page-up",
        0x22 => "page-down",
        0x23 => "end",
        0x24 => "home",
        0x25 => "left",
        0x26 => "up",
        0x27 => "right",
        0x28 => "down",
        0x2C => "print-screen",
        0x2D => "insert",
        0x2E => "delete",
        0x5D => "menu",
        0x6A => "numpad-multiply",
        0x6B => "numpad-add",
        0x6D => "numpad-subtract",
        0x6E => "numpad-decimal",
        0x6F => "numpad-divide",
        0x90 => "num-lock",
        0x91 => "scroll-lock",
        0xBA => "semicolon",
        0xBB => "equals",
        0xBC => "comma",
        0xBD => "minus",
        0xBE => "period",
        0xBF => "slash",
        0xC0 => "backtick",
        0xDB => "left-bracket",
        0xDC => "backslash",
        0xDD => "right-bracket",
        0xDE => "quote",
        _ => "",
    };
    if !fixed.is_empty() {
        return Some(fixed.to_string());
    }
    match vk {
        0x30..=0x39 => Some(((b'0' + (vk - 0x30) as u8) as char).to_string()),
        0x41..=0x5A => Some(((b'a' + (vk - 0x41) as u8) as char).to_string()),
        0x60..=0x69 => Some(format!("numpad-{}", vk - 0x60)),
        0x70..=0x87 => Some(format!("f{}", vk - 0x6F)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// What the keyboard is aimed at, and what was clicked

/// What has the keyboard, as far as the recorder can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusKind {
    /// A password field.
    Password,
    /// A field that holds text and is not a password field.
    Text,
    /// Something else (a button, a list, a page whose contents are not described to Windows).
    Other,
    /// The recorder could not find out.
    Unknown,
}

/// Decide what has the keyboard from what Windows' accessibility information says: whether it is marked as a password
/// field and what kind of control it is (the numbers are Windows' UI Automation control types).
pub fn classify_focus(is_password: bool, control_type: i32) -> FocusKind {
    if is_password {
        return FocusKind::Password;
    }
    match control_type {
        // Edit, ComboBox, Document: places where text is typed.
        50004 | 50003 | 50030 => FocusKind::Text,
        _ => FocusKind::Other,
    }
}

/// The kind of control, in English, from Windows' UI Automation control type number. English whatever the language of
/// Windows, so the log reads the same everywhere.
pub fn role_name(control_type: i32) -> &'static str {
    match control_type {
        50000 => "button",
        50001 => "calendar",
        50002 => "check box",
        50003 => "combo box",
        50004 => "edit",
        50005 => "link",
        50006 => "image",
        50007 => "list item",
        50008 => "list",
        50009 => "menu",
        50010 => "menu bar",
        50011 => "menu item",
        50012 => "progress bar",
        50013 => "radio button",
        50014 => "scroll bar",
        50015 => "slider",
        50016 => "spinner",
        50017 => "status bar",
        50018 => "tab",
        50019 => "tab item",
        50020 => "text",
        50021 => "tool bar",
        50022 => "tool tip",
        50023 => "tree",
        50024 => "tree item",
        50025 => "custom",
        50026 => "group",
        50027 => "thumb",
        50028 => "data grid",
        50029 => "data item",
        50030 => "document",
        50031 => "split button",
        50032 => "window",
        50033 => "pane",
        50034 => "header",
        50035 => "header item",
        50036 => "table",
        50037 => "title bar",
        50038 => "separator",
        50039 => "semantic zoom",
        50040 => "app bar",
        _ => "unknown",
    }
}

/// The two questions the recorder asks Windows' accessibility information. Asking can take a while (a program that
/// is busy answers late), so this is only ever asked from the thread that does the noting, never from the thread that
/// looks at the screen.
pub trait Lookups {
    /// What has the keyboard.
    fn focus(&mut self) -> FocusKind;
    /// The name and kind of the item at this point of the desktop, if it has a name.
    fn element_at(&mut self, x: i32, y: i32) -> Option<(String, String)>;
}

// ---------------------------------------------------------------------------------------------------------------
// The raw happenings and the filter that turns them into log lines

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl Button {
    pub fn name(self) -> &'static str {
        match self {
            Button::Left => "left",
            Button::Right => "right",
            Button::Middle => "middle",
            Button::Back => "back",
            Button::Forward => "forward",
        }
    }
}

/// The window in front. `bounds` is `(x, y, width, height)` on the whole desktop, in pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowState {
    pub title: String,
    pub process: String,
    pub bounds: Option<(i32, i32, i32, i32)>,
}

/// Something that happened, with positions on the whole desktop (the main display's top left corner is 0, 0 and other
/// displays can have negative positions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Raw {
    Move { x: i32, y: i32 },
    Click { x: i32, y: i32, button: Button },
    KeyDown { vk: u32, mods: Mods },
    KeyUp { vk: u32 },
    Window(WindowState),
}

/// Holds back a value that changes faster than it is worth writing, and makes sure the last one still gets written.
struct Throttle<T> {
    every_ms: u64,
    written: Option<(u64, T)>,
    pending: Option<(u64, T)>,
}

impl<T: Clone + PartialEq> Throttle<T> {
    fn new(every_ms: u64) -> Self {
        Self { every_ms, written: None, pending: None }
    }

    /// A new value at time `t`. Returns what is to be written now, if anything.
    fn offer(&mut self, t: u64, value: T) -> Option<(u64, T)> {
        if self.written.as_ref().is_some_and(|(_, w)| *w == value) {
            // Back where it was last written: nothing to say.
            self.pending = None;
            return None;
        }
        let due = self.written.as_ref().is_none_or(|(at, _)| t.saturating_sub(*at) >= self.every_ms);
        if due {
            self.pending = None;
            self.written = Some((t, value.clone()));
            Some((t, value))
        } else {
            self.pending = Some((t, value));
            None
        }
    }

    /// The held-back value, if enough time has passed since the last one was written.
    fn due(&mut self, now: u64) -> Option<(u64, T)> {
        let (at, _) = self.written.as_ref()?;
        if self.pending.is_none() || now.saturating_sub(*at) < self.every_ms {
            return None;
        }
        self.flush()
    }

    /// The held-back value, whatever the time.
    fn flush(&mut self) -> Option<(u64, T)> {
        let (t, value) = self.pending.take()?;
        self.written = Some((t, value.clone()));
        Some((t, value))
    }

    /// This value has been written by other means.
    fn note(&mut self, t: u64, value: T) {
        self.pending = None;
        self.written = Some((t, value));
    }
}

/// Decides what goes into the log. Everything it is given is stamped with recording time already.
///
/// * Positions are written relative to the recorded display's top left corner (`origin` is where that corner is on the
///   desktop), so they are pixels of the video when the whole display is recorded.
/// * The cursor and the window in front are written when they change, no faster than [`MOVE_EVERY_MS`] and
///   [`WINDOW_EVERY_MS`], and the last one is not lost.
/// * A click is written with its position, followed by the name and kind of the clicked item when Windows has one.
/// * Keys are written only at [`Level::All`], one line per press (not per repeat), with their modifiers, and
///   **never** when the keyboard is aimed at a password field. A character (a letter, digit or symbol typed on its own)
///   is also not written unless the recorder is sure the keyboard is aimed at a text field that is not a password
///   field: Windows does not describe every program's fields, and a program that does not say could be hiding one.
///   Keys that are not characters (Enter, the arrows) and shortcuts (Ctrl+S) cannot spell a secret and are written
///   unless the keyboard is known to be in a password field.
/// * The times never go backwards, so the log can be read in order.
pub struct InteractionFilter {
    keys: bool,
    origin: (i32, i32),
    cursor: Throttle<(i32, i32)>,
    window: Throttle<WindowState>,
    down: BTreeSet<u32>,
    last_t: u64,
    out: Vec<(u64, LogEvent)>,
}

impl InteractionFilter {
    pub fn new(keys: bool, origin: (i32, i32)) -> Self {
        Self {
            keys,
            origin,
            cursor: Throttle::new(MOVE_EVERY_MS),
            window: Throttle::new(WINDOW_EVERY_MS),
            down: BTreeSet::new(),
            last_t: 0,
            out: Vec::new(),
        }
    }

    fn put(&mut self, t: u64, event: LogEvent) {
        let t = t.max(self.last_t);
        self.last_t = t;
        self.out.push((t, event));
    }

    fn put_cursor(&mut self, t: u64, (x, y): (i32, i32)) {
        let event = LogEvent::CursorMove { x: x - self.origin.0, y: y - self.origin.1 };
        self.put(t, event);
    }

    fn put_window(&mut self, t: u64, window: &WindowState) {
        let (ox, oy) = self.origin;
        let event = LogEvent::Window {
            title: window.title.clone(),
            process: window.process.clone(),
            bounds: window.bounds.map(|(x, y, width, height)| LogBounds { x: x - ox, y: y - oy, width, height }),
        };
        self.put(t, event);
    }

    /// Something happened at recording time `t`. Returns the lines to write.
    pub fn handle(&mut self, t: u64, raw: Raw, lookups: &mut dyn Lookups) -> Vec<(u64, LogEvent)> {
        match raw {
            Raw::Move { x, y } => {
                if let Some((at, at_point)) = self.cursor.offer(t, (x, y)) {
                    self.put_cursor(at, at_point);
                }
            }
            Raw::Click { x, y, button } => {
                // The path of the cursor ends where the click is.
                if let Some((at, point)) = self.cursor.flush() {
                    self.put_cursor(at, point);
                }
                self.cursor.note(t, (x, y));
                let event =
                    LogEvent::Click { x: x - self.origin.0, y: y - self.origin.1, button: button.name().to_string() };
                self.put(t, event);
                if let Some((name, role)) = lookups.element_at(x, y) {
                    if !name.trim().is_empty() {
                        self.put(t, LogEvent::Element { name, role });
                    }
                }
            }
            Raw::KeyDown { vk, mods } => self.key_down(t, vk, mods, lookups),
            Raw::KeyUp { vk } => {
                self.down.remove(&vk);
            }
            Raw::Window(window) => {
                // Keys still counted as held when the window changes are most likely ones whose release was missed.
                if self
                    .window
                    .written
                    .as_ref()
                    .is_none_or(|(_, w)| w.title != window.title || w.process != window.process)
                {
                    self.down.clear();
                }
                if let Some((at, shown)) = self.window.offer(t, window) {
                    self.put_window(at, &shown);
                }
            }
        }
        std::mem::take(&mut self.out)
    }

    fn key_down(&mut self, t: u64, vk: u32, mods: Mods, lookups: &mut dyn Lookups) {
        if !self.keys || is_modifier_key(vk) {
            return;
        }
        // A key held down repeats; it is one press.
        if !self.down.insert(vk) {
            return;
        }
        let Some(name) = key_name(vk) else { return };
        let allowed = match lookups.focus() {
            FocusKind::Password => false,
            FocusKind::Text => true,
            FocusKind::Other | FocusKind::Unknown => !is_character_key(vk) || mods.is_shortcut(),
        };
        if allowed {
            self.put(t, LogEvent::Key { key: name, modifiers: mods.names() });
        }
    }

    /// Time has passed with nothing new: write what was held back if it is due.
    pub fn tick(&mut self, t: u64) -> Vec<(u64, LogEvent)> {
        if let Some((at, point)) = self.cursor.due(t) {
            self.put_cursor(at, point);
        }
        if let Some((at, window)) = self.window.due(t) {
            self.put_window(at, &window);
        }
        std::mem::take(&mut self.out)
    }

    /// Recording is ending: write whatever is still held back.
    pub fn finish(&mut self) -> Vec<(u64, LogEvent)> {
        if let Some((at, point)) = self.cursor.flush() {
            self.put_cursor(at, point);
        }
        if let Some((at, window)) = self.window.flush() {
            self.put_window(at, &window);
        }
        std::mem::take(&mut self.out)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Matching Windows' clock to recording time

/// Matches moments on the computer's clock to recording time. The recording loop says "it is now `media_ms`" every few
/// milliseconds; something that happened a little earlier or later is placed by how long before or after that it was.
/// While recording is paused nothing has a recording time, so nothing is kept.
#[derive(Debug, Clone)]
pub struct Timeline {
    at: Instant,
    ms: u64,
    paused: bool,
}

pub type SharedTimeline = Arc<Mutex<Timeline>>;

impl Timeline {
    /// Recording time 0 is `now`.
    pub fn new(now: Instant) -> Self {
        Self { at: now, ms: 0, paused: false }
    }

    pub fn shared(now: Instant) -> SharedTimeline {
        Arc::new(Mutex::new(Self::new(now)))
    }

    /// It is `now` on the clock and `media_ms` of recording.
    pub fn set(&mut self, now: Instant, media_ms: u64) {
        self.at = now;
        self.ms = media_ms;
    }

    /// Recording is paused: nothing that happens now is kept.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Recording carries on from where it stopped.
    pub fn resume(&mut self, now: Instant) {
        self.at = now;
        self.paused = false;
    }

    /// The recording time of a moment, or `None` while paused.
    pub fn media_ms_at(&self, moment: Instant) -> Option<u64> {
        if self.paused {
            return None;
        }
        Some(if moment >= self.at {
            self.ms + moment.duration_since(self.at).as_millis() as u64
        } else {
            self.ms.saturating_sub(self.at.duration_since(moment).as_millis() as u64)
        })
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The platform, and a take's use of it

/// What the platform needs to know to start noting.
#[derive(Debug, Clone, Copy)]
pub struct InteractionSetup {
    /// Whether keys are noted.
    pub keys: bool,
    /// Where the recorded display's top left corner is on the desktop.
    pub origin: (i32, i32),
}

/// What the platform has noted since it was last asked.
#[derive(Debug, Default)]
pub struct Drained {
    pub lines: Vec<(u64, LogEvent)>,
    /// Things the author should be told, as (code, message), each given once.
    pub notices: Vec<(String, String)>,
    /// How many happenings were thrown away because the platform could not keep up, in all so far.
    pub dropped: u64,
}

/// A running collector of what the author does. Dropping it stops it.
pub trait InteractionSource {
    fn drain(&mut self) -> Drained;
}

/// Starts collecting. On Windows this installs the hooks and starts the accessibility lookups.
pub trait InteractionPlatform {
    fn start(&self, setup: &InteractionSetup, timeline: SharedTimeline) -> Result<Box<dyn InteractionSource>, String>;
}

/// How many lines of each kind were written in a take.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InteractionCounts {
    pub moves: u64,
    pub clicks: u64,
    pub keys: u64,
    pub windows: u64,
    pub elements: u64,
}

impl InteractionCounts {
    fn add(&mut self, event: &LogEvent) {
        match event {
            LogEvent::CursorMove { .. } => self.moves += 1,
            LogEvent::Click { .. } => self.clicks += 1,
            LogEvent::Key { .. } => self.keys += 1,
            LogEvent::Window { .. } => self.windows += 1,
            LogEvent::Element { .. } => self.elements += 1,
            LogEvent::Marker { .. } => {}
        }
    }
}

/// What a take's interaction recording did, for the summary at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InteractionsReport {
    pub keys: bool,
    pub counts: InteractionCounts,
    /// Happenings thrown away because the computer could not keep up.
    pub dropped: u64,
    pub recorded_ms: u64,
}

/// One take's interaction recording, driven by the same calls as the rest of the take.
pub struct InteractionTake {
    timeline: SharedTimeline,
    source: Box<dyn InteractionSource>,
    keys: bool,
    lines: Vec<(u64, LogEvent)>,
    notices: Vec<(String, String)>,
    counts: InteractionCounts,
    dropped: u64,
    dropped_said: bool,
    media_ms: u64,
}

impl InteractionTake {
    /// Start noting. Recording time 0 is now.
    pub fn start(platform: &dyn InteractionPlatform, setup: &InteractionSetup) -> Result<Self, String> {
        let timeline = Timeline::shared(Instant::now());
        let source = platform.start(setup, Arc::clone(&timeline))?;
        Ok(Self {
            timeline,
            source,
            keys: setup.keys,
            lines: Vec::new(),
            notices: Vec::new(),
            counts: InteractionCounts::default(),
            dropped: 0,
            dropped_said: false,
            media_ms: 0,
        })
    }

    fn with_timeline(&self, change: impl FnOnce(&mut Timeline)) {
        change(&mut self.timeline.lock().unwrap_or_else(|e| e.into_inner()));
    }

    /// Collect what the platform has noted so far.
    fn collect(&mut self) {
        let drained = self.source.drain();
        for (_, event) in &drained.lines {
            self.counts.add(event);
        }
        self.lines.extend(drained.lines);
        self.notices.extend(drained.notices);
        self.dropped = self.dropped.max(drained.dropped);
        if self.dropped > 0 && !self.dropped_said {
            self.dropped_said = true;
            self.notices.push((
                "interactions-dropped".to_string(),
                "Windows was too slow to answer for a moment, so some of what you did with the mouse or keyboard was not noted. The video and the sound are not affected.".to_string(),
            ));
        }
    }

    /// Recording time is now `media_ms`: tell the platform, and collect what it has noted.
    pub fn pump(&mut self, media_ms: u64) {
        self.media_ms = media_ms;
        self.with_timeline(|t| t.set(Instant::now(), media_ms));
        self.collect();
    }

    /// Recording is paused: nothing is noted until it carries on.
    pub fn paused(&mut self) {
        self.with_timeline(|t| t.pause());
    }

    pub fn resumed(&mut self) {
        self.with_timeline(|t| t.resume(Instant::now()));
    }

    /// The lines noted since the last time this was called, ready for the log.
    pub fn take_lines(&mut self) -> Vec<(u64, LogEvent)> {
        self.collect();
        std::mem::take(&mut self.lines)
    }

    pub fn take_notices(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.notices)
    }

    /// Stop noting. Returns what was done. The lines noted so far must have been taken (`take_lines`) first; whatever
    /// the platform still holds is not written, and not counted.
    pub fn finish(self, media_ms: u64) -> InteractionsReport {
        let InteractionTake { keys, counts, dropped, media_ms: last_ms, source, .. } = self;
        // Stopping the platform is dropping it.
        drop(source);
        InteractionsReport { keys, counts, dropped, recorded_ms: media_ms.max(last_ms) }
    }
}

/// Test doubles for the pieces that need Windows.
#[cfg(test)]
pub(crate) mod fakes {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// What the platform was started with: whether keys are noted, and the display's corner.
    pub type StartedWith = (bool, (i32, i32));

    /// Answers the accessibility questions from a script.
    pub struct FakeLookups {
        pub focus: FocusKind,
        pub element: Option<(String, String)>,
        pub focus_asked: usize,
        pub points_asked: Vec<(i32, i32)>,
    }

    impl FakeLookups {
        pub fn new(focus: FocusKind) -> Self {
            Self { focus, element: None, focus_asked: 0, points_asked: Vec::new() }
        }
    }

    impl Lookups for FakeLookups {
        fn focus(&mut self) -> FocusKind {
            self.focus_asked += 1;
            self.focus
        }

        fn element_at(&mut self, x: i32, y: i32) -> Option<(String, String)> {
            self.points_asked.push((x, y));
            self.element.clone()
        }
    }

    /// A collector that hands over what a test puts in it, and notes when it is stopped.
    #[derive(Clone, Default)]
    pub struct FakeFeed {
        pub waiting: Arc<Mutex<Drained>>,
        pub stopped: Arc<AtomicBool>,
        pub started_with: Arc<Mutex<Option<StartedWith>>>,
        pub timeline: Arc<Mutex<Option<SharedTimeline>>>,
    }

    impl FakeFeed {
        pub fn push(&self, t: u64, event: LogEvent) {
            self.waiting.lock().unwrap().lines.push((t, event));
        }

        pub fn say(&self, code: &str, message: &str) {
            self.waiting.lock().unwrap().notices.push((code.to_string(), message.to_string()));
        }

        pub fn set_dropped(&self, dropped: u64) {
            self.waiting.lock().unwrap().dropped = dropped;
        }

        pub fn was_stopped(&self) -> bool {
            self.stopped.load(Ordering::SeqCst)
        }

        /// What the platform's clock would say recording time is for a moment.
        pub fn media_ms_at(&self, moment: Instant) -> Option<u64> {
            let timeline = self.timeline.lock().unwrap().clone().expect("the platform was started");
            let guard = timeline.lock().unwrap();
            guard.media_ms_at(moment)
        }
    }

    pub struct FakeSource {
        feed: FakeFeed,
    }

    impl InteractionSource for FakeSource {
        fn drain(&mut self) -> Drained {
            let mut waiting = self.feed.waiting.lock().unwrap();
            let dropped = waiting.dropped;
            Drained {
                lines: std::mem::take(&mut waiting.lines),
                notices: std::mem::take(&mut waiting.notices),
                dropped,
            }
        }
    }

    impl Drop for FakeSource {
        fn drop(&mut self) {
            self.feed.stopped.store(true, Ordering::SeqCst);
        }
    }

    pub struct FakePlatform {
        pub feed: FakeFeed,
        pub fail: Option<String>,
    }

    impl FakePlatform {
        pub fn new() -> Self {
            Self { feed: FakeFeed::default(), fail: None }
        }
    }

    impl InteractionPlatform for FakePlatform {
        fn start(
            &self,
            setup: &InteractionSetup,
            timeline: SharedTimeline,
        ) -> Result<Box<dyn InteractionSource>, String> {
            if let Some(why) = &self.fail {
                return Err(why.clone());
            }
            *self.feed.started_with.lock().unwrap() = Some((setup.keys, setup.origin));
            *self.feed.timeline.lock().unwrap() = Some(timeline);
            Ok(Box::new(FakeSource { feed: self.feed.clone() }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::*;
    use super::*;
    use std::time::Duration;

    fn lookups(focus: FocusKind) -> FakeLookups {
        FakeLookups::new(focus)
    }

    fn mv(x: i32, y: i32) -> Raw {
        Raw::Move { x, y }
    }

    fn keydown(vk: u32) -> Raw {
        Raw::KeyDown { vk, mods: Mods::default() }
    }

    fn key(name: &str, modifiers: &[&str]) -> LogEvent {
        LogEvent::Key { key: name.into(), modifiers: modifiers.iter().map(|m| m.to_string()).collect() }
    }

    fn cursor(x: i32, y: i32) -> LogEvent {
        LogEvent::CursorMove { x, y }
    }

    const A: u32 = 0x41;
    const ENTER: u32 = 0x0D;
    const S: u32 = 0x53;

    // ----- the choice

    #[test]
    fn the_level_is_read_from_its_three_words_and_nothing_else() {
        assert_eq!(Level::parse("off").unwrap(), Level::Off);
        assert_eq!(Level::parse(" No-Keys ").unwrap(), Level::NoKeys);
        assert_eq!(Level::parse("ON").unwrap(), Level::All);
        let e = Level::parse("yes").unwrap_err();
        assert!(e.contains("off, no-keys or on") && e.contains("\"yes\""), "{e}");
        assert!(!Level::Off.is_on() && Level::NoKeys.is_on() && Level::All.is_on());
        assert!(!Level::Off.keys() && !Level::NoKeys.keys() && Level::All.keys());
    }

    #[test]
    fn the_author_is_told_in_plain_words_what_is_noted_and_what_is_not() {
        let all = explanation(Level::All);
        for needle in ["events.jsonl", "never put into a published course", "keys you press", "password", "no-keys"] {
            assert!(all.contains(needle), "missing {needle:?} in {all}");
        }
        assert!(all.contains("writes no letters, digits or symbols at all"), "the doubt rule is stated");
        let no_keys = explanation(Level::NoKeys);
        assert!(no_keys.contains("No keys are noted"), "{no_keys}");
        assert!(!no_keys.contains("keys you press"), "{no_keys}");
        assert!(announcement(Level::All).contains("Keys typed in password fields are never noted"));
        assert!(announcement(Level::NoKeys).contains("Keys are not."));
        assert!(announcement(Level::Off).is_empty());
        for text in [&all, &no_keys, &announcement(Level::All), &announcement(Level::NoKeys)] {
            assert!(!text.contains('\u{2014}'), "no em dashes");
        }
    }

    // ----- keys

    #[test]
    fn keys_are_named_by_the_key() {
        assert_eq!(key_name(0x41).as_deref(), Some("a"));
        assert_eq!(key_name(0x5A).as_deref(), Some("z"));
        assert_eq!(key_name(0x30).as_deref(), Some("0"));
        assert_eq!(key_name(0x39).as_deref(), Some("9"));
        assert_eq!(key_name(0x0D).as_deref(), Some("enter"));
        assert_eq!(key_name(0x1B).as_deref(), Some("escape"));
        assert_eq!(key_name(0x20).as_deref(), Some("space"));
        assert_eq!(key_name(0x70).as_deref(), Some("f1"));
        assert_eq!(key_name(0x87).as_deref(), Some("f24"));
        assert_eq!(key_name(0x60).as_deref(), Some("numpad-0"));
        assert_eq!(key_name(0x69).as_deref(), Some("numpad-9"));
        assert_eq!(key_name(0xBA).as_deref(), Some("semicolon"));
        assert_eq!(key_name(0xDE).as_deref(), Some("quote"));
        assert_eq!(key_name(0xAF), None, "the volume key is not noted");
        assert_eq!(key_name(0x10), None, "a modifier is not a key of its own");
    }

    #[test]
    fn every_key_that_has_a_name_is_told_apart_and_character_keys_are_the_ones_that_type() {
        let mut seen = std::collections::HashSet::new();
        for vk in 0..=0xFF_u32 {
            if let Some(name) = key_name(vk) {
                assert!(seen.insert(name.clone()), "two keys are both called {name}");
                assert!(!name.is_empty());
            }
        }
        assert!(is_character_key(0x41) && is_character_key(0x35) && is_character_key(0x20) && is_character_key(0xBA));
        assert!(is_character_key(0x61), "number pad digits type");
        assert!(
            !is_character_key(0x0D) && !is_character_key(0x08) && !is_character_key(0x25) && !is_character_key(0x70)
        );
        assert!(!is_character_key(0x09), "tab does not type a character");
        assert!(is_modifier_key(0x10) && is_modifier_key(0xA2) && is_modifier_key(0x5B) && !is_modifier_key(0x41));
    }

    #[test]
    fn modifiers_are_listed_in_one_fixed_order() {
        let all = Mods { ctrl: true, alt: true, shift: true, win: true };
        assert_eq!(all.names(), ["ctrl", "alt", "shift", "win"]);
        assert!(Mods::default().names().is_empty());
        assert_eq!(Mods { shift: true, ctrl: true, ..Mods::default() }.names(), ["ctrl", "shift"]);
    }

    #[test]
    fn what_has_the_keyboard_is_worked_out_from_windows_description() {
        assert_eq!(classify_focus(true, 50004), FocusKind::Password);
        assert_eq!(classify_focus(true, 50000), FocusKind::Password, "marked as a password wins over the kind");
        assert_eq!(classify_focus(false, 50004), FocusKind::Text, "edit");
        assert_eq!(classify_focus(false, 50003), FocusKind::Text, "combo box");
        assert_eq!(classify_focus(false, 50030), FocusKind::Text, "document");
        assert_eq!(classify_focus(false, 50000), FocusKind::Other, "button");
        assert_eq!(classify_focus(false, 50033), FocusKind::Other, "a pane");
    }

    #[test]
    fn controls_are_named_in_english() {
        assert_eq!(role_name(50000), "button");
        assert_eq!(role_name(50011), "menu item");
        assert_eq!(role_name(50004), "edit");
        assert_eq!(role_name(50040), "app bar");
        assert_eq!(role_name(1), "unknown");
        for id in 50000..=50040 {
            assert_ne!(role_name(id), "unknown", "{id}");
        }
    }

    #[test]
    fn characters_are_noted_only_when_the_field_is_known_to_be_an_ordinary_text_field() {
        let cases = [
            (FocusKind::Text, true),
            (FocusKind::Password, false),
            (FocusKind::Other, false),
            (FocusKind::Unknown, false),
        ];
        for (focus, written) in cases {
            let mut filter = InteractionFilter::new(true, (0, 0));
            let mut l = lookups(focus);
            let out = filter.handle(10, keydown(A), &mut l);
            assert_eq!(out.len(), usize::from(written), "{focus:?}");
            if written {
                assert_eq!(out[0], (10, key("a", &[])));
            }
        }
    }

    #[test]
    fn keys_that_cannot_spell_a_secret_are_noted_unless_the_field_is_known_to_be_a_password_field() {
        for (focus, written) in [
            (FocusKind::Text, true),
            (FocusKind::Other, true),
            (FocusKind::Unknown, true),
            (FocusKind::Password, false),
        ] {
            let mut filter = InteractionFilter::new(true, (0, 0));
            let mut l = lookups(focus);
            assert_eq!(filter.handle(5, keydown(ENTER), &mut l).len(), usize::from(written), "enter in {focus:?}");
            let ctrl_s = Raw::KeyDown { vk: S, mods: Mods { ctrl: true, ..Mods::default() } };
            let out = filter.handle(6, ctrl_s, &mut l);
            assert_eq!(out.len(), usize::from(written), "ctrl+s in {focus:?}");
            if written {
                assert_eq!(out[0].1, key("s", &["ctrl"]));
            }
        }
    }

    #[test]
    fn ctrl_and_alt_together_is_a_character_not_a_shortcut() {
        // On many keyboards AltGr is Ctrl+Alt, and it types a symbol: it must be treated like typing.
        let altgr = Raw::KeyDown { vk: 0x51, mods: Mods { ctrl: true, alt: true, ..Mods::default() } };
        let mut filter = InteractionFilter::new(true, (0, 0));
        assert!(filter.handle(1, altgr.clone(), &mut lookups(FocusKind::Unknown)).is_empty());
        let mut filter = InteractionFilter::new(true, (0, 0));
        assert_eq!(filter.handle(1, altgr, &mut lookups(FocusKind::Text)).len(), 1);
        // Shift alone is not a shortcut either.
        let shifted = Raw::KeyDown { vk: A, mods: Mods { shift: true, ..Mods::default() } };
        let mut filter = InteractionFilter::new(true, (0, 0));
        assert!(filter.handle(1, shifted, &mut lookups(FocusKind::Other)).is_empty());
        // Alt alone, Ctrl alone and the Windows key are.
        for mods in [
            Mods { alt: true, ..Mods::default() },
            Mods { ctrl: true, ..Mods::default() },
            Mods { win: true, ..Mods::default() },
        ] {
            let mut filter = InteractionFilter::new(true, (0, 0));
            let out = filter.handle(1, Raw::KeyDown { vk: A, mods }, &mut lookups(FocusKind::Unknown));
            assert_eq!(out.len(), 1, "{mods:?}");
        }
    }

    #[test]
    fn without_keys_no_key_is_noted_and_the_keyboard_is_never_looked_at() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Text);
        for vk in [A, ENTER, S] {
            assert!(filter.handle(1, keydown(vk), &mut l).is_empty());
            filter.handle(2, Raw::KeyUp { vk }, &mut l);
        }
        assert_eq!(l.focus_asked, 0, "not even asked what has the keyboard");
    }

    #[test]
    fn a_held_key_is_one_press_and_pressing_it_again_is_another() {
        let mut filter = InteractionFilter::new(true, (0, 0));
        let mut l = lookups(FocusKind::Text);
        assert_eq!(filter.handle(1, keydown(A), &mut l).len(), 1);
        for t in 2..10 {
            assert!(filter.handle(t, keydown(A), &mut l).is_empty(), "repeat at {t}");
        }
        filter.handle(10, Raw::KeyUp { vk: A }, &mut l);
        assert_eq!(filter.handle(11, keydown(A), &mut l).len(), 1);
    }

    #[test]
    fn a_missed_release_does_not_silence_a_key_once_the_window_changes() {
        let mut filter = InteractionFilter::new(true, (0, 0));
        let mut l = lookups(FocusKind::Text);
        filter.handle(0, Raw::Window(WindowState { title: "a".into(), process: "a.exe".into(), bounds: None }), &mut l);
        assert_eq!(filter.handle(1, keydown(A), &mut l).len(), 1);
        // The release never came (the key went up in another program).
        filter.handle(
            300,
            Raw::Window(WindowState { title: "b".into(), process: "b.exe".into(), bounds: None }),
            &mut l,
        );
        assert_eq!(filter.handle(301, keydown(A), &mut l).len(), 1);
    }

    #[test]
    fn modifier_keys_alone_are_not_noted_but_travel_with_the_key() {
        let mut filter = InteractionFilter::new(true, (0, 0));
        let mut l = lookups(FocusKind::Text);
        assert!(filter
            .handle(1, Raw::KeyDown { vk: 0xA2, mods: Mods { ctrl: true, ..Mods::default() } }, &mut l)
            .is_empty());
        assert_eq!(l.focus_asked, 0, "a modifier is not worth asking about");
        let out =
            filter.handle(2, Raw::KeyDown { vk: S, mods: Mods { ctrl: true, shift: true, ..Mods::default() } }, &mut l);
        assert_eq!(out[0].1, key("s", &["ctrl", "shift"]));
    }

    #[test]
    fn a_key_with_no_name_is_not_noted() {
        let mut filter = InteractionFilter::new(true, (0, 0));
        assert!(filter.handle(1, keydown(0xAF), &mut lookups(FocusKind::Text)).is_empty());
    }

    // ----- the cursor

    #[test]
    fn the_cursor_is_written_no_faster_than_every_33_ms_and_the_last_place_is_not_lost() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Unknown);
        let mut written = Vec::new();
        // The mouse reports every 10 ms for 100 ms.
        for i in 0..=10 {
            written.extend(filter.handle(i * 10, mv(i as i32, 0), &mut l));
            written.extend(filter.tick(i * 10));
        }
        assert!(written.windows(2).all(|w| w[1].0 - w[0].0 >= MOVE_EVERY_MS), "{written:?}");
        // Nothing more comes; the held-back place is written once it is due, and not before.
        written.extend(filter.tick(100));
        written.extend(filter.tick(160));
        written.extend(filter.finish());
        assert_eq!(written.last().unwrap().1, cursor(10, 0), "the last place is written");
        assert!(written.len() < 11, "far fewer lines than reports: {}", written.len());
    }

    #[test]
    fn a_held_back_place_waits_for_its_time_and_is_then_written_by_the_clock_alone() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Unknown);
        assert_eq!(filter.handle(0, mv(1, 1), &mut l), vec![(0, cursor(1, 1))]);
        assert!(filter.handle(10, mv(9, 9), &mut l).is_empty(), "too soon");
        assert!(filter.tick(20).is_empty(), "still too soon");
        assert_eq!(filter.tick(33), vec![(10, cursor(9, 9))], "stamped when it got there");
        assert!(filter.tick(500).is_empty(), "written once");
    }

    #[test]
    fn the_last_place_and_the_last_window_are_written_when_recording_ends_even_if_their_time_has_not_come() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Unknown);
        let first = WindowState { title: "a".into(), process: "a.exe".into(), bounds: None };
        let second = WindowState { title: "b".into(), process: "b.exe".into(), bounds: None };
        filter.handle(0, mv(1, 1), &mut l);
        filter.handle(0, Raw::Window(first), &mut l);
        // Moments later, before either could be written again, the cursor moves and another window comes to the front.
        assert!(filter.handle(10, mv(9, 9), &mut l).is_empty());
        assert!(filter.handle(10, Raw::Window(second), &mut l).is_empty());
        let end = filter.finish();
        assert_eq!(end.len(), 2, "{end:?}");
        assert_eq!(end[0], (10, cursor(9, 9)));
        assert!(matches!(&end[1].1, LogEvent::Window { title, .. } if title == "b"));
        assert!(filter.finish().is_empty(), "written once");
    }

    #[test]
    fn a_cursor_that_has_not_moved_writes_nothing_and_one_that_returns_before_its_turn_writes_nothing() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Unknown);
        assert_eq!(filter.handle(0, mv(5, 5), &mut l).len(), 1);
        assert!(filter.handle(100, mv(5, 5), &mut l).is_empty());
        // It wanders off and comes back before the next line was due: no change to tell.
        assert!(!filter.handle(110, mv(6, 6), &mut l).is_empty(), "after its time, a new place is written");
        assert!(filter.handle(120, mv(7, 7), &mut l).is_empty(), "too soon again: held back");
        assert!(filter.handle(125, mv(6, 6), &mut l).is_empty(), "and back where it was last written: nothing to say");
        assert!(filter.tick(300).is_empty());
        assert!(filter.finish().is_empty());
    }

    #[test]
    fn positions_are_measured_from_the_recorded_displays_top_left_corner() {
        // A second display to the right of a 1920 wide main display.
        let mut filter = InteractionFilter::new(false, (1920, 0));
        let mut l = lookups(FocusKind::Unknown);
        assert_eq!(filter.handle(0, mv(2000, 100), &mut l), vec![(0, cursor(80, 100))]);
        // The cursor on the main display is to the left of the recorded one, so it is negative.
        assert_eq!(filter.handle(100, mv(1900, 50), &mut l), vec![(100, cursor(-20, 50))]);
        let out = filter.handle(200, Raw::Click { x: 2010, y: 120, button: Button::Left }, &mut l);
        assert_eq!(out[0].1, LogEvent::Click { x: 90, y: 120, button: "left".into() });
        assert_eq!(l.points_asked, [(2010, 120)], "Windows is asked about the desktop position, not the display one");
    }

    // ----- clicks

    #[test]
    fn a_click_ends_the_cursors_path_then_names_what_was_clicked() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Unknown);
        l.element = Some(("Save".into(), "button".into()));
        filter.handle(0, mv(1, 1), &mut l);
        assert!(filter.handle(10, mv(40, 40), &mut l).is_empty());
        let out = filter.handle(20, Raw::Click { x: 50, y: 60, button: Button::Right }, &mut l);
        assert_eq!(
            out,
            vec![
                (10, cursor(40, 40)),
                (20, LogEvent::Click { x: 50, y: 60, button: "right".into() }),
                (20, LogEvent::Element { name: "Save".into(), role: "button".into() }),
            ]
        );
        // The click already said where the cursor is, so the cursor does not repeat it.
        assert!(filter.handle(30, mv(50, 60), &mut l).is_empty());
    }

    #[test]
    fn a_click_on_something_without_a_name_writes_only_the_click() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Unknown);
        l.element = Some(("  ".into(), "pane".into()));
        assert_eq!(filter.handle(5, Raw::Click { x: 1, y: 1, button: Button::Left }, &mut l).len(), 1);
        l.element = None;
        assert_eq!(filter.handle(6, Raw::Click { x: 1, y: 1, button: Button::Left }, &mut l).len(), 1);
    }

    #[test]
    fn every_button_has_a_name() {
        let names: Vec<_> = [Button::Left, Button::Right, Button::Middle, Button::Back, Button::Forward]
            .iter()
            .map(|b| b.name())
            .collect();
        assert_eq!(names, ["left", "right", "middle", "back", "forward"]);
    }

    // ----- windows

    #[test]
    fn the_window_in_front_is_written_when_it_changes_with_its_place_on_the_display() {
        let mut filter = InteractionFilter::new(false, (1920, 0));
        let mut l = lookups(FocusKind::Unknown);
        let notepad = WindowState {
            title: "Untitled - Notepad".into(),
            process: "notepad.exe".into(),
            bounds: Some((2000, 40, 800, 600)),
        };
        let out = filter.handle(0, Raw::Window(notepad.clone()), &mut l);
        assert_eq!(
            out,
            vec![(
                0,
                LogEvent::Window {
                    title: "Untitled - Notepad".into(),
                    process: "notepad.exe".into(),
                    bounds: Some(LogBounds { x: 80, y: 40, width: 800, height: 600 })
                }
            )]
        );
        assert!(filter.handle(300, Raw::Window(notepad.clone()), &mut l).is_empty(), "no change, no line");
        // Moved: a line, with the new place.
        let moved = WindowState { bounds: Some((2100, 40, 800, 600)), ..notepad.clone() };
        let out = filter.handle(600, Raw::Window(moved), &mut l);
        assert!(matches!(&out[0].1, LogEvent::Window { bounds: Some(b), .. } if b.x == 180));
        // A window that will not say where it is.
        let blind = WindowState { bounds: None, ..notepad };
        let out = filter.handle(900, Raw::Window(blind), &mut l);
        assert!(matches!(&out[0].1, LogEvent::Window { bounds: None, .. }));
    }

    #[test]
    fn a_title_that_changes_all_the_time_is_written_four_times_a_second_at_most_and_ends_on_the_last() {
        let mut filter = InteractionFilter::new(false, (0, 0));
        let mut l = lookups(FocusKind::Unknown);
        let titled =
            |n: u64| WindowState { title: format!("Copying {n} percent"), process: "x.exe".into(), bounds: None };
        let mut written = Vec::new();
        for n in 0..=100u64 {
            written.extend(filter.handle(n * 10, Raw::Window(titled(n)), &mut l));
            written.extend(filter.tick(n * 10));
        }
        written.extend(filter.finish());
        assert!(written.len() <= 6, "{} lines for a second of changes", written.len());
        assert!(matches!(&written.last().unwrap().1, LogEvent::Window { title, .. } if title == "Copying 100 percent"));
    }

    // ----- the order of the log

    #[test]
    fn the_times_in_the_log_never_go_backwards() {
        let mut filter = InteractionFilter::new(true, (0, 0));
        let mut l = lookups(FocusKind::Text);
        let mut all = Vec::new();
        all.extend(filter.handle(100, Raw::Click { x: 1, y: 1, button: Button::Left }, &mut l));
        // Something stamped earlier than a line already written (a slow lookup delayed the first).
        all.extend(filter.handle(40, keydown(A), &mut l));
        all.extend(filter.handle(90, mv(5, 5), &mut l));
        all.extend(filter.tick(200));
        all.extend(filter.finish());
        assert!(all.len() >= 3);
        assert!(all.windows(2).all(|w| w[0].0 <= w[1].0), "{all:?}");
    }

    // ----- the clock

    #[test]
    fn a_moment_is_placed_by_how_far_it_is_from_the_last_time_recording_time_was_told() {
        let t0 = Instant::now();
        let mut timeline = Timeline::new(t0);
        timeline.set(t0 + Duration::from_millis(1000), 800);
        assert_eq!(timeline.media_ms_at(t0 + Duration::from_millis(1000)), Some(800));
        assert_eq!(timeline.media_ms_at(t0 + Duration::from_millis(1030)), Some(830), "after");
        assert_eq!(timeline.media_ms_at(t0 + Duration::from_millis(970)), Some(770), "before");
        assert_eq!(timeline.media_ms_at(t0), Some(0), "before the start it is not below zero");
        assert_eq!(Timeline::new(t0).media_ms_at(t0 + Duration::from_millis(20)), Some(20));
    }

    #[test]
    fn nothing_has_a_recording_time_while_paused_and_time_carries_on_from_where_it_stopped() {
        let t0 = Instant::now();
        let mut timeline = Timeline::new(t0);
        timeline.set(t0 + Duration::from_millis(5000), 5000);
        timeline.pause();
        assert_eq!(timeline.media_ms_at(t0 + Duration::from_millis(6000)), None);
        // Resumed 20 seconds after the clock started: recording time carries on from 5000, not from 20000.
        timeline.resume(t0 + Duration::from_millis(20_000));
        assert_eq!(timeline.media_ms_at(t0 + Duration::from_millis(20_100)), Some(5100));
    }

    // ----- a take

    fn setup(keys: bool) -> InteractionSetup {
        InteractionSetup { keys, origin: (10, 20) }
    }

    #[test]
    fn a_take_starts_the_platform_with_the_setup_and_hands_over_what_it_notes_counting_each_kind() {
        let platform = FakePlatform::new();
        let mut take = InteractionTake::start(&platform, &setup(true)).unwrap();
        assert_eq!(*platform.feed.started_with.lock().unwrap(), Some((true, (10, 20))));
        platform.feed.push(10, cursor(1, 1));
        platform.feed.push(20, LogEvent::Click { x: 1, y: 1, button: "left".into() });
        platform.feed.push(20, LogEvent::Element { name: "Save".into(), role: "button".into() });
        platform.feed.push(30, key("a", &[]));
        platform.feed.push(40, LogEvent::Window { title: "t".into(), process: "p".into(), bounds: None });
        take.pump(50);
        let lines = take.take_lines();
        assert_eq!(lines.len(), 5);
        assert!(take.take_lines().is_empty(), "handed over once");
        let report = take.finish(60);
        assert_eq!(report.counts, InteractionCounts { moves: 1, clicks: 1, keys: 1, windows: 1, elements: 1 });
        assert!(report.keys);
        assert_eq!(report.recorded_ms, 60);
        assert!(platform.feed.was_stopped(), "the platform is stopped when the take ends");
    }

    #[test]
    fn lines_are_collected_when_they_are_asked_for_so_the_last_ones_are_not_left_behind() {
        let platform = FakePlatform::new();
        let mut take = InteractionTake::start(&platform, &setup(false)).unwrap();
        platform.feed.push(5, cursor(0, 0));
        // No pump in between: the session asks once more just before the take is finished.
        assert_eq!(take.take_lines(), vec![(5, cursor(0, 0))]);
        platform.feed.push(6, cursor(1, 0));
        let report = take.finish(10);
        assert_eq!(report.counts.moves, 1, "what was never handed over is not counted as written");
        assert!(!report.keys);
        assert!(platform.feed.was_stopped());
    }

    #[test]
    fn a_platform_that_cannot_start_says_why_and_the_take_does_not_begin() {
        let mut platform = FakePlatform::new();
        platform.fail = Some("Windows would not allow the hooks".into());
        match InteractionTake::start(&platform, &setup(true)) {
            Err(e) => assert!(e.contains("would not allow"), "{e}"),
            Ok(_) => panic!("started although the platform could not"),
        }
    }

    #[test]
    fn the_platforms_clock_follows_the_recording_clock_through_a_pause() {
        let platform = FakePlatform::new();
        let mut take = InteractionTake::start(&platform, &setup(true)).unwrap();
        let now = Instant::now();
        take.pump(1000);
        let at_pump = platform.feed.media_ms_at(now);
        assert!(at_pump.is_some_and(|ms| (990..=1010).contains(&ms)), "{at_pump:?}");
        take.paused();
        assert_eq!(platform.feed.media_ms_at(Instant::now()), None, "nothing is kept while paused");
        take.resumed();
        let after = platform.feed.media_ms_at(Instant::now()).unwrap();
        assert!((990..=1100).contains(&after), "recording carries on from where it stopped: {after}");
    }

    #[test]
    fn a_notice_from_the_platform_is_passed_on_once() {
        let platform = FakePlatform::new();
        let mut take = InteractionTake::start(&platform, &setup(true)).unwrap();
        platform.feed.say("interactions-no-accessibility", "no names");
        take.pump(10);
        take.pump(20);
        assert_eq!(take.take_notices(), vec![("interactions-no-accessibility".to_string(), "no names".to_string())]);
        assert!(take.take_notices().is_empty());
    }

    #[test]
    fn happenings_thrown_away_are_said_once_and_counted() {
        let platform = FakePlatform::new();
        let mut take = InteractionTake::start(&platform, &setup(true)).unwrap();
        take.pump(10);
        assert!(take.take_notices().is_empty(), "nothing lost, nothing said");
        platform.feed.set_dropped(3);
        take.pump(20);
        take.pump(30);
        platform.feed.set_dropped(7);
        take.pump(40);
        let notices = take.take_notices();
        assert_eq!(notices.len(), 1, "said once: {notices:?}");
        assert_eq!(notices[0].0, "interactions-dropped");
        assert!(notices[0].1.contains("video and the sound are not affected"));
        assert_eq!(take.finish(50).dropped, 7);
    }
}
