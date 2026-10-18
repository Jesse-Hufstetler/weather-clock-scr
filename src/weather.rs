//! Keyless multi-source weather consensus: NWS + MET Norway + nine Open-Meteo models,
//! averaged at "now". Same idea as weather.html, minus OpenWeatherMap.
//! All values are kept in °F / mph internally.

use crate::http::get_json;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default)]
pub struct Weather {
    pub place: String,
    pub temp: f64,
    pub feels: f64,
    pub dew: f64,
    pub wind: Option<f64>,
    pub pop: Option<f64>,
    pub hi: Option<f64>,
    pub lo: Option<f64>,
    pub sunrise: Vec<i64>,
    pub sunset: Vec<i64>,
    pub utc_offset: i64,
    pub fetched: i64,
    pub sources: usize,
}

pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

// ───────────── unit + weather math (ported from weather-data.js) ─────────────
fn ctof(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}
fn kmh(k: f64) -> f64 {
    k * 0.621371
}
fn ms(m: f64) -> f64 {
    m * 2.236936
}
fn rh_pct(t_f: f64, d_f: f64) -> f64 {
    let (c, d, a, b) = ((t_f - 32.0) * 5.0 / 9.0, (d_f - 32.0) * 5.0 / 9.0, 17.62, 243.12);
    (100.0 * (a * d / (b + d) - a * c / (b + c)).exp()).min(100.0)
}
/// Wind chill when cold and breezy, heat index when hot, else air temperature.
pub fn feels_like_f(t: f64, dew: f64, wind: f64) -> f64 {
    if t <= 50.0 && wind > 3.0 {
        let v = wind.powf(0.16);
        return 35.74 + 0.6215 * t - 35.75 * v + 0.4275 * t * v;
    }
    if t < 80.0 {
        return t;
    }
    let rh = rh_pct(t, dew);
    let mut hi = -42.379 + 2.04901523 * t + 10.14333127 * rh - 0.22475541 * t * rh - 0.00683783 * t * t
        - 0.05481717 * rh * rh
        + 0.00122874 * t * t * rh
        + 0.00085282 * t * rh * rh
        - 0.00000199 * t * t * rh * rh;
    if rh < 13.0 && t <= 112.0 {
        hi -= (13.0 - rh) / 4.0 * ((17.0 - (t - 95.0).abs()) / 17.0).sqrt();
    } else if rh > 85.0 && t <= 87.0 {
        hi += (rh - 85.0) / 10.0 * ((87.0 - t) / 5.0);
    }
    hi
}

// ───────────── time parsing ─────────────
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// "2026-10-18T19:00:00+00:00" / "...Z" / "2026-10-18T19:00" -> unix seconds.
fn iso_unix(s: &str) -> Option<i64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d, h, mi) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?);
    let sec = if s.get(16..17) == Some(":") { n(17, 19)? } else { 0 };
    let mut t = days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec;
    let tail = s.get(19.min(s.len())..).unwrap_or("");
    if let Some(sign) = tail.chars().find(|c| *c == '+' || *c == '-') {
        let i = tail.find(sign).unwrap();
        let (oh, om) = (tail.get(i + 1..i + 3).and_then(|x| x.parse::<i64>().ok())?, tail.get(i + 4..i + 6).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0));
        let off = (oh * 3600 + om * 60) * if sign == '-' { -1 } else { 1 };
        t -= off;
    }
    Some(t)
}

/// "PT6H", "P1DT6H", "PT30M" -> seconds.
fn iso_dur(d: &str) -> i64 {
    let (mut total, mut num, mut in_time) = (0i64, 0i64, false);
    for c in d.chars().skip(1) {
        match c {
            'T' => in_time = true,
            '0'..='9' => num = num * 10 + c.to_digit(10).unwrap() as i64,
            'D' => {
                total += num * 86400;
                num = 0
            }
            'H' => {
                total += num * 3600;
                num = 0
            }
            'M' if in_time => {
                total += num * 60;
                num = 0
            }
            _ => num = 0,
        }
    }
    total
}

// ───────────── series ─────────────
type Pts = Vec<(i64, f64)>;
struct Series {
    src: String,
    metric: &'static str,
    pts: Pts,
}

fn value_at(pts: &Pts, now: i64) -> Option<f64> {
    let first = pts.first()?;
    let last = pts.last()?;
    if now < first.0 {
        return (first.0 - now <= 7200).then_some(first.1);
    }
    if now >= last.0 {
        return (now - last.0 <= 7200).then_some(last.1);
    }
    let i = pts.partition_point(|p| p.0 <= now);
    let (a, b) = (pts[i - 1], pts[i]);
    let f = (now - a.0) as f64 / (b.0 - a.0).max(1) as f64;
    Some(a.1 + (b.1 - a.1) * f)
}

