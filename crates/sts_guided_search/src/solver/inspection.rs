//! Offline diagnosis of one actual retention batch. Never guides the solver.
use super::retention::SCORE_LANES;
use super::*;
use serde_json::{json, Value};

pub(super) struct RetentionCohort {
    reference_step: usize,
    pool: String,
    at_work: usize,
    raw_candidates: usize,
    virtual_candidates: usize,
    nodes: Vec<Node>,
    kept: Vec<bool>,
    reference: usize,
    context_effects: bool,
    context_card_compositions: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{
        cards::CardId,
        monsters::factory::EncounterId,
        potions::{Potion, PotionId},
    };
    use sts_core::state::map::node::RoomType;

    fn mixed_response_root(energy: u8) -> CombatPosition {
        use sts_core::content::{monsters::EnemyId, powers::PowerId};
        use sts_core::runtime::combat::{CombatCard, Power};
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 18;
        c.entities.player.max_hp = 75;
        c.turn.energy = energy;
        let mut attacker = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        attacker.id = 1;
        attacker.current_hp = 34;
        attacker.max_hp = 34;
        let mut other = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        other.id = 2;
        other.current_hp = 100;
        other.max_hp = 100;
        c.entities.monsters = vec![attacker, other];
        for (owner, strength) in [(0, 4), (1, 15), (2, 8)] {
            sts_core::content::powers::store::set_powers_for(
                &mut c,
                owner,
                vec![Power {
                    power_type: PowerId::Strength,
                    amount: strength,
                    instance_id: None,
                    extra_data: 0,
                    payload: Default::default(),
                    just_applied: false,
                }],
            );
        }
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::ShrugItOff, 701),
            CombatCard::for_scenario(CardId::HeavyBlade, 702),
            CombatCard::for_scenario(CardId::Strike, 703),
        ];
        c.zones.hand[0].upgrades = 1;
        c.zones.hand[1].upgrades = 1;
        c.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Reaper, 704)].into();
        CombatPosition::new(sts_core::state::core::EngineState::CombatPlayerTurn, c)
    }

    #[test]
    fn local_response_shares_payment_and_completes_kill_plus_block() {
        let space = CombatSearchSpace::with_potion_limit(0);
        let config = SolverConfig {
            max_potion_uses: Some(0),
            ..Default::default()
        };
        for energy in [2, 3] {
            let root = mixed_response_root(energy);
            let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
            let mut probe = TurnProbe::local_response(&node);
            probe
                .advance(
                    1,
                    &space,
                    &config,
                    &root,
                    Instant::now() + Duration::from_secs(5),
                )
                .unwrap();
            assert!(!probe.result(1)["complete"].as_bool().unwrap());
            probe
                .advance(
                    2000,
                    &space,
                    &config,
                    &root,
                    Instant::now() + Duration::from_secs(5),
                )
                .unwrap();
            assert!(probe.result(2000)["complete"].as_bool().unwrap());
            if energy == 2 {
                assert!(
                    probe.best_resource.is_none(),
                    "kill and block cannot share only two energy"
                );
                assert!(probe.losses > 0);
            } else {
                let best = probe.best_resource.as_ref().unwrap();
                assert_eq!(response_resources(best), (18, 10, 8));
                assert_eq!(
                    best.state.position().combat.entities.monsters[0].current_hp,
                    0
                );
                assert!(
                    probe.response_support_actions_admitted > 0,
                    "newly drawn Reaper can offer immediate healing support"
                );
                assert_eq!(
                    crate::shuffle_order::replay(&root, &best.inputs())
                        .unwrap()
                        .last(),
                    Some(best.state.position())
                );
            }
            assert_eq!(node.state.position(), &root);
        }
    }

    #[test]
    fn local_response_realizes_strength_then_healing_before_enemy_damage() {
        use sts_core::{content::powers::PowerId, runtime::combat::CombatCard};
        let mut root = mixed_response_root(4);
        root.combat.entities.player.current_hp = 4;
        root.combat
            .entities
            .power_db
            .get_mut(&root.combat.entities.player.id)
            .unwrap()
            .iter_mut()
            .find(|p| p.power_type == PowerId::Strength)
            .unwrap()
            .amount = 32;
        root.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::LimitBreak, 801),
            CombatCard::for_scenario(CardId::Reaper, 802),
            CombatCard::for_scenario(CardId::Defend, 803),
        ];
        root.combat.zones.hand[0].upgrades = 1;
        root.combat.zones.draw_pile.clear();
        let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let mut probe = TurnProbe::local_response(&node);
        let space = CombatSearchSpace::with_potion_limit(0);
        let config = SolverConfig {
            max_potion_uses: Some(0),
            ..Default::default()
        };
        probe
            .advance(
                2000,
                &space,
                &config,
                &root,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert!(probe.result(2000)["complete"].as_bool().unwrap());
        let best = probe.best_resource.as_ref().unwrap();
        assert_eq!(response_resources(best), (61, 61, 0));
        let replay = crate::shuffle_order::replay(&root, &best.inputs()).unwrap();
        assert!(replay
            .iter()
            .any(|p| p.combat.entities.player.current_hp == 75));
        assert_eq!(replay.last(), Some(best.state.position()));
        assert!(best
            .state
            .position()
            .combat
            .zones
            .exhaust_pile
            .iter()
            .any(|c| c.id == CardId::Reaper));
        assert_eq!(node.state.position(), &root);
    }

    #[test]
    fn local_response_admits_drawn_attack_debuffs_and_settles_artifact() {
        use sts_core::{
            content::powers::PowerId,
            runtime::combat::{CombatCard, Power},
        };
        for (card, target, artifact) in [
            (CardId::PiercingWail, None, false),
            (CardId::Blind, Some(1), false),
            (CardId::PiercingWail, None, true),
        ] {
            let mut root = super::super::tests::root(&[CardId::Adrenaline], 999);
            root.combat.zones.draw_pile = vec![
                CombatCard::for_scenario(card, 101),
                CombatCard::for_scenario(CardId::Wound, 102),
            ]
            .into();
            if artifact {
                sts_core::content::powers::store::set_powers_for(
                    &mut root.combat,
                    1,
                    vec![Power {
                        power_type: PowerId::Artifact,
                        amount: 1,
                        instance_id: None,
                        extra_data: 0,
                        payload: Default::default(),
                        just_applied: false,
                    }],
                );
            }
            let witness = vec![
                ClientInput::PlayCard {
                    card_index: 0,
                    target: None,
                },
                ClientInput::PlayCard {
                    card_index: 0,
                    target,
                },
                ClientInput::EndTurn,
            ];
            let expected = crate::shuffle_order::replay(&root, &witness)
                .unwrap()
                .pop()
                .unwrap();
            let unprotected = crate::shuffle_order::replay(&root, &[ClientInput::EndTurn])
                .unwrap()
                .pop()
                .unwrap();
            if artifact {
                assert_eq!(
                    expected.combat.entities.player.current_hp,
                    unprotected.combat.entities.player.current_hp
                );
                assert_eq!(expected.combat.get_power(1, PowerId::Artifact), 0);
            } else {
                assert!(
                    expected.combat.entities.player.current_hp
                        > unprotected.combat.entities.player.current_hp
                );
            }
            let node = Node::root(CombatSearchState::new(root.clone()).unwrap())
                .with_potion_limit(Some(0));
            let space = CombatSearchSpace::with_potion_limit(0);
            let config = SolverConfig {
                max_potion_uses: Some(0),
                ..Default::default()
            };
            let mut whole = TurnProbe::local_response(&node);
            let mut chunks = TurnProbe::local_response(&node);
            whole
                .advance(
                    1000,
                    &space,
                    &config,
                    &root,
                    Instant::now() + Duration::from_secs(5),
                )
                .unwrap();
            for work in 1..=1000 {
                chunks
                    .advance(
                        work,
                        &space,
                        &config,
                        &root,
                        Instant::now() + Duration::from_secs(5),
                    )
                    .unwrap();
                assert!(chunks.work <= work);
            }
            assert!(whole.result(1000)["complete"].as_bool().unwrap());
            assert!(whole.response_support_actions_admitted > 0);
            let best = whole.best_hp.as_ref().unwrap();
            assert_eq!(
                best.state.position().combat.entities.player.current_hp,
                expected.combat.entities.player.current_hp
            );
            assert_eq!(
                crate::shuffle_order::replay(&root, &best.inputs())
                    .unwrap()
                    .last(),
                Some(best.state.position())
            );
            assert_eq!(whole.result(1000), chunks.result(1000));
        }
    }

    #[test]
    fn cohort_context_uses_capture_settings_and_full_card_attributes() {
        use sts_core::runtime::combat::CombatCard;
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Strike, 3),
        ]
        .into();
        let make = |combat| {
            Node::root(
                CombatSearchState::new(CombatPosition::new(
                    sts_core::state::core::EngineState::CombatPlayerTurn,
                    combat,
                ))
                .unwrap(),
            )
        };
        let reference = make(combat.clone());
        combat.zones.draw_pile.swap(0, 1);
        let order_variant = make(combat.clone());
        combat.zones.hand[0].cost_for_turn = Some(0);
        let cheaper_hand = make(combat.clone());
        combat.zones.hand[0].cost_for_turn = None;
        combat
            .zones
            .exhaust_pile
            .push(combat.zones.draw_pile.pop().unwrap());
        let exhausted = make(combat);
        let pool = vec![reference.clone(), order_variant, cheaper_hand, exhausted];
        for (effects, compositions, expected) in [
            (false, false, vec![true, true, true, true]),
            (true, false, vec![true, true, false, true]),
            (true, true, vec![true, true, false, false]),
        ] {
            let cohort = RetentionCohort::new(
                0,
                &pool,
                &pool[..1],
                &reference,
                "next_turn",
                0,
                effects,
                compositions,
            );
            assert_eq!(
                cohort.nodes.len(),
                4,
                "context never merges exact order variants"
            );
            assert_eq!(
                cohort
                    .nodes
                    .iter()
                    .map(|n| cohort.shares_reference_context(n))
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn first_action_completes_card_selection_and_resumes_without_followup_or_potions() {
        use sts_core::runtime::combat::CombatCard;
        let mut root = crate::local::ConstructedCombat {
            seed: 37,
            encounter: EncounterId::JawWorm,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 68,
            deck: vec![(CardId::Strike, 0); 10],
            relics: vec![],
            potions: vec![PotionId::SwiftPotion],
            max_potion_uses: None,
        }
        .build()
        .unwrap();
        let mut grit = CombatCard::for_scenario(CardId::TrueGrit, 101);
        grit.upgrades = 1;
        root.combat.zones.hand = vec![
            grit,
            CombatCard::for_scenario(CardId::Defend, 102),
            CombatCard::for_scenario(CardId::AscendersBane, 103),
        ];
        let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let mut probe = TurnProbe::first_action(&node, false);
        let space = CombatSearchSpace::default();
        let config = SolverConfig::default();
        probe
            .advance(100, &space, &config, &root, Instant::now())
            .unwrap();
        assert_eq!(probe.work, 0);
        probe
            .advance(
                1,
                &space,
                &config,
                &root,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(probe.work, 1);
        assert!(!probe.result(1)["complete"].as_bool().unwrap());
        probe
            .advance(
                100,
                &space,
                &config,
                &root,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert!(probe.result(100)["complete"].as_bool().unwrap());
        assert_eq!(probe.endpoints, 3); // Defend, or True Grit selecting either other card.
        for witness in probe.score_witnesses.iter().flatten() {
            assert!(!actions::unfinished(witness));
            assert_eq!(
                witness.state.position().combat.turn.turn_count,
                node.state.position().combat.turn.turn_count
            );
            let inputs = witness.inputs();
            assert!(matches!(
                inputs.as_slice(),
                [ClientInput::PlayCard { card_index: 1, .. }]
                    | [
                        ClientInput::PlayCard { card_index: 0, .. },
                        ClientInput::SubmitSelection(_)
                    ]
            ));
            assert_eq!(
                witness
                    .state
                    .position()
                    .combat
                    .turn
                    .counters
                    .cards_played_this_turn,
                1,
                "a first-action witness cannot include a protection followup"
            );
            assert_eq!(witness.state.potion_uses(), 0);
            assert_eq!(
                crate::shuffle_order::replay(&root, &inputs).unwrap().last(),
                Some(witness.state.position())
            );
        }
        let mut uninterrupted = TurnProbe::first_action(&node, false);
        uninterrupted
            .advance(
                100,
                &space,
                &config,
                &root,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(probe.result(100), uninterrupted.result(100));
        assert_eq!(
            probe
                .score_witnesses
                .iter()
                .map(|w| w.as_ref().map(Node::inputs))
                .collect::<Vec<_>>(),
            uninterrupted
                .score_witnesses
                .iter()
                .map(|w| w.as_ref().map(Node::inputs))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn first_action_keeps_automatic_turn_crossings_out_of_same_turn_scores() {
        use sts_core::{content::powers::PowerId, runtime::combat::CombatCard};
        let mut root = crate::local::ConstructedCombat {
            seed: 37,
            encounter: EncounterId::TimeEater,
            act: 3,
            room: RoomType::MonsterRoomBoss,
            hp: 75,
            deck: vec![(CardId::Strike, 0); 10],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        root.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 101)];
        let id = root.combat.entities.monsters[0].id;
        root.combat
            .entities
            .power_db
            .get_mut(&id)
            .unwrap()
            .iter_mut()
            .find(|p| p.power_type == PowerId::TimeWarp)
            .unwrap()
            .amount = 11;
        let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let mut probe = TurnProbe::first_action(&node, true);
        probe
            .advance(
                100,
                &CombatSearchSpace::with_potion_limit(0),
                &SolverConfig::default(),
                &root,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert!(probe.result(100)["complete"].as_bool().unwrap());
        assert_eq!(probe.work, 1);
        assert_eq!(probe.crossed_turn, 1);
        assert_eq!(probe.endpoints, 0);
        assert!(probe.score_maxima.is_none());
    }

    #[test]
    fn current_hand_defense_scores_match_replayed_responses_and_fairy_is_not_free() {
        let base = crate::local::ConstructedCombat {
            seed: 37,
            encounter: EncounterId::JawWorm,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 68,
            deck: vec![(CardId::Strike, 0); 10],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        let config = SolverConfig {
            max_potion_uses: Some(0),
            ..Default::default()
        };
        let space = CombatSearchSpace::with_potion_limit(0);
        let mut results = vec![];
        let mut scores = vec![];
        for (cards, energy, fairy) in [
            (vec![CardId::Strike], 3, false),
            (vec![CardId::Defend], 3, false),
            (vec![CardId::Defend], 0, false),
            (vec![CardId::Defend; 2], 1, false),
            (vec![CardId::Defend; 2], 2, false),
            (vec![CardId::Strike], 3, true),
        ] {
            let mut root = base.clone();
            root.combat.turn.energy = energy;
            root.combat.zones.hand = cards
                .into_iter()
                .enumerate()
                .map(|(i, card)| {
                    sts_core::runtime::combat::CombatCard::for_scenario(card, 101 + i as u32)
                })
                .collect();
            if fairy {
                root.combat.entities.player.current_hp = 1;
                root.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
            }
            let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
            scores.push(retention::scores(&node));
            let mut probe = TurnProbe::new(&node, false);
            let deadline = Instant::now() + Duration::from_secs(5);
            probe.advance(1, &space, &config, &root, deadline).unwrap();
            if energy > 0 {
                assert!(!probe.result(1)["complete"].as_bool().unwrap());
            }
            probe
                .advance(100, &space, &config, &root, deadline)
                .unwrap();
            assert!(probe.result(100)["complete"].as_bool().unwrap());
            if fairy {
                assert!(probe.resource_rejected > 0);
                assert!(probe.best_hp.is_none());
                assert_eq!(probe.wins, 0);
            } else {
                let best = probe.best_hp.as_ref().unwrap();
                assert_eq!(
                    crate::shuffle_order::replay(&root, &best.inputs())
                        .unwrap()
                        .last(),
                    Some(best.state.position())
                );
                assert_eq!(
                    best.state.position().combat.turn.turn_count,
                    base.combat.turn.turn_count + 1
                );
                for (lane, witness) in probe.score_witnesses.iter().enumerate() {
                    let witness = witness.as_ref().unwrap();
                    assert_eq!(
                        retention::scores(witness)[lane],
                        probe.score_maxima.unwrap()[lane]
                    );
                    assert_eq!(
                        crate::shuffle_order::replay(&root, &witness.inputs())
                            .unwrap()
                            .last(),
                        Some(witness.state.position())
                    );
                }
                results.push(best.state.position().combat.entities.player.current_hp);
                assert_eq!(
                    scores.last().unwrap()[4],
                    i64::from(*results.last().unwrap()) * 10,
                    "in these direct, nonlethal fixtures the survival hint matches the fully replayed best response"
                );
            }
        }
        assert_eq!(scores[0][..4], scores[1][..4]);
        assert_eq!(results[1] - results[0], 5);
        assert_eq!(results[2], results[0], "zero Energy cannot pay for Defend");
        assert_eq!(
            results[3], results[1],
            "one Energy cannot pay for both Defends"
        );
        assert_eq!(results[4] - results[3], 5);
    }

    #[test]
    fn score_lanes_keep_their_distinct_reachable_responses() {
        use sts_core::runtime::combat::CombatCard;
        let mut root = crate::local::ConstructedCombat {
            seed: 37,
            encounter: EncounterId::JawWorm,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 68,
            deck: vec![(CardId::Strike, 0); 10],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        let mut attack = CombatCard::for_scenario(CardId::Strike, 101);
        attack.base_damage_override = Some(10);
        root.combat.zones.hand = vec![attack, CombatCard::for_scenario(CardId::Defend, 102)];
        root.combat
            .zones
            .exhaust_pile
            .push(CombatCard::for_scenario(CardId::BandageUp, 103));
        root.combat.turn.energy = 1;
        let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let mut probe = TurnProbe::new(&node, true);
        probe
            .advance(
                100,
                &CombatSearchSpace::with_potion_limit(0),
                &SolverConfig::default(),
                &root,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert!(probe.result(100)["complete"].as_bool().unwrap());
        assert!(
            probe.score_witnesses[0].as_ref().unwrap().state
                != probe.score_witnesses[1].as_ref().unwrap().state
        );
        for (lane, witness) in probe.score_witnesses.iter().enumerate() {
            let witness = witness.as_ref().unwrap();
            assert_eq!(
                retention::scores(witness)[lane],
                probe.score_maxima.unwrap()[lane]
            );
            assert_ne!(retention::scores(witness), probe.score_maxima.unwrap());
            assert_eq!(
                crate::shuffle_order::replay(&root, &witness.inputs())
                    .unwrap()
                    .last(),
                Some(witness.state.position())
            );
        }
        // With two energy both play orders reach the same score. Their exact
        // discard orders remain different, and the witness is only one of them.
        root.combat.turn.energy = 2;
        let node = Node {
            state: CombatSearchState::new(root.clone()).unwrap(),
            ..node
        };
        let mut probe = TurnProbe::new(&node, true);
        probe
            .advance(
                100,
                &CombatSearchSpace::with_potion_limit(0),
                &SolverConfig::default(),
                &root,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert!(probe.result(100)["complete"].as_bool().unwrap());
        assert!(probe.score_maxima_count.iter().all(|&count| count >= 2));
        assert!(
            probe.score_maxima_count[4] > probe.score_maxima_count[0],
            "Defend alone shares the resource maximum, without buying damage"
        );
    }
}

impl RetentionCohort {
    pub(super) fn new(
        reference_step: usize,
        pool: &[Node],
        kept: &[Node],
        reference: &Node,
        label: &str,
        at_work: usize,
        context_effects: bool,
        context_card_compositions: bool,
    ) -> Self {
        let mut nodes: Vec<Node> = vec![];
        let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut virtual_candidates = 0;
        for node in pool {
            if node.state.selected_members().is_some() {
                virtual_candidates += 1;
                continue;
            }
            let bucket = index.entry(node.index_key()).or_default();
            if bucket.iter().any(|&i| {
                nodes[i].state == node.state && nodes[i].expenditures() == node.expenditures()
            }) {
                continue;
            }
            bucket.push(nodes.len());
            nodes.push(node.clone());
        }
        Self {
            reference_step,
            context_effects,
            context_card_compositions,
            pool: label.into(),
            at_work,
            raw_candidates: pool.len(),
            virtual_candidates,
            reference: nodes
                .iter()
                .position(|n| {
                    n.state == reference.state && n.expenditures() == reference.expenditures()
                })
                .unwrap(),
            kept: nodes
                .iter()
                .map(|n| {
                    kept.iter()
                        .any(|k| k.state == n.state && k.expenditures() == n.expenditures())
                })
                .collect(),
            nodes,
        }
    }

    fn shares_reference_context(&self, node: &Node) -> bool {
        retention::same_retention_context(
            node,
            &self.nodes[self.reference],
            self.context_effects,
            self.context_card_compositions,
        )
    }
}

#[derive(Serialize)]
pub struct RetentionInspection {
    reference_step: usize,
    width: usize,
    card_only: bool,
    next_turn_work: Vec<usize>,
    continuation_work: usize,
    seconds_per_candidate_per_phase: f64,
    pool: String,
    at_work: usize,
    raw_candidates: usize,
    virtual_candidates_skipped: usize,
    unique_materialized_candidates: usize,
    reference: usize,
    context_effects: bool,
    context_card_compositions: bool,
    elapsed_ms: f64,
    candidates: Vec<Value>,
}

fn facts(node: &Node) -> Value {
    let c = &node.state.position().combat;
    let values = retention::evaluate(node, None);
    let card = |c: &sts_core::runtime::combat::CombatCard| format!("{:?}+{}", c.id, c.upgrades);
    json!({
        "turn":c.turn.turn_count + 1, "hp":c.entities.player.current_hp,
        "block":c.entities.player.block, "energy":c.turn.energy,
        "explicit_expenditures":node.expenditures(), "potions":c.entities.potions,
        "scores":values.lanes, "recovery":retention::recovery::profile(node, values.recovery),
        "draw_triggers":retention::draw_triggers::profile(c),
        "hand":c.zones.hand.iter().map(card).collect::<Vec<_>>(),
        "draw":c.zones.draw_pile.iter().map(card).collect::<Vec<_>>(),
        "discard":c.zones.discard_pile.iter().map(card).collect::<Vec<_>>(),
        "exhaust":c.zones.exhaust_pile.iter().map(card).collect::<Vec<_>>(),
        "powers":sts_core::content::powers::store::powers_for(c, c.entities.player.id),
        "monsters":c.entities.monsters.iter().map(|m| json!({
            "enemy":m.enemy_id,"hp":m.current_hp,"block":m.block,
            "powers":sts_core::content::powers::store::powers_for(c,m.id),
            "move":sts_core::sim::combat_projection::project_monster_move_preview_in_combat(c,m),
            "attack_request":sts_core::sim::combat_projection::project_monster_attack_request_in_combat(c,m)
        })).collect::<Vec<_>>()
    })
}

/// Breadth-first enumeration to a completed action or player-turn boundary.
/// No beam scores, heuristic pruning, rollout policy, or early victory stop.
/// Exact state AND explicit expenditure equality are required for deduplication.
struct TurnProbe {
    turn: u32,
    // Some(n) stops after one fully resolved play/choice, excluding EndTurn
    // and potion actions at admission. Virtual selection steps do not end it.
    first_action_from: Option<usize>,
    // A source-relative proposal scope. Draws and mandatory choices run
    // normally; newly exposed sources enter only through direct response
    // support requests. Neither admission nor coverage proves turn optimality.
    source_hand: Option<
        std::collections::HashSet<(
            sts_core::runtime::combat::CardLineageId,
            sts_core::content::cards::CardId,
        )>,
    >,
    source_hand_actions_excluded: usize,
    response_support_actions_admitted: usize,
    crossed_turn: usize,
    pass: Pass,
    active: VecDeque<Node>,
    current: Option<Expansion>,
    seen: HashMap<u64, Vec<Node>>,
    stats: SolverStats,
    work: usize,
    duplicates: usize,
    endpoints: usize,
    losses: usize,
    escapes: usize,
    wins: usize,
    best_win_hp: Option<i32>,
    score_maxima: Option<[i64; SCORE_LANES]>,
    /// Distinct exact endpoints attaining each observed maximum, including the
    /// first witness. A tie does not establish equivalent future play.
    score_maxima_count: [usize; SCORE_LANES],
    /// One actually reached endpoint per score lane, not a joint synthetic state.
    score_witnesses: [Option<Node>; SCORE_LANES],
    best_hp: Option<Node>,
    best_resource: Option<Node>,
    resource_rejected: usize,
}

// Keep banked HP and conditional healing separate even when this diagnostic
// compares their capped sum. This is a reachable response with a remaining
// request, not a promise that the request can be accessed or paid later.
fn response_resources(node: &Node) -> (i64, i32, i64) {
    let c = &node.state.position().combat;
    let hp = c.entities.player.current_hp;
    let reserve = retention::recovery_reserve(c);
    let capacity = i64::from((c.entities.player.max_hp - hp).max(0));
    (i64::from(hp) + reserve.min(capacity), hp, reserve)
}

/// Typed requests nominate a followup; the normal engine settles every actual
/// play. Reactive attacks, prevention and exhaust choices are not fabricated.
/// Other new-source plans (indirect draw, multi-card kills or setup)
/// remain outside this offline scope rather than being assigned a bad value.
fn offers_response_support(
    node: &Node,
    index: usize,
    target: Option<sts_core::EntityId>,
    sources: &std::collections::HashSet<(
        sts_core::runtime::combat::CardLineageId,
        sts_core::content::cards::CardId,
    )>,
) -> bool {
    use sts_core::content::cards::{
        can_play_card, can_play_card_ignoring_energy, get_card_definition, resolve_card_play,
        CardType,
    };
    use sts_core::runtime::action::Action;
    let c = &node.state.position().combat;
    let player = &c.entities.player;
    let pressure: i32 = c
        .entities
        .monsters
        .iter()
        .filter(|m| m.current_hp > 0 && !m.half_dead && !m.is_escaped)
        .map(|m| {
            sts_core::sim::combat_projection::project_monster_attack_request_in_combat(c, m)
                .map_or(0, |r| r.total_damage)
        })
        .sum();
    let needs_block = pressure > player.block;
    let needs_healing = player.current_hp < player.max_hp;
    let needs_energy = c.zones.hand.iter().any(|card| {
        sources.contains(&(card.lineage, card.id))
            && i32::from(card.get_cost()) > i32::from(c.turn.energy)
            && can_play_card(card, c).is_err()
            && can_play_card_ignoring_energy(card, c).is_ok()
    });
    resolve_card_play(c.zones.hand[index].id, c, &c.zones.hand[index], target)
        .iter()
        .any(|effect| match &effect.action {
            Action::GainBlock { target, amount } => {
                needs_block && *target == player.id && *amount > 0
            }
            Action::BlockPerNonAttack { block_per_card } => {
                needs_block
                    && *block_per_card > 0
                    && c.zones.hand.iter().enumerate().any(|(i, card)| {
                        i != index && get_card_definition(card.id).card_type != CardType::Attack
                    })
            }
            Action::Heal { target, amount } => needs_healing && *target == player.id && *amount > 0,
            Action::VampireDamageAllEnemies { damages, .. } => {
                needs_healing && damages.iter().any(|&amount| amount > 0)
            }
            Action::GainEnergy { amount } => needs_energy && *amount > 0,
            action @ Action::ApplyPower { .. } => {
                needs_block && super::tactics::attack_debuff_request(c, action)
            }
            _ => false,
        })
}

fn enemy_hp(node: &Node) -> i32 {
    node.state
        .position()
        .combat
        .entities
        .monsters
        .iter()
        .map(|m| m.current_hp.max(0))
        .sum()
}

impl TurnProbe {
    fn local_response(node: &Node) -> Self {
        Self {
            source_hand: Some(
                node.state
                    .position()
                    .combat
                    .zones
                    .hand
                    .iter()
                    .map(|card| (card.lineage, card.id))
                    .collect(),
            ),
            ..Self::new(node, true)
        }
    }

    fn first_action(node: &Node, card_only: bool) -> Self {
        Self {
            first_action_from: Some(node.input_count),
            ..Self::new(node, card_only)
        }
    }

    fn new(node: &Node, card_only: bool) -> Self {
        Self {
            turn: node.state.position().combat.turn.turn_count,
            first_action_from: None,
            source_hand: None,
            source_hand_actions_excluded: 0,
            response_support_actions_admitted: 0,
            crossed_turn: 0,
            pass: Pass::new(node.clone(), 1, card_only),
            active: VecDeque::from([node.clone()]),
            current: None,
            seen: HashMap::from([(node.index_key(), vec![node.clone()])]),
            stats: SolverStats::default(),
            work: 0,
            duplicates: 0,
            endpoints: 0,
            losses: 0,
            escapes: 0,
            wins: 0,
            best_win_hp: None,
            score_maxima: None,
            score_maxima_count: [0; SCORE_LANES],
            score_witnesses: std::array::from_fn(|_| None),
            best_hp: None,
            best_resource: None,
            resource_rejected: 0,
        }
    }

    fn advance(
        &mut self,
        allowance: usize,
        space: &CombatSearchSpace,
        config: &SolverConfig,
        origin: &CombatPosition,
        deadline: Instant,
    ) -> Result<(), String> {
        while self.work < allowance && Instant::now() < deadline {
            if self
                .current
                .as_ref()
                .is_none_or(|e| e.index == e.branches.len())
            {
                let Some(node) = self.active.pop_front() else {
                    self.current = None;
                    break;
                };
                let mut expansion = self
                    .pass
                    .prepare_node(node, space, config, &mut self.stats)?;
                if self.first_action_from.is_some() {
                    expansion.branches.retain(|&i| {
                        !matches!(
                            expansion.choices.branches()[i],
                            CombatBranch::Input(
                                ClientInput::EndTurn
                                    | ClientInput::UsePotion { .. }
                                    | ClientInput::DiscardPotion(_)
                            )
                        )
                    });
                }
                if let Some(sources) = &self.source_hand {
                    let before = expansion.branches.len();
                    expansion.branches.retain(|&i| {
                        let CombatBranch::Input(ClientInput::PlayCard { card_index, target }) =
                            &expansion.choices.branches()[i]
                        else {
                            return true;
                        };
                        let card = &expansion.node.state.position().combat.zones.hand[*card_index];
                        if sources.contains(&(card.lineage, card.id)) {
                            return true;
                        }
                        let support =
                            offers_response_support(&expansion.node, *card_index, *target, sources);
                        self.response_support_actions_admitted += usize::from(support);
                        support
                    });
                    self.source_hand_actions_excluded += before - expansion.branches.len();
                }
                self.current = Some(expansion);
            }
            let e = self.current.as_mut().unwrap();
            if e.index == e.branches.len() {
                continue;
            }
            let branch = e.branches[e.index];
            let input = e.choices.input(branch)?;
            let state = match e.choices.transition(
                branch,
                CombatStepLimits {
                    max_engine_steps: config.max_engine_steps_per_input,
                    deadline: Some(deadline),
                },
            )? {
                Transition::Settled {
                    state,
                    engine_steps,
                } => {
                    self.stats.engine_steps += engine_steps;
                    state
                }
                Transition::Interrupted {
                    cause: crate::TransitionInterruption::Deadline,
                    engine_steps,
                } => {
                    self.stats.engine_steps += engine_steps;
                    break;
                }
                Transition::Interrupted { .. } => return Err("inspection engine-step limit".into()),
            };
            self.work += 1;
            e.index += 1;
            let mut node = Node {
                state,
                card_baseline: e.node.card_baseline.clone(),
                potion_budget: e.node.potion_budget.clone(),
                trace: e.node.trace.clone(),
                input_count: e.node.input_count,
            };
            if let Some(input) = input {
                let mut expenditures = node.expenditures();
                expenditures.record(&input);
                node.trace = Some(Arc::new(Trace {
                    previous: node.trace,
                    input,
                    expenditures,
                }));
                node.input_count += 1;
            }
            if node.exceeds_potion_budget() {
                self.stats.resource_limit_states_skipped += 1;
                continue;
            }
            let bucket = self.seen.entry(node.index_key()).or_default();
            if bucket
                .iter()
                .any(|n| n.state == node.state && n.expenditures() == node.expenditures())
            {
                self.duplicates += 1;
                continue;
            }
            bucket.push(node.clone());
            let terminal = space.terminal(&node.state);
            let next_turn = node.state.position().combat.turn.turn_count > self.turn;
            let boundary = !actions::unfinished(&node)
                && self
                    .first_action_from
                    .map_or(next_turn, |start| node.input_count > start);
            if terminal == Some(true) || (terminal.is_none() && boundary) {
                if config.max_potion_uses.is_some_and(|max| {
                    project_observed_combat_outcome_with_expenditures_v1(
                        origin,
                        node.expenditures(),
                        node.state.position(),
                    )
                    .total_potion_expenditures as usize
                        > max
                }) {
                    self.resource_rejected += 1;
                    continue;
                }
            }
            match terminal {
                Some(false) => {
                    if node.state.position().combat.runtime.combat_smoked {
                        self.escapes += 1;
                    } else {
                        self.losses += 1;
                    }
                }
                Some(true) => {
                    self.wins += 1;
                    let hp = node.state.position().combat.entities.player.current_hp;
                    self.best_win_hp = Some(self.best_win_hp.map_or(hp, |before| before.max(hp)));
                    self.observe_resources(&node);
                }
                None if boundary && next_turn && self.first_action_from.is_some() => {
                    // An automatic end (e.g. Time Warp) is a different horizon;
                    // do not rank its next-turn score against in-turn responses.
                    self.crossed_turn += 1;
                }
                None if boundary => {
                    self.endpoints += 1;
                    self.observe_resources(&node);
                    let scores = retention::scores(&node);
                    for lane in 0..SCORE_LANES {
                        if self
                            .score_maxima
                            .is_none_or(|before| scores[lane] > before[lane])
                        {
                            self.score_witnesses[lane] = Some(node.clone());
                            self.score_maxima_count[lane] = 1;
                        } else if self
                            .score_maxima
                            .is_some_and(|before| scores[lane] == before[lane])
                        {
                            self.score_maxima_count[lane] += 1;
                        }
                    }
                    self.score_maxima = Some(self.score_maxima.map_or(scores, |before| {
                        std::array::from_fn(|i| before[i].max(scores[i]))
                    }));
                    let rank = |n: &Node| {
                        (
                            n.state.position().combat.entities.player.current_hp,
                            -enemy_hp(n),
                        )
                    };
                    if self
                        .best_hp
                        .as_ref()
                        .is_none_or(|before| rank(&node) > rank(before))
                    {
                        self.best_hp = Some(node);
                    }
                }
                None => self.active.push_back(node),
            }
        }
        Ok(())
    }

    fn observe_resources(&mut self, node: &Node) {
        let rank = |n: &Node| {
            let (capped, hp, _) = response_resources(n);
            (capped, hp, -enemy_hp(n))
        };
        if self
            .best_resource
            .as_ref()
            .is_none_or(|before| rank(node) > rank(before))
        {
            self.best_resource = Some(node.clone());
        }
    }

    fn result(&self, requested: usize) -> Value {
        json!({"requested_work":requested,"work":self.work,
            "engine_steps":self.stats.engine_steps,
            "remaining_branches":self.current.as_ref().map_or(0,|e|e.branches.len()-e.index),
            "complete":self.active.is_empty() && self.current.as_ref().is_none_or(|e|e.index==e.branches.len()),
            "open_nodes":self.active.len(),"exact_duplicates":self.duplicates,
            "horizon":if self.first_action_from.is_some() { "first_action" } else if self.source_hand.is_some() { "local_response" } else { "next_turn" },
            "proactive_play_scope":if self.source_hand.is_some() { "source_hand_and_direct_response_support" } else { "all_admitted_sources" },
            "source_hand_actions_excluded":self.source_hand_actions_excluded,
            "response_support_actions_admitted":self.response_support_actions_admitted,
            "next_turn_endpoints":if self.first_action_from.is_none() { self.endpoints } else { 0 },
            "same_turn_action_endpoints":if self.first_action_from.is_some() { self.endpoints } else { 0 },
            "automatic_turn_crossings":self.crossed_turn,"losses":self.losses,"escapes":self.escapes,
            "wins":self.wins,"best_win_hp":self.best_win_hp,"score_maxima":self.score_maxima,
            "score_maxima_count":self.score_maxima_count,
            "resource_rejected":self.resource_rejected,
            "best_joint_resource":self.best_resource.as_ref().map(|node| {
                let (capped, hp, reserve) = response_resources(node);
                json!({"realized_hp":hp,"conditional_healing_request":reserve,
                    "capped_hp_plus_request":capped,"enemy_hp":enemy_hp(node)})
            }),
            "best_hp":self.best_hp.as_ref().map(|n|n.state.position().combat.entities.player.current_hp),
            "enemy_hp_at_best_hp":self.best_hp.as_ref().map(enemy_hp)})
    }
}

impl CombatSolver {
    /// Offline, separately budgeted diagnosis. Reads a captured cohort without
    /// advancing or guiding the original search. No state is fed back to it.
    /// Each candidate receives the same work milestones and its own time cap.
    pub fn inspect_retention(
        &self,
        milestones: &[usize],
        per_candidate: Duration,
        continuation_work: usize,
    ) -> Result<Option<RetentionInspection>, String> {
        if milestones.is_empty()
            || milestones[0] == 0
            || milestones.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("inspection work must be positive and increasing".into());
        }
        let Some((pass, cohort)) = self
            .beams
            .iter()
            .flat_map(|b| &b.passes)
            .find_map(|p| p.audit.as_ref()?.cohort.as_ref().map(|c| (p, c)))
        else {
            return Ok(None);
        };
        let start = Instant::now();
        let mut candidates = vec![];
        for (i, node) in cohort.nodes.iter().enumerate() {
            // Validate every materialized capture from the original root. This
            // is diagnostic setup, reported separately from simulation work.
            let inputs = node.inputs();
            let mut validation_work = inputs.len();
            if crate::shuffle_order::replay(&self.root, &inputs)?.last()
                != Some(node.state.position())
            {
                return Err("captured retention prefix does not replay exactly".into());
            }
            let candidate_start = Instant::now();
            let mut first = TurnProbe::first_action(node, pass.card_only);
            let first_work = *milestones.last().unwrap();
            first.advance(
                first_work,
                &self.space,
                &self.config,
                &self.root,
                Instant::now() + per_candidate,
            )?;
            let mut first_witnesses = vec![];
            for (lane, witness) in first.score_witnesses.iter().enumerate() {
                if let Some(witness) = witness {
                    validation_work += witness.input_count;
                    if crate::shuffle_order::replay(&self.root, &witness.inputs())?.last()
                        != Some(witness.state.position())
                    {
                        return Err("first-action response does not replay exactly".into());
                    }
                    first_witnesses.push(
                        json!({"lane":lane,"endpoint":facts(witness),"inputs":witness.inputs()}),
                    );
                }
            }
            let first_elapsed = candidate_start.elapsed();
            let response_start = Instant::now();
            let mut response = TurnProbe::local_response(node);
            let mut response_measured = vec![];
            for &work in milestones {
                response.advance(
                    work,
                    &self.space,
                    &self.config,
                    &self.root,
                    response_start + per_candidate,
                )?;
                response_measured.push(response.result(work));
            }
            let response_compute_elapsed = response_start.elapsed();
            let response_validation = response.best_resource.as_ref().map_or(0, |n| n.input_count);
            if let Some(best) = &response.best_resource {
                validation_work += response_validation;
                if crate::shuffle_order::replay(&self.root, &best.inputs())?.last()
                    != Some(best.state.position())
                {
                    return Err("local response does not replay exactly".into());
                }
            }
            let response_elapsed = response_start.elapsed();
            let mut probe = TurnProbe::new(node, pass.card_only);
            let turn_start = Instant::now();
            let mut measured = vec![];
            for &work in milestones {
                probe.advance(
                    work,
                    &self.space,
                    &self.config,
                    &self.root,
                    turn_start + per_candidate,
                )?;
                measured.push(probe.result(work));
            }
            let turn_compute_elapsed = turn_start.elapsed();
            let best = probe.best_hp.as_ref();
            // Only the single EndTurn case: no guessed end-turn hook sequence,
            // no extra combat search and no inferred future draw batches.
            let shuffle_observation = best
                .filter(|best| {
                    node.state.position().combat.zones.draw_pile.is_empty()
                        && best.input_count == node.input_count + 1
                        && best
                            .trace
                            .as_ref()
                            .is_some_and(|t| t.input == ClientInput::EndTurn)
                })
                .map(|best| {
                    match crate::shuffle_order::mapping(
                        node.state.position(),
                        &ClientInput::EndTurn,
                        best.state.position(),
                    ) {
                        Ok(map) => json!({"status":"verified", "receipt":map.receipt,
                        "draw_to_input":map.draw_to_input, "drawn":map.drawn}),
                        Err(reason) => json!({"status":"unavailable", "reason":reason}),
                    }
                });
            if let Some(best) = best {
                validation_work += best.input_count;
                if crate::shuffle_order::replay(&self.root, &best.inputs())?.last()
                    != Some(best.state.position())
                {
                    return Err("inspection endpoint does not replay exactly".into());
                }
            }
            if let Some(best) = &probe.best_resource {
                validation_work += best.input_count;
                if crate::shuffle_order::replay(&self.root, &best.inputs())?.last()
                    != Some(best.state.position())
                {
                    return Err("inspection joint-resource response does not replay exactly".into());
                }
            }
            // Export each distinct lane winner once. These are the endpoints
            // observed at the final probe milestone, not every earlier budget.
            let mut score_witnesses: Vec<(&Node, Vec<usize>)> = vec![];
            for (lane, witness) in probe.score_witnesses.iter().enumerate() {
                let Some(witness) = witness else { continue };
                if let Some((_, lanes)) = score_witnesses.iter_mut().find(|(node, _)| {
                    node.state == witness.state && node.expenditures() == witness.expenditures()
                }) {
                    lanes.push(lane);
                    continue;
                }
                validation_work += witness.input_count;
                if crate::shuffle_order::replay(&self.root, &witness.inputs())?.last()
                    != Some(witness.state.position())
                {
                    return Err("inspection score witness does not replay exactly".into());
                }
                score_witnesses.push((witness, vec![lane]));
            }
            let continuation = if continuation_work > 0 {
                let mut config = self.config.clone();
                config.max_potion_uses = if pass.card_only {
                    Some(0)
                } else {
                    node.remaining_potion_budget()
                };
                config.shuffle_continuation_work = 0;
                let mut child = CombatSolver::new(node.state.position().clone(), config)?;
                let start = Instant::now();
                let stop = child.advance(continuation_work, per_candidate)?;
                let mut witnesses = vec![];
                for plan in child.plans() {
                    let mut whole = inputs.clone();
                    whole.extend_from_slice(&plan.inputs);
                    validation_work += whole.len();
                    if crate::shuffle_order::replay(&self.root, &whole)?.last()
                        != Some(&plan.successor)
                    {
                        return Err(
                            "inspection continuation does not replay from original root".into()
                        );
                    }
                    let outcome = materialize_replay_checked_combat_outcome_v1(
                        &self.root,
                        &whole,
                        &plan.successor,
                    )?;
                    if self
                        .config
                        .max_potion_uses
                        .is_none_or(|max| outcome.total_potion_expenditures as usize <= max)
                    {
                        witnesses.push(json!({"inputs":whole,"outcome":outcome}));
                    }
                }
                Some(
                    json!({"config":child.config,"stop":stop,"stats":child.stats(),
                    "elapsed_ms":start.elapsed().as_secs_f64()*1000.,"witnesses":witnesses}),
                )
            } else {
                None
            };
            candidates.push(
                json!({"candidate":i,"kept":cohort.kept[i],"reference":i==cohort.reference,
                "shares_reference_context":cohort.shares_reference_context(node),
                "root":facts(node),"inputs":inputs,"milestones":measured,
                "observed_boundaries_after_search":self.graph.retention_guide(pass.card_only, self.config.max_potion_uses).observed_boundaries(node),
                "first_action":{"measurement":first.result(first_work),"witnesses":first_witnesses,"elapsed_ms":first_elapsed.as_secs_f64()*1000.},
                "local_response":{"milestones":response_measured,
                    "elapsed_ms":response_elapsed.as_secs_f64()*1000.,
                    "compute_elapsed_ms":response_compute_elapsed.as_secs_f64()*1000.,
                    "validation_replay_inputs":response_validation,
                    "best_joint_endpoint":response.best_resource.as_ref().map(facts),
                    "best_joint_inputs":response.best_resource.as_ref().map(Node::inputs)},
                "elapsed_ms":candidate_start.elapsed().as_secs_f64()*1000.,
                "next_turn_compute_elapsed_ms":turn_compute_elapsed.as_secs_f64()*1000.,
                "validation_replay_inputs":validation_work,
                "shuffle_observation_inputs":usize::from(shuffle_observation.is_some()),
                "end_turn_shuffle":shuffle_observation,
                "continuation":continuation,
                "score_witnesses":score_witnesses.iter().map(|(node, lanes)| json!({
                    "lanes":lanes,"endpoint":facts(node),"inputs":node.inputs()
                })).collect::<Vec<_>>(),
                "best_joint_endpoint":probe.best_resource.as_ref().map(facts),
                "best_joint_inputs":probe.best_resource.as_ref().map(Node::inputs),
                "best_hp_endpoint":best.map(facts),"best_hp_inputs":best.map(Node::inputs)}),
            );
        }
        Ok(Some(RetentionInspection {
            reference_step: cohort.reference_step,
            width: pass.width,
            card_only: pass.card_only,
            next_turn_work: milestones.to_vec(),
            continuation_work,
            seconds_per_candidate_per_phase: per_candidate.as_secs_f64(),
            pool: cohort.pool.clone(),
            at_work: cohort.at_work,
            raw_candidates: cohort.raw_candidates,
            virtual_candidates_skipped: cohort.virtual_candidates,
            unique_materialized_candidates: cohort.nodes.len(),
            reference: cohort.reference,
            context_effects: cohort.context_effects,
            context_card_compositions: cohort.context_card_compositions,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
            candidates,
        }))
    }
}
