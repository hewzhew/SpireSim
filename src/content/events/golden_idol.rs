// Java: GoldenIdolEvent (exordium)
// Screen 0: [Take] → obtain Golden Idol relic + enter trap screen | [Leave]
// Screen 1 (trap): [Run] Obtain Injury curse | [Fight] Take damage | [Lose Max HP]
// Screen 2: [Leave]
//
// Java constructor calculates damage and maxHpLoss based on A15:
//   damage = (int)(maxHP * 0.25f) or 0.35f at A15+
//   maxHpLoss = (int)(maxHP * 0.08f) or 0.10f at A15+, min 1

use crate::content::cards::CardId;
use crate::content::relics::RelicId;
use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventCardKind, EventChoiceMeta, EventEffect, EventId, EventOption,
    EventOptionSemantics, EventOptionTransition, EventRelicKind, EventState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

fn calc_damage(run_state: &RunState) -> i32 {
    if run_state.ascension_level >= 15 {
        (run_state.max_hp as f32 * 0.35) as i32
    } else {
        (run_state.max_hp as f32 * 0.25) as i32
    }
}

fn calc_max_hp_loss(run_state: &RunState) -> i32 {
    let loss = if run_state.ascension_level >= 15 {
        (run_state.max_hp as f32 * 0.10) as i32
    } else {
        (run_state.max_hp as f32 * 0.08) as i32
    };
    loss.max(1)
}

pub fn get_options(run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![
            EventOption::new(
                EventChoiceMeta::new("[Take] Obtain Golden Idol."),
                EventOptionSemantics {
                    action: EventActionKind::Gain,
                    effects: vec![EventEffect::ObtainRelic {
                        count: 1,
                        kind: EventRelicKind::Specific(RelicId::GoldenIdol),
                    }],
                    constraints: vec![],
                    transition: EventOptionTransition::AdvanceScreen,
                    repeatable: false,
                    terminal: false,
                },
            ),
            EventOption::new(
                EventChoiceMeta::new("[Leave]"),
                EventOptionSemantics {
                    action: EventActionKind::Leave,
                    effects: vec![],
                    constraints: vec![],
                    transition: EventOptionTransition::Complete,
                    repeatable: false,
                    terminal: true,
                },
            ),
        ],
        1 => {
            let damage = calc_damage(run_state);
            let max_hp_loss = calc_max_hp_loss(run_state);
            vec![
                EventOption::new(
                    EventChoiceMeta::new("[Run] Obtain Injury curse."),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        effects: vec![EventEffect::ObtainCurse {
                            count: 1,
                            kind: EventCardKind::Specific(CardId::Injury),
                        }],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: true,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[Fight] Take {} damage.", damage)),
                    EventOptionSemantics {
                        action: EventActionKind::Fight,
                        effects: vec![EventEffect::LoseHp(damage)],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: true,
                    },
                ),
                EventOption::new(
                    EventChoiceMeta::new(format!("[Lose Max HP] Lose {} Max HP.", max_hp_loss)),
                    EventOptionSemantics {
                        action: EventActionKind::Decline,
                        effects: vec![EventEffect::LoseMaxHp(max_hp_loss)],
                        constraints: vec![],
                        transition: EventOptionTransition::AdvanceScreen,
                        repeatable: false,
                        terminal: true,
                    },
                ),
            ]
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

pub fn handle_choice(_engine_state: &mut EngineState, run_state: &mut RunState, choice_idx: usize) {
    let mut event_state = run_state.event_state.take().unwrap();

    match event_state.current_screen {
        0 => {
            match choice_idx {
                0 => {
                    // Take: obtain Golden Idol, advance to trap screen
                    let relic_id = if run_state.relics.iter().any(|r| r.id == RelicId::GoldenIdol) {
                        RelicId::Circlet
                    } else {
                        RelicId::GoldenIdol
                    };
                    if let Some(next_state) = run_state.obtain_relic_with_source(
                        relic_id,
                        EngineState::EventRoom,
                        DomainEventSource::Event(EventId::GoldenIdol),
                    ) {
                        *_engine_state = next_state;
                    }
                    event_state.current_screen = 1;
                }
                _ => {
                    // Leave — don't take the idol
                    event_state.completed = true;
                }
            }
        }
        1 => {
            // Trap punishment
            match choice_idx {
                0 => {
                    // Run: obtain Injury curse
                    super::obtain_event_card(run_state, EventId::GoldenIdol, CardId::Injury);
                }
                1 => {
                    // Java uses DamageInfo(null, damage): normal ownerless damage.
                    super::apply_player_default_damage(
                        run_state,
                        calc_damage(run_state),
                        super::EventDamageOwner::None,
                        DomainEventSource::Event(EventId::GoldenIdol),
                    );
                }
                _ => {
                    // Lose Max HP
                    let max_hp_loss = calc_max_hp_loss(run_state);
                    run_state.lose_max_hp_with_source(
                        max_hp_loss,
                        DomainEventSource::Event(EventId::GoldenIdol),
                    );
                }
            }
            event_state.current_screen = 2;
        }
        _ => {
            event_state.completed = true;
        }
    }

    run_state.event_state = Some(event_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;
    use crate::state::selection::DomainEvent;

    #[test]
    fn trap_damage_uses_java_ownerless_default_damage_hooks() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.max_hp = 20;
        run_state.current_hp = 20;
        run_state.event_state = Some(EventState::new(EventId::GoldenIdol));
        run_state.event_state.as_mut().unwrap().current_screen = 1;
        run_state.relics.push(RelicState::new(RelicId::Torii));
        run_state.relics.push(RelicState::new(RelicId::TungstenRod));
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 1);

        assert_eq!(
            run_state.current_hp, 16,
            "Java DamageInfo(null, 5) skips Torii but Tungsten reduces HP loss by 1"
        );
        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 2);
    }

    #[test]
    fn run_trap_runs_obtain_hooks_before_injury_obtained_event() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.max_hp = 80;
        run_state.current_hp = 80;
        run_state.relics.push(RelicState::new(RelicId::CeramicFish));
        run_state.event_state = Some(EventState::new(EventId::GoldenIdol));
        run_state.event_state.as_mut().unwrap().current_screen = 1;
        run_state.emitted_events.clear();
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        let events = run_state.take_emitted_events();
        let fish_gold_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::GoldChanged {
                        delta: 9,
                        source: DomainEventSource::Event(EventId::GoldenIdol),
                        ..
                    }
                )
            })
            .expect("Ceramic Fish should run when Golden Idol's Injury is obtained");
        let obtained_pos = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::CardObtained {
                        card,
                        source: DomainEventSource::Event(EventId::GoldenIdol),
                    } if card.id == CardId::Injury
                )
            })
            .expect("Golden Idol should obtain Injury through ShowCardAndObtainEffect");

        assert!(
            fish_gold_pos < obtained_pos,
            "Java GoldenIdol trap queues ShowCardAndObtainEffect for Injury; that effect runs onObtainCard before Soul.obtain"
        );
    }
}
