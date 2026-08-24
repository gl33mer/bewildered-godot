//! Bewildered core — board simulation, match detection, cascade resolution,
//! special gem creation, resonance echoes, and scoring.

use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

pub mod gem_types;
pub use gem_types::GemKind;

/// The direction gravity pulls gems toward. Down = toward the bottom row,
/// Right = toward the right column, Up = toward the top row, Left = toward the
/// left column. Powers the 90° Gravity Tumbler mechanic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Down,
    Right,
    Up,
    Left,
}

impl Direction {
    /// Rotate the gravity direction 90° clockwise (matching a clockwise
    /// physical board tumbling).
    pub fn rotate_cw(self) -> Direction {
        match self {
            Direction::Down => Direction::Left,
            Direction::Left => Direction::Up,
            Direction::Up => Direction::Right,
            Direction::Right => Direction::Down,
        }
    }

    /// Rotate the gravity direction 90° counter-clockwise.
    pub fn rotate_ccw(self) -> Direction {
        match self {
            Direction::Down => Direction::Right,
            Direction::Right => Direction::Up,
            Direction::Up => Direction::Left,
            Direction::Left => Direction::Down,
        }
    }
}

/// Rule modifiers from active relics — threaded through core systems.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleModifiers {
    /// Diagonal matches also count (Diagonal Sight relic)
    pub diagonal_matches: bool,
    /// Add a 5th gem color to the pool (Fifth Hue relic)
    pub fifth_hue: bool,
    /// Echo charges last N moves instead of 1 (Echo Chamber relic)
    pub echo_extra_moves: u8,
    /// Clearing a corner cell refunds 1 move (Corner Cutter relic)
    pub corner_cutter: bool,
    /// Nova requires one fewer color match but grants no move refund (Greedy Nova relic)
    pub greedy_nova: bool,
    /// Score multiplier bonus (flat percentage)
    pub score_bonus_pct: f32,
    /// Extra moves granted at level start (Extra Moves relic)
    pub extra_moves: u8,
    /// Collection target reduction percentage (Collection Bonus relic)
    pub collection_reduction_pct: u8,
}

impl RuleModifiers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge another RuleModifiers into this one (for stacking relics)
    pub fn merge(&mut self, other: &RuleModifiers) {
        self.diagonal_matches |= other.diagonal_matches;
        self.fifth_hue |= other.fifth_hue;
        self.echo_extra_moves = self.echo_extra_moves.max(other.echo_extra_moves);
        self.corner_cutter |= other.corner_cutter;
        self.greedy_nova |= other.greedy_nova;
        self.score_bonus_pct += other.score_bonus_pct;
        self.extra_moves += other.extra_moves;
        self.collection_reduction_pct = self
            .collection_reduction_pct
            .max(other.collection_reduction_pct);
    }

    /// Get the echo charge duration (1 + extra_moves)
    pub fn echo_duration(&self) -> u8 {
        1 + self.echo_extra_moves
    }

    /// Check if diagonal matches are enabled
    pub fn diagonal_matches_enabled(&self) -> bool {
        self.diagonal_matches
    }

    /// Get the number of gem types (4 base + 1 if fifth_hue)
    pub fn gem_type_count(&self, base_count: usize) -> usize {
        if self.fifth_hue {
            base_count + 1
        } else {
            base_count
        }
    }
}

/// A gem on the board. Tracks its kind, whether it carries an echo charge,
/// and any special properties (e.g. Bolt, Prism, Nova).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gem {
    pub kind: GemKind,
    pub echo: Option<EchoCharge>,
    pub special: Option<SpecialGem>,
}

/// Special gem types created by matches of 4+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecialGem {
    /// 4 in a row — clears entire row or column when activated
    Bolt { horizontal: bool },
    /// L/T shaped 5 — clears 3x3 area
    Prism,
    /// 5 in a row — clears all gems of one color
    Nova,
}

impl SpecialGem {
    /// Get the cells affected by activating this special gem at the given position.
    pub fn activation_cells(
        &self,
        board: &Board,
        center_row: usize,
        center_col: usize,
    ) -> Vec<(usize, usize)> {
        match self {
            SpecialGem::Bolt { horizontal } => {
                if *horizontal {
                    // Clear entire row
                    (0..board.width).map(|c| (center_row, c)).collect()
                } else {
                    // Clear entire column
                    (0..board.height).map(|r| (r, center_col)).collect()
                }
            }
            SpecialGem::Prism => {
                // Clear 3x3 area centered on the gem
                let mut cells = Vec::new();
                for dr in -1i32..=1 {
                    for dc in -1i32..=1 {
                        let r = center_row as i32 + dr;
                        let c = center_col as i32 + dc;
                        if r >= 0 && r < board.height as i32 && c >= 0 && c < board.width as i32 {
                            cells.push((r as usize, c as usize));
                        }
                    }
                }
                cells
            }
            SpecialGem::Nova => {
                // Nova clears all gems of the same kind — handled specially in activation
                Vec::new()
            }
        }
    }
}

