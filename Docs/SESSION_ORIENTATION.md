# SESSION_ORIENTATION.md — Lead Engineer Orientation & Stage 6 Kickoff

**Date**: 2026-08-24
**Engineer**: Lead Rust systems programmer / Godot 4 engineer (Bewildered)
**Working tree**: `/home/gorg/Projects/BewilderedGodot/bewildered` (uncommitted work on `master`)

---

## 1. Understanding of the FFI Boundary Contract (Rust ↔ Godot)

The architecture is a strict **simulation/presentation decoupling**. Rust owns all game *rules*;
Godot owns all *presentation*. The contract between them is the **`BoardSim` GDExtension class**
(summarized directly in the HARNDOFF completion notes):

### Rust (authoritative state) — `rust/crates/bewildered-godot/src/lib.rs`
- `BoardSim` is a `#[godot_api]` `RefCounted` class registered via `gdext` (`gdext_rust_init`).
- It owns a `bewildered_core::Board` (`Option<Board>`), instantiated via `BoardSim.new()` in GDScript
  (no Engine singleton — see DEVIATIONS §Stage 1, Deviation 1).
- **Required surface already implemented:**
  - `new_board(width, height, seed)` — builds the board with 4 gem types.
  - `try_swap(ax, ay, bx, by) -> bool` — resolves the move **and drives its full internal cascade
    loop**, emitting signals as it goes.
  - `get_cell(x, y) -> Dictionary` — exposes `empty`, `kind`, `has_echo`, `echo_moves_left`, and now
    **`special`** (0=None, 1=Bolt⚡, 2=Prism🌈, 3=Nova💥).
  - Getters: `get_width`, `get_height`, `get_combo`, `get_resonance_multiplier`.
- **Signals emitted** (Godot consumes them for juice/audio):
  - `match_resolved(cells, gem_kind, cascade_depth)`
  - `special_gem_created(pos, kind)` (0=Bolt, 1=Prism, 2=Nova — **note:** this signal kind is the
    0-based special enum from `bewildered_core::SpecialGem`; `get_cell` reports the 1-based
    game-facing id, a small consistency wart to reconcile in Stage 6).
  - `echo_charged(cells)`, `echo_detonated(cells, multiplier)`
  - `move_rejected(ax, ay, bx, by)`
  - `objective_progress(current, target)` — currently a **hardcoded placeholder** (`combo` vs `10000`);
    Stage 6 must replace this with authoritative level-objective tracking.

### Godot (presentation only) — `scripts/board.gd`, `scripts/gem.gd`
- `GameBoard` instantiates `BoardSim`, connects its signals, and renders the board by polling
  `get_cell()` via `refresh_board()`.
- `Gem` renders a procedural shape+color for `kind` and overlays an emoji glyph for `special`
  (⚡/🌈/💥) through a centered `Label` (`special_overlay`).
- Godot handles tweens, gravity/bounce animation, procedural particle flashes, HUD labels, and input.
- **AudioManager** autoload (`scripts/audio_manager.gd`) converts board signals into native
  AudioServer-bus-agnostic procedural SFX/music.

### Contract invariants
1. Every mutation that changes board state goes **through `BoardSim`**; Godot never writes rules state.
2. Godot reads board state by **querying `get_cell`** (single source of truth).
3. Cascade, echo, special, and objective truths are all decided inside the Rust cascade loop; Godot
   only *animates* what the signals describe.
4. `_sync_board_state()` / `refresh_board()` re-queries all 64 (or N×M) cells after each move so the
   visuals and the simulation can never diverge (DEVIATIONS §Stage 4: "guaranteeing 64 gems on every
   move").

---

## 2. Current State of the Codebase (post-Stages 0–5)

| Area | State |
|------|-------|
| Rust workspace (`rust/`) | 4 crates; `cargo build` passes cleanly (dev profile). `libbewildered_godot.so` loads in Godot 4.7.2. |
| FFI bridge (`bewildered-godot`) | `BoardSim` full API + signals; `objective_progress` is a Stage-1 placeholder. |
| Content crate | `Level`, `Pack`, `Objective`, `Blocker`, `Relic` models + RON/zip loaders + `validate_level`. Campaign RON files present under `rust/crates/bewildered-content/assets/campaign/` (manifest + campaign-001..008). |
| Gem rendering | Procedural shape+color by `kind`; echo tint; **emoji `special` overlays wired** (⚡🌈💥). |
| Core loop | Mouse/keyboard select → swap → cascade → gravity → spawn, with input guarding (`is_animating`). |
| Juice | Tweens (swap/clear/fall/bounce), procedural particle flashes, rejection flash. |
| Audio | `AudioManager` autoload, Master/Music/SFX buses, cascade pitch escalation. |
| HUD | **Debug-only** `Panel` HUD in `board.gd` (top-right). No level HUD yet. |
| Modals | None. No victory/defeat flow. |

### Uncommitted working-tree changes (present when I onboarded)
- `commit_msg.txt` deleted (transient artifact).
- `rust/crates/bewildered-core/src/lib.rs`, `rust/crates/bewildered-godot/src/lib.rs` — special-gem
  exposure groundwork.
- `scripts/board.gd` — passes `special` to `set_gem()` in `refresh_board()`.
- `scenes/debug_hud.tscn` untracked (currently unused; debug HUD is built in code).

I treated these as in-flight Gate-2 work and completed them rather than reverting.

---

## 3. Immediate Roadmap — Complete Gate 2, then Stage 6

### Gate 2 (done this session)
1. **`gem.gd` layout warning fix** — `special_overlay` now uses `PRESET_CENTER` + explicit
   `horizontal/vertical_alignment`, `position`, and `custom_minimum_size` (no more anchor fighting).
2. **Special-gem FFI IDs** — `get_cell` reports `special` (0/1/2/3). Verified via `cargo build`.
   **Extra fix:** `_animate_new_gems_spawn()` in `board.gd` now forwards `special` to `set_gem()`
   so special gems keep their emoji when re-spawned during cascade gravity drops (was losing it).

### Stage 6 — Content & Level HUD (next)
1. Copy `.ron` level packs into a root `levels/` directory (source:
   `rust/crates/bewildered-content/assets/campaign/` — the `BewilderedForMighration/.../levels/` path
   referenced in orientation is empty; the content-crate campaign is the authoritative pack).
2. Extend `BoardSim` to **load a level** via `bewildered-content::Level` (grid, gem set, objective,
   blockers, seed) and track **authoritative objective_progress**:
   - getters: `get_moves_remaining()`, `get_target_score()`, `get_objective_description()`,
     `is_level_cleared()`, `is_level_failed()`.
   - Replace the hardcoded `objective_progress(current=combo, target=10000)` with real objective
     accounting in the `try_swap` cascade loop.
3. Build a clean Control/CanvasLayer HUD (`scenes/hud.tscn`): Level Title, Moves Left,
   Target-Objective progress bar, Score.
4. Add **Chamber Clear** victory modal and **Out-of-Moves** retry modal
   (`scenes/level_complete_dialog.tscn`).
5. MCP screenshot verification of the HUD, objective tracking, and level transitions.

### Decisions/notes carried forward
- **Reconcile special-id naming** between `special_gem_created` (0-based) and `get_cell` (1-based).
- **Objective types** to support: `ScoreTarget`, `Collection`, `Descent` (blockers), `Survival`.
  The core `Board` already handles rules; content crate provides the model; `BoardSim` must bridge
  them into the Godot HUD.
- The debug HUD and the new level HUD should remain separable (debug panel is a dev aid; level HUD is
  the shipped UI).