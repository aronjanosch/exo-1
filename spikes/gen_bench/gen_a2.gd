extends RefCounted
## Variant A2: GDScript, optimised. Same output as A, every trick that stays in GDScript:
## per-thread noise sets (no duplicate per call), scalar noise calls (no Vector3 temporaries),
## inlined cube_to_sphere and macro lookup, normals only for inner vertices (ring copies),
## in-place writes into pre-sized packed arrays, threaded macro bake.
## Workload: see SPEC.md.

const R := 6000.0
const GRID := 32
const M := GRID + 3
const MACRO := 512
const CHUNK_SIZE := 2.0 / 128.0
const FACE_NORMALS: Array[Vector3] = [
	Vector3(1, 0, 0), Vector3(-1, 0, 0),
	Vector3(0, 1, 0), Vector3(0, -1, 0),
	Vector3(0, 0, 1), Vector3(0, 0, -1),
]
const BIOME_COLORS: Array[Color] = [
	Color(0.20, 0.55, 0.20), Color(0.75, 0.65, 0.40),
	Color(0.45, 0.40, 0.38), Color(0.90, 0.92, 0.95),
]

var seed_value := 1337
var templates: Dictionary
var macro := PackedFloat32Array()
var _sets: Dictionary = {}
var _mutex := Mutex.new()
var _face_u: Array[Vector3] = []
var _face_v: Array[Vector3] = []


func setup(seed: int) -> void:
	seed_value = seed
	templates = {
		"region": _mk(seed + 1, 0.00033, FastNoiseLite.FRACTAL_FBM, 3),
		"face": _mk(seed + 2, 0.0011, FastNoiseLite.FRACTAL_RIDGED, 4),
		"foot": _mk(seed + 3, 0.0066, FastNoiseLite.FRACTAL_FBM, 4),
		"warp": _mk(seed + 4, 0.0025, FastNoiseLite.FRACTAL_NONE, 1),
		"warped": _mk(seed + 5, 0.002, FastNoiseLite.FRACTAL_FBM, 2),
		"m_elev": _mk(seed + 10, 0.0002, FastNoiseLite.FRACTAL_FBM, 3),
		"m_moist": _mk(seed + 11, 0.0003, FastNoiseLite.FRACTAL_FBM, 2),
		"m_temp": _mk(seed + 12, 0.0003, FastNoiseLite.FRACTAL_FBM, 2),
		"m_land": _mk(seed + 13, 0.0004, FastNoiseLite.FRACTAL_NONE, 1),
		"forest": _mk(seed + 20, 0.005, FastNoiseLite.FRACTAL_FBM, 2),
	}
	_face_u.clear()
	_face_v.clear()
	for f in 6:
		var nrm := FACE_NORMALS[f]
		var u := Vector3(nrm.y, nrm.z, nrm.x)
		_face_u.append(u)
		_face_v.append(nrm.cross(u))


static func _mk(seed: int, freq: float, fractal: int, octaves: int) -> FastNoiseLite:
	var n := FastNoiseLite.new()
	n.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
	n.seed = seed
	n.frequency = freq
	n.fractal_type = fractal
	n.fractal_octaves = octaves
	n.fractal_lacunarity = 2.0
	n.fractal_gain = 0.5
	return n


## One set of noise objects per thread, created on first use.
func _noise_set() -> Dictionary:
	var tid := OS.get_thread_caller_id()
	_mutex.lock()
	var s = _sets.get(tid)
	if s == null:
		s = {}
		for k in templates:
			s[k] = (templates[k] as FastNoiseLite).duplicate()
		_sets[tid] = s
	_mutex.unlock()
	return s


func bake() -> void:
	macro.resize(6 * MACRO * MACRO * 4)
	for row in 6 * MACRO:
		_bake_row(row)


func bake_threads(n: int) -> void:
	macro.resize(6 * MACRO * MACRO * 4)
	var gid := WorkerThreadPool.add_group_task(_bake_row, 6 * MACRO, n, true, "macro bake")
	WorkerThreadPool.wait_for_group_task_completion(gid)


