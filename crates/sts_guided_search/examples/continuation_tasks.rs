//! Offline preparation and measurement of past-only combat continuation tasks.
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use sts_core::{
    content::identity::gameplay_display_label_v1 as name,
    sim::combat::{CombatPosition, CombatStepLimits},
    state::core::{ClientInput, EngineState},
};
use sts_guided_search::{
    continuation::{
        validate_reference_trajectory, ContinuationExperiment, ContinuationStop,
        ContinuationTaskSpec, ResourceRequirement, ValidatedContinuationTask,
    },
    path_search::{
        linear::{FrozenLinearPrior, LinearWeights},
        GroupedNeutralPrior, PathBudget, PathOrder, PathPrior, PathSearch, PathStop,
    },
    solver::{CombatSolver, SolverConfig},
};

#[path = "continuation_tasks/routes.rs"]
mod routes;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Reconstruct explicitly selected complete witnesses as current path decisions.
    Routes(routes::Args),
    /// Fixed-goal path search from full roots, with a frozen local or neutral prior.
    Path(PathArgs),
    /// Search explicit root tasks and sample actual retained prefixes; no references are read.
    Sample {
        #[arg(long)]
        roots: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 2500)]
        work: usize,
        #[arg(long, default_value_t = 12.0)]
        seconds: f64,
        #[arg(long, default_value_t = 32)]
        width: usize,
        #[arg(long, default_value_t = 8)]
        count: usize,
    },
    /// Validate explicitly named combat_solver artifacts and export separate task/evaluation files.
    Prepare {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Read only tasks.json and task files; the reference answers are not loaded.
    Run {
        #[arg(long)]
        tasks: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_delimiter = ',', default_value = "2000,10000,50000")]
        work: Vec<usize>,
        /// Total search wall allowance for each task, shared across milestones.
        #[arg(long, default_value_t = 30.0)]
        seconds: f64,
        #[arg(long, default_value_t = 128)]
        width: usize,
    },
}

