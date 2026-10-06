extends RefCounted
## Variant A: GDScript, straightforward (same style as spikes/planet/terrain.gd).
## Vector3 maths, one FastNoiseLite call per sample, own noise copies per chunk call.
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


func bake() -> void:
	var m_elev: FastNoiseLite = templates.m_elev
	var m_moist: FastNoiseLite = templates.m_moist
	var m_temp: FastNoiseLite = templates.m_temp
	var m_land: FastNoiseLite = templates.m_land
	macro.resize(6 * MACRO * MACRO * 4)
	for f in 6:
		for j in MACRO:
			var b := -1.0 + (j + 0.5) * 2.0 / MACRO
			for i in MACRO:
				var a := -1.0 + (i + 0.5) * 2.0 / MACRO
				var d := cube_to_sphere(f, a, b)
				var p := d * R
				var elev := m_elev.get_noise_3dv(p)
				var moist := m_moist.get_noise_3dv(p)
				var temp := 1.0 - absf(d.y) * 1.2 + 0.3 * m_temp.get_noise_3dv(p) - 0.2 * maxf(elev, 0.0)
				var land := float(clampi(floori((m_land.get_noise_3dv(p) + 1.0) * 2.0), 0, 3))
				var k := ((f * MACRO + j) * MACRO + i) * 4
				macro[k] = elev
				macro[k + 1] = temp
				macro[k + 2] = moist
				macro[k + 3] = land


## Returns (elev, temp, moist, landform).
func _macro_lookup(face: int, a: float, b: float) -> Vector4:
	var u := clampf((a + 1.0) * 0.5 * MACRO - 0.5, 0.0, MACRO - 1.0)
	var v := clampf((b + 1.0) * 0.5 * MACRO - 0.5, 0.0, MACRO - 1.0)
	var i0 := mini(int(u), MACRO - 2)
	var j0 := mini(int(v), MACRO - 2)
	var fu := u - i0
	var fv := v - j0
	var base := (face * MACRO + j0) * MACRO + i0
	var out := Vector4()
	for c in 3:
		var c00 := macro[base * 4 + c]
		var c10 := macro[(base + 1) * 4 + c]
		var c01 := macro[(base + MACRO) * 4 + c]
		var c11 := macro[(base + MACRO + 1) * 4 + c]
		out[c] = lerpf(lerpf(c00, c10, fu), lerpf(c01, c11, fu), fv)
	var ni := mini(int(u + 0.5), MACRO - 1)
	var nj := mini(int(v + 0.5), MACRO - 1)
	out.w = macro[((face * MACRO + nj) * MACRO + ni) * 4 + 3]
	return out


## Thread safe after bake(): only reads macro and the templates, uses own noise copies.
func build_chunk(face: int, ix: int, iy: int) -> Dictionary:
	var t0 := Time.get_ticks_usec()
	var region: FastNoiseLite = templates.region.duplicate()
	var facen: FastNoiseLite = templates.face.duplicate()
	var foot: FastNoiseLite = templates.foot.duplicate()
	var warp: FastNoiseLite = templates.warp.duplicate()
	var warped: FastNoiseLite = templates.warped.duplicate()
	var forest: FastNoiseLite = templates.forest.duplicate()

	var a0 := -1.0 + ix * CHUNK_SIZE
	var b0 := -1.0 + iy * CHUNK_SIZE
	var step := CHUNK_SIZE / GRID
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
	for j in M:
		for i in M:
			var a := a0 + (i - 1) * step
			var b := b0 + (j - 1) * step
			var d := cube_to_sphere(face, a, b)
			var p := d * R
			var m4 := _macro_lookup(face, clampf(a, -1.0, 1.0), clampf(b, -1.0, 1.0))
			var w := Vector3(
				warp.get_noise_3dv(p + Vector3(1013, 0, 0)),
				warp.get_noise_3dv(p + Vector3(0, 2027, 0)),
				warp.get_noise_3dv(p + Vector3(0, 0, 3041)))
			var pw := p + w * 150.0
			var h := m4.x * 80.0 \
				+ region.get_noise_3dv(p) * 100.0 \
				+ facen.get_noise_3dv(p) * 60.0 \
				+ foot.get_noise_3dv(p) * 8.0 \
				+ warped.get_noise_3dv(pw) * 25.0
			var row := 1
			if h > 120.0 and m4.y < 0.45:
				row = 3
			elif m4.w == 3.0:
				row = 2
			elif m4.z > 0.0:
				row = 0
			var k := j * M + i
			dirs[k] = d
			heights[k] = h
			rows[k] = row
			pos[k] = d * (R + h)

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
	for j in M:
		for i in M:
			var k := j * M + i
			var ck := clampi(j, 1, M - 2) * M + clampi(i, 1, M - 2)
			var nrm := (pos[ck + 1] - pos[ck - 1]).cross(pos[ck + M] - pos[ck - M]).normalized()
			if nrm.dot(dirs[ck]) < 0.0:
				nrm = -nrm
			normals[k] = nrm
			colors[k] = BIOME_COLORS[rows[k]]
			if k == ck:
				verts[k] = pos[k] - centre
				if i >= 1 and i <= GRID + 1 and j >= 1 and j <= GRID + 1:
					biomes[rows[k]] += 1
					height_sum += heights[k]
			else:
				verts[k] = pos[ck] - dirs[ck] * skirt_depth - centre
				uvs[k] = Vector2(1, 0)

	var key := face * 16384 + ix * 128 + iy
	var site_on := (_hash(key, 2, 0, 0, 0) & 3) == 0
	var site_dir := cube_to_sphere(face, a0 + CHUNK_SIZE * 0.5, b0 + CHUNK_SIZE * 0.5)
	var canopy := _scatter(0, 7, key, face, a0, b0, centre, heights, normals, forest, site_on, site_dir)
	var rocks := _scatter(1, 11, key, face, a0, b0, centre, heights, normals, forest, site_on, site_dir)

	return {
		"verts": verts, "normals": normals, "colors": colors, "uvs": uvs,
		"canopy": canopy, "rocks": rocks,
		"height_sum": height_sum, "biomes": biomes,
		"usec": Time.get_ticks_usec() - t0,
	}