func _bake_row(row: int) -> void:
	var s := _noise_set()
	var m_elev: FastNoiseLite = s.m_elev
	var m_moist: FastNoiseLite = s.m_moist
	var m_temp: FastNoiseLite = s.m_temp
	var m_land: FastNoiseLite = s.m_land
	var f := row / MACRO
	var j := row % MACRO
	var nrm := FACE_NORMALS[f]
	var fu := _face_u[f]
	var fv := _face_v[f]
	var b := -1.0 + (j + 0.5) * 2.0 / MACRO
	var k := row * MACRO * 4
	for i in MACRO:
		var a := -1.0 + (i + 0.5) * 2.0 / MACRO
		var px := nrm.x + fu.x * a + fv.x * b
		var py := nrm.y + fu.y * a + fv.y * b
		var pz := nrm.z + fu.z * a + fv.z * b
		var x2 := px * px
		var y2 := py * py
		var z2 := pz * pz
		var dx := px * sqrt(1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0)
		var dy := py * sqrt(1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0)
		var dz := pz * sqrt(1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0)
		var wx := dx * R
		var wy := dy * R
		var wz := dz * R
		var elev := m_elev.get_noise_3d(wx, wy, wz)
		macro[k] = elev
		macro[k + 1] = 1.0 - absf(dy) * 1.2 + 0.3 * m_temp.get_noise_3d(wx, wy, wz) - 0.2 * maxf(elev, 0.0)
		macro[k + 2] = m_moist.get_noise_3d(wx, wy, wz)
		macro[k + 3] = float(clampi(floori((m_land.get_noise_3d(wx, wy, wz) + 1.0) * 2.0), 0, 3))
		k += 4


