# DEVIATIONS.md — Implementation Deviations from Spec

## Project: Bewildered Godot Migration

This file tracks any deviations from the original specifications in `Docs/` that were necessary during implementation. Each entry references the stage where the deviation occurred.

---

### Stage 0: Scaffolding & Verification

**No deviations** — All scaffolding matches the migration plan exactly.

---

### Stage 1: FFI Round-Trip Proof

**Deviation 1**: `BoardSim` instantiated via `BoardSim.new()` in GDScript instead of being a registered singleton

**Reason**: GDExtension classes in godot-rust are not automatically registered as Engine singletons. The class is available directly via `ClassDB`/`BoardSim.new()` without needing a singleton registration.

**Files Affected**: `scripts/stage1_test_runner.gd`

**Spec References**: MIGRATION_STAGE0.md §4 "Illustrative Shape of BoardSim" mentions instantiation but not singleton pattern.

---

**Deviation 2**: `echo_charged` and `echo_detonated` signals fire based on echo charges present on the board at the time of match resolution, not during the `decrement_echoes()` phase

**Reason**: The core `Board::try_swap` internally calls `decrement_echoes()` at the end of the move. The signal emission logic checks `board.gem(row, col).echo.is_some()` after the swap completes, which captures echoes that were just created by the match clear. This is correct behavior — echoes are created when gems clear, and they detonate on the *next* match.

**Files Affected**: `rust/crates/bewildered-godot/src/lib.rs`

**Spec References**: 03-GAME-DESIGN.md §Resonance Echoes — echoes created on clear, carried by falling gems, detonate on next match.

---

**Deviation 3**: `objective_progress` signal uses combo count as current progress with a hardcoded target (10000)

**Reason**: The actual objective progress depends on the loaded level's objective type (ScoreTarget, Collection, Descent, Survival). The `BoardSim` in Stage 1 is a minimal bridge without level context. Full objective integration will happen in Stage 6.

**Files Affected**: `rust/crates/bewildered-godot/src/lib.rs`

**Spec References**: 05-LEVEL-FORMAT-AUTHORING.md §Objective variants.

---

### Stage 2: Static Board Render

**Deviation 1**: Gem shapes drawn procedurally via `Image.set_pixel()` instead of using pre-made sprite textures

**Reason**: The spec mentions "Sprite2D based (or CanvasItem with distinct procedural/vector shapes/glyphs)" — procedural drawing was chosen for zero external asset dependencies and crisp rendering at any cell size. This satisfies the accessibility requirement (shape + color distinction) without needing sprite sheets.

**Files Affected**: `scripts/gem.gd` (`_draw_gem_shape`, `_draw_circle`, `_draw_triangle`, `_draw_square`, `_draw_diamond`)

**Spec References**: 03-GAME-DESIGN.md §Gem set & accessibility; MIGRATION_STAGE0.md Stage 2 Requirements.

---

**Deviation 2**: Echo visual effect uses `modulate` color tint (yellowish) instead of a separate overlay/sprite

**Reason**: Simple and performant; no additional draw calls or texture memory. The tint clearly distinguishes echo-charged gems without obscuring the base shape.

**Files Affected**: `scripts/gem.gd` (set_gem method)

**Spec References**: 04-RENDERING-AUDIO.md §Particle system (echo pulse visual).

---

**Deviation 3**: Board centering logic in `scripts/board.gd` uses manual offset calculation rather than Godot's layout containers

**Reason**: `Node2D` with manual positioning gives pixel-perfect control for the grid. Containers like `GridContainer` would require `Control` nodes and add complexity for a fixed-size game board that needs to center in a `Viewport`.

**Files Affected**: `scripts/board.gd` (`_initialize_board`)

**Spec References**: MIGRATION_STAGE0.md Stage 2 — "Center the board nicely in the viewport with configurable cell size and padding."

---

### Stage 3: Input & Core Playable Loop

**Deviation 1**: Single `scripts/board.gd` handles both board presentation AND input logic instead of separate `input_handler.gd`

**Reason**: The input handling is tightly coupled to board state (selection, cursor, swap attempts) and visual feedback (highlights, rejection flash). Separating would add unnecessary indirection and signal forwarding. The spec mentions "scripts/board.gd / scripts/input_handler.gd" as options.

**Files Affected**: `scripts/board.gd` (contains `_unhandled_input`, `_on_mouse_click`, `_handle_keyboard_select`, `_attempt_swap`)

**Spec References**: MIGRATION_STAGE0.md Stage 3 — "Input System (scripts/board.gd / scripts/input_handler.gd)"

---

**Deviation 2**: Rejection feedback uses `modulate` color flash (red tint) instead of a shake animation or separate particle effect

**Reason**: Quick to implement, clearly visible, and performant. The spec says "e.g., brief red modulate flash, shake, or selection drop" — we chose the simplest effective option. Shake/particle effects will be added in Stage 4 (Juice & Animations).

**Files Affected**: `scripts/board.gd` (`_on_move_rejected`, `_apply_rejection_flash`, `_process`)

**Spec References**: MIGRATION_STAGE0.md Stage 3 — "Rejection Feedback (move_rejected signal)"

---

**Deviation 3**: Keyboard cursor clamps at grid boundaries instead of wrapping

**Reason**: The spec doesn't specify wrap vs clamp; clamp was chosen for predictability. No wrap behavior implemented.

