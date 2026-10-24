//! Flurry: streams of glowing smoke particles chasing a swirling star. A port of Roy Curtis' Flurry-WebGL
//! (https://github.com/RoyCurtis/Flurry-WebGL, MIT), which is itself a port of Calum Robinson's Flurry
//! (https://github.com/calumr/flurry). The simulation runs here on the CPU; `render.rs` draws the quads this
//! produces into a persistent buffer that is faded a little every frame, which is what leaves the trails.

use crate::options::{Kind, Options, Param, Preset, Values};
use std::f64::consts::PI;

pub const MAX_SMOKE: usize = 1800;
const MAX_ANGLES: f64 = 16384.0;
const BIG_MYSTERY: f64 = 1800.0;
const STREAMS: usize = 32;
/// Particle sizes are tuned for roughly this many pixels of screen width; wider buffers scale them up.
const REF_WIDTH: f64 = 1280.0;

const BLEND_NAMES: [&str; 4] = ["Normal", "Additive", "Subtract", "Multiply"];
const COLOR_NAMES: [&str; 13] = ["Red", "Magenta", "Blue", "Cyan", "Green", "Yellow", "Slow cyclic", "Cyclic", "Tie-dye", "Rainbow", "White", "Multicolour", "Dark"];
pub const BLEND_NORMAL: usize = 0;
pub const BLEND_ADDITIVE: usize = 1;
pub const BLEND_SUBTRACT: usize = 2;
pub const BLEND_MULTIPLY: usize = 3;

macro_rules! p {
    ($key:literal, $label:literal, $group:literal, $kind:expr, $def:expr) => {
        Param { key: $key, label: $label, group: $group, kind: $kind, default: $def as f64 }
    };
}

