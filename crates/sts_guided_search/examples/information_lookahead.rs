//! Observable end-to-end demonstration, not a natural NoSL performance test.
use clap::Parser;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use sts_combat_outcome::{
    agent::{
        belief::{
            CombatBeliefEnvironmentV1, CombatPublicBoundaryV1, HistoryRejuvenationConfig,
            IndependentStreamsCombatBeliefSamplerV1, ObservedSuccessorRescueConfig,
        },
        information::{
            action::{
                project_public_combat_actions_v1, resolve_public_combat_action_v1,
                PublicCombatActionChoiceV1 as Choice, PublicCombatAtomicActionV1 as Atomic,
            },
            run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1},
        },
    },
    combat_witness_materialization::project_replay_checked_combat_outcome_v1,
};
use sts_core::{
    content::{
        cards::CardId, identity::gameplay_display_label_v1 as name, monsters::factory::EncounterId,
        potions::PotionId,
    },
    runtime::combat::Intent,
    sim::combat::{
        CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
    },
    state::map::node::RoomType,
};
use sts_guided_search::{
    information_search::{
        lookahead, policy_successors, replay_policy, ContingentPolicy, LookaheadConfig,
    },
    local::ConstructedCombat,
};
use sts_oracle_learning::eval::{
    combat_case::load_combat_case,
    combat_case_context::restore_combat_case_production_session_v2,
    run_control::{LearningBoundaryV1, LearningEnvV3},
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    output: PathBuf,
    /// Optional JSON array of explicit ConstructedCombat recipes.
    #[arg(long, conflicts_with = "production_case")]
    input: Option<PathBuf>,
    /// Validated production combat case; kept separate from the sampled futures.
    #[arg(long, conflicts_with = "input")]
    production_case: Option<PathBuf>,
    #[arg(long, default_value_t = 6)]
    depth: usize,
    #[arg(long, default_value_t = 3)]
    width: usize,
    #[arg(long, default_value_t = 3_000)]
    work_per_decision: usize,
    #[arg(long, default_value_t = 80_000)]
    total_work: usize,
    /// Independent public-equivalent futures; finite coverage is not a posterior guarantee.
    #[arg(long, default_value_t = 8)]
    particles: usize,
    #[arg(long, default_value_t = 30)]
    seconds: u64,
    #[arg(long)]
    allow_potions: bool,
    /// Disable history-compatible proposals for a fixed-population diagnostic.
    #[arg(long)]
    fixed_population: bool,
    /// Public-history-checked proposals only when the actual observation is uncovered.
    #[arg(long, default_value_t = 0)]
    rescue_attempts_per_donor: usize,
    #[arg(long, default_value_t = 2)]
    rescue_moves_per_attempt: usize,
    #[arg(long, default_value_t = 32)]
    rescue_retained: usize,
    /// Separate transition budget for all uncovered-observation rescues.
    #[arg(long, default_value_t = 0)]
    rescue_work: usize,
}

struct RunCase {
    label: String,
    source: CombatPosition,
    public_run_context: PublicCombatRunContextV1,
    provenance: serde_json::Value,
}

fn run_cases(args: &Args) -> Result<Vec<RunCase>, Box<dyn std::error::Error>> {
    if let Some(path) = &args.production_case {
        let case = load_combat_case(path)?;
        let identity = case.replay_identity_v2()?;
        let session = restore_combat_case_production_session_v2(
            &case.core,
            case.production_context.as_ref(),
        )?;
        let LearningBoundaryV1::Combat { boundary } =
            LearningEnvV3::from_session(session).observe()?
        else {
            return Err("production case did not restore to a combat decision".into());
        };
        return Ok(vec![RunCase {
            label: format!(
                "production case A{} F{} seed {}",
                case.core.run.act, case.core.run.floor, case.core.source.seed
            ),
            source: case.core.position,
            public_run_context: boundary.public_run_context,
            provenance: json!({"kind": "validated_production_combat_case", "path": path,
                "source": case.core.source, "identity": identity}),
        }]);
    }
    let specs = args
        .input
        .as_ref()
        .map(
            |path| -> Result<Vec<ConstructedCombat>, Box<dyn std::error::Error>> {
                Ok(serde_json::from_slice(&fs::read(path)?)?)
            },
        )
        .transpose()?
        .unwrap_or_else(cases);
    specs
        .into_iter()
        .map(|spec| {
            if spec.max_potion_uses.is_some() {
                return Err(
                    "this demo supports slot permissions; recipe potion-count limits must be None"
                        .into(),
                );
            }
            Ok(RunCase {
                label: format!("constructed {:?} A20", spec.encounter),
                source: spec.build()?,
                public_run_context: PublicCombatRunContextV1::Unavailable {
                    reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
                },
                provenance: json!({"kind": "constructed_recipe", "spec": spec}),
            })
        })
        .collect()
}

