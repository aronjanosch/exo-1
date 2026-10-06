extends Node3D
## Local ENet relay; authority stays with each client's ship unless --reference.
const Fixture := preload("res://spikes/network/fixture.gd")
const Snapshot := preload("res://spikes/network/snapshot.gd")
const Buffer := preload("res://spikes/network/buffer.gd")
const Link := preload("res://spikes/network/link.gd")
const Ship := preload("res://spikes/planet/ship.gd")
const SpikeInput := preload("res://spikes/planet/spike_input.gd")

var options := {}
var host := false
var reference := false
var bot := false
var slot := 1
var port := 17440
var rate := 30.0
var buffer_seconds := 0.15
var duration := 0.0
var shift_threshold := 1000.0
var peer := ENetMultiplayerPeer.new()
var fixture: Node3D
var link := Link.new()
var histories := {}
var proxies := {}
var avatars := {}
var owners := {}
var authoritative := {}
var commands: Array[Dictionary] = []
var input_ticks := {}
var input_sent := {}
var response_ms: Array[float] = []
var clock_offset := 0.0
var clock_rtt := INF
var clock_ready := false
var started := 0.0
var send_accum := 0.0
var ping_accum := 0.0
var seq := 0
var input_tick := 0
var send_bytes := 0
var received_bytes := 0
var invalid := 0
var hold_frames := 0
var displayed_frames := 0
var frame_cpu: Array[float] = []
var physics_script_cpu: Array[float] = []
var engine_physics_cpu: Array[float] = []
var rpc_cpu_us := 0
var label: Label
var spectator: Camera3D
var observe := false
var walking := false
var frame_slot := 0
var ready_to_send := false
var maximum_remotes := 0
var phase := "manual"
var last_seen := {}
var control_server: TCPServer
var control_clients: Array[Dictionary] = []
var agent_input := Vector3.ZERO
var agent_turn := Vector2.ZERO
var agent_roll := 0.0
var agent_piloting := false
var screenshot_taken := false

func _ready() -> void:
	# A headless viewer otherwise spins ~1000 display frames/s and needlessly
	# interpolates seven ships each time; eight instances starve ENet polling.
	Engine.max_fps = 60
	for arg in OS.get_cmdline_user_args():
		var pieces := arg.trim_prefix("--").split("=", true, 1)
		options[pieces[0]] = pieces[1] if pieces.size() > 1 else "true"
	host = options.has("host")
	reference = options.has("reference")
	bot = options.has("bot")
	slot = int(options.get("slot", "1" if host else "2"))
	port = int(options.get("port", "17440"))
	rate = clampf(float(options.get("rate", "30")), 1, 60)
	buffer_seconds = clampf(float(options.get("buffer", "150")), 0, 1000) / 1000.0
	duration = float(options.get("seconds", "0"))
	shift_threshold = float(options.get("shift", "1000"))
	link.configure(float(options.get("delay", "0")), float(options.get("jitter", "0")),
		float(options.get("loss", "0")), 4000 + slot)
	if slot < 1 or slot > 8:
		push_error("slot must be 1..8")
		get_tree().quit(2)
		return
	fixture = Fixture.new()
	add_child(fixture)
	fixture.setup(int(options.get("planet", "0")), slot)
	fixture.ship.set_meta("owner", slot)
	fixture.ship.camera.far = 250000.0
	if reference and not host:
		fixture.ship.freeze = true
		fixture.ship.piloted = false
	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-50, -30, 0)
	add_child(light)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color(0.035, 0.06, 0.1)
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 0.5
	add_child(environment)
	var canvas := CanvasLayer.new()
	add_child(canvas)
	label = Label.new()
	label.position = Vector2(12, 12)
	canvas.add_child(label)
	spectator = Camera3D.new()
	spectator.far = 250000
	add_child(spectator)
	observe = options.has("observe")
	if observe:
		spectator.make_current()
	peer.peer_disconnected.connect(_peer_left)
	peer.peer_connected.connect(_configure_connection)
	var error: Error
	if host:
		peer.set_bind_ip(str(options.get("bind", "0.0.0.0")))
		error = peer.create_server(port, 7, 2)
		owners[1] = slot
		clock_ready = true
		ready_to_send = true
	else:
		error = peer.create_client(str(options.get("connect", "127.0.0.1")), port, 2)
		multiplayer.connected_to_server.connect(_connected)
		multiplayer.connection_failed.connect(_connection_failed)
		multiplayer.server_disconnected.connect(_connection_failed)
	if error != OK:
		push_error("ENet setup: %s" % error_string(error))
		get_tree().quit(2)
		return
	# This spike already relays snapshots explicitly; peer-to-peer relay adds
	# unnecessary peer announcements and races simultaneous ENet departures.
	multiplayer.server_relay = false
	multiplayer.multiplayer_peer = peer
	if options.has("agent-port"):
		control_server = TCPServer.new()
		var control_error := control_server.listen(int(options["agent-port"]), "127.0.0.1")
		if control_error != OK:
			push_error("agent port unavailable")
			get_tree().quit(2)
			return
	started = now()
	print("SPIKE4 START slot=%d host=%s reference=%s port=%d" % [slot, host, reference, port])

