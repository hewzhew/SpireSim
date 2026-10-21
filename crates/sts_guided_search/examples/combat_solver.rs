//! Offline adapter: supplied construction recipes or explicitly sourced roots.
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use sts_core::{
    content::identity::gameplay_display_label_v1 as name,
    sim::{
        combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper},
        combat_action_surface::CombatIndexedChoiceCandidateV2,
    },
    state::core::ClientInput,
};
use sts_guided_search::{
    local::ConstructedCombat,
    observation::consequences::{plan_consequences, PlanProposal},
    solver::{CombatSolver, PassAuditReport, SolverConfig},
};
use sts_oracle_learning::eval::combat_case::{load_combat_case, CombatCase};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    /// Cumulative search work. A single 0 only replays supplied plan_proposals.
    #[arg(long, value_delimiter = ',', default_value = "2000,10000,50000")]
    work: Vec<usize>,
    /// Total wall-clock search allowance per case, shared by milestones.
    #[arg(long, default_value_t = 30.0)]
    seconds: f64,
    #[arg(long, default_value_t = 128)]
    width: usize,
    /// False measures fixed-work round-robin without changing other search rules.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    balance_beam_widths: bool,
    /// Distinct decision contexts get representatives before order variants.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    diversify_decision_contexts: bool,
    /// Experimental next-turn diversity for remaining/exhausted card composition.
    #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
    preserve_card_compositions: bool,
    /// Experimental fair completion of choices belonging to each game action.
    #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
    complete_actions: bool,
    /// Continue new hand/effect opportunities before score retention (experimental).
    #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
    follow_preparations: bool,
    /// Short kill rescue and root effect-order hypotheses for reactive enemies.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    tactical_attacks: bool,
    /// False disables the shared conservative action-equivalence reduction.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    collapse_equivalent_actions: bool,
    /// Incremental one-potion edits of verified plans, within the same budget.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    refine_potion_plans: bool,
    /// Replay adjacent complete-card swaps along this search's verified wins.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    refine_card_order: bool,
    /// Explicit frozen pilot model; validates its domain before starting search.
    #[arg(long)]
    resource_model: Option<PathBuf>,
    /// Export replayed trajectories and complete one-step sources for value fitting.
    /// Offline collection is timed and charged separately from combat search.
    #[arg(long, default_value_t = 0)]
    value_data_work: usize,
    /// Offline work per sampled continuation; zero omits continuation labels.
    #[arg(long, default_value_t = 0)]
    value_continuation_work: usize,
    /// Offline complete-response graphs from recorded turn entries and mode forks.
    #[arg(long, default_value_t = 0)]
    value_response_work: usize,
    /// Per-order continuation work after a verified win; zero disables repair.
    #[arg(long, default_value_t = 0)]
    shuffle_work: usize,
    /// Work per score-tied pre-shuffle alternative; zero disables the experiment.
    #[arg(long, default_value_t = 0)]
    order_family_work: usize,
    /// Capture the first omission of this reference step at --inspect-width, then
    /// enumerate each candidate to its next turn using a separate offline budget.
    #[arg(long)]
    inspect_retention: Option<usize>,
    /// Observed pass width; defaults to --width without changing search admission.
    #[arg(long, requires = "inspect_retention")]
    inspect_width: Option<usize>,
    #[arg(long, default_value_t = false)]
    inspect_card_only: bool,
    #[arg(long, value_delimiter = ',', default_value = "64,256,1024,4096")]
    inspection_work: Vec<usize>,
    /// Optional independent whole-combat continuation per captured candidate.
    #[arg(long, default_value_t = 0)]
    inspection_continuation_work: usize,
    /// Observe at most this many actual contested retention groups (4 states/side).
    #[arg(long, default_value_t = 0)]
    candidate_samples: usize,
}

#[derive(Deserialize, Serialize)]
struct Case {
    label: String,
    /// Replay a supplied prefix before starting a new independent diagnostic
    /// search. Reference inputs, if any, are relative to this derived root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    prefix: Vec<ClientInput>,
    /// Optional diagnostic witness; replayed and watched, never seeded into search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reference: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inventory_context: Option<InventoryContext>,
    /// Concrete source-relative hypotheses, observed independently of search.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    plan_proposals: Vec<PlanProposal>,
    #[serde(flatten)]
    source: Source,
}

#[derive(Deserialize, Serialize)]
struct Reference {
    inputs: Vec<ClientInput>,
    /// Optional expected endpoint; absent endpoints are reconstructed and still
    /// required to be a resource-valid victory by the shared audit owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    successor: Option<CombatPosition>,
}

