use std::fs;
use std::path::{Path, PathBuf};

use sts_oracle_learning::agent::task::{
    TaskBudgetV1, TaskPublicCandidateV1, TaskPublicObservationV1, TaskStrategicActionV1,
};
use sts_oracle_learning::ai::planner_core::{
    PlannerDecisionContext, PlannerRewardDescriptor, PlannerShopInventoryV1,
};
use sts_oracle_learning::content::cards::CardId;
use sts_oracle_learning::content::events::face_trader::FaceTraderResult;
use sts_oracle_learning::content::potions::{Potion, PotionId};
use sts_oracle_learning::content::relics::{RelicId, RelicState};
use sts_oracle_learning::eval::run_control::{
    CombatResearchPlanOptionsV1, RunControlConfig, RunControlSession, RunControlSessionCheckpointV1,
};
use sts_oracle_learning::state::core::EngineState;
use sts_oracle_learning::state::events::{EventId, EventState};
use sts_oracle_learning::state::map::{
    node::{MapEdge, MapRoomNode, RoomType},
    state::MapState,
};
use sts_oracle_learning::state::rewards::{RewardCard, RewardItem, RewardState};
use sts_oracle_learning::state::selection::DomainEventSource;
use sts_oracle_learning::state::shop::{ShopCard, ShopPotion, ShopRelic, ShopState};

use crate::decision::{
    DecisionControlV1, DecisionDetailV1, RunPlayBoundChoiceV1, RunPlayDecisionReadingV1,
};
use crate::driver::{create_constructed_decision_session_v1, RunPlayControllerV1};
use crate::model::RunPlayEventV1;
use crate::storage::RunPlayPathsV1;

#[path = "decision_tests/combat_hp.rs"]
mod combat_hp_tests;
#[path = "decision_tests/draft_open.rs"]
mod draft_open_tests;
#[path = "decision_tests/preparation_classifier_cases.rs"]
mod preparation_classifier_cases;
#[path = "decision_tests/research_budget.rs"]
mod research_budget_tests;
#[path = "decision_tests/reward_sequence.rs"]
mod reward_sequence_tests;
#[path = "decision_tests/saved_plans.rs"]
mod saved_plans_tests;

pub(crate) struct TestDirectory(pub(crate) PathBuf);
impl TestDirectory {
    pub(crate) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sts-decision-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("sts-decision-"));
        fs::remove_dir_all(root).unwrap();
    }
}

pub(crate) fn fixture(kind: &str) -> RunControlSession {
    let mut session = RunControlSession::new(RunControlConfig {
        seed: 20260910,
        ascension_level: 20,
        final_act: true,
        ..RunControlConfig::default()
    });
    session.run_state.event_state = None;
    match kind {
        "astrolabe" | "empty_cage" | "bottled_flame" | "bottled_lightning" | "bottled_tornado" => {
            session.run_state.master_deck.clear();
            for card in [
                CardId::AscendersBane,
                CardId::Strike,
                CardId::Defend,
                CardId::Strike,
                CardId::Bash,
                CardId::ShrugItOff,
                CardId::Corruption,
            ] {
                session.run_state.add_card_to_deck(card);
            }
            session
                .run_state
                .upgrade_card(session.run_state.master_deck[4].lineage);
            let relic = match kind {
                "astrolabe" => RelicId::Astrolabe,
                "empty_cage" => RelicId::EmptyCage,
                "bottled_flame" => RelicId::BottledFlame,
                "bottled_lightning" => RelicId::BottledLightning,
                _ => RelicId::BottledTornado,
            };
            session.engine_state = session
                .run_state
                .obtain_relic_with_source(
                    relic,
                    EngineState::MapNavigation,
                    DomainEventSource::Relic(relic),
                )
                .expect("normal on-equip selection boundary");
        }
        "reward" | "reward_late" | "reward_potions" => {
            if kind == "reward_potions" {
                session.run_state.potions = vec![
                    Some(Potion::new(PotionId::FirePotion, 0)),
                    Some(Potion::new(PotionId::SwiftPotion, 1)),
                ];
            }
            if kind == "reward_late" {
                session.run_state.advance_act();
                session.run_state.advance_act();
                session.run_state.floor_num = 35;
                session.run_state.master_deck.clear();
                for card in [
                    CardId::AscendersBane,
                    CardId::Corruption,
                    CardId::DarkEmbrace,
                    CardId::FeelNoPain,
                    CardId::ShrugItOff,
                    CardId::ShrugItOff,
                    CardId::BurningPact,
                    CardId::BodySlam,
                    CardId::PommelStrike,
                    CardId::Shockwave,
                ] {
                    session.run_state.add_card_to_deck(card);
                }
            }
            let cards = [
                CardId::ShrugItOff,
                CardId::PerfectedStrike,
                CardId::ThunderClap,
            ]
            .map(|card| RewardCard::new(card, 0))
            .to_vec();
            let mut rewards = RewardState::new();
            rewards.items = vec![RewardItem::Card {
                cards: cards.clone(),
            }];
            rewards.pending_card_choice = Some(cards);
            rewards.pending_card_reward_index = Some(0);
            session.engine_state = EngineState::RewardScreen(rewards);
        }
        "full_reward" | "reward_sequence" | "reward_sequence_crowded" | "reward_double_gold" => {
            session.run_state.potions = match kind {
                "full_reward" => vec![
                    Some(Potion::new(PotionId::FirePotion, 0)),
                    Some(Potion::new(PotionId::SwiftPotion, 1)),
                ],
                "reward_sequence_crowded" => vec![Some(Potion::new(PotionId::FirePotion, 0)), None],
                _ => vec![None, None],
            };
            let mut rewards = RewardState::new();
            rewards.items = vec![
                RewardItem::Gold { amount: 11 },
                RewardItem::Potion {
                    potion_id: PotionId::AncientPotion,
                },
                RewardItem::Card {
                    cards: vec![RewardCard::new(CardId::Headbutt, 0)],
                },
            ];
            if kind == "reward_sequence_crowded" {
                rewards.items.insert(
                    2,
                    RewardItem::Potion {
                        potion_id: PotionId::SwiftPotion,
                    },
                );
            }
            if kind == "reward_double_gold" {
                rewards.items.push(RewardItem::Gold { amount: 54 });
            }
            session.engine_state = EngineState::RewardScreen(rewards);
        }
        "shop" | "shop_twins" | "shop_poor" | "shop_courier" => {
            session.run_state.gold = 450;
            session.run_state.potions = vec![
                Some(Potion::new(PotionId::BlockPotion, 0)),
                Some(Potion::new(PotionId::StrengthPotion, 1)),
            ];
            let mut shop = ShopState::new();
            shop.cards = vec![
                ShopCard {
                    card_id: CardId::ShrugItOff,
                    upgrades: 0,
                    price: 60,
                    can_buy: true,
                    blocked_reason: None,
                },
                ShopCard {
                    card_id: CardId::PommelStrike,
                    upgrades: 0,
                    price: 80,
                    can_buy: true,
                    blocked_reason: None,
                },
            ];
            shop.relics = vec![ShopRelic {
                relic_id: RelicId::MembershipCard,
                price: 150,
                can_buy: true,
                blocked_reason: None,
            }];
            shop.potions = vec![ShopPotion {
                potion_id: PotionId::PowerPotion,
                price: 100,
                can_buy: true,
                blocked_reason: None,
            }];
            if kind == "shop_twins" {
                shop.cards[1] = shop.cards[0].clone();
            }
            if kind == "shop_poor" {
                session.run_state.gold = 10;
            }
            if kind == "shop_courier" {
                session
                    .run_state
                    .relics
                    .push(RelicState::new(RelicId::Courier));
                session.run_state.potions[0] = None;
                // Constructed offers already include Courier's entry discount.
                for card in &mut shop.cards {
                    card.price = (card.price as f32 * 0.8).round() as i32;
                }
                shop.relics[0].price = 120;
                shop.potions[0].price = 80;
                shop.purge_cost = 60;
            }
            session.engine_state = EngineState::Shop(shop);
        }
        "route" | "route_question" => {
            session.run_state.advance_act();
            session.run_state.floor_num = 17;
            let mut left = MapRoomNode::new(0, 0);
            left.class = Some(if kind == "route_question" {
                RoomType::EventRoom
            } else {
                RoomType::MonsterRoom
            });
            left.edges.insert(MapEdge::new(0, 0, 0, 1));
            let mut right = MapRoomNode::new(1, 0);
            right.class = Some(RoomType::MonsterRoom);
            right.edges.insert(MapEdge::new(1, 0, 1, 1));
            let mut campfire = MapRoomNode::new(0, 1);
            campfire.class = Some(RoomType::RestRoom);
            let mut shop = MapRoomNode::new(1, 1);
            shop.class = Some(RoomType::ShopRoom);
            session.run_state.map = MapState::new(vec![vec![left, right], vec![campfire, shop]]);
            session.run_state.map.current_x = -1;
            session.run_state.map.current_y = -1;
            session.engine_state = EngineState::MapNavigation;
        }
        "combat_finish"
        | "combat_defeat"
        | "combat_draft_edit"
        | "combat_multiturn"
        | "combat_discovery"
        | "combat_discovery_preview"
        | "combat_exhaust"
        | "combat_discard"
        | "combat_pact_draft"
        | "combat_brew_draft"
        | "combat_reaper_finish"
        | "combat_hemokinesis_finish"
        | "combat_thorns_finish" => {
            use sts_oracle_learning::content::monsters::factory::EncounterId;
            use sts_oracle_learning::sim::combat_start::build_natural_combat_start;
            use sts_oracle_learning::state::core::{
                ActiveCombat, CombatContext, RoomCombatContext,
            };

            session.run_state.floor_num = 1;
            session.run_state.master_deck.clear();
            let deck = if kind == "combat_reaper_finish" {
                vec![CardId::Reaper]
            } else if kind == "combat_hemokinesis_finish" {
                vec![CardId::Hemokinesis]
            } else if kind == "combat_thorns_finish" {
                session
                    .run_state
                    .relics
                    .push(RelicState::new(RelicId::BronzeScales));
                vec![CardId::Defend]
            } else if matches!(kind, "combat_finish" | "combat_draft_edit") {
                vec![CardId::Strike]
            } else {
                vec![
                    CardId::Strike,
                    CardId::Bash,
                    CardId::Defend,
                    CardId::Strike,
                    if matches!(kind, "combat_exhaust" | "combat_pact_draft") {
                        CardId::BurningPact
                    } else {
                        CardId::Defend
                    },
                ]
            };
            for card in deck {
                session.run_state.add_card_to_deck(card);
            }
            if kind == "combat_defeat" {
                session.run_state.current_hp = 1;
            }
            if matches!(kind, "combat_discovery" | "combat_discovery_preview") {
                session.run_state.potions[0] = Some(Potion::new(PotionId::PowerPotion, 0));
            }
            if kind == "combat_draft_edit" {
                session.run_state.potions[0] = Some(Potion::new(PotionId::BlockPotion, 0));
            }
            if matches!(kind, "combat_discard" | "combat_brew_draft") {
                session.run_state.potions[0] = Some(Potion::new(PotionId::GamblersBrew, 0));
            }
            let (engine, mut combat) = build_natural_combat_start(
                &mut session.run_state,
                EncounterId::JawWorm,
                RoomType::MonsterRoom,
            )
            .unwrap();
            // Deliberately one hit from victory. The normal constructor owns
            // player state, card lineage and move plans in this constructed case.
            if kind.ends_with("_finish") {
                combat.entities.monsters[0].current_hp = 1;
            }
            if kind == "combat_draft_edit" {
                combat.entities.monsters[0].current_hp = 12;
            }
            if kind == "combat_thorns_finish" {
                // Constructed protected enemy phase: the normal relic hook
                // supplies Thorns, and battle-end healing settles during End Turn.
                combat.entities.player.block = 20;
            }
            let position = sts_oracle_learning::sim::combat::CombatPosition::new(engine, combat);
            let preparation = match kind {
                "combat_discovery" | "combat_discard" => {
                    vec![sts_oracle_learning::state::core::ClientInput::UsePotion {
                        potion_index: 0,
                        target: None,
                    }]
                }
                "combat_exhaust" => vec![sts_oracle_learning::state::core::ClientInput::PlayCard {
                    card_index: position
                        .combat
                        .zones
                        .hand
                        .iter()
                        .position(|c| c.id == CardId::BurningPact)
                        .unwrap(),
                    target: None,
                }],
                _ => vec![],
            };
            let position = if preparation.is_empty() {
                position
            } else {
                sts_combat_planner::materialize_exact_action_line(
                    &sts_oracle_learning::sim::combat::EngineCombatStepper,
                    &position,
                    &preparation,
                    250,
                )
                .unwrap()
                .final_position
            };
            let engine = position.engine;
            let combat = position.combat;
            session.engine_state = engine.clone();
            session.active_combat = Some(ActiveCombat::new_for_encounter(
                engine,
                combat,
                EncounterId::JawWorm,
                CombatContext::Room(RoomCombatContext {
                    room_type: RoomType::MonsterRoom,
                }),
            ));
        }
        _ => panic!("unknown fixture"),
    }
    session
}

fn controller(root: &Path, kind: &str) -> RunPlayControllerV1 {
    controller_with_session(root, kind, &fixture(kind))
}

pub(crate) fn controller_with_session(
    root: &Path,
    kind: &str,
    session: &RunControlSession,
) -> RunPlayControllerV1 {
    let paths = RunPlayPathsV1::new(root.join(kind));
    create_constructed_decision_session_v1(
        &paths,
        RunControlSessionCheckpointV1::from_session(session),
        &format!("strategic-interaction-{kind}"),
        TaskBudgetV1 {
            max_world_transitions: 256,
            max_reloads: 0,
        },
        CombatResearchPlanOptionsV1::default(),
    )
    .unwrap();
    RunPlayControllerV1::open(paths).unwrap()
}

#[test]
fn brief_reading_preserves_choices_resources_and_pending_card_rules() {
    let dir = TestDirectory::new();
    for kind in [
        "reward",
        "full_reward",
        "route",
        "combat_multiturn",
        "combat_discovery",
        "combat_exhaust",
    ] {
        let mut game = controller(&dir.0, kind);
        let before = game.head().clone();
        let full = game.decision_reading_v1(&[], None).unwrap();
        let brief = game
            .decision_reading_v1(&[DecisionDetailV1::Brief], None)
            .unwrap();
        let brief_with_state = game
            .decision_reading_v1(&[DecisionDetailV1::Brief, DecisionDetailV1::State], None)
            .unwrap();
        assert_eq!(brief_with_state.view, brief.view);
        assert!(brief_with_state.state.is_some());
        assert_eq!(brief.source, full.source);
        assert_eq!(brief.options, full.options);
        assert_eq!(brief.selections, full.selections);
        assert_eq!(brief.context, full.context);
        assert_eq!(brief.can_collect_rewards, full.can_collect_rewards);
        assert_eq!(brief.last_choice, full.last_choice);
        assert_eq!(*game.head(), before);
        assert!(brief.view.contains("HP "));
        assert!(brief.view.contains("Potions:"));
        assert!(!brief.view.contains("Keys:"));
        assert!(!brief.view.contains("Missing keys:"));
        if !kind.starts_with("combat") {
            if kind == "reward" {
                assert!(brief.view.contains("Relics:"));
                assert!(brief.view.contains("cards |"));
            } else {
                assert!(!brief.view.contains("Relics:"));
            }
            assert!(brief.view.contains("Deck:"));
        }
        if kind == "reward" {
            assert!(brief.view.contains("Shrug It Off"));
            assert!(brief.view.contains("1c · Skill · 8 base Block · draw 1"));
            assert!(!brief.view.contains("Printed rule (base/upgraded):"));
            assert!(!brief
                .view
                .contains(crate::mechanics_info::KEY_COLLECTION_TIMING));
        }
        if kind == "full_reward" {
            assert!(brief.view.contains("Unavailable reward:"));
            assert!(brief.view.contains("Potion slots full."));
            assert!(brief.options.iter().any(|option| matches!(
                option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::DiscardRunPotion { .. }
                }
            )));
        }
        if kind == "route" {
            assert!(full.view.contains("Connected paths to the first Elite"));
            assert!(!brief.view.contains("Connected paths to the first Elite"));
            for option in &brief.options {
                if let TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
                } = option.candidate
                {
                    assert!(brief.view.contains(&format!("Go ({x},{y})")));
                }
            }
        }
        if kind.starts_with("combat") {
            for prefix in ["Turn ", "Hand:", "Draw ", "M0 "] {
                let line = full
                    .view
                    .lines()
                    .find(|line| line.starts_with(prefix))
                    .unwrap();
                assert!(brief.view.lines().any(|candidate| candidate == line));
            }
            assert!(!brief
                .view
                .contains("Possible moves (base values before powers; not a prediction)"));
        }
        if matches!(kind, "combat_discovery" | "combat_exhaust") {
            assert!(brief.view.contains("Choice cards:"));
            assert!(!brief.view.contains("Printed rule (base/upgraded):"));
            assert!(full.view.contains("Choice card rules (base/upgraded):"));
        }
    }
}

#[test]
fn preview_defeat_shows_observed_terminal_hp_without_moving_the_head() {
    use sts_oracle_learning::eval::run_control::RunActionResultChangeV1;

    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_defeat");
    let head = game.head().clone();
    let comparison = game
        .compare_action_lines_v1(&[vec![crate::selector::RunPlayActionSelectorV1::EndTurn]])
        .unwrap();
    assert_eq!(game.head(), &head);
    let last = comparison.lines[0].steps.last().unwrap();
    assert!(last.next_decision.is_none());
    let hp_change = last
        .action_result
        .as_ref()
        .unwrap()
        .changes
        .iter()
        .find_map(|change| match change {
            RunActionResultChangeV1::HpChanged {
                before_current,
                before_max,
                after_current,
                after_max,
            } => Some((*before_current, *before_max, *after_current, *after_max)),
            _ => None,
        })
        .expect("terminal action preserves its observed HP change");
    assert_eq!(hp_change.0, 1);
    assert_eq!(hp_change.2, 0);
    let expected = format!(
        "Defeat | HP {}/{} → {}/{}",
        hp_change.0, hp_change.1, hp_change.2, hp_change.3
    );
    let preview = crate::compact::render_preview_v1(&comparison, false).unwrap();
    let detail = crate::compact::render_preview_v1(&comparison, true).unwrap();
    assert!(
        preview.contains(&format!("Result: {expected}")),
        "{preview}"
    );
    assert!(detail.contains(&format!("final {expected}")), "{detail}");
}

