//! One OpenGL context per window. Pipeline per frame:
//!   1. background shader -> half-float offscreen buffer at native resolution
//!   2. blit to the window with a touch of noise dither (kills banding) and a brightness control
//!   3. text runs (GDI-rasterised masks) with a soft shadow, scaled/drifted for burn-in protection

use crate::bg;
use crate::config::Config;
use crate::flurry::{self, Flurry};
use crate::options;
use crate::gl::{self, Gl};
use crate::text::Rasterizer;
use crate::weather::Weather;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use windows_sys::Win32::Foundation::{HWND, SYSTEMTIME};
use windows_sys::Win32::Graphics::Gdi::{GetDC, HDC};
use windows_sys::Win32::Graphics::OpenGL::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongW, SetLayeredWindowAttributes, GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED};

const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_LUMINANCE: u32 = 0x1909;
const GL_RGBA32F: i32 = 0x8814;
const GL_FLOAT_: u32 = 0x1406;
const GL_REPEAT: u32 = 0x2901;
const NOISE_N: usize = 256;
const GL_LUMINANCE_ALPHA_: u32 = 0x190A;
const GL_LINEAR_MIPMAP_NEAREST_: i32 = 0x2701;
const GL_FUNC_ADD_: u32 = 0x8006;
const GL_FUNC_SUBTRACT_: u32 = 0x800A;
const GL_TEXTURE_ENV_: u32 = 0x2300;
const GL_TEXTURE_ENV_MODE_: u32 = 0x2200;
const GL_MODULATE_: i32 = 0x2100;
/// Multipass simulations advance at this fixed rate, whatever the display's refresh rate is.
const SIM_STEP: f64 = 1.0 / 60.0;

/// Smootherstep easing used for every transition (the shader has the same function).
pub fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

pub const CONTRAST_NAMES: [&str; 3] = ["Soft shadow", "Black halo (blurs out)", "Black outline"];
/// Transition styles; index 0 is "None" and the last entry picks a random one each time.
pub const TRANS_NAMES: [&str; 5] = ["None (instant)", "Fade", "Blur fade", "Zoom", "Random"];
/// Stable names stored in config.json (same order as TRANS_NAMES), so reordering or removing styles never changes what a
/// saved choice means.
pub const TRANS_IDS: [&str; 5] = ["none", "fade", "blur", "zoom", "random"];

/// Older configs stored the position in an earlier list (none, fade, blur, wipe, slide, iris, zoom, random).
pub fn transition_from_legacy(n: u64) -> usize {
    match n {
        0 => 0,
        1 => 1,
        2 => 2,
        6 => 3, // zoom
        7 => 4, // random
        _ => 2, // wipe, slide and iris no longer exist: use the default (blur fade)
    }
}

/// Seconds the screensaver takes to appear / to disappear after you touch something.
pub const TRANS_IN_SECS: f64 = 0.9;
pub const TRANS_OUT_SECS: f64 = 0.5;

/// Turn the configured choice into a concrete style 0..=3 (the "Random" entry rolls a die from `salt`).
pub fn resolve_transition(choice: usize, salt: u64) -> usize {
    if choice + 1 >= TRANS_NAMES.len() {
        1 + (salt.wrapping_mul(2654435761) >> 7) as usize % (TRANS_NAMES.len() - 2)
    } else {
        choice
    }
}

pub struct Frame<'a> {
    pub t: f64,
    pub cfg: &'a Config,
    pub wx: Option<&'a Weather>,
    pub show_text: bool,
    pub bg: usize,
    pub now: SYSTEMTIME,
    /// Transition state: how visible the scene is (1 = normal, 0 = the old screen) and the style (see TRANS_NAMES).
    pub vis: f32,
    pub tstyle: usize,
}

#[derive(Default, Clone, Copy)]
struct Run {
    tex: u32,
    w: i32,
    h: i32,
    pad: i32,
    /// Pre-blurred, low-resolution copy of the mask used for the shadow / halo / outline (bilinear-upscaled = smooth).
    sh_tex: u32,
}

/// A text run rasterised and blurred off the render thread (or on it, the first time).
struct Prepared {
    w: i32,
    h: i32,
    pad: i32,
    data: Vec<u8>,
    sdata: Vec<u8>,
    sw: usize,
    sh: usize,
}

struct Job {
    key: String,
    text: String,
    px: i32,
    pad: i32,
    th: i32,
    style: usize,
    face: String,
}

/// Rasterise `text` and build the blurred companion mask for the chosen contrast style.
fn prepare(rast: &mut Rasterizer, text: &str, px: i32, pad: i32, th: i32, style: usize) -> Prepared {
    let r = rast.raster(text, px, pad);
    let thf = th as f32;
    let (f, radius) = match style {
        // halo: a wide blur; the shader turns it into "fully black near the text, fading to nothing"
        1 => {
            let f = (th / 270).max(2);
            (f, ((0.017 * thf) / f as f32 - 0.5).round().max(1.0) as usize)
        }
        // outline: an exact distance-based dilation at full resolution, so the thickness is the same everywhere
        2 => {
            let sdata = outline_mask(&r.data, r.w as usize, r.h as usize, 0.006 * thf);
            let (w, h) = (r.w as usize, r.h as usize);
            return Prepared { w: r.w, h: r.h, pad: r.pad, data: r.data, sdata, sw: w, sh: h };
        }
        // soft shadow (the original look)
        _ => {
            let f = (th / 270).max(2);
            (f, ((0.0075 * thf) / f as f32).round().max(1.0) as usize)
        }
    };
    let (sdata, sw, sh) = blur_lowres(&r.data, r.w as usize, r.h as usize, f as usize, radius);
    Prepared { w: r.w, h: r.h, pad: r.pad, data: r.data, sdata, sw, sh }
}

struct Locs {
    bg_res: i32,
    bg_time: i32,
    bub: i32,
    bcol: i32,
    blit_src: i32,
    blit_bright: i32,
    blit_t: i32,
    text_tex: i32,
    text_alpha: i32,
    text_shoff: i32,
    text_sh: i32,
    text_mode: i32,
    text_pass: i32,
    tr_scene: i32,
    tr_small: i32,
    tr_dark: i32,
    tr_outside: i32,
    tr_have: i32,
    tr_p: i32,
    tr_style: i32,
    tr_res: i32,
}

/// A Shadertoy-style multipass background: up to four buffers (each a ping-pong pair of float textures) + Image.
struct MultiState {
    spec: bg::Multi,
    progs: Vec<u32>, // one per buffer, then Image last
    tex: Vec<[u32; 2]>,
    /// Which texture of each pair holds the buffer's latest finished frame.
    read: Vec<usize>,
    mip: Vec<bool>,
    noise: u32,
    sw: i32,
    sh: i32,
    /// Pixel size of each buffer (the simulation size unless the pass asks for a fixed one).
    bsize: Vec<(i32, i32)>,
    last_step: f64,
}

/// The Flurry background: the particle simulation, its texture, and the buffer size it last drew into (the buffer is
/// persistent, so it is cleared whenever that changes).
struct FlurryState {
    sim: Flurry,
    tex: u32,
    last_t: f64,
    size: (i32, i32),
}

pub struct Renderer {
    /// True when the driver honoured a swap interval of 1 (so SwapBuffers paces frames to the display).
    pub vsync: bool,
    hdc: HDC,
    rc: HGLRC,
    gl: Gl,
    pub w: i32,
    pub h: i32,
    bg: usize,
    bg_prog: u32,
    blit_prog: u32,
    text_prog: u32,
    loc: Locs,
    fbo: u32,
    fbo_tex: u32,
    fw: i32,
    fh: i32,
    rast: Rasterizer,
    face: String,
    runs: [Run; 8],
    keys: [String; 8],
    /// Background worker that rasterises the next clock string ahead of time, so the once-a-second text
    /// change costs only a texture upload on the render thread.
    jobs: Sender<Job>,
    done: Receiver<(String, Prepared)>,
    ready: HashMap<String, Prepared>,
    pending: HashSet<String>,
    multi: Option<MultiState>,
    flurry: Option<FlurryState>,
    /// Simulation clock (see `render`): advances by the elapsed wall time times the speed setting.
    sim_clock: f64,
    sim_prev: f64,
    sim_fbo: u32,
    trans_prog: u32,
    down_prog: u32,
    down_src: i32,
    down_texel: i32,
    /// Quarter-resolution copy of the scene for the blur transition.
    bl_fbo: [u32; 2],
    bl_tex: [u32; 2],
    blur_prog: u32,
    blur_src: i32,
    blur_r: i32,
    blur_ia: i32,
    bl_w: i32,
    bl_h: i32,
    /// Offscreen copy of the finished frame, only used while a transition is running.
    sc_fbo: u32,
    sc_tex: u32,
    sc_w: i32,
    sc_h: i32,
    hwnd: HWND,
    /// A picture of the user's desktop (see desktopshot.rs) and whether the fade should go to it right now.
    out_tex: u32,
    pub out_have: bool,
    pub out_use: bool,
    /// The window is WS_EX_LAYERED: fades go through its alpha so the real desktop shows through.
    layered: bool,
    alpha: i32,
}

