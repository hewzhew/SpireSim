//! In-memory exact-branch environment over one bounded complete-turn surface.
//!
//! This crate owns the live observation/commit boundary shared by the manual
//! play frontend and learning adapters. It is intentionally an exact-branch
//! research environment: observing a candidate includes its replayed public
//! successor. It is not a no-reload deployment observation.

mod combat_draft;

pub use combat_draft::{
    CombatDraftAdvanceV1, CombatDraftEnvErrorV1, CombatDraftEnvStatusV1, CombatDraftEnvV1,
};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sts_combat_outcome::combat_witness_materialization::materialize_replay_checked_combat_outcome_v1;
pub use sts_combat_outcome::combat_witness_materialization::CombatWitnessMaterializedOutcomeV1;
use sts_combat_planner::combat_plan_state_guide_policy_v1;
use sts_complete_turn_proposal::{
    exact_complete_turn_proposals_guided_v1, exact_complete_turn_proposals_with_partial_guides_v1,
    ExactCompleteTurnCompletedEndpointLayerV1, ExactCompleteTurnPrefixLayerV1,
    ExactCompleteTurnPrefixSessionV1, ExactCompleteTurnProposalBatchV1,
    ExactCompleteTurnProposalConfigV1, ExactCompleteTurnProposalGuideRankV1,
};
use sts_complete_turn_view::{
    project_complete_turn_candidate_v1, project_complete_turn_completed_endpoint_layer_v1,
    project_complete_turn_prefix_layer_v1, project_complete_turn_public_prefix_state_v1,
    CompleteTurnCompletedEndpointLayerViewV1, CompleteTurnDecisionViewV1,
    CompleteTurnPrefixLayerViewV1, CompleteTurnPublicPrefixScorerV1, CompleteTurnSelectedActionV1,
};
use sts_core::ai::combat_state_key::{combat_exact_state_key, CombatExactStateKey};
use sts_core::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use sts_core::sim::combat_case_contract::COMBAT_CASE_SCHEMA;
use sts_core::state::core::{ClientInput, EngineState};

pub const COMPLETE_TURN_ENV_MAX_ENGINE_STEPS: usize = 250;
pub const COMPLETE_TURN_ENV_MAX_INNER_NODES: usize = 256;
pub const COMPLETE_TURN_ENV_MAX_PARTIAL_FRONTIER_STATES: usize = 32;
pub const COMPLETE_TURN_ENV_MAX_END_STATES: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteTurnExactSurfaceDiagnosticsV1 {
    pub max_inner_nodes: usize,
    pub max_partial_frontier_states: usize,
    pub max_end_states: usize,
    pub per_bucket_limit: usize,
    pub candidate_count: usize,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub engine_steps: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
    pub preselection_candidate_count: usize,
    pub resource_profile_retained_candidate_count: usize,
    pub support_retained_candidate_count: usize,
    pub completed_endpoint_reserved_candidate_count: usize,
    pub partial_prefix_scorer_active: bool,
    pub interactive_prefix_selection_active: bool,
    pub completed_endpoint_reservation_active: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct CombatCaseRootV2 {
    schema: String,
    position: CombatPosition,
}

#[derive(Clone)]
struct PendingCompleteTurnDecisionV1 {
    surface: ExactCompleteTurnProposalBatchV1,
    view: CompleteTurnDecisionViewV1,
    partial_prefix_scorer_active: bool,
    interactive_prefix_selection_active: bool,
    completed_endpoint_reservation_active: bool,
}

#[derive(Clone)]
struct PendingCompleteTurnPrefixGenerationV1 {
    session: Option<ExactCompleteTurnPrefixSessionV1>,
    observed_layer: Option<(
        ExactCompleteTurnPrefixLayerV1,
        CompleteTurnPrefixLayerViewV1,
    )>,
    completed_endpoints: Option<(
        ExactCompleteTurnCompletedEndpointLayerV1,
        CompleteTurnCompletedEndpointLayerViewV1,
    )>,
    exhausted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteTurnExactStepV1 {
    pub chosen_ordinal: usize,
    pub boundary: sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1,
    pub terminal: CombatTerminal,
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub turn_count: u32,
}

/// One replay-verified action group from a real player-turn boundary to the
/// next player-turn boundary or the terminal win.
#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayVerifiedCompleteTurnV1 {
    pub from_turn: u32,
    pub to_turn: u32,
    pub boundary: sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1,
    pub inputs: Vec<ClientInput>,
}

/// A whole exact action line that Rust replayed from a production combat case
/// and proved ends in a combat win.
#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayVerifiedWinningCompleteTurnLineV1 {
    pub action_count: usize,
    pub turns: Vec<ReplayVerifiedCompleteTurnV1>,
}

#[derive(Clone)]
pub struct CompleteTurnExactEnvV1 {
    root: CombatPosition,
    position: CombatPosition,
    chosen_inputs: Vec<sts_core::state::core::ClientInput>,
    proposal_config: ExactCompleteTurnProposalConfigV1,
    partial_prefix_scorer: Option<Arc<dyn CompleteTurnPublicPrefixScorerV1>>,
    prefix_generation: Option<PendingCompleteTurnPrefixGenerationV1>,
    pending: Option<PendingCompleteTurnDecisionV1>,
}

impl CompleteTurnExactEnvV1 {
    pub fn from_case_json(payload: &[u8], allow_potions: bool) -> Result<Self, String> {
        Self::from_case_json_with_limits(
            payload,
            allow_potions,
            COMPLETE_TURN_ENV_MAX_INNER_NODES,
            COMPLETE_TURN_ENV_MAX_PARTIAL_FRONTIER_STATES,
            COMPLETE_TURN_ENV_MAX_END_STATES,
            COMPLETE_TURN_ENV_MAX_END_STATES,
        )
    }

