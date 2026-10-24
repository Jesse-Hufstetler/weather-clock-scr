// "Split" by @Xor, https://fragcoord.xyz/s/sz0i56em (written in FragCoord's GOLF shorthand; this is its GLSL conversion)

void mainImage(out vec4 fragColor, vec2 fragCoord) {
    vec4 O = vec4(0.0);
    vec2 p = vec2((2.0*fragCoord-iResolution.xy)/iResolution.y);
    vec2 c = vec2(4.0*p/(3.0-p.y)+iTime);
    p=5.0*p/(2.0+fract(dot(cos(floor(c+0.5)+iResolution.xy),sin(ceil(c+cos(c*10.0)))))-p.y)+iTime;
    O=tanh(.2/abs(sin(p.x+p.y+vec4(1.0, .7, .6, 0.0)*sin(p.y))));
    O*=O;
    fragColor = O;
}
