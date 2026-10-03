extends Node3D
## Terrain collision only near anchors (player, ship): one HeightMapShape3D patch
## per cube-face cell at patch_depth. Each patch lives in its own tangent frame
## and stores heights relative to that frame, so the sphere's curvature is in
## the heights and the 0.42 m flat-plane error does not apply. Patches are
## larger than their cell, so neighbours overlap instead of leaving gaps.
## Heights are built on the WorkerThreadPool from the CPU height function.

const TerrainScript := preload("res://spikes/planet/terrain.gd")
## Cells must stay well below the patch width (31 m) so overlapping patches
## cover them; set from the radius by main (8 at R = 3 km, cells about 18 m).
var patch_depth := 8
const PATCH_SAMPLES := 32  # square power of two, 1 m spacing, 31 m wide
const UPDATE_INTERVAL := 0.2
const MAX_ADDS_PER_FRAME := 16  # spread body creation, avoids frame spikes

@export var ring_radius := 100.0  # start value from the brief (100-300 m)

var terrain: Node3D  # radius, height_amplitude, noise
var stats: Dictionary
## Nodes the ring follows. Each may expose `linear_velocity` or `velocity`.
var anchors: Array[Node3D] = []

var _patches := {}  # Vector3i(face, ia, ib) -> StaticBody3D
var _pending := {}  # key -> Job
var _timer := 0.0
var _build_ms_sum := 0.0
var _build_count := 0
var _build_ms_max := 0.0


class Job:
	extends RefCounted
	var key: Vector3i
	var face: int
	var a0: float
	var b0: float
	var size: float
	var radius: float
	var amplitude: float
	var noise: FastNoiseLite
	var task_id := -1
	var xform: Transform3D
	var heights: PackedFloat32Array
	var build_usec := 0


var _process_ms_max := 0.0


func reset_max_stats() -> void:
	_build_ms_max = 0.0
	_process_ms_max = 0.0


func set_anchors(nodes: Array) -> void:
	anchors.assign(nodes)
	_timer = 0.0  # rebuild the ring right away


func _process(delta: float) -> void:
	var t_frame := Time.get_ticks_usec()
	_collect_jobs()
	_timer -= delta
	if _timer <= 0.0:
		_timer = UPDATE_INTERVAL
		_update_ring()
	stats.collision_patches = _patches.size()
	stats.patches_pending = _pending.size()
	stats.patch_ms_avg = _build_ms_sum / maxi(_build_count, 1)
	stats.patch_ms_max = _build_ms_max
	_process_ms_max = maxf(_process_ms_max, (Time.get_ticks_usec() - t_frame) / 1000.0)
	stats.ring_frame_ms_max = _process_ms_max


## True if a built patch covers this world position (used to tell a real
## fall-through from simply standing outside the ring).
func has_patch_near(pos: Vector3) -> bool:
	for body: StaticBody3D in _patches.values():
		var local := body.global_transform.affine_inverse() * pos
		if absf(local.x) < PATCH_SAMPLES * 0.5 - 1.0 and absf(local.z) < PATCH_SAMPLES * 0.5 - 1.0:
			return true
	return false


func _update_ring() -> void:
	var points: Array[Vector3] = []
	for a in anchors:
		if not is_instance_valid(a) or not a.is_inside_tree():
			continue
		var v: Vector3 = a.linear_velocity if "linear_velocity" in a else a.velocity
		var p := a.global_position - global_position
		for q in [p, p + v * 0.5]:  # now and half a second ahead
			# Only anchors near the ground need collision. Compare on the base
			# sphere, so terrain height does not inflate the ring.
			var dir: Vector3 = q.normalized()
			if q.length() - terrain.radius - terrain.height_at(dir) < ring_radius:
				points.append(dir * terrain.radius)

	var want := {}  # key -> [face, a0, b0, size, min_dist]
	for face in 6:
		_collect(face, -1.0, -1.0, 2.0, 0, points, want)

	for key in want:
		var cell: Array = want[key]
		if cell[4] < ring_radius and not _patches.has(key) and not _pending.has(key):
			_start_job(key, cell)
	for key in _patches.keys():
		if not want.has(key):  # want uses 1.3 x radius: hysteresis
			_patches[key].queue_free()
			_patches.erase(key)


