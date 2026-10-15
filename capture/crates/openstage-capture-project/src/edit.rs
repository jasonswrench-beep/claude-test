// SPDX-License-Identifier: Apache-2.0
//! The edit decision list, `edit.json`: reading it and checking it.
//!
//! The editor (TypeScript, in `capture-edit/`) makes and changes this file; the renderer (Rust) reads
//! it. Reading is strict about what the file is and lenient about what is in it: a file that is not an
//! edit, or has a version this program does not know, is refused; a file that reads but contains
//! mistakes comes back with its problems, so the author can be told in words.
//!
//! The checks, and the codes that name them, are the same as in the TypeScript editor
//! (`capture-edit/src/validate.ts`). The files in `capture-edit/fixtures/` are shared examples that
//! both must agree on; a test here reads them. The format is described in `docs/capture-edit-format.md`.

use serde::Deserialize;
use std::collections::HashSet;

pub const EDIT_FORMAT: &str = "openstage-capture-edit";
/// The shortest a clip may be, in milliseconds.
pub const MIN_CLIP_MS: f64 = 100.0;
/// Imported video clips can be at most this long.
pub const MAX_IMPORTED_VIDEO_MS: f64 = 30_000.0;
pub const MIN_VOLUME_DB: f64 = -40.0;
pub const MAX_VOLUME_DB: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrackKind {
    Screen,
    Webcam,
    Audio,
    Media,
    Overlay,
    Zoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    Screen,
    Webcam,
    Microphone,
    SystemAudio,
    Image,
    Audio,
    Video,
}

impl TrackKind {
    fn allows(self, source: SourceKind) -> bool {
        use SourceKind as S;
        match self {
            TrackKind::Screen => source == S::Screen,
            TrackKind::Webcam => source == S::Webcam,
            TrackKind::Audio => matches!(source, S::Microphone | S::SystemAudio | S::Audio),
            TrackKind::Media => matches!(source, S::Image | S::Video | S::Audio),
            TrackKind::Overlay | TrackKind::Zoom => false,
        }
    }

    fn allows_item(self, item: &Item) -> bool {
        match self {
            TrackKind::Overlay => matches!(
                item,
                Item::Text { .. } | Item::Shape { .. } | Item::Arrow { .. } | Item::Blur { .. } | Item::Redact { .. }
            ),
            TrackKind::Zoom => matches!(item, Item::Zoom { .. }),
            _ => false,
        }
    }

    fn name(self) -> &'static str {
        match self {
            TrackKind::Screen => "screen",
            TrackKind::Webcam => "webcam",
            TrackKind::Audio => "audio",
            TrackKind::Media => "imported media",
            TrackKind::Overlay => "overlays",
            TrackKind::Zoom => "zoom",
        }
    }
}

/// Where a clip's picture or sound comes from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceRef {
    /// A file of a recording: take number and file name.
    Take { take: u32, media: String },
    /// A file from the course's assets.
    Asset { asset: String },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    #[serde(rename = "ref")]
    pub source: SourceRef,
    pub kind: SourceKind,
    pub name: String,
    /// Its natural length; `None` for a still image.
    pub duration_ms: Option<f64>,
    pub has_text_alternative: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub source: SourceRef,
    pub source_start_ms: f64,
    pub source_end_ms: f64,
    pub start_ms: f64,
    #[serde(default)]
    pub muted: bool,
    pub volume_db: f64,
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
    /// Where the picture sits in the finished video (new in 0.3), or `None` for where it is drawn by default.
    #[serde(default)]
    pub placement: Option<Rect>,
    pub alt_text: Option<String>,
    pub decorative: Option<bool>,
}

impl Clip {
    pub fn length_ms(&self) -> f64 {
        self.source_end_ms - self.source_start_ms
    }
    pub fn end_ms(&self) -> f64 {
        self.start_ms + self.length_ms()
    }
}

/// How the lines of a text overlay sit across its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// What a shape is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    #[default]
    Rectangle,
    Ellipse,
}

/// A rectangle on the picture, in ten-thousandths of its width and height from its top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// The whole width or height of the picture, in the units rectangles are measured in.
pub const FULL: f64 = 10_000.0;

impl Rect {
    fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height].iter().all(|n| n.is_finite() && n.fract() == 0.0)
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= FULL
            && self.y + self.height <= FULL
    }
}

