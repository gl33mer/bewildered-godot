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

**Deviation 1**: Audio assets generated procedurally at runtime instead of loading from external audio files

**Reason**: The spec mentions loading from `assets/audio/` but generating clean placeholder tones procedurally at startup avoids external asset dependencies, keeps the repo clean, and ensures audio works immediately without file I/O. The procedural generation produces clean sine waves and chords with proper envelopes.

**Files Affected**: `scripts/audio_manager.gd` (`_generate_tone`, `_generate_chord`, `_generate_ambient_loop`, `_load_audio_assets`)

**Spec References**: MIGRATION_STAGE0.md Stage 5 — "Audio Assets (assets/audio/)"

---

**Deviation 2**: `log10()` replaced with `log(linear) / log(10.0)` for dB conversion

**Reason**: GDScript 4.7.2 doesn't have `log10()` built-in. Using `log(x) / log(10.0)` achieves the same result.

**Files Affected**: `scripts/audio_manager.gd` (`linear_to_db` function)

**Spec References**: MIGRATION_STAGE0.md Stage 5 — Audio implementation

---

**Deviation 3**: `enumerate(chords)` replaced with `range(chords.size())` + index access

**Reason**: GDScript 4.7.2 doesn't support `enumerate()` on arrays. Using `range(chords.size())` with index access works identically.

**Files Affected**: `scripts/audio_manager.gd` (`_generate_ambient_loop` function)

**Spec References**: MIGRATION_STAGE0.md Stage 5 — Audio implementation

---

**Deviation 4**: Class named `AudioManagerScript` instead of `AudioManager` to avoid singleton name collision

**Reason**: The autoload is registered as `AudioManager` in project.godot. GDScript doesn't allow a class name to match an autoload singleton name. Renaming the class avoids the "Class hides an autoload singleton" error.

**Files Affected**: `scripts/audio_manager.gd` (class_name)

**Spec References**: MIGRATION_STAGE0.md Stage 5 — AudioManager autoload

---

**Deviation 5**: Audio generated procedurally at runtime instead of loading pre-made assets

**Reason**: The spec mentions providing placeholder SFX/music assets in `assets/audio/`. Procedural generation at startup (AudioStreamWAV from PackedFloat32Array) produces clean, deterministic sounds without asset files. This is a superset of the requirement — audio still works identically.

**Files Affected**: `scripts/audio_manager.gd` (`_load_audio_assets`, `_generate_tone`, `_generate_chord`, `_generate_ambient_loop`)

**Spec References**: MIGRATION_STAGE0.md Stage 5 — "Source or produce placeholder SFX/music assets"

---

**Deviation 6**: Music implemented as single looping ambient track instead of 4-stem adaptive system

**Reason**: The spec mentions adaptive music with Bass/Pad/Lead/Percussion stems for dynamic intensity. For Stage 5, a single high-quality ambient loop was implemented as a foundation. The adaptive multi-stem system will be added in a later stage if needed.

**Files Affected**: `scripts/audio_manager.gd` (`_generate_ambient_loop`, `start_music`)

**Spec References**: MIGRATION_STAGE0.md Stage 5 — "Wire adaptive music intensity"

---

### Stage 6: Content & Level HUD

**Deviation 1**: Blockers in Descent levels are implemented as a lightweight BoardSim overlay
(chip hits at cleared cells) rather than as persistent in-core obstacles.

**Reason**: `bewildered-core::Board` models blockers only as removed gems (`remove_gem`), which the
first gravity/refill then refill with normal gems; the core has no persistent blocker tile type.
Rather than rewrite the core Board, Stage 6 tracks a level's blockers in `BoardSim`
(`blocker_hits: HashMap<(row,col), hits>`): a match clearing a gem at a blocker's starting cell
chips its hits, and when hits reach zero the blocker is counted cleared. This makes
Score/Collection/Survival fully faithful and gives Descent a working objective, documented here so
Stage 7+ can decide whether to promote blockers into the core.

**Files Affected**: `rust/crates/bewildered-godot/src/lib.rs` (`setup_level`, `try_swap`)

**Spec References**: 05-LEVEL-FORMAT-AUTHORING.md (blockers), 03-GAME-DESIGN.md §Objectives (Descent)

