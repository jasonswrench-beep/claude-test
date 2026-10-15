// SPDX-License-Identifier: Apache-2.0
//! Recording the webcam (story A2 in `docs/prd.md`).
//!
//! The webcam goes into a file of its own, `webcam.mp4`, beside `screen.mp4` in the take's folder. It is
//! never burned into the screen video: where it appears (a small picture in a corner, full screen, or not
//! at all) is decided when the video is edited, and the author can change their mind.
//!
//! This file holds what can be checked without a camera: listing and choosing cameras, choosing which of a
//! camera's modes to use, and above all **keeping the webcam picture with the voice**. A webcam picture
//! reaches the program a tenth of a second or more after it was taken (the camera, the USB cable and
//! Windows all take their time), while a microphone's sound arrives within a few hundredths. If pictures
//! were placed in the video when they arrive, the mouth would be seen to move after the words were heard.
//! So each picture is placed where its **capture time stamp** says it was taken, on the same clock that
//! Windows uses for the sound, and the video is written a little behind the recording clock to make room
//! for that. When a camera's time stamps cannot be believed, the pictures are placed when they arrive, and
//! the summary says so.
//!
//! Choices that could go either way, for the owner to veto:
//!
//! * The camera is off unless asked for (`--camera`), because a camera is more private than a microphone.
//! * The webcam is recorded as the camera sees it, not mirrored.
//! * A camera that stops (unplugged, or taken by another program) does **not** stop the recording: the
//!   screen and sound carry on and the webcam video ends where the camera stopped.
//! * The picture is about 1280 by 720 at up to 30 a second, however good the camera is. A talking head shown
//!   small in a corner gains nothing from 4K, and 4K would cost much more processor time and memory.

use crate::report::clock;
use crate::video::{Pacer, SinkWork, VideoSink};
use crate::{Frame, FrameSource, ScreenError};
use openstage_capture::SessionError;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The file each take's webcam video goes into.
pub const WEBCAM_FILE: &str = "webcam.mp4";

/// A camera's time stamp is believed if the picture reached the program after it was taken, and not more
/// than this long after (units of 100 nanoseconds). Anything else means the camera counts time on some
/// other clock.
const MAX_BELIEVED_DELAY_100NS: i64 = 30_000_000;

/// A moment of the video is written once a picture taken at or after it has arrived, because only then is
/// the picture that was current at that moment known. If the camera sends nothing for this long, the
/// moment is written with the picture it has, so a camera that has stalled does not hold up the file.
const MAX_WAIT_100NS: i64 = 15_000_000;

/// The most pictures written to the file at one look, so that a burst after a stall cannot hold up the
/// screen and the sound for long. The rest follow at the next look, a few milliseconds later.
const MAX_WRITES_PER_LOOK: u32 = 8;

/// The most pictures kept waiting to be placed. A camera that is a second ahead of the recording clock
/// would otherwise fill memory.
const MAX_WAITING: usize = 90;

fn media_error(what: impl std::fmt::Display) -> SessionError {
    SessionError::Media(what.to_string())
}

/// One camera that can be recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraInfo {
    /// Its place in the list, from 1.
    pub number: u32,
    /// What Windows calls it, such as "NexiGo Iris 4K Webcam".
    pub name: String,
    /// Windows' own name for it (needed to open it; not shown to the author).
    pub id: String,
}

impl CameraInfo {
    /// For the author to read or hear, with the number to choose it by.
    pub fn describe(&self) -> String {
        format!("{}. {}", self.number, self.name)
    }
}

/// What a camera is sending: the size of its pictures, how many a second, and the form they come in.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraMode {
    pub width: u32,
    pub height: u32,
    /// Pictures a second, as the camera says (may have a fraction, such as 29.97).
    pub fps: f64,
    /// For example "MJPG" or "YUY2". For the author's information only.
    pub format: String,
}

impl CameraMode {
    pub fn describe(&self) -> String {
        format!("{} by {} at {} pictures a second ({})", self.width, self.height, self.fps.round(), self.format)
    }
}

/// A camera that has been started.
pub struct StartedCamera {
    /// Its pictures: blue, green, red, alpha, rows top to bottom, with the time each was taken (`captured_100ns`)
    /// and the time it reached the program (`arrived_100ns`) when the camera says them.
    pub source: Box<dyn FrameSource>,
    pub mode: CameraMode,
}

/// Everything the webcam needs from the operating system.
pub trait CameraPlatform {
    /// The cameras this computer has, numbered from 1.
    fn list_cameras(&self) -> Result<Vec<CameraInfo>, ScreenError>;
    /// Open a camera and start its pictures coming.
    fn start_camera(&self, camera: &CameraInfo) -> Result<StartedCamera, ScreenError>;
}

/// The real thing on Windows; on any other system a stand-in that says so.
pub fn system_camera_platform() -> Box<dyn CameraPlatform> {
    #[cfg(windows)]
    {
        Box::new(crate::win_camera::WindowsCameras)
    }
    #[cfg(not(windows))]
    {
        Box::new(NoCameras)
    }
}

#[cfg(not(windows))]
struct NoCameras;

#[cfg(not(windows))]
impl CameraPlatform for NoCameras {
    fn list_cameras(&self) -> Result<Vec<CameraInfo>, ScreenError> {
        Err(ScreenError::Failed("Cameras can only be used on Windows.".to_string()))
    }
    fn start_camera(&self, _: &CameraInfo) -> Result<StartedCamera, ScreenError> {
        Err(ScreenError::Failed("Cameras can only be used on Windows.".to_string()))
    }
}

/// Number a list of cameras from 1 in the order given.
pub fn number_cameras(mut cameras: Vec<CameraInfo>) -> Vec<CameraInfo> {
    for (i, camera) in cameras.iter_mut().enumerate() {
        camera.number = i as u32 + 1;
    }
    cameras
}

fn list_text(cameras: &[CameraInfo]) -> String {
    cameras.iter().map(CameraInfo::describe).collect::<Vec<_>>().join("; ")
}

/// The cameras written out in words.
pub fn describe_cameras(cameras: &[CameraInfo]) -> String {
    match cameras {
        [] => "No camera was found. If one is plugged in, check that Windows lists it (Settings, Bluetooth & devices, Cameras) and that no other program is using it.".to_string(),
        [one] => format!("One camera was found: {}.", one.describe()),
        many => format!("{} cameras were found: {}.", many.len(), list_text(many)),
    }
}

