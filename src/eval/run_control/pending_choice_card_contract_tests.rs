use super::decision_surface::{build_decision_surface, surface_legal_visibility_violations};
use super::view_model::CandidateAction;
use super::{RunControlConfig, RunControlSession, RunDecisionAction};
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::runtime::monster_move::{AttackSpec, DamageKind, MonsterMoveSpec, MonsterTurnPlan};
use crate::state::core::{
    ActiveCombat, ClientInput, CombatContext, EngineState, GridSelectReason, HandSelectReason,
    PendingChoice, RoomCombatContext,
};
use crate::state::map::node::RoomType;

#[derive(Clone, Copy, Debug)]
enum ExpectedPendingChoice {
    Hand(HandSelectReason),
    Grid(GridSelectReason),
    Discovery,
}

#[derive(Clone, Debug)]
struct PendingCardContractCase {
    name: &'static str,
    card: CombatCard,
    target: Option<usize>,
    setup: CombatSetup,
    expected: ExpectedPendingChoice,
}

#[derive(Clone, Debug, Default)]
struct CombatSetup {
    hand_after_play_card: Vec<CombatCard>,
    draw_pile: Vec<CombatCard>,
    discard_pile: Vec<CombatCard>,
    exhaust_pile: Vec<CombatCard>,
}

#[test]
fn ironclad_and_neow_colorless_pending_cards_surface_all_legal_resolutions() {
    for case in pending_card_contract_cases() {
        let mut session = active_combat_session(build_combat_for_case(&case));
        let play_input = ClientInput::PlayCard {
            card_index: 0,
            target: case.target,
        };

        session
            .apply_decision_action(RunDecisionAction::Input(play_input))
            .unwrap_or_else(|err| {
                panic!("{} should play into a stable boundary: {err}", case.name)
            });

        assert_pending_choice_matches(&case, &session.engine_state);
        assert_pending_choice_surface_contract(&case, &session);
    }
}

#[test]
fn pending_card_contract_cases_are_real_visible_card_plays() {
    for case in pending_card_contract_cases() {
        let session = active_combat_session(build_combat_for_case(&case));
        let surface = build_decision_surface(&session);
        let expected_input = ClientInput::PlayCard {
            card_index: 0,
            target: case.target,
        };

        assert!(
            surface
                .view
                .candidates
                .iter()
                .any(|candidate| candidate.action.executable_input() == Some(expected_input.clone())),
            "{} setup must expose the source card play before checking the pending surface; candidates={:?}",
            case.name,
            surface.view.candidates
        );
    }
}

fn assert_pending_choice_matches(case: &PendingCardContractCase, engine_state: &EngineState) {
    let EngineState::PendingChoice(choice) = engine_state else {
        panic!(
            "{} should stop at PendingChoice, got {engine_state:?}",
            case.name
        );
    };

    match (case.expected, choice) {
        (
            ExpectedPendingChoice::Hand(expected_reason),
            PendingChoice::HandSelect { reason, .. },
        ) => assert_eq!(
            *reason, expected_reason,
            "{} produced unexpected hand-select reason",
            case.name
        ),
        (
            ExpectedPendingChoice::Grid(expected_reason),
            PendingChoice::GridSelect { reason, .. },
        ) => assert_eq!(
            *reason, expected_reason,
            "{} produced unexpected grid-select reason",
            case.name
        ),
        (ExpectedPendingChoice::Discovery, PendingChoice::DiscoverySelect(_)) => {}
        (expected, actual) => panic!(
            "{} produced wrong pending choice kind: expected {expected:?}, got {actual:?}",
            case.name
        ),
    }
}

fn assert_pending_choice_surface_contract(
    case: &PendingCardContractCase,
    session: &RunControlSession,
) {
    let violations = surface_legal_visibility_violations(session);
    assert!(
        violations.is_empty(),
        "{} pending surface must expose every legal resolution: {violations:?}",
        case.name
    );

    let surface = build_decision_surface(session);
    assert!(
        !surface.view.candidates.is_empty(),
        "{} pending surface must have visible candidates",
        case.name
    );
    assert!(
        !surface
            .view
            .candidates
            .iter()
            .any(|candidate| candidate.label == "Proceed"
                || candidate.action.executable_input() == Some(ClientInput::Proceed)),
        "{} pending surface must not use fake Proceed fallback: {:?}",
        case.name,
        surface.view.candidates
    );

    if let Some(selection_surface) = super::selection_surface::active_selection_surface(session) {
        let select = surface
            .view
            .candidates
            .iter()
            .find(|candidate| candidate.id == "select")
            .unwrap_or_else(|| {
                panic!(
                    "{} compact selection surface must expose a select command: {:?}",
                    case.name, surface.view.candidates
                )
            });
        assert!(
            matches!(select.action, CandidateAction::Parameterized { .. }),
            "{} compact selection command must stay manual, got {:?}",
            case.name,
            select.action
        );
        assert!(
            select
                .note
                .as_ref()
                .is_some_and(|note| note.contains(&format!("{}", selection_surface.max_choices))),
            "{} compact selection command should describe bounds, got {:?}",
            case.name,
            select.note
        );
        assert!(
            !surface
                .view
                .candidates
                .iter()
                .any(|candidate| candidate.label.contains(", ")),
            "{} compact selection surface must not enumerate card combinations: {:?}",
            case.name,
            surface.view.candidates
        );
        return;
    }

    for candidate in &surface.view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            panic!(
                "{} pending candidate '{}' must be executable, got {:?}",
                case.name, candidate.id, candidate.action
            );
        };
        session
            .validate_input_for_current_state(&input)
            .unwrap_or_else(|err| {
                panic!(
                    "{} pending candidate '{}' should pass input gate: {err}",
                    case.name, candidate.id
                )
            });
    }
}

