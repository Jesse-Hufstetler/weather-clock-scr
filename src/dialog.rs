//! Settings dialog (`/c`), built from plain Win32 controls so there are no resources to ship.

use crate::bg;
use crate::config::Config;
use crate::render::{Frame, Renderer};
use crate::weather::{self, Weather};
use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::time::Instant;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CB_ADDSTRING_: u32 = 0x0143;
const CB_GETCURSEL_: u32 = 0x0147;
const CB_SETCURSEL_: u32 = 0x014E;
const BM_GETCHECK_: u32 = 0x00F0;
const BM_SETCHECK_: u32 = 0x00F1;
const TBM_GETPOS_: u32 = 0x0400;
const TBM_SETPOS_: u32 = 0x0405;
const TBM_SETRANGE_: u32 = 0x0406;
const WM_HSCROLL_: u32 = 0x0114;

const ID_LOC: i32 = 101;
const ID_LOOKUP: i32 = 102;
const ID_STATUS: i32 = 103;
const ID_UNITS: i32 = 104;
const ID_H24: i32 = 105;
const ID_SECS: i32 = 106;
const ID_DETAILS: i32 = 107;
const ID_BG: i32 = 108;
const ID_FPS: i32 = 110;
const ID_BRIGHT: i32 = 111;
const ID_MOVE: i32 = 112;
const ID_MON: i32 = 113;
const ID_REFRESH: i32 = 114;
const ID_FONT: i32 = 115;
const ID_PREVIEW: i32 = 116;
const ID_BRIGHT_V: i32 = 117;
const ID_MOVE_V: i32 = 118;
const ID_AUTO: i32 = 122;
/// Posted by the detection thread when `DETECT_RESULT` is ready.
const WM_DETECTED: u32 = WM_APP + 1;

const FPS_OPTS: [u32; 3] = [0, 60, 30];
const FPS_NAMES: [&str; 3] = ["Match display refresh rate (smoothest)", "Cap at 60 fps", "Cap at 30 fps"];
const REFRESH_OPTS: [u32; 4] = [10, 20, 30, 60];
const MON_OPTS: [&str; 3] = ["Clock on every monitor", "Clock on main monitor, background on others", "Clock on main monitor, others black"];
const FONT_OPTS: [&str; 5] = ["Droid Sans Mono (bundled)", "Segoe UI", "Consolas", "Bahnschrift", "Cascadia Mono"];

struct State {
    cfg: Config,
    resolved: Option<(f64, f64, String, String)>, // lat, lon, name, text it was resolved for
    dpi: i32,
    font: HFONT,
    /// Live preview: same renderer as the screensaver, driven by the dialog's current control values.
    preview: Option<Renderer>,
    wx: Weather,
    last_font: String,
    started: Instant,
    credit_bg: usize,
    detected: Option<weather::Detected>,
}

const ID_TIMER: usize = 1;
/// Design-unit (96 dpi) geometry. The options panel is fixed-width and scrolls when the window is short;
/// the preview takes whatever room is left, keeping 16:9.
const PANEL_W: i32 = 470;
const CONTENT_H: i32 = 418;
const BUTTON_ROW_H: i32 = 46;
const MIN_PREVIEW_W: i32 = 240;
const MARGIN: i32 = 14;
const ID_CAPTION: i32 = 119;
const ID_CREDIT: i32 = 120;
const ID_ABOUT: i32 = 121;
const CREDIT_H: i32 = 170;

/// The running fullscreen-size test, if any. The preview stops rendering meanwhile so the two don't compete for the GPU.
static TEST_RUN: Mutex<Option<std::process::Child>> = Mutex::new(None);

static DETECT_RESULT: Mutex<Option<Result<weather::Detected, String>>> = Mutex::new(None);

