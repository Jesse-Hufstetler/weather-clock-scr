// "Screensaver squares" by julianlumia - https://www.shadertoy.com/view/Wdcyz7 (2020-09-20)
// "small change after following the tutorial by bigwings" - full credits to BigWings (Martijn Steinrucken, The Art of Code):
// https://www.youtube.com/watch?v=rvDo9LvfoVE
// License: no license stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// Only change: the local variable `star` in StarLayer (which shadowed the function `star`) is renamed `st`,
// since not every desktop GLSL compiler accepts a variable named like the function it is initialised from.
// Changed for speed, same picture: each layer's colour only depends on which grid cell the pixel is in (the picture is made of
// flat-coloured squares), so the colour of every cell that can be on screen is computed once per frame into a table
// (table.glsl) and the image pass (image.glsl) reads one table entry per layer instead of looping over 9 neighbours.

#define NUM_LAYERS 3.


float Hash21(vec2 p){
   p = fract (p*vec2(123.34,456.21));
   p+= dot(p,p+45.32);
    return fract(p.x*p.y);
    }


mat2 Rot(float a){

 float s = sin(a), c= cos(a);
   return mat2(c,-s,s,c);
}
float star(vec2 uv){
    float d = length(uv);
    float m = .1/d;
    m *= smoothstep(1.,.01,d);
	return m;
}

vec2 GetPos(vec2 id, vec2 offs){
    		float n = Hash21(id+offs);
    return offs+sin(n)*3.;
}


// the original StarLayer body for one cell: `id` is floor(layer coordinate) - 0.5
vec3 StarCell(vec2 id){
    vec3 col = vec3(0);
    for(int y = -1; y<=1; y++){
        for(int x = -1; x<=1; x++){
            vec2 offs = vec2(x,y);
            float n = Hash21(id+offs);
            float size = fract(n*545.32);
            float st=  star(GetPos(id,offs));
            vec3 color =sin(vec3(1.,0.2,0.9)*fract(n*2345.2)*123.2)*.5+.5;
            color= color*vec3(1.,1.,.5+size);
            st*=sin(iTime*2.+n*6.2831)*.5+1.;
            col+= st*size*color;
        }
    }
    return col;
}

// ----- cell table (not in the original) -----
// One block of TABLE_N x TABLE_N cells per layer, stacked vertically, plus one header row at the top whose texel L holds
// the integer cell (k0.x, k0.y) at the centre of layer L's block. Cell k of layer L is at texel
// (k.x - k0.x + TABLE_N/2, L*TABLE_N + k.y - k0.y + TABLE_N/2).
const float TABLE_N = 128.0;

// the layer coordinate system of the original mainImage, as a function of the layer index i (0, 1/3, 2/3) and the pixel
// position in screen units (the original's uv before it is moved and rotated)
vec2 layerCoord(vec2 uv, float i, float t, out float fade) {
    float move = t*3.;
    uv += vec2(cos(move),sin(move));
    uv *= Rot(t);
    float depth = fract(i+t);
    float scale = mix(20.,.5, depth);
    fade = depth*smoothstep(1.,.9,depth);
    return uv*scale+i*453.2-move;
}
