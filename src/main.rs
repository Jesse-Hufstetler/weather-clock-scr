#![windows_subsystem = "windows"]

mod bg;
mod config;
mod dialog;
mod gl;
mod http;
mod png;
mod render;
mod text;
mod weather;

use config::Config;
use render::{Frame, Renderer};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use weather::Weather;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static WEATHER: Mutex<Option<Weather>> = Mutex::new(None);
static SAVER: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicU64 = AtomicU64::new(0);
static INIT_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);
/// A transition is configured, so input fades the screensaver out instead of quitting on the spot.
static FADE_OUT: AtomicBool = AtomicBool::new(false);
/// Tick count when the fade-out began (0 = not leaving yet).
static EXIT_START: AtomicU64 = AtomicU64::new(0);

pub fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(config::dir().join("log.txt")) {
        let _ = writeln!(f, "[{}] {msg}", weather::now_unix());
    }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

// ───────────── weather thread ─────────────
fn spawn_weather(mut cfg: Config, fetch_network: bool) {
    if let Some(w) = weather::load_cache() {
        if !cfg.located() || w.place == cfg.place {
            *WEATHER.lock().unwrap() = Some(w);
        }
    }
    if !fetch_network {
        return;
    }
    std::thread::spawn(move || loop {
        if cfg.auto_location {
            // every cycle: Windows Location Service, then IP address — so a laptop that moves gets new weather
            match weather::detect() {
                Ok(d) => {
                    cfg.lat = d.lat;
                    cfg.lon = d.lon;
                    cfg.place = d.name.clone();
                    Config::store_detected(d.lat, d.lon, &d.name, &d.text);
                }
                Err(e) => {
                    log(&format!("location detection failed: {e}"));
                    if !cfg.lat.is_finite() {
                        // never detected anything: fall back to the typed location, if any
                        match weather::geocode(&cfg.location).ok().filter(|_| !cfg.location.is_empty()) {
                            Some((lat, lon, name)) => {
                                cfg.lat = lat;
                                cfg.lon = lon;
                                cfg.place = name;
                            }
                            None => {
                                std::thread::sleep(Duration::from_secs(60));
                                continue;
                            }
                        }
                    }
                }
            }
        } else if !cfg.located() {
            match weather::geocode(&cfg.location) {
                Ok((lat, lon, name)) => {
                    cfg.lat = lat;
                    cfg.lon = lon;
                    cfg.place = name;
                    cfg.resolved_for = cfg.location.clone();
                    cfg.save();
                }
                Err(e) => {
                    log(&format!("location: {e}"));
                    std::thread::sleep(Duration::from_secs(60));
                    continue;
                }
            }
        }
        match weather::fetch(cfg.lat, cfg.lon, &cfg.place) {
            Ok(w) => {
                weather::save_cache(&w);
                *WEATHER.lock().unwrap() = Some(w);
                std::thread::sleep(Duration::from_secs(cfg.refresh_min as u64 * 60));
            }
            Err(e) => {
                log(&format!("weather: {e}"));
                std::thread::sleep(Duration::from_secs(90));
            }
        }
    });
}

// ───────────── windows ─────────────
fn quit() {
    unsafe { PostQuitMessage(0) };
}

// KNOWN ISSUES / NOTES ON THE START AND END OF A RUN (transitions + flashes)
//
// * A faint black flash is still visible at start/stop, roughly along the top of the taskbar. What has been ruled out:
//   the startup screenshot has no dark row there (dumped and inspected), and the taskbar window's own pixels do not
//   change. What was fixed: the window used to get a taskbar button that appeared and vanished (the taskbar then
//   re-laid itself out and flashed), so it is now WS_EX_TOOLWINDOW. What remains is probably the shell: it keeps the
//   taskbar above our window for ~0.3 s after we appear, then drops it from topmost, and promotes it again ~0.1 s
//   after we are destroyed. Untested ideas: keep the window off the taskbar strip until the shell has demoted the
//   taskbar, or fade the window with layered alpha instead of moving it on-screen.
// * The window is created far off-screen and moved onto the monitor after two frames have been presented, because
//   showing a fresh GL window lets the compositor display its blank (black) surface for a frame or two.
// * The fade BACK to the desktop blends with the screenshot taken at start-up, so anything that changed on screen in
//   the meantime (a notification, a video) pops at the very end. An earlier version re-captured the screen behind the
//   saver at exit using SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE) and gave better results in principle, but
//   toggling the affinity was a suspect for the black flash and was removed. Worth revisiting (e.g. capture with
//   Windows.Graphics.Capture or DXGI desktop duplication, which can exclude our window without touching it).
// * Screen capture with GDI (CopyFromScreen, BitBlt) does not reliably show this window, so the transitions were
//   checked frame by frame with /snap (WXSCR_TRANS=<style>:<visibility>) and not by recording the real screen.

