//! Thread priority for the live path. The audio callback already runs time-critical
//! (cpal's WASAPI host sets it); the engine tick and the render loop run above normal, so
//! preparation work, the app's web view or other programs can't delay a beat or a frame.

/// Raises the calling thread to above-normal priority. Returns whether it changed.
#[must_use]
pub fn raise_current_thread() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
        };
        // SAFETY: GetCurrentThread returns a pseudo-handle valid for this thread.
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL) != 0 }
    }
    #[cfg(not(windows))]
    false
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn raises_only_the_calling_thread() {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, GetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
            THREAD_PRIORITY_NORMAL,
        };
        // SAFETY: pseudo-handle for the calling thread.
        let priority = || unsafe { GetThreadPriority(GetCurrentThread()) };
        std::thread::spawn(move || {
            assert!(super::raise_current_thread());
            assert_eq!(priority(), THREAD_PRIORITY_ABOVE_NORMAL);
        })
        .join()
        .unwrap();
        assert_eq!(priority(), THREAD_PRIORITY_NORMAL);
    }
}