pub static OPTIONS: Options = Options {
    title: "Flurry",
    params: &[
        p!("focus", "Focus (particle spread)", "Shape", Kind::Float(0.0, 200.0), 0),
        p!("gravity", "Gravity", "Shape", Kind::Float(0.0, 600.0), 150),
        p!("incohesion", "Incohesion", "Shape", Kind::Float(0.0, 1.0), 0.07),
        p!("seraphDistance", "Distance", "Shape", Kind::Float(0.0, 500.0), 200),
        p!("backColor", "Background colour", "Colour", Kind::Color, 0),
        p!("blendMode", "Blend mode", "Colour", Kind::Choice(&BLEND_NAMES), BLEND_ADDITIVE),
        p!("brightness", "Particle brightness", "Colour", Kind::Float(0.0, 5.0), 1),
        p!("colorIncoherence", "Colour incoherence", "Colour", Kind::Float(0.0, 3.0), 0.15),
        p!("colorMode", "Colour mode", "Colour", Kind::Choice(&COLOR_NAMES), 8),
        p!("fade", "Trail fade per frame (1 = no trails)", "Colour", Kind::Float(0.0, 1.0), 0.1),
        p!("fieldCoherence", "Field coherence", "Field", Kind::Float(0.0, 10.0), 0),
        p!("fieldSpeed", "Field speed", "Field", Kind::Float(0.0, 100.0), 12),
        p!("fieldRange", "Field range", "Field", Kind::Float(0.0, 500.0), 100),
        p!("numStreams", "Number of streams", "Streams", Kind::Int(1, STREAMS as i32), 5),
        p!("streamBias", "Stream bias", "Streams", Kind::Float(0.0, 50.0), 7),
        p!("streamExpansion", "Stream expansion", "Streams", Kind::Float(1.0, 250.0), 100),
        p!("streamSize", "Stream size", "Streams", Kind::Float(0.0, 100.0), 25),
        p!("streamSpeed", "Stream speed", "Streams", Kind::Float(0.0, 100.0), 45),
    ],
    // the presets of Flurry-WebGL; colours are 0xRRGGBB
    presets: &[
        Preset { name: "Default", values: &[] },
        Preset { name: "Nevada", values: &[("focus", 200.0), ("fade", 0.02), ("brightness", 5.0), ("streamSize", 0.0)] },
        Preset {
            name: "Birds",
            values: &[("focus", 200.0), ("backColor", 0xdeccb2 as f64), ("blendMode", 2.0), ("fade", 0.198518), ("brightness", 1.985180), ("numStreams", 8.0), ("streamBias", 5.514389), ("streamSize", 0.0)],
        },
        Preset {
            name: "Skittle Spray",
            values: &[
                ("blendMode", 0.0), ("brightness", 5.0), ("colorIncoherence", 3.0), ("colorMode", 9.0), ("fade", 0.0), ("fieldSpeed", 20.0), ("numStreams", 32.0), ("streamBias", 18.0),
                ("streamExpansion", 250.0), ("streamSize", 30.0), ("streamSpeed", 100.0),
            ],
        },
        Preset {
            name: "Treasure",
            values: &[
                ("focus", 30.347163), ("seraphDistance", 157.0), ("backColor", 0x030e31 as f64), ("colorMode", 3.0), ("fade", 0.055144), ("brightness", 2.040324), ("colorIncoherence", 0.496295),
                ("fieldSpeed", 100.0), ("streamSize", 50.0),
            ],
        },
        Preset {
            name: "Bokeh",
            values: &[
                ("focus", 200.0), ("gravity", 91.0), ("incohesion", 1.0), ("seraphDistance", 211.0), ("fade", 1.0), ("brightness", 5.0), ("fieldSpeed", 55.143891), ("fieldRange", 500.0),
                ("numStreams", 13.0), ("streamBias", 0.0), ("streamSize", 33.0), ("streamSpeed", 40.0),
            ],
        },
        Preset {
            name: "Heaven Or Las Vegas",
            values: &[
                ("incohesion", 1.0), ("backColor", 0x69102a as f64), ("colorMode", 0.0), ("fade", 0.055144), ("colorIncoherence", 1.191108), ("fieldRange", 49.0), ("numStreams", 13.0), ("streamSize", 9.0),
            ],
        },
        Preset {
            name: "Garlands",
            values: &[
                ("focus", 86.706181), ("gravity", 330.0), ("incohesion", 1.0), ("seraphDistance", 102.0), ("backColor", 0xaa9eca as f64), ("blendMode", 0.0), ("colorMode", 9.0), ("fade", 0.033086),
                ("brightness", 0.055144), ("colorIncoherence", 3.0), ("fieldSpeed", 100.0), ("fieldRange", 70.0), ("numStreams", 18.0), ("streamSize", 50.0), ("streamSpeed", 100.0),
            ],
        },
        Preset {
            name: "Passionfruit",
            values: &[
                ("focus", 86.706181), ("gravity", 330.0), ("incohesion", 1.0), ("seraphDistance", 0.0), ("backColor", 0x892f2f as f64), ("colorMode", 5.0), ("fade", 0.297777), ("brightness", 5.0),
                ("colorIncoherence", 0.0), ("fieldSpeed", 100.0), ("fieldRange", 70.0), ("numStreams", 20.0), ("streamSize", 20.0), ("streamSpeed", 100.0),
            ],
        },
        Preset {
            name: "Spider",
            values: &[
                ("gravity", 162.574090), ("incohesion", 0.0), ("backColor", 0xffffff as f64), ("blendMode", 2.0), ("colorMode", 10.0), ("fade", 0.5), ("brightness", 1.047734), ("colorIncoherence", 0.0),
                ("fieldCoherence", 10.0), ("fieldSpeed", 3.308633), ("fieldRange", 500.0), ("numStreams", 8.0), ("streamBias", 25.366190), ("streamExpansion", 250.0), ("streamSize", 3.308633), ("streamSpeed", 100.0),
            ],
        },
    ],
};

/// The options in the form the simulation uses.
pub struct Params {
    pub focus: f64,
    pub gravity: f64,
    pub incohesion: f64,
    pub seraph: f64,
    pub back: [f32; 3],
    pub blend: usize,
    pub brightness: f64,
    pub color_incoherence: f64,
    pub color_mode: usize,
    pub fade: f64,
    pub field_coherence: f64,
    pub field_speed: f64,
    pub field_range: f64,
    pub streams: usize,
    pub stream_bias: f64,
    pub stream_expansion: f64,
    pub stream_size: f64,
    pub stream_speed: f64,
}

