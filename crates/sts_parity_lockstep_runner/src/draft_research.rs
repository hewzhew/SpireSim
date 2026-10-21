//! Bounded turn trials at an existing workspace node. The cached planner is
//! used only as a root generator; all selected edits remain workspace-owned.
use super::*;
use sts_combat_planner::{
    CombatDecisionRoot, CombatDraftSession, CombatPlanningQuantum, CompleteTurnOption,
    CompleteTurnOptionBoundary, TurnOptionAction, TurnOptionGeneratorConfig,
};
use sts_complete_turn_view::{project_combat_draft_option_v1, CompleteTurnSelectedActionV1};
use sts_oracle_run_control::{
    agent::information::state::{public_combat_state_v1, PublicCombatStateV1},
    content::{
        identity::gameplay_identity_from_input_alias_v1,
        powers::{store, uses_sentinel_amount, PowerId},
    },
    sim::combat::{CombatPosition, EngineCombatStepper},
    state::core::{ClientInput, EngineState},
};

const TRANSITION_STEPS: usize = 512;

#[path = "draft_research_page.rs"]
mod page;
use page::{outcome_label, trial_action_label, trial_line_label};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TurnResearchRequestV1 {
    pub work: usize,
    pub engine_steps: usize,
    #[serde(default)]
    pub potion_slots: Vec<u8>,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
    #[serde(default)]
    pub outcome: Option<TurnOutcomeV1>,
    #[serde(default)]
    pub minimum_hp: Option<i32>,
    #[serde(default)]
    pub minimum_block: Option<i32>,
    #[serde(default)]
    pub enemy_hp_at_most: Option<TurnEnemyHpLimitV1>,
    #[serde(default)]
    pub player_powers: Vec<TurnPlayerPowerFilterV1>,
}

/// Which side of a selected candidate's final EndTurn starts the next trial.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TurnResearchStartV1 {
    #[default]
    NextTurn,
    BeforeEndTurn,
}

/// The planner's settled boundary, independently of HP or enemy presence.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcomeV1 {
    NextTurn,
    Victory,
    Defeat,
    Escape,
}

impl From<CompleteTurnOptionBoundary> for TurnOutcomeV1 {
    fn from(boundary: CompleteTurnOptionBoundary) -> Self {
        match boundary {
            CompleteTurnOptionBoundary::NextPlayerTurn => Self::NextTurn,
            CompleteTurnOptionBoundary::TerminalWin => Self::Victory,
            CompleteTurnOptionBoundary::TerminalLoss => Self::Defeat,
            CompleteTurnOptionBoundary::Escape => Self::Escape,
        }
    }
}

impl TurnOutcomeV1 {
    fn id(self) -> &'static str {
        match self {
            Self::NextTurn => "next_turn",
            Self::Victory => "victory",
            Self::Defeat => "defeat",
            Self::Escape => "escape",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::NextTurn => "Next turn",
            Self::Victory => "Victory",
            Self::Defeat => "Defeat",
            Self::Escape => "Escape",
        }
    }
}

/// Conditions on the settled successor, not on which cards were played.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TurnPlayerPowerFilterV1 {
    pub power: String,
    #[serde(default = "default_present")]
    pub present: bool,
    #[serde(default)]
    pub minimum_amount: Option<i32>,
}

