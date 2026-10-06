shader_type spatial;
// Flat-shaded terrain: face normal (from screen-space derivatives) for lighting,
// smooth vertex normal for slope-based colour blending. Planet-space triplanar
// detail, so no tangents are needed anywhere on the sphere.

uniform float planet_radius = 3000.0;
// World position of the planet centre (moves with origin shifts).
uniform vec3 planet_center = vec3(0.0);
// Spike 8: sea level above the base radius; chunk LOD colour as instance uniform.
uniform float sea_level = 0.0;
instance uniform vec4 lod_color : source_color = vec4(1.0);
uniform bool show_lod = false;
uniform bool show_skirts = true;
uniform float detail_scale = 0.5; // checker cells per metre
// How faceted the lighting is: 1 = pure face normal, 0 = smooth.
uniform float flat_strength = 1.0;
// Facets fade to smooth with distance; far away they only read as noise.
uniform float flat_fade_start = 80.0;
uniform float flat_fade_end = 400.0;

varying vec3 planet_pos; // relative to the planet centre
varying vec3 world_normal;
varying float skirt;

void vertex() {
	planet_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz - planet_center;
	world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
	skirt = UV.x;
}

float checker(vec2 p) {
	vec2 c = floor(p);
	return mod(c.x + c.y, 2.0);
}

void fragment() {
	// Interior triangles have skirt == 0 exactly; skirt walls interpolate to 1.
	bool on_skirt = skirt > 0.001;
	if (!show_skirts && on_skirt) {
		discard;
	}
	vec3 up = normalize(planet_pos);
	vec3 sn = normalize(world_normal);
	float slope = 1.0 - clamp(dot(sn, up), 0.0, 1.0);
	float h = length(planet_pos) - planet_radius;

	// Spike 8: flat ground wears the biome tint (vertex colour), steep ground a rock
	// colour mixed with it, a cap above the high ground, sand just above the sea.
	float hs = h - sea_level;
	vec3 tint = COLOR.rgb;
	vec3 rock = mix(vec3(0.50, 0.45, 0.42), tint, 0.25);
	vec3 cap = vec3(0.92, 0.93, 0.96);
	vec3 col = mix(vec3(0.76, 0.70, 0.50), tint, smoothstep(0.0, 3.0, hs));
	col = mix(col, rock, smoothstep(0.12, 0.25, slope));
	col = mix(col, cap, smoothstep(110.0, 140.0, hs) * (1.0 - smoothstep(0.2, 0.35, slope)));

	// Triplanar checker as a motion cue, faded out with distance against aliasing.
	vec3 w = abs(sn);
	w /= (w.x + w.y + w.z);
	vec3 q = planet_pos * detail_scale;
	float c = checker(q.yz) * w.x + checker(q.zx) * w.y + checker(q.xy) * w.z;
	float fade = clamp(length(VERTEX) / 60.0, 0.0, 1.0);
	col *= mix(mix(0.93, 1.0, c), 0.965, fade);

	if (show_lod) {
		col = mix(col, lod_color.rgb, 0.6);
	}
	ALBEDO = col;

	// Skirts keep the smooth normal of their border vertex, so the vertical
	// walls light like the ground they hide a crack in.
	// The face normal comes from screen-space derivatives. On sub-pixel slivers
	// (skirt edges) they are zero and normalize() gives NaN, which survives
	// mix(..., 0) and renders as unlit dark specks. Across triangle edges they
	// can also be wild. So: guard the length, orient by the smooth normal, and
	// fall back to the smooth one when they disagree a lot.
	if (!on_skirt) {
		vec3 smooth_n = normalize(NORMAL);
		vec3 fn = smooth_n;
		vec3 c = cross(dFdx(VERTEX), dFdy(VERTEX));
		float len = length(c);
		if (len > 1e-12) {
			fn = c / len;
			if (dot(fn, smooth_n) < 0.0) {
				fn = -fn;
			}
			if (dot(fn, smooth_n) < 0.5) {
				fn = smooth_n;
			}
		}
		float k = flat_strength * (1.0 - smoothstep(flat_fade_start, flat_fade_end, length(VERTEX)));
		NORMAL = normalize(mix(smooth_n, fn, k));
	}
}