impl Params {
    pub fn from_values(v: &Values) -> Params {
        let v = OPTIONS.resolve(v);
        let g = |k: &str| v[k];
        let rgb = g("backColor") as u32;
        Params {
            focus: g("focus"),
            gravity: g("gravity"),
            incohesion: g("incohesion"),
            seraph: g("seraphDistance"),
            back: [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0],
            blend: g("blendMode") as usize,
            brightness: g("brightness"),
            color_incoherence: g("colorIncoherence"),
            color_mode: g("colorMode") as usize,
            fade: g("fade"),
            field_coherence: g("fieldCoherence"),
            field_speed: g("fieldSpeed"),
            field_range: g("fieldRange"),
            streams: g("numStreams") as usize,
            stream_bias: g("streamBias"),
            stream_expansion: g("streamExpansion"),
            stream_size: g("streamSize"),
            stream_speed: g("streamSpeed"),
        }
    }
}

struct Rng(u64);

impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn flt(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    /// Roughly bell-shaped, in -scale..scale.
    fn bell(&mut self, scale: f64) -> f64 {
        scale * (1.0 - (self.unit() + self.unit() + self.unit()) / 1.5)
    }
    fn clib(&mut self) -> u32 {
        (self.unit() * 32767.0) as u32
    }
}

struct Star {
    pos: [f64; 3],
    mystery: f64,
    rot_speed: f64,
}

impl Star {
    fn update(&mut self, time: f64, p: &Params) {
        let rots_per_second = (2.0 * PI * 12.0 / MAX_ANGLES) * self.rot_speed;
        let angle = time * rots_per_second;
        let cf = ((7.0 * angle).cos() + (3.0 * angle).cos() + (13.0 * angle).cos()) / 6.0 + 0.75;
        let rads = 2.0 * PI * self.mystery / BIG_MYSTERY;
        let mut pos = [
            250.0 * cf * (11.0 * (rads + 3.0 * angle)).cos(),
            250.0 * cf * (12.0 * (rads + 4.0 * angle)).sin(),
            250.0 * (23.0 * (rads + 4.0 * angle)).cos(),
        ];
        let rot = angle * 0.501 + 5.01 * self.mystery / BIG_MYSTERY;
        let (cr, sr) = (rot.cos(), rot.sin());
        let (x1, y1, z1) = (pos[0] * cr - pos[1] * sr, pos[1] * cr + pos[0] * sr, pos[2]);
        let (x2, z2) = (x1 * cr - z1 * sr, z1 * cr + x1 * sr);
        let y3 = y1 * cr - z2 * sr;
        let z3 = z2 * cr + y1 * sr + p.seraph * 10.0;
        let rot = angle * 2.501 + 85.01 * self.mystery / BIG_MYSTERY;
        let (cr, sr) = (rot.cos(), rot.sin());
        pos[0] = x2 * cr - y3 * sr;
        pos[1] = y3 * cr + x2 * sr;
        pos[2] = z3;
        self.pos = pos;
    }
}

/// One stream: where its particles flow towards, and the colour of its next burst.
struct Spark {
    pos: [f64; 3],
    color: [f64; 3],
    mystery: f64,
}

impl Spark {
    fn update(&mut self, time: f64, rand_seed: f64, p: &Params, rng: &mut Rng) {
        let rots_per_second = 2.0 * PI * p.field_speed / MAX_ANGLES;
        self.update_color(time, rand_seed, p);
        let angle = time * rots_per_second;
        let rads = 2.0 * PI * self.mystery / BIG_MYSTERY;
        let cf = ((7.0 * angle).cos() + (3.0 * angle).cos() + (13.0 * angle).cos()) / 6.0 + 2.0;
        let r = p.field_range * 10.0;
        let pos = [r * cf * (11.0 * (rads + 3.0 * angle)).cos(), r * cf * (12.0 * (rads + 4.0 * angle)).sin(), r * (23.0 * (rads + 12.0 * angle)).cos()];
        let rot = angle * 0.501 + 5.01 * self.mystery / BIG_MYSTERY;
        let (cr, sr) = (rot.cos(), rot.sin());
        let (x1, y1, z1) = (pos[0] * cr - pos[1] * sr, pos[1] * cr + pos[0] * sr, pos[2]);
        let (x2, y2, z2) = (x1 * cr - z1 * sr, y1, z1 * cr + x1 * sr);
        let (x3, y3, z3) = (x2, y2 * cr - z2 * sr, z2 * cr + y2 * sr + p.seraph * 10.0);
        let rot = angle * 2.501 + 85.01 * self.mystery / BIG_MYSTERY;
        let (cr, sr) = (rot.cos(), rot.sin());
        let (x4, y4, z4) = (x3 * cr - y3 * sr, y3 * cr + x3 * sr, z3);
        let c = 5.0 * p.field_coherence;
        self.pos = [x4 + rng.bell(c), y4 + rng.bell(c), z4 + rng.bell(c)];
    }

