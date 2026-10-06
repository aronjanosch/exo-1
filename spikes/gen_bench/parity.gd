extends SceneTree
## Compares two variants chunk by chunk: godot --headless --path . -s res://spikes/gen_bench/parity.gd -- --a=A --b=A2 [--chunks=40]

func _make(name: String) -> Object:
	match name:
		"A": return preload("res://spikes/gen_bench/gen_a.gd").new()
		"A2": return preload("res://spikes/gen_bench/gen_a2.gd").new()
		_: return ClassDB.instantiate("ExoGen")


func _init() -> void:
	var args := {}
	for a in OS.get_cmdline_user_args():
		var kv := a.trim_prefix("--").split("=", true, 1)
		args[kv[0]] = kv[1] if kv.size() > 1 else "1"
	var ga := _make(args.get("a", "A"))
	var gb := _make(args.get("b", "A2"))
	for g in [ga, gb]:
		g.call("setup", 1337)
		g.call("bake")
	var n := int(args.get("chunks", "40"))
	var max_v := 0.0
	var max_n := 0.0
	var max_h := 0.0
	var cnt_diff := 0
	var bio_diff := 0
	var scat_pos := 0.0
	for k in n:
		var f := k % 6
		var ix := (k * 37 + 11) % 128
		var iy := (k * 53 + 29) % 128
		var ra: Dictionary = ga.call("build_chunk", f, ix, iy)
		var rb: Dictionary = gb.call("build_chunk", f, ix, iy)
		var va: PackedVector3Array = ra.verts
		var vb: PackedVector3Array = rb.verts
		var na: PackedVector3Array = ra.normals
		var nb: PackedVector3Array = rb.normals
		for i in va.size():
			max_v = maxf(max_v, va[i].distance_to(vb[i]))
			max_n = maxf(max_n, na[i].distance_to(nb[i]))
		max_h = maxf(max_h, absf(float(ra.height_sum) - float(rb.height_sum)))
		for key in ["canopy", "rocks"]:
			var ca: PackedFloat32Array = ra[key]
			var cb: PackedFloat32Array = rb[key]
			if ca.size() != cb.size():
				cnt_diff += 1
			else:
				for i in ca.size():
					scat_pos = maxf(scat_pos, absf(ca[i] - cb[i]))
		if ra.biomes != rb.biomes:
			bio_diff += 1
	print("parity %s vs %s over %d chunks: max vert diff %.4f m, max normal diff %.5f, max height_sum diff %.4f, chunks with different scatter counts %d, different biome counts %d, max scatter value diff %.4f" % [
		args.get("a", "A"), args.get("b", "A2"), n, max_v, max_n, max_h, cnt_diff, bio_diff, scat_pos])
	quit(0)
