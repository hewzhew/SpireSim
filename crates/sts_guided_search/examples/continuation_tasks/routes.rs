//! Explicit artifact reader for whole-route reconstruction; never resumes search.
use super::*;
use sts_guided_search::path_search::routes::{
    reconstruct_observed_victory, reconstruct_route, resource_preference, CompleteRoute,
    RouteLimits, ROUTE_SCHEMA,
};
use sts_guided_search::path_search::{linear, PathContext};

#[derive(clap::Args)]
pub(super) struct Args {
    /// Export numerical inputs using the same encoder as the runtime model.
    #[arg(long)]
    features: bool,
    /// Explicit pairs [[[source_index,witness_index],[source_index,witness_index]],...].
    /// Enables observed-victory evidence, including victories below the HP goal.
    #[arg(long, requires = "features")]
    comparisons: Option<PathBuf>,
    /// Score reconstructed routes with frozen weights, for cross-language checks.
    #[arg(long)]
    model: Option<PathBuf>,
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 4096)]
    max_decisions: usize,
    #[arg(long, default_value_t = 10.0)]
    seconds_per_route: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    artifact: PathBuf,
    witness_indices: Vec<usize>,
    qualification: String,
    /// Explicit new resource query over this unchanged root and route evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requirement: Option<ResourceRequirement>,
}

#[derive(Deserialize)]
struct Artifact {
    producer: Value,
    task_input: ContinuationTaskSpec,
    #[serde(alias = "plans")]
    witnesses: Vec<Witness>,
}

#[derive(Deserialize)]
struct Witness {
    inputs: Vec<ClientInput>,
    successor: CombatPosition,
    outcome: Value,
}

