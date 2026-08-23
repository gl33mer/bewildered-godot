# Mission: Bewildered — TUI -> Godot 4 Migration

You are an expert Rust systems programmer and Godot 4 developer. We are migrating **Bewildered** (a Bejeweled-style match-3 with Resonance Echoes and roguelike Descent runs) from a terminal (TUI) game to Godot 4.7 using GDExtension (`gdext`).

The smoke test is complete and Godot AI MCP is active and connected.

---

### 1. Legacy Repository & Reference Path
The original working TUI codebase and documentation are located at:
`../../BewilderedForMighration/bewildered/`

Read the logic, crates, and docs from there. **Do not modify that directory.**

---

### 2. The Non-Negotiable Boundary Contract

* **Rust owns every rule.** `bewildered-core` handles board simulation, deterministic cascade resolution, Resonance Echoes, relic triggers, scoring, and seeds. Godot/GDScript must never re-implement match-3 rules.
* **Godot owns presentation.** GDScript translates player input into Rust calls (e.g. `try_swap(x1, y1, x2, y2)`) and translates Rust signals (`match_resolved`, `special_gem_created`, `echo_detonated`, etc.) into tweens, audio, and particle effects.
* **`bewildered-godot` is thin glue.** It is a `cdylib` GDExtension crate that exposes `BoardSim` and emits Godot signals. No gameplay logic belongs here.
* **No wildcard matches:** Exhaustive pattern matching only in Rust and GDScript.

---

### 3. Target Workspace Layout (Root: `.`)

Scaffold the project layout at the current directory:

```text
.
|-- project.godot
|-- bewildered.gdextension         # GDExtension manifest
|-- rust/
|   |-- .gdignore                  # Crucial: Tells Godot to ignore target/
|   |-- Cargo.toml                 # Workspace root
|   `-- crates/
|       |-- bewildered-core/       # Copied verbatim from ../../BewilderedForMighration/bewildered/crates/bewildered-core
|       |-- bewildered-content/    # Copied verbatim from ../../BewilderedForMighration/bewildered/crates/bewildered-content
|       |-- bewildered-solver/     # Copied verbatim from ../../BewilderedForMighration/bewildered/crates/bewildered-solver
|       `-- bewildered-godot/      # NEW: GDExtension cdylib bridge
|-- scenes/
|-- scripts/
|-- assets/
|-- levels/
|-- Docs/                          # Copied from ../../BewilderedForMighration/bewildered/Docs/
|-- HANDOFF.md
`-- Docs/DEVIATIONS.md
```

#### Key Scaffolding Files to Generate:

1. **`rust/.gdignore`**: Empty file.
2. **`rust/Cargo.toml`**:
```toml
[workspace]
members = [
    "crates/bewildered-core",
    "crates/bewildered-content",
    "crates/bewildered-solver",
    "crates/bewildered-godot",
]
resolver = "2"
```

3. **`rust/crates/bewildered-godot/Cargo.toml`**:
```toml
[package]
name = "bewildered-godot"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
godot = { version = "0.2", features = ["experimental-threads"] }
bewildered-core = { path = "../bewildered-core" }
bewildered-content = { path = "../bewildered-content" }
```

4. **`rust/crates/bewildered-godot/src/lib.rs`**:
```rust
use godot::prelude::*;

struct BewilderedExtension;

#[gdextension]
unsafe impl ExtensionLibrary for BewilderedExtension {}
```

5. **`bewildered.gdextension`** (at project root):
```ini
[configuration]
entry_symbol = "gdextension_initialize"
compatibility_minimum = 4.1

[libraries]
linux.debug.x86_64 = "res://rust/target/debug/libbewildered_godot.so"
linux.release.x86_64 = "res://rust/target/release/libbewildered_godot.so"
```

---

### 4. Illustrative Shape of `BoardSim` (For Stage 1+)

```rust
// rust/crates/bewildered-godot/src/board_sim.rs
use godot::prelude::*;
use bewildered_core::Board;

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct BoardSim {
    board: Option<Board>,
    base: Base<RefCounted>,
}

#[godot_api]
impl BoardSim {
    #[func]
    fn new_board(&mut self, width: i32, height: i32, seed: i64) { /* ... */ }