/// Experimental provenance only; never supplied to the solver's ranking.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InventoryContext {
    group: String,
    role: String,
    reason: String,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
enum Source {
    Constructed {
        spec: ConstructedCombat,
    },
    Snapshot {
        producer: String,
        #[serde(deserialize_with = "deserialize_position")]
        position: Box<CombatPosition>,
        max_potion_uses: Option<usize>,
    },
    /// Read the exported Rust artifact directly, preserving its u64 RNG state.
    /// Relative paths are resolved against the input manifest's directory.
    CombatCase {
        path: PathBuf,
        max_potion_uses: Option<usize>,
    },
}

fn load_source(
    source: &Source,
    manifest_directory: &Path,
) -> Result<(CombatPosition, Option<usize>, Option<CombatCase>), String> {
    match source {
        Source::Constructed { spec } => Ok((spec.build()?, spec.max_potion_uses, None)),
        Source::Snapshot {
            producer,
            position,
            max_potion_uses,
        } => {
            if producer.trim().is_empty() {
                return Err("snapshot producer is required".into());
            }
            Ok(((**position).clone(), *max_potion_uses, None))
        }
        Source::CombatCase {
            path,
            max_potion_uses,
        } => {
            let resolved = manifest_directory.join(path);
            let case = load_combat_case(&resolved)
                .map_err(|error| format!("combat case {}: {error}", resolved.display()))?;
            case.replay_identity_v2()?;
            Ok((case.core.position.clone(), *max_potion_uses, Some(case)))
        }
    }
}

// Serde's tagged/flattened enum buffer does not coerce JSON object keys back
// into integers. Combat snapshots contain indexed maps, so decode this field
// through JSON's own deserializer rather than the enum's generic buffer.
fn deserialize_position<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Box<CombatPosition>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

fn card_label(card: &sts_core::runtime::combat::CombatCard) -> String {
    card_name(card.id, card.upgrades)
}

fn card_name(id: sts_core::content::cards::CardId, upgrades: u8) -> String {
    let suffix = match upgrades {
        0 => String::new(),
        n if n > 1 || id == sts_core::content::cards::CardId::SearingBlow => format!("+{n}"),
        _ => "+".into(),
    };
    format!("{}{suffix}", name(id))
}

/// Offline admission diagnostic only. Search does not consume these results.
/// Input offsets refer to settled gameplay decisions, not virtual tree nodes.
fn entity_input_admission(
    root: &CombatPosition,
    inputs: &[ClientInput],
    max_potion_uses: Option<usize>,
) -> Result<serde_json::Value, String> {
    let states = sts_guided_search::shuffle_order::replay(root, inputs)?;
    let mut potion_uses = 0;
    let mut checked = 0;
    let mut terminal = 0;
    let mut rejected = vec![];
    for (offset, state) in states.into_iter().enumerate() {
        if offset > 0 {
            potion_uses += usize::from(matches!(inputs[offset - 1], ClientInput::UsePotion { .. }));
        }
        if EngineCombatStepper.terminal(&state) != sts_core::sim::combat::CombatTerminal::Unresolved
        {
            terminal += 1;
            continue;
        }
        checked += 1;
        if let Err(error) =
            sts_guided_search::entities::encode_position(state, potion_uses, max_potion_uses)
        {
            rejected.push(serde_json::json!({"input_offset":offset,"error":error}));
        }
    }
    Ok(serde_json::json!({
        "schema":sts_guided_search::entities::SCHEMA,
        "decisions_checked":checked,"terminal_states_skipped":terminal,
        "admitted":checked - rejected.len(),"rejected":rejected,
    }))
}

fn reference_pass_summary(pass: &PassAuditReport, input_counts: &[usize]) -> String {
    let expanded = pass
        .steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step.events.contains_key("expanded"))
        .map(|(i, _)| input_counts[i])
        .max();
    let mut summary = format!(
        "Reference width {} ({}): max expanded input {}",
        pass.width,
        if pass.card_only {
            "cards only"
        } else {
            "ordinary"
        },
        expanded.map_or_else(|| "none".into(), |input| input.to_string())
    );
    if let Some((i, step)) = pass
        .steps
        .iter()
        .enumerate()
        .find(|(_, step)| step.first_omission.is_some())
    {
        let omission = step.first_omission.as_ref().unwrap();
        summary.push_str(&format!(
            "; first omission step {i} (input {}), {} capacity {}, ranks {}; {}",
            input_counts[i],
            omission.pool,
            omission.capacity,
            omission
                .ranks
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join("/"),
            if step.events.contains_key("expanded") {
                "also expanded"
            } else {
                "not expanded in this budget"
            }
        ));
    } else {
        summary.push_str("; no scored omission observed");
    }
    for (event, label) in [
        ("turn_quota", "turn quota"),
        ("reserve_full", "reserve full"),
    ] {
        if let Some((i, _)) = pass
            .steps
            .iter()
            .enumerate()
            .find(|(_, step)| step.events.contains_key(event))
        {
            summary.push_str(&format!("; {label} step {i} (input {})", input_counts[i]));
        }
    }
    summary
}

