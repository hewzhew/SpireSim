use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionConstraint,
    EventOptionSemantics, EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn get_damage(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        (run_state.max_hp as f32 * 0.3).round() as i32
    } else {
        (run_state.max_hp as f32 * 0.2).round() as i32
    }
}

fn has_upgradable_cards(run_state: &RunState) -> bool {
    run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade)
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    if event_state.current_screen == 1 {
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

    let damage = get_damage(run_state);
    let mut choices = Vec::new();

    if has_upgradable_cards(run_state) {
        choices.push(EventOption::new(
            EventChoiceMeta::new(format!(
                "[Enter the Light] Take {} damage. Upgrade 2 random cards.",
                damage
            )),
            EventOptionSemantics {
                action: EventActionKind::Trade,
                effects: vec![
                    EventEffect::LoseHp(damage),
                    EventEffect::UpgradeCard { count: 2 },
                ],
                constraints: vec![EventOptionConstraint::RequiresUpgradeableCard],
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        ));
    } else {
        choices.push(EventOption::new(
            EventChoiceMeta::disabled(
                "[Enter the Light] No upgradable cards.",
                "No upgradable cards in your deck.",
            ),
            EventOptionSemantics {
                action: EventActionKind::Trade,
                effects: vec![
                    EventEffect::LoseHp(damage),
                    EventEffect::UpgradeCard { count: 2 },
                ],
                constraints: vec![EventOptionConstraint::RequiresUpgradeableCard],
                transition: EventOptionTransition::AdvanceScreen,
                ..Default::default()
            },
        ));
    }

    choices.push(EventOption::new(
        EventChoiceMeta::new("[Leave]"),
        EventOptionSemantics {
            action: EventActionKind::Leave,
            transition: EventOptionTransition::AdvanceScreen,
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

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            let mut advance_screen = false;
            match choice_idx {
                0 => {
                    // Enter Light: take damage + upgrade 2 random cards
                    if has_upgradable_cards(run_state) {
                        let damage = get_damage(run_state);
                        super::apply_player_default_damage(
                            run_state,
                            damage,
                            super::EventDamageOwner::Player,
                            DomainEventSource::Event(EventId::ShiningLight),
                        );
                        run_state.upgrade_random_cards_with_source(
                            2,
                            DomainEventSource::Event(EventId::ShiningLight),
                        );
                        advance_screen = true;
                    }
                }
                _ => {
                    // Leave
                    advance_screen = true;
                }
            }
            if advance_screen {
                event_state.current_screen = 1;
            }
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
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::EngineState;
    use crate::state::events::{
        EventActionKind, EventEffect, EventId, EventOptionConstraint, EventOptionTransition,
        EventState,
    };
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    fn shining_run(current_hp: i32, max_hp: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = current_hp;
        run_state.max_hp = max_hp;
        run_state.event_state = Some(EventState::new(EventId::ShiningLight));
        run_state.emitted_events.clear();
        run_state
    }

    #[test]
    fn options_expose_structured_damage_random_upgrade_and_leave_semantics() {
        let run_state = shining_run(80, 80);
        let event_state = run_state.event_state.as_ref().unwrap();

        let options = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &run_state,
            event_state,
        )
        .expect("Shining Light should expose structured event semantics");

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].semantics.action, EventActionKind::Trade);
        assert_eq!(
            options[0].semantics.effects,
            vec![
                EventEffect::LoseHp(16),
                EventEffect::UpgradeCard { count: 2 }
            ]
        );
        assert!(options[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresUpgradeableCard));
        assert_eq!(
            options[0].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );
        assert_eq!(options[1].semantics.action, EventActionKind::Leave);
        assert_eq!(
            options[1].semantics.transition,
            EventOptionTransition::AdvanceScreen
        );

        let mut no_upgrade = shining_run(80, 80);
        no_upgrade.master_deck = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Injury,
            11,
        )];
        let disabled = crate::engine::event_handler::try_get_structured_event_options_for_state(
            &no_upgrade,
            no_upgrade.event_state.as_ref().unwrap(),
        )
        .expect("Shining Light disabled option should still expose semantics");
        assert!(disabled[0].ui.disabled);
        assert!(disabled[0]
            .semantics
            .constraints
            .contains(&EventOptionConstraint::RequiresUpgradeableCard));

        let mut result_screen = EventState::new(EventId::ShiningLight);
        result_screen.current_screen = 1;
        let leave_options =
            crate::engine::event_handler::try_get_structured_event_options_for_state(
                &run_state,
                &result_screen,
            )
            .expect("Shining Light result screen should expose leave semantics");
        assert_eq!(leave_options.len(), 1);
        assert_eq!(leave_options[0].semantics.action, EventActionKind::Leave);
        assert_eq!(
            leave_options[0].semantics.transition,
            EventOptionTransition::Complete
        );
    }

    #[test]
    fn enter_light_damage_and_random_upgrades_use_event_source() {
        let mut run_state = shining_run(80, 80);
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 64);
        assert_eq!(
            run_state
                .master_deck
                .iter()
                .filter(|card| card.upgrades > 0)
                .count(),
            2
        );
        let events = run_state.take_emitted_events();
        assert!(events.iter().any(|event| matches!(
            event,
            DomainEvent::HpChanged {
                delta: -16,
                current_hp: 64,
                max_hp: 80,
                source: DomainEventSource::Event(EventId::ShiningLight),
            }
        )));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::CardUpgraded {
                        source: DomainEventSource::Event(EventId::ShiningLight),
                        ..
                    }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn enter_light_normal_damage_applies_torii_then_tungsten() {
        let mut run_state = shining_run(20, 20);
        run_state.relics.push(RelicState::new(RelicId::Torii));
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 20);
        assert!(!run_state
            .take_emitted_events()
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
    }

    #[test]
    fn leave_does_not_damage_or_upgrade() {
        let mut run_state = shining_run(80, 80);
        let before = run_state.master_deck.clone();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(run_state.current_hp, 80);
        assert_eq!(
            run_state
                .master_deck
                .iter()
                .map(|card| (card.id, card.upgrades))
                .collect::<Vec<_>>(),
            before
                .iter()
                .map(|card| (card.id, card.upgrades))
                .collect::<Vec<_>>()
        );
        assert!(run_state.take_emitted_events().is_empty());
    }

    #[test]
    fn disabled_enter_light_does_not_apply_damage_when_no_cards_can_upgrade() {
        let mut run_state = shining_run(30, 30);
        run_state.master_deck = vec![
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Injury,
                11,
            ),
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::AscendersBane,
                12,
            ),
        ];
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.current_hp, 30);
        assert_eq!(
            run_state.event_state.as_ref().unwrap().current_screen,
            0,
            "disabled Java option should not advance the event state"
        );
        assert!(run_state.take_emitted_events().is_empty());
    }
}
