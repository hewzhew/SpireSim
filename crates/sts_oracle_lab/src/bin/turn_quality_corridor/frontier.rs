use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::content::potions::PotionId;
use sts_oracle_runtime::content::powers::{store::powers_for, PowerId};
use sts_oracle_runtime::eval::combat_case::{
    load_combat_case, save_combat_case, CombatCase, CombatCaseWitnessBudgetV1,
};
use sts_oracle_runtime::sim::combat::CombatPosition;

use super::{
    load_checkpoint, validate_checkpoint, FrontierStateCheckpointV1,
    TurnQualityCorridorCensorReasonV1, TurnQualityCorridorSettingsV1,
};
use crate::combat_replay_tools::save_combat_inputs;

mod probe;
use probe::{probe_next_turn_frontier, TurnQualityNextTurnProbeReportV1};

#[derive(Debug, Args)]
pub(crate) struct TurnQualityFrontierArgs {
    #[arg(long)]
    case: PathBuf,
    /// Exact compressed frontier produced by `turn-quality-corridor`.
    #[arg(long)]
    checkpoint: PathBuf,
    /// Export one exact descendant case for each available diagnostic view and
    /// tracked-potion state. The directory must not already exist.
    #[arg(long)]
    export_representatives_dir: Option<PathBuf>,
    /// Probe at most this many survival-ranked frontier roots with exact
    /// complete-turn enumeration. Zero disables the bounded viability probe.
    #[arg(long, default_value_t = 0)]
    probe_next_turn_roots: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TurnQualityFrontierReportV1 {
    schema_name: &'static str,
    schema_version: u32,
    behavioral_scope: &'static str,
    case: PathBuf,
    checkpoint: PathBuf,
    root_exact_state_hash: String,
    frontier_depth: usize,
    frontier_states: usize,
    min_boundary_player_hp: i32,
    min_terminal_player_hp: i32,
    settings: TurnQualityCorridorSettingsV1,
    inherited_censoring_reasons: Vec<TurnQualityCorridorCensorReasonV1>,
    tracked_potion: Option<TrackedPotionIdentityV1>,
    census: TurnQualityFrontierCensusV1,
    representative_selection_is_policy: bool,
    representatives: Vec<TurnQualityFrontierRepresentativeV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_turn_probe: Option<TurnQualityNextTurnProbeReportV1>,
    exported_representatives_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
struct TrackedPotionIdentityV1 {
    slot: usize,
    potion_id: PotionId,
    potion_uuid: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum TrackedPotionStateV1 {
    NotTracked,
    SameIdentityAvailable,
    Consumed,
    DifferentIdentity,
    MissingSlot,
    RootIdentityUnavailable,
}

impl TrackedPotionStateV1 {
    fn file_label(self) -> &'static str {
        match self {
            Self::NotTracked => "not_tracked",
            Self::SameIdentityAvailable => "available",
            Self::Consumed => "consumed",
            Self::DifferentIdentity => "different_identity",
            Self::MissingSlot => "missing_slot",
            Self::RootIdentityUnavailable => "root_identity_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TurnQualityRepresentativeViewV1 {
    Survival,
    Progress,
    ActiveSetup,
}

impl TurnQualityRepresentativeViewV1 {
    const ALL: [Self; 3] = [Self::Survival, Self::Progress, Self::ActiveSetup];

    fn file_label(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Progress => "progress",
            Self::ActiveSetup => "active_setup",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct TurnQualityFrontierCensusV1 {
    player_hp: Vec<I32CountV1>,
    player_turn: Vec<U32CountV1>,
    living_enemy_count: Vec<UsizeCountV1>,
    enemy_hp_plus_block: Vec<I32CountV1>,
    tracked_potion_state: Vec<TrackedPotionStateCountV1>,
    living_enemy_composition: Vec<LivingEnemyCompositionCountV1>,
    active_setup: Vec<ActiveSetupCountV1>,
}

#[derive(Clone, Debug, Serialize)]
struct I32CountV1 {
    value: i32,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct U32CountV1 {
    value: u32,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct UsizeCountV1 {
    value: usize,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct TrackedPotionStateCountV1 {
    state: TrackedPotionStateV1,
    count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct LivingEnemyTypeCountV1 {
    enemy_id: EnemyId,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct LivingEnemyCompositionCountV1 {
    members: Vec<LivingEnemyTypeCountV1>,
    count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct ActiveSetupV1 {
    strength: i32,
    dexterity: i32,
    demon_form: i32,
    dark_embrace: i32,
    feel_no_pain: i32,
    corruption: i32,
}

#[derive(Clone, Debug, Serialize)]
struct ActiveSetupCountV1 {
    setup: ActiveSetupV1,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct PlayerPowerAmountV1 {
    power_id: PowerId,
    amount: i32,
}

#[derive(Clone, Debug, Serialize)]
struct TurnQualityFrontierFeaturesV1 {
    player_hp: i32,
    player_block: i32,
    player_turn: u32,
    living_enemy_count: usize,
    enemy_hp_plus_block: i32,
    living_enemy_composition: Vec<LivingEnemyTypeCountV1>,
    tracked_potion_state: TrackedPotionStateV1,
    active_setup: ActiveSetupV1,
    player_powers: Vec<PlayerPowerAmountV1>,
    prefix_action_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct TurnQualityFrontierRepresentativeV1 {
    view: TurnQualityRepresentativeViewV1,
    tracked_potion_state: TrackedPotionStateV1,
    exact_state_hash: String,
    features: TurnQualityFrontierFeaturesV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    case: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_case_prefix_actions: Option<PathBuf>,
}

pub(crate) fn inspect_frontier(
    args: TurnQualityFrontierArgs,
) -> Result<TurnQualityFrontierReportV1, String> {
    let TurnQualityFrontierArgs {
        case,
        checkpoint,
        export_representatives_dir,
        probe_next_turn_roots,
    } = args;
    let base_case = load_combat_case(&case)?;
    let root_exact_state_hash = combat_exact_state_hash_v2(
        &base_case.core.position.engine,
        &base_case.core.position.combat,
    );
    let checkpoint_payload = load_checkpoint(&checkpoint)?;
    validate_checkpoint(
        &checkpoint_payload,
        &root_exact_state_hash,
        checkpoint_payload.min_boundary_player_hp,
        checkpoint_payload.min_terminal_player_hp,
        &checkpoint_payload.settings,
    )?;
    if let Some(directory) = export_representatives_dir.as_ref() {
        if directory.exists() {
            return Err(format!(
                "representative output directory already exists: {}",
                directory.display()
            ));
        }
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }

    let configured_potion_slot = checkpoint_payload.settings.potion_slot;
    let tracked_potion = tracked_potion_identity(&base_case, &checkpoint_payload.settings);
    let census = frontier_census(
        &checkpoint_payload.frontier,
        configured_potion_slot,
        tracked_potion.as_ref(),
    );
    let representatives = select_frontier_representatives(
        &base_case,
        &checkpoint_payload.frontier,
        checkpoint_payload.next_depth,
        configured_potion_slot,
        tracked_potion.as_ref(),
        export_representatives_dir.as_ref(),
    )?;
    let next_turn_probe = (probe_next_turn_roots > 0)
        .then(|| {
            probe_next_turn_frontier(
                &base_case,
                &checkpoint_payload,
                probe_next_turn_roots,
                configured_potion_slot,
                tracked_potion.as_ref(),
                export_representatives_dir.as_ref(),
            )
        })
        .transpose()?;

    Ok(TurnQualityFrontierReportV1 {
        schema_name: "OracleTurnQualityFrontierV1",
        schema_version: 1,
        behavioral_scope: "read_only_exact_frontier_census_and_diagnostic_case_export",
        case,
        checkpoint,
        root_exact_state_hash,
        frontier_depth: checkpoint_payload.next_depth,
        frontier_states: checkpoint_payload.frontier.len(),
        min_boundary_player_hp: checkpoint_payload.min_boundary_player_hp,
        min_terminal_player_hp: checkpoint_payload.min_terminal_player_hp,
        settings: checkpoint_payload.settings,
        inherited_censoring_reasons: checkpoint_payload.censoring_reasons.into_iter().collect(),
        tracked_potion,
        census,
        representative_selection_is_policy: false,
        representatives,
        next_turn_probe,
        exported_representatives_dir: export_representatives_dir,
    })
}

fn tracked_potion_identity(
    base_case: &CombatCase,
    settings: &TurnQualityCorridorSettingsV1,
) -> Option<TrackedPotionIdentityV1> {
    let slot = settings.potion_slot?;
    let potion = base_case
        .core
        .position
        .combat
        .entities
        .potions
        .get(slot)
        .and_then(Option::as_ref)?;
    Some(TrackedPotionIdentityV1 {
        slot,
        potion_id: potion.id,
        potion_uuid: potion.uuid,
    })
}

fn tracked_potion_state(
    position: &CombatPosition,
    configured_slot: Option<usize>,
    tracked: Option<&TrackedPotionIdentityV1>,
) -> TrackedPotionStateV1 {
    let Some(slot) = configured_slot else {
        return TrackedPotionStateV1::NotTracked;
    };
    let Some(tracked) = tracked else {
        return TrackedPotionStateV1::RootIdentityUnavailable;
    };
    let Some(slot_value) = position.combat.entities.potions.get(slot) else {
        return TrackedPotionStateV1::MissingSlot;
    };
    match slot_value {
        None => TrackedPotionStateV1::Consumed,
        Some(potion) if potion.id == tracked.potion_id && potion.uuid == tracked.potion_uuid => {
            TrackedPotionStateV1::SameIdentityAvailable
        }
        Some(_) => TrackedPotionStateV1::DifferentIdentity,
    }
}

fn frontier_census(
    frontier: &[FrontierStateCheckpointV1],
    configured_slot: Option<usize>,
    tracked: Option<&TrackedPotionIdentityV1>,
) -> TurnQualityFrontierCensusV1 {
    let mut player_hp = BTreeMap::<i32, usize>::new();
    let mut player_turn = BTreeMap::<u32, usize>::new();
    let mut living_enemy_count = BTreeMap::<usize, usize>::new();
    let mut enemy_hp_plus_block = BTreeMap::<i32, usize>::new();
    let mut tracked_potion_states = BTreeMap::<TrackedPotionStateV1, usize>::new();
    let mut living_enemy_compositions =
        BTreeMap::<Vec<(u16, usize)>, (Vec<LivingEnemyTypeCountV1>, usize)>::new();
    let mut active_setups = BTreeMap::<(i32, i32, i32, i32, i32, i32), usize>::new();

    for state in frontier {
        let features = frontier_features(state, configured_slot, tracked);
        *player_hp.entry(features.player_hp).or_default() += 1;
        *player_turn.entry(features.player_turn).or_default() += 1;
        *living_enemy_count
            .entry(features.living_enemy_count)
            .or_default() += 1;
        *enemy_hp_plus_block
            .entry(features.enemy_hp_plus_block)
            .or_default() += 1;
        *tracked_potion_states
            .entry(features.tracked_potion_state)
            .or_default() += 1;
        let composition_key = features
            .living_enemy_composition
            .iter()
            .map(|member| (member.enemy_id.stable_ordinal(), member.count))
            .collect::<Vec<_>>();
        let entry = living_enemy_compositions
            .entry(composition_key)
            .or_insert_with(|| (features.living_enemy_composition.clone(), 0));
        entry.1 += 1;
        let setup = features.active_setup;
        *active_setups
            .entry((
                setup.strength,
                setup.dexterity,
                setup.demon_form,
                setup.dark_embrace,
                setup.feel_no_pain,
                setup.corruption,
            ))
            .or_default() += 1;
    }

    TurnQualityFrontierCensusV1 {
        player_hp: player_hp
            .into_iter()
            .map(|(value, count)| I32CountV1 { value, count })
            .collect(),
        player_turn: player_turn
            .into_iter()
            .map(|(value, count)| U32CountV1 { value, count })
            .collect(),
        living_enemy_count: living_enemy_count
            .into_iter()
            .map(|(value, count)| UsizeCountV1 { value, count })
            .collect(),
        enemy_hp_plus_block: enemy_hp_plus_block
            .into_iter()
            .map(|(value, count)| I32CountV1 { value, count })
            .collect(),
        tracked_potion_state: tracked_potion_states
            .into_iter()
            .map(|(state, count)| TrackedPotionStateCountV1 { state, count })
            .collect(),
        living_enemy_composition: living_enemy_compositions
            .into_iter()
            .map(|(_, (members, count))| LivingEnemyCompositionCountV1 { members, count })
            .collect(),
        active_setup: active_setups
            .into_iter()
            .map(
                |(
                    (strength, dexterity, demon_form, dark_embrace, feel_no_pain, corruption),
                    count,
                )| {
                    ActiveSetupCountV1 {
                        setup: ActiveSetupV1 {
                            strength,
                            dexterity,
                            demon_form,
                            dark_embrace,
                            feel_no_pain,
                            corruption,
                        },
                        count,
                    }
                },
            )
            .collect(),
    }
}

fn frontier_features(
    state: &FrontierStateCheckpointV1,
    configured_slot: Option<usize>,
    tracked: Option<&TrackedPotionIdentityV1>,
) -> TurnQualityFrontierFeaturesV1 {
    let combat = &state.position.combat;
    let living_enemy_composition = living_enemy_composition(combat);
    let living_enemy_count = living_enemy_composition
        .iter()
        .map(|member| member.count)
        .sum();
    let enemy_hp_plus_block = combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| {
            monster
                .current_hp
                .max(0)
                .saturating_add(monster.block.max(0))
        })
        .fold(0_i32, i32::saturating_add);
    let mut player_powers = powers_for(combat, combat.entities.player.id)
        .unwrap_or_default()
        .iter()
        .map(|power| PlayerPowerAmountV1 {
            power_id: power.power_type,
            amount: power.amount,
        })
        .collect::<Vec<_>>();
    player_powers.sort_by_key(|power| power.power_id.stable_ordinal());
    TurnQualityFrontierFeaturesV1 {
        player_hp: combat.entities.player.current_hp,
        player_block: combat.entities.player.block,
        player_turn: combat.turn.turn_count,
        living_enemy_count,
        enemy_hp_plus_block,
        living_enemy_composition,
        tracked_potion_state: tracked_potion_state(&state.position, configured_slot, tracked),
        active_setup: active_setup(combat),
        player_powers,
        prefix_action_count: state.actions.len(),
    }
}

fn living_enemy_composition(
    combat: &sts_oracle_runtime::runtime::combat::CombatState,
) -> Vec<LivingEnemyTypeCountV1> {
    let mut counts = BTreeMap::<u16, (EnemyId, usize)>::new();
    for monster in combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
    {
        let entry = counts
            .entry(monster.enemy_id.stable_ordinal())
            .or_insert((monster.enemy_id, 0));
        entry.1 += 1;
    }
    counts
        .into_iter()
        .map(|(_, (enemy_id, count))| LivingEnemyTypeCountV1 { enemy_id, count })
        .collect()
}

fn active_setup(combat: &sts_oracle_runtime::runtime::combat::CombatState) -> ActiveSetupV1 {
    let player = combat.entities.player.id;
    ActiveSetupV1 {
        strength: combat.get_power(player, PowerId::Strength),
        dexterity: combat.get_power(player, PowerId::Dexterity),
        demon_form: combat.get_power(player, PowerId::DemonForm),
        dark_embrace: combat.get_power(player, PowerId::DarkEmbrace),
        feel_no_pain: combat.get_power(player, PowerId::FeelNoPain),
        corruption: combat.get_power(player, PowerId::Corruption),
    }
}

fn select_frontier_representatives(
    base_case: &CombatCase,
    frontier: &[FrontierStateCheckpointV1],
    depth: usize,
    configured_slot: Option<usize>,
    tracked: Option<&TrackedPotionIdentityV1>,
    output_dir: Option<&PathBuf>,
) -> Result<Vec<TurnQualityFrontierRepresentativeV1>, String> {
    let states_by_potion = frontier.iter().fold(
        BTreeMap::<TrackedPotionStateV1, Vec<&FrontierStateCheckpointV1>>::new(),
        |mut grouped, state| {
            grouped
                .entry(tracked_potion_state(
                    &state.position,
                    configured_slot,
                    tracked,
                ))
                .or_default()
                .push(state);
            grouped
        },
    );
    let mut representatives = Vec::new();
    for (potion_state, candidates) in states_by_potion {
        let mut selected_hashes = BTreeSet::<String>::new();
        for view in TurnQualityRepresentativeViewV1::ALL {
            let selected = candidates
                .iter()
                .copied()
                .filter(|state| !selected_hashes.contains(&state.exact_state_hash))
                .max_by(|left, right| compare_representative(left, right, view));
            let Some(selected) = selected else {
                continue;
            };
            selected_hashes.insert(selected.exact_state_hash.clone());
            let (case_path, actions_path) = if let Some(directory) = output_dir {
                let stem = format!("{}.{}", potion_state.file_label(), view.file_label());
                let case_path = directory.join(format!("{stem}.case.json"));
                let actions_path = directory.join(format!("{stem}.prefix.actions.json"));
                export_representative_case(base_case, selected, depth, &case_path)?;
                save_combat_inputs(&actions_path, selected.actions.clone())?;
                (Some(case_path), Some(actions_path))
            } else {
                (None, None)
            };
            representatives.push(TurnQualityFrontierRepresentativeV1 {
                view,
                tracked_potion_state: potion_state,
                exact_state_hash: selected.exact_state_hash.clone(),
                features: frontier_features(selected, configured_slot, tracked),
                case: case_path,
                original_case_prefix_actions: actions_path,
            });
        }
    }
    Ok(representatives)
}

fn compare_representative(
    left: &FrontierStateCheckpointV1,
    right: &FrontierStateCheckpointV1,
    view: TurnQualityRepresentativeViewV1,
) -> Ordering {
    let left_hp = left.position.combat.entities.player.current_hp;
    let right_hp = right.position.combat.entities.player.current_hp;
    let left_living = left
        .position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count();
    let right_living = right
        .position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count();
    let burden = |state: &FrontierStateCheckpointV1| {
        state
            .position
            .combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .map(|monster| {
                monster
                    .current_hp
                    .max(0)
                    .saturating_add(monster.block.max(0))
            })
            .fold(0_i32, i32::saturating_add)
    };
    let left_burden = burden(left);
    let right_burden = burden(right);
    let left_setup = active_setup(&left.position.combat);
    let right_setup = active_setup(&right.position.combat);
    let setup_key = |setup: ActiveSetupV1| {
        (
            setup.demon_form,
            setup.strength,
            setup.dark_embrace,
            setup.feel_no_pain,
            setup.corruption,
            setup.dexterity,
        )
    };
    let deterministic_tail = || {
        right
            .actions
            .len()
            .cmp(&left.actions.len())
            .then_with(|| right.exact_state_hash.cmp(&left.exact_state_hash))
    };
    match view {
        TurnQualityRepresentativeViewV1::Survival => left_hp
            .cmp(&right_hp)
            .then_with(|| right_living.cmp(&left_living))
            .then_with(|| right_burden.cmp(&left_burden))
            .then_with(|| setup_key(left_setup).cmp(&setup_key(right_setup)))
            .then_with(deterministic_tail),
        TurnQualityRepresentativeViewV1::Progress => right_living
            .cmp(&left_living)
            .then_with(|| right_burden.cmp(&left_burden))
            .then_with(|| left_hp.cmp(&right_hp))
            .then_with(|| setup_key(left_setup).cmp(&setup_key(right_setup)))
            .then_with(deterministic_tail),
        TurnQualityRepresentativeViewV1::ActiveSetup => setup_key(left_setup)
            .cmp(&setup_key(right_setup))
            .then_with(|| right_living.cmp(&left_living))
            .then_with(|| right_burden.cmp(&left_burden))
            .then_with(|| left_hp.cmp(&right_hp))
            .then_with(deterministic_tail),
    }
}

fn export_representative_case(
    base_case: &CombatCase,
    state: &FrontierStateCheckpointV1,
    depth: usize,
    output: &PathBuf,
) -> Result<(), String> {
    let mut exported = base_case.clone();
    exported.core.position = state.position.clone();
    exported.refresh_derived_summaries_and_clear_production_context();
    exported.branch_evidence = None;
    exported.atomic_combat_search_attempts.clear();
    exported.failed_atomic_combat_search = None;
    exported.path.clear();
    exported.core.gap.boundary = format!("turn quality frontier depth {depth}");
    exported.core.gap.reason = "oracle_lab_turn_quality_frontier_representative".to_string();
    exported.core.gap.witness_budget = CombatCaseWitnessBudgetV1::NotRun;
    save_combat_case(output, &exported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_runtime::content::potions::{Potion, PotionId};
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::blank_test_combat;

    #[test]
    fn frontier_census_tracks_concrete_potion_identity_consumption() {
        let tracked = TrackedPotionIdentityV1 {
            slot: 2,
            potion_id: PotionId::DexterityPotion,
            potion_uuid: 77,
        };
        let mut available_combat = blank_test_combat();
        available_combat.entities.potions =
            vec![None, None, Some(Potion::new(PotionId::DexterityPotion, 77))];
        let available_position =
            CombatPosition::new(EngineState::CombatPlayerTurn, available_combat);
        let mut consumed_position = available_position.clone();
        consumed_position.combat.entities.potions[2] = None;
        let frontier = vec![
            FrontierStateCheckpointV1 {
                exact_state_hash: "available".to_string(),
                position: available_position,
                actions: Vec::new(),
            },
            FrontierStateCheckpointV1 {
                exact_state_hash: "consumed".to_string(),
                position: consumed_position,
                actions: Vec::new(),
            },
        ];

        let census = frontier_census(&frontier, Some(2), Some(&tracked));

        assert_eq!(census.tracked_potion_state.len(), 2);
        assert!(census.tracked_potion_state.iter().any(|entry| {
            entry.state == TrackedPotionStateV1::SameIdentityAvailable && entry.count == 1
        }));
        assert!(census
            .tracked_potion_state
            .iter()
            .any(|entry| { entry.state == TrackedPotionStateV1::Consumed && entry.count == 1 }));
    }
}
