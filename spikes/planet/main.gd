extends Node3D
## Spike 1 entry point. Builds everything in code to keep the scene file trivial.
## Holds the planet model (gravity and atmosphere by altitude), switches between
## walker and ship (F), and keeps sky, fog and ambient light in step with the
## active camera.

const PlayerScript := preload("res://spikes/planet/player.gd")
const ShipScript := preload("res://spikes/planet/ship.gd")
const OverlayScript := preload("res://spikes/planet/debug_overlay.gd")
const TerrainScript := preload("res://spikes/planet/terrain.gd")
const RingScript := preload("res://spikes/planet/collision_ring.gd")
const AutoTestScript := preload("res://spikes/planet/auto_test.gd")

## Planet radius in metres. 5 km is the first guide value (DECISIONS.md); --radius=<m> overrides.
@export var planet_radius := 5000.0
@export var surface_gravity := 9.81  # arcade, not physical for a 3 km planet
@export var atmosphere_height := 1200.0  # start value, tune by feel
@export var fog_density := 0.00025  # at the surface

## Read by the overlay; filled by the terrain and later systems.
var stats := {}

var player: CharacterBody3D
var ship: RigidBody3D
var terrain: Node3D
var ring: Node3D
## The body the player controls right now (player or ship).
var active: Node3D

var _env: Environment
var _sky_mat: ShaderMaterial


func _ready() -> void:
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--radius="):
			planet_radius = arg.trim_prefix("--radius=").to_float()
	stats.radius = planet_radius
	_build_environment()

	# Depths follow the radius so chunk and cell sizes stay about the same:
	# leaf chunks about 37 m, collision cells at most about 20 m.
	var face_edge := planet_radius * PI * 0.5
	terrain = TerrainScript.new()
	terrain.radius = planet_radius
	terrain.max_depth = maxi(1, roundi(log(face_edge / 37.0) / log(2.0)))
	terrain.stats = stats
	add_child(terrain)

	ring = RingScript.new()
	ring.terrain = terrain
	ring.patch_depth = ceili(log(face_edge / 20.0) / log(2.0))
	ring.stats = stats
	add_child(ring)

	player = PlayerScript.new()
	player.planet_center = Vector3.ZERO
	player.terrain = terrain
	player.planet = self
	player.ring = ring
	add_child(player)
	var up := Vector3.UP
	player.global_position = up * (planet_radius + height_at(up) + 2.0)

	ship = ShipScript.new()
	ship.planet = self
	add_child(ship)
	var ship_dir := (up * planet_radius + Vector3(0, 0, -15)).normalized()
	ship.global_transform = Transform3D(_basis_for_up(ship_dir), ship_dir * (planet_radius + height_at(ship_dir) + 3.0))
	ship.freeze = true  # parked ships stay put, no collision needed under them

	active = player
	ring.set_anchors([player])
	stats.rescues = 0

	var overlay := OverlayScript.new()
	overlay.main = self
	add_child(overlay)

	if "--auto-shot" in OS.get_cmdline_user_args():
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


## Radial gravity, falls off with 1/r^2 above the surface.
func gravity_at(pos: Vector3) -> Vector3:
	var r := maxf(pos.length(), planet_radius)
	return -pos.normalized() * surface_gravity * pow(planet_radius / r, 2.0)


## Atmosphere density 0..1: full at the surface, gone at atmosphere_height.
func density_at(pos: Vector3) -> float:
	var alt := pos.length() - planet_radius
	return 1.0 - smoothstep(0.0, atmosphere_height, alt)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and Input.mouse_mode != Input.MOUSE_MODE_CAPTURED:
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.physical_keycode == KEY_ESCAPE:
			Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		elif event.physical_keycode == KEY_F:
			if active == player and player.global_position.distance_to(ship.global_position) < 8.0:
				_enter_ship()
			elif active == ship:
				_exit_ship()


## Short status for the overlay.
func mode_text() -> String:
	if active == ship:
		return "SHIP (F exit)  hover assist %s (H)  horizon follow %s (L)" % [
			"on" if ship.hover_assist else "off", "on" if ship.horizon_follow else "off"]
	if player.fly_mode:
		return "FLY (V)"
	var near := player.global_position.distance_to(ship.global_position) < 8.0
	return "walk" + ("  [F] enter ship" if near else "")


func _enter_ship() -> void:
	player.process_mode = Node.PROCESS_MODE_DISABLED
	player.visible = false
	ship.freeze = false
	ship.piloted = true
	ship.camera.make_current()
	active = ship
	ring.set_anchors([ship])


## No reparenting yet (spike 3): the player is simply put next to the ship.
func _exit_ship() -> void:
	var b := ship.global_transform.basis
	var up := ship.global_position.normalized()
	player.global_position = ship.global_position + b.x * 4.0 + up * 1.0
	player.velocity = ship.linear_velocity
	player.fly_mode = false
	player.process_mode = Node.PROCESS_MODE_INHERIT
	player.visible = true
	player.get_camera().make_current()
	ship.piloted = false
	ship.linear_velocity = Vector3.ZERO
	ship.angular_velocity = Vector3.ZERO
	ship.freeze = true
	active = player
	ring.set_anchors([player])


func _process(_delta: float) -> void:
	var cam := get_viewport().get_camera_3d()
	if cam == null:
		return
	var pos := cam.global_position
	var r := maxf(pos.length(), planet_radius)
	var density := density_at(pos)
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
	var sun := DirectionalLight3D.new()
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
	player.global_position = dir * (planet_radius + height_at(dir) + 150.0)
	player.look_at_planet(-0.35)
	await get_tree().create_timer(3.0).timeout
	overlay.save_screenshot("low")
	player.global_position = Vector3(0.3, 0.4, 1).normalized() * (planet_radius * 2.5)
	player.look_at_planet()
	await get_tree().create_timer(3.0).timeout
	overlay.save_screenshot("orbit")
	get_tree().quit()