pub(super) fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.max_decisions == 0
        || !args.seconds_per_route.is_finite()
        || args.seconds_per_route <= 0.0
    {
        return Err("positive reconstruction limits required".into());
    }
    let allowance = Duration::try_from_secs_f64(args.seconds_per_route)?;
    let model = args
        .model
        .as_ref()
        .map(|p| -> Result<_, Box<dyn std::error::Error>> {
            Ok(FrozenLinearPrior::new(read_json(p)?)?)
        })
        .transpose()?;
    let manifest_bytes = fs::read(&args.manifest)?;
    let sources: Vec<Source> = serde_json::from_slice(&manifest_bytes)?;
    let comparison_bytes = args.comparisons.as_ref().map(fs::read).transpose()?;
    let comparisons: Vec<[[usize; 2]; 2]> = comparison_bytes
        .as_ref()
        .map(|b| serde_json::from_slice(b))
        .transpose()?
        .unwrap_or_default();
    if args.comparisons.is_some() && comparisons.is_empty() {
        return Err("preference export needs explicit comparisons".into());
    }
    for pair in &comparisons {
        for [source, witness] in pair {
            if !sources
                .get(*source)
                .is_some_and(|s| s.witness_indices.contains(witness))
            {
                return Err("comparison references an undeclared route".into());
            }
        }
    }
    if sources.is_empty()
        || sources.iter().any(|s| {
            s.qualification.trim().is_empty()
                || s.witness_indices.is_empty()
                || s.witness_indices
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != s.witness_indices.len()
        })
    {
        return Err("declare qualified sources with nonempty, nonduplicate witness indices".into());
    }
    new_output(&args.output)?;
    let mut provenance = producer()?;
    let observed = args.comparisons.is_some();
    let evidence_kind = if observed {
        "observed_complete_victory"
    } else {
        "known_qualified_complete_route"
    };
    provenance["executor"] = json!(if observed {
        "sts_guided_search::path_search::routes::reconstruct_observed_victory"
    } else {
        "sts_guided_search::path_search::routes::reconstruct_route"
    });
    write_json(
        &args.output.join("producer.json"),
        &json!({
            "producer":provenance, "route_schema":ROUTE_SCHEMA,
            "manifest_sha256":format!("{:x}", Sha256::digest(&manifest_bytes)), "sources":sources,
            "evidence_kind":evidence_kind,"comparisons":comparisons,
            "comparisons_sha256":comparison_bytes.as_ref().map(|b|format!("{:x}",Sha256::digest(b))),
            "limits":{"max_decisions":args.max_decisions,"seconds_per_route":args.seconds_per_route,
                "engine_steps_per_input":limits().max_engine_steps}
        }),
    )?;
    let mut receipts = Vec::new();
    let mut feature_files = Vec::new();
    let mut reconstructed =
        std::collections::HashMap::<[usize; 2], (CompleteRoute, String, String)>::new();
    for (source_index, source) in sources.iter().enumerate() {
        let path = args
            .manifest
            .parent()
            .unwrap_or(Path::new("."))
            .join(&source.artifact);
        let bytes = fs::read(&path)?;
        let artifact: Artifact = serde_json::from_slice(&bytes)?;
        if !artifact.producer.is_object() || artifact.producer.as_object().unwrap().is_empty() {
            return Err("route source needs a producer record".into());
        }
        if !artifact.task_input.prefix.is_empty() {
            return Err("route data requires an explicit full root, not a continuation cut".into());
        }
        let mut task_input = artifact.task_input;
        if let Some(goal) = &source.requirement {
            task_input.requirement = goal.clone();
        }
        let task = ValidatedContinuationTask::new(task_input, limits())?;
        let source_id = format!("{:x}", Sha256::digest(&bytes));
        let mut accepted = 0;
        for &index in &source.witness_indices {
            let witness = artifact
                .witnesses
                .get(index)
                .ok_or("declared witness absent")?;
            let start = Instant::now();
            let reconstruct = if observed {
                reconstruct_observed_victory
            } else {
                reconstruct_route
            };
            let result = reconstruct(
                &task.spec().origin,
                &task.spec().requirement,
                &witness.inputs,
                &witness.successor,
                RouteLimits {
                    max_decisions: args.max_decisions,
                    step: CombatStepLimits {
                        deadline: Some(
                            start
                                .checked_add(allowance)
                                .ok_or("route deadline overflow")?,
                        ),
                        ..limits()
                    },
                },
            );
            let mut receipt = json!({"source_index":source_index,"witness_index":index,
                "source_sha256":source_id,"group":task.spec().group,"task_id":task.id(),
                "reconstruction_micros":start.elapsed().as_micros()});
            match result {
                Ok(route) if serde_json::to_value(&route.outcome)? == witness.outcome => {
                    let filename = format!("route-{source_index:03}-{index:03}.json");
                    let log_probability = route.log_probability(&GroupedNeutralPrior)?;
                    let root_id = format!(
                        "{:x}",
                        Sha256::digest(serde_json::to_vec(&serde_json::to_value(&route.root)?)?)
                    );
                    let route_id =
                        format!("{:x}", Sha256::digest(serde_json::to_vec(&route.inputs)?));
                    receipt["root_id"] = json!(root_id);
                    receipt["route_id"] = json!(route_id);
                    receipt["goal"] = json!(route.goal);
                    let goal_satisfied = route.goal.accepts(&route.outcome);
                    receipt["goal_satisfied"] = json!(goal_satisfied);
                    if let Some(model) = &model {
                        receipt["model_revision"] = json!(model.revision());
                        receipt["model_log_probability"] = json!(route.log_probability(model)?);
                        receipt["model_relative_prior_cost"] =
                            json!(route.relative_prior_cost(model)?);
                    }
                    if args.features {
                        let mut decisions = Vec::new();
                        for d in &route.decisions {
                            let features = linear::encode(
                                &d.state,
                                PathContext {
                                    root: &route.root,
                                    goal: &route.goal,
                                    spent_originals: d.spent_originals_before,
                                },
                                &d.branches,
                            )?;
                            decisions.push(json!({"features":features,"selected":d.selected}));
                        }
                        let file = format!("features-{source_index:03}-{index:03}.json");
                        // Compact numerical payload; descriptive source evidence stays in route files.
                        fs::write(
                            args.output.join(&file),
                            serde_json::to_vec(&json!({
                                "schema":linear::SCHEMA,"root_id":root_id,"route_id":route_id,
                                "family":task.spec().group,"goal":route.goal,"decisions":decisions,
                                "evidence_kind":evidence_kind,"goal_satisfied":goal_satisfied,
                            }))?,
                        )?;
                        feature_files.push(file);
                    }
                    write_json(
                        &args.output.join(&filename),
                        &json!({
                            "producer":provenance,"source_producer":artifact.producer,
                            "root_source_producer":task.spec().source_producer,
                            "source":source,"source_sha256":source_id,"witness_index":index,
                            "group":task.spec().group,"task_id":task.id(),
                            "evidence_kind":evidence_kind,"goal_satisfied":goal_satisfied,"route":route
                        }),
                    )?;
                    receipt["status"] = json!("reconstructed");
                    receipt["file"] = json!(filename);
                    receipt["inputs"] = json!(route.inputs.len());
                    receipt["decisions"] = json!(route.decisions.len());
                    receipt["outcome"] = serde_json::to_value(&route.outcome)?;
                    receipt["neutral_log_probability"] = json!(log_probability);
                    if observed {
                        reconstructed.insert([source_index, index], (route, root_id, route_id));
                    }
                    accepted += 1;
                }
                result => {
                    receipt["status"] = json!("not_exported");
                    receipt["reason"] = json!(match result {
                        Err(error) => error,
                        Ok(_) => "current materialized outcome differs from source evidence".into(),
                    });
                }
            }
            receipts.push(receipt);
        }
        println!(
            "{}: {} complete routes reconstructed, {} not exported",
            task.spec().group,
            accepted,
            source.witness_indices.len() - accepted
        );
    }
    write_json(&args.output.join("summary.json"), &receipts)?;
    if args.features {
        if observed {
            let mut pairs = Vec::new();
            let mut reviews = Vec::new();
            let mut paired_features = std::collections::BTreeSet::new();
            for [left, right] in comparisons {
                let Some((a, root_id, a_id)) = reconstructed.get(&left) else {
                    return Err(
                        "comparison route failed reconstruction; no preference index exported"
                            .into(),
                    );
                };
                let Some((b, _, b_id)) = reconstructed.get(&right) else {
                    return Err(
                        "comparison route failed reconstruction; no preference index exported"
                            .into(),
                    );
                };
                let order = resource_preference(a, b)?;
                reviews.push(json!({"left":left,"right":right,"preference":format!("{order:?}")}));
                let ids = match order {
                    Some(std::cmp::Ordering::Greater) => Some((a_id, b_id)),
                    Some(std::cmp::Ordering::Less) => Some((b_id, a_id)),
                    _ => None,
                };
                if let Some((better, worse)) = ids {
                    for [source, witness] in [left, right] {
                        paired_features.insert(format!("features-{source:03}-{witness:03}.json"));
                    }
                    pairs.push(json!({"root_id":root_id,"goal":a.goal,
                        "better_route_id":better,"worse_route_id":worse}));
                }
            }
            write_json(&args.output.join("comparisons.json"), &reviews)?;
            write_json(
                &args.output.join("preferences.json"),
                &json!({
                    "schema":1,"features":paired_features,"pairs":pairs,
                    "label":"strict_materialized_resource_dominance"
                }),
            )?;
            println!(
                "{} explicit comparisons, {} strict resource preferences",
                reviews.len(),
                pairs.len()
            );
        } else {
            write_json(&args.output.join("features.json"), &feature_files)?;
        }
    }
    Ok(())
}
