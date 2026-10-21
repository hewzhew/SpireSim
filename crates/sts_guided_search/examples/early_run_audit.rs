//! Automatic natural-prefix diagnostic. No external actor or historical state.
use clap::Parser;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use sts_guided_search::natural::audit::{self, AuditConfig};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    output: PathBuf,
    #[arg(long,num_args=1..,default_values_t=[91_001_u64,91_002,91_003])]
    seeds: Vec<u64>,
    #[arg(long, default_value_t = 30_000)]
    work_per_combat: usize,
    #[arg(long, default_value_t = 180_000)]
    work_per_run: usize,
    #[arg(long, default_value_t = 20)]
    seconds_per_run: u64,
    #[arg(long, default_value_t = 1024)]
    max_actions: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut unique = args.seeds.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != args.seeds.len() || unique.is_empty() || unique.len() > 16 {
        return Err("declare 1..16 distinct seeds".into());
    }
    if args.output.exists() && fs::read_dir(&args.output)?.next().is_some() {
        return Err("output must be new or empty".into());
    }
    if args.max_actions == 0
        || args.work_per_combat == 0
        || args.work_per_run == 0
        || args.seconds_per_run == 0
    {
        return Err("budgets must be positive".into());
    }
    let config = AuditConfig {
        max_actions: args.max_actions,
        work_per_combat: args.work_per_combat,
        work_per_run: args.work_per_run,
        seconds_per_run: args.seconds_per_run,
    };
    fs::create_dir_all(&args.output)?;
    let git_head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    let git_status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;
    if !git_head.status.success() || !git_status.status.success() {
        return Err("cannot record repository producer".into());
    }
    let producer = json!({"git_head":String::from_utf8_lossy(&git_head.stdout).trim(),"git_worktree_dirty":!git_status.stdout.is_empty(),
        "binary_sha256":format!("{:x}",Sha256::digest(fs::read(std::env::current_exe()?)?)),
        "source_sha256":format!("{:x}",Sha256::digest(concat!(include_str!("early_run_audit.rs"),include_str!("../src/natural.rs"),include_str!("../src/natural/strategic.rs"),
            include_str!("../src/natural/audit.rs"),include_str!("../src/natural/audit/policy.rs"),include_str!("../src/solver.rs"),include_str!("../src/solver/retention.rs"),include_str!("../src/combat.rs")).as_bytes())),
        "information":"oracle_combat_search_visible_setup_rules","actor":"early_setup_rules_v1_with_combat_solver"});
    // Declare the full panel before inspecting any offer or outcome.
    fs::write(
        args.output.join("manifest.json"),
        serde_json::to_vec_pretty(
            &json!({"seeds":args.seeds,"config":config,"producer":producer,
        "scope":"fresh Ironclad A20, Heart available, Rich Neow, through natural F6-F8 elite settlement",
        "purpose":"automatic execution and decision-quality diagnosis; not learned policy evaluation or seed-SL certification"}),
        )?,
    )?;
    let mut summaries = Vec::new();
    for seed in &args.seeds {
        match audit::run(*seed, config.clone()) {
            Ok(report) => {
                fs::write(
                    args.output.join(format!("{seed}.json")),
                    serde_json::to_vec_pretty(&json!({"producer":producer,"report":report}))?,
                )?;
                let mut lines = vec![format!(
                    "Seed {seed} | Ironclad A20 | automatic early-run diagnostic"
                )];
                lines.extend(report.decisions.iter().map(|d| {
                    format!(
                        "F{}{} | HP {} | {}",
                        d.floor,
                        d.turn.map(|t| format!(" T{t}")).unwrap_or_default(),
                        d.hp_before,
                        d.chosen
                    )
                }));
                lines.push(format!("Stop: {}\n{}", report.stop, report.final_view));
                fs::write(
                    args.output.join(format!("{seed}.txt")),
                    lines.join("\n") + "\n",
                )?;
                println!(
                    "Seed {seed} | {} | replay {}\n{}",
                    report.stop, report.trajectory_replayed, report.final_view
                );
                summaries.push(json!({
                    "seed":seed,
                    "stop":report.stop,
                    "error":report.error,
                    "search_work":report.search_work,
                    "trajectory_replayed":report.trajectory_replayed,
                    "final_resources":report.final_resources,
                    "settled_combats":report.settled_combats,
                }));
            }
            Err(error) => {
                println!("Seed {seed}: initialization/configuration failure");
                summaries.push(json!({"seed":seed,"error":error}));
            }
        }
        fs::write(
            args.output.join("summary.json"),
            serde_json::to_vec_pretty(
                &json!({"producer":producer,"declared_seeds":args.seeds,"completed_reports":summaries}),
            )?,
        )?;
    }
    Ok(())
}
