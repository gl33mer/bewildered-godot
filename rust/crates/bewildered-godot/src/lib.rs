use godot::prelude::*;
use bewildered_core::{Blocker, Board, CubeBoard, CubeOutcome, DescentRun, GemKind, MatchConfig, MoveOutcome, Relic, RuleModifiers, SpecialGem};
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
            let blocker = match gem.blocker {
                None => 0,
                Some(Blocker::Stone) => 1,
                Some(Blocker::Ice { .. }) => 2,
            };
            dict.set("blocker", blocker.to_variant());
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

/// Multi-face cube simulation for 3D Bewildered. A thin FFI shell over
/// [`bewildered_core::CubeBoard`] — all rules live in bewildered-core.
#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct CubeSim {
    cube: Option<CubeBoard>,
    face_size: i32,
    // Descent chamber accounting (Phase 6)
    chamber: i32,
    score: i64,
    moves_used: i32,
    moves_total: i32,
    score_target: i64,
    chamber_cleared: bool,
    chamber_failed: bool,
    relic_extra_moves: i32,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for CubeSim {
    fn init(_base: Base<RefCounted>) -> Self {
        Self {
            cube: None,
            face_size: 6,
            chamber: 1,
            score: 0,
            moves_used: 0,
            moves_total: 0,
            score_target: 0,
            chamber_cleared: false,
            chamber_failed: false,
            relic_extra_moves: 0,
            base: _base,
        }
    }
}

#[godot_api]
impl CubeSim {
    #[signal]
    fn cube_match_resolved(face: i32, cleared_cells: Array<Vector2i>, gem_kind: i32, cascade_depth: i32);

    #[signal]
    fn cube_special_gem_created(face: i32, pos: Vector2i, kind: i32);

    #[signal]
    fn cube_echo_detonated(face: i32, cells: Array<Vector2i>, multiplier: f32);

    #[signal]
    fn antipodal_echo_charged(target_face: i32, cells: Array<Vector2i>);

    #[signal]
    fn cube_move_rejected(face: i32, ax: i32, ay: i32, bx: i32, by: i32);

    #[signal]
    fn descent_chamber_finished(chamber: i32, cleared: bool);

    /// Create a match-free N x N x 6 cube board (deterministic seed).
    #[func]
    fn new_cube_board(&mut self, face_size: i32, seed: i64) {
        let n = face_size.max(2) as usize;
        self.face_size = face_size.max(2);
        self.cube = Some(CubeBoard::new(
            n,
            seed as u64,
            vec![
                GemKind::Circle,
                GemKind::Triangle,
                GemKind::Square,
                GemKind::Diamond,
            ],
        ));
    }

    /// Inspect a cell on a face: {empty, kind, has_echo, special, blocker}.
    /// special: 0=none, 1=Bolt, 2=Prism, 3=Nova. blocker: 0=none, 1=Stone, 2=Ice.
    #[func]
    fn get_face_cell(&self, face: i32, x: i32, y: i32) -> Dictionary {
        let mut dict = Dictionary::new();
        dict.set("empty", true.to_variant());
        let Some(cube) = &self.cube else {
            return dict;
        };
        if face < 0 || face >= 6 || x < 0 || y < 0 || x >= self.face_size || y >= self.face_size {
            return dict;
        }
        let cell = cube.cell(face as usize, x as usize, y as usize);
        match cube.get(cell) {
            Some(gem) => {
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
                let blocker = match gem.blocker {
                    None => 0,
                    Some(Blocker::Stone) => 1,
                    Some(Blocker::Ice { .. }) => 2,
                };
                dict.set("blocker", blocker.to_variant());
            }
            None => {}
        }
        dict
    }

    /// Swap two orthogonally adjacent cells on one face. Returns true when the
    /// swap was legal and resolved matches/cascades.
    #[func]
    fn try_face_swap(&mut self, face: i32, ax: i32, ay: i32, bx: i32, by: i32) -> bool {
        let Some(cube) = &mut self.cube else {
            return false;
        };
        if face < 0
            || face >= 6
            || ax < 0
            || ay < 0
            || bx < 0
            || by < 0
            || ax >= self.face_size
            || ay >= self.face_size
            || bx >= self.face_size
            || by >= self.face_size
        {
            self.emit_move_rejected(face, ax, ay, bx, by);
            return false;
        }
        let a = cube.cell(face as usize, ax as usize, ay as usize);
        let b = cube.cell(face as usize, bx as usize, by as usize);
        match cube.try_swap(a, b) {
            Some(outcome) => {
                self.emit_cube_outcome(&outcome);
                true
            }
            None => {
                self.emit_move_rejected(face, ax, ay, bx, by);
                false
            }
        }
    }

