// SPDX-License-Identifier: Apache-2.0
//! What a recording did, in numbers a person can read.
//!
//! After a long recording the questions are: did the computer keep up, did the program grow without
//! limit, and did sound and picture stay together? The recorder keeps a few counters while it works
//! ([`RecordingReport`]) and [`summarize`] turns them into findings in plain words.

use crate::interactions::InteractionsReport;
use crate::timing::MAX_OFFSET_MS;
use crate::webcam::WebcamReport;
use crate::{CaptureWork, SinkWork};
use std::time::Duration;

/// The share of looks that may be over the 40 ms line before sound counts as out of step with the picture.
const MAX_SHARE_OVER: f64 = 0.01;
use crate::{check, Check, CPU_TARGET_PERCENT};

/// How one sound track behaved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackReport {
    /// True for the microphone, false for the computer's own sound.
    pub microphone: bool,
    /// How much sound the sound card really delivered, in milliseconds.
    pub heard_ms: u64,
    /// Silence filled in where nothing arrived, in milliseconds.
    pub padded_ms: u64,
    /// Sound dropped because the sound card ran ahead of the computer, in milliseconds.
    pub trimmed_ms: u64,
    /// At worst, the sound was held back this long, so it played this much later than its picture, and when.
    pub worst_later_ms: u64,
    pub worst_later_at_ms: u64,
    /// At worst, the sound was this far short when it was due, so it played this much earlier than its
    /// picture, and when. Only meaningful for a source that sends sound all the time (a microphone): a
    /// quiet computer sends nothing, which looks the same.
    pub worst_earlier_ms: u64,
    pub worst_earlier_at_ms: u64,
    /// How many times the sound was placed, and on how many of them it was more than 40 ms later (or,
    /// for a microphone, missing by more than 40 ms). Zero when not counted, in which case only the
    /// worst moment is judged.
    pub looks: u64,
    pub looks_over_later: u64,
    pub looks_over_earlier: u64,
    /// How many times Windows reported lost sound.
    pub lost_moments: u64,
    /// Each time Windows moved the computer's sound to another output and the recording followed it.
    pub output_moves: Vec<OutputMove>,
    /// Why the recording could not follow the sound to another output, if it could not.
    pub follow_problem: Option<String>,
}

/// One time the recording followed the computer's sound to another sound output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputMove {
    /// When, in recording time, in milliseconds.
    pub at_ms: u64,
    /// The output it was listening to, and the one it listened to from then on.
    pub from: String,
    pub to: String,
}

/// Where the time of the picture work went during a recording. Times are wall-clock, so a step that
/// waits (for the graphics card, say) counts its waiting too; they are upper limits on processor time.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PictureWork {
    /// Bringing pictures from the graphics card into memory (on Windows' own threads).
    pub capture: CaptureWork,
    /// Handing pictures to the video writer (on the recording thread).
    pub sink: SinkWork,
    /// The processor time the recording thread used altogether, if the system can say.
    pub loop_cpu: Option<Duration>,
}

/// Counters from one recording.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordingReport {
    pub recorded_ms: u64,
    pub pictures_written: u64,
    /// Beats of the steady frame rate that came and went without a fresh picture because the program was late.
    pub beats_skipped: u64,
    /// Pictures that Windows' capture service had to throw away because the program was not taking them fast enough.
    pub pictures_replaced: u64,
    /// How many pictures Windows delivered while recording, and the most that were waiting at one look
    /// (the waiting line holds 8). Zero when the source does not count.
    pub pictures_received: u64,
    pub most_waiting: usize,
    /// Real time the recording took, in seconds, and the processor time the program used meanwhile.
    pub wall_seconds: f64,
    pub cpu_seconds: Option<f64>,
    pub logical_cores: u32,
    pub memory_first_mb: Option<f64>,
    pub memory_peak_mb: Option<f64>,
    pub tracks: Vec<TrackReport>,
    /// How regularly the program looked at the screen and the sound while recording.
    pub looks: LookStats,
    /// Where the picture work's time went.
    pub work: PictureWork,
    /// What the webcam did, when one was recorded.
    pub webcam: Option<WebcamReport>,
    /// What was noted of the author's mouse and keyboard, when that was turned on.
    pub interactions: Option<InteractionsReport>,
}

/// One moment when the program went a long time without looking at the screen and the sound.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlowMoment {
    /// When, in recording time, in milliseconds.
    pub at_ms: u64,
    /// How long it went without looking.
    pub gap_ms: u64,
    /// How long the look just before took for the picture, and for the sound.
    pub picture_ms: u64,
    pub sound_ms: u64,
    /// Of the sound's time, how long writing sound into the video file took.
    pub sound_video_ms: u64,
    /// How long saving to disk took in the gap.
    pub saving_ms: u64,
}