    fn update_color(&mut self, time: f64, rand_seed: f64, p: &Params) {
        let rots_per_second = 2.0 * PI * p.field_speed / MAX_ANGLES;
        let angle = time * rots_per_second;
        let cycle = match p.color_mode {
            9 => 1.5,
            8 => 4.5,
            7 => 20.0,
            _ => 120.0,
        };
        let color_rot = 2.0 * PI / cycle;
        let (red_shift, green_shift, blue_shift) = (0.0, cycle / 3.0, cycle * 2.0 / 3.0);
        let base = match p.color_mode {
            10 => [0.1875; 3],
            11 => [0.0625; 3],
            12 => [0.0; 3],
            m => {
                let t = if m < 6 { (m as f64 / 6.0) * cycle } else { time + rand_seed };
                [
                    0.109375 * (((t + red_shift) * color_rot).cos() + 1.0),
                    0.109375 * (((t + green_shift) * color_rot).cos() + 1.0),
                    0.109375 * (((t + blue_shift) * color_rot).cos() + 1.0),
                ]
            }
        };
        let rads = 2.0 * PI * self.mystery / BIG_MYSTERY;
        self.color[0] = base[0] + 0.0625 * (0.5 + (15.0 * (rads + 3.0 * angle)).cos() + (7.0 * (rads + angle)).sin());
        self.color[1] = base[1] + 0.0625 * (0.5 + (rads + angle).sin());
        self.color[2] = base[2] + 0.0625 * (0.5 + (37.0 * (rads + angle)).cos());
    }
}

#[derive(Clone, Copy, Default)]
struct Particle {
    pos: [f64; 3],
    old: [f64; 3],
    delta: [f64; 3],
    color: [f64; 4],
    born: f64,
    frame: u32,
    dead: bool,
}

/// One smoke particle as a textured quad, in pixels (origin bottom-left), corners in drawing order.
pub struct Quad {
    pub x: [f32; 4],
    pub y: [f32; 4],
    pub u: [f32; 4],
    pub v: [f32; 4],
    pub color: [f32; 4],
}

pub struct Flurry {
    rng: Rng,
    time: f64,
    rand_seed: f64,
    star: Star,
    sparks: Vec<Spark>,
    parts: Vec<Particle>,
    next: usize,
    last_emit: f64,
    old_star: [f64; 3],
    first: bool,
    /// Smoothed seconds per frame.
    dt: f64,
    pub quads: Vec<Quad>,
}

impl Flurry {
    pub fn new() -> Flurry {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
        let mut rng = Rng(seed ^ 0x9E37_79B9_7F4A_7C15);
        for _ in 0..8 {
            rng.unit();
        }
        let rand_seed = rng.flt(0.0, 300.0);
        let star = Star { pos: [0.0; 3], mystery: rng.flt(0.0, 10.0), rot_speed: rng.flt(0.4, 0.9) };
        let sparks = (0..STREAMS)
            .map(|i| Spark { pos: [rng.flt(-100.0, 100.0), rng.flt(-100.0, 100.0), rng.flt(-100.0, 100.0)], color: [0.0; 3], mystery: BIG_MYSTERY * (i as f64 + 1.0) / 32.0 })
            .collect();
        let old_star = [rng.flt(-100.0, 100.0), rng.flt(-100.0, 100.0), rng.flt(-100.0, 100.0)];
        Flurry {
            rng,
            time: rand_seed,
            rand_seed,
            star,
            sparks,
            parts: vec![Particle { dead: true, ..Default::default() }; MAX_SMOKE],
            next: 0,
            last_emit: 0.25,
            old_star,
            first: true,
            dt: 1.0 / 60.0,
            quads: Vec::with_capacity(MAX_SMOKE),
        }
    }

    /// Advance by `dt` seconds and rebuild `quads` for a `w` x `h` pixel buffer.
    pub fn step(&mut self, dt: f64, p: &Params, w: f64, h: f64) {
        let dt = dt.clamp(0.0005, 0.1);
        self.dt += (dt - self.dt) * 0.05;
        self.time += dt;
        let n = p.streams.clamp(1, STREAMS);
        self.star.update(self.time, p);
        let (time, seed) = (self.time, self.rand_seed);
        for s in self.sparks.iter_mut().take(n) {
            s.update(time, seed, p, &mut self.rng);
        }
        self.update_smoke(dt, p, n);
        self.build(p, w, h);
    }

