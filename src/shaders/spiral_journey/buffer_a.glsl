// "Spiral Journey" by ChunderFPV - https://www.shadertoy.com/view/mssSWr (CC BY-NC-SA 3.0 default)
// Buffer A: the spirals, blended with the previous frame (iChannel0 = itself, mip-filtered).
// iChannel1 was a Shadertoy-hosted image used only for a faint background tint; the host supplies a generated
// tileable colour-noise texture in its place. Otherwise unmodified.

void mainImage( out vec4 RGBA, in vec2 XY )
{
    float sec = 600.; // time to cycle through number line
    float st = 0.; // start time
    float ra = ((mod(T+st, sec)/sec)-.5)*4.; // time to range -2 to +2
    ra *= .97; // truncate near +/-inf
    vec2 m = (M.xy/R*4.)-2.; // mouse
    float t = (M.z > 0.) ? R2N(m.x): R2N(ra); // time
    float e = (M.z > 0.) ? R2N(-m.y): t; // exponent
    float z = (e < 0.) ? pow(100., e): pow(500., e); // zoom
    z *= SCALE;
    z = clamp(z, CL(z, 1e-16, 1e-15), CL(z, 1e+18, 1e+19)); // soft clamp at precision loss
    vec2 sc = SC*z;

    vec2 tc = SC*SCALE; // texture coords
    float tl = length(tc); // non-zoomed screen radius
    float ring = pow(min(tl, 1./tl), 2.);

    float tr = sqrt(dot(tc, tc)); // transform
    vec3 tex = texture(iChannel1, tc/tr+t, .5).rgb*tr;

    float bs = -.005*sign(t); // radial blur strength
    float maa = (e < 0.) ? pow(z/tl/SCALE, .25): .5; // mipmap aa
    float fade = min(.5, abs(e)*25.); // fade near 0
    vec3 c = texture(iChannel0, mix(XY/R, vec2(.5), bs), .5).rgb*.96; // blend with previous frames
    c += vec3(.1, .3, .6)*tex*.02*fade; // background color
    if (e < 0.) c += pow(z/tl*fade, 1.25)*vec3(.1, .2, .3)*.005; // blue dot
    c += vec3(.1, .2, .3)*ring*min(.03, abs(e)*10.); // blue ring

    vec3 dc = vec3(.8, .4, 1.)*ring*.5; // denominator color
    vec3 nc = vec3(.8, .1, .2)*ring*.5; // numerator color
    vec3 fc = vec3(1., .6, .3*tl)*pow(tl, clamp(-1./e, 0., 1.))*.4;  // fraction color

    float r = length(sc); // screen radius
    float sr = TAU/4.; // starting rotation
    float px = length(fwidth(sc))/1.414; // pixel radius
    float pr = max(px*3., .005*z); // point radius
    float cr = pow(z/SCALE, 1./e)*TAU; // counter rotation
    float fscr = sqrt(abs(t)*cr*2.*TAU)*sign(t); // counter rotation
    float rb = sin(T*TAU/10.)*.1; // rotation bounce

    float lim = 4.; // loop limit
    for (float i=0.; i<lim; i++) // overlap cells for spirals
    {
        float rr = round((exp(log(r)/e)-i)/lim)*lim+i; // rounded radius
        float l = pow(rr, e); // radius
        c += DS(1./t, (1./t)*cr-sr+rb*.25, pr*.5)*nc; // numerator spiral
        c += DS(t, t*cr-sr+rb*.5, pr*.5)*dc; // denominator spiral
        c += FS(t, fscr+rb*.75, pr)*fc; // fraction spiral
    }
    vec2 b = abs(fract(XY/R+.5)-.5)/fwidth(XY/R)/5.; // black border
    c = min(c, min(b.x, b.y)*.75); // blend with border

    RGBA = vec4(c, 1.);
}
