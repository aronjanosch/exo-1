extends RefCounted
## Fixed data-only wire format. Planet-relative doubles; never Variant objects.
const SIZE := 144
const VERSION := 1

static func encode(s: Dictionary) -> PackedByteArray:
	var b := PackedByteArray()
	b.resize(SIZE)
	b.encode_u32(0, VERSION)
	b.encode_u32(4, s.owner)
	b.encode_u32(8, s.planet)
	b.encode_u32(12, s.frame)
	b.encode_u32(16, s.frame_id)
	b.encode_u32(20, s.seq)
	b.encode_double(24, s.t)
	put_vector(b, 32, s.p, true)
	put_vector(b, 56, s.v, false)
	put_quat(b, 68, s.q)
	put_vector(b, 84, s.wp, true)
	put_vector(b, 108, s.wv, false)
	put_quat(b, 120, s.wq)
	b.encode_u32(136, s.flags)
	b.encode_u32(140, s.input_tick)
	return b

static func decode(b: PackedByteArray) -> Dictionary:
	if b.size() != SIZE or b.decode_u32(0) != VERSION:
		return {}
	var s := {"owner": b.decode_u32(4), "planet": b.decode_u32(8),
		"frame": b.decode_u32(12), "frame_id": b.decode_u32(16),
		"seq": b.decode_u32(20), "t": b.decode_double(24),
		"p": get_vector(b, 32, true), "v": get_vector(b, 56, false),
		"q": get_quat(b, 68), "wp": get_vector(b, 84, true),
		"wv": get_vector(b, 108, false), "wq": get_quat(b, 120),
		"flags": b.decode_u32(136), "input_tick": b.decode_u32(140)}
	if s.owner < 1 or s.owner > 8 or s.planet > 1 or s.frame > 1 or s.frame_id > 8:
		return {}
	if s.frame == 1 and s.frame_id == 0:
		return {}
	if not is_finite(s.t) or s.t < 0.0:
		return {}
	for key in ["p", "v", "wp", "wv"]:
		if not s[key].is_finite() or s[key].length() > 1000000.0:
			return {}
	for key in ["q", "wq"]:
		var q: Quaternion = s[key]
		if not q.is_finite() or q.length_squared() < 0.5 or q.length_squared() > 1.5:
			return {}
		s[key] = q.normalized()
	return s

static func put_vector(b: PackedByteArray, offset: int, v: Vector3, double: bool) -> void:
	for i in 3:
		if double:
			b.encode_double(offset + i * 8, v[i])
		else:
			b.encode_float(offset + i * 4, v[i])

static func get_vector(b: PackedByteArray, offset: int, double: bool) -> Vector3:
	var v := Vector3.ZERO
	for i in 3:
		v[i] = b.decode_double(offset + i * 8) if double else b.decode_float(offset + i * 4)
	return v

static func put_quat(b: PackedByteArray, offset: int, q: Quaternion) -> void:
	var components := [q.x, q.y, q.z, q.w]
	for i in 4:
		b.encode_float(offset + i * 4, components[i])

static func get_quat(b: PackedByteArray, offset: int) -> Quaternion:
	return Quaternion(b.decode_float(offset), b.decode_float(offset + 4),
		b.decode_float(offset + 8), b.decode_float(offset + 12))

static func make(owner: int, t: float, p: Vector3, v: Vector3, q: Quaternion) -> Dictionary:
	return {"owner": owner, "planet": 0, "frame": 0, "frame_id": 0,
		"seq": 0, "t": t, "p": p, "v": v, "q": q,
		"wp": p + Vector3(6, 0, 0), "wv": v, "wq": q,
		"flags": 0, "input_tick": 0}