thread_local! {
    static ST: RefCell<Option<State>> = const { RefCell::new(None) };
    static PANEL: Cell<isize> = const { Cell::new(0) };
    static PREV: Cell<isize> = const { Cell::new(0) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Controls live in the scrolling panel; the footer buttons live directly on the dialog.
unsafe fn item(dlg: HWND, id: i32) -> HWND {
    let panel = PANEL.with(|c| c.get()) as HWND;
    let h = if panel.is_null() { null_mut() } else { GetDlgItem(panel, id) };
    if h.is_null() {
        GetDlgItem(dlg, id)
    } else {
        h
    }
}

unsafe fn get_text(h: HWND) -> String {
    let n = GetWindowTextLengthW(h);
    let mut buf = vec![0u16; n as usize + 1];
    GetWindowTextW(h, buf.as_mut_ptr(), n + 1);
    String::from_utf16_lossy(&buf[..n as usize])
}

unsafe fn set_text(h: HWND, s: &str) {
    SetWindowTextW(h, wide(s).as_ptr());
}

unsafe fn checked(dlg: HWND, id: i32) -> bool {
    SendMessageW(item(dlg, id), BM_GETCHECK_, 0, 0) == 1
}

unsafe fn sel(dlg: HWND, id: i32) -> usize {
    SendMessageW(item(dlg, id), CB_GETCURSEL_, 0, 0).max(0) as usize
}

/// Options controls, parented to the scrolling panel.
unsafe fn build(panel: HWND) {
    let (cfg, dpi, font) = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        (s.cfg.clone(), s.dpi, s.font)
    });
    let sc = |v: i32| v * dpi / 96;
    let hinst = GetModuleHandleW(null());
    let mk = |class: &str, text: &str, style: u32, ex: u32, x: i32, y: i32, w: i32, h: i32, id: i32| -> HWND {
        let hw = CreateWindowExW(ex, wide(class).as_ptr(), wide(text).as_ptr(), WS_CHILD | WS_VISIBLE | style, sc(x), sc(y), sc(w), sc(h), panel, id as isize as HMENU, hinst, null());
        SendMessageW(hw, WM_SETFONT, font as usize, 1);
        hw
    };
    let (lx, cx) = (14, 170);
    let mut y = 14;
    let label = |t: &str, y: i32| {
        mk("STATIC", t, 0, 0, lx, y + 3, 150, 18, 0);
    };

    label("Location", y);
    let auto = mk("BUTTON", "Detect automatically (Windows location, then IP)", BS_AUTOCHECKBOX as u32 | WS_TABSTOP, 0, cx, y, 290, 20, ID_AUTO);
    SendMessageW(auto, BM_SETCHECK_, cfg.auto_location as usize, 0);
    y += 26;
    // the box always shows something useful: the detected place when auto, else your override (pre-filled with the detection)
    let shown = if cfg.auto_location || cfg.location.is_empty() { cfg.detected_text.clone() } else { cfg.location.clone() };
    let dis = if cfg.auto_location { WS_DISABLED } else { 0 };
    mk("EDIT", &shown, (ES_AUTOHSCROLL as u32) | WS_TABSTOP | dis, WS_EX_CLIENTEDGE, cx, y, 170, 22, ID_LOC);
    mk("BUTTON", "Look up", BS_PUSHBUTTON as u32 | WS_TABSTOP | dis, 0, cx + 178, y - 1, 70, 24, ID_LOOKUP);
    y += 26;
    let status = if cfg.lat.is_finite() { format!("{}  ({:.2}, {:.2})", cfg.place, cfg.lat, cfg.lon) } else { "detecting\u{2026}".to_string() };
    mk("STATIC", &status, 0, 0, cx, y, 290, 34, ID_STATUS);
    y += 36;

    let chk = |t: &str, id: i32, on: bool, y: i32| {
        let h = mk("BUTTON", t, BS_AUTOCHECKBOX as u32 | WS_TABSTOP, 0, cx, y, 280, 20, id);
        SendMessageW(h, BM_SETCHECK_, on as usize, 0);
    };
    label("Units", y);
    chk("Fahrenheit and mph (uncheck for \u{b0}C, km/h)", ID_UNITS, cfg.fahrenheit, y);
    y += 24;
    label("Clock", y);
    chk("24-hour time", ID_H24, cfg.clock24, y);
    y += 22;
    chk("Show seconds", ID_SECS, cfg.seconds, y);
    y += 22;
    chk("Show high/low, rain, wind and sunset line", ID_DETAILS, cfg.details, y);
    y += 32;

    let combo = |id: i32, opts: &[&str], cur: usize, y: i32, style: u32| {
        let h = mk("COMBOBOX", "", style | WS_VSCROLL | WS_TABSTOP, 0, cx, y, 280, 200, id);
        for o in opts {
            SendMessageW(h, CB_ADDSTRING_, 0, wide(o).as_ptr() as isize);
        }
        SendMessageW(h, CB_SETCURSEL_, cur, 0);
        h
    };
    let mut names: Vec<&str> = bg::NAMES.to_vec();
    names.push("Random each time");
    label("Background", y);
    combo(ID_BG, &names, cfg.background.min(names.len() - 1), y, CBS_DROPDOWNLIST as u32);
    y += 30;
    label("Frame rate", y);
    combo(ID_FPS, &FPS_NAMES, FPS_OPTS.iter().position(|f| *f == cfg.fps).unwrap_or(0), y, CBS_DROPDOWNLIST as u32);
    y += 30;
    label("Brightness", y);
    let tb = |id: i32, vid: i32, lo: i32, hi: i32, val: i32, y: i32| {
        let h = mk("msctls_trackbar32", "", (TBS_HORZ | TBS_NOTICKS) as u32 | WS_TABSTOP, 0, cx - 4, y, 230, 24, id);
        SendMessageW(h, TBM_SETRANGE_, 1, ((hi << 16) | lo) as isize);
        SendMessageW(h, TBM_SETPOS_, 1, val as isize);
        mk("STATIC", &format!("{val}%"), 0, 0, cx + 236, y + 3, 44, 18, vid);
    };
    tb(ID_BRIGHT, ID_BRIGHT_V, 10, 100, cfg.brightness as i32, y);
    y += 28;
    label("Clock movement", y);
    tb(ID_MOVE, ID_MOVE_V, 0, 100, cfg.movement as i32, y);
    y += 28;
    label("Multiple monitors", y);
    combo(ID_MON, &MON_OPTS, cfg.monitors, y, CBS_DROPDOWNLIST as u32);
    y += 30;
    label("Refresh weather every", y);
    let r_names: Vec<String> = REFRESH_OPTS.iter().map(|m| format!("{m} minutes")).collect();
    let r_refs: Vec<&str> = r_names.iter().map(|s| s.as_str()).collect();
    combo(ID_REFRESH, &r_refs, REFRESH_OPTS.iter().position(|m| *m == cfg.refresh_min).unwrap_or(1), y, CBS_DROPDOWNLIST as u32);
    y += 30;
    label("Font", y);
    let fh = combo(ID_FONT, &FONT_OPTS, 0, y, CBS_DROPDOWN as u32);
    set_text(fh, if cfg.font.is_empty() { FONT_OPTS[0] } else { &cfg.font });
}