impl Point {
    fn is_valid(&self) -> bool {
        [self.x, self.y].iter().all(|n| n.is_finite() && n.fract() == 0.0 && *n >= 0.0 && *n <= FULL)
    }
}

/// Something on an overlay or zoom track.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Item {
    #[serde(rename_all = "camelCase")]
    Text {
        id: String,
        name: String,
        start_ms: f64,
        duration_ms: f64,
        text: String,
        rect: Rect,
        font_size_bp: f64,
        color: String,
        background: Option<String>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        align: Align,
    },
    #[serde(rename_all = "camelCase")]
    Shape {
        id: String,
        name: String,
        start_ms: f64,
        duration_ms: f64,
        #[serde(default)]
        shape: ShapeKind,
        rect: Rect,
        outline: String,
        fill: Option<String>,
        line_width_bp: f64,
        alt_text: Option<String>,
        decorative: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    Arrow {
        id: String,
        name: String,
        start_ms: f64,
        duration_ms: f64,
        from: Point,
        to: Point,
        color: String,
        line_width_bp: f64,
        alt_text: Option<String>,
        decorative: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    Blur { id: String, name: String, start_ms: f64, duration_ms: f64, rect: Rect },
    #[serde(rename_all = "camelCase")]
    Redact { id: String, name: String, start_ms: f64, duration_ms: f64, rect: Rect, color: String },
    #[serde(rename_all = "camelCase")]
    Zoom {
        id: String,
        name: String,
        start_ms: f64,
        duration_ms: f64,
        rect: Rect,
        transition_ms: f64,
        description: String,
    },
}

impl Item {
    pub fn id(&self) -> &str {
        match self {
            Item::Text { id, .. }
            | Item::Shape { id, .. }
            | Item::Arrow { id, .. }
            | Item::Blur { id, .. }
            | Item::Redact { id, .. }
            | Item::Zoom { id, .. } => id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Item::Text { name, .. }
            | Item::Shape { name, .. }
            | Item::Arrow { name, .. }
            | Item::Blur { name, .. }
            | Item::Redact { name, .. }
            | Item::Zoom { name, .. } => name,
        }
    }
    pub fn start_ms(&self) -> f64 {
        match self {
            Item::Text { start_ms, .. }
            | Item::Shape { start_ms, .. }
            | Item::Arrow { start_ms, .. }
            | Item::Blur { start_ms, .. }
            | Item::Redact { start_ms, .. }
            | Item::Zoom { start_ms, .. } => *start_ms,
        }
    }
    pub fn duration_ms(&self) -> f64 {
        match self {
            Item::Text { duration_ms, .. }
            | Item::Shape { duration_ms, .. }
            | Item::Arrow { duration_ms, .. }
            | Item::Blur { duration_ms, .. }
            | Item::Redact { duration_ms, .. }
            | Item::Zoom { duration_ms, .. } => *duration_ms,
        }
    }
    pub fn end_ms(&self) -> f64 {
        self.start_ms() + self.duration_ms()
    }
    /// What it is called in a sentence.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Item::Text { .. } => "text overlay",
            Item::Shape { .. } => "shape",
            Item::Arrow { .. } => "arrow",
            Item::Blur { .. } => "blur",
            Item::Redact { .. } => "redaction",
            Item::Zoom { .. } => "zoom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    pub clips: Vec<Clip>,
    /// Overlays and zooms. A file of version 0.1 has none.
    #[serde(default)]
    pub items: Vec<Item>,
}

/// `edit.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub format: String,
    pub schema_version: String,
    pub next_id: f64,
    pub sources: Vec<SourceInfo>,
    pub tracks: Vec<Track>,
}

impl Edit {
    /// How long the whole edit is: where the last clip ends.
    pub fn length_ms(&self) -> f64 {
        self.tracks.iter().flat_map(|t| &t.clips).map(Clip::end_ms).fold(0.0, f64::max)
    }

    fn source(&self, r: &SourceRef) -> Option<&SourceInfo> {
        self.sources.iter().find(|s| &s.source == r)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The edit is not sound and must not be rendered.
    Error,
    /// The edit is sound, but something OpenStage requires is still missing.
    NeedsAttention,
    /// Sound and complete, but likely to be hard to see or read.
    Warning,
}

impl Severity {
    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::NeedsAttention => "needs-attention",
            Severity::Warning => "warning",
        }
    }
}

