# HANDOFF.md — Stage Completion Summaries

## Project: Bewildered Godot Migration

### Stage 0 — Scaffolding & Verification ✅ COMPLETE

**Date**: 2026-08-23

**Summary**: 
- Created Rust workspace at `rust/` with 4 crates: `bewildered-core`, `bewildered-content`, `bewildered-solver`, `bewildered-godot`
- Copied `bewildered-core`, `bewildered-content`, `bewildered-solver`, and `Docs/` verbatim from legacy repo
- Scaffolded GDExtension bridge crate (`bewildered-godot`) with `cdylib` output
- Created `rust/.gdignore`, `rust/Cargo.toml`, `bewildered.gdextension`
- Verified `cargo test --workspace` passes (8 tests total: 3 content, 5 content roundtrip)
- Verified `cargo build` produces `libbewildered_godot.so` with correct `gdext_rust_init` entry point
- Verified Godot 4.7.2 loads the extension without symbol errors
- Verified MCP server connects and editor screenshot capture works

**Test Results**:
```
cargo test --workspace:
  bewildered-content: 3 passed
  content_roundtrip: 5 passed (level load, ron roundtrip, pack dir, objective variants, pack zip)
  bewildered-core: 0 tests (tests in lib.rs)
  bewildered-godot: 0 tests (empty bridge)
  bewildered-solver: 0 tests (tests in main.rs)
```

**Key Files Created**:
- `rust/.gdignore` - Empty file to ignore target/
- `rust/Cargo.toml` - Workspace with resolver = "2"
- `rust/crates/bewildered-godot/Cargo.toml` - cdylib with godot dependency
- `rust/crates/bewildered-godot/src/lib.rs` - Minimal ExtensionLibrary
- `bewildered.gdextension` - Entry symbol `gdext_rust_init`

**Next Stage**: Stage 1 — FFI Round-Trip Proof

---

### Stage 1 — FFI Round-Trip Proof ✅ COMPLETE

**Date**: 2026-08-23

**Summary**:
- Implemented `BoardSim` class in `bewildered-godot` with all required methods and signals
- Created headless test scene (`scenes/stage1_test.tscn`) with runner script (`scripts/stage1_test_runner.gd`)
- Verified end-to-end Rust↔Godot boundary works correctly

**BoardSim API Implemented**:
- `new_board(width: i32, height: i32, seed: i64)` — Initializes `bewildered_core::Board` with 4 gem types
- `try_swap(ax: i32, ay: i32, bx: i32, by: i32) -> bool` — Executes swap, converts `MoveOutcome` to signals
- `get_cell(x: i32, y: i32) -> Dictionary` — Inspects cell state (kind, echo charge)
- `get_width()`, `get_height()`, `get_combo()`, `get_resonance_multiplier()` — Board getters

**Signals Emitted**:
- `match_resolved(cleared_cells: Array<Vector2i>, gem_kind: i32, cascade_depth: i32)`
- `special_gem_created(pos: Vector2i, kind: i32)` — 0=Bolt, 1=Prism, 2=Nova
- `echo_charged(cells: Array<Vector2i>)`
- `echo_detonated(cells: Array<Vector2i>, multiplier: f32)`
- `move_rejected(ax: i32, ay: i32, bx: i32, by: i32)`
- `objective_progress(current: i64, target: i64)`

**Test Results** (from `scripts/stage1_test_runner.gd`):
```
=== Stage 1 FFI Round-Trip Test Started ===

--- Test 1: Initialize Board ---
Board size: 8 x 8
Board initialized successfully

--- Test 2: Inspect Initial Board ---
Cell (0, 0): kind=1, has_echo=false
... (64 cells printed)

--- Test 3: Invalid Swap (Out of Bounds) ---
SIGNAL: move_rejected - (-1, 0) -> (0, 0)
Result: false (expected false)
PASS: move_rejected signal should have fired

--- Test 4: Invalid Swap (Non-Adjacent) ---
SIGNAL: move_rejected - (0, 0) -> (0, 2)
Result: false (expected false)
PASS: move_rejected signal should have fired

--- Test 5: Valid Swap Creating Match ---
SIGNAL: match_resolved - cells=4, kind=2, cascade=1
SIGNAL: match_resolved - cells=4, kind=2, cascade=2
SIGNAL: match_resolved - cells=3, kind=1, cascade=3
SIGNAL: match_resolved - cells=3, kind=3, cascade=4
SIGNAL: special_gem_created - pos=(6, 0), kind=Bolt
SIGNAL: special_gem_created - pos=(5, 4), kind=Bolt
SIGNAL: objective_progress - current=3, target=10000
Found valid swap at (0, 0) -> (1, 0)
PASS: Valid swap executed and signals should have fired

--- Test 6: Board State After Swap ---
Combo: 3, Resonance: 1.00

=== Stage 1 FFI Round-Trip Test Completed ===
```

**Key Files Created/Modified**:
- `rust/crates/bewildered-godot/src/lib.rs` — Full BoardSim implementation
- `scenes/stage1_test.tscn` — Headless test scene
- `scripts/stage1_test_runner.gd` — GDScript test runner with signal handlers

**Next Stage**: Stage 2 — Static Board Render
- Build `board.tscn` and `gem.tscn` (`Sprite2D`)
- Render initial grid from `BoardSim`
- Verify glyph/shape distinction across all gem types
- Capture MCP screenshot to verify