const BLIT_FS: &str = "#version 120
uniform sampler2D src; uniform float bright; uniform float t; varying vec2 uv;
void main(){
  vec3 c = texture2D(src, uv).rgb * bright;
  float n = fract(sin(dot(gl_FragCoord.xy + vec2(fract(t)*91.0, fract(t*1.7)*57.0), vec2(12.9898, 78.233))) * 43758.5453);
  float m = fract(sin(dot(gl_FragCoord.yx + vec2(fract(t*0.3)*37.0, 11.0), vec2(39.3468, 11.135))) * 24634.6345);
  c += (n + m - 1.0) / 255.0;
  gl_FragColor = vec4(c, 1.0);
}";

/// `mode` 0: soft drop shadow. 1: black halo, like stacking blurred black copies behind the text so it goes from
/// fully opaque next to the glyphs to fully transparent. 2: thick black outline (distance-based dilation of the glyphs).
const TEXT_FS: &str = "#version 120
uniform sampler2D tex; uniform sampler2D sh; uniform float alpha; uniform vec2 shoff; uniform int mode; uniform int pass; varying vec2 uv;
void main(){
  float m = texture2D(tex, uv).r;
  float s;
  if (mode == 1) {
    s = 1.0 - exp(-9.0 * texture2D(sh, uv).r);
  } else if (mode == 2) {
    s = texture2D(sh, uv).r;
  } else {
    s = texture2D(sh, uv - shoff).r + texture2D(sh, uv + shoff).r;
    s = clamp(s * 1.25, 0.0, 1.0) * 0.9;
  }
  // pass 0 paints every run's black backdrop (shadow / halo / outline), pass 1 then paints the glyphs over all of it,
  // so one run's outline never cuts into a neighbouring run's letters
  if (pass == 0) gl_FragColor = vec4(0.0, 0.0, 0.0, s) * alpha;
  else gl_FragColor = vec4(vec3(m), m) * alpha;
}";

/// Box-downsample by 4 in one pass: four bilinear taps at +-1 source texel, each averaging a 2x2 block.
const DOWN_FS: &str = "#version 120
uniform sampler2D src; uniform vec2 texel; varying vec2 uv;
void main(){
  vec3 c = texture2D(src, uv + texel * vec2( 1.0,  1.0)).rgb + texture2D(src, uv + texel * vec2(-1.0,  1.0)).rgb
         + texture2D(src, uv + texel * vec2( 1.0, -1.0)).rgb + texture2D(src, uv + texel * vec2(-1.0, -1.0)).rgb;
  gl_FragColor = vec4(c * 0.25, 1.0);
}";

/// Blur of the quarter-resolution copy: 28 taps on a golden-angle spiral (offsets precomputed, they are the same for every pixel).
/// Runs at quarter resolution only, because the result is smooth anyway.
const BLUR_FS: &str = "#version 120
uniform sampler2D src; uniform float r; uniform float ia; varying vec2 uv;
void main(){
  vec3 acc = vec3(0.0);
  acc += texture2D(src, clamp(uv + vec2(0.13363 * ia, 0.00000) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.17067 * ia, 0.15635) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.02612 * ia, -0.29766) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.21512 * ia, 0.28058) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.39476 * ia, -0.06982) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.37395 * ia, -0.23789) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.12507 * ia, 0.46530) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.23855 * ia, -0.45929) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.51755 * ia, 0.18899) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.53841 * ia, 0.22227) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.25953 * ia, -0.55466) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.19182 * ia, 0.61149) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.57811 * ia, -0.33499) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.67816 * ia, -0.14912) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.41385 * ia, 0.58872) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.09565 * ia, -0.73785) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.58701 * ia, 0.49468) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.78989 * ia, 0.03271) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.57613 * ia, -0.57340) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.03850 * ia, 0.83363) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.54827 * ia, -0.65692) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.86846 * ia, 0.11679) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.73580 * ia, 0.51203) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.20101 * ia, -0.89380) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.46513 * ia, 0.81157) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.90919 * ia, -0.28998) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(0.88311 * ia, -0.40811) * r, 0.0, 1.0)).rgb;
  acc += texture2D(src, clamp(uv + vec2(-0.38252 * ia, 0.91423) * r, 0.0, 1.0)).rgb;
  gl_FragColor = vec4(acc / 28.0, 1.0);
}";

/// Final pass while the screensaver is appearing or disappearing, for the effects that need a shader: `p` is visibility
/// (0 hidden .. 1 normal). In a layered window the compositor does the fading (the real desktop shows through), so this only
/// blurs or zooms what we rendered; `dark` is 1 where there is no layered window (the settings preview) and then also
/// fades to black here.
const TRANS_FS: &str = "#version 120
uniform sampler2D scene; uniform sampler2D small; uniform sampler2D outside; uniform float have; uniform float p; uniform int style; uniform float dark; uniform vec2 res; varying vec2 uv;
float ease(float x){ x = clamp(x, 0.0, 1.0); return x*x*x*(x*(x*6.0-15.0)+10.0); }
vec3 B(vec2 c){ return texture2D(scene, clamp(c, 0.0, 1.0)).rgb; }
vec3 S(vec2 c){ return texture2D(small, clamp(c, 0.0, 1.0)).rgb; } // quarter-resolution blurred copy (style 2)
void main(){
  float e = ease(p);
  float asp = res.x / res.y;
  vec3 c;
  if (style == 2) {
    // blur fade: the picture comes into focus as it appears (and defocuses as it leaves). The blur itself is computed
    // at quarter resolution (BLUR_FS); here it is just upscaled, handing over to the sharp image near the end.
    c = mix(S(uv), B(uv), smoothstep(0.8, 1.0, e));
  } else if (style == 3) {
    // zoom: pulls back from a close-up as it appears (and dives in as it leaves)
    float s = mix(0.62, 1.0, e);
    c = B((uv - 0.5) * s + 0.5);
  } else {
    c = B(uv);
  }
  // without a layered window the fade goes to a picture of the user's desktop when one is loaded, else to black
  vec3 other = have > 0.5 ? texture2D(outside, uv).rgb : vec3(0.0);
  c = mix(other, c, mix(1.0, e, dark));
  gl_FragColor = vec4(c, 1.0);
}";

