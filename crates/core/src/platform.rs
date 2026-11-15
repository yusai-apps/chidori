//! Small OS helpers: the QPC clock, OS build, display language, thread priority, cursor position.
//!
//! On macOS "QPC" is `mach_absolute_time` in 100 ns units: the clock Core Audio stamps buffers
//! with (`mHostTime`), so capture timestamps and `qpc_now_100ns` share one base as on Windows.

/// QueryPerformanceCounter in 100 ns units, the same base WASAPI uses for packet timestamps.
#[cfg(windows)]
pub fn qpc_now_100ns() -> u64 {
    use std::sync::OnceLock;
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    static FREQ: OnceLock<i64> = OnceLock::new();
    let freq = *FREQ.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: plain out-parameter.
        unsafe { QueryPerformanceFrequency(&mut f) }.ok();
        f.max(1)
    });
    let mut c = 0i64;
    // SAFETY: plain out-parameter.
    unsafe { QueryPerformanceCounter(&mut c) }.ok();
    ((c as i128) * 10_000_000 / freq as i128) as u64
}

#[cfg(target_os = "macos")]
pub fn qpc_now_100ns() -> u64 {
    mac::host_time_to_100ns(mac::host_time_now())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn qpc_now_100ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    (START.get_or_init(Instant::now).elapsed().as_nanos() / 100) as u64
}

/// Windows build number (e.g. 26100), via RtlGetVersion so the manifest cannot hide it.
#[cfg(windows)]
pub fn os_build() -> u32 {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut info =
        OSVERSIONINFOW { dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
    // SAFETY: `info` is a properly sized OSVERSIONINFOW.
    let status = unsafe { RtlGetVersion(&mut info) };
    if status.is_ok() { info.dwBuildNumber } else { 0 }
}

/// macOS has no build number to compare: its major version (e.g. 26) instead.
#[cfg(target_os = "macos")]
pub fn os_build() -> u32 {
    sysinfo::System::os_version().and_then(|v| v.split('.').next().and_then(|m| m.parse().ok())).unwrap_or(0)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn os_build() -> u32 {
    0
}

/// Whether Windows' display language (the user's UI language) is Japanese.
#[cfg(windows)]
pub fn display_language_is_japanese() -> bool {
    const LANG_JAPANESE: u16 = 0x11;
    // SAFETY: takes no arguments and only returns a language ID.
    let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    lang & 0x3ff == LANG_JAPANESE
}

/// Whether the first of the user's preferred languages (System Settings › Language) is Japanese.
#[cfg(target_os = "macos")]
pub fn display_language_is_japanese() -> bool {
    std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|list| {
            list.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).find(|w| !w.is_empty()).map(str::to_owned)
        })
        .is_none_or(|first| first.starts_with("ja"))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn display_language_is_japanese() -> bool {
    true
}

/// Per-app capture needs build 20348 or later (in practice Windows 11).
pub const APP_LOOPBACK_MIN_BUILD: u32 = 20348;

/// One app's sound on its own. Not yet on macOS, which records the whole system or the mic.
pub fn app_loopback_supported() -> bool {
    cfg!(windows) && os_build() >= APP_LOOPBACK_MIN_BUILD
}

/// Raises the calling thread to MMCSS "Pro Audio". Keep the guard alive for the thread.
#[cfg(windows)]
pub struct MmcssGuard(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for MmcssGuard {
    fn drop(&mut self) {
        use windows::Win32::System::Threading::AvRevertMmThreadCharacteristics;
        // SAFETY: the handle came from AvSetMmThreadCharacteristicsW on this thread.
        let _ = unsafe { AvRevertMmThreadCharacteristics(self.0) };
    }
}

#[cfg(windows)]
pub fn raise_to_pro_audio() -> Option<MmcssGuard> {
    use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
    use windows::core::w;
    let mut index = 0u32;
    // SAFETY: valid task name and out-parameter.
    match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) } {
        Ok(h) => Some(MmcssGuard(h)),
        Err(err) => {
            tracing::warn!("MMCSS Pro Audio failed: {err}");
            None
        }
    }
}

/// Cursor position in physical pixels.
#[cfg(windows)]
pub fn cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    // SAFETY: plain out-parameter.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((p.x, p.y))
}

/// Cursor position in the global display space xcap's `Monitor::from_point` takes (points).
#[cfg(target_os = "macos")]
pub fn cursor_pos() -> Option<(i32, i32)> {
    let (x, y) = mac::cursor_location()?;
    Some((x.round() as i32, y.round() as i32))
}

