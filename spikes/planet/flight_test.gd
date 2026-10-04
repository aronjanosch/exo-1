extends SceneTree
## Real rigid-body/controller checks on a cheap spherical fixture. Initial
## placement is scripted; measured motion uses SpikeInput and physics ticks.
## godot-agent --headless --path . --fixed-fps 60 --script res://spikes/planet/flight_test.gd

const Ship := preload("res://spikes/planet/ship.gd")
const SpikeInput := preload("res://spikes/planet/spike_input.gd")

class TestPlanet extends Node:
	var planet_radius := 5000.0
	var centre := Vector3.ZERO
	var terrain_height := 0.0
	var atmosphere := false
	func to_planet(p: Vector3) -> Vector3:
		return p - centre
	func height_at(_dir: Vector3) -> float:
		return terrain_height
	func density_at(p: Vector3) -> float:
		return 1.0 - smoothstep(0.0, 1200.0, to_planet(p).length() - planet_radius) if atmosphere else 0.0
	func gravity_at(p: Vector3) -> Vector3:
		var r := to_planet(p)
		return -r.normalized() * 9.81 * pow(planet_radius / r.length(), 2.0)

var world: Node3D
var planet: TestPlanet
var ship: RigidBody3D
var failures := 0

func _initialize() -> void:
	Engine.physics_ticks_per_second = 60
	_run.call_deferred()

func _spawn(height: float, atmosphere := false) -> void:
	SpikeInput.held.clear()
	if world:
		world.free()
	world = Node3D.new()
	root.add_child(world)
	planet = TestPlanet.new()
	planet.atmosphere = atmosphere
	world.add_child(planet)
	ship = Ship.new()
	ship.planet = planet
	ship.position = Vector3(0, planet.planet_radius + height, 0)
	world.add_child(ship)
	ship.piloted = true
	await _ticks(3)

func _ticks(count: int) -> void:
	for i in count:
		await physics_frame

