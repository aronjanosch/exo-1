extends RefCounted
## Timestamp-ordered, bounded history in shared coordinates. No extrapolation.
var history: Array[Dictionary] = []
var duplicates := 0
var reordered := 0
var restart_floor := -INF

func push(s: Dictionary) -> void:
	if s.t < restart_floor:
		return
	# A rejoining owner restarts its sequence but has a newer shared timestamp.
	# Reordered older packets do not trigger this reset or bridge two lives.
	if not history.is_empty() and s.t > history[-1].t and s.seq < history[-1].seq:
		history.clear()
		restart_floor = s.t
	for i in history.size():
		if history[i].seq == s.seq:
			duplicates += 1
			return
		if history[i].t > s.t:
			history.insert(i, s)
			reordered += 1
			_trim()
			return
	history.append(s)
	_trim()

func _trim() -> void:
	while history.size() > 128:
		history.pop_front()

func sample(t: float) -> Dictionary:
	if history.is_empty():
		return {}
	if t < history[0].t:
		var first := history[0].duplicate()
		first.mode = "startup"
		return first
	for i in range(1, history.size()):
		if history[i].t >= t:
			return between(history[i - 1], history[i], t)
	var last := history[-1].duplicate()
	last.mode = "hold"
	return last

static func between(a: Dictionary, b: Dictionary, t: float) -> Dictionary:
	var span: float = b.t - a.t
	var u := clampf((t - a.t) / maxf(span, 0.000001), 0, 1)
	var s := a.duplicate()
	s.mode = "interpolate"
	s.t = t
	if a.planet == b.planet:
		s.p = hermite(a.p, a.v, b.p, b.v, span, u)
		s.v = a.v.lerp(b.v, u)
		s.q = a.q.slerp(b.q, u)
	elif u >= 1.0:
		s = b.duplicate()
		s.mode = "transition"
	if a.frame == b.frame and a.frame_id == b.frame_id and a.planet == b.planet:
		s.wp = hermite(a.wp, a.wv, b.wp, b.wv, span, u)
		s.wv = a.wv.lerp(b.wv, u)
		s.wq = a.wq.slerp(b.wq, u)
	elif u >= 1.0:
		for key in ["frame", "frame_id", "wp", "wv", "wq", "flags"]:
			s[key] = b[key]
	return s

static func hermite(p0: Vector3, v0: Vector3, p1: Vector3, v1: Vector3,
		dt: float, u: float) -> Vector3:
	var u2 := u * u
	var u3 := u2 * u
	return p0 * (2 * u3 - 3 * u2 + 1) + v0 * dt * (u3 - 2 * u2 + u) \
		+ p1 * (-2 * u3 + 3 * u2) + v1 * dt * (u3 - u2)
