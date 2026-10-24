//! Background fragment shaders (GLSL 1.20 so they run anywhere with OpenGL 2.x+).
//! Every shader defines `mainImage` and gets `iResolution` / `iTime` (Shadertoy-style).

pub const NAMES: [&str; 14] = [
    "Bubbles (SKEENSAVUH, Shadertoy)",
    "Random gallery (SnoopethDuckDuck, Shadertoy)",
    "Frozen flame cavern (mrange, Shadertoy)",
    "Zippy zaps (SnoopethDuckDuck, Shadertoy)",
    "Perhaps a screensaver (pirandello, Shadertoy)",
    "Hex crystal (ChunderFPV, Shadertoy)",
    "Flurry (Calum Robinson, WebGL port by Roy Curtis)",
    "Spiral journey (ChunderFPV, Shadertoy)",
    "Screensaver squares (julianlumia, Shadertoy)",
    "Rainbeam (Inityx, Shadertoy)",
    "Zooming bokeh (FreakZillA8, Shadertoy)",
    "Ultra liquid bokeh (Weyland after iq, Shadertoy)",
    "Ice and fire (mattz, Shadertoy)",
    "Wavy background (gigaherz, Shadertoy)",
];

/// Stable ids stored in config.json, so adding or reordering backgrounds never changes what a saved choice means.
pub const IDS: [&str; 14] = ["bubbles", "random_gallery", "frozen_flame", "zippy_zaps", "perhaps_a_screensaver", "hex_crystal", "mac_flurry", "spiral_journey", "screensaver_squares", "rainbeam", "zooming_bokeh", "ultra_liquid_bokeh", "ice_and_fire", "wavy_background"];
const RANDOM_ID: &str = "random";

/// Config id -> index into `NAMES`; "random" maps to `NAMES.len()`, unknown ids fall back to the first background.
pub fn id_to_index(id: &str) -> usize {
    if id == RANDOM_ID {
        NAMES.len()
    } else {
        IDS.iter().position(|x| *x == id).unwrap_or(0)
    }
}

pub fn index_to_id(i: usize) -> &'static str {
    IDS.get(i).copied().unwrap_or(RANDOM_ID)
}

