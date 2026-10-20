// "ice and fire" by mattz - https://www.shadertoy.com/view/MdfBzl (2017-06-15) - image pass (see common.glsl for the licence notes)

// lattice point table written by points.glsl: TABLE_W x TABLE_H texels, texel (ix + TABLE_X0, iy + TABLE_Y0) holds the point
vec2 latticePoint(vec2 id) {
    return texture(iChannel0, (id + vec2(TABLE_X0, TABLE_Y0) + 0.5) / vec2(TABLE_W, TABLE_H)).xy;
}

//////////////////////////////////////////////////////////////////////

void mainImage( out vec4 fragColor, in vec2 fragCoord ) {

    float scl = 4.1 / iResolution.y;

    // get 2D scene coords
    vec2 p = (fragCoord - 0.5 - 0.5*iResolution.xy) * scl;

    // get triangular base coords
    vec2 tfloor = floor(cart2tri * p + 0.5);

    // the 9 neighbouring lattice points come from the per-frame table (see points.glsl)
    vec2 pts[9];

    for (int i=0; i<3; ++i) {
        for (int j=0; j<3; ++j) {
            pts[3*i+j] = latticePoint(tfloor + vec2(i-1, j-1));
        }
    }

    // color accumulator
    vec4 cw = vec4(0);

    // for each of the 4 quads:
    for (int i=0; i<2; ++i) {
        for (int j=0; j<2; ++j) {

            // look at lower and upper triangle in this quad
            vec4 t00 = vec4(pts[3*i+j  ], tfloor + vec2(i-1, j-1));
            vec4 t10 = vec4(pts[3*i+j+3], tfloor + vec2(i,   j-1));
            vec4 t01 = vec4(pts[3*i+j+1], tfloor + vec2(i-1, j));
            vec4 t11 = vec4(pts[3*i+j+4], tfloor + vec2(i,   j));

            // lower
            tri_color(p, t00, t10, t11, scl, cw);

            // upper
            tri_color(p, t00, t11, t01, scl, cw);

        }
    }


    // final pixel color
    fragColor = cw / cw.w;

}
