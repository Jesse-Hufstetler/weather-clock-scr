// "Painted" by @Xor, https://fragcoord.xyz/s/pz22y9ow (written in FragCoord's GOLF shorthand; this is its GLSL conversion)

void mainImage(out vec4 fragColor, vec2 fragCoord) {
    vec4 O = vec4(0.0);
    vec2 p = vec2((2.0*fragCoord-iResolution.xy)/iResolution.y);
    p=p*10.0/(3.0-p.y)+iTime;
    O+=sin(sin(dot(ceil(p+sin(p*3.0).yx),vec2(17.0, 79.0)))*7e3+vec4(0.0, 1.0, 2.0, 0.0))+.5;
    fragColor = O;
}
