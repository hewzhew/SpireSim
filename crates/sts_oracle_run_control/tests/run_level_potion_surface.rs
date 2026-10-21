use sts_oracle_run_control::ai::planner_core::PlannerAction;
use sts_oracle_run_control::content::potions::{Potion, PotionId};
use sts_oracle_run_control::eval::run_control::{
    build_decision_surface, capture_planner_boundary_yield_v1, PlannerBoundaryYieldKindV1,
    RunControlSession, RunControlSessionCheckpointV1, RunDecisionAction,
};
use sts_oracle_run_control::state::core::{ClientInput, EngineState};
use sts_oracle_run_control::state::events::{EventId, EventState};
use sts_oracle_run_control::state::map::node::{MapEdge, MapRoomNode, RoomType};
use sts_oracle_run_control::state::map::state::MapState;
use sts_oracle_run_control::state::shop::ShopState;

#[test]
fn shop_exposes_and_executes_run_level_potion_actions() {
    let mut session = RunControlSession::new(Default::default());
    session.engine_state = EngineState::Shop(ShopState::new());
    session.run_state.potions = vec![Some(Potion::new(PotionId::FruitJuice, 10))];

    let capture =
        capture_planner_boundary_yield_v1(&session, PlannerBoundaryYieldKindV1::CallbackStop)
            .expect("shop boundary should project");
    let visit = capture
        .visits
        .first()
        .expect("shop should produce one planner boundary visit");
    assert!(visit
        .legal_candidate_set
        .candidates
        .iter()
        .any(|candidate| matches!(
            candidate.action,
            PlannerAction::UseRunPotion {
                slot: 0,
                potion: PotionId::FruitJuice,
                potion_uuid: 10,
            }
        )));
    assert!(visit
        .legal_candidate_set
        .candidates
        .iter()
        .any(|candidate| matches!(
            candidate.action,
            PlannerAction::DiscardRunPotion {
                slot: 0,
                potion: PotionId::FruitJuice,
                potion_uuid: 10,
            }
        )));

    session
        .apply_decision_action(RunDecisionAction::Input(ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }))
        .expect("shop Fruit Juice use should preserve the shop boundary");
    assert_eq!(session.run_state.current_hp, 85);
    assert_eq!(session.run_state.max_hp, 85);
    assert!(session.run_state.potions[0].is_none());
    assert!(matches!(session.engine_state, EngineState::Shop(_)));
}

#[test]
fn we_meet_again_potion_lock_lasts_through_leave_until_room_travel() {
    // Cover the ordinary Leave screen and the engine's completed-event path.
    for already_completed in [false, true] {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        let mut event = EventState::new(EventId::WeMeetAgain);
        event.current_screen = 1;
        event.completed = already_completed;
        session.run_state.event_state = Some(event);
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::FruitJuice, 10)),
            Some(Potion::new(PotionId::ExplosivePotion, 11)),
        ];
        let mut current = MapRoomNode::new(0, 0);
        current.class = Some(RoomType::EventRoom);
        current.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut next = MapRoomNode::new(0, 1);
        next.class = Some(RoomType::RestRoom);
        session.run_state.map = MapState::new(vec![vec![current], vec![next]]);
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 0;

        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::EventChoice(0)))
            .expect("Leave opens the map");
        assert!(matches!(session.engine_state, EngineState::MapNavigation));

        // A resumable checkpoint must retain current-room potion permissions.
        let checkpoint = RunControlSessionCheckpointV1::from_session(&session);
        let encoded = serde_json::to_vec(&checkpoint).unwrap();
        let checkpoint: RunControlSessionCheckpointV1 = serde_json::from_slice(&encoded).unwrap();
        let mut session = checkpoint.into_session().unwrap();
        let surface = build_decision_surface(&session);
        assert_eq!(surface.view.candidates.len(), 1);
        assert!(session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::DiscardPotion(1)))
            .is_err());
        assert!(session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            }))
            .is_err());

        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::SelectMapNode(0)))
            .expect("travel to the next room");
        assert!(matches!(session.engine_state, EngineState::Campfire));
        assert!(session.run_state.event_state.is_none());
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::DiscardPotion(1)))
            .expect("discard is restored in the next room");
        let previous_max_hp = session.run_state.max_hp;
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            }))
            .expect("Fruit Juice is usable in the next room");
        assert_eq!(session.run_state.max_hp, previous_max_hp + 5);
    }
}