## Thread safe after bake(): only reads macro, uses the calling thread's noise set.
func build_chunk(face: int, ix: int, iy: int) -> Dictionary:
	var t0 := Time.get_ticks_usec()
	var s := _noise_set()
	var region: FastNoiseLite = s.region
	var facen: FastNoiseLite = s.face
	var foot: FastNoiseLite = s.foot
	var warp: FastNoiseLite = s.warp
	var warped: FastNoiseLite = s.warped

	var a0 := -1.0 + ix * CHUNK_SIZE
	var b0 := -1.0 + iy * CHUNK_SIZE
	var step := CHUNK_SIZE / GRID
	var nrm := FACE_NORMALS[face]
	var fu := _face_u[face]
	var fv := _face_v[face]
	var centre := cube_to_sphere(face, a0 + CHUNK_SIZE * 0.5, b0 + CHUNK_SIZE * 0.5) * R
	var edge_m := (cube_to_sphere(face, a0, b0) - cube_to_sphere(face, a0 + CHUNK_SIZE, b0)).length() * R
	var skirt_depth := maxf(2.0, edge_m / GRID * 4.0)

	var pos := PackedVector3Array()
	var dirs := PackedVector3Array()
	var heights := PackedFloat32Array()
	var rows := PackedByteArray()
	pos.resize(M * M)
	dirs.resize(M * M)
	heights.resize(M * M)
	rows.resize(M * M)
	var mac := macro
	var k := 0
	var t_a := Time.get_ticks_usec()
	for j in M:
		var b := b0 + (j - 1) * step
		var bc := clampf(b, -1.0, 1.0)
		var v := clampf((bc + 1.0) * 0.5 * MACRO - 0.5, 0.0, MACRO - 1.0)
		var j0 := mini(int(v), MACRO - 2)
		var fv_ := v - j0
		var nj := mini(int(v + 0.5), MACRO - 1)
		for i in M:
			var a := a0 + (i - 1) * step
			var px := nrm.x + fu.x * a + fv.x * b
			var py := nrm.y + fu.y * a + fv.y * b
			var pz := nrm.z + fu.z * a + fv.z * b
			var x2 := px * px
			var y2 := py * py
			var z2 := pz * pz
			var dx := px * sqrt(1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0)
			var dy := py * sqrt(1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0)
			var dz := pz * sqrt(1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0)
			var wx := dx * R
			var wy := dy * R
			var wz := dz * R

			# macro lookup, inlined
			var u := clampf((clampf(a, -1.0, 1.0) + 1.0) * 0.5 * MACRO - 0.5, 0.0, MACRO - 1.0)
			var i0 := mini(int(u), MACRO - 2)
			var fu_ := u - i0
			var base := ((face * MACRO + j0) * MACRO + i0) * 4
			var b2 := base + MACRO * 4
			var w00 := (1.0 - fu_) * (1.0 - fv_)
			var w10 := fu_ * (1.0 - fv_)
			var w01 := (1.0 - fu_) * fv_
			var w11 := fu_ * fv_
			var elev := mac[base] * w00 + mac[base + 4] * w10 + mac[b2] * w01 + mac[b2 + 4] * w11
			var temp := mac[base + 1] * w00 + mac[base + 5] * w10 + mac[b2 + 1] * w01 + mac[b2 + 5] * w11
			var moist := mac[base + 2] * w00 + mac[base + 6] * w10 + mac[b2 + 2] * w01 + mac[b2 + 6] * w11
			var land := mac[((face * MACRO + nj) * MACRO + mini(int(u + 0.5), MACRO - 1)) * 4 + 3]

			var h := elev * 80.0 \
				+ region.get_noise_3d(wx, wy, wz) * 100.0 \
				+ facen.get_noise_3d(wx, wy, wz) * 60.0 \
				+ foot.get_noise_3d(wx, wy, wz) * 8.0 \
				+ warped.get_noise_3d(
					wx + 150.0 * warp.get_noise_3d(wx + 1013.0, wy, wz),
					wy + 150.0 * warp.get_noise_3d(wx, wy + 2027.0, wz),
					wz + 150.0 * warp.get_noise_3d(wx, wy, wz + 3041.0)) * 25.0
			var row := 1
			if h > 120.0 and temp < 0.45:
				row = 3
			elif land == 3.0:
				row = 2
			elif moist > 0.0:
				row = 0
			var d := Vector3(dx, dy, dz)
			dirs[k] = d
			heights[k] = h
			rows[k] = row
			pos[k] = d * (R + h)
			k += 1

	var t_b := Time.get_ticks_usec()
	var verts := PackedVector3Array()
	var normals := PackedVector3Array()
	var uvs := PackedVector2Array()
	var colors := PackedColorArray()
	verts.resize(M * M)
	normals.resize(M * M)
	uvs.resize(M * M)
	colors.resize(M * M)
	var biomes := PackedInt32Array([0, 0, 0, 0])
	var height_sum := 0.0
	# inner vertices: i, j in 1..M-2
	for j in range(1, M - 1):
		for i in range(1, M - 1):
			var kk := j * M + i
			var n := (pos[kk + 1] - pos[kk - 1]).cross(pos[kk + M] - pos[kk - M]).normalized()
			if n.dot(dirs[kk]) < 0.0:
				n = -n
			normals[kk] = n
			verts[kk] = pos[kk] - centre
			var row := rows[kk]
			colors[kk] = BIOME_COLORS[row]
			biomes[row] += 1
			height_sum += heights[kk]
	# ring: copy from the clamped inner neighbour, push down along that vertex's direction
	var skirt_uv := Vector2(1, 0)
	for j in M:
		var cj := clampi(j, 1, M - 2)
		var edge_row := j == 0 or j == M - 1
		for i in M:
			if not edge_row and i != 0 and i != M - 1:
				continue
			var kk := j * M + i
			var ck := cj * M + clampi(i, 1, M - 2)
			normals[kk] = normals[ck]
			colors[kk] = colors[ck]
			verts[kk] = pos[ck] - dirs[ck] * skirt_depth - centre
			uvs[kk] = skirt_uv

	var t_c := Time.get_ticks_usec()
	var key := face * 16384 + ix * 128 + iy
	var site_on := (_hash(key, 2, 0, 0, 0) & 3) == 0
	var site_dir := cube_to_sphere(face, a0 + CHUNK_SIZE * 0.5, b0 + CHUNK_SIZE * 0.5)
	var canopy := _scatter(0, 7, key, face, a0, b0, centre, heights, normals, s.forest, site_on, site_dir)
	var rocks := _scatter(1, 11, key, face, a0, b0, centre, heights, normals, s.forest, site_on, site_dir)

	return {
		"verts": verts, "normals": normals, "colors": colors, "uvs": uvs,
		"canopy": canopy, "rocks": rocks,
		"height_sum": height_sum, "biomes": biomes,
		"usec": Time.get_ticks_usec() - t0,
		"phase_usec": [t_a - t0, t_b - t_a, t_c - t_b, Time.get_ticks_usec() - t_c],
	}


