// "Screensaver squares" by julianlumia - image pass (see common.glsl for the credits and the change notes).
void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    vec2 uv = (fragCoord.xy-.5*iResolution.xy)/iResolution.y;
    uv*=1.;
    float t = iTime*.05;
    vec3 col = vec3(0.);
    float tableH = 3.0*TABLE_N + 1.0;

    for (int l=0; l<3; l++){
        float i = l == 0 ? 0. : (l == 1 ? 1./3. : 2./3.);
        float fade;
        vec2 cell = floor(layerCoord(uv, i, t, fade));
        // the table was built a fraction of a second ago: use the centre cell it recorded, not today's
        vec2 k0 = texture(iChannel0, vec2((float(l) + 0.5)/TABLE_N, (3.0*TABLE_N + 0.5)/tableH)).xy;
        vec2 tex = clamp(cell - k0 + TABLE_N*0.5, 0.0, TABLE_N - 1.0);
        col += texture(iChannel0, vec2((tex.x + 0.5)/TABLE_N, (float(l)*TABLE_N + tex.y + 0.5)/tableH)).rgb * fade;
    }

    fragColor = vec4(col,1.0);
}
