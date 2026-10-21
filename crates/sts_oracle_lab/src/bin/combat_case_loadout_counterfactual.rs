use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::Serialize;
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::content::powers::PowerId;
use sts_oracle_runtime::content::relics::{RelicId, RelicState};
use sts_oracle_runtime::engine::action_handlers::execute_action;
use sts_oracle_runtime::eval::combat_case::{
    load_combat_case, CombatCase, CombatCaseWitnessBudgetV1,
};
use sts_oracle_runtime::eval::combat_case_context::CombatCaseReplayCapabilityV2;
use sts_oracle_runtime::runtime::action::Action;
use sts_oracle_runtime::runtime::combat::CardLineageId;
use sts_oracle_runtime::state::core::EngineState;

#[derive(Clone, Debug, Args)]
pub(super) struct CombatCaseLoadoutCounterfactualArgs {
    /// Exact source case. The source is never modified.
    #[arg(long)]
    case: PathBuf,
    /// Fresh isolated counterfactual case path.
    #[arg(long)]
    output: PathBuf,
    /// Exact typed lineage of one master-deck card to remove from its current combat zone.
    #[arg(long, required = true)]
    remove_card_lineage: CardLineageId,
    /// Stable relic ID to add. The current typed implementation accepts BronzeScales only.
    #[arg(long)]
    add_relic: String,
}

#[derive(Debug, Serialize)]
pub(super) struct CombatCaseLoadoutCounterfactualReportV1 {
    schema_name: &'static str,
    schema_version: u32,
    contract: &'static str,
    source_case: PathBuf,
    output_case: PathBuf,
    source_root_exact_state_hash: String,
    counterfactual_root_exact_state_hash: String,
    removed_card: RemovedCardV1,
    added_relic: &'static str,
    applied_battle_start_effect: AppliedBattleStartEffectV1,
    replay_capability: CombatCaseReplayCapabilityV2,
}

#[derive(Debug, Serialize)]
struct RemovedCardV1 {
    lineage: CardLineageId,
    card: String,
    upgrades: u8,
}

#[derive(Debug, Serialize)]
struct AppliedBattleStartEffectV1 {
    power: &'static str,
    amount: i32,
}

