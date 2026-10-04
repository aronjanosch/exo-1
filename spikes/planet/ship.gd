extends RigidBody3D
## Ship on Jolt with inertia: thrust, radial gravity and quadratic air drag
## (atmosphere only). Without the hover assist it keeps its momentum and glides
## in space. Engine gravity is off; gravity comes from the planet model.
## Rotation is still rate-controlled (like a flight computer): an assumption.
## Mouse: pitch and yaw. W/S thrust, A/D strafe, Space/Ctrl up/down, Q/E roll,
## Shift boost, H flight assist on/off (default on), L horizon follow on/off.
## Horizon follow (default on, initiator's decision): "straight" means along the
## horizon, not off the planet. The ship's frame turns with the local up as it
## moves over the sphere, so its pitch relative to the horizon stays constant.
## All numbers are start values.
## Spike 3 assumptions (agreed for testing, not designed): walkable greybox
## cabin with a ramp at the back, gravity inside always towards the cabin
## floor, F at the seat to sit/stand, hover assist holds the ship when nobody
## sits at the controls.

const SpikeInput := preload("res://spikes/planet/spike_input.gd")

@export var thrust_accel := 20.0  # m/s^2
@export var boost_factor := 5.0
@export var turn_rate := 2.5  # rad/s cap
@export var roll_rate := 1.8
@export var mouse_sensitivity := 0.002  # rad per pixel
@export var assisted_accel := 30.0  # m/s^2; all assisted correction shares one budget
@export var assisted_braking := 40.0  # m/s^2
@export var assisted_boost_accel := 60.0
@export var assisted_reverse_speed := 25.0
@export var assisted_strafe_speed := 20.0
@export var assisted_vertical_speed := 15.0
## (terrain clearance in metres, forward speed in m/s). Spike tuning only.
@export var forward_speed_curve := PackedVector2Array([
	Vector2(30, 25), Vector2(150, 60), Vector2(600, 150), Vector2(1200, 350),
])
## Quadratic drag a = k * density * v^2. Start value: terminal speed at the
## surface about 200 m/s with normal thrust, about 450 m/s with boost.
@export var drag_k := 0.0005
## Landing aid with hover assist: sink rate capped to this share of the height
## above ground per second (but never below 2 m/s).
@export var landing_sink_factor := 0.5

var planet: Node  # gravity_at(pos), density_at(pos), height_at(dir), planet_radius, to_planet(pos)
var piloted := false
var hover_assist := true  # H; assisted velocity goals, off preserves the original glide
var horizon_follow := true
var commanded_speed := 0.0
var forward_speed_limit := 25.0
var terrain_clearance := 0.0
var camera: Camera3D
## Seat position in ship space; the walker sits here.
const SEAT_POS := Vector3(0, 0.6, -3.0)
## Test drive while nobody pilots (auto-test only): local thrust input, roll, boost.
var test_input := Vector3.ZERO
var test_roll := 0.0
var test_boost := 1.0

var _mouse := Vector2.ZERO
var _horizon_w := Vector3.ZERO


func _ready() -> void:
	gravity_scale = 0.0
	mass = 2000.0
	collision_layer = 1
	collision_mask = 1  # ignores the walker (layer 2), so walking inside does not push the ship
	continuous_cd = true
	# Godot damps every rigid body by default (0.1/s); in space nothing should.
	linear_damp_mode = RigidBody3D.DAMP_MODE_REPLACE
	linear_damp = 0.0
	angular_damp_mode = RigidBody3D.DAMP_MODE_REPLACE
	angular_damp = 0.0
	can_sleep = false
	_build_body()

	camera = Camera3D.new()
	camera.near = 0.05
	camera.far = 50000.0
	camera.fov = 75.0
	camera.position = Vector3(0, 5.5, 17)
	camera.rotation.x = deg_to_rad(-10)
	add_child(camera)


