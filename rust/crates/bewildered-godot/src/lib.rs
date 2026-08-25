use godot::prelude::*;
use bewildered_core::{
    Board, CellId, Cube6Face, Direction, Flat2D, GemKind, MoveOutcome, RuleModifiers, SpecialGem,
    Topology,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand::Rng;
use bewildered_content::{BlockerKind, Level, Objective};
use std::collections::HashMap;

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
pub struct BoardSim {
    board: Option<Board>,
    level: Option<Level>,
    error_message: String,

    // Authoritative level / objective state (Stage 6).
    moves_used: i32,
    moves_total: i32,
    score: i64,
    collected: i64,
    collection_target: i32,
    blockers_remaining: i64,
    blocker_target: i32,
    blocker_hits: HashMap<(usize, usize), i32>,
    cleared: bool,
    failed: bool,

    base: Base<RefCounted>,
}

#[godot_api]
impl BoardSim {
    #[signal]
    fn match_resolved(cleared_cells: Array<Vector2i>, gem_kind: i32, cascade_depth: i32);

    #[signal]
    fn special_gem_created(pos: Vector2i, kind: i32);

    #[signal]
    fn echo_charged(cells: Array<Vector2i>);

    #[signal]
    fn echo_detonated(cells: Array<Vector2i>, multiplier: f32);

    #[signal]
    fn move_rejected(ax: i32, ay: i32, bx: i32, by: i32);

    #[signal]
    fn objective_progress(current: i64, target: i64);

    // --- Constructors / level loading ---

    #[func]
    fn new_board(&mut self, width: i32, height: i32, seed: i64) {
        let gem_types = vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ];
        self.board = Some(Board::new(
            width as usize,
            height as usize,
            seed as u64,
            gem_types,
        ));
        self.level = None;
    }

    /// Load a level definition from a RON file on disk (absolute/OS path; the
    /// Godot side globalizes `res://` before calling).
    #[func]
    fn load_level_file(&mut self, path: GString) -> bool {
        let p = path.to_string();
        match Level::load_ron(&p) {
            Ok(level) => {
                self.setup_level(level);
                true
            }
            Err(e) => {
                self.error_message = format!("Failed to load level: {e}");
                godot_print!("[bewildered] {e}");
                false
            }
        }
    }

    /// Load a level definition from a raw RON string (handy for testing and
    /// for in-memory packs without touching the filesystem).
    #[func]
    fn load_level_from_ron(&mut self, content: GString) -> bool {
        let s = content.to_string();
        match ron::from_str::<Level>(&s) {
            Ok(level) => {
                self.setup_level(level);
                true
            }
            Err(e) => {
                self.error_message = format!("Failed to parse level: {e}");
                godot_print!("[bewildered] {e}");
                false
            }
        }
    }

    fn setup_level(&mut self, level: Level) {
        let gem_types = level.gem_types.clone();
        let seed = level.seed_override.unwrap_or(1);

        let mut board = Board::with_rules(
            level.grid.width,
            level.grid.height,
            seed,
            gem_types,
            RuleModifiers::new(),
        );

        // Apply blockers (remove the starting gem at each blocker position).
        self.blocker_hits.clear();
        for b in &level.blockers {
            let (col, row) = b.pos;
            board.remove_gem(row, col);
            let hits = match b.kind {
                BlockerKind::Ice { hits } => hits as i32,
                BlockerKind::Crate { hits } => hits as i32,
            };
            self.blocker_hits.insert((row, col), hits);
        }

        let (moves_total, collection_target, blocker_target) = match &level.objective {
            Objective::ScoreTarget { max_moves, .. } => (*max_moves as i32, 0, 0),
            Objective::Collection { count, .. } => (20, *count as i32, 0),
            Objective::Descent {
                blockers_to_clear,
            } => (20, 0, *blockers_to_clear as i32),
            Objective::Survival { max_moves } => (*max_moves as i32, 0, 0),
        };

        self.level = Some(level);
        self.board = Some(board);
        self.error_message = String::new();
        self.moves_used = 0;
        self.moves_total = moves_total;
        self.score = 0;
        self.collected = 0;
        self.collection_target = collection_target;
        self.blocker_target = blocker_target;
        self.blockers_remaining = blocker_target as i64;
        self.cleared = false;
        self.failed = false;
    }

    // --- Core move ---

    #[func]
    fn try_swap(&mut self, ax: i32, ay: i32, bx: i32, by: i32) -> bool {
        let (w, h) = match &self.board {
            Some(b) => (b.width as i32, b.height as i32),
            None => (0, 0),
        };
        let out_of_bounds =
            ax < 0 || ay < 0 || bx < 0 || by < 0 || ax >= w || ay >= h || bx >= w || by >= h;
        let non_adjacent = (ax - bx).abs() + (ay - by).abs() != 1;
        if out_of_bounds || non_adjacent {
            self.base_mut().emit_signal(
                "move_rejected",
                &[
                    ax.to_variant(),
                    ay.to_variant(),
                    bx.to_variant(),
                    by.to_variant(),
                ],
            );
            return false;
        }

        let outcome = {
            let board = self.board.as_mut().unwrap();
            board.try_swap(ay as usize, ax as usize, by as usize, bx as usize)
        };

        match &outcome {
            MoveOutcome::Illegal | MoveOutcome::NoMatch => {
                self.base_mut().emit_signal(
                    "move_rejected",
                    &[
                        ax.to_variant(),
                        ay.to_variant(),
                        bx.to_variant(),
                        by.to_variant(),
                    ],
                );
                false
            }
            MoveOutcome::Success { .. } => self.handle_success_move(&outcome),
        }
    }

    /// Rotate the board grid 90° (clockwise or counter-clockwise): the grid data
    /// truly transposes (width/height swap) with gravity always Down, gems fall
    /// to the new bottom row + cascade. Always succeeds and costs a move.
    #[func]
    fn rotate_board(&mut self, clockwise: bool) -> bool {
        let outcome = {
            let board = self.board.as_mut().unwrap();
            board.rotate_board(clockwise)
        };

        match &outcome {
            MoveOutcome::Success { .. } => self.handle_success_move(&outcome),
            _ => false,
        }
    }

    /// Shared accounting + signal emission for a successful move (swap or
    /// rotation). Returns true. Mutates objective state and emits the cascade,
    /// special, echo, and objective signals Godot uses for presentation.
    fn handle_success_move(&mut self, outcome: &MoveOutcome) -> bool {
        // --- Authoritative objective accounting (Stage 6) ---
        if !self.cleared && !self.failed {
            self.moves_used += 1;

            let gained = {
                let b = self.board.as_ref().unwrap();
                bewildered_core::calculate_score(outcome, b.combo, &b.rule_modifiers) as i64
            };
            self.score += gained;

            // Collection: count target-gem clears across all cascades.
            let collection_tg = match &self.level {
                Some(l) => match &l.objective {
                    Objective::Collection { target_gem, .. } => Some(*target_gem),
                    _ => None,
                },
                None => None,
            };
            if let Some(tg) = collection_tg {
                let cleared = self
                    .board
                    .as_ref()
                    .map(|b| b.cleared_this_move.clone())
                    .unwrap_or_default();
                for &(_r, _c, kind) in &cleared {
                    if kind == tg {
                        self.collected += 1;
                    }
                }
            }

            // Descent: chip blocker hits at cleared cells.
            if self.blocker_target > 0 {
                let cleared = self
                    .board
                    .as_ref()
                    .map(|b| b.cleared_this_move.clone())
                    .unwrap_or_default();
                for &(r, c, _kind) in &cleared {
                    if let Some(hits) = self.blocker_hits.get_mut(&(r, c)) {
                        *hits -= 1;
                        if *hits <= 0 {
                            self.blocker_hits.remove(&(r, c));
                            self.blockers_remaining =
                                self.blockers_remaining.saturating_sub(1);
                        }
                    }
                }
            }

            self.update_level_state();
        }

        // --- Emit cascade signals for Godot presentation ---
        let (match_signals, special_signals, echo_charged_signals, echo_detonated_cells, has_echo_detonation, resonance_mult) = {
            let board_ref = self.board.as_ref().unwrap();
            let mut match_signals = Vec::new();
            let mut special_signals = Vec::new();
            let mut echo_charged_signals = Vec::new();
            let mut echo_detonated_cells = Array::new();
            let mut has_echo_detonation = false;
            let mut resonance_mult = 1.0f32;

            // One match_resolved per cascade depth, so Godot can animate each
            // pop/fall with readable pacing instead of clearing in one frame.
            if let MoveOutcome::Success { clears_by_depth, .. } = outcome {
                for (cascade_idx, depth_cells) in clears_by_depth.iter().enumerate() {
                    let mut cleared_cells = Array::new();
                    let mut kind = 0i32;
                    for &(row, col, gk) in depth_cells {
                        cleared_cells.push(Vector2i::new(col as i32, row as i32));
                        kind = gk as i32;
                    }
                    if !cleared_cells.is_empty() {
                        match_signals
                            .push((cleared_cells, kind, cascade_idx as i32 + 1));
                    }
                }
            }

            // Special gem creation + echo detection over the initial match set.
            if let MoveOutcome::Success { matches, .. } = outcome {
                for m in matches {
                    if m.is_special {
                        if let Some(special) = m.special_type {
                            let special_kind = match special {
                                SpecialGem::Bolt { .. } => 0,
                                SpecialGem::Prism => 1,
                                SpecialGem::Nova => 2,
                            };
                            let center_idx = m.cells.len() / 2;
                            let (center_row, center_col) = m.cells[center_idx];
                            special_signals.push((
                                Vector2i::new(center_col as i32, center_row as i32),
                                special_kind,
                            ));
                        }
                    }

                    let mut echo_cells = Array::new();
                    for &(row, col) in &m.cells {
                        if let Some(gem) = board_ref.gem(row, col) {
                            if gem.echo.is_some() {
                                echo_cells.push(Vector2i::new(col as i32, row as i32));
                                echo_detonated_cells.push(Vector2i::new(col as i32, row as i32));
                                has_echo_detonation = true;
                            }
                        }
                    }
                    if !echo_cells.is_empty() {
                        echo_charged_signals.push(echo_cells);
                    }
                }
            }

            if has_echo_detonation {
                resonance_mult = board_ref.resonance_multiplier;
            }

            (
                match_signals,
                special_signals,
                echo_charged_signals,
                echo_detonated_cells,
                has_echo_detonation,
                resonance_mult,
            )
        };

        for (cells, kind, cascade) in match_signals {
            self.base_mut().emit_signal(
                "match_resolved",
                &[cells.to_variant(), kind.to_variant(), cascade.to_variant()],
            );
        }
        for (pos, kind) in special_signals {
            self.base_mut().emit_signal(
                "special_gem_created",
                &[pos.to_variant(), kind.to_variant()],
            );
        }
        for cells in echo_charged_signals {
            self.base_mut().emit_signal("echo_charged", &[cells.to_variant()]);
        }
        if has_echo_detonation && !echo_detonated_cells.is_empty() {
            self.base_mut().emit_signal(
                "echo_detonated",
                &[
                    echo_detonated_cells.to_variant(),
                    resonance_mult.to_variant(),
                ],
            );
        }

        // Real objective progress.
        let (cur, tgt) = self.objective_progress();
        self.base_mut().emit_signal(
            "objective_progress",
            &[cur.to_variant(), tgt.to_variant()],
        );

        true
    }

    fn objective_progress(&self) -> (i64, i64) {
        match &self.level {
            Some(l) => match &l.objective {
                Objective::ScoreTarget { points, .. } => (self.score, *points as i64),
                Objective::Collection { count, .. } => (self.collected, *count as i64),
                Objective::Descent {
                    blockers_to_clear,
                } => {
                    let cleared = (self.blocker_target - self.blockers_remaining as i32).max(0);
                    (cleared as i64, *blockers_to_clear as i64)
                }
                Objective::Survival { max_moves } => (self.moves_used as i64, *max_moves as i64),
            },
            None => (0, 0),
        }
    }

    fn update_level_state(&mut self) {
        if self.cleared || self.failed {
            return;
        }
        let Some(level) = &self.level else { return };
        match &level.objective {
            Objective::ScoreTarget { points, .. } => {
                if self.score >= *points as i64 {
                    self.cleared = true;
                } else if self.moves_used >= self.moves_total {
                    self.failed = true;
                }
            }
            Objective::Collection { count, .. } => {
                if self.collected >= *count as i64 {
                    self.cleared = true;
                } else if self.moves_used >= self.moves_total {
                    self.failed = true;
                }
            }
            Objective::Descent { .. } => {
                if self.blockers_remaining <= 0 {
                    self.cleared = true;
                } else if self.moves_used >= self.moves_total {
                    self.failed = true;
                }
            }
            Objective::Survival { max_moves } => {
                if self.moves_used >= *max_moves as i32 {
                    self.cleared = true;
                } else if let Some(board) = &self.board {
                    if !board.has_legal_moves() {
                        self.failed = true;
                    }
                }
            }
        }
    }

    // --- Cell inspection ---

    #[func]
    fn get_cell(&self, x: i32, y: i32) -> Dictionary {
        let mut dict = Dictionary::new();
        let Some(board) = &self.board else {
            dict.set("empty", true.to_variant());
            return dict;
        };

        if x < 0 || y < 0 || x >= board.width as i32 || y >= board.height as i32 {
            dict.set("empty", true.to_variant());
            return dict;
        }

        if let Some(gem) = board.gem(y as usize, x as usize) {
            dict.set("empty", false.to_variant());
            dict.set("kind", (gem.kind as i32).to_variant());
            dict.set("has_echo", gem.echo.is_some().to_variant());
            if let Some(echo) = &gem.echo {
                dict.set("echo_moves_left", echo.moves_left.to_variant());
            }
            let special = match gem.special {
                None => 0,
                Some(SpecialGem::Bolt { .. }) => 1,
                Some(SpecialGem::Prism) => 2,
                Some(SpecialGem::Nova) => 3,
            };
            dict.set("special", special.to_variant());
        } else {
            dict.set("empty", true.to_variant());
        }
        dict
    }

    // --- Board getters ---

    #[func]
    fn get_width(&self) -> i32 {
        self.board.as_ref().map(|b| b.width as i32).unwrap_or(0)
    }

    #[func]
    fn get_height(&self) -> i32 {
        self.board.as_ref().map(|b| b.height as i32).unwrap_or(0)
    }

    #[func]
    fn get_combo(&self) -> i32 {
        self.board.as_ref().map(|b| b.combo as i32).unwrap_or(0)
    }

    #[func]
    fn get_resonance_multiplier(&self) -> f32 {
        self.board
            .as_ref()
            .map(|b| b.resonance_multiplier)
            .unwrap_or(1.0)
    }

    // --- Level / objective getters (Stage 6) ---

    #[func]
    fn get_level_id(&self) -> String {
        self.level.as_ref().map(|l| l.id.clone()).unwrap_or_default()
    }

    #[func]
    fn get_level_title(&self) -> String {
        self.level
            .as_ref()
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }

    #[func]
    fn get_moves_remaining(&self) -> i32 {
        (self.moves_total - self.moves_used).max(0)
    }

    #[func]
    fn get_target_score(&self) -> i64 {
        match &self.level {
            Some(l) => match &l.objective {
                Objective::ScoreTarget { points, .. } => *points as i64,
                Objective::Collection { count, .. } => *count as i64,
                Objective::Descent {
                    blockers_to_clear,
                } => *blockers_to_clear as i64,
                Objective::Survival { max_moves } => *max_moves as i64,
            },
            None => 0,
        }
    }

    #[func]
    fn get_objective_progress(&self) -> i64 {
        self.objective_progress().0
    }

    #[func]
    fn get_score(&self) -> i64 {
        self.score
    }

    #[func]
    fn get_objective_description(&self) -> String {
        match &self.level {
            Some(l) => match &l.objective {
                Objective::ScoreTarget { points, max_moves } => {
                    format!("Score {} points — {} moves", points, max_moves)
                }
                Objective::Collection {
                    target_gem,
                    count,
                } => format!("Collect {} {}", count, target_gem),
                Objective::Descent {
                    blockers_to_clear,
                } => format!("Clear {} blockers", blockers_to_clear),
                Objective::Survival { max_moves } => format!("Survive {} moves", max_moves),
            },
            None => "No level loaded".into(),
        }
    }

    #[func]
    fn is_level_cleared(&self) -> bool {
        self.cleared
    }

    #[func]
    fn is_level_failed(&self) -> bool {
        self.failed
    }

    #[func]
    fn get_last_error(&self) -> String {
        self.error_message.clone()
    }
}

