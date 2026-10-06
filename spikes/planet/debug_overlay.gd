extends CanvasLayer
## Debug overlay: FPS, frame times, worst frame in a sliding window, render and
## memory counters, chunk stats, altitude. F3 toggles it. F12 saves a screenshot
## to user://screenshots/.

const WINDOW_SEC := 5.0

var main: Node3D

var _label: Label
var _frame_times: Array[Vector2] = []  # (timestamp, frame ms)


func _ready() -> void:
	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
	for side in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 12)
	add_child(margin)

	_label = Label.new()
	_label.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	_label.add_theme_color_override("font_outline_color", Color.BLACK)
	_label.add_theme_constant_override("outline_size", 4)
	margin.add_child(_label)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		if event.physical_keycode == KEY_F3:
			_label.visible = not _label.visible
		elif event.physical_keycode == KEY_F12:
			save_screenshot()


func _process(delta: float) -> void:
	var now := Time.get_ticks_msec() / 1000.0
	_frame_times.append(Vector2(now, delta * 1000.0))
	while _frame_times[0].x < now - WINDOW_SEC:
		_frame_times.pop_front()
	var worst := 0.0
	for ft in _frame_times:
		worst = maxf(worst, ft.y)

	if not _label.visible:
		return

	var body: Node3D = main.active
	var p: Vector3 = main.to_planet(body.global_position)
	var v: Vector3 = body.linear_velocity if body is RigidBody3D else body.velocity
	var dist := p.length()
	var dir := p / dist
	var altitude: float = dist - main.planet_radius
	var above_surface: float = altitude - main.height_at(dir)
	var stat_parts: PackedStringArray = []
	for key in main.stats:
		var value = main.stats[key]
		stat_parts.append("%s %s" % [key, ("%.2f" % value) if value is float else str(value)])

	_label.text = "\n".join([
		"FPS %d   frame %.2f ms   worst(%ds) %.2f ms" % [Engine.get_frames_per_second(), delta * 1000.0, WINDOW_SEC, worst],
		"process %.2f ms   physics %.2f ms" % [
			Performance.get_monitor(Performance.TIME_PROCESS) * 1000.0,
			Performance.get_monitor(Performance.TIME_PHYSICS_PROCESS) * 1000.0],
		"draw calls %d   primitives %d" % [
			Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME),
			Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME)],
		"mem static %.1f MB   video %.1f MB" % [
			Performance.get_monitor(Performance.MEMORY_STATIC) / 1048576.0,
			Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED) / 1048576.0],
		"   ".join(stat_parts.slice(0, 4)),
		"   ".join(stat_parts.slice(4)),
		"altitude %.2f m   above surface %.2f m   dist centre %.2f m" % [altitude, above_surface, dist],
		_ground_line(dir),
		"speed %.1f m/s   %s" % [v.length(), main.mode_text()],
		"flight assist %s   goal %.1f m/s   forward limit %.1f m/s" % [
			"on" if main.ship.hover_assist else "off", main.ship.commanded_speed, main.ship.forward_speed_limit],
		"%s   %s" % [RenderingServer.get_video_adapter_name(), ProjectSettings.get_setting("rendering/renderer/rendering_method")],
	])


func save_screenshot(tag := "") -> void:
	DirAccess.make_dir_recursive_absolute("user://screenshots")
	var img := get_viewport().get_texture().get_image()
	var path := "user://screenshots/%s%s.png" % [Time.get_datetime_string_from_system().replace(":", "-"), ("-" + tag) if tag else ""]
	img.save_png(path)
	print("screenshot: ", ProjectSettings.globalize_path(path))


## Spike 8: what the generator says about the spot under the body (PlanetGen.sample, no mesh needed).
func _ground_line(dir: Vector3) -> String:
	var gen: RefCounted = main.terrain.gen
	var s: Dictionary = gen.sample(dir)
	var near: PackedVector3Array = gen.sites_near(dir, 100000.0)
	var site_m := INF
	for site in near:
		site_m = minf(site_m, main.planet_radius * acos(clampf(dir.dot(site), -1.0, 1.0)))
	return "biome %d   height above sea %.1f m   slope %.0f deg   temp %.2f moist %.2f landform %d   macro elev %.1f stamp %.1f   site %s" % [
		s.biome, s.height_above_sea, s.slope_deg, s.temperature, s.moisture, s.landform, s.macro_elevation, s.stamp_height,
		("%.0f m" % site_m) if site_m < INF else "none"]
