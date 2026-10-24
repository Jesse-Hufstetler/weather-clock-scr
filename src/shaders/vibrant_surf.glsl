// "Vibrant Surf Port" by @Frostbyte, https://fragcoord.xyz/s/vt4k0465
// SPDX-License-Identifier: CC-BY-NC-SA-4.0
// Copyright (c) 2026 @Frostbyte
//[LICENSE] https://creativecommons.org/licenses/by-nc-sa/4.0/

//Old Shader updated and ported from Shadertoy: https://www.shadertoy.com/view/wfdGzH
/*
    Fork from Diatribes: https://www.shadertoy.com/view/w3SSWd
    Inspired by Xor's raymarchers
    https://www.shadertoy.com/view/tXlXDX
*/
// (the audio reactivity of the original is dropped: the audio texture reads as silence, 0)

void mainImage(out vec4 fragColor, vec2 u) {
    float i = 0.0, d = 0.0, s = 0.0, t = iTime;
    for(fragColor = vec4(0.0); i++<1e2; ) {
        vec3 p = d * normalize(vec3(u+u,0) - iResolution.xyy );
        float a = 0.11 * t + p.z * 0.55;
        mat2 rot = mat2(cos(a), sin(a), -sin(a), cos(a));
        p.xy = p.xy * rot;

        p.z -= t;

        for (s = .1; s < 3.;
            p -= dot(cos(t+p * s* 16.), vec3(.01)) / s,
            p.xy += sin(p.yx*rot*.9+p.z*.1)*.15,
            s *= 1.42);
        d += s = .02 + abs(3.-length(p.yx))*.1;
        fragColor += (1.+cos(d+vec4(.8,8.+i*.1,4,0))) / s;
    }
    fragColor = 2.*tanh(fragColor / 6e3);
}
