//! Bewildered content — level/pack data model and RON serialization.

use bewildered_core::{Board, MoveOutcome};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::FileOptions;
use zip::ZipArchive;
use zip::ZipWriter;

pub use bewildered_core::GemKind;

/// A level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub id: String,
    pub name: String,
    pub grid: GridSize,
    pub gem_types: Vec<GemKind>,
    pub blockers: Vec<Blocker>,
    pub objective: Objective,
    pub relic_pool_tags: Vec<String>,
    pub seed_override: Option<u64>,
    #[serde(default)]
    pub gems: Vec<Option<GemKind>>,
}

/// Grid dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridSize {
    pub width: usize,
    pub height: usize,
}

/// A blocker tile on the board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocker {
    pub pos: (usize, usize),
    pub kind: BlockerKind,
}

/// Types of blockers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockerKind {
    Ice { hits: u8 },
    Crate { hits: u8 },
}

/// Level objective.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Objective {
    ScoreTarget { points: u32, max_moves: u32 },
    Collection { target_gem: GemKind, count: u32 },
    Descent { blockers_to_clear: u32 },
    Survival { max_moves: u32 },
}

/// Validation check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

/// Validation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub passed: bool,
    pub checks: Vec<ValidationCheck>,
}

/// Campaign pack manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub title: String,
    pub author: String,
    pub levels: Vec<String>,
    pub relic_pools: HashMap<String, Vec<Relic>>,
}

/// A relic (passive modifier).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relic {
    pub id: String,
    pub name: String,
    pub description: String,
    pub effect: RelicEffect,
}

/// Relic effects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelicEffect {
    DiagonalMatches,
    FifthHue,
    EchoChamber { extra_moves: u8 },
    CornerCutter,
    GreedyNova,
    ExtraMoves { count: u8 },
    CollectionBonus { reduction_pct: u8 },
    ScoreBonus { bonus_pct: f32 },
    PrismMaster { extra_clears: u8 },
    BoltMaster { extra_length: u8 },
    NovaMaster { extra_colors: u8 },
    ShieldBearer { blocks_per_chamber: u8 },
    TimeDilation { extra_seconds: u8 },
    Gambler { risk_pct: u8, reward_pct: u8 },
}

impl RelicEffect {
    pub fn to_rule_modifiers(&self) -> bewildered_core::RuleModifiers {
        let mut modifiers = bewildered_core::RuleModifiers::new();
        match self {
            RelicEffect::DiagonalMatches => modifiers.diagonal_matches = true,
            RelicEffect::FifthHue => modifiers.fifth_hue = true,
            RelicEffect::EchoChamber { extra_moves } => modifiers.echo_extra_moves = *extra_moves,
            RelicEffect::CornerCutter => modifiers.corner_cutter = true,
            RelicEffect::GreedyNova => modifiers.greedy_nova = true,
            RelicEffect::ExtraMoves { count } => modifiers.extra_moves = *count,
            RelicEffect::CollectionBonus { reduction_pct } => {
                modifiers.collection_reduction_pct = *reduction_pct
            }
            RelicEffect::ScoreBonus { bonus_pct } => modifiers.score_bonus_pct = *bonus_pct,
            RelicEffect::PrismMaster { .. } => {}
            RelicEffect::BoltMaster { .. } => {}
            RelicEffect::NovaMaster { .. } => {}
            RelicEffect::ShieldBearer { .. } => {}
            RelicEffect::TimeDilation { .. } => {}
            RelicEffect::Gambler { .. } => {}
        }
        modifiers
    }
}

impl Default for Level {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default Level".to_string(),
            grid: GridSize {
                width: 8,
                height: 8,
            },
            gem_types: vec![
                GemKind::Circle,
                GemKind::Triangle,
                GemKind::Square,
                GemKind::Diamond,
            ],
            blockers: Vec::new(),
            objective: Objective::ScoreTarget {
                points: 10000,
                max_moves: 20,
            },
            relic_pool_tags: vec!["descent-early".to_string()],
            seed_override: None,
            gems: vec![None; 64],
        }
    }
}

impl Default for Pack {
    fn default() -> Self {
        Self {
            id: "default-pack".to_string(),
            title: "Default Pack".to_string(),
            author: "Unknown".to_string(),
            levels: Vec::new(),
            relic_pools: HashMap::new(),
        }
    }
}

