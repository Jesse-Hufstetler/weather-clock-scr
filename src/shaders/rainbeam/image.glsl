// "Rainbeam" by Inityx - https://www.shadertoy.com/view/flffWX (CC BY-NC-SA 3.0 default)
// Image pass: colour grade + dither. iChannel1 (a Shadertoy-hosted noise texture) is the host's generated noise.
// Only change: `saturate` is renamed `rb_saturate` (some GLSL compilers treat `saturate` as a built-in).

const float DITHER = 1.0 / 128.0;
const float B = 0.2;

const mat3 BLEED = mat3(
    1.0, B, B,
    B, 1.0, B,
    B, B, 1.0
);


float rb_saturate(float value) {
    return (value <= 0.5)
        ? value
        : 1.5 - (1.0 / (value + 0.5));
}

vec3 grade(vec3 color) {
    vec3 bled = color * BLEED;
    return vec3(
        rb_saturate(bled.r),
        rb_saturate(bled.g),
        rb_saturate(bled.b)
    );
}

vec3 dither(vec3 color, vec2 fragCoord) {
    vec3 noise = texture(iChannel1, fragCoord/iResolution.xy).rgb;
    return color + (noise - 0.5) * DITHER;
}

void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    vec3 raw = texture(iChannel0, fragCoord/iResolution.xy).rgb;
    vec3 graded = dither(grade(raw), fragCoord);

    fragColor = vec4(graded, 1.0);
}
