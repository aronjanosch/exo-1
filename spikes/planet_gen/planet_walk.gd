extends Node
## Spike 8, T5: a scripted walker at 1.8 m/s on the ground for 5 minutes of simulated time, from
## four starts (the spawn, the basin shore, the foot of the escarpment, the plateau approach).
## Biome id and height are logged every metre of path. No fall-through allowed.
## Run: godot --headless --path . --fixed-fps 60 -- --planet-walk
## Results: printed and written to user://planet_walk_results.txt.

const SpikeInput := preload("res://spikes/planet/spike_input.gd")
const DURATION := 300.0  # simulated seconds
const SPEED := 1.8

var main: Node3D
var _report: PackedStringArray = []


func _ready() -> void:
	_run.call_deferred()


func _offset(dir: Vector3, tangent: Vector3, metres: float) -> Vector3:
	var a: float = metres / main.planet_radius
	return (dir * cos(a) + tangent * sin(a)).normalized()


func _toward(from: Vector3, target: Vector3) -> Vector3:
	return (target - from * from.dot(target)).normalized()


func _starts() -> Array:
	var recipe: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("res://spikes/planet_gen/recipe.json"))
	var out: Array = []
	var up_dir := Vector3.UP
	out.append(["spawn, heading east", up_dir, Vector3.RIGHT])
	for stamp: Dictionary in recipe.stamps:
		var c := Vector3(stamp.center[0], stamp.center[1], stamp.center[2]).normalized()
		var t := c.cross(Vector3.UP).normalized()
		match stamp.type:
			"basin":
				var s := _offset(c, t, 900.0)
				out.append(["basin shore, heading to the centre", s, _toward(s, c)])
			"escarpment":
				var n := c.cross(t).normalized()
				var s := _offset(c, n, -300.0)
				out.append(["escarpment foot, heading up the step", s, _toward(s, c)])
			"plateau":
				var s := _offset(c, t, 1000.0)
				out.append(["plateau approach, heading to the centre", s, _toward(s, c)])
	return out


func _settle(max_frames := 900) -> void:
	var calm := 0
	for _i in max_frames:
		await get_tree().physics_frame
		var idle: bool = main.terrain._pending.is_empty() and main.terrain._done.is_empty() and main.ring._pending.is_empty()
		calm = calm + 1 if idle and main.ring.has_patch_near(main.player.global_position) else 0
		if calm >= 30:
			return


func _run() -> void:
	var gen: RefCounted = main.terrain.gen
	var sea: float = gen.sea_level()
	var player: CharacterBody3D = main.player
	player.walk_speed = SPEED
	var total_rescues := 0
	var any_fail := false
	_report.append("==== planet walk (T5): %.1f m/s, %.0f s simulated per start, record every metre ====" % [SPEED, DURATION])
	for start: Array in _starts():
		var dir: Vector3 = start[1]
		var h: float = gen.height_at(dir)
		player.fly_mode = false
		player.global_position = main.planet_center + dir * (main.planet_radius + h + 0.5)
		player.velocity = Vector3.ZERO
		player.look_at_point(player.global_position + (start[2] as Vector3) * 50.0)
		await _settle()
		SpikeInput.held[KEY_W] = true
		var rescues0: int = main.stats.rescues
		var biomes: PackedInt32Array = []
		var heights: PackedFloat32Array = []
		var unpatched := 0
		var airborne := 0
		var frames := 0
		var path := 0.0
		var next_mark := 0.0
		var last := player.global_position
		var min_ground := INF
		while frames < int(DURATION * 60.0):
			await get_tree().physics_frame
			frames += 1
			var p := player.global_position
			path += p.distance_to(last)
			last = p
			if not main.ring.has_patch_near(p):
				unpatched += 1
			if not player.is_on_floor():
				airborne += 1
			if path >= next_mark:
				var d: Vector3 = (p - main.planet_center).normalized()
				var s: Dictionary = gen.sample(d)
				biomes.append(s.biome)
				heights.append(s.height_above_sea)
				min_ground = minf(min_ground, s.height_above_sea)
				next_mark += 1.0
		SpikeInput.held.erase(KEY_W)
		var rescues: int = main.stats.rescues - rescues0
		total_rescues += rescues
		# biome statistics along the path
		var changes := 0
		var runs: Array[int] = []
		var run := 1
		for i in range(1, biomes.size()):
			if biomes[i] != biomes[i - 1]:
				changes += 1
				runs.append(run)
				run = 1
			else:
				run += 1
		var shortest := "none (no complete stretch)" if runs.is_empty() else "%d m" % runs.min()
		var hmin := INF
		var hmax := -INF
		for v in heights:
			hmin = minf(hmin, v)
			hmax = maxf(hmax, v)
		var seen := {}
		for b in biomes:
			seen[b] = seen.get(b, 0) + 1
		var line := "%s: path %.0f m (%d marks), biome changes %d, shortest complete stretch %s, biomes visited %s, height above sea %.1f..%.1f m, reached water %s (lowest ground %.1f m above sea), fall-through rescues %d, frames without patch %d, frames not on floor %d" % [
			start[0], path, biomes.size(), changes, shortest, str(seen), hmin, hmax, "yes" if hmin < 0.0 else "no", min_ground, rescues, unpatched, airborne]
		_report.append(line)
		print(line)
		if rescues > 0 or path < SPEED * DURATION * 0.5:
			any_fail = true
	_report.append("total rescues %d (sea level %.2f m above the base radius)" % [total_rescues, sea])
	for l in _report:
		print(l)
	var f := FileAccess.open("user://planet_walk_results.txt", FileAccess.WRITE)
	f.store_string("\n".join(_report) + "\n")
	f.close()
	print("results: ", ProjectSettings.globalize_path("user://planet_walk_results.txt"))
	get_tree().quit(1 if any_fail else 0)