/// One finding about an edit, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The same names as in the TypeScript editor, such as `overlap` or `needs-alt-text`.
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub track_id: Option<String>,
    pub clip_id: Option<String>,
    pub item_id: Option<String>,
}

/// An edit that was read, and what is wrong with it.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEdit {
    pub edit: Edit,
    pub problems: Vec<Problem>,
}

impl ParsedEdit {
    /// Whether the edit can be rendered: it has no errors (things still needed are allowed here).
    pub fn is_sound(&self) -> bool {
        !self.problems.iter().any(|p| p.severity == Severity::Error)
    }
}

/// Time in words, the same wording as the editor: `72400` is "1 minute 12.4 seconds".
pub fn describe_time(ms: f64) -> String {
    let total = if ms.is_finite() && ms > 0.0 { ms.round() as u64 } else { 0 };
    let hours = total / 3_600_000;
    let minutes = total / 60_000 % 60;
    let rest = total % 60_000;
    let plural = |n: u64, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(plural(hours, "hour"));
    }
    if minutes > 0 {
        parts.push(plural(minutes, "minute"));
    }
    if rest > 0 || parts.is_empty() {
        let text = format!("{:.3}", rest as f64 / 1000.0);
        let text = text.trim_end_matches('0').trim_end_matches('.');
        let text = if text.is_empty() { "0" } else { text };
        parts.push(format!("{text} {}", if text == "1" { "second" } else { "seconds" }));
    }
    parts.join(" ")
}

fn whole(n: f64) -> bool {
    n.is_finite() && n >= 0.0 && n.fract() == 0.0
}

