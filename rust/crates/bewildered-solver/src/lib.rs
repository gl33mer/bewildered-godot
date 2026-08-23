//! Bewildered solver library — validation logic for levels and packs.

use anyhow::Result;
use bewildered_content::{Level, Objective};
use bewildered_core::{Board, MoveOutcome};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize, Debug, Clone)]
pub struct SolverResult {
    pub level_id: String,
    pub passed: bool,
    pub static_checks: StaticCheckResult,
    pub search_check: SearchCheckResult,
    pub no_softlock_check: NoSoftlockResult,
}

#[derive(Serialize, Debug, Clone)]
pub struct StaticCheckResult {
    pub passed: bool,
    pub message: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct SearchCheckResult {
    pub passed: bool,
    pub best_progress: u32,
    pub target: u32,
    pub seeds_tested: usize,
    pub moves_per_seed: usize,
    pub message: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct NoSoftlockResult {
    pub passed: bool,
    pub reshuffles_per_playthrough: f32,
    pub playouts_sampled: usize,
    pub message: String,
}

/// Public API: validate a single level file
pub fn validate_level_file(path: &PathBuf) -> Result<SolverResult> {
    let content = std::fs::read_to_string(path)?;
    let level: Level = ron::from_str(&content)?;
    validate_level(&level)
}

/// Public API: validate an already-parsed Level
pub fn validate_level(level: &Level) -> Result<SolverResult> {
    let static_result = static_checks(level)?;
    let search_result = search_check(level)?;
    let no_softlock_result = no_softlock_check(level)?;

    let passed = static_result.passed && search_result.passed && no_softlock_result.passed;

    let result = SolverResult {
        level_id: level.id.clone(),
        passed,
        static_checks: static_result,
        search_check: search_result,
        no_softlock_check: no_softlock_result,
    };

    Ok(result)
}

/// Public API: validate an entire pack directory
pub fn validate_pack(dir: &PathBuf) -> Result<Vec<SolverResult>> {
    let mut results = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.extension().is_some_and(|ext| ext == "ron") && file_name != "manifest.ron" {
            results.push(validate_level_file(&path)?);
        }
    }

    Ok(results)
}

fn static_checks(level: &Level) -> Result<StaticCheckResult> {
    // Check grid dimensions
    if level.grid.width == 0 || level.grid.height == 0 {
        return Ok(StaticCheckResult {
            passed: false,
            message: "Grid dimensions must be positive".to_string(),
        });
    }
    if level.grid.width > 16 || level.grid.height > 16 {
        return Ok(StaticCheckResult {
            passed: false,
            message: "Grid too large (max 16x16)".to_string(),
        });
    }

    // Check objective is reachable in principle
    match &level.objective {
        Objective::ScoreTarget { points, max_moves } => {
            if *points == 0 {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: "Score target must be positive".to_string(),
                });
            }
            if *max_moves == 0 {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: "Max moves must be positive".to_string(),
                });
            }
            let max_possible = *max_moves * 1000;
            if *points > max_possible {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: format!(
                        "Score target {} exceeds theoretical max {} for {} moves",
                        points, max_possible, max_moves
                    ),
                });
            }
        }
        Objective::Collection { count, .. } => {
            if *count == 0 {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: "Collection target must be positive".to_string(),
                });
            }
            let total_cells = (level.grid.width * level.grid.height) as u32;
            if *count > total_cells * 3 {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: format!(
                        "Collection target {} unrealistic for grid size {}",
                        count, total_cells
                    ),
                });
            }
        }
        Objective::Descent { blockers_to_clear } => {
            if *blockers_to_clear == 0 {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: "Blockers to clear must be positive".to_string(),
                });
            }
            let blocker_count = level.blockers.len() as u32;
            if *blockers_to_clear > blocker_count {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: format!(
                        "Blockers to clear {} exceeds placed blockers {}",
                        blockers_to_clear, blocker_count
                    ),
                });
            }
        }
        Objective::Survival { max_moves } => {
            if *max_moves == 0 {
                return Ok(StaticCheckResult {
                    passed: false,
                    message: "Max moves must be positive".to_string(),
                });
            }
        }
    }

    // Check gem types are not empty
    if level.gem_types.is_empty() {
        return Ok(StaticCheckResult {
            passed: false,
            message: "At least one gem type must be specified".to_string(),
        });
    }

    // Check blockers don't exceed grid
    for blocker in &level.blockers {
        if blocker.pos.0 >= level.grid.width || blocker.pos.1 >= level.grid.height {
            return Ok(StaticCheckResult {
                passed: false,
                message: format!(
                    "Blocker at {:?} outside grid {}x{}",
                    blocker.pos, level.grid.width, level.grid.height
                ),
            });
        }
    }

    Ok(StaticCheckResult {
        passed: true,
        message: "All static checks passed".to_string(),
    })
}