fn keep(out: &mut Vec<Series>, src: &str, metric: &'static str, mut pts: Pts, now: i64) {
    pts.retain(|p| p.0 > now - 36 * 3600 && p.0 < now + 40 * 3600);
    pts.sort_by_key(|p| p.0);
    if !pts.is_empty() {
        out.push(Series { src: src.to_string(), metric, pts });
    }
}

// ───────────── sources ─────────────
fn fetch_nws(lat: f64, lon: f64, now: i64) -> Result<Vec<Series>, String> {
    let pt = get_json(&format!("https://api.weather.gov/points/{lat:.4},{lon:.4}"))?;
    let url = pt["properties"]["forecastGridData"].as_str().ok_or("NWS: no grid")?.to_string();
    let p = get_json(&url)?["properties"].clone();
    let mut out = Vec::new();
    let mut add = |key: &str, metric: &'static str, conv: &dyn Fn(f64) -> f64| {
        let mut pts = Pts::new();
        for x in p[key]["values"].as_array().into_iter().flatten() {
            let Some(v) = x["value"].as_f64() else { continue };
            let Some((t0, dur)) = x["validTime"].as_str().and_then(|s| s.split_once('/')) else { continue };
            let Some(start) = iso_unix(t0) else { continue };
            let hours = (iso_dur(dur) / 3600).clamp(1, 48);
            for h in 0..hours {
                pts.push((start + h * 3600, conv(v)));
            }
        }
        keep(&mut out, "nws", metric, pts, now);
    };
    add("temperature", "temp", &ctof);
    add("apparentTemperature", "feels", &ctof);
    add("dewpoint", "dew", &ctof);
    add("windSpeed", "wind", &kmh);
    add("probabilityOfPrecipitation", "pop", &|v| v);
    Ok(out)
}

fn fetch_metno(lat: f64, lon: f64, now: i64) -> Result<Vec<Series>, String> {
    let j = get_json(&format!("https://api.met.no/weatherapi/locationforecast/2.0/complete?lat={lat:.4}&lon={lon:.4}"))?;
    let ts = j["properties"]["timeseries"].as_array().ok_or("MET: no data")?;
    let mut out = Vec::new();
    let inst = |field: &str, conv: &dyn Fn(f64) -> f64| -> Pts {
        ts.iter()
            .filter_map(|t| Some((iso_unix(t["time"].as_str()?)?, conv(t["data"]["instant"]["details"][field].as_f64()?))))
            .collect()
    };
    let temp = inst("air_temperature", &ctof);
    let dew = inst("dew_point_temperature", &ctof);
    let wind = inst("wind_speed", &ms);
    // MET.no has no apparent temperature: derive it with the same formula the page uses.
    let feels: Pts = temp
        .iter()
        .filter_map(|&(t, tv)| {
            let d = dew.iter().find(|p| p.0 == t)?.1;
            let w = wind.iter().find(|p| p.0 == t)?.1;
            Some((t, feels_like_f(tv, d, w)))
        })
        .collect();
    let pop: Pts = ts
        .iter()
        .filter_map(|t| {
            let b = &t["data"]["next_1_hours"]["details"]["probability_of_precipitation"];
            let b = if b.is_null() { &t["data"]["next_6_hours"]["details"]["probability_of_precipitation"] } else { b };
            Some((iso_unix(t["time"].as_str()?)?, b.as_f64()?))
        })
        .collect();
    keep(&mut out, "metno", "temp", temp, now);
    keep(&mut out, "metno", "dew", dew, now);
    keep(&mut out, "metno", "wind", wind, now);
    keep(&mut out, "metno", "feels", feels, now);
    keep(&mut out, "metno", "pop", pop, now);
    Ok(out)
}

struct Om {
    series: Vec<Series>,
    utc_offset: i64,
    sunrise: Vec<i64>,
    sunset: Vec<i64>,
}