## Greybox with a walkable cabin: floor, walls, roof, front, open back with a
## ramp. Origin is at the floor bottom. Cabin inside: 4 m wide, 2.6 m high, 8 m long.
func _build_body() -> void:
	var hull_mat := StandardMaterial3D.new()
	hull_mat.albedo_color = Color(0.95, 0.5, 0.15)
	var inner_mat := StandardMaterial3D.new()
	inner_mat.albedo_color = Color(0.55, 0.55, 0.6)
	var glass_mat := StandardMaterial3D.new()
	glass_mat.albedo_color = Color(0.3, 0.8, 0.9)

	# [size, position, x rotation in degrees, material, collides]
	var parts := [
		[Vector3(4.0, 0.3, 8.0), Vector3(0, 0.15, 0), 0.0, inner_mat, true],  # floor
		[Vector3(0.3, 2.6, 8.0), Vector3(-2.15, 1.6, 0), 0.0, hull_mat, true],  # left wall
		[Vector3(0.3, 2.6, 8.0), Vector3(2.15, 1.6, 0), 0.0, hull_mat, true],  # right wall
		[Vector3(4.6, 0.3, 8.0), Vector3(0, 3.05, 0), 0.0, hull_mat, true],  # roof
		[Vector3(4.6, 2.6, 0.3), Vector3(0, 1.6, -4.15), 0.0, hull_mat, true],  # front
		[Vector3(3.6, 1.0, 0.05), Vector3(0, 2.0, -3.98), 0.0, glass_mat, false],  # window
		[Vector3(1.0, 0.5, 0.8), Vector3(0, 0.55, -3.3), 0.0, inner_mat, false],  # seat
		[Vector3(9.0, 0.25, 2.0), Vector3(0, 1.2, 1.0), 0.0, hull_mat, false],  # wings
	]
	_build_ramp(inner_mat)
	for part in parts:
		var box := BoxMesh.new()
		box.size = part[0]
		box.material = part[3]
		var mi := MeshInstance3D.new()
		mi.mesh = box
		mi.position = part[1]
		mi.rotation_degrees.x = part[2]
		add_child(mi)
		if part[4]:
			var shape := BoxShape3D.new()
			shape.size = part[0]
			var col := CollisionShape3D.new()
			col.shape = shape
			col.position = part[1]
			col.rotation_degrees.x = part[2]
			add_child(col)


## Ramp from the floor edge (z 4, y 0.3) down to 0.5 m below the hull bottom
## (z 6.6), so it reaches the ground on slopes too. Its own body on layer 4:
## only the walker collides with it, so it can dig into the terrain without
## pushing the ship.
func _build_ramp(mat: Material) -> void:
	var ramp := AnimatableBody3D.new()
	ramp.collision_layer = 4
	# With sync_to_physics (default on) the ramp did not follow the ship: it
	# stayed where the ship was created, at the planet centre (found in spike 5).
	ramp.sync_to_physics = false
	ramp.collision_mask = 0
	# Solid wedge down to 1.5 m below the hull: a thin slab let the walker slip
	# underneath where the terrain dips below the ship.
	var shape := ConvexPolygonShape3D.new()
	var pts := PackedVector3Array()
	for x in [-1.5, 1.5]:
		pts.append_array([Vector3(x, 0.3, 4.0), Vector3(x, -0.5, 6.6), Vector3(x, -1.5, 6.6), Vector3(x, -1.5, 4.0)])
	shape.points = pts
	var col := CollisionShape3D.new()
	col.shape = shape
	ramp.add_child(col)
	# Visual: just the walking surface.
	var box := BoxMesh.new()
	box.size = Vector3(3.0, 0.1, 2.72)
	box.material = mat
	var mi := MeshInstance3D.new()
	mi.mesh = box
	mi.position = Vector3(0, -0.148, 5.285)
	mi.rotation_degrees.x = 17.1
	ramp.add_child(mi)
	add_child(ramp)


## True if a world position (the walker's feet) is inside the cabin. `margin`
## grows the box (positive) or shrinks it (negative) for hysteresis.
func cabin_contains(world_pos: Vector3, margin := 0.0) -> bool:
	var p := to_local(world_pos)
	return absf(p.x) < 1.95 + margin and p.y > 0.0 - margin and p.y < 2.9 + margin \
		and absf(p.z) < 4.0 + margin


func _unhandled_input(event: InputEvent) -> void:
	if not piloted:
		return
	if event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		_mouse += event.relative * mouse_sensitivity
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.physical_keycode == KEY_H:
			hover_assist = not hover_assist
		elif event.physical_keycode == KEY_L:
			horizon_follow = not horizon_follow


func clearance_at(world_pos: Vector3) -> float:
	var p: Vector3 = planet.to_planet(world_pos)
	return p.length() - planet.planet_radius - planet.height_at(p.normalized())


func forward_speed_at(clearance: float) -> float:
	for i in range(1, forward_speed_curve.size()):
		var lo := forward_speed_curve[i - 1]
		var hi := forward_speed_curve[i]
		if clearance <= hi.x:
			return lerpf(lo.y, hi.y, smoothstep(lo.x, hi.x, clearance))
	return forward_speed_curve[forward_speed_curve.size() - 1].y


