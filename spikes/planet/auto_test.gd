extends Node
## Scripted test run for measurements: walk, board the ship, climb to space,
## come back, cruise low, land. Holds keys through SpikeInput (focus-independent)
## and taps one-shot keys (F) as real input events. Prints a summary per phase and
## writes it to user://spike_results.txt. Run: `godot --path . -- --auto-test`.

const SpikeInput := preload("res://spikes/planet/spike_input.gd")
const JitterProbe := preload("res://spikes/planet/jitter_probe.gd")

var main: Node3D
var overlay: CanvasLayer

var _phase := ""
var _frames: PackedFloat32Array = []
var _report: PackedStringArray = []
var _rescues_at_start := 0
var _skip_frames := 0  # frames after a screenshot (readback + PNG) are not measured
var _probe: Node
var _tick_steps: PackedFloat32Array = []  # per physics tick, planet space, while recording
var _record_ticks := false
var _last_tick_pos := Vector3.ZERO


func _ready() -> void:
	_probe = JitterProbe.new()
	_probe.main = main
	add_child(_probe)
	if "--fly-out" in OS.get_cmdline_user_args():
		_run_fly()
	elif "--fly-to-second" in OS.get_cmdline_user_args():
		_run_second()
	else:
		_run()


## Steps in world coordinates: a planet-relative difference has the float
## resolution of the planet distance (7.8 mm at 100 km), not of the body.
## Ticks with an origin shift are skipped.
var _last_tick_shifts := 0
var _last_tick_vel := Vector3.ZERO
var _tick_count := 0


func _physics_process(_delta: float) -> void:
	var p: Vector3 = main.active.global_position
	if _record_ticks and main.shift_count == _last_tick_shifts:
		_tick_count += 1
		# Deviation of the actual step from velocity * dt (velocity of the step
		# just taken), after the first ticks of the drift.
		var expected: Vector3 = _last_tick_vel * _delta
		if _tick_count > 10:
			_tick_steps.append((p - _last_tick_pos - expected).length())
	_last_tick_pos = p
	_last_tick_vel = main.ship.linear_velocity
	_last_tick_shifts = main.shift_count


## Spike 5 runs board directly at the seat (spike 3 cabin) and fly with the
## hover assist on, which spike 3 made optional (off by default).
func _board_at_seat(ship: RigidBody3D) -> void:
	main.player.enter_ship_frame(ship)
	main.player.position = Vector3(0, 0.32, -2.5)
	await _wait(0.3)
	_tap(KEY_F)
	await _wait(0.5)
	ship.hover_assist = true


