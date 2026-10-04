extends RefCounted
## Arcade planetary influence: full inside atmosphere, smoothly absent in space.
## Heights are metres above the reference sphere, independent of terrain.

static func strength(altitude: float, atmosphere_height: float, end_height: float) -> float:
	# Keep the interval valid even if provisional exported values are misordered.
	var end := maxf(end_height, atmosphere_height + 1.0)
	return 1.0 - smoothstep(atmosphere_height, end, altitude)
