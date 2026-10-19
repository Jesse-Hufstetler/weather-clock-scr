// "Spiral Journey" by ChunderFPV - https://www.shadertoy.com/view/mssSWr (2022-12-20)
// "10 minute number spiral animation." License: no license stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// This is the shader's "Common" tab, unmodified.
#define SCALE 2.5
#define R iResolution.xy
#define M iMouse
#define T iTime
#define SC ((XY-.5*R)/R.y)
#define CS(a) vec2(cos(a), cos(a-TAU/4.))
#define TAU 6.2831853071795865
#define DS(n,a,pr) max(0., 1.-length(sc-CS(a-n*TAU*rr)*l)/(pr))
#define FS(n,a,pr) max(0., 1.-length(sc-CS(a-(TAU*sqrt(abs(n))*sign(n))*sqrt(rr*2.))*l)/(pr))
#define R2N(m) (abs(m) > 1.) ? 1./(2.*sign(m)-m): m
#define CL(x,a,b) smoothstep(0.,1.,(2./3.)*(x-a)/(b-a)+(1./6.))*(b-a)+a // https://www.shadertoy.com/view/Ws3Xzr
