// "ice and fire" by mattz - https://www.shadertoy.com/view/MdfBzl - lattice point table (NOT part of the original).
// Writes triPoint(id) for every lattice index id the image pass can ask for; the table is small because the lattice that is
// visible on screen is only ~10 x 6 points (indices roughly -7..7 by -4..3, generous for wide monitors).
void mainImage( out vec4 fragColor, in vec2 fragCoord ) {
    vec2 id = floor(fragCoord) - vec2(TABLE_X0, TABLE_Y0);
    fragColor = vec4(triPoint(id), 0.0, 1.0);
}
