//! Roguelike Descent run state and the relic pool.
//!
//! A [`DescentRun`] walks the player through sequential chambers. Between
//! chambers it offers a 3-relic draft; picked relics merge their passive
//! [`RuleModifiers`] into the run for the rest of the descent. All rules live
//! here; Godot only presents the draft screen and relic tray.

use crate::RuleModifiers;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Rare,
    Epic,
}

impl Rarity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Rarity::Common => "Common",
            Rarity::Rare => "Rare",
            Rarity::Epic => "Epic",
        }
    }
}

/// A passive run modifier offered in the between-chamber draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relic {
    pub id: &'static str,
    pub name: &'static str,
    pub description: String,
    pub rarity: Rarity,
    pub modifiers: RuleModifiers,
}

/// The static relic pool. Only modifiers with real simulation effects are
/// pool-eligible.
pub fn relic_pool() -> Vec<Relic> {
    vec![
        Relic {
            id: "extra_moves",
            name: "Time Weaver",
            description: "+4 moves in every chamber.".to_string(),
            rarity: Rarity::Common,
            modifiers: RuleModifiers {
                extra_moves: 4,
                ..RuleModifiers::default()
            },
        },
        Relic {
            id: "echo_chamber",
            name: "Echo Chamber",
            description: "Echo charges persist for 2 extra turns.".to_string(),
            rarity: Rarity::Rare,
            modifiers: RuleModifiers {
                echo_extra_moves: 2,
                ..RuleModifiers::default()
            },
        },
        Relic {
            id: "resonant_heart",
            name: "Resonant Heart",
            description: "Echo charges persist for 1 extra turn.".to_string(),
            rarity: Rarity::Common,
            modifiers: RuleModifiers {
                echo_extra_moves: 1,
                ..RuleModifiers::default()
            },
        },
        Relic {
            id: "golden_touch",
            name: "Golden Touch",
            description: "+30% score from every match.".to_string(),
            rarity: Rarity::Rare,
            modifiers: RuleModifiers {
                score_bonus_pct: 0.30,
                ..RuleModifiers::default()
            },
        },
        Relic {
            id: "midas_core",
            name: "Midas Core",
            description: "+60% score from every match.".to_string(),
            rarity: Rarity::Epic,
            modifiers: RuleModifiers {
                score_bonus_pct: 0.60,
                ..RuleModifiers::default()
            },
        },
        Relic {
            id: "deep_echoes",
            name: "Deep Echoes",
            description: "+3 moves and echo charges last 1 extra turn.".to_string(),
            rarity: Rarity::Rare,
            modifiers: RuleModifiers {
                extra_moves: 3,
                echo_extra_moves: 1,
                ..RuleModifiers::default()
            },
        },
        Relic {
            id: "gilded_hours",
            name: "Gilded Hours",
            description: "+2 moves and +15% score.".to_string(),
            rarity: Rarity::Common,
            modifiers: RuleModifiers {
                extra_moves: 2,
                score_bonus_pct: 0.15,
                ..RuleModifiers::default()
            },
        },
    ]
}

/// Sequential-chamber run state with the between-chamber relic draft.
pub struct DescentRun {
    pub chamber: u32,
    pub base_seed: u64,
    pub relics: Vec<Relic>,
    /// All picked relics' modifiers merged into one rule set.
    pub modifiers: RuleModifiers,
    rng: StdRng,
}

impl DescentRun {
    pub fn new(base_seed: u64) -> Self {
        Self {
            chamber: 1,
            base_seed,
            relics: Vec::new(),
            modifiers: RuleModifiers::default(),
            rng: StdRng::seed_from_u64(base_seed),
        }
    }

