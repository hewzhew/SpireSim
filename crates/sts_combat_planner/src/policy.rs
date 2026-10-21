use std::sync::Arc;

use sts_combat_strategy::{
    combat_plan_selection_member_timing_v1, combat_plan_state_guide_rank_v1,
    CombatPlanActionTimingV1,
};
pub use sts_combat_tactics::{
    CombatActionPolicy, CombatGuideLaneId, CombatGuideServiceBias, CombatPolicyChoice,
    CombatSelectionPreferences, CombatStateGuide, CombatStateGuideRank, SharedCombatActionPolicy,
    UniformCombatActionPolicy,
};
use sts_core::sim::combat::CombatPosition;
use sts_core::sim::combat_action_surface::CombatSelectionActionFamilyV2;
use sts_core::state::core::ClientInput;

/// Adds one plan-owned turn-boundary guide without changing action generation.
///
/// This is intentionally not a turn-generation guide: the plan may describe
/// a desirable cross-turn state without knowing which mid-turn investment
/// actions reach it. The wrapped action weights and partial-state guides remain
/// byte-for-byte authoritative inside a single exact step.
pub struct CombatPlanStateGuidePolicyV1 {
    base: SharedCombatActionPolicy,
}

/// Orders concrete members of a structured selection using encounter-owned
/// plan timing without adding any state-guide lane.
///
/// This is separate from [`CombatPlanStateGuidePolicyV1`] so callers can
/// distinguish cross-turn state guidance from within-choice action ordering.
pub struct CombatPlanSelectionTimingPolicyV1 {
    base: SharedCombatActionPolicy,
}

impl CombatPlanStateGuidePolicyV1 {
    pub fn new(base: SharedCombatActionPolicy) -> Self {
        Self { base }
    }
}

impl CombatPlanSelectionTimingPolicyV1 {
    pub fn new(base: SharedCombatActionPolicy) -> Self {
        Self { base }
    }
}

impl CombatActionPolicy for CombatPlanStateGuidePolicyV1 {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        self.base.weights(position, choices)
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_tactics::CombatSelectionPreferences {
        self.base.structured_selection_preferences(position, family)
    }

    fn structured_selection_member_weights(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        self.base
            .structured_selection_member_weights(position, family, members)
    }

    fn state_guide_rank(&self, position: &CombatPosition) -> Option<CombatStateGuideRank> {
        self.base.state_guide_rank(position)
    }

    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        let mut guides = self.base.state_guides(position);
        if let Some(rank) = combat_plan_state_guide_rank_v1(position) {
            guides.push(CombatStateGuide::new(
                COMBAT_PLAN_STATE_GUIDE_LANE_V1,
                rank.components().to_vec(),
            ));
        }
        guides
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.base.turn_generation_guides(position)
    }
}

impl CombatActionPolicy for CombatPlanSelectionTimingPolicyV1 {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        self.base.weights(position, choices)
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_tactics::CombatSelectionPreferences {
        self.base.structured_selection_preferences(position, family)
    }

    fn structured_selection_member_weights(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        let mut weights = self
            .base
            .structured_selection_member_weights(position, family, members);
        if weights.len() != members.len() || members.is_empty() {
            return weights;
        }
        let timings = members
            .iter()
            .map(|member| combat_plan_selection_member_timing_v1(position, family, member))
            .collect::<Vec<_>>();
        let has_compatible_member = timings
            .iter()
            .any(|timing| !matches!(timing, CombatPlanActionTimingV1::Defer(_)));
        if !has_compatible_member {
            return weights;
        }
        let compatible_floor = weights
            .iter()
            .zip(&timings)
            .filter_map(|(weight, timing)| {
                (!matches!(timing, CombatPlanActionTimingV1::Defer(_))
                    && weight.is_finite()
                    && *weight > 0.0)
                    .then_some(*weight)
            })
            .min_by(f64::total_cmp)
            .unwrap_or(1.0);
        for (weight, timing) in weights.iter_mut().zip(timings) {
            if matches!(timing, CombatPlanActionTimingV1::Defer(_)) {
                // Categorical ordering only: every compatible member remains
                // ahead of a member which destroys a plan-owned resource.
                // The deferred member keeps positive mass and is never
                // removed from exact search.
                *weight = compatible_floor * 0.5;
            }
        }
        weights
    }

    fn state_guide_rank(&self, position: &CombatPosition) -> Option<CombatStateGuideRank> {
        self.base.state_guide_rank(position)
    }

    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.base.state_guides(position)
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.base.turn_generation_guides(position)
    }
}

pub fn combat_plan_state_guide_policy_v1(
    base: SharedCombatActionPolicy,
) -> SharedCombatActionPolicy {
    Arc::new(CombatPlanStateGuidePolicyV1::new(base))
}

pub fn combat_plan_selection_timing_policy_v1(
    base: SharedCombatActionPolicy,
) -> SharedCombatActionPolicy {
    Arc::new(CombatPlanSelectionTimingPolicyV1::new(base))
}

pub const COMBAT_PLAN_STATE_GUIDE_LANE_V1: CombatGuideLaneId = CombatGuideLaneId::new(0x4350_0001);

