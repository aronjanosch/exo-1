extends SceneTree
## Accelerated real-Jolt capture, deterministic impaired replay, contact worlds.
const Fixture := preload("res://spikes/network/fixture.gd")
const Snapshot := preload("res://spikes/network/snapshot.gd")
const Buffer := preload("res://spikes/network/buffer.gd")
const Link := preload("res://spikes/network/link.gd")
const Ship := preload("res://spikes/planet/ship.gd")
const SpikeInput := preload("res://spikes/planet/spike_input.gd")
const Walker := preload("res://spikes/network/walker.gd")
const DT := 1.0 / 60.0
var failures := 0
var trajectory: Array[Dictionary] = []
var fixture: Node3D
var output := {"engine": Engine.get_version_info().string, "cases": [], "checks": [], "contacts": {}, "capture": {}}

func _initialize() -> void:
	Engine.physics_ticks_per_second = 60
	_run.call_deferred()

func check(condition: bool, note: String) -> void:
	output.checks.append({"pass": condition, "note": note})
	print("%s %s" % ["PASS" if condition else "FAIL", note])
	if not condition:
		failures += 1

func _run() -> void:
	_unit_checks()
	fixture = Fixture.new()
	root.add_child(fixture)
	fixture.setup(0, 1, false)
	fixture.ship.camera.current = false
	for i in 5:
		await physics_frame
	var initial_height: float = fixture.relative_position(0, fixture.ship.global_position).y
	var max_height := initial_height
	var shift_jump := 0.0
	var speed_before := 0.0
	var input_response_ticks := 0
	var capture_trace := []
	for i in 1501:
		var t := float(i) * DT
		Fixture.drive(fixture.ship, t)
		await physics_frame
		var s: Dictionary = fixture.state(1, t, i)
		trajectory.append(s)
		if i < 650:
			capture_trace.append({"i": i, "tick": Engine.get_physics_frames(), "p": [s.p.x, s.p.y, s.p.z], "v": [s.v.x, s.v.y, s.v.z], "shifts": fixture.shifts, "server_p": str(PhysicsServer3D.body_get_state(fixture.ship.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM).origin)})
		max_height = maxf(max_height, s.p.y)
		if input_response_ticks == 0 and s.v.length() > 0.01:
			input_response_ticks = i + 1
		# Shift only in process, never at the physics-frame signal boundary.
		await process_frame
		if fixture.ship.global_position.length() > 10.0:
			var server_pose: Transform3D = PhysicsServer3D.body_get_state(fixture.ship.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM)
			var p0: Vector3 = fixture.relative_position(0, server_pose.origin)
			speed_before = fixture.ship.linear_velocity.length()
			if i < 650:
				capture_trace[i]["before_shift_node"] = str(fixture.ship.global_position)
				capture_trace[i]["before_shift_server"] = str(PhysicsServer3D.body_get_state(fixture.ship.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM).origin)
			fixture.shift(fixture.ship.global_position.round())
			shift_jump = maxf(shift_jump, p0.distance_to(fixture.relative_position(0, fixture.ship.global_position)))
			check(absf(speed_before - fixture.ship.linear_velocity.length()) < 0.00001, "shift preserves velocity") if fixture.shifts == 1 else null
	output.capture = {"frames": trajectory.size(), "shifts": fixture.shifts,
		"shift_jump_mm": shift_jump * 1000, "start_height": initial_height,
		"maximum_height": max_height, "final_height": trajectory[-1].p.y,
		"final_speed": trajectory[-1].v.length(), "input_response_ticks": input_response_ticks}
	var trace_file := FileAccess.open("user://spike4-capture-trace.json", FileAccess.WRITE)
	trace_file.store_string(JSON.stringify(capture_trace))
	check(input_response_ticks <= 2, "own input responds within two physics ticks (%d)" % input_response_ticks)
	check(max_height - 5000 > 45, "real ship takeoff >45 m")
	check(trajectory[-1].p.y - 5000 < 1.0 and trajectory[-1].v.length() < 1.0, "real ship lands on fixture")
	check(fixture.shifts > 10 and shift_jump < 0.002, "capture survives >10 origin shifts; jump %.3f mm" % (shift_jump * 1000))
	if "--capture-only" in OS.get_cmdline_user_args():
		fixture.free()
		quit()
		return
	# Full Cartesian matrix: 20/30 Hz, 100/150 ms extra buffer, 0/50/150 ms
	# one-way delay, 0/1/5/10 percent loss; 2 and 8 total simulated players.
	for players in [2, 8]:
		for rate in [20, 30]:
			for buffering in [100, 150]:
				for delay in [0, 50, 150]:
					for loss in [0, 1, 5, 10]:
						output.cases.append(_replay(players, rate, buffering, delay, loss))
		print("MATRIX completed %d players" % players)
	var holds := 0.0
	var error_p95 := 0.0
	for case in output.cases:
		if case.rate == 30 and case.buffer_ms == 150:
			for metrics in case.phases.values():
				holds = maxf(holds, metrics.hold_percent)
				error_p95 = maxf(error_p95, metrics.error_p95_mm)
	check(holds == 0 and error_p95 < 10, "30 Hz/150 ms matrix: no underruns; p95 error %.3f mm <10 mm fixture bound" % error_p95)
	await _passenger_check()
	await _contacts()
	fixture.free()
	output.failures = failures
	var file := FileAccess.open("user://spike4-matrix.json", FileAccess.WRITE)
	file.store_string(JSON.stringify(output, "\t"))
	print("SPIKE4 MATRIX %d cases, %d failures; %s" % [output.cases.size(), failures,
		ProjectSettings.globalize_path("user://spike4-matrix.json")])
	quit(0 if failures == 0 else 1)