/// Caption under the preview and the OK/Cancel row; `layout` positions them.
unsafe fn build_footer(dlg: HWND) {
    let font = ST.with(|s| s.borrow().as_ref().unwrap().font);
    let hinst = GetModuleHandleW(null());
    let mk = |class: &str, text: &str, style: u32, id: i32| {
        let hw = CreateWindowExW(0, wide(class).as_ptr(), wide(text).as_ptr(), WS_CHILD | WS_VISIBLE | style, 0, 0, 10, 10, dlg, id as isize as HMENU, hinst, null());
        SendMessageW(hw, WM_SETFONT, font as usize, 1);
    };
    mk("STATIC", "Live preview \u{2014} changes show instantly", 0, ID_CAPTION);
    mk("SysLink", "", WS_TABSTOP, ID_CREDIT); // credit text with clickable source links
    mk("BUTTON", "Fullscreen-size test", BS_PUSHBUTTON as u32 | WS_TABSTOP, ID_PREVIEW);
    mk("BUTTON", "Credits\u{2026}", BS_PUSHBUTTON as u32 | WS_TABSTOP, ID_ABOUT);
    mk("BUTTON", "OK", BS_DEFPUSHBUTTON as u32 | WS_TABSTOP, IDOK);
    mk("BUTTON", "Cancel", BS_PUSHBUTTON as u32 | WS_TABSTOP, IDCANCEL);
}

