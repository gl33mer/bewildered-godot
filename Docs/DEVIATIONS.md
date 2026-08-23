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

*Pending — to be filled during Stage 2*

---

### Stage 3: Input & Core Playable Loop

*Pending — to be filled during Stage 3*

---

### Stage 4: Juice & Animations

*Pending — to be filled during Stage 4*

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