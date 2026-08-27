//! Full match-3 simulation over a [`Cube6Face`] manifold.
//!
//! [`crate::Board`] owns the classic 2D rules; this module owns the cube
//! variant so the Godot bridge (`CubeSim`) stays a thin FFI shell. All rules
//! live here: seam-aware match detection, cascades, echo detonations,
//! antipodal shockwaves, blockers, and face rotation (the 3D Tumbler).

use crate::topology::{find_line_runs, CellId, Cube6Face};
use crate::{Direction, EchoCharge, Gem, GemKind, SpecialGem, Topology};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Result of a successful cube move (swap or face rotation).
#[derive(Debug, Clone)]
pub struct CubeOutcome {
    /// Number of cascade steps (>= 1).
    pub cascades: usize,
    /// Per-cascade-depth cleared cells with their kind at clear-time.
    pub clears_by_depth: Vec<Vec<(CellId, GemKind)>>,
    /// Resonance multiplier reached by echo detonations this move.
    pub resonance_multiplier: f32,
    /// Cells whose echo charges detonated (origin of each shockwave).
    pub echoes_detonated: Vec<CellId>,
    /// Antipodal cells charged by the shockwaves.
    pub antipodal_charged: Vec<CellId>,
    /// Special gems persisted onto the board this move `(cell, kind, special)`.
    pub specials_created: Vec<(CellId, GemKind, SpecialGem)>,
}

/// Configuration flags for optional match mechanics (debug/baseline mode).
#[derive(Debug, Clone, Copy)]
pub struct MatchConfig {
    pub enable_echo: bool,
    pub enable_antipodal: bool,
    pub enable_specials: bool,
    /// Enable roguelike descent system (chambers, relics, scoring, move limits).
    pub enable_descent: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            enable_echo: true,
            enable_antipodal: true,
            enable_specials: true,
            enable_descent: true,
        }
    }
}

impl MatchConfig {
    /// Baseline: only standard match-3/4/5, no echo, antipodal, specials, no descent.
    pub fn baseline() -> Self {
        Self {
            enable_echo: false,
            enable_antipodal: false,
            enable_specials: false,
            enable_descent: false,
        }
    }

    /// Pure vanilla: only match-3/4/5 clears, no special mechanics at all.
    pub fn vanilla() -> Self {
        Self {
            enable_echo: false,
            enable_antipodal: false,
            enable_specials: false,
            enable_descent: false,
        }
    }

    /// Full featured: all mechanics enabled.
    pub fn full() -> Self {
        Self {
            enable_echo: true,
            enable_antipodal: true,
            enable_specials: true,
            enable_descent: true,
        }
    }
}

/// A six-face match-3 board. Cell space is the flat `CellId` space of
/// [`Cube6Face`]: `face * N * N + y * N + x`,
/// faces ordered 0=Front, 1=Right, 2=Back, 3=Left, 4=Top, 5=Bottom.
pub struct CubeBoard {
    /// One slot per cube cell; `None` = empty (transient, refilled same move).
    pub cells: Vec<Option<Gem>>,
    pub topology: Cube6Face,
    pub face_size: usize,
    pub rng: StdRng,
    pub gem_types: Vec<GemKind>,
    pub combo: usize,
    pub resonance_multiplier: f32,
    pub resonance_stack: usize,
    /// Relic bonus: freshly seeded echo charges last 1 + this many turns.
    pub echo_extra_moves: u8,
    /// Relic bonus: fractional score multiplier applied by the FFI layer.
    pub score_bonus_pct: f32,
    /// Relic bonus: extra moves granted at chamber start (run-layer).
    pub extra_moves: u8,
    /// Optional match mechanics config.
    pub match_config: MatchConfig,
}

impl CubeBoard {
    pub fn new(face_size: usize, seed: u64, gem_types: Vec<GemKind>) -> Self {
        Self::with_config(face_size, seed, gem_types, MatchConfig::default())
    }

    pub fn new_baseline(face_size: usize, seed: u64, gem_types: Vec<GemKind>) -> Self {
        Self::with_config(face_size, seed, gem_types, MatchConfig::baseline())
    }

