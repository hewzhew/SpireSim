//! Event policy bridge owned by the strategic-owner crate.

use sts_simulator::eval::run_control::{
    exact_direct_card_acquisition_policy_prior_v1, DecisionCandidateKey, DecisionSurface,
    RunControlSession, RunPolicyCandidateV1,
};
use sts_simulator::state::events::{EventCardKind, EventEffect, EventOption};

use super::owner_model::{OwnerDecision, OwnerRoutine};

pub(super) fn event_owner_decision(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> OwnerDecision {
    match direct_card_choice_decision(session, surface) {
        Ok(Some(decision)) => return decision,
        Ok(None) => {}
        Err(err) => return OwnerDecision::Gap(err),
    }
    match sts_run_strategy::owner_policy::event_owner_policy_selector(&session.run_state) {
        Ok(selector) => visible_event_option_decision(session, surface, &selector),
        Err(sts_run_strategy::owner_policy::EventOwnerPolicyGap::UnownedEvent(_)) => {
            OwnerDecision::Candidates(Vec::new())
        }
        Err(err) => OwnerDecision::Gap(format!("{err:?}")),
    }
}

fn direct_card_choice_decision(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> Result<Option<OwnerDecision>, String> {
    let Some(event) = session.run_state.event_state.as_ref() else {
        return Ok(None);
    };
    let options = sts_simulator::engine::event_handler::get_event_options(&session.run_state);
    let visible = surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            let Some(DecisionCandidateKey::EventOption {
                event_id,
                screen,
                option_index,
                ..
            }) = candidate.key
            else {
                return None;
            };
            (event_id == event.id && screen == event.current_screen)
                .then(|| options.get(option_index).map(|option| (candidate, option)))
                .flatten()
        })
        .filter(|(_, option)| !option.ui.disabled)
        .collect::<Vec<_>>();
    if visible.len() < 2
        || visible
            .iter()
            .any(|(_, option)| !is_single_direct_card_option(option))
    {
        return Ok(None);
    }

    let legal = visible
        .iter()
        .map(|(candidate, _)| {
            let action = candidate.action.executable_action_ref().ok_or_else(|| {
                format!(
                    "direct card event candidate '{}' is not executable",
                    candidate.id
                )
            })?;
            Ok(RunPolicyCandidateV1 {
                candidate_id: &candidate.id,
                label: &candidate.label,
                action,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let prior = exact_direct_card_acquisition_policy_prior_v1(session, &legal)?;
    let selected_id = prior
        .entries
        .first()
        .map(|entry| entry.candidate_id.as_str())
        .ok_or_else(|| "direct card acquisition policy returned no candidates".to_string())?;
    let selected = visible
        .iter()
        .map(|(candidate, _)| *candidate)
        .find(|candidate| candidate.id == selected_id)
        .ok_or_else(|| {
            format!("direct card acquisition policy selected absent candidate '{selected_id}'")
        })?;
    let action = selected.action.executable_action().ok_or_else(|| {
        format!("selected direct card candidate '{selected_id}' is not executable")
    })?;
    Ok(Some(OwnerDecision::Routine(OwnerRoutine::Candidate {
        candidate_id: selected.id.clone(),
        action,
    })))
}

fn is_single_direct_card_option(option: &EventOption) -> bool {
    matches!(
        option.semantics.effects.as_slice(),
        [EventEffect::ObtainCard {
            count: 1,
            kind: EventCardKind::Specific(_),
        }] | [EventEffect::ObtainColorlessCard {
            count: 1,
            kind: EventCardKind::Specific(_),
        }]
    )
}

fn visible_event_option_decision(
    session: &RunControlSession,
    surface: &DecisionSurface,
    selector: &sts_run_strategy::owner_policy::EventOwnerOptionSelector,
) -> OwnerDecision {
    let Some(event) = session.run_state.event_state.as_ref() else {
        return OwnerDecision::Gap("event owner requires event_state".to_string());
    };
    let options = sts_simulator::engine::event_handler::get_event_options(&session.run_state);
    let matches = surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            let Some(DecisionCandidateKey::EventOption {
                event_id,
                screen,
                option_index,
                ..
            }) = candidate.key
            else {
                return None;
            };
            if event_id != event.id || screen != event.current_screen {
                return None;
            }
            let option = options.get(option_index)?;
            if option.ui.disabled || !selector.matches(option_index, &option.semantics) {
                return None;
            }
            candidate
                .action
                .executable_action()
                .map(|action| (candidate.id.clone(), action))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [(candidate_id, action)] => OwnerDecision::Routine(OwnerRoutine::Candidate {
            candidate_id: candidate_id.clone(),
            action: action.clone(),
        }),
        [] => OwnerDecision::Gap(format!("event selector {selector:?} has no visible option")),
        _ => OwnerDecision::Gap(format!(
            "event selector {selector:?} matched {} visible options",
            matches.len()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_simulator::content::cards::CardId;
    use sts_simulator::content::monsters::factory::EncounterId;
    use sts_simulator::eval::run_control::{
        build_decision_surface, exact_direct_card_acquisition_policy_decision_v1, RunControlConfig,
    };
    use sts_simulator::runtime::combat::CombatCard;
    use sts_simulator::state::core::EngineState;
    use sts_simulator::state::events::{EventId, EventState};

    #[test]
    fn unowned_event_is_absent_policy_but_missing_event_is_an_error() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.event_state = Some(EventState::new(EventId::SpireHeart));
        assert!(
            crate::current_privileged_strategic_owner_action_result_v1(&session)
                .unwrap()
                .is_none()
        );
        session.run_state.event_state = None;
        assert!(crate::current_privileged_strategic_owner_action_result_v1(&session).is_err());
    }

    #[test]
    fn library_card_choice_uses_the_shared_card_acquisition_policy() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 2;
        session.run_state.floor_num = 31;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.current_hp = 76;
        session.run_state.max_hp = 76;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Anger, 0),
            (CardId::Bash, 1),
            (CardId::Bloodletting, 0),
            (CardId::Clothesline, 1),
            (CardId::DarkEmbrace, 0),
            (CardId::DarkShackles, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::DoubleTap, 0),
            (CardId::Exhume, 0),
            (CardId::FiendFire, 0),
            (CardId::Hemokinesis, 0),
            (CardId::Impervious, 0),
            (CardId::PommelStrike, 0),
            (CardId::SecondWind, 0),
            (CardId::SeverSoul, 0),
            (CardId::ShrugItOff, 0),
            (CardId::ShrugItOff, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Whirlwind, 1),
        ]);
        let mut event = EventState::new(EventId::TheLibrary);
        event.current_screen = 1;
        event.card_payload = vec![
            crate::state::events::EventCardState {
                card_id: CardId::Inflame,
                upgrades: 0,
            },
            crate::state::events::EventCardState {
                card_id: CardId::FeelNoPain,
                upgrades: 0,
            },
        ];
        session.run_state.event_state = Some(event);
        session.engine_state = EngineState::EventRoom;

        let surface = build_decision_surface(&session);
        let OwnerDecision::Routine(OwnerRoutine::Candidate { candidate_id, .. }) =
            event_owner_decision(&session, &surface)
        else {
            panic!("Library card choice should produce one routine candidate");
        };
        let selected = surface
            .view
            .candidates
            .iter()
            .find(|candidate| candidate.id == candidate_id)
            .and_then(|candidate| candidate.key.as_ref())
            .expect("selected Library candidate should retain its typed key");
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                matches!(
                    candidate.key,
                    Some(DecisionCandidateKey::EventOption { .. })
                )
                .then(|| {
                    candidate
                        .action
                        .executable_action_ref()
                        .map(|action| RunPolicyCandidateV1 {
                            candidate_id: &candidate.id,
                            label: &candidate.label,
                            action,
                        })
                })
                .flatten()
            })
            .collect::<Vec<_>>();
        let policy = exact_direct_card_acquisition_policy_decision_v1(&session, &legal)
            .expect("shared direct-card policy should explain the Library candidates");
        assert!(
            matches!(
                selected,
                DecisionCandidateKey::EventOption {
                    event_id: EventId::TheLibrary,
                    screen: 1,
                    option_index: 1,
                    ..
                }
            ),
            "selected={selected:?} evidence={:#?}",
            policy.evidence
        );
    }

    fn owned_deck(cards: &[(CardId, u8)]) -> Vec<CombatCard> {
        cards
            .iter()
            .enumerate()
            .map(|(index, (card, upgrades))| {
                let mut owned = CombatCard::for_scenario(*card, index as u32);
                owned.upgrades = *upgrades;
                owned
            })
            .collect()
    }
}
