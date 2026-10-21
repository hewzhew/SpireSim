//! Sparse semantic graph encoding for exact-branch complete-turn candidates.

use sts_combat_strategy::{
    CombatPlanMilestoneV1, CombatPlanTransitionAnnotationV1, CombatPlanTransitionV1,
};
use sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1;
use sts_oracle_eval::agent::information::action::{
    PublicCombatActionSurfaceV1, PublicCombatAtomicActionV1,
};
use sts_oracle_eval::agent::information::run::{
    PublicCombatRunContextGapV1, PublicCombatRunContextV1,
};
use sts_oracle_eval::agent::information::state::PublicCombatStateV1;
use sts_oracle_learning::eval::run_control::{
    CompleteTurnCandidateViewV1, CompleteTurnCompletedEndpointLayerViewV1,
    CompleteTurnDecisionViewV1, CompleteTurnObservationContractV1, CompleteTurnPrefixLayerViewV1,
    CompleteTurnPublicPrefixBoundaryV1, CompleteTurnSelectedActionV1, LearningCombatAtomicActionV1,
    LearningCombatIndexedChoiceV1, LearningCombatModelObservationV1,
    LearningCombatSelectionFamilyV1, COMPLETE_TURN_COMPLETED_ENDPOINT_LAYER_VIEW_SCHEMA,
    COMPLETE_TURN_DECISION_VIEW_SCHEMA, COMPLETE_TURN_PREFIX_LAYER_VIEW_SCHEMA,
};

use super::combat::CombatTokenIndex;
use super::{
    CategoricalField, CombatActionKind, CompleteTurnBoundaryKind, CompleteTurnMilestoneKind,
    RelationKind, ScalarField, SemanticBatchBuilder, SemanticCompleteness, SemanticEncodingError,
    TokenKind,
};

impl SemanticBatchBuilder {
    pub fn push_complete_turn_decision(
        &mut self,
        decision: &CompleteTurnDecisionViewV1,
    ) -> Result<(), SemanticEncodingError> {
        if decision.schema != COMPLETE_TURN_DECISION_VIEW_SCHEMA
            || decision.observation_contract
                != CompleteTurnObservationContractV1::ExactBranchObservedPublicSuccessor
        {
            return Err(SemanticEncodingError::CompleteTurnObservationContractMismatch);
        }
        self.push_complete_turn_candidates(&decision.root, &decision.candidates)
    }

    pub fn push_complete_turn_completed_endpoint_layer(
        &mut self,
        layer: &CompleteTurnCompletedEndpointLayerViewV1,
    ) -> Result<(), SemanticEncodingError> {
        if layer.schema != COMPLETE_TURN_COMPLETED_ENDPOINT_LAYER_VIEW_SCHEMA
            || layer.observation_contract
                != CompleteTurnObservationContractV1::ExactBranchObservedPublicCompletedEndpoint
        {
            return Err(SemanticEncodingError::CompleteTurnObservationContractMismatch);
        }
        self.push_complete_turn_candidates(&layer.root, &layer.candidates)
    }

    fn push_complete_turn_candidates(
        &mut self,
        root_state: &PublicCombatStateV1,
        candidates: &[CompleteTurnCandidateViewV1],
    ) -> Result<(), SemanticEncodingError> {
        self.completeness.push(SemanticCompleteness::Complete as u8);
        let detached_context = PublicCombatRunContextV1::Unavailable {
            reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        };
        let root = self.encode_combat_observation(
            LearningCombatModelObservationV1::from_public_state(&detached_context, root_state),
            false,
        )?;

        let before = self.candidate_token_indices.len();
        for candidate in candidates {
            let token = self.begin_candidate()?;
            self.edge(root.root, RelationKind::ObservationHasCandidate, token);
            self.category(
                token,
                CategoricalField::CompleteTurnBoundary,
                complete_turn_boundary_kind(candidate.boundary) as i64,
            );
            self.scalar(
                token,
                ScalarField::CompleteTurnPotionSpendCount,
                candidate.potion_slot_mask.count_ones(),
            );

            let successor = self.encode_combat_observation(
                LearningCombatModelObservationV1::from_public_state(
                    &detached_context,
                    &candidate.successor,
                ),
                false,
            )?;
            self.edge(token, RelationKind::CandidateHasSuccessor, successor.root);

            for (position, action) in candidate.actions.iter().enumerate() {
                self.encode_complete_turn_action(token, position, action, &root)?;
            }
            if let Some(annotation) = &candidate.plan_transition {
                self.encode_complete_turn_transition(token, annotation)?;
            }
            self.finish_candidate(token)?;
        }
        let actual = self.candidate_token_indices.len() - before;
        if actual != candidates.len() {
            return Err(SemanticEncodingError::CandidateAlignmentMismatch {
                expected: candidates.len(),
                actual,
            });
        }
        self.finish_row()
    }

