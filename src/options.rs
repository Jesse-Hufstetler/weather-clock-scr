//! Per-background options and presets. A background that has options declares them as data (`Options`); the settings
//! window builds its options panel from that, and the config stores the chosen values under the background's id.
//! Backgrounds without options need nothing here.

use crate::config::Config;
use std::collections::BTreeMap;

/// Parameter values by key. Anything missing falls back to the parameter's default.
pub type Values = BTreeMap<String, f64>;

pub enum Kind {
    Float(f64, f64),
    Int(i32, i32),
    Bool,
    /// Stored as 0xRRGGBB.
    Color,
    /// Index into the list.
    Choice(&'static [&'static str]),
}

pub struct Param {
    pub key: &'static str,
    pub label: &'static str,
    /// Parameters with the same group name are listed together under that heading.
    pub group: &'static str,
    pub kind: Kind,
    pub default: f64,
}

pub struct Preset {
    pub name: &'static str,
    /// Only the values that differ from the defaults.
    pub values: &'static [(&'static str, f64)],
}

pub struct Options {
    pub title: &'static str,
    pub params: &'static [Param],
    pub presets: &'static [Preset],
}

/// The options of background `idx`, if it has any.
pub fn for_bg(idx: usize) -> Option<&'static Options> {
    match idx {
        crate::bg::FLURRY => Some(&crate::flurry::OPTIONS),
        _ => None,
    }
}

impl Param {
    pub fn range(&self) -> (f64, f64) {
        match self.kind {
            Kind::Float(lo, hi) => (lo, hi),
            Kind::Int(lo, hi) => (lo as f64, hi as f64),
            Kind::Bool => (0.0, 1.0),
            Kind::Color => (0.0, 16_777_215.0),
            Kind::Choice(c) => (0.0, (c.len() - 1) as f64),
        }
    }

    pub fn clamp(&self, v: f64) -> f64 {
        let (lo, hi) = self.range();
        let v = if v.is_finite() { v } else { self.default };
        match self.kind {
            Kind::Int(..) | Kind::Choice(_) | Kind::Bool | Kind::Color => v.round().clamp(lo, hi),
            Kind::Float(..) => v.clamp(lo, hi),
        }
    }
}

impl Options {
    /// Every parameter, taken from `v` where present (clamped to its range) and from the default otherwise.
    pub fn resolve(&self, v: &Values) -> Values {
        self.params.iter().map(|p| (p.key.to_string(), p.clamp(v.get(p.key).copied().unwrap_or(p.default)))).collect()
    }

    pub fn defaults(&self) -> Values {
        self.resolve(&Values::new())
    }

    pub fn preset_values(&self, p: &Preset) -> Values {
        let mut v = self.defaults();
        for (k, x) in p.values {
            if let Some(param) = self.params.iter().find(|q| q.key == *k) {
                v.insert(k.to_string(), param.clamp(*x));
            }
        }
        v
    }
}

/// The options values in effect for background `idx` (all keys filled in), or empty if it has none.
pub fn values_for(cfg: &Config, idx: usize) -> Values {
    match for_bg(idx) {
        Some(o) => o.resolve(cfg.bg_opts.get(crate::bg::index_to_id(idx)).unwrap_or(&Values::new())),
        None => Values::new(),
    }
}

/// Whether two value sets are the same for every parameter of `o` (floats compared to display precision).
pub fn same(o: &Options, a: &Values, b: &Values) -> bool {
    let (a, b) = (o.resolve(a), o.resolve(b));
    o.params.iter().all(|p| (a[p.key] - b[p.key]).abs() < 1e-6 * (1.0 + a[p.key].abs()))
}
