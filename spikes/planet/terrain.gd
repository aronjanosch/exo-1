extends Node3D
## Cube-sphere terrain with a quadtree LOD per cube face.
## Chunk arrays are built on the WorkerThreadPool; the main thread only uploads
## finished arrays into meshes (a few per frame). Skirts hide LOD cracks.
## Debug keys: 1 LOD colours, 2 skirts on/off, 3 freeze LOD, 4 reset max stats,
## 5 cycle flat-shading strength (1, 0.6, 0.3, 0).

const GRID := 32  # quads per chunk side
const FACE_NORMALS: Array[Vector3] = [
	Vector3(1, 0, 0), Vector3(-1, 0, 0),
	Vector3(0, 1, 0), Vector3(0, -1, 0),
	Vector3(0, 0, 1), Vector3(0, 0, -1),
]
const LOD_COLORS: Array[Color] = [
	Color(0.9, 0.1, 0.1), Color(0.9, 0.5, 0.1), Color(0.9, 0.9, 0.1), Color(0.1, 0.9, 0.1),
	Color(0.1, 0.9, 0.9), Color(0.1, 0.3, 0.9), Color(0.6, 0.1, 0.9), Color(0.9, 0.1, 0.7),
	Color(1, 1, 1),
]

@export var radius := 3000.0
@export var noise_seed := 1
@export var height_amplitude := 150.0  # start value, tune by feel
@export var noise_frequency := 0.0008  # largest features about 1.2 km
@export var max_depth := 7  # about 37 m chunks, 1.15 m quads at R = 3 km
@export var split_factor := 1.5
@export var merge_factor := 1.8  # larger than split_factor, avoids flicker
@export var max_uploads_per_frame := 4

var stats: Dictionary
var noise: FastNoiseLite
var material: ShaderMaterial
var frozen := false

var _roots: Array[ChunkNode] = []
var _pending: Array[Job] = []
var _done: Array[Job] = []
var _visible_count := 0
var _node_count := 0
var _build_ms_sum := 0.0
var _build_count := 0
var _build_ms_max := 0.0
var _upload_ms_max := 0.0
var _lod_ms_max := 0.0


class ChunkNode:
	extends RefCounted
	var face: int
	var a0: float
	var b0: float
	var size: float  # in cube coordinates, the whole face is 2
	var depth: int
	var center: Vector3  # on the base sphere, planet space
	var edge_m: float
	var bound: float
	var children: Array[ChunkNode] = []
	var mesh_instance: MeshInstance3D
	var discarded := false


class Job:
	extends RefCounted
	var node: ChunkNode
	var noise: FastNoiseLite  # own copy per job, never shared across threads
	var radius: float
	var amplitude: float
	var skirt_depth: float
	var color: Color
	var task_id := -1
	var arrays: Array
	var build_usec := 0


func _ready() -> void:
	noise = _make_noise()
	material = ShaderMaterial.new()
	material.shader = preload("res://spikes/planet/terrain.gdshader")
	material.set_shader_parameter("planet_radius", radius)
	material.set_shader_parameter("show_lod", false)
	material.set_shader_parameter("show_skirts", true)
	material.set_shader_parameter("flat_strength", 1.0)

	for face in 6:
		var root := _make_node(face, -1.0, -1.0, 2.0, 0)
		var job := _make_job(root)
		_build_job(job)  # roots synchronously, so there is always a planet
		_record_build(job)
		_upload(job)
		_roots.append(root)


## Terrain height above the base radius along a unit direction (CPU, main thread).
func height_at(dir: Vector3) -> float:
	return noise.get_noise_3dv(dir * radius) * height_amplitude


func _unhandled_input(event: InputEvent) -> void:
	if not (event is InputEventKey and event.pressed and not event.echo):
		return
	match event.physical_keycode:
		KEY_1:
			var on: bool = not material.get_shader_parameter("show_lod")
			material.set_shader_parameter("show_lod", on)
		KEY_2:
			var on: bool = not material.get_shader_parameter("show_skirts")
			material.set_shader_parameter("show_skirts", on)
		KEY_3:
			frozen = not frozen
		KEY_4:
			reset_max_stats()
		KEY_5:
			var steps := [1.0, 0.6, 0.3, 0.0]
			var cur: float = material.get_shader_parameter("flat_strength")
			var next: float = steps[(steps.find(cur) + 1) % steps.size()]
			material.set_shader_parameter("flat_strength", next)
			stats.flat_strength = next


