#version 120
// "Ultra Liquid Bokeh" by Weyland - https://www.shadertoy.com/view/MtdXzr (2016-10-31)
// "Messing around with iq's bubbles to get endless bokeh variations and nice colorchannel interactions"
// Based on Inigo Quilez's "Bubbles" (2013): https://www.shadertoy.com/view/4dl3zn
// License (stated in the shader's own header, below): Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported.
//
// Restructured for speed, same picture. The original is one fragment shader that loops over all 128 bubbles for every pixel
// and re-derives each bubble (about 900 sin() calls per pixel) every time. Here each bubble is instead drawn as one quad
// that covers just its own circle, with additive blending (the original just sums the bubbles too), so a pixel only pays for
// the bubbles that actually touch it. The bubble parameters (position, radius, colour) are evaluated once per frame on the
// CPU with the original formulas (bg::bokeh_data), and the original's final `color *= sqrt(1.5-0.5*length(uv))` is applied
// per bubble, which is the same because a sum scales term by term.
//
// This file is the fragment shader for one bubble (see Renderer::render_bubbles). The original per-bubble code, for
// reference, evaluated by bg::bokeh_data:
//        float pha =      sin(float(i)*546.13+1.0)*0.5 + 0.5;
//        float siz = pow( sin(float(i)*651.74+5.0)*0.5 + 0.5, 4.0 );
//        float pox =      sin(float(i)*321.55+4.1) * iResolution.x / iResolution.y;
//        float rad = 0.1+0.5*siz+sin(pha+siz)/4.0;
//        vec2  pos = vec2( pox+sin(iTime/15.+pha+siz), -1.0-rad + (2.0+2.0*rad)*mod(pha+0.3*(iTime/7.)*(0.2+0.8*siz),1.0));
//        float dis = length( uv - pos );
//        vec3  col = mix( vec3(0.194*sin(iTime/6.0)+0.3,0.2,0.3*pha), vec3(1.1*sin(iTime/9.0)+0.3,0.2*pha,0.4), 0.5+0.5*sin(float(i)));
//        color += col.zyx *(1.0-smoothstep( rad*0.15, rad, dis ));
//
// Created by inigo quilez - iq/2013 : https://www.shadertoy.com/view/4dl3zn
// License Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported License.
// Messed up by Weyland

uniform vec4 bub;   // (centre.xy, radius, 0) in the original's scene units (height = 2, centred)
uniform vec3 bcol;  // the bubble's colour, already swizzled the way the original adds it
varying vec2 uv;    // offset from the bubble centre (set per vertex)

void main() {
    float rad = bub.z;
    float dis = length(uv);
    vec2 p = bub.xy + uv;
    float a = 1.0 - smoothstep(rad*0.15, rad, dis);
    gl_FragColor = vec4(bcol * (a * sqrt(1.5 - 0.5*length(p))), 1.0);
}
