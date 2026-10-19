// "Spiral Journey" by ChunderFPV - https://www.shadertoy.com/view/mssSWr (CC BY-NC-SA 3.0 default)
// Image pass. The original also draws a mouse-driven number counter (code from https://www.shadertoy.com/view/7tV3zK,
// using a font texture and textureGrad) only while a mouse button is held; the saver never has that, so it is omitted.

void mainImage( out vec4 RGBA, in vec2 XY )
{
    vec3 c = texture(iChannel0, XY/R, max(1., length(SC*2.))).rgb;
    c *= c*3.5; // contrast

    RGBA = vec4(c, 1.);
}
