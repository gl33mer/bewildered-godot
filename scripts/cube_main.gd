extends Node3D

## Bewildered 3D Cube Chamber (Phase 4).
## Builds six N x N faces around a CubeSim, renders gems in 3D, handles the
## snap-turn orbit camera (A/D yaw, W/S pitch, Q/E face tumble) and mouse
## picking (click select / click adjacent to swap) on the active face.
## All rules live in Rust (CubeSim); this script is presentation + input only.

const CELL := 1.0
const BOARD_SEED := 20260825

## Cells per face edge (4..10). Rebuild with keys 1-4 in game.
@export var face_size: int = 6

# Face conventions — MUST match bewildered-core topology.rs:
# 0=Front(+Z) 1=Right(+X) 2=Back(-Z) 3=Left(-X) 4=Top(+Y) 5=Bottom(-Y)
# u = local +x axis in world, v = local +y (down) axis in world.
const NORMALS: Array[Vector3] = [
	Vector3(0, 0, 1), Vector3(1, 0, 0), Vector3(0, 0, -1),
	Vector3(-1, 0, 0), Vector3(0, 1, 0), Vector3(0, -1, 0),
]
const U_AXES: Array[Vector3] = [
	Vector3(1, 0, 0), Vector3(0, 0, -1), Vector3(-1, 0, 0),
	Vector3(0, 0, 1), Vector3(1, 0, 0), Vector3(1, 0, 0),
]
const V_AXES: Array[Vector3] = [
	Vector3(0, -1, 0), Vector3(0, -1, 0), Vector3(0, -1, 0),
	Vector3(0, -1, 0), Vector3(0, 0, -1), Vector3(0, 0, 1),
]

const KIND_COLORS: Array[Color] = [
	Color("39c5cf"), # Circle — cyan
	Color("e8c832"), # Triangle — yellow
	Color("4fae4f"), # Square — green
	Color("cc4fcc"), # Diamond — magenta
]

const CLEAR_DURATION := 0.16
const REFRESH_DELAY := 0.42
const SPIN_DURATION := 0.24

var sim: CubeSim
var camera: CubeSnapCamera
var holders: Array[Node3D] = []
var rest_transforms: Array[Transform3D] = []
var gem_nodes := {} # Vector3i(face,x,y) -> MeshInstance3D
var kind_materials: Array[StandardMaterial3D] = []
var echo_materials: Array[StandardMaterial3D] = []
var stone_material: StandardMaterial3D
var ice_material: StandardMaterial3D
var plate_material: StandardMaterial3D
var paper_material: StandardMaterial3D
var selected := {} # {face, x, y}
var busy := false
var _touch_device := false
var _drag_start := Vector2.ZERO
var _drag_gem_face := -1
var _drag_gem_cell := Vector2i(-1, -1)
# Pinch-to-zoom tracking.
var _pinch_touch1 := -1
var _pinch_touch2 := -1
var _pinch_touch1_pos := Vector2.ZERO
var _pinch_touch2_pos := Vector2.ZERO
var _pinch_start_dist := 0.0
var _pinch_start_zoom := 1.0
var _debug_shatter := false
var _debug_beam := false
var _debug_shockwave := false
var _debug_selection := false
var _debug_facespin := false
var status_label: Label
var faces_root: Node3D

# Descent (Phase 6)
const DESCENT_LENGTH := 3
var runner: DescentRunner
var relic_ui: RelicSelection
var held_relics: Array = []
var stats_label: Label
var tray_box: HBoxContainer

const ANTIPODE_FACE := [2, 3, 0, 1, 5, 4]

const ATLAS_DB := preload("res://scripts/atlas_db.gd")
const DUOTONE_3D := preload("res://assets/shaders/duotone_card_3d.gdshader")
const SHEET_TEX := preload("res://assets/sprites/halftone_sheet_clean.png")

## Icon stamp per visual state (kind 0-3, specials, blockers).
const STATE_ICONS := {
	"kind0": "kind_disc",
	"kind1": "kind_triangle",
	"kind2": "kind_key",
	"kind3": "kind_diamond",
	"special1": "special_bolt",
	"special2": "special_prism",
	"special3": "special_nova",
	"blocker1": "blocker_stone",
	"blocker2": "blocker_ice",
}

var icon_materials := {} # state key -> ShaderMaterial
var icon_meshes := {} # state key -> QuadMesh


func _ready() -> void:
	_touch_device = DisplayServer.is_touchscreen_available()
	_build_environment()
	_build_materials()

	camera = CubeSnapCamera.new()
	add_child(camera)

	_build_hud()

	start_descent(face_size)


## Begin a fresh descent at the given face size (also the size-switch entry).
func start_descent(n: int) -> void:
	face_size = clampi(n, 4, 10)
	_build_chamber_visuals()

	runner = DescentRunner.new()
	runner.start_run(BOARD_SEED)
	held_relics.clear()
	_begin_chamber()


