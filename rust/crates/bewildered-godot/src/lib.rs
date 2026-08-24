use godot::prelude::*;
use bewildered_core::{calculate_score, Board, GemKind, MoveOutcome, RuleModifiers, SpecialGem};
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
    // (row, col) -> hits remaining. Ice/Crate need multiple hits to clear.
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
            MoveOutcome::Success { matches, .. } => {
                // --- Authoritative objective accounting (Stage 6) ---
                if !self.cleared && !self.failed {
                    self.moves_used += 1;

                    let gained = {
                        let b = self.board.as_ref().unwrap();
                        calculate_score(&outcome, b.combo, &b.rule_modifiers) as i64
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
                // Build all signal payloads inside a scoped block so the
                // immutable board borrow ends before we emit (which needs &mut).
                let (match_signals, special_signals, echo_charged_signals, echo_detonated_cells, has_echo_detonation, resonance_mult) = {
                    let board_ref = self.board.as_ref().unwrap();
                    let mut match_signals = Vec::new();
                    let mut special_signals = Vec::new();
                    let mut echo_charged_signals = Vec::new();
                    let mut echo_detonated_cells = Array::new();
                    let mut has_echo_detonation = false;
                    let mut resonance_mult = 1.0f32;

                    for (cascade_idx, m) in matches.iter().enumerate() {
                        let mut cleared_cells = Array::new();
                        for &(row, col) in &m.cells {
                            cleared_cells.push(Vector2i::new(col as i32, row as i32));
                        }
                        match_signals.push((cleared_cells, m.kind as i32, cascade_idx as i32 + 1));

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
                                    echo_detonated_cells
                                        .push(Vector2i::new(col as i32, row as i32));
                                    has_echo_detonation = true;
                                }
                            }
                        }
                        if !echo_cells.is_empty() {
                            echo_charged_signals.push(echo_cells);
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
                        &[echo_detonated_cells.to_variant(), resonance_mult.to_variant()],
                    );
                }

                // Real objective progress, not a placeholder.
                let (cur, tgt) = self.objective_progress();
                self.base_mut().emit_signal(
                    "objective_progress",
                    &[cur.to_variant(), tgt.to_variant()],
                );

                true
            }
        }
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

struct BewilderedExtension;

#[gdextension]
unsafe impl ExtensionLibrary for BewilderedExtension {}