    pub fn from_case_json_with_limits(
        payload: &[u8],
        allow_potions: bool,
        max_inner_nodes: usize,
        max_partial_frontier_states: usize,
        max_end_states: usize,
        per_bucket_limit: usize,
    ) -> Result<Self, String> {
        let position = decode_combat_case_root(payload)?;
        let proposal_config = complete_turn_proposal_config_v1(
            allow_potions,
            max_inner_nodes,
            max_partial_frontier_states,
            max_end_states,
            per_bucket_limit,
        )?;
        Ok(Self::from_position_with_config(position, proposal_config))
    }

    pub fn from_position(position: CombatPosition, allow_potions: bool) -> Self {
        let proposal_config = complete_turn_proposal_config_v1(
            allow_potions,
            COMPLETE_TURN_ENV_MAX_INNER_NODES,
            COMPLETE_TURN_ENV_MAX_PARTIAL_FRONTIER_STATES,
            COMPLETE_TURN_ENV_MAX_END_STATES,
            COMPLETE_TURN_ENV_MAX_END_STATES,
        )
        .expect("default complete-turn proposal limits are valid");
        Self::from_position_with_config(position, proposal_config)
    }

    pub fn from_position_with_config(
        position: CombatPosition,
        proposal_config: ExactCompleteTurnProposalConfigV1,
    ) -> Self {
        Self {
            root: position.clone(),
            position,
            chosen_inputs: Vec::new(),
            proposal_config,
            partial_prefix_scorer: None,
            prefix_generation: None,
            pending: None,
        }
    }

    pub fn with_partial_prefix_scorer(
        mut self,
        scorer: Arc<dyn CompleteTurnPublicPrefixScorerV1>,
    ) -> Result<Self, String> {
        if self.pending.is_some() {
            return Err(
                "cannot replace a partial-prefix scorer after observing a surface".to_owned(),
            );
        }
        let root = project_complete_turn_public_prefix_state_v1(&self.position)?;
        if !scorer.supports_root(&root) {
            return Err("partial-prefix scorer does not support this public root".to_owned());
        }
        self.partial_prefix_scorer = Some(scorer);
        Ok(self)
    }

    pub fn observe(&mut self) -> Result<&CompleteTurnDecisionViewV1, String> {
        if combat_terminal(&self.position.engine, &self.position.combat)
            != CombatTerminal::Unresolved
        {
            return Err("complete-turn environment is already terminal".to_owned());
        }
        if self.prefix_generation.is_some() {
            return Err(
                "finish or discard the interactive prefix generation before ordinary observe"
                    .to_owned(),
            );
        }
        if self.pending.is_none() {
            let public_root = project_complete_turn_public_prefix_state_v1(&self.position)?;
            let active_scorer = self
                .partial_prefix_scorer
                .as_deref()
                .filter(|scorer| scorer.supports_root(&public_root));
            let partial_prefix_scorer_active = active_scorer.is_some();
            let surface = complete_turn_surface_with_config_and_prefix_scorer_v1(
                &self.position,
                self.proposal_config,
                active_scorer,
            );
            if surface.candidates.is_empty() {
                return Err("complete-turn surface has no candidates".to_owned());
            }
            let view = project_complete_turn_surface_v1(&self.position, &surface)?;
            self.pending = Some(PendingCompleteTurnDecisionV1 {
                surface,
                view,
                partial_prefix_scorer_active,
                interactive_prefix_selection_active: false,
                completed_endpoint_reservation_active: false,
            });
        }
        Ok(&self
            .pending
            .as_ref()
            .expect("pending decision was initialized")
            .view)
    }

    pub fn surface_diagnostics(&mut self) -> Result<CompleteTurnExactSurfaceDiagnosticsV1, String> {
        self.observe()?;
        let surface = &self
            .pending
            .as_ref()
            .expect("observe initialized the complete-turn surface")
            .surface;
        Ok(CompleteTurnExactSurfaceDiagnosticsV1 {
            max_inner_nodes: self.proposal_config.max_inner_nodes,
            max_partial_frontier_states: self.proposal_config.max_partial_frontier_states,
            max_end_states: self.proposal_config.max_end_states,
            per_bucket_limit: self.proposal_config.per_bucket_limit,
            candidate_count: surface.candidates.len(),
            nodes_expanded: surface.nodes_expanded,
            nodes_generated: surface.nodes_generated,
            engine_steps: surface.engine_steps,
            exact_state_skips: surface.exact_state_skips,
            truncated_children: surface.truncated_children,
            preselection_candidate_count: surface.preselection_candidate_count,
            resource_profile_retained_candidate_count: surface
                .resource_profile_retained_candidate_count,
            support_retained_candidate_count: surface.support_retained_candidate_count,
            completed_endpoint_reserved_candidate_count: surface
                .completed_endpoint_reserved_candidate_count,
            partial_prefix_scorer_active: self
                .pending
                .as_ref()
                .expect("observe initialized the complete-turn surface")
                .partial_prefix_scorer_active,
            interactive_prefix_selection_active: self
                .pending
                .as_ref()
                .expect("observe initialized the complete-turn surface")
                .interactive_prefix_selection_active,
            completed_endpoint_reservation_active: self
                .pending
                .as_ref()
                .expect("observe initialized the complete-turn surface")
                .completed_endpoint_reservation_active,
        })
    }

