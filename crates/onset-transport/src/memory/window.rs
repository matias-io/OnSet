//! Brings a process's main window to the front. rekordbox's playback clock stalled while
//! it sat behind OnSet during a calibration (2026-10-22: the elapsed time read 0:02.6 twenty
//! seconds after resuming at 0:04.7), so the calibrator raises it before every step in which
//! the deck has to play. This moves a window; it reads and writes nothing in the process.
#![allow(unsafe_code)]

use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

/// The first visible, unowned top-level window of `pid`: its main window.
fn main_window(pid: u32) -> Option<HWND> {
    struct Search {
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        // SAFETY: `data` is the `Search` passed to `EnumWindows` below, alive for the call.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut owner_pid = 0u32;
        // SAFETY: plain queries on a window handle the system just handed us.
        unsafe { GetWindowThreadProcessId(hwnd, &raw mut owner_pid) };
        let main = owner_pid == search.pid
            && unsafe { IsWindowVisible(hwnd) } != 0
            && unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
        if main {
            search.found = hwnd;
            return 0; // stop enumerating
        }
        TRUE
    }
    let mut search = Search {
        pid,
        found: std::ptr::null_mut(),
    };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe { EnumWindows(Some(visit), (&raw mut search) as LPARAM) };
    (!search.found.is_null()).then_some(search.found)
}

/// Restores and raises `pid`'s main window. True when it is the foreground window afterwards;
/// Windows refuses the switch when the caller is not itself in the foreground.
pub fn bring_to_front(pid: u32) -> bool {
    let Some(hwnd) = main_window(pid) else {
        return false;
    };
    // SAFETY: plain window calls on a handle that belongs to a live top-level window.
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
        GetForegroundWindow() == hwnd
    }
}
