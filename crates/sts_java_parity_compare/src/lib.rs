use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use sts_core::content::cards::CardId;
use sts_core::content::cards::{build_java_id_map, java_id};
use sts_core::content::monsters::factory::EncounterId;
use sts_core::content::monsters::EnemyId;
use sts_core::content::potions::{Potion, PotionId};
use sts_core::content::powers::PowerId;
use sts_core::content::relics::{RelicId, RelicState};
use sts_core::runtime::combat::{
    CardLineageId, CardLineageOrigin, CombatCard, Power, PowerPayload,
};
use sts_core::runtime::rng::StsRng;
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use sts_core::state::core::{ClientInput, EngineState};
use sts_core::state::map::node::RoomType;
use sts_core::state::run::RunState;
use sts_java_parity_capture::CaptureReceipt;
use sts_java_parity_live::{
    validate_combat_scenario_recipe, ActionReceipt, CombatScenarioRecipe,
    ConstructedScenarioProbeReport, LaunchEvidence, ScenarioCapability, ScenarioPlan,
    ScenarioProbePlan, SessionManifest, SCENARIO_CONTENT_PROFILE_ALL_UNLOCKED,
    SCENARIO_PLAN_SCHEMA,
};
use sts_java_parity_protocol::{
    COMBAT_CONTROL_GRANTS, CONTROL_PROFILE, RUN_CONTROL_GRANTS, RUN_CONTROL_PROFILE,
    SCENARIO_CONTROL_GRANTS, SCENARIO_CONTROL_PROFILE,
};

mod legacy_narrow_transition;

const REPORT_SCHEMA: &str = "sts_java_parity_comparison_v1";
const RECEIPT_SCHEMA: &str = "sts_java_parity_capture_receipt_v1";
const BASE_DEFEND_JAVA_ID: &str = "Defend_R";
const BASE_STRIKE_JAVA_ID: &str = "Strike_R";
const WHIRLWIND_JAVA_ID: &str = "Whirlwind";
const CORRUPTION_JAVA_ID: &str = "Corruption";
const POWER_THROUGH_JAVA_ID: &str = "Power Through";
const SECOND_WIND_JAVA_ID: &str = "Second Wind";
const FEED_JAVA_ID: &str = "Feed";
const FEED_BASE_DAMAGE: i32 = 10;
const FEED_UPGRADE_DAMAGE: i32 = 2;
const FEED_BASE_MAX_HP_GAIN: i32 = 3;
const FEED_UPGRADE_MAX_HP_GAIN: i32 = 1;
const MEAT_ON_THE_BONE_HEAL: i32 = 12;
const BURNING_BLOOD_HEAL: i32 = 6;
const FRESH_IRONCLAD_GOLD: i32 = 99;
const MAX_ENGINE_STEPS: usize = 64;
const LIVE_COMPARISON_FAILURE_SCHEMA: &str = "sts_java_parity_live_comparison_failure_v1";

/// A Rust combat root constructed from the same typed declaration consumed by
/// Java's `scenario_create_combat` action.
///
/// The constructor never reads a Java frame.  The frame remains an independent
/// oracle used later by the PairedRoot projection comparator.
pub struct ConstructedRustScenarioRootV1 {
    pub run_state: RunState,
    pub engine_state: EngineState,
    pub combat: sts_core::runtime::combat::CombatState,
    pub encounter: EncounterId,
    pub recipe_id: String,
    pub scenario_id: String,
}

pub fn construct_rust_scenario_root_v1(
    plan: &ScenarioPlan,
) -> Result<ConstructedRustScenarioRootV1, String> {
    validate_combat_scenario_recipe(&plan.recipe)?;
    if plan.schema != SCENARIO_PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.run.character != "IRONCLAD"
        || plan.run.content_profile != SCENARIO_CONTENT_PROFILE_ALL_UNLOCKED
        || plan.run.ascension > 20
    {
        return Err("Rust scenario construction requires one validated Ironclad plan".to_owned());
    }
    let seed_text =
        plan.run.seed.as_deref().ok_or_else(|| {
            "Rust scenario construction requires an explicit engine seed".to_owned()
        })?;
    let seed = seed_text
        .parse::<u64>()
        .map_err(|_| "scenario run seed is not a decimal engine seed".to_owned())?;
    let ascension_level =
        u8::try_from(plan.run.ascension).map_err(|_| "scenario ascension exceeds u8".to_owned())?;

    let mut encounter = None;
    let mut health = None;
    let mut deck = None;
    for capability in &plan.recipe.capabilities {
        match capability {
            ScenarioCapability::Encounter(spec) => {
                encounter = Some(EncounterId::from_stable_id(&spec.encounter_id).ok_or_else(
                    || {
                        format!(
                            "Rust has no exact encounter identity {:?}",
                            spec.encounter_id
                        )
                    },
                )?);
            }
            ScenarioCapability::PlayerHealth(spec) => health = Some(spec),
            ScenarioCapability::MasterDeck(spec) => deck = Some(spec),
            ScenarioCapability::FreshRelics(_) | ScenarioCapability::FreshPotions(_) => {
                return Err(
                    "Rust scenario root v1 does not yet own optional fresh relic or potion construction"
                        .to_owned(),
                );
            }
        }
    }
    let encounter = encounter.ok_or_else(|| "scenario recipe omitted encounter".to_owned())?;
    let health = health.ok_or_else(|| "scenario recipe omitted player health".to_owned())?;
    let deck = deck.ok_or_else(|| "scenario recipe omitted master deck".to_owned())?;
    let java_cards = build_java_id_map();
    let master_deck = deck
        .cards
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let card_id = java_cards
                .get(spec.card_id.as_str())
                .copied()
                .ok_or_else(|| {
                    format!("Rust has no exact Java card identity {:?}", spec.card_id)
                })?;
            let slot = u32::try_from(index)
                .map_err(|_| "scenario deck index exceeds u32".to_owned())?
                .checked_add(1)
                .ok_or_else(|| "scenario recipe slot overflowed u32".to_owned())?;
            let mut card = CombatCard::for_scenario(card_id, slot);
            card.upgrades = u8::try_from(spec.upgrades)
                .map_err(|_| "scenario card upgrades exceed Rust u8".to_owned())?;
            Ok(card)
        })
        .collect::<Result<Vec<_>, String>>()?;

    // `all_unlocked` makes the original start-run lifecycle report all three
    // character wins before Settings.setFinalActAvailability() runs.  The
    // Rust root must therefore build the same Act 4/key-enabled run rather
    // than treating content unlocks as card-pool-only metadata.
    let mut run_state = RunState::new(seed, ascension_level, true, "Ironclad");
    run_state.master_deck = master_deck;
    run_state.current_hp = i32::try_from(health.current_hp)
        .map_err(|_| "scenario current HP exceeds Rust i32".to_owned())?;
    run_state.max_hp =
        i32::try_from(health.max_hp).map_err(|_| "scenario max HP exceeds Rust i32".to_owned())?;

    // Java's scenario action enters a synthetic MonsterRoom through
    // nextRoomTransitionStart.  That transition advances the public floor
    // from the initial Neow floor 0 to combat floor 1 before constructing the
    // encounter; it does not traverse a generated map edge.
    run_state.act_num = 1;
    run_state.floor_num = 1;
    // Java nextRoomTransition increments floorNum and then recreates every
    // floor-scoped RNG from Settings.seed + floorNum before the room builds
    // its monsters and deck.  Assigning floor_num without this transition
    // left Rust on the run-start seed and changed both Cultist HP and every
    // later floor-scoped random decision.
    run_state
        .rng_pool
        .generate_floor_seeds(seed, run_state.floor_num);
    let (engine_state, combat) = sts_core::sim::combat_start::build_natural_combat_start(
        &mut run_state,
        encounter,
        RoomType::MonsterRoom,
    )?;
    Ok(ConstructedRustScenarioRootV1 {
        run_state,
        engine_state,
        combat,
        encounter,
        recipe_id: plan.recipe.recipe_id.clone(),
        scenario_id: plan.recipe.scenario_id.clone(),
    })
}

