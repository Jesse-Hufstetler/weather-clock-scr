//! Background fragment shaders (GLSL 1.20 so they run anywhere with OpenGL 2.x+).
//! Every shader defines `mainImage` and gets `iResolution` / `iTime` (Shadertoy-style).

pub const NAMES: [&str; 13] = [
    "Original bubbles (dash2)",
    "Random gallery (SnoopethDuckDuck, Shadertoy)",
    "Frozen flame cavern (mrange, Shadertoy)",
    "Zippy zaps (SnoopethDuckDuck, Shadertoy)",
    "Perhaps a screensaver (pirandello, Shadertoy)",
    "Hex crystal (ChunderFPV, Shadertoy)",
    "Mac flurry (ufffd, Shadertoy)",
    "Spiral journey (ChunderFPV, Shadertoy)",
    "Screensaver squares (julianlumia, Shadertoy)",
    "Rainbeam (Inityx, Shadertoy)",
    "Zooming bokeh (FreakZillA8, Shadertoy)",
    "Ultra liquid bokeh (Weyland after iq, Shadertoy)",
    "Ice and fire (mattz, Shadertoy)",
];

/// Stable ids stored in config.json, so adding or reordering backgrounds never changes what a saved choice means.
pub const IDS: [&str; 13] = ["bubbles", "random_gallery", "frozen_flame", "zippy_zaps", "perhaps_a_screensaver", "hex_crystal", "mac_flurry", "spiral_journey", "screensaver_squares", "rainbeam", "zooming_bokeh", "ultra_liquid_bokeh", "ice_and_fire"];
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
pub const TIME_SCALE: [f32; 13] = [1.0, 0.45, 1.0, 0.6, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

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

pub const CREDITS: [Credit; 13] = [
    Credit {
        author: "Jesse Hufstetler (ported from dash2.html)",
        license: "Own work",
        source: "jesse-hufstetler.github.io/dash2.html",
        retrieved: "2026-10-18",
        changes: "Dither moved to the final blit; otherwise a straight port",
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
        changes: "Zero-initialised i, split one dot(u *= m, u) for defined order; playback 0.6x",
    },
    Credit {
        author: "pirandello (fork of \"Trailing the Twinkling Tunnelwisp\"; orig. BeRo & Paul Karlik)",
        license: "CC0 / public domain (stated in the shader)",
        source: "shadertoy.com/view/Wfy3zh",
        retrieved: "2026-10-18",
        changes: "Image pass only (sound omitted); zero-initialised locals; split one dot(sin(p *= s), ..) for defined order",
    },
    Credit {
        author: "ChunderFPV (hexagonal tiles by lomateron)",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/ct2SWw",
        retrieved: "2026-10-18",
        changes: "Unmodified code (iMouse supplied as constant 0, so it auto-rotates)",
    },
    Credit {
        author: "ufffd (simplex noise by Ian McEwan / Ashima Arts, MIT)",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/lX2SDt",
        retrieved: "2026-10-18",
        changes: "Multipass port: simulation at 360px height, stepped at 60 Hz; buffers A-C share one file; O-=O -> O=vec4(0)",
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
        changes: "Renamed one local variable (star -> st) that shadowed a function",
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
        changes: "Unmodified code",
    },
    Credit {
        author: "mattz (palette, hash and barycentric helpers adapted from ll2GD3, 4djSRW, lslXDf as credited in the code)",
        license: "CC BY-NC-SA 3.0 Unported (stated in the shader)",
        source: "shadertoy.com/view/MdfBzl",
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
uniform sampler2D iChannel0, iChannel1, iChannel2, iChannel3;
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
        8 => include_str!("shaders/screensaver_squares.glsl"),
        10 => include_str!("shaders/zooming_bokeh.glsl"),
        11 => include_str!("shaders/ultra_liquid_bokeh.glsl"),
        12 => include_str!("shaders/ice_and_fire.glsl"),
        _ => return None,
    })
}

