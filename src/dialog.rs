//! Settings dialog (`/c`), built from plain Win32 controls so there are no resources to ship.

use crate::bg;
use crate::config::Config;
use crate::options::{self, Kind, Options, Param, Values};
use std::collections::BTreeMap;
use crate::render::{self, Frame, Renderer};
use crate::weather::{self, Weather};
use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::time::Instant;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
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
const ID_CONTRAST: i32 = 123;
const ID_TRANS: i32 = 124;
const ID_SCALE: i32 = 125;
const ID_BGOPT: i32 = 126;
const ID_SPEED: i32 = 127;
const ID_SPEED_V: i32 = 128;
/// Posted by the detection thread when `DETECT_RESULT` is ready.
const WM_DETECTED: u32 = WM_APP + 1;

const FPS_OPTS: [u32; 3] = [0, 60, 30];
const FPS_NAMES: [&str; 3] = ["Match display refresh rate (smoothest)", "Cap at 60 fps", "Cap at 30 fps"];
const SCALE_OPTS: [u32; 3] = [100, 75, 50];
const SCALE_NAMES: [&str; 3] = ["Native (sharpest)", "75% (lighter)", "50% (much lighter)"];
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
    /// When the preview started replaying the chosen transition (disappear, then appear again).
    trans_demo: Option<Instant>,
    trans_salt: u64,
    detected: Option<weather::Detected>,
}

const ID_TIMER: usize = 1;
/// Design-unit (96 dpi) geometry. The options panel is fixed-width and scrolls when the window is short;
/// the preview takes whatever room is left, keeping 16:9.
const PANEL_W: i32 = 470;
const CONTENT_H: i32 = 568;
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
    static OPT: RefCell<Option<OptState>> = const { RefCell::new(None) };
    static OPTWIN: Cell<isize> = const { Cell::new(0) };
    static OPTPANEL: Cell<isize> = const { Cell::new(0) };
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
    let has_opts = options::for_bg(cfg.background).is_some();
    mk("BUTTON", "Background options and presets\u{2026}", BS_PUSHBUTTON as u32 | WS_TABSTOP | if has_opts { 0 } else { WS_DISABLED }, 0, cx, y, 280, 24, ID_BGOPT);
    y += 32;
    label("Frame rate", y);
    combo(ID_FPS, &FPS_NAMES, FPS_OPTS.iter().position(|f| *f == cfg.fps).unwrap_or(0), y, CBS_DROPDOWNLIST as u32);
    y += 30;
    label("Render resolution", y);
    combo(ID_SCALE, &SCALE_NAMES, SCALE_OPTS.iter().position(|v| *v == cfg.render_scale).unwrap_or(0), y, CBS_DROPDOWNLIST as u32);
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
    label("Background speed", y);
    tb(ID_SPEED, ID_SPEED_V, 10, 200, cfg.speed as i32, y);
    y += 28;
    label("Text contrast", y);
    combo(ID_CONTRAST, &render::CONTRAST_NAMES, cfg.contrast, y, CBS_DROPDOWNLIST as u32);
    y += 30;
    label("Transition", y);
    combo(ID_TRANS, &render::TRANS_NAMES, cfg.transition, y, CBS_DROPDOWNLIST as u32);
    y += 30;
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