func now() -> float:
	return Time.get_ticks_usec() / 1000000.0

func server_now() -> float:
	return now() + clock_offset

func _connected() -> void:
	hello.rpc_id(1, slot, fixture.planet_id, reference)
	clock_ping.rpc_id(1, now())

func _configure_connection(id: int) -> void:
	# Fault injection owns packet loss in this LAN spike. ENet's adaptive
	# unreliable throttle otherwise adds unrelated drops when eight local
	# physics processes contend for CPU. No WAN congestion claim is made.
	peer.get_peer(id).throttle_configure(1000, 32, 0)

@rpc("any_peer", "call_remote", "reliable", 0)
func hello(requested: int, planet_id: int, wants_reference: bool) -> void:
	if not host:
		return
	var sender := multiplayer.get_remote_sender_id()
	if requested < 2 or requested > 8 or requested in owners.values() or planet_id < 0 or planet_id > 1 or wants_reference != reference:
		peer.disconnect_peer(sender)
		return
	owners[sender] = requested
	if reference:
		var body := Ship.new()
		body.planet = fixture.planet
		body.position = fixture.world_position(planet_id, Vector3((requested - 1) * 20, 5000.05, 0))
		fixture.add_child(body)
		body.piloted = false
		body.camera.current = false
		body.set_meta("owner", requested)
		authoritative[requested] = {"body": body, "planet": planet_id}
	accepted.rpc_id(sender)
	print("SPIKE4 JOIN slot=%d" % requested)

@rpc("authority", "call_remote", "reliable", 0)
func accepted() -> void:
	ready_to_send = true

@rpc("any_peer", "call_remote", "unreliable", 0)
func clock_ping(sent: float) -> void:
	if host:
		clock_pong.rpc_id(multiplayer.get_remote_sender_id(), sent, now())

@rpc("authority", "call_remote", "unreliable", 0)
func clock_pong(sent: float, server_time: float) -> void:
	var rtt := now() - sent
	if rtt < clock_rtt:
		clock_rtt = rtt
		clock_offset = server_time - (sent + now()) * 0.5
	clock_ready = true

@rpc("any_peer", "call_remote", "unreliable", 1)
func snapshot_packet(data: PackedByteArray) -> void:
	var begin := Time.get_ticks_usec()
	var sender := multiplayer.get_remote_sender_id()
	var s := Snapshot.decode(data)
	if s.is_empty() or (host and (not owners.has(sender) or s.owner != owners[sender] or reference)) or (not host and sender != 1):
		invalid += 1
		return
	received_bytes += data.size()
	# Relay immediately; artificial end-to-end impairment belongs to each receiver.
	if host:
		for id in _connected_peers():
			if id != sender:
				snapshot_packet.rpc_id(id, data)
				send_bytes += data.size()
	link.enqueue(server_now(), data)
	rpc_cpu_us += Time.get_ticks_usec() - begin

