use std::{fmt, path::PathBuf};

use serde::de::{
    value::MapAccessDeserializer, Error as DeError, IgnoredAny, MapAccess, SeqAccess, Visitor,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ai::combat_search_v2::CombatSearchV2PotionPolicy;
use crate::content::events::neow::NeowEligibilityV1;
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatCard;
use crate::state::core::{ActiveCombat, EngineState};
use crate::state::map::state::MapState;
use crate::state::run::{RunState, RunStateCheckpointV1, RunStateScheduleCheckpointV1};
use crate::state::selection::DomainEvent;

use super::auto_capture::AutoCombatCaptureConfig;
use super::combat_line_adjudication::CombatLineAdjudicationV1;
use super::oracle_combat_witness_contract::OracleCombatWitnessFailureV1;
use super::outcome::CombatOutcomeTracker;
use super::progress_step::{RunControlAutoStopV1, RunProgressStepV1};
use super::reward_auto::RewardAutomationConfig;
use super::trace_annotation::{CombatAutomationTrajectoryRecordV1, RunControlTraceAnnotationV1};
use super::transition_report::ActionResult;

mod apply;
mod combat;

#[derive(Clone, Debug)]
pub struct RunControlConfig {
    pub seed: u64,
    pub ascension_level: u8,
    pub final_act: bool,
    pub player_class: &'static str,
    pub neow_eligibility: NeowEligibilityV1,
    pub reward_automation: RewardAutomationConfig,
    pub auto_capture: AutoCombatCaptureConfig,
    pub search_max_nodes: Option<usize>,
    pub search_wall_ms: Option<u64>,
    pub search_max_hp_loss: Option<u32>,
    pub search_potion_policy: Option<CombatSearchV2PotionPolicy>,
    pub search_max_potions_used: Option<u32>,
}

impl Default for RunControlConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            ascension_level: 0,
            final_act: false,
            player_class: "Ironclad",
            neow_eligibility: NeowEligibilityV1::Rich,
            reward_automation: RewardAutomationConfig::default(),
            auto_capture: AutoCombatCaptureConfig::default(),
            search_max_nodes: None,
            search_wall_ms: None,
            search_max_hp_loss: None,
            search_potion_policy: None,
            search_max_potions_used: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunControlSession {
    pub engine_state: EngineState,
    pub run_state: RunState,
    pub active_combat: Option<ActiveCombat>,
    pub decision_step: u64,
    pub neow_eligibility: NeowEligibilityV1,
    pub reward_automation: RewardAutomationConfig,
    pub shop_visit_context: Option<ShopVisitContextV1>,
    pub(in crate::eval::run_control) auto_capture: AutoCombatCaptureConfig,
    pub(in crate::eval::run_control) search_max_nodes: Option<usize>,
    pub(in crate::eval::run_control) search_wall_ms: Option<u64>,
    pub(in crate::eval::run_control) search_max_hp_loss: Option<u32>,
    pub(in crate::eval::run_control) search_potion_policy: Option<CombatSearchV2PotionPolicy>,
    pub(in crate::eval::run_control) search_max_potions_used: Option<u32>,
    pub(super) combat_outcomes: CombatOutcomeTracker,
    pub(in crate::eval::run_control) combat_sequence: u64,
    pub(in crate::eval::run_control) auto_capture_last_combat_sequence: Option<u64>,
    last_completed_combat_sequence: Option<u64>,
    last_completed_combat_source: Option<CombatCompletionSource>,
    current_combat_source: Option<CombatCompletionSource>,
    last_combat_automation_sequence: Option<u64>,
    last_combat_automation_trajectory: Option<CombatAutomationTrajectoryRecordV1>,
    last_capture_case: Option<LastBenchmarkCaptureCase>,
    recent_combat_attrition: Option<RecentCombatAttritionV1>,
    recent_combat_enemy_hp: Option<RecentCombatEnemyHpV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ShopVisitContextV1 {
    #[serde(default)]
    pub entry_act: u8,
    #[serde(default)]
    pub entry_floor: i32,
    pub entry_gold: i32,
    pub maw_bank_live_at_entry: bool,
    #[serde(default)]
    pub membership_card_owned_at_entry: bool,
    pub spent_gold_in_visit: bool,
}

impl ShopVisitContextV1 {
    fn from_run_state(run_state: &RunState) -> Self {
        Self {
            entry_act: run_state.act_num,
            entry_floor: run_state.floor_num,
            entry_gold: run_state.gold,
            maw_bank_live_at_entry: run_state
                .relics
                .iter()
                .any(|relic| relic.id == RelicId::MawBank && !relic.used_up),
            membership_card_owned_at_entry: run_state
                .relics
                .iter()
                .any(|relic| relic.id == RelicId::MembershipCard),
            spent_gold_in_visit: false,
        }
    }

    fn matches_run_state(self, run_state: &RunState) -> bool {
        self.entry_act == run_state.act_num && self.entry_floor == run_state.floor_num
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(in crate::eval::run_control) struct LastBenchmarkCaptureCase {
    pub root: PathBuf,
    pub case_id: String,
    pub combat_sequence: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(in crate::eval::run_control) enum CombatCompletionSource {
    Manual,
    SearchCombat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecentCombatAttritionV1 {
    pub act: u8,
    pub floor: i32,
    pub combat_sequence: u64,
    pub start_hp: i32,
    pub end_combat_hp: i32,
    pub raw_hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecentCombatEnemyHpV1 {
    pub combat_sequence: u64,
    pub terminal_enemy_hp: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunControlSessionCheckpointV1 {
    engine_state: EngineState,
    run_state: RunStateCheckpointV1,
    active_combat: Option<ActiveCombat>,
    decision_step: u64,
    neow_eligibility: NeowEligibilityV1,
    reward_automation: RewardAutomationConfig,
    shop_visit_context: Option<ShopVisitContextV1>,
    auto_capture: AutoCombatCaptureConfig,
    search_max_nodes: Option<usize>,
    search_wall_ms: Option<u64>,
    search_max_hp_loss: Option<u32>,
    search_potion_policy: Option<CombatSearchV2PotionPolicy>,
    search_max_potions_used: Option<u32>,
    combat_outcomes: CombatOutcomeTracker,
    combat_sequence: u64,
    auto_capture_last_combat_sequence: Option<u64>,
    last_completed_combat_sequence: Option<u64>,
    last_completed_combat_source: Option<CombatCompletionSource>,
    current_combat_source: Option<CombatCompletionSource>,
    last_combat_automation_sequence: Option<u64>,
    last_combat_automation_trajectory: Option<CombatAutomationTrajectoryRecordV1>,
    last_capture_case: Option<LastBenchmarkCaptureCase>,
    recent_combat_attrition: Option<RecentCombatAttritionV1>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct RunControlSessionCheckpointExtrasV1 {
    #[serde(skip_serializing_if = "neow_eligibility_is_rich")]
    neow_eligibility: NeowEligibilityV1,
    #[serde(skip_serializing_if = "reward_automation_config_is_default")]
    reward_automation: RewardAutomationConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    shop_visit_context: Option<ShopVisitContextV1>,
    #[serde(skip_serializing_if = "auto_capture_config_is_default")]
    auto_capture: AutoCombatCaptureConfig,
    #[serde(skip_serializing_if = "combat_outcome_tracker_is_default")]
    combat_outcomes: CombatOutcomeTracker,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_capture_last_combat_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_completed_combat_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_completed_combat_source: Option<CombatCompletionSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_combat_source: Option<CombatCompletionSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_combat_automation_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_combat_automation_trajectory: Option<CombatAutomationTrajectoryRecordV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_capture_case: Option<LastBenchmarkCaptureCase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recent_combat_attrition: Option<RecentCombatAttritionV1>,
}

impl RunControlSessionCheckpointExtrasV1 {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Serialize for RunControlSessionCheckpointV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let extras = RunControlSessionCheckpointExtrasV1 {
            neow_eligibility: self.neow_eligibility,
            reward_automation: self.reward_automation.clone(),
            shop_visit_context: self.shop_visit_context,
            auto_capture: self.auto_capture.clone(),
            combat_outcomes: self.combat_outcomes.clone(),
            auto_capture_last_combat_sequence: self.auto_capture_last_combat_sequence,
            last_completed_combat_sequence: self.last_completed_combat_sequence,
            last_completed_combat_source: self.last_completed_combat_source,
            current_combat_source: self.current_combat_source,
            last_combat_automation_sequence: self.last_combat_automation_sequence,
            last_combat_automation_trajectory: self.last_combat_automation_trajectory.clone(),
            last_capture_case: self.last_capture_case.clone(),
            recent_combat_attrition: self.recent_combat_attrition,
        };
        let extras = (!extras.is_empty()).then_some(extras);
        (
            &self.engine_state,
            &self.run_state,
            &self.active_combat,
            self.decision_step,
            &self.search_max_nodes,
            &self.search_wall_ms,
            &self.search_max_hp_loss,
            &self.search_potion_policy,
            &self.search_max_potions_used,
            self.combat_sequence,
            extras,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RunControlSessionCheckpointV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(RunControlSessionCheckpointVisitorV1)
    }
}

/// Decode the compact tuple through typed sequence access. Real combat states
/// contain integer-keyed entity maps that MessagePack supports but an
/// intermediate `serde_json::Value` cannot represent.
struct RunControlSessionCheckpointVisitorV1;

impl<'de> Visitor<'de> for RunControlSessionCheckpointVisitorV1 {
    type Value = RunControlSessionCheckpointV1;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a compact run-control checkpoint tuple or legacy checkpoint object")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let engine_state = checkpoint_field(&mut sequence, 0, &self)?;
        let run_state = checkpoint_field(&mut sequence, 1, &self)?;
        let active_combat = checkpoint_field(&mut sequence, 2, &self)?;
        let decision_step = checkpoint_field(&mut sequence, 3, &self)?;
        let search_max_nodes = checkpoint_field(&mut sequence, 4, &self)?;
        let search_wall_ms = checkpoint_field(&mut sequence, 5, &self)?;
        let search_max_hp_loss = checkpoint_field(&mut sequence, 6, &self)?;
        let search_potion_policy = checkpoint_field(&mut sequence, 7, &self)?;
        let search_max_potions_used = checkpoint_field(&mut sequence, 8, &self)?;
        let combat_sequence = checkpoint_field(&mut sequence, 9, &self)?;
        let extras: Option<RunControlSessionCheckpointExtrasV1> =
            checkpoint_field(&mut sequence, 10, &self)?;
        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(A::Error::invalid_length(12, &self));
        }
        let extras = extras.unwrap_or_default();
        Ok(RunControlSessionCheckpointV1 {
            engine_state,
            run_state,
            active_combat,
            decision_step,
            neow_eligibility: extras.neow_eligibility,
            reward_automation: extras.reward_automation,
            shop_visit_context: extras.shop_visit_context,
            auto_capture: extras.auto_capture,
            search_max_nodes,
            search_wall_ms,
            search_max_hp_loss,
            search_potion_policy,
            search_max_potions_used,
            combat_outcomes: extras.combat_outcomes,
            combat_sequence,
            auto_capture_last_combat_sequence: extras.auto_capture_last_combat_sequence,
            last_completed_combat_sequence: extras.last_completed_combat_sequence,
            last_completed_combat_source: extras.last_completed_combat_source,
            current_combat_source: extras.current_combat_source,
            last_combat_automation_sequence: extras.last_combat_automation_sequence,
            last_combat_automation_trajectory: extras.last_combat_automation_trajectory,
            last_capture_case: extras.last_capture_case,
            recent_combat_attrition: extras.recent_combat_attrition,
        })
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        RunControlSessionCheckpointLegacyV1::deserialize(MapAccessDeserializer::new(map))
            .map(RunControlSessionCheckpointLegacyV1::into_checkpoint)
    }
}

fn checkpoint_field<'de, A, T>(
    sequence: &mut A,
    index: usize,
    visitor: &RunControlSessionCheckpointVisitorV1,
) -> Result<T, A::Error>
where
    A: SeqAccess<'de>,
    T: Deserialize<'de>,
{
    sequence
        .next_element()?
        .ok_or_else(|| A::Error::invalid_length(index, visitor))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunControlSessionCheckpointLegacyV1 {
    engine_state: EngineState,
    run_state: RunStateCheckpointV1,
    #[serde(default)]
    active_combat: Option<ActiveCombat>,
    decision_step: u64,
    #[serde(default)]
    neow_eligibility: NeowEligibilityV1,
    #[serde(default)]
    reward_automation: RewardAutomationConfig,
    #[serde(default)]
    shop_visit_context: Option<ShopVisitContextV1>,
    #[serde(default)]
    auto_capture: AutoCombatCaptureConfig,
    #[serde(default)]
    search_max_nodes: Option<usize>,
    #[serde(default)]
    search_wall_ms: Option<u64>,
    #[serde(default)]
    search_max_hp_loss: Option<u32>,
    #[serde(default)]
    search_potion_policy: Option<CombatSearchV2PotionPolicy>,
    #[serde(default)]
    search_max_potions_used: Option<u32>,
    #[serde(default)]
    combat_outcomes: CombatOutcomeTracker,
    combat_sequence: u64,
    #[serde(default)]
    auto_capture_last_combat_sequence: Option<u64>,
    #[serde(default)]
    last_completed_combat_sequence: Option<u64>,
    #[serde(default)]
    last_completed_combat_source: Option<CombatCompletionSource>,
    #[serde(default)]
    current_combat_source: Option<CombatCompletionSource>,
    #[serde(default)]
    last_combat_automation_sequence: Option<u64>,
    #[serde(default)]
    last_combat_automation_trajectory: Option<CombatAutomationTrajectoryRecordV1>,
    #[serde(default)]
    last_capture_case: Option<LastBenchmarkCaptureCase>,
    #[serde(default)]
    recent_combat_attrition: Option<RecentCombatAttritionV1>,
}

impl RunControlSessionCheckpointLegacyV1 {
    fn into_checkpoint(self) -> RunControlSessionCheckpointV1 {
        RunControlSessionCheckpointV1 {
            engine_state: self.engine_state,
            run_state: self.run_state,
            active_combat: self.active_combat,
            decision_step: self.decision_step,
            neow_eligibility: self.neow_eligibility,
            reward_automation: self.reward_automation,
            shop_visit_context: self.shop_visit_context,
            auto_capture: self.auto_capture,
            search_max_nodes: self.search_max_nodes,
            search_wall_ms: self.search_wall_ms,
            search_max_hp_loss: self.search_max_hp_loss,
            search_potion_policy: self.search_potion_policy,
            search_max_potions_used: self.search_max_potions_used,
            combat_outcomes: self.combat_outcomes,
            combat_sequence: self.combat_sequence,
            auto_capture_last_combat_sequence: self.auto_capture_last_combat_sequence,
            last_completed_combat_sequence: self.last_completed_combat_sequence,
            last_completed_combat_source: self.last_completed_combat_source,
            current_combat_source: self.current_combat_source,
            last_combat_automation_sequence: self.last_combat_automation_sequence,
            last_combat_automation_trajectory: self.last_combat_automation_trajectory,
            last_capture_case: self.last_capture_case,
            recent_combat_attrition: self.recent_combat_attrition,
        }
    }
}

fn reward_automation_config_is_default(value: &RewardAutomationConfig) -> bool {
    value == &RewardAutomationConfig::default()
}

fn neow_eligibility_is_rich(value: &NeowEligibilityV1) -> bool {
    *value == NeowEligibilityV1::Rich
}

fn auto_capture_config_is_default(value: &AutoCombatCaptureConfig) -> bool {
    value == &AutoCombatCaptureConfig::default()
}

fn combat_outcome_tracker_is_default(value: &CombatOutcomeTracker) -> bool {
    value == &CombatOutcomeTracker::default()
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunControlDecisionParentSnapshotV1 {
    pub source: String,
    pub command: String,
    pub snapshot: RunControlSessionCheckpointV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunProgressOutcome {
    pub message: String,
    pub action_result: Option<ActionResult>,
    pub atomic_combat_search_rejection: Option<AtomicCombatSearchRejectionV2>,
    pub oracle_combat_witness_failure: Option<OracleCombatWitnessFailureV1>,
    pub execution_adjudication: Option<CombatLineAdjudicationV1>,
    pub auto_applied_steps: Vec<RunControlAutoAppliedStepV1>,
    pub trace_annotations: Vec<RunControlTraceAnnotationV1>,
    pub decision_parent_snapshots: Vec<RunControlDecisionParentSnapshotV1>,
    pub progress_steps: Vec<RunProgressStepV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AtomicCombatSearchRejectionV2 {
    InvalidCardIdentity,
    NoCompleteWinningCandidate,
    DirtyWinningCandidateRejected,
    HpLossLimitExceeded,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunControlAutoAppliedStepV1 {
    pub kind: RunControlAutoAppliedKindV1,
    pub label: String,
    pub action_result: Option<ActionResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunControlAutoAppliedKindV1 {
    RewardPolicyCandidate,
    CombatSearch,
    RoutePolicy,
    RewardOverlay,
    RoutineCandidate,
    AutoCapture,
    OwnerRoutine,
}

impl RunProgressOutcome {
    pub fn progress(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            action_result: None,
            atomic_combat_search_rejection: None,
            oracle_combat_witness_failure: None,
            execution_adjudication: None,
            auto_applied_steps: Vec::new(),
            trace_annotations: Vec::new(),
            decision_parent_snapshots: Vec::new(),
            progress_steps: Vec::new(),
        }
    }

    pub(in crate::eval::run_control) fn message(message: impl Into<String>) -> Self {
        Self::progress(message)
    }

    pub(in crate::eval::run_control) fn action(
        message: impl Into<String>,
        action_result: ActionResult,
    ) -> Self {
        Self {
            message: message.into(),
            action_result: Some(action_result),
            atomic_combat_search_rejection: None,
            oracle_combat_witness_failure: None,
            execution_adjudication: None,
            auto_applied_steps: Vec::new(),
            trace_annotations: Vec::new(),
            decision_parent_snapshots: Vec::new(),
            progress_steps: Vec::new(),
        }
    }

    pub(in crate::eval::run_control) fn with_auto_applied_steps(
        mut self,
        auto_applied_steps: Vec<RunControlAutoAppliedStepV1>,
    ) -> Self {
        self.auto_applied_steps.extend(auto_applied_steps);
        self
    }

    pub(in crate::eval::run_control) fn with_trace_annotations(
        mut self,
        trace_annotations: Vec<RunControlTraceAnnotationV1>,
    ) -> Self {
        if let Some(adjudication) = trace_annotations.iter().rev().find_map(|annotation| {
            let RunControlTraceAnnotationV1::CombatSearchPerformance { snapshot } = annotation
            else {
                return None;
            };
            snapshot.execution_adjudication.clone()
        }) {
            self.execution_adjudication = Some(adjudication);
        }
        self.trace_annotations.extend(trace_annotations);
        self
    }

    pub(in crate::eval::run_control) fn with_decision_parent_snapshots(
        mut self,
        snapshots: Vec<RunControlDecisionParentSnapshotV1>,
    ) -> Self {
        self.decision_parent_snapshots.extend(snapshots);
        self
    }

    pub(in crate::eval::run_control) fn with_progress_step(
        mut self,
        step: RunProgressStepV1,
    ) -> Self {
        assert!(
            !self
                .progress_steps
                .iter()
                .any(|step| matches!(step, RunProgressStepV1::Stop(_))),
            "progress steps cannot be appended after Stop"
        );
        self.progress_steps.push(step);
        self
    }

    pub(in crate::eval::run_control) fn with_progress_steps(
        mut self,
        progress_steps: Vec<RunProgressStepV1>,
    ) -> Self {
        assert!(
            !self
                .progress_steps
                .iter()
                .any(|step| matches!(step, RunProgressStepV1::Stop(_))),
            "progress steps cannot be appended after Stop"
        );
        assert!(
            progress_steps
                .iter()
                .enumerate()
                .all(|(index, step)| !matches!(step, RunProgressStepV1::Stop(_))
                    || index + 1 == progress_steps.len()),
            "Stop must be the final progress step"
        );
        self.progress_steps.extend(progress_steps);
        self
    }

    pub fn auto_stop(&self) -> Option<&RunControlAutoStopV1> {
        self.progress_steps
            .last()
            .and_then(RunProgressStepV1::as_stop)
    }

    pub fn single_decision_transaction(&self) -> Option<&super::RunDecisionTransactionV1> {
        let mut decisions = self
            .progress_steps
            .iter()
            .filter_map(RunProgressStepV1::as_decision);
        let decision = decisions.next()?;
        decisions.next().is_none().then_some(decision)
    }

    pub fn single_forced_transition(&self) -> Option<&super::RunForcedTransitionV1> {
        let mut transitions = self
            .progress_steps
            .iter()
            .filter_map(RunProgressStepV1::as_forced_transition);
        let transition = transitions.next()?;
        transitions.next().is_none().then_some(transition)
    }

    pub fn single_combat_resolution(&self) -> Option<&super::RunCombatResolutionV1> {
        let mut resolutions = self
            .progress_steps
            .iter()
            .filter_map(RunProgressStepV1::as_combat_resolution);
        let resolution = resolutions.next()?;
        resolutions.next().is_none().then_some(resolution)
    }

    pub(in crate::eval::run_control) fn with_atomic_combat_search_rejection(
        mut self,
        rejection: AtomicCombatSearchRejectionV2,
    ) -> Self {
        self.atomic_combat_search_rejection = Some(rejection);
        self
    }

    pub(in crate::eval::run_control) fn with_oracle_combat_witness_failure(
        mut self,
        failure: OracleCombatWitnessFailureV1,
    ) -> Self {
        self.oracle_combat_witness_failure = Some(failure);
        self
    }

    pub(in crate::eval::run_control) fn with_execution_adjudication(
        mut self,
        adjudication: CombatLineAdjudicationV1,
    ) -> Self {
        self.execution_adjudication = Some(adjudication);
        self
    }
}

impl RunControlSession {
    pub fn new(config: RunControlConfig) -> Self {
        let run_state = RunState::new_with_neow_eligibility(
            config.seed,
            config.ascension_level,
            config.final_act,
            config.player_class,
            config.neow_eligibility,
        );
        let engine_state = EngineState::EventRoom;

        Self {
            engine_state,
            run_state,
            active_combat: None,
            decision_step: 0,
            neow_eligibility: config.neow_eligibility,
            reward_automation: config.reward_automation,
            shop_visit_context: None,
            auto_capture: config.auto_capture,
            search_max_nodes: config.search_max_nodes,
            search_wall_ms: config.search_wall_ms,
            search_max_hp_loss: config.search_max_hp_loss,
            search_potion_policy: config.search_potion_policy,
            search_max_potions_used: config.search_max_potions_used,
            combat_outcomes: CombatOutcomeTracker::default(),
            combat_sequence: 0,
            auto_capture_last_combat_sequence: None,
            last_completed_combat_sequence: None,
            last_completed_combat_source: None,
            current_combat_source: None,
            last_combat_automation_sequence: None,
            last_combat_automation_trajectory: None,
            last_capture_case: None,
            recent_combat_attrition: None,
            recent_combat_enemy_hp: None,
        }
    }

    /// Reconstruct the durable environment configuration for a fresh seed.
    ///
    /// Episode schedulers replace a completed run without rebuilding policy
    /// configuration from defaults.  The current session remains the source
    /// of truth for every setting that must survive that replacement; only
    /// the run seed changes.
    pub fn fresh_run_config(&self, seed: u64) -> RunControlConfig {
        RunControlConfig {
            seed,
            ascension_level: self.run_state.ascension_level,
            final_act: self.run_state.is_final_act_available,
            player_class: self.run_state.player_class,
            neow_eligibility: self.neow_eligibility,
            reward_automation: self.reward_automation.clone(),
            auto_capture: self.auto_capture.clone(),
            search_max_nodes: self.search_max_nodes,
            search_wall_ms: self.search_wall_ms,
            search_max_hp_loss: self.search_max_hp_loss,
            search_potion_policy: self.search_potion_policy,
            search_max_potions_used: self.search_max_potions_used,
        }
    }

    pub fn set_auto_capture_config(&mut self, auto_capture: AutoCombatCaptureConfig) {
        self.auto_capture = auto_capture;
        self.auto_capture_last_combat_sequence = None;
    }

    pub fn recent_combat_enemy_hp(&self) -> Option<RecentCombatEnemyHpV1> {
        self.recent_combat_enemy_hp
    }

    pub fn shop_visit_context(&self) -> Option<ShopVisitContextV1> {
        self.shop_visit_context
            .filter(|context| context.matches_run_state(&self.run_state))
    }

    pub fn membership_card_acquired_during_current_shop_visit(&self) -> bool {
        self.shop_visit_context().is_some_and(|context| {
            !context.membership_card_owned_at_entry
                && self
                    .run_state
                    .relics
                    .iter()
                    .any(|relic| relic.id == RelicId::MembershipCard)
        })
    }

    pub(in crate::eval::run_control) fn observe_shop_visit_before_input(&mut self) {
        if engine_state_is_inside_shop_visit(&self.engine_state) {
            if !self
                .shop_visit_context
                .is_some_and(|context| context.matches_run_state(&self.run_state))
            {
                self.shop_visit_context = Some(ShopVisitContextV1::from_run_state(&self.run_state));
            }
        } else {
            self.shop_visit_context = None;
        }
    }

    pub(in crate::eval::run_control) fn observe_shop_visit_after_input(
        &mut self,
        gold_before: i32,
    ) {
        if engine_state_is_inside_shop_visit(&self.engine_state) {
            if !self
                .shop_visit_context
                .is_some_and(|context| context.matches_run_state(&self.run_state))
            {
                self.shop_visit_context = Some(ShopVisitContextV1::from_run_state(&self.run_state));
            }
            if self.run_state.gold < gold_before {
                if let Some(context) = self.shop_visit_context.as_mut() {
                    context.spent_gold_in_visit = true;
                }
            }
        } else {
            self.shop_visit_context = None;
        }
    }
}

fn engine_state_is_inside_shop_visit(state: &EngineState) -> bool {
    match state {
        EngineState::Shop(_) => true,
        EngineState::RewardOverlay { return_state, .. }
        | EngineState::MapOverlay { return_state } => {
            engine_state_is_inside_shop_visit(return_state)
        }
        EngineState::RunPendingChoice(choice) => {
            engine_state_is_inside_shop_visit(&choice.return_state)
        }
        _ => false,
    }
}

impl RunControlSessionCheckpointV1 {
    /// Returns only the persistent card-reward RNG needed by the explicit
    /// Java/Rust parity owner. This is not part of the public run decision.
    #[doc(hidden)]
    pub fn card_rng_for_run_play_parity_v1(&self) -> crate::runtime::rng::StsRng {
        self.run_state.rng_pool.card_rng.clone()
    }

    /// Returns Java's `NeowEvent.rng` counterpart for the explicit parity
    /// owner. This stream is independent from `AbstractDungeon.cardRng`.
    #[doc(hidden)]
    pub fn neow_rng_for_run_play_parity_v1(&self) -> Result<crate::runtime::rng::StsRng, String> {
        self.run_state
            .neow_rng
            .clone()
            .ok_or_else(|| "NeowEvent.rng has not been initialized".to_owned())
    }

    pub fn from_session(session: &RunControlSession) -> Self {
        Self {
            engine_state: session.engine_state.clone(),
            run_state: RunStateCheckpointV1::from_run_state(&session.run_state),
            active_combat: session.active_combat.clone(),
            decision_step: session.decision_step,
            neow_eligibility: session.neow_eligibility,
            reward_automation: session.reward_automation.clone(),
            shop_visit_context: session.shop_visit_context,
            auto_capture: session.auto_capture.clone(),
            search_max_nodes: session.search_max_nodes,
            search_wall_ms: session.search_wall_ms,
            search_max_hp_loss: session.search_max_hp_loss,
            search_potion_policy: session.search_potion_policy,
            search_max_potions_used: session.search_max_potions_used,
            combat_outcomes: session.combat_outcomes.clone(),
            combat_sequence: session.combat_sequence,
            auto_capture_last_combat_sequence: session.auto_capture_last_combat_sequence,
            last_completed_combat_sequence: session.last_completed_combat_sequence,
            last_completed_combat_source: session.last_completed_combat_source,
            current_combat_source: session.current_combat_source,
            last_combat_automation_sequence: session.last_combat_automation_sequence,
            last_combat_automation_trajectory: session.last_combat_automation_trajectory.clone(),
            last_capture_case: session.last_capture_case.clone(),
            recent_combat_attrition: session.recent_combat_attrition,
        }
    }

    pub fn last_combat_automation_trajectory_record(
        &self,
    ) -> Option<&CombatAutomationTrajectoryRecordV1> {
        self.last_combat_automation_trajectory.as_ref()
    }

    pub fn recent_combat_attrition(&self) -> Option<RecentCombatAttritionV1> {
        self.recent_combat_attrition
    }

    pub fn take_last_combat_automation_trajectory_record(
        &mut self,
    ) -> Option<CombatAutomationTrajectoryRecordV1> {
        self.last_combat_automation_trajectory.take()
    }

    pub fn restore_last_combat_automation_trajectory_record(
        &mut self,
        record: CombatAutomationTrajectoryRecordV1,
    ) {
        self.last_combat_automation_trajectory = Some(record);
    }

    pub fn take_run_state_map_for_external_ref(&mut self) -> MapState {
        std::mem::take(&mut self.run_state.map)
    }

    pub fn restore_run_state_map_from_external_ref(&mut self, map: MapState) {
        self.run_state.map = map;
    }

    pub fn take_run_state_master_deck_for_external_ref(&mut self) -> Vec<CombatCard> {
        std::mem::take(&mut self.run_state.master_deck)
    }

    pub fn restore_run_state_master_deck_from_external_ref(
        &mut self,
        master_deck: Vec<CombatCard>,
    ) {
        self.run_state.master_deck = master_deck;
    }

    pub fn take_run_state_relics_for_external_ref(
        &mut self,
    ) -> Vec<crate::content::relics::RelicState> {
        std::mem::take(&mut self.run_state.relics)
    }

    pub fn restore_run_state_relics_from_external_ref(
        &mut self,
        relics: Vec<crate::content::relics::RelicState>,
    ) {
        self.run_state.relics = relics;
    }

    pub fn take_run_state_potions_for_external_ref(
        &mut self,
    ) -> Vec<Option<crate::content::potions::Potion>> {
        std::mem::take(&mut self.run_state.potions)
    }

    pub fn restore_run_state_potions_from_external_ref(
        &mut self,
        potions: Vec<Option<crate::content::potions::Potion>>,
    ) {
        self.run_state.potions = potions;
    }

    pub fn take_run_state_schedule_for_external_ref(&mut self) -> RunStateScheduleCheckpointV1 {
        self.run_state.take_schedule_for_external_ref()
    }

    pub fn restore_run_state_schedule_from_external_ref(
        &mut self,
        schedule: RunStateScheduleCheckpointV1,
    ) {
        self.run_state.restore_schedule_from_external_ref(schedule);
    }

    pub fn take_run_state_emitted_events_for_external_ref(&mut self) -> Vec<DomainEvent> {
        self.run_state.take_emitted_events_for_external_ref()
    }

    pub fn restore_run_state_emitted_events_from_external_ref(
        &mut self,
        emitted_events: Vec<DomainEvent>,
    ) {
        self.run_state
            .restore_emitted_events_from_external_ref(emitted_events);
    }

    pub fn clear_combat_diagnostics_for_external_checkpoint(&mut self) {
        self.combat_outcomes = CombatOutcomeTracker::default();
        self.clear_combat_diagnostic_metadata();
    }

    pub fn trim_combat_diagnostics_for_durable_branch(&mut self) {
        // Replaying a suffix can finish this combat. Preserve its in-progress
        // potion count and HP baseline while dropping completed diagnostics.
        self.combat_outcomes.retain_active_drafts_only();
        self.clear_combat_diagnostic_metadata();
    }

    fn clear_combat_diagnostic_metadata(&mut self) {
        self.last_completed_combat_sequence = None;
        self.last_completed_combat_source = None;
        self.current_combat_source = None;
        self.last_combat_automation_sequence = None;
        self.last_combat_automation_trajectory = None;
        self.last_capture_case = None;
    }

    /// Removes controller/search observations that cannot change simulator
    /// mechanics. External exact-state workbenches must call this before
    /// content-addressing a recovery checkpoint so diagnostic history does
    /// not create false root identities.
    #[doc(hidden)]
    pub fn clear_non_gameplay_diagnostics_for_external_checkpoint(&mut self) {
        self.clear_combat_diagnostics_for_external_checkpoint();
        self.recent_combat_attrition = None;
    }

    pub fn take_active_combat_for_external_ref(&mut self) -> Option<ActiveCombat> {
        self.active_combat.take()
    }

    pub fn restore_active_combat_from_external_ref(&mut self, active_combat: ActiveCombat) {
        self.active_combat = Some(active_combat);
    }

    pub fn into_session(self) -> Result<RunControlSession, String> {
        Ok(RunControlSession {
            engine_state: self.engine_state,
            run_state: self.run_state.into_run_state()?,
            active_combat: self.active_combat,
            decision_step: self.decision_step,
            neow_eligibility: self.neow_eligibility,
            reward_automation: self.reward_automation,
            shop_visit_context: self.shop_visit_context,
            auto_capture: self.auto_capture,
            search_max_nodes: self.search_max_nodes,
            search_wall_ms: self.search_wall_ms,
            search_max_hp_loss: self.search_max_hp_loss,
            search_potion_policy: self.search_potion_policy,
            search_max_potions_used: self.search_max_potions_used,
            combat_outcomes: self.combat_outcomes,
            combat_sequence: self.combat_sequence,
            auto_capture_last_combat_sequence: self.auto_capture_last_combat_sequence,
            last_completed_combat_sequence: self.last_completed_combat_sequence,
            last_completed_combat_source: self.last_completed_combat_source,
            current_combat_source: self.current_combat_source,
            last_combat_automation_sequence: self.last_combat_automation_sequence,
            last_combat_automation_trajectory: self.last_combat_automation_trajectory,
            last_capture_case: self.last_capture_case,
            recent_combat_attrition: self.recent_combat_attrition,
            recent_combat_enemy_hp: None,
        })
    }
}

pub fn canonical_player_class(raw: &str) -> Result<&'static str, String> {
    match raw.to_ascii_lowercase().as_str() {
        "ironclad" | "red" => Ok("Ironclad"),
        "silent" | "green" => Ok("Silent"),
        "defect" | "blue" => Ok("Defect"),
        "watcher" | "purple" => Ok("Watcher"),
        _ => Err(format!("unsupported player class '{raw}'")),
    }
}

#[cfg(test)]
mod tests;