/// Give a scrolling panel a vertical scroll range for `content_h` pixels of content in a `page_h` pixel high window,
/// keeping the current position as far as the new range allows.
unsafe fn update_scroll(panel: HWND, content_h: i32, page_h: i32) {
    let mut si: SCROLLINFO = std::mem::zeroed();
    si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
    si.fMask = SIF_POS;
    GetScrollInfo(panel, SB_VERT, &mut si);
    let old_pos = si.nPos;
    si.fMask = SIF_RANGE | SIF_PAGE;
    si.nMin = 0;
    si.nMax = content_h - 1;
    si.nPage = page_h as u32;
    SetScrollInfo(panel, SB_VERT, &si, 1);
    si.fMask = SIF_POS;
    GetScrollInfo(panel, SB_VERT, &mut si);
    if si.nPos != old_pos {
        ScrollWindowEx(panel, 0, old_pos - si.nPos, null(), null(), null_mut(), null_mut(), SW_SCROLLCHILDREN | SW_INVALIDATE | SW_ERASE);
    }
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
    update_scroll(panel, sc(CONTENT_H), panel_h);
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
        // the colour swatches of the options window paint themselves in their colour
        WM_CTLCOLORSTATIC if GetParent(hwnd) as isize == OPTWIN.with(|c| c.get()) => return SendMessageW(GetParent(hwnd), msg, wp, lp),
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
    cfg.render_scale = SCALE_OPTS[sel(dlg, ID_SCALE).min(2)];
    cfg.brightness = SendMessageW(item(dlg, ID_BRIGHT), TBM_GETPOS_, 0, 0) as u32;
    cfg.movement = SendMessageW(item(dlg, ID_MOVE), TBM_GETPOS_, 0, 0) as u32;
    cfg.speed = (SendMessageW(item(dlg, ID_SPEED), TBM_GETPOS_, 0, 0) as u32).clamp(10, 200);
    cfg.contrast = sel(dlg, ID_CONTRAST).min(render::CONTRAST_NAMES.len() - 1);
    cfg.transition = sel(dlg, ID_TRANS).min(render::TRANS_NAMES.len() - 1);
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
            for (tb, label) in [(ID_BRIGHT, ID_BRIGHT_V), (ID_MOVE, ID_MOVE_V), (ID_SPEED, ID_SPEED_V)] {
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
                ID_BGOPT => open_options(hwnd),
                ID_BG if (wp >> 16) & 0xFFFF == 1 /* CBN_SELCHANGE */ => {
                    close_options();
                    EnableWindow(item(hwnd, ID_BGOPT), options::for_bg(sel(hwnd, ID_BG)).is_some() as i32);
                }
                ID_TRANS if (wp >> 16) & 0xFFFF == 1 /* CBN_SELCHANGE */ => {
                    ST.with(|s| {
                        let mut s = s.borrow_mut();
                        let s = s.as_mut().unwrap();
                        s.trans_demo = Some(Instant::now());
                        s.trans_salt = s.trans_salt.wrapping_add(1);
                    });
                }
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
        // picking a transition replays it in the preview: the scene disappears, rests a moment, then appears again
        let (mut vis, mut tstyle) = (1.0f32, 0usize);
        if let Some(t0) = s.trans_demo {
            let el = t0.elapsed().as_secs_f64();
            let out = render::TRANS_OUT_SECS;
            let rest = 0.35;
            if el >= out + rest + render::TRANS_IN_SECS {
                s.trans_demo = None;
            } else {
                tstyle = render::resolve_transition(cfg.transition, s.trans_salt);
                if el < out {
                    vis = (1.0 - el / out) as f32;
                } else if el < out + rest {
                    vis = 0.0;
                } else {
                    vis = ((el - out - rest) / render::TRANS_IN_SECS) as f32;
                }
            }
        }
        let f = Frame { t, cfg: &cfg, wx: Some(&s.wx), show_text: true, bg, now: st, vis, tstyle };
        r.render(&f, None);
        if s.credit_bg != bg {
            s.credit_bg = bg;
            let text = if cfg.background >= bg::NAMES.len() { format!("Random each time \u{2014} currently showing:\r\n{}", bg::credit_markup(bg)) } else { bg::credit_markup(bg) };
            set_text(GetDlgItem(dlg, ID_CREDIT), &text);
        }
    });
}

// ───────────── background options and presets window ─────────────
// Built entirely from the background's `Options` declaration (options.rs), so a new background with options needs no
// dialog code. Every change is written straight into the dialog's config, so the live preview shows it at once; OK keeps
// it, Cancel puts the values and saved presets back as they were when the window opened.

const ID_O_PRESET: i32 = 900;
const ID_O_SAVE: i32 = 901;
const ID_O_DELETE: i32 = 902;
const ID_O_PARAM: i32 = 1000;
const ID_O_VALUE: i32 = 1500;
const OPT_PANEL_W: i32 = 500;
const OPT_TOP_H: i32 = 46;
const OPT_BOTTOM_H: i32 = 46;
const CB_RESETCONTENT_: u32 = 0x014B;
const SS_NOTIFY_: u32 = 0x0100;
const TRACK_STEPS: f64 = 1000.0;

struct OptState {
    bg: usize,
    opts: &'static Options,
    /// Built-in presets first, then the user's: name, complete values, built-in?
    presets: Vec<(String, Values, bool)>,
    /// Values and saved presets as they were on opening, for Cancel.
    snapshot: (Option<Values>, Option<BTreeMap<String, Values>>),
    brushes: Vec<(u32, HBRUSH)>,
    bold: HFONT,
    content_h: i32,
}