fn search_check(level: &Level) -> Result<SearchCheckResult> {
    let seeds_to_test = if level.seed_override.is_some() { 1 } else { 50 };
    let max_moves = match &level.objective {
        Objective::ScoreTarget { max_moves, .. } | Objective::Survival { max_moves } => *max_moves,
        _ => 20,
    };

    let mut best_overall_progress = 0u32;
    let mut seeds_tested = 0;

    for seed_idx in 0..seeds_to_test {
        let seed = level.seed_override.unwrap_or_else(|| {
            use rand::Rng;
            rand::thread_rng().gen()
        }) ^ seed_idx as u64;

        let mut board = Board::new(
            level.grid.width,
            level.grid.height,
            seed,
            level.gem_types.clone(),
        );

        for blocker in &level.blockers {
            board.remove_gem(blocker.pos.1, blocker.pos.0);
        }

        let progress = run_greedy_search(&mut board, &level.objective, max_moves as usize)?;
        best_overall_progress = best_overall_progress.max(progress);
        seeds_tested += 1;
    }

    let (passed, target, message) = match &level.objective {
        Objective::ScoreTarget { points, .. } => (
            best_overall_progress >= *points,
            *points,
            if best_overall_progress >= *points {
                format!(
                    "Reached target score: {} >= {}",
                    best_overall_progress, points
                )
            } else {
                format!(
                    "Best score {} < target {} across {} seeds",
                    best_overall_progress, points, seeds_tested
                )
            },
        ),
        Objective::Collection { count, .. } => (
            best_overall_progress >= *count,
            *count,
            if best_overall_progress >= *count {
                format!(
                    "Reached collection target: {} >= {}",
                    best_overall_progress, count
                )
            } else {
                format!(
                    "Best collection {} < target {} across {} seeds",
                    best_overall_progress, count, seeds_tested
                )
            },
        ),
        Objective::Descent { blockers_to_clear } => (
            best_overall_progress >= *blockers_to_clear,
            *blockers_to_clear,
            if best_overall_progress >= *blockers_to_clear {
                format!(
                    "Reached descent target: {} >= {}",
                    best_overall_progress, blockers_to_clear
                )
            } else {
                format!(
                    "Best cleared {} < target {} across {} seeds",
                    best_overall_progress, blockers_to_clear, seeds_tested
                )
            },
        ),
        Objective::Survival { max_moves } => (
            best_overall_progress >= *max_moves,
            *max_moves,
            if best_overall_progress >= *max_moves {
                format!(
                    "Reached survival target: {} >= {} moves",
                    best_overall_progress, max_moves
                )
            } else {
                format!(
                    "Best survival {} < target {} moves across {} seeds",
                    best_overall_progress, max_moves, seeds_tested
                )
            },
        ),
    };

    Ok(SearchCheckResult {
        passed,
        best_progress: best_overall_progress,
        target,
        seeds_tested,
        moves_per_seed: max_moves as usize,
        message,
    })
}

fn run_greedy_search(board: &mut Board, objective: &Objective, max_moves: usize) -> Result<u32> {
    let mut total_progress = 0u32;

    for _move_idx in 0..max_moves {
        let legal_moves = find_legal_moves(board);

        if legal_moves.is_empty() {
            break;
        }

        let mut best_move = None;
        let mut best_score = 0u32;

        for &(r1, c1, r2, c2) in &legal_moves {
            let mut sim_board = board.clone();
            let outcome = sim_board.try_swap(r1, c1, r2, c2);

            let score = evaluate_outcome(&outcome, objective);

            if score > best_score {
                best_score = score;
                best_move = Some((r1, c1, r2, c2));
            }
        }

        if let Some((r1, c1, r2, c2)) = best_move {
            let outcome = board.try_swap(r1, c1, r2, c2);
            total_progress += evaluate_outcome(&outcome, objective);
            board.decrement_echoes();
        } else {
            let (r1, c1, r2, c2) = legal_moves[0];
            let outcome = board.try_swap(r1, c1, r2, c2);
            total_progress += evaluate_outcome(&outcome, objective);
            board.decrement_echoes();
        }
    }

    Ok(total_progress)
}

