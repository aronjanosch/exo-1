extends CanvasLayer
## Minimal piloting feedback, independent of the F3 diagnostics.

var ship: RigidBody3D
var _label: Label
var _elapsed := 0.0


func _ready() -> void:
	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
	margin.add_theme_constant_override("margin_left", 24)
	margin.add_theme_constant_override("margin_bottom", 24)
	add_child(margin)
	_label = Label.new()
	_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_label.size_flags_vertical = Control.SIZE_SHRINK_END
	_label.add_theme_font_size_override("font_size", 20)
	_label.add_theme_color_override("font_color", Color(0.75, 0.95, 1.0))
	_label.add_theme_color_override("font_outline_color", Color.BLACK)
	_label.add_theme_constant_override("outline_size", 5)
	margin.add_child(_label)
	visible = false


func _process(delta: float) -> void:
	var just_seated: bool = ship.piloted and not visible
	visible = ship.piloted
	_elapsed += delta
	if not visible or (_elapsed < 0.1 and not just_seated):
		return
	_elapsed = 0.0
	var planet_pos: Vector3 = ship.planet.to_planet(ship.global_position)
	var up := planet_pos.normalized()
	var altitude: float = planet_pos.length() - ship.planet.planet_radius
	var nose_pitch := rad_to_deg(asin(clampf((-ship.global_basis.z).dot(up), -1.0, 1.0)))
	var view_pitch := rad_to_deg(asin(clampf((-ship.camera.global_basis.z).dot(up), -1.0, 1.0)))
	var limit := "%.0f m/s" % ship.forward_speed_limit if ship.hover_assist else "manual"
	_label.text = "SPEED  %5.1f m/s     FORWARD LIMIT  %s\nALTITUDE  %5.0f m     BRAKE [X]  %s\nGROUND  %5.0f m      VERTICAL  %+.1f m/s\nNOSE  %+.1f°     VIEW  %+.1f°\nFLIGHT ASSIST [H]  %s     PLANET FOLLOW [L]  %s" % [
		ship.linear_velocity.length(), limit, altitude, "ACTIVE" if ship.brake_active else "READY",
		ship.clearance_at(ship.global_position),
		ship.linear_velocity.dot(up), nose_pitch, view_pitch, "ON" if ship.hover_assist else "OFF",
		"ON %.0f%%" % (ship.planet_follow_strength * 100.0) if ship.horizon_follow else "OFF"]