#[test]
fn preview_cannot_reveal_question_mark_room() {
    use crate::selector::RunPlayActionSelectorV1;

    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "route_question");
    let head = game.head().clone();
    let error = game
        .compare_action_lines_v1(&[vec![RunPlayActionSelectorV1::ChooseRoute { x: 0, y: 0 }]])
        .err()
        .expect("an unrevealed room must reject preview");
    assert!(error.contains("unrevealed ? room"), "{error}");
    assert_eq!(game.head(), &head);

    let decision = head
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let ordinal = decision
        .candidates
        .iter()
        .position(|candidate| {
            matches!(
                candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ChooseRouteNode { x: 0, y: 0, .. }
                }
            )
        })
        .unwrap();
    let ordinal_error = game
        .compare_action_lines_v1(&[vec![RunPlayActionSelectorV1::OrdinalPath(vec![ordinal])]])
        .err()
        .expect("an ordinal must not bypass the room guard");
    assert!(
        ordinal_error.contains("unrevealed ? room"),
        "{ordinal_error}"
    );
    assert_eq!(game.head(), &head);

    game.compare_action_lines_v1(&[vec![RunPlayActionSelectorV1::ChooseRoute { x: 1, y: 0 }]])
        .unwrap();
    assert_eq!(game.head(), &head);
}

#[test]
fn map_choice_reading_exposes_when_a_branch_loses_the_key_route() {
    let dir = TestDirectory::new();
    let mut session = fixture("route");
    let key = &mut session.run_state.map.graph[1][0];
    key.class = Some(RoomType::MonsterRoomElite);
    key.has_emerald_key = true;
    key.edges.insert(MapEdge::new(0, 1, 0, 2));
    session.run_state.map.graph[1][1]
        .edges
        .insert(MapEdge::new(1, 1, 0, 2));
    let mut boss = MapRoomNode::new(0, 2);
    boss.class = Some(RoomType::MonsterRoomBoss);
    session.run_state.map.graph.push(vec![boss]);
    let mut game = controller_with_session(&dir.0, "key-route-fork", &session);
    let head = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let left = reading
        .view
        .lines()
        .find(|l| l.contains("Go (0,0) Monster"))
        .unwrap_or_else(|| panic!("{}", reading.view));
    let right = reading
        .view
        .lines()
        .find(|l| l.contains("Go (1,0) Monster"))
        .unwrap_or_else(|| panic!("{}", reading.view));
    assert!(left.contains("Emerald elite: on every route"));
    assert!(right.contains("Emerald elite: none"));
    assert!(right.contains("shop map y1 reachable"));
    let brief = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    assert_eq!(reading.options, brief.options);
    assert_eq!(reading.source, brief.source);
    assert_eq!(brief.view.matches("Legend:").count(), 1);
    assert!(brief.view.contains("From (0,0)"));
    assert!(brief.view.contains("K(0,1)"));
    assert!(brief.view.contains("From (1,0)"));
    assert!(!brief.view.contains("All next-room choices"));
    let decision = head
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    assert_eq!(
        reading
            .options
            .iter()
            .map(|o| &o.candidate)
            .collect::<Vec<_>>(),
        decision.candidates.iter().collect::<Vec<_>>()
    );
    assert_eq!(*game.head(), head);
}

fn pick(
    reading: &RunPlayDecisionReadingV1,
    predicate: impl Fn(&TaskStrategicActionV1) -> bool,
) -> RunPlayBoundChoiceV1 {
    let options: Vec<_> = reading
        .options
        .iter()
        .filter(|option| {
            matches!(&option.candidate,
        TaskPublicCandidateV1::Strategic { action } if predicate(action))
        })
        .collect();
    assert_eq!(options.len(), 1);
    RunPlayBoundChoiceV1 {
        source: reading.source.clone().unwrap(),
        choice: options[0].choice.clone(),
    }
}

#[test]
fn context_reuse_is_explicit_and_keeps_choices_state_and_recovery_complete() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward");
    let full = game.decision_reading_v1(&[], None).unwrap();
    let brief = game
        .decision_reading_v1(&[DecisionDetailV1::State], full.context.as_ref())
        .unwrap();
    assert!(!brief.view.contains("Deck:"));
    assert!(!brief.view.contains("Relics:"));
    assert!(!brief.view.contains("Potions:"));
    assert!(brief.view.contains("Unchanged: Deck, Relics, Potions"));
    assert_eq!(brief.source, full.source);
    assert_eq!(brief.options, full.options);
    assert!(brief.state.is_some());
    assert_eq!(game.decision_reading_v1(&[], None).unwrap(), full);

    let mut bad_ack = full.context.clone().unwrap();
    bad_ack.deck = Some("a context the caller does not possess".into());
    let refreshed = game.decision_reading_v1(&[], Some(&bad_ack)).unwrap();
    assert!(refreshed.view.contains("Deck:"));
    assert!(!refreshed.view.contains("Relics:"));
    let chosen = pick(&full, |a| {
        matches!(
            a,
            TaskStrategicActionV1::TakeCard {
                card: CardId::ShrugItOff,
                ..
            }
        )
    });
    game.choose_bound_v1(&chosen).unwrap();
    let after = game
        .decision_reading_v1(&[], full.context.as_ref())
        .unwrap();
    assert!(after.view.contains("Deck:") && after.view.contains("Shrug It Off"));
    assert!(!after.view.contains("Relics:") && !after.view.contains("Potions:"));
    assert!(
        game.choose_bound_v1(&chosen).is_err(),
        "context acknowledgement is not action authority"
    );
    drop(game);
    let mut reopened =
        RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join("reward"))).unwrap();
    let recovered = reopened.decision_reading_v1(&[], None).unwrap();
    assert!(
        recovered.view.contains("Deck:")
            && recovered.view.contains("Relics:")
            && recovered.view.contains("Potions:")
    );
}

#[test]
fn context_reuse_refreshes_relics_prices_and_potion_contents_independently() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "shop");
    let full = game.decision_reading_v1(&[], None).unwrap();
    game.choose_bound_v1(&pick(&full, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyRelic {
                relic: RelicId::MembershipCard,
                ..
            }
        )
    }))
    .unwrap();
    let discounted = game
        .decision_reading_v1(&[], full.context.as_ref())
        .unwrap();
    assert!(discounted
        .view
        .contains("Relics: Burning Blood, Membership Card"));
    assert!(discounted.view.contains("Power Potion 50g"));
    assert!(!discounted.view.contains("Deck:") && !discounted.view.contains("Potions:"));
    let management = game
        .decision_reading_v1(&[DecisionDetailV1::Options], None)
        .unwrap();
    game.choose_bound_v1(&pick(&management, |a| {
        matches!(a, TaskStrategicActionV1::DiscardRunPotion { slot: 0, .. })
    }))
    .unwrap();
    let discarded = game
        .decision_reading_v1(&[], discounted.context.as_ref())
        .unwrap();
    assert!(discarded.view.contains("Potions: P0 empty"));
    assert!(!discarded.view.contains("Relics:") && !discarded.view.contains("Deck:"));
}

#[test]
fn different_gold_amounts_have_distinct_claim_commands_and_remain_readable() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward_double_gold");
    let public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let first = crate::selector::parse_action_selector_v1("claim:Gold#0", "test").unwrap();
    let second = crate::selector::parse_action_selector_v1("claim:Gold#1", "test").unwrap();
    let first_choice = crate::selector::resolve_action_selector_v1(&public, &first).unwrap();
    let second_choice = crate::selector::resolve_action_selector_v1(&public, &second).unwrap();
    assert_ne!(first_choice, second_choice);
    assert_eq!(
        crate::selector::semantic_action_for_choice_v1(&public, &first_choice).unwrap(),
        "claim:Gold#0"
    );
    assert_eq!(
        crate::selector::semantic_action_for_choice_v1(&public, &second_choice).unwrap(),
        "claim:Gold#1"
    );
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let gold = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = gold.observation else {
        panic!("reward page")
    };
    let start_gold = observation.run.gold;
    game.choose_bound_v1(&RunPlayBoundChoiceV1 {
        source: reading.source.unwrap(),
        choice: second_choice,
    })
    .unwrap();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.view.contains(&format!("Gold {}", start_gold + 54)));
    assert!(reading.view.contains("11 gold"));
    assert!(!reading.view.contains("54 gold"));
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ClaimReward {
                reward: PlannerRewardDescriptor::Gold { amount: 11 },
                ..
            }
        )
    }))
    .unwrap();
    assert!(game
        .decision_reading_v1(&[], None)
        .unwrap()
        .view
        .contains(&format!("Gold {}", start_gold + 65)));
}

#[test]
fn unclaimable_reward_remains_visible_through_context_reuse_and_reward_reindexing() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "full_reward");
    let full = game.decision_reading_v1(&[], None).unwrap();
    let claim_potion = |a: &TaskStrategicActionV1| {
        matches!(
            a,
            TaskStrategicActionV1::ClaimReward {
                reward: PlannerRewardDescriptor::Potion {
                    potion: PotionId::AncientPotion
                },
                ..
            }
        )
    };
    assert!(full
        .view
        .contains("Unavailable reward: potion Ancient Potion"));
    assert!(full
        .view
        .contains("Potion slots full. Choose a use/discard command below"));
    assert!(full
        .view
        .contains("Potion choice: 1 rewards, 0 empty slots"));
    assert!(full.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::DiscardRunPotion { slot: 1, .. }
        }
    )));
    assert!(full.view.contains("Potion (base rule): Gain Artifact"));
    assert!(!full.options.iter().any(|o| matches!(&o.candidate,
        TaskPublicCandidateV1::Strategic { action } if claim_potion(action))));
    assert_eq!(full.view.matches("11 gold").count(), 1);
    assert!(!full.view.contains("Unavailable reward: card"));

    game.choose_bound_v1(&pick(&full, |a| {
        matches!(
            a,
            TaskStrategicActionV1::ClaimReward {
                reward: PlannerRewardDescriptor::Gold { .. },
                ..
            }
        )
    }))
    .unwrap();
    let shifted = game
        .decision_reading_v1(&[], full.context.as_ref())
        .unwrap();
    assert!(shifted
        .view
        .contains("Unavailable reward: potion Ancient Potion"));
    assert!(shifted.view.contains("Unchanged: Deck, Relics, Potions"));
    assert!(shifted.view.contains("Potion (base rule): Gain Artifact"));
    assert!(!shifted.view.contains("11 gold"));
    assert!(!shifted.view.contains("Unavailable reward: card"));

    game.choose_bound_v1(&pick(&shifted, |a| {
        matches!(a, TaskStrategicActionV1::DiscardRunPotion { slot: 1, .. })
    }))
    .unwrap();
    let available = game
        .decision_reading_v1(&[], shifted.context.as_ref())
        .unwrap();
    assert!(available.view.contains("P1 empty"));
    assert!(available.view.contains("Potion (base rule): Gain Artifact"));
    assert!(!available.view.contains("Unavailable reward:"));
    assert!(!available.view.contains("Potion slots full"));
    assert_eq!(available.view.matches("Ancient Potion").count(), 1);
    let claim = pick(&available, claim_potion);
    game.choose_bound_v1(&claim).unwrap();
    let taken = game
        .decision_reading_v1(&[], available.context.as_ref())
        .unwrap();
    assert!(taken.view.contains("P1 Ancient Potion"));
    assert!(!taken.view.contains("Unavailable reward:"));
    assert_eq!(taken.view.matches("Ancient Potion").count(), 1);
    assert!(game.choose_bound_v1(&claim).is_err());
}

#[test]
fn crowded_potion_rewards_offer_management_on_the_default_reading() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward_sequence_crowded");
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading
        .view
        .contains("Potion choice: 2 rewards, 1 empty slots"));
    assert!(reading.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::DiscardRunPotion { slot: 0, .. }
        }
    )));
    let mut session = fixture("full_reward");
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::Sozu));
    let mut game = controller_with_session(&dir.0, "sozu-rewards", &session);
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.view.contains("Sozu prevents obtaining potions"));
    assert!(!reading.view.contains("Potion choice:"));
}

#[test]
fn ectoplasm_reward_receipt_distinguishes_face_value_from_actual_gold_gain() {
    let dir = TestDirectory::new();
    let mut session = fixture("full_reward");
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::Ectoplasm));
    let mut game = controller_with_session(&dir.0, "ectoplasm-rewards", &session);
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.view.contains("Gold gains: prevented by Ectoplasm"));
    let claim = pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ClaimReward {
                reward: PlannerRewardDescriptor::Gold { .. },
                ..
            }
        )
    });
    let label = reading
        .options
        .iter()
        .find(|option| option.choice == claim.choice)
        .unwrap()
        .label
        .clone();
    assert!(label.contains("Gold gain prevented by Ectoplasm"));
    let gold = session.run_state.gold;
    game.choose_bound_v1(&claim).unwrap();
    assert!(game
        .decision_reading_v1(&[], None)
        .unwrap()
        .view
        .contains(&format!("Gold {gold}")));
}

#[test]
fn bonfire_selection_preserves_source_and_exposes_each_offers_executed_reward() {
    use sts_oracle_learning::state::selection::DomainEventSource;
    let cards = [
        CardId::Strike,
        CardId::ShrugItOff,
        CardId::Inflame,
        CardId::Offering,
        CardId::Bite,
        CardId::Injury,
    ];
    for (chosen_card, reward, expected_hp, expected_max_hp, modifier, gained_relic) in [
        (
            CardId::Strike,
            "basic; remove card; no additional reward",
            31,
            75,
            None,
            None,
        ),
        (
            CardId::ShrugItOff,
            "common; remove card; heal 5 HP",
            36,
            75,
            None,
            None,
        ),
        (
            CardId::Inflame,
            "uncommon; remove card; heal to full HP",
            75,
            75,
            None,
            None,
        ),
        (
            CardId::Offering,
            "rare; remove card; gain 10 Max HP",
            85,
            85,
            None,
            None,
        ),
        (
            CardId::Bite,
            "special; remove card; heal 5 HP",
            36,
            75,
            None,
            None,
        ),
        (
            CardId::Injury,
            "curse; remove card; obtain Spirit Poop",
            31,
            75,
            None,
            Some(RelicId::SpiritPoop),
        ),
        (
            CardId::Injury,
            "curse; remove card; obtain Circlet",
            31,
            75,
            Some(RelicId::SpiritPoop),
            Some(RelicId::Circlet),
        ),
        (
            CardId::Offering,
            "Mark of the Bloom blocks healing",
            31,
            85,
            Some(RelicId::MarkOfTheBloom),
            None,
        ),
    ] {
        let dir = TestDirectory::new();
        let mut session = fixture("reward");
        session.run_state.current_hp = 31;
        session.run_state.master_deck.clear();
        for card in cards {
            session.run_state.add_card_to_deck(card);
        }
        if let Some(relic) = modifier {
            session.run_state.relics.push(RelicState::new(relic));
        }
        let mut event = EventState::new(EventId::BonfireElementals);
        event.current_screen = 1;
        session.run_state.event_state = Some(event);
        session.engine_state = EngineState::EventRoom;
        let mut game = controller_with_session(&dir.0, "bonfire", &session);
        let offer = game.decision_reading_v1(&[], None).unwrap();
        assert!(
            offer.view.contains("Reward by its rarity"),
            "{}",
            offer.view
        );
        game.choose_bound_v1(&pick(&offer, |action| {
            matches!(action, TaskStrategicActionV1::ChooseEventOption { .. })
        }))
        .unwrap();
        let reading = game
            .decision_reading_v1(&[DecisionDetailV1::State], None)
            .unwrap();
        let decision = reading.state.as_ref().unwrap();
        let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
            panic!("strategic selection");
        };
        assert!(matches!(
            observation.context,
            sts_oracle_learning::ai::planner_core::PlannerDecisionContext::RunChoice {
                source: Some(DomainEventSource::Event(EventId::BonfireElementals)),
                ..
            }
        ));
        let ordinal = cards.iter().position(|card| *card == chosen_card).unwrap();
        let selected = pick(
            &reading,
            |action| matches!(action, TaskStrategicActionV1::SubmitRunSelection { selected_card_ordinals, .. } if *selected_card_ordinals == vec![ordinal]),
        );
        let candidate = &decision.candidates[selected.choice.ordinal_path[0]];
        let label = reading
            .options
            .iter()
            .find(|option| option.choice == selected.choice)
            .unwrap();
        assert!(label.label.contains(reward), "{}", label.label);
        assert!(crate::render::render_candidate_with_observation(
            candidate,
            Some(&decision.observation)
        )
        .contains(reward));
        game.choose_bound_v1(&selected).unwrap();
        let actual = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert_eq!(
            (actual.run_state.current_hp, actual.run_state.max_hp),
            (expected_hp, expected_max_hp),
            "{chosen_card:?} {modifier:?}"
        );
        assert_eq!(actual.run_state.master_deck.len(), cards.len() - 1);
        assert!(actual
            .run_state
            .master_deck
            .iter()
            .all(|card| card.id != chosen_card));
        if let Some(relic) = gained_relic {
            assert!(actual.run_state.relics.iter().any(|held| held.id == relic));
        }
    }
}

#[test]
fn event_reading_uses_public_labels_and_retains_typed_mechanics() {
    use sts_oracle_learning::content::events::neow::NeowEligibilityV1;
    let dir = TestDirectory::new();
    let paths = RunPlayPathsV1::new(dir.0.join("neow"));
    crate::driver::create_natural_session_v1(
        &paths,
        sts_oracle_learning::eval::run_control::RunControlConfig {
            seed: 2026091007,
            ascension_level: 20,
            neow_eligibility: NeowEligibilityV1::Rich,
            final_act: true,
            ..Default::default()
        },
        TaskBudgetV1 {
            max_world_transitions: 256,
            max_reloads: 0,
        },
        CombatResearchPlanOptionsV1::default(),
    )
    .unwrap();
    let mut game = RunPlayControllerV1::open(paths).unwrap();
    let reading = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    for option in &reading.options {
        if let TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    public_semantics, ..
                },
        } = &option.candidate
        {
            assert_eq!(Some(&option.label), public_semantics.public_label.as_ref());
            assert!(public_semantics.structured_semantics.is_some());
        }
    }
    assert!(reading.state.is_some());
    assert!(!reading.view.contains("mechanics=["));
}

