use crate::{
    action::{ResolvedRustActionV1, RustActionExecutionV1},
    bootstrap::{bind_session, BoundCardSourceV1},
    projection::{project_snapshot_v1, validate_combat_decision_scope_v2},
    rng_trace::RustRngTraceScopeV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use sts_oracle_run_control::{
    eval::run_control::{
        build_decision_surface, exact_card_reward_policy_prior_v1, AtomicCombatSearchOptionsV2,
        RunCombatResolutionKindV1, RunControlSession, RunDecisionAction, RunPolicyCandidateV1,
        RunProgressOutcome,
    },
    runtime::combat::{
        CardLineageId, CardLineageOrigin, CombatCardBirthCause, CombatCardBirthReceipt,
    },
    state::{
        core::{ClientInput, EngineState, RunPendingChoiceReason, RunPendingChoiceState},
        selection::DomainEventSource,
    },
};
use sts_parity_lockstep::{
    natural_map_node_id_v1, natural_run_card_binding_v1, natural_run_neow_drawback_card_source_v1,
    AdapterFailureKindV1, AdapterFailureV1, ExecutionInputsV1, LockstepAdapterV1,
    NaturalRunBootstrapV1, NaturalRunCardSourceV1, NaturalRunCombatCardBirthCauseV1,
    NaturalRunRngCallLedgerV1, ParitySnapshotV1, ProducerIdentityV1, SemanticActionV1,
    SemanticScalarV1,
};

#[path = "pandora.rs"]
mod pandora;
use pandora::PandoraGrantContext;

#[derive(Clone)]
pub struct PreparedRustActionV1 {
    based_on_observation_id: String,
    decision_step: u64,
    semantic: SemanticActionV1,
    execution: RustActionExecutionV1,
    based_on_generation: Arc<()>,
    successor: Option<Box<RustLockstepAdapterV1>>,
}

impl PreparedRustActionV1 {
    /// The native input already resolved by this adapter at its source boundary.
    /// UI-only preparation such as opening the smith selector has no input.
    /// Inspection grants no authority to execute against another boundary.
    pub fn resolved_input_v1(&self) -> Option<&ClientInput> {
        match &self.execution {
            RustActionExecutionV1::Candidate { input, .. }
            | RustActionExecutionV1::DirectInput { input } => Some(input),
            RustActionExecutionV1::BeginCampfireSmith => None,
        }
    }

    /// Source-bound run-control identity, when the input came from a candidate.
    pub fn resolved_candidate_id_v1(&self) -> Option<&str> {
        match &self.execution {
            RustActionExecutionV1::Candidate { candidate_id, .. } => Some(candidate_id),
            RustActionExecutionV1::DirectInput { .. }
            | RustActionExecutionV1::BeginCampfireSmith => None,
        }
    }
}

impl std::fmt::Debug for PreparedRustActionV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedRustActionV1")
            .field("based_on_observation_id", &self.based_on_observation_id)
            .field("decision_step", &self.decision_step)
            .field("semantic", &self.semantic)
            .field("execution", &self.execution)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct RustLockstepAdapterV1 {
    session: RunControlSession,
    producer: ProducerIdentityV1,
    bindings: BTreeMap<CardLineageId, BoundCardSourceV1>,
    current: Option<ParitySnapshotV1>,
    current_actions: Vec<ResolvedRustActionV1>,
    active_combat_room: Option<String>,
    next_combat_action_ordinal: u32,
    last_rng_call_ledger: Option<NaturalRunRngCallLedgerV1>,
    // Process-local predecessor identity. Exact forks share it until either
    // commits; separately reconstructed or diverged branches cannot adopt a
    // prepared successor just because their observation labels coincide.
    generation: Arc<()>,
}

struct RustCombatBirthContextV1 {
    rust_cause: CombatCardBirthCause,
    shared_cause: NaturalRunCombatCardBirthCauseV1,
}

impl RustLockstepAdapterV1 {
    pub fn new(
        session: RunControlSession,
        bootstrap: NaturalRunBootstrapV1,
        producer_version: impl Into<String>,
        session_epoch: impl Into<String>,
    ) -> Result<Self, String> {
        let producer = ProducerIdentityV1 {
            implementation: "sts_oracle_run_control".to_owned(),
            version: producer_version.into(),
            session_epoch: session_epoch.into(),
        };
        if producer.version.is_empty() || producer.session_epoch.is_empty() {
            return Err("Rust lockstep producer version and epoch must not be empty".to_owned());
        }
        let bindings = bind_session(&bootstrap, &session)?;
        Ok(Self {
            session,
            producer,
            bindings,
            current: None,
            current_actions: Vec::new(),
            active_combat_room: None,
            next_combat_action_ordinal: 0,
            last_rng_call_ledger: None,
            generation: Arc::new(()),
        })
    }

    pub fn session(&self) -> &RunControlSession {
        &self.session
    }

    /// Display-only event text attached to this observation's resolved actions.
    /// Candidate IDs join the existing resolver output; labels never resolve actions.
    pub fn event_action_labels_v1(&self) -> Vec<(SemanticActionV1, String)> {
        if !matches!(self.session.engine_state, EngineState::EventRoom) {
            return Vec::new();
        }
        let candidates = build_decision_surface(&self.session).view.candidates;
        self.current_actions
            .iter()
            .filter_map(|resolved| {
                if !matches!(
                    resolved.semantic,
                    SemanticActionV1::Select {
                        surface: sts_parity_lockstep::DecisionSurfaceV1::Event
                            | sts_parity_lockstep::DecisionSurfaceV1::Neow,
                        ..
                    }
                ) {
                    return None;
                }
                let RustActionExecutionV1::Candidate { candidate_id, .. } = &resolved.execution
                else {
                    return None;
                };
                let candidate = candidates
                    .iter()
                    .find(|candidate| &candidate.id == candidate_id)?;
                Some((resolved.semantic.clone(), candidate.label.clone()))
            })
            .collect()
    }

    /// Replays an analyst's complete semantic combat draft from this fixed
    /// root. It has no Java or journal authority and does not search.
    pub fn verify_semantic_combat_victory_v1(
        &self,
        actions: &[SemanticActionV1],
    ) -> Result<ParitySnapshotV1, String> {
        self.verify_semantic_combat_draft_v1(actions, false)
    }

    /// Verify a closed battle outcome: victory or explicit Smoke Bomb escape.
    /// Escape remains separate from the strict victory verifier and its rewards.
    pub fn verify_semantic_combat_completion_v1(
        &self,
        actions: &[SemanticActionV1],
    ) -> Result<ParitySnapshotV1, String> {
        self.verify_semantic_combat_draft_v1(actions, true)
    }

    fn verify_semantic_combat_draft_v1(
        &self,
        actions: &[SemanticActionV1],
        allow_escape: bool,
    ) -> Result<ParitySnapshotV1, String> {
        self.validate_combat_draft_root_v1()?;
        if actions.is_empty() {
            return Err("combat draft requires a nonempty line".to_owned());
        }
        let mut replay = self.clone();
        let mut snapshot = replay.observe().map_err(|error| error.message)?;
        for action in actions {
            if replay.session.active_combat.is_none()
                || (!allow_escape
                    && matches!(action, SemanticActionV1::UsePotion { potion_id, .. } if potion_id == "smokebomb"))
            {
                return Err("combat draft cannot escape or continue beyond combat".to_owned());
            }
            let prepared = replay
                .prepare(action, &snapshot)
                .map_err(|error| error.message)?;
            snapshot = replay.execute(prepared).map_err(|error| error.message)?;
        }
        let kind = if allow_escape && replay.session.run_state.room_smoked {
            RunCombatResolutionKindV1::SmokeBombEscape
        } else {
            RunCombatResolutionKindV1::CompleteVictory
        };
        validate_combat_trajectory_end_v1(&replay.session, kind)?;
        Ok(snapshot)
    }

    /// A repaired decision may be suspended inside a card or potion choice.
    /// Drafting and verification share this gate so those roots can continue
    /// privately without submitting an unverified selection to Java first.
    pub fn validate_combat_draft_root_v1(&self) -> Result<(), String> {
        if self.session.active_combat.is_none()
            || (self.session.engine_state != EngineState::CombatPlayerTurn
                && !crate::action::is_supported_discovery(&self.session)
                && !crate::action::is_supported_codex(&self.session)
                && !crate::action::is_supported_hand_select(&self.session)
                && !crate::action::is_supported_combat_grid_select(&self.session))
        {
            return Err("combat draft requires a supported active combat decision".to_owned());
        }
        validate_combat_decision_scope_v2(&self.session)
    }

    pub fn into_session(self) -> RunControlSession {
        self.session
    }

    /// Transition-scoped RNG calls from the most recently executed semantic
    /// action. Preparation probes and private searches never enter this scope.
    pub fn last_rng_call_ledger_v1(&self) -> Option<&NaturalRunRngCallLedgerV1> {
        self.last_rng_call_ledger.as_ref()
    }

    /// Search a private Rust clone, then re-resolve the adopted winning inputs
    /// one boundary at a time into the shared semantic action contract.
    ///
    /// The live adapter is not mutated. Callers must still submit every
    /// returned action through the lockstep runner.
    pub fn plan_winning_combat_trajectory_v1(
        &self,
        options: AtomicCombatSearchOptionsV2,
    ) -> Result<Vec<SemanticActionV1>, String> {
        let (kind, semantics) = self.plan_combat_trajectory_v1(options)?;
        if kind != RunCombatResolutionKindV1::CompleteVictory {
            return Err(format!(
                "Rust combat search returned {kind:?} instead of complete victory"
            ));
        }
        Ok(semantics)
    }

    /// Search a private Rust clone for either a complete win or an explicitly
    /// requested turn segment, then re-resolve every adopted input into the
    /// shared semantic action contract. The live adapter is never mutated.
    pub fn plan_combat_trajectory_v1(
        &self,
        options: AtomicCombatSearchOptionsV2,
    ) -> Result<(RunCombatResolutionKindV1, Vec<SemanticActionV1>), String> {
        if self.session.engine_state != EngineState::CombatPlayerTurn {
            return Err("Rust combat-trajectory planning requires a player combat turn".to_owned());
        }

        let mut searched = self.session.clone();
        let outcome = searched.apply_atomic_combat_search_v2(options)?;
        let resolution = outcome
            .single_combat_resolution()
            .ok_or_else(|| compact_combat_search_failure(&outcome))?;
        let kind = resolution.kind;
        let inputs = resolution
            .trajectory
            .actions
            .iter()
            .map(|action| action.input.clone())
            .collect::<Vec<_>>();
        if inputs.is_empty() {
            return Err("Rust combat search returned an empty trajectory".to_owned());
        }

        let (semantics, replay) = self.replay_semantic_combat_inputs_v1(&inputs)?;
        validate_combat_trajectory_end_v1(&replay, kind)?;
        Ok((kind, semantics))
    }

    /// Choose one already-projected card-reward action with the simulator's
    /// exact card-reward policy. The policy ranks legal Rust candidates; this
    /// adapter remains the sole owner of translating the selected candidate to
    /// the shared semantic action submitted to Java.
    pub fn preferred_card_reward_action_v1(&self) -> Result<SemanticActionV1, String> {
        if crate::projection::reward_state(&self.session)?
            .is_none_or(|reward| reward.pending_card_choice.is_none())
        {
            return Err("Rust card-reward planning requires an open card reward".to_owned());
        }

        let surface = build_decision_surface(&self.session);
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: candidate.id.as_str(),
                        label: candidate.label.as_str(),
                        action,
                    })
            })
            .collect::<Vec<_>>();
        let preferred = exact_card_reward_policy_prior_v1(&self.session, &legal)?
            .entries
            .into_iter()
            .next()
            .ok_or_else(|| "Rust card-reward policy returned no preferred action".to_owned())?
            .candidate_id;
        let matches = crate::action::resolved_actions_v1(&self.session, &self.bindings)?
            .into_iter()
            .filter(|resolved| {
                matches!(
                    &resolved.execution,
                    RustActionExecutionV1::Candidate { candidate_id, .. }
                        if candidate_id == &preferred
                )
            })
            .collect::<Vec<_>>();
        let [selected] = matches.as_slice() else {
            return Err(format!(
                "preferred Rust card-reward candidate resolved to {} shared actions instead of one",
                matches.len()
            ));
        };
        Ok(selected.semantic.clone())
    }

    /// Translate one portfolio-owned, replay-exact winning input line into
    /// the shared semantic action contract without mutating the live adapter.
    ///
    /// The caller still has to submit every returned semantic action through
    /// the lockstep runner. This bridge deliberately accepts no search-engine
    /// options: portfolio orchestration remains outside this adapter.
    pub fn semantic_winning_combat_trajectory_from_inputs_v1(
        &self,
        inputs: &[ClientInput],
    ) -> Result<Vec<SemanticActionV1>, String> {
        let (semantics, replay) = self.replay_semantic_combat_inputs_v1(inputs)?;
        validate_combat_trajectory_end_v1(&replay, RunCombatResolutionKindV1::CompleteVictory)?;
        Ok(semantics)
    }

    /// Translate one replay-exact combat prefix which remains at a player
    /// turn boundary. Prefixes have no commit authority; this exists so an
    /// orchestration owner can present compact guidance candidates without
    /// exposing runtime-local hand indices or card lineage values.
    pub fn semantic_combat_prefix_from_inputs_v1(
        &self,
        inputs: &[ClientInput],
    ) -> Result<Vec<SemanticActionV1>, String> {
        let (semantics, replay) = self.replay_semantic_combat_inputs_v1(inputs)?;
        validate_combat_trajectory_end_v1(&replay, RunCombatResolutionKindV1::TurnSegment)?;
        Ok(semantics)
    }

    /// Translate a complete escape independently of winning-trajectory claims.
    pub fn semantic_combat_escape_from_inputs_v1(
        &self,
        inputs: &[ClientInput],
    ) -> Result<Vec<SemanticActionV1>, String> {
        let (semantics, replay) = self.replay_semantic_combat_inputs_v1(inputs)?;
        validate_combat_trajectory_end_v1(&replay, RunCombatResolutionKindV1::SmokeBombEscape)?;
        Ok(semantics)
    }

    fn replay_semantic_combat_inputs_v1(
        &self,
        inputs: &[ClientInput],
    ) -> Result<(Vec<SemanticActionV1>, RunControlSession), String> {
        if self.session.engine_state != EngineState::CombatPlayerTurn {
            return Err("Rust combat-input replay requires a player combat turn".to_owned());
        }
        if inputs.is_empty() {
            return Err("Rust combat-input replay requires a non-empty trajectory".to_owned());
        }

        let mut replay = self.session.clone();
        let mut replay_bindings = self.bindings.clone();
        let room_id = current_combat_room(&replay)?;
        let mut action_ordinal = self.next_combat_action_ordinal;
        let mut semantics = Vec::with_capacity(inputs.len());
        for (step_index, input) in inputs.iter().enumerate() {
            let resolved = crate::action::resolved_actions_v1(&replay, &replay_bindings)?;
            let matches = resolved
                .iter()
                .filter(|candidate| {
                    matches!(
                        &candidate.execution,
                        RustActionExecutionV1::Candidate {
                            input: candidate_input,
                            ..
                        } | RustActionExecutionV1::DirectInput {
                            input: candidate_input,
                        } if candidate_input == input
                    )
                })
                .collect::<Vec<_>>();
            let [selected] = matches.as_slice() else {
                return Err(format!(
                    "Rust planned input {step_index} resolves to {} semantic actions instead of one",
                    matches.len()
                ));
            };
            semantics.push(selected.semantic.clone());
            let birth_context =
                rust_combat_birth_context(&replay, &replay_bindings, &selected.semantic)?;
            let receipt = execute_rust_action_v1(
                &mut replay,
                &selected.execution,
                &selected.semantic,
                birth_context.as_ref(),
            )
            .map_err(|message| {
                format!("Rust planned input {step_index} failed exact replay: {message}")
            })?;
            if let (Some(context), Some(receipt)) = (birth_context.as_ref(), receipt.as_ref()) {
                bind_rust_combat_births(
                    &mut replay_bindings,
                    &room_id,
                    action_ordinal,
                    &context.shared_cause,
                    receipt,
                )?;
            }
            if advances_combat_birth_ordinal(birth_context.as_ref(), receipt.as_ref()) {
                action_ordinal = action_ordinal
                    .checked_add(1)
                    .ok_or_else(|| "Rust combat action ordinal overflowed".to_owned())?;
            }
        }
        Ok((semantics, replay))
    }

    fn project_session(
        &self,
        session: &RunControlSession,
    ) -> Result<(ParitySnapshotV1, Vec<ResolvedRustActionV1>), AdapterFailureV1> {
        self.project_session_with_bindings(session, &self.bindings)
    }

    fn capture(&mut self) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        let combat_room = match &self.session.engine_state {
            EngineState::PendingChoice(
                sts_oracle_run_control::state::core::PendingChoice::CardRewardSelect { .. },
            ) if crate::action::is_supported_codex(&self.session) => {
                Some(current_combat_room(&self.session).map_err(|message| {
                    AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
                })?)
            }
            EngineState::PendingChoice(
                sts_oracle_run_control::state::core::PendingChoice::DiscoverySelect(_),
            ) if crate::action::is_supported_discovery(&self.session) => {
                Some(current_combat_room(&self.session).map_err(|message| {
                    AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
                })?)
            }
            EngineState::CombatPlayerTurn => {
                Some(current_combat_room(&self.session).map_err(|message| {
                    AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
                })?)
            }
            EngineState::PendingChoice(
                sts_oracle_run_control::state::core::PendingChoice::HandSelect { .. },
            ) if crate::action::is_supported_hand_select(&self.session) => {
                Some(current_combat_room(&self.session).map_err(|message| {
                    AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
                })?)
            }
            EngineState::PendingChoice(
                sts_oracle_run_control::state::core::PendingChoice::GridSelect { .. }
                | sts_oracle_run_control::state::core::PendingChoice::ScrySelect { .. },
            ) if crate::action::is_supported_combat_grid_select(&self.session) => {
                Some(current_combat_room(&self.session).map_err(|message| {
                    AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
                })?)
            }
            _ => None,
        };
        align_combat_lifecycle(
            &mut self.bindings,
            &mut self.active_combat_room,
            &mut self.next_combat_action_ordinal,
            combat_room,
        );
        let (snapshot, actions) = self.project_session(&self.session)?;
        self.current = Some(snapshot.clone());
        self.current_actions = actions;
        Ok(snapshot)
    }

    fn bind_transition_cards(
        &mut self,
        action: &SemanticActionV1,
        prior_lineages: &BTreeSet<CardLineageId>,
        deck_transform: bool,
    ) -> Result<(), AdapterFailureV1> {
        bind_transition_master_deck_cards(
            &self.session,
            &mut self.bindings,
            action,
            prior_lineages,
            deck_transform,
        )
        .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))
    }

    fn prepare_successor(
        &self,
        execution: &RustActionExecutionV1,
        semantic: &SemanticActionV1,
    ) -> Result<Box<Self>, AdapterFailureV1> {
        let mut successor = Box::new(Self {
            session: self.session.clone(),
            producer: self.producer.clone(),
            bindings: self.bindings.clone(),
            current: None,
            current_actions: Vec::new(),
            active_combat_room: self.active_combat_room.clone(),
            next_combat_action_ordinal: self.next_combat_action_ordinal,
            last_rng_call_ledger: None,
            generation: Arc::new(()),
        });
        // Execute and validate exactly once, privately. Dropping preparation
        // leaves the live session, bindings and published RNG ledger unchanged.
        successor.apply_successor(execution, semantic)?;
        Ok(successor)
    }

    fn prepare_successor_with_inputs(
        &self,
        execution: &RustActionExecutionV1,
        semantic: &SemanticActionV1,
        inputs: &ExecutionInputsV1,
    ) -> Result<Box<Self>, AdapterFailureV1> {
        use sts_oracle_run_control::runtime::event_time::EventTimeScope;
        inputs
            .validate()
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        let samples = inputs
            .secret_portal_playtime_bits
            .as_deref()
            .unwrap_or_default()
            .iter()
            .copied()
            .map(f32::from_bits)
            .collect();
        let scope = EventTimeScope::begin(samples)
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        let successor = self.prepare_successor(execution, semantic);
        let (provided, consumed) = scope.finish();
        if consumed > 0 && inputs.secret_portal_playtime_bits.is_none() {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Unobserved,
                "Secret Portal candidate filtering requires observed original-game playtime",
            ));
        }
        if provided != consumed {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Evidence,
                format!(
                    "Secret Portal playtime consumption differs: Java {provided}, Rust {consumed}"
                ),
            ));
        }
        successor
    }

    fn apply_successor(
        &mut self,
        execution: &RustActionExecutionV1,
        semantic: &SemanticActionV1,
    ) -> Result<(), AdapterFailureV1> {
        let prior_lineages = self
            .session
            .run_state
            .master_deck
            .iter()
            .map(|card| card.lineage)
            .collect::<BTreeSet<_>>();
        let deck_transform = is_deck_transform_selection(&self.session, semantic);
        let pandora_grant = PandoraGrantContext::prepare(&self.session, semantic)
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        self.last_rng_call_ledger = None;
        let rng_trace = RustRngTraceScopeV1::begin(&mut self.session)
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        let birth_context = rust_combat_birth_context(&self.session, &self.bindings, semantic)
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        let receipt = execute_rust_action_v1(
            &mut self.session,
            execution,
            semantic,
            birth_context.as_ref(),
        )
        .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Rejected, message))?;
        let rng_call_ledger = rng_trace
            .finish()
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        if let Some(context) = pandora_grant {
            context
                .bind(&self.session, &mut self.bindings)
                .map_err(|message| {
                    AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
                })?;
        } else {
            self.bind_transition_cards(semantic, &prior_lineages, deck_transform)?;
        }
        if birth_context.as_ref().is_some_and(|context| {
            context.shared_cause == NaturalRunCombatCardBirthCauseV1::CombatEntryResolution
        }) && receipt.is_some()
        {
            let room = current_combat_room(&self.session).map_err(|message| {
                AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
            })?;
            align_combat_lifecycle(
                &mut self.bindings,
                &mut self.active_combat_room,
                &mut self.next_combat_action_ordinal,
                Some(room),
            );
        }
        if let (Some(context), Some(receipt)) = (birth_context.as_ref(), receipt.as_ref()) {
            bind_rust_combat_births(
                &mut self.bindings,
                self.active_combat_room.as_deref().ok_or_else(|| {
                    AdapterFailureV1::new(
                        AdapterFailureKindV1::Evidence,
                        "Rust combat action has no active room identity",
                    )
                })?,
                self.next_combat_action_ordinal,
                &context.shared_cause,
                receipt,
            )
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        }
        if advances_combat_birth_ordinal(birth_context.as_ref(), receipt.as_ref()) {
            self.next_combat_action_ordinal = self
                .next_combat_action_ordinal
                .checked_add(1)
                .ok_or_else(|| {
                    AdapterFailureV1::new(
                        AdapterFailureKindV1::Evidence,
                        "Rust combat action ordinal overflowed",
                    )
                })?;
        }
        self.current = None;
        self.current_actions.clear();
        self.capture()?;
        self.last_rng_call_ledger = Some(rng_call_ledger);
        Ok(())
    }

    fn project_session_with_bindings(
        &self,
        session: &RunControlSession,
        bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
    ) -> Result<(ParitySnapshotV1, Vec<ResolvedRustActionV1>), AdapterFailureV1> {
        let supported_boundary = matches!(session.engine_state, EngineState::MapNavigation)
            || matches!(
                &session.engine_state,
                EngineState::MapOverlay { return_state }
                    if matches!(return_state.as_ref(), EngineState::RewardScreen(_))
            )
            || matches!(session.engine_state, EngineState::CombatPlayerTurn)
            || crate::action::is_supported_hand_select(session)
            || crate::action::is_supported_combat_grid_select(session)
            || crate::action::is_supported_discovery(session)
            || crate::action::is_supported_codex(session)
            || matches!(session.engine_state, EngineState::RewardScreen(_))
            || matches!(session.engine_state, EngineState::RewardOverlay { .. })
            || matches!(session.engine_state, EngineState::TreasureRoom(_))
            || matches!(session.engine_state, EngineState::BossRelicSelect(_))
            || matches!(
                session.engine_state,
                EngineState::DoubleBossComplete
                    | EngineState::Act3VictoryComplete
                    | EngineState::TrueVictoryComplete
            )
            || crate::action::is_heart_victory(session)
            || matches!(session.engine_state, EngineState::Campfire)
            || matches!(session.engine_state, EngineState::Shop(_))
            || matches!(session.engine_state, EngineState::EventRoom)
            || matches!(session.engine_state, EngineState::RunPendingChoice(_));
        if !supported_boundary {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Unsupported,
                format!(
                    "Rust lockstep v1 does not yet support boundary {:?}",
                    session.engine_state
                ),
            ));
        }
        validate_combat_decision_scope_v2(session)
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Unsupported, message))?;
        project_snapshot_v1(session, self.producer.clone(), bindings)
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))
    }
}

