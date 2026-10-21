use std::path::Path;

use crate::eval::combat_case::{
    CombatCase, CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary, CombatCaseSource,
    CombatCaseWitnessBudgetV1,
};
use crate::eval::combat_case_context::capture_oracle_analysis_combat_case_production_context_v2;

use super::oracle_analysis_workspace_store::load_oracle_analysis_workspace_artifact_v1;
use super::oracle_run::{oracle_run_combat_budgets_v1, OracleRunConfig};

/// Recover one exact combat from an analysis workspace whose unrelated
/// branches may no longer pass current whole-frontier validation.
///
/// The selected branch is still deserialized through the current checkpoint
/// types. This deliberately bypasses only cross-branch fingerprint validation;
/// it does not reinterpret or edit the saved combat state.
pub fn recover_oracle_analysis_combat_case_v2(
    path: &Path,
    branch_id: usize,
) -> Result<CombatCase, String> {
    let artifact = load_oracle_analysis_workspace_artifact_v1(path)?;
    artifact.validate_supported_semantics()?;
    let explorer = &artifact.session.explorer;
    let saved = explorer
        .branches
        .iter()
        .find(|branch| branch.branch_id == branch_id)
        .ok_or_else(|| format!("oracle analysis workspace has no branch {branch_id}"))?;
    let source = CombatCaseSource {
        seed: artifact.seed,
        ascension: artifact.ascension,
        generation: saved.path_depth as usize,
        branch_id: saved.branch_id,
        parent_id: saved.parent_branch_id,
    };
    let session = explorer.hydrated_branch_session(saved)?.into_session()?;
    let position = session.current_active_combat_position()?;
    let (generation_work, wall_ms) = if position.combat.meta.is_boss_fight {
        (
            artifact.budget.boss_generation_work,
            artifact.budget.boss_ms,
        )
    } else if position.combat.meta.is_elite_fight {
        (
            artifact.budget.elite_generation_work,
            artifact.budget.elite_ms,
        )
    } else {
        (
            artifact.budget.hallway_generation_work,
            artifact.budget.hallway_ms,
        )
    };
    let mut case = CombatCase::new(
        source,
        CombatCaseGap {
            boundary: format!(
                "Act {} Floor {} recovered oracle analysis combat",
                session.run_state.act_num, session.run_state.floor_num
            ),
            reason: "selected_branch_recovery".to_string(),
            witness_budget: CombatCaseWitnessBudgetV1::TurnGraphPortfolioV1 {
                generation_work,
                wall_ms,
            },
        },
        CombatCaseRunSummary {
            act: session.run_state.act_num,
            floor: session.run_state.floor_num,
            hp: session.run_state.current_hp,
            max_hp: session.run_state.max_hp,
            gold: session.run_state.gold,
            deck_size: session.run_state.master_deck.len(),
            relic_count: session.run_state.relics.len(),
            potion_slots: session.run_state.potions.len(),
        },
        Vec::new(),
        None,
        Vec::new(),
        CombatCaseRngSummary::from_pool(&session.run_state.rng_pool),
        position,
    );
    let owner_budgets = oracle_run_combat_budgets_v1(&OracleRunConfig {
        seed: artifact.seed,
        ascension: artifact.ascension,
        budget: artifact.budget,
    })
    .with_guidance_bundle(artifact.combat_guidance_bundle.clone());
    case.production_context = Some(capture_oracle_analysis_combat_case_production_context_v2(
        &case.core,
        &session,
        &owner_budgets,
    )?);
    Ok(case)
}
