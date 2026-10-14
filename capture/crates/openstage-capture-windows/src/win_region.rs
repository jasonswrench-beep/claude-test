// SPDX-License-Identifier: Apache-2.0
//! The window for choosing a region by dragging (story A1 in `docs/prd.md`). Windows only.
//!
//! Two windows work together:
//!
//! * An **overlay** that dims the chosen display and shows the rectangle with eight handles. The mouse
//!   moves it (drag inside), resizes it (drag a handle) or draws a new one (drag on the dimmed screen).
//!   The geometry is in `region_editor.rs`, the drawing in `region_paint.rs`.
//! * A small ordinary **window of four numbers** (left, top, width, height) with "Use this region",
//!   "Whole display" and "Cancel". The numbers follow the mouse, and typing in them moves the rectangle.
//!   Because it is made of ordinary Windows controls, it works with the keyboard alone and is read by
//!   screen readers (Tab between the boxes and buttons, Enter to use the region, Escape to cancel).
//!   Nothing can only be done with the mouse.
//!
//! It has been type-checked on Linux and never run. Whether NVDA and Narrator read it well is not yet
//! known. Treat all of it as unproven until the owner runs it.

use crate::region_editor::{Handle, PickOutcome, RegionEditor};
use crate::region_paint::picker_overlay;
use crate::win_layered::{dpi_for, register_class, Surface};
use crate::{DisplayInfo, Region};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, ScreenToClient, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DEFAULT_QUALITY, FF_DONTCARE,
    HFONT, OUT_DEFAULT_PRECIS,
};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetForegroundWindow, GetMessageW,
    GetWindowTextW, GetWindowThreadProcessId, IsDialogMessageW, LoadCursorW, PostQuitMessage, SendMessageW, SetCursor,
    SetForegroundWindow, SetWindowTextW, ShowWindow, TranslateMessage, BN_CLICKED, BS_DEFPUSHBUTTON, BS_PUSHBUTTON,
    EN_CHANGE, ES_AUTOHSCROLL, ES_MULTILINE, ES_NUMBER, ES_READONLY, HMENU, HTCLIENT, IDC_CROSS, IDC_SIZEALL,
    IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, MA_NOACTIVATE, MB_ICONEXCLAMATION, MSG, SW_SHOW,
    SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR, WM_SETFONT, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

// Control numbers. OK and Cancel use the numbers Windows gives the Enter and Escape keys.
const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_WHOLE: i32 = 101;
const ID_LEFT: i32 = 201;
const ID_HEIGHT: i32 = 204;
const ID_STATUS: i32 = 210;
/// A window asks this of a dialog to learn which button Enter presses.
const DM_GETDEFID: u32 = 0x0400;
/// The answer carries this marker in its upper half to say there is one.
const DC_HASDEFID: i32 = 0x534B;
/// How close, in pixels at 96 dots per inch, the pointer must be to a handle or edge to grab it.
const GRAB: i32 = 10;

struct Picker {
    editor: RegionEditor,
    number: u32,
    scale: i32,
    origin: (i32, i32),
    overlay: HWND,
    dialog: HWND,
    edits: [HWND; 4],
    status: HWND,
    surface: Surface,
    outcome: Option<PickOutcome>,
}

thread_local! {
    static PICKER: RefCell<Option<Picker>> = const { RefCell::new(None) };
    /// True while the program itself writes into the number boxes, so that is not mistaken for typing.
    static UPDATING: Cell<bool> = const { Cell::new(false) };
}

/// Look at or change the picker. `None` if it is not there or is in use further up (a message that arrives
/// while another is being handled is simply left alone).
fn with<R>(f: impl FnOnce(&mut Picker) -> R) -> Option<R> {
    PICKER.with(|cell| cell.try_borrow_mut().ok().and_then(|mut guard| guard.as_mut().map(f)))
}

fn set_text(window: HWND, text: &str) {
    unsafe {
        let _ = SetWindowTextW(window, &HSTRING::from(text));
    }
}

/// Draw the rectangle on the dimmed screen again.
fn repaint() {
    with(|p| {
        let canvas = picker_overlay(&p.editor, p.scale);
        if let Err(e) = p.surface.show(&canvas, p.origin.0, p.origin.1) {
            eprintln!("The rectangle could not be drawn: {e}");
        }
    });
}