fn opt_with<R>(f: impl FnOnce(&mut OptState) -> R) -> R {
    OPT.with(|o| f(o.borrow_mut().as_mut().expect("options window state")))
}

fn cfg_with<R>(f: impl FnOnce(&mut Config) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut().as_mut().expect("dialog state").cfg))
}

fn opt_current() -> Values {
    let bg = opt_with(|o| o.bg);
    cfg_with(|c| options::values_for(c, bg))
}

fn opt_store(vals: Values) {
    let bg = opt_with(|o| o.bg);
    cfg_with(|c| c.bg_opts.insert(crate::bg::index_to_id(bg).to_string(), vals));
}

fn preset_list(opts: &Options, cfg: &Config, bg: usize) -> Vec<(String, Values, bool)> {
    let mut v: Vec<_> = opts.presets.iter().map(|p| (p.name.to_string(), opts.preset_values(p), true)).collect();
    if let Some(named) = cfg.bg_presets.get(crate::bg::index_to_id(bg)) {
        v.extend(named.iter().map(|(n, vals)| (n.clone(), opts.resolve(vals), false)));
    }
    v
}

fn fmt_value(p: &Param, v: f64) -> String {
    match p.kind {
        Kind::Int(..) => format!("{}", v as i64),
        Kind::Float(lo, hi) if hi - lo <= 2.0 => format!("{v:.2}"),
        Kind::Float(lo, hi) if hi - lo <= 50.0 => format!("{v:.1}"),
        Kind::Float(..) => format!("{v:.0}"),
        Kind::Color => format!("#{:06x}", v as u32),
        Kind::Choice(c) => c[(v as usize).min(c.len() - 1)].to_string(),
        Kind::Bool => String::new(),
    }
}

/// 0xRRGGBB <-> COLORREF (0x00BBGGRR)
fn to_colorref(rgb: u32) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

unsafe fn opt_item(id: i32) -> HWND {
    GetDlgItem(OPTPANEL.with(|c| c.get()) as HWND, id)
}

/// Point every control at `vals`.
unsafe fn opt_load_controls(vals: &Values) {
    let opts = opt_with(|o| o.opts);
    for (i, p) in opts.params.iter().enumerate() {
        let v = vals[p.key];
        let h = opt_item(ID_O_PARAM + i as i32);
        match p.kind {
            Kind::Float(lo, hi) => {
                SendMessageW(h, TBM_SETPOS_, 1, ((v - lo) / (hi - lo) * TRACK_STEPS).round() as isize);
            }
            Kind::Int(..) => {
                SendMessageW(h, TBM_SETPOS_, 1, v as isize);
            }
            Kind::Bool => {
                SendMessageW(h, BM_SETCHECK_, (v != 0.0) as usize, 0);
            }
            Kind::Choice(_) => {
                SendMessageW(h, CB_SETCURSEL_, v as usize, 0);
            }
            Kind::Color => {
                InvalidateRect(h, null(), 1);
            }
        }
        if !matches!(p.kind, Kind::Bool | Kind::Choice(_)) {
            set_text(opt_item(ID_O_VALUE + i as i32), &fmt_value(p, v));
        }
    }
}

/// Show which preset the current values are (or "Custom").
unsafe fn opt_show_preset() {
    let cur = opt_current();
    let hit = opt_with(|o| o.presets.iter().position(|(_, v, _)| options::same(o.opts, v, &cur)));
    let combo = GetDlgItem(OPTWIN.with(|c| c.get()) as HWND, ID_O_PRESET);
    SendMessageW(combo, CB_SETCURSEL_, hit.map_or(usize::MAX, |i| i), 0);
    if hit.is_none() {
        set_text(combo, "Custom");
    }
    let user = hit.map_or(false, |i| !opt_with(|o| o.presets[i].2));
    EnableWindow(GetDlgItem(OPTWIN.with(|c| c.get()) as HWND, ID_O_DELETE), user as i32);
}

unsafe fn opt_fill_presets() {
    let combo = GetDlgItem(OPTWIN.with(|c| c.get()) as HWND, ID_O_PRESET);
    SendMessageW(combo, CB_RESETCONTENT_, 0, 0);
    let names: Vec<String> = opt_with(|o| o.presets.iter().map(|p| p.0.clone()).collect());
    for n in &names {
        SendMessageW(combo, CB_ADDSTRING_, 0, wide(n).as_ptr() as isize);
    }
    opt_show_preset();
}

