//! Mechanics-aware tactical state facts, decorators, and guide controls.
//!
//! Base tactical implementations live in their own compilation owner. This
//! crate can wrap any neutral combat policy but owns no search or rollout.

mod tactical_action_context;
mod tactical_action_rank;
mod tactical_card_pile_facts;
mod tactical_enemy_mitigation_facts;
mod tactical_enemy_phase_value;
mod tactical_pressure_facts;
mod tactical_state_facts;
mod tactical_state_guides;
mod tactical_timed_enemy_threat;

pub use tactical_action_context::{
    combat_tactical_action_context_v1, CombatTacticalActionContextV1, CombatTacticalActionPriorV1,
    CombatTacticalPhaseGuardV1,
};
pub use tactical_action_rank::{
    combat_tactical_action_ranking_enabled_v1, compare_combat_tactical_action_ranks_v1,
    CombatTacticalActionRankV1,
};
pub use tactical_card_pile_facts::{
    combat_tactical_choker_capacity_v1, combat_tactical_hand_value_v1,
    combat_tactical_next_draw_value_v1, CombatTacticalCardPileValueV1,
    CombatTacticalChokerCapacityV1,
};
pub use tactical_enemy_mitigation_facts::{
    combat_tactical_monster_attack_relevance_v1, combat_tactical_sustained_mitigation_score_v1,
};
pub use tactical_enemy_phase_value::{
    combat_tactical_enemy_phase_value_v1, CombatTacticalEnemyPhaseValueV1,
};
pub use tactical_pressure_facts::{
    combat_tactical_pressure_v1, combat_tactical_visible_incoming_damage_v1,
    combat_tactical_visible_incoming_damage_with_player_intangible_v1, CombatTacticalPressureV1,
};
pub use tactical_state_facts::{
    combat_player_setup_summary_v1, combat_player_tactical_opportunity_summary_v1,
    combat_priority_owner_progress_v1, connected_second_wind_wound_engine_v1,
    CombatPlayerSetupSummaryV1, CombatPlayerTacticalOpportunitySummaryV1,
    CombatPriorityOwnerProgressV1,
};
pub use tactical_state_guides::{
    combat_tactical_guide_bundle_v1, CombatTacticalGuideBundleV1, CombatTacticalStateValueV1,
};
pub use tactical_timed_enemy_threat::{
    combat_tactical_timed_enemy_threat_for_target_v1, combat_tactical_timed_enemy_threats_v1,
    CombatTacticalTimedEnemyThreatKindV1, CombatTacticalTimedEnemyThreatV1,
};

use std::sync::Arc;

use sts_combat_tactics::{
    CombatActionPolicy, CombatGuideServiceBias, CombatPolicyChoice, CombatStateGuide,
    CombatStateGuideRank, SharedCombatActionPolicy, COMBAT_FEED_PAYOFF_GUIDE_LANE_V1,
    COMBAT_SURVIVAL_GUIDE_LANE_V1,
};
use sts_core::content::cards::{evaluate_card_for_play, CardId};
use sts_core::content::monsters::EnemyId;
use sts_core::sim::combat::CombatPosition;
use sts_core::sim::combat_action_surface::CombatSelectionActionFamilyV2;
use sts_core::state::core::ClientInput;

/// Gives an owner-authorized concrete potion lane one real root challenge.
///
/// The ordinary policy deliberately assigns near-zero mass to potion actions
/// which lack an immediate tactical reason. That remains correct for conserved
/// and mixed search, but it can make a later exact-slot quality lane repeat the
/// no-potion search without meaningfully testing the resource it was opened
/// for. This wrapper changes only action guidance at the unchanged lane root;
/// legality, later timing, witness replay, and owner acceptance stay exact.
pub struct AuthorizedPotionTrialPolicyV1 {
    base: SharedCombatActionPolicy,
    root: CombatPosition,
    allowed_potion_slots: u64,
}

/// Adds one independent, explicitly requested Feed-payoff view without
/// changing legal actions, base action weights, survival guidance, or terminal
/// acceptance. This is a laboratory decorator: ordinary production combat
/// search never installs it implicitly.
pub struct FeedPayoffGuidePolicyV1 {
    base: SharedCombatActionPolicy,
    root_max_hp: i32,
}

pub fn feed_payoff_guide_policy_v1(
    base: SharedCombatActionPolicy,
    root_max_hp: i32,
) -> SharedCombatActionPolicy {
    Arc::new(FeedPayoffGuidePolicyV1 { base, root_max_hp })
}

impl FeedPayoffGuidePolicyV1 {
    fn guide(&self, position: &CombatPosition) -> CombatStateGuide {
        let combat = &position.combat;
        let gained_max_hp = combat
            .entities
            .player
            .max_hp
            .saturating_sub(self.root_max_hp)
            .max(0);
        let live_feed_count = combat
            .zones
            .hand
            .iter()
            .chain(combat.zones.draw_pile.iter())
            .chain(combat.zones.discard_pile.iter())
            .filter(|card| card.id == CardId::Feed)
            .count() as i32;
        let feed_in_hand = combat
            .zones
            .hand
            .iter()
            .find(|card| card.id == CardId::Feed);
        let minimum_feed_gap = feed_in_hand.and_then(|feed| {
            combat
                .entities
                .monsters
                .iter()
                .filter(|monster| monster.is_alive_for_action())
                .map(|monster| {
                    let damage =
                        evaluate_card_for_play(feed, combat, Some(monster.id)).base_damage_mut;
                    monster
                        .current_hp
                        .saturating_add(monster.block)
                        .saturating_sub(damage)
                        .max(0)
                })
                .min()
        });
        let lethal_ready = i32::from(minimum_feed_gap == Some(0));
        let proximity = minimum_feed_gap.map_or(i32::MIN, |gap| gap.saturating_neg());
        CombatStateGuide::new(
            COMBAT_FEED_PAYOFF_GUIDE_LANE_V1,
            vec![
                gained_max_hp,
                i32::from(live_feed_count > 0),
                lethal_ready,
                proximity,
                live_feed_count,
            ],
        )
    }
}

