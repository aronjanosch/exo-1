extends SceneTree
## Free a live terrain/ring scene with unreaped worker tasks. A clean process
## exit matters: the original bug crashed only after the gameplay report.

const Terrain := preload("res://spikes/planet/terrain.gd")
const Ring := preload("res://spikes/planet/collision_ring.gd")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var world := Node3D.new()
	root.add_child(world)
	var camera := Camera3D.new()
	camera.position = Vector3(0, 5001, 0)
	world.add_child(camera)
	camera.make_current()
	var terrain := Terrain.new()
	terrain.radius = 5000.0
	terrain.max_depth = 2
	terrain.stats = {}
	world.add_child(terrain)
	var anchor := CharacterBody3D.new()
	anchor.position = Vector3.UP * (terrain.radius + terrain.height_at(Vector3.UP) + 1.0)
	world.add_child(anchor)
	var ring := Ring.new()
	ring.terrain = terrain
	ring.stats = {}
	ring.set_anchors([anchor])
	world.add_child(ring)
	terrain._process(0.0)
	ring._process(0.0)
	var terrain_tasks: int = terrain._pending.size()
	var ring_tasks: int = ring._pending.size()
	if terrain_tasks == 0 or ring_tasks == 0:
		push_error("Shutdown fixture must contain both kinds of worker task")
		quit(1)
		return
	var jobs: Array[WeakRef] = []
	for job in terrain._pending:
		jobs.append(weakref(job))
	for job in ring._pending.values():
		jobs.append(weakref(job))
	world.free()
	for job in jobs:
		if job.get_ref() != null:
			push_error("Worker still retains a script job after scene removal")
			quit(1)
			return
	print("PASS shutdown releases %d terrain and %d collision tasks" % [terrain_tasks, ring_tasks])
	quit(0)