pub(super) fn run(
    args: CombatCaseLoadoutCounterfactualArgs,
) -> Result<CombatCaseLoadoutCounterfactualReportV1, String> {
    if args.output.exists() {
        return Err(format!(
            "counterfactual output already exists: {}",
            args.output.display()
        ));
    }
    if args.case == args.output {
        return Err("source and counterfactual output paths must differ".to_string());
    }
    let relic_id = RelicId::from_stable_id(&args.add_relic).ok_or_else(|| {
        format!(
            "unknown stable relic ID '{}'; use the simulator stable ID",
            args.add_relic
        )
    })?;
    if relic_id != RelicId::BronzeScales {
        return Err(format!(
            "loadout counterfactual currently supports BronzeScales only; '{}' has battle-start semantics that this command does not yet reconstruct",
            relic_id.stable_id()
        ));
    }

    let mut case = load_combat_case(&args.case)?;
    if !matches!(case.core.position.engine, EngineState::CombatPlayerTurn) {
        return Err("loadout counterfactual requires a stable player-turn combat root".to_string());
    }
    let source_root_exact_state_hash =
        combat_exact_state_hash_v2(&case.core.position.engine, &case.core.position.combat);
    let combat = &mut case.core.position.combat;
    if !combat.engine.action_queue.is_empty()
        || !combat.zones.queued_cards.is_empty()
        || !combat.zones.limbo.is_empty()
    {
        return Err(
            "loadout counterfactual refuses a root with queued or limbo combat work".to_string(),
        );
    }
    if combat.entities.player.has_relic(relic_id) {
        return Err(format!(
            "source combat already has relic {}",
            relic_id.stable_id()
        ));
    }

    let master_matches = combat
        .meta
        .master_deck_snapshot
        .iter()
        .enumerate()
        .filter(|(_, card)| card.lineage == args.remove_card_lineage)
        .collect::<Vec<_>>();
    if master_matches.len() != 1 {
        return Err(format!(
            "expected card lineage {} exactly once in the master deck, found {}",
            args.remove_card_lineage,
            master_matches.len()
        ));
    }
    let master_index = master_matches[0].0;
    let master_card = combat.meta.master_deck_snapshot[master_index].clone();
    let zone_card = combat
        .take_card_by_lineage_from_anywhere(args.remove_card_lineage)
        .ok_or_else(|| {
            format!(
                "card lineage {} is in the master deck but absent from every combat zone",
                args.remove_card_lineage
            )
        })?;
    if zone_card.id != master_card.id || zone_card.upgrades != master_card.upgrades {
        return Err(format!(
            "card lineage {} disagrees between master deck and combat zone",
            args.remove_card_lineage
        ));
    }
    let removed_master_card = combat
        .meta
        .master_deck_snapshot
        .remove_card_by_lineage(args.remove_card_lineage)
        .ok_or_else(|| {
            format!(
                "card lineage {} disappeared from the master deck during counterfactual setup",
                args.remove_card_lineage
            )
        })?;
    debug_assert_eq!(removed_master_card, master_card);

    combat.entities.player.add_relic(RelicState::new(relic_id));
    let player_id = combat.entities.player.id;
    let mut battle_start_actions =
        sts_oracle_runtime::content::relics::bronze_scales::BronzeScales::at_battle_start(
            player_id,
        );
    if battle_start_actions.len() != 1 {
        return Err(format!(
            "BronzeScales battle-start hook produced {} actions, expected exactly one",
            battle_start_actions.len()
        ));
    }
    let action = battle_start_actions.remove(0).action;
    let amount = match &action {
        Action::ApplyPower {
            source,
            target,
            power_id: PowerId::Thorns,
            amount,
        } if *source == player_id && *target == player_id => *amount,
        other => {
            return Err(format!(
                "BronzeScales battle-start hook changed shape: {other:?}"
            ));
        }
    };
    execute_action(action, combat);

    case.core.gap.boundary = format!(
        "{} [isolated loadout counterfactual]",
        case.core.gap.boundary
    );
    case.core.gap.reason = format!(
        "removed master-deck card {}+{} lineage {}; added BronzeScales and replayed its typed battle-start hook",
        master_card.id.stable_id(),
        master_card.upgrades,
        master_card.lineage
    );
    case.core.gap.witness_budget = CombatCaseWitnessBudgetV1::NotRun;
    case.core.run.deck_size = case.core.position.combat.meta.master_deck_snapshot.len();
    case.core.run.relic_count = case.core.position.combat.entities.player.relics.len();
    case.branch_evidence = None;
    case.atomic_combat_search_attempts.clear();
    case.failed_atomic_combat_search = None;
    case.path.clear();
    case.refresh_derived_summaries_and_clear_production_context();

    let counterfactual_root_exact_state_hash =
        combat_exact_state_hash_v2(&case.core.position.engine, &case.core.position.combat);
    let replay_capability = case.replay_capability_v2()?;
    if replay_capability != CombatCaseReplayCapabilityV2::IsolatedProjection {
        return Err(format!(
            "counterfactual case retained unexpected replay capability {replay_capability:?}"
        ));
    }
    write_new_case(&args.output, &case)?;
    let reloaded = load_combat_case(&args.output)?;
    let reloaded_hash = combat_exact_state_hash_v2(
        &reloaded.core.position.engine,
        &reloaded.core.position.combat,
    );
    if reloaded_hash != counterfactual_root_exact_state_hash {
        return Err(format!(
            "saved counterfactual hash mismatch: expected {counterfactual_root_exact_state_hash}, reloaded {reloaded_hash}"
        ));
    }

    Ok(CombatCaseLoadoutCounterfactualReportV1 {
        schema_name: "OracleCombatCaseLoadoutCounterfactualV1",
        schema_version: 1,
        contract: "isolated_exact_combat_projection_same_rng_remove_one_master_card_add_bronze_scales_apply_typed_battle_start_hook_no_run_level_counterfactual_claim",
        source_case: args.case,
        output_case: args.output,
        source_root_exact_state_hash,
        counterfactual_root_exact_state_hash,
        removed_card: RemovedCardV1 {
            lineage: master_card.lineage,
            card: master_card.id.stable_id().to_string(),
            upgrades: master_card.upgrades,
        },
        added_relic: RelicId::BronzeScales.stable_id(),
        applied_battle_start_effect: AppliedBattleStartEffectV1 {
            power: "Thorns",
            amount,
        },
        replay_capability,
    })
}

fn write_new_case(path: &Path, case: &CombatCase) -> Result<(), String> {
    let mut payload = serde_json::to_vec_pretty(case)
        .map_err(|error| format!("failed to serialize counterfactual case: {error}"))?;
    payload.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "failed to create fresh counterfactual case '{}': {error}",
                path.display()
            )
        })?;
    file.write_all(&payload).map_err(|error| {
        format!(
            "failed to write counterfactual case '{}': {error}",
            path.display()
        )
    })?;
    file.sync_all().map_err(|error| {
        format!(
            "failed to sync counterfactual case '{}': {error}",
            path.display()
        )
    })
}