## Preview terrain over the braking horizon. This lowers the requested speed;
## it does not snap velocity or promise collision avoidance on every approach.
func _flight_clearance(world_pos: Vector3, v: Vector3, current_clearance: float) -> float:
	var p: Vector3 = planet.to_planet(world_pos)
	var up := p.normalized()
	var sink := maxf(0.0, -v.dot(up))
	var clearance := current_clearance - sink * 0.5 - sink * sink / (2.0 * assisted_braking)
	var preview_time := 0.5 + v.length() / assisted_braking
	var horizon_w := up.cross(v) / p.length()
	for i in range(1, 4):
		var t := preview_time * float(i) / 3.0
		var preview := world_pos + v * t
		if horizon_follow and horizon_w.length_squared() > 0.00000001:
			# Follow the curved path instead of looking along a line off the planet.
			preview = world_pos + p.rotated(horizon_w.normalized(), horizon_w.length() * t) - p + up * v.dot(up) * t
		clearance = minf(clearance, clearance_at(preview))
	return clearance


func _integrate_forces(state: PhysicsDirectBodyState3D) -> void:
	var dt := state.step
	var b := state.transform.basis
	var gravity: Vector3 = planet.gravity_at(state.transform.origin)
	var density: float = planet.density_at(state.transform.origin)

	var input := Vector3.ZERO
	var roll := 0.0
	var boost := 1.0
	if piloted:
		input = Vector3(SpikeInput.axis(KEY_D, KEY_A), SpikeInput.axis(KEY_SPACE, KEY_CTRL), -SpikeInput.axis(KEY_W, KEY_S))
		roll = SpikeInput.axis(KEY_Q, KEY_E)
		boost = boost_factor if SpikeInput.pressed(KEY_SHIFT) else 1.0
	else:
		input = test_input
		roll = test_roll
		boost = test_boost

	var v := state.linear_velocity
	var drag := -v * drag_k * density * v.length()
	if hover_assist:
		var pos: Vector3 = planet.to_planet(state.transform.origin)
		var up := pos.normalized()
		terrain_clearance = clearance_at(state.transform.origin)
		var clearance := _flight_clearance(state.transform.origin, v, terrain_clearance)
		forward_speed_limit = forward_speed_at(clearance)
		if boost > 1.0:
			# Boost stays gentle near terrain and cannot exceed high-altitude cruise.
			forward_speed_limit = lerpf(forward_speed_limit,
				minf(forward_speed_curve[-1].y, forward_speed_limit * 2.5), smoothstep(30.0, 150.0, clearance))
		var request := input.limit_length(1.0)
		var forward_speed := forward_speed_limit if request.z < 0.0 else assisted_reverse_speed
		var goal := b * Vector3(request.x * assisted_strafe_speed,
			request.y * assisted_vertical_speed, request.z * forward_speed)
		# Slow the requested descent near the ground, without clamping momentum.
		var sink_goal := -goal.dot(up)
		var sink_cap := maxf(2.0, maxf(0.0, terrain_clearance) * landing_sink_factor)
		if sink_goal > sink_cap:
			goal += up * (sink_goal - sink_cap)
		commanded_speed = goal.length()
		var correction := (goal - v) / dt
		var budget := assisted_boost_accel if boost > 1.0 else assisted_accel
		if correction.dot(v) < 0.0:
			budget = assisted_braking
		var curve_accel := Vector3.ZERO
		if horizon_follow:
			curve_accel = (up.cross(v) / pos.length()).cross(v)
		# Gravity cancellation is the existing arcade hover assumption. Drag,
		# turning and velocity correction compete within one thrust budget.
		# Reserve thrust for the curved path/drag first. Otherwise a large speed
		# error consumes the entire budget and the ship climbs while accelerating.
		var support := (curve_accel - drag).limit_length(budget)
		var thrust := support + correction.limit_length(maxf(0.0, budget - support.length()))
		v += (thrust + drag) * dt
	else:
		commanded_speed = 0.0
		v += (b * input.limit_length(1.0)) * thrust_accel * boost * dt
		v += gravity * dt
		v -= v * minf(1.0, drag_k * density * v.length() * dt)
	state.linear_velocity = v

	# Rotation: mouse movement is an angle per step (like mouse look), capped
	# at turn_rate and smoothed a little so the ship has some weight.
	var pitch := clampf(-_mouse.y / dt, -turn_rate, turn_rate)
	var yaw := clampf(-_mouse.x / dt, -turn_rate, turn_rate)
	_mouse = Vector2.ZERO
	var target_w := b * Vector3(pitch, yaw, roll * roll_rate)
	# Smooth the player's rotation, not the changing planet frame. Smoothing
	# horizon transport creates a persistent outward pitch at high speed.
	var control_w := state.angular_velocity - _horizon_w
	_horizon_w = Vector3.ZERO
	if horizon_follow:
		# Rate at which the local up turns while moving over the sphere:
		# d(up)/dt = v_tangential / r  =>  w = up x v / r.
		var pos: Vector3 = planet.to_planet(state.transform.origin)
		_horizon_w = pos.normalized().cross(state.linear_velocity) / pos.length()
	state.angular_velocity = control_w.lerp(target_w, minf(1.0, 12.0 * dt)) + _horizon_w
