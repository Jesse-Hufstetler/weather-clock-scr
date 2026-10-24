//! A picture of the user's desktop, assembled by asking each of its windows to draw itself (PrintWindow).
//!
//! Needed because a screensaver that Windows starts itself runs on a separate, empty desktop, where it can neither see nor
//! switch to the user's desktop; the real desktop (and its windows) still exist on the other desktop though, and a thread
//! can attach to it and talk to those windows. The result is only an approximation of the screen (no cursor, no hover
//! states, windows that refuse to draw offscreen are skipped), which is fine for a half-second fade.

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Storage::Xps::PrintWindow;
use windows_sys::Win32::System::StationsAndDesktops::{OpenDesktopW, SetThreadDesktop};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub struct Shot {
    pub w: i32,
    pub h: i32,
    /// Bottom-up BGRA, like an OpenGL texture and like Renderer::set_outside expects.
    pub bgra: Vec<u8>,
    /// How many windows were drawn into it (for the log).
    pub windows: usize,
}

struct Ctx {
    wins: Vec<HWND>,
    mon: RECT,
    pid: u32,
}

unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> i32 {
    let c = &mut *(lp as *mut Ctx);
    if IsWindowVisible(h) == 0 || IsIconic(h) != 0 {
        return 1;
    }
    let mut cloaked: u32 = 0;
    DwmGetWindowAttribute(h, 14 /* DWMWA_CLOAKED */, &mut cloaked as *mut _ as *mut _, 4);
    if cloaked != 0 {
        return 1;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, &mut pid);
    if pid == c.pid {
        return 1;
    }
    let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TRANSPARENT != 0 {
        return 1;
    }
    if ex & WS_EX_LAYERED != 0 {
        let (mut key, mut alpha, mut flags) = (0u32, 0u8, 0u32);
        if GetLayeredWindowAttributes(h, &mut key, &mut alpha, &mut flags) != 0 && flags & LWA_ALPHA != 0 && alpha < 8 {
            return 1;
        }
    }
    let mut r: RECT = std::mem::zeroed();
    GetWindowRect(h, &mut r);
    if r.right <= r.left || r.bottom <= r.top || r.right <= c.mon.left || r.left >= c.mon.right || r.bottom <= c.mon.top || r.top >= c.mon.bottom {
        return 1;
    }
    c.wins.push(h);
    1
}

/// Attach the calling thread to the user's "Default" desktop. Call once, on a fresh thread that owns no windows, before
/// capture_default_desktop.
pub unsafe fn attach_default_desktop() -> Result<(), String> {
    let name: Vec<u16> = "Default\0".encode_utf16().collect();
    let d = OpenDesktopW(name.as_ptr(), 0, 0, 0x1000_0000 /* GENERIC_ALL */);
    if d.is_null() {
        return Err(format!("OpenDesktop(Default) failed, error {}", windows_sys::Win32::Foundation::GetLastError()));
    }
    if SetThreadDesktop(d) == 0 {
        return Err(format!("SetThreadDesktop(Default) failed, error {}", windows_sys::Win32::Foundation::GetLastError()));
    }
    Ok(())
}

/// Capture monitor `mon` of the desktop this thread is attached to (see attach_default_desktop).
pub unsafe fn capture_default_desktop(mon: RECT) -> Result<Shot, String> {
    let (w, h) = (mon.right - mon.left, mon.bottom - mon.top);
    let mut ctx = Ctx { wins: Vec::new(), mon, pid: GetCurrentProcessId() };
    EnumWindows(Some(collect), &mut ctx as *mut _ as LPARAM); // top of the z-order first

    let sdc = GetDC(std::ptr::null_mut());
    let canvas_dc = CreateCompatibleDC(sdc);
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = h; // bottom-up
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let canvas = CreateDIBSection(sdc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if canvas.is_null() || bits.is_null() {
        DeleteDC(canvas_dc);
        ReleaseDC(std::ptr::null_mut(), sdc);
        return Err("could not create the canvas bitmap".into());
    }
    let old = SelectObject(canvas_dc, canvas as _);
    // background: the desktop colour (a wallpaper is not drawn; windows that cover it usually hide it anyway)
    let brush = CreateSolidBrush(GetSysColor(COLOR_DESKTOP));
    let full = RECT { left: 0, top: 0, right: w, bottom: h };
    FillRect(canvas_dc, &full, brush);
    DeleteObject(brush as _);

    let mut drawn = 0usize;
    for &win in ctx.wins.iter().rev() {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(win, &mut r);
        let (ww, wh) = (r.right - r.left, r.bottom - r.top);
        let wdc = CreateCompatibleDC(sdc);
        let wbmp = CreateCompatibleBitmap(sdc, ww, wh);
        let wold = SelectObject(wdc, wbmp as _);
        // PW_RENDERFULLCONTENT (2): also windows that draw through DirectComposition / the GPU (Chromium, UWP)
        if PrintWindow(win, wdc, 2) != 0 {
            BitBlt(canvas_dc, r.left - mon.left, r.top - mon.top, ww, wh, wdc, 0, 0, SRCCOPY);
            drawn += 1;
        }
        SelectObject(wdc, wold);
        DeleteObject(wbmp as _);
        DeleteDC(wdc);
    }
    GdiFlush();
    let bgra = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
    SelectObject(canvas_dc, old);
    DeleteObject(canvas as _);
    DeleteDC(canvas_dc);
    ReleaseDC(std::ptr::null_mut(), sdc);
    Ok(Shot { w, h, bgra, windows: drawn })
}