/// Input arrived: start the fade-out if there is one (the main loop quits when it finishes), else quit now.
fn leave() {
    if FADE_OUT.load(Ordering::Relaxed) {
        let now = unsafe { GetTickCount64() }.max(1);
        let _ = EXIT_START.compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    } else {
        quit();
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let saver = SAVER.load(Ordering::Relaxed);
    let age = GetTickCount64().saturating_sub(STARTED.load(Ordering::Relaxed));
    match msg {
        WM_SETCURSOR if saver => {
            SetCursor(null_mut());
            return 1;
        }
        WM_MOUSEMOVE if saver => {
            let mut p = POINT { x: 0, y: 0 };
            GetCursorPos(&mut p);
            let mut init = INIT_POS.lock().unwrap();
            match *init {
                None => *init = Some((p.x, p.y)),
                Some((x, y)) => {
                    if age > 600 && ((p.x - x).abs() > 6 || (p.y - y).abs() > 6) {
                        leave();
                    }
                }
            }
            return 0;
        }
        WM_KEYDOWN | WM_SYSKEYDOWN if saver || wp as u16 == VK_ESCAPE => {
            if age > 300 {
                leave();
            }
            return 0;
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN if saver => {
            if age > 300 {
                leave();
            }
            return 0;
        }
        WM_ACTIVATEAPP if saver && wp == 0 && age > 1500 => quit(),
        WM_SYSCOMMAND if saver && (wp & 0xFFF0 == SC_SCREENSAVE as usize || wp & 0xFFF0 == SC_MONITORPOWER as usize) => return 0,
        WM_ERASEBKGND => return 1,
        WM_CLOSE => {
            DestroyWindow(hwnd);
            return 0;
        }
        WM_DESTROY => quit(),
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe fn register_class() -> (HINSTANCE, Vec<u16>) {
    let hinst = GetModuleHandleW(null());
    let name = wide("WxScrWindow");
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(wndproc);
    wc.hInstance = hinst;
    wc.lpszClassName = name.as_ptr();
    wc.hbrBackground = GetStockObject(BLACK_BRUSH) as _;
    wc.hCursor = null_mut();
    wc.style = CS_OWNDC;
    RegisterClassW(&wc);
    (hinst, name)
}

struct Win {
    hwnd: HWND,
    rect: RECT,
    renderer: Option<Renderer>,
    show_text: bool,
}

struct Mon {
    rect: RECT,
    primary: bool,
}

unsafe extern "system" fn enum_mon(hm: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> i32 {
    let v = &mut *(lp as *mut Vec<Mon>);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(hm, &mut mi) != 0 {
        v.push(Mon { rect: mi.rcMonitor, primary: mi.dwFlags & 1 != 0 });
    }
    1
}

/// Screenshot of a screen rectangle as bottom-up BGRA (what OpenGL wants), or None if GDI refuses.
unsafe fn capture(rect: &RECT) -> Option<(i32, i32, Vec<u8>)> {
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 {
        return None;
    }
    let sdc = GetDC(null_mut());
    let mdc = CreateCompatibleDC(sdc);
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = h; // positive = bottom-up, row 0 at the bottom like an OpenGL texture
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut core::ffi::c_void = null_mut();
    let bmp = CreateDIBSection(sdc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    let mut out = None;
    if !bmp.is_null() && !bits.is_null() {
        let old = SelectObject(mdc, bmp as _);
        if BitBlt(mdc, 0, 0, w, h, sdc, rect.left, rect.top, SRCCOPY | CAPTUREBLT) != 0 {
            GdiFlush();
            out = Some((w, h, std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec()));
        }
        SelectObject(mdc, old);
    }
    if !bmp.is_null() {
        DeleteObject(bmp as _);
    }
    DeleteDC(mdc);
    ReleaseDC(null_mut(), sdc);
    out
}

fn pick_bg(cfg: &Config) -> usize {
    if cfg.background >= bg::NAMES.len() {
        (weather::now_unix() as usize / 7) % bg::NAMES.len()
    } else {
        cfg.background
    }
}

unsafe fn run(mode: Mode, cfg: Config) {
    let (hinst, class) = register_class();
    let title = wide("Weather Clock");
    let mut wins: Vec<Win> = Vec::new();
    let mut parent: HWND = null_mut();
    let bg_idx = pick_bg(&cfg);

    // fade in/out (not in the little Windows settings thumbnail)
    let salt = weather::now_unix() as u64;
    let (style_in, style_out) = if matches!(mode, Mode::Preview(_)) {
        (0, 0)
    } else {
        (render::resolve_transition(cfg.transition, salt), render::resolve_transition(cfg.transition, salt ^ 0x5bd1))
    };
    FADE_OUT.store(style_out != 0, Ordering::Relaxed);
    let mut shown_at: Option<Instant> = None; // set once the first frame is on screen
    let mut exit_t0: Option<Instant> = None; // set once the fade-out has started

    match &mode {
        Mode::Saver | Mode::Windowed => {
            let mut mons: Vec<Mon> = Vec::new();
            EnumDisplayMonitors(null_mut(), null(), Some(enum_mon), &mut mons as *mut _ as LPARAM);
            if mons.is_empty() {
                mons.push(Mon { rect: RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) }, primary: true });
            }
            if matches!(mode, Mode::Windowed) {
                // the test run covers the primary monitor at full size (quits on mouse move, key or click like the real saver), topmost
                mons.retain(|m| m.primary);
            }
            // what is on screen right now: the screensaver will dissolve out of it
            let shots: Vec<Option<(i32, i32, Vec<u8>)>> = if style_in != 0 { mons.iter().map(|m| capture(&m.rect)).collect() } else { Vec::new() };
            for (mi, m) in mons.iter().enumerate() {
                let (w, h) = (m.rect.right - m.rect.left, m.rect.bottom - m.rect.top);
                // topmost (also for the fullscreen test, so it can not end up behind another window) and a tool window: no
                // taskbar button appearing and disappearing, which made the taskbar re-lay itself out and flash
                let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
                let style = WS_POPUP | WS_VISIBLE;
                // fading in: the window is created far off-screen and moved onto the monitor once two frames have been
                // presented, so the compositor never shows its blank initial surface (a black flash)
                let (wx0, wy0) = if style_in != 0 { (-32000, -32000) } else { (m.rect.left, m.rect.top) };
                let hwnd = CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), style, wx0, wy0, w, h, null_mut(), null_mut(), hinst, null());
                if hwnd.is_null() {
                    continue;
                }
                if matches!(mode, Mode::Windowed) && style_in == 0 {
                    SetForegroundWindow(hwnd); // so Esc reaches it
                }
                let black = cfg.monitors == 2 && !m.primary;
                let show_text = !(cfg.monitors > 0 && !m.primary);
                let renderer = if black {
                    None
                } else {
                    let mut rc: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut rc);
                    match Renderer::new(hwnd, rc.right.max(1), rc.bottom.max(1), &cfg, bg_idx) {
                        Ok(mut r) => {
                            if let Some(Some((sw, sh, data))) = shots.get(mi) {
                                r.set_outside(*sw, *sh, data);
                            }
                            Some(r)
                        }
                        Err(e) => {
                            log(&format!("renderer: {e}"));
                            None
                        }
                    }
                };
                wins.push(Win { hwnd, rect: m.rect, renderer, show_text });
            }
            if matches!(mode, Mode::Saver) {
                ShowCursor(0);
            }
        }
        Mode::Preview(p) => {
            parent = *p as HWND;
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(parent, &mut rc);
            let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_CHILD | WS_VISIBLE, 0, 0, rc.right, rc.bottom, parent, null_mut(), hinst, null());
            if let Ok(r) = Renderer::new(hwnd, rc.right.max(1), rc.bottom.max(1), &cfg, bg_idx) {
                wins.push(Win { hwnd, rect: rc, renderer: Some(r), show_text: true });
            }
        }
    }
    if wins.is_empty() {
        return;
    }

    let start = Instant::now();
    let t_offset = (weather::now_unix() % 1000) as f64;
    // No cap by default: SwapBuffers with vsync paces us to the display's refresh rate. Only fall back to a
    // 60 fps timer if the driver refused vsync (otherwise we'd spin a core flat out).
    let all_vsync = wins.iter().filter_map(|w| w.renderer.as_ref()).all(|r| r.vsync);
    let frame_dt = if cfg.fps > 0 {
        Some(Duration::from_secs_f64(1.0 / cfg.fps as f64))
    } else if all_vsync {
        None
    } else {
        Some(Duration::from_secs_f64(1.0 / 60.0))
    };
    let mut next = Instant::now();
    let debug = std::env::var_os("WXSCR_DEBUG").is_some();
    let (mut frames, mut last_report) = (0u32, Instant::now());
    let mut last_parent_check = Instant::now();
    loop {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                return;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !parent.is_null() && last_parent_check.elapsed() > Duration::from_secs(1) {
            last_parent_check = Instant::now();
            if IsWindow(parent) == 0 {
                return;
            }
        }
        let now = Instant::now();
        if frame_dt.is_none() || now >= next {
            let mut st = std::mem::zeroed();
            GetLocalTime(&mut st);
            let guard = WEATHER.lock().unwrap();
            if exit_t0.is_none() && EXIT_START.load(Ordering::Relaxed) != 0 {
                exit_t0 = Some(Instant::now()); // fade back into the screenshot taken at start
            }
            let leaving = exit_t0.is_some();
            let vis_in = shown_at.map_or(0.0, |t| (t.elapsed().as_secs_f64() / render::TRANS_IN_SECS).min(1.0));
            let vis_out = exit_t0.map_or(1.0, |t| 1.0 - t.elapsed().as_secs_f64() / render::TRANS_OUT_SECS);
            let (vis, tstyle) = if leaving { (vis_in.min(vis_out.max(0.0)), style_out) } else { (vis_in, style_in) };
            for w in wins.iter_mut() {
                if let Some(r) = w.renderer.as_mut() {
                    let f = Frame { t: t_offset + start.elapsed().as_secs_f64(), cfg: &cfg, wx: guard.as_ref(), show_text: w.show_text, bg: bg_idx, now: st, vis: if tstyle == 0 { 1.0 } else { vis as f32 }, tstyle };
                    r.render(&f, None);
                }
            }
            drop(guard);
            if shown_at.is_none() {
                if style_in != 0 {
                    // frame 1 (the old screen, untouched) is presented; present it once more, then slide the window onto the monitor
                    if debug {
                        log(&format!("first frame drawn after {:?}; moving window into place", start.elapsed()));
                    }
                    let guard = WEATHER.lock().unwrap();
                    for w in wins.iter_mut() {
                        if let Some(r) = w.renderer.as_mut() {
                            let f = Frame { t: t_offset + start.elapsed().as_secs_f64(), cfg: &cfg, wx: guard.as_ref(), show_text: w.show_text, bg: bg_idx, now: st, vis: 0.0, tstyle: style_in };
                            r.render(&f, None);
                        }
                    }
                    drop(guard);
                    for w in wins.iter() {
                        SetWindowPos(w.hwnd, HWND_TOPMOST, w.rect.left, w.rect.top, 0, 0, SWP_NOSIZE | SWP_SHOWWINDOW);
                        SetForegroundWindow(w.hwnd); // so key presses reach it
                    }
                }
                shown_at = Some(Instant::now());
            }
            if leaving && vis_out <= 0.0 {
                return; // faded to black
            }
            if debug {
                frames += 1;
                if last_report.elapsed() >= Duration::from_secs(5) {
                    log(&format!("fps={:.1} vsync={} cap={:?} vis={:.2} style_in={} shown={}", frames as f64 / last_report.elapsed().as_secs_f64(), all_vsync, frame_dt, vis, tstyle, shown_at.is_some()));
                    frames = 0;
                    last_report = Instant::now();
                }
            }
            if let Some(dt) = frame_dt {
                next += dt;
                if next < now {
                    next = now + dt;
                }
            }
        } else {
            let ms = (next - now).as_millis() as u32;
            MsgWaitForMultipleObjects(0, null(), 0, ms.max(1), QS_ALLINPUT);
        }
    }
}