impl Renderer {
    pub unsafe fn new(hwnd: HWND, w: i32, h: i32, cfg: &Config, bg_idx: usize) -> Result<Renderer, String> {
        let hdc = GetDC(hwnd);
        let mut pfd: PIXELFORMATDESCRIPTOR = std::mem::zeroed();
        pfd.nSize = std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16;
        pfd.nVersion = 1;
        pfd.dwFlags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER | PFD_SUPPORT_COMPOSITION; // composition: let the compositor handle this window (alpha, capture)
        pfd.iPixelType = PFD_TYPE_RGBA;
        pfd.cColorBits = 24;
        let fmt = ChoosePixelFormat(hdc, &pfd);
        if fmt == 0 || SetPixelFormat(hdc, fmt, &pfd) == 0 {
            return Err("no usable pixel format".into());
        }
        let rc = wglCreateContext(hdc);
        if rc.is_null() || wglMakeCurrent(hdc, rc) == 0 {
            return Err("could not create an OpenGL context".into());
        }
        let g = Gl::load()?;
        let mut vsync = false;
        if let Some(p) = wglGetProcAddress(b"wglSwapIntervalEXT\0".as_ptr()) {
            let f: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(p);
            vsync = f(1) != 0;
        }
        let blit_prog = gl::program(&g, BLIT_FS)?;
        let text_prog = gl::program(&g, TEXT_FS)?;
        let trans_prog = gl::program(&g, TRANS_FS)?;
        let down_prog = gl::program(&g, DOWN_FS)?;
        let (down_src, down_texel) = (gl::uloc(&g, down_prog, "src"), gl::uloc(&g, down_prog, "texel"));
        let blur_prog = gl::program(&g, BLUR_FS)?;
        let (blur_src, blur_r, blur_ia) = (gl::uloc(&g, blur_prog, "src"), gl::uloc(&g, blur_prog, "r"), gl::uloc(&g, blur_prog, "ia"));
        let bg_prog = gl::program(&g, &bg::source(bg_idx))?;
        let loc = Locs {
            bg_res: gl::uloc(&g, bg_prog, "wx_res"),
            bg_time: gl::uloc(&g, bg_prog, "iTime"),
            bub: gl::uloc(&g, bg_prog, "bub"),
            bcol: gl::uloc(&g, bg_prog, "bcol"),
            blit_src: gl::uloc(&g, blit_prog, "src"),
            blit_bright: gl::uloc(&g, blit_prog, "bright"),
            blit_t: gl::uloc(&g, blit_prog, "t"),
            text_tex: gl::uloc(&g, text_prog, "tex"),
            text_alpha: gl::uloc(&g, text_prog, "alpha"),
            text_shoff: gl::uloc(&g, text_prog, "shoff"),
            text_sh: gl::uloc(&g, text_prog, "sh"),
            text_mode: gl::uloc(&g, text_prog, "mode"),
            text_pass: gl::uloc(&g, text_prog, "pass"),
            tr_scene: gl::uloc(&g, trans_prog, "scene"),
            tr_small: gl::uloc(&g, trans_prog, "small"),
            tr_dark: gl::uloc(&g, trans_prog, "dark"),
            tr_outside: gl::uloc(&g, trans_prog, "outside"),
            tr_have: gl::uloc(&g, trans_prog, "have"),
            tr_p: gl::uloc(&g, trans_prog, "p"),
            tr_style: gl::uloc(&g, trans_prog, "style"),
            tr_res: gl::uloc(&g, trans_prog, "res"),
        };
        let (jobs, job_rx) = channel::<Job>();
        let (done_tx, done) = channel::<(String, Prepared)>();
        std::thread::spawn(move || {
            // the worker owns its own GDI rasteriser (a device context must stay on the thread that made it)
            let mut rast: Option<(String, Rasterizer)> = None;
            while let Ok(job) = job_rx.recv() {
                if rast.as_ref().map_or(true, |(face, _)| *face != job.face) {
                    rast = Some((job.face.clone(), Rasterizer::new(&job.face)));
                }
                let p = prepare(&mut rast.as_mut().unwrap().1, &job.text, job.px, job.pad, job.th, job.style);
                if done_tx.send((job.key, p)).is_err() {
                    break;
                }
            }
        });
        let mut r = Renderer {
            vsync,
            hdc,
            rc,
            gl: g,
            w,
            h,
            // multipass backgrounds are built on the first frame (render() sees the index differ)
            bg: if bg::is_multi(bg_idx) || bg::is_flurry(bg_idx) { usize::MAX } else { bg_idx },
            bg_prog,
            blit_prog,
            text_prog,
            loc,
            fbo: 0,
            fbo_tex: 0,
            fw: 0,
            fh: 0,
            rast: Rasterizer::new(&cfg.font),
            face: cfg.font.clone(),
            runs: [Run::default(); 8],
            keys: Default::default(),
            jobs,
            done,
            ready: HashMap::new(),
            pending: HashSet::new(),
            multi: None,
            flurry: None,
            sim_clock: 0.0,
            sim_prev: f64::NAN,
            sim_fbo: 0,
            trans_prog,
            down_prog,
            down_src,
            down_texel,
            bl_fbo: [0; 2],
            bl_tex: [0; 2],
            blur_prog,
            blur_src,
            blur_r,
            blur_ia,
            bl_w: 0,
            bl_h: 0,
            sc_fbo: 0,
            sc_tex: 0,
            sc_w: 0,
            sc_h: 0,
            hwnd,
            out_tex: 0,
            out_have: false,
            out_use: false,
            layered: GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0,
            alpha: -1,
        };
        r.ensure_fbo(w, h);
        Ok(r)
    }

    /// First pending OpenGL error (0 = none); for the diagnostic log.
    pub unsafe fn gl_error(&self) -> u32 {
        glGetError()
    }

    /// Driver identification for the diagnostic log.
    pub unsafe fn driver_info(&self) -> String {
        let s = |n: u32| {
            let p = glGetString(n);
            if p.is_null() { String::from("?") } else { std::ffi::CStr::from_ptr(p as *const _).to_string_lossy().into_owned() }
        };
        format!("{} | {} | {}", s(GL_VENDOR), s(GL_RENDERER), s(GL_VERSION))
    }

    /// Swap the text font (settings preview); cached glyph runs are rebuilt on the next frame.
    pub fn set_font(&mut self, face: &str) {
        self.rast = Rasterizer::new(face);
        self.face = face.to_string();
        self.keys = Default::default();
        self.ready.clear();
        self.pending.clear();
    }

