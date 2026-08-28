//! Bewildered core — board simulation, match detection, cascade resolution,
//! special gem creation, resonance echoes, and scoring.

use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

pub mod cube_board;
pub mod gem_types;
pub mod relics;
pub mod topology;
pub use cube_board::{CubeBoard, CubeOutcome, MatchConfig};
pub use gem_types::GemKind;
pub use relics::{DescentRun, Relic, Rarity};
pub use topology::{find_line_runs, CellId, Cube6Face, Flat2D, Topology};

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
#[derive(Debug, Serialize, Deserialize)]
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
    /// Topology for antipodal echo shockwaves and other geometry-dependent rules
    #[serde(skip)]
    pub topology: Option<Box<dyn Topology>>,
}

impl Default for RuleModifiers {
    fn default() -> Self {
        Self {
            diagonal_matches: false,
            fifth_hue: false,
            echo_extra_moves: 0,
            corner_cutter: false,
            greedy_nova: false,
            score_bonus_pct: 0.0,
            extra_moves: 0,
            collection_reduction_pct: 0,
            topology: None,
        }
    }
}

impl Clone for RuleModifiers {
    fn clone(&self) -> Self {
        Self {
            diagonal_matches: self.diagonal_matches,
            fifth_hue: self.fifth_hue,
            echo_extra_moves: self.echo_extra_moves,
            corner_cutter: self.corner_cutter,
            greedy_nova: self.greedy_nova,
            score_bonus_pct: self.score_bonus_pct,
            extra_moves: self.extra_moves,
            collection_reduction_pct: self.collection_reduction_pct,
            topology: None, // Topology is per-board, not cloned
        }
    }
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
/// Optional blocker occupying this cell (Stone or Ice).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gem {
    pub kind: GemKind,
    pub echo: Option<EchoCharge>,
    pub special: Option<SpecialGem>,
    pub blocker: Option<Blocker>,
}

impl Gem {
    /// A plain gem with no echo, special, or blocker.
    pub fn simple(kind: GemKind) -> Self {
        Self { kind, echo: None, special: None, blocker: None }
    }
}

/// Durable blockers that occupy cells and affect gravity/matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Blocker {
    /// Indestructible stone — falls with gravity, cannot be matched.
    Stone,
    /// Ice encasing a gem — immovable (immune to gravity) until adjacent
    /// match breaks it, then reveals the encased gem.
    Ice { layers: u8 },
}

impl Blocker {
    pub fn is_immovable(&self) -> bool {
        matches!(self, Blocker::Ice { .. })
    }