/// Playback speed per background (Shadertoy shaders are often tuned faster than a screensaver wants).
pub const TIME_SCALE: [f32; 14] = [1.0, 0.45, 1.0, 0.6, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

/// Backgrounds drawn as geometry (one additively blended quad per object) instead of one big fragment shader. Their source
/// file is a plain fragment shader for one object, not a Shadertoy mainImage.
pub fn is_geometry(i: usize) -> bool {
    i == 11
}

/// The objects of a geometry background for shader time `t` and buffer aspect: 8 floats each, see `bokeh_data`.
pub fn geometry_data(i: usize, t: f32, aspect: f32) -> Vec<f32> {
    if i == 11 { bokeh_data(t, aspect) } else { Vec::new() }
}

/// Ultra liquid bokeh: the 128 bubbles of the original shader (position, radius, radius squared, colour as (z, y, x, 0)),
/// evaluated once per frame instead of once per pixel. Same formulas as the shader's loop body; arguments are rounded to f32 first like the GPU does.
fn bokeh_data(t: f32, aspect: f32) -> Vec<f32> {
    let sin = |x: f32| (x as f64).sin() as f32;
    let mut out = Vec::with_capacity(128 * 8);
    for i in 0..128 {
        let fi = i as f32;
        let pha = sin(fi * 546.13 + 1.0) * 0.5 + 0.5;
        let siz = (sin(fi * 651.74 + 5.0) * 0.5 + 0.5).powf(4.0);
        let pox = sin(fi * 321.55 + 4.1) * aspect;
        let rad = 0.1 + 0.5 * siz + sin(pha + siz) / 4.0;
        let k = pha + 0.3 * (t / 7.0) * (0.2 + 0.8 * siz);
        let pos = (pox + sin(t / 15.0 + pha + siz), -1.0 - rad + (2.0 + 2.0 * rad) * (k - k.floor()));
        let m = 0.5 + 0.5 * sin(fi);
        let (c0, c1) = ([0.194 * sin(t / 6.0) + 0.3, 0.2, 0.3 * pha], [1.1 * sin(t / 9.0) + 0.3, 0.2 * pha, 0.4]);
        let col = [c0[0] + (c1[0] - c0[0]) * m, c0[1] + (c1[1] - c0[1]) * m, c0[2] + (c1[2] - c0[2]) * m];
        out.extend_from_slice(&[pos.0, pos.1, rad, rad * rad, col[2], col[1], col[0], 0.0]);
    }
    out
}

/// Who made each background and under what terms (shown in the settings window).
pub struct Credit {
    pub author: &'static str,
    pub license: &'static str,
    pub source: &'static str,
    pub retrieved: &'static str,
    /// What was changed from the original, if anything.
    pub changes: &'static str,
}

const SHADERTOY_DEFAULT: &str = "CC BY-NC-SA 3.0 (Shadertoy default; none stated by the author)";

pub const CREDITS: [Credit; 14] = [
    Credit {
        author: "Jesse Hufstetler (JesseHufstetler on Shadertoy)",
        license: "Own work",
        source: "shadertoy.com/view/l3XGRB",
        retrieved: "2026-10-19",
        changes: "Renamed a parameter called exp (it shadowed the GLSL built-in); otherwise unmodified",
    },
    Credit {
        author: "SnoopethDuckDuck (fork of \"starting point 8008\")",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/stdSDf",
        retrieved: "2026-10-18",
        changes: "Unmodified code; playback slowed to 0.45x",
    },
    Credit {
        author: "mrange",
        license: "CC0 (public domain)",
        source: "shadertoy.com/view/N3dXRI",
        retrieved: "2026-10-18",
        changes: "Zero-initialised i, z and O (desktop GLSL does not)",
    },
    Credit {
        author: "SnoopethDuckDuck (golf help: Nguyen2007, bug)",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/XXyGzh",
        retrieved: "2026-10-18",
        changes: "Zero-initialised i, split one dot(u *= m, u) for defined order, loop written out plainly and mat2 built component by component (driver crash workarounds); playback 0.6x",
    },
    Credit {
        author: "pirandello (fork of \"Trailing the Twinkling Tunnelwisp\"; orig. BeRo & Paul Karlik)",
        license: "CC0 / public domain (stated in the shader)",
        source: "shadertoy.com/view/Wfy3zh",
        retrieved: "2026-10-18",
        changes: "Image pass only (sound omitted); zero-initialised locals; split one dot(sin(p *= s), ..) for defined order; mat2 built component by component (a driver crash workaround)",
    },
    Credit {
        author: "ChunderFPV (hexagonal tiles by lomateron)",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/ct2SWw",
        retrieved: "2026-10-18",
        changes: "RT() builds its mat2 component by component (a driver crash workaround); iMouse supplied as constant 0, so it auto-rotates",
    },
    Credit {
        author: "Roy Curtis (Flurry-WebGL), after Flurry by Calum Robinson (github.com/calumr/flurry)",
        license: "MIT (Flurry-WebGL's licence, granted with Calum Robinson's permission)",
        source: "github.com/RoyCurtis/Flurry-WebGL",
        retrieved: "2026-10-24",
        changes: "Ported from JavaScript/WebGL to Rust/OpenGL; trail fade and gravity made independent of frame rate; particle sizes scale with resolution; the texture-frame index uses the original's & operator; options and presets are edited in this app's own window",
    },
    Credit {
        author: "ChunderFPV",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/mssSWr",
        retrieved: "2026-10-18",
        changes: "Multipass port; mouse counter overlay omitted; hosted background texture replaced by generated noise",
    },
    Credit {
        author: "julianlumia (after the starfield tutorial by BigWings / The Art of Code)",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/Wdcyz7",
        retrieved: "2026-10-18",
        changes: "Renamed one local variable (star -> st) that shadowed a function; the colour of every grid cell is computed once per frame into a table instead of per pixel (same picture, much faster)",
    },
    Credit {
        author: "Inityx",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/flffWX",
        retrieved: "2026-10-18",
        changes: "Multipass port at 360px / 60 Hz steps; hosted noise textures replaced by generated noise; two identifiers renamed (blobs, saturate)",
    },
    Credit {
        author: "FreakZillA8 (mash-up; credits shadertoy.com/view/XcKGzG, /wlBGDG and /4djSRW for the hash)",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/NfSXRt",
        retrieved: "2026-10-18",
        changes: "Unmodified code; upstream parts keep their own authors' licenses (not verified)",
    },
    Credit {
        author: "Weyland (derived from \"Bubbles\" by Inigo Quilez, 2013)",
        license: "CC BY-NC-SA 3.0 Unported (stated in the shader)",
        source: "shadertoy.com/view/MtdXzr (original: shadertoy.com/view/4dl3zn)",
        retrieved: "2026-10-18",
        changes: "Each bubble is drawn as a quad with additive blending and its values are computed once per frame on the CPU, instead of every pixel looping over all 128 bubbles (same picture, much faster); formulas unmodified",
    },
    Credit {
        author: "mattz (palette, hash and barycentric helpers adapted from ll2GD3, 4djSRW, lslXDf as credited in the code)",
        license: "CC BY-NC-SA 3.0 Unported (stated in the shader)",
        source: "shadertoy.com/view/MdfBzl",
        retrieved: "2026-10-18",
        changes: "The jittered lattice points are computed once per frame into a small table instead of per pixel (same picture, much faster); otherwise unmodified",
    },
    Credit {
        author: "gigaherz",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/ltGSWD",
        retrieved: "2026-10-18",
        changes: "Unmodified code",
    },
];

/// Multi-line credit text for the settings window (\r\n for Win32 static controls).
pub fn credit_text(i: usize) -> String {
    let i = i.min(NAMES.len() - 1);
    let c = &CREDITS[i];
    format!(
        "{}\r\nBy: {}\r\nLicense: {}\r\nSource: {}\r\nRetrieved: {}\r\nChanges: {}",
        NAMES[i], c.author, c.license, c.source, c.retrieved, c.changes
    )
}

/// Same text as `credit_text`, as SysLink markup: every Shadertoy / website address becomes a clickable link.
pub fn credit_markup(i: usize) -> String {
    let text = credit_text(i);
    let shadertoy = text.contains("shadertoy.com");
    let mut out = String::with_capacity(text.len() * 2);
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.is_empty() {
            return;
        }
        let w = word.trim_end_matches('.');
        let tail = &word[w.len()..];
        let url = if w.starts_with("http") {
            Some(w.to_string())
        } else if [".com/", ".io/", ".org/"].iter().any(|d| w.contains(d)) {
            Some(format!("https://{w}"))
        } else if shadertoy && w.len() > 3 && w.starts_with('/') && w[1..].chars().all(|c| c.is_ascii_alphanumeric()) {
            Some(format!("https://www.shadertoy.com/view{w}"))
        } else {
            None
        };
        match url {
            Some(u) => out.push_str(&format!("<a href=\"{u}\">{w}</a>{tail}")),
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in text.chars() {
        if matches!(c, ' ' | '\r' | '\n' | ',' | '(' | ')' | ';') {
            flush(&mut word, &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Everything credited in one place: the app, every background, the weather data and the font.
pub fn about_text() -> String {
    let mut s = format!(
        "Weather Clock Screensaver {}\r\nhttps://github.com/Jesse-Hufstetler/weather-clock-scr\r\n\r\nBACKGROUNDS\r\n",
        env!("CARGO_PKG_VERSION")
    );
    for i in 0..NAMES.len() {
        s += &format!("\r\n{}\r\n", credit_text(i).replace("\r\n", "\r\n    "));
    }
    s += "\r\nWEATHER DATA (no API keys)\r\n\
    - NWS, api.weather.gov: US government data, public domain.\r\n\
    - MET Norway, api.met.no: CC BY 4.0 / NLOD 2.0. Data from MET Norway.\r\n\
    - Open-Meteo, open-meteo.com: CC BY 4.0, free for non-commercial use. Models: NOAA GFS/HRRR/NBM, ECMWF IFS, DWD ICON, Environment Canada GEM, UK Met Office, Meteo-France, JMA.\r\n\
    - Location: Windows Location Service first, then ipwho.is (IP address); BigDataCloud (reverse geocoding, to name the place) and zippopotam.us (zip codes you type).\r\n\
    Forecasts are averaged across sources; this is not an official forecast.\r\n\
    \r\nFONT\r\n\
    - Droid Sans Mono, Apache License 2.0 (as used on the original dash2 page).\r\n\
    \r\nSee THIRD_PARTY.md in the repository for full notices.";
    s
}

/// Shadertoy compatibility layer: `iResolution` is a vec3 there, and GLSL 1.20 lacks tanh()/round().
pub const HEADER: &str = "#version 120
uniform vec2 wx_res;
uniform float iTime;
uniform float iTimeDelta;
uniform sampler2D iChannel0, iChannel1, iChannel2, iChannel3, iChannel4;
uniform float wx_aspect; // width / height of the simulation buffers (for passes whose size differs from it)
#define texture texture2D
#define iResolution vec3(wx_res, 1.0)
#define iMouse vec4(0.0)
#define tanh wx_tanh
#define round wx_round
float wx_tanh(float x){ x = clamp(x, -15.0, 15.0); float e = exp(2.0*x); return (e - 1.0)/(e + 1.0); }
vec2 wx_tanh(vec2 x){ return vec2(wx_tanh(x.x), wx_tanh(x.y)); }
vec3 wx_tanh(vec3 x){ return vec3(wx_tanh(x.x), wx_tanh(x.y), wx_tanh(x.z)); }
vec4 wx_tanh(vec4 x){ return vec4(wx_tanh(x.x), wx_tanh(x.y), wx_tanh(x.z), wx_tanh(x.w)); }
float wx_round(float x){ return floor(x + 0.5); }
vec3 wx_round(vec3 x){ return floor(x + 0.5); }
";
pub const FOOTER: &str = "\nvoid main(){ vec4 c; mainImage(c, gl_FragCoord.xy); gl_FragColor = c; }\n";

pub fn source(i: usize) -> String {
    if is_geometry(i) {
        return single(i).unwrap_or(BUBBLES).to_string(); // a complete fragment shader, no Shadertoy wrapper
    }
    // multipass backgrounds are built from their passes (see `multi`); a plain fallback keeps this total
    format!("{HEADER}{}{FOOTER}", single(i).unwrap_or(BUBBLES))
}

/// Single-pass shader source per background index (multipass ones are built from their passes by `multi`).
fn single(i: usize) -> Option<&'static str> {
    Some(match i {
        0 => BUBBLES,
        1 => include_str!("shaders/random_gallery.glsl"),
        2 => include_str!("shaders/frozen_flame.glsl"),
        3 => include_str!("shaders/zippy_zaps.glsl"),
        4 => include_str!("shaders/perhaps_a_screensaver.glsl"),
        5 => include_str!("shaders/hex_crystal.glsl"),
        10 => include_str!("shaders/zooming_bokeh.glsl"),
        11 => include_str!("shaders/ultra_liquid_bokeh.glsl"),
        13 => include_str!("shaders/wavy_background.glsl"),
        _ => return None,
    })
}

/// The default background: the repo owner's own Shadertoy shader (see src/shaders/bubbles.glsl).
const BUBBLES: &str = include_str!("shaders/bubbles.glsl");

// ───────────── multipass (feedback) backgrounds ─────────────
/// Flurry is not a shader: a CPU particle simulation drawn as geometry (see flurry.rs). It has options and presets.
pub const FLURRY: usize = 6;

pub fn is_flurry(i: usize) -> bool {
    i == FLURRY
}

pub const SPIRAL: usize = 7;
pub const RAINBEAM: usize = 9;
pub const ICE: usize = 12;
pub const SQUARES: usize = 8;

pub fn is_multi(i: usize) -> bool {
    i == SPIRAL || i == RAINBEAM || i == ICE || i == SQUARES
}

/// What an `iChannelN` reads: nothing, or the latest output of a buffer (`mip` = sampled through its mip chain).
#[derive(Clone, Copy)]
pub enum Src {
    None,
    /// A generated tileable colour-noise texture (stands in for Shadertoy-hosted images).
    Noise,
    Buf(usize, bool),
}

pub struct Pass {
    pub code: String,
    pub inputs: [Src; 5],
    /// Fixed size in pixels for a small helper buffer (None = the simulation size).
    pub size: Option<(i32, i32)>,
}

pub struct Multi {
    pub common: &'static str,
    pub buffers: Vec<Pass>,
    pub image: Pass,
    /// The simulation runs at this fixed height (width follows the aspect). These shaders step in whole pixels,
    /// so their look depends on resolution; ~Shadertoy's preview size keeps the original look and the GPU cost low.
    pub sim_height: i32,
}

pub fn multi(i: usize) -> Option<Multi> {
    match i {
        SPIRAL => Some(spiral()),
        RAINBEAM => Some(rainbeam()),
        ICE => Some(ice()),
        SQUARES => Some(squares()),
        _ => None,
    }
}

/// Ice and fire: a tiny table of the jittered lattice points (computed per frame) plus the image pass that reads it.
fn ice() -> Multi {
    let points = Pass {
        code: include_str!("shaders/ice_and_fire/points.glsl").to_string(),
        inputs: [Src::None; 5],
        size: Some((32, 8)), // TABLE_W x TABLE_H, see common.glsl
    };
    let image = Pass {
        code: include_str!("shaders/ice_and_fire/image.glsl").to_string(),
        inputs: [Src::Buf(0, false), Src::None, Src::None, Src::None, Src::None],
        size: None,
    };
    Multi { common: include_str!("shaders/ice_and_fire/common.glsl"), buffers: vec![points], image, sim_height: 360 }
}

/// Screensaver squares: a per-frame table with the colour of every grid cell that can be on screen, plus the image pass.
fn squares() -> Multi {
    let table = Pass {
        code: include_str!("shaders/screensaver_squares/table.glsl").to_string(),
        inputs: [Src::None; 5],
        size: Some((128, 3 * 128 + 1)), // TABLE_N wide, three blocks of TABLE_N rows plus a header row (see common.glsl)
    };
    let image = Pass {
        code: include_str!("shaders/screensaver_squares/image.glsl").to_string(),
        inputs: [Src::Buf(0, false), Src::None, Src::None, Src::None, Src::None],
        size: None,
    };
    Multi { common: include_str!("shaders/screensaver_squares/common.glsl"), buffers: vec![table], image, sim_height: 360 }
}

fn rainbeam() -> Multi {
    let a = Pass {
        code: include_str!("shaders/rainbeam/buffer_a.glsl").to_string(),
        inputs: [Src::Buf(0, false), Src::Noise, Src::None, Src::None, Src::None],
        size: None,
    };
    let image = Pass { code: include_str!("shaders/rainbeam/image.glsl").to_string(), inputs: [Src::Buf(0, false), Src::Noise, Src::None, Src::None, Src::None], size: None };
    Multi { common: "", buffers: vec![a], image, sim_height: 360 }
}

fn spiral() -> Multi {
    let a = Pass {
        code: include_str!("shaders/spiral_journey/buffer_a.glsl").to_string(),
        inputs: [Src::Buf(0, true), Src::Noise, Src::None, Src::None, Src::None],
        size: None,
    };
    let image = Pass { code: include_str!("shaders/spiral_journey/image.glsl").to_string(), inputs: [Src::Buf(0, true), Src::None, Src::None, Src::None, Src::None], size: None };
    Multi { common: include_str!("shaders/spiral_journey/common.glsl"), buffers: vec![a], image, sim_height: 720 }
}

pub fn pass_source(common: &str, code: &str) -> String {
    format!("{HEADER}{common}\n{code}{FOOTER}")
}
