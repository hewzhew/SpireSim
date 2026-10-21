use super::decision_surface::{
    build_decision_surface, resolve_candidate_alias, resolve_surface_candidate,
    surface_legal_visibility_violations,
};
use super::view_model::{CandidateAction, DecisionCandidate};
use super::{RunControlConfig, RunControlSession, RunDecisionAction};
use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::content::potions::{Potion, PotionId};
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::action::CardDestination;
use crate::runtime::combat::CombatCard;
use crate::state::core::{
    ActiveCombat, ChooseOneCardChoice, ClientInput, CombatContext, DiscoveryChoiceState,
    EngineState, GridSelectReason, PendingChoice, RoomCombatContext, RunPendingChoiceReason,
    RunPendingChoiceState,
};
use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};
use crate::state::map::state::MapState;
use crate::state::rewards::{BossRelicChoiceState, RewardCard, RewardItem, RewardState};

#[test]
fn decision_surface_visible_candidates_are_accepted_by_input_gate() {
    for session in contract_sessions() {
        let surface = build_decision_surface(&session);
        for candidate in &surface.view.candidates {
            let Some(input) = candidate.action.executable_input() else {
                continue;
            };
            session
                .validate_input_for_current_state(&input)
                .unwrap_or_else(|err| {
                    panic!(
                        "visible candidate '{}' in '{}' should be accepted: {err}",
                        candidate.id, surface.view.header.title
                    )
                });
        }
    }
}

#[test]
fn shop_potion_candidate_is_unavailable_when_potion_slots_are_full() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.gold = 200;
    session.run_state.potions = vec![
        Some(Potion::new(PotionId::BlockPotion, 0)),
        Some(Potion::new(PotionId::StrengthPotion, 1)),
        Some(Potion::new(PotionId::DexterityPotion, 2)),
    ];
    let mut shop = crate::state::shop::ShopState::new();
    shop.potions.push(crate::state::shop::ShopPotion {
        potion_id: PotionId::StrengthPotion,
        price: 50,
        can_buy: true,
        blocked_reason: None,
    });
    session.engine_state = EngineState::Shop(shop);

    let surface = build_decision_surface(&session);
    let potion = surface
        .view
        .candidates
        .iter()
        .find(|candidate| candidate.id == "potion-0")
        .expect("shop potion candidate");

    assert_eq!(
        potion.action,
        CandidateAction::Unavailable {
            reason: "no empty potion slot".to_string()
        }
    );
    assert!(session
        .validate_input_for_current_state(&ClientInput::BuyPotion(0))
        .is_err());
}

#[test]
fn shop_candidates_become_unavailable_when_current_gold_is_too_low() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.gold = 40;
    let mut shop = crate::state::shop::ShopState::new();
    shop.cards.push(crate::state::shop::ShopCard {
        card_id: CardId::Armaments,
        upgrades: 0,
        price: 50,
        can_buy: true,
        blocked_reason: None,
    });
    shop.relics.push(crate::state::shop::ShopRelic {
        relic_id: RelicId::InkBottle,
        price: 260,
        can_buy: true,
        blocked_reason: None,
    });
    session.engine_state = EngineState::Shop(shop);

    let surface = build_decision_surface(&session);
    for id in ["card-0", "relic-0"] {
        let candidate = surface
            .view
            .candidates
            .iter()
            .find(|candidate| candidate.id == id)
            .expect("shop candidate");
        assert_eq!(
            candidate.action,
            CandidateAction::Unavailable {
                reason: "not enough gold".to_string()
            }
        );
    }
    assert!(session
        .validate_input_for_current_state(&ClientInput::BuyCard(0))
        .is_err());
    assert!(session
        .validate_input_for_current_state(&ClientInput::BuyRelic(0))
        .is_err());
}

#[test]
fn decision_surface_visible_ids_resolve_to_their_candidate() {
    for session in contract_sessions() {
        let surface = build_decision_surface(&session);
        for candidate in &surface.view.candidates {
            if candidate.action.executable_input().is_none() {
                continue;
            }
            let resolved =
                resolve_surface_candidate(&surface, &session.engine_state, &candidate.id)
                    .unwrap_or_else(|| {
                        panic!(
                            "visible candidate '{}' in '{}' should resolve by id",
                            candidate.id, surface.view.header.title
                        )
                    });
            assert_eq!(
                resolved.action.executable_input(),
                candidate.action.executable_input(),
                "visible candidate '{}' in '{}' resolved to a different input",
                candidate.id,
                surface.view.header.title
            );
        }
    }
}

