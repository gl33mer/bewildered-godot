extends Node2D

class_name GameBoard

@export var board_width: int = 8
@export var board_height: int = 8
@export var seed: int = 12345
@export var cell_size: float = 64.0
@export var padding: float = 6.0

@onready var board_sim: RefCounted = BoardSim.new()
@onready var gem_scene: PackedScene = preload("res://scenes/gem.tscn")
@onready var audio_manager: AudioManager = AudioManager

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

# Buffers for the synchronous cascade signal burst emitted by board_sim
# during try_swap (all cascade depths arrive in the same frame). We accumulate
# them here and run a single controlled clear/fall/spawn sequence afterwards.
var _pending_cascade_clears: Array = []

# Animation timing constants
const SWAP_ANIM_DURATION: float = 0.12
const CLEAR_ANIM_DURATION: float = 0.15
const FALL_ANIM_DURATION: float = 0.18
const BOUNCE_ANIM_DURATION: float = 0.1

# Gravity Tumbler + cascade pacing constants
const ROTATE_ANIM_DURATION: float = 0.25
const CASCADE_STEP_PAUSE: float = 0.1

# Debug HUD nodes.
# The panel lives on a CanvasLayer added to the scene ROOT — never under this
# rotating Board node — so UI stays perfectly static while the Tumbler spins
# (QA Issue A).
var debug_layer: CanvasLayer
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
signal level_cleared(level_id: String)
signal level_failed(level_id: String)
# Emitted when the grid is resized (dev grid-size switcher) so the HUD can
# re-center and re-scale the board to the new dimensions.
signal board_resized

@export var current_level_index: int = 1

var current_level_id: String = "campaign-001"
var _level_result_handled: bool = false

func _ready():
	_connect_signals()
	_create_highlights()
	_load_level(current_level_index)
	_create_debug_hud()
	_run_coord_self_test()

func _load_level(index: int) -> void:
	current_level_id = "campaign-%03d" % index
	var res_path := "res://levels/%s.ron" % current_level_id
	# Read via FileAccess: on Android the .ron files live inside the APK pck
	# and are invisible to OS-level file APIs.
	var ok: bool = false
	if FileAccess.file_exists(res_path):
		var f := FileAccess.open(res_path, FileAccess.READ)
		if f:
			ok = board_sim.load_level_from_ron(f.get_as_text())
	if not ok:
		# Desktop fallback: direct OS path (dev checkouts outside the pck).
		ok = board_sim.load_level_file(ProjectSettings.globalize_path(res_path))
	if not ok:
		push_warning("[bewildered] Failed to load level %s: %s" % [current_level_id, board_sim.get_last_error()])
	# Always trust the sim's actual grid dimensions (levels can differ from the
	# exported defaults, and a dev grid resize must not leak into the next
	# campaign level). Keeps board_width/board_height + gem layout consistent.
	board_width = int(board_sim.get_width())
	board_height = int(board_sim.get_height())
	selected_cell = Vector2i(-1, -1)
	cursor_cell = Vector2i(0, 0)
	hover_cell = Vector2i(-1, -1)
	reset_game_stats()
	_level_result_handled = false
	# Reset the gem instance array for the (re)loaded level's grid.
	for gem in gem_instances:
		if gem != null && is_instance_valid(gem):
			gem.queue_free()
	gem_instances = []
	gem_instances.resize(board_width * board_height)
	_sync_board_state()

func load_next_level() -> void:
	current_level_index += 1
	_load_level(current_level_index)

func retry_level() -> void:
	_load_level(current_level_index)

