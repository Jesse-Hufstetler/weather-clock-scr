// "fun zooming bokeh screensaver!" by FreakZillA8 - https://www.shadertoy.com/view/NfSXRt (2026-04-25)
// A mash-up; the author credits https://www.shadertoy.com/view/XcKGzG, https://www.shadertoy.com/view/wlBGDG and
// https://www.shadertoy.com/view/4djSRW (the hash function). Their own licences apply to what was taken from them.
// License of this shader: none stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// Included unmodified; the author's own header follows.

// i'm noob, mashed up shader
// credit to:
// https://www.shadertoy.com/view/XcKGzG
// https://www.shadertoy.com/view/wlBGDG
// https://www.shadertoy.com/view/4djSRW
// there was some other stuff I looked at but I forget where that was, sorry if I forgot to credit you! it's my first time I'll be more careful next time

// CODE STARTS HERE!
// random values hash function (https://www.shadertoy.com/view/4djSRW)
vec3 hash32(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * vec3(.1031, .1030, .0973));
    p3 += dot(p3, p3.yxz+33.33);
    return fract((p3.xxy+p3.yzz)*p3.zyx);
}

mat2 rot(float angle) {
    return mat2(cos(angle), sin(angle), -sin(angle), cos(angle));
}

#define LAYERS 16.0

void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    vec2 w = iResolution.xy;
    vec2 f2xyG = fragCoord / w - vec2(0.5);

    vec2 o = f2xyG * 2.0;
    float e = length(o);
    float k = o.y + o.x;

    // aspect ratio CORRECTION!
    f2xyG.y /= w.x / w.y;
    f2xyG *= 2.0;

    vec3 f3TotalCol = vec3(0.0);

    for(float i = 0.0; i < LAYERS; ++i) {
        vec2 f2xy = f2xyG;

        // random offset
        f2xy = f2xy * rot(i * 2.0) + vec2(0.03, 0.04) * i;

        // starfield style zoomer
        float fDuration = LAYERS;
        float fTick = mod(iTime * 0.5 + i, fDuration);
        float fDistance = (fDuration - fTick) * 0.25;
        float fVisualSize = 1.0 / fDistance;
        f2xy /= fVisualSize;

        float fLumaFactor = 0.5 / (fDistance * fDistance);

        // fade in bubbles
        float fFadeIn = smoothstep(0.0, 1.5, fTick);
        fLumaFactor *= fFadeIn;

        float allscale = 1.0; // (this stuff from https://www.shadertoy.com/view/XcKGzG)
        f2xy *= allscale;
        vec2 f2Id = floor(f2xy + allscale);
        f2xy = fract(f2xy);
        f2xy = f2xy * 2.0 - 1.0;

        // random noise
        vec3 f3Noise = 2.0 * hash32(f2Id * 0.5 / allscale + i * 0.01) - 1.0; // maybe "all scale" isn't a great variable name
        vec2 f2Offset = f3Noise.xy * 0.8;
        f2xy += f2Offset;

        vec2 f2ScaleLimit = vec2(1.0) - abs(f2Offset);
        float fMaxScale = min(f2ScaleLimit.x, f2ScaleLimit.y);
        float fScale = mix(0.2, fMaxScale, f3Noise.z * 0.5 + 0.5);
        f2xy /= fScale;

        // bubble shape (from https://www.shadertoy.com/view/wlBGDG)
        float l = length(f2xy);
        float r = fract(abs(f3Noise.x) * 9.7) * 0.8 + 0.2; // RANDOM RADIUS TWEAKER!

        float ring = pow(max(1.0 - abs(l - r + e * 0.2), 0.0), 25.0) * 0.2;
        float filled = smoothstep(r, r - e * 0.2, l);
        float fSignal = ring + filled;
        fSignal = clamp(fSignal, 0.0, 1.0);

        // bubble color (once again from https://www.shadertoy.com/view/wlBGDG)
        float colorMix = fract((i / LAYERS) + f3Noise.y);
        vec3 baseColor = mix(vec3(0.6, 0.46, 0.4), vec3(0.25, 0.15, 0.3) + vec3(0.0, k, k) * 0.25, colorMix);

        f3TotalCol += fSignal * pow(baseColor, vec3(3.0)) * fLumaFactor;
    }

    fragColor = vec4(sqrt(f3TotalCol) * 2.0, 1.0);
}
