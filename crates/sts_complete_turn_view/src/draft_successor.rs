//! Small public resource projection of a retained successor. No replay or ranking.
use std::fmt::Write;

use serde::Serialize;
use sts_core::content::{identity::gameplay_display_label_v1, potions::PotionId};
use sts_core::runtime::combat::CombatState;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CombatDraftSuccessorSummaryV1 {
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    /// Public monster slots are retained, including defeated and reviving enemies.
    pub monsters: Vec<CombatDraftMonsterSummaryV1>,
    /// Remaining contents in slot order, not a claim about potion consumption.
    pub potions: Vec<Option<PotionId>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CombatDraftMonsterSummaryV1 {
    pub slot: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub escaped: bool,
    pub half_dead: bool,
}

pub(crate) fn project_draft_successor(state: &CombatState) -> CombatDraftSuccessorSummaryV1 {
    CombatDraftSuccessorSummaryV1 {
        hp: state.entities.player.current_hp,
        max_hp: state.entities.player.max_hp,
        block: state.entities.player.block,
        monsters: state
            .entities
            .monsters
            .iter()
            .map(|monster| CombatDraftMonsterSummaryV1 {
                slot: monster.slot,
                hp: monster.current_hp,
                max_hp: monster.max_hp,
                block: monster.block,
                alive: monster.is_alive_for_action(),
                escaped: monster.is_escaped,
                half_dead: monster.half_dead,
            })
            .collect(),
        potions: state
            .entities
            .potions
            .iter()
            .map(|p| p.as_ref().map(|p| p.id))
            .collect(),
    }
}

pub fn render_combat_draft_successor_v1(state: &CombatDraftSuccessorSummaryV1) -> String {
    let mut text = format!("HP {}/{}", state.hp, state.max_hp);
    if state.block != 0 {
        write!(text, " | Block {}", state.block).unwrap();
    }
    for monster in &state.monsters {
        write!(text, " | M{} ", monster.slot).unwrap();
        if monster.escaped {
            text.push_str("escaped");
        } else if monster.half_dead {
            text.push_str("reviving");
        } else if !monster.alive {
            text.push_str("defeated");
        } else {
            write!(text, "{}/{}", monster.hp, monster.max_hp).unwrap();
            if monster.block != 0 {
                write!(text, " +{} Block", monster.block).unwrap();
            }
        }
    }
    text.push_str(" | Potions: ");
    if state.potions.iter().all(Option::is_none) {
        text.push_str("empty");
    } else {
        for (slot, potion) in state.potions.iter().enumerate() {
            if slot > 0 {
                text.push_str(", ");
            }
            write!(
                text,
                "P{slot} {}",
                potion.map_or("empty", gameplay_display_label_v1)
            )
            .unwrap();
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{monsters::EnemyId, potions::Potion};
    use sts_core::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn resource_reading_preserves_slots_and_distinguishes_reviving_from_defeated() {
        let mut combat = blank_test_combat();
        let mut defeated = planned_monster(EnemyId::Cultist, 0);
        defeated.slot = 0;
        defeated.current_hp = 0;
        defeated.is_dying = true;
        let mut reviving = planned_monster(EnemyId::AwakenedOne, 0);
        reviving.slot = 2;
        reviving.current_hp = 0;
        reviving.half_dead = true;
        let mut alive = planned_monster(EnemyId::Cultist, 1);
        alive.slot = 4;
        alive.current_hp = 30;
        alive.max_hp = 30;
        alive.block = 7;
        combat.entities.monsters = vec![defeated, reviving, alive];
        combat.entities.potions = vec![None, Some(Potion::new(PotionId::FairyPotion, 1))];
        let summary = project_draft_successor(&combat);
        assert_eq!(
            summary.monsters.iter().map(|m| m.slot).collect::<Vec<_>>(),
            [0, 2, 4]
        );
        let text = render_combat_draft_successor_v1(&summary);
        assert!(text.contains("M0 defeated"));
        assert!(text.contains("M2 reviving"));
        assert!(text.contains("M4 30/30 +7 Block"));
        assert!(text.contains("P0 empty, P1 Fairy in a Bottle"));
        assert!(!text.contains("Block 0"));
    }
}
