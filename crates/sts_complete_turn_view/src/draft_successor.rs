//! Small public resource projection of a retained successor. No replay or ranking.
use std::fmt::Write;

use serde::Serialize;
use sts_core::content::potions::PotionId;
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

pub fn render_combat_draft_successor_v1(
    state: &CombatDraftSuccessorSummaryV1,
    locale: sts_presentation::Locale,
) -> String {
    let text = |key: &str, values: &[(&str, &str)]| crate::draft_message(locale, key, values);
    let mut output = text(
        "hp",
        &[
            ("hp", &state.hp.to_string()),
            ("max", &state.max_hp.to_string()),
        ],
    );
    if state.block != 0 {
        output.push_str(&text("block", &[("block", &state.block.to_string())]));
    }
    for monster in &state.monsters {
        write!(output, " | M{} ", monster.slot).unwrap();
        if monster.escaped {
            output.push_str(&text("escaped", &[]));
        } else if monster.half_dead {
            output.push_str(&text("reviving", &[]));
        } else if !monster.alive {
            output.push_str(&text("defeated", &[]));
        } else {
            write!(output, "{}/{}", monster.hp, monster.max_hp).unwrap();
            if monster.block != 0 {
                output.push_str(&text(
                    "monsterBlock",
                    &[("block", &monster.block.to_string())],
                ));
            }
        }
    }
    let empty = text("empty", &[]);
    let potions = if state.potions.iter().all(Option::is_none) {
        empty.clone()
    } else {
        state
            .potions
            .iter()
            .enumerate()
            .map(|(slot, potion)| {
                format!(
                    "P{slot} {}",
                    potion.map_or(empty.as_str(), |id| locale.entity("potion", id.stable_id()))
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    output.push_str(&text("potions", &[("potions", &potions)]));
    output
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
        let text = render_combat_draft_successor_v1(&summary, sts_presentation::Locale::En);
        assert!(text.contains("M0 defeated"));
        assert!(text.contains("M2 reviving"));
        assert!(text.contains("M4 30/30 +7 Block"));
        assert!(text.contains("P0 empty, P1 Fairy in a Bottle"));
        assert!(!text.contains("Block 0"));
        let original = summary.clone();
        let chinese = render_combat_draft_successor_v1(&summary, sts_presentation::Locale::ZhCn);
        assert!(chinese.contains("M0 已击败") && chinese.contains("M2 复活中"));
        assert!(chinese.contains("M4 30/30 +7 格挡"));
        assert!(chinese.contains(
            sts_presentation::Locale::ZhCn.entity("potion", PotionId::FairyPotion.stable_id())
        ));
        assert_eq!(summary, original);
    }
}
