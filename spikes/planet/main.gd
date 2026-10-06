extends Node3D
## Spike 1 entry point. Builds everything in code to keep the scene file trivial.
## Holds the planet model (gravity and atmosphere by altitude), switches between
## walker and ship (F), and keeps sky, fog and ambient light in step with the
## active camera.
## Spike 5: the planet centre is a variable (planet_center), not the origin.
## --planet-offset=x,y,z moves it; --origin-shift=<m> moves the whole world back
## whenever the active body is farther than <m> from the origin.
## --second-planet=x,y,z,radius adds a planet relative to the first one.
## planet_center, planet_radius, terrain and ring always describe the current
## planet (nearest surface, 500 m hysteresis). Gravity and atmosphere come only
## from the current planet: a test assumption, not designed.
## --recenter: when the current planet changes, shift so its centre is the origin.

const PlayerScript := preload("res://spikes/planet/player.gd")
const ShipScript := preload("res://spikes/planet/ship.gd")
const OverlayScript := preload("res://spikes/planet/debug_overlay.gd")
const TerrainScript := preload("res://spikes/planet/terrain.gd")
const RingScript := preload("res://spikes/planet/collision_ring.gd")
const AutoTestScript := preload("res://spikes/planet/auto_test.gd")
const SpikeInput := preload("res://spikes/planet/spike_input.gd")
const PlanetField := preload("res://spikes/planet/planet_field.gd")
const PlanetWalkScript := preload("res://spikes/planet_gen/planet_walk.gd")
const PlanetShotsScript := preload("res://spikes/planet_gen/planet_shots.gd")

## Planet radius in metres. 5 km is the first guide value (DECISIONS.md); --radius=<m> overrides.
@export var planet_radius := 5000.0
@export var surface_gravity := 9.81  # arcade surface/atmosphere acceleration
@export var atmosphere_height := 1200.0  # start value, tune by feel
@export var gravity_end_height := 6000.0  # no planetary influence above this altitude
@export var fog_density := 0.00025  # at the surface

## Read by the overlay; filled by the terrain and later systems.
var stats := {}

## World position of the planet centre. Changes with every origin shift.
var planet_center := Vector3.ZERO
## Shift when the active body is farther than this from the origin (0 = off).
var origin_shift_distance := 0.0
## Sum of all shifts in double precision (GDScript floats are 64 bit), so the
## true position stays known: true = world + shifted_total.
var shifted_total := [0.0, 0.0, 0.0]
var shift_count := 0
var shift_ms_max := 0.0
var _shift_in_physics := "--shift-in-physics" in OS.get_cmdline_user_args()
var recenter := "--recenter" in OS.get_cmdline_user_args()
var planet_switches := 0
var _mouse_released := false  # Escape pressed: do not re-grab on focus


class PlanetBody:
	extends RefCounted
	var center: Vector3
	var radius: float
	var terrain: Node3D
	var ring: Node3D


var planets: Array[PlanetBody] = []
var current: PlanetBody

var player: CharacterBody3D
var ship: RigidBody3D
var terrain: Node3D
var ring: Node3D
## The body the player controls right now (player or ship).
var active: Node3D

var _env: Environment
var sun: DirectionalLight3D
## Spike 8: orbit camera (key O): about 15 km out, looking at the planet, mouse rotates, wheel zooms.
var orbit_cam: Camera3D
var orbit_active := false
var orbit_distance := 15000.0
var _orbit_yaw := 0.6
var _orbit_pitch := 0.35
var _orbit_prev_cam: Camera3D
var _sky_mat: ShaderMaterial