    /// Observes every public candidate generated at one exact action depth.
    /// Repeated calls return the same layer until `retain_prefix_layer` is
    /// called, so a model may score the entire ragged layer in one batch.
    pub fn observe_prefix_layer(
        &mut self,
    ) -> Result<Option<&CompleteTurnPrefixLayerViewV1>, String> {
        if combat_terminal(&self.position.engine, &self.position.combat)
            != CombatTerminal::Unresolved
        {
            return Err("complete-turn environment is already terminal".to_owned());
        }
        if self.pending.is_some() {
            return Err(
                "complete-turn surface is already materialized; choose it before generating another"
                    .to_owned(),
            );
        }
        if self.partial_prefix_scorer.is_some() {
            return Err(
                "interactive prefix selection cannot be combined with a callback prefix scorer"
                    .to_owned(),
            );
        }
        if self.prefix_generation.is_none() {
            self.prefix_generation = Some(PendingCompleteTurnPrefixGenerationV1 {
                session: Some(ExactCompleteTurnPrefixSessionV1::new(
                    &self.position,
                    self.proposal_config,
                )),
                observed_layer: None,
                completed_endpoints: None,
                exhausted: false,
            });
        }
        let generation = self
            .prefix_generation
            .as_mut()
            .expect("prefix generation was initialized");
        if generation.observed_layer.is_none() && !generation.exhausted {
            match generation
                .session
                .as_mut()
                .expect("unfinished prefix generation retains its session")
                .observe_layer()
            {
                Some(layer) => {
                    let view = project_complete_turn_prefix_layer_v1(
                        &self.position,
                        &layer,
                        self.proposal_config.max_engine_steps_per_action,
                    )?;
                    generation.observed_layer = Some((layer, view));
                }
                None => generation.exhausted = true,
            }
        }
        Ok(generation.observed_layer.as_ref().map(|(_, view)| view))
    }

    pub fn retain_prefix_layer(&mut self, ordinals: &[usize]) -> Result<(), String> {
        let generation = self.prefix_generation.as_mut().ok_or_else(|| {
            "observe an interactive complete-turn prefix layer before retaining it".to_owned()
        })?;
        if generation.observed_layer.is_none() {
            return Err("interactive complete-turn prefix layer is not pending".to_owned());
        }
        generation
            .session
            .as_mut()
            .expect("unfinished prefix generation retains its session")
            .retain_layer(ordinals)?;
        generation.observed_layer = None;
        Ok(())
    }

    /// Reserves model-selected prefixes and fills only unused capacity with
    /// the existing proposal-diversity selector. This is proposal coverage,
    /// not an execution-policy fallback.
    pub fn retain_prefix_layer_with_default_fill(
        &mut self,
        ordinals: &[usize],
    ) -> Result<(), String> {
        let generation = self.prefix_generation.as_mut().ok_or_else(|| {
            "observe an interactive complete-turn prefix layer before retaining it".to_owned()
        })?;
        if generation.observed_layer.is_none() {
            return Err("interactive complete-turn prefix layer is not pending".to_owned());
        }
        generation
            .session
            .as_mut()
            .expect("unfinished prefix generation retains its session")
            .retain_layer_with_default_fill(ordinals)?;
        generation.observed_layer = None;
        Ok(())
    }

    /// Finds a demonstrated prefix using public selected-action semantics
    /// only. This aligns replay-verified teacher evidence with a generated
    /// layer without exposing exact inputs, identities, or RNG.
    pub fn matching_prefix_public_actions_ordinal(
        &mut self,
        actions: &[CompleteTurnSelectedActionV1],
    ) -> Result<Option<usize>, String> {
        let Some(layer) = self.observe_prefix_layer()? else {
            return Ok(None);
        };
        unique_public_action_match(
            layer
                .candidates
                .iter()
                .enumerate()
                .filter_map(|(ordinal, candidate)| {
                    (candidate.actions == actions).then_some(ordinal)
                }),
            "complete-turn prefix layer",
        )
    }

    /// Replays one known exact input prefix from the current turn root and
    /// matches its exact successor inside the pending generated layer. This
    /// teacher-alignment diagnostic tolerates alternative public action lines
    /// that reached the same simulator state through exact deduplication.
    pub fn matching_prefix_successor_ordinal(
        &mut self,
        inputs: &[ClientInput],
    ) -> Result<Option<usize>, String> {
        let expected = replay_turn_inputs(&self.position, inputs)?;
        let expected_key = combat_exact_state_key(&expected.engine, &expected.combat);
        if self.observe_prefix_layer()?.is_none() {
            return Ok(None);
        }
        let layer = &self
            .prefix_generation
            .as_ref()
            .expect("prefix generation exists after observing a layer")
            .observed_layer
            .as_ref()
            .expect("prefix layer exists after observing a layer")
            .0;
        let matches = layer
            .try_project_candidates(|_, candidate| {
                Ok(combat_exact_state_key(&candidate.engine, &candidate.combat) == expected_key)
            })?
            .into_iter()
            .enumerate()
            .filter_map(|(ordinal, matches)| matches.then_some(ordinal));
        unique_exact_match(matches, "complete-turn prefix layer")
    }

    /// Materializes every generated complete-turn endpoint before the public
    /// width selector runs. The returned ordinals reserve proposal capacity;
    /// they are not executable choice ordinals.
    pub fn observe_completed_endpoints(
        &mut self,
    ) -> Result<&CompleteTurnCompletedEndpointLayerViewV1, String> {
        if self.pending.is_some() {
            return Err("complete-turn surface is already materialized".to_owned());
        }
        let generation = self.prefix_generation.as_mut().ok_or_else(|| {
            "observe interactive complete-turn prefix layers before completed endpoints".to_owned()
        })?;
        if generation.observed_layer.is_some() {
            return Err(
                "retain the pending complete-turn prefix layer before observing completed endpoints"
                    .to_owned(),
            );
        }
        if !generation.exhausted {
            return Err(
                "observe complete-turn prefix layers until enumeration is exhausted before completed endpoints"
                    .to_owned(),
            );
        }
        if generation.completed_endpoints.is_none() {
            let session = generation
                .session
                .take()
                .expect("exhausted prefix generation retains its unfinished endpoint session");
            let layer = session.finish_completed_endpoints(&complete_turn_completed_guide_ranks)?;
            let view = project_complete_turn_completed_endpoint_layer_v1(
                &self.position,
                &layer,
                self.proposal_config.max_engine_steps_per_action,
            )?;
            generation.completed_endpoints = Some((layer, view));
        }
        Ok(&generation
            .completed_endpoints
            .as_ref()
            .expect("completed endpoint layer was initialized")
            .1)
    }

