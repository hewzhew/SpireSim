use crate::ai::reward_policy_v1::{
    build_reward_decision_context_v1, plan_reward_decision_v1, RewardPolicyActionV1,
    RewardPolicyClassV1, RewardPolicyConfigV1,
};
use crate::ai::strategy::deck_role_inventory::DeckRoleInventory;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::state::core::{ClientInput, EngineState};
use crate::state::rewards::{RewardCard, RewardItem, RewardState};
use crate::state::run::RunState;

use super::session::{RunControlSession, RunProgressOutcome};
use super::trace_annotation::RunControlTraceAnnotationV1;
use super::DecisionCandidateKey;

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RewardAutomationConfig {
    pub claim_gold: bool,
    pub claim_potion_with_empty_slot: bool,
    pub claim_safe_relic_without_sapphire_key: bool,
}

impl Default for RewardAutomationConfig {
    fn default() -> Self {
        Self {
            claim_gold: true,
            claim_potion_with_empty_slot: true,
            claim_safe_relic_without_sapphire_key: true,
        }
    }
}

pub(in crate::eval::run_control) fn ensure_singing_bowl_card_reward_action(
    session: &RunControlSession,
    reward_index: usize,
) -> Result<(), String> {
    if !session
        .run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::SingingBowl)
    {
        return Err("Singing Bowl card reward requires Singing Bowl relic".to_string());
    }

    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward,
        EngineState::RewardOverlay { reward_state, .. } => reward_state,
        _ => return Err("Singing Bowl card reward requires a reward screen".to_string()),
    };
    if reward.pending_card_choice.is_some() {
        return Err(
            "Singing Bowl visible card reward requires an unopened card reward item".to_string(),
        );
    }
    if !matches!(
        reward.items.get(reward_index),
        Some(RewardItem::Card { .. })
    ) {
        return Err(format!(
            "reward item {reward_index} is not a visible card reward item"
        ));
    }
    Ok(())
}

pub(in crate::eval::run_control) fn active_pending_reward_cards(
    session: &RunControlSession,
) -> Option<Vec<RewardCard>> {
    let cards = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward.pending_card_choice.as_ref()?,
        EngineState::RewardOverlay { reward_state, .. } => {
            reward_state.pending_card_choice.as_ref()?
        }
        _ => return None,
    };
    Some(cards.clone())
}

struct RewardPolicyPlan {
    reward_index: usize,
    trace_annotation: RunControlTraceAnnotationV1,
}

struct RewardPotionSpacePlan {
    key: DecisionCandidateKey,
    input: ClientInput,
}

impl RewardAutomationConfig {
    pub fn summary(&self) -> String {
        format!(
            "auto-reward: gold={} potion_if_empty_slot={} safe_relic_without_sapphire_key={}",
            on_off(self.claim_gold),
            on_off(self.claim_potion_with_empty_slot),
            on_off(self.claim_safe_relic_without_sapphire_key)
        )
    }
}

pub fn apply_reward_policy_step(
    session: &mut RunControlSession,
) -> Result<Option<RunProgressOutcome>, String> {
    let Some(plan) = next_reward_policy_claim(session)? else {
        return Ok(None);
    };
    let action = super::RunDecisionAction::Input(ClientInput::ClaimReward(plan.reward_index));
    let surface = super::build_decision_surface(session);
    let matches = surface
        .view
        .candidates
        .iter()
        .filter(|candidate| candidate.action.executable_action().as_ref() == Some(&action))
        .collect::<Vec<_>>();
    let [candidate] = matches.as_slice() else {
        return Err(format!(
            "reward policy action {action:?} matched {} public candidates",
            matches.len()
        ));
    };
    let candidate_id = candidate.id.clone();
    let transaction =
        session.execute_reward_candidate_transaction(&candidate_id, plan.trace_annotation)?;
    Ok(Some(transaction.project_progress_outcome(session)))
}