func _ready() -> void:
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--radius="):
			planet_radius = arg.trim_prefix("--radius=").to_float()
		elif arg.begins_with("--planet-offset="):
			var c := arg.trim_prefix("--planet-offset=").split_floats(",")
			planet_center = Vector3(c[0], c[1], c[2])
		elif arg.begins_with("--origin-shift="):
			origin_shift_distance = arg.trim_prefix("--origin-shift=").to_float()
	stats.radius = planet_radius
	_build_environment()

	_add_planet(planet_center, planet_radius, 1, stats)
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--second-planet="):
			var c := arg.trim_prefix("--second-planet=").split_floats(",")
			_add_planet(planet_center + Vector3(c[0], c[1], c[2]), c[3], 2, {})

	player = PlayerScript.new()
	player.planet = self
	add_child(player)
	_use_planet(planets[0])
	var up := Vector3.UP
	player.global_position = planet_center + up * (planet_radius + height_at(up) + 2.0)

	ship = ShipScript.new()
	ship.planet = self
	add_child(ship)
	# Parked 15 m ahead, ramp towards the walker, floor resting on the highest
	# terrain point under the hull. Frozen until someone first sits down.
	var ship_dir := (up * planet_radius + Vector3(0, 0, -15)).normalized()
	var ship_basis := _basis_for_up(ship_dir)
	var ground := -INF
	for corner in [Vector3.ZERO, Vector3(2, 0, 4), Vector3(-2, 0, 4), Vector3(2, 0, -4), Vector3(-2, 0, -4)]:
		var d: Vector3 = (ship_dir * planet_radius + ship_basis * corner).normalized()
		ground = maxf(ground, height_at(d))
	ship.global_transform = Transform3D(ship_basis, planet_center + ship_dir * (planet_radius + ground + 0.05))
	ship.freeze = true

	active = player
	ring.set_anchors([player, ship])
	stats.planet = 0
	stats.rescues = 0

	var overlay := OverlayScript.new()
	overlay.main = self
	add_child(overlay)

	if "--planet-walk" in OS.get_cmdline_user_args():
		var walk := PlanetWalkScript.new()
		walk.main = self
		add_child(walk)
	elif "--planet-shots" in OS.get_cmdline_user_args():
		var shots := PlanetShotsScript.new()
		shots.main = self
		shots.overlay = overlay
		add_child(shots)
	elif "--auto-shot" in OS.get_cmdline_user_args():
		_auto_shot(overlay)
	elif "--auto-test" in OS.get_cmdline_user_args():
		var test := AutoTestScript.new()
		test.main = self
		test.overlay = overlay
		add_child(test)


## Terrain height above the base radius along a unit direction.
## CPU-side on purpose: collision depends on it (no GPU readback).
func height_at(dir: Vector3) -> float:
	return terrain.height_at(dir)


## Distance from the true origin (before any shift), in double precision.
func true_distance(pos: Vector3) -> float:
	var s := 0.0
	for i in 3:
		var c: float = pos[i] + shifted_total[i]
		s += c * c
	return sqrt(s)


## World position -> position relative to the planet centre.
func to_planet(pos: Vector3) -> Vector3:
	return pos - planet_center


## Shared gravity/planet-follow envelope. Takes world positions.
func field_strength_at(pos: Vector3) -> float:
	var altitude := to_planet(pos).length() - planet_radius
	return PlanetField.strength(altitude, atmosphere_height, gravity_end_height)


## Arcade radial gravity: full in atmosphere, softly fading to zero in space.
func gravity_at(pos: Vector3) -> Vector3:
	var p := to_planet(pos)
	return -p.normalized() * surface_gravity * field_strength_at(pos)


## Atmosphere density 0..1: full at the surface, gone at atmosphere_height.
func density_at(pos: Vector3) -> float:
	var alt := to_planet(pos).length() - planet_radius
	return 1.0 - smoothstep(0.0, atmosphere_height, alt)


## Keeps the active body near the origin by moving everything else back.
## Offsets are whole metres: subtracting them is exact for positions whose float
## step is at most 1 m, unless the result lands in a coarser range.
## Runs in _process, not _physics_process: Godot hands moved bodies to the
## physics server only when it flushes transform notifications, and there is no
## flush between two _physics_process calls of the same tick. A shift there let
## the walker's move_and_slide run against stale collision patches (rescues).
## --shift-in-physics keeps the old behaviour for comparison.
func _process_shift(in_physics: bool) -> void:
	if in_physics != _shift_in_physics:
		return
	if origin_shift_distance > 0.0 and active.global_position.length() > origin_shift_distance:
		shift_origin(active.global_position.round())


