extends SceneTree
## Thread scaling of A2: macro bake and chunk throughput at several thread counts.
func _init() -> void:
	var g = preload("res://spikes/gen_bench/gen_a2.gd").new()
	g.setup(1337)
	for n in [1, 2, 4, 8, 12, 16]:
		var t0 := Time.get_ticks_usec()
		g.bake_threads(n)
		print("A2 bake_threads(%d): %.0f ms" % [n, (Time.get_ticks_usec() - t0) / 1000.0])
	for n in [8, 10, 12, 14, 16]:
		var action := func(idx: int) -> void:
			g.build_chunk(idx % 6, (idx * 37 + 11) % 128, (idx * 53 + 29) % 128)
		var t0 := Time.get_ticks_usec()
		WorkerThreadPool.wait_for_group_task_completion(WorkerThreadPool.add_group_task(action, 600, n, true))
		print("A2 chunks, %d threads: %.0f chunks/s" % [n, 600 / ((Time.get_ticks_usec() - t0) / 1e6)])
	quit(0)
