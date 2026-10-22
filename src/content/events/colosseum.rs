// Java: Colosseum (city)
// Screen 0 (INTRO): [Proceed] → fight description
// Screen 1 (FIGHT): [Fight!] → combat with "Colosseum Slavers" (no rewards, rewardAllowed=false)
// After first combat → reopen() → Screen 2 (POST_COMBAT):
//   [Flee] → leave (openMap)
//   [Fight] → combat with "Colosseum Nobs" (rewards: RARE relic, UNCOMMON relic, 100g)
// Screen 3 (LEAVE): [Leave] → openMap

use crate::content::monsters::factory::EncounterId;
use crate::rewards::state::{RewardItem, RewardState};
use crate::state::core::{CombatStartRequest, EngineState, PostCombatReturn};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventOption, EventOptionSemantics,
    EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use sts_presentation::Locale;

const FIRST_ENCOUNTER: EncounterId = EncounterId::ColosseumSlavers;
const SECOND_ENCOUNTER: EncounterId = EncounterId::ColosseumNobs;

/// Public fixed encounters and return behavior, shared by raw and localized
/// option readings. No encounter is constructed or sampled here.
pub fn option_rule(screen: usize, option: usize, locale: Locale) -> Option<String> {
    let key = match (screen, option) {
        (0, 0) => "intro",
        (1, 0) => "firstFight",
        (2, 1) => "secondFight",
        _ => return None,
    };
    let name = |encounter: EncounterId| match locale {
        Locale::En => encounter.stable_id().to_owned(),
        Locale::ZhCn => format!(
            "{} [{}]",
            locale.entity("encounter", encounter.stable_id()),
            encounter.stable_id()
        ),
    };
    Some(
        locale
            .text(
                "event",
                &format!("colosseum.{key}"),
                &[
                    ("first", &name(FIRST_ENCOUNTER)),
                    ("second", &name(SECOND_ENCOUNTER)),
                ],
            )
            .expect("Colosseum option rule"),
    )
}

