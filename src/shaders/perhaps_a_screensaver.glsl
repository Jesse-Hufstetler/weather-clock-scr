// "Perhaps a screensaver" by pirandello - https://www.shadertoy.com/view/Wfy3zh
// Fork of "Trailing the Twinkling Tunnelwisp" (CC0). Header of the original reads:
//   Volumetric tunnel shader (distance field raymarching). Gyroid-based, twisting tunnel with animated "wisp" lighting.
//   Based on the original by BeRo & Paul Karlik. Palette, wisp animation and customisation extensions by ChatGPT (2024),
//   modified with ChatGPT (2025). CC0/Public Domain.
// License: CC0 / Public Domain, as stated in the shader source.
// Only the Image pass is used here; the shader's Sound pass is deliberately not included.
// Changes: i, z, s, d, p, q are zero-initialised (WebGL zeroes locals, desktop GLSL does not), and in g() the
// argument-order-dependent `dot(sin(p *= s), cos(p.zxwy))` is split into two statements. tanh() comes from the host shim.

// mat2(<swizzled vec4 expression>) crashes the compiler of some GPU drivers; spell the components out.
mat2 wx_mat2(vec4 q) { return mat2(q.x, q.y, q.z, q.w); }

float g(vec4 p, float s) {
    p *= s;
    return abs(dot(sin(p), cos(p.zxwy)) - 1.) / s;
}

void mainImage(out vec4 O, vec2 C) {
    float i = 0., d = 0., z = 0., s = 0., T = iTime;
    vec4 o = vec4(0), q = vec4(0), p = vec4(0), U = vec4(2, 1, 0, 3);

    for (
        vec2 r = iResolution.xy;
        ++i < 79.;
        z += d + 5E-4,
        q = vec4(normalize(vec3((C + C - r) / r.y, 2)) * z, .2),
        q.z += T / 3E1,
        s = q.y + .1,
        q.y = abs(s),
        p = q,
        p.y -= .11,
        p.xy *= wx_mat2(cos(11. * U.zywz - 2. * p.z)),
        p.y -= .2,
        d = abs(g(p, 8.) - g(p, 24.)) / 4.,
        //Palette Color
        //p = 1.3 + 1.2 * cos(vec4(2.1, 4.5, 1.7, 0.0) + 5.5 * q.z)
        //p = 1.0 + 1.2 * cos(vec4(2.6, 5.0, 3.2, 0.0) + 5.1 * q.z)
        p = 1.4 + 1.8 * cos(vec4(1.8, 3.1, 4.5, 0.0) + 7.0 * q.z)
        //p = 1.2 + 0.8 * cos(vec4(0.7, 2.8, 4.7, 0.0) + 3.4 * q.z)
        //p = 1.18 + cos(vec4(3.1, 5.2, 4.4, 0.0) + 3.6 * q.z)
        //p = 1.25 + cos(vec4(1.4, 2.1, 0.5, 0.0) + 3.2 * q.z)
    )
        // Glow accumulation (unchanged)
        o += (s > 0. ? 1. : .1) * p.w * p / max(s > 0. ? d : d * d * d, 5E-4);

    // --- Animated, color-shifting, moving tunnelwisp ---
    vec2 wispPos = 1.5 * vec2(cos(T * 0.7), sin(T * 0.9));
    float wispDist = length(q.xy - wispPos);
    vec3 wispColor = vec3(1.0, 0.8 + 0.2 * sin(T), 0.7 + 0.3 * cos(T * 1.3));
    o.xyz += (2.0 + sin(T * 2.0)) * 800.0 * wispColor / (wispDist + 0.4);

    // Tone mapping
    O = tanh(o / 1E5);
}