pub(crate) fn uniform_policy() -> SharedCombatActionPolicy {
    Arc::new(UniformCombatActionPolicy)
}

pub(crate) fn normalized_probabilities(
    weights: impl IntoIterator<Item = f64>,
    uniform_exploration_ppm: u32,
) -> Vec<f64> {
    let weights = weights
        .into_iter()
        .map(|weight| {
            if weight.is_finite() && weight > 0.0 {
                weight
            } else {
                1.0
            }
        })
        .collect::<Vec<_>>();
    if weights.is_empty() {
        return Vec::new();
    }
    let total = weights.iter().sum::<f64>();
    let uniform = 1.0 / weights.len() as f64;
    let epsilon = (uniform_exploration_ppm.min(1_000_000) as f64) / 1_000_000.0;
    weights
        .into_iter()
        .map(|weight| {
            ((1.0 - epsilon) * (weight / total) + epsilon * uniform).max(f64::MIN_POSITIVE)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;
    use sts_core::content::monsters::EnemyId;
    use sts_core::runtime::combat::CombatCard;
    use sts_core::state::core::{EngineState, HandSelectReason, PendingChoice};
    use sts_core::state::selection::{SelectionResolution, SelectionScope};
    use sts_core::test_support::{blank_test_combat, test_monster};

    #[test]
    fn mixed_distribution_is_positive_and_normalized() {
        let distribution = normalized_probabilities([100.0, 1.0, 0.0, f64::NAN], 50_000);
        assert!(distribution.iter().all(|probability| *probability > 0.0));
        assert!((distribution.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
        assert!(distribution[0] > distribution[1]);
    }

    #[test]
    fn full_uniform_mix_ignores_expert_weight() {
        let distribution = normalized_probabilities([100.0, 1.0], 1_000_000);
        assert_eq!(distribution, vec![0.5, 0.5]);
    }

    #[test]
    fn plan_state_guide_policy_does_not_reorder_mid_turn_actions() {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.slot = 2;
        combat.entities.monsters.push(awakened);
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Corruption, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let corruption = ClientInput::PlayCard {
            card_index: 0,
            target: None,
        };
        let defend = ClientInput::PlayCard {
            card_index: 1,
            target: None,
        };
        let policy = combat_plan_state_guide_policy_v1(Arc::new(UniformCombatActionPolicy));

        let weights = policy.weights(
            &position,
            &[
                CombatPolicyChoice::Atomic(&corruption),
                CombatPolicyChoice::Atomic(&defend),
            ],
        );

        assert_eq!(weights, vec![1.0, 1.0]);
        assert!(policy.turn_generation_guides(&position).is_empty());
        let guides = policy.state_guides(&position);
        assert_eq!(guides.len(), 1);
        assert_eq!(guides[0].lane, COMBAT_PLAN_STATE_GUIDE_LANE_V1);
    }

    #[test]
    fn plan_state_guide_policy_preserves_structured_selection_ordering() {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.id = 10;
        awakened.slot = 2;
        combat.entities.monsters.push(awakened);
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::DemonForm, 31),
            CombatCard::for_scenario(CardId::Strike, 32),
        ];
        let position = CombatPosition::new(
            EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages: sts_core::test_support::scenario_lineages([31, 32]),
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: HandSelectReason::Exhaust,
            }),
            combat,
        );
        let surface = sts_core::sim::combat_action_surface::combat_legal_action_surface_v2(
            &position.engine,
            &position.combat,
        );
        let family = surface
            .selection_families
            .first()
            .expect("forced exhaust family");
        let members = vec![
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                [sts_core::test_support::scenario_lineage(31)],
            )),
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                [sts_core::test_support::scenario_lineage(32)],
            )),
        ];
        let policy = combat_plan_state_guide_policy_v1(Arc::new(UniformCombatActionPolicy));

        let weights = policy.structured_selection_member_weights(&position, family, &members);

        assert_eq!(weights, vec![1.0, 1.0]);
    }

    #[test]
    fn plan_selection_timing_orders_compatible_exhaust_without_adding_guides() {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.id = 10;
        awakened.slot = 2;
        combat.entities.monsters.push(awakened);
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::DemonForm, 31),
            CombatCard::for_scenario(CardId::Strike, 32),
        ];
        let position = CombatPosition::new(
            EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages: sts_core::test_support::scenario_lineages([31, 32]),
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: HandSelectReason::Exhaust,
            }),
            combat,
        );
        let surface = sts_core::sim::combat_action_surface::combat_legal_action_surface_v2(
            &position.engine,
            &position.combat,
        );
        let family = surface
            .selection_families
            .first()
            .expect("forced exhaust family");
        let members = vec![
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                [sts_core::test_support::scenario_lineage(31)],
            )),
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                [sts_core::test_support::scenario_lineage(32)],
            )),
        ];
        let policy = combat_plan_selection_timing_policy_v1(Arc::new(UniformCombatActionPolicy));

        let weights = policy.structured_selection_member_weights(&position, family, &members);

        assert!(weights[1] > weights[0]);
        assert!(weights[0] > 0.0);
        assert!(policy.state_guides(&position).is_empty());
        assert!(policy.turn_generation_guides(&position).is_empty());
    }
}