func _unit_checks() -> void:
	var a := Snapshot.make(1, 1.0, Vector3(1, 5000, 3), Vector3(10, 0, 0), Quaternion.IDENTITY)
	var wire := Snapshot.encode(a)
	var b := Snapshot.decode(wire)
	check(wire.size() == 144 and b.p == a.p and b.v == a.v, "fixed 144-byte data roundtrip")
	check(Snapshot.decode(PackedByteArray([1, 2])).is_empty(), "reject truncated payload")
	wire.encode_double(24, NAN)
	check(Snapshot.decode(wire).is_empty(), "reject nonfinite timestamp")
	var invalid_frame := a.duplicate()
	invalid_frame.frame = 1
	check(Snapshot.decode(Snapshot.encode(invalid_frame)).is_empty(), "reject ship-frame snapshot without parent id")
	b = a.duplicate()
	b.seq = 2
	b.t = 1.1
	b.p.x += 1
	var buffer := Buffer.new()
	buffer.push(b)
	buffer.push(a)
	buffer.push(a)
	var s: Dictionary = buffer.sample(1.05)
	check(absf(s.p.x - 1.5) < 0.00001 and buffer.reordered == 1 and buffer.duplicates == 1,
		"Hermite, reordered packet and duplicate handling")
	check(buffer.sample(2.0).mode == "hold", "underrun holds, does not extrapolate collision")
	var transition := b.duplicate()
	transition.seq = 3
	transition.t = 1.2
	transition.frame = 1
	transition.frame_id = 2
	transition.wp = Vector3(0, 0.3, 1)
	buffer.push(transition)
	check(buffer.sample(1.15).frame == 0 and buffer.sample(1.2).frame == 1,
		"frame transition never mixes ship-local and planet-local positions")
	for i in 200:
		var item := a.duplicate()
		item.seq = i + 10
		item.t = i + 10.0
		buffer.push(item)
	check(buffer.history.size() == 128, "snapshot history bounded")
	var reconnect := Buffer.new()
	var old := a.duplicate()
	old.seq = 100
	old.t = 10.0
	reconnect.push(old)
	var fresh := a.duplicate()
	fresh.seq = 1
	fresh.t = 12.0
	reconnect.push(fresh)
	reconnect.push(old)
	check(reconnect.history.size() == 1 and reconnect.sample(12).seq == 1,
		"rejoining owner resets history; queued old life cannot return")
	var coordinate_fixture := Fixture.new()
	coordinate_fixture.origin_x = 200000
	coordinate_fixture.origin_y = 5000
	var near := coordinate_fixture.world_position(1, Vector3(0.25, 5000.5, 0.125))
	var far := coordinate_fixture.world_position(0, Vector3(0.25, 5000.5, 0.125))
	check(near == Vector3(0.25, 0.5, 0.125) and far.x == -199999.75, "different planets reconstruct in independent origins")
	coordinate_fixture.origin_x += 10000
	coordinate_fixture.origin_y += 10000
	coordinate_fixture.origin_z -= 10000
	check((coordinate_fixture.world_position(1, Vector3(0.25, 5000.5, 0.125)) + Vector3(10000, 10000, -10000)).distance_to(near) < 0.001,
		"shift leaves shared snapshots unchanged")
	coordinate_fixture.free()