/// How regularly the program looked at the screen and the sound. It needs to look at least every 33
/// milliseconds to keep a steady 30 pictures a second.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LookStats {
    pub looks: u64,
    pub average_gap_ms: f64,
    pub longest_gap_ms: u64,
    pub gaps_over_50_ms: u64,
    pub gaps_over_100_ms: u64,
    /// The worst few, longest first.
    pub slow_moments: Vec<SlowMoment>,
}

pub(crate) fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Milliseconds of `time` for every second of the recording.
fn per_second_ms(time: Duration, seconds: f64) -> f64 {
    time.as_secs_f64() * 1000.0 / seconds
}

/// A few sentences on where the processor time went, to add to the processor finding. Empty when the
/// recording measured nothing of it.
fn where_the_time_went(r: &RecordingReport, cpu_seconds: f64) -> String {
    let (c, s) = (&r.work.capture, &r.work.sink);
    if c.pictures == 0 && s.pictures == 0 {
        return String::new();
    }
    let secs = r.wall_seconds.max(0.001);
    let ms = |t: Duration| per_second_ms(t, secs);
    let a_second = |n: u64| n as f64 / secs;
    let capture_total = c.memory + c.readback + c.rows;
    let sink_total = s.copy + s.call;
    let mut text = format!(
        " Where the picture work went, for every second of the recording (steps that wait count their waiting, so these are upper limits): bringing pictures from the graphics card into memory took {:.0} ms ({:.0} pictures a second, {:.1} ms each; {:.0} ms of it getting memory, {:.0} ms the graphics card's copy and the wait for it, {:.0} ms copying rows; {} of {} pictures used memory that an earlier picture had used)",
        ms(capture_total),
        a_second(c.pictures),
        if c.pictures == 0 { 0.0 } else { capture_total.as_secs_f64() * 1000.0 / c.pictures as f64 },
        ms(c.memory),
        ms(c.readback),
        ms(c.rows),
        c.reused,
        c.pictures
    );
    text += &format!(
        "; handing pictures to the video writer took {:.0} ms ({:.0} pictures a second, {:.1} ms each; {:.0} ms copying each into the writer's own memory, {:.0} ms the writer's own work, which includes converting the colours)",
        ms(sink_total),
        a_second(s.pictures),
        if s.pictures == 0 { 0.0 } else { sink_total.as_secs_f64() * 1000.0 / s.pictures as f64 },
        ms(s.copy),
        ms(s.call)
    );
    // The recording thread's own processor time includes the writer's work; the capture work is on other threads.
    let accounted = capture_total + r.work.loop_cpu.unwrap_or(sink_total);
    if let Some(loop_cpu) = r.work.loop_cpu {
        text += &format!("; the recording thread itself used {:.0} ms of processor time", ms(loop_cpu));
    }
    let total_ms = cpu_seconds * 1000.0 / secs;
    let rest = (total_ms - ms(accounted)).max(0.0);
    text += &format!(
        ". The program used {total_ms:.0} ms of processor time a second in all, so at least {rest:.0} ms of it is outside these steps: Windows' own video threads, the graphics driver and the sound."
    );
    text
}

/// The finding about the noting of what the author did, in plain words. It says how many of each kind were noted, so a
/// person who moved the mouse and clicked can see that it was heard, and it fails only when nothing at all was noted.
fn interactions_finding(i: &InteractionsReport) -> Check {
    let c = &i.counts;
    let total = c.moves + c.clicks + c.keys + c.windows + c.elements;
    let ok = total > 0;
    let mut text = format!(
        "Noted what you did for {}: {} cursor positions, {} clicks, {} clicked item names, {} window changes and {}.",
        clock(i.recorded_ms),
        c.moves,
        c.clicks,
        c.elements,
        c.windows,
        if i.keys { format!("{} keys", c.keys) } else { "no keys (you chose to leave them out)".to_string() }
    );
    if !ok {
        text += " Nothing was noted at all, so Windows did not tell the recorder what the mouse and keyboard did. Please tell me.";
    }
    if i.keys && c.keys == 0 && c.clicks + c.moves > 0 {
        text += " No keys were noted. If you typed, the recorder could not be sure the keyboard was in an ordinary text field (it writes no letters, digits or symbols when it cannot tell), or the program you typed in does not describe its fields to Windows.";
    }
    if c.clicks > 0 && c.elements == 0 {
        text +=
            " No clicked item had a name that Windows would give, so the log says where you clicked but not on what.";
    }
    if i.dropped > 0 {
        text += &format!(" {} happenings were not noted because Windows was too slow to answer.", i.dropped);
    }
    check("run-interactions", ok, text)
}