func _check(condition: bool, note: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", note])
	if not condition:
		failures += 1

func _run() -> void:
	await _spawn(15.0, true)
	SpikeInput.held[KEY_W] = true
	await _ticks(2)
	_check(ship.linear_velocity.length() > 0.0 and ship.linear_velocity.length() < 0.2,
		"takeoff builds thrust instead of applying full acceleration (%.3f m/s after two ticks)" % ship.linear_velocity.length())
	await _ticks(238)
	_check(absf(ship.linear_velocity.length() - 45.0) < 1.0, "low flight %.2f m/s (target 45)" % ship.linear_velocity.length())
	SpikeInput.held.clear()
	await _ticks(30)
	_check(ship.linear_velocity.length() > 20.0, "ground stop retains movement after 0.5 s (%.2f m/s)" % ship.linear_velocity.length())
	await _ticks(90)
	_check(ship.linear_velocity.length() > 12.0, "gentle ground release still moving after 2 s (%.2f m/s)" % ship.linear_velocity.length())
	var ground_ticks := 120
	while ship.linear_velocity.length() > 0.5 and ground_ticks < 300:
		await physics_frame
		ground_ticks += 1
	_check(ship.linear_velocity.length() < 0.5, "neutral stops in atmosphere in %.2f s" % (ground_ticks / 60.0))
	_check(absf(ship.clearance_at(ship.global_position) - 15.0) < 2.0, "hover/curvature clearance %.2f m" % ship.clearance_at(ship.global_position))

	SpikeInput.held[KEY_W] = true
	await _ticks(240)
	SpikeInput.held[KEY_SHIFT] = true
	SpikeInput.held[KEY_X] = true
	await _ticks(120)
	_check(ship.brake_active and ship.commanded_speed == 0.0 and ship.linear_velocity.length() < 0.5,
		"firm brake overrides forward/boost and stops ground flight in 2 s")
	SpikeInput.held.erase(KEY_X)
	await _ticks(240)
	_check(not ship.brake_active and absf(ship.linear_velocity.length() - 45.0) < 1.0,
		"releasing brake restores held movement input")

	await _spawn(150.0)
	SpikeInput.held[KEY_W] = true
	await _ticks(300)
	_check(absf(ship.linear_velocity.length() - 60.0) < 2.0, "150 m flight %.2f m/s (target 60)" % ship.linear_velocity.length())
	var distance := 0.0
	var previous := ship.global_position
	SpikeInput.held.clear()
	SpikeInput.held[KEY_X] = true
	for i in 120:
		await physics_frame
		distance += previous.distance_to(ship.global_position)
		previous = ship.global_position
	_check(ship.linear_velocity.length() < 0.5 and distance < 60.0, "firm stop %.2f m over 2 s, speed %.3f" % [distance, ship.linear_velocity.length()])

	await _spawn(150.0)
	SpikeInput.held[KEY_W] = true
	SpikeInput.held[KEY_SHIFT] = true
	await _ticks(240)
	_check(ship.linear_velocity.length() > 140.0 and ship.linear_velocity.length() < 155.0, "boost at 150 m %.2f m/s" % ship.linear_velocity.length())
	SpikeInput.held.erase(KEY_SHIFT)
	await _ticks(240)
	_check(absf(ship.linear_velocity.length() - 60.0) < 3.0, "boost release returns to cruise %.2f m/s" % ship.linear_velocity.length())

	await _spawn(150.0)
	SpikeInput.held[KEY_W] = true
	SpikeInput.held[KEY_D] = true
	await _ticks(240)
	_check(ship.linear_velocity.length() < 61.0, "diagonal speed %.2f m/s" % ship.linear_velocity.length())
	var acceleration_max := 0.0
	var last_v := ship.linear_velocity
	for i in 120:
		ship._mouse = Vector2(0.01, 0.0)  # same mouse accumulator used by input events
		await physics_frame
		acceleration_max = maxf(acceleration_max, (ship.linear_velocity - last_v).length() * 60.0)
		last_v = ship.linear_velocity
	await _ticks(120)
	var local_v: Vector3 = ship.global_basis.inverse() * ship.linear_velocity
	_check(local_v.z < -30.0 and absf(local_v.y) < 2.0, "turn redirects movement; local velocity %s" % local_v)
	_check(acceleration_max < 40.5, "turn total acceleration %.2f m/s²" % acceleration_max)

	await _spawn(2000.0)
	SpikeInput.held[KEY_W] = true
	await _ticks(1500)
	_check(absf(ship.linear_velocity.length() - 350.0) < 3.0, "high flight %.2f m/s (target 350)" % ship.linear_velocity.length())
	_check(absf(ship.clearance_at(ship.global_position) - 2000.0) < 20.0, "curved high flight clearance %.2f m" % ship.clearance_at(ship.global_position))
	var high_distance := 0.0
	previous = ship.global_position
	SpikeInput.held.clear()
	var stop_ticks := 0
	while ship.linear_velocity.length() > 0.5 and stop_ticks < 540:
		await physics_frame
		high_distance += previous.distance_to(ship.global_position)
		previous = ship.global_position
		stop_ticks += 1
	_check(stop_ticks > 240 and stop_ticks < 540 and ship.linear_velocity.length() < 0.5,
		"350 m/s gentle stop %.2f s / %.1f m / residual %.3f m/s" % [stop_ticks / 60.0, high_distance, ship.linear_velocity.length()])
	_check(absf(ship.clearance_at(ship.global_position) - 2000.0) < 20.0,
		"gentle high-speed stop retains curved flight")

	SpikeInput.held[KEY_W] = true
	await _ticks(480)
	SpikeInput.held.clear()
	SpikeInput.held[KEY_X] = true
	var before_brake := ship.linear_velocity
	await _ticks(2)
	_check(ship.linear_velocity.length() > 340.0 and (ship.linear_velocity - before_brake).length() < 6.0,
		"pressing firm brake preserves momentum and bounds initial correction")
	var firm_ticks := 2
	while ship.linear_velocity.length() > 0.5 and firm_ticks < 240:
		await physics_frame
		firm_ticks += 1
	_check(firm_ticks < stop_ticks / 2 and firm_ticks <= 210 and ship.linear_velocity.length() < 0.5,
		"350 m/s firm stop %.2f s (gentle %.2f s)" % [firm_ticks / 60.0, stop_ticks / 60.0])
	await _ticks(240 - firm_ticks)
	_check(ship.linear_velocity.length() < 0.5, "held brake settles at rest within 4 s")
	SpikeInput.held.erase(KEY_X)

	# A heading change must redirect trajectory, not just the model. Measure
	# remaining side-slip after the mouse stops moving, at travel speed.
	SpikeInput.held[KEY_W] = true
	await _ticks(480)
	for i in 30:
		ship._mouse = Vector2(0.008, 0.0)
		await physics_frame
	await _ticks(150)
	local_v = ship.global_basis.inverse() * ship.linear_velocity
	_check(local_v.z < -330.0 and absf(local_v.x) < 5.0,
		"high-speed turn catches heading within 2.5 s; local velocity %s" % local_v)

	await _spawn(2000.0)
	SpikeInput.held[KEY_W] = true
	await _ticks(600)
	SpikeInput.held.clear()
	var v0 := ship.linear_velocity
	ship.hover_assist = false
	await _ticks(2)
	_check((ship.linear_velocity - v0).length() < 1.0, "assist toggle preserves momentum")
	await _ticks(120)
	_check(ship.linear_velocity.length() > 340.0, "unassisted vacuum coasts %.2f m/s" % ship.linear_velocity.length())
	ship.hover_assist = true
	await _ticks(540)
	_check(ship.linear_velocity.length() < 0.5, "assist arrests high-speed drift in 9 s (%.3f m/s)" % ship.linear_velocity.length())

	await _spawn(2000.0)
	SpikeInput.held[KEY_W] = true
	await _ticks(600)
	ship.hover_assist = false
	SpikeInput.held[KEY_X] = true
	await _ticks(240)
	_check(ship.brake_active and ship.commanded_speed == 0.0 and ship.linear_velocity.length() < 0.5,
		"firm brake works with assist off and overrides held W; active %s, goal %.3f, residual %.3f" % [
			ship.brake_active, ship.commanded_speed, ship.linear_velocity.length()])
	SpikeInput.held.erase(KEY_X)
	await _ticks(30)
	_check(not ship.brake_active and ship.linear_velocity.length() > 5.0,
		"brake release restores manual thrust")
	SpikeInput.held.clear()
	await _ticks(120)
	_check(ship.linear_velocity.length() > 5.0, "manual flight still coasts after braking")

	await _spawn(700.0)
	SpikeInput.held[KEY_W] = true
	SpikeInput.held[KEY_CTRL] = true
	await _ticks(1800)
	_check(ship.forward_speed_limit < 120.0 and ship.linear_velocity.length() < 90.0,
		"descending lowers limit %.2f, speed %.2f, clearance %.2f" % [ship.forward_speed_limit, ship.linear_velocity.length(), ship.clearance_at(ship.global_position)])

	await _spawn(600.0)
	planet.terrain_height = 570.0
	SpikeInput.held[KEY_W] = true
	await _ticks(240)
	_check(absf(ship.linear_velocity.length() - 45.0) < 2.0, "mountain clearance governs speed %.2f" % ship.linear_velocity.length())
	var before := ship.linear_velocity
	var offset := Vector3(10000, 0, 0)
	planet.centre -= offset
	ship.global_position -= offset
	await _ticks(3)
	_check(absf(ship.clearance_at(ship.global_position) - 30.0) < 2.0 and (ship.linear_velocity - before).length() < 1.0,
		"origin shift preserves flight frame")
	SpikeInput.held.clear()
	print("FLIGHT TEST: %d failures" % failures)
	quit(0 if failures == 0 else 1)
