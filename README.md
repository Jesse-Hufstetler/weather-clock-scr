# wxscr — weather-consensus clock screensaver

A native Windows `.scr` (Rust + OpenGL, ~650 KB, no runtime, no API key). A replacement for the
Chromium-web-page screensaver + `dash2.html` setup.

- **Weather without keys**: averages NWS, MET Norway and nine Open-Meteo models (GFS, ECMWF, ICON, GEM, UKMO,
  Météo-France, JMA, HRRR, NBM) at "now" — the same consensus idea as `weather.html`, minus OpenWeatherMap.
- **Clearly labelled**: big *feels like* temperature and a smaller *dew point*, each with a caption. Optional line with
  high/low, rain chance, wind and next sunrise/sunset. All text is fully opaque.
- **Readable on anything**: pick how the text is kept legible over busy backgrounds: a soft shadow, a black halo that blurs out to nothing, or a thick black outline.
- **Transitions**: the screensaver fades in over whatever was on screen when it starts and fades back out to the live desktop when you touch something (the window's own transparency is animated, so the real desktop shows through). Choose fade, blur fade (the scene focuses in), zoom, none or random; picking one in settings replays it in the preview, and the *Fullscreen-size test* uses it too.
- **Backgrounds**: your own *SKEENSAVUH* bubbles shader (the default) plus 27 other shaders from Shadertoy and fragcoord.xyz (Frozen flame cavern, Zippy zaps, Perhaps a screensaver, Hex crystal, Spiral journey, Screensaver squares, Rainbeam, Zooming bokeh, Ultra liquid bokeh, Ice and fire, Wavy background, Accretion, Surf 2, Ghosts, Shooting stars, Mini: Hillside, Morphing abstract, Disco maze, Painted, Split, Truchet tunnel, Rainbow travel, Traffic, Flare, 3D fire golf, Vibrant surf, Stellar, Disco hive), a native port of Flurry (Roy Curtis' Flurry-WebGL, after Calum Robinson's Flurry), or random. Multipass shaders with feedback buffers (Spiral journey, Rainbeam) are supported by a small Shadertoy-style engine. A **Background speed** slider (10-200%) in settings slows down or speeds up every background. Author, license, source, retrieval date and changes show in settings under the preview, with a full list under **Credits…**.
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

## Background options and presets

A background can have its own options. When the chosen one does, **Background options and presets…** under the Background box opens a
window with its sliders, choices and colours, plus a **Preset** box. Every change shows in the live preview straight away; **OK** keeps it,
**Cancel** puts everything back. To save your own preset, type a name in the Preset box and click **Save**; **Delete** removes one of yours
(the built-in presets stay). Values are stored per background under `bg_options` and `bg_presets` in `config.json`.

Flurry is the first background with options (its ten Flurry-WebGL presets are built in: Default, Nevada, Birds, Skittle Spray, Treasure,
Bokeh, Heaven Or Las Vegas, Garlands, Passionfruit, Spider). To give another background options, declare an `Options` (parameters with a
key, label, group, range and default, plus presets) as data, return it from `options::for_bg`, and read the values with
`options::values_for` where it renders. The window builds itself from that declaration.

## Performance

Several backgrounds were restructured so the GPU does not repeat work that is the same for every pixel (the pictures are unchanged; see
`THIRD_PARTY.md` for the per-file notes). Measured at 1920x1080 on an Intel HD 4600 (ms per frame, lower is better):

| background | before | after |
|---|---|---|
| Ultra liquid bokeh | 117 | 15 |
| Ice and fire | 41 | 12.5 |
| Screensaver squares | 28 | 9.5 |

The heaviest ones are ray-marching shaders that really do a lot of work per pixel (Frozen flame ~150 ms, Perhaps a screensaver ~117 ms on
that GPU). If a background is too heavy for your GPU, set **Render resolution** in settings to 75% or 50%: the background is rendered
smaller and scaled up while the clock text stays sharp.

## When Windows starts the screensaver itself

Windows runs an idle-started screensaver on its own empty "Screen-saver" desktop, where there is nothing to fade over and from which a process cannot switch back to yours. So the saver does not stay there: it asks the Task Scheduler to run an on-demand task, `WeatherClockScreensaverHandoff` (no schedule or trigger), which starts a helper copy of the saver, `wxscr-handoff.exe` (the Task Scheduler will not start a `.scr`), on your real desktop and exits at once; Windows switches back about a third of a second later and the copy fades in and out over your live windows, exactly as when started from the Test button. Only one saver runs at a time. The installer puts the helper in `C:\Program Files\Weather Clock` and registers the task for your account; the uninstaller removes both, and the saver itself never writes files elsewhere or registers anything (this keeps it from looking like malware to antivirus heuristics). If the hand-off is not installed (you used only the `.scr`) or fails, the saver falls back to staying on the empty desktop: it fades in from black and fades out to a picture of your desktop assembled from your windows (refreshed every minute). A side effect of the hand-off is that Windows sees the saver end right away, so "On resume, display logon screen" no longer locks the PC.

The saver keeps a small diagnostic log in `%APPDATA%\wxscr\saver.log` (window messages that can end a run, a timeline of the fade-out, frame stalls and a heartbeat every 5 minutes); attach it when reporting a problem that only happens after it has been running a long time.

## Notes

- Location is detected automatically on every launch: the Windows Location Service first, then your IP address (ipwho.is). Uncheck **Detect automatically** in settings to type an override (a US zip, `City, ST` or `lat,lon`); the box is pre-filled with the detected place.
- Bundled font: Droid Sans Mono (Apache-2.0), same as the original page. Any installed font name can be typed in settings.
- MET Norway asks API users to identify themselves; edit `UA` in `src/http.rs` with your contact details.
- Data: api.weather.gov, api.met.no, open-meteo.com (free for non-commercial use), zippopotam.us, ipwho.is.
