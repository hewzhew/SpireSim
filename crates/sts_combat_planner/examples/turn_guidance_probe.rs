//! Constructed, fixed-input ordering diagnostics. No run actor or Java control.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path, sync::Arc, time::Instant};
use sts_combat_planner::{
    materialize_exact_action_line, replay_turn_option, CombatActionPolicy, CombatDecisionRoot,
    CombatGuideLaneId, CombatPlanningQuantum, CombatPolicyChoice, CombatSelectionPreferences,
    CombatStateGuide, ReplayLimits, TurnOptionGeneratorConfig, TurnOptionGeneratorSession,
};
use sts_core::{
    content::{
        cards::CardId,
        monsters::EnemyId,
        potions::{Potion, PotionId},
        powers::{store, PowerId},
    },
    runtime::{
        combat::{CombatCard, CombatRng, Power, PowerPayload},
        rng::RngPool,
    },
    sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper},
    sim::combat_action_surface::{CombatSelectionActionFamilyV2, CombatSelectionReasonV2},
    state::{
        core::{ClientInput, EngineState, HandSelectReason},
        selection::{SelectionResolution, SelectionScope},
    },
    test_support::{blank_test_combat, planned_monster},
};

const WORK: usize = 8192;
const STEPS: usize = 524_288;
const TRANSITION: usize = 512;

#[derive(Clone, Copy, Default)]
struct ProbePolicy {
    depth: bool,
    exhaust_sentinel: bool,
    gamble: GamblePreference,
}

#[derive(Clone, Copy, Default)]
enum GamblePreference {
    #[default]
    None,
    Wounds,
    All,
}

impl ProbePolicy {
    fn name(self) -> &'static str {
        match self.gamble {
            GamblePreference::Wounds => return "replace_wounds",
            GamblePreference::All => return "replace_all",
            GamblePreference::None => {}
        }
        match (self.depth, self.exhaust_sentinel) {
            (false, false) => "uniform",
            (true, false) => "depth",
            (false, true) => "sentinel_preference",
            (true, true) => "depth_and_sentinel",
        }
    }
}

