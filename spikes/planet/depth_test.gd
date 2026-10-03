extends Node3D
## Depth-buffer check for spike question 4 (near 0.05 m, far 50 km).
## Pairs of camera-facing quads, red in front, green behind, at several
## distances and gaps. Prints the share of green pixels inside each red quad
## (z-fighting) and saves a screenshot. The camera sits 3000 m from the origin,
## like a player on the planet surface.
## Run: godot --path . res://spikes/planet/depth_test.tscn [--rendering-method forward_plus]

const DISTANCES: Array[float] = [10.0, 100.0, 500.0, 2000.0, 10000.0, 40000.0]
const GAPS: Array[float] = [0.01, 0.1, 1.0]
const ORIGIN := Vector3(0, 3000, 0)

var _camera: Camera3D


func _ready() -> void:
	_camera = Camera3D.new()
	_camera.near = 0.05
	_camera.far = 50000.0
	_camera.fov = 75.0
	_camera.position = ORIGIN
	add_child(_camera)
	_camera.make_current()

	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color.BLACK
	var we := WorldEnvironment.new()
	we.environment = env
	add_child(we)

	await get_tree().process_frame
	var tiles := _layout()
	for t in tiles:
		_add_quad(t.dir, t.dist, t.size, Color.RED)
		_add_quad(t.dir, t.dist + t.gap, t.size * 1.2, Color.GREEN)

	for i in 10:
		await get_tree().process_frame
	var img := get_viewport().get_texture().get_image()
	var method := RenderingServer.get_current_rendering_method()
	DirAccess.make_dir_recursive_absolute("user://screenshots")
	img.save_png("user://screenshots/depth-test-%s.png" % method)

	var lines: PackedStringArray = ["depth test, renderer %s, near %.2f far %.0f, camera %.0f m from origin" % [
		method, _camera.near, _camera.far, ORIGIN.length()]]
	lines.append("green share inside the red quad (0 = clean, ~0.5 = full z-fighting)")
	var header := "gap \\ dist"
	for d in DISTANCES:
		header += "%10.0f m" % d
	lines.append(header)
	var k := 0
	for g in GAPS:
		var row := "%8.2f m " % g
		for d in DISTANCES:
			row += "%12.3f" % _green_share(img, tiles[k].rect)
			k += 1
		lines.append(row)
	print("\n".join(lines))
	get_tree().quit()


## One screen tile per (gap, distance): rows are gaps, columns distances.
func _layout() -> Array:
	var vp := get_viewport().get_visible_rect().size
	var cols := DISTANCES.size()
	var rows := GAPS.size()
	var tile := Vector2(vp.x / cols, vp.y / rows)
	var tiles := []
	for r in rows:
		for c in cols:
			var center := Vector2((c + 0.5) * tile.x, (r + 0.5) * tile.y)
			var inner := tile * 0.25  # sample the middle of the red quad only
			var d := DISTANCES[c]
			# World size so the red quad covers about 60% of the tile.
			var a := _camera.project_position(center - tile * 0.3, d)
			var b := _camera.project_position(center + tile * 0.3, d)
			var size := Vector2(absf((b - a).dot(_camera.global_basis.x)), absf((b - a).dot(_camera.global_basis.y)))
			tiles.append({
				"dir": _camera.project_ray_normal(center),
				"dist": d,
				"gap": GAPS[r],
				"size": size,
				"rect": Rect2i(Vector2i(center - inner * 0.5), Vector2i(inner)),
			})
	return tiles


## Quad facing the camera, centred on the ray, at `dist` along the view axis.
func _add_quad(dir: Vector3, dist: float, size: Vector2, color: Color) -> void:
	var mesh := QuadMesh.new()
	mesh.size = size
	var mat := StandardMaterial3D.new()
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.albedo_color = color
	mesh.material = mat
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	add_child(mi)
	var forward := -_camera.global_basis.z
	var t := dist / dir.dot(forward)  # depth along the view axis is `dist`
	mi.global_transform = Transform3D(_camera.global_basis, _camera.global_position + dir * t)


func _green_share(img: Image, rect: Rect2i) -> float:
	var green := 0
	var total := 0
	for y in range(rect.position.y, rect.end.y, 2):
		for x in range(rect.position.x, rect.end.x, 2):
			var c := img.get_pixel(x, y)
			if c.g > 0.5 and c.r < 0.5:
				green += 1
			total += 1
	return float(green) / maxi(total, 1)
