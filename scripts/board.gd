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
var hover_cell: Vector2i = Vector2i(-1, -1)  # Mouse hover cell
var selection_highlight: Sprite2D
var cursor_highlight: Sprite2D
var hover_highlight: Sprite2D
var is_processing_swap: bool = false

# Animation state
var is_animating: bool = false
var animating_gems: Array[Node2D] = []

# Animation timing constants
const SWAP_ANIM_DURATION: float = 0.12
const CLEAR_ANIM_DURATION: float = 0.15
const FALL_ANIM_DURATION: float = 0.18
const BOUNCE_ANIM_DURATION: float = 0.1

# Debug HUD nodes
var debug_panel: Panel
var debug_vbox: VBoxContainer
var hover_label: Label
var selected_label: Label
var cursor_label: Label
var action_label: Label
var moves_label: Label
var score_label: Label
var combo_label: Label
var cascade_label: Label
var multiplier_label: Label
var coord_test_label: Label

# Rejection feedback state
var rejected_cells: Array[Vector2i] = []
var rejection_flash_timer: float = 0.0
const REJECTION_FLASH_DURATION = 0.3

# Debug / QA state
var total_moves: int = 0
var total_score: int = 0
var last_action_log: String = "Waiting for input..."
var last_swap_details: Dictionary = {}
var last_cascade_depth: int = 0
var last_multiplier: float = 1.0
var last_match_count: int = 0

# Coordinate self-test
var coord_test_results: Array[Dictionary] = []

signal debug_log_updated(message: String)

func _ready():
	_connect_signals()
	_create_highlights()
	_initialize_board()
	_create_debug_hud()
	_run_coord_self_test()

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
	for x in range(int(cell_size + 4)):
		for y in range(int(cell_size + 4)):
			if x < 2 || x >= int(cell_size + 2) || y < 2 || y >= int(cell_size + 2):
				sel_img.set_pixel(x, y, Color(1, 1, 1, 1))
	var sel_tex = ImageTexture.create_from_image(sel_img)
	selection_highlight.texture = sel_tex
	selection_highlight.texture_filter = 0
	selection_highlight.visible = false
	selection_highlight.z_index = 10
	add_child(selection_highlight)
	
	# Cursor highlight - subtle cyan border (keyboard)
	cursor_highlight = Sprite2D.new()
	var cur_img = Image.create(int(cell_size + 4), int(cell_size + 4), false, Image.FORMAT_RGBA8)
	cur_img.fill(Color(0, 0, 0, 0))
	for x in range(int(cell_size + 4)):
		for y in range(int(cell_size + 4)):
			if x < 1 || x >= int(cell_size + 3) || y < 1 || y >= int(cell_size + 3):
				cur_img.set_pixel(x, y, Color(0, 0.8, 1, 0.8))
	var cur_tex = ImageTexture.create_from_image(cur_img)
	cursor_highlight.texture = cur_tex
	cursor_highlight.texture_filter = 0
	cursor_highlight.visible = true
	cursor_highlight.z_index = 5
	add_child(cursor_highlight)
	
	# Hover highlight - subtle white/gray border (mouse)
	hover_highlight = Sprite2D.new()
	var hov_img = Image.create(int(cell_size + 4), int(cell_size + 4), false, Image.FORMAT_RGBA8)
	hov_img.fill(Color(0, 0, 0, 0))
	for x in range(int(cell_size + 4)):
		for y in range(int(cell_size + 4)):
			if x < 1 || x >= int(cell_size + 3) || y < 1 || y >= int(cell_size + 3):
				hov_img.set_pixel(x, y, Color(0.9, 0.9, 0.9, 0.6))
	var hov_tex = ImageTexture.create_from_image(hov_img)
	hover_highlight.texture = hov_tex
	hover_highlight.texture_filter = 0
	hover_highlight.visible = false
	hover_highlight.z_index = 6
	add_child(hover_highlight)
	
	_update_cursor_highlight()
	_create_debug_hud()

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