## Spike 5, question 4: take off, fly to the second planet, land, walk.
## The transfer is a test autopilot that sets the velocity directly (not a
## flight model): accelerate at 50 m/s^2 up to 3000 m/s, brake to stop 500 m
## above the near side of planet 2, turn upright, then land with the normal
## controls. Speeds are test values, not designed.
func _run_second() -> void:
	await _wait(3.0)
	var ship: RigidBody3D = main.ship
	await _board_at_seat(ship)

	_begin("lift off to 2000 m")
	_keys([KEY_SPACE, KEY_SHIFT], true)
	var t := 0.0
	while _altitude() < 2000.0 and t < 40.0:
		t += await _frame()
	_keys([KEY_SPACE, KEY_SHIFT], false)
	_end("%.1f s" % t)

	_begin("transfer to planet 2")
	ship.hover_assist = false
	var speed := 0.0
	var max_speed := 0.0
	t = 0.0
	while t < 900.0:
		var p0: Vector3 = main.planets[0].center
		var p1: Vector3 = main.planets[1].center
		var up_b := (p0 - p1).normalized()
		var target: Vector3 = p1 + up_b * (main.planets[1].radius + 500.0)
		var to := target - ship.global_position
		var dist := to.length()
		if dist < 5.0:
			break
		var dt := await _frame()
		t += dt
		speed = minf(minf(speed + 50.0 * dt, 3000.0), sqrt(2.0 * 50.0 * dist))
		max_speed = maxf(max_speed, speed)
		ship.linear_velocity = to / dist * speed
	ship.linear_velocity = Vector3.ZERO
	var up_local: Vector3 = main.to_planet(ship.global_position).normalized()
	ship.global_transform = Transform3D(main._basis_for_up(up_local), ship.global_position)
	ship.hover_assist = true
	_end("%.1f s, top speed %.0f m/s, current planet %d, switches %d, dist origin %.0f m" % [
		t, max_speed, main.planets.find(main.current), main.planet_switches, ship.global_position.length()])
	_shot("second-arrive")

	_begin("land on planet 2")
	_keys([KEY_CTRL], true)
	t = 0.0
	while t < 60.0 and ship.linear_velocity.length() > 0.05 or t < 3.0:
		t += await _frame()
	_keys([KEY_CTRL], false)
	_shot("second-landed")
	_end("%.1f s, %.2f m above ground" % [t, _above_ground()])
	await _stand_still("idle 5 s (landed on planet 2)", ship)

	_tap(KEY_F)  # stand up in the cabin, then down the ramp (spike 3)
	await _wait(0.5)
	main.player.look_at_point(ship.to_global(Vector3(0, 1.0, 12)))
	_keys([KEY_W], true)
	await _wait(4.0)
	_keys([KEY_W], false)
	await _wait(1.0)
	await _stand_still("stand still 5 s (walker, planet 2)", main.player)
	var p_start: Vector3 = main.player.global_position
	main.player.look_at_point(p_start + (p_start - ship.global_position))
	_begin("walk 10 s on planet 2")
	var start: Vector3 = main.to_planet(p_start)
	var uncovered := 0
	_keys([KEY_W, KEY_SHIFT], true)
	t = 0.0
	while t < 10.0:
		t += await _frame()
		if not main.ring.has_patch_near(main.player.global_position):
			uncovered += 1
	_keys([KEY_W, KEY_SHIFT], false)
	_end("walked %.0f m, outside ship %s, frames without patch %d, dist origin %.0f m" % [
		start.distance_to(main.to_planet(main.player.global_position)), main.player.ship_frame == null, uncovered,
		main.player.global_position.length()])
	_shot("second-walk")
	_finish()


## Spike 5, question 2: fly the ship out to 100 km from the origin and measure
## at each mark: standing still, drifting at 2 m/s, jitter, frame times.
func _run_fly() -> void:
	await _wait(3.0)
	var ship: RigidBody3D = main.ship
	await _board_at_seat(ship)
	for mark in [10000.0, 25000.0, 50000.0, 100000.0]:
		_begin("climb to %d km" % int(mark / 1000.0))
		_keys([KEY_SPACE, KEY_SHIFT], true)
		var t := 0.0
		while main.true_distance(ship.global_position) < mark and t < 120.0:
			t += await _frame()
		_keys([KEY_SPACE, KEY_SHIFT], false)
		while ship.linear_velocity.length() > 0.5 and t < 180.0:
			t += await _frame()  # hover assist brakes
		_end("%.1f s, max speed reached on the way" % t)
		await _stand_still("hold at %d km" % int(mark / 1000.0), ship)

		_begin("drift 2 m/s at %d km" % int(mark / 1000.0))
		ship.hover_assist = false
		ship.linear_velocity = -ship.global_transform.basis.z * 2.0
		_tick_steps = PackedFloat32Array()
		_tick_count = 0
		_record_ticks = true
		await _wait(3.0)
		_record_ticks = false
		ship.hover_assist = true
		var steps := _tick_steps.duplicate()
		steps.sort()
		var n := steps.size()
		_end("ticks %d, step deviation from v*dt median %.3f mm max %.3f mm (step about %.1f mm), dist origin %.0f m" % [
			n, steps[n / 2] * 1000.0 if n else 0.0, steps[n - 1] * 1000.0 if n else 0.0,
			2000.0 / Engine.physics_ticks_per_second, main.true_distance(ship.global_position)])
		_shot("fly-%dkm" % int(mark / 1000.0))
	_finish()