impl Level {
    /// Load a level from a RON file.
    pub fn load_ron<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let level: Self = ron::from_str(&content)?;
        Ok(level)
    }

    /// Save a level to a RON file.
    pub fn save_ron<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let content = ron::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Load a level from a loose directory (for in-dev packs).
    /// Expects a file named `<level_id>.ron` in the directory.
    pub fn load_from_dir<P: AsRef<Path>>(dir: P, level_id: &str) -> anyhow::Result<Self> {
        let path = dir.as_ref().join(format!("{}.ron", level_id));
        Self::load_ron(path)
    }

    /// Save a level to a loose directory.
    pub fn save_to_dir<P: AsRef<Path>>(&self, dir: P) -> anyhow::Result<()> {
        let path = dir.as_ref().join(format!("{}.ron", self.id));
        self.save_ron(path)
    }
}

impl Pack {
    /// Load a pack from a .bwpack zip file.
    pub fn load_zip<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        // Read manifest.ron
        let manifest_content = {
            let mut manifest_file = archive.by_name("manifest.ron")?;
            let mut content = String::new();
            manifest_file.read_to_string(&mut content)?;
            content
        };
        let pack: Self = ron::from_str(&manifest_content)?;

        // Load all level files to verify
        for level_id in &pack.levels {
            let level_name = format!("{}.ron", level_id);
            let mut level_file = archive.by_name(&level_name)?;
            let mut level_content = String::new();
            level_file.read_to_string(&mut level_content)?;
            // We just verify the level can be parsed; actual levels are loaded on demand
            let _level: Level = ron::from_str(&level_content)?;
        }

        Ok(pack)
    }

    /// Save a pack to a .bwpack zip file.
    pub fn save_zip<P: AsRef<Path>>(&self, path: P, levels: &[Level]) -> anyhow::Result<()> {
        let file = fs::File::create(path)?;
        let mut zip = ZipWriter::new(file);
        let options: FileOptions<()> =
            FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // Write manifest.ron
        let manifest_content = ron::to_string(self)?;
        zip.start_file("manifest.ron", options)?;
        zip.write_all(manifest_content.as_bytes())?;

        // Write each level file
        for level in levels {
            let level_content = ron::to_string(level)?;
            let level_name = format!("{}.ron", level.id);
            zip.start_file(&level_name, options)?;
            zip.write_all(level_content.as_bytes())?;
        }

        zip.finish()?;
        Ok(())
    }

    /// Load a pack from a loose directory (for in-dev packs).
    /// Expects manifest.ron and level files in the directory.
    pub fn load_dir<P: AsRef<Path>>(dir: P) -> anyhow::Result<Self> {
        let dir = dir.as_ref();
        let manifest_path = dir.join("manifest.ron");
        let manifest_content = fs::read_to_string(manifest_path)?;
        let pack: Self = ron::from_str(&manifest_content)?;
        Ok(pack)
    }

    /// Save a pack to a loose directory.
    pub fn save_dir<P: AsRef<Path>>(&self, dir: P, levels: &[Level]) -> anyhow::Result<()> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;

        // Write manifest.ron
        let manifest_content = ron::to_string(self)?;
        fs::write(dir.join("manifest.ron"), manifest_content)?;

        // Write each level file
        for level in levels {
            let level_content = ron::to_string(level)?;
            fs::write(dir.join(format!("{}.ron", level.id)), level_content)?;
        }

        Ok(())
    }

    /// Load all levels for this pack from a loose directory.
    pub fn load_levels_from_dir<P: AsRef<Path>>(&self, dir: P) -> anyhow::Result<Vec<Level>> {
        let dir = dir.as_ref();
        let mut levels = Vec::new();
        for level_id in &self.levels {
            let level = Level::load_from_dir(dir, level_id)?;
            levels.push(level);
        }
        Ok(levels)
    }
}

