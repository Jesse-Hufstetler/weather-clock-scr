// CC0: Submerged cave of frozen flame
//  Was tinkering on saturday evening, This is the result
// by mrange - https://www.shadertoy.com/view/N3dXRI  (CC0 / public domain)
// Twigl: https://twigl.app?ol=true&ss=-P4DSKsn-0yVRAUzc8f0
// Only change from the original: i, z and O are explicitly zero-initialised
// (WebGL zeroes locals, desktop GLSL does not).

void mainImage(out vec4 o, vec2 C) {
  float i=0.,z=0.,d,s,S;
  vec3 p,P,O=vec3(0.),r=iResolution,U=vec3(1,2,3);
  for(
  ; p=z*normalize(vec3(C,r.y)-.5*r)
  , p.z+=iTime/9.
  , p.y=abs(s=p.y+.1)-.1
  , P=p
  , ++i<77.
  ; S=length(P-p)
  , s=s<0.?s/9.:0.
  , d=abs(.63-length(p-round(p))+s)+2e-3-s
  , p=1.+sin(5.*P.x+9.*S+P.z/2.+U)
  , O+=p.x/d*S*S*p-sign(s)*U
  , z+=d/5.
  ) for(S=6.;S<96.;S+=S) p+=sin(p.yzx*S+P.z)/S;
  o.xyz=tanh(O/1e3+pow(z,5.)*U/2e3);
}
