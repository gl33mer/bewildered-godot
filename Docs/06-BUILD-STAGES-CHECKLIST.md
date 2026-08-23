# Bewildered — Build Stages & Checklist

**Instructions for the agent**: work top to bottom. Check off each box (`- [x]`) as you complete and
verify it — "verify" means it builds/passes tests, not just "code written". Don't skip ahead; later
stages assume earlier ones are solid, especially Stage 1–3 which everything else depends on. Commit
after each completed stage with a message referencing the stage number. If a task reveals the spec
is wrong or underspecified, fix it by *updating the relevant spec file* and noting the change at the
bottom of this file under "Deviations from spec", rather than silently improvising — the spec files
are the source of truth for design intent.

Read `07-OMARCHY-SETUP.md` before Stage 0.

---

## Stage 0 — Environment & scaffolding
- [x] Confirm Rust toolchain via mise: `mise ls rust`; if unmanaged/missing, `mise use -g rust@latest`
      or (preferred) add a project-root `mise.toml` pinning it. Do **not** default to a manual
      `rustup`-installer script on this OS — see `07-OMARCHY-SETUP.md §Toolchain` for why and for the
      `mise doctor` fallback if `cargo`/`rustc` still aren't on `PATH` after activation.
- [x] Confirm system audio dev headers available (PipeWire/ALSA) for `kira`'s `cpal` backend.
- [x] `cargo new` the workspace root, create the 5-member workspace layout from
      `02-ARCHITECTURE.md` (`bewildered-core`, `bewildered-content`, `bewildered-render`,
      `bewildered-audio`, `bewildered-tui`, `bewildered-editor`, `bewildered-solver` — 4 libs + 3 bins).
- [x] Root `Cargo.toml` workspace members + shared `[workspace.dependencies]` pins for
      ratatui/crossterm/ratatui-image/kira/serde/ron/rand/rand_chacha/clap/thiserror/anyhow/proptest/zip.
- [x] `bewildered-tui` boots to a blank ratatui screen and exits cleanly on `q`/`Ctrl-C` (raw mode
      enter/leave handled correctly — verify terminal isn't left broken on panic; use a panic hook
      that restores the terminal).