func _scatter(kind: int, cells: int, key: int, face: int, a0: float, b0: float, centre: Vector3,
		heights: PackedFloat32Array, normals: PackedVector3Array, forest: FastNoiseLite,
		site_on: bool, site_dir: Vector3) -> PackedFloat32Array:
	var out := PackedFloat32Array()
	var cos_slope := 0.8192 if kind == 0 else 0.7071
	var h_min := -20.0 if kind == 0 else -50.0
	var h_max := 140.0 if kind == 0 else 300.0
	for cj in cells:
		for ci in cells:
			var s := (ci + _hash01(key, kind, ci, cj, 0)) / cells
			var t := (cj + _hash01(key, kind, ci, cj, 1)) / cells
			var gx := s * GRID
			var gy := t * GRID
			var gi := mini(int(gx), GRID - 1)
			var gj := mini(int(gy), GRID - 1)
			var fx := gx - gi
			var fy := gy - gj
			var k00 := (gj + 1) * M + gi + 1
			var h := lerpf(lerpf(heights[k00], heights[k00 + 1], fx),
				lerpf(heights[k00 + M], heights[k00 + M + 1], fx), fy)
			var n := normals[k00].lerp(normals[k00 + 1], fx).lerp(
				normals[k00 + M].lerp(normals[k00 + M + 1], fx), fy).normalized()
			var dir := cube_to_sphere(face, a0 + s * CHUNK_SIZE, b0 + t * CHUNK_SIZE)
			if n.dot(dir) < cos_slope or h < h_min or h > h_max:
				continue
			if kind == 0:
				if forest.get_noise_3dv(dir * R) <= 0.1:
					continue
			elif _hash01(key, 1, ci, cj, 2) >= 0.5:
				continue
			if site_on and (dir - site_dir).length() * R < 12.0:
				continue
			var up := dir
			var ref := Vector3(0, 1, 0) if absf(up.y) < 0.99 else Vector3(1, 0, 0)
			var tg := up.cross(ref).normalized()
			var bt := up.cross(tg)
			var yaw := _hash01(key, kind, ci, cj, 3) * TAU
			var x := tg * cos(yaw) + bt * sin(yaw)
			var z := x.cross(up)
			var sc := 0.8 + 0.6 * _hash01(key, kind, ci, cj, 4) if kind == 0 \
				else 0.5 + 1.0 * _hash01(key, kind, ci, cj, 4)
			var o := dir * (R + h) - centre
			var xs := x * sc
			var ys := up * sc
			var zs := z * sc
			out.append_array(PackedFloat32Array([
				xs.x, xs.y, xs.z, ys.x, ys.y, ys.z, zs.x, zs.y, zs.z, o.x, o.y, o.z]))
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