    /// Finds one generated completed endpoint by public selected-action
    /// semantics only. This is teacher alignment for reservation, not an
    /// execution choice.
    pub fn matching_completed_endpoint_public_actions_ordinal(
        &mut self,
        actions: &[CompleteTurnSelectedActionV1],
    ) -> Result<Option<usize>, String> {
        let layer = self.observe_completed_endpoints()?;
        unique_public_action_match(
            layer
                .candidates
                .iter()
                .enumerate()
                .filter_map(|(ordinal, candidate)| {
                    (candidate.actions == actions).then_some(ordinal)
                }),
            "complete-turn completed endpoint layer",
        )
    }

    /// Replays one known exact complete-turn line and matches its exact
    /// successor before endpoint-width selection. This is the completed-state
    /// counterpart to `matching_prefix_successor_ordinal`.
    pub fn matching_completed_endpoint_successor_ordinal(
        &mut self,
        inputs: &[ClientInput],
    ) -> Result<Option<usize>, String> {
        let expected = replay_turn_inputs(&self.position, inputs)?;
        let expected_key = combat_exact_state_key(&expected.engine, &expected.combat);
        self.observe_completed_endpoints()?;
        let layer = &self
            .prefix_generation
            .as_ref()
            .expect("completed endpoint generation exists after observation")
            .completed_endpoints
            .as_ref()
            .expect("completed endpoint layer exists after observation")
            .0;
        let matches =
            layer
                .try_project_candidates(|candidate, _| {
                    Ok(combat_exact_state_key(
                        &candidate.successor.engine,
                        &candidate.successor.combat,
                    ) == expected_key)
                })?
                .into_iter()
                .enumerate()
                .filter_map(|(ordinal, matches)| matches.then_some(ordinal));
        unique_exact_match(matches, "complete-turn completed endpoint layer")
    }

    /// Reserves generated completed endpoints first, then lets the maintained
    /// endpoint selector fill only the unused executable-surface capacity.
    pub fn retain_completed_endpoints_with_default_fill(
        &mut self,
        ordinals: &[usize],
    ) -> Result<&CompleteTurnDecisionViewV1, String> {
        if self.pending.is_some() {
            return Err("complete-turn surface is already materialized".to_owned());
        }
        self.observe_completed_endpoints()?;
        let surface = self
            .prefix_generation
            .as_ref()
            .expect("completed endpoint generation existed after observation")
            .completed_endpoints
            .as_ref()
            .expect("completed endpoint layer existed after observation")
            .0
            .retain_with_default_fill(ordinals)?;
        if surface.candidates.is_empty() {
            return Err("complete-turn surface has no candidates".to_owned());
        }
        let view = project_complete_turn_surface_v1(&self.position, &surface)?;
        self.prefix_generation = None;
        self.pending = Some(PendingCompleteTurnDecisionV1 {
            surface,
            view,
            partial_prefix_scorer_active: false,
            interactive_prefix_selection_active: true,
            completed_endpoint_reservation_active: !ordinals.is_empty(),
        });
        Ok(&self
            .pending
            .as_ref()
            .expect("completed endpoint surface was initialized")
            .view)
    }

    /// Finalizes the externally retained prefix frontier into the same exact
    /// complete-turn surface consumed by ordinary `choose`.
    pub fn finish_prefix_surface(&mut self) -> Result<&CompleteTurnDecisionViewV1, String> {
        self.retain_completed_endpoints_with_default_fill(&[])
    }

    pub fn matching_complete_turn_public_actions_ordinal(
        &mut self,
        actions: &[CompleteTurnSelectedActionV1],
    ) -> Result<Option<usize>, String> {
        let decision = self.observe()?;
        unique_public_action_match(
            decision
                .candidates
                .iter()
                .enumerate()
                .filter_map(|(ordinal, candidate)| {
                    (candidate.actions == actions).then_some(ordinal)
                }),
            "complete-turn surface",
        )
    }

    pub fn matching_candidate_ordinal(
        &mut self,
        inputs: &[ClientInput],
    ) -> Result<Option<usize>, String> {
        self.observe()?;
        let surface = &self
            .pending
            .as_ref()
            .expect("observe initialized the complete-turn surface")
            .surface;
        let matches = surface
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| {
                candidate
                    .actions
                    .iter()
                    .map(|action| &action.input)
                    .eq(inputs.iter())
                    .then_some(ordinal)
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Ok(None),
            [ordinal] => Ok(Some(*ordinal)),
            _ => Err("complete-turn surface contains duplicate exact input lines".to_owned()),
        }
    }

    pub fn matching_successor_ordinal(
        &mut self,
        inputs: &[ClientInput],
    ) -> Result<Option<usize>, String> {
        self.observe()?;
        let expected = replay_turn_inputs(&self.position, inputs)?;
        let expected_key = combat_exact_state_key(&expected.engine, &expected.combat);
        let surface = &self
            .pending
            .as_ref()
            .expect("observe initialized the complete-turn surface")
            .surface;
        matching_successor_ordinal(surface, &expected_key)
    }

