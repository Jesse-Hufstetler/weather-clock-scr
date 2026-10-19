// "SKEENSAVUH" by JesseHufstetler (2024-02-10) - https://www.shadertoy.com/view/l3XGRB
// The repo owner's own shader (a fork of "Fork bubs JesseHufst 540"), included here as the default background.
// Changes: the bubble-exponent parameter `exp` is renamed `ex` (it shadowed the GLSL built-in exp()).

vec3 ScreenSpaceDither(vec2 vScreenPos, float colorDepth)
{
    // lestyn's RGB dither (7 asm instructions) from Portal 2 X360, slightly modified for VR
    vec3 vDither = vec3(dot(vec2(131.0, 312.0), vScreenPos.xy + iTime));
    vDither.rgb = fract(vDither.rgb / vec3(103.0, 71.0, 97.0)) - vec3(0.5, 0.5, 0.5);
    return (vDither.rgb / colorDepth) * 0.375;
}
void bub(inout vec4 fragColor, in vec2 fragCoord, float radius, vec2 bubbleCenter, vec3 bubbleColor, float ex) {
        float dis = length( fragCoord - bubbleCenter);
        float disN = dis/radius;
        if (dis < radius) {
            fragColor.xyz += pow((1.-disN),ex)*bubbleColor;
        }
}
void slideBub(inout vec4 fragColor, in vec2 fragCoord, float radius, vec2 speed, vec3 bubbleColor, float ex) {
    vec2 bubbleCenter;
    bubbleCenter = (sin(iTime*speed+speed*500.)*iResolution.xy+iResolution.xy)/2.;
    bub(fragColor, fragCoord, radius, bubbleCenter, bubbleColor, ex);
}


float myTime = 0.;
float oldTime = 0.;
void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    float scale = (iResolution.x + iResolution.y)/2.;
    fragColor = vec4(0.0,0.0,0.0,1.0);


    float divvy = .999;
    float size = 3.;
    vec3 pink = vec3(1.,.5, 1.);
    vec3 yellow = vec3(1.,1.,.5);
    vec3 blue = vec3(0.5,.9,1.0);
    vec3 lightPink = vec3(1.,.9,1.);
    vec3 white = vec3(1.,1.,1.);
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.11/divvy,.16/divvy), pink,5.);
    divvy *= 1.1;
    //slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.1/divvy,.13/divvy), yellow, 5.);
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.1/divvy,.14/divvy), blue, 5.);
    divvy *= 1.1;
    //slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.1/divvy,.15/divvy), lightPink,5.);
    divvy *= 1.13;
    //slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.11/divvy,.16/divvy), pink,5.);
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.1/divvy,.13/divvy), yellow, 5.);
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.1/divvy,.14/divvy), blue,5.);
    divvy *= 1.1;
    slideBub(fragColor, fragCoord, iResolution.y*size/divvy, vec2(.1/divvy,.15/divvy), lightPink,5.);
    //slideBub(fragColor, fragCoord, iResolution.y*1., vec2(.1,.003), white, 5.);
    //fragColor += fragColor;
    //fragColor += fragColor;
    //fragColor += fragColor;
    //fragColor += fragColor;
    //fragColor += fragColor;
    //fragColor += fragColor;
    fragColor *= fragColor;
    //fragColor *= fragColor;
    //fragColor += fragColor;
    //fragColor *= fragColor;
    //fragColor *= fragColor;
    //fragColor += fragColor;
    //fragColor += fragColor;



    fragColor *= fragColor;
    fragColor += vec4(ScreenSpaceDither(fragCoord.xy, 20.0), 1.);
    //fragColor += (vec4(rand(), rand(), rand(), 1.)-.5)/10.;
}