func _create_debug_hud() -> void:
	# Create debug HUD panel (Control) at top-right of viewport
	debug_panel = Panel.new()
	debug_panel.layout_mode = 3
	debug_panel.anchors_preset = 9  # Top-right
	debug_panel.anchor_right = 1.0
	debug_panel.anchor_top = 0.0
	debug_panel.offset_right = -20
	debug_panel.offset_top = 20
	debug_panel.offset_left = -320
	debug_panel.offset_bottom = -400
	debug_panel.add_theme_constant_override("panel_border_width", 2)
	debug_panel.add_theme_color_override("panel_bg", Color(0.05, 0.05, 0.08, 0.85))
	debug_panel.add_theme_color_override("border_color", Color(0.3, 0.3, 0.4, 1.0))
	add_child(debug_panel)
	
	# VBoxContainer for labels
	debug_vbox = VBoxContainer.new()
	debug_vbox.layout_mode = 3
	debug_vbox.anchors_preset = 15  # Full rect
	debug_vbox.offset_left = 10
	debug_vbox.offset_top = 10
	debug_vbox.offset_right = -10
	debug_vbox.offset_bottom = -10
	debug_vbox.add_theme_constant_override("separation", 4)
	debug_panel.add_child(debug_vbox)
	
	# Title
	var title = Label.new()
	title.text = "DEBUG HUD"
	title.add_theme_font_size_override("font_size", 16)
	title.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.custom_minimum_size = Vector2(0, 24)
	debug_vbox.add_child(title)
	
	# Separator
	var sep = HSeparator.new()
	debug_vbox.add_child(sep)
	
	# Hover label
	hover_label = Label.new()
	hover_label.text = "Hover: None"
	hover_label.add_theme_font_size_override("font_size", 14)
	hover_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	debug_vbox.add_child(hover_label)
	
	# Selected label
	selected_label = Label.new()
	selected_label.text = "Selected: None"
	selected_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(selected_label)
	
	# Cursor label
	cursor_label = Label.new()
	cursor_label.text = "Cursor: (0, 0)"
	cursor_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(cursor_label)
	
	# Action log
	action_label = Label.new()
	action_label.text = "Action: Waiting for input..."
	action_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(action_label)
	
	# Moves label
	moves_label = Label.new()
	moves_label.text = "Moves: 0"
	moves_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(moves_label)
	
	# Score label
	score_label = Label.new()
	score_label.text = "Score: 0"
	score_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(score_label)
	
	# Combo label
	combo_label = Label.new()
	combo_label.text = "Combo: 0"
	combo_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(combo_label)
	
	# Cascade label
	cascade_label = Label.new()
	cascade_label.text = "Last Cascade: -"
	cascade_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(cascade_label)
	
	# Multiplier label
	multiplier_label = Label.new()
	multiplier_label.text = "Multiplier: 1.00x"
	multiplier_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(multiplier_label)
	
	# Coord test label
	coord_test_label = Label.new()
	coord_test_label.text = "Coord Test: Not run"
	coord_test_label.add_theme_font_size_override("font_size", 14)
	debug_vbox.add_child(coord_test_label)

func _get_cell_position(x: int, y: int) -> Vector2:
	var board_pixel_width = board_width * cell_size + (board_width - 1) * padding
	var board_pixel_height = board_height * cell_size + (board_height - 1) * padding
	var offset_x = -board_pixel_width / 2.0 + cell_size / 2.0
	var offset_y = -board_pixel_height / 2.0 + cell_size / 2.0
	return Vector2(offset_x + x * (cell_size + padding), offset_y + y * (cell_size + padding))

func _update_cursor_highlight() -> void:
	var pos = _get_cell_position(cursor_cell.x, cursor_cell.y)
	cursor_highlight.position = pos - Vector2(2, 2)

func _update_hover_highlight() -> void:
	if hover_cell.x >= 0 && hover_cell.y >= 0:
		var pos = _get_cell_position(hover_cell.x, hover_cell.y)
		hover_highlight.position = pos - Vector2(2, 2)
		hover_highlight.visible = true
	else:
		hover_highlight.visible = false

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

	var stride: float = cell_size + padding
	var x = int(floor(local_x / stride))
	var y = int(floor(local_y / stride))

	if x >= 0 && x < board_width && y >= 0 && y < board_height:
		return Vector2i(x, y)
	return Vector2i(-1, -1)

func _is_adjacent(a: Vector2i, b: Vector2i) -> bool:
	return (abs(a.x - b.x) == 1 && a.y == b.y) || (abs(a.y - b.y) == 1 && a.x == b.x)

