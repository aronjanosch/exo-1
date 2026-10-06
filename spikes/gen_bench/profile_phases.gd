extends SceneTree
## Phase split of variant A2: setup / vertex loop / normals+ring / scatter (mean ms per chunk).
func _init() -> void:
	var g = preload("res://spikes/gen_bench/gen_a2.gd").new()
	g.setup(1337)
	g.bake()
	var sum := [0.0, 0.0, 0.0, 0.0]
	for k in 200:
		var r: Dictionary = g.build_chunk(k % 6, (k * 37 + 11) % 128, (k * 53 + 29) % 128)
		for i in 4:
			sum[i] += r.phase_usec[i]
	print("A2 phases ms/chunk: setup %.3f  vertex loop %.3f  normals+ring %.3f  scatter %.3f" % [sum[0] / 200000.0, sum[1] / 200000.0, sum[2] / 200000.0, sum[3] / 200000.0])
	quit(0)