#[test]
fn potion_events_show_delivery_before_choice_and_preserve_full_inventory() {
    use sts_oracle_learning::ai::planner_core::PlannerEventTransitionV1;

    for (event, screen, count, reward_screen) in [
        (EventId::WomanInBlue, 0, 1, true),
        (EventId::WomanInBlue, 0, 2, true),
        (EventId::WomanInBlue, 0, 3, true),
        (EventId::Lab, 0, 2, true),
        (EventId::Neow, 0, 3, true),
        (EventId::KnowingSkull, 1, 1, false),
    ] {
        let dir = TestDirectory::new();
        let mut session = RunControlSession::new(RunControlConfig {
            seed: 2,
            ascension_level: 20,
            ..RunControlConfig::default()
        });
        if event == EventId::Neow {
            session.run_state =
                sts_oracle_learning::state::run::RunState::new(2, 20, false, "Ironclad");
        }
        session.engine_state = EngineState::EventRoom;
        session.run_state.gold = 100;
        if event == EventId::Neow {
            sts_oracle_learning::content::events::neow::handle_choice(
                &mut session.engine_state,
                &mut session.run_state,
                0,
            );
        } else {
            let mut state = EventState::new(event);
            state.current_screen = screen;
            session.run_state.event_state = Some(state);
        }
        for (slot, potion) in [PotionId::SpeedPotion, PotionId::DuplicationPotion]
            .into_iter()
            .enumerate()
        {
            session.run_state.potions[slot] = Some(Potion::new(potion, slot as u32 + 1));
        }
        assert!(session.run_state.potions.iter().all(Option::is_some));
        let held = session.run_state.potions.clone();
        let mut game = controller_with_session(&dir.0, "potion-event-delivery", &session);
        let before = game.head().clone();
        let reading = game.decision_reading_v1(&[], None).unwrap();
        let selected = reading
            .options
            .iter()
            .find(|option| {
                matches!(&option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ChooseEventOption { public_semantics, .. }
                } if public_semantics.potions_gained == count)
            })
            .unwrap_or_else(|| panic!("{event:?}: {}", reading.view));
        let choice = RunPlayBoundChoiceV1 {
            source: reading.source.clone().unwrap(),
            choice: selected.choice.clone(),
        };
        let TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    public_semantics, ..
                },
        } = &selected.candidate
        else {
            panic!("expected a potion event option");
        };
        let semantics = public_semantics.structured_semantics.as_ref().unwrap();
        let expected_transition = if reward_screen {
            PlannerEventTransitionV1::OpenReward
        } else {
            PlannerEventTransitionV1::None
        };
        assert_eq!(semantics.transition, expected_transition, "{event:?}");
        let delivery = if reward_screen {
            "reveal rewards for individual inspection and optional claims"
        } else {
            "a newly generated potion is lost when inventory is full"
        };
        assert!(
            reading.view.contains(delivery),
            "{event:?}: {}",
            reading.view
        );
        assert!(
            crate::render::render_event_structured_semantics_v1(Some(semantics)).contains(delivery)
        );
        assert_eq!(
            *game.head(),
            before,
            "reading does not generate the rewards"
        );

        game.choose_bound_v1(&choice).unwrap();
        let actual = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert_eq!(actual.run_state.potions, held, "{event:?}");
        if reward_screen {
            let EngineState::RewardScreen(rewards) = actual.engine_state else {
                panic!("{event:?} must reveal its potion rewards");
            };
            assert_eq!(rewards.items.len(), count);
            assert!(rewards
                .items
                .iter()
                .all(|item| matches!(item, RewardItem::Potion { .. })));
        } else {
            assert!(matches!(actual.engine_state, EngineState::EventRoom));
        }
    }
}

#[test]
fn event_choices_show_joust_odds_from_shared_semantics_without_revealing_the_roll() {
    use sts_oracle_learning::ai::planner_core::{
        PlannerEventPublicEffectV1 as Effect, PlannerEventRandomOutcomeV1 as Random,
    };
    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    session.run_state.gold = 100;
    let mut event = EventState::new(EventId::TheJoust);
    event.current_screen = 1;
    session.run_state.event_state = Some(event);
    let mut game = controller_with_session(&dir.0, "joust-odds", &session);
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let odds = "Joust odds: Owner wins 30%; Murderer wins 70%";
    assert_eq!(reading.view.matches(odds).count(), 2);
    assert!(reading.view.contains("Gold -50..+50"));
    assert!(reading.view.contains("Gold -50..+200"));
    for option in &reading.options {
        if let TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::ChooseEventOption {
                    public_semantics, ..
                },
        } = &option.candidate
        {
            let effects = &public_semantics
                .structured_semantics
                .as_ref()
                .unwrap()
                .effects;
            assert!(effects.contains(&Effect::RandomOutcome {
                outcome: Random::JoustOwnerWin,
                chance_percent: Some(30),
            }));
            assert!(
                crate::render::render_event_public_effect_v1(effects.last().unwrap())
                    .contains(odds)
            );
        }
    }
    assert_eq!(*game.head(), before);
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseEventOption {
                option_index: 1,
                ..
            }
        )
    }))
    .unwrap();
    let pending = game.decision_reading_v1(&[], None).unwrap();
    assert!(pending.view.contains(odds));
    game.choose_bound_v1(&pick(&pending, |action| {
        matches!(action, TaskStrategicActionV1::ChooseEventOption { .. })
    }))
    .unwrap();
    let rolled = game.decision_reading_v1(&[], None).unwrap();
    assert!(rolled.view.contains("Gold +0..+250"));
    assert!(!rolled.view.contains("Owner wins!"));
    assert!(!rolled.view.contains("Murderer wins!"));
    assert!(!rolled.view.contains(odds));
}

#[test]
fn ectoplasm_event_reading_preserves_payment_and_blocks_the_actual_joust_payout() {
    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    session.run_state.gold = 100;
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::Ectoplasm));
    let mut event = EventState::new(EventId::TheJoust);
    event.current_screen = 1;
    session.run_state.event_state = Some(event);
    let mut game = controller_with_session(&dir.0, "ecto-joust", &session);
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert_eq!(
        reading
            .view
            .matches("With Ectoplasm: Gold received 0; Gold paid 50.")
            .count(),
        2
    );
    assert!(reading.view.contains("Gold -50..+200"));
    assert!(reading.view.contains("Owner wins 30%"));
    assert_eq!(*game.head(), before);
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseEventOption {
                option_index: 1,
                ..
            }
        )
    }))
    .unwrap();
    let paid = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(paid.run_state.gold, 50);
    let pending = game.decision_reading_v1(&[], None).unwrap();
    game.choose_bound_v1(&pick(&pending, |action| {
        matches!(action, TaskStrategicActionV1::ChooseEventOption { .. })
    }))
    .unwrap();
    let rolled = game.decision_reading_v1(&[], None).unwrap();
    assert!(rolled
        .view
        .contains("With Ectoplasm: Gold received 0; Gold paid 0."));
    game.choose_bound_v1(&pick(&rolled, |action| {
        matches!(action, TaskStrategicActionV1::ChooseEventOption { .. })
    }))
    .unwrap();
    let finish = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(finish.run_state.gold, 50);
    // A known winning payout exercises the blocker even if the preceding
    // random bet happened to lose.
    let mut winning = paid;
    let event = winning.run_state.event_state.as_mut().unwrap();
    event.current_screen = 3;
    event.internal_state = 3;
    for (name, blocked, expected_gold) in [("blocked-win", true, 50), ("paid-win", false, 300)] {
        let mut session = winning.clone();
        if !blocked {
            session
                .run_state
                .relics
                .retain(|relic| relic.id != RelicId::Ectoplasm);
        }
        let mut game = controller_with_session(&dir.0, name, &session);
        let reading = game.decision_reading_v1(&[], None).unwrap();
        game.choose_bound_v1(&pick(&reading, |action| {
            matches!(action, TaskStrategicActionV1::ChooseEventOption { .. })
        }))
        .unwrap();
        let actual = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert_eq!(actual.run_state.gold, expected_gold);
    }
}

#[test]
fn sozu_event_reading_predicts_claims_and_keeps_held_potions() {
    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    session.run_state.event_state = Some(EventState::new(EventId::Lab));
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::Sozu));
    session.run_state.potions[0] = Some(Potion::new(PotionId::SpeedPotion, 1));
    let held = session.run_state.potions.clone();
    let mut game = controller_with_session(&dir.0, "sozu-lab", &session);
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.view.contains("With Sozu: potions obtained 0."));
    assert_eq!(*game.head(), before);
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(action, TaskStrategicActionV1::ChooseEventOption { .. })
    }))
    .unwrap();
    loop {
        let reading = game.decision_reading_v1(&[], None).unwrap();
        if !reading.options.iter().any(|option| {
            matches!(
                option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ClaimReward {
                        reward: PlannerRewardDescriptor::Potion { .. },
                        ..
                    }
                }
            )
        }) {
            break;
        }
        game.choose_bound_v1(&pick(&reading, |action| {
            matches!(
                action,
                TaskStrategicActionV1::ClaimReward {
                    reward_item_index: 0,
                    reward: PlannerRewardDescriptor::Potion { .. },
                    ..
                }
            )
        }))
        .unwrap();
    }
    let actual = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(actual.run_state.potions, held);
}

#[test]
fn falling_reading_uses_the_selected_upgraded_card_and_removes_that_copy() {
    use sts_oracle_learning::runtime::combat::CombatCard;

    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    let mut upgraded = CombatCard::for_scenario(CardId::DarkEmbrace, 902);
    upgraded.upgrades = 1;
    session.run_state.master_deck =
        vec![CombatCard::for_scenario(CardId::DarkEmbrace, 901), upgraded];
    session.run_state.event_state = Some(EventState {
        id: EventId::Falling,
        current_screen: 1,
        internal_state: 0x3ff | (1 << 10) | (0x3ff << 20),
        ..EventState::new(EventId::Falling)
    });
    let mut game = controller_with_session(&dir.0, "falling-upgraded-removal", &session);
    let before = game.head().clone();
    for brief in [false, true] {
        let details = if brief {
            vec![DecisionDetailV1::Brief]
        } else {
            vec![]
        };
        let reading = game.decision_reading_v1(&details, None).unwrap();
        assert!(
            reading.view.contains("[Power] Remove Dark Embrace+."),
            "{}",
            reading.view
        );
        assert!(
            !reading.view.contains("remove-card:1:Dark Embrace:"),
            "{}",
            reading.view
        );
        if brief {
            assert!(reading.view.contains("1c · Power"), "{}", reading.view);
            assert!(!reading.view.contains("2c · Power"), "{}", reading.view);
        } else {
            assert!(
                reading.view.contains("Card Dark Embrace+ | Power | 1c"),
                "{}",
                reading.view
            );
        }
        assert_eq!(*game.head(), before);
    }
    let reading = game.decision_reading_v1(&[], None).unwrap();
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseEventOption {
                event: EventId::Falling,
                option_index: 1,
                ..
            }
        )
    }))
    .unwrap();
    let actual = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(actual.run_state.master_deck.len(), 1);
    assert_eq!(
        actual.run_state.master_deck[0].lineage,
        session.run_state.master_deck[0].lineage
    );
    assert_eq!(actual.run_state.master_deck[0].upgrades, 0);
}

#[test]
fn event_choices_expand_identified_item_rules_before_the_player_chooses() {
    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    session.run_state.event_state = Some(EventState::new(EventId::DrugDealer));
    let mut game = controller_with_session(&dir.0, "event-item-rules", &session);
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(
        reading.view.contains("Card J.A.X. | Skill | 0c"),
        "{}",
        reading.view
    );
    assert!(reading
        .view
        .contains("Block does not prevent the self HP loss"));
    assert!(reading.view.contains(&format!(
        "Relic {}:",
        sts_oracle_learning::content::identity::gameplay_display_label_v1(
            RelicId::MutagenicStrength
        )
    )));
    assert!(reading
        .view
        .contains("Artifact can block the removal debuff"));
    assert_eq!(reading.view.matches("Card J.A.X.").count(), 1);
    assert_eq!(*game.head(), before);
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseEventOption {
                event: EventId::DrugDealer,
                option_index: 0,
                ..
            }
        )
    }))
    .unwrap();
    let after = game.decision_reading_v1(&[], None).unwrap();
    assert!(
        !after.view.contains("Card J.A.X."),
        "Leave has no item effect"
    );

    session.run_state.event_state = Some(EventState::new(EventId::GoldenIdol));
    session
        .run_state
        .event_state
        .as_mut()
        .unwrap()
        .current_screen = 1;
    let mut game = controller_with_session(&dir.0, "event-curse-rules", &session);
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.view.contains("Card Injury | Curse | unplayable"));
    assert!(
        !reading.view.contains("Relic Golden Idol:"),
        "Prior-screen rewards are not current effects"
    );
}

#[test]
fn event_reading_shows_net_hp_cost_after_max_hp_healing() {
    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig {
        ascension_level: 20,
        ..RunControlConfig::default()
    });
    session.engine_state = EngineState::EventRoom;
    session.run_state.max_hp = 75;
    session.run_state.current_hp = 63;
    session.run_state.event_state = Some(EventState::new(EventId::ForgottenAltar));
    let mut game = controller_with_session(&dir.0, "altar-resource-cost", &session);
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.view.contains("[Pray] Gain 5 Max HP. Lose 26 HP."));
    assert!(reading
        .view
        .contains("Resources (base effect summary): HP -21; Max HP +5"));
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseEventOption {
                event: EventId::ForgottenAltar,
                option_index: 1,
                ..
            }
        )
    }))
    .unwrap();
    let after = game.decision_reading_v1(&[], None).unwrap();
    assert!(after.view.contains("HP 42/80"));
}

#[test]
fn event_default_reading_exposes_random_rewards_and_colosseum_combat_stages() {
    let dir = TestDirectory::new();
    let mut session = fixture("reward");
    session.engine_state = EngineState::EventRoom;
    let mut event = EventState::new(EventId::Colosseum);
    event.current_screen = 2;
    session.run_state.event_state = Some(event);
    let mut game = controller_with_session(&dir.0, "colosseum-effects", &session);
    let view = game.decision_reading_v1(&[], None).unwrap().view;
    assert!(view.contains("ColosseumNobs"), "{view}");
    assert!(view.contains("On victory"), "{view}");
    assert!(view.contains("obtain-relic:1:random-rare"), "{view}");
    assert!(view.contains("obtain-relic:1:random-uncommon"), "{view}");
    assert!(view.contains("gain-gold:100"), "{view}");
    assert!(view.contains("start-combat"), "{view}");
    session
        .run_state
        .event_state
        .as_mut()
        .unwrap()
        .current_screen = 1;
    let mut first = controller_with_session(&dir.0, "colosseum-first", &session);
    let view = first.decision_reading_v1(&[], None).unwrap().view;
    assert!(view.contains("ColosseumSlavers"), "{view}");
    assert!(view.contains("No item rewards"), "{view}");
    assert!(
        view.contains("settled HP and remaining resources"),
        "{view}"
    );
}

#[test]
fn brief_smith_rows_preserve_secondary_effects_and_exact_copy_choices() {
    let dir = TestDirectory::new();
    let mut session = fixture("bottled_flame");
    session.run_state.master_deck.clear();
    for card in [
        CardId::PommelStrike,
        CardId::PommelStrike,
        CardId::PommelStrike,
        CardId::PommelStrike,
        CardId::Bash,
        CardId::FlameBarrier,
        CardId::Armaments,
    ] {
        session.run_state.add_card_to_deck(card);
    }
    // Obtain the binding through the real on-equip choice, never from an index encoding.
    let mut bottle = controller_with_session(&dir.0, "bottle-copy", &session);
    let selection = bottle.decision_reading_v1(&[], None).unwrap();
    bottle
        .choose_bound_v1(&RunPlayBoundChoiceV1 {
            source: selection.source.unwrap(),
            choice: selection.options[1].choice.clone(),
        })
        .unwrap();
    let mut session = bottle
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    session.engine_state = EngineState::Campfire;
    session.run_state.master_deck[3].base_damage_override = Some(30);
    session.run_state.current_hp = 42;
    session.run_state.max_hp = 75;
    let mut game = controller_with_session(&dir.0, "smith-copy", &session);
    let head = game.head().clone();
    let brief = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    let expanded = game
        .decision_reading_v1(&[DecisionDetailV1::Brief, DecisionDetailV1::Options], None)
        .unwrap();
    let full = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    assert_eq!(brief.options, full.options);
    assert_eq!(brief.options, expanded.options);
    assert_eq!(*game.head(), head);
    assert!(
        brief.view.contains("Rest | HP 42 -> 64 (+22)"),
        "{}",
        brief.view
    );
    for (prefix, facts) in [
        (
            "Upgrade one Pommel Strike (choose 1 of 2 copies)",
            vec!["base damage/hit 9 -> 10", "draw 1 -> 2"],
        ),
        (
            "Upgrade one Pommel Strike [Bottled Flame]",
            vec!["draw 1 -> 2"],
        ),
        (
            "Upgrade one Pommel Strike (damage 30)",
            vec!["base damage/hit 30 -> 31"],
        ),
        (
            "Upgrade one Bash",
            vec!["base damage/hit 8 -> 10", "Vulnerable 2 -> 3"],
        ),
        (
            "Upgrade one Flame Barrier",
            vec!["Block 12 -> 16", "retaliation/hit 4 -> 6"],
        ),
        (
            "Upgrade one Armaments",
            vec!["combat upgrade: one card in hand -> whole hand"],
        ),
    ] {
        let row = brief
            .view
            .lines()
            .find(|line| line.contains(prefix))
            .unwrap_or_else(|| panic!("missing {prefix}: {}", brief.view));
        for fact in facts {
            assert!(row.contains(fact), "{row}");
        }
    }
    assert!(!brief.view.contains("@deck[2]"));
    assert!(!brief.view.contains("Smith upgrades:"));
    assert!(!brief.view.contains("Printed rule"));
    assert_eq!(
        expanded.view.matches("Upgrade one Pommel Strike").count(),
        4
    );
    let TaskPublicObservationV1::Strategic { observation } =
        &full.state.as_ref().unwrap().observation
    else {
        panic!("strategic observation")
    };
    assert_eq!(
        observation
            .cards
            .iter()
            .filter(|card| card.bottled_by.is_some())
            .count(),
        1
    );
    assert_eq!(observation.cards[1].bottled_by, Some(RelicId::BottledFlame));
    let ordinary = serde_json::to_value(observation.cards[0]).unwrap();
    assert!(ordinary.get("bottled_by").is_none());
    let restored: sts_oracle_learning::agent::task::TaskStrategicCardV1 =
        serde_json::from_value(ordinary).unwrap();
    assert_eq!(restored, observation.cards[0]);
    println!("{}", brief.view);
    // The representative row still executes its original exact-copy choice.
    let choice = pick(&brief, |action| {
        matches!(
            action,
            TaskStrategicActionV1::Smith {
                card_ordinal: 0,
                ..
            }
        )
    });
    game.choose_bound_v1(&choice).unwrap();
    let actual = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(actual.run_state.master_deck[0].upgrades, 1);
    assert!(actual.run_state.master_deck[1..]
        .iter()
        .all(|card| card.upgrades == 0));
    let committed = game.head().clone();
    drop(game);
    let reopened =
        RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join("smith-copy"))).unwrap();
    assert_eq!(*reopened.head(), committed);
}

#[test]
fn brief_smith_keeps_the_rule_for_an_unnamed_upgrade_effect() {
    let dir = TestDirectory::new();
    let mut session = fixture("reward");
    session.engine_state = EngineState::Campfire;
    session.run_state.master_deck.clear();
    session.run_state.add_card_to_deck(CardId::DoubleTap);
    let mut game = controller_with_session(&dir.0, "smith-rule-fallback", &session);
    let reading = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    let row = reading
        .view
        .lines()
        .find(|row| row.contains("Upgrade one Double Tap"))
        .unwrap();
    assert!(row.contains("Rule:"), "{row}");
    assert!(
        row.contains(
            sts_oracle_learning::content::cards::mechanics::explanation(CardId::DoubleTap).unwrap()
        ),
        "{row}"
    );
    assert!(!row.contains("cost 1c -> 1c"), "{row}");
}