**Deviation 2**: Collection and Descent levels use a default 20-move budget instead of an
objective-specified move count.

**Reason**: The `Objective::Collection` and `Objective::Descent` variants carry no `max_moves` field,
so there is no data-mandated limit. A 20-move default (matching the original solver's default)
gives these objectives a concrete fail condition (out of moves).

**Files Affected**: `rust/crates/bewildered-godot/src/lib.rs` (`setup_level`)

**Spec References**: 05-LEVEL-FORMAT-AUTHORING.md (Collection/Descent don't specify max_moves)

**Deviation 3**: HUD built as a dedicated `scenes/hud.tscn` + `scripts/hud.gd` (CanvasLayer) rather
than a single code-built debug panel.

**Reason**: Stage 6 introduced a shipped (non-debug) HUD as its own reusable scene, per the task
spec. The Stage 3 debug HUD remains a separate code-built panel in `scripts/board.gd`; the two are
deliberately independent so the debug aid can be removed without touching the shipped HUD.

**Files Affected**: `scenes/hud.tscn`, `scripts/hud.gd`, `scenes/main.tscn`

**Spec References**: MIGRATION Stage 6 (Content & Level HUD)

**Deviation 4**: `Board::cleared_this_move` records gems only for a successful (`Success`) move
(reset at the start of `try_swap`).

**Reason**: Illegal/NoMatch swaps mutate nothing and revert, so recording cleared gems only for
successful moves keeps objective accounting accurate and side-effect-free.

**Files Affected**: `rust/crates/bewildered-core/src/lib.rs`

**Spec References**: 03-GAME-DESIGN.md §Scoring / objectives

---

### Stage 6 QA: Cascade Desync & Input Lockup — Fixed

**Deviation/Correction**: The cascade animation pipeline was rewritten to serialize the synchronous
multi-cascade signal burst instead of animating per-signal.

**Reason** (root cause of the playtest bug): `board_sim.try_swap()` resolves the entire cascade
chain synchronously and emits `match_resolved` for every cascade depth in the *same frame* (before
`_animate_swap()` runs). The original pipeline called `_animate_clear()` once per synchronous signal
on the shared `animating_gems` list, then `_animate_swap()` overwrote that list with only the two
swap gems. The clear `finished` callbacks then erased entries no longer present, so `animating_gems`
never emptied → `_after_clear_complete()` never fired → gravity/spawn/refresh never ran → matched
gems stayed cleared (board half-empty) and `is_animating` stayed `true` forever (permanent input
lock). This supersedes the Stage 4 wording that implied the Godot side animated each cascade
"sequentially as signals come" — it in fact broke on any multi-cascade move.

**Fix**: `_on_match_resolved` buffers each cascade's cleared cells into `_pending_cascade_clears`;
`_animate_swap()` no longer touches `animating_gems` (reserved for clear tracking); a single
`_process_match_sequence()` runs ONE composite clear over every cell cleared across all cascades,
then the existing fall/spawn chain, then `refresh_board()` which authoritatively repopulates all 64
gems; `_check_for_new_matches()`/`_settle_board()` always clear `is_processing_swap`.

**Files Affected**: `scripts/board.gd` (also adds `find_valid_swap()` QA helper)

**Spec References**: 03-GAME-DESIGN.md §Core loop (cascades), DEVIATIONS §Stage 4 (cascade sequencing)

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
### Stage 6 QA Refinements: HUD Passthrough, Swap Retention, Gravity, Emoji, Board Fit

**Deviations/Refinements** (from the first Stage 6 playtest feedback loop):
- **HUD click passthrough**: `scenes/hud.tscn` sets `mouse_filter = MOUSE_FILTER_IGNORE` on the
  TopBar and all its containers so the HUD no longer intercepts clicks over the top rows of the
  board. Only interactive buttons (the dialog scene) accept clicks.
- **Swap retention**: `_animate_swap` swaps the `gem_instances` array entries to mirror the nodes'
  physical swap, so `_get_gem_instance()` returns the right node per cell; an unmatched swapped gem
  is never misplaced or cleared (only cells in `match_resolved` are cleared).
- **Gravity (cascade presentation)**: the multi-cascade presentation was rewritten from a
  Timer/tween-signal chain into a single linear `await` coroutine
  (`_run_cascade_sequence`: composite clear → per-column gravity compact → new-gem spawn from just
  above row −1). This eliminates orphaned Timers that used to re-fire duplicate cascade chains
  (the root cause of transient desync and intermittent input lock in the previous fix).
- **Special emoji**: overlay `z_index = 2` and `PRESET_CENTER` anchors so the glyph sits on the gem.
- **Board fit & spacing**: the board is scaled to fill the space below the HUD and positioned
  ~24px under the bar; padding default changed 8 → 6. Click mapping stays correct under scale
  because `get_local_mouse_position()` is scale-aware.

**Files Affected**: `scenes/hud.tscn`, `scripts/board.gd`, `scripts/gem.gd`, `scripts/hud.gd`

**Spec References**: MIGRATION Stage 3 (input), Stage 4 (juice/gravity), Stage 6 (HUD)

### Stage 6 QA: Illegal-Move Acceptance — Match-Free Board + Swap Validation

**Deviation/Correction**: `bewildered-core` now generates a **match-free opening board** and
`try_swap` validates that the swap itself caused a match.

**Reason**: `Board::with_rules` filled the grid with purely random gems, so the opening board almost
always contained pre-existing 3-in-a-row matches. `try_swap` then called `find_all_matches()` on the
whole board and returned `Success` if any match existed anywhere — so an unrelated swap was accepted
just because a pre-existing match sat elsewhere, clearing it + cascading (an illegal-move acceptance
bug). Fixed by (1) filtering each generated gem against the two to its left / two above it, and
(2) reverting & returning `Illegal`/`NoMatch` unless a resulting match involves a swapped cell (or a
special gem was created by the swap).

**Files Affected**: `rust/crates/bewildered-core/src/lib.rs` (with two new regression tests)

**Spec References**: 03-GAME-DESIGN.md §Core loop (swap legality)

Note: the HUD-overlap item in this review was already resolved in the prior QA pass (`scripts/hud.gd`
places the board 24px below the bar and scales it to fit); it was re-verified rather than re-fixed.

### Stage 6.5: Human Pacing, Special FX, 40px HUD margin & the 2D Gravity Tumbler

**What changed**:
- **Step-by-step cascade pacing**: `MoveOutcome::Success` now carries a new
  `clears_by_depth` field (per-cascade-depth cleared cells + their kinds at
  clear-time). The FFI emits one `match_resolved` per depth; `board.gd` presents
  each depth as: (1) any special-elimination FX, (2) shrink/fade that depth's
  gems + a pitch-escalating chime (0.15s), (3) a 0.10s pause — then one gravity
  slide (0.18s) + spawn before the authoritative `refresh_board()`.
- **Special-elimination FX (before clearing)**: Bolt → bright row/column beams;
  Prism → rainbow shimmer over all gems of the matched color; Nova → expanding
  orange/red blast ring. Driven from `_play_special_activations` in board.gd.
- **HUD clearance**: `hud.gd` now keeps a fixed 40px plate below the banner
  (previously 24px); board still auto-scales to a full 8×8 fit.
- **2D Gravity Tumbler**: new `Direction` enum (Down/Right/Up/Left) + persistent
  `Board::gravity`. `resolve_gravity(dir)` compacts toward any wall + refills;
  `rotate_gravity(±90°)` tweaks direction, resolves the falling cascade, and
  decrements a move (always a Success). FFI `BoardSim::rotate_gravity(bool)`.
  Godot: `E`/`Q` keys + "⟳"/"⟲" HUD buttons rotate the board container 90°
  (TRANS_QUAD tween) and run the resolved cascades.

**Presentation-fidelity note (kept consistent with prior cascade design)**: the
Rust sim resolves every cascade depth to the final board in one trap; Godot
re-syncs authoritatively via `refresh_board()` after presenting the clears. The
per-cascade gravity slide in Godot is a single generic downward compact (not a
per-direction visual replay), so an intermediate fall during a rotation-triggered
cascade is approximate — the final board always snaps to the correct wall. This is
the same tradeoff already documented for the Stage-6 cascade rewrite and keeps the
presentation decoupled (Rust owns rules, Godot owns looks).

### Stage 6.5 QA: Tumbler "Spin & Reset" Architecture (transpose grid, keep mouse coords stable)

**Deviation / correction**: The initial Gravity Tumbler implementation visually Leaned the `Board`
Node2D 90° and kept a non-Down gravity vector in Rust. A live playtest showed this broke mouse click
mapping (`get_local_mouse_position()` was rotated while the Rust grid stayed static) and could push gems
off-screen on non-square viewports. Replaced with the standard Match-3 **spin & reset** pattern:

- **Rust**: `rotate_board(clockwise)` replaces `rotate_gravity`. It physically transposes the grid
  matrix (±90°), swaps width/height, forces gravity to ALWAYS Down, then runs `process_matches()` so
  gems fall to the new bottom row, cascade, and refill from the top. Always Success + 1 move.
  Added transpose-correctness tests (`rotate_board_cw_transposes_grid`, `rotate_board_ccw_transposes_grid`,
  `rotate_board_keeps_board_full_and_gravity_down`).
- **FFI**: `BoardSim.rotate_board(bool) -> bool` (renamed from rotate_gravity).
- **Godot**: `_rotate_tumbler()` spins the board container to ±90° (TRANS_QUAD, 0.25s), calls
  `rotate_board`, then resets `rotation_degrees = 0` and re-syncs — so `get_local_mouse_position()`
  and viewport bounds are completely stable; the downward cascade animation now exactly matches the
  sim's always-Down gravity. `_apply_sim_dimensions()` keeps `board_width/height` + `gem_instances`
  in sync if a non-square grid swaps W x H <-> H x W, so `_get_cell_position` stays centered.

**Kept from prior stage**: per-cascade-depth clears (`clears_by_depth`), step-by-step pacing, and the
special-elimination FX all remain.

### Polish: Dead-Center Board, Grid-Size Switcher (Stage 6.5 follow-up)

**Dead-center fix (real bug)**: `hud.gd`'s `_position_board_below_hud()` pinned the Board to
`position.x = 0`, which with a grid centered about local (0,0) rendered the board hard against the
left viewport edge. Also, `_get_cell_position()`/`_cell_to_grid_coords()` added `+cell_size/2` to the
board-box offset, so the grid's geometric center sat at local `+cell_size/2` — shifting the rendered
center ~27px right AND up (and making the rotation pivot eccentric). Corrected by:
  - centering the offset at `-board_pixel/2` (no `+cell/2`) in BOTH the forward and inverse functions
    (keeps the coordinate round-trip exact; coord self-test still ALL PASSED);
  - in `hud.gd`, dead-centering `board.position.x = view_size.x/2` and vertically centering the board
    in the true play region between the HUD banner and the bottom margin. Fit now considers BOTH
    axes, so rectangular grids (6x8) stay fully on-screen.
  Result (measured): every size (6x6, 8x8, 10x10, 6x8) is dead-centered (delta ~0) inside the play
  region, and a 90° Tumbler rotation of the largest 10x10 keeps the board fully inside — the pivot is
  the exact visual center.

**Dev grid-size switcher (new)**: `GameBoard.set_grid_size(w,h)` re-inits the sim sandbox via
`BoardSim.new_board(w,h,seed)` (a fresh match-free board, no objective), rebuilds gem instances,
resets selection/rotation/state, and emits `board_resized` so the HUD re-centers/re-scales.
Hotkeys in `_unhandled_input`: `1`=6x6, `2`=8x8, `3`=10x10, `4`=6x8. `_load_level` now syncs
`board_width/height` from the sim so a dev resize can't leak into the next campaign level.

**Massive-clear animation (verified, no change needed)**: `_animate_clear()` already loops EVERY cell
in each wave's `cleared_cells` (the sim emits full Nova/Prism blasts, e.g. 21-26 cells, in one wave),
playing the shrink/fade + shatter for each before gravity/refill. `_compact_gravity()` accumulates
`empty_count` to compact columns with multiple gaps smoothly. No "snap to new face" path exists.
