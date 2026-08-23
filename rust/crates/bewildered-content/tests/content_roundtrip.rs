//! Round-trip tests for RON serialization and .bwpack zip I/O.

use anyhow::Result;
use bewildered_content::{
    Blocker, BlockerKind, GemKind, GridSize, Level, Objective, Pack, Relic, RelicEffect,
};
use ron;
use tempfile::tempdir;

#[test]
fn test_level_ron_roundtrip() -> Result<()> {
    // Create a level with all objective variants
    let level = Level {
        id: "test-roundtrip".to_string(),
        name: "Roundtrip Test".to_string(),
        grid: GridSize {
            width: 8,
            height: 8,
        },
        gem_types: vec![
            GemKind::Circle,
            GemKind::Triangle,
            GemKind::Square,
            GemKind::Diamond,
            GemKind::Star,
        ],
        blockers: vec![
            Blocker {
                pos: (2, 3),
                kind: BlockerKind::Ice { hits: 2 },
            },
            Blocker {
                pos: (5, 5),
                kind: BlockerKind::Crate { hits: 1 },
            },
        ],
        objective: Objective::ScoreTarget {
            points: 12000,
            max_moves: 20,
        },
        relic_pool_tags: vec!["descent-early".to_string()],
        seed_override: Some(42),
        gems: vec![],
    };

    let dir = tempdir()?;
    let path = dir.path().join("test-roundtrip.ron");

    // Save
    level.save_ron(&path)?;

    // Load
    let loaded = Level::load_ron(&path)?;

    // Verify
    assert_eq!(level.id, loaded.id);
    assert_eq!(level.name, loaded.name);
    assert_eq!(level.grid.width, loaded.grid.width);
    assert_eq!(level.grid.height, loaded.grid.height);
    assert_eq!(level.gem_types, loaded.gem_types);
    assert_eq!(level.blockers.len(), loaded.blockers.len());
    assert_eq!(level.relic_pool_tags, loaded.relic_pool_tags);
    assert_eq!(level.seed_override, loaded.seed_override);

    // Check objective round-trip
    match (&level.objective, &loaded.objective) {
        (
            Objective::ScoreTarget {
                points: p1,
                max_moves: m1,
            },
            Objective::ScoreTarget {
                points: p2,
                max_moves: m2,
            },
        ) => {
            assert_eq!(p1, p2);
            assert_eq!(m1, m2);
        }
        _ => panic!("Objective type mismatch"),
    }

    Ok(())
}

#[test]
fn test_all_objective_variants_roundtrip() -> Result<()> {
    let objectives = vec![
        Objective::ScoreTarget {
            points: 10000,
            max_moves: 20,
        },
        Objective::Collection {
            target_gem: GemKind::Circle,
            count: 50,
        },
        Objective::Descent {
            blockers_to_clear: 5,
        },
        Objective::Survival { max_moves: 30 },
    ];

    // Debug: print RON for each objective
    for obj in &objectives {
        let ron_str = ron::to_string(obj).unwrap();
        eprintln!("Objective RON: {}", ron_str);
    }

    for (i, objective) in objectives.into_iter().enumerate() {
        let level = Level {
            id: format!("test-obj-{}", i),
            name: format!("Objective Test {}", i),
            grid: GridSize {
                width: 8,
                height: 8,
            },
            gem_types: vec![GemKind::Circle, GemKind::Triangle, GemKind::Square],
            blockers: vec![],
            objective,
            relic_pool_tags: vec![],
            seed_override: Some(42),
            gems: vec![],
        };

        let dir = tempdir()?;
        let path = dir.path().join(format!("test-obj-{}.ron", i));

        level.save_ron(&path)?;
        let content = std::fs::read_to_string(&path)?;
        eprintln!("Full level RON:\n{}", content);
        let loaded = Level::load_ron(&path)?;

        // Verify objective matches
        assert_eq!(
            format!("{:?}", level.objective),
            format!("{:?}", loaded.objective)
        );
    }

    Ok(())
}