    pub fn push_complete_turn_prefix_layer(
        &mut self,
        layer: &CompleteTurnPrefixLayerViewV1,
    ) -> Result<(), SemanticEncodingError> {
        if layer.schema != COMPLETE_TURN_PREFIX_LAYER_VIEW_SCHEMA
            || layer.observation_contract
                != CompleteTurnObservationContractV1::ExactBranchObservedPublicPartialPrefix
        {
            return Err(SemanticEncodingError::CompleteTurnObservationContractMismatch);
        }
        self.completeness.push(SemanticCompleteness::Complete as u8);
        let detached_context = PublicCombatRunContextV1::Unavailable {
            reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        };
        let root = self.encode_combat_observation(
            LearningCombatModelObservationV1::from_public_state(
                &detached_context,
                &layer.root.observation,
            ),
            false,
        )?;

        let before = self.candidate_token_indices.len();
        for candidate in &layer.candidates {
            let token = self.begin_candidate()?;
            self.edge(root.root, RelationKind::ObservationHasCandidate, token);
            let current = self.encode_combat_observation(
                LearningCombatModelObservationV1::from_public_state(
                    &detached_context,
                    &candidate.current.observation,
                ),
                false,
            )?;
            self.edge(token, RelationKind::CandidateHasSuccessor, current.root);
            for (position, action) in candidate.actions.iter().enumerate() {
                self.encode_complete_turn_action(token, position, action, &root)?;
            }
            match &candidate.current.boundary {
                CompleteTurnPublicPrefixBoundaryV1::Decision { action_surface } => {
                    self.category(
                        token,
                        CategoricalField::CompleteTurnBoundary,
                        CompleteTurnBoundaryKind::Other as i64,
                    );
                    self.encode_available_action_surface(token, action_surface, &current)?;
                }
                CompleteTurnPublicPrefixBoundaryV1::TerminalWin => self.category(
                    token,
                    CategoricalField::CompleteTurnBoundary,
                    CompleteTurnBoundaryKind::TerminalWin as i64,
                ),
                CompleteTurnPublicPrefixBoundaryV1::TerminalLoss => self.category(
                    token,
                    CategoricalField::CompleteTurnBoundary,
                    CompleteTurnBoundaryKind::TerminalLoss as i64,
                ),
            }
            self.finish_candidate(token)?;
        }
        let actual = self.candidate_token_indices.len() - before;
        if actual != layer.candidates.len() {
            return Err(SemanticEncodingError::CandidateAlignmentMismatch {
                expected: layer.candidates.len(),
                actual,
            });
        }
        self.finish_row()
    }