const AI_RNG: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.aiRng";
const SHUFFLE_RNG: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.shuffleRng";
const CARD_RANDOM_RNG: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRandomRng";
const MISC_RNG: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng";
const MONSTER_HP_RNG: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterHpRng";
const POTION_RNG: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.potionRng";

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub source: ComparisonSourceV1,
    pub transition: ComparedTransitionV1,
    pub original_game: TransitionProjectionV1,
    pub simulator: TransitionProjectionV1,
    pub claims: ComparisonClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonSourceV1 {
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub receipt_path: String,
    pub receipt_sha256: String,
    pub capture_id: String,
    pub communicationmod_source_commit: String,
    pub communicationmod_jar_sha256: String,
    pub sts_jar_sha256: String,
    pub mod_the_spire_jar_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub java_instance_uuid: String,
    pub java_card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionProjectionV1 {
    pub energy_before: i32,
    pub energy_after: i32,
    pub block_before: i32,
    pub block_after: i32,
    pub hand_count_before: usize,
    pub hand_count_after: usize,
    pub discard_count_before: usize,
    pub discard_count_after: usize,
    pub moved_card_reached_discard: bool,
    pub six_rng_streams_unchanged: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonClaimsV1 {
    pub completed_capture_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub unique_hand_to_discard_card_inferred: bool,
    pub base_defend_energy_matches: bool,
    pub base_defend_block_matches: bool,
    pub card_zones_match: bool,
    pub player_hp_and_monsters_unchanged: bool,
    pub six_rng_streams_match: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityStrikeComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedTargetedTransitionV1,
    pub original_game: TargetedStrikeProjectionV1,
    pub simulator: TargetedStrikeProjectionV1,
    pub claims: TargetedStrikeClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiveComparisonSourceV1 {
    pub session_manifest_path: String,
    pub session_manifest_sha256: String,
    pub action_receipt_path: String,
    pub action_receipt_sha256: String,
    pub before_frame_path: String,
    pub before_frame_sha256: String,
    pub after_frame_path: String,
    pub after_frame_sha256: String,
    pub session_id: String,
    pub capture_id: String,
    pub communicationmod_source_commit: String,
    pub communicationmod_jar_sha256: String,
    pub sts_jar_sha256: String,
    pub mod_the_spire_jar_sha256: String,
    pub launch: Option<LaunchEvidence>,
    pub transition_origin: String,
    pub scenario_context: Option<CombatScenarioRecipe>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveComparisonEvidenceScopeV1 {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub constructed_root_verified: bool,
    pub original_game_transition_captured: bool,
    pub simulator_comparison: bool,
    pub narrow_mechanics_parity: bool,
    pub strict_runtime_profile: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityFeedTerminalSettlementComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedTargetedTransitionV1,
    pub original_game: FeedTerminalSettlementProjectionV1,
    pub simulator: FeedTerminalSettlementProjectionV1,
    pub claims: FeedTerminalSettlementClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedTerminalSettlementProjectionV1 {
    pub player_hp_before: i32,
    pub player_max_hp_before: i32,
    pub gold_before: i32,
    pub target_hp_before: i32,
    pub target_block_before: i32,
    pub feed_damage: i32,
    pub feed_max_hp_gain: i32,
    pub meat_on_the_bone_heal: i32,
    pub burning_blood_heal: i32,
    pub player_hp_after: i32,
    pub player_max_hp_after: i32,
    pub gold_after: i32,
    pub successor_boundary: String,
    pub successor_decision: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedTerminalSettlementClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_terminal_transition_verified: bool,
    pub java_owned_feed_and_target_identity_verified: bool,
    pub exact_constructed_predecessor_scope_verified: bool,
    pub feed_lethal_damage_matches: bool,
    pub feed_max_hp_growth_matches: bool,
    pub meat_on_the_bone_victory_heal_matches: bool,
    pub burning_blood_victory_heal_matches: bool,
    pub terminal_hp_max_hp_and_gold_match: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedComparisonWorkflowV1 {
    pub schema: String,
    pub created_at_unix_ms: u128,
    pub passed: bool,
    pub provenance: String,
    pub transition_id: String,
    pub comparison_kind: String,
    pub session_id: String,
    pub case_id: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub launch_profile: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub probe_report: ComparisonFileIdentityV1,
    pub action_receipt: ComparisonFileIdentityV1,
    pub comparison_report: ComparisonFileIdentityV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonFileIdentityV1 {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedTargetedTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_id: String,
    pub java_instance_uuid: String,
    pub java_card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
    pub target_monster_index: usize,
    pub target_monster_instance_id: u64,
    pub target_monster_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetedStrikeProjectionV1 {
    pub energy_before: i32,
    pub energy_after: i32,
    pub player_hp_before: i32,
    pub player_hp_after: i32,
    pub player_block_before: i32,
    pub player_block_after: i32,
    pub target_hp_before: i32,
    pub target_hp_after: i32,
    pub target_block_before: i32,
    pub target_block_after: i32,
    pub hand_count_before: usize,
    pub hand_count_after: usize,
    pub discard_count_before: usize,
    pub discard_count_after: usize,
    pub moved_card_reached_discard: bool,
    pub other_monsters_unchanged: bool,
    pub six_rng_streams_unchanged: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetedStrikeClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_action_identity_verified: bool,
    pub unique_hand_to_discard_card_inferred: bool,
    pub base_strike_energy_matches: bool,
    pub base_strike_target_damage_matches: bool,
    pub card_zones_match: bool,
    pub player_and_other_monsters_unchanged: bool,
    pub six_rng_streams_match: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityWhirlwindComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedUntargetedXCostTransitionV1,
    pub original_game: WhirlwindProjectionV1,
    pub simulator: WhirlwindProjectionV1,
    pub claims: WhirlwindClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedUntargetedXCostTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_id: String,
    pub java_instance_uuid: String,
    pub java_card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
    pub action_parameters_empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhirlwindProjectionV1 {
    pub energy_before: i32,
    pub energy_committed: i32,
    pub energy_after: i32,
    pub effect_repetitions: i32,
    pub chemical_x_bonus_repetitions: i32,
    pub damage_per_enemy_total: i32,
    pub player_hp_before: i32,
    pub player_hp_after: i32,
    pub player_max_hp_before: i32,
    pub player_max_hp_after: i32,
    pub player_block_before: i32,
    pub player_block_after: i32,
    pub monster_hp_before: Vec<i32>,
    pub monster_hp_after: Vec<i32>,
    pub hand_count_before: usize,
    pub hand_count_after: usize,
    pub discard_count_before: usize,
    pub discard_count_after: usize,
    pub moved_card_reached_discard: bool,
    pub six_rng_streams_unchanged: bool,
    pub observed_relic_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhirlwindClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_untargeted_action_identity_verified: bool,
    pub no_caller_supplied_x_parameter_verified: bool,
    pub unique_hand_to_discard_card_inferred: bool,
    pub relic_identity_projection_matches_bound_context: bool,
    pub full_current_energy_commitment_matches: bool,
    pub declared_x_cost_modifier_matches: bool,
    pub total_damage_matches: bool,
    pub card_zones_match: bool,
    pub player_state_matches: bool,
    pub all_six_rng_streams_match: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityDefendLiveComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedTransitionV1,
    pub original_game: TransitionProjectionV1,
    pub simulator: TransitionProjectionV1,
    pub claims: LiveDefendClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiveDefendClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_action_identity_verified: bool,
    pub unique_hand_to_discard_card_inferred: bool,
    pub base_defend_energy_matches: bool,
    pub base_defend_block_matches: bool,
    pub card_zones_match: bool,
    pub player_hp_and_monsters_unchanged: bool,
    pub six_rng_streams_match: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityCultistEndTurnComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedEndTurnTransitionV1,
    pub original_game: EndTurnProjectionV1,
    pub simulator: EndTurnProjectionV1,
    pub claims: CultistEndTurnClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedEndTurnTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_id: String,
    pub monster_instance_id: u64,
    pub monster_id: String,
    pub reconstructed_planned_move_id: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EndTurnProjectionV1 {
    pub turn_before: u32,
    pub turn_after: u32,
    pub player_hp_before: i32,
    pub player_hp_after: i32,
    pub player_block_before: i32,
    pub player_block_after: i32,
    pub energy_before: i32,
    pub energy_after: i32,
    pub monster_hp_before: i32,
    pub monster_hp_after: i32,
    pub hand_count_before: usize,
    pub hand_count_after: usize,
    pub draw_count_before: usize,
    pub draw_count_after: usize,
    pub discard_count_before: usize,
    pub discard_count_after: usize,
    pub exhaust_count_before: usize,
    pub exhaust_count_after: usize,
    pub rng_transitions: Vec<RngTransitionProjectionV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RngTransitionProjectionV1 {
    pub owner: String,
    pub counter_before: i64,
    pub counter_after: i64,
    pub state_changed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CultistEndTurnClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_end_turn_identity_verified: bool,
    pub a20_act1_floor1_cultist_turn_one_scope_verified: bool,
    pub hand_discard_exhaust_and_draw_zones_match: bool,
    pub player_turn_reset_matches: bool,
    pub cultist_public_transition_matches: bool,
    pub cultist_move_history_and_ritual_power_match: bool,
    pub all_six_rng_streams_match: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityBlockPotionComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedPotionTransitionV1,
    pub original_game: BlockPotionProjectionV1,
    pub simulator: BlockPotionProjectionV1,
    pub claims: BlockPotionClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedPotionTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_id: String,
    pub potion_slot: usize,
    pub java_potion_id: String,
    pub potency: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockPotionProjectionV1 {
    pub turn_before: u32,
    pub turn_after: u32,
    pub player_hp_before: i32,
    pub player_hp_after: i32,
    pub player_max_hp_before: i32,
    pub player_max_hp_after: i32,
    pub player_block_before: i32,
    pub player_block_after: i32,
    pub energy_before: i32,
    pub energy_after: i32,
    pub potion_slot: usize,
    pub slot_empty_before: bool,
    pub slot_empty_after: bool,
    pub card_zones_unchanged: bool,
    pub monsters_unchanged: bool,
    pub six_rng_streams_unchanged: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockPotionClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_potion_identity_verified: bool,
    pub unmodified_block_potion_scope_verified: bool,
    pub block_gain_matches: bool,
    pub selected_slot_consumption_matches: bool,
    pub player_energy_hp_cards_and_monsters_match: bool,
    pub all_six_rng_streams_match: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaParityFirePotionComparisonReportV1 {
    pub schema: String,
    pub status: String,
    pub scope: String,
    pub reconstruction: String,
    pub evidence_scope: LiveComparisonEvidenceScopeV1,
    pub source: LiveComparisonSourceV1,
    pub transition: ComparedTargetedPotionTransitionV1,
    pub original_game: FirePotionProjectionV1,
    pub simulator: FirePotionProjectionV1,
    pub claims: FirePotionClaimsV1,
    pub not_claimed: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedTargetedPotionTransitionV1 {
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub action_id: String,
    pub potion_slot: usize,
    pub java_potion_id: String,
    pub potency: i32,
    pub target_monster_index: usize,
    pub target_monster_instance_id: u64,
    pub target_monster_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FirePotionProjectionV1 {
    pub turn_before: u32,
    pub turn_after: u32,
    pub player_hp_before: i32,
    pub player_hp_after: i32,
    pub player_max_hp_before: i32,
    pub player_max_hp_after: i32,
    pub player_block_before: i32,
    pub player_block_after: i32,
    pub energy_before: i32,
    pub energy_after: i32,
    pub potion_slot: usize,
    pub slot_empty_before: bool,
    pub slot_empty_after: bool,
    pub target_hp_before: i32,
    pub target_hp_after: i32,
    pub target_block_before: i32,
    pub target_block_after: i32,
    pub card_zones_unchanged: bool,
    pub other_monsters_unchanged: bool,
    pub six_rng_streams_unchanged: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FirePotionClaimsV1 {
    pub completed_live_action_identity_verified: bool,
    pub adjacent_stable_transition_verified: bool,
    pub java_owned_potion_and_target_identity_verified: bool,
    pub unmodified_fire_potion_scope_verified: bool,
    pub target_damage_matches: bool,
    pub selected_slot_consumption_matches: bool,
    pub player_cards_and_other_monsters_match: bool,
    pub all_six_rng_streams_match: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum JavaParityLiveComparisonReportV1 {
    Defend(JavaParityDefendLiveComparisonReportV1),
    Strike(JavaParityStrikeComparisonReportV1),
    Whirlwind(JavaParityWhirlwindComparisonReportV1),
    OrderedCardPlay(Box<JavaParityOrderedCardPlayComparisonReportV1>),
    CorruptionChain(Box<JavaParityCorruptionChainComparisonReportV1>),
    LetterOpener(Box<JavaParityLetterOpenerComparisonReportV1>),
    CultistEndTurn(JavaParityCultistEndTurnComparisonReportV1),
    BlockPotion(JavaParityBlockPotionComparisonReportV1),
    FirePotion(JavaParityFirePotionComparisonReportV1),
    FeedTerminalSettlement(Box<JavaParityFeedTerminalSettlementComparisonReportV1>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JavaParityLiveComparisonFailureKindV1 {
    EvidenceRejected,
    Unsupported,
    MechanicsDivergence,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JavaParityLiveComparisonFailureStageV1 {
    LoadEvidence,
    ValidateEvidence,
    SelectComparator,
    CompareTransition,
    CompareProjection,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JavaParityLiveComparisonFailureDetailsV1 {
    BlockPotionProjectionMismatch {
        original_game: Box<BlockPotionProjectionV1>,
        simulator: Box<BlockPotionProjectionV1>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JavaParityLiveComparisonFailureV1 {
    pub schema: String,
    pub kind: JavaParityLiveComparisonFailureKindV1,
    pub stage: JavaParityLiveComparisonFailureStageV1,
    pub message: String,
    pub details: Option<JavaParityLiveComparisonFailureDetailsV1>,
}

impl JavaParityLiveComparisonFailureV1 {
    fn new(
        kind: JavaParityLiveComparisonFailureKindV1,
        stage: JavaParityLiveComparisonFailureStageV1,
        message: String,
    ) -> Self {
        Self {
            schema: LIVE_COMPARISON_FAILURE_SCHEMA.to_owned(),
            kind,
            stage,
            message,
            details: None,
        }
    }

    fn block_potion_projection_mismatch(
        original_game: &BlockPotionProjectionV1,
        simulator: &BlockPotionProjectionV1,
    ) -> Self {
        Self {
            schema: LIVE_COMPARISON_FAILURE_SCHEMA.to_owned(),
            kind: JavaParityLiveComparisonFailureKindV1::MechanicsDivergence,
            stage: JavaParityLiveComparisonFailureStageV1::CompareProjection,
            message: "Rust and Java Block Potion report projections disagree".to_owned(),
            details: Some(
                JavaParityLiveComparisonFailureDetailsV1::BlockPotionProjectionMismatch {
                    original_game: Box::new(original_game.clone()),
                    simulator: Box::new(simulator.clone()),
                },
            ),
        }
    }
}

impl From<String> for JavaParityLiveComparisonFailureV1 {
    fn from(message: String) -> Self {
        Self::new(
            JavaParityLiveComparisonFailureKindV1::Unknown,
            JavaParityLiveComparisonFailureStageV1::CompareTransition,
            message,
        )
    }
}

impl JavaParityLiveComparisonReportV1 {
    pub fn status(&self) -> &str {
        match self {
            Self::Defend(report) => &report.status,
            Self::Strike(report) => &report.status,
            Self::Whirlwind(report) => &report.status,
            Self::OrderedCardPlay(report) => &report.status,
            Self::CorruptionChain(report) => &report.status,
            Self::LetterOpener(report) => &report.status,
            Self::CultistEndTurn(report) => &report.status,
            Self::BlockPotion(report) => &report.status,
            Self::FirePotion(report) => &report.status,
            Self::FeedTerminalSettlement(report) => &report.status,
        }
    }

    pub fn transition_kind(&self) -> &str {
        match self {
            Self::Defend(_) => "base_defend",
            Self::Strike(_) => "targeted_base_strike",
            Self::Whirlwind(report) => {
                if report.original_game.chemical_x_bonus_repetitions > 0 {
                    "untargeted_whirlwind_full_energy_chemical_x"
                } else {
                    "untargeted_whirlwind_full_energy"
                }
            }
            Self::OrderedCardPlay(report) => {
                ordered_card_comparison_kind(&report.transition.java_card_id)
                    .unwrap_or("unsupported_ordered_card_play")
            }
            Self::CorruptionChain(report) => {
                corruption_chain_comparison_kind(&report.transition.java_card_id)
                    .unwrap_or("unsupported_corruption_chain_play")
            }
            Self::LetterOpener(report) => letter_opener_comparison_kind(report),
            Self::CultistEndTurn(_) => "cultist_turn_one_end_turn",
            Self::BlockPotion(_) => "block_potion",
            Self::FirePotion(_) => "targeted_fire_potion",
            Self::FeedTerminalSettlement(_) => "feed_terminal_settlement",
        }
    }

    pub fn frame_ids(&self) -> (u64, u64) {
        match self {
            Self::Defend(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::Strike(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::Whirlwind(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::OrderedCardPlay(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::CorruptionChain(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::LetterOpener(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::CultistEndTurn(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::BlockPotion(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::FirePotion(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
            Self::FeedTerminalSettlement(report) => (
                report.transition.from_frame_id,
                report.transition.to_frame_id,
            ),
        }
    }

    fn evidence_scope(&self) -> &LiveComparisonEvidenceScopeV1 {
        match self {
            Self::Defend(report) => &report.evidence_scope,
            Self::Strike(report) => &report.evidence_scope,
            Self::Whirlwind(report) => &report.evidence_scope,
            Self::OrderedCardPlay(report) => &report.evidence_scope,
            Self::CorruptionChain(report) => &report.evidence_scope,
            Self::LetterOpener(report) => &report.evidence_scope,
            Self::CultistEndTurn(report) => &report.evidence_scope,
            Self::BlockPotion(report) => &report.evidence_scope,
            Self::FirePotion(report) => &report.evidence_scope,
            Self::FeedTerminalSettlement(report) => &report.evidence_scope,
        }
    }

    fn source(&self) -> &LiveComparisonSourceV1 {
        match self {
            Self::Defend(report) => &report.source,
            Self::Strike(report) => &report.source,
            Self::Whirlwind(report) => &report.source,
            Self::OrderedCardPlay(report) => &report.source,
            Self::CorruptionChain(report) => &report.source,
            Self::LetterOpener(report) => &report.source,
            Self::CultistEndTurn(report) => &report.source,
            Self::BlockPotion(report) => &report.source,
            Self::FirePotion(report) => &report.source,
            Self::FeedTerminalSettlement(report) => &report.source,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct EnvelopeKind {
    kind: String,
}

#[derive(Clone, Debug, Deserialize)]
struct FrameBoundaryEnvelope {
    boundary: BoundaryProjection,
}

#[derive(Clone, Debug, Deserialize)]
struct StateFrameProjection {
    kind: String,
    capture_id: String,
    #[serde(default)]
    producer_session_id: Option<String>,
    #[serde(default)]
    connection_id: Option<String>,
    frame_id: u64,
    previous_frame_id: Option<u64>,
    profile: String,
    grants: GrantsProjection,
    boundary: BoundaryProjection,
    public_base: PublicBaseProjection,
    parity_internal: ParityInternalProjection,
    rng_internal: RngInternalProjection,
    #[serde(default)]
    card_birth_witness: Option<sts_java_parity_protocol::CardBirthWitness>,
    #[serde(default)]
    action_space: Option<ActionSpaceProjection>,
    #[serde(default)]
    scenario: Option<CombatScenarioRecipe>,
}

#[derive(Clone, Debug, Deserialize)]
struct RunDecisionStateFrameProjection {
    kind: String,
    capture_id: String,
    #[serde(default)]
    producer_session_id: Option<String>,
    #[serde(default)]
    connection_id: Option<String>,
    frame_id: u64,
    previous_frame_id: Option<u64>,
    profile: String,
    grants: GrantsProjection,
    boundary: BoundaryProjection,
    public_base: RunDecisionPublicBaseProjection,
    #[serde(default)]
    scenario: Option<CombatScenarioRecipe>,
}

#[derive(Clone, Debug, Deserialize)]
struct RunDecisionPublicBaseProjection {
    run: RunResourceProjection,
    decision: RunDecisionProjection,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct RunResourceProjection {
    ascension: u8,
    character: String,
    act: i32,
    floor: i32,
    current_hp: i32,
    max_hp: i32,
    gold: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct RunDecisionProjection {
    kind: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ActionSpaceProjection {
    kind: String,
    actions: Vec<ActionProjection>,
}

#[derive(Clone, Debug, Deserialize)]
struct ActionProjection {
    action_id: String,
    kind: String,
    #[serde(default)]
    card_uuid: Option<String>,
    #[serde(default)]
    card_id: Option<String>,
    #[serde(default)]
    potion_slot: Option<usize>,
    #[serde(default)]
    potion_id: Option<String>,
    #[serde(default)]
    potion_potency: Option<i32>,
    #[serde(default)]
    target_monster_instance_id: Option<u64>,
    #[serde(default)]
    target_monster_index: Option<usize>,
    #[serde(default)]
    target_monster_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct GrantsProjection {
    control: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct BoundaryProjection {
    kind: String,
    stable: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct PublicBaseProjection {
    run: RunProjection,
    combat: CombatProjection,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct RunProjection {
    ascension: u8,
    character: String,
    act: i32,
    floor: i32,
}

#[derive(Clone, Debug, Deserialize)]
struct CombatProjection {
    turn: u32,
    player: PlayerProjection,
    hand: Vec<CardProjection>,
    draw_count: usize,
    discard: PublicZoneProjection,
    exhaust: PublicZoneProjection,
    monsters: Vec<MonsterProjection>,
    #[serde(default)]
    potions: Vec<PotionProjection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct PotionProjection {
    slot_index: usize,
    potion_slot: i32,
    potion_id: String,
    empty: bool,
    potency: i32,
    can_use: bool,
    can_discard: bool,
    target_required: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct PlayerProjection {
    current_hp: i32,
    max_hp: i32,
    block: i32,
    energy: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
struct CardProjection {
    instance_uuid: String,
    card_id: String,
    upgrades: u8,
    cost_for_turn: i32,
}

#[derive(Clone, Debug, Deserialize)]
struct PublicZoneProjection {
    cards: Vec<CardProjection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct MonsterProjection {
    index: usize,
    instance_id: u64,
    id: String,
    current_hp: i32,
    max_hp: i32,
    block: i32,
    dead_or_escaped: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct ParityInternalProjection {
    zones: ParityZonesProjection,
    #[serde(default)]
    monsters: Option<Vec<MonsterInternalProjection>>,
    #[serde(default)]
    relics: Vec<RelicIdentityProjection>,
    #[serde(default)]
    player_powers: Vec<PowerInternalProjection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct RelicIdentityProjection {
    index: usize,
    relic_id: String,
    #[serde(default)]
    counter: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct MonsterInternalProjection {
    instance_id: u64,
    next_move_id: Option<u8>,
    intent: Option<String>,
    move_history: Vec<u8>,
    powers: Vec<PowerInternalProjection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct PowerInternalProjection {
    index: usize,
    id: String,
    class_name: String,
    amount: i32,
    #[serde(rename = "type")]
    power_type: String,
    #[serde(default)]
    ritual_skip_first: Option<bool>,
    #[serde(default)]
    ritual_player_controlled: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct ParityZonesProjection {
    draw: OrderedZoneProjection,
    discard: OrderedZoneProjection,
    exhaust: OrderedZoneProjection,
    hand: OrderedZoneProjection,
    limbo: OrderedZoneProjection,
}

#[derive(Clone, Debug, Deserialize)]
struct OrderedZoneProjection {
    ordering: String,
    top: String,
    cards: Vec<CardProjection>,
}

#[derive(Clone, Debug, Deserialize)]
struct RngInternalProjection {
    streams: Vec<RngStreamProjection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
struct RngStreamProjection {
    owner: String,
    algorithm: String,
    scope: String,
    state_hex: Vec<String>,
    counter: i64,
}

struct LoadedCapture {
    receipt: CaptureReceipt,
    artifact_sha256: String,
    receipt_sha256: String,
    frames: Vec<StateFrameProjection>,
}

struct LoadedLiveTransition {
    manifest: SessionManifest,
    receipt: ActionReceipt,
    manifest_path: PathBuf,
    receipt_path: PathBuf,
    before_path: PathBuf,
    after_path: PathBuf,
    manifest_sha256: String,
    receipt_sha256: String,
    before_sha256: String,
    after_sha256: String,
    before: StateFrameProjection,
    after: StateFrameProjection,
}

struct LoadedLiveFrameFiles {
    manifest: SessionManifest,
    receipt: ActionReceipt,
    manifest_path: PathBuf,
    receipt_path: PathBuf,
    before_path: PathBuf,
    after_path: PathBuf,
    manifest_sha256: String,
    receipt_sha256: String,
    before_sha256: String,
    after_sha256: String,
    before_bytes: Vec<u8>,
    after_bytes: Vec<u8>,
}

struct LoadedFeedTerminalTransition {
    files: LoadedLiveFrameFiles,
    before: StateFrameProjection,
    after: RunDecisionStateFrameProjection,
}

struct TargetedCardPlay {
    moved: CardProjection,
    target_index: usize,
    target_instance_id: u64,
    target_id: String,
}

pub fn compare_base_defend_capture(
    artifact_path: &Path,
    receipt_path: &Path,
    from_frame_id: u64,
) -> Result<JavaParityComparisonReportV1, String> {
    let loaded = load_completed_capture(artifact_path, receipt_path)?;
    let before = loaded
        .frames
        .iter()
        .find(|frame| frame.frame_id == from_frame_id)
        .ok_or_else(|| format!("capture has no frame {from_frame_id}"))?;
    let to_frame_id = from_frame_id
        .checked_add(1)
        .ok_or_else(|| "from_frame overflowed u64".to_owned())?;
    let after = loaded
        .frames
        .iter()
        .find(|frame| frame.frame_id == to_frame_id)
        .ok_or_else(|| format!("capture has no frame {to_frame_id}"))?;
    validate_adjacent_frames(before, after, &loaded.receipt.capture_id)?;
    let moved = infer_base_defend_transition(before, after)?;
    let simulator = run_minimal_simulator_transition(before, after, &moved)?;
    let original_game = TransitionProjectionV1 {
        energy_before: before.public_base.combat.player.energy,
        energy_after: after.public_base.combat.player.energy,
        block_before: before.public_base.combat.player.block,
        block_after: after.public_base.combat.player.block,
        hand_count_before: before.public_base.combat.hand.len(),
        hand_count_after: after.public_base.combat.hand.len(),
        discard_count_before: before.public_base.combat.discard.cards.len(),
        discard_count_after: after.public_base.combat.discard.cards.len(),
        moved_card_reached_discard: true,
        six_rng_streams_unchanged: true,
    };
    Ok(JavaParityComparisonReportV1 {
        schema: REPORT_SCHEMA.to_owned(),
        status: "matched".to_owned(),
        scope: "local_deterministic_card_play".to_owned(),
        reconstruction: "minimal_plain_combat_v1".to_owned(),
        source: ComparisonSourceV1 {
            artifact_path: absolute_display(artifact_path)?,
            artifact_sha256: loaded.artifact_sha256,
            receipt_path: absolute_display(receipt_path)?,
            receipt_sha256: loaded.receipt_sha256,
            capture_id: loaded.receipt.capture_id.clone(),
            communicationmod_source_commit: loaded.receipt.source_commit.clone(),
            communicationmod_jar_sha256: loaded.receipt.loaded_mod_jar_sha256.clone(),
            sts_jar_sha256: loaded.receipt.sts_jar_sha256.clone(),
            mod_the_spire_jar_sha256: loaded.receipt.mod_the_spire_jar_sha256.clone(),
        },
        transition: ComparedTransitionV1 {
            from_frame_id,
            to_frame_id,
            java_instance_uuid: moved.instance_uuid.clone(),
            java_card_id: moved.card_id.clone(),
            upgrades: moved.upgrades,
            cost_for_turn: moved.cost_for_turn,
        },
        original_game,
        simulator,
        claims: ComparisonClaimsV1 {
            completed_capture_identity_verified: true,
            adjacent_stable_transition_verified: true,
            unique_hand_to_discard_card_inferred: true,
            base_defend_energy_matches: true,
            base_defend_block_matches: true,
            card_zones_match: true,
            player_hp_and_monsters_unchanged: true,
            six_rng_streams_match: true,
        },
        not_claimed: vec![
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
            "enemy intent and hidden monster runtime are fully isomorphic".to_owned(),
            "Java UUID representation equals the simulator's internal numeric UUID".to_owned(),
            "all save/load and RNG streams are covered".to_owned(),
        ],
    })
}

pub fn compare_base_strike_live(
    action_receipt_path: &Path,
) -> Result<JavaParityStrikeComparisonReportV1, String> {
    let loaded = load_completed_live_transition(action_receipt_path)?;
    validate_live_adjacent_frames(&loaded)?;
    compare_base_strike_loaded(loaded)
}

pub fn compare_base_defend_live(
    action_receipt_path: &Path,
) -> Result<JavaParityDefendLiveComparisonReportV1, String> {
    let loaded = load_completed_live_transition(action_receipt_path)?;
    validate_live_adjacent_frames(&loaded)?;
    compare_base_defend_loaded(loaded)
}

pub fn compare_live_action(
    action_receipt_path: &Path,
) -> Result<JavaParityLiveComparisonReportV1, String> {
    compare_live_action_typed(action_receipt_path).map_err(|failure| failure.message)
}

pub fn compare_live_action_typed(
    action_receipt_path: &Path,
) -> Result<JavaParityLiveComparisonReportV1, JavaParityLiveComparisonFailureV1> {
    let files = load_completed_live_frame_files(action_receipt_path).map_err(|message| {
        JavaParityLiveComparisonFailureV1::new(
            JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
            JavaParityLiveComparisonFailureStageV1::LoadEvidence,
            message,
        )
    })?;
    let successor: FrameBoundaryEnvelope =
        serde_json::from_slice(&files.after_bytes).map_err(|error| {
            JavaParityLiveComparisonFailureV1::new(
                JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
                JavaParityLiveComparisonFailureStageV1::LoadEvidence,
                format!("failed to decode successor frame boundary: {error}"),
            )
        })?;
    if successor.boundary.kind != "combat_player_decision" {
        if successor.boundary.kind != "unsupported_run_decision" {
            return Err(JavaParityLiveComparisonFailureV1::new(
                JavaParityLiveComparisonFailureKindV1::Unsupported,
                JavaParityLiveComparisonFailureStageV1::SelectComparator,
                format!(
                    "live comparator does not support combat transition successor boundary '{}'",
                    successor.boundary.kind
                ),
            ));
        }
        let loaded = parse_completed_feed_terminal_transition(files).map_err(|message| {
            JavaParityLiveComparisonFailureV1::new(
                JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
                JavaParityLiveComparisonFailureStageV1::LoadEvidence,
                message,
            )
        })?;
        validate_feed_terminal_frame_pair(&loaded).map_err(|message| {
            JavaParityLiveComparisonFailureV1::new(
                JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
                JavaParityLiveComparisonFailureStageV1::ValidateEvidence,
                message,
            )
        })?;
        let action = receipted_action_from_frame(&loaded.before, &loaded.files.receipt).map_err(
            |message| {
                JavaParityLiveComparisonFailureV1::new(
                    JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
                    JavaParityLiveComparisonFailureStageV1::ValidateEvidence,
                    message,
                )
            },
        )?;
        return match (action.kind.as_str(), action.card_id.as_deref()) {
            ("play_card", Some(FEED_JAVA_ID)) => compare_feed_terminal_settlement_loaded(loaded)
                .map(Box::new)
                .map(JavaParityLiveComparisonReportV1::FeedTerminalSettlement)
                .map_err(JavaParityLiveComparisonFailureV1::from),
            _ => Err(JavaParityLiveComparisonFailureV1::new(
                JavaParityLiveComparisonFailureKindV1::Unsupported,
                JavaParityLiveComparisonFailureStageV1::SelectComparator,
                "live comparator does not support this combat-to-run transition yet".to_owned(),
            )),
        };
    }
    let loaded = parse_completed_live_transition(files).map_err(|message| {
        JavaParityLiveComparisonFailureV1::new(
            JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
            JavaParityLiveComparisonFailureStageV1::LoadEvidence,
            message,
        )
    })?;
    validate_live_frame_pair(&loaded).map_err(|message| {
        JavaParityLiveComparisonFailureV1::new(
            JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
            JavaParityLiveComparisonFailureStageV1::ValidateEvidence,
            message,
        )
    })?;
    let action = receipted_action(&loaded).map_err(|message| {
        JavaParityLiveComparisonFailureV1::new(
            JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
            JavaParityLiveComparisonFailureStageV1::ValidateEvidence,
            message,
        )
    })?;
    let has_player_powers = !loaded.before.parity_internal.player_powers.is_empty();
    let letter_opener_context = has_letter_opener_context(&loaded);
    match (action.kind.as_str(), action.card_id.as_deref()) {
        ("play_card", Some(BASE_DEFEND_JAVA_ID)) if letter_opener_context => {
            validate_typed_same_turn(&loaded)?;
            compare_letter_opener_loaded(loaded)
                .map(Box::new)
                .map(JavaParityLiveComparisonReportV1::LetterOpener)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        ("play_card", Some(BASE_DEFEND_JAVA_ID)) => {
            validate_typed_same_turn(&loaded)?;
            compare_base_defend_loaded(loaded)
                .map(JavaParityLiveComparisonReportV1::Defend)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        ("play_card", Some(BASE_STRIKE_JAVA_ID)) => {
            validate_typed_same_turn(&loaded)?;
            compare_base_strike_loaded(loaded)
                .map(JavaParityLiveComparisonReportV1::Strike)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        ("play_card", Some(WHIRLWIND_JAVA_ID)) => {
            validate_typed_same_turn(&loaded)?;
            compare_whirlwind_loaded(loaded)
                .map(JavaParityLiveComparisonReportV1::Whirlwind)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        ("play_card", Some(CORRUPTION_JAVA_ID)) => {
            validate_typed_same_turn(&loaded)?;
            compare_corruption_chain_loaded(loaded)
                .map(Box::new)
                .map(JavaParityLiveComparisonReportV1::CorruptionChain)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        ("play_card", Some(POWER_THROUGH_JAVA_ID | SECOND_WIND_JAVA_ID)) if has_player_powers => {
            validate_typed_same_turn(&loaded)?;
            compare_corruption_chain_loaded(loaded)
                .map(Box::new)
                .map(JavaParityLiveComparisonReportV1::CorruptionChain)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        ("play_card", Some(POWER_THROUGH_JAVA_ID | SECOND_WIND_JAVA_ID)) => {
            validate_typed_same_turn(&loaded)?;
            compare_ordered_card_play_loaded(loaded)
                .map(Box::new)
                .map(JavaParityLiveComparisonReportV1::OrderedCardPlay)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        ("end_turn", None) if letter_opener_context => compare_letter_opener_loaded(loaded)
            .map(Box::new)
            .map(JavaParityLiveComparisonReportV1::LetterOpener)
            .map_err(JavaParityLiveComparisonFailureV1::from),
        ("end_turn", None) => compare_cultist_turn_one_end_turn_loaded(loaded)
            .map(JavaParityLiveComparisonReportV1::CultistEndTurn)
            .map_err(JavaParityLiveComparisonFailureV1::from),
        ("use_potion", None) if action.potion_id.as_deref() == Some("Block Potion") => {
            validate_typed_same_turn(&loaded)?;
            compare_block_potion_loaded(loaded).map(JavaParityLiveComparisonReportV1::BlockPotion)
        }
        ("use_potion", None) if action.potion_id.as_deref() == Some("Fire Potion") => {
            validate_typed_same_turn(&loaded)?;
            compare_fire_potion_loaded(loaded)
                .map(JavaParityLiveComparisonReportV1::FirePotion)
                .map_err(JavaParityLiveComparisonFailureV1::from)
        }
        _ => Err(JavaParityLiveComparisonFailureV1::new(
            JavaParityLiveComparisonFailureKindV1::Unsupported,
            JavaParityLiveComparisonFailureStageV1::SelectComparator,
            "live comparator does not support the receipted typed action yet".to_owned(),
        )),
    }
}

fn validate_typed_same_turn(
    loaded: &LoadedLiveTransition,
) -> Result<(), JavaParityLiveComparisonFailureV1> {
    validate_live_same_turn(loaded).map_err(|message| {
        JavaParityLiveComparisonFailureV1::new(
            JavaParityLiveComparisonFailureKindV1::EvidenceRejected,
            JavaParityLiveComparisonFailureStageV1::ValidateEvidence,
            message,
        )
    })
}

pub fn compare_block_potion_live(
    action_receipt_path: &Path,
) -> Result<JavaParityBlockPotionComparisonReportV1, String> {
    let loaded = load_completed_live_transition(action_receipt_path)?;
    validate_live_adjacent_frames(&loaded)?;
    compare_block_potion_loaded(loaded).map_err(|failure| failure.message)
}

pub fn compare_cultist_turn_one_end_turn_live(
    action_receipt_path: &Path,
) -> Result<JavaParityCultistEndTurnComparisonReportV1, String> {
    let loaded = load_completed_live_transition(action_receipt_path)?;
    validate_live_frame_pair(&loaded)?;
    compare_cultist_turn_one_end_turn_loaded(loaded)
}

pub fn compare_whirlwind_live(
    action_receipt_path: &Path,
) -> Result<JavaParityWhirlwindComparisonReportV1, String> {
    let loaded = load_completed_live_transition(action_receipt_path)?;
    validate_live_adjacent_frames(&loaded)?;
    compare_whirlwind_loaded(loaded)
}

fn compare_base_strike_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityStrikeComparisonReportV1, String> {
    let targeted =
        infer_base_strike_transition(&loaded.before, &loaded.after, &loaded.receipt.action_id)?;
    let simulator =
        run_minimal_targeted_strike_transition(&loaded.before, &loaded.after, &targeted)?;
    let before_combat = &loaded.before.public_base.combat;
    let after_combat = &loaded.after.public_base.combat;
    let target_before = &before_combat.monsters[targeted.target_index];
    let target_after = &after_combat.monsters[targeted.target_index];
    let original_game = TargetedStrikeProjectionV1 {
        energy_before: before_combat.player.energy,
        energy_after: after_combat.player.energy,
        player_hp_before: before_combat.player.current_hp,
        player_hp_after: after_combat.player.current_hp,
        player_block_before: before_combat.player.block,
        player_block_after: after_combat.player.block,
        target_hp_before: target_before.current_hp,
        target_hp_after: target_after.current_hp,
        target_block_before: target_before.block,
        target_block_after: target_after.block,
        hand_count_before: before_combat.hand.len(),
        hand_count_after: after_combat.hand.len(),
        discard_count_before: before_combat.discard.cards.len(),
        discard_count_after: after_combat.discard.cards.len(),
        moved_card_reached_discard: true,
        other_monsters_unchanged: true,
        six_rng_streams_unchanged: true,
    };
    Ok(JavaParityStrikeComparisonReportV1 {
        schema: "sts_java_parity_strike_comparison_v2".to_owned(),
        status: "matched".to_owned(),
        scope: "local_targeted_base_strike".to_owned(),
        reconstruction: "minimal_plain_multi_monster_combat_v1".to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedTargetedTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.receipt.action_id,
            java_instance_uuid: targeted.moved.instance_uuid,
            java_card_id: targeted.moved.card_id,
            upgrades: targeted.moved.upgrades,
            cost_for_turn: targeted.moved.cost_for_turn,
            target_monster_index: targeted.target_index,
            target_monster_instance_id: targeted.target_instance_id,
            target_monster_id: targeted.target_id,
        },
        original_game,
        simulator,
        claims: TargetedStrikeClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_action_identity_verified: true,
            unique_hand_to_discard_card_inferred: true,
            base_strike_energy_matches: true,
            base_strike_target_damage_matches: true,
            card_zones_match: true,
            player_and_other_monsters_unchanged: true,
            six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability follows from this adjacent transition".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
            "monster hidden runtime and intent state are fully isomorphic".to_owned(),
            "relics, powers, Strength, Vulnerable, or blocked Strike damage are covered".to_owned(),
            "end-turn or enemy-action parity follows from this card-play comparison".to_owned(),
        ],
    })
}

fn compare_whirlwind_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityWhirlwindComparisonReportV1, String> {
    let relic_context = whirlwind_relic_context(&loaded)?;
    let moved = infer_whirlwind_transition(&loaded, &relic_context)?;
    let simulator =
        run_minimal_whirlwind_transition(&loaded.before, &loaded.after, &moved, &relic_context)?;
    let before_combat = &loaded.before.public_base.combat;
    let after_combat = &loaded.after.public_base.combat;
    let energy_committed = before_combat.player.energy;
    let effect_repetitions = energy_committed + relic_context.chemical_x_bonus_repetitions;
    let original_game = WhirlwindProjectionV1 {
        energy_before: before_combat.player.energy,
        energy_committed,
        energy_after: after_combat.player.energy,
        effect_repetitions,
        chemical_x_bonus_repetitions: relic_context.chemical_x_bonus_repetitions,
        damage_per_enemy_total: effect_repetitions * 5,
        player_hp_before: before_combat.player.current_hp,
        player_hp_after: after_combat.player.current_hp,
        player_max_hp_before: before_combat.player.max_hp,
        player_max_hp_after: after_combat.player.max_hp,
        player_block_before: before_combat.player.block,
        player_block_after: after_combat.player.block,
        monster_hp_before: before_combat
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        monster_hp_after: after_combat
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        hand_count_before: before_combat.hand.len(),
        hand_count_after: after_combat.hand.len(),
        discard_count_before: before_combat.discard.cards.len(),
        discard_count_after: after_combat.discard.cards.len(),
        moved_card_reached_discard: true,
        six_rng_streams_unchanged: true,
        observed_relic_ids: relic_context.observed_relic_ids.clone(),
    };
    let chemical_x = relic_context.chemical_x_bonus_repetitions > 0;
    Ok(JavaParityWhirlwindComparisonReportV1 {
        schema: "sts_java_parity_whirlwind_comparison_v2".to_owned(),
        status: "matched".to_owned(),
        scope: if chemical_x {
            "fresh_exact_chemical_x_unupgraded_whirlwind_from_positive_energy"
        } else {
            "ordinary_unupgraded_whirlwind_from_positive_energy"
        }
        .to_owned(),
        reconstruction: if chemical_x {
            "minimal_declared_chemical_x_multi_monster_combat_v1"
        } else {
            "minimal_plain_multi_monster_combat_v1"
        }
        .to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedUntargetedXCostTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.receipt.action_id,
            java_instance_uuid: moved.instance_uuid,
            java_card_id: moved.card_id,
            upgrades: moved.upgrades,
            cost_for_turn: moved.cost_for_turn,
            action_parameters_empty: true,
        },
        original_game,
        simulator,
        claims: WhirlwindClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_untargeted_action_identity_verified: true,
            no_caller_supplied_x_parameter_verified: true,
            unique_hand_to_discard_card_inferred: true,
            relic_identity_projection_matches_bound_context: true,
            full_current_energy_commitment_matches: true,
            declared_x_cost_modifier_matches: true,
            total_damage_matches: true,
            card_zones_match: true,
            player_state_matches: true,
            all_six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability follows from this adjacent transition".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
            "individual hit events or their animation ordering are observed by adjacent frames"
                .to_owned(),
            if chemical_x {
                "coverage of relics other than the fresh exact Chemical X, free-to-play copies, Strength, Vulnerable, block, or lethal damage"
            } else {
                "coverage of Chemical X, other modifying relics, free-to-play copies, Strength, Vulnerable, block, or lethal damage"
            }
            .to_owned(),
            "other X-cost cards inherit parity from this Whirlwind comparison".to_owned(),
        ],
    })
}

fn compare_base_defend_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityDefendLiveComparisonReportV1, String> {
    let moved = infer_base_defend_live_transition(&loaded)?;
    let simulator = run_minimal_simulator_transition(&loaded.before, &loaded.after, &moved)?;
    let before_combat = &loaded.before.public_base.combat;
    let after_combat = &loaded.after.public_base.combat;
    let original_game = TransitionProjectionV1 {
        energy_before: before_combat.player.energy,
        energy_after: after_combat.player.energy,
        block_before: before_combat.player.block,
        block_after: after_combat.player.block,
        hand_count_before: before_combat.hand.len(),
        hand_count_after: after_combat.hand.len(),
        discard_count_before: before_combat.discard.cards.len(),
        discard_count_after: after_combat.discard.cards.len(),
        moved_card_reached_discard: true,
        six_rng_streams_unchanged: true,
    };
    Ok(JavaParityDefendLiveComparisonReportV1 {
        schema: "sts_java_parity_defend_live_comparison_v2".to_owned(),
        status: "matched".to_owned(),
        scope: "local_base_defend".to_owned(),
        reconstruction: "minimal_plain_combat_v1".to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            java_instance_uuid: moved.instance_uuid,
            java_card_id: moved.card_id,
            upgrades: moved.upgrades,
            cost_for_turn: moved.cost_for_turn,
        },
        original_game,
        simulator,
        claims: LiveDefendClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_action_identity_verified: true,
            unique_hand_to_discard_card_inferred: true,
            base_defend_energy_matches: true,
            base_defend_block_matches: true,
            card_zones_match: true,
            player_hp_and_monsters_unchanged: true,
            six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability follows from this adjacent transition".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
            "monster hidden runtime and intent state are fully isomorphic".to_owned(),
            "Dexterity, Frail, relic, or power-modified Defend is covered".to_owned(),
            "end-turn or enemy-action parity follows from this card-play comparison".to_owned(),
        ],
    })
}

fn compare_block_potion_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityBlockPotionComparisonReportV1, JavaParityLiveComparisonFailureV1> {
    let action = receipted_action(&loaded)?;
    let slot = action
        .potion_slot
        .ok_or_else(|| "typed Block Potion action has no potion slot".to_owned())?;
    if action.kind != "use_potion"
        || action.action_id != format!("use_potion:{slot}:Block Potion:none")
        || action.potion_id.as_deref() != Some("Block Potion")
        || action.potion_potency != Some(12)
        || action.card_uuid.is_some()
        || action.card_id.is_some()
        || action.target_monster_instance_id.is_some()
        || action.target_monster_index.is_some()
        || action.target_monster_id.is_some()
        || !loaded
            .before
            .grants
            .control
            .iter()
            .any(|grant| grant == "combat_use_potion")
    {
        return Err(
            "receipted action is not a typed targetless unmodified Block Potion use"
                .to_owned()
                .into(),
        );
    }
    let constructed_fresh_exact =
        validate_fresh_potion_scenario_binding(&loaded, slot, "Block Potion")?;
    let original_game =
        validate_block_potion_original_transition(&loaded.before, &loaded.after, slot)?;
    let simulator = run_block_potion_transition(&loaded.before, &loaded.after, slot)?;
    ensure_block_potion_projection_match(&original_game, &simulator)?;
    Ok(JavaParityBlockPotionComparisonReportV1 {
        schema: "sts_java_parity_block_potion_comparison_v3".to_owned(),
        status: "matched".to_owned(),
        scope: if constructed_fresh_exact {
            "fresh_exact_targetless_block_potion_without_relic_modifiers"
        } else {
            "ordinary_targetless_block_potion"
        }
        .to_owned(),
        reconstruction: "typed_potion_slot_and_local_combat_v1".to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedPotionTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.receipt.action_id,
            potion_slot: slot,
            java_potion_id: "Block Potion".to_owned(),
            potency: 12,
        },
        original_game,
        simulator,
        claims: BlockPotionClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_potion_identity_verified: true,
            unmodified_block_potion_scope_verified: true,
            block_gain_matches: true,
            selected_slot_consumption_matches: true,
            player_energy_hp_cards_and_monsters_match: true,
            all_six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability follows from this adjacent transition".to_owned(),
            "Sacred Bark, Toy Ornithopter, potion-modifying relics, powers, or custom potions are covered".to_owned(),
            "targeted, discovery, random, passive, discard, or out-of-combat potion parity".to_owned(),
            "all potion slots are reconstructed when they are not involved in this transition".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
        ],
    })
}

fn ensure_block_potion_projection_match(
    original_game: &BlockPotionProjectionV1,
    simulator: &BlockPotionProjectionV1,
) -> Result<(), JavaParityLiveComparisonFailureV1> {
    if original_game == simulator {
        Ok(())
    } else {
        Err(
            JavaParityLiveComparisonFailureV1::block_potion_projection_mismatch(
                original_game,
                simulator,
            ),
        )
    }
}

fn compare_fire_potion_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityFirePotionComparisonReportV1, String> {
    let action = receipted_action(&loaded)?;
    let slot = action
        .potion_slot
        .ok_or_else(|| "typed Fire Potion action has no potion slot".to_owned())?;
    let target_index = action
        .target_monster_index
        .ok_or_else(|| "typed Fire Potion action has no target index".to_owned())?;
    let target_instance_id = action
        .target_monster_instance_id
        .ok_or_else(|| "typed Fire Potion action has no target instance".to_owned())?;
    let target_id = action
        .target_monster_id
        .clone()
        .ok_or_else(|| "typed Fire Potion action has no target ID".to_owned())?;
    if action.kind != "use_potion"
        || action.action_id != format!("use_potion:{slot}:Fire Potion:monster:{target_instance_id}")
        || action.potion_id.as_deref() != Some("Fire Potion")
        || action.potion_potency != Some(20)
        || action.card_uuid.is_some()
        || action.card_id.is_some()
        || !loaded
            .receipt
            .parameters
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        || !loaded
            .before
            .grants
            .control
            .iter()
            .any(|grant| grant == "combat_use_potion")
    {
        return Err(
            "receipted action is not a typed targeted unmodified Fire Potion use".to_owned(),
        );
    }
    let target = loaded
        .before
        .public_base
        .combat
        .monsters
        .get(target_index)
        .ok_or_else(|| "typed Fire Potion target index is absent from predecessor".to_owned())?;
    if target.index != target_index
        || target.instance_id != target_instance_id
        || target.id != target_id
    {
        return Err("typed Fire Potion target identity is inconsistent".to_owned());
    }
    let constructed_fresh_exact =
        validate_fresh_potion_scenario_binding(&loaded, slot, "Fire Potion")?;
    let original_game = validate_fire_potion_original_transition(
        &loaded.before,
        &loaded.after,
        slot,
        target_index,
    )?;
    let simulator = run_fire_potion_transition(&loaded.before, &loaded.after, slot, target_index)?;
    if original_game != simulator {
        return Err("Rust and Java Fire Potion report projections disagree".to_owned());
    }
    Ok(JavaParityFirePotionComparisonReportV1 {
        schema: "sts_java_parity_fire_potion_comparison_v1".to_owned(),
        status: "matched".to_owned(),
        scope: if constructed_fresh_exact {
            "fresh_exact_targeted_fire_potion_without_modifiers"
        } else {
            "ordinary_targeted_fire_potion_without_modifiers"
        }
        .to_owned(),
        reconstruction: "typed_potion_slot_target_and_local_combat_v1".to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedTargetedPotionTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.receipt.action_id.clone(),
            potion_slot: slot,
            java_potion_id: "Fire Potion".to_owned(),
            potency: 20,
            target_monster_index: target_index,
            target_monster_instance_id: target_instance_id,
            target_monster_id: target_id,
        },
        original_game,
        simulator,
        claims: FirePotionClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_potion_and_target_identity_verified: true,
            unmodified_fire_potion_scope_verified: true,
            target_damage_matches: true,
            selected_slot_consumption_matches: true,
            player_cards_and_other_monsters_match: true,
            all_six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability follows from this adjacent transition".to_owned(),
            "Sacred Bark, Toy Ornithopter, potion-modifying relics, powers, block, Intangible, or lethal damage are covered".to_owned(),
            "other targeted, area, discovery, random, passive, discard, or out-of-combat potion parity".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
        ],
    })
}

fn validate_fresh_potion_scenario_binding(
    loaded: &LoadedLiveTransition,
    slot: usize,
    potion_id: &str,
) -> Result<bool, String> {
    let Some(recipe) = loaded.before.scenario.as_ref() else {
        return Ok(false);
    };
    let expected = recipe
        .capabilities
        .iter()
        .find_map(|capability| match capability {
            ScenarioCapability::FreshPotions(spec) => Some(&spec.slots),
            _ => None,
        })
        .ok_or_else(|| {
            "constructed potion comparison omitted fresh exact potion capability".to_owned()
        })?;
    let observed = &loaded.before.public_base.combat.potions;
    if observed.len() != expected.len()
        || observed.iter().zip(expected).any(|(observed, expected)| {
            observed.slot_index != expected.slot_index
                || observed.potion_slot
                    != if expected.potion_id == "Potion Slot" {
                        -1
                    } else {
                        i32::try_from(expected.slot_index).unwrap_or(-99)
                    }
                || observed.potion_id != expected.potion_id
                || observed.empty != (expected.potion_id == "Potion Slot")
        })
    {
        return Err(
            "observed potion inventory disagrees with the bound fresh exact capability".to_owned(),
        );
    }
    let selected = expected.get(slot).ok_or_else(|| {
        "constructed potion action selected a slot outside its bound capability".to_owned()
    })?;
    if selected.slot_index != slot || selected.potion_id != potion_id {
        return Err("constructed potion action disagrees with its bound exact slot".to_owned());
    }
    Ok(true)
}

fn live_comparison_source(loaded: &LoadedLiveTransition) -> Result<LiveComparisonSourceV1, String> {
    let (transition_origin, scenario_context) = live_transition_origin(loaded)?;
    Ok(LiveComparisonSourceV1 {
        session_manifest_path: absolute_display(&loaded.manifest_path)?,
        session_manifest_sha256: loaded.manifest_sha256.clone(),
        action_receipt_path: absolute_display(&loaded.receipt_path)?,
        action_receipt_sha256: loaded.receipt_sha256.clone(),
        before_frame_path: absolute_display(&loaded.before_path)?,
        before_frame_sha256: loaded.before_sha256.clone(),
        after_frame_path: absolute_display(&loaded.after_path)?,
        after_frame_sha256: loaded.after_sha256.clone(),
        session_id: loaded.manifest.session_id.clone(),
        capture_id: loaded.manifest.capture_id.clone(),
        communicationmod_source_commit: loaded.manifest.source_commit.clone(),
        communicationmod_jar_sha256: loaded.manifest.loaded_mod_jar_sha256.clone(),
        sts_jar_sha256: loaded.manifest.sts_jar_sha256.clone(),
        mod_the_spire_jar_sha256: loaded.manifest.mod_the_spire_jar_sha256.clone(),
        launch: loaded.manifest.launch.clone(),
        transition_origin: transition_origin.to_owned(),
        scenario_context,
    })
}

fn live_transition_origin(
    loaded: &LoadedLiveTransition,
) -> Result<(&'static str, Option<CombatScenarioRecipe>), String> {
    match loaded.manifest.profile.as_str() {
        SCENARIO_CONTROL_PROFILE => {
            let launch = loaded.manifest.launch.as_ref().ok_or_else(|| {
                "constructed comparison session omitted typed launch evidence".to_owned()
            })?;
            if loaded.manifest.schema != "sts_java_parity_live_session_v3"
                || launch.control_profile != SCENARIO_CONTROL_PROFILE
                || launch.storage_class != "isolated_working_directory_v1"
                || launch.isolation_id.as_deref().is_none_or(str::is_empty)
                || !matches!(launch.profile.as_str(), "strict" | "accelerated")
            {
                return Err(
                    "constructed comparison did not originate from a validated isolated scenario launch"
                        .to_owned(),
                );
            }
            let before = loaded.before.scenario.as_ref().ok_or_else(|| {
                "constructed comparison predecessor omitted scenario context".to_owned()
            })?;
            let after = loaded.after.scenario.as_ref().ok_or_else(|| {
                "constructed comparison successor omitted scenario context".to_owned()
            })?;
            validate_combat_scenario_recipe(before)?;
            validate_combat_scenario_recipe(after)?;
            if before != after || before.provenance != "constructed_validated" {
                return Err(
                    "constructed comparison did not preserve its exact validated recipe".to_owned(),
                );
            }
            Ok(("constructed_validated", Some(before.clone())))
        }
        CONTROL_PROFILE | RUN_CONTROL_PROFILE => {
            if loaded.before.scenario.is_some() || loaded.after.scenario.is_some() {
                return Err(
                    "ordinary live comparison unexpectedly carries scenario context".to_owned(),
                );
            }
            Ok(("live_unclassified", None))
        }
        _ => Err("live comparison has an unsupported transition origin".to_owned()),
    }
}

fn live_comparison_evidence_scope(
    loaded: &LoadedLiveTransition,
) -> Result<LiveComparisonEvidenceScopeV1, String> {
    let (origin, _) = live_transition_origin(loaded)?;
    Ok(LiveComparisonEvidenceScopeV1 {
        natural_reachability: false,
        strategy_evidence: false,
        training_eligible: false,
        constructed_root_verified: origin == "constructed_validated",
        original_game_transition_captured: true,
        simulator_comparison: true,
        narrow_mechanics_parity: true,
        strict_runtime_profile: loaded
            .manifest
            .launch
            .as_ref()
            .is_some_and(|launch| launch.profile == "strict"),
    })
}

fn feed_terminal_transition_origin(
    loaded: &LoadedFeedTerminalTransition,
) -> Result<(&'static str, CombatScenarioRecipe), String> {
    if loaded.files.manifest.profile != SCENARIO_CONTROL_PROFILE {
        return Err("Feed terminal comparison requires a constructed scenario session".to_owned());
    }
    let launch = loaded.files.manifest.launch.as_ref().ok_or_else(|| {
        "constructed Feed terminal comparison omitted typed launch evidence".to_owned()
    })?;
    if loaded.files.manifest.schema != "sts_java_parity_live_session_v3"
        || launch.control_profile != SCENARIO_CONTROL_PROFILE
        || launch.storage_class != "isolated_working_directory_v1"
        || launch.isolation_id.as_deref().is_none_or(str::is_empty)
        || !matches!(launch.profile.as_str(), "strict" | "accelerated")
    {
        return Err(
            "Feed terminal comparison did not originate from a validated isolated scenario launch"
                .to_owned(),
        );
    }
    let before = loaded
        .before
        .scenario
        .as_ref()
        .ok_or_else(|| "Feed predecessor omitted scenario context".to_owned())?;
    let after = loaded
        .after
        .scenario
        .as_ref()
        .ok_or_else(|| "Feed terminal successor omitted scenario context".to_owned())?;
    validate_combat_scenario_recipe(before)?;
    validate_combat_scenario_recipe(after)?;
    if before != after || before.provenance != "constructed_validated" {
        return Err("Feed terminal transition did not preserve its exact recipe".to_owned());
    }
    Ok(("constructed_validated", before.clone()))
}

fn feed_terminal_comparison_source(
    loaded: &LoadedFeedTerminalTransition,
) -> Result<LiveComparisonSourceV1, String> {
    let (transition_origin, scenario_context) = feed_terminal_transition_origin(loaded)?;
    let files = &loaded.files;
    Ok(LiveComparisonSourceV1 {
        session_manifest_path: absolute_display(&files.manifest_path)?,
        session_manifest_sha256: files.manifest_sha256.clone(),
        action_receipt_path: absolute_display(&files.receipt_path)?,
        action_receipt_sha256: files.receipt_sha256.clone(),
        before_frame_path: absolute_display(&files.before_path)?,
        before_frame_sha256: files.before_sha256.clone(),
        after_frame_path: absolute_display(&files.after_path)?,
        after_frame_sha256: files.after_sha256.clone(),
        session_id: files.manifest.session_id.clone(),
        capture_id: files.manifest.capture_id.clone(),
        communicationmod_source_commit: files.manifest.source_commit.clone(),
        communicationmod_jar_sha256: files.manifest.loaded_mod_jar_sha256.clone(),
        sts_jar_sha256: files.manifest.sts_jar_sha256.clone(),
        mod_the_spire_jar_sha256: files.manifest.mod_the_spire_jar_sha256.clone(),
        launch: files.manifest.launch.clone(),
        transition_origin: transition_origin.to_owned(),
        scenario_context: Some(scenario_context),
    })
}

fn feed_terminal_comparison_evidence_scope(
    loaded: &LoadedFeedTerminalTransition,
) -> Result<LiveComparisonEvidenceScopeV1, String> {
    feed_terminal_transition_origin(loaded)?;
    Ok(LiveComparisonEvidenceScopeV1 {
        natural_reachability: false,
        strategy_evidence: false,
        training_eligible: false,
        constructed_root_verified: true,
        original_game_transition_captured: true,
        simulator_comparison: true,
        narrow_mechanics_parity: true,
        strict_runtime_profile: loaded
            .files
            .manifest
            .launch
            .as_ref()
            .is_some_and(|launch| launch.profile == "strict"),
    })
}

fn validate_feed_terminal_frame_pair(loaded: &LoadedFeedTerminalTransition) -> Result<(), String> {
    let before = &loaded.before;
    let after = &loaded.after;
    let expected_controls: BTreeSet<&str> = SCENARIO_CONTROL_GRANTS.into_iter().collect();
    for (kind, capture_id, producer_session_id, connection_id, frame_id, profile, grants, stable) in [
        (
            before.kind.as_str(),
            before.capture_id.as_str(),
            before.producer_session_id.as_deref(),
            before.connection_id.as_deref(),
            before.frame_id,
            before.profile.as_str(),
            &before.grants,
            before.boundary.stable,
        ),
        (
            after.kind.as_str(),
            after.capture_id.as_str(),
            after.producer_session_id.as_deref(),
            after.connection_id.as_deref(),
            after.frame_id,
            after.profile.as_str(),
            &after.grants,
            after.boundary.stable,
        ),
    ] {
        let actual_controls: BTreeSet<&str> = grants.control.iter().map(String::as_str).collect();
        if kind != "state_frame"
            || capture_id != loaded.files.manifest.capture_id
            || producer_session_id != Some(loaded.files.manifest.producer_session_id.as_str())
            || connection_id != Some(loaded.files.manifest.connection_id.as_str())
            || profile != SCENARIO_CONTROL_PROFILE
            || actual_controls != expected_controls
            || grants.control.len() != actual_controls.len()
            || !stable
            || frame_id == 0
        {
            return Err(
                "Feed terminal frame identity, grants, profile, or stability is inconsistent"
                    .to_owned(),
            );
        }
    }
    if before.boundary.kind != "combat_player_decision"
        || after.boundary.kind != "unsupported_run_decision"
        || after.public_base.decision.kind != "combat_reward"
        || before.frame_id != loaded.files.receipt.based_on_frame_id
        || after.frame_id != loaded.files.receipt.successor_frame_id.unwrap_or_default()
        || after.previous_frame_id != Some(before.frame_id)
    {
        return Err(
            "Feed receipt does not bind one adjacent combat-to-reward transition".to_owned(),
        );
    }
    validate_frame_zone_consistency(before)?;
    if before.public_base.run.ascension != after.public_base.run.ascension
        || before.public_base.run.character != after.public_base.run.character
        || before.public_base.run.act != after.public_base.run.act
        || before.public_base.run.floor != after.public_base.run.floor
        || after.public_base.run.character != "IRONCLAD"
    {
        return Err("Feed terminal action crosses a run or character identity".to_owned());
    }
    feed_terminal_transition_origin(loaded)?;
    Ok(())
}

fn compare_feed_terminal_settlement_loaded(
    loaded: LoadedFeedTerminalTransition,
) -> Result<JavaParityFeedTerminalSettlementComparisonReportV1, String> {
    let action = receipted_action_from_frame(&loaded.before, &loaded.files.receipt)?;
    validate_feed_terminal_predecessor(&loaded, action)?;
    let simulator = run_feed_terminal_settlement_transition(&loaded, action)?;
    let before_player = &loaded.before.public_base.combat.player;
    let target = &loaded.before.public_base.combat.monsters[0];
    let after_run = &loaded.after.public_base.run;
    let moved = loaded
        .before
        .public_base
        .combat
        .hand
        .iter()
        .find(|card| action.card_uuid.as_deref() == Some(card.instance_uuid.as_str()))
        .ok_or_else(|| "Feed action UUID is absent from the predecessor hand".to_owned())?;
    let expected = expected_feed_terminal_settlement(
        before_player.current_hp,
        before_player.max_hp,
        FRESH_IRONCLAD_GOLD,
        target.current_hp,
        target.block,
        moved.upgrades,
        1,
        loaded.after.boundary.kind.clone(),
        loaded.after.public_base.decision.kind.clone(),
    )?;
    let original_game = FeedTerminalSettlementProjectionV1 {
        player_hp_after: after_run.current_hp,
        player_max_hp_after: after_run.max_hp,
        gold_after: after_run.gold,
        ..expected
    };
    if original_game != simulator {
        return Err("Rust and Java Feed terminal settlement projections disagree".to_owned());
    }
    Ok(JavaParityFeedTerminalSettlementComparisonReportV1 {
        schema: "sts_java_parity_feed_terminal_settlement_comparison_v1".to_owned(),
        status: "matched".to_owned(),
        scope: "constructed_feed_plus_lethal_with_vajra_meat_on_the_bone_and_burning_blood"
            .to_owned(),
        reconstruction: "typed_combat_to_reward_terminal_settlement_v1".to_owned(),
        evidence_scope: feed_terminal_comparison_evidence_scope(&loaded)?,
        source: feed_terminal_comparison_source(&loaded)?,
        transition: ComparedTargetedTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.files.receipt.action_id.clone(),
            java_instance_uuid: moved.instance_uuid.clone(),
            java_card_id: moved.card_id.clone(),
            upgrades: moved.upgrades,
            cost_for_turn: moved.cost_for_turn,
            target_monster_index: 0,
            target_monster_instance_id: target.instance_id,
            target_monster_id: target.id.clone(),
        },
        original_game,
        simulator,
        claims: FeedTerminalSettlementClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_terminal_transition_verified: true,
            java_owned_feed_and_target_identity_verified: true,
            exact_constructed_predecessor_scope_verified: true,
            feed_lethal_damage_matches: true,
            feed_max_hp_growth_matches: true,
            meat_on_the_bone_victory_heal_matches: true,
            burning_blood_victory_heal_matches: true,
            terminal_hp_max_hp_and_gold_match: true,
        },
        not_claimed: vec![
            "natural reachability or strategy quality follows from this constructed transition"
                .to_owned(),
            "the Java animation/action queue exposes the individual intermediate healing frames"
                .to_owned(),
            "combat reward contents, reward RNG, potion drop RNG, or card reward RNG match"
                .to_owned(),
            "other Strength values, relic orders, enemies, powers, death hooks, or victory relic combinations are covered"
                .to_owned(),
            "the unsupported_run_decision boundary is a model-controllable reward surface"
                .to_owned(),
        ],
    })
}

fn validate_feed_terminal_predecessor(
    loaded: &LoadedFeedTerminalTransition,
    action: &ActionProjection,
) -> Result<(), String> {
    let before = &loaded.before;
    let combat = &before.public_base.combat;
    if action.kind != "play_card"
        || action.card_id.as_deref() != Some(FEED_JAVA_ID)
        || action.target_monster_index != Some(0)
        || action.target_monster_instance_id != Some(1)
        || action.target_monster_id.as_deref() != Some("Cultist")
        || before.public_base.run.ascension != 20
        || before.public_base.run.act != 1
        || before.public_base.run.floor != 1
        || combat.turn != 3
        || combat.player.current_hp <= 0
        || combat.player.current_hp > combat.player.max_hp
        || combat.player.block != 0
        || combat.player.energy != 3
        || combat.monsters.len() != 1
        || combat.monsters[0].index != 0
        || combat.monsters[0].instance_id != 1
        || combat.monsters[0].id != "Cultist"
        || combat.monsters[0].current_hp <= 0
        || combat.monsters[0].max_hp != 56
        || combat.monsters[0].block != 0
        || combat.monsters[0].dead_or_escaped
    {
        return Err(
            "Feed terminal predecessor is outside the exact supported combat scope".to_owned(),
        );
    }
    let zones = &before.parity_internal.zones;
    if !zones.draw.cards.is_empty()
        || !zones.discard.cards.is_empty()
        || !zones.exhaust.cards.is_empty()
        || !zones.limbo.cards.is_empty()
        || zones.hand.cards.len() != 2
        || zones.hand.cards != combat.hand
        || zones.hand.cards[0].card_id != "Bludgeon"
        || zones.hand.cards[0].upgrades != 1
        || zones.hand.cards[0].cost_for_turn != 3
        || zones.hand.cards[1].card_id != FEED_JAVA_ID
        || zones.hand.cards[1].upgrades > 1
        || zones.hand.cards[1].cost_for_turn != 1
        || action.card_uuid.as_deref() != Some(zones.hand.cards[1].instance_uuid.as_str())
    {
        return Err("Feed terminal predecessor card zones are outside scope".to_owned());
    }
    let expected_relics = ["Burning Blood", "Vajra", "Meat on the Bone"];
    if before.parity_internal.relics.len() != expected_relics.len()
        || before
            .parity_internal
            .relics
            .iter()
            .zip(expected_relics)
            .enumerate()
            .any(|(index, (relic, expected))| relic.index != index || relic.relic_id != expected)
    {
        return Err("Feed terminal predecessor relic order is outside scope".to_owned());
    }
    if before.parity_internal.player_powers.len() != 1 {
        return Err("Feed terminal predecessor requires exact Vajra Strength".to_owned());
    }
    let strength = &before.parity_internal.player_powers[0];
    if strength.index != 0
        || strength.id != "Strength"
        || strength.class_name != "com.megacrit.cardcrawl.powers.StrengthPower"
        || strength.amount != 1
        || strength.power_type != "BUFF"
    {
        return Err("Feed terminal predecessor player power is not exact Strength 1".to_owned());
    }
    let monster = only_typed_monster(before)?;
    if monster.instance_id != 1
        || monster.next_move_id != Some(1)
        || monster.intent.as_deref() != Some("ATTACK")
        || monster.move_history != [3, 1, 1]
        || monster.powers.len() != 2
    {
        return Err("Feed terminal predecessor Cultist runtime is outside scope".to_owned());
    }
    let ritual = &monster.powers[0];
    let cultist_strength = &monster.powers[1];
    if ritual.index != 0
        || ritual.id != "Ritual"
        || ritual.class_name != "com.megacrit.cardcrawl.powers.RitualPower"
        || ritual.amount != 5
        || ritual.power_type != "BUFF"
        || ritual.ritual_skip_first != Some(false)
        || ritual.ritual_player_controlled != Some(false)
        || cultist_strength.index != 1
        || cultist_strength.id != "Strength"
        || cultist_strength.class_name != "com.megacrit.cardcrawl.powers.StrengthPower"
        || cultist_strength.amount != 5
        || cultist_strength.power_type != "BUFF"
    {
        return Err("Feed terminal predecessor Cultist powers are outside scope".to_owned());
    }
    if combat.potions.len() != 2
        || combat.potions.iter().enumerate().any(|(index, potion)| {
            potion.slot_index != index
                || potion.potion_slot != -1
                || potion.potion_id != "Potion Slot"
                || !potion.empty
        })
    {
        return Err("Feed terminal predecessor potion inventory is outside scope".to_owned());
    }
    validate_rng_owners(&before.rng_internal.streams)?;
    let recipe = before
        .scenario
        .as_ref()
        .ok_or_else(|| "Feed terminal predecessor omitted its recipe".to_owned())?;
    validate_feed_terminal_recipe(recipe, combat.player.max_hp, zones.hand.cards[1].upgrades)?;
    let expected_damage = feed_damage_for(zones.hand.cards[1].upgrades, strength.amount)?;
    let lethal_threshold = combat.monsters[0]
        .current_hp
        .checked_add(combat.monsters[0].block)
        .ok_or_else(|| "Feed lethal threshold overflowed i32".to_owned())?;
    if expected_damage < lethal_threshold {
        return Err(
            "Feed terminal predecessor is not lethal under the typed card state".to_owned(),
        );
    }
    let after = &loaded.after.public_base.run;
    let expected = expected_feed_terminal_settlement(
        combat.player.current_hp,
        combat.player.max_hp,
        FRESH_IRONCLAD_GOLD,
        combat.monsters[0].current_hp,
        combat.monsters[0].block,
        zones.hand.cards[1].upgrades,
        strength.amount,
        loaded.after.boundary.kind.clone(),
        loaded.after.public_base.decision.kind.clone(),
    )?;
    if after.current_hp != expected.player_hp_after
        || after.max_hp != expected.player_max_hp_after
        || after.gold != expected.gold_after
    {
        return Err(format!(
            "Feed terminal Java successor disagrees with typed settlement: expected {}/{}/{}, observed {}/{}/{}",
            expected.player_hp_after,
            expected.player_max_hp_after,
            expected.gold_after,
            after.current_hp,
            after.max_hp,
            after.gold
        ));
    }
    Ok(())
}

fn validate_feed_terminal_recipe(
    recipe: &CombatScenarioRecipe,
    predecessor_max_hp: i32,
    feed_upgrades: u8,
) -> Result<(), String> {
    let [ScenarioCapability::Encounter(encounter), ScenarioCapability::PlayerHealth(health), ScenarioCapability::MasterDeck(deck), ScenarioCapability::FreshRelics(relics)] =
        recipe.capabilities.as_slice()
    else {
        return Err("Feed terminal recipe does not have the exact four capabilities".to_owned());
    };
    if encounter.encounter_id != "Cultist"
        || health.current_hp == 0
        || health.current_hp > health.max_hp
        || i32::try_from(health.max_hp).ok() != Some(predecessor_max_hp)
        || deck.cards.len() != 2
        || deck.cards[0].card_id != "Bludgeon"
        || deck.cards[0].upgrades != 1
        || deck.cards[1].card_id != FEED_JAVA_ID
        || u8::try_from(deck.cards[1].upgrades).ok() != Some(feed_upgrades)
        || relics.relics.len() != 3
        || relics.relics[0].relic_id != "Burning Blood"
        || relics.relics[1].relic_id != "Vajra"
        || relics.relics[2].relic_id != "Meat on the Bone"
    {
        return Err("Feed terminal recipe values are outside scope".to_owned());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn expected_feed_terminal_settlement(
    player_hp_before: i32,
    player_max_hp_before: i32,
    gold_before: i32,
    target_hp_before: i32,
    target_block_before: i32,
    feed_upgrades: u8,
    strength: i32,
    successor_boundary: String,
    successor_decision: String,
) -> Result<FeedTerminalSettlementProjectionV1, String> {
    let feed_damage = feed_damage_for(feed_upgrades, strength)?;
    let feed_max_hp_gain = feed_max_hp_gain_for(feed_upgrades)?;
    let player_max_hp_after = player_max_hp_before
        .checked_add(feed_max_hp_gain)
        .ok_or_else(|| "Feed maximum-HP settlement overflowed i32".to_owned())?;
    let after_feed = player_hp_before
        .checked_add(feed_max_hp_gain)
        .ok_or_else(|| "Feed current-HP settlement overflowed i32".to_owned())?
        .min(player_max_hp_after);
    let meat_requested = if i64::from(after_feed) * 2 <= i64::from(player_max_hp_after) {
        MEAT_ON_THE_BONE_HEAL
    } else {
        0
    };
    let after_meat = after_feed
        .checked_add(meat_requested)
        .ok_or_else(|| "Meat on the Bone settlement overflowed i32".to_owned())?
        .min(player_max_hp_after);
    let meat_on_the_bone_heal = after_meat - after_feed;
    let player_hp_after = after_meat
        .checked_add(BURNING_BLOOD_HEAL)
        .ok_or_else(|| "Burning Blood settlement overflowed i32".to_owned())?
        .min(player_max_hp_after);
    let burning_blood_heal = player_hp_after - after_meat;
    Ok(FeedTerminalSettlementProjectionV1 {
        player_hp_before,
        player_max_hp_before,
        gold_before,
        target_hp_before,
        target_block_before,
        feed_damage,
        feed_max_hp_gain,
        meat_on_the_bone_heal,
        burning_blood_heal,
        player_hp_after,
        player_max_hp_after,
        gold_after: gold_before,
        successor_boundary,
        successor_decision,
    })
}

fn feed_damage_for(upgrades: u8, strength: i32) -> Result<i32, String> {
    if upgrades > 1 {
        return Err("Feed terminal comparator supports at most one upgrade".to_owned());
    }
    FEED_BASE_DAMAGE
        .checked_add(FEED_UPGRADE_DAMAGE * i32::from(upgrades))
        .and_then(|damage| damage.checked_add(strength))
        .ok_or_else(|| "Feed damage projection overflowed i32".to_owned())
}

fn feed_max_hp_gain_for(upgrades: u8) -> Result<i32, String> {
    if upgrades > 1 {
        return Err("Feed terminal comparator supports at most one upgrade".to_owned());
    }
    FEED_BASE_MAX_HP_GAIN
        .checked_add(FEED_UPGRADE_MAX_HP_GAIN * i32::from(upgrades))
        .ok_or_else(|| "Feed maximum-HP gain projection overflowed i32".to_owned())
}

fn run_feed_terminal_settlement_transition(
    loaded: &LoadedFeedTerminalTransition,
    action: &ActionProjection,
) -> Result<FeedTerminalSettlementProjectionV1, String> {
    let (mut combat, identities) =
        legacy_narrow_transition::reconstruct_card_combat_base(&loaded.before)?;
    combat.entities.monsters[0].enemy_id = EnemyId::Cultist;
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::BurningBlood));
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Vajra));
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MeatOnTheBone));
    sts_core::content::powers::store::set_powers_for(
        &mut combat,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let selected_uuid = action
        .card_uuid
        .as_deref()
        .and_then(|uuid| identities.rust_lineage(uuid))
        .ok_or_else(|| "Feed UUID is absent from reconstructed combat".to_owned())?;
    let card_index = combat
        .zones
        .hand
        .iter()
        .position(|card| card.lineage == selected_uuid)
        .ok_or_else(|| "Feed is absent from reconstructed hand".to_owned())?;
    let feed_upgrades = combat.zones.hand[card_index].upgrades;
    let expected = expected_feed_terminal_settlement(
        combat.entities.player.current_hp,
        combat.entities.player.max_hp,
        FRESH_IRONCLAD_GOLD,
        combat.entities.monsters[0].current_hp,
        combat.entities.monsters[0].block,
        feed_upgrades,
        1,
        "unsupported_run_decision".to_owned(),
        "combat_reward".to_owned(),
    )?;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index,
        target: Some(1),
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed Feed+ play is not legal in the Rust stepper".to_owned());
    }
    let step = stepper.apply_to_stable(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err("Rust Feed terminal transition did not settle finitely".to_owned());
    }
    if !matches!(step.position.engine, EngineState::RewardScreen(_))
        || !stepper.is_exact_victory(&step.position)
        || step.position.combat.entities.monsters[0].current_hp > 0
    {
        return Err("Rust Feed transition did not reach an exact combat victory reward".to_owned());
    }
    let reached = &step.position.combat;
    Ok(FeedTerminalSettlementProjectionV1 {
        player_hp_after: reached.entities.player.current_hp,
        player_max_hp_after: reached.entities.player.max_hp,
        gold_after: reached.entities.player.gold,
        ..expected
    })
}

fn compare_cultist_turn_one_end_turn_loaded(
    loaded: LoadedLiveTransition,
) -> Result<JavaParityCultistEndTurnComparisonReportV1, String> {
    let action = receipted_action(&loaded)?;
    if action.kind != "end_turn"
        || action.action_id != "end_turn"
        || action.card_uuid.is_some()
        || action.card_id.is_some()
        || action.target_monster_instance_id.is_some()
        || action.target_monster_index.is_some()
        || action.target_monster_id.is_some()
    {
        return Err("receipted action is not the typed parameter-free end turn".to_owned());
    }
    validate_cultist_turn_one_original_transition(&loaded.before, &loaded.after)?;
    let simulator = run_cultist_turn_one_end_turn_transition(&loaded.before, &loaded.after)?;
    let original_game = project_end_turn_frames(&loaded.before, &loaded.after)?;
    if simulator != original_game {
        return Err("Rust and Java end-turn report projections disagree".to_owned());
    }
    let monster = &loaded.before.public_base.combat.monsters[0];
    let (transition_origin, _) = live_transition_origin(&loaded)?;
    Ok(JavaParityCultistEndTurnComparisonReportV1 {
        schema: "sts_java_parity_cultist_end_turn_comparison_v2".to_owned(),
        status: "matched".to_owned(),
        scope: format!("{transition_origin}_a20_cultist_turn_one_end_turn"),
        reconstruction: "typed_zones_rng_move_history_and_power_v1".to_owned(),
        evidence_scope: live_comparison_evidence_scope(&loaded)?,
        source: live_comparison_source(&loaded)?,
        transition: ComparedEndTurnTransitionV1 {
            from_frame_id: loaded.before.frame_id,
            to_frame_id: loaded.after.frame_id,
            action_id: loaded.receipt.action_id,
            monster_instance_id: monster.instance_id,
            monster_id: monster.id.clone(),
            reconstructed_planned_move_id: 3,
        },
        original_game,
        simulator,
        claims: CultistEndTurnClaimsV1 {
            completed_live_action_identity_verified: true,
            adjacent_stable_transition_verified: true,
            java_owned_end_turn_identity_verified: true,
            a20_act1_floor1_cultist_turn_one_scope_verified: true,
            hand_discard_exhaust_and_draw_zones_match: true,
            player_turn_reset_matches: true,
            cultist_public_transition_matches: true,
            cultist_move_history_and_ritual_power_match: true,
            all_six_rng_streams_match: true,
        },
        not_claimed: vec![
            "natural reachability follows from this adjacent transition".to_owned(),
            "arbitrary monster, turn, relic, power, or status-card end-turn parity".to_owned(),
            "monster-specific private runtime beyond typed move history and power instances is fully isomorphic".to_owned(),
            "exact Java combat state is restorable as a Rust checkpoint".to_owned(),
            "all save/load and RNG streams beyond the six declared owners are covered".to_owned(),
        ],
    })
}

pub fn write_report_new(path: &Path, report: &JavaParityComparisonReportV1) -> Result<(), String> {
    write_json_report_new(path, report)
}

pub fn write_json_report_new<T: Serialize>(path: &Path, report: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create report directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "failed to create fresh report '{}': {error}",
                path.display()
            )
        })?;
    serde_json::to_writer_pretty(&mut file, report)
        .map_err(|error| format!("failed to encode comparison report: {error}"))?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|error| {
            format!(
                "failed to persist comparison report '{}': {error}",
                path.display()
            )
        })
}

pub fn write_constructed_comparison_workflow_new(
    probe_report_path: &Path,
    comparison_report_path: &Path,
    workflow_path: &Path,
    comparison: &JavaParityLiveComparisonReportV1,
) -> Result<ConstructedComparisonWorkflowV1, String> {
    let (probe_bytes, probe) = read_constructed_probe_report(probe_report_path)?;
    let workflow = bind_constructed_comparison_workflow(
        probe_report_path,
        &probe_bytes,
        &probe,
        comparison_report_path,
        comparison,
    )?;
    write_json_report_new(workflow_path, &workflow)?;
    Ok(workflow)
}

pub fn recompare_constructed_probe_new(
    probe_report_path: &Path,
    comparison_report_path: &Path,
    workflow_path: &Path,
) -> Result<
    (
        JavaParityLiveComparisonReportV1,
        ConstructedComparisonWorkflowV1,
    ),
    String,
> {
    let (probe_bytes, probe) = read_constructed_probe_report(probe_report_path)?;
    let (action_receipt_path, _) = validate_capture_only_constructed_probe(&probe)?;
    let comparison = compare_live_action(&action_receipt_path)?;
    write_json_report_new(comparison_report_path, &comparison)?;
    let workflow = bind_constructed_comparison_workflow(
        probe_report_path,
        &probe_bytes,
        &probe,
        comparison_report_path,
        &comparison,
    )?;
    write_json_report_new(workflow_path, &workflow)?;
    Ok((comparison, workflow))
}

fn read_constructed_probe_report(
    probe_report_path: &Path,
) -> Result<(Vec<u8>, ConstructedScenarioProbeReport), String> {
    let probe_bytes = fs::read(probe_report_path).map_err(|error| {
        format!(
            "failed to read constructed probe report '{}': {error}",
            probe_report_path.display()
        )
    })?;
    let probe: ConstructedScenarioProbeReport = serde_json::from_slice(&probe_bytes)
        .map_err(|error| format!("failed to decode constructed probe report: {error}"))?;
    Ok((probe_bytes, probe))
}

fn bind_constructed_comparison_workflow(
    probe_report_path: &Path,
    probe_bytes: &[u8],
    probe: &ConstructedScenarioProbeReport,
    comparison_report_path: &Path,
    comparison: &JavaParityLiveComparisonReportV1,
) -> Result<ConstructedComparisonWorkflowV1, String> {
    let (action_receipt_path, action_receipt) = validate_capture_only_constructed_probe(probe)?;
    let successor = probe.successor.as_ref().unwrap();
    let source = comparison.source();
    let evidence = comparison.evidence_scope();
    let (from_frame_id, to_frame_id) = comparison.frame_ids();
    let source_receipt = fs::canonicalize(&source.action_receipt_path).map_err(|error| {
        format!(
            "failed to resolve comparison action receipt '{}': {error}",
            source.action_receipt_path
        )
    })?;
    let probe_receipt = fs::canonicalize(&action_receipt_path).map_err(|error| {
        format!(
            "failed to resolve probe action receipt '{}': {error}",
            action_receipt_path.display()
        )
    })?;
    let probe_launch =
        probe.session.launch.as_ref().ok_or_else(|| {
            "constructed probe report omitted its typed launch evidence".to_owned()
        })?;
    let comparison_launch = source.launch.as_ref().ok_or_else(|| {
        "constructed comparison report omitted its typed launch evidence".to_owned()
    })?;
    if comparison.status() != "matched"
        || source.transition_origin != "constructed_validated"
        || source.session_id != probe.session.session_id
        || source_receipt != probe_receipt
        || source.action_receipt_sha256 != action_receipt.sha256
        || from_frame_id != probe.root_frame_id
        || to_frame_id != successor.frame_id
        || serde_json::to_value(source.scenario_context.as_ref())
            .map_err(|error| format!("failed to encode comparison scenario context: {error}"))?
            != successor.scenario_context
        || serde_json::to_value(comparison_launch)
            .map_err(|error| format!("failed to encode comparison launch evidence: {error}"))?
            != serde_json::to_value(probe_launch)
                .map_err(|error| format!("failed to encode probe launch evidence: {error}"))?
        || evidence.natural_reachability
        || evidence.strategy_evidence
        || evidence.training_eligible
        || !evidence.constructed_root_verified
        || !evidence.original_game_transition_captured
        || !evidence.simulator_comparison
        || !evidence.narrow_mechanics_parity
        || evidence.strict_runtime_profile != (probe_launch.profile == "strict")
    {
        return Err(
            "comparison report does not bind the exact constructed probe and scoped evidence"
                .to_owned(),
        );
    }
    let scenario = source
        .scenario_context
        .as_ref()
        .ok_or_else(|| "constructed comparison report omitted its scenario context".to_owned())?;
    let comparison_bytes = fs::read(comparison_report_path).map_err(|error| {
        format!(
            "failed to read comparison report '{}': {error}",
            comparison_report_path.display()
        )
    })?;
    let stored_comparison: serde_json::Value = serde_json::from_slice(&comparison_bytes)
        .map_err(|error| format!("failed to decode stored comparison report: {error}"))?;
    let expected_comparison = serde_json::to_value(comparison)
        .map_err(|error| format!("failed to encode in-memory comparison report: {error}"))?;
    if stored_comparison != expected_comparison {
        return Err(
            "stored comparison report does not equal the comparison being bound".to_owned(),
        );
    }
    let comparison_report = file_identity_from_bytes(comparison_report_path, &comparison_bytes)?;
    Ok(ConstructedComparisonWorkflowV1 {
        schema: "sts_java_parity_constructed_comparison_workflow_v1".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        passed: true,
        provenance: "constructed_validated".to_owned(),
        transition_id: probe.transition_id.clone(),
        comparison_kind: comparison.transition_kind().to_owned(),
        session_id: probe.session.session_id.clone(),
        case_id: scenario.case_id.clone(),
        from_frame_id,
        to_frame_id,
        launch_profile: probe_launch.profile.clone(),
        evidence_scope: evidence.clone(),
        probe_report: file_identity_from_bytes(probe_report_path, probe_bytes)?,
        action_receipt,
        comparison_report,
    })
}

fn validate_capture_only_constructed_probe(
    probe: &ConstructedScenarioProbeReport,
) -> Result<(PathBuf, ComparisonFileIdentityV1), String> {
    validate_constructed_probe_plan_identity(probe)?;
    if probe.schema != "sts_java_parity_constructed_probe_report_v2"
        || probe.provenance != "constructed_validated"
        || !probe.passed
        || probe.failure.is_some()
        || !probe.evidence_scope.constructed_root_verified
        || !probe.evidence_scope.original_game_transition_captured
        || probe.evidence_scope.simulator_comparison
        || probe.evidence_scope.natural_reachability
        || probe.evidence_scope.strategy_evidence
        || probe.evidence_scope.training_eligible
    {
        return Err(
            "constructed comparison workflow requires one passing capture-only probe report"
                .to_owned(),
        );
    }
    let action = probe
        .action
        .as_ref()
        .ok_or_else(|| "passing constructed probe omitted its action proof".to_owned())?;
    let successor = probe
        .successor
        .as_ref()
        .ok_or_else(|| "passing constructed probe omitted its successor proof".to_owned())?;
    let action_receipt_path = PathBuf::from(&action.receipt_path);
    let action_receipt = file_identity(&action_receipt_path)?;
    if action.status != "completed"
        || action.successor_frame_id != Some(successor.frame_id)
        || action.based_on_frame_id != probe.root_frame_id
        || action.receipt_sha256 != action_receipt.sha256
    {
        return Err("constructed probe action proof is inconsistent".to_owned());
    }
    Ok((action_receipt_path, action_receipt))
}

fn validate_constructed_probe_plan_identity(
    probe: &ConstructedScenarioProbeReport,
) -> Result<(), String> {
    let plan_bytes = fs::read(&probe.probe_plan_path).map_err(|error| {
        format!(
            "failed to re-read constructed probe plan '{}': {error}",
            probe.probe_plan_path
        )
    })?;
    if sha256_hex(&plan_bytes) != probe.probe_plan_sha256 {
        return Err("constructed probe plan changed after capture".to_owned());
    }
    let plan: ScenarioProbePlan = serde_json::from_slice(&plan_bytes)
        .map_err(|error| format!("failed to decode constructed probe plan: {error}"))?;
    if plan.schema != "sts_java_play_scenario_probe_plan_v2"
        || plan.provenance != "constructed_validated"
        || plan.probe_case_id != probe.probe_case_id
        || plan.transition != probe.transition
        || probe.transition_id != probe.transition.id()
    {
        return Err("constructed probe report disagrees with its bound typed plan".to_owned());
    }
    Ok(())
}

fn file_identity(path: &Path) -> Result<ComparisonFileIdentityV1, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read workflow input '{}': {error}",
            path.display()
        )
    })?;
    file_identity_from_bytes(path, &bytes)
}

fn file_identity_from_bytes(path: &Path, bytes: &[u8]) -> Result<ComparisonFileIdentityV1, String> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "failed to resolve workflow input '{}': {error}",
            path.display()
        )
    })?;
    Ok(ComparisonFileIdentityV1 {
        path: canonical.to_string_lossy().into_owned(),
        bytes: u64::try_from(bytes.len())
            .map_err(|_| "workflow input length does not fit u64".to_owned())?,
        sha256: sha256_hex(bytes),
    })
}

fn current_unix_ms() -> Result<u128, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

fn load_completed_live_transition(
    action_receipt_path: &Path,
) -> Result<LoadedLiveTransition, String> {
    let files = load_completed_live_frame_files(action_receipt_path)?;
    parse_completed_live_transition(files)
}

fn parse_completed_live_transition(
    files: LoadedLiveFrameFiles,
) -> Result<LoadedLiveTransition, String> {
    let before: StateFrameProjection = serde_json::from_slice(&files.before_bytes)
        .map_err(|error| format!("failed to decode predecessor combat frame: {error}"))?;
    let after: StateFrameProjection = serde_json::from_slice(&files.after_bytes)
        .map_err(|error| format!("failed to decode successor combat frame: {error}"))?;
    Ok(LoadedLiveTransition {
        manifest: files.manifest,
        receipt: files.receipt,
        manifest_path: files.manifest_path,
        receipt_path: files.receipt_path,
        before_path: files.before_path,
        after_path: files.after_path,
        manifest_sha256: files.manifest_sha256,
        receipt_sha256: files.receipt_sha256,
        before_sha256: files.before_sha256,
        after_sha256: files.after_sha256,
        before,
        after,
    })
}

fn parse_completed_feed_terminal_transition(
    files: LoadedLiveFrameFiles,
) -> Result<LoadedFeedTerminalTransition, String> {
    let before: StateFrameProjection = serde_json::from_slice(&files.before_bytes)
        .map_err(|error| format!("failed to decode predecessor combat frame: {error}"))?;
    let after: RunDecisionStateFrameProjection = serde_json::from_slice(&files.after_bytes)
        .map_err(|error| format!("failed to decode successor run-decision frame: {error}"))?;
    Ok(LoadedFeedTerminalTransition {
        files,
        before,
        after,
    })
}

fn load_completed_live_frame_files(
    action_receipt_path: &Path,
) -> Result<LoadedLiveFrameFiles, String> {
    let receipt_path = fs::canonicalize(action_receipt_path).map_err(|error| {
        format!(
            "failed to resolve live action receipt '{}': {error}",
            action_receipt_path.display()
        )
    })?;
    let receipts_dir = receipt_path
        .parent()
        .ok_or_else(|| "live action receipt has no parent directory".to_owned())?;
    if receipts_dir.file_name().and_then(|name| name.to_str()) != Some("receipts") {
        return Err("live action receipt is not inside a receipts directory".to_owned());
    }
    let session_dir = receipts_dir
        .parent()
        .ok_or_else(|| "live action receipt has no session directory".to_owned())?;
    let manifest_path = session_dir.join("session.json");
    let receipt_bytes = fs::read(&receipt_path).map_err(|error| {
        format!(
            "failed to read live action receipt '{}': {error}",
            receipt_path.display()
        )
    })?;
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
        format!(
            "failed to read live session manifest '{}': {error}",
            manifest_path.display()
        )
    })?;
    let receipt: ActionReceipt = serde_json::from_slice(&receipt_bytes)
        .map_err(|error| format!("failed to decode live action receipt: {error}"))?;
    let manifest: SessionManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("failed to decode live session manifest: {error}"))?;
    let version_pair = matches!(
        (receipt.schema.as_str(), manifest.schema.as_str()),
        (
            "sts_java_parity_live_action_receipt_v1",
            "sts_java_parity_live_session_v1"
        ) | (
            "sts_java_parity_live_action_receipt_v2",
            "sts_java_parity_live_session_v2"
        ) | (
            "sts_java_parity_live_action_receipt_v2",
            "sts_java_parity_live_session_v3"
        )
    );
    if !version_pair
        || receipt.status != "completed"
        || receipt.code.is_some()
        || receipt.successor_frame_id != receipt.based_on_frame_id.checked_add(1)
    {
        return Err(
            "live action receipt is not one completed adjacent supported-version action".to_owned(),
        );
    }
    if !matches!(
        manifest.profile.as_str(),
        CONTROL_PROFILE | RUN_CONTROL_PROFILE | SCENARIO_CONTROL_PROFILE
    ) || receipt.session_id != manifest.session_id
        || receipt.connection_id != manifest.connection_id
    {
        return Err("live action receipt and session manifest identities disagree".to_owned());
    }
    if !receipt
        .parameters
        .as_object()
        .is_some_and(serde_json::Map::is_empty)
    {
        return Err("scoped combat comparison requires empty action parameters".to_owned());
    }
    let manifest_session_dir = fs::canonicalize(&manifest.session_dir).map_err(|error| {
        format!(
            "failed to resolve manifest session directory '{}': {error}",
            manifest.session_dir
        )
    })?;
    if manifest_session_dir != session_dir {
        return Err("manifest session directory does not own the action receipt".to_owned());
    }
    validate_accepted_producer_result(&receipt)?;

    let before_path = session_dir
        .join("frames")
        .join(format!("frame-{:020}.json", receipt.based_on_frame_id));
    let successor_frame_id = receipt
        .successor_frame_id
        .ok_or_else(|| "completed action receipt has no successor frame ID".to_owned())?;
    let after_path = session_dir
        .join("frames")
        .join(format!("frame-{successor_frame_id:020}.json"));
    let advertised_after = receipt
        .successor_frame_path
        .as_ref()
        .ok_or_else(|| "completed action receipt has no successor frame path".to_owned())?;
    let resolved_after = fs::canonicalize(advertised_after).map_err(|error| {
        format!(
            "failed to resolve advertised successor frame '{}': {error}",
            advertised_after
        )
    })?;
    let after_path = fs::canonicalize(&after_path).map_err(|error| {
        format!(
            "failed to resolve expected successor frame '{}': {error}",
            after_path.display()
        )
    })?;
    if resolved_after != after_path {
        return Err(
            "receipt successor frame path escapes or disagrees with its session".to_owned(),
        );
    }
    let before_path = fs::canonicalize(&before_path).map_err(|error| {
        format!(
            "failed to resolve predecessor frame '{}': {error}",
            before_path.display()
        )
    })?;
    let before_bytes = fs::read(&before_path)
        .map_err(|error| format!("failed to read predecessor frame: {error}"))?;
    let after_bytes = fs::read(&after_path)
        .map_err(|error| format!("failed to read successor frame: {error}"))?;
    Ok(LoadedLiveFrameFiles {
        manifest,
        receipt,
        manifest_path,
        receipt_path,
        before_path,
        after_path,
        manifest_sha256: sha256_hex(&manifest_bytes),
        receipt_sha256: sha256_hex(&receipt_bytes),
        before_sha256: sha256_hex(&before_bytes),
        after_sha256: sha256_hex(&after_bytes),
        before_bytes,
        after_bytes,
    })
}

fn validate_accepted_producer_result(receipt: &ActionReceipt) -> Result<(), String> {
    let result = receipt
        .producer_result
        .as_ref()
        .ok_or_else(|| "completed live action receipt has no producer result".to_owned())?;
    let matches = result.get("kind").and_then(serde_json::Value::as_str) == Some("action_result")
        && result.get("status").and_then(serde_json::Value::as_str) == Some("accepted")
        && result
            .get("connection_id")
            .and_then(serde_json::Value::as_str)
            == Some(receipt.connection_id.as_str())
        && result.get("request_id").and_then(serde_json::Value::as_str)
            == Some(receipt.request_id.as_str())
        && result
            .get("based_on_frame_id")
            .and_then(serde_json::Value::as_u64)
            == Some(receipt.based_on_frame_id)
        && result.get("action_id").and_then(serde_json::Value::as_str)
            == Some(receipt.action_id.as_str());
    if !matches {
        return Err("producer result does not accept the exact receipted action".to_owned());
    }
    Ok(())
}

fn validate_live_adjacent_frames(loaded: &LoadedLiveTransition) -> Result<(), String> {
    validate_live_frame_pair(loaded)?;
    validate_live_same_turn(loaded)
}

fn validate_live_frame_pair(loaded: &LoadedLiveTransition) -> Result<(), String> {
    let before = &loaded.before;
    let after = &loaded.after;
    for frame in [before, after] {
        let actual_controls: BTreeSet<&str> =
            frame.grants.control.iter().map(String::as_str).collect();
        let current_controls: BTreeSet<&str> = match loaded.manifest.profile.as_str() {
            CONTROL_PROFILE => COMBAT_CONTROL_GRANTS.into_iter().collect(),
            RUN_CONTROL_PROFILE => RUN_CONTROL_GRANTS.into_iter().collect(),
            SCENARIO_CONTROL_PROFILE => SCENARIO_CONTROL_GRANTS.into_iter().collect(),
            _ => return Err("live session has an unsupported control profile".to_owned()),
        };
        let legacy_run_controls: BTreeSet<&str> = [
            "run_start",
            "run_choose_event",
            "run_choose_map_node",
            "run_claim_combat_reward",
            "run_close_combat_reward",
            "run_acknowledge_ftue",
            "combat_play_card",
            "combat_end_turn",
        ]
        .into_iter()
        .collect();
        let grants_supported = actual_controls == current_controls
            || (loaded.manifest.profile == RUN_CONTROL_PROFILE
                && actual_controls == legacy_run_controls);
        if frame.kind != "state_frame"
            || frame.capture_id != loaded.manifest.capture_id
            || frame.producer_session_id.as_deref()
                != Some(loaded.manifest.producer_session_id.as_str())
            || frame.connection_id.as_deref() != Some(loaded.manifest.connection_id.as_str())
            || frame.profile != loaded.manifest.profile
            || !grants_supported
            || frame.grants.control.len() != actual_controls.len()
            || frame.boundary.kind != "combat_player_decision"
            || !frame.boundary.stable
        {
            return Err(
                "live frame identity, grants, or stable boundary is inconsistent".to_owned(),
            );
        }
        validate_frame_zone_consistency(frame)?;
    }
    if before.frame_id != loaded.receipt.based_on_frame_id
        || after.frame_id != loaded.receipt.successor_frame_id.unwrap_or_default()
        || after.previous_frame_id != Some(before.frame_id)
    {
        return Err(
            "live receipt does not bind one adjacent predecessor/successor pair".to_owned(),
        );
    }
    if before.public_base.run != after.public_base.run
        || before.public_base.run.character != "IRONCLAD"
    {
        return Err("live action crosses a run or character boundary".to_owned());
    }
    live_transition_origin(loaded)?;
    Ok(())
}

fn validate_live_same_turn(loaded: &LoadedLiveTransition) -> Result<(), String> {
    if loaded.before.public_base.combat.turn != loaded.after.public_base.combat.turn {
        return Err("live card action crosses a turn boundary".to_owned());
    }
    Ok(())
}

fn validate_cultist_turn_one_original_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
) -> Result<(), String> {
    let before_combat = &before.public_base.combat;
    let after_combat = &after.public_base.combat;
    if before.public_base.run.ascension != 20
        || before.public_base.run.act != 1
        || before.public_base.run.floor != 1
        || before_combat.turn != 1
        || after_combat.turn != 2
    {
        return Err(
            "end-turn comparison is scoped to A20 Act 1 floor 1 Cultist turn one".to_owned(),
        );
    }
    if before_combat.monsters.len() != 1
        || after_combat.monsters.len() != 1
        || before_combat.monsters[0].id != "Cultist"
        || before_combat.monsters[0] != after_combat.monsters[0]
        || before_combat.monsters[0].dead_or_escaped
    {
        return Err("Cultist turn-one public monster transition is outside scope".to_owned());
    }
    let before_internal = only_typed_monster(before)?;
    let after_internal = only_typed_monster(after)?;
    if before_internal.instance_id != before_combat.monsters[0].instance_id
        || after_internal.instance_id != after_combat.monsters[0].instance_id
        || before_internal.next_move_id != Some(3)
        || before_internal.intent.as_deref() != Some("BUFF")
        || before_internal.move_history != [3]
        || !before_internal.powers.is_empty()
        || after_internal.next_move_id != Some(1)
        || after_internal.intent.as_deref() != Some("ATTACK")
        || after_internal.move_history != [3, 1]
        || after_internal.powers.len() != 1
    {
        return Err("typed Cultist move/power transition is outside scope".to_owned());
    }
    let ritual = &after_internal.powers[0];
    if ritual.index != 0
        || ritual.id != "Ritual"
        || ritual.class_name != "com.megacrit.cardcrawl.powers.RitualPower"
        || ritual.amount != 5
        || ritual.power_type != "BUFF"
        || ritual.ritual_skip_first != Some(false)
        || ritual.ritual_player_controlled != Some(false)
    {
        return Err("typed Java Cultist Ritual power instance is inconsistent".to_owned());
    }
    if after_combat.player.current_hp != before_combat.player.current_hp
        || after_combat.player.max_hp != before_combat.player.max_hp
        || after_combat.player.block != 0
        || after_combat.player.energy != 3
    {
        return Err(
            "Cultist Ritual turn does not have the expected player reset projection".to_owned(),
        );
    }
    if !before.parity_internal.zones.limbo.cards.is_empty()
        || !after.parity_internal.zones.limbo.cards.is_empty()
    {
        return Err("end-turn comparison does not cover a non-empty limbo zone".to_owned());
    }
    let before_cards = card_identity_multiset(&before.parity_internal.zones);
    let after_cards = card_identity_multiset(&after.parity_internal.zones);
    if before_cards != after_cards {
        return Err("card identity multiset changed across the end-turn transition".to_owned());
    }
    validate_rng_owners(&before.rng_internal.streams)?;
    validate_rng_owners(&after.rng_internal.streams)?;
    let before_ai = rng_stream(&before.rng_internal.streams, AI_RNG)?;
    let after_ai = rng_stream(&after.rng_internal.streams, AI_RNG)?;
    if after_ai.counter != before_ai.counter + 1 {
        return Err("Cultist next-intent roll did not consume exactly one aiRng draw".to_owned());
    }
    Ok(())
}

fn run_cultist_turn_one_end_turn_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
) -> Result<EndTurnProjectionV1, String> {
    run_cultist_turn_one_end_turn_transition_with_letter_opener(before, after, None)
        .map(|(projection, _)| projection)
}

fn run_cultist_turn_one_end_turn_transition_with_letter_opener(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    letter_opener_counter: Option<i32>,
) -> Result<(EndTurnProjectionV1, Option<i32>), String> {
    let zones = &before.parity_internal.zones;
    let identities = compile_captured_scenario_bindings(zones)?;
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.meta.ascension_level = before.public_base.run.ascension;
    combat.turn.turn_count = before.public_base.combat.turn;
    combat.turn.set_energy(
        u8::try_from(before.public_base.combat.player.energy)
            .map_err(|_| "captured energy does not fit simulator u8".to_owned())?,
    );
    combat.entities.player.current_hp = before.public_base.combat.player.current_hp;
    combat.entities.player.max_hp = before.public_base.combat.player.max_hp;
    combat.entities.player.block = before.public_base.combat.player.block;
    combat.zones.hand = build_cards(&zones.hand.cards, &identities)?;
    combat.zones.discard_pile = build_cards(&zones.discard.cards, &identities)?.into();
    combat.zones.exhaust_pile = build_cards(&zones.exhaust.cards, &identities)?.into();
    combat.zones.limbo = build_cards(&zones.limbo.cards, &identities)?;
    let mut rust_draw = build_cards(&zones.draw.cards, &identities)?;
    rust_draw.reverse();
    combat.zones.draw_pile = rust_draw.into();

    let captured = &before.public_base.combat.monsters[0];
    let mut cultist = sts_core::test_support::test_monster(EnemyId::Cultist);
    cultist.id = usize::try_from(captured.instance_id)
        .map_err(|_| "captured monster instance ID does not fit simulator usize".to_owned())?;
    cultist.slot = u8::try_from(captured.index)
        .map_err(|_| "captured monster index does not fit simulator slot".to_owned())?;
    cultist.current_hp = captured.current_hp;
    cultist.max_hp = captured.max_hp;
    cultist.block = captured.block;
    cultist.set_planned_move_id(3);
    cultist.move_history_mut().push_back(3);
    cultist.cultist.first_move = false;
    combat.entities.monsters = vec![cultist];
    if let Some(counter) = letter_opener_counter {
        let mut relic = RelicState::new(RelicId::LetterOpener);
        relic.counter = counter;
        combat.entities.player.add_relic(relic);
    }
    inject_rng_streams(&mut combat.rng.pool, &before.rng_internal.streams)?;
    combat.update_hand_cards();

    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &ClientInput::EndTurn) {
        return Err("reconstructed end turn is not legal in the Rust stepper".to_owned());
    }
    let step = stepper.apply_to_stable(
        &position,
        ClientInput::EndTurn,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err("Rust Cultist end turn did not reach a finite stable boundary".to_owned());
    }
    let reached = &step.position.combat;
    let reached_zones = project_zones(reached, &identities)?;
    if reached_zones != after.parity_internal.zones.as_card_zones() {
        return Err("Rust end-turn hand/discard/exhaust/draw zones do not match Java".to_owned());
    }
    if reached.turn.turn_count != after.public_base.combat.turn
        || reached.turn.energy as i32 != after.public_base.combat.player.energy
        || reached.entities.player.current_hp != after.public_base.combat.player.current_hp
        || reached.entities.player.max_hp != after.public_base.combat.player.max_hp
        || reached.entities.player.block != after.public_base.combat.player.block
    {
        return Err("Rust end-turn player/turn reset does not match Java".to_owned());
    }
    let reached_monster = reached
        .entities
        .monsters
        .first()
        .ok_or_else(|| "Rust end-turn successor lost the Cultist".to_owned())?;
    let after_monster = &after.public_base.combat.monsters[0];
    if reached.entities.monsters.len() != 1
        || reached_monster.id != usize::try_from(after_monster.instance_id).unwrap_or(usize::MAX)
        || reached_monster.current_hp != after_monster.current_hp
        || reached_monster.max_hp != after_monster.max_hp
        || reached_monster.block != after_monster.block
        || reached_monster.is_dying != after_monster.dead_or_escaped
        || reached_monster.is_escaped != after_monster.dead_or_escaped
    {
        return Err("Rust Cultist public successor does not match Java".to_owned());
    }
    if reached_monster.planned_move_id() != 1
        || reached_monster
            .move_history()
            .iter()
            .copied()
            .collect::<Vec<_>>()
            != [3, 1]
    {
        return Err("Rust Cultist planned move/history does not match Java".to_owned());
    }
    let powers = sts_core::content::powers::store::powers_snapshot_for(reached, reached_monster.id);
    if powers.len() != 1
        || powers[0].power_type != PowerId::Ritual
        || powers[0].amount != 5
        || powers[0].extra_data != sts_core::content::powers::core::ritual::extra_data(false, false)
    {
        return Err("Rust Cultist Ritual power instance does not match Java".to_owned());
    }
    let rust_rng = project_rng_streams(&reached.rng.pool);
    if rust_rng != after.rng_internal.streams {
        for owner in [
            AI_RNG,
            SHUFFLE_RNG,
            CARD_RANDOM_RNG,
            MISC_RNG,
            MONSTER_HP_RNG,
            POTION_RNG,
        ] {
            let rust = rng_stream(&rust_rng, owner)?;
            let java = rng_stream(&after.rng_internal.streams, owner)?;
            if rust != java {
                return Err(format!(
                    "Rust end-turn RNG successor diverges for {owner}: Rust counter={} state={:?}; Java counter={} state={:?}",
                    rust.counter, rust.state_hex, java.counter, java.state_hex
                ));
            }
        }
        return Err("Rust end-turn RNG successor ordering does not match Java".to_owned());
    }
    let reached_letter_opener_counter = reached
        .entities
        .player
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::LetterOpener)
        .map(|relic| relic.counter);
    Ok((
        project_end_turn_state(before, reached, &rust_rng)?,
        reached_letter_opener_counter,
    ))
}

fn project_end_turn_frames(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
) -> Result<EndTurnProjectionV1, String> {
    Ok(EndTurnProjectionV1 {
        turn_before: before.public_base.combat.turn,
        turn_after: after.public_base.combat.turn,
        player_hp_before: before.public_base.combat.player.current_hp,
        player_hp_after: after.public_base.combat.player.current_hp,
        player_block_before: before.public_base.combat.player.block,
        player_block_after: after.public_base.combat.player.block,
        energy_before: before.public_base.combat.player.energy,
        energy_after: after.public_base.combat.player.energy,
        monster_hp_before: before.public_base.combat.monsters[0].current_hp,
        monster_hp_after: after.public_base.combat.monsters[0].current_hp,
        hand_count_before: before.parity_internal.zones.hand.cards.len(),
        hand_count_after: after.parity_internal.zones.hand.cards.len(),
        draw_count_before: before.parity_internal.zones.draw.cards.len(),
        draw_count_after: after.parity_internal.zones.draw.cards.len(),
        discard_count_before: before.parity_internal.zones.discard.cards.len(),
        discard_count_after: after.parity_internal.zones.discard.cards.len(),
        exhaust_count_before: before.parity_internal.zones.exhaust.cards.len(),
        exhaust_count_after: after.parity_internal.zones.exhaust.cards.len(),
        rng_transitions: project_rng_transitions(
            &before.rng_internal.streams,
            &after.rng_internal.streams,
        )?,
    })
}

fn project_end_turn_state(
    before: &StateFrameProjection,
    reached: &sts_core::runtime::combat::CombatState,
    reached_rng: &[RngStreamProjection],
) -> Result<EndTurnProjectionV1, String> {
    Ok(EndTurnProjectionV1 {
        turn_before: before.public_base.combat.turn,
        turn_after: reached.turn.turn_count,
        player_hp_before: before.public_base.combat.player.current_hp,
        player_hp_after: reached.entities.player.current_hp,
        player_block_before: before.public_base.combat.player.block,
        player_block_after: reached.entities.player.block,
        energy_before: before.public_base.combat.player.energy,
        energy_after: reached.turn.energy as i32,
        monster_hp_before: before.public_base.combat.monsters[0].current_hp,
        monster_hp_after: reached.entities.monsters[0].current_hp,
        hand_count_before: before.parity_internal.zones.hand.cards.len(),
        hand_count_after: reached.zones.hand.len(),
        draw_count_before: before.parity_internal.zones.draw.cards.len(),
        draw_count_after: reached.zones.draw_pile.len(),
        discard_count_before: before.parity_internal.zones.discard.cards.len(),
        discard_count_after: reached.zones.discard_pile.len(),
        exhaust_count_before: before.parity_internal.zones.exhaust.cards.len(),
        exhaust_count_after: reached.zones.exhaust_pile.len(),
        rng_transitions: project_rng_transitions(&before.rng_internal.streams, reached_rng)?,
    })
}

fn project_rng_transitions(
    before: &[RngStreamProjection],
    after: &[RngStreamProjection],
) -> Result<Vec<RngTransitionProjectionV1>, String> {
    validate_rng_owners(before)?;
    validate_rng_owners(after)?;
    [
        AI_RNG,
        SHUFFLE_RNG,
        CARD_RANDOM_RNG,
        MISC_RNG,
        MONSTER_HP_RNG,
        POTION_RNG,
    ]
    .into_iter()
    .map(|owner| {
        let before = rng_stream(before, owner)?;
        let after = rng_stream(after, owner)?;
        Ok(RngTransitionProjectionV1 {
            owner: owner.to_owned(),
            counter_before: before.counter,
            counter_after: after.counter,
            state_changed: before != after,
        })
    })
    .collect()
}

fn rng_stream<'a>(
    streams: &'a [RngStreamProjection],
    owner: &str,
) -> Result<&'a RngStreamProjection, String> {
    let mut matches = streams.iter().filter(|stream| stream.owner == owner);
    let stream = matches
        .next()
        .ok_or_else(|| format!("missing RNG stream: {owner}"))?;
    if matches.next().is_some() {
        return Err(format!("duplicate RNG stream: {owner}"));
    }
    Ok(stream)
}

fn only_typed_monster(frame: &StateFrameProjection) -> Result<&MonsterInternalProjection, String> {
    let monsters = frame
        .parity_internal
        .monsters
        .as_ref()
        .ok_or_else(|| "frame does not expose typed parity_internal.monsters".to_owned())?;
    if monsters.len() != 1 {
        return Err("typed end-turn scope requires exactly one internal monster".to_owned());
    }
    Ok(&monsters[0])
}

fn card_identity_multiset(zones: &ParityZonesProjection) -> BTreeMap<String, CardProjection> {
    zones
        .draw
        .cards
        .iter()
        .chain(&zones.hand.cards)
        .chain(&zones.discard.cards)
        .chain(&zones.exhaust.cards)
        .chain(&zones.limbo.cards)
        .map(|card| (card.instance_uuid.clone(), card.clone()))
        .collect()
}

fn receipted_action(loaded: &LoadedLiveTransition) -> Result<&ActionProjection, String> {
    receipted_action_from_frame(&loaded.before, &loaded.receipt)
}

fn receipted_action_from_frame<'a>(
    before: &'a StateFrameProjection,
    receipt: &ActionReceipt,
) -> Result<&'a ActionProjection, String> {
    let action_space = before
        .action_space
        .as_ref()
        .ok_or_else(|| "predecessor frame has no typed action space".to_owned())?;
    if action_space.kind != "combat_action_space" {
        return Err("predecessor action space has an unsupported kind".to_owned());
    }
    let mut matches = action_space
        .actions
        .iter()
        .filter(|action| action.action_id == receipt.action_id);
    let action = matches
        .next()
        .ok_or_else(|| "receipted action is absent from the predecessor action space".to_owned())?;
    if matches.next().is_some() {
        return Err("receipted action is duplicated in the predecessor action space".to_owned());
    }
    Ok(action)
}

fn infer_base_defend_live_transition(
    loaded: &LoadedLiveTransition,
) -> Result<CardProjection, String> {
    let action = receipted_action(loaded)?;
    if action.kind != "play_card"
        || action.card_id.as_deref() != Some(BASE_DEFEND_JAVA_ID)
        || action.target_monster_instance_id.is_some()
        || action.target_monster_index.is_some()
        || action.target_monster_id.is_some()
    {
        return Err("receipted action is not a typed untargeted base Defend play".to_owned());
    }
    let moved = infer_base_defend_transition(&loaded.before, &loaded.after)?;
    if action.card_uuid.as_deref() != Some(moved.instance_uuid.as_str()) {
        return Err("typed Defend UUID does not match the uniquely moved hand card".to_owned());
    }
    Ok(moved)
}

struct WhirlwindRelicContext {
    observed_relic_ids: Vec<String>,
    chemical_x_bonus_repetitions: i32,
}

fn whirlwind_relic_context(loaded: &LoadedLiveTransition) -> Result<WhirlwindRelicContext, String> {
    if loaded.before.parity_internal.relics != loaded.after.parity_internal.relics {
        return Err("relic identity projection changed while playing Whirlwind".to_owned());
    }
    let relics = &loaded.before.parity_internal.relics;
    if relics
        .iter()
        .enumerate()
        .any(|(index, relic)| relic.index != index || relic.relic_id.is_empty())
        || relics
            .iter()
            .map(|relic| relic.relic_id.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            != relics.len()
    {
        return Err("Whirlwind relic identity projection is not exact and ordered".to_owned());
    }
    let observed_relic_ids = relics
        .iter()
        .map(|relic| relic.relic_id.clone())
        .collect::<Vec<_>>();
    let Some(recipe) = loaded.before.scenario.as_ref() else {
        if !observed_relic_ids.is_empty() && observed_relic_ids.as_slice() != ["Burning Blood"] {
            return Err(
                "unclassified Whirlwind scope requires no projected relic modifier".to_owned(),
            );
        }
        return Ok(WhirlwindRelicContext {
            observed_relic_ids,
            chemical_x_bonus_repetitions: 0,
        });
    };
    let declared = recipe.capabilities.iter().find_map(|capability| {
        if let ScenarioCapability::FreshRelics(spec) = capability {
            Some(
                spec.relics
                    .iter()
                    .map(|relic| relic.relic_id.clone())
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        }
    });

    match declared {
        Some(expected) => {
            if observed_relic_ids != expected {
                return Err(
                    "observed Whirlwind relic identities disagree with the bound fresh exact capability"
                        .to_owned(),
                );
            }
            let chemical_x_bonus_repetitions = match expected.as_slice() {
                [] => 0,
                [only] if only == "Chemical X" => 2,
                _ => return Err(
                    "Whirlwind comparator covers only an empty fresh relic set or exact Chemical X"
                        .to_owned(),
                ),
            };
            Ok(WhirlwindRelicContext {
                observed_relic_ids,
                chemical_x_bonus_repetitions,
            })
        }
        None => {
            let legacy_without_projection = recipe.capability_registry
                == "communicationmod.parity-combat-capabilities.v1"
                && observed_relic_ids.is_empty();
            if !legacy_without_projection && observed_relic_ids.as_slice() != ["Burning Blood"] {
                return Err(
                    "ordinary Ironclad Whirlwind scope requires only the unchanged starter relic"
                        .to_owned(),
                );
            }
            Ok(WhirlwindRelicContext {
                observed_relic_ids,
                chemical_x_bonus_repetitions: 0,
            })
        }
    }
}

fn infer_whirlwind_transition(
    loaded: &LoadedLiveTransition,
    relic_context: &WhirlwindRelicContext,
) -> Result<CardProjection, String> {
    let action = receipted_action(loaded)?;
    if action.kind != "play_card"
        || action.card_id.as_deref() != Some(WHIRLWIND_JAVA_ID)
        || action.target_monster_instance_id.is_some()
        || action.target_monster_index.is_some()
        || action.target_monster_id.is_some()
    {
        return Err("receipted action is not a typed untargeted Whirlwind play".to_owned());
    }
    if !loaded
        .receipt
        .parameters
        .as_object()
        .is_some_and(serde_json::Map::is_empty)
    {
        return Err("receipted Whirlwind action has caller-supplied parameters".to_owned());
    }

    let before_combat = &loaded.before.public_base.combat;
    let after_combat = &loaded.after.public_base.combat;
    let after_hand = unique_cards_by_uuid(&after_combat.hand)?;
    let moved: Vec<&CardProjection> = before_combat
        .hand
        .iter()
        .filter(|card| !after_hand.contains_key(card.instance_uuid.as_str()))
        .collect();
    if moved.len() != 1
        || before_combat.hand.len() != after_combat.hand.len() + 1
        || action.card_uuid.as_deref() != Some(moved[0].instance_uuid.as_str())
    {
        return Err(
            "transition does not move the receipted Whirlwind uniquely out of hand".to_owned(),
        );
    }
    for card in &after_combat.hand {
        let previous = before_combat
            .hand
            .iter()
            .find(|candidate| candidate.instance_uuid == card.instance_uuid)
            .ok_or_else(|| "Whirlwind transition introduces an unexpected hand card".to_owned())?;
        if previous != card {
            return Err(
                "an untouched hand card changed during the Whirlwind transition".to_owned(),
            );
        }
    }
    let moved = moved[0];
    let mut expected_discard = loaded.before.parity_internal.zones.discard.cards.clone();
    expected_discard.push(moved.clone());
    if loaded.after.parity_internal.zones.discard.cards != expected_discard
        || loaded.before.parity_internal.zones.draw.cards
            != loaded.after.parity_internal.zones.draw.cards
        || loaded.before.parity_internal.zones.exhaust.cards
            != loaded.after.parity_internal.zones.exhaust.cards
        || loaded.before.parity_internal.zones.limbo.cards
            != loaded.after.parity_internal.zones.limbo.cards
    {
        return Err(
            "card zones do not describe one ordinary Whirlwind hand-to-discard play".to_owned(),
        );
    }
    if moved.card_id != WHIRLWIND_JAVA_ID || moved.upgrades != 0 || moved.cost_for_turn != -1 {
        return Err("the moved card is not one ordinary unupgraded Whirlwind".to_owned());
    }

    let committed = before_combat.player.energy;
    if committed <= 0 {
        return Err("scoped Whirlwind comparison requires positive predecessor energy".to_owned());
    }
    let effect_repetitions = committed
        .checked_add(relic_context.chemical_x_bonus_repetitions)
        .ok_or_else(|| "Whirlwind effect repetition count overflowed i32".to_owned())?;
    let total_damage = effect_repetitions
        .checked_mul(5)
        .ok_or_else(|| "Whirlwind total damage overflowed i32".to_owned())?;
    if after_combat.player.energy != 0
        || after_combat.player.current_hp != before_combat.player.current_hp
        || after_combat.player.max_hp != before_combat.player.max_hp
        || after_combat.player.block != before_combat.player.block
    {
        return Err(
            "ordinary Whirlwind did not consume all current energy or changed player state"
                .to_owned(),
        );
    }
    if before_combat.monsters.is_empty()
        || before_combat.monsters.len() != after_combat.monsters.len()
    {
        return Err(
            "scoped Whirlwind transition requires a stable nonempty monster set".to_owned(),
        );
    }
    for (before, after) in before_combat.monsters.iter().zip(&after_combat.monsters) {
        if before.index != after.index
            || before.instance_id != after.instance_id
            || before.id != after.id
            || before.max_hp != after.max_hp
            || before.block != 0
            || after.block != 0
            || before.dead_or_escaped
            || after.dead_or_escaped
            || before.current_hp <= total_damage
            || after.current_hp != before.current_hp - total_damage
        {
            return Err(
                "monster projection is not plain nonlethal unblocked Whirlwind total damage"
                    .to_owned(),
            );
        }
    }
    if loaded.before.rng_internal.streams.len() != 6
        || loaded.before.rng_internal.streams != loaded.after.rng_internal.streams
    {
        return Err("the six captured RNG streams changed while playing Whirlwind".to_owned());
    }
    validate_rng_owners(&loaded.before.rng_internal.streams)?;
    Ok(moved.clone())
}

fn infer_base_strike_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    action_id: &str,
) -> Result<TargetedCardPlay, String> {
    let action_space = before
        .action_space
        .as_ref()
        .ok_or_else(|| "predecessor frame has no typed action space".to_owned())?;
    if action_space.kind != "combat_action_space" {
        return Err("predecessor action space has an unsupported kind".to_owned());
    }
    let matches: Vec<&ActionProjection> = action_space
        .actions
        .iter()
        .filter(|action| action.action_id == action_id)
        .collect();
    if matches.len() != 1 {
        return Err("receipted action is not unique in the predecessor action space".to_owned());
    }
    let action = matches[0];
    let card_uuid = action
        .card_uuid
        .as_deref()
        .ok_or_else(|| "receipted play_card action has no card UUID".to_owned())?;
    let target_instance_id = action
        .target_monster_instance_id
        .ok_or_else(|| "receipted Strike action has no target monster".to_owned())?;
    if action.kind != "play_card" || action.card_id.as_deref() != Some(BASE_STRIKE_JAVA_ID) {
        return Err("receipted action is not a typed base Strike play".to_owned());
    }

    let before_combat = &before.public_base.combat;
    let after_combat = &after.public_base.combat;
    let after_hand = unique_cards_by_uuid(&after_combat.hand)?;
    let moved: Vec<&CardProjection> = before_combat
        .hand
        .iter()
        .filter(|card| !after_hand.contains_key(card.instance_uuid.as_str()))
        .collect();
    if moved.len() != 1
        || before_combat.hand.len() != after_combat.hand.len() + 1
        || moved[0].instance_uuid != card_uuid
    {
        return Err("transition does not move the receipted card uniquely out of hand".to_owned());
    }
    for card in &after_combat.hand {
        let previous = before_combat
            .hand
            .iter()
            .find(|candidate| candidate.instance_uuid == card.instance_uuid)
            .ok_or_else(|| "transition introduces an unexpected hand card".to_owned())?;
        if previous != card {
            return Err("an untouched hand card changed during the Strike transition".to_owned());
        }
    }
    let moved = moved[0];
    let mut expected_discard = before.parity_internal.zones.discard.cards.clone();
    expected_discard.push(moved.clone());
    if after.parity_internal.zones.discard.cards != expected_discard
        || before.parity_internal.zones.draw.cards != after.parity_internal.zones.draw.cards
        || before.parity_internal.zones.exhaust.cards != after.parity_internal.zones.exhaust.cards
        || before.parity_internal.zones.limbo.cards != after.parity_internal.zones.limbo.cards
    {
        return Err(
            "card zones do not describe one ordinary Strike hand-to-discard play".to_owned(),
        );
    }
    if moved.card_id != BASE_STRIKE_JAVA_ID || moved.upgrades != 0 || moved.cost_for_turn != 1 {
        return Err("the moved card is not one ordinary unupgraded Ironclad Strike".to_owned());
    }
    if after_combat.player.energy != before_combat.player.energy - 1
        || after_combat.player.current_hp != before_combat.player.current_hp
        || after_combat.player.max_hp != before_combat.player.max_hp
        || after_combat.player.block != before_combat.player.block
    {
        return Err("original-game Strike changed player state beyond energy -1".to_owned());
    }
    if before_combat.monsters.len() != after_combat.monsters.len() {
        return Err("monster count changed during the scoped Strike transition".to_owned());
    }
    let target_index = before_combat
        .monsters
        .iter()
        .position(|monster| monster.instance_id == target_instance_id)
        .ok_or_else(|| "typed Strike target is absent from the predecessor monsters".to_owned())?;
    let target_before = &before_combat.monsters[target_index];
    let target_after = after_combat
        .monsters
        .get(target_index)
        .ok_or_else(|| "typed Strike target index is absent from the successor".to_owned())?;
    if action.target_monster_index != Some(target_index)
        || action.target_monster_id.as_deref() != Some(target_before.id.as_str())
        || target_before.index != target_index
        || target_after.index != target_index
        || target_after.instance_id != target_before.instance_id
        || target_after.id != target_before.id
        || target_before.block != 0
        || target_after.block != 0
        || target_after.current_hp != target_before.current_hp - 6
        || target_after.max_hp != target_before.max_hp
        || target_after.dead_or_escaped != target_before.dead_or_escaped
    {
        return Err(
            "typed target or plain unblocked 6-damage Strike delta is inconsistent".to_owned(),
        );
    }
    for (index, (left, right)) in before_combat
        .monsters
        .iter()
        .zip(&after_combat.monsters)
        .enumerate()
    {
        if index != target_index && left != right {
            return Err(
                "a non-target monster changed during the scoped Strike transition".to_owned(),
            );
        }
    }
    if before.rng_internal.streams.len() != 6
        || before.rng_internal.streams != after.rng_internal.streams
    {
        return Err("the six captured RNG streams changed while playing Strike".to_owned());
    }
    validate_rng_owners(&before.rng_internal.streams)?;
    Ok(TargetedCardPlay {
        moved: moved.clone(),
        target_index,
        target_instance_id,
        target_id: target_before.id.clone(),
    })
}

fn validate_block_potion_original_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    slot: usize,
) -> Result<BlockPotionProjectionV1, String> {
    let before_combat = &before.public_base.combat;
    let after_combat = &after.public_base.combat;
    if before_combat.turn != after_combat.turn {
        return Err("Block Potion transition crosses a turn boundary".to_owned());
    }
    if before_combat.potions.len() != after_combat.potions.len() {
        return Err("potion slot count changed while using Block Potion".to_owned());
    }
    let before_slot = before_combat.potions.get(slot).ok_or_else(|| {
        "typed Block Potion slot is absent from predecessor observation".to_owned()
    })?;
    let after_slot = after_combat
        .potions
        .get(slot)
        .ok_or_else(|| "typed Block Potion slot is absent from successor observation".to_owned())?;
    let slot_i32 = i32::try_from(slot)
        .map_err(|_| "typed Block Potion slot does not fit Java int".to_owned())?;
    if before_slot.slot_index != slot
        || before_slot.potion_slot != slot_i32
        || before_slot.potion_id != "Block Potion"
        || before_slot.empty
        || before_slot.potency != 12
        || !before_slot.can_use
        || !before_slot.can_discard
        || before_slot.target_required
        || after_slot.slot_index != slot
        || after_slot.potion_slot != -1
        || after_slot.potion_id != "Potion Slot"
        || !after_slot.empty
        || after_slot.potency != 0
        || after_slot.can_use
        || after_slot.can_discard
        || after_slot.target_required
    {
        return Err("captured potion slot is not one ordinary consumed Block Potion".to_owned());
    }
    for (index, (left, right)) in before_combat
        .potions
        .iter()
        .zip(&after_combat.potions)
        .enumerate()
    {
        if index != slot && left != right {
            return Err("an untouched potion slot changed while using Block Potion".to_owned());
        }
    }
    if before_combat.player.current_hp != after_combat.player.current_hp
        || before_combat.player.max_hp != after_combat.player.max_hp
        || before_combat.player.energy != after_combat.player.energy
        || after_combat.player.block != before_combat.player.block + 12
        || before_combat.monsters != after_combat.monsters
    {
        return Err("scoped Block Potion delta is not exactly player block +12".to_owned());
    }
    let cards_unchanged =
        before.parity_internal.zones.as_card_zones() == after.parity_internal.zones.as_card_zones();
    if !cards_unchanged {
        return Err("card zones changed while using scoped Block Potion".to_owned());
    }
    validate_rng_owners(&before.rng_internal.streams)?;
    let six_rng_streams_unchanged = before.rng_internal.streams.len() == 6
        && before.rng_internal.streams == after.rng_internal.streams;
    if !six_rng_streams_unchanged {
        return Err("the six captured RNG streams changed while using Block Potion".to_owned());
    }
    Ok(BlockPotionProjectionV1 {
        turn_before: before_combat.turn,
        turn_after: after_combat.turn,
        player_hp_before: before_combat.player.current_hp,
        player_hp_after: after_combat.player.current_hp,
        player_max_hp_before: before_combat.player.max_hp,
        player_max_hp_after: after_combat.player.max_hp,
        player_block_before: before_combat.player.block,
        player_block_after: after_combat.player.block,
        energy_before: before_combat.player.energy,
        energy_after: after_combat.player.energy,
        potion_slot: slot,
        slot_empty_before: before_slot.empty,
        slot_empty_after: after_slot.empty,
        card_zones_unchanged: cards_unchanged,
        monsters_unchanged: true,
        six_rng_streams_unchanged,
    })
}

fn validate_fire_potion_original_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    slot: usize,
    target_index: usize,
) -> Result<FirePotionProjectionV1, String> {
    let before_combat = &before.public_base.combat;
    let after_combat = &after.public_base.combat;
    if before_combat.turn != after_combat.turn {
        return Err("Fire Potion transition crosses a turn boundary".to_owned());
    }
    if before_combat.potions.len() != after_combat.potions.len() {
        return Err("potion slot count changed while using Fire Potion".to_owned());
    }
    let before_slot = before_combat
        .potions
        .get(slot)
        .ok_or_else(|| "typed Fire Potion slot is absent from predecessor".to_owned())?;
    let after_slot = after_combat
        .potions
        .get(slot)
        .ok_or_else(|| "typed Fire Potion slot is absent from successor".to_owned())?;
    let slot_i32 = i32::try_from(slot)
        .map_err(|_| "typed Fire Potion slot does not fit Java int".to_owned())?;
    if before_slot.slot_index != slot
        || before_slot.potion_slot != slot_i32
        || before_slot.potion_id != "Fire Potion"
        || before_slot.empty
        || before_slot.potency != 20
        || !before_slot.can_use
        || !before_slot.can_discard
        || !before_slot.target_required
        || after_slot.slot_index != slot
        || after_slot.potion_slot != -1
        || after_slot.potion_id != "Potion Slot"
        || !after_slot.empty
        || after_slot.potency != 0
        || after_slot.can_use
        || after_slot.can_discard
        || after_slot.target_required
    {
        return Err("captured potion slot is not one ordinary consumed Fire Potion".to_owned());
    }
    for (index, (left, right)) in before_combat
        .potions
        .iter()
        .zip(&after_combat.potions)
        .enumerate()
    {
        if index != slot && left != right {
            return Err("an untouched potion slot changed while using Fire Potion".to_owned());
        }
    }
    if before_combat.player != after_combat.player {
        return Err("scoped Fire Potion changed player state".to_owned());
    }
    if before_combat.monsters.len() != after_combat.monsters.len() {
        return Err("monster count changed while using Fire Potion".to_owned());
    }
    let target_before = before_combat
        .monsters
        .get(target_index)
        .ok_or_else(|| "typed Fire Potion target is absent from predecessor".to_owned())?;
    let target_after = after_combat
        .monsters
        .get(target_index)
        .ok_or_else(|| "typed Fire Potion target is absent from successor".to_owned())?;
    let mut expected_target = target_before.clone();
    expected_target.current_hp = expected_target
        .current_hp
        .checked_sub(20)
        .ok_or_else(|| "Fire Potion target HP underflowed".to_owned())?;
    if target_before.index != target_index
        || target_before.block != 0
        || target_before.dead_or_escaped
        || target_before.current_hp <= 20
        || target_after != &expected_target
    {
        return Err(
            "scoped Fire Potion target is not one unblocked nonlethal 20-damage delta".to_owned(),
        );
    }
    let other_monsters_unchanged = before_combat
        .monsters
        .iter()
        .zip(&after_combat.monsters)
        .enumerate()
        .all(|(index, (left, right))| index == target_index || left == right);
    if !other_monsters_unchanged {
        return Err("a non-target monster changed while using Fire Potion".to_owned());
    }
    let card_zones_unchanged =
        before.parity_internal.zones.as_card_zones() == after.parity_internal.zones.as_card_zones();
    if !card_zones_unchanged {
        return Err("card zones changed while using Fire Potion".to_owned());
    }
    validate_rng_owners(&before.rng_internal.streams)?;
    let six_rng_streams_unchanged = before.rng_internal.streams.len() == 6
        && before.rng_internal.streams == after.rng_internal.streams;
    if !six_rng_streams_unchanged {
        return Err("the six captured RNG streams changed while using Fire Potion".to_owned());
    }
    Ok(FirePotionProjectionV1 {
        turn_before: before_combat.turn,
        turn_after: after_combat.turn,
        player_hp_before: before_combat.player.current_hp,
        player_hp_after: after_combat.player.current_hp,
        player_max_hp_before: before_combat.player.max_hp,
        player_max_hp_after: after_combat.player.max_hp,
        player_block_before: before_combat.player.block,
        player_block_after: after_combat.player.block,
        energy_before: before_combat.player.energy,
        energy_after: after_combat.player.energy,
        potion_slot: slot,
        slot_empty_before: before_slot.empty,
        slot_empty_after: after_slot.empty,
        target_hp_before: target_before.current_hp,
        target_hp_after: target_after.current_hp,
        target_block_before: target_before.block,
        target_block_after: target_after.block,
        card_zones_unchanged,
        other_monsters_unchanged,
        six_rng_streams_unchanged,
    })
}

fn run_fire_potion_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    slot: usize,
    target_index: usize,
) -> Result<FirePotionProjectionV1, String> {
    let (combat, identities) =
        legacy_narrow_transition::reconstruct_potion_combat(before, slot, PotionId::FirePotion)?;
    let target_entity_id = combat
        .entities
        .monsters
        .get(target_index)
        .ok_or_else(|| {
            "captured Fire Potion target does not fit reconstructed monsters".to_owned()
        })?
        .id;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::UsePotion {
        potion_index: slot,
        target: Some(target_entity_id),
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed Fire Potion action is not legal in the Rust stepper".to_owned());
    }
    let step = stepper.apply_to_stable(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err(
            "Rust Fire Potion transition did not reach a finite stable boundary".to_owned(),
        );
    }
    let reached = &step.position.combat;
    let projected = project_zones(reached, &identities)?;
    let rust_rng = project_rng_streams(&reached.rng.pool);
    let mut expected_monsters = position.combat.entities.monsters.clone();
    expected_monsters[target_index].current_hp -= 20;
    let target_before = &position.combat.entities.monsters[target_index];
    let target_after = &reached.entities.monsters[target_index];
    if projected != after.parity_internal.zones.as_card_zones()
        || reached.turn.turn_count != after.public_base.combat.turn
        || reached.turn.energy as i32 != after.public_base.combat.player.energy
        || reached.entities.player.current_hp != after.public_base.combat.player.current_hp
        || reached.entities.player.max_hp != after.public_base.combat.player.max_hp
        || reached.entities.player.block != after.public_base.combat.player.block
        || reached.entities.monsters != expected_monsters
        || target_after.current_hp != after.public_base.combat.monsters[target_index].current_hp
        || reached
            .entities
            .potions
            .get(slot)
            .and_then(Option::as_ref)
            .is_some()
        || rust_rng != after.rng_internal.streams
    {
        return Err(
            "Rust Fire Potion successor does not match the captured scoped projection".to_owned(),
        );
    }
    Ok(FirePotionProjectionV1 {
        turn_before: before.public_base.combat.turn,
        turn_after: reached.turn.turn_count,
        player_hp_before: before.public_base.combat.player.current_hp,
        player_hp_after: reached.entities.player.current_hp,
        player_max_hp_before: before.public_base.combat.player.max_hp,
        player_max_hp_after: reached.entities.player.max_hp,
        player_block_before: before.public_base.combat.player.block,
        player_block_after: reached.entities.player.block,
        energy_before: before.public_base.combat.player.energy,
        energy_after: reached.turn.energy as i32,
        potion_slot: slot,
        slot_empty_before: false,
        slot_empty_after: true,
        target_hp_before: target_before.current_hp,
        target_hp_after: target_after.current_hp,
        target_block_before: target_before.block,
        target_block_after: target_after.block,
        card_zones_unchanged: projected == before.parity_internal.zones.as_card_zones(),
        other_monsters_unchanged: position
            .combat
            .entities
            .monsters
            .iter()
            .zip(&reached.entities.monsters)
            .enumerate()
            .all(|(index, (left, right))| index == target_index || left == right),
        six_rng_streams_unchanged: rust_rng == before.rng_internal.streams,
    })
}

fn run_block_potion_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    slot: usize,
) -> Result<BlockPotionProjectionV1, String> {
    let (combat, identities) =
        legacy_narrow_transition::reconstruct_potion_combat(before, slot, PotionId::BlockPotion)?;

    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::UsePotion {
        potion_index: slot,
        target: None,
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err(
            "reconstructed Block Potion action is not legal in the Rust stepper".to_owned(),
        );
    }
    let step = stepper.apply_to_stable(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err(
            "Rust Block Potion transition did not reach a finite stable boundary".to_owned(),
        );
    }
    let reached = &step.position.combat;
    let projected = project_zones(reached, &identities)?;
    let rust_rng = project_rng_streams(&reached.rng.pool);
    if projected != after.parity_internal.zones.as_card_zones()
        || reached.turn.turn_count != after.public_base.combat.turn
        || reached.turn.energy as i32 != after.public_base.combat.player.energy
        || reached.entities.player.current_hp != after.public_base.combat.player.current_hp
        || reached.entities.player.max_hp != after.public_base.combat.player.max_hp
        || reached.entities.player.block != after.public_base.combat.player.block
        || reached.entities.monsters != position.combat.entities.monsters
        || reached
            .entities
            .potions
            .get(slot)
            .and_then(Option::as_ref)
            .is_some()
        || rust_rng != after.rng_internal.streams
    {
        return Err(
            "Rust Block Potion successor does not match the captured scoped projection".to_owned(),
        );
    }
    Ok(BlockPotionProjectionV1 {
        turn_before: before.public_base.combat.turn,
        turn_after: reached.turn.turn_count,
        player_hp_before: before.public_base.combat.player.current_hp,
        player_hp_after: reached.entities.player.current_hp,
        player_max_hp_before: before.public_base.combat.player.max_hp,
        player_max_hp_after: reached.entities.player.max_hp,
        player_block_before: before.public_base.combat.player.block,
        player_block_after: reached.entities.player.block,
        energy_before: before.public_base.combat.player.energy,
        energy_after: reached.turn.energy as i32,
        potion_slot: slot,
        slot_empty_before: false,
        slot_empty_after: true,
        card_zones_unchanged: projected == before.parity_internal.zones.as_card_zones(),
        monsters_unchanged: reached.entities.monsters == position.combat.entities.monsters,
        six_rng_streams_unchanged: rust_rng == before.rng_internal.streams,
    })
}

fn run_minimal_targeted_strike_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    targeted: &TargetedCardPlay,
) -> Result<TargetedStrikeProjectionV1, String> {
    let zones = &before.parity_internal.zones;
    let (combat, identities) = reconstruct_plain_card_combat(before)?;

    let selected_rust_lineage = identities
        .rust_lineage(targeted.moved.instance_uuid.as_str())
        .ok_or_else(|| "inferred Strike UUID is absent from the local identity map".to_owned())?;
    let card_index = combat
        .zones
        .hand
        .iter()
        .position(|card| card.lineage == selected_rust_lineage)
        .ok_or_else(|| "inferred Strike is absent from reconstructed hand".to_owned())?;
    let target_entity_id = combat
        .entities
        .monsters
        .get(targeted.target_index)
        .ok_or_else(|| "reconstructed Strike target index is absent".to_owned())?
        .id;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index,
        target: Some(target_entity_id),
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed targeted Strike is not legal in the Rust stepper".to_owned());
    }
    let step = stepper.apply_to_stable(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err("Rust Strike transition did not reach a finite stable boundary".to_owned());
    }
    let reached = &step.position.combat;
    let projected = project_zones(reached, &identities)?;
    let after_combat = &after.public_base.combat;
    let target_before = &before.public_base.combat.monsters[targeted.target_index];
    let target_after = &after_combat.monsters[targeted.target_index];
    let reached_target = &reached.entities.monsters[targeted.target_index];
    let other_monsters_unchanged = reached
        .entities
        .monsters
        .iter()
        .zip(&position.combat.entities.monsters)
        .enumerate()
        .all(|(index, (right, left))| index == targeted.target_index || right == left);
    let rust_rng = project_rng_streams(&reached.rng.pool);
    if projected != after.parity_internal.zones.as_card_zones()
        || reached.turn.energy as i32 != after_combat.player.energy
        || reached.entities.player.current_hp != after_combat.player.current_hp
        || reached.entities.player.max_hp != after_combat.player.max_hp
        || reached.entities.player.block != after_combat.player.block
        || reached_target.current_hp != target_after.current_hp
        || reached_target.block != target_after.block
        || !other_monsters_unchanged
        || rust_rng != after.rng_internal.streams
    {
        return Err(
            "Rust Strike successor does not match the captured scoped projection".to_owned(),
        );
    }
    Ok(TargetedStrikeProjectionV1 {
        energy_before: before.public_base.combat.player.energy,
        energy_after: reached.turn.energy as i32,
        player_hp_before: before.public_base.combat.player.current_hp,
        player_hp_after: reached.entities.player.current_hp,
        player_block_before: before.public_base.combat.player.block,
        player_block_after: reached.entities.player.block,
        target_hp_before: target_before.current_hp,
        target_hp_after: reached_target.current_hp,
        target_block_before: target_before.block,
        target_block_after: reached_target.block,
        hand_count_before: zones.hand.cards.len(),
        hand_count_after: reached.zones.hand.len(),
        discard_count_before: zones.discard.cards.len(),
        discard_count_after: reached.zones.discard_pile.len(),
        moved_card_reached_discard: reached
            .zones
            .discard_pile
            .iter()
            .any(|card| card.lineage == selected_rust_lineage),
        other_monsters_unchanged,
        six_rng_streams_unchanged: rust_rng == before.rng_internal.streams,
    })
}

fn run_minimal_whirlwind_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    moved: &CardProjection,
    relic_context: &WhirlwindRelicContext,
) -> Result<WhirlwindProjectionV1, String> {
    let zones = &before.parity_internal.zones;
    let (mut combat, identities) = reconstruct_plain_card_combat(before)?;
    if relic_context.chemical_x_bonus_repetitions > 0 {
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::ChemicalX));
    }

    let selected_rust_lineage = identities
        .rust_lineage(moved.instance_uuid.as_str())
        .ok_or_else(|| {
            "inferred Whirlwind UUID is absent from the local identity map".to_owned()
        })?;
    let card_index = combat
        .zones
        .hand
        .iter()
        .position(|card| card.lineage == selected_rust_lineage)
        .ok_or_else(|| "inferred Whirlwind is absent from reconstructed hand".to_owned())?;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index,
        target: None,
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed Whirlwind is not legal in the Rust stepper".to_owned());
    }
    let step = stepper.apply_to_stable(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err("Rust Whirlwind transition did not reach a finite stable boundary".to_owned());
    }

    let reached = &step.position.combat;
    let after_combat = &after.public_base.combat;
    let projected = project_zones(reached, &identities)?;
    let rust_rng = project_rng_streams(&reached.rng.pool);
    let monsters_match = reached.entities.monsters.len() == after_combat.monsters.len()
        && reached
            .entities
            .monsters
            .iter()
            .zip(&after_combat.monsters)
            .all(|(rust, java)| {
                rust.current_hp == java.current_hp
                    && rust.max_hp == java.max_hp
                    && rust.block == java.block
            });
    if projected != after.parity_internal.zones.as_card_zones()
        || reached.turn.energy as i32 != after_combat.player.energy
        || reached.entities.player.current_hp != after_combat.player.current_hp
        || reached.entities.player.max_hp != after_combat.player.max_hp
        || reached.entities.player.block != after_combat.player.block
        || !monsters_match
        || rust_rng != after.rng_internal.streams
    {
        return Err(
            "Rust Whirlwind successor does not match the captured scoped projection".to_owned(),
        );
    }

    let energy_committed = before.public_base.combat.player.energy;
    let effect_repetitions = energy_committed + relic_context.chemical_x_bonus_repetitions;
    Ok(WhirlwindProjectionV1 {
        energy_before: energy_committed,
        energy_committed,
        energy_after: reached.turn.energy as i32,
        effect_repetitions,
        chemical_x_bonus_repetitions: relic_context.chemical_x_bonus_repetitions,
        damage_per_enemy_total: effect_repetitions * 5,
        player_hp_before: before.public_base.combat.player.current_hp,
        player_hp_after: reached.entities.player.current_hp,
        player_max_hp_before: before.public_base.combat.player.max_hp,
        player_max_hp_after: reached.entities.player.max_hp,
        player_block_before: before.public_base.combat.player.block,
        player_block_after: reached.entities.player.block,
        monster_hp_before: before
            .public_base
            .combat
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        monster_hp_after: reached
            .entities
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        hand_count_before: zones.hand.cards.len(),
        hand_count_after: reached.zones.hand.len(),
        discard_count_before: zones.discard.cards.len(),
        discard_count_after: reached.zones.discard_pile.len(),
        moved_card_reached_discard: reached
            .zones
            .discard_pile
            .iter()
            .any(|card| card.lineage == selected_rust_lineage),
        six_rng_streams_unchanged: rust_rng == before.rng_internal.streams,
        observed_relic_ids: relic_context.observed_relic_ids.clone(),
    })
}

fn load_completed_capture(
    artifact_path: &Path,
    receipt_path: &Path,
) -> Result<LoadedCapture, String> {
    let artifact = fs::read(artifact_path).map_err(|error| {
        format!(
            "failed to read artifact '{}': {error}",
            artifact_path.display()
        )
    })?;
    let receipt_bytes = fs::read(receipt_path).map_err(|error| {
        format!(
            "failed to read receipt '{}': {error}",
            receipt_path.display()
        )
    })?;
    let receipt: CaptureReceipt = serde_json::from_slice(&receipt_bytes)
        .map_err(|error| format!("failed to decode capture receipt: {error}"))?;
    if receipt.schema != RECEIPT_SCHEMA || receipt.status != "completed" {
        return Err("capture receipt is not a completed V1 receipt".to_owned());
    }
    let artifact_sha256 = sha256_hex(&artifact);
    if receipt.artifact.bytes != artifact.len() as u64
        || !receipt
            .artifact
            .sha256
            .eq_ignore_ascii_case(&artifact_sha256)
    {
        return Err(
            "capture artifact bytes or SHA-256 do not match the completion receipt".to_owned(),
        );
    }
    let text = std::str::from_utf8(&artifact)
        .map_err(|_| "capture artifact is not valid UTF-8".to_owned())?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != receipt.captured_frames.saturating_add(2) {
        return Err(
            "capture artifact record count does not match the completion receipt".to_owned(),
        );
    }
    let kinds: Vec<EnvelopeKind> = lines
        .iter()
        .map(|line| {
            serde_json::from_str(line).map_err(|error| format!("invalid capture envelope: {error}"))
        })
        .collect::<Result<_, _>>()?;
    if kinds.first().map(|kind| kind.kind.as_str()) != Some("hello")
        || kinds.get(1).map(|kind| kind.kind.as_str()) != Some("hello_ack")
    {
        return Err("capture artifact does not begin with hello and hello_ack".to_owned());
    }
    let frames: Vec<StateFrameProjection> = lines
        .iter()
        .skip(2)
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| format!("invalid state_frame projection: {error}"))
        })
        .collect::<Result<_, _>>()?;
    if frames.len() != receipt.captured_frames
        || frames.first().map(|frame| frame.frame_id) != Some(receipt.first_frame_id)
        || frames.last().map(|frame| frame.frame_id) != Some(receipt.last_frame_id)
    {
        return Err("capture frame range does not match the completion receipt".to_owned());
    }
    for (index, frame) in frames.iter().enumerate() {
        let expected_id =
            u64::try_from(index).map_err(|_| "frame index does not fit u64".to_owned())? + 1;
        let expected_previous = (index > 0).then_some(expected_id - 1);
        if frame.kind != "state_frame"
            || frame.capture_id != receipt.capture_id
            || frame.frame_id != expected_id
            || frame.previous_frame_id != expected_previous
        {
            return Err("capture frame identity or sequence is inconsistent".to_owned());
        }
    }
    Ok(LoadedCapture {
        receipt,
        artifact_sha256,
        receipt_sha256: sha256_hex(&receipt_bytes),
        frames,
    })
}

fn validate_adjacent_frames(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    capture_id: &str,
) -> Result<(), String> {
    if before.capture_id != capture_id
        || after.capture_id != capture_id
        || after.previous_frame_id != Some(before.frame_id)
        || before.profile != "parity_snapshot"
        || after.profile != "parity_snapshot"
        || !before.grants.control.is_empty()
        || !after.grants.control.is_empty()
        || before.boundary.kind != "combat_player_decision"
        || after.boundary.kind != "combat_player_decision"
        || !before.boundary.stable
        || !after.boundary.stable
    {
        return Err(
            "selected frames are not one adjacent observe-only stable transition".to_owned(),
        );
    }
    if before.public_base.run != after.public_base.run
        || before.public_base.run.character != "IRONCLAD"
        || before.public_base.combat.turn != after.public_base.combat.turn
    {
        return Err("selected frames cross a run, character, or turn boundary".to_owned());
    }
    validate_frame_zone_consistency(before)?;
    validate_frame_zone_consistency(after)
}

fn validate_frame_zone_consistency(frame: &StateFrameProjection) -> Result<(), String> {
    for zone in [
        &frame.parity_internal.zones.draw,
        &frame.parity_internal.zones.discard,
        &frame.parity_internal.zones.exhaust,
        &frame.parity_internal.zones.hand,
        &frame.parity_internal.zones.limbo,
    ] {
        if zone.ordering != "java_group_index_ascending" || zone.top != "last" {
            return Err("parity zone ordering is not the V1 Java ordering".to_owned());
        }
    }
    if frame.public_base.combat.hand != frame.parity_internal.zones.hand.cards
        || frame.public_base.combat.discard.cards != frame.parity_internal.zones.discard.cards
        || frame.public_base.combat.exhaust.cards != frame.parity_internal.zones.exhaust.cards
        || frame.public_base.combat.draw_count != frame.parity_internal.zones.draw.cards.len()
    {
        return Err("public and parity-internal card zones disagree".to_owned());
    }
    Ok(())
}

fn infer_base_defend_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
) -> Result<CardProjection, String> {
    let before_combat = &before.public_base.combat;
    let after_combat = &after.public_base.combat;
    let after_hand: BTreeMap<&str, &CardProjection> = unique_cards_by_uuid(&after_combat.hand)?;
    let moved: Vec<&CardProjection> = before_combat
        .hand
        .iter()
        .filter(|card| !after_hand.contains_key(card.instance_uuid.as_str()))
        .collect();
    if moved.len() != 1 || before_combat.hand.len() != after_combat.hand.len() + 1 {
        return Err("transition does not move exactly one card out of hand".to_owned());
    }
    let moved = moved[0];
    for card in &after_combat.hand {
        let Some(previous) = before_combat
            .hand
            .iter()
            .find(|candidate| candidate.instance_uuid == card.instance_uuid)
        else {
            return Err("transition introduces an unexpected hand card".to_owned());
        };
        if previous != card {
            return Err("an untouched hand card changed during the transition".to_owned());
        }
    }
    let mut expected_discard = before.parity_internal.zones.discard.cards.clone();
    expected_discard.push(moved.clone());
    if after.parity_internal.zones.discard.cards != expected_discard
        || before.parity_internal.zones.draw.cards != after.parity_internal.zones.draw.cards
        || before.parity_internal.zones.exhaust.cards != after.parity_internal.zones.exhaust.cards
        || before.parity_internal.zones.limbo.cards != after.parity_internal.zones.limbo.cards
    {
        return Err("card zones do not describe one ordinary hand-to-discard play".to_owned());
    }
    if moved.card_id != BASE_DEFEND_JAVA_ID || moved.upgrades != 0 || moved.cost_for_turn != 1 {
        return Err("the inferred card is not one ordinary unupgraded Ironclad Defend".to_owned());
    }
    if after_combat.player.energy != before_combat.player.energy - 1
        || after_combat.player.block != before_combat.player.block + 5
    {
        return Err("original-game Defend delta is not energy -1 and block +5".to_owned());
    }
    if before_combat.player.current_hp != after_combat.player.current_hp
        || before_combat.player.max_hp != after_combat.player.max_hp
        || before_combat.monsters != after_combat.monsters
    {
        return Err("player HP or monsters changed during the local Defend transition".to_owned());
    }
    if before.rng_internal.streams.len() != 6
        || before.rng_internal.streams != after.rng_internal.streams
    {
        return Err("the six captured RNG streams changed while playing Defend".to_owned());
    }
    validate_rng_owners(&before.rng_internal.streams)?;
    Ok(moved.clone())
}

fn run_minimal_simulator_transition(
    before: &StateFrameProjection,
    after: &StateFrameProjection,
    moved: &CardProjection,
) -> Result<TransitionProjectionV1, String> {
    let zones = &before.parity_internal.zones;
    let (combat, identities) = reconstruct_plain_card_combat(before)?;

    let selected_rust_lineage = identities
        .rust_lineage(moved.instance_uuid.as_str())
        .ok_or_else(|| "inferred Defend UUID is absent from the local identity map".to_owned())?;
    let card_index = combat
        .zones
        .hand
        .iter()
        .position(|card| card.lineage == selected_rust_lineage)
        .ok_or_else(|| "inferred Defend is absent from reconstructed hand".to_owned())?;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index,
        target: None,
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(&position, &input) {
        return Err("reconstructed Defend action is not legal in the Rust stepper".to_owned());
    }
    let step = stepper.apply_to_stable(
        &position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err("Rust Defend transition did not reach a finite stable boundary".to_owned());
    }
    let reached = &step.position.combat;
    if reached.entities.monsters != position.combat.entities.monsters {
        return Err(
            "Rust Defend transition unexpectedly changed the placeholder monsters".to_owned(),
        );
    }
    let projected = project_zones(reached, &identities)?;
    if projected != after.parity_internal.zones.as_card_zones()
        || reached.turn.energy as i32 != after.public_base.combat.player.energy
        || reached.entities.player.block != after.public_base.combat.player.block
        || reached.entities.player.current_hp != after.public_base.combat.player.current_hp
        || reached.entities.player.max_hp != after.public_base.combat.player.max_hp
    {
        return Err(
            "Rust Defend successor does not match the captured local projection".to_owned(),
        );
    }
    let rust_rng = project_rng_streams(&reached.rng.pool);
    if rust_rng != after.rng_internal.streams {
        return Err(
            "Rust Defend successor does not preserve the six captured RNG streams".to_owned(),
        );
    }
    Ok(TransitionProjectionV1 {
        energy_before: before.public_base.combat.player.energy,
        energy_after: reached.turn.energy as i32,
        block_before: before.public_base.combat.player.block,
        block_after: reached.entities.player.block,
        hand_count_before: zones.hand.cards.len(),
        hand_count_after: reached.zones.hand.len(),
        discard_count_before: zones.discard.cards.len(),
        discard_count_after: reached.zones.discard_pile.len(),
        moved_card_reached_discard: reached
            .zones
            .discard_pile
            .iter()
            .any(|card| card.lineage == selected_rust_lineage),
        six_rng_streams_unchanged: rust_rng == before.rng_internal.streams,
    })
}

fn reconstruct_plain_card_combat(
    before: &StateFrameProjection,
) -> Result<
    (
        sts_core::runtime::combat::CombatState,
        CrossRuntimeCardBindings,
    ),
    String,
> {
    if !before.parity_internal.player_powers.is_empty() {
        return Err("plain card reconstruction does not support player powers".to_owned());
    }
    legacy_narrow_transition::reconstruct_card_combat_base(before)
}

fn reconstruct_corruption_card_combat(
    before: &StateFrameProjection,
) -> Result<
    (
        sts_core::runtime::combat::CombatState,
        CrossRuntimeCardBindings,
    ),
    String,
> {
    validate_exact_corruption_player_power(&before.parity_internal.player_powers)?;
    let (mut combat, identities) = legacy_narrow_transition::reconstruct_card_combat_base(before)?;
    sts_core::content::powers::store::set_powers_for(
        &mut combat,
        0,
        vec![Power {
            power_type: PowerId::Corruption,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    sts_core::content::cards::ironclad::corruption::corruption_on_apply(&mut combat);
    Ok((combat, identities))
}

fn validate_exact_corruption_player_power(
    powers: &[PowerInternalProjection],
) -> Result<(), String> {
    if powers.len() != 1 {
        return Err("Corruption reconstruction requires exactly one player power".to_owned());
    }
    let power = &powers[0];
    if power.index != 0
        || power.id != "Corruption"
        || power.class_name != "com.megacrit.cardcrawl.powers.CorruptionPower"
        || power.amount != -1
        || power.power_type != "BUFF"
        || power.ritual_skip_first.is_some()
        || power.ritual_player_controlled.is_some()
    {
        return Err("player power is not the exact supported CorruptionPower".to_owned());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComparableZones {
    draw: Vec<CardProjection>,
    discard: Vec<CardProjection>,
    exhaust: Vec<CardProjection>,
    hand: Vec<CardProjection>,
    limbo: Vec<CardProjection>,
}

impl ParityZonesProjection {
    fn as_card_zones(&self) -> ComparableZones {
        ComparableZones {
            draw: self.draw.cards.clone(),
            discard: self.discard.cards.clone(),
            exhaust: self.exhaust.cards.clone(),
            hand: self.hand.cards.clone(),
            limbo: self.limbo.cards.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct JavaCardUuid(String);

impl JavaCardUuid {
    fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty() {
            return Err("Java card UUID must not be empty".to_owned());
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CardBindingEvidence {
    CapturedScenarioRecipe {
        recipe_slot: u32,
        captured_zone: &'static str,
        captured_index: usize,
    },
    CapturedBirth {
        cause: card_birth::CardBindingBirthCauseEvidence,
        from_frame_id: u64,
        to_frame_id: u64,
        generation_index: u32,
    },
}

/// A proven relation between Java's random per-instance UUID and one
/// Rust-local semantic lineage.
///
/// Neither side is derived from or numerically compared with the other.  A
/// relation is created only while constructing the captured root or by a
/// transition-specific generation witness.
struct CrossRuntimeCardBindings {
    java_to_rust: BTreeMap<JavaCardUuid, CardLineageId>,
    rust_to_java: BTreeMap<CardLineageId, JavaCardUuid>,
    evidence_by_rust: BTreeMap<CardLineageId, CardBindingEvidence>,
}

struct CapturedScenarioCardRecipe {
    java_uuid: JavaCardUuid,
    rust_lineage: CardLineageId,
    captured_zone: &'static str,
    captured_index: usize,
}

/// Compiles one artificial parity root directly from the captured Java card
/// objects. This defines new `constructed_scenario` lineages; it does not infer
/// bindings to an independently existing Rust natural run.
struct CapturedScenarioRecipe {
    cards: Vec<CapturedScenarioCardRecipe>,
}

impl CapturedScenarioRecipe {
    fn compile(zones: &ParityZonesProjection) -> Result<Self, String> {
        let mut cards = Vec::new();
        let mut stable_semantics_by_uuid = BTreeMap::new();
        for (captured_zone, zone) in [
            ("draw", &zones.draw),
            ("hand", &zones.hand),
            ("discard", &zones.discard),
            ("exhaust", &zones.exhaust),
            ("limbo", &zones.limbo),
        ] {
            for (captured_index, card) in zone.cards.iter().enumerate() {
                let java_uuid = JavaCardUuid::new(card.instance_uuid.clone())?;
                if let Some((existing_id, existing_upgrades)) =
                    stable_semantics_by_uuid.get(&java_uuid)
                {
                    if existing_id != &card.card_id || *existing_upgrades != card.upgrades {
                        return Err(format!(
                            "captured Java UUID {} has conflicting stable card semantics",
                            java_uuid.as_str()
                        ));
                    }
                    continue;
                }
                let recipe_slot = u32::try_from(cards.len())
                    .map_err(|_| "captured scenario has more card slots than u32".to_owned())?
                    .checked_add(1)
                    .ok_or_else(|| "captured scenario card slot overflowed u32".to_owned())?;
                cards.push(CapturedScenarioCardRecipe {
                    java_uuid: java_uuid.clone(),
                    rust_lineage: CardLineageId::constructed_scenario(recipe_slot),
                    captured_zone,
                    captured_index,
                });
                stable_semantics_by_uuid.insert(java_uuid, (card.card_id.clone(), card.upgrades));
            }
        }
        Ok(Self { cards })
    }
}

fn compile_captured_scenario_bindings(
    zones: &ParityZonesProjection,
) -> Result<CrossRuntimeCardBindings, String> {
    CrossRuntimeCardBindings::from_captured_scenario_recipe(CapturedScenarioRecipe::compile(zones)?)
}

impl CrossRuntimeCardBindings {
    fn from_captured_scenario_recipe(recipe: CapturedScenarioRecipe) -> Result<Self, String> {
        let mut bindings = Self {
            java_to_rust: BTreeMap::new(),
            rust_to_java: BTreeMap::new(),
            evidence_by_rust: BTreeMap::new(),
        };
        for card in recipe.cards {
            let recipe_slot = card.rust_lineage.ordinal();
            bindings.bind(
                card.java_uuid,
                card.rust_lineage,
                CardBindingEvidence::CapturedScenarioRecipe {
                    recipe_slot,
                    captured_zone: card.captured_zone,
                    captured_index: card.captured_index,
                },
            )?;
        }
        Ok(bindings)
    }

    fn bind(
        &mut self,
        java_uuid: JavaCardUuid,
        rust_lineage: CardLineageId,
        evidence: CardBindingEvidence,
    ) -> Result<(), String> {
        if let Some(existing) = self.java_to_rust.get(&java_uuid) {
            return Err(format!(
                "Java card UUID {} is already bound to Rust lineage {existing}",
                java_uuid.as_str()
            ));
        }
        if let Some(existing) = self.rust_to_java.get(&rust_lineage) {
            return Err(format!(
                "Rust lineage {rust_lineage} is already bound to Java card UUID {}",
                existing.as_str()
            ));
        }
        self.java_to_rust.insert(java_uuid.clone(), rust_lineage);
        self.rust_to_java.insert(rust_lineage, java_uuid);
        self.evidence_by_rust.insert(rust_lineage, evidence);
        Ok(())
    }

    fn rust_lineage(&self, java_uuid: &str) -> Option<CardLineageId> {
        self.java_to_rust
            .get(&JavaCardUuid(java_uuid.to_owned()))
            .copied()
    }

    fn java_uuid(&self, rust_lineage: CardLineageId) -> Option<&JavaCardUuid> {
        self.rust_to_java.get(&rust_lineage)
    }

    fn contains_rust(&self, rust_lineage: CardLineageId) -> bool {
        self.rust_to_java.contains_key(&rust_lineage)
    }

    fn bind_complete_successor_zones(
        &mut self,
        combat: &sts_core::runtime::combat::CombatState,
        expected: &ParityZonesProjection,
    ) -> Result<ComparableZones, String> {
        let mut draw = combat.zones.draw_pile.iter().collect::<Vec<_>>();
        draw.reverse();
        let discard = combat.zones.discard_pile.iter().collect::<Vec<_>>();
        let exhaust = combat.zones.exhaust_pile.iter().collect::<Vec<_>>();
        let hand = combat.zones.hand.iter().collect::<Vec<_>>();
        let limbo = combat.zones.limbo.iter().collect::<Vec<_>>();
        let mut seen_rust = BTreeSet::new();
        let mut seen_java = BTreeSet::new();
        let mut stable_semantics_by_rust = BTreeMap::new();
        let mut stable_semantics_by_java = BTreeMap::new();
        for (name, rust, java) in [
            ("draw", draw.as_slice(), expected.draw.cards.as_slice()),
            (
                "discard",
                discard.as_slice(),
                expected.discard.cards.as_slice(),
            ),
            (
                "exhaust",
                exhaust.as_slice(),
                expected.exhaust.cards.as_slice(),
            ),
            ("hand", hand.as_slice(), expected.hand.cards.as_slice()),
            ("limbo", limbo.as_slice(), expected.limbo.cards.as_slice()),
        ] {
            self.verify_successor_zone(
                name,
                rust,
                java,
                &mut seen_rust,
                &mut seen_java,
                &mut stable_semantics_by_rust,
                &mut stable_semantics_by_java,
            )?;
        }
        if seen_rust.len() != seen_java.len()
            || seen_rust.iter().any(|rust| {
                self.rust_to_java
                    .get(rust)
                    .is_none_or(|java| !seen_java.contains(java))
            })
            || seen_java.iter().any(|java| {
                self.java_to_rust
                    .get(java)
                    .is_none_or(|rust| !seen_rust.contains(rust))
            })
        {
            return Err(
                "successor card identity map is not a one-to-one cover of both live successor zones"
                    .to_owned(),
            );
        }
        let projected = project_zones(combat, self)?;
        if projected != expected.as_card_zones() {
            return Err(
                "Rust successor card zones differ from Java after identity binding".to_owned(),
            );
        }
        Ok(projected)
    }

    fn verify_successor_zone(
        &self,
        name: &str,
        rust: &[&CombatCard],
        java: &[CardProjection],
        seen_rust: &mut BTreeSet<CardLineageId>,
        seen_java: &mut BTreeSet<JavaCardUuid>,
        stable_semantics_by_rust: &mut BTreeMap<CardLineageId, (CardId, u8)>,
        stable_semantics_by_java: &mut BTreeMap<JavaCardUuid, (String, u8)>,
    ) -> Result<(), String> {
        if rust.len() != java.len() {
            return Err(format!("Rust and Java {name} zone lengths differ"));
        }
        for (rust_card, java_card) in rust.iter().zip(java) {
            if java_id(rust_card.id) != java_card.card_id
                || rust_card.upgrades != java_card.upgrades
                || rust_card.cost_for_turn_java() != java_card.cost_for_turn
            {
                return Err(format!("Rust and Java {name} card semantics differ"));
            }
            let java_uuid = JavaCardUuid::new(java_card.instance_uuid.clone())?;
            if stable_semantics_by_rust
                .insert(rust_card.lineage, (rust_card.id, rust_card.upgrades))
                .is_some_and(|existing| existing != (rust_card.id, rust_card.upgrades))
                || stable_semantics_by_java
                    .insert(
                        java_uuid.clone(),
                        (java_card.card_id.clone(), java_card.upgrades),
                    )
                    .is_some_and(|existing| {
                        existing != (java_card.card_id.clone(), java_card.upgrades)
                    })
            {
                return Err(format!(
                    "Rust or Java {name} same-lineage copies have conflicting stable semantics"
                ));
            }
            seen_rust.insert(rust_card.lineage);
            seen_java.insert(java_uuid);
            match self.rust_to_java.get(&rust_card.lineage) {
                Some(bound) if bound.as_str() == java_card.instance_uuid => {}
                Some(_) => {
                    return Err(format!(
                        "known Rust card moved to a different Java identity in {name}"
                    ))
                }
                None => {
                    return Err(format!(
                        "Rust card lineage {} in {name} has no provenance-backed Java binding",
                        rust_card.lineage
                    ));
                }
            }
        }
        Ok(())
    }
}

fn build_cards(
    cards: &[CardProjection],
    identities: &CrossRuntimeCardBindings,
) -> Result<Vec<CombatCard>, String> {
    let java_ids = build_java_id_map();
    cards
        .iter()
        .map(|captured| {
            let id = *java_ids.get(captured.card_id.as_str()).ok_or_else(|| {
                format!(
                    "simulator has no Java card ID mapping for {}",
                    captured.card_id
                )
            })?;
            let lineage = identities
                .rust_lineage(captured.instance_uuid.as_str())
                .ok_or_else(|| {
                    format!(
                        "card UUID {} is absent from the identity map",
                        captured.instance_uuid
                    )
                })?;
            let mut card = CombatCard::with_lineage(id, lineage);
            card.upgrades = captured.upgrades;
            if card.cost_for_turn_java() != captured.cost_for_turn {
                card.set_cost_for_turn_java(captured.cost_for_turn);
            }
            Ok(card)
        })
        .collect()
}

fn project_zones(
    combat: &sts_core::runtime::combat::CombatState,
    identities: &CrossRuntimeCardBindings,
) -> Result<ComparableZones, String> {
    let mut draw = project_cards(combat.zones.draw_pile.iter(), identities)?;
    draw.reverse();
    Ok(ComparableZones {
        draw,
        discard: project_cards(combat.zones.discard_pile.iter(), identities)?,
        exhaust: project_cards(combat.zones.exhaust_pile.iter(), identities)?,
        hand: project_cards(combat.zones.hand.iter(), identities)?,
        limbo: project_cards(combat.zones.limbo.iter(), identities)?,
    })
}

fn project_cards<'a>(
    cards: impl Iterator<Item = &'a CombatCard>,
    identities: &CrossRuntimeCardBindings,
) -> Result<Vec<CardProjection>, String> {
    cards
        .map(|card| {
            let instance_uuid = identities
                .java_uuid(card.lineage)
                .ok_or_else(|| format!("simulator produced unmapped card UUID {}", card.lineage))?;
            Ok(CardProjection {
                instance_uuid: instance_uuid.as_str().to_owned(),
                card_id: java_id(card.id).to_owned(),
                upgrades: card.upgrades,
                cost_for_turn: card.cost_for_turn_java(),
            })
        })
        .collect()
}

fn unique_cards_by_uuid(
    cards: &[CardProjection],
) -> Result<BTreeMap<&str, &CardProjection>, String> {
    let mut by_uuid = BTreeMap::new();
    for card in cards {
        if by_uuid.insert(card.instance_uuid.as_str(), card).is_some() {
            return Err(format!("duplicate hand card UUID: {}", card.instance_uuid));
        }
    }
    Ok(by_uuid)
}

fn validate_rng_owners(streams: &[RngStreamProjection]) -> Result<(), String> {
    let expected = BTreeSet::from([
        AI_RNG,
        SHUFFLE_RNG,
        CARD_RANDOM_RNG,
        MISC_RNG,
        MONSTER_HP_RNG,
        POTION_RNG,
    ]);
    let actual: BTreeSet<&str> = streams.iter().map(|stream| stream.owner.as_str()).collect();
    if actual != expected || streams.len() != expected.len() {
        return Err("capture does not contain exactly the six supported RNG owners".to_owned());
    }
    Ok(())
}

fn inject_rng_streams(
    pool: &mut sts_core::runtime::rng::RngPool,
    streams: &[RngStreamProjection],
) -> Result<(), String> {
    validate_rng_owners(streams)?;
    for stream in streams {
        let rng = decode_rng(stream)?;
        match stream.owner.as_str() {
            AI_RNG => pool.ai_rng = rng,
            SHUFFLE_RNG => pool.shuffle_rng = rng,
            CARD_RANDOM_RNG => pool.card_random_rng = rng,
            MISC_RNG => pool.misc_rng = rng,
            MONSTER_HP_RNG => pool.monster_hp_rng = rng,
            POTION_RNG => pool.potion_rng = rng,
            _ => unreachable!("validated RNG owner"),
        }
    }
    Ok(())
}

fn decode_rng(stream: &RngStreamProjection) -> Result<StsRng, String> {
    if stream.algorithm != "com.badlogic.gdx.math.RandomXS128" || stream.state_hex.len() != 2 {
        return Err(format!(
            "unsupported RNG representation for {}",
            stream.owner
        ));
    }
    let seed0 = u64::from_str_radix(&stream.state_hex[0], 16)
        .map_err(|error| format!("invalid RNG seed0 for {}: {error}", stream.owner))?;
    let seed1 = u64::from_str_radix(&stream.state_hex[1], 16)
        .map_err(|error| format!("invalid RNG seed1 for {}: {error}", stream.owner))?;
    let counter = u32::try_from(stream.counter)
        .map_err(|_| format!("RNG counter does not fit u32 for {}", stream.owner))?;
    Ok(StsRng::from_state(seed0, seed1, counter))
}

fn project_rng_streams(pool: &sts_core::runtime::rng::RngPool) -> Vec<RngStreamProjection> {
    [
        (AI_RNG, "floor", &pool.ai_rng),
        (SHUFFLE_RNG, "floor", &pool.shuffle_rng),
        (CARD_RANDOM_RNG, "floor", &pool.card_random_rng),
        (MISC_RNG, "floor", &pool.misc_rng),
        (MONSTER_HP_RNG, "floor", &pool.monster_hp_rng),
        (POTION_RNG, "run", &pool.potion_rng),
    ]
    .into_iter()
    .map(|(owner, scope, rng)| RngStreamProjection {
        owner: owner.to_owned(),
        algorithm: "com.badlogic.gdx.math.RandomXS128".to_owned(),
        scope: scope.to_owned(),
        state_hex: vec![format!("{:016X}", rng.seed0), format!("{:016X}", rng.seed1)],
        counter: i64::from(rng.counter),
    })
    .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}

fn absolute_display(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(path)
    };
    Ok(absolute.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests;

mod card_birth;
pub use card_birth::{
    reconcile_card_birth_witness_v1, CardBirthTransitionCauseV1, CardBirthTransitionContextV1,
    JavaCardBirthObservedCardV1, ReconciledCardBirthProductV1,
};

mod corruption_chain;
pub use corruption_chain::*;
mod letter_opener;
pub use letter_opener::*;
mod lifecycle;
mod natural;
pub use natural::*;
mod ordered_card_play;
pub use ordered_card_play::*;
mod suite;
pub use suite::*;

mod trajectory;
pub use trajectory::*;
