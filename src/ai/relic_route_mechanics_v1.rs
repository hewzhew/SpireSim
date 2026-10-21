//! Typed relic mechanics whose value depends on visible route timing.
//!
//! This module owns mechanical scope only. It does not rank relics, routes, or
//! event choices.

use serde::{Deserialize, Serialize};

use crate::content::relics::{RelicId, RelicState};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitedCombatRouteEffectKindV1 {
    SetEnemiesToOneHpAtBattleStart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitedCombatRouteEffectV1 {
    pub affected_combat_count: usize,
    pub effect: LimitedCombatRouteEffectKindV1,
}

pub fn limited_combat_route_effect_v1(relic: RelicId) -> Option<LimitedCombatRouteEffectV1> {
    match relic {
        RelicId::NeowsLament => {
            usize::try_from(RelicState::new(relic).counter)
                .ok()
                .map(|affected_combat_count| LimitedCombatRouteEffectV1 {
                    affected_combat_count,
                    effect: LimitedCombatRouteEffectKindV1::SetEnemiesToOneHpAtBattleStart,
                })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neows_lament_exposes_its_bounded_combat_scope_without_policy() {
        let facts = limited_combat_route_effect_v1(RelicId::NeowsLament)
            .expect("Neow's Lament route mechanics");

        assert_eq!(facts.affected_combat_count, 3);
        assert_eq!(
            facts.effect,
            LimitedCombatRouteEffectKindV1::SetEnemiesToOneHpAtBattleStart
        );
        assert_eq!(limited_combat_route_effect_v1(RelicId::BurningBlood), None);
    }
}