    pub fn with_config(
        face_size: usize,
        seed: u64,
        gem_types: Vec<GemKind>,
        match_config: MatchConfig,
    ) -> Self {
        let topology = Cube6Face::new(face_size);
        let mut board = Self {
            cells: vec![None; topology.cell_count()],
            topology,
            face_size,
            rng: StdRng::seed_from_u64(seed),
            gem_types,
            combo: 0,
            resonance_multiplier: 1.0,
            resonance_stack: 0,
            echo_extra_moves: 0,
            score_bonus_pct: 0.0,
            extra_moves: 0,
            match_config,
        };
        board.fill_random_match_free();
        board
    }

    pub fn set_match_config(&mut self, config: MatchConfig) {
        self.match_config = config;
    }

    /// Cell handle for face-local coordinates.
    pub fn cell(&self, face: usize, x: usize, y: usize) -> CellId {
        let n = self.face_size;
        CellId(((face * n + y) * n + x) as u32)
    }

    /// Face-local coordinates of a cell: `(face, x, y)`.
    pub fn coords(&self, cell: CellId) -> (usize, usize, usize) {
        let n = self.face_size;
        let c = cell.0 as usize;
        let face = c / (n * n);
        let rem = c % (n * n);
        (face, rem % n, rem / n)
    }

    pub fn face_of(&self, cell: CellId) -> usize {
        self.coords(cell).0
    }

    pub fn get(&self, cell: CellId) -> Option<&Gem> {
        self.cells.get(cell.0 as usize).and_then(|g| g.as_ref())
    }

    /// Seam-aware run detection over unblocked gems.
    pub fn find_matches(&self) -> Vec<(Vec<CellId>, GemKind)> {
        let kinds: Vec<Option<u8>> = self
            .cells
            .iter()
            .map(|g| match g {
                Some(gem) if gem.blocker.is_none() => Some(gem.kind as u8),
                _ => None,
            })
            .collect();
        find_line_runs(&self.topology, &kinds, 3)
            .into_iter()
            .map(|(cells, kind)| {
                (
                    cells,
                    GemKind::try_from(kind).unwrap_or(GemKind::Circle),
                )
            })
            .collect()
    }

    /// Fill every empty cell with a random gem, then re-roll middles of any
    /// accidental (seam-crossing) pre-matches until the board is match-free.
    /// Bounded loop keeps generation deterministic.
    fn fill_random_match_free(&mut self) {
        for i in 0..self.cells.len() {
            self.cells[i] = Some(self.random_gem());
        }
        for _ in 0..64 {
            let runs = self.find_matches();
            if runs.is_empty() {
                break;
            }
            for (run, _) in &runs {
                let mid = run[run.len() / 2];
                let g = self.random_gem();
                if let Some(slot) = self.cells.get_mut(mid.0 as usize) {
                    *slot = Some(g);
                }
            }
        }
    }

    fn random_gem(&mut self) -> Gem {
        let idx = self.rng.gen_range(0..self.gem_types.len());
        Gem::simple(self.gem_types[idx])
    }

    /// Orthogonal neighbours of a cell across the whole manifold.
    fn neighbors(&self, cell: CellId) -> Vec<CellId> {
        let dirs = [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ];
        dirs.iter()
            .filter_map(|d| self.topology.step(cell, *d))
            .map(|(c, _)| c)
            .collect()
    }

    /// Try to swap two same-face orthogonally adjacent gems. Returns the move
    /// outcome, or `None` if the swap was rejected (bounds, non-adjacency,
    /// blocked cells, or no match created).
    pub fn try_swap(&mut self, a: CellId, b: CellId) -> Option<CubeOutcome> {
        let (fa, xa, ya) = self.coords(a);
        let (fb, xb, yb) = self.coords(b);
        if fa != fb || xa >= self.face_size || ya >= self.face_size {
            return None;
        }
        if xa.abs_diff(xb) + ya.abs_diff(yb) != 1 {
            return None;
        }
        if self.get(a)?.blocker.is_some() || self.get(b)?.blocker.is_some() {
            return None;
        }

        self.cells.swap(a.0 as usize, b.0 as usize);

        let mut runs = self.find_matches();
        let swap_caused = runs.iter().any(|(cells, _)| cells.contains(&a) || cells.contains(&b));
        if !swap_caused {
            // Revert — no match was produced by THIS swap.
            self.cells.swap(a.0 as usize, b.0 as usize);
            return None;
        }

        Some(Self::process_cascades(self, &mut runs))
    }