@rpc("any_peer", "call_remote", "unreliable", 0)
func input_command(tick: int, movement: Vector3, mouse: Vector2, roll: float) -> void:
	var sender := multiplayer.get_remote_sender_id()
	if not host or not reference or not owners.has(sender) or not movement.is_finite() or not mouse.is_finite() or not is_finite(roll):
		return
	if link.rng.randf() < link.loss:
		return
	commands.append({"due": server_now() + maxf(0, link.delay + link.rng.randf_range(-link.jitter, link.jitter)),
		"slot": owners[sender], "tick": tick, "movement": movement.limit_length(),
		"mouse": mouse.limit_length(0.1), "roll": clampf(roll, -1, 1)})
	if commands.size() > 512:
		commands.pop_front()

func _physics_process(dt: float) -> void:
	var begin := Time.get_ticks_usec()
	if not is_instance_valid(fixture) or not ready_to_send:
		return
	var elapsed := now() - started
	if bot:
		phase = Fixture.drive(fixture.ship, elapsed)
	elif agent_piloting:
		fixture.ship.piloted = false
		fixture.ship.test_input = agent_input
		fixture.ship.test_roll = agent_roll
		fixture.ship._mouse = agent_turn * dt
	if reference and not host:
		input_tick += 1
		var movement: Vector3 = fixture.ship.test_input if bot or agent_piloting else Vector3(
			SpikeInput.axis(KEY_D, KEY_A), SpikeInput.axis(KEY_SPACE, KEY_CTRL), -SpikeInput.axis(KEY_W, KEY_S))
		var mouse: Vector2 = fixture.ship._mouse
		fixture.ship._mouse = Vector2.ZERO
		input_sent[input_tick] = now()
		while input_sent.size() > 256:
			input_sent.erase(input_sent.keys()[0])
		input_command.rpc_id(1, input_tick, movement, mouse, SpikeInput.axis(KEY_Q, KEY_E))
	if host and reference:
		for i in range(commands.size() - 1, -1, -1):
			if commands[i].due <= server_now():
				var c: Dictionary = commands[i]
				if authoritative.has(c.slot) and c.tick > int(input_ticks.get(c.slot, 0)):
					var body: RigidBody3D = authoritative[c.slot].body
					body.test_input = c.movement
					body._mouse += c.mouse
					body.test_roll = c.roll
					input_ticks[c.slot] = c.tick
				commands.remove_at(i)
	send_accum += dt
	if send_accum >= 1.0 / rate and clock_ready:
		send_accum = fmod(send_accum, 1.0 / rate)
		seq += 1
		if not reference or host:
			_send(Snapshot.encode(fixture.state(slot, server_now(), seq)))
		if host and reference:
			for owner in authoritative:
				var body: RigidBody3D = authoritative[owner].body
				var s := Snapshot.make(owner, server_now(), fixture.relative_position(authoritative[owner].planet, body.global_position),
					body.linear_velocity, body.global_basis.get_rotation_quaternion())
				s.seq = seq
				s.planet = authoritative[owner].planet
				s.input_tick = int(input_ticks.get(owner, 0))
				_send(Snapshot.encode(s))
	_record(physics_script_cpu, float(Time.get_ticks_usec() - begin) / 1000.0)
	_record(engine_physics_cpu, Performance.get_monitor(Performance.TIME_PHYSICS_PROCESS) * 1000.0)

func _send(data: PackedByteArray) -> void:
	if host:
		for id in _connected_peers():
			snapshot_packet.rpc_id(id, data)
			send_bytes += data.size()
	else:
		snapshot_packet.rpc_id(1, data)
		send_bytes += data.size()