# Dev/QA grid-size switcher: re-init the sandbox board with new dimensions via
# BoardSim.new_board (a fresh match-free board, no objective). Rebuilds the gem
# instances, resets input/animation state and rotation, then asks the HUD to
# re-center and re-scale the board to fit.
func set_grid_size(w: int, h: int) -> void:
	if w < 4 || h < 4:
		push_warning("[bewildered] Refusing tiny grid %d x %d" % [w, h])
		return
	rotation_degrees = 0.0
	board_sim.new_board(w, h, seed)
	board_width = w
	board_height = h
	selected_cell = Vector2i(-1, -1)
	hover_cell = Vector2i(-1, -1)
	cursor_cell = Vector2i(0, 0)
	reset_game_stats()
	is_animating = false
	is_processing_swap = false
	_pending_cascade_clears.clear()
	# Dev boards have no objective; suppress the level-win/fail outcome popups.
	_level_result_handled = true
	for gem in gem_instances:
		if gem != null && is_instance_valid(gem):
			gem.queue_free()
	gem_instances = []
	gem_instances.resize(w * h)
	_update_selection_highlight()
	_update_cursor_highlight()
	_update_hover_highlight()
	_sync_board_state()
	emit_signal("board_resized")
	_update_debug_log("Grid resized to %d x %d (dev sandbox)" % [w, h])

func reset_game_stats() -> void:
	total_moves = 0
	total_score = 0
	last_match_count = 0
	last_cascade_depth = 0
	last_multiplier = 1.0

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

func _initialize_board() -> void:
	# Create gems first, then connect signals, then initialize board
	# This ensures signals are ready before any cascades fire
	
	for gem in gem_instances:
		if gem != null && is_instance_valid(gem):
			gem.queue_free()
	gem_instances.clear()
	gem_instances.resize(board_width * board_height)
	
	# Initialize board_sim (may fire cascade signals immediately)
	board_sim.new_board(board_width, board_height, seed)
	
	# Sync visual board state with simulation
	_sync_board_state()
	
	_update_cursor_highlight()

func _create_debug_hud() -> void:
	if debug_panel != null:
		return  # Already built (guard: _ready + _create_highlights both call this)
	# QA Issue A fix: parent the debug HUD to a root-level CanvasLayer so it is
	# completely independent of the rotating Board Node2D.
	debug_layer = CanvasLayer.new()
	debug_layer.name = "DebugHUDLayer"
	debug_layer.layer = 20
	get_tree().root.add_child.call_deferred(debug_layer)
	# Create debug HUD panel (Control) at top-right of viewport
	debug_panel = Panel.new()
	debug_panel.layout_mode = 3
	# Pin to the top-right of the viewport: both left/right anchors at 1.0 so
	# offset_left/-right measure inward from the right edge.
	debug_panel.anchor_left = 1.0
	debug_panel.anchor_right = 1.0
	debug_panel.anchor_top = 0.0
	debug_panel.anchor_bottom = 0.0
	debug_panel.offset_left = -340
	debug_panel.offset_right = -20
	debug_panel.offset_top = 70
	debug_panel.offset_bottom = 420
	debug_panel.add_theme_constant_override("panel_border_width", 2)
	debug_panel.add_theme_color_override("panel_bg", Color(0.05, 0.05, 0.08, 0.85))
	debug_panel.add_theme_color_override("border_color", Color(0.3, 0.3, 0.4, 1.0))
	# Attach to the static root CanvasLayer, NOT to this rotating Board node.
	debug_layer.add_child(debug_panel)
	
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
	# The grid box is centered EXACTLY about local (0,0) so the Board node origin
	# is the dead-center pivot: static placement and the 90° Tumbler rotation
	# both revolve around the visual center, and mouse coords map 1:1.
	var offset_x = -board_pixel_width / 2.0
	var offset_y = -board_pixel_height / 2.0
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
	var offset_x = -board_pixel_width / 2.0
	var offset_y = -board_pixel_height / 2.0

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
	
	# Reset the cascade-signal buffer before board_sim synchronously bursts
	# match_resolved signals for every cascade depth of this move.
	_pending_cascade_clears.clear()
	
	# Store swap details for debug log
	last_swap_details = {"a": a, "b": b}
	last_match_count = 0
	last_cascade_depth = 0
	last_multiplier = 1.0
	
	var result = board_sim.try_swap(a.x, a.y, b.x, b.y)
	
	if result:
		total_moves += 1
		audio_manager.play_swap()
		# Successful swap - animate the swap
		selected_cell = Vector2i(-1, -1)
		_update_selection_highlight()
		_animate_swap(a, b)
	else:
		# Failed swap - animate rejection snap-back
		audio_manager.play_reject()
		selected_cell = Vector2i(-1, -1)
		_update_selection_highlight()
		_animate_rejection_snapback(a, b)

