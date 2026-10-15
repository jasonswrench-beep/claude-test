// SPDX-License-Identifier: Apache-2.0
//! Turning an edit into a finished video: the plan.
//!
//! The editor (TypeScript) saves what the author decided as `edit.json`. Rendering has three parts:
//! working out, for every frame of the finished video, which pictures are shown where and how far the
//! zoom has moved (this crate); drawing those pictures (later, with Windows' video tools doing the
//! decoding and the encoding); and mixing the sound (the sums are here, the files are read by the
//! caller). Everything in this crate is plain arithmetic, so it is tested here without any video.
//!
//! Nothing here reads or writes a file.

mod audio;
mod canvas;
mod compose;
mod plan;
mod text;
mod zoom;

pub use audio::{gain_at, mix_block, AudioSpan, StereoFrames};
pub use canvas::{Canvas, Picture, PixelRect, Rgb, SourceRect};
pub use compose::{compose_frame, PictureSource, DEFAULT_WEBCAM_PLACE};
pub use plan::{plan, FrameRecipe, Layer, RenderPlan, RenderSettings, VideoSpan};
pub use text::{line_start, padding, wrap_lines, BlockText, TextFit, TextRenderer, TextRequest, LINE_SPACING};
pub use zoom::{zoom_view, ZoomSpan, ZoomView};