    /// Rotate one face's grid 90° (the 3D Gravity Tumbler). Always succeeds;
    /// costs a move. Any resulting matches resolve as a cascade wave.
    pub fn rotate_face(&mut self, face: usize, clockwise: bool) -> Option<CubeOutcome> {
        if face >= 6 {
            return None;
        }
        let n = self.face_size;
        let base = face * n * n;
        let mut rotated: Vec<Option<Gem>> = vec![None; n * n];
        for ny in 0..n {
            for nx in 0..n {
                let (ox, oy) = if clockwise {
                    // CW display rotation: new(nx,ny) = old(ny, n-1-nx)
                    (ny, n - 1 - nx)
                } else {
                    // CCW: new(nx,ny) = old(n-1-ny, nx)
                    (n - 1 - ny, nx)
                };
                rotated[ny * n + nx] = self.cells[base + oy * n + ox].take();
            }
        }
        for (i, g) in rotated.into_iter().enumerate() {
            self.cells[base + i] = g;
        }

        self.combo = 0;
        let mut runs = self.find_matches();
        Some(Self::process_cascades(self, &mut runs))
    }

    /// Shared cascade resolver: clears runs (with echo detonations, special
    /// activations, ice chipping, antipodal charging), refills, rescans until
    /// stable. Returns the accumulated outcome.
    fn process_cascades(&mut self, initial_runs: &mut Vec<(Vec<CellId>, GemKind)>) -> CubeOutcome {
        let mut outcome = CubeOutcome {
            cascades: 0,
            clears_by_depth: Vec::new(),
            resonance_multiplier: 1.0,
            echoes_detonated: Vec::new(),
            antipodal_charged: Vec::new(),
            specials_created: Vec::new(),
        };
        let mut current_runs = std::mem::take(initial_runs);
        let mut fresh_charges: Vec<CellId> = Vec::new();

        loop {
            if current_runs.is_empty() {
                break;
            }
            outcome.cascades += 1;
            self.combo += 1;

            let mut extra_clears: Vec<CellId> = Vec::new();
            let mut nova_kind: Option<GemKind> = None;

            // Echo detonation (optional).
            if self.match_config.enable_echo {
                for (run, kind) in &current_runs {
                    let echoed: Vec<CellId> = run
                        .iter()
                        .filter(|c| {
                            if fresh_charges.contains(c) {
                                return false;
                            }
                            self.get(**c).and_then(|g| g.echo.as_ref()).is_some()
                        })
                        .copied()
                        .collect();
                    if !echoed.is_empty() {
                        self.resonance_stack += 1;
                        self.resonance_multiplier = (1.5
                            + (self.resonance_stack.saturating_sub(1) as f32 * 0.5))
                            .min(4.0);

                        for origin in &echoed {
                            outcome.echoes_detonated.push(*origin);
                            // Antipodal charging (optional).
                            if self.match_config.enable_antipodal {
                                for nb in self.neighbors(*origin) {
                                    extra_clears.push(nb);
                                }
                                if let Some(anti) = self.topology.antipode(*origin) {
                                    if let Some(gem) = self.cells.get_mut(anti.0 as usize).and_then(|s| s.as_mut()) {
                                        if let Some(echo) = &mut gem.echo {
                                            echo.moves_left = echo.moves_left.max(2);
                                        } else {
                                            gem.echo = Some(EchoCharge::with_duration(2));
                                        }
                                        outcome.antipodal_charged.push(anti);
                                        fresh_charges.push(anti);
                                    } else {
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Special creation: 4-run -> Bolt, 5+-run -> Nova (optional).
            if self.match_config.enable_specials {
                for (run, kind) in &current_runs {
                    if run.len() >= 5 {
                        nova_kind = Some(*kind);
                    }
                }
            }

            // Collect the clear set for this depth (matched + extras).
            let mut clearing: Vec<CellId> = current_runs
                .iter()
                .flat_map(|(run, _)| run.iter().copied())
                .collect();
            for c in extra_clears {
                if !clearing.contains(&c) {
                    clearing.push(c);
                }
            }
            if let Some(nk) = nova_kind {
                for i in 0..self.cells.len() {
                    if let Some(gem) = &self.cells[i] {
                        if gem.blocker.is_none() && gem.kind == nk {
                            let cid = CellId(i as u32);
                            if !clearing.contains(&cid) {
                                clearing.push(cid);
                            }
                        }
                    }
                }
            }

            // Clear, recording kinds at clear-time; chip ice adjacent to clears.
            let mut depth_clears: Vec<(CellId, GemKind)> = Vec::new();
            let mut pending_echo_cells: Vec<CellId> = Vec::new();
            let mut specials_to_place: Vec<(CellId, GemKind, SpecialGem)> = Vec::new();

            if self.match_config.enable_specials {
                for (run, kind) in &current_runs {
                    if run.len() == 4 {
                        let mid = run[2];
                        specials_to_place.push((mid, *kind, SpecialGem::Bolt { horizontal: true }));
                    } else if run.len() >= 5 {
                        let mid = run[run.len() / 2];
                        specials_to_place.push((mid, *kind, SpecialGem::Nova));
                    }
                }
            }

            for cell in &clearing {
                let idx = cell.0 as usize;
                if let Some(gem) = &self.cells[idx] {
                    let kind = gem.kind;
                    depth_clears.push((*cell, kind));
                    if gem.special.is_none() && gem.blocker.is_none() {
                        pending_echo_cells.push(*cell);
                    }
                    self.cells[idx] = None;
                }
            }
            for cell in &clearing {
                for nb in self.neighbors(*cell) {
                    let idx = nb.0 as usize;
                    if let Some(gem) = self.cells[idx].as_mut() {
                        if let Some(blocker) = &gem.blocker {
                            gem.blocker = blocker.hit();
                        }
                    }
                }
            }
            if self.match_config.enable_specials {
                for (cell, kind, special) in specials_to_place {
                    let idx = cell.0 as usize;
                    if idx < self.cells.len() {
                        let mut gem = Gem::simple(kind);
                        gem.special = Some(special);
                        self.cells[idx] = Some(gem);
                        outcome.specials_created.push((cell, kind, special));
                    }
                }
            }

            outcome.clears_by_depth.push(depth_clears);

            // Refill empties (echo charges land on the fresh gems filling the
            // cleared cells — this is how future detonations are seeded).
            let mut pending = pending_echo_cells;
            pending.sort_unstable();
            pending.dedup();
            for i in 0..self.cells.len() {
                if self.cells[i].is_none() {
                    let mut gem = self.random_gem();
                    if pending.binary_search(&CellId(i as u32)).is_ok() {
                        if self.match_config.enable_echo {
                            gem.echo = Some(EchoCharge::with_duration(1 + self.echo_extra_moves));
                            fresh_charges.push(CellId(i as u32));
                        }
                    }
                    self.cells[i] = Some(gem);
                }
            }

            current_runs = self.find_matches();
        }

        // Decrement echo charges ONCE at the end of the move (not per cascade).
        // fresh_charges accumulates all cells seeded with echoes during this move.
        if self.match_config.enable_echo {
            for c in &fresh_charges {
                if let Some(gem) = self.cells.get_mut(c.0 as usize).and_then(|s| s.as_mut()) {
                    if let Some(echo) = &mut gem.echo {
                        echo.moves_left = echo.moves_left.saturating_sub(1);
                        if echo.moves_left == 0 {
                            gem.echo = None;
                        }
                    }
                }
            }
            fresh_charges.retain(|c| self.get(*c).and_then(|g| g.echo.as_ref()).is_some());
        }

        outcome.resonance_multiplier = self.resonance_multiplier;
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds() -> Vec<GemKind> {
        vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ]
    }

    fn set(board: &mut CubeBoard, face: usize, x: usize, y: usize, kind: GemKind) {
        let i = board.cell(face, x, y).0 as usize;
        board.cells[i] = Some(Gem::simple(kind));
    }

    #[test]
    fn cube_board_starts_match_free() {
        for seed in 0..40u64 {
            let board = CubeBoard::new(5, seed, kinds());
            assert!(
                board.find_matches().is_empty(),
                "seed {} generated a pre-existing match",
                seed
            );
        }
    }

    #[test]
    fn cube_board_fully_populated() {
        let board = CubeBoard::new(6, 9, kinds());
        assert_eq!(board.cells.len(), 6 * 36);
        assert!(board.cells.iter().all(|c| c.is_some()));
    }

    /// A swap completing a run across the Front->Right seam must clear cells
    /// on BOTH faces and leave the board fully refilled.
    #[test]
    fn cube_swap_resolves_cross_seam_run() {
        let n = 5;
        let mut board = CubeBoard::new(n, 11, kinds());

        // Row 0 of Front: C C T .. ; Right face (0,0): C. Swapping a C up from
        // (n-1,1) completes C-C-C-C spanning the seam.
        set(&mut board, 0, n - 3, 0, GemKind::Circle);
        set(&mut board, 0, n - 2, 0, GemKind::Circle);
        set(&mut board, 0, n - 1, 0, GemKind::Triangle);
        set(&mut board, 1, 0, 0, GemKind::Circle);
        set(&mut board, 0, n - 1, 1, GemKind::Circle);

        let outcome = board.try_swap(
            board.cell(0, n - 1, 0),
            board.cell(0, n - 1, 1),
        );
        let outcome = outcome.expect("swap must be accepted");
        assert!(outcome.cascades >= 1);

        let cleared: Vec<CellId> = outcome
            .clears_by_depth
            .iter()
            .flatten()
            .map(|(c, _)| *c)
            .collect();
        assert!(cleared.contains(&board.cell(1, 0, 0)), "cross-seam cell on Right face must clear");
        assert!(cleared.contains(&board.cell(0, n - 1, 0)), "swapped-in cell must clear");

        // Board refills to full occupancy.
        assert!(board.cells.iter().all(|c| c.is_some()));
    }

    /// An echoed gem participating in a match detonates: its orthogonal ring
    /// clears and the exact antipodal cell gains an echo charge.
    #[test]
    fn cube_echo_detonation_clears_ring_and_charges_antipode() {
        let n = 5;
        let mut board = CubeBoard::new(n, 21, kinds());

        set(&mut board, 0, n - 3, 0, GemKind::Circle);
        set(&mut board, 0, n - 2, 0, GemKind::Circle);
        set(&mut board, 0, n - 1, 0, GemKind::Triangle);
        set(&mut board, 1, 0, 0, GemKind::Circle);
        set(&mut board, 0, n - 1, 1, GemKind::Circle);

        // Give the swapped-in gem an echo charge.
        let swapper = board.cell(0, n - 1, 1);
        let idx = swapper.0 as usize;
        let mut gem = board.cells[idx].take().unwrap();
        gem.echo = Some(EchoCharge::new());
        board.cells[idx] = Some(gem);

        let outcome = board
            .try_swap(swapper, board.cell(0, n - 1, 0))
            .expect("swap must be accepted");
        assert!(!outcome.echoes_detonated.is_empty(), "echo must detonate");
        assert!(outcome.resonance_multiplier > 1.0);

        // Antipodal charge lands on the opposite face (Front -> Back = face 2).
        assert!(!outcome.antipodal_charged.is_empty());
        for anti in &outcome.antipodal_charged {
            assert_eq!(board.face_of(*anti), 2, "shockwave must strike the Back face");
            let cell_idx = anti.0 as usize;
            let gem = board.get(*anti).expect("antipodal cell occupied");
            assert!(gem.echo.is_some());
        }
    }

    /// Face rotation is a deterministic permutation and never loses gems.
    #[test]
    fn rotate_face_is_deterministic_permutation() {
        let n = 4;
        let mut board = CubeBoard::new(n, 31, kinds());
        let pattern = [
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
            GemKind::Circle,
            GemKind::Square,
            GemKind::Diamond,
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Diamond,
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
        ];
        for y in 0..n {
            for x in 0..n {
                set(&mut board, 0, x, y, pattern[y * n + x]);
            }
        }
        let before: Vec<_> = (0..board.cells.len()).collect();

        board.rotate_face(0, true).expect("rotation succeeds");

        // CW: new(x, y) = old(y, n-1-x)
        for y in 0..n {
            for x in 0..n {
                let got = board.get(board.cell(0, x, y)).unwrap().kind;
                assert_eq!(got, pattern[(n - 1 - x) * n + y], "at ({x},{y})");
            }
        }
        // Total occupancy unchanged.
        assert_eq!(board.cells.len(), before.len());
        assert!(board.cells.iter().all(|c| c.is_some()));
    }

    /// Blocked cells never participate in cube runs.
    #[test]
    fn cube_blocked_cell_breaks_run() {
        use crate::Blocker;
        let n = 4;
        let mut board = CubeBoard::new(n, 41, kinds());
        let row = |y| board.cell(0, 0, y);
        let _ = row;
        for x in 0..3 {
            set(&mut board, 0, x, 1, GemKind::Circle);
        }
        let mid = board.cell(0, 1, 1);
        let idx = mid.0 as usize;
        let mut gem = board.cells[idx].take().unwrap();
        gem.blocker = Some(Blocker::Ice { layers: 1 });
        board.cells[idx] = Some(gem);

        assert!(board.find_matches().is_empty(), "iced gem must break the run");
    }
}