fn bind_transition_master_deck_cards(
    session: &RunControlSession,
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    action: &SemanticActionV1,
    prior_lineages: &BTreeSet<CardLineageId>,
    deck_transform: bool,
) -> Result<(), String> {
    if matches!(action, SemanticActionV1::Select {
        surface: sts_parity_lockstep::DecisionSurfaceV1::CardReward, choice,
    } if *choice == sts_parity_lockstep::natural_singing_bowl_choice_v1())
    {
        if session
            .run_state
            .master_deck
            .iter()
            .any(|card| !prior_lineages.contains(&card.lineage))
        {
            return Err("Singing Bowl unexpectedly acquired a master-deck card".into());
        }
        return Ok(());
    }
    if matches!(action, SemanticActionV1::EndTurn) {
        return bind_monster_grants(session, bindings, prior_lineages);
    }
    if let SemanticActionV1::Select {
        surface: sts_parity_lockstep::DecisionSurfaceV1::Event,
        choice,
    } = action
    {
        use sts_oracle_run_control::{content::cards::CardId, state::events::EventId};
        if choice.kind == "match_card" {
            let board = crate::match_game::board(
                session
                    .run_state
                    .event_state
                    .as_ref()
                    .filter(|event| event.id == EventId::MatchAndKeep)
                    .ok_or("Match and Keep action lost its event")?,
            )?;
            if board.first.is_none() {
                let [a, b] = board
                    .last_pair
                    .ok_or("match flip omitted the settled pair")?;
                if board.cards[a as usize].matched && board.cards[b as usize].matched {
                    let event = session.run_state.event_state.as_ref().unwrap();
                    let first_dealt = event.extra_data[15] as usize;
                    let card = event.card_payload[event.extra_data[first_dealt] as usize].card_id;
                    return bind_event_grant_at(
                        session,
                        bindings,
                        prior_lineages,
                        EventId::MatchAndKeep,
                        card,
                        1,
                        u32::from(4 - board.attempts_remaining),
                    );
                }
            }
        }
        let contract = match (
            choice.id.as_str(),
            choice.occurrence,
            choice.attributes.get("screen"),
        ) {
            ("ghosts", 0, Some(SemanticScalarV1::Integer(0))) => Some((
                EventId::Ghosts,
                CardId::Apparition,
                if session.run_state.ascension_level >= 15 {
                    3
                } else {
                    5
                },
            )),
            ("windinghalls", 0, Some(SemanticScalarV1::Integer(1))) => {
                Some((EventId::WindingHalls, CardId::Madness, 2))
            }
            ("windinghalls", 1, Some(SemanticScalarV1::Integer(1))) => {
                Some((EventId::WindingHalls, CardId::Writhe, 1))
            }
            ("goldenshrine", 1, Some(SemanticScalarV1::Integer(0))) => {
                Some((EventId::GoldenShrine, CardId::Regret, 1))
            }
            ("drugdealer", 0, Some(SemanticScalarV1::Integer(0))) => {
                Some((EventId::DrugDealer, CardId::JAX, 1))
            }
            ("addict", 1, Some(SemanticScalarV1::Integer(0))) => {
                Some((EventId::Addict, CardId::Shame, 1))
            }
            ("nest", 1, Some(SemanticScalarV1::Integer(1))) => {
                Some((EventId::Nest, CardId::RitualDagger, 1))
            }
            _ => None,
        };
        if let Some((event, card, count)) = contract {
            return bind_event_grant(session, bindings, prior_lineages, event, card, count);
        }
    }
    if deck_transform {
        rebind_transformed_master_deck_card(session, bindings, action, prior_lineages)?;
        return Ok(());
    }
    let source = if let Some(source) = natural_run_neow_drawback_card_source_v1(action)? {
        Some(source)
    } else if let SemanticActionV1::Select {
        surface: sts_parity_lockstep::DecisionSurfaceV1::CardReward,
        choice,
    } = action
    {
        if choice.kind != "reward_card"
            || !matches!(
                choice.attributes.get("upgrades"),
                Some(SemanticScalarV1::Integer(value)) if *value >= 0 && *value <= i64::from(u8::MAX)
            )
        {
            return Err("card reward action has an invalid typed card identity".to_owned());
        }
        if session
            .run_state
            .event_state
            .as_ref()
            .is_some_and(|event| event.id == sts_oracle_run_control::state::events::EventId::Neow)
            && session.run_state.map.current_x == -1
            && session.run_state.map.current_y == -1
        {
            Some(NaturalRunCardSourceV1::NeowCardReward { product_slot: 0 })
        } else {
            let room_id = natural_map_node_id_v1(
                i64::from(session.run_state.map.current_x),
                i64::from(session.run_state.map.current_y),
            )?;
            let product_slots = bindings
                .values()
                .filter_map(|bound| match &bound.source {
                    NaturalRunCardSourceV1::CardReward {
                        room_id: existing_room,
                        product_slot,
                    } if existing_room == &room_id => Some(*product_slot),
                    _ => None,
                })
                .collect::<BTreeSet<_>>();
            let next_slot = u32::try_from(product_slots.len())
                .map_err(|_| "card reward product count exceeds u32".to_owned())?;
            if product_slots.iter().copied().ne(0..next_slot) {
                return Err("card reward product slots are not contiguous".to_owned());
            }
            Some(NaturalRunCardSourceV1::CardReward {
                room_id,
                product_slot: next_slot,
            })
        }
    } else if let SemanticActionV1::SubmitSelection {
        surface: sts_parity_lockstep::DecisionSurfaceV1::GridSelect,
        choices,
    } = action
    {
        if let [choice] = choices.as_slice() {
            if choice.kind == "event_card_offer" {
                let event = session
                    .run_state
                    .event_state
                    .as_ref()
                    .ok_or("Rust event acquisition omitted event state")?;
                if event.id != sts_oracle_run_control::state::events::EventId::TheLibrary
                    || event.current_screen != 2
                    || choice.id != "thelibrary"
                    || choice.attributes.get("screen") != Some(&SemanticScalarV1::Integer(1))
                    || choice.occurrence >= 20
                {
                    return Err("Rust event acquisition has an invalid selected source".to_owned());
                }
                Some(NaturalRunCardSourceV1::EventCard {
                    room_id: natural_map_node_id_v1(
                        i64::from(session.run_state.map.current_x),
                        i64::from(session.run_state.map.current_y),
                    )?,
                    event_id: choice.id.clone(),
                    screen: 1,
                    offer_index: choice.occurrence,
                })
            } else {
                None
            }
        } else {
            None
        }
    } else if matches!(action, SemanticActionV1::Select {surface:sts_parity_lockstep::DecisionSurfaceV1::Shop,choice} if choice.kind=="shop_card")
    {
        let room_id = natural_map_node_id_v1(
            i64::from(session.run_state.map.current_x),
            i64::from(session.run_state.map.current_y),
        )?;
        let product_slot = bindings
            .values()
            .filter_map(|bound| match &bound.source {
                NaturalRunCardSourceV1::ShopPurchase {
                    room_id: room,
                    product_slot,
                } if room == &room_id => Some(*product_slot),
                _ => None,
            })
            .max()
            .map(|slot| {
                slot.checked_add(1)
                    .ok_or("shop acquisition ordinal overflow")
            })
            .transpose()?
            .unwrap_or(0);
        Some(NaturalRunCardSourceV1::ShopPurchase {
            room_id,
            product_slot,
        })
    } else {
        None
    };
    let Some(source) = source else {
        return Ok(());
    };

    let products = session
        .run_state
        .master_deck
        .iter()
        .filter(|card| !prior_lineages.contains(&card.lineage))
        .collect::<Vec<_>>();
    if products.is_empty() && matches!(source, NaturalRunCardSourceV1::NeowDrawback { .. }) {
        // Omamori can consume the Neow drawback before a card enters the deck.
        return Ok(());
    }
    let [card] = products.as_slice() else {
        return Err(format!(
            "typed master-deck acquisition produced {} Rust cards instead of one",
            products.len()
        ));
    };
    if let SemanticActionV1::Select {
        surface: sts_parity_lockstep::DecisionSurfaceV1::CardReward,
        choice,
    } = action
    {
        if sts_java_parity_protocol::canonical_game_id(card.id.stable_id()) != choice.id {
            return Err("Rust card reward acquired a different card ID".to_owned());
        }
    }
    let binding = natural_run_card_binding_v1(&source)?;
    if let SemanticActionV1::SubmitSelection { choices, .. } = action {
        if let [choice] = choices.as_slice() {
            if choice.kind == "event_card_offer"
                && (choice.attributes.get("card_id")
                    != Some(&SemanticScalarV1::Text(
                        sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                    ))
                    || choice.attributes.get("upgrades")
                        != Some(&SemanticScalarV1::Integer(i64::from(card.upgrades))))
            {
                return Err("Rust event acquired card differs from the selected offer".to_owned());
            }
        }
    }
    if bindings
        .values()
        .any(|existing| existing.binding == binding)
        || bindings.contains_key(&card.lineage)
    {
        return Err("typed master-deck acquisition repeated a stable binding".to_owned());
    }
    bindings.insert(
        card.lineage,
        BoundCardSourceV1 {
            binding,
            source,
            expected_card_id: sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
            expected_upgrades: card.upgrades,
        },
    );
    Ok(())
}

