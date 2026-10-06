extends SceneTree
## How much of a chunk is just the FastNoiseLite calls? godot --headless --path . -s res://spikes/gen_bench/profile_noise.gd
## Runs the 7 noise calls per vertex for 1225 vertices x 200 chunks with nothing else around them.

func _init() -> void:
	var A = preload("res://spikes/gen_bench/gen_a2.gd")
	var g = A.new()
	g.setup(1337)
	var region: FastNoiseLite = g.templates.region
	var facen: FastNoiseLite = g.templates.face
	var foot: FastNoiseLite = g.templates.foot
	var warp: FastNoiseLite = g.templates.warp
	var warped: FastNoiseLite = g.templates.warped
	var sink := 0.0
	var n := 200 * 35 * 35
	var t0 := Time.get_ticks_usec()
	for i in n:
		var x := float(i % 4000) * 1.5
		var y := float(i % 3777) * 1.3
		var z := float(i % 2999) * 1.1
		sink += region.get_noise_3d(x, y, z) + facen.get_noise_3d(x, y, z) + foot.get_noise_3d(x, y, z) \
			+ warp.get_noise_3d(x + 1013.0, y, z) + warp.get_noise_3d(x, y + 2027.0, z) + warp.get_noise_3d(x, y, z + 3041.0) \
			+ warped.get_noise_3d(x, y, z)
	var us := Time.get_ticks_usec() - t0
	print("7 noise calls x 1225 vertices: %.3f ms per chunk (loop overhead included, sink %f)" % [us / 200.0 / 1000.0, sink])
	t0 = Time.get_ticks_usec()
	for i in n:
		var x := float(i % 4000) * 1.5
		var y := float(i % 3777) * 1.3
		var z := float(i % 2999) * 1.1
		sink += x + y + z
	us = Time.get_ticks_usec() - t0
	print("empty loop baseline: %.3f ms per chunk" % [us / 200.0 / 1000.0])
	quit(0)