func _process(delta: float) -> void:
	if _skip_frames > 0:
		_skip_frames -= 1
	elif _phase != "":
		_frames.append(delta * 1000.0)
		if delta > 0.033:
			var s: Dictionary = main.stats
			print("SPIKE %.1f ms  phase '%s'  alt %.0f  patches %d  chunks %d  pending %d  objects %d" % [
				delta * 1000.0, _phase, _altitude(), s.get("collision_patches", 0), s.chunks_total,
				s.chunks_pending, Performance.get_monitor(Performance.OBJECT_COUNT)])


func _run() -> void:
	if "--board-only" in OS.get_cmdline_user_args():
		await _board_debug()
		get_tree().quit()
		return
	await _wait(3.0)  # initial chunk burst, measured separately below
	_begin("startup-settle")
	await _wait(2.0)
	_end()

	await _stand_still("stand still 5 s (walker)", main.player)

	# Walk away from the parked ship: in spike 1 the walker ran into it at some
	# radii, which looked like a precision wall.
	var p0: Vector3 = main.player.global_position
	main.player.look_at_point(p0 + (p0 - main.ship.global_position))
	if OS.get_cmdline_user_args().has("--trace"):
		for c in main.ship.get_children():
			if c is AnimatableBody3D:
				print("IDS ship %d ramp %d ramp->ship local %s ramp physics origin %s ramp node origin %s" % [
					main.ship.get_instance_id() % 100000, c.get_instance_id() % 100000,
					main.ship.to_local(c.global_position),
					PhysicsServer3D.body_get_state(c.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM).origin, c.global_position])
	_begin("walk 20 s (run)")
	var start: Vector3 = main.to_planet(main.player.global_position)
	_keys([KEY_W, KEY_SHIFT], true)
	var walk_t := 0.0
	var slow_frames := 0
	var uncovered := 0  # frames without a collision patch under the walker
	var last_print := -1.0
	while walk_t < 20.0:
		walk_t += await _frame()
		var v: Vector3 = main.player.velocity
		var up: Vector3 = main.to_planet(main.player.global_position).normalized()
		var hspeed := (v - up * v.dot(up)).length()
		if not main.ring.has_patch_near(main.player.global_position):
			uncovered += 1
		if OS.get_cmdline_user_args().has("--trace") and int(walk_t * 4.0) != int((walk_t - get_process_delta_time()) * 4.0):
			print("TRACE t %.2f agl %.2f patches %d pending %d hspeed %.1f" % [walk_t, _above_ground(),
				main.stats.collision_patches, main.stats.patches_pending, hspeed])
		if walk_t > 0.5 and hspeed < 6.0:
			slow_frames += 1
			if walk_t - last_print > 0.5:
				last_print = walk_t
				print("SLOW t %.2f hspeed %.2f vup %.2f floor %s wall %s agl %.2f  %s" % [
					walk_t, hspeed, v.dot(up), main.player.is_on_floor(), main.player.is_on_wall(), _above_ground(),
					_contacts(main.player, up)])
	_keys([KEY_W, KEY_SHIFT], false)
	var walked: float = start.distance_to(main.to_planet(main.player.global_position))
	_end("walked %.0f m, slow frames %d, frames without patch %d" % [walked, slow_frames, uncovered])
	if "--walk-only" in OS.get_cmdline_user_args():
		_finish()
		return

	# Spike 3: walk up the ramp into the parked ship and sit down.
	var ship: RigidBody3D = main.ship
	_begin("walk into parked ship")
	var behind: Vector3 = main.to_planet(ship.to_global(Vector3(0, 0, 12))).normalized()
	main.player.global_position = main.planet_center + behind * (main.planet_radius + main.height_at(behind) + 0.1)
	main.player.look_at_point(ship.to_global(Vector3(0, 1.5, 0)))
	_keys([KEY_W], true)
	await _wait(5.0)
	_keys([KEY_W], false)
	await _wait(0.3)
	var entered: bool = main.player.ship_frame == ship
	var could_sit: bool = main.near_seat()
	_end("in cabin %s, at seat %s" % [entered, could_sit])
	if not could_sit:  # keep the rest of the run going
		main.player.enter_ship_frame(ship)
		main.player.position = Vector3(0, 0.32, -2.5)
		await _wait(0.3)
	_tap(KEY_F)
	await _wait(0.5)
	ship.hover_assist = true  # the flight phases below were built around the assist

	_begin("climb to 2000 m")
	_keys([KEY_SPACE, KEY_SHIFT], true)
	var shot_500 := false
	var t := 0.0
	while _altitude() < 2000.0 and t < 40.0:
		if not shot_500 and _altitude() > 500.0:
			shot_500 = true
			_shot("climb-500m")
		t += await _frame()
	_keys([KEY_SPACE, KEY_SHIFT], false)
	_shot("space-2000m")
	_end("reached %.0f m in %.1f s" % [_altitude(), t])

	await _glide_in_space()

	_begin("descend to 120 m above ground")
	_keys([KEY_CTRL, KEY_SHIFT], true)
	var shot_300 := false
	t = 0.0
	while _above_ground() > 120.0 and t < 60.0:
		if not shot_300 and _above_ground() < 300.0:
			shot_300 = true
			_shot("descend-300m")
		t += await _frame()
	_keys([KEY_CTRL, KEY_SHIFT], false)
	await _wait(1.5)  # hover assist brakes
	_end("%.1f s" % t)

	_begin("cruise low 15 s (boost)")
	_keys([KEY_W, KEY_SHIFT], true)
	var min_agl := INF
	t = 0.0
	while t < 15.0:
		# Bot altitude hold, like a player would fly: 80-150 m above ground.
		var agl := _above_ground()
		min_agl = minf(min_agl, agl)
		_keys([KEY_SPACE], agl < 80.0)
		_keys([KEY_CTRL], agl > 150.0)
		t += await _frame()
	_keys([KEY_W, KEY_SHIFT, KEY_SPACE, KEY_CTRL], false)
	await _wait(2.0)
	_shot("cruise")
	_end("lowest %.0f m above ground" % min_agl)

	_begin("land")
	_keys([KEY_CTRL], true)
	t = 0.0
	while t < 40.0 and main.ship.linear_velocity.length() > 0.05 or t < 3.0:
		t += await _frame()
	_keys([KEY_CTRL], false)
	_shot("landed")
	_end("%.1f s, %.2f m above ground, rescues so far %d" % [t, _above_ground(), main.stats.rescues])

	await _stand_still("idle 5 s (landed ship)", main.ship)

	await _ship_interior_tests()
	_finish()


