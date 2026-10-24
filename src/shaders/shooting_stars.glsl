/*
    "Shooting Stars" by @XorDev
    https://www.shadertoy.com/view/ctXGRn

    I got hit with inspiration for the concept of shooting stars.
    This is what I came up with.

    Tweet: twitter.com/XorDev/status/1604218610737680385
    Twigl: t.co/i7nkUWIpD8
    <300 chars playlist: shadertoy.com/playlist/fXlGDN
*/
void mainImage(out vec4 O, vec2 I)
{
    //Clear fragcolor
    O = vec4(0.0);

    //Line dimensions (box) and position relative to line
    vec2 b = vec2(0,.2), p = vec2(0.0);
    //Rotation matrix
    mat2 R = mat2(1.0);
    //Iterate 20 times
    for(float i=.9; i++<20.; )
    {
        //Rotate for each iteration
        vec4 cs = cos(i+vec4(0,33,11,0));
        R = mat2(cs.x, cs.y, cs.z, cs.w); // built component by component (a driver crash workaround)
        //Using rotated boxes
        p = R*(fract((I/iResolution.y*i*.1+iTime*b)*R)-.5);
        //Add attenuation, with my favorite color palette
        O += 1e-3/length(clamp(p,-b,b)-p)
        *(cos(p.y/.1+vec4(0,1,2,3))+1.);
    }
}
