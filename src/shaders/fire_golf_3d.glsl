// "3D Fire Golf" by @Xor, https://fragcoord.xyz/s/3uxnauw2 (written in FragCoord's GOLF shorthand; this is its GLSL conversion)
// Original: https://fragcoord.xyz/s/3zoe0vgo

void mainImage(out vec4 fragColor, vec2 fragCoord) {
    vec4 O = vec4(0.0);
    float z = 0.0; float d = 0.0;
    for(int _fc=0;_fc<70;_fc++)
    {
        // the original reads gl_FragCoord.rgb, whose z is 0.5 for a full-screen quad
        vec3 p = vec3(z * normalize(2.0*vec3(fragCoord, 0.5) - iResolution.xyy));
        p.z += 5.0+cos(iTime);
        vec4 cs = cos(iTime+p.y/4.0+vec4(0, 8, 5, 0));
        p.xz = p.xz * mat2(cs.x, cs.y, cs.z, cs.w); // built component by component (a driver crash workaround)
        d = 2.0; for(int _fc1=0;_fc1<6;_fc1++) { d/=.8;
            p += cos((p.yzx - vec3(iTime, 0.0, 0.0) * 8.0) * d + iTime) / d; }
        d = .01 + abs(length(p.xz) + p.y*.3 -1.0) / 9.0; z += d;
        O += (sin(p.y/2.0-vec4(0.0, 1.0, 2.0, 0.0)) + 1.1) / d;
    }
    O = tanh(O / 1e3);
    fragColor = O;
}
