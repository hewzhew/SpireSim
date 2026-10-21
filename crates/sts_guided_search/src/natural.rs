//! Fresh early Act 1 execution environment. Callers own policy and search.
//! The shared run loop owns all game effects, including combat settlement.
pub mod audit;
mod segment;
mod strategic;
pub use segment::{SegmentRecipe, SegmentRoom};
#[cfg(test)]
mod tests;
mod view;

use crate::{entities, observation, CombatBranch, CombatSearchSpace, CombatSearchState};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use sts_core::engine::run_loop::tick_run_active_with_observer;
use sts_core::sim::combat::{stable_boundary, CombatPosition};
use sts_core::state::core::{ActiveCombat, ClientInput, CombatContext, EngineState, RunResult};
use sts_core::state::map::node::RoomType;
use sts_core::state::run::RunState;

static NEXT_SOURCE: AtomicU64 = AtomicU64::new(1);
fn next_source() -> Result<u64, String> {
    NEXT_SOURCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| "natural source exhausted".into())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NaturalAction {
    Run(ClientInput),
    Combat(CombatBranch),
}

#[derive(Clone, Debug, Serialize)]
pub struct NaturalOption {
    pub action: NaturalAction,
    pub label: String,
    /// References the current combat observation; clients still submit `action`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_choice: Option<observation::ModelChoice>,
}

/// Current resources at the decision boundary; the run copy settles after combat.
#[derive(Serialize)]
pub struct NaturalResources<'a> {
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub potions: &'a [Option<sts_core::content::potions::Potion>],
    pub relics: &'a [sts_core::content::relics::RelicState],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CombatResult {
    pub floor: i32,
    pub encounter: Option<sts_core::content::monsters::factory::EncounterId>,
    pub context: CombatContext,
    pub burning: bool,
    pub entry_hp: i32,
    pub exit_hp: i32,
    pub potion_uses: usize,
    pub defeated: bool,
    pub escaped: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NaturalStop {
    EliteDefeated,
    SegmentCompleted,
    SegmentEscaped,
    SegmentNotDefeated,
    Death,
    MissedWindow,
    ActionBudget,
    ExecutionFailure(String),
}

/// One resident, newly initialized A20 Ironclad run, through the first F6–F8
/// elite reward boundary. Inputs execute through source-checked entrypoints.
pub struct NaturalEpisode {
    pub(crate) run: RunState,
    pub(crate) engine: EngineState,
    pub(crate) active: Option<ActiveCombat>,
    pub(crate) cursor: Option<CombatSearchState>,
    source: u64,
    steps: usize,
    max_actions: usize,
    stop: Option<NaturalStop>,
    results: Vec<CombatResult>,
    entry: Option<CombatResult>,
    segment: Option<SegmentRecipe>,
}

impl NaturalEpisode {
    pub fn new(seed: u64, max_actions: usize) -> Result<Self, String> {
        Self::from_initial_run(RunState::new(seed, 20, true, "Ironclad"), max_actions)
    }

    fn from_initial_run(run: RunState, max_actions: usize) -> Result<Self, String> {
        if max_actions == 0 {
            return Err("action budget must be positive".into());
        }
        Ok(Self {
            run,
            engine: EngineState::EventRoom,
            active: None,
            cursor: None,
            source: next_source()?,
            steps: 0,
            max_actions,
            stop: None,
            results: vec![],
            entry: None,
            segment: None,
        })
    }
    pub fn source(&self) -> u64 {
        self.source
    }
    pub fn steps(&self) -> usize {
        self.steps
    }
    pub fn stop(&self) -> Option<&NaturalStop> {
        self.stop.as_ref()
    }
    pub fn results(&self) -> &[CombatResult] {
        &self.results
    }
    pub fn run(&self) -> &RunState {
        &self.run
    }
    pub fn engine(&self) -> &EngineState {
        &self.engine
    }