## Rebuild sim + faces for the current face_size and wire sim signals.
func _build_chamber_visuals() -> void:
	if faces_root:
		faces_root.queue_free()
	if _refresh_timer:
		_refresh_timer.stop()
	busy = false
	selected = {}
	gem_nodes.clear()
	holders.clear()
	rest_transforms.clear()

	sim = CubeSim.new()
	sim.new_cube_board(face_size, BOARD_SEED)
	# Baseline: pure match-3/4/5 only (no echo, antipodal, specials).
	sim.set_match_config_baseline()
	sim.cube_match_resolved.connect(_on_match_resolved)
	sim.cube_echo_detonated.connect(_on_echo_detonated)
	sim.antipodal_echo_charged.connect(_on_antipodal_charged)
	sim.cube_special_gem_created.connect(_on_special_created)
	sim.descent_chamber_finished.connect(_on_chamber_finished)

	faces_root = Node3D.new()
	faces_root.name = "Faces"
	add_child(faces_root)
	for f in 6:
		_build_face(f)

	camera.set_distance(2.35 * face_size + 1.4)


## Apply relic modifiers and start the runner's current chamber.
func _begin_chamber() -> void:
	sim.set_relic_modifiers(
		runner.get_echo_extra(), runner.get_score_pct(), runner.get_extra_moves())
	sim.start_chamber(runner.get_chamber(), runner.get_chamber_seed())
	_refresh_all()
	_update_tray()


# ---------------------------------------------------------------- building --

func _build_environment() -> void:
	var sky_mat := ProceduralSkyMaterial.new()
	sky_mat.sky_top_color = Color("101528")
	sky_mat.sky_horizon_color = Color("2a3555")
	sky_mat.ground_bottom_color = Color("0a0d18")
	sky_mat.ground_horizon_color = Color("232b47")
	var sky := Sky.new()
	sky.sky_material = sky_mat
	var env := Environment.new()
	env.background_mode = Environment.BG_SKY
	env.sky = sky
	env.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	env.ambient_light_energy = 1.2
	var world_env := WorldEnvironment.new()
	world_env.environment = env
	add_child(world_env)

	# Three balanced lights for even face illumination.
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-45.0, 30.0, 0.0)
	sun.light_energy = 1.0
	sun.shadow_enabled = true
	add_child(sun)

	var fill1 := DirectionalLight3D.new()
	fill1.rotation_degrees = Vector3(45.0, -150.0, 0.0)
	fill1.light_energy = 0.5
	fill1.shadow_enabled = false
	add_child(fill1)

	var fill2 := DirectionalLight3D.new()
	fill2.rotation_degrees = Vector3(-45.0, -30.0, 0.0)
	fill2.light_energy = 0.5
	fill2.shadow_enabled = false
	add_child(fill2)


func _build_materials() -> void:
	for c in KIND_COLORS:
		var m := StandardMaterial3D.new()
		m.albedo_color = c
		m.roughness = 0.35
		m.metallic = 0.08
		kind_materials.append(m)

		var e := StandardMaterial3D.new()
		e.albedo_color = c.lerp(Color.WHITE, 0.35)
		e.roughness = 0.3
		e.emission_enabled = true
		e.emission = c
		e.emission_energy_multiplier = 0.9
		echo_materials.append(e)

	stone_material = StandardMaterial3D.new()
	stone_material.albedo_color = Color("6a6f78")
	stone_material.roughness = 0.95

	paper_material = StandardMaterial3D.new()
	paper_material.albedo_color = Color("f2efe6")
	paper_material.roughness = 0.75

	ice_material = StandardMaterial3D.new()
	ice_material.albedo_color = Color(0.62, 0.83, 0.98, 0.6)
	ice_material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	ice_material.roughness = 0.1
	ice_material.emission_enabled = true
	ice_material.emission = Color(0.3, 0.55, 0.85)
	ice_material.emission_energy_multiplier = 0.25

	plate_material = StandardMaterial3D.new()
	plate_material.albedo_color = Color("181d2e")
	plate_material.roughness = 0.85

	_build_icon_assets()


