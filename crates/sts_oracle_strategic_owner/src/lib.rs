//! Privileged typed strategic owners shared by oracle runtime and learning.
//!
//! This package owns policy selection only. Branch scheduling, persistence,
//! analysis workspaces, resident services, and model projection remain with
//! their respective downstream crates.

extern crate self as sts_simulator;

use serde::{Deserialize, Serialize};

pub use sts_oracle_run_control::{
    agent, ai, content, engine, eval, fixtures, runtime, sim, state, test_support, EntityId,
};

mod boss_relic_owner;
mod campfire_owner;
mod card_reward_owner;
mod combat_plan_owner;
mod event_owner_bridge;
mod owner_commands;
mod owner_model;
mod owners;
mod reward_tiny_owner;
mod run_choice_owner;
mod shop_tiny_owner;

pub use combat_plan_owner::{
    privileged_combat_research_support_v1, PRIVILEGED_COMBAT_PLAN_OWNER_REVISION_V1,
    PRIVILEGED_COMBAT_PLAN_OWNER_V1,
};
pub use owner_commands::executable_choices;
pub use owner_model::{
    cleanup_target_label, ChoiceAnnotation, DecisionKey, OwnerCandidateDecision, OwnerChoice,
    OwnerChoiceExpansion, OwnerDecision, OwnerRoutine,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Owner {
    CardReward,
    BossRelic,
    Event(state::events::EventId),
    RewardTiny,
    ShopTiny,
    Campfire,
    RunChoice,
}

pub fn owner_for_current_boundary(session: &eval::run_control::RunControlSession) -> Option<Owner> {
    use state::core::EngineState;

    match &session.engine_state {
        EngineState::EventRoom => {
            let event = session.run_state.event_state.as_ref()?;
            if event.id == state::events::EventId::Neow {
                None
            } else {
                Some(Owner::Event(event.id))
            }
        }
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            Some(Owner::CardReward)
        }
        EngineState::RewardScreen(reward) if reward.has_card_reward_item() => {
            Some(Owner::RewardTiny)
        }
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_some() =>
        {
            Some(Owner::CardReward)
        }
        EngineState::RewardOverlay { reward_state, .. } if reward_state.has_card_reward_item() => {
            Some(Owner::RewardTiny)
        }
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => Some(Owner::RewardTiny),
        EngineState::BossRelicSelect(_) => Some(Owner::BossRelic),
        EngineState::Shop(_) => Some(Owner::ShopTiny),
        EngineState::Campfire => Some(Owner::Campfire),
        EngineState::RunPendingChoice(choice) if run_choice_owner::can_handle(choice.reason) => {
            Some(Owner::RunChoice)
        }
        EngineState::RunPendingChoice(_) => None,
        _ => None,
    }
}

pub fn owner_decision(
    session: &eval::run_control::RunControlSession,
    owner: Owner,
    surface: &eval::run_control::DecisionSurface,
) -> OwnerDecision {
    owners::owner_decision(session, owner, surface)
}

