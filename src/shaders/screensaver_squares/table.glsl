// "Screensaver squares" - cell table pass (NOT part of the original), see common.glsl.
void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    float t = iTime*.05;
    float row = floor(fragCoord.y);
    if (row >= 3.0*TABLE_N) {
        // header row: the centre cell of each layer's block
        float fade;
        float L = floor(fragCoord.x);
        float i = L < 0.5 ? 0. : (L < 1.5 ? 1./3. : 2./3.);
        vec2 k0 = floor(layerCoord(vec2(0.), i, t, fade));
        fragColor = vec4(k0, 0., 1.);
        return;
    }
    float L = floor(row / TABLE_N);
    float i = L < 0.5 ? 0. : (L < 1.5 ? 1./3. : 2./3.);
    float fade;
    vec2 k0 = floor(layerCoord(vec2(0.), i, t, fade));
    vec2 k = k0 + vec2(floor(fragCoord.x), row - L*TABLE_N) - TABLE_N*0.5;
    fragColor = vec4(StarCell(k - 0.5), 1.0);
}
