extends "res://spikes/planet/player.gd"
## Same movement controller; ready builds the capsule without mouse capture.
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
	_camera.far = 250000.0
	_camera.fov = 75.0
	_head.add_child(_camera)
	collision_layer = 2
	collision_mask = 1 | 4
	platform_floor_layers = 0
	floor_max_angle = deg_to_rad(50)
	floor_snap_length = 0.5