func _collect(face: int, a0: float, b0: float, size: float, depth: int, points: Array[Vector3], out: Dictionary) -> void:
	var r: float = terrain.radius
	var center := TerrainScript.cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5) * r
	var edge := (TerrainScript.cube_to_sphere(face, a0, b0) - TerrainScript.cube_to_sphere(face, a0 + size, b0)).length() * r
	var bound := edge * 0.75
	var best := INF
	for p in points:
		best = minf(best, maxf(0.0, p.distance_to(center) - bound))
	if best > ring_radius * 1.3:
		return
	if depth == patch_depth:
		var key := Vector3i(face, roundi((a0 + 1.0) / size), roundi((b0 + 1.0) / size))
		out[key] = [face, a0, b0, size, best]
		return
	var half := size * 0.5
	for j in 2:
		for i in 2:
			_collect(face, a0 + i * half, b0 + j * half, half, depth + 1, points, out)


func _start_job(key: Vector3i, cell: Array) -> void:
	var job := Job.new()
	job.key = key
	job.face = cell[0]
	job.a0 = cell[1]
	job.b0 = cell[2]
	job.size = cell[3]
	job.radius = terrain.radius
	job.amplitude = terrain.height_amplitude
	job.noise = terrain.noise.duplicate()
	job.task_id = WorkerThreadPool.add_task(_build_job.bind(job), false, "collision patch")
	_pending[key] = job


func _collect_jobs() -> void:
	var adds := 0
	for key in _pending.keys():
		if adds >= MAX_ADDS_PER_FRAME:
			break
		var job: Job = _pending[key]
		if not WorkerThreadPool.is_task_completed(job.task_id):
			continue
		adds += 1
		WorkerThreadPool.wait_for_task_completion(job.task_id)
		_pending.erase(key)
		var ms := job.build_usec / 1000.0
		_build_ms_sum += ms
		_build_count += 1
		_build_ms_max = maxf(_build_ms_max, ms)

		var shape := HeightMapShape3D.new()
		shape.map_width = PATCH_SAMPLES
		shape.map_depth = PATCH_SAMPLES
		shape.map_data = job.heights
		var col := CollisionShape3D.new()
		col.shape = shape
		var body := StaticBody3D.new()
		body.add_child(col)
		body.transform = job.xform
		add_child(body)
		_patches[key] = body


## Worker thread. For each grid point (x, z) of the tangent frame, find the
## height y where the vertical line through it meets the terrain:
## |C + xT + zB + y*up| = R + h(dir)  =>  y = sqrt(r^2 - x^2 - z^2) - R.
## Two iterations are enough because h changes slowly along the line.
func _build_job(job: Job) -> void:
	var t0 := Time.get_ticks_usec()
	var r := job.radius
	var mid := job.size * 0.5
	var up := TerrainScript.cube_to_sphere(job.face, job.a0 + mid, job.b0 + mid)
	var east := TerrainScript.cube_to_sphere(job.face, job.a0 + job.size, job.b0 + mid) \
		- TerrainScript.cube_to_sphere(job.face, job.a0, job.b0 + mid)
	var t := (east - up * east.dot(up)).normalized()
	var b := t.cross(up)
	var c := up * r
	job.xform = Transform3D(Basis(t, up, b), c)

	var n := PATCH_SAMPLES
	var half := (n - 1) * 0.5
	job.heights = PackedFloat32Array()
	job.heights.resize(n * n)
	for j in n:
		var z := j - half
		for i in n:
			var x := i - half
			var flat := c + t * x + b * z
			var y := 0.0
			for _iter in 2:
				var dir := (flat + up * y).normalized()
				var surface := r + job.noise.get_noise_3dv(dir * r) * job.amplitude
				y = sqrt(surface * surface - x * x - z * z) - r
			job.heights[j * n + i] = y
	job.build_usec = Time.get_ticks_usec() - t0
