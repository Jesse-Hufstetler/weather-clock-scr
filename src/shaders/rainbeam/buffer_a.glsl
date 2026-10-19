// "Rainbeam" by Inityx - https://www.shadertoy.com/view/flffWX (2022-05-03) - "Blinding beam of rainbow"
// License: no license stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// Buffer A: five coloured blobs, blended with the previous frame (iChannel0 = itself) with a slight zoom.
// iChannel1 was a Shadertoy-hosted noise texture; the host supplies a generated one. iTimeDelta is the host's fixed
// 1/60 s simulation step. Only change: the local `blobs` (which shadowed the function it was initialised from) is
// renamed `blob_color`.

const float FADE_RATE = 1.9;
const float ECCENTRICITY = 1.0/48.0;
const float BLOB_SIZE = 1.0/56.0;
const float PI = 3.14159265;
const float TAU = PI * 2.0;
const float PHI = 1.618033988;
const int BLOBS = 5;
const float ROTATION_SPEED = 1.0 / 16.0;
const float COLOR_SPEED = 1.0 / 50.0;
const float TURBULENCE_SPEED = 1.0 / 2048.0;

vec2 zoom(vec2 uv, float amount) {
    return ((uv - vec2(0.5)) / amount) + vec2(0.5);
}

vec3 color_spin(int i) {
    float theta = (fract(iTime * COLOR_SPEED) + (float(i) / float(BLOBS))) * TAU;
    vec3 rgb = vec3(
        sin(theta),
        sin(theta + 1.0 * TAU / 3.0),
        sin(theta + 2.0 * TAU / 3.0)
    );
    return max(rgb, vec3(0.0));
}

vec3 blobs(vec2 uv) {
    vec3 color = vec3(0.0);
    for (int i = 0; i < BLOBS; i++) {
        float theta = (fract(iTime * ROTATION_SPEED) + (float(i) / float(BLOBS))) * TAU;

        vec2 direction = vec2(sin(theta), cos(theta));
        vec2 coordinate = pow(float(i + 1) / float(BLOBS), 0.75) * direction * ECCENTRICITY;

        if (distance(coordinate, uv) < BLOB_SIZE) {
            color += color_spin(i);
        }
    }

    return color;
}

void mainImage( out vec4 fragColor, in vec2 fragCoord ) {
    vec2 raw_uv = fragCoord/iResolution.xy;
    vec2 corrected_uv = (raw_uv - vec2(0.5)) / vec2(1.0, iResolution.x/iResolution.y);
    vec3 blob_color = blobs(corrected_uv);

    vec2 prev_uv =
        zoom(raw_uv, 1.0 + iTimeDelta) +
        (texture(iChannel1, corrected_uv - iTime * TURBULENCE_SPEED).rg - 0.5) / 2048.0;

    vec3 previous = texture(iChannel0, prev_uv).rgb;

    float factor = 1.0 - (iTimeDelta * FADE_RATE);

    fragColor = vec4(vec3(blob_color + previous * factor), 1.0);
}