## Terrain and collision ring for one planet. Depths follow the radius so chunk
## and cell sizes stay about the same: leaf chunks about 37 m, cells at most 20 m.
func _add_planet(center: Vector3, radius: float, noise_seed: int, planet_stats: Dictionary) -> void:
	var pb := PlanetBody.new()
	pb.center = center
	pb.radius = radius
	var face_edge := radius * PI * 0.5
	pb.terrain = TerrainScript.new()
	pb.terrain.radius = radius
	# Own PlanetGen per planet. The start planet (seed index 1) uses the recipe as written;
	# further planets get their own seed.
	pb.terrain.gen = TerrainScript.make_gen(-1 if noise_seed == 1 else noise_seed * 7919, radius)
	pb.terrain.max_depth = maxi(1, roundi(log(face_edge / 37.0) / log(2.0)))
	pb.terrain.stats = planet_stats
	pb.terrain.position = center
	add_child(pb.terrain)
	pb.terrain.material.set_shader_parameter("planet_center", center)
	pb.ring = RingScript.new()
	pb.ring.terrain = pb.terrain
	pb.ring.patch_depth = ceili(log(face_edge / 20.0) / log(2.0))
	pb.ring.stats = planet_stats
	pb.ring.position = center
	add_child(pb.ring)
	planets.append(pb)


func _use_planet(pb: PlanetBody) -> void:
	if current and current != pb:
		current.ring.set_anchors([])
	current = pb
	planet_center = pb.center
	planet_radius = pb.radius
	terrain = pb.terrain
	ring = pb.ring
	player.planet_center = pb.center
	player.terrain = pb.terrain
	player.ring = pb.ring
	if active:
		ring.set_anchors([player, ship])


## Nearest surface wins, with 500 m hysteresis so it does not flip-flop.
func _update_current_planet() -> void:
	if planets.size() < 2:
		return
	var p := active.global_position
	var best := current
	var best_alt := p.distance_to(current.center) - current.radius - 500.0
	for pb in planets:
		var alt := p.distance_to(pb.center) - pb.radius
		if alt < best_alt:
			best = pb
			best_alt = alt
	if best != current:
		_use_planet(best)
		planet_switches += 1
		stats.planet = planets.find(best)
		stats.planet_switches = planet_switches
		if recenter:
			shift_origin(best.center.round())


func shift_origin(offset: Vector3) -> void:
	var t0 := Time.get_ticks_usec()
	var before := to_planet(active.global_position)
	var v_before: Vector3 = ship.linear_velocity
	for pb in planets:
		pb.terrain.global_position -= offset
		pb.ring.global_position -= offset
		pb.center -= offset
		pb.terrain.material.set_shader_parameter("planet_center", pb.center)
	# The walker is a child of the ship while in the cabin (spike 3) and then
	# moves with it; shifting it again would move it twice.
	for n: Node3D in [player, ship]:
		if n.get_parent() == self:
			n.global_position -= offset
	planet_center = current.center
	player.planet_center = planet_center
	for i in 3:
		shifted_total[i] += offset[i]
	shift_count += 1
	shift_ms_max = maxf(shift_ms_max, (Time.get_ticks_usec() - t0) / 1000.0)
	stats.shifts = shift_count
	# Did the active body move relative to the planet, or the ship lose speed?
	stats.shift_jump_mm_max = maxf(stats.get("shift_jump_mm_max", 0.0),
		before.distance_to(to_planet(active.global_position)) * 1000.0)
	stats.shift_dv_max = maxf(stats.get("shift_dv_max", 0.0), v_before.distance_to(ship.linear_velocity))
	stats.shift_ms_max = shift_ms_max


## The window may get focus only after _ready set CAPTURED, then Godot thinks
## the mouse is grabbed while the compositor never locked it. Grab again on
## focus, unless the player released the mouse with Escape.
func _notification(what: int) -> void:
	if what == NOTIFICATION_APPLICATION_FOCUS_IN and not _mouse_released and not SpikeInput.scripted():
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