func _finish() -> void:
	var text := "\n".join(_report)
	print("\n==== spike 5 auto-test %s ====\n" % " ".join(OS.get_cmdline_user_args()), text)
	var f := FileAccess.open("user://spike5_results.txt", FileAccess.WRITE)
	if f:
		f.store_string(text + "\n")
	get_tree().quit()


## Slide contacts of the last move: normal angle to up, contact height above
## the feet, and which collider (patch bodies are numbered by instance id).
func _contacts(body: CharacterBody3D, up: Vector3) -> String:
	var parts: PackedStringArray = []
	for i in body.get_slide_collision_count():
		var c := body.get_slide_collision(i)
		var n := c.get_normal()
		parts.append("[%.1f deg, h %.3f m, %s]" % [
			rad_to_deg(n.angle_to(up)), (c.get_position() - body.global_position).dot(up),
			str(c.get_collider_id() % 100000)])
	return " ".join(parts)


func _begin(name: String) -> void:
	_phase = name
	# Frame number of each phase start, to cut movie-maker clips (30 fps).
	print("PHASE '%s' frame %d" % [name, Engine.get_frames_drawn()])
	_frames = PackedFloat32Array()
	_rescues_at_start = main.stats.get("rescues", 0)
	if _probe:
		_probe.reset()