func _process(dt: float) -> void:
	if not fixture:
		return
	var begin := Time.get_ticks_usec()
	_control_poll()
	var elapsed := now() - started
	if not host and peer.get_connection_status() == MultiplayerPeer.CONNECTION_CONNECTED:
		ping_accum += dt
		if ping_accum > 0.5:
			ping_accum = 0
			clock_ping.rpc_id(1, now())
	for item in link.ready(server_now()):
		var s := Snapshot.decode(item.data)
		if s.owner == slot and not reference:
			continue
		if not histories.has(s.owner):
			histories[s.owner] = Buffer.new()
			_make_proxy(s.owner)
		histories[s.owner].push(s)
		last_seen[s.owner] = server_now()
		maximum_remotes = maxi(maximum_remotes, histories.size())
	for owner in histories.keys():
		if server_now() - float(last_seen.get(owner, 0)) > 2.0:
			_remove_owner(owner)
	var target := server_now() - link.delay - buffer_seconds
	var samples := {}
	for owner in histories:
		var s: Dictionary = histories[owner].sample(target)
		if s.is_empty():
			continue
		samples[owner] = s
		var body: RigidBody3D = proxies[owner]
		body.global_transform = Transform3D(Basis(s.q), fixture.world_position(s.planet, s.p))
		body.linear_velocity = s.v
		displayed_frames += 1
		if s.mode == "hold":
			hold_frames += 1
		if owner == slot and reference:
			fixture.ship.global_transform = body.global_transform
			fixture.ship.linear_velocity = s.v
			if input_sent.has(s.input_tick):
				_record(response_ms, (now() - input_sent[s.input_tick]) * 1000)
				input_sent.erase(s.input_tick)
	for owner in samples:
		var s: Dictionary = samples[owner]
		var avatar: Node3D = avatars[owner]
		if s.frame == 1:
			var parent_body: Node3D = fixture.ship if s.frame_id == slot else proxies.get(s.frame_id)
			if parent_body:
				avatar.global_transform = parent_body.global_transform * Transform3D(Basis(s.wq), s.wp)
		else:
			avatar.global_transform = Transform3D(Basis(s.wq), fixture.world_position(s.planet, s.wp))
	# Shift between physics ticks. Buffers contain no local coordinates.
	var active: Node3D = fixture.walker if walking else fixture.ship
	if shift_threshold > 0 and active.global_position.length() > shift_threshold:
		fixture.shift(active.global_position.round())
	if walking and fixture.walker.ship_frame == null:
		for owner in proxies:
			var body: RigidBody3D = proxies[owner]
			if body.cabin_contains(fixture.walker.global_position, -0.1):
				fixture.walker.enter_ship_frame(body)
				frame_slot = owner
				break
	elif walking and fixture.walker.ship_frame and not fixture.walker.ship_frame.cabin_contains(fixture.walker.global_position, 0.3):
		fixture.walker.leave_ship_frame(fixture)
		frame_slot = 0
	if observe:
		var observed: Node3D = proxies.values()[0] if not proxies.is_empty() else fixture.ship
		spectator.global_position = observed.global_position + Vector3(18, 12, 28)
		spectator.look_at(observed.global_position + Vector3(0, 1, 0))
	if DisplayServer.get_name() != "headless" and Engine.get_process_frames() % 6 == 0:
		label.text = "Spike 4 | %s | slot %d | planet %d | %s | %s\n%d Hz + %.0f ms buffer | delay %.0f +/- %.0f ms | loss %.0f%%\nremotes %d | hold %.2f%% | shifts %d | frame %d | %.1f m/s\nWASD Space/Ctrl: ship | mouse: turn | Q/E: roll | H: assist\nF: walk/seat | B: remote cabin (test placement) | Tab: follow remote | O: origin shift\nEsc: release mouse | click: capture | scripted runs never capture" % [
			"host" if host else "client", slot, fixture.planet_id, "HOST AUTH reference" if reference else "CLIENT AUTH",
			phase, int(rate), buffer_seconds * 1000, link.delay * 1000, link.jitter * 1000, link.loss * 100,
			proxies.size(), 100.0 * hold_frames / maxf(1, displayed_frames), fixture.shifts, frame_slot,
			fixture.ship.linear_velocity.length()]
	_record(frame_cpu, float(Time.get_ticks_usec() - begin) / 1000.0)
	if options.has("shot") and elapsed > 4.0 and not screenshot_taken:
		screenshot_taken = true
		_capture.call_deferred()
	if duration > 0 and elapsed >= duration:
		_finish()
	elif not host and elapsed > 8 and not ready_to_send:
		_connection_failed()

func _make_proxy(owner: int) -> void:
	var body := Ship.new()
	body.planet = fixture.planet
	body.freeze = true
	body.freeze_mode = RigidBody3D.FREEZE_MODE_KINEMATIC
	body.position = Vector3(0, -10000, 0)
	fixture.add_child(body)
	body.piloted = false
	body.camera.current = false
	body.set_meta("owner", owner)
	# Own host-authoritative copy must not collide with its displayed duplicate.
	if owner == slot:
		body.collision_layer = 0
	proxies[owner] = body
	var avatar := MeshInstance3D.new()
	var capsule := CapsuleMesh.new()
	capsule.radius = 0.35
	capsule.height = 1.8
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.25, 0.95, 0.45)
	capsule.material = material
	avatar.mesh = capsule
	fixture.add_child(avatar)
	avatars[owner] = avatar