    fn update_smoke(&mut self, dt: f64, p: &Params, n: usize) {
        let star = self.star.pos;
        if self.first {
            self.first = false;
            self.last_emit = self.time;
        } else if self.time - self.last_emit >= 1.0 / 121.0 {
            let delta = [(self.old_star[0] - star[0]) * 5.0, (self.old_star[1] - star[1]) * 5.0, (self.old_star[2] - star[2]) * 5.0];
            for i in 0..n {
                let spark = &self.sparks[i];
                let coherence = (1.0 + self.rng.bell(0.25 * p.incohesion)).max(0.0);
                let d = [star[0] - spark.pos[0], star[1] - spark.pos[1], star[2] - spark.pos[2]];
                let rsq = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).max(1e-9);
                let mag = p.stream_speed * 10.0 * coherence / rsq.sqrt();
                let ci = p.color_incoherence;
                let part = &mut self.parts[self.next];
                part.pos = star;
                part.old = star;
                part.delta = [delta[0] - d[0] * mag, delta[1] - d[1] * mag, delta[2] - d[2] * mag];
                part.color = [
                    spark.color[0] * (1.0 + self.rng.bell(ci)),
                    spark.color[1] * (1.0 + self.rng.bell(ci)),
                    spark.color[2] * (1.0 + self.rng.bell(ci)),
                    0.85 * (1.0 + self.rng.bell(0.5 * ci)),
                ];
                part.born = self.time;
                part.dead = false;
                part.frame = self.rng.clib() & 63;
                self.next = (self.next + 1) % MAX_SMOKE;
            }
            self.last_emit = self.time;
        }
        self.old_star = star;

        let frame_mod = 42.5 * self.dt;
        let drag = 0.9965f64.powf(dt * 85.0);
        for (idx, part) in self.parts.iter_mut().enumerate() {
            if part.dead {
                continue;
            }
            let mut d = part.delta;
            for (j, spark) in self.sparks.iter().take(n).enumerate() {
                let r = [part.pos[0] - spark.pos[0], part.pos[1] - spark.pos[1], part.pos[2] - spark.pos[2]];
                let rsq = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).max(1e-9);
                let mut f = (p.gravity * 10000.0 / rsq) * frame_mod;
                if idx % n == j {
                    f *= 1.0 + p.stream_bias;
                }
                let mag = f / rsq.sqrt();
                d[0] -= r[0] * mag;
                d[1] -= r[1] * mag;
                d[2] -= r[2] * mag;
            }
            d = [d[0] * drag, d[1] * drag, d[2] * drag];
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= 25_000_000.0 {
                part.dead = true;
                continue;
            }
            part.delta = d;
            part.old = part.pos;
            for k in 0..3 {
                part.pos[k] += d[k] * dt;
            }
        }
    }

    fn build(&mut self, p: &Params, sw: f64, sh: f64) {
        self.quads.clear();
        let scale = sw / REF_WIDTH;
        let ratio = sw / sh;
        let stream_size = p.stream_size * 1000.0;
        let max_width = (stream_size + 2.5 * p.stream_expansion) * ratio;
        let (half_w, half_h) = (sw * 0.5, sh * 0.5);
        for (idx, part) in self.parts.iter_mut().enumerate() {
            if part.dead {
                continue;
            }
            let width = (stream_size + (self.time - part.born) * p.stream_expansion) * ratio;
            if width >= max_width {
                part.dead = true;
                continue;
            }
            let (z, oldz) = (part.pos[2], part.old[2]);
            let sx = part.pos[0] * sw / z + half_w;
            let sy = part.pos[1] * sw / z + half_h;
            if sx > sw + 50.0 || sx < -50.0 || sy > sh + 50.0 || sy < -50.0 || z < 25.0 || oldz < 25.0 {
                continue;
            }
            let w = (width * scale / z).max(1.0);
            let osx = part.old[0] * sw / oldz + half_w;
            let osy = part.old[1] * sw / oldz + half_h;
            let (dx, dy) = (sx - osx, sy - osy);
            let d = fast_dist(dx, dy);
            let sm = if d != 0.0 { w / d } else { 0.0 };
            let ow = (width * scale / oldz).max(1.0);
            let os = if d != 0.0 { ow / d } else { 0.0 };
            let m = 1.0 + sm;
            let (dxs, dys, dxos, dyos, dxm, dym) = (dx * sm, dy * sm, dx * os, dy * os, dx * m, dy * m);

            part.frame = (part.frame + 1) % 64;
            let u0 = (part.frame & 7) as f32 * 0.125;
            let v0 = (part.frame >> 3) as f32 * 0.125;
            let cm = 0.375 * p.brightness;
            let off = (idx % 4) as f64 - 2.0;
            let off = off * p.focus * scale;
            let c = part.color;
            self.quads.push(Quad {
                x: [(sx + dxm - dys + off) as f32, (sx + dxm + dys + off) as f32, (osx - dxm + dyos + off) as f32, (osx - dxm - dyos + off) as f32],
                y: [(sy + dym + dxs + off) as f32, (sy + dym - dxs + off) as f32, (osy - dym - dxos + off) as f32, (osy - dym + dxos + off) as f32],
                u: [u0, u0 + 0.125, u0 + 0.125, u0],
                v: [v0, v0, v0 + 0.125, v0 + 0.125],
                color: [(c[0] * cm) as f32, (c[1] * cm) as f32, (c[2] * cm) as f32, (c[3] * cm) as f32],
            });
        }
    }
}

