// "mac flurry screensaver" by ufffd - https://www.shadertoy.com/view/lX2SDt (CC BY-NC-SA 3.0 default)
// Buffer D: sums all the 'tails' from buffers A, B and C, colorizes them, and adds a glowy feedback layer.
// Only change: `O-=O;` (relies on O starting at zero, which desktop GLSL does not guarantee) is written `O = vec4(0.);`.

void mainImage( out vec4 O, in vec2 xy )
{
    vec2 uv = xy/R;
    vec2 uvc = (xy-R/2.)/R.y;
    vec4 t[3];
    t[0] = texture(iChannel0,uv);
    t[1] = texture(iChannel1,uv),
    t[2] = texture(iChannel2,uv);
    O = vec4(0.);
    vec3 rootcol = vec3(
        snoise(vec3(10.,10.,iTime*.1)),
        .6+snoise(vec3(1.5,1.5,iTime*.1))*.2,
        1.
    );
    for (int i=0;i <3;i++)
    for (int j=0;j<4;j++) {
        vec3 col = rootcol;
        col.r = mix(rootcol.r,snoise(vec3(float(i)*10.,float(j)*10.,iTime*.1)),0.2);
        col.r += length(uvc)*.1;
        col = hsv2rgb(col);
        O+=t[i][j]*vec4(col,1.);
    }
    vec4 lp = texture(iChannel3,uv,0.5);
    lp.rgb = (lp.rgb - 0.504)*1.001 + 0.5;
    O = mix(O,lp,0.95);
}