unsafe fn opt_set_param(i: usize, v: f64) {
    let opts = opt_with(|o| o.opts);
    let p = &opts.params[i];
    let v = p.clamp(v);
    let mut vals = opt_current();
    vals.insert(p.key.to_string(), v);
    opt_store(vals);
    if !matches!(p.kind, Kind::Bool | Kind::Choice(_)) {
        set_text(opt_item(ID_O_VALUE + i as i32), &fmt_value(p, v));
    }
    opt_show_preset();
}

unsafe fn opt_build(win: HWND) {
    let (dpi, font) = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        (s.dpi, s.font)
    });
    let sc = |v: i32| v * dpi / 96;
    let hinst = GetModuleHandleW(null());
    let mk = |parent: HWND, f: HFONT, class: &str, text: &str, style: u32, ex: u32, x: i32, y: i32, w: i32, h: i32, id: i32| -> HWND {
        let hw = CreateWindowExW(ex, wide(class).as_ptr(), wide(text).as_ptr(), WS_CHILD | WS_VISIBLE | style, sc(x), sc(y), sc(w), sc(h), parent, id as isize as HMENU, hinst, null());
        SendMessageW(hw, WM_SETFONT, f as usize, 1);
        hw
    };
    // top bar: preset chooser
    mk(win, font, "STATIC", "Preset", 0, 0, 12, 15, 52, 18, 0);
    mk(win, font, "COMBOBOX", "", CBS_DROPDOWN as u32 | WS_VSCROLL | WS_TABSTOP, 0, 66, 11, 250, 240, ID_O_PRESET);
    mk(win, font, "BUTTON", "Save", BS_PUSHBUTTON as u32 | WS_TABSTOP, 0, 324, 10, 72, 24, ID_O_SAVE);
    mk(win, font, "BUTTON", "Delete", BS_PUSHBUTTON as u32 | WS_TABSTOP, 0, 402, 10, 72, 24, ID_O_DELETE);
    mk(win, font, "BUTTON", "OK", BS_DEFPUSHBUTTON as u32 | WS_TABSTOP, 0, 0, 0, 10, 10, IDOK);
    mk(win, font, "BUTTON", "Cancel", BS_PUSHBUTTON as u32 | WS_TABSTOP, 0, 0, 0, 10, 10, IDCANCEL);

    let panel = CreateWindowExW(WS_EX_CONTROLPARENT, wide("WxScrPanel").as_ptr(), null(), WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN, 0, 0, 10, 10, win, null_mut(), hinst, null());
    OPTPANEL.with(|c| c.set(panel as isize));
    let (opts, bold) = opt_with(|o| (o.opts, o.bold));
    let vals = opt_current();
    let mut y = 10;
    let mut group = "";
    for (i, p) in opts.params.iter().enumerate() {
        if p.group != group {
            group = p.group;
            y += if y > 10 { 8 } else { 0 };
            mk(panel, bold, "STATIC", p.group, 0, 0, 12, y, 300, 18, 0);
            y += 24;
        }
        let id = ID_O_PARAM + i as i32;
        let vid = ID_O_VALUE + i as i32;
        let v = vals[p.key];
        let label = |t: &str| {
            mk(panel, font, "STATIC", t, 0, 0, 12, y + 3, 178, 18, 0);
        };
        match p.kind {
            Kind::Float(..) | Kind::Int(..) => {
                label(p.label);
                let (lo, hi) = match p.kind {
                    Kind::Float(..) => (0, TRACK_STEPS as i32),
                    _ => (p.range().0 as i32, p.range().1 as i32),
                };
                let h = mk(panel, font, "msctls_trackbar32", "", (TBS_HORZ | TBS_NOTICKS) as u32 | WS_TABSTOP, 0, 188, y, 232, 24, id);
                SendMessageW(h, TBM_SETRANGE_, 1, ((hi << 16) | lo) as isize);
                mk(panel, font, "STATIC", &fmt_value(p, v), 0, 0, 426, y + 3, 64, 18, vid);
            }
            Kind::Bool => {
                let h = mk(panel, font, "BUTTON", p.label, BS_AUTOCHECKBOX as u32 | WS_TABSTOP, 0, 12, y, 400, 20, id);
                SendMessageW(h, BM_SETCHECK_, (v != 0.0) as usize, 0);
            }
            Kind::Choice(names) => {
                label(p.label);
                let h = mk(panel, font, "COMBOBOX", "", CBS_DROPDOWNLIST as u32 | WS_VSCROLL | WS_TABSTOP, 0, 196, y, 230, 240, id);
                for n in names {
                    SendMessageW(h, CB_ADDSTRING_, 0, wide(n).as_ptr() as isize);
                }
            }
            Kind::Color => {
                label(p.label);
                mk(panel, font, "STATIC", "", WS_BORDER | SS_NOTIFY_, 0, 196, y, 70, 22, id);
                mk(panel, font, "STATIC", &fmt_value(p, v), 0, 0, 274, y + 3, 120, 18, vid);
            }
        }
        y += 30;
    }
    opt_with(|o| o.content_h = y + 10);
    opt_load_controls(&vals);
    opt_fill_presets();
}