/// No cursor position on other OSes yet: screenshots are Windows and macOS only.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}

/// True while a process with this PID exists and has not exited.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// True while a process with this PID exists and has not exited.
#[cfg(target_os = "macos")]
pub fn process_alive(pid: u32) -> bool {
    mac::process_alive(pid)
}

/// True while a process with this PID exists and has not exited.
#[cfg(windows)]
pub fn process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: standard handle lifecycle; closed below.
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code).is_ok();
        let _ = CloseHandle(h);
        ok && code == STILL_ACTIVE.0 as u32
    }
}

/// Plays a WAV file from memory without waiting. A newer sound cuts off one still playing.
#[cfg(windows)]
pub fn play_wav(wav: &'static [u8]) {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    use windows::core::PCWSTR;
    // SAFETY: with SND_MEMORY the pointer is the WAV data, which is 'static, so it outlives the
    // asynchronous playback. A sound that fails to play is not worth reporting.
    let _ = unsafe { PlaySoundW(PCWSTR(wav.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
}

/// Plays a WAV file from memory without waiting. A newer sound cuts off one still playing.
#[cfg(target_os = "macos")]
pub fn play_wav(wav: &'static [u8]) {
    mac::play_wav(wav);
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn play_wav(_wav: &'static [u8]) {}

/// Total physical memory in bytes.
pub fn total_memory() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.total_memory()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    #[repr(C)]
    #[derive(Default)]
    struct TimebaseInfo {
        numer: u32,
        denom: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    unsafe extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut TimebaseInfo) -> i32;
        fn kill(pid: i32, sig: i32) -> i32;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *mut c_void);
    }

    pub fn host_time_now() -> u64 {
        // SAFETY: takes no arguments.
        unsafe { mach_absolute_time() }
    }

    /// Host time ticks to 100 ns units (ticks are 1 ns on Intel, 1/24 µs on Apple Silicon).
    pub fn host_time_to_100ns(ticks: u64) -> u64 {
        static BASE: OnceLock<(u32, u32)> = OnceLock::new();
        let (numer, denom) = *BASE.get_or_init(|| {
            let mut info = TimebaseInfo::default();
            // SAFETY: plain out-parameter.
            unsafe { mach_timebase_info(&mut info) };
            (info.numer.max(1), info.denom.max(1))
        });
        ((ticks as u128 * numer as u128) / (denom as u128 * 100)) as u64
    }

    pub fn cursor_location() -> Option<(f64, f64)> {
        // SAFETY: a null source gives a fresh event carrying the current mouse location; it is
        // released after the read.
        unsafe {
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return None;
            }
            let p = CGEventGetLocation(event);
            CFRelease(event);
            Some((p.x, p.y))
        }
    }

    pub fn process_alive(pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else { return false };
        // SAFETY: signal 0 only checks that the process exists. EPERM still means it exists.
        pid > 0 && (unsafe { kill(pid, 0) } == 0 || std::io::Error::last_os_error().raw_os_error() == Some(1))
    }

    /// `afplay` reads the sound from a file, so each sound is written once to the temp folder.
    pub fn play_wav(wav: &'static [u8]) {
        use std::sync::Mutex;
        static PLAYING: Mutex<Option<std::process::Child>> = Mutex::new(None);
        let path = std::env::temp_dir().join(format!("chidori-sound-{:x}.wav", wav.as_ptr() as usize));
        if !path.exists() && std::fs::write(&path, wav).is_err() {
            return;
        }
        let Ok(mut playing) = PLAYING.lock() else { return };
        if let Some(mut old) = playing.take() {
            let _ = old.kill();
            let _ = old.wait();
        }
        *playing = std::process::Command::new("/usr/bin/afplay").arg(&path).spawn().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qpc_is_monotonic_and_in_100ns_units() {
        let a = qpc_now_100ns();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let b = qpc_now_100ns();
        let elapsed_ms = (b - a) / 10_000;
        assert!((15..200).contains(&elapsed_ms), "{elapsed_ms}");
    }

    #[cfg(windows)]
    #[test]
    fn os_build_is_reported() {
        assert!(os_build() > 10_000);
    }

    #[test]
    fn own_process_is_alive() {
        assert!(process_alive(std::process::id()));
        assert!(!process_alive(0xFFFF_FFF0));
    }
}
