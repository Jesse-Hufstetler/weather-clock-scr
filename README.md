# wxscr — weather-consensus clock screensaver

A native Windows `.scr` (Rust + OpenGL, ~650 KB, no runtime, no API key). A replacement for the
Chromium-web-page screensaver + `dash2.html` setup.

- **Weather without keys**: averages NWS, MET Norway and nine Open-Meteo models (GFS, ECMWF, ICON, GEM, UKMO,
  Météo-France, JMA, HRRR, NBM) at "now" — the same consensus idea as `weather.html`, minus OpenWeatherMap.
- **Clearly labelled**: big *feels like* temperature, dimmer *dew point*, each with a caption. Optional line with
  high/low, rain chance, wind and next sunrise/sunset.
- **Backgrounds**: your original bubbles shader (ported verbatim), Silk ribbons, Aurora, Starstream (Flurry-ish), Plasma bloom, or random.
- **Cheap on power**: backgrounds render at reduced resolution into a half-float buffer, then are upscaled with
  dithering; 30 fps default; brightness slider for OLED; text breathes/drifts slowly against burn-in.
- Works across multiple monitors, per-monitor DPI aware, cached weather so numbers show instantly on start.

## Install

```
cargo build --release
copy target\release\wxscr.exe "Weather Clock.scr"
```

Right-click `Weather Clock.scr` → **Install**, then pick it in Screen Saver Settings → **Settings…**
(or just double-click it to open settings).

## Command line

| flag | |
|---|---|
| `/s` | run the screensaver |
| `/c` | settings dialog |
| `/p <hwnd>` | preview in the Windows settings thumbnail |
| `/w` | windowed test run (Esc to quit) |
| `/snap <prefix> [w h [bg]]` | render backgrounds offscreen to `<prefix>-N.png` (dev) |

Settings live in `%APPDATA%\wxscr\config.json`; the weather cache and a small `log.txt` are next to it.

## Notes

- Location accepts a US zip, `City, ST`, `lat,lon`, or empty for IP-based lookup (ipwho.is).
- Bundled font: Droid Sans Mono (Apache-2.0), same as the original page. Any installed font name can be typed in settings.
- MET Norway asks API users to identify themselves; edit `UA` in `src/http.rs` with your contact details.
- Data: api.weather.gov, api.met.no, open-meteo.com (free for non-commercial use), zippopotam.us, ipwho.is.
