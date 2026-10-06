extends SceneTree
## Spike 8, T1 scene part: the meshes the terrain really built and the collision patches the
## ring really made agree with PlanetGen.height_at to within 1 mm.
## Run: godot --headless --path . --script res://spikes/planet_gen/scene_test.gd (no --fixed-fps: the settle loop counts frames, the workers need wall time)
## Arithmetic is done on GDScript floats (64 bit) from the stored 32-bit vertex values.

const Terrain := preload("res://spikes/planet/terrain.gd")
const Ring := preload("res://spikes/planet/collision_ring.gd")
const TOLERANCE := 0.001


func _initialize() -> void:
	_run.call_deferred()


func _len(x: float, y: float, z: float) -> float:
	return sqrt(x * x + y * y + z * z)


func _run() -> void:
	var world := Node3D.new()
	root.add_child(world)
	var radius := 5000.0
	var terrain: Node3D = Terrain.new()
	terrain.radius = radius
	terrain.max_depth = maxi(1, roundi(log(radius * PI * 0.5 / 37.0) / log(2.0)))
	terrain.stats = {}
	world.add_child(terrain)
	var gen: RefCounted = terrain.gen
	# near-ground camera and anchor: a finest-level chunk that has trees, so the dressing path is exercised
	var dir0 := Vector3(0.3, 1.0, 0.2).normalized()
	var rng := RandomNumberGenerator.new()
	rng.seed = 7
	var n_fine: int = 1 << int(terrain.max_depth)
	for _i in 2000:
		var sz: float = 2.0 / n_fine
		var d: Dictionary = gen.build_chunk(rng.randi() % 6, -1.0 + (rng.randi() % n_fine) * sz, -1.0 + (rng.randi() % n_fine) * sz, sz, true)
		if d.scatter.has("canopy") and d.scatter.canopy.count >= 6:
			dir0 = (d.center as Vector3).normalized()
			break
	var camera := Camera3D.new()
	world.add_child(camera)
	camera.position = dir0 * (radius + gen.height_at(dir0) + 2.0)
	camera.make_current()
	var anchor := CharacterBody3D.new()
	anchor.position = camera.position
	world.add_child(anchor)
	var ring := Ring.new()
	ring.terrain = terrain
	ring.stats = {}
	ring.patch_depth = ceili(log(radius * PI * 0.5 / 20.0) / log(2.0))
	ring.set_anchors([anchor])
	world.add_child(ring)

	# let the LOD and the worker jobs settle: nothing pending for 60 frames in a row
	var calm := 0
	for _i in 1200:
		await process_frame
		var idle: bool = terrain._pending.is_empty() and terrain._done.is_empty() and ring._pending.is_empty()
		calm = calm + 1 if idle else 0
		if calm >= 60 and _i > 120:
			print("settled after %d frames" % _i)
			break

	var by_depth := {}
	var stack: Array = terrain._roots.duplicate()
	while not stack.is_empty():
		var nd = stack.pop_back()
		by_depth[nd.depth] = by_depth.get(nd.depth, 0) + 1
		stack.append_array(nd.children)
	print("chunk nodes by depth ", by_depth, ", camera dir ", dir0)

	# ---- meshes
	var worst_mesh := 0.0
	var n_vert := 0
	var n_mesh := 0
	var depths := {}
	for mi: MeshInstance3D in terrain.get_children().filter(func(c): return c is MeshInstance3D and c != terrain.water and not c.name.begins_with("SiteMarker")):
		var arrays := mi.mesh.surface_get_arrays(0)
		var verts: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
		var uvs: PackedVector2Array = arrays[Mesh.ARRAY_TEX_UV]
		var c := mi.position
		n_mesh += 1
		for k in verts.size():
			if uvs[k].x > 0.0:
				continue  # skirt
			var x := float(c.x) + float(verts[k].x)
			var y := float(c.y) + float(verts[k].y)
			var z := float(c.z) + float(verts[k].z)
			var l := _len(x, y, z)
			var h := l - radius
			var dir := Vector3(x / l, y / l, z / l)
			var err := absf(h - gen.height_at_xyz(x, y, z))
			worst_mesh = maxf(worst_mesh, err)
			n_vert += 1
	print("T1 scene meshes: %d chunk meshes, %d vertices, max |mesh height - height_at| = %.6f m" % [n_mesh, n_vert, worst_mesh])

	var mm_nodes := 0
	var mm_inst := 0
	for mi: MeshInstance3D in terrain.get_children().filter(func(c): return c is MeshInstance3D and c != terrain.water and not c.name.begins_with("SiteMarker")):
		for c in mi.get_children():
			if c is MultiMeshInstance3D:
				mm_nodes += 1
				mm_inst += c.multimesh.instance_count
	print("dressing: %d MultiMeshInstance3D nodes, %d instances under the visible-or-hidden chunk meshes; %d site markers" % [mm_nodes, mm_inst, terrain.get_children().filter(func(c): return c.name.begins_with("SiteMarker")).size()])

	# ---- collision patches
	var worst_patch := 0.0
	var worst_at := ""
	var worst_redo := 0.0
	var worst_quant := 0.0
	var n_patch := 0
	var n_samples := 0
	for body: StaticBody3D in ring._patches.values():
		var shape: HeightMapShape3D = body.get_child(0).shape
		var data := shape.map_data
		var tr := body.transform
		var w := shape.map_width
		var half := (w - 1) * 0.5
		var again: PackedFloat32Array = gen.patch_heights(tr.basis.y, tr.basis.x, tr.basis.z, w)
		for q in again.size():
			worst_redo = maxf(worst_redo, absf(again[q] - data[q]))
		n_patch += 1
		for j in w:
			for i in w:
				var lx := i - half
				var lz := j - half
				var ly := float(data[j * w + i])
				# frame centre exactly up * R (as PlanetGen used it), not the 32-bit body origin
				var ox: float = tr.basis.y.x * radius
				var oy: float = tr.basis.y.y * radius
				var oz: float = tr.basis.y.z * radius
				var px: float = ox + tr.basis.x.x * lx + tr.basis.y.x * ly + tr.basis.z.x * lz
				var py: float = oy + tr.basis.x.y * lx + tr.basis.y.y * ly + tr.basis.z.y * lz
				var pz: float = oz + tr.basis.x.z * lx + tr.basis.y.z * ly + tr.basis.z.z * lz
				var l := _len(px, py, pz)
				var err := absf(l - radius - gen.height_at_xyz(px, py, pz))
				# the same with the body's 32-bit origin (what the physics server sees)
				var qx: float = tr.origin.x + px - ox
				var qy: float = tr.origin.y + py - oy
				var qz: float = tr.origin.z + pz - oz
				worst_quant = maxf(worst_quant, absf(_len(qx, qy, qz) - radius - gen.height_at_xyz(qx, qy, qz)))
				if err > worst_patch:
					worst_patch = err
					worst_at = "i %d j %d ly %.3f" % [i, j, ly]
				n_samples += 1
	print("T1 scene collision: %d patches, %d samples, max |patch height - height_at| = %.6f m" % [n_patch, n_samples, worst_patch])

	print("  worst patch sample: ", worst_at, "  rebuilt-with-same-basis diff ", worst_redo, "  (with the 32-bit body origin: max error %.6f m)" % worst_quant)
	var ok := n_mesh > 0 and n_patch > 0 and worst_mesh < TOLERANCE and worst_patch < TOLERANCE
	print("%s T1 scene part (tolerance %.0f mm)" % ["PASS" if ok else "FAIL", TOLERANCE * 1000.0])
	world.free()
	quit(0 if ok else 1)
