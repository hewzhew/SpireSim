use crate::content::cards::{CardId, CardTag};
use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn get_hp_loss(run_state: &RunState) -> i32 {
    let mut loss = (run_state.max_hp as f32 * 0.3).ceil() as i32;
    if loss >= run_state.max_hp {
        loss = run_state.max_hp - 1;
    }
    loss
}

fn starter_strike_replacement_effects(run_state: &RunState) -> Vec<EventEffect> {
    let mut effects = run_state
        .master_deck
        .iter()
        .rev()
        .filter(|card| {
            crate::content::cards::get_card_definition(card.id)
                .tags
                .contains(&CardTag::StarterStrike)
        })
        .map(|card| EventEffect::RemoveCard {
            count: 1,
            target_lineage: Some(card.lineage),
            kind: EventCardKind::Specific(card.id),
        })
        .collect::<Vec<_>>();
    effects.push(EventEffect::ObtainCard {
        count: 5,
        kind: EventCardKind::Specific(CardId::Bite),
    });
    effects
}

pub fn get_choices(run_state: &RunState, event_state: &EventState) -> Vec<EventChoiceMeta> {
    get_options(run_state, event_state)
        .into_iter()
        .map(|option| option.ui)
        .collect()
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    if event_state.current_screen != 0 {
        return vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                effects: vec![],
                constraints: vec![],
                transition: EventOptionTransition::Complete,
                repeatable: false,
                terminal: true,
            },
        )];
    }

    let hp_loss = get_hp_loss(run_state);
    let mut accept_effects = vec![EventEffect::LoseMaxHp(hp_loss)];
    accept_effects.extend(starter_strike_replacement_effects(run_state));
    let mut choices = vec![EventOption::new(
        EventChoiceMeta::new(format!(
            "[Accept] Lose {} Max HP. Replace all Strikes with 5 Bites.",
            hp_loss
        )),
        EventOptionSemantics {
            action: EventActionKind::Accept,
            effects: accept_effects,
            constraints: vec![],
            transition: EventOptionTransition::AdvanceScreen,
            repeatable: false,
            terminal: false,
        },
    )];

    let has_vial = run_state.relics.iter().any(|r| r.id == RelicId::BloodVial);
    let mut vial_effects = vec![EventEffect::LoseRelic {
        specific: Some(RelicId::BloodVial),
        starter_only: false,
    }];
    vial_effects.extend(starter_strike_replacement_effects(run_state));
    if has_vial {
        choices.push(EventOption::new(
            EventChoiceMeta::new("[Give Vial] Lose Blood Vial. Replace all Strikes with 5 Bites."),
            EventOptionSemantics {
                action: EventActionKind::Trade,
                effects: vial_effects,
                constraints: vec![EventOptionConstraint::RequiresRelic(RelicId::BloodVial)],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        ));
    }

    choices.push(EventOption::new(
        EventChoiceMeta::new("[Refuse] Leave."),
        EventOptionSemantics {
            action: EventActionKind::Decline,
            effects: vec![],
            constraints: vec![],
            transition: EventOptionTransition::AdvanceScreen,
            repeatable: false,
            terminal: false,
        },
    ));
    choices
}

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    // Accept: Max HP loss
                    let hp_loss = get_hp_loss(run_state);
                    let source = DomainEventSource::Event(EventId::Vampires);
                    run_state.lose_max_hp_with_source(hp_loss, source);
                    replace_attacks(run_state, source);
                    event_state.current_screen = 1;
                }
                1 => {
                    // Give Vial -> Requires BloodVial
                    let source = DomainEventSource::Event(EventId::Vampires);
                    if let Some(pos) = run_state
                        .relics
                        .iter()
                        .position(|r| r.id == RelicId::BloodVial)
                    {
                        run_state.remove_relic_at_with_source(pos, source);
                        replace_attacks(run_state, source);
                        event_state.current_screen = 1;
                    } else {
                        // Without a vial the original dialog omits the trade
                        // button entirely; button 1 is Refuse.
                        event_state.current_screen = 2;
                    }
                }
                _ => {
                    // Refuse
                    event_state.current_screen = 2;
                }
            }
        }
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