/// Multi-face cube simulation for the 3D Bewildered mode.
#[derive(GodotClass)]
#[class(base=RefCounted, init)]
pub struct CubeSim {
    board: Option<Board>,
    topology: Cube6Face,
    face_size: i32,
    error_message: String,
    base: Base<RefCounted>,
}

#[godot_api]
impl CubeSim {
    #[signal]
    fn cube_match_resolved(face: i32, cleared_cells: Array<Vector2i>, gem_kind: i32, cascade_depth: i32);

    #[signal]
    fn cube_special_gem_created(face: i32, pos: Vector2i, kind: i32);

    #[signal]
    fn cube_echo_charged(face: i32, cells: Array<Vector2i>);

    #[signal]
    fn cube_echo_detonated(face: i32, cells: Array<Vector2i>, multiplier: f32);

    #[signal]
    fn antipodal_echo_charged(target_face: i32, cells: Array<Vector2i>);

    #[signal]
    fn cube_move_rejected(face: i32, ax: i32, ay: i32, bx: i32, by: i32);

    #[signal]
    fn cube_objective_progress(current: i64, target: i64);

    /// Create a new cube board with the given face size (N x N per face) and seed.
    /// All 6 faces share the same seed for deterministic generation.
    #[func]
    fn new_cube_board(&mut self, face_size: i32, seed: i64) {
        self.face_size = face_size;
        self.topology = Cube6Face::new(face_size as usize);
        let total_cells = self.topology.cell_count();

        let gem_types = vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
        ];

