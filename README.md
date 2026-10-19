# wxscr — weather-consensus clock screensaver

A native Windows `.scr` (Rust + OpenGL, ~650 KB, no runtime, no API key). A replacement for the
Chromium-web-page screensaver + `dash2.html` setup.

- **Weather without keys**: averages NWS, MET Norway and nine Open-Meteo models (GFS, ECMWF, ICON, GEM, UKMO,
  Météo-France, JMA, HRRR, NBM) at "now" — the same consensus idea as `weather.html`, minus OpenWeatherMap.
- **Clearly labelled**: big *feels like* temperature, dimmer *dew point*, each with a caption. Optional line with
  high/low, rain chance, wind and next sunrise/sunset.
- **Backgrounds**: your original bubbles shader (ported verbatim) plus twelve Shadertoy shaders (Random gallery, Frozen flame cavern, Zippy zaps, Perhaps a screensaver, Hex crystal, Mac flurry, Spiral journey, Screensaver squares, Rainbeam, Zooming bokeh, Ultra liquid bokeh, Ice and fire), or random. Multipass shaders with feedback buffers (Mac flurry, Spiral journey, Rainbeam) are supported by a small Shadertoy-style engine. Author, license, source, retrieval date and changes show in settings under the preview, with a full list under **Credits…**.
- **Smooth and light**: native resolution at your display's refresh rate (paced by vsync, so it idles the CPU: a few percent of one core here). Optional fps cap and a brightness slider for OLED; the text breathes and drifts slowly against burn-in.
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

- Location is detected automatically on every launch: the Windows Location Service first, then your IP address (ipwho.is). Uncheck **Detect automatically** in settings to type an override (a US zip, `City, ST` or `lat,lon`); the box is pre-filled with the detected place.
- Bundled font: Droid Sans Mono (Apache-2.0), same as the original page. Any installed font name can be typed in settings.
- MET Norway asks API users to identify themselves; edit `UA` in `src/http.rs` with your contact details.
- Data: api.weather.gov, api.met.no, open-meteo.com (free for non-commercial use), zippopotam.us, ipwho.is.
