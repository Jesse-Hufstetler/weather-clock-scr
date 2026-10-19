// "Screensaver squares" by julianlumia - https://www.shadertoy.com/view/Wdcyz7 (2020-09-20)
// "small change after following the tutorial by bigwings" - full credits to BigWings (Martijn Steinrucken, The Art of Code):
// https://www.youtube.com/watch?v=rvDo9LvfoVE
// License: no license stated by the author, so Shadertoy's default applies:
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 (CC BY-NC-SA 3.0).
// Only change: the local variable `star` in StarLayer (which shadowed the function `star`) is renamed `st`,
// since not every desktop GLSL compiler accepts a variable named like the function it is initialised from.

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


vec3 StarLayer( vec2 uv){
    vec3 col = vec3(0);
 	vec2 id = floor(uv)-0.5;

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


void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    vec2 uv = (fragCoord.xy-.5*iResolution.xy)/iResolution.y;
    uv*=1.;
    float t = iTime*.05;
    float move = t*3.;
    uv+= vec2(cos(move),sin(move));

    uv *= Rot(t);
      vec3 col = vec3(0.);


    for (float i=0.; i<1.; i+=1./NUM_LAYERS){
        float depth = fract(i+t);
  		float scale = mix(20.,.5, depth);
        float fade = depth*smoothstep(1.,.9,depth);
        col += StarLayer(uv*scale+i*453.2-move)*fade;
    }


    fragColor = vec4(col,1.0);
}
