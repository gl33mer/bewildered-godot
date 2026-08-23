extends Node2D

class_name GameBoard

@export var board_width: int = 8
@export var board_height: int = 8
@export var seed: int = 12345
@export var cell_size: float = 64.0
@export var padding: float = 8.0

@onready var board_sim: RefCounted = BoardSim.new()
@onready var gem_scene: PackedScene = preload("res://scenes/gem.tscn")

var gem_instances: Array[Node2D] = []

# Input state
var selected_cell: Vector2i = Vector2i(-1, -1)
var cursor_cell: Vector2i = Vector2i(0, 0)
var selection_highlight: Sprite2D
var cursor_highlight: Sprite2D
var is_processing_swap: bool = false

# Rejection feedback state
var rejected_cells: Array[Vector2i] = []
var rejection_flash_timer: float = 0.0
const REJECTION_FLASH_DURATION = 0.3

func _ready():
	_connect_signals()
	_create_highlights()
	_initialize_board()

func _connect_signals() -> void:
	board_sim.match_resolved.connect(_on_match_resolved)
	board_sim.special_gem_created.connect(_on_special_gem_created)
	board_sim.echo_charged.connect(_on_echo_charged)
	board_sim.echo_detonated.connect(_on_echo_detonated)
	board_sim.move_rejected.connect(_on_move_rejected)
	board_sim.objective_progress.connect(_on_objective_progress)

func _create_highlights() -> void:
	# Selection highlight - bright white border
	selection_highlight = Sprite2D.new()
	var sel_img = Image.create(int(cell_size + 4), int(cell_size + 4), false, Image.FORMAT_RGBA8)
	sel_img.fill(Color(0, 0, 0, 0))
	# Draw border
	for x in range(int(cell_size + 4)):
		for y in range(int(cell_size + 4)):
			if x < 2 || x >= int(cell_size + 2) || y < 2 || y >= int(cell_size + 2):
				sel_img.set_pixel(x, y, Color(1, 1, 1, 1))
	var sel_tex = ImageTexture.create_from_image(sel_img)
	selection_highlight.texture = sel_tex
	selection_highlight.texture_filter = 0  # NEAREST
	selection_highlight.visible = false
	selection_highlight.z_index = 10
	add_child(selection_highlight)
	
	# Cursor highlight - subtle cyan border
	cursor_highlight = Sprite2D.new()
	var cur_img = Image.create(int(cell_size + 4), int(cell_size + 4), false, Image.FORMAT_RGBA8)
	cur_img.fill(Color(0, 0, 0, 0))
	for x in range(int(cell_size + 4)):
		for y in range(int(cell_size + 4)):
			if x < 1 || x >= int(cell_size + 3) || y < 1 || y >= int(cell_size + 3):
				cur_img.set_pixel(x, y, Color(0, 0.8, 1, 0.8))
	var cur_tex = ImageTexture.create_from_image(cur_img)
	cursor_highlight.texture = cur_tex
	cursor_highlight.texture_filter = 0  # NEAREST
	cursor_highlight.visible = true
	cursor_highlight.z_index = 5
	add_child(cursor_highlight)
	
	_update_cursor_highlight()

func _initialize_board() -> void:
	board_sim.new_board(board_width, board_height, seed)
	
	for gem in gem_instances:
		if gem != null && is_instance_valid(gem):
			gem.queue_free()
	gem_instances.clear()
	gem_instances.resize(board_width * board_height)
	
	var board_pixel_width = board_width * cell_size + (board_width - 1) * padding
	var board_pixel_height = board_height * cell_size + (board_height - 1) * padding
	var offset_x = -board_pixel_width / 2.0 + cell_size / 2.0
	var offset_y = -board_pixel_height / 2.0 + cell_size / 2.0
	
	for y in range(board_height):
		for x in range(board_width):
			var cell = board_sim.get_cell(x, y)
			if not cell.empty:
				var kind = cell.kind
				var has_echo = cell.has_echo
				
				var gem_instance = gem_scene.instantiate()
				gem_instance.set_gem(kind, has_echo)
				
				var pos_x = offset_x + x * (cell_size + padding)
				var pos_y = offset_y + y * (cell_size + padding)
				gem_instance.position = Vector2(pos_x, pos_y)
				
				add_child(gem_instance)
				gem_instances[y * board_width + x] = gem_instance
			else:
				gem_instances[y * board_width + x] = null
	
	_update_cursor_highlight()