/// Every problem in an edit.
pub fn validate(edit: &Edit) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for track in &edit.tracks {
        if !seen.insert(&track.id) {
            out.push(Problem {
                code: "duplicate-id",
                severity: Severity::Error,
                message: format!("Two tracks have the id {}. Every id must be different.", track.id),
                track_id: Some(track.id.clone()),
                clip_id: None,
                item_id: None,
            });
        }
        check_items(track, &mut seen, &mut out);
        let mut previous: Option<&Clip> = None;
        for clip in &track.clips {
            if !seen.insert(&clip.id) {
                out.push(Problem {
                    code: "duplicate-id",
                    severity: Severity::Error,
                    message: format!("Two clips have the id {}. Every id must be different.", clip.id),
                    track_id: Some(track.id.clone()),
                    clip_id: None,
                    item_id: None,
                });
            }
            let here = |code: &'static str, severity: Severity, message: String| Problem {
                code,
                severity,
                message,
                track_id: Some(track.id.clone()),
                clip_id: Some(clip.id.clone()),
                item_id: None,
            };
            let label = format!("The clip \"{}\" on the {} track", clip.name, track.name);
            if clip.placement.is_some_and(|p| !p.is_valid()) {
                out.push(here(
                    "bad-rect",
                    Severity::Error,
                    format!("{label} has a place on the picture that is not a rectangle inside it."),
                ));
            }

            if ![clip.start_ms, clip.source_start_ms, clip.source_end_ms, clip.fade_in_ms, clip.fade_out_ms]
                .iter()
                .all(|n| whole(*n))
            {
                out.push(here(
                    "bad-time",
                    Severity::Error,
                    format!("{label} has a time that is not a whole number of milliseconds, or is below zero."),
                ));
                previous = Some(clip);
                continue;
            }
            if clip.length_ms() < MIN_CLIP_MS {
                out.push(here(
                    "too-short",
                    Severity::Error,
                    format!(
                        "{label} is {} long. A clip must be at least {}.",
                        describe_time(clip.length_ms().max(0.0)),
                        describe_time(MIN_CLIP_MS)
                    ),
                ));
            }

            match edit.source(&clip.source) {
                None => out.push(here(
                    "unknown-source",
                    Severity::Error,
                    format!("{label} uses a recording or file that this edit does not know about."),
                )),
                Some(source) => {
                    if clip.placement.is_some()
                        && matches!(source.kind, SourceKind::Microphone | SourceKind::SystemAudio | SourceKind::Audio)
                    {
                        out.push(here(
                            "placement-not-allowed",
                            Severity::Error,
                            format!("{label} holds {}, which is sound only, so it cannot have a place on the picture. Take the place away.", source.name),
                        ));
                    }
                    if !track.kind.allows(source.kind) {
                        out.push(here(
                            "source-not-allowed",
                            Severity::Error,
                            format!("{label} holds {}, which cannot go on a {} track.", source.name, track.kind.name()),
                        ));
                    }
                    if let Some(duration) = source.duration_ms {
                        if clip.source_end_ms > duration {
                            out.push(here(
                                "beyond-source",
                                Severity::Error,
                                format!(
                                    "{label} uses up to {} of {}, which is only {} long.",
                                    describe_time(clip.source_end_ms),
                                    source.name,
                                    describe_time(duration)
                                ),
                            ));
                        }
                    }
                    let is_asset = matches!(source.source, SourceRef::Asset { .. });
                    if is_asset && source.kind == SourceKind::Video && clip.length_ms() > MAX_IMPORTED_VIDEO_MS {
                        out.push(here(
                            "imported-video-too-long",
                            Severity::Error,
                            format!(
                                "{label} is {} long. Imported video clips can be at most {}.",
                                describe_time(clip.length_ms()),
                                describe_time(MAX_IMPORTED_VIDEO_MS)
                            ),
                        ));
                    }
                    if source.kind == SourceKind::Image
                        && clip.decorative != Some(true)
                        && clip.alt_text.as_deref().unwrap_or("").trim().is_empty()
                    {
                        out.push(here(
                            "needs-alt-text",
                            Severity::NeedsAttention,
                            format!("{label} is an image with no alt text. Describe it, or mark it as decorative."),
                        ));
                    }
                    if is_asset
                        && matches!(source.kind, SourceKind::Video | SourceKind::Audio)
                        && source.has_text_alternative != Some(true)
                    {
                        out.push(here(
                            "needs-text-alternative",
                            Severity::NeedsAttention,
                            if source.kind == SourceKind::Video {
                                format!("{label} is an imported video with no captions or narration script. Add one.")
                            } else {
                                format!("{label} is imported sound with no captions, script or transcript. Add one, or describe it if it is wordless music.")
                            },
                        ));
                    }
                }
            }

            if clip.fade_in_ms + clip.fade_out_ms > clip.length_ms() {
                out.push(here(
                    "fade-too-long",
                    Severity::Error,
                    format!(
                        "{label} fades in for {} and out for {}, which is longer than the clip.",
                        describe_time(clip.fade_in_ms),
                        describe_time(clip.fade_out_ms)
                    ),
                ));
            }
            if !(clip.volume_db >= MIN_VOLUME_DB && clip.volume_db <= MAX_VOLUME_DB) || clip.volume_db.fract() != 0.0 {
                out.push(here(
                    "volume-range",
                    Severity::Error,
                    format!(
                        "{label} has its volume changed by {} decibels. Use a whole number from {MIN_VOLUME_DB} to {MAX_VOLUME_DB}.",
                        clip.volume_db
                    ),
                ));
            }

            if let Some(before) = previous {
                if clip.start_ms < before.start_ms {
                    out.push(here(
                        "unsorted",
                        Severity::Error,
                        format!("{label} comes after a clip that starts later. Clips must be in order of start."),
                    ));
                } else if clip.start_ms < before.end_ms() {
                    out.push(here(
                        "overlap",
                        Severity::Error,
                        format!(
                            "{label} starts at {}, before the clip \"{}\" before it ends at {}.",
                            describe_time(clip.start_ms),
                            before.name,
                            describe_time(before.end_ms())
                        ),
                    ));
                }
            }
            previous = Some(clip);
        }
    }
    out
}

/// The most zooms that may start within one second.
const MAX_ZOOMS_PER_SECOND: usize = 3;
const VERY_SMALL_TEXT_BP: f64 = 150.0;
const LARGE_TEXT_BP: f64 = 222.0;
const LARGE_BOLD_TEXT_BP: f64 = 173.0;

