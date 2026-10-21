//! Explicit conditional resource task over the resident continuous run owner.
//! No policy, search, reward shaping, state import or teacher is implemented here.
use crate::entities::EntityPosition;
use crate::natural::{NaturalAction, NaturalEpisode, NaturalStop, SegmentRecipe};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sts_core::content::potions::PotionId;
use sts_core::runtime::combat::MetaChange;
use sts_core::state::core::{ClientInput, EngineState};
use sts_core::state::map::RoomType;
use sts_core::state::rewards::RewardItem;

pub const SCHEMA: u32 = 1;
pub const CONTEXT_WIDTH: usize = 24;
pub const CONTEXT_KINDS: usize = 7;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRequirement {
    pub potion: PotionId,
    pub count: usize,
}

/// A conditional task, not a claim that these requirements are whole-run value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentRequirement {
    pub min_hp: i32,
    pub reserve: Vec<PotionRequirement>,
}

impl SegmentRequirement {
    fn validate(&self) -> Result<(), String> {
        if !(1..=999).contains(&self.min_hp) || self.reserve.len() > 5 {
            return Err("invalid segment resource requirement".into());
        }
        for (i, r) in self.reserve.iter().enumerate() {
            if !(1..=5).contains(&r.count) || self.reserve[..i].iter().any(|p| p.potion == r.potion)
            {
                return Err("duplicate or invalid potion requirement".into());
            }
        }
        Ok(())
    }