fn cases() -> Vec<ConstructedCombat> {
    use CardId::*;
    let base = vec![
        (Bash, 0),
        (AscendersBane, 0),
        (Strike, 0),
        (Strike, 0),
        (Strike, 0),
        (Strike, 0),
        (Strike, 0),
        (Defend, 0),
        (Defend, 0),
        (Defend, 0),
        (Defend, 0),
    ];
    let mut ordinary = base.clone();
    ordinary.extend([(FlashOfSteel, 0), (ShrugItOff, 0)]);
    let mut elite = base;
    elite.extend([(Bludgeon, 0), (ShrugItOff, 0), (Headbutt, 0)]);
    vec![
        ConstructedCombat {
            seed: 73,
            encounter: EncounterId::JawWorm,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 68,
            deck: ordinary,
            relics: vec![],
            potions: vec![],
            max_potion_uses: None,
        },
        ConstructedCombat {
            seed: 211,
            encounter: EncounterId::GremlinNob,
            act: 1,
            room: RoomType::MonsterRoomElite,
            hp: 68,
            deck: elite,
            relics: vec![],
            potions: vec![PotionId::FirePotion],
            max_potion_uses: None,
        },
    ]
}

// Deliberately simple and public. This example's ranking is not a trained value
// function, HP/resource exchange rate, mechanism model, or optimality claim.
fn estimate(boundary: &CombatPublicBoundaryV1) -> f64 {
    let observation = match boundary {
        CombatPublicBoundaryV1::Terminal {
            outcome: CombatTerminal::Win,
            observation,
            ..
        } => return observation.player.hp as f64,
        CombatPublicBoundaryV1::Terminal { .. } => return -10_000.0,
        CombatPublicBoundaryV1::Decision { decision } => &decision.observation,
    };
    let enemy_hp = observation
        .monsters
        .iter()
        .map(|m| m.hp.max(0) as f64)
        .sum::<f64>();
    let incoming = observation
        .monsters
        .iter()
        .filter(|m| m.alive)
        .map(|m| match &m.intent.intent {
            Some(
                Intent::Attack { hits, .. }
                | Intent::AttackBuff { hits, .. }
                | Intent::AttackDebuff { hits, .. }
                | Intent::AttackDefend { hits, .. },
            ) => m.intent.preview_damage_per_hit.unwrap_or(0).max(0) as f64 * *hits as f64,
            _ => 0.0,
        })
        .sum::<f64>();
    observation.player.hp as f64 - 0.5 * enemy_hp
        + 0.7 * (observation.player.block.max(0) as f64).min(incoming)
}

fn observation(
    policy: &ContingentPolicy,
) -> &sts_combat_outcome::agent::information::state::PublicCombatStateV1 {
    match &policy.boundary {
        CombatPublicBoundaryV1::Decision { decision } => &decision.observation,
        CombatPublicBoundaryV1::Terminal { observation, .. } => observation,
    }
}