func _unhandled_input(event: InputEvent) -> void:
	if bot:
		return
	if event is InputEventMouseButton and event.pressed:
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	if event is InputEventMouseMotion and reference and not host and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		fixture.ship._mouse += event.relative * fixture.ship.mouse_sensitivity
	if event is InputEventKey and event.pressed and not event.echo:
		match event.physical_keycode:
			KEY_ESCAPE:
				Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
			KEY_TAB:
				observe = not observe
				spectator.current = observe
				if not observe:
					(fixture.walker.get_camera() if walking else fixture.ship.camera).make_current()
			KEY_O:
				fixture.shift(Vector3(10000, 10000, -10000))
			KEY_B:
				if not proxies.is_empty() and not reference:
					_enter_walking(proxies.values()[0])
			KEY_F:
				if not reference:
					if walking:
						_seat()
						fixture.ship.piloted = true
						fixture.ship.camera.make_current()
						walking = false
					else:
						_enter_walking(fixture.ship)

func _enter_walking(body: RigidBody3D) -> void:
	if fixture.walker.ship_frame:
		fixture.walker.leave_ship_frame(fixture)
	fixture.walker.global_position = body.to_global(Vector3(0, 0.31, 1.0))
	fixture.walker.velocity = Vector3.ZERO
	fixture.walker.enter_ship_frame(body)
	fixture.walker.velocity = Vector3.ZERO
	fixture.walker.set_physics_process(true)
	fixture.ship.piloted = false
	fixture.ship.test_input = Vector3.ZERO
	fixture.walker.get_camera().make_current()
	walking = true
	frame_slot = int(body.get_meta("owner", slot))

func _seat() -> void:
	if fixture.walker.ship_frame:
		fixture.walker.leave_ship_frame(fixture)
	fixture.walker.global_position = fixture.ship.to_global(Ship.SEAT_POS)
	fixture.walker.enter_ship_frame(fixture.ship)
	fixture.walker.position = Ship.SEAT_POS
	fixture.walker.velocity = Vector3.ZERO
	fixture.walker.set_physics_process(false)
	SpikeInput.held.clear()
	frame_slot = slot

func _peer_left(id: int) -> void:
	if owners.has(id):
		var owner: int = owners[id]
		owners.erase(id)
		_remove_owner(owner)
		# No RPC from the disconnect callback: simultaneous departures can
		# invalidate ENet's queued reliable packets before their flush.
		# Other viewers expire the departed owner's snapshots after two seconds.

func _connected_peers() -> Array[int]:
	var connected: Array[int] = []
	for id in multiplayer.get_peers():
		var connection := peer.get_peer(id)
		if connection and connection.is_active() and connection.get_state() == ENetPacketPeer.STATE_CONNECTED:
			connected.append(id)
	return connected

@rpc("authority", "call_remote", "reliable", 0)
func peer_left(owner: int) -> void:
	_remove_owner(owner)

func _remove_owner(owner: int) -> void:
	if fixture.walker.ship_frame == proxies.get(owner):
		fixture.walker.leave_ship_frame(fixture)
		frame_slot = 0
	for mapping in [proxies, avatars]:
		if mapping.has(owner):
			mapping[owner].queue_free()
			mapping.erase(owner)
	histories.erase(owner)
	last_seen.erase(owner)
	if authoritative.has(owner):
		authoritative[owner].body.queue_free()
		authoritative.erase(owner)

func _connection_failed() -> void:
	push_error("SPIKE4 connection failed/disconnected")
	get_tree().quit(2)

