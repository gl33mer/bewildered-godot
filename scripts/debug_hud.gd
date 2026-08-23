extends Control

class_name DebugHUD

@onready var board: Node2D = get_node("../Board")

# UI Elements
@onready var hover_label: Label = %HoverLabel
@onready var selected_label: Label = %SelectedLabel
@onready var cursor_label: Label = %CursorLabel
@onready var action_log: Label = %ActionLog
@onready var moves_label: Label = %MovesLabel
@onready var score_label: Label = %ScoreLabel
@onready var combo_label: Label = %ComboLabel
@onready var cascade_label: Label = %CascadeLabel
@onready var multiplier_label: Label = %MultiplierLabel
@onready var coord_test_label: Label = %CoordTestLabel

func _ready():
	set_process(true)

func _process(delta: float) -> void:
	_update_hud()

func _update_hud() -> void:
	if board == null || !is_instance_valid(board):
		return
	
	# Hover cell
	var hover = board.get_hover_cell()
	if hover.x >= 0:
		var cell = board.get_board_sim().get_cell(hover.x, hover.y)
		var kind_name = _gem_kind_to_string(cell.kind) if not cell.empty else "Empty"
		hover_label.text = "Hover: (%d, %d) - %s" % [hover.x, hover.y, kind_name]
		hover_label.add_theme_color_override("font_color", Color(0.9, 0.9, 0.9))
	else:
		hover_label.text = "Hover: None"
		hover_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	
	# Selected cell
	var selected = board.get_selected_cell()
	if selected.x >= 0:
		var cell = board.get_board_sim().get_cell(selected.x, selected.y)
		var kind_name = _gem_kind_to_string(cell.kind) if not cell.empty else "Empty"
		selected_label.text = "Selected: (%d, %d) - %s" % [selected.x, selected.y, kind_name]
		selected_label.add_theme_color_override("font_color", Color(1, 1, 1))
	else:
		selected_label.text = "Selected: None"
		selected_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	
	# Keyboard cursor
	var cursor = board.get_cursor_cell()
	cursor_label.text = "Cursor: (%d, %d)" % [cursor.x, cursor.y]
	
	# Action log
	var action = board.get_last_action_log()
	action_log.text = "Action: %s" % action
	if action.begins_with("REJECTED"):
		action_log.add_theme_color_override("font_color", Color(1, 0.4, 0.4))
	elif action.begins_with("Swap") || action.begins_with("Keyboard swap"):
		action_log.add_theme_color_override("font_color", Color(0.6, 1, 0.6))
	else:
		action_log.add_theme_color_override("font_color", Color(0.9, 0.9, 0.9))
	
	# Stats
	moves_label.text = "Moves: %d" % board.get_total_moves()
	score_label.text = "Score: %d" % board.get_total_score()
	
	# Get board sim for combo/multiplier
	var sim = board.get_board_sim()
	if sim != null:
		combo_label.text = "Combo: %d" % sim.get_combo()
		multiplier_label.text = "Multiplier: %.2fx" % sim.get_resonance_multiplier()
	
	# Cascade depth from last swap
	if board.last_cascade_depth > 0:
		cascade_label.text = "Last Cascade: %d" % board.last_cascade_depth
	else:
		cascade_label.text = "Last Cascade: -"
	
	# Coordinate self-test results
	var results = board.get_coord_test_results()
	if results.size() > 0:
		var passed = 0
		var total = 0
		for r in results:
			total += 3
			if r.center.passed: passed += 1
			if r.top_left.passed: passed += 1
			if r.bottom_right.passed: passed += 1
		
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