//! Background fragment shaders (GLSL 1.20 so they run anywhere with OpenGL 2.x+).
//! Every shader defines `mainImage` and gets `iResolution` / `iTime` (Shadertoy-style).

pub const NAMES: [&str; 8] = [
    "Original bubbles (dash2)",
    "Silk ribbons",
    "Aurora",
    "Starstream (Flurry-ish)",
    "Plasma bloom",
    "Random gallery (SnoopethDuckDuck, Shadertoy)",
    "Frozen flame cavern (mrange, Shadertoy)",
    "Zippy zaps (SnoopethDuckDuck, Shadertoy)",
];

/// Playback speed per background (Shadertoy shaders are often tuned faster than a screensaver wants).
pub const TIME_SCALE: [f32; 8] = [1.0, 1.0, 1.0, 1.0, 1.0, 0.45, 1.0, 0.6];

/// Shadertoy compatibility layer: `iResolution` is a vec3 there, and GLSL 1.20 lacks tanh()/round().
/// Who made each background and under what terms (shown in the settings window).
pub struct Credit {
    pub author: &'static str,
    pub license: &'static str,
    pub source: &'static str,
    pub retrieved: &'static str,
    /// What was changed from the original, if anything.
    pub changes: &'static str,
}

const OWN: Credit = Credit {
    author: "Written for this project",
    license: "Original to this project",
    source: "src/bg.rs",
    retrieved: "n/a",
    changes: "n/a",
};
const SHADERTOY_DEFAULT: &str = "CC BY-NC-SA 3.0 (Shadertoy default; none stated by the author)";

