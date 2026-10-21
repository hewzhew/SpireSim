use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ai::combat_witness_contract::{
    CombatWitnessPotionPolicyV1, CombatWitnessSatisfactionV1,
};
use crate::content::monsters::EnemyId;
use crate::state::core::{EngineState, RunResult};

use super::oracle_combat_witness_budget::{
    OracleRunCombatWitnessBudgetsV1, OracleRunCombatWitnessQualityPolicyV1,
};
use super::oracle_combat_witness_contract::{
    OracleCombatWitnessCheckpointV1, OracleCombatWitnessFailureV1,
};
use super::oracle_resident_combat_witness_job::OracleResidentCombatWitnessJobV1;
use super::oracle_selection_cursor::LazyUnorderedSelectionCursorV1;
use super::{
    oracle_active_victory_potion_slot_mask_v1, NeowOracleExpansionV1, OracleCombatWitnessAdvanceV1,
    OracleCombatWitnessOptionsV1, OracleCombatWitnessQuantumV1, RunControlHpLossLimit,
    RunControlSession, RunControlSessionCheckpointV1, RunControlTraceAnnotationV1,
    RunDecisionAction, RunPolicyPriorFnV1, RunProgressJournalV1, RunProgressStepV1,
    StrategicProbeShadowOrderKeyV1,
};

mod branch_scheduling;
mod checkpoint;
mod checkpoint_restore;
mod combat_completion;
mod decision_materialization;
mod decision_supply;
mod explicit_transactions;
mod scheduling;