- [x] CI-equivalent local check: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace`
      all clean. Set this up as a `justfile` or `Makefile` target now — it'll be run constantly.

## Stage 1 — Core simulation (`bewildered-core`)
- [x] Board representation: flat `Vec<Option<Gem>>` sized `width*height`, `Gem { kind, echo: Option<EchoCharge> }`.
- [x] Match detection: scan rows+columns for runs of 3+ identical `GemKind`, return match shapes
      (not just cleared cells — need shape to determine Bolt/Prism/Nova creation per `03-GAME-DESIGN.md`).
- [x] Swap legality check (would this swap create a match) + swap application + snap-back on illegal.
- [x] Gravity/refill: collapse column gaps down, spawn new gems from top using the level's active
      `gem_types` and the run's seeded RNG.
- [x] Cascade loop: after a swap resolves, re-scan-and-clear-and-refill until no more matches, with a
      combo-depth counter emitted per step (this is `core`'s contract with the presentation layer).
- [x] Special gem creation (4→Bolt, 5-L/T→Prism, 5-line→Nova) + activation effects.
- [x] Resonance Echo system exactly per `03-GAME-DESIGN.md §Resonance Echoes`: charge on clear,
      carried by falling-in gem, one-move expiry, detonation extra-ring-clear + multiplier stacking.
- [x] Scoring per `03-GAME-DESIGN.md §Scoring`, constants named and centralized.
- [x] `proptest` coverage: no swap ever produces a board with an "impossible" cell state; cascade
      resolution always terminates (no infinite loop) on randomized boards up to 16x16.
- [x] Deterministic replay test: same seed + same move sequence → byte-identical resulting board and
      score, every time (this underpins both Daily Descent and the solver).

## Stage 2 — Content pipeline (`bewildered-content`)
- [x] `Level`, `Objective`, `Blocker`, `Pack` types per `05-LEVEL-FORMAT-AUTHORING.md`, `serde`-derived.
- [x] RON load/save round-trip tests (load a hand-written `.ron`, verify fields; save, reload, diff).
- [x] `.bwpack` zip read/write (manifest + level files), and loose-directory loading for in-dev packs.
- [x] Write 3–5 hand-authored placeholder levels exercising each `Objective` variant — these are
      throwaway/test fixtures, not the final campaign.
- [x] Embed a tiny default campaign in the `bewildered-tui` binary (`include_dir!` or build-script
      copy) so the game is playable with zero external files on first run.

## Stage 3 — Solver (`bewildered-solver`)
- [x] Static sanity checks per `05-LEVEL-FORMAT-AUTHORING.md §Solvability validation` step 1.
- [x] Bounded greedy/lookahead search implementation, step 2 — report best-reached progress on
      failure, not just pass/fail.
- [x] No-softlock sampling check, step 3.
- [x] CLI: `bewildered-solver check <level.ron>` and `check-pack <dir>`, human-readable + `--json`
      output (the editor will consume `--json`).
- [x] Run the solver against every Stage 2 placeholder level; fix or discard any that fail.

## Stage 4 — Rendering (`bewildered-render`) + Tier 1 in-game
- [x] Tier detection wrapper around `ratatui-image`'s picker (env-var guess → control-sequence query
      → config override), exposed as a single `RenderTier` enum the rest of the crate switches on.
- [x] Tier 1 board widget: gem glyph+color per cell, legible at 80x24, colorblind-safe glyph set per
      `03-GAME-DESIGN.md §Gem set & accessibility`.
- [x] Particle pool + update loop per `04-RENDERING-AUDIO.md §Particle system`, Tier 1 drawing
      (ASCII spark glyphs).
- [x] HUD: score, moves remaining, objective progress, combo counter, active relics list.
- [x] Wire `bewildered-tui`'s main loop per the shape in `02-ARCHITECTURE.md §Main loop shape`:
      input → `core::apply_move` → particle/audio triggers → animate → draw.
- [x] `criterion` benchmark for board match-scan + cascade resolution on a 12x12 board; confirm it
      clears the <100µs target from `02-ARCHITECTURE.md` (informational — don't over-optimize past
      this, just confirm no accidental O(n²)+ blowup).
- [x] **Playable milestone**: a full game of standalone-level Bewildered, Tier 1 rendering, no audio
      yet, is playable start to finish. This is a good point to pause and get human feedback if
      possible before layering on audio/higher tiers.

## Stage 5 — Audio (`bewildered-audio`)
- [x] `kira` wrapper API: `play_sfx`, `set_music_intensity`, silent no-op backend when no audio
      device is available (must not crash headless/SSH sessions).
- [x] Source or produce placeholder SFX/music assets (short synthesized/CC0 placeholders are fine for
      early stages — flag clearly in `assets/README.md` which assets are placeholders vs. final).
- [x] Wire SFX triggers to `MoveOutcome` events (swap, match sizes, special creation/activation,
      cascade steps with pitch-up, Resonance Echo detonation, chamber clear, relic pick, run loss).
- [x] Wire adaptive music intensity to recent combo activity (decay toward 0 when idle).
- [x] Config toggles: mute all, mute music only, mute SFX only, low-fx particle density.

## Stage 6 — Descent mode & Relics
- [x] `Relic` type + effect application hooks into `bewildered-core` (needs `core` to expose
      extension points for rule modifiers — e.g. a `RuleModifiers` struct threaded through match
      detection/scoring rather than relics reaching into private state).
- [x] Relic pool data + 8–12 initial relics per `03-GAME-DESIGN.md §The Descent` examples (include at
      least 2 explicitly anti-synergistic pairs for build-decision tension).
- [x] Chamber sequencing, relic-offer screen (3 choices) between chambers, run win/loss handling.
- [x] Daily Descent: date→`u64` seed hashing, deterministic chamber+relic sequence, local
      high-score/result storage in `$XDG_DATA_HOME/bewildered/`.

## Stage 7 — Tier 2 & Tier 3 rendering
- [x] Tier 2: half-block/braille gem rendering (pre-baked lookup tables), sub-cell particle motion
      using `f32` world coords quantized at draw time, screen-shake/flash effects.
- [x] Tier 3: author or source a minimal gem + particle sprite sheet, wire through `ratatui-image`'s
      Kitty/Sixel/iTerm2 path.
- [x] Manual verification pass across at least: a bare `xterm`/SSH session (Tier 1), a true-color
      terminal without graphics protocol e.g. Alacritty (Tier 2), and Kitty or Ghostty (Tier 3).

## Stage 8 — Editor (`bewildered-editor`)
- [x] Editor crate compiles cleanly (`make ci` passes)
- [x] `bewildered-solver` split into library + CLI; public API (`validate_level`, `validate_level_file`, `validate_pack`)
- [x] Editor calls solver library in-process (no process spawn)
- [x] Result type consistency: `anyhow::Result` used throughout
- [x] Exhaustive matches on `EditorMode` (no wildcard)
- [x] Ownership issues fixed (`.as_ref()`, `.clone()`)
- [x] Key bindings: uppercase K/J for level move to avoid collision with cursor movement
- [ ] Grid pane with paint/cursor controls per `05-LEVEL-FORMAT-AUTHORING.md §The editor`.
- [ ] Objective panel, blocker placement, gem-type-pool selection.
- [ ] Live preview pane using `bewildered-render` directly (not a reimplementation).
- [ ] Pack panel: level ordering, relic pool assignment, `.bwpack` export.
- [ ] Undo/redo command stack on grid edits.
- [ ] Use the editor to author the real starter campaign (replacing Stage 2's throwaway fixtures) —
      aim for a first campaign of 12–20 chambers with a difficulty curve, all solver-validated.

## Stage 9 — Polish & packaging
- [ ] `--low-fx`, `--render-tier`, `--mute*` CLI flags mirrored in `config.toml`.
- [ ] Startup terminal-size check with a friendly message if below minimum playable size, rather than
      a garbled layout.
- [ ] `PKGBUILD` for Arch/AUR packaging of `bewildered-tui` + `bewildered-editor` (Omarchy targets
      pacman/AUR-native installs — see `07-OMARCHY-SETUP.md`).
- [ ] README with screenshots/asciinema-style capture (Tier 1 and Tier 3 if feasible) and a "playable
      over SSH" callout — that's a genuine differentiator worth marketing in the README itself.
- [ ] Final full-workspace check: `cargo build --release --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings`.

## Stage 10 (stretch, optional) — Generation assist & sharing
- [ ] `bewildered-editor --generate` procedural-fill-then-solve loop per
      `05-LEVEL-FORMAT-AUTHORING.md §Level generation assist`.
- [ ] Daily Descent shareable result string (Wordle-style emoji/text summary, no networking required).

---

## Deviations from spec
*(Agent: log here — one line per deviation — any point where implementation reality forced a change
from what a spec file says, and update that spec file itself to match. Keep this list, don't delete
entries.)*

- Stage 0: Used `StdRng` (from `rand::rngs`) instead of `ChaCha8Rng` directly due to trait bound issues with `rand_chacha` 0.10 and `rand` 0.8 compatibility.
- Stage 2: RON schema uses `grid` struct instead of separate `width`/`height` fields for consistency with `GridSize` type
- Stage 8: `bewildered-solver` split into lib + bin (was binary-only); editor calls solver library API; `Level.gems` field made optional with `serde(default)` for embedded campaign compatibility; editor TUI is stubbed (compiles but not fully implemented)