/// Position panel, preview and footer for the current client size; resize the preview renderer to match.
unsafe fn layout(dlg: HWND) {
    let dpi = ST.with(|s| s.borrow().as_ref().map(|s| s.dpi)).unwrap_or(96);
    let sc = |v: i32| v * dpi / 96;
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(dlg, &mut rc);
    let (cw, ch) = (rc.right, rc.bottom);
    let btn_h = sc(BUTTON_ROW_H);
    let panel = PANEL.with(|c| c.get()) as HWND;
    let prev = PREV.with(|c| c.get()) as HWND;
    if panel.is_null() || cw <= 0 || ch <= 0 {
        return;
    }
    // options panel (left): fixed width, scrolls vertically when the window is shorter than its content
    let panel_h = (ch - btn_h).max(sc(60));
    MoveWindow(panel, 0, 0, sc(PANEL_W), panel_h, 1);
    let mut si: SCROLLINFO = std::mem::zeroed();
    si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
    si.fMask = SIF_POS;
    GetScrollInfo(panel, SB_VERT, &mut si);
    let old_pos = si.nPos;
    si.fMask = SIF_RANGE | SIF_PAGE;
    si.nMin = 0;
    si.nMax = sc(CONTENT_H) - 1;
    si.nPage = panel_h as u32;
    SetScrollInfo(panel, SB_VERT, &si, 1);
    si.fMask = SIF_POS;
    GetScrollInfo(panel, SB_VERT, &mut si);
    if si.nPos != old_pos {
        ScrollWindowEx(panel, 0, old_pos - si.nPos, null(), null(), null_mut(), null_mut(), SW_SCROLLCHILDREN | SW_INVALIDATE | SW_ERASE);
    }
    // preview (right): 16:9, as large as fits
    let rx = sc(PANEL_W + 8);
    let (top, caption_h) = (sc(MARGIN), sc(26) + sc(CREDIT_H));
    let avail_w = (cw - rx - sc(MARGIN)).max(sc(MIN_PREVIEW_W));
    let avail_h = (ch - btn_h - top - caption_h).max(sc(60));
    let (mut pw, mut ph) = (avail_w, avail_w * 9 / 16);
    if ph > avail_h {
        ph = avail_h;
        pw = ph * 16 / 9;
    }
    if !prev.is_null() {
        MoveWindow(prev, rx, top, pw, ph, 1);
        let mut pc: RECT = std::mem::zeroed();
        GetClientRect(prev, &mut pc);
        ST.with(|s| {
            if let Some(r) = s.borrow_mut().as_mut().and_then(|s| s.preview.as_mut()) {
                r.w = pc.right.max(1);
                r.h = pc.bottom.max(1);
            }
        });
    }
    MoveWindow(GetDlgItem(dlg, ID_CAPTION), rx, top + ph + sc(6), pw.max(sc(120)), sc(18), 1);
    // the credit text takes whatever is left above the footer, so long entries (several wrapped lines) aren't clipped
    let credit_top = top + ph + sc(30);
    let credit_h = (ch - btn_h - credit_top).max(sc(CREDIT_H) - sc(6));
    MoveWindow(GetDlgItem(dlg, ID_CREDIT), rx, credit_top, pw.max(sc(240)), credit_h, 1);
    // footer buttons
    let by = ch - btn_h + sc(10);
    MoveWindow(GetDlgItem(dlg, ID_PREVIEW), sc(MARGIN), by, sc(150), sc(26), 1);
    MoveWindow(GetDlgItem(dlg, ID_ABOUT), sc(MARGIN) + sc(160), by, sc(100), sc(26), 1);
    let cancel_x = cw - sc(MARGIN) - sc(110);
    MoveWindow(GetDlgItem(dlg, IDCANCEL), cancel_x, by, sc(110), sc(26), 1);
    MoveWindow(GetDlgItem(dlg, IDOK), cancel_x - sc(10) - sc(110), by, sc(110), sc(26), 1);
}

unsafe fn scroll_panel(panel: HWND, f: impl Fn(&SCROLLINFO) -> i32) {
    let mut si: SCROLLINFO = std::mem::zeroed();
    si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
    si.fMask = SIF_ALL;
    GetScrollInfo(panel, SB_VERT, &mut si);
    let max = (si.nMax - si.nPage as i32 + 1).max(0);
    let new = f(&si).clamp(0, max);
    if new != si.nPos {
        ScrollWindowEx(panel, 0, si.nPos - new, null(), null(), null_mut(), null_mut(), SW_SCROLLCHILDREN | SW_INVALIDATE | SW_ERASE);
        si.fMask = SIF_POS;
        si.nPos = new;
        SetScrollInfo(panel, SB_VERT, &si, 1);
    }
}

