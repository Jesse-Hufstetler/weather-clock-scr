//! Diagnostic log for the screensaver (`%APPDATA%\wxscr\saver.log`): the window messages that can end or disturb a run,
//! a timeline of the fade-out, frame stalls and a slow heartbeat. It exists to find problems that only show up after the
//! screensaver has been running for a long time. Cheap enough to leave on; the file is trimmed when it gets large.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, GetThreadDesktop, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static ON: AtomicBool = AtomicBool::new(false);

fn path() -> std::path::PathBuf {
    crate::config::dir().join("saver.log")
}

/// Start logging (call once, in a screensaver run). Trims an oversized old log first.
pub fn enable() {
    if let Ok(meta) = std::fs::metadata(path()) {
        if meta.len() > 1_000_000 {
            if let Ok(s) = std::fs::read(path()) {
                let keep = &s[s.len() - 300_000..];
                let _ = std::fs::write(path(), keep);
            }
        }
    }
    ON.store(true, Ordering::Relaxed);
}

pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

pub fn log(msg: &str) {
    if !on() {
        return;
    }
    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path()) {
        let _ = writeln!(f, "{:02}-{:02} {:02}:{:02}:{:02}.{:03} {msg}", st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, st.wMilliseconds);
    }
}

fn wide_to_string(buf: &[u16]) -> String {
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

/// "class 'title'" of a window, or "none".
pub unsafe fn window_desc(h: HWND) -> String {
    if h.is_null() {
        return "none".into();
    }
    let mut cls = [0u16; 128];
    let mut title = [0u16; 128];
    GetClassNameW(h, cls.as_mut_ptr(), 128);
    GetWindowTextW(h, title.as_mut_ptr(), 128);
    format!("{:?} {} '{}'", h, wide_to_string(&cls), wide_to_string(&title))
}

/// Name of the desktop that currently receives input ("Default" normally, "Winlogon" on the lock / sign-in screen).
pub unsafe fn input_desktop() -> String {
    let d = OpenInputDesktop(0, 0, 0x0001 /* DESKTOP_READOBJECTS */);
    if d.is_null() {
        return format!("(cannot open: error {})", windows_sys::Win32::Foundation::GetLastError());
    }
    let mut buf = [0u16; 64];
    let mut needed = 0u32;
    GetUserObjectInformationW(d, UOI_NAME, buf.as_mut_ptr() as *mut _, 128, &mut needed);
    CloseDesktop(d);
    wide_to_string(&buf)
}

/// One line about the current state of the world, for the heartbeat and for the start of a fade-out.
pub unsafe fn context() -> String {
    format!("foreground={} input-desktop={}", window_desc(GetForegroundWindow()), input_desktop())
}

/// Log the window messages that matter for how a run ends. Does not handle them.
pub unsafe fn message(msg: u32, wp: WPARAM, lp: LPARAM) {
    let text = match msg {
        WM_ACTIVATEAPP => format!("WM_ACTIVATEAPP active={} thread={}", wp, lp),
        WM_ACTIVATE => format!("WM_ACTIVATE state={} minimized={} other={}", wp & 0xFFFF, (wp >> 16) & 0xFFFF, window_desc(lp as HWND)),
        WM_NCACTIVATE => format!("WM_NCACTIVATE active={}", wp),
        WM_SETFOCUS => "WM_SETFOCUS".into(),
        WM_KILLFOCUS => format!("WM_KILLFOCUS new-focus={}", window_desc(wp as HWND)),
        WM_SHOWWINDOW => format!("WM_SHOWWINDOW show={} status={}", wp, lp),
        WM_WINDOWPOSCHANGED => {
            let w = &*(lp as *const WINDOWPOS);
            format!("WM_WINDOWPOSCHANGED pos=({},{}) size={}x{} flags=0x{:x} after={:?}", w.x, w.y, w.cx, w.cy, w.flags, w.hwndInsertAfter)
        }
        WM_DISPLAYCHANGE => format!("WM_DISPLAYCHANGE bpp={} size={}x{}", wp, lp & 0xFFFF, (lp >> 16) & 0xFFFF),
        WM_POWERBROADCAST => format!("WM_POWERBROADCAST event=0x{:x} data={}", wp, lp),
        WM_SYSCOMMAND => format!("WM_SYSCOMMAND cmd=0x{:x} lp={}", wp & 0xFFF0, lp),
        WM_CLOSE => "WM_CLOSE".into(),
        WM_DESTROY => "WM_DESTROY".into(),
        WM_QUERYENDSESSION => format!("WM_QUERYENDSESSION flags=0x{:x}", lp),
        WM_ENDSESSION => format!("WM_ENDSESSION ending={} flags=0x{:x}", wp, lp),
        WM_SETTINGCHANGE => {
            let what = if lp != 0 {
                let p = lp as *const u16;
                let mut v = Vec::new();
                for i in 0..64 {
                    let c = *p.add(i);
                    if c == 0 {
                        break;
                    }
                    v.push(c);
                }
                String::from_utf16_lossy(&v)
            } else {
                String::new()
            };
            format!("WM_SETTINGCHANGE wp=0x{:x} '{}'", wp, what)
        }
        WM_DEVICECHANGE => format!("WM_DEVICECHANGE event=0x{:x}", wp),
        0x02B1 => format!("WM_WTSSESSION_CHANGE event={} (1 connect, 2 disconnect, 5 logon, 6 logoff, 7 lock, 8 unlock) session={}", wp, lp),
        0x02E0 => "WM_DPICHANGED".into(),
        0x031A => "WM_THEMECHANGED".into(),
        0x031E => "WM_DWMCOMPOSITIONCHANGED".into(),
        WM_CAPTURECHANGED => "WM_CAPTURECHANGED".into(),
        _ => return,
    };
    log(&text);
}

/// Name of the desktop the calling thread is attached to.
pub unsafe fn thread_desktop() -> String {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    let d = GetThreadDesktop(GetCurrentThreadId());
    if d.is_null() {
        return "(none)".into();
    }
    let mut buf = [0u16; 64];
    let mut needed = 0u32;
    GetUserObjectInformationW(d, UOI_NAME, buf.as_mut_ptr() as *mut _, 128, &mut needed);
    wide_to_string(&buf)
}

/// Mean colour of the whole screen as seen from the calling thread's desktop (a 48x27 grab through GDI), as text.
unsafe fn screen_mean() -> String {
    use windows_sys::Win32::Graphics::Gdi::*;
    let sdc = GetDC(std::ptr::null_mut());
    if sdc.is_null() {
        return "no screen DC".into();
    }
    let mdc = CreateCompatibleDC(sdc);
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = 48;
    bi.bmiHeader.biHeight = -27;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(sdc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    let mut out = "grab failed".to_string();
    if !bmp.is_null() && !bits.is_null() {
        let old = SelectObject(mdc, bmp as _);
        SetStretchBltMode(mdc, HALFTONE);
        let (sw, sh) = (GetDeviceCaps(sdc, HORZRES as i32), GetDeviceCaps(sdc, VERTRES as i32));
        if StretchBlt(mdc, 0, 0, 48, 27, sdc, 0, 0, sw, sh, SRCCOPY) != 0 {
            let px = std::slice::from_raw_parts(bits as *const u8, 48 * 27 * 4);
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for p in px.chunks_exact(4) {
                b += p[0] as u32;
                g += p[1] as u32;
                r += p[2] as u32;
            }
            let n = 48 * 27;
            out = format!("mean rgb=({}, {}, {}) of {}x{}", r / n, g / n, b / n, sw, sh);
        }
        SelectObject(mdc, old);
    }
    if !bmp.is_null() {
        DeleteObject(bmp as _);
    }
    DeleteDC(mdc);
    ReleaseDC(std::ptr::null_mut(), sdc);
    out
}

/// Log which desktops exist, which one is active, and what a screen grab shows from this thread's desktop and from the
/// "Default" desktop (done on a short-lived helper thread, since a thread that owns windows cannot change desktop).
pub unsafe fn probe(label: &str) {
    if !on() {
        return;
    }
    let here = screen_mean();
    log(&format!("probe [{label}]: thread desktop='{}' input desktop='{}'; screen grab from this desktop: {here}", thread_desktop(), input_desktop()));
    let label = label.to_string();
    let h = std::thread::spawn(move || {
        use windows_sys::Win32::System::StationsAndDesktops::{OpenDesktopW, SetThreadDesktop};
        let name: Vec<u16> = "Default\0".encode_utf16().collect();
        let d = OpenDesktopW(name.as_ptr(), 0, 0, 0x1000_0000);
        if d.is_null() {
            log(&format!("probe [{label}]: OpenDesktop(Default) failed, error {}", windows_sys::Win32::Foundation::GetLastError()));
            return;
        }
        if SetThreadDesktop(d) == 0 {
            log(&format!("probe [{label}]: SetThreadDesktop(Default) failed, error {}", windows_sys::Win32::Foundation::GetLastError()));
            return;
        }
        log(&format!("probe [{label}]: screen grab from the Default desktop: {}", screen_mean()));
    });
    let _ = h.join();
}