    /// Seed for the current chamber's board (deterministic per run).
    pub fn chamber_seed(&self) -> u64 {
        self.base_seed ^ (self.chamber as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    /// Offer 3 distinct relics the player does not own yet.
    pub fn draft_offers(&mut self) -> Vec<Relic> {
        let pool = relic_pool();
        let owned: Vec<&str> = self.relics.iter().map(|r| r.id).collect();
        let available: Vec<&Relic> = pool
            .iter()
            .filter(|r| !owned.contains(&r.id))
            .collect();
        let mut offers: Vec<Relic> = Vec::new();
        let mut picked: Vec<usize> = Vec::new();
        let take = available.len().min(3);
        while offers.len() < take {
            let idx = self.rng.gen_range(0..available.len());
            if picked.contains(&idx) {
                continue;
            }
            picked.push(idx);
            offers.push(available[idx].clone());
        }
        offers
    }

    /// Pick a relic from the last draft offer set. Merges its modifiers.
    pub fn pick_relic(&mut self, relic: &Relic) {
        if self.relics.iter().any(|r| r.id == relic.id) {
            return;
        }
        self.modifiers.merge(&relic.modifiers);
        self.relics.push(relic.clone());
    }

    /// Advance to the next chamber.
    pub fn advance_chamber(&mut self) {
        self.chamber += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_offers_three_distinct() {
        let mut run = DescentRun::new(42);
        for _ in 0..20 {
            let offers = run.draft_offers();
            assert_eq!(offers.len(), 3);
            let ids: Vec<&str> = offers.iter().map(|r| r.id).collect();
            let unique: std::collections::HashSet<&str> = ids.clone().into_iter().collect();
            assert_eq!(unique.len(), 3, "offers must be distinct: {:?}", ids);
        }
    }

    #[test]
    fn draft_is_deterministic_per_seed() {
        let mut a = DescentRun::new(7);
        let mut b = DescentRun::new(7);
        let oa = a.draft_offers();
        let ob = b.draft_offers();
        assert_eq!(
            oa.iter().map(|r| r.id).collect::<Vec<_>>(),
            ob.iter().map(|r| r.id).collect::<Vec<_>>()
        );
        let mut c = DescentRun::new(8);
        let oc = c.draft_offers();
        assert_ne!(
            oa.iter().map(|r| r.id).collect::<Vec<_>>(),
            oc.iter().map(|r| r.id).collect::<Vec<_>>(),
            "different seeds should usually offer different relics"
        );
    }

    #[test]
    fn picking_merges_modifiers_and_blocks_duplicates() {
        let mut run = DescentRun::new(1);
        let offers = run.draft_offers();
        let echo = offers
            .iter()
            .find(|r| r.id == "echo_chamber")
            .cloned()
            .expect("pool contains echo_chamber");
        run.pick_relic(&echo);
        assert_eq!(run.modifiers.echo_extra_moves, 2);
        assert_eq!(run.relics.len(), 1);

        // Duplicate pick is a no-op.
        run.pick_relic(&echo);
        assert_eq!(run.relics.len(), 1);
        assert_eq!(run.modifiers.echo_extra_moves, 2);

        // Next draft excludes the owned relic.
        let next = run.draft_offers();
        assert!(next.iter().all(|r| r.id != "echo_chamber"));
    }

    #[test]
    fn modifiers_stack_across_picks() {
        let mut run = DescentRun::new(3);
        let pool = relic_pool();
        let moves = pool.iter().find(|r| r.id == "extra_moves").unwrap().clone();
        let golden = pool.iter().find(|r| r.id == "golden_touch").unwrap().clone();
        run.pick_relic(&moves);
        run.pick_relic(&golden);
        assert_eq!(run.modifiers.extra_moves, 4);
        assert!((run.modifiers.score_bonus_pct - 0.30).abs() < 1e-6);
    }

    #[test]
    fn chamber_seed_varies_per_chamber() {
        let mut run = DescentRun::new(99);
        let s1 = run.chamber_seed();
        run.advance_chamber();
        let s2 = run.chamber_seed();
        assert_ne!(s1, s2);
        assert_eq!(run.chamber, 2);
    }

    #[test]
    fn full_descent_flow_three_chambers() {
        let mut run = DescentRun::new(2026);
        // Chambers 1 -> 2 -> 3: two between-chamber drafts.
        for expected in 1..=3 {
            assert_eq!(run.chamber, expected);
            let offers = run.draft_offers();
            if expected < 3 {
                assert!(!offers.is_empty());
                run.pick_relic(&offers[0]);
            }
            run.advance_chamber();
        }
        assert_eq!(run.chamber, 4);
        assert_eq!(run.relics.len(), 2);
        // Merged modifiers reflect the two picks.
        assert!(run.modifiers.extra_moves > 0 || run.modifiers.echo_extra_moves > 0
            || run.modifiers.score_bonus_pct > 0.0);
    }
}