fn bind_event_grant(
    session: &RunControlSession,
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    prior_lineages: &BTreeSet<CardLineageId>,
    event_id: sts_oracle_run_control::state::events::EventId,
    expected_card: sts_oracle_run_control::content::cards::CardId,
    expected_count: usize,
) -> Result<(), String> {
    bind_event_grant_at(
        session,
        bindings,
        prior_lineages,
        event_id,
        expected_card,
        expected_count,
        0,
    )
}

fn bind_event_grant_at(
    session: &RunControlSession,
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    prior_lineages: &BTreeSet<CardLineageId>,
    event_id: sts_oracle_run_control::state::events::EventId,
    expected_card: sts_oracle_run_control::content::cards::CardId,
    expected_count: usize,
    first_slot: u32,
) -> Result<(), String> {
    use sts_oracle_run_control::state::{events::EventId, selection::DomainEvent};
    let (event_key, successor_screen) = match event_id {
        EventId::Ghosts => ("ghosts", 1),
        EventId::WindingHalls => ("windinghalls", 2),
        EventId::GoldenShrine => ("goldenshrine", 1),
        EventId::DrugDealer => ("drugdealer", 1),
        EventId::Addict => ("addict", 1),
        EventId::Nest => ("nest", 2),
        EventId::MatchAndKeep => (
            "matchandkeep",
            session
                .run_state
                .event_state
                .as_ref()
                .ok_or("missing Match and Keep successor")?
                .current_screen,
        ),
        _ => return Err("unscoped event grant".to_owned()),
    };
    let run = &session.run_state;
    if !run
        .event_state
        .as_ref()
        .is_some_and(|event| event.id == event_id && event.current_screen == successor_screen)
    {
        return Err("Rust event grant lacks its event successor".to_owned());
    }
    // The domain's CardObtained emissions witness construction order. Never
    // assign these same-named cards by their current master-deck positions.
    let products =
        run.emitted_events
            .iter()
            .filter_map(|event| match event {
                DomainEvent::CardObtained {
                    card,
                    source:
                        sts_oracle_run_control::state::selection::DomainEventSource::Event(
                            source_event,
                        ),
                } if *source_event == event_id && !prior_lineages.contains(&card.lineage) => Some(card),
                _ => None,
            })
            .collect::<Vec<_>>();
    let new_lineages = run
        .master_deck
        .iter()
        .filter(|card| !prior_lineages.contains(&card.lineage))
        .map(|card| card.lineage)
        .collect::<BTreeSet<_>>();
    let observed_lineages = products
        .iter()
        .map(|card| card.lineage)
        .collect::<BTreeSet<_>>();
    // Omamori can cancel the shrine's curse. With neither an acquisition
    // emission nor a new deck object there is no card to bind; the ordinary
    // relic/deck comparison still verifies the actual interception outcome.
    if matches!(
        event_id,
        EventId::GoldenShrine | EventId::MatchAndKeep | EventId::Addict
    ) && products.is_empty()
        && new_lineages.is_empty()
    {
        return Ok(());
    }
    if products.len() != expected_count
        || observed_lineages.len() != expected_count
        || new_lineages != observed_lineages
    {
        return Err("Rust event grant lacks complete unique acquisition witnesses".to_owned());
    }
    let room_id =
        natural_map_node_id_v1(i64::from(run.map.current_x), i64::from(run.map.current_y))?;
    for (slot, card) in products.into_iter().enumerate() {
        if card.id != expected_card
            || !run.master_deck.iter().any(|actual| {
                actual.lineage == card.lineage
                    && actual.id == card.id
                    && actual.upgrades == card.upgrades
            })
        {
            return Err("Rust event acquired a different card than its witness".to_owned());
        }
        let source = NaturalRunCardSourceV1::EventGrant {
            room_id: room_id.clone(),
            event_id: event_key.into(),
            product_slot: first_slot + slot as u32,
        };
        let binding = natural_run_card_binding_v1(&source)?;
        if bindings.contains_key(&card.lineage)
            || bindings.values().any(|bound| bound.binding == binding)
        {
            return Err("Rust event repeated an acquisition source".to_owned());
        }
        bindings.insert(
            card.lineage,
            BoundCardSourceV1 {
                binding,
                source,
                expected_card_id: sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                expected_upgrades: card.upgrades,
            },
        );
    }
    Ok(())
}