func _end(note := "") -> void:
	var sorted := _frames.duplicate()
	sorted.sort()
	var n := sorted.size()
	var avg := 0.0
	for x in sorted:
		avg += x
	avg /= maxi(n, 1)
	var s: Dictionary = main.stats
	_report.append("%-30s frames %5d  avg %5.2f ms  p99 %6.2f ms  max %6.2f ms  >33ms %3d  rescues %d  %s" % [
		_phase, n, avg, sorted[int(n * 0.99)] if n else 0.0, sorted[n - 1] if n else 0.0,
		_count_over(sorted, 33.3), s.get("rescues", 0) - _rescues_at_start, note])
	_report.append("%-30s chunk build avg %.2f max %.2f ms, upload max %.2f ms, lod max %.2f ms, patch avg %.2f max %.2f ms, patches %d" % [
		"", s.build_ms_avg, s.build_ms_max, s.upload_ms_max, s.lod_ms_max,
		s.get("patch_ms_avg", 0.0), s.get("patch_ms_max", 0.0), s.get("collision_patches", 0)])
	_report.append("%-30s main thread max per frame: terrain %.2f ms, ring %.2f ms" % [
		"", s.get("terrain_frame_ms_max", 0.0), s.get("ring_frame_ms_max", 0.0)])
	_report.append("%-30s jitter (calculated) ground 2 m ahead %.2f px (%.2f mm), ship nose %.2f px (%.2f mm); shifts %d, shift max %.2f ms" % [
		"", _probe.ground_px_max, _probe.ground_mm_max, _probe.ship_px_max, _probe.ship_mm_max,
		s.get("shifts", 0), s.get("shift_ms_max", 0.0)])
	if s.get("shifts", 0) > 0:
		_report.append("%-30s shift: max jump relative to planet %.4f mm, max ship speed change %.4f m/s" % [
			"", s.get("shift_jump_mm_max", 0.0), s.get("shift_dv_max", 0.0)])
	_phase = ""
	main.terrain.reset_max_stats()
	main.ring.reset_max_stats()


## Physics check: in space (no drag) with the assist off, the ship keeps its
## momentum. The walker stands up and walks while it glides.
func _glide_in_space() -> void:
	var ship: RigidBody3D = main.ship
	var player: CharacterBody3D = main.player
	_begin("glide in space (assist off)")
	ship.hover_assist = false
	_keys([KEY_W, KEY_SHIFT], true)
	await _wait(2.0)
	_keys([KEY_W, KEY_SHIFT], false)
	var v0: float = ship.linear_velocity.length()
	var alt0 := _altitude()
	_tap(KEY_F)  # stand up
	await _wait(0.3)
	var floor_frames := 0
	var frames := 0
	var left_ship := false
	_keys([KEY_W], true)
	var t := 0.0
	while t < 5.0:
		if t > 1.5 and SpikeInput.held.has(KEY_W):
			_keys([KEY_W], false)
			_keys([KEY_S], true)
		elif t > 2.5 and SpikeInput.held.has(KEY_S):
			_keys([KEY_S], false)
		t += await _frame()
		frames += 1
		if player.is_on_floor():
			floor_frames += 1
		if player.ship_frame != ship:
			left_ship = true
	_keys([KEY_W, KEY_S], false)
	var v1: float = ship.linear_velocity.length()
	_end("ship %.1f -> %.1f m/s in 5 s, altitude %.0f -> %.0f m, walker on floor %d%%, left ship %s" % [
		v0, v1, alt0, _altitude(), 100 * floor_frames / maxi(frames, 1), left_ship])
	player.position = Vector3(0, 0.32, -2.5)  # back to the seat (test shortcut)
	await _wait(0.3)
	_tap(KEY_F)
	await _wait(0.3)
	ship.hover_assist = true