fn default_present() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TurnPlayerPowerChangeV1 {
    pub power: PowerId,
    pub before: Option<i32>,
    pub after: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TurnEnemyHpLimitV1 {
    pub slot: u8,
    pub hp: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TurnResearchOptionV1 {
    pub reference: String,
    pub label: String,
    pub boundary: String,
    pub action_count: usize,
    pub prefix_action_count: usize,
    pub player_power_changes: Vec<TurnPlayerPowerChangeV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TurnResearchPageV1 {
    pub after: Option<String>,
    pub hp_range: Option<TurnResourceRangeV1>,
    pub block_range: Option<TurnResourceRangeV1>,
    pub options: Vec<TurnResearchOptionV1>,
    pub total: usize,
    pub outcomes: BTreeMap<TurnOutcomeV1, usize>,
    pub outcome: Option<TurnOutcomeV1>,
    pub matching: usize,
    pub next_offset: Option<usize>,
    pub coverage: String,
    pub generation_work: usize,
    pub engine_steps: usize,
    pub potion_slots: Vec<u8>,
    pub minimum_hp: Option<i32>,
    pub minimum_block: Option<i32>,
    pub enemy_hp_at_most: Option<TurnEnemyHpLimitV1>,
    pub player_powers: Vec<TurnPlayerPowerFilterV1>,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TurnResearchDetailV1 {
    pub reference: String,
    pub boundary: String,
    pub actions: Vec<CompleteTurnSelectedActionV1>,
    pub successor: PublicCombatStateV1,
    pub summary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TurnResearchKey {
    node: usize,
    potion_mask: u64,
    after: Option<(u64, usize, TurnResearchStartV1)>,
}

#[derive(Clone)]
struct TurnResearchPrefix {
    exact_actions: Vec<TurnOptionAction>,
    actions: Vec<CompleteTurnSelectedActionV1>,
    label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct TurnResourceRangeV1 {
    pub minimum: i32,
    pub maximum: i32,
}

pub(super) struct TurnResearch {
    prefix: Option<TurnResearchPrefix>,
    id: String,
    generator: CombatDraftSession,
}

impl ManualDraftWorkspaceV1 {
    pub fn research_turns(
        &mut self,
        request: &TurnResearchRequestV1,
    ) -> Result<TurnResearchPageV1, String> {
        self.research_turns_after(request, None)
    }

    pub fn research_turns_after(
        &mut self,
        request: &TurnResearchRequestV1,
        after: Option<&str>,
    ) -> Result<TurnResearchPageV1, String> {
        self.research_turns_from(request, after, TurnResearchStartV1::NextTurn)
    }

    pub fn research_turns_from(
        &mut self,
        request: &TurnResearchRequestV1,
        after: Option<&str>,
        start: TurnResearchStartV1,
    ) -> Result<TurnResearchPageV1, String> {
        if after.is_none() && start != TurnResearchStartV1::NextTurn {
            return Err("before_end_turn requires an existing turn candidate".into());
        }
        if request.work > 65_536 || request.engine_steps > 1_048_576 || request.limit > 8 {
            return Err(
                "turn trial limits: work <= 65536, engine_steps <= 1048576, limit 0..8".into(),
            );
        }
        resolve_power_filters(&request.player_powers)?;
        if request.minimum_block.is_some_and(|block| block < 0) {
            return Err("minimum_block must be nonnegative".into());
        }
        if self.replay_stop.is_some()
            || self.strategic_preview_stop().is_some()
            || self.states[0].adapter.session().active_combat.is_none()
        {
            return Err("turn trials require a replayed combat workspace".into());
        }
        let adapter = &self.states.last().unwrap().adapter;
        if adapter.session().engine_state != EngineState::CombatPlayerTurn {
            return Err("resolve the current choice before requesting turn trials".into());
        }
        let active = adapter
            .session()
            .active_combat
            .as_ref()
            .ok_or("combat has ended")?;
        let mut position =
            CombatPosition::new(active.engine_state.clone(), active.combat_state.clone());
        let mut parent = None;
        if let Some(reference) = after {
            let (key, handle) = self.resolve_turn(reference)?;
            let trial = &self.research[&key];
            if trial.prefix.is_some() {
                return Err("choose a continuation before researching further".into());
            }
            let option = trial.generator.option(handle).unwrap();
            if option.boundary() != CompleteTurnOptionBoundary::NextPlayerTurn {
                return Err(
                    "continuation requires a candidate reaching the next player turn".into(),
                );
            }
            position = match start {
                TurnResearchStartV1::NextTurn => option.exact_successor().clone(),
                TurnResearchStartV1::BeforeEndTurn => {
                    let (last, prefix) = option
                        .actions()
                        .split_last()
                        .ok_or("empty turn candidate")?;
                    if last.input != ClientInput::EndTurn {
                        return Err("candidate must end with an explicit EndTurn".into());
                    }
                    // Reuse any already verified prefix root, independently of
                    // suffix potion permissions and result filters.
                    let cached = self.research.iter().find(|(k, _)| {
                        k.node == self.graph.cursor
                            && k.after == Some((key.potion_mask, handle.ordinal(), start))
                    });
                    if let Some((_, cached)) = cached {
                        cached.generator.current_root().unwrap().position().clone()
                    } else {
                        sts_combat_planner::replay_atomic_actions(
                            &EngineCombatStepper,
                            &position,
                            prefix,
                            TRANSITION_STEPS,
                        )
                        .map_err(|e| format!("turn prefix replay failed: {e:?}"))?
                        .0
                    }
                }
            };
            parent = Some((key, handle));
        }
        if request.enemy_hp_at_most.as_ref().is_some_and(|f| {
            !position
                .combat
                .entities
                .monsters
                .iter()
                .any(|m| m.slot == f.slot)
        }) {
            return Err("enemy HP filter must name a monster slot in this root".into());
        }
        let mut mask = 0u64;
        for &slot in &request.potion_slots {
            if slot >= 64
                || usize::from(slot) >= position.combat.entities.potions.len()
                || mask & (1 << slot) != 0
            {
                return Err("potion_slots must contain distinct existing slots".into());
            }
            mask |= 1 << slot;
        }
        let key = TurnResearchKey {
            node: self.graph.cursor,
            potion_mask: mask,
            after: parent.map(|(key, handle)| (key.potion_mask, handle.ordinal(), start)),
        };
        if !self.research.contains_key(&key) {
            let prefix = if let Some((parent_key, handle)) = parent {
                let trial = &self.research[&parent_key];
                let option = trial.generator.option(handle).unwrap().clone();
                let mut detail =
                    project_combat_draft_option_v1(&trial.generator, handle, TRANSITION_STEPS)?;
                // Establish a usable semantic prefix before allowing any further search.
                let (mut semantics, mut replay) = self.validate_turn_line(None, &option)?;
                let mut exact_actions = option.actions().to_vec();
                if start == TurnResearchStartV1::BeforeEndTurn {
                    exact_actions.pop();
                    detail.actions.pop();
                    if semantics.pop() != Some(SemanticActionV1::EndTurn) {
                        return Err("candidate's final semantic action must be EndTurn".into());
                    }
                    replay = self.replay_trial_semantics(&semantics)?;
                }
                let active = replay
                    .session()
                    .active_combat
                    .as_ref()
                    .ok_or("prefix ended combat")?;
                // The stepper consumes transition-only output mailboxes; the
                // run adapter can retain diagnostic notes for its caller.
                // Compare the same kernel boundary without erasing source output.
                let mut semantic_combat = active.combat_state.clone();
                semantic_combat.clear_transition_observations();
                position.combat.clear_transition_observations();
                if active.engine_state != position.engine || semantic_combat != position.combat {
                    return Err("turn prefix differs between planner and semantic replay".into());
                }
                let prefix_label = if detail.actions.is_empty() {
                    "current position, before End turn".to_owned()
                } else {
                    trial_line_label(&detail.actions)
                };
                Some(TurnResearchPrefix {
                    label: format!(
                        "{}\n  {}",
                        prefix_label,
                        outcome_label(detail.boundary, &public_combat_state_v1(&position.combat))
                    ),
                    actions: detail.actions,
                    exact_actions,
                })
            } else {
                None
            };
            let root =
                CombatDecisionRoot::new(position).map_err(|e| format!("turn trial root: {e:?}"))?;
            self.research.insert(
                key,
                TurnResearch {
                    id: uuid::Uuid::new_v4().simple().to_string(),
                    prefix,
                    generator: CombatDraftSession::new(
                        root,
                        TurnOptionGeneratorConfig {
                            max_engine_steps_per_transition: TRANSITION_STEPS,
                            allow_potion_expenditure: mask != 0,
                            allow_potion_discard: false,
                            allowed_potion_slots: Some(mask),
                            ..Default::default()
                        },
                    ),
                },
            );
        }
        let scope = self.action_view()?.reference_prefix().to_owned();
        self.research.get_mut(&key).unwrap().page(request, &scope)
    }

    fn resolve_turn(
        &mut self,
        reference: &str,
    ) -> Result<(TurnResearchKey, sts_combat_planner::CombatDraftOptionHandle), String> {
        let scope = self.action_view()?.reference_prefix().to_owned();
        for (key, trial) in &self.research {
            if key.node != self.graph.cursor {
                continue;
            }
            let prefix = format!("{scope}:turn:{}:", trial.id);
            if let Some(ordinal) = reference.strip_prefix(&prefix) {
                let index: usize = ordinal.parse().map_err(|_| "invalid turn reference")?;
                if index.to_string() != ordinal {
                    return Err("invalid turn reference".into());
                }
                let handle = trial
                    .generator
                    .option_handle(index)
                    .map_err(|_| "unknown turn candidate")?;
                return Ok((*key, handle));
            }
        }
        Err(
            "turn candidate belongs to another view or expired research; inspect current trials"
                .into(),
        )
    }

    pub fn inspect_turn(&mut self, reference: &str) -> Result<TurnResearchDetailV1, String> {
        let (key, handle) = self.resolve_turn(reference)?;
        let trial = &self.research[&key];
        let mut detail =
            project_combat_draft_option_v1(&trial.generator, handle, TRANSITION_STEPS)?;
        let mut summary = format!(
            "Rust: {}",
            outcome_label(detail.boundary, &detail.successor)
        );
        // Reuse the normal action presentation after semantic translation, so
        // draw, discovery and duplicate cards keep their actual step context.
        let option = trial.generator.option(handle).unwrap();
        if let Some(prefix) = &trial.prefix {
            detail.actions.splice(..0, prefix.actions.clone());
        }
        if matches!(
            detail.boundary,
            CompleteTurnOptionBoundary::NextPlayerTurn | CompleteTurnOptionBoundary::TerminalWin
        ) {
            let (actions, mut replay) = self.translated_turn(trial.prefix.as_ref(), option)?;
            for action in actions {
                let snapshot = replay.observe().map_err(|e| e.message)?;
                let space = sts_parity_lockstep::SemanticActionSpaceV1::from_snapshot(&snapshot)?;
                let labels = crate::describe_manual_actions_v1(&snapshot, &space, &[])?;
                let candidate = space
                    .actions
                    .iter()
                    .find(|c| c.action == action)
                    .ok_or("turn action disappeared")?;
                summary.push_str(&format!("\n{}", labels[&candidate.handle]));
                let prepared = replay.prepare(&action, &snapshot).map_err(|e| e.message)?;
                replay.execute(prepared).map_err(|e| e.message)?;
            }
            let snapshot = replay.observe().map_err(|e| e.message)?;
            let state = crate::DecisionStateV1::capture(replay.session(), &snapshot)?;
            summary.push_str(&format!(
                "\n\n{}",
                state.render(Some(&self.decision_state()?), true)?
            ));
        } else {
            for action in &detail.actions {
                summary.push_str(&format!("\n{}", trial_action_label(action)));
            }
        }
        Ok(TurnResearchDetailV1 {
            reference: reference.into(),
            boundary: TurnOutcomeV1::from(detail.boundary).id().into(),
            actions: detail.actions,
            successor: detail.successor,
            summary,
        })
    }

    fn translated_turn(
        &self,
        prefix: Option<&TurnResearchPrefix>,
        option: &CompleteTurnOption,
    ) -> Result<(Vec<SemanticActionV1>, RustLockstepAdapterV1), String> {
        let adapter = self.states.last().unwrap().adapter.clone();
        let inputs = prefix
            .into_iter()
            .flat_map(|p| p.exact_actions.iter())
            .chain(option.actions())
            .map(|a| a.input.clone())
            .collect::<Vec<_>>();
        let actions = match option.boundary() {
            CompleteTurnOptionBoundary::TerminalWin => {
                adapter.semantic_winning_combat_trajectory_from_inputs_v1(&inputs)?
            }
            CompleteTurnOptionBoundary::NextPlayerTurn => {
                adapter.semantic_combat_prefix_from_inputs_v1(&inputs)?
            }
            CompleteTurnOptionBoundary::Escape => {
                adapter.semantic_combat_escape_from_inputs_v1(&inputs)?
            }
            _ => {
                return Err("defeat trials remain inspectable; select another continuation".into())
            }
        };
        Ok((actions, adapter))
    }

    fn validate_turn_line(
        &self,
        prefix: Option<&TurnResearchPrefix>,
        option: &CompleteTurnOption,
    ) -> Result<(Vec<SemanticActionV1>, RustLockstepAdapterV1), String> {
        let active = self
            .states
            .last()
            .unwrap()
            .adapter
            .session()
            .active_combat
            .as_ref()
            .ok_or("combat has ended")?;
        let mut position =
            CombatPosition::new(active.engine_state.clone(), active.combat_state.clone());
        if let Some(prefix) = prefix {
            position = sts_combat_planner::replay_atomic_actions(
                &EngineCombatStepper,
                &position,
                &prefix.exact_actions,
                TRANSITION_STEPS,
            )
            .map_err(|e| format!("turn prefix replay failed: {e:?}"))?
            .0;
        }
        let root =
            CombatDecisionRoot::new(position).map_err(|e| format!("turn replay root: {e:?}"))?;
        sts_combat_planner::replay_turn_option(
            &root,
            option,
            &EngineCombatStepper,
            sts_combat_planner::ReplayLimits::deterministic(option.engine_steps()),
        )
        .map_err(|e| format!("turn candidate replay failed: {e:?}"))?;
        let (actions, _) = self.translated_turn(prefix, option)?;
        let replay = self.replay_trial_semantics(&actions)?;
        Ok((actions, replay))
    }

    fn replay_trial_semantics(
        &self,
        actions: &[SemanticActionV1],
    ) -> Result<RustLockstepAdapterV1, String> {
        let mut replay = self.states.last().unwrap().adapter.clone();
        for action in actions {
            validate_private_step(&self.states[0].adapter, &mut replay, action)?;
            let before = replay.observe().map_err(|e| e.message)?;
            let prepared = replay.prepare(action, &before).map_err(|e| e.message)?;
            replay.execute(prepared).map_err(|e| e.message)?;
        }
        Ok(replay)
    }

    pub fn choose_turn(&mut self, reference: &str) -> Result<(), String> {
        let (key, handle) = self.resolve_turn(reference)?;
        let trial = &self.research[&key];
        let (actions, _) = self.validate_turn_line(
            trial.prefix.as_ref(),
            trial.generator.option(handle).unwrap(),
        )?;
        // A storage error preserves the same acknowledged semantic prefix as
        // manual editing. No planner state becomes a durable game checkpoint.
        for action in actions {
            self.choose(action)?;
        }
        Ok(())
    }
}

fn resolve_power_filters(
    filters: &[TurnPlayerPowerFilterV1],
) -> Result<Vec<(PowerId, &TurnPlayerPowerFilterV1)>, String> {
    if filters.len() > 8 {
        return Err("at most eight player power conditions are supported".into());
    }
    let mut resolved = Vec::new();
    for filter in filters {
        let power = gameplay_identity_from_input_alias_v1::<PowerId>(&filter.power)
            .ok_or_else(|| format!("unknown or ambiguous player power: {}", filter.power))?;
        if resolved.iter().any(|(id, _)| *id == power) {
            return Err("use one condition per player power".into());
        }
        if filter.minimum_amount.is_some() && (!filter.present || uses_sentinel_amount(power)) {
            return Err("minimum_amount requires a present power with a numeric amount".into());
        }
        resolved.push((power, filter));
    }
    Ok(resolved)
}
