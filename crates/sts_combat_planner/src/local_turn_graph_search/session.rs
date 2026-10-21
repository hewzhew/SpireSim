use super::*;

impl LocalTurnGraphWitnessSession {
    pub fn set_satisfaction(&mut self, satisfaction: OracleCombatWitnessSatisfaction) {
        self.config.satisfaction = satisfaction;
    }

    /// Enables read-only plan facts on subsequently materialized exact edges.
    ///
    /// Enabling after graph construction would leave a mixture of annotated
    /// and unannotated edges, so the session rejects that ambiguous state.
    pub fn enable_plan_transition_annotations(
        &mut self,
    ) -> Result<(), LocalTurnGraphPlanAnnotationEnableError> {
        if self.used.exact_edges > 0 {
            return Err(LocalTurnGraphPlanAnnotationEnableError::EdgesAlreadyMaterialized);
        }
        self.collect_plan_transition_annotations = true;
        Ok(())
    }

    pub fn with_policy(
        root: CombatDecisionRoot,
        config: LocalTurnGraphWitnessConfig,
        policy: SharedCombatActionPolicy,
    ) -> Self {
        Self::with_policy_and_compiled_trajectory_query(root, config, policy, None)
    }

    pub fn with_policy_and_trajectory_query(
        root: CombatDecisionRoot,
        config: LocalTurnGraphWitnessConfig,
        policy: SharedCombatActionPolicy,
        query: &CombatTrajectoryQueryV1,
    ) -> Result<Self, CombatTrajectoryQueryCompileErrorV1> {
        let query = Arc::new(CompiledCombatTrajectoryQueryV1::compile(query)?);
        Ok(Self::with_policy_and_compiled_trajectory_query(
            root,
            config,
            policy,
            Some(query),
        ))
    }

    fn with_policy_and_compiled_trajectory_query(
        root: CombatDecisionRoot,
        config: LocalTurnGraphWitnessConfig,
        policy: SharedCombatActionPolicy,
        trajectory_query: Option<Arc<CompiledCombatTrajectoryQueryV1>>,
    ) -> Self {
        let original_root = root.position().clone();
        let root_exact_key = root
            .exact_state_key()
            .expect("a newly constructed combat root retains its exact key")
            .clone();
        let root_guides = policy.state_guides(root.position());
        let root_public_plan =
            public_combat_plan_v1(&public_combat_state_v1(&root.position().combat));
        let root_backed_guides = guide_rank_map(&root_guides);
        let root_trajectory_progress = trajectory_query
            .as_ref()
            .map(|query| query.initial_progress());
        let generator = turn_generator_for_potion_budget(
            root.clone(),
            config.generator,
            policy.clone(),
            config.max_potions_used,
            0,
            trajectory_query.clone(),
            root_trajectory_progress.clone(),
        );
        let root_generation_service_views =
            generation_service_views_from_lanes(generator.retained_guide_lanes());
        let mut nodes_by_exact_key = HashMap::with_hasher(FxBuildHasher);
        nodes_by_exact_key.insert(
            ConstrainedExactStateKey::new(
                root_exact_key,
                config.max_potions_used,
                0,
                root_public_plan.clone(),
                root_trajectory_progress.clone(),
            ),
            0,
        );
        let root_node = GraphNode {
            generator,
            public_plan: root_public_plan,
            public_plan_proposal_attempted: false,
            potion_expenditures: 0,
            diagnostic_parent: None,
            path_negative_log_policy: 0.0,
            path_atomic_depth: 0,
            relative_turn_depth: 0,
            visits: 0,
            first_service_selection: None,
            first_guide_service_selection: None,
            generated_options: 0,
            children: Vec::new(),
            guides: root_guides,
            generation_service_views: root_generation_service_views,
            next_generation_service_view: 0,
            widen_anchor_visits: 0,
            widen_proposal_root_visits: 0,
            widen_proposal_continuation_visits: 0,
            widen_guide_visits: BTreeMap::new(),
            boundary_anchor_services: 0,
            boundary_proposal_root_services: 0,
            boundary_proposal_continuation_services: 0,
            boundary_guide_services: 0,
            generation_anchor_services: 0,
            generation_guide_services: 0,
            backed_guides: root_backed_guides,
            synced_gaps: 0,
            exhausted: false,
        };
        let mut shared_agenda = SharedBoundaryAgenda::new(config.guide_service_bias);
        shared_agenda.publish_node(0, &root_node);
        let root_proposal_enqueued =
            plan_prefix_root_eligible(root_node.generator.root().position())
                && shared_agenda.publish_proposal_root(0, &root_node);
        Self {
            original_root,
            config,
            policy,
            trajectory_query,
            public_plan_turn_proposal: None,
            collect_plan_transition_annotations: false,
            shared_agenda,
            nodes: vec![root_node],
            nodes_by_exact_key,
            used: LocalTurnGraphWitnessCounters {
                exact_nodes: 1,
                plan_prefix_root_enqueues: usize::from(root_proposal_enqueued),
                ..LocalTurnGraphWitnessCounters::default()
            },
            performance_timing: LocalTurnGraphPerformanceTiming::default(),
            granted_selections: 0,
            granted_generation_work: 0,
            granted_engine_steps: 0,
            generation_gaps: Vec::new(),
            root_action_families: Vec::new(),
            witness: None,
            witness_frontier: Vec::new(),
            replay_failure: None,
        }
    }

