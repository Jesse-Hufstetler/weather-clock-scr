//! Settings dialog (`/c`), built from plain Win32 controls so there are no resources to ship.

use crate::bg;
use crate::config::Config;
use crate::weather;
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
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
const ID_QUALITY: i32 = 109;
const ID_FPS: i32 = 110;
const ID_BRIGHT: i32 = 111;
const ID_MOVE: i32 = 112;
const ID_MON: i32 = 113;
const ID_REFRESH: i32 = 114;
const ID_FONT: i32 = 115;
const ID_PREVIEW: i32 = 116;
const ID_BRIGHT_V: i32 = 117;
const ID_MOVE_V: i32 = 118;

const FPS_OPTS: [u32; 4] = [15, 20, 30, 60];
const REFRESH_OPTS: [u32; 4] = [10, 20, 30, 60];
const QUALITY_OPTS: [&str; 4] = ["Lowest power (35% resolution)", "Balanced (50%)", "High (75%)", "Native (100%)"];
const MON_OPTS: [&str; 3] = ["Clock on every monitor", "Clock on main monitor, background on others", "Clock on main monitor, others black"];
const FONT_OPTS: [&str; 5] = ["Droid Sans Mono (bundled)", "Segoe UI", "Consolas", "Bahnschrift", "Cascadia Mono"];

struct State {
    cfg: Config,
    resolved: Option<(f64, f64, String, String)>, // lat, lon, name, text it was resolved for
    dpi: i32,
    font: HFONT,
}

thread_local! { static ST: RefCell<Option<State>> = const { RefCell::new(None) }; }

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe fn item(dlg: HWND, id: i32) -> HWND {
    GetDlgItem(dlg, id)
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

unsafe fn build(dlg: HWND) {
    let (cfg, dpi, font) = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        (s.cfg.clone(), s.dpi, s.font)
    });
    let sc = |v: i32| v * dpi / 96;
    let hinst = GetModuleHandleW(null());
    let mk = |class: &str, text: &str, style: u32, ex: u32, x: i32, y: i32, w: i32, h: i32, id: i32| -> HWND {
        let hw = CreateWindowExW(ex, wide(class).as_ptr(), wide(text).as_ptr(), WS_CHILD | WS_VISIBLE | style, sc(x), sc(y), sc(w), sc(h), dlg, id as isize as HMENU, hinst, null());
        SendMessageW(hw, WM_SETFONT, font as usize, 1);
        hw
    };
    let (lx, cx) = (14, 170);
    let mut y = 14;
    let label = |t: &str, y: i32| {
        mk("STATIC", t, 0, 0, lx, y + 3, 150, 18, 0);
    };

    label("Location", y);
    mk("EDIT", &cfg.location, (ES_AUTOHSCROLL as u32) | WS_TABSTOP, WS_EX_CLIENTEDGE, cx, y, 170, 22, ID_LOC);
    mk("BUTTON", "Look up", BS_PUSHBUTTON as u32 | WS_TABSTOP, 0, cx + 178, y - 1, 70, 24, ID_LOOKUP);
    y += 26;
    let status = if cfg.located() { format!("{}  ({:.2}, {:.2})", cfg.place, cfg.lat, cfg.lon) } else { "zip, \"City, ST\", lat,lon \u{2014} or empty to locate by IP".to_string() };
    mk("STATIC", &status, 0, 0, cx, y, 280, 18, ID_STATUS);
    y += 28;

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
    label("Render quality", y);
    combo(ID_QUALITY, &QUALITY_OPTS, cfg.quality, y, CBS_DROPDOWNLIST as u32);
    y += 30;
    label("Frame rate", y);
    let fps_names: Vec<String> = FPS_OPTS.iter().map(|f| format!("{f} fps{}", if *f == 30 { " (recommended)" } else { "" })).collect();
    let fps_refs: Vec<&str> = fps_names.iter().map(|s| s.as_str()).collect();
    combo(ID_FPS, &fps_refs, FPS_OPTS.iter().position(|f| *f == cfg.fps).unwrap_or(2), y, CBS_DROPDOWNLIST as u32);
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
    y += 40;

    mk("BUTTON", "Preview", BS_PUSHBUTTON as u32 | WS_TABSTOP, 0, lx, y, 80, 26, ID_PREVIEW);
    mk("BUTTON", "OK", BS_DEFPUSHBUTTON as u32 | WS_TABSTOP, 0, cx + 50, y, 110, 26, IDOK);
    mk("BUTTON", "Cancel", BS_PUSHBUTTON as u32 | WS_TABSTOP, 0, cx + 170, y, 110, 26, IDCANCEL);
}

/// Pull every control back into a Config. Returns None (after telling the user) if the location can't be resolved.
unsafe fn collect(dlg: HWND, resolve: bool) -> Option<Config> {
    let (mut cfg, resolved) = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        (s.cfg.clone(), s.resolved.clone())
    });
    let text = get_text(item(dlg, ID_LOC)).trim().to_string();
    if text != cfg.resolved_for || !cfg.lat.is_finite() {
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
    cfg.location = text;
    cfg.fahrenheit = checked(dlg, ID_UNITS);
    cfg.clock24 = checked(dlg, ID_H24);
    cfg.seconds = checked(dlg, ID_SECS);
    cfg.details = checked(dlg, ID_DETAILS);
    cfg.background = sel(dlg, ID_BG);
    cfg.quality = sel(dlg, ID_QUALITY);
    cfg.fps = FPS_OPTS[sel(dlg, ID_FPS).min(3)];
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
            build(hwnd);
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
                ID_PREVIEW => {
                    if let Some(cfg) = collect(hwnd, true) {
                        cfg.save();
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = std::process::Command::new(exe).arg("/w").spawn();
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
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

pub unsafe fn run(owner: HWND) {
    let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES };
    InitCommonControlsEx(&icc);
    let dpi = GetDpiForSystem() as i32;
    let font = CreateFontW(-(9 * dpi / 72), 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI").as_ptr());
    ST.with(|s| *s.borrow_mut() = Some(State { cfg: Config::load(), resolved: None, dpi, font }));

    let hinst = GetModuleHandleW(null());
    let class = wide("WxScrConfig");
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(proc);
    wc.hInstance = hinst;
    wc.lpszClassName = class.as_ptr();
    wc.hbrBackground = (COLOR_BTNFACE + 1) as isize as HBRUSH;
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    RegisterClassW(&wc);

    let sc = |v: i32| v * dpi / 96;
    let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let mut r = RECT { left: 0, top: 0, right: sc(470), bottom: sc(572) };
    AdjustWindowRectEx(&mut r, style, 0, 0);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let (x, y) = ((GetSystemMetrics(SM_CXSCREEN) - w) / 2, (GetSystemMetrics(SM_CYSCREEN) - h) / 2);
    let hwnd = CreateWindowExW(0, class.as_ptr(), wide("Weather Clock Screensaver Settings").as_ptr(), style | WS_VISIBLE, x, y, w, h, owner, null_mut(), hinst, null());
    if hwnd.is_null() {
        return;
    }
    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        if IsDialogMessageW(hwnd, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