fn fetch_open_meteo(lat: f64, lon: f64, now: i64) -> Result<Om, String> {
    const MODELS: [&str; 9] = [
        "gfs_seamless", "ecmwf_ifs025", "icon_seamless", "gem_seamless", "ukmo_seamless",
        "meteofrance_seamless", "jma_seamless", "gfs_hrrr", "ncep_nbm_conus",
    ];
    const VARS: [(&str, &str); 5] = [
        ("temperature_2m", "temp"), ("apparent_temperature", "feels"), ("dew_point_2m", "dew"),
        ("wind_speed_10m", "wind"), ("precipitation_probability", "pop"),
    ];
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}&hourly={}&daily=sunrise,sunset&models={}\
         &temperature_unit=fahrenheit&wind_speed_unit=mph&timeformat=unixtime&forecast_days=2&timezone=auto",
        VARS.map(|v| v.0).join(","),
        MODELS.join(",")
    );
    let j = get_json(&url)?;
    let times: Vec<i64> = j["hourly"]["time"].as_array().ok_or("Open-Meteo: no data")?.iter().filter_map(|t| t.as_i64()).collect();
    let mut series = Vec::new();
    for model in MODELS {
        for (api, metric) in VARS {
            let Some(arr) = j["hourly"][format!("{api}_{model}")].as_array() else { continue };
            let pts: Pts = arr.iter().enumerate().filter_map(|(i, v)| Some((*times.get(i)?, v.as_f64()?))).collect();
            keep(&mut series, model, metric, pts, now);
        }
    }
    let day = |key: &str| -> Vec<i64> {
        let a = if j["daily"][format!("{key}_gfs_seamless")].is_array() { &j["daily"][format!("{key}_gfs_seamless")] } else { &j["daily"][key] };
        a.as_array().into_iter().flatten().filter_map(|v| v.as_i64()).collect()
    };
    Ok(Om { series, utc_offset: j["utc_offset_seconds"].as_i64().unwrap_or(0), sunrise: day("sunrise"), sunset: day("sunset") })
}

// ───────────── consensus ─────────────
fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

pub fn fetch(lat: f64, lon: f64, place: &str) -> Result<Weather, String> {
    let now = now_unix();
    let (a, b, c) = std::thread::scope(|s| {
        let nws = s.spawn(|| fetch_nws(lat, lon, now));
        let met = s.spawn(|| fetch_metno(lat, lon, now));
        let om = s.spawn(|| fetch_open_meteo(lat, lon, now));
        (nws.join(), met.join(), om.join())
    });
    let mut series = Vec::new();
    let mut errs = Vec::new();
    let mut take = |r: Result<Result<Vec<Series>, String>, _>| match r {
        Ok(Ok(v)) => series.extend(v),
        Ok(Err(e)) => errs.push(e),
        Err(_) => errs.push("source panicked".to_string()),
    };
    take(a);
    take(b);
    let (mut off, mut sunrise, mut sunset) = (0, vec![], vec![]);
    match c {
        Ok(Ok(om)) => {
            off = om.utc_offset;
            sunrise = om.sunrise;
            sunset = om.sunset;
            series.extend(om.series);
        }
        Ok(Err(e)) => errs.push(e),
        Err(_) => errs.push("source panicked".into()),
    }

    let now_vals = |metric: &str| -> Vec<f64> { series.iter().filter(|s| s.metric == metric).filter_map(|s| value_at(&s.pts, now)).collect() };
    let temp = mean(&now_vals("temp"));
    let dew = mean(&now_vals("dew"));
    let wind = mean(&now_vals("wind"));
    let (Some(temp), Some(dew)) = (temp, dew) else {
        return Err(format!("no forecast data ({})", errs.join("; ")));
    };
    let feels = mean(&now_vals("feels")).unwrap_or_else(|| feels_like_f(temp, dew, wind.unwrap_or(0.0)));

    // today's high/low in the forecast location's own day, from sources that cover (nearly) the whole day
    let day0 = (now + off).div_euclid(86400) * 86400 - off;
    let (mut his, mut los) = (vec![], vec![]);
    for s in series.iter().filter(|s| s.metric == "temp") {
        let today: Vec<f64> = s.pts.iter().filter(|p| p.0 >= day0 && p.0 < day0 + 86400).map(|p| p.1).collect();
        if today.len() >= 20 {
            his.push(today.iter().cloned().fold(f64::MIN, f64::max));
            los.push(today.iter().cloned().fold(f64::MAX, f64::min));
        }
    }
    // rain chance: each source's peak over the next 12 hours, averaged
    let pops: Vec<f64> = series
        .iter()
        .filter(|s| s.metric == "pop")
        .filter_map(|s| s.pts.iter().filter(|p| p.0 >= now - 1800 && p.0 <= now + 12 * 3600).map(|p| p.1).fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v)))))
        .collect();
    let sources = {
        let mut names: Vec<&String> = series.iter().map(|s| &s.src).collect();
        names.sort();
        names.dedup();
        names.len()
    };
    Ok(Weather {
        place: place.to_string(),
        temp,
        feels,
        dew,
        wind,
        pop: mean(&pops),
        hi: mean(&his),
        lo: mean(&los),
        sunrise,
        sunset,
        utc_offset: off,
        fetched: now,
        sources,
    })
}

