use godot::prelude::*;
use bewildered_core::{Board, GemKind, MoveOutcome, SpecialGem};

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
pub struct BoardSim {
    board: Option<Board>,
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
    }

    #[func]
    fn try_swap(&mut self, ax: i32, ay: i32, bx: i32, by: i32) -> bool {
        let Some(board) = &mut self.board else {
            return false;
        };

        // Check bounds
        if ax < 0 || ay < 0 || bx < 0 || by < 0 {
            self.base_mut().emit_signal("move_rejected", &[ax.to_variant(), ay.to_variant(), bx.to_variant(), by.to_variant()]);
            return false;
        }
        if ax >= board.width as i32 || ay >= board.height as i32 || bx >= board.width as i32 || by >= board.height as i32 {
            self.base_mut().emit_signal("move_rejected", &[ax.to_variant(), ay.to_variant(), bx.to_variant(), by.to_variant()]);
            return false;
        }

        let outcome = board.try_swap(ay as usize, ax as usize, by as usize, bx as usize);

        match outcome {
            MoveOutcome::Illegal => {
                self.base_mut().emit_signal("move_rejected", &[ax.to_variant(), ay.to_variant(), bx.to_variant(), by.to_variant()]);
                false
            }
            MoveOutcome::NoMatch => {
                self.base_mut().emit_signal("move_rejected", &[ax.to_variant(), ay.to_variant(), bx.to_variant(), by.to_variant()]);
                false
            }
            MoveOutcome::Success { matches, cascades: _, resonance_multiplier: _ } => {
                // Collect all signal data first to avoid borrow issues
                let mut match_signals = Vec::new();
                let mut special_signals = Vec::new();
                let mut echo_charged_signals = Vec::new();
                let mut echo_detonated_cells = Array::new();
                let mut has_echo_detonation = false;
                let mut resonance_mult = 1.0f32;
                let combo = board.combo;

                for (cascade_idx, m) in matches.iter().enumerate() {
                    // Convert cells to Array<Vector2i>
                    let mut cleared_cells = Array::new();
                    for &(row, col) in &m.cells {
                        cleared_cells.push(Vector2i::new(col as i32, row as i32));
                    }

                    match_signals.push((cleared_cells, m.kind as i32, cascade_idx as i32 + 1));

                    // Check for special gem creation
                    if m.is_special {
                        if let Some(special) = m.special_type {
                            let special_kind = match special {
                                SpecialGem::Bolt { .. } => 0,
                                SpecialGem::Prism => 1,
                                SpecialGem::Nova => 2,
                            };
                            // Emit at the center of the match
                            let center_idx = m.cells.len() / 2;
                            let (center_row, center_col) = m.cells[center_idx];
                            special_signals.push((Vector2i::new(center_col as i32, center_row as i32), special_kind));
                        }
                    }

                    // Check for echo charges on cleared cells
                    let mut echo_cells = Array::new();
                    for &(row, col) in &m.cells {
                        if let Some(gem) = board.gem(row, col) {
                            if gem.echo.is_some() {
                                echo_cells.push(Vector2i::new(col as i32, row as i32));
                                // Also track for echo_detonated
                                echo_detonated_cells.push(Vector2i::new(col as i32, row as i32));
                                has_echo_detonation = true;
                            }
                        }
                    }
                    if !echo_cells.is_empty() {
                        echo_charged_signals.push(echo_cells);
                    }
                }

                if has_echo_detonation {
                    resonance_mult = board.resonance_multiplier;
                }

                // Now emit all signals
                for (cells, kind, cascade) in match_signals {
                    self.base_mut().emit_signal("match_resolved", &[cells.to_variant(), kind.to_variant(), cascade.to_variant()]);
                }
                for (pos, kind) in special_signals {
                    self.base_mut().emit_signal("special_gem_created", &[pos.to_variant(), kind.to_variant()]);
                }
                for cells in echo_charged_signals {
                    self.base_mut().emit_signal("echo_charged", &[cells.to_variant()]);
                }
                if has_echo_detonation && !echo_detonated_cells.is_empty() {
                    self.base_mut().emit_signal("echo_detonated", &[echo_detonated_cells.to_variant(), resonance_mult.to_variant()]);
                }
                self.base_mut().emit_signal("objective_progress", &[(combo as i64).to_variant(), 10000i64.to_variant()]);

                true
            }
        }
    }

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
        } else {
            dict.set("empty", true.to_variant());
        }
        dict
    }

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
        self.board.as_ref().map(|b| b.resonance_multiplier).unwrap_or(1.0)
    }
}

struct BewilderedExtension;

#[gdextension]
unsafe impl ExtensionLibrary for BewilderedExtension {}