/// A transform replaces one selected deck object but preserves the logical
/// source-bound slot. Runtime UUIDs and Rust lineages both change; carrying
/// the selected binding forward is the cross-runtime identity witness.
fn rebind_transformed_master_deck_card(
    session: &RunControlSession,
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    action: &SemanticActionV1,
    prior_lineages: &BTreeSet<CardLineageId>,
) -> Result<(), String> {
    if let SemanticActionV1::Select {
        surface: sts_parity_lockstep::DecisionSurfaceV1::GridSelect,
        choice,
    } = action
    {
        return rebind_witnessed_deck_transforms(
            session,
            bindings,
            prior_lineages,
            DomainEventSource::Relic(sts_oracle_run_control::content::relics::RelicId::Astrolabe),
            sts_oracle_run_control::state::selection::SelectionReason::TransformUpgraded,
            &[None, None, Some(choice)],
        );
    }
    let SemanticActionV1::SubmitSelection {
        surface: sts_parity_lockstep::DecisionSurfaceV1::GridSelect,
        choices,
    } = action
    else {
        return Err("deck transform did not use the typed grid-selection surface".to_owned());
    };
    if choices.len() == 2
        && session
            .run_state
            .event_state
            .as_ref()
            .is_some_and(|e| e.id == sts_oracle_run_control::state::events::EventId::DrugDealer)
    {
        return rebind_witnessed_deck_transforms(
            session,
            bindings,
            prior_lineages,
            DomainEventSource::Event(sts_oracle_run_control::state::events::EventId::DrugDealer),
            sts_oracle_run_control::state::selection::SelectionReason::Transform,
            &choices.iter().map(Some).collect::<Vec<_>>(),
        );
    }
    let [selected] = choices.as_slice() else {
        return Err("typed deck mutation must select exactly one card".to_owned());
    };
    if selected.kind != "deck_card" || selected.occurrence != 0 {
        return Err("typed deck mutation has an invalid selected-card identity".to_owned());
    }
    let products = session
        .run_state
        .master_deck
        .iter()
        .filter(|card| !prior_lineages.contains(&card.lineage))
        .collect::<Vec<_>>();
    let [product] = products.as_slice() else {
        return Err(format!(
            "single-card deck transform produced {} new cards",
            products.len()
        ));
    };
    let matching = bindings
        .iter()
        .filter(|(_, bound)| bound.binding == selected.id)
        .map(|(lineage, bound)| (*lineage, bound.clone()))
        .collect::<Vec<_>>();
    let [(selected_lineage, rebound)] = matching.as_slice() else {
        return Err(format!(
            "selected deck binding resolves to {} Rust lineages instead of one",
            matching.len()
        ));
    };
    if session
        .run_state
        .master_deck
        .iter()
        .any(|card| card.lineage == *selected_lineage)
    {
        return Err("deck transform retained its selected Rust lineage".to_owned());
    }
    let mut rebound = rebound.clone();
    bindings.remove(selected_lineage);
    rebound.expected_card_id = sts_java_parity_protocol::canonical_game_id(product.id.stable_id());
    rebound.expected_upgrades = product.upgrades;
    if bindings.insert(product.lineage, rebound).is_some() {
        return Err("deck transform reused an existing Rust lineage".to_owned());
    }
    Ok(())
}

