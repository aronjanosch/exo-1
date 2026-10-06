extends Node
## Spike 8: scripted screenshots (needs a rendering device): four orbit views, the basin shore, the
## escarpment, the plateau edge, one forest edge, one site marker. PNGs go to
## spikes/planet_gen/shots/. Run: godot --path . -- --planet-shots   (under xvfb-run with a GL driver
## if there is no display, see REPORT.md).

var main: Node3D
var overlay: CanvasLayer
var _taken: PackedStringArray = []


func _ready() -> void:
	_run.call_deferred()


func _settle(max_frames := 1500) -> void:
	var calm := 0
	for _i in max_frames:
		await get_tree().process_frame
		var idle: bool = main.terrain._pending.is_empty() and main.terrain._done.is_empty() and main.ring._pending.is_empty()
		calm = calm + 1 if idle else 0
		if calm >= 40:
			break
	for _i in 5:
		await get_tree().process_frame


func _shot(tag: String) -> void:
	await _settle()
	var img := get_viewport().get_texture().get_image()
	if img == null or img.is_empty():
		print("no image for ", tag, " (no rendering device)")
		return
	img.resize(960, 540)
	var dir := ProjectSettings.globalize_path("res://spikes/planet_gen/shots")
	DirAccess.make_dir_recursive_absolute(dir)
	var path := dir + "/" + tag + ".png"
	img.save_png(path)
	_taken.append(tag)
	print("shot: ", path)


func _offset(dir: Vector3, tangent: Vector3, metres: float) -> Vector3:
	var a: float = metres / main.planet_radius
	return (dir * cos(a) + tangent * sin(a)).normalized()


## Stand on the ground at `dir` (eye height + extra) and look at a world direction on the sphere.
func _stand(dir: Vector3, look_at_dir: Vector3, extra := 0.0, look_extra := 0.0) -> void:
	main.set_orbit(false)
	var gen: RefCounted = main.terrain.gen
	var p: Node3D = main.player
	p.fly_mode = true  # no physics needed for a picture
	p._collision.disabled = true
	p.global_position = main.planet_center + dir * (main.planet_radius + gen.height_at(dir) + extra)
	var target: Vector3 = main.planet_center + look_at_dir * (main.planet_radius + gen.height_at(look_at_dir) + look_extra)
	p.look_at_point(target)
	_light_from(p.global_position)


## The light comes from behind the camera and a bit above, so every picture is lit.
func _light_from(cam_pos: Vector3) -> void:
	var from: Vector3 = (cam_pos - main.planet_center).normalized()
	var l: DirectionalLight3D = main.sun
	var look: Vector3 = -(from + Vector3(0.35, 0.35, 0.0)).normalized()
	var up := Vector3.UP if absf(look.y) < 0.99 else Vector3.RIGHT
	l.look_at_from_position(Vector3.ZERO, look, up)


func _run() -> void:
	await _settle(60)
	var gen: RefCounted = main.terrain.gen
	var recipe: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("res://spikes/planet_gen/recipe.json"))
	var stamps := {}
	for s: Dictionary in recipe.stamps:
		stamps[s.type] = Vector3(s.center[0], s.center[1], s.center[2]).normalized()
	var basin: Vector3 = stamps.basin
	var esc: Vector3 = stamps.escarpment
	var plat: Vector3 = stamps.plateau

	# orbit, four sides: over the basin, the rim, the plateau, and the far side of the planet
	var far := -(basin + esc + plat).normalized()
	var views := {"orbit-basin": basin, "orbit-rim": esc, "orbit-plateau": plat, "orbit-far": far}
	for tag: String in views:
		main.set_orbit(true)
		main.orbit_to(views[tag])
		_light_from(main.orbit_cam.global_position)
		await _shot(tag)

	# basin shore: walk out from the basin centre until the ground rises above the sea
	var t := basin.cross(Vector3.UP).normalized()
	var shore := basin
	var m := 0.0
	while m < 3000.0:
		shore = _offset(basin, t, m)
		if gen.height_at(shore) > gen.sea_level() + 0.5 and m > 200.0:
			break
		m += 5.0
	_stand(_offset(basin, t, m + 25.0), basin, 1.7, 0.0)
	await _shot("basin-shore")

	# escarpment: from the low side, looking at the step
	var n := esc.cross(esc.cross(Vector3.UP).normalized()).normalized()
	_stand(_offset(esc, n, -140.0), esc, 1.7, 30.0)
	await _shot("escarpment")

	# plateau edge: from outside, looking at the rim of the plateau
	var tp := plat.cross(Vector3.UP).normalized()
	_stand(_offset(plat, tp, 1000.0), _offset(plat, tp, 600.0), 6.0, 60.0)
	await _shot("plateau-edge")

	# forest edge: a finest chunk with only a few trees; stand 30 m away from its centre
	var rng := RandomNumberGenerator.new()
	rng.seed = 11
	var nf: int = 1 << int(main.terrain.max_depth)
	var sz := 2.0 / nf
	var forest_dir := Vector3.ZERO
	for _i in 4000:
		var d: Dictionary = gen.build_chunk(rng.randi() % 6, -1.0 + (rng.randi() % nf) * sz, -1.0 + (rng.randi() % nf) * sz, sz, true)
		if d.scatter.has("canopy") and d.scatter.canopy.count >= 3 and d.scatter.canopy.count <= 5:
			forest_dir = (d.center as Vector3).normalized()
			break
	if forest_dir != Vector3.ZERO:
		var tf := forest_dir.cross(Vector3.UP).normalized()
		_stand(_offset(forest_dir, tf, -35.0), forest_dir, 1.7, 3.0)
		await _shot("forest-edge")

	# site marker: 70 m away
	var sites: PackedVector3Array = gen.sites()
	if sites.size() > 0:
		var s0 := sites[0]
		var ts := s0.cross(Vector3.UP).normalized()
		_stand(_offset(s0, ts, 70.0), s0, 1.7, 12.0)
		await _shot("site-marker")

	print("shots taken: ", _taken)
	get_tree().quit(0)