/// The options panel: forwards control notifications to the dialog and scrolls (bar, wheel).
unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let dpi = ST.with(|s| s.borrow().as_ref().map(|s| s.dpi)).unwrap_or(96);
    match msg {
        WM_COMMAND | WM_HSCROLL_ => return SendMessageW(GetParent(hwnd), msg, wp, lp),
        WM_VSCROLL => {
            let line = 24 * dpi / 96;
            let code = (wp & 0xFFFF) as i32;
            scroll_panel(hwnd, |si| match code {
                SB_LINEUP => si.nPos - line,
                SB_LINEDOWN => si.nPos + line,
                SB_PAGEUP => si.nPos - si.nPage as i32,
                SB_PAGEDOWN => si.nPos + si.nPage as i32,
                SB_THUMBTRACK | SB_THUMBPOSITION => si.nTrackPos,
                SB_TOP => 0,
                SB_BOTTOM => i32::MAX,
                _ => si.nPos,
            });
            return 0;
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp >> 16) & 0xFFFF) as u16 as i16 as i32;
            scroll_panel(hwnd, |si| si.nPos - delta * (48 * dpi / 96) / 120);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Pull every control back into a Config. Returns None (after telling the user) if the location can't be resolved.
unsafe fn collect(dlg: HWND, resolve: bool) -> Option<Config> {
    let (mut cfg, resolved, detected) = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        (s.cfg.clone(), s.resolved.clone(), s.detected.clone())
    });
    let text = get_text(item(dlg, ID_LOC)).trim().to_string();
    let auto = checked(dlg, ID_AUTO);
    cfg.auto_location = auto;
    if auto {
        // detected values win; the typed override is left as it was
        if let Some(d) = detected {
            cfg.lat = d.lat;
            cfg.lon = d.lon;
            cfg.place = d.name;
            cfg.detected_text = d.text;
        }
    } else if text != cfg.resolved_for || !cfg.lat.is_finite() {
        match resolved.filter(|r| r.3 == text) {
            Some((lat, lon, name, _)) => {
                cfg.lat = lat;
                cfg.lon = lon;
                cfg.place = name;
                cfg.resolved_for = text.clone();
            }
            None if resolve => {
                SetCursor(LoadCursorW(null_mut(), IDC_WAIT));
                match weather::geocode(&text) {
                    Ok((lat, lon, name)) => {
                        cfg.lat = lat;
                        cfg.lon = lon;
                        cfg.place = name;
                        cfg.resolved_for = text.clone();
                    }
                    Err(e) => {
                        MessageBoxW(dlg, wide(&format!("Couldn't find that location:\n{e}")).as_ptr(), wide("Weather Clock").as_ptr(), MB_ICONWARNING);
                        return None;
                    }
                }
            }
            None => {}
        }
    }
    if !auto {
        cfg.location = text;
    }
    cfg.fahrenheit = checked(dlg, ID_UNITS);
    cfg.clock24 = checked(dlg, ID_H24);
    cfg.seconds = checked(dlg, ID_SECS);
    cfg.details = checked(dlg, ID_DETAILS);
    cfg.background = sel(dlg, ID_BG);
    cfg.fps = FPS_OPTS[sel(dlg, ID_FPS).min(2)];
    cfg.brightness = SendMessageW(item(dlg, ID_BRIGHT), TBM_GETPOS_, 0, 0) as u32;
    cfg.movement = SendMessageW(item(dlg, ID_MOVE), TBM_GETPOS_, 0, 0) as u32;
    cfg.monitors = sel(dlg, ID_MON);
    cfg.refresh_min = REFRESH_OPTS[sel(dlg, ID_REFRESH).min(3)];
    let font = get_text(item(dlg, ID_FONT)).trim().to_string();
    cfg.font = if font.is_empty() || font == FONT_OPTS[0] { String::new() } else { font };
    Some(cfg)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let hinst = GetModuleHandleW(null());
            let dpi = ST.with(|s| s.borrow().as_ref().unwrap().dpi);
            let panel = CreateWindowExW(WS_EX_CONTROLPARENT, wide("WxScrPanel").as_ptr(), null(), WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN, 0, 0, PANEL_W * dpi / 96, CONTENT_H * dpi / 96, hwnd, null_mut(), hinst, null());
            PANEL.with(|c| c.set(panel as isize));
            build(panel);
            build_footer(hwnd);
            make_preview(hwnd);
            layout(hwnd);
            // detect in the background (Windows location can take a few seconds); WM_DETECTED delivers the result
            let target = hwnd as usize;
            std::thread::spawn(move || {
                let r = weather::detect();
                *DETECT_RESULT.lock().unwrap() = Some(r);
                PostMessageW(target as HWND, WM_DETECTED, 0, 0);
            });
            return 0;
        }
        WM_DETECTED => {
            let result = DETECT_RESULT.lock().unwrap().take();
            let auto = checked(hwnd, ID_AUTO);
            let edit = item(hwnd, ID_LOC);
            match result {
                Some(Ok(d)) => {
                    set_text(item(hwnd, ID_STATUS), &format!("{}  ({:.2}, {:.2})\r\nvia {}", d.name, d.lat, d.lon, d.source));
                    // pre-fill: always when auto; when typing an override, only if the box is still empty
                    if auto || get_text(edit).trim().is_empty() {
                        set_text(edit, &d.text);
                    }
                    ST.with(|s| s.borrow_mut().as_mut().unwrap().detected = Some(d));
                }
                Some(Err(e)) => set_text(item(hwnd, ID_STATUS), &format!("Couldn't detect your location:\r\n{e}")),
                None => {}
            }
            return 0;
        }
        WM_SIZE => {
            if wp != SIZE_MINIMIZED as usize {
                layout(hwnd);
            }
            return 0;
        }
        WM_GETMINMAXINFO => {
            let dpi = ST.with(|s| s.borrow().as_ref().map(|s| s.dpi)).unwrap_or(96);
            let mut r = RECT { left: 0, top: 0, right: (PANEL_W + 8 + MIN_PREVIEW_W + MARGIN) * dpi / 96, bottom: (BUTTON_ROW_H + 300) * dpi / 96 };
            AdjustWindowRectEx(&mut r, WS_OVERLAPPEDWINDOW, 0, 0);
            let mmi = &mut *(lp as *mut MINMAXINFO);
            mmi.ptMinTrackSize.x = r.right - r.left;
            mmi.ptMinTrackSize.y = r.bottom - r.top;
            return 0;
        }
        WM_NOTIFY => {
            // a click (or Enter) on a link in the credit text opens it in the default browser
            let hdr = &*(lp as *const NMHDR);
            if hdr.idFrom == ID_CREDIT as usize && (hdr.code == NM_CLICK || hdr.code == NM_RETURN) {
                let link = &*(lp as *const NMLINK);
                let url: Vec<u16> = link.item.szUrl.iter().copied().take_while(|&c| c != 0).chain(Some(0)).collect();
                ShellExecuteW(null_mut(), wide("open").as_ptr(), url.as_ptr(), null(), null(), SW_SHOWNORMAL);
                return 0;
            }
        }
        WM_TIMER if wp == ID_TIMER => {
            let mut run = TEST_RUN.lock().unwrap();
            if run.as_mut().map_or(false, |c| matches!(c.try_wait(), Ok(None))) {
                return 0;
            }
            *run = None;
            drop(run);
            tick(hwnd);
            return 0;
        }
        WM_HSCROLL_ => {
            for (tb, label) in [(ID_BRIGHT, ID_BRIGHT_V), (ID_MOVE, ID_MOVE_V)] {
                let v = SendMessageW(item(hwnd, tb), TBM_GETPOS_, 0, 0);
                set_text(item(hwnd, label), &format!("{v}%"));
            }
            return 0;
        }
        WM_COMMAND => {
            let id = (wp & 0xFFFF) as i32;
            match id {
                ID_AUTO => update_auto_ui(hwnd),
                ID_LOOKUP => {
                    let text = get_text(item(hwnd, ID_LOC)).trim().to_string();
                    SetCursor(LoadCursorW(null_mut(), IDC_WAIT));
                    set_text(item(hwnd, ID_STATUS), "looking up\u{2026}");
                    match weather::geocode(&text) {
                        Ok((lat, lon, name)) => {
                            set_text(item(hwnd, ID_STATUS), &format!("{name}  ({lat:.2}, {lon:.2})"));
                            ST.with(|s| s.borrow_mut().as_mut().unwrap().resolved = Some((lat, lon, name, text)));
                        }
                        Err(e) => set_text(item(hwnd, ID_STATUS), &format!("not found: {e}")),
                    }
                }
                ID_ABOUT => show_credits(hwnd),
                ID_PREVIEW => {
                    if let Some(cfg) = collect(hwnd, true) {
                        cfg.save();
                        if let Ok(exe) = std::env::current_exe() {
                            if let Ok(child) = std::process::Command::new(exe).arg("/w").spawn() {
                                *TEST_RUN.lock().unwrap() = Some(child);
                            }
                        }
                    }
                }
                IDOK => {
                    if let Some(cfg) = collect(hwnd, true) {
                        cfg.save();
                        DestroyWindow(hwnd);
                    }
                }
                IDCANCEL => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            return 0;
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            return 0;
        }
        WM_DESTROY => {
            KillTimer(hwnd, ID_TIMER);
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn about_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            MoveWindow(GetDlgItem(hwnd, 1), 0, 0, rc.right, rc.bottom, 1);
            return 0;
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Scrollable read-only window with every credit, license and data attribution.
unsafe fn show_credits(owner: HWND) {
    let hinst = GetModuleHandleW(null());
    let (dpi, font) = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        (s.dpi, s.font)
    });
    let class = wide("WxScrAbout");
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(about_proc);
    wc.hInstance = hinst;
    wc.lpszClassName = class.as_ptr();
    wc.hbrBackground = (COLOR_WINDOW + 1) as isize as HBRUSH;
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    wc.hIcon = LoadImageW(hinst, 1 as *const u16, IMAGE_ICON, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON), LR_DEFAULTCOLOR) as _;
    RegisterClassW(&wc); // returns 0 if already registered, which is fine
    let sc = |v: i32| v * dpi / 96;
    let w = CreateWindowExW(0, class.as_ptr(), wide("Credits and licenses").as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, sc(720), sc(600), owner, null_mut(), hinst, null());
    if w.is_null() {
        return;
    }
    let edit = CreateWindowExW(0, wide("EDIT").as_ptr(), wide(&bg::about_text()).as_ptr(), WS_CHILD | WS_VISIBLE | WS_VSCROLL | (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32, 0, 0, 10, 10, w, 1 as HMENU, hinst, null());
    SendMessageW(edit, WM_SETFONT, font as usize, 1);
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(w, &mut rc);
    MoveWindow(edit, 0, 0, rc.right, rc.bottom, 1);
    SendMessageW(edit, 0x00B1 /* EM_SETSEL */, 0, 0); // no selected text on open
}