/// Adapts privileged exact owners into a complete positive-support search
/// prior. This may inspect exact successors and simulator-private state; it is
/// an oracle exploration aid, not a deployable public baseline or model label.
pub fn privileged_oracle_search_prior_v1(
    session: &eval::run_control::RunControlSession,
    legal: &[eval::run_control::RunPolicyCandidateV1<'_>],
) -> Result<eval::run_control::RunPolicyPriorV1, String> {
    optional_privileged_oracle_search_prior_v1(session, legal)?
        .ok_or_else(|| "strategic owner has no supported choice at this boundary".to_string())
}

// Absence of a policy choice is different from an invalid projection or a
// failed exact transition. Task support needs this distinction; search still
// requires a complete prior and uses the strict wrapper above.
fn optional_privileged_oracle_search_prior_v1(
    session: &eval::run_control::RunControlSession,
    legal: &[eval::run_control::RunPolicyCandidateV1<'_>],
) -> Result<Option<eval::run_control::RunPolicyPriorV1>, String> {
    use content::potions::PotionId;
    use eval::run_control::RunDecisionAction;
    use state::core::{ClientInput, EngineState};

    // Fruit Juice realizes a permanent resource immediately. Entropic Brew is
    // also legally usable outside combat, but consuming it exchanges a
    // deferrable known potion for unknown replacements. It must remain a
    // normal strategic candidate instead of pre-empting rewards, shops,
    // routes, or other boundary owners merely because `canUse` is true.
    let immediate_run_potion_ids = legal
        .iter()
        .filter(|candidate| match candidate.action {
            RunDecisionAction::Input(ClientInput::UsePotion {
                potion_index,
                target: None,
            }) => session
                .run_state
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .is_some_and(|potion| potion.id == PotionId::FruitJuice),
            _ => false,
        })
        .map(|candidate| candidate.candidate_id.to_string())
        .collect::<Vec<_>>();
    if !immediate_run_potion_ids.is_empty() {
        return eval::run_control::positive_ranked_run_policy_prior_v1(
            legal,
            immediate_run_potion_ids,
        )
        .map(Some);
    }
    if matches!(session.engine_state, EngineState::Shop(_)) {
        return eval::run_control::exact_shop_policy_prior_v1(session, legal).map(Some);
    }
    if matches!(session.engine_state, EngineState::Campfire) {
        return eval::run_control::exact_campfire_policy_prior_v1(session, legal).map(Some);
    }
    if matches!(session.engine_state, EngineState::BossRelicSelect(_)) {
        return eval::run_control::exact_boss_relic_policy_prior_v1(session, legal).map(Some);
    }
    if matches!(session.engine_state, EngineState::TreasureRoom(_)) {
        let open_chest_ids = legal
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.action,
                    RunDecisionAction::Input(ClientInput::OpenChest)
                )
            })
            .map(|candidate| candidate.candidate_id.to_string())
            .collect::<Vec<_>>();
        if open_chest_ids.is_empty() {
            return Err("treasure owner requires a legal open-chest candidate".to_string());
        }
        return eval::run_control::positive_ranked_run_policy_prior_v1(legal, open_chest_ids)
            .map(Some);
    }
    if session.engine_state.is_map_surface() {
        return eval::run_control::exact_route_policy_prior_v1(session, legal).map(Some);
    }
    if eval::run_control::reward_policy_has_claimable_step(session)? {
        return eval::run_control::positive_ranked_run_policy_prior_v1(
            legal,
            owner_preferred_candidate_ids_v1(session)?,
        )
        .map(Some);
    }
    if matches!(&session.engine_state,
        EngineState::RewardScreen(reward) | EngineState::RewardOverlay { reward_state: reward, .. }
            if reward.pending_card_choice.is_none())
    {
        let progress = eval::run_control::reward_policy_progress_v1(session)?;
        let preferred = if progress.preferred_card_candidate_ids.is_empty() {
            owner_preferred_candidate_ids_v1(session)?
        } else {
            progress.preferred_card_candidate_ids
        };
        if preferred.is_empty() {
            return Ok(None);
        }
        return eval::run_control::positive_ranked_run_policy_prior_v1(legal, preferred).map(Some);
    }
    let card_reward_ids = eval::run_control::build_decision_surface(session)
        .view
        .candidates
        .into_iter()
        .filter(|candidate| {
            candidate.action.executable_action_ref().is_some()
                && matches!(
                    candidate.key,
                    Some(
                        eval::run_control::DecisionCandidateKey::CardRewardPick { .. }
                            | eval::run_control::DecisionCandidateKey::CardRewardOpen { .. }
                            | eval::run_control::DecisionCandidateKey::CardRewardSingingBowl { .. }
                            | eval::run_control::DecisionCandidateKey::CardRewardSkip { .. }
                    )
                )
        })
        .map(|candidate| candidate.id)
        .collect::<std::collections::BTreeSet<_>>();
    if !card_reward_ids.is_empty()
        && card_reward_ids.iter().all(|candidate_id| {
            legal
                .iter()
                .any(|candidate| candidate.candidate_id == candidate_id.as_str())
        })
    {
        return eval::run_control::exact_card_reward_policy_prior_v1(session, legal).map(Some);
    }
    let preferred = owner_preferred_candidate_ids_v1(session)?;
    if preferred.is_empty() {
        return Ok(None);
    }
    eval::run_control::positive_ranked_run_policy_prior_v1(legal, preferred).map(Some)
}

