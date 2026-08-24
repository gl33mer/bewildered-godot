extends Sprite2D

class_name Gem

@export var cell_size: float = 64.0

var current_kind: int = -1
var has_echo: bool = false
var current_special: int = 0  # 0 = None, 1 = Bolt, 2 = Prism, 3 = Nova

var special_overlay: Label

func _ready() -> void:
	texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	
	special_overlay = Label.new()
	special_overlay.name = "SpecialOverlay"
	special_overlay.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	special_overlay.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	special_overlay.add_theme_font_size_override("font_size", int(cell_size * 0.45))
	special_overlay.visible = false
	special_overlay.z_index = 10
	special_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	special_overlay.position = Vector2(-cell_size * 0.5, -cell_size * 0.5)
	special_overlay.size = Vector2(cell_size, cell_size)
	add_child(special_overlay)

func set_gem(kind: int, echo: bool, special: int = 0) -> void:
	current_kind = kind
	has_echo = echo
	current_special = special
	
	var img = Image.create(int(cell_size), int(cell_size), false, Image.FORMAT_RGBA8)
	_draw_gem_shape(img, kind)
	
	var tex = ImageTexture.create_from_image(img)
	self.texture = tex
	
	if echo:
		modulate = Color(1.0, 1.0, 0.4, 1.0)
	else:
		modulate = Color(1.0, 1.0, 1.0, 1.0)
	
	_update_special_overlay()

func _update_special_overlay() -> void:
	if special_overlay == null:
		return
		
	if current_special == 0:
		special_overlay.visible = false
		return
	
	var text = ""
	var overlay_color = Color.WHITE
	
	match current_special:
		1:
			text = "⚡"
			overlay_color = Color.YELLOW
		2:
			text = "🌈"
			overlay_color = Color.MAGENTA
		3:
			text = "💥"
			overlay_color = Color.ORANGE
		_:
			special_overlay.visible = false
			return
	
	special_overlay.text = text
	special_overlay.add_theme_color_override("font_color", overlay_color)
	special_overlay.add_theme_color_override("font_outline_color", Color.BLACK)
	special_overlay.add_theme_constant_override("outline_size", int(cell_size * 0.08))
	special_overlay.visible = true

func _draw_gem_shape(img: Image, kind: int) -> void:
	var size = int(cell_size)
	var center = Vector2(size / 2.0, size / 2.0)
	var radius = size * 0.35
	
	img.fill(Color(0, 0, 0, 0))
	
	var colors = [
		Color(0.0, 0.85, 0.85, 1.0),
		Color(0.95, 0.85, 0.1, 1.0),
		Color(0.2, 0.85, 0.2, 1.0),
		Color(0.9, 0.2, 0.9, 1.0),
	]
	
	var draw_color = colors[kind % colors.size()]
	var outline_color = Color(draw_color.r * 0.3, draw_color.g * 0.3, draw_color.b * 0.3, 1.0)
	
	match kind:
		0:
			_draw_circle(img, center, radius, draw_color, outline_color)
		1:
			_draw_triangle(img, center, radius, draw_color, outline_color)
		2:
			_draw_square(img, center, radius, draw_color, outline_color)
		3:
			_draw_diamond(img, center, radius, draw_color, outline_color)

func _draw_circle(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			var dist = pos.distance_to(center)
			if dist <= radius:
				var alpha = 1.0
				if dist > radius - 2.5:
					alpha = (radius - dist + 1.0) / 2.5
				img.set_pixel(x, y, fill_color.lerp(outline_color, 1.0 - alpha))

func _draw_triangle(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	var p1 = Vector2(center.x, center.y - radius)
	var p2 = Vector2(center.x - radius * 0.866, center.y + radius * 0.5)
	var p3 = Vector2(center.x + radius * 0.866, center.y + radius * 0.5)
	
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			if _point_in_triangle(pos, p1, p2, p3):
				var dist_to_edge = _dist_to_triangle_edge(pos, p1, p2, p3)
				var alpha = 1.0
				if dist_to_edge < 2.5:
					alpha = dist_to_edge / 2.5
				img.set_pixel(x, y, fill_color.lerp(outline_color, 1.0 - alpha))

func _draw_square(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	var half = radius * 0.9
	var left = center.x - half
	var right = center.x + half
	var top = center.y - half
	var bottom = center.y + half
	
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			if pos.x >= left and pos.x <= right and pos.y >= top and pos.y <= bottom:
				var dist_to_edge = min(pos.x - left, right - pos.x, pos.y - top, bottom - pos.y)
				var alpha = 1.0
				if dist_to_edge < 2.5:
					alpha = dist_to_edge / 2.5
				img.set_pixel(x, y, fill_color.lerp(outline_color, 1.0 - alpha))

func _draw_diamond(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			var dx = abs(pos.x - center.x)
			var dy = abs(pos.y - center.y)
			if dx + dy <= radius:
				var dist_to_edge = radius - (dx + dy)
				var alpha = 1.0
				if dist_to_edge < 2.5:
					alpha = dist_to_edge / 2.5
				img.set_pixel(x, y, fill_color.lerp(outline_color, 1.0 - alpha))

func _point_in_triangle(p: Vector2, a: Vector2, b: Vector2, c: Vector2) -> bool:
	var v0 = c - a
	var v1 = b - a
	var v2 = p - a
	var dot00 = v0.dot(v0)
	var dot01 = v0.dot(v1)
	var dot02 = v0.dot(v2)
	var dot11 = v1.dot(v1)
	var dot12 = v1.dot(v2)
	var inv_denom = 1.0 / (dot00 * dot11 - dot01 * dot01)
	var u = (dot11 * dot02 - dot01 * dot12) * inv_denom
	var v = (dot00 * dot12 - dot01 * dot02) * inv_denom
	return (u >= 0) and (v >= 0) and (u + v <= 1)

func _dist_to_triangle_edge(p: Vector2, a: Vector2, b: Vector2, c: Vector2) -> float:
	return min(
		_dist_to_segment(p, a, b),
		_dist_to_segment(p, b, c),
		_dist_to_segment(p, c, a)
	)

func _dist_to_segment(p: Vector2, a: Vector2, b: Vector2) -> float:
	var ab = b - a
	var ap = p - a
	var t = ap.dot(ab) / ab.dot(ab)
	t = clamp(t, 0.0, 1.0)
	var closest = a + ab * t
	return p.distance_to(closest)
