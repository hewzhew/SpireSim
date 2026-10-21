use crate::state::core::EngineState;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventOption, EventOptionSemantics,
    EventOptionTransition, EventState,
};
use crate::state::run::RunState;

pub fn get_options(_run_state: &RunState, event_state: &EventState) -> Vec<EventOption> {
    match event_state.current_screen {
        0 => vec![
            EventOption::new(
                EventChoiceMeta::new("[Enter] Step into the portal."),
                EventOptionSemantics {
                    action: EventActionKind::Accept,
                    transition: EventOptionTransition::AdvanceScreen,
                    repeatable: false,
                    terminal: false,
                    ..Default::default()
                },
            ),
            EventOption::new(
                EventChoiceMeta::new("[Leave]"),
                EventOptionSemantics {
                    action: EventActionKind::Decline,
                    transition: EventOptionTransition::AdvanceScreen,
                    repeatable: false,
                    terminal: false,
                    ..Default::default()
                },
            ),
        ],
        1 => vec![EventOption::new(
            EventChoiceMeta::new("[Continue]"),
            EventOptionSemantics {
                action: EventActionKind::Special,
                effects: vec![EventEffect::StartCombat],
                transition: EventOptionTransition::StartCombat,
                repeatable: false,
                terminal: true,
                ..Default::default()
            },
        )],
        _ => vec![EventOption::new(
            EventChoiceMeta::new("[Leave]"),
            EventOptionSemantics {
                action: EventActionKind::Leave,
                transition: EventOptionTransition::Complete,
                repeatable: false,
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
            event_state.current_screen = if choice_idx == 0 { 1 } else { 2 };
            run_state.event_state = Some(event_state);
        }
        1 => {
            // Java creates a synthetic boss MapRoomNode at y=15 and runs the
            // normal next-room transition before MonsterRoomBoss.onPlayerEntry().
            run_state.map.current_y = 15;
            run_state.map.current_x = -1;
            run_state.floor_num += 1;
            run_state.room_mugged = false;
            run_state.room_smoked = false;
            apply_secret_portal_boss_room_entry_relics(run_state);
            *engine_state = EngineState::CombatPlayerTurn;
        }
        _ => {
            event_state.completed = true;
            run_state.event_state = Some(event_state);
        }
    }
}

fn apply_secret_portal_boss_room_entry_relics(run_state: &mut RunState) {
    // AbstractDungeon.nextRoomTransition() calls relic.onEnterRoom(nextRoom.room)
    // before MonsterRoomBoss.onPlayerEntry(). For a synthetic boss room, the
    // only modeled all-room mechanical hook here is Maw Bank; shop/rest/event
    // room-specific hooks do not apply.
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == crate::content::relics::RelicId::MawBank && !relic.used_up)
    {
        run_state.change_gold_with_source(
            12,
            crate::state::selection::DomainEventSource::Relic(
                crate::content::relics::RelicId::MawBank,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::events::EventId;

    #[test]
    fn accepting_secret_portal_moves_to_boss_combat_boundary() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.act_num = 3;
        run_state.floor_num = 40;
        run_state.event_state = Some(EventState::new(EventId::SecretPortal));
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 1);
        assert!(matches!(engine_state, EngineState::EventRoom));

        handle_choice(&mut engine_state, &mut run_state, 0);

        assert!(matches!(engine_state, EngineState::CombatPlayerTurn));
        assert_eq!(
            run_state.map.current_x, -1,
            "Java SecretPortal constructs MapRoomNode(-1, 15); pathX.add(1) is only path history"
        );
        assert_eq!(run_state.map.current_y, 15);
        assert_eq!(run_state.floor_num, 41);
        assert_eq!(
            run_state.map.get_current_room_type(),
            Some(crate::state::map::node::RoomType::MonsterRoomBoss)
        );
        assert!(run_state.event_state.is_none());
    }

    #[test]
    fn secret_portal_boss_transition_applies_all_room_entry_relics() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.act_num = 3;
        run_state.event_state = Some(EventState::new(EventId::SecretPortal));
        run_state
            .relics
            .push(crate::content::relics::RelicState::new(
                crate::content::relics::RelicId::MawBank,
            ));
        let starting_gold = run_state.gold;
        let mut engine_state = EngineState::EventRoom;

        handle_choice(&mut engine_state, &mut run_state, 0);
        handle_choice(&mut engine_state, &mut run_state, 0);

        assert_eq!(run_state.gold, starting_gold + 12);
    }
}