/// Reports whether the current reward surface has one low-agency claim that
/// the reward policy can apply before a nested strategy boundary is opened.
pub fn reward_policy_has_claimable_step(session: &RunControlSession) -> Result<bool, String> {
    Ok(next_reward_policy_claim(session)?.is_some())
}

pub fn apply_reward_potion_space_step(
    session: &mut RunControlSession,
) -> Result<Option<RunProgressOutcome>, String> {
    let Some(plan) = reward_potion_space_plan(session) else {
        return Ok(None);
    };
    let action = super::RunDecisionAction::Input(plan.input);
    let surface = super::build_decision_surface(session);
    let matches = surface
        .view
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.key.as_ref() == Some(&plan.key)
                && candidate.action.executable_action().as_ref() == Some(&action)
        })
        .collect::<Vec<_>>();
    let [candidate] = matches.as_slice() else {
        return Err(format!(
            "reward potion-space action {action:?} matched {} public candidates",
            matches.len()
        ));
    };
    let candidate_id = candidate.id.clone();
    session
        .apply_owner_candidate(&candidate_id, action)
        .map(Some)
}

pub(super) fn reward_policy_has_potion_space_step(session: &RunControlSession) -> bool {
    reward_potion_space_plan(session).is_some()
}

fn reward_potion_space_plan(session: &RunControlSession) -> Option<RewardPotionSpacePlan> {
    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward,
        EngineState::RewardOverlay { reward_state, .. } => reward_state,
        _ => return None,
    };
    if session.run_state.find_empty_potion_slot().is_some()
        || session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::Sozu)
        || !reward
            .items
            .iter()
            .any(|item| matches!(item, RewardItem::Potion { .. }))
    {
        return None;
    }
    let is_we_meet_again = session
        .run_state
        .event_state
        .as_ref()
        .is_some_and(|event| event.id == crate::state::events::EventId::WeMeetAgain);
    if let Some((potion_index, potion)) =
        session
            .run_state
            .potions
            .iter()
            .enumerate()
            .find_map(|(potion_index, potion)| {
                let potion = potion.as_ref()?;
                (potion.id == crate::content::potions::PotionId::FruitJuice
                    && potion.can_use
                    && crate::content::potions::potion_can_use_out_of_combat(
                        potion.id,
                        is_we_meet_again,
                    ))
                .then_some((potion_index, potion))
            })
    {
        return Some(RewardPotionSpacePlan {
            key: DecisionCandidateKey::RunPotionUse {
                slot: potion_index,
                potion: potion.id,
                uuid: potion.uuid,
            },
            input: ClientInput::UsePotion {
                potion_index,
                target: None,
            },
        });
    }

    if !crate::content::potions::potion_can_discard_in_event(is_we_meet_again) {
        return None;
    }
    let incoming_potions = reward
        .items
        .iter()
        .filter_map(|item| match item {
            RewardItem::Potion { potion_id } => Some(*potion_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    if incoming_potions.contains(&PotionId::FruitJuice) {
        if let Some((slot, potion)) = newest_discardable_duplicate_potion(session, None) {
            return Some(discard_potion_space_plan(slot, potion));
        }
    }

    if incoming_potions.contains(&PotionId::GamblersBrew) {
        let roles = DeckRoleInventory::from_deck(&session.run_state.master_deck);
        // Selective hand redraw is the incoming resource. Vulnerable cards do
        // not make redraw stronger; they only lower the opportunity cost of
        // removing the newest duplicate Fear while one Fear remains.
        if roles.vulnerable_units >= 2 {
            if let Some((slot, potion)) =
                newest_discardable_duplicate_potion(session, Some(PotionId::FearPotion))
            {
                return Some(discard_potion_space_plan(slot, potion));
            }
        }
    }

    if incoming_potions.contains(&PotionId::StrengthPotion) {
        let roles = DeckRoleInventory::from_deck(&session.run_state.master_deck);
        if roles.strength_payoff_units > 0 && roles.vulnerable_units > 0 {
            if let Some((slot, potion)) = session
                .run_state
                .potions
                .iter()
                .enumerate()
                .filter_map(|(slot, potion)| {
                    let potion = potion.as_ref()?;
                    (potion.id == PotionId::FearPotion && potion.can_discard)
                        .then_some((slot, potion))
                })
                .max_by_key(|(_, potion)| potion.uuid)
            {
                return Some(discard_potion_space_plan(slot, potion));
            }
        }
    }
    if let Some(slot) = super::potion_reward_policy_audit_v1(session)
        .and_then(|audit| audit.recommended_discard_slot())
    {
        if let Some(potion) = session
            .run_state
            .potions
            .get(slot)
            .and_then(Option::as_ref)
            .filter(|potion| potion.can_discard)
        {
            return Some(discard_potion_space_plan(slot, potion));
        }
    }
    None
}

fn newest_discardable_duplicate_potion(
    session: &RunControlSession,
    required_id: Option<PotionId>,
) -> Option<(usize, &crate::content::potions::Potion)> {
    session
        .run_state
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            if required_id.is_some_and(|required_id| potion.id != required_id) {
                return None;
            }
            let copies = session
                .run_state
                .potions
                .iter()
                .filter(|candidate| {
                    candidate
                        .as_ref()
                        .is_some_and(|candidate| candidate.id == potion.id)
                })
                .count();
            (copies > 1 && potion.can_discard).then_some((slot, potion))
        })
        .max_by_key(|(_, potion)| potion.uuid)
}