## Duotone icon quads: shared ShaderMaterial + QuadMesh (custom atlas UVs)
## per visual state.
func _build_icon_assets() -> void:
	var tint_for := {
		"kind0": Color("00e5ff"), "kind1": Color("ffb300"),
		"kind2": Color("00e676"), "kind3": Color("e040fb"),
		"special1": Color("ffe082"), "special2": Color("ffe082"),
		"special3": Color("ffe082"), "blocker1": Color("78909c"),
		"blocker2": Color("80deea"),
	}
	for key in STATE_ICONS:
		var rect: Rect2 = ATLAS_DB.icon_rect(STATE_ICONS[key])
		var mat := ShaderMaterial.new()
		mat.shader = DUOTONE_3D
		mat.set_shader_parameter("highlight_color", tint_for[key])
		mat.set_shader_parameter("echo_amount", 0.0)
		mat.set_shader_parameter("atlas_region",
			Vector4(rect.position.x / SHEET_TEX.get_width(),
				rect.position.y / SHEET_TEX.get_height(),
				rect.size.x / SHEET_TEX.get_width(),
				rect.size.y / SHEET_TEX.get_height()))
		icon_materials[key] = mat

		var mesh := QuadMesh.new()
		mesh.size = Vector2(CELL * 0.66, CELL * 0.66)
		var tex_mat := mat
		tex_mat.set_shader_parameter("source_tex", SHEET_TEX)
		icon_meshes[key] = mesh


func _build_face(f: int) -> void:
	var half := face_size * CELL * 0.5
	var holder := Node3D.new()
	holder.name = "Face%d" % f
	var basis := Basis(U_AXES[f], V_AXES[f], NORMALS[f])
	holder.transform = Transform3D(basis, NORMALS[f] * half)
	faces_root.add_child(holder)
	holders.append(holder)
	rest_transforms.append(holder.transform)

	var plate := MeshInstance3D.new()
	var plate_mesh := BoxMesh.new()
	plate_mesh.size = Vector3(face_size * CELL, face_size * CELL, 0.16)
	plate.mesh = plate_mesh
	plate.material_override = plate_material
	plate.position = Vector3(0, 0, -0.11)
	holder.add_child(plate)

	var body := StaticBody3D.new()
	body.set_meta("face_id", f)
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(face_size * CELL, face_size * CELL, 0.3)
	shape.shape = box
	shape.position = Vector3(0, 0, -0.05)
	body.add_child(shape)
	holder.add_child(body)

	for y in face_size:
		for x in face_size:
			var gem := MeshInstance3D.new()
			var mesh := BoxMesh.new()
			mesh.size = Vector3(CELL * 0.84, CELL * 0.84, 0.18)
			gem.mesh = mesh
			gem.position = _cell_local(x, y)
			holder.add_child(gem)
			gem_nodes[Vector3i(f, x, y)] = gem

			var icon := MeshInstance3D.new()
			icon.name = "Icon"
			icon.position = Vector3(0, 0, 0.11)
			gem.add_child(icon)


func _build_hud() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)

	status_label = Label.new()
	status_label.text = ""
	status_label.position = Vector2(16, 12)
	status_label.add_theme_font_size_override("font_size", 20)
	layer.add_child(status_label)

	var hint := Label.new()
	hint.text = "A/D turn  ·  W/S pitch  ·  Q/E tumble  ·  Click swap  ·  1-4 board size  ·  R restart descent"
	hint.position = Vector2(16, 44)
	hint.add_theme_font_size_override("font_size", 15)
	hint.modulate = Color(1, 1, 1, 0.75)
	layer.add_child(hint)

	stats_label = Label.new()
	stats_label.position = Vector2(16, 72)
	stats_label.add_theme_font_size_override("font_size", 16)
	stats_label.add_theme_color_override("font_color", Color("ffd76a"))
	layer.add_child(stats_label)

	# Relic tray (held relic badges with hover tooltips).
	tray_box = HBoxContainer.new()
	tray_box.position = Vector2(16, 100)
	tray_box.add_theme_constant_override("separation", 6)
	layer.add_child(tray_box)

	relic_ui = RelicSelection.new()
	relic_ui.relic_chosen.connect(_on_relic_chosen)
	add_child(relic_ui)

	_build_debug_effects_ui(layer)

	var touch := CubeTouchControls.new()
	touch.turn_left.connect(func(): _try_turn(-1))
	touch.turn_right.connect(func(): _try_turn(1))
	touch.pitch_up.connect(func(): _try_pitch(true))
	touch.pitch_down.connect(func(): _try_pitch(false))
	touch.spin_ccw.connect(func(): _try_tumble(false))
	touch.spin_cw.connect(func(): _try_tumble(true))
	touch.size_selected.connect(func(n: int): start_descent(n))
	add_child(touch)