fn pending_card_contract_cases() -> Vec<PendingCardContractCase> {
    vec![
        PendingCardContractCase {
            name: "Ironclad Armaments",
            card: card(CardId::Armaments, 1),
            target: None,
            setup: CombatSetup {
                hand_after_play_card: vec![card(CardId::Strike, 10), card(CardId::Defend, 20)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Hand(HandSelectReason::Upgrade),
        },
        PendingCardContractCase {
            name: "Ironclad Dual Wield",
            card: card(CardId::DualWield, 1),
            target: None,
            setup: CombatSetup {
                hand_after_play_card: vec![card(CardId::Strike, 10), card(CardId::Bash, 20)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Hand(HandSelectReason::Copy { amount: 2 }),
        },
        PendingCardContractCase {
            name: "Ironclad Exhume",
            card: card(CardId::Exhume, 1),
            target: None,
            setup: CombatSetup {
                exhaust_pile: vec![card(CardId::Strike, 10), card(CardId::Defend, 20)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Grid(GridSelectReason::Exhume { upgrade: false }),
        },
        PendingCardContractCase {
            name: "Ironclad Headbutt",
            card: card(CardId::Headbutt, 1),
            target: Some(1),
            setup: CombatSetup {
                discard_pile: vec![card(CardId::Strike, 10), card(CardId::Defend, 20)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Grid(GridSelectReason::MoveToDrawPile),
        },
        PendingCardContractCase {
            name: "Ironclad True Grit+",
            card: upgraded_card(CardId::TrueGrit, 1),
            target: None,
            setup: CombatSetup {
                hand_after_play_card: vec![card(CardId::Strike, 10), card(CardId::Defend, 20)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Hand(HandSelectReason::Exhaust),
        },
        PendingCardContractCase {
            name: "Ironclad Warcry",
            card: card(CardId::Warcry, 1),
            target: None,
            setup: CombatSetup {
                hand_after_play_card: vec![card(CardId::Strike, 10), card(CardId::Defend, 20)],
                draw_pile: vec![card(CardId::Bash, 30)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Hand(HandSelectReason::PutOnDrawPile),
        },
        PendingCardContractCase {
            name: "Colorless Discovery",
            card: card(CardId::Discovery, 1),
            target: None,
            setup: CombatSetup::default(),
            expected: ExpectedPendingChoice::Discovery,
        },
        PendingCardContractCase {
            name: "Colorless Secret Technique",
            card: card(CardId::SecretTechnique, 1),
            target: None,
            setup: CombatSetup {
                draw_pile: vec![card(CardId::Defend, 10), card(CardId::ShrugItOff, 20)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Grid(GridSelectReason::SkillFromDeckToHand),
        },
        PendingCardContractCase {
            name: "Colorless Secret Weapon",
            card: card(CardId::SecretWeapon, 1),
            target: None,
            setup: CombatSetup {
                draw_pile: vec![card(CardId::Strike, 10), card(CardId::Bash, 20)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Grid(GridSelectReason::AttackFromDeckToHand),
        },
        PendingCardContractCase {
            name: "Colorless Thinking Ahead",
            card: card(CardId::ThinkingAhead, 1),
            target: None,
            setup: CombatSetup {
                hand_after_play_card: vec![card(CardId::Strike, 10), card(CardId::Defend, 20)],
                draw_pile: vec![card(CardId::Bash, 30), card(CardId::PommelStrike, 40)],
                ..Default::default()
            },
            expected: ExpectedPendingChoice::Hand(HandSelectReason::PutOnDrawPile),
        },
    ]
}

fn build_combat_for_case(case: &PendingCardContractCase) -> CombatState {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![visible_test_monster()];
    combat.zones.hand = std::iter::once(case.card.clone())
        .chain(case.setup.hand_after_play_card.clone())
        .collect();
    combat.zones.draw_pile = (case.setup.draw_pile.clone()).into();
    combat.zones.discard_pile = case.setup.discard_pile.clone().into();
    combat.zones.exhaust_pile = (case.setup.exhaust_pile.clone()).into();
    combat.turn.energy = 3;
    combat
}

fn visible_test_monster() -> crate::runtime::combat::MonsterEntity {
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    let plan = MonsterTurnPlan::from_spec(
        1,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 11,
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    );
    monster.set_planned_move_id(plan.move_id);
    monster.set_planned_steps(plan.steps);
    monster.set_planned_visible_spec(plan.visible_spec);
    monster
}

fn active_combat_session(combat: CombatState) -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::CombatPlayerTurn;
    session.active_combat = Some(ActiveCombat::new(
        EngineState::CombatPlayerTurn,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    session
}

fn card(id: CardId, uuid: u32) -> CombatCard {
    CombatCard::for_scenario(id, uuid)
}

fn upgraded_card(id: CardId, uuid: u32) -> CombatCard {
    let mut card = CombatCard::for_scenario(id, uuid);
    card.upgrades = 1;
    card
}