    pub fn choose(&mut self, ordinal: usize) -> Result<CompleteTurnExactStepV1, String> {
        let candidate_count = self
            .pending
            .as_ref()
            .ok_or_else(|| "observe the complete-turn decision before choosing".to_owned())?
            .surface
            .candidates
            .len();
        if ordinal >= candidate_count {
            return Err(format!(
                "complete-turn ordinal {ordinal} is outside {candidate_count} candidates"
            ));
        }
        let pending = self
            .pending
            .take()
            .expect("pending decision existed after validation");
        let candidate = &pending.surface.candidates[ordinal];
        let boundary = candidate.boundary;
        self.chosen_inputs
            .extend(candidate.actions.iter().map(|action| action.input.clone()));
        self.position = candidate.successor.clone();
        let terminal = combat_terminal(&self.position.engine, &self.position.combat);
        Ok(CompleteTurnExactStepV1 {
            chosen_ordinal: ordinal,
            boundary,
            terminal,
            player_hp: self.position.combat.entities.player.current_hp,
            player_max_hp: self.position.combat.entities.player.max_hp,
            turn_count: self.position.combat.turn.turn_count,
        })
    }

    pub fn terminal(&self) -> CombatTerminal {
        combat_terminal(&self.position.engine, &self.position.combat)
    }

    pub fn player_hp(&self) -> i32 {
        self.position.combat.entities.player.current_hp
    }

    pub fn player_max_hp(&self) -> i32 {
        self.position.combat.entities.player.max_hp
    }

    pub fn turn_count(&self) -> u32 {
        self.position.combat.turn.turn_count
    }

    pub fn materialized_outcome(&self) -> Result<CombatWitnessMaterializedOutcomeV1, String> {
        materialize_replay_checked_combat_outcome_v1(
            &self.root,
            &self.chosen_inputs,
            &self.position,
        )
    }
}

/// Decode and exact-replay one captured combat action line, grouping inputs
/// only at simulator-owned player-turn boundaries. This deliberately does not
/// require the bounded proposal surface to recall the line: it establishes the
/// teacher turns that a later carriage audit must try to retain.
pub fn replay_verified_winning_complete_turn_line_v1(
    case_payload: &[u8],
    actions_payload: &[u8],
) -> Result<ReplayVerifiedWinningCompleteTurnLineV1, String> {
    let root = decode_combat_case_root(case_payload)?;
    if combat_terminal(&root.engine, &root.combat) != CombatTerminal::Unresolved
        || !matches!(root.engine, EngineState::CombatPlayerTurn)
    {
        return Err(format!(
            "combat case root is not an unresolved player-turn boundary: engine={:?}, terminal={:?}",
            root.engine,
            combat_terminal(&root.engine, &root.combat)
        ));
    }
    let inputs: Vec<ClientInput> = serde_json::from_slice(actions_payload)
        .map_err(|error| format!("failed to decode combat action line: {error}"))?;
    if inputs.is_empty() {
        return Err("winning combat action line must not be empty".to_owned());
    }

    let mut turn_start = root;
    let mut position = turn_start.clone();
    let mut turn_inputs = Vec::new();
    let mut turns = Vec::new();
    for (action_index, input) in inputs.iter().enumerate() {
        let before_terminal = combat_terminal(&position.engine, &position.combat);
        if before_terminal != CombatTerminal::Unresolved {
            return Err(format!(
                "combat action line has input {action_index} after terminal {before_terminal:?}"
            ));
        }
        position = apply_replay_input(&position, input.clone()).map_err(|error| {
            format!("combat action {action_index} failed exact replay: {error}")
        })?;
        turn_inputs.push(input.clone());

        let terminal = combat_terminal(&position.engine, &position.combat);
        let boundary = if terminal == CombatTerminal::Win {
            Some(sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::TerminalWin)
        } else if terminal == CombatTerminal::Loss {
            Some(sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::TerminalLoss)
        } else if matches!(position.engine, EngineState::CombatPlayerTurn)
            && position.combat.turn.turn_count > turn_start.combat.turn.turn_count
        {
            Some(sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn)
        } else {
            None
        };
        let Some(boundary) = boundary else {
            continue;
        };
        if terminal != CombatTerminal::Unresolved && action_index + 1 != inputs.len() {
            return Err(format!(
                "combat action line reaches {terminal:?} before its final input"
            ));
        }
        turns.push(ReplayVerifiedCompleteTurnV1 {
            from_turn: turn_start.combat.turn.turn_count,
            to_turn: position.combat.turn.turn_count,
            boundary,
            inputs: std::mem::take(&mut turn_inputs),
        });
        turn_start = position.clone();
    }

    let terminal = combat_terminal(&position.engine, &position.combat);
    if terminal != CombatTerminal::Win {
        return Err(format!(
            "combat action line replayed to {terminal:?}, expected Win"
        ));
    }
    if !turn_inputs.is_empty() {
        return Err("winning combat action line ended inside an incomplete turn".to_owned());
    }
    Ok(ReplayVerifiedWinningCompleteTurnLineV1 {
        action_count: inputs.len(),
        turns,
    })
}

fn decode_combat_case_root(payload: &[u8]) -> Result<CombatPosition, String> {
    let case: CombatCaseRootV2 = serde_json::from_slice(payload)
        .map_err(|error| format!("failed to decode combat case: {error}"))?;
    if case.schema != COMBAT_CASE_SCHEMA {
        return Err(format!(
            "expected {COMBAT_CASE_SCHEMA}, got '{}'",
            case.schema
        ));
    }
    Ok(case.position)
}

fn unique_public_action_match(
    matches: impl Iterator<Item = usize>,
    context: &str,
) -> Result<Option<usize>, String> {
    let matches = matches.collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [ordinal] => Ok(Some(*ordinal)),
        _ => Err(format!(
            "{context} contains duplicate public selected-action lines"
        )),
    }
}