fn action_name(policy: &ContingentPolicy) -> String {
    let (CombatPublicBoundaryV1::Decision { decision }, Some(action)) =
        (&policy.boundary, &policy.action)
    else {
        return match policy.stop {
            Some(sts_guided_search::information_search::LeafStop::Terminal) => {
                "Combat finished".into()
            }
            Some(sts_guided_search::information_search::LeafStop::Work) => {
                "Unknown (work limit)".into()
            }
            Some(sts_guided_search::information_search::LeafStop::Time) => {
                "Unknown (time limit)".into()
            }
            Some(sts_guided_search::information_search::LeafStop::Depth) => {
                "Unknown (depth limit)".into()
            }
            Some(sts_guided_search::information_search::LeafStop::PopulationWork) => {
                "Unknown (candidate reconstruction budget)".into()
            }
            _ => "Unknown (no candidate)".into(),
        };
    };
    match action {
        Choice::Atomic { action_ordinal } => {
            match &decision.actions.atomic_actions[*action_ordinal] {
                Atomic::PlayCard {
                    hand_index,
                    target_monster_index,
                } => {
                    let card = &decision.observation.cards.hand.cards[*hand_index];
                    let upgrade = if card.upgrades == 0 {
                        String::new()
                    } else if card.upgrades == 1 && card.card_id != CardId::SearingBlow {
                        "+".into()
                    } else {
                        format!("+{}", card.upgrades)
                    };
                    format!(
                        "Play {}{}{}",
                        name(card.card_id),
                        upgrade,
                        target_monster_index
                            .map(|i| format!(" -> M{i}"))
                            .unwrap_or_default()
                    )
                }
                Atomic::EndTurn => "End turn".into(),
                Atomic::UsePotion { potion_index, .. } => format!(
                    "Use P{potion_index} {}",
                    name(
                        decision.observation.potions[*potion_index]
                            .as_ref()
                            .unwrap()
                            .potion_id
                    )
                ),
                action => format!("{action:?}"),
            }
        }
        Choice::Selection {
            family_ordinal,
            selected_domain_ordinals,
        } => {
            let family = &decision.actions.selection_families[*family_ordinal];
            let selected = selected_domain_ordinals.iter().map(|i| match &family.domain[*i] {
                sts_combat_outcome::agent::information::action::PublicCombatSelectionDomainCandidateV1::Card { card_id: Some(id), .. } => name(*id).to_owned(),
                _ => format!("choice {i}"),
            }).collect::<Vec<_>>().join(", ");
            format!("Select {selected}")
        }
    }
}

