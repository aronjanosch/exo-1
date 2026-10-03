shader_type sky;
// Planet-aware sky: "up" and the horizon come from the camera's position on
// the planet, not from world Y. Blends from a blue day sky to black space with
// stars as the camera leaves the atmosphere.

uniform vec3 planet_up = vec3(0.0, 1.0, 0.0);
uniform float atmosphere = 1.0; // density at the camera, 0..1
uniform float horizon_sin = 0.0; // sin of the horizon dip angle
uniform vec3 zenith_color : source_color = vec3(0.24, 0.44, 0.82);
uniform vec3 horizon_color : source_color = vec3(0.72, 0.82, 0.95);
uniform vec3 space_color : source_color = vec3(0.0, 0.0, 0.012);

float hash(vec3 p) {
	p = fract(p * 0.3183099 + 0.1);
	p *= 17.0;
	return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

void sky() {
	// Elevation above the true (dipped) horizon.
	float e = dot(EYEDIR, planet_up) + horizon_sin;
	vec3 day = mix(horizon_color, zenith_color, smoothstep(0.0, 0.45, e));

	float star = step(0.9985, hash(floor(EYEDIR * 400.0)));
	vec3 space = space_color + vec3(star) * 0.9;

	vec3 col = mix(space, day, atmosphere);
	float sun = smoothstep(0.9994, 0.9997, dot(EYEDIR, LIGHT0_DIRECTION));
	col += vec3(1.0, 0.95, 0.8) * sun * 4.0;
	COLOR = col;
}