func _build_debug_effects_ui(layer: CanvasLayer) -> void:
	var panel := PanelContainer.new()
	panel.name = "DebugEffects"
	var bg := StyleBoxFlat.new()
	bg.bg_color = Color(0.07, 0.09, 0.16, 0.88)
	bg.set_corner_radius_all(8)
	bg.set_content_margin_all(10)
	panel.add_theme_stylebox_override("panel", bg)
	panel.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	panel.position = Vector2(-220, 96)
	panel.size = Vector2(208, 0)
	layer.add_child(panel)

	var vbox := VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 6)
	panel.add_child(vbox)

	var header := Label.new()
	header.text = "EFFECTS (DEBUG)"
	header.add_theme_font_size_override("font_size", 13)
	header.modulate = Color(1, 1, 1, 0.55)
	vbox.add_child(header)

	var toggles := [
		{"flag": "_debug_shatter", "label": "Shatter burst"},
		{"flag": "_debug_beam", "label": "Antipodal beam"},
		{"flag": "_debug_shockwave", "label": "Shockwave"},
		{"flag": "_debug_selection", "label": "Selection scale"},
		{"flag": "_debug_facespin", "label": "Face spin tumble"},
	]
	for t in toggles:
		var h := HBoxContainer.new()
		var cb := CheckBox.new()
		cb.button_pressed = false
		var flag_name: String = t.flag
		var cb2 := cb
		cb.toggled.connect(func(on: bool):
			set(flag_name, on)
		)
		h.add_child(cb)
		var lbl := Label.new()
		lbl.text = t.label
		lbl.add_theme_font_size_override("font_size", 14)
		h.add_child(lbl)
		vbox.add_child(h)

	# Zoom slider.
	var zsep := Label.new()
	zsep.text = "---"
	zsep.modulate = Color(1, 1, 1, 0.2)
	vbox.add_child(zsep)

	var zheader := Label.new()
	zheader.text = "ZOOM"
	zheader.add_theme_font_size_override("font_size", 13)
	zheader.modulate = Color(1, 1, 1, 0.55)
	vbox.add_child(zheader)

	var zslider := HSlider.new()
	zslider.min_value = 0.5
	zslider.max_value = 2.0
	zslider.step = 0.05
	zslider.value = 1.0
	zslider.custom_minimum_size = Vector2(180, 44)
	zslider.add_theme_constant_override("slider_height", 12)
	zslider.add_theme_constant_override("grabber_width", 28)
	zslider.add_theme_constant_override("grabber_height", 28)
	zslider.value_changed.connect(func(v: float):
		camera.set_zoom_factor(v)
	)
	vbox.add_child(zslider)

	var zval := Label.new()
	zval.text = "1.0x"
	zval.add_theme_font_size_override("font_size", 13)
	zval.modulate = Color(1, 1, 1, 0.7)
	zval.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	zslider.value_changed.connect(func(v: float):
		zval.text = "%.1fx" % v
	)
	vbox.add_child(zval)


func _update_tray() -> void:
	for child in tray_box.get_children():
		child.queue_free()
	held_relics = runner.get_held_relics() if runner and runner.is_running() else []
	for relic in held_relics:
		var badge := PanelContainer.new()
		var style := StyleBoxFlat.new()
		style.bg_color = Color("232c4e")
		style.set_corner_radius_all(6)
		style.content_margin_left = 8.0
		style.content_margin_right = 8.0
		style.content_margin_top = 3.0
		style.content_margin_bottom = 3.0
		badge.add_theme_stylebox_override("panel", style)
		var label := Label.new()
		label.text = "◆ %s" % str(relic.get("name", ""))
		label.add_theme_font_size_override("font_size", 13)
		badge.add_child(label)
		badge.tooltip_text = "%s (%s): %s" % [
			str(relic.get("name", "")),
			str(relic.get("rarity", "")),
			str(relic.get("description", "")),
		]
		tray_box.add_child(badge)


func _cell_local(x: int, y: int) -> Vector3:
	var c := (face_size - 1) * 0.5
	return Vector3((x - c) * CELL, (y - c) * CELL, 0.09)


# ------------------------------------------------------------------ render --

## Map a cell dict to an icon state key ("kind0..3", "special1..3", "blocker1/2").
func _state_key_for(d: Dictionary) -> String:
	var blocker := int(d.get("blocker", 0))
	if blocker > 0:
		return "blocker%d" % blocker
	var special := int(d.get("special", 0))
	if special > 0:
		return "special%d" % special
	return "kind%d" % (int(d.get("kind", 0)) % 4)


func _refresh_cell(f: int, x: int, y: int) -> void:
	var key := Vector3i(f, x, y)
	var gem: MeshInstance3D = gem_nodes[key]
	var d := sim.get_face_cell(f, x, y)
	if d.get("empty", true):
		gem.visible = false
		return
	gem.visible = true
	# Paper card body; blockers tint the body instead of the stamp.
	if int(d.get("blocker", 0)) == 1:
		gem.material_override = stone_material
	elif int(d.get("blocker", 0)) == 2:
		gem.material_override = ice_material
	else:
		gem.material_override = paper_material

	var state := _state_key_for(d)
	var icon := gem.get_node("Icon") as MeshInstance3D
	icon.mesh = icon_meshes[state]
	icon.material_override = icon_materials[state]
	var echoing: bool = d.get("has_echo", false)
	icon_materials[state].set_shader_parameter(
		"echo_amount", 1.0 if (echoing or int(d.get("special", 0)) > 0) else 0.0)


