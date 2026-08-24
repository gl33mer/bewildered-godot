extends Control
class_name LevelCompleteDialog

## Victory/defeat modal shown when a chamber is cleared or the player runs
## out of moves. The single primary button routes to "next" or "retry".

signal next_chamber_pressed
signal retry_pressed

enum Mode { VICTORY, FAILED }

var mode: Mode = Mode.VICTORY

@onready var result_title: Label = %ResultTitle
@onready var summary_label: Label = %SummaryLabel
@onready var score_label: Label = %ScoreLabel
@onready var primary_button: Button = %PrimaryButton

func _ready() -> void:
	visible = false
	primary_button.pressed.connect(_on_primary_pressed)

func show_victory(level_title: String, score: int) -> void:
	mode = Mode.VICTORY
	visible = true
	result_title.text = "Chamber Cleared"
	result_title.add_theme_color_override("font_color", Color(0.5, 1.0, 0.6))
	summary_label.text = "%s\nObjective complete!" % level_title
	score_label.text = "Score: %s" % _fmt_int(score)
	primary_button.text = "Next Chamber"

func show_failed(level_title: String, score: int) -> void:
	mode = Mode.FAILED
	visible = true
	result_title.text = "Chamber Failed"
	result_title.add_theme_color_override("font_color", Color(1.0, 0.55, 0.5))
	summary_label.text = "Out of moves in\n%s" % level_title
	score_label.text = "Score: %s" % _fmt_int(score)
	primary_button.text = "Retry"

func hide_dialog() -> void:
	visible = false

func _on_primary_pressed() -> void:
	if mode == Mode.VICTORY:
		next_chamber_pressed.emit()
	else:
		retry_pressed.emit()
	hide_dialog()

func _fmt_int(v: int) -> String:
	return str(v)