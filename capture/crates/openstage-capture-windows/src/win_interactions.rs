// SPDX-License-Identifier: Apache-2.0
//! Noting what the author does, on Windows (story A6).
//!
//! Three kinds of thing are used, and nothing else:
//!
//! * A **mouse hook** that Windows calls whenever a mouse button goes down. It is only ever asked about clicks.
//! * A **keyboard hook**, installed only at `--interactions on`. With `no-keys` there is no keyboard hook at all, so
//!   not one key press is ever seen by this program. (Windows requires a hook to see keys; it cannot be told to look
//!   only at some of them.)
//! * Windows' **accessibility information** (UI Automation), which names the item under a click and says whether the
//!   keyboard is aimed at a password field.
//!
//! A hook must answer in a fraction of a second or Windows removes it, and the accessibility questions can take a
//! long time when a program is busy. So the hooks only stamp what happened with recording time and hand it on; a
//! second thread asks the questions and works out what to write (the rules are in `interactions.rs`). The thread
//! that looks at the screen and listens to the sound never waits for either. If the second thread falls behind, what
//! the hooks hand over is thrown away and counted, and the author is told.
//!
//! The cursor's place is read from Windows every 10 ms rather than from the hook, because a mouse can report a
//! thousand times a second and nothing needs that.
//!
//! Unproven until the owner runs it: written and type-checked without a Windows computer.

use crate::interactions::{
    classify_focus, role_name, Button, Drained, FocusKind, InteractionFilter, InteractionPlatform, InteractionSetup,
    InteractionSource, Lookups, Mods, Raw, SharedTimeline, WindowState,
};
use crate::win_windows::program_name;
use openstage_capture_project::LogEvent;
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG,
    MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT,
    WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
};

/// How many happenings the hooks may have handed over and not yet been dealt with before more are thrown away.
const QUEUE: usize = 8192;
/// How often the cursor is looked at.
const CURSOR_EVERY: Duration = Duration::from_millis(10);
/// How often the window in front is looked at.
const WINDOW_EVERY: Duration = Duration::from_millis(100);

/// The real thing on Windows.
#[derive(Default)]
pub struct WindowsInteractions;

impl WindowsInteractions {
    pub fn new() -> Self {
        Self
    }
}

/// Something a hook saw, already placed in recording time.
struct Stamped {
    ms: u64,
    raw: Raw,
}

/// What the hook functions need. They are bare functions that Windows calls, so this lives with the hook thread.
struct HookState {
    tx: SyncSender<Stamped>,
    timeline: SharedTimeline,
    dropped: Arc<AtomicU64>,
}

thread_local! {
    static HOOK: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

/// Hand a happening on, placed in recording time. Nothing while recording is paused. Never waits.
fn hand_over(raw: Raw) {
    HOOK.with(|slot| {
        let Ok(slot) = slot.try_borrow() else { return };
        let Some(state) = slot.as_ref() else { return };
        let now = Instant::now();
        let ms = state.timeline.lock().unwrap_or_else(|e| e.into_inner()).media_ms_at(now);
        let Some(ms) = ms else { return };
        match state.tx.try_send(Stamped { ms, raw }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                state.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    });
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let button = match wparam.0 as u32 {
            WM_LBUTTONDOWN => Some(Button::Left),
            WM_RBUTTONDOWN => Some(Button::Right),
            WM_MBUTTONDOWN => Some(Button::Middle),
            WM_XBUTTONDOWN => match (info.mouseData >> 16) & 0xFFFF {
                1 => Some(Button::Back),
                2 => Some(Button::Forward),
                _ => None,
            },
            _ => None,
        };
        if let Some(button) = button {
            hand_over(Raw::Click { x: info.pt.x, y: info.pt.y, button });
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn held(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16) & 0x8000 != 0
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let mods = Mods {
                    ctrl: held(VK_CONTROL),
                    alt: held(VK_MENU),
                    shift: held(VK_SHIFT),
                    win: held(VK_LWIN) || held(VK_RWIN),
                };
                hand_over(Raw::KeyDown { vk: info.vkCode, mods });
            }
            WM_KEYUP | WM_SYSKEYUP => hand_over(Raw::KeyUp { vk: info.vkCode }),
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The thread that owns the hooks. A hook works only while its thread handles messages.
fn run_hooks(keys: bool, state: HookState, ready: Sender<Result<u32, String>>) {
    crate::win::prepare();
    HOOK.with(|slot| *slot.borrow_mut() = Some(state));
    unsafe {
        let module = match GetModuleHandleW(None) {
            Ok(module) => module,
            Err(e) => {
                let _ = ready.send(Err(format!("Windows would not say which program this is: {e}")));
                return;
            }
        };
        let mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(module.into()), 0) {
            Ok(hook) => hook,
            Err(e) => {
                let _ = ready.send(Err(format!("Windows would not let the program watch the mouse buttons: {e}")));
                return;
            }
        };
        let keyboard = if keys {
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(module.into()), 0) {
                Ok(hook) => Some(hook),
                Err(e) => {
                    let _ = UnhookWindowsHookEx(mouse);
                    let _ = ready.send(Err(format!("Windows would not let the program watch the keyboard: {e}")));
                    return;
                }
            }
        } else {
            None
        };
        let _ = ready.send(Ok(GetCurrentThreadId()));
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            DispatchMessageW(&message);
        }
        let _ = UnhookWindowsHookEx(mouse);
        if let Some(keyboard) = keyboard {
            let _ = UnhookWindowsHookEx(keyboard);
        }
    }
    // Letting go of the sender tells the other thread that nothing more will come.
    HOOK.with(|slot| slot.borrow_mut().take());
}