#[test]
fn smith_reading_compares_actual_instance_upgrades_and_preserves_each_choice() {
    use sts_oracle_learning::content::cards::mechanics;
    use sts_oracle_learning::runtime::combat::CardBlueprint;
    for (card, upgrades, modifier, fact, expected_cost) in [
        (CardId::DemonForm, 0, 0, "Strength/turn 2 -> 3", 3),
        (CardId::Corruption, 0, 0, "cost 3c -> 2c", 2),
        (CardId::Corruption, 0, -2, "cost 1c -> 2c", 2),
        (CardId::DarkEmbrace, 0, 0, "cost 2c -> 1c", 1),
        (CardId::Defend, 0, 0, "Block 5 -> 8", 1),
        (CardId::FlameBarrier, 0, 0, "Block 12 -> 16", 2),
        (CardId::Carnage, 0, 0, "base damage/hit 20 -> 28", 2),
        (CardId::PommelStrike, 0, 0, "draw 1 -> 2", 1),
        (
            CardId::TrueGrit,
            0,
            0,
            "Exhaust target: random -> chosen",
            1,
        ),
        (CardId::LimitBreak, 0, 0, "Exhaust on play true -> false", 1),
        (CardId::Apparition, 0, 0, "Ethereal true -> false", 1),
        (CardId::SearingBlow, 3, 0, "base damage/hit 27 -> 34", 2),
        (CardId::Whirlwind, 0, 0, "base damage/hit 5 -> 8", -1),
    ] {
        let dir = TestDirectory::new();
        let mut session = fixture("reward");
        session.engine_state = EngineState::Campfire;
        session.run_state.master_deck.clear();
        for _ in 0..2 {
            session
                .run_state
                .add_card_to_deck_with_upgrades(card, upgrades);
        }
        for instance in &mut session.run_state.master_deck {
            instance.cost_modifier = modifier;
        }
        let mut game = controller_with_session(&dir.0, "smith-comparison", &session);
        let before = game.decision_reading_v1(&[], None).unwrap();
        assert!(before.view.contains(fact), "{card:?}: {}", before.view);
        assert!(before.view.contains(mechanics::explanation(card).unwrap()));
        assert_eq!(
            before
                .view
                .matches("    Printed rule (base/upgraded):")
                .count(),
            1
        );
        let smiths: Vec<_> = before
            .options
            .iter()
            .filter(|option| {
                matches!(
                    &option.candidate,
                    TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::Smith { .. }
                    }
                )
            })
            .collect();
        assert_eq!(smiths.len(), 2);
        assert_ne!(smiths[0].choice, smiths[1].choice);
        let unchanged = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert!(unchanged
            .run_state
            .master_deck
            .iter()
            .all(|instance| instance.upgrades == upgrades));
        game.choose_bound_v1(&RunPlayBoundChoiceV1 {
            source: before.source.unwrap(),
            choice: smiths[0].choice.clone(),
        })
        .unwrap();
        let after = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        let changed = &after.run_state.master_deck[0];
        assert_eq!(changed.upgrades, upgrades + 1);
        assert_eq!(
            changed.combat_cost_without_turn_override_java(),
            expected_cost
        );
        assert_eq!(after.run_state.master_deck[1].upgrades, upgrades);
        let old = CardBlueprint::from_stat_equivalent(&unchanged.run_state.master_deck[0]);
        let new = CardBlueprint::from_stat_equivalent(changed);
        let a = mechanics::card_blueprint_persistent_base_numbers_v1(&old);
        let b = mechanics::card_blueprint_persistent_base_numbers_v1(&new);
        for (name, a, b) in [
            ("base damage/hit", a.damage, b.damage),
            ("Block", a.block, b.block),
        ] {
            if a != b {
                assert!(before.view.contains(&format!("{name} {a} -> {b}")));
            }
        }
    }
}

#[test]
fn event_upgrade_reading_compares_instances_for_single_and_multiple_selections() {
    use sts_oracle_learning::state::core::{RunPendingChoiceReason, RunPendingChoiceState};
    for count in [1, 2] {
        let dir = TestDirectory::new();
        let mut session = fixture("reward");
        session.run_state.master_deck.clear();
        for card in [CardId::Apotheosis, CardId::Corruption, CardId::TrueGrit] {
            session.run_state.add_card_to_deck(card);
        }
        session.run_state.master_deck[1].cost_modifier = -2;
        let mut event = EventState::new(EventId::UpgradeShrine);
        event.current_screen = 1;
        session.run_state.event_state = Some(event);
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            reason: RunPendingChoiceReason::Upgrade,
            source: DomainEventSource::Event(EventId::UpgradeShrine),
            min_choices: count,
            max_choices: count,
            return_state: Box::new(EngineState::EventRoom),
        });
        let mut game = controller_with_session(&dir.0, "event-upgrade", &session);
        let before_head = game.head().clone();
        let reading = game
            .decision_reading_v1(&[DecisionDetailV1::Brief], None)
            .unwrap();
        for fact in [
            "cost 2c -> 1c",
            "cost 1c -> 2c",
            "Exhaust target: random -> chosen",
        ] {
            assert!(reading.view.contains(fact), "{}", reading.view);
        }
        assert_eq!(*game.head(), before_head, "reading must not upgrade cards");
        let choice = if count == 1 {
            reading.options.iter().find(|option| matches!(
                &option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::SubmitRunSelection { selected_card_ordinals, .. }
                } if selected_card_ordinals == &[1]
            )).unwrap().choice.clone()
        } else {
            let mut choice = reading.selections[0].choice.clone();
            choice.ordinal_path.extend([1, 0]);
            choice
        };
        game.choose_bound_v1(&RunPlayBoundChoiceV1 {
            source: reading.source.unwrap(),
            choice,
        })
        .unwrap();
        let actual = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert_eq!(
            actual.run_state.master_deck[1].combat_cost_without_turn_override_java(),
            2
        );
        assert_eq!(actual.run_state.master_deck[1].upgrades, 1);
        assert_eq!(
            actual.run_state.master_deck[0].upgrades,
            u8::from(count == 2)
        );
        assert_eq!(actual.run_state.master_deck[2].upgrades, 0);
    }
}

#[test]
fn late_heart_key_choices_show_missing_keys_without_early_or_collected_tutorials() {
    let dir = TestDirectory::new();
    for (act, keys, expected) in [
        (1, [false; 3], None),
        (
            3,
            [false; 3],
            Some("Heart keys still needed: Ruby, Sapphire, Emerald"),
        ),
        (
            3,
            [true, false, true],
            Some("Heart keys still needed: Sapphire"),
        ),
        (3, [true; 3], None),
    ] {
        let mut session = fixture("route");
        session.run_state.act_num = act;
        session.run_state.keys = keys;
        let mut game =
            controller_with_session(&dir.0, &format!("late-keys-{act}-{keys:?}"), &session);
        let before = game.head().clone();
        let reading = game
            .decision_reading_v1(&[DecisionDetailV1::Brief], None)
            .unwrap();
        match expected {
            Some(message) => assert!(reading.view.contains(message), "{}", reading.view),
            None => assert!(
                !reading.view.contains("Heart keys still needed:"),
                "{}",
                reading.view
            ),
        }
        assert!(!reading.view.contains("Missing keys:"));
        assert_eq!(*game.head(), before);
    }
    let mut session = fixture("route");
    session.run_state.act_num = 3;
    session.run_state.map.graph[0][0].class = Some(RoomType::RestRoom);
    let mut game = controller_with_session(&dir.0, "recall-ruby-effect", &session);
    let reading = game.decision_reading_v1(&[], None).unwrap();
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseRouteNode { x: 0, y: 0, .. }
        )
    }))
    .unwrap();
    let camp = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    assert!(camp
        .view
        .contains("Gain the Ruby Key instead of another campfire action."));
    game.choose_bound_v1(&pick(&camp, |action| {
        matches!(action, TaskStrategicActionV1::Recall)
    }))
    .unwrap();
    let actual = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(actual.run_state.keys, [true, false, false]);
    let next = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    assert!(next
        .view
        .contains("Heart keys still needed: Sapphire, Emerald"));
    assert!(!next.view.contains("still needed: Ruby"));
}

#[test]
fn key_status_is_available_in_combat_readings_and_bound_mechanics_queries() {
    for (keys, status, missing) in [
        (
            [false; 3],
            "Keys: none",
            Some("Missing keys: Ruby: Recall at campfire"),
        ),
        (
            [false, true, true],
            "Keys: Sapphire, Emerald",
            Some("Missing keys: Ruby: Recall at campfire"),
        ),
        ([true; 3], "Keys: Ruby, Sapphire, Emerald", None),
    ] {
        for kind in ["route", "combat_multiturn"] {
            let dir = TestDirectory::new();
            let mut session = fixture(kind);
            session.run_state.keys = keys;
            let mut game = controller_with_session(&dir.0, "key-status", &session);
            let head = game.head().clone();
            let reading = game
                .decision_reading_v1(&[DecisionDetailV1::State], None)
                .unwrap();
            let decision = reading.state.as_ref().unwrap();
            let info =
                crate::mechanics_info::render(Some(decision), "keys", sts_presentation::Locale::En)
                    .unwrap();
            for view in [&reading.view, &info] {
                assert!(view.contains(status), "{kind}: {view}");
                if let Some(missing) = missing {
                    assert!(view.contains(missing), "{kind}: {view}");
                } else {
                    assert!(!view.contains("Missing keys:"), "{kind}: {view}");
                }
            }
            assert_eq!(game.head(), &head, "key queries preserve the actual game");

            if kind == "combat_multiturn" {
                use sts_oracle_learning::agent::information::run::{
                    PublicCombatRunContextGapV1, PublicCombatRunContextV1,
                };
                let mut detached = decision.clone();
                let TaskPublicObservationV1::Combat { run_context, .. } = &mut detached.observation
                else {
                    unreachable!()
                };
                *run_context = PublicCombatRunContextV1::Unavailable {
                    reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
                };
                let info = crate::mechanics_info::render(
                    Some(&detached),
                    "keys",
                    sts_presentation::Locale::En,
                )
                .unwrap();
                assert!(info.contains("Current key status unavailable"));
                assert!(
                    !info.contains("Keys:"),
                    "a detached combat does not imply no keys"
                );
            }
        }
    }
}

#[test]
fn potion_info_locales_change_only_text_and_reject_unsupported_languages() {
    use crate::agent_protocol::{
        run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1,
    };
    let directory = TestDirectory::new();
    let mut game = controller(&directory.0, "combat_multiturn");
    let before = game.head().clone();
    let checkpoint = game.current_external_exact_checkpoint_v1().unwrap();
    let input = [Some("en"), Some("zh-CN"), None, Some("fr")].into_iter().enumerate().map(|(i, locale)| {
        let mut request = serde_json::json!({"schema":"sts-run-play-agent-request-v1", "request_id":format!("locale-{i}"), "command":"info", "query":"potion SteroidPotion"});
        if let Some(locale) = locale { request["locale"] = locale.into(); }
        format!("{request}\n")
    }).collect::<String>();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::public_manual_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let responses: Vec<serde_json::Value> = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(responses[0]["result"]["view"]
        .as_str()
        .unwrap()
        .contains("Flex Potion | ID: SteroidPotion"));
    assert!(responses[1]["result"]["view"]
        .as_str()
        .unwrap()
        .contains("类固醇药水 | ID：SteroidPotion"));
    assert_eq!(
        responses[0]["result"]["view"],
        responses[2]["result"]["view"]
    );
    assert_eq!(responses[3]["status"], "error");
    assert_eq!(game.head(), &before);
    assert_eq!(
        serde_json::to_value(game.current_external_exact_checkpoint_v1().unwrap()).unwrap(),
        serde_json::to_value(checkpoint).unwrap()
    );
}

#[test]
fn map_reading_distinguishes_room_actions_completed_nodes_and_overlays() {
    let dir = TestDirectory::new();
    let mut session = fixture("route");
    session.run_state.map.graph[0][0].class = Some(RoomType::RestRoom);
    let mut game = controller_with_session(&dir.0, "map-room-stage", &session);

    let entrance = game.decision_reading_v1(&[], None).unwrap();
    assert!(entrance.view.contains("| Map · choose next room"));
    assert!(!entrance.view.contains("completed node"));
    assert!(!entrance.view.contains("current node"));
    game.choose_bound_v1(&pick(&entrance, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseRouteNode { x: 0, y: 0, .. }
        )
    }))
    .unwrap();
    let campfire = game.decision_reading_v1(&[], None).unwrap();
    assert!(campfire.view.contains("| Campfire"));
    assert!(campfire.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Rest
        }
    )));

    // A map overlay can be opened before the room action has been taken.
    // Its Cancel returns to that screen; its node icon is not completion proof.
    let mut overlay_session = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    overlay_session.engine_state = EngineState::map_overlay(EngineState::Campfire);
    let mut overlay_game = controller_with_session(&dir.0, "map-room-overlay", &overlay_session);
    let overlay = overlay_game.decision_reading_v1(&[], None).unwrap();
    assert!(overlay.view.contains("| Map overlay · choose next room, or cancel to return to the room screen · current node (0,0) Campfire"));
    assert!(!overlay.view.contains("completed node"));
    overlay_game
        .choose_bound_v1(&pick(&overlay, |action| {
            matches!(action, TaskStrategicActionV1::Cancel { .. })
        }))
        .unwrap();
    let returned = overlay_game.decision_reading_v1(&[], None).unwrap();
    assert!(returned.view.contains("| Campfire"));
    assert!(returned.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Rest
        }
    )));

    game.choose_bound_v1(&pick(&campfire, |action| {
        matches!(action, TaskStrategicActionV1::Rest)
    }))
    .unwrap();
    let completed = game.decision_reading_v1(&[], None).unwrap();
    assert!(completed
        .view
        .contains("| Map · choose next room · completed node (0,0) Campfire"));
    assert!(completed.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::ChooseRouteNode { .. }
        }
    )));
    assert!(!completed.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Rest
        }
    )));
}

#[test]
fn campfire_rest_reading_matches_actual_healing_and_relic_modifiers() {
    for (name, hp, relic, expected) in [
        ("rest-max", 14, None, 38),
        ("rest-cap", 74, None, 80),
        ("rest-pillow", 14, Some(RelicId::RegalPillow), 53),
        ("rest-bloom", 14, Some(RelicId::MarkOfTheBloom), 14),
    ] {
        let dir = TestDirectory::new();
        let mut session = fixture("reward");
        session.engine_state = EngineState::Campfire;
        session.run_state.current_hp = hp;
        session.run_state.max_hp = 80;
        if let Some(relic) = relic {
            session.run_state.relics.push(RelicState::new(relic));
        }
        let mut game = controller_with_session(&dir.0, name, &session);
        let before = game.decision_reading_v1(&[], None).unwrap();
        assert!(
            before.view.contains(&format!(
                "Rest | HP {} -> {} (+{})",
                hp,
                expected,
                expected - hp
            )),
            "{}",
            before.view
        );
        game.choose_bound_v1(&pick(&before, |action| {
            matches!(action, TaskStrategicActionV1::Rest)
        }))
        .unwrap();
        let after = game.decision_reading_v1(&[], None).unwrap();
        assert!(
            after.view.contains(&format!("HP {expected}/80")),
            "{}",
            after.view
        );
    }
}

#[test]
fn event_reading_distinguishes_repeated_continue_screens() {
    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    session.run_state.event_state = Some(EventState::new(EventId::MaskedBandits));
    let mut game = controller_with_session(&dir.0, "masked-bandits-dialogue", &session);

    for screen in 0..=3 {
        let reading = game.decision_reading_v1(&[], None).unwrap();
        assert!(reading
            .view
            .contains(&format!("Masked Bandits | Event screen {screen}")));
        if screen > 0 {
            assert_eq!(reading.options[0].label, "[Continue]");
        }
        game.choose_bound_v1(&pick(&reading, |action| {
            matches!(
                action,
                TaskStrategicActionV1::ChooseEventOption {
                    event: EventId::MaskedBandits,
                    screen: current,
                    option_index: 0,
                    ..
                } if *current == screen
            )
        }))
        .unwrap();
    }

    let finished = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    assert!(finished.view.contains("| Map"));
    assert!(!finished.view.contains("Event screen"));
    let Some(sts_oracle_learning::agent::task::TaskPublicDecisionV1 {
        observation: TaskPublicObservationV1::Strategic { observation },
        ..
    }) = finished.state
    else {
        panic!("expected map decision after the final continue");
    };
    assert!(matches!(
        observation.context,
        PlannerDecisionContext::Map { .. }
    ));
}

#[test]
fn face_trader_result_reading_keeps_the_selected_branch() {
    let dir = TestDirectory::new();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::EventRoom;
    let mut event = EventState::new(EventId::FaceTrader);
    event.current_screen = 1;
    session.run_state.event_state = Some(event);
    let mut game = controller_with_session(&dir.0, "face-trader-result", &session);

    let choice = game.decision_reading_v1(&[], None).unwrap();
    assert!(choice.view.contains("Scene: "));
    game.choose_bound_v1(&pick(&choice, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ChooseEventOption {
                event: EventId::FaceTrader,
                screen: 1,
                option_index: 2,
                ..
            }
        )
    }))
    .unwrap();

    let result = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    let Some(sts_oracle_learning::agent::task::TaskPublicDecisionV1 {
        observation: TaskPublicObservationV1::Strategic { observation },
        ..
    }) = result.state
    else {
        panic!("expected strategic result reading");
    };
    assert!(matches!(
        observation.context,
        PlannerDecisionContext::Event {
            event: EventId::FaceTrader,
            screen: 2,
            face_trader_result: Some(FaceTraderResult::Leave),
        }
    ));
    assert!(result.view.contains("Scene: "));
    assert!(!result.view.contains("result choice unavailable"));
}

#[test]
fn reward_choice_is_executed_once_and_survives_reopen_without_advice() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward");
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let command = pick(&reading, |a| {
        matches!(
            a,
            TaskStrategicActionV1::TakeCard {
                card: CardId::ShrugItOff,
                ..
            }
        )
    });
    game.choose_bound_v1(&command).unwrap();
    let committed = game.head().clone();
    assert!(matches!(
        committed.events.last(),
        Some(RunPlayEventV1::Choice {
            strategic_owner_advice: None,
            ..
        })
    ));
    let after = game.decision_reading_v1(&[], None).unwrap();
    assert!(after.view.contains("Shrug It Off"));
    assert_ne!(after.source, reading.source);
    assert_eq!(
        after.last_choice.as_ref().unwrap().choices,
        vec![command.choice.clone()]
    );
    assert_eq!(
        after.last_choice.as_ref().unwrap().from_archive_node_id,
        command.source.archive_node_id
    );
    assert!(game
        .choose_bound_v1(&command)
        .unwrap_err()
        .contains("Decision changed"));
    assert_eq!(game.head(), &committed);
    drop(game);
    let mut reopened =
        RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join("reward"))).unwrap();
    assert!(reopened.choose_bound_v1(&command).is_err());
    assert_eq!(reopened.decision_reading_v1(&[], None).unwrap(), after);
    reopened.back(1).unwrap();
    assert!(
        reopened.choose_bound_v1(&command).is_err(),
        "returning to same board must not revive old replies"
    );
}

