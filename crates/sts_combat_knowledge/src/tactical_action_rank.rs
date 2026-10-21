//! Typed ordering coordinates for one already legal combat action.
//!
//! Coordinate calculation remains with mechanics-aware producers while it
//! depends on atomic-private facts. This module owns the ordering contract and
//! the exact point where an optional root prior may break otherwise semantic
//! ties. It cannot alter legality or execute an action.

use std::cmp::Ordering;

use sts_core::state::core::EngineState;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTacticalActionRankV1 {
    pub recoverable_resource_urgency: i32,
    pub role_rank: i32,
    pub lethal_external_payoff: i32,
    pub potion_tactical_rank: i32,
    pub mitigation: i32,
    pub action_supply: i32,
    pub reactive_risk: i32,
    pub targets_timed_threat: i32,
    pub timed_threat_urgency: i32,
    pub timed_threat_raw_damage: i32,
    pub target_progress: i32,
    pub block: i32,
    pub damage: i32,
    pub cheaper_cost: i32,
    pub phase_setup: i32,
    pub phase_survival: i32,
    pub phase_transition_safety: i32,
    pub resource_timing: i32,
    pub pending_choice_primary: i32,
    pub pending_choice_secondary: i32,
    pub pending_choice_selected_count: i32,
    /// Signed base-two multiplier applied after ordinal rank becomes a
    /// positive policy weight. It deliberately does not participate in `Ord`.
    pub policy_log2_bias: i32,
}

/// Whether the shared tactical action-ranking contract applies at this engine
/// boundary. Other phases preserve the exact legal-action order supplied by
/// their owner.
pub fn combat_tactical_action_ranking_enabled_v1(engine: &EngineState) -> bool {
    matches!(
        engine,
        EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
    )
}

impl Ord for CombatTacticalActionRankV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.recoverable_resource_urgency
            .cmp(&other.recoverable_resource_urgency)
            .then_with(|| self.role_rank.cmp(&other.role_rank))
            .then_with(|| {
                self.lethal_external_payoff
                    .cmp(&other.lethal_external_payoff)
            })
            .then_with(|| self.potion_tactical_rank.cmp(&other.potion_tactical_rank))
            .then_with(|| self.mitigation.cmp(&other.mitigation))
            .then_with(|| self.action_supply.cmp(&other.action_supply))
            .then_with(|| self.reactive_risk.cmp(&other.reactive_risk))
            .then_with(|| self.targets_timed_threat.cmp(&other.targets_timed_threat))
            .then_with(|| self.timed_threat_urgency.cmp(&other.timed_threat_urgency))
            .then_with(|| {
                self.timed_threat_raw_damage
                    .cmp(&other.timed_threat_raw_damage)
            })
            .then_with(|| self.phase_setup.cmp(&other.phase_setup))
            .then_with(|| self.phase_survival.cmp(&other.phase_survival))
            .then_with(|| {
                self.phase_transition_safety
                    .cmp(&other.phase_transition_safety)
            })
            .then_with(|| self.resource_timing.cmp(&other.resource_timing))
            .then_with(|| self.target_progress.cmp(&other.target_progress))
            .then_with(|| self.block.cmp(&other.block))
            .then_with(|| self.damage.cmp(&other.damage))
            .then_with(|| self.cheaper_cost.cmp(&other.cheaper_cost))
            .then_with(|| {
                self.pending_choice_primary
                    .cmp(&other.pending_choice_primary)
            })
            .then_with(|| {
                self.pending_choice_secondary
                    .cmp(&other.pending_choice_secondary)
            })
            .then_with(|| {
                self.pending_choice_selected_count
                    .cmp(&other.pending_choice_selected_count)
            })
    }
}

impl PartialOrd for CombatTacticalActionRankV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Comparison used by ascending Rust sorts to place the better action first.
///
/// A root prior may distinguish actions only after the three leading semantic
/// coordinates tie. Remaining semantic coordinates always remain the final
/// deterministic comparison surface.
pub fn compare_combat_tactical_action_ranks_v1(
    left: &CombatTacticalActionRankV1,
    left_root_action_prior_score: Option<f64>,
    right: &CombatTacticalActionRankV1,
    right_root_action_prior_score: Option<f64>,
) -> Ordering {
    right
        .recoverable_resource_urgency
        .cmp(&left.recoverable_resource_urgency)
        .then_with(|| right.role_rank.cmp(&left.role_rank))
        .then_with(|| {
            right
                .lethal_external_payoff
                .cmp(&left.lethal_external_payoff)
        })
        .then_with(|| {
            compare_prior_scores(right_root_action_prior_score, left_root_action_prior_score)
        })
        .then_with(|| right.cmp(left))
}

fn compare_prior_scores(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_bias_is_not_an_ordering_coordinate() {
        let left = CombatTacticalActionRankV1 {
            policy_log2_bias: -10,
            ..CombatTacticalActionRankV1::default()
        };
        let right = CombatTacticalActionRankV1 {
            policy_log2_bias: 10,
            ..CombatTacticalActionRankV1::default()
        };

        assert_eq!(left.cmp(&right), Ordering::Equal);
        assert_eq!(
            compare_combat_tactical_action_ranks_v1(&left, None, &right, None),
            Ordering::Equal
        );
    }

    #[test]
    fn root_prior_intervenes_only_after_leading_semantic_coordinates_tie() {
        let semantic_leader = CombatTacticalActionRankV1 {
            recoverable_resource_urgency: 1,
            ..CombatTacticalActionRankV1::default()
        };
        let favored_by_prior = CombatTacticalActionRankV1::default();
        assert_eq!(
            compare_combat_tactical_action_ranks_v1(
                &semantic_leader,
                Some(0.0),
                &favored_by_prior,
                Some(1.0),
            ),
            Ordering::Less
        );

        let later_semantic_leader = CombatTacticalActionRankV1 {
            mitigation: 1,
            ..CombatTacticalActionRankV1::default()
        };
        assert_eq!(
            compare_combat_tactical_action_ranks_v1(
                &later_semantic_leader,
                Some(0.0),
                &favored_by_prior,
                Some(1.0),
            ),
            Ordering::Greater
        );
    }
}
