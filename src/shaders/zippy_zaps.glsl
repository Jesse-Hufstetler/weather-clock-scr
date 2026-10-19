// "Zippy Zaps [394 Chars]" by SnoopethDuckDuck (golf help: Nguyen2007, bug)
// https://www.shadertoy.com/view/XXyGzh
// License: no license stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// Changes from the original: `i` is explicitly zero-initialised (WebGL zeroes locals, desktop GLSL does not),
// and `dot(u *= mat2(...), u)` is split in two so evaluation order is well defined. tanh() comes from the host shim.
// The golfed for-loop (one big comma expression) is also written out as plain statements with the same maths and
// order, because it crashed the compiler of some GPU drivers.

void mainImage( out vec4 o, vec2 u )
{
    vec2 v = iResolution.xy;
         u = .2*(u+u-v)/v.y;

    vec4 z = o = vec4(1,2,3,0);
    float a = .5, t = iTime;

    for (int n = 1; n < 19; n++)
    {
        float i = float(n);
        t += 1.;
        a += .03;
        v = cos(t - 7.*u*pow(a, i)) - 5.*u;
        vec4 q = cos(i + .02*t - z.wxzw*11.);
        u = u * mat2(q.x, q.y, q.z, q.w);
        u += tanh(40. * dot(u,u) * cos(1e2*u.yx + t)) / 2e2
           + .2 * a * u
           + cos(4./exp(dot(o,o)/1e2) + t) / 3e2;
        o += (1. + cos(z+t))
           / length((1.+i*dot(v,v))
                  * sin(1.5*u/(.5-dot(u,u)) - 9.*u.yx + t));
    }

     o = 25.6 / (min(o, 13.) + 164. / o)
       - dot(u, u) / 250.;
}