/// The finding about the webcam, in plain words.
fn webcam_finding(w: &WebcamReport) -> Check {
    let fps = w.fps.max(1) as u64;
    // A moment of the video for every 1/fps of the recording, counting the one at the very start.
    let expected = w.recorded_ms * fps / 1000 + 1;
    let seconds = (w.recorded_ms as f64 / 1000.0).max(0.001);
    let camera_rate = w.pictures_received as f64 / seconds;
    let complete = w.stopped.is_some() || w.pictures_written * 100 >= expected * 98;
    let steady = w.stopped.is_some() || w.longest_gap_ms <= 1000;
    let ok = complete && steady && w.stopped.is_none();
    let mut text = format!(
        "The webcam ({}) was recorded at {} by {}, {} pictures a second: {} pictures for {} of recording. The camera sent {} pictures, about {:.0} a second{}; the longest wait between two was {} ms.",
        w.camera,
        w.width,
        w.height,
        w.fps,
        w.pictures_written,
        clock(w.recorded_ms),
        w.pictures_received,
        camera_rate,
        if camera_rate < w.fps as f64 * 0.8 && w.stopped.is_none() {
            format!(" (fewer than the {} asked for: cameras do this in dim light, and the video repeats pictures to fill the gaps)", w.fps)
        } else {
            String::new()
        },
        w.longest_gap_ms
    );
    if w.stamped > 0 {
        text += &format!(
            " Each picture was placed where its camera time stamp says it was taken, which is what keeps the webcam with your voice. It reached the program {} ms after it was taken on average ({} ms at most).",
            w.delay_average_ms, w.delay_most_ms
        );
        if w.unstamped > 0 {
            text += &format!(
                " {} pictures had no time stamp that could be used and were placed when they arrived.",
                w.unstamped
            );
        }
    } else if w.pictures_received > 0 {
        text += " This camera's time stamps could not be used, so each picture was placed when it arrived. A webcam picture is on its way for a tenth of a second or more, so it may appear that much later than your voice. Please tell me, and I will add a way to correct it.";
    }
    if w.pictures_repeated * 10 > w.pictures_written && w.stopped.is_none() {
        text += &format!(
            " {} of the {} pictures in the video repeat the one before, because the camera sent fewer than the video holds.",
            w.pictures_repeated, w.pictures_written
        );
    }
    if w.pictures_lost > 0 {
        text += &format!(" The program was too slow to take {} camera pictures.", w.pictures_lost);
    }
    if w.pictures_wrong_size > 0 {
        text += &format!(" {} pictures of another size were left out.", w.pictures_wrong_size);
    }
    if let Some((at, why)) = &w.stopped {
        text += &format!(
            " The camera stopped at {}: {why} The webcam video ends there; the screen and the sound carried on.",
            clock(*at)
        );
    } else if !complete {
        text += &format!(
            " Only {} of the {} pictures a video of this length should hold were written, so the webcam video has gaps.",
            w.pictures_written, expected
        );
    } else if !steady {
        text +=
            " The camera went more than a second without sending a picture, so the webcam picture froze for that time.";
    }
    check("run-webcam", ok, text)
}