        // Generate a match-free board using the topology
        let mut gems = Vec::with_capacity(total_cells);
        for _ in 0..total_cells {
            gems.push(None);
        }

        // Fill with match-free gems using topology-aware generation
        // For now, use a simple approach: fill each face independently
        let per_face = (face_size * face_size) as usize;
        for face in 0..6 {
            let mut rng = StdRng::seed_from_u64(
                seed as u64 + face as u64 * 10000,
            );
            for row in 0..face_size as usize {
                for col in 0..face_size as usize {
                    let mut kind = gem_types[rng.gen_range(0..gem_types.len())];
                    for _ in 0..20 {
                        let candidate = gem_types[rng.gen_range(0..gem_types.len())];
                        let idx = self.face_index(face, col as i32, row as i32);
                        let horiz = col >= 2
                            && gems[idx - 1].as_ref().map(|g: &bewildered_core::Gem| g.kind) == Some(candidate)
                            && gems[idx - 2].as_ref().map(|g: &bewildered_core::Gem| g.kind) == Some(candidate);
                        let vert = row >= 2
                            && gems[idx - per_face].as_ref().map(|g: &bewildered_core::Gem| g.kind) == Some(candidate)
                            && gems[idx - 2 * per_face].as_ref().map(|g: &bewildered_core::Gem| g.kind) == Some(candidate);
                        if !horiz && !vert {
                            kind = candidate;
                            break;
                        }
                    }
                    let idx = self.face_index(face, col as i32, row as i32);
                    gems[idx] = Some(bewildered_core::Gem {
                        kind,
                        echo: None,
                        special: None,
                        blocker: None,
                    });
                }
            }
        }

