use std::collections::HashSet;

use super::*;

impl LocalTurnGraphWitnessSession {
    /// Generates one coherent public-plan proposal surface for a fresh
    /// composite planning node. The returned exact options are ordinary graph
    /// edges; this function owns neither cross-turn scheduling nor outcomes.
    pub(super) fn seed_public_plan_proposals(
        &mut self,
        node_id: usize,
        deadline: Option<Instant>,
        _stepper: &dyn CombatStepper,
    ) -> bool {
        let Some(config) = self.public_plan_turn_proposal else {
            return false;
        };
        if self.nodes[node_id].public_plan_proposal_attempted {
            return false;
        }
        self.nodes[node_id].public_plan_proposal_attempted = true;
        self.used.public_plan_proposal_attempts =
            self.used.public_plan_proposal_attempts.saturating_add(1);

        let remaining_work = self
            .granted_generation_work
            .saturating_sub(self.used.generation_work);
        if remaining_work == 0 || deadline.is_some_and(|limit| Instant::now() >= limit) {
            return false;
        }

        let position = self.nodes[node_id].generator.root().position().clone();
        let remaining_potions = self
            .config
            .max_potions_used
            .map(|limit| limit.saturating_sub(self.nodes[node_id].potion_expenditures));
        let report = exact_complete_turn_proposals_guided_with_deadline_v1(
            &position,
            ExactCompleteTurnProposalConfigV1 {
                max_inner_nodes: config.max_inner_nodes.min(remaining_work),
                max_partial_frontier_states: ExactCompleteTurnProposalConfigV1::default()
                    .max_partial_frontier_states,
                max_end_states: config.max_end_states,
                per_bucket_limit: config.per_bucket_limit,
                max_engine_steps_per_action: self.config.generator.max_engine_steps_per_transition,
                allow_potion_expenditure: self.config.generator.allow_potion_expenditure,
                max_potion_expenditures: remaining_potions,
                allowed_potion_slots: self.config.generator.allowed_potion_slots,
                allow_potion_discard: self.config.generator.allow_potion_discard,
            },
            deadline,
            &|successor| {
                self.policy
                    .state_guides(successor)
                    .into_iter()
                    .map(|guide| ExactCompleteTurnProposalGuideRankV1 {
                        lane: guide.lane.value(),
                        components: guide.rank.components().to_vec(),
                    })
                    .collect()
            },
        );
        self.used.generation_work = self
            .used
            .generation_work
            .saturating_add(report.nodes_expanded);
        self.used.engine_steps = self.used.engine_steps.saturating_add(report.engine_steps);
        self.used.applied_action_transitions = self
            .used
            .applied_action_transitions
            .saturating_add(report.nodes_generated);
        self.used.public_plan_proposal_applied_transitions = self
            .used
            .public_plan_proposal_applied_transitions
            .saturating_add(report.nodes_generated);
        self.used.public_plan_proposal_completed_options = self
            .used
            .public_plan_proposal_completed_options
            .saturating_add(report.candidates.len());

        let root_identity = self.nodes[node_id]
            .generator
            .root()
            .exact_state_identity()
            .clone();
        let options = report
            .candidates
            .into_iter()
            .filter_map(|candidate| {
                let boundary = match candidate.boundary {
                    ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn => {
                        CompleteTurnOptionBoundary::NextPlayerTurn
                    }
                    ExactCompleteTurnProposalBoundaryV1::TerminalWin => {
                        CompleteTurnOptionBoundary::TerminalWin
                    }
                    ExactCompleteTurnProposalBoundaryV1::TerminalLoss => {
                        CompleteTurnOptionBoundary::TerminalLoss
                    }
                    ExactCompleteTurnProposalBoundaryV1::Other => return None,
                };
                let actions = candidate
                    .actions
                    .into_iter()
                    .map(|action| TurnOptionAction {
                        input: action.input,
                        expected_successor_hash: action.expected_successor_hash.into(),
                        engine_steps: action.engine_steps,
                    })
                    .collect();
                Some(CompleteTurnOption::new(
                    root_identity.clone(),
                    actions,
                    boundary,
                    candidate.successor,
                    0.0,
                ))
            })
            .collect::<Vec<_>>();

        let per_transition_steps = self.config.generator.max_engine_steps_per_transition.max(1);

        let agenda = public_combat_agenda_for_plan_v1(
            &public_combat_state_v1(&position.combat),
            &self.nodes[node_id].public_plan,
        );
        let groundable = options
            .iter()
            .enumerate()
            .map(|(plan_index, option)| GroundableCompleteTurnV1 {
                plan_index,
                actions: option
                    .actions()
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
                successor: option.exact_successor().clone(),
            })
            .collect();
        let Ok(grounded) = ground_public_combat_agenda_v1(
            &position,
            &self.nodes[node_id].public_plan,
            &agenda,
            groundable,
            per_transition_steps,
        ) else {
            self.used.public_plan_proposal_grounding_failures = self
                .used
                .public_plan_proposal_grounding_failures
                .saturating_add(1);
            return report.nodes_expanded > 0;
        };
        let retained = grounded
            .into_frontier_candidates()
            .into_iter()
            .map(|grounded| grounded.candidate.plan_index)
            .collect::<HashSet<_>>();
        let retained_options = options
            .into_iter()
            .enumerate()
            .filter_map(|(index, option)| retained.contains(&index).then_some(option))
            .collect::<Vec<_>>();
        self.used.public_plan_proposal_retained_options = self
            .used
            .public_plan_proposal_retained_options
            .saturating_add(retained_options.len());
        self.nodes[node_id]
            .generator
            .publish_external_completed_options(retained_options);
        report.nodes_expanded > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use sts_agent::agent::task::PublicCombatPlanStageV1;
    use sts_core::content::cards::CardId;
    use sts_core::content::monsters::EnemyId;
    use sts_core::runtime::combat::CombatCard;
    use sts_core::sim::combat::EngineCombatStepper;
    use sts_core::state::core::EngineState;

    #[test]
    fn resident_public_plan_proposals_publish_only_the_agenda_frontier() {
        let root = commitment_test_root();
        let decision_root = CombatDecisionRoot::new(root).expect("valid combat root");
        let mut session = LocalTurnGraphWitnessSession::with_policy(
            decision_root,
            LocalTurnGraphWitnessConfig::default(),
            Arc::new(crate::UniformCombatActionPolicy),
        );
        assert_eq!(
            session.nodes[0].public_plan.stage,
            PublicCombatPlanStageV1::EstablishEngine
        );
        session
            .enable_public_plan_turn_proposals(PublicPlanTurnProposalConfig {
                max_inner_nodes: 128,
                max_end_states: 24,
                per_bucket_limit: 24,
            })
            .expect("fresh graph accepts proposal owner");
        session.granted_generation_work = 128;

        assert!(session.seed_public_plan_proposals(0, None, &EngineCombatStepper));

        let counters = session.counters();
        assert_eq!(counters.public_plan_proposal_attempts, 1);
        assert!(counters.public_plan_proposal_completed_options > 1);
        assert!(
            counters.public_plan_proposal_retained_options
                < counters.public_plan_proposal_completed_options
        );
        assert_eq!(
            session.nodes[0].generator.completed_options().len(),
            counters.public_plan_proposal_retained_options
        );
    }

    fn commitment_test_root() -> CombatPosition {
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.turn.energy = 1;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::FeelNoPain, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        let mut monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        monster.current_hp = 80;
        monster.max_hp = 80;
        combat.entities.monsters = vec![monster];
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }
}