#[test]
fn displayed_choices_are_dense_and_keep_original_execution_paths() {
    let dir = TestDirectory::new();
    for kind in ["reward_potions", "full_reward", "shop", "shop_courier"] {
        let mut game = controller(&dir.0, kind);
        let reading = game
            .decision_reading_v1(&[DecisionDetailV1::State], None)
            .unwrap();
        let expanded = game
            .decision_reading_v1(&[DecisionDetailV1::Options], None)
            .unwrap();
        if kind == "full_reward" {
            assert_eq!(expanded.options, reading.options);
        } else {
            assert!(expanded.options.len() > reading.options.len());
        }
        assert_eq!(&expanded.options[..reading.options.len()], &reading.options);
        assert_eq!(expanded.source, reading.source);
        for option in &reading.options {
            let command = crate::selector::semantic_action_for_choice_v1(
                reading.state.as_ref().unwrap(),
                &option.choice,
            )
            .unwrap();
            assert!(reading
                .view
                .contains(&format!("./p {command}  |  Effect: {}", option.label)));
            assert!(
                kind == "full_reward"
                    || !matches!(
                        option.candidate,
                        TaskPublicCandidateV1::Strategic {
                            action: TaskStrategicActionV1::DiscardRunPotion { .. }
                                | TaskStrategicActionV1::UseRunPotion { .. }
                                | TaskStrategicActionV1::RemoveCard { .. }
                        }
                    )
            );
            assert_eq!(
                reading.state.as_ref().unwrap().candidates[option.choice.ordinal_path[0]],
                option.candidate
            );
        }
        if kind == "reward_potions" {
            // Potion management precedes card choices in the engine surface.
            // Display option zero must still take the card, not discard a potion.
            let first = &reading.options[0];
            assert_ne!(first.choice.ordinal_path, vec![0]);
            assert!(matches!(
                first.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::TakeCard {
                        card: CardId::ShrugItOff,
                        ..
                    }
                }
            ));
            game.choose_bound_v1(&RunPlayBoundChoiceV1 {
                source: reading.source.unwrap(),
                choice: first.choice.clone(),
            })
            .unwrap();
            let after = game.decision_reading_v1(&[], None).unwrap();
            assert!(after.view.contains("Shrug It Off"));
            assert_eq!(
                after.context.unwrap().potions,
                reading.context.unwrap().potions
            );
        }
    }
}

#[test]
fn reward_progress_distinguishes_empty_loot_from_a_full_slot_potion() {
    let dir = TestDirectory::new();
    for empty in [false, true] {
        let mut session = fixture("full_reward");
        if empty {
            session.engine_state = EngineState::RewardScreen(RewardState::new());
        }
        let mut game = controller_with_session(
            &dir.0,
            if empty {
                "empty-rewards"
            } else {
                "unclaimed-rewards"
            },
            &session,
        );
        let before = game.head().clone();
        let brief = game
            .decision_reading_v1(&[DecisionDetailV1::Brief], None)
            .unwrap();
        let expanded = game
            .decision_reading_v1(&[DecisionDetailV1::Brief, DecisionDetailV1::Options], None)
            .unwrap();
        assert!(brief.view.contains("leave rewards"), "{}", brief.view);
        assert!(!brief.view.contains("proceed Reward"));
        assert_eq!(brief.source, expanded.source);
        if empty {
            assert!(brief.view.contains("Rewards complete"));
            assert!(brief
                .view
                .contains("Other commands in options: discard held potion x2"));
            assert!(!brief.view.contains("Unavailable reward"));
            assert!(!expanded.view.contains("Other commands in options"));
            assert_eq!(brief.options.len(), 1);
            assert_eq!(expanded.options.len(), 3);
        } else {
            assert!(brief.view.contains("Rewards (3 remaining)"));
            assert!(!brief.view.contains("Rewards complete"));
            assert!(brief
                .view
                .contains("Unavailable reward: potion Ancient Potion"));
            assert!(brief.view.contains("Potion slots full"));
            assert!(brief.view.contains("discard potion[0] Fire Potion"));
            assert_eq!(brief.options, expanded.options);
        }
        assert_eq!(*game.head(), before, "reading must not settle rewards");
    }
}

#[test]
fn out_of_combat_potion_use_is_secondary_and_retains_its_bound_choice() {
    let dir = TestDirectory::new();
    let mut session = fixture("reward");
    session.run_state.potions[0] = Some(Potion::new(PotionId::BloodPotion, 0));
    let mut game = controller_with_session(&dir.0, "blood-reward", &session);
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let expanded = game
        .decision_reading_v1(&[DecisionDetailV1::Options], None)
        .unwrap();
    assert!(!reading.view.lines().any(
        |line| line.starts_with(|c: char| c.is_ascii_digit()) && line.contains(": use potion")
    ));
    assert!(reading.view.contains("P0 Blood Potion"));
    assert!(
        reading
            .view
            .contains("Other commands in options: use held potion x1, discard held potion x1"),
        "{}",
        reading.view
    );
    assert_eq!(&expanded.options[..reading.options.len()], &reading.options);
    let potion = pick(&expanded, |a| {
        matches!(a, TaskStrategicActionV1::UseRunPotion { .. })
    });
    game.choose_bound_v1(&potion).unwrap();
    let after = game.decision_reading_v1(&[], None).unwrap();
    assert!(after.view.contains("P0 empty"));
    assert_eq!(after.options[0].label, reading.options[0].label);
    assert!(game.choose_bound_v1(&potion).is_err());
}

#[test]
fn full_shop_exposes_inventory_and_reprices_before_explicit_replacement() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "shop");
    let initial = game.decision_reading_v1(&[], None).unwrap();
    assert_eq!(initial.control, DecisionControlV1::Strategic);
    for item in [
        "Shrug It Off 60g",
        "Pommel Strike 80g",
        "Membership Card 150g",
        "Power Potion 100g",
    ] {
        assert_eq!(initial.view.matches(item).count(), 1, "one line per offer");
    }
    assert!(initial.view.contains("Power Potion 100g (slots full)"));
    let purchasable = initial
        .view
        .lines()
        .find(|line| line.starts_with("Purchasable now:"))
        .unwrap();
    assert!(purchasable.contains("Membership Card (150g)"));
    assert!(purchasable.contains("Shrug It Off (60g)"));
    assert!(!purchasable.contains("Power Potion"));
    assert_eq!(initial.view.matches("Potion replacement:").count(), 1);
    assert!(!initial.view.contains("Permanent-card acquisition:"));
    assert!(initial
        .view
        .contains("Discard the bottle you choose to replace, then read the updated offers"));
    assert!(initial.view.contains("Power Potion 100g"));
    assert!(!initial.options.iter().any(|o| matches!(
        o.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::BuyPotion { .. }
        }
    )));
    assert!(!initial
        .view
        .lines()
        .any(|line| line.starts_with(|c: char| c.is_ascii_digit())
            && line.contains(": discard potion")));
    let before_inspect = game.head().clone();
    let expanded = game
        .decision_reading_v1(&[DecisionDetailV1::Options], None)
        .unwrap();
    assert!(expanded.view.contains("discard potion"));
    assert_eq!(&expanded.options[..initial.options.len()], &initial.options);
    assert!(expanded.options.len() > initial.options.len());
    assert_eq!(expanded.source, initial.source);
    assert_eq!(expanded.view.matches("Shrug It Off 60g").count(), 1);
    assert_eq!(game.head(), &before_inspect);
    let stale_card = pick(&initial, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyCard {
                card: CardId::ShrugItOff,
                ..
            }
        )
    });
    game.choose_bound_v1(&pick(&initial, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyRelic {
                relic: RelicId::MembershipCard,
                ..
            }
        )
    }))
    .unwrap();
    let discounted = game.decision_reading_v1(&[], None).unwrap();
    let purchasable = discounted
        .view
        .lines()
        .find(|line| line.starts_with("Purchasable now:"))
        .unwrap();
    assert!(purchasable.contains("Shrug It Off (30g)"));
    assert!(!purchasable.contains("Membership Card"));
    assert_eq!(discounted.view.matches("Shrug It Off 30g").count(), 1);
    assert!(discounted.view.contains("Power Potion 50g"));
    assert!(game.choose_bound_v1(&stale_card).is_err());
    let management = game
        .decision_reading_v1(&[DecisionDetailV1::Options], None)
        .unwrap();
    game.choose_bound_v1(&pick(&management, |a| {
        matches!(a, TaskStrategicActionV1::DiscardRunPotion { slot: 0, .. })
    }))
    .unwrap();
    let empty_slot = game.decision_reading_v1(&[], None).unwrap();
    assert_eq!(empty_slot.view.matches("Power Potion 50g").count(), 1);
    assert!(empty_slot.view.contains("buy Power Potion 50g"));
    assert!(empty_slot.view.contains("P0 empty"));
    assert!(!empty_slot.view.contains("Potion replacement:"));
    assert!(empty_slot
        .view
        .lines()
        .find(|line| line.starts_with("Purchasable now:"))
        .unwrap()
        .contains("Power Potion (50g)"));
    let potion = pick(&empty_slot, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyPotion {
                potion: PotionId::PowerPotion,
                price: 50,
                ..
            }
        )
    });
    game.choose_bound_v1(&potion).unwrap();
    let bought = game.decision_reading_v1(&[], None).unwrap();
    assert!(bought.view.contains("P0 Power Potion"));
    assert!(bought.view.contains("Gold 250"));
    assert!(!bought.view.contains("Power Potion 50g"));
    assert!(!bought
        .view
        .lines()
        .find(|line| line.starts_with("Purchasable now:"))
        .unwrap()
        .contains("Power Potion"));
    assert!(game.choose_bound_v1(&potion).is_err());
    let before_shift = pick(&bought, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyCard {
                card: CardId::PommelStrike,
                ..
            }
        )
    });
    game.choose_bound_v1(&pick(&bought, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyCard {
                card: CardId::ShrugItOff,
                ..
            }
        )
    }))
    .unwrap();
    assert!(game.choose_bound_v1(&before_shift).is_err());
    let shifted = game.decision_reading_v1(&[], None).unwrap();
    let pommel = pick(&shifted, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyCard {
                card: CardId::PommelStrike,
                shop_slot: 0,
                ..
            }
        )
    });
    game.choose_bound_v1(&pommel).unwrap();
    assert!(game
        .decision_reading_v1(&[], None)
        .unwrap()
        .view
        .contains("Gold 180"));
}

#[test]
fn grouped_shop_preserves_identical_offers_and_blocked_stock() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "shop_twins");
    let before = game.decision_reading_v1(&[], None).unwrap();
    let first = pick(&before, |a| {
        matches!(a, TaskStrategicActionV1::BuyCard { shop_slot: 0, .. })
    });
    let second = pick(&before, |a| {
        matches!(a, TaskStrategicActionV1::BuyCard { shop_slot: 1, .. })
    });
    assert_ne!(first.choice, second.choice);
    for choice in [&first, &second] {
        let decision = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let command =
            crate::selector::semantic_action_for_choice_v1(&decision, &choice.choice).unwrap();
        assert!(before
            .view
            .contains(&format!("./p {command}  |  Effect: buy Shrug It Off 60g")));
    }
    assert_eq!(before.view.matches("buy Shrug It Off 60g").count(), 2);
    game.choose_bound_v1(&first).unwrap();
    assert!(game.choose_bound_v1(&second).is_err());
    let shifted = game
        .decision_reading_v1(&[], before.context.as_ref())
        .unwrap();
    assert_eq!(shifted.view.matches("buy Shrug It Off 60g").count(), 1);
    game.choose_bound_v1(&pick(&shifted, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyCard {
                shop_slot: 0,
                card: CardId::ShrugItOff,
                ..
            }
        )
    }))
    .unwrap();

    let mut poor = controller(&dir.0, "shop_poor");
    let poor = poor.decision_reading_v1(&[], None).unwrap();
    for item in [
        "Shrug It Off 60g",
        "Pommel Strike 80g",
        "Membership Card 150g",
        "Power Potion 100g",
    ] {
        assert_eq!(poor.view.matches(item).count(), 1);
    }
    assert!(poor.view.contains("Shrug It Off 60g (insufficient gold)"));
    assert!(!poor.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::BuyCard { .. }
                | TaskStrategicActionV1::BuyRelic { .. }
                | TaskStrategicActionV1::BuyPotion { .. }
        }
    )));
}

#[test]
fn choice_readings_expose_typed_items_and_rules_without_changing_choices() {
    let dir = TestDirectory::new();
    let mut session = fixture("shop");
    session.run_state.potions = vec![None, None];
    let EngineState::Shop(shop) = &mut session.engine_state else {
        unreachable!();
    };
    shop.cards[0].card_id = CardId::Corruption;
    shop.cards[0].upgrades = 1;
    shop.relics[0].relic_id = RelicId::Sling;
    shop.potions[0].potion_id = PotionId::HeartOfIron;
    let mut game = controller_with_session(&dir.0, "typed-shop-facts", &session);
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading
        .view
        .contains("buy Corruption+ 60g\n  Card: Power | 2c"));
    assert!(reading.view.contains("buy Sling of Courage 150g\n  Relic:"));
    assert!(reading.view.contains("elite combats only"));
    assert!(reading.view.contains("including the Heart"));
    assert!(reading
        .view
        .contains("buy Heart of Iron 100g\n  Potion (base rule): Gain Metallicize"));
    assert_eq!(
        reading
            .options
            .iter()
            .filter(|option| matches!(
                option.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::BuyPotion {
                        potion: PotionId::HeartOfIron,
                        ..
                    }
                }
            ))
            .count(),
        1
    );
    assert_eq!(*game.head(), before);

    let mut session = fixture("reward");
    let EngineState::RewardScreen(reward) = &mut session.engine_state else {
        unreachable!();
    };
    let cards = vec![
        RewardCard::new(CardId::Uppercut, 1),
        RewardCard::new(CardId::Anger, 0),
        RewardCard::new(CardId::BloodForBlood, 1),
        RewardCard::new(CardId::ShrugItOff, 1),
    ];
    reward.items[0] = RewardItem::Card {
        cards: cards.clone(),
    };
    reward.pending_card_choice = Some(cards);
    session.run_state.add_card_to_deck(CardId::Corruption);
    session.run_state.add_card_to_deck(CardId::DarkEmbrace);
    session.run_state.add_card_to_deck(CardId::Corruption);
    session.run_state.add_card_to_deck(CardId::Anger);
    session.run_state.add_card_to_deck(CardId::Anger);
    session.run_state.add_card_to_deck(CardId::BodySlam);
    let mut game = controller_with_session(&dir.0, "typed-reward-facts", &session);
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading
        .view
        .contains("take Uppercut+\n  Card: Attack | 2c | tags: none"));
    assert!(reading
        .view
        .contains(sts_oracle_learning::content::cards::ironclad::uppercut::MECHANICS));
    assert!(reading
        .view
        .contains("take Anger\n  Card: Attack | 0c | tags: none"));
    assert!(reading.view.contains("take Blood for Blood+\n  Card: Attack | 3c | tags: none\n  Printed base stats at this upgrade: damage 22"));
    assert!(reading.view.contains("take Shrug It Off+\n  Card: Skill | 1c | tags: none\n  Printed base stats at this upgrade: Block 11"));
    assert!(!reading.view.contains("Owned deck rules"));
    let expanded = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    assert!(expanded
        .view
        .contains("Owned deck rules (deduplicated by card and upgrade; Power effects apply after deployment):"));
    assert_eq!(expanded.view.matches("  Card Anger | Attack").count(), 1);
    assert!(expanded.view.contains("  Card Body Slam | Attack"));
    assert!(reading
        .view
        .contains("add a copy of this card to your discard pile"));
    assert!(expanded.view.contains("current Block"));
    assert_eq!(
        expanded.view.matches("  Card Corruption | Power").count(),
        1
    );
    assert!(expanded
        .view
        .contains("including upgraded Skills that normally do not Exhaust"));
    assert!(expanded.view.contains("  Card Dark Embrace | Power"));
    let inventory = crate::compact::render_inventory_v1(&mut game, true).unwrap();
    assert!(inventory.contains("including upgraded Skills that normally do not Exhaust"));
    assert!(inventory.contains("Dark Embrace"));
    assert!(reading.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::SkipCardReward { .. }
        }
    )));
    assert_eq!(*game.head(), before);
}

#[test]
fn brief_shop_keeps_prices_and_replacement_potions_without_unaffordable_rules() {
    let dir = TestDirectory::new();
    let mut poor = controller(&dir.0, "shop_poor");
    let brief = poor
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    assert!(brief.view.contains("Shrug It Off 60g (insufficient gold)"));
    assert!(brief
        .view
        .contains("Membership Card 150g (insufficient gold)"));
    assert!(!brief.view.contains("  Relic:"), "{}", brief.view);
    assert!(
        !brief.view.contains("  Potion (base rule):"),
        "{}",
        brief.view
    );
    assert!(!brief.view.contains("Gain 8 Block"), "{}", brief.view);
    let full = poor.decision_reading_v1(&[], None).unwrap();
    assert!(full.view.contains("  Relic:"));
    assert!(full.view.contains("  Potion (base rule):"));
    assert!(full.view.contains("Gain 8 Block"));
    assert_eq!(brief.options, full.options);
    let mut stocked = controller(&dir.0, "shop");
    let stocked = stocked
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    assert!(stocked.view.contains("Power Potion 100g (slots full)"));
    assert!(
        stocked.view.contains("  Potion (base rule):"),
        "{}",
        stocked.view
    );
}