pub use checkpoint::{
    OracleRunActiveCombatCheckpointV1, OracleRunBranchCheckpointV1, OracleRunCheckpointPayloadsV1,
    OracleRunDeferredCombatCheckpointV1, OracleRunExplorerCheckpointV1,
    OracleRunJournalNodeCheckpointV1, OracleRunSessionPayloadRefsV1,
};
pub use checkpoint_restore::seed_oracle_run_explorer_from_checkpoint_v1;
use combat_completion::FinishedOracleCombatV1;
pub use combat_completion::OracleRunCombatEvidenceKindV1;
use decision_supply::decision_supply_for_branch;
pub use explicit_transactions::{
    OracleRunExplorerCombatCommitV1, OracleRunExplorerDecisionCommitV1,
    OracleRunExplorerExplicitTransactionsV1,
};
use scheduling::PreparedScheduledOracleRunWorkV1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleRunWorkKindV1 {
    MapTravel,
    RewardAction,
    EventOption,
    ShopAction,
    CampfireAction,
    RunChoice,
    TreasureAction,
    BossRelicChoice,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleRunBoundaryV1 {
    MapDecision,
    Combat,
    Reward,
    Event,
    Shop,
    Campfire,
    RunChoice,
    Treasure,
    BossRelic,
    TerminalVictory,
    TerminalDefeat,
    DoubleBossComplete,
    Act3VictoryComplete,
    TrueVictoryComplete,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OracleRunReplayStepV1 {
    pub candidate_id: String,
    pub label: String,
    pub action: RunDecisionAction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LazyOracleRunDecisionV1 {
    pub parent_branch_id: usize,
    pub parent_state_fingerprint: String,
    pub neow_root_candidate_id: String,
    pub kind: OracleRunWorkKindV1,
    pub candidate_id: String,
    pub label: String,
    pub action: RunDecisionAction,
    pub stable_work_key: String,
    pub path_negative_log_policy: f64,
    pub path_discrepancy: u64,
    pub path_depth: u64,
    #[serde(default)]
    pub parent_act: u8,
    #[serde(default)]
    pub parent_floor: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_edge_probe: Option<OracleRunCombatEdgeProbeV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LazyOracleRunSelectionFamilyV1 {
    pub family_key: String,
    pub parent_branch_id: usize,
    pub parent_state_fingerprint: String,
    pub neow_root_candidate_id: String,
    pub kind: OracleRunWorkKindV1,
    pub candidate_id: String,
    pub label: String,
    pub path_negative_log_policy: f64,
    pub path_discrepancy: u64,
    pub path_depth: u64,
    pub parent_act: u8,
    pub parent_floor: i32,
    pub public_probability: f64,
    cursor: LazyUnorderedSelectionCursorV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outstanding_work_key: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum OracleRunCombatEdgeProbeV1 {
    NotImmediateCombat,
    HeuristicEstimate {
        order_key: StrategicProbeShadowOrderKeyV1,
    },
}

#[derive(Clone, Debug)]
pub struct OracleRunBranchV1 {
    pub branch_id: usize,
    pub parent_branch_id: Option<usize>,
    pub neow_root_candidate_id: String,
    pub neow_root_label: String,
    pub state_fingerprint: String,
    pub boundary: OracleRunBoundaryV1,
    pub path_negative_log_policy: f64,
    pub path_discrepancy: u64,
    pub path_depth: u64,
    pub replay: Vec<OracleRunReplayStepV1>,
    pub journal: RunProgressJournalV1,
    pub session: RunControlSession,
}

#[derive(Clone, Debug)]
pub struct ExactDuplicateOracleRunBranchV1 {
    pub branch_id: usize,
    pub parent_branch_id: Option<usize>,
    pub survivor_branch_id: usize,
    pub neow_root_candidate_id: String,
    pub state_fingerprint: String,
    pub replay: Vec<OracleRunReplayStepV1>,
    pub journal: RunProgressJournalV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunUnresolvedCombatV1 {
    pub branch_id: usize,
    pub failure: OracleCombatWitnessFailureV1,
    pub evidence_kind: OracleRunCombatEvidenceKindV1,
    pub last_status: Option<String>,
    /// Exact candidate-generation work consumed by the witness portfolio.
    pub generation_work: u64,
    pub exact_states: usize,
    pub applied_action_transitions: usize,
    pub unique_successor_states: usize,
    pub duplicate_exact_successors: usize,
    pub completed_turn_options: usize,
    pub retained_state_work: usize,
    pub max_player_turn: u32,
    pub max_path_atomic_depth: usize,
    pub generation_gap_count: usize,
    pub incumbent_final_hp: Option<i32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OraclePendingCombatEnemyV1 {
    pub enemy_id: EnemyId,
    pub name: String,
    pub current_hp: i32,
    pub max_hp: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleCombatSearchResumeKindV1 {
    Fresh,
    /// Reserved for a future checkpoint that serializes the tactical frontier.
    SearchResumeExact,
    StateReplayExactSearchRestarted,
}

#[derive(Clone, Debug, Serialize)]
pub struct OraclePendingCombatSummaryV1 {
    pub branch_id: usize,
    pub act: u8,
    pub floor: i32,
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub elite: bool,
    pub boss: bool,
    pub enemies: Vec<OraclePendingCombatEnemyV1>,
    /// Exact candidate-generation work consumed by the witness portfolio.
    pub generation_work: u64,
    pub engine_steps: usize,
    pub exact_states: usize,
    pub applied_action_transitions: usize,
    pub unique_successor_states: usize,
    pub duplicate_exact_successors: usize,
    pub completed_turn_options: usize,
    pub retained_state_work: usize,
    pub queued_anchor_entries: usize,
    pub queued_guided_entries: Vec<usize>,
    pub max_player_turn: u32,
    pub max_path_atomic_depth: usize,
    pub max_completed_turn_options_at_state: usize,
    pub generation_gap_count: usize,
    pub pending_witness_replay: bool,
    pub quantum_count: usize,
    pub last_quantum_generation_work: usize,
    pub last_quantum_engine_steps: usize,
    pub incumbent_final_hp: Option<i32>,
    pub incumbent_hp_loss: Option<i32>,
    pub incumbent_action_count: Option<usize>,
    pub potion_spend_requires_satisfaction: bool,
    pub incumbent_revision: u64,
    pub quanta_since_incumbent_improvement: usize,
    pub last_status: Option<String>,
    pub remaining_generation_work: usize,
    pub remaining_wall_ms: Option<u64>,
    pub resume_kind: OracleCombatSearchResumeKindV1,
    pub restart_count: usize,
}

impl OracleRunCombatWitnessBudgetsV1 {
    pub fn for_session(&self, session: &RunControlSession) -> OracleCombatWitnessOptionsV1 {
        self.for_session_stage(session, 1)
    }

    pub fn for_session_stage(
        &self,
        session: &RunControlSession,
        stage: u8,
    ) -> OracleCombatWitnessOptionsV1 {
        let Some(active) = session.active_combat.as_ref() else {
            return scale_combat_options(self.hallway.clone(), self.stage_divisor(stage));
        };
        let mut options = if active.combat_state.meta.is_boss_fight {
            self.boss.clone()
        } else if active.combat_state.meta.is_elite_fight {
            self.elite.clone()
        } else {
            self.hallway.clone()
        };
        if self.quality_policy == OracleRunCombatWitnessQualityPolicyV1::StrategicRun {
            options.satisfaction = Some(
                if super::strategic_combat_persistent_payoff_matters_v1(session) {
                    CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain
                } else {
                    match super::strategic_combat_quality_hp_loss_limit_v1(session) {
                        RunControlHpLossLimit::Limit(limit) => {
                            CombatWitnessSatisfactionV1::HpLossAtMost(limit)
                        }
                        RunControlHpLossLimit::Unlimited => {
                            CombatWitnessSatisfactionV1::FirstCompleteWin
                        }
                    }
                },
            );
        }
        let uses_potion_stages = self.uses_potion_conserving_primary(session, &options);
        if stage == 0 && uses_potion_stages {
            options.potion_policy = Some(CombatWitnessPotionPolicyV1::Never);
            options.max_potions_used = Some(0);
        }
        if stage == 0
            && uses_potion_stages
            && oracle_active_victory_potion_slot_mask_v1(session) != 0
        {
            return scale_potion_stage_options(
                options,
                self.potion_stage_allowance_divisor(session),
                self.potion_stage_wall_divisor(session),
            );
        }
        scale_combat_options(options, self.stage_divisor(stage))
    }

    pub fn for_session_stage_with_prior(
        &self,
        session: &RunControlSession,
        stage: u8,
        _prior: &OracleCombatWitnessCheckpointV1,
    ) -> OracleCombatWitnessOptionsV1 {
        let mut options = self.for_session_stage(session, stage);
        if stage == 0
            || !self.uses_potion_conserving_primary(session, &self.for_session_stage(session, 1))
        {
            return options;
        }
        options.potion_policy = Some(CombatWitnessPotionPolicyV1::SemanticBudgeted);
        options.max_potions_used = Some(1);
        let active_slots = oracle_active_victory_potion_slot_mask_v1(session);
        if active_slots == 0 {
            options.allowed_potion_slots = Some(0);
            return options;
        }
        // Keep the original high-stakes two-potion surface as a final Boss
        // rescue, after cheaper clean and single-slot searches had an
        // independent chance to produce an exact witness.
        if boss_multi_potion_fallback_stage(session) == Some(stage) {
            options.potion_policy = Some(CombatWitnessPotionPolicyV1::SemanticBudgeted);
            options.max_potions_used = session.active_combat.as_ref().and_then(|active| {
                crate::ai::combat_witness_contract::high_stakes_semantic_witness_potion_budget_v1(
                    &active.combat_state,
                )
            });
            options.allowed_potion_slots = Some(active_slots);
            return scale_potion_stage_options(
                options,
                self.potion_stage_allowance_divisor(session),
                self.potion_stage_wall_divisor(session),
            );
        }
        options = scale_potion_stage_options(
            options,
            self.potion_stage_allowance_divisor(session),
            self.potion_stage_wall_divisor(session),
        );
        options.allowed_potion_slots = Some(active_potion_slot_mask_for_stage(active_slots, stage));
        options
    }

    pub fn for_session_stage_restore(
        &self,
        session: &RunControlSession,
        stage: u8,
        checkpoint: &OracleCombatWitnessCheckpointV1,
    ) -> OracleCombatWitnessOptionsV1 {
        let mut options = self.for_session_stage(session, stage);
        options.max_potions_used = checkpoint.max_potions_used;
        options.allowed_potion_slots = checkpoint.allowed_potion_slots;
        if checkpoint.allowed_potion_slots.is_some() && checkpoint.max_potions_used != Some(0) {
            options.potion_policy = Some(CombatWitnessPotionPolicyV1::SemanticBudgeted);
        }
        options
    }

    fn stage_divisor(&self, stage: u8) -> u32 {
        if stage == 0 {
            self.initial_divisor.max(1)
        } else {
            1
        }
    }

    fn potion_stage_allowance_divisor(&self, session: &RunControlSession) -> u32 {
        let active_identities = oracle_active_victory_potion_slot_mask_v1(session).count_ones();
        active_identities
            .saturating_add(u32::from(
                boss_multi_potion_fallback_stage(session).is_some(),
            ))
            .max(1)
    }

    fn potion_stage_wall_divisor(&self, session: &RunControlSession) -> u32 {
        // Generation work keeps the bounded exploratory overage documented for
        // potion staging. Wall time cannot do the same: the caller's combat
        // deadline is authoritative, so include the clean primary as another
        // scheduled stage and leave real time for the final configured lane.
        self.potion_stage_allowance_divisor(session)
            .saturating_add(1)
    }

    pub fn has_identity_partitioned_potion_allowance(&self, session: &RunControlSession) -> bool {
        self.uses_potion_conserving_primary(session, &self.for_session_stage(session, 1))
            && oracle_active_victory_potion_slot_mask_v1(session) != 0
    }

    pub fn has_later_stage(&self, session: &RunControlSession, stage: u8) -> bool {
        let uses_potion_stages =
            self.uses_potion_conserving_primary(session, &self.for_session_stage(session, 1));
        let active_potion_stages = if uses_potion_stages {
            oracle_active_victory_potion_slot_mask_v1(session)
                .count_ones()
                .saturating_add(u32::from(
                    boss_multi_potion_fallback_stage(session).is_some(),
                ))
        } else {
            0
        };
        usize::from(stage) < active_potion_stages as usize
            || (stage == 0 && self.initial_divisor > 1)
    }

    pub fn needs_later_stage(
        &self,
        session: &RunControlSession,
        stage: u8,
        work: &OracleResidentCombatWitnessJobV1,
    ) -> bool {
        self.has_later_stage(session, stage) && !work.has_refinement_ending_witness()
    }

    fn uses_potion_conserving_primary(
        &self,
        session: &RunControlSession,
        options: &OracleCombatWitnessOptionsV1,
    ) -> bool {
        if self.quality_policy != OracleRunCombatWitnessQualityPolicyV1::StrategicRun
            || options.max_potions_used.is_some()
            || options.potion_policy.is_some()
            || options.allowed_potion_slots.is_some()
        {
            return false;
        }
        session.active_combat.as_ref().is_some_and(|active| {
            active
                .combat_state
                .entities
                .potions
                .iter()
                .flatten()
                .any(|potion| {
                    potion.can_use || potion.id == crate::content::potions::PotionId::FairyPotion
                })
        })
    }
}

fn boss_multi_potion_fallback_stage(session: &RunControlSession) -> Option<u8> {
    let active = session.active_combat.as_ref()?;
    if !active.combat_state.meta.is_boss_fight {
        return None;
    }
    let identity_stages = oracle_active_victory_potion_slot_mask_v1(session).count_ones();
    if identity_stages == 0 {
        return None;
    }
    u8::try_from(identity_stages).ok()?.checked_add(1)
}

fn active_potion_slot_mask_for_stage(active_slots: u64, stage: u8) -> u64 {
    let mut remaining = active_slots;
    for _ in 1..stage {
        remaining &= remaining.saturating_sub(1);
    }
    if remaining == 0 {
        0
    } else {
        1_u64 << remaining.trailing_zeros()
    }
}

fn scale_combat_options(
    mut options: OracleCombatWitnessOptionsV1,
    divisor: u32,
) -> OracleCombatWitnessOptionsV1 {
    let divisor = usize::try_from(divisor.max(1)).unwrap_or(usize::MAX);
    options.max_generation_work = options
        .max_generation_work
        .map(|value| value.saturating_add(divisor - 1) / divisor)
        .map(|value| value.max(1));
    options.wall_ms = options
        .wall_ms
        .map(|value| value.saturating_add(divisor as u64 - 1) / divisor as u64)
        .map(|value| value.max(1));
    options
}

fn scale_potion_stage_options(
    mut options: OracleCombatWitnessOptionsV1,
    generation_work_divisor: u32,
    wall_divisor: u32,
) -> OracleCombatWitnessOptionsV1 {
    let generation_work_divisor =
        usize::try_from(generation_work_divisor.max(1)).unwrap_or(usize::MAX);
    let wall_divisor = u64::from(wall_divisor.max(1));
    options.max_generation_work = options
        .max_generation_work
        .map(|value| value.saturating_add(generation_work_divisor - 1) / generation_work_divisor)
        .map(|value| value.max(1));
    options.wall_ms = options
        .wall_ms
        .map(|value| value.saturating_add(wall_divisor - 1) / wall_divisor)
        .map(|value| value.max(1));
    options
}

#[derive(Clone, Debug)]
pub struct OracleRunExploreBudgetV1 {
    pub max_work_items: usize,
    pub wall_ms: Option<u64>,
    pub combat: OracleRunCombatWitnessBudgetsV1,
    pub combat_quantum_generation_work: usize,
    pub combat_quantum_ms: Option<u64>,
    pub decision_prior: Option<RunPolicyPriorFnV1>,
    pub decision_annotation: Option<OracleRunDecisionAnnotationFnV1>,
    pub combat_edge_order: Option<OracleRunCombatEdgeOrderFnV1>,
}

pub type OracleRunDecisionAnnotationFnV1 =
    fn(&RunControlSession, &str) -> Option<RunControlTraceAnnotationV1>;
pub type OracleRunCombatEdgeOrderFnV1 =
    fn(&RunControlSession, &str, &RunDecisionAction) -> Option<StrategicProbeShadowOrderKeyV1>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum OracleRunExploreStopV1 {
    Victory { branch_id: usize },
    WorkExhausted,
    WorkBudgetExhausted,
    WallDeadlineReached,
}

struct PendingOracleCombatV1 {
    branch_id: usize,
    stage: u8,
    work: OracleResidentCombatWitnessJobV1,
}

struct DeferredOracleCombatV1 {
    branch_id: usize,
    stage: u8,
    prior_work: OracleCombatWitnessCheckpointV1,
}

pub struct OracleRunExplorerV1 {
    pub branches: Vec<OracleRunBranchV1>,
    pub pending_decisions: VecDeque<LazyOracleRunDecisionV1>,
    pub retired_exact_duplicates: Vec<ExactDuplicateOracleRunBranchV1>,
    pub unresolved_combats: Vec<OracleRunUnresolvedCombatV1>,
    pub combat_search_restarts: usize,
    pending_combats: VecDeque<PendingOracleCombatV1>,
    deferred_combats: VecDeque<DeferredOracleCombatV1>,
    pending_selection_families: VecDeque<LazyOracleRunSelectionFamilyV1>,
    last_served_neow_root: Option<String>,
    next_branch_id: usize,
    state_index: BTreeMap<String, usize>,
    registered_work_keys: BTreeSet<String>,
}

pub struct OracleRunExploreResultV1 {
    pub explorer: OracleRunExplorerV1,
    pub stop: OracleRunExploreStopV1,
    pub work_items: usize,
    pub combat_quanta: usize,
    pub decision_service_ms: u64,
    pub combat_service_ms: u64,
    pub combat_edge_probe_evaluations: usize,
    pub immediate_combat_edge_hints: usize,
    pub elapsed_ms: u64,
}

impl OracleRunExploreResultV1 {
    pub fn witness(&self) -> Option<&OracleRunBranchV1> {
        let OracleRunExploreStopV1::Victory { branch_id } = self.stop else {
            return None;
        };
        self.explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == branch_id)
    }

    pub fn furthest_branch(&self) -> Option<&OracleRunBranchV1> {
        self.explorer.branches.iter().max_by_key(|branch| {
            (
                branch.session.run_state.act_num,
                branch.session.run_state.floor_num,
                branch.journal.len(),
                branch.branch_id,
            )
        })
    }
}

impl OracleRunExplorerV1 {
    fn empty() -> Self {
        Self {
            branches: Vec::new(),
            pending_decisions: VecDeque::new(),
            retired_exact_duplicates: Vec::new(),
            unresolved_combats: Vec::new(),
            combat_search_restarts: 0,
            pending_combats: VecDeque::new(),
            deferred_combats: VecDeque::new(),
            pending_selection_families: VecDeque::new(),
            last_served_neow_root: None,
            next_branch_id: 0,
            state_index: BTreeMap::new(),
            registered_work_keys: BTreeSet::new(),
        }
    }

    pub fn pending_combat_count(&self) -> usize {
        self.pending_combats.len()
    }

    pub fn deferred_combat_count(&self) -> usize {
        self.deferred_combats.len()
    }

    pub fn deferred_combat_branch_ids(&self) -> Vec<usize> {
        self.deferred_combats
            .iter()
            .map(|combat| combat.branch_id)
            .collect()
    }

    pub fn pending_decision_discrepancy_counts(&self) -> BTreeMap<u64, usize> {
        let mut counts = BTreeMap::new();
        for decision in &self.pending_decisions {
            *counts.entry(decision.path_discrepancy).or_insert(0) += 1;
        }
        counts
    }

    pub fn decisions_for_branch(&self, branch_id: usize) -> Vec<LazyOracleRunDecisionV1> {
        self.pending_decisions
            .iter()
            .filter(|work| work.parent_branch_id == branch_id)
            .cloned()
            .collect()
    }

    pub fn record_combat_search_restart(&mut self) {
        self.combat_search_restarts = self.combat_search_restarts.saturating_add(1);
    }

    pub fn deferred_combat_effective_discrepancy_counts(&self) -> BTreeMap<u64, usize> {
        let mut counts = BTreeMap::new();
        for deferred in &self.deferred_combats {
            let Some(branch) = self
                .branches
                .iter()
                .find(|branch| branch.branch_id == deferred.branch_id)
            else {
                continue;
            };
            *counts.entry(branch.path_discrepancy).or_insert(0) += 1;
        }
        counts
    }

    pub fn pending_combat_summaries(&self) -> Result<Vec<OraclePendingCombatSummaryV1>, String> {
        self.pending_combats
            .iter()
            .map(|pending| {
                let branch = self
                    .branches
                    .iter()
                    .find(|branch| branch.branch_id == pending.branch_id)
                    .ok_or_else(|| {
                        format!(
                            "pending combat references missing branch {}",
                            pending.branch_id
                        )
                    })?;
                let active = branch.session.active_combat.as_ref().ok_or_else(|| {
                    format!(
                        "pending combat branch {} has no active combat",
                        pending.branch_id
                    )
                })?;
                let enemies = active
                    .combat_state
                    .entities
                    .monsters
                    .iter()
                    .filter(|monster| !monster.is_dying && !monster.is_escaped)
                    .map(|monster| OraclePendingCombatEnemyV1 {
                        enemy_id: monster.enemy_id,
                        name: super::view_model::monster_name(monster.enemy_id),
                        current_hp: monster.current_hp,
                        max_hp: monster.max_hp,
                    })
                    .collect();
                let progress = pending.work.evidence();
                Ok(OraclePendingCombatSummaryV1 {
                    branch_id: branch.branch_id,
                    act: branch.session.run_state.act_num,
                    floor: branch.session.run_state.floor_num,
                    player_hp: branch.session.run_state.current_hp,
                    player_max_hp: branch.session.run_state.max_hp,
                    elite: active.combat_state.meta.is_elite_fight,
                    boss: active.combat_state.meta.is_boss_fight,
                    enemies,
                    generation_work: progress.generation_work,
                    engine_steps: progress.engine_steps,
                    exact_states: progress.exact_states,
                    applied_action_transitions: progress.applied_action_transitions,
                    unique_successor_states: progress.unique_successor_states,
                    duplicate_exact_successors: progress.duplicate_exact_successors,
                    completed_turn_options: progress.completed_turn_options,
                    retained_state_work: progress.retained_state_work,
                    queued_anchor_entries: progress.queued_anchor_entries,
                    queued_guided_entries: progress.queued_guided_entries,
                    max_player_turn: progress.max_player_turn,
                    max_path_atomic_depth: progress.max_path_atomic_depth,
                    max_completed_turn_options_at_state: progress
                        .max_completed_turn_options_at_state,
                    generation_gap_count: progress.generation_gap_count,
                    pending_witness_replay: progress.pending_witness_replay,
                    quantum_count: pending.work.quantum_count(),
                    last_quantum_generation_work: progress.last_quantum_generation_work,
                    last_quantum_engine_steps: progress.last_quantum_engine_steps,
                    incumbent_final_hp: progress.incumbent_final_hp,
                    incumbent_hp_loss: progress.incumbent_hp_loss,
                    incumbent_action_count: progress.incumbent_action_count,
                    potion_spend_requires_satisfaction: progress.potion_spend_requires_satisfaction,
                    incumbent_revision: progress.incumbent_revision,
                    quanta_since_incumbent_improvement: progress.quanta_since_incumbent_improvement,
                    last_status: progress.last_status,
                    remaining_generation_work: pending.work.remaining_generation_work(),
                    remaining_wall_ms: pending.work.remaining_wall_ms(),
                    resume_kind: if pending.work.restart_count() == 0 {
                        OracleCombatSearchResumeKindV1::Fresh
                    } else {
                        OracleCombatSearchResumeKindV1::StateReplayExactSearchRestarted
                    },
                    restart_count: pending.work.restart_count(),
                })
            })
            .collect()
    }

    pub(super) fn accept_branch(&mut self, branch: OracleRunBranchV1) -> Option<usize> {
        // Exact-state admission is deliberately first-wins. Replacing the
        // survivor based on policy, discrepancy, or path quality would change
        // global search behavior and must be introduced as an explicit
        // dominance policy, never as a branch-storage refactor.
        if let Some(survivor_branch_id) = self.state_index.get(&branch.state_fingerprint).copied() {
            self.retired_exact_duplicates
                .push(ExactDuplicateOracleRunBranchV1 {
                    branch_id: branch.branch_id,
                    parent_branch_id: branch.parent_branch_id,
                    survivor_branch_id,
                    neow_root_candidate_id: branch.neow_root_candidate_id,
                    state_fingerprint: branch.state_fingerprint,
                    replay: branch.replay,
                    journal: branch.journal,
                });
            return None;
        }
        let branch_id = branch.branch_id;
        self.state_index
            .insert(branch.state_fingerprint.clone(), branch_id);
        self.branches.push(branch);
        Some(branch_id)
    }

    pub fn drain_pending_combats(&mut self) -> Vec<(usize, u8, OracleResidentCombatWitnessJobV1)> {
        self.pending_combats
            .drain(..)
            .map(|pending| (pending.branch_id, pending.stage, pending.work))
            .collect()
    }

    #[cfg(test)]
    pub(super) fn materialize_explicit_smoke_bomb_escape(
        &mut self,
        branch_id: usize,
    ) -> Result<Option<usize>, String> {
        let prepared = self.prepare_explicit_smoke_bomb_escape(branch_id)?;
        self.commit_explicit_combat(prepared)
    }
}

pub fn seed_oracle_run_explorer_v1(
    expansion: NeowOracleExpansionV1,
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> Result<OracleRunExplorerV1, String> {
    if !expansion.unresolved.is_empty() {
        return Err(format!(
            "cannot seed oracle run while {} Neow outcomes remain unresolved",
            expansion.unresolved.len()
        ));
    }
    let mut explorer = OracleRunExplorerV1::empty();
    let mut root_ids = Vec::new();
    for candidate in expansion.completed {
        if !candidate.session.engine_state.is_map_surface() {
            return Err(format!(
                "completed Neow candidate '{}' is not at a map boundary",
                candidate.root_candidate_id
            ));
        }
        let branch_id = explorer.next_branch_id;
        explorer.next_branch_id = explorer.next_branch_id.saturating_add(1);
        let session = candidate.session;
        let branch = OracleRunBranchV1 {
            branch_id,
            parent_branch_id: None,
            neow_root_candidate_id: candidate.root_candidate_id,
            neow_root_label: candidate.root_label,
            state_fingerprint: run_control_session_fingerprint_v3(&session),
            boundary: classify_run_boundary(&session),
            path_negative_log_policy: 0.0,
            path_discrepancy: 0,
            path_depth: 1,
            replay: candidate
                .replay
                .into_iter()
                .map(|step| OracleRunReplayStepV1 {
                    candidate_id: step.candidate_id,
                    label: step.label,
                    action: step.action,
                })
                .collect(),
            journal: candidate.journal,
            session,
        };
        if let Some(root_id) = explorer.accept_branch(branch) {
            root_ids.push(root_id);
        }
    }

    // Neow closure retains action-distinct replay paths for audit, including
    // equivalent claim orders for duplicate potion RewardItems. Admission is
    // exact-state first-wins, so normalize policy mass over the states that
    // actually survive instead of the larger pre-dedup path count.
    let root_negative_log_policy = (explorer.branches.len().max(1) as f64).ln();
    for branch in &mut explorer.branches {
        branch.path_negative_log_policy = root_negative_log_policy;
    }

    let mut work_by_root = BTreeMap::<String, VecDeque<LazyOracleRunDecisionV1>>::new();
    for branch_id in root_ids {
        let branch = explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == branch_id)
            .ok_or_else(|| format!("missing oracle root branch {branch_id}"))?;
        let supply = decision_supply_for_branch(branch, decision_prior)?;
        for work in supply.decisions {
            if explorer
                .registered_work_keys
                .insert(work.stable_work_key.clone())
            {
                work_by_root
                    .entry(branch.neow_root_candidate_id.clone())
                    .or_default()
                    .push_back(work);
            }
        }
        if let Some(family) = supply.selection_family {
            if !explorer
                .registered_work_keys
                .insert(family.family_key.clone())
            {
                return Err(format!(
                    "oracle root selection family '{}' was registered twice",
                    family.family_key
                ));
            }
            explorer.pending_selection_families.push_back(family);
        }
    }
    loop {
        let mut added = false;
        for root_work in work_by_root.values_mut() {
            if let Some(work) = root_work.pop_front() {
                explorer.pending_decisions.push_back(work);
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    Ok(explorer)
}

/// Resume exact oracle execution from one already committed run state.
///
/// This deliberately restores no historical sibling frontier. The journal is
/// carried forward solely so a later victory remains replayable from the
/// original run start.
pub fn seed_oracle_run_explorer_from_session_v1(
    session: RunControlSession,
    journal: RunProgressJournalV1,
    combat_budgets: &OracleRunCombatWitnessBudgetsV1,
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> Result<OracleRunExplorerV1, String> {
    let mut explorer = OracleRunExplorerV1::empty();
    let branch_id = explorer.next_branch_id;
    explorer.next_branch_id = explorer.next_branch_id.saturating_add(1);
    let branch = OracleRunBranchV1 {
        branch_id,
        parent_branch_id: None,
        neow_root_candidate_id: "continued-exact-state".to_string(),
        neow_root_label: "continued exact state".to_string(),
        state_fingerprint: run_control_session_fingerprint_v3(&session),
        boundary: classify_run_boundary(&session),
        path_negative_log_policy: 0.0,
        path_discrepancy: 0,
        path_depth: 1,
        replay: Vec::new(),
        journal,
        session,
    };
    let branch_id = explorer
        .accept_branch(branch)
        .ok_or_else(|| "continued oracle state was unexpectedly duplicated".to_string())?;
    explorer.schedule_branch(branch_id, combat_budgets, decision_prior)?;
    Ok(explorer)
}

pub fn drive_oracle_run_explorer_v1(
    mut explorer: OracleRunExplorerV1,
    budget: OracleRunExploreBudgetV1,
) -> Result<OracleRunExploreResultV1, String> {
    if budget.max_work_items == 0 {
        return Err("oracle run work budget must be positive".to_string());
    }
    if budget.combat_quantum_generation_work == 0 {
        return Err("oracle combat quantum generation-work budget must be positive".to_string());
    }
    let started = Instant::now();
    let deadline = budget
        .wall_ms
        .and_then(|wall_ms| started.checked_add(Duration::from_millis(wall_ms)));
    let quantum = OracleCombatWitnessQuantumV1 {
        label: "oracle_run_quantum",
        additional_generation_work: budget.combat_quantum_generation_work,
        soft_wall_ms: budget.combat_quantum_ms,
    };
    let mut work_items = 0usize;
    let mut combat_quanta = 0usize;
    let mut combat_edge_probe_evaluations = 0usize;
    let mut immediate_combat_edge_hints = 0usize;
    let mut decision_service = Duration::ZERO;
    let mut combat_service = Duration::ZERO;

    let stop = loop {
        if work_items >= budget.max_work_items {
            break OracleRunExploreStopV1::WorkBudgetExhausted;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            break OracleRunExploreStopV1::WallDeadlineReached;
        }
        let has_decision = !explorer.pending_decisions.is_empty();
        let has_combat = !explorer.pending_combats.is_empty();
        let has_deferred_combat = !explorer.deferred_combats.is_empty();
        if !has_decision && !has_combat && !has_deferred_combat {
            break OracleRunExploreStopV1::WorkExhausted;
        }

        // A combat is an expensive, incrementally evaluated edge on the
        // currently selected strategic prefix.  While it is active, no
        // sibling strategic prefix may start another combat.
        if has_combat {
            let mut pending = explorer
                .pending_combats
                .pop_front()
                .expect("combat existence checked above");
            let service_started = Instant::now();
            let advance = pending.work.advance(&quantum, deadline);
            work_items = work_items.saturating_add(1);
            combat_quanta = combat_quanta.saturating_add(1);
            let mut stop_after_service = None;
            match advance {
                OracleCombatWitnessAdvanceV1::Pending => {
                    explorer.pending_combats.push_front(pending);
                }
                OracleCombatWitnessAdvanceV1::GlobalDeadlineReached => {
                    explorer.pending_combats.push_front(pending);
                    stop_after_service = Some(OracleRunExploreStopV1::WallDeadlineReached);
                }
                OracleCombatWitnessAdvanceV1::ReadyToFinish
                | OracleCombatWitnessAdvanceV1::AllowanceExhausted => {
                    let stage = pending.stage;
                    let needs_later_stage = explorer
                        .branches
                        .iter()
                        .find(|branch| branch.branch_id == pending.branch_id)
                        .is_some_and(|branch| {
                            budget
                                .combat
                                .needs_later_stage(&branch.session, stage, &pending.work)
                        });
                    if needs_later_stage {
                        explorer.deferred_combats.push_back(DeferredOracleCombatV1 {
                            branch_id: pending.branch_id,
                            stage: stage.saturating_add(1),
                            prior_work: pending.work.checkpoint(),
                        });
                    } else {
                        let prepared =
                            explorer.prepare_explicit_combat(pending.branch_id, &pending.work)?;
                        let branch_schedule = prepared
                            .prospective_branch()
                            .map(|branch| {
                                explorer.prepare_branch_schedule(
                                    branch,
                                    &budget.combat,
                                    budget.decision_prior,
                                )
                            })
                            .transpose()?;
                        let finished = explorer.commit_prepared_combat(prepared)?;
                        match finished {
                            FinishedOracleCombatV1::Resolved(branch_id) => {
                                explorer.apply_branch_schedule(
                                    branch_schedule
                                        .expect("resolved combat must prepare child scheduling"),
                                );
                                let boundary = explorer
                                    .branches
                                    .iter()
                                    .find(|branch| branch.branch_id == branch_id)
                                    .map(|branch| branch.boundary)
                                    .expect("resolved combat branch must remain addressable");
                                if boundary == OracleRunBoundaryV1::TerminalVictory {
                                    stop_after_service =
                                        Some(OracleRunExploreStopV1::Victory { branch_id });
                                }
                            }
                            FinishedOracleCombatV1::Unresolved(unresolved) => {
                                explorer.unresolved_combats.push(unresolved);
                            }
                            FinishedOracleCombatV1::ExactDuplicate => {
                                if let Some(branch_schedule) = branch_schedule {
                                    explorer.apply_branch_schedule(branch_schedule);
                                }
                            }
                        }
                    }
                }
            }
            combat_service = combat_service.saturating_add(service_started.elapsed());
            if let Some(stop) = stop_after_service {
                break stop;
            }
            continue;
        }

        let (probe_evaluations, immediate_hints) =
            explorer.refresh_combat_edge_probes(budget.combat_edge_order)?;
        combat_edge_probe_evaluations =
            combat_edge_probe_evaluations.saturating_add(probe_evaluations);
        immediate_combat_edge_hints = immediate_combat_edge_hints.saturating_add(immediate_hints);
        let scheduled = explorer
            .prepare_next_scheduled_work()
            .expect("strategic work existence checked above");
        let service_started = Instant::now();
        work_items = work_items.saturating_add(1);
        let mut stop_after_service = None;
        match scheduled {
            PreparedScheduledOracleRunWorkV1::Decision {
                root,
                index,
                work: decision,
            } => {
                let selection_service =
                    explorer.prepare_selection_member_release(&decision.stable_work_key)?;
                let prepared = explorer
                    .prepare_explicit_decision(decision.clone(), budget.decision_annotation)?;
                let branch_schedule = explorer.prepare_branch_schedule(
                    prepared.prospective_branch(),
                    &budget.combat,
                    budget.decision_prior,
                )?;
                explorer.commit_scheduled_decision(root, index, &decision.stable_work_key);
                let child_branch_id = explorer.commit_prepared_decision(prepared);
                explorer.apply_branch_schedule(branch_schedule);
                explorer.apply_selection_member_release(selection_service);
                if let Some(branch_id) = child_branch_id {
                    let boundary = explorer
                        .branches
                        .iter()
                        .find(|branch| branch.branch_id == branch_id)
                        .map(|branch| branch.boundary)
                        .expect("materialized oracle branch must remain addressable");
                    if boundary == OracleRunBoundaryV1::TerminalVictory {
                        stop_after_service = Some(OracleRunExploreStopV1::Victory { branch_id });
                    }
                }
            }
            PreparedScheduledOracleRunWorkV1::DeferredCombat {
                root,
                index,
                branch_id,
                stage,
            } => {
                let deferred = &explorer.deferred_combats[index];
                assert_eq!(
                    (deferred.branch_id, deferred.stage),
                    (branch_id, stage),
                    "prepared deferred combat identity must remain stable before construction"
                );
                let pending = explorer.prepare_deferred_combat(deferred, &budget.combat)?;
                explorer.commit_scheduled_deferred_combat(root, index, branch_id, stage);
                explorer.apply_prepared_deferred_combat(pending);
            }
        }
        decision_service = decision_service.saturating_add(service_started.elapsed());
        if let Some(stop) = stop_after_service {
            break stop;
        }
    };

    Ok(OracleRunExploreResultV1 {
        explorer,
        stop,
        work_items,
        combat_quanta,
        decision_service_ms: duration_ms(decision_service),
        combat_service_ms: duration_ms(combat_service),
        combat_edge_probe_evaluations,
        immediate_combat_edge_hints,
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

const ORACLE_RUN_STATE_FINGERPRINT_ALGORITHM: &str =
    "blake2b_256_canonical_run_checkpoint_stable_gameplay_identity_v3";

const RUN_CONTROL_CONTINUATION_FINGERPRINT_ALGORITHM: &str =
    "blake2b_256_run_control_continuation_identity_v1";

pub fn run_control_session_fingerprint_v3(session: &RunControlSession) -> String {
    let mut normalized = session.clone();
    normalized.decision_step = 0;
    normalized.run_state.emitted_events.clear();
    normalized.combat_sequence = 0;
    normalized.auto_capture_last_combat_sequence = None;
    let mut checkpoint = RunControlSessionCheckpointV1::from_session(&normalized);
    checkpoint.clear_combat_diagnostics_for_external_checkpoint();
    canonical_oracle_hash(&checkpoint)
}

/// Identifies future simulator behavior while excluding process-local combat
/// diagnostics and controller-history counters. Unlike the durable V3 exact
/// state ID, this is intended for accepting a replayed suffix across two
/// independently published task archives.
pub fn run_control_continuation_fingerprint_v1(session: &RunControlSession) -> String {
    let mut normalized = session.clone();
    normalized.decision_step = 0;
    normalized.run_state.emitted_events.clear();
    normalized.combat_sequence = 0;
    normalized.auto_capture_last_combat_sequence = None;
    let mut checkpoint = RunControlSessionCheckpointV1::from_session(&normalized);
    checkpoint.clear_non_gameplay_diagnostics_for_external_checkpoint();
    canonical_oracle_hash(&(RUN_CONTROL_CONTINUATION_FINGERPRINT_ALGORITHM, checkpoint))
}

pub(super) fn run_session_fingerprint_v3(session: &RunControlSession) -> String {
    run_control_session_fingerprint_v3(session)
}

fn canonical_oracle_hash<T: Serialize>(value: &T) -> String {
    let canonical_value = serde_json::to_value(value)
        .expect("oracle run checkpoint should serialize into canonical JSON value");
    crate::eval::fingerprint::hash_serializable(&canonical_value)
}

pub(super) fn classify_run_boundary(session: &RunControlSession) -> OracleRunBoundaryV1 {
    if session.active_combat.is_some() {
        return OracleRunBoundaryV1::Combat;
    }
    match session.engine_state {
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => {
            OracleRunBoundaryV1::MapDecision
        }
        EngineState::CombatStart(_)
        | EngineState::CombatProcessing
        | EngineState::CombatPlayerTurn
        | EngineState::PendingChoice(_) => OracleRunBoundaryV1::Combat,
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => {
            OracleRunBoundaryV1::Reward
        }
        EngineState::EventRoom => OracleRunBoundaryV1::Event,
        EngineState::Shop(_) => OracleRunBoundaryV1::Shop,
        EngineState::Campfire => OracleRunBoundaryV1::Campfire,
        EngineState::RunPendingChoice(_) => OracleRunBoundaryV1::RunChoice,
        EngineState::TreasureRoom(_) => OracleRunBoundaryV1::Treasure,
        EngineState::BossRelicSelect(_) => OracleRunBoundaryV1::BossRelic,
        EngineState::DoubleBossComplete => OracleRunBoundaryV1::DoubleBossComplete,
        EngineState::Act3VictoryComplete => OracleRunBoundaryV1::Act3VictoryComplete,
        EngineState::TrueVictoryComplete => OracleRunBoundaryV1::TrueVictoryComplete,
        EngineState::GameOver(RunResult::Victory) => OracleRunBoundaryV1::TerminalVictory,
        EngineState::GameOver(RunResult::Defeat) => OracleRunBoundaryV1::TerminalDefeat,
    }
}

#[cfg(test)]
mod tests;