/// What the collecting thread sends back.
enum Out {
    Line(u64, LogEvent),
    Notice(String, String),
}

/// The questions put to Windows' accessibility information.
struct WindowsLookups {
    automation: Option<IUIAutomation>,
}

impl Lookups for WindowsLookups {
    fn focus(&mut self) -> FocusKind {
        let Some(automation) = &self.automation else { return FocusKind::Unknown };
        unsafe {
            let Ok(element) = automation.GetFocusedElement() else { return FocusKind::Unknown };
            match (element.CurrentIsPassword(), element.CurrentControlType()) {
                (Ok(password), Ok(kind)) => classify_focus(password.as_bool(), kind.0),
                _ => FocusKind::Unknown,
            }
        }
    }

    fn element_at(&mut self, x: i32, y: i32) -> Option<(String, String)> {
        let automation = self.automation.as_ref()?;
        unsafe {
            let element = automation.ElementFromPoint(POINT { x, y }).ok()?;
            let name = element.CurrentName().ok()?.to_string();
            let role = role_name(element.CurrentControlType().ok()?.0).to_string();
            Some((name, role))
        }
    }
}

/// The window in front and where it is, or `None` when there is none.
fn foreground() -> Option<(isize, WindowState)> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut title = [0u16; 512];
        let length = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        let title = String::from_utf16_lossy(&title[..length.min(title.len())]);
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        let mut frame = RECT::default();
        let measured = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok()
            || GetWindowRect(hwnd, &mut frame).is_ok();
        let bounds = measured.then_some((frame.left, frame.top, frame.right - frame.left, frame.bottom - frame.top));
        Some((hwnd.0 as isize, WindowState { title, process: program_name(process_id), bounds }))
    }
}

