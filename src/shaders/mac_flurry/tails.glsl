// "mac flurry screensaver" by ufffd - https://www.shadertoy.com/view/lX2SDt (CC BY-NC-SA 3.0 default)
// Buffers A, B and C of the original are identical except for the noise offset passed to fbDot (1., 10., 20.);
// the host supplies it as `#define TAIL_OFFSET <n>.` in front of this file. Each renders one feedback 'tail' per channel.

float drawTail(in vec2 uvc, in float offset, in int ch) {
    vec2 lpuv = uvc;
    uvc += coffset;
    float cdist = length(uvc);
    float circle = SS(csize,csize*.75,cdist);
    float ang = fbm(vec3(uvc*nsize,offset+iTime*.02+cdist*.1),3)*TAU;
    float distf = dspace/R.y;
    float dist = distf + fbm(vec3(uvc*nsize,offset+33.+iTime*.2-cdist*.1),2)*distf/2.;
    lpuv += vec2(0.,R/200.)*Rot(ang)*dist;
    lpuv = UVC2UV(lpuv);
    float lp = texture(iChannel0, lpuv, .5)[ch];
    lp=contrast(lp,1.0001);
    return mix(pow(circle,5.)*10.,lp*0.995,1.-circle);
}

float fbDot(in int ch, in vec2 xy, float offset) {
    offset += float(ch)*20.;
    vec2 uvc = (xy-R/2.)/R.y;
    return drawTail(uvc,offset,ch);
}

void mainImage( out vec4 O, in vec2 xy ) {
    vec2 uvc = (xy-R/2.)/R.y;
    float cdist = length(uvc);
    xy += vec2(
        snoise(vec3(uvc*nsize,99.+iTime*.1)),
        snoise(vec3(uvc*nsize,77.+iTime*.1))
    )*(cdist+0.5);
    O.r = fbDot(0, xy, TAIL_OFFSET);
    O.g = fbDot(1, xy, TAIL_OFFSET);
    O.b = fbDot(2, xy, TAIL_OFFSET);
    O.a = fbDot(3, xy, TAIL_OFFSET);
}
