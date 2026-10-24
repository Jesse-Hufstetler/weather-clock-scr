//! Settings stored as JSON in %APPDATA%\wxscr\config.json (no registry, no key material needed).

use crate::options::Values;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn values_from(v: &Value) -> Values {
    v.as_object().map(|o| o.iter().filter_map(|(k, x)| x.as_f64().map(|x| (k.clone(), x))).collect()).unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Detect the location on every launch (Windows Location Service, then IP address). When false, `location` is used.
    pub auto_location: bool,
    /// Typed override: zip, "city, ST" or "lat,lon". Only used when `auto_location` is off.
    pub location: String,
    /// What auto-detection last found, as text the Location box can be pre-filled with.
    pub detected_text: String,
    /// Resolved coordinates for `resolved_for` (cached so we don't geocode on every launch).
    pub lat: f64,
    pub lon: f64,
    pub place: String,
    pub resolved_for: String,
    pub fahrenheit: bool,
    pub clock24: bool,
    pub seconds: bool,
    pub details: bool,
    /// Index into bg::BACKGROUNDS, or bg::RANDOM.
    pub background: usize,
    /// Frame-rate cap; 0 = no cap (render at the display's refresh rate, paced by vsync).
    pub fps: u32,
    /// 10..=100 percent.
    pub brightness: u32,
    /// 0..=100: how much the text breathes/drifts (burn-in protection).
    pub movement: u32,
    /// 0 = clock on every monitor, 1 = clock on primary only (others show background), 2 = clock on primary only (others black).
    pub monitors: usize,
    pub refresh_min: u32,
    /// Empty = the bundled Droid Sans Mono.
    pub font: String,
    /// How the text is kept readable over busy backgrounds: 0 soft shadow, 1 black halo, 2 black outline.
    pub contrast: usize,
    /// Percent of the screen resolution the background is rendered at (100, 75 or 50); it is scaled up to the screen, the
    /// text stays sharp. Lower values make heavy backgrounds run smoothly on slower GPUs.
    pub render_scale: u32,
    /// Fade in / out of the screensaver: index into render::TRANS_NAMES (last = random).
    pub transition: usize,
    /// Chosen option values per background id (see options.rs); missing keys mean the default.
    pub bg_opts: BTreeMap<String, Values>,
    /// Presets the user saved, per background id, then by name.
    pub bg_presets: BTreeMap<String, BTreeMap<String, Values>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            auto_location: true,
            location: String::new(),
            detected_text: String::new(),
            lat: f64::NAN,
            lon: f64::NAN,
            place: String::new(),
            resolved_for: String::new(),
            fahrenheit: true,
            clock24: false,
            seconds: true,
            details: true,
            background: 0,
            fps: 0,
            brightness: 100,
            movement: 60,
            monitors: 0,
            refresh_min: 20,
            font: String::new(),
            contrast: 0,
            render_scale: 100,
            transition: 2,
            bg_opts: BTreeMap::new(),
            bg_presets: BTreeMap::new(),
        }
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join("wxscr");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn path() -> PathBuf {
    dir().join("config.json")
}

impl Config {
    pub fn load() -> Config {
        let mut c = Config::default();
        let Ok(s) = std::fs::read_to_string(path()) else { return c };
        let Ok(v) = serde_json::from_str::<Value>(&s) else { return c };
        let st = |k: &str, d: &str| v[k].as_str().unwrap_or(d).to_string();
        let bo = |k: &str, d: bool| v[k].as_bool().unwrap_or(d);
        let nu = |k: &str, d: u64| v[k].as_u64().unwrap_or(d);
        c.auto_location = bo("auto_location", true);
        c.location = st("location", "");
        c.detected_text = st("detected_text", "");
        c.lat = v["lat"].as_f64().unwrap_or(f64::NAN);
        c.lon = v["lon"].as_f64().unwrap_or(f64::NAN);
        c.place = st("place", "");
        c.resolved_for = st("resolved_for", "");
        c.fahrenheit = bo("fahrenheit", c.fahrenheit);
        c.clock24 = bo("clock24", c.clock24);
        c.seconds = bo("seconds", c.seconds);
        c.details = bo("details", c.details);
        // stored as a stable string id ("bubbles", "zippy_zaps", "random", ...); older numeric values fall back to the default
        c.background = v["background"].as_str().map(crate::bg::id_to_index).unwrap_or(0);
        // new key name on purpose: older configs saved a 30 fps cap that should no longer apply
        c.fps = match nu("fps_cap", 0) { 0 => 0, n => n.clamp(10, 240) as u32 };
        c.brightness = nu("brightness", c.brightness as u64).clamp(10, 100) as u32;
        c.movement = nu("movement", c.movement as u64).min(100) as u32;
        c.monitors = (nu("monitors", c.monitors as u64) as usize).min(2);
        c.refresh_min = nu("refresh_min", c.refresh_min as u64).clamp(5, 180) as u32;
        c.font = st("font", "");
        c.render_scale = match nu("render_scale", 100) { 75 => 75, 50 => 50, _ => 100 };
        c.contrast = (nu("contrast", c.contrast as u64) as usize).min(crate::render::CONTRAST_NAMES.len() - 1);
        c.transition = match &v["transition"] {
            Value::String(id) => crate::render::TRANS_IDS.iter().position(|x| x == id).unwrap_or(c.transition),
            Value::Number(n) => crate::render::transition_from_legacy(n.as_u64().unwrap_or(2)),
            _ => c.transition,
        };
        if let Some(o) = v["bg_options"].as_object() {
            c.bg_opts = o.iter().map(|(id, vals)| (id.clone(), values_from(vals))).collect();
        }
        if let Some(o) = v["bg_presets"].as_object() {
            c.bg_presets = o.iter().map(|(id, named)| (id.clone(), named.as_object().map(|n| n.iter().map(|(name, vals)| (name.clone(), values_from(vals))).collect()).unwrap_or_default())).collect();
        }
        c
    }

    pub fn save(&self) {
        let num = |x: f64| if x.is_finite() { json!(x) } else { Value::Null };
        let v = json!({
            "auto_location": self.auto_location, "location": self.location, "detected_text": self.detected_text, "lat": num(self.lat), "lon": num(self.lon),
            "place": self.place, "resolved_for": self.resolved_for,
            "fahrenheit": self.fahrenheit, "clock24": self.clock24, "seconds": self.seconds,
            "details": self.details, "background": crate::bg::index_to_id(self.background),
            "fps_cap": self.fps, "brightness": self.brightness, "movement": self.movement,
            "monitors": self.monitors, "refresh_min": self.refresh_min, "font": self.font,
            "contrast": self.contrast, "render_scale": self.render_scale, "transition": crate::render::TRANS_IDS.get(self.transition).copied().unwrap_or("blur"),
            "bg_options": self.bg_opts, "bg_presets": self.bg_presets,
        });
        let _ = std::fs::write(path(), serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    pub fn located(&self) -> bool {
        self.lat.is_finite() && self.lon.is_finite() && (self.auto_location || self.resolved_for == self.location)
    }
}

impl Config {
    /// Record a fresh auto-detected fix without clobbering settings the user may have just changed elsewhere.
    pub fn store_detected(lat: f64, lon: f64, place: &str, text: &str) {
        let mut c = Config::load();
        c.lat = lat;
        c.lon = lon;
        c.place = place.to_string();
        c.detected_text = text.to_string();
        c.save();
    }
}