/// Straight port of the shader in dash2.html (dither moved to the final blit).
const BUBBLES: &str = r#"
vec3 rgb2hsv(vec3 c){
    vec4 K = vec4(0.0, -1.0/3.0, 2.0/3.0, -1.0);
    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    return vec3(abs(q.z + (q.w - q.y) / (6.0*d + e)), d / (q.x + e), q.x);
}
vec3 hsv2rgb(vec3 c){
    vec4 K = vec4(1.0, 2.0/3.0, 1.0/3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}
void bub(inout vec4 fc, vec2 fragCoord, float radius, vec2 center, vec3 col, float e){
    float dis = length(fragCoord - center);
    float disN = dis / radius;
    if (dis < radius) fc.xyz += pow(1.0 - disN, e) * col;
}
void slideBub(inout vec4 fc, vec2 fragCoord, float radius, vec2 speed, vec3 col, float e){
    vec2 center = (sin(iTime*speed + speed*500.0) * iResolution.xy + iResolution.xy) / 2.0;
    bub(fc, fragCoord, radius, center, col, e);
}
void mainImage(out vec4 fragColor, in vec2 fragCoord){
    fragColor = vec4(0.0, 0.0, 0.0, 1.0);
    float divvy = 0.999;
    float size = 3.0;
    vec3 pink = vec3(1.0, 0.5, 1.0);
    vec3 yellow = vec3(1.0, 1.0, 0.5);
    vec3 blue = vec3(0.5, 0.9, 1.0);
    vec3 lightPink = vec3(1.0, 0.9, 1.0);
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(0.11/divvy, 0.16/divvy), pink, 5.0);
    divvy *= 1.1;
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(0.1/divvy, 0.14/divvy), blue, 5.0);
    divvy *= 1.1;
    divvy *= 1.13;
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(0.1/divvy, 0.13/divvy), yellow, 5.0);
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(0.1/divvy, 0.14/divvy), blue, 5.0);
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(0.1/divvy, 0.15/divvy), lightPink, 5.0);
    fragColor *= fragColor;
    fragColor *= fragColor;
    fragColor = vec4(rgb2hsv(fragColor.xyz), fragColor.w);
    fragColor.x *= 2.0;
    fragColor.x -= 0.6;
    fragColor = vec4(hsv2rgb(fragColor.xyz), fragColor.w);
}
"#;

// ───────────── multipass (feedback) backgrounds ─────────────
/// Index of the one multipass background (Shadertoy-style buffers A-D + Image).
pub const FLURRY: usize = 6;

pub const SPIRAL: usize = 7;
pub const RAINBEAM: usize = 9;

pub fn is_multi(i: usize) -> bool {
    i == FLURRY || i == SPIRAL || i == RAINBEAM
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
    pub inputs: [Src; 4],
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
        FLURRY => Some(flurry()),
        SPIRAL => Some(spiral()),
        RAINBEAM => Some(rainbeam()),
        _ => None,
    }
}

fn rainbeam() -> Multi {
    let a = Pass {
        code: include_str!("shaders/rainbeam/buffer_a.glsl").to_string(),
        inputs: [Src::Buf(0, false), Src::Noise, Src::None, Src::None],
    };
    let image = Pass { code: include_str!("shaders/rainbeam/image.glsl").to_string(), inputs: [Src::Buf(0, false), Src::Noise, Src::None, Src::None] };
    Multi { common: "", buffers: vec![a], image, sim_height: 360 }
}

fn spiral() -> Multi {
    let a = Pass {
        code: include_str!("shaders/spiral_journey/buffer_a.glsl").to_string(),
        inputs: [Src::Buf(0, true), Src::Noise, Src::None, Src::None],
    };
    let image = Pass { code: include_str!("shaders/spiral_journey/image.glsl").to_string(), inputs: [Src::Buf(0, true), Src::None, Src::None, Src::None] };
    Multi { common: include_str!("shaders/spiral_journey/common.glsl"), buffers: vec![a], image, sim_height: 720 }
}

fn flurry() -> Multi {
    let tails = |n: &str| Pass {
        code: format!("#define TAIL_OFFSET {n}\n{}", include_str!("shaders/mac_flurry/tails.glsl")),
        inputs: [Src::None; 4],
    };
    let mut a = tails("1.");
    let mut b = tails("10.");
    let mut c = tails("20.");
    a.inputs[0] = Src::Buf(0, true); // A reads its own previous frame, mip-filtered
    b.inputs[0] = Src::Buf(1, false);
    c.inputs[0] = Src::Buf(2, false);
    let d = Pass {
        code: include_str!("shaders/mac_flurry/buffer_d.glsl").to_string(),
        inputs: [Src::Buf(0, false), Src::Buf(1, false), Src::Buf(2, false), Src::Buf(3, true)],
    };
    let image = Pass { code: include_str!("shaders/mac_flurry/image.glsl").to_string(), inputs: [Src::Buf(3, false), Src::None, Src::None, Src::None] };
    Multi { common: include_str!("shaders/mac_flurry/common.glsl"), buffers: vec![a, b, c, d], image, sim_height: 360 }
}

pub fn pass_source(common: &str, code: &str) -> String {
    format!("{HEADER}{common}\n{code}{FOOTER}")
}
