# wxscr — weather-consensus clock screensaver

A native Windows `.scr` (Rust + OpenGL, ~650 KB, no runtime, no API key). A replacement for the
Chromium-web-page screensaver + `dash2.html` setup.

- **Weather without keys**: averages NWS, MET Norway and nine Open-Meteo models (GFS, ECMWF, ICON, GEM, UKMO,
  Météo-France, JMA, HRRR, NBM) at "now" — the same consensus idea as `weather.html`, minus OpenWeatherMap.
- **Clearly labelled**: big *feels like* temperature and a smaller *dew point*, each with a caption. Optional line with
  high/low, rain chance, wind and next sunrise/sunset. All text is fully opaque.
- **Readable on anything**: pick how the text is kept legible over busy backgrounds: a soft shadow, a black halo that blurs out to nothing, or a thick black outline.
- **Transitions**: the screensaver fades in over whatever was on screen when it starts and fades back out to the live desktop when you touch something (the window's own transparency is animated, so the real desktop shows through). Choose fade, blur fade (the scene focuses in), zoom, none or random; picking one in settings replays it in the preview, and the *Fullscreen-size test* uses it too.
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
| `/bench [w h [frames [bg]]]` | time every background offscreen at w x h (default 1920x1080) and write ms/frame to `log.txt` (dev) |

Settings live in `%APPDATA%\wxscr\config.json`; the weather cache and a small `log.txt` are next to it.

## Performance

Several backgrounds were restructured so the GPU does not repeat work that is the same for every pixel (the pictures are unchanged; see
`THIRD_PARTY.md` for the per-file notes). Measured at 1920x1080 on an Intel HD 4600 (ms per frame, lower is better):

| background | before | after |
|---|---|---|
| Mac flurry | 49 | 10 |
| Ultra liquid bokeh | 117 | 15 |
| Ice and fire | 41 | 12.5 |
| Screensaver squares | 28 | 9.5 |

The heaviest ones are ray-marching shaders that really do a lot of work per pixel (Frozen flame ~150 ms, Perhaps a screensaver ~117 ms on
that GPU). If a background is too heavy for your GPU, set **Render resolution** in settings to 75% or 50%: the background is rendered
smaller and scaled up while the clock text stays sharp.

## When Windows starts the screensaver itself

Windows runs an idle-started screensaver on its own empty "Screen-saver" desktop, where there is nothing to fade over and from which a process cannot switch back to yours. So the saver does not stay there: it asks the Task Scheduler (a one-off on-demand task, `WeatherClockScreensaverHandoff`, running a copy at `%APPDATA%\wxscr\wxscr-handoff.exe`) to start a copy on your real desktop and exits at once; Windows switches back about a third of a second later and the copy fades in and out over your live windows, exactly as when started from the Test button. Only one saver runs at a time. If the hand-off fails, the saver falls back to staying on the empty desktop: it fades in from black and fades out to a picture of your desktop assembled from your windows (refreshed every minute). A side effect of the hand-off is that Windows sees the saver end right away, so "On resume, display logon screen" no longer locks the PC.

The saver keeps a small diagnostic log in `%APPDATA%\wxscr\saver.log` (window messages that can end a run, a timeline of the fade-out, frame stalls and a heartbeat every 5 minutes); attach it when reporting a problem that only happens after it has been running a long time.

## Notes

- Location is detected automatically on every launch: the Windows Location Service first, then your IP address (ipwho.is). Uncheck **Detect automatically** in settings to type an override (a US zip, `City, ST` or `lat,lon`); the box is pre-filled with the detected place.
- Bundled font: Droid Sans Mono (Apache-2.0), same as the original page. Any installed font name can be typed in settings.
- MET Norway asks API users to identify themselves; edit `UA` in `src/http.rs` with your contact details.
- Data: api.weather.gov, api.met.no, open-meteo.com (free for non-commercial use), zippopotam.us, ipwho.is.