impl CombatActionPolicy for FeedPayoffGuidePolicyV1 {
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
        guides.push(self.guide(position));
        guides
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        let mut guides = self.base.turn_generation_guides(position);
        guides.push(self.guide(position));
        guides
    }
}

/// Encounter-owned service concentration for a sparse semantic corridor.
///
/// Darklings can turn raw long-horizon exploration into an arbitrarily deep
/// reincarnation loop. The existing survival guide preserves the bounded
/// healthy-state corridor, but one equal service per guide reaches that
/// corridor too late under the ordinary allowance. Give that existing typed
/// view two additional one-shot services without changing its rank, action
/// weights, legal surface, or terminal authority.
pub fn darkling_survival_guide_service_bias_v1(
    position: &CombatPosition,
) -> Option<CombatGuideServiceBias> {
    let monsters = &position.combat.entities.monsters;
    (monsters.len() >= 2
        && monsters
            .iter()
            .all(|monster| monster.enemy_id == EnemyId::Darkling))
    .then_some(CombatGuideServiceBias {
        lane: COMBAT_SURVIVAL_GUIDE_LANE_V1,
        extra_services_per_cycle: 2,
    })
}

pub fn authorized_potion_trial_policy_v1(
    base: SharedCombatActionPolicy,
    root: CombatPosition,
    allowed_potion_slots: u64,
) -> SharedCombatActionPolicy {
    Arc::new(AuthorizedPotionTrialPolicyV1 {
        base,
        root,
        allowed_potion_slots,
    })
}

impl CombatActionPolicy for AuthorizedPotionTrialPolicyV1 {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        let mut weights = self.base.weights(position, choices);
        if position != &self.root || weights.len() != choices.len() {
            return weights;
        }
        let Some(root_trial_weight) = weights
            .iter()
            .copied()
            .filter(|weight| weight.is_finite() && *weight > 0.0)
            .max_by(f64::total_cmp)
        else {
            return weights;
        };
        for (weight, choice) in weights.iter_mut().zip(choices) {
            let CombatPolicyChoice::Atomic(ClientInput::UsePotion { potion_index, .. }) = choice
            else {
                continue;
            };
            let Ok(slot) = u32::try_from(*potion_index) else {
                continue;
            };
            if 1_u64
                .checked_shl(slot)
                .is_some_and(|slot_mask| self.allowed_potion_slots & slot_mask != 0)
            {
                *weight = weight.max(root_trial_weight);
            }
        }
        weights
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
        self.base.state_guides(position)
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.base.turn_generation_guides(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::state::core::EngineState;

    struct FixedPolicy;

    impl CombatActionPolicy for FixedPolicy {
        fn weights(
            &self,
            _position: &CombatPosition,
            choices: &[CombatPolicyChoice<'_>],
        ) -> Vec<f64> {
            vec![1.0e-6, 1.0, 0.5][..choices.len()].to_vec()
        }
    }

    #[test]
    fn authorized_potion_trial_only_floors_the_exact_root_and_slot() {
        let root = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        let allowed = ClientInput::UsePotion {
            potion_index: 2,
            target: None,
        };
        let disallowed = ClientInput::UsePotion {
            potion_index: 1,
            target: None,
        };
        let end_turn = ClientInput::EndTurn;
        let choices = [
            CombatPolicyChoice::Atomic(&allowed),
            CombatPolicyChoice::Atomic(&end_turn),
            CombatPolicyChoice::Atomic(&disallowed),
        ];
        let policy = authorized_potion_trial_policy_v1(Arc::new(FixedPolicy), root.clone(), 1 << 2);

        assert_eq!(policy.weights(&root, &choices), vec![1.0, 1.0, 0.5]);

        let mut later = root;
        later.combat.entities.player.block = 1;
        assert_eq!(policy.weights(&later, &choices), vec![1.0e-6, 1.0, 0.5]);
    }

    #[test]
    fn darkling_group_concentrates_existing_survival_service_only() {
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.entities.monsters = (0..3)
            .map(|index| {
                let mut monster = sts_core::test_support::test_monster(EnemyId::Darkling);
                monster.id = index + 1;
                monster
            })
            .collect();
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        assert_eq!(
            darkling_survival_guide_service_bias_v1(&position),
            Some(CombatGuideServiceBias {
                lane: COMBAT_SURVIVAL_GUIDE_LANE_V1,
                extra_services_per_cycle: 2,
            })
        );

        let ordinary = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        assert_eq!(darkling_survival_guide_service_bias_v1(&ordinary), None);
    }
}