    /// Load a picture of the user's desktop (bottom-up BGRA, like an OpenGL texture) for the fade-out to end on.
    pub unsafe fn set_outside(&mut self, w: i32, h: i32, bgra: &[u8]) {
        const GL_BGRA: u32 = 0x80E1;
        wglMakeCurrent(self.hdc, self.rc);
        if self.out_tex == 0 {
            glGenTextures(1, &mut self.out_tex);
        }
        glBindTexture(GL_TEXTURE_2D, self.out_tex);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA as i32, w, h, 0, GL_BGRA, GL_UNSIGNED_BYTE, bgra.as_ptr() as *const _);
        self.out_have = true;
    }

    /// Quarter-resolution targets for the blur transition: [0] the shrunk scene, [1] its blur.
    unsafe fn ensure_blur(&mut self, tw: i32, th: i32) {
        let (w, h) = ((tw / 4).max(16), (th / 4).max(16));
        if self.bl_fbo[0] != 0 && w == self.bl_w && h == self.bl_h {
            return;
        }
        for k in 0..2 {
            if self.bl_fbo[k] == 0 {
                (self.gl.GenFramebuffers)(1, &mut self.bl_fbo[k]);
            } else {
                glDeleteTextures(1, &self.bl_tex[k]);
            }
            glGenTextures(1, &mut self.bl_tex[k]);
            glBindTexture(GL_TEXTURE_2D, self.bl_tex[k]);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
            glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA as i32, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
            (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.bl_fbo[k]);
            (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, self.bl_tex[k], 0);
        }
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, 0);
        self.bl_w = w;
        self.bl_h = h;
    }

    /// Offscreen colour buffer the finished frame is drawn into while a transition runs.
    unsafe fn ensure_scene(&mut self, tw: i32, th: i32) {
        let (w, h) = (tw.max(16), th.max(16));
        if self.sc_fbo != 0 && w == self.sc_w && h == self.sc_h {
            return;
        }
        if self.sc_fbo == 0 {
            (self.gl.GenFramebuffers)(1, &mut self.sc_fbo);
        } else {
            glDeleteTextures(1, &self.sc_tex);
        }
        glGenTextures(1, &mut self.sc_tex);
        glBindTexture(GL_TEXTURE_2D, self.sc_tex);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA as i32, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.sc_fbo);
        (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, self.sc_tex, 0);
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, 0);
        self.sc_w = w;
        self.sc_h = h;
    }

    unsafe fn ensure_fbo(&mut self, tw: i32, th: i32) {
        let (fw, fh) = (tw.max(16), th.max(16));
        if self.fbo != 0 && fw == self.fw && fh == self.fh {
            return;
        }
        if self.fbo_tex != 0 {
            glDeleteTextures(1, &self.fbo_tex);
        } else {
            (self.gl.GenFramebuffers)(1, &mut self.fbo);
        }
        glGenTextures(1, &mut self.fbo_tex);
        glBindTexture(GL_TEXTURE_2D, self.fbo_tex);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        // half-float keeps dark gradients smooth; fall back to 8-bit if the driver refuses it
        for (internal, ty) in [(gl::RGBA16F, gl::HALF_FLOAT), (GL_RGBA, GL_UNSIGNED_BYTE)] {
            glTexImage2D(GL_TEXTURE_2D, 0, internal as i32, fw, fh, 0, GL_RGBA, ty, std::ptr::null());
            (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.fbo);
            (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, self.fbo_tex, 0);
            if (self.gl.CheckFramebufferStatus)(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE {
                break;
            }
        }
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, 0);
        self.fw = fw;
        self.fh = fh;
    }

    unsafe fn quad(x0: f32, y0: f32, x1: f32, y1: f32, text: bool) {
        let (vb, vt) = if text { (1.0, 0.0) } else { (0.0, 1.0) };
        // NDC corners; uv (0,0) at top-left so the top-down GDI masks come out upright
        glBegin(GL_QUADS);
        glTexCoord2f(0.0, vb);
        glVertex2f(x0, y0);
        glTexCoord2f(1.0, vb);
        glVertex2f(x1, y0);
        glTexCoord2f(1.0, vt);
        glVertex2f(x1, y1);
        glTexCoord2f(0.0, vt);
        glVertex2f(x0, y1);
        glEnd();
    }

    fn text_key(&self, text: &str, px: i32, pad: i32, style: usize) -> String {
        format!("{}|{px}|{pad}|{style}|{text}", self.face)
    }

    /// Collect finished background rasterisations.
    fn drain(&mut self) {
        while let Ok((k, p)) = self.done.try_recv() {
            self.pending.remove(&k);
            if self.ready.len() > 8 {
                self.ready.clear();
            }
            self.ready.insert(k, p);
        }
    }

    /// Ask the worker to rasterise a string we expect to need soon (e.g. the clock text for the next second).
    fn prefetch(&mut self, text: &str, px: i32, pad: i32, th: i32, style: usize) {
        let key = self.text_key(text, px, pad, style);
        if self.ready.contains_key(&key) || self.pending.contains(&key) || self.keys.contains(&key) {
            return;
        }
        let job = Job { key: key.clone(), text: text.to_string(), px, pad, th, style, face: self.face.clone() };
        if self.jobs.send(job).is_ok() {
            self.pending.insert(key);
        }
    }

    unsafe fn upload(&mut self, slot: usize, text: &str, px: i32, pad: i32, th: i32, style: usize) {
        let key = self.text_key(text, px, pad, style);
        if self.keys[slot] == key {
            return;
        }
        self.drain();
        // use the worker's result if it has one; otherwise (first frame, size change) do it here
        let r = match self.ready.remove(&key) {
            Some(p) => p,
            None => prepare(&mut self.rast, text, px, pad, th, style),
        };
        let (sw, sh) = (r.sw as i32, r.sh as i32);
        let run = &mut self.runs[slot];
        if run.tex == 0 {
            glGenTextures(1, &mut run.tex);
            glGenTextures(1, &mut run.sh_tex);
        }
        glBindTexture(GL_TEXTURE_2D, run.sh_tex);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glTexImage2D(GL_TEXTURE_2D, 0, gl::LUMINANCE8 as i32, sw, sh, 0, GL_LUMINANCE, GL_UNSIGNED_BYTE, r.sdata.as_ptr() as *const _);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        glBindTexture(GL_TEXTURE_2D, run.tex);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glTexImage2D(GL_TEXTURE_2D, 0, gl::LUMINANCE8 as i32, r.w, r.h, 0, GL_LUMINANCE, GL_UNSIGNED_BYTE, r.data.as_ptr() as *const _);
        // no mipmaps: GenerateMipmap on this texture stalled ~300 ms on every clock tick, and the text is drawn at ~1:1 anyway
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        *run = Run { tex: run.tex, w: r.w, h: r.h, pad: r.pad, sh_tex: run.sh_tex };
        self.keys[slot] = key;
    }

    /// Switch background: single-pass shaders swap one program; multipass ones build their buffers.
    unsafe fn set_background(&mut self, idx: usize, tw: i32, th: i32) {
        self.free_multi();
        self.free_flurry();
        if bg::is_flurry(idx) {
            self.flurry = Some(self.build_flurry());
            self.bg = idx;
            return;
        }
        if let Some(spec) = bg::multi(idx) {
            match self.build_multi(spec, tw, th) {
                Ok(m) => {
                    self.multi = Some(m);
                }
                Err(e) => {
                    crate::log(&format!("multipass background failed: {e}"));
                    // fall back to the first background so the screen is never blank
                    self.bg_prog = gl::program(&self.gl, &bg::source(0)).unwrap_or(self.bg_prog);
                    self.loc.bg_res = gl::uloc(&self.gl, self.bg_prog, "wx_res");
                    self.loc.bg_time = gl::uloc(&self.gl, self.bg_prog, "iTime");
                    self.loc.bub = gl::uloc(&self.gl, self.bg_prog, "bub");
        self.loc.bcol = gl::uloc(&self.gl, self.bg_prog, "bcol");
                }
            }
            self.bg = idx;
            return;
        }
        self.bg_prog = gl::program(&self.gl, &bg::source(idx)).unwrap_or(self.bg_prog);
        self.loc.bg_res = gl::uloc(&self.gl, self.bg_prog, "wx_res");
        self.loc.bg_time = gl::uloc(&self.gl, self.bg_prog, "iTime");
        self.loc.bub = gl::uloc(&self.gl, self.bg_prog, "bub");
        self.loc.bcol = gl::uloc(&self.gl, self.bg_prog, "bcol");
        self.bg = idx;
    }

    unsafe fn build_multi(&mut self, spec: bg::Multi, tw: i32, th: i32) -> Result<MultiState, String> {
        let n = spec.buffers.len();
        let mut progs = Vec::with_capacity(n + 1);
        for (k, p) in spec.buffers.iter().enumerate() {
            progs.push(gl::program(&self.gl, &bg::pass_source(spec.common, &p.code)).map_err(|e| format!("buffer {k}: {e}"))?);
        }
        progs.push(gl::program(&self.gl, &bg::pass_source(spec.common, &spec.image.code)).map_err(|e| format!("image: {e}"))?);
        let sh = spec.sim_height;
        let sw = ((sh as f32 * tw as f32 / th.max(1) as f32).round() as i32).max(16);
        // which buffers are read through their mip chain by some pass, and whether anything wants the noise texture
        let mut mip = vec![false; n];
        let mut wants_noise = false;
        for p in spec.buffers.iter().chain(std::iter::once(&spec.image)) {
            for s in p.inputs {
                match s {
                    bg::Src::Buf(b, true) => mip[b] = true,
                    bg::Src::Noise => wants_noise = true,
                    _ => {}
                }
            }
        }
        let mut noise = 0u32;
        if wants_noise {
            glGenTextures(1, &mut noise);
            glBindTexture(GL_TEXTURE_2D, noise);
            glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
            glTexImage2D(GL_TEXTURE_2D, 0, GL_RGB as i32, NOISE_N as i32, NOISE_N as i32, 0, GL_RGB, GL_UNSIGNED_BYTE, noise_texture().as_ptr() as *const _);
            (self.gl.GenerateMipmap)(GL_TEXTURE_2D);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_REPEAT as i32);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_REPEAT as i32);
        }
        let bsize: Vec<(i32, i32)> = spec.buffers.iter().map(|p| p.size.unwrap_or((sw, sh))).collect();
        let mut tex = vec![[0u32; 2]; n];
        if self.sim_fbo == 0 {
            (self.gl.GenFramebuffers)(1, &mut self.sim_fbo);
        }
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.sim_fbo);
        for k in 0..n {
            for j in 0..2 {
                glGenTextures(1, &mut tex[k][j]);
                glBindTexture(GL_TEXTURE_2D, tex[k][j]);
                // 32-bit float: feedback trails multiply by <1 every step, which half-float rounding would stall
                glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA32F, bsize[k].0, bsize[k].1, 0, GL_RGBA, GL_FLOAT_, std::ptr::null());
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, if mip[k] { gl::LINEAR_MIPMAP_LINEAR } else { GL_LINEAR as i32 });
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
                // start from black
                (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex[k][j], 0);
                glViewport(0, 0, bsize[k].0, bsize[k].1);
                glClearColor(0.0, 0.0, 0.0, 0.0);
                glClear(GL_COLOR_BUFFER_BIT);
                if mip[k] {
                    (self.gl.GenerateMipmap)(GL_TEXTURE_2D);
                }
            }
        }
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, 0);
        Ok(MultiState { spec, progs, tex, read: vec![0; n], mip, noise, sw, sh, bsize, last_step: f64::NEG_INFINITY })
    }

    unsafe fn build_flurry(&mut self) -> FlurryState {
        let mut tex = 0u32;
        glGenTextures(1, &mut tex);
        glBindTexture(GL_TEXTURE_2D, tex);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_LUMINANCE_ALPHA_ as i32, 256, 256, 0, GL_LUMINANCE_ALPHA_, GL_UNSIGNED_BYTE, flurry::texture().as_ptr() as *const _);
        (self.gl.GenerateMipmap)(GL_TEXTURE_2D);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR_MIPMAP_NEAREST_);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        FlurryState { sim: Flurry::new(), tex, last_t: f64::NAN, size: (0, 0) }
    }

    unsafe fn free_flurry(&mut self) {
        if let Some(f) = self.flurry.take() {
            glDeleteTextures(1, &f.tex);
        }
    }

    /// Flurry: fade the persistent buffer a little towards the background colour, then draw this frame's particles on top.
    unsafe fn render_flurry(&mut self, f: &Frame, speed: f64) {
        let Some(mut st) = self.flurry.take() else { return };
        let p = flurry::Params::from_values(&options::values_for(f.cfg, bg::FLURRY));
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.fbo);
        glViewport(0, 0, self.fw, self.fh);
        (self.gl.UseProgram)(0);
        (self.gl.ActiveTexture)(gl::TEXTURE0);
        let b = p.back;
        if st.size != (self.fw, self.fh) {
            st.size = (self.fw, self.fh);
            glClearColor(b[0], b[1], b[2], 1.0);
            glClear(GL_COLOR_BUFFER_BIT);
        }
        // the speed setting scales the simulated time step; the trail fade below follows it so trails keep their length
        let dt = (if st.last_t.is_finite() && f.t >= st.last_t { (f.t - st.last_t).min(0.1) } else { 1.0 / 60.0 }) * speed;
        st.last_t = f.t;
        st.sim.step(dt, &p, self.fw as f64, self.fh as f64);

        glEnable(GL_BLEND);
        glDisable(GL_TEXTURE_2D);
        (self.gl.BlendEquation)(GL_FUNC_ADD_);
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
        // the fade is per 1/60 s, so the trails last equally long at any frame rate
        let a = (1.0 - (1.0 - p.fade).powf(dt * 60.0)) as f32;
        glColor4f(b[0], b[1], b[2], a);
        glBegin(GL_QUADS);
        for (x, y) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            glVertex2f(x, y);
        }
        glEnd();

        match p.blend {
            flurry::BLEND_NORMAL => glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA),
            flurry::BLEND_SUBTRACT => {
                (self.gl.BlendEquation)(GL_FUNC_SUBTRACT_);
                glBlendFunc(GL_ZERO, GL_ONE_MINUS_SRC_COLOR);
            }
            flurry::BLEND_MULTIPLY => glBlendFunc(GL_ZERO, GL_SRC_COLOR),
            _ => glBlendFunc(GL_SRC_ALPHA, GL_ONE),
        }
        glEnable(GL_TEXTURE_2D);
        glBindTexture(GL_TEXTURE_2D, st.tex);
        glTexEnvi(GL_TEXTURE_ENV_, GL_TEXTURE_ENV_MODE_, GL_MODULATE_);
        let (iw, ih) = (2.0 / self.fw as f32, 2.0 / self.fh as f32);
        glBegin(GL_QUADS);
        for q in &st.sim.quads {
            glColor4f(q.color[0], q.color[1], q.color[2], q.color[3]);
            for k in 0..4 {
                glTexCoord2f(q.u[k], q.v[k]);
                glVertex2f(q.x[k] * iw - 1.0, q.y[k] * ih - 1.0);
            }
        }
        glEnd();
        glDisable(GL_TEXTURE_2D);
        (self.gl.BlendEquation)(GL_FUNC_ADD_);
        glDisable(GL_BLEND);
        glColor4f(1.0, 1.0, 1.0, 1.0);
        self.flurry = Some(st);
    }

    unsafe fn free_multi(&mut self) {
        if let Some(m) = self.multi.take() {
            for t in &m.tex {
                glDeleteTextures(2, t.as_ptr());
            }
            if m.noise != 0 {
                glDeleteTextures(1, &m.noise);
            }
            for p in &m.progs {
                (self.gl.DeleteProgram)(*p);
            }
        }
    }

    unsafe fn bind_inputs(&self, m: &MultiState, inputs: &[bg::Src; 5], prog: u32) {
        for (n, s) in inputs.iter().enumerate() {
            let tex = match s {
                bg::Src::Buf(b, _) => m.tex[*b][m.read[*b]],
                bg::Src::Noise => m.noise,
                bg::Src::None => continue,
            };
            (self.gl.ActiveTexture)(gl::TEXTURE0 + n as u32);
            glBindTexture(GL_TEXTURE_2D, tex);
            (self.gl.Uniform1i)(gl::uloc(&self.gl, prog, &format!("iChannel{n}")), n as i32);
        }
        (self.gl.ActiveTexture)(gl::TEXTURE0);
    }

    /// Advance the simulation (fixed 60 Hz steps) and draw the Image pass into the window-resolution buffer.
    unsafe fn render_multi(&mut self, sim_t: f32, tw: i32, th: i32) {
        // keep the simulation's aspect in step with the window (the settings preview resizes)
        let (cur_sw, cur_sh) = match self.multi.as_ref() {
            Some(m) => (m.sw, m.sh),
            None => return,
        };
        let want_sw = ((cur_sh as f32 * tw as f32 / th.max(1) as f32).round() as i32).max(16);
        if want_sw != cur_sw {
            let idx = self.bg;
            self.set_background(idx, tw, th);
        }
        let Some(mut m) = self.multi.take() else { return };
        let n = m.spec.buffers.len();
        // The simulation steps once per 1/60 s of simulation time, so the speed setting slows (or quickens) the feedback
        // itself, not just the clock the shaders read. A few steps per frame are allowed to keep up with speeds above 1x.
        let st = sim_t as f64;
        let behind = st - m.last_step;
        let (steps, first_t) = if !(0.0..=0.25).contains(&behind) {
            (1, st - SIM_STEP)
        } else {
            (((behind + 0.001) / SIM_STEP).floor().min(4.0) as u32, m.last_step)
        };
        if steps > 0 {
            m.last_step = if steps >= 4 || !(0.0..=0.25).contains(&behind) { st } else { first_t + steps as f64 * SIM_STEP };
            (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.sim_fbo);
          for s in 0..steps {
            let step_t = (first_t + (s + 1) as f64 * SIM_STEP) as f32;
            for k in 0..n {
                let write = 1 - m.read[k];
                (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, m.tex[k][write], 0);
                let prog = m.progs[k];
                (self.gl.UseProgram)(prog);
                glViewport(0, 0, m.bsize[k].0, m.bsize[k].1);
                (self.gl.Uniform2f)(gl::uloc(&self.gl, prog, "wx_res"), m.bsize[k].0 as f32, m.bsize[k].1 as f32);
                (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "wx_aspect"), m.sw as f32 / m.sh as f32);
                (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "iTime"), step_t);
                (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "iTimeDelta"), SIM_STEP as f32);
                self.bind_inputs(&m, &m.spec.buffers[k].inputs, prog);
                Self::quad(-1.0, -1.0, 1.0, 1.0, false);
                m.read[k] = write;
                if m.mip[k] {
                    glBindTexture(GL_TEXTURE_2D, m.tex[k][write]);
                    (self.gl.GenerateMipmap)(GL_TEXTURE_2D);
                }
            }
          }
        }
        // Image pass -> the same offscreen buffer single-pass backgrounds use (then blit + text as usual)
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.fbo);
        glViewport(0, 0, self.fw, self.fh);
        let prog = m.progs[n];
        (self.gl.UseProgram)(prog);
        (self.gl.Uniform2f)(gl::uloc(&self.gl, prog, "wx_res"), self.fw as f32, self.fh as f32);
        (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "wx_aspect"), m.sw as f32 / m.sh as f32);
        (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "iTime"), sim_t);
        (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "iTimeDelta"), SIM_STEP as f32);
        self.bind_inputs(&m, &m.spec.image.inputs, prog);
        Self::quad(-1.0, -1.0, 1.0, 1.0, false);
        self.multi = Some(m);
    }

    /// Geometry backgrounds: clear, then one additively blended quad per object (see bg::is_geometry).
    unsafe fn render_geometry(&mut self, bg_idx: usize, sim_t: f32) {
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.fbo);
        glViewport(0, 0, self.fw, self.fh);
        glClearColor(0.0, 0.0, 0.0, 1.0);
        glClear(GL_COLOR_BUFFER_BIT);
        let aspect = self.fw as f32 / self.fh as f32;
        let data = bg::geometry_data(bg_idx, sim_t, aspect);
        (self.gl.UseProgram)(self.bg_prog);
        glEnable(GL_BLEND);
        glBlendFunc(GL_ONE, GL_ONE);
        for o in data.chunks_exact(8) {
            let (px, py, rad) = (o[0], o[1], o[2]);
            // skip bubbles that are completely off screen (the scene spans x in -aspect..aspect, y in -1..1)
            if px + rad < -aspect || px - rad > aspect || py + rad < -1.0 || py - rad > 1.0 {
                continue;
            }
            (self.gl.Uniform4f)(self.loc.bub, px, py, rad, 0.0);
            (self.gl.Uniform3f)(self.loc.bcol, o[4], o[5], o[6]);
            glBegin(GL_QUADS);
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                glTexCoord2f(sx * rad, sy * rad);
                glVertex2f((px + sx * rad) / aspect, py + sy * rad);
            }
            glEnd();
        }
        glDisable(GL_BLEND);
    }

    /// Draw one frame. `target` = (fbo, w, h) for offscreen rendering, None for the window.
    pub unsafe fn render(&mut self, f: &Frame, target: Option<(u32, i32, i32)>) {
        wglMakeCurrent(self.hdc, self.rc);
        let (tw, th) = target.map_or((self.w, self.h), |t| (t.1, t.2));
        // the background is rendered at a fraction of the output size when asked (scaled up by the blit; text stays sharp)
        let sc = f.cfg.render_scale.clamp(25, 100) as i32;
        self.ensure_fbo(tw * sc / 100, th * sc / 100);
        if f.bg != self.bg {
            self.set_background(f.bg, tw, th);
        }
        glDisable(GL_BLEND);
        // 1. background -> offscreen
        // simulation time = wall time scaled by the user's speed setting; accumulated so moving the slider never makes it jump
        let speed = f.cfg.speed.clamp(10, 200) as f64 / 100.0;
        let dt = f.t - self.sim_prev;
        self.sim_clock = if self.sim_prev.is_finite() && (0.0..=0.25).contains(&dt) { self.sim_clock + dt * speed } else { f.t * speed };
        self.sim_prev = f.t;
        let sim_t = (self.sim_clock as f32) * bg::TIME_SCALE[f.bg.min(bg::NAMES.len() - 1)];
        if self.flurry.is_some() {
            self.render_flurry(f, speed);
        } else if bg::is_geometry(f.bg) && self.multi.is_none() {
            self.render_geometry(f.bg, sim_t);
        } else if self.multi.is_some() {
            self.render_multi(sim_t, tw, th);
        } else {
            (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.fbo);
            glViewport(0, 0, self.fw, self.fh);
            (self.gl.UseProgram)(self.bg_prog);
            (self.gl.Uniform2f)(self.loc.bg_res, self.fw as f32, self.fh as f32);
            (self.gl.Uniform1f)(self.loc.bg_time, sim_t);
            Self::quad(-1.0, -1.0, 1.0, 1.0, false);
        }
        // 2. blit to target (or to the scene buffer first, while a transition is running)
        let target_fbo = target.map_or(0, |t| t.0);
        // fades: a layered window is faded by the compositor (the desktop shows through); blur and zoom still need our shader
        if self.layered {
            let alpha = (ease(f.vis) * 255.0 + 0.5) as i32;
            if alpha != self.alpha {
                self.alpha = alpha;
                SetLayeredWindowAttributes(self.hwnd, 0, alpha as u8, LWA_ALPHA);
            }
        }
        let trans = f.vis < 0.999 && (f.tstyle >= 2 || (f.tstyle == 1 && !self.layered));
        let dst = if trans {
            self.ensure_scene(tw, th);
            self.sc_fbo
        } else {
            target_fbo
        };
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, dst);
        glViewport(0, 0, tw, th);
        (self.gl.UseProgram)(self.blit_prog);
        (self.gl.ActiveTexture)(gl::TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, self.fbo_tex);
        (self.gl.Uniform1i)(self.loc.blit_src, 0);
        (self.gl.Uniform1f)(self.loc.blit_bright, f.cfg.brightness as f32 / 100.0);
        (self.gl.Uniform1f)(self.loc.blit_t, f.t as f32);
        Self::quad(-1.0, -1.0, 1.0, 1.0, false);
        // 3. text
        if f.show_text {
            self.draw_text(f, tw, th);
        }
        // 4. transition: the finished frame goes through the transition shader to the real target
        if trans {
            if f.tstyle == 2 {
                // blur: shrink the scene to a quarter of its size first; the blur samples that small copy
                self.ensure_blur(tw, th);
                (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.bl_fbo[0]);
                glViewport(0, 0, self.bl_w, self.bl_h);
                (self.gl.UseProgram)(self.down_prog);
                (self.gl.ActiveTexture)(gl::TEXTURE0);
                glBindTexture(GL_TEXTURE_2D, self.sc_tex);
                (self.gl.Uniform1i)(self.down_src, 0);
                (self.gl.Uniform2f)(self.down_texel, 1.0 / tw as f32, 1.0 / th as f32);
                Self::quad(-1.0, -1.0, 1.0, 1.0, false);
                // ... then blur the small copy (radius in screen heights, shrinking to zero as the picture appears)
                (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.bl_fbo[1]);
                (self.gl.UseProgram)(self.blur_prog);
                glBindTexture(GL_TEXTURE_2D, self.bl_tex[0]);
                (self.gl.Uniform1i)(self.blur_src, 0);
                (self.gl.Uniform1f)(self.blur_r, (1.0 - ease(f.vis)) * 0.07);
                (self.gl.Uniform1f)(self.blur_ia, th as f32 / tw as f32);
                Self::quad(-1.0, -1.0, 1.0, 1.0, false);
            }
            (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, target_fbo);
            glViewport(0, 0, tw, th);
            (self.gl.UseProgram)(self.trans_prog);
            (self.gl.ActiveTexture)(gl::TEXTURE0 + 1);
            glBindTexture(GL_TEXTURE_2D, if f.tstyle == 2 { self.bl_tex[1] } else { self.sc_tex });
            (self.gl.Uniform1i)(self.loc.tr_small, 1);
            (self.gl.ActiveTexture)(gl::TEXTURE0 + 2);
            glBindTexture(GL_TEXTURE_2D, self.out_tex);
            (self.gl.Uniform1i)(self.loc.tr_outside, 2);
            (self.gl.Uniform1f)(self.loc.tr_have, if self.out_have && self.out_use { 1.0 } else { 0.0 });
            (self.gl.ActiveTexture)(gl::TEXTURE0 + 1);
            (self.gl.ActiveTexture)(gl::TEXTURE0);
            glBindTexture(GL_TEXTURE_2D, self.sc_tex);
            (self.gl.Uniform1i)(self.loc.tr_scene, 0);
            (self.gl.Uniform1f)(self.loc.tr_dark, if self.layered { 0.0 } else { 1.0 });
            (self.gl.Uniform1f)(self.loc.tr_p, f.vis.max(0.0));
            (self.gl.Uniform1i)(self.loc.tr_style, f.tstyle as i32);
            (self.gl.Uniform2f)(self.loc.tr_res, tw as f32, th as f32);
            Self::quad(-1.0, -1.0, 1.0, 1.0, false);
        }
        if target.is_none() {
            SwapBuffers(self.hdc);
        }
    }

    unsafe fn draw_text(&mut self, f: &Frame, tw: i32, th: i32) {
        let hh = th as f32;
        let (em1, emc, em2, em3, em4) = ((0.19 * hh) as i32, (0.034 * hh) as i32, (0.085 * hh) as i32, (0.25 * hh) as i32, (0.036 * hh) as i32);
        let pad = (0.035 * hh).ceil() as i32;
        // the dew point is the secondary reading: noticeably smaller than the feels-like temperature
        let em_dew = (0.72 * em1 as f32) as i32;
        let cfg = f.cfg;
        let style = cfg.contrast;
        let st = &f.now;

        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        let d = st.wDay as u32;
        let suffix = if (11..=13).contains(&(d % 100)) { "th" } else { match d % 10 { 1 => "st", 2 => "nd", 3 => "rd", _ => "th" } };
        let date = format!("{} {}{} {}", MONTHS[(st.wMonth as usize).clamp(1, 12) - 1], d, suffix, st.wYear);
        let hour = if cfg.clock24 { st.wHour as u32 } else { (st.wHour as u32 + 11) % 12 + 1 };
        let time = if cfg.seconds { format!("{}:{:02}:{:02}", hour, st.wMinute, st.wSecond) } else { format!("{}:{:02}", hour, st.wMinute) };
        // what the clock will read a second (or, without seconds, a minute) from now, for the background worker
        let next_time = {
            let step = if cfg.seconds { 1 } else { 60 };
            let tot = (st.wHour as u32 * 3600 + st.wMinute as u32 * 60 + st.wSecond as u32 + step) % 86400;
            let (h, m, s) = (tot / 3600, tot / 60 % 60, tot % 60);
            let h = if cfg.clock24 { h } else { (h + 11) % 12 + 1 };
            if cfg.seconds { format!("{}:{:02}:{:02}", h, m, s) } else { format!("{}:{:02}", h, m) }
        };

        // rows: weather (day / feels / dew + captions), date, time, details
        let temp_s = |v: f64| format!("{}\u{b0}", (if cfg.fahrenheit { v } else { (v - 32.0) * 5.0 / 9.0 }).round() as i64);
        let has_wx = f.wx.is_some();
        if let Some(w) = f.wx {
            self.upload(0, DAYS[(st.wDayOfWeek as usize) % 7], em1, pad, th, style);
            self.upload(1, &temp_s(w.feels), em1, pad, th, style);
            self.upload(2, &temp_s(w.dew), em_dew, pad, th, style);
            self.upload(3, "feels like", emc, pad, th, style);
            self.upload(4, "dew point", emc, pad, th, style);
        }
        self.upload(5, &date, em2, pad, th, style);
        self.upload(6, &time, em3, pad, th, style);
        self.prefetch(&next_time, em3, pad, th, style);
        let details = if cfg.details { f.wx.map(|w| details_line(w, cfg)).unwrap_or_default() } else { String::new() };
        let has_details = !details.is_empty();
        if has_details {
            self.upload(7, &details, em4, pad, th, style);
        }

        // layout in pixels, origin at the block's centre; y grows downward
        let cw = |r: &Run| (r.w - 2 * r.pad) as f32;
        let chh = |r: &Run| (r.h - 2 * r.pad) as f32;
        let r = self.runs;
        let mut items: Vec<(usize, f32, f32)> = Vec::new(); // slot, x (left), y (top)
        let mut y = 0.0f32;
        if has_wx {
            let gap = em1 as f32 * 0.55;
            // each reading sits in a column as wide as the wider of the number and its caption
            let (wt, ww) = (cw(&r[1]).max(cw(&r[3])), cw(&r[2]).max(cw(&r[4])));
            let total = cw(&r[0]) + gap + wt + gap + ww;
            let x0 = -total / 2.0;
            let (xd, xt, xw) = (x0, x0 + cw(&r[0]) + gap, x0 + cw(&r[0]) + gap + wt + gap);
            items.push((0, xd, y));
            items.push((1, xt + (wt - cw(&r[1])) / 2.0, y));
            // the smaller dew point shares the baseline (bottom edge) of the feels-like number
            items.push((2, xw + (ww - cw(&r[2])) / 2.0, y + chh(&r[1]) - chh(&r[2])));
            let cy = y + chh(&r[1]) * 0.80;
            items.push((3, xt + (wt - cw(&r[3])) / 2.0, cy));
            items.push((4, xw + (ww - cw(&r[4])) / 2.0, cy));
            y = cy + chh(&r[3]) * 0.95 + em2 as f32 * 0.10;
        }
        items.push((5, -cw(&r[5]) / 2.0, y));
        y += chh(&r[5]) * 0.92;
        items.push((6, -cw(&r[6]) / 2.0, y));
        y += chh(&r[6]) * 0.90;
        if has_details {
            items.push((7, -cw(&r[7]) / 2.0, y));
            y += chh(&r[7]);
        }
        let block_h = y;

        // burn-in protection: slow breathing + wandering (same idea as dash2's 12 s scale animation)
        let m = cfg.movement as f64 / 100.0;
        let t = f.t;
        let s = (0.90 - 0.14 * m * (0.5 - 0.5 * (std::f64::consts::TAU * t / 24.0).cos())) as f32;
        let dx = (m * 0.045 * tw as f64 * (std::f64::consts::TAU * t / 97.0).sin()) as f32;
        let dy = (m * 0.035 * th as f64 * (std::f64::consts::TAU * t / 71.0 + 1.3).sin()) as f32;
        let (cx, cy0) = (tw as f32 / 2.0 + dx, th as f32 / 2.0 + dy);

        glEnable(GL_BLEND);
        glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
        (self.gl.UseProgram)(self.text_prog);
        (self.gl.ActiveTexture)(gl::TEXTURE0);
        (self.gl.Uniform1i)(self.loc.text_tex, 0);
        (self.gl.Uniform1i)(self.loc.text_sh, 1);
        (self.gl.Uniform1i)(self.loc.text_mode, style as i32);
        (self.gl.Uniform1f)(self.loc.text_alpha, 1.0);
        let sh_px = 0.009 * hh;
        for pass in 0..2 {
        (self.gl.Uniform1i)(self.loc.text_pass, pass);
        for &(slot, x, ly) in &items {
            let run = self.runs[slot];
            let (sx, sy) = (cx + x * s - run.pad as f32 * s, cy0 + (ly - block_h / 2.0) * s - run.pad as f32 * s);
            let (qw, qh) = (run.w as f32 * s, run.h as f32 * s);
            (self.gl.ActiveTexture)(gl::TEXTURE0 + 1);
            glBindTexture(GL_TEXTURE_2D, run.sh_tex);
            (self.gl.ActiveTexture)(gl::TEXTURE0);
            glBindTexture(GL_TEXTURE_2D, run.tex);
            (self.gl.Uniform2f)(self.loc.text_shoff, sh_px / run.w as f32, sh_px / run.h as f32);
            let ndc = |px: f32, size: f32| px / size * 2.0 - 1.0;
            Self::quad(ndc(sx, tw as f32), -ndc(sy + qh, th as f32), ndc(sx + qw, tw as f32), -ndc(sy, th as f32), true);
        }
        }
        glDisable(GL_BLEND);
    }

    /// Developer benchmark: average milliseconds per frame for `frames` frames of background `bg` rendered offscreen
    /// at w x h (GPU-synchronised with glFinish, no vsync involved).
    pub unsafe fn bench(&mut self, cfg: &Config, wx: &Weather, bg: usize, w: i32, h: i32, frames: u32) -> f64 {
        wglMakeCurrent(self.hdc, self.rc);
        let (mut tex, mut fbo) = (0u32, 0u32);
        glGenTextures(1, &mut tex);
        glBindTexture(GL_TEXTURE_2D, tex);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA as i32, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
        (self.gl.GenFramebuffers)(1, &mut fbo);
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, fbo);
        (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
        let mut st: SYSTEMTIME = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut st);
        // dev: WXSCR_BENCH_TRANS=<style>:<visibility> benchmarks a frame in the middle of a transition
        let (bvis, bstyle) = match std::env::var("WXSCR_BENCH_TRANS") {
            Ok(v) => {
                let p: Vec<&str> = v.split(':').collect();
                (p.get(1).and_then(|x| x.parse().ok()).unwrap_or(1.0f32), p.first().and_then(|x| x.parse().ok()).unwrap_or(0usize))
            }
            Err(_) => (1.0, 0),
        };
        let frame = |t: f64| Frame { t, cfg, wx: Some(wx), show_text: true, bg, now: st, vis: bvis, tstyle: bstyle };
        for k in 0..20 {
            self.render(&frame(100.0 + k as f64 / 60.0), Some((fbo, w, h)));
        }
        glFinish();
        let mut best = f64::MAX;
        for round in 0..3 {
            let t0 = std::time::Instant::now();
            for k in 0..frames {
                self.render(&frame(101.0 + (round * frames + k) as f64 / 60.0), Some((fbo, w, h)));
                glFinish();
            }
            best = best.min(t0.elapsed().as_secs_f64() * 1000.0 / frames as f64);
        }
        best
    }

    /// Render one frame offscreen and return top-down RGB (developer /snap command).
    pub unsafe fn snapshot(&mut self, f: &Frame, w: i32, h: i32) -> Vec<u8> {
        wglMakeCurrent(self.hdc, self.rc);
        let (mut tex, mut fbo) = (0u32, 0u32);
        glGenTextures(1, &mut tex);
        glBindTexture(GL_TEXTURE_2D, tex);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA as i32, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
        (self.gl.GenFramebuffers)(1, &mut fbo);
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, fbo);
        (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
        if bg::is_multi(f.bg) || bg::is_flurry(f.bg) {
            // feedback simulations need history: run ~4 s of steps first
            for k in 0..240 {
                let warm = Frame { t: f.t - (240 - k) as f64 / 60.0, cfg: f.cfg, wx: f.wx, show_text: false, bg: f.bg, now: f.now, vis: 1.0, tstyle: 0 };
                self.render(&warm, Some((fbo, w, h)));
            }
        }
        self.render(f, Some((fbo, w, h)));
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, fbo);
        glPixelStorei(GL_PACK_ALIGNMENT, 1);
        let mut buf = vec![0u8; (w * h * 3) as usize];
        glReadPixels(0, 0, w, h, GL_RGB, GL_UNSIGNED_BYTE, buf.as_mut_ptr() as *mut _);
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, 0);
        let row = (w * 3) as usize;
        let mut out = Vec::with_capacity(buf.len());
        for y in (0..h as usize).rev() {
            out.extend_from_slice(&buf[y * row..(y + 1) * row]);
        }
        out
    }
}

