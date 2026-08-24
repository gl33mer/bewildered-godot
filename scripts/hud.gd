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

@onready var level_title: Label = %LevelTitle
@onready var objective_label: Label = %ObjectiveLabel
@onready var progress_bar: ProgressBar = %ObjectiveProgress
@onready var progress_label: Label = %ProgressLabel
@onready var moves_label: Label = %MovesLabel
@onready var score_label: Label = %ScoreLabel
@onready var multiplier_label: Label = %MultiplierLabel

func _ready() -> void:
	board = get_node(board_path)
	sim = board.get_board_sim()
	board.level_cleared.connect(_on_level_cleared)
	board.level_failed.connect(_on_level_failed)
	_refresh()

func _process(_delta: float) -> void:
	_refresh()

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