func _on_mouse_click(event: InputEventMouseButton) -> void:
	if event.pressed && event.button_index == MOUSE_BUTTON_LEFT:
		if is_animating:
			return
		# Use get_local_mouse_position() for accurate local coordinates
		var local_pos = get_local_mouse_position()
		var cell = _cell_to_grid_coords(local_pos)
		
		# Update hover to clicked cell for visual feedback
		hover_cell = cell
		_update_hover_highlight()
		
		if selected_cell.x < 0:
			# First click - select this cell
			selected_cell = cell
			_update_selection_highlight()
			_update_debug_log("Selected cell (%d, %d)" % [cell.x, cell.y])
		else:
			if cell == selected_cell:
				# Clicked same cell - deselect
				selected_cell = Vector2i(-1, -1)
				_update_selection_highlight()
				_update_debug_log("Deselected cell (%d, %d)" % [cell.x, cell.y])
			elif _is_adjacent(selected_cell, cell):
				# Adjacent cell - attempt swap
				_update_debug_log("Swap attempted: (%d, %d) <-> (%d, %d)" % [selected_cell.x, selected_cell.y, cell.x, cell.y])
				_attempt_swap(selected_cell, cell)
			else:
				# Non-adjacent - transfer selection
				selected_cell = cell
				_update_selection_highlight()
				_update_debug_log("Selection moved to (%d, %d)" % [cell.x, cell.y])

func _attempt_swap(a: Vector2i, b: Vector2i) -> void:
	if is_processing_swap || is_animating:
		return
	is_processing_swap = true
	
	# Store swap details for debug log
	last_swap_details = {"a": a, "b": b}
	last_match_count = 0
	last_cascade_depth = 0
	last_multiplier = 1.0
	
	var result = board_sim.try_swap(a.x, a.y, b.x, b.y)
	
	if result:
		total_moves += 1
		# Successful swap - animate the swap
		selected_cell = Vector2i(-1, -1)
		_update_selection_highlight()
		_animate_swap(a, b)
	else:
		# Failed swap - animate rejection snap-back
		selected_cell = Vector2i(-1, -1)
		_update_selection_highlight()
		_animate_rejection_snapback(a, b)

