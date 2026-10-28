//! Ties the companion's lifetime to DevTools (`--parent <pid>`).

use std::ffi::c_void;
use std::thread;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::hooks::WM_APP_SHUTDOWN;

/// Watches `pid` on a background thread and posts a shutdown message to the
/// engine when it exits. Returns false when the parent is already gone.
pub fn watch(pid: u32, engine: HWND) -> bool {
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return false;
    };

    let raw_engine = engine.0 as isize;
    let raw_handle = handle.0 as isize;
    let spawned = thread::Builder::new()
        .name("snapit-parent-watch".into())
        .spawn(move || {
            let handle = HANDLE(raw_handle as *mut c_void);
            unsafe {
                let _ = WaitForSingleObject(handle, INFINITE);
                let _ = PostMessageW(
                    Some(HWND(raw_engine as *mut c_void)),
                    WM_APP_SHUTDOWN,
                    WPARAM(0),
                    LPARAM(0),
                );
                let _ = CloseHandle(handle);
            }
        });

    if spawned.is_err() {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return false;
    }

    true
}