fn is_deck_transform_selection(session: &RunControlSession, action: &SemanticActionV1) -> bool {
    if let (
        EngineState::RunPendingChoice(choice),
        SemanticActionV1::Select {
            surface: sts_parity_lockstep::DecisionSurfaceV1::GridSelect,
            ..
        },
    ) = (&session.engine_state, action)
    {
        return crate::action::is_astrolabe_choice_v1(choice) && choice.selected.len() == 2;
    }
    matches!(
        (&session.engine_state, action),
        (
            EngineState::RunPendingChoice(choice),
            SemanticActionV1::SubmitSelection {
                surface: sts_parity_lockstep::DecisionSurfaceV1::GridSelect,
                ..
            }
        ) if matches!(
            choice.reason,
            RunPendingChoiceReason::Transform
                | RunPendingChoiceReason::TransformNonBottled
                | RunPendingChoiceReason::TransformUpgraded
        )
    )
}

fn rebind_witnessed_deck_transforms(
    session: &RunControlSession,
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    prior_lineages: &BTreeSet<CardLineageId>,
    source: DomainEventSource,
    reason: sts_oracle_run_control::state::selection::SelectionReason,
    expected_choices: &[Option<&sts_parity_lockstep::SemanticChoiceV1>],
) -> Result<(), String> {
    use sts_oracle_run_control::state::selection::{DomainEvent, SelectionScope};
    let count = expected_choices.len();
    let selections = session
        .run_state
        .emitted_events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::SelectionResolved {
                scope: SelectionScope::Deck,
                reason: actual_reason,
                selected,
                source: actual,
            } if *actual == source && *actual_reason == reason => Some(selected),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [selected] = selections.as_slice() else {
        return Err("Deck transform has no unique ordered selection witness".into());
    };
    if selected.len() != count
        || selected
            .iter()
            .map(|s| s.card_lineage())
            .collect::<BTreeSet<_>>()
            .len()
            != count
        || selected
            .iter()
            .zip(expected_choices)
            .any(|(target, choice)| {
                choice.is_some_and(|choice| {
                    bindings
                        .get(&target.card_lineage())
                        .is_none_or(|bound| bound.binding != choice.id)
                })
            })
    {
        return Err(
            "Deck transform selection witness disagrees with the final selected card".into(),
        );
    }
    let products = session
        .run_state
        .emitted_events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::CardTransformed {
                before,
                after,
                source: actual,
            } if *actual == source => Some((before, after)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let after_lineages = session
        .run_state
        .master_deck
        .iter()
        .map(|card| card.lineage)
        .collect::<BTreeSet<_>>();
    let selected_lineages = selected
        .iter()
        .map(|s| s.card_lineage())
        .collect::<BTreeSet<_>>();
    if products.len() != count
        || prior_lineages
            .difference(&after_lineages)
            .copied()
            .collect::<BTreeSet<_>>()
            != selected_lineages
        || products
            .iter()
            .map(|(before, _)| before.lineage)
            .collect::<BTreeSet<_>>()
            != selected_lineages
        || products
            .iter()
            .map(|(_, after)| after.lineage)
            .collect::<BTreeSet<_>>()
            != after_lineages
                .difference(prior_lineages)
                .copied()
                .collect::<BTreeSet<_>>()
        || after_lineages.difference(prior_lineages).count() != count
    {
        return Err(
            "Deck transform transformation witnesses do not cover the exact deck mutation".into(),
        );
    }
    let mut rebound = Vec::new();
    for (before, after) in products {
        let bound = bindings
            .get(&before.lineage)
            .ok_or("Deck transform predecessor binding missing")?;
        if bound.expected_card_id
            != sts_java_parity_protocol::canonical_game_id(before.id.stable_id())
            || !session.run_state.master_deck.iter().any(|card| {
                card.lineage == after.lineage
                    && card.id == after.id
                    && card.upgrades == after.upgrades
            })
        {
            return Err("Deck transform product disagrees with its transformation witness".into());
        }
        let mut updated = bound.clone();
        updated.expected_card_id =
            sts_java_parity_protocol::canonical_game_id(after.id.stable_id());
        updated.expected_upgrades = after.upgrades;
        rebound.push((before.lineage, after.lineage, updated));
    }
    for (before, after, bound) in rebound {
        bindings.remove(&before);
        if bindings.insert(after, bound).is_some() {
            return Err("Deck transform reused a bound product lineage".into());
        }
    }
    Ok(())
}

fn compact_combat_search_failure(outcome: &RunProgressOutcome) -> String {
    let details = outcome
        .message
        .lines()
        .map(str::trim)
        .filter(|line| {
            [
                "result=",
                "best_complete_candidate",
                "detail=",
                "coverage_status=",
                "terminal_wins=",
                "nodes_expanded=",
                "nodes_generated=",
                "coverage_reason=",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        })
        .take(8)
        .collect::<Vec<_>>();
    let rejection = outcome
        .atomic_combat_search_rejection
        .map(|rejection| format!("{rejection:?}"))
        .unwrap_or_else(|| "none".to_owned());
    if details.is_empty() {
        format!("Rust combat search returned no single combat resolution; rejection={rejection}")
    } else {
        format!(
            "Rust combat search returned no single combat resolution; rejection={rejection}; {}",
            details.join("; ")
        )
    }
}

fn execute_rust_action_v1(
    session: &mut RunControlSession,
    execution: &RustActionExecutionV1,
    semantic: &SemanticActionV1,
    birth_context: Option<&RustCombatBirthContextV1>,
) -> Result<Option<CombatCardBirthReceipt>, String> {
    match execution {
        RustActionExecutionV1::Candidate { candidate_id, .. } => {
            execute_with_combat_birth_observation(session, birth_context, |session| {
                session
                    .execute_candidate_transaction(candidate_id)
                    .map(|_| ())
            })
        }
        RustActionExecutionV1::DirectInput { input } => {
            execute_with_combat_birth_observation(session, birth_context, |session| {
                session
                    .apply_decision_action(RunDecisionAction::Input(input.clone()))
                    .map(|_| ())
            })
        }
        RustActionExecutionV1::BeginCampfireSmith => {
            if birth_context.is_some()
                || !matches!(session.engine_state, EngineState::Campfire)
                || !matches!(
                    semantic,
                    SemanticActionV1::Select {
                        surface: sts_parity_lockstep::DecisionSurfaceV1::Campfire,
                        choice,
                    } if choice.kind == "campfire_option" && choice.id == "smith" && choice.occurrence == 0
                )
                || !sts_oracle_run_control::engine::campfire_handler::get_available_options(
                    &session.run_state,
                )
                .iter()
                .any(|choice| {
                    matches!(
                        choice,
                        sts_oracle_run_control::state::core::CampfireChoice::Smith(_)
                    )
                })
            {
                return Err(
                    "Rust campfire Smith staging no longer matches its typed action".to_owned(),
                );
            }
            session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: 1,
                max_choices: 1,
                reason: RunPendingChoiceReason::Upgrade,
                source: DomainEventSource::CampfireSmith,
                return_state: Box::new(EngineState::MapNavigation),
            });
            session.decision_step = session.decision_step.saturating_add(1);
            Ok(None)
        }
    }
}

fn validate_combat_trajectory_end_v1(
    replay: &RunControlSession,
    kind: RunCombatResolutionKindV1,
) -> Result<(), String> {
    match kind {
        RunCombatResolutionKindV1::CompleteVictory
            if matches!(
                replay.engine_state,
                EngineState::RewardScreen(_)
                    | EngineState::DoubleBossComplete
                    | EngineState::Act3VictoryComplete
                    | EngineState::TrueVictoryComplete
            ) && replay.active_combat.is_none()
                && !replay.run_state.room_smoked =>
        {
            Ok(())
        }
        RunCombatResolutionKindV1::TurnSegment
            if replay.engine_state == EngineState::CombatPlayerTurn
                && replay.active_combat.is_some() =>
        {
            Ok(())
        }
        RunCombatResolutionKindV1::SmokeBombEscape
            if replay.active_combat.is_none()
                && replay.run_state.room_smoked
                && replay.run_state.current_hp > 0
                && matches!(&replay.engine_state, EngineState::RewardScreen(rewards)
                if rewards.screen_context == sts_oracle_run_control::state::rewards::RewardScreenContext::SmokedCombat
                    && rewards.items.is_empty() && rewards.pending_card_choice.is_none()
                    && rewards.pending_card_reward_index.is_none() && rewards.skippable) =>
        {
            Ok(())
        }
        _ => Err(format!(
            "Rust {kind:?} trajectory ended at an incompatible {:?} boundary",
            replay.engine_state
        )),
    }
}

fn current_combat_room(session: &RunControlSession) -> Result<String, String> {
    if session.engine_state != EngineState::CombatPlayerTurn
        && !crate::action::is_supported_hand_select(session)
        && !crate::action::is_supported_combat_grid_select(session)
        && !crate::action::is_supported_discovery(session)
        && !crate::action::is_supported_codex(session)
    {
        return Err("Rust combat birth source requires a player combat boundary".to_owned());
    }
    natural_map_node_id_v1(
        i64::from(session.run_state.map.current_x),
        i64::from(session.run_state.map.current_y),
    )
}

fn align_combat_lifecycle(
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    active_combat_room: &mut Option<String>,
    next_combat_action_ordinal: &mut u32,
    observed_combat_room: Option<String>,
) {
    if active_combat_room.as_deref() == observed_combat_room.as_deref() {
        return;
    }
    bindings
        .retain(|_, bound| !matches!(&bound.source, NaturalRunCardSourceV1::CombatBirth { .. }));
    *active_combat_room = observed_combat_room;
    *next_combat_action_ordinal = 0;
}