func _refresh_all() -> void:
	for f in 6:
		for y in face_size:
			for x in face_size:
				_refresh_cell(f, x, y)


# ------------------------------------------------------------------ events --

var _refresh_timer: Timer

func _on_match_resolved(face: int, cleared_cells: Array, gem_kind: int, _cascade_depth: int) -> void:
	# Cleared-cell coordinates are face-local per the FFI contract.
	var color: Color = KIND_COLORS[gem_kind % 4]
	for v in cleared_cells:
		var pos: Vector2i = v
		var key := Vector3i(face, pos.x, pos.y)
		if gem_nodes.has(key):
			var gem: MeshInstance3D = gem_nodes[key]
			var tw := create_tween()
			tw.tween_property(gem, "scale", Vector3.ONE * 0.02, CLEAR_DURATION) \
				.set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
		_spawn_shatter(face, pos, color)
	_schedule_refresh()


## One-shot gem shatter burst (GPUParticles3D), blowing out normal to the
## face plane.
func _spawn_shatter(face: int, pos: Vector2i, color: Color) -> void:
	if face < 0 or face >= 6:
		return
	if not _debug_shatter:
		return
	var p := GPUParticles3D.new()
	p.one_shot = true
	p.emitting = true
	p.explosiveness = 1.0
	p.amount = 14
	p.lifetime = 0.55
	p.local_coords = true
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3(0, 0, 1)
	pm.spread = 65.0
	pm.initial_velocity_min = 1.6
	pm.initial_velocity_max = 3.4
	pm.gravity = Vector3(0, 0, -5.0)
	pm.scale_min = 0.5
	pm.scale_max = 1.0
	pm.color = color
	p.process_material = pm
	var mesh := QuadMesh.new()
	mesh.size = Vector2(0.16, 0.16)
	var m := StandardMaterial3D.new()
	m.vertex_color_use_as_albedo = true
	m.emission_enabled = true
	m.emission = color
	m.emission_energy_multiplier = 1.2
	m.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mesh.material = m
	p.draw_pass_1 = mesh
	p.position = _cell_local(pos.x, pos.y) + Vector3(0, 0, 0.1)
	holders[face].add_child(p)
	get_tree().create_timer(1.1).timeout.connect(p.queue_free)


## Antipodal Resonance Beam: energy lance from a detonated cell through the
## cube center, striking the exact opposite face cell.
func _spawn_antipodal_beam(origin_face: int, pos: Vector2i) -> void:
	if origin_face < 0 or origin_face >= 6:
		return
	if not _debug_beam:
		return
	var target_face: int = ANTIPODE_FACE[origin_face]
	var target_pos := pos
	if origin_face <= 3:
		target_pos = Vector2i(face_size - 1 - pos.x, face_size - 1 - pos.y)
	var start := holders[origin_face].to_global(_cell_local(pos.x, pos.y))
	var end := holders[target_face].to_global(_cell_local(target_pos.x, target_pos.y))
	# Push endpoints slightly past the surface so the lance visibly enters/exits.
	start += NORMALS[origin_face] * 0.45
	end += NORMALS[target_face] * 0.45

	var beam := MeshInstance3D.new()
	var mesh := CylinderMesh.new()
	mesh.height = start.distance_to(end)
	mesh.top_radius = 0.085
	mesh.bottom_radius = 0.085
	beam.mesh = mesh
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(0.5, 0.7, 1.0, 0.85)
	m.emission_enabled = true
	m.emission = Color(0.45, 0.65, 1.0)
	m.emission_energy_multiplier = 3.0
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	# X-ray: the lance phases through the cube body, always visible.
	m.no_depth_test = true
	m.render_priority = 10
	beam.material_override = m
	# Extra render priority on the geometry instance too.
	beam.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(beam)
	beam.global_position = (start + end) * 0.5
	beam.global_transform.basis = Basis(Quaternion(Vector3.UP, (end - start).normalized()))

	var tw := create_tween()
	tw.set_parallel(true)
	tw.tween_property(m, "albedo_color:a", 0.0, 0.55)
	tw.tween_property(beam, "scale", Vector3(0.15, 1.0, 0.15), 0.55)
	tw.chain().tween_callback(beam.queue_free)

	_spawn_shockwave(target_face, target_pos, Color(0.45, 0.65, 1.0))


func _on_echo_detonated(face: int, cells: Array, multiplier: float) -> void:
	for v in cells:
		var pos: Vector2i = v
		_spawn_shockwave(face, pos, Color(1.0, 0.85, 0.25))
		_spawn_antipodal_beam(face, pos)
	_update_status("Echo detonation ×%.1f" % multiplier)


func _on_antipodal_charged(target_face: int, cells: Array) -> void:
	for v in cells:
		var pos: Vector2i = v
		_spawn_shockwave(target_face, pos, Color(0.45, 0.65, 1.0))
	_update_status("Antipodal resonance strikes face %d" % target_face)


