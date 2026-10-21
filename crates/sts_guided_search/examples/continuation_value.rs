//! Fresh returns from a declared continuation executor. Only states after an
//! exploratory action receive its return; the pre-action source is not labeled.
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use sts_combat_atomic::RetainedCombatTacticalPolicyV1;
use sts_combat_planner::SelectionTransactionCursor;
use sts_combat_tactics::{CombatActionPolicy, CombatPolicyChoice};
use sts_core::{
    sim::combat::{CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper},
    state::core::ClientInput,
};
use sts_guided_search::{
    entities,
    local::ConstructedCombat,
    resource_value as value, shuffle_order,
    solver::{CombatSolver, SolverConfig, SolverStats, SolverStop},
    CombatSearchState,
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 256)]
    max_inputs: usize,
    /// Input-index cadence along the executed source trajectories.
    #[arg(long, default_value_t = 8)]
    branch_every: usize,
    /// Total independently labeled search anchors per case, including its root.
    #[arg(long, default_value_t = 32)]
    max_anchors: usize,
    /// Independently metered search at each labeled anchor; 0 is tactical-only.
    #[arg(long, default_value_t = 10000)]
    solver_work: usize,
    /// Frozen evaluator used by this round's search and source trajectories.
    #[arg(long)]
    resource_model: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    label: String,
    /// Every intervention sharing initialization randomness stays in this group.
    group: String,
    spec: ConstructedCombat,
    /// Exact source preparation before collecting independent continuations.
    /// Same initialization seed remains in the same split group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    prefix: Vec<ClientInput>,
}

impl Case {
    fn build_root(&self) -> Result<CombatPosition, String> {
        let initial = self.spec.build()?;
        shuffle_order::replay(&initial, &self.prefix)?
            .pop()
            .ok_or_else(|| "empty source replay".into())
    }
}

#[derive(Serialize)]
struct Row {
    features: Vec<f32>,
    entities: entities::EntityPosition,
    trace: usize,
    /// Offset in the executed suffix. No exploratory prefix is a training row.
    offset: usize,
}

#[derive(Serialize)]
struct Trace {
    prefix: Vec<ClientInput>,
    inputs: Vec<ClientInput>,
    endpoint: CombatPosition,
    terminal_return: Option<f32>,
    terminal_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tactical_control: Option<Control>,
    #[serde(skip_serializing_if = "Option::is_none")]
    search: Option<SearchReceipt>,
}

#[derive(Serialize)]
struct Control {
    inputs: Vec<ClientInput>,
    endpoint: CombatPosition,
    terminal_return: Option<f32>,
}

#[derive(Serialize)]
struct SearchReceipt {
    stats: SolverStats,
    stop: SolverStop,
    elapsed_ms: f64,
    selected_solver_plan: bool,
}

fn terminal_return(p: &CombatPosition) -> Option<f32> {
    match EngineCombatStepper.terminal(p) {
        CombatTerminal::Win if EngineCombatStepper.is_exact_victory(p) => {
            Some(p.combat.entities.player.current_hp as f32 / 75.)
        }
        CombatTerminal::Loss => Some(-1.),
        _ => None,
    }
}

fn terminal_kind(p: &CombatPosition) -> &'static str {
    match EngineCombatStepper.terminal(p) {
        CombatTerminal::Win if EngineCombatStepper.is_exact_victory(p) => "win",
        CombatTerminal::Win => "escape",
        CombatTerminal::Loss => "loss",
        CombatTerminal::Unresolved => "cutoff",
    }
}

const ENTITY_OBJECTIVE: &str = "fixed_executor_actual_outcome_and_retained_hp_v1";

fn verify(
    root: &CombatPosition,
    prefix: &[ClientInput],
    states: &[CombatPosition],
    inputs: &[ClientInput],
) -> Result<(), String> {
    let full_inputs = prefix.iter().chain(inputs).cloned().collect::<Vec<_>>();
    if &shuffle_order::replay(root, &full_inputs)?[prefix.len()..] != states {
        return Err("independent continuation replay mismatch".into());
    }
    Ok(())
}

fn argmax(weights: &[f64]) -> Result<usize, String> {
    if weights.is_empty() || weights.iter().any(|w| !w.is_finite() || *w < 0.) {
        return Err("invalid frozen-policy weights".into());
    }
    // Stable source order resolves ties; no RNG is consumed by the actor.
    Ok((1..weights.len()).fold(
        0,
        |best, i| if weights[i] > weights[best] { i } else { best },
    ))
}