#[test]
fn decision_surface_pending_choices_expose_all_legal_inputs() {
    for session in pending_choice_contract_sessions() {
        let violations = surface_legal_visibility_violations(&session);
        assert!(
            violations.is_empty(),
            "decision surface contract violations: {violations:?}"
        );
    }
}

#[test]
fn decision_surface_scry_exposes_keep_and_discard_choices() {
    let session = pending_scry_session();
    let surface = build_decision_surface(&session);

    assert!(surface.view.candidates.iter().any(|candidate| {
        candidate.id == "select"
            && candidate.label.contains("Submit selection")
            && candidate.action.summary() == "select <idx...>"
    }));
    assert!(
        !surface
            .view
            .candidates
            .iter()
            .any(|candidate| candidate.label == "Discard Strike, Defend"),
        "scry should use compact selection surface instead of enumerating combinations"
    );
}

#[test]
fn decision_surface_contextual_numeric_aliases_are_screen_scoped() {
    let shop = test_session_at_shop();
    let shop_surface = build_decision_surface(&shop);
    assert_eq!(
        resolve_surface_candidate(&shop_surface, &shop.engine_state, "1")
            .and_then(|candidate| candidate.action.executable_input()),
        Some(ClientInput::BuyCard(1))
    );

    let campfire = campfire_session();
    let campfire_surface = build_decision_surface(&campfire);
    assert_eq!(
        resolve_surface_candidate(&campfire_surface, &campfire.engine_state, "8")
            .and_then(|candidate| candidate.action.executable_input()),
        Some(ClientInput::CampfireOption(
            crate::state::core::CampfireChoice::Smith(8)
        ))
    );
}

#[test]
fn decision_surface_fusion_hammer_hides_smith_candidates() {
    let mut session = campfire_session();
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::FusionHammer));

    let surface = build_decision_surface(&session);

    assert!(
        surface
            .view
            .candidates
            .iter()
            .all(|candidate| !candidate.id.starts_with("smith-")),
        "Fusion Hammer should remove normal Smith candidates from the campfire surface"
    );
    assert!(
        resolve_surface_candidate(&surface, &session.engine_state, "8").is_none(),
        "bare numeric campfire alias must not resolve to a hidden Smith candidate"
    );
    assert!(
        session
            .validate_input_for_current_state(&ClientInput::CampfireOption(
                crate::state::core::CampfireChoice::Smith(8)
            ))
            .is_err(),
        "direct Smith input should not pass the run-control input gate under Fusion Hammer"
    );
}

#[test]
fn decision_surface_boss_relic_screen_exposes_skip_candidate() {
    let session = boss_relic_session();
    let surface = build_decision_surface(&session);

    assert!(surface.view.candidates.iter().any(|candidate| {
        candidate.id == "skip" && candidate.action.executable_input() == Some(ClientInput::Cancel)
    }));
    assert_eq!(
        resolve_surface_candidate(&surface, &session.engine_state, "skip")
            .and_then(|candidate| candidate.action.executable_input()),
        Some(ClientInput::Cancel)
    );
}

#[test]
fn decision_surface_reward_overlay_back_notes_shop_rewards_remain_available() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut reward_state = RewardState::new();
    reward_state.items = vec![RewardItem::Card {
        cards: vec![RewardCard::new(CardId::Shockwave, 0)],
    }];
    session.engine_state = EngineState::RewardOverlay {
        reward_state,
        return_state: Box::new(EngineState::Shop(crate::state::shop::ShopState::new())),
    };

    let surface = build_decision_surface(&session);
    let back = surface
        .view
        .candidates
        .iter()
        .find(|candidate| candidate.id == "back")
        .expect("reward overlay should expose a return candidate");

    assert_eq!(back.label, "Return to shop");
    assert!(
        !back
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("abandoned"),
        "returning to the parent screen must not be described as abandoning overlay rewards"
    );
    assert!(
        back.note
            .as_deref()
            .unwrap_or_default()
            .contains("remain available"),
        "return note should tell the player that shop overlay rewards can be reopened"
    );
    assert!(
        surface
            .view
            .context
            .iter()
            .all(|line| !line.contains("abandons")),
        "details/context should not contradict Java-style overlay return behavior"
    );
}