func _get_cell_position(x: int, y: int) -> Vector2:
	var board_pixel_width = board_width * cell_size + (board_width - 1) * padding
	var board_pixel_height = board_height * cell_size + (board_height - 1) * padding
	var offset_x = -board_pixel_width / 2.0 + cell_size / 2.0
	var offset_y = -board_pixel_height / 2.0 + cell_size / 2.0
	return Vector2(offset_x + x * (cell_size + padding), offset_y + y * (cell_size + padding))

func _update_cursor_highlight() -> void:
	var pos = _get_cell_position(cursor_cell.x, cursor_cell.y)
	cursor_highlight.position = pos - Vector2(2, 2)

func _update_selection_highlight() -> void:
	if selected_cell.x >= 0 && selected_cell.y >= 0:
		var pos = _get_cell_position(selected_cell.x, selected_cell.y)
		selection_highlight.position = pos - Vector2(2, 2)
		selection_highlight.visible = true
	else:
		selection_highlight.visible = false

func _cell_to_grid_coords(pos: Vector2) -> Vector2i:
	var board_pixel_width = board_width * cell_size + (board_width - 1) * padding
	var board_pixel_height = board_height * cell_size + (board_height - 1) * padding
	var offset_x = -board_pixel_width / 2.0 + cell_size / 2.0
	var offset_y = -board_pixel_height / 2.0 + cell_size / 2.0
	
	var local_x = pos.x - offset_x
	var local_y = pos.y - offset_y
	
	var x = int(local_x / (cell_size + padding))
	var y = int(local_y / (cell_size + padding))
	
	x = clamp(x, 0, board_width - 1)
	y = clamp(y, 0, board_height - 1)
	
	return Vector2i(x, y)

func _is_adjacent(a: Vector2i, b: Vector2i) -> bool:
	return (abs(a.x - b.x) == 1 && a.y == b.y) || (abs(a.y - b.y) == 1 && a.x == b.x)

func _on_mouse_click(event: InputEventMouseButton) -> void:
	if event.pressed && event.button_index == MOUSE_BUTTON_LEFT:
		var local_pos = to_local(event.global_position)
		var cell = _cell_to_grid_coords(local_pos)
		
		if selected_cell.x < 0:
			# First click - select this cell
			selected_cell = cell
			_update_selection_highlight()
		else:
			if cell == selected_cell:
				# Clicked same cell - deselect
				selected_cell = Vector2i(-1, -1)
				_update_selection_highlight()
			elif _is_adjacent(selected_cell, cell):
				# Adjacent cell - attempt swap
				_attempt_swap(selected_cell, cell)
			else:
				# Non-adjacent - transfer selection
				selected_cell = cell
				_update_selection_highlight()

func _attempt_swap(a: Vector2i, b: Vector2i) -> void:
	if is_processing_swap:
		return
	is_processing_swap = true
	
	var result = board_sim.try_swap(a.x, a.y, b.x, b.y)
	
	if result:
		# Successful swap - clear selection and refresh after a short delay
		selected_cell = Vector2i(-1, -1)
		_update_selection_highlight()
		call_deferred("_refresh_after_swap")
	else:
		# Failed swap - signal handler will flash
		selected_cell = Vector2i(-1, -1)
		_update_selection_highlight()
		is_processing_swap = false

func _refresh_after_swap() -> void:
	# Called deferred to let signals process
	refresh_board()
	_update_cursor_highlight()
	is_processing_swap = false

func _unhandled_input(event: InputEvent) -> void:
	if is_processing_swap:
		return
	
	# Keyboard navigation
	if event is InputEventKey && event.pressed:
		match event.keycode:
			KEY_UP, KEY_W:
				cursor_cell.y = max(0, cursor_cell.y - 1)
				_update_cursor_highlight()
			KEY_DOWN, KEY_S:
				cursor_cell.y = min(board_height - 1, cursor_cell.y + 1)
				_update_cursor_highlight()
			KEY_LEFT, KEY_A:
				cursor_cell.x = max(0, cursor_cell.x - 1)
				_update_cursor_highlight()
			KEY_RIGHT, KEY_D:
				cursor_cell.x = min(board_width - 1, cursor_cell.x + 1)
				_update_cursor_highlight()
			KEY_SPACE, KEY_ENTER:
				_handle_keyboard_select()
	
	# Mouse click handling
	if event is InputEventMouseButton:
		_on_mouse_click(event)