pub const CREDITS: [Credit; 8] = [
    Credit {
        author: "Jesse Hufstetler (ported from dash2.html)",
        license: "Own work",
        source: "jesse-hufstetler.github.io/dash2.html",
        retrieved: "2026-10-18",
        changes: "Dither moved to the final blit; otherwise a straight port",
    },
    OWN,
    OWN,
    OWN,
    OWN,
    Credit {
        author: "SnoopethDuckDuck (fork of \"starting point 8008\")",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/stdSDf",
        retrieved: "2026-10-18",
        changes: "Unmodified code; playback slowed to 0.45x, rendered at native resolution",
    },
    Credit {
        author: "mrange",
        license: "CC0 (public domain)",
        source: "shadertoy.com/view/N3dXRI",
        retrieved: "2026-10-18",
        changes: "Zero-initialised i, z and O (desktop GLSL does not); rendered at native resolution",
    },
    Credit {
        author: "SnoopethDuckDuck (golf help: Nguyen2007, bug)",
        license: SHADERTOY_DEFAULT,
        source: "shadertoy.com/view/XXyGzh",
        retrieved: "2026-10-18",
        changes: "Zero-initialised i, split one dot(u *= m, u) for defined order; playback 0.6x, native resolution",
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
    - zippopotam.us (zip codes) and ipwho.is (IP location) for finding your location.\r\n\
    Forecasts are averaged across sources; this is not an official forecast.\r\n\
    \r\nFONT\r\n\
    - Droid Sans Mono, Apache License 2.0 (as used on the original dash2 page).\r\n\
    \r\nSee THIRD_PARTY.md in the repository for full notices.";
    s
}

/// Minimum render scale per background. Hard-edged, high-frequency shaders turn grainy when upscaled from a
/// reduced-resolution buffer, so they always render at native resolution.
pub const MIN_SCALE: [f32; 8] = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

pub const HEADER: &str = "#version 120
uniform vec2 wx_res;
uniform float iTime;
#define iResolution vec3(wx_res, 1.0)
#define tanh wx_tanh
#define round wx_round
float wx_tanh(float x){ x = clamp(x, -15.0, 15.0); float e = exp(2.0*x); return (e - 1.0)/(e + 1.0); }
vec2 wx_tanh(vec2 x){ return vec2(wx_tanh(x.x), wx_tanh(x.y)); }
vec3 wx_tanh(vec3 x){ return vec3(wx_tanh(x.x), wx_tanh(x.y), wx_tanh(x.z)); }
float wx_round(float x){ return floor(x + 0.5); }
vec3 wx_round(vec3 x){ return floor(x + 0.5); }
";
pub const FOOTER: &str = "\nvoid main(){ vec4 c; mainImage(c, gl_FragCoord.xy); gl_FragColor = c; }\n";

pub fn source(i: usize) -> String {
    format!("{HEADER}{}{FOOTER}", SHADERS[i.min(SHADERS.len() - 1)])
}

pub const SHADERS: [&str; 8] = [
    BUBBLES,
    RIBBONS,
    AURORA,
    STARSTREAM,
    PLASMA,
    include_str!("shaders/random_gallery.glsl"),
    include_str!("shaders/frozen_flame.glsl"),
    include_str!("shaders/zippy_zaps.glsl"),
];

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

/// Twisting silk ribbons, a nod to the Windows Ribbons saver.
const RIBBONS: &str = r#"
vec3 pal(float i){ return 0.5 + 0.5*cos(6.2831*(vec3(0.0, 0.33, 0.67) + i*0.11 + iTime*0.012)); }
void mainImage(out vec4 fc, in vec2 frag){
    vec2 p = (frag - 0.5*iResolution.xy) / iResolution.y;
    vec3 col = vec3(0.006, 0.008, 0.02) * (1.2 - p.y);
    for (int i = 0; i < 8; i++){
        float fi = float(i);
        float t = iTime*(0.10 + 0.017*fi) + fi*1.9;
        float x = p.x*(1.0 + 0.07*fi) + 0.15*sin(t*0.5 + fi);
        float yc = 0.20*sin(x*1.7 + t) + 0.09*sin(x*3.3 - t*1.4 + fi) + 0.04*sin(x*6.1 + t*2.3) + (fi - 3.5)*0.055;
        float tw = x*2.1 + t*1.2 + fi*2.3;
        float c = cos(tw);
        float w = 0.010 + 0.060*abs(c);
        float dy = p.y - yc;
        float d = abs(dy) / w;
        float body = smoothstep(1.0, 0.82, d);
        float lit = 0.5 + 0.5*(dy/w)*sin(tw)*sign(c);
        float face = 0.55 + 0.45*smoothstep(-0.25, 0.25, c);
        vec3 base = mix(pal(fi), vec3(0.85), 0.12);
        col += base * body * (0.20 + 0.80*lit) * face * smoothstep(0.0, 0.12, w - 0.010);
        col += base * 0.0018 / (abs(dy) + 0.010);
    }
    col = 1.0 - exp(-col*1.05);
    fc = vec4(col, 1.0);
}
"#;

const AURORA: &str = r#"
float hash(vec2 p){ vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }
float noise(vec2 p){
    vec2 i = floor(p), f = fract(p);
    f = f*f*(3.0 - 2.0*f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}
float fbm(vec2 p){
    float a = 0.5, s = 0.0;
    for (int i = 0; i < 4; i++){ s += a*noise(p); p = p*2.03 + 7.1; a *= 0.5; }
    return s;
}
void mainImage(out vec4 fc, in vec2 frag){
    vec2 uv = frag / iResolution.xy;
    vec2 p = vec2(uv.x * iResolution.x / iResolution.y, uv.y);
    float t = iTime * 0.04;
    vec3 col = mix(vec3(0.0, 0.004, 0.016), vec3(0.004, 0.02, 0.05), uv.y);
    for (int i = 0; i < 3; i++){
        float fi = float(i);
        float base = 0.24 + 0.13*fi + 0.30*(fbm(vec2(p.x*1.1 + fi*3.1 + t*2.0, t + fi*1.7)) - 0.35);
        float h = uv.y - base;
        float curtain = smoothstep(-0.015, 0.03, h) * exp(-h * (3.2 + fi*0.8));
        float rays = 0.30 + 0.70*fbm(vec2(p.x*9.0 + fi*5.0 + 0.6*sin(t*9.0 + fi), t*5.0 + fi + h*1.5));
        vec3 c = mix(vec3(0.10, 1.00, 0.55), vec3(0.65, 0.25, 0.95), clamp(h*2.2, 0.0, 1.0));
        c = mix(c, vec3(0.2, 0.7, 1.0), 0.35*fi/2.0);
        col += c * curtain * rays * 0.62;
    }
    col = 1.0 - exp(-col*1.4);
    fc = vec4(col, 1.0);
}
"#;

/// Swirling additive streamers: three emitters leave spiral trails, like a slow Flurry.
const STARSTREAM: &str = r#"
void mainImage(out vec4 fc, in vec2 frag){
    vec2 p = (frag - 0.5*iResolution.xy) / iResolution.y;
    vec3 col = vec3(0.0);
    float T = iTime * 0.55;
    for (int e = 0; e < 3; e++){
        float fe = float(e);
        vec3 tint = 0.5 + 0.5*cos(6.2831*(vec3(0.0, 0.33, 0.67) + fe*0.31 + T*0.03));
        for (int j = 0; j < 40; j++){
            float age = float(j) * 0.045;
            float tt = T - age;
            vec2 c = vec2(0.34*sin(tt*0.31 + fe*2.1) + 0.12*sin(tt*0.83 + fe), 0.20*sin(tt*0.27 + fe*1.3) + 0.09*cos(tt*0.71 + fe*3.0));
            float fade = 1.0 - float(j)/40.0;
            for (int k = 0; k < 3; k++){
                float fk = float(k);
                float ang = tt*(1.6 + 0.35*fe + 0.22*fk) + fe*2.0 + fk*2.094 + age*2.2;
                float rad = 0.012 + 0.36*age*(0.55 + 0.45*sin(tt*0.5 + fe + fk));
                vec2 pos = c + rad*vec2(cos(ang), sin(ang));
                float d2 = dot(p - pos, p - pos);
                col += tint * fade * 0.00009 / (d2 + 0.000012 + 0.00004*age);
            }
        }
    }
    col = col + 0.35*col*col;
    col = 1.0 - exp(-col*1.4);
    fc = vec4(col, 1.0);
}
"#;

const PLASMA: &str = r#"
void mainImage(out vec4 fc, in vec2 frag){
    vec2 p = frag / iResolution.y * 3.2;
    float t = iTime * 0.16;
    float v = sin(p.x + t) + sin(p.y*1.3 - t*1.2) + sin((p.x + p.y)*0.7 + t*0.7);
    v += sin(length(p - vec2(2.8 + sin(t)*1.2, 1.6 + cos(t*0.8)*0.9))*1.9 - t*1.6);
    v += 0.6*sin(length(p - vec2(1.0, 2.2))*2.6 + t);
    // indigo -> violet -> magenta, kept dark so the clock stays readable
    float h = 0.76 + 0.13*sin(v*0.5 + t*0.8);
    vec3 pp = abs(fract(vec3(h) + vec3(1.0, 2.0/3.0, 1.0/3.0))*6.0 - 3.0);
    vec3 c = mix(vec3(1.0), clamp(pp - 1.0, 0.0, 1.0), 0.78) * (0.30 + 0.38*(0.5 + 0.5*sin(v*1.3 + t)));
    vec2 q = frag / iResolution.xy - 0.5;
    c *= 1.0 - 0.9*dot(q, q);
    fc = vec4(c, 1.0);
}
"#;