#[test]
fn decision_surface_shop_exposes_pending_reward_overlay_candidate() {
    let mut session = test_session_at_shop();
    let EngineState::Shop(shop) = &mut session.engine_state else {
        panic!("test session should start in shop");
    };
    let mut pending = RewardState::new();
    pending.items = vec![RewardItem::Gold { amount: 25 }];
    shop.pending_reward_overlay = Some(pending);

    let surface = build_decision_surface(&session);
    let rewards = surface
        .view
        .candidates
        .iter()
        .find(|candidate| candidate.id == "rewards")
        .expect("shop should expose pending overlay rewards");

    assert_eq!(rewards.label, "Open pending rewards");
    assert_eq!(
        rewards.action.executable_input(),
        Some(ClientInput::OpenRewardOverlay)
    );
}

#[test]
fn decision_surface_reward_screen_exposes_typed_singing_bowl_actions() {
    let mut pending = reward_card_choice_session();
    pending
        .run_state
        .relics
        .push(RelicState::new(RelicId::SingingBowl));
    let pending_surface = build_decision_surface(&pending);

    assert!(pending_surface.view.candidates.iter().any(|candidate| {
        candidate.id == "bowl"
            && matches!(
                candidate.action.executable_action(),
                Some(crate::eval::run_control::RunDecisionAction::Input(
                    ClientInput::SelectCard(_)
                ))
            )
    }));

    let mut unopened = reward_screen_session();
    unopened
        .run_state
        .relics
        .push(RelicState::new(RelicId::SingingBowl));
    let unopened_surface = build_decision_surface(&unopened);

    assert!(unopened_surface.view.candidates.iter().any(|candidate| {
        candidate.id == "bowl"
            && matches!(
                candidate.action.executable_action(),
                Some(crate::eval::run_control::RunDecisionAction::SingingBowlCardReward { .. })
            )
    }));
}

#[test]
fn decision_surface_label_aliases_cover_leave_and_skip() {
    let candidates = vec![
        DecisionCandidate {
            id: "0".to_string(),
            label: "Leave.".to_string(),
            action: CandidateAction::Execute(ClientInput::EventChoice(0).into()),
            key: None,
            note: None,
            resolution: None,
        },
        DecisionCandidate {
            id: "1".to_string(),
            label: "Skip card reward".to_string(),
            action: CandidateAction::Execute(ClientInput::Proceed.into()),
            key: None,
            note: None,
            resolution: None,
        },
    ];

    assert_eq!(
        resolve_candidate_alias(&candidates, &EngineState::EventRoom, "leave")
            .map(|candidate| candidate.id.as_str()),
        Some("0")
    );
    assert_eq!(
        resolve_candidate_alias(
            &candidates,
            &EngineState::RewardScreen(RewardState::new()),
            "skip"
        )
        .map(|candidate| candidate.id.as_str()),
        Some("1")
    );
}

fn contract_sessions() -> Vec<RunControlSession> {
    let mut sessions = vec![
        RunControlSession::new(RunControlConfig::default()),
        test_session_after_neow_at_map(),
        test_session_with_first_monster_room(),
        pending_grid_session(),
        reward_screen_session(),
        reward_card_choice_session(),
        test_session_at_shop(),
        campfire_session(),
        boss_relic_session(),
        run_pending_choice_session(),
    ];
    let mut combat = test_session_with_first_monster_room();
    combat
        .apply_decision_action(RunDecisionAction::Input(ClientInput::SelectMapNode(0)))
        .expect("map input should enter combat");
    sessions.push(combat);
    sessions
}

fn pending_choice_contract_sessions() -> Vec<RunControlSession> {
    vec![
        pending_grid_session(),
        pending_hand_session(),
        pending_discovery_session(),
        pending_scry_session(),
        pending_card_reward_session(),
        pending_foreign_influence_session(),
        pending_choose_one_session(),
        pending_stance_session(),
    ]
}

fn pending_grid_session() -> RunControlSession {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Defend, 20),
    ]
    .into();
    let choice = PendingChoice::GridSelect {
        source_pile: crate::state::core::PileType::Discard,
        candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: GridSelectReason::MoveToDrawPile,
    };
    pending_session(choice, combat)
}

fn pending_hand_session() -> RunControlSession {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Defend, 20),
    ];
    let choice = PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: crate::state::HandSelectReason::Discard,
    };
    pending_session(choice, combat)
}

fn pending_discovery_session() -> RunControlSession {
    let choice = PendingChoice::DiscoverySelect(DiscoveryChoiceState {
        cards: vec![CardId::Strike, CardId::Defend],
        colorless: false,
        card_type: None,
        amount: 1,
        can_skip: true,
    });
    pending_session(choice, crate::test_support::blank_test_combat())
}