fn discard_potion_space_plan(
    slot: usize,
    potion: &crate::content::potions::Potion,
) -> RewardPotionSpacePlan {
    RewardPotionSpacePlan {
        key: DecisionCandidateKey::RunPotionDiscard {
            slot,
            potion: potion.id,
            uuid: potion.uuid,
        },
        input: ClientInput::DiscardPotion(slot),
    }
}

pub fn reward_surface_has_only_unclaimable_potions(session: &RunControlSession) -> bool {
    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward,
        EngineState::RewardOverlay { reward_state, .. } => reward_state,
        _ => return false,
    };
    reward_state_has_only_unclaimable_potions(&session.run_state, reward)
}

// Neow enumeration uses this structural boundary, not the strategy's decision
// to decline a reward: its branches deliberately explore alternative choices.
pub(super) fn map_overlay_returns_to_only_unclaimable_potions(session: &RunControlSession) -> bool {
    let EngineState::MapOverlay { return_state } = &session.engine_state else {
        return false;
    };
    let mut returned = session.clone();
    returned.engine_state = return_state.as_ref().clone();
    reward_surface_has_only_unclaimable_potions(&returned)
}

pub(super) fn reward_state_has_only_unclaimable_potions(
    run_state: &RunState,
    reward: &RewardState,
) -> bool {
    let context = build_reward_decision_context_v1(run_state, reward);
    !context.candidates.is_empty()
        && context.candidates.iter().all(|candidate| {
            matches!(
                candidate.class,
                RewardPolicyClassV1::PotionNoEmptySlot | RewardPolicyClassV1::PotionBlockedBySozu
            )
        })
}

fn next_reward_policy_claim(
    session: &RunControlSession,
) -> Result<Option<RewardPolicyPlan>, String> {
    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward,
        EngineState::RewardOverlay { reward_state, .. } => reward_state,
        _ => return Ok(None),
    };
    let context = build_reward_decision_context_v1(&session.run_state, reward);
    let decision = plan_reward_decision_v1(&context, &reward_policy_config(session));
    let RewardPolicyActionV1::Claim { index, .. } = &decision.action else {
        return Ok(None);
    };
    let record = decision.to_noncombat_decision_record_v1();
    Ok(Some(RewardPolicyPlan {
        reward_index: *index,
        trace_annotation: super::noncombat_policy_annotation::noncombat_policy_annotation(
            "reward policy",
            record,
        )?,
    }))
}