/// Auto checkbox toggled: the box is read-only (showing the detected place) in auto mode, editable otherwise.
unsafe fn update_auto_ui(dlg: HWND) {
    let auto = checked(dlg, ID_AUTO);
    EnableWindow(item(dlg, ID_LOC), (!auto) as i32);
    EnableWindow(item(dlg, ID_LOOKUP), (!auto) as i32);
    let detected = ST.with(|s| s.borrow().as_ref().and_then(|s| s.detected.clone()));
    if let Some(d) = detected {
        let edit = item(dlg, ID_LOC);
        // switching to auto shows the detection again; switching to manual keeps it as the starting point to edit
        if auto || get_text(edit).trim().is_empty() {
            set_text(edit, &d.text);
        }
    }
}

unsafe extern "system" fn preview_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_ERASEBKGND {
        return 1;
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Child window + renderer for the live preview, and the timer that redraws it.
unsafe fn make_preview(dlg: HWND) {
    let hinst = GetModuleHandleW(null());
    let class = wide("WxScrPreview");
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(preview_proc);
    wc.hInstance = hinst;
    wc.lpszClassName = class.as_ptr();
    wc.hbrBackground = GetStockObject(BLACK_BRUSH) as _;
    wc.style = CS_OWNDC;
    RegisterClassW(&wc);
    // initial size is irrelevant: `layout` resizes the window and the renderer right after
    let hw = CreateWindowExW(WS_EX_CLIENTEDGE, class.as_ptr(), null(), WS_CHILD | WS_VISIBLE, 0, 0, 320, 180, dlg, null_mut(), hinst, null());
    if hw.is_null() {
        return;
    }
    PREV.with(|c| c.set(hw as isize));
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hw, &mut rc);
    let cfg = ST.with(|s| s.borrow().as_ref().unwrap().cfg.clone());
    if let Ok(r) = Renderer::new(hw, rc.right.max(1), rc.bottom.max(1), &cfg, 0) {
        ST.with(|s| {
            let mut s = s.borrow_mut();
            let s = s.as_mut().unwrap();
            s.preview = Some(r);
            s.last_font = cfg.font.clone();
        });
        SetTimer(dlg, ID_TIMER, 10, None); // WM_TIMER is coarse; with vsync each render blocks to the display rate anyway
    }
}

