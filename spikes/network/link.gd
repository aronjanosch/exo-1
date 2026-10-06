extends RefCounted
## One-way artificial delay in the snapshot layer, not the OS/network stack.
var delay := 0.0
var jitter := 0.0
var loss := 0.0
var rng := RandomNumberGenerator.new()
var queue: Array[Dictionary] = []
var sent := 0
var dropped := 0

func configure(delay_ms: float, jitter_ms: float, loss_percent: float, seed_value: int) -> void:
	delay = clampf(delay_ms, 0, 5000) / 1000.0
	jitter = clampf(jitter_ms, 0, 5000) / 1000.0
	loss = clampf(loss_percent, 0, 100) / 100.0
	rng.seed = seed_value

func enqueue(now: float, data: PackedByteArray, destination := 0) -> void:
	sent += 1
	if rng.randf() < loss:
		dropped += 1
		return
	var due := now + maxf(0.0, delay + rng.randf_range(-jitter, jitter))
	var item := {"due": due, "data": data, "destination": destination}
	var i := queue.size()
	while i > 0 and queue[i - 1].due > due:
		i -= 1
	queue.insert(i, item)
	if queue.size() > 4096:
		queue.pop_front()
		dropped += 1

func ready(now: float) -> Array[Dictionary]:
	var output: Array[Dictionary] = []
	while not queue.is_empty() and queue[0].due <= now:
		output.append(queue.pop_front())
	return output