    /// Require the outgoing type inventory, not the original instance or zero uses.
    pub fn assess(&self, episode: &NaturalEpisode) -> Result<Option<bool>, String> {
        self.validate()?;
        match episode.stop() {
            Some(NaturalStop::SegmentCompleted) => {
                let r = episode.resources();
                Ok(Some(
                    r.hp >= self.min_hp
                        && self.reserve.iter().all(|p| {
                            r.potions
                                .iter()
                                .flatten()
                                .filter(|q| q.id == p.potion)
                                .count()
                                >= p.count
                        }),
                ))
            }
            Some(
                NaturalStop::Death | NaturalStop::SegmentEscaped | NaturalStop::SegmentNotDefeated,
            ) => Ok(Some(false)),
            None | Some(NaturalStop::ActionBudget | NaturalStop::ExecutionFailure(_)) => Ok(None),
            _ => Err("resource target requires a supplied segment endpoint".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BetweenRooms {
    /// A fixed experimental continuation: claim gold, decline all other rewards,
    /// follow the only supplied route. Not learned strategic behavior.
    GoldOnly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentTaskSpec {
    pub seed: u64,
    pub recipe: SegmentRecipe,
    pub requirement: SegmentRequirement,
    pub between_rooms: BetweenRooms,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SegmentPosition {
    pub schema: u32,
    pub combat_schema: u32,
    pub combat: EntityPosition,
    pub context_ids: Vec<[i64; 2]>,
    pub context_values: Vec<[f32; CONTEXT_WIDTH]>,
    pub context_refs: Vec<[i64; 3]>,
}

impl SegmentPosition {
    fn push(&mut self, kind: i64, category: i64, v: [f32; CONTEXT_WIDTH], refs: [i64; 3]) -> i64 {
        let index = self.context_ids.len() as i64;
        self.context_ids.push([kind, category]);
        self.context_values.push(v);
        self.context_refs.push(refs);
        index
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SegmentTransition {
    pub before: String,
    pub decision: Option<SegmentPosition>,
    pub action: NaturalAction,
    pub input: Option<ClientInput>,
    pub after: String,
}

pub struct SegmentTask {
    spec: SegmentTaskSpec,
    episode: NaturalEpisode,
    max_actions: usize,
    initial: serde_json::Value,
    transitions: Vec<SegmentTransition>,
    failure: Option<String>,
}

fn digest(episode: &NaturalEpisode) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&episode.snapshot()).unwrap())
    )
}

impl SegmentTask {
    pub fn new(spec: SegmentTaskSpec, max_actions: usize) -> Result<Self, String> {
        spec.requirement.validate()?;
        if !(1..=8192).contains(&max_actions) {
            return Err("invalid segment task action budget".into());
        }
        let episode = NaturalEpisode::from_segment(spec.seed, max_actions, &spec.recipe)?;
        let initial = episode.snapshot();
        let mut task = Self {
            spec,
            episode,
            max_actions,
            initial,
            transitions: vec![],
            failure: None,
        };
        task.intermission()?;
        // A failed input projection is an admission failure, not an invisible case.
        task.decision()?;
        Ok(task)
    }
    pub fn source(&self) -> u64 {
        self.episode.source()
    }
    pub fn episode(&self) -> &NaturalEpisode {
        &self.episode
    }
    pub fn transitions(&self) -> &[SegmentTransition] {
        &self.transitions
    }
    pub fn requirement(&self) -> &SegmentRequirement {
        &self.spec.requirement
    }

    pub fn decision(&self) -> Result<Option<SegmentPosition>, String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.episode.stop().is_some() {
            return Ok(None);
        }
        let combat = self
            .episode
            .combat_entities()?
            .ok_or("segment task is not at a combat decision")?;
        let mut p = SegmentPosition {
            schema: SCHEMA,
            combat_schema: crate::entities::SCHEMA,
            combat,
            context_ids: vec![],
            context_values: vec![],
            context_refs: vec![],
        };
        let current = self.episode.results().len();
        let mut goal = [0.; CONTEXT_WIDTH];
        goal[..4].copy_from_slice(&[
            self.spec.requirement.min_hp as f32 / 75.,
            current as f32 / 8.,
            self.spec.recipe.route.len() as f32 / 8.,
            (self.spec.recipe.route.len() - current) as f32 / 8.,
        ]);
        // Category 1 is the enforced GoldOnly continuation, not a client hint.
        p.push(1, 1, goal, [-1; 3]);
        for (offset, room) in self.spec.recipe.route[current..].iter().enumerate() {
            let mut v = [0.; CONTEXT_WIDTH];
            v[..3].copy_from_slice(&[
                offset as f32 / 8.,
                f32::from(offset == 0),
                f32::from(room.room == RoomType::MonsterRoomElite),
            ]);
            p.push(2, room.encounter.stable_ordinal() as i64, v, [0, -1, -1]);
        }
        // Canonical order prevents the serialization order of a set from becoming a feature.
        let mut reserves = self.spec.requirement.reserve.iter().collect::<Vec<_>>();
        reserves.sort_by_key(|r| r.potion.stable_ordinal());
        for r in reserves {
            let mut v = [0.; CONTEXT_WIDTH];
            v[0] = r.count as f32;
            p.push(3, r.potion.stable_ordinal() as i64, v, [0, -1, -1]);
        }
        let mut master = vec![];
        for (i, c) in self.episode.run().master_deck.iter().enumerate() {
            let mut v = [0.; CONTEXT_WIDTH];
            v[..4].copy_from_slice(&[
                i as f32 / 32.,
                c.upgrades as f32,
                c.misc_value as f32 / 20.,
                c.cost_modifier as f32 / 3.,
            ]);
            for (slot, value, scale) in [
                (4, c.base_damage_override, 20.),
                (6, c.base_block_override, 20.),
                (8, c.cost_for_turn.map(i32::from), 3.),
                (10, c.exhaust_override.map(i32::from), 1.),
                (12, c.retain_override.map(i32::from), 1.),
            ] {
                v[slot] = f32::from(value.is_some());
                v[slot + 1] = value.unwrap_or(0) as f32 / scale;
            }
            v[14] = f32::from(c.free_to_play_once);
            v[15] = c.energy_on_use as f32 / 3.;
            master.push((
                c.lineage,
                p.push(4, c.id.stable_ordinal() as i64, v, [0, -1, -1]),
            ));
        }
        let active = self
            .episode
            .active
            .as_ref()
            .ok_or("missing resident combat")?;
        for (i, change) in active.combat_state.meta.meta_changes.iter().enumerate() {
            let (kind, lineage, amount) = match change {
                MetaChange::ModifyCardMisc {
                    card_lineage,
                    amount,
                } => (5, *card_lineage, *amount),
                MetaChange::UpgradeMasterDeckCard { card_lineage } => (6, *card_lineage, 1),
                MetaChange::AddCardToMasterDeck(_) => {
                    return Err("unsettled acquisition outside segment encoding".into())
                }
            };
            let target = master
                .iter()
                .find(|(id, _)| *id == lineage)
                .ok_or("permanent change lacks a master-deck source")?
                .1;
            let mut v = [0.; CONTEXT_WIDTH];
            v[0] = i as f32 / 32.;
            v[1] = amount as f32 / if kind == 5 { 20. } else { 1. };
            p.push(kind, 0, v, [target, -1, -1]);
        }
        Ok(Some(p))
    }

    fn perform(&mut self, action: &NaturalAction) -> Result<(), String> {
        let before = digest(&self.episode);
        let decision = if matches!(action, NaturalAction::Combat(_)) {
            self.decision()?
        } else {
            None
        };
        let input = self.episode.choose(self.episode.source(), action)?;
        self.transitions.push(SegmentTransition {
            before,
            decision,
            action: action.clone(),
            input,
            after: digest(&self.episode),
        });
        Ok(())
    }

    fn intermission(&mut self) -> Result<(), String> {
        while self.episode.stop().is_none() && self.episode.active.is_none() {
            let input = match self.episode.engine() {
                EngineState::MapNavigation | EngineState::MapOverlay { .. } => {
                    ClientInput::SelectMapNode(0)
                }
                EngineState::RewardScreen(r) if r.pending_card_choice.is_none() => {
                    match r.items.iter().position(|x| {
                        matches!(x, RewardItem::Gold { .. } | RewardItem::StolenGold { .. })
                    }) {
                        Some(i) => ClientInput::ClaimReward(i),
                        None => ClientInput::Proceed,
                    }
                }
                other => {
                    return Err(format!(
                        "unexpected segment intermission boundary: {}",
                        other.kind_v1()
                    ))
                }
            };
            self.perform(&NaturalAction::Run(input))?;
        }
        Ok(())
    }

    /// Ordinals only index this exact source's combat choices. No stale rebinding.
    pub fn advance(&mut self, source: u64, ordinal: usize) -> Result<(), String> {
        if self.failure.is_some() || self.episode.stop().is_some() || source != self.source() {
            return Err("closed or stale segment source".into());
        }
        let options = self.episode.options()?;
        let action = &options
            .get(ordinal)
            .ok_or("segment action outside source")?
            .action;
        if !matches!(action, NaturalAction::Combat(_)) {
            return Err("expected combat action".into());
        }
        let result = self.perform(action).and_then(|_| self.intermission());
        if let Err(e) = &result {
            self.failure = Some(e.clone());
        }
        result
    }

    pub fn target(&self) -> Result<Option<bool>, String> {
        if self.failure.is_some() {
            Ok(None)
        } else {
            self.spec.requirement.assess(&self.episode)
        }
    }

    /// Reconstruct from the declared recipe, never from an evidence snapshot.
    pub fn verify(&self) -> Result<(), String> {
        if self.failure.is_some() {
            return Err("failed segment cannot receive replay verification".into());
        }
        let episode =
            NaturalEpisode::from_segment(self.spec.seed, self.max_actions, &self.spec.recipe)?;
        let mut replay = Self {
            spec: self.spec.clone(),
            initial: episode.snapshot(),
            episode,
            max_actions: self.max_actions,
            transitions: vec![],
            failure: None,
        };
        if replay.initial != self.initial {
            return Err("segment initial state changed".into());
        }
        for step in &self.transitions {
            if digest(&replay.episode) != step.before {
                return Err("segment replay source changed".into());
            }
            let numbers = if matches!(step.action, NaturalAction::Combat(_)) {
                replay.decision()?
            } else {
                None
            };
            if numbers != step.decision {
                return Err("segment numerical decision changed".into());
            }
            let input = replay.episode.choose(replay.source(), &step.action)?;
            if input != step.input || digest(&replay.episode) != step.after {
                return Err("segment replay successor changed".into());
            }
        }
        if replay.episode.snapshot() != self.episode.snapshot()
            || replay.target()? != self.target()?
        {
            return Err("segment final resources or target changed".into());
        }
        Ok(())
    }

    pub fn evidence(&self) -> serde_json::Value {
        let target = self.target();
        let assessment_error = target.as_ref().err().cloned();
        serde_json::json!({"contract":"segment_resource_task_v1", "specification":self.spec,
            "max_actions":self.max_actions, "initial":self.initial, "transitions":self.transitions,
            "successor":self.episode.snapshot(), "resources":self.episode.resources(), "failure":self.failure,
            "assessment_error":assessment_error, "attained":target.ok().flatten()})
    }
}

#[cfg(test)]
mod tests;