/// Redraw the preview from whatever the controls say right now (no apply/OK needed).
unsafe fn tick(dlg: HWND) {
    let Some(cfg) = collect(dlg, false) else { return };
    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
    GetLocalTime(&mut st);
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut().unwrap();
        let t = s.started.elapsed().as_secs_f64() + 100.0;
        let Some(r) = s.preview.as_mut() else { return };
        if cfg.font != s.last_font {
            r.set_font(&cfg.font);
            s.last_font = cfg.font.clone();
        }
        // "random" cycles through the backgrounds so you can see them all
        let bg = if cfg.background >= bg::NAMES.len() { (t as usize / 8) % bg::NAMES.len() } else { cfg.background };
        let f = Frame { t, cfg: &cfg, wx: Some(&s.wx), show_text: true, bg, now: st };
        r.render(&f, None);
        if s.credit_bg != bg {
            s.credit_bg = bg;
            let text = if cfg.background >= bg::NAMES.len() { format!("Random each time \u{2014} currently showing:\r\n{}", bg::credit_markup(bg)) } else { bg::credit_markup(bg) };
            set_text(GetDlgItem(dlg, ID_CREDIT), &text);
        }
    });
}

pub unsafe fn run(owner: HWND) {
    let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES | ICC_LINK_CLASS };
    InitCommonControlsEx(&icc);
    let dpi = GetDpiForSystem() as i32;
    let font = CreateFontW(-(9 * dpi / 72), 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI").as_ptr());
    ST.with(|s| *s.borrow_mut() = Some(State { cfg: Config::load(), resolved: None, dpi, font, preview: None, wx: weather::load_cache().unwrap_or_else(weather::sample), last_font: String::new(), started: Instant::now(), credit_bg: usize::MAX, detected: None }));

    let hinst = GetModuleHandleW(null());
    let class = wide("WxScrConfig");
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(proc);
    wc.hInstance = hinst;
    wc.lpszClassName = class.as_ptr();
    wc.hbrBackground = (COLOR_BTNFACE + 1) as isize as HBRUSH;
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    // the icon is embedded as resource #1 (see wxscr.rc); harmless if it's missing
    wc.hIcon = LoadImageW(hinst, 1 as *const u16, IMAGE_ICON, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON), LR_DEFAULTCOLOR) as _;
    RegisterClassW(&wc);

    let mut pc: WNDCLASSW = std::mem::zeroed();
    pc.lpfnWndProc = Some(panel_proc);
    pc.hInstance = hinst;
    pc.hbrBackground = (COLOR_BTNFACE + 1) as isize as HBRUSH;
    pc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    let panel_class = wide("WxScrPanel");
    pc.lpszClassName = panel_class.as_ptr();
    RegisterClassW(&pc);

    let sc = |v: i32| v * dpi / 96;
    let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
    // comfortable default: options panel + a 400px-wide preview, but never bigger than ~92% of the screen
    let (scr_w, scr_h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let mut r = RECT { left: 0, top: 0, right: sc(PANEL_W + 8 + 400 + MARGIN), bottom: sc(CONTENT_H + BUTTON_ROW_H + 40) };
    AdjustWindowRectEx(&mut r, style, 0, 0);
    let (w, h) = ((r.right - r.left).min(scr_w * 92 / 100), (r.bottom - r.top).min(scr_h * 92 / 100));
    let (x, y) = ((scr_w - w) / 2, (scr_h - h) / 2);
    let hwnd = CreateWindowExW(0, class.as_ptr(), wide("Weather Clock Screensaver Settings").as_ptr(), style, x, y, w, h, owner, null_mut(), hinst, null());
    if hwnd.is_null() {
        return;
    }
    ShowWindow(hwnd, SW_SHOWMAXIMIZED); // opens maximized: biggest preview, and plenty of room for the options
    let small = LoadImageW(hinst, 1 as *const u16, IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), LR_DEFAULTCOLOR);
    if !small.is_null() {
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small as isize);
    }
    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        if IsDialogMessageW(hwnd, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
