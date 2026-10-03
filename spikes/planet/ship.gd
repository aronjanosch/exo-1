extends RigidBody3D
## Simple arcade ship on Jolt. Engine gravity is off; radial gravity and drag
## come from the planet model and are blended by altitude.
## Mouse: pitch and yaw. W/S thrust, A/D strafe, Space/Ctrl up/down, Q/E roll,
## Shift boost, H hover assist on/off, L horizon follow on/off.
## Horizon follow (default on, initiator's decision): "straight" means along the
## horizon, not off the planet. The ship's frame turns with the local up as it
## moves over the sphere, so its pitch relative to the horizon stays constant.
## All numbers are start values.

const SpikeInput := preload("res://spikes/planet/spike_input.gd")

@export var thrust_accel := 20.0  # m/s^2
@export var boost_factor := 5.0
@export var turn_rate := 2.5  # rad/s cap
@export var roll_rate := 1.8
@export var mouse_sensitivity := 0.002  # rad per pixel
@export var assist_damping := 1.2  # 1/s, brakes when there is no input
@export var air_drag := 0.25  # 1/s at full atmosphere density
## Landing aid with hover assist: sink rate capped to this share of the height
## above ground per second (but never below 2 m/s).
@export var landing_sink_factor := 0.5

var planet: Node  # gravity_at(pos), density_at(pos), height_at(dir), planet_radius, to_planet(pos)
var piloted := false
var hover_assist := true
var horizon_follow := true
var camera: Camera3D

var _mouse := Vector2.ZERO


func _ready() -> void:
	gravity_scale = 0.0
	mass = 2000.0
	continuous_cd = true
	can_sleep = false
	_build_body()

	camera = Camera3D.new()
	camera.near = 0.05
	camera.far = 50000.0
	camera.fov = 75.0
	camera.position = Vector3(0, 3.5, 12)
	camera.rotation.x = deg_to_rad(-10)
	add_child(camera)


## Goofy greybox: a fat orange hull, a cockpit bubble and two stubby wings.
func _build_body() -> void:
	var shape := BoxShape3D.new()
	shape.size = Vector3(3.0, 1.4, 5.5)
	var col := CollisionShape3D.new()
	col.shape = shape
	add_child(col)

	var hull_mat := StandardMaterial3D.new()
	hull_mat.albedo_color = Color(0.95, 0.5, 0.15)
	var glass_mat := StandardMaterial3D.new()
	glass_mat.albedo_color = Color(0.3, 0.8, 0.9)
	var parts := [
		[Vector3(3.0, 1.4, 5.5), Vector3.ZERO, hull_mat],
		[Vector3(1.6, 0.8, 1.8), Vector3(0, 0.9, -1.2), glass_mat],
		[Vector3(6.0, 0.25, 1.6), Vector3(0, -0.2, 0.8), hull_mat],
	]
	for part in parts:
		var box := BoxMesh.new()
		box.size = part[0]
		box.material = part[2]
		var mi := MeshInstance3D.new()
		mi.mesh = box
		mi.position = part[1]
		add_child(mi)


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

	var v := state.linear_velocity
	v += (b * input.limit_length(1.0)) * thrust_accel * boost * dt
	if hover_assist and piloted:
		# Drone-like: cancel gravity and brake on axes without input.
		var local_v := b.inverse() * v
		for axis in 3:
			if input[axis] == 0.0:
				local_v[axis] -= local_v[axis] * minf(1.0, assist_damping * dt)
		v = b * local_v
		var pos: Vector3 = planet.to_planet(state.transform.origin)
		var up := pos.normalized()
		var agl: float = pos.length() - planet.planet_radius - planet.height_at(up)
		var sink := -v.dot(up)
		var cap := maxf(2.0, agl * landing_sink_factor)
		if sink > cap:
			v += up * (sink - cap)
	else:
		v += gravity * dt
	v -= v * minf(1.0, air_drag * density * dt)
	state.linear_velocity = v

	# Rotation: mouse movement is an angle per step (like mouse look), capped
	# at turn_rate and smoothed a little so the ship has some weight.
	var pitch := clampf(-_mouse.y / dt, -turn_rate, turn_rate)
	var yaw := clampf(-_mouse.x / dt, -turn_rate, turn_rate)
	_mouse = Vector2.ZERO
	var target_w := b * Vector3(pitch, yaw, roll * roll_rate)
	if horizon_follow:
		# Rate at which the local up turns while moving over the sphere:
		# d(up)/dt = v_tangential / r  =>  w = up x v / r.
		var pos: Vector3 = planet.to_planet(state.transform.origin)
		target_w += pos.normalized().cross(state.linear_velocity) / pos.length()
	state.angular_velocity = state.angular_velocity.lerp(target_w, minf(1.0, 12.0 * dt))

