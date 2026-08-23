# Bewildered — Technology Stack

## Decision: Rust

Rust over Go for this project. Both are readily available on Omarchy Quattro (pacman has both), but
Rust wins on every axis that matters here:

- The two crates that make the "forget it's a TUI" goal achievable — **ratatui** (TUI framework) and
  **ratatui-image** (Sixel/Kitty/iTerm2 unified image rendering) — are Rust-first and best-in-class.
  Go's TUI ecosystem (bubbletea, tcell) has no equivalent unified graphics-protocol layer.
- Predictable, allocation-controllable performance for the particle system and board simulation
  without a GC pause ever showing up as dropped frames.
- `serde` + RON gives us a very low-ceremony, strongly-typed level-data pipeline shared identically
  between the game and the editor.
- Single static-ish binary, trivial to package for Arch/AUR.

## Core crates

| Concern | Crate | Why |
|---|---|---|
| Terminal UI framework | `ratatui` | The de facto standard Rust TUI library; immediate-mode widget model fits a redraw-every-frame game loop naturally. |
| Terminal backend | `crossterm` | Cross-platform (works fine over SSH too), pairs natively with ratatui. |
| Enhanced graphics | `ratatui-image` | Auto-detects Sixel / Kitty Graphics Protocol / iTerm2, falls back to Unicode half-blocks. This is the entire enabler of Tier 2/3 rendering — see `04-RENDERING-AUDIO.md`. |
| Audio | `kira` | Purpose-built game-audio crate (not just playback like `rodio`): supports layered/stem-based adaptive music, real-time parameter tweening (pitch, volume), and sound "instances" — exactly what combo-scaling SFX and adaptive music stems need. |
| Serialization | `serde` + `ron` | RON (Rusty Object Notation) is human-writable, diff-friendly in git, and native to Rust's type system — ideal for hand-authored level files. |
| RNG (seeded runs) | `rand` + `rand_chacha` | `ChaCha8Rng` gives deterministic, reproducible seeded runs (needed for Daily Descent and for the solvability validator to replay a level exactly). |
| CLI / arg parsing | `clap` (derive) | For the game binary's launch flags and the editor/validator/solver CLI tools. |
| Error handling | `thiserror` (lib crates) + `anyhow` (binaries) | Standard split. |
| Property/fuzz testing | `proptest` | For match-detection and cascade-resolution correctness. |
| Packaging (level packs) | `zip` | `.bwpack` files are just zip archives of RON + metadata; trivial and inspectable. |

## Explicitly deferred / rejected

- **Full ECS (`bevy_ecs`, `hecs`)** — considered, rejected for v1. A match-3 board tops out at a few
  hundred live entities (gems + particles); a plain data-oriented design (flat `Vec`/`SmallVec`
  buffers, struct-of-arrays where profiling calls for it) is simpler, has zero framework overhead,
  and is easier for an agent to reason about and modify. **Revisit in Stage 5+**: if the Relic/status
  effect system (roguelike modifiers stacking rules) grows complex enough that ad-hoc `match`
  dispatch becomes unwieldy, adopt `hecs` (lightweight, no plugin system, easy to bolt onto an
  existing loop) specifically for relic/status-effect entities — not for the board or particles.
- **`notcurses`** — the C library with the deepest terminal-graphics feature set, but it's a C
  dependency with its own build/runtime story, and `ratatui-image` already gets us Sixel/Kitty/iTerm2
  from pure Rust. Not worth the FFI/build complexity for this project.
- **`sled`/SQL for scores** — overkill. Local high scores / Daily Descent results are a small JSON
  file in `$XDG_DATA_HOME/bewildered/`.
- **Go + bubbletea** — a completely viable second choice if the agent or future maintainers strongly
  prefer Go; noted here so the decision is legible, but Rust is the recommendation and what the rest
  of this spec assumes.

## Minimum Rust version & toolchain

- Rust 2024 edition, MSRV tracks latest stable (Omarchy ships/updates via `rustup`, no reason to
  pin old).
- `cargo` workspace (see `02-ARCHITECTURE.md`) with 5 members.

Proceed to `02-ARCHITECTURE.md`.
