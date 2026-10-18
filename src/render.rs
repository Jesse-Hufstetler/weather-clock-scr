//! One OpenGL context per window. Pipeline per frame:
//!   1. background shader -> half-float offscreen buffer (at a reduced scale: cheap on the GPU)
//!   2. blit to the window with a touch of noise dither (kills banding) and a brightness control
//!   3. text runs (GDI-rasterised masks) with a soft shadow, scaled/drifted for burn-in protection

use crate::bg;
use crate::config::Config;
use crate::gl::{self, Gl};
use crate::text::Rasterizer;
use crate::weather::Weather;
use windows_sys::Win32::Foundation::{HWND, SYSTEMTIME};
use windows_sys::Win32::Graphics::Gdi::{GetDC, HDC};
use windows_sys::Win32::Graphics::OpenGL::*;

const QUALITY_SCALE: [f32; 4] = [0.35, 0.5, 0.75, 1.0];
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_LUMINANCE: u32 = 0x1909;

pub struct Frame<'a> {
    pub t: f64,
    pub cfg: &'a Config,
    pub wx: Option<&'a Weather>,
    pub show_text: bool,
    pub bg: usize,
    pub now: SYSTEMTIME,
}

#[derive(Default, Clone, Copy)]
struct Run {
    tex: u32,
    w: i32,
    h: i32,
    pad: i32,
    /// Pre-blurred, low-resolution copy of the mask used for the drop shadow (bilinear-upscaled = smooth).
    sh_tex: u32,
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
}

