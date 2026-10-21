use std::path::PathBuf;

use crate::content::potions::Potion;
use crate::sim::combat::CombatPosition;
use crate::state::core::EngineState;

use super::{CombatCompletionSource, LastBenchmarkCaptureCase, RunControlSession};
use crate::eval::run_control::trace_annotation::CombatAutomationTrajectoryRecordV1;
use crate::eval::run_control::CombatBaselineOutcomeV1;

impl RunControlSession {
    #[doc(hidden)]
    pub fn combat_sequence(&self) -> u64 {
        self.combat_sequence
    }

    /// Process-local correspondence for diagnostic boundaries from one execution
    /// fork. Slots can change on summoning; entity ids identify the same monster.
    /// This is not part of the public actor observation or durable evidence.
    #[doc(hidden)]
    pub fn combat_monster_entities_for_run_play_v1(&self) -> Vec<(u8, crate::EntityId)> {
        self.active_combat
            .as_ref()
            .map(|active| {
                active
                    .combat_state
                    .entities
                    .monsters
                    .iter()
                    .map(|monster| (monster.slot, monster.id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Capture the stable player-input position at the active combat boundary.
    pub fn current_active_combat_position(&self) -> Result<CombatPosition, String> {
        let combat = self
            .active_combat
            .as_ref()
            .map(|active| (&active.engine_state, &active.combat_state))
            .ok_or_else(|| "no active combat state to capture".to_string())?;
        match combat.0 {
            EngineState::CombatPlayerTurn | EngineState::PendingChoice(_) => {
                Ok(CombatPosition::new(combat.0.clone(), combat.1.clone()))
            }
            _ => Err(format!(
                "cannot capture combat from engine state {:?}",
                combat.0
            )),
        }
    }

    pub fn current_combat_position_for_actions(&self) -> Result<CombatPosition, String> {
        let active = self
            .active_combat
            .as_ref()
            .ok_or_else(|| "no active combat state".to_string())?;
        let engine = match &active.engine_state {
            EngineState::CombatPlayerTurn
            | EngineState::CombatProcessing
            | EngineState::PendingChoice(_) => active.engine_state.clone(),
            other => {
                return Err(format!(
                    "engine state {other:?} is not an active combat input state"
                ))
            }
        };
        Ok(CombatPosition::new(engine, active.combat_state.clone()))
    }

    #[doc(hidden)]
    pub fn register_active_combat_sequence_v1(&mut self) -> Result<u64, String> {
        self.current_active_combat_position()?;
        self.observe_active_combat_started();
        Ok(self.combat_sequence)
    }

    pub fn rebase_current_combat_outcome_tracking_v1(
        &mut self,
    ) -> Result<(CombatPosition, u64), String> {
        let position = self.current_combat_position_for_actions()?;
        self.combat_outcomes.rebase_active(&position.combat);
        Ok((position, self.combat_sequence))
    }

    pub(super) fn cleanup_inactive_combat(&mut self) {
        if !matches!(
            self.engine_state,
            EngineState::CombatPlayerTurn
                | EngineState::CombatProcessing
                | EngineState::PendingChoice(_)
        ) {
            self.active_combat = None;
        }
    }

    pub(super) fn ensure_combat_started_if_needed(&mut self) -> Result<(), String> {
        super::super::combat_start::ensure_combat_started_if_needed(
            &mut self.engine_state,
            &mut self.run_state,
            &mut self.active_combat,
        )
    }

    pub(in crate::eval::run_control) fn observe_active_combat_started(&mut self) {
        let started = self.combat_outcomes.ensure_started(
            self.active_combat
                .as_ref()
                .map(|active| &active.combat_state),
        );
        if started {
            self.combat_sequence = self.combat_sequence.saturating_add(1);
            self.current_combat_source = Some(CombatCompletionSource::Manual);
        }
    }

    pub(in crate::eval::run_control) fn remember_capture_case(
        &mut self,
        root: PathBuf,
        case_id: String,
    ) {
        self.observe_active_combat_started();
        self.last_capture_case = Some(LastBenchmarkCaptureCase {
            root,
            case_id,
            combat_sequence: self.combat_sequence,
        });
    }

    pub(in crate::eval::run_control) fn last_capture_case(
        &self,
    ) -> Option<&LastBenchmarkCaptureCase> {
        self.last_capture_case.as_ref()
    }

    pub(in crate::eval::run_control) fn last_completed_combat_matches_capture_case(&self) -> bool {
        let Some(case) = self.last_capture_case.as_ref() else {
            return false;
        };
        self.last_completed_combat_sequence == Some(case.combat_sequence)
    }

    pub(in crate::eval::run_control) fn last_completed_manual_combat_matches_capture_case(
        &self,
    ) -> bool {
        self.last_completed_combat_matches_capture_case()
            && self.last_completed_combat_source == Some(CombatCompletionSource::Manual)
    }

    pub(in crate::eval::run_control) fn mark_current_combat_search_resolved(&mut self) {
        if self.active_combat.is_some() {
            self.current_combat_source = Some(CombatCompletionSource::SearchCombat);
        }
    }

    pub(in crate::eval::run_control) fn remember_combat_automation_trajectory(
        &mut self,
        record: CombatAutomationTrajectoryRecordV1,
    ) {
        self.last_combat_automation_sequence = Some(self.combat_sequence);
        self.last_combat_automation_trajectory = Some(record);
    }

    pub fn last_combat_automation_trajectory(&self) -> Option<&CombatAutomationTrajectoryRecordV1> {
        self.last_combat_automation_trajectory.as_ref()
    }

    pub fn last_combat_baseline(&self) -> Option<&CombatBaselineOutcomeV1> {
        self.combat_outcomes.last()
    }

    pub fn recent_combat_attrition(&self) -> Option<super::RecentCombatAttritionV1> {
        self.recent_combat_attrition
    }

    pub(in crate::eval::run_control) fn preserve_recent_combat_attrition_availability_from(
        &mut self,
        expected: &Self,
    ) {
        // Exact replay can derive this fact from combat-resolution entries even
        // when a legacy checkpoint predates the durable field. Keep the saved
        // unknown explicit instead of silently backfilling historical state.
        let Some(expected_fact) = expected.recent_combat_attrition else {
            self.recent_combat_attrition = None;
            return;
        };

        // The session fingerprint deliberately normalizes the runtime combat
        // sequence counter. Imported and compacted continuations can therefore
        // have a different counter provenance even when every durable combat
        // observation replays exactly. Reconcile only that counter, and only
        // after every semantic attrition field has matched.
        if let Some(mut replayed_fact) = self.recent_combat_attrition {
            replayed_fact.combat_sequence = expected_fact.combat_sequence;
            if replayed_fact == expected_fact {
                self.recent_combat_attrition = Some(expected_fact);
            }
        }
    }

    pub fn last_completed_combat_automation_trajectory(
        &self,
    ) -> Option<&CombatAutomationTrajectoryRecordV1> {
        if self.last_completed_combat_sequence != self.last_combat_automation_sequence {
            return None;
        }
        self.last_combat_automation_trajectory.as_ref()
    }

    pub(in crate::eval::run_control) fn visible_potions(&self) -> &[Option<Potion>] {
        self.active_combat
            .as_ref()
            .map(|active| active.combat_state.entities.potions.as_slice())
            .unwrap_or(self.run_state.potions.as_slice())
    }

    /// Return the player HP visible at the current run or combat boundary.
    pub fn visible_player_hp(&self) -> (i32, i32) {
        self.active_combat
            .as_ref()
            .map(|active| {
                (
                    active.combat_state.entities.player.current_hp,
                    active.combat_state.entities.player.max_hp,
                )
            })
            .unwrap_or((self.run_state.current_hp, self.run_state.max_hp))
    }
}
