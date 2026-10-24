/*
    "Hillside" by @XorDev
    https://www.shadertoy.com/view/3fySDD

    A series of lightweight shaders made for easy learning and high-performance applications:
    https://www.shadertoy.com/playlist/ccffz7

    MIT license
    Credit is appreciated, but not required!

    Here's the overview:
    1) Center the coordinates and scale to fit vertically
    2) Generate some noise
    3) Set the wave frequencies and offsets
    4) Use the distance to the sine waves for lightness
    5) Tonemap with tanh
       Intro to tonemapping: https://mini.gmshaders.com/p/tonemaps

*/

void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    //Centered and scale to fit vertically
    vec2 p = (fragCoord*2.-iResolution.xy)/iResolution.y;

    //Quick noise pattern
    float n = mod(dot(fragCoord, sin(fragCoord.yx)),.1);
    //RGBA wave frequencies
    vec4 f = vec4(.7,1,1.3,0);
    //RGBA wave offsets
    vec4 o = vec4(0,1,2,0);

    //Tanh for tonemapping
    //https://www.shadertoy.com/view/ms3BD7
    //https://mini.gmshaders.com/p/func-tanh
    fragColor = tanh(.2 / abs(p.y + .3*cos(iTime+n+p.x*f+o)));
}
