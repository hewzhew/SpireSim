// Java: Bonfire (shrines) — "Bonfire Elementals"
// Screen 0 (INTRO): [Approach] → Screen 1
// Screen 1 (CHOOSE): [Offer card] → grid-select to sacrifice a card
// After grid-select selects a card: reward based on offered card's rarity is
// applied before ordinary master-deck removal, matching Java update().
//   (rarity stored in internal_state by Purge handler in run_loop.rs)
//   Curse → SpiritPoop relic (Circlet if already owned)
//   Basic → nothing
//   Common/Special → heal 5
//   Uncommon → heal to full
//   Rare → +10 maxHP + heal to full
// Screen 3 (COMPLETE): [Leave]

use crate::content::cards::CardRarity;
use crate::content::relics::RelicId;
use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{EventChoiceMeta, EventId, EventOption, EventState};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;
use sts_presentation::Locale;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferReward {
    Nothing,
    Heal(i32),
    HealToFull,
    MaxHpAndHealToFull(i32),
    Relic(RelicId),
}

pub fn offer_reward_for_rarity(rarity: CardRarity, owns_spirit_poop: bool) -> OfferReward {
    match rarity {
        CardRarity::Basic => OfferReward::Nothing,
        CardRarity::Common | CardRarity::Special => OfferReward::Heal(5),
        CardRarity::Uncommon => OfferReward::HealToFull,
        CardRarity::Rare => OfferReward::MaxHpAndHealToFull(10),
        CardRarity::Curse => OfferReward::Relic(if owns_spirit_poop {
            RelicId::Circlet
        } else {
            RelicId::SpiritPoop
        }),
    }
}

pub const OFFER_RARITIES: [CardRarity; 6] = [
    CardRarity::Basic,
    CardRarity::Common,
    CardRarity::Special,
    CardRarity::Uncommon,
    CardRarity::Rare,
    CardRarity::Curse,
];

pub fn describe_offer_reward(reward: OfferReward, healing_blocked: bool, locale: Locale) -> String {
    let text = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("event", &format!("bonfire.{key}"), parameters)
            .expect("bonfire reward rule")
    };
    let bloom = locale.entity("relic", RelicId::MarkOfTheBloom.stable_id());
    let rule = match reward {
        OfferReward::Nothing => text("nothing", &[]),
        OfferReward::Heal(hp) => text("heal", &[("hp", &hp.to_string())]),
        OfferReward::HealToFull => text("healFull", &[]),
        OfferReward::MaxHpAndHealToFull(hp) => {
            if healing_blocked {
                return text("maxHpBlocked", &[("hp", &hp.to_string()), ("relic", bloom)]);
            }
            text("maxHpFull", &[("hp", &hp.to_string())])
        }
        OfferReward::Relic(relic) => text(
            "relic",
            &[("relic", locale.entity("relic", relic.stable_id()))],
        ),
    };
    if healing_blocked
        && matches!(
            reward,
            OfferReward::Heal(_) | OfferReward::HealToFull | OfferReward::MaxHpAndHealToFull(_)
        )
    {
        text("healingBlocked", &[("rule", &rule), ("relic", bloom)])
    } else {
        rule
    }
}

pub fn describe_offering_rewards(
    owns_spirit_poop: bool,
    healing_blocked: bool,
    locale: Locale,
) -> String {
    OFFER_RARITIES
        .into_iter()
        .map(|rarity| {
            locale
                .text(
                    "event",
                    "bonfire.rarityRule",
                    &[
                        (
                            "rarity",
                            locale.entity("cardRarity", rarity.vocabulary_token_v1()),
                        ),
                        (
                            "reward",
                            &describe_offer_reward(
                                offer_reward_for_rarity(rarity, owns_spirit_poop),
                                healing_blocked,
                                locale,
                            ),
                        ),
                    ],
                )
                .expect("bonfire rarity rule")
        })
        .collect::<Vec<_>>()
        .join(
            locale
                .entry("event", "bonfire.separator")
                .expect("bonfire rule separator"),
        )
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    let owns_spirit_poop = run_state.relics.iter().any(|r| r.id == RelicId::SpiritPoop);
    let healing_blocked = run_state
        .relics
        .iter()
        .any(|r| r.id == RelicId::MarkOfTheBloom);
    let rewards = describe_offering_rewards(owns_spirit_poop, healing_blocked, Locale::En);
    crate::content::events::bonfire_options(
        event_state,
        "[Approach] Investigate the bonfire.",
        format!("[Offer] Remove one eligible card. Reward by its rarity — {rewards}."),
    )
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn handle_choice(engine_state: &mut EngineState, run_state: &mut RunState, _choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            // Approach → go to card sacrifice screen
            event_state.current_screen = 1;
        }
        1 => {
            if crate::state::core::has_non_bottled_purgeable_master_deck_card(run_state) {
                event_state.current_screen = 2;
                // Sacrifice a card via grid-select.
                // The Purge handler in run_loop.rs stores the removed card's rarity
                // in event_state.internal_state before removal.
                run_state.event_state = Some(event_state);
                *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                    selected: Vec::new(),
                    min_choices: 1,
                    max_choices: 1,
                    reason: RunPendingChoiceReason::PurgeNonBottled,
                    source: DomainEventSource::Event(EventId::BonfireElementals),
                    return_state: Box::new(EngineState::EventRoom),
                });
                return;
            } else {
                event_state.current_screen = 3;
            }
        }
        2 => {
            // Returned from purge. Read rarity from internal_state
            // (set by Purge handler: 0=Curse, 1=Basic, 2=Common, 3=Special, 4=Uncommon, 5=Rare)
            let rarity = event_state.internal_state;
            apply_offer_reward(engine_state, run_state, rarity);
            event_state.current_screen = 3;
        }
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

