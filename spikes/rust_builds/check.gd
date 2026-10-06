extends Node
## Spike 7 check, runs inside an exported build: loads ExoGen, builds the 200 spike-6 chunks,
## compares the checksum and prints one line "SPIKE7 {json}". Exit code 0 = checksum ok.

const EXPECTED := {"height_sum": 4169609.072, "canopy": 3021, "rocks": 11549}


func _ready() -> void:
	var result := {"os": OS.get_name(), "ok": false}
	if not ClassDB.class_exists("ExoGen"):
		result["error"] = "ExoGen not loaded"
		_finish(result)
		return
	var gen: Object = ClassDB.instantiate("ExoGen")
	gen.call("setup", 1337)
	var t0 := Time.get_ticks_usec()
	gen.call("bake_threads", OS.get_processor_count())
	result["bake_ms"] = (Time.get_ticks_usec() - t0) / 1000.0
	var chunks: Array[Vector3i] = []
	for k in 200:
		chunks.append(Vector3i(k % 6, (k * 37 + 11) % 128, (k * 53 + 29) % 128))
	for i in 20:
		gen.call("build_chunk", chunks[i].x, chunks[i].y, chunks[i].z)
	var sum_h := 0.0
	var canopy := 0
	var rocks := 0
	t0 = Time.get_ticks_usec()
	for c in chunks:
		var r: Dictionary = gen.call("build_chunk", c.x, c.y, c.z)
		sum_h += float(r["height_sum"])
		canopy += (r["canopy"] as PackedFloat32Array).size() / 12
		rocks += (r["rocks"] as PackedFloat32Array).size() / 12
	result["chunk_ms_mean"] = (Time.get_ticks_usec() - t0) / 1000.0 / chunks.size()
	result["height_sum"] = sum_h
	result["canopy"] = canopy
	result["rocks"] = rocks
	result["ok"] = absf(sum_h - EXPECTED.height_sum) < 0.01 and canopy == EXPECTED.canopy and rocks == EXPECTED.rocks
	_finish(result)


func _finish(result: Dictionary) -> void:
	var line := "SPIKE7 " + JSON.stringify(result)
	print(line)
	var f := FileAccess.open(OS.get_executable_path().get_base_dir().path_join("spike7_result.json"), FileAccess.WRITE)
	if f:
		f.store_string(JSON.stringify(result))
		f.close()
	get_tree().quit(0 if result.ok else 1)