func _on_special_created(face: int, pos: Vector2i, kind: int) -> void:
	var key := Vector3i(face, pos.x, pos.y)
	if not gem_nodes.has(key):
		return
	var gem: MeshInstance3D = gem_nodes[key]
	var names := {1: "⚡ Bolt", 2: "✦ Prism", 3: "💥 Nova"}
	_spawn_shockwave(face, pos, Color(1, 1, 1))
	_update_status("%s created" % names.get(kind, "Special"))


func _schedule_refresh() -> void:
	if _refresh_timer == null:
		_refresh_timer = Timer.new()
		_refresh_timer.one_shot = true
		_refresh_timer.wait_time = REFRESH_DELAY
		_refresh_timer.timeout.connect(_on_refresh_timer)
		add_child(_refresh_timer)
	_refresh_timer.start()


func _on_chamber_finished(chamber: int, cleared: bool) -> void:
	busy = true
	if cleared:
		if chamber >= DESCENT_LENGTH:
			_update_status("DESCENT COMPLETE — %d relic(s) held. Press R for a new run." % held_relics.size())
		else:
			_update_status("Chamber %d cleared! Choose a relic…" % chamber)
			var offers: Array = runner.next_draft()
			relic_ui.show_offers(offers)
	else:
		_update_status("Descent failed — press R to restart")


func _on_relic_chosen(id: String) -> void:
	runner.choose_relic(id)
	runner.advance_chamber()
	relic_ui.hide_offers()
	_begin_chamber()
	busy = false
	_update_status("Chamber %d — %d relic(s) held" % [runner.get_chamber(), runner.get_relic_count()])


func _on_refresh_timer() -> void:
	for key in gem_nodes:
		var gem: MeshInstance3D = gem_nodes[key]
		gem.scale = Vector3.ONE
	_refresh_all()
	busy = false
	_update_status("")


## One-shot expanding translucent panel on a face cell position (juice).
func _spawn_shockwave(face: int, pos: Vector2i, color: Color) -> void:
	if face < 0 or face >= 6:
		return
	if not _debug_shockwave:
		return
	var key := Vector3i(face, pos.x, pos.y)
	var origin_local: Vector3 = _cell_local(pos.x, pos.y)
	var fx := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = Vector3(CELL, CELL, 0.06)
	fx.mesh = mesh
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(color.r, color.g, color.b, 0.55)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	fx.material_override = m
	fx.position = origin_local + Vector3(0, 0, 0.25)
	holders[face].add_child(fx)
	var tw := create_tween()
	tw.set_parallel(true)
	tw.tween_property(fx, "scale", Vector3(2.6, 2.6, 1.0), 0.4) \
		.set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tw.tween_property(m, "albedo_color:a", 0.0, 0.4)
	tw.chain().tween_callback(fx.queue_free)


# ------------------------------------------------------------------- input --

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		match event.physical_keycode:
			KEY_A, KEY_LEFT: _try_turn(-1)
			KEY_D, KEY_RIGHT: _try_turn(1)
			KEY_W, KEY_UP: _try_pitch(true)
			KEY_S, KEY_DOWN: _try_pitch(false)
			KEY_Q: _try_tumble(false)
			KEY_E: _try_tumble(true)
			KEY_1: start_descent(4)
			KEY_2: start_descent(6)
			KEY_3: start_descent(8)
			KEY_4: start_descent(10)
			KEY_R: start_descent(face_size)
	elif event is InputEventMouseButton and event.pressed \
			and event.button_index == MOUSE_BUTTON_LEFT:
		# On touch devices Godot synthesizes mouse events from taps; handling
		# both would select+deselect in a single tap. Touch = ScreenTouch only.
		if not _touch_device:
			_handle_click(event.position)
	elif event is InputEventScreenTouch and event.pressed:
		# Pinch-to-zoom: track first two fingers.
		if _pinch_touch1 == -1:
			_pinch_touch1 = event.index
			_pinch_touch1_pos = event.position
		elif _pinch_touch2 == -1 and event.index != _pinch_touch1:
			_pinch_touch2 = event.index
			_pinch_touch2_pos = event.position
			_pinch_start_dist = _pinch_touch1_pos.distance_to(_pinch_touch2_pos)
			_pinch_start_zoom = camera.zoom_factor
		# Also handle drag/swipe on first finger.
		if event.index == _pinch_touch1:
			_drag_start = event.position
			_drag_gem_face = -1
			_drag_gem_cell = Vector2i(-1, -1)
			var hit := _pick_face_cell(event.position)
			if not hit.is_empty():
				_drag_gem_face = hit.face
				_drag_gem_cell = Vector2i(hit.x, hit.y)
	elif event is InputEventScreenTouch and not event.pressed:
		# Finger released.
		if event.index == _pinch_touch1:
			_pinch_touch1 = -1
			_pinch_touch2 = -1
			_finalize_drag(event.position)
		elif event.index == _pinch_touch2:
			_pinch_touch2 = -1
	elif event is InputEventScreenDrag:
		# Update pinch zoom if two fingers.
		if _pinch_touch1 != -1 and _pinch_touch2 != -1:
			# Check if this drag event is from one of our tracked fingers.
			if event.index == _pinch_touch1:
				_pinch_touch1_pos = event.position
			elif event.index == _pinch_touch2:
				_pinch_touch2_pos = event.position
			var d: float = _pinch_touch1_pos.distance_to(_pinch_touch2_pos)
			var factor := d / _pinch_start_dist
			var new_zoom := clampf(_pinch_start_zoom * factor, 0.5, 2.0)
			camera.set_zoom_factor(new_zoom)
			# Update the HUD slider if it exists.
			var panel := get_node_or_null("CanvasLayer/DebugEffects")
			if panel:
				var slider := panel.find_child("HSlider")
				if slider:
					slider.value = new_zoom
		else:
			_update_drag(event.position)