func _finish() -> void:
	frame_cpu.sort()
	physics_script_cpu.sort()
	engine_physics_cpu.sort()
	response_ms.sort()
	var result := {"slot": slot, "host": host, "reference": reference,
		"remote_count": histories.size(), "maximum_remotes": maximum_remotes, "seconds": now() - started,
		"rate": rate, "buffer_ms": buffer_seconds * 1000,
		"delay_ms": link.delay * 1000, "loss_percent": link.loss * 100,
		"send_payload_bytes": send_bytes, "receive_payload_bytes": received_bytes,
		"enet_sent_bytes": peer.host.pop_statistic(ENetConnection.HOST_TOTAL_SENT_DATA),
		"enet_received_bytes": peer.host.pop_statistic(ENetConnection.HOST_TOTAL_RECEIVED_DATA),
		"snapshot_bytes": Snapshot.SIZE, "clock_rtt_ms": clock_rtt * 1000 if clock_ready and not host else 0,
		"cpu_process_mean_ms": _mean(frame_cpu), "cpu_process_p95_ms": _percentile(frame_cpu, 0.95),
		"cpu_physics_script_mean_ms": _mean(physics_script_cpu),
		"cpu_engine_physics_mean_ms": _mean(engine_physics_cpu), "cpu_rpc_total_ms": rpc_cpu_us / 1000.0,
		"hold_percent": 100.0 * hold_frames / maxf(1, displayed_frames),
		"input_echo_median_ms": _percentile(response_ms, 0.5), "input_echo_samples": response_ms.size(), "invalid": invalid,
		"shifts": fixture.shifts, "mouse_captured": Input.mouse_mode == Input.MOUSE_MODE_CAPTURED}
	var name := "user://spike4-live-%d-%d.json" % [port, slot]
	var file := FileAccess.open(name, FileAccess.WRITE)
	file.store_string(JSON.stringify(result, "\t"))
	print("SPIKE4 RESULT ", JSON.stringify(result))
	peer.close()
	get_tree().quit(0 if (host or histories.size() > 0) and invalid == 0 else 1)

func get_state() -> Dictionary:
	var remotes := []
	for owner in histories:
		var sample: Dictionary = histories[owner].sample(server_now() - link.delay - buffer_seconds)
		remotes.append({"owner": owner, "planet": sample.planet, "position": _vector(sample.p),
			"sequence": sample.seq,
			"world_position": _vector(fixture.world_position(sample.planet, sample.p)),
			"frame": sample.frame, "frame_id": sample.frame_id, "mode": sample.mode})
	return {"slot": slot, "ready": ready_to_send, "physics_tick": Engine.get_physics_frames(),
		"authority": "host-reference" if reference else "client", "planet": fixture.planet_id,
		"ship_position": _vector(fixture.relative_position(fixture.planet_id, fixture.ship.global_position)),
		"ship_velocity": _vector(fixture.ship.linear_velocity), "walking": walking,
		"frame_id": frame_slot, "walker_local": _vector(fixture.walker.position),
		"walker_on_floor": fixture.walker.is_on_floor(), "shifts": fixture.shifts,
		"origin": [fixture.origin_x, fixture.origin_y, fixture.origin_z],
		"remotes": remotes, "invalid": invalid}

func do_action(action: String, args: Dictionary) -> Dictionary:
	if not _valid_action_args(action, args):
		return {"ok": false, "error": "invalid action arguments"}
	match action:
		"pilot":
			bot = false
			agent_piloting = true
			agent_input = _argument_vector(args.get("movement", [0, 0, 0])).limit_length()
			agent_turn = Vector2(clampf(float(args.get("yaw", 0)), -1, 1), clampf(float(args.get("pitch", 0)), -1, 1))
			agent_roll = clampf(float(args.get("roll", 0)), -1, 1)
			if walking:
				_seat()
				fixture.ship.camera.make_current()
				walking = false
		"board_remote", "walk_own":
			if reference:
				return {"ok": false, "error": "cabin is outside the one-ship reference"}
			var body: RigidBody3D = fixture.ship
			if action == "board_remote":
				var owner := int(args.get("owner", 1 if slot != 1 else 2))
				if not proxies.has(owner):
					return {"ok": false, "error": "remote ship missing"}
				body = proxies[owner]
			bot = false
			agent_piloting = false
			_enter_walking(body)
		"walk":
			SpikeInput.held.clear()
			var movement := _argument_vector(args.get("movement", [0, 0, 0]))
			for key in [KEY_D, KEY_A, KEY_W, KEY_S]:
				if (key == KEY_D and movement.x > 0) or (key == KEY_A and movement.x < 0) or (key == KEY_W and movement.z < 0) or (key == KEY_S and movement.z > 0):
					SpikeInput.held[key] = true
		"shift":
			fixture.shift(_argument_vector(args.get("offset", [10000, 10000, -10000])).round())
		"observe":
			observe = true
			spectator.make_current()
		"place_ship":
			if reference:
				return {"ok": false, "error": "place own authority only"}
			bot = false
			fixture.ship.piloted = false
			fixture.ship.test_input = Vector3.ZERO
			fixture.ship.global_position = fixture.world_position(fixture.planet_id,
				_argument_vector(args.get("position", [0, 5001, 0])))
			fixture.ship.linear_velocity = _argument_vector(args.get("velocity", [0, 0, 0]))
		_:
			return {"ok": false, "error": "unknown action"}
	return {"ok": true, "state": get_state()}