fn trace(root: &CombatPosition, inputs: &[ClientInput]) -> Result<String, String> {
    let mut p = root.clone();
    let mut lines = vec![];
    for input in inputs {
        let c = &p.combat;
        let target_label = |target: &Option<_>| {
            target
                .and_then(|id| c.entities.monsters.iter().position(|m| m.id == id))
                .map(|i| format!(" -> M{i} {}", name(c.entities.monsters[i].enemy_id)))
                .unwrap_or_default()
        };
        let action = match input {
            ClientInput::PlayCard { card_index, target } => {
                let card = &c.zones.hand[*card_index];
                let target = target_label(target);
                format!("Play {}{target}", card_label(card))
            }
            ClientInput::UsePotion {
                potion_index,
                target,
            } => format!(
                "Use P{potion_index} {}{}",
                name(c.entities.potions[*potion_index].as_ref().unwrap().id),
                target_label(target)
            ),
            ClientInput::SubmitDiscoverChoice(index) => {
                let surface = EngineCombatStepper.legal_action_surface(&p);
                let candidate = surface
                    .indexed_choice
                    .as_ref()
                    .and_then(|choice| choice.candidates.get(*index))
                    .ok_or("trace missing indexed choice")?;
                let label = match candidate {
                    CombatIndexedChoiceCandidateV2::Card { card_id, upgrades } => {
                        card_name(*card_id, *upgrades)
                    }
                    CombatIndexedChoiceCandidateV2::Stance { stance } => name(*stance).to_owned(),
                };
                format!("Choose {label}")
            }
            ClientInput::EndTurn => "End turn".into(),
            ClientInput::SubmitSelection(selection) => {
                let cards = c
                    .zones
                    .hand
                    .iter()
                    .chain(c.zones.draw_pile.iter())
                    .chain(c.zones.discard_pile.iter())
                    .chain(c.zones.exhaust_pile.iter())
                    .chain(c.zones.limbo.iter())
                    .collect::<Vec<_>>();
                let names = selection
                    .selected
                    .iter()
                    .map(|selected| {
                        cards
                            .iter()
                            .find(|card| card.lineage == selected.card_lineage())
                            .map(|card| card_label(card))
                            .ok_or("selection card missing from trace source")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                format!("Select {}", names.join(", "))
            }
            other => format!("{other:?}"),
        };
        let before_turn = c.turn.turn_count + 1;
        let next = EngineCombatStepper.apply_to_stable(
            &p,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        if next.truncated || next.timed_out {
            return Err("trace replay interrupted".into());
        }
        p = next.position;
        let enemies = p
            .combat
            .entities
            .monsters
            .iter()
            .map(|m| format!("{} {}", name(m.enemy_id), m.current_hp))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "T{before_turn} {action} | HP {} Block {} | {enemies}",
            p.combat.entities.player.current_hp, p.combat.entities.player.block
        ));
    }
    Ok(lines.join("\n") + "\n")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.work.is_empty()
        || (args.work[0] == 0 && args.work.len() != 1)
        || args.work.windows(2).any(|w| w[0] >= w[1])
        || !args.seconds.is_finite()
        || args.seconds <= 0.0
    {
        return Err("work must increase and seconds must be positive".into());
    }
    if args.value_data_work == 0
        && (args.value_continuation_work > 0 || args.value_response_work > 0)
    {
        return Err("offline continuations/responses require --value-data-work".into());
    }
    let cases: Vec<Case> = serde_json::from_slice(&fs::read(&args.input)?)?;
    if args.work == [0]
        && (cases.is_empty() || cases.iter().any(|case| case.plan_proposals.is_empty()))
    {
        return Err("--work 0 requires supplied plan_proposals in every case".into());
    }
    if args.output.exists() && fs::read_dir(&args.output)?.next().is_some() {
        return Err("output directory must be new or empty; preserve earlier results".into());
    }
    fs::create_dir_all(&args.output)?;
    let implementation = [
        include_str!("../src/solver.rs"),
        include_str!("../src/solver/graph.rs"),
        include_str!("../src/solver/audit.rs"),
        include_str!("../src/solver/sampling.rs"),
        include_str!("../src/solver/actions.rs"),
        include_str!("../src/solver/tactics.rs"),
        include_str!("../src/solver/tactics/effects.rs"),
        include_str!("../src/solver/tactics/response.rs"),
        include_str!("../src/solver/inspection.rs"),
        include_str!("../src/solver/shuffle.rs"),
        include_str!("../src/solver/refinement.rs"),
        include_str!("../src/shuffle_order.rs"),
        include_str!("../src/observation.rs"),
        include_str!("../src/observation/monster.rs"),
        include_str!("../src/entities.rs"),
        include_str!("../src/entities/effects.rs"),
        include_str!("../src/entities/potions.rs"),
        include_str!("../src/observation/consequences.rs"),
        include_str!("../../../src/sim/combat.rs"),
        include_str!("../../../src/sim/combat_projection.rs"),
        include_str!("../../../src/engine/action_handlers/cards/turn_triggers.rs"),
        include_str!("../../../src/engine/action_handlers/damage/health.rs"),
        include_str!("../../../src/runtime/combat/oracle_shuffle_observation.rs"),
        include_str!("../src/solver/retention.rs"),
        include_str!("../src/solver/retention/recovery.rs"),
        include_str!("../src/solver/retention/draw_triggers.rs"),
        include_str!("../../../src/content/powers/mod.rs"),
        include_str!("../../../src/content/powers/ironclad/evolve.rs"),
        include_str!("../../../src/content/powers/ironclad/fire_breathing.rs"),
        include_str!("combat_solver.rs"),
        include_str!("../src/combat.rs"),
        include_str!("../../../src/sim/combat_action_equivalence.rs"),
        include_str!("../../../src/eval/combat_witness_materialization.rs"),
        include_str!("../src/resource_value.rs"),
        include_str!("../src/resource_value/data.rs"),
    ]
    .concat();
    let implementation_sha256 = format!("{:x}", Sha256::digest(implementation.as_bytes()));
    let git_head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    let git_status = std::process::Command::new("git")
        .args(["status", "--short"])
        .output()?;
    let mut summary = vec![];
    for (index, case) in cases.iter().enumerate() {
        let (root, potion_limit, source_combat_case) = load_source(
            &case.source,
            args.input.parent().unwrap_or_else(|| Path::new(".")),
        )?;
        let root = if case.prefix.is_empty() {
            root
        } else {
            sts_guided_search::shuffle_order::replay(&root, &case.prefix)?
                .pop()
                .ok_or("empty prefix replay")?
        };
        let admission_start = Instant::now();
        let root_entity_admission = entity_input_admission(&root, &[], potion_limit)?;
        let root_entity_admission_ms = admission_start.elapsed().as_secs_f64() * 1000.;
        let resource_model: Option<sts_guided_search::resource_value::ValueModel> = args
            .resource_model
            .as_ref()
            .map(|path| -> Result<_, Box<dyn std::error::Error>> {
                Ok(serde_json::from_slice(&fs::read(path)?)?)
            })
            .transpose()?;
        let prediction_start = Instant::now();
        let plan_value_predictions = resource_model
            .as_ref()
            .map(|model| {
                case.plan_proposals
                    .iter()
                    .map(|proposal| {
                        model
                            .prediction_trace(&root, &proposal.inputs)
                            .map(|trace| serde_json::json!({"label":proposal.label,"trace":trace}))
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?;
        let prediction_elapsed_ms = prediction_start.elapsed().as_secs_f64() * 1000.;
        let consequence_start = Instant::now();
        let mut labels = std::collections::HashSet::new();
        let consequences = case
            .plan_proposals
            .iter()
            .map(|proposal| {
                if !labels.insert(&proposal.label) {
                    return Err("plan proposal labels must be unique within a case".to_string());
                }
                plan_consequences(&root, proposal)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let consequence_elapsed_ms = consequence_start.elapsed().as_secs_f64() * 1000.0;
        for result in &consequences {
            for &(offset, frame_index) in &result.player_turn_frames {
                let observation = &result.frames[frame_index].observation;
                let enemies = observation
                    .monsters
                    .iter()
                    .map(|m| format!("{}:{}", m.name, m.hp[0]))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "{} / {} | T+{} | HP {} | enemies [{}] | potion uses {}",
                    case.label,
                    result.label,
                    offset,
                    observation.hp[0],
                    enemies,
                    observation.potion_uses
                );
            }
            let last = result.frames.last().ok_or("empty plan consequences")?;
            println!(
                "{} / {} | endpoint {:?} | unreached turns {:?}",
                case.label, result.label, last.boundary, result.unreached_player_turns
            );
        }
        if args.work == [0] {
            let artifact = serde_json::json!({"case":case,"root":root,"source_combat_case":source_combat_case,
                "implementation_sha256":implementation_sha256,
                "workspace_git_head":String::from_utf8_lossy(&git_head.stdout).trim(),
                "workspace_status":String::from_utf8_lossy(&git_status.stdout),
                "information":"full_state_oracle","search_work":0,
                "root_entity_admission":root_entity_admission,"root_entity_admission_ms":root_entity_admission_ms,
                "consequence_elapsed_ms":consequence_elapsed_ms,"plan_consequences":consequences,
                "plan_value_predictions":plan_value_predictions,"prediction_elapsed_ms":prediction_elapsed_ms});
            fs::write(
                args.output.join(format!("case-{index:03}.json")),
                serde_json::to_vec_pretty(&artifact)?,
            )?;
            summary.push(
                serde_json::json!({"label":case.label,"case_file":format!("case-{index:03}.json"),
                "search_work":0,"proposal_count":consequences.len()}),
            );
            continue;
        }
        let config = SolverConfig {
            max_width: args.width,
            max_potion_uses: potion_limit,
            balance_beam_widths: args.balance_beam_widths,
            diversify_decision_contexts: args.diversify_decision_contexts,
            preserve_card_compositions: args.preserve_card_compositions,
            complete_actions: args.complete_actions,
            follow_preparations: args.follow_preparations,
            tactical_attacks: args.tactical_attacks,
            collapse_equivalent_actions: args.collapse_equivalent_actions,
            refine_potion_plans: args.refine_potion_plans,
            refine_card_order: args.refine_card_order,
            resource_model,
            shuffle_continuation_work: args.shuffle_work,
            order_family_work: args.order_family_work,
            ..Default::default()
        };
        let mut solver = CombatSolver::new(root.clone(), config.clone())?;
        if args.candidate_samples > 0 {
            solver.sample_candidates(args.candidate_samples, 4)?;
        }
        if let Some(reference) = &case.reference {
            let reconstructed;
            let expected = if let Some(expected) = &reference.successor {
                expected
            } else {
                reconstructed = sts_guided_search::shuffle_order::replay(&root, &reference.inputs)?
                    .pop()
                    .ok_or("empty reference replay")?;
                &reconstructed
            };
            solver.audit_reference(&reference.inputs, expected)?;
        }
        if let Some(step) = args.inspect_retention {
            solver.capture_retention(
                step,
                args.inspect_width.unwrap_or(args.width),
                args.inspect_card_only,
            )?;
        }
        let start = Instant::now();
        let mut milestones = vec![];
        for &work in &args.work {
            let remaining = Duration::from_secs_f64(args.seconds).saturating_sub(start.elapsed());
            let stop = solver.advance(work.saturating_sub(solver.stats().work()), remaining)?;
            let outcomes = solver.plans().iter().map(|p| serde_json::json!({"hp":p.outcome.final_hp,"min_stable_hp":p.min_stable_hp,"potion_uses":p.outcome.total_potion_expenditures,"spent_slots":p.outcome.starting_potion_expenditure_slots,"remaining":p.successor.combat.entities.potions,"turn":p.successor.combat.turn.turn_count+1,"found_at_work":p.found_at_work,"origin":p.origin,"outcome":p.outcome})).collect::<Vec<_>>();
            milestones.push(serde_json::json!({"requested_work":work,"stop":stop,"elapsed_ms":start.elapsed().as_millis(),"stats":solver.stats(),"passes":solver.progress(),"shuffle_progress":solver.shuffle_progress(),"outcomes":outcomes}));
            let alternatives = solver
                .plans()
                .iter()
                .take(4)
                .map(|p| {
                    let reserve = p
                        .successor
                        .combat
                        .entities
                        .potions
                        .iter()
                        .flatten()
                        .map(|p| name(p.id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "HP {} (use {}, keep {})",
                        p.outcome.final_hp,
                        p.outcome.total_potion_expenditures,
                        if reserve.is_empty() { "-" } else { &reserve },
                    )
                })
                .collect::<Vec<_>>();
            let result = if alternatives.is_empty() {
                "Unknown".into()
            } else {
                alternatives.join("; ")
            };
            println!(
                "{} | work {} | {result} | {:.2}s",
                case.label,
                solver.stats().work(),
                start.elapsed().as_secs_f64()
            );
            if solver.is_complete() || start.elapsed().as_secs_f64() >= args.seconds {
                break;
            }
        }
        let inspection = if args.inspect_retention.is_some() {
            solver.inspect_retention(
                &args.inspection_work,
                Duration::from_secs_f64(args.seconds),
                args.inspection_continuation_work,
            )?
        } else {
            None
        };
        if args.value_data_work > 0 {
            let start = Instant::now();
            let data = sts_guided_search::resource_value::data::collect(
                &solver,
                args.value_data_work,
                args.value_continuation_work,
                args.value_response_work,
            )?;
            let predictions = config.resource_model.as_ref().map(|model| {
                data.rows
                    .iter()
                    .map(|r| model.predict_features(&r.features))
                    .collect::<Vec<_>>()
            });
            fs::write(
                args.output.join(format!("case-{index:03}-value.json")),
                serde_json::to_vec(&serde_json::json!({
                "case":case, "root":root, "data":data, "predictions":predictions, "collection_ms":start.elapsed().as_millis(),
                "collection_budgets": {"branches": args.value_data_work, "continuations_per_source": args.value_continuation_work, "responses": args.value_response_work},
                    "implementation_sha256":implementation_sha256,
                }))?,
            )?;
        }
        let generated_proposals = solver.effect_plan_proposals();
        if let Some(samples) = solver.candidate_samples() {
            fs::write(
                args.output.join(format!("case-{index:03}-candidates.json")),
                serde_json::to_vec(&serde_json::json!({
                    "case":case, "root":root, "config":config, "samples":samples,
                    "implementation_sha256":implementation_sha256,
                    "workspace_git_head":String::from_utf8_lossy(&git_head.stdout).trim(),
                    "workspace_status":String::from_utf8_lossy(&git_status.stdout),
                    "information":"full_state_oracle", "target":null,
                }))?,
            )?;
        }
        let start = Instant::now();
        let generated_consequences = generated_proposals
            .iter()
            .map(|proposal| plan_consequences(&root, proposal))
            .collect::<Result<Vec<_>, _>>()?;
        let generated_consequence_elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
        let admission_start = Instant::now();
        let plan_entity_admission = solver
            .plans()
            .iter()
            .map(|plan| entity_input_admission(&root, &plan.inputs, potion_limit))
            .collect::<Result<Vec<_>, _>>()?;
        let plan_entity_admission_ms = admission_start.elapsed().as_secs_f64() * 1000.;
        let audit = solver.audit();
        if let Some(audit) = &audit {
            let inputs = audit
                .reference
                .iter()
                .map(|step| step.inputs_completed)
                .collect::<Vec<_>>();
            for pass in &audit.passes {
                println!("{}", reference_pass_summary(pass, &inputs));
            }
        }
        let artifact = serde_json::json!({"case":case,"root":root,"source_combat_case":source_combat_case,"config":config,"implementation_sha256":implementation_sha256,"workspace_git_head":String::from_utf8_lossy(&git_head.stdout).trim(),"workspace_status":String::from_utf8_lossy(&git_status.stdout),"information":"full_state_oracle","milestones":milestones,"plans":solver.plans(),"observed_loss_sample":solver.loss_sample(),"audit":audit,"retention_inspection":inspection,"plan_consequences":consequences,"consequence_elapsed_ms":consequence_elapsed_ms,"plan_value_predictions":plan_value_predictions,"prediction_elapsed_ms":prediction_elapsed_ms,"generated_plan_proposals":generated_proposals,"generated_plan_consequences":generated_consequences,"generated_consequence_elapsed_ms":generated_consequence_elapsed_ms,"root_entity_admission":root_entity_admission,"root_entity_admission_ms":root_entity_admission_ms,"plan_entity_admission":plan_entity_admission,"plan_entity_admission_ms":plan_entity_admission_ms});
        fs::write(
            args.output.join(format!("case-{index:03}.json")),
            serde_json::to_vec_pretty(&artifact)?,
        )?;
        if let Some(reference) = &case.reference {
            fs::write(
                args.output.join(format!("case-{index:03}-reference.txt")),
                trace(&root, &reference.inputs)?,
            )?;
        }
        for (plan_index, plan) in solver.plans().iter().enumerate() {
            fs::write(
                args.output
                    .join(format!("case-{index:03}-plan-{plan_index:02}.txt")),
                trace(&root, &plan.inputs)?,
            )?;
        }
        if let Some(sample) = solver.loss_sample() {
            fs::write(
                args.output
                    .join(format!("case-{index:03}-observed-loss.txt")),
                trace(&root, &sample.inputs)?,
            )?;
        }
        summary.push(serde_json::json!({"label":case.label,"inventory_context":case.inventory_context,"case_file":format!("case-{index:03}.json"),"milestones":milestones}));
    }
    fs::write(
        args.output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{cards::CardId, monsters::factory::EncounterId};
    use sts_core::state::map::node::RoomType;

    #[test]
    fn audit_summary_keeps_virtual_step_identity_and_does_not_call_omission_permanent() {
        use sts_guided_search::solver::{AuditCount, RetentionAudit, StepAudit, WorkAudit};
        let mut pass = PassAuditReport {
            width: 4,
            card_only: false,
            work: WorkAudit::default(),
            steps: vec![StepAudit::default(); 4],
            quotas: vec![],
        };
        pass.steps[0].events.insert(
            "expanded".into(),
            AuditCount {
                count: 1,
                first_work: Some(0),
            },
        );
        pass.steps[2].first_omission = Some(RetentionAudit {
            pool: "next_action".into(),
            candidates: 10,
            capacity: 4,
            ranks: [8, 9, 7, 6, 5],
            equal_scores: 0,
            kept: false,
            kept_without_backed_guidance: false,
            kept_without_resource_model: None,
            competitors: vec![],
            same_context_kept: None,
            backed_returns: vec![],
            backed_kept: vec![],
        });
        let summary = reference_pass_summary(&pass, &[0, 1, 1, 2]);
        assert!(summary.contains("max expanded input 0"), "{summary}");
        assert!(summary.contains("step 2 (input 1)"), "{summary}");
        assert!(summary.contains("not expanded in this budget"), "{summary}");
        pass.steps[2].events.insert(
            "expanded".into(),
            AuditCount {
                count: 1,
                first_work: Some(90),
            },
        );
        let recovered = reference_pass_summary(&pass, &[0, 1, 1, 2]);
        assert!(recovered.contains("also expanded"), "{recovered}");
        assert!(!recovered.contains("not expanded"), "{recovered}");
    }

    #[test]
    fn audit_summary_distinguishes_unserved_queued_work_from_scored_omissions() {
        use sts_guided_search::solver::{AuditCount, StepAudit, WorkAudit};
        let mut pass = PassAuditReport {
            width: 8,
            card_only: true,
            work: WorkAudit::default(),
            steps: vec![StepAudit::default(); 3],
            quotas: vec![],
        };
        for event in ["generated", "retained", "turn_quota", "reserve_full"] {
            pass.steps[1].events.insert(
                event.into(),
                AuditCount {
                    count: 1,
                    first_work: Some(30),
                },
            );
        }
        let summary = reference_pass_summary(&pass, &[0, 1, 2]);
        assert!(summary.contains("cards only"), "{summary}");
        assert!(summary.contains("max expanded input none"), "{summary}");
        assert!(summary.contains("no scored omission observed"), "{summary}");
        assert!(summary.contains("turn quota step 1 (input 1)"), "{summary}");
        assert!(
            summary.contains("reserve full step 1 (input 1)"),
            "{summary}"
        );
    }

    #[test]
    fn entity_admission_replays_settled_offsets_and_counts_spent_potions() {
        let root = ConstructedCombat {
            seed: 37,
            encounter: EncounterId::SlimeBoss,
            act: 1,
            room: RoomType::MonsterRoomBoss,
            hp: 68,
            deck: vec![(CardId::Defend, 0); 5],
            relics: vec![],
            potions: vec![sts_core::content::potions::PotionId::BlockPotion],
            max_potion_uses: None,
        }
        .build()
        .unwrap();
        let before = root.clone();
        let inputs = [
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
            ClientInput::EndTurn,
        ];
        let admitted = entity_input_admission(&root, &inputs, Some(1)).unwrap();
        assert_eq!(admitted["decisions_checked"], 3);
        assert_eq!(admitted["admitted"], 3);
        assert_eq!(admitted["terminal_states_skipped"], 0);
        let exceeded = entity_input_admission(&root, &inputs, Some(0)).unwrap();
        assert_eq!(exceeded["admitted"], 1);
        let rejected = exceeded["rejected"].as_array().unwrap();
        assert_eq!(rejected.len(), 2);
        assert_eq!(rejected[0]["input_offset"], 1);
        assert_eq!(rejected[1]["input_offset"], 2);
        assert_eq!(root, before);
    }

    #[test]
    fn entity_admission_reports_uninspected_enemy_without_rejecting_the_search_case() {
        let root = ConstructedCombat {
            seed: 37,
            encounter: EncounterId::GremlinLeader,
            act: 2,
            room: RoomType::MonsterRoomElite,
            hp: 68,
            deck: vec![(CardId::Defend, 0); 5],
            relics: vec![],
            potions: vec![],
            max_potion_uses: None,
        }
        .build()
        .unwrap();
        let admission = entity_input_admission(&root, &[], None).unwrap();
        assert_eq!(admission["decisions_checked"], 1);
        assert_eq!(admission["admitted"], 0);
        assert!(admission["rejected"][0]["error"]
            .as_str()
            .unwrap()
            .contains("enemy decision state"));
    }

    #[test]
    fn snapshot_case_roundtrips_nonempty_integer_indexed_monster_state() {
        let position = ConstructedCombat {
            seed: 37,
            encounter: EncounterId::SmallSlimes,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 68,
            deck: vec![(CardId::Strike, 0), (CardId::Defend, 0)],
            relics: vec![],
            potions: vec![],
            max_potion_uses: None,
        }
        .build()
        .unwrap();
        assert!(!position.combat.runtime.monster_protocol.is_empty());
        let input = serde_json::to_string(&vec![Case {
            label: "constructed snapshot roundtrip".into(),
            prefix: vec![],
            reference: None,
            inventory_context: None,
            plan_proposals: vec![],
            source: Source::Snapshot {
                producer: "unit-test construction".into(),
                position: Box::new(position.clone()),
                max_potion_uses: None,
            },
        }])
        .unwrap();
        let decoded: Vec<Case> = serde_json::from_str(&input).unwrap();
        let Source::Snapshot {
            position: actual, ..
        } = &decoded[0].source
        else {
            panic!("wrong source kind");
        };
        assert_eq!(actual.as_ref(), &position);
    }

    #[test]
    fn combat_case_file_preserves_rng_and_discovery_without_manifest_copy() {
        use sts_core::content::potions::PotionId;
        use sts_oracle_learning::eval::combat_case::{
            save_combat_case, CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary,
            CombatCaseSource, CombatCaseWitnessBudgetV1,
        };
        let mut position = ConstructedCombat {
            seed: 37,
            encounter: EncounterId::SmallSlimes,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 68,
            deck: vec![(CardId::Strike, 0), (CardId::Defend, 0)],
            relics: vec![],
            potions: vec![PotionId::PowerPotion],
            max_potion_uses: None,
        }
        .build()
        .unwrap();
        position.combat.rng.card_random_rng.seed0 = 391671782463821179;
        position.combat.rng.card_random_rng.seed1 = 5725662666139093745;
        let case = CombatCase::new(
            CombatCaseSource {
                seed: 37,
                ascension: 20,
                generation: 0,
                branch_id: 0,
                parent_id: None,
            },
            CombatCaseGap {
                boundary: "constructed discovery input".into(),
                reason: "exact file input test".into(),
                witness_budget: CombatCaseWitnessBudgetV1::NotRun,
            },
            CombatCaseRunSummary {
                act: 1,
                floor: 1,
                hp: 68,
                max_hp: position.combat.entities.player.max_hp,
                gold: 0,
                deck_size: 2,
                relic_count: 0,
                potion_slots: position.combat.entities.potions.len(),
            },
            vec![],
            None,
            vec![],
            CombatCaseRngSummary::from_pool(&position.combat.rng.pool),
            position.clone(),
        );
        let directory = std::env::temp_dir().join(format!(
            "sts_solver_case_file_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let artifact = directory.join("original.json");
        save_combat_case(&artifact, &case).unwrap();
        let manifest: Vec<Case> = serde_json::from_str(
            r#"[{"label":"file input","source":"combat_case","path":"original.json","max_potion_uses":2}]"#,
        )
        .unwrap();
        let (loaded, limit, provenance) = load_source(&manifest[0].source, &directory).unwrap();
        assert_eq!(loaded, position);
        assert_eq!(limit, Some(2));
        assert_eq!(provenance.unwrap().core.position, position);
        let potion = ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        };
        assert_eq!(
            sts_guided_search::shuffle_order::step(&loaded, potion.clone()).unwrap(),
            sts_guided_search::shuffle_order::step(&position, potion).unwrap()
        );
        let encoded_manifest = serde_json::to_value(&manifest).unwrap();
        assert!(encoded_manifest[0].get("position").is_none());
        fs::remove_file(artifact).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
