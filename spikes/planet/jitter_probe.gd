extends Node
## Spike 5: how far the GPU puts things from where they are, in pixels.
## The renderers compute modelview = view * model in float32 on the GPU
## (gles3 scene.glsl:718, forward_clustered scene_forward_clustered.glsl:440
## in 4.7.2-stable). This node repeats that in float32 on the CPU and compares
## it with the same product in double (GDScript floats), for two probes:
## the ground 2 m in front of the camera (nearest terrain chunk) and the ship.
## Calculated, not read back from the GPU.

var main: Node3D

## Worst error since the last reset, in pixels and millimetres.
var ground_px_max := 0.0
var ground_mm_max := 0.0
var ship_px_max := 0.0
var ship_mm_max := 0.0


func reset() -> void:
	ground_px_max = 0.0
	ground_mm_max = 0.0
	ship_px_max = 0.0
	ship_mm_max = 0.0


func _process(_delta: float) -> void:
	var cam := get_viewport().get_camera_3d()
	if cam == null:
		return
	var cx := cam.global_transform
	# Pixels at 1080 lines, so headless runs (64 px viewport) compare too.
	var px_angle := 2.0 * tan(deg_to_rad(cam.fov) * 0.5) / 1080.0

	var chunk := _nearest_chunk(cam.global_position)
	# Only chunks near the camera have geometry there; a far chunk's origin
	# would measure an object that does not exist.
	if chunk and chunk.global_position.distance_to(cam.global_position) < 200.0:
		# Ground point 2 m ahead (along the view, flattened to the planet's up).
		var up: Vector3 = main.to_planet(cam.global_position).normalized()
		var fwd := -cx.basis.z
		fwd = (fwd - up * fwd.dot(up)).normalized()
		var world := cam.global_position + fwd * 2.0 - up * 1.7
		var local := chunk.global_transform.affine_inverse() * world
		var e := _error(cx, chunk.global_transform, local)
		if e.y > 0.0:
			ground_mm_max = maxf(ground_mm_max, e.x * 1000.0)
			ground_px_max = maxf(ground_px_max, e.x / (e.y * px_angle))
	var ship: Node3D = main.ship
	var e2 := _error(cx, ship.global_transform, Vector3(0, 0.7, -2.75))  # nose top
	if e2.y > 0.0:
		ship_mm_max = maxf(ship_mm_max, e2.x * 1000.0)
		ship_px_max = maxf(ship_px_max, e2.x / (e2.y * px_angle))
	main.stats.jitter_ground_px = ground_px_max
	main.stats.jitter_ship_px = ship_px_max


## Returns (screen-plane error in metres, depth in metres; depth 0 = behind
## the camera or closer than 0.5 m, not counted).
func _error(cam: Transform3D, model: Transform3D, v: Vector3) -> Vector2:
	var gpu := (cam.affine_inverse() * model) * v  # float32, like the shader
	var exact := _exact(cam, model, v)
	var dx: float = gpu.x - exact[0]
	var dy: float = gpu.y - exact[1]
	var depth: float = -exact[2]
	return Vector2(sqrt(dx * dx + dy * dy), depth if depth > 0.5 else 0.0)


## R_cam^T * (o_model + R_model * v - o_cam) in double precision.
func _exact(cam: Transform3D, model: Transform3D, v: Vector3) -> Array:
	var w := []
	for i in 3:
		var s: float = model.origin[i] - cam.origin[i]
		s += float(model.basis.x[i]) * v.x + float(model.basis.y[i]) * v.y + float(model.basis.z[i]) * v.z
		w.append(s)
	var out := []
	for axis: Vector3 in [cam.basis.x, cam.basis.y, cam.basis.z]:
		out.append(float(axis.x) * w[0] + float(axis.y) * w[1] + float(axis.z) * w[2])
	return out


func _nearest_chunk(pos: Vector3) -> MeshInstance3D:
	var best: MeshInstance3D = null
	var best_d := INF
	for c in main.terrain.get_children():
		if c is MeshInstance3D and c.visible:
			var d := pos.distance_squared_to(c.global_position)
			if d < best_d:
				best_d = d
				best = c
	return best