    /// Rotate one face's grid 90 degrees (the 3D Tumbler spin). Always
    /// succeeds for a valid face and resolves any resulting cascades.
    #[func]
    fn rotate_face_gravity(&mut self, face: i32, clockwise: bool) -> bool {
        let Some(cube) = &mut self.cube else {
            return false;
        };
        if face < 0 || face >= 6 {
            return false;
        }
        let outcome = cube.rotate_face(face as usize, clockwise);
        match outcome {
            Some(o) => {
                self.emit_cube_outcome(&o);
                true
            }
            None => false,
        }
    }

    /// Number of cells per face edge (N).
    #[func]
    fn get_face_size(&self) -> i32 {
        self.face_size
    }

    /// Whether a board exists (new_cube_board was called).
    #[func]
    fn is_ready(&self) -> bool {
        self.cube.is_some()
    }

    /// Set match mechanics config: enable/disable echo, antipodal, specials.
    /// Baseline mode (all false) = pure match-3/4/5 only.
    #[func]
    fn set_match_config(&mut self, enable_echo: bool, enable_antipodal: bool, enable_specials: bool, enable_descent: bool) {
        if let Some(cube) = &mut self.cube {
            cube.set_match_config(MatchConfig {
                enable_echo,
                enable_antipodal,
                enable_specials,
                enable_descent,
            });
        }
    }

    /// Preset: baseline (match-3/4/5 only, no echo/antipodal/specials, no descent).
    #[func]
    fn set_match_config_baseline(&mut self) {
        self.set_match_config(false, false, false, false);
    }

    /// Preset: vanilla (pure match-3/4/5 only, no special mechanics at all).
    #[func]
    fn set_match_config_vanilla(&mut self) {
        self.set_match_config(false, false, false, false);
    }

    /// Preset: full (all mechanics enabled including descent).
    #[func]
    fn set_match_config_full(&mut self) {
        self.set_match_config(true, true, true, true);
    }

    /// Get current match config for debugging.
    #[func]
    fn get_match_config(&self) -> Dictionary {
        if let Some(cube) = &self.cube {
            let mut dict = Dictionary::new();
            dict.set("enable_echo", cube.match_config.enable_echo);
            dict.set("enable_antipodal", cube.match_config.enable_antipodal);
            dict.set("enable_specials", cube.match_config.enable_specials);
            dict.set("enable_descent", cube.match_config.enable_descent);
            return dict;
        }
        Dictionary::new()
    }

    // --- Descent chamber lifecycle (Phase 6) ---

    /// Apply relic modifiers to this chamber's simulation.
    #[func]
    fn set_relic_modifiers(&mut self, echo_extra: i32, score_pct: f32, extra_moves: i32) {
        if let Some(cube) = &mut self.cube {
            cube.echo_extra_moves = echo_extra.max(0) as u8;
            cube.score_bonus_pct = score_pct;
            cube.extra_moves = extra_moves.max(0) as u8;
        }
        self.relic_extra_moves = extra_moves.max(0);
    }

    /// Start a descent chamber: fresh board, chamber-scaled score target,
    /// move pool = 18 + relic bonus + 2 per chamber beyond the first.
    #[func]
    fn start_chamber(&mut self, chamber: i32, seed: i64) {
        self.new_cube_board(self.face_size.max(2), seed);
        self.chamber = chamber;
        self.score = 0;
        self.moves_used = 0;
        self.score_target = 600 + 400 * (chamber - 1).max(0) as i64;
        self.moves_total = 18 + self.relic_extra_moves + 2 * (chamber - 1).max(0);
        self.chamber_cleared = false;
        self.chamber_failed = false;
    }

    #[func]
    fn get_chamber(&self) -> i32 {
        self.chamber
    }

    #[func]
    fn get_score(&self) -> i64 {
        self.score
    }

    #[func]
    fn get_score_target(&self) -> i64 {
        self.score_target
    }

    #[func]
    fn get_moves_remaining(&self) -> i32 {
        (self.moves_total - self.moves_used).max(0)
    }

