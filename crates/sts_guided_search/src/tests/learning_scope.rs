//! Short, constructed transition probes for designing a broader learner input.
//! These exercise the shared executor, not a trained policy or a complete battle.
use super::*;
use crate::observation::{combat_observation, model_choices, CombatObservation, ModelChoice};
use serde_json::{json, Value};
use sts_core::content::monsters::factory::EncounterId;
use sts_core::content::relics::RelicId;
use sts_core::sim::combat_start::build_natural_combat_start;
use sts_core::state::{map::node::RoomType, run::RunState};

struct Probe {
    name: &'static str,
    construction: Value,
    root: CombatPosition,
    session: SearchSession<CombatSearchSpace>,
    source: NodeRef,
    boundaries: Vec<Value>,
}

impl Probe {
    fn new(
        name: &'static str,
        deck: Option<&[(CardId, u8)]>,
        enemy: EncounterId,
        room: RoomType,
        relic: Option<RelicId>,
        potion: Option<PotionId>,
    ) -> Self {
        let mut run = RunState::new(73, 20, true, "Ironclad");
        run.current_hp = 68;
        if let Some(deck) = deck {
            let original: Vec<_> = run.master_deck.iter().map(|c| c.lineage).collect();
            for lineage in original {
                run.remove_card_from_deck(lineage);
            }
            assert!(run.master_deck.is_empty());
            for &(card, upgrades) in deck {
                assert!(run.add_card_to_deck_with_upgrades(card, upgrades));
            }
        }
        if let Some(relic) = relic {
            assert!(run
                .obtain_relic(relic, EngineState::CombatPlayerTurn)
                .is_none());
        }
        if let Some(potion) = potion {
            assert!(run.obtain_potion(Potion::new(potion, 0)));
        }
        let construction = json!({"seed":73, "ascension":20, "starting_hp":68,
            "enemy":enemy, "room":room, "deck":run.master_deck,
            "relics":run.relics, "potions":run.potions});
        let (engine, combat) = build_natural_combat_start(&mut run, enemy, room).unwrap();
        let root = CombatPosition::new(engine, combat);
        let session = SearchSession::new(
            CombatSearchSpace::default(),
            CombatSearchState::new(root.clone()).unwrap(),
            SearchConfig::default(),
            SearchBudget {
                max_services: 64,
                max_engine_steps: 16_384,
                max_nodes: 65,
                max_depth: 64,
                deadline: None,
                guidance: GuidanceBudget::default(),
            },
        )
        .unwrap();
        let source = session.root();
        let mut probe = Self {
            name,
            construction,
            root,
            session,
            source,
            boundaries: vec![],
        };
        probe.record();
        probe
    }

    fn observation(&self) -> CombatObservation {
        combat_observation(self.session.view(self.source).unwrap().state).unwrap()
    }

    fn record(&mut self) {
        let view = self.session.view(self.source).unwrap();
        let choices = model_choices(view.state, &view.actions).unwrap();
        assert_eq!(choices.len(), view.actions.len());
        let entities = crate::entities::encode(&self.session, self.source).unwrap();
        assert_eq!(entities.actions.len(), choices.len());
        self.boundaries.push(
            json!({"observation":self.observation(), "choices":choices, "entities":entities,
            "schema_2_error":crate::local_tactics::encode(&self.session, self.source).err()}),
        );
    }

    fn act(&mut self, select: impl Fn(&CombatObservation, &ModelChoice) -> bool) {
        let view = self.session.view(self.source).unwrap();
        let choices = model_choices(view.state, &view.actions).unwrap();
        let observation = self.observation();
        let index = choices
            .iter()
            .position(|c| select(&observation, c))
            .unwrap_or_else(|| panic!("{}: requested probe action unavailable", self.name));
        let mut weights = vec![0.0; choices.len()];
        weights[index] = 1.0;
        self.session
            .submit_guidance(
                self.source,
                Guidance {
                    weights: Some(weights),
                    value: None,
                },
            )
            .unwrap();
        self.source = match self.session.follow_guidance(self.source).unwrap() {
            StepStatus::NeedsGuidance(next) | StepStatus::Guided(next) => next,
            other => panic!("{}: unexpected probe boundary {other:?}", self.name),
        };
        self.record();
    }

    fn play(&mut self, card: CardId) {
        // Select one current, source-bound instance; this is a probe, not a teacher.
        self.act(|o, c| matches!(c, ModelChoice::Play {hand, ..} if o.hand[*hand].card == card));
    }

    fn finish(self, output: Option<&std::path::Path>) {
        let trace = self.session.observed_trace(self.source).unwrap();
        let mut replay = self.root.clone();
        for step in &trace {
            assert!(EngineCombatStepper.is_legal_action(&replay, &step.input));
            let next = EngineCombatStepper.apply_to_stable(
                &replay,
                step.input.clone(),
                CombatStepLimits {
                    max_engine_steps: 256,
                    deadline: None,
                },
            );
            assert!(!next.truncated && !next.timed_out);
            assert_eq!(
                next.position, step.successor,
                "{}: prefix replay",
                self.name
            );
            replay = next.position;
        }
        assert_eq!(
            &replay,
            self.session.view(self.source).unwrap().state.position()
        );
        let accepted = self
            .boundaries
            .iter()
            .filter(|b| b["schema_2_error"].is_null())
            .count();
        if let Some(output) = output {
            let report = json!({"schema":1, "name":self.name,
                "provenance":"constructed_transition_probe", "construction":self.construction,
                "root":self.root, "boundaries":self.boundaries, "inputs":trace,
                "prefix_replayed":true, "scope":"declared transitions only"});
            let file = std::fs::File::options()
                .write(true)
                .create_new(true)
                .open(output.join(format!("{}.json", self.name)))
                .unwrap();
            serde_json::to_writer(file, &report).unwrap();
        }
        println!(
            "{}: {} inputs replayed; schema 2 accepted {}/{} boundaries",
            self.name,
            trace.len(),
            accepted,
            self.boundaries.len()
        );
    }
}