pub fn apply_offer_reward(engine_state: &mut EngineState, run_state: &mut RunState, rarity: i32) {
    let source = DomainEventSource::Event(EventId::BonfireElementals);
    let rarity = match rarity {
        0 => CardRarity::Curse,
        1 => CardRarity::Basic,
        2 => CardRarity::Common,
        3 => CardRarity::Special,
        4 => CardRarity::Uncommon,
        5 => CardRarity::Rare,
        _ => return,
    };
    let owns_spirit_poop = run_state.relics.iter().any(|r| r.id == RelicId::SpiritPoop);
    match offer_reward_for_rarity(rarity, owns_spirit_poop) {
        OfferReward::Relic(relic_id) => {
            if let Some(next_state) =
                run_state.obtain_relic_with_source(relic_id, EngineState::EventRoom, source)
            {
                *engine_state = next_state;
            }
        }
        OfferReward::Nothing => {}
        OfferReward::Heal(hp) => {
            run_state.heal_with_source(hp, source);
        }
        OfferReward::HealToFull => {
            run_state.heal_with_source(run_state.max_hp, source);
        }
        OfferReward::MaxHpAndHealToFull(hp) => {
            // Rare -> Java increaseMaxHp(10, false), then heal(maxHealth).
            run_state.gain_max_hp_with_source(hp, hp, source);
            run_state.heal_with_source(run_state.max_hp, source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_choices, get_options, handle_choice};
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason};
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionConstraint,
        EventOptionTransition, EventSelectionKind, EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn bonfire_run(rarity_state: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        let mut event_state = EventState::new(EventId::BonfireElementals);
        event_state.current_screen = 2;
        event_state.internal_state = rarity_state;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        run_state
    }

    fn deck_card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    #[test]
    fn structured_options_expose_bonfire_offer_selection_boundary() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![deck_card(CardId::Strike, 101)];
        let mut event_state = EventState::new(EventId::BonfireElementals);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);

        let options = get_options(&run_state, run_state.event_state.as_ref().unwrap());

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert!(options[0]
            .semantics
            .effects
            .contains(&EventEffect::RemoveCard {
                count: 1,
                target_lineage: None,
                kind: EventCardKind::Unknown,
            }));
        assert!(options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresNonBottledPurgeableCard));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::OfferCard)
        );
    }

    #[test]
    fn structured_options_expose_intro_and_complete_boundaries() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let mut intro = EventState::new(EventId::BonfireElementals);
        intro.current_screen = 0;
        let intro_options = get_options(&run_state, &intro);
        assert_eq!(intro_options[0].semantics.action, EventActionKind::Continue);
        assert_eq!(
            intro_options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut complete = EventState::new(EventId::BonfireElementals);
        complete.current_screen = 3;
        let complete_options = get_options(&run_state, &complete);
        assert_eq!(complete_options[0].semantics.action, EventActionKind::Leave);
        assert!(complete_options[0].semantics.terminal);
    }

    #[test]
    fn offer_without_purgeable_card_advances_without_pending_like_java() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![deck_card(CardId::AscendersBane, 11)];
        let mut event_state = EventState::new(EventId::BonfireElementals);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        let choices = get_choices(&run_state, run_state.event_state.as_ref().unwrap());
        assert!(
            !choices[0].disabled,
            "Java Bonfire keeps Offer clickable and handles the empty group in buttonEffect"
        );

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 3);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(!run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SpiritPoop));
        assert!(run_state.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn offer_selection_excludes_bottled_and_unpurgeable_cards_like_java() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101),
            deck_card(CardId::Defend, 102),
            deck_card(CardId::AscendersBane, 103),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        let mut event_state = EventState::new(EventId::BonfireElementals);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Bonfire should open deck purge selection");
        };
        assert_eq!(choice.reason, RunPendingChoiceReason::PurgeNonBottled);
        let request = choice.selection_request(&run_state);
        assert_eq!(request.reason, SelectionReason::Purge);
        assert_eq!(
            request.targets,
            vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(102)
            )],
            "Java opens CardGroup.getGroupWithoutBottledCards(masterDeck.getPurgeableCards())"
        );
    }

    #[test]
    fn offer_removes_selected_card_with_event_source_and_applies_post_selection_reward() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![deck_card(CardId::Strike, 101)];
        let mut event_state = EventState::new(EventId::BonfireElementals);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(
            run_state.event_state.as_ref().unwrap().current_screen,
            3,
            "Java Bonfire applies the selected-card callback during the grid-select update path"
        );
        assert_eq!(
            run_state.event_state.as_ref().unwrap().internal_state,
            1,
            "Strike is Basic, and Bonfire rewards read the offered card rarity after selection"
        );
        assert!(run_state.master_deck.is_empty());
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::BonfireElementals),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
    }

    #[test]
    fn common_offer_selection_heals_during_post_selection_callback() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 10;
        run_state.max_hp = 80;
        run_state.master_deck = vec![deck_card(CardId::PommelStrike, 101)];
        let mut event_state = EventState::new(EventId::BonfireElementals);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )],
            })),
        ));

        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 3);
        assert_eq!(run_state.current_hp, 15);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: 5,
                current_hp: 15,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::BonfireElementals),
            }
        )));
    }

    #[test]
    fn curse_offer_selection_applies_reward_before_remove_hook_like_java() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 80;
        run_state.max_hp = 80;
        run_state.master_deck = vec![deck_card(CardId::Parasite, 101)];
        let mut event_state = EventState::new(EventId::BonfireElementals);
        event_state.current_screen = 1;
        run_state.event_state = Some(event_state);
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let mut combat_state = None;
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            Some(ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Deck,
                selected: vec![SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )],
            })),
        ));

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 3);
        assert_eq!(run_state.max_hp, 77);
        let events = run_state.take_emitted_events();
        let relic_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::RelicObtained {
                        relic_id: RelicId::SpiritPoop,
                        source: DomainEventSource::Event(EventId::BonfireElementals),
                    }
                )
            })
            .expect("curse offer should obtain Spirit Poop");
        let removed_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardRemoved {
                        card,
                        source: DomainEventSource::Event(EventId::BonfireElementals),
                    } if card.id == CardId::Parasite && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
                )
            })
            .expect("the offered curse should still be removed");
        let max_hp_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::MaxHpChanged {
                        delta: -3,
                        source: DomainEventSource::Event(EventId::BonfireElementals),
                        ..
                    }
                )
            })
            .expect("Parasite remove hook should still run");

        assert!(
            relic_pos < removed_pos && relic_pos < max_hp_pos,
            "Java Bonfire.setReward(offeredCard.rarity) runs before masterDeck.removeCard(offeredCard)"
        );
    }

    #[test]
    fn common_offer_heals_with_event_source() {
        let mut run_state = bonfire_run(2);
        run_state.current_hp = 10;
        run_state.max_hp = 80;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 15);
        assert!(run_state.take_emitted_events().iter().any(|event| {
            matches!(
                event,
                DomainEvent::HpChanged {
                    delta: 5,
                    current_hp: 15,
                    max_hp: 80,
                    source: DomainEventSource::Event(EventId::BonfireElementals)
                }
            )
        }));
    }

    #[test]
    fn rare_offer_matches_java_max_hp_then_full_heal_sequence() {
        let mut run_state = bonfire_run(5);
        run_state.current_hp = 30;
        run_state.max_hp = 80;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.max_hp, 90);
        assert_eq!(run_state.current_hp, 90);
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::MaxHpChanged {
                    delta: 10,
                    current_hp: 40,
                    max_hp: 90,
                    source: DomainEventSource::Event(EventId::BonfireElementals)
                }
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::HpChanged {
                    delta: 50,
                    current_hp: 90,
                    max_hp: 90,
                    source: DomainEventSource::Event(EventId::BonfireElementals)
                }
            )
        }));
    }

    #[test]
    fn heal_rewards_obey_mark_of_the_bloom() {
        let mut run_state = bonfire_run(4);
        run_state.current_hp = 10;
        run_state.max_hp = 80;
        run_state
            .relics
            .push(RelicState::new(RelicId::MarkOfTheBloom));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 10);
        assert!(!run_state
            .take_emitted_events()
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
    }

    #[test]
    fn curse_offer_obtains_spirit_poop_with_event_source() {
        let mut run_state = bonfire_run(0);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SpiritPoop));
        assert!(run_state.take_emitted_events().iter().any(|event| {
            matches!(
                event,
                DomainEvent::RelicObtained {
                    relic_id: RelicId::SpiritPoop,
                    source: DomainEventSource::Event(EventId::BonfireElementals)
                }
            )
        }));
    }
}