func _truth(t: float) -> Dictionary:
	var index := clampi(int(floor(t / DT)), 0, trajectory.size() - 2)
	return Buffer.between(trajectory[index], trajectory[index + 1], t)

func _replay(players: int, rate: int, buffering: int, delay: int, loss: int) -> Dictionary:
	var remote_count := players - 1
	var links: Array = []
	var buffers: Array = []
	var last_errors: Array = []
	var last_positions: Array = []
	var last_velocities: Array = []
	var last_accelerations: Array = []
	for owner in remote_count:
		var link := Link.new()
		link.configure(delay, 20 if delay > 0 else 0, loss, 4400 + owner)
		links.append(link)
		buffers.append(Buffer.new())
		last_errors.append(Vector3.ZERO)
		last_positions.append(Vector3.ZERO)
		last_velocities.append(Vector3.ZERO)
		last_accelerations.append(Vector3.ZERO)
	var phases := {}
	for name in ["takeoff", "cruise", "turn", "landing", "idle"]:
		phases[name] = {"errors": [], "step_errors": [], "jerk": [], "rotation": [], "holds": 0, "samples": 0}
	var cpu: Array[float] = []
	var bytes := 0
	var tx_usec := 0
	var rx_usec := 0
	var step := 60 / rate
	for i in trajectory.size():
		var now := float(i) * DT
		var target := now - float(delay + buffering) / 1000.0
		var begin := Time.get_ticks_usec()
		if i % step == 0:
			var tx_begin := Time.get_ticks_usec()
			for owner in remote_count:
				var s := trajectory[i].duplicate()
				s.owner = owner + 2
				s.seq = i
				s.p.x += owner * 20
				var wire := Snapshot.encode(s)
				links[owner].enqueue(now, wire)
				bytes += wire.size()
			tx_usec += Time.get_ticks_usec() - tx_begin
		var rx_begin := Time.get_ticks_usec()
		for owner in remote_count:
			for item in links[owner].ready(now):
				buffers[owner].push(Snapshot.decode(item.data))
			var shown: Dictionary = buffers[owner].sample(target)
			if target < 0.5 or shown.is_empty():
				continue
			var truth := _truth(target)
			truth.p.x += owner * 20
			var error: Vector3 = shown.p - truth.p
			var name := "takeoff" if target < 4 else ("cruise" if target < 10 else ("turn" if target < 16 else ("landing" if target < 24 else "idle")))
			var metrics: Dictionary = phases[name]
			metrics.errors.append(error.length() * 1000)
			metrics.rotation.append(rad_to_deg(shown.q.angle_to(truth.q)))
			metrics.samples += 1
			if shown.mode == "hold":
				metrics.holds += 1
			if target > 0.5 + 3 * DT:
				metrics.step_errors.append((error - last_errors[owner]).length() * 1000)
				var velocity: Vector3 = (shown.p - last_positions[owner]) / DT
				var acceleration: Vector3 = (velocity - last_velocities[owner]) / DT
				metrics.jerk.append((acceleration - last_accelerations[owner]).length() / DT)
				last_velocities[owner] = velocity
				last_accelerations[owner] = acceleration
			last_errors[owner] = error
			last_positions[owner] = shown.p
		rx_usec += Time.get_ticks_usec() - rx_begin
		cpu.append(float(Time.get_ticks_usec() - begin) / 1000.0)
	var measured := {}
	for name in phases:
		var m: Dictionary = phases[name]
		measured[name] = {"error_rms_mm": rms(m.errors), "error_p95_mm": percentile(m.errors, 0.95),
			"error_max_mm": percentile(m.errors, 1), "step_error_p95_mm": percentile(m.step_errors, 0.95),
			"jerk_p95_m_s3": percentile(m.jerk, 0.95), "rotation_p95_deg": percentile(m.rotation, 0.95),
			"hold_percent": float(m.holds) / maxf(1, m.samples) * 100}
	var dropped := 0
	for link in links:
		dropped += link.dropped
	return {"players": players, "rate": rate, "buffer_ms": buffering, "delay_ms": delay,
		"jitter_ms": 20 if delay > 0 else 0, "loss_percent": loss,
		"playout_delay_ms": delay + buffering, "payload_tx_kB_s": bytes / 25.0 / 1000,
		"cpu_mean_ms": mean(cpu), "cpu_p95_ms": percentile(cpu, 0.95),
		"encode_queue_mean_ms_per_frame": float(tx_usec) / trajectory.size() / 1000,
		"decode_interpolate_measure_mean_ms_per_frame": float(rx_usec) / trajectory.size() / 1000,
		"dropped": dropped, "phases": measured}