fn details_line(w: &Weather, cfg: &Config) -> String {
    let t = |v: f64| format!("{}\u{b0}", (if cfg.fahrenheit { v } else { (v - 32.0) * 5.0 / 9.0 }).round() as i64);
    let mut parts: Vec<String> = Vec::new();
    if let (Some(hi), Some(lo)) = (w.hi, w.lo) {
        parts.push(format!("high {}  low {}", t(hi), t(lo)));
    }
    if let Some(p) = w.pop {
        parts.push(format!("rain {}%", p.round() as i64));
    }
    if let Some(v) = w.wind {
        parts.push(if cfg.fahrenheit { format!("wind {} mph", v.round() as i64) } else { format!("wind {} km/h", (v * 1.609344).round() as i64) });
    }
    // next sun event, shown in the forecast location's own clock
    let now = crate::weather::now_unix();
    let next = w.sunrise.iter().map(|&x| (x, "sunrise")).chain(w.sunset.iter().map(|&x| (x, "sunset"))).filter(|p| p.0 > now).min_by_key(|p| p.0);
    if let Some((ts, label)) = next {
        let secs = (ts + w.utc_offset).rem_euclid(86400);
        let (h, m) = ((secs / 3600) as u32, (secs % 3600 / 60) as u32);
        let clock = if cfg.clock24 { format!("{h}:{m:02}") } else { format!("{}:{:02} {}", (h + 11) % 12 + 1, m, if h < 12 { "AM" } else { "PM" }) };
        parts.push(format!("{label} {clock}"));
    }
    parts.join("   \u{b7}   ")
}