    pub fn witness(&self) -> Option<&OracleCombatWitness> {
        self.witness.as_ref()
    }

    pub fn witness_frontier(&self) -> &[OracleCombatWitness] {
        &self.witness_frontier
    }

    pub fn restore_verified_witness(&mut self, witness: OracleCombatWitness) -> Result<(), String> {
        if self.trajectory_query.is_some() {
            return Err(
                "query-enabled local graph cannot restore a witness without replayed trajectory evidence"
                    .to_string(),
            );
        }
        if !sts_core::sim::combat::is_exact_combat_victory(&witness.final_position) {
            return Err("restored local-turn-graph witness is not terminal victory".to_string());
        }
        self.remember_witness(witness);
        Ok(())
    }

    pub(super) fn witness_satisfies(&self) -> bool {
        self.witness_frontier
            .iter()
            .any(|witness| self.witness_satisfies_config(witness))
    }

    pub(super) fn trajectory_progress_satisfies(
        &self,
        progress: Option<&CombatTrajectoryProgressV1>,
    ) -> bool {
        match (&self.trajectory_query, progress) {
            (None, None) => true,
            (Some(query), Some(progress)) => query.is_satisfied(progress),
            _ => false,
        }
    }

    pub(super) fn verify_witness_trajectory_query(
        &self,
        witness: &OracleCombatWitness,
        stepper: &dyn CombatStepper,
    ) -> Result<(), OracleCombatWitnessReplayError> {
        let Some(query) = self.trajectory_query.as_ref() else {
            return Ok(());
        };
        let events =
            replay_combat_trajectory_events_v1(&self.original_root, &witness.actions, stepper)?;
        query
            .evaluation_for_events(&events)
            .satisfied
            .then_some(())
            .ok_or(OracleCombatWitnessReplayError::TrajectoryQueryMismatch)
    }

    fn witness_satisfies_config(&self, witness: &OracleCombatWitness) -> bool {
        if !witness_within_potion_contract(
            &self.original_root,
            witness,
            self.config.max_potions_used,
            self.config.generator.allowed_potion_slots,
        ) {
            return false;
        }
        if self.config.require_no_unrecovered_stolen_gold
            && sts_combat_evidence::ai::combat_persistent_outcome_v1::unrecovered_stolen_gold(
                &witness.final_position.combat,
            ) > 0
        {
            return false;
        }
        let initial_hp = self.original_root.combat.entities.player.current_hp;
        let final_hp = witness.final_position.combat.entities.player.current_hp;
        let hp_loss = initial_hp.saturating_sub(final_hp).max(0) as u32;
        let root_external_burden =
            sts_combat_evidence::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1::from_combat(
                &self.original_root.combat,
            )
            .external_burden_count;
        let final_external_burden =
            sts_combat_evidence::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1::from_combat(
                &witness.final_position.combat,
            )
            .external_burden_count;
        let has_no_new_external_burden = final_external_burden <= root_external_burden;
        match self.config.satisfaction {
            OracleCombatWitnessSatisfaction::FirstWitness => true,
            OracleCombatWitnessSatisfaction::HpLossAtMost(limit) => hp_loss <= limit,
            OracleCombatWitnessSatisfaction::FinalHpAtLeast(minimum) => final_hp >= minimum,
            OracleCombatWitnessSatisfaction::FinalHpAndMaxHpAtLeast {
                final_hp: minimum_hp,
                final_max_hp: minimum_max_hp,
            } => {
                final_hp >= minimum_hp
                    && witness.final_position.combat.entities.player.max_hp >= minimum_max_hp
            }
            OracleCombatWitnessSatisfaction::FirstWitnessWithoutNewExternalBurden => {
                has_no_new_external_burden
            }
            OracleCombatWitnessSatisfaction::HpLossAtMostWithoutNewExternalBurden(limit) => {
                hp_loss <= limit && has_no_new_external_burden
            }
            OracleCombatWitnessSatisfaction::PotionFreeHpLossAtMostWithoutNewExternalBurden(
                limit,
            ) => {
                hp_loss <= limit
                    && has_no_new_external_burden
                    && witness_potion_expenditures(&self.original_root, witness) == 0
            }
            OracleCombatWitnessSatisfaction::BudgetOrExhaustion => false,
        }
    }

    pub(super) fn remember_witness(&mut self, witness: OracleCombatWitness) -> WitnessAdmission {
        if !witness_within_potion_contract(
            &self.original_root,
            &witness,
            self.config.max_potions_used,
            self.config.generator.allowed_potion_slots,
        ) {
            return WitnessAdmission::default();
        }
        let frontier_changed = remember_nondominated_witness(
            &self.original_root,
            &mut self.witness_frontier,
            &witness,
        );
        let selected = self.witness_frontier.iter().reduce(|best, candidate| {
            if witness_better_with_potion_budget(
                &self.original_root,
                candidate,
                best,
                self.config.max_potions_used,
            ) {
                candidate
            } else {
                best
            }
        });
        let selected_changed = match (self.witness.as_ref(), selected) {
            (None, Some(_)) => true,
            (Some(current), Some(next)) => current.actions != next.actions,
            _ => false,
        };
        self.witness = selected.cloned();
        WitnessAdmission {
            frontier_changed,
            selected_changed,
        }
    }
}
