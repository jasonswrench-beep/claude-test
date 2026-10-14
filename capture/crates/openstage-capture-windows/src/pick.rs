// SPDX-License-Identifier: Apache-2.0
//! The `pick-region` command: let the author choose the part of a display to record by dragging it, and
//! hand back the four numbers `--region` takes (story A1 in `docs/prd.md`).
//!
//! The picker itself is `win_region.rs`. This is what surrounds it: finding the display, reading the
//! starting rectangle, saying in words what was chosen, and saving the numbers to a file where a script
//! (or OpenStage Author) can read them. The picker is passed in, so the rest can be tested here.

use crate::{check, system_platform, Check, DisplayInfo, PickOutcome, Platform, Region};
use std::path::PathBuf;

/// Something that lets the author choose a rectangle of a display, starting from one if given. On Windows
/// this is the picker window; tests give one that answers at once.
pub type Picker<'a> = &'a dyn Fn(&DisplayInfo, u32, Option<Region>) -> Result<PickOutcome, String>;

/// What to do.
#[derive(Debug, Clone)]
pub struct PickRegionOptions {
    /// Which display, by number (1 is the main display).
    pub display: u32,
    /// Start from this rectangle (four numbers, as for `--region`), else from the middle of the display.
    pub start: Option<String>,
    /// Save the chosen numbers here, as `left,top,width,height` on one line, when a region is chosen.
    pub out: Option<PathBuf>,
}

/// The four numbers as `--region` takes them.
pub fn region_numbers(r: &Region) -> String {
    format!("{},{},{},{}", r.x, r.y, r.width, r.height)
}

/// Choose a region on this computer's screen. Reports one finding, `region-chosen`; true if a region was chosen.
pub fn run_pick_region(opts: &PickRegionOptions, report: &mut dyn FnMut(Check)) -> bool {
    run_pick_region_with(system_platform().as_ref(), &crate::pick_region, opts, report)
}

