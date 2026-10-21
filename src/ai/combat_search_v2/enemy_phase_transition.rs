#[cfg(test)]
use super::action_effects::card_play_effect_facts;
use super::action_effects::CardPlayEffectFacts;
#[cfg(test)]
use super::enemy_mechanics_profile::enemy_mechanics_profile;
use super::enemy_mechanics_profile::EnemyMechanicsProfileV1;
use super::*;
use crate::content::cards;
#[cfg(test)]
use crate::content::powers::PowerId;
use crate::runtime::combat::CombatCard;

mod awakened_one;
mod damage_projection;
mod projection;
mod transition_rules;
pub(super) use awakened_one::{
    awakened_one_strength_transition_opportunity, AwakenedOneStrengthTransitionOpportunity,
};
use projection::{PhaseProjection, ProjectedMonsterDamage};
use transition_rules::{
    observe_champ_threshold_transition, observe_guardian_transition, observe_lagavulin_transition,
    observe_split_transition, CHAMP_THRESHOLD_DEBT_RISK_PER_HP, CHAMP_THRESHOLD_TRIGGER_RISK,
    GUARDIAN_MODE_SHIFT_TRIGGER_RISK, LAGAVULIN_WAKE_RISK, SPLIT_TRIGGER_RISK_PER_DEBT_HP,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct EnemyPhaseTransitionHint {
    /// Exact immediate damage progress from the simulator's resolved card
    /// actions, including multi-hit damage, Strength, Vulnerable, and relic
    /// modifiers.  Action ordering must not infer lethality from a card's
    /// single-hit base damage.
    pub(super) projected_damage_progress: i32,
    pub(super) projected_target_lethal: bool,
    pub(super) split_trigger_count: usize,
    pub(super) split_debt_hp: i32,
    pub(super) guardian_mode_shift_trigger_count: usize,
    pub(super) guardian_min_threshold_remaining_before_hit: Option<i32>,
    pub(super) lagavulin_wake_risk_count: usize,
    pub(super) champ_threshold_trigger_count: usize,
    pub(super) champ_threshold_debt_hp: i32,
    pub(super) awakened_one_strength_transition: Option<AwakenedOneStrengthTransitionOpportunity>,
}

#[cfg(test)]
pub(super) fn enemy_phase_transition_hint_for_input(
    combat: &CombatState,
    input: &ClientInput,
    phase_guard: CombatSearchPhaseGuardPluginId,
) -> EnemyPhaseTransitionHint {
    match input {
        ClientInput::PlayCard { card_index, target } => {
            let Some(card) = combat.zones.hand.get(*card_index) else {
                return EnemyPhaseTransitionHint::default();
            };
            enemy_phase_transition_hint_for_input_with_effects(
                combat,
                *card_index,
                *target,
                enemy_mechanics_profile(combat),
                card_play_effect_facts(combat, card, *target),
                phase_guard,
            )
        }
        _ => EnemyPhaseTransitionHint::default(),
    }
}

pub(super) fn enemy_phase_transition_hint_for_input_with_effects(
    combat: &CombatState,
    card_index: usize,
    target: Option<usize>,
    enemy_mechanics: EnemyMechanicsProfileV1,
    effects: CardPlayEffectFacts,
    phase_guard: CombatSearchPhaseGuardPluginId,
) -> EnemyPhaseTransitionHint {
    let Some(card) = combat.zones.hand.get(card_index) else {
        return EnemyPhaseTransitionHint::default();
    };
    let mut hint = play_card_phase_transition_hint(combat, card, target, phase_guard);
    hint.awakened_one_strength_transition = awakened_one_strength_transition_opportunity(
        combat,
        card_index,
        target,
        enemy_mechanics,
        effects,
    );
    hint
}

impl EnemyPhaseTransitionHint {
    pub(super) fn ordering_risk_score(self) -> i32 {
        self.split_debt_hp
            .saturating_mul(SPLIT_TRIGGER_RISK_PER_DEBT_HP)
            .saturating_add(
                (self.guardian_mode_shift_trigger_count as i32)
                    .saturating_mul(GUARDIAN_MODE_SHIFT_TRIGGER_RISK),
            )
            .saturating_add(
                (self.lagavulin_wake_risk_count as i32).saturating_mul(LAGAVULIN_WAKE_RISK),
            )
            .saturating_add(
                (self.champ_threshold_trigger_count as i32)
                    .saturating_mul(CHAMP_THRESHOLD_TRIGGER_RISK),
            )
            .saturating_add(
                self.champ_threshold_debt_hp
                    .saturating_mul(CHAMP_THRESHOLD_DEBT_RISK_PER_HP),
            )
    }
}

fn play_card_phase_transition_hint(
    combat: &CombatState,
    card: &CombatCard,
    target: Option<usize>,
    phase_guard: CombatSearchPhaseGuardPluginId,
) -> EnemyPhaseTransitionHint {
    let mut hint = EnemyPhaseTransitionHint::default();
    let evaluated = cards::evaluate_card_for_play(card, combat, target);
    let actions = cards::resolve_card_play_with_context(
        evaluated.id,
        combat,
        &evaluated,
        target,
        cards::CardUseContext {
            played_from_hand: true,
        },
    );
    let mut projection = PhaseProjection::from_combat(combat);

    damage_projection::observe_actions_damage(
        &mut projection,
        actions.into_iter().map(|action| action.action),
    );

    let target_kind = cards::effective_target(card);
    let relevant = projection
        .monsters
        .values()
        .filter(|projected| match target_kind {
            cards::CardTarget::AllEnemy => true,
            cards::CardTarget::Enemy | cards::CardTarget::SelfAndEnemy => {
                target == Some(projected.entity_id)
            }
            _ => false,
        })
        .collect::<Vec<_>>();
    hint.projected_damage_progress = relevant.iter().fold(0_i32, |total, projected| {
        let initial_block = combat
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == projected.entity_id)
            .map_or(0, |monster| monster.block);
        let before = projected.current_hp.saturating_add(initial_block);
        let after = projected
            .projected_hp
            .saturating_add(projected.projected_block);
        total.saturating_add(before.saturating_sub(after).max(0))
    });
    hint.projected_target_lethal = relevant
        .iter()
        .any(|projected| projected.current_hp > 0 && projected.projected_hp <= 0);

    for projected in projection.monsters.values() {
        observe_split_transition(&mut hint, projected);
        observe_guardian_transition(&mut hint, projected);
        observe_lagavulin_transition(&mut hint, projected);
        if phase_guard.guards_champ_split() {
            observe_champ_threshold_transition(&mut hint, projected);
        }
    }

    hint
}

#[cfg(test)]
mod tests;
