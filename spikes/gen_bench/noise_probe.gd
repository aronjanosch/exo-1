extends SceneTree
## Writes the 10 SPEC noises at 1000 test points to a CSV.
## godot --headless --path <project> -s res://spikes/gen_bench/noise_probe.gd -- /path/out.csv

const SEED := 1337
# name, seed offset, frequency, fractal (0 none, 1 fbm, 2 ridged), octaves
const CFG := [
	["region", 1, 0.00033, 1, 3],
	["face", 2, 0.0011, 2, 4],
	["foot", 3, 0.0066, 1, 4],
	["warp", 4, 0.0025, 0, 1],
	["warped", 5, 0.002, 1, 2],
	["m_elev", 10, 0.0002, 1, 3],
	["m_moist", 11, 0.0003, 1, 2],
	["m_temp", 12, 0.0003, 1, 2],
	["m_land", 13, 0.0004, 0, 1],
	["forest", 20, 0.005, 1, 2],
]


func _init() -> void:
	var args := OS.get_cmdline_user_args()
	var path: String = args[0] if args.size() > 0 else "noise_probe_godot.csv"
	var noises: Array[FastNoiseLite] = []
	var head := "k"
	for c in CFG:
		var n := FastNoiseLite.new()
		n.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
		n.seed = SEED + c[1]
		n.frequency = c[2]
		n.fractal_lacunarity = 2.0
		n.fractal_gain = 0.5
		n.fractal_octaves = c[4]
		match c[3]:
			0: n.fractal_type = FastNoiseLite.FRACTAL_NONE
			1: n.fractal_type = FastNoiseLite.FRACTAL_FBM
			2: n.fractal_type = FastNoiseLite.FRACTAL_RIDGED
		noises.append(n)
		head += "," + c[0]
	var f := FileAccess.open(path, FileAccess.WRITE)
	f.store_line(head)
	for k in 1000:
		var p := Vector3(
			float((k * 7919) % 6001 - 3000),
			float((k * 104729) % 6001 - 3000),
			float((k * 1299709) % 6001 - 3000))
		var line := str(k)
		for n in noises:
			line += "," + String.num(n.get_noise_3dv(p), 12)
		f.store_line(line)
	f.close()
	quit()