#[test]
fn compact_shop_shows_executable_offers_and_curse_removal() {
    let dir = TestDirectory::new();
    let mut session = fixture("shop");
    session.run_state.add_card_to_deck(CardId::Doubt);
    let mut game = controller_with_session(&dir.0, "shop-with-curse", &session);
    let status = crate::compact::render_status_v1(game.head()).unwrap();

    let cards = status.find("Cards for sale").unwrap();
    let relics = status.find("Relics for sale").unwrap();
    let potions = status.find("Potions for sale").unwrap();
    let removal = status.find("Remove card:").unwrap();
    let leave = status.find("Leave: leave").unwrap();
    let deck = status.find("Deck: ").unwrap();
    assert!(cards < relics && relics < potions && potions < removal);
    assert!(removal < leave && leave < deck);
    assert!(status.contains("remove:Doubt+0#0  |  Doubt"));
    assert!(status.contains("Power Potion 100g (slots full)"));
    assert!(!status.contains("buy:PowerPotion"));
    assert!(status.contains("buy:ShrugItOff"));
    assert!(!status.contains("Deck ("));
    let inventory = crate::compact::render_inventory_v1(&mut game, false).unwrap();
    assert!(inventory.contains("Deck ("));
    assert!(inventory.lines().any(|line| line.trim() == "Doubt"));
    assert!(inventory.contains("Relics ("));
    let before = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::BuyCard {
                card: CardId::ShrugItOff,
                ..
            }
        )
    }))
    .unwrap();
    let after = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let receipt = crate::compact::render_inventory_change_v1(&before, &after);
    assert!(receipt.contains("Deck change: +Shrug It Off"));
    assert!(!receipt.contains("Relic gained:"));

    let twins = controller(&dir.0, "shop_twins");
    let twins = crate::compact::render_status_v1(twins.head()).unwrap();
    let commands: Vec<_> = twins
        .lines()
        .filter(|line| line.contains("· Shrug It Off ·"))
        .map(|line| line.split("  |  ").next().unwrap().trim())
        .collect();
    assert_eq!(commands.len(), 2);
    assert_ne!(commands[0], commands[1]);

    let poor = controller(&dir.0, "shop_poor");
    let poor = crate::compact::render_status_v1(poor.head()).unwrap();
    assert!(poor.contains("Shrug It Off 60g (insufficient gold)"));
    assert!(!poor.contains("buy:"));
    assert!(poor.contains("Leave: leave"));
}

#[test]
fn compact_inventory_reads_combat_master_deck_without_advancing() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let head = game.head().clone();
    let inventory = crate::compact::render_inventory_v1(&mut game, false).unwrap();
    assert!(inventory.contains("Deck ("));
    assert!(inventory.contains("Relics ("));
    assert!(inventory.contains("Strike"));
    let explained = crate::compact::render_inventory_v1(&mut game, true).unwrap();
    assert!(explained.contains("Permanent deck; costs before combat powers/relics."));
    assert!(explained.contains("Strike | Attack | 1c"));
    assert_eq!(*game.head(), head);
}

#[test]
fn fixed_block_requests_reach_inventory_and_acquisition_readings() {
    let dir = TestDirectory::new();
    let mut session = fixture("reward");
    let cards: Vec<_> = [0, 1]
        .into_iter()
        .map(|upgrades| RewardCard::new(CardId::PowerThrough, upgrades))
        .collect();
    for upgrades in [0, 1] {
        session
            .run_state
            .add_card_to_deck_with_upgrades(CardId::PowerThrough, upgrades);
    }
    let mut rewards = RewardState::new();
    rewards.items = vec![RewardItem::Card {
        cards: cards.clone(),
    }];
    rewards.pending_card_choice = Some(cards);
    rewards.pending_card_reward_index = Some(0);
    session.engine_state = EngineState::RewardScreen(rewards);
    let mut game = controller_with_session(&dir.0, "fixed-block-readings", &session);
    let before = game.head().clone();
    let inventory = crate::compact::render_inventory_v1(&mut game, true).unwrap();
    let candidates = game.decision_reading_v1(&[], None).unwrap().view;
    for text in [inventory, candidates] {
        for block in [15, 20] {
            assert!(
                text.contains(&format!(
                    "Printed Block request at this upgrade: {block} Block"
                )),
                "{text}"
            );
        }
    }
    assert_eq!(*game.head(), before);
}

#[test]
fn inventory_effects_reports_real_upgrades_and_conditional_engine_rules() {
    let dir = TestDirectory::new();
    let mut session = fixture("reward_late");
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::Kunai));
    session.run_state.potions = vec![Some(Potion::new(PotionId::SpeedPotion, 9001)), None];
    for card in [
        CardId::Sentinel,
        CardId::Sentinel,
        CardId::BattleTrance,
        CardId::Warcry,
        CardId::Offering,
    ] {
        session.run_state.add_card_to_deck(card);
    }
    let to_upgrade: Vec<_> = session
        .run_state
        .master_deck
        .iter()
        .filter(|card| {
            matches!(
                card.id,
                CardId::Corruption
                    | CardId::Sentinel
                    | CardId::BattleTrance
                    | CardId::Warcry
                    | CardId::Offering
            )
        })
        .map(|card| card.lineage)
        .collect();
    for lineage in to_upgrade {
        session.run_state.upgrade_card(lineage);
    }
    let expected_size = session.run_state.master_deck.len();
    let mut game = controller_with_session(&dir.0, "inventory-engine", &session);
    let before = game.head().clone();
    let explained = crate::compact::render_inventory_v1(&mut game, true).unwrap();
    assert!(explained.contains(&format!("Deck ({expected_size}):")));
    assert!(explained.contains("Corruption+ | Power | 2c"));
    assert!(explained.contains("2× Sentinel+ | Skill | 1c"));
    assert!(explained.contains("If this card is Exhausted, gain 2 Energy (3 if upgraded)"));
    assert!(explained.contains("playing it does not Exhaust it"));
    assert!(explained.contains("Warcry+ | Skill | 0c"));
    assert!(explained.contains("Printed draw request at this upgrade: 5 cards."));
    assert!(explained.contains("Counts cards, not damage hits or targets"));
    assert!(explained.contains("P0 Speed Potion"));
    assert!(explained.contains("P1 empty"));
    assert!(explained.contains("Gain Dexterity equal to potency (base 5)"));
    assert!(explained.contains("Draw 1 card (2 if upgraded), then put 1 card"));
    assert!(
        explained.contains(sts_oracle_learning::content::cards::ironclad::battle_trance::MECHANICS)
    );
    let summary = explained
        .find("Mechanic summary (same deck copies listed above):")
        .unwrap();
    assert!(explained.find("2× Sentinel+ | Skill | 1c").unwrap() < summary);
    assert!(explained.contains("request 3 Energy only when this card is Exhausted"));
    assert!(explained.contains("2x Sentinel+ | Skill | 1c"));
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading
        .view
        .contains(&format!("Deck: {expected_size} cards |")));
    assert_eq!(*game.head(), before);
}

#[test]
fn deck_rules_distinguish_strike_tags_and_missing_key_sources() {
    let dir = TestDirectory::new();
    let mut session = fixture("reward");
    session.run_state.add_card_to_deck(CardId::Anger);
    session.run_state.add_card_to_deck(CardId::PerfectedStrike);
    session.run_state.add_card_to_deck(CardId::PommelStrike);
    session.run_state.keys = [false, true, true];
    let mut game = controller_with_session(&dir.0, "deck-key-facts", &session);
    let before = game.head().clone();
    let explained = crate::compact::render_inventory_v1(&mut game, true).unwrap();
    assert!(explained.contains("Anger | Attack | 0c | tags: none"));
    assert!(explained.contains("Perfected Strike | Attack | 2c | tags: Strike"));
    assert!(explained.contains("Pommel Strike | Attack | 1c | tags: Strike"));
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.view.contains("Keys: Sapphire, Emerald"));
    assert!(reading
        .view
        .contains("Missing keys: Ruby: Recall at campfire"));
    assert!(!reading.view.contains("Missing keys: Sapphire"));
    assert!(reading
        .view
        .contains(crate::mechanics_info::KEY_COLLECTION_TIMING));
    assert!(!reading
        .view
        .contains(crate::mechanics_info::EMERALD_KEY_OPPORTUNITIES));
    assert_eq!(*game.head(), before);
}

#[test]
fn key_choices_expose_shared_opportunities_without_changing_the_decision() {
    let dir = TestDirectory::new();
    for (name, item, query) in [
        ("sapphire", RewardItem::SapphireKey, "SapphireKey"),
        ("emerald", RewardItem::EmeraldKey, "EmeraldKey"),
    ] {
        let mut session = fixture("full_reward");
        let mut rewards = RewardState::new();
        rewards.items = vec![
            RewardItem::Relic {
                relic_id: RelicId::Anchor,
            },
            item,
        ];
        session.engine_state = EngineState::RewardScreen(rewards);
        let mut game = controller_with_session(&dir.0, name, &session);
        let before = game.head().clone();
        let reading = game.decision_reading_v1(&[], None).unwrap();
        let mechanics =
            crate::mechanics_info::render(None, query, sts_presentation::Locale::En).unwrap();
        for line in mechanics.lines() {
            assert!(
                reading.view.contains(&format!("  {line}")),
                "{}",
                reading.view
            );
        }
        assert!(reading
            .view
            .contains("At the start of each combat, gain 10 Block"));
        assert_eq!(*game.head(), before);
        assert!(reading.options.iter().any(|option| matches!(
            option.candidate,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ClaimReward {
                    reward: PlannerRewardDescriptor::Relic {
                        relic: RelicId::Anchor
                    },
                    ..
                }
            }
        )));
    }
    let mut session = fixture("reward");
    session.engine_state = EngineState::Campfire;
    let mut game = controller_with_session(&dir.0, "ruby-opportunities", &session);
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    for line in crate::mechanics_info::render(None, "RubyKey", sts_presentation::Locale::En)
        .unwrap()
        .lines()
    {
        assert!(
            reading.view.contains(&format!("  {line}")),
            "{}",
            reading.view
        );
    }
    assert_eq!(*game.head(), before);
    assert!(reading.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Recall
        }
    )));
}

#[test]
fn brief_emerald_reward_explains_and_preserves_the_other_relic() {
    let dir = TestDirectory::new();
    let mut session = fixture("full_reward");
    let mut rewards = RewardState::new();
    rewards.items = vec![
        RewardItem::Relic {
            relic_id: RelicId::LetterOpener,
        },
        RewardItem::EmeraldKey,
    ];
    session.engine_state = EngineState::RewardScreen(rewards);
    let mut game = controller_with_session(&dir.0, "emerald-and-relic", &session);
    let before = game.head().clone();
    let reading = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    assert!(reading
        .view
        .contains(crate::mechanics_info::EMERALD_KEY_REWARD_RELATIONSHIP));
    assert_eq!(*game.head(), before);
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ClaimReward {
                reward: PlannerRewardDescriptor::EmeraldKey,
                ..
            }
        )
    }))
    .unwrap();
    let successor = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    let relic = pick(&successor, |action| {
        matches!(
            action,
            TaskStrategicActionV1::ClaimReward {
                reward: PlannerRewardDescriptor::Relic {
                    relic: RelicId::LetterOpener
                },
                ..
            }
        )
    });
    game.choose_bound_v1(&relic).unwrap();
    let final_reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(final_reading.view.contains("Letter Opener"));
    assert!(final_reading.view.contains("Keys: Emerald"));
}

#[test]
fn shop_and_campfire_readings_show_only_their_connected_preparation_routes() {
    let dir = TestDirectory::new();
    for (name, room) in [
        ("shop", RoomType::ShopRoom),
        ("campfire", RoomType::RestRoom),
    ] {
        let mut session = fixture("shop");
        if name == "campfire" {
            session.engine_state = EngineState::Campfire;
        }
        let mut current = MapRoomNode::new(0, 0);
        current.class = Some(room);
        current.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut next = MapRoomNode::new(0, 1);
        next.class = Some(RoomType::MonsterRoom);
        next.edges.insert(MapEdge::new(0, 1, 0, 2));
        let mut later_shop = MapRoomNode::new(0, 2);
        later_shop.class = Some(RoomType::ShopRoom);
        let mut unconnected_shop = MapRoomNode::new(1, 1);
        unconnected_shop.class = Some(RoomType::ShopRoom);
        session.run_state.map = MapState::new(vec![
            vec![current],
            vec![next, unconnected_shop],
            vec![later_shop],
        ]);
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 0;
        let mut game = controller_with_session(&dir.0, name, &session);
        let before = game.head().clone();
        let reading = game.decision_reading_v1(&[], None).unwrap();
        if name == "shop" {
            assert!(reading.view.contains("Removal:"));
        }
        assert!(reading
            .view
            .contains("Published routes after current room (0,0)"));
        assert!(
            reading.view.find("./p ").unwrap()
                < reading
                    .view
                    .find("Published routes after current room")
                    .unwrap()
        );
        assert!(reading
            .view
            .contains("After leaving this room, choose one next branch."));
        assert!(reading.view.contains("shop map y2 reachable"));
        assert!(!reading.view.contains("shop map y1 reachable"));
        assert!(reading
            .view
            .contains("Elite-free prefix to endpoint: (0,2) Shop"));
        assert!(reading
            .view
            .contains("Rest none; Shop (0,2); path: (0,1) Monster -> (0,2) Shop"));
        assert!(!reading.view.contains("Rest (0,0)"));
        assert!(!reading.view.contains("Shop (0,0)"));
        assert_eq!(*game.head(), before);
        let reused = game
            .decision_reading_v1(&[], reading.context.as_ref())
            .unwrap();
        assert!(reused.view.contains("shop map y2 reachable"));
        assert_eq!(reused.options, reading.options);
    }
}

#[test]
fn missing_emerald_reading_exposes_later_act_opportunities_without_advancing() {
    let dir = TestDirectory::new();
    for act in 1..=3 {
        let mut session = fixture("reward");
        session.run_state.act_num = act;
        session.run_state.keys = [true, true, false];
        let mut game = controller_with_session(&dir.0, &format!("emerald-act-{act}"), &session);
        let before = game.head().clone();
        let reading = game.decision_reading_v1(&[], None).unwrap();
        assert!(reading
            .view
            .contains("Missing keys: Emerald: burning elite key reward"));
        assert!(reading
            .view
            .contains(crate::mechanics_info::KEY_COLLECTION_TIMING));
        assert!(reading
            .view
            .contains(crate::mechanics_info::EMERALD_KEY_OPPORTUNITIES));
        assert_eq!(*game.head(), before);
    }
}

#[test]
fn compact_shop_relic_purchase_reports_only_the_new_relic() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "shop");
    let before = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::BuyRelic {
                relic: RelicId::MembershipCard,
                ..
            }
        )
    }))
    .unwrap();
    let after = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let receipt = crate::compact::render_inventory_change_v1(&before, &after);
    assert_eq!(receipt, "Relic gained: Membership Card\n");
}

fn shop_inventory(reading: &RunPlayDecisionReadingV1) -> &PlannerShopInventoryV1 {
    let TaskPublicObservationV1::Strategic { observation } =
        &reading.state.as_ref().expect("explicit state").observation
    else {
        panic!("expected strategic reading");
    };
    let PlannerDecisionContext::Shop { inventory, .. } = &observation.context else {
        panic!("expected shop inventory");
    };
    inventory
}

#[test]
fn courier_restock_refreshes_bound_offers_and_keeps_full_slot_potions_visible() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "shop_courier");
    let before = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    let stale_neighbor = pick(&before, |a| {
        matches!(a, TaskStrategicActionV1::BuyCard { shop_slot: 1, .. })
    });
    let card = pick(&before, |a| {
        matches!(a, TaskStrategicActionV1::BuyCard { shop_slot: 0, .. })
    });
    game.choose_bound_v1(&card).unwrap();
    let restocked = game
        .decision_reading_v1(&[DecisionDetailV1::State], before.context.as_ref())
        .unwrap();
    let original = shop_inventory(&before);
    let current = shop_inventory(&restocked);
    assert_eq!(current.cards.len(), original.cards.len());
    assert_eq!(current.cards[1], original.cards[1]);
    assert_eq!(current.relics, original.relics);
    assert_eq!(current.potions, original.potions);
    assert_ne!(
        current.cards[0], original.cards[0],
        "fixture must exercise changed stock"
    );
    let replacement = pick(&restocked, |a| {
        matches!(a, TaskStrategicActionV1::BuyCard { shop_slot: 0, card, price, upgrades }
            if *card == current.cards[0].card && *price == current.cards[0].price && *upgrades == current.cards[0].upgrades)
    });
    let label = &restocked
        .options
        .iter()
        .find(|o| o.choice == replacement.choice)
        .unwrap()
        .label;
    assert_eq!(restocked.view.matches(label).count(), 1);
    let head = game.head().clone();
    for stale in [&card, &stale_neighbor] {
        assert!(game
            .choose_bound_v1(stale)
            .unwrap_err()
            .contains("Decision changed"));
    }
    assert_eq!(*game.head(), head);

    let potion = pick(&restocked, |a| {
        matches!(a, TaskStrategicActionV1::BuyPotion { shop_slot: 0, .. })
    });
    game.choose_bound_v1(&potion).unwrap();
    let full = game
        .decision_reading_v1(&[DecisionDetailV1::State], restocked.context.as_ref())
        .unwrap();
    assert_eq!(shop_inventory(&full).potions.len(), 1);
    assert_eq!(shop_inventory(&full).cards, current.cards);
    assert!(full.view.contains("P0 Power Potion"));
    assert!(full.view.contains("(slots full)"));
    assert!(!full.options.iter().any(|option| matches!(
        option.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::BuyPotion { .. }
        }
    )));
    let restock = &shop_inventory(&full).potions[0];
    let name = sts_oracle_learning::content::identity::gameplay_display_label_v1(restock.potion);
    assert_eq!(
        full.view
            .matches(&format!("{name} {}g (slots full)", restock.price))
            .count(),
        1
    );
    assert!(game.choose_bound_v1(&potion).is_err());
    let head = game.head().clone();
    drop(game);
    let mut reopened =
        RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join("shop_courier"))).unwrap();
    assert_eq!(*reopened.head(), head);
    assert!(reopened.choose_bound_v1(&potion).is_err());
    assert_eq!(
        reopened
            .decision_reading_v1(&[DecisionDetailV1::State], restocked.context.as_ref())
            .unwrap(),
        full
    );
}

#[test]
fn courier_membership_purchase_reprices_stock_and_replaces_the_relic_together() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "shop_courier");
    let before = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    let membership = pick(&before, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyRelic {
                relic: RelicId::MembershipCard,
                ..
            }
        )
    });
    game.choose_bound_v1(&membership).unwrap();
    let after = game
        .decision_reading_v1(&[DecisionDetailV1::State], before.context.as_ref())
        .unwrap();
    let old = shop_inventory(&before);
    let new = shop_inventory(&after);
    assert_eq!(new.relics.len(), 1);
    assert_ne!(new.relics[0].relic, RelicId::MembershipCard);
    assert_eq!(new.cards.len(), old.cards.len());
    for (old, new) in old.cards.iter().zip(&new.cards) {
        assert_eq!(
            (new.slot, new.card, new.upgrades),
            (old.slot, old.card, old.upgrades)
        );
        assert_eq!(new.price * 2, old.price);
    }
    assert_eq!(new.potions[0].potion, old.potions[0].potion);
    assert_eq!(new.potions[0].price, 40);
    assert!(after.view.contains("Gold 330"));
    assert!(after.view.contains("buy Shrug It Off 24g"));
    assert!(after.view.contains("buy Pommel Strike 32g"));
    let replacement = pick(&after, |a| {
        matches!(a, TaskStrategicActionV1::BuyRelic { shop_slot: 0, relic, price }
            if *relic == new.relics[0].relic && *price == new.relics[0].price)
    });
    let label = &after
        .options
        .iter()
        .find(|o| o.choice == replacement.choice)
        .unwrap()
        .label;
    assert_eq!(after.view.matches(label).count(), 1);
    for option in &before.options {
        let stale = RunPlayBoundChoiceV1 {
            source: before.source.clone().unwrap(),
            choice: option.choice.clone(),
        };
        assert!(game.choose_bound_v1(&stale).is_err());
    }
    game.choose_bound_v1(&pick(&after, |a| {
        matches!(
            a,
            TaskStrategicActionV1::BuyCard {
                shop_slot: 1,
                card: CardId::PommelStrike,
                price: 32,
                ..
            }
        )
    }))
    .unwrap();
    assert!(game
        .decision_reading_v1(&[], None)
        .unwrap()
        .view
        .contains("Gold 298"));
}

