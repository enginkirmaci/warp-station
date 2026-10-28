//! Global low-level mouse/keyboard hooks.
//!
//! The hook procedures never block: they update a shared latest-position slot
//! and post a coalesced message to the engine window. The engine drains the
//! latest position on its own thread, exactly like the .NET `MouseService`.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Result, anyhow};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    PostMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
};

use crate::settings::{HoldKey, MouseButton, Settings};

// Engine window messages.
pub const WM_APP_DRAG_START: u32 = WM_APP + 1;
pub const WM_APP_DRAG_END: u32 = WM_APP + 2;
pub const WM_APP_MOUSE_MOVE: u32 = WM_APP + 3;
pub const WM_APP_CANCEL: u32 = WM_APP + 4;
pub const WM_APP_SHUTDOWN: u32 = WM_APP + 5;

static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

/// Shared between the hook threads and the engine.
pub struct Runtime {
    engine: AtomicIsize,
    pub settings: Settings,
    latest: Mutex<(i32, i32)>,
    move_posted: AtomicBool,
    pub listening: AtomicBool,
    pub holding_key: AtomicBool,
    pub hold_key_used: AtomicBool,
}

impl Runtime {
    pub fn new(settings: Settings) -> Arc<Self> {
        Arc::new(Self {
            engine: AtomicIsize::new(0),
            settings,
            latest: Mutex::new((0, 0)),
            move_posted: AtomicBool::new(false),
            listening: AtomicBool::new(false),
            holding_key: AtomicBool::new(false),
            hold_key_used: AtomicBool::new(false),
        })
    }

    pub fn set_engine(&self, hwnd: HWND) {
        self.engine.store(hwnd.0 as isize, Ordering::Release);
    }

    pub fn latest_point(&self) -> (i32, i32) {
        *self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Clears the coalescing flag *before* reading the point, so a move that
    /// arrives while the engine is processing posts a fresh message.
    pub fn take_move(&self) -> (i32, i32) {
        self.move_posted.store(false, Ordering::Release);
        self.latest_point()
    }

    fn note_move(&self, x: i32, y: i32) {
        *self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = (x, y);

        if self
            .move_posted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            self.post(WM_APP_MOUSE_MOVE, 0, 0);
        }
    }

    fn post(&self, message: u32, x: i32, y: i32) {
        let raw = self.engine.load(Ordering::Acquire);
        if raw == 0 {
            return;
        }
        let hwnd = HWND(raw as *mut c_void);
        let _ = unsafe {
            PostMessageW(
                Some(hwnd),
                message,
                WPARAM(x as u32 as usize),
                LPARAM(y as u32 as isize),
            )
        };
    }

    fn is_configured_down(&self, message: u32, mouse_data: u32) -> bool {
        match self.settings.mouse_button {
            MouseButton::Left => message == WM_LBUTTONDOWN,
            MouseButton::Right => message == WM_RBUTTONDOWN,
            MouseButton::Middle => message == WM_MBUTTONDOWN,
            MouseButton::XButton1 => message == WM_XBUTTONDOWN && x_button(mouse_data) == XBUTTON1,
            MouseButton::XButton2 => message == WM_XBUTTONDOWN && x_button(mouse_data) == XBUTTON2,
        }
    }

    fn is_configured_up(&self, message: u32, mouse_data: u32) -> bool {
        match self.settings.mouse_button {
            MouseButton::Left => message == WM_LBUTTONUP,
            MouseButton::Right => message == WM_RBUTTONUP,
            MouseButton::Middle => message == WM_MBUTTONUP,
            MouseButton::XButton1 => message == WM_XBUTTONUP && x_button(mouse_data) == XBUTTON1,
            MouseButton::XButton2 => message == WM_XBUTTONUP && x_button(mouse_data) == XBUTTON2,
        }
    }

    fn is_hold_key(&self, vk: u16) -> bool {
        match self.settings.hold_key {
            HoldKey::Control => vk == VK_LCONTROL.0 || vk == VK_RCONTROL.0,
            HoldKey::Alt => vk == VK_LMENU.0 || vk == VK_RMENU.0,
            HoldKey::Shift => vk == VK_LSHIFT.0 || vk == VK_RSHIFT.0,
            HoldKey::Win => vk == VK_LWIN.0 || vk == VK_RWIN.0,
        }
    }
}

fn x_button(mouse_data: u32) -> u16 {
    ((mouse_data >> 16) & 0xffff) as u16
}

