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
	await _ticks(240)
	_check(absf(ship.linear_velocity.length() - 25.0) < 1.0, "low flight %.2f m/s (target 25)" % ship.linear_velocity.length())
	SpikeInput.held.clear()
	await _ticks(120)
	_check(ship.linear_velocity.length() < 0.5, "neutral stops in atmosphere")
	_check(absf(ship.clearance_at(ship.global_position) - 15.0) < 2.0, "hover/curvature clearance %.2f m" % ship.clearance_at(ship.global_position))

	await _spawn(150.0)
	SpikeInput.held[KEY_W] = true
	await _ticks(300)
	_check(absf(ship.linear_velocity.length() - 60.0) < 2.0, "150 m flight %.2f m/s (target 60)" % ship.linear_velocity.length())
	var distance := 0.0
	var previous := ship.global_position
	SpikeInput.held.clear()
	for i in 120:
		await physics_frame
		distance += previous.distance_to(ship.global_position)
		previous = ship.global_position
	_check(ship.linear_velocity.length() < 0.5 and distance < 60.0, "release stop %.2f m over 2 s, speed %.3f" % [distance, ship.linear_velocity.length()])

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
	SpikeInput.held.clear()
	var v0 := ship.linear_velocity
	ship.hover_assist = false
	await _ticks(2)
	_check((ship.linear_velocity - v0).length() < 1.0, "assist toggle preserves momentum")
	await _ticks(120)
	_check(ship.linear_velocity.length() > 340.0, "unassisted vacuum coasts %.2f m/s" % ship.linear_velocity.length())
	ship.hover_assist = true
	await _ticks(900)
	_check(ship.linear_velocity.length() < 0.5, "assist arrests high-speed drift in 15 s (%.3f m/s)" % ship.linear_velocity.length())

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
	_check(ship.linear_velocity.length() < 27.0, "mountain clearance governs speed %.2f" % ship.linear_velocity.length())
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