fn find_legal_moves(board: &Board) -> Vec<(usize, usize, usize, usize)> {
    let mut moves = Vec::new();

    for row in 0..board.height {
        for col in 0..board.width {
            if col + 1 < board.width && board.would_match(row, col, row, col + 1) {
                moves.push((row, col, row, col + 1));
            }
            if row + 1 < board.height && board.would_match(row, col, row + 1, col) {
                moves.push((row, col, row + 1, col));
            }
        }
    }

    moves
}

fn evaluate_outcome(outcome: &MoveOutcome, objective: &Objective) -> u32 {
    match outcome {
        MoveOutcome::Success {
            matches, cascades, ..
        } => {
            let mut progress = 0u32;

            for m in matches {
                let match_size = m.cells.len() as u32;

                match objective {
                    Objective::ScoreTarget { .. } => {
                        use bewildered_core::scoring::*;
                        let base = BASE_GEM_POINTS * match_size;
                        let match_mult = match match_size {
                            3 => MATCH_3_MULTIPLIER,
                            4 => MATCH_4_MULTIPLIER,
                            _ => MATCH_5_MULTIPLIER,
                        };
                        let cascade_mult = CASCADE_MULTIPLIER.powi(*cascades as i32);
                        progress += (base as f32 * match_mult * cascade_mult) as u32;
                    }
                    Objective::Collection { .. } => {
                        progress += match_size;
                    }
                    Objective::Descent { .. } => {
                        progress += match_size;
                    }
                    Objective::Survival { .. } => {
                        progress += 1;
                    }
                }
            }

            progress
        }
        MoveOutcome::NoMatch => 0,
        MoveOutcome::Illegal => 0,
    }
}

fn no_softlock_check(level: &Level) -> Result<NoSoftlockResult> {
    let playouts = if level.seed_override.is_some() {
        10
    } else {
        100
    };
    let max_moves = match &level.objective {
        Objective::ScoreTarget { max_moves, .. } | Objective::Survival { max_moves } => *max_moves,
        _ => 30,
    };

    let mut total_reshuffles = 0usize;
    let mut playouts_completed = 0;

    for seed_idx in 0..playouts {
        let seed = level.seed_override.unwrap_or_else(|| {
            use rand::Rng;
            rand::thread_rng().gen()
        }) ^ seed_idx as u64;

        let mut board = Board::new(
            level.grid.width,
            level.grid.height,
            seed,
            level.gem_types.clone(),
        );

        for blocker in &level.blockers {
            board.remove_gem(blocker.pos.1, blocker.pos.0);
        }

        let mut reshuffles_this_playout = 0;

        for _move_idx in 0..max_moves {
            if !board.has_legal_moves() {
                reshuffles_this_playout += 1;
                board = Board::new(
                    level.grid.width,
                    level.grid.height,
                    seed ^ reshuffles_this_playout as u64,
                    level.gem_types.clone(),
                );
                for blocker in &level.blockers {
                    board.remove_gem(blocker.pos.1, blocker.pos.0);
                }
            }

            let legal_moves = find_legal_moves(&board);
            if legal_moves.is_empty() {
                break;
            }

            let (r1, c1, r2, c2) = legal_moves[0];
            board.try_swap(r1, c1, r2, c2);
            board.decrement_echoes();
        }

        total_reshuffles += reshuffles_this_playout;
        playouts_completed += 1;
    }

    let avg_reshuffles = if playouts_completed > 0 {
        total_reshuffles as f32 / playouts_completed as f32
    } else {
        0.0
    };

    let passed = avg_reshuffles < 1.0;

    let message = if passed {
        format!(
            "Average reshuffles per playthrough: {:.2} (threshold: <1.0)",
            avg_reshuffles
        )
    } else {
        format!(
            "Too many reshuffles: {:.2} per playthrough (threshold: <1.0)",
            avg_reshuffles
        )
    };

    Ok(NoSoftlockResult {
        passed,
        reshuffles_per_playthrough: avg_reshuffles,
        playouts_sampled: playouts_completed,
        message,
    })
}
