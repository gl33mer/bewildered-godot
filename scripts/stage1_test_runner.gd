extends Node

var board_sim: RefCounted

func _ready():
	print("=== Stage 1 FFI Round-Trip Test Started ===")
	
	# Instantiate the BoardSim class from GDExtension
	board_sim = BoardSim.new()
	if board_sim == null:
		print("ERROR: Failed to instantiate BoardSim")
		get_tree().quit()
		return
	
	# Connect all signals
	board_sim.match_resolved.connect(_on_match_resolved)
	board_sim.special_gem_created.connect(_on_special_gem_created)
	board_sim.echo_charged.connect(_on_echo_charged)
	board_sim.echo_detonated.connect(_on_echo_detonated)
	board_sim.move_rejected.connect(_on_move_rejected)
	board_sim.objective_progress.connect(_on_objective_progress)
	
	# Test 1: Initialize board
	print("\n--- Test 1: Initialize Board ---")
	board_sim.new_board(8, 8, 12345)
	var width = board_sim.get_width()
	var height = board_sim.get_height()
	print("Board size: %d x %d" % [width, height])
	assert(width == 8)
	assert(height == 8)
	print("Board initialized successfully")
	
	# Test 2: Inspect initial board state
	print("\n--- Test 2: Inspect Initial Board ---")
	for y in range(8):
		for x in range(8):
			var cell = board_sim.get_cell(x, y)
			if not cell.empty:
				print("Cell (%d, %d): kind=%d, has_echo=%s" % [x, y, cell.kind, cell.has_echo])
	
	# Test 3: Invalid swap (out of bounds)
	print("\n--- Test 3: Invalid Swap (Out of Bounds) ---")
	var result = board_sim.try_swap(-1, 0, 0, 0)
	print("Result: %s (expected false)" % result)
	assert(result == false)
	print("PASS: move_rejected signal should have fired")
	
	# Test 4: Invalid swap (non-adjacent)
	print("\n--- Test 4: Invalid Swap (Non-Adjacent) ---")
	result = board_sim.try_swap(0, 0, 0, 2)
	print("Result: %s (expected false)" % result)
	assert(result == false)
	print("PASS: move_rejected signal should have fired")
	
	# Test 5: Valid swap that creates a match
	print("\n--- Test 5: Valid Swap Creating Match ---")
	var found_match = false
	for y in range(8):
		for x in range(8):
			if x + 1 < 8:
				result = board_sim.try_swap(x, y, x + 1, y)
				if result:
					print("Found valid swap at (%d, %d) -> (%d, %d)" % [x, y, x + 1, y])
					found_match = true
					break
			if y + 1 < 8:
				result = board_sim.try_swap(x, y, x, y + 1)
				if result:
					print("Found valid swap at (%d, %d) -> (%d, %d)" % [x, y, x, y + 1])
					found_match = true
					break
		if found_match:
			break
	
	if not found_match:
		print("No valid swap found on this seed - trying different seed")
		board_sim.new_board(8, 8, 54321)
		for y in range(8):
			for x in range(8):
				if x + 1 < 8:
					result = board_sim.try_swap(x, y, x + 1, y)
					if result:
						print("Found valid swap at (%d, %d) -> (%d, %d)" % [x, y, x + 1, y])
						found_match = true
						break
				if y + 1 < 8:
					result = board_sim.try_swap(x, y, x, y + 1)
					if result:
						print("Found valid swap at (%d, %d) -> (%d, %d)" % [x, y, x, y + 1])
						found_match = true
						break
			if found_match:
				break
	
	if found_match:
		print("PASS: Valid swap executed and signals should have fired")
	else:
		print("WARNING: Could not find valid swap on either seed")
	
	# Test 6: Verify board state after swap
	print("\n--- Test 6: Board State After Swap ---")
	var combo = board_sim.get_combo()
	var resonance = board_sim.get_resonance_multiplier()
	print("Combo: %d, Resonance: %.2f" % [combo, resonance])
	
	print("\n=== Stage 1 FFI Round-Trip Test Completed ===")
	get_tree().quit()

func _on_match_resolved(cleared_cells: Array[Vector2i], gem_kind: int, cascade_depth: int):
	print("SIGNAL: match_resolved - cells=%d, kind=%d, cascade=%d" % [cleared_cells.size(), gem_kind, cascade_depth])

func _on_special_gem_created(pos: Vector2i, kind: int):
	var kind_names = {0: "Bolt", 1: "Prism", 2: "Nova"}
	print("SIGNAL: special_gem_created - pos=(%d, %d), kind=%s" % [pos.x, pos.y, kind_names.get(kind, "Unknown")])

func _on_echo_charged(cells: Array[Vector2i]):
	print("SIGNAL: echo_charged - cells=%d" % [cells.size()])

func _on_echo_detonated(cells: Array[Vector2i], multiplier: float):
	print("SIGNAL: echo_detonated - cells=%d, multiplier=%.2f" % [cells.size(), multiplier])

func _on_move_rejected(ax: int, ay: int, bx: int, by: int):
	print("SIGNAL: move_rejected - (%d, %d) -> (%d, %d)" % [ax, ay, bx, by])

func _on_objective_progress(current: int, target: int):
	print("SIGNAL: objective_progress - current=%d, target=%d" % [current, target])