fn is_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn channel(hex: &str, start: usize) -> f64 {
    let v = u8::from_str_radix(&hex[start..start + 2], 16).unwrap_or(0) as f64 / 255.0;
    if v <= 0.03928 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(color: &str) -> f64 {
    0.2126 * channel(color, 1) + 0.7152 * channel(color, 3) + 0.0722 * channel(color, 5)
}

/// The WCAG contrast ratio between two colours written "#RRGGBB".
pub fn contrast_ratio(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

fn describe_ratio(ratio: f64) -> String {
    format!("{:.1} to 1", (ratio * 10.0).floor() / 10.0)
}

fn check_items<'a>(track: &'a Track, seen: &mut HashSet<&'a str>, out: &mut Vec<Problem>) {
    let mut previous: Option<&Item> = None;
    let mut zoom_starts: Vec<f64> = Vec::new();
    for item in &track.items {
        if !seen.insert(item.id()) {
            out.push(Problem {
                code: "duplicate-id",
                severity: Severity::Error,
                message: format!("Two items have the id {}. Every id must be different.", item.id()),
                track_id: Some(track.id.clone()),
                clip_id: None,
                item_id: None,
            });
        }
        let here = |code: &'static str, severity: Severity, message: String| Problem {
            code,
            severity,
            message,
            track_id: Some(track.id.clone()),
            clip_id: None,
            item_id: Some(item.id().to_string()),
        };
        let label = format!("The {} \"{}\" on the {} track", item.kind_name(), item.name(), track.name);

        if !track.kind.allows_item(item) {
            out.push(here(
                "item-wrong-track",
                Severity::Error,
                format!("{label} cannot go on a {} track.", track.kind.name()),
            ));
        }
        if !whole(item.start_ms()) || !whole(item.duration_ms()) {
            out.push(here(
                "bad-time",
                Severity::Error,
                format!("{label} has a time that is not a whole number of milliseconds, or is below zero."),
            ));
            previous = Some(item);
            continue;
        }
        if item.duration_ms() < MIN_CLIP_MS {
            out.push(here(
                "too-short",
                Severity::Error,
                format!(
                    "{label} is {} long. It must be at least {}.",
                    describe_time(item.duration_ms()),
                    describe_time(MIN_CLIP_MS)
                ),
            ));
        }

        let colors: Vec<&str> = match item {
            Item::Text { color, background, .. } => {
                std::iter::once(color.as_str()).chain(background.as_deref()).collect()
            }
            Item::Shape { outline, fill, .. } => std::iter::once(outline.as_str()).chain(fill.as_deref()).collect(),
            Item::Arrow { color, .. } | Item::Redact { color, .. } => vec![color.as_str()],
            _ => Vec::new(),
        };
        if colors.iter().any(|c| !is_color(c)) {
            out.push(here(
                "bad-color",
                Severity::Error,
                format!("{label} has a colour that is not written like #FFFFFF."),
            ));
        }

        match item {
            Item::Arrow { from, to, .. } => {
                if !from.is_valid() || !to.is_valid() {
                    out.push(here("bad-rect", Severity::Error, format!("{label} starts or ends outside the picture.")));
                }
            }
            Item::Text { rect, .. }
            | Item::Shape { rect, .. }
            | Item::Blur { rect, .. }
            | Item::Redact { rect, .. }
            | Item::Zoom { rect, .. } => {
                if !rect.is_valid() {
                    out.push(here(
                        "bad-rect",
                        Severity::Error,
                        format!("{label} is not a rectangle inside the picture."),
                    ));
                }
            }
        }

        match item {
            Item::Text { text, font_size_bp, color, background, bold, .. } => {
                if text.trim().is_empty() {
                    out.push(here("text-empty", Severity::Error, format!("{label} has no text.")));
                }
                if font_size_bp.fract() != 0.0 || *font_size_bp < 50.0 || *font_size_bp > 3000.0 {
                    out.push(here(
                        "bad-style",
                        Severity::Error,
                        format!("{label} has a text size outside the range that can be drawn."),
                    ));
                } else if *font_size_bp < VERY_SMALL_TEXT_BP {
                    out.push(here(
                        "text-too-small",
                        Severity::Warning,
                        format!("{label} is very small; viewers may not be able to read it. Make the text larger."),
                    ));
                }
                if let Some(background) = background {
                    if is_color(color) && is_color(background) {
                        let ratio = contrast_ratio(color, background);
                        let large = *font_size_bp >= if *bold { LARGE_BOLD_TEXT_BP } else { LARGE_TEXT_BP };
                        let needed = if large { 3.0 } else { 4.5 };
                        if ratio < needed {
                            out.push(here(
                                "low-contrast",
                                Severity::Warning,
                                format!(
                                    "{label} is hard to read: its colour and its background have a contrast of {}, and {} is needed. Choose colours that differ more.",
                                    describe_ratio(ratio),
                                    describe_ratio(needed)
                                ),
                            ));
                        }
                    }
                }
            }
            Item::Shape { line_width_bp, alt_text, decorative, .. }
            | Item::Arrow { line_width_bp, alt_text, decorative, .. } => {
                if line_width_bp.fract() != 0.0 || *line_width_bp < 1.0 || *line_width_bp > 500.0 {
                    out.push(here(
                        "bad-style",
                        Severity::Error,
                        format!("{label} has a line thickness outside the range that can be drawn."),
                    ));
                }
                if *decorative != Some(true) && alt_text.as_deref().unwrap_or("").trim().is_empty() {
                    out.push(here(
                        "needs-alt-text",
                        Severity::NeedsAttention,
                        format!("{label} has no alt text. Say what it points out, or mark it as decorative."),
                    ));
                }
            }
            Item::Zoom { rect, transition_ms, description, duration_ms, .. } => {
                if transition_ms.fract() != 0.0 || *transition_ms < 0.0 || transition_ms * 2.0 > *duration_ms {
                    out.push(here(
                        "transition-too-long",
                        Severity::Error,
                        format!(
                            "{label} moves in and out for {} each, which does not fit in {}.",
                            describe_time(transition_ms.max(0.0)),
                            describe_time(*duration_ms)
                        ),
                    ));
                }
                if description.trim().is_empty() {
                    out.push(here(
                        "needs-description",
                        Severity::NeedsAttention,
                        format!("{label} has no description. Say what it shows, so the zoom is not only a picture."),
                    ));
                }
                if rect.is_valid() && (rect.width - rect.height).abs() > 100.0 {
                    out.push(here(
                        "zoom-aspect",
                        Severity::Warning,
                        format!("{label} is not the same shape as the picture, so the zoomed view will have empty bars at its sides or top and bottom."),
                    ));
                }
                zoom_starts.push(item.start_ms());
            }
            Item::Blur { .. } | Item::Redact { .. } => {}
        }

        if let Some(before) = previous {
            if item.start_ms() < before.start_ms() {
                out.push(here(
                    "unsorted",
                    Severity::Error,
                    format!("{label} comes after an item that starts later. Items must be in order of start."),
                ));
            } else if track.kind == TrackKind::Zoom && item.start_ms() < before.end_ms() {
                out.push(here(
                    "overlap",
                    Severity::Error,
                    format!(
                        "{label} starts at {}, before the zoom \"{}\" before it ends at {}.",
                        describe_time(item.start_ms()),
                        before.name(),
                        describe_time(before.end_ms())
                    ),
                ));
            }
        }
        previous = Some(item);
    }
    zoom_starts.sort_by(|a, b| a.total_cmp(b));
    for window in zoom_starts.windows(MAX_ZOOMS_PER_SECOND + 1) {
        if window[MAX_ZOOMS_PER_SECOND] - window[0] < 1000.0 {
            out.push(Problem {
                code: "too-many-zooms",
                severity: Severity::Error,
                message: format!(
                    "More than {MAX_ZOOMS_PER_SECOND} zooms start within one second, around {}. That is too fast to follow.",
                    describe_time(window[0])
                ),
                track_id: Some(track.id.clone()),
                clip_id: None,
                item_id: None,
            });
            break;
        }
    }
}