/// The findings for a recording, in plain words.
pub fn summarize(r: &RecordingReport) -> Vec<Check> {
    let mut out = Vec::new();

    // Did the program keep up with the screen?
    let beats = r.pictures_written + r.beats_skipped;
    let skipped_percent = if beats == 0 { 0.0 } else { r.beats_skipped as f64 * 100.0 / beats as f64 };
    out.push(check(
        "run-summary",
        skipped_percent <= 1.0,
        format!(
            "Recorded {}. {} pictures were written. {} steps of the steady 30 pictures a second ({skipped_percent:.1} percent) had to be skipped because the program was late, and Windows threw away {} pictures that were not taken in time while recording{}.{}{}",
            clock(r.recorded_ms),
            r.pictures_written,
            r.beats_skipped,
            r.pictures_replaced,
            if r.pictures_received > 0 {
                format!(
                    " (it delivered {} in all, about {:.0} a second, and at one look it took as many as {} at once; the waiting line holds 8, and more can arrive while it is emptied)",
                    r.pictures_received,
                    r.pictures_received as f64 * 1000.0 / r.recorded_ms.max(1) as f64,
                    r.most_waiting
                )
            } else {
                String::new()
            },
            if skipped_percent > 1.0 { " More than 1 percent skipped means the video may look jerky." } else { "" },
            if skipped_percent > 1.0 && r.looks.looks > 0 {
                format!(
                    " The program looked at the screen every {:.0} ms on average (it must be at least every 33 ms), and {} of the {} looks were more than 50 ms apart.",
                    r.looks.average_gap_ms, r.looks.gaps_over_50_ms, r.looks.looks
                )
            } else {
                String::new()
            }
        ),
    ));

    // Did the program ever go quiet for a long moment?
    if r.looks.looks > 0 {
        let minutes = (r.recorded_ms as f64 / 60_000.0).max(1.0);
        let calm = r.looks.longest_gap_ms <= 250 && r.looks.gaps_over_100_ms as f64 <= minutes;
        let worst: Vec<String> = r
            .looks
            .slow_moments
            .iter()
            .take(3)
            .map(|m| {
                format!(
                    "{} ms at {} (the picture took {} ms and the sound {} ms just before, of which {} ms was writing sound into the video file; saving to disk took {} ms; the rest was waiting or other work)",
                    m.gap_ms,
                    clock(m.at_ms),
                    m.picture_ms,
                    m.sound_ms,
                    m.sound_video_ms,
                    m.saving_ms
                )
            })
            .collect();
        out.push(check(
            "run-stalls",
            calm,
            format!(
                "The program went more than 100 ms without looking at the screen {} time(s) (more than 50 ms: {}). The longest wait was {} ms.{}{}",
                r.looks.gaps_over_100_ms,
                r.looks.gaps_over_50_ms,
                r.looks.longest_gap_ms,
                if worst.is_empty() { String::new() } else { format!(" The worst: {}.", worst.join("; ")) },
                if calm { "" } else { " That is more than one long wait a minute, or one longer than 250 ms, which is what makes video jerky." }
            ),
        ));
    }

    // How hard did the computer work?
    if let Some(cpu) = r.cpu_seconds.filter(|_| r.wall_seconds > 0.0) {
        let core_percent = cpu * 100.0 / r.wall_seconds;
        let cores = r.logical_cores.max(1) as f64;
        let machine_percent = core_percent / cores;
        let laptop_percent = core_percent / 4.0;
        out.push(check(
            "run-cpu",
            machine_percent <= CPU_TARGET_PERCENT,
            format!(
                "While recording, the program used about {core_percent:.0} percent of one processor core on average. That is {machine_percent:.1} percent of this computer's {} cores. If a computer with 4 cores had to do the same work, it would be about {laptop_percent:.1} percent, against a target of {CPU_TARGET_PERCENT:.0} percent{}",
                r.logical_cores,
                if laptop_percent > CPU_TARGET_PERCENT {
                    ". That projection is above the target, so a 4-core laptop may struggle; it needs a real measurement on one, or a faster way to move the pictures."
                } else {
                    "."
                }
            ) + &where_the_time_went(r, cpu)
                + if r.webcam.is_some() {
                    " The webcam (its picture, turning it into colours Windows can encode, and its own video file) is included in these figures."
                } else {
                    ""
                },
        ));
    }

    // Did the program grow without limit?
    if let (Some(first), Some(peak)) = (r.memory_first_mb, r.memory_peak_mb) {
        let grew = peak - first;
        let hours = (r.recorded_ms as f64 / 3_600_000.0).max(1.0 / 3600.0);
        out.push(check(
            "run-memory",
            grew <= 300.0,
            format!(
                "The program used {first:.0} megabytes of memory at the start and {peak:.0} at most (grew {grew:.0}, about {:.0} per hour).{}",
                grew / hours,
                if grew > 300.0 { " Growth like this over a long recording is a leak and must be fixed." } else { "" }
            ),
        ));
    }

    // Did the webcam keep up, and is it placed with the voice?
    if let Some(w) = &r.webcam {
        out.push(webcam_finding(w));
    }

    // Was what the author did noted?
    if let Some(i) = &r.interactions {
        out.push(interactions_finding(i));
    }

    // Did sound and picture stay together?
    for t in &r.tracks {
        let name = if t.microphone { "microphone" } else { "computer's sound" };
        let check_name = if t.microphone { "run-sound-microphone" } else { "run-sound-system" };
        // A quiet computer sends no sound at all, which looks like sound arriving too early, so only a
        // microphone (which sends sound all the time) can be judged on that.
        // What keeps sound with the picture is that it does not stay away from it: one moment over the
        // line is a blip that corrects itself within a look, so a sound track passes if no more than 1
        // percent of the looks were over. With no count, the worst moment alone is judged.
        let ok_by = |worst: u64, over: u64| match t.looks {
            0 => worst as f64 <= MAX_OFFSET_MS,
            looks => over as f64 <= looks as f64 * MAX_SHARE_OVER,
        };
        let within = ok_by(t.worst_later_ms, t.looks_over_later)
            && (!t.microphone || ok_by(t.worst_earlier_ms, t.looks_over_earlier));
        let lost = if t.lost_moments > 0 {
            format!(
                " Windows reported {} moment{} of lost sound.",
                t.lost_moments,
                if t.lost_moments == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        };
        let how_often = |over: u64| {
            if t.looks > 0 && over > 0 {
                format!(
                    ", and over {MAX_OFFSET_MS:.0} ms on {over} of {} looks ({:.2} percent)",
                    t.looks,
                    over as f64 * 100.0 / t.looks as f64
                )
            } else {
                String::new()
            }
        };
        let later = format!(
            "{} ms later than its picture (at {}){}",
            t.worst_later_ms,
            clock(t.worst_later_at_ms),
            how_often(t.looks_over_later)
        );
        let earlier_often = how_often(t.looks_over_earlier);
        let too_far = if within {
            String::new()
        } else if t.looks > 0 {
            format!(", more than the {MAX_OFFSET_MS:.0} ms allowed for more than 1 percent of the recording")
        } else {
            format!(", more than the {MAX_OFFSET_MS:.0} ms allowed")
        };
        let message = if t.microphone {
            // A microphone sends sound all the time, so its sound card's speed against the computer's clock can be told.
            let net = t.heard_ms as i64 - r.recorded_ms as i64;
            let ppm = if r.recorded_ms == 0 { 0.0 } else { net as f64 * 1_000_000.0 / r.recorded_ms as f64 };
            format!(
                "The {name}'s sound card ran {} the computer's clock by {} ms over the recording ({:.0} parts per million). The program corrected this by dropping {} ms of sound and filling in {} ms of silence. At worst the sound played {later}, and {} ms earlier (at {}){earlier_often}{too_far}.{lost}",
                if net >= 0 { "faster than" } else { "slower than" },
                net.abs(),
                ppm.abs(),
                t.trimmed_ms,
                t.padded_ms,
                t.worst_earlier_ms,
                clock(t.worst_earlier_at_ms),
            )
        } else {
            // The computer sends nothing while it is quiet, so silence is filled in and says nothing about the clock.
            format!(
                "The {name}: {} ms was filled in as silence while the computer was quiet, and {} ms of surplus was dropped. At worst the sound played {later}{too_far}.{lost}",
                t.padded_ms, t.trimmed_ms,
            )
        };
        let moves = if t.output_moves.is_empty() {
            String::new()
        } else {
            let list: Vec<String> =
                t.output_moves.iter().map(|m| format!("at {} from {} to {}", clock(m.at_ms), m.from, m.to)).collect();
            format!(
                " Windows moved the computer's sound to another sound output {} time{}, and the recording followed it: {}.",
                t.output_moves.len(),
                if t.output_moves.len() == 1 { "" } else { "s" },
                list.join("; ")
            )
        };
        let problem = match &t.follow_problem {
            Some(why) => format!(" The recording could not follow the sound: {why}"),
            None => String::new(),
        };
        out.push(check(check_name, within && t.follow_problem.is_none(), format!("{message}{moves}{problem}")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> RecordingReport {
        RecordingReport {
            recorded_ms: 3_600_000,
            pictures_written: 108_000,
            beats_skipped: 12,
            pictures_replaced: 40,
            pictures_received: 216_000,
            most_waiting: 3,
            wall_seconds: 3600.0,
            cpu_seconds: Some(1800.0),
            logical_cores: 32,
            memory_first_mb: Some(180.0),
            memory_peak_mb: Some(260.0),
            tracks: vec![
                TrackReport {
                    microphone: true,
                    heard_ms: 3_600_180,
                    padded_ms: 0,
                    trimmed_ms: 160,
                    worst_later_ms: 38,
                    worst_later_at_ms: 125_000,
                    worst_earlier_ms: 12,
                    worst_earlier_at_ms: 2_000_000,
                    lost_moments: 1,
                    ..Default::default()
                },
                TrackReport {
                    microphone: false,
                    heard_ms: 2_000_000,
                    padded_ms: 1_600_000,
                    trimmed_ms: 0,
                    worst_later_ms: 0,
                    worst_later_at_ms: 0,
                    worst_earlier_ms: 900,
                    worst_earlier_at_ms: 61_000,
                    lost_moments: 1,
                    ..Default::default()
                },
            ],
            looks: LookStats {
                looks: 700_000,
                average_gap_ms: 5.1,
                longest_gap_ms: 90,
                gaps_over_50_ms: 20,
                gaps_over_100_ms: 0,
                slow_moments: vec![],
            },
            work: PictureWork::default(),
            webcam: None,
            interactions: None,
        }
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks.iter().find(|c| c.check == name).unwrap_or_else(|| panic!("no check {name}: {checks:?}"))
    }

    #[test]
    fn an_hour_that_went_well_is_told_in_words() {
        let checks = summarize(&good());
        assert!(checks.iter().all(|c| c.ok), "{checks:?}");
        let summary = &find(&checks, "run-summary").message;
        assert!(summary.starts_with("Recorded 1:00:00. 108000 pictures were written. 12 steps"), "{summary}");
        assert!(
            summary.contains(
                "it delivered 216000 in all, about 60 a second, and at one look it took as many as 3 at once"
            ),
            "{summary}"
        );
        let cpu = &find(&checks, "run-cpu").message;
        assert!(
            cpu.contains("about 50 percent of one processor core")
                && cpu.contains("1.6 percent of this computer's 32 cores"),
            "{cpu}"
        );
        assert!(
            cpu.contains("about 12.5 percent, against a target of 10 percent") && cpu.contains("may struggle"),
            "{cpu}"
        );
        let memory = &find(&checks, "run-memory").message;
        assert!(memory.contains("180 megabytes of memory at the start and 260 at most"), "{memory}");
        let stalls = &find(&checks, "run-stalls").message;
        assert!(stalls.contains("0 time(s)") && stalls.contains("longest wait was 90 ms"), "{stalls}");
        let mic = &find(&checks, "run-sound-microphone").message;
        assert!(
            mic.contains("ran faster than the computer's clock by 180 ms") && mic.contains("50 parts per million"),
            "{mic}"
        );
        assert!(
            mic.contains("38 ms later than its picture (at 2:05), and 12 ms earlier (at 33:20)")
                && !mic.contains("more than"),
            "{mic}"
        );
        let system = &find(&checks, "run-sound-system").message;
        assert!(system.contains("1600000 ms was filled in as silence while the computer was quiet"), "{system}");
        assert!(
            find(&checks, "run-sound-system").ok,
            "a quiet computer sends nothing, which must not be taken for sound arriving early"
        );
        assert!(!system.contains("earlier"), "{system}");
    }

    #[test]
    fn a_program_that_fell_behind_says_the_video_may_look_jerky_and_how_often_it_looked() {
        let mut r = RecordingReport { beats_skipped: 5_000, ..good() };
        r.looks.average_gap_ms = 38.0;
        r.looks.gaps_over_50_ms = 900;
        let c = summarize(&r);
        let s = find(&c, "run-summary");
        assert!(!s.ok && s.message.contains("may look jerky"), "{}", s.message);
        assert!(
            s.message.contains("looked at the screen every 38 ms on average")
                && s.message.contains("900 of the 700000 looks"),
            "{}",
            s.message
        );
    }

    #[test]
    fn long_waits_are_listed_with_what_the_program_was_doing() {
        let mut r = good();
        r.looks.longest_gap_ms = 650;
        r.looks.gaps_over_100_ms = 31;
        r.looks.slow_moments = vec![SlowMoment {
            at_ms: 192_000,
            gap_ms: 650,
            picture_ms: 3,
            sound_ms: 1,
            sound_video_ms: 0,
            saving_ms: 640,
        }];
        let c = summarize(&r);
        let stalls = find(&c, "run-stalls");
        assert!(!stalls.ok);
        assert!(
            stalls.message.contains("31 time(s)")
                && stalls.message.contains("650 ms at 3:12")
                && stalls.message.contains("saving to disk took 640 ms"),
            "{}",
            stalls.message
        );
    }

    #[test]
    fn the_processor_finding_says_where_the_picture_work_went() {
        let secs = Duration::from_secs;
        let mut r = good(); // an hour, 1800 s of processor time: 500 ms for every second
        r.work = PictureWork {
            capture: CaptureWork {
                pictures: 144_000,
                reused: 143_990,
                memory: secs(36),
                readback: secs(432),
                rows: secs(180),
            },
            sink: SinkWork { pictures: 108_000, copy: secs(108), call: secs(324) },
            loop_cpu: Some(secs(540)),
        };
        let checks = summarize(&r);
        let cpu = &find(&checks, "run-cpu").message;
        assert!(cpu.contains("Where the picture work went, for every second of the recording"), "{cpu}");
        assert!(
            cpu.contains("took 180 ms (40 pictures a second, 4.5 ms each; 10 ms of it getting memory, 120 ms the graphics card's copy and the wait for it, 50 ms copying rows; 143990 of 144000 pictures used memory that an earlier picture had used)"),
            "{cpu}"
        );
        assert!(
            cpu.contains("took 120 ms (30 pictures a second, 4.0 ms each; 30 ms copying each into the writer's own memory, 90 ms the writer's own work, which includes converting the colours)"),
            "{cpu}"
        );
        assert!(cpu.contains("the recording thread itself used 150 ms of processor time"), "{cpu}");
        // 500 ms in all; the capture steps (180) and the recording thread (150) account for at most 330.
        assert!(
            cpu.contains("500 ms of processor time a second in all, so at least 170 ms of it is outside these steps"),
            "{cpu}"
        );
        assert!(
            find(&checks, "run-cpu").ok == find(&summarize(&good()), "run-cpu").ok,
            "the breakdown changes no verdict"
        );
    }

    #[test]
    fn without_a_recording_thread_reading_the_writer_steps_stand_in_for_it() {
        let secs = Duration::from_secs;
        let mut r = good();
        r.work = PictureWork {
            capture: CaptureWork { pictures: 3600, readback: secs(360), ..CaptureWork::default() },
            sink: SinkWork { pictures: 3600, copy: secs(360), call: secs(360) },
            loop_cpu: None,
        };
        let cpu = summarize(&r).into_iter().find(|c| c.check == "run-cpu").unwrap().message;
        assert!(!cpu.contains("the recording thread itself"), "{cpu}");
        // 100 ms (capture) + 200 ms (writer) = 300 ms accounted of 500: at least 200 ms elsewhere.
        assert!(cpu.contains("at least 200 ms of it is outside these steps"), "{cpu}");
    }

    #[test]
    fn a_recording_that_measured_no_picture_work_says_nothing_about_it() {
        let cpu = summarize(&good()).into_iter().find(|c| c.check == "run-cpu").unwrap().message;
        assert!(!cpu.contains("Where the picture work went"), "{cpu}");
    }

    #[test]
    fn a_program_that_uses_too_much_of_the_computer_fails() {
        let r = RecordingReport { cpu_seconds: Some(3600.0 * 4.0), logical_cores: 8, ..good() };
        let c = summarize(&r);
        let cpu = find(&c, "run-cpu");
        assert!(!cpu.ok && cpu.message.contains("50.0 percent of this computer's 8 cores"), "{}", cpu.message);
    }

    #[test]
    fn memory_that_keeps_growing_is_called_a_leak() {
        let r = RecordingReport { memory_peak_mb: Some(900.0), ..good() };
        let c = summarize(&r);
        let m = find(&c, "run-memory");
        assert!(!m.ok && m.message.contains("is a leak"), "{}", m.message);
    }

    #[test]
    fn a_microphone_that_wandered_too_far_from_its_picture_fails_with_the_numbers_and_when() {
        let mut r = good();
        r.tracks[0].worst_later_ms = 75;
        r.tracks[0].worst_later_at_ms = 61_000;
        let c = summarize(&r);
        let m = find(&c, "run-sound-microphone");
        assert!(
            !m.ok
                && m.message.contains("75 ms later than its picture (at 1:01)")
                && m.message.contains("more than the 40 ms allowed"),
            "{}",
            m.message
        );
        assert!(find(&c, "run-sound-system").ok, "the other track is judged on its own");
    }

    #[test]
    fn the_computers_sound_is_judged_only_on_playing_late_not_on_silence() {
        let mut r = good();
        r.tracks[1].worst_later_ms = 120;
        let c = summarize(&r);
        let m = find(&c, "run-sound-system");
        assert!(!m.ok && m.message.contains("120 ms later than its picture"), "{}", m.message);
    }

    #[test]
    fn a_source_that_does_not_count_pictures_is_not_made_to_claim_a_number() {
        let r = RecordingReport { pictures_received: 0, ..good() };
        let checks = summarize(&r);
        let summary = &find(&checks, "run-summary").message;
        assert!(!summary.contains("it delivered"), "{summary}");
    }

    #[test]
    fn a_short_recording_with_nothing_measured_still_reports_what_it_can() {
        let r = RecordingReport { recorded_ms: 5_000, pictures_written: 150, ..RecordingReport::default() };
        let c = summarize(&r);
        assert_eq!(c.len(), 1, "no cpu, memory, look or sound numbers were measured, so none are claimed");
        assert!(c[0].message.starts_with("Recorded 0:05."));
    }

    #[test]
    fn times_are_said_as_minutes_and_seconds_or_hours() {
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
    }

    #[test]
    fn a_followed_move_is_told_and_a_failed_one_is_a_problem() {
        let mut r = good();
        r.tracks[1].output_moves = vec![OutputMove {
            at_ms: 133_000,
            from: "Speakers (Realtek)".into(),
            to: "Headphones (2- Shure MV7)".into(),
        }];
        let checks = summarize(&r);
        let system = find(&checks, "run-sound-system");
        assert!(system.ok, "{system:?}");
        assert!(
            system.message.contains(
                "moved the computer's sound to another sound output 1 time, and the recording followed it: at 2:13 from Speakers (Realtek) to Headphones (2- Shure MV7)."
            ),
            "{}",
            system.message
        );
        assert!(!find(&checks, "run-sound-microphone").message.contains("moved"));

        r.tracks[1].follow_problem = Some("Another program has taken the headphones.".into());
        let checks = summarize(&r);
        let system = find(&checks, "run-sound-system");
        assert!(!system.ok, "a sound that could not be followed is a failure");
        assert!(
            system.message.contains("could not follow the sound: Another program has taken the headphones."),
            "{}",
            system.message
        );
    }

    #[test]
    fn one_moment_over_the_line_is_a_blip_but_often_over_it_is_not() {
        // The owner's 70 second run: the computer's sound reached 55 ms once, in about 14,000 looks.
        let mut r = good();
        r.tracks[1].worst_later_ms = 55;
        r.tracks[1].worst_later_at_ms = 30_000;
        r.tracks[1].looks = 14_000;
        r.tracks[1].looks_over_later = 1;
        let checks = summarize(&r);
        let system = find(&checks, "run-sound-system");
        assert!(system.ok, "{}", system.message);
        assert!(
            system
                .message
                .contains("55 ms later than its picture (at 0:30), and over 40 ms on 1 of 14000 looks (0.01 percent)"),
            "{}",
            system.message
        );
        assert!(!system.message.contains("allowed"));

        // The same worst moment, but over the line on 3 percent of the looks, is sound that stays away from the picture.
        r.tracks[1].looks_over_later = 420;
        let checks = summarize(&r);
        let system = find(&checks, "run-sound-system");
        assert!(!system.ok);
        assert!(
            system.message.contains("on 420 of 14000 looks (3.00 percent)")
                && system.message.contains("for more than 1 percent of the recording")
        );

        // Exactly 1 percent is still fine; a microphone is judged the same way on being too early.
        r.tracks[1].looks_over_later = 140;
        assert!(find(&summarize(&r), "run-sound-system").ok);
        let mut m = good();
        m.tracks[0].looks = 1000;
        m.tracks[0].worst_earlier_ms = 90;
        m.tracks[0].looks_over_earlier = 50;
        let checks = summarize(&m);
        assert!(!find(&checks, "run-sound-microphone").ok);
        assert!(find(&checks, "run-sound-microphone").message.contains("over 40 ms on 50 of 1000 looks"));
    }

    fn webcam_hour() -> WebcamReport {
        WebcamReport {
            camera: "NexiGo Iris 4K Webcam".into(),
            width: 1280,
            height: 720,
            fps: 30,
            pictures_written: 108_001,
            pictures_repeated: 300,
            pictures_received: 107_950,
            pictures_wrong_size: 0,
            pictures_lost: 0,
            longest_gap_ms: 140,
            stamped: 107_950,
            unstamped: 0,
            delay_average_ms: 96,
            delay_most_ms: 210,
            stopped: None,
            recorded_ms: 3_600_000,
            work: SinkWork::default(),
        }
    }

    #[test]
    fn a_recording_without_a_webcam_says_nothing_about_one() {
        assert!(summarize(&good()).iter().all(|c| c.check != "run-webcam"));
    }

    #[test]
    fn a_webcam_that_kept_up_is_told_in_words_with_how_late_its_pictures_arrived() {
        let mut r = good();
        r.webcam = Some(webcam_hour());
        let checks = summarize(&r);
        let c = find(&checks, "run-webcam");
        assert!(c.ok, "{}", c.message);
        assert!(c.message.contains("The webcam (NexiGo Iris 4K Webcam) was recorded at 1280 by 720, 30 pictures a second: 108001 pictures for 1:00:00 of recording."), "{}", c.message);
        assert!(
            c.message.contains(
                "The camera sent 107950 pictures, about 30 a second; the longest wait between two was 140 ms."
            ),
            "{}",
            c.message
        );
        assert!(
            c.message.contains("placed where its camera time stamp says it was taken")
                && c.message.contains("96 ms after it was taken on average (210 ms at most)"),
            "{}",
            c.message
        );
        assert!(!c.message.contains("fewer than"), "{}", c.message);
        assert!(
            find(&checks, "run-cpu").message.contains("The webcam (its picture"),
            "the processor finding says the webcam is in it"
        );
    }

    #[test]
    fn a_camera_whose_time_stamps_cannot_be_used_is_said_to_place_pictures_late() {
        let mut r = good();
        let mut w = webcam_hour();
        w.stamped = 0;
        w.unstamped = w.pictures_received;
        r.webcam = Some(w);
        let m = &find(&summarize(&r), "run-webcam").message.clone();
        assert!(
            m.contains("time stamps could not be used") && m.contains("may appear that much later than your voice"),
            "{m}"
        );
    }

    #[test]
    fn a_slow_camera_a_stalled_camera_and_a_camera_that_stopped_are_each_a_finding() {
        let mut r = good();
        let mut slow = webcam_hour();
        slow.pictures_received = 54_000;
        slow.pictures_repeated = 54_000;
        r.webcam = Some(slow);
        let c = find(&summarize(&r), "run-webcam").clone();
        assert!(
            c.ok && c.message.contains("fewer than the 30 asked for") && c.message.contains("repeat the one before"),
            "{}",
            c.message
        );

        let mut stalled = webcam_hour();
        stalled.longest_gap_ms = 4_200;
        r.webcam = Some(stalled);
        let c = find(&summarize(&r), "run-webcam").clone();
        assert!(!c.ok && c.message.contains("more than a second without sending a picture"), "{}", c.message);

        let mut stopped = webcam_hour();
        stopped.stopped = Some((754_000, "The camera was unplugged.".into()));
        stopped.pictures_written = 22_621;
        r.webcam = Some(stopped);
        let c = find(&summarize(&r), "run-webcam").clone();
        assert!(!c.ok && c.message.contains("The camera stopped at 12:34: The camera was unplugged. The webcam video ends there; the screen and the sound carried on."), "{}", c.message);

        let mut gaps = webcam_hour();
        gaps.pictures_written = 90_000;
        r.webcam = Some(gaps);
        let c = find(&summarize(&r), "run-webcam").clone();
        assert!(!c.ok && c.message.contains("Only 90000 of the 108001 pictures"), "{}", c.message);
    }

    #[test]
    fn pictures_lost_or_of_the_wrong_size_are_counted_in_the_finding() {
        let mut r = good();
        let mut w = webcam_hour();
        w.pictures_lost = 12;
        w.pictures_wrong_size = 2;
        r.webcam = Some(w);
        let c = find(&summarize(&r), "run-webcam").clone();
        assert!(
            c.message.contains("too slow to take 12 camera pictures")
                && c.message.contains("2 pictures of another size"),
            "{}",
            c.message
        );
    }
}