/// Echo charge attached to a cell after a match clears.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EchoCharge {
    /// How many moves the echo can survive before expiring.
    pub moves_left: u8,
}

impl EchoCharge {
    pub fn new() -> Self {
        Self { moves_left: 1 }
    }

    pub fn with_duration(moves: u8) -> Self {
        Self { moves_left: moves }
    }
}

impl Default for EchoCharge {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of applying a swap attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveOutcome {
    /// The swap was legal and produced matches.
    Success {
        matches: Vec<Match>,
        cascades: usize,
        resonance_multiplier: f32,
        /// Per-cascade-depth cleared cell lists (each entry carries the cell's
        /// kind at clear-time). Lets the Godot layer present each cascade step
        /// with readable pacing instead of clearing everything in one frame.
        clears_by_depth: Vec<Vec<(usize, usize, GemKind)>>,
    },
    /// The swap was illegal (no match would be created) — reverts the swap.
    Illegal,
    /// The swap was legal but caused no matches (no cascade).
    NoMatch,
}

/// A match found on the board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Match {
    pub cells: Vec<(usize, usize)>,
    pub kind: GemKind,
    pub is_special: bool,
    pub special_type: Option<SpecialGem>,
}

/// Core game state — the mutable board and auxiliary data.
#[derive(Debug, Clone)]
pub struct Board {
    /// Flat vector of gems; None = empty cell.
    pub gems: Vec<Option<Gem>>,
    /// Board width.
    pub width: usize,
    /// Board height.
    pub height: usize,
    /// Current combo count (for scoring).
    pub combo: usize,
    /// Active gem types for spawning.
    pub gem_types: Vec<GemKind>,
    /// RNG for deterministic spawning.
    rng: StdRng,
    /// Current resonance multiplier from echo detonations (1.0 = no resonance).
    pub resonance_multiplier: f32,
    /// Count of echoes that detonated in the current move (for stacking).
    pub resonance_stack: usize,
    /// Rule modifiers from active relics.
    pub rule_modifiers: RuleModifiers,
    /// Gems (by position and kind) cleared during the most recent successful
    /// move — captured at clear-time (the board refills, so kinds are otherwise
    /// lost). Used for authoritative objective tracking.
    pub cleared_this_move: Vec<(usize, usize, GemKind)>,
    /// Current gravity pull direction (defaults to Down).
    pub gravity: Direction,
}

impl Board {
    /// Create a new board with the given dimensions and seed.
    pub fn new(width: usize, height: usize, seed: u64, gem_types: Vec<GemKind>) -> Self {
        Self::with_rules(width, height, seed, gem_types, RuleModifiers::new())
    }

    /// Create a new board with custom rule modifiers.
    pub fn with_rules(
        width: usize,
        height: usize,
        seed: u64,
        gem_types: Vec<GemKind>,
        rule_modifiers: RuleModifiers,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let gt: Vec<GemKind> = gem_types.clone();
        let mut gems: Vec<Option<Gem>> = Vec::with_capacity(width * height);

        // Fill with random gems, guaranteeing the board starts match-free. Each
        // gem is rejected if it would complete a 3-in-a-row with the two gems
        // already placed directly to its left or directly above it. This keeps
        // the opening board free of pre-existing matches (and thus prevents an
        // unrelated swap from being accepted merely because a pre-existing match
        // exists elsewhere on the board).
        for i in 0..(width * height) {
            let r = i / width;
            let c = i % width;
            let mut kind = gt[rng.gen_range(0..gt.len())];
            for _ in 0..20 {
                let candidate = gt[rng.gen_range(0..gt.len())];
                let horiz = c >= 2
                    && gems[i - 1].as_ref().map(|g| g.kind) == Some(candidate)
                    && gems[i - 2].as_ref().map(|g| g.kind) == Some(candidate);
                let vert = r >= 2
                    && gems[i - width].as_ref().map(|g| g.kind) == Some(candidate)
                    && gems[i - 2 * width].as_ref().map(|g| g.kind) == Some(candidate);
                if !horiz && !vert {
                    kind = candidate;
                    break;
                }
            }
            gems.push(Some(Gem {
                kind,
                echo: None,
                special: None,
            }));
        }

        Self {
            gems,
            width,
            height,
            combo: 0,
            gem_types,
            rng,
            resonance_multiplier: 1.0,
            resonance_stack: 0,
            rule_modifiers,
            cleared_this_move: Vec::new(),
            gravity: Direction::Down,
        }
    }

