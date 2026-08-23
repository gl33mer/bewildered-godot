# Bewildered — Game Design

## Core loop (Bejeweled baseline)

- Grid of gems, default 8x8 (levels can configure width/height, see `05-LEVEL-FORMAT-AUTHORING.md`).
- Player swaps two orthogonally-adjacent gems. A swap is only legal if it creates at least one match
  of 3+ same-type gems in a row/column (standard Bejeweled legality rule) — illegal swaps snap back
  with a small "denied" shake/sound, no move consumed.
- Matches clear, gems above fall to fill gaps (gravity), new gems spawn from the top. Falling can
  chain into further matches ("cascades") — each cascade step increments a combo counter.
- Matches of 4 or 5+ create special gems (standard genre convention):
  - 4-in-a-row → **Bolt** gem: clears its entire row or column when matched/activated.
  - L/T-shaped 5 → **Prism** gem: clears a 3x3 area.
  - 5-in-a-row → **Nova** gem: clears all gems of one color on the board.
  - Combining two specials (swap them into each other) triggers a combined larger effect.

## Objectives (per level)

Levels are objective-driven, not just "clear until moves run out" — this is what makes level *design*
meaningful and gives the editor something to configure per-level:

- **Score Target**: reach N points within M moves.
- **Collection**: clear N gems of a specific color/type.
- **Descent**: clear "blocker" tiles embedded in the grid (e.g. ice, crates) by matching adjacent
  gems, blockers may require multiple hits.
- **Survival**: last M moves without the board "locking" (no legal moves) — a shuffle is auto-offered
  once per level as a safety valve, tracked as a soft penalty not a loss.
- **Timed** (optional, off by default — TUI input latency over SSH makes hard real-time timers a
  fairness risk; if used, timer should be *move-clock* style, not animation-frame-rate dependent).

Objective type is data (`enum Objective` in `bewildered-content`), not hardcoded per-level logic.

## The twist: Resonance Echoes

The one genuinely new mechanic. Rules:

1. When a match clears, every cell the match occupied gets an **Echo charge** (visually: a faint
   pulsing glyph/shimmer left in that empty cell as it's refilled — the *new* gem that falls into
   that cell carries the echo, not the cell itself).
2. An echo-carrying gem is visually marked (subtle glyph modifier + distinct particle trail on Tier
   2/3 rendering; a `·` corner-tick on Tier 1 ASCII).
3. If an echo-carrying gem is part of a match on the **very next player move**, the match
   "detonates": it clears one extra ring of adjacent gems beyond the normal match shape, and awards a
   **Resonance multiplier** (starts at x1.5, stacks additively per simultaneous echo detonation in
   the same move, capped at x4).
4. Echo charges expire (silently, no penalty) if not used within one move — this keeps the mechanic
   as a light "was that gem echoed? maybe swap here instead" read, not a system players must track
   indefinitely.

Why this works for a TUI specifically: it adds strategic depth **without adding UI surface area** —
no new input, no new panel, just a glyph/shimmer difference on gems that are already on screen. It
also gives the audio/particle system something structured to escalate around (see
`04-RENDERING-AUDIO.md` — echo detonations are the primary trigger for the biggest particle/sound
payoffs).

## Run structure: The Descent

Outside of standalone/campaign level play, Bewildered's headline mode is a roguelike run:

- A **Descent** is a sequence of **Chambers** (boards), each with one Objective, increasing in
  difficulty.
- After clearing a Chamber, the player picks **1 of 3 offered Relics** (passive modifiers). Relics
  are simple, composable rule tweaks — examples:
  - *Diagonal Sight*: diagonal 3-in-a-rows also count as matches.
  - *Fifth Hue*: a 5th gem color enters the pool (harder matches, more Bolt/Prism opportunities).
  - *Echo Chamber*: Echo charges last 2 moves instead of 1.
  - *Corner Cutter*: clearing a board corner cell refunds one move.
  - *Greedy Nova*: Nova gems clear one *fewer* color-match minimum but grant no move refund.
  - Relics stack for the rest of the run; a few are deliberately anti-synergistic with each other to
    create real deck-like build decisions (this is the "roguelike deckbuilder" DNA borrowed from
    *Match Morphosis* / *House and Hand* per the research notes in `00-OVERVIEW.md`).
- A Descent ends on completing its final Chamber (win) or failing a Chamber's fail-condition (out of
  moves on a Score/Descent-blocker objective; loss just ends the run, no punishing meta-loss).
- **Daily Descent**: the date is hashed to a `u64` seed → deterministic Chamber sequence + relic
  offers, identical for every player that day. Requires nothing beyond the existing seeded-RNG core
  (`rand_chacha::ChaCha8Rng::seed_from_u64`) — see `02-ARCHITECTURE.md`. Results (score, chambers
  cleared) are written to local high-score storage; no networking required for v1 (a shareable
  result string, à la Wordle, is a nice cheap Stage-6+ addition — not required for launch).

## Gem set & accessibility

- Default 4–6 gem types depending on relics active. Each gem type is defined by **both** a color
  *and* a distinct glyph/shape — never color alone — so the game is playable in a colorblind-safe way
  and in degraded/no-color terminals. Suggested glyph set (Tier 1 ASCII):
  `● (circle) ▲ (triangle) ■ (square) ◆ (diamond) ✦ (star) ✚ (cross)`.
- Color palette should be configurable via a small number of named palettes (default, deuteranopia,
  protanopia, tritanopia, high-contrast) stored in config — trivial since color is just a ratatui
  `Style`, not baked into glyph choice.

## Scoring

- Base points per gem cleared, scaled by match size (3/4/5+), multiplied by cascade depth (each
  cascade step in a chain multiplies subsequent clears), further multiplied by any active Resonance
  multiplier. Exact tuning constants live in `bewildered-core::scoring` as named constants, not
  magic numbers, so they're easy for the agent (or a human) to rebalance later without hunting
  through code.

Proceed to `04-RENDERING-AUDIO.md`.
