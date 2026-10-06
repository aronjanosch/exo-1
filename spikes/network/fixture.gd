extends Node3D
## Cheap test world. Real spike-3 ship/player controllers, flat landing patch.
const Ship := preload("res://spikes/planet/ship.gd")
const NetworkWalker := preload("res://spikes/network/walker.gd")
const Snapshot := preload("res://spikes/network/snapshot.gd")
const CENTRES := [Vector3.ZERO, Vector3(200000, 0, 0)]
const RADIUS := 5000.0

class Planet extends Node3D:
	var planet_radius := 5000.0
	var centre := Vector3.ZERO
	var zero_gravity := false
	func to_planet(p: Vector3) -> Vector3:
		return p - centre
	func height_at(_dir: Vector3) -> float:
		return 0.0
	func density_at(p: Vector3) -> float:
		return 1.0 - smoothstep(0.0, 1200.0, to_planet(p).length() - planet_radius)
	## Flight code asks for the gravity/planet-follow envelope. The fixture stays in
	## the low atmosphere where the real planet returns 1.0, so no fade here.
	func field_strength_at(_p: Vector3) -> float:
		return 1.0
	func gravity_at(p: Vector3) -> Vector3:
		if zero_gravity:
			return Vector3.ZERO
		var r := to_planet(p)
		return -r.normalized() * 9.81 * pow(planet_radius / r.length(), 2.0)

var planet: Planet
var planet_id := 0
## Doubles live in scalar GDScript floats, not a far-away Vector3.
var origin_x := 0.0
var origin_y := 5000.0
var origin_z := 0.0
var shifts := 0
var ship: RigidBody3D
var walker: CharacterBody3D
var floor_body: StaticBody3D

func setup(id := 0, slot := 1, with_walker := true) -> void:
	planet_id = id
	origin_x = float(CENTRES[id].x)
	planet = Planet.new()
	planet.centre = world_position(id, Vector3.ZERO)
	add_child(planet)
	floor_body = StaticBody3D.new()
	floor_body.position = world_position(id, Vector3(0, RADIUS - 0.5, 0))
	add_child(floor_body)
	var box := BoxShape3D.new()
	box.size = Vector3(4000, 1, 4000)
	var collider := CollisionShape3D.new()
	collider.shape = box
	floor_body.add_child(collider)
	var mesh := BoxMesh.new()
	mesh.size = box.size
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(0.12, 0.2, 0.25)
	mesh.material = mat
	var visual := MeshInstance3D.new()
	visual.mesh = mesh
	floor_body.add_child(visual)
	ship = Ship.new()
	ship.planet = planet
	ship.position = world_position(id, Vector3((slot - 1) * 20, RADIUS + 0.05, 0))
	add_child(ship)
	ship.piloted = true
	ship.camera.make_current()
	if with_walker:
		walker = NetworkWalker.new()
		walker.planet = planet
		walker.planet_center = planet.centre
		walker.position = ship.position + Vector3(6, 0, 0)
		add_child(walker)
		walker.enter_ship_frame(ship)
		walker.position = Ship.SEAT_POS
		walker.set_physics_process(false)
		walker.get_camera().current = false

func world_position(id: int, p: Vector3) -> Vector3:
	return Vector3(float(CENTRES[id].x) + float(p.x) - origin_x,
		float(CENTRES[id].y) + float(p.y) - origin_y,
		float(CENTRES[id].z) + float(p.z) - origin_z)

func relative_position(id: int, p: Vector3) -> Vector3:
	return Vector3(float(p.x) + (origin_x - float(CENTRES[id].x)),
		float(p.y) + (origin_y - float(CENTRES[id].y)),
		float(p.z) + (origin_z - float(CENTRES[id].z)))

func shift(offset: Vector3) -> void:
	if offset == Vector3.ZERO:
		return
	for child in get_children():
		if child is Node3D:
			if child is RigidBody3D and not child.freeze:
				# _process can see a visual transform one physics step behind Jolt.
				# Shifting that old transform rewinds a moving ship by one step.
				var pose: Transform3D = PhysicsServer3D.body_get_state(child.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM)
				pose.origin -= offset
				child.global_transform = pose
			else:
				child.position -= offset
	origin_x += float(offset.x)
	origin_y += float(offset.y)
	origin_z += float(offset.z)
	planet.centre -= offset
	if walker:
		walker.planet_center = planet.centre
	shifts += 1

func state(owner: int, t: float, seq: int) -> Dictionary:
	var s := Snapshot.make(owner, t, relative_position(planet_id, ship.global_position),
		ship.linear_velocity, ship.global_basis.get_rotation_quaternion())
	s.planet = planet_id
	s.seq = seq
	if walker:
		s.frame = 1 if walker.ship_frame else 0
		s.frame_id = int(walker.ship_frame.get_meta("owner", owner)) if walker.ship_frame else 0
		s.wp = walker.position if walker.ship_frame else relative_position(planet_id, walker.global_position)
		s.wv = walker.ship_frame.global_basis.inverse() * walker.velocity if walker.ship_frame else walker.velocity
		s.wq = walker.basis.get_rotation_quaternion() if walker.ship_frame else walker.global_basis.get_rotation_quaternion()
	return s

static func drive(body: RigidBody3D, t: float) -> String:
	body.piloted = false
	body.test_input = Vector3.ZERO
	body.test_roll = 0.0
	if t < 4.0:
		body.test_input.y = 1.0
		return "takeoff"
	if t < 10.0:
		body.test_input.z = -1.0
		return "cruise"
	if t < 16.0:
		body.test_input.z = -1.0
		body._mouse = Vector2(0.003, 0)
		return "turn"
	if t < 24.0:
		body.test_input.y = -1.0
		return "landing"
	return "idle"
