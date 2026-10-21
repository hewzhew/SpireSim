//! Automatic early-run diagnostic: explicit setup rules plus oracle combat.
//! No learned strategic policy, seed-SL qualification or teacher-label authority.
mod policy;
#[cfg(test)]
mod tests;

use super::{NaturalAction, NaturalEpisode};
use crate::solver::{CombatSolver, SolverConfig, SolverPassProgress, SolverStats, SolverStop};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use sts_core::content::potions::PotionId;
use sts_core::state::core::ClientInput;
use sts_core::state::map::node::RoomType;

#[derive(Clone, Debug, Serialize)]
pub struct AuditConfig {
    pub max_actions: usize,
    pub work_per_combat: usize,
    pub work_per_run: usize,
    pub seconds_per_run: u64,
}

#[derive(Clone, Debug, Serialize)]
pub enum AuditInput {
    Strategic(ClientInput),
    Combat(ClientInput),
}

#[derive(Clone, Debug, Serialize)]
pub struct SetupCandidateScore {
    pub option: usize,
    pub score: i32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuditDecision {
    pub floor: i32,
    pub turn: Option<u32>,
    pub site: String,
    pub hp_before: i32,
    pub chosen: String,
    pub reason: String,
    /// Strategic alternatives remain visible for reviewing the setup policy.
    pub offered: Vec<String>,
    pub setup_scores: Vec<SetupCandidateScore>,
    pub input: AuditInput,
    pub successor_digest: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuditCombat {
    pub floor: i32,
    pub stats: SolverStats,
    pub passes: Vec<SolverPassProgress>,
    pub search_stop: Option<SolverStop>,
    pub error: Option<String>,
    pub alternatives: Vec<Value>,
    pub selected: Option<usize>,
    pub selection_reason: String,
    pub source: sts_core::sim::combat::CombatPosition,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuditFinalResources {
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub potions: Vec<Option<PotionId>>,
}

impl AuditFinalResources {
    fn from_episode(episode: &NaturalEpisode) -> Self {
        let resources = episode.resources();
        Self {
            hp: resources.hp,
            max_hp: resources.max_hp,
            gold: resources.gold,
            potions: resources
                .potions
                .iter()
                .map(|potion| potion.as_ref().map(|p| p.id))
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct AuditReport {
    pub seed: u64,
    pub config: AuditConfig,
    pub solver_config: SolverConfig,
    pub information: &'static str,
    pub actor: &'static str,
    pub decisions: Vec<AuditDecision>,
    pub combats: Vec<AuditCombat>,
    pub stop: String,
    pub error: Option<String>,
    pub failed_input: Option<AuditInput>,
    pub final_view: String,
    pub final_snapshot: Value,
    pub final_resources: AuditFinalResources,
    pub settled_combats: Vec<super::CombatResult>,
    pub search_work: usize,
    pub trajectory_replayed: bool,
}

fn digest(v: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(v).unwrap()))
}

fn apply(e: &mut NaturalEpisode, input: &AuditInput) -> Result<(), String> {
    match input {
        AuditInput::Strategic(input) => {
            e.choose(e.source(), &NaturalAction::Run(input.clone()))?;
        }
        AuditInput::Combat(input) => e.choose_combat_input(e.source(), input)?,
    }
    Ok(())
}

fn execute(
    e: &mut NaturalEpisode,
    report: &mut AuditReport,
    input: AuditInput,
    chosen: String,
    reason: String,
    offered: Vec<String>,
    setup_scores: Vec<SetupCandidateScore>,
) -> Result<(), String> {
    let floor = e.run().floor_num;
    let turn = e
        .active
        .as_ref()
        .map(|a| a.combat_state.turn.turn_count + 1);
    let site = e.engine().kind_v1().to_owned();
    let hp_before = e.resources().hp;
    if let Err(error) = apply(e, &input) {
        report.failed_input = Some(input);
        return Err(error);
    }
    report.decisions.push(AuditDecision {
        floor,
        turn,
        site,
        hp_before,
        chosen,
        reason,
        offered,
        setup_scores,
        input,
        successor_digest: digest(&e.snapshot()),
    });
    Ok(())
}

/// Every declared seed gets a report, including unavailable rules, missing
/// witnesses and execution failures. Only supplied inputs are replayed; replay
/// never invokes the policy again or imports a saved state.
pub fn run(seed: u64, config: AuditConfig) -> Result<AuditReport, String> {
    if config.max_actions == 0
        || config.work_per_combat == 0
        || config.work_per_run == 0
        || config.seconds_per_run == 0
    {
        return Err("audit budgets must be positive".into());
    }
    let mut episode = NaturalEpisode::new(seed, config.max_actions)?;
    let mut report = AuditReport {
        seed,
        config,
        solver_config: SolverConfig::default(),
        information: "oracle_combat_search_visible_setup_rules",
        actor: "early_setup_rules_v1_with_combat_solver",
        decisions: vec![],
        combats: vec![],
        stop: String::new(),
        error: None,
        failed_input: None,
        final_view: String::new(),
        final_snapshot: Value::Null,
        final_resources: AuditFinalResources::from_episode(&episode),
        settled_combats: vec![],
        search_work: 0,
        trajectory_replayed: false,
    };
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(report.config.seconds_per_run))
        .ok_or("duration overflow")?;
    let execution = drive(&mut episode, &mut report, deadline);
    if let Err(error) = execution {
        report.stop = "ExecutionFailure".into();
        report.error = Some(error);
    }
    report.final_snapshot = episode.snapshot();
    report.final_resources = AuditFinalResources::from_episode(&episode);
    report.settled_combats = episode.results().to_vec();
    report.final_view = episode
        .view()
        .unwrap_or_else(|e| format!("View unavailable: {e}"));
    if report.error.is_none() {
        let verification = replay(&report);
        match verification {
            Ok(()) => report.trajectory_replayed = true,
            Err(error) => {
                report.stop = "ReplayFailure".into();
                report.error = Some(error);
            }
        }
    }
    Ok(report)
}

fn drive(
    e: &mut NaturalEpisode,
    report: &mut AuditReport,
    deadline: Instant,
) -> Result<(), String> {
    let mut setup = policy::SetupPolicy::default();
    while e.stop().is_none() {
        if Instant::now() >= deadline {
            report.stop = "TimeBudget".into();
            return Ok(());
        }
        if let Some(active) = &e.active {
            let position = sts_core::sim::combat::CombatPosition::new(
                e.engine.clone(),
                active.combat_state.clone(),
            );
            let work = report.config.work_per_combat.min(
                report
                    .config
                    .work_per_run
                    .saturating_sub(report.search_work),
            );
            if work == 0 {
                report.stop = "RunSearchBudgetBeforeCombat".into();
                return Ok(());
            }
            let mut solver = CombatSolver::new(position.clone(), report.solver_config.clone())?;
            let search = solver.advance(work, deadline.saturating_duration_since(Instant::now()));
            report.search_work += solver.stats().work();
            let plans = solver.plans();
            let elite = e
                .run()
                .map
                .get_current_node()
                .is_some_and(|n| n.class == Some(RoomType::MonsterRoomElite));
            // Declared diagnostic preference, not a potion valuation model:
            // in hallways, conserve if a no-expenditure win loses at most 1/8
            // max HP and leaves at least half health. Otherwise prefer final HP.
            let conserve = !elite
                && plans.iter().any(|p| {
                    p.outcome.total_potion_expenditures == 0
                        && p.outcome.final_hp
                            >= position.combat.entities.player.current_hp
                                - position.combat.entities.player.max_hp / 8
                        && p.outcome.final_hp * 2 >= position.combat.entities.player.max_hp
                });
            let selected = plans
                .iter()
                .enumerate()
                .filter(|(_, p)| !conserve || p.outcome.total_potion_expenditures == 0)
                .max_by_key(|(i, p)| {
                    (
                        p.outcome.final_hp,
                        std::cmp::Reverse(p.outcome.total_potion_expenditures),
                        std::cmp::Reverse(p.inputs.len()),
                        std::cmp::Reverse(*i),
                    )
                })
                .map(|(i, _)| i);
            let selection_reason = if conserve {
                "Hallway reserve rule: choose the healthiest no-expenditure win"
            } else {
                "Choose the healthiest replayed win; use potion expenditure only as a tie-break"
            }
            .to_owned();
            report.combats.push(AuditCombat {
                floor: e.run().floor_num,
                stats: solver.stats().clone(),
                passes: solver.progress(),
                search_stop: search.as_ref().ok().copied(),
                error: search.as_ref().err().cloned(),
                source: position,
                alternatives: plans
                    .iter()
                    .map(|p| serde_json::json!({"outcome":p.outcome,"inputs":p.inputs,"min_stable_hp":p.min_stable_hp,"origin":p.origin}))
                    .collect(),
                selected: if search.is_ok() { selected } else { None },
                selection_reason: selection_reason.clone(),
            });
            search?;
            let Some(selected) = selected else {
                report.stop = "CombatSearchUnknown".into();
                return Ok(());
            };
            for input in &plans[selected].inputs {
                if Instant::now() >= deadline {
                    report.stop = "TimeBudget".into();
                    return Ok(());
                }
                // A complete selected line is adopted input by input on the
                // naturally acquired state. No position replacement occurs.
                let label = e
                    .options()?
                    .iter()
                    .find(|o| {
                        o.action == NaturalAction::Combat(crate::CombatBranch::Input(input.clone()))
                    })
                    .map(|o| o.label.clone())
                    .unwrap_or_else(|| format!("{input:?}"));
                execute(
                    e,
                    report,
                    AuditInput::Combat(input.clone()),
                    label,
                    selection_reason.clone(),
                    vec![],
                    vec![],
                )?;
                if e.stop().is_some() {
                    break;
                }
            }
            if e.active.is_some() && e.stop().is_none() {
                return Err("complete combat plan did not finish the actual fight".into());
            }
        } else {
            let options = e.options()?;
            let decision = match setup.choose(e, &options) {
                Ok(choice) => choice,
                Err(reason) => {
                    report.stop = format!("SetupUnavailable: {reason}");
                    return Ok(());
                }
            };
            let option = &options[decision.0];
            let NaturalAction::Run(input) = &option.action else {
                return Err("setup policy selected combat".into());
            };
            execute(
                e,
                report,
                AuditInput::Strategic(input.clone()),
                option.label.clone(),
                decision.1,
                options.iter().map(|o| o.label.clone()).collect(),
                decision.2,
            )?;
        }
    }
    report.stop = format!("{:?}", e.stop().unwrap());
    Ok(())
}

pub fn replay(report: &AuditReport) -> Result<(), String> {
    let mut episode = NaturalEpisode::new(report.seed, report.config.max_actions)?;
    for (index, decision) in report.decisions.iter().enumerate() {
        apply(&mut episode, &decision.input)?;
        if digest(&episode.snapshot()) != decision.successor_digest {
            return Err(format!("natural replay differs after input {index}"));
        }
    }
    if episode.snapshot() != report.final_snapshot {
        return Err("natural replay final state differs".into());
    }
    Ok(())
}