/// Put the rectangle's numbers into the four boxes and say it in words.
fn show_numbers() {
    let Some((r, text, edits, status)) = with(|p| (p.editor.region(), p.editor.describe(p.number), p.edits, p.status))
    else {
        return;
    };
    UPDATING.with(|u| u.set(true));
    for (edit, value) in edits.iter().zip([r.x, r.y, r.width, r.height]) {
        set_text(*edit, &value.to_string());
    }
    set_text(status, &text);
    UPDATING.with(|u| u.set(false));
}

/// Read the four boxes. `Err` says in words what is wrong.
fn read_boxes() -> Result<[u32; 4], String> {
    let Some(edits) = with(|p| p.edits) else { return Err("The picker is not open.".to_string()) };
    let mut numbers = [0u32; 4];
    for (slot, edit) in numbers.iter_mut().zip(edits) {
        let mut buffer = [0u16; 16];
        let length = unsafe { GetWindowTextW(edit, &mut buffer) }.max(0) as usize;
        let text = String::from_utf16_lossy(&buffer[..length]);
        *slot = text.trim().parse().map_err(|_| "Each of the four boxes needs a whole number.".to_string())?;
    }
    Ok(numbers)
}

/// The boxes were typed in: move the rectangle to match if the numbers can be used, else say why not.
fn apply_boxes() -> Result<(), String> {
    let [x, y, width, height] = read_boxes()?;
    let applied = with(|p| p.editor.set_numbers(x, y, width, height).map(|_| (p.editor.describe(p.number), p.status)));
    match applied {
        Some(Ok((text, status))) => {
            repaint();
            set_text(status, &text);
            Ok(())
        }
        Some(Err(why)) => Err(why),
        None => Ok(()),
    }
}

fn say(text: &str) {
    if let Some(status) = with(|p| p.status) {
        set_text(status, text);
    }
}

fn close_both() {
    if let Some((dialog, overlay)) = with(|p| (p.dialog, p.overlay)) {
        unsafe {
            let _ = DestroyWindow(dialog);
            let _ = DestroyWindow(overlay);
        }
    }
}

fn finish(outcome: PickOutcome) {
    with(|p| p.outcome = Some(outcome));
    close_both();
}

fn accept() {
    // What the boxes say is what is chosen; if a box cannot be used the author is told, and nothing closes.
    if let Err(why) = apply_boxes() {
        say(&format!("That cannot be used yet: {why}"));
        // A sound too, for an author who is not looking at the window.
        unsafe {
            let _ = MessageBeep(MB_ICONEXCLAMATION);
        }
        return;
    }
    match with(|p| p.editor.accepted()) {
        Some(Ok((region, notes))) => finish(PickOutcome::Chosen { region, notes }),
        Some(Err(why)) => {
            say(&why);
            unsafe {
                let _ = MessageBeep(MB_ICONEXCLAMATION);
            }
        }
        None => {}
    }
}

