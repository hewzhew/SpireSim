use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventSelectionKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

const DAMAGE: i32 = 7;
const MIN_GOLD: i32 = 50;
const MAX_GOLD: i32 = 80;
const REQUIRED_DAMAGE: i32 = 10;

fn has_high_damage_card(run_state: &RunState) -> bool {
    run_state.master_deck.iter().any(|c| {
        let def = crate::content::cards::get_card_definition(c.id);
        def.card_type == crate::content::cards::CardType::Attack
            && c.base_damage_override
                .unwrap_or(def.base_damage + i32::from(c.upgrades) * def.upgrade_damage)
                >= REQUIRED_DAMAGE
    })
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    if event_state.current_screen == 1 {
        return vec![EventOption::new(
            EventChoiceMeta::new("[Proceed]"),
            EventOptionSemantics {
                action: EventActionKind::DeckOperation,
                effects: vec![EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: None,
                    kind: EventCardKind::Unknown,
                }],
                transition: EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard),
                repeatable: false,
                terminal: false,
                ..Default::default()
            },
        )];
    }
    if event_state.current_screen >= 2 {
        return vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                terminal: true,
                ..Default::default()
            },
        )];
    }

    let can_attack = has_high_damage_card(run_state);
    let mut choices = vec![EventOption::new(
        EventChoiceMeta::new(format!(
            "[Remove a card] Take {} damage. Remove a card from your deck.",
            DAMAGE
        )),
        EventOptionSemantics {
            action: EventActionKind::DeckOperation,
            effects: vec![
                EventEffect::LoseHp(DAMAGE),
                EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: None,
                    kind: EventCardKind::Unknown,
                },
            ],
            transition: EventOptionTransition::AdvanceScreen,
            repeatable: false,
            terminal: false,
            ..Default::default()
        },
    )];

    if can_attack {
        choices.push(EventOption::new(
            EventChoiceMeta::new(format!("[Attack] Gain {}-{} Gold.", MIN_GOLD, MAX_GOLD)),
            EventOptionSemantics {
                action: EventActionKind::Gain,
                effects: vec![EventEffect::GainGoldRange {
                    min: MIN_GOLD,
                    max: MAX_GOLD,
                }],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
                ..Default::default()
            },
        ));
    } else {
        choices.push(EventOption::new(
            EventChoiceMeta::disabled(
                format!(
                    "[Attack] Requires an Attack card with ≥{} damage.",
                    REQUIRED_DAMAGE
                ),
                "No qualifying attack card.",
            ),
            EventOptionSemantics {
                action: EventActionKind::Gain,
                effects: vec![EventEffect::GainGoldRange {
                    min: MIN_GOLD,
                    max: MAX_GOLD,
                }],
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
                ..Default::default()
            },
        ));
    }

    choices.push(EventOption::new(
        EventChoiceMeta::new("[Leave]"),
        EventOptionSemantics {
            action: EventActionKind::Leave,
            transition: EventOptionTransition::AdvanceScreen,
            repeatable: false,
            terminal: false,
            ..Default::default()
        },
    ));
    choices
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
            match choice_idx {
                0 => {
                    // Java first applies damage and moves to the PURGE screen.
                    // A second button press opens the grid select.
                    super::apply_player_default_damage(
                        run_state,
                        DAMAGE,
                        super::EventDamageOwner::Player,
                        DomainEventSource::Event(EventId::GoldenWing),
                    );
                    event_state.current_screen = 1;
                }
                1 => {
                    // Attack: gain gold
                    if has_high_damage_card(run_state) {
                        let gold = run_state.rng_pool.misc_rng.random_range(MIN_GOLD, MAX_GOLD);
                        run_state.change_gold_with_source(
                            gold,
                            DomainEventSource::Event(EventId::GoldenWing),
                        );
                        event_state.current_screen = 2;
                    }
                }
                _ => {
                    // Leave
                    event_state.current_screen = 2;
                }
            }
        }
        1 => {
            event_state.current_screen = 2;
            run_state.event_state = Some(event_state);
            *engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                reason: RunPendingChoiceReason::PurgeNonBottled,
                source: DomainEventSource::Event(EventId::GoldenWing),
                min_choices: 1,
                max_choices: 1,
                return_state: Box::new(EngineState::EventRoom),
            });
            return;
        }
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

