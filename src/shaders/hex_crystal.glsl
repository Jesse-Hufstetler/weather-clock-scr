// "hex crystal" by ChunderFPV - https://www.shadertoy.com/view/ct2SWw (2023-02-22)
// "Sort of a crystal nebula of hexagons." Uses simple hexagonal tiles by lomateron (https://www.shadertoy.com/view/MlXyDl).
// License: no license stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// Included unmodified; the host supplies iMouse as a constant 0, so the auto-rotation branch is always taken.
#define H(a) (cos(radians(vec3(100, 140, 190))-((a)*6.2832))*.5+.5) // hue pallete
#define RT(a) mat2(cos(m.a*1.571+vec4(0,-1.571,1.571,0))) // rotate
void mainImage( out vec4 C, in vec2 U )
{
    vec3  c = vec3(0),
          o = vec3(1 ,1.732, 1),
          u, v, a, b;
    vec2  R = iResolution.xy,
          m = iMouse.xy/R*4.-2.,
          uv = (U-.5*R)/R.y; // 2d coords
    float t = iTime/5.,
          tr = smoothstep(0., 1., sin(t)*.6+.5),
          s = tr*4.+2., // scale
          h, k, i;
    if (iMouse.z < 1.) m = vec2(sin(t/2.)*.6, sin(t)*.4); // rotate with time
    mat2  pitch = RT(y),
          yaw = RT(x);
    for (i = .1; i < 1.; i+=.1)
    {
        u = normalize(vec3(uv, .7*sqrt(i)))*s; // 3d coords
        u.yz *= pitch;
        u.xz *= yaw;

        // simple hexagonal tiles by lomateron https://www.shadertoy.com/view/MlXyDl
        a = mod(u, o)*2.-o;
        b = mod(u+o*.5, o)*2.-o;
        h = min(dot(a, a), dot(b, b))*.5;

        v = h*h*H(i-t/5.);
        k = 1.1-max(i, tr-i);
        c += v*k;
    }
    C = vec4(pow(c, vec3(2.5))*.5+.05, 1);
}
