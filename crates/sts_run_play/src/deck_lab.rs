//! Counterfactual encounter checks using the same CombatSolver as run-play.
//! Current resources are copied; diagnostic RNG never reads the real future.
use std::fmt::Write;
use std::time::{Duration, Instant};
use sts_presentation::Locale;

use crate::combat_trace::{
    combat_turn_start_v1, RunPlayCombatEndTurnReviewV1, RunPlayCombatMonsterSummaryV1,
    RunPlayCombatTurnStartV1,
};
use crate::driver::{compact_continuation_input_v1, RunPlayCombatTurnContinuationSemanticActionV1};
use serde::Serialize;
use sts_guided_search::solver::{
    CombatSolver, SolverConfig, SolverLossSample, SolverPlan, SolverStats, SolverStop,
};
use sts_oracle_learning::agent::information::state::{
    public_combat_state_v1, CombatLearningEnemyIdentityV1, PublicCombatStateV1,
};
use sts_oracle_learning::content::monsters::factory::EncounterId;
use sts_oracle_learning::eval::run_control::public_encounter_probe_plan_v1;
use sts_oracle_learning::runtime::rng::RngPool;
use sts_oracle_learning::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
};
use sts_oracle_learning::sim::combat_start::{build_natural_combat_start, encounter_id_from_input};
use sts_oracle_learning::state::core::ClientInput;
use sts_oracle_learning::state::map::node::RoomType;
use sts_oracle_learning::state::run::RunState;
use sts_oracle_learning::state::DomainCardSnapshot;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Budget {
    pub scenarios: usize,
    pub work: usize,
    pub ms: u64,
    pub full_hp: bool,
    pub max_potion_uses: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Encounter {
    pub encounter: EncounterId,
    pub room_type: RoomType,
}

pub fn available(run: &RunState) -> Vec<Encounter> {
    available_for_public_run(run.act_num, run.boss_key)
}

pub(crate) fn available_for_public_run(act: u8, boss: Option<EncounterId>) -> Vec<Encounter> {
    let mut result: Vec<_> = public_encounter_probe_plan_v1(act, boss)
        .into_iter()
        .map(|probe| Encounter {
            encounter: probe.encounter,
            room_type: probe.room_type,
        })
        .collect();
    if !result
        .iter()
        .any(|item| item.encounter == EncounterId::TheHeart)
    {
        result.push(Encounter {
            encounter: EncounterId::TheHeart,
            room_type: RoomType::MonsterRoomBoss,
        });
    }
    result
}

pub fn select(run: &RunState, names: &[String]) -> Result<Vec<Encounter>, String> {
    if names.is_empty() {
        return Err("deck-lab requires --encounter NAME (or --list)".into());
    }
    let available = available(run);
    let mut selected = Vec::new();
    for name in names {
        let id = encounter_id_from_input(name)?;
        let spec = available
            .iter()
            .find(|item| item.encounter == id)
            .ok_or_else(|| {
                format!(
                    "{name} is outside the Act {} diagnostic battery; use --list",
                    run.act_num
                )
            })?;
        if !selected.contains(spec) {
            selected.push(*spec);
        }
    }
    Ok(selected)
}

#[derive(Serialize)]
pub struct Case {
    pub encounter: Encounter,
    pub diagnostic_seed: u64,
    pub root: CombatPosition,
    pub stop: SolverStop,
    pub stats: SolverStats,
    pub plans: Vec<SolverPlan>,
    /// One observed losing branch, never evidence that every branch loses.
    pub loss_sample: Option<SolverLossSample>,
    pub diagnostic_lines: Vec<DiagnosticLine>,
    pub elapsed_ms: u128,
}

#[derive(Serialize)]
pub struct DiagnosticLine {
    /// A case-local winning plan index, or the solver's one observed loss sample.
    pub candidate_index: Option<usize>,
    pub turns: Vec<DiagnosticTurn>,
}

#[derive(Serialize)]
pub struct DiagnosticTurn {
    pub turn: u32,
    pub start: RunPlayCombatTurnStartV1,
    pub actions: Vec<RunPlayCombatTurnContinuationSemanticActionV1>,
    /// Target names resolved from the actual state immediately before each input.
    pub action_labels: Vec<String>,
    /// Settled input observations, in the same order as actions/action_labels.
    pub action_steps: Vec<DiagnosticActionStep>,
    pub end_turn: Option<RunPlayCombatEndTurnReviewV1>,
    pub end_player_hp: i32,
    pub end_monsters: Vec<RunPlayCombatMonsterSummaryV1>,
}

#[derive(Serialize)]
pub struct DiagnosticActionStep {
    /// Index in the complete line, including pending card selections.
    pub input_index: usize,
    pub before: DiagnosticBoundary,
    pub after: DiagnosticBoundary,
    /// Actual ordered draw events during this settled input, including triggers.
    /// Hand-size changes are not a substitute for this observation.
    pub drawn_cards: Vec<DomainCardSnapshot>,
}

#[derive(Serialize)]
pub struct DiagnosticBoundary {
    pub turn: u32,
    pub state: RunPlayCombatTurnStartV1,
    pub draw_pile_count: usize,
    pub discard_pile_count: usize,
    pub exhaust_pile_count: usize,
}

fn diagnostic_boundary(
    position: &CombatPosition,
    public: &PublicCombatStateV1,
) -> DiagnosticBoundary {
    DiagnosticBoundary {
        turn: position.combat.turn.turn_count,
        state: combat_turn_start_v1(public),
        draw_pile_count: position.combat.zones.draw_pile.len(),
        discard_pile_count: position.combat.zones.discard_pile.len(),
        exhaust_pile_count: position.combat.zones.exhaust_pile.len(),
    }
}

fn diagnostic_line(
    root: &CombatPosition,
    inputs: &[ClientInput],
    expected: &CombatPosition,
    max_engine_steps: usize,
    candidate_index: Option<usize>,
) -> Result<DiagnosticLine, String> {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut turns = Vec::<DiagnosticTurn>::new();
    for (index, input) in inputs.iter().enumerate() {
        if !stepper.is_legal_action(&position, input) {
            return Err(format!("deck-lab diagnostic action {index} is not legal"));
        }
        let turn = position.combat.turn.turn_count;
        let before = public_combat_state_v1(&position.combat);
        if turns.last().is_none_or(|row| row.turn != turn) {
            turns.push(DiagnosticTurn {
                turn,
                start: combat_turn_start_v1(&before),
                actions: Vec::new(),
                action_labels: Vec::new(),
                action_steps: Vec::new(),
                end_turn: None,
                end_player_hp: before.player.hp,
                end_monsters: Vec::new(),
            });
        }
        let action = compact_continuation_input_v1(&position, input, index)?;
        let observed = stepper
            .apply_to_stable_observed(
                &position,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps,
                    deadline: None,
                },
            )
            .ok_or_else(|| "deck-lab diagnostic replay needs draw observations".to_string())?;
        let result = observed.step;
        if result.truncated || result.timed_out {
            return Err(format!("deck-lab diagnostic action {index} did not settle"));
        }
        let after = public_combat_state_v1(&result.position.combat);
        let row = turns.last_mut().expect("diagnostic turn inserted");
        row.action_labels
            .push(diagnostic_action_label(&action, &before));
        row.actions.push(action);
        row.action_steps.push(DiagnosticActionStep {
            input_index: index,
            before: diagnostic_boundary(&position, &before),
            after: diagnostic_boundary(&result.position, &after),
            drawn_cards: observed.drawn_cards,
        });
        row.end_player_hp = after.player.hp;
        row.end_monsters = combat_turn_start_v1(&after).monsters;
        if *input == ClientInput::EndTurn {
            row.end_turn = Some(RunPlayCombatEndTurnReviewV1 {
                player_block: before.player.block,
                visible_incoming_damage: crate::combat_explain::visible_incoming_damage_v1(&before),
                observed_player_hp_delta: after.player.hp - before.player.hp,
            });
        }
        position = result.position;
    }
    if &position != expected {
        return Err("deck-lab diagnostic replay differs from the observed endpoint".into());
    }
    Ok(DiagnosticLine {
        candidate_index,
        turns,
    })
}