fn reward_policy_config(session: &RunControlSession) -> RewardPolicyConfigV1 {
    RewardPolicyConfigV1 {
        claim_gold: session.reward_automation.claim_gold,
        claim_potion_with_empty_slot: session.reward_automation.claim_potion_with_empty_slot,
        claim_safe_relic_without_sapphire_key: session
            .reward_automation
            .claim_safe_relic_without_sapphire_key,
    }
}

fn on_off(enabled: bool) -> &'static str {
    if enabled {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::potions::PotionId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::eval::run_control::RunDecisionSelectionSourceV1;
    use crate::state::rewards::{RewardItem, RewardState};

    #[test]
    fn reward_policy_claims_exactly_one_public_candidate_per_step() {
        let mut session = reward_screen_session(vec![
            RewardItem::Gold { amount: 19 },
            RewardItem::Potion {
                potion_id: PotionId::EssenceOfSteel,
            },
            RewardItem::Card { cards: Vec::new() },
        ]);

        let gold = apply_reward_policy_step(&mut session)
            .expect("gold policy step should run")
            .expect("gold should be selected");

        assert_eq!(session.run_state.gold, 118);
        assert!(session.run_state.potions[0].is_none());
        assert_reward_policy_transaction(&gold, 0, 1);

        let potion = apply_reward_policy_step(&mut session)
            .expect("potion policy step should run")
            .expect("potion should be selected on the next boundary");

        assert_eq!(
            session.run_state.potions[0]
                .as_ref()
                .map(|potion| potion.id),
            Some(PotionId::EssenceOfSteel)
        );
        assert_reward_policy_transaction(&potion, 1, 2);
        assert!(apply_reward_policy_step(&mut session)
            .expect("card boundary should be inspected")
            .is_none());
        let EngineState::RewardScreen(reward) = &session.engine_state else {
            panic!("card reward should keep reward screen open");
        };
        assert!(matches!(reward.items.as_slice(), [RewardItem::Card { .. }]));
    }

    #[test]
    fn reward_policy_leaves_potion_when_slots_are_full() {
        let mut session = reward_screen_session(vec![RewardItem::Potion {
            potion_id: PotionId::EssenceOfSteel,
        }]);
        session.run_state.potions = vec![
            Some(crate::content::potions::Potion::new(
                PotionId::FirePotion,
                1,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::DexterityPotion,
                2,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::StrengthPotion,
                3,
            )),
        ];

        let outcome = apply_reward_policy_step(&mut session).expect("policy should inspect reward");

        assert!(outcome.is_none());
        assert_eq!(session.decision_step, 0);
        let EngineState::RewardScreen(reward) = &session.engine_state else {
            panic!("full potion slots should leave reward screen open");
        };
        assert!(matches!(
            reward.items.as_slice(),
            [RewardItem::Potion {
                potion_id: PotionId::EssenceOfSteel
            }]
        ));
    }

    #[test]
    fn reward_potion_space_step_realizes_fruit_juice_before_claiming_potion() {
        let mut session = reward_screen_session(vec![RewardItem::Potion {
            potion_id: PotionId::EssenceOfSteel,
        }]);
        session.run_state.potions = vec![
            Some(crate::content::potions::Potion::new(
                PotionId::FruitJuice,
                1,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::FirePotion,
                2,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::DexterityPotion,
                3,
            )),
        ];
        let before_hp = session.run_state.current_hp;
        let before_max_hp = session.run_state.max_hp;

        let realized = apply_reward_potion_space_step(&mut session)
            .expect("potion-space policy should inspect the reward")
            .expect("Fruit Juice should be realized as one owner decision");

        assert_eq!(session.run_state.current_hp, before_hp + 5);
        assert_eq!(session.run_state.max_hp, before_max_hp + 5);
        assert!(session.run_state.potions[0].is_none());
        assert_reward_owner_transaction(&realized, 0, 1);
        let EngineState::RewardScreen(reward) = &session.engine_state else {
            panic!("using Fruit Juice must preserve the reward screen");
        };
        assert!(matches!(
            reward.items.as_slice(),
            [RewardItem::Potion {
                potion_id: PotionId::EssenceOfSteel
            }]
        ));

        let claimed = apply_reward_policy_step(&mut session)
            .expect("reward policy should inspect the opened slot")
            .expect("the potion reward should be claimed on the next decision");
        assert_eq!(
            session.run_state.potions[0]
                .as_ref()
                .map(|potion| potion.id),
            Some(PotionId::EssenceOfSteel)
        );
        assert_reward_policy_transaction(&claimed, 1, 2);
    }

    #[test]
    fn reward_potion_space_step_discards_newest_duplicate_for_fruit_juice() {
        let mut session = reward_screen_session(vec![RewardItem::Potion {
            potion_id: PotionId::FruitJuice,
        }]);
        session.run_state.potions = vec![
            Some(crate::content::potions::Potion::new(
                PotionId::FearPotion,
                11,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::FearPotion,
                19,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::FirePotion,
                7,
            )),
        ];

        let discarded = apply_reward_potion_space_step(&mut session)
            .expect("potion-space policy should inspect duplicate inventory")
            .expect("Fruit Juice should justify discarding the newest duplicate");

        assert!(session.run_state.potions[1].is_none());
        assert_eq!(
            session.run_state.potions[0]
                .as_ref()
                .map(|potion| (potion.id, potion.uuid)),
            Some((PotionId::FearPotion, 11))
        );
        assert_reward_owner_transaction(&discarded, 0, 1);

        apply_reward_policy_step(&mut session)
            .expect("reward policy should inspect the opened slot")
            .expect("Fruit Juice should be claimed on the next atomic step");
        assert_eq!(
            session.run_state.potions[1]
                .as_ref()
                .map(|potion| potion.id),
            Some(PotionId::FruitJuice)
        );
    }

    #[test]
    fn gamblers_brew_replaces_covered_duplicate_fear_without_exhaust_roles() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;

        let mut session = full_belt_gamblers_reward_session();
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Bash, 101),
            CombatCard::for_scenario(CardId::ThunderClap, 102),
            CombatCard::for_scenario(CardId::Strike, 103),
        ];

        let discarded = apply_reward_potion_space_step(&mut session)
            .expect("potion-space policy should inspect the covered duplicate")
            .expect("selective redraw should replace the newest covered duplicate Fear");

        assert!(session.run_state.potions[1].is_none());
        assert_reward_owner_transaction(&discarded, 0, 1);
        apply_reward_policy_step(&mut session)
            .expect("reward policy should inspect the opened slot")
            .expect("Gambler's Brew should be claimed on the next atomic step");
        assert_eq!(
            session.run_state.potions[1]
                .as_ref()
                .map(|potion| potion.id),
            Some(PotionId::GamblersBrew)
        );
    }

    #[test]
    fn gamblers_brew_keeps_duplicate_fear_when_its_opportunity_cost_is_uncovered() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;

        let mut session = full_belt_gamblers_reward_session();
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Bash, 101),
            CombatCard::for_scenario(CardId::DarkEmbrace, 102),
            CombatCard::for_scenario(CardId::SecondWind, 103),
            CombatCard::for_scenario(CardId::PowerThrough, 104),
        ];

        assert!(apply_reward_potion_space_step(&mut session)
            .expect("potion-space policy should inspect the uncovered duplicate")
            .is_none());
        assert!(session.run_state.potions.iter().all(Option::is_some));
    }

    #[test]
    fn reward_potion_space_step_replaces_covered_fear_with_strength_payoff() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;

        let mut session = full_belt_strength_reward_session();
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Bash, 101),
            CombatCard::for_scenario(CardId::SwordBoomerang, 102),
        ];

        let discarded = apply_reward_potion_space_step(&mut session)
            .expect("potion-space policy should inspect Strength support")
            .expect("covered Fear should be replaced for a concrete Strength payoff");

        assert!(session.run_state.potions[1].is_none());
        assert_reward_owner_transaction(&discarded, 0, 1);
        apply_reward_policy_step(&mut session)
            .expect("reward policy should inspect the opened slot")
            .expect("Strength should be claimed on the next atomic step");
        assert_eq!(
            session.run_state.potions[1]
                .as_ref()
                .map(|potion| potion.id),
            Some(PotionId::StrengthPotion)
        );
    }

    #[test]
    fn reward_potion_space_step_keeps_fear_without_both_semantic_dependencies() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;

        for deck in [
            vec![CombatCard::for_scenario(CardId::SwordBoomerang, 101)],
            vec![CombatCard::for_scenario(CardId::Bash, 102)],
        ] {
            let mut session = full_belt_strength_reward_session();
            session.run_state.master_deck = deck;

            assert!(apply_reward_potion_space_step(&mut session)
                .expect("potion-space policy should inspect incomplete support")
                .is_none());
            assert_eq!(
                session.run_state.potions[1]
                    .as_ref()
                    .map(|potion| potion.id),
                Some(PotionId::FearPotion)
            );
        }
    }

    #[test]
    fn reward_policy_leaves_sozu_blocked_potion_for_explicit_exit() {
        let mut session = reward_screen_session(vec![RewardItem::Potion {
            potion_id: PotionId::EnergyPotion,
        }]);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::Sozu));
        assert!(session.run_state.find_empty_potion_slot().is_some());

        let outcome = apply_reward_policy_step(&mut session)
            .expect("reward policy should inspect blocked potion");

        assert!(outcome.is_none());
        assert!(session.run_state.potions.iter().all(Option::is_none));
        let EngineState::RewardScreen(reward) = &session.engine_state else {
            panic!("blocked potion should remain on the reward screen until exit");
        };
        assert_eq!(reward.items.len(), 1);
        assert_eq!(session.decision_step, 0);
    }

    #[test]
    fn reward_policy_claims_stolen_gold_as_one_transaction() {
        let mut session = reward_screen_session(vec![RewardItem::StolenGold { amount: 40 }]);

        let outcome = apply_reward_policy_step(&mut session)
            .expect("policy should run")
            .expect("stolen gold should be selected");

        assert_eq!(session.run_state.gold, 139);
        assert_reward_policy_transaction(&outcome, 0, 1);
    }

    #[test]
    fn reward_policy_claims_safe_relic_with_policy_annotation() {
        let mut session = reward_screen_session(vec![RewardItem::Relic {
            relic_id: crate::content::relics::RelicId::Anchor,
        }]);

        let outcome = apply_reward_policy_step(&mut session)
            .expect("policy should run")
            .expect("safe relic should be selected");

        assert!(session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == crate::content::relics::RelicId::Anchor));
        assert_reward_policy_transaction(&outcome, 0, 1);
        let super::super::trace_annotation::RunControlTraceAnnotationV1::NonCombatPolicyDecision {
            record,
            ..
        } = &outcome.trace_annotations[0]
        else {
            panic!("safe relic policy claim should attach noncombat policy evidence");
        };
        crate::ai::noncombat_decision_v1::validate_noncombat_decision_record_v1(record)
            .expect("safe relic reward record should validate");
        assert_eq!(
            record.site,
            crate::ai::noncombat_decision_v1::DecisionSiteKindV1::Reward
        );
        assert_eq!(
            record.selection.status,
            crate::ai::noncombat_decision_v1::PolicySelectionStatusV1::Selected
        );
    }

    #[test]
    fn reward_policy_leaves_relic_when_sapphire_key_is_available() {
        let mut session = reward_screen_session(vec![
            RewardItem::Relic {
                relic_id: crate::content::relics::RelicId::Anchor,
            },
            RewardItem::SapphireKey,
        ]);

        let outcome = apply_reward_policy_step(&mut session).expect("policy should inspect reward");

        assert!(outcome.is_none());
        assert!(!session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == crate::content::relics::RelicId::Anchor));
        let EngineState::RewardScreen(reward) = &session.engine_state else {
            panic!("sapphire/relic choice should remain on reward screen");
        };
        assert!(matches!(
            reward.items.as_slice(),
            [
                RewardItem::Relic {
                    relic_id: crate::content::relics::RelicId::Anchor
                },
                RewardItem::SapphireKey
            ]
        ));
    }

    #[test]
    fn reward_policy_leaves_relic_when_safe_relic_claiming_is_disabled() {
        let mut session = reward_screen_session(vec![RewardItem::Relic {
            relic_id: crate::content::relics::RelicId::Anchor,
        }]);
        session
            .reward_automation
            .claim_safe_relic_without_sapphire_key = false;

        let outcome = apply_reward_policy_step(&mut session).expect("policy should inspect reward");

        assert!(outcome.is_none());
        assert!(!session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == crate::content::relics::RelicId::Anchor));
    }

    fn assert_reward_policy_transaction(
        outcome: &RunProgressOutcome,
        before_step: u64,
        after_step: u64,
    ) {
        let Some(transaction) = outcome.single_decision_transaction() else {
            panic!("one reward policy step should preserve exactly one transaction");
        };
        assert_eq!(
            transaction.selection.source,
            RunDecisionSelectionSourceV1::RewardPolicy
        );
        assert_eq!(transaction.before.decision_step, before_step);
        assert_eq!(transaction.after.decision_step, after_step);
        assert_eq!(outcome.trace_annotations.len(), 1);
        let super::super::trace_annotation::RunControlTraceAnnotationV1::NonCombatPolicyDecision {
            record,
            ..
        } = &outcome.trace_annotations[0]
        else {
            panic!("reward policy transaction should attach noncombat policy evidence");
        };
        crate::ai::noncombat_decision_v1::validate_noncombat_decision_record_v1(record)
            .expect("reward policy record should validate");
        assert_eq!(
            record.site,
            crate::ai::noncombat_decision_v1::DecisionSiteKindV1::Reward
        );
        assert_eq!(
            record.data_role,
            crate::ai::noncombat_decision_v1::DataRoleV1::BehaviorPolicyNotTeacher
        );
    }

    fn assert_reward_owner_transaction(
        outcome: &RunProgressOutcome,
        before_step: u64,
        after_step: u64,
    ) {
        let Some(transaction) = outcome.single_decision_transaction() else {
            panic!("one reward owner step should preserve exactly one transaction");
        };
        assert_eq!(
            transaction.selection.source,
            RunDecisionSelectionSourceV1::OwnerPolicy
        );
        assert_eq!(transaction.before.decision_step, before_step);
        assert_eq!(transaction.after.decision_step, after_step);
    }

    fn reward_screen_session(items: Vec<RewardItem>) -> RunControlSession {
        let mut session = RunControlSession::new(super::super::RunControlConfig::default());
        let mut rewards = RewardState::new();
        rewards.items = items;
        session.engine_state = EngineState::RewardScreen(rewards);
        session
    }

    fn full_belt_strength_reward_session() -> RunControlSession {
        let mut session = reward_screen_session(vec![RewardItem::Potion {
            potion_id: PotionId::StrengthPotion,
        }]);
        session.run_state.potions = vec![
            Some(crate::content::potions::Potion::new(
                PotionId::AncientPotion,
                1,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::FearPotion,
                2,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::AttackPotion,
                3,
            )),
        ];
        session
    }

    fn full_belt_gamblers_reward_session() -> RunControlSession {
        let mut session = reward_screen_session(vec![RewardItem::Potion {
            potion_id: PotionId::GamblersBrew,
        }]);
        session.run_state.potions = vec![
            Some(crate::content::potions::Potion::new(
                PotionId::FearPotion,
                11,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::FearPotion,
                19,
            )),
            Some(crate::content::potions::Potion::new(
                PotionId::FirePotion,
                7,
            )),
        ];
        session
    }
}