    pub fn resources(&self) -> NaturalResources<'_> {
        if let Some(active) = &self.active {
            let entities = &active.combat_state.entities;
            NaturalResources {
                hp: entities.player.current_hp,
                max_hp: entities.player.max_hp,
                gold: entities.player.gold,
                potions: &entities.potions,
                relics: &entities.player.relics,
            }
        } else {
            NaturalResources {
                hp: self.run.current_hp,
                max_hp: self.run.max_hp,
                gold: self.run.gold,
                potions: &self.run.potions,
                relics: &self.run.relics,
            }
        }
    }

    pub fn options(&self) -> Result<Vec<NaturalOption>, String> {
        if self.stop.is_some() {
            return Ok(vec![]);
        }
        if let Some(state) = &self.cursor {
            let branches = CombatSearchSpace::default().decision_choices(state)?;
            let refs = branches.iter().map(|c| &c.action).collect::<Vec<_>>();
            let names = observation::model_choices(state, &refs)?;
            let observation = observation::combat_observation(state)?;
            return Ok(branches
                .into_iter()
                .zip(names)
                .map(|(c, name)| NaturalOption {
                    action: NaturalAction::Combat(c.action),
                    label: view::choice_label(&observation, &name),
                    combat_choice: Some(name),
                })
                .collect());
        }
        strategic::options(&self.run, &self.engine)
    }

    pub fn combat_entities(&self) -> Result<Option<entities::EntityPosition>, String> {
        self.cursor
            .as_ref()
            .map(|state| {
                let choices = CombatSearchSpace::default().decision_choices(state)?;
                entities::encode_choices(
                    state,
                    &choices.iter().map(|c| &c.action).collect::<Vec<_>>(),
                    None,
                )
            })
            .transpose()
    }

    pub fn combat_observation(&self) -> Result<Option<observation::CombatObservation>, String> {
        self.cursor
            .as_ref()
            .map(observation::combat_observation)
            .transpose()
    }

    /// Diagnostic/replay evidence, never an input to a public-information policy.
    pub fn snapshot(&self) -> serde_json::Value {
        let mut state = serde_json::json!({"run": sts_core::state::run::RunStateCheckpointV1::from_run_state(&self.run), "engine": self.engine, "active": self.active,
            "results": self.results, "stop": self.stop,
            "selection_family": self.cursor.as_ref().and_then(|s| s.selection_family()),
            "selection_members": self.cursor.as_ref().and_then(|s| s.selected_members()),
            "potion_uses": self.cursor.as_ref().map(|s| s.potion_uses())});
        if let Some(recipe) = &self.segment {
            state["supplied_segment"] = serde_json::to_value(recipe).unwrap();
        }
        state
    }

    /// Choose exactly one object from this source. Invalid/stale requests do not
    /// consume budget or alter the run. Execution errors seal the episode.
    pub fn choose(
        &mut self,
        source: u64,
        action: &NaturalAction,
    ) -> Result<Option<ClientInput>, String> {
        if source != self.source || self.stop.is_some() {
            return Err("stale or closed natural source".into());
        }
        if !self.options()?.iter().any(|o| &o.action == action) {
            return Err("action outside natural source".into());
        }
        self.apply_checked(action)
    }

    /// Execute a materialized solver input through the ordinary run loop.
    /// Validation includes symbolic selection families; a plan never replaces
    /// the active combat or writes its predicted resources into the run.
    pub fn choose_combat_input(&mut self, source: u64, input: &ClientInput) -> Result<(), String> {
        use sts_core::sim::combat::{CombatStepper, EngineCombatStepper};
        if source != self.source || self.stop.is_some() {
            return Err("stale or closed natural source".into());
        }
        let cursor = self.cursor.as_ref().ok_or("not a combat decision")?;
        if cursor.selection_family().is_some() {
            return Err(
                "finish the existing symbolic selection before submitting a materialized input"
                    .into(),
            );
        }
        if !EngineCombatStepper.is_legal_action(cursor.position(), input) {
            return Err("input outside the current combat action surface".into());
        }
        self.apply_checked(&NaturalAction::Combat(CombatBranch::Input(input.clone())))?;
        Ok(())
    }

    fn apply_checked(&mut self, action: &NaturalAction) -> Result<Option<ClientInput>, String> {
        self.source = next_source()?;
        self.steps += 1;
        let result = self.apply(action);
        if let Err(error) = &result {
            self.stop = Some(NaturalStop::ExecutionFailure(error.clone()));
        }
        if self.stop.is_none() && self.steps >= self.max_actions {
            self.stop = Some(NaturalStop::ActionBudget);
        }
        result
    }

    fn apply(&mut self, action: &NaturalAction) -> Result<Option<ClientInput>, String> {
        let input = match action {
            NaturalAction::Run(input) => input.clone(),
            NaturalAction::Combat(branch) => {
                let state = self.cursor.as_ref().ok_or("missing combat source")?;
                let space = CombatSearchSpace::default();
                match space.input(state, branch)? {
                    Some(input) => input,
                    None => {
                        self.cursor = Some(space.virtual_successor(state, branch)?);
                        return Ok(None);
                    }
                }
            }
        };
        let uses = self.cursor.as_ref().map_or(0, |s| s.potion_uses())
            + usize::from(matches!(input, ClientInput::UsePotion { .. }) && self.active.is_some());
        if let Some(entry) = &mut self.entry {
            entry.potion_uses = uses;
        }
        self.tick(Some(input.clone()))?;
        let mut settled = false;
        for _ in 0..100_000 {
            if self.stop.is_some() {
                break;
            }
            let pending = matches!(
                self.engine,
                EngineState::CombatStart(_) | EngineState::CombatProcessing
            ) || self.active.as_ref().is_some_and(|a| {
                !matches!(
                    self.engine,
                    EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
                ) || !stable_boundary(&self.engine, &a.combat_state)
            });
            if !pending {
                settled = true;
                break;
            }
            self.tick(None)?;
        }
        if !settled && self.stop.is_none() {
            return Err("natural input exceeded settling tick budget".into());
        }
        self.cursor = None;
        if let Some(active) = &self.active {
            let mut state = CombatSearchState::new(CombatPosition::new(
                self.engine.clone(),
                active.combat_state.clone(),
            ))?;
            state.potion_uses = uses;
            self.cursor = Some(state);
        }
        if self.segment.is_none()
            && self.stop.is_none()
            && (self.run.act_num != 1 || self.run.floor_num > 8)
        {
            self.stop = Some(NaturalStop::MissedWindow);
        }
        Ok(Some(input))
    }

    fn tick(&mut self, input: Option<ClientInput>) -> Result<(), String> {
        let entry_hp = self.run.current_hp;
        let outcome = sts_core::state::run::with_suppressed_obtain_logs(|| {
            tick_run_active_with_observer(&mut self.engine, &mut self.run, &mut self.active, input)
        });
        if self.entry.is_none() {
            if let Some(a) = &self.active {
                self.entry = Some(CombatResult {
                    floor: self.run.floor_num,
                    encounter: a.encounter_id,
                    context: a.context.clone(),
                    burning: self
                        .run
                        .map
                        .get_current_node()
                        .is_some_and(|n| n.has_emerald_key),
                    entry_hp,
                    exit_hp: entry_hp,
                    potion_uses: 0,
                    defeated: false,
                    escaped: false,
                });
            }
        }
        if let Some(finished) = outcome.finished_combat {
            let mut entry = self.entry.take().ok_or("combat settlement without entry")?;
            entry.exit_hp = self.run.current_hp;
            entry.escaped = self.run.room_smoked;
            entry.defeated = !entry.escaped
                && finished.combat_state.entities.player.current_hp > 0
                && finished
                    .combat_state
                    .entities
                    .monsters
                    .iter()
                    .all(|m| m.current_hp <= 0 && !m.is_escaped && !m.half_dead);
            if let Some(recipe) = &self.segment {
                let expected = recipe
                    .route
                    .get(self.results.len())
                    .ok_or("combat outside supplied segment")?;
                if entry.encounter != Some(expected.encounter)
                    || !matches!(&entry.context, CombatContext::Room(c) if c.room_type == expected.room)
                    || entry.floor != self.results.len() as i32 + 1
                {
                    return Err("combat does not match supplied segment".into());
                }
                if entry.escaped {
                    self.stop = Some(NaturalStop::SegmentEscaped);
                } else if !entry.defeated && self.run.current_hp > 0 {
                    self.stop = Some(NaturalStop::SegmentNotDefeated);
                } else if entry.defeated && self.results.len() + 1 == recipe.route.len() {
                    self.stop = Some(NaturalStop::SegmentCompleted);
                }
            } else if entry.defeated
                && (6..=8).contains(&entry.floor)
                && matches!(&entry.context, CombatContext::Room(c) if c.room_type == RoomType::MonsterRoomElite)
            {
                self.stop = Some(NaturalStop::EliteDefeated);
            }
            self.results.push(entry);
        }
        if matches!(self.engine, EngineState::GameOver(RunResult::Defeat)) {
            self.stop = Some(NaturalStop::Death);
        } else if !outcome.keep_running {
            return Err(format!("run loop stopped at {}", self.engine.kind_v1()));
        }
        Ok(())
    }
}