// ───────────── developer snapshot: /snap <prefix> [w h] ─────────────
unsafe fn snap(prefix: &str, w: i32, h: i32, only: Option<usize>) {
    let (hinst, class) = register_class();
    let title = wide("snap");
    let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_POPUP, 0, 0, 64, 64, null_mut(), null_mut(), hinst, null());
    let mut cfg = Config::load();
    // dev overrides: WXSCR_CONTRAST=0..2, WXSCR_TRANS=<style>:<visibility>
    if let Some(c) = std::env::var("WXSCR_CONTRAST").ok().and_then(|v| v.parse().ok()) {
        cfg.contrast = c;
    }
    let (mut vis, mut tstyle) = (1.0f32, 0usize);
    if let Ok(v) = std::env::var("WXSCR_TRANS") {
        let p: Vec<&str> = v.split(':').collect();
        tstyle = p.first().and_then(|x| x.parse().ok()).unwrap_or(0);
        vis = p.get(1).and_then(|x| x.parse().ok()).unwrap_or(1.0);
    }
    let mut r = match Renderer::new(hwnd, w, h, &cfg, 0) {
        Ok(r) => r,
        Err(e) => return log(&format!("snap: {e}")),
    };
    let now = weather::now_unix();
    let wx = Weather {
        place: "North Canton, OH".into(),
        temp: 62.0,
        feels: 60.0,
        dew: 33.0,
        wind: Some(8.0),
        pop: Some(20.0),
        hi: Some(63.0),
        lo: Some(41.0),
        sunrise: vec![now + 40000],
        sunset: vec![now + 9000],
        utc_offset: -14400,
        fetched: now,
        sources: 11,
    };
    let mut st = std::mem::zeroed();
    GetLocalTime(&mut st);
    for i in 0..bg::NAMES.len() {
        if only.map_or(false, |o| o != i) {
            continue;
        }
        let f = Frame { t: 120.0 + i as f64 * 17.0, cfg: &cfg, wx: Some(&wx), show_text: true, bg: i, now: st, vis, tstyle };
        let px = r.snapshot(&f, w, h);
        let _ = png::write(&format!("{prefix}-{i}.png"), w as u32, h as u32, &px);
    }
}