unsafe extern "system" fn dialog_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            match id {
                ID_OK if code == BN_CLICKED => accept(),
                ID_CANCEL => finish(PickOutcome::Cancelled),
                ID_WHOLE => {
                    with(|p| p.editor.use_whole_display());
                    repaint();
                    show_numbers();
                }
                ID_LEFT..=ID_HEIGHT if code == EN_CHANGE && !UPDATING.with(|u| u.get()) => {
                    if let Err(why) = apply_boxes() {
                        say(&why);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            finish(PickOutcome::Cancelled);
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // Enter in a box presses "Use this region".
        DM_GETDEFID => LRESULT(((DC_HASDEFID << 16) | ID_OK) as isize),
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn pointer(lparam: LPARAM) -> (i32, i32) {
    // The position is two signed 16-bit numbers packed into one.
    ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32)
}

fn cursor_for(handle: Option<Handle>) -> PCWSTR {
    match handle {
        Some(Handle::TopLeft) | Some(Handle::BottomRight) => IDC_SIZENWSE,
        Some(Handle::TopRight) | Some(Handle::BottomLeft) => IDC_SIZENESW,
        Some(Handle::Top) | Some(Handle::Bottom) => IDC_SIZENS,
        Some(Handle::Left) | Some(Handle::Right) => IDC_SIZEWE,
        Some(Handle::Inside) => IDC_SIZEALL,
        None => IDC_CROSS,
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        // Clicking the dimmed screen must not take the keyboard away from the window of numbers.
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT => {
            let mut at = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut at);
                let _ = ScreenToClient(hwnd, &mut at);
            }
            let handle = with(|p| p.editor.hit_test(at.x, at.y, GRAB * p.scale)).flatten();
            if let Ok(cursor) = unsafe { LoadCursorW(None, cursor_for(handle)) } {
                unsafe { SetCursor(Some(cursor)) };
            }
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = pointer(lparam);
            unsafe { SetCapture(hwnd) };
            with(|p| p.editor.press(x, y, GRAB * p.scale));
            repaint();
            show_numbers();
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = pointer(lparam);
            if with(|p| p.editor.dragging()).unwrap_or(false) {
                with(|p| p.editor.drag_to(x, y));
                repaint();
                show_numbers();
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with(|p| p.editor.release());
            unsafe {
                let _ = ReleaseCapture();
            }
            show_numbers();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Make windows of the picker's kind ask for keyboard focus even when the program was started from a console.
fn bring_to_front(window: HWND) {
    unsafe {
        let mine = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let joined = foreground_thread != 0 && foreground_thread != mine;
        if joined {
            let _ = AttachThreadInput(mine, foreground_thread, true);
        }
        let _ = SetForegroundWindow(window);
        if joined {
            let _ = AttachThreadInput(mine, foreground_thread, false);
        }
    }
}

struct Layout {
    dpi: u32,
}

impl Layout {
    fn px(&self, logical: i32) -> i32 {
        logical * self.dpi as i32 / 96
    }
}

unsafe fn control(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    rect: (i32, i32, i32, i32),
    id: i32,
    font: HFONT,
) -> Result<HWND, String> {
    unsafe {
        let hinstance =
            GetModuleHandleW(None).map_err(|e| format!("Windows would not say which program this is: {e}"))?;
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            &HSTRING::from(text),
            style | WS_CHILD | WS_VISIBLE,
            rect.0,
            rect.1,
            rect.2,
            rect.3,
            Some(parent),
            Some(HMENU(id as usize as *mut c_void)),
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("Windows would not make a part of the window: {e}"))?;
        SendMessageW(window, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        Ok(window)
    }
}

/// Let the author choose a rectangle of `display` (which is Display `number`), starting from `initial`.
/// Returns when the author uses it or cancels.
pub fn pick_region(display: &DisplayInfo, number: u32, initial: Option<Region>) -> Result<PickOutcome, String> {
    crate::win::prepare();
    register_class(w!("OpenStageCaptureRegionOverlay"), Some(overlay_proc))?;
    register_class(w!("OpenStageCaptureRegionDialog"), Some(dialog_proc))?;
    let dpi = dpi_for(display.handle);
    let layout = Layout { dpi };
    let scale = ((dpi + 48) / 96).max(1) as i32;
    let editor = RegionEditor::new(display.width, display.height, initial);
    let start = editor.region();

    unsafe {
        let hinstance =
            GetModuleHandleW(None).map_err(|e| format!("Windows would not say which program this is: {e}"))?;

        // The dimmed screen with the rectangle on it.
        let overlay = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("OpenStageCaptureRegionOverlay"),
            w!(""),
            WS_POPUP,
            display.x,
            display.y,
            display.width as i32,
            display.height as i32,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("Windows would not make the picker: {e}"))?;
        let surface = Surface::new(overlay, display.width, display.height)?;
        surface.show(&picker_overlay(&editor, scale), display.x, display.y)?;

        // The window of numbers, in the corner of the same display.
        let (client_width, client_height) = (layout.px(480), layout.px(420));
        let style = WS_CAPTION | WS_SYSMENU;
        let mut frame = RECT { left: 0, top: 0, right: client_width, bottom: client_height };
        let _ = AdjustWindowRectExForDpi(&mut frame, style, false, WS_EX_TOPMOST, dpi);
        let (frame_width, frame_height) = (frame.right - frame.left, frame.bottom - frame.top);
        let margin = layout.px(24);
        let dialog = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("OpenStageCaptureRegionDialog"),
            &HSTRING::from("Choose the part of the screen to record"),
            style,
            display.x + (display.width as i32 - frame_width - margin).max(0),
            display.y + (display.height as i32 - frame_height - margin * 2).max(0),
            frame_width,
            frame_height,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("Windows would not make the window of numbers: {e}"))?;

        let font = CreateFontW(
            -layout.px(16),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            w!("Segoe UI"),
        );

        let m = layout.px(16);
        let intro = format!(
            "Choose the part of Display {number} to record. Drag on the dimmed screen, or type the four numbers. Enter uses it. Escape cancels."
        );
        control(dialog, w!("STATIC"), &intro, WINDOW_STYLE(0), (m, m, client_width - 2 * m, layout.px(72)), 0, font)?;

        // Each box comes straight after its label, which is how a screen reader learns the box's name.
        let fields = [
            ("Left (pixels from the left edge)", start.x),
            ("Top (pixels from the top edge)", start.y),
            ("Width in pixels", start.width),
            ("Height in pixels", start.height),
        ];
        let mut edits = [HWND::default(); 4];
        for (i, (label, value)) in fields.iter().enumerate() {
            let y = layout.px(96) + i as i32 * layout.px(36);
            control(
                dialog,
                w!("STATIC"),
                label,
                WINDOW_STYLE(0),
                (m, y + layout.px(4), layout.px(320), layout.px(24)),
                0,
                font,
            )?;
            edits[i] = control(
                dialog,
                w!("EDIT"),
                &value.to_string(),
                WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_NUMBER as u32 | ES_AUTOHSCROLL as u32),
                (layout.px(348), y, layout.px(116), layout.px(28)),
                ID_LEFT + i as i32,
                font,
            )?;
        }
        // What is chosen, and what is wrong if anything is, in a box that can be tabbed to and read.
        control(
            dialog,
            w!("STATIC"),
            "Current choice, and any problem with it",
            WINDOW_STYLE(0),
            (m, layout.px(248), client_width - 2 * m, layout.px(24)),
            0,
            font,
        )?;
        let status = control(
            dialog,
            w!("EDIT"),
            &editor.describe(number),
            WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_READONLY as u32 | ES_MULTILINE as u32),
            (m, layout.px(274), client_width - 2 * m, layout.px(84)),
            ID_STATUS,
            font,
        )?;
        let y = layout.px(372);
        control(
            dialog,
            w!("BUTTON"),
            "Use this region",
            WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            (m, y, layout.px(160), layout.px(34)),
            ID_OK,
            font,
        )?;
        control(
            dialog,
            w!("BUTTON"),
            "Whole display",
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            (m + layout.px(172), y, layout.px(150), layout.px(34)),
            ID_WHOLE,
            font,
        )?;
        control(
            dialog,
            w!("BUTTON"),
            "Cancel",
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            (m + layout.px(334), y, layout.px(130), layout.px(34)),
            ID_CANCEL,
            font,
        )?;

        PICKER.with(|cell| {
            *cell.borrow_mut() = Some(Picker {
                editor,
                number,
                scale,
                origin: (display.x, display.y),
                overlay,
                dialog,
                edits,
                status,
                surface,
                outcome: None,
            });
        });

        let _ = ShowWindow(overlay, SW_SHOWNOACTIVATE);
        let _ = ShowWindow(dialog, SW_SHOW);
        bring_to_front(dialog);
        let _ = SetFocus(Some(edits[0]));
        // Select the number, so that typing replaces it (EM_SETSEL from the start to the end).
        SendMessageW(edits[0], 0x00B1, Some(WPARAM(0)), Some(LPARAM(-1)));

        // Handle messages until the window of numbers is gone. Tab, Enter and Escape in it are handled for us.
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            if !IsDialogMessageW(dialog, &message).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    let outcome = PICKER.with(|cell| cell.borrow_mut().take()).and_then(|p| p.outcome);
    Ok(outcome.unwrap_or(PickOutcome::Cancelled))
}
