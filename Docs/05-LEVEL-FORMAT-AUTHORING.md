# Bewildered — Level Format & Authoring Tool

## Level schema (RON)

One level = one RON file, deserialized into `bewildered_content::Level`. Sketch (finalize exact
field names during Stage 2 implementation, but keep this shape):

```ron
Level(
    id: "chamber-003",
    name: "The Glass Stair",
    grid: (width: 8, height: 8),
    gem_types: [Circle, Triangle, Square, Diamond],   // which of the shared gem set is in play
    blockers: [
        (pos: (2, 3), kind: Ice(hits: 2)),
        (pos: (5, 5), kind: Crate(hits: 1)),
    ],
    objective: ScoreTarget(points: 12000, max_moves: 20),
    relic_pool_tags: ["descent-early"],   // which relic pool this chamber draws offers from, if part of a Descent
    seed_override: None,                  // Some(u64) pins gem-fill RNG for a hand-tuned puzzle; None = random per playthrough
)
```

```ron
// Campaign pack manifest, one per .bwpack
Pack(
    id: "starter-campaign",
    title: "Bewildered: First Descent",
    author: "...",
    levels: ["chamber-001", "chamber-002", "chamber-003", ...],  // ordered
    relic_pools: { "descent-early": [...], "descent-late": [...] },
)
```

- `Objective` is the `enum` from `03-GAME-DESIGN.md` (`ScoreTarget`, `Collection`, `Descent`,
  `Survival`), `#[serde]`-tagged so RON stays readable.
- A `.bwpack` file is a zip containing `manifest.ron` + one `.ron` per level + optional pack-specific
  assets (custom gem palette, music override) — see `bewildered-content::pack` for the loader.
  Regular directories of loose RON files (no zip) are also loadable directly for in-development
  packs — the editor works against a loose directory and exports to `.bwpack` only on publish.

## Solvability validation (`bewildered-solver`)

Every level must pass validation before shipping. The solver is a headless CLI built on
`bewildered-core` (no rendering, no audio):

```
bewildered-solver check levels/chamber-003.ron
bewildered-solver check-pack levels/starter-campaign/
```

Validation strategy (tiered, cheapest first):
1. **Static sanity**: grid dimensions valid, objective reachable in principle (e.g. `Collection`
   target ≤ total gems of that type that can plausibly appear), no blocker configuration that
   isolates cells illegally.
2. **Reachability search**: for the level's *starting* board (seeded if `seed_override` is set,
   otherwise sampled N times with different seeds), run a bounded search (greedy + limited-depth
   lookahead, not full minimax — boards are too large for exhaustive search) attempting to hit the
   objective within `max_moves`. Report pass/fail and, on failure, the best score/progress reached,
   so a level author gets actionable feedback ("reached 8400/12000 points in 20 moves across 50
   sampled seeds") rather than a bare fail.
3. **No-softlock check**: confirm the game's built-in shuffle-when-locked safety net (see
   `03-GAME-DESIGN.md §Survival`) is never needed more than once in a typical playout — frequent
   forced reshuffles indicate a badly tuned gem-type count for the grid size.

This solver is also invoked from CI (`cargo test` harness shells out to it, or better, is called as
a library function directly from a `#[test]`) and from inside the editor as a live "Validate" action.

## The editor (`bewildered-editor`)

A second `ratatui` application, sharing `bewildered-render` with the game so the preview pane is the
*actual* game renderer, not a mockup. Layout:

- **Grid pane**: paint gem types / blockers onto the board with the keyboard (cursor + hotkeys per
  gem type/blocker, vim-style `hjkl` movement fits the Omarchy/Neovim-adjacent audience well).
- **Objective panel**: form-style editor for the level's `Objective` variant and its parameters.
- **Live preview pane**: renders the level through `bewildered-render` exactly as the game would,
  including a "simulate" mode that lets the author actually play the level in place to feel it out.
- **Validate action**: runs `bewildered-solver` in-process (as a library call, not a subprocess, for
  speed) and surfaces pass/fail + sampled-seed results inline.
- **Pack panel**: reorder levels, assign relic pools, set pack metadata, export to `.bwpack`.
- Undo/redo on all grid edits (simple command-stack, not full ECS-style change tracking — keep this
  lightweight).

### Level generation assist (nice-to-have, Stage 6+)
An optional `bewildered-editor --generate` mode that procedurally fills a grid (respecting
blocker/objective constraints) and immediately runs it through the solver, repeating with a new seed
until a passing candidate is found — gives authors a starting point to hand-tune rather than a blank
grid. This is a straightforward loop (`generate → solve → keep or retry`) built entirely from
pieces that already exist (`bewildered-core`'s board fill + `bewildered-solver`), so it's cheap once
both exist, but it is not required for a shippable v1.

Proceed to `06-BUILD-STAGES-CHECKLIST.md`.
