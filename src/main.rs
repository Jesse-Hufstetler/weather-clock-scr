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

// NOTES ON THE START AND END OF A RUN (transitions + flashes)
//
// * Fading is done by the compositor: with a transition enabled the window is WS_EX_LAYERED and the renderer sets its
//   whole-window alpha (SetLayeredWindowAttributes / LWA_ALPHA) from the transition progress, so the real desktop shows
//   through live, in both directions. No screenshot of the desktop is taken. The window is created fully transparent
//   and is only made visible by the first alpha > 0, so there is no black first frame. Per-pixel effects (wipes, irises)
//   are not possible this way and were dropped; blur and zoom only affect what we render, not the desktop.
// * The window is 1 pixel shorter than the monitor: that avoids both the compositor bypass that ignores the alpha (exact
//   size) and the shell's full-screen reaction that flashes a black bar on the taskbar (see where the window is created).
// * The window is a tool window (WS_EX_TOOLWINDOW) so no taskbar button appears and disappears; that button used to make
//   the taskbar re-lay itself out and flash.
// * Screen capture (GDI, desktop duplication and Windows.Graphics.Capture were all tried) cannot see this window while
//   it is full-screen: it is presented outside the compositor's capture path. To check what the user sees, run with the
//   env var WXSCR_SMALL=1 (a 1000x560 window): a capture tool then sees it, and the alpha fade (real desktop blending
//   with the scene, no black dip at either end) was verified that way with WGC on this machine. Frames of the effects
//   alone can also be rendered with /snap (WXSCR_TRANS=<style>:<visibility>).

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
            for m in mons.iter() {
                let (mut w, mut h) = (m.rect.right - m.rect.left, m.rect.bottom - m.rect.top);
                let (mut wl, mut wt) = (m.rect.left, m.rect.top);
                // dev switch: a 1000x560 window instead of full-screen (see the notes above: capture tools cannot see full-screen ones)
                if std::env::var_os("WXSCR_SMALL").is_some() {
                    (wl, wt, w, h) = (400, 300, 1000, 560);
                }
                // One pixel short of the full monitor height, on purpose (the bottom pixel row shows what is behind us):
                //  * a window that covers the whole monitor makes the shell react (it demotes and redraws the taskbar), which
                //    showed up as a ~10 px black bar along the top of the taskbar for ~0.4 s at start and at exit; measured
                //    frame by frame with Windows.Graphics.Capture, sizes of monitor+0/+1/+2 all showed it, monitor-1 in
                //    either dimension never did, and the window still covers the taskbar;
                //  * a window that exactly matches the monitor is also presented outside the compositor, which ignores its
                //    alpha (no fade), so the exact size was no option for transitions either.
                h -= 1;
                // topmost (also for the fullscreen test, so it can not end up behind another window) and a tool window: no
                // taskbar button appearing and disappearing, which made the taskbar re-lay itself out and flash
                let mut ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
                if style_in != 0 {
                    ex |= WS_EX_LAYERED; // fade in/out through the window's alpha, so the real desktop shows through
                }
                let hwnd = CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), WS_POPUP | WS_VISIBLE, wl, wt, w, h, null_mut(), null_mut(), hinst, null());
                if hwnd.is_null() {
                    continue;
                }
                if style_in != 0 {
                    SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA); // invisible until the first frame has been drawn
                }
                if matches!(mode, Mode::Windowed) {
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
                        Ok(r) => Some(r),
                        Err(e) => {
                            log(&format!("renderer: {e}"));
                            None
                        }
                    }
                };
                wins.push(Win { renderer, show_text });
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
                wins.push(Win { renderer: Some(r), show_text: true });
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
                exit_t0 = Some(Instant::now());
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
                // frame 1 is drawn (at alpha 0): the fade-in starts from here
                if debug {
                    log(&format!("first frame drawn after {:?}", start.elapsed()));
                }
                shown_at = Some(Instant::now());
            }
            if leaving && vis_out <= 0.0 {
                return; // fully faded out
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

// ───────────── developer benchmark: /bench [w h [frames [only]]] ─────────────
/// Renders every background offscreen at w x h and logs the average ms per frame (and the fps that implies).
unsafe fn bench(w: i32, h: i32, frames: u32, only: Option<usize>) {
    let (hinst, class) = register_class();
    let hwnd = CreateWindowExW(0, class.as_ptr(), wide("bench").as_ptr(), WS_POPUP, 0, 0, 64, 64, null_mut(), null_mut(), hinst, null());
    let cfg = Config::load();
    let mut r = match Renderer::new(hwnd, w, h, &cfg, 0) {
        Ok(r) => r,
        Err(e) => return log(&format!("bench: {e}")),
    };
    let wx = weather::load_cache().unwrap_or_else(weather::sample);
    log(&format!("bench {w}x{h}, {frames} frames each"));
    for i in 0..bg::NAMES.len() {
        if only.map_or(false, |o| o != i) {
            continue;
        }
        let ms = r.bench(&cfg, &wx, i, w, h, frames);
        log(&format!("bench {:>2} {:<46} {:7.2} ms  {:6.1} fps", i, bg::NAMES[i], ms, 1000.0 / ms));
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
            "bench" => {
                let a = |i: usize, d: i32| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
                let only = args.get(4).and_then(|s| s.parse().ok());
                bench(a(1, 1920), a(2, 1080), a(3, 60) as u32, only);
            }
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
