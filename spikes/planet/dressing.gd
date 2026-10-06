extends RefCounted
## Spike 8: placeholder dressing built in code (original, no assets): a cone-and-cylinder
## tree, a low-poly rock, a tall pillar as site marker. Meshes are made once and shared by all
## MultiMeshInstance3D nodes. Instance colour tints the foliage and the rocks per biome.

static var _meshes := {}


static func mesh_for(kind: String) -> Mesh:
	if not _meshes.has(kind):
		match kind:
			"canopy":
				_meshes[kind] = _tree()
			"rocks":
				_meshes[kind] = _rock()
			_:
				_meshes[kind] = _rock()
	return _meshes[kind]


static func _shifted(arrays: Array, offset: Vector3, scale := Vector3.ONE) -> Array:
	var verts: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var normals: PackedVector3Array = arrays[Mesh.ARRAY_NORMAL]
	for i in verts.size():
		verts[i] = verts[i] * scale + offset
		normals[i] = (normals[i] / scale).normalized()
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_NORMAL] = normals
	return arrays


static func _tree() -> ArrayMesh:
	var mesh := ArrayMesh.new()
	var trunk := CylinderMesh.new()
	trunk.top_radius = 0.12
	trunk.bottom_radius = 0.2
	trunk.height = 1.4
	trunk.radial_segments = 5
	trunk.rings = 1
	var trunk_mat := StandardMaterial3D.new()
	trunk_mat.albedo_color = Color(0.36, 0.26, 0.18)
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, _shifted(trunk.get_mesh_arrays(), Vector3(0, 0.7, 0)))
	mesh.surface_set_material(0, trunk_mat)
	var cone := CylinderMesh.new()
	cone.top_radius = 0.0
	cone.bottom_radius = 1.3
	cone.height = 3.6
	cone.radial_segments = 6
	cone.rings = 1
	var leaf_mat := StandardMaterial3D.new()
	leaf_mat.vertex_color_use_as_albedo = true  # multimesh instance colour
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, _shifted(cone.get_mesh_arrays(), Vector3(0, 1.4 + 1.8, 0)))
	mesh.surface_set_material(1, leaf_mat)
	return mesh


static func _rock() -> ArrayMesh:
	var s := SphereMesh.new()
	s.radius = 0.7
	s.height = 1.4
	s.radial_segments = 5
	s.rings = 3
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, _shifted(s.get_mesh_arrays(), Vector3(0, 0.15, 0), Vector3(1.0, 0.6, 0.85)))
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.roughness = 1.0
	mesh.surface_set_material(0, mat)
	return mesh


## One MultiMeshInstance3D per kind and chunk. `buffer` is the MultiMesh layout from
## PlanetGen.build_chunk (16 floats per instance: 3x4 transform rows, RGBA), relative to the chunk
## centre, so the node sits at the chunk mesh's origin as its child.
static func make_instances(kind: String, entry: Dictionary, aabb: AABB) -> MultiMeshInstance3D:
	var mm := MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.use_colors = true
	mm.mesh = mesh_for(kind)
	mm.instance_count = entry.count
	mm.buffer = entry.buffer
	mm.custom_aabb = aabb
	var mmi := MultiMeshInstance3D.new()
	mmi.multimesh = mm
	mmi.name = kind
	mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	return mmi


## Site marker: a plain tall pillar standing on the ground at `dir`, one distinct colour.
static func make_marker(radius: float, ground_height: float, dir: Vector3) -> MeshInstance3D:
	var pillar := CylinderMesh.new()
	pillar.top_radius = 0.6
	pillar.bottom_radius = 0.9
	pillar.height = 24.0
	pillar.radial_segments = 8
	pillar.rings = 1
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(0.95, 0.45, 0.15)
	mat.emission_enabled = true
	mat.emission = Color(0.95, 0.45, 0.15)
	mat.emission_energy_multiplier = 0.4
	pillar.material = mat
	var mi := MeshInstance3D.new()
	mi.mesh = pillar
	mi.name = "SiteMarker"
	var up := dir.normalized()
	var fwd := Vector3.FORWARD - up * Vector3.FORWARD.dot(up)
	if fwd.length_squared() < 1e-6:
		fwd = Vector3.RIGHT - up * Vector3.RIGHT.dot(up)
	fwd = fwd.normalized()
	mi.transform = Transform3D(Basis(fwd.cross(up), up, -fwd), up * (radius + ground_height + 10.0))
	return mi
