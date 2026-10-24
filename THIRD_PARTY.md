# Third-party material

## Shaders (`src/shaders/`)

| File | Author | Source | License |
|---|---|---|---|
| `random_gallery.glsl` | SnoopethDuckDuck (fork of "starting point 8008") | https://www.shadertoy.com/view/stdSDf | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0** (attribution, non-commercial, share-alike) |
| `zippy_zaps.glsl` | SnoopethDuckDuck (golf help: Nguyen2007, bug) | https://www.shadertoy.com/view/XXyGzh | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**; small changes noted in the file header |

The default bubbles shader (`src/shaders/bubbles.glsl`, Shadertoy "SKEENSAVUH") is the repo owner's own work. Shaders from
Shadertoy keep their own licence; the NonCommercial and ShareAlike terms apply to those files, not to the rest of the code.

## Font

`assets/DroidSansMono.ttf` — Droid Sans Mono, Apache License 2.0.

### Added later

| File | Author | Source | License |
|---|---|---|---|
| `frozen_flame.glsl` | mrange | https://www.shadertoy.com/view/N3dXRI | **CC0** (public domain), per the comment in the shader; only change: zero-initialised locals |
| `perhaps_a_screensaver.glsl` | pirandello (fork of "Trailing the Twinkling Tunnelwisp"; original by BeRo & Paul Karlik) | https://www.shadertoy.com/view/Wfy3zh | **CC0 / public domain**, as stated in the shader. Image pass only; the shader's sound pass is not included. Retrieved 2026-10-18 |
| `hex_crystal.glsl` | ChunderFPV (uses hexagonal tiles by lomateron, https://www.shadertoy.com/view/MlXyDl) | https://www.shadertoy.com/view/ct2SWw | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**; unmodified (iMouse supplied as a constant). Retrieved 2026-10-18 |
| `spiral_journey/` (3 files) | ChunderFPV | https://www.shadertoy.com/view/mssSWr | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**. Mouse counter overlay (uses code from https://www.shadertoy.com/view/7tV3zK) omitted; the hosted background texture is replaced by generated noise. Retrieved 2026-10-18 |
| `screensaver_squares.glsl` | julianlumia (after the starfield tutorial by BigWings / The Art of Code, https://www.youtube.com/watch?v=rvDo9LvfoVE) | https://www.shadertoy.com/view/Wdcyz7 | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**; one local variable renamed. Retrieved 2026-10-18 |
| `rainbeam/` (2 files) | Inityx | https://www.shadertoy.com/view/flffWX | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**. Multipass; hosted noise textures replaced by generated noise; identifiers `blobs` and `saturate` renamed. Retrieved 2026-10-18 |
| `zooming_bokeh.glsl` | FreakZillA8 (a mash-up crediting https://www.shadertoy.com/view/XcKGzG, https://www.shadertoy.com/view/wlBGDG and https://www.shadertoy.com/view/4djSRW for the hash function) | https://www.shadertoy.com/view/NfSXRt | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**; unmodified. The upstream shaders it borrows from have their own authors and licenses, which I could not check. Retrieved 2026-10-18 |
| `ultra_liquid_bokeh.glsl` (+ `bg::bokeh_data` in `src/bg.rs`) | Weyland (derived from "Bubbles" by Inigo Quilez, 2013, https://www.shadertoy.com/view/4dl3zn) | https://www.shadertoy.com/view/MtdXzr | **CC BY-NC-SA 3.0 Unported**, stated in the shader header; restructured for speed: each bubble is drawn as an additively blended quad and its parameters (the original formulas, unchanged) are evaluated once per frame on the CPU, instead of every pixel looping over all bubbles. Retrieved 2026-10-18 |
| `ice_and_fire/` (3 files) | mattz (uses a palette adapted from https://www.shadertoy.com/view/ll2GD3, hashes from https://www.shadertoy.com/view/4djSRW and barycentric code adapted from https://www.shadertoy.com/view/lslXDf, as credited in the shader) | https://www.shadertoy.com/view/MdfBzl | **CC BY-NC-SA 3.0 Unported**, stated in the shader header. The jittered lattice points are computed once per frame into a small table (`points.glsl`, not in the original) and read back by the image pass, instead of per pixel; the formulas are unchanged. The snippets it borrows keep their own authors' licenses. Retrieved 2026-10-18 |
| `wavy_background.glsl` | gigaherz | https://www.shadertoy.com/view/ltGSWD | No license stated by the author → Shadertoy default, **CC BY-NC-SA 3.0**; unmodified. Retrieved 2026-10-18 |

All thirteen Shadertoy shaders were retrieved on **2026-10-18** (code pasted from the shader pages by the repo owner).
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


## Flurry (`src/flurry.rs`)

A Rust/OpenGL port of **Flurry-WebGL** by Roy Curtis (https://github.com/RoyCurtis/Flurry-WebGL), which is itself derived from **Flurry** by
Calum Robinson (https://github.com/calumr/flurry). The simulation, the particle texture generator and the ten built-in presets follow
Flurry-WebGL; changes are listed in `src/bg.rs` and in the file header. Flurry-WebGL's license, which says it is granted with Calum
Robinson's permission:

> **This license applies to the HTML/CSS/JS code that makes up Flurry-WebGL; however large parts of its codebase is derived/copied from the original code base by Calum Robinson. With his permission, I have licensed this work under MIT.**
> 
> The MIT License (MIT)
> 
> Copyright (c) 2014 Roy Adrian Curtis
> 
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
> 
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
> 
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.