use serde::{Deserialize, Serialize};

use crate::content::cards::CardId;
use crate::content::potions::{Potion, PotionId};
use crate::content::relics::RelicState;
use crate::eval::fingerprint::StateFingerprintV3;
use crate::runtime::combat::{
    CombatCard, CombatPhase, EphemeralCounters, OrbEntity, OrbId, Power, StanceId,
    ThiefRuntimeState,
};
use crate::runtime::monster_move::{MonsterMoveSpec, MonsterTurnSteps};
use crate::sim::combat::CombatTerminal;
use crate::sim::combat_action_surface::{
    CombatSelectionActionFamilyV2, CombatSelectionDomainCandidateV2, CombatSelectionReasonV2,
    CombatSelectionStatusV2,
};
use crate::sim::combat_projection::MonsterMovePreview;
use crate::state::core::{ClientInput, PileType};
use std::collections::BTreeMap;

pub const ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_NAME: &str = "OracleAnalysisCombatScratch";
pub const ORACLE_ANALYSIS_COMBAT_SCRATCH_SCHEMA_VERSION: u32 = 1;
pub const ORACLE_ANALYSIS_COMBAT_SCRATCH_DECISION_DELTA_KIND: &str =
    "combat_scratch_decision_delta_v1";
pub const ORACLE_ANALYSIS_COMBAT_SCRATCH_NAVIGATION_KIND: &str = "combat_scratch_navigation_v1";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchNavigationV1 {
    pub kind: String,
    pub run_node_id: usize,
    pub source_scratch_node_id: u64,
    pub cursor_scratch_node_id: u64,
    pub scratch_node_count: usize,
    pub parent_scratch_node_id: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchNodeCheckpointV1 {
    pub scratch_node_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_scratch_node_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ClientInput>,
    pub exact_state_hash: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchCheckpointV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub run_node_id: usize,
    pub root_exact_state_hash: String,
    pub max_engine_steps_per_transition: usize,
    pub cursor_scratch_node_id: u64,
    pub next_scratch_node_id: u64,
    pub nodes: Vec<OracleAnalysisCombatScratchNodeCheckpointV1>,
    #[serde(default)]
    pub baseline_source: OracleAnalysisCombatLineLabBaselineSourceV1,
    #[serde(default = "combat_line_lab_root_path")]
    pub baseline_scratch_node_ids: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_scratch_node_id: Option<u64>,
}

fn combat_line_lab_root_path() -> Vec<u64> {
    vec![0]
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCombatLineLabBaselineSourceV1 {
    #[default]
    Root,
    ResidentIncumbent,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCombatLineLabLineV1 {
    Baseline,
    #[default]
    Current,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchContextV1 {
    pub act: u8,
    pub floor: i32,
    pub gold: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchCardV1 {
    #[serde(flatten)]
    pub card: CombatCard,
    pub effective_cost: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OracleAnalysisCombatScratchActionSelectorV1 {
    Atomic {
        scratch_node_id: u64,
        action_index: usize,
    },
    Selection {
        scratch_node_id: u64,
        family_index: usize,
        input_index: usize,
    },
    Card {
        scratch_node_id: u64,
        card_lineage: crate::runtime::combat::CardLineageId,
        target: Option<usize>,
    },
    HandCard {
        scratch_node_id: u64,
        hand_index: usize,
        target_index: Option<usize>,
    },
    Potion {
        scratch_node_id: u64,
        potion_uuid: u32,
        target: Option<usize>,
    },
    PotionSlot {
        scratch_node_id: u64,
        potion_slot: usize,
        target_index: Option<usize>,
    },
    EndTurn {
        scratch_node_id: u64,
    },
}

impl OracleAnalysisCombatScratchActionSelectorV1 {
    pub(super) fn scratch_node_id(self) -> u64 {
        match self {
            Self::Atomic {
                scratch_node_id, ..
            }
            | Self::Selection {
                scratch_node_id, ..
            }
            | Self::Card {
                scratch_node_id, ..
            }
            | Self::HandCard {
                scratch_node_id, ..
            }
            | Self::Potion {
                scratch_node_id, ..
            }
            | Self::PotionSlot {
                scratch_node_id, ..
            }
            | Self::EndTurn { scratch_node_id } => scratch_node_id,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchActionV1 {
    pub selector: OracleAnalysisCombatScratchActionSelectorV1,
    pub action_ref: String,
    pub action_key: String,
    pub input: ClientInput,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchSelectionFamilyV1 {
    pub family_index: usize,
    pub family: CombatSelectionActionFamilyV2,
    pub total_input_count: usize,
    pub page_offset: usize,
    pub page_limit: usize,
    pub next_page_offset: Option<usize>,
    pub actions: Vec<OracleAnalysisCombatScratchActionV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchActionSurfaceV1 {
    pub exact_state_hash: String,
    pub atomic_actions: Vec<OracleAnalysisCombatScratchActionV1>,
    pub selection_families: Vec<OracleAnalysisCombatScratchSelectionFamilyV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchPlayerV1 {
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub energy: u8,
    pub stance: StanceId,
    pub orbs: Vec<OrbEntity>,
    pub relics: Vec<RelicState>,
    pub powers: Vec<Power>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchMonsterV1 {
    pub entity_id: usize,
    pub slot: u8,
    pub label: String,
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub is_dying: bool,
    pub is_escaped: bool,
    pub half_dead: bool,
    pub planned_move_id: u8,
    pub planned_steps: MonsterTurnSteps,
    pub intent: Option<MonsterMoveSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_preview: Option<MonsterMovePreview>,
    pub thief: ThiefRuntimeState,
    pub powers: Vec<Power>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchPositionV1 {
    pub fingerprint: StateFingerprintV3,
    pub terminal: CombatTerminal,
    pub turn: u32,
    pub phase: CombatPhase,
    pub counters: EphemeralCounters,
    pub player: OracleAnalysisCombatScratchPlayerV1,
    pub hand: Vec<OracleAnalysisCombatScratchCardV1>,
    pub draw_pile_top_first: Vec<OracleAnalysisCombatScratchCardV1>,
    pub discard_pile: Vec<OracleAnalysisCombatScratchCardV1>,
    pub exhaust_pile: Vec<OracleAnalysisCombatScratchCardV1>,
    pub limbo: Vec<OracleAnalysisCombatScratchCardV1>,
    pub potions: Vec<Option<Potion>>,
    pub monsters: Vec<OracleAnalysisCombatScratchMonsterV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchViewV1 {
    pub run_node_id: usize,
    pub context: OracleAnalysisCombatScratchContextV1,
    pub root_exact_state_hash: String,
    pub max_engine_steps_per_transition: usize,
    pub cursor_scratch_node_id: u64,
    pub scratch_node_count: usize,
    pub parent_scratch_node_id: Option<u64>,
    pub input_from_parent: Option<ClientInput>,
    pub position: OracleAnalysisCombatScratchPositionV1,
    pub legal_actions: OracleAnalysisCombatScratchActionSurfaceV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionActionV1 {
    pub index: usize,
    pub input: ClientInput,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionSelectionFamilyV1 {
    pub family_index: usize,
    pub reason: CombatSelectionReasonV2,
    pub source_pile: Option<PileType>,
    pub domain: Vec<OracleAnalysisCombatScratchDecisionSelectionCandidateV1>,
    pub raw_domain_count: u64,
    pub eligible_domain_count: u64,
    pub max_distinct_selection_count: u64,
    pub declared_min: u64,
    pub declared_max: u64,
    pub effective_max: u64,
    pub selection_status: CombatSelectionStatusV2,
    pub total_input_count: usize,
    pub page_offset: usize,
    pub page_limit: usize,
    pub next_page_offset: Option<usize>,
    pub actions: Vec<OracleAnalysisCombatScratchDecisionSelectionActionV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionSelectionCandidateV1 {
    pub domain_index: usize,
    pub card_id: Option<CardId>,
    pub upgrades: Option<u8>,
    pub eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionSelectionActionV1 {
    pub index: usize,
    pub selected_domain_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionCardV1 {
    pub id: CardId,
    #[serde(default, skip_serializing_if = "scratch_zero_u8")]
    pub upgrades: u8,
    #[serde(default, skip_serializing_if = "scratch_zero_i32")]
    pub misc_value: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_damage_override: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_block_override: Option<i32>,
    #[serde(default, skip_serializing_if = "scratch_zero_i8")]
    pub cost_modifier: i8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_for_turn: Option<u8>,
    #[serde(default, skip_serializing_if = "scratch_zero_i32")]
    pub base_damage_mut: i32,
    #[serde(default, skip_serializing_if = "scratch_zero_i32")]
    pub base_block_mut: i32,
    #[serde(default, skip_serializing_if = "scratch_zero_i32")]
    pub base_magic_num_mut: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multi_damage: Vec<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_override: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_override: Option<bool>,
    #[serde(default, skip_serializing_if = "scratch_false")]
    pub free_to_play_once: bool,
    #[serde(default, skip_serializing_if = "scratch_zero_i32")]
    pub energy_on_use: i32,
    pub effective_cost: i32,
}

impl From<OracleAnalysisCombatScratchCardV1> for OracleAnalysisCombatScratchDecisionCardV1 {
    fn from(value: OracleAnalysisCombatScratchCardV1) -> Self {
        let card = value.card;
        Self {
            id: card.id,
            upgrades: card.upgrades,
            misc_value: card.misc_value,
            base_damage_override: card.base_damage_override,
            base_block_override: card.base_block_override,
            cost_modifier: card.cost_modifier,
            cost_for_turn: card.cost_for_turn,
            base_damage_mut: card.base_damage_mut,
            base_block_mut: card.base_block_mut,
            base_magic_num_mut: card.base_magic_num_mut,
            multi_damage: card.multi_damage.into_iter().collect(),
            exhaust_override: card.exhaust_override,
            retain_override: card.retain_override,
            free_to_play_once: card.free_to_play_once,
            energy_on_use: card.energy_on_use,
            effective_cost: value.effective_cost,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionHandCardV1 {
    pub hand_index: usize,
    #[serde(flatten)]
    pub card: OracleAnalysisCombatScratchDecisionCardV1,
    #[serde(default, skip_serializing_if = "scratch_false")]
    pub playable_without_target: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub playable_target_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionPotionV1 {
    pub potion_slot: usize,
    pub id: PotionId,
    pub can_use: bool,
    pub can_discard: bool,
    pub requires_target: bool,
    #[serde(default, skip_serializing_if = "scratch_false")]
    pub usable_without_target: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usable_target_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionMonsterV1 {
    pub monster_index: usize,
    pub label: String,
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub is_dying: bool,
    pub is_escaped: bool,
    pub half_dead: bool,
    pub planned_move_id: u8,
    pub planned_steps: MonsterTurnSteps,
    pub intent: Option<MonsterMoveSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_preview: Option<MonsterMovePreview>,
    pub thief: ThiefRuntimeState,
    pub powers: Vec<Power>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionViewV1 {
    pub run_node_id: usize,
    pub context: OracleAnalysisCombatScratchContextV1,
    pub cursor_scratch_node_id: u64,
    pub scratch_node_count: usize,
    pub parent_scratch_node_id: Option<u64>,
    pub terminal: CombatTerminal,
    pub turn: u32,
    pub phase: CombatPhase,
    pub counters: EphemeralCounters,
    pub player: OracleAnalysisCombatScratchPlayerV1,
    pub hand: Vec<OracleAnalysisCombatScratchDecisionHandCardV1>,
    pub draw_pile_top_first: Vec<OracleAnalysisCombatScratchDecisionCardV1>,
    pub discard_pile: Vec<OracleAnalysisCombatScratchDecisionCardV1>,
    pub exhaust_pile: Vec<OracleAnalysisCombatScratchDecisionCardV1>,
    pub potions: Vec<OracleAnalysisCombatScratchDecisionPotionV1>,
    pub monsters: Vec<OracleAnalysisCombatScratchDecisionMonsterV1>,
    pub atomic_actions: Vec<OracleAnalysisCombatScratchDecisionActionV1>,
    pub selection_families: Vec<OracleAnalysisCombatScratchDecisionSelectionFamilyV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchSequenceDeltaV1<T> {
    pub base_len: usize,
    pub retain_prefix: usize,
    pub remove_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub insert: Vec<T>,
    pub result_len: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchCountersDeltaV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cards_played_this_turn: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacks_played_this_turn: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cards_discarded_this_turn: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_ids_played_this_turn: Option<Vec<CardId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_ids_played_this_combat: Option<Vec<CardId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbs_channeled_this_turn: Option<Vec<OrbId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbs_channeled_this_combat: Option<Vec<OrbId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mantra_gained_this_combat: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub times_damaged_this_combat: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victory_triggered: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_cost_for_turn: Option<Option<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub early_end_turn_pending: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_monster_turn_pending: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_escaping: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape_pending_reward: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchPlayerDeltaV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_hp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stance: Option<StanceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbs: Option<Vec<OrbEntity>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relics: Option<Vec<RelicState>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub powers: Option<Vec<Power>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchMonsterDeltaV1 {
    pub monster_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_hp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_dying: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_escaped: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub half_dead: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_move_id: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_steps: Option<MonsterTurnSteps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<Option<MonsterMoveSpec>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_preview: Option<Option<MonsterMovePreview>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thief: Option<ThiefRuntimeState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub powers: Option<Vec<Power>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchDecisionDeltaV1 {
    pub kind: String,
    pub run_node_id: usize,
    pub base_scratch_node_id: u64,
    pub cursor_scratch_node_id: u64,
    pub scratch_node_count: usize,
    pub parent_scratch_node_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<OracleAnalysisCombatScratchContextV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<CombatTerminal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<CombatPhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counters: Option<OracleAnalysisCombatScratchCountersDeltaV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<OracleAnalysisCombatScratchPlayerDeltaV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<Vec<OracleAnalysisCombatScratchDecisionHandCardV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_pile_top_first: Option<
        OracleAnalysisCombatScratchSequenceDeltaV1<OracleAnalysisCombatScratchDecisionCardV1>,
    >,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard_pile: Option<
        OracleAnalysisCombatScratchSequenceDeltaV1<OracleAnalysisCombatScratchDecisionCardV1>,
    >,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_pile: Option<
        OracleAnalysisCombatScratchSequenceDeltaV1<OracleAnalysisCombatScratchDecisionCardV1>,
    >,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_potion_slots: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub potion_upserts: Vec<OracleAnalysisCombatScratchDecisionPotionV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monsters: Option<Vec<OracleAnalysisCombatScratchDecisionMonsterV1>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monster_updates: Vec<OracleAnalysisCombatScratchMonsterDeltaV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atomic_actions: Option<Vec<OracleAnalysisCombatScratchDecisionActionV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_families: Option<Vec<OracleAnalysisCombatScratchDecisionSelectionFamilyV1>>,
}

impl From<OracleAnalysisCombatScratchViewV1> for OracleAnalysisCombatScratchDecisionViewV1 {
    fn from(view: OracleAnalysisCombatScratchViewV1) -> Self {
        let position = view.position;
        let monster_indices = position
            .monsters
            .iter()
            .enumerate()
            .map(|(monster_index, monster)| (monster.entity_id, monster_index))
            .collect::<BTreeMap<_, _>>();
        let atomic_inputs = view
            .legal_actions
            .atomic_actions
            .iter()
            .map(|action| &action.input)
            .collect::<Vec<_>>();
        let hand = position
            .hand
            .into_iter()
            .enumerate()
            .map(
                |(hand_index, card)| OracleAnalysisCombatScratchDecisionHandCardV1 {
                    hand_index,
                    card: card.into(),
                    playable_without_target: atomic_inputs.iter().any(|input| {
                        matches!(
                            input,
                            ClientInput::PlayCard {
                                card_index,
                                target: None,
                            } if *card_index == hand_index
                        )
                    }),
                    playable_target_indices: local_target_indices(
                        &atomic_inputs,
                        &monster_indices,
                        |input| match input {
                            ClientInput::PlayCard { card_index, target }
                                if *card_index == hand_index =>
                            {
                                *target
                            }
                            _ => None,
                        },
                    ),
                },
            )
            .collect();
        let potions = position
            .potions
            .into_iter()
            .enumerate()
            .filter_map(|(potion_slot, potion)| {
                potion.map(|potion| OracleAnalysisCombatScratchDecisionPotionV1 {
                    potion_slot,
                    id: potion.id,
                    can_use: potion.can_use,
                    can_discard: potion.can_discard,
                    requires_target: potion.requires_target,
                    usable_without_target: atomic_inputs.iter().any(|input| {
                        matches!(
                            input,
                            ClientInput::UsePotion {
                                potion_index,
                                target: None,
                            } if *potion_index == potion_slot
                        )
                    }),
                    usable_target_indices: local_target_indices(
                        &atomic_inputs,
                        &monster_indices,
                        |input| match input {
                            ClientInput::UsePotion {
                                potion_index,
                                target,
                            } if *potion_index == potion_slot => *target,
                            _ => None,
                        },
                    ),
                })
            })
            .collect();
        let monsters = position
            .monsters
            .into_iter()
            .enumerate()
            .map(
                |(monster_index, monster)| OracleAnalysisCombatScratchDecisionMonsterV1 {
                    monster_index,
                    label: monster.label,
                    current_hp: monster.current_hp,
                    max_hp: monster.max_hp,
                    block: monster.block,
                    is_dying: monster.is_dying,
                    is_escaped: monster.is_escaped,
                    half_dead: monster.half_dead,
                    planned_move_id: monster.planned_move_id,
                    planned_steps: monster.planned_steps,
                    intent: monster.intent,
                    move_preview: monster.move_preview,
                    thief: monster.thief,
                    powers: monster.powers,
                },
            )
            .collect();
        Self {
            run_node_id: view.run_node_id,
            context: view.context,
            cursor_scratch_node_id: view.cursor_scratch_node_id,
            scratch_node_count: view.scratch_node_count,
            parent_scratch_node_id: view.parent_scratch_node_id,
            terminal: position.terminal,
            turn: position.turn,
            phase: position.phase,
            counters: position.counters,
            player: position.player,
            hand,
            draw_pile_top_first: position
                .draw_pile_top_first
                .into_iter()
                .map(Into::into)
                .collect(),
            discard_pile: position.discard_pile.into_iter().map(Into::into).collect(),
            exhaust_pile: position.exhaust_pile.into_iter().map(Into::into).collect(),
            potions,
            monsters,
            atomic_actions: view
                .legal_actions
                .atomic_actions
                .into_iter()
                .filter(|action| {
                    !matches!(
                        &action.input,
                        ClientInput::EndTurn
                            | ClientInput::PlayCard { .. }
                            | ClientInput::UsePotion { .. }
                    )
                })
                .map(|action| {
                    let OracleAnalysisCombatScratchActionSelectorV1::Atomic {
                        action_index, ..
                    } = action.selector
                    else {
                        unreachable!("atomic action carries an atomic selector")
                    };
                    OracleAnalysisCombatScratchDecisionActionV1 {
                        index: action_index,
                        input: action.input,
                    }
                })
                .collect(),
            selection_families: view
                .legal_actions
                .selection_families
                .into_iter()
                .map(decision_selection_family)
                .collect(),
        }
    }
}

fn decision_selection_family(
    family: OracleAnalysisCombatScratchSelectionFamilyV1,
) -> OracleAnalysisCombatScratchDecisionSelectionFamilyV1 {
    let card_domain_indices = family
        .family
        .raw_domain
        .iter()
        .enumerate()
        .filter_map(|(domain_index, candidate)| match candidate {
            CombatSelectionDomainCandidateV2::CardLineage { lineage, .. } => {
                Some((*lineage, domain_index))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let scry_domain_indices = family
        .family
        .raw_domain
        .iter()
        .enumerate()
        .filter_map(|(domain_index, candidate)| match candidate {
            CombatSelectionDomainCandidateV2::ScryIndex { index, .. } => usize::try_from(*index)
                .ok()
                .map(|index| (index, domain_index)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let domain = family
        .family
        .raw_domain
        .iter()
        .enumerate()
        .map(|(domain_index, candidate)| match candidate {
            CombatSelectionDomainCandidateV2::CardLineage {
                card_id,
                upgrades,
                eligible,
                ..
            } => OracleAnalysisCombatScratchDecisionSelectionCandidateV1 {
                domain_index,
                card_id: *card_id,
                upgrades: *upgrades,
                eligible: *eligible,
            },
            CombatSelectionDomainCandidateV2::ScryIndex {
                card_id,
                currently_present,
                ..
            } => OracleAnalysisCombatScratchDecisionSelectionCandidateV1 {
                domain_index,
                card_id: *card_id,
                upgrades: None,
                eligible: *currently_present,
            },
        })
        .collect();
    let actions = family
        .actions
        .into_iter()
        .map(|action| {
            let OracleAnalysisCombatScratchActionSelectorV1::Selection { input_index, .. } =
                action.selector
            else {
                unreachable!("structured action carries a selection selector")
            };
            let selected_domain_indices = match action.input {
                ClientInput::SubmitSelection(resolution) => resolution
                    .selected_card_lineages()
                    .into_iter()
                    .map(|uuid| {
                        *card_domain_indices
                            .get(&uuid)
                            .expect("structured card input belongs to its exact domain")
                    })
                    .collect(),
                ClientInput::SubmitScryDiscard(indices) => indices
                    .into_iter()
                    .map(|index| {
                        *scry_domain_indices
                            .get(&index)
                            .expect("structured scry input belongs to its exact domain")
                    })
                    .collect(),
                _ => Vec::new(),
            };
            OracleAnalysisCombatScratchDecisionSelectionActionV1 {
                index: input_index,
                selected_domain_indices,
            }
        })
        .collect();
    let exact = family.family;
    OracleAnalysisCombatScratchDecisionSelectionFamilyV1 {
        family_index: family.family_index,
        reason: exact.reason,
        source_pile: exact.source_pile,
        domain,
        raw_domain_count: exact.raw_domain_count,
        eligible_domain_count: exact.eligible_domain_count,
        max_distinct_selection_count: exact.max_distinct_selection_count,
        declared_min: exact.declared_min,
        declared_max: exact.declared_max,
        effective_max: exact.effective_max,
        selection_status: exact.selection_status,
        total_input_count: family.total_input_count,
        page_offset: family.page_offset,
        page_limit: family.page_limit,
        next_page_offset: family.next_page_offset,
        actions,
    }
}

fn local_target_indices(
    inputs: &[&ClientInput],
    monster_indices: &BTreeMap<usize, usize>,
    target_for: impl Fn(&ClientInput) -> Option<usize>,
) -> Vec<usize> {
    let mut indices = inputs
        .iter()
        .filter_map(|input| target_for(input))
        .filter_map(|entity_id| monster_indices.get(&entity_id).copied())
        .collect::<Vec<_>>();
    indices.sort_unstable();
    indices.dedup();
    indices
}

fn scratch_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn scratch_zero_i8(value: &i8) -> bool {
    *value == 0
}

fn scratch_zero_i32(value: &i32) -> bool {
    *value == 0
}

fn scratch_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchTreeNodeV1 {
    pub scratch_node_id: u64,
    pub parent_scratch_node_id: Option<u64>,
    pub is_cursor: bool,
    pub input_from_parent: Option<ClientInput>,
    pub action_key_from_parent: Option<String>,
    pub exact_state_hash: String,
    pub terminal: CombatTerminal,
    pub turn: u32,
    pub player_hp: i32,
    pub player_block: i32,
    pub enemy_hp_total: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchTreeV1 {
    pub run_node_id: usize,
    pub root_exact_state_hash: String,
    pub cursor_scratch_node_id: u64,
    pub nodes: Vec<OracleAnalysisCombatScratchTreeNodeV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchSearchRequestV1 {
    pub max_quanta: usize,
    pub quantum_generation_work: usize,
    pub quantum_ms: u64,
    pub wall_ms: u64,
}

impl Default for OracleAnalysisCombatScratchSearchRequestV1 {
    fn default() -> Self {
        Self {
            max_quanta: 4,
            quantum_generation_work: 1_024,
            quantum_ms: 100,
            wall_ms: 1_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCombatScratchSearchExitV1 {
    WitnessAdded,
    PortfolioCompleteWithoutWitness,
    AllowanceExhausted,
    DeadlineReached,
    QuantumLimitReached,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatScratchSearchReportV1 {
    pub run_node_id: usize,
    pub source_scratch_node_id: u64,
    pub search_root_exact_state_hash: String,
    pub exit: OracleAnalysisCombatScratchSearchExitV1,
    pub quanta_served: usize,
    pub elapsed_ms: u64,
    pub generation_work: u64,
    pub exact_states: usize,
    pub completed_turn_options: usize,
    pub max_player_turn: u32,
    pub last_status: Option<String>,
    pub additional_potions_allowed: u32,
    pub appended_action_count: usize,
    pub first_appended_scratch_node_id: Option<u64>,
    pub terminal_scratch_node_id: Option<u64>,
}

pub const ORACLE_ANALYSIS_COMBAT_LINE_LAB_DELTA_KIND: &str = "combat_line_lab_decision_delta_v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabLocationV1 {
    pub action_index: usize,
    pub turn: u32,
    pub action_in_turn: usize,
    pub on_baseline: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabFrameV1 {
    pub run_node_id: usize,
    pub context: OracleAnalysisCombatScratchContextV1,
    pub baseline_source: OracleAnalysisCombatLineLabBaselineSourceV1,
    pub baseline_action_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_line: Option<OracleAnalysisCombatLineLabLineSummaryV1>,
    pub location: OracleAnalysisCombatLineLabLocationV1,
    pub terminal: CombatTerminal,
    pub turn: u32,
    pub phase: CombatPhase,
    pub counters: EphemeralCounters,
    pub player: OracleAnalysisCombatScratchPlayerV1,
    pub hand: Vec<OracleAnalysisCombatScratchDecisionHandCardV1>,
    pub draw_pile_top_first: Vec<OracleAnalysisCombatScratchDecisionCardV1>,
    pub discard_pile: Vec<OracleAnalysisCombatScratchDecisionCardV1>,
    pub exhaust_pile: Vec<OracleAnalysisCombatScratchDecisionCardV1>,
    pub potions: Vec<OracleAnalysisCombatScratchDecisionPotionV1>,
    pub monsters: Vec<OracleAnalysisCombatScratchDecisionMonsterV1>,
    pub atomic_actions: Vec<OracleAnalysisCombatScratchDecisionActionV1>,
    pub selection_families: Vec<OracleAnalysisCombatScratchDecisionSelectionFamilyV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabDecisionDeltaV1 {
    pub kind: String,
    pub run_node_id: usize,
    pub from: OracleAnalysisCombatLineLabLocationV1,
    pub to: OracleAnalysisCombatLineLabLocationV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<CombatTerminal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<CombatPhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counters: Option<OracleAnalysisCombatScratchCountersDeltaV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<OracleAnalysisCombatScratchPlayerDeltaV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<Vec<OracleAnalysisCombatScratchDecisionHandCardV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_pile_top_first: Option<
        OracleAnalysisCombatScratchSequenceDeltaV1<OracleAnalysisCombatScratchDecisionCardV1>,
    >,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard_pile: Option<
        OracleAnalysisCombatScratchSequenceDeltaV1<OracleAnalysisCombatScratchDecisionCardV1>,
    >,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_pile: Option<
        OracleAnalysisCombatScratchSequenceDeltaV1<OracleAnalysisCombatScratchDecisionCardV1>,
    >,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_potion_slots: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub potion_upserts: Vec<OracleAnalysisCombatScratchDecisionPotionV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monsters: Option<Vec<OracleAnalysisCombatScratchDecisionMonsterV1>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monster_updates: Vec<OracleAnalysisCombatScratchMonsterDeltaV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atomic_actions: Option<Vec<OracleAnalysisCombatScratchDecisionActionV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_families: Option<Vec<OracleAnalysisCombatScratchDecisionSelectionFamilyV1>>,
}

impl OracleAnalysisCombatLineLabDecisionDeltaV1 {
    pub(super) fn from_scratch(
        from: OracleAnalysisCombatLineLabLocationV1,
        to: OracleAnalysisCombatLineLabLocationV1,
        delta: OracleAnalysisCombatScratchDecisionDeltaV1,
    ) -> Self {
        Self {
            kind: ORACLE_ANALYSIS_COMBAT_LINE_LAB_DELTA_KIND.to_string(),
            run_node_id: delta.run_node_id,
            from,
            to,
            terminal: delta.terminal,
            turn: delta.turn,
            phase: delta.phase,
            counters: delta.counters,
            player: delta.player,
            hand: delta.hand,
            draw_pile_top_first: delta.draw_pile_top_first,
            discard_pile: delta.discard_pile,
            exhaust_pile: delta.exhaust_pile,
            removed_potion_slots: delta.removed_potion_slots,
            potion_upserts: delta.potion_upserts,
            monsters: delta.monsters,
            monster_updates: delta.monster_updates,
            atomic_actions: delta.atomic_actions,
            selection_families: delta.selection_families,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabCardCandidateV1 {
    pub occurrence: usize,
    pub hand_index: usize,
    pub upgrades: u8,
    pub effective_cost: i32,
    pub playable_without_target: bool,
    pub playable_target_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OracleAnalysisCombatLineLabPlayCardResultV1 {
    Played {
        input: ClientInput,
        delta: OracleAnalysisCombatLineLabDecisionDeltaV1,
    },
    AmbiguousCard {
        card_id: CardId,
        candidates: Vec<OracleAnalysisCombatLineLabCardCandidateV1>,
    },
    AmbiguousTarget {
        card_id: CardId,
        occurrence: usize,
        playable_target_indices: Vec<usize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabPotionCandidateV1 {
    pub occurrence: usize,
    pub potion_slot: usize,
    pub can_use: bool,
    pub requires_target: bool,
    pub usable_without_target: bool,
    pub usable_target_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OracleAnalysisCombatLineLabUsePotionResultV1 {
    Used {
        input: ClientInput,
        delta: OracleAnalysisCombatLineLabDecisionDeltaV1,
    },
    AmbiguousPotion {
        potion_id: PotionId,
        candidates: Vec<OracleAnalysisCombatLineLabPotionCandidateV1>,
    },
    AmbiguousTarget {
        potion_id: PotionId,
        occurrence: usize,
        usable_target_indices: Vec<usize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabTurnSummaryV1 {
    pub turn: u32,
    pub action_count: usize,
    pub start_hp: i32,
    pub end_hp: i32,
    pub end_block: i32,
    pub enemy_hp_total: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabLineSummaryV1 {
    pub terminal: CombatTerminal,
    pub suffix_known: bool,
    pub action_count: usize,
    pub initial_hp: i32,
    pub final_hp: i32,
    pub potions_used: usize,
    pub turns: Vec<OracleAnalysisCombatLineLabTurnSummaryV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OracleAnalysisCombatLineLabActionV1 {
    PlayCard {
        card_id: CardId,
        upgrades: u8,
        hand_index: usize,
        target_index: Option<usize>,
    },
    UsePotion {
        potion_id: PotionId,
        potion_slot: usize,
        target_index: Option<usize>,
    },
    DiscardPotion {
        potion_id: PotionId,
        potion_slot: usize,
    },
    EndTurn,
    Other {
        input: ClientInput,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabDivergenceV1 {
    pub action_index: usize,
    pub baseline_action: Option<OracleAnalysisCombatLineLabActionV1>,
    pub current_action: Option<OracleAnalysisCombatLineLabActionV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabActionSummaryV1 {
    pub action_index: usize,
    pub turn: u32,
    pub action_in_turn: usize,
    pub action: OracleAnalysisCombatLineLabActionV1,
    pub result_hp: i32,
    pub result_block: i32,
    pub result_enemy_hp_total: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabCompareV1 {
    pub run_node_id: usize,
    pub baseline_source: OracleAnalysisCombatLineLabBaselineSourceV1,
    pub common_prefix_actions: usize,
    pub first_divergence: Option<OracleAnalysisCombatLineLabDivergenceV1>,
    pub baseline: OracleAnalysisCombatLineLabLineSummaryV1,
    pub current: OracleAnalysisCombatLineLabLineSummaryV1,
    pub baseline_tail: Vec<OracleAnalysisCombatLineLabActionSummaryV1>,
    pub current_tail: Vec<OracleAnalysisCombatLineLabActionSummaryV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCombatLineLabOpenV1 {
    pub baseline: OracleAnalysisCombatLineLabLineSummaryV1,
    pub frame: OracleAnalysisCombatLineLabFrameV1,
}