var _process_ms_max := 0.0


func reset_max_stats() -> void:
	_process_ms_max = 0.0
	_build_ms_max = 0.0
	_upload_ms_max = 0.0
	_lod_ms_max = 0.0


func _process(_delta: float) -> void:
	var t_frame := Time.get_ticks_usec()
	_collect_jobs()
	var uploads := 0
	while uploads < max_uploads_per_frame and not _done.is_empty():
		var job: Job = _done.pop_front()
		if not job.node.discarded:
			_upload(job)
			uploads += 1

	var camera := get_viewport().get_camera_3d()  # player or ship, whichever is active
	if not frozen and camera:
		var t0 := Time.get_ticks_usec()
		var cam := camera.global_position - global_position
		_visible_count = 0
		for root in _roots:
			_update_node(root, cam)
		_lod_ms_max = maxf(_lod_ms_max, (Time.get_ticks_usec() - t0) / 1000.0)

	stats.chunks_visible = _visible_count
	stats.chunks_total = _node_count
	stats.chunks_pending = _pending.size() + _done.size()
	stats.build_ms_avg = _build_ms_sum / maxi(_build_count, 1)
	stats.build_ms_max = _build_ms_max
	stats.upload_ms_max = _upload_ms_max
	stats.lod_ms_max = _lod_ms_max
	stats.lod_frozen = frozen
	_process_ms_max = maxf(_process_ms_max, (Time.get_ticks_usec() - t_frame) / 1000.0)
	stats.terrain_frame_ms_max = _process_ms_max


func _update_node(n: ChunkNode, cam: Vector3) -> void:
	var dist := maxf(0.0, cam.distance_to(n.center) - n.bound)
	var factor := merge_factor if not n.children.is_empty() else split_factor
	var want_split := n.depth < max_depth and dist < n.edge_m * factor
	if want_split:
		if n.children.is_empty():
			_split(n)
		var children_ready := true
		for c in n.children:
			if c.mesh_instance == null:
				children_ready = false
		if children_ready:
			n.mesh_instance.visible = false
			for c in n.children:
				_update_node(c, cam)
			return
		# Children still building: keep the parent on screen, no holes.
	elif not n.children.is_empty():
		for c in n.children:
			_discard(c)
		n.children.clear()
	n.mesh_instance.visible = true
	_visible_count += 1


func _split(n: ChunkNode) -> void:
	var half := n.size * 0.5
	for j in 2:
		for i in 2:
			var c := _make_node(n.face, n.a0 + i * half, n.b0 + j * half, half, n.depth + 1)
			n.children.append(c)
			var job := _make_job(c)
			job.task_id = WorkerThreadPool.add_task(_build_job.bind(job), false, "terrain chunk")
			_pending.append(job)


func _discard(n: ChunkNode) -> void:
	n.discarded = true
	_node_count -= 1
	if n.mesh_instance:
		n.mesh_instance.queue_free()
		n.mesh_instance = null
	for c in n.children:
		_discard(c)
	n.children.clear()


func _collect_jobs() -> void:
	var i := 0
	while i < _pending.size():
		var job := _pending[i]
		if WorkerThreadPool.is_task_completed(job.task_id):
			WorkerThreadPool.wait_for_task_completion(job.task_id)
			_pending.remove_at(i)
			_record_build(job)
			if not job.node.discarded:
				_done.append(job)
		else:
			i += 1


func _record_build(job: Job) -> void:
	var ms := job.build_usec / 1000.0
	_build_ms_sum += ms
	_build_count += 1
	_build_ms_max = maxf(_build_ms_max, ms)


func _upload(job: Job) -> void:
	var t0 := Time.get_ticks_usec()
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, job.arrays)
	mesh.surface_set_material(0, material)
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	mi.position = job.node.center
	mi.visible = false
	add_child(mi)
	job.node.mesh_instance = mi
	job.arrays = []
	_upload_ms_max = maxf(_upload_ms_max, (Time.get_ticks_usec() - t0) / 1000.0)