/// Read the text of an `edit.json`. `Err` says in words why it is not an edit this program can read.
pub fn parse_edit(text: &str) -> Result<ParsedEdit, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("This is not a readable edit file: {e}"))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(EDIT_FORMAT) {
        return Err(format!("This is not an OpenStage Capture edit file: its format should be \"{EDIT_FORMAT}\"."));
    }
    match value.get("schemaVersion").and_then(|v| v.as_str()) {
        Some(v) if v.starts_with("0.1.") || v.starts_with("0.2.") || v.starts_with("0.3.") => {}
        other => {
            return Err(format!(
                "This edit file is version {}, and this program reads versions 0.1, 0.2 and 0.3. It was probably made by a newer program.",
                other.unwrap_or("(missing)")
            ))
        }
    }
    let edit: Edit = serde_json::from_value(value).map_err(|e| format!("This edit file cannot be read: {e}"))?;
    let problems = validate(&edit);
    Ok(ParsedEdit { edit, problems })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../capture-edit/fixtures")
    }

    #[derive(Deserialize)]
    struct Expected {
        fixtures: BTreeMap<String, Want>,
    }
    #[derive(Deserialize)]
    struct Want {
        readable: bool,
        problems: Vec<String>,
    }

    #[test]
    fn time_is_said_in_the_same_words_as_the_editor() {
        for (ms, words) in [
            (0.0, "0 seconds"),
            (1000.0, "1 second"),
            (500.0, "0.5 seconds"),
            (12_400.0, "12.4 seconds"),
            (60_000.0, "1 minute"),
            (61_000.0, "1 minute 1 second"),
            (72_400.0, "1 minute 12.4 seconds"),
            (3_725_000.0, "1 hour 2 minutes 5 seconds"),
            (3_600_000.0, "1 hour"),
            (72_435.0, "1 minute 12.435 seconds"),
            (-100.0, "0 seconds"),
        ] {
            assert_eq!(describe_time(ms), words, "{ms}");
        }
    }

    #[test]
    fn every_shared_example_is_found_to_be_what_it_is_meant_to_show() {
        let expected: Expected =
            serde_json::from_str(&fs::read_to_string(fixtures().join("expected.json")).unwrap()).unwrap();
        let mut on_disk: Vec<String> = fs::read_dir(fixtures())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json") && n != "expected.json")
            .collect();
        on_disk.sort();
        assert_eq!(on_disk, expected.fixtures.keys().cloned().collect::<Vec<_>>(), "expected.json lists every fixture");
        assert!(expected.fixtures.len() >= 15);

        for (name, want) in &expected.fixtures {
            let text = fs::read_to_string(fixtures().join(name)).unwrap();
            match parse_edit(&text) {
                Err(message) => {
                    assert!(!want.readable, "{name} should read, but: {message}");
                    assert!(!message.is_empty());
                }
                Ok(parsed) => {
                    assert!(want.readable, "{name} should not have read");
                    let mut found: Vec<String> =
                        parsed.problems.iter().map(|p| format!("{}:{}", p.code, p.severity.word())).collect();
                    found.sort();
                    let mut wanted = want.problems.clone();
                    wanted.sort();
                    assert_eq!(found, wanted, "{name}");
                    assert_eq!(parsed.is_sound(), !wanted.iter().any(|p| p.ends_with(":error")), "{name}");
                }
            }
        }
    }

    #[test]
    fn a_plain_edit_has_the_length_of_its_longest_track_and_reads_its_tracks_and_clips() {
        let text = fs::read_to_string(fixtures().join("valid-basic.json")).unwrap();
        let parsed = parse_edit(&text).unwrap();
        assert!(parsed.is_sound() && parsed.problems.is_empty());
        assert_eq!(parsed.edit.tracks.len(), 3);
        assert_eq!(parsed.edit.tracks[0].clips.len(), 2, "the screen track was split once");
        assert_eq!(parsed.edit.length_ms(), 60_000.0);
        assert_eq!(parsed.edit.tracks[0].clips[1].start_ms, 20_000.0);
        assert_eq!(parsed.edit.tracks[0].clips[1].source_start_ms, 20_000.0);
    }

    #[test]
    fn what_is_not_an_edit_is_refused_in_words() {
        for (text, fragment) in [
            ("nonsense", "not a readable edit file"),
            ("[]", "not an OpenStage Capture edit file"),
            (r#"{"format":"openstage-capture-edit"}"#, "version (missing)"),
            (r#"{"format":"openstage-capture-edit","schemaVersion":"0.4.0"}"#, "version 0.4.0"),
            (r#"{"format":"openstage-capture-edit","schemaVersion":"0.1.0"}"#, "cannot be read"),
        ] {
            let message = parse_edit(text).unwrap_err();
            assert!(message.contains(fragment), "{text}: {message}");
        }
    }
}