/// Choose a camera by its number, or by part of its name (capital letters do not matter). `default` is the
/// first in the list. Says in words what is wrong if there is none, or more than one, and lists the ones that
/// matched.
pub fn choose_camera<'a>(cameras: &'a [CameraInfo], choice: &str) -> Result<&'a CameraInfo, String> {
    let choice = choice.trim();
    if cameras.is_empty() {
        return Err(describe_cameras(cameras));
    }
    if choice.eq_ignore_ascii_case("default") {
        return Ok(&cameras[0]);
    }
    if choice.is_empty() {
        return Err(format!("Say which camera: its number, or part of its name. {}", describe_cameras(cameras)));
    }
    if let Ok(number) = choice.parse::<u32>() {
        return cameras.iter().find(|c| c.number == number).ok_or_else(|| {
            format!(
                "There is no camera number {number}. Choose a number from 1 to {}: {}",
                cameras.len(),
                list_text(cameras)
            )
        });
    }
    let wanted = choice.to_lowercase();
    let matches: Vec<&CameraInfo> = cameras.iter().filter(|c| c.name.to_lowercase().contains(&wanted)).collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("No camera has \"{choice}\" in its name. {}", describe_cameras(cameras))),
        many => Err(format!(
            "More than one camera has \"{choice}\" in its name: {}. Use a number, or more of the name.",
            many.iter().map(|c| c.describe()).collect::<Vec<_>>().join("; ")
        )),
    }
}

/// How a camera's pictures are stored while they travel, as far as it matters for choosing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureForm {
    /// Uncompressed (NV12, YUY2): cheap to convert, but a lot of data for a USB cable.
    Raw,
    /// Compressed picture by picture (MJPG): needs a decoder, but fits a USB cable at 30 a second.
    Mjpeg,
    /// Anything else.
    Other,
}

/// One mode a camera offers.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeOption {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub form: PictureForm,
}

/// The size wanted for a webcam picture, and the rate.
const WANTED_WIDTH: u32 = 1280;
const WANTED_HEIGHT: u32 = 720;

