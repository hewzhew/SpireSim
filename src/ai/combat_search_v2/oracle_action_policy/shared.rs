use crate::sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper};
use crate::state::core::ClientInput;
use serde::Serialize;
pub use sts_combat_knowledge::CombatTacticalGuideBundleV1 as OracleCombatGuideBundleV1;
#[cfg(test)]
use sts_combat_knowledge::{
    combat_player_setup_summary_v1 as player_setup_summary,
    combat_player_tactical_opportunity_summary_v1,
    combat_priority_owner_progress_v1 as encounter_priority_owner_progress,
    CombatPlayerTacticalOpportunitySummaryV1 as PlayerTacticalOpportunitySummary,
    CombatPriorityOwnerProgressV1 as EncounterPriorityOwnerProgress,
};
use sts_combat_knowledge::{
    combat_tactical_action_context_v1, combat_tactical_action_ranking_enabled_v1,
    combat_tactical_guide_bundle_v1, compare_combat_tactical_action_ranks_v1,
    CombatTacticalActionPriorV1, CombatTacticalActionRankV1, CombatTacticalPhaseGuardV1,
    CombatTacticalStateValueV1,
};

use super::super::value::combat_search_state_value_for_state;

#[derive(Clone, Debug, Serialize)]
pub struct OracleAtomicActionPriorityDiagnosticV1 {
    pub role: &'static str,
    pub recoverable_resource_urgency: i32,
    pub role_rank: i32,
    pub mitigation: i32,
    pub action_supply: i32,
    pub reactive_risk: i32,
    pub target_progress: i32,
    pub block: i32,
    pub damage: i32,
    pub phase_setup: i32,
    pub phase_survival: i32,
    pub phase_transition_safety: i32,
    pub resource_timing: i32,
    pub policy_log2_bias: i32,
    pub direct_persistent_enemy_strength_down: i32,
    pub direct_temporary_enemy_strength_down: i32,
    pub direct_visible_attack_mitigation_hint: i32,
    pub direct_player_block: i32,
    pub reactive_player_block: i32,
    pub reactive_enemy_damage: i32,
    pub reactive_bad_draw_cards: i32,
}

/// Converts the existing typed action-ordering knowledge into positive,
/// relative policy weights. The caller owns normalization and the uniform
/// exploration floor.
///
/// This function does not evaluate successor states and cannot remove a legal
/// action. An input the exact stepper cannot address receives neutral weight.
pub fn oracle_atomic_action_policy_weights(
    position: &CombatPosition,
    inputs: &[ClientInput],
) -> Vec<f64> {
    oracle_atomic_action_policy_weights_from_iter(position, inputs.iter())
}

/// Borrowing counterpart used by planner adapters whose choices already hold
/// references to exact inputs.  Keeping this separate from the owned-slice
/// compatibility API avoids cloning every `ClientInput` merely to rank it.
pub fn oracle_atomic_action_policy_weights_for_refs(
    position: &CombatPosition,
    inputs: &[&ClientInput],
) -> Vec<f64> {
    oracle_atomic_action_policy_weights_from_iter(position, inputs.iter().copied())
}

/// Fast counterpart for inputs already obtained from the exact legal-action
/// surface of `position`.
///
/// Unlike the compatibility APIs above, this does not ask the stepper to
/// rediscover and validate every action or build the legacy ordering summary.
/// It uses the same typed priority and comparison rules and returns the same
/// ordinal weights. Callers must not pass speculative inputs.
pub fn oracle_legal_atomic_action_policy_weights_for_refs(
    position: &CombatPosition,
    inputs: &[&ClientInput],
) -> Vec<f64> {
    oracle_legal_atomic_action_policy_weights_from_iter(position, inputs.iter().copied())
}

fn oracle_legal_atomic_action_policy_weights_from_iter<'a, I>(
    position: &CombatPosition,
    inputs: I,
) -> Vec<f64>
where
    I: Clone + ExactSizeIterator<Item = &'a ClientInput>,
{
    let plugins = oracle_action_ordering_plugins(position);
    let ranked = inputs
        .clone()
        .enumerate()
        .map(|(original_action_id, input)| {
            (
                original_action_id,
                super::super::action_priority::priority_for_input_with_plugins(
                    &position.engine,
                    &position.combat,
                    input,
                    plugins,
                )
                .tactical_rank_v1(),
            )
        })
        .collect::<Vec<_>>();
    oracle_atomic_action_policy_weights_from_ranked_priorities(position, inputs, ranked)
}