    #[func]
    fn is_chamber_cleared(&self) -> bool {
        self.chamber_cleared
    }

    #[func]
    fn is_chamber_failed(&self) -> bool {
        self.chamber_failed
    }

    // --- signal emission helpers ---

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

    /// Convert a core outcome into presentation signals. Cells are grouped per
    /// face so each signal's coordinates are face-local.
    fn emit_cube_outcome(&mut self, outcome: &CubeOutcome) {
        // --- descent accounting ---
        if !self.chamber_cleared && !self.chamber_failed {
            self.moves_used += 1;
            let cleared_count: usize = outcome.clears_by_depth.iter().map(|d| d.len()).sum();
            let base = 10.0f32 * cleared_count as f32
                * 1.5f32.powi(outcome.cascades as i32)
                * outcome.resonance_multiplier;
            let bonus = 1.0 + self.cube.as_ref().map(|c| c.score_bonus_pct).unwrap_or(0.0);
            self.score += (base * bonus) as i64;
            if self.score >= self.score_target {
                self.chamber_cleared = true;
            } else if self.moves_used >= self.moves_total {
                self.chamber_failed = true;
            }
            if self.chamber_cleared || self.chamber_failed {
                let chamber = self.chamber;
                let cleared = self.chamber_cleared;
                self.base_mut().emit_signal(
                    "descent_chamber_finished",
                    &[chamber.to_variant(), cleared.to_variant()],
                );
            }
        }

        // One match_resolved per (cascade depth, face) group.
        for (depth, depth_cells) in outcome.clears_by_depth.iter().enumerate() {
            let mut groups: std::collections::BTreeMap<i32, (Array<Vector2i>, i32)> =
                std::collections::BTreeMap::new();
            for (cell, kind) in depth_cells {
                let (f, x, y) = {
                    let cube = self.cube.as_ref().unwrap();
                    cube.coords(*cell)
                };
                let entry = groups.entry(f as i32).or_insert_with(|| (Array::new(), *kind as i32));
                entry.0.push(Vector2i::new(x as i32, y as i32));
            }
            for (face, (cells, kind)) in groups {
                self.base_mut().emit_signal(
                    "cube_match_resolved",
                    &[
                        face.to_variant(),
                        cells.to_variant(),
                        kind.to_variant(),
                        (depth as i32 + 1).to_variant(),
                    ],
                );
            }
        }

        // Special gems persisted this move.
        for (cell, kind, special) in &outcome.specials_created {
            let (f, x, y) = {
                let cube = self.cube.as_ref().unwrap();
                cube.coords(*cell)
            };
            let kind_id = match special {
                SpecialGem::Bolt { .. } => 1,
                SpecialGem::Prism => 2,
                SpecialGem::Nova => 3,
            };
            let _ = kind;
            self.base_mut().emit_signal(
                "cube_special_gem_created",
                &[
                    (f as i32).to_variant(),
                    Vector2i::new(x as i32, y as i32).to_variant(),
                    kind_id.to_variant(),
                ],
            );
        }

        // Echo detonations grouped by origin face.
        if !outcome.echoes_detonated.is_empty() {
            let mut groups: std::collections::BTreeMap<i32, Array<Vector2i>> =
                std::collections::BTreeMap::new();
            for cell in &outcome.echoes_detonated {
                let (f, x, y) = {
                    let cube = self.cube.as_ref().unwrap();
                    cube.coords(*cell)
                };
                groups
                    .entry(f as i32)
                    .or_insert_with(Array::new)
                    .push(Vector2i::new(x as i32, y as i32));
            }
            for (face, cells) in groups {
                self.base_mut().emit_signal(
                    "cube_echo_detonated",
                    &[
                        face.to_variant(),
                        cells.to_variant(),
                        outcome.resonance_multiplier.to_variant(),
                    ],
                );
            }
        }

        // Antipodal shockwave strikes grouped by target face.
        if !outcome.antipodal_charged.is_empty() {
            let mut groups: std::collections::BTreeMap<i32, Array<Vector2i>> =
                std::collections::BTreeMap::new();
            for cell in &outcome.antipodal_charged {
                let (f, x, y) = {
                    let cube = self.cube.as_ref().unwrap();
                    cube.coords(*cell)
                };
                groups
                    .entry(f as i32)
                    .or_insert_with(Array::new)
                    .push(Vector2i::new(x as i32, y as i32));
            }
            for (target_face, cells) in groups {
                self.base_mut().emit_signal(
                    "antipodal_echo_charged",
                    &[target_face.to_variant(), cells.to_variant()],
                );
            }
        }
    }
}