func _passenger_check() -> void:
	var walker := Walker.new()
	walker.planet = fixture.planet
	walker.planet_center = fixture.planet.centre
	fixture.add_child(walker)
	walker.set_physics_process(false)
	walker.get_camera().current = false
	fixture.ship.freeze = true
	fixture.ship.global_transform = Transform3D(Basis.IDENTITY, Vector3(0, 100, 0))
	walker.global_position = fixture.ship.to_global(Vector3(0, 0.31, 1))
	walker.enter_ship_frame(fixture.ship)
	walker.velocity = Vector3.ZERO
	walker.set_physics_process(true)
	var cabin_max_error := 0.0
	var min_floor := INF
	var count := 0
	for i in 360:
		await process_frame
		# 350 m/s interpolated remote ship, with gentle yaw and origin stress.
		fixture.ship.position += Vector3(0, 0, -350 * DT)
		fixture.ship.rotate_y(0.003)
		if i % 30 == 0:
			fixture.shift(fixture.ship.global_position.round())
		await physics_frame
		cabin_max_error = maxf(cabin_max_error, Vector2(walker.position.x, walker.position.z - 1).length())
		min_floor = minf(min_floor, walker.position.y)
		if walker.is_on_floor():
			count += 1
	check(cabin_max_error < 0.05 and min_floor > 0.28 and count > 300,
		"walker on interpolated 350 m/s foreign ship: drift %.3f m, floor %d/360" % [cabin_max_error, count])
	# Use real controller to walk sideways within a foreign cabin.
	SpikeInput.held[KEY_D] = true
	for i in 12:
		await physics_frame
	SpikeInput.held.clear()
	check(walker.position.x > 0.5 and walker.position.x < 1.5, "walking inside foreign cabin uses local frame")
	var state := Snapshot.make(2, 1, Vector3(20, 5000, 0), Vector3.ZERO, Quaternion.IDENTITY)
	state.frame = 1
	state.frame_id = 1
	state.wp = walker.position
	var decoded := Snapshot.decode(Snapshot.encode(state))
	check(decoded.frame_id == 1 and decoded.wp.distance_to(walker.position) < 0.000001,
		"passenger snapshot preserves foreign ship id and local feet")
	walker.leave_ship_frame(fixture)
	check(walker.ship_frame == null and walker.get_parent() == fixture, "foreign ship exit returns to planet/world frame")
	# Ground-side walker next to a parked foreign collider, not merely a pose
	# roundtrip. No terrain safety net is present in this fixture.
	await process_frame
	fixture.ship.global_transform = Transform3D(Basis.IDENTITY,
		fixture.world_position(0, Vector3(0, 5000.05, 0)))
	walker.global_position = fixture.world_position(0, Vector3(7, 5000.05, 0))
	walker.planet_center = fixture.planet.centre
	walker.velocity = Vector3.ZERO
	SpikeInput.held[KEY_D] = true
	var ground_start := walker.global_position
	var floor_ticks := 0
	for i in 24:
		await physics_frame
		if walker.is_on_floor():
			floor_ticks += 1
	SpikeInput.held.clear()
	check(walker.global_position.distance_to(ground_start) > 1.0 and floor_ticks > 15,
		"planet-frame walker moves beside parked foreign ship; floor %d/24" % floor_ticks)
	fixture.walker = walker
	var ground_state: Dictionary = fixture.state(2, 2.0, 5)
	var ground_decoded := Snapshot.decode(Snapshot.encode(ground_state))
	check(ground_decoded.frame == 0 and ground_decoded.wp.distance_to(fixture.relative_position(0, walker.global_position)) < 0.000001,
		"outside walker snapshot uses shared planet frame")
	fixture.walker = null
	walker.free()

