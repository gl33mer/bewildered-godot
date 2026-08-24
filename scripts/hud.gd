extends CanvasLayer
class_name GameHUD

## Level HUD: chamber title, objective + progress bar, moves remaining,
## score and the active resonance multiplier badge. Also owns the victory /
## defeat modal (level_complete_dialog.tscn).

@export var board_path: NodePath = NodePath("../Board")
@export var dialog_scene: PackedScene = preload("res://scenes/level_complete_dialog.tscn")

var board: Node2D
var sim: RefCounted
var dialog: Control = null
var top_bar: PanelContainer
var _board_positioned: bool = false

@onready var level_title: Label = %LevelTitle
@onready var objective_label: Label = %ObjectiveLabel
@onready var progress_bar: ProgressBar = %ObjectiveProgress
@onready var progress_label: Label = %ProgressLabel
@onready var moves_label: Label = %MovesLabel
@onready var score_label: Label = %ScoreLabel
@onready var multiplier_label: Label = %MultiplierLabel
@onready var rotate_ccw_button: Button = %RotateCCW
@onready var rotate_cw_button: Button = %RotateCW

func _ready() -> void:
	board = get_node(board_path)
	sim = board.get_board_sim()
	top_bar = get_node_or_null("Root/TopBar")
	board.level_cleared.connect(_on_level_cleared)
	board.level_failed.connect(_on_level_failed)
	rotate_ccw_button.pressed.connect(board.rotate_counter_clockwise)
	rotate_cw_button.pressed.connect(board.rotate_clockwise)
	_refresh()

func _process(_delta: float) -> void:
	# Position the board once the HUD bar has a real size (layout runs after
	# _ready). Keeps the top rows of gems fully clickable below the HUD.
	if not _board_positioned and top_bar != null and top_bar.size.y > 0.0:
		_position_board_below_hud()
		_board_positioned = true
	_refresh()

func _position_board_below_hud() -> void:
	if board == null:
		return
	var view_size := get_viewport().get_visible_rect().size
	var hud_bottom_screen: float = top_bar.global_position.y + top_bar.size.y
	var board_px: float = float(board.board_height) * float(board.cell_size) + (float(board.board_height) - 1.0) * float(board.padding)
	# Scale the board so it fills the space below the HUD with generous breathing
	# room: a fixed 40px top plate below the banner (so the top gem row is clearly
	# clear of the HUD) and 24px bottom/side margins. Never scales UP. Node scale
	# keeps click mapping correct because get_local_mouse_position() returns
	# design-unit local coords.
	var top_gap := 40.0
	var bottom_gap := 24.0
	var avail_h := view_size.y - hud_bottom_screen - top_gap - bottom_gap
	var avail_w := view_size.x - 48.0
	var fit: float = min(min(1.0, avail_h / board_px), avail_w / board_px)
	var scaled_px: float = board_px * fit
	var half_vp_h := view_size.y * 0.5
	board.scale = Vector2(fit, fit)
	board.position = Vector2(0.0, hud_bottom_screen - half_vp_h + top_gap + scaled_px * 0.5)

func _refresh() -> void:
	if sim == null:
		return
	level_title.text = sim.get_level_title()
	objective_label.text = sim.get_objective_description()

	var cur: int = sim.get_objective_progress()
	var tgt: int = sim.get_target_score()
	if tgt > 0:
		progress_bar.max_value = tgt
		progress_bar.value = min(cur, tgt)
		progress_label.text = "%d / %d" % [cur, tgt]
	else:
		progress_label.text = "-"

	moves_label.text = "Moves: %d" % sim.get_moves_remaining()
	score_label.text = "Score: %d" % sim.get_score()
	multiplier_label.text = "Resonance x%.2f" % sim.get_resonance_multiplier()

func _on_level_cleared(_level_id: String) -> void:
	_show_dialog(true)

func _on_level_failed(_level_id: String) -> void:
	_show_dialog(false)

func _show_dialog(victory: bool) -> void:
	if dialog != null:
		dialog.queue_free()
	dialog = dialog_scene.instantiate()
	add_child(dialog)
	if victory:
		dialog.show_victory(sim.get_level_title(), sim.get_score())
	else:
		dialog.show_failed(sim.get_level_title(), sim.get_score())
	dialog.next_chamber_pressed.connect(_on_next)
	dialog.retry_pressed.connect(_on_retry)

func _on_next() -> void:
	board.load_next_level()
	if dialog != null:
		dialog.queue_free()
		dialog = null

func _on_retry() -> void:
	board.retry_level()
	if dialog != null:
		dialog.queue_free()
		dialog = null
