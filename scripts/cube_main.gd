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

var selected := {} # {face, x, y}
var busy := false
var status_label: Label
var faces_root: Node3D

const ANTIPODE_FACE := [2, 3, 0, 1, 5, 4]


func _ready() -> void:
	_build_environment()
	_build_materials()

	camera = CubeSnapCamera.new()
	add_child(camera)

	_build_hud()

	_start_chamber(face_size)


## (Re)build the whole chamber for a face size (4..10). Keys 1-4 switch live.
func _start_chamber(n: int) -> void:
	face_size = clampi(n, 4, 10)
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
	sim.cube_match_resolved.connect(_on_match_resolved)
	sim.cube_echo_detonated.connect(_on_echo_detonated)
	sim.antipodal_echo_charged.connect(_on_antipodal_charged)
	sim.cube_special_gem_created.connect(_on_special_created)

	faces_root = Node3D.new()
	faces_root.name = "Faces"
	add_child(faces_root)
	for f in 6:
		_build_face(f)

	camera.set_distance(2.35 * face_size + 1.4)
	_refresh_all()


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
	env.ambient_light_energy = 0.7
	var world_env := WorldEnvironment.new()
	world_env.environment = env
	add_child(world_env)

	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-52.0, 32.0, 0.0)
	sun.light_energy = 1.25
	sun.shadow_enabled = true
	add_child(sun)

	var fill := DirectionalLight3D.new()
	fill.rotation_degrees = Vector3(30.0, -140.0, 0.0)
	fill.light_energy = 0.35
	fill.shadow_enabled = false
	add_child(fill)


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

			var glyph := Label3D.new()
			glyph.name = "Glyph"
			glyph.font_size = 220
			glyph.pixel_size = 0.0022
			glyph.outline_size = 40
			glyph.modulate = Color.WHITE
			glyph.position = Vector3(0, 0, 0.13)
			gem.add_child(glyph)


func _build_hud() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)

	status_label = Label.new()
	status_label.text = ""
	status_label.position = Vector2(16, 12)
	status_label.add_theme_font_size_override("font_size", 20)
	layer.add_child(status_label)

	var hint := Label.new()
	hint.text = "A/D turn  ·  W/S pitch  ·  Q/E tumble  ·  Click swap  ·  1-4 board size (4/6/8/10)"
	hint.position = Vector2(16, 44)
	hint.add_theme_font_size_override("font_size", 15)
	hint.modulate = Color(1, 1, 1, 0.75)
	layer.add_child(hint)


func _cell_local(x: int, y: int) -> Vector3:
	var c := (face_size - 1) * 0.5
	return Vector3((x - c) * CELL, (y - c) * CELL, 0.09)


# ------------------------------------------------------------------ render --

func _material_for(d: Dictionary) -> StandardMaterial3D:
	if d.get("blocker", 0) == 1:
		return stone_material
	if d.get("blocker", 0) == 2:
		return ice_material
	var kind: int = d.get("kind", 0)
	if d.get("has_echo", false):
		return echo_materials[kind % 4]
	return kind_materials[kind % 4]


func _glyph_for(d: Dictionary) -> String:
	match int(d.get("blocker", 0)):
		1: return "🪨"
		2: return "🧊"
	match int(d.get("special", 0)):
		1: return "⚡"
		2: return "✦"
		3: return "💥"
	return ""


func _refresh_cell(f: int, x: int, y: int) -> void:
	var key := Vector3i(f, x, y)
	var gem: MeshInstance3D = gem_nodes[key]
	var d := sim.get_face_cell(f, x, y)
	if d.get("empty", true):
		gem.visible = false
		return
	gem.visible = true
	gem.material_override = _material_for(d)
	var glyph: Label3D = gem.get_node("Glyph")
	glyph.text = _glyph_for(d)
	glyph.modulate = Color.YELLOW if int(d.get("blocker", 0)) == 0 else Color.WHITE


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


## One-shot gem shatter burst, blowing out normal to the face plane.
func _spawn_shatter(face: int, pos: Vector2i, color: Color) -> void:
	if face < 0 or face >= 6:
		return
	var p := CPUParticles3D.new()
	p.one_shot = true
	p.emitting = true
	p.explosiveness = 1.0
	p.amount = 14
	p.lifetime = 0.55
	p.local_coords = true
	p.direction = Vector3(0, 0, 1)
	p.spread = 65.0
	p.initial_velocity_min = 1.6
	p.initial_velocity_max = 3.4
	p.gravity = Vector3(0, 0, -5.0)
	p.scale_amount_min = 0.5
	p.scale_amount_max = 1.0
	var mesh := QuadMesh.new()
	mesh.size = Vector2(0.16, 0.16)
	var m := StandardMaterial3D.new()
	m.albedo_color = color
	m.emission_enabled = true
	m.emission = color
	m.emission_energy_multiplier = 1.2
	m.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mesh.material = m
	p.mesh = mesh
	p.position = _cell_local(pos.x, pos.y) + Vector3(0, 0, 0.1)
	holders[face].add_child(p)
	get_tree().create_timer(1.1).timeout.connect(p.queue_free)


## Antipodal Resonance Beam: energy lance from a detonated cell through the
## cube center, striking the exact opposite face cell.
func _spawn_antipodal_beam(origin_face: int, pos: Vector2i) -> void:
	if origin_face < 0 or origin_face >= 6:
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
			KEY_1: _start_chamber(4)
			KEY_2: _start_chamber(6)
			KEY_3: _start_chamber(8)
			KEY_4: _start_chamber(10)
	elif event is InputEventMouseButton and event.pressed \
			and event.button_index == MOUSE_BUTTON_LEFT:
		_handle_click(event.position)


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
	if sim.try_face_swap(face, ax, ay, bx, by):
		# Clear animations arrive via cube_match_resolved; refresh timer unlocks.
		pass
	else:
		_flash_reject(face, Vector2i(ax, ay), Vector2i(bx, by))
		busy = false


func _flash_reject(face: int, a: Vector2i, b: Vector2i) -> void:
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
