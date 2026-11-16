//! Thread priority for the live path. The audio callback already runs time-critical
//! (cpal's WASAPI host sets it). The engine tick joins Windows' multimedia scheduler (MMCSS)
//! as an audio task, and the render loop runs above normal, so preparation work, the app's
//! web view or other programs can't delay a beat or a frame.

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

/// Registers the calling thread with Windows' multimedia scheduler as an "Audio" task, for
/// the rest of its life; falls back to above-normal priority when the scheduler refuses.
/// Returns whether either took effect.
#[must_use]
pub fn join_audio_class() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
        let task: Vec<u16> = "Audio\0".encode_utf16().collect();
        let mut index = 0u32;
        // SAFETY: `task` is a NUL-terminated UTF-16 string and `index` a valid out pointer.
        // The registration is kept until the thread ends, so the handle is not reverted.
        let handle = unsafe { AvSetMmThreadCharacteristicsW(task.as_ptr(), &raw mut index) };
        if !handle.is_null() {
            return true;
        }
        tracing::debug!(
            "multimedia scheduling unavailable: {}",
            std::io::Error::last_os_error()
        );
    }
    raise_current_thread()
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

    #[cfg(windows)]
    #[test]
    fn an_audio_task_runs_above_normal() {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, GetThreadPriority, THREAD_PRIORITY_NORMAL,
        };
        std::thread::spawn(|| {
            assert!(super::join_audio_class());
            // SAFETY: pseudo-handle for the calling thread.
            let priority = unsafe { GetThreadPriority(GetCurrentThread()) };
            assert!(priority > THREAD_PRIORITY_NORMAL, "priority {priority}");
        })
        .join()
        .unwrap();
    }
}
