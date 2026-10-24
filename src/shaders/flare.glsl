// "Flare [148]" by @Xor, https://fragcoord.xyz/s/fatt9xo6 (written in FragCoord's GOLF shorthand; this is its GLSL conversion)

void mainImage(out vec4 fragColor, vec2 fragCoord) {
    vec4 O = vec4(0.0);
    for(float i=-1.;i<1.;i+=.1) {
        vec4 cs = cos(iTime+i+vec4(0,11,33,0));
        mat2 m = mat2(cs.x, cs.y, cs.z, cs.w); // built component by component (a driver crash workaround)
        O+=(cos(i/.3+vec4(0.0, 1.0, 2.0, 0.0))+1.)/max(length(tan(((2.*fragCoord-iResolution.xy)/iResolution.y+i)/.6*m+i)),i*i);
    }
    O=tanh(O*O/1e3);
    fragColor = O;
}