#[cfg(test)]
mod tests {
    use super::handle_choice;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::run_loop::tick_run;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ClientInput, EngineState, RunPendingChoiceReason};
    use crate::state::events::{
        EventActionKind, EventEffect, EventOptionTransition, EventSelectionKind,
    };
    use crate::state::events::{EventId, EventState};
    use crate::state::run::RunState;
    use crate::state::selection::{
        DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
        SelectionTargetRef,
    };

    fn golden_wing_run() -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state.event_state = Some(EventState::new(EventId::GoldenWing));
        run_state.emitted_events.clear();
        run_state
    }

    fn deck_card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    #[test]
    fn remove_path_damage_uses_event_source_before_purge_selection() {
        let mut run_state = golden_wing_run();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 13);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -7,
                current_hp: 13,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::GoldenWing),
            }
        )));
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert_eq!(
            run_state.event_state.as_ref().unwrap().current_screen,
            1,
            "Java GoldenWing enters PURGE first; the next button press opens the deck picker"
        );
    }

    #[test]
    fn remove_path_damage_respects_tungsten_rod_like_java_player_damage() {
        let mut run_state = golden_wing_run();
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 14);
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -6,
                current_hp: 14,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::GoldenWing),
            }
        )));
    }

    #[test]
    fn remove_path_selection_excludes_bottled_and_unpurgeable_cards_like_java() {
        let mut run_state = golden_wing_run();
        run_state.master_deck = vec![
            deck_card(CardId::Strike, 101),
            deck_card(CardId::Defend, 102),
            deck_card(CardId::AscendersBane, 103),
        ];
        let mut bottle = RelicState::new(RelicId::BottledFlame);
        bottle.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        let EngineState::RunPendingChoice(choice) = engine_state else {
            panic!("Golden Wing remove path should open deck purge selection");
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
    fn remove_path_removes_selected_card_with_event_source() {
        let mut run_state = golden_wing_run();
        run_state.master_deck = vec![deck_card(CardId::Strike, 101)];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
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
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 2);
        assert!(run_state.master_deck.is_empty());
        assert!(run_state.take_emitted_events().iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Event(EventId::GoldenWing),
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )));
    }

    #[test]
    fn attack_option_uses_upgraded_master_deck_base_damage_like_java() {
        let mut run_state = golden_wing_run();
        run_state.master_deck.clear();
        let mut pommel = CombatCard::for_scenario(CardId::PommelStrike, 99);
        pommel.upgrades = 1;
        run_state.master_deck.push(pommel);

        let choices = super::get_choices(&run_state, run_state.event_state.as_ref().unwrap());

        assert!(
            !choices[1].disabled,
            "Java CardHelper.hasCardWithXDamage checks the card instance baseDamage after upgrade"
        );
    }

    #[test]
    fn attack_option_does_not_count_non_attack_base_damage() {
        let mut run_state = golden_wing_run();
        run_state.master_deck.clear();
        let mut defend = CombatCard::for_scenario(CardId::Defend, 100);
        defend.base_damage_override = Some(super::REQUIRED_DAMAGE);
        run_state.master_deck.push(defend);

        let choices = super::get_choices(&run_state, run_state.event_state.as_ref().unwrap());

        assert!(choices[1].disabled);
    }

    #[test]
    fn options_expose_structured_remove_and_random_gold_semantics() {
        let mut run_state = golden_wing_run();
        let mut pommel = CombatCard::for_scenario(CardId::PommelStrike, 99);
        pommel.upgrades = 1;
        run_state.master_deck.push(pommel);
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Golden Wing should expose structured event semantics");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].semantics.action, EventActionKind::DeckOperation);
        assert_eq!(
            options[0].semantics.effects,
            vec![
                EventEffect::LoseHp(super::DAMAGE),
                EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: None,
                    kind: crate::state::events::EventCardKind::Unknown,
                },
            ]
        );
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Gain);
        assert_eq!(
            options[1].semantics.effects,
            vec![EventEffect::GainGoldRange {
                min: super::MIN_GOLD,
                max: super::MAX_GOLD,
            }]
        );
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut purge_screen = event_state.clone();
        purge_screen.current_screen = 1;
        let purge_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &purge_screen,
            )
            .expect("Golden Wing purge screen should expose the pending selection boundary");
        assert_eq!(
            purge_options[0].semantics.transition,
            EventOptionTransition::OpenSelection(EventSelectionKind::RemoveCard)
        );
    }

    #[test]
    fn disabled_attack_option_does_not_advance_or_grant_gold() {
        let mut run_state = golden_wing_run();
        run_state.master_deck.clear();
        let starting_gold = run_state.gold;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.gold, starting_gold);
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
        assert!(matches!(engine_state, EngineState::EventRoom));
        assert!(run_state.take_emitted_events().is_empty());
    }
}