func _finalize_drag(final_pos: Vector2) -> void:
	var delta = final_pos - _drag_start
	if delta.length_squared() < 24 * 24: # tap threshold ~24px
		_handle_click(final_pos)
		return
	# Drag — compute end cell
	var end_hit = _pick_face_cell(final_pos)
	var end_face = -1
	var end_cell = Vector2i(-1, -1)
	if not end_hit.is_empty():
		end_face = end_hit.face
		end_cell = Vector2i(end_hit.x, end_hit.y)
	# Same face drag (started on a gem, ended on same face)?
	if _drag_gem_face >= 0 and end_face == _drag_gem_face:
		var dx = int(end_cell.x) - int(_drag_gem_cell.x)
		var dy = int(end_cell.y) - int(_drag_gem_cell.y)
		if abs(dx) + abs(dy) == 1:
			_attempt_swap(_drag_gem_face, _drag_gem_cell.x, _drag_gem_cell.y, end_cell.x, end_cell.y)
		else:
			# Deselect and reselect the new cell
			_clear_selection()
			if not end_hit.is_empty():
				selected = {"face": end_face, "x": end_cell.x, "y": end_cell.y}
				_apply_selection_scale()
	else:
		# Drag started on empty area OR cross-face: camera turn.
		# Simple heuristic: horizontal drag → yaw, vertical → pitch.
		var dx = delta.x
		var dy = delta.y
		if abs(dx) > abs(dy):
			_try_turn(sign(dx))
		else:
			_try_pitch(dy > 0)

func _update_drag(pos: Vector2) -> void:
	# No 3D preview needed; distance test on release decides tap vs swap.
	pass


func _try_turn(dir: int) -> void:
	if busy or camera.is_turning():
		return
	_clear_selection()
	camera.turn_yaw(dir)


func _try_pitch(up: bool) -> void:
	if busy or camera.is_turning():
		return
	_clear_selection()
	if up:
		camera.pitch_up()
	else:
		camera.pitch_down()


func _try_tumble(clockwise: bool) -> void:
	if busy or camera.is_turning():
		return
	var f := _active_face_id()
	busy = true
	if not sim.rotate_face_gravity(f, clockwise):
		busy = false
		return
	_clear_selection()
	var holder := holders[f]
	if not _debug_facespin:
		# Instant snap — no tween animation.
		holder.transform = rest_transforms[f]
		_on_refresh_timer()
		return
	var q0: Quaternion = holder.transform.basis.orthonormalized().get_rotation_quaternion()
	var angle := deg_to_rad(90.0) * (1.0 if clockwise else -1.0)
	var q1: Quaternion = (holder.transform.basis * Basis(Vector3(0, 0, 1), angle)) \
		.orthonormalized().get_rotation_quaternion()
	var tw := create_tween()
	tw.tween_method(
		func(t: float): holder.transform.basis = Basis(q0.slerp(q1, t)),
		0.0, 1.0, SPIN_DURATION
	).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tw.finished.connect(func():
		holder.transform = rest_transforms[f]
		_on_refresh_timer()
	)


func _handle_click(screen_pos: Vector2) -> void:
	if busy or camera.is_turning():
		return
	var hit := _pick_face_cell(screen_pos)
	if hit.is_empty():
		_clear_selection()
		return
	var face: int = hit.face
	var x: int = hit.x
	var y: int = hit.y

	if selected.is_empty():
		selected = {"face": face, "x": x, "y": y}
		_apply_selection_scale()
		return

	var same_cell := int(selected.face) == face \
		and int(selected.x) == x and int(selected.y) == y
	if int(selected.face) == face and not same_cell:
		var ax: int = int(selected.x)
		var ay: int = int(selected.y)
		if absi(ax - x) + absi(ay - y) == 1:
			_attempt_swap(face, ax, ay, x, y)
			return

	# Re-select a different cell; clicking the same cell deselects.
	_clear_selection()
	if not same_cell:
		selected = {"face": face, "x": x, "y": y}
		_apply_selection_scale()


