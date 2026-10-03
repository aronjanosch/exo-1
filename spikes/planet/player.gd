extends CharacterBody3D
## First-person walker with radial gravity. "Up" is away from the planet centre.
## V toggles a debug fly mode (no gravity, no collision) to inspect the planet.
## Mouse wheel changes fly speed.
## Inside a ship cabin the walker is a child of the ship and walks in the ship's
## frame: the parent transform carries it along, velocity is relative to the
## ship, gravity points to the cabin floor (spike 3 assumption).

const SpikeInput := preload("res://spikes/planet/spike_input.gd")

@export var planet_center := Vector3.ZERO
@export var gravity := 9.81  # fallback when there is no planet model
@export var walk_speed := 5.0
@export var run_speed := 12.0
@export var jump_speed := 5.0
@export var mouse_sensitivity := 0.0025
@export var eye_height := 1.7

## Safety net under the collision ring: anything with radius and height_at(dir).
var terrain: Node3D
var planet: Node  # gravity_at(pos), stats
var ring: Node  # has_patch_near(pos)
var fly_mode := false
var _grounded := false
var _was_grounded_last := false
var fly_speed := 50.0
## The ship whose cabin the walker is in, or null.
var ship_frame: Node3D

var _head: Node3D
var _camera: Camera3D
var _collision: CollisionShape3D
var _pitch := 0.0
var _yaw_input := 0.0


func _ready() -> void:
	var capsule := CapsuleShape3D.new()
	capsule.radius = 0.35
	capsule.height = 1.8
	_collision = CollisionShape3D.new()
	_collision.shape = capsule
	_collision.position.y = 0.9
	add_child(_collision)

	_head = Node3D.new()
	_head.position.y = eye_height
	add_child(_head)

	_camera = Camera3D.new()
	_camera.near = 0.05
	_camera.far = 50000.0
	_camera.fov = 75.0
	_head.add_child(_camera)
	_camera.make_current()

	collision_layer = 2  # ships ignore this layer, so walking inside never pushes them
	collision_mask = 1 | 4  # world and ships, plus ship ramps (layer 4)
	platform_floor_layers = 0  # the parent transform already carries us; no platform velocity on top
	floor_max_angle = deg_to_rad(50.0)
	floor_snap_length = 0.5
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--safe-margin="):  # spike 5 diagnostic, default 0.001
			safe_margin = arg.trim_prefix("--safe-margin=").to_float()
	# Scripted runs never grab the mouse: capturing pulls the pointer and focus
	# to the game window, even when it starts unfocused on another workspace.
	if not SpikeInput.scripted():
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		_yaw_input -= event.relative.x * mouse_sensitivity
		_pitch = clampf(_pitch - event.relative.y * mouse_sensitivity, -1.5, 1.5)
		_head.rotation.x = _pitch
	elif event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			fly_speed = minf(fly_speed * 1.25, 5000.0)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			fly_speed = maxf(fly_speed / 1.25, 1.0)
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.physical_keycode == KEY_V:
			fly_mode = not fly_mode
			_collision.disabled = fly_mode
			velocity = Vector3.ZERO


func _physics_process(delta: float) -> void:
	var up := ship_frame.global_basis.y if ship_frame else (global_position - planet_center).normalized()
	_align_to_up(up)

	var b := global_transform.basis
	var input := Vector2(
		SpikeInput.axis(KEY_D, KEY_A),
		SpikeInput.axis(KEY_W, KEY_S))

	if fly_mode:
		var cam_b := _camera.global_transform.basis
		var dir := cam_b.x * input.x - cam_b.z * input.y
		dir += up * SpikeInput.axis(KEY_SPACE, KEY_CTRL)
		var speed := fly_speed * (4.0 if SpikeInput.pressed(KEY_SHIFT) else 1.0)
		velocity = dir.normalized() * speed if dir.length_squared() > 0.0 else Vector3.ZERO
		global_position += velocity * delta
		return

	var speed := run_speed if SpikeInput.pressed(KEY_SHIFT) else walk_speed
	var horizontal := (b.x * input.x - b.z * input.y).limit_length(1.0) * speed
	var vertical := velocity.dot(up)
	var jumping := false
	if _grounded or is_on_floor():
		jumping = SpikeInput.pressed(KEY_SPACE)
		vertical = jump_speed if jumping else 0.0
	else:
		var g: float = gravity
		if planet and not ship_frame:
			g = planet.gravity_at(global_position).length()
		vertical -= g * delta
	velocity = horizontal + up * vertical
	up_direction = up
	move_and_slide()
	_grounded = is_on_floor()
	if terrain and not ship_frame:
		_ground_safety(jumping)


## Called deferred by main when the walker enters or leaves a cabin. Keeps the
## world position; converts between world and ship-relative velocity.
func enter_ship_frame(ship: RigidBody3D) -> void:
	if ship_frame == ship:
		return
	velocity -= ship.linear_velocity
	reparent(ship, true)
	ship_frame = ship


func leave_ship_frame(world: Node3D) -> void:
	if ship_frame == null:
		return
	var ship := ship_frame as RigidBody3D
	ship_frame = null
	reparent(world, true)
	velocity += ship.linear_velocity


## Where a collision patch exists, only catch real fall-throughs (and count
## them). Where none exists yet, keep the feet on the CPU height function.
func _ground_safety(jumping: bool) -> void:
	var offset := global_position - planet_center
	var dist := offset.length()
	var dir := offset / dist
	var surface: float = terrain.radius + terrain.height_at(dir)
	var covered: bool = ring != null and ring.has_patch_near(global_position)
	var snap := false
	if dist < surface - 0.3:
		snap = true
		if covered and planet:
			planet.stats.rescues += 1
	elif not covered and (dist < surface or (_was_grounded_last and not jumping and dist - surface < 0.5)):
		snap = true
	if snap:
		global_position = planet_center + dir * surface
		velocity -= dir * velocity.dot(dir)
		_grounded = true
	_was_grounded_last = _grounded


## Debug helper: point the camera at the planet centre (used in fly mode).
func look_at_planet(pitch := -1.5) -> void:
	var to_center := (planet_center - global_position).normalized()
	_align_to_up(-to_center)
	_pitch = pitch
	_head.rotation.x = _pitch


## Debug helper: turn head and body towards a world point.
func look_at_point(target: Vector3) -> void:
	var up := (global_position - planet_center).normalized()
	var to := target - global_position
	var flat := (to - up * to.dot(up)).normalized()
	var right := flat.cross(up)
	global_transform.basis = Basis(right, up, -flat)
	_pitch = atan2(to.dot(up), (to - up * to.dot(up)).length())
	_head.rotation.x = _pitch


func get_camera() -> Camera3D:
	return _camera


## Keep the heading, rotate so local Y points along `up`.
func _align_to_up(up: Vector3) -> void:
	var b := global_transform.basis.rotated(up, _yaw_input) if _yaw_input != 0.0 else global_transform.basis
	_yaw_input = 0.0
	var forward := -b.z
	forward = forward - up * forward.dot(up)
	if forward.length_squared() < 1e-6:
		forward = b.y - up * b.y.dot(up)
	forward = forward.normalized()
	var right := forward.cross(up)
	global_transform.basis = Basis(right, up, -forward)