/// Distance from the origin to (x, y) with about 3.5% error, as in the original.
fn fast_dist(x: f64, y: f64) -> f64 {
    let (x, y) = (x.abs(), y.abs());
    let mn = x.min(y);
    x + y - mn * 0.5 - mn * 0.25 + mn * 0.0625
}

/// The 256x256 luminance+alpha particle texture: an 8x8 grid of 32x32 speckled soft spots, built the way the original does.
pub fn texture() -> Vec<u8> {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D ^ 0xA5A5);
    let mut small = [[0u8; 32]; 32];
    let mut big = vec![0u8; 256 * 256 * 2];
    let radius = |i: usize, j: usize| (((i as f64 - 15.5).powi(2)) + ((j as f64 - 15.5).powi(2))).sqrt();
    let mut first = true;
    for ti in 0..8 {
        for tj in 0..8 {
            if ti == 7 && tj == 7 {
                // the last tile is the previous one averaged with the very first, so the sequence loops smoothly
                let first_tile = |i: usize, j: usize| big[(i * 256 + j) * 2] as f64;
                for i in 0..32 {
                    for j in 0..32 {
                        small[i][j] = ((small[i][j] as f64 + first_tile(i, j)) / 2.0).min(255.0) as u8;
                    }
                }
            } else {
                for i in 0..32 {
                    for j in 0..32 {
                        let r = radius(i, j);
                        if first {
                            small[i][j] = if r > 15.0 { 0 } else { (255.0 * (r * PI / 31.0).cos()) as u8 };
                        } else {
                            let t = if r > 15.0 { 0.0 } else { 255.0 * (r * PI / 31.0).cos() };
                            small[i][j] = ((t + small[i][j] as f64 + small[i][j] as f64) / 3.0).min(255.0) as u8;
                        }
                    }
                }
                first = false;
                // speckle
                for i in 2..30 {
                    for j in 2..30 {
                        let mut speck = 1u32;
                        while speck <= 32 && rng.clib() % 2 == 1 {
                            small[i][j] = (small[i][j] as u32 + speck).min(255) as u8;
                            speck += speck;
                        }
                        speck = 1;
                        while speck <= 32 && rng.clib() % 2 == 1 {
                            small[i][j] = (small[i][j] as i32 - speck as i32).max(0) as u8;
                            speck += speck;
                        }
                    }
                }
                // smooth, twice
                for _ in 0..2 {
                    let mut filtered = small;
                    for i in 1..31 {
                        for j in 1..31 {
                            let t = small[i][j] as u32 * 4 + small[i - 1][j] as u32 + small[i + 1][j] as u32 + small[i][j - 1] as u32 + small[i][j + 1] as u32;
                            filtered[i][j] = (t / 8) as u8;
                        }
                    }
                    small = filtered;
                }
            }
            for i in 0..32 {
                for j in 0..32 {
                    let o = ((ti * 32 + i) * 256 + tj * 32 + j) * 2;
                    big[o] = small[i][j];
                    big[o + 1] = small[i][j];
                }
            }
        }
    }
    big
}
