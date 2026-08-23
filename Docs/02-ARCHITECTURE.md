# Bewildered — Architecture

## Cargo workspace layout

```
bewildered/
├── Cargo.toml                 # workspace root
├── crates/
│   ├── bewildered-core/       # board sim, rules, RNG, scoring — no I/O, no rendering
│   ├── bewildered-content/    # level/pack data model + RON (de)serialization + solvability checker
│   ├── bewildered-render/     # ratatui widgets, particle system, 3-tier rendering, shared by game + editor
│   ├── bewildered-audio/      # kira wrapper: SFX bank, adaptive music stem controller
│   └── bewildered-tui/        # (bin) the actual game
│   └── bewildered-editor/     # (bin) level/pack authoring tool
│   └── bewildered-solver/     # (bin) headless CLI: validate/solve levels, used in CI and by the editor
├── assets/
│   ├── sfx/                   # short .ogg samples
│   ├── music/                 # stem .ogg files per track
│   └── glyphs/                # optional pixel-art sprites for Tier 3 (Kitty/Sixel) rendering
├── levels/                    # first-party campaign(s), plain RON files, organized by pack
└── docs/                      # this spec set lives here once checked into the repo
```

Five library/binary boundaries, deliberately:

- **`bewildered-core`** is pure logic: the board grid, match detection, cascade resolution, the
  Resonance Echo system, scoring, move validation, relic/status-effect application. Zero
  dependencies on ratatui/kira/terminal anything. This is what `proptest` and the solver hammer on,
  and it's what makes headless (non-rendered) simulation possible for the validator.
- **`bewildered-content`** owns the level/pack **data model** and RON parsing (see
  `05-LEVEL-FORMAT-AUTHORING.md` for the schema). Depends on `bewildered-core` for the types a level
  configures (gem sets, objectives) but not vice versa.
- **`bewildered-render`** is the shared presentation layer: ratatui widgets for the board, HUD,
  menus; the particle system; the tier-detection/fallback logic wrapping `ratatui-image`. Both the
  game and the editor draw the board through this crate so they never visually drift apart —
  **the editor's live preview is not a mockup, it's the real renderer.**
- **`bewildered-audio`** wraps `kira`: exposes a small API (`play_sfx(SfxId, ComboLevel)`,
  `set_music_intensity(f32)`) so game/editor code never touches `kira` directly.
- **`bewildered-tui`**, **`bewildered-editor`**, **`bewildered-solver`** are thin binaries that wire
  the library crates together into an event loop, an editor UI, and a CLI respectively.

## Why this split

- An agent (or human) working on match-detection logic never needs to touch rendering code, and vice
  versa — reduces the blast radius of any single change, which matters a lot when an autonomous
  agent is doing the editing.
- `bewildered-core` being dependency-free and deterministic (seeded RNG) is what makes the
  **solvability validator** possible: it can run thousands of headless simulated playouts per second
  with no terminal, no audio, no allocation surprises.
- Sharing `bewildered-render` between game and editor is the single most important architectural
  decision for the "authoring tool" requirement — it guarantees WYSIWYG and means rendering
  improvements (new particle effects, a new tier-3 sprite) benefit the editor for free.

## Main loop shape (`bewildered-tui`)

Turn-based simulation, real-time presentation — standard pattern for this genre:

```
loop {
    poll input (crossterm event, non-blocking, ~16ms budget)
    if a player move was made:
        core::apply_move(&mut board, mv) -> MoveOutcome   // pure, synchronous, cheap
        render::spawn_particles_for(outcome)               // queues animation, does not block
        audio::trigger_for(outcome)
    advance particle simulation by dt
    advance music intensity toward target (kira tween)
    render::draw(frame, &board, &particles, &hud)           // ratatui draw call
}
```

Board *logic* is instant (a match resolves in one function call); everything "juicy" — cascades
animating in sequence, particles bursting, screen-shake, combo-counter climbing — is purely a
presentation-layer animation queue consuming the already-computed `MoveOutcome`. This separation
(instant authoritative logic, animated presentation) is what keeps the game logically simple *and*
lets the presentation layer be as extravagant as the terminal allows.

## Performance targets

- Steady-state frame budget: 16ms (60Hz-equivalent redraw), degrading gracefully to the terminal's
  actual refresh cadence over SSH.
- Zero heap allocation in the per-frame particle update path (pre-allocated particle pool, fixed
  capacity, oldest-recycled).
- Board operations (match-scan, cascade resolution) on a 12x12 board: target < 100µs; this is a
  non-issue with Rust and a flat array board representation but is called out because it's the kind
  of thing worth a `criterion` benchmark in Stage 4.

## Configuration & data locations (XDG)

- Settings: `$XDG_CONFIG_HOME/bewildered/config.toml`
- Save/progress/high scores: `$XDG_DATA_HOME/bewildered/`
- User-installed level packs: `$XDG_DATA_HOME/bewildered/packs/*.bwpack`
- First-party levels ship inside the binary via `include_dir!` or are read from
  `/usr/share/bewildered/levels` when installed as a system package (support both: embedded default
  campaign, external packs loaded at runtime).

Proceed to `03-GAME-DESIGN.md`.
