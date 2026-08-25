extends CanvasLayer
class_name RelicSelection

## Between-chamber relic draft screen. Shows 3 styled relic cards; emits
## relic_chosen(id) when the player picks one. Pure presentation.

signal relic_chosen(id: String)

const RARITY_COLORS := {
	"Common": Color("9fb4c8"),
	"Rare": Color("4fa3e0"),
	"Epic": Color("c884ff"),
}

var _root: Control


func _ready() -> void:
	layer = 15
	_root = _build_root()
	add_child(_root)
	_root.visible = false


func show_offers(offers: Array) -> void:
	for child in _cards_box().get_children():
		child.queue_free()
	for offer in offers:
		_cards_box().add_child(_build_card(offer))
	_root.visible = true


func hide_offers() -> void:
	_root.visible = false


func _cards_box() -> HBoxContainer:
	return _root.get_node("Center/Panel/VBox/Cards") as HBoxContainer


func _build_root() -> Control:
	var root := Control.new()
	root.name = "RelicDraft"
	root.set_anchors_preset(Control.PRESET_FULL_RECT)

	var dim := ColorRect.new()
	dim.color = Color(0.02, 0.03, 0.08, 0.82)
	dim.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.add_child(dim)

	var center := CenterContainer.new()
	center.name = "Center"
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.add_child(center)

	var panel := PanelContainer.new()
	panel.name = "Panel"
	var style := StyleBoxFlat.new()
	style.bg_color = Color("141a2c")
	style.border_color = Color("3d4a75")
	style.set_border_width_all(2)
	style.set_corner_radius_all(10)
	style.content_margin_left = 26.0
	style.content_margin_right = 26.0
	style.content_margin_top = 20.0
	style.content_margin_bottom = 22.0
	panel.add_theme_stylebox_override("panel", style)
	center.add_child(panel)

	var vbox := VBoxContainer.new()
	vbox.name = "VBox"
	vbox.add_theme_constant_override("separation", 14)
	panel.add_child(vbox)

	var title := Label.new()
	title.text = "— CHAMBER CLEARED — CHOOSE A RELIC —"
	title.add_theme_font_size_override("font_size", 24)
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	vbox.add_child(title)

	var cards := HBoxContainer.new()
	cards.name = "Cards"
	cards.add_theme_constant_override("separation", 18)
	vbox.add_child(cards)

	return root


func _build_card(offer: Dictionary) -> PanelContainer:
	var card := PanelContainer.new()
	card.custom_minimum_size = Vector2(220, 240)
	var style := StyleBoxFlat.new()
	style.bg_color = Color("1b2340")
	style.border_color = RARITY_COLORS.get(offer.get("rarity", "Common"), Color.WHITE)
	style.set_border_width_all(2)
	style.set_corner_radius_all(8)
	style.content_margin_left = 14.0
	style.content_margin_right = 14.0
	style.content_margin_top = 12.0
	style.content_margin_bottom = 12.0
	card.add_theme_stylebox_override("panel", style)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 8)
	card.add_child(vbox)

	var name_label := Label.new()
	name_label.text = str(offer.get("name", "Relic"))
	name_label.add_theme_font_size_override("font_size", 20)
	name_label.add_theme_color_override(
		"font_color", RARITY_COLORS.get(offer.get("rarity", "Common"), Color.WHITE))
	name_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	vbox.add_child(name_label)

	var rarity_label := Label.new()
	rarity_label.text = str(offer.get("rarity", "Common"))
	rarity_label.add_theme_font_size_override("font_size", 13)
	rarity_label.modulate = Color(1, 1, 1, 0.7)
	vbox.add_child(rarity_label)

	var desc := Label.new()
	desc.text = str(offer.get("description", ""))
	desc.add_theme_font_size_override("font_size", 14)
	desc.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	desc.size_flags_vertical = Control.SIZE_EXPAND_FILL
	vbox.add_child(desc)

	var choose := Button.new()
	choose.text = "Choose"
	var relic_id := str(offer.get("id", ""))
	choose.pressed.connect(func(): relic_chosen.emit(relic_id))
	vbox.add_child(choose)

	return card