fn rust_combat_birth_context(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
    semantic: &SemanticActionV1,
) -> Result<Option<RustCombatBirthContextV1>, String> {
    if session.run_state.player_class == "Watcher"
        && (matches!(
            semantic,
            SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Map,
                ..
            }
        ) || (session.engine_state == EngineState::DoubleBossComplete
            && matches!(
                semantic,
                SemanticActionV1::Proceed {
                    surface: sts_parity_lockstep::DecisionSurfaceV1::DoubleBoss
                }
            )))
    {
        return Ok(Some(RustCombatBirthContextV1 {
            rust_cause: CombatCardBirthCause::CombatEntryResolution,
            shared_cause: NaturalRunCombatCardBirthCauseV1::CombatEntryResolution,
        }));
    }
    if matches!(
        semantic,
        SemanticActionV1::SubmitSelection {
            surface: sts_parity_lockstep::DecisionSurfaceV1::HandSelect,
            ..
        }
    ) && crate::action::is_supported_hand_select(session)
        || matches!(
            semantic,
            SemanticActionV1::SubmitSelection {
                surface: sts_parity_lockstep::DecisionSurfaceV1::GridSelect,
                ..
            }
        ) && crate::action::is_supported_combat_grid_select(session)
    {
        return Ok(Some(RustCombatBirthContextV1 {
            rust_cause: CombatCardBirthCause::CombatSelectionResolution,
            shared_cause: NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution,
        }));
    }
    if crate::action::is_supported_codex(session) {
        return match semantic {
            SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::CombatCodex,
                ..
            }
            | SemanticActionV1::Cancel {
                surface: sts_parity_lockstep::DecisionSurfaceV1::CombatCodex,
            } => Ok(Some(RustCombatBirthContextV1 {
                rust_cause: CombatCardBirthCause::CombatSelectionResolution,
                shared_cause: NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution,
            })),
            SemanticActionV1::DiscardPotion { .. } => Ok(None),
            _ => Err("Rust Codex resolution requires its typed choice".to_owned()),
        };
    }
    if crate::action::is_supported_discovery(session) {
        return match semantic {
            SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::CombatDiscovery,
                ..
            }
            | SemanticActionV1::Cancel {
                surface: sts_parity_lockstep::DecisionSurfaceV1::CombatDiscovery,
            } => Ok(Some(RustCombatBirthContextV1 {
                rust_cause: CombatCardBirthCause::DiscoveryResolution,
                shared_cause: NaturalRunCombatCardBirthCauseV1::DiscoveryResolution,
            })),
            SemanticActionV1::DiscardPotion { .. } => Ok(None),
            _ => Err("Rust Discovery resolution requires its typed choice".to_owned()),
        };
    }
    if session.engine_state != EngineState::CombatPlayerTurn {
        return Ok(None);
    }
    let context = match semantic {
        SemanticActionV1::PlayCard { card_binding, .. } => {
            let matching = bindings
                .iter()
                .filter(|(_, source)| &source.binding == card_binding)
                .map(|(lineage, _)| *lineage)
                .collect::<Vec<_>>();
            let [producer_lineage] = matching.as_slice() else {
                return Err(format!(
                    "Rust card-play binding resolves to {} lineages instead of one",
                    matching.len()
                ));
            };
            RustCombatBirthContextV1 {
                rust_cause: CombatCardBirthCause::CardPlay {
                    producer_lineage: *producer_lineage,
                },
                shared_cause: NaturalRunCombatCardBirthCauseV1::CardPlay {
                    producer_binding: card_binding.clone(),
                },
            }
        }
        SemanticActionV1::EndTurn => RustCombatBirthContextV1 {
            rust_cause: CombatCardBirthCause::EndTurnResolution,
            shared_cause: NaturalRunCombatCardBirthCauseV1::EndTurnResolution,
        },
        SemanticActionV1::UsePotion {
            slot, potion_id, ..
        } => {
            let potion = session
                .active_combat
                .as_ref()
                .and_then(|active| active.combat_state.entities.potions.get(usize::from(*slot)))
                .and_then(Option::as_ref)
                .ok_or_else(|| "Rust potion-use birth source points to an empty slot".to_owned())?;
            if sts_java_parity_protocol::canonical_game_id(potion.id.stable_id()) != *potion_id {
                return Err(
                    "Rust potion-use birth source disagrees with the semantic action".to_owned(),
                );
            }
            RustCombatBirthContextV1 {
                rust_cause: CombatCardBirthCause::PotionUse {
                    potion_id: potion.id,
                    slot: usize::from(*slot),
                },
                shared_cause: NaturalRunCombatCardBirthCauseV1::PotionUse {
                    potion_id: potion_id.clone(),
                    slot: *slot,
                },
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(context))
}

fn advances_combat_birth_ordinal(
    context: Option<&RustCombatBirthContextV1>,
    receipt: Option<&CombatCardBirthReceipt>,
) -> bool {
    // Selection movement alone does not allocate a birth scope. A resumed
    // action queue can create cards; only its nonempty receipt owns one.
    context.is_some_and(|context| match context.shared_cause {
        NaturalRunCombatCardBirthCauseV1::CombatEntryResolution => receipt.is_some(),
        NaturalRunCombatCardBirthCauseV1::CombatSelectionResolution => {
            receipt.is_some_and(|receipt| !receipt.products.is_empty())
        }
        _ => true,
    })
}

fn execute_with_combat_birth_observation(
    session: &mut RunControlSession,
    context: Option<&RustCombatBirthContextV1>,
    execute: impl FnOnce(&mut RunControlSession) -> Result<(), String>,
) -> Result<Option<CombatCardBirthReceipt>, String> {
    let entry = context
        .is_some_and(|context| context.rust_cause == CombatCardBirthCause::CombatEntryResolution);
    if let Some(context) = context.filter(|_| !entry) {
        session
            .active_combat
            .as_mut()
            .ok_or_else(|| "Rust combat birth source has no active combat".to_owned())?
            .combat_state
            .begin_card_birth_observation(context.rust_cause.clone());
    }
    if entry {
        sts_oracle_run_control::sim::combat_start::with_combat_entry_birth_observation(|| {
            execute(session)
        })?;
    } else {
        execute(session)?;
    }
    let Some(context) = context else {
        return Ok(None);
    };
    let Some(active) = session.active_combat.as_mut() else {
        // Combat-only cards have no observable successor identity after the
        // combat has already closed.
        return Ok(None);
    };
    let receipt = active
        .combat_state
        .take_card_birth_observation()
        .ok_or_else(|| "Rust combat transition lost its card-birth receipt".to_owned())?;
    if receipt.cause != context.rust_cause {
        return Err("Rust card-birth receipt disagrees with its typed action cause".to_owned());
    }
    Ok(Some(receipt))
}

fn bind_rust_combat_births(
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    room_id: &str,
    action_ordinal: u32,
    cause: &NaturalRunCombatCardBirthCauseV1,
    receipt: &CombatCardBirthReceipt,
) -> Result<(), String> {
    if !receipt.complete || receipt.unsupported_reason.is_some() {
        return Err(format!(
            "Rust combat card-birth receipt is incomplete{}",
            receipt
                .unsupported_reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default()
        ));
    }
    if usize::try_from(receipt.allocated_count) != Ok(receipt.products.len())
        || receipt
            .products
            .iter()
            .enumerate()
            .any(|(index, product)| u32::try_from(index) != Ok(product.generation_index))
    {
        return Err("Rust combat card-birth receipt has non-contiguous products".to_owned());
    }
    for product in &receipt.products {
        if product.card.lineage.origin() != CardLineageOrigin::CombatGenerated {
            return Err("Rust combat card birth has non-generated lineage".to_owned());
        }
        if let Some(copy) = &product.autoplay_copy {
            if !bindings.contains_key(&copy.producer_lineage) {
                return Err("Rust auto-play copy has no previously witnessed producer".to_owned());
            }
        }
        let source = NaturalRunCardSourceV1::CombatBirth {
            room_id: room_id.to_owned(),
            action_ordinal,
            cause: cause.clone(),
            product_slot: product.generation_index,
        };
        let binding = natural_run_card_binding_v1(&source)?;
        if let Some(existing) = bindings.get(&product.card.lineage) {
            return Err(format!(
                "Rust combat card birth repeats lineage {}: existing source {:?}, new source {:?}",
                product.card.lineage, existing.source, source
            ));
        }
        if let Some(existing) = bindings
            .values()
            .find(|existing| existing.binding == binding)
        {
            return Err(format!(
                "Rust combat card birth repeats stable binding {binding}: existing source {:?}, new lineage {}",
                existing.source, product.card.lineage
            ));
        }
        bindings.insert(
            product.card.lineage,
            BoundCardSourceV1 {
                binding,
                source,
                expected_card_id: sts_java_parity_protocol::canonical_game_id(
                    product.card.id.stable_id(),
                ),
                expected_upgrades: product.card.upgrades,
            },
        );
    }
    Ok(())
}

impl LockstepAdapterV1 for RustLockstepAdapterV1 {
    type PreparedAction = PreparedRustActionV1;

    fn observe(&mut self) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        self.capture()
    }

    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        based_on: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, AdapterFailureV1> {
        let _timing = sts_parity_lockstep::timing_span_v1("rust.prepare");
        let Some(current) = self.current.as_ref() else {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Rejected,
                "Rust adapter must observe before preparing an action",
            ));
        };
        if current != based_on {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Rejected,
                "Rust action was based on a stale or foreign observation",
            ));
        }
        let matches = self
            .current_actions
            .iter()
            .filter(|candidate| &candidate.semantic == action)
            .collect::<Vec<_>>();
        let [resolved] = matches.as_slice() else {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Unsupported,
                format!(
                    "Rust action resolves to {} current candidates instead of one",
                    matches.len()
                ),
            ));
        };
        let semantic = resolved.semantic.clone();
        let execution = resolved.execution.clone();
        // Map entry may consume a real-time input after the Java command has
        // begun. Resolve legality now, but do not precompute that successor
        // with the standalone simulator clock. Combat keeps its fast path.
        let successor = if self.session.run_state.act_num == 3
            && matches!(
                semantic,
                SemanticActionV1::Select {
                    surface: sts_parity_lockstep::DecisionSurfaceV1::Map,
                    ..
                }
            ) {
            None
        } else {
            Some(self.prepare_successor_with_inputs(
                &execution,
                &semantic,
                &ExecutionInputsV1::default(),
            )?)
        };

        Ok(PreparedRustActionV1 {
            based_on_observation_id: based_on.observation_id.clone(),
            decision_step: self.session.decision_step,
            semantic,
            execution,
            based_on_generation: Arc::clone(&self.generation),
            successor,
        })
    }

    fn execute(
        &mut self,
        prepared: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        self.execute_with_inputs_v1(prepared, &ExecutionInputsV1::default())
    }

    fn execute_with_inputs_v1(
        &mut self,
        prepared: Self::PreparedAction,
        inputs: &ExecutionInputsV1,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        let _timing = sts_parity_lockstep::timing_span_v1("rust.execute");
        let current = self.current.as_ref().ok_or_else(|| {
            AdapterFailureV1::new(
                AdapterFailureKindV1::Rejected,
                "Rust adapter has no current observation",
            )
        })?;
        if !Arc::ptr_eq(&self.generation, &prepared.based_on_generation)
            || current.observation_id != prepared.based_on_observation_id
            || self.session.decision_step != prepared.decision_step
            || !self.current_actions.iter().any(|candidate| {
                candidate.semantic == prepared.semantic && candidate.execution == prepared.execution
            })
        {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Rejected,
                "Rust prepared action became stale before execution",
            ));
        }
        inputs
            .validate()
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        let successor = match prepared.successor {
            Some(successor) => {
                if inputs.has_samples() {
                    return Err(AdapterFailureV1::new(
                        AdapterFailureKindV1::Evidence,
                        "Java consumed Secret Portal playtime but Rust did not",
                    ));
                }
                successor
            }
            None => {
                self.prepare_successor_with_inputs(&prepared.execution, &prepared.semantic, inputs)?
            }
        };
        let snapshot = successor
            .current
            .as_ref()
            .ok_or_else(|| {
                AdapterFailureV1::new(
                    AdapterFailureKindV1::Evidence,
                    "Rust prepared successor has no verified observation",
                )
            })?
            .clone();
        *self = *successor;
        Ok(snapshot)
    }

    fn last_rng_call_ledger_v1(&self) -> Option<&NaturalRunRngCallLedgerV1> {
        self.last_rng_call_ledger.as_ref()
    }
}