    /// Get the index for (row, col).
    #[inline]
    fn idx(&self, row: usize, col: usize) -> usize {
        row * self.width + col
    }

    /// Get the gem at (row, col) — row-major indexing.
    pub fn gem(&self, row: usize, col: usize) -> Option<&Gem> {
        if row >= self.height || col >= self.width {
            return None;
        }
        self.gems[self.idx(row, col)].as_ref()
    }

    /// Get mutable reference to gem at (row, col).
    pub fn gem_mut(&mut self, row: usize, col: usize) -> Option<&mut Gem> {
        if row >= self.height || col >= self.width {
            return None;
        }
        let idx = self.idx(row, col);
        self.gems[idx].as_mut()
    }

    /// Set a gem at (row, col) to a specific kind.
    pub fn set_gem(&mut self, row: usize, col: usize, kind: GemKind) {
        if row < self.height && col < self.width {
            let idx = self.idx(row, col);
            self.gems[idx] = Some(Gem { kind, echo: None, special: None });
        }
    }

    /// Remove a gem at (row, col).
    pub fn remove_gem(&mut self, row: usize, col: usize) {
        if row < self.height && col < self.width {
            let idx = self.idx(row, col);
            self.gems[idx] = None;
        }
    }

    /// Get the kind of the gem at a flat index (borrow-free helper).
    fn gem_kind_at(gems: &[Option<Gem>], idx: usize) -> Option<GemKind> {
        gems.get(idx).and_then(|g| g.as_ref()).map(|g| g.kind)
    }

    /// Try to swap two adjacent gems. Returns the move outcome.
    pub fn try_swap(&mut self, row1: usize, col1: usize, row2: usize, col2: usize) -> MoveOutcome {
        // Check bounds
        if row1 >= self.height || col1 >= self.width || row2 >= self.height || col2 >= self.width {
            return MoveOutcome::Illegal;
        }

        // Check adjacency (orthogonal only)
        let dr = row1.abs_diff(row2);
        let dc = col1.abs_diff(col2);
        if dr + dc != 1 {
            return MoveOutcome::Illegal;
        }

        let idx1 = self.idx(row1, col1);
        let idx2 = self.idx(row2, col2);

        // Check if both cells have gems
        if self.gems[idx1].is_none() || self.gems[idx2].is_none() {
            return MoveOutcome::Illegal;
        }

        // Simulate the swap
        self.gems.swap(idx1, idx2);

        // Check for matches
        let initial_matches = self.find_all_matches();

        // A swap is only legal if at least one resulting match involves one of
        // the two swapped cells (or a special gem was created by the swap). This
        // stops an unrelated swap from being accepted merely because a
        // pre-existing match exists somewhere else on the board.
        let swap_caused = initial_matches.iter().any(|m| {
            m.cells.contains(&(row1, col1))
                || m.cells.contains(&(row2, col2))
                || (m.is_special && m.special_type.is_some())
        });

        if initial_matches.is_empty() || !swap_caused {
            // No match was actually produced by THIS swap — revert it.
            self.gems.swap(idx1, idx2);
            return if initial_matches.is_empty() {
                MoveOutcome::Illegal
            } else {
                MoveOutcome::NoMatch
            };
        }

        // Reset resonance for this move
        self.resonance_multiplier = 1.0;
        self.resonance_stack = 0;
        self.cleared_this_move.clear();

        // Process matches and cascades with echo detonation
        let (total_cascades, clears_by_depth) =
            self.process_matches(initial_matches.clone());

        let resonance_mult = self.resonance_multiplier;

        MoveOutcome::Success {
            matches: initial_matches,
            cascades: total_cascades,
            resonance_multiplier: resonance_mult,
            clears_by_depth,
        }
    }