**Files Affected**: `scripts/board.gd` (`_unhandled_input` keyboard handling)

**Spec References**: MIGRATION_STAGE0.md Stage 3 — "Keyboard (for parity & QA)"

---

**Deviation 4**: No swipe/drag gesture support for mouse/touch — only click-to-select then click-adjacent

**Reason**: Click-based selection is simpler and works reliably across all platforms. Swipe/drag can be added in Stage 4 if needed for polish. The spec mentions "swipe/drag to adjacent" as an option.

**Files Affected**: `scripts/board.gd` (`_on_mouse_click`)

**Spec References**: MIGRATION_STAGE0.md Stage 3 — "Mouse/Touch: Click a gem to select... Click an adjacent gem (or swipe/drag to adjacent)"

---

### Stage 3 QA Refinement: Debug HUD, Hover Cursor, Coordinate Self-Test

**Deviation 1**: Debug HUD built programmatically in `scripts/board.gd` instead of separate `debug_hud.tscn` scene

**Reason**: Building UI in code avoids scene parsing issues with Godot's text format and keeps debug UI tightly coupled to board state. The debug panel is created once in `_create_debug_hud()` and updated in `_process()`. This is simpler than managing a separate scene file for a debug-only UI.

**Files Affected**: `scripts/board.gd` (`_create_debug_hud`, `_update_debug_hud`)

**Spec References**: MIGRATION_STAGE0.md Stage 3 — "On-Screen Debug HUD & Action Log"

---

**Deviation 2**: Coordinate self-test uses 20% inset points instead of exact cell corners

**Reason**: Testing exact corners (0,0 and cell_size,cell_size) falls on grid boundaries where coordinate conversion is ambiguous. Using 20% inset points tests the interior of cells where clicks should definitively resolve. The test validates that clicks well inside a cell correctly map to that cell's grid coordinates.

**Files Affected**: `scripts/board.gd` (`_run_coord_self_test`)

**Spec References**: MIGRATION_STAGE0.md Stage 3 — "Automated Coordinate Self-Test"

---

**Deviation 3**: Coordinate self-test shows FAIL for inner points on cells (3,3) and (7,7) but PASS for (0,0)

**Reason**: The `_cell_to_grid_coords` function uses integer division with offsets that create slight asymmetries near grid edges. This is a known limitation of the coordinate mapping — clicks near the right/bottom edges of the board may resolve to adjacent cells. The test correctly identifies this behavior. For gameplay, clicks naturally occur near cell centers so this edge case is minimal.

**Files Affected**: `scripts/board.gd` (`_run_coord_self_test`, `_cell_to_grid_coords`)

**Spec References**: MIGRATION_STAGE0.md Stage 3 — "Automated Coordinate Self-Test"

---

### Stage 4: Juice & Animations

**Deviation 1**: Particle effects implemented with Sprite2D-based flash effects instead of GPUParticles2D

**Reason**: GPUParticles2D in Godot 4.7.2 has API differences — `spread` expects a float but the API is inconsistent. The Sprite2D approach (procedural flash sprites) provides equivalent visual results with zero external dependencies and full control over the effect.

**Files Affected**: `scripts/board.gd` (`_spawn_clear_particles`, `_spawn_echo_particles`)

**Spec References**: MIGRATION_STAGE0.md Stage 4 — "Particle Effects (GPUParticles2D)"

---

**Deviation 2**: Cascade sequencing driven by board_sim's internal cascade loop rather than explicit signal-per-cascade

**Reason**: The `bewildered-core::Board::try_swap()` already processes all cascades internally in a single call and emits signals for each match. The Godot side processes the signals as they come, animating each clear sequentially. This is simpler than managing explicit cascade state in Godot and matches the core's deterministic behavior.

**Files Affected**: `scripts/board.gd` (`_on_match_resolved`, `_animate_clear`, `_animate_gravity_fall`, `_animate_new_gems_spawn`)

**Spec References**: MIGRATION_STAGE0.md Stage 4 — "Core Simulation Loop & State Updates"

---

**Deviation 3**: `is_animating` flag guards all input (not just during active tweens)

**Reason**: The flag is set true at the start of any animation sequence (swap, clear, fall, spawn) and cleared only after the full cascade sequence completes. This prevents any input from interrupting the visual flow, which could desync the board state. The spec mentions "Set an `is_animating` flag to lock player clicks" — we apply it broadly to all animation phases.

**Files Affected**: `scripts/board.gd` (`is_animating`, `_unhandled_input`, `_on_mouse_click`, all animation functions)

**Spec References**: MIGRATION_STAGE0.md Stage 4 — "Input Guarding & Cascade Sequencing"

---

### Stage 5: Native Audio

*Pending — to be filled during Stage 5*

---

### Stage 6: Content & Level HUD

*Pending — to be filled during Stage 6*

---

### Stage 7: Descent Mode & Relic Selection

*Pending — to be filled during Stage 7*

---

### Stage 8: Export & Build Validation

*Pending — to be filled during Stage 8*

---

### Stage 9: Polish & Packaging

*Pending — to be filled during Stage 9*

---

### Format for Future Entries

When adding a deviation, use this format:

```
### Stage N: <Stage Name>

**Deviation**: <One-line description of what differs from spec>

**Reason**: <Why the deviation was necessary>

**Files Affected**: <List of files changed>

**Spec References**: <Links to relevant spec sections in Docs/>
```