enum Mode {
    Saver,
    Windowed,
    Preview(isize),
}

fn main() {
    // a screensaver has no console: leave a trace in log.txt if anything panics
    std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let first = args.first().map(|s| s.to_lowercase()).unwrap_or_default();
    let (flag, inline) = match first.split_once(':') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (first.clone(), None),
    };
    let flag = flag.trim_start_matches(|c| c == '/' || c == '-').to_string();
    unsafe {
        STARTED.store(GetTickCount64(), Ordering::Relaxed);
        match flag.as_str() {
            "s" => {
                SAVER.store(true, Ordering::Relaxed);
                let cfg = Config::load();
                spawn_weather(cfg.clone(), true);
                run(Mode::Saver, cfg);
            }
            "w" => {
                SAVER.store(true, Ordering::Relaxed); // same input handling as the real saver: mouse move / key / click quits
                let cfg = Config::load();
                spawn_weather(cfg.clone(), true);
                run(Mode::Windowed, cfg);
            }
            "p" => {
                let h = args.get(1).or(inline.as_ref()).and_then(|s| s.parse::<isize>().ok()).unwrap_or(0);
                let cfg = Config::load();
                spawn_weather(cfg.clone(), false);
                if h != 0 {
                    run(Mode::Preview(h), cfg);
                }
            }
            "detect" => match weather::detect() {
                Ok(d) => log(&format!("detect: {} | text={:?} | ({:.4}, {:.4}) | via {}", d.name, d.text, d.lat, d.lon, d.source)),
                Err(e) => log(&format!("detect failed: {e}")),
            },
            "snap" => {
                let a = |i: usize, d: i32| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
                let only = args.get(4).and_then(|s| s.parse().ok());
                snap(args.get(1).map(|s| s.as_str()).unwrap_or("snap"), a(2, 1280), a(3, 720), only);
            }
            "c" => {
                let parent = inline.or_else(|| args.get(1).cloned()).and_then(|s| s.parse::<isize>().ok()).unwrap_or(0);
                dialog::run(parent as HWND);
            }
            _ => dialog::run(null_mut()),
        }
    }
}