    fn encode_available_action_surface(
        &mut self,
        prefix_candidate: u64,
        surface: &PublicCombatActionSurfaceV1,
        current: &CombatTokenIndex,
    ) -> Result<(), SemanticEncodingError> {
        for action in &surface.atomic_actions {
            let token = self.add_token(TokenKind::CompleteTurnAction)?;
            self.edge(
                prefix_candidate,
                RelationKind::CandidateHasAvailableAction,
                token,
            );
            let semantics = match action {
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                } => LearningCombatAtomicActionV1::PlayCard {
                    hand_index: *hand_index,
                    target_monster_index: *target_monster_index,
                },
                PublicCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                } => LearningCombatAtomicActionV1::UsePotion {
                    potion_index: *potion_index,
                    target_monster_index: *target_monster_index,
                },
                PublicCombatAtomicActionV1::DiscardPotion { potion_index } => {
                    LearningCombatAtomicActionV1::DiscardPotion {
                        potion_index: *potion_index,
                    }
                }
                PublicCombatAtomicActionV1::EndTurn => LearningCombatAtomicActionV1::EndTurn,
                PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index } => {
                    let indexed = surface
                        .indexed_choice
                        .as_ref()
                        .ok_or(SemanticEncodingError::UnsupportedCompleteTurnSelectedAction)?;
                    let candidate = indexed
                        .candidates
                        .get(*choice_index)
                        .ok_or(SemanticEncodingError::UnsupportedCompleteTurnSelectedAction)?;
                    LearningCombatAtomicActionV1::SubmitIndexedChoice {
                        choice_index: *choice_index,
                        indexed: LearningCombatIndexedChoiceV1 {
                            input_encoding: indexed.input_encoding,
                            reason: &indexed.reason,
                            candidate,
                        },
                    }
                }
                PublicCombatAtomicActionV1::Proceed => LearningCombatAtomicActionV1::Proceed,
                PublicCombatAtomicActionV1::Cancel => LearningCombatAtomicActionV1::Cancel,
            };
            self.encode_atomic_action(token, semantics, current)?;
        }
        for family in &surface.selection_families {
            let token = self.add_token(TokenKind::CompleteTurnAction)?;
            self.edge(
                prefix_candidate,
                RelationKind::CandidateHasAvailableAction,
                token,
            );
            self.category(
                token,
                CategoricalField::CombatActionKind,
                CombatActionKind::BeginSelection as i64,
            );
            self.encode_selection_family(
                token,
                RelationKind::CandidateHasSelectionDomain,
                LearningCombatSelectionFamilyV1::from_public(family),
            )?;
        }
        Ok(())
    }

    fn encode_complete_turn_action(
        &mut self,
        candidate: u64,
        position: usize,
        selected: &CompleteTurnSelectedActionV1,
        root: &CombatTokenIndex,
    ) -> Result<(), SemanticEncodingError> {
        let token = self.add_token(TokenKind::CompleteTurnAction)?;
        self.edge(candidate, RelationKind::CandidateHasAction, token);
        self.scalar(token, ScalarField::CompleteTurnActionPosition, position);
        match selected {
            CompleteTurnSelectedActionV1::Atomic {
                action,
                card,
                potion,
                ..
            } => match action {
                PublicCombatAtomicActionV1::PlayCard {
                    target_monster_index,
                    ..
                } => {
                    self.category(
                        token,
                        CategoricalField::CombatActionKind,
                        CombatActionKind::PlayCard as i64,
                    );
                    let card = card
                        .as_ref()
                        .ok_or(SemanticEncodingError::UnsupportedCompleteTurnSelectedAction)?;
                    let payload = self.encode_combat_card(card, None, &root.monsters)?;
                    self.edge(token, RelationKind::CandidateHasPayload, payload);
                    self.encode_monster_target(token, *target_monster_index, &root.monsters)?;
                }
                PublicCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                } => {
                    self.category(
                        token,
                        CategoricalField::CombatActionKind,
                        CombatActionKind::UsePotion as i64,
                    );
                    self.scalar(token, ScalarField::ActionIndex, *potion_index);
                    let potion = potion
                        .as_ref()
                        .ok_or(SemanticEncodingError::UnsupportedCompleteTurnSelectedAction)?;
                    let payload = self.add_token(TokenKind::PotionSlot)?;
                    self.potion_identity_with_mechanics(
                        payload,
                        CategoricalField::PotionId,
                        potion.potion_id,
                    );
                    self.edge(token, RelationKind::CandidateHasPayload, payload);
                    self.encode_monster_target(token, *target_monster_index, &root.monsters)?;
                }
                PublicCombatAtomicActionV1::DiscardPotion { potion_index } => {
                    self.category(
                        token,
                        CategoricalField::CombatActionKind,
                        CombatActionKind::DiscardPotion as i64,
                    );
                    self.scalar(token, ScalarField::ActionIndex, *potion_index);
                }
                PublicCombatAtomicActionV1::EndTurn => self.category(
                    token,
                    CategoricalField::CombatActionKind,
                    CombatActionKind::EndTurn as i64,
                ),
                PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index } => {
                    self.category(
                        token,
                        CategoricalField::CombatActionKind,
                        CombatActionKind::SubmitIndexedChoice as i64,
                    );
                    self.scalar(token, ScalarField::ActionIndex, *choice_index);
                }
                PublicCombatAtomicActionV1::Proceed => self.category(
                    token,
                    CategoricalField::CombatActionKind,
                    CombatActionKind::Proceed as i64,
                ),
                PublicCombatAtomicActionV1::Cancel => self.category(
                    token,
                    CategoricalField::CombatActionKind,
                    CombatActionKind::Cancel as i64,
                ),
            },
            CompleteTurnSelectedActionV1::Selection {
                family,
                selected_domain_ordinals,
            } => {
                self.category(
                    token,
                    CategoricalField::CombatActionKind,
                    CombatActionKind::BeginSelection as i64,
                );
                let domains = self.encode_selection_family(
                    token,
                    RelationKind::CandidateHasSelectionDomain,
                    LearningCombatSelectionFamilyV1::from_public(family),
                )?;
                for (chosen_position, domain_ordinal) in
                    selected_domain_ordinals.iter().copied().enumerate()
                {
                    let target = domains.get(domain_ordinal).copied().ok_or(
                        SemanticEncodingError::MissingSelectionDomain(domain_ordinal),
                    )?;
                    let chosen = self.add_token(TokenKind::SelectionChosen)?;
                    self.edge(token, RelationKind::SelectionHasChosen, chosen);
                    self.edge(chosen, RelationKind::ChosenTargetsDomain, target);
                    self.scalar(
                        chosen,
                        ScalarField::SelectionChosenPosition,
                        chosen_position,
                    );
                }
            }
        }
        Ok(())
    }

    fn encode_complete_turn_transition(
        &mut self,
        candidate: u64,
        annotation: &CombatPlanTransitionAnnotationV1,
    ) -> Result<(), SemanticEncodingError> {
        let transition = complete_turn_transition(annotation);
        let token = self.add_token(TokenKind::CompleteTurnTransition)?;
        self.edge(candidate, RelationKind::CandidateHasTransition, token);
        for milestone in &transition.completed_milestones {
            let milestone_token = self.add_token(TokenKind::CompleteTurnMilestone)?;
            self.edge(token, RelationKind::TransitionHasMilestone, milestone_token);
            self.category(
                milestone_token,
                CategoricalField::CompleteTurnMilestone,
                complete_turn_milestone_kind(*milestone) as i64,
            );
        }
        Ok(())
    }
}