fn oracle_atomic_action_policy_weights_from_iter<'a, I>(
    position: &CombatPosition,
    inputs: I,
) -> Vec<f64>
where
    I: Clone + ExactSizeIterator<Item = &'a ClientInput>,
{
    let stepper = EngineCombatStepper;
    let plugins = oracle_action_ordering_plugins(position);
    let ranked = inputs
        .clone()
        .enumerate()
        .filter_map(|(original_action_id, input)| {
            stepper.choice_for_legal_input(position, input).map(|_| {
                (
                    original_action_id,
                    super::super::action_priority::priority_for_input_with_plugins(
                        &position.engine,
                        &position.combat,
                        input,
                        plugins,
                    )
                    .tactical_rank_v1(),
                )
            })
        })
        .collect::<Vec<_>>();
    oracle_atomic_action_policy_weights_from_ranked_priorities(position, inputs, ranked)
}

fn oracle_atomic_action_policy_weights_from_ranked_priorities<'a, I>(
    position: &CombatPosition,
    inputs: I,
    mut ranked: Vec<(usize, CombatTacticalActionRankV1)>,
) -> Vec<f64>
where
    I: ExactSizeIterator<Item = &'a ClientInput>,
{
    if combat_tactical_action_ranking_enabled_v1(&position.engine) {
        ranked.sort_by(|(left_id, left), (right_id, right)| {
            compare_combat_tactical_action_ranks_v1(left, None, right, None)
                .then_with(|| left_id.cmp(right_id))
        });
    }

    let mut rank_by_input = vec![None; inputs.len()];
    let mut previous_priority = None;
    let mut competition_rank = 0;
    for (ordered_index, (original_action_id, priority)) in ranked.into_iter().enumerate() {
        if previous_priority.is_some_and(|previous| {
            compare_combat_tactical_action_ranks_v1(&previous, None, &priority, None)
                != std::cmp::Ordering::Equal
        }) {
            competition_rank = ordered_index;
        }
        rank_by_input[original_action_id] = Some((competition_rank, priority.policy_log2_bias));
        previous_priority = Some(priority);
    }

    rank_by_input
        .into_iter()
        .zip(inputs)
        // The source is a semantic action ordering, not a calibrated action
        // probability. Reciprocal competition rank keeps that ordering useful
        // without inventing distinctions between tied card instances or
        // mechanically symmetric targets. The caller still owns normalization
        // and its uniform exploration floor.
        .map(|(rank, input)| {
            if matches!(input, ClientInput::UsePotion { .. })
                && !super::super::potions::semantic_potion_action_allowed(&position.combat, input)
            {
                return 1.0e-6;
            }
            rank.map_or(1.0, |(rank, policy_log2_bias)| {
                oracle_rank_weight_with_semantic_bias(rank, policy_log2_bias)
            })
        })
        .collect()
}

/// Exposes the typed ordering coordinates used by the oracle action policy.
/// This is diagnostic provenance only: it neither changes the ordering nor
/// turns a heuristic preference into outcome evidence.
pub fn oracle_atomic_action_policy_priority_diagnostics_v1(
    position: &CombatPosition,
    inputs: &[ClientInput],
) -> Vec<Option<OracleAtomicActionPriorityDiagnosticV1>> {
    let stepper = EngineCombatStepper;
    inputs
        .iter()
        .map(|input| {
            stepper.choice_for_legal_input(position, input)?;
            let priority = super::super::action_priority::priority_for_input_with_plugins(
                &position.engine,
                &position.combat,
                input,
                oracle_action_ordering_plugins(position),
            );
            Some(OracleAtomicActionPriorityDiagnosticV1 {
                role: priority.role.label(),
                recoverable_resource_urgency: priority.recoverable_resource_urgency,
                role_rank: priority.role_rank,
                mitigation: priority.mitigation,
                action_supply: priority.action_supply,
                reactive_risk: priority.reactive_risk,
                target_progress: priority.target_progress,
                block: priority.block,
                damage: priority.damage,
                phase_setup: priority.phase_setup,
                phase_survival: priority.phase_survival,
                phase_transition_safety: priority.phase_transition_safety,
                resource_timing: priority.resource_timing,
                policy_log2_bias: priority.policy_log2_bias,
                direct_persistent_enemy_strength_down: priority
                    .effects
                    .direct
                    .persistent_enemy_strength_down,
                direct_temporary_enemy_strength_down: priority
                    .effects
                    .direct
                    .temporary_enemy_strength_down,
                direct_visible_attack_mitigation_hint: priority
                    .effects
                    .direct
                    .visible_attack_mitigation_hint,
                direct_player_block: priority.effects.direct.player_block,
                reactive_player_block: priority.effects.reactive.player_block,
                reactive_enemy_damage: priority.effects.reactive.enemy_damage,
                reactive_bad_draw_cards: priority.effects.reactive.bad_draw_cards,
            })
        })
        .collect()
}