fn replace_attacks(run_state: &mut RunState, source: DomainEventSource) {
    // Identify Strikes to remove
    let strikes_to_remove: Vec<crate::runtime::combat::CardLineageId> = run_state
        .master_deck
        .iter()
        .rev()
        .filter(|card| {
            let def = crate::content::cards::get_card_definition(card.id);
            def.tags.contains(&CardTag::StarterStrike)
        })
        .map(|card| card.lineage)
        .collect();

    for lineage in strikes_to_remove {
        run_state.remove_card_from_deck_with_source(lineage, source);
    }

    // Add 5 Bites through the DeckManager pipeline
    for _ in 0..5 {
        super::obtain_event_card(run_state, EventId::Vampires, CardId::Bite);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;
    use crate::state::events::{
        EventActionKind, EventCardKind, EventEffect, EventId, EventOptionConstraint,
        EventOptionTransition, EventState,
    };
    use crate::state::selection::{DomainEvent, DomainEventSource};

    #[test]
    fn give_vial_option_exposes_constraint_and_bite_reward() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.relics.push(RelicState::new(RelicId::BloodVial));
        rs.event_state = Some(EventState::new(EventId::Vampires));
        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        let give_vial = &options[1];

        assert!(!give_vial.ui.disabled);
        assert_eq!(give_vial.semantics.action, EventActionKind::Trade);
        assert_eq!(
            give_vial.semantics.constraints,
            vec![EventOptionConstraint::RequiresRelic(RelicId::BloodVial)]
        );
        assert!(give_vial
            .semantics
            .effects
            .contains(&EventEffect::ObtainCard {
                count: 5,
                kind: EventCardKind::Specific(CardId::Bite),
            }));
        assert_eq!(
            give_vial.semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
    }

    #[test]
    fn options_expose_each_starter_strike_removed_in_original_game_order() {
        let mut rs = RunState::new(1, 0, false, "Ironclad");
        rs.master_deck = vec![
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 101),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Defend, 102),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 103),
            crate::runtime::combat::CombatCard::for_scenario(CardId::StrikeG, 104),
        ];
        rs.relics.push(RelicState::new(RelicId::BloodVial));
        rs.event_state = Some(EventState::new(EventId::Vampires));

        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        let expected_replacement = vec![
            EventEffect::RemoveCard {
                count: 1,
                target_lineage: Some(crate::runtime::combat::CardLineageId::constructed_scenario(
                    104,
                )),
                kind: EventCardKind::Specific(CardId::StrikeG),
            },
            EventEffect::RemoveCard {
                count: 1,
                target_lineage: Some(crate::runtime::combat::CardLineageId::constructed_scenario(
                    103,
                )),
                kind: EventCardKind::Specific(CardId::Strike),
            },
            EventEffect::RemoveCard {
                count: 1,
                target_lineage: Some(crate::runtime::combat::CardLineageId::constructed_scenario(
                    101,
                )),
                kind: EventCardKind::Specific(CardId::Strike),
            },
            EventEffect::ObtainCard {
                count: 5,
                kind: EventCardKind::Specific(CardId::Bite),
            },
        ];

        assert_eq!(&options[0].semantics.effects[1..], expected_replacement);
        assert_eq!(&options[1].semantics.effects[1..], expected_replacement);
    }

    #[test]
    fn accept_loses_max_hp_replaces_starter_strikes_with_event_sources() {
        let mut rs = RunState::new(1, 0, false, "Ironclad");
        rs.current_hp = 70;
        rs.max_hp = 80;
        rs.event_state = Some(EventState::new(EventId::Vampires));
        rs.emitted_events.clear();
        let starter_strikes = rs
            .master_deck
            .iter()
            .filter(|card| {
                crate::content::cards::get_card_definition(card.id)
                    .tags
                    .contains(&CardTag::StarterStrike)
            })
            .count();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.max_hp, 56);
        assert_eq!(rs.current_hp, 56);
        assert_eq!(
            rs.master_deck
                .iter()
                .filter(|card| card.id == CardId::Bite)
                .count(),
            5
        );
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::MaxHpChanged {
                delta: -24,
                current_hp: 56,
                max_hp: 56,
                source: DomainEventSource::Event(EventId::Vampires),
            }
        )));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::CardRemoved {
                        source: DomainEventSource::Event(EventId::Vampires),
                        ..
                    }
                ))
                .count(),
            starter_strikes
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::Vampires),
                    } if card.id == CardId::Bite
                ))
                .count(),
            5
        );
    }

    #[test]
    fn replace_attacks_removes_starter_strikes_in_reverse_master_deck_order_like_java() {
        let mut rs = RunState::new(1, 0, false, "Ironclad");
        rs.master_deck = vec![
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 101),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Defend, 102),
            crate::runtime::combat::CombatCard::for_scenario(CardId::Strike, 103),
            crate::runtime::combat::CombatCard::for_scenario(CardId::StrikeG, 104),
        ];
        rs.event_state = Some(EventState::new(EventId::Vampires));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        let removed_lineages = rs
            .take_emitted_events()
            .iter()
            .filter_map(|event| match event {
                DomainEvent::CardRemoved {
                    card,
                    source: DomainEventSource::Event(EventId::Vampires),
                } => Some(card.lineage),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            removed_lineages,
            vec![
                crate::runtime::combat::CardLineageId::constructed_scenario(104),
                crate::runtime::combat::CardLineageId::constructed_scenario(103),
                crate::runtime::combat::CardLineageId::constructed_scenario(101),
            ],
            "Java Vampires.replaceAttacks iterates masterDeck from size - 1 down to 0"
        );
    }

    #[test]
    fn give_vial_removes_relic_without_max_hp_loss_and_replaces_strikes() {
        let mut rs = RunState::new(1, 0, false, "Ironclad");
        rs.current_hp = 70;
        rs.max_hp = 80;
        rs.relics.push(RelicState::new(RelicId::BloodVial));
        rs.event_state = Some(EventState::new(EventId::Vampires));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(rs.max_hp, 80);
        assert_eq!(rs.current_hp, 70);
        assert!(!rs.relics.iter().any(|relic| relic.id == RelicId::BloodVial));
        assert_eq!(
            rs.master_deck
                .iter()
                .filter(|card| card.id == CardId::Bite)
                .count(),
            5
        );
        let events = rs.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::RelicLost {
                relic_id: RelicId::BloodVial,
                source: DomainEventSource::Event(EventId::Vampires),
            }
        )));
        assert!(!events
            .iter()
            .any(|event| matches!(event, DomainEvent::MaxHpChanged { .. })));
    }

    #[test]
    fn refusal_without_vial_uses_second_button_and_preserves_deck() {
        let mut rs = RunState::new(1, 0, false, "Ironclad");
        rs.current_hp = 70;
        rs.max_hp = 80;
        rs.event_state = Some(EventState::new(EventId::Vampires));
        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        assert_eq!(options.len(), 2);
        assert_eq!(options[1].semantics.action, EventActionKind::Decline);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert!(!options[1].semantics.terminal);
        rs.emitted_events.clear();
        let before = rs
            .master_deck
            .iter()
            .map(|card| (card.id, card.lineage))
            .collect::<Vec<_>>();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 1);

        assert_eq!(
            rs.master_deck
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 2);
        assert!(!rs.event_state.as_ref().unwrap().completed);
        assert!(rs.take_emitted_events().is_empty());
        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].semantics.action, EventActionKind::Leave);
        handle_choice(&mut engine_state, &mut rs, 0);
        assert!(rs.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn refusal_with_vial_uses_third_button_and_keeps_vial() {
        let mut rs = RunState::new(1, 0, false, "Ironclad");
        rs.relics.push(RelicState::new(RelicId::BloodVial));
        rs.event_state = Some(EventState::new(EventId::Vampires));
        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        assert_eq!(options.len(), 3);
        assert_eq!(options[2].semantics.action, EventActionKind::Decline);
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 2);
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 2);
        assert!(!rs.event_state.as_ref().unwrap().completed);
        assert!(rs.relics.iter().any(|r| r.id == RelicId::BloodVial));
        assert!(rs.take_emitted_events().is_empty());
        handle_choice(&mut engine_state, &mut rs, 0);
        assert!(rs.event_state.as_ref().unwrap().completed);
    }

    #[test]
    fn accept_max_hp_loss_and_strike_removals_precede_delayed_bite_obtains() {
        let mut rs = RunState::new(1, 0, false, "Ironclad");
        rs.current_hp = 70;
        rs.max_hp = 80;
        rs.gold = 0;
        rs.relics.push(RelicState::new(RelicId::CeramicFish));
        rs.event_state = Some(EventState::new(EventId::Vampires));
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.max_hp, 56);
        assert_eq!(rs.current_hp, 56);
        assert_eq!(rs.gold, 45);
        let labels = rs
            .take_emitted_events()
            .into_iter()
            .filter_map(|event| match event {
                DomainEvent::MaxHpChanged {
                    delta: -24,
                    source: DomainEventSource::Event(EventId::Vampires),
                    ..
                } => Some("max_hp_loss"),
                DomainEvent::CardRemoved {
                    source: DomainEventSource::Event(EventId::Vampires),
                    ..
                } => Some("strike_removed"),
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Event(EventId::Vampires),
                    ..
                } => Some("ceramic_fish_gold"),
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Event(EventId::Vampires),
                } if card.id == CardId::Bite => Some("bite_obtained"),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "max_hp_loss",
                "strike_removed",
                "strike_removed",
                "strike_removed",
                "strike_removed",
                "strike_removed",
                "ceramic_fish_gold",
                "bite_obtained",
                "ceramic_fish_gold",
                "bite_obtained",
                "ceramic_fish_gold",
                "bite_obtained",
                "ceramic_fish_gold",
                "bite_obtained",
                "ceramic_fish_gold",
                "bite_obtained",
            ],
            "Java Vampires decreases max HP, removes all starter Strikes, then delayed Bite effects resolve obtain hooks before Soul.obtain"
        );
    }
}
