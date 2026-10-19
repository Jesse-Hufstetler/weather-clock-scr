//! One OpenGL context per window. Pipeline per frame:
//!   1. background shader -> half-float offscreen buffer at native resolution
//!   2. blit to the window with a touch of noise dither (kills banding) and a brightness control
//!   3. text runs (GDI-rasterised masks) with a soft shadow, scaled/drifted for burn-in protection

use crate::bg;
use crate::config::Config;
use crate::gl::{self, Gl};
use crate::text::Rasterizer;
use crate::weather::Weather;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use windows_sys::Win32::Foundation::{HWND, SYSTEMTIME};
use windows_sys::Win32::Graphics::Gdi::{GetDC, HDC};
use windows_sys::Win32::Graphics::OpenGL::*;

const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_LUMINANCE: u32 = 0x1909;
const GL_RGBA32F: i32 = 0x8814;
const GL_FLOAT_: u32 = 0x1406;
const GL_REPEAT: u32 = 0x2901;
const NOISE_N: usize = 256;
/// Multipass simulations advance at this fixed rate, whatever the display's refresh rate is.
const SIM_STEP: f64 = 1.0 / 60.0;

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
    tr_outside: i32,
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
    last_step: f64,
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
    sim_fbo: u32,
    trans_prog: u32,
    /// Offscreen copy of the finished frame, only used while a transition is running.
    sc_fbo: u32,
    sc_tex: u32,
    sc_w: i32,
    sc_h: i32,
    /// The screen as it was before the screensaver appeared (or is behind it when it leaves); 1x1 black until set.
    out_tex: u32,
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

/// Final pass while the screensaver is appearing or disappearing. `p` is visibility (0 black .. 1 normal).
const TRANS_FS: &str = "#version 120
uniform sampler2D scene; uniform sampler2D outside; uniform float p; uniform int style; uniform vec2 res; varying vec2 uv;
float ease(float x){ x = clamp(x, 0.0, 1.0); return x*x*x*(x*(x*6.0-15.0)+10.0); }
// B = the screensaver frame, A = what was on screen before it started / is behind it when it ends (black if unknown)
vec3 B(vec2 c){ return texture2D(scene, clamp(c, 0.0, 1.0)).rgb; }
vec3 A(vec2 c){ return texture2D(outside, clamp(c, 0.0, 1.0)).rgb; }
void main(){
  float e = ease(p);
  float asp = res.x / res.y;
  vec3 c;
  if (style == 2) {
    // cross blur: both pictures defocus as they swap, the screensaver sharpening out of the haze
    float r = sin(3.14159265 * e) * 0.06;
    vec3 a = vec3(0.0);
    vec3 b = vec3(0.0);
    for (int i = 0; i < 24; i++) {
      float ang = float(i) * 2.39996;
      float rr = sqrt((float(i) + 0.5) / 24.0);
      vec2 o = vec2(cos(ang) / asp, sin(ang)) * rr * r;
      a += A(uv + o);
      b += B(uv + o);
    }
    c = mix(a, b, smoothstep(0.15, 0.85, e)) / 24.0;
  } else if (style == 3) {
    // zoom: the screensaver pulls back from a close-up as it fades over the old screen (and dives in as it leaves)
    float s = mix(0.62, 1.0, e);
    c = mix(A(uv), B((uv - 0.5) * s + 0.5), e);
  } else {
    c = mix(A(uv), B(uv), e);
  }
  gl_FragColor = vec4(c, 1.0);
}";

impl Renderer {
    pub unsafe fn new(hwnd: HWND, w: i32, h: i32, cfg: &Config, bg_idx: usize) -> Result<Renderer, String> {
        let hdc = GetDC(hwnd);
        let mut pfd: PIXELFORMATDESCRIPTOR = std::mem::zeroed();
        pfd.nSize = std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16;
        pfd.nVersion = 1;
        pfd.dwFlags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER;
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
        let bg_prog = gl::program(&g, &bg::source(bg_idx))?;
        let loc = Locs {
            bg_res: gl::uloc(&g, bg_prog, "wx_res"),
            bg_time: gl::uloc(&g, bg_prog, "iTime"),
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
            tr_outside: gl::uloc(&g, trans_prog, "outside"),
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
            bg: if bg::is_multi(bg_idx) { usize::MAX } else { bg_idx },
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
            sim_fbo: 0,
            trans_prog,
            sc_fbo: 0,
            sc_tex: 0,
            sc_w: 0,
            sc_h: 0,
            out_tex: 0,
        };
        r.set_outside(1, 1, &[0, 0, 0, 255]);
        r.ensure_fbo(w, h);
        Ok(r)
    }