/// The same with the screen and the picker given, so that it can be tried without a screen.
pub fn run_pick_region_with(
    platform: &dyn Platform,
    picker: Picker,
    opts: &PickRegionOptions,
    report: &mut dyn FnMut(Check),
) -> bool {
    let mut fail = |message: String| {
        report(check("region-chosen", false, message));
        false
    };
    let start = match opts.start.as_deref().map(Region::parse).transpose() {
        Ok(start) => start,
        Err(message) => return fail(message),
    };
    let displays = match platform.list_displays() {
        Ok(displays) => displays,
        Err(e) => return fail(e.to_string()),
    };
    let Some(display) = displays.iter().find(|d| d.number == opts.display) else {
        return fail(format!("There is no Display {}. Choose a number from 1 to {}.", opts.display, displays.len()));
    };
    // A starting rectangle that does not fit this display is not an error here: the picker starts in the middle.
    let start = start.filter(|r| r.fit(display.width, display.height).is_ok());
    match picker(display, display.number, start) {
        Err(message) => fail(message),
        Ok(PickOutcome::Cancelled) => {
            fail("The picker was closed without choosing, so no region was chosen.".to_string())
        }
        Ok(PickOutcome::Chosen { region, notes }) => {
            let numbers = region_numbers(&region);
            if let Some(path) = &opts.out {
                if let Err(e) = std::fs::write(path, format!("{numbers}\r\n")) {
                    return fail(format!(
                        "The region was chosen, but its numbers could not be saved in {}: {e}",
                        path.display()
                    ));
                }
            }
            let mut message = format!(
                "Chosen: {} of Display {}. To record it, use --region {numbers}.",
                region.describe(),
                display.number
            );
            for note in notes {
                message.push(' ');
                message.push_str(&note);
            }
            report(check("region-chosen", true, message));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrameSource, ScreenError, WindowsVersion};
    use std::cell::RefCell;

    struct Screen(Vec<DisplayInfo>);
    impl Platform for Screen {
        fn windows_version(&self) -> Result<WindowsVersion, ScreenError> {
            Ok(WindowsVersion { major: 10, minor: 0, build: 26200 })
        }
        fn capture_supported(&self) -> bool {
            true
        }
        fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
            Ok(self.0.clone())
        }
        fn start_display_capture(&self, _: &DisplayInfo, _: bool) -> Result<Box<dyn FrameSource>, ScreenError> {
            Err(ScreenError::NotWindows)
        }
    }

    fn display(number: u32, width: u32, height: u32) -> DisplayInfo {
        DisplayInfo {
            number,
            x: 0,
            y: 0,
            width,
            height,
            main: number == 1,
            system_name: format!("D{number}"),
            handle: number as isize,
        }
    }

    fn two_displays() -> Screen {
        Screen(vec![display(1, 1920, 1080), display(2, 1280, 720)])
    }

    fn run(screen: &Screen, picker: Picker, opts: &PickRegionOptions) -> (bool, Vec<Check>) {
        let mut found = Vec::new();
        let ok = run_pick_region_with(screen, picker, opts, &mut |c| found.push(c));
        (ok, found)
    }

    fn options() -> PickRegionOptions {
        PickRegionOptions { display: 1, start: None, out: None }
    }

    #[test]
    fn a_chosen_region_is_said_in_words_with_the_numbers_to_use_and_saved_for_a_script() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("chosen-region.txt");
        let picker = |_: &DisplayInfo, _: u32, _: Option<Region>| {
            Ok(PickOutcome::Chosen {
                region: Region { x: 100, y: 80, width: 1280, height: 720 },
                notes: vec!["The width 1281 was made 1280, because H.264 video needs an even width.".into()],
            })
        };
        let (ok, found) = run(&two_displays(), &picker, &PickRegionOptions { out: Some(out.clone()), ..options() });
        assert!(ok);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].check, "region-chosen");
        assert!(found[0].ok);
        let m = &found[0].message;
        assert!(m.contains("Chosen: the part of the display that starts 100 pixels from the left and 80 from the top, 1280 by 720 pixels of Display 1."), "{m}");
        assert!(m.contains("use --region 100,80,1280,720."), "{m}");
        assert!(m.contains("The width 1281 was made 1280"), "{m}");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "100,80,1280,720\r\n");
    }

    #[test]
    fn the_picker_is_given_the_chosen_display_and_a_starting_rectangle_that_fits() {
        let seen = RefCell::new(Vec::new());
        let picker = |d: &DisplayInfo, n: u32, start: Option<Region>| {
            seen.borrow_mut().push((d.width, d.height, n, start));
            Ok(PickOutcome::Cancelled)
        };
        let screen = two_displays();
        run(&screen, &picker, &PickRegionOptions { display: 2, start: Some("10,20,640,360".into()), out: None });
        run(&screen, &picker, &PickRegionOptions { display: 2, start: Some("900,20,640,360".into()), out: None });
        let seen = seen.borrow();
        assert_eq!(seen[0], (1280, 720, 2, Some(Region { x: 10, y: 20, width: 640, height: 360 })));
        assert_eq!(seen[1], (1280, 720, 2, None), "a start that runs off the display is ignored, not an error");
    }

    #[test]
    fn closing_the_picker_chooses_nothing_and_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("chosen-region.txt");
        let picker = |_: &DisplayInfo, _: u32, _: Option<Region>| Ok(PickOutcome::Cancelled);
        let (ok, found) = run(&two_displays(), &picker, &PickRegionOptions { out: Some(out.clone()), ..options() });
        assert!(!ok);
        assert!(!found[0].ok && found[0].message.contains("no region was chosen"), "{:?}", found[0]);
        assert!(!out.exists());
    }

    #[test]
    fn mistakes_are_explained_before_anything_is_shown() {
        let never = |_: &DisplayInfo, _: u32, _: Option<Region>| -> Result<PickOutcome, String> {
            panic!("the picker must not open")
        };
        let (ok, found) = run(&two_displays(), &never, &PickRegionOptions { display: 5, ..options() });
        assert!(!ok && found[0].message.contains("There is no Display 5. Choose a number from 1 to 2."), "{found:?}");
        let (ok, found) = run(&two_displays(), &never, &PickRegionOptions { start: Some("1,2,3".into()), ..options() });
        assert!(!ok && found[0].message.contains("has 3 numbers"), "{found:?}");
    }

    #[test]
    fn a_picker_that_cannot_open_and_numbers_that_cannot_be_saved_are_reported() {
        let broken = |_: &DisplayInfo, _: u32, _: Option<Region>| Err("Windows would not make the picker.".to_string());
        let (ok, found) = run(&two_displays(), &broken, &options());
        assert!(!ok && found[0].message == "Windows would not make the picker.", "{found:?}");

        let picker = |_: &DisplayInfo, _: u32, _: Option<Region>| {
            Ok(PickOutcome::Chosen { region: Region { x: 0, y: 0, width: 640, height: 360 }, notes: vec![] })
        };
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-folder").join("chosen.txt");
        let (ok, found) = run(&two_displays(), &picker, &PickRegionOptions { out: Some(missing), ..options() });
        assert!(!ok && found[0].message.contains("could not be saved"), "{found:?}");
    }

    #[test]
    fn the_numbers_are_in_the_order_region_takes_them() {
        assert_eq!(region_numbers(&Region { x: 1, y: 2, width: 3, height: 4 }), "1,2,3,4");
        assert_eq!(Region::parse("1,2,3,4").unwrap(), Region { x: 1, y: 2, width: 3, height: 4 });
    }
}
