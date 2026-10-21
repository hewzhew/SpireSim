use crate::state::core::{EngineState, RunResult};
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventState,
};
use crate::state::map::RoomType;
use crate::state::run::RunState;

/// ProceedButton.goToVictoryRoomOrTheDoor uses an ordinary room transition,
/// followed by VictoryRoom.onPlayerEntry. It does not construct TheEnding yet.
pub(crate) fn enter_victory_room(run: &mut RunState) {
    run.complete_current_room_encounter(Some(RoomType::MonsterRoomBoss));
    run.advance_floor_and_generate_seeds();
    // Shared map identity represents Java's synthetic (-1, 15) node as (0, 15).
    run.map.current_x = 0;
    run.map.current_y = 15;
    run.room_mugged = false;
    run.room_smoked = false;
    run.on_enter_room();
    run.event_state = Some(EventState::new(EventId::SpireHeart));
}

pub fn get_options(_run: &RunState, event: &EventState) -> Vec<EventOption> {
    let text = match event.current_screen {
        0 => "[Continue]",
        1 => "[Attack]",
        2 => "[Continue]",
        3 => "[Sleep]",
        4 => "[Continue] Open the door",
        _ => return Vec::new(),
    };
    vec![EventOption::new(
        EventChoiceMeta::new(text),
        EventOptionSemantics {
            action: EventActionKind::Continue,
            transition: if event.current_screen >= 3 {
                EventOptionTransition::Complete
            } else {
                EventOptionTransition::AdvanceScreen
            },
            terminal: event.current_screen >= 3,
            ..Default::default()
        },
    )]
}

pub fn handle_choice(engine: &mut EngineState, run: &mut RunState, choice: usize) {
    if choice != 0 {
        return;
    }
    let Some(event) = run.event_state.as_mut() else {
        return;
    };
    // Keep original CUR_SCREEN ordinals. Heart-striking effects use only the
    // visual MathUtils RNG and change no deterministic run resources.
    match event.current_screen {
        0 | 1 => event.current_screen += 1,
        2 => {
            event.current_screen = if run.is_final_act_available && run.keys.iter().all(|key| *key)
            {
                4
            } else {
                3
            };
        }
        3 => {
            run.event_state = None;
            *engine = EngineState::GameOver(RunResult::Victory);
        }
        4 => {
            // DoorUnlockScreen is automatic settlement after this one choice.
            // Its TheEnding construction heals and initializes the new act.
            run.event_state = None;
            run.enter_final_act();
            *engine = EngineState::MapNavigation;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::rng::StsRng;

    #[test]
    fn spire_heart_preserves_each_page_and_heals_only_after_the_door_choice() {
        let mut run = RunState::new(1, 20, true, "Ironclad");
        run.act_num = 3;
        run.floor_num = 51;
        run.current_hp = 40;
        run.keys = [true; 3];
        run.relics.push(RelicState::new(RelicId::MawBank));
        let gold = run.gold;
        enter_victory_room(&mut run);
        assert_eq!(run.floor_num, 52);
        assert_eq!(run.rng_pool.shuffle_rng, StsRng::new(53));
        assert_eq!(run.gold, gold + 12);
        let rng = run.rng_pool.clone();
        let mut engine = EngineState::EventRoom;
        for screen in [0, 1, 2, 4] {
            assert_eq!(run.event_state.as_ref().unwrap().current_screen, screen);
            assert_eq!(
                get_options(&run, run.event_state.as_ref().unwrap()).len(),
                1
            );
            assert_eq!(engine, EngineState::EventRoom);
            assert_eq!(run.act_num, 3);
            assert_eq!(run.current_hp, 40);
            assert_eq!(run.rng_pool, rng);
            handle_choice(&mut engine, &mut run, 0);
        }
        assert_eq!(engine, EngineState::MapNavigation);
        assert_eq!(run.act_num, 4);
        assert_eq!(run.floor_num, 52);
        assert!(run.event_state.is_none());
        assert!(run.current_hp > 40);
        assert_eq!(run.boss_key, Some(EncounterId::TheHeart));
        assert_eq!(run.elite_monster_list, vec![EncounterId::ShieldAndSpear; 3]);
    }
}