    /// Give the transitions something to blend with: a BGRA, bottom-up screenshot of the screen without the saver.
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
    }

    /// Swap the text font (settings preview); cached glyph runs are rebuilt on the next frame.
    pub fn set_font(&mut self, face: &str) {
        self.rast = Rasterizer::new(face);
        self.face = face.to_string();
        self.keys = Default::default();
        self.ready.clear();
        self.pending.clear();
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
                }
            }
            self.bg = idx;
            return;
        }
        self.bg_prog = gl::program(&self.gl, &bg::source(idx)).unwrap_or(self.bg_prog);
        self.loc.bg_res = gl::uloc(&self.gl, self.bg_prog, "wx_res");
        self.loc.bg_time = gl::uloc(&self.gl, self.bg_prog, "iTime");
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
                glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA32F, sw, sh, 0, GL_RGBA, GL_FLOAT_, std::ptr::null());
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, if mip[k] { gl::LINEAR_MIPMAP_LINEAR } else { GL_LINEAR as i32 });
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
                glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
                // start from black
                (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex[k][j], 0);
                glViewport(0, 0, sw, sh);
                glClearColor(0.0, 0.0, 0.0, 0.0);
                glClear(GL_COLOR_BUFFER_BIT);
                if mip[k] {
                    (self.gl.GenerateMipmap)(GL_TEXTURE_2D);
                }
            }
        }
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, 0);
        Ok(MultiState { spec, progs, tex, read: vec![0; n], mip, noise, sw, sh, last_step: f64::NEG_INFINITY })
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

    unsafe fn bind_inputs(&self, m: &MultiState, inputs: &[bg::Src; 4], prog: u32) {
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
    unsafe fn render_multi(&mut self, t: f64, sim_t: f32, tw: i32, th: i32) {
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
        if t - m.last_step >= SIM_STEP - 0.001 || t < m.last_step {
            m.last_step = t;
            (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.sim_fbo);
            glViewport(0, 0, m.sw, m.sh);
            for k in 0..n {
                let write = 1 - m.read[k];
                (self.gl.FramebufferTexture2D)(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, GL_TEXTURE_2D, m.tex[k][write], 0);
                let prog = m.progs[k];
                (self.gl.UseProgram)(prog);
                (self.gl.Uniform2f)(gl::uloc(&self.gl, prog, "wx_res"), m.sw as f32, m.sh as f32);
                (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "iTime"), sim_t);
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
        // Image pass -> the same offscreen buffer single-pass backgrounds use (then blit + text as usual)
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.fbo);
        glViewport(0, 0, self.fw, self.fh);
        let prog = m.progs[n];
        (self.gl.UseProgram)(prog);
        (self.gl.Uniform2f)(gl::uloc(&self.gl, prog, "wx_res"), self.fw as f32, self.fh as f32);
        (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "iTime"), sim_t);
        (self.gl.Uniform1f)(gl::uloc(&self.gl, prog, "iTimeDelta"), SIM_STEP as f32);
        self.bind_inputs(&m, &m.spec.image.inputs, prog);
        Self::quad(-1.0, -1.0, 1.0, 1.0, false);
        self.multi = Some(m);
    }

    /// Draw one frame. `target` = (fbo, w, h) for offscreen rendering, None for the window.
    pub unsafe fn render(&mut self, f: &Frame, target: Option<(u32, i32, i32)>) {
        wglMakeCurrent(self.hdc, self.rc);
        let (tw, th) = target.map_or((self.w, self.h), |t| (t.1, t.2));
        self.ensure_fbo(tw, th);
        if f.bg != self.bg {
            self.set_background(f.bg, tw, th);
        }
        glDisable(GL_BLEND);
        // 1. background -> offscreen
        let sim_t = (f.t as f32) * bg::TIME_SCALE[f.bg.min(bg::NAMES.len() - 1)];
        if self.multi.is_some() {
            self.render_multi(f.t, sim_t, tw, th);
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
        let trans = f.vis < 0.999 && f.tstyle != 0;
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
            (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, target_fbo);
            glViewport(0, 0, tw, th);
            (self.gl.UseProgram)(self.trans_prog);
            (self.gl.ActiveTexture)(gl::TEXTURE0);
            glBindTexture(GL_TEXTURE_2D, self.sc_tex);
            (self.gl.Uniform1i)(self.loc.tr_scene, 0);
            (self.gl.ActiveTexture)(gl::TEXTURE0 + 1);
            glBindTexture(GL_TEXTURE_2D, self.out_tex);
            (self.gl.ActiveTexture)(gl::TEXTURE0);
            (self.gl.Uniform1i)(self.loc.tr_outside, 1);
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
        if bg::is_multi(f.bg) {
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
