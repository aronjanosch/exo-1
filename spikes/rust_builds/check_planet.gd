extends Node
## Spike 8 check, runs inside an exported build: loads PlanetGen, bakes the recipe, compares sea
## level, site count and a height checksum, builds 200 chunks and prints one line "SPIKE8 {json}".
## Exit code 0 = ok. (Spike 7's check.gd does the same for ExoGen.)

const EXPECTED_SEA := -7.487349
const EXPECTED_SITES := 24
const EXPECTED_SUM := 11950.597682


func _ready() -> void:
	var result := {"os": OS.get_name(), "ok": false}
	if not ClassDB.class_exists("PlanetGen"):
		result["error"] = "PlanetGen not loaded"
		_finish(result)
		return
	var gen: Object = ClassDB.instantiate("PlanetGen")
	if not gen.call("load_recipe", FileAccess.get_file_as_string("res://recipe.json"), -1, 0.0):
		result["error"] = "recipe did not load"
		_finish(result)
		return
	var st: Dictionary = gen.call("bake", 0)
	result["bake_ms"] = st.bake_ms
	result["sea"] = st.sea_level_m
	result["sites"] = st.site_count
	var sum_h := 0.0
	for k in 1000:
		var a := k * 2.399963
		var z := 1.0 - 2.0 * (k + 0.5) / 1000.0
		var r := sqrt(1.0 - z * z)
		sum_h += gen.call("height_at", Vector3(r * cos(a), z, r * sin(a)))
	result["height_sum"] = sum_h
	var t0 := Time.get_ticks_usec()
	var verts := 0
	for k in 200:
		var d: Dictionary = gen.call("build_chunk", k % 6, -1.0 + ((k * 37 + 11) % 128) * (2.0 / 128), -1.0 + ((k * 53 + 29) % 128) * (2.0 / 128), 2.0 / 128, true)
		verts += (d["verts"] as PackedVector3Array).size()
	result["chunk_ms_mean"] = (Time.get_ticks_usec() - t0) / 1000.0 / 200.0
	result["ok"] = absf(st.sea_level_m - EXPECTED_SEA) < 0.001 and st.site_count == EXPECTED_SITES and verts == 200 * 35 * 35 and absf(sum_h - EXPECTED_SUM) < 0.5
	_finish(result)


func _finish(result: Dictionary) -> void:
	var line := "SPIKE8 " + JSON.stringify(result)
	print(line)
	var f := FileAccess.open(OS.get_executable_path().get_base_dir().path_join("spike8_result.json"), FileAccess.WRITE)
	if f:
		f.store_string(JSON.stringify(result))
		f.close()
	get_tree().quit(0 if result.ok else 1)
