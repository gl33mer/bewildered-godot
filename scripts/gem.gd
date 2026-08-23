extends Sprite2D

class_name Gem

@export var cell_size: float = 64.0

var current_kind: int = -1
var has_echo: bool = false

func _ready():
	# Set texture filter to nearest for crisp pixel art
	texture_filter = 0  # NEAREST

func set_gem(kind: int, echo: bool) -> void:
	current_kind = kind
	has_echo = echo
	
	# Create procedural texture based on kind
	var img = Image.create(cell_size, cell_size, false, Image.FORMAT_RGBA8)
	_draw_gem_shape(img, kind)
	
	var texture = ImageTexture.create_from_image(img)
	texture = texture.duplicate() as ImageTexture

	self.texture = texture
	
	# Apply echo visual effect
	if echo:
		modulate = Color(1.0, 1.0, 0.5, 1.0)  # Yellowish tint for echo
	else:
		modulate = Color(1.0, 1.0, 1.0, 1.0)

func _draw_gem_shape(img: Image, kind: int) -> void:
	var size = int(cell_size)
	var center = Vector2(size / 2.0, size / 2.0)
	var radius = size * 0.35
	var padding = 4
	
	# Clear with transparent
	img.fill(Color(0, 0, 0, 0))
	
	# Colors per kind (matching 03-GAME-DESIGN.md accessibility)
	# 0=Circle(Cyan), 1=Triangle(Yellow), 2=Square(Green), 3=Diamond(Magenta)
	var colors = [
		Color(0.0, 0.8, 0.8, 1.0),    # Cyan - Circle
		Color(0.9, 0.8, 0.1, 1.0),    # Yellow - Triangle
		Color(0.2, 0.8, 0.2, 1.0),    # Green - Square
		Color(0.9, 0.2, 0.9, 1.0),    # Magenta - Diamond
	]
	
	var color = colors[kind % colors.size()]
	var outline_color = Color(color.r * 0.5, color.g * 0.5, color.b * 0.5, 1.0)
	
	match kind:
		0: # Circle
			_draw_circle(img, center, radius, color, outline_color)
		1: # Triangle
			_draw_triangle(img, center, radius, color, outline_color)
		2: # Square
			_draw_square(img, center, radius, color, outline_color)
		3: # Diamond
			_draw_diamond(img, center, radius, color, outline_color)

func _draw_circle(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			var dist = pos.distance_to(center)
			if dist <= radius:
				var alpha = 1.0
				if dist > radius - 2.0:
					alpha = (radius - dist + 1.0) / 2.0
				img.set_pixel(x, y, fill_color.lerp(outline_color, 1.0 - alpha))

func _draw_triangle(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	# Triangle pointing up
	var p1 = Vector2(center.x, center.y - radius)
	var p2 = Vector2(center.x - radius * 0.866, center.y + radius * 0.5)
	var p3 = Vector2(center.x + radius * 0.866, center.y + radius * 0.5)
	
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			if _point_in_triangle(pos, p1, p2, p3):
				var dist_to_edge = _dist_to_triangle_edge(pos, p1, p2, p3)
				var alpha = 1.0
				if dist_to_edge < 2.0:
					alpha = dist_to_edge / 2.0
				img.set_pixel(x, y, fill_color.lerp(outline_color, 1.0 - alpha))

func _draw_square(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	var half = radius
	var left = center.x - half
	var right = center.x + half
	var top = center.y - half
	var bottom = center.y + half
	
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			if pos.x >= left && pos.x <= right && pos.y >= top && pos.y <= bottom:
				var dist_to_edge = min(pos.x - left, right - pos.x, pos.y - top, bottom - pos.y)
				var alpha = 1.0
				if dist_to_edge < 2.0:
					alpha = dist_to_edge / 2.0
				img.set_pixel(x, y, fill_color.lerp(outline_color, 1.0 - alpha))

func _draw_diamond(img: Image, center: Vector2, radius: float, fill_color: Color, outline_color: Color) -> void:
	var size = int(cell_size)
	# Diamond: center, top, right, bottom, left
	for y in range(size):
		for x in range(size):
			var pos = Vector2(x, y)
			var dx = abs(pos.x - center.x)
			var dy = abs(pos.y - center.y)
			if dx + dy <= radius:
				var dist_to_edge = radius - (dx + dy)
				var alpha = 1.0
				if dist_to_edge < 2.0:
					alpha = dist_to_edge / 2.0
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
	return (u >= 0) && (v >= 0) && (u + v <= 1)

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