func _valid_action_args(action: String, args: Dictionary) -> bool:
	var schemas := {"pilot": {"movement": "vector", "yaw": "number", "pitch": "number", "roll": "number"},
		"board_remote": {"owner": "owner"}, "walk_own": {}, "walk": {"movement": "vector"},
		"shift": {"offset": "vector"}, "observe": {}, "place_ship": {"position": "vector", "velocity": "vector"}}
	if not schemas.has(action):
		return false
	for key in args:
		if not schemas[action].has(key):
			return false
		var value: Variant = args[key]
		if schemas[action][key] == "vector":
			if not value is Array or value.size() != 3:
				return false
			for component in value:
				if not (component is int or component is float) or not is_finite(float(component)):
					return false
		else:
			if not (value is int or value is float) or not is_finite(float(value)):
				return false
			if schemas[action][key] == "owner" and (float(value) < 1 or float(value) > 8 or float(value) != floorf(float(value))):
				return false
	return true

func _capture() -> void:
	await RenderingServer.frame_post_draw
	var path := "user://spike4-shot-%d.png" % slot
	get_viewport().get_texture().get_image().save_png(path)
	print("SPIKE4 SCREENSHOT ", ProjectSettings.globalize_path(path))

func _argument_vector(value: Variant) -> Vector3:
	if not value is Array or value.size() != 3:
		return Vector3.ZERO
	var v := Vector3(float(value[0]), float(value[1]), float(value[2]))
	return v.limit_length(1000000) if v.is_finite() else Vector3.ZERO

func _vector(v: Vector3) -> Array:
	return [v.x, v.y, v.z]

func _control_poll() -> void:
	if not control_server:
		return
	if control_server.is_connection_available() and control_clients.size() < 8:
		control_clients.append({"peer": control_server.take_connection(), "text": "", "answered": false})
	for i in range(control_clients.size() - 1, -1, -1):
		var client: Dictionary = control_clients[i]
		var socket: StreamPeerTCP = client.peer
		socket.poll()
		if socket.get_status() != StreamPeerTCP.STATUS_CONNECTED:
			control_clients.remove_at(i)
			continue
		var available := socket.get_available_bytes()
		if available == 0:
			continue
		if available > 4096 or client.text.length() + available > 4096 or client.answered:
			socket.disconnect_from_host()
			control_clients.remove_at(i)
			continue
		var data := socket.get_data(available)
		client.text += data[1].get_string_from_utf8()
		if not client.text.contains("\n"):
			continue
		var request: Variant = JSON.parse_string(client.text.get_slice("\n", 0))
		var response := {"ok": false, "error": "invalid request"}
		if request is Dictionary:
			if request.get("op") == "get_state":
				response = {"ok": true, "state": get_state()}
			elif request.get("op") == "do_action" and request.get("args", {}) is Dictionary:
				response = do_action(str(request.get("action", "")), request.get("args", {}))
		socket.put_data((JSON.stringify(response) + "\n").to_utf8_buffer())
		client.answered = true

func _mean(values: Array[float]) -> float:
	var total := 0.0
	for value in values:
		total += value
	return total / maxf(1, values.size())

func _record(values: Array[float], value: float) -> void:
	values.append(value)
	if values.size() > 20000:
		values.pop_front()

func _percentile(values: Array[float], fraction: float) -> float:
	return values[mini(values.size() - 1, int(values.size() * fraction))] if not values.is_empty() else 0.0