func _animate_swap(a: Vector2i, b: Vector2i) -> void:
	is_animating = true
	is_processing_swap = false
	
	var gem_a = _get_gem_instance(a.x, a.y)
	var gem_b = _get_gem_instance(b.x, b.y)
	if gem_a == null || gem_b == null:
		# Fallback: no live gems to animate — go straight to the cascade phase.
		call_deferred("_process_match_sequence")
		return
	
	var pos_a = _get_cell_position(a.x, a.y)
	var pos_b = _get_cell_position(b.x, b.y)
	
	# The two nodes physically trade cells during the swap animation, so the
	# gem_instances array must trade with them. Otherwise _get_gem_instance()
	# returns the WRONG node for a cell, which could clear/misplace an unmatched
	# swapped gem (e.g. only one side of the swap actually matches).
	var ia := a.y * board_width + a.x
	var ib := b.y * board_width + b.x
	gem_instances[ia] = gem_b
	gem_instances[ib] = gem_a
	
	# TODO: the swap gems are intentionally NOT added to animating_gems — that
	# array is reserved for tracking the clear animation so the cascade chain
	# advances exactly once after every matched gem has been cleared.
	
	# Animate gem A to B's position
	var tween_a = create_tween()
	tween_a.set_parallel(true)
	tween_a.tween_property(gem_a, "position", pos_b, SWAP_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	
	# Animate gem B to A's position
	var tween_b = create_tween()
	tween_b.set_parallel(true)
	tween_b.tween_property(gem_b, "position", pos_a, SWAP_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	
	# Wait for swap animation to complete, then run the cascade sequence.
	await tween_a.finished
	_process_match_sequence()

func _animate_rejection_snapback(a: Vector2i, b: Vector2i) -> void:
	is_animating = true
	is_processing_swap = false
	
	var gem_a = _get_gem_instance(a.x, a.y)
	var gem_b = _get_gem_instance(b.x, b.y)
	if gem_a == null || gem_b == null:
		is_animating = false
		is_processing_swap = false
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
	
	# Safety timeout to force unlock if tween hangs
	var safety_timer = get_tree().create_timer(0.5)
	# Connect timeout to force unlock if tween hangs
	safety_timer.timeout.connect(_force_unlock_rejection.bind())
	await tween_a.finished
	
	# Ensure clean state after rejection
	is_animating = false
	is_processing_swap = false
	selected_cell = Vector2i(-1, -1)
	_sync_board_state()

func _process_match_sequence() -> void:
	# Called after the swap animation. board_sim emitted every cascade signal
	# synchronously during try_swap, so it already holds the final board and the
	# per-cascade clears were buffered in _pending_cascade_clears. Run the whole
	# cascade presentation as ONE linear await coroutine — no orphaned timers can
	# call back into the chain — guaranteed to end in an authoritative
	# refresh_board() and an input unlock.
	_run_cascade_sequence()

func _run_cascade_sequence() -> void:
	# Present each cascade depth STEP-BY-STEP so the player can read a multi-
	# cascade chain instead of all gems vanishing in one frame:
	#   1) special-elimination FX for any matched specials
	#   2) shrink/fade that depth's gems + pitch-escalating chime (0.15s)
	#   3) brief pause (0.10s) registering the new cascade
	# then a single gravity slide + spawn settle, and a full authoritative sync.
	if _pending_cascade_clears.is_empty():
		_settle_board()
		return

	for entry in _pending_cascade_clears:
		var cells: Array[Vector2i] = entry["cells"]
		var kind: int = entry["kind"]
		var depth: int = entry["depth"]
		if cells.is_empty():
			continue

		# 1) Special-elimination FX must play BEFORE the wave clears.
		var fx_time := _play_special_activations(cells, kind)
		if fx_time > 0.0:
			await get_tree().create_timer(fx_time).timeout

		# 2) Shrink/fade these gems + escalating chime (0.15s).
		audio_manager.play_match(depth)
		var clear_time := _animate_clear(cells, kind)
		if clear_time > 0.0:
			await get_tree().create_timer(clear_time).timeout

		# 3) Let the eye register the newly formed cascade.
		await get_tree().create_timer(CASCADE_STEP_PAUSE).timeout

	# Gravity slide: existing gems fall into the voids (0.18s).
	var fall_time := _compact_gravity()
	if fall_time > 0.0:
		await get_tree().create_timer(fall_time).timeout

	# Spawn new gems from above and drop them into place (0.18s).
	var spawn_time := _spawn_new_gems()
	if spawn_time > 0.0:
		await get_tree().create_timer(spawn_time).timeout

	# Authoritative sync + unlock.
	refresh_board()
	_update_cursor_highlight()
	_update_hover_highlight()
	is_animating = false
	is_processing_swap = false

func _settle_board() -> void:
	refresh_board()
	_update_cursor_highlight()
	_update_hover_highlight()
	is_animating = false
	is_processing_swap = false

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
			KEY_1:
				set_grid_size(6, 6)
				return
			KEY_2:
				set_grid_size(8, 8)
				return
			KEY_3:
				set_grid_size(10, 10)
				return
			KEY_4:
				set_grid_size(6, 8)
				return
			KEY_E:
				_attempt_rotate(true)
				return
			KEY_Q:
				_attempt_rotate(false)
				return
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
				
				var special = cell.get("special", 0)
				gem_instance.set_gem_state(cell.kind, cell.has_echo, special, cell.get("blocker", 0))

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

	# Level outcome detection (fires once per level via the guard flag).
	if not _level_result_handled:
		if board_sim.is_level_cleared():
			_level_result_handled = true
			emit_signal("level_cleared", current_level_id)
		elif board_sim.is_level_failed():
			_level_result_handled = true
			emit_signal("level_failed", current_level_id)

	_update_debug_hud()

func _clear_rejection_flash() -> void:
	for cell in rejected_cells:
		var idx = cell.y * board_width + cell.x
		var gem = gem_instances[idx]
		if gem != null && is_instance_valid(gem):
			gem.modulate = Color(1, 1, 1, 1)
	rejected_cells.clear()

func _animate_clear(cleared_cells: Array[Vector2i], gem_kind: int) -> float:
	# Fade/scale out the gems at the given cells. Returns the clear duration so
	# the coroutine can await the animation before applying gravity.
	for cell in cleared_cells:
		var gem = _get_gem_instance(cell.x, cell.y)
		if gem != null && is_instance_valid(gem):
			var idx = cell.y * board_width + cell.x
			gem_instances[idx] = null
			_spawn_clear_particles(gem, gem_kind)
			var tween = create_tween()
			tween.set_parallel(true)
			tween.tween_property(gem, "scale", Vector2(0, 0), CLEAR_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
			tween.tween_property(gem, "modulate:a", 0.0, CLEAR_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
			tween.finished.connect(gem.queue_free.bind())
	return CLEAR_ANIM_DURATION

func _compact_gravity() -> float:
	# Compact remaining live gems downward per column into the cleared holes.
	# Existing gems slide from their current cell down to their new cell. Returns
	# the longest fall (incl. bounce) so the coroutine can await it, or 0 if
	# nothing moved.
	var max_fall_time := 0.0
	for x in range(board_width):
		var empty_count := 0
		for y in range(board_height - 1, -1, -1):
			var idx = y * board_width + x
			var gem = gem_instances[idx]
			if gem == null || !is_instance_valid(gem):
				empty_count += 1
			elif empty_count > 0:
				var target_y = y + empty_count
				var target_pos = _get_cell_position(x, target_y)
				var fall_duration = FALL_ANIM_DURATION * empty_count
				max_fall_time = max(max_fall_time, fall_duration)
				var tween = create_tween()
				tween.tween_property(gem, "position", target_pos, fall_duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
				tween.tween_property(gem, "scale", Vector2(1.1, 0.9), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
				tween.tween_property(gem, "scale", Vector2(1.0, 1.0), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
				gem_instances[target_y * board_width + x] = gem
				gem_instances[y * board_width + x] = null
	if max_fall_time > 0.0:
		return max_fall_time + BOUNCE_ANIM_DURATION
	return 0.0

func _spawn_new_gems() -> float:
	# Spawn brand-new gems just above the top row and drop them into their
	# destination cells (the holes the gravity compact left at the top of each
	# column). Returns the longest drop (incl. bounce), or 0 if none spawned.
	var max_fall_time := 0.0
	for x in range(board_width):
		for y in range(board_height):
			var idx = y * board_width + x
			var cell = board_sim.get_cell(x, y)
			var gem = gem_instances[idx]
			if not cell.empty && (gem == null || !is_instance_valid(gem)):
				var kind = cell.kind
				var has_echo = cell.has_echo
				var special = cell.get("special", 0)
				var gem_instance = gem_scene.instantiate()
				gem_instance.set_gem_state(kind, has_echo, special, cell.get("blocker", 0))
				gem_instance.position = _get_cell_position(x, -1)
				add_child(gem_instance)
				gem_instances[idx] = gem_instance
				var fall_duration = FALL_ANIM_DURATION * (y + 1)
				max_fall_time = max(max_fall_time, fall_duration)
				var tween = create_tween()
				tween.tween_property(gem_instance, "position", _get_cell_position(x, y), fall_duration).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
				tween.tween_property(gem_instance, "scale", Vector2(1.1, 0.9), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
				tween.tween_property(gem_instance, "scale", Vector2(1.0, 1.0), BOUNCE_ANIM_DURATION).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
	if max_fall_time > 0.0:
		return max_fall_time + BOUNCE_ANIM_DURATION
	return 0.0

func _sync_board_state() -> void:
	# Sync visual board state with board_sim after animations
	refresh_board()
	_update_cursor_highlight()
	_update_hover_highlight()
	is_animating = false
	is_processing_swap = false

func _exit_tree() -> void:
	# Free the root-level debug layer with the board so it can't leak across
	# scene reloads.
	if debug_layer != null && is_instance_valid(debug_layer):
		debug_layer.queue_free()
	debug_layer = null
	debug_panel = null

# Signal handlers for match resolution and effects
func _on_match_resolved(cleared_cells: Array[Vector2i], gem_kind: int, cascade_depth: int) -> void:
	last_match_count += cleared_cells.size()
	last_cascade_depth = max(last_cascade_depth, cascade_depth)
	print("Match resolved: %d cells, kind %d, cascade %d" % [cleared_cells.size(), gem_kind, cascade_depth])
	
	# The chime is NOT played here: all cascade depths arrive synchronously in
	# one frame, so playing it here would overlap every depth's chime together.
	# Instead the paced cascade sequence (_run_cascade_sequence) plays it at the
	# moment each depth's gems pop, escalating the pitch per step.
	
	# board_sim emits every cascade depth synchronously in one frame (during
	# try_swap). We must NOT animate clears here — buffer them so the pipeline
	# runs a single controlled clear sequence after the swap animation finishes.
	_pending_cascade_clears.append({
		"cells": cleared_cells.duplicate(),
		"kind": gem_kind,
		"depth": cascade_depth,
	})

func _on_special_gem_created(pos: Vector2i, kind: int) -> void:
	var kind_names = {0: "Bolt", 1: "Prism", 2: "Nova"}
	print("Special gem created: %s at (%d, %d)" % [kind_names.get(kind, "Unknown"), pos.x, pos.y])
	
	# Play special gem creation sound
	audio_manager.play_special_create()
	
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
	
	# Play echo detonation sound - the signature audio payoff
	audio_manager.play_echo_detonate()
	
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

# --- QA / debug helpers ---
# Find a legal swap that creates a match (for automated play-testing / QA).
func find_valid_swap() -> Array:
	var sw: int = board_sim.get_width()
	var sh: int = board_sim.get_height()
	var kinds: Dictionary = {}
	for y in sw:
		for x in sw:
			var c = board_sim.get_cell(x, y)
			kinds[Vector2i(x, y)] = int(c.kind) if not bool(c.empty) else -1
	for y in sw:
		for x in sw:
			if int(kinds[Vector2i(x, y)]) < 0:
				continue
			if x + 1 < sw and _swap_creates_match(kinds, sw, sh, x, y, x + 1, y):
				return [Vector2i(x, y), Vector2i(x + 1, y)]
			if y + 1 < sh and _swap_creates_match(kinds, sw, sh, x, y, x, y + 1):
				return [Vector2i(x, y), Vector2i(x, y + 1)]
	return []

func _swap_creates_match(kinds: Dictionary, w: int, h: int, ax: int, ay: int, bx: int, by: int) -> bool:
	var a := Vector2i(ax, ay)
	var b := Vector2i(bx, by)
	var ka := int(kinds[a])
	var kb := int(kinds[b])
	if ka < 0 or kb < 0:
		return false
	kinds[a] = kb
	kinds[b] = ka
	var matches := _run_len(kinds, w, h, ax, ay, kb) >= 3 or _run_len(kinds, w, h, bx, by, ka) >= 3
	kinds[a] = ka
	kinds[b] = kb
	return matches

func _run_len(kinds: Dictionary, w: int, h: int, x: int, y: int, k: int) -> int:
	var horiz := 1
	var cx := x - 1
	while cx >= 0 and int(kinds.get(Vector2i(cx, y), -1)) == k:
		horiz += 1
		cx -= 1
	cx = x + 1
	while cx < w and int(kinds.get(Vector2i(cx, y), -1)) == k:
		horiz += 1
		cx += 1
	if horiz >= 3:
		return horiz
	var vert := 1
	var cy := y - 1
	while cy >= 0 and int(kinds.get(Vector2i(x, cy), -1)) == k:
		vert += 1
		cy -= 1
	cy = y + 1
	while cy < h and int(kinds.get(Vector2i(x, cy), -1)) == k:
		vert += 1
		cy += 1
	return vert

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

func _force_unlock_rejection() -> void:
	if is_animating:
		is_animating = false
		is_processing_swap = false
		selected_cell = Vector2i(-1, -1)
		_sync_board_state()

# --- Gravity Tumbler (90° board rotation) ---

# Public entry points for the on-screen rotate buttons.
func rotate_clockwise() -> void:
	_attempt_rotate(true)

func rotate_counter_clockwise() -> void:
	_attempt_rotate(false)

func _attempt_rotate(clockwise: bool) -> void:
	if is_processing_swap || is_animating:
		return
	is_processing_swap = true

	# board_sim's rotate_board transposes the grid in Rust and buffers each
	# cascade depth's clears into _pending_cascade_clears (same contract as a
	# swap).
	_pending_cascade_clears.clear()
	_rotate_tumbler(clockwise)

# The standard Match-3 "spin & reset" tumbler — made SEAMLESS (QA Issue B):
#
# The sim's transpose mapping is:  CW  old(x,y) -> new(y, old_h-1-x)
#                                  CCW old(x,y) -> new(x, old_w-1-y)
# A visual container rotation of −90° displays contents exactly as a CW
# transpose, and +90° exactly as a CCW transpose. So:
#   1) Tween the board to the matching angle (−90° for CW, +90° for CCW).
#   2) While STILL rotated, re-seat every gem node into its new transposed
#      grid cell — on screen each gem does not move a single pixel.
#   3) Reset rotation_degrees = 0 — also pixel-invisible now, because the
#      local positions already encode the rotated frame. No snap, ever.
#   4) Present the gravity-down cascades as usual.
func _rotate_tumbler(clockwise: bool) -> void:
	is_animating = true
	is_processing_swap = false

	# 1) Animate the visual spin of the whole board container to the angle that
	# MATCHES the sim's transpose direction (−90° shows a CW-transposed grid).
	var target_angle := -90.0 if clockwise else 90.0
	var spin := create_tween().set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	spin.tween_property(self, "rotation_degrees", target_angle, ROTATE_ANIM_DURATION)
	audio_manager.play_swap()
	await spin.finished

	# Capture the pre-rotation dimensions for the index remap below.
	var old_w := board_width
	var old_h := board_height

	# 2) Authoritative grid transpose in Rust (gravity always down); produces
	# the per-depth cascade clears that _process_match_sequence will present.
	var result = board_sim.rotate_board(clockwise)
	if not result:
		rotation_degrees = 0.0
		is_animating = false
		is_processing_swap = false
		return

	total_moves += 1
	selected_cell = Vector2i(-1, -1)
	_update_selection_highlight()

	# Sync sim dimensions if a non-square grid swapped W x H <-> H x W so
	# _get_cell_position stays perfectly centered below the HUD.
	_apply_sim_dimensions()

	# 3) SEAMLESS RE-SEAT: while the container is still at ±90°, move every gem
	# node into its new transposed cell. Because the spin angle matches the
	# data mapping, each surviving gem's screen position is unchanged by this.
	_reseat_gems_into_transposed_cells(clockwise, old_w, old_h)

	# Resetting to 0° is now visually invisible — local positions already
	# carry the rotated frame. Mouse coords / viewport bounds are stable again.
	rotation_degrees = 0.0

	# 4) Present the gravity-down cascades (per-depth clears => slide => spawn
	# => refresh + unlock).
	_process_match_sequence()

func _reseat_gems_into_transposed_cells(clockwise: bool, old_w: int, old_h: int) -> void:
	# Remap every existing gem node from its OLD grid index to its NEW one and
	# set position directly (no tween): at the matched spin angle the new cell
	# renders at exactly the same screen point the gem already occupies.
	var new_instances: Array[Node2D] = []
	new_instances.resize(board_width * board_height)
	for oy in range(old_h):
		for ox in range(old_w):
			var idx := oy * old_w + ox
			if idx >= gem_instances.size():
				continue
			var gem = gem_instances[idx]
			if gem == null || !is_instance_valid(gem):
				continue
			# Same mapping as bewildered-core::Board::rotate_board.
			var nx: int
			var ny: int
			if clockwise:
				nx = oy
				ny = old_h - 1 - ox
			else:
				nx = ox
				ny = old_w - 1 - oy
			gem.position = _get_cell_position(nx, ny)
			new_instances[ny * board_width + nx] = gem
	gem_instances = new_instances

# Keep board_width/board_height + the gem instance array in sync with the sim so
# a transposed (W x H <-> H x W) board stays centered.
func _apply_sim_dimensions() -> void:
	var sw := int(board_sim.get_width())
	var sh := int(board_sim.get_height())
	if sw != board_width or sh != board_height:
		board_width = sw
		board_height = sh
		gem_instances.resize(sw * sh)

# --- Special-gem elimination FX (fired before a computer of that kind clears) ---

# Detect matched specials in a wave and play their signature FX. Returns the
# max FX duration to await before clearing so the beam/ring visibly plays.
func _play_special_activations(cells: Array[Vector2i], kind: int) -> float:
	var bolt_cells: Array[Vector2i] = []
	var prism_pos: Vector2i = Vector2i(-1, -1)
	var nova_pos: Vector2i = Vector2i(-1, -1)
	for cell in cells:
		var gem = _get_gem_instance(cell.x, cell.y)
		if gem == null || !is_instance_valid(gem):
			continue
		if gem.current_special == 1:
			bolt_cells.append(cell)
		elif gem.current_special == 2:
			prism_pos = cell
		elif gem.current_special == 3:
			nova_pos = cell

	var max_t := 0.0
	if not bolt_cells.is_empty():
		_spawn_bolt_beam(cells)
		max_t = max(max_t, 0.22)
	if prism_pos.x >= 0:
		_spawn_prism_shimmer(kind)
		max_t = max(max_t, 0.3)
	if nova_pos.x >= 0:
		_spawn_nova_blast(nova_pos)
		max_t = max(max_t, 0.35)
	return max_t

func _spawn_bolt_beam(cells: Array[Vector2i]) -> void:
	# A bright beam across every row & column touched by the cleared Bolt. Each
	# unique row yields a full-width beam; each unique column a full-height beam.
	var rows := {}
	var cols := {}
	for cell in cells:
		rows[cell.y] = true
		cols[cell.x] = true
	for row in rows:
		_spawn_row_beam(int(row))
	for col in cols:
		_spawn_col_beam(int(col))

func _beam_sprite(w: int, h: int, color: Color) -> Sprite2D:
	var img := Image.create(int(w), int(h), false, Image.FORMAT_RGBA8)
	img.fill(color)
	var sprite := Sprite2D.new()
	sprite.texture = ImageTexture.create_from_image(img)
	sprite.z_index = 18
	add_child(sprite)
	return sprite

func _spawn_row_beam(row: int) -> void:
	var board_px_w := int(board_width * cell_size + (board_width - 1) * padding)
	var beam_h := int(cell_size * 0.85)
	var beam := _beam_sprite(board_px_w, beam_h, Color(1.0, 1.0, 0.55, 0.95))
	var center := _get_cell_position(0, row) + Vector2(cell_size / 2.0, cell_size / 2.0)
	# Horizontal beam spans the board, so its x-center is the board origin x = 0.
	beam.position = Vector2(0.0, center.y)
	var tween := create_tween()
	tween.set_parallel(true)
	tween.tween_property(beam, "modulate:a", 0.0, 0.22).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.finished.connect(beam.queue_free.bind())

func _spawn_col_beam(col: int) -> void:
	var board_px_h := int(board_height * cell_size + (board_height - 1) * padding)
	var beam_w := int(cell_size * 0.85)
	var beam := _beam_sprite(beam_w, board_px_h, Color(1.0, 1.0, 0.55, 0.95))
	var center := _get_cell_position(col, 0) + Vector2(cell_size / 2.0, cell_size / 2.0)
	beam.position = Vector2(center.x, 0.0)
	var tween := create_tween()
	tween.set_parallel(true)
	tween.tween_property(beam, "modulate:a", 0.0, 0.22).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.finished.connect(beam.queue_free.bind())

func _spawn_prism_shimmer(kind: int) -> void:
	# Flash every gem of the matched color through a rainbow cycle, then clear.
	var rainbow := [
		Color(1.0, 0.3, 0.3, 1.0),
		Color(1.0, 0.6, 0.2, 1.0),
		Color(1.0, 1.0, 0.3, 1.0),
		Color(0.4, 1.0, 0.4, 1.0),
		Color(0.3, 0.6, 1.0, 1.0),
		Color(0.7, 0.3, 1.0, 1.0),
	]
	var step := 0.05
	for gem in gem_instances:
		if gem == null || !is_instance_valid(gem):
			continue
		if gem.current_kind != kind:
			continue
		var tween := create_tween()
		tween.set_parallel(true)
		for ci in range(rainbow.size()):
			tween.tween_property(gem, "modulate", rainbow[ci], step).set_trans(Tween.TRANS_LINEAR)
		tween.tween_property(gem, "modulate", Color(1, 1, 1, 1), step)

func _spawn_nova_blast(pos: Vector2i) -> void:
	# Expanding orange/red blast ring centered on the Nova.
	var center := _get_cell_position(pos.x, pos.y) + Vector2(cell_size / 2.0, cell_size / 2.0)
	var size := int(cell_size * 3)
	var img := Image.create(size, size, false, Image.FORMAT_RGBA8)
	img.fill(Color(0, 0, 0, 0))
	var c := Vector2(size / 2.0, size / 2.0)
	for y in range(size):
		for x in range(size):
			var p := Vector2(x, y)
			var dist := p.distance_to(c)
			var r0 := size * 0.22
			var r1 := size * 0.5
			if dist >= r0 and dist <= r1:
				var t := (dist - r0) / (r1 - r0)
				img.set_pixel(x, y, Color(1.0, 0.35 + 0.4 * t, 0.1, 1.0 - t))
	var ring := Sprite2D.new()
	ring.texture = ImageTexture.create_from_image(img)
	ring.position = center
	ring.z_index = 18
	add_child(ring)
	var tween := create_tween()
	tween.set_parallel(true)
	tween.tween_property(ring, "scale", Vector2(1.6, 1.6), 0.35).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.tween_property(ring, "modulate:a", 0.0, 0.35).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.finished.connect(ring.queue_free.bind())