#[test]
fn route_inspection_preserves_head_and_selection_stops_at_combat() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "route");
    let before = game.head().clone();
    let reading = game
        .decision_reading_v1(&[DecisionDetailV1::Map], None)
        .unwrap();
    assert!(reading.view.contains("(0,1) Campfire"));
    assert!(reading.view.contains("(1,1) Shop"));
    assert_eq!(game.head(), &before);
    game.choose_bound_v1(&pick(&reading, |a| {
        matches!(a, TaskStrategicActionV1::ChooseRouteNode { x: 0, y: 0, .. })
    }))
    .unwrap();
    let at_combat = game.head().clone();
    let next = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    assert_eq!(next.control, DecisionControlV1::Combat);
    let sts_oracle_learning::agent::task::TaskPublicObservationV1::Combat { observation, .. } =
        &next.state.as_ref().unwrap().observation
    else {
        panic!("expected combat");
    };
    assert!(next.view.contains(&format!(
        "HP {}/{}",
        observation.player.hp, observation.player.max_hp
    )));
    assert!(next.view.contains("Turn 1 | Player turn | Energy 3"));
    assert!(next.view.contains("Hand:") && next.view.contains("[1c ") && next.view.contains("M0 "));
    assert!(next
        .view
        .contains(&format!("Draw {}", observation.cards.draw.cards.len())));
    assert!(next.options.is_empty());
    assert!(next.source.is_none());
    assert!(next.context.is_none());
    let cold = game
        .decision_reading_v1(&[], reading.context.as_ref())
        .unwrap();
    assert_eq!(cold.view, next.view);
    assert!(cold.state.is_none());
    assert_eq!(
        *game.head(),
        at_combat,
        "handoff reading must not start search or act"
    );
    let combat_choice = RunPlayBoundChoiceV1 {
        source: crate::decision::decision_source(game.head()).unwrap(),
        choice: sts_oracle_learning::agent::task::TaskPublicChoiceV1::one(0),
    };
    assert!(game.choose_bound_v1(&combat_choice).is_err());
    assert_eq!(*game.head(), at_combat);
    assert_eq!(game.head().head_revision, before.head_revision + 1);
}

#[test]
fn escape_reading_reports_settlement_instead_of_a_playable_hand() {
    let mut session = fixture("combat_multiturn");
    let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
    combat.turn.mark_player_escaping();
    let state = sts_oracle_learning::agent::information::state::public_combat_state_v1(combat);
    assert!(!state.cards.hand.cards.is_empty());

    let view = crate::decision::combat::state_reading(&state);
    assert!(
        view.contains("Escape settlement pending | use act end"),
        "{view}"
    );
    assert!(!view.contains("Player turn"));
    assert!(!view.contains("Hand:"));
    assert!(!view.contains("M0 "));
}

#[test]
fn compact_hand_uses_public_target_damage_and_keeps_copies_and_variable_hits_distinct() {
    let session = fixture("combat_multiturn");
    let combat = &session.active_combat.as_ref().unwrap().combat_state;
    let mut state = sts_oracle_learning::agent::information::state::public_combat_state_v1(combat);
    let mut monster = state.monsters[0].clone();
    monster.slot = 2;
    state.monsters.push(monster.clone());
    monster.slot = 6;
    state.monsters.push(monster);
    state.monsters[0].alive = false;
    let mut template = state.cards.hand.cards[0].clone();
    template.current_block = 0;
    template.damage_by_monster_order = vec![999, 12, 8];
    state.cards.hand.cards = [
        (CardId::Cleave, 0, 1, 0),
        (CardId::TwinStrike, 0, 1, 0),
        (CardId::FiendFire, 0, 2, 0),
        (CardId::Defend, 0, 3, 5),
        (CardId::Defend, 1, 1, 8),
        (CardId::Whirlwind, 0, -1, 0),
        (CardId::Dazed, 0, -2, 0),
    ]
    .into_iter()
    .map(|(id, upgrades, cost, block)| {
        let mut card = template.clone();
        card.card_id = id;
        card.upgrades = upgrades;
        card.effective_cost = cost;
        card.current_block = block;
        card
    })
    .collect();
    let view = crate::decision::combat::state_reading(&state);
    assert!(view.contains("Damage: per-target hit after Strength/Weak/Vulnerable, before block."));
    assert!(view.contains("Cleave[1c M2:12d M6:8d all]"), "{view}");
    assert!(view.contains("Twin Strike[1c M2:12dx2 M6:8dx2]"));
    assert!(view.contains("Fiend Fire[2c M2:12d/hit M6:8d/hit]"));
    assert!(view.contains("Defend[3c 5b], Defend+[1c 8b]"));
    assert!(view.contains("Whirlwind[Xc M2:12d/hit M6:8d/hit all]"));
    assert!(view.contains("Dazed[unplayable]"));
    assert!(!view.contains("999"));
    for card in &mut state.cards.hand.cards {
        card.damage_by_monster_order = vec![999, 7, 7];
    }
    assert!(crate::decision::combat::state_reading(&state).contains("Fiend Fire[2c 7d/hit]"));
}

#[test]
fn combat_rules_bind_active_powers_and_hand_without_exposing_hidden_state() {
    use sts_oracle_learning::agent::information::combat::{
        HiddenInformationReasonV1, ObservationEvidenceKindV1,
    };
    use sts_oracle_learning::agent::information::state::CombatLearningPowerV1;
    use sts_oracle_learning::content::powers::PowerId;
    let session = fixture("combat_multiturn");
    let combat = &session.active_combat.as_ref().unwrap().combat_state;
    let mut state = sts_oracle_learning::agent::information::state::public_combat_state_v1(combat);
    let template = state.cards.hand.cards[0].clone();
    state.cards.hand.cards = [
        (CardId::Sentinel, 0),
        (CardId::Sentinel, 0),
        (CardId::Corruption, 1),
        (CardId::LimitBreak, 1),
        (CardId::Apparition, 1),
    ]
    .into_iter()
    .map(|(card_id, upgrades)| {
        sts_oracle_learning::agent::information::state::CombatLearningCardV1 {
            card_id,
            upgrades,
            ..template.clone()
        }
    })
    .collect();
    state.cards.draw.cards.push(
        sts_oracle_learning::agent::information::state::CombatLearningCardV1 {
            card_id: CardId::Feed,
            ..template.clone()
        },
    );
    let power = |power, amount| CombatLearningPowerV1 {
        power,
        amount,
        extra_data: 0,
        just_applied: false,
        payload_card: None,
        source_creature: None,
    };
    state.player.powers = vec![
        power(PowerId::DarkEmbrace, 2),
        power(PowerId::Corruption, -1),
        power(PowerId::NoDraw, -1),
    ];
    state.monsters[0].powers = vec![power(PowerId::Invincible, 17)];
    state.monsters[0].powers[0].extra_data = 200;
    state.monsters[0].intent.intent = None;
    state.monsters[0].intent.preview_damage_per_hit = Some(987654);
    state.monsters[0].intent.evidence = ObservationEvidenceKindV1::Hidden;
    state.monsters[0].intent.hidden_reason = Some(HiddenInformationReasonV1::RunicDome);
    state.potions = vec![Some(
        sts_oracle_learning::agent::information::state::CombatLearningPotionV1 {
            potion_id: PotionId::SpeedPotion,
            can_use: false,
            can_discard: true,
            requires_target: false,
        },
    )];
    let before = state.clone();
    let view = crate::decision::combat::state_reading(&state);
    assert_eq!(state, before);
    assert_eq!(view.matches("  Card Sentinel | Skill").count(), 1);
    assert!(view.contains("If this card is Exhausted, gain 2 Energy (3 if upgraded)"));
    assert!(view.contains("Player Dark Embrace: current amount 2"));
    assert!(view.contains("amount 2 draws 2 per Exhaust"));
    assert!(view.contains("Player Corruption: present (not a numeric stack count)"));
    assert!(view.contains("Prevents further card draw while active"));
    assert!(view.contains("  Card Limit Break+ | Skill"));
    assert!(view.contains("  Card Apparition+ | Skill"));
    assert!(view.contains("remaining damage allowance 17; reset cap 200"));
    assert!(view.contains("Invincible (damage left 17/200)"));
    for amount in [0, 2, 200] {
        let mut power = state.monsters[0].powers[0].clone();
        power.amount = amount;
        assert_eq!(
            crate::render::render_powers(&[power]),
            format!("Invincible (damage left {amount}/200)"),
        );
    }
    assert!(view.contains("Speed Potion | usable false | discardable true"));
    assert!(view.contains("Intent hidden"));
    assert!(view.contains("Possible moves (base values before powers; not a prediction)"));
    assert!(!view.contains("987654") && !view.contains("Card Feed"));
}

#[test]
fn combat_rules_include_discovery_choices_and_exhaust_selection_cards() {
    let dir = TestDirectory::new();
    for kind in ["combat_discovery", "combat_exhaust"] {
        let mut game = controller(&dir.0, kind);
        let before = game.head().clone();
        let reading = game.decision_reading_v1(&[], None).unwrap();
        assert!(
            reading.view.contains("Choice card rules (base/upgraded):"),
            "{}",
            reading.view
        );
        assert!(reading.view.contains("Printed rule (base/upgraded):"));
        assert_eq!(*game.head(), before);
    }
}

#[test]
fn combat_selector_reports_an_extraneous_target_without_rewriting_the_action() {
    let dir = TestDirectory::new();
    let game = controller(&dir.0, "combat_multiturn");
    let decision = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let targeted = crate::selector::parse_action_selector_v1("play:Defend#0@0", "test").unwrap();
    let error = crate::selector::resolve_action_selector_v1(&decision, &targeted).unwrap_err();
    assert!(
        error.contains("legal play without a monster target"),
        "{error}"
    );
    assert!(error.contains("omit @0"), "{error}");
    let untargeted = crate::selector::parse_action_selector_v1("play:Defend#0", "test").unwrap();
    assert!(crate::selector::resolve_action_selector_v1(&decision, &untargeted).is_ok());
}

#[test]
fn combat_selector_explains_absent_cards_instead_of_encouraging_retries() {
    let dir = TestDirectory::new();
    let game = controller(&dir.0, "combat_multiturn");
    let before = game.head().clone();
    let decision = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    for command in ["play:Apotheosis+0", "play:Apotheosis#0"] {
        let action = crate::selector::parse_action_selector_v1(command, "test").unwrap();
        let error = crate::selector::resolve_action_selector_v1(&decision, &action).unwrap_err();
        assert!(
            error.contains("not in the hand at this boundary"),
            "{error}"
        );
        assert!(!error.contains("use read or options"), "{error}");
        assert!(!error.contains("repeat play"), "{error}");
    }
    assert_eq!(*game.head(), before);
}

#[test]
fn chest_can_be_left_unopened_without_a_cursed_key_cost() {
    use sts_oracle_learning::content::relics::RelicTier;
    use sts_oracle_learning::state::rewards::{TreasureChestSize, TreasureChestState};
    let dir = TestDirectory::new();
    let mut session = fixture("route");
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::CursedKey));
    session.engine_state = EngineState::TreasureRoom(TreasureChestState {
        size: TreasureChestSize::Small,
        base_relic_tier: RelicTier::Common,
        gold_reward_base_amount: Some(25),
    });
    let mut game = controller_with_session(&dir.0, "chest", &session);
    let root = game.head().position.archive_node_id().to_owned();
    let before = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    assert!(
        before.view.contains("Leave chest unopened"),
        "{}",
        before.view
    );
    game.choose_bound_v1(&pick(&before, |a| {
        matches!(a, TaskStrategicActionV1::Proceed { .. })
    }))
    .unwrap();
    let after = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation: old } =
        &before.state.unwrap().observation
    else {
        panic!()
    };
    let TaskPublicObservationV1::Strategic { observation: new } = &after.state.unwrap().observation
    else {
        panic!()
    };
    assert_eq!(new.run, old.run, "leaving must not change gold or health");
    assert_eq!(new.cards, old.cards);
    assert_eq!(new.relics, old.relics);
    assert!(matches!(new.context, PlannerDecisionContext::Map { .. }));
    game.jump(&format!("n{root}")).unwrap();
    let at_chest = game.decision_reading_v1(&[], None).unwrap();
    game.choose_bound_v1(&pick(&at_chest, |a| {
        matches!(a, TaskStrategicActionV1::OpenChest)
    }))
    .unwrap();
    let opened = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &opened.state.unwrap().observation
    else {
        panic!()
    };
    assert_eq!(
        observation.cards.len(),
        old.cards.len() + 1,
        "opening still obtains a curse"
    );
}

#[test]
fn act_returns_the_successor_reading_and_selection_jump_renders_after_commit() {
    use crate::agent_protocol::{
        run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1,
    };
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_pact_draft");
    let input = serde_json::json!({"schema":"sts-run-play-agent-request-v1", "request_id":"act",
        "command":"act", "actions":["play:BurningPact"]});
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(format!("{input}\n")),
        &mut output,
    )
    .unwrap();
    let response: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    assert_eq!(response["result"]["kind"], "acted");
    let reading = game
        .decision_reading_v1(&[DecisionDetailV1::Brief], None)
        .unwrap();
    assert_eq!(
        response["result"]["reading"],
        serde_json::to_value(&reading).unwrap()
    );
    assert_eq!(reading.selections.len(), 1);
    let public = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let displayed_commands = reading
        .view
        .lines()
        .filter(|line| line.starts_with("  ./p act select:"))
        .collect::<Vec<_>>();
    assert_eq!(
        displayed_commands.len(),
        reading.selections[0].members.len()
    );
    for (member, line) in reading.selections[0].members.iter().zip(displayed_commands) {
        let command = line.trim().split("  |  ").next().unwrap();
        let spec = command.strip_prefix("./p act ").unwrap();
        let selector = crate::selector::parse_action_selector_v1(&spec, "test").unwrap();
        let actual = crate::selector::resolve_action_selector_v1(&public, &selector).unwrap();
        let mut expected = reading.selections[0].choice.clone();
        expected.ordinal_path.push(member.ordinal);
        assert_eq!(actual, expected, "displayed command: {command}");
    }
    let root = game.head().position.archive_node_id().to_owned();
    let mut choice = reading.selections[0].choice.clone();
    choice
        .ordinal_path
        .push(reading.selections[0].members[0].ordinal);
    let selected = RunPlayBoundChoiceV1 {
        source: reading.source.unwrap(),
        choice,
    };
    game.choose_bound_v1(&selected).unwrap();
    game.jump(&format!("n{root}")).unwrap();
    let rendered = crate::render::render_show_v1(game.head()).unwrap();
    assert!(rendered.contains("<parameterized>"), "{rendered}");
    let restored = game.decision_reading_v1(&[], None).unwrap();
    assert_eq!(restored.selections.len(), 1);
    assert!(
        game.choose_bound_v1(&selected).is_err(),
        "old choices remain stale after jumping back"
    );
}

#[test]
fn malformed_or_foreign_choices_cannot_change_the_published_head() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward");
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let command = pick(&reading, |a| {
        matches!(a, TaskStrategicActionV1::SkipCardReward { .. })
    });
    let before = game.head().clone();
    let mut invalid = command.clone();
    invalid.choice.ordinal_path = vec![usize::MAX];
    assert!(game.choose_bound_v1(&invalid).is_err());
    invalid = command.clone();
    invalid.source.session_id.push_str("-other");
    assert!(game.choose_bound_v1(&invalid).is_err());
    invalid = command.clone();
    invalid.source.public_snapshot_id.push_str("-other");
    assert!(game.choose_bound_v1(&invalid).is_err());
    assert_eq!(game.head(), &before);
    let mut other_writer =
        RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join("reward"))).unwrap();
    other_writer.choose_bound_v1(&command).unwrap();
    assert!(game.choose_bound_v1(&command).is_err());
    assert_eq!(
        RunPlayPathsV1::new(dir.0.join("reward"))
            .load_head()
            .unwrap(),
        *other_writer.head()
    );
    let fresh = game.decision_reading_v1(&[], None).unwrap();
    assert_eq!(fresh, other_writer.decision_reading_v1(&[], None).unwrap());
}

#[test]
fn jsonl_choices_use_bound_objects_and_duplicate_delivery_does_not_repeat_execution() {
    use crate::agent_protocol::{
        run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1,
    };
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward");
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let selection = pick(&reading, |a| {
        matches!(a, TaskStrategicActionV1::SkipCardReward { .. })
    });
    let choice = serde_json::json!({"schema":"sts-run-play-agent-request-v1", "request_id":"choose-1",
        "command":"choose", "selection":selection, "known_context":reading.context});
    let inspect = serde_json::json!({"schema":"sts-run-play-agent-request-v1", "request_id":"inspect",
        "command":"decision", "details":["state"]});
    let input = format!("{choice}\n{choice}\n{inspect}\n");
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let responses: Vec<serde_json::Value> = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["result"]["kind"], "chosen");
    assert!(responses[0]["result"]["reading"]["view"]
        .as_str()
        .unwrap()
        .contains("Deck:"));
    assert_eq!(responses[1]["status"], "error");
    assert_eq!(responses[2]["result"]["kind"], "decision");
    assert_eq!(
        responses[0]["result"]["reading"]["last_choice"],
        responses[2]["result"]["reading"]["last_choice"]
    );
    assert!(responses[2]["result"]["reading"]["state"].is_object());
    assert!(responses[2]["result"]["reading"]["view"]
        .as_str()
        .unwrap()
        .contains("Deck:"));
    assert_eq!(
        game.head().head_revision,
        selection.source.head_revision + 1
    );
    let mut unbound = choice;
    unbound["selection"]
        .as_object_mut()
        .unwrap()
        .remove("source");
    assert!(serde_json::from_value::<crate::agent_protocol::AgentShellRequestV1>(unbound).is_err());
}