fn complete_turn_transition(
    annotation: &CombatPlanTransitionAnnotationV1,
) -> &CombatPlanTransitionV1 {
    match annotation {
        CombatPlanTransitionAnnotationV1::AwakenedOnePhaseControl(transition)
        | CombatPlanTransitionAnnotationV1::BronzeAutomatonControl(transition)
        | CombatPlanTransitionAnnotationV1::ChampPhaseControl(transition)
        | CombatPlanTransitionAnnotationV1::DonuAndDecaGrowthControl(transition) => transition,
    }
}

fn complete_turn_boundary_kind(
    boundary: ExactCompleteTurnProposalBoundaryV1,
) -> CompleteTurnBoundaryKind {
    match boundary {
        ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn => {
            CompleteTurnBoundaryKind::NextPlayerTurn
        }
        ExactCompleteTurnProposalBoundaryV1::TerminalWin => CompleteTurnBoundaryKind::TerminalWin,
        ExactCompleteTurnProposalBoundaryV1::TerminalLoss => CompleteTurnBoundaryKind::TerminalLoss,
        ExactCompleteTurnProposalBoundaryV1::Other => CompleteTurnBoundaryKind::Other,
    }
}

fn complete_turn_milestone_kind(milestone: CombatPlanMilestoneV1) -> CompleteTurnMilestoneKind {
    match milestone {
        CombatPlanMilestoneV1::EscalatingAddsRemoved => {
            CompleteTurnMilestoneKind::EscalatingAddsRemoved
        }
        CombatPlanMilestoneV1::AttackMitigationTargetExposed => {
            CompleteTurnMilestoneKind::AttackMitigationTargetExposed
        }
        CombatPlanMilestoneV1::WithheldSurvivalResourceRecovered => {
            CompleteTurnMilestoneKind::WithheldSurvivalResourceRecovered
        }
        CombatPlanMilestoneV1::ForcedAttackWindowReached => {
            CompleteTurnMilestoneKind::ForcedAttackWindowReached
        }
        CombatPlanMilestoneV1::ForcedAttackWindowSurvived => {
            CompleteTurnMilestoneKind::ForcedAttackWindowSurvived
        }
        CombatPlanMilestoneV1::ForcedAttackRecoveryWindowClosed => {
            CompleteTurnMilestoneKind::ForcedAttackRecoveryWindowClosed
        }
        CombatPlanMilestoneV1::UntaxedTransitionWindowReached => {
            CompleteTurnMilestoneKind::UntaxedTransitionWindowReached
        }
        CombatPlanMilestoneV1::TransitionWindowClosed => {
            CompleteTurnMilestoneKind::TransitionWindowClosed
        }
        CombatPlanMilestoneV1::SecondPhaseOpeningSurvived => {
            CompleteTurnMilestoneKind::SecondPhaseOpeningSurvived
        }
        CombatPlanMilestoneV1::ThresholdCommitted => CompleteTurnMilestoneKind::ThresholdCommitted,
        CombatPlanMilestoneV1::DebuffCleanseCompleted => {
            CompleteTurnMilestoneKind::DebuffCleanseCompleted
        }
        CombatPlanMilestoneV1::ExecuteWindowSurvived => {
            CompleteTurnMilestoneKind::ExecuteWindowSurvived
        }
        CombatPlanMilestoneV1::TeamGrowthSourceEliminated => {
            CompleteTurnMilestoneKind::TeamGrowthSourceEliminated
        }
        CombatPlanMilestoneV1::EncounterDefeated => CompleteTurnMilestoneKind::EncounterDefeated,
    }
}