pub fn validate_level(level: &Level) -> anyhow::Result<ValidationResult> {
    let mut checks = Vec::new();

    // Static checks
    if level.grid.width == 0 || level.grid.height == 0 {
        checks.push(ValidationCheck {
            name: "grid_dimensions".to_string(),
            passed: false,
            message: "Grid dimensions must be positive".to_string(),
        });
    } else {
        checks.push(ValidationCheck {
            name: "grid_dimensions".to_string(),
            passed: true,
            message: "Grid dimensions are valid".to_string(),
        });
    }

    if level.grid.width > 16 || level.grid.height > 16 {
        checks.push(ValidationCheck {
            name: "grid_size_limit".to_string(),
            passed: false,
            message: "Grid too large (max 16x16)".to_string(),
        });
    } else {
        checks.push(ValidationCheck {
            name: "grid_size_limit".to_string(),
            passed: true,
            message: "Grid size within limits".to_string(),
        });
    }

    if level.gem_types.is_empty() {
        checks.push(ValidationCheck {
            name: "gem_types".to_string(),
            passed: false,
            message: "At least one gem type required".to_string(),
        });
    } else {
        checks.push(ValidationCheck {
            name: "gem_types".to_string(),
            passed: true,
            message: "Gem types defined".to_string(),
        });
    }

    // Check objective
    match &level.objective {
        Objective::ScoreTarget { points, max_moves } => {
            if *points == 0 {
                checks.push(ValidationCheck {
                    name: "score_target".to_string(),
                    passed: false,
                    message: "Score target must be positive".to_string(),
                });
            } else {
                checks.push(ValidationCheck {
                    name: "score_target".to_string(),
                    passed: true,
                    message: "Score target is positive".to_string(),
                });
            }
            if *max_moves == 0 {
                checks.push(ValidationCheck {
                    name: "max_moves".to_string(),
                    passed: false,
                    message: "Max moves must be positive".to_string(),
                });
            } else {
                checks.push(ValidationCheck {
                    name: "max_moves".to_string(),
                    passed: true,
                    message: "Max moves is positive".to_string(),
                });
            }
        }
        Objective::Collection { count, .. } => {
            if *count == 0 {
                checks.push(ValidationCheck {
                    name: "collection_count".to_string(),
                    passed: false,
                    message: "Collection count must be positive".to_string(),
                });
            } else {
                checks.push(ValidationCheck {
                    name: "collection_count".to_string(),
                    passed: true,
                    message: "Collection count is positive".to_string(),
                });
            }
        }
        Objective::Descent { blockers_to_clear } => {
            if *blockers_to_clear == 0 {
                checks.push(ValidationCheck {
                    name: "descent_blockers".to_string(),
                    passed: false,
                    message: "Blockers to clear must be positive".to_string(),
                });
            } else {
                checks.push(ValidationCheck {
                    name: "descent_blockers".to_string(),
                    passed: true,
                    message: "Blockers to clear is positive".to_string(),
                });
            }
        }
        Objective::Survival { max_moves } => {
            if *max_moves == 0 {
                checks.push(ValidationCheck {
                    name: "survival_moves".to_string(),
                    passed: false,
                    message: "Max moves must be positive".to_string(),
                });
            } else {
                checks.push(ValidationCheck {
                    name: "survival_moves".to_string(),
                    passed: true,
                    message: "Max moves is positive".to_string(),
                });
            }
        }
    }

    // Check blockers
    for blocker in &level.blockers {
        if blocker.pos.0 >= level.grid.width || blocker.pos.1 >= level.grid.height {
            checks.push(ValidationCheck {
                name: "blocker_bounds".to_string(),
                passed: false,
                message: format!("Blocker at {:?} outside grid", blocker.pos),
            });
        } else {
            checks.push(ValidationCheck {
                name: "blocker_bounds".to_string(),
                passed: true,
                message: "Blocker within bounds".to_string(),
            });
        }
    }

    // Run search check
    let search_passed = run_search_check(level);
    checks.push(ValidationCheck {
        name: "search_check".to_string(),
        passed: search_passed,
        message: if search_passed {
            "Level appears solvable"
        } else {
            "Level may not be solvable"
        }
        .to_string(),
    });

    let passed = checks.iter().all(|c| c.passed);
    Ok(ValidationResult { passed, checks })
}