fn tree(policy: &ContingentPolicy, depth: usize, lines: &mut Vec<String>) {
    if lines.len() >= 100 {
        return;
    }
    let c = observation(policy);
    let hand = c
        .cards
        .hand
        .cards
        .iter()
        .map(|card| {
            let cost = match card.effective_cost {
                -2 => "-".into(),
                -1 => "Xc".into(),
                n => format!("{n}c"),
            };
            format!("{}[{cost}]", name(card.card_id))
        })
        .collect::<Vec<_>>()
        .join(", ");
    lines.push(format!(
        "{}{} | HP {} | Hand: {}",
        "  ".repeat(depth),
        action_name(policy),
        c.player.hp,
        hand
    ));
    for branch in &policy.branches {
        if policy.branches.len() > 1 {
            lines.push(format!(
                "{}Scenario mass {:.1}%",
                "  ".repeat(depth + 1),
                branch.probability * 100.0
            ));
            tree(&branch.continuation, depth + 2, lines);
        } else {
            tree(&branch.continuation, depth, lines);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.work_per_decision == 0 || args.total_work == 0 || args.seconds == 0 {
        return Err("budgets must be positive".into());
    }
    if !(1..=256).contains(&args.particles) {
        return Err("particles must be between 1 and 256".into());
    }
    if args.rescue_attempts_per_donor > 0
        && (args.rescue_moves_per_attempt == 0
            || args.rescue_retained == 0
            || args.rescue_work == 0)
    {
        return Err("rescue attempts need positive moves and a separate work budget".into());
    }
    if args.output.exists() && fs::read_dir(&args.output)?.next().is_some() {
        return Err("output must be new or empty".into());
    }
    let cases = run_cases(&args)?;
    fs::create_dir_all(&args.output)?;
    let git_head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    let git_status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;
    let producer = json!({"git_head": String::from_utf8_lossy(&git_head.stdout).trim(),
        "git_worktree_dirty": !git_status.stdout.is_empty(),
        "source_sha256": format!("{:x}", Sha256::digest(concat!(
            include_str!("../src/information_search.rs"), include_str!("information_lookahead.rs"),
            include_str!("../../../src/agent/belief/combat.rs"),
            include_str!("../../../src/agent/belief/environment.rs"),
            include_str!("../../../src/agent/belief/rejuvenation.rs")
        ).as_bytes())),
        "kind": "independent_execution_demo", "execution": "source world, not a candidate particle",
        "sampling": "independent_streams_with_draw_order_constraints", "particle_seeds": (10_001..10_001 + args.particles as u64).collect::<Vec<_>>() });
    for (case_index, case) in cases.iter().enumerate() {
        let source = &case.source;
        let mut environment = CombatBeliefEnvironmentV1::from_sampler(
            case.public_run_context.clone(),
            source,
            &IndependentStreamsCombatBeliefSamplerV1::new(
                (10_001..10_001 + args.particles as u64).collect(),
            )
            .with_draw_order_knowledge(10_000),
        )?;
        let initial = source.clone();
        let mut actual = initial.clone();
        let mut actual_history = environment.public_history().clone();
        let config = LookaheadConfig {
            depth: args.depth,
            width: args.width,
            work: args.work_per_decision,
            max_engine_steps: 512,
            selection_limit: 32,
            allowed_potion_slots: if args.allow_potions {
                (0..source.combat.entities.potions.len()).collect()
            } else {
                vec![]
            },
            rejuvenation: (!args.fixed_population).then_some(HistoryRejuvenationConfig {
                target_particles: args.particles,
                moves_per_particle: 4,
                seed: 70_001,
            }),
        };
        let mut used = 0;
        let mut inputs = Vec::new();
        let mut rows = Vec::new();
        let mut text = vec![
            format!(
                "{} | HP {} | independent execution",
                case.label, source.combat.entities.player.current_hp
            ),
            "Candidates approximate possible futures. Execution follows the separate source world."
                .into(),
        ];
        let deadline = Instant::now() + Duration::from_secs(args.seconds);
        let mut stop = "decision_limit".to_owned();
        let mut policy_replay_work = 0;
        let mut execution_conditioning_work = 0;
        let mut rescue_work = 0;
        for index in 0..64 {
            if matches!(
                environment.public_history().current(),
                CombatPublicBoundaryV1::Terminal { .. }
            ) {
                stop = "terminal".into();
                break;
            }
            if used >= args.total_work || Instant::now() >= deadline {
                stop = "budget".into();
                break;
            }
            let mut config = config.clone();
            config.work = config.work.min(args.total_work - used);
            let report = lookahead(
                &environment,
                &config,
                deadline.saturating_duration_since(Instant::now()),
                &estimate,
                &EngineCombatStepper,
            )?;
            used += report.stats.transitions;
            if index == 0 {
                policy_replay_work =
                    replay_policy(&environment, &report.policy, &EngineCombatStepper, 512)?;
                fs::write(
                    args.output
                        .join(format!("{case_index}-initial-policy.json")),
                    serde_json::to_vec_pretty(&json!({
                    "producer": producer, "source": source, "config": config, "report": report }))?,
                )?;
                let mut lines = Vec::new();
                tree(&report.policy, 0, &mut lines);
                fs::write(
                    args.output.join(format!("{case_index}-initial-policy.txt")),
                    lines.join("\n") + "\n",
                )?;
            }
            let Some(action) = &report.policy.action else {
                stop = action_name(&report.policy);
                break;
            };
            let projection = project_public_combat_actions_v1(&actual.engine, &actual.combat)?;
            inputs.push(resolve_public_combat_action_v1(&projection, action)?);
            let c = observation(&report.policy);
            let label = action_name(&report.policy);
            let enemy_hp = c
                .monsters
                .iter()
                .map(|m| m.hp.to_string())
                .collect::<Vec<_>>()
                .join(",");
            text.push(format!(
                "T{} {label} | HP {} Block {} Energy {} | Enemy HP {enemy_hp}",
                c.turn.turn_count + 1,
                c.player.hp,
                c.player.block,
                c.turn.energy
            ));
            let (branches, work) =
                policy_successors(&environment, &report.policy, &EngineCombatStepper, 512)?;
            execution_conditioning_work += work;
            let (history, next) =
                actual_history.advance_exact(&actual, action.clone(), &EngineCombatStepper, 512)?;
            actual = next;
            actual_history = history;
            let observed = actual_history.entries().last().unwrap();
            let predictions = branches.iter().map(|branch| json!({
                "scenario_mass": branch.probability(),
                "observation": branch.environment().public_history().entries().last().unwrap(),
            })).collect::<Vec<_>>();
            // Match visible outcomes only. No actual lineage, RNG, source seed
            // or particle origin is used to choose a branch.
            let matching = branches
                .into_iter()
                .filter(|branch| branch.matches_observation(observed))
                .collect::<Vec<_>>();
            rows.push(json!({"action": label, "public_boundary": report.policy.boundary, "search": report.stats,
                "rejuvenation": report.policy.rejuvenation, "actual_successor": observed, "matching_branches": matching.len(),
                "predicted_successors": predictions,
                "selected_leaves": report.policy.leaves, "alternatives": report.alternatives.iter().map(|p|
                    json!({"action": action_name(p), "value_estimate": p.value_estimate, "leaves": p.leaves})).collect::<Vec<_>>() }));
            if matching.is_empty() && args.rescue_attempts_per_donor > 0 {
                let settings = ObservedSuccessorRescueConfig {
                    attempts_per_donor: args.rescue_attempts_per_donor,
                    moves_per_attempt: args.rescue_moves_per_attempt,
                    max_retained: args.rescue_retained,
                    seed: 80_001 + index as u64,
                };
                let bound = settings.work_bound(&environment)?;
                if bound > args.rescue_work - rescue_work {
                    stop = "Unknown: insufficient observed-successor rescue work".into();
                    break;
                }
                let remaining_time = deadline.saturating_duration_since(Instant::now());
                if remaining_time.is_zero() {
                    stop = "Unknown: observed-successor rescue time limit".into();
                    break;
                }
                let (recovered, report) = environment.rescue_observed_successor(
                    observed,
                    &settings,
                    bound,
                    remaining_time,
                    &EngineCombatStepper,
                    512,
                )?;
                rescue_work += report.replay_transitions;
                let timed_out = report.timed_out;
                rows.last_mut().unwrap()["observed_successor_rescue"] =
                    json!({"config": settings, "report": report});
                if timed_out {
                    stop = "Unknown: observed-successor rescue timed out".into();
                    break;
                }
                if let Some(next) = recovered {
                    environment = next;
                    continue;
                }
            }
            if matching.len() != 1 {
                stop = "Unknown: observation outside candidate coverage".into();
                break;
            }
            environment = matching.into_iter().next().unwrap().into_environment();
        }
        let endpoint = &actual;
        let mut replay = initial.clone();
        for input in &inputs {
            let result = EngineCombatStepper.apply_to_stable(
                &replay,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            if result.truncated || result.timed_out {
                return Err("execution replay interrupted".into());
            }
            replay = result.position;
        }
        if replay != *endpoint {
            return Err("execution replay endpoint differs".into());
        }
        let outcome = project_replay_checked_combat_outcome_v1(&initial, &inputs, endpoint);
        let terminal = EngineCombatStepper.terminal(endpoint);
        if terminal != CombatTerminal::Unresolved && !stop.starts_with("Unknown:") {
            stop = "terminal".into();
        }
        let summary = format!("Stop {stop} | {terminal:?} | HP {} | Potion expenditures {} | Search transitions {used}", outcome.final_hp, outcome.total_potion_expenditures);
        text.push(summary.clone());
        fs::write(
            args.output.join(format!("{case_index}-run.txt")),
            text.join("\n") + "\n",
        )?;
        fs::write(
            args.output.join(format!("{case_index}-run.json")),
            serde_json::to_vec_pretty(&json!({
            "producer": producer, "provenance": case.provenance, "source": source, "execution_root": initial,
            "config": config, "total_work_limit": args.total_work, "seconds_limit": args.seconds,
            "steps": rows, "inputs": inputs, "endpoint": endpoint, "stop": stop, "terminal": terminal,
            "outcome": outcome, "search_work": used, "execution_replay_work": inputs.len(), "initial_policy_replay_work": policy_replay_work,
            "execution_conditioning_work": execution_conditioning_work, "observed_successor_rescue_work": rescue_work,
            "rescue_work_limit": args.rescue_work, "actual_history": actual_history,
            "limitations": ["history-compatible proposals are correlated and do not estimate a calibrated posterior",
                "distinct private endpoints can differ only in irrelevant RNG streams; counts are not independent evidence",
                "finite populations can miss the actual next observation; execution stops at missing coverage",
                "public heuristic is only a candidate estimate", "width and depth can miss tactics"] }))?,
        )?;
        println!("{}: {summary}", case.label);
    }
    Ok(())
}
