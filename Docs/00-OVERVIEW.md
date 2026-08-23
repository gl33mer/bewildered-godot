# Bewildered — Project Overview

**Bewildered** is a Bejeweled-inspired match-3 game that lives entirely in a terminal, built to be
handed to a **Pi coding agent** running inside **Omarchy Quattro** (Arch + Hyprland/Quickshell) for
implementation. This document is the entry point to the full spec set. Read it first, then follow
the numbered files in order. `06-BUILD-STAGES-CHECKLIST.md` is the actual task list — check items
off there as you go.

## Spec file map

| File | Purpose |
|---|---|
| `00-OVERVIEW.md` | This file — vision, pillars, twists summary |
| `01-TECH-STACK.md` | Chosen stack, rejected alternatives, rationale |
| `02-ARCHITECTURE.md` | Cargo workspace layout, crate responsibilities, data flow |
| `03-GAME-DESIGN.md` | Core match-3 rules, gem types, the twists, scoring, run structure |
| `04-RENDERING-AUDIO.md` | TUI rendering tiers, particle system, audio design |
| `05-LEVEL-FORMAT-AUTHORING.md` | RON level schema, campaign packs, the editor tool, solvability validation |
| `06-BUILD-STAGES-CHECKLIST.md` | Staged, checkbox task list from `cargo init` to shippable build |
| `07-OMARCHY-SETUP.md` | Omarchy Quattro-specific environment setup, packages, terminal/audio notes |

## Design pillars

1. **"Forget it's a terminal."** Every visual and audio decision should push toward the dopamine
   feedback of a native Bejeweled-style game — cascading particles, combo escalation, punchy SFX —
   using only what a terminal can give us (Unicode glyphs, 24-bit color, and, where available,
   Kitty/Sixel graphics). Nothing should ever look like a spreadsheet.
2. **Runs everywhere, shines in the best terminals.** The game must be fully playable in a bare
   80x24 `xterm` over SSH, and progressively more beautiful in a modern terminal (Ghostty, Kitty,
   WezTerm) — see the three-tier rendering strategy in `04-RENDERING-AUDIO.md`.
3. **Performance is a feature.** 60fps-equivalent particle/animation updates, sub-millisecond board
   logic, no allocation churn in the hot loop. A match-3 board is tiny (≤12x12); there is no excuse
   for jank.
4. **Content is data, not code.** Levels, campaigns, gem sets, and relics are all data files. Adding
   a new level pack should never require a Rust rebuild.
5. **Authoring is a first-class tool, not an afterthought.** The level editor is a real application
   with live preview, solvability checking, and pack export — built early enough that later levels
   are made *with* it.

## The twist: what makes Bewildered not-just-Bejeweled

Modern match-3 has moved on from "swap two gems, clear a board, repeat" (see research notes below).
The strongest current work in the space — *Match Morphosis* ("Candy Crush meets Slay the Spire"),
*House and Hand*, *Lumines Arise* (beat-synced visuals/audio), *Two Dots*-style objective boards —
combines match-3's tactile core with either **roguelike run structure** or **rhythm-driven
feedback**. Bewildered takes both:

- **Resonance Echoes** — the core new mechanic. See `03-GAME-DESIGN.md §Resonance Echoes` for full
  rules; in short, a match leaves a one-turn "echo" charge on the cells it cleared. If a new gem
  lands there and gets matched on the *next* turn, the echo detonates for a chain bonus. This turns
  match-3 from a purely reactive swap-and-see game into a light one-move-ahead planning game,
  without adding any UI complexity — the echo is just a shimmering glyph on the board.
- **Descent structure** — the game is organized as a *run*: a sequence of boards ("chambers") with
  escalating objectives, punctuated by a **Relic** choice between chambers (a passive rule-modifier,
  e.g. "diagonal matches count", "a 5th gem color enters the pool", "clearing a corner refunds a
  move"). Runs are seeded, so a "Daily Descent" seeded-challenge mode is close to free.
- **Beat-aware feedback, not literal rhythm-game timing.** No rhythm-game input timing requirement
  (that would fight the turn-based swap core) — instead the *adaptive music* (layered stems via
  `kira`) and SFX pitch/velocity scale with combo streak length, and particle density/color
  saturation scale with cascade depth, so a big cascade *feels* like a beat drop even though input
  is untimed.
- **Solver-verified levels.** Every hand-authored or generated level is run through a headless
  solvability checker before it ships. No "impossible board" bugs.

## Research notes (for context, not exhaustive)

- **Omarchy Quattro** (released Aug 2026) is DHH's Arch-based, Hyprland/Quickshell desktop. It ships
  a curated package set (Neovim, build tooling, etc.), its own `omarchy` package channel over
  pacman, full AUR access (~117k packages), and — notably — treats coding agents as system citizens
  (a Claude Code usage widget in the bar, a `Pi` desktop theme, agent skills symlinked into
  `~/.claude/skills`). Practically: **pacman + AUR give us everything needed** (Rust toolchain,
  ALSA/PipeWire dev headers, a modern terminal emulator) with no exotic setup. See
  `07-OMARCHY-SETUP.md`.
- **Terminal graphics protocols** have matured: `ratatui-image` unifies Sixel, Kitty Graphics
  Protocol, iTerm2, and a Unicode half-block fallback behind one API, auto-detecting terminal
  capability. This is what makes the three-tier rendering strategy possible without three codebases.
- **Modern match-3 direction**: roguelike-deckbuilder hybridization (*Match Morphosis*, *House and
  Hand*) and audio-reactive presentation (*Lumines Arise*) are the two live trends worth taking
  from; Bewildered borrows structure from the former and feedback philosophy from the latter.

Proceed to `01-TECH-STACK.md`.