func _animate_swap(a: Vector2i, b: Vector2i) -> void:
	is_animating = true
	is_processing_swap = false
	
	var gem_a = _get_gem_instance(a.x, a.y)
	var gem_b = _get_gem_instance(b.x, b.y)
	if gem_a == null || gem_b == null:
		# Fallback to instant refresh
		call_deferred("_sync_board_state")
		return
	
	var pos_a = _get_cell_position(a.x, a.y)
	var pos_b = _get_cell_position(b.x, b.y)
	
	# Store original positions for potential snap-back
	animating_gems = [gem_a, gem_b]
	
	# Animate gem A to B's position
	var tween_a = create_tween()
	tween_a.set_parallel(true)
	tween_a.tween_property(gem_a, "position", pos_b, SWAP_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	
	# Animate gem B to A's position
	var tween_b = create_tween()
	tween_b.set_parallel(true)
	tween_b.tween_property(gem_b, "position", pos_a, SWAP_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	
	# Wait for swap animation to complete, then process matches
	await tween_a.finished
	
	# After swap animation, process the match signals that were already emitted
	# The board_sim already updated, now we need to animate the clear
	_process_match_sequence()

func _animate_rejection_snapback(a: Vector2i, b: Vector2i) -> void:
	is_animating = true
	is_processing_swap = false
	
	var gem_a = _get_gem_instance(a.x, a.y)
	var gem_b = _get_gem_instance(b.x, b.y)
	if gem_a == null || gem_b == null:
		is_animating = false
		return
	
	var pos_a = _get_cell_position(a.x, a.y)
	var pos_b = _get_cell_position(b.x, b.y)
	
	animating_gems = [gem_a, gem_b]
	
	# Animate both gems to each other's positions and back (snap-back)
	var tween_a = create_tween()
	tween_a.set_parallel(true)
	tween_a.tween_property(gem_a, "position", pos_b, SWAP_ANIM_DURATION * 0.5).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween_a.tween_property(gem_a, "position", pos_a, SWAP_ANIM_DURATION * 0.5).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
	
	var tween_b = create_tween()
	tween_b.set_parallel(true)
	tween_b.tween_property(gem_b, "position", pos_a, SWAP_ANIM_DURATION * 0.5).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween_b.tween_property(gem_b, "position", pos_b, SWAP_ANIM_DURATION * 0.5).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
	
	# Add red flash during rejection
	_rejection_flash([gem_a, gem_b])
	
	await tween_a.finished
	is_animating = false

func _process_match_sequence() -> void:
	# This will be called after swap animation completes
	# The match signals from board_sim have already been emitted
	# We need to animate the clear and fall
	# The signals (_on_match_resolved, etc.) will handle the animation
	pass

func _get_gem_instance(x: int, y: int) -> Node2D:
	var idx = y * board_width + x
	if idx >= 0 && idx < gem_instances.size():
		return gem_instances[idx]
	return null

func _rejection_flash(gems: Array[Node2D]) -> void:
	for gem in gems:
		if gem == null || !is_instance_valid(gem):
			continue
		var tween = create_tween()
		tween.set_parallel(true)
		tween.tween_property(gem, "modulate", Color(1.0, 0.2, 0.2, 1.0), 0.05)
		tween.tween_property(gem, "modulate", Color(1.0, 1.0, 1.0, 1.0), 0.1)

func _unhandled_input(event: InputEvent) -> void:
	if is_processing_swap || is_animating:
		return
	
	# Mouse motion - update hover cell
	if event is InputEventMouseMotion:
		var local_pos = get_local_mouse_position()
		var cell = _cell_to_grid_coords(local_pos)
		if cell != hover_cell:
			hover_cell = cell
			_update_hover_highlight()
	
	# Keyboard navigation
	if event is InputEventKey && event.pressed:
		match event.keycode:
			KEY_UP, KEY_W:
				cursor_cell.y = max(0, cursor_cell.y - 1)
				_update_cursor_highlight()
				_update_debug_log("Cursor moved to (%d, %d)" % [cursor_cell.x, cursor_cell.y])
			KEY_DOWN, KEY_S:
				cursor_cell.y = min(board_height - 1, cursor_cell.y + 1)
				_update_cursor_highlight()
				_update_debug_log("Cursor moved to (%d, %d)" % [cursor_cell.x, cursor_cell.y])
			KEY_LEFT, KEY_A:
				cursor_cell.x = max(0, cursor_cell.x - 1)
				_update_cursor_highlight()
				_update_debug_log("Cursor moved to (%d, %d)" % [cursor_cell.x, cursor_cell.y])
			KEY_RIGHT, KEY_D:
				cursor_cell.x = min(board_width - 1, cursor_cell.x + 1)
				_update_cursor_highlight()
				_update_debug_log("Cursor moved to (%d, %d)" % [cursor_cell.x, cursor_cell.y])
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
		_update_debug_log("Keyboard selected (%d, %d)" % [cursor_cell.x, cursor_cell.y])
	else:
		if cursor_cell == selected_cell:
			# Deselect
			selected_cell = Vector2i(-1, -1)
			_update_selection_highlight()
			_update_debug_log("Keyboard deselected")
		elif _is_adjacent(selected_cell, cursor_cell):
			# Attempt swap
			_update_debug_log("Keyboard swap: (%d, %d) <-> (%d, %d)" % [selected_cell.x, selected_cell.y, cursor_cell.x, cursor_cell.y])
			_attempt_swap(selected_cell, cursor_cell)
		else:
			# Move selection to cursor
			selected_cell = cursor_cell
			_update_selection_highlight()
			_update_debug_log("Keyboard selection moved to (%d, %d)" % [cursor_cell.x, cursor_cell.y])

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
	
	_update_debug_hud()

func _clear_rejection_flash() -> void:
	for cell in rejected_cells:
		var idx = cell.y * board_width + cell.x
		var gem = gem_instances[idx]
		if gem != null && is_instance_valid(gem):
			gem.modulate = Color(1, 1, 1, 1)
	rejected_cells.clear()

func _animate_clear(cleared_cells: Array[Vector2i], gem_kind: int) -> void:
	is_animating = true
	var animating = []
	
	for cell in cleared_cells:
		var gem = _get_gem_instance(cell.x, cell.y)
		if gem != null && is_instance_valid(gem):
			animating.append(gem)
			
			# Spawn particle burst for this gem
			_spawn_clear_particles(gem, gem_kind)
			
			# Animate scale down and fade out
			var tween = create_tween()
			tween.set_parallel(true)
			tween.tween_property(gem, "scale", Vector2(0, 0), CLEAR_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
			tween.tween_property(gem, "modulate:a", 0.0, CLEAR_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
			tween.finished.connect(_on_gem_clear_finished.bind(gem))
			animating_gems.append(gem)
	
	if animating.is_empty():
		is_animating = false
		_after_clear_complete()
func _on_gem_clear_finished(gem: Node2D) -> void:
	if gem != null && is_instance_valid(gem):
		gem.queue_free()
	animating_gems.erase(gem)
	if animating_gems.is_empty():
		is_animating = false
		_after_clear_complete()

func _after_clear_complete() -> void:
	# After clear, animate gravity fall for new gems
	_animate_gravity_fall()

func _animate_gravity_fall() -> void:
	is_animating = true
	
	# For each column, find gems that need to fall
	var any_falling = false
	var max_fall_time = 0.0
	
	for x in range(board_width):
		var fall_distance = 0
		for y in range(board_height - 1, -1, -1):
			var idx = y * board_width + x
			var cell = board_sim.get_cell(x, y)
			var gem = gem_instances[idx]
			
			if cell.empty && gem != null && is_instance_valid(gem):
				# This gem should fall - find how far
				var target_y = y
				while target_y + 1 < board_height && board_sim.get_cell(x, target_y + 1).empty:
					target_y += 1
				
				if target_y > y:
					fall_distance = target_y - y
					var target_pos = _get_cell_position(x, target_y)
					var fall_duration = FALL_ANIM_DURATION * fall_distance
					max_fall_time = max(max_fall_time, fall_duration)
					
					var tween = create_tween()
					tween.tween_property(gem, "position", target_pos, fall_duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
					# Add subtle bounce at the end
					tween.tween_property(gem, "scale", Vector2(1.1, 0.9), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
					tween.tween_property(gem, "scale", Vector2(1.0, 1.0), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
					
					any_falling = true
					
					# Update gem_instances array
					gem_instances[target_y * board_width + x] = gem
					gem_instances[y * board_width + x] = null
	
	if any_falling:
		# Wait for longest fall to complete
		var timer = Timer.new()
		timer.one_shot = true
		timer.wait_time = max_fall_time + BOUNCE_ANIM_DURATION
		timer.timeout.connect(_after_fall_complete)
		add_child(timer)
		timer.start()
	else:
		is_animating = false
		_after_fall_complete()

func _after_fall_complete() -> void:
	# After gravity fall, spawn new gems at top and animate them falling in
	_animate_new_gems_spawn()

func _animate_new_gems_spawn() -> void:
	# Spawn new gems at top of board and animate them falling in
	var any_new = false
	var max_fall_time = 0.0
	
	for x in range(board_width):
		for y in range(board_height):
			var idx = y * board_width + x
			var cell = board_sim.get_cell(x, y)
			var gem = gem_instances[idx]
			
			if not cell.empty && (gem == null || !is_instance_valid(gem)):
				# Need to spawn new gem
				var kind = cell.kind
				var has_echo = cell.has_echo
				
				var gem_instance = gem_scene.instantiate()
				gem_instance.set_gem(kind, has_echo)
				
				# Start above the board
				var start_pos = _get_cell_position(x, -1)
				var end_pos = _get_cell_position(x, y)
				gem_instance.position = start_pos
				
				add_child(gem_instance)
				gem_instances[idx] = gem_instance
				
				var fall_duration = FALL_ANIM_DURATION * (y + 1)
				max_fall_time = max(max_fall_time, fall_duration)
				
				var tween = create_tween()
				tween.tween_property(gem_instance, "position", end_pos, fall_duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
				# Add bounce
				tween.tween_property(gem_instance, "scale", Vector2(1.1, 0.9), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
				tween.tween_property(gem_instance, "scale", Vector2(1.0, 1.0), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
				
				any_new = true
	
	if any_new:
		var timer = Timer.new()
		timer.one_shot = true
		timer.wait_time = max_fall_time + BOUNCE_ANIM_DURATION
		timer.timeout.connect(_check_for_new_matches)
		add_child(timer)
		timer.start()
	else:
		is_animating = false
		_check_for_new_matches()

func _check_for_new_matches() -> void:
	# After all animations complete, check if there are new matches (cascade)
	# The board_sim will have already processed this via try_swap cascade loop
	# We just need to refresh the visual state
	refresh_board()
	_update_cursor_highlight()
	_update_hover_highlight()
	is_animating = false

func _sync_board_state() -> void:
	# Sync visual board state with board_sim after animations
	refresh_board()
	_update_cursor_highlight()
	_update_hover_highlight()
	is_animating = false
	is_processing_swap = false

# Signal handlers for match resolution and effects
func _on_match_resolved(cleared_cells: Array[Vector2i], gem_kind: int, cascade_depth: int) -> void:
	last_match_count += cleared_cells.size()
	last_cascade_depth = max(last_cascade_depth, cascade_depth)
	print("Match resolved: %d cells, kind %d, cascade %d" % [cleared_cells.size(), gem_kind, cascade_depth])
	
	# Animate the cleared cells
	_animate_clear(cleared_cells, gem_kind)

func _on_special_gem_created(pos: Vector2i, kind: int) -> void:
	var kind_names = {0: "Bolt", 1: "Prism", 2: "Nova"}
	print("Special gem created: %s at (%d, %d)" % [kind_names.get(kind, "Unknown"), pos.x, pos.y])
	
	# Spawn special gem creation effect
	_spawn_special_gem_effect(pos, kind)

func _spawn_special_gem_effect(pos: Vector2i, kind: int) -> void:
	var gem = _get_gem_instance(pos.x, pos.y)
	if gem == null || !is_instance_valid(gem):
		return
	
	# Pulse effect for special gem creation
	var tween = create_tween()
	tween.set_parallel(true)
	tween.tween_property(gem, "scale", Vector2(1.3, 1.3), 0.15).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.tween_property(gem, "scale", Vector2(1.0, 1.0), 0.15).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
	
	# Color flash based on kind
	var colors = {0: Color(1, 1, 0, 1), 1: Color(1, 0.5, 0, 1), 2: Color(1, 0, 1, 1)}  # Bolt=Yellow, Prism=Orange, Nova=Magenta
	var flash_color = colors.get(kind, Color(1, 1, 1, 1))
	
	var color_tween = create_tween()
	color_tween.set_parallel(true)
	color_tween.tween_property(gem, "modulate", flash_color, 0.1)
	color_tween.tween_property(gem, "modulate", Color(1, 1, 1, 1), 0.2)

func _on_echo_charged(cells: Array[Vector2i]) -> void:
	print("Echo charged on %d cells" % [cells.size()])
	# Visual update will happen on refresh_board()

func _on_echo_detonated(cells: Array[Vector2i], multiplier: float) -> void:
	last_multiplier = multiplier
	print("Echo detonated: %d cells, multiplier %.2f" % [cells.size(), multiplier])
	
	# Spawn echo detonation effect - the signature visual payoff
	_spawn_echo_detonation(cells)

func _spawn_echo_detonation(cells: Array[Vector2i]) -> void:
	# The signature visual payoff - large particle explosion / shockwave
	for cell in cells:
		var gem = _get_gem_instance(cell.x, cell.y)
		if gem == null || !is_instance_valid(gem):
			continue
		
		var center_pos = gem.global_position
		_spawn_echo_particles(center_pos)
		
		# Shockwave ring animation
		var ring = Sprite2D.new()
		var ring_img = Image.create(int(cell_size * 3), int(cell_size * 3), false, Image.FORMAT_RGBA8)
		ring_img.fill(Color(0, 0, 0, 0))
		var center = Vector2(cell_size * 1.5, cell_size * 1.5)
		for y in range(int(cell_size * 3)):
			for x in range(int(cell_size * 3)):
				var pos = Vector2(x, y)
				var dist = pos.distance_to(center)
				var inner = cell_size * 0.8
				var outer = cell_size * 1.2
				if dist >= inner && dist <= outer:
					var alpha = 1.0 - (dist - inner) / (outer - inner)
					ring_img.set_pixel(x, y, Color(1.0, 0.9, 0.3, alpha))
		var ring_tex = ImageTexture.create_from_image(ring_img)
		ring.texture = ring_tex
		ring.position = center_pos
		ring.z_index = 20
		add_child(ring)
		
		# Animate ring expanding and fading
		var tween = create_tween()
		tween.set_parallel(true)
		tween.tween_property(ring, "scale", Vector2(2.0, 2.0), 0.3).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
		tween.tween_property(ring, "modulate:a", 0.0, 0.3).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
		tween.finished.connect(ring.queue_free.bind())

func _on_move_rejected(ax: int, ay: int, bx: int, by: int) -> void:
	print("Move rejected: (%d, %d) -> (%d, %d)" % [ax, ay, bx, by])
	rejected_cells = [Vector2i(ax, ay), Vector2i(bx, by)]
	rejection_flash_timer = REJECTION_FLASH_DURATION
	
	var reason = "Unknown"
	if ax < 0 || ay < 0 || bx < 0 || by < 0 || ax >= board_width || ay >= board_height || bx >= board_width || by >= board_height:
		reason = "Out of bounds"
	elif !_is_adjacent(Vector2i(ax, ay), Vector2i(bx, by)):
		reason = "Non-adjacent"
	else:
		reason = "No match created"
	
	_update_debug_log("REJECTED: %s at (%d, %d) <-> (%d, %d)" % [reason, ax, ay, bx, by])

func _on_objective_progress(current: int, target: int) -> void:
	print("Objective progress: %d / %d" % [current, target])

func _update_debug_log(msg: String) -> void:
	last_action_log = msg
	emit_signal("debug_log_updated", msg)

func _run_coord_self_test() -> void:
	coord_test_results.clear()
	
	# Test positions: top-left, center, bottom-right of specific cells
	var test_cells = [
		Vector2i(0, 0),      # Top-left corner
		Vector2i(3, 3),      # Center-ish
		Vector2i(7, 7),      # Bottom-right
	]
	
	for cell in test_cells:
		# _get_cell_position returns TOP-LEFT of cell, not center
		var pos_tl = _get_cell_position(cell.x, cell.y)
		var pos = pos_tl + Vector2(cell_size / 2.0, cell_size / 2.0)  # actual center
		
		# Test center of cell
		var resolved = _cell_to_grid_coords(pos)
		var passed = resolved == cell
		
		# Test point 20% from top-left (well inside cell)
		var inset = cell_size * 0.2
		var tl_pos = pos_tl + Vector2(inset, inset)
		var tl_resolved = _cell_to_grid_coords(tl_pos)
		var tl_passed = tl_resolved == cell
		
		# Test point 20% from bottom-right (well inside cell)
		var br_pos = pos_tl + Vector2(cell_size - inset, cell_size - inset)
		var br_resolved = _cell_to_grid_coords(br_pos)
		var br_passed = br_resolved == cell
		
		var result = {
			"cell": cell,
			"center": {"resolved": resolved, "passed": passed},
			"inner_top_left": {"resolved": tl_resolved, "passed": tl_passed},
			"inner_bottom_right": {"resolved": br_resolved, "passed": br_passed},
		}
		coord_test_results.append(result)
		
		print("COORD TEST cell (%d, %d): center=%s innerTL=%s innerBR=%s" % [
			cell.x, cell.y,
			"PASS" if passed else "FAIL",
			"PASS" if tl_passed else "FAIL",
			"PASS" if br_passed else "FAIL"
		])
	
	var all_passed = true
	for r in coord_test_results:
		if !r.center.passed || !r.inner_top_left.passed || !r.inner_bottom_right.passed:
			all_passed = false
	
	print("COORD SELF-TEST: %s" % ["ALL PASSED" if all_passed else "SOME FAILED"])

func _update_debug_hud() -> void:
	if debug_panel == null:
		return
	
	# Hover cell
	var hover = hover_cell
	if hover.x >= 0:
		var cell = board_sim.get_cell(hover.x, hover.y)
		var kind_name = _gem_kind_to_string(cell.kind) if not cell.empty else "Empty"
		hover_label.text = "Hover: (%d, %d) - %s" % [hover.x, hover.y, kind_name]
		hover_label.add_theme_color_override("font_color", Color(0.9, 0.9, 0.9))
	else:
		hover_label.text = "Hover: None"
		hover_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	
	# Selected cell
	var selected = selected_cell
	if selected.x >= 0:
		var cell = board_sim.get_cell(selected.x, selected.y)
		var kind_name = _gem_kind_to_string(cell.kind) if not cell.empty else "Empty"
		selected_label.text = "Selected: (%d, %d) - %s" % [selected.x, selected.y, kind_name]
		selected_label.add_theme_color_override("font_color", Color(1, 1, 1))
	else:
		selected_label.text = "Selected: None"
		selected_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	
	# Keyboard cursor
	var cursor = cursor_cell
	cursor_label.text = "Cursor: (%d, %d)" % [cursor.x, cursor.y]
	
	# Action log
	var action = last_action_log
	action_label.text = "Action: %s" % action
	if action.begins_with("REJECTED"):
		action_label.add_theme_color_override("font_color", Color(1, 0.4, 0.4))
	elif action.begins_with("Swap") || action.begins_with("Keyboard swap"):
		action_label.add_theme_color_override("font_color", Color(0.6, 1, 0.6))
	else:
		action_label.add_theme_color_override("font_color", Color(0.9, 0.9, 0.9))
	
	# Stats
	moves_label.text = "Moves: %d" % total_moves
	score_label.text = "Score: %d" % total_score
	
	# Get board sim for combo/multiplier
	var sim = board_sim
	if sim != null:
		combo_label.text = "Combo: %d" % sim.get_combo()
		multiplier_label.text = "Multiplier: %.2fx" % sim.get_resonance_multiplier()
	
	# Cascade depth from last swap
	if last_cascade_depth > 0:
		cascade_label.text = "Last Cascade: %d" % last_cascade_depth
	else:
		cascade_label.text = "Last Cascade: -"
	
	# Coordinate self-test results
	var results = coord_test_results
	if results.size() > 0:
		var passed = 0
		var total = 0
		for r in results:
			total += 3
			if r.center.passed: passed += 1
			if r.inner_top_left.passed: passed += 1
			if r.inner_bottom_right.passed: passed += 1
		
		if passed == total:
			coord_test_label.text = "Coord Test: %d/%d PASSED" % [passed, total]
			coord_test_label.add_theme_color_override("font_color", Color(0.4, 1, 0.4))
		else:
			coord_test_label.text = "Coord Test: %d/%d FAILED" % [passed, total]
			coord_test_label.add_theme_color_override("font_color", Color(1, 0.4, 0.4))
	else:
		coord_test_label.text = "Coord Test: Not run"

func _gem_kind_to_string(kind: int) -> String:
	match kind:
		0: return "Circle (Cyan)"
		1: return "Triangle (Yellow)"
		2: return "Square (Green)"
		3: return "Diamond (Magenta)"
		4: return "Star"
		5: return "Cross"
		_: return "Unknown (%d)" % kind

# Public getters for debug HUD
func get_hover_cell() -> Vector2i:
	return hover_cell

func get_selected_cell() -> Vector2i:
	return selected_cell

func get_cursor_cell() -> Vector2i:
	return cursor_cell

func get_last_action_log() -> String:
	return last_action_log

func get_total_moves() -> int:
	return total_moves

func get_total_score() -> int:
	return total_score

func get_coord_test_results() -> Array[Dictionary]:
	return coord_test_results

func get_board_sim() -> RefCounted:
	return board_sim

func _spawn_clear_particles(gem: Node2D, gem_kind: int) -> void:
	# Create a small particle burst for gem clearing
	# Skip GPUParticles2D for now - use simple visual effect instead
	var flash = Sprite2D.new()
	var flash_img = Image.create(int(cell_size), int(cell_size), false, Image.FORMAT_RGBA8)
	flash_img.fill(Color(0, 0, 0, 0))
	var center = Vector2(cell_size / 2.0, cell_size / 2.0)
	for y in range(int(cell_size)):
		for x in range(int(cell_size)):
			var pos = Vector2(x, y)
			var dist = pos.distance_to(center)
			if dist <= cell_size * 0.4:
				var alpha = 1.0 - dist / (cell_size * 0.4)
				var colors = [
					Color(0.0, 0.8, 0.8, 1.0),    # Cyan - Circle
					Color(0.9, 0.8, 0.1, 1.0),    # Yellow - Triangle
					Color(0.2, 0.8, 0.2, 1.0),    # Green - Square
					Color(0.9, 0.2, 0.9, 1.0),    # Magenta - Diamond
				]
				var color = colors[gem_kind % colors.size()]
				flash_img.set_pixel(x, y, Color(color.r, color.g, color.b, alpha * 0.8))
	var flash_tex = ImageTexture.create_from_image(flash_img)
	flash.texture = flash_tex
	flash.global_position = gem.global_position
	flash.z_index = 15
	add_child(flash)
	
	# Animate flash expanding and fading
	var tween = create_tween()
	tween.set_parallel(true)
	tween.tween_property(flash, "scale", Vector2(2.0, 2.0), 0.2).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.tween_property(flash, "modulate:a", 0.0, 0.2).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.finished.connect(flash.queue_free.bind())

func _spawn_echo_particles(pos: Vector2) -> void:
	# Small echo particles for echo detonation - use simple flash effect
	var flash = Sprite2D.new()
	var flash_img = Image.create(int(cell_size * 3), int(cell_size * 3), false, Image.FORMAT_RGBA8)
	flash_img.fill(Color(0, 0, 0, 0))
	var center = Vector2(cell_size * 1.5, cell_size * 1.5)
	for y in range(int(cell_size * 3)):
		for x in range(int(cell_size * 3)):
			var pos2 = Vector2(x, y)
			var dist = pos2.distance_to(center)
			if dist <= cell_size * 1.2 && dist >= cell_size * 0.8:
				var alpha = 1.0 - (dist - cell_size * 0.8) / (cell_size * 0.4)
				flash_img.set_pixel(x, y, Color(1.0, 0.9, 0.3, alpha * 0.8))
	var flash_tex = ImageTexture.create_from_image(flash_img)
	flash.texture = flash_tex
	flash.position = pos
	flash.z_index = 15
	add_child(flash)
	
	# Animate flash expanding and fading
	var tween = create_tween()
	tween.set_parallel(true)
	tween.tween_property(flash, "scale", Vector2(2.0, 2.0), 0.3).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.tween_property(flash, "modulate:a", 0.0, 0.3).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.finished.connect(flash.queue_free.bind())