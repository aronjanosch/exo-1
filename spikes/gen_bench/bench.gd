extends SceneTree
## Spike 6 harness for the Godot-side variants.
## godot --headless --path . -s res://spikes/gen_bench/bench.gd -- --variant=A|A2|B [--reps=3] [--chunks=200] [--out=file.json] [--threads=1,4,8,16] [--quick]
## Measures bake, single-thread time per chunk (wall, and time inside the generator), WorkerThreadPool throughput, checksums.

const SEED := 1337

var gen: Object
var chunk_list: Array[Vector3i] = []


func _init() -> void:
	var args := {}
	for a in OS.get_cmdline_user_args():
		var kv := a.trim_prefix("--").split("=", true, 1)
		args[kv[0]] = kv[1] if kv.size() > 1 else "1"
	var variant: String = args.get("variant", "A")
	var reps: int = int(args.get("reps", "3"))
	var n_chunks: int = int(args.get("chunks", "200"))
	var out_path: String = args.get("out", "")
	var thread_counts: Array = []
	for t in String(args.get("threads", "1,4,8,16")).split(","):
		thread_counts.append(int(t))
	if args.has("quick"):
		reps = 1

	match variant:
		"A": gen = preload("res://spikes/gen_bench/gen_a.gd").new()
		"A2": gen = preload("res://spikes/gen_bench/gen_a2.gd").new()
		"B": gen = ClassDB.instantiate("ExoGen")
		_:
			push_error("unknown variant " + variant)
			quit(2)
			return
	for k in n_chunks:
		chunk_list.append(Vector3i(k % 6, (k * 37 + 11) % 128, (k * 53 + 29) % 128))

	var result := {"variant": variant, "chunks": n_chunks, "reps": reps, "cores": OS.get_processor_count()}
	gen.call("setup", SEED)
	var t0 := Time.get_ticks_usec()
	gen.call("bake")
	result["bake_ms_single"] = (Time.get_ticks_usec() - t0) / 1000.0
	print("%s bake (single thread) %.0f ms" % [variant, result["bake_ms_single"]])
	if gen.has_method("bake_threads"):
		t0 = Time.get_ticks_usec()
		gen.call("bake_threads", OS.get_processor_count())
		result["bake_ms_threads"] = (Time.get_ticks_usec() - t0) / 1000.0
		print("%s bake (%d threads) %.0f ms" % [variant, OS.get_processor_count(), result["bake_ms_threads"]])

	for i in 20:  # warm-up
		gen.call("build_chunk", chunk_list[i % n_chunks].x, chunk_list[i % n_chunks].y, chunk_list[i % n_chunks].z)

	# Single thread, wall time per chunk and time inside the generator.
	var wall: Array[float] = []
	var inner: Array[float] = []
	var sum_h := 0.0
	var n_canopy := 0
	var n_rocks := 0
	var biomes := [0, 0, 0, 0]
	var first_chunk: Dictionary
	for rep in reps:
		for c in chunk_list:
			var ta := Time.get_ticks_usec()
			var r: Dictionary = gen.call("build_chunk", c.x, c.y, c.z)
			var tb := Time.get_ticks_usec()
			wall.append((tb - ta) / 1000.0)
			inner.append(float(r["usec"]) / 1000.0)
			if rep == 0:
				sum_h += float(r["height_sum"])
				n_canopy += (r["canopy"] as PackedFloat32Array).size() / 12
				n_rocks += (r["rocks"] as PackedFloat32Array).size() / 12
				for b in 4:
					biomes[b] += (r["biomes"] as PackedInt32Array)[b]
				if first_chunk.is_empty():
					first_chunk = r
	result["single"] = _stats(wall)
	result["single_inner"] = _stats(inner)
	result["call_overhead_ms_mean"] = result["single"]["mean_ms"] - result["single_inner"]["mean_ms"]
	result["checksum"] = {"height_sum": sum_h, "canopy": n_canopy, "rocks": n_rocks, "biomes": biomes}
	result["first_chunk"] = {
		"verts": (first_chunk["verts"] as PackedVector3Array).size(),
		"v100": str((first_chunk["verts"] as PackedVector3Array)[100]),
		"n100": str((first_chunk["normals"] as PackedVector3Array)[100]),
	}
	print("%s single: mean %.3f  p95 %.3f  max %.3f ms (inside %.3f ms)  checksum %s" % [
		variant, result["single"]["mean_ms"], result["single"]["p95_ms"], result["single"]["max_ms"],
		result["single_inner"]["mean_ms"], str(result["checksum"])])

	# Throughput with the WorkerThreadPool.
	result["throughput"] = {}
	for n_threads in thread_counts:
		if n_threads > OS.get_processor_count():
			continue
		var total := n_chunks * reps
		var cl := chunk_list
		var g := gen
		var action := func(idx: int) -> void:
			var c: Vector3i = cl[idx % cl.size()]
			g.call("build_chunk", c.x, c.y, c.z)
		var ta := Time.get_ticks_usec()
		var gid := WorkerThreadPool.add_group_task(action, total, n_threads, true, "bench")
		WorkerThreadPool.wait_for_group_task_completion(gid)
		var secs := (Time.get_ticks_usec() - ta) / 1e6
		result["throughput"][str(n_threads)] = total / secs
		print("%s throughput %d threads: %.1f chunks/s" % [variant, n_threads, total / secs])

	if out_path != "":
		var f := FileAccess.open(out_path, FileAccess.WRITE)
		f.store_string(JSON.stringify(result, "  "))
		f.close()
	quit(0)


static func _stats(values: Array[float]) -> Dictionary:
	var sorted := values.duplicate()
	sorted.sort()
	var sum := 0.0
	for v in values:
		sum += v
	return {
		"mean_ms": sum / values.size(),
		"p95_ms": sorted[int(0.95 * (sorted.size() - 1))],
		"max_ms": sorted[sorted.size() - 1],
		"min_ms": sorted[0],
		"n": values.size(),
	}