fn choices(p: &CombatPosition) -> Result<(Vec<ClientInput>, usize), String> {
    let mut surface = EngineCombatStepper.legal_action_surface(p);
    // The planner and executed fallback share the same preserve-potions contract.
    surface.atomic_actions.retain(|input| {
        !matches!(
            input,
            ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
        )
    });
    let policy = RetainedCombatTacticalPolicyV1;
    let refs = surface
        .atomic_actions
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .chain(
            surface
                .selection_families
                .iter()
                .map(CombatPolicyChoice::StructuredSelection),
        )
        .collect::<Vec<_>>();
    let weights = policy.weights(p, &refs);
    if weights.len() != refs.len() {
        return Err("frozen policy changed action width".into());
    }
    let selected = argmax(&weights)?;
    let atomic_len = surface.atomic_actions.len();
    drop(refs);
    let mut actions = surface.atomic_actions;
    let mut preferred = selected;
    for (i, family) in surface.selection_families.iter().enumerate() {
        let mut cursor = SelectionTransactionCursor::new(family).map_err(|e| format!("{e:?}"))?;
        // Pilot cards only need single-card choices. Refuse larger languages
        // rather than silently truncate the frozen actor's action set.
        if cursor.remaining_input_count() > 64 {
            return Err("continuation pilot selection exceeds 64 exact inputs".into());
        }
        let members = std::iter::from_fn(|| cursor.next_input()).collect::<Vec<_>>();
        if selected == atomic_len + i {
            let member_weights = policy.structured_selection_member_weights(p, family, &members);
            if member_weights.len() != members.len() {
                return Err("frozen policy changed selection width".into());
            }
            preferred = actions.len() + argmax(&member_weights)?;
        }
        actions.extend(members);
    }
    Ok((actions, preferred))
}

fn rollout(
    start: &CombatPosition,
    max_inputs: usize,
) -> Result<(Vec<CombatPosition>, Vec<ClientInput>), String> {
    let mut states = vec![start.clone()];
    let mut inputs = vec![];
    for _ in 0..max_inputs {
        let p = states.last().unwrap();
        if EngineCombatStepper.terminal(p) != CombatTerminal::Unresolved {
            break;
        }
        let (actions, chosen) = choices(p)?;
        let input = actions[chosen].clone();
        states.push(shuffle_order::step(p, input.clone())?);
        inputs.push(input);
    }
    Ok((states, inputs))
}

fn record(
    root: &CombatPosition,
    prefix: Vec<ClientInput>,
    states: &[CombatPosition],
    inputs: Vec<ClientInput>,
    all_rows: bool,
    rows: &mut Vec<Row>,
    traces: &mut Vec<Trace>,
) -> Result<(), String> {
    let endpoint = states.last().unwrap();
    verify(root, &prefix, states, &inputs)?;
    let result = terminal_return(endpoint); // A cutoff is censored, never death.
    for (offset, p) in states
        .iter()
        .enumerate()
        .take(if all_rows { states.len() } else { 1 })
    {
        if EngineCombatStepper.terminal(p) == CombatTerminal::Unresolved {
            let state = CombatSearchState::new(p.clone())?;
            rows.push(Row {
                features: if value::validate_position(p).is_ok() {
                    value::features(&state)
                } else {
                    vec![] // The scoped Guardian input is unavailable outside its domain.
                },
                entities: entities::encode_position(p.clone(), 0, Some(0))?,
                trace: traces.len(),
                offset,
            });
        }
    }
    traces.push(Trace {
        prefix,
        inputs,
        endpoint: endpoint.clone(),
        terminal_return: result,
        terminal_kind: terminal_kind(endpoint),
        tactical_control: None,
        search: None,
    });
    Ok(())
}

