use clap::Parser;
#[path = "combat_case_review/adjudication_probe.rs"]
mod adjudication_probe;
#[path = "combat_case_review/args.rs"]
mod args;
#[path = "combat_case_review/awakened_one_evidence.rs"]
mod awakened_one_evidence;
#[path = "combat_case_review/awakened_opening_probe.rs"]
mod awakened_opening_probe;
#[path = "combat_case_review/boss_pressure_lens.rs"]
mod boss_pressure_lens;
#[path = "combat_case_review/case_payload.rs"]
mod case_payload;
#[path = "combat_case_review/champ_phase.rs"]
mod champ_phase;
#[path = "combat_case_review/classification.rs"]
mod classification;
#[path = "combat_case_review/counterfactual_hp.rs"]
mod counterfactual_hp;
#[path = "combat_case_review/focus.rs"]
mod focus;
#[path = "combat_case_review/frozen_panel_lanes.rs"]
mod frozen_panel_lanes;
#[path = "combat_case_review/key_card_lifecycle.rs"]
mod key_card_lifecycle;
#[path = "combat_case_review/line_lab.rs"]
mod line_lab;
#[path = "combat_case_review/options.rs"]
mod options;
#[path = "combat_case_review/power_setup_counterfactual.rs"]
mod power_setup_counterfactual;
#[path = "combat_case_review/quality_lanes.rs"]
mod quality_lanes;
#[path = "combat_case_review/review_pipeline.rs"]
mod review_pipeline;
#[path = "combat_case_review/search_intervention.rs"]
mod search_intervention;
#[path = "combat_case_review/search_review.rs"]
mod search_review;
#[path = "combat_case_review/search_runner.rs"]
mod search_runner;
#[path = "combat_case_review/search_types.rs"]
mod search_types;
#[path = "combat_case_review/strategic_feedback.rs"]
mod strategic_feedback;

use args::Args;
use options::ReviewOptions;
use review_pipeline::build_review;
use sts_oracle_runtime::eval::combat_case::load_combat_case;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let case = load_combat_case(&args.case)?;
    let options = ReviewOptions::from_args(&args);
    let review = build_review(args.case.display().to_string(), options, case);
    let payload = if args.compact {
        serde_json::to_string(&review)?
    } else {
        serde_json::to_string_pretty(&review)?
    };
    if let Some(path) = args.write_review.as_ref() {
        std::fs::write(path, payload)?;
        println!("{}", path.display());
    } else {
        println!("{payload}");
    }
    Ok(())
}
