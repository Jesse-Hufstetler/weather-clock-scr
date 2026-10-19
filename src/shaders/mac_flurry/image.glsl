// "mac flurry screensaver" by ufffd - https://www.shadertoy.com/view/lX2SDt (CC BY-NC-SA 3.0 default)
// Image pass: shows Buffer D. Unmodified.

// Buffers A, B, and C are near identical, each renders one feedback 'tail' in each of its channels
// Buffer D sums all of those 'tails' together, colorizes them, and adds an extra layer of glowy feedback

void mainImage( out vec4 O, in vec2 xy ) {
    vec2 uv = xy/iResolution.xy;
    O = texture(iChannel0,uv);
}
