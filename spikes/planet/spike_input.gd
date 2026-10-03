extends RefCounted
## Key state for the spike: real keyboard OR keys held by a script (auto-test).
## Scripted keys bypass Godot's input state, which is cleared whenever the
## window loses focus, so test runs do not depend on the desktop.

static var held := {}  # Key -> true, set by auto_test.gd


static func pressed(key: Key) -> bool:
	return held.has(key) or Input.is_physical_key_pressed(key)


static func axis(pos_key: Key, neg_key: Key) -> float:
	return float(pressed(pos_key)) - float(pressed(neg_key))