fn pending_scry_session() -> RunControlSession {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Defend, 20),
    ])
    .into();
    let choice = PendingChoice::ScrySelect {
        cards: vec![CardId::Strike, CardId::Defend],
        card_lineages: crate::test_support::scenario_lineages([10, 20]),
    };
    pending_session(choice, combat)
}

fn pending_card_reward_session() -> RunControlSession {
    let choice = PendingChoice::CardRewardSelect {
        cards: vec![CardId::BattleTrance, CardId::ShrugItOff],
        destination: CardDestination::Hand,
        can_skip: true,
    };
    pending_session(choice, crate::test_support::blank_test_combat())
}

fn pending_foreign_influence_session() -> RunControlSession {
    let choice = PendingChoice::ForeignInfluenceSelect {
        cards: vec![CardId::Strike, CardId::Headbutt],
        upgraded: true,
    };
    pending_session(choice, crate::test_support::blank_test_combat())
}

fn pending_choose_one_session() -> RunControlSession {
    let choice = PendingChoice::ChooseOneSelect {
        choices: vec![
            ChooseOneCardChoice {
                card_id: CardId::InfernalBlade,
                upgrades: 0,
            },
            ChooseOneCardChoice {
                card_id: CardId::Warcry,
                upgrades: 1,
            },
        ],
    };
    pending_session(choice, crate::test_support::blank_test_combat())
}

fn pending_stance_session() -> RunControlSession {
    pending_session(
        PendingChoice::StanceChoice,
        crate::test_support::blank_test_combat(),
    )
}

fn pending_session(
    choice: PendingChoice,
    combat: crate::runtime::combat::CombatState,
) -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::PendingChoice(choice.clone());
    session.active_combat = Some(ActiveCombat::new(
        EngineState::PendingChoice(choice),
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    session
}

fn reward_screen_session() -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::RewardScreen(RewardState {
        items: vec![
            RewardItem::Gold { amount: 19 },
            RewardItem::Potion {
                potion_id: PotionId::StrengthPotion,
            },
            RewardItem::Card {
                cards: vec![
                    RewardCard::new(CardId::Shockwave, 0),
                    RewardCard::new(CardId::Armaments, 0),
                    RewardCard::new(CardId::SeverSoul, 0),
                ],
            },
        ],
        ..RewardState::new()
    });
    session
}

fn reward_card_choice_session() -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::RewardScreen(RewardState {
        items: Vec::new(),
        pending_card_choice: Some(vec![
            RewardCard::new(CardId::Shockwave, 0),
            RewardCard::new(CardId::Armaments, 0),
            RewardCard::new(CardId::SeverSoul, 0),
        ]),
        ..RewardState::new()
    });
    session
}

fn test_session_at_shop() -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.event_state = None;
    session.run_state.gold = 100;
    let mut shop = crate::state::shop::ShopState::new();
    shop.cards = vec![
        crate::state::shop::ShopCard {
            card_id: CardId::Armaments,
            upgrades: 0,
            price: 49,
            can_buy: true,
            blocked_reason: None,
        },
        crate::state::shop::ShopCard {
            card_id: CardId::ShrugItOff,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        },
    ];
    session.engine_state = EngineState::Shop(shop);
    session
}

fn campfire_session() -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::Campfire;
    session
}

fn boss_relic_session() -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::BossRelicSelect(BossRelicChoiceState::new(vec![
        RelicId::CoffeeDripper,
        RelicId::BlackBlood,
        RelicId::Astrolabe,
    ]));
    session
}

fn run_pending_choice_session() -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: RunPendingChoiceReason::Upgrade,
        source: crate::state::selection::DomainEventSource::Selection(
            RunPendingChoiceReason::Upgrade.into(),
        ),
        return_state: Box::new(EngineState::Campfire),
    });
    session
}

fn test_session_with_first_monster_room() -> RunControlSession {
    let mut session = test_session_after_neow_at_map();
    let mut first = MapRoomNode::new(0, 0);
    first.class = Some(RoomType::MonsterRoom);
    first.edges.insert(MapEdge::new(0, 0, 0, 1));
    let mut second = MapRoomNode::new(0, 1);
    second.class = Some(RoomType::MonsterRoom);
    session.run_state.map = MapState::new(vec![vec![first], vec![second]]);
    session.run_state.monster_list = vec![EncounterId::JawWorm, EncounterId::Cultist];
    session
}

fn test_session_after_neow_at_map() -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.event_state = None;
    session.engine_state = EngineState::MapNavigation;
    session
}