func _handle_keyboard_select() -> void:
	if selected_cell.x < 0:
		# Select current cursor cell
		selected_cell = cursor_cell
		_update_selection_highlight()
	else:
		if cursor_cell == selected_cell:
			# Deselect
			selected_cell = Vector2i(-1, -1)
			_update_selection_highlight()
		elif _is_adjacent(selected_cell, cursor_cell):
			# Attempt swap
			_attempt_swap(selected_cell, cursor_cell)
		else:
			# Move selection to cursor
			selected_cell = cursor_cell
			_update_selection_highlight()

func refresh_board() -> void:
	for y in range(board_height):
		for x in range(board_width):
			var idx = y * board_width + x
			var gem_instance = gem_instances[idx]
			var cell = board_sim.get_cell(x, y)
			
			if not cell.empty:
				if gem_instance == null || !is_instance_valid(gem_instance):
					gem_instance = gem_scene.instantiate()
					add_child(gem_instance)
					gem_instances[idx] = gem_instance
				
				gem_instance.set_gem(cell.kind, cell.has_echo)
				
				var pos = _get_cell_position(x, y)
				gem_instance.position = pos
			else:
				if gem_instance != null && is_instance_valid(gem_instance):
					gem_instance.queue_free()
					gem_instances[idx] = null
	
	# Re-apply rejection flash if active
	if rejection_flash_timer > 0:
		_apply_rejection_flash()

func _apply_rejection_flash() -> void:
	for cell in rejected_cells:
		var idx = cell.y * board_width + cell.x
		var gem = gem_instances[idx]
		if gem != null && is_instance_valid(gem):
			var flash_intensity = rejection_flash_timer / REJECTION_FLASH_DURATION
			gem.modulate = Color(1.0, 1.0 - flash_intensity * 0.8, 1.0 - flash_intensity * 0.8, 1.0)

func _process(delta: float) -> void:
	if rejection_flash_timer > 0:
		rejection_flash_timer -= delta
		if rejection_flash_timer <= 0:
			rejection_flash_timer = 0
			_clear_rejection_flash()
		else:
			_apply_rejection_flash()

func _clear_rejection_flash() -> void:
	for cell in rejected_cells:
		var idx = cell.y * board_width + cell.x
		var gem = gem_instances[idx]
		if gem != null && is_instance_valid(gem):
			gem.modulate = Color(1, 1, 1, 1)
	rejected_cells.clear()

# Signal handlers
func _on_match_resolved(cleared_cells: Array[Vector2i], gem_kind: int, cascade_depth: int) -> void:
	print("Match resolved: %d cells, kind %d, cascade %d" % [cleared_cells.size(), gem_kind, cascade_depth])

func _on_special_gem_created(pos: Vector2i, kind: int) -> void:
	var kind_names = {0: "Bolt", 1: "Prism", 2: "Nova"}
	print("Special gem created: %s at (%d, %d)" % [kind_names.get(kind, "Unknown"), pos.x, pos.y])

func _on_echo_charged(cells: Array[Vector2i]) -> void:
	print("Echo charged on %d cells" % [cells.size()])
	# Visual update will happen on refresh_board()

func _on_echo_detonated(cells: Array[Vector2i], multiplier: float) -> void:
	print("Echo detonated: %d cells, multiplier %.2f" % [cells.size(), multiplier])

func _on_move_rejected(ax: int, ay: int, bx: int, by: int) -> void:
	print("Move rejected: (%d, %d) -> (%d, %d)" % [ax, ay, bx, by])
	rejected_cells = [Vector2i(ax, ay), Vector2i(bx, by)]
	rejection_flash_timer = REJECTION_FLASH_DURATION

func _on_objective_progress(current: int, target: int) -> void:
	print("Objective progress: %d / %d" % [current, target])

func get_board_sim() -> RefCounted:
	return board_sim