fn bind_monster_grants(
    session: &RunControlSession,
    bindings: &mut BTreeMap<CardLineageId, BoundCardSourceV1>,
    prior_lineages: &BTreeSet<CardLineageId>,
) -> Result<(), String> {
    use sts_oracle_run_control::{
        content::{cards::CardId, monsters::EnemyId},
        state::selection::{DomainEvent, DomainEventSource},
    };
    let run = &session.run_state;
    let products = run
        .emitted_events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::CardObtained {
                card,
                source:
                    DomainEventSource::CombatMonsterGrant {
                        monster_id,
                        monster_slot,
                    },
            } if !prior_lineages.contains(&card.lineage) => {
                Some((card, *monster_id, *monster_slot))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let new_lineages = run
        .master_deck
        .iter()
        .filter(|card| !prior_lineages.contains(&card.lineage))
        .map(|card| card.lineage)
        .collect::<BTreeSet<_>>();
    let observed = products
        .iter()
        .map(|(card, _, _)| card.lineage)
        .collect::<BTreeSet<_>>();
    if new_lineages != observed || observed.len() != products.len() {
        return Err("Rust monster grant lacks complete unique acquisition witnesses".into());
    }
    let room_id =
        natural_map_node_id_v1(i64::from(run.map.current_x), i64::from(run.map.current_y))?;
    for (card, monster_id, monster_slot) in products {
        if monster_id != EnemyId::WrithingMass
            || card.id != CardId::Parasite
            || card.upgrades != 0
            || !run.master_deck.iter().any(|actual| {
                actual.lineage == card.lineage
                    && actual.id == card.id
                    && actual.upgrades == card.upgrades
            })
        {
            return Err("Rust monster grant differs from its supported acquisition witness".into());
        }
        let source = NaturalRunCardSourceV1::MonsterGrant {
            room_id: room_id.clone(),
            monster_id: "writhingmass".into(),
            monster_slot,
            product_slot: 0,
        };
        let binding = natural_run_card_binding_v1(&source)?;
        if bindings.contains_key(&card.lineage)
            || bindings.values().any(|bound| bound.binding == binding)
        {
            return Err("Rust monster repeated a permanent acquisition source".into());
        }
        bindings.insert(
            card.lineage,
            BoundCardSourceV1 {
                binding,
                source,
                expected_card_id: "parasite".into(),
                expected_upgrades: card.upgrades,
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_search_failure_keeps_resource_rejection_and_open_coverage() {
        let outcome = RunProgressOutcome::progress(
            "Search combat did not modify state.\n\
             result=complete_winning_candidate_exceeds_hp_loss_limit\n\
             detail=candidate_hp_loss=31 max_hp_loss=25\n\
             best_complete_candidate terminal=Victory final_hp=45 hp_loss=31 turns=8 cards_played=22 potions_used=0 actions=30\n\
             coverage_status=TimeBudgetLimited\n\
             terminal_wins=3\n\
             nodes_expanded=100\n\
             nodes_generated=200\n\
             coverage_reason=wall-clock deadline reached with frontier still open",
        );
        let error = compact_combat_search_failure(&outcome);
        assert!(error.contains("candidate_hp_loss=31 max_hp_loss=25"));
        assert!(error.contains("final_hp=45 hp_loss=31"));
        assert!(
            error.contains("coverage_reason=wall-clock deadline reached with frontier still open")
        );
    }

    #[test]
    fn monster_grant_requires_typed_witness_and_survives_deck_reordering() {
        use sts_oracle_run_control::{
            content::{cards::CardId, monsters::EnemyId},
            state::selection::DomainEventSource,
        };
        let mut session = RunControlSession::new(Default::default());
        session.run_state.map.current_x = 3;
        session.run_state.map.current_y = 3;
        let prior = session
            .run_state
            .master_deck
            .iter()
            .map(|card| card.lineage)
            .collect();
        session.run_state.add_card_to_deck_with_upgrades_from(
            CardId::Parasite,
            0,
            DomainEventSource::CombatMonsterGrant {
                monster_id: EnemyId::WrithingMass,
                monster_slot: 0,
            },
        );
        let mut bindings = BTreeMap::new();
        bind_transition_master_deck_cards(
            &session,
            &mut bindings,
            &SemanticActionV1::EndTurn,
            &prior,
            false,
        )
        .unwrap();
        assert_eq!(bindings.len(), 1);
        let (lineage, bound) = bindings.iter().next().unwrap();
        assert_eq!(
            bound.binding,
            "natural_run/monster_grant/F04X03/writhingmass/0/0"
        );
        let lineage = *lineage;
        let binding = bound.binding.clone();
        session.run_state.master_deck.reverse();
        let mut reordered = BTreeMap::new();
        bind_monster_grants(&session, &mut reordered, &prior).unwrap();
        assert_eq!(reordered[&lineage].binding, binding);
        assert!(bind_monster_grants(&session, &mut reordered, &prior).is_err());
        let mut wrong = session.clone();
        for event in &mut wrong.run_state.emitted_events {
            if let sts_oracle_run_control::state::selection::DomainEvent::CardObtained {
                source: DomainEventSource::CombatMonsterGrant { monster_id, .. },
                ..
            } = event
            {
                *monster_id = EnemyId::JawWorm;
            }
        }
        assert!(bind_monster_grants(&wrong, &mut BTreeMap::new(), &prior).is_err());
        session.run_state.emitted_events.clear();
        assert!(bind_monster_grants(&session, &mut BTreeMap::new(), &prior).is_err());
    }

    #[test]
    fn event_grants_use_action_and_acquisition_witnesses_not_deck_order() {
        use sts_oracle_run_control::{
            content::events::{addict, drug_dealer, golden_shrine, nest, winding_halls},
            state::events::{EventId, EventState},
        };
        for (event_id, key, screen, option, count) in [
            (EventId::WindingHalls, "windinghalls", 1, 0, 2),
            (EventId::WindingHalls, "windinghalls", 1, 1, 1),
            (EventId::GoldenShrine, "goldenshrine", 0, 1, 1),
            (EventId::DrugDealer, "drugdealer", 0, 0, 1),
            (EventId::Addict, "addict", 0, 1, 1),
            (EventId::Nest, "nest", 1, 1, 1),
        ] {
            let mut session = RunControlSession::new(Default::default());
            session.run_state.map.current_x = 0;
            session.run_state.map.current_y = 12;
            let mut event = EventState::new(event_id);
            event.current_screen = screen;
            session.run_state.event_state = Some(event);
            session.engine_state = EngineState::EventRoom;
            let prior = session
                .run_state
                .master_deck
                .iter()
                .map(|c| c.lineage)
                .collect();
            let action = SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Event,
                choice: sts_parity_lockstep::SemanticChoiceV1 {
                    kind: "event_option".into(),
                    id: key.into(),
                    occurrence: option as u32,
                    attributes: BTreeMap::from([(
                        "screen".into(),
                        SemanticScalarV1::Integer(screen as i64),
                    )]),
                },
            };
            match event_id {
                EventId::Nest => {
                    nest::handle_choice(&mut session.engine_state, &mut session.run_state, option)
                }
                EventId::Addict => {
                    addict::handle_choice(&mut session.engine_state, &mut session.run_state, option)
                }
                EventId::DrugDealer => drug_dealer::handle_choice(
                    &mut session.engine_state,
                    &mut session.run_state,
                    option,
                ),
                EventId::GoldenShrine => golden_shrine::handle_choice(
                    &mut session.engine_state,
                    &mut session.run_state,
                    option,
                ),
                _ => winding_halls::handle_choice(
                    &mut session.engine_state,
                    &mut session.run_state,
                    option,
                ),
            }
            let mut bindings = BTreeMap::new();
            bind_transition_master_deck_cards(&session, &mut bindings, &action, &prior, false)
                .unwrap();
            assert_eq!(bindings.len(), count);
            session.run_state.master_deck.reverse();
            let mut reordered = BTreeMap::new();
            bind_transition_master_deck_cards(&session, &mut reordered, &action, &prior, false)
                .unwrap();
            for (lineage, bound) in &bindings {
                assert_eq!(bound.binding, reordered[lineage].binding);
            }
            assert!(bind_transition_master_deck_cards(
                &session,
                &mut bindings,
                &action,
                &prior,
                false
            )
            .is_err());
            let mut wrong_action = action.clone();
            if let SemanticActionV1::Select { choice, .. } = &mut wrong_action {
                choice.occurrence = 1 - option as u32;
            }
            let mut wrong_bindings = BTreeMap::new();
            let wrong_result = bind_transition_master_deck_cards(
                &session,
                &mut wrong_bindings,
                &wrong_action,
                &prior,
                false,
            );
            assert!(wrong_result.is_err() || wrong_bindings.is_empty());
            session.run_state.emitted_events.clear();
            assert!(bind_transition_master_deck_cards(
                &session,
                &mut BTreeMap::new(),
                &action,
                &prior,
                false
            )
            .is_err());
        }
    }

    #[test]
    fn golden_shrine_blocked_curse_does_not_bind_an_existing_regret() {
        use sts_oracle_run_control::{
            content::{
                cards::CardId,
                events::golden_shrine,
                relics::{RelicId, RelicState},
            },
            state::events::{EventId, EventState},
        };
        let mut session = RunControlSession::new(Default::default());
        session.run_state.map.current_x = 3;
        session.run_state.map.current_y = 4;
        session.run_state.add_card_to_deck(CardId::Regret);
        let prior = session
            .run_state
            .master_deck
            .iter()
            .map(|card| card.lineage)
            .collect();
        session.run_state.emitted_events.clear();
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::Omamori));
        session.run_state.event_state = Some(EventState::new(EventId::GoldenShrine));
        session.engine_state = EngineState::EventRoom;
        golden_shrine::handle_choice(&mut session.engine_state, &mut session.run_state, 1);
        let mut bindings = BTreeMap::new();
        bind_event_grant(
            &session,
            &mut bindings,
            &prior,
            EventId::GoldenShrine,
            CardId::Regret,
            1,
        )
        .unwrap();
        assert!(bindings.is_empty());
        assert_eq!(
            session
                .run_state
                .relics
                .iter()
                .find(|relic| relic.id == RelicId::Omamori)
                .unwrap()
                .counter,
            1
        );
    }

    #[test]
    fn ghosts_grants_bind_emitted_construction_order_even_when_deck_order_changes() {
        use sts_oracle_run_control::{
            eval::run_control::RunControlConfig,
            state::events::{EventId, EventState},
        };
        for ascension in [0, 20] {
            let mut session = RunControlSession::new(RunControlConfig {
                ascension_level: ascension,
                ..Default::default()
            });
            session.run_state.map.current_x = 3;
            session.run_state.map.current_y = 4;
            session.run_state.event_state = Some(EventState::new(EventId::Ghosts));
            session.engine_state = EngineState::EventRoom;
            let prior = session
                .run_state
                .master_deck
                .iter()
                .map(|card| card.lineage)
                .collect();
            sts_oracle_run_control::content::events::ghosts::handle_choice(
                &mut session.engine_state,
                &mut session.run_state,
                0,
            );
            let mut original = BTreeMap::new();
            bind_event_grant(
                &session,
                &mut original,
                &prior,
                EventId::Ghosts,
                sts_oracle_run_control::content::cards::CardId::Apparition,
                if ascension >= 15 { 3 } else { 5 },
            )
            .unwrap();
            assert_eq!(original.len(), if ascension >= 15 { 3 } else { 5 });
            session.run_state.master_deck.reverse();
            let mut reordered = BTreeMap::new();
            bind_event_grant(
                &session,
                &mut reordered,
                &prior,
                EventId::Ghosts,
                sts_oracle_run_control::content::cards::CardId::Apparition,
                if ascension >= 15 { 3 } else { 5 },
            )
            .unwrap();
            for (lineage, bound) in &original {
                assert_eq!(bound.binding, reordered[lineage].binding);
            }
            assert!(bind_event_grant(
                &session,
                &mut reordered,
                &prior,
                EventId::Ghosts,
                sts_oracle_run_control::content::cards::CardId::Apparition,
                if ascension >= 15 { 3 } else { 5 }
            )
            .is_err());
            session.run_state.emitted_events.clear();
            assert!(bind_event_grant(
                &session,
                &mut BTreeMap::new(),
                &prior,
                EventId::Ghosts,
                sts_oracle_run_control::content::cards::CardId::Apparition,
                if ascension >= 15 { 3 } else { 5 }
            )
            .is_err());
        }
    }

    fn bound(source: NaturalRunCardSourceV1, binding: &str) -> BoundCardSourceV1 {
        BoundCardSourceV1 {
            binding: binding.to_owned(),
            source,
            expected_card_id: "burn".to_owned(),
            expected_upgrades: 0,
        }
    }

    #[test]
    fn combat_lifecycle_drops_room_local_births_but_keeps_run_bindings() {
        let mut bindings = BTreeMap::from([
            (
                CardLineageId::master_deck(1),
                bound(
                    NaturalRunCardSourceV1::StarterDeck {
                        player_class: "ironclad".to_owned(),
                        source_slot: 0,
                    },
                    "natural_run/starter/0",
                ),
            ),
            (
                CardLineageId::combat_generated(1),
                bound(
                    NaturalRunCardSourceV1::CombatBirth {
                        room_id: "F11X03".to_owned(),
                        action_ordinal: 3,
                        cause: NaturalRunCombatCardBirthCauseV1::EndTurnResolution,
                        product_slot: 0,
                    },
                    "natural_run/combat_birth/F11X03/3/0",
                ),
            ),
        ]);
        let mut active_room = Some("F11X03".to_owned());
        let mut next_action = 4;

        align_combat_lifecycle(
            &mut bindings,
            &mut active_room,
            &mut next_action,
            Some("F16X00".to_owned()),
        );

        assert_eq!(active_room.as_deref(), Some("F16X00"));
        assert_eq!(next_action, 0);
        assert_eq!(bindings.len(), 1);
        assert!(bindings.contains_key(&CardLineageId::master_deck(1)));
        assert!(!bindings.contains_key(&CardLineageId::combat_generated(1)));
    }

    #[test]
    fn single_exhaust_hand_selection_stays_inside_the_existing_combat_lifecycle() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.map.current_x = 3;
        session.run_state.map.current_y = 10;
        session.engine_state = EngineState::PendingChoice(
            sts_oracle_run_control::state::core::PendingChoice::HandSelect {
                candidate_lineages: vec![CardLineageId::master_deck(1)],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: sts_oracle_run_control::state::core::HandSelectReason::Exhaust,
            },
        );
        let room = current_combat_room(&session).unwrap();
        let mut bindings = BTreeMap::from([(
            CardLineageId::combat_generated(1),
            bound(
                NaturalRunCardSourceV1::CombatBirth {
                    room_id: room.clone(),
                    action_ordinal: 3,
                    cause: NaturalRunCombatCardBirthCauseV1::EndTurnResolution,
                    product_slot: 0,
                },
                "natural_run/combat_birth/F11X03/3/0",
            ),
        )]);
        let mut active_room = Some(room.clone());
        let mut next_action = 4;

        align_combat_lifecycle(
            &mut bindings,
            &mut active_room,
            &mut next_action,
            Some(room),
        );

        assert_eq!(next_action, 4);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn input_bridge_replays_generated_card_after_non_generating_hand_selection() {
        use sts_oracle_run_control::{
            content::{cards::CardId, monsters::EnemyId},
            state::{
                core::{ActiveCombat, CombatContext, RoomCombatContext},
                map::RoomType,
                selection::{SelectionResolution, SelectionScope},
            },
            test_support::{blank_test_combat, test_monster},
        };
        use sts_parity_lockstep::{natural_card_reward_choice_v1, NaturalRewardCardV1};

        let config = Default::default();
        let bootstrap = crate::natural_run_bootstrap_v1(&config).unwrap();
        let mut adapter = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap,
            "fixture",
            "input-birth-scope",
        )
        .unwrap();
        adapter.session.run_state.map.current_x = 0;
        adapter.session.run_state.map.current_y = 0;
        adapter.session.run_state.event_state = None;
        let mut hand = Vec::new();
        for id in [CardId::Purity, CardId::Anger] {
            let prior = adapter
                .session
                .run_state
                .master_deck
                .iter()
                .map(|c| c.lineage)
                .collect();
            adapter
                .session
                .run_state
                .add_card_to_deck_with_upgrades_from(id, 0, DomainEventSource::RewardScreen);
            let card = adapter
                .session
                .run_state
                .master_deck
                .last()
                .unwrap()
                .clone();
            let action = SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::CardReward,
                choice: natural_card_reward_choice_v1(
                    &NaturalRewardCardV1 {
                        card_id: sts_java_parity_protocol::canonical_game_id(id.stable_id()),
                        upgrades: 0,
                    },
                    0,
                ),
            };
            adapter
                .bind_transition_cards(&action, &prior, false)
                .unwrap();
            hand.push(card);
        }
        // A focused constructed combat checks the two adapter paths. Natural
        // full-run acceptance continues to use the original-game recording.
        let mut combat = blank_test_combat();
        combat.zones.hand = hand;
        combat.meta.master_deck_snapshot = adapter.session.run_state.master_deck.clone().into();
        combat.entities.player.relics = adapter.session.run_state.relics.clone();
        combat.entities.potions = adapter.session.run_state.potions.clone();
        let mut monster = test_monster(EnemyId::Cultist);
        let target = monster.id;
        let plan = sts_oracle_run_control::content::monsters::roll_monster_turn_plan(
            &mut combat.rng.ai_rng,
            &monster,
            0,
            99,
            std::slice::from_ref(&monster),
            &[],
        );
        monster.set_planned_move_id(plan.move_id);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters = vec![monster];
        adapter.session.engine_state = EngineState::CombatPlayerTurn;
        adapter.session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let original = adapter.observe().unwrap();
        let mut oracle = adapter.session.clone();
        let mut inputs = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            ClientInput::SubmitSelection(SelectionResolution {
                scope: SelectionScope::Hand,
                selected: vec![],
            }),
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
            ClientInput::EndTurn,
        ];
        for input in &inputs {
            oracle
                .apply_decision_action(RunDecisionAction::Input(input.clone()))
                .unwrap();
        }
        let generated_index = oracle
            .active_combat
            .as_ref()
            .unwrap()
            .combat_state
            .zones
            .hand
            .iter()
            .position(|card| card.lineage.origin() == CardLineageOrigin::CombatGenerated)
            .expect("the Anger copy was drawn");
        let play_copy = ClientInput::PlayCard {
            card_index: generated_index,
            target: Some(target),
        };
        oracle
            .apply_decision_action(RunDecisionAction::Input(play_copy.clone()))
            .unwrap();
        inputs.push(play_copy);

        let semantics = adapter
            .semantic_combat_prefix_from_inputs_v1(&inputs)
            .unwrap();
        assert_eq!(adapter.observe().unwrap(), original);
        assert!(
            matches!(&semantics[4], SemanticActionV1::PlayCard { card_binding, .. }
            if card_binding.starts_with("natural_run/combat_birth/"))
        );
        for action in semantics {
            let before = adapter.observe().unwrap();
            let prepared = adapter.prepare(&action, &before).unwrap();
            adapter.execute(prepared).unwrap();
        }
        assert_eq!(
            adapter.session.active_combat.as_ref().unwrap().combat_state,
            oracle.active_combat.as_ref().unwrap().combat_state
        );
    }
}
