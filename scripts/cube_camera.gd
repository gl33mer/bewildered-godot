extends Camera3D
class_name CubeSnapCamera

## Snap-turn orbit camera for the 3D cube chamber.
## Yaws in 90-degree steps around the cube (Front -> Right -> Back -> Left)
## and pitches in steps between Bottom / equator / Top. All turns are smooth
## tweens (0.22s, TRANS_QUAD). The camera never rolls: it always orbits the
## cube center and looks at it with world-up kept as close as possible.

signal turn_finished

const TURN_DURATION := 0.22
const PITCH_STEP := 45.0
const MAX_ELEVATION := 78.0
const DEFAULT_ELEVATION := 22.0

var azimuth_deg := 0.0
var elevation_deg := DEFAULT_ELEVATION
var distance := 14.0
var base_distance := 14.0
var zoom_factor := 1.0

var _tween: Tween


func _ready() -> void:
	_apply()


func is_turning() -> bool:
	return _tween != null and _tween.is_valid() and _tween.is_running()


## Adjust orbit distance (dynamic scaling for variable face sizes).
func set_distance(d: float) -> void:
	distance = maxf(d, 3.0)
	base_distance = distance
	zoom_factor = 1.0
	_apply()


func set_zoom_factor(zf: float) -> void:
	zoom_factor = zf
	distance = base_distance * zoom_factor
	_apply()


## Yaw one 90-degree step. dir > 0 cycles the viewed face Front -> Right ->
## Back -> Left; dir < 0 goes back.
func turn_yaw(dir: int) -> void:
	_go_to(azimuth_deg + 90.0 * float(dir), elevation_deg)


func pitch_up() -> void:
	_go_to(azimuth_deg, minf(elevation_deg + PITCH_STEP, MAX_ELEVATION))


func pitch_down() -> void:
	_go_to(azimuth_deg, maxf(elevation_deg - PITCH_STEP, -MAX_ELEVATION))


## The cube face currently presented to the camera:
## 0=Front 1=Right 2=Back 3=Left 4=Top 5=Bottom
func active_face() -> int:
	if elevation_deg > 60.0:
		return 4
	if elevation_deg < -60.0:
		return 5
	var a := wrapf(azimuth_deg, 0.0, 360.0)
	var sector := wrapi(roundi(a / 90.0), 0, 4)
	return [0, 1, 2, 3][sector]


## Snap the view to present a given face (used after antipodal flashes etc).
func snap_to_face(face: int) -> void:
	var side_azimuth := {0: 0.0, 1: 90.0, 2: 180.0, 3: 270.0}
	if face <= 3:
		_go_to(side_azimuth[face], DEFAULT_ELEVATION)
	elif face == 4:
		_go_to(azimuth_deg, MAX_ELEVATION)
	else:
		_go_to(azimuth_deg, -MAX_ELEVATION)


func _go_to(new_azimuth: float, new_elevation: float) -> void:
	if _tween and _tween.is_valid():
		_tween.kill()
	var from := Vector2(azimuth_deg, elevation_deg)
	var to := Vector2(new_azimuth, new_elevation)
	# Shortest angular path for yaw.
	to.x = from.x + wrapf(to.x - from.x, -180.0, 180.0)
	_tween = create_tween()
	_tween.tween_method(_set_angles, from, to, TURN_DURATION) \
		.set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	_tween.finished.connect(func(): turn_finished.emit(), CONNECT_ONE_SHOT)


func _set_angles(v: Vector2) -> void:
	azimuth_deg = v.x
	elevation_deg = v.y
	_apply()


func _apply() -> void:
	var az := deg_to_rad(azimuth_deg)
	var el := deg_to_rad(elevation_deg)
	position = Vector3(
		sin(az) * cos(el),
		sin(el),
		cos(az) * cos(el)
	) * distance
	look_at(Vector3.ZERO, Vector3.UP)