// ───────────── location ─────────────
/// zip ("44720"), "lat,lon", "City, ST", or empty = locate by IP. Returns (lat, lon, display name).
pub fn geocode(text: &str) -> Result<(f64, f64, String), String> {
    let text = text.trim();
    if text.is_empty() {
        let j = get_json("https://ipwho.is/")?;
        let (lat, lon) = (j["latitude"].as_f64().ok_or("IP lookup failed")?, j["longitude"].as_f64().ok_or("IP lookup failed")?);
        let name = [j["city"].as_str().unwrap_or(""), j["region_code"].as_str().unwrap_or("")].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
        return Ok((lat, lon, name));
    }
    let parts: Vec<&str> = text.split(|c| c == ',' || c == ' ').filter(|s| !s.is_empty()).collect();
    if parts.len() == 2 {
        if let (Ok(a), Ok(b)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
            if a.abs() <= 90.0 && b.abs() <= 180.0 {
                return Ok((a, b, format!("{a:.3}, {b:.3}")));
            }
        }
    }
    if text.len() >= 5 && text.chars().take(5).all(|c| c.is_ascii_digit()) && (text.len() == 5 || text.as_bytes()[5] == b'-') {
        let j = get_json(&format!("https://api.zippopotam.us/us/{}", &text[..5]))?;
        let p = &j["places"][0];
        let lat = p["latitude"].as_str().and_then(|s| s.parse().ok()).ok_or("zip not found")?;
        let lon = p["longitude"].as_str().and_then(|s| s.parse().ok()).ok_or("zip not found")?;
        return Ok((lat, lon, format!("{}, {}", p["place name"].as_str().unwrap_or(""), p["state abbreviation"].as_str().unwrap_or(""))));
    }
    let (first, rest) = text.split_once(',').map(|(a, b)| (a.trim(), b.trim().to_lowercase())).unwrap_or((text, String::new()));
    let enc: String = first.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    let j = get_json(&format!("https://geocoding-api.open-meteo.com/v1/search?name={enc}&count=10&language=en&format=json"))?;
    let res = j["results"].as_array().ok_or_else(|| format!("no place found for \"{text}\""))?;
    let pick = res
        .iter()
        .find(|r| !rest.is_empty() && ["admin1", "country", "country_code"].iter().any(|k| r[*k].as_str().map(|s| s.to_lowercase() == rest).unwrap_or(false)))
        .unwrap_or(&res[0]);
    let name = [pick["name"].as_str().unwrap_or(""), pick["admin1"].as_str().unwrap_or("")].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
    Ok((pick["latitude"].as_f64().ok_or("bad geocode")?, pick["longitude"].as_f64().ok_or("bad geocode")?, name))
}

// ───────────── cache (so the screensaver has numbers the instant it starts) ─────────────
fn cache_path() -> std::path::PathBuf {
    crate::config::dir().join("cache.json")
}

pub fn save_cache(w: &Weather) {
    let v = json!({
        "place": w.place, "temp": w.temp, "feels": w.feels, "dew": w.dew, "wind": w.wind, "pop": w.pop,
        "hi": w.hi, "lo": w.lo, "sunrise": w.sunrise, "sunset": w.sunset, "utc_offset": w.utc_offset,
        "fetched": w.fetched, "sources": w.sources,
    });
    let _ = std::fs::write(cache_path(), v.to_string());
}

pub fn load_cache() -> Option<Weather> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(cache_path()).ok()?).ok()?;
    let ints = |k: &str| -> Vec<i64> { v[k].as_array().into_iter().flatten().filter_map(|x| x.as_i64()).collect() };
    let w = Weather {
        place: v["place"].as_str()?.to_string(),
        temp: v["temp"].as_f64()?,
        feels: v["feels"].as_f64()?,
        dew: v["dew"].as_f64()?,
        wind: v["wind"].as_f64(),
        pop: v["pop"].as_f64(),
        hi: v["hi"].as_f64(),
        lo: v["lo"].as_f64(),
        sunrise: ints("sunrise"),
        sunset: ints("sunset"),
        utc_offset: v["utc_offset"].as_i64().unwrap_or(0),
        fetched: v["fetched"].as_i64()?,
        sources: v["sources"].as_u64().unwrap_or(0) as usize,
    };
    (now_unix() - w.fetched < 3 * 3600).then_some(w)
}

/// Placeholder numbers for the settings preview / snapshots when nothing has been fetched yet.
pub fn sample() -> Weather {
    let now = now_unix();
    let local_offset = unsafe {
        use windows_sys::Win32::System::Time::GetTimeZoneInformation;
        let mut tz = std::mem::zeroed();
        let id = GetTimeZoneInformation(&mut tz);
        -((tz.Bias + if id == 2 { tz.DaylightBias } else { 0 }) as i64) * 60
    };
    Weather {
        place: "Sample".into(),
        temp: 62.0,
        feels: 60.0,
        dew: 33.0,
        wind: Some(8.0),
        pop: Some(20.0),
        hi: Some(63.0),
        lo: Some(41.0),
        sunrise: vec![now + 40000],
        sunset: vec![now + 9000],
        utc_offset: local_offset,
        fetched: now,
        sources: 0,
    }
}