fn runtime() -> Option<&'static Arc<Runtime>> {
    RUNTIME.get()
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let Some(runtime) = runtime() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    let message = wparam.0 as u32;
    let data = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };

    match message {
        WM_MOUSEMOVE => {
            if runtime.listening.load(Ordering::Relaxed) {
                runtime.note_move(data.pt.x, data.pt.y);
            }
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
            if runtime.is_configured_down(message, data.mouseData) {
                runtime.listening.store(true, Ordering::Release);
                runtime.post(WM_APP_DRAG_START, data.pt.x, data.pt.y);
            }
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP
            if runtime.is_configured_up(message, data.mouseData)
                && runtime.listening.swap(false, Ordering::AcqRel) =>
        {
            // Carry the actual button in the message: the engine may process
            // this after a newer drag has already started.
            let was_left = (message == WM_LBUTTONUP) as i32;
            runtime.post(WM_APP_DRAG_END, was_left, 0);
        }
        _ => {}
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let Some(runtime) = runtime() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    let message = wparam.0 as u32;
    let data = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let vk = data.vkCode as u16;

    if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
        if vk == VK_ESCAPE.0 {
            runtime.post(WM_APP_CANCEL, 0, 0);
        }

        if runtime.settings.enable_hold_key && runtime.is_hold_key(vk) {
            runtime.holding_key.store(true, Ordering::Release);
        }
    } else if (message == WM_KEYUP || message == WM_SYSKEYUP) && runtime.settings.enable_hold_key {
        let mut suppress = false;
        if runtime.is_hold_key(vk) {
            runtime.holding_key.store(false, Ordering::Release);

            // Like the .NET hook: swallow the Win key-up that ended a snap so
            // the Start menu does not open after a Win+drag.
            suppress = runtime.settings.hold_key == HoldKey::Win
                && runtime.hold_key_used.load(Ordering::Acquire);
        }

        // The .NET handler clears the flag on every key-up, but only
        // suppresses the Win key-up itself.
        runtime.hold_key_used.store(false, Ordering::Release);
        if suppress {
            return LRESULT(1);
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub struct Hooks {
    thread_ids: Vec<u32>,
    threads: Vec<JoinHandle<()>>,
}

impl Hooks {
    pub fn install(runtime: Arc<Runtime>) -> Result<Self> {
        if RUNTIME.set(runtime).is_err() {
            return Err(anyhow!("hook runtime already installed"));
        }

        let (mouse_tx, mouse_rx) = mpsc::channel();
        let mouse_thread = thread::Builder::new()
            .name("snapit-mouse-hook".into())
            .spawn(move || run_mouse_hook_thread(mouse_tx))?;

        let mouse_thread_id = mouse_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow!("mouse hook thread did not start"))??;

        let (keyboard_tx, keyboard_rx) = mpsc::channel();
        let keyboard_thread = thread::Builder::new()
            .name("snapit-keyboard-hook".into())
            .spawn(move || run_keyboard_hook_thread(keyboard_tx))?;

        let keyboard_thread_id = keyboard_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow!("keyboard hook thread did not start"))??;

        Ok(Self {
            thread_ids: vec![mouse_thread_id, keyboard_thread_id],
            threads: vec![mouse_thread, keyboard_thread],
        })
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        for thread_id in self.thread_ids.drain(..) {
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn run_mouse_hook_thread(tx: mpsc::Sender<Result<u32>>) {
    unsafe {
        let module = match GetModuleHandleW(None) {
            Ok(module) => module,
            Err(error) => {
                let _ = tx.send(Err(anyhow!("GetModuleHandleW failed: {error}")));
                return;
            }
        };

        let hook =
            match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(HINSTANCE(module.0)), 0) {
                Ok(hook) => hook,
                Err(error) => {
                    let _ = tx.send(Err(anyhow!(
                        "SetWindowsHookExW(WH_MOUSE_LL) failed: {error}"
                    )));
                    return;
                }
            };

        let thread_id = GetCurrentThreadId();
        let _ = tx.send(Ok(thread_id));
        drop(tx);

        message_loop();
        let _ = UnhookWindowsHookEx(hook);
    }
}

fn run_keyboard_hook_thread(tx: mpsc::Sender<Result<u32>>) {
    unsafe {
        let module = match GetModuleHandleW(None) {
            Ok(module) => module,
            Err(error) => {
                let _ = tx.send(Err(anyhow!("GetModuleHandleW failed: {error}")));
                return;
            }
        };

        let hook = match SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            Some(HINSTANCE(module.0)),
            0,
        ) {
            Ok(hook) => hook,
            Err(error) => {
                let _ = tx.send(Err(anyhow!(
                    "SetWindowsHookExW(WH_KEYBOARD_LL) failed: {error}"
                )));
                return;
            }
        };

        let thread_id = GetCurrentThreadId();
        let _ = tx.send(Ok(thread_id));
        drop(tx);

        message_loop();
        let _ = UnhookWindowsHookEx(hook);
    }
}

unsafe fn message_loop() {
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