#[test]
fn test_pack_zip_roundtrip() -> Result<()> {
    // Create a pack with multiple levels
    let levels = vec![
        Level {
            id: "chamber-001".to_string(),
            name: "First Chamber".to_string(),
            grid: GridSize {
                width: 8,
                height: 8,
            },
            gem_types: vec![GemKind::Circle, GemKind::Triangle],
            blockers: vec![],
            objective: Objective::ScoreTarget {
                points: 5000,
                max_moves: 15,
            },
            relic_pool_tags: vec!["descent-early".to_string()],
            seed_override: Some(1),
            gems: vec![],
        },
        Level {
            id: "chamber-002".to_string(),
            name: "Second Chamber".to_string(),
            grid: GridSize {
                width: 8,
                height: 8,
            },
            gem_types: vec![GemKind::Circle, GemKind::Triangle, GemKind::Square],
            blockers: vec![Blocker {
                pos: (3, 3),
                kind: BlockerKind::Ice { hits: 1 },
            }],
            objective: Objective::Collection {
                target_gem: GemKind::Triangle,
                count: 30,
            },
            relic_pool_tags: vec!["descent-early".to_string()],
            seed_override: Some(2),
            gems: vec![],
        },
    ];

    let pack = Pack {
        id: "test-pack".to_string(),
        title: "Test Pack".to_string(),
        author: "Test Author".to_string(),
        levels: vec!["chamber-001".to_string(), "chamber-002".to_string()],
        relic_pools: {
            let mut pools = std::collections::HashMap::new();
            pools.insert(
                "descent-early".to_string(),
                vec![Relic {
                    id: "relic-1".to_string(),
                    name: "Test Relic".to_string(),
                    description: "A test relic".to_string(),
                    effect: RelicEffect::EchoChamber { extra_moves: 1 },
                }],
            );
            pools
        },
    };

    let dir = tempdir()?;
    let zip_path = dir.path().join("test-pack.bwpack");

    // Save pack to zip
    pack.save_zip(&zip_path, &levels)?;

    // Load pack from zip
    let loaded_pack = Pack::load_zip(&zip_path)?;

    // Verify pack metadata
    assert_eq!(pack.id, loaded_pack.id);
    assert_eq!(pack.title, loaded_pack.title);
    assert_eq!(pack.author, loaded_pack.author);
    assert_eq!(pack.levels, loaded_pack.levels);
    assert_eq!(pack.relic_pools.len(), loaded_pack.relic_pools.len());

    Ok(())
}

#[test]
fn test_pack_dir_roundtrip() -> Result<()> {
    let levels = vec![Level {
        id: "dir-level-1".to_string(),
        name: "Dir Level 1".to_string(),
        grid: GridSize {
            width: 6,
            height: 6,
        },
        gem_types: vec![GemKind::Circle, GemKind::Triangle],
        blockers: vec![],
        objective: Objective::Survival { max_moves: 25 },
        relic_pool_tags: vec![],
        seed_override: None,
        gems: vec![],
    }];

    let pack = Pack {
        id: "dir-pack".to_string(),
        title: "Directory Pack".to_string(),
        author: "Dir Author".to_string(),
        levels: vec!["dir-level-1".to_string()],
        relic_pools: std::collections::HashMap::new(),
    };

    let dir = tempdir()?;

    // Save pack to loose directory
    pack.save_dir(dir.path(), &levels)?;

    // Load pack from loose directory
    let loaded_pack = Pack::load_dir(dir.path())?;

    assert_eq!(pack.id, loaded_pack.id);
    assert_eq!(pack.title, loaded_pack.title);
    assert_eq!(pack.levels, loaded_pack.levels);

    // Load levels from directory
    let loaded_levels = loaded_pack.load_levels_from_dir(dir.path())?;
    assert_eq!(levels.len(), loaded_levels.len());
    assert_eq!(levels[0].id, loaded_levels[0].id);
    assert_eq!(levels[0].objective, loaded_levels[0].objective);

    Ok(())
}

#[test]
fn test_level_load_from_dir() -> Result<()> {
    let level = Level {
        id: "dir-load-test".to_string(),
        name: "Dir Load Test".to_string(),
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
        blockers: vec![],
        objective: Objective::ScoreTarget {
            points: 8000,
            max_moves: 18,
        },
        relic_pool_tags: vec![],
        seed_override: Some(123),
        gems: vec![],
    };

    let dir = tempdir()?;
    level.save_to_dir(dir.path())?;

    let loaded = Level::load_from_dir(dir.path(), "dir-load-test")?;

    assert_eq!(level.id, loaded.id);
    assert_eq!(level.seed_override, loaded.seed_override);

    Ok(())
}

#[test]
#[cfg(feature = "embedded-campaign")]
fn test_embedded_campaign_loads() -> Result<()> {
    let (pack, levels) = bewildered_content::try_get_embedded_campaign()
        .ok_or_else(|| anyhow::anyhow!("embedded campaign not found"))?;

    // Verify pack metadata
    assert_eq!(pack.id, "default-campaign");
    assert_eq!(pack.title, "Bewildered Campaign");
    assert!(!pack.levels.is_empty());

    // Verify all levels load
    assert_eq!(levels.len(), pack.levels.len());
    for level in &levels {
        assert!(!level.id.is_empty());
        assert!(!level.name.is_empty());
        assert!(level.grid.width > 0 && level.grid.height > 0);
        assert!(!level.gem_types.is_empty());
    }

    // Verify we have at least one of each objective type
    let mut has_score = false;
    let mut has_collection = false;
    let mut has_descent = false;
    let mut has_survival = false;

    for level in &levels {
        match &level.objective {
            Objective::ScoreTarget { .. } => has_score = true,
            Objective::Collection { .. } => has_collection = true,
            Objective::Descent { .. } => has_descent = true,
            Objective::Survival { .. } => has_survival = true,
        }
    }

    assert!(has_score, "Missing ScoreTarget objective");
    assert!(has_collection, "Missing Collection objective");
    assert!(has_descent, "Missing Descent objective");
    assert!(has_survival, "Missing Survival objective");

    Ok(())
}
