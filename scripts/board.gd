extends Node2D

class_name GameBoard

@export var board_width: int = 8
@export var board_height: int = 8
@export var seed: int = 12345
@export var cell_size: float = 64.0
@export var padding: float = 8.0

@onready var board_sim: RefCounted = BoardSim.new()
@onready var gem_scene: PackedScene = preload("res://scenes/gem.tscn")

var gem_instances: Array[Node2D] = []

func _ready():
	_initialize_board()

func _initialize_board() -> void:
	# Initialize the Rust board simulation
	board_sim.new_board(board_width, board_height, seed)
	
	# Clear any existing gems
	for gem in gem_instances:
		if gem != null && is_instance_valid(gem):
			gem.queue_free()
	gem_instances.clear()
	gem_instances.resize(board_width * board_height)
	
	# Calculate board offset to center
	var board_pixel_width = board_width * cell_size + (board_width - 1) * padding
	var board_pixel_height = board_height * cell_size + (board_height - 1) * padding
	var offset_x = -board_pixel_width / 2.0 + cell_size / 2.0
	var offset_y = -board_pixel_height / 2.0 + cell_size / 2.0
	
	# Create gem instances for each cell
	for y in range(board_height):
		for x in range(board_width):
			var cell = board_sim.get_cell(x, y)
			if not cell.empty:
				var kind = cell.kind
				var has_echo = cell.has_echo
				
				var gem_instance = gem_scene.instantiate()
				gem_instance.set_gem(kind, has_echo)
				
				var pos_x = offset_x + x * (cell_size + padding)
				var pos_y = offset_y + y * (cell_size + padding)
				gem_instance.position = Vector2(pos_x, pos_y)
				
				add_child(gem_instance)
				gem_instances[y * board_width + x] = gem_instance
			else:
				gem_instances[y * board_width + x] = null

func refresh_board() -> void:
	# Refresh all gems from board_sim state
	for y in range(board_height):
		for x in range(board_width):
			var idx = y * board_width + x
			var gem_instance = gem_instances[idx]
			var cell = board_sim.get_cell(x, y)
			
			if not cell.empty:
				if gem_instance == null || !is_instance_valid(gem_instance):
					# Create new gem instance
					gem_instance = gem_scene.instantiate()
					add_child(gem_instance)
					gem_instances[idx] = gem_instance
				
				gem_instance.set_gem(cell.kind, cell.has_echo)
				
				var pos_x = -((board_width * cell_size + (board_width - 1) * padding) / 2.0) + cell_size / 2.0 + x * (cell_size + padding)
				var pos_y = -((board_height * cell_size + (board_height - 1) * padding) / 2.0) + cell_size / 2.0 + y * (cell_size + padding)
				gem_instance.position = Vector2(pos_x, pos_y)
			else:
				if gem_instance != null && is_instance_valid(gem_instance):
					gem_instance.queue_free()
					gem_instances[idx] = null

func get_board_sim() -> RefCounted:
	return board_sim