## Debug: walk from behind into the parked ship and log what happens.
func _board_debug() -> void:
	await _wait(3.0)
	var ship: RigidBody3D = main.ship
	var player: CharacterBody3D = main.player
	var behind: Vector3 = main.to_planet(ship.to_global(Vector3(0, 0, 12))).normalized()
	player.global_position = main.planet_center + behind * (main.planet_radius + main.height_at(behind) + 0.1)
	player.look_at_point(ship.to_global(Vector3(0, 1.5, 0)))
	for c in ship.get_children():
		if c is AnimatableBody3D:
			print("RAMP global origin local-to-ship %s layer %d inside_tree %s rid_space %s" % [ship.to_local(c.global_position), c.collision_layer, c.is_inside_tree(), PhysicsServer3D.body_get_space(c.get_rid())])
	print("PLAYER layer %d mask %d" % [player.collision_layer, player.collision_mask])
	_keys([KEY_W], true)
	var t := 0.0
	var next := 0.0
	while t < 5.0:
		if t >= next:
			next += 0.25
			print("BOARD t %.2f local %s floor %s wall %s in_frame %s" % [
				t, ship.to_local(player.global_position), player.is_on_floor(), player.is_on_wall(), player.ship_frame != null])
		t += await _frame()
	_keys([KEY_W], false)


## Spike 3: leave and re-enter the landed ship on foot, then stand and walk in
## a ship that accelerates and rolls (driven by the test, nobody at the seat).
func _ship_interior_tests() -> void:
	var ship: RigidBody3D = main.ship
	var player: CharacterBody3D = main.player

	_begin("walk out of landed ship")
	_tap(KEY_F)  # stand up
	await _wait(0.5)
	player.look_at_point(ship.to_global(Vector3(0, 1.0, 12)))
	_keys([KEY_W], true)
	await _wait(4.0)
	_keys([KEY_W], false)
	await _wait(0.5)
	_end("outside %s, %.2f m above ground" % [player.ship_frame == null and player.get_parent() == main, _above_ground()])

	_begin("walk back in to the seat")
	player.look_at_point(ship.to_global(ship.SEAT_POS))
	_keys([KEY_W], true)
	await _wait(5.0)
	_keys([KEY_W], false)
	await _wait(0.3)
	var ramp_end: Vector3 = main.to_planet(ship.to_global(Vector3(0, 0, 5.8)))
	var ramp_gap: float = ramp_end.length() - main.planet_radius - main.height_at(ramp_end.normalized())
	_end("in cabin %s, at seat %s, ramp end %.2f m above ground, ship tilt %.0f deg" % [
		player.ship_frame == ship, main.near_seat(), ramp_gap,
		rad_to_deg(ship.global_basis.y.angle_to(main.to_planet(ship.global_position).normalized()))])
	if not main.near_seat():  # keep the run going: put the walker at the seat
		player.enter_ship_frame(ship)
		player.position = Vector3(0, 0.32, -2.5)
		await _wait(0.3)

	_tap(KEY_F)  # sit, climb well clear of the hills, stand up while hovering
	await _wait(0.3)
	_keys([KEY_SPACE, KEY_SHIFT], true)
	var climb_t := 0.0
	while _above_ground() < 400.0 and climb_t < 20.0:
		climb_t += await _frame()
	_keys([KEY_SPACE, KEY_SHIFT], false)
	await _wait(3.0)
	_tap(KEY_F)
	await _wait(1.0)

	_begin("stand in ship: boost + roll 6 s")
	var start_local: Vector3 = player.position
	var max_off := 0.0
	var floor_frames := 0
	var frames := 0
	var left_ship := false
	ship.test_input = Vector3(0, 0, -1)
	ship.test_boost = 5.0
	ship.test_roll = 0.3
	var t := 0.0
	while t < 6.0:
		t += await _frame()
		frames += 1
		max_off = maxf(max_off, player.position.distance_to(start_local))
		if player.is_on_floor():
			floor_frames += 1
		if player.ship_frame != ship:
			left_ship = true
	var top_speed: float = ship.linear_velocity.length()
	ship.test_input = Vector3.ZERO
	ship.test_roll = 0.0
	ship.test_boost = 1.0
	_end("ship %.0f m/s, %.0f m above ground, walker drift in cabin %.3f m, on floor %d%%, left ship %s" % [
		top_speed, _above_ground(), max_off, 100 * floor_frames / maxi(frames, 1), left_ship])
	await _wait(3.0)

	_begin("walk in moving ship 6 s")
	ship.test_input = Vector3(0, 0, -1)
	floor_frames = 0
	frames = 0
	left_ship = false
	var min_y := INF
	var max_y := -INF
	var min_agl := INF
	t = 0.0
	# Forward into the front wall for 2 s, back 1 s (stays inside: the back is open).
	_keys([KEY_W], true)
	while t < 6.0:
		if t > 2.0 and SpikeInput.held.has(KEY_W):
			_keys([KEY_W], false)
			_keys([KEY_S], true)
		elif t > 3.0 and SpikeInput.held.has(KEY_S):
			_keys([KEY_S], false)
		t += await _frame()
		frames += 1
		min_agl = minf(min_agl, _above_ground())
		if player.ship_frame == ship:
			min_y = minf(min_y, player.position.y)
			max_y = maxf(max_y, player.position.y)
		if player.is_on_floor():
			floor_frames += 1
		if player.ship_frame != ship:
			left_ship = true
	_keys([KEY_W, KEY_S], false)
	ship.test_input = Vector3.ZERO
	_end("ship %.0f m/s, lowest %.0f m above ground, walker height in cabin %.3f-%.3f m, on floor %d%%, left ship %s" % [
		ship.linear_velocity.length(), min_agl, min_y, max_y, 100 * floor_frames / maxi(frames, 1), left_ship])