func _unhandled_input(event: InputEvent) -> void:
	# Always set, not only when != CAPTURED: Godot's idea of the mode can be stale.
	if orbit_active and event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		_orbit_yaw -= event.relative.x * 0.004
		_orbit_pitch = clampf(_orbit_pitch + event.relative.y * 0.004, -1.5, 1.5)
		_update_orbit_camera()
		get_viewport().set_input_as_handled()
	elif orbit_active and event is InputEventMouseButton and event.pressed and event.button_index in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
		orbit_distance = clampf(orbit_distance * (0.9 if event.button_index == MOUSE_BUTTON_WHEEL_UP else 1.1), planet_radius * 1.2, 40000.0)
		_update_orbit_camera()
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.pressed and not SpikeInput.scripted():
		_mouse_released = false
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.physical_keycode == KEY_ESCAPE:
			_mouse_released = true
			Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		elif event.physical_keycode == KEY_O:
			set_orbit(not orbit_active)
		elif event.physical_keycode == KEY_F:
			if active == ship:
				stand_up()
			elif near_seat():
				sit_down()


## Orbit camera on or off. While on, the walker is paused (as while seated, only without the ship).
func set_orbit(on: bool) -> void:
	if on == orbit_active:
		return
	if on:
		if orbit_cam == null:
			orbit_cam = Camera3D.new()
			orbit_cam.far = 100000.0
			orbit_cam.near = 5.0
			orbit_cam.fov = 50.0
			add_child(orbit_cam)
		_orbit_prev_cam = get_viewport().get_camera_3d()
		if active == player:
			player.process_mode = Node.PROCESS_MODE_DISABLED
		orbit_active = true
		_update_orbit_camera()
		orbit_cam.make_current()
	else:
		orbit_active = false
		if active == player:
			player.process_mode = Node.PROCESS_MODE_INHERIT
		if is_instance_valid(_orbit_prev_cam):
			_orbit_prev_cam.make_current()


## Put the orbit camera on the sphere around the planet at the given direction (script or mouse).
func orbit_to(dir: Vector3, distance := 15000.0) -> void:
	orbit_distance = distance
	_orbit_yaw = atan2(dir.x, dir.z)
	_orbit_pitch = asin(clampf(dir.y, -1.0, 1.0))
	_update_orbit_camera()


func _update_orbit_camera() -> void:
	var dir := Vector3(cos(_orbit_pitch) * sin(_orbit_yaw), sin(_orbit_pitch), cos(_orbit_pitch) * cos(_orbit_yaw))
	orbit_cam.global_position = planet_center + dir * orbit_distance
	var up := Vector3.UP if absf(dir.y) < 0.99 else Vector3.RIGHT
	orbit_cam.look_at(planet_center, up)


## Short status for the overlay.
func mode_text() -> String:
	if active == ship:
		return "SHIP (F stand up)  flight assist %s (H)  planet follow %s (L)" % [
			"on" if ship.hover_assist else "off", "on" if ship.horizon_follow else "off"]
	if player.fly_mode:
		return "FLY (V)"
	if player.ship_frame:
		return "in cabin" + ("  [F] sit" if near_seat() else "")
	return "walk"


func near_seat() -> bool:
	return player.ship_frame == ship and player.position.distance_to(ShipScript.SEAT_POS) < 1.8


## Spike 3: the walker stays a child of the ship while seated.
func sit_down() -> void:
	player.process_mode = Node.PROCESS_MODE_DISABLED
	player.transform = Transform3D(Basis(), ShipScript.SEAT_POS - Vector3(0, 0.3, 0))
	ship.freeze = false
	ship.piloted = true
	ship.camera.make_current()
	active = ship
	ring.set_anchors([player, ship])


func stand_up() -> void:
	ship.piloted = false  # hover assist now holds the ship
	player.transform = Transform3D(Basis(), Vector3(0, 0.32, ShipScript.SEAT_POS.z + 1.0))
	player.velocity = Vector3.ZERO
	player.process_mode = Node.PROCESS_MODE_INHERIT
	player.get_camera().make_current()
	active = player
	ring.set_anchors([player, ship])


