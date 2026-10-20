// "mac flurry screensaver" by ufffd - https://www.shadertoy.com/view/lX2SDt (CC BY-NC-SA 3.0 default)
// Buffers A, B and C of the original are identical except for the noise offset passed to fbDot (1., 10., 20.); each renders
// one feedback 'tail' per channel. Here the host supplies which tail this is with `#define ANG_PANEL <n>.` and
// `#define DIST_PANEL <n>.` in front of this file.
// Changed for speed: the simplex noise that picks each tail's direction and spacing (and the lookup warp) is no longer
// evaluated per pixel; it is read from the noise-field pass (see field.glsl). Everything else is the original code.

float drawTail(in vec2 uvc, in float ang, in float distNoise, in int ch) {
    vec2 lpuv = uvc;
    uvc += coffset;
    float cdist = length(uvc);
    float circle = SS(csize,csize*.75,cdist);
    float distf = dspace/R.y;
    float dist = distf + distNoise*distf/2.;
    lpuv += vec2(0.,R/200.)*Rot(ang)*dist;
    lpuv = UVC2UV(lpuv);
    float lp = texture(iChannel0, lpuv, .5)[ch];
    lp=contrast(lp,1.0001);
    return mix(pow(circle,5.)*10.,lp*0.995,1.-circle);
}

void mainImage( out vec4 O, in vec2 xy ) {
    vec2 uv0 = xy/R;
    vec2 uvc = (xy-R/2.)/R.y;
    float cdist = length(uvc);
    xy += nf(6., uv0).xy*(cdist+0.5);
    vec2 uvw = (xy-R/2.)/R.y;
    vec4 ang = nf(ANG_PANEL, uv0)*TAU;
    vec4 dn = nf(DIST_PANEL, uv0);
    O.r = drawTail(uvw, ang.r, dn.r, 0);
    O.g = drawTail(uvw, ang.g, dn.g, 1);
    O.b = drawTail(uvw, ang.b, dn.b, 2);
    O.a = drawTail(uvw, ang.a, dn.a, 3);
}