impl CombatActionPolicy for ProbePolicy {
    fn weights(&self, _: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        vec![1.0; choices.len()]
    }
    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> CombatSelectionPreferences {
        if matches!(self.gamble, GamblePreference::None)
            || family.reason != CombatSelectionReasonV2::Hand(HandSelectReason::GamblingChip)
        {
            return CombatSelectionPreferences::default();
        }
        // Only the current hand is used. The counterexample keeps a finisher
        // which the deliberately wrong All preference throws away.
        let lineages = position
            .combat
            .zones
            .hand
            .iter()
            .filter(|card| matches!(self.gamble, GamblePreference::All) || card.id == CardId::Wound)
            .map(|card| card.lineage);
        CombatSelectionPreferences::new([ClientInput::SubmitSelection(
            SelectionResolution::card_lineages(SelectionScope::Hand, lineages),
        )])
    }
    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        if self.depth {
            vec![CombatStateGuide::new(
                CombatGuideLaneId::new(900),
                vec![i32::from(
                    position.combat.turn.counters.cards_played_this_turn,
                )],
            )]
        } else {
            vec![]
        }
    }

    fn structured_selection_member_weights(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        // An explicit experimental preference, including in the counterexample
        // where consuming Sentinel loses the only available defense. It does
        // not inspect the witness, future draw order or desired outcome.
        if !self.exhaust_sentinel
            || family.reason != CombatSelectionReasonV2::Hand(HandSelectReason::Exhaust)
        {
            return vec![1.0; members.len()];
        }
        members
            .iter()
            .map(|input| {
                if let ClientInput::SubmitSelection(resolution) = input {
                    if resolution.selected_card_lineages().iter().any(|lineage| {
                        position
                            .combat
                            .zones
                            .hand
                            .iter()
                            .any(|card| card.lineage == *lineage && card.id == CardId::Sentinel)
                    }) {
                        return 100.0;
                    }
                }
                1.0
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Goal {
    Victory(i32),
    NextTurn { hp: i32, power: Option<PowerId> },
}
impl Goal {
    fn accepts(self, root: &CombatPosition, successor: &CombatPosition) -> bool {
        let hp = successor.combat.entities.player.current_hp;
        match self {
            Self::Victory(minimum) => {
                hp >= minimum && EngineCombatStepper.is_exact_victory(successor)
            }
            Self::NextTurn { hp: minimum, power } => {
                hp >= minimum
                    && successor.engine == EngineState::CombatPlayerTurn
                    && successor.combat.turn.turn_count > root.combat.turn.turn_count
                    && power.is_none_or(|id| {
                        store::has_power(&successor.combat, successor.combat.entities.player.id, id)
                    })
            }
        }
    }
    fn description(self) -> Value {
        match self {
            Self::Victory(hp) => json!({"boundary":"victory","minimum_hp":hp}),
            Self::NextTurn { hp, power } => {
                json!({"boundary":"next_turn","minimum_hp":hp,"power":power})
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Step {
    Play(CardId, bool),
    Select(CardId),
    SelectAll,
    End,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    LongFinish,
    ShortFinishThorns,
    BashBeforeAttacks,
    RageDefense,
    PactSentinel,
    PactKeepSentinel,
    DrawBeforeTrance,
    ShuffleDraw,
    GamblersSelection,
    GamblersKeepFinisher,
    PowerSetup,
    StopThorns,
}

impl Scenario {
    fn id(self) -> &'static str {
        match self {
            Self::LongFinish => "long_finish",
            Self::ShortFinishThorns => "short_finish_thorns",
            Self::BashBeforeAttacks => "bash_before_attacks",
            Self::RageDefense => "rage_defense",
            Self::PactSentinel => "pact_sentinel",
            Self::PactKeepSentinel => "pact_keep_sentinel",
            Self::DrawBeforeTrance => "draw_before_trance",
            Self::ShuffleDraw => "shuffle_draw",
            Self::GamblersSelection => "gamblers_selection",
            Self::GamblersKeepFinisher => "gamblers_keep_finisher",
            Self::PowerSetup => "power_setup",
            Self::StopThorns => "stop_thorns",
        }
    }
}

struct Case {
    kind: Scenario,
    name: &'static str,
    position: CombatPosition,
    goal: Goal,
    witness: Vec<Step>,
    preparation: Vec<sts_combat_planner::TurnOptionAction>,
    preparation_root: Option<CombatPosition>,
}

fn cases(variant: usize) -> Vec<Case> {
    use CardId::*;
    use Step::*;
    let specifications = [
        (
            Scenario::LongFinish,
            vec![Anger; 6],
            36,
            Goal::Victory(40),
            vec![Play(Anger, true); 6],
        ),
        (
            Scenario::ShortFinishThorns,
            [vec![Anger; 6], vec![Bludgeon]].concat(),
            32,
            Goal::Victory(39),
            vec![Play(Bludgeon, true)],
        ),
        (
            Scenario::BashBeforeAttacks,
            [vec![Bash], vec![Anger; 4]].concat(),
            44,
            Goal::Victory(40),
            [vec![Play(Bash, true)], vec![Play(Anger, true); 4]].concat(),
        ),
        (
            Scenario::RageDefense,
            [vec![Rage], vec![Anger; 6]].concat(),
            1000,
            Goal::NextTurn {
                hp: 40,
                power: None,
            },
            [
                vec![Play(Rage, false)],
                vec![Play(Anger, true); 4],
                vec![End],
            ]
            .concat(),
        ),
        (
            Scenario::PactSentinel,
            [vec![BurningPact, Sentinel], vec![Anger; 4]].concat(),
            32,
            Goal::Victory(40),
            vec![
                Play(BurningPact, false),
                Select(Sentinel),
                Play(Bludgeon, true),
            ],
        ),
        (
            Scenario::PactKeepSentinel,
            vec![BurningPact, Sentinel, Wound],
            1000,
            Goal::NextTurn { hp: 1, power: None },
            vec![Select(Wound), Play(Sentinel, false), End],
        ),
        (
            Scenario::DrawBeforeTrance,
            vec![BattleTrance, FlashOfSteel, Anger],
            35,
            Goal::Victory(40),
            vec![
                Play(FlashOfSteel, true),
                Play(BattleTrance, false),
                Play(Bludgeon, true),
            ],
        ),
        (
            Scenario::ShuffleDraw,
            vec![BattleTrance, FlashOfSteel, Anger],
            35,
            Goal::Victory(40),
            vec![
                Play(FlashOfSteel, true),
                Play(BattleTrance, false),
                Play(Bludgeon, true),
            ],
        ),
        (
            Scenario::GamblersSelection,
            vec![Wound; 6],
            32,
            Goal::Victory(40),
            vec![SelectAll, Play(Bludgeon, true)],
        ),
        (
            Scenario::GamblersKeepFinisher,
            [vec![Bludgeon], vec![Wound; 5]].concat(),
            32,
            Goal::Victory(40),
            vec![Select(Wound), Play(SeeingRed, false), Play(Bludgeon, true)],
        ),
        (
            Scenario::PowerSetup,
            [vec![DemonForm], vec![Anger; 6]].concat(),
            1000,
            Goal::NextTurn {
                hp: 40,
                power: Some(PowerId::DemonForm),
            },
            vec![Play(DemonForm, false), End],
        ),
        (
            Scenario::StopThorns,
            vec![Anger; 6],
            1000,
            Goal::NextTurn {
                hp: 40,
                power: None,
            },
            vec![End],
        ),
    ];
    specifications
        .into_iter()
        .map(|(kind, hand, monster_hp, goal, witness)| {
            let name = kind.id();
            let mut combat = blank_test_combat();
            combat.entities.player.current_hp = 40;
            combat.entities.player.max_hp = 40;
            let mut enemy = planned_monster(
                EnemyId::JawWorm,
                if matches!(kind, Scenario::RageDefense | Scenario::PactKeepSentinel) {
                    1
                } else {
                    2
                },
            );
            enemy.current_hp = monster_hp;
            enemy.max_hp = monster_hp;
            combat.entities.monsters.push(enemy);
            combat.zones.hand = hand
                .into_iter()
                .enumerate()
                .map(|(i, id)| {
                    let mut card = CombatCard::for_scenario(id, (i + 1) as u32);
                    if matches!(id, Bludgeon | Sentinel) {
                        card.upgrades = 1;
                    }
                    card
                })
                .collect();
            let rotation = variant % combat.zones.hand.len();
            combat.zones.hand.rotate_left(rotation);
            let mut draw = vec![];
            if matches!(
                kind,
                Scenario::DrawBeforeTrance | Scenario::ShuffleDraw | Scenario::GamblersSelection
            ) {
                draw.extend((101..104).map(|id| CombatCard::for_scenario(Wound, id)));
            }
            if matches!(
                kind,
                Scenario::PactSentinel
                    | Scenario::DrawBeforeTrance
                    | Scenario::ShuffleDraw
                    | Scenario::GamblersSelection
            ) {
                let mut finisher = CombatCard::for_scenario(Bludgeon, 104);
                finisher.upgrades = 1;
                draw.push(finisher);
            }
            if kind == Scenario::ShuffleDraw {
                combat.zones.discard_pile = draw.into();
                combat.rng = CombatRng::new(RngPool::new([17, 91, 123][variant]));
            } else {
                combat.zones.draw_pile = draw.into();
            }
            if kind == Scenario::PactSentinel {
                combat.turn.energy = 1;
            }
            if kind == Scenario::GamblersKeepFinisher {
                combat.turn.energy = 2;
                combat.zones.draw_pile = (101..107)
                    .map(|id| {
                        CombatCard::for_scenario(if id == 101 { SeeingRed } else { Wound }, id)
                    })
                    .collect();
            }
            if kind == Scenario::PactKeepSentinel {
                combat.turn.energy = 2;
                combat.entities.player.current_hp = 5;
                combat.zones.draw_pile = (101..103)
                    .map(|id| CombatCard::for_scenario(Wound, id))
                    .collect();
            }
            if kind == Scenario::BashBeforeAttacks {
                combat.turn.energy = 2;
            }
            if matches!(kind, Scenario::ShortFinishThorns | Scenario::StopThorns) {
                store::set_powers_for(
                    &mut combat,
                    1,
                    vec![Power {
                        power_type: PowerId::Thorns,
                        instance_id: None,
                        amount: 1,
                        extra_data: 0,
                        payload: PowerPayload::None,
                        just_applied: false,
                    }],
                );
            }
            let mut position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
            let mut preparation = vec![];
            let mut preparation_root = None;
            if matches!(
                kind,
                Scenario::GamblersSelection | Scenario::GamblersKeepFinisher
            ) {
                position.combat.entities.potions =
                    vec![Some(Potion::new(PotionId::GamblersBrew, 1))];
                preparation_root = Some(position.clone());
                let prepared = materialize_exact_action_line(
                    &EngineCombatStepper,
                    &position,
                    &[ClientInput::UsePotion {
                        potion_index: 0,
                        target: None,
                    }],
                    TRANSITION,
                )
                .unwrap();
                preparation = prepared.actions;
                position = prepared.final_position;
            }
            if kind == Scenario::PactKeepSentinel {
                preparation_root = Some(position.clone());
                let card_index = position
                    .combat
                    .zones
                    .hand
                    .iter()
                    .position(|card| card.id == BurningPact)
                    .unwrap();
                let prepared = materialize_exact_action_line(
                    &EngineCombatStepper,
                    &position,
                    &[ClientInput::PlayCard {
                        card_index,
                        target: None,
                    }],
                    TRANSITION,
                )
                .unwrap();
                preparation = prepared.actions;
                position = prepared.final_position;
            }
            Case {
                kind,
                name,
                position,
                goal,
                witness,
                preparation,
                preparation_root,
            }
        })
        .collect()
}

fn validate_case(case: &Case) -> Vec<sts_combat_planner::TurnOptionAction> {
    let mut position = case.position.clone();
    let mut actions = vec![];
    for step in &case.witness {
        let input = match *step {
            Step::Play(id, target) => ClientInput::PlayCard {
                card_index: position
                    .combat
                    .zones
                    .hand
                    .iter()
                    .position(|c| c.id == id)
                    .expect("fixture witness card"),
                target: target.then_some(1),
            },
            Step::Select(id) => ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                vec![
                    position
                        .combat
                        .zones
                        .hand
                        .iter()
                        .find(|c| c.id == id)
                        .expect("fixture selection card")
                        .lineage,
                ],
            )),
            Step::SelectAll => ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                position.combat.zones.hand.iter().map(|c| c.lineage),
            )),
            Step::End => ClientInput::EndTurn,
        };
        let materialized =
            materialize_exact_action_line(&EngineCombatStepper, &position, &[input], TRANSITION)
                .unwrap_or_else(|e| panic!("{} fixture: {e:?}", case.name));
        actions.extend(materialized.actions);
        position = materialized.final_position;
    }
    assert!(
        case.goal.accepts(&case.position, &position),
        "{} fixture goal is not reached",
        case.name
    );
    actions
}

fn identities(generator: &TurnOptionGeneratorSession) -> Vec<String> {
    generator.completed_options().iter().map(|o|serde_json::to_string(&json!({"actions":o.actions(),"boundary":format!("{:?}",o.boundary()),"successor":o.exact_successor_hash()})).unwrap()).collect()
}

fn probe(case: &Case, policy: ProbePolicy, output: &Path) -> (Value, Option<BTreeSet<String>>) {
    let root = CombatDecisionRoot::new(case.position.clone()).unwrap();
    let config = TurnOptionGeneratorConfig {
        allow_potion_expenditure: false,
        allow_potion_discard: false,
        allowed_potion_slots: Some(0),
        ..Default::default()
    };
    let mut generator =
        TurnOptionGeneratorSession::with_policy(root.clone(), config, Arc::new(policy));
    let mut inspected = 0;
    let mut matching = 0;
    let mut first = None;
    let mut curve = vec![];
    let mut search_ns = 0;
    let mut final_report = None;
    for work in 1..=WORK {
        let started = Instant::now();
        let report = generator.advance(
            &EngineCombatStepper,
            CombatPlanningQuantum::deterministic(1, if work == 1 { STEPS } else { 0 }),
        );
        search_ns += started.elapsed().as_nanos();
        assert!(
            report.gaps.is_empty(),
            "{} mechanics gap: {:?}",
            case.name,
            report.gaps
        );
        for option in &generator.completed_options()[inspected..] {
            if case.goal.accepts(&case.position, option.exact_successor()) {
                matching += 1;
                if first.is_none() {
                    first = Some(
                        json!({"work":report.after.generation_work,"engine_steps":report.after.engine_steps,"actions":option.actions(),"successor":option.exact_successor_hash()}),
                    );
                }
            }
        }
        inspected = generator.completed_options().len();
        if [16, 64, 256, 1024, 4096, WORK].contains(&work) || generator.is_finished() {
            curve.push(json!({"work":report.after.generation_work,"engine_steps":report.after.engine_steps,"outcomes":inspected,"matching":matching}));
        }
        final_report = Some(report);
        if generator.is_finished() {
            break;
        }
    }
    let report = final_report.unwrap();
    for option in generator.completed_options() {
        let replay = replay_turn_option(
            &root,
            option,
            &EngineCombatStepper,
            ReplayLimits::deterministic(STEPS),
        )
        .unwrap();
        assert_eq!(replay.position, *option.exact_successor());
    }
    let mut batch = TurnOptionGeneratorSession::with_policy(root.clone(), config, Arc::new(policy));
    let batched = batch.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(WORK, STEPS),
    );
    assert_eq!(report.after, batched.after);
    assert_eq!(identities(&generator), identities(&batch));
    let result = json!({"policy":policy.name(),"root_hash":root.exact_state_hash(),"goal":case.goal.description(),"work_limit":WORK,"engine_step_limit":STEPS,"work":report.after.generation_work,"engine_steps":report.after.engine_steps,"coverage":if generator.is_finished() {"complete"} else {"open"},"matching":matching,"outcomes":inspected,"first_match":first,"curve":curve,"search_ms":search_ns as f64/1e6,"all_replayed":true,"split_equal":true});
    fs::write(
        output.join(format!("{}.json", policy.name())),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    let complete = generator.is_finished().then(|| {
        generator
            .completed_options()
            .iter()
            .map(|o| o.exact_successor_hash().to_owned())
            .collect()
    });
    (result, complete)
}

fn probe_intent_edit(case: &Case, output: &Path, frozen_wrong_policy: bool) -> Value {
    let root = CombatDecisionRoot::new(case.position.clone()).unwrap();
    let policy = ProbePolicy {
        gamble: if frozen_wrong_policy {
            GamblePreference::All
        } else {
            GamblePreference::None
        },
        ..Default::default()
    };
    let select = |all: bool| {
        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Hand,
            case.position
                .combat
                .zones
                .hand
                .iter()
                .filter(|card| all || card.id == CardId::Wound)
                .map(|card| card.lineage),
        ))
    };
    let wrong = select(true);
    let corrected = select(false);
    let run = |split: bool| {
        let mut generator = TurnOptionGeneratorSession::with_policy(
            root.clone(),
            TurnOptionGeneratorConfig {
                allow_potion_expenditure: false,
                allow_potion_discard: false,
                allowed_potion_slots: Some(0),
                ..Default::default()
            },
            Arc::new(policy),
        );
        let source = generator.root_selection_intent().source;
        let mut first = None;
        let mut events = vec![];
        for (at, members, grant) in [
            (0, vec![wrong.clone()], 64),
            (64, vec![], 64),
            (128, vec![corrected.clone()], WORK - 128),
        ] {
            let counters = generator.counters();
            let budget = generator.granted_budget();
            let retained = identities(&generator);
            let probabilities: Vec<_> = generator
                .completed_options()
                .iter()
                .map(|o| o.negative_log_policy())
                .collect();
            generator
                .replace_root_selection_intent(&source, &members, &EngineCombatStepper)
                .unwrap();
            assert_eq!(generator.root_selection_intent().members, members);
            assert_eq!(generator.counters(), counters);
            assert_eq!(generator.granted_budget(), budget);
            assert_eq!(identities(&generator), retained);
            assert_eq!(
                generator
                    .completed_options()
                    .iter()
                    .map(|o| o.negative_log_policy())
                    .collect::<Vec<_>>(),
                probabilities
            );
            events.push(json!({"at_work":at,"members":members,"retained_options":retained.len(),"used":{"work":counters.generation_work,"engine_steps":counters.engine_steps},"granted":{"work":budget.generation_work,"engine_steps":budget.engine_steps}}));
            for i in 0..if split { grant } else { 1 } {
                let report = generator.advance(
                    &EngineCombatStepper,
                    CombatPlanningQuantum::deterministic(
                        if split { 1 } else { grant },
                        if at == 0 && i == 0 { STEPS } else { 0 },
                    ),
                );
                assert!(report.gaps.is_empty());
                if first.is_none() {
                    if let Some(option) = generator
                        .completed_options()
                        .iter()
                        .find(|o| case.goal.accepts(&case.position, o.exact_successor()))
                    {
                        first = Some(
                            json!({"work":report.after.generation_work,"engine_steps":report.after.engine_steps,"actions":option.actions()}),
                        );
                    }
                }
            }
            assert_eq!(&identities(&generator)[..retained.len()], retained);
        }
        (generator, first, events)
    };
    let (generator, first, events) = run(true);
    let (batch, _, _) = run(false);
    assert_eq!(generator.counters(), batch.counters());
    assert_eq!(generator.granted_budget(), batch.granted_budget());
    assert_eq!(identities(&generator), identities(&batch));
    assert!(
        first.is_some(),
        "{} corrected intention still has no witness",
        case.name
    );
    for option in generator.completed_options() {
        let replay = replay_turn_option(
            &root,
            option,
            &EngineCombatStepper,
            ReplayLimits::deterministic(STEPS),
        )
        .unwrap();
        assert_eq!(replay.position, *option.exact_successor());
    }
    let result = json!({
        "case":case.name,"frozen_wrong_policy":frozen_wrong_policy,"events":events,
        "first_match":first,"work":generator.counters().generation_work,"engine_steps":generator.counters().engine_steps,
        "outcomes":generator.completed_options().len(),"all_replayed":true,"split_equal":true,
        "edits_preserve_results_and_budget":true,
    });
    fs::write(
        output.join(if frozen_wrong_policy {
            "intent_edit_frozen_wrong.json"
        } else {
            "intent_edit_uniform.json"
        }),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or("usage: turn_guidance_probe <new-output-directory>")?;
    let directory = Path::new(&directory);
    fs::create_dir(directory)?;
    let producer = json!({
        "package_version": env!("CARGO_PKG_VERSION"),
        "executable_sha256": format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?)),
        "probe_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("turn_guidance_probe.rs"))),
    });
    fs::write(
        directory.join("producer.json"),
        serde_json::to_vec_pretty(&producer)?,
    )?;
    let mut results = vec![];
    let mut selection_trials = vec![];
    let mut intent_trials = vec![];
    for variant in 0..3 {
        for case in cases(variant) {
            let witness = validate_case(&case);
            let output = directory.join(format!("{}-{variant}", case.name));
            fs::create_dir(&output)?;
            fs::write(
                output.join("input.json"),
                serde_json::to_vec(
                    &json!({"provenance":"constructed","variant":variant,"shuffle_seed":if case.kind==Scenario::ShuffleDraw {Some([17,91,123][variant])} else {None},"position":case.position,"preparation_root":case.preparation_root,"preparation":case.preparation,"validated_witness":witness}),
                )?,
            )?;
            let (uniform, uniform_complete) = probe(
                &case,
                ProbePolicy {
                    depth: false,
                    exhaust_sentinel: false,
                    gamble: GamblePreference::None,
                },
                &output,
            );
            let (depth, depth_complete) = probe(
                &case,
                ProbePolicy {
                    depth: true,
                    exhaust_sentinel: false,
                    gamble: GamblePreference::None,
                },
                &output,
            );
            let compared_complete = match (&uniform_complete, &depth_complete) {
                (Some(a), Some(b)) => {
                    assert_eq!(a, b, "{} completed state sets differ", case.name);
                    true
                }
                _ => false,
            };
            println!(
                "{} v{} | uniform {} | depth {}",
                case.name,
                variant,
                first_match_label(&uniform),
                first_match_label(&depth)
            );
            if matches!(
                case.kind,
                Scenario::PactSentinel | Scenario::PactKeepSentinel
            ) {
                for guide in [false, true] {
                    let policy = ProbePolicy {
                        depth: guide,
                        exhaust_sentinel: true,
                        gamble: GamblePreference::None,
                    };
                    let (trial, complete) = probe(&case, policy, &output);
                    let compared = match (&uniform_complete, &complete) {
                        (Some(a), Some(b)) => {
                            assert_eq!(
                                a, b,
                                "{} selection preference changed complete states",
                                case.name
                            );
                            true
                        }
                        _ => false,
                    };
                    println!(
                        "{} v{} | {} {}",
                        case.name,
                        variant,
                        policy.name(),
                        first_match_label(&trial)
                    );
                    selection_trials.push(json!({"case":case.name,"variant":variant,"trial":trial,"complete_sets_equal":compared.then_some(true)}));
                }
            }
            if matches!(
                case.kind,
                Scenario::GamblersSelection | Scenario::GamblersKeepFinisher
            ) {
                for frozen_wrong_policy in [false, true] {
                    let trial = probe_intent_edit(&case, &output, frozen_wrong_policy);
                    println!(
                        "{} v{} | edit intention (frozen wrong: {}) {}",
                        case.name,
                        variant,
                        frozen_wrong_policy,
                        first_match_label(&trial)
                    );
                    intent_trials.push(json!({"case":case.name,"variant":variant,"trial":trial}));
                }
                for gamble in [GamblePreference::Wounds, GamblePreference::All] {
                    let policy = ProbePolicy {
                        gamble,
                        ..Default::default()
                    };
                    let (trial, complete) = probe(&case, policy, &output);
                    let compared = match (&uniform_complete, &complete) {
                        (Some(a), Some(b)) => {
                            assert_eq!(
                                a, b,
                                "{} sparse preference changed complete states",
                                case.name
                            );
                            true
                        }
                        _ => false,
                    };
                    println!(
                        "{} v{} | {} {}",
                        case.name,
                        variant,
                        policy.name(),
                        first_match_label(&trial)
                    );
                    selection_trials.push(json!({"case":case.name,"variant":variant,"trial":trial,"complete_sets_equal":compared.then_some(true)}));
                }
            }
            results.push(json!({"case":case.name,"variant":variant,"uniform":uniform,"depth":depth,"complete_sets_equal":compared_complete.then_some(true)}));
        }
    }
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(
            &json!({"kind":"constructed-turn-guidance-comparison","producer":producer,"results":results,"selection_trials":selection_trials,"intent_trials":intent_trials}),
        )?,
    )?;
    Ok(())
}

fn first_match_label(result: &Value) -> String {
    result["first_match"]["work"].as_u64().map_or_else(
        || format!("Unknown within {WORK} work"),
        |work| work.to_string(),
    )
}
