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

const RECIPE_PATH := "res://spikes/planet_gen/recipe.json"

@export var radius := 3000.0
## Spike 8: every height, mesh and patch comes from this PlanetGen (Rust). Null = build the default one.
var gen: RefCounted
## -1 keeps the recipe's seed.
@export var gen_seed := -1
@export var max_depth := 7  # about 37 m chunks, 1.15 m quads at R = 3 km
@export var split_factor := 1.5
@export var merge_factor := 1.8  # larger than split_factor, avoids flicker
@export var max_uploads_per_frame := 4

var stats: Dictionary
var material: ShaderMaterial
var frozen := false
var water: MeshInstance3D

var _relief := 150.0  # largest |height| over the planet (stamps included), for chunk bounds
var _indices := PackedInt32Array()
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
	var gen: RefCounted  # PlanetGen: build_chunk takes &self, shared across worker threads
	var with_scatter := false
	var task_id := -1
	var arrays: Array
	var center: Vector3
	var scatter: Dictionary
	var build_usec := 0


## One PlanetGen per planet: recipe from the data file, baked here (main thread, about 0.5 s).
static func make_gen(seed_override: int, planet_radius: float) -> RefCounted:
	var g: RefCounted = ClassDB.instantiate("PlanetGen")
	if not g.load_recipe(FileAccess.get_file_as_string(RECIPE_PATH), seed_override, planet_radius):
		push_error("PlanetGen: recipe did not load")
		return null
	g.bake(0)
	return g


func _ready() -> void:
	if gen == null:
		gen = make_gen(gen_seed, radius)
	var relief: Vector2 = gen.height_range()
	_relief = maxf(absf(relief.x), absf(relief.y))
	_build_indices()
	_make_water()
	material = ShaderMaterial.new()
	material.shader = preload("res://spikes/planet/terrain.gdshader")
	material.set_shader_parameter("planet_radius", radius)
	material.set_shader_parameter("sea_level", gen.sea_level())
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


func _exit_tree() -> void:
	# Completed tasks also retain their bound Job until they are waited for.
	# Release them while GDScript and this worker target are still alive.
	for job in _pending:
		WorkerThreadPool.wait_for_task_completion(job.task_id)
	_pending.clear()
	_done.clear()


## Terrain height above the base radius along a unit direction. Same function as the mesh and the patches.
func height_at(dir: Vector3) -> float:
	return gen.height_at(dir)


## Sea level above the base radius (water sphere radius = radius + sea_level).
func sea_level() -> float:
	return gen.sea_level()


func _make_water() -> void:
	var sphere := SphereMesh.new()
	var r: float = radius + gen.sea_level()
	sphere.radius = r
	sphere.height = r * 2.0
	sphere.radial_segments = 128
	sphere.rings = 64
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(0.16, 0.38, 0.52, 0.82)
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	mat.roughness = 0.35
	sphere.material = mat
	water = MeshInstance3D.new()
	water.name = "Water"
	water.mesh = sphere
	water.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(water)


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
	mi.position = job.center  # exactly the centre the vertices are relative to
	mi.set_instance_shader_parameter("lod_color", LOD_COLORS[mini(job.node.depth, LOD_COLORS.size() - 1)])
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
	n.bound = n.edge_m * 0.75 + _relief
	_node_count += 1
	return n


func _make_job(n: ChunkNode) -> Job:
	var job := Job.new()
	job.node = n
	job.gen = gen
	job.with_scatter = n.depth == max_depth  # dressing only on the finest chunks
	return job


func _build_indices() -> void:
	var m := GRID + 3
	_indices.resize((m - 1) * (m - 1) * 6)
	var w := 0
	for j in m - 1:
		for i in m - 1:
			var k00 := j * m + i
			var k10 := k00 + 1
			var k01 := k00 + m
			var k11 := k01 + 1
			_indices[w] = k00
			_indices[w + 1] = k01
			_indices[w + 2] = k10
			_indices[w + 3] = k10
			_indices[w + 4] = k01
			_indices[w + 5] = k11
			w += 6


## Runs on a worker thread. Touches only the job, never the scene tree.
## The vertex grid of (GRID + 3)^2 samples, normals, skirts and colours come from
## PlanetGen.build_chunk (Rust); only the index list (built once) is added here.
func _build_job(job: Job) -> void:
	var t0 := Time.get_ticks_usec()
	var n := job.node
	var d: Dictionary = job.gen.build_chunk(n.face, n.a0, n.b0, n.size, job.with_scatter)
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = d.verts
	arrays[Mesh.ARRAY_NORMAL] = d.normals
	arrays[Mesh.ARRAY_TEX_UV] = d.uvs
	arrays[Mesh.ARRAY_COLOR] = d.colors
	arrays[Mesh.ARRAY_INDEX] = _indices
	job.arrays = arrays
	job.center = d.center
	job.scatter = d.scatter
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
