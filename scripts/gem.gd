extends Sprite2D

class_name Gem

## Doodle-atlas gem: an ink stamp card from the halftone sheet rendered with
## the duotone gradient-map shader (deep ink -> dynamic gem tint), with an
## echo state that shifts the card to pulsing solar gold.

const ATLAS_DB := preload("res://scripts/atlas_db.gd")
const DUOTONE_SHADER := preload("res://assets/shaders/duotone_card.gdshader")

## Per-kind highlight tints (kind 0..3) + blocker tints.
const KIND_TINTS: Array[Color] = [
	Color("00e5ff"), # Disc / Eye — electric cyan
	Color("ffb300"), # Flame / Triangle — sunburst amber
	Color("00e676"), # Box / Key — radiant emerald
	Color("e040fb"), # Diamond / Ghost — mystic magenta
]
const STONE_TINT := Color("78909c")
const ICE_TINT := Color("80deea")

@export var cell_size: float = 64.0

var current_kind: int = -1
var has_echo: bool = false
var current_special: int = 0  # 0 = None, 1 = Bolt, 2 = Prism, 3 = Nova
var current_blocker: int = 0  # 0 = None, 1 = Stone, 2 = Ice

var _material: ShaderMaterial
var _special_icon: Sprite2D


func _ready() -> void:
	texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	_material = ShaderMaterial.new()
	_material.shader = DUOTONE_SHADER
	material = _material

	_special_icon = Sprite2D.new()
	_special_icon.name = "SpecialIcon"
	_special_icon.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	_special_icon.position = Vector2(cell_size * 0.26, -cell_size * 0.26)
	_special_icon.scale = Vector2.ONE * (cell_size / 96.0) * 0.62
	_special_icon.visible = false
	add_child(_special_icon)


func set_gem(kind: int, echo: bool, special: int = 0) -> void:
	set_gem_state(kind, echo, special, 0)


func set_gem_state(kind: int, echo: bool, special: int, blocker: int) -> void:
	current_kind = kind
	has_echo = echo
	current_special = special
	current_blocker = blocker
	_apply_visual()


func _icon_rect(name: String) -> Rect2:
	return ATLAS_DB.icon_rect(name)


func _make_atlas(rect: Rect2) -> AtlasTexture:
	var tex := AtlasTexture.new()
	tex.atlas = load(ATLAS_DB.SHEET)
	tex.region = rect
	return tex


func _apply_visual() -> void:
	# Pick the stamp: blockers override the kind icon; specials ride as a
	# small corner badge on top of the kind stamp.
	var icon := "kind_disc"
	var tint := KIND_TINTS[0]
	match current_kind % 4:
		0:
			icon = "kind_disc"
			tint = KIND_TINTS[0]
		1:
			icon = "kind_triangle"
			tint = KIND_TINTS[1]
		2:
			icon = "kind_key"
			tint = KIND_TINTS[2]
		3:
			icon = "kind_diamond"
			tint = KIND_TINTS[3]

	if current_blocker == 1:
		icon = "blocker_stone"
		tint = STONE_TINT
	elif current_blocker == 2:
		icon = "blocker_ice"
		tint = ICE_TINT

	texture = _make_atlas(_icon_rect(icon))
	_material.set_shader_parameter("highlight_color", tint)
	_material.set_shader_parameter("echo_amount", 1.0 if has_echo else 0.0)

	# Corner badge for special gems (gold duotone).
	if current_special > 0 and current_blocker == 0:
		var badge := "special_bolt"
		match current_special:
			1: badge = "special_bolt"
			2: badge = "special_prism"
			3: badge = "special_nova"
		_special_icon.texture = _make_atlas(_icon_rect(badge))
		var bm := _special_icon.material as ShaderMaterial
		if bm == null:
			bm = ShaderMaterial.new()
			bm.shader = DUOTONE_SHADER
			_special_icon.material = bm
		bm.set_shader_parameter("highlight_color", Color("ffe082"))
		bm.set_shader_parameter("echo_amount", 1.0)
		_special_icon.visible = true
	else:
		_special_icon.visible = false