#[derive(Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub resource_contract: &'static str,
    pub information_contract: &'static str,
    pub budget: Budget,
    pub solver_config: SolverConfig,
    pub opening_surveys: Vec<OpeningSurvey>,
    pub cases: Vec<Case>,
}

/// Root generation only; no searches or combat outcomes in this survey.
#[derive(Serialize)]
pub struct OpeningSurvey {
    pub encounter: Encounter,
    pub generated_starts: usize,
    pub powers: Vec<PowerOpeningPresence>,
    pub examples: Vec<OpeningExample>,
}

#[derive(Serialize)]
pub struct OpeningExample {
    pub diagnostic_seed: u64,
    pub start: RunPlayCombatTurnStartV1,
}

#[derive(Serialize)]
pub struct PowerOpeningPresence {
    pub card: sts_oracle_learning::content::cards::CardId,
    pub present_seeds: Vec<u64>,
    pub absent_seeds: Vec<u64>,
}

fn survey_openings(
    run: &RunState,
    encounter: Encounter,
    full_hp: bool,
) -> Result<OpeningSurvey, String> {
    use sts_oracle_learning::content::cards::{get_card_definition, CardType};
    let mut powers = Vec::<PowerOpeningPresence>::new();
    for card in &run.master_deck {
        if get_card_definition(card.id).card_type == CardType::Power
            && !powers.iter().any(|power| power.card == card.id)
        {
            powers.push(PowerOpeningPresence {
                card: card.id,
                present_seeds: Vec::new(),
                absent_seeds: Vec::new(),
            });
        }
    }
    let generated_starts = if powers.is_empty() { 0 } else { 16 };
    let mut examples = Vec::new();
    for seed in 1..=generated_starts as u64 {
        let root = prepare(run, encounter, seed, full_hp)?;
        let mut first_presence_or_absence = false;
        for power in &mut powers {
            let seeds = if root
                .combat
                .zones
                .hand
                .iter()
                .any(|card| card.id == power.card)
            {
                &mut power.present_seeds
            } else {
                &mut power.absent_seeds
            };
            first_presence_or_absence |= seeds.is_empty();
            seeds.push(seed);
        }
        if first_presence_or_absence {
            examples.push(OpeningExample {
                diagnostic_seed: seed,
                start: combat_turn_start_v1(&public_combat_state_v1(&root.combat)),
            });
        }
    }
    Ok(OpeningSurvey {
        encounter,
        generated_starts,
        powers,
        examples,
    })
}

fn prepare(
    run: &RunState,
    encounter: Encounter,
    seed: u64,
    full_hp: bool,
) -> Result<CombatPosition, String> {
    let mut hypothetical = run.clone();
    hypothetical.rng_pool = RngPool::new(seed);
    // A diagnostic elite is ordinary, not implicitly the current burning node.
    hypothetical.map.current_x = -1;
    hypothetical.map.current_y = -1;
    if full_hp {
        hypothetical.current_hp = hypothetical.max_hp;
    }
    let (engine, combat) =
        build_natural_combat_start(&mut hypothetical, encounter.encounter, encounter.room_type)?;
    Ok(CombatPosition::new(engine, combat))
}