    /// Find all matches on the board (horizontal and vertical, 3+).
    /// Also detects L/T-shaped matches for Prism creation.
    pub fn find_all_matches(&self) -> Vec<Match> {
        // First pass: find all raw matches (horizontal and vertical)
        let mut raw_matches = Vec::new();

        // Horizontal matches
        for row in 0..self.height {
            let mut col = 0;
            while col < self.width {
                let idx = self.idx(row, col);
                if let Some(gem) = &self.gems[idx] {
                    let kind = gem.kind;
                    let mut run_len = 1;
                    while col + run_len < self.width {
                        let next_idx = self.idx(row, col + run_len);
                        if let Some(next_gem) = &self.gems[next_idx] {
                            if next_gem.kind == kind {
                                run_len += 1;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }

                    if run_len >= 3 {
                        let cells: Vec<(usize, usize)> =
                            (0..run_len).map(|i| (row, col + i)).collect();
                        raw_matches.push((cells, kind, true)); // true = horizontal
                    }
                    col += run_len;
                } else {
                    col += 1;
                }
            }
        }

        // Vertical matches
        for col in 0..self.width {
            let mut row = 0;
            while row < self.height {
                let idx = self.idx(row, col);
                if let Some(gem) = &self.gems[idx] {
                    let kind = gem.kind;
                    let mut run_len = 1;
                    while row + run_len < self.height {
                        let next_idx = self.idx(row + run_len, col);
                        if let Some(next_gem) = &self.gems[next_idx] {
                            if next_gem.kind == kind {
                                run_len += 1;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }

                    if run_len >= 3 {
                        let cells: Vec<(usize, usize)> =
                            (0..run_len).map(|i| (row + i, col)).collect();
                        raw_matches.push((cells, kind, false)); // false = vertical
                    }
                    row += run_len;
                } else {
                    row += 1;
                }
            }
        }

        // Second pass: classify matches and detect special gems
        let mut matches = Vec::new();

        // First, find Prism candidates (intersecting matches)
        let mut prism_cells = std::collections::HashSet::new();

        for i in 0..raw_matches.len() {
            for j in i + 1..raw_matches.len() {
                let (cells1, kind1, horiz1) = &raw_matches[i];
                let (cells2, kind2, horiz2) = &raw_matches[j];

                if kind1 != kind2 {
                    continue;
                }
                if *horiz1 == *horiz2 {
                    continue;
                } // Must be perpendicular

                // Find intersection
                let set1: std::collections::HashSet<_> = cells1.iter().collect();
                let intersection: Vec<_> = cells2.iter().filter(|c| set1.contains(c)).collect();

                if intersection.len() == 1 {
                    // They intersect at exactly one cell
                    let total_unique = cells1.len() + cells2.len() - 1;
                    if total_unique == 5 || total_unique == 7 {
                        // L-shape (5) or T-shape (7) -> Prism
                        let intersect_cell = *intersection[0];
                        let mut combined = cells1.clone();
                        combined.extend(cells2.iter().filter(|c| **c != intersect_cell));

                        // Mark all cells as used for Prism
                        for c in &combined {
                            prism_cells.insert(*c);
                        }

                        matches.push(Match {
                            cells: combined,
                            kind: *kind1,
                            is_special: true,
                            special_type: Some(SpecialGem::Prism),
                        });
                    }
                }
            }
        }

        // Now process non-Prism matches
        for (cells, kind, horizontal) in raw_matches {
            // Skip if all cells are part of a Prism
            if cells.iter().all(|c| prism_cells.contains(c)) {
                continue;
            }

            let run_len = cells.len();

            // Check if this is a straight 5+ (Nova) or 4 (Bolt)
            let (is_special, special_type) = if run_len >= 5 {
                (true, Some(SpecialGem::Nova))
            } else if run_len == 4 {
                (true, Some(SpecialGem::Bolt { horizontal }))
            } else {
                (false, None)
            };

            // Filter out cells already in Prism
            let filtered_cells: Vec<_> = cells
                .into_iter()
                .filter(|c| !prism_cells.contains(c))
                .collect();

            if filtered_cells.len() >= 3 {
                matches.push(Match {
                    cells: filtered_cells,
                    kind,
                    is_special,
                    special_type,
                });
            }
        }

        matches
    }

    /// Process matches: clear gems, apply gravity, refill, repeat until no matches.
    /// Returns (total cascade count, per-depth cleared cell list-with-kind).
    fn process_matches(
        &mut self,
        initial_matches: Vec<Match>,
    ) -> (usize, Vec<Vec<(usize, usize, GemKind)>>) {
        let mut total_cascades = 0;
        let mut current_matches = initial_matches;
        let mut clears_by_depth: Vec<Vec<(usize, usize, GemKind)>> = Vec::new();

        loop {
            if current_matches.is_empty() {
                break;
            }

            total_cascades += 1;
            self.combo += 1;

            // First, check for echo detonations and special gem activations
            let mut extra_clear_cells = Vec::new();
            let mut nova_color: Option<GemKind> = None;

            for m in &current_matches {
                // Check for echo detonation: any gem in match has an echo charge
                let has_echo = m
                    .cells
                    .iter()
                    .any(|(r, c)| self.gem(*r, *c).and_then(|g| g.echo.as_ref()).is_some());

                if has_echo {
                    self.resonance_stack += 1;
                    // Update resonance multiplier: base 1.5 + 0.5 per additional stack, capped at 4.0
                    self.resonance_multiplier = (scoring::RESONANCE_BASE_MULTIPLIER
                        + (self.resonance_stack.saturating_sub(1) as f32
                            * scoring::RESONANCE_STACK_INCREMENT))
                        .min(scoring::RESONANCE_MAX_MULTIPLIER);

                    // Echo detonation: clear extra ring around match
                    for &(r, c) in &m.cells {
                        for dr in -1i32..=1 {
                            for dc in -1i32..=1 {
                                let nr = r as i32 + dr;
                                let nc = c as i32 + dc;
                                if nr >= 0
                                    && nr < self.height as i32
                                    && nc >= 0
                                    && nc < self.width as i32
                                {
                                    extra_clear_cells.push((nr as usize, nc as usize));
                                }
                            }
                        }
                    }
                }

                // Handle special gem activation
                if let Some(special) = m.special_type {
                    match special {
                        SpecialGem::Bolt { .. } => {
                            // Find the center-ish cell of the match
                            let (cr, cc) = m.cells[m.cells.len() / 2];
                            for cell in special.activation_cells(self, cr, cc) {
                                extra_clear_cells.push(cell);
                            }
                        }
                        SpecialGem::Prism => {
                            let (cr, cc) = m.cells[m.cells.len() / 2];
                            for cell in special.activation_cells(self, cr, cc) {
                                extra_clear_cells.push(cell);
                            }
                        }
                        SpecialGem::Nova => {
                            // Nova clears all gems of the matched kind
                            nova_color = Some(m.kind);
                        }
                    }
                }
            }

            // Clear matched cells and create echoes (for non-detonated cells)
            let mut round_clears: Vec<(usize, usize, GemKind)> = Vec::new();
            let echo_duration = self.rule_modifiers.echo_duration();
            for m in &current_matches {
                for &(r, c) in &m.cells {
                    let cleared_kind = self.gem(r, c).map(|g| g.kind);
                    if let Some(kind) = cleared_kind {
                        if let Some(gem) = self.gem_mut(r, c) {
                            // Create echo charge on the cell (will be carried by new gem after gravity)
                            gem.echo = Some(EchoCharge::with_duration(echo_duration));
                        }
                        self.remove_gem(r, c);
                        self.cleared_this_move.push((r, c, kind));
                        round_clears.push((r, c, kind));
                    }
                }
            }

            // Clear extra cells from detonations/specials
            for (r, c) in extra_clear_cells {
                if let Some(kind) = self.gem(r, c).map(|g| g.kind) {
                    self.cleared_this_move.push((r, c, kind));
                    round_clears.push((r, c, kind));
                    self.remove_gem(r, c);
                }
            }

            // Clear Nova color if present
            if let Some(color) = nova_color {
                for i in 0..self.gems.len() {
                    let is_nova_kind = Self::gem_kind_at(&self.gems, i) == Some(color);
                    if is_nova_kind {
                        let (nr, nc) = (i / self.width, i % self.width);
                        self.cleared_this_move.push((nr, nc, color));
                        round_clears.push((nr, nc, color));
                        self.gems[i] = None;
                    }
                }
            }

            clears_by_depth.push(round_clears);

            // Persist special gems created by this cascade step: place the
            // special gem at the center of its matching run so it persists on
            // the board as a real gem (survives gravity/refill, is reported via
            // get_cell.special and rendered by Godot as an emoji overlay).
            for m in &current_matches {
                if let Some(special) = m.special_type {
                    let (cr, cc) = m.cells[m.cells.len() / 2];
                    let idx = self.idx(cr, cc);
                    self.gems[idx] = Some(Gem {
                        kind: m.kind,
                        echo: None,
                        special: Some(special),
                    });
                }
            }

            // Apply gravity toward the current gravity wall
            self.apply_gravity_dir(self.gravity);

            // Refill from the upstream wall (echoes carry over to new gems)
            self.refill();

            // Check for new matches (cascade)
            current_matches = self.find_all_matches();
        }

        (total_cascades, clears_by_depth)
    }

    /// The public gravity resolver: compacts gems toward the given wall, then
    /// refills the resulting empty (upstream) boundary cells with random gems.
    pub fn resolve_gravity(&mut self, dir: Direction) {
        self.apply_gravity_dir(dir);
        self.refill();
    }

    /// Rotate gravity 90° (clockwise or counter-clockwise) and resolve the
    /// resulting tumbling cascades as a move. Always succeeds and costs a move.
    pub fn rotate_gravity(&mut self, clockwise: bool) -> MoveOutcome {
        self.gravity = if clockwise {
            self.gravity.rotate_cw()
        } else {
            self.gravity.rotate_ccw()
        };

        self.combo = 0;
        self.resonance_multiplier = 1.0;
        self.resonance_stack = 0;
        self.cleared_this_move.clear();

        // Tumble: pull every gem toward the new wall, refill the emptied side.
        self.resolve_gravity(self.gravity);

        let initial_matches = self.find_all_matches();
        let (total_cascades, clears_by_depth) = self.process_matches(initial_matches.clone());

        MoveOutcome::Success {
            matches: initial_matches,
            cascades: total_cascades,
            resonance_multiplier: self.resonance_multiplier,
            clears_by_depth,
        }
    }

    /// Apply gravity: compact gems toward the given wall direction.
    fn apply_gravity_dir(&mut self, dir: Direction) {
        match dir {
            Direction::Down | Direction::Up => self.apply_gravity_vertical(dir),
            Direction::Right | Direction::Left => self.apply_gravity_horizontal(dir),
        }
    }

    /// Compact gems within each column toward the top (Up) or bottom (Down).
    fn apply_gravity_vertical(&mut self, dir: Direction) {
        for col in 0..self.width {
            let mut write_row = if dir == Direction::Down {
                self.height - 1
            } else {
                0
            };
            let row_iter: Vec<usize> = if dir == Direction::Down {
                (0..self.height).rev().collect()
            } else {
                (0..self.height).collect()
            };
            for read_row in row_iter {
                let read_idx = self.idx(read_row, col);
                if self.gems[read_idx].is_some() {
                    if read_row != write_row {
                        let write_idx = self.idx(write_row, col);
                        self.gems[write_idx] = self.gems[read_idx].take();
                    }
                    if dir == Direction::Down {
                        if write_row > 0 {
                            write_row -= 1;
                        }
                    } else {
                        write_row += 1;
                    }
                }
            }
        }
    }

    /// Compact gems within each row toward the left (Left) or right (Right).
    fn apply_gravity_horizontal(&mut self, dir: Direction) {
        for row in 0..self.height {
            let mut write_col = if dir == Direction::Right {
                self.width - 1
            } else {
                0
            };
            let col_iter: Vec<usize> = if dir == Direction::Right {
                (0..self.width).rev().collect()
            } else {
                (0..self.width).collect()
            };
            for read_col in col_iter {
                let read_idx = self.idx(row, read_col);
                if self.gems[read_idx].is_some() {
                    if read_col != write_col {
                        let write_idx = self.idx(row, write_col);
                        self.gems[write_idx] = self.gems[read_idx].take();
                    }
                    if dir == Direction::Right {
                        if write_col > 0 {
                            write_col -= 1;
                        }
                    } else {
                        write_col += 1;
                    }
                }
            }
        }
    }

    /// Refill empty cells from the top with random gems.
    fn refill(&mut self) {
        for col in 0..self.width {
            for row in 0..self.height {
                let idx = self.idx(row, col);
                if self.gems[idx].is_none() {
                    let kind_idx = self.rng.gen_range(0..self.gem_types.len());
                    self.gems[idx] = Some(Gem {
                        kind: self.gem_types[kind_idx],
                        echo: None,
                        special: None,
                    });
                }
            }
        }
    }

    /// Decrement echo charges after a move.
    pub fn decrement_echoes(&mut self) {
        for gem in self.gems.iter_mut().flatten() {
            if let Some(echo) = &mut gem.echo {
                if echo.moves_left > 0 {
                    echo.moves_left -= 1;
                    if echo.moves_left == 0 {
                        gem.echo = None;
                    }
                }
            }
        }
    }

    /// Check if there are any legal moves on the board.
    pub fn has_legal_moves(&self) -> bool {
        for row in 0..self.height {
            for col in 0..self.width {
                // Check right
                if col + 1 < self.width && self.would_match(row, col, row, col + 1) {
                    return true;
                }
                // Check down
                if row + 1 < self.height && self.would_match(row, col, row + 1, col) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if swapping two positions would create a match.
    pub fn would_match(&self, r1: usize, c1: usize, r2: usize, c2: usize) -> bool {
        let idx1 = self.idx(r1, c1);
        let idx2 = self.idx(r2, c2);

        if self.gems[idx1].is_none() || self.gems[idx2].is_none() {
            return false;
        }

        // Simulate swap
        let kind1 = self.gems[idx1].as_ref().unwrap().kind;
        let kind2 = self.gems[idx2].as_ref().unwrap().kind;

        // Check matches around both positions after swap
        self.check_match_at(r1, c1, kind2) || self.check_match_at(r2, c2, kind1)
    }

    /// Check if there's a match at (row, col) assuming it has the given kind.
    fn check_match_at(&self, row: usize, col: usize, kind: GemKind) -> bool {
        // Horizontal
        let mut count = 1;
        // Left
        let mut c = col;
        while c > 0 {
            c -= 1;
            if let Some(gem) = self.gem(row, c) {
                if gem.kind == kind {
                    count += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        // Right
        c = col;
        while c + 1 < self.width {
            c += 1;
            if let Some(gem) = self.gem(row, c) {
                if gem.kind == kind {
                    count += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        if count >= 3 {
            return true;
        }

        // Vertical
        count = 1;
        // Up
        let mut r = row;
        while r > 0 {
            r -= 1;
            if let Some(gem) = self.gem(r, col) {
                if gem.kind == kind {
                    count += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        // Down
        r = row;
        while r + 1 < self.height {
            r += 1;
            if let Some(gem) = self.gem(r, col) {
                if gem.kind == kind {
                    count += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        count >= 3
    }
}

/// Scoring constants.
pub mod scoring {
    pub const BASE_GEM_POINTS: u32 = 10;
    pub const MATCH_3_MULTIPLIER: f32 = 1.0;
    pub const MATCH_4_MULTIPLIER: f32 = 1.5;
    pub const MATCH_5_MULTIPLIER: f32 = 2.0;
    pub const CASCADE_MULTIPLIER: f32 = 1.5;
    pub const RESONANCE_BASE_MULTIPLIER: f32 = 1.5;
    pub const RESONANCE_STACK_INCREMENT: f32 = 0.5;
    pub const RESONANCE_MAX_MULTIPLIER: f32 = 4.0;
}

/// Calculate score for a move outcome.
pub fn calculate_score(outcome: &MoveOutcome, combo: usize, rule_modifiers: &RuleModifiers) -> u32 {
    let MoveOutcome::Success {
        matches,
        cascades,
        resonance_multiplier,
        ..
    } = outcome
    else {
        return 0;
    };

    let mut score = 0u32;
    let cascade_mult = scoring::CASCADE_MULTIPLIER.powi(*cascades as i32);

    for m in matches {
        let match_size = m.cells.len();
        let base = scoring::BASE_GEM_POINTS * match_size as u32;

        let match_mult = match match_size {
            3 => scoring::MATCH_3_MULTIPLIER,
            4 => scoring::MATCH_4_MULTIPLIER,
            _ => scoring::MATCH_5_MULTIPLIER,
        };

        let combo_mult = 1.0 + (combo as f32 * 0.1);

        score +=
            (base as f32 * match_mult * cascade_mult * combo_mult * *resonance_multiplier) as u32;
    }

    // Apply score bonus from rule modifiers
    if rule_modifiers.score_bonus_pct > 0.0 {
        score = (score as f32 * (1.0 + rule_modifiers.score_bonus_pct)) as u32;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4-in-a-row match must leave a horizontal Bolt special gem persisted on
    /// the board (not just fire a create signal). This guards the FFI contract
    /// where Godot reads `get_cell().special` to render the emoji overlay.
    #[test]
    fn four_in_a_row_persists_bolt_special_gem() {
        // Search across deterministic seeds: the 4-run must place a horizontal
        // Bolt onto the board. Sometimes a later cascade legitimately re-matches
        // and re-clears the bolt (correct behavior); we assert that for at least
        // one seed the bolt survives to the final board, proving the special gem
        // is persisted on the board (not merely signalled).
        let mut surviving_seed = None;
        for seed in 0..=400u64 {
            let mut board = Board::new(5, 5, seed, vec![
                GemKind::Circle,
                GemKind::Triangle,
                GemKind::Square,
            ]);

            // Fill a clean two-color checkerboard (no pre-existing 3-in-a-rows).
            let checker = |i: usize, j: usize| {
                if (i + j) % 2 == 0 {
                    GemKind::Circle
                } else {
                    GemKind::Triangle
                }
            };
            for i in 0..board.height {
                for j in 0..board.width {
                    board.set_gem(i, j, checker(i, j));
                }
            }
            // Plant a horizontal run of 3 Circles on row 0 + a Circle directly below
            // the break cell, and a Square to cap the row so it stays a 4 (not 5).
            board.set_gem(0, 0, GemKind::Circle);
            board.set_gem(0, 1, GemKind::Circle);
            board.set_gem(0, 2, GemKind::Circle);
            board.set_gem(0, 3, GemKind::Triangle); // break cell
            board.set_gem(0, 4, GemKind::Square);   // cap -> keeps run at 4
            board.set_gem(1, 3, GemKind::Circle);   // swap source (completes the run)

            let outcome = board.try_swap(0, 3, 1, 3);

            if !matches!(outcome, MoveOutcome::Success { .. }) {
                continue;
            }

            let has_horizontal_bolt = board
                .gems
                .iter()
                .flatten()
                .any(|g| matches!(g.special, Some(SpecialGem::Bolt { horizontal: true })));
            if has_horizontal_bolt {
                surviving_seed = Some(seed);
                break;
            }
        }

        assert!(
            surviving_seed.is_some(),
            "for some seed a 4-in-a-row must leave a horizontal Bolt special gem on the board"
        );
    }

    /// The starting board must be free of pre-existing matches so that an
    /// unrelated swap is never accepted just because a match pre-exists on the
    /// board. (Uses the game's real 4-6 color pools, which are match-free-able;
    /// 2-color grids cannot be match-free on an 8x8.)
    #[test]
    fn new_board_is_match_free() {
        let four = vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ];
        let five = vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
            GemKind::Star,
        ];
        let six = vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
            GemKind::Star,
            GemKind::Cross,
        ];
        for seed in 0..200u64 {
            for gt in [four.clone(), five.clone(), six.clone()] {
                let board = Board::new(8, 8, seed, gt);
                assert!(
                    board.find_all_matches().is_empty(),
                    "seed {} produced a pre-existing match",
                    seed
                );
            }
        }
    }

    /// An adjacent swap that does NOT create a match must be rejected (and the
    /// board reverted), even on a board that might otherwise contain matches.
    #[test]
    fn unrelated_swap_is_rejected() {
        for seed in 0..300u64 {
            let mut board = Board::new(8, 8, seed, vec![
                GemKind::Circle,
                GemKind::Triangle,
                GemKind::Square,
                GemKind::Diamond,
            ]);
            for r in 0..board.height {
                for c in 0..board.width {
                    if c + 1 < board.width && !board.would_match(r, c, r, c + 1) {
                        let before = board.gems.clone();
                        let outcome = board.try_swap(r, c, r, c + 1);
                        assert!(
                            matches!(outcome, MoveOutcome::Illegal | MoveOutcome::NoMatch),
                            "seed {} pair ({r},{c})->({r},{}) should be rejected",
                            seed,
                            c + 1
                        );
                        assert_eq!(board.gems, before, "board must be reverted on illegal swap");
                        return;
                    }
                }
            }
        }
        panic!("no non-matching adjacent pair found across seeds");
    }

    #[test]
    fn direction_rotation_cycles() {
        assert_eq!(Direction::Down.rotate_cw(), Direction::Left);
        assert_eq!(Direction::Left.rotate_cw(), Direction::Up);
        assert_eq!(Direction::Up.rotate_cw(), Direction::Right);
        assert_eq!(Direction::Right.rotate_cw(), Direction::Down);
        assert_eq!(Direction::Down.rotate_ccw(), Direction::Right);
        assert_eq!(Direction::Right.rotate_ccw(), Direction::Up);
        assert_eq!(Direction::Up.rotate_ccw(), Direction::Left);
        assert_eq!(Direction::Left.rotate_ccw(), Direction::Down);
    }

    #[test]
    fn rotate_gravity_succeeds_and_changes_direction() {
        let mut board = Board::new(8, 8, 12345, vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ]);
        assert_eq!(board.gravity, Direction::Down);
        assert!(matches!(
            board.rotate_gravity(true),
            MoveOutcome::Success { .. }
        ));
        assert_eq!(board.gravity, Direction::Left);
        assert!(matches!(
            board.rotate_gravity(false),
            MoveOutcome::Success { .. }
        ));
        assert_eq!(board.gravity, Direction::Down);
    }

    #[test]
    fn rotate_gravity_keeps_board_full() {
        // Four clockwise rotations return gravity to Down; each opponent rotation
        // is a valid move (Success) and refills the board so no cell is empty.
        let mut board = Board::new(8, 8, 42, vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ]);
        for _ in 0..4 {
            assert!(matches!(
                board.rotate_gravity(true),
                MoveOutcome::Success { .. }
            ));
            for (i, g) in board.gems.iter().enumerate() {
                assert!(g.is_some(), "board not full after rotation at idx {}", i);
            }
        }
        assert_eq!(board.gravity, Direction::Down);
    }
}