/// Roguelike Descent run orchestrator: chamber progression and the
/// between-chamber 3-relic draft. Thin shell over bewildered_core::DescentRun.
#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct DescentRunner {
    run: Option<DescentRun>,
    pending_offers: Vec<Relic>,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for DescentRunner {
    fn init(_base: Base<RefCounted>) -> Self {
        Self {
            run: None,
            pending_offers: Vec::new(),
            base: _base,
        }
    }
}

#[godot_api]
impl DescentRunner {
    #[signal]
    fn draft_ready(offers: Array<Dictionary>);

    /// Begin a fresh descent (chamber 1, no relics).
    #[func]
    fn start_run(&mut self, seed: i64) {
        self.run = Some(DescentRun::new(seed as u64));
        self.pending_offers.clear();
    }

    #[func]
    fn is_running(&self) -> bool {
        self.run.is_some()
    }

    #[func]
    fn get_chamber(&self) -> i32 {
        self.run.as_ref().map(|r| r.chamber as i32).unwrap_or(0)
    }

    #[func]
    fn get_chamber_seed(&self) -> i64 {
        self.run
            .as_ref()
            .map(|r| r.chamber_seed() as i64)
            .unwrap_or(0)
    }

    /// Roll the 3-relic draft for the current transition and return the
    /// offers as Array of {id, name, description, rarity}.
    #[func]
    fn next_draft(&mut self) -> Array<Dictionary> {
        let mut arr = Array::new();
        let Some(run) = &mut self.run else {
            return arr;
        };
        self.pending_offers = run.draft_offers();
        for relic in &self.pending_offers {
            let mut d = Dictionary::new();
            d.set("id", relic.id.to_variant());
            d.set("name", relic.name.to_variant());
            d.set("description", relic.description.to_variant());
            d.set("rarity", relic.rarity.as_str().to_variant());
            arr.push(&d);
        }
        self.base_mut().emit_signal("draft_ready", &[arr.to_variant()]);
        arr
    }

    /// Choose one of the pending offers by id. Merges its modifiers.
    /// Returns false if the id is not among the pending offers.
    #[func]
    fn choose_relic(&mut self, id: GString) -> bool {
        let id_s = id.to_string();
        let Some(relic) = self
            .pending_offers
            .iter()
            .find(|r| r.id == id_s)
            .cloned()
        else {
            return false;
        };
        if let Some(run) = &mut self.run {
            run.pick_relic(&relic);
        }
        self.pending_offers.clear();
        true
    }

    /// Advance to the next chamber.
    #[func]
    fn advance_chamber(&mut self) {
        if let Some(run) = &mut self.run {
            run.advance_chamber();
        }
    }

    // Merged relic modifier getters (for wiring into CubeSim).

    #[func]
    fn get_extra_moves(&self) -> i32 {
        self.run
            .as_ref()
            .map(|r| r.modifiers.extra_moves as i32)
            .unwrap_or(0)
    }

    #[func]
    fn get_echo_extra(&self) -> i32 {
        self.run
            .as_ref()
            .map(|r| r.modifiers.echo_extra_moves as i32)
            .unwrap_or(0)
    }

    #[func]
    fn get_score_pct(&self) -> f32 {
        self.run
            .as_ref()
            .map(|r| r.modifiers.score_bonus_pct)
            .unwrap_or(0.0)
    }

    /// Number of relics held.
    #[func]
    fn get_relic_count(&self) -> i32 {
        self.run.as_ref().map(|r| r.relics.len() as i32).unwrap_or(0)
    }

    /// Held relic summaries (id, name, description, rarity) for the HUD tray.
    #[func]
    fn get_held_relics(&self) -> Array<Dictionary> {
        let mut arr = Array::new();
        if let Some(run) = &self.run {
            for relic in &run.relics {
                let mut d = Dictionary::new();
                d.set("id", relic.id.to_variant());
                d.set("name", relic.name.to_variant());
                d.set("description", relic.description.to_variant());
                d.set("rarity", relic.rarity.as_str().to_variant());
                arr.push(&d);
            }
        }
        arr
    }
}

struct BewilderedExtension;

#[gdextension]
unsafe impl ExtensionLibrary for BewilderedExtension {}