pub fn evaluate(
    run: &RunState,
    encounters: &[Encounter],
    budget: Budget,
) -> Result<Report, String> {
    if !(1..=16).contains(&budget.scenarios)
        || !(1..=1_000_000).contains(&budget.work)
        || !(1..=20_000).contains(&budget.ms)
        || budget.max_potion_uses > 16
        || encounters.is_empty()
    {
        return Err("deck-lab requires scenarios 1..16, work 1..1000000, ms 1..20000, potion expenditures 0..16 and a selected encounter".into());
    }
    let config = SolverConfig {
        max_potion_uses: Some(budget.max_potion_uses),
        ..SolverConfig::default()
    };
    let mut cases = Vec::new();
    let mut opening_surveys = Vec::new();
    for encounter in encounters {
        opening_surveys.push(survey_openings(run, *encounter, budget.full_hp)?);
        for seed in 1..=budget.scenarios as u64 {
            let root = prepare(run, *encounter, seed, budget.full_hp)?;
            let mut solver = CombatSolver::new(root.clone(), config.clone())?;
            let started = Instant::now();
            let stop = solver.advance(budget.work, Duration::from_millis(budget.ms))?;
            let search_elapsed_ms = started.elapsed().as_millis();
            let mut diagnostic_lines = solver
                .plans()
                .iter()
                .enumerate()
                .map(|(index, plan)| {
                    diagnostic_line(
                        &root,
                        &plan.inputs,
                        &plan.successor,
                        config.max_engine_steps_per_input,
                        Some(index),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            if diagnostic_lines.is_empty() {
                if let Some(loss) = solver.loss_sample() {
                    diagnostic_lines.push(diagnostic_line(
                        &root,
                        &loss.inputs,
                        &loss.successor,
                        config.max_engine_steps_per_input,
                        None,
                    )?);
                }
            }
            cases.push(Case {
                encounter: *encounter,
                diagnostic_seed: seed,
                root,
                stop,
                stats: solver.stats().clone(),
                plans: solver.plans().to_vec(),
                loss_sample: solver.loss_sample().cloned(),
                diagnostic_lines,
                elapsed_ms: search_elapsed_ms,
            });
        }
    }
    Ok(Report {
        schema: "sts-run-play-deck-lab-v7",
        resource_contract: "Current permanent deck, relic counters and potions, or the explicitly prepared branch. HP stays current unless full-hp is requested. Potion limits include passive use and discards. Elite cases are ordinary elites.",
        information_contract: "Fixed diagnostic seeds 1..scenarios, not the actual future or a win-rate estimate. Winning lines are independently replayed. A losing sample records the greatest turn reached, keeping the first sample at an equal turn; no retained win leaves other branches Unknown. Search ms excludes diagnostic replay. Detailed turns and action boundaries are saved in the artifact.",
        budget, solver_config: config, opening_surveys, cases,
    })
}

pub fn render(report: &Report, details: bool) -> String {
    let mut out = String::from("Deck lab | current whole_combat solver | fixed diagnostic RNG\n");
    writeln!(
        out,
        "{}\n{}",
        report.resource_contract, report.information_contract
    )
    .unwrap();
    for survey in &report.opening_surveys {
        if survey.powers.is_empty() {
            continue;
        }
        writeln!(out, "\n{} broader opening coverage | {} fixed starts generated, Hand presence only. Combat searches still use seeds 1..{} with the declared budget. Counts describe these diagnostic starts; use owned access and protection to assess a late Power.", survey.encounter.encounter.stable_id(), survey.generated_starts, report.budget.scenarios).unwrap();
        for power in &survey.powers {
            writeln!(
                out,
                "  {} (all upgrades): starts in Hand {}/{}",
                crate::render::format_card(power.card, 0),
                power.present_seeds.len(),
                survey.generated_starts
            )
            .unwrap();
            if details {
                writeln!(
                    out,
                    "    present seeds {:?} | absent seeds {:?}",
                    power.present_seeds, power.absent_seeds
                )
                .unwrap();
            }
        }
        if !details {
            continue;
        }
        writeln!(out, "  Opening examples before player actions: first presence/absence of each owned Power, shared seeds displayed once. Hand presence shows access at this root; later arrival depends on subsequent actions and draws.").unwrap();
        for example in &survey.examples {
            let start = &example.start;
            let hand = opening_hand_labels(start);
            let powers = if start.player_powers.is_empty() {
                "none".into()
            } else {
                start
                    .player_powers
                    .iter()
                    .map(|power| {
                        format!(
                            "{} {}",
                            sts_oracle_learning::content::identity::gameplay_display_label_v1(
                                power.power
                            ),
                            power.amount
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            writeln!(out, "    seed {} | HP {}/{} | Energy {} | Block {} | active effects: {} | visible incoming {} | Hand: {}",
                example.diagnostic_seed, start.player_hp, start.player_max_hp,
                start.energy, start.player_block, powers,
                start.visible_incoming_damage.map_or_else(|| "unobserved".into(), |n| n.to_string()), hand).unwrap();
        }
    }
    let mut covered_encounters = Vec::new();
    for case in &report.cases {
        if !covered_encounters.contains(&case.encounter) {
            let cases: Vec<_> = report
                .cases
                .iter()
                .filter(|other| other.encounter == case.encounter)
                .collect();
            write_setup_coverage(&mut out, &cases);
            covered_encounters.push(case.encounter);
        }
        let hp = case.root.combat.entities.player.current_hp;
        writeln!(
            out,
            "{} seed={} | start HP {} | work {}/{} | {:?} | {}ms | {}",
            case.encounter.encounter.stable_id(),
            case.diagnostic_seed,
            hp,
            case.stats.work(),
            report.budget.work,
            case.stop,
            case.elapsed_ms,
            if case.plans.is_empty() {
                "no win found"
            } else {
                "replayed win found"
            }
        )
        .unwrap();
        let indexes = if details {
            writeln!(
                out,
                "  {} retained winning lines (search results, not independent trials)",
                case.plans.len()
            )
            .unwrap();
            (0..case.plans.len()).collect()
        } else {
            diagnostic_summary_indexes(&case.plans)
        };
        if !details && !indexes.is_empty() {
            writeln!(out, "  Highest settled HP found per starting-potion set and expenditure count; minimum HP and turns belong to that same line.").unwrap();
        }
        for i in indexes {
            let plan = &case.plans[i];
            let spent = case
                .root
                .combat
                .entities
                .potions
                .iter()
                .enumerate()
                .filter_map(|(slot, potion)| {
                    (plan.outcome.starting_potion_expenditure_slots & (1_u64 << slot) != 0)
                        .then_some(potion.as_ref())
                        .flatten()
                        .map(|potion| {
                            format!(
                                "P{slot} {}",
                                sts_oracle_learning::content::identity::gameplay_display_label_v1(
                                    potion.id
                                )
                            )
                        })
                })
                .collect::<Vec<_>>();
            let label = if details {
                format!("c{i}: ")
            } else {
                String::new()
            };
            writeln!(out, "  {label}HP {} | min stable HP {} | turns {} | potion expenditures {} | starting potions spent: {}",
                plan.outcome.final_hp, plan.min_stable_hp, plan.successor.combat.turn.turn_count.saturating_add(1),
                plan.outcome.total_potion_expenditures, if spent.is_empty() { "none".into() } else { spent.join(", ") }).unwrap();
        }
        if case.plans.is_empty() {
            writeln!(
                out,
                "  Budget result: 0 replayed winning lines; stopping reason above."
            )
            .unwrap();
            if let Some(loss) = &case.loss_sample {
                let combat = &loss.successor.combat;
                let enemy_hp: i32 = combat
                    .entities
                    .monsters
                    .iter()
                    .map(|enemy| enemy.current_hp.max(0))
                    .sum();
                writeln!(
                    out,
                    "  One observed losing line: turn {} | remaining enemy HP {} | actions {}",
                    combat.turn.turn_count.saturating_add(1),
                    enemy_hp,
                    loss.inputs.len()
                )
                .unwrap();
            }
        }
        // Show the resource tradeoff without making the player unpack the full
        // solver JSON. At most two winning traces; ties keep the first plan.
        let cheapest = case
            .plans
            .iter()
            .enumerate()
            .min_by_key(|(index, plan)| (plan.outcome.total_potion_expenditures, *index))
            .map(|(index, _)| index);
        let displayed = |line: &&DiagnosticLine| {
            line.candidate_index.is_none()
                || line.candidate_index == Some(0)
                || line.candidate_index == cheapest
        };
        for line in case
            .diagnostic_lines
            .iter()
            .filter(|line| details && displayed(line))
        {
            let source = line.candidate_index.map_or_else(
                || "observed losing branch".into(),
                |index| format!("c{index}"),
            );
            writeln!(out, "  Trace {source}:").unwrap();
            if let Some(plan) = cheapest
                .filter(|index| line.candidate_index == Some(*index))
                .and_then(|index| case.plans.get(index))
            {
                writeln!(
                    out,
                    "    Fewest potion expenditures among retained winning lines: {}",
                    plan.outcome.total_potion_expenditures
                )
                .unwrap();
            }
            for row in &line.turns {
                let enemy_hp = diagnostic_enemy_rosters(row);
                let end = row.end_turn.map_or_else(String::new, |end| {
                    format!(
                        " | Block before End Turn {} / Incoming {}",
                        end.player_block,
                        end.visible_incoming_damage
                            .map_or_else(|| "unobserved".into(), |n| n.to_string())
                    )
                });
                let powers = if row.start.player_powers.is_empty() {
                    String::new()
                } else {
                    format!(
                        " | Powers: {}",
                        crate::decision::combat::power_labels(&row.start.player_powers, Locale::En)
                    )
                };
                let actions = row.action_labels.join(" · ");
                let hand = opening_hand_labels(&row.start);
                writeln!(
                    out,
                    "    T{} start | Energy {} | Block {} | Hand: {}",
                    row.turn.saturating_add(1),
                    row.start.energy,
                    row.start.player_block,
                    hand
                )
                .unwrap();
                writeln!(
                    out,
                    "    T{} | HP {}->{} | {}{}{} | {}",
                    row.turn.saturating_add(1),
                    row.start.player_hp,
                    row.end_player_hp,
                    enemy_hp,
                    end,
                    powers,
                    actions
                )
                .unwrap();
            }
        }
        if details && case.diagnostic_lines.iter().any(|line| !displayed(&line)) {
            writeln!(
                out,
                "  Other candidate traces are saved in the full artifact."
            )
            .unwrap();
        }
    }
    if !details {
        writeln!(
            out,
            "To include per-turn traces in a new diagnostic run, add details before after."
        )
        .unwrap();
    }
    out
}

// Diagnostic plans cannot be adopted into the actual game. The brief compares
// resources, rather than printing the number of paths the search happened to
// retain. Keep each starting-potion set and total expenditure count distinct.
fn diagnostic_summary_indexes(plans: &[SolverPlan]) -> Vec<usize> {
    let mut groups = std::collections::BTreeMap::new();
    for (index, plan) in plans.iter().enumerate() {
        let key = (
            plan.outcome.starting_potion_expenditure_slots,
            plan.outcome.total_potion_expenditures,
        );
        let rank = |p: &SolverPlan| {
            (
                p.outcome.final_hp,
                p.min_stable_hp,
                std::cmp::Reverse(p.successor.combat.turn.turn_count),
            )
        };
        let selected = groups.entry(key).or_insert(index);
        if rank(plan) > rank(&plans[*selected]) {
            *selected = index;
        }
    }
    groups.into_values().collect()
}

fn opening_hand_labels(start: &RunPlayCombatTurnStartV1) -> String {
    let hand = start
        .hand
        .iter()
        .map(|card| {
            crate::decision::combat::hand_card_label(
                card.card_id,
                card.upgrades,
                card.effective_cost,
                Locale::En,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    if hand.is_empty() {
        "empty".into()
    } else {
        hand
    }
}

// Aggregate facts already owned by the diagnostic roots and replays. Keep
// opening access separate from a recorded play and from its potion actions.
// These counts describe the chosen cases, not a natural draw probability.
fn write_setup_coverage(out: &mut String, cases: &[&Case]) {
    use sts_oracle_learning::content::cards::{get_card_definition, CardType};
    let Some(first) = cases.first() else {
        return;
    };
    let mut powers = Vec::new();
    for card in first.root.combat.meta.master_deck_snapshot.iter() {
        if get_card_definition(card.id).card_type == CardType::Power && !powers.contains(&card.id) {
            powers.push(card.id);
        }
    }
    if powers.is_empty() {
        return;
    }
    writeln!(out, "{} setup coverage | {} fixed cases | opening presence and first recorded play on the primary line (c0 or observed loss)", first.encounter.encounter.stable_id(), cases.len()).unwrap();
    for id in powers {
        let opening_count = cases
            .iter()
            .filter(|case| case.root.combat.zones.hand.iter().any(|card| card.id == id))
            .count();
        writeln!(
            out,
            "  {} (all upgrades): in opening Hand in {opening_count}/{} cases",
            crate::render::format_card(id, 0),
            cases.len()
        )
        .unwrap();
        for case in cases {
            let play = case
                .diagnostic_lines
                .iter()
                .find(|line| line.candidate_index.is_none() || line.candidate_index == Some(0))
                .and_then(|line| {
                    line.turns.iter().find_map(|row| {
                        row.actions.iter().enumerate().find_map(|(index, action)| {
                            matches!(action, RunPlayCombatTurnContinuationSemanticActionV1::PlayCard { card_id, .. }
                                if *card_id == id).then(|| {
                                    let potions = row.actions[..index].iter().zip(&row.action_labels[..index])
                                        .filter(|(action, _)| matches!(action, RunPlayCombatTurnContinuationSemanticActionV1::UsePotion { .. }))
                                        .map(|(_, label)| label.as_str()).collect::<Vec<_>>();
                                    format!("T{} {} | earlier potion actions on that turn: {}", row.turn.saturating_add(1), row.action_labels[index],
                                        if potions.is_empty() { "none".into() } else { potions.join("; ") })
                                })
                        })
                    })
                });
            writeln!(
                out,
                "    seed {} first recorded play: {}",
                case.diagnostic_seed,
                play.as_deref()
                    .unwrap_or("not recorded on the displayed line")
            )
            .unwrap();
        }
        if opening_count == cases.len() {
            writeln!(out, "    Every tested opening has this Power in Hand. Compare a later arrival using owned draw, Energy and protection.").unwrap();
        }
    }
}

fn diagnostic_enemy_rosters(row: &DiagnosticTurn) -> String {
    let roster = |monsters: &[RunPlayCombatMonsterSummaryV1]| {
        if monsters.is_empty() {
            return "none alive".to_owned();
        }
        monsters
            .iter()
            .map(|monster| {
                let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
                format!(
                    "M{} {} HP {}",
                    monster.slot,
                    sts_oracle_learning::content::identity::gameplay_display_label_v1(enemy_id),
                    monster.hp
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    // Slots address each snapshot, not a persistent enemy. Summons can reorder
    // the roster, so never compute cross-snapshot HP deltas by slot equality.
    format!(
        "Enemies start: {} | Enemies end: {}",
        roster(&row.start.monsters),
        roster(&row.end_monsters)
    )
}

fn diagnostic_action_label(
    action: &RunPlayCombatTurnContinuationSemanticActionV1,
    before: &PublicCombatStateV1,
) -> String {
    use RunPlayCombatTurnContinuationSemanticActionV1 as A;
    let target = |slot: &Option<usize>| {
        slot.map_or_else(String::new, |slot| {
            let name = before
                .monsters
                .iter()
                .find(|monster| usize::from(monster.slot) == slot)
                .map(|monster| {
                    let CombatLearningEnemyIdentityV1::Known { enemy_id } = monster.enemy;
                    sts_oracle_learning::content::identity::gameplay_display_label_v1(enemy_id)
                })
                .unwrap_or("unobserved target".into());
            format!(" @M{slot} {name}")
        })
    };
    match action {
        A::PlayCard {
            card_id,
            upgrades,
            target_monster_index,
        } => format!(
            "{}{}",
            crate::render::format_card(*card_id, *upgrades),
            target(target_monster_index)
        ),
        A::UsePotion {
            potion_id,
            potion_index,
            target_monster_index,
        } => format!(
            "Use P{potion_index} {}{}",
            sts_oracle_learning::content::identity::gameplay_display_label_v1(*potion_id),
            target(target_monster_index)
        ),
        A::DiscardPotion {
            potion_id,
            potion_index,
        } => format!(
            "Discard P{potion_index} {}",
            sts_oracle_learning::content::identity::gameplay_display_label_v1(*potion_id)
        ),
        A::EndTurn => "End turn".into(),
        A::Selection {
            input_kind,
            selected_cards,
        } => {
            if selected_cards.is_empty() {
                input_kind.to_string()
            } else {
                format!(
                    "Select {}",
                    selected_cards
                        .iter()
                        .map(|card| crate::render::format_card(card.card_id, card.upgrades))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
    use sts_oracle_learning::content::cards::CardId;
    use sts_oracle_learning::content::relics::{RelicId, RelicState};

    fn resources_value(run: &RunState) -> serde_json::Value {
        serde_json::json!({ "hp": run.current_hp, "max_hp": run.max_hp, "gold": run.gold,
            "deck": run.master_deck, "relics": run.relics, "potions": run.potions, "rng": run.rng_pool })
    }

    #[test]
    fn diagnostic_steps_observe_exhaust_draws_and_no_draw_at_exact_boundaries() {
        use sts_oracle_learning::runtime::combat::CombatCard;
        let run = fixture("reward").run_state;
        let mut root = prepare(
            &run,
            Encounter {
                encounter: EncounterId::BookOfStabbing,
                room_type: RoomType::MonsterRoomElite,
            },
            1,
            false,
        )
        .unwrap();
        let mut embrace = CombatCard::for_scenario(CardId::DarkEmbrace, 1);
        embrace.upgrades = 1;
        root.combat.zones.hand = vec![
            embrace,
            CombatCard::for_scenario(CardId::Pummel, 2),
            CombatCard::for_scenario(CardId::BattleTrance, 3),
            CombatCard::for_scenario(CardId::Pummel, 4),
            CombatCard::for_scenario(CardId::Brutality, 5),
        ];
        root.combat.zones.draw_pile = (20..35)
            .map(|slot| CombatCard::for_scenario(CardId::Defend, slot))
            .collect::<Vec<_>>()
            .into();
        root.combat.zones.discard_pile.clear();
        root.combat.zones.exhaust_pile.clear();
        root.combat.turn.energy = 10;
        let inputs = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            ClientInput::EndTurn,
        ];
        let limits = CombatStepLimits {
            max_engine_steps: 1000,
            deadline: None,
        };
        // The unobserved path supplies the exact endpoint independently.
        let stepper = EngineCombatStepper;
        let mut expected = root.clone();
        for (index, input) in inputs.iter().enumerate() {
            assert!(
                stepper.is_legal_action(&expected, input),
                "input {index}: {input:?}; engine {:?}; hand {:?}",
                expected.engine,
                expected
                    .combat
                    .zones
                    .hand
                    .iter()
                    .map(|card| card.id)
                    .collect::<Vec<_>>()
            );
            let result = stepper.apply_to_stable(&expected, input.clone(), limits);
            assert!(!result.truncated && !result.timed_out);
            expected = result.position;
        }
        let line = diagnostic_line(&root, &inputs, &expected, 1000, Some(0)).unwrap();
        let steps = &line.turns[0].action_steps;
        assert_eq!(steps.len(), inputs.len());
        assert_eq!(
            steps.iter().map(|s| s.input_index).collect::<Vec<_>>(),
            (0..inputs.len()).collect::<Vec<_>>()
        );
        let first_pummel = &steps[1];
        assert_eq!(
            first_pummel.before.state.energy - first_pummel.after.state.energy,
            1
        );
        assert_eq!(
            first_pummel.before.state.hand.len(),
            first_pummel.after.state.hand.len()
        );
        assert_eq!(
            first_pummel.after.exhaust_pile_count,
            first_pummel.before.exhaust_pile_count + 1
        );
        assert_eq!(first_pummel.drawn_cards.len(), 1);
        assert_eq!(first_pummel.drawn_cards[0].id, CardId::Defend);
        assert_eq!(
            steps[2].drawn_cards.len(),
            3,
            "Battle Trance draws before applying NoDraw"
        );
        let second_pummel = &steps[3];
        assert!(
            second_pummel.drawn_cards.is_empty(),
            "NoDraw suppresses Dark Embrace's triggered draw"
        );
        assert_eq!(
            second_pummel.after.exhaust_pile_count,
            second_pummel.before.exhaust_pile_count + 1
        );
        assert_eq!(
            second_pummel.after.state.hand.len() + 1,
            second_pummel.before.state.hand.len()
        );
        let end = steps.last().unwrap();
        assert_eq!(end.after.turn, end.before.turn + 1);
        assert_eq!(
            end.drawn_cards.len(),
            6,
            "next-turn normal and Brutality draws occur after NoDraw expires"
        );
        assert_eq!(end.after.state.hand.len(), 6);
        assert_eq!(
            end.after.state.player_hp,
            expected.combat.entities.player.current_hp
        );
        assert_eq!(line.turns[0].actions.len(), steps.len());
        assert_eq!(line.turns[0].action_labels.len(), steps.len());
    }

    #[test]
    fn preparation_resources_match_adoption_without_changing_the_actual_root() {
        use sts_oracle_learning::state::core::EngineState;
        for (kind, line) in [
            ("smith", "smith:PommelStrike#1"),
            ("rest", "rest"),
            ("buy", "buy:MembershipCard#0/buy:PommelStrike#0"),
            ("take", "take:ShrugItOff#0"),
        ] {
            let dir = TestDirectory::new();
            let mut session = fixture(if kind == "buy" { "shop" } else { "reward" });
            session
                .run_state
                .relics
                .push(RelicState::new(RelicId::CeramicFish));
            if matches!(kind, "smith" | "rest") {
                session.engine_state = EngineState::Campfire;
                session.run_state.current_hp = 20;
                session.run_state.add_card_to_deck(CardId::PommelStrike);
                session.run_state.add_card_to_deck(CardId::PommelStrike);
            }
            let mut game = controller_with_session(&dir.0, kind, &session);
            let before_head = game.head().clone();
            let before = game
                .current_external_exact_checkpoint_v1()
                .unwrap()
                .into_session()
                .unwrap();
            let selectors = crate::selector::parse_action_line_v1(line, "deck-lab after").unwrap();
            let (prepared, actions) = game.prepare_resources_for_deck_lab_v1(&selectors).unwrap();
            let resources = prepared
                .current_run_decision_checkpoint_for_run_play_v1()
                .unwrap()
                .into_session()
                .unwrap()
                .run_state;
            assert_eq!(actions.len(), selectors.len());
            assert_eq!(game.head(), &before_head);
            assert_eq!(game.paths().load_head().unwrap(), before_head);
            let unchanged = game
                .current_external_exact_checkpoint_v1()
                .unwrap()
                .into_session()
                .unwrap();
            assert_eq!(
                resources_value(&unchanged.run_state),
                resources_value(&before.run_state)
            );
            match kind {
                "smith" => assert_eq!(resources.master_deck.iter().filter(|card| card.id == CardId::PommelStrike && card.upgrades == 1).count(), 1),
                "rest" => assert_eq!(resources.current_hp, 42),
                "buy" => assert_eq!(resources.gold, 269, "Membership Card reprices the second purchase; Ceramic Fish pays on adding the card"),
                "take" => assert_eq!(resources.gold, before.run_state.gold + 9),
                _ => unreachable!(),
            }
            game.anchor_action_line_v1(&selectors).unwrap();
            let adopted = game
                .current_external_exact_checkpoint_v1()
                .unwrap()
                .into_session()
                .unwrap();
            assert_eq!(
                resources_value(&adopted.run_state),
                resources_value(&resources),
                "{kind}"
            );
        }
    }

    #[test]
    fn failed_preparation_and_travel_or_combat_choices_leave_actual_resources_unchanged() {
        for (kind, lines) in [
            ("shop", vec!["buy:MembershipCard#0/buy:missing"]),
            ("route", vec!["route:0.0", "ordinal:0"]),
            ("combat_finish", vec!["end"]),
        ] {
            let dir = TestDirectory::new();
            let mut game = controller_with_session(&dir.0, kind, &fixture(kind));
            let before = game.head().clone();
            for line in lines {
                let selectors =
                    crate::selector::parse_action_line_v1(line, "deck-lab after").unwrap();
                assert!(game.prepare_resources_for_deck_lab_v1(&selectors).is_err());
                assert_eq!(game.head(), &before);
                assert_eq!(game.paths().load_head().unwrap(), before);
            }
        }
    }

    #[test]
    fn preparation_requires_the_selection_triggered_by_a_purchase() {
        use sts_oracle_learning::state::core::EngineState;
        use sts_oracle_learning::state::shop::ShopRelic;
        let dir = TestDirectory::new();
        let mut session = fixture("shop");
        let EngineState::Shop(shop) = &mut session.engine_state else {
            panic!("shop fixture");
        };
        shop.relics[0] = ShopRelic {
            relic_id: RelicId::BottledFlame,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        };
        let mut game = controller_with_session(&dir.0, "bottled-preparation", &session);
        let before = game.head().clone();
        let incomplete =
            crate::selector::parse_action_line_v1("buy:BottledFlame#0", "deck-lab after").unwrap();
        assert!(game
            .prepare_resources_for_deck_lab_v1(&incomplete)
            .err()
            .unwrap()
            .contains("pending card selection"));
        assert_eq!(game.head(), &before);
        let complete = crate::selector::parse_action_line_v1(
            "buy:BottledFlame#0/select:Bash#0",
            "deck-lab after",
        )
        .unwrap();
        let (prepared, _) = game.prepare_resources_for_deck_lab_v1(&complete).unwrap();
        let resources = prepared
            .current_run_decision_checkpoint_for_run_play_v1()
            .unwrap()
            .into_session()
            .unwrap()
            .run_state;
        let bottled = resources
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::BottledFlame)
            .unwrap();
        let chosen = resources
            .master_deck
            .iter()
            .find(|card| card.id == CardId::Bash)
            .unwrap();
        assert_eq!(
            sts_oracle_learning::content::relics::persistent_state::persistent_relic_bound_card_lineage_v1(bottled).unwrap(),
            Some(chosen.lineage)
        );
        assert_eq!(resources.gold, 300);
        assert_eq!(game.head(), &before);
    }

    #[test]
    fn collector_summons_keep_start_and_end_rosters_separate_and_actions_name_actual_targets() {
        use sts_oracle_learning::content::monsters::EnemyId;
        let run = fixture("reward").run_state;
        let root = prepare(
            &run,
            Encounter {
                encounter: EncounterId::Collector,
                room_type: RoomType::MonsterRoomBoss,
            },
            1,
            true,
        )
        .unwrap();
        let result = EngineCombatStepper.apply_to_stable(
            &root,
            ClientInput::EndTurn,
            CombatStepLimits {
                max_engine_steps: 100000,
                deadline: None,
            },
        );
        assert!(!result.truncated && !result.timed_out);
        let line = diagnostic_line(
            &root,
            &[ClientInput::EndTurn],
            &result.position,
            100000,
            None,
        )
        .unwrap();
        let row = &line.turns[0];
        assert_eq!(row.start.monsters.len(), 1);
        assert_eq!(row.end_monsters.len(), 3);
        let view = diagnostic_enemy_rosters(row);
        assert!(
            view.contains("Enemies start: M0 The Collector HP 300"),
            "{view}"
        );
        assert!(view.contains("Enemies end: M0 Torch Head HP"), "{view}");
        assert!(view.contains("M2 The Collector HP 300"), "{view}");
        assert!(!view.contains("300->"), "{view}");
        let before = public_combat_state_v1(&result.position.combat);
        assert!(matches!(
            before.monsters[2].enemy,
            CombatLearningEnemyIdentityV1::Known {
                enemy_id: EnemyId::TheCollector
            }
        ));
        let action = RunPlayCombatTurnContinuationSemanticActionV1::PlayCard {
            card_id: CardId::Strike,
            upgrades: 0,
            target_monster_index: Some(2),
        };
        assert_eq!(
            diagnostic_action_label(&action, &before),
            "Strike @M2 The Collector"
        );
    }

    #[test]
    fn heart_case_preserves_real_resources_and_replaces_future_rng() {
        let mut run = fixture("reward").run_state;
        run.current_hp = 23;
        run.relics.push(RelicState::new(RelicId::Sling));
        let spec = select(&run, &["CorruptHeart".into()]).unwrap()[0];
        let before = (
            run.current_hp,
            run.max_hp,
            run.gold,
            run.master_deck.clone(),
            run.relics.clone(),
            run.potions.clone(),
            serde_json::to_value(&run.rng_pool).unwrap(),
        );
        let current = prepare(&run, spec, 1, false).unwrap();
        let full = prepare(&run, spec, 1, true).unwrap();
        assert_eq!(current.combat.entities.player.current_hp, 23);
        assert_eq!(full.combat.entities.player.current_hp, run.max_hp);
        assert!(current.combat.meta.is_boss_fight);
        assert!(!current.combat.meta.is_elite_fight);
        assert_eq!(
            sts_oracle_learning::content::powers::store::power_amount(
                &current.combat,
                0,
                sts_oracle_learning::content::powers::PowerId::Strength
            ),
            0,
            "Sling does not apply to the Heart"
        );
        assert_eq!(
            current.combat.meta.master_deck_snapshot.as_ref(),
            run.master_deck.as_slice()
        );
        let mut other = run.clone();
        other.rng_pool = RngPool::new(999999);
        assert_eq!(prepare(&other, spec, 1, false).unwrap(), current);
        assert_eq!(
            (
                run.current_hp,
                run.max_hp,
                run.gold,
                run.master_deck.clone(),
                run.relics.clone(),
                run.potions.clone(),
                serde_json::to_value(&run.rng_pool).unwrap()
            ),
            before
        );
    }

    #[test]
    fn solver_witnesses_and_misses_keep_distinct_meanings() {
        let mut run = fixture("reward").run_state;
        run.master_deck.clear();
        for _ in 0..5 {
            run.add_card_to_deck(CardId::Bludgeon);
        }
        let specs = [Encounter {
            encounter: EncounterId::Cultist,
            room_type: RoomType::MonsterRoom,
        }];
        let budget = Budget {
            scenarios: 1,
            work: 1000,
            ms: 2000,
            full_hp: false,
            max_potion_uses: 0,
        };
        let mut report = evaluate(&run, &specs, budget).unwrap();
        assert!(!report.cases[0].plans.is_empty());
        let line = &report.cases[0].diagnostic_lines[0];
        assert_eq!(line.candidate_index, Some(0));
        assert_eq!(
            line.turns.last().unwrap().end_player_hp,
            report.cases[0].plans[0]
                .successor
                .combat
                .entities
                .player
                .current_hp
        );
        assert!(render(&report, true).contains("Bludgeon @M0"));
        assert!(render(&report, true).contains("M0 Cultist HP"));
        let brief = render(&report, false);
        assert!(brief.contains("replayed win found"));
        assert!(!brief.contains("retained winning lines"));
        assert!(!brief.contains("Trace c0:"));
        assert!(!brief.contains("T1 start |"));
        assert!(brief.len() < render(&report, true).len());
        assert_eq!(report.cases[0].diagnostic_lines.len(), 1);
        assert!(render(&report, true).contains(&format!(
            "T1 start | Energy {} | Block {} | Hand: Bludgeon[3c]",
            line.turns[0].start.energy, line.turns[0].start.player_block
        )));
        let original_line_count = report.cases[0].diagnostic_lines.len();
        let other_line = diagnostic_line(
            &report.cases[0].root,
            &report.cases[0].plans[0].inputs,
            &report.cases[0].plans[0].successor,
            report.solver_config.max_engine_steps_per_input,
            Some(1),
        )
        .unwrap();
        report.cases[0].diagnostic_lines.push(other_line);
        let text = render(&report, true);
        assert!(text.contains("Trace c0:"));
        assert!(!text.contains("Trace c1:"));
        assert!(text.contains("Other candidate traces are saved in the full artifact."));
        assert_eq!(
            report.cases[0].diagnostic_lines.len(),
            original_line_count + 1
        );
        // Rendering fixture: expensive c0 and economical c1 share a replay here;
        // test the presentation policy, not a claim about potion execution.
        let case = &mut report.cases[0];
        let mut economical = case.plans[0].clone();
        economical.outcome.total_potion_expenditures = 0;
        case.plans[0].outcome.total_potion_expenditures = 1;
        case.plans.push(economical);
        let text = render(&report, true);
        assert!(text.contains("Trace c0:"));
        assert!(text.contains("Trace c1:"));
        assert!(text.contains("Fewest potion expenditures among retained winning lines: 0"));
        assert_eq!(text.matches("Trace c").count(), 2);
        assert!(report.cases[0].stats.work() <= 1000);
        // Synthetic render-only outcomes: repeated paths must not become an
        // apparent sample count, and different potion resources stay separate.
        let mut base = report.cases[0].plans[1].clone();
        base.outcome.final_hp = 40;
        base.min_stable_hp = 10;
        base.outcome.starting_potion_expenditure_slots = 0;
        let mut better = base.clone();
        better.outcome.final_hp = 45;
        let mut safer_tie = better.clone();
        safer_tie.min_stable_hp = 20;
        let mut other_potion = base.clone();
        other_potion.outcome.starting_potion_expenditure_slots = 1;
        let mut extra_expenditure = base.clone();
        extra_expenditure.outcome.total_potion_expenditures = 1;
        report.cases[0].plans = vec![base; 20];
        report.cases[0]
            .plans
            .extend([better, safer_tie, other_potion, extra_expenditure]);
        assert_eq!(
            diagnostic_summary_indexes(&report.cases[0].plans),
            vec![21, 23, 22]
        );
        let brief = render(&report, false);
        assert_eq!(brief.matches("min stable HP").count(), 3);
        assert!(!brief.contains("c21:"));
        let detailed = render(&report, true);
        assert!(detailed.contains("24 retained winning lines"));
        assert!(detailed.contains("c21: HP 45 | min stable HP 20"));
        let miss = evaluate(&run, &specs, Budget { work: 1, ..budget }).unwrap();
        assert!(miss.cases[0].plans.is_empty());
        assert!(render(&miss, false).contains("Budget result: 0 replayed winning lines"));
        assert!(evaluate(
            &run,
            &specs,
            Budget {
                scenarios: 0,
                ..budget
            }
        )
        .is_err());
    }

    #[test]
    fn opening_survey_exposes_missing_powers_without_changing_search_seeds_or_source() {
        let mut run = fixture("reward").run_state;
        run.master_deck.clear();
        run.add_card_to_deck(CardId::DemonForm);
        for _ in 0..19 {
            run.add_card_to_deck(CardId::Defend);
        }
        run.relics.push(RelicState::new(RelicId::Vajra));
        let before = resources_value(&run);
        let specs = [Encounter {
            encounter: EncounterId::Cultist,
            room_type: RoomType::MonsterRoom,
        }];
        let report = evaluate(
            &run,
            &specs,
            Budget {
                scenarios: 3,
                work: 1,
                ms: 1000,
                full_hp: false,
                max_potion_uses: 0,
            },
        )
        .unwrap();
        assert_eq!(
            report
                .cases
                .iter()
                .map(|case| case.diagnostic_seed)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let survey = &report.opening_surveys[0];
        assert_eq!(survey.generated_starts, 16);
        let power = &survey.powers[0];
        assert_eq!(power.card, CardId::DemonForm);
        assert!(!power.present_seeds.is_empty());
        assert!(!power.absent_seeds.is_empty());
        let mut seeds = power.present_seeds.clone();
        seeds.extend(&power.absent_seeds);
        seeds.sort_unstable();
        assert_eq!(seeds, (1..=16).collect::<Vec<_>>());
        assert_eq!(survey.examples.len(), 2);
        let example_seeds = survey
            .examples
            .iter()
            .map(|example| example.diagnostic_seed)
            .collect::<Vec<_>>();
        assert!(example_seeds.contains(&power.present_seeds[0]));
        assert!(example_seeds.contains(&power.absent_seeds[0]));
        for example in &survey.examples {
            let actual = prepare(&run, specs[0], example.diagnostic_seed, false).unwrap();
            assert_eq!(
                example.start,
                combat_turn_start_v1(&public_combat_state_v1(&actual.combat))
            );
            assert!(example.start.player_powers.iter().any(|power| power.power
                == sts_oracle_learning::content::powers::PowerId::Strength
                && power.amount == 1));
            assert!(!example.start.player_powers.iter().any(
                |power| power.power == sts_oracle_learning::content::powers::PowerId::DemonForm
            ));
        }
        for case in &report.cases {
            assert_eq!(
                power.present_seeds.contains(&case.diagnostic_seed),
                case.root
                    .combat
                    .zones
                    .hand
                    .iter()
                    .any(|card| card.id == CardId::DemonForm)
            );
        }
        assert_eq!(resources_value(&run), before);
        let text = render(&report, true);
        assert!(text.contains("16 fixed starts generated, Hand presence only"));
        assert!(text.contains("Combat searches still use seeds 1..3"));
        assert!(text.contains("absent seeds"));
        assert!(text.contains("Opening examples before player actions"));
        assert!(text.contains("active effects: Strength 1"));
    }

    #[test]
    fn setup_coverage_counts_cases_and_separates_access_from_play() {
        let mut run = fixture("reward").run_state;
        run.master_deck.clear();
        for card in [CardId::Inflame, CardId::Inflame, CardId::Defend] {
            run.add_card_to_deck(card);
        }
        let specs = [Encounter {
            encounter: EncounterId::Cultist,
            room_type: RoomType::MonsterRoom,
        }];
        let mut report = evaluate(
            &run,
            &specs,
            Budget {
                scenarios: 2,
                work: 1,
                ms: 1000,
                full_hp: false,
                max_potion_uses: 0,
            },
        )
        .unwrap();
        assert_eq!(
            report.opening_surveys[0].examples.len(),
            1,
            "same root shared by both Inflame copies"
        );
        let text = render(&report, true);
        assert!(
            text.contains("Inflame (all upgrades): in opening Hand in 2/2 cases"),
            "{text}"
        );
        assert!(
            text.contains("seed 1 first recorded play: not recorded on the displayed line"),
            "{text}"
        );
        assert!(
            text.contains("Every tested opening has this Power in Hand"),
            "{text}"
        );
        assert!(!text.contains("Defend (all upgrades)"), "{text}");
        // View fixture: an upgraded command on a later turn still belongs to
        // the owned Power kind; presence in the opening does not imply play.
        let case = &mut report.cases[0];
        let start = combat_turn_start_v1(&public_combat_state_v1(&case.root.combat));
        case.diagnostic_lines.push(DiagnosticLine {
            candidate_index: Some(0),
            turns: vec![DiagnosticTurn {
                turn: 2,
                start,
                actions: vec![
                    RunPlayCombatTurnContinuationSemanticActionV1::UsePotion {
                        potion_id:
                            sts_oracle_learning::content::potions::PotionId::DuplicationPotion,
                        potion_index: 0,
                        target_monster_index: None,
                    },
                    RunPlayCombatTurnContinuationSemanticActionV1::PlayCard {
                        card_id: CardId::Inflame,
                        upgrades: 1,
                        target_monster_index: None,
                    },
                ],
                action_labels: vec!["Use P0 Duplication Potion".into(), "Inflame+".into()],
                action_steps: Vec::new(),
                end_turn: None,
                end_player_hp: run.current_hp,
                end_monsters: Vec::new(),
            }],
        });
        let text = render(&report, true);
        assert!(text.contains("seed 1 first recorded play: T3 Inflame+ | earlier potion actions on that turn: Use P0 Duplication Potion"), "{text}");
        assert!(
            text.contains("seed 2 first recorded play: not recorded on the displayed line"),
            "{text}"
        );
        assert_eq!(text.matches("Cultist setup coverage |").count(), 1);
    }

    #[test]
    fn losing_diagnostics_replay_hp_defense_and_actual_power_deployments() {
        let mut run = fixture("reward").run_state;
        run.current_hp = 17;
        run.master_deck.clear();
        for card in [
            CardId::Inflame,
            CardId::DemonForm,
            CardId::Barricade,
            CardId::FeelNoPain,
        ] {
            run.add_card_to_deck(card);
        }
        let specs = [Encounter {
            encounter: EncounterId::Cultist,
            room_type: RoomType::MonsterRoom,
        }];
        let report = evaluate(
            &run,
            &specs,
            Budget {
                scenarios: 1,
                work: 8000,
                ms: 2000,
                full_hp: false,
                max_potion_uses: 0,
            },
        )
        .unwrap();
        let case = &report.cases[0];
        assert!(case.plans.is_empty());
        let loss = case
            .loss_sample
            .as_ref()
            .expect("a loss was actually observed");
        assert_eq!(case.diagnostic_lines.len(), 1);
        let line = &case.diagnostic_lines[0];
        assert_eq!(line.candidate_index, None);
        assert_eq!(line.turns.first().unwrap().start.player_hp, 17);
        assert_eq!(
            line.turns.last().unwrap().end_player_hp,
            loss.successor.combat.entities.player.current_hp
        );
        assert!(line.turns.iter().any(|row| row
            .end_turn
            .is_some_and(|end| end.observed_player_hp_delta < 0)));
        let text = render(&report, true);
        assert!(text.contains("Trace observed losing branch:"), "{text}");
        assert!(
            text.contains("Block before End Turn 0 / Incoming"),
            "{text}"
        );
        for row in &line.turns {
            assert!(
                text.contains(&format!(
                    "T{} start | Energy {} | Block {} | Hand:",
                    row.turn.saturating_add(1),
                    row.start.energy,
                    row.start.player_block
                )),
                "{text}"
            );
            for card in &row.start.hand {
                assert!(
                    text.contains(&crate::decision::combat::hand_card_label(
                        card.card_id,
                        card.upgrades,
                        card.effective_cost,
                        Locale::En
                    )),
                    "{text}"
                );
            }
        }
        assert!(text.contains("keeping the first sample at an equal turn"));
        assert_eq!(run.current_hp, 17, "diagnosis leaves the run alone");
        let mut wrong_endpoint = loss.successor.clone();
        wrong_endpoint.combat.entities.player.current_hp += 1;
        assert!(diagnostic_line(
            &case.root,
            &loss.inputs,
            &wrong_endpoint,
            report.solver_config.max_engine_steps_per_input,
            None
        )
        .is_err());
    }

    #[test]
    fn encounter_catalogue_follows_current_act_boss_and_includes_act_four_elite() {
        let act1 = available_for_public_run(1, Some(EncounterId::TheGuardian));
        assert!(act1
            .iter()
            .any(|spec| spec.encounter == EncounterId::GremlinNob));
        let act2 = available_for_public_run(2, Some(EncounterId::Collector));
        assert!(act2
            .iter()
            .any(|spec| spec.encounter == EncounterId::Collector));
        assert!(!act2
            .iter()
            .any(|spec| spec.encounter == EncounterId::GremlinNob));
        let act4 = available_for_public_run(4, Some(EncounterId::TheHeart));
        assert!(act4
            .iter()
            .any(|spec| spec.encounter == EncounterId::ShieldAndSpear
                && spec.room_type == RoomType::MonsterRoomElite));
        assert_eq!(
            act4.iter()
                .filter(|spec| spec.encounter == EncounterId::TheHeart)
                .count(),
            1
        );
    }
}