func _make_node(face: int, a0: float, b0: float, size: float, depth: int) -> ChunkNode:
	var n := ChunkNode.new()
	n.face = face
	n.a0 = a0
	n.b0 = b0
	n.size = size
	n.depth = depth
	n.center = cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5) * radius
	n.edge_m = (cube_to_sphere(face, a0, b0) - cube_to_sphere(face, a0 + size, b0)).length() * radius
	n.bound = n.edge_m * 0.75 + height_amplitude
	_node_count += 1
	return n


func _make_job(n: ChunkNode) -> Job:
	var job := Job.new()
	job.node = n
	job.noise = noise.duplicate()
	job.radius = radius
	job.amplitude = height_amplitude
	job.skirt_depth = maxf(2.0, n.edge_m / GRID * 4.0)
	job.color = LOD_COLORS[mini(n.depth, LOD_COLORS.size() - 1)]
	return job


func _make_noise() -> FastNoiseLite:
	var n := FastNoiseLite.new()
	n.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
	n.seed = noise_seed
	n.frequency = noise_frequency
	n.fractal_type = FastNoiseLite.FRACTAL_FBM
	n.fractal_octaves = 6
	return n


## Runs on a worker thread. Touches only the job, never the scene tree.
## Grid of (GRID + 3)^2 samples: the outer ring is used for normals and becomes
## the skirt (border vertex pushed down towards the centre).
func _build_job(job: Job) -> void:
	var t0 := Time.get_ticks_usec()
	var n := job.node
	var m := GRID + 3
	var step := n.size / GRID
	var pos := PackedVector3Array()
	var dirs := PackedVector3Array()
	pos.resize(m * m)
	dirs.resize(m * m)
	for j in m:
		for i in m:
			var d := cube_to_sphere(n.face, n.a0 + (i - 1) * step, n.b0 + (j - 1) * step)
			var h := job.noise.get_noise_3dv(d * job.radius) * job.amplitude
			pos[j * m + i] = d * (job.radius + h)
			dirs[j * m + i] = d

	var verts := PackedVector3Array()
	var normals := PackedVector3Array()
	var uvs := PackedVector2Array()
	var colors := PackedColorArray()
	verts.resize(m * m)
	normals.resize(m * m)
	uvs.resize(m * m)
	colors.resize(m * m)
	colors.fill(job.color)
	for j in m:
		for i in m:
			var k := j * m + i
			var ck := clampi(j, 1, m - 2) * m + clampi(i, 1, m - 2)
			var nrm := (pos[ck + 1] - pos[ck - 1]).cross(pos[ck + m] - pos[ck - m]).normalized()
			if nrm.dot(dirs[ck]) < 0.0:
				nrm = -nrm
			normals[k] = nrm
			if k == ck:
				verts[k] = pos[k] - n.center
			else:
				verts[k] = pos[ck] - dirs[ck] * job.skirt_depth - n.center
				uvs[k] = Vector2(1, 0)  # skirt flag for the shader

	var indices := PackedInt32Array()
	indices.resize((m - 1) * (m - 1) * 6)
	var w := 0
	for j in m - 1:
		for i in m - 1:
			var k00 := j * m + i
			var k10 := k00 + 1
			var k01 := k00 + m
			var k11 := k01 + 1
			indices[w] = k00
			indices[w + 1] = k01
			indices[w + 2] = k10
			indices[w + 3] = k10
			indices[w + 4] = k01
			indices[w + 5] = k11
			w += 6

	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_TEX_UV] = uvs
	arrays[Mesh.ARRAY_COLOR] = colors
	arrays[Mesh.ARRAY_INDEX] = indices
	job.arrays = arrays
	job.build_usec = Time.get_ticks_usec() - t0


## Face axes are chosen so that u x v = face normal on every face, so one
## triangle winding works everywhere. Spherified-cube mapping keeps chunk sizes
## more even than plain normalisation.
static func cube_to_sphere(face: int, a: float, b: float) -> Vector3:
	var nrm := FACE_NORMALS[face]
	var u := Vector3(nrm.y, nrm.z, nrm.x)
	var v := nrm.cross(u)
	var p := nrm + u * a + v * b
	var x2 := p.x * p.x
	var y2 := p.y * p.y
	var z2 := p.z * p.z
	return Vector3(
		p.x * sqrt(1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0),
		p.y * sqrt(1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0),
		p.z * sqrt(1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0))
