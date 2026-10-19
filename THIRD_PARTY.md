# Third-party material

## Shaders (`src/shaders/`)

| File | Author | Source | License |
|---|---|---|---|
| `random_gallery.glsl` | SnoopethDuckDuck (fork of "starting point 8008") | https://www.shadertoy.com/view/stdSDf | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0** (attribution, non-commercial, share-alike) |
| `zippy_zaps.glsl` | SnoopethDuckDuck (golf help: Nguyen2007, bug) | https://www.shadertoy.com/view/XXyGzh | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**; small changes noted in the file header |

The original bubbles shader (in `src/bg.rs`) is the repo owner's own work, ported from `dash2.html`. Shaders from
Shadertoy keep their own licence; the NonCommercial and ShareAlike terms apply to those files, not to the rest of the code.

## Font

`assets/DroidSansMono.ttf` — Droid Sans Mono, Apache License 2.0.

### Added later

| File | Author | Source | License |
|---|---|---|---|
| `frozen_flame.glsl` | mrange | https://www.shadertoy.com/view/N3dXRI | **CC0** (public domain), per the comment in the shader; only change: zero-initialised locals |

All three Shadertoy shaders were retrieved on **2026-10-18** (code pasted from the shader pages by the repo owner).
The same author / license / source / retrieved / changes details are shown in the settings window under the preview,
and in full under **Credits…**.

## Weather data (fetched at runtime, nothing bundled)

| Service | Terms |
|---|---|
| NWS (api.weather.gov) | US government data, public domain |
| MET Norway (api.met.no) | CC BY 4.0 / NLOD 2.0 — credit "Data from MET Norway" |
| Open-Meteo (open-meteo.com) | CC BY 4.0; free for non-commercial use |
| Windows Location Service | device location (needs Settings > Privacy > Location on) |
| ipwho.is | IP-based location fallback |
| BigDataCloud (api.bigdatacloud.net) | reverse geocoding: names the detected place |
| zippopotam.us | resolves zip codes you type |