func _attempt_swap(face: int, ax: int, ay: int, bx: int, by: int) -> void:
	busy = true
	_clear_selection()
	var key_a := Vector3i(face, ax, ay)
	var key_b := Vector3i(face, bx, by)
	var gem_a: MeshInstance3D = gem_nodes[key_a]
	var gem_b: MeshInstance3D = gem_nodes[key_b]
	var pos_a := gem_a.position
	var pos_b := gem_b.position
	# Animate the visual swap.
	var tw := create_tween()
	tw.set_parallel(true)
	tw.tween_property(gem_a, "position", pos_b, 0.12).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tw.tween_property(gem_b, "position", pos_a, 0.12).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tw.finished.connect(func():
		# After visual swap, apply the logical swap.
		if sim.try_face_swap(face, ax, ay, bx, by):
			# Clear animations arrive via cube_match_resolved; refresh timer unlocks.
			pass
		else:
			# Revert visual swap on reject.
			var tw2 := create_tween()
			tw2.set_parallel(true)
			tw2.tween_property(gem_a, "position", pos_a, 0.1).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
			tw2.tween_property(gem_b, "position", pos_b, 0.1).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
			_flash_reject(face, Vector2i(ax, ay), Vector2i(bx, by))
			busy = false
	)


func _flash_reject(face: int, a: Vector2i, b: Vector2i) -> void:
	if not _debug_selection:
		return
	for pos in [a, b]:
		var key := Vector3i(face, pos.x, pos.y)
		if not gem_nodes.has(key):
			continue
		var gem: MeshInstance3D = gem_nodes[key]
		var tw := create_tween()
		tw.tween_property(gem, "scale", Vector3(1.15, 1.15, 1.15), 0.07)
		tw.tween_property(gem, "scale", Vector3.ONE, 0.15)
	_update_status("Illegal move")


func _apply_selection_scale() -> void:
	if selected.is_empty():
		return
	if not _debug_selection:
		return
	var key := Vector3i(int(selected.face), int(selected.x), int(selected.y))
	if gem_nodes.has(key):
		var gem: MeshInstance3D = gem_nodes[key]
		var tw := create_tween()
		tw.tween_property(gem, "scale", Vector3(1.18, 1.18, 1.3), 0.08)


func _clear_selection() -> void:
	if selected.is_empty():
		return
	var key := Vector3i(int(selected.face), int(selected.x), int(selected.y))
	if gem_nodes.has(key):
		var gem: MeshInstance3D = gem_nodes[key]
		var tw := create_tween()
		tw.tween_property(gem, "scale", Vector3.ONE, 0.08)
	selected = {}


# ------------------------------------------------------------------ picking --

func _pick_face_cell(screen_pos: Vector2) -> Dictionary:
	var space := get_world_3d().direct_space_state
	var from := camera.project_ray_origin(screen_pos)
	var dir := camera.project_ray_normal(screen_pos)
	var query := PhysicsRayQueryParameters3D.create(from, from + dir * 100.0)
	var result := space.intersect_ray(query)
	if result.is_empty() or not result.collider is StaticBody3D:
		return {}
	var body: StaticBody3D = result.collider
	if not body.has_meta("face_id"):
		return {}
	var face: int = body.get_meta("face_id")
	var local := holders[face].to_local(result.position)
	var x := clampi(int(floor(local.x / CELL + face_size * 0.5)), 0, face_size - 1)
	var y := clampi(int(floor(local.y / CELL + face_size * 0.5)), 0, face_size - 1)
	return {"face": face, "x": x, "y": y}


func _active_face_id() -> int:
	return camera.active_face()


func _update_status(text: String) -> void:
	if status_label:
		status_label.text = text
		if text != "":
			_transient_until = Time.get_ticks_msec() + 2200


func _process(_delta: float) -> void:
	var names := ["Front", "Right", "Back", "Left", "Top", "Bottom"]
	_update_status_label_if_idle(names[_active_face_id()])
	if stats_label and sim:
		stats_label.text = "Chamber %d/%d   ·   Score %d/%d   ·   Moves %d" % [
			sim.get_chamber(), DESCENT_LENGTH,
			sim.get_score(), sim.get_score_target(),
			sim.get_moves_remaining(),
		]


var _last_face_name := ""
var _transient_until := 0

func _update_status_label_if_idle(face_name: String) -> void:
	if busy or status_label == null:
		return
	if Time.get_ticks_msec() < _transient_until:
		return # let transient messages (echo/reject) linger briefly
	var wanted := "Active face: %s" % face_name
	if _last_face_name != wanted:
		_last_face_name = wanted
		status_label.text = wanted