fn card_count(o: &CombatObservation, card: CardId) -> usize {
    o.hand
        .iter()
        .chain(&o.draw_top_first)
        .chain(&o.discard)
        .chain(&o.exhaust)
        .chain(&o.limbo)
        .filter(|c| c.card == card)
        .count()
}

#[test]
#[ignore = "explicit learning-scope diagnosis; optional fresh STS_LEARNING_SCOPE_OUTPUT directory"]
fn learning_scope_probes() {
    use CardId::*;
    let output = std::env::var_os("STS_LEARNING_SCOPE_OUTPUT").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        assert!(output.is_absolute());
        std::fs::create_dir(output).expect("probe output must be a new directory");
    }
    let output = output.as_deref();
    let make = |name, deck: Option<&[(CardId, u8)]>, relic, potion| {
        Probe::new(
            name,
            deck,
            EncounterId::JawWorm,
            RoomType::MonsterRoom,
            relic,
            potion,
        )
    };

    let mut control = make("starter_control", None, None, None);
    control.act(|_, c| matches!(c, ModelChoice::EndTurn {}));
    assert!(crate::local_tactics::encode(&control.session, control.source).is_ok());
    control.finish(output);

    let large = vec![(Strike, 0); 20];
    let mut size = make("twenty_card_deck", Some(&large), None, None);
    size.act(|_, c| matches!(c, ModelChoice::EndTurn {}));
    assert_eq!(card_count(&size.observation(), Strike), 20);
    size.finish(output);

    let mut strength = make(
        "strength_scaling",
        Some(&[(Inflame, 0), (HeavyBlade, 0), (Defend, 0)]),
        None,
        None,
    );
    strength.play(Inflame);
    assert!(strength
        .observation()
        .powers
        .iter()
        .any(|p| p.power == PowerId::Strength && p.amount == Some(2)));
    assert!(strength
        .observation()
        .hand
        .iter()
        .any(|c| c.card == HeavyBlade && c.damage_by_target == [20]));
    strength.play(HeavyBlade);
    strength.finish(output);

    let mut exhaust = make(
        "exhaust_defense_selection",
        Some(&[
            (Corruption, 1),
            (FeelNoPain, 0),
            (BurningPact, 0),
            (Strike, 0),
            (Defend, 0),
        ]),
        None,
        None,
    );
    exhaust.play(FeelNoPain);
    exhaust.play(Corruption);
    exhaust.play(BurningPact);
    exhaust.act(|_, c| matches!(c, ModelChoice::BeginSelection { .. }));
    exhaust.act(|_, c| matches!(c, ModelChoice::SelectMember { .. }));
    exhaust.act(|_, c| matches!(c, ModelChoice::FinishSelection {}));
    assert_eq!(exhaust.observation().block, 6);
    assert_eq!(exhaust.observation().exhaust.len(), 2);
    exhaust.finish(output);

    let mut wounds = make(
        "generated_status",
        Some(&[(PowerThrough, 0), (Strike, 0), (Defend, 0)]),
        None,
        None,
    );
    wounds.play(PowerThrough);
    assert_eq!(card_count(&wounds.observation(), Wound), 2);
    assert_eq!(wounds.observation().block, 15);
    wounds.finish(output);

    let mut relic = make(
        "relic_counter",
        Some(&[(Strike, 0), (Strike, 0), (Strike, 0), (Defend, 0)]),
        Some(RelicId::Shuriken),
        None,
    );
    for _ in 0..3 {
        relic.play(Strike);
    }
    assert!(relic
        .observation()
        .powers
        .iter()
        .any(|p| p.power == PowerId::Strength && p.amount == Some(1)));
    assert!(relic.boundaries.iter().any(|b| b["observation"]["relics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["relic"] == "Shuriken" && r["counter"] == 2)));
    relic.finish(output);

    let mut discovery = make("potion_discovery", None, None, Some(PotionId::PowerPotion));
    discovery.act(|_, c| matches!(c, ModelChoice::UsePotion { slot: 0, .. }));
    assert!(discovery.observation().indexed_choice_reason.is_some());
    let before = discovery.observation();
    let count = before.hand.len() + before.draw_top_first.len() + before.discard.len();
    discovery.act(|_, c| matches!(c, ModelChoice::Choose { .. }));
    let after = discovery.observation();
    assert_eq!(
        after.hand.len() + after.draw_top_first.len() + after.discard.len(),
        count + 1
    );
    assert!(after
        .hand
        .iter()
        .any(|c| matches!(c.cost, crate::observation::CardCost::Energy(0))));
    assert!(after.potions.iter().all(Option::is_none));
    discovery.finish(output);

    let mut sentries = Probe::new(
        "elite_status",
        None,
        EncounterId::ThreeSentries,
        RoomType::MonsterRoomElite,
        None,
        None,
    );
    sentries.act(|_, c| matches!(c, ModelChoice::EndTurn {}));
    assert!(card_count(&sentries.observation(), Dazed) > 0);
    assert_eq!(sentries.observation().monsters.len(), 3);
    sentries.finish(output);
}