        self.board = Some(Board {
            gems,
            width: face_size as usize * 6, // Logical width for Board (not used for cube)
            height: face_size as usize,
            combo: 0,
            gem_types,
            rng: StdRng::seed_from_u64(seed as u64),
            resonance_multiplier: 1.0,
            resonance_stack: 0,
            rule_modifiers: RuleModifiers::default(),
            cleared_this_move: Vec::new(),
            gravity: Direction::Down,
        });
    }

    /// Get the cell at (face, x, y). Face in 0..5, x,y in 0..face_size-1.
    #[func]
    fn get_face_cell(&self, face: i32, x: i32, y: i32) -> Dictionary {
        let mut dict = Dictionary::new();
        let Some(board) = &self.board else {
            dict.set("empty", true.to_variant());
            return dict;
        };

        if face < 0 || face >= 6 || x < 0 || y < 0 || x >= self.face_size || y >= self.face_size {
            dict.set("empty", true.to_variant());
            return dict;
        }

        let idx = self.face_index(face, x, y);
        if let Some(gem) = board.gems.get(idx).and_then(|g| g.as_ref()) {
            dict.set("empty", false.to_variant());
            dict.set("kind", (gem.kind as i32).to_variant());
            dict.set("has_echo", gem.echo.is_some().to_variant());
            if let Some(echo) = &gem.echo {
                dict.set("echo_moves_left", echo.moves_left.to_variant());
            }
            let special = match gem.special {
                None => 0,
                Some(SpecialGem::Bolt { .. }) => 1,
                Some(SpecialGem::Prism) => 2,
                Some(SpecialGem::Nova) => 3,
            };
            dict.set("special", special.to_variant());
        } else {
            dict.set("empty", true.to_variant());
        }
        dict
    }

    /// Attempt a swap between two adjacent cells on the same face.
    /// Returns true if the swap was legal and produced matches.
    #[func]
    fn try_face_swap(&mut self, face: i32, ax: i32, ay: i32, bx: i32, by: i32) -> bool {
        if face < 0 || face >= 6 {
            return false;
        }
        if ax < 0 || ay < 0 || bx < 0 || by < 0 || ax >= self.face_size || ay >= self.face_size || bx >= self.face_size || by >= self.face_size {
            self.emit_move_rejected(face, ax, ay, bx, by);
            return false;
        }
        if (ax - bx).abs() + (ay - by).abs() != 1 {
            self.emit_move_rejected(face, ax, ay, bx, by);
            return false;
        }

        // Compute indices before mutable borrow
        let fs = self.face_size as usize;
        let idx_a = face as usize * fs * fs + ay as usize * fs + ax as usize;
        let idx_b = face as usize * fs * fs + by as usize * fs + bx as usize;

        let board = self.board.as_mut().unwrap();

        if board.gems[idx_a].is_none() || board.gems[idx_b].is_none() {
            self.emit_move_rejected(face, ax, ay, bx, by);
            return false;
        }

        // Simulate swap
        board.gems.swap(idx_a, idx_b);

        // Check for matches using topology-aware find_line_runs
        let initial_matches = Self::find_all_matches_cube(&self.topology, board);

        // Check if the swap caused a match involving the swapped cells
        // Collect all global indices from the matches
        let mut matched_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let fs = self.face_size as usize;
        for m in &initial_matches {
            for (row, col) in &m.cells {
                let idx = (face as usize) * fs * fs + (*row as usize) * fs + (*col as usize);
                matched_indices.insert(idx);
            }
        }
        let swap_caused = matched_indices.contains(&idx_a) || matched_indices.contains(&idx_b)
            || initial_matches.iter().any(|m| m.is_special);

        if initial_matches.is_empty() || !swap_caused {
            board.gems.swap(idx_a, idx_b);
            self.emit_move_rejected(face, ax, ay, bx, by);
            return false;
        }

        // Process matches
        let outcome = MoveOutcome::Success {
            matches: initial_matches,
            cascades: 1,
            resonance_multiplier: 1.0,
            clears_by_depth: vec![],
        };

        self.handle_cube_success_move(&outcome);
        true
    }

    /// Rotate gravity on the active face 90° (clockwise or counter-clockwise).
    /// The face's grid transposes with gravity always Down in the new orientation.
    #[func]
    fn rotate_face_gravity(&mut self, face: i32, clockwise: bool) -> bool {
        if face < 0 || face >= 6 {
            return false;
        }
        // Delegate to Board's rotate_board for now
        let outcome = {
            let board = self.board.as_mut().unwrap();
            board.rotate_board(clockwise)
        };
        matches!(&outcome, MoveOutcome::Success { .. })
    }

    fn face_index(&self, face: i32, x: i32, y: i32) -> usize {
        let fs = self.face_size as usize;
        (face as usize * fs * fs) + (y as usize * fs) + x as usize
    }

    fn emit_move_rejected(&mut self, face: i32, ax: i32, ay: i32, bx: i32, by: i32) {
        self.base_mut().emit_signal(
            "cube_move_rejected",
            &[
                face.to_variant(),
                ax.to_variant(),
                ay.to_variant(),
                bx.to_variant(),
                by.to_variant(),
            ],
        );
    }

    fn find_all_matches_cube(topology: &Cube6Face, board: &Board) -> Vec<bewildered_core::Match> {
        let gems: Vec<Option<u8>> = board
            .gems
            .iter()
            .map(|g| g.as_ref().map(|gem| gem.kind as u8))
            .collect();
        bewildered_core::find_line_runs(topology, &gems, 3)
            .into_iter()
            .map(|(cells, kind)| bewildered_core::Match {
                cells: cells.iter().map(|c| {
                    let (_, x, y) = topology.coords(*c);
                    (x as usize, y as usize)
                }).collect(),
                kind: match GemKind::try_from(kind as u8) {
                    Ok(gk) => gk,
                    Err(_) => GemKind::Circle,
                },
                is_special: false,
                special_type: None,
            })
            .collect()
    }

    fn handle_cube_success_move(&mut self, outcome: &MoveOutcome) {
        if let MoveOutcome::Success { matches, .. } = outcome {
            // Emit match signals with face indices
            for m in matches {
                let mut cleared_cells = Array::new();
                let mut face_for_cells = 0;
                for &(row, col) in &m.cells {
                    cleared_cells.push(Vector2i::new(col as i32, row as i32));
                    if let Some(&(first_row, first_col)) = m.cells.first() {
                        let cell_id = CellId(self.face_index(face_for_cells as i32, first_col as i32, first_row as i32) as u32);
                        let (f, _, _) = self.topology.coords(cell_id);
                        face_for_cells = f as i32;
                    }
                }
                if !cleared_cells.is_empty() {
                    self.base_mut().emit_signal(
                        "cube_match_resolved",
                        &[
                            face_for_cells.to_variant(),
                            cleared_cells.to_variant(),
                            (m.kind as i32).to_variant(),
                            1i32.to_variant(),
                        ],
                    );
                }
            }
        }
    }
}

struct BewilderedExtension;

#[gdextension]
unsafe impl ExtensionLibrary for BewilderedExtension {}
