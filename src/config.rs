//! Settings stored as JSON in %APPDATA%\wxscr\config.json (no registry, no key material needed).

use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    /// What the user typed: zip, "city, ST", "lat,lon", or empty for "locate me by IP".
    pub location: String,
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
    /// 0..=3 -> background render scale (see render.rs).
    pub quality: usize,
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            location: "44720".into(),
            lat: f64::NAN,
            lon: f64::NAN,
            place: String::new(),
            resolved_for: String::new(),
            fahrenheit: true,
            clock24: false,
            seconds: true,
            details: true,
            background: 0,
            quality: 1,
            fps: 30,
            brightness: 100,
            movement: 60,
            monitors: 0,
            refresh_min: 20,
            font: String::new(),
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
        c.location = st("location", &c.location);
        c.lat = v["lat"].as_f64().unwrap_or(f64::NAN);
        c.lon = v["lon"].as_f64().unwrap_or(f64::NAN);
        c.place = st("place", "");
        c.resolved_for = st("resolved_for", "");
        c.fahrenheit = bo("fahrenheit", c.fahrenheit);
        c.clock24 = bo("clock24", c.clock24);
        c.seconds = bo("seconds", c.seconds);
        c.details = bo("details", c.details);
        c.background = nu("background", c.background as u64) as usize;
        c.quality = (nu("quality", c.quality as u64) as usize).min(3);
        c.fps = nu("fps", c.fps as u64).clamp(10, 120) as u32;
        c.brightness = nu("brightness", c.brightness as u64).clamp(10, 100) as u32;
        c.movement = nu("movement", c.movement as u64).min(100) as u32;
        c.monitors = (nu("monitors", c.monitors as u64) as usize).min(2);
        c.refresh_min = nu("refresh_min", c.refresh_min as u64).clamp(5, 180) as u32;
        c.font = st("font", "");
        c
    }

    pub fn save(&self) {
        let num = |x: f64| if x.is_finite() { json!(x) } else { Value::Null };
        let v = json!({
            "location": self.location, "lat": num(self.lat), "lon": num(self.lon),
            "place": self.place, "resolved_for": self.resolved_for,
            "fahrenheit": self.fahrenheit, "clock24": self.clock24, "seconds": self.seconds,
            "details": self.details, "background": self.background, "quality": self.quality,
            "fps": self.fps, "brightness": self.brightness, "movement": self.movement,
            "monitors": self.monitors, "refresh_min": self.refresh_min, "font": self.font,
        });
        let _ = std::fs::write(path(), serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    pub fn located(&self) -> bool {
        self.lat.is_finite() && self.lon.is_finite() && self.resolved_for == self.location
    }
}
