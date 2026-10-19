# wxscr — weather-consensus clock screensaver

A native Windows `.scr` (Rust + OpenGL, ~650 KB, no runtime, no API key). A replacement for the
Chromium-web-page screensaver + `dash2.html` setup.

- **Weather without keys**: averages NWS, MET Norway and nine Open-Meteo models (GFS, ECMWF, ICON, GEM, UKMO,
  Météo-France, JMA, HRRR, NBM) at "now" — the same consensus idea as `weather.html`, minus OpenWeatherMap.
- **Clearly labelled**: big *feels like* temperature and a smaller *dew point*, each with a caption. Optional line with
  high/low, rain chance, wind and next sunrise/sunset. All text is fully opaque.
- **Readable on anything**: pick how the text is kept legible over busy backgrounds: a soft shadow, a black halo that blurs out to nothing, or a thick black outline.
- **Transitions**: the screensaver dissolves out of whatever was on screen when it starts, and back into the real screen when you touch something (it grabs a screenshot just before appearing and blends with it both ways). Choose fade, blur fade, zoom, none or random; picking one in settings replays it in the preview, and the *Fullscreen-size test* uses it too.
- **Backgrounds**: your own *SKEENSAVUH* bubbles shader (the default) plus thirteen other Shadertoy shaders (Random gallery, Frozen flame cavern, Zippy zaps, Perhaps a screensaver, Hex crystal, Mac flurry, Spiral journey, Screensaver squares, Rainbeam, Zooming bokeh, Ultra liquid bokeh, Ice and fire, Wavy background), or random. Multipass shaders with feedback buffers (Mac flurry, Spiral journey, Rainbeam) are supported by a small Shadertoy-style engine. Author, license, source, retrieval date and changes show in settings under the preview, with a full list under **Credits…**.
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
| `/w` | full-size test run on the primary monitor (quits on mouse move or key, like the saver) |
| `/snap <prefix> [w h [bg]]` | render backgrounds offscreen to `<prefix>-N.png` (dev) |

Settings live in `%APPDATA%\wxscr\config.json`; the weather cache and a small `log.txt` are next to it.

## Known issues

- A faint black flash can still show along the taskbar when the screensaver starts or stops (the shell reshuffles the taskbar around the full-screen window).
- The transition back to the desktop blends with a screenshot taken when the screensaver started, so changes on screen in the meantime pop at the end.

## Notes

- Location is detected automatically on every launch: the Windows Location Service first, then your IP address (ipwho.is). Uncheck **Detect automatically** in settings to type an override (a US zip, `City, ST` or `lat,lon`); the box is pre-filled with the detected place.
- Bundled font: Droid Sans Mono (Apache-2.0), same as the original page. Any installed font name can be typed in settings.
- MET Norway asks API users to identify themselves; edit `UA` in `src/http.rs` with your contact details.
- Data: api.weather.gov, api.met.no, open-meteo.com (free for non-commercial use), zippopotam.us, ipwho.is.