pub fn get_options(_run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => {
            // Introduction
            vec![EventOption::new(
                EventChoiceMeta::new(format!(
                    "[Proceed] {}",
                    option_rule(0, 0, Locale::En).unwrap()
                )),
                EventOptionSemantics {
                    action: EventActionKind::Continue,
                    transition: EventOptionTransition::AdvanceScreen,
                    ..Default::default()
                },
            )]
        }
        1 => {
            // Ready to fight
            vec![EventOption::new(
                EventChoiceMeta::new(format!(
                    "[Fight!] {}",
                    option_rule(1, 0, Locale::En).unwrap()
                )),
                EventOptionSemantics {
                    action: EventActionKind::Fight,
                    effects: vec![EventEffect::StartCombat],
                    transition: EventOptionTransition::StartCombat,
                    ..Default::default()
                },
            )]
        }
        2 => {
            // Post-first-combat: choose to fight Nobs or flee
            vec![
                EventOption::new(
                    EventChoiceMeta::new("[Flee] Leave the Colosseum."),
                    EventOptionSemantics {
                        action: EventActionKind::Leave,
                        transition: EventOptionTransition::Complete,
                        terminal: true,
                        ..Default::default()
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Fight] {}",
                        option_rule(2, 1, Locale::En).unwrap()
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::Fight,
                        effects: vec![
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomRareRelic,
                            },
                            EventEffect::ObtainRelic {
                                count: 1,
                                kind: EventRelicKind::RandomUncommonRelic,
                            },
                            EventEffect::GainGold(100),
                            EventEffect::StartCombat,
                        ],
                        transition: EventOptionTransition::StartCombat,
                        terminal: true,
                        ..Default::default()
                    },
                ),
            ]
        }
        _ => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..Default::default()
            },
        )],
    }
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            // Intro → ready to fight
            event_state.current_screen = 1;
        }
        1 => {
            // First fight: Colosseum Slavers (no rewards)
            // Java: rewardAllowed = false, enterCombatFromImage()
            // After combat, return to EventRoom → screen 2 (POST_COMBAT)
            event_state.current_screen = 2;
            run_state.event_state = Some(event_state);
            *engine_state = EngineState::CombatStart(CombatStartRequest::event(
                FIRST_ENCOUNTER,
                RewardState::new(),
                false,
                true,
                false,
                PostCombatReturn::EventRoom,
            ));
            return;
        }
        2 => {
            match choice_idx {
                0 => {
                    // Flee — leave the Colosseum
                    event_state.completed = true;
                }
                _ => {
                    // Fight Nobs: set up rewards BEFORE combat (matches Java)
                    // Java: addRelicToRewards(RARE), addRelicToRewards(UNCOMMON), addGoldToRewards(100)
                    let mut rewards = RewardState::new();

                    let rare_relic =
                        run_state.random_screenless_relic(crate::content::relics::RelicTier::Rare);
                    rewards.items.push(RewardItem::Relic {
                        relic_id: rare_relic,
                    });

                    let uncommon_relic = run_state
                        .random_screenless_relic(crate::content::relics::RelicTier::Uncommon);
                    rewards.items.push(RewardItem::Relic {
                        relic_id: uncommon_relic,
                    });

                    // Gold reward
                    rewards.items.push(RewardItem::Gold { amount: 100 });

                    event_state.current_screen = 3;
                    event_state.completed = true;
                    run_state.event_state = Some(event_state);
                    *engine_state = EngineState::CombatStart(CombatStartRequest::event(
                        SECOND_ENCOUNTER,
                        rewards,
                        true,
                        false,
                        true,
                        PostCombatReturn::MapNavigation,
                    ));
                    return;
                }
            }
        }
        _ => {
            // Post-second-combat leave
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::core::CombatContext;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionTransition, EventRelicKind,
    };

    #[test]
    fn structured_options_expose_colosseum_combat_boundaries() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let intro = EventState::new(EventId::Colosseum);
        let intro_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state, &intro,
            )
            .expect("Colosseum should expose structured event options");

        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut first_fight = EventState::new(EventId::Colosseum);
        first_fight.current_screen = 1;
        let first_fight_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &first_fight,
            )
            .expect("Colosseum first fight screen should expose structured event options");

        assert_eq!(
            first_fight_options[0].semantics.action,
            EventActionKind::Fight
        );
        assert!(first_fight_options[0]
            .semantics
            .effects
            .contains(&EventEffect::StartCombat));
        assert_eq!(
            first_fight_options[0].semantics.transition,
            EventOptionTransition::StartCombat
        );
        assert!(
            !first_fight_options[0].semantics.terminal,
            "first Colosseum fight returns to the event room after combat"
        );

        let mut post_combat = EventState::new(EventId::Colosseum);
        post_combat.current_screen = 2;
        let post_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &post_combat,
            )
            .expect("Colosseum post-combat screen should expose structured event options");

        assert_eq!(post_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            post_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
        assert_eq!(post_options[1].semantics.action, EventActionKind::Fight);
        assert!(post_options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomRareRelic,
            }));
        assert!(post_options[1]
            .semantics
            .effects
            .contains(&EventEffect::ObtainRelic {
                count: 1,
                kind: EventRelicKind::RandomUncommonRelic,
            }));
        assert!(post_options[1]
            .semantics
            .effects
            .contains(&EventEffect::GainGold(100)));
        assert!(post_options[1]
            .semantics
            .effects
            .contains(&EventEffect::StartCombat));
        assert_eq!(
            post_options[1].semantics.transition,
            EventOptionTransition::StartCombat
        );
        assert!(post_options[1].semantics.terminal);
    }

    #[test]
    fn first_fight_returns_to_event_room_without_rewards_or_elite_trigger() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.event_state = Some(crate::state::events::EventState::new(
            crate::state::events::EventId::Colosseum,
        ));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("first Colosseum fight should request CombatStart");
        };
        assert_eq!(request.encounter_id, EncounterId::ColosseumSlavers);
        let CombatContext::Event(combat) = request.context else {
            panic!("first Colosseum fight should carry event combat context");
        };
        assert!(!combat.reward_allowed);
        assert!(combat.no_cards_in_rewards);
        assert!(!combat.elite_trigger);
        assert!(matches!(
            combat.post_combat_return,
            PostCombatReturn::EventRoom
        ));
    }

    #[test]
    fn second_fight_preserves_java_elite_trigger_without_normal_elite_rewards() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.event_state = Some(crate::state::events::EventState::new(
            crate::state::events::EventId::Colosseum,
        ));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);
        run_state.event_state.as_mut().unwrap().current_screen = 2;
        engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        let EngineState::CombatStart(request) = engine_state else {
            panic!("second Colosseum fight should request CombatStart");
        };
        assert_eq!(request.encounter_id, EncounterId::ColosseumNobs);
        let CombatContext::Event(combat) = request.context else {
            panic!("second Colosseum fight should carry event combat context");
        };
        assert!(combat.reward_allowed);
        assert!(combat.elite_trigger);
        assert_eq!(combat.rewards.items.len(), 3);
        assert!(combat
            .rewards
            .items
            .iter()
            .any(|item| matches!(item, RewardItem::Gold { amount: 100 })));
        assert!(combat
            .rewards
            .items
            .iter()
            .any(|item| matches!(item, RewardItem::Relic { .. })));
    }
}
