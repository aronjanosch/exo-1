extends SceneTree
## Spike 8, T4 and T6 (CPU only): bake, sites, chunk build time per depth with and without
## scatter, scatter instances per finest chunk.
## Run: godot --headless --path . --script res://spikes/planet_gen/bench.gd
## The chunk times are the full GDScript-visible cost of PlanetGen.build_chunk (Rust work plus
## conversion into packed arrays), main thread, one call at a time.

const Terrain := preload("res://spikes/planet/terrain.gd")
const SAMPLES := 300


func _initialize() -> void:
	_run.call_deferred()


func _stats(us: PackedFloat64Array) -> String:
	us.sort()
	var mean := 0.0
	for v in us:
		mean += v
	mean /= us.size()
	return "mean %.0f us  P95 %.0f us  max %.0f us" % [mean, us[int(us.size() * 0.95)], us[us.size() - 1]]


func _run() -> void:
	var gen: RefCounted = ClassDB.instantiate("PlanetGen")
	var json := FileAccess.get_file_as_string("res://spikes/planet_gen/recipe.json")
	gen.load_recipe(json, -1, 0.0)
	var t0 := Time.get_ticks_usec()
	var st: Dictionary = gen.bake(0)
	print("T6 bake (all cores, from GDScript): %.1f ms wall, core reports %.1f ms (macro %.1f, sea level %.1f, statistics %.1f, sites %.1f), threads %d" % [
		(Time.get_ticks_usec() - t0) / 1000.0, st.bake_ms, st.macro_ms, st.sea_ms, st.stats_ms, st.sites_ms, st.threads])
	t0 = Time.get_ticks_usec()
	gen.load_recipe(json, -1, 0.0)
	gen.bake(1)
	print("T6 bake, 1 thread: %.1f ms wall" % [(Time.get_ticks_usec() - t0) / 1000.0])
	print("T4 sites: count %d, smallest pair %.1f m, mean nearest neighbour %.1f m, largest nearest neighbour %.1f m" % [
		st.site_count, st.site_min_pair_m, st.site_mean_nn_m, st.site_max_nn_m])

	var max_depth := maxi(1, roundi(log(5000.0 * PI * 0.5 / 37.0) / log(2.0)))
	var rng := RandomNumberGenerator.new()
	rng.seed = 42
	for scatter in [false, true]:
		for depth in range(0, max_depth + 1):
			if scatter and depth < max_depth - 1:
				continue
			var n := 1 << depth
			var size := 2.0 / n
			var us := PackedFloat64Array()
			for _i in SAMPLES:
				var face := rng.randi() % 6
				var ix := rng.randi() % n
				var iy := rng.randi() % n
				var t := Time.get_ticks_usec()
				gen.build_chunk(face, -1.0 + ix * size, -1.0 + iy * size, size, scatter)
				us.append(Time.get_ticks_usec() - t)
			print("T6 chunk depth %d %s: %s" % [depth, "with scatter   " if scatter else "without scatter", _stats(us)])

	# scatter instances per finest chunk
	var n := 1 << max_depth
	var size := 2.0 / n
	var counts := {}
	var total := PackedInt32Array()
	var nonempty := 0
	var mx := 0
	var sum_nonempty := 0
	for _i in 1500:
		var d: Dictionary = gen.build_chunk(rng.randi() % 6, -1.0 + (rng.randi() % n) * size, -1.0 + (rng.randi() % n) * size, size, true)
		var c := 0
		for kind in d.scatter:
			counts[kind] = counts.get(kind, 0) + d.scatter[kind].count
			c += d.scatter[kind].count
		total.append(c)
		mx = maxi(mx, c)
		if c > 0:
			nonempty += 1
			sum_nonempty += c
	var s := 0
	for v in total:
		s += v
	print("T6 scatter at depth %d (1500 random chunks of about 31 m): mean %.1f instances per chunk, max %d, chunks with any %d%%, mean over those %.1f; per kind totals %s" % [
		max_depth, s / 1500.0, mx, nonempty * 100 / 1500, sum_nonempty / maxf(nonempty, 1), str(counts)])
	quit(0)