## Precision check: nothing should move. Reports the largest frame-to-frame
## step and the total drift of the body and of its camera.
func _stand_still(name: String, body: Node3D) -> void:
	await _wait(1.0)  # let it settle
	_begin(name)
	var cam := get_viewport().get_camera_3d()
	var start: Vector3 = main.to_planet(body.global_position)
	var last := start
	var last_cam: Vector3 = main.to_planet(cam.global_position)
	var max_step := 0.0
	var max_cam_step := 0.0
	var cam_start: Vector3 = main.to_planet(cam.global_position)
	var cam_moves := 0
	var t := 0.0
	while t < 5.0:
		t += await _frame()
		max_step = maxf(max_step, main.to_planet(body.global_position).distance_to(last))
		var cam_step: float = main.to_planet(cam.global_position).distance_to(last_cam)
		max_cam_step = maxf(max_cam_step, cam_step)
		if cam_step > 0.0001:
			cam_moves += 1
		last = main.to_planet(body.global_position)
		last_cam = main.to_planet(cam.global_position)
	_end("max step %.4f mm, drift %.4f mm, camera max step %.4f mm, camera drift %.1f mm, frames camera moved >0.1 mm %d, dist centre %.1f m, dist origin %.1f m" % [
		max_step * 1000.0, start.distance_to(main.to_planet(body.global_position)) * 1000.0,
		max_cam_step * 1000.0, cam_start.distance_to(main.to_planet(cam.global_position)) * 1000.0, cam_moves,
		main.to_planet(body.global_position).length(), body.global_position.length()])


func _shot(tag: String) -> void:
	overlay.save_screenshot("s5-" + tag)
	_skip_frames = 2


func _count_over(sorted: PackedFloat32Array, limit: float) -> int:
	var c := 0
	for x in sorted:
		if x > limit:
			c += 1
	return c


func _altitude() -> float:
	return main.to_planet(main.active.global_position).length() - main.planet_radius


func _above_ground() -> float:
	var p: Vector3 = main.to_planet(main.active.global_position)
	return p.length() - main.planet_radius - main.height_at(p.normalized())


func _keys(keys: Array, pressed: bool) -> void:
	for k in keys:
		if pressed:
			SpikeInput.held[k] = true
		else:
			SpikeInput.held.erase(k)


func _send(k: Key, pressed: bool) -> void:
	var ev := InputEventKey.new()
	ev.physical_keycode = k
	ev.keycode = k
	ev.pressed = pressed
	Input.parse_input_event(ev)


func _tap(key: Key) -> void:
	_send(key, true)
	await _frame()
	_send(key, false)


func _wait(sec: float) -> void:
	await get_tree().create_timer(sec).timeout


func _frame() -> float:
	await get_tree().process_frame
	return get_process_delta_time()