fn oracle_action_ordering_plugins(
    position: &CombatPosition,
) -> super::super::CombatSearchActionOrderingPlugins<'static> {
    let context = combat_tactical_action_context_v1(position);
    let phase_guard = match context.phase_guard {
        CombatTacticalPhaseGuardV1::Default => {
            super::super::CombatSearchPhaseGuardPluginId::Default
        }
        CombatTacticalPhaseGuardV1::ChampSplit => {
            super::super::CombatSearchPhaseGuardPluginId::ChampSplitGuard
        }
        CombatTacticalPhaseGuardV1::TimeEaterClock => {
            super::super::CombatSearchPhaseGuardPluginId::TimeEaterClockHint
        }
    };
    super::super::CombatSearchActionOrderingPlugins {
        phase_guard,
        action_prior: match context.action_prior {
            CombatTacticalActionPriorV1::Default => {
                super::super::CombatSearchActionPriorPluginId::Default
            }
            CombatTacticalActionPriorV1::KeyCardOnline => {
                super::super::CombatSearchActionPriorPluginId::KeyCardOnline
            }
        },
        ..super::super::CombatSearchActionOrderingPlugins::default()
    }
}

fn oracle_combat_state_value(position: &CombatPosition) -> CombatTacticalStateValueV1 {
    combat_search_state_value_for_state(&position.engine, &position.combat)
}

/// Computes every independent guide lane from one shared state evaluation.
pub fn oracle_combat_guide_bundle_v1(position: &CombatPosition) -> OracleCombatGuideBundleV1 {
    let value = oracle_combat_state_value(position);
    combat_tactical_guide_bundle_v1(position, &value)
}

/// Reuses the mature search's typed, lexicographic state knowledge without
/// transferring ownership of its frontier or terminal claims. Components are
/// ordered exactly as `CombatTacticalStateValueV1::cmp` orders them.
pub fn oracle_combat_state_guide_components(position: &CombatPosition) -> Vec<i32> {
    oracle_combat_guide_bundle_v1(position).progress
}

/// A separate non-authoritative view of the same typed state knowledge.
/// Keeping survival independent from progress lets multi-heuristic search
/// retain healthy setup lines without inventing a conversion rate between
/// enemy progress and player HP.
pub fn oracle_combat_survival_guide_components(position: &CombatPosition) -> Vec<i32> {
    oracle_combat_guide_bundle_v1(position).survival
}

/// A non-authoritative long-horizon view for setup-heavy combats. Progress
/// and survival guides can both prefer an earlier turn forever: the former
/// because setup has not dealt damage yet, and the latter because later turns
/// have usually paid some HP. Keeping horizon in its own queue gives those
/// states service without calibrating turn depth against HP or enemy damage.
pub fn oracle_combat_horizon_guide_components(position: &CombatPosition) -> Vec<i32> {
    oracle_combat_guide_bundle_v1(position).horizon
}

/// A horizon view specifically for partial states inside one player turn.
/// The ordinary horizon guide starts with `turn_count`, which is constant
/// until EndTurn and therefore cannot help a lazy complete-turn generator
/// expose longer setup sequences.  This view rewards realized action depth
/// first, then persistent assets and concrete combat progress.  It owns only
/// one guide lane; the anchor, progress, survival, and setup lanes remain
/// independent.
pub fn oracle_combat_turn_generation_guide_components(position: &CombatPosition) -> Vec<i32> {
    oracle_combat_guide_bundle_v1(position).turn_generation
}

/// An independent view of persistent player setup. Damage-first and
/// survival-first guides both undervalue a turn which spends energy putting
/// powers in play: the enemy is still healthy and the immediate block may
/// already have expired by the next player boundary. This lane recognizes
/// the resulting exact state, rather than assigning bonuses to the actions
/// which happened to create it.
pub fn oracle_combat_setup_guide_components(position: &CombatPosition) -> Vec<i32> {
    oracle_combat_guide_bundle_v1(position).setup
}

#[cfg(test)]
fn player_tactical_opportunity_summary(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
) -> PlayerTacticalOpportunitySummary {
    combat_player_tactical_opportunity_summary_v1(position, value.survival_margin)
}

#[cfg(test)]
fn dark_embrace_wound_access_conversion_ready(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
) -> bool {
    player_tactical_opportunity_summary(position, value).dark_embrace_wound_access_conversion != 0
}

#[cfg(test)]
fn runic_cube_emergency_draw_conversion_ready(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
) -> bool {
    player_tactical_opportunity_summary(position, value).runic_cube_emergency_draw_conversion != 0
}

fn oracle_ordinal_rank_weight(rank: usize) -> f64 {
    1.0 / rank.saturating_add(1) as f64
}

fn oracle_rank_weight_with_semantic_bias(rank: usize, policy_log2_bias: i32) -> f64 {
    const MAX_ABS_POLICY_LOG2_BIAS: i32 = 20;
    oracle_ordinal_rank_weight(rank)
        * 2.0_f64.powi(policy_log2_bias.clamp(-MAX_ABS_POLICY_LOG2_BIAS, MAX_ABS_POLICY_LOG2_BIAS))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
