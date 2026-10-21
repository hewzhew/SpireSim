use super::*;
use sts_combat_planner::materialize_exact_action_line;
use sts_oracle_learning::agent::task::TaskPublicObservationV1;
use sts_oracle_learning::sim::combat::EngineCombatStepper;

impl AgentCombatDraftRuntimeV1 {
    /// Validate and prepare everything before replacing resident draft work.
    /// Only the draft core retains the prefix; the durable controller stays put.
    pub(crate) fn open_candidate(
        &mut self,
        controller: &mut RunPlayControllerV1,
        candidate: &TaskResearchPlanCandidateV1,
        turn: u32,
        replace: bool,
    ) -> Result<AgentCombatDraftStatusV1, String> {
        if turn == 0 {
            return Err("Draft turn starts at 1".into());
        }
        if self.draft.is_some() && !replace {
            return Err("A combat draft already exists; use replace: true to replace it".into());
        }
        let position = controller.current_combat_position_for_lab_v1()?;
        let root = CombatDecisionRoot::new(position.clone())
            .map_err(|error| format!("combat draft requires a stable root: {error:?}"))?;
        let limit = controller
            .head()
            .combat_plan_options
            .max_engine_steps_per_transition;
        let choices = candidate
            .plan
            .steps
            .iter()
            .map(|step| step.choice.clone())
            .collect::<Vec<_>>();
        let inputs = crate::proposal_audit::resolve_target_inputs_v1(&position, &choices, limit)?;
        let (probe, outcome) = controller.exact_combat_replay_probe_v1(
            &inputs,
            TaskPrivilegedSupportProvenanceV1 {
                access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
                producer_name: "analyst_combat_draft_open".into(),
                producer_revision: 1,
            },
        )?;
        if outcome != candidate.outcome
            || probe.steps.len() != candidate.plan.steps.len()
            || probe
                .steps
                .iter()
                .zip(&candidate.plan.steps)
                .any(|(actual, expected)| {
                    actual.source_decision.information.snapshot_id
                        != expected.source_public_snapshot_id
                        || actual.choice != expected.choice
                })
            || probe
                .steps
                .first()
                .map(|step| &step.source_decision.information.snapshot_id)
                != Some(&candidate.plan.root_public_snapshot_id)
        {
            return Err("Combat candidate no longer replays from this root".into());
        }
        let prefix_count = probe.steps.iter().position(|step| {
            matches!(&step.source_decision.observation, TaskPublicObservationV1::Combat { observation, .. }
                    if observation.turn.turn_count.saturating_add(1) == turn)
        }).ok_or_else(|| format!("Combat candidate has no turn {turn}"))?;
        let prefix = materialize_exact_action_line(
            &EngineCombatStepper,
            &position,
            &inputs[..prefix_count],
            limit,
        )
        .map_err(|error| format!("Cannot replay combat draft prefix: {error:?}"))?;
        let env = CombatDraftEnvV1::from_exact_prefix(
            position,
            TurnOptionGeneratorConfig {
                max_engine_steps_per_transition: limit,
                allow_potion_discard: false,
                ..TurnOptionGeneratorConfig::default()
            },
            prefix.actions,
        )
        .map_err(|error| error.to_string())?;
        let draft = AgentCombatDraftV1 {
            root_head_revision: controller.head().head_revision,
            root_exact_state_hash: root.exact_state_hash().to_owned(),
            env,
            consumed: CombatPlanningCounters::default(),
        };
        let mut reading = reading::status(&draft.env)?;
        reading.view.push_str(&format!(
            "Edit from turn {turn} | {prefix_count} earlier actions retained\n"
        ));
        let response = AgentCombatDraftStatusV1 {
            draft_present: true,
            frame_ref: draft.frame_ref(),
            durable_head_unchanged: true,
            root_head_revision: Some(draft.root_head_revision),
            reading,
            commit_candidate_ref: None,
        };
        self.draft = Some(draft);
        self.selection_binding.entries.clear();
        Ok(response)
    }
}