fn run_search_check(level: &Level) -> bool {
    // Create a board with the level's seed (or random if not overridden)
    let seed = level.seed_override.unwrap_or_else(rand::random);
    let mut board = Board::new(
        level.grid.width,
        level.grid.height,
        seed,
        level.gem_types.clone(),
    );

    // Place blockers
    for blocker in &level.blockers {
        board.remove_gem(blocker.pos.1, blocker.pos.0);
    }

    let max_moves = match &level.objective {
        Objective::ScoreTarget { max_moves, .. } => *max_moves,
        Objective::Survival { max_moves } => *max_moves,
        _ => 20,
    };

    // Try a few random playthroughs
    for _ in 0..10 {
        let mut test_board = board.clone();
        let mut progress = 0u32;

        for _ in 0..max_moves {
            // Find legal moves without moving test_board into a closure
            let mut legal_moves = Vec::new();
            for r in 0..test_board.height {
                for c in 0..test_board.width {
                    if c + 1 < test_board.width && test_board.would_match(r, c, r, c + 1) {
                        legal_moves.push((r, c, r, c + 1));
                    }
                    if r + 1 < test_board.height && test_board.would_match(r, c, r + 1, c) {
                        legal_moves.push((r, c, r + 1, c));
                    }
                }
            }

            if legal_moves.is_empty() {
                break;
            }

            // Pick a random legal move
            let idx = rand::random::<usize>() % legal_moves.len();
            let (r1, c1, r2, c2) = legal_moves[idx];
            let outcome = test_board.try_swap(r1, c1, r2, c2);

            if let MoveOutcome::Success { matches, .. } = outcome {
                for m in matches {
                    match &level.objective {
                        Objective::ScoreTarget { .. } => {
                            progress += 100;
                        }
                        Objective::Collection { target_gem, .. } => {
                            if m.kind == *target_gem {
                                progress += m.cells.len() as u32;
                            }
                        }
                        Objective::Descent { .. } => {
                            progress += 1;
                        }
                        Objective::Survival { max_moves: _ } => {
                            progress += 1;
                        }
                    }
                }
                test_board.decrement_echoes();
            }
        }

        let target = match &level.objective {
            Objective::ScoreTarget { points, .. } => *points,
            Objective::Collection { count, .. } => *count,
            Objective::Descent { blockers_to_clear } => *blockers_to_clear,
            Objective::Survival { max_moves } => *max_moves,
        };

        if progress >= target {
            return true;
        }
    }
    false
}

/// Convert a vector of RelicEffects to combined RuleModifiers.
pub fn relics_to_rule_modifiers(relics: &[RelicEffect]) -> bewildered_core::RuleModifiers {
    let mut combined = bewildered_core::RuleModifiers::new();
    for effect in relics {
        combined.merge(&effect.to_rule_modifiers());
    }
    combined
}

/// Try to load the embedded campaign pack and levels.
/// Returns None if the embedded-campaign feature is not enabled or assets are missing.
#[cfg(feature = "embedded-campaign")]
pub fn try_get_embedded_campaign() -> Option<(Pack, Vec<Level>)> {
    use include_dir::include_dir;
    let campaign_dir = include_dir!("assets/campaign");

    let manifest_file = campaign_dir.get_file("manifest.ron")?;
    let manifest_content = std::str::from_utf8(manifest_file.contents()).ok()?;
    let pack: Pack = ron::from_str(manifest_content).ok()?;

    let mut levels = Vec::new();
    for level_entry in &pack.levels {
        let filename = format!("{}.ron", level_entry);
        let level_file = campaign_dir.get_file(&filename)?;
        let level_content = std::str::from_utf8(level_file.contents()).ok()?;
        let level: Level = ron::from_str(level_content).ok()?;
        levels.push(level);
    }

    Some((pack, levels))
}

#[cfg(not(feature = "embedded-campaign"))]
pub fn try_get_embedded_campaign() -> Option<(Pack, Vec<Level>)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relic_to_modifiers() {
        let echo = RelicEffect::EchoChamber { extra_moves: 1 };
        let mods = echo.to_rule_modifiers();
        assert_eq!(mods.echo_extra_moves, 1);
    }

    #[test]
    fn test_multiple_relics() {
        let relics = vec![
            RelicEffect::EchoChamber { extra_moves: 1 },
            RelicEffect::DiagonalMatches,
        ];
        let mods = relics_to_rule_modifiers(&relics);
        assert_eq!(mods.echo_extra_moves, 1);
        assert!(mods.diagonal_matches);
    }

    #[test]
    fn test_validate_level_basic() {
        let level = Level::default();
        let result = validate_level(&level);
        assert!(result.is_ok());
    }

    #[test]
    fn load_all_campaign_levels() {
        // Every campaign level shipped in the root `levels/` dir (sourced from
        // this crate's assets/campaign) must parse — covering 4–6 gem types,
        // blockers, and all four objective variants used by Stage 6.
        for i in 1..=8 {
            let id = format!("campaign-{:03}", i);
            let path = format!("assets/campaign/{}.ron", id);
            let level = Level::load_ron(&path)
                .unwrap_or_else(|e| panic!("{} failed to parse: {}", id, e));
            assert!(!level.name.is_empty(), "{} missing name", id);
            assert_eq!(level.grid.width, 8);
            assert_eq!(level.grid.height, 8);
            assert!(
                !level.gem_types.is_empty(),
                "{} missing gem_types",
                id
            );
        }
    }
}
