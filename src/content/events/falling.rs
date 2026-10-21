use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventOption,
    EventOptionConstraint, EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

// internal_state packs pre-selected card indices:
// bits[0..9] = skill card deck index (0x3FF = no skill)
// bits[10..19] = power card deck index (0x3FF = no power)
// bits[20..29] = attack card deck index (0x3FF = no attack)
const NO_CARD: i32 = 0x3FF;

fn skill_idx(s: i32) -> usize {
    (s & 0x3FF) as usize
}
fn power_idx(s: i32) -> usize {
    ((s >> 10) & 0x3FF) as usize
}
fn attack_idx(s: i32) -> usize {
    ((s >> 20) & 0x3FF) as usize
}

fn encode_indices(skill: i32, power: i32, attack: i32) -> i32 {
    (skill & 0x3FF) | ((power & 0x3FF) << 10) | ((attack & 0x3FF) << 20)
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![EventOption::new(
            EventChoiceMeta::new("[Proceed]"),
            EventOptionSemantics {
                action: EventActionKind::Continue,
                effects: vec![],
                constraints: vec![],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        )],
        1 => {
            let s = event_state.internal_state;
            let has_skill = (s & 0x3FF) != NO_CARD;
            let has_power = ((s >> 10) & 0x3FF) != NO_CARD;
            let has_attack = ((s >> 20) & 0x3FF) != NO_CARD;

            if !has_skill && !has_power && !has_attack {
                return vec![EventOption::new(
                    EventChoiceMeta::new("[Land Safely] Nothing happens."),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        effects: vec![],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                )];
            }

            let mut choices = vec![];
            if has_skill {
                let effect = card_remove_effect(run_state, skill_idx(s));
                choices.push(EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Skill] Remove {}.",
                        card_name(run_state, skill_idx(s))
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![effect],
                        constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            } else {
                choices.push(EventOption::new(
                    EventChoiceMeta::disabled("[Skill] No Skills.", "No Skills"),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![],
                        constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            }
            if has_power {
                let effect = card_remove_effect(run_state, power_idx(s));
                choices.push(EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Power] Remove {}.",
                        card_name(run_state, power_idx(s))
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![effect],
                        constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            } else {
                choices.push(EventOption::new(
                    EventChoiceMeta::disabled("[Power] No Powers.", "No Powers"),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![],
                        constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            }
            if has_attack {
                let effect = card_remove_effect(run_state, attack_idx(s));
                choices.push(EventOption::new(
                    EventChoiceMeta::new(format!(
                        "[Attack] Remove {}.",
                        card_name(run_state, attack_idx(s))
                    )),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![effect],
                        constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            } else {
                choices.push(EventOption::new(
                    EventChoiceMeta::disabled("[Attack] No Attacks.", "No Attacks"),
                    EventOptionSemantics {
                        action: EventActionKind::DeckOperation,
                        effects: vec![],
                        constraints: vec![EventOptionConstraint::RequiresRemovableCard],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: false,
                    },
                ));
            }
            choices
        }
        _ => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                effects: vec![],
                constraints: vec![],
                transition: EventOptionTransition::Complete,
                repeatable: false,
                terminal: true,
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

fn card_name(run_state: &RunState, deck_idx: usize) -> String {
    run_state
        .master_deck
        .get(deck_idx)
        .map(|card| {
            let name = crate::content::cards::get_card_definition(card.id).name;
            match card.upgrades {
                0 => name.to_string(),
                1 => format!("{name}+"),
                n => format!("{name}+{n}"),
            }
        })
        .unwrap_or_else(|| "unknown card".to_string())
}

fn card_remove_effect(run_state: &RunState, deck_idx: usize) -> EventEffect {
    match run_state.master_deck.get(deck_idx) {
        Some(card) => EventEffect::RemoveCard {
            count: 1,
            target_lineage: Some(card.lineage),
            kind: EventCardKind::Specific(card.id),
        },
        None => EventEffect::RemoveCard {
            count: 1,
            target_lineage: None,
            kind: EventCardKind::Unknown,
        },
    }
}

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            event_state.current_screen = 1;
        }
        1 => {
            let s = event_state.internal_state;
            let has_skill = (s & 0x3FF) != NO_CARD;
            let has_power = ((s >> 10) & 0x3FF) != NO_CARD;
            let has_attack = ((s >> 20) & 0x3FF) != NO_CARD;
            if !has_skill && !has_power && !has_attack {
                event_state.current_screen = 2;
            } else {
                let card_idx = match choice_idx {
                    0 if has_skill => Some(skill_idx(s)),
                    1 if has_power => Some(power_idx(s)),
                    2 if has_attack => Some(attack_idx(s)),
                    _ => None,
                };
                if let Some(card_idx) = card_idx {
                    if card_idx < run_state.master_deck.len() {
                        let lineage = run_state.master_deck[card_idx].lineage;
                        run_state.remove_card_from_deck_with_source(
                            lineage,
                            DomainEventSource::Event(crate::state::events::EventId::Falling),
                        );
                    }
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

/// Initialize Falling state: pre-select cards using miscRng (matches Java constructor).
/// Java calls CardHelper.returnCardOfType(type, miscRng) for each present type,
/// which uses miscRng.random(cards.size() - 1)
pub fn init_falling_state(run_state: &mut RunState) -> i32 {
    use crate::content::cards::CardType;

    let mut s_idx = NO_CARD;
    let mut p_idx = NO_CARD;
    let mut a_idx = NO_CARD;
    // Falling.setCards samples Attack, Skill, Power, independently of the
    // dialog's Skill, Power, Attack button order. CardHelper preserves deck
    // order while excluding bottled cards, then calls random(size - 1).
    for (card_type, selected) in [
        (CardType::Attack, &mut a_idx),
        (CardType::Skill, &mut s_idx),
        (CardType::Power, &mut p_idx),
    ] {
        let candidates: Vec<usize> = run_state
            .master_deck
            .iter()
            .enumerate()
            .filter(|(_, card)| {
                !crate::state::core::master_deck_card_is_bottled(card, &run_state.relics)
                    && crate::content::cards::get_card_definition(card.id).card_type == card_type
            })
            .map(|(index, _)| index)
            .collect();
        if !candidates.is_empty() {
            let pick = run_state
                .rng_pool
                .misc_rng
                .random((candidates.len() - 1) as i32) as usize;
            *selected = candidates[pick] as i32;
        }
    }

    encode_indices(s_idx, p_idx, a_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::state::selection::DomainEvent;

    #[test]
    fn falling_samples_attack_skill_power_with_single_bound_calls_in_deck_order() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRng, StsRngCall};
        let mut run = RunState::new(1, 20, true, "Ironclad");
        run.master_deck = [
            CardId::Strike,
            CardId::Defend,
            CardId::Inflame,
            CardId::Bash,
            CardId::TrueGrit,
            CardId::ShrugItOff,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| CombatCard::for_scenario(id, index as u32 + 100))
        .collect();
        run.rng_pool.misc_rng = StsRng::new(29);
        let trace = begin_sts_rng_trace().unwrap();
        let selected = init_falling_state(&mut run);
        let calls = trace.finish().unwrap();
        assert_eq!(calls.len(), 3);
        for (index, range, candidates, actual) in [
            (0, 1, vec![0, 3], attack_idx(selected)),
            (1, 2, vec![1, 4, 5], skill_idx(selected)),
            (2, 0, vec![2], power_idx(selected)),
        ] {
            let StsRngCall::RandomInt {
                range: actual_range,
                result,
            } = calls[index].call
            else {
                panic!("CardHelper.returnCardOfType uses random(size - 1)");
            };
            assert_eq!(actual_range, range);
            assert_eq!(actual, candidates[result as usize]);
        }
    }

    #[test]
    fn falling_skill_option_exposes_remove_operation() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.add_card_to_deck(CardId::ShrugItOff);
        let skill_index = rs.master_deck.len() - 1;
        rs.upgrade_card(rs.master_deck[skill_index].lineage);
        let state = EventState {
            id: crate::state::events::EventId::Falling,
            current_screen: 1,
            internal_state: encode_indices(skill_index as i32, NO_CARD, NO_CARD),
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        };
        let options = get_options(&rs, &state);
        assert!(
            options[0].ui.text.contains("Shrug It Off+"),
            "Falling should display the preselected specific Skill card"
        );
        assert!(matches!(
            options[0].semantics.effects.as_slice(),
            [EventEffect::RemoveCard {
                count: 1,
                target_lineage: Some(_),
                kind: EventCardKind::Specific(CardId::ShrugItOff),
            }]
        ));
    }

    #[test]
    fn falling_init_ignores_bottled_cards_like_java_card_helper() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![
            CombatCard::for_scenario(CardId::Defend, 11),
            CombatCard::for_scenario(CardId::Inflame, 12),
            CombatCard::for_scenario(CardId::Strike, 13),
        ];
        let mut bottled_skill = RelicState::new(RelicId::BottledLightning);
        bottled_skill.amount = crate::test_support::scenario_lineage(11).to_runtime_i32_payload();
        rs.relics.push(bottled_skill);

        let state = init_falling_state(&mut rs);

        assert_eq!(skill_idx(state), NO_CARD as usize);
        assert_eq!(power_idx(state), 1);
        assert_eq!(attack_idx(state), 2);
    }

    #[test]
    fn falling_removal_uses_event_domain_source() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![CombatCard::for_scenario(CardId::Strike, 11)];
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::Falling,
            current_screen: 1,
            internal_state: encode_indices(NO_CARD, NO_CARD, 0),
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });

        let mut engine_state = EngineState::EventRoom;
        handle_choice(&mut engine_state, &mut rs, 2);

        assert!(rs.master_deck.is_empty());
        assert!(rs.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(crate::state::events::EventId::Falling),
            } if card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(11)
        )));
    }

    #[test]
    fn disabled_missing_type_choice_does_not_advance_or_remove_card() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![CombatCard::for_scenario(CardId::Inflame, 12)];
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::Falling,
            current_screen: 1,
            internal_state: encode_indices(NO_CARD, 0, NO_CARD),
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        assert!(options[0].ui.disabled);

        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.master_deck.len(), 1);
        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 1);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(rs.take_emitted_events().is_empty());
    }

    #[test]
    fn land_safely_without_any_candidates_advances_like_java() {
        let mut rs = RunState::new(1, 0, true, "Ironclad");
        rs.master_deck = vec![];
        rs.event_state = Some(EventState {
            id: crate::state::events::EventId::Falling,
            current_screen: 1,
            internal_state: encode_indices(NO_CARD, NO_CARD, NO_CARD),
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: Vec::new(),
            extra_data: Vec::new(),
        });
        rs.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        let options = get_options(&rs, rs.event_state.as_ref().unwrap());
        assert_eq!(options.len(), 1);
        assert!(!options[0].ui.disabled);

        handle_choice(&mut engine_state, &mut rs, 0);

        assert_eq!(rs.event_state.as_ref().unwrap().current_screen, 2);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(rs.take_emitted_events().is_empty());
    }
}