    pub fn hit(&self) -> Option<Blocker> {
        match self {
            Blocker::Stone => None, // Stone never breaks
            Blocker::Ice { layers: 1 } => None, // Last layer breaks, reveals gem
            Blocker::Ice { layers } => Some(Blocker::Ice { layers: layers - 1 }),
        }
    }
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
    pub rng: StdRng,
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
            for _ in 0..100 {
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
                blocker: None,
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
            self.gems[idx] = Some(Gem { kind, echo: None, special: None, blocker: None });
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

        // Blocked cells (Stone/Ice) cannot be swapped by the player.
        if self.gems[idx1].as_ref().unwrap().blocker.is_some()
            || self.gems[idx2].as_ref().unwrap().blocker.is_some()
        {
            return MoveOutcome::Illegal;
        }

        // Store pre-swap matches to detect if this swap CREATED new matches
        let pre_swap_matches = self.find_all_matches();
        let pre_swap_cells: std::collections::HashSet<(usize, usize)> = 
            pre_swap_matches.iter().flat_map(|m| m.cells.iter().cloned()).collect();

        // Simulate the swap
        self.gems.swap(idx1, idx2);

        // Check for matches after swap
        let post_swap_matches = self.find_all_matches();

        // A swap is only legal if it CREATED new matches involving the swapped cells.
        // Check if there are NEW matches that involve the swapped cells.
        let swap_caused = post_swap_matches.iter().any(|m| {
            m.cells.contains(&(row1, col1)) || m.cells.contains(&(row2, col2))
        });

        // Also check that the matches are NEW (not pre-existing)
        let new_match_cells: std::collections::HashSet<(usize, usize)> = 
            post_swap_matches.iter().flat_map(|m| m.cells.iter().cloned()).collect();
        let pre_swap_cells: std::collections::HashSet<(usize, usize)> = 
            pre_swap_matches.iter().flat_map(|m| m.cells.iter().cloned()).collect();
        let new_match_cells: std::collections::HashSet<_> = 
            new_match_cells.difference(&pre_swap_cells).cloned().collect();
        
        let swap_created_new_matches = new_match_cells.iter().any(|cell| {
            *cell == (row1, col1) || *cell == (row2, col2)
        });

        if post_swap_matches.is_empty() || !swap_created_new_matches {
            // No match was actually produced by THIS swap — revert it.
            self.gems.swap(idx1, idx2);
            return if pre_swap_matches.is_empty() {
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
            self.process_matches(self.find_all_matches());

        let resonance_mult = self.resonance_multiplier;

        MoveOutcome::Success {
            matches: self.find_all_matches(), // Re-evaluate after cascades
            cascades: total_cascades,
            resonance_multiplier: self.resonance_multiplier,
            clears_by_depth: Vec::new(), // Simplified
        }
    }

    /// Find all matches on the board (horizontal and vertical, 3+).
    /// Also detects L/T-shaped matches for Prism creation.
    pub fn find_all_matches(&self) -> Vec<Match> {
        // First pass: find all raw matches (horizontal and vertical)
        let mut raw_matches = Vec::new();

        // Horizontal matches. Blocked cells (Stone/Ice) never participate in
        // runs — they break the run like an empty cell would.
        for row in 0..self.height {
            let mut col = 0;
            while col < self.width {
                let idx = self.idx(row, col);
                if let Some(gem) = &self.gems[idx] {
                    if gem.blocker.is_some() {
                        col += 1;
                        continue;
                    }
                    let kind = gem.kind;
                    let mut run_len = 1;
                    while col + run_len < self.width {
                        let next_idx = self.idx(row, col + run_len);
                        if let Some(next_gem) = &self.gems[next_idx] {
                            if next_gem.blocker.is_some() {
                                break;
                            }
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

        // Vertical matches (blocked cells break runs, same as horizontal)
        for col in 0..self.width {
            let mut row = 0;
            while row < self.height {
                let idx = self.idx(row, col);
                if let Some(gem) = &self.gems[idx] {
                    if gem.blocker.is_some() {
                        row += 1;
                        continue;
                    }
                    let kind = gem.kind;
                    let mut run_len = 1;
                    while row + run_len < self.height {
                        let next_idx = self.idx(row + run_len, col);
                        if let Some(next_gem) = &self.gems[next_idx] {
                            if next_gem.blocker.is_some() {
                                break;
                            }
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

                    // Antipodal Resonance Shockwave: charge the cell on the exact
                    // opposite face (Cube6Face) or do nothing (Flat2D).
                    // Use the first cell of the match as the detonation origin.
                    if let Some(&(det_r, det_c)) = m.cells.first() {
                        self.charge_antipodal_echo(det_r, det_c);
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

            // Deduplicate extra_clear_cells to avoid clearing the same cell multiple times
            let unique_extra_clears: std::collections::HashSet<(usize, usize)> = extra_clear_cells.iter().cloned().collect();
            for (r, c) in unique_extra_clears {
                if let Some(kind) = self.gem(r, c).map(|g| g.kind) {
                    self.cleared_this_move.push((r, c, kind));
                    round_clears.push((r, c, kind));
                    self.remove_gem(r, c);
                }
            }

            // Clear Nova color if present (but skip special gems - they are immune to Nova)
            if let Some(color) = nova_color {
                for i in 0..self.gems.len() {
                    let is_nova_kind = Self::gem_kind_at(&self.gems, i) == Some(color);
                    if is_nova_kind {
                        // Skip special gems - they are immune to Nova
                        if self.gems[i].as_ref().map(|g| g.special.is_some()).unwrap_or(false) {
                            continue;
                        }
                        let (nr, nc) = (i / self.width, i % self.width);
                        self.cleared_this_move.push((nr, nc, color));
                        round_clears.push((nr, nc, color));
                        self.gems[i] = None;
                    }
                }
            }

            // Break ice blockers adjacent to any cleared cell in this cascade step
            for m in &current_matches {
                for &(r, c) in &m.cells {
                    self.hit_adjacent_ice(r, c);
                }
            }
            // Also hit ice adjacent to extra clear cells from specials/detonations
            for (r, c) in &extra_clear_cells {
                self.hit_adjacent_ice(*r, *c);
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
                        blocker: None,
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

    /// Rotate the grid matrix 90° (clockwise or counter-clockwise) and resolve
    /// the resulting falling cascades as a move. Always succeeds and costs a
    /// move. Gravity is ALWAYS Down in the rotated orientation: the grid data
    /// truly transposes (width/height swap) and gems fall toward the new bottom
    /// row, cascade, and refill from the top.
    ///
    /// Clockwise 90°:  new[r][c] = old[height-1-c][r]
    /// Counter-clockwise: new[r][c] = old[c][width-1-r]
    pub fn rotate_board(&mut self, clockwise: bool) -> MoveOutcome {
        let old_w = self.width;
        let old_h = self.height;
        let mut new_gems = vec![None; old_w * old_h];
        for nr in 0..old_w {
            for nc in 0..old_h {
                // nr in 0..new_height(=old_w), nc in 0..new_width(=old_h)
                let (or, oc) = if clockwise {
                    (old_h - 1 - nc, nr)
                } else {
                    (nc, old_w - 1 - nr)
                };
                new_gems[nr * old_h + nc] = self.gems[or * old_w + oc].clone();
            }
        }

        self.gems = new_gems;
        self.height = old_w;
        self.width = old_h;
        self.gravity = Direction::Down;

        self.combo = 0;
        self.resonance_multiplier = 1.0;
        self.resonance_stack = 0;
        self.cleared_this_move.clear();

        // Gems fall toward the new bottom row, cascade, and refill from the top.
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
    /// Stone blockers fall with gravity; Ice blockers are immovable and act as
    /// a floor/ceiling that falling gems stack against.
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
                let immovable = self.gems[read_idx]
                    .as_ref()
                    .map(|g| g.blocker.as_ref().map(|b| b.is_immovable()).unwrap_or(false))
                    .unwrap_or(false);
                if immovable {
                    // Frozen cell acts as the new floor: subsequent gems stack
                    // against it instead of overwriting it.
                    if dir == Direction::Down {
                        write_row = read_row.saturating_sub(1);
                    } else if read_row + 1 < self.height {
                        write_row = read_row + 1;
                    }
                    continue;
                }
                if self.gems[read_idx].is_some() {
                    if read_row != write_row {
                        let write_idx = self.idx(write_row, col);
                        self.gems[write_idx] = self.gems[read_idx].take();
                    }
                    if dir == Direction::Down {
                        if write_row > 0 {
                            write_row -= 1;
                        }
                    } else if write_row + 1 < self.height {
                        write_row += 1;
                    }
                }
            }
        }
    }

    /// Compact gems within each row toward the left (Left) or right (Right).
    /// Stone blockers fall with gravity; Ice blockers are immovable and act as
    /// a wall that sliding gems stack against.
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
                let immovable = self.gems[read_idx]
                    .as_ref()
                    .map(|g| g.blocker.as_ref().map(|b| b.is_immovable()).unwrap_or(false))
                    .unwrap_or(false);
                if immovable {
                    if dir == Direction::Right {
                        write_col = read_col.saturating_sub(1);
                    } else if read_col + 1 < self.width {
                        write_col = read_col + 1;
                    }
                    continue;
                }
                if self.gems[read_idx].is_some() {
                    if read_col != write_col {
                        let write_idx = self.idx(row, write_col);
                        self.gems[write_idx] = self.gems[read_idx].take();
                    }
                    if dir == Direction::Right {
                        if write_col > 0 {
                            write_col -= 1;
                        }
                    } else if write_col + 1 < self.width {
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
                        blocker: None,
                    });
                }
            }
        }
    }

    /// Hit ice blockers adjacent to the given cell (orthogonal neighbors).
    /// Returns the number of ice layers broken.
    fn hit_adjacent_ice(&mut self, row: usize, col: usize) -> usize {
        let mut broken = 0;
        let neighbors = [
            (row.wrapping_sub(1), col),
            (row + 1, col),
            (row, col.wrapping_sub(1)),
            (row, col + 1),
        ];
        for (nr, nc) in neighbors {
            if nr < self.height && nc < self.width {
                let idx = self.idx(nr, nc);
                if let Some(gem) = &mut self.gems[idx] {
                    if let Some(Blocker::Ice { layers }) = gem.blocker {
                        if layers == 1 {
                            gem.blocker = None; // Ice breaks, reveals gem
                        } else {
                            gem.blocker = Some(Blocker::Ice { layers: layers - 1 });
                        }
                        broken += 1;
                    }
                }
            }
        }
        broken
    }

    /// Charge the antipodal cell of an echo detonation using the board's topology.
    /// For Flat2D, does nothing. For Cube6Face, charges the opposite face cell.
    pub fn charge_antipodal_echo(&mut self, row: usize, col: usize) {
        if let Some(topology) = &self.rule_modifiers.topology {
            let cell = CellId(self.idx(row, col) as u32);
            if let Some(anti_cell) = topology.antipode(cell) {
                let anti_idx = anti_cell.0 as usize;
                if anti_idx < self.gems.len() {
                    if let Some(gem) = &mut self.gems[anti_idx] {
                        // Add or extend echo charge on the antipodal cell
                        if let Some(echo) = &mut gem.echo {
                            echo.moves_left = echo.moves_left.max(2); // At least 2 moves for antipodal
                        } else {
                            gem.echo = Some(EchoCharge::with_duration(2));
                        }
                    }
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

        // Blocked cells can never be swapped into a match.
        if self.gems[idx1].as_ref().unwrap().blocker.is_some()
            || self.gems[idx2].as_ref().unwrap().blocker.is_some()
        {
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
                if gem.blocker.is_none() && gem.kind == kind {
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
                if gem.blocker.is_none() && gem.kind == kind {
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
                if gem.blocker.is_none() && gem.kind == kind {
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
                if gem.blocker.is_none() && gem.kind == kind {
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

    /// Free-function index helper avoiding two-phase borrow issues in tests.
    fn idx_of(b: &Board, r: usize, c: usize) -> usize {
        b.idx(r, c)
    }

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

    /// Stone blockers fall with gravity like normal gems.
    #[test]
    fn stone_blocker_falls_with_gravity() {
        let mut board = Board::new(3, 3, 1, vec![GemKind::Circle, GemKind::Triangle]);
        // Empty the middle column below a Stone at (0,1).
        let i = idx_of(&board, 1, 1);
        board.gems[i] = None;
        let i = idx_of(&board, 2, 1);
        board.gems[i] = None;
        let i = idx_of(&board, 0, 1);
        board.gems[i] = Some(Gem {
            kind: GemKind::Square,
            echo: None,
            special: None,
            blocker: Some(Blocker::Stone),
        });

        board.resolve_gravity(Direction::Down);

        // Stone compacts to the bottom of its column.
        let bottom = board.gem(2, 1).expect("stone must land at bottom");
        assert_eq!(bottom.blocker, Some(Blocker::Stone));
        // Cells above refilled with normal gems (no blocker).
        for r in 0..2 {
            let g = board.gem(r, 1).expect("refilled");
            assert!(g.blocker.is_none());
        }
    }

    /// Ice blockers are immune to gravity: neither they nor gems stacked above
    /// them fall past their frozen position.
    #[test]
    fn ice_blocker_is_immovable_under_gravity() {
        let mut board = Board::new(3, 3, 1, vec![GemKind::Circle, GemKind::Triangle]);
        let i = idx_of(&board, 2, 1);
        board.gems[i] = None;
        let i = idx_of(&board, 1, 1);
        board.gems[i] = Some(Gem {
            kind: GemKind::Circle,
            echo: None,
            special: None,
            blocker: Some(Blocker::Ice { layers: 2 }),
        });

        board.resolve_gravity(Direction::Down);

        let g = board.gem(1, 1).expect("iced gem must stay put");
        assert_eq!(g.blocker, Some(Blocker::Ice { layers: 2 }));
    }

    /// Blocked cells cannot be swapped and never participate in match runs.
    #[test]
    fn blocked_cells_reject_swaps_and_break_runs() {
        let kinds = vec![GemKind::Circle, GemKind::Triangle];
        let mut board = Board::new(4, 1, 1, kinds);
        // Row: Circle, Circle(stoned), Circle, Triangle
        for (c, mut g) in [
            (0usize, Gem::simple(GemKind::Circle)),
            (1, Gem::simple(GemKind::Circle)),
            (2, Gem::simple(GemKind::Circle)),
            (3, Gem::simple(GemKind::Triangle)),
        ] {
            if c == 1 {
                g.blocker = Some(Blocker::Stone);
            }
            let i = idx_of(&board, 0, c);
            board.gems[i] = Some(g);
        }

        // Three Circles exist but the Stone breaks the run — no match.
        assert!(board.find_all_matches().is_empty());

        // Swapping the stoned cell is rejected outright.
        assert!(matches!(
            board.try_swap(0, 1, 0, 2),
            MoveOutcome::Illegal
        ));
    }

    /// An adjacent match chips one ice layer per cascade step. With two layers,
    /// one move leaves the gem still frozen (and immovable).
    #[test]
    fn adjacent_match_thaws_one_ice_layer() {
        let mut board = Board::new(4, 4, 7, vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ]);
        let set = |b: &mut Board, r: usize, c: usize, k: GemKind, blk: Option<Blocker>| {
            let i = b.idx(r, c);
            b.gems[i] = Some(Gem { kind: k, echo: None, special: None, blocker: blk });
        };
        // Grid chosen so swapping (0,2)<->(1,2) makes a C-C-C row match at
        // row 1, orthogonal to the iced gem at (2,1). No pre-existing runs.
        set(&mut board, 0, 0, GemKind::Triangle, None);
        set(&mut board, 0, 1, GemKind::Square, None);
        set(&mut board, 0, 2, GemKind::Circle, None);
        set(&mut board, 0, 3, GemKind::Triangle, None);
        set(&mut board, 1, 0, GemKind::Circle, None);
        set(&mut board, 1, 1, GemKind::Circle, None);
        set(&mut board, 1, 2, GemKind::Square, None);
        set(&mut board, 1, 3, GemKind::Diamond, None);
        set(&mut board, 2, 0, GemKind::Diamond, None);
        set(&mut board, 2, 1, GemKind::Square, Some(Blocker::Ice { layers: 2 }));
        set(&mut board, 2, 2, GemKind::Diamond, None);
        set(&mut board, 2, 3, GemKind::Square, None);
        set(&mut board, 3, 0, GemKind::Square, None);
        set(&mut board, 3, 1, GemKind::Diamond, None);
        set(&mut board, 3, 2, GemKind::Triangle, None);
        set(&mut board, 3, 3, GemKind::Circle, None);

        assert!(board.find_all_matches().is_empty(), "setup must be match-free");

        let outcome = board.try_swap(0, 2, 1, 2);
        assert!(matches!(outcome, MoveOutcome::Success { .. }), "swap must create the row match");

        // One adjacent match step: 2 layers -> 1 layer. The gem is still frozen
        // in place (gravity cannot move it).
        let g = board.gem(2, 1).expect("iced gem still at its cell");
        assert_eq!(g.blocker, Some(Blocker::Ice { layers: 1 }));
    }

    /// Echo detonations charge the exact antipodal cell through the cube
    /// (verified on a degenerate 1x1-face cube where all 6 faces fit in a
    /// 1x6 board: cell 0 (Front) charges cell 2 (Back)).
    #[test]
    fn antipodal_echo_charges_opposite_face() {
        let mods = RuleModifiers {
            topology: Some(Box::new(Cube6Face::new(1))),
            ..RuleModifiers::new()
        };
        let mut board = Board::with_rules(1, 6, 3, vec![GemKind::Circle], mods);

        board.charge_antipodal_echo(0, 0);

        // Face 0 cell 0 -> antipode is Face 2 cell 0 = index 2 = (row 2, col 0).
        let anti = board.gem(2, 0).expect("antipodal cell occupied");
        let echo = anti.echo.as_ref().expect("antipode must carry an echo charge");
        assert!(echo.moves_left >= 2, "antipodal echo lasts at least 2 moves");

        // Non-antipodal cells stay uncharged.
        assert!(board.gem(1, 0).unwrap().echo.is_none());
        assert!(board.gem(3, 0).unwrap().echo.is_none());
    }

    /// On Flat2D there is no antipode: charging must be a clean no-op.
    #[test]
    fn flat2d_antipodal_charge_is_noop() {
        let mods = RuleModifiers {
            topology: Some(Box::new(Flat2D::new(3, 3))),
            ..RuleModifiers::new()
        };
        let mut board = Board::with_rules(3, 3, 3, vec![GemKind::Circle], mods);

        board.charge_antipodal_echo(1, 1);

        assert!(board.gems.iter().flatten().all(|g| g.echo.is_none()));
    }

    /// rotate_board transposes the physical grid, swaps width/height, and keeps
    /// gravity pointing Down. Uses a non-square board to prove transposition.
    #[test]
    fn rotate_board_cw_transposes_grid() {
        let kinds = [
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ];
        // width=2, height=3 (3 rows, 2 cols). Encode cell kind by row-major index.
        let mut board = Board::new(2, 3, 7, kinds.to_vec());
        for r in 0..3 {
            for c in 0..2 {
                board.set_gem(r, c, kinds[(r * 2 + c) % 4]);
            }
        }

        assert!(matches!(
            board.rotate_board(true),
            MoveOutcome::Success { .. }
        ));
        // Width/height swap: width=3, height=2.
        assert_eq!(board.width, 3);
        assert_eq!(board.height, 2);
        assert_eq!(board.gravity, Direction::Down);

        // CW: new[r][c] = old[old_h-1-c][r], old_h = 3.
        // new(0,0) = old(3-1-0, 0) = old(2,0) -> idx 2*2+0=4 -> 4%4=0 Circle
        // new(1,0) = old(2,1) -> idx 5 -> 5%4=1 Triangle
        assert_eq!(board.gem(0, 0).map(|g| g.kind), Some(GemKind::Circle));
        assert_eq!(board.gem(1, 0).map(|g| g.kind), Some(GemKind::Triangle));
        // new(0,2) = old(3-1-2,0)=old(0,0) -> idx 0 -> Circle
        assert_eq!(board.gem(0, 2).map(|g| g.kind), Some(GemKind::Circle));
    }

    /// Counter-clockwise transpose: new[r][c] = old[c][width-1-r].
    #[test]
    fn rotate_board_ccw_transposes_grid() {
        let kinds = [
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ];
        let mut board = Board::new(3, 2, 9, kinds.to_vec()); // width=3, height=2
        for r in 0..2 {
            for c in 0..3 {
                board.set_gem(r, c, kinds[(r * 3 + c) % 4]);
            }
        }
        assert!(matches!(
            board.rotate_board(false),
            MoveOutcome::Success { .. }
        ));
        // width=old_height=2, height=old_width=3
        assert_eq!(board.width, 2);
        assert_eq!(board.height, 3);
        assert_eq!(board.gravity, Direction::Down);
        // CCW: new[r][c] = old[c][width-1-r]; old width=3.
        // new(0,0) = old(0, 3-1-0)=old(0,2) -> idx 0*3+2=2 -> 2%4=2 Square
        assert_eq!(board.gem(0, 0).map(|g| g.kind), Some(GemKind::Square));
    }

    #[test]
    fn rotate_board_keeps_board_full_and_gravity_down() {
        // Rotating an 8x8 and running cascades must leave a full board with
        // gravity still pointing Down (always in rotate_board).
        let mut board = Board::new(8, 8, 42, vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ]);
        for _ in 0..4 {
            assert!(matches!(
                board.rotate_board(true),
                MoveOutcome::Success { .. }
            ));
            assert_eq!(board.gravity, Direction::Down);
            assert_eq!(board.width, 8);
            assert_eq!(board.height, 8);
            for (i, g) in board.gems.iter().enumerate() {
                assert!(g.is_some(), "board not full after rotation at idx {}", i);
            }
        }
    }
}