/// Outline alpha for a glyph mask: 1 inside `radius` pixels of any glyph pixel (distance measured with an exact
/// Euclidean distance transform), fading over the last pixel so the outer edge is anti-aliased.
fn outline_mask(src: &[u8], w: usize, h: usize, radius: f32) -> Vec<u8> {
    const INF: f32 = 1.0e12;
    // squared distance to the nearest glyph pixel (Felzenszwalb & Huttenlocher), columns then rows
    let mut d: Vec<f32> = src.iter().map(|&v| if v >= 128 { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let (mut f, mut out, mut z) = (vec![0f32; n], vec![0f32; n], vec![0f32; n + 1]);
    let mut v = vec![0usize; n];
    let mut pass = |len: usize, f: &[f32], out: &mut [f32]| {
        let mut k = 0usize;
        v[0] = 0;
        z[0] = -INF;
        z[1] = INF;
        for q in 1..len {
            let mut s;
            loop {
                let p = v[k];
                s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
                if k > 0 && s <= z[k] {
                    k -= 1;
                } else {
                    break;
                }
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = INF;
        }
        k = 0;
        for q in 0..len {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let dq = q as f32 - v[k] as f32;
            out[q] = dq * dq + f[v[k]];
        }
    };
    for x in 0..w {
        for y in 0..h {
            f[y] = d[y * w + x];
        }
        pass(h, &f, &mut out);
        for y in 0..h {
            d[y * w + x] = out[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&d[y * w..(y + 1) * w]);
        pass(w, &f, &mut out);
        d[y * w..(y + 1) * w].copy_from_slice(&out[..w]);
    }
    d.iter().map(|&sq| ((radius - sq.sqrt() + 0.5).clamp(0.0, 1.0) * 255.0) as u8).collect()
}

/// Box-downsample a mask by `f`, then blur it with three box passes of `radius`. Returns (data, w, h).
fn blur_lowres(src: &[u8], w: usize, h: usize, f: usize, radius: usize) -> (Vec<u8>, usize, usize) {
    let (sw, sh) = ((w + f - 1) / f, (h + f - 1) / f);
    let mut a = vec![0f32; sw * sh];
    for y in 0..h {
        for x in 0..w {
            a[(y / f) * sw + x / f] += src[y * w + x] as f32;
        }
    }
    let norm = 1.0 / (f * f) as f32 / 255.0;
    a.iter_mut().for_each(|v| *v *= norm);
    let mut b = vec![0f32; sw * sh];
    let win = (2 * radius + 1) as f32;
    for _ in 0..3 {
        // horizontal (edges read as 0, so the shadow fades out inside the padding)
        for y in 0..sh {
            let row = &a[y * sw..(y + 1) * sw];
            let mut sum: f32 = row.iter().take(radius + 1).sum();
            for x in 0..sw {
                b[y * sw + x] = sum / win;
                if x + radius + 1 < sw {
                    sum += row[x + radius + 1];
                }
                if x >= radius {
                    sum -= row[x - radius];
                }
            }
        }
        // vertical
        for x in 0..sw {
            let mut sum: f32 = (0..=radius.min(sh - 1)).map(|y| b[y * sw + x]).sum();
            for y in 0..sh {
                a[y * sw + x] = sum / win;
                if y + radius + 1 < sh {
                    sum += b[(y + radius + 1) * sw + x];
                }
                if y >= radius {
                    sum -= b[(y - radius) * sw + x];
                }
            }
        }
    }
    (a.iter().map(|v| (v * 255.0).clamp(0.0, 255.0) as u8).collect(), sw, sh)
}

/// Tileable colour value-noise (RGB8, NOISE_N squared): stands in for the hosted textures some Shadertoy shaders sample.
fn noise_texture() -> Vec<u8> {
    fn hash(x: u32, y: u32, c: u32, o: u32) -> f32 {
        let mut h = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ c.wrapping_mul(2147483629) ^ o.wrapping_mul(1274126177);
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        h ^= h >> 16;
        (h & 0xFFFF) as f32 / 65535.0
    }
    let n = NOISE_N;
    let mut acc = vec![0f32; n * n * 3];
    let mut norm = 0.0;
    for oct in 0..5u32 {
        let period = 4u32 << oct;
        let amp = 0.55f32.powi(oct as i32);
        norm += amp;
        for c in 0..3u32 {
            for y in 0..n {
                for x in 0..n {
                    let (fx, fy) = (x as f32 * period as f32 / n as f32, y as f32 * period as f32 / n as f32);
                    let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
                    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
                    let v = |dx: u32, dy: u32| hash((x0 + dx) % period, (y0 + dy) % period, c, oct);
                    let top = v(0, 0) + (v(1, 0) - v(0, 0)) * sx;
                    let bot = v(0, 1) + (v(1, 1) - v(0, 1)) * sx;
                    acc[(y * n + x) * 3 + c as usize] += amp * (top + (bot - top) * sy);
                }
            }
        }
    }
    acc.iter().map(|v| ((((v / norm) - 0.5) * 1.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8).collect()
}
