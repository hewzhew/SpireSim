use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::options::ReviewOptions;

#[path = "counterfactual_hp/classification.rs"]
mod classification;
#[path = "counterfactual_hp/execution.rs"]
mod execution;
#[path = "counterfactual_hp/targets.rs"]
mod targets;
#[path = "counterfactual_hp/types.rs"]
mod types;

pub(super) use types::CounterfactualHpProbe;

use classification::classify_counterfactual_hp_probe;
use execution::run_counterfactual_hp_level;
use targets::counterfactual_hp_targets;

pub(super) fn run_counterfactual_hp_probe(
    options: &ReviewOptions,
    case: &CombatCase,
) -> CounterfactualHpProbe {
    let original_hp = case.core.position.combat.entities.player.current_hp;
    let max_hp = case.core.position.combat.entities.player.max_hp.max(1);
    let levels = counterfactual_hp_targets(&options.counterfactual_hp_levels, original_hp, max_hp)
        .into_iter()
        .map(|(label, hp)| run_counterfactual_hp_level(options, case, label, hp))
        .collect::<Vec<_>>();
    let classification = classify_counterfactual_hp_probe(&levels, original_hp);
    CounterfactualHpProbe {
        schema: "counterfactual_hp_probe_v0",
        contract: "diagnostic_only_mutate_root_player_hp_then_replay_found_win_line_on_original_hp_no_runner_policy_change",
        original_hp,
        max_hp,
        levels,
        classification,
    }
}