fn owner_preferred_candidate_ids_v1(
    session: &eval::run_control::RunControlSession,
) -> Result<Vec<String>, String> {
    if matches!(session.engine_state, state::core::EngineState::EventRoom)
        && session.run_state.event_state.is_none()
    {
        return Err("event boundary is missing its event state".to_string());
    }
    let mut trial = session.clone();
    let Some(owner) = owner_for_current_boundary(&trial) else {
        return Ok(Vec::new());
    };
    let surface = eval::run_control::build_decision_surface(&trial);
    Ok(match owner_decision(&trial, owner, &surface) {
        OwnerDecision::Candidates(choices) => choices
            .into_iter()
            .map(|choice| choice.candidate_id)
            .collect(),
        OwnerDecision::Routine(OwnerRoutine::Candidate { candidate_id, .. }) => {
            vec![candidate_id]
        }
        OwnerDecision::Routine(OwnerRoutine::RewardPolicyStep) => {
            match try_apply_reward_policy_routine(&mut trial)? {
                None => Vec::new(),
                Some(outcome) => vec![outcome
                    .single_decision_transaction()
                    .ok_or_else(|| {
                        "reward policy step did not produce one decision transaction".to_string()
                    })?
                    .selection
                    .candidate_id
                    .clone()],
            }
        }
        OwnerDecision::Routine(OwnerRoutine::ForcedTransition(_)) => Vec::new(),
        OwnerDecision::Gap(error) => return Err(error),
    })
}

fn apply_reward_policy_routine(
    session: &mut eval::run_control::RunControlSession,
) -> Result<eval::run_control::RunProgressOutcome, String> {
    try_apply_reward_policy_routine(session)?
        .ok_or_else(|| "reward owner has unresolved policy work".to_string())
}