#[derive(clap::Args)]
struct PathArgs {
    /// Frozen exploration order. Relative prior permits continuous preferred paths.
    #[arg(long, value_enum, default_value = "path-probability")]
    order: PathOrderArg,
    /// Frozen local linear weights. Omit for the untrained grouped prior.
    #[arg(long)]
    model: Option<PathBuf>,
    #[arg(long)]
    roots: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, value_delimiter = ',', default_value = "500,2500,10000")]
    work: Vec<usize>,
    #[arg(long, default_value_t = 10.0)]
    seconds: f64,
    #[arg(long, default_value_t = 2000)]
    max_nodes: usize,
    #[arg(long, default_value_t = 12000)]
    max_edges: usize,
    #[arg(long, default_value_t = 128)]
    max_depth: usize,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PathOrderArg {
    PathProbability,
    RelativePrior,
}
impl From<PathOrderArg> for PathOrder {
    fn from(value: PathOrderArg) -> Self {
        match value {
            PathOrderArg::PathProbability => Self::PathProbability,
            PathOrderArg::RelativePrior => Self::RelativePrior,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceSource {
    artifact: PathBuf,
    plan_index: usize,
    /// A declared family (normally run seed), shared across all its fights and cuts.
    group: String,
    /// Human-readable qualification such as natural root or constructed boss diagnostic.
    qualification: String,
    /// Exact zero-based combat turn_count values, not distances to the answer.
    turn_counts: Vec<u32>,
    requirement: ResourceRequirement,
}

fn limits() -> CombatStepLimits {
    CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn new_output(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(path)?; // Never overwrite a partial or earlier experiment.
    Ok(())
}

fn producer() -> Result<Value, Box<dyn std::error::Error>> {
    let executable_sha256 = format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?));
    // Build-time source bytes for this adapter and the evidence producer. The
    // executable digest also identifies the compiled simulator/dependencies.
    let files = [
        include_str!("continuation_tasks.rs"),
        include_str!("../src/continuation.rs"),
        include_str!("../src/solver.rs"),
        include_str!("../src/solver/prefixes.rs"),
        include_str!("../src/solver/retention.rs"),
        include_str!("../src/combat.rs"),
        include_str!("../src/objective.rs"),
        include_str!("../src/selection.rs"),
        include_str!("../src/path_search.rs"),
        include_str!("../src/path_search/prior.rs"),
        include_str!("../src/path_search/linear.rs"),
        include_str!("../src/path_search/linear/features.rs"),
        include_str!("../src/entities.rs"),
        include_str!("../src/path_search/admission.rs"),
        include_str!("../src/path_search/routes.rs"),
        include_str!("../src/observation.rs"),
        include_str!("../src/observation/monster.rs"),
        include_str!("continuation_tasks/routes.rs"),
        include_str!("../../../src/eval/combat_witness_materialization.rs"),
    ];
    let mut hash = Sha256::new();
    for file in files {
        hash.update((file.len() as u64).to_le_bytes());
        hash.update(file);
    }
    Ok(
        json!({"executor": "sts_guided_search::CombatSolver", "information": "full_state_oracle",
        "executable_sha256": executable_sha256, "adapter_executor_source_sha256": format!("{:x}", hash.finalize()),
        "source_scope": "adapter, continuation, solver, prefix sampling, retention, combat adapter, objective, selection, path search, prior, linear encoder/model, entities, admission, route reconstruction, observation, route exporter, outcome materialization"}),
    )
}

fn prepare(manifest: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let sources: Vec<ReferenceSource> = read_json(manifest)?;
    if sources.is_empty() {
        return Err("reference manifest must not be empty".into());
    }
    new_output(output)?;
    let mut task_files = vec![];
    let mut evaluations = vec![];
    for source in sources {
        if source.group.trim().is_empty()
            || source.qualification.trim().is_empty()
            || source.turn_counts.is_empty()
            || source.turn_counts.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("declare a group, source qualification and increasing turn counts".into());
        }
        let path = manifest
            .parent()
            .unwrap_or(Path::new("."))
            .join(&source.artifact);
        let bytes = fs::read(&path)?;
        let artifact: Value = serde_json::from_slice(&bytes)?;
        let original_head = artifact["workspace_git_head"]
            .as_str()
            .ok_or("reference missing producer HEAD")?;
        let original_implementation = artifact["implementation_sha256"]
            .as_str()
            .ok_or("reference missing implementation digest")?;
        let original_producer =
            format!("CombatSolver {original_head} implementation {original_implementation}");
        let root: CombatPosition = serde_json::from_value(artifact["root"].clone())?;
        let plan = artifact["plans"]
            .as_array()
            .and_then(|plans| plans.get(source.plan_index))
            .ok_or("reference plan is absent")?;
        let inputs: Vec<ClientInput> = serde_json::from_value(plan["inputs"].clone())?;
        let successor: CombatPosition = serde_json::from_value(plan["successor"].clone())?;
        let started = Instant::now();
        let (states, engine_steps) =
            validate_reference_trajectory(&root, &inputs, &successor, limits())?;
        let validation_micros = started.elapsed().as_micros();
        let mut cuts = vec![];
        for &turn in &source.turn_counts {
            let prefix_len = states
                .iter()
                .position(|s| {
                    s.combat.turn.turn_count == turn && s.engine == EngineState::CombatPlayerTurn
                })
                .ok_or_else(|| {
                    format!(
                        "reference has no unresolved player-turn boundary for turn_count {turn}"
                    )
                })?;
            let task = ValidatedContinuationTask::new(
                ContinuationTaskSpec {
                    source_producer: original_producer.clone(),
                    group: source.group.clone(),
                    origin: root.clone(),
                    prefix: inputs[..prefix_len].to_vec(),
                    requirement: source.requirement.clone(),
                },
                limits(),
            )?;
            let file = format!("task-{:03}.json", task_files.len());
            write_json(&output.join(&file), task.spec())?;
            cuts.push(
                json!({"task": file, "task_id": task.id(), "prefix_length": prefix_len,
                "anchor_turn_count": turn, "reference_suffix": &inputs[prefix_len..]}),
            );
            task_files.push(file);
        }
        evaluations.push(json!({"source":source,"source_sha256":format!("{:x}", Sha256::digest(&bytes)),
            "source_case":artifact["case"], "source_config":artifact["config"],
            "source_producer":original_producer, "source_workspace_status":artifact["workspace_status"],
            "reference_successor":successor, "reference_validation_work":inputs.len(),
            "reference_validation_engine_steps":engine_steps,"reference_validation_micros":validation_micros,"cuts":cuts}));
    }
    write_json(
        &output.join("evaluation.json"),
        &json!({"producer":producer()?,"references":evaluations}),
    )?;
    write_json(&output.join("tasks.json"), &task_files)?;
    println!(
        "Prepared {} continuation tasks; references saved separately.",
        task_files.len()
    );
    Ok(())
}

fn run(
    tasks: &Path,
    output: &Path,
    milestones: &[usize],
    seconds: f64,
    width: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if milestones.is_empty()
        || milestones[0] == 0
        || milestones.windows(2).any(|w| w[0] >= w[1])
        || !seconds.is_finite()
        || seconds <= 0.0
        || !(4..=4096).contains(&width)
    {
        return Err(
            "require increasing positive work, positive finite seconds, width in 4..=4096".into(),
        );
    }
    let allowance = Duration::try_from_secs_f64(seconds)?;
    let files: Vec<PathBuf> = read_json(tasks)?;
    if files.is_empty() {
        return Err("task index must not be empty".into());
    }
    new_output(output)?;
    let provenance = producer()?;
    write_json(&output.join("producer.json"), &provenance)?;
    let mut summary = vec![];
    for (index, file) in files.iter().enumerate() {
        let path = tasks.parent().unwrap_or(Path::new(".")).join(file);
        let spec: ContinuationTaskSpec = read_json(&path)?;
        let config = SolverConfig {
            max_width: width,
            ..Default::default()
        };
        let setup = Instant::now();
        let task = ValidatedContinuationTask::new(spec, limits())?;
        let mut experiment = ContinuationExperiment::new(task, config)?;
        let setup_micros = setup.elapsed().as_micros();
        let start = Instant::now();
        for &milestone in milestones {
            let receipt = experiment.advance(
                milestone.saturating_sub(experiment.work()),
                allowance.saturating_sub(start.elapsed()),
            )?;
            if receipt.stop != ContinuationStop::WorkBudget {
                break;
            }
        }
        let receipt = experiment.receipts().last().unwrap();
        let report = json!({"task_id":experiment.task().id(), "task_input":experiment.task().spec(),
            "executor_config":experiment.config(), "producer":provenance,
            "requested_cumulative_work":milestones, "search_wall_allowance_seconds":seconds,
            "setup_micros":setup_micros, "prefix_validation_work":experiment.task().validation_work(),
            "prefix_validation_engine_steps":experiment.task().validation_engine_steps(),
            "receipts":experiment.receipts(), "witnesses":experiment.witnesses()});
        let result_file = format!("result-{index:03}.json");
        write_json(&output.join(&result_file), &report)?;
        let outcomes = receipt
            .target_frontier
            .iter()
            .map(|o| {
                let spent = experiment
                    .task()
                    .spec()
                    .origin
                    .combat
                    .entities
                    .potions
                    .iter()
                    .take(u64::BITS as usize)
                    .enumerate()
                    .filter(|(slot, _)| o.starting_potion_expenditure_slots & (1_u64 << slot) != 0)
                    .filter_map(|(slot, p)| p.as_ref().map(|p| format!("P{slot} {}", name(p.id))))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "HP {}, spent {}",
                    o.final_hp,
                    if spent.is_empty() { "none" } else { &spent }
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        println!(
            "Task {index} | Turn {} | work {} | {}",
            experiment.task().anchor().combat.turn.turn_count + 1,
            receipt.cumulative_work,
            if outcomes.is_empty() {
                "target not observed"
            } else {
                &outcomes
            }
        );
        summary.push(json!({"result":result_file, "task_id":experiment.task().id(), "group":experiment.task().spec().group,
            "prefix_length":experiment.task().spec().prefix.len(), "anchor_turn_count":experiment.task().anchor().combat.turn.turn_count,
            "last_receipt":receipt}));
    }
    write_json(&output.join("summary.json"), &summary)?;
    Ok(())
}

fn sample(
    roots: &Path,
    output: &Path,
    work: usize,
    seconds: f64,
    width: usize,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if work == 0
        || !seconds.is_finite()
        || seconds <= 0.0
        || !(4..=4096).contains(&width)
        || !(1..=64).contains(&count)
    {
        return Err("require positive work/seconds, width in 4..=4096 and count in 1..=64".into());
    }
    let allowance = Duration::try_from_secs_f64(seconds)?;
    let files: Vec<PathBuf> = read_json(roots)?;
    if files.is_empty() {
        return Err("root index must not be empty".into());
    }
    new_output(output)?;
    let provenance = producer()?;
    let mut generated = Vec::new();
    let mut records = Vec::new();
    for file in files {
        let input_path = roots.parent().unwrap_or(Path::new(".")).join(file);
        let input: ContinuationTaskSpec = read_json(&input_path)?;
        if !input.prefix.is_empty() {
            return Err("sample requires an original root with an empty prefix".into());
        }
        let setup = Instant::now();
        let root = ValidatedContinuationTask::new(input, limits())?;
        let config = SolverConfig {
            max_width: width,
            ..Default::default()
        };
        let mut solver = CombatSolver::new(root.anchor().clone(), config.clone())?;
        let setup_micros = setup.elapsed().as_micros();
        let start = Instant::now();
        let stop = solver.advance(work, allowance)?;
        let search_micros = start.elapsed().as_micros();
        let sampling = Instant::now();
        let candidates = solver.sample_prefixes(count);
        let sampling_micros = sampling.elapsed().as_micros();
        let mut exports = Vec::new();
        for candidate in candidates {
            let validate = Instant::now();
            let mut spec = root.spec().clone();
            spec.source_producer = format!(
                "CombatSolver retained prefix sampler {}",
                provenance["executable_sha256"].as_str().unwrap()
            );
            spec.prefix = candidate.inputs;
            let task = ValidatedContinuationTask::new(spec, limits())?;
            if task.anchor() != &candidate.successor {
                return Err("sampled prefix differs from its replay".into());
            }
            let validation_micros = validate.elapsed().as_micros();
            let name = format!("task-{:03}.json", generated.len());
            write_json(&output.join(&name), task.spec())?;
            exports.push(json!({"task":name,"task_id":task.id(),"width":candidate.width,"card_only":candidate.card_only,
                "pool":candidate.pool,"validation_work":task.validation_work(),"validation_engine_steps":task.validation_engine_steps(),
                "validation_micros":validation_micros}));
            generated.push(name);
        }
        println!(
            "Sampled {} prefixes | work {}",
            exports.len(),
            solver.stats().work()
        );
        records.push(json!({"root_task":root.spec(),"root_task_id":root.id(),"executor_config":config,
            "requested_work":work,"seconds":seconds,"requested_count":count,"setup_micros":setup_micros,
            "search_micros":search_micros,"sampling_micros":sampling_micros,"stop":stop,
            "stats":solver.stats(),"progress":solver.progress(),"existing_plans":solver.plans(),"exports":exports}));
    }
    write_json(
        &output.join("sampling.json"),
        &json!({"producer":provenance,"roots":records}),
    )?;
    write_json(&output.join("tasks.json"), &generated)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::Routes(args) => routes::run(args),
        Command::Path(args) => run_paths(args),
        Command::Sample {
            roots,
            output,
            work,
            seconds,
            width,
            count,
        } => sample(&roots, &output, work, seconds, width, count),
        Command::Prepare { manifest, output } => prepare(&manifest, &output),
        Command::Run {
            tasks,
            output,
            work,
            seconds,
            width,
        } => run(&tasks, &output, &work, seconds, width),
    }
}

fn run_paths(args: PathArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.work.is_empty()
        || args.work[0] == 0
        || args.work.windows(2).any(|w| w[0] >= w[1])
        || args.max_nodes == 0
        || args.max_edges == 0
        || args.max_depth == 0
        || !args.seconds.is_finite()
        || args.seconds <= 0.0
    {
        return Err(
            "require increasing positive work and positive time/storage/depth limits".into(),
        );
    }
    let allowance = Duration::try_from_secs_f64(args.seconds)?;
    let weights: Option<LinearWeights> = args.model.as_ref().map(|p| read_json(p)).transpose()?;
    let files: Vec<PathBuf> = read_json(&args.roots)?;
    if files.is_empty() {
        return Err("root index must not be empty".into());
    }
    new_output(&args.output)?;
    if let Some(weights) = &weights {
        write_json(&args.output.join("prior-weights.json"), weights)?;
    }
    let mut provenance = producer()?;
    provenance["executor"] = json!("sts_guided_search::path_search::PathSearch");
    write_json(&args.output.join("producer.json"), &provenance)?;
    let mut summary = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let file = args.roots.parent().unwrap_or(Path::new(".")).join(file);
        let input: ContinuationTaskSpec = read_json(&file)?;
        if !input.prefix.is_empty() {
            return Err("path search requires an empty-prefix full root".into());
        }
        let setup = Instant::now();
        let task = ValidatedContinuationTask::new(input, limits())?;
        let prior: Box<dyn PathPrior> = match &weights {
            Some(weights) => Box::new(FrozenLinearPrior::new(weights.clone())?),
            None => Box::new(GroupedNeutralPrior),
        };
        let mut search = PathSearch::with_order(
            task.anchor().clone(),
            task.spec().requirement.clone(),
            prior,
            args.order.into(),
        )?;
        let setup_micros = setup.elapsed().as_micros();
        let start = Instant::now();
        let deadline = start
            .checked_add(allowance)
            .ok_or("path search deadline overflow")?;
        let mut receipts = Vec::new();
        for &work in &args.work {
            let before = search.stats().work();
            let stop = search.advance(PathBudget {
                work: work.saturating_sub(before),
                max_nodes: args.max_nodes,
                max_pending_edges: args.max_edges,
                max_depth: args.max_depth,
                max_engine_steps_per_input: limits().max_engine_steps,
                deadline: Some(deadline),
            })?;
            receipts.push(
                json!({"requested_cumulative_work":work,"stats":search.stats(),"stop":stop,
                "retained_nodes":search.retained_nodes(),"pending_edges":search.pending_edges(),
                "depth_deferred_edges":search.depth_deferred_edges(),
                "search_micros":start.elapsed().as_micros(),
                "outcomes":search.plans().iter().map(|p| &p.outcome).collect::<Vec<_>>()}),
            );
            if stop != PathStop::WorkBudget {
                break;
            }
        }
        let name = format!("result-{index:03}.json");
        write_json(
            &args.output.join(&name),
            &json!({"producer":provenance,"task_input":task.spec(),
            "prior_revision":search.prior_revision(),"setup_micros":setup_micros,
            "order":search.order(),
            "limits":{"nodes":args.max_nodes,"pending_edges":args.max_edges,"depth":args.max_depth,
                "engine_steps_per_input":limits().max_engine_steps,"seconds":args.seconds},
            "work_definition":"prepared states + transition attempts + replay attempts",
            "receipts":receipts,"plans":search.plans()}),
        )?;
        println!(
            "{}: {} qualifying outcomes | work {} | {}",
            task.spec().group,
            search.plans().len(),
            search.stats().work(),
            receipts.last().unwrap()["stop"].as_str().unwrap()
        );
        summary.push(
            json!({"file":name,"group":task.spec().group,"stats":search.stats(),
            "qualifying_outcomes":search.plans().len()}),
        );
    }
    write_json(&args.output.join("summary.json"), &summary)?;
    Ok(())
}
