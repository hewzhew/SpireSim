use super::super::context::PotionPlanningContext;
use super::super::decision::{PotionGateDecision, PotionGateReason, PotionTacticalRole};
use super::super::semantics::PotionArea;
use crate::runtime::combat::CombatState;

pub(super) fn direct_damage_gate(
    combat: &CombatState,
    context: PotionPlanningContext,
    target: Option<usize>,
    amount: i32,
    area: PotionArea,
) -> PotionGateDecision {
    match area {
        PotionArea::SingleEnemy => {
            let Some(target) = target else {
                return PotionGateDecision::reject(PotionGateReason::InvalidTarget);
            };
            let Some(monster) = combat
                .entities
                .monsters
                .iter()
                .find(|monster| monster.id == target && monster.is_alive_for_action())
            else {
                return PotionGateDecision::reject(PotionGateReason::InvalidTarget);
            };
            if amount >= monster.current_hp + monster.block {
                PotionGateDecision::allow(
                    PotionGateReason::DirectDamageCanKill,
                    PotionTacticalRole::LethalDamage,
                )
            } else {
                pressure_gate(context)
            }
        }
        PotionArea::AllEnemies => {
            let can_kill = combat
                .entities
                .monsters
                .iter()
                .filter(|monster| monster.is_alive_for_action())
                .any(|monster| amount >= monster.current_hp + monster.block);
            if can_kill {
                PotionGateDecision::allow(
                    PotionGateReason::DirectDamageCanKill,
                    PotionTacticalRole::LethalDamage,
                )
            } else {
                pressure_gate(context)
            }
        }
    }
}

pub(super) fn enemy_power_gate(
    combat: &CombatState,
    context: PotionPlanningContext,
    target: Option<usize>,
) -> PotionGateDecision {
    if !target_points_to_live_enemy(combat, target) {
        return PotionGateDecision::reject(PotionGateReason::InvalidTarget);
    }
    pressure_gate(context)
}

pub(super) fn block_gate(context: PotionPlanningContext) -> PotionGateDecision {
    if context.visible_attack_is_lethal {
        PotionGateDecision::allow(
            PotionGateReason::VisibleIncomingLethal,
            PotionTacticalRole::PreventVisibleLethal,
        )
    } else if context.has_uncovered_visible_hp_loss() {
        if context.high_stakes_combat {
            PotionGateDecision::allow(
                PotionGateReason::VisibleIncomingUncoveredByHandBlock,
                PotionTacticalRole::PreventUncoveredDamage,
            )
        } else {
            PotionGateDecision::reject(PotionGateReason::NoTacticalPressure)
        }
    } else if context.visible_hp_loss {
        PotionGateDecision::reject(PotionGateReason::VisibleIncomingFullyBlockable)
    } else {
        PotionGateDecision::reject(PotionGateReason::NoVisibleIncomingHpLoss)
    }
}

pub(super) fn wounded_resource_gate(context: PotionPlanningContext) -> PotionGateDecision {
    if !context.player_is_wounded() {
        return PotionGateDecision::reject(PotionGateReason::NotWounded);
    }
    pressure_gate(context)
}

pub(super) fn max_hp_gate(context: PotionPlanningContext) -> PotionGateDecision {
    if context.visible_attack_is_lethal {
        PotionGateDecision::allow(
            PotionGateReason::VisibleIncomingLethal,
            PotionTacticalRole::PreventVisibleLethal,
        )
    } else if context.has_uncovered_visible_hp_loss() {
        if context.high_stakes_combat {
            PotionGateDecision::allow(
                PotionGateReason::VisibleIncomingUncoveredByHandBlock,
                PotionTacticalRole::PreventUncoveredDamage,
            )
        } else {
            PotionGateDecision::reject(PotionGateReason::NoTacticalPressure)
        }
    } else if context.player_is_wounded() && context.high_stakes_combat {
        PotionGateDecision::allow(
            PotionGateReason::PlayerWounded,
            PotionTacticalRole::SustainResource,
        )
    } else {
        PotionGateDecision::reject(PotionGateReason::NoTacticalPressure)
    }
}

pub(super) fn resource_conversion_gate(context: PotionPlanningContext) -> PotionGateDecision {
    if !context.has_living_enemy() {
        PotionGateDecision::reject(PotionGateReason::NoLivingEnemy)
    } else if context.visible_attack_is_lethal {
        PotionGateDecision::allow(
            PotionGateReason::VisibleIncomingLethal,
            PotionTacticalRole::PreventVisibleLethal,
        )
    } else if context.has_uncovered_visible_hp_loss() {
        PotionGateDecision::allow(
            PotionGateReason::VisibleIncomingUncoveredByHandBlock,
            PotionTacticalRole::PreventUncoveredDamage,
        )
    } else if context.high_stakes_combat && context.lacks_visible_lethal() {
        PotionGateDecision::allow(
            PotionGateReason::HighStakesNoVisibleHandLethal,
            PotionTacticalRole::HighStakesResourceConversion,
        )
    } else if context.visible_hp_loss {
        PotionGateDecision::reject(PotionGateReason::VisibleIncomingFullyBlockable)
    } else {
        PotionGateDecision::reject(PotionGateReason::NoTacticalPressure)
    }
}

pub(super) fn pressure_gate(context: PotionPlanningContext) -> PotionGateDecision {
    if !context.has_living_enemy() {
        PotionGateDecision::reject(PotionGateReason::NoLivingEnemy)
    } else if context.visible_attack_is_lethal {
        PotionGateDecision::allow(
            PotionGateReason::VisibleIncomingLethal,
            PotionTacticalRole::PreventVisibleLethal,
        )
    } else if context.has_uncovered_visible_hp_loss() {
        if context.high_stakes_combat {
            PotionGateDecision::allow(
                PotionGateReason::VisibleIncomingUncoveredByHandBlock,
                PotionTacticalRole::PreventUncoveredDamage,
            )
        } else {
            PotionGateDecision::reject(PotionGateReason::NoTacticalPressure)
        }
    } else if context.high_stakes_combat && context.lacks_visible_lethal() {
        PotionGateDecision::allow(
            PotionGateReason::HighStakesNoVisibleHandLethal,
            PotionTacticalRole::HighStakesResourceConversion,
        )
    } else if context.visible_hp_loss {
        PotionGateDecision::reject(PotionGateReason::VisibleIncomingFullyBlockable)
    } else {
        PotionGateDecision::reject(PotionGateReason::NoTacticalPressure)
    }
}

fn target_points_to_live_enemy(combat: &CombatState, target: Option<usize>) -> bool {
    let Some(target) = target else {
        return false;
    };
    combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.id == target && monster.is_alive_for_action())
}