fn unique_exact_match(
    matches: impl Iterator<Item = usize>,
    context: &str,
) -> Result<Option<usize>, String> {
    let matches = matches.collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [ordinal] => Ok(Some(*ordinal)),
        _ => Err(format!("{context} contains duplicate exact successors")),
    }
}

fn replay_turn_inputs(
    root: &CombatPosition,
    inputs: &[ClientInput],
) -> Result<CombatPosition, String> {
    let mut expected = root.clone();
    for input in inputs {
        expected = apply_replay_input(&expected, input.clone())?;
    }
    Ok(expected)
}

fn apply_replay_input(
    position: &CombatPosition,
    input: ClientInput,
) -> Result<CombatPosition, String> {
    if !EngineCombatStepper.is_legal_action(position, &input)
        && EngineCombatStepper
            .choice_for_legal_input(position, &input)
            .is_none()
    {
        return Err(format!(
            "known combat input is not legal at its replay boundary: {input:?}"
        ));
    }
    let step = EngineCombatStepper.apply_to_stable(
        position,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: COMPLETE_TURN_ENV_MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err(format!(
            "known combat input exceeded the finite engine-step limit: {input:?}"
        ));
    }
    Ok(step.position)
}

fn matching_successor_ordinal(
    surface: &ExactCompleteTurnProposalBatchV1,
    expected_key: &CombatExactStateKey,
) -> Result<Option<usize>, String> {
    let matches = surface
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| {
            (&combat_exact_state_key(&candidate.successor.engine, &candidate.successor.combat)
                == expected_key)
                .then_some(ordinal)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [ordinal] => Ok(Some(*ordinal)),
        _ => Err("complete-turn surface contains duplicate exact successors".to_owned()),
    }
}

pub fn complete_turn_surface_v1(
    position: &CombatPosition,
    allow_potions: bool,
) -> ExactCompleteTurnProposalBatchV1 {
    let config = complete_turn_proposal_config_v1(
        allow_potions,
        COMPLETE_TURN_ENV_MAX_INNER_NODES,
        COMPLETE_TURN_ENV_MAX_PARTIAL_FRONTIER_STATES,
        COMPLETE_TURN_ENV_MAX_END_STATES,
        COMPLETE_TURN_ENV_MAX_END_STATES,
    )
    .expect("default complete-turn proposal limits are valid");
    complete_turn_surface_with_config_v1(position, config)
}

pub fn complete_turn_surface_with_config_v1(
    position: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
) -> ExactCompleteTurnProposalBatchV1 {
    exact_complete_turn_proposals_guided_v1(position, config, &complete_turn_completed_guide_ranks)
}

fn complete_turn_completed_guide_ranks(
    successor: &CombatPosition,
) -> Vec<ExactCompleteTurnProposalGuideRankV1> {
    combat_plan_state_guide_policy_v1(sts_combat_atomic::retained_combat_tactical_policy_v1())
        .state_guides(successor)
        .into_iter()
        .map(|guide| ExactCompleteTurnProposalGuideRankV1 {
            lane: guide.lane.value(),
            components: guide.rank.components().to_vec(),
        })
        .collect()
}

pub fn complete_turn_surface_with_config_and_prefix_scorer_v1(
    position: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
    partial_prefix_scorer: Option<&dyn CompleteTurnPublicPrefixScorerV1>,
) -> ExactCompleteTurnProposalBatchV1 {
    let public_root = project_complete_turn_public_prefix_state_v1(position).ok();
    let partial_prefix_scorer = partial_prefix_scorer.filter(|scorer| {
        public_root
            .as_ref()
            .is_some_and(|root| scorer.supports_root(root))
    });
    exact_complete_turn_proposals_with_partial_guides_v1(
        position,
        config,
        &|partial| prefix_guide_ranks(partial_prefix_scorer, partial),
        &|successor| {
            let mut ranks = complete_turn_completed_guide_ranks(successor);
            ranks.extend(prefix_guide_ranks(partial_prefix_scorer, successor));
            ranks
        },
    )
}

fn prefix_guide_ranks(
    scorer: Option<&dyn CompleteTurnPublicPrefixScorerV1>,
    position: &CombatPosition,
) -> Vec<ExactCompleteTurnProposalGuideRankV1> {
    let Some(scorer) = scorer else {
        return Vec::new();
    };
    let Ok(public) = project_complete_turn_public_prefix_state_v1(position) else {
        return Vec::new();
    };
    scorer
        .guide_ranks(&public)
        .into_iter()
        .map(|rank| ExactCompleteTurnProposalGuideRankV1 {
            lane: rank.lane,
            components: rank.components,
        })
        .collect()
}

pub fn complete_turn_proposal_config_v1(
    allow_potions: bool,
    max_inner_nodes: usize,
    max_partial_frontier_states: usize,
    max_end_states: usize,
    per_bucket_limit: usize,
) -> Result<ExactCompleteTurnProposalConfigV1, String> {
    for (name, value) in [
        ("max_inner_nodes", max_inner_nodes),
        ("max_partial_frontier_states", max_partial_frontier_states),
        ("max_end_states", max_end_states),
        ("per_bucket_limit", per_bucket_limit),
    ] {
        if value == 0 {
            return Err(format!("complete-turn {name} must be positive"));
        }
    }
    Ok(ExactCompleteTurnProposalConfigV1 {
        max_inner_nodes,
        max_partial_frontier_states,
        max_end_states,
        per_bucket_limit,
        max_engine_steps_per_action: COMPLETE_TURN_ENV_MAX_ENGINE_STEPS,
        allow_potion_expenditure: allow_potions,
        max_potion_expenditures: if allow_potions { None } else { Some(0) },
        allowed_potion_slots: None,
        allow_potion_discard: false,
    })
}

