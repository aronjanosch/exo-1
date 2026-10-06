extends SceneTree
## Spike 6 demand test: how many terrain chunks the current LOD requests per
## second while a camera flies a great circle at constant speed.
## Usage: godot --headless --path . -s res://spikes/gen_bench/demand_test.gd -- speed=45 depth=7 height=20 secs=60 out=/tmp/x.json
## Real time: dt = actual frame delta, Engine.max_fps = 60. Height is above the
## base sphere (radius + height); the terrain amplitude is ignored.

const TERRAIN := preload("res://spikes/planet/terrain.gd")
const WARMUP_S := 10.0

var terrain: Node3D
var cam: Camera3D
var speed := 45.0
var depth := 7
var height := 20.0
var secs := 60.0
var radius := 6000.0
var out_path := ""

var angle := 0.0
var t := 0.0
var started := false
var seen := {}  # ChunkNode instance id -> true (uploaded)
var bins_all: Array[int] = []
var bins_fine: Array[int] = []
var by_depth := {}  # depth -> [warmup, steady]
var pending_max := 0
var pending_max_steady := 0
var frames := 0
var dt_max := 0.0
var last_ticks := 0


func _initialize() -> void:
	for a in OS.get_cmdline_user_args():
		var kv := a.split("=")
		match kv[0]:
			"speed": speed = float(kv[1])
			"depth": depth = int(kv[1])
			"height": height = float(kv[1])
			"secs": secs = float(kv[1])
			"out": out_path = kv[1]
	Engine.max_fps = 60
	terrain = Node3D.new()
	terrain.set_script(TERRAIN)
	terrain.radius = radius
	terrain.max_depth = depth
	root.add_child(terrain)
	cam = Camera3D.new()
	root.add_child(cam)
	cam.make_current()
	_place()
	# roots exist after _ready; mark them as seen so they count as the sync start
	_scan(true)  # roots are not ready yet here; depth 0 is skipped in _scan anyway
	last_ticks = Time.get_ticks_usec()


func _place() -> void:
	var r := radius + height
	cam.position = Vector3(cos(angle), 0, sin(angle)) * r


func _scan(initial: bool) -> void:
	var stack: Array = terrain._roots.duplicate()
	while not stack.is_empty():
		var n = stack.pop_back()
		if n.mesh_instance != null and not seen.has(n.get_instance_id()):
			seen[n.get_instance_id()] = true
			if not initial and n.depth > 0:  # the 6 roots are built synchronously in _ready, not counted
				var b := int(t)
				while bins_all.size() <= b:
					bins_all.append(0)
					bins_fine.append(0)
				bins_all[b] += 1
				if n.depth == depth:
					bins_fine[b] += 1
				var d: Array = by_depth.get(n.depth, [0, 0])
				d[0 if t < WARMUP_S else 1] += 1
				by_depth[n.depth] = d
		for c in n.children:
			stack.append(c)


func _process(_delta: float) -> bool:
	var now := Time.get_ticks_usec()
	var dt := (now - last_ticks) / 1e6
	last_ticks = now
	if not started:
		started = true  # first frame: dt includes startup, skip
		return false
	t += dt
	frames += 1
	dt_max = maxf(dt_max, dt)
	angle += speed * dt / (radius + height)
	_place()
	_scan(false)
	var p: int = terrain.stats.get("chunks_pending", 0)
	pending_max = maxi(pending_max, p)
	if t >= WARMUP_S:
		pending_max_steady = maxi(pending_max_steady, p)
	if t >= secs:
		_finish()
		return true
	return false


func _sum(a: Array[int], from: int, to: int) -> int:
	var s := 0
	for i in range(from, mini(to, a.size())):
		s += a[i]
	return s


func _stat(a: Array[int], from: int, to: int) -> Dictionary:
	var mx := 0
	var n := 0
	for i in range(from, to):
		var v: int = a[i] if i < a.size() else 0
		mx = maxi(mx, v)
		n += 1
	return {"mean_per_s": float(_sum(a, from, to)) / maxf(n, 1), "peak_1s": mx, "total": _sum(a, from, to)}


func _finish() -> void:
	var w := int(WARMUP_S)
	var total := int(secs)
	var edge_m: float = 0.0
	for r in terrain._roots:
		edge_m = r.edge_m
		break
	var res := {
		"params": {
			"radius": radius, "max_depth": depth, "split_factor": terrain.split_factor,
			"merge_factor": terrain.merge_factor, "speed_mps": speed, "height_m": height,
			"sim_seconds_real": t, "frames": frames, "dt_max_s": dt_max,
			"root_edge_m": edge_m, "finest_edge_m_approx": edge_m / pow(2.0, depth),
			"max_uploads_per_frame": terrain.max_uploads_per_frame,
		},
		"warmup_0_10s": {"all": _stat(bins_all, 0, w), "finest": _stat(bins_fine, 0, w)},
		"steady_10s_end": {"all": _stat(bins_all, w, total), "finest": _stat(bins_fine, w, total)},
		"by_depth_uploaded": by_depth,
		"pending_max_all": pending_max,
		"pending_max_steady": pending_max_steady,
		"terrain_build_count": terrain._build_count,
		"terrain_build_ms_avg": terrain._build_ms_sum / maxi(terrain._build_count, 1),
		"terrain_build_ms_max": terrain._build_ms_max,
		"node_count_end": terrain._node_count,
		"uploaded_total": seen.size(),
		"bins_all": bins_all,
		"bins_finest": bins_fine,
	}
	var js := JSON.stringify(res, "  ")
	if out_path != "":
		var f := FileAccess.open(out_path, FileAccess.WRITE)
		f.store_string(js)
		f.close()
	print(js)
