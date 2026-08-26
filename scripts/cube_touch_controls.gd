extends CanvasLayer
class_name CubeTouchControls

## Mobile touch navigation overlay for the 3D cube chamber:
##   < >  snap-turn camera yaw, ^ v snap-pitch, spin buttons = tumbler.
## Buttons are 80px (comfortable touch targets) and consume only their own
## touches; the overlay root is MOUSE_FILTER_IGNORE so board clicks pass by.

signal turn_left
signal turn_right
signal pitch_up
signal pitch_down
signal spin_ccw
signal spin_cw

const BUTTON_SIZE := 80.0


func _ready() -> void:
	layer = 12
	var root := Control.new()
	root.name = "TouchRoot"
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)

	# Bottom-left: yaw pads.
	var left := HBoxContainer.new()
	left.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	left.position = Vector2(16, -104)
	left.add_theme_constant_override("separation", 10)
	root.add_child(left)
	_add_button(left, "◀", func(): turn_left.emit())
	_add_button(left, "▶", func(): turn_right.emit())

	# Bottom-right: pitch pads.
	var right := HBoxContainer.new()
	right.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	right.grow_horizontal = Control.GROW_DIRECTION_BOTH
	right.grow_vertical = Control.GROW_DIRECTION_BEGIN
	right.position = Vector2(-186, -104)
	right.add_theme_constant_override("separation", 10)
	root.add_child(right)
	_add_button(right, "▲", func(): pitch_up.emit())
	_add_button(right, "▼", func(): pitch_down.emit())

	# Bottom-center: tumbler spin pair.
	var center := HBoxContainer.new()
	center.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	center.grow_horizontal = Control.GROW_DIRECTION_BOTH
	center.grow_vertical = Control.GROW_DIRECTION_BEGIN
	center.position = Vector2(-85, -104)
	center.add_theme_constant_override("separation", 10)
	root.add_child(center)
	_add_button(center, "CCW", func(): spin_ccw.emit())
	_add_button(center, "CW", func(): spin_cw.emit())


func _add_button(parent: Control, label: String, action: Callable) -> void:
	var b := Button.new()
	b.text = label
	b.custom_minimum_size = Vector2(BUTTON_SIZE, BUTTON_SIZE)
	b.add_theme_font_size_override("font_size", 26 if label.length() > 1 else 34)
	b.mouse_filter = Control.MOUSE_FILTER_STOP
	b.focus_mode = Control.FOCUS_NONE
	b.pressed.connect(action)
	parent.add_child(b)