pub fn project_complete_turn_surface_v1(
    position: &CombatPosition,
    surface: &ExactCompleteTurnProposalBatchV1,
) -> Result<CompleteTurnDecisionViewV1, String> {
    let policy =
        combat_plan_state_guide_policy_v1(sts_combat_atomic::retained_combat_tactical_policy_v1());
    let candidates = surface
        .candidates
        .iter()
        .map(|candidate| {
            let guides = policy
                .state_guides(&candidate.successor)
                .into_iter()
                .map(|guide| ExactCompleteTurnProposalGuideRankV1 {
                    lane: guide.lane.value(),
                    components: guide.rank.components().to_vec(),
                })
                .collect::<Vec<_>>();
            project_complete_turn_candidate_v1(
                position,
                candidate,
                &guides,
                COMPLETE_TURN_ENV_MAX_ENGINE_STEPS,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(CompleteTurnDecisionViewV1::new(position, candidates))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{cards::CardId, monsters::EnemyId};
    use sts_core::runtime::combat::CombatCard;
    use sts_core::state::core::EngineState;
    use sts_core::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn proposal_limits_are_explicit_and_nonzero() {
        let config = complete_turn_proposal_config_v1(false, 1024, 128, 96, 48).unwrap();

        assert_eq!(config.max_inner_nodes, 1024);
        assert_eq!(config.max_partial_frontier_states, 128);
        assert_eq!(config.max_end_states, 96);
        assert_eq!(config.per_bucket_limit, 48);
        assert!(!config.allow_potion_expenditure);
        assert_eq!(config.max_potion_expenditures, Some(0));
        assert!(complete_turn_proposal_config_v1(true, 0, 128, 96, 48).is_err());
        assert!(complete_turn_proposal_config_v1(true, 1024, 0, 96, 48).is_err());
        assert!(complete_turn_proposal_config_v1(true, 1024, 128, 0, 48).is_err());
        assert!(complete_turn_proposal_config_v1(true, 1024, 128, 96, 0).is_err());
    }

    #[test]
    fn exact_winning_action_line_is_split_only_at_real_player_turn_boundaries() {
        let mut combat = blank_test_combat();
        let mut monster = planned_monster(EnemyId::JawWorm, 1);
        monster.current_hp = 6;
        monster.max_hp = 6;
        let target = monster.id;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 10)];
        combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 11)].into();
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let case_payload = serde_json::to_vec(&serde_json::json!({
            "schema": COMBAT_CASE_SCHEMA,
            "position": root,
        }))
        .unwrap();
        let inputs = vec![
            ClientInput::EndTurn,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
        ];
        let actions_payload = serde_json::to_vec(&inputs).unwrap();

        let line =
            replay_verified_winning_complete_turn_line_v1(&case_payload, &actions_payload).unwrap();

        assert_eq!(line.action_count, 2);
        assert_eq!(line.turns.len(), 2);
        assert_eq!(line.turns[0].from_turn, 1);
        assert_eq!(line.turns[0].to_turn, 2);
        assert_eq!(
            line.turns[0].boundary,
            sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn
        );
        assert_eq!(line.turns[0].inputs, [ClientInput::EndTurn]);
        assert_eq!(line.turns[1].from_turn, 2);
        assert_eq!(
            line.turns[1].boundary,
            sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1::TerminalWin
        );
        assert_eq!(line.turns[1].inputs, inputs[1..]);
    }

    #[test]
    fn exact_winning_action_line_rejects_an_unfinished_replay() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let case_payload = serde_json::to_vec(&serde_json::json!({
            "schema": COMBAT_CASE_SCHEMA,
            "position": root,
        }))
        .unwrap();
        let actions_payload = serde_json::to_vec(&vec![ClientInput::EndTurn]).unwrap();

        let error = replay_verified_winning_complete_turn_line_v1(&case_payload, &actions_payload)
            .unwrap_err();

        assert!(error.contains("expected Win"));
    }

    #[test]
    fn known_legal_turn_line_matches_its_replayed_successor() {
        let mut combat = blank_test_combat();
        let monster = planned_monster(EnemyId::JawWorm, 1);
        let target = monster.id;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
        let mut env = CompleteTurnExactEnvV1::from_position_with_config(
            CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            complete_turn_proposal_config_v1(false, 64, 64, 64, 64).unwrap(),
        );
        let inputs = [
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
            ClientInput::EndTurn,
        ];

        assert!(env.matching_successor_ordinal(&inputs).unwrap().is_some());
    }

    #[test]
    fn public_positive_prefix_scorer_is_active_on_its_supported_root() {
        let mut combat = blank_test_combat();
        let monster = planned_monster(EnemyId::JawWorm, 1);
        let target = monster.id;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let inputs = [
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
            ClientInput::EndTurn,
        ];
        let evidence =
            sts_complete_turn_view::CompleteTurnPositivePrefixEvidenceV1::from_replayed_complete_turn(
                &position,
                &inputs,
                COMPLETE_TURN_ENV_MAX_ENGINE_STEPS,
            )
            .unwrap();
        let mut env = CompleteTurnExactEnvV1::from_position_with_config(
            position,
            complete_turn_proposal_config_v1(false, 64, 1, 8, 8).unwrap(),
        )
        .with_partial_prefix_scorer(Arc::new(evidence))
        .unwrap();

        assert!(env.matching_successor_ordinal(&inputs).unwrap().is_some());
        assert!(
            env.surface_diagnostics()
                .unwrap()
                .partial_prefix_scorer_active
        );
    }

    #[test]
    fn exact_prefix_successor_alignment_stays_inside_the_pending_layer() {
        let mut combat = blank_test_combat();
        let monster = planned_monster(EnemyId::JawWorm, 1);
        let target = monster.id;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let mut env = CompleteTurnExactEnvV1::from_position_with_config(
            CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            complete_turn_proposal_config_v1(false, 64, 8, 8, 8).unwrap(),
        );
        let prefix = [ClientInput::PlayCard {
            card_index: 0,
            target: Some(target),
        }];

        let ordinal = env
            .matching_prefix_successor_ordinal(&prefix)
            .unwrap()
            .expect("the exact one-action successor is generated");
        assert!(
            ordinal
                < env
                    .observe_prefix_layer()
                    .unwrap()
                    .unwrap()
                    .candidates
                    .len()
        );
    }

    #[test]
    fn retaining_every_interactive_prefix_matches_the_ordinary_surface() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let config = complete_turn_proposal_config_v1(false, 64, 64, 64, 64).unwrap();
        let mut ordinary =
            CompleteTurnExactEnvV1::from_position_with_config(position.clone(), config);
        let expected = ordinary.observe().unwrap().clone();

        let mut interactive = CompleteTurnExactEnvV1::from_position_with_config(position, config);
        let mut observed_layers = 0;
        loop {
            let Some(layer) = interactive.observe_prefix_layer().unwrap().cloned() else {
                break;
            };
            assert!(!layer.candidates.is_empty());
            assert!(layer.candidates.len() <= layer.max_retained_states);
            observed_layers += 1;
            interactive
                .retain_prefix_layer(&(0..layer.candidates.len()).collect::<Vec<_>>())
                .unwrap();
        }
        let actual = interactive.finish_prefix_surface().unwrap().clone();

        assert!(observed_layers > 0);
        assert_eq!(actual, expected);
        assert!(
            interactive
                .surface_diagnostics()
                .unwrap()
                .interactive_prefix_selection_active
        );
    }

    #[test]
    fn empty_external_reservation_matches_the_default_partial_selector() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let config = complete_turn_proposal_config_v1(false, 64, 2, 64, 64).unwrap();
        let mut ordinary =
            CompleteTurnExactEnvV1::from_position_with_config(position.clone(), config);
        let expected = ordinary.observe().unwrap().clone();

        let mut interactive = CompleteTurnExactEnvV1::from_position_with_config(position, config);
        while interactive.observe_prefix_layer().unwrap().is_some() {
            interactive
                .retain_prefix_layer_with_default_fill(&[])
                .unwrap();
        }
        let actual = interactive.finish_prefix_surface().unwrap().clone();

        assert_eq!(actual, expected);
    }

    #[test]
    fn completed_teacher_line_survives_an_empty_residual_prefix_retain() {
        let mut combat = blank_test_combat();
        let monster = planned_monster(EnemyId::JawWorm, 1);
        let target = monster.id;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let config = complete_turn_proposal_config_v1(false, 64, 64, 64, 64).unwrap();
        let exact_inputs = [
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
            ClientInput::EndTurn,
        ];
        let mut ordinary =
            CompleteTurnExactEnvV1::from_position_with_config(position.clone(), config);
        let target_ordinal = ordinary
            .matching_candidate_ordinal(&exact_inputs)
            .unwrap()
            .expect("ordinary surface retains the target complete turn");
        let target_actions = ordinary.observe().unwrap().candidates[target_ordinal]
            .actions
            .clone();

        let mut interactive = CompleteTurnExactEnvV1::from_position_with_config(position, config);
        let mut ended_with_empty_retain = false;
        loop {
            let Some(layer) = interactive.observe_prefix_layer().unwrap().cloned() else {
                break;
            };
            let prefix_len = layer.atomic_depth.min(target_actions.len());
            match interactive
                .matching_prefix_public_actions_ordinal(&target_actions[..prefix_len])
                .unwrap()
            {
                Some(ordinal) => interactive
                    .retain_prefix_layer_with_default_fill(&[ordinal])
                    .unwrap(),
                None => {
                    assert!(layer.atomic_depth >= target_actions.len());
                    interactive.retain_prefix_layer(&[]).unwrap();
                    ended_with_empty_retain = true;
                }
            }
        }
        interactive.finish_prefix_surface().unwrap();

        assert!(ended_with_empty_retain);
        assert!(interactive
            .matching_complete_turn_public_actions_ordinal(&target_actions)
            .unwrap()
            .is_some());
    }

    #[test]
    fn completed_endpoint_reservation_is_distinct_from_execution_choice() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let config = complete_turn_proposal_config_v1(false, 64, 64, 1, 1).unwrap();
        let mut env = CompleteTurnExactEnvV1::from_position_with_config(position, config);

        while let Some(layer) = env.observe_prefix_layer().unwrap().cloned() {
            env.retain_prefix_layer(&(0..layer.candidates.len()).collect::<Vec<_>>())
                .unwrap();
        }
        let endpoints = env.observe_completed_endpoints().unwrap().clone();
        assert!(endpoints.candidates.len() > endpoints.max_retained_states);
        assert!(env
            .matching_completed_endpoint_successor_ordinal(&[ClientInput::EndTurn])
            .unwrap()
            .is_some());
        let reserved_actions = endpoints
            .candidates
            .last()
            .expect("test surface has a completed endpoint")
            .actions
            .clone();
        let reserved_ordinal = env
            .matching_completed_endpoint_public_actions_ordinal(&reserved_actions)
            .unwrap()
            .expect("public endpoint actions uniquely match");

        let execution_surface = env
            .retain_completed_endpoints_with_default_fill(&[reserved_ordinal])
            .unwrap();
        assert_eq!(execution_surface.candidates.len(), 1);
        assert_eq!(execution_surface.candidates[0].actions, reserved_actions);
        let diagnostics = env.surface_diagnostics().unwrap();
        assert_eq!(diagnostics.completed_endpoint_reserved_candidate_count, 1);
        assert!(diagnostics.completed_endpoint_reservation_active);
        assert_eq!(env.terminal(), CombatTerminal::Unresolved);
    }
}