/// Each anchor starts the same fresh search and executes its tactical control.
/// The executor receives no completed suffix from the source trajectory.
fn record_search(
    root: &CombatPosition,
    prefix: Vec<ClientInput>,
    anchor: &CombatPosition,
    args: &Args,
    config: &SolverConfig,
    rows: &mut Vec<Row>,
    traces: &mut Vec<Trace>,
) -> Result<(), String> {
    let (control_states, control_inputs) = rollout(anchor, args.max_inputs)?;
    verify(root, &prefix, &control_states, &control_inputs)?;
    let control = Control {
        terminal_return: terminal_return(control_states.last().unwrap()),
        endpoint: control_states.last().unwrap().clone(),
        inputs: control_inputs.clone(),
    };
    let started = Instant::now();
    let mut solver = CombatSolver::new(anchor.clone(), config.clone())?;
    let stop = solver.advance(args.solver_work, Duration::from_secs(10))?;
    if stop == SolverStop::TimeBudget {
        return Err(
            "search continuation hit wall limit before its deterministic work limit".into(),
        );
    }
    let best = solver
        .plans()
        .iter()
        .filter(|p| {
            p.inputs.len() <= args.max_inputs && EngineCombatStepper.is_exact_victory(&p.successor)
        })
        .max_by_key(|p| p.outcome.final_hp);
    let adopted = best.filter(|p| {
        control
            .terminal_return
            .is_none_or(|v| p.outcome.final_hp as f32 / 75. > v)
    });
    let (states, inputs) = if let Some(plan) = adopted {
        (
            shuffle_order::replay(anchor, &plan.inputs)?,
            plan.inputs.clone(),
        )
    } else {
        (control_states, control_inputs)
    };
    record(root, prefix, &states, inputs, false, rows, traces)?;
    let trace = traces.last_mut().unwrap();
    trace.tactical_control = Some(control);
    trace.search = Some(SearchReceipt {
        stats: solver.stats().clone(),
        stop,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
        selected_solver_plan: adopted.is_some(),
    });
    Ok(())
}

type Anchor = (Vec<ClientInput>, CombatPosition);

/// Cadence alone can skip a whole decision phase. Include actual choices and
/// the first complete player decision seen in each turn of each source path.
fn source_offsets(states: &[CombatPosition], inputs: usize, cadence: usize) -> Vec<usize> {
    let mut player_turns = BTreeSet::new();
    (0..inputs)
        .filter(|&i| {
            let p = &states[i];
            let pending = !matches!(
                p.engine,
                sts_core::state::core::EngineState::CombatPlayerTurn
            );
            let first_player = !pending && player_turns.insert(p.combat.turn.turn_count);
            i % cadence == 0 || pending || first_player
        })
        .collect()
}