fn try_apply_reward_policy_routine(
    session: &mut eval::run_control::RunControlSession,
) -> Result<Option<eval::run_control::RunProgressOutcome>, String> {
    if let Some(outcome) = eval::run_control::apply_reward_policy_step(session)? {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = eval::run_control::apply_reward_potion_space_step(session)? {
        return Ok(Some(outcome));
    }
    if !eval::run_control::reward_policy_progress_v1(session)?.is_complete() {
        return Ok(None);
    }
    let action = eval::run_control::RunDecisionAction::Input(reward_tiny_exit_input(session)?);
    let surface = eval::run_control::build_decision_surface(session);
    let candidate_id = owner_commands::owner_candidate_id_for_action(&surface, &action)?;
    session
        .apply_owner_candidate(&candidate_id, action)
        .map(Some)
}

fn reward_tiny_exit_input(
    session: &eval::run_control::RunControlSession,
) -> Result<state::core::ClientInput, String> {
    use state::core::{ClientInput, EngineState};

    let exit = match &session.engine_state {
        EngineState::RewardScreen(_) => ClientInput::Proceed,
        EngineState::RewardOverlay { .. } => ClientInput::Cancel,
        _ => return Err("RewardTiny owner requires reward surface".to_string()),
    };
    if eval::run_control::reward_policy_progress_v1(session)?.is_complete() {
        let surface = eval::run_control::build_decision_surface(session);
        if surface
            .visible_executable_inputs
            .iter()
            .any(|visible_input| visible_input == &exit)
        {
            return Ok(exit);
        }
        return Err(format!(
            "input {exit:?} is not visible at {}",
            surface.view.header.title
        ));
    }
    Err("RewardTiny owner has unresolved reward policy work".to_string())
}

pub fn apply_owner_routine(
    session: &mut eval::run_control::RunControlSession,
    routine: OwnerRoutine,
) -> Result<eval::run_control::RunProgressOutcome, String> {
    match routine {
        OwnerRoutine::Candidate {
            candidate_id,
            action,
        } => session.apply_owner_candidate(&candidate_id, action),
        OwnerRoutine::RewardPolicyStep => apply_reward_policy_routine(session),
        OwnerRoutine::ForcedTransition(kind) => session.apply_forced_transition(kind),
    }
}

pub fn current_oracle_candidate_order_v1(
    session: &eval::run_control::RunControlSession,
) -> Vec<String> {
    let surface = eval::run_control::build_decision_surface(session);
    let legal = surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            candidate.action.executable_action_ref().map(|action| {
                eval::run_control::RunPolicyCandidateV1 {
                    candidate_id: &candidate.id,
                    label: &candidate.label,
                    action,
                }
            })
        })
        .collect::<Vec<_>>();
    if legal.is_empty() {
        return Vec::new();
    }
    privileged_oracle_search_prior_v1(session, &legal)
        .map(|prior| {
            prior
                .entries
                .into_iter()
                .map(|entry| entry.candidate_id)
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the first exact strategic-owner action at the current boundary.
///
/// This is privileged policy output. Downstream consumers must project it
/// onto their own unchanged public candidate surface and retain provenance.
pub fn current_privileged_strategic_owner_action_v1(
    session: &eval::run_control::RunControlSession,
) -> Option<(String, eval::run_control::RunDecisionAction)> {
    current_privileged_strategic_owner_action_result_v1(session)
        .ok()
        .flatten()
}

/// Fallible form used by provenance-carrying support adapters. Structural
/// owner/prior failures must remain errors instead of being reported as an
/// ordinary absence of owner capability.
pub fn current_privileged_strategic_owner_action_result_v1(
    session: &eval::run_control::RunControlSession,
) -> Result<Option<(String, eval::run_control::RunDecisionAction)>, String> {
    use state::core::EngineState;

    let surface = eval::run_control::build_decision_surface(session);
    if matches!(session.engine_state, EngineState::EventRoom)
        && session
            .run_state
            .event_state
            .as_ref()
            .is_some_and(|event| event.id == state::events::EventId::Neow)
    {
        return Ok(None);
    }
    if matches!(session.engine_state, EngineState::RunPendingChoice(_)) {
        let Some(owner) = owner_for_current_boundary(session) else {
            return Ok(None);
        };
        return Ok(match owner_decision(session, owner, &surface) {
            OwnerDecision::Candidates(choices) => choices
                .into_iter()
                .next()
                .map(|choice| (choice.candidate_id, choice.action)),
            OwnerDecision::Routine(OwnerRoutine::Candidate {
                candidate_id,
                action,
            }) => Some((candidate_id, action)),
            OwnerDecision::Routine(OwnerRoutine::RewardPolicyStep)
            | OwnerDecision::Routine(OwnerRoutine::ForcedTransition(_)) => None,
            OwnerDecision::Gap(error) => return Err(error),
        });
    }

    let legal = surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            candidate.action.executable_action_ref().map(|action| {
                eval::run_control::RunPolicyCandidateV1 {
                    candidate_id: &candidate.id,
                    label: &candidate.label,
                    action,
                }
            })
        })
        .collect::<Vec<_>>();
    let Some(prior) = optional_privileged_oracle_search_prior_v1(session, &legal)? else {
        return Ok(None);
    };
    let Some(first) = prior.entries.first() else {
        return Ok(None);
    };
    let candidate = legal
        .into_iter()
        .find(|candidate| candidate.candidate_id == first.candidate_id)
        .ok_or_else(|| "strategic owner prior selected a non-legal candidate".to_string())?;
    Ok(Some((first.candidate_id.clone(), candidate.action.clone())))
}

/// Joins the current exact owner action to the unchanged planner-facing
/// strategic candidate identity. Downstream model/task adapters own only their
/// respective ordinal languages.
pub enum PrivilegedStrategicOwnerPlannerActionV1 {
    Choice {
        planner_candidate_id: String,
        action: eval::run_control::RunDecisionAction,
    },
    ExactOwnerUnavailable,
    OwnerCandidateNotRepresented,
}

