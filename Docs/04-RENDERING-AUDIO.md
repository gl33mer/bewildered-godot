# Bewildered — Rendering & Audio

## Three-tier rendering strategy

`ratatui-image` detects terminal capability at startup (env-var guess, then a control-sequence
query, with a manual override in config for terminals that misreport). Bewildered defines three
rendering tiers on top of that detection — same game state, three fidelity levels, chosen
automatically and overridable with `--render-tier`:

### Tier 1 — Universal ASCII (any terminal, 80x24, SSH-safe)
- Gems: single glyph + fg color in a bordered cell (`ratatui::widgets::Block` per cell or a custom
  buffer-writing widget for performance — prefer the latter once profiling shows `Block`-per-cell is
  too slow at 8x8+).
- Particles: reuse of a small set of ASCII spark glyphs (`*`, `+`, `.`, `'`) with color-only motion —
  a particle is one cell, moves by re-rendering at a new cell position each frame tick, faded via
  color intensity over its lifetime.
- This tier must look *intentional*, not degraded — treat it as the primary target, not a fallback
  players will resent. Reference points: the crispness of `Bastion`/`Caves of Qud`-style TUI
  aesthetics, not a spreadsheet.

### Tier 2 — Unicode half-block / Braille (any true-color terminal, no graphics protocol needed)
- Gems: rendered into a 2x2 (half-block, `▀▄▌▐█`) or 2x4 (braille dot patterns) sub-cell grid per
  board cell for smoother shapes and gradient shading — this is what `ratatui-image`'s "halfblocks"
  fallback mode already does for images; Bewildered's own gem glyphs get the same subpixel treatment
  by hand (small pre-baked bitmap-to-halfblock lookup per gem type, computed once at startup, not
  per frame).
- Particles: this is where the "particle trail" effect actually reads as motion — braille's 2x4
  dot resolution per cell gives enough sub-cell precision for a spark to visibly arc between board
  cells rather than jumping cell-to-cell. Store particle position as `f32` world coordinates,
  quantize to the nearest braille dot only at draw time.
- Screen-shake and color-flash effects (brief full-viewport tint on big cascades) are cheap and
  effective at this tier.

### Tier 3 — Pixel graphics (Kitty Graphics Protocol / Sixel / iTerm2, via `ratatui-image`)
- Gems and particle bursts rendered as small pre-authored sprite/spritesheet PNGs (`assets/glyphs/`),
  composited and blitted through `ratatui-image`'s picker. This is the "forget it's a terminal" tier
  for players on Kitty/Ghostty/WezTerm.
- Because `bewildered-render` abstracts tier selection behind one drawing API, gameplay code never
  branches on tier — only the widget implementations differ.
- Sprite budget: keep sprite sheets small (gem set + a handful of particle frames); this is
  decoration, the game must remain fully playable and legible with sprites disabled.

**Rule for all tiers**: input latency and board legibility never regress for aesthetics. If a tier's
richer effects would blur which cell is which, clip/simplify rather than let form beat function.

## Particle system

A single, tier-agnostic particle pool lives in `bewildered-render`:

```rust
struct Particle {
    pos: (f32, f32),      // world coords, board-cell units (fractional)
    vel: (f32, f32),
    life: f32,             // seconds remaining
    max_life: f32,
    kind: ParticleKind,    // Spark, Shard, Confetti, EchoPulse, ...
    color: Color,
}
```

- Fixed-capacity pool (`Vec<Particle>` pre-sized, e.g. 512), oldest-recycled on overflow — guarantees
  the zero-per-frame-allocation target from `02-ARCHITECTURE.md`.
- Emitters are just functions that push N particles with a shape (burst, fountain, trail-along-path)
  given a triggering `MoveOutcome` — a plain match emits a small burst per cleared gem; a Bolt gem
  emits a directional streak along its row/column; a Resonance Echo detonation emits a larger,
  denser, higher-saturation burst that scales with the multiplier (this is the primary "big payoff"
  visual and should be the most impressive effect in the game).
- Update step is a trivial `pos += vel * dt; life -= dt`, integrated once per frame regardless of
  render tier; only the *drawing* of a particle differs per tier.

## Audio design (`kira`)

- **SFX bank**: short one-shots for swap, illegal-swap-denied, match (small/medium/large), special
  gem creation, special gem activation, cascade step, Resonance Echo detonation, Chamber clear,
  Relic pick, run loss. Each combo-relevant SFX (match, cascade step) is pitched up slightly per
  consecutive cascade step (`kira`'s playback rate parameter) — mirrors the classic "rising pitch on
  combo" dopamine trick from Candy Crush/Puyo Puyo.
- **Adaptive music**: 3–4 layered stems per track (e.g. percussion, bass, pad, lead) mixed via
  `kira`'s tween API. A single `intensity: f32` (0.0–1.0) driven by recent combo activity fades stems
  in/out smoothly — high intensity during a big cascade, settling back to a sparse loop during quiet
  board-scanning moments. This is the non-literal-rhythm-game answer to *Lumines Arise*'s beat-synced
  presentation: the music *reacts to* play, it doesn't *gate* play.
- **Muting/degraded environments**: audio must be optional and default-detect a headless/no-audio-
  device environment (common over SSH) without erroring — `bewildered-audio` should treat "no output
  device" as a silent no-op backend, not a crash.

## Accessibility notes (carries over from `03-GAME-DESIGN.md`)

- Screen-shake, flash effects, and audio are all independently toggle-able in config.
- Particle density has a `--low-fx` / config equivalent for low-end terminals or players sensitive to
  visual noise.

Proceed to `05-LEVEL-FORMAT-AUTHORING.md`.
