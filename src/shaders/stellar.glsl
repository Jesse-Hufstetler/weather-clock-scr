// "Stellar" by @Xor, https://fragcoord.xyz/s/swl6w8ul
// (the original is one comma-expression loop body that assigns v inside an argument list; it is written out in order here)

void mainImage(out vec4 fragColor, vec2 fragCoord)
{
    fragColor = vec4(0.0);
    vec3 p = vec3(0.0), v = vec3(0.0);
    float i = 0.0, z = 0.0, d = 0.0, l = 0.0;
    for(; i++ < 1e2; fragColor += (cos(v.z + l * vec4(3, 2, 1, 0)) + 1.0) / d) {
        // the original reads gl_FragCoord.rgb, whose z is 0.5 for a full-screen quad
        p = z * (vec3(fragCoord, 0.5) * 2.0 - iResolution.xyy) / iResolution.y;
        p.z += 2.0;
        v = vec3(atan(p.x, p.z), atan(p.y, length(p.xz)), log(l = length(p))) * 8.0 + iTime;
        d = length(cos(v) + sin(v.yzx + v + iTime - l)) * l*0.03;
        z += d;
    }
    fragColor = tanh(fragColor / 8e3);
}