pub struct Renderer {
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
    scale_idx: usize,
    rast: Rasterizer,
    runs: [Run; 8],
    keys: [String; 8],
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

const TEXT_FS: &str = "#version 120
uniform sampler2D tex; uniform sampler2D sh; uniform float alpha; uniform vec2 shoff; varying vec2 uv;
void main(){
  float m = texture2D(tex, uv).r;
  float s = texture2D(sh, uv - shoff).r + texture2D(sh, uv + shoff).r;
  s = clamp(s * 1.25, 0.0, 1.0) * 0.9;
  float a = m + (1.0 - m) * s;
  gl_FragColor = vec4(vec3(m), a) * alpha;
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
        if let Some(p) = wglGetProcAddress(b"wglSwapIntervalEXT\0".as_ptr()) {
            let f: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(p);
            f(1);
        }
        let blit_prog = gl::program(&g, BLIT_FS)?;
        let text_prog = gl::program(&g, TEXT_FS)?;
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
        };
        let mut r = Renderer {
            hdc,
            rc,
            gl: g,
            w,
            h,
            bg: bg_idx,
            bg_prog,
            blit_prog,
            text_prog,
            loc,
            fbo: 0,
            fbo_tex: 0,
            fw: 0,
            fh: 0,
            scale_idx: usize::MAX,
            rast: Rasterizer::new(&cfg.font),
            runs: [Run::default(); 8],
            keys: Default::default(),
        };
        r.ensure_fbo(w, h, cfg.quality);
        Ok(r)
    }

    unsafe fn ensure_fbo(&mut self, tw: i32, th: i32, quality: usize) {
        let (fw, fh) = (((tw as f32 * QUALITY_SCALE[quality]) as i32).max(16), ((th as f32 * QUALITY_SCALE[quality]) as i32).max(16));
        if self.fbo != 0 && fw == self.fw && fh == self.fh && quality == self.scale_idx {
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
        self.scale_idx = quality;
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

    unsafe fn upload(&mut self, slot: usize, text: &str, px: i32, pad: i32, th: i32) {
        let key = format!("{px}|{pad}|{text}");
        if self.keys[slot] == key {
            return;
        }
        let r = self.rast.raster(text, px, pad);
        // shadow: shrink the mask, box-blur it three times (~Gaussian), keep it small and bilinear-filtered
        let f = (th / 270).max(2);
        let radius = ((0.0075 * th as f32) / f as f32).round().max(1.0) as usize;
        let (sdata, sw, sh) = blur_lowres(&r.data, r.w as usize, r.h as usize, f as usize, radius);
        let run = &mut self.runs[slot];
        if run.tex == 0 {
            glGenTextures(1, &mut run.tex);
            glGenTextures(1, &mut run.sh_tex);
        }
        glBindTexture(GL_TEXTURE_2D, run.sh_tex);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glTexImage2D(GL_TEXTURE_2D, 0, gl::LUMINANCE8 as i32, sw as i32, sh as i32, 0, GL_LUMINANCE, GL_UNSIGNED_BYTE, sdata.as_ptr() as *const _);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        glBindTexture(GL_TEXTURE_2D, run.tex);
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glTexImage2D(GL_TEXTURE_2D, 0, gl::LUMINANCE8 as i32, r.w, r.h, 0, GL_LUMINANCE, GL_UNSIGNED_BYTE, r.data.as_ptr() as *const _);
        (self.gl.GenerateMipmap)(GL_TEXTURE_2D);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        *run = Run { tex: run.tex, w: r.w, h: r.h, pad: r.pad, sh_tex: run.sh_tex };
        self.keys[slot] = key;
    }

    /// Draw one frame. `target` = (fbo, w, h) for offscreen rendering, None for the window.
    pub unsafe fn render(&mut self, f: &Frame, target: Option<(u32, i32, i32)>) {
        wglMakeCurrent(self.hdc, self.rc);
        let (tw, th) = target.map_or((self.w, self.h), |t| (t.1, t.2));
        self.ensure_fbo(tw, th, f.cfg.quality);
        if f.bg != self.bg {
            self.bg_prog = gl::program(&self.gl, &bg::source(f.bg)).unwrap_or(self.bg_prog);
            self.loc.bg_res = gl::uloc(&self.gl, self.bg_prog, "wx_res");
            self.loc.bg_time = gl::uloc(&self.gl, self.bg_prog, "iTime");
            self.bg = f.bg;
        }
        glDisable(GL_BLEND);
        // 1. background -> offscreen
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, self.fbo);
        glViewport(0, 0, self.fw, self.fh);
        (self.gl.UseProgram)(self.bg_prog);
        (self.gl.Uniform2f)(self.loc.bg_res, self.fw as f32, self.fh as f32);
        (self.gl.Uniform1f)(self.loc.bg_time, (f.t as f32) * bg::TIME_SCALE[f.bg.min(bg::NAMES.len() - 1)]);
        Self::quad(-1.0, -1.0, 1.0, 1.0, false);
        // 2. blit to target
        (self.gl.BindFramebuffer)(gl::FRAMEBUFFER, target.map_or(0, |t| t.0));
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
        if target.is_none() {
            SwapBuffers(self.hdc);
        }
    }

    unsafe fn draw_text(&mut self, f: &Frame, tw: i32, th: i32) {
        let hh = th as f32;
        let (em1, emc, em2, em3, em4) = ((0.19 * hh) as i32, (0.034 * hh) as i32, (0.085 * hh) as i32, (0.25 * hh) as i32, (0.036 * hh) as i32);
        let pad = (0.035 * hh).ceil() as i32;
        let cfg = f.cfg;
        let st = &f.now;

        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        let d = st.wDay as u32;
        let suffix = if (11..=13).contains(&(d % 100)) { "th" } else { match d % 10 { 1 => "st", 2 => "nd", 3 => "rd", _ => "th" } };
        let date = format!("{} {}{} {}", MONTHS[(st.wMonth as usize).clamp(1, 12) - 1], d, suffix, st.wYear);
        let hour = if cfg.clock24 { st.wHour as u32 } else { (st.wHour as u32 + 11) % 12 + 1 };
        let time = if cfg.seconds { format!("{}:{:02}:{:02}", hour, st.wMinute, st.wSecond) } else { format!("{}:{:02}", hour, st.wMinute) };

        // rows: weather (day / feels / dew + captions), date, time, details
        let temp_s = |v: f64| format!("{}\u{b0}", (if cfg.fahrenheit { v } else { (v - 32.0) * 5.0 / 9.0 }).round() as i64);
        let has_wx = f.wx.is_some();
        if let Some(w) = f.wx {
            self.upload(0, DAYS[(st.wDayOfWeek as usize) % 7], em1, pad, th);
            self.upload(1, &temp_s(w.feels), em1, pad, th);
            self.upload(2, &temp_s(w.dew), em1, pad, th);
            self.upload(3, "feels like", emc, pad, th);
            self.upload(4, "dew point", emc, pad, th);
        }
        self.upload(5, &date, em2, pad, th);
        self.upload(6, &time, em3, pad, th);
        let details = if cfg.details { f.wx.map(|w| details_line(w, cfg)).unwrap_or_default() } else { String::new() };
        let has_details = !details.is_empty();
        if has_details {
            self.upload(7, &details, em4, pad, th);
        }

        // layout in pixels, origin at the block's centre; y grows downward
        let cw = |r: &Run| (r.w - 2 * r.pad) as f32;
        let chh = |r: &Run| (r.h - 2 * r.pad) as f32;
        let r = self.runs;
        let mut items: Vec<(usize, f32, f32, f32)> = Vec::new(); // slot, x (left), y (top), alpha
        let mut y = 0.0f32;
        if has_wx {
            let gap = em1 as f32 * 0.55;
            let total = cw(&r[0]) + gap + cw(&r[1]) + gap + cw(&r[2]);
            let x0 = -total / 2.0;
            let (xd, xt, xw) = (x0, x0 + cw(&r[0]) + gap, x0 + cw(&r[0]) + gap + cw(&r[1]) + gap);
            items.push((0, xd, y, 1.0));
            items.push((1, xt, y, 1.0));
            items.push((2, xw, y, 0.62));
            let cy = y + chh(&r[1]) * 0.80;
            items.push((3, xt + (cw(&r[1]) - cw(&r[3])) / 2.0, cy, 0.7));
            items.push((4, xw + (cw(&r[2]) - cw(&r[4])) / 2.0, cy, 0.7));
            y = cy + chh(&r[3]) * 0.95 + em2 as f32 * 0.10;
        }
        items.push((5, -cw(&r[5]) / 2.0, y, 1.0));
        y += chh(&r[5]) * 0.92;
        items.push((6, -cw(&r[6]) / 2.0, y, 1.0));
        y += chh(&r[6]) * 0.90;
        if has_details {
            items.push((7, -cw(&r[7]) / 2.0, y, 0.6));
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
        let sh_px = 0.009 * hh;
        for (slot, x, ly, alpha) in items {
            let run = self.runs[slot];
            let (sx, sy) = (cx + x * s - run.pad as f32 * s, cy0 + (ly - block_h / 2.0) * s - run.pad as f32 * s);
            let (qw, qh) = (run.w as f32 * s, run.h as f32 * s);
            (self.gl.ActiveTexture)(gl::TEXTURE0 + 1);
            glBindTexture(GL_TEXTURE_2D, run.sh_tex);
            (self.gl.ActiveTexture)(gl::TEXTURE0);
            glBindTexture(GL_TEXTURE_2D, run.tex);
            (self.gl.Uniform1f)(self.loc.text_alpha, alpha);
            (self.gl.Uniform2f)(self.loc.text_shoff, sh_px / run.w as f32, sh_px / run.h as f32);
            let ndc = |px: f32, size: f32| px / size * 2.0 - 1.0;
            Self::quad(ndc(sx, tw as f32), -ndc(sy + qh, th as f32), ndc(sx + qw, tw as f32), -ndc(sy, th as f32), true);
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
