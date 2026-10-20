// "mac flurry screensaver" by ufffd - https://www.shadertoy.com/view/lX2SDt (CC BY-NC-SA 3.0 default)
// Noise-field pass: NOT part of the original. The original evaluates ~25 simplex-noise calls per pixel in each of buffers
// A, B and C (and 14 more in buffer D), but every one of those noise values varies extremely slowly across the screen (the
// noise is sampled at 0.15 units per screen-height). Here they are computed once per frame on a tiny grid (GW x GH texels
// per "panel") and the full-resolution passes just read them back with bilinear filtering (see nf() in the Common code).
// The expressions are the original ones, evaluated at the grid position.
//
// Panels (each GW wide, side by side):
//   0..2  direction noise for the tails of buffers A, B, C   (.rgba = the four channels; the pass multiplies by TAU)
//   3..5  spacing noise for the tails of buffers A, B, C
//   6     the warp that nudges every lookup (.rg)
//   7     colour constants of buffer D: .r = rootcol.r, .g = rootcol.g
//   8..10 colour constants of buffer D: .rgba = hue noise for j = 0..3, one panel per i = 0..2

vec4 tailAng(vec2 uvc, float offset) {
    uvc += coffset;
    float cdist = length(uvc);
    return vec4(
        fbm(vec3(uvc*nsize, offset +  0. + iTime*.02 + cdist*.1), 3),
        fbm(vec3(uvc*nsize, offset + 20. + iTime*.02 + cdist*.1), 3),
        fbm(vec3(uvc*nsize, offset + 40. + iTime*.02 + cdist*.1), 3),
        fbm(vec3(uvc*nsize, offset + 60. + iTime*.02 + cdist*.1), 3));
}

vec4 tailDist(vec2 uvc, float offset) {
    uvc += coffset;
    float cdist = length(uvc);
    return vec4(
        fbm(vec3(uvc*nsize, offset + 33. +  0. + iTime*.2 - cdist*.1), 2),
        fbm(vec3(uvc*nsize, offset + 33. + 20. + iTime*.2 - cdist*.1), 2),
        fbm(vec3(uvc*nsize, offset + 33. + 40. + iTime*.2 - cdist*.1), 2),
        fbm(vec3(uvc*nsize, offset + 33. + 60. + iTime*.2 - cdist*.1), 2));
}

float tailOffset(float tail) { return tail < 0.5 ? 1. : (tail < 1.5 ? 10. : 20.); }

void mainImage( out vec4 O, in vec2 xy ) {
    float panel = floor(xy.x / GW);
    vec2 uv = vec2(xy.x - panel*GW + 0.5, xy.y + 0.5) / vec2(GW, GH);
    vec2 uvc = (uv - 0.5) * vec2(wx_aspect, 1.0);
    O = vec4(0.);
    if (panel < 2.5) {
        O = tailAng(uvc, tailOffset(panel));
    } else if (panel < 5.5) {
        O = tailDist(uvc, tailOffset(panel - 3.));
    } else if (panel < 6.5) {
        O.rg = vec2(snoise(vec3(uvc*nsize, 99.+iTime*.1)), snoise(vec3(uvc*nsize, 77.+iTime*.1)));
    } else if (panel < 7.5) {
        O.r = snoise(vec3(10.,10.,iTime*.1));
        O.g = .6 + snoise(vec3(1.5,1.5,iTime*.1))*.2;
    } else {
        float i = panel - 8.;
        O = vec4(
            snoise(vec3(i*10.,  0., iTime*.1)),
            snoise(vec3(i*10., 10., iTime*.1)),
            snoise(vec3(i*10., 20., iTime*.1)),
            snoise(vec3(i*10., 30., iTime*.1)));
    }
}