    #[func]
    fn try_swap(&mut self, ax: i32, ay: i32, bx: i32, by: i32) -> bool { /* ... */ }

    #[func]
    fn get_cell(&self, x: i32, y: i32) -> Dictionary { /* initial board draw inspection */ }

    #[signal]
    fn match_resolved(cleared_cells: Array<Vector2i>, gem_kind: i32, cascade_depth: i32);

    #[signal]
    fn special_gem_created(pos: Vector2i, kind: i32);

    #[signal]
    fn echo_charged(cells: Array<Vector2i>);

    #[signal]
    fn echo_detonated(cells: Array<Vector2i>, multiplier: f32);

    #[signal]
    fn move_rejected(ax: i32, ay: i32, bx: i32, by: i32);

    #[signal]
    fn objective_progress(current: i64, target: i64);
}
```

---

### 5. Staged Build Plan

Work top to bottom. Verify each stage before moving on (`cargo test`, `cargo build`, and **MCP screenshots/inspections** for visual stages). Commit after each verified stage and maintain `Docs/DEVIATIONS.md` and `HANDOFF.md`.

- [ ] **Stage 0 -- Scaffolding & Verification:**
  - Verify environment (`rustc`, `cargo`, `godot`).
  - Copy `bewildered-core`, `bewildered-content`, `bewildered-solver`, and `Docs/` verbatim from `../../BewilderedForMighration/bewildered/`.
  - Set up `rust/Cargo.toml`, `rust/.gdignore`, `bewildered.gdextension`, and `bewildered-godot`.
  - Confirm `cargo test --workspace` passes 100% identically to the old repo.
  - Compile `cargo build` and verify Godot loads the empty extension without crashing.
- [ ] **Stage 1 -- FFI Round-Trip Proof:**
  - Implement minimal `BoardSim` in `bewildered-godot` exposing `try_swap`, `get_cell`, and core signals.
  - Create a headless test scene calling `try_swap` and printing signals to the Godot output log. Prove the boundary works end-to-end.
- [ ] **Stage 2 -- Static Board Render:**
  - Build `board.tscn` and `gem.tscn` (`Sprite2D`). Render initial grid from `BoardSim`. Verify glyph/shape distinction across all gem types. Capture an MCP screenshot to verify.
- [ ] **Stage 3 -- Input & Core Playable Loop:**
  - Implement mouse/keyboard selection & swap input. Wire `try_swap` to signal handlers for instant grid refresh and rejection feedback. Capture an MCP screenshot and test a full clear.
- [ ] **Stage 4 -- Juice & Animations:**
  - Add `Tween` animations for swap, fall, and clear. Add `GPUParticles2D` for gem shatter and resonance echo detonation. Capture MCP screenshots.
- [ ] **Stage 5 -- Native Audio:**
  - Set up audio buses, pitch-escalating match SFX per cascade step, and dynamic music intensity.
- [ ] **Stage 6 -- Content & Level HUD:**
  - Load real `.ron` level packs via `bewildered-content`. Build Godot `Control` HUD (score, moves, objective). Play full campaign levels.
- [ ] **Stage 7 -- Descent Mode & Relic Selection:**
  - Implement Descent run flow and relic selection UI according to `03-GAME-DESIGN.md` (logic in Rust, presentation in Godot).
- [ ] **Stage 8 -- Export & Build Validation:**
  - Confirm `cargo build --release` compiles cleanly and validate Linux desktop export.
- [ ] **Stage 9 -- Polish & Packaging:**
  - Settings, clean `.gitignore`, README with screenshots, license file.
- [ ] **Stage 10 (Deferred)** -- Godot editor plugin for level authoring (out of scope until Stages 0-9 are complete).

---

### 6. Your Immediate Action: Execute Stage 0 Only

1. Port crates and Docs from `../../BewilderedForMighration/bewildered/`.
2. Scaffold the workspace files (`rust/Cargo.toml`, `rust/.gdignore`, `bewildered.gdextension`, and `bewildered-godot`).
3. Run `cargo test --workspace` inside `rust/` to verify byte-for-byte logic parity.
4. Run `cargo build` and verify via MCP/Godot that the extension loads cleanly.
5. Initialize `HANDOFF.md` and `Docs/DEVIATIONS.md`.
6. **Stop and report** the Stage 0 summary and test results before proceeding to Stage 1.