func _contacts() -> void:
	# Two separate physics worlds each own one dynamic ship and one stale
	# kinematic proxy. They disagree about collision time under 150 ms delay.
	var spaces: Array = []
	var bodies: Array = []
	var ghosts: Array = []
	for index in 2:
		var viewport := SubViewport.new()
		viewport.own_world_3d = true
		root.add_child(viewport)
		spaces.append(viewport)
		var planet := Fixture.Planet.new()
		planet.centre = Vector3(0, -5000, 0)
		planet.zero_gravity = true
		viewport.add_child(planet)
		for which in 2:
			var ship := Ship.new()
			ship.planet = planet
			ship.hover_assist = false
			ship.horizon_follow = false
			ship.drag_k = 0
			ship.position = Vector3(-12 if which == index else 12, 100, 0) if index == 0 else Vector3(12 if which == index else -12, 100, 0)
			# index 0 owns left, index 1 owns right.
			ship.position.x = (-12 if index == 0 else 12) if which == 0 else (12 if index == 0 else -12)
			ship.freeze = which == 1
			ship.freeze_mode = RigidBody3D.FREEZE_MODE_KINEMATIC
			viewport.add_child(ship)
			ship.camera.current = false
			if which == 0:
				ship.linear_velocity = Vector3(15 if index == 0 else -15, 0, 0)
				bodies.append(ship)
			else:
				ghosts.append(ship)
	var histories: Array = [[], []]
	var first_contact: Array = [-1, -1]
	var max_disagreement := 0.0
	for i in 180:
		await process_frame
		for index in 2:
			histories[index].append(bodies[index].global_transform)
			var other := 1 - index
			# Different one-way delays to expose conflicting contact histories.
			var lag := 9 if index == 0 else 3
			if histories[other].size() > lag:
				ghosts[index].global_transform = histories[other][histories[other].size() - 1 - lag]
		await physics_frame
		for index in 2:
			if first_contact[index] < 0 and absf(bodies[index].linear_velocity.x) < 10:
				first_contact[index] = i
			var other := 1 - index
			max_disagreement = maxf(max_disagreement, ghosts[index].global_position.distance_to(bodies[other].global_position))
	output.contacts = {"left_contact_tick": first_contact[0], "right_contact_tick": first_contact[1],
		"proxy_authority_max_disagreement_m": max_disagreement,
		"left_final_velocity": str(bodies[0].linear_velocity), "right_final_velocity": str(bodies[1].linear_velocity)}
	check(first_contact[0] >= 0 and first_contact[1] >= 0, "independent authorities both detect ship contact")
	check(first_contact[0] != first_contact[1] and max_disagreement > 0.3,
		"contact conflict exposed: ticks %s, disagreement %.2f m" % [first_contact, max_disagreement])
	for viewport in spaces:
		viewport.free()

func mean(values: Array) -> float:
	var total := 0.0
	for value in values:
		total += float(value)
	return total / maxf(1, values.size())

func rms(values: Array) -> float:
	var total := 0.0
	for value in values:
		total += float(value) * float(value)
	return sqrt(total / maxf(1, values.size()))

func percentile(values: Array, fraction: float) -> float:
	if values.is_empty():
		return 0.0
	values.sort()
	return float(values[mini(values.size() - 1, int(values.size() * fraction))])
