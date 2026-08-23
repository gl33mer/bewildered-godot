# Bewildered — Omarchy Quattro Environment Setup

Omarchy Quattro (v4, shipped Aug 2026) is DHH's Arch-based, Hyprland/Quickshell desktop layer. Base
is straight Arch Linux with `pacman`, full AUR access, and Omarchy's own package channel
(stable/rc/edge/dev) mirrored on Cloudflare, roughly a month behind upstream Arch by default (a
safety lag, security fixes jump the queue). Practically, everything this project needs is a normal
`pacman`/`mise`/`cargo` install — there is no Omarchy-specific packaging obstacle.

## Toolchain — **`mise` is the path, not a manual `rustup` install**

Omarchy manages dev-language runtimes (Rust, Go, Ruby, Node, Python, etc.) through **`mise`**
(mise-en-place), installable via the Omarchy Menu's Install > Development section or directly on the
CLI. This matters because a toolchain can be present in the OS image without being on `PATH` in a
fresh shell until it's activated through mise — don't assume `rustc`/`cargo` missing means Rust isn't
installed; check mise first.

- [ ] Check what mise already sees: `mise ls rust`. If a version is listed, it's managed — no
      install needed, just activation (next step). If mise itself isn't on `PATH`, install it per
      Omarchy's own docs (Omarchy Menu → Install > Development, or `curl https://mise.run | sh` per
      upstream mise install instructions) rather than reaching for a distro package.
- [ ] Activate/pin a Rust toolchain for this project: `mise use -g rust@latest` for a global
      default, or (**preferred for this repo**) drop a `mise.toml` at the workspace root pinning an
      exact version so `cd`-ing into the project always gets the same toolchain regardless of what's
      set globally:
      ```toml
      [tools]
      rust = "latest"
      ```
      Note mise's Rust support works *through* rustup under the hood (it installs rustup if missing,
      then sets `RUSTUP_TOOLCHAIN` per mise's resolved version) — so `rustup`/`cargo`/`rustc` all
      still behave normally once mise has activated a version; there's no parallel/conflicting
      toolchain mechanism to worry about, just an activation step mise handles for you.
- [ ] If `cargo`/`rustc` are still missing or broken after the above, run `mise doctor` and check for
      a stale/uninstalled version before troubleshooting anything else — this resolves the vast
      majority of "Rust should be there but isn't on PATH" confusion on Omarchy.
- [ ] Build essentials: `pacman -S --needed base-devel` (should already be present on any Omarchy
      install, but confirm — `cc`/`pkg-config` are needed by several crates' build scripts, notably
      audio backends).
- [ ] `git` (present by default on Omarchy).

## Audio (`kira` → `cpal` backend)

- [ ] Omarchy runs PipeWire by default (modern Arch/Hyprland stack standard). `cpal`'s ALSA backend
      talks to PipeWire's ALSA-compat shim out of the box on Arch — no special configuration needed.
      Confirm with: `pactl info` or `wpctl status` to verify PipeWire is running.
- [ ] Dev headers: `pacman -S --needed alsa-lib` (provides the headers `cpal`'s build needs even
      though runtime routing goes through PipeWire).
- [ ] If building/running headless (CI, no PipeWire session — e.g. a bare SSH box or container),
      `bewildered-audio` must degrade to its silent no-op backend per
      `04-RENDERING-AUDIO.md §Muting/degraded environments` rather than failing the build/run. Verify
      this explicitly as part of Stage 5 in `06-BUILD-STAGES-CHECKLIST.md`.

## Terminal for development & Tier verification

- [ ] Omarchy's default terminal historically has been Alacritty; Quattro's Quickshell rewrite may
      have changed defaults — check `~/.config/hypr/` / the Omarchy manual (`omarchy.org/manual`) for
      whatever the current default is. Alacritty does **not** support Sixel/Kitty graphics protocols,
      so it will exercise **Tier 2** (half-block/braille) rendering, not Tier 3.
- [ ] For Tier 3 (Kitty Graphics Protocol) verification specifically, install **Kitty** or
      **Ghostty** from the AUR/pacman (`pacman -S kitty` or check AUR for `ghostty`) alongside
      whatever the default terminal is — don't replace the default, just have one Tier-3-capable
      terminal available to test against, per Stage 7's manual verification pass.
- [ ] `ratatui-image`'s protocol auto-detection should be trusted first; only fall back to the
      `--render-tier` override flag if a terminal misreports its capability (some terminals answer
      capability queries incorrectly — this is a known rough edge across the terminal-graphics
      ecosystem, not specific to this project).

## Package distribution target

- [ ] Once Stage 9 is reached, a `PKGBUILD` targeting the AUR is the natural distribution path for
      an Omarchy-native install (`pacman`/`yay`/`paru` all resolve AUR packages the same way regular
      users on this OS already work). No custom Omarchy package-channel submission is needed or
      expected for a project at this stage — AUR is sufficient and is what an Omarchy user would
      reach for.

## Suggested additional Pi agent tools

The agent already has `pi-web-access` enabled, which covers the research this spec set already did
(match-3 design research, crate documentation lookups, Omarchy specifics) plus anything that comes
up mid-implementation (checking exact `ratatui-image`/`kira` API signatures against their current
docs, which move faster than any static spec can track). Beyond that, if available in the Pi tool
catalog, these would materially help this specific project:

- **A terminal/PTY execution tool with screenshot/capture capability** (e.g. something that can spawn
  the built binary inside a real PTY and capture the rendered frame as text or an image) — this is
  the single highest-value addition, since a TUI game's correctness is fundamentally about what
  actually renders in a terminal, not just what compiles. Without it, the agent is verifying layout
  logic blind. If Pi has any screen/PTY-capture tool, enable it for this project.
- **`cargo` test/bench runner tool** (if Pi distinguishes this from generic shell exec) — Stage 1's
  `proptest` suite and Stage 4's `criterion` benchmark are load-bearing for this spec's correctness
  and performance claims and should be run frequently, not just at the end of a stage.
- **Audio playback/inspection is not needed as an agent tool** — verifying `kira` wiring is
  compiles-and-triggers-correctly territory (unit/integration tests on the event→SFX-ID mapping), not
  something the agent needs to *hear*; don't over-invest here.

Nothing else in this project needs bespoke tooling beyond a normal Rust dev loop (`cargo build`,
`cargo test`, `cargo clippy`, `cargo run --bin bewildered-tui`) plus web access for API/docs lookups.

---

This completes the spec set. Start at Stage 0 in `06-BUILD-STAGES-CHECKLIST.md`.