/// Which of a camera's modes to use: the one closest to 1280 by 720 at 30 a second. A rate that is too slow
/// matters more than a size that is a little off, and a mode with an odd side is never chosen (H.264 needs
/// even sides). `None` if the camera offers nothing usable.
pub fn pick_mode(options: &[ModeOption]) -> Option<usize> {
    let wanted_area = (WANTED_WIDTH * WANTED_HEIGHT) as f64;
    options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.width % 2 == 0 && o.height % 2 == 0 && o.width >= 320 && o.height >= 180 && o.fps >= 5.0)
        .map(|(i, o)| {
            let size = ((o.width * o.height) as f64 / wanted_area).log2().abs();
            let rate = if o.fps >= 29.0 {
                0.0
            } else if o.fps >= 24.0 {
                0.5
            } else if o.fps >= 15.0 {
                2.0
            } else {
                4.0
            };
            let form = match o.form {
                PictureForm::Raw => 0.0,
                PictureForm::Mjpeg => 0.1,
                PictureForm::Other => 0.5,
            };
            (i, size + rate + form)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// What the webcam did during a take, for the summary.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WebcamReport {
    pub camera: String,
    pub width: u32,
    pub height: u32,
    /// Pictures a second in the file.
    pub fps: u32,
    /// Moments of the video written (the file's pictures).
    pub pictures_written: u64,
    /// Of those, how many showed the same camera picture as the one before (the camera was slower than the file).
    pub pictures_repeated: u64,
    /// Pictures the camera delivered while recording.
    pub pictures_received: u64,
    /// Pictures that were the wrong size and were left out.
    pub pictures_wrong_size: u64,
    /// Pictures that were thrown away because the program was too slow to take them.
    pub pictures_lost: u64,
    /// The longest time between two pictures from the camera, in milliseconds.
    pub longest_gap_ms: u64,
    /// How many pictures had a time stamp that could be believed, and how many did not.
    pub stamped: u64,
    pub unstamped: u64,
    /// How long after being taken the pictures reached the program, in milliseconds (pictures with a time stamp only).
    pub delay_average_ms: u64,
    pub delay_most_ms: u64,
    /// If the camera stopped before the recording ended: when (recording time, milliseconds) and why.
    pub stopped: Option<(u64, String)>,
    /// How long the recording ran.
    pub recorded_ms: u64,
    /// Where the time of handing pictures to the video writer went.
    pub work: SinkWork,
}

/// A camera picture with the moment it belongs to, in recording time (units of 100 nanoseconds; may be
/// before zero for a picture taken before recording began).
struct Placed {
    place: i64,
    frame: Frame,
}

/// The webcam during one take: takes the camera's pictures, puts each where it was taken, and writes the
/// steady-rate video.
pub struct WebcamTake {
    camera: String,
    source: Box<dyn FrameSource>,
    sink: Option<Box<dyn VideoSink>>,
    pacer: Pacer,
    fps: u32,
    size: (u32, u32),
    /// The camera's clock now, in units of 100 nanoseconds (the clock Windows stamps pictures and sound with).
    clock: Rc<dyn Fn() -> i64>,
    /// The camera-clock time at which recording time was zero. Found from the first look, then moved earlier
    /// if a later look shows it was too late (a look delayed by the computer being busy).
    anchor: Option<i64>,
    /// Waiting to be placed in the video, oldest first.
    waiting: VecDeque<Placed>,
    /// The newest picture taken at or before the moment being written.
    held: Option<Placed>,
    /// Where the picture last written was taken, to tell a repeat from a new picture.
    last_written_place: Option<i64>,
    next_beat: u64,
    last_media_ms: u64,
    /// Where the last picture received was taken (for the longest gap).
    last_place: Option<i64>,
    /// How long to wait, when recording stops or pauses, for the pictures taken in its last moments to arrive.
    settle: Duration,
    /// An error found where it could not be returned, to be returned at the next look.
    deferred: Option<SessionError>,
    /// Set when the camera has stopped: nothing is written after this moment (recording time, 100 ns).
    end_at: Option<i64>,
    stopped: Option<(u64, String)>,
    notices: Vec<(String, String)>,
    // Counts for the report.
    written: u64,
    repeated: u64,
    received: u64,
    wrong_size: u64,
    lost_at_start: u64,
    /// Pictures the source threw away during pauses, when nobody was taking them: not the program's slowness.
    lost_in_pauses: u64,
    lost_at_pause: Option<u64>,
    longest_gap: i64,
    stamped: u64,
    unstamped: u64,
    delay_total: i64,
    delay_most: i64,
}

impl WebcamTake {
    /// `first` is the picture the camera gave while starting; it fixes the size of the video.
    pub fn new(
        camera: String,
        source: Box<dyn FrameSource>,
        sink: Box<dyn VideoSink>,
        first: Frame,
        fps: u32,
        clock: Rc<dyn Fn() -> i64>,
    ) -> Self {
        let size = (first.width, first.height);
        let lost_at_start = source.dropped();
        Self {
            camera,
            source,
            sink: Some(sink),
            pacer: Pacer::new(fps),
            fps: fps.max(1),
            size,
            clock,
            anchor: None,
            waiting: VecDeque::new(),
            // Before any picture of the recording is placed, this one stands in for the camera's view.
            held: Some(Placed { place: i64::MIN, frame: first }),
            last_written_place: None,
            next_beat: 0,
            last_media_ms: 0,
            last_place: None,
            settle: Duration::ZERO,
            deferred: None,
            end_at: None,
            stopped: None,
            notices: Vec::new(),
            written: 0,
            repeated: 0,
            received: 0,
            wrong_size: 0,
            lost_at_start,
            lost_in_pauses: 0,
            lost_at_pause: None,
            longest_gap: 0,
            stamped: 0,
            unstamped: 0,
            delay_total: 0,
            delay_most: 0,
        }
    }

    /// When recording stops or pauses, wait up to this long for the pictures taken in its last moments to arrive
    /// (they are a tenth of a second or more on their way), so that the end of the video is not made of older
    /// pictures. Nothing is waited for if the camera has stopped.
    pub fn with_settle(mut self, settle: Duration) -> Self {
        self.settle = settle;
        self
    }

    /// Wait for the first picture taken after `media_ms` to arrive, up to the settle time. Every picture taken
    /// before it has arrived by then, because a camera sends in order.
    fn settle_at(&mut self, media_ms: u64) {
        if self.settle.is_zero() || self.stopped.is_some() {
            return;
        }
        let Some(anchor) = self.anchor else { return };
        let wanted = media_ms as i64 * 10_000;
        let deadline = Instant::now() + self.settle;
        while self.waiting.back().is_none_or(|p| p.place <= wanted) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || self.source.ended().is_some() {
                break;
            }
            if let Some(frame) = self.source.next_frame(left.min(Duration::from_millis(20))) {
                let now = (self.clock)();
                self.take_in(frame, now, anchor);
            }
        }
    }

    /// Take what the camera has sent, up to `media_ms` of recording time, and write the video as far as it is
    /// known.
    pub fn pump(&mut self, media_ms: u64) -> Result<(), SessionError> {
        if let Some(e) = self.deferred.take() {
            return Err(e);
        }
        self.last_media_ms = media_ms;
        let now = (self.clock)();
        let candidate = now - media_ms as i64 * 10_000;
        let anchor = *self.anchor.get_or_insert(candidate);
        let anchor = anchor.min(candidate);
        self.anchor = Some(anchor);
        while let Some(frame) = self.source.next_frame(Duration::ZERO) {
            self.take_in(frame, now, anchor);
        }
        self.check_stopped(media_ms);
        self.write_due(media_ms, false)
    }

    /// Note whether the camera has stopped sending for good.
    fn check_stopped(&mut self, media_ms: u64) {
        if self.stopped.is_some() {
            return;
        }
        if let Some(why) = self.source.ended() {
            let newest = self.waiting.back().map(|p| p.place).or(self.held.as_ref().map(|h| h.place)).unwrap_or(0);
            self.end_at = Some(newest.max(0));
            self.notices.push((
                "webcam-stopped".to_string(),
                format!(
                    "The webcam stopped at {} of the recording: {why} The screen and the sound are still being recorded. The webcam video ends there.",
                    clock(media_ms)
                ),
            ));
            self.stopped = Some((media_ms, why));
        }
    }

    fn take_in(&mut self, frame: Frame, now: i64, anchor: i64) {
        self.received += 1;
        if (frame.width, frame.height) != self.size {
            self.wrong_size += 1;
            self.source.recycle(frame);
            return;
        }
        let arrived = frame.arrived_100ns.unwrap_or(now);
        let (at, believed) = match frame.captured_100ns {
            Some(taken) if arrived >= taken && arrived - taken <= MAX_BELIEVED_DELAY_100NS => (taken, true),
            _ => (arrived, false),
        };
        if believed {
            let delay = arrived - at;
            self.stamped += 1;
            self.delay_total += delay;
            self.delay_most = self.delay_most.max(delay);
        } else {
            self.unstamped += 1;
        }
        let place = at - anchor;
        if let Some(previous) = self.last_place {
            self.longest_gap = self.longest_gap.max(place - previous);
        }
        self.last_place = Some(place);
        // Cameras send in order, but keep the line in order whatever happens.
        let at_index = self.waiting.iter().rposition(|p| p.place <= place).map_or(0, |i| i + 1);
        self.waiting.insert(at_index, Placed { place, frame });
        while self.waiting.len() > MAX_WAITING {
            if let Some(old) = self.waiting.pop_front() {
                self.source.recycle(old.frame);
            }
        }
    }

    /// Write every moment of the video that is known, oldest first. With `finishing`, write all the moments up to
    /// the end of the recording with the pictures there are.
    fn write_due(&mut self, media_ms: u64, finishing: bool) -> Result<(), SessionError> {
        let mut limit = media_ms as i64 * 10_000;
        if let Some(end) = self.end_at {
            limit = limit.min(end);
        }
        let all_known = finishing || self.end_at.is_some();
        let mut wrote = 0;
        while wrote < MAX_WRITES_PER_LOOK {
            let moment = self.pacer.pts_100ns(self.next_beat);
            if moment > limit {
                break;
            }
            let later_picture_here = self.waiting.back().is_some_and(|p| p.place > moment);
            let waited_long = limit >= moment + MAX_WAIT_100NS;
            if !(later_picture_here || waited_long || all_known) {
                break;
            }
            // The newest picture taken at or before this moment.
            while self.waiting.front().is_some_and(|p| p.place <= moment) {
                let Some(next) = self.waiting.pop_front() else { break };
                match self.held.as_ref() {
                    Some(held) if held.place > next.place => self.source.recycle(next.frame),
                    _ => {
                        if let Some(old) = self.held.replace(next) {
                            self.source.recycle(old.frame);
                        }
                    }
                }
            }
            let Some(held) = self.held.as_ref() else { break };
            let duration = self.pacer.duration_100ns(self.next_beat);
            let sink = self.sink.as_mut().expect("the webcam file is open until it is finished");
            sink.write(&held.frame, moment, duration)
                .map_err(|e| media_error(format!("Writing the webcam video stopped with a problem: {e}")))?;
            if self.last_written_place == Some(held.place) {
                self.repeated += 1;
            }
            self.last_written_place = Some(held.place);
            self.written += 1;
            self.next_beat += 1;
            wrote += 1;
        }
        Ok(())
    }

    /// Recording was paused. Finish the video up to this moment; what the camera sees from here to the resume
    /// has no place in the recording.
    pub fn paused(&mut self) {
        let media_ms = self.last_media_ms;
        // Take whatever has arrived, and wait a moment for the rest, so the last moments before the pause use
        // the pictures taken then.
        let now = (self.clock)();
        if let Some(anchor) = self.anchor {
            while let Some(frame) = self.source.next_frame(Duration::ZERO) {
                self.take_in(frame, now, anchor);
            }
        }
        self.settle_at(media_ms);
        if let Err(e) = self.write_due(media_ms, true) {
            self.deferred = Some(e);
        }
        self.forget_waiting();
        // From here until the resume nobody takes the camera's pictures, so the ones it throws away are not counted.
        self.lost_at_pause = Some(self.source.dropped());
    }

    /// Recording carries on. Pictures taken during the pause are thrown away, except the newest, which stands
    /// in for the camera's view until the first new picture arrives. The recording clock is found again,
    /// because the pause left it behind.
    pub fn resumed(&mut self) {
        let mut newest = None;
        while let Some(frame) = self.source.next_frame(Duration::ZERO) {
            if (frame.width, frame.height) == self.size {
                if let Some(old) = newest.replace(frame) {
                    self.source.recycle(old);
                }
            } else {
                self.source.recycle(frame);
            }
        }
        if let Some(frame) = newest {
            if let Some(old) = self.held.replace(Placed { place: i64::MIN, frame }) {
                self.source.recycle(old.frame);
            }
        }
        self.forget_waiting();
        self.count_pause_losses();
        self.anchor = None;
        self.last_place = None;
        self.last_written_place = None;
    }

    fn count_pause_losses(&mut self) {
        if let Some(at) = self.lost_at_pause.take() {
            self.lost_in_pauses += self.source.dropped().saturating_sub(at);
        }
    }

    fn forget_waiting(&mut self) {
        while let Some(old) = self.waiting.pop_front() {
            self.source.recycle(old.frame);
        }
    }

    /// Anything the author should be told about now, as (code, message), once.
    pub fn notices(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.notices)
    }

    /// Write the last moments, complete the file, and say what happened.
    pub fn finish(mut self, media_ms: u64) -> Result<WebcamReport, SessionError> {
        if let Some(e) = self.deferred.take() {
            return Err(e);
        }
        self.last_media_ms = media_ms;
        let now = (self.clock)();
        let candidate = now - media_ms as i64 * 10_000;
        let anchor = self.anchor.map_or(candidate, |a| a.min(candidate));
        self.anchor = Some(anchor);
        while let Some(frame) = self.source.next_frame(Duration::ZERO) {
            self.take_in(frame, now, anchor);
        }
        self.settle_at(media_ms);
        self.check_stopped(media_ms);
        // Everything up to the end is written, even if a limit of eight at a time has to be repeated.
        loop {
            let before = self.next_beat;
            self.write_due(media_ms, true)?;
            if self.next_beat == before {
                break;
            }
        }
        self.count_pause_losses();
        let lost = self.source.dropped().saturating_sub(self.lost_at_start).saturating_sub(self.lost_in_pauses);
        let sink = self.sink.take().expect("the webcam file is open until it is finished");
        let work = sink.work();
        sink.finish().map_err(|e| media_error(format!("The webcam video file could not be completed: {e}")))?;
        let average = if self.stamped == 0 { 0 } else { self.delay_total / self.stamped as i64 / 10_000 };
        Ok(WebcamReport {
            camera: self.camera.clone(),
            width: self.size.0,
            height: self.size.1,
            fps: self.fps,
            pictures_written: self.written,
            pictures_repeated: self.repeated,
            pictures_received: self.received,
            pictures_wrong_size: self.wrong_size,
            pictures_lost: lost,
            longest_gap_ms: (self.longest_gap.max(0) / 10_000) as u64,
            stamped: self.stamped,
            unstamped: self.unstamped,
            delay_average_ms: average.max(0) as u64,
            delay_most_ms: (self.delay_most.max(0) / 10_000) as u64,
            stopped: self.stopped.clone(),
            recorded_ms: media_ms,
            work,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    // ---- fakes ----

    /// A camera whose pictures the test hands over by hand.
    struct Camera {
        line: Rc<RefCell<VecDeque<Frame>>>,
        /// Pictures still on their way: they arrive while the program waits for them.
        in_flight: Rc<RefCell<VecDeque<Frame>>>,
        ended: Rc<RefCell<Option<String>>>,
        recycled: Rc<Cell<u32>>,
        lost: Rc<Cell<u64>>,
    }
    impl FrameSource for Camera {
        fn next_frame(&mut self, timeout: Duration) -> Option<Frame> {
            if let Some(frame) = self.line.borrow_mut().pop_front() {
                return Some(frame);
            }
            if timeout.is_zero() {
                return None;
            }
            self.in_flight.borrow_mut().pop_front()
        }
        fn dropped(&self) -> u64 {
            self.lost.get()
        }
        fn recycle(&mut self, _: Frame) {
            self.recycled.set(self.recycled.get() + 1);
        }
        fn ended(&self) -> Option<String> {
            self.ended.borrow().clone()
        }
    }

    /// What was written to the fake file: (when, how long, which picture) for every moment.
    #[derive(Default)]
    struct File {
        moments: Vec<(i64, i64, u8)>,
        finished: bool,
    }
    struct Sink(Rc<RefCell<File>>);
    impl VideoSink for Sink {
        fn write(&mut self, frame: &Frame, pts_100ns: i64, duration_100ns: i64) -> Result<(), ScreenError> {
            self.0.borrow_mut().moments.push((pts_100ns, duration_100ns, frame.bgra[0]));
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<(), ScreenError> {
            self.0.borrow_mut().finished = true;
            Ok(())
        }
    }

    fn picture(id: u8, taken: Option<i64>, arrived: Option<i64>) -> Frame {
        Frame { width: 4, height: 2, t_ms: 0, captured_100ns: taken, arrived_100ns: arrived, bgra: vec![id; 4 * 2 * 4] }
    }

    type Line = Rc<RefCell<VecDeque<Frame>>>;

    struct Rig {
        take: Option<WebcamTake>,
        line: Line,
        in_flight: Line,
        ended: Rc<RefCell<Option<String>>>,
        file: Rc<RefCell<File>>,
        now: Rc<Cell<i64>>,
        recycled: Rc<Cell<u32>>,
        lost: Rc<Cell<u64>>,
    }

    /// The camera clock reads the same as recording time plus `OFFSET`, so the two clocks differ the way real ones do.
    const OFFSET: i64 = 7_000_000_000;

    fn new_rig(fps: u32) -> Rig {
        rig_with(fps, 0, Duration::ZERO)
    }

    /// A rig whose camera had already lost `lost_at_start` pictures when the take began, and which waits `settle`
    /// for the last pictures at the end.
    fn rig_with(fps: u32, lost_at_start: u64, settle: Duration) -> Rig {
        let line: Line = Rc::new(RefCell::new(VecDeque::new()));
        let in_flight: Line = Rc::new(RefCell::new(VecDeque::new()));
        let ended = Rc::new(RefCell::new(None));
        let file = Rc::new(RefCell::new(File::default()));
        let now = Rc::new(Cell::new(OFFSET));
        let recycled = Rc::new(Cell::new(0));
        let lost = Rc::new(Cell::new(lost_at_start));
        let camera = Camera {
            line: Rc::clone(&line),
            in_flight: Rc::clone(&in_flight),
            ended: Rc::clone(&ended),
            recycled: Rc::clone(&recycled),
            lost: Rc::clone(&lost),
        };
        let clock_now = Rc::clone(&now);
        let take = WebcamTake::new(
            "Test Camera".into(),
            Box::new(camera),
            Box::new(Sink(Rc::clone(&file))),
            picture(255, None, None),
            fps,
            Rc::new(move || clock_now.get()),
        )
        .with_settle(settle);
        Rig { take: Some(take), line, in_flight, ended, file, now, recycled, lost }
    }

    impl Rig {
        fn take(&mut self) -> &mut WebcamTake {
            self.take.as_mut().unwrap()
        }
        /// Let recording time pass from `from_ms` to `to_ms` in steps of 5 ms (the program looks every 5 ms), calling
        /// `deliver` first at each step with the recording time in milliseconds so it can make pictures arrive.
        fn run(&mut self, from_ms: u64, to_ms: u64, mut deliver: impl FnMut(u64, &Line)) {
            let mut at = from_ms;
            while at <= to_ms {
                self.now.set(OFFSET + at as i64 * 10_000);
                deliver(at, &self.line);
                self.take().pump(at).unwrap();
                at += 5;
            }
        }
    }

    /// The time picture `n` of a camera taking `fps` pictures a second is taken, in milliseconds.
    fn taken_ms(n: u64, fps: u64) -> u64 {
        n * 1000 / fps
    }

    /// A camera taking `fps` pictures a second, each reaching the program `delay_ms` after it was taken. Picture
    /// number n has the number n as its pixels, and is taken at n / fps seconds.
    fn steady_camera(fps: u64, delay_ms: u64) -> impl FnMut(u64, &Line) {
        let mut next = 0u64;
        move |at_ms, line| {
            // A picture reaches the program when its delay has passed since it was taken.
            while taken_ms(next, fps) + delay_ms <= at_ms {
                let taken = OFFSET + taken_ms(next, fps) as i64 * 10_000;
                let arrived = OFFSET + (taken_ms(next, fps) + delay_ms) as i64 * 10_000;
                line.borrow_mut().push_back(picture(next as u8, Some(taken), Some(arrived)));
                next += 1;
            }
        }
    }

    /// How many moments at the start of the video can show the picture taken at that moment, when the recording is
    /// `end_ms` long and pictures take `delay_ms` to arrive: the later ones are written before their pictures
    /// have arrived. (Waiting for them at the end is tested separately.)
    fn fresh_moments(end_ms: u64, delay_ms: u64, fps: u64) -> usize {
        ((end_ms - delay_ms) * fps / 1000) as usize
    }

    // ---- placing pictures where they were taken ----

    #[test]
    fn a_picture_is_placed_where_it_was_taken_not_when_it_arrived() {
        // Pictures arrive 200 ms late. Moment n of the video must still show the picture taken at moment n.
        let mut rig = new_rig(30);
        rig.run(0, 2000, steady_camera(30, 200));
        let report = rig.take.take().unwrap().finish(2000).unwrap();
        let file = rig.file.borrow();
        assert!(file.finished);
        for (moment, (pts, _, shown)) in file.moments.iter().enumerate() {
            assert_eq!(*pts, moment as i64 * 10_000_000 / 30, "moments follow one another with no gap");
            if moment < fresh_moments(2000, 200, 30) {
                assert_eq!(*shown as usize, moment, "moment {moment} shows the picture taken then, not a stale one");
            }
        }
        assert_eq!(file.moments.len() as u64, report.pictures_written);
        assert!(report.pictures_written >= 60, "{report:?}");
        assert_eq!((report.delay_average_ms, report.delay_most_ms), (200, 200));
        assert_eq!((report.stamped, report.unstamped), (report.pictures_received, 0));
    }

    #[test]
    fn without_placing_by_the_time_taken_the_picture_would_be_late_which_the_test_would_notice() {
        // The same camera, but the take is told every picture was taken when it arrived (no stamps). Moment n
        // then shows a picture that was taken 200 ms (6 moments) earlier. This is what placing by time taken avoids.
        let mut rig = new_rig(30);
        let mut steady = steady_camera(30, 200);
        rig.run(0, 2000, |at, line| {
            steady(at, line);
            for frame in line.borrow_mut().iter_mut() {
                frame.captured_100ns = None;
            }
        });
        let _ = rig.take.take().unwrap().finish(2000).unwrap();
        let file = rig.file.borrow();
        let late =
            file.moments.iter().enumerate().skip(20).take(20).all(|(m, (_, _, shown))| (*shown as usize) + 5 <= m);
        assert!(late, "moments show pictures about 6 moments old: {:?}", &file.moments[20..30]);
    }

    #[test]
    fn the_file_is_as_long_as_the_recording_even_though_it_is_written_behind_the_clock() {
        let mut rig = new_rig(30);
        rig.run(0, 3000, steady_camera(30, 150));
        let report = rig.take.take().unwrap().finish(3000).unwrap();
        // Moments 0 to 90 inclusive: the last is exactly at 3 seconds.
        assert_eq!(report.pictures_written, 91, "{report:?}");
        let file = rig.file.borrow();
        let last = file.moments.last().unwrap();
        assert_eq!(last.0, 90 * 10_000_000 / 30);
    }

    #[test]
    fn writing_stays_behind_the_clock_by_about_the_cameras_delay_while_recording() {
        let mut rig = new_rig(30);
        rig.run(0, 1000, steady_camera(30, 200));
        // A 200 ms delay means moments after about 800 ms cannot be known yet, so are not written yet.
        let written = rig.file.borrow().moments.len();
        assert!((20..=26).contains(&written), "{written} moments written after 1 s");
    }

    #[test]
    fn at_the_end_the_program_waits_for_the_last_pictures_so_the_video_does_not_end_on_old_ones() {
        let mut rig = rig_with(30, 0, Duration::from_millis(400));
        rig.run(0, 1000, steady_camera(30, 200));
        // The pictures taken after 800 ms are still on their way when recording stops at 1000 ms.
        for n in 25..45u64 {
            let taken = OFFSET + taken_ms(n, 30) as i64 * 10_000;
            let arrived = OFFSET + (taken_ms(n, 30) + 200) as i64 * 10_000;
            rig.in_flight.borrow_mut().push_back(picture(n as u8, Some(taken), Some(arrived)));
        }
        let _ = rig.take.take().unwrap().finish(1000).unwrap();
        let file = rig.file.borrow();
        assert_eq!(file.moments.len(), 31);
        for (moment, (_, _, shown)) in file.moments.iter().enumerate() {
            assert_eq!(*shown as usize, moment, "even the last moment {moment} shows the picture taken then");
        }
    }

    #[test]
    fn the_wait_at_the_end_is_over_as_soon_as_a_later_picture_is_there_and_never_longer_than_allowed() {
        // A camera with nothing more to send: the wait ends at the allowed time, not never.
        let mut rig = rig_with(30, 0, Duration::from_millis(60));
        rig.run(0, 500, steady_camera(30, 50));
        let started = Instant::now();
        let _ = rig.take.take().unwrap().finish(500).unwrap();
        assert!(started.elapsed() < Duration::from_millis(1500), "waited {:?}", started.elapsed());
    }

    #[test]
    fn pictures_with_stamps_that_cannot_be_believed_are_placed_when_they_arrive() {
        let mut rig = new_rig(30);
        // The camera counts from zero on its own clock: its stamps are nowhere near the program's.
        rig.run(0, 1500, |at, line| {
            if at % 33 == 0 {
                let n = (at / 33) as u8;
                line.borrow_mut().push_back(picture(n, Some(at as i64 * 10_000), Some(OFFSET + at as i64 * 10_000)));
            }
        });
        let report = rig.take.take().unwrap().finish(1500).unwrap();
        assert_eq!(report.stamped, 0);
        assert_eq!(report.unstamped, report.pictures_received);
        assert!(report.pictures_written >= 44, "{report:?}");
        // A picture with no stamp at all is placed on arrival too.
        let mut rig2 = new_rig(30);
        rig2.run(0, 1000, |at, line| {
            if at % 33 == 0 {
                line.borrow_mut().push_back(picture(1, None, Some(OFFSET + at as i64 * 10_000)));
            }
        });
        let report2 = rig2.take.take().unwrap().finish(1000).unwrap();
        assert_eq!(report2.stamped, 0);
        assert!(report2.pictures_received > 0);
    }

    #[test]
    fn a_picture_that_seems_to_arrive_before_it_was_taken_is_not_believed() {
        let mut rig = new_rig(30);
        rig.run(0, 1000, |at, line| {
            if at % 33 == 0 {
                let arrived = OFFSET + at as i64 * 10_000;
                line.borrow_mut().push_back(picture(1, Some(arrived + 5_000_000), Some(arrived)));
            }
        });
        let report = rig.take.take().unwrap().finish(1000).unwrap();
        assert_eq!(report.stamped, 0, "{report:?}");
    }

    #[test]
    fn a_look_that_came_late_does_not_move_every_picture_late() {
        // The first look comes 40 ms after the recording clock said 0 ... then looks come on time. The
        // clock found from the first look is corrected by the later ones.
        let mut rig = new_rig(30);
        rig.now.set(OFFSET + 40 * 10_000);
        rig.take().pump(0).unwrap();
        rig.run(5, 1500, steady_camera(30, 100));
        let report = rig.take.take().unwrap().finish(1500).unwrap();
        let file = rig.file.borrow();
        for (moment, (_, _, shown)) in file.moments.iter().enumerate().take(fresh_moments(1500, 100, 30)) {
            assert_eq!(*shown as usize, moment, "moment {moment}");
        }
        assert!(report.pictures_written >= 45);
    }

    // ---- stalls and a camera slower than the file ----

    #[test]
    fn a_slow_camera_repeats_its_pictures_and_says_so() {
        let mut rig = new_rig(30);
        rig.run(0, 2000, steady_camera(10, 100));
        let report = rig.take.take().unwrap().finish(2000).unwrap();
        assert!(report.pictures_repeated >= report.pictures_written / 2, "{report:?}");
        assert!(report.longest_gap_ms >= 99 && report.longest_gap_ms <= 101, "{report:?}");
        // The file still has a moment for every 1/30 s.
        assert_eq!(report.pictures_written, 61);
    }

    #[test]
    fn a_camera_that_stalls_for_a_while_does_not_hold_up_the_file() {
        let mut rig = new_rig(30);
        // 1 second of pictures, then nothing for 3 seconds, then pictures again.
        let mut steady = steady_camera(30, 100);
        let thrown_away: Line = Rc::new(RefCell::new(VecDeque::new()));
        rig.run(0, 5000, |at, line| {
            if (1000..4000).contains(&at) {
                // The camera is not sending, but the maker of pictures stays in step with the time.
                steady(at, &thrown_away);
                thrown_away.borrow_mut().clear();
            } else {
                steady(at, line);
            }
        });
        let written_before_end = rig.file.borrow().moments.len();
        assert!(written_before_end > 100, "the file kept growing through the stall: {written_before_end}");
        let report = rig.take.take().unwrap().finish(5000).unwrap();
        assert_eq!(report.pictures_written, 151);
        assert!(report.longest_gap_ms >= 3000, "the stall is reported: {report:?}");
        assert!(report.pictures_repeated > 80);
    }

    #[test]
    fn a_burst_after_a_stall_is_written_in_small_steps() {
        let mut rig = new_rig(30);
        // 40 pictures all arriving at once, 3 seconds after the recording began.
        rig.now.set(OFFSET + 3_000 * 10_000);
        for n in 0..40u64 {
            let taken = OFFSET + taken_ms(n, 30) as i64 * 10_000;
            rig.line.borrow_mut().push_back(picture(n as u8, Some(taken), Some(OFFSET + 2_900 * 10_000)));
        }
        let before = rig.file.borrow().moments.len();
        rig.take().pump(3_000).unwrap();
        let after = rig.file.borrow().moments.len();
        assert!(after - before <= MAX_WRITES_PER_LOOK as usize, "one look wrote {} moments", after - before);
        rig.run(3_005, 3_200, |_, _| {});
        assert!(rig.file.borrow().moments.len() > after, "the rest followed");
    }

    // ---- pause and resume ----

    #[test]
    fn nothing_taken_during_a_pause_is_in_the_file_and_the_file_has_no_gap() {
        let mut rig = new_rig(30);
        // Picture n is taken at wall-clock time n / 30 s and arrives 100 ms later. Recording runs from wall time 0
        // to 1 s, is paused until wall time 6 s, and then runs 1 s more, so recording time is wall time less 5 s.
        let mut camera = steady_camera(30, 100);
        rig.run(0, 1000, &mut camera);
        rig.take().paused();
        assert_eq!(rig.file.borrow().moments.len(), 31, "every moment up to the pause is written");
        // The camera keeps sending while recording time stands still.
        for wall in 1000..=6000u64 {
            camera(wall, &rig.line);
        }
        rig.now.set(OFFSET + 6000 * 10_000);
        rig.take().resumed();
        for step in 0..=200u64 {
            let media = 1000 + step * 5;
            let wall = 6000 + step * 5;
            rig.now.set(OFFSET + wall as i64 * 10_000);
            camera(wall, &rig.line);
            rig.take().pump(media).unwrap();
        }
        let report = rig.take.take().unwrap().finish(2000).unwrap();
        let file = rig.file.borrow();
        for (moment, (pts, _, _)) in file.moments.iter().enumerate() {
            assert_eq!(*pts, moment as i64 * 10_000_000 / 30, "no gap and no overlap at moment {moment}");
        }
        assert_eq!(report.pictures_written, 61, "{report:?}");
        // Before the pause: the picture taken then, as far as pictures had arrived (those taken in the last 100 ms
        // before the pause were still on their way, and no wait was asked for here).
        for moment in 0..=27usize {
            assert_eq!(file.moments[moment].2 as usize, moment);
        }
        assert!(file.moments[28..=30].iter().all(|m| m.2 == 27), "{:?}", &file.moments[26..32]);
        // After the resume every moment shows the picture taken then: recording time is wall time less 5 seconds, which
        // is 150 pictures, so moment m shows picture m + 150. The pictures taken in the last moments of the pause
        // (still on their way when it ended) are placed at those moments, not at the moment they arrived, and none
        // from the middle of the pause is shown.
        for moment in 31..=57usize {
            assert_eq!(file.moments[moment].2 as usize, moment + 150, "moment {moment}");
        }
    }

    #[test]
    fn at_a_pause_the_program_waits_for_the_last_pictures_too_so_the_moments_before_it_are_not_stale() {
        let mut rig = rig_with(30, 0, Duration::from_millis(400));
        rig.run(0, 1000, steady_camera(30, 200));
        for n in 25..45u64 {
            let taken = OFFSET + taken_ms(n, 30) as i64 * 10_000;
            let arrived = OFFSET + (taken_ms(n, 30) + 200) as i64 * 10_000;
            rig.in_flight.borrow_mut().push_back(picture(n as u8, Some(taken), Some(arrived)));
        }
        rig.take().paused();
        let file = rig.file.borrow();
        assert_eq!(file.moments.len(), 31, "every moment up to the pause is written at the pause");
        for (moment, (_, _, shown)) in file.moments.iter().enumerate() {
            assert_eq!(*shown as usize, moment, "moment {moment} shows the picture taken then");
        }
    }

    #[test]
    fn a_video_file_that_cannot_be_completed_is_an_error_with_a_plain_message() {
        struct Unfinishable;
        impl VideoSink for Unfinishable {
            fn write(&mut self, _: &Frame, _: i64, _: i64) -> Result<(), ScreenError> {
                Ok(())
            }
            fn finish(self: Box<Self>) -> Result<(), ScreenError> {
                Err(ScreenError::Failed("the drive was removed".into()))
            }
        }
        let camera = Camera {
            line: Rc::new(RefCell::new(VecDeque::new())),
            in_flight: Rc::new(RefCell::new(VecDeque::new())),
            ended: Rc::new(RefCell::new(None)),
            recycled: Rc::new(Cell::new(0)),
            lost: Rc::new(Cell::new(0)),
        };
        let take = WebcamTake::new(
            "x".into(),
            Box::new(camera),
            Box::new(Unfinishable),
            picture(1, None, None),
            30,
            Rc::new(|| OFFSET),
        );
        let e = take.finish(500).unwrap_err().to_string();
        assert!(e.contains("webcam video file could not be completed") && e.contains("the drive was removed"), "{e}");
    }

    #[test]
    fn a_pause_straight_after_starting_still_finishes_the_file() {
        let mut rig = new_rig(30);
        rig.run(0, 500, steady_camera(30, 50));
        rig.take().paused();
        rig.take().resumed();
        let report = rig.take.take().unwrap().finish(500).unwrap();
        assert!(report.pictures_written >= 15);
        assert!(rig.file.borrow().finished);
    }

    // ---- a camera that stops ----

    #[test]
    fn a_camera_that_stops_ends_the_webcam_video_and_says_so_once_while_recording_goes_on() {
        let mut rig = new_rig(30);
        let mut camera = steady_camera(30, 100);
        let ended = Rc::clone(&rig.ended);
        rig.run(0, 3000, |at, line| {
            // The camera is unplugged at 1 s: nothing arrives after the pictures taken by then.
            if at == 1000 {
                *ended.borrow_mut() = Some("The camera was unplugged.".to_string());
            }
            if at < 1000 {
                camera(at, line);
            }
        });
        let notices = rig.take().notices();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert_eq!(notices[0].0, "webcam-stopped");
        assert!(notices[0].1.contains("The camera was unplugged.") && notices[0].1.contains("still being recorded"));
        assert!(notices[0].1.contains(&clock(1000)), "{}", notices[0].1);
        assert!(rig.take().notices().is_empty(), "said once");
        let report = rig.take.take().unwrap().finish(3000).unwrap();
        assert_eq!(report.stopped.as_ref().map(|s| s.0), Some(1000));
        let n = report.pictures_written;
        assert!((25..=31).contains(&n), "the video ends where the camera stopped: {n} moments");
        assert!(rig.file.borrow().finished);
    }

    // ---- pictures that cannot be used ----

    #[test]
    fn a_picture_of_a_different_size_is_left_out_and_counted() {
        let mut rig = new_rig(30);
        rig.run(0, 300, |at, line| {
            if at == 100 {
                let mut odd = picture(9, Some(OFFSET + 1_000_000), Some(OFFSET + 1_100_000));
                odd.width = 8;
                line.borrow_mut().push_back(odd);
            }
        });
        let report = rig.take.take().unwrap().finish(300).unwrap();
        assert_eq!(report.pictures_wrong_size, 1);
        assert!(rig.recycled.get() >= 1, "its memory was given back");
        assert!(rig.file.borrow().moments.iter().all(|m| m.2 != 9));
    }

    #[test]
    fn pictures_the_program_was_too_slow_to_take_are_counted_from_the_start_of_the_take() {
        // 5 were lost while the countdown ran; 3 more while recording.
        let mut rig = rig_with(30, 5, Duration::ZERO);
        rig.lost.set(8);
        rig.run(0, 100, |_, _| {});
        let report = rig.take.take().unwrap().finish(100).unwrap();
        assert_eq!(report.pictures_lost, 3, "{report:?}");
    }

    #[test]
    fn pictures_thrown_away_during_a_pause_are_not_counted_as_the_programs_slowness() {
        let mut rig = new_rig(30);
        rig.run(0, 500, steady_camera(30, 50));
        rig.take().paused();
        rig.lost.set(rig.lost.get() + 50); // nobody was taking pictures during the pause
        rig.take().resumed();
        rig.run(500, 800, |_, _| {});
        rig.lost.set(rig.lost.get() + 2); // two lost while recording
        let report = rig.take.take().unwrap().finish(800).unwrap();
        assert_eq!(report.pictures_lost, 2, "{report:?}");

        // Stopping while paused counts the same way.
        let mut rig = new_rig(30);
        rig.run(0, 500, steady_camera(30, 50));
        rig.take().paused();
        rig.lost.set(rig.lost.get() + 40);
        let report = rig.take.take().unwrap().finish(500).unwrap();
        assert_eq!(report.pictures_lost, 0, "{report:?}");
    }

    #[test]
    fn pictures_waiting_to_be_placed_never_grow_without_limit() {
        let mut rig = new_rig(30);
        // Pictures taken far ahead of the recording clock would wait for ever.
        rig.run(0, 100, |at, line| {
            let taken = OFFSET + (at as i64 + 500_000) * 10_000;
            line.borrow_mut().push_back(picture(1, Some(taken), Some(taken + 10_000)));
        });
        assert!(rig.take().waiting.len() <= MAX_WAITING);
    }

    #[test]
    fn a_failing_disk_ends_the_take_with_a_plain_message() {
        struct Failing;
        impl VideoSink for Failing {
            fn write(&mut self, _: &Frame, _: i64, _: i64) -> Result<(), ScreenError> {
                Err(ScreenError::Failed("disk full".into()))
            }
            fn finish(self: Box<Self>) -> Result<(), ScreenError> {
                Ok(())
            }
        }
        let camera = Camera {
            line: Rc::new(RefCell::new(VecDeque::new())),
            in_flight: Rc::new(RefCell::new(VecDeque::new())),
            ended: Rc::new(RefCell::new(None)),
            recycled: Rc::new(Cell::new(0)),
            lost: Rc::new(Cell::new(0)),
        };
        let mut take = WebcamTake::new(
            "x".into(),
            Box::new(camera),
            Box::new(Failing),
            picture(1, None, None),
            30,
            Rc::new(|| OFFSET),
        );
        // After two seconds with no picture the first moments are written with the picture there is.
        let e = take.pump(2000).unwrap_err().to_string();
        assert!(e.contains("webcam video stopped with a problem") && e.contains("disk full"), "{e}");
    }

    // ---- choosing a camera and a mode ----

    fn cameras() -> Vec<CameraInfo> {
        number_cameras(vec![
            CameraInfo { number: 0, name: "NexiGo Iris 4K Webcam".into(), id: "a".into() },
            CameraInfo { number: 0, name: "Integrated Camera".into(), id: "b".into() },
            CameraInfo { number: 0, name: "OBS Virtual Camera".into(), id: "c".into() },
        ])
    }

    #[test]
    fn cameras_are_numbered_and_described_in_words() {
        let list = cameras();
        assert_eq!(list[2].describe(), "3. OBS Virtual Camera");
        assert_eq!(
            describe_cameras(&list),
            "3 cameras were found: 1. NexiGo Iris 4K Webcam; 2. Integrated Camera; 3. OBS Virtual Camera."
        );
        assert_eq!(describe_cameras(&list[..1]), "One camera was found: 1. NexiGo Iris 4K Webcam.");
        assert!(describe_cameras(&[]).starts_with("No camera was found."));
    }

    #[test]
    fn a_camera_is_chosen_by_number_or_by_part_of_its_name() {
        let list = cameras();
        assert_eq!(choose_camera(&list, "2").unwrap().name, "Integrated Camera");
        assert_eq!(choose_camera(&list, "nexigo").unwrap().number, 1);
        assert_eq!(choose_camera(&list, "default").unwrap().number, 1);
        assert_eq!(choose_camera(&list, " Default ").unwrap().number, 1);
        let e = choose_camera(&list, "9").unwrap_err();
        assert!(e.contains("There is no camera number 9. Choose a number from 1 to 3"), "{e}");
        let e = choose_camera(&list, "usb").unwrap_err();
        assert!(e.contains("No camera has \"usb\" in its name") && e.contains("3 cameras were found"), "{e}");
        let e = choose_camera(&list, "camera").unwrap_err();
        assert!(
            e.contains("More than one camera") && e.contains("2. Integrated Camera") && e.contains("3. OBS"),
            "{e}"
        );
        assert!(choose_camera(&list, "").unwrap_err().contains("Say which camera"));
        assert!(choose_camera(&[], "1").unwrap_err().starts_with("No camera was found."));
    }

    fn mode(width: u32, height: u32, fps: f64, form: PictureForm) -> ModeOption {
        ModeOption { width, height, fps, form }
    }

    #[test]
    fn the_mode_closest_to_720p_at_30_is_chosen() {
        let modes = [
            mode(3840, 2160, 30.0, PictureForm::Mjpeg),
            mode(1920, 1080, 30.0, PictureForm::Mjpeg),
            mode(1280, 720, 30.0, PictureForm::Mjpeg),
            mode(1280, 720, 10.0, PictureForm::Raw),
            mode(640, 480, 30.0, PictureForm::Raw),
        ];
        assert_eq!(pick_mode(&modes), Some(2));
    }

    #[test]
    fn a_fast_enough_mode_beats_a_better_sized_slow_one() {
        // 720p raw is only 10 a second over this cable; 480p raw is 30. Smooth motion matters more.
        let modes = [mode(1280, 720, 10.0, PictureForm::Raw), mode(640, 480, 30.0, PictureForm::Raw)];
        assert_eq!(pick_mode(&modes), Some(1));
    }

    #[test]
    fn raw_pictures_win_a_tie_and_odd_or_tiny_sizes_are_never_chosen() {
        let modes = [mode(1280, 720, 30.0, PictureForm::Mjpeg), mode(1280, 720, 30.0, PictureForm::Raw)];
        assert_eq!(pick_mode(&modes), Some(1));
        assert_eq!(pick_mode(&[mode(1281, 721, 30.0, PictureForm::Raw)]), None);
        assert_eq!(pick_mode(&[mode(160, 120, 30.0, PictureForm::Raw)]), None);
        assert_eq!(pick_mode(&[mode(1280, 720, 2.0, PictureForm::Raw)]), None);
        assert_eq!(pick_mode(&[]), None);
        let fraction = [mode(1280, 720, 29.97, PictureForm::Raw), mode(1280, 720, 24.0, PictureForm::Raw)];
        assert_eq!(pick_mode(&fraction), Some(0), "29.97 counts as 30");
    }

    #[test]
    fn a_mode_is_described_in_words() {
        let m = CameraMode { width: 1280, height: 720, fps: 29.97, format: "MJPG".into() };
        assert_eq!(m.describe(), "1280 by 720 at 30 pictures a second (MJPG)");
    }
}
