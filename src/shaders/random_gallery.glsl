// "random gallery" by SnoopethDuckDuck (forked from "starting point 8008")
// https://www.shadertoy.com/view/stdSDf
// License: no license stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// Included unmodified (the host adds a tanh() polyfill for GLSL 1.20).
#define pi 3.14159

float thc(float a, float b) {
    return tanh(a * cos(b)) / tanh(a);
}

float ths(float a, float b) {
    return tanh(a * sin(b)) / tanh(a);
}

vec2 thc(float a, vec2 b) {
    return tanh(a * cos(b)) / tanh(a);
}

vec2 ths(float a, vec2 b) {
    return tanh(a * sin(b)) / tanh(a);
}

vec3 pal( in float t, in vec3 a, in vec3 b, in vec3 c, in vec3 d )
{
    return a + b*cos( 6.28318*(c*t+d) );
}

float h21 (vec2 a) {
    return fract(sin(dot(a.xy, vec2(12.9898, 78.233))) * 43758.5453123);
}

float mlength(vec2 uv) {
    return max(abs(uv.x), abs(uv.y));
}

float rand(float val, vec2 ipos) {
    float v = h21(floor(val) + 0.01 * ipos);
    float v2 = h21(floor(val) + 1. + 0.01 * ipos);
    float m = fract(val);
    m = m * m * (3. - 2. * m); // could use different function here
    return mix(v, v2, m);
}

void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    vec2 uv = (fragCoord-0.5*iResolution.xy)/iResolution.y;
        
    float sc = 7.;
    uv.x += 0.04 * iTime;
    uv.y -= cos(1.1 * floor(sc * uv.x) + 0.05 * iTime);
     
    vec2 ipos = floor(sc * uv) + 0.5;
    vec2 fpos = sc * uv - ipos;
    
    // could use rand(,) instead of h21() here but it gets chaotic when u stack them
    float a = 2. * pi * h21(ipos);
    float val0 = h21(ipos) - 10. * (cos(a) * uv.x + sin(a) * uv.y) - 0.1 * iTime;
    float v0 = rand(val0, ipos);
    
    float val = h21(ipos) - 2.5 * v0 * thc(4., v0 * 10. * length(fpos)) - 0.5 * iTime;
    float v = rand(val, ipos);
    
    float rd = 0.5 * v;
    float t = 10. * v + length(fpos) * 10. * v0 - iTime;
    vec2 p = (0.5 - rd) * vec2(cos(t), sin(t));
    
    float d = length(fpos - p);
    float k = 0.5;
    float s = smoothstep(-k, k, -d + rd);
    s = 2. * s * s * s;
    vec3 col = vec3(s);
    
    vec3 e = vec3(1.);
    col = s * pal(4. * v + d, e, e, e, 0.5 * vec3(0.,0.33,0.66));
    col += 0.1;
    
    fragColor = vec4(col, 1.);
}
