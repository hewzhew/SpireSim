use std::time::{Duration, Instant};

use crate::state::core::{EngineState, RunResult};

use super::planner_boundary_capture::{capture_planner_boundary_v1, PlannerBoundaryVisitOutcomeV1};
use super::{PlannerBoundaryCaptureSegmentV1, PlannerBoundaryYieldKindV1};
use super::{
    RunControlAutoStepOptions, RunControlAutoStopKind, RunControlAutoStopV1, RunControlSession,
    RunProgressJournalV1, RunProgressOutcome, RunProgressStepV1,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedRunDriver {
    max_progress_steps: usize,
    wall_time: Option<Duration>,
    stop_before_combat: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundedRunResultV1 {
    pub entries: Vec<RunProgressOutcome>,
    pub planner_capture: PlannerBoundaryCaptureSegmentV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundedRunStepContextV1 {
    pub applied_progress_steps: usize,
    pub remaining_progress_steps: usize,
    pub remaining_wall_ms: Option<u64>,
    pub committed_journal: RunProgressJournalV1,
    pub planner_capture: PlannerBoundaryCaptureSegmentV1,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundedRunStepControlV1<T> {
    Continue {
        progress_steps: Vec<RunProgressStepV1>,
    },
    Stop {
        progress_steps: Vec<RunProgressStepV1>,
        output: T,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundedRunDriveStopV1<T> {
    Step(T),
    ProgressBudgetExhausted,
    WallDeadlineReached,
    RunCompleted { victory: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundedRunDriveResultV1<T> {
    pub stop: BoundedRunDriveStopV1<T>,
    pub journal: RunProgressJournalV1,
    pub planner_capture: PlannerBoundaryCaptureSegmentV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundedRunDriveErrorV1 {
    pub message: String,
    pub journal: RunProgressJournalV1,
    pub planner_capture: PlannerBoundaryCaptureSegmentV1,
}

impl<T> BoundedRunDriveResultV1<T> {
    pub fn applied_progress_steps(&self) -> usize {
        self.journal.len()
    }
}

impl BoundedRunDriver {
    pub fn new(max_progress_steps: usize, wall_ms: Option<u64>) -> Result<Self, String> {
        if max_progress_steps == 0 {
            return Err("bounded run driver requires at least one progress step".to_string());
        }
        Ok(Self {
            max_progress_steps,
            wall_time: wall_ms.map(Duration::from_millis),
            stop_before_combat: false,
        })
    }

    pub fn stopping_before_combat(mut self) -> Self {
        self.stop_before_combat = true;
        self
    }

    pub fn run(
        &self,
        session: &mut RunControlSession,
        options: RunControlAutoStepOptions,
    ) -> Result<BoundedRunResultV1, String> {
        let mut entries = Vec::new();
        let result = self
            .drive_with(session, |session, _context| {
                if self.stop_before_combat && session.active_combat.is_some() {
                    return Ok(BoundedRunStepControlV1::Stop {
                        progress_steps: Vec::new(),
                        output: driver_stop(
                            RunControlAutoStopKind::CombatBoundary,
                            "bounded run reached combat boundary".to_string(),
                            0,
                        ),
                    });
                }

                let outcome = session.apply_progress_step(options.clone())?;
                match outcome.progress_steps.as_slice() {
                    [RunProgressStepV1::Decision(_)
                    | RunProgressStepV1::ForcedTransition(_)
                    | RunProgressStepV1::CombatResolution(_)] => {
                        let progress_steps = outcome.progress_steps.clone();
                        entries.push(outcome);
                        Ok(BoundedRunStepControlV1::Continue { progress_steps })
                    }
                    [RunProgressStepV1::Stop(_)] => Ok(BoundedRunStepControlV1::Stop {
                        progress_steps: Vec::new(),
                        output: outcome,
                    }),
                    _ => Err(
                        "atomic progress must return exactly one mutation or one typed stop"
                            .to_string(),
                    ),
                }
            })
            .map_err(|error| error.message)?;

        let applied_progress_steps = result.applied_progress_steps();
        let planner_capture = result.planner_capture;
        match result.stop {
            BoundedRunDriveStopV1::Step(outcome) => entries.push(outcome),
            BoundedRunDriveStopV1::ProgressBudgetExhausted => entries.push(driver_stop(
                RunControlAutoStopKind::ProgressBudgetExhausted,
                format!(
                    "progress-step budget exhausted at {}",
                    self.max_progress_steps
                ),
                applied_progress_steps,
            )),
            BoundedRunDriveStopV1::WallDeadlineReached => entries.push(driver_stop(
                RunControlAutoStopKind::WallDeadlineReached,
                "bounded run wall deadline reached".to_string(),
                applied_progress_steps,
            )),
            BoundedRunDriveStopV1::RunCompleted { victory } => entries.push(driver_stop(
                RunControlAutoStopKind::RunCompleted,
                format!(
                    "run completed with {}",
                    if victory { "victory" } else { "defeat" }
                ),
                applied_progress_steps,
            )),
        }

        Ok(BoundedRunResultV1 {
            entries,
            planner_capture,
        })
    }

    pub fn drive_with<T>(
        &self,
        session: &mut RunControlSession,
        mut execute_one: impl FnMut(
            &mut RunControlSession,
            BoundedRunStepContextV1,
        ) -> Result<BoundedRunStepControlV1<T>, String>,
    ) -> Result<BoundedRunDriveResultV1<T>, BoundedRunDriveErrorV1> {
        let started = Instant::now();
        let mut journal = RunProgressJournalV1::default();
        let mut planner_capture = PlannerBoundaryCaptureSegmentV1::default();

        loop {
            let applied_progress_steps = journal.len();
            if let Some(victory) = terminal_result(session) {
                return Ok(BoundedRunDriveResultV1 {
                    stop: BoundedRunDriveStopV1::RunCompleted { victory },
                    journal,
                    planner_capture,
                });
            }
            let pending_capture =
                capture_planner_boundary_v1(session).map_err(|message| BoundedRunDriveErrorV1 {
                    message,
                    journal: journal.clone(),
                    planner_capture: planner_capture.clone(),
                })?;
            if applied_progress_steps >= self.max_progress_steps {
                if let Some(pending) = pending_capture {
                    planner_capture.push(pending.finish(PlannerBoundaryVisitOutcomeV1::Yielded {
                        yield_kind: PlannerBoundaryYieldKindV1::ProgressBudgetExhausted,
                    }));
                }
                return Ok(BoundedRunDriveResultV1 {
                    stop: BoundedRunDriveStopV1::ProgressBudgetExhausted,
                    journal,
                    planner_capture,
                });
            }
            let remaining_wall_ms = self.wall_time.map(|wall_time| {
                wall_time
                    .saturating_sub(started.elapsed())
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64
            });
            if remaining_wall_ms == Some(0) {
                if let Some(pending) = pending_capture {
                    planner_capture.push(pending.finish(PlannerBoundaryVisitOutcomeV1::Yielded {
                        yield_kind: PlannerBoundaryYieldKindV1::WallDeadlineReached,
                    }));
                }
                return Ok(BoundedRunDriveResultV1 {
                    stop: BoundedRunDriveStopV1::WallDeadlineReached,
                    journal,
                    planner_capture,
                });
            }
            let remaining_progress_steps = self
                .max_progress_steps
                .saturating_sub(applied_progress_steps);
            let context = BoundedRunStepContextV1 {
                applied_progress_steps,
                remaining_progress_steps,
                remaining_wall_ms,
                committed_journal: journal.clone(),
                planner_capture: planner_capture.clone(),
            };
            let control = execute_one(session, context).map_err(|message| {
                if let Some(pending) = pending_capture.clone() {
                    planner_capture
                        .push(pending.finish(PlannerBoundaryVisitOutcomeV1::ExecutionFailed));
                }
                BoundedRunDriveErrorV1 {
                    message,
                    journal: journal.clone(),
                    planner_capture: planner_capture.clone(),
                }
            })?;
            match control {
                BoundedRunStepControlV1::Continue { progress_steps } => {
                    if let Err(message) =
                        validate_progress_batch(&progress_steps, remaining_progress_steps, true)
                    {
                        if let Some(pending) = pending_capture {
                            planner_capture.push(
                                pending.finish(PlannerBoundaryVisitOutcomeV1::ExecutionFailed),
                            );
                        }
                        return Err(BoundedRunDriveErrorV1 {
                            message,
                            journal: journal.clone(),
                            planner_capture,
                        });
                    }
                    if let Err(message) = journal.append_committed_steps(progress_steps.clone()) {
                        if let Some(pending) = pending_capture {
                            planner_capture.push(
                                pending.finish(PlannerBoundaryVisitOutcomeV1::ExecutionFailed),
                            );
                        }
                        return Err(BoundedRunDriveErrorV1 {
                            message,
                            journal: journal.clone(),
                            planner_capture,
                        });
                    }
                    if let Some(pending) = pending_capture {
                        planner_capture.push(pending.finish_for_progress(
                            &progress_steps,
                            PlannerBoundaryYieldKindV1::CallbackStop,
                        ));
                    }
                }
                BoundedRunStepControlV1::Stop {
                    progress_steps,
                    output,
                } => {
                    if let Err(message) =
                        validate_progress_batch(&progress_steps, remaining_progress_steps, false)
                    {
                        if let Some(pending) = pending_capture {
                            planner_capture.push(
                                pending.finish(PlannerBoundaryVisitOutcomeV1::ExecutionFailed),
                            );
                        }
                        return Err(BoundedRunDriveErrorV1 {
                            message,
                            journal: journal.clone(),
                            planner_capture,
                        });
                    }
                    if let Err(message) = journal.append_committed_steps(progress_steps.clone()) {
                        if let Some(pending) = pending_capture {
                            planner_capture.push(
                                pending.finish(PlannerBoundaryVisitOutcomeV1::ExecutionFailed),
                            );
                        }
                        return Err(BoundedRunDriveErrorV1 {
                            message,
                            journal: journal.clone(),
                            planner_capture,
                        });
                    }
                    if let Some(pending) = pending_capture {
                        planner_capture.push(pending.finish_for_progress(
                            &progress_steps,
                            PlannerBoundaryYieldKindV1::CallbackStop,
                        ));
                    }
                    return Ok(BoundedRunDriveResultV1 {
                        stop: BoundedRunDriveStopV1::Step(output),
                        journal,
                        planner_capture,
                    });
                }
            }
        }
    }
}

impl BoundedRunResultV1 {
    pub fn final_stop(&self) -> Option<&RunControlAutoStopV1> {
        self.entries.last().and_then(RunProgressOutcome::auto_stop)
    }

    pub fn applied_progress_steps(&self) -> usize {
        self.entries
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome.progress_steps.as_slice(),
                    [RunProgressStepV1::Decision(_)
                        | RunProgressStepV1::ForcedTransition(_)
                        | RunProgressStepV1::CombatResolution(_)]
                )
            })
            .count()
    }
}

fn driver_stop(
    kind: RunControlAutoStopKind,
    reason: String,
    applied_operations: usize,
) -> RunProgressOutcome {
    RunProgressOutcome::progress(reason.clone()).with_progress_step(RunProgressStepV1::Stop(
        RunControlAutoStopV1 {
            kind,
            reason,
            applied_operations,
        },
    ))
}

fn validate_progress_batch(
    progress_steps: &[RunProgressStepV1],
    remaining: usize,
    requires_progress: bool,
) -> Result<(), String> {
    if requires_progress && progress_steps.is_empty() {
        return Err("bounded run continuation made no progress".to_string());
    }
    if progress_steps
        .iter()
        .any(|step| matches!(step, RunProgressStepV1::Stop(_)))
    {
        return Err("bounded run callback returned a stop as committed progress".to_string());
    }
    if progress_steps.len() > remaining {
        return Err(format!(
            "bounded run step applied {} progress steps with only {remaining} remaining",
            progress_steps.len()
        ));
    }
    Ok(())
}

fn terminal_result(session: &RunControlSession) -> Option<bool> {
    match session.engine_state {
        EngineState::GameOver(RunResult::Victory) => Some(true),
        EngineState::GameOver(RunResult::Defeat) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run_control::RunDecisionSelectionSourceV1;

    #[test]
    fn driver_persists_atomic_progress_before_human_stop() {
        let mut session = RunControlSession::new(Default::default());
        let result = BoundedRunDriver::new(4, None)
            .unwrap()
            .run(&mut session, Default::default())
            .unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.applied_progress_steps(), 1);
        let transaction = result.entries[0]
            .single_decision_transaction()
            .expect("first journal entry should be the routine decision");
        assert_eq!(
            transaction.selection.source,
            RunDecisionSelectionSourceV1::RoutinePolicy
        );
        assert_eq!(
            result.final_stop().map(|stop| stop.kind),
            Some(RunControlAutoStopKind::HumanBoundary)
        );
        assert_eq!(result.planner_capture.visits.len(), 2);
        assert!(matches!(
            result.planner_capture.visits[0].outcome,
            super::PlannerBoundaryVisitOutcomeV1::Selected { .. }
        ));
        assert!(matches!(
            result.planner_capture.visits[1].outcome,
            super::PlannerBoundaryVisitOutcomeV1::Yielded {
                yield_kind: PlannerBoundaryYieldKindV1::CallbackStop
            }
        ));
    }

    #[test]
    fn progress_budget_adds_a_typed_final_stop() {
        let mut session = RunControlSession::new(Default::default());
        let result = BoundedRunDriver::new(1, None)
            .unwrap()
            .run(&mut session, Default::default())
            .unwrap();

        assert_eq!(result.applied_progress_steps(), 1);
        assert_eq!(
            result.final_stop().map(|stop| stop.kind),
            Some(RunControlAutoStopKind::ProgressBudgetExhausted)
        );
        assert!(matches!(
            result
                .planner_capture
                .visits
                .last()
                .map(|visit| &visit.outcome),
            Some(super::PlannerBoundaryVisitOutcomeV1::Yielded {
                yield_kind: PlannerBoundaryYieldKindV1::ProgressBudgetExhausted
            })
        ));
    }

    #[test]
    fn zero_wall_budget_stops_without_mutating() {
        let mut session = RunControlSession::new(Default::default());
        let result = BoundedRunDriver::new(4, Some(0))
            .unwrap()
            .run(&mut session, Default::default())
            .unwrap();

        assert_eq!(result.applied_progress_steps(), 0);
        assert_eq!(session.decision_step, 0);
        assert_eq!(
            result.final_stop().map(|stop| stop.kind),
            Some(RunControlAutoStopKind::WallDeadlineReached)
        );
        assert_eq!(result.planner_capture.visits.len(), 1);
        assert!(matches!(
            result.planner_capture.visits[0].outcome,
            super::PlannerBoundaryVisitOutcomeV1::Yielded {
                yield_kind: PlannerBoundaryYieldKindV1::WallDeadlineReached
            }
        ));
    }

    #[test]
    fn callback_protocol_owns_repetition_and_remaining_budget() {
        let mut session = RunControlSession::new(Default::default());
        let mut calls = 0usize;
        let result = BoundedRunDriver::new(3, None)
            .unwrap()
            .drive_with(&mut session, |session, context| {
                assert_eq!(context.applied_progress_steps, calls);
                assert_eq!(context.remaining_progress_steps, 3 - calls);
                calls += 1;
                session.decision_step += 1;
                Ok(BoundedRunStepControlV1::<()>::Continue {
                    progress_steps: vec![synthetic_forced_transition(calls as u64)],
                })
            })
            .unwrap();

        assert_eq!(calls, 3);
        assert_eq!(session.decision_step, 3);
        assert_eq!(result.applied_progress_steps(), 3);
        assert_eq!(result.journal.entries().len(), 3);
        assert!(matches!(
            result.stop,
            BoundedRunDriveStopV1::ProgressBudgetExhausted
        ));
    }

    #[test]
    fn callback_error_retains_prior_committed_journal_entries() {
        let mut session = RunControlSession::new(Default::default());
        let mut calls = 0usize;
        let error = BoundedRunDriver::new(3, None)
            .unwrap()
            .drive_with(&mut session, |_session, _context| {
                calls += 1;
                if calls == 2 {
                    return Err("fixture failure".to_string());
                }
                Ok(BoundedRunStepControlV1::<()>::Continue {
                    progress_steps: vec![synthetic_forced_transition(1)],
                })
            })
            .unwrap_err();

        assert_eq!(error.message, "fixture failure");
        assert_eq!(error.journal.len(), 1);
    }

    fn synthetic_forced_transition(step: u64) -> RunProgressStepV1 {
        use crate::eval::run_control::transition_report::{ActionResult, RunApplyStatus};
        use crate::eval::run_control::{
            RunDecisionBoundaryV1, RunForcedTransitionKindV1, RunForcedTransitionV1,
        };

        let boundary = RunDecisionBoundaryV1 {
            decision_step: step,
            title: "test".to_string(),
            location: "test".to_string(),
            candidates: Vec::new(),
        };
        RunProgressStepV1::ForcedTransition(RunForcedTransitionV1 {
            schema_name: "RunForcedTransition".to_string(),
            schema_version: 1,
            kind: RunForcedTransitionKindV1::EmptyCampfireExit,
            before: boundary.clone(),
            result: ActionResult {
                status: RunApplyStatus::Running,
                chosen_label: "test".to_string(),
                changes: Vec::new(),
            },
            after: boundary,
        })
    }
}