pub fn current_privileged_strategic_owner_planner_action_v1(
    session: &eval::run_control::RunControlSession,
) -> Result<PrivilegedStrategicOwnerPlannerActionV1, String> {
    let Some((run_candidate_id, action)) =
        current_privileged_strategic_owner_action_result_v1(session)?
    else {
        return Ok(PrivilegedStrategicOwnerPlannerActionV1::ExactOwnerUnavailable);
    };
    let segment = eval::run_control::capture_planner_boundary_yield_v1(
        session,
        eval::run_control::PlannerBoundaryYieldKindV1::CallbackStop,
    )?;
    let [visit] = segment.visits.as_slice() else {
        return Ok(PrivilegedStrategicOwnerPlannerActionV1::OwnerCandidateNotRepresented);
    };
    let Some(link) = visit
        .candidate_links
        .iter()
        .find(|link| link.run_candidate_id == run_candidate_id)
    else {
        return Ok(PrivilegedStrategicOwnerPlannerActionV1::OwnerCandidateNotRepresented);
    };
    Ok(PrivilegedStrategicOwnerPlannerActionV1::Choice {
        planner_candidate_id: link.planner_candidate_id.clone(),
        action,
    })
}

/// Compatibility entry point for the retained fixed-node learning prototype.
pub fn current_privileged_node_sl_owner_action_v1(
    session: &eval::run_control::RunControlSession,
) -> Option<(String, eval::run_control::RunDecisionAction)> {
    current_privileged_strategic_owner_action_v1(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::cards::CardId;
    use content::potions::{Potion, PotionId};
    use content::relics::RelicTier;
    use eval::run_control::{RunControlConfig, RunControlSession, RunDecisionAction};
    use runtime::combat::CombatCard;
    use state::core::{ClientInput, EngineState};
    use state::rewards::{RewardItem, RewardState, TreasureChestSize, TreasureChestState};

    #[test]
    fn full_potion_belt_cannot_outvote_opening_a_treasure_chest() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::EntropicBrew, 1)),
            Some(Potion::new(PotionId::LiquidMemories, 2)),
            Some(Potion::new(PotionId::Elixir, 3)),
        ];
        session.engine_state = EngineState::TreasureRoom(TreasureChestState {
            size: TreasureChestSize::Medium,
            base_relic_tier: RelicTier::Uncommon,
            gold_reward_base_amount: None,
        });

        let (_, action) = current_privileged_strategic_owner_action_result_v1(&session)
            .expect("treasure owner decision")
            .expect("treasure owner action");

        assert!(matches!(
            action,
            RunDecisionAction::Input(ClientInput::OpenChest)
        ));
    }

    #[test]
    fn fruit_juice_realizes_before_opening_a_treasure_chest() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::FruitJuice, 1)),
            Some(Potion::new(PotionId::LiquidMemories, 2)),
            Some(Potion::new(PotionId::Elixir, 3)),
        ];
        session.engine_state = EngineState::TreasureRoom(TreasureChestState {
            size: TreasureChestSize::Medium,
            base_relic_tier: RelicTier::Uncommon,
            gold_reward_base_amount: None,
        });

        let (_, action) = current_privileged_strategic_owner_action_result_v1(&session)
            .expect("treasure owner decision")
            .expect("treasure owner action");

        assert_eq!(
            action,
            RunDecisionAction::Input(ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            })
        );
    }

    #[test]
    fn grounded_energy_reward_replaces_unique_flexible_colorless_slot() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.master_deck = [
            CardId::DarkEmbrace,
            CardId::Impervious,
            CardId::FiendFire,
            CardId::Clothesline,
            CardId::Whirlwind,
        ]
        .into_iter()
        .enumerate()
        .map(|(uuid, card)| CombatCard::for_scenario(card, uuid as u32))
        .collect();
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::ColorlessPotion, 1)),
            Some(Potion::new(PotionId::LiquidMemories, 2)),
            Some(Potion::new(PotionId::Elixir, 3)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::EnergyPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let (_, action) = current_privileged_strategic_owner_action_result_v1(&session)
            .expect("potion reward owner decision")
            .expect("potion reward owner action");

        assert_eq!(
            action,
            RunDecisionAction::Input(ClientInput::DiscardPotion(0))
        );
    }
}