#[test]
fn combat_research_cache_is_bound_to_the_potion_contract() {
    use crate::agent_protocol::{
        run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1,
    };
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let before = game.head().clone();
    let input = [Some(1_u32), Some(1), Some(0), Some(0), None, None]
        .into_iter().enumerate().map(|(i, ceiling)| {
            let mut request = serde_json::json!({
                "schema":"sts-run-play-agent-request-v1", "request_id":format!("potions-{i}"),
                "command":"combat_research", "effort":"whole_combat", "max_search_ms":1000, "max_generation_work":1000,
            });
            if let Some(ceiling) = ceiling { request["max_potions_used"] = ceiling.into(); }
            format!("{request}\n")
        }).collect::<String>();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let responses = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    for (i, response) in responses.iter().enumerate() {
        assert_eq!(response["status"], "ok", "{response}");
        assert_eq!(response["result"]["reused_cached_frontier"], i % 2 == 1);
        assert_eq!(response["result"]["durable_head_unchanged"], true);
        if i < 4 {
            assert_eq!(
                response["result"]["max_potions_used"],
                if i < 2 { 1 } else { 0 }
            );
        }
    }
    assert_eq!(game.head(), &before);
    assert_ne!(
        responses[0]["result"]["report_id"],
        responses[2]["result"]["report_id"]
    );
}

#[test]
fn combat_commit_returns_usable_successor_choices_without_an_extra_read() {
    use crate::agent_protocol::{
        run_agent_jsonl_v1, AgentCapabilitiesV1, AgentPermissionEnvelopeV1,
    };
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let before = game.head().head_revision;
    let requests = [
        serde_json::json!({"command":"decision"}),
        serde_json::json!({"command":"combat_research","effort":"fast_witness"}),
        serde_json::json!({"command":"combat_review","candidate_refs":["c0"],"effort":"fast_witness"}),
        serde_json::json!({"command":"combat_review","candidate_refs":["c0"],"effort":"fast_witness","turn":1,"include_steps":true}),
        serde_json::json!({"command":"combat_review","candidate_refs":["c0"],"effort":"fast_witness","turn":99}),
        serde_json::json!({"command":"commit_combat_plan","candidate_ref":"c0","effort":"fast_witness"}),
        serde_json::json!({"command":"commit_combat_plan","candidate_ref":"c0","effort":"fast_witness"}),
        serde_json::json!({"command":"decision","details":["brief"]}),
    ];
    let input = requests
        .into_iter()
        .enumerate()
        .map(|(i, mut request)| {
            request["schema"] = "sts-run-play-agent-request-v1".into();
            request["request_id"] = format!("combat-{i}").into();
            format!("{request}\n")
        })
        .collect::<String>();
    let mut output = Vec::new();
    run_agent_jsonl_v1(
        &mut game,
        AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None)),
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let responses: Vec<serde_json::Value> = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for i in [0, 1, 2, 3, 5, 7] {
        assert_eq!(responses[i]["status"], "ok", "{}", responses[i]);
    }
    assert_eq!(responses[0]["result"]["reading"]["control"], "combat");
    assert_eq!(responses[1]["result"]["root_head_revision"], before);
    assert_eq!(responses[1]["result"]["durable_head_unchanged"], true);
    let research_view = responses[1]["result"]["view"].as_str().unwrap();
    let review_view = responses[2]["result"]["view"].as_str().unwrap();
    let plan_id = responses[1]["result"]["candidates"][0]["plan_id"]
        .as_str()
        .unwrap();
    assert!(research_view.contains(&format!("{plan_id}: HP 74/75 after combat")));
    assert!(research_view.contains("min observed 68"));
    assert_eq!(
        responses[1]["result"]["candidates"][0]["minimum_observed_hp"],
        68
    );
    assert!(review_view.contains("Minimum observed HP: 68"));
    assert!(review_view.contains("Turn 1"));
    assert!(review_view.contains("Strike @M0"));
    assert!(review_view.contains("Actions: HP +6"));
    assert!(!review_view.contains("Hand:"));
    let expanded = responses[3]["result"]["view"].as_str().unwrap();
    assert!(expanded.contains("Hand: Strike[1c]"));
    assert!(expanded.contains("M0 Jaw Worm: HP 1/42"));
    assert!(research_view.contains("from T1 HP 68/75"));
    assert!(research_view.contains(&format!("./p commit {plan_id}")));
    assert!(!research_view.contains("Next actions:"));
    assert!(!research_view.contains("Cards played this turn"));
    assert!(!review_view.contains("play:Strike"));
    assert!(responses[2]["result"]["trace"]["candidates"][0]
        .get("steps")
        .is_none());
    assert_eq!(
        responses[3]["result"]["trace"]["candidates"][0]["steps"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        responses[4]["status"], "error",
        "absent turn must be explicit"
    );
    for view in [research_view, review_view] {
        assert!(!view.contains("snapshot"));
        assert!(!view.contains("\u{1b}"));
        assert!(view.ends_with('\n'));
    }
    assert_eq!(
        responses[6]["status"], "error",
        "duplicate commit must not replay the fight"
    );
    let committed = &responses[5]["result"]["reading"];
    assert_eq!(committed, &responses[7]["result"]["reading"]);
    assert_eq!(committed["control"], "strategic");
    assert!(committed["view"].as_str().unwrap().contains("Rewards"));
    assert!(committed.get("state").is_none());
    assert!(game.head().head_revision > before);
    assert_eq!(
        committed["source"]["head_revision"],
        game.head().head_revision
    );
    let reward = committed["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| {
            matches!(
                serde_json::from_value::<TaskPublicCandidateV1>(option["candidate"].clone())
                    .unwrap(),
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ClaimReward {
                        reward: PlannerRewardDescriptor::Gold { .. },
                        ..
                    }
                }
            )
        })
        .expect("postcombat gold choice");
    let selection: RunPlayBoundChoiceV1 = serde_json::from_value(serde_json::json!({
        "source":committed["source"], "choice":reward["choice"]
    }))
    .unwrap();
    game.choose_bound_v1(&selection).unwrap();
    let claimed = game.head().clone();
    assert!(game.choose_bound_v1(&selection).is_err());
    assert_eq!(*game.head(), claimed);
    drop(game);
    let reopened =
        RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join("combat_finish"))).unwrap();
    assert_eq!(*reopened.head(), claimed);
}

#[test]
fn pending_combat_choices_are_bound_and_stop_after_the_selected_action() {
    let dir = TestDirectory::new();
    for kind in ["combat_discovery", "combat_exhaust", "combat_discard"] {
        let mut game = controller(&dir.0, kind);
        let reading = game
            .decision_reading_v1(&[DecisionDetailV1::State], None)
            .unwrap();
        assert_eq!(reading.control, DecisionControlV1::Combat);
        let source = reading.source.clone().expect("pending choice source");
        let before = game.head().clone();
        let choice = if kind == "combat_discovery" {
            let cards = reading
                .options
                .iter()
                .filter(|option| {
                    matches!(
                        option.candidate,
                        TaskPublicCandidateV1::CombatIndexedChoice { .. }
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(cards.len(), 3);
            assert!(reading.selections.is_empty());
            cards[0].choice.clone()
        } else {
            assert_eq!(reading.selections.len(), 1);
            let family = &reading.selections[0];
            assert!(reading.view.contains(&family.label));
            let strikes = family
                .members
                .iter()
                .filter(|m| m.label == "Strike")
                .collect::<Vec<_>>();
            assert_eq!(
                strikes.len(),
                2,
                "same-name occurrences must remain separate"
            );
            let mut choice = family.choice.clone();
            choice.ordinal_path.push(strikes[1].ordinal);
            if kind == "combat_discard" {
                choice.ordinal_path.push(strikes[0].ordinal);
            }
            let mut invalid = choice.clone();
            invalid.ordinal_path.push(strikes[1].ordinal);
            assert!(game
                .choose_bound_v1(&RunPlayBoundChoiceV1 {
                    source: source.clone(),
                    choice: invalid
                })
                .is_err());
            assert_eq!(*game.head(), before);
            choice
        };
        let selected = RunPlayBoundChoiceV1 {
            source,
            choice: choice.clone(),
        };
        game.choose_bound_v1(&selected).unwrap();
        let after = game
            .decision_reading_v1(&[DecisionDetailV1::State], None)
            .unwrap();
        assert_eq!(after.control, DecisionControlV1::Combat);
        assert!(after.options.is_empty());
        assert!(after.selections.is_empty());
        assert!(after.source.is_none());
        assert_eq!(game.head().head_revision, before.head_revision + 1);
        assert_eq!(after.last_choice.as_ref().unwrap().choices, vec![choice]);
        let TaskPublicObservationV1::Combat { observation, .. } =
            &after.state.as_ref().unwrap().observation
        else {
            panic!("combat successor")
        };
        assert_eq!(
            observation.turn.turn_count, 0,
            "must not play the rest of the turn"
        );
        if kind == "combat_exhaust" {
            assert!(observation
                .cards
                .exhaust
                .cards
                .iter()
                .any(|card| card.card_id == CardId::Strike));
        }
        let committed = game.head().clone();
        assert!(game.choose_bound_v1(&selected).is_err());
        assert_eq!(*game.head(), committed);
        drop(game);
        let reopened = RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join(kind))).unwrap();
        assert_eq!(*reopened.head(), committed);
    }
}

#[test]
fn run_selections_preserve_order_instances_and_stop_at_the_return_boundary() {
    let dir = TestDirectory::new();
    for (kind, expected_domain, chosen) in [
        ("astrolabe", vec![1, 2, 3, 4, 5, 6], vec![2, 0, 1]),
        ("empty_cage", vec![1, 2, 3, 4, 5, 6], vec![2, 1]),
    ] {
        let mut game = controller(&dir.0, kind);
        let reading = game
            .decision_reading_v1(&[DecisionDetailV1::State], None)
            .unwrap();
        assert_eq!(reading.control, DecisionControlV1::Strategic);
        assert_eq!(reading.selections.len(), 1, "{kind}: {}", reading.view);
        assert!(!reading.options.iter().any(|o| matches!(
            o.candidate,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::BeginRunCardSelection { .. }
            }
        )));
        assert!(!reading.view.contains("input="));
        let family = &reading.selections[0];
        assert_eq!(family.members.len(), expected_domain.len());
        assert_eq!(
            (family.min, family.max),
            (chosen.len() as u64, chosen.len() as u64)
        );
        let decision = reading.state.as_ref().unwrap();
        let TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::BeginRunCardSelection {
                    selectable_card_ordinals,
                    ..
                },
        } = &decision.candidates[family.choice.ordinal_path[0]]
        else {
            panic!("selection family")
        };
        assert_eq!(*selectable_card_ordinals, expected_domain);
        let mut expected = fixture(kind).run_state;
        for (index, member) in family.members.iter().enumerate() {
            assert_eq!(
                member.ordinal, index,
                "choice uses domain position, not deck ordinal"
            );
            let card = &expected.master_deck[expected_domain[index]];
            assert_eq!(
                member.label,
                crate::render::format_card(card.id, card.upgrades)
            );
        }
        let chosen_lineages: Vec<_> = chosen
            .iter()
            .map(|&i| expected.master_deck[expected_domain[i]].lineage)
            .collect();
        let mut choice = family.choice.clone();
        choice
            .ordinal_path
            .extend(chosen.iter().map(|&i| family.members[i].ordinal));
        let selected = RunPlayBoundChoiceV1 {
            source: reading.source.clone().unwrap(),
            choice,
        };
        let before = game.head().clone();
        // These are execution errors, so verify that rejecting them leaves the
        // durable head and the following valid choice intact.
        let mut invalid_paths = vec![family.choice.ordinal_path.clone()];
        let mut out_of_range = selected.choice.ordinal_path.clone();
        out_of_range[1] = family.members.len();
        invalid_paths.push(out_of_range);
        let mut too_many = selected.choice.ordinal_path.clone();
        too_many.push(0);
        invalid_paths.push(too_many);
        if chosen.len() > 1 {
            let mut duplicate = selected.choice.ordinal_path.clone();
            duplicate[2] = duplicate[1];
            invalid_paths.push(duplicate);
        }
        for ordinal_path in invalid_paths {
            let mut invalid = selected.clone();
            invalid.choice.ordinal_path = ordinal_path;
            assert!(game.choose_bound_v1(&invalid).is_err());
            assert_eq!(*game.head(), before);
        }
        game.choose_bound_v1(&selected).unwrap();
        let after = game
            .decision_reading_v1(&[DecisionDetailV1::State], None)
            .unwrap();
        assert!(after.selections.is_empty());
        assert_eq!(
            after.last_choice.as_ref().unwrap().choices,
            vec![selected.choice.clone()]
        );
        assert_eq!(game.head().head_revision, before.head_revision + 1);
        let actual = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert!(matches!(actual.engine_state, EngineState::MapNavigation));
        match kind {
            "astrolabe" => {
                expected.transform_card_lineages_deferred_obtain_with_source(
                    &chosen_lineages,
                    true,
                    DomainEventSource::Relic(RelicId::Astrolabe),
                );
                let deck = |run: &sts_oracle_learning::state::run::RunState| {
                    run.master_deck
                        .iter()
                        .map(|c| (c.lineage, c.id, c.upgrades))
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    deck(&actual.run_state),
                    deck(&expected),
                    "transforms consume the ordered instances"
                );
            }
            "empty_cage" => {
                let retained: Vec<_> = expected
                    .master_deck
                    .iter()
                    .filter(|c| !chosen_lineages.contains(&c.lineage))
                    .map(|c| c.lineage)
                    .collect();
                assert_eq!(
                    actual
                        .run_state
                        .master_deck
                        .iter()
                        .map(|c| c.lineage)
                        .collect::<Vec<_>>(),
                    retained
                );
            }
            _ => unreachable!(),
        }
        let committed = game.head().clone();
        assert!(game.choose_bound_v1(&selected).is_err());
        assert_eq!(*game.head(), committed);
        drop(game);
        let mut reopened =
            RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join(kind))).unwrap();
        assert_eq!(*reopened.head(), committed);
        assert_eq!(
            reopened
                .decision_reading_v1(&[DecisionDetailV1::State], None)
                .unwrap(),
            after
        );
    }
}

#[test]
fn bottled_single_card_choices_keep_the_filtered_instances_and_upgrades() {
    use sts_oracle_learning::content::relics::persistent_state::persistent_relic_bound_card_lineage_v1;

    let dir = TestDirectory::new();
    for (kind, deck_ordinals, selected_index) in [
        ("bottled_flame", vec![1, 3, 4], 1),
        ("bottled_lightning", vec![2, 5], 1),
        ("bottled_tornado", vec![6], 0),
    ] {
        let mut game = controller(&dir.0, kind);
        let reading = game.decision_reading_v1(&[], None).unwrap();
        assert!(
            reading.selections.is_empty(),
            "single choices use ordinary bound options"
        );
        assert_eq!(reading.options.len(), deck_ordinals.len());
        assert!(!reading.view.contains("@deck"));
        let prepared = fixture(kind);
        for (option, &deck_ordinal) in reading.options.iter().zip(&deck_ordinals) {
            let TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::SubmitRunSelection {
                        selected_card_ordinals,
                        ..
                    },
            } = &option.candidate
            else {
                panic!("single deck choice")
            };
            assert_eq!(*selected_card_ordinals, vec![deck_ordinal]);
            let card = &prepared.run_state.master_deck[deck_ordinal];
            assert_eq!(
                option.label,
                format!(
                    "Select {}",
                    crate::render::format_card(card.id, card.upgrades)
                )
            );
        }
        let selected = RunPlayBoundChoiceV1 {
            source: reading.source.clone().unwrap(),
            choice: reading.options[selected_index].choice.clone(),
        };
        game.choose_bound_v1(&selected).unwrap();
        let actual = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert!(matches!(actual.engine_state, EngineState::MapNavigation));
        assert_eq!(
            persistent_relic_bound_card_lineage_v1(actual.run_state.relics.last().unwrap())
                .unwrap(),
            Some(prepared.run_state.master_deck[deck_ordinals[selected_index]].lineage)
        );
        let committed = game.head().clone();
        assert_eq!(committed.head_revision, selected.source.head_revision + 1);
        assert!(game.choose_bound_v1(&selected).is_err());
        assert_eq!(*game.head(), committed);
        drop(game);
        let reopened = RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join(kind))).unwrap();
        assert_eq!(*reopened.head(), committed);
    }
}

#[test]
fn draft_readings_keep_turn_choices_private_until_a_verified_win_is_adopted() {
    use crate::agent_combat_draft::AgentCombatDraftRuntimeV1;

    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "combat_finish");
    let mut draft = AgentCombatDraftRuntimeV1::default();
    let before = game.head().clone();
    let empty = draft.status(&mut game, false).unwrap();
    assert!(!empty.draft_present);
    assert!(empty.reading.options.is_empty());
    let generated = draft.advance(&mut game, 1024, 65_536).unwrap();
    assert!(generated.generation.generation_work_consumed <= 1024);
    assert!(generated.generation.engine_steps_consumed <= 65_536);
    assert!(!generated.reading.options.is_empty());
    assert!(generated.reading.view.starts_with("Draft | A20 | HP"));
    assert!(!generated.reading.view.contains("frame="));
    assert!(!generated.reading.view.contains("combat-draft-v1"));
    let option = generated
        .reading
        .options
        .iter()
        .find(|o| o.boundary == "terminal_win")
        .unwrap();
    assert!(generated.reading.view.contains(&option.label));
    // The retained victory includes the already-settled Burning Blood heal.
    assert_eq!(option.successor.hp, 74);
    assert_eq!(option.successor.monsters[0].hp, 0);
    assert!(!option.successor.monsters[0].alive);
    assert!(option.action_count > 0);
    let reread = draft.status(&mut game, false).unwrap();
    assert_eq!(reread.reading.options, generated.reading.options);
    let inspected = draft
        .inspect(&mut game, &generated.frame_ref, &option.option_ref)
        .unwrap();
    assert!(inspected.view.contains("Strike[1c]"));
    assert!(draft
        .choose(&mut game, "another frame", &option.option_ref)
        .is_err());
    assert_eq!(*game.head(), before);
    let chosen = draft
        .choose(&mut game, &generated.frame_ref, &option.option_ref)
        .unwrap();
    assert_eq!(chosen.response.commit_candidate_ref, Some("c0"));
    assert!(chosen.response.reading.options.is_empty());
    assert!(chosen.response.reading.view.contains("Victory"));
    assert_eq!(*game.head(), before);
    let restored = draft.undo(&mut game).unwrap();
    assert_eq!(restored.frame_ref, generated.frame_ref);
    assert_eq!(
        restored.reading.options.len(),
        generated.reading.options.len()
    );
    let chosen = draft
        .choose(&mut game, &restored.frame_ref, &option.option_ref)
        .unwrap();
    let attempt = chosen.commit_attempt.unwrap();
    let candidate_id =
        crate::driver::combat_research_candidate_id_v1(&attempt.attempt.candidates[0]).unwrap();
    game.anchor_cached_combat_research_candidate_id_v1(
        before.head_revision,
        &attempt.attempt,
        &candidate_id,
    )
    .unwrap();
    let after = game.decision_reading_v1(&[], None).unwrap();
    assert_eq!(after.control, DecisionControlV1::Strategic);
    assert!(after.view.contains("Rewards"));
    assert!(
        draft.status(&mut game, false).is_err(),
        "adopted combat invalidates the private root"
    );
}