unsafe fn opt_layout(win: HWND) {
    let dpi = ST.with(|s| s.borrow().as_ref().map(|s| s.dpi)).unwrap_or(96);
    let sc = |v: i32| v * dpi / 96;
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(win, &mut rc);
    let (cw, ch) = (rc.right, rc.bottom);
    let (top, bottom) = (sc(OPT_TOP_H), sc(OPT_BOTTOM_H));
    let panel = OPTPANEL.with(|c| c.get()) as HWND;
    if panel.is_null() {
        return;
    }
    let page = (ch - top - bottom).max(sc(40));
    MoveWindow(panel, 0, top, cw, page, 1);
    update_scroll(panel, sc(opt_with(|o| o.content_h)), page);
    let by = ch - bottom + sc(10);
    MoveWindow(GetDlgItem(win, IDCANCEL), cw - sc(14) - sc(100), by, sc(100), sc(26), 1);
    MoveWindow(GetDlgItem(win, IDOK), cw - sc(14) - sc(100) - sc(10) - sc(100), by, sc(100), sc(26), 1);
}

unsafe fn pick_color(win: HWND, i: usize) {
    let opts = opt_with(|o| o.opts);
    let cur = opt_current()[opts.params[i].key] as u32;
    let mut custom = [0xFFFFFFu32; 16];
    let mut cc: CHOOSECOLORW = std::mem::zeroed();
    cc.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
    cc.hwndOwner = win;
    cc.rgbResult = to_colorref(cur);
    cc.lpCustColors = custom.as_mut_ptr();
    cc.Flags = CC_RGBINIT | CC_FULLOPEN;
    if ChooseColorW(&mut cc) != 0 {
        opt_set_param(i, to_colorref(cc.rgbResult) as f64);
        InvalidateRect(opt_item(ID_O_PARAM + i as i32), null(), 1);
    }
}

unsafe fn opt_message(win: HWND, text: &str) {
    MessageBoxW(win, wide(text).as_ptr(), wide("Weather Clock").as_ptr(), MB_ICONINFORMATION);
}