func _scatter(kind: int, cells: int, key: int, face: int, a0: float, b0: float, centre: Vector3,
		heights: PackedFloat32Array, normals: PackedVector3Array, forest: FastNoiseLite,
		site_on: bool, site_dir: Vector3) -> PackedFloat32Array:
	var out := PackedFloat32Array()
	out.resize(cells * cells * 12)
	var count := 0
	var cos_slope := 0.8192 if kind == 0 else 0.7071
	var h_min := -20.0 if kind == 0 else -50.0
	var h_max := 140.0 if kind == 0 else 300.0
	var inv_cells := 1.0 / cells
	for cj in cells:
		for ci in cells:
			var s := (ci + _hash01(key, kind, ci, cj, 0)) * inv_cells
			var t := (cj + _hash01(key, kind, ci, cj, 1)) * inv_cells
			var gx := s * GRID
			var gy := t * GRID
			var gi := mini(int(gx), GRID - 1)
			var gj := mini(int(gy), GRID - 1)
			var fx := gx - gi
			var fy := gy - gj
			var k00 := (gj + 1) * M + gi + 1
			var h := lerpf(lerpf(heights[k00], heights[k00 + 1], fx),
				lerpf(heights[k00 + M], heights[k00 + M + 1], fx), fy)
			if h < h_min or h > h_max:
				continue
			var n := normals[k00].lerp(normals[k00 + 1], fx).lerp(
				normals[k00 + M].lerp(normals[k00 + M + 1], fx), fy).normalized()
			var dir := cube_to_sphere(face, a0 + s * CHUNK_SIZE, b0 + t * CHUNK_SIZE)
			if n.dot(dir) < cos_slope:
				continue
			if kind == 0:
				if forest.get_noise_3d(dir.x * R, dir.y * R, dir.z * R) <= 0.1:
					continue
			elif _hash01(key, 1, ci, cj, 2) >= 0.5:
				continue
			if site_on and (dir - site_dir).length_squared() * (R * R) < 144.0:
				continue
			var ref := Vector3(0, 1, 0) if absf(dir.y) < 0.99 else Vector3(1, 0, 0)
			var tg := dir.cross(ref).normalized()
			var bt := dir.cross(tg)
			var yaw := _hash01(key, kind, ci, cj, 3) * TAU
			var x := tg * cos(yaw) + bt * sin(yaw)
			var z := x.cross(dir)
			var sc := 0.8 + 0.6 * _hash01(key, kind, ci, cj, 4) if kind == 0 \
				else 0.5 + 1.0 * _hash01(key, kind, ci, cj, 4)
			var o := dir * (R + h) - centre
			var w := count * 12
			out[w] = x.x * sc
			out[w + 1] = x.y * sc
			out[w + 2] = x.z * sc
			out[w + 3] = dir.x * sc
			out[w + 4] = dir.y * sc
			out[w + 5] = dir.z * sc
			out[w + 6] = z.x * sc
			out[w + 7] = z.y * sc
			out[w + 8] = z.z * sc
			out[w + 9] = o.x
			out[w + 10] = o.y
			out[w + 11] = o.z
			count += 1
	out.resize(count * 12)
	return out


static func _lowbias32(x: int) -> int:
	x &= 0xFFFFFFFF
	x ^= x >> 16
	x = (x * 0x7feb352d) & 0xFFFFFFFF
	x ^= x >> 15
	x = (x * 0x846ca68b) & 0xFFFFFFFF
	x ^= x >> 16
	return x


static func _hash(key: int, kind: int, ci: int, cj: int, salt: int) -> int:
	var inner := (kind * 0x85EBCA6B + ci * 0xC2B2AE35 + cj * 0x27D4EB2F + salt * 0x165667B1) & 0xFFFFFFFF
	return _lowbias32(((key * 0x9E3779B1) & 0xFFFFFFFF) ^ _lowbias32(inner))


static func _hash01(key: int, kind: int, ci: int, cj: int, salt: int) -> float:
	return float(_hash(key, kind, ci, cj, salt) >> 8) / 16777216.0


static func cube_to_sphere(face: int, a: float, b: float) -> Vector3:
	var nrm := FACE_NORMALS[face]
	var u := Vector3(nrm.y, nrm.z, nrm.x)
	var v := nrm.cross(u)
	var p := nrm + u * a + v * b
	var x2 := p.x * p.x
	var y2 := p.y * p.y
	var z2 := p.z * p.z
	return Vector3(
		p.x * sqrt(1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0),
		p.y * sqrt(1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0),
		p.z * sqrt(1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0))