## Moves the walker into or out of the ship's frame. A plain box test instead
## of Area3D signals: reparenting re-fires area signals, and a deferred call
## from that handler crashed Godot 4.7.2 (segfault in CallQueue).
func _physics_process(_delta: float) -> void:
	_process_shift(true)
	if player.process_mode == Node.PROCESS_MODE_DISABLED or player.fly_mode:
		return
	if player.ship_frame == null and ship.cabin_contains(player.global_position, -0.2):
		player.enter_ship_frame(ship)
	elif player.ship_frame == ship and not ship.cabin_contains(player.global_position, 0.3):
		player.leave_ship_frame(self)


func _process(_delta: float) -> void:
	_update_current_planet()
	_process_shift(false)
	var cam := get_viewport().get_camera_3d()
	if cam == null:
		return
	var pos := to_planet(cam.global_position)
	var r := maxf(pos.length(), planet_radius)
	var density := density_at(cam.global_position)  # takes world positions
	_sky_mat.set_shader_parameter("planet_up", pos.normalized())
	_sky_mat.set_shader_parameter("atmosphere", density)
	_sky_mat.set_shader_parameter("horizon_sin", sqrt(maxf(0.0, 1.0 - pow(planet_radius / r, 2.0))))
	_env.fog_density = fog_density * density
	_env.ambient_light_color = Color(0.08, 0.08, 0.1).lerp(Color(0.55, 0.65, 0.8), density)
	stats.atmosphere = density
	stats.gravity = gravity_at(active.global_position).length()


func _basis_for_up(up: Vector3) -> Basis:
	var fwd := Vector3.FORWARD - up * Vector3.FORWARD.dot(up)
	if fwd.length_squared() < 1e-6:
		fwd = Vector3.RIGHT - up * Vector3.RIGHT.dot(up)
	fwd = fwd.normalized()
	return Basis(fwd.cross(up), up, -fwd)


func _build_environment() -> void:
	sun = DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-50, 30, 0)
	sun.shadow_enabled = false
	add_child(sun)

	_sky_mat = ShaderMaterial.new()
	_sky_mat.shader = preload("res://spikes/planet/sky.gdshader")
	var sky := Sky.new()
	sky.sky_material = _sky_mat
	sky.radiance_size = Sky.RADIANCE_SIZE_32  # ambient comes from a colour, keep this cheap
	_env = Environment.new()
	_env.background_mode = Environment.BG_SKY
	_env.sky = sky
	_env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	_env.reflected_light_source = Environment.REFLECTION_SOURCE_DISABLED
	_env.fog_enabled = true
	_env.fog_light_color = Color(0.72, 0.82, 0.95)
	_env.fog_sky_affect = 0.0
	var world_env := WorldEnvironment.new()
	world_env.environment = _env
	add_child(world_env)


## Scripted screenshots for checks without a human at the keyboard. Run with
## `godot --path . -- --auto-shot`. Covers ground, low flight over a cube-face
## corner (seams), the ship from the walker, and orbit.
func _auto_shot(overlay: CanvasLayer) -> void:
	await get_tree().create_timer(3.0).timeout
	overlay.save_screenshot("ground")
	player.look_at_point(ship.global_position)
	await get_tree().create_timer(1.0).timeout
	overlay.save_screenshot("ship")
	player.fly_mode = true
	var dir := Vector3(1, 1, 1).normalized()
	player.global_position = planet_center + dir * (planet_radius + height_at(dir) + 150.0)
	player.look_at_planet(-0.35)
	await get_tree().create_timer(3.0).timeout
	overlay.save_screenshot("low")
	player.global_position = planet_center + Vector3(0.3, 0.4, 1).normalized() * (planet_radius * 2.5)
	player.look_at_planet()
	await get_tree().create_timer(3.0).timeout
	overlay.save_screenshot("orbit")
	get_tree().quit()