unsafe extern "system" fn opt_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            OPTWIN.with(|c| c.set(hwnd as isize)); // the controls built next look the window up
            opt_build(hwnd);
            opt_layout(hwnd);
            return 0;
        }
        WM_SIZE => {
            if wp != SIZE_MINIMIZED as usize && OPT.with(|o| o.borrow().is_some()) {
                opt_layout(hwnd);
            }
            return 0;
        }
        WM_CTLCOLORSTATIC if GetDlgCtrlID(lp as HWND) >= ID_O_PARAM && OPT.with(|o| o.borrow().is_some()) => {
            let id = GetDlgCtrlID(lp as HWND);
            let opts = opt_with(|o| o.opts);
            let i = (id - ID_O_PARAM) as usize;
            if i < opts.params.len() && matches!(opts.params[i].kind, Kind::Color) {
                let rgb = opt_current()[opts.params[i].key] as u32;
                return opt_with(|o| {
                    if let Some(b) = o.brushes.iter().find(|b| b.0 == rgb) {
                        return b.1 as LRESULT;
                    }
                    let b = CreateSolidBrush(to_colorref(rgb));
                    o.brushes.push((rgb, b));
                    b as LRESULT
                });
            }
        }
        WM_HSCROLL_ => {
            let id = GetDlgCtrlID(lp as HWND);
            let opts = opt_with(|o| o.opts);
            if id >= ID_O_PARAM && ((id - ID_O_PARAM) as usize) < opts.params.len() {
                let i = (id - ID_O_PARAM) as usize;
                let pos = SendMessageW(lp as HWND, TBM_GETPOS_, 0, 0) as f64;
                let v = match opts.params[i].kind {
                    Kind::Float(lo, hi) => lo + pos / TRACK_STEPS * (hi - lo),
                    _ => pos,
                };
                opt_set_param(i, v);
            }
            return 0;
        }
        WM_COMMAND => {
            let (id, code) = ((wp & 0xFFFF) as i32, ((wp >> 16) & 0xFFFF) as u32);
            let opts = opt_with(|o| o.opts);
            match id {
                ID_O_PRESET if code == 1 /* CBN_SELCHANGE */ => {
                    let i = SendMessageW(GetDlgItem(hwnd, ID_O_PRESET), CB_GETCURSEL_, 0, 0);
                    let vals = if i >= 0 { opt_with(|o| o.presets.get(i as usize).map(|p| p.1.clone())) } else { None };
                    if let Some(vals) = vals {
                        opt_store(vals.clone());
                        opt_load_controls(&vals);
                        opt_show_preset();
                    }
                }
                ID_O_SAVE => {
                    let name = get_text(GetDlgItem(hwnd, ID_O_PRESET)).trim().to_string();
                    if name.is_empty() || name == "Custom" {
                        opt_message(hwnd, "Type a name for the preset in the Preset box first, then click Save.");
                    } else if opt_with(|o| o.presets.iter().any(|p| p.2 && p.0.eq_ignore_ascii_case(&name))) {
                        opt_message(hwnd, "That name belongs to a built-in preset. Pick another name.");
                    } else {
                        let (vals, bg) = (opt_current(), opt_with(|o| o.bg));
                        cfg_with(|c| {
                            c.bg_presets.entry(crate::bg::index_to_id(bg).to_string()).or_default().insert(name.clone(), vals);
                        });
                        let list = cfg_with(|c| preset_list(opts, c, bg));
                        opt_with(|o| o.presets = list);
                        opt_fill_presets();
                        // show the new name even when its values equal a built-in preset's
                        if let Some(i) = opt_with(|o| o.presets.iter().position(|p| !p.2 && p.0 == name)) {
                            SendMessageW(GetDlgItem(hwnd, ID_O_PRESET), CB_SETCURSEL_, i, 0);
                            EnableWindow(GetDlgItem(hwnd, ID_O_DELETE), 1);
                        }
                    }
                }
                ID_O_DELETE => {
                    let i = SendMessageW(GetDlgItem(hwnd, ID_O_PRESET), CB_GETCURSEL_, 0, 0);
                    let target = if i >= 0 { opt_with(|o| o.presets.get(i as usize).filter(|p| !p.2).map(|p| p.0.clone())) } else { None };
                    if let Some(name) = target {
                        if MessageBoxW(hwnd, wide(&format!("Delete the preset \"{name}\"?")).as_ptr(), wide("Weather Clock").as_ptr(), MB_YESNO | MB_ICONQUESTION) == IDYES {
                            let bg = opt_with(|o| o.bg);
                            cfg_with(|c| {
                                if let Some(m) = c.bg_presets.get_mut(crate::bg::index_to_id(bg)) {
                                    m.remove(&name);
                                }
                            });
                            let list = cfg_with(|c| preset_list(opts, c, bg));
                            opt_with(|o| o.presets = list);
                            opt_fill_presets();
                        }
                    }
                }
                IDOK => {
                    DestroyWindow(hwnd);
                }
                IDCANCEL => {
                    let (bg, (vals, saved)) = opt_with(|o| (o.bg, o.snapshot.clone()));
                    let key = crate::bg::index_to_id(bg).to_string();
                    cfg_with(|c| {
                        match vals {
                            Some(v) => c.bg_opts.insert(key.clone(), v),
                            None => c.bg_opts.remove(&key),
                        };
                        match saved {
                            Some(s) => c.bg_presets.insert(key, s),
                            None => c.bg_presets.remove(&key),
                        };
                    });
                    DestroyWindow(hwnd);
                }
                _ if id >= ID_O_PARAM && ((id - ID_O_PARAM) as usize) < opts.params.len() => {
                    let i = (id - ID_O_PARAM) as usize;
                    match opts.params[i].kind {
                        Kind::Bool => {
                            let on = SendMessageW(opt_item(id), BM_GETCHECK_, 0, 0) == 1;
                            opt_set_param(i, on as i32 as f64);
                        }
                        Kind::Choice(_) if code == 1 => {
                            let v = SendMessageW(opt_item(id), CB_GETCURSEL_, 0, 0).max(0);
                            opt_set_param(i, v as f64);
                        }
                        Kind::Color if code == 0 /* STN_CLICKED */ => pick_color(hwnd, i),
                        _ => {}
                    }
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
            OPTWIN.with(|c| c.set(0));
            OPTPANEL.with(|c| c.set(0));
            if let Some(o) = OPT.with(|o| o.borrow_mut().take()) {
                for (_, b) in o.brushes {
                    DeleteObject(b as _);
                }
                DeleteObject(o.bold as _);
            }
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Close the options window, keeping whatever was set (used when the background changes under it).
unsafe fn close_options() {
    let w = OPTWIN.with(|c| c.get()) as HWND;
    if !w.is_null() {
        DestroyWindow(w);
    }
}

unsafe fn open_options(owner: HWND) {
    let existing = OPTWIN.with(|c| c.get()) as HWND;
    if !existing.is_null() {
        SetForegroundWindow(existing);
        return;
    }
    let bg = sel(owner, ID_BG); // the background as chosen in the dialog right now, not as last saved
    let Some(opts) = options::for_bg(bg) else { return };
    let (dpi, _) = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        (s.dpi, s.font)
    });
    let sc = |v: i32| v * dpi / 96;
    let hinst = GetModuleHandleW(null());
    let class = wide("WxScrOptions");
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(opt_proc);
    wc.hInstance = hinst;
    wc.lpszClassName = class.as_ptr();
    wc.hbrBackground = (COLOR_BTNFACE + 1) as isize as HBRUSH;
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    wc.hIcon = LoadImageW(hinst, 1 as *const u16, IMAGE_ICON, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON), LR_DEFAULTCOLOR) as _;
    RegisterClassW(&wc); // returns 0 if already registered, which is fine

    let key = crate::bg::index_to_id(bg);
    let state = cfg_with(|c| OptState {
        bg,
        opts,
        presets: preset_list(opts, c, bg),
        snapshot: (c.bg_opts.get(key).cloned(), c.bg_presets.get(key).cloned()),
        brushes: Vec::new(),
        bold: CreateFontW(-(9 * dpi / 72), 0, 0, 0, FW_BOLD as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI").as_ptr()),
        content_h: 0,
    });
    OPT.with(|o| *o.borrow_mut() = Some(state));

    // as tall as the content wants (up to 90% of the screen), at the left so the live preview stays visible
    let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
    let (scr_w, scr_h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let content = 20 * 30 + 4 * 32 + 20;
    let mut r = RECT { left: 0, top: 0, right: sc(OPT_PANEL_W + 20), bottom: sc(OPT_TOP_H + OPT_BOTTOM_H + content) };
    AdjustWindowRectEx(&mut r, style, 0, 0);
    let (w, h) = ((r.right - r.left).min(scr_w * 90 / 100), (r.bottom - r.top).min(scr_h * 90 / 100));
    let title = format!("{} options and presets", opts.title);
    let win = CreateWindowExW(0, class.as_ptr(), wide(&title).as_ptr(), style | WS_VISIBLE, sc(24), (scr_h - h) / 3, w, h, owner, null_mut(), hinst, null());
    if win.is_null() {
        crate::log(&format!("options window could not be created (error {})", windows_sys::Win32::Foundation::GetLastError()));
        OPT.with(|o| *o.borrow_mut() = None);
        return;
    }
    OPTWIN.with(|c| c.set(win as isize));
}

pub unsafe fn run(owner: HWND) {
    let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES | ICC_LINK_CLASS };
    InitCommonControlsEx(&icc);
    let dpi = GetDpiForSystem() as i32;
    let font = CreateFontW(-(9 * dpi / 72), 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI").as_ptr());
    ST.with(|s| *s.borrow_mut() = Some(State { cfg: Config::load(), resolved: None, dpi, font, preview: None, wx: weather::load_cache().unwrap_or_else(weather::sample), last_font: String::new(), started: Instant::now(), credit_bg: usize::MAX, detected: None, trans_demo: None, trans_salt: 0 }));

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