/// Everything the collecting thread does until the hooks are gone.
fn collect(
    inbox: Receiver<Stamped>,
    outbox: Sender<Out>,
    setup: InteractionSetup,
    timeline: SharedTimeline,
    stop: Arc<AtomicBool>,
) {
    crate::win::prepare();
    let automation: Option<IUIAutomation> =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };
    if automation.is_none() {
        let _ = outbox.send(Out::Notice(
            "interactions-no-accessibility".into(),
            if setup.keys {
                "Windows' accessibility information is not available, so the names of the things you click are not noted, and no letters, digits or symbols you type are noted either (the recorder cannot check whether a field is a password field). Clicks, the cursor, the windows and keys such as Enter and shortcuts are still noted."
            } else {
                "Windows' accessibility information is not available, so the names of the things you click are not noted. Clicks, the cursor and the windows are still noted."
            }
            .into(),
        ));
    }
    let mut lookups = WindowsLookups { automation };
    let mut filter = InteractionFilter::new(setup.keys, setup.origin);
    let send = |lines: Vec<(u64, LogEvent)>| {
        for (t, event) in lines {
            let _ = outbox.send(Out::Line(t, event));
        }
    };
    let media_ms = |at: Instant| timeline.lock().unwrap_or_else(|e| e.into_inner()).media_ms_at(at);
    let mut last_cursor = Instant::now().checked_sub(CURSOR_EVERY).unwrap_or_else(Instant::now);
    let mut last_window = Instant::now().checked_sub(WINDOW_EVERY).unwrap_or_else(Instant::now);
    let mut last_hwnd = 0isize;
    loop {
        match inbox.recv_timeout(CURSOR_EVERY) {
            Ok(Stamped { ms, raw }) => send(filter.handle(ms, raw, &mut lookups)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if stop.load(Ordering::Acquire) {
            continue; // the hooks are being taken out; just let the queue run dry
        }
        let now = Instant::now();
        if now.duration_since(last_cursor) >= CURSOR_EVERY {
            last_cursor = now;
            if let Some(ms) = media_ms(now) {
                let mut at = POINT::default();
                if unsafe { GetCursorPos(&mut at) }.is_ok() {
                    send(filter.handle(ms, Raw::Move { x: at.x, y: at.y }, &mut lookups));
                }
                send(filter.tick(ms));
            }
        }
        if now.duration_since(last_window) >= WINDOW_EVERY {
            last_window = now;
            if let (Some(ms), Some((hwnd, state))) = (media_ms(now), foreground()) {
                if hwnd != last_hwnd {
                    last_hwnd = hwnd;
                    // Some programs (web browsers) only describe their fields once they are asked: ask as soon as
                    // the window comes to the front, so the answer is ready when a key is pressed.
                    if setup.keys {
                        let _ = lookups.focus();
                    }
                }
                send(filter.handle(ms, Raw::Window(state), &mut lookups));
            }
        }
    }
    send(filter.finish());
}

struct WindowsSource {
    outbox: Receiver<Out>,
    dropped: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    hook_thread_id: u32,
    hooks: Option<JoinHandle<()>>,
    collector: Option<JoinHandle<()>>,
}

impl InteractionSource for WindowsSource {
    fn drain(&mut self) -> Drained {
        let mut drained = Drained { dropped: self.dropped.load(Ordering::Relaxed), ..Drained::default() };
        while let Ok(item) = self.outbox.try_recv() {
            match item {
                Out::Line(t, event) => drained.lines.push((t, event)),
                Out::Notice(code, message) => drained.notices.push((code, message)),
            }
        }
        drained
    }
}

impl Drop for WindowsSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Taking the hooks out is quick: ask the hook thread to leave its message loop and wait for it.
        unsafe {
            let _ = PostThreadMessageW(self.hook_thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(hooks) = self.hooks.take() {
            let _ = hooks.join();
        }
        // The collecting thread may be waiting for a program that does not answer. The end of a recording does not
        // wait for that: give it half a second and let it finish by itself.
        if let Some(collector) = self.collector.take() {
            for _ in 0..50 {
                if collector.is_finished() {
                    let _ = collector.join();
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

impl InteractionPlatform for WindowsInteractions {
    fn start(&self, setup: &InteractionSetup, timeline: SharedTimeline) -> Result<Box<dyn InteractionSource>, String> {
        let (tx, inbox) = mpsc::sync_channel::<Stamped>(QUEUE);
        let dropped = Arc::new(AtomicU64::new(0));
        let state = HookState { tx, timeline: Arc::clone(&timeline), dropped: Arc::clone(&dropped) };
        let (ready_tx, ready_rx) = mpsc::channel();
        let keys = setup.keys;
        let hooks = std::thread::spawn(move || run_hooks(keys, state, ready_tx));
        let hook_thread_id = match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(id)) => id,
            Ok(Err(why)) => {
                let _ = hooks.join();
                return Err(why);
            }
            Err(_) => return Err("Windows did not set up the watching of the mouse in time.".to_string()),
        };
        let (out_tx, outbox) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let collector = {
            let (setup, stop) = (*setup, Arc::clone(&stop));
            std::thread::spawn(move || collect(inbox, out_tx, setup, timeline, stop))
        };
        Ok(Box::new(WindowsSource {
            outbox,
            dropped,
            stop,
            hook_thread_id,
            hooks: Some(hooks),
            collector: Some(collector),
        }))
    }
}