/// Equal opportunity for observed engine phases, then turns within each phase.
/// Prefix hashes order members without reading results or consuming game RNG.
/// Exact positions, including order and RNG, are the only deduplication key.
fn sample_anchors(
    root: &CombatPosition,
    anchors: Vec<Anchor>,
    limit: usize,
) -> Result<Vec<Anchor>, String> {
    let mut unique = vec![];
    for (prefix, p) in anchors {
        if p != *root && !unique.iter().any(|(_, other)| other == &p) {
            unique.push((prefix, p));
        }
    }
    let mut phases: [BTreeMap<u32, Vec<Anchor>>; 2] = Default::default();
    for anchor in unique {
        let phase = usize::from(!matches!(
            anchor.1.engine,
            sts_core::state::core::EngineState::CombatPlayerTurn
        ));
        phases[phase]
            .entry(anchor.1.combat.turn.turn_count)
            .or_default()
            .push(anchor);
    }
    let mut queues: [Vec<VecDeque<Anchor>>; 2] = Default::default();
    for (i, phase) in phases.into_iter().enumerate() {
        for group in phase.into_values() {
            let mut ordered = group
                .into_iter()
                .map(|anchor| {
                    let bytes = serde_json::to_vec(&anchor.0).map_err(|e| e.to_string())?;
                    Ok((Sha256::digest(bytes), anchor))
                })
                .collect::<Result<Vec<_>, String>>()?;
            ordered.sort_by(|a, b| a.0.cmp(&b.0));
            queues[i].push(ordered.into_iter().map(|(_, anchor)| anchor).collect());
        }
    }
    let mut cursors = [0, 0];
    let mut selected = vec![];
    while selected.len() < limit {
        let before = selected.len();
        for phase in 0..2 {
            let groups = &mut queues[phase];
            for _ in 0..groups.len() {
                let i = cursors[phase];
                cursors[phase] = (i + 1) % groups.len();
                if let Some(anchor) = groups[i].pop_front() {
                    selected.push(anchor);
                    break;
                }
            }
            if selected.len() == limit {
                break;
            }
        }
        if selected.len() == before {
            break;
        }
    }
    Ok(selected)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.max_inputs == 0
        || args.branch_every == 0
        || args.max_anchors == 0
        || args.output.exists()
    {
        return Err("positive limits and a fresh output directory are required".into());
    }
    if args.resource_model.is_some() && args.solver_work == 0 {
        return Err("a resource model requires the search continuation executor".into());
    }
    let bytes = fs::read(&args.input)?;
    let cases: Vec<Case> = serde_json::from_slice(&bytes)?;
    // Read exactly once: every anchor in this collection has the same teacher.
    let model_bytes = args.resource_model.as_ref().map(fs::read).transpose()?;
    let config = SolverConfig {
        max_potion_uses: Some(0),
        resource_model: model_bytes
            .as_ref()
            .map(|bytes| serde_json::from_slice(bytes))
            .transpose()?,
        ..Default::default()
    };
    let mut config_receipt = serde_json::to_value(&config)?;
    if let Some(bytes) = &model_bytes {
        // Keep an immutable identity, not a full copy of weights in every case.
        config_receipt["resource_model"] = serde_json::json!({
            "sha256":format!("{:x}", Sha256::digest(bytes)),
            "objective":config.resource_model.as_ref().unwrap().objective,
        });
    }
    let executable_sha256 = format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?));
    fs::create_dir_all(&args.output)?;
    for (index, case) in cases.into_iter().enumerate() {
        if case.label.trim().is_empty() || case.group.trim().is_empty() {
            return Err("case label and split group are required".into());
        }
        let started = Instant::now();
        let root = case.build_root()?;
        entities::encode_position(root.clone(), 0, Some(0))?;
        let scalar_input = value::validate_position(&root).is_ok();
        let (states, inputs) = rollout(&root, args.max_inputs)?;
        let names = if scalar_input {
            value::feature_names(&CombatSearchState::new(root.clone())?)
        } else {
            vec![]
        };
        let (mut rows, mut traces) = (vec![], vec![]);
        if args.solver_work > 0 {
            record_search(&root, vec![], &root, &args, &config, &mut rows, &mut traces)?;
            let planned_inputs = traces[0].inputs.clone();
            let planned_states = shuffle_order::replay(&root, &planned_inputs)?;
            // Both real trajectories supply candidates, including EndTurn and
            // every concrete single-step option. The selected root plan biases
            // coverage; its result never becomes a suffix target automatically.
            let mut anchors = vec![];
            for (states, inputs) in [(&states, &inputs), (&planned_states, &planned_inputs)]
                .into_iter()
                .filter(|_| args.max_anchors > 1)
            {
                for offset in source_offsets(states, inputs.len(), args.branch_every) {
                    anchors.push((inputs[..offset].to_vec(), states[offset].clone()));
                    for input in choices(&states[offset])?.0 {
                        let next = shuffle_order::step(&states[offset], input.clone())?;
                        if EngineCombatStepper.terminal(&next) == CombatTerminal::Unresolved {
                            let mut prefix = inputs[..offset].to_vec();
                            prefix.push(input);
                            anchors.push((prefix, next));
                        }
                    }
                }
            }
            // Every label belongs to a separate executor invocation.
            for (prefix, anchor) in sample_anchors(&root, anchors, args.max_anchors - 1)? {
                record_search(
                    &root,
                    prefix,
                    &anchor,
                    &args,
                    &config,
                    &mut rows,
                    &mut traces,
                )?;
            }
        } else {
            record(
                &root,
                vec![],
                &states,
                inputs.clone(),
                true,
                &mut rows,
                &mut traces,
            )?;
            for offset in (0..inputs.len()).step_by(args.branch_every) {
                let (actions, preferred) = choices(&states[offset])?;
                for (i, input) in actions.into_iter().enumerate() {
                    if i == preferred {
                        continue;
                    }
                    let next = shuffle_order::step(&states[offset], input.clone())?;
                    let mut prefix = inputs[..offset].to_vec();
                    prefix.push(input);
                    let (suffix_states, suffix_inputs) = rollout(&next, args.max_inputs)?;
                    record(
                        &root,
                        prefix,
                        &suffix_states,
                        suffix_inputs,
                        false,
                        &mut rows,
                        &mut traces,
                    )?;
                }
            }
        }
        let wins = traces
            .iter()
            .filter(|t| t.terminal_return.is_some_and(|v| v >= 0.))
            .count();
        let losses = traces
            .iter()
            .filter(|t| t.terminal_return == Some(-1.))
            .count();
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.;
        println!(
            "{} rows={} traces={} wins={} losses={} censored={} ms={elapsed_ms:.1}",
            case.label,
            rows.len(),
            traces.len(),
            wins,
            losses,
            traces.len() - wins - losses
        );
        let mut continuation = serde_json::json!({"policy":"RetainedCombatTacticalPolicyV1", "selection":"argmax_first_tie",
            "executable_sha256":executable_sha256, "max_inputs":args.max_inputs,
            "potion_contract":"no_active_use_or_discard_automatic_effects_remain"});
        if args.solver_work > 0 {
            continuation["policy"] = "CombatSolverPlusTacticalControlV2".into();
            continuation["solver_work"] = args.solver_work.into();
            continuation["solver_config"] = config_receipt.clone();
            continuation["selection"] = "best_completed_hp_ties_tactical_then_search".into();
        }
        let document = serde_json::json!({"schema":1, "feature_schema":scalar_input.then_some(value::SCHEMA), "entity_schema":entities::SCHEMA,
            "entity_objective":ENTITY_OBJECTIVE,
            "objective":if !scalar_input {ENTITY_OBJECTIVE} else if args.solver_work > 0 { value::SEARCH_CONTINUATION_OBJECTIVE } else {value::CONTINUATION_OBJECTIVE}, "information":"full_state_oracle",
            "continuation":continuation,
            "input_sha256":format!("{:x}", Sha256::digest(&bytes)), "branch_every":args.branch_every, "max_anchors":args.max_anchors,
            "sampling":if args.solver_work > 0 {Some("phase_turn_round_robin_prefix_sha256_v1")} else {None},
            "case":case, "root":root, "feature_names":names, "rows":rows, "traces":traces,
            "elapsed_ms":elapsed_ms});
        fs::write(
            args.output.join(format!("case-{index:03}.json")),
            serde_json::to_vec(&document)?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_choice_and_turn_boundaries_are_not_lost_between_cadence_steps() {
        use sts_core::{
            content::{cards::CardId, monsters::factory::EncounterId},
            state::map::node::RoomType,
        };
        let root = ConstructedCombat {
            seed: 3101,
            encounter: EncounterId::TheGuardian,
            act: 1,
            room: RoomType::MonsterRoomBoss,
            hp: 75,
            deck: vec![
                (CardId::TrueGrit, 1),
                (CardId::Strike, 0),
                (CardId::Defend, 0),
                (CardId::Headbutt, 0),
                (CardId::ShrugItOff, 0),
            ],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        let card_index = root
            .combat
            .zones
            .hand
            .iter()
            .position(|c| c.id == CardId::TrueGrit)
            .unwrap();
        let mut inputs = vec![ClientInput::PlayCard {
            card_index,
            target: None,
        }];
        let pending = shuffle_order::step(&root, inputs[0].clone()).unwrap();
        assert!(!matches!(
            pending.engine,
            sts_core::state::core::EngineState::CombatPlayerTurn
        ));
        let (choices, preferred) = choices(&pending).unwrap();
        inputs.push(choices[preferred].clone());
        inputs.extend([ClientInput::EndTurn, ClientInput::EndTurn]);
        let states = shuffle_order::replay(&root, &inputs).unwrap();
        assert_eq!(source_offsets(&states, inputs.len(), 100000), vec![0, 1, 3]);
        let mut anchors = (0..states.len())
            .map(|i| (inputs[..i].to_vec(), states[i].clone()))
            .collect::<Vec<_>>();
        anchors.push((inputs[..1].to_vec(), pending.clone()));
        let selected = sample_anchors(&root, anchors.clone(), 2).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(
            selected
                .iter()
                .filter(|(_, p)| !matches!(
                    p.engine,
                    sts_core::state::core::EngineState::CombatPlayerTurn
                ))
                .count(),
            1
        );
        assert_eq!(selected, sample_anchors(&root, anchors, 2).unwrap());
        for (prefix, p) in selected {
            assert_eq!(
                shuffle_order::replay(&root, &prefix).unwrap().last(),
                Some(&p)
            );
        }
    }

    #[test]
    fn source_prefix_preserves_construction_and_replays_exactly() {
        use sts_core::{
            content::{cards::CardId, monsters::factory::EncounterId},
            state::map::node::RoomType,
        };
        let spec = ConstructedCombat {
            seed: 3101,
            encounter: EncounterId::TheGuardian,
            act: 1,
            room: RoomType::MonsterRoomBoss,
            hp: 75,
            deck: vec![(CardId::Strike, 0), (CardId::Defend, 0), (CardId::Bash, 1)],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        };
        let mut case: Case = serde_json::from_value(serde_json::json!({
            "label":"prefix", "group":"seed-3101", "spec":spec,
        }))
        .unwrap();
        let initial = case.spec.build().unwrap();
        assert_eq!(case.build_root().unwrap(), initial);
        let input = choices(&initial)
            .unwrap()
            .0
            .into_iter()
            .find(|i| matches!(i, ClientInput::PlayCard { .. }))
            .unwrap();
        case.prefix = vec![input.clone()];
        assert_eq!(
            case.build_root().unwrap(),
            shuffle_order::step(&initial, input).unwrap()
        );
        assert_eq!(case.spec.build().unwrap(), initial);
        case.prefix = vec![ClientInput::PlayCard {
            card_index: 99,
            target: None,
        }];
        assert!(case.build_root().is_err());
    }
    #[test]
    fn argmax_is_stable_and_rejects_malformed_weights() {
        assert_eq!(argmax(&[1., 3., 3.]).unwrap(), 1);
        assert!(argmax(&[f64::NAN]).is_err());
        assert!(argmax(&[]).is_err());
    }

    #[test]
    fn failed_search_executes_to_real_loss_without_using_or_discarding_potions() {
        use sts_core::{
            content::{cards::CardId, monsters::factory::EncounterId, potions::PotionId},
            state::map::node::RoomType,
        };
        let root = ConstructedCombat {
            seed: 4300010,
            encounter: EncounterId::JawWorm,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 1,
            deck: vec![(CardId::Defend, 0)],
            relics: vec![],
            potions: vec![PotionId::FirePotion],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        assert!(EngineCombatStepper
            .legal_action_surface(&root)
            .atomic_actions
            .iter()
            .any(|i| matches!(i, ClientInput::UsePotion { .. })));
        let args = Args {
            input: PathBuf::new(),
            output: PathBuf::new(),
            max_inputs: 256,
            branch_every: 8,
            max_anchors: 1,
            solver_work: 1,
            resource_model: None,
        };
        let config = SolverConfig {
            max_potion_uses: Some(0),
            ..Default::default()
        };
        let (mut rows, mut traces) = (vec![], vec![]);
        for _ in 0..2 {
            record_search(&root, vec![], &root, &args, &config, &mut rows, &mut traces).unwrap();
        }
        assert_eq!(traces[0].terminal_kind, "loss");
        assert_eq!(traces[0].terminal_return, Some(-1.));
        assert!(!traces[0].search.as_ref().unwrap().selected_solver_plan);
        assert_eq!(traces[0].inputs, traces[1].inputs);
        assert_eq!(traces[0].endpoint, traces[1].endpoint);
        assert!(traces[0].inputs.iter().all(|i| !matches!(
            i,
            ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
        )));
        assert!(traces[0].endpoint.combat.entities.potions[0].is_some());
        assert!(rows[0].features.is_empty());
        assert_eq!(rows[0].entities, rows[1].entities);
    }

    #[test]
    fn deviation_labels_only_its_successor_and_cutoff_is_censored() {
        use sts_core::{
            content::{cards::CardId, monsters::factory::EncounterId},
            state::map::node::RoomType,
        };
        let root = ConstructedCombat {
            seed: 3101,
            encounter: EncounterId::TheGuardian,
            act: 1,
            room: RoomType::MonsterRoomBoss,
            hp: 75,
            deck: vec![(CardId::Strike, 0), (CardId::Defend, 0), (CardId::Bash, 1)],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        let (actions, _) = choices(&root).unwrap();
        let input = actions
            .into_iter()
            .find(|a| matches!(a, ClientInput::PlayCard { .. }))
            .unwrap();
        let next = shuffle_order::step(&root, input.clone()).unwrap();
        let (states, inputs) = rollout(&next, 1).unwrap();
        let repeated = rollout(&next, 1).unwrap();
        assert_eq!(states, repeated.0);
        assert_eq!(inputs, repeated.1);
        let (mut rows, mut traces) = (vec![], vec![]);
        record(
            &root,
            vec![input],
            &states,
            inputs,
            false,
            &mut rows,
            &mut traces,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].features,
            value::features(&CombatSearchState::new(next).unwrap())
        );
        assert_ne!(
            rows[0].features,
            value::features(&CombatSearchState::new(root).unwrap())
        );
        assert_eq!(traces[0].terminal_return, None);
    }

    #[test]
    fn searched_target_is_only_for_its_own_anchor_and_keeps_the_control() {
        use sts_core::{
            content::{cards::CardId, monsters::factory::EncounterId},
            state::map::node::RoomType,
        };
        let root = ConstructedCombat {
            seed: 3101,
            encounter: EncounterId::TheGuardian,
            act: 1,
            room: RoomType::MonsterRoomBoss,
            hp: 75,
            deck: vec![
                (CardId::Strike, 0),
                (CardId::Defend, 0),
                (CardId::Bash, 1),
                (CardId::Carnage, 1),
                (CardId::ShrugItOff, 1),
                (CardId::Inflame, 1),
            ],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        let args = Args {
            input: PathBuf::new(),
            output: PathBuf::new(),
            max_inputs: 256,
            branch_every: 8,
            max_anchors: 32,
            solver_work: 5000,
            resource_model: None,
        };
        let (mut rows, mut traces) = (vec![], vec![]);
        let mut config = SolverConfig {
            max_potion_uses: Some(0),
            ..Default::default()
        };
        record_search(&root, vec![], &root, &args, &config, &mut rows, &mut traces).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].offset, 0);
        assert_eq!(
            rows[0].features,
            value::features(&CombatSearchState::new(root.clone()).unwrap())
        );
        let trace = &traces[0];
        let control = trace.tactical_control.as_ref().unwrap();
        assert!(trace.terminal_return.unwrap() >= control.terminal_return.unwrap());
        assert_eq!(
            shuffle_order::replay(&root, &control.inputs)
                .unwrap()
                .last(),
            Some(&control.endpoint)
        );
        assert_eq!(trace.search.as_ref().unwrap().stats.resource_evaluations, 0);
        let names = value::feature_names(&CombatSearchState::new(root.clone()).unwrap());
        config.resource_model = Some(value::ValueModel {
            schema: value::SCHEMA,
            objective: value::SEARCH_CONTINUATION_OBJECTIVE.into(),
            hidden_weights: vec![vec![0.; names.len()]],
            feature_names: names,
            hidden_bias: vec![0.],
            output_weights: vec![0.],
            output_bias: 0.,
            hand_encoder: None,
            resource_curve: None,
        });
        let (mut guided_rows, mut guided_traces) = (vec![], vec![]);
        record_search(
            &root,
            vec![],
            &root,
            &args,
            &config,
            &mut guided_rows,
            &mut guided_traces,
        )
        .unwrap();
        assert!(
            guided_traces[0]
                .search
                .as_ref()
                .unwrap()
                .stats
                .resource_evaluations
                > 0
        );
        assert_eq!(guided_rows.len(), 1);
        let guided_control = guided_traces[0].tactical_control.as_ref().unwrap();
        assert_eq!(guided_control.inputs, control.inputs);
        assert_eq!(guided_control.endpoint, control.endpoint);
        assert_eq!(guided_control.terminal_return, control.terminal_return);
        let weak = Args {
            solver_work: 1,
            ..args
        };
        let (mut weak_rows, mut weak_traces) = (vec![], vec![]);
        record_search(
            &root,
            vec![],
            &root,
            &weak,
            &config,
            &mut weak_rows,
            &mut weak_traces,
        )
        .unwrap();
        let weak_trace = &weak_traces[0];
        assert!(!weak_trace.search.as_ref().unwrap().selected_solver_plan);
        assert_eq!(weak_trace.inputs, control.inputs);
        assert_eq!(weak_trace.endpoint, control.endpoint);
        assert_eq!(weak_trace.terminal_return, control.terminal_return);
        assert_ne!(weak_trace.terminal_kind, "cutoff");
        assert_eq!(
            weak_rows[0].entities,
            entities::encode_position(root, 0, Some(0)).unwrap()
        );
    }
}
