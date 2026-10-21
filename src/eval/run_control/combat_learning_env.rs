use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::agent::belief::sample_independent_combat_futures_v1;
use crate::agent::information::action::project_public_combat_actions_v1;
use crate::agent::information::combat::ObservationEvidenceKindV1;
use crate::agent::information::state::{
    public_combat_state_v1, CombatLearningEnemyIdentityV1, PublicCombatStateV1,
};
use crate::ai::combat_state_key::combat_exact_state_hash_v2;
use crate::content::potions::PotionId;
use crate::runtime::combat::CombatState;
use crate::runtime::rng::RngPool;
use crate::sim::combat_start::build_natural_combat_start;
use crate::state::core::{ActiveCombat, ClientInput, CombatContext, EngineState};
use crate::state::run::RunState;

use super::learning_env::{learning_combat_boundary_v1, prepare_learning_combat_input_v1};
use super::{
    run_control_session_fingerprint_v3, CombatBaselineOutcomeV1, LearningActionV1,
    LearningCombatBoundaryV1, RunControlSession, RunControlSessionCheckpointV1, RunDecisionAction,
};

/// Exact immutable combat root shared by every replicate in one comparison group.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningRootIdentityV1 {
    /// Exact normalized run-control fingerprint, including the active combat and run context.
    pub root_id: String,
    /// Exact combat-mechanics identity retained separately for diagnostics and validation.
    pub exact_combat_state_hash: String,
}

/// Small public-state summary captured once beside an exact combat root.
///
/// This is collection metadata, not a second combat observation or a policy feature schema.
/// Counts describe the exact root boundary and are intentionally independent from display text.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningRootContextV1 {
    pub act: u8,
    pub floor: i32,
    pub ascension_level: u8,
    pub turn: u32,
    pub is_boss_fight: bool,
    pub is_elite_fight: bool,
    pub monster_count: u32,
    pub living_monster_count: u32,
    pub potion_slot_count: u32,
    pub filled_potion_count: u32,
    pub usable_potion_count: u32,
    pub master_deck_card_count: u32,
    pub relic_count: u32,
    pub hand_card_count: u32,
    pub hp: i32,
    pub max_hp: i32,
}

/// One stochastic replicate from an exact combat root.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningEpisodeIdentityV1 {
    pub root: CombatLearningRootIdentityV1,
    pub replicate_index: u32,
}

/// Persistent run resources at one exact combat boundary.
///
/// Potion identity stays explicit evidence; this type assigns no retained value
/// or exchange rate between HP, gold, and inventory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningResourceSnapshotV1 {
    pub hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub potion_ids: Vec<Option<PotionId>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningTerminalOutcomeV1 {
    pub episode: CombatLearningEpisodeIdentityV1,
    pub combat: CombatBaselineOutcomeV1,
    pub resources: CombatLearningResourceSnapshotV1,
    pub enemy_start_hp: i32,
    pub enemy_final_hp: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatLearningBoundaryV1 {
    Decision {
        episode: CombatLearningEpisodeIdentityV1,
        boundary: LearningCombatBoundaryV1,
    },
    Terminal {
        outcome: CombatLearningTerminalOutcomeV1,
    },
}

impl CombatLearningBoundaryV1 {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal { .. })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningStepV1 {
    pub terminated: bool,
    pub boundary: CombatLearningBoundaryV1,
}

/// Exact resumable state for one combat episode.
///
/// The immutable root and replicate identity stay beside the current session state so a
/// checkpoint cannot silently lose the grouping needed by same-root estimators.
#[derive(Clone, Debug, PartialEq)]
pub struct CombatLearningEnvCheckpointV1 {
    episode: CombatLearningEpisodeIdentityV1,
    session: RunControlSessionCheckpointV1,
    root_previous_outcome: Option<CombatBaselineOutcomeV1>,
    enemy_start_hp: i32,
    combat_sequence: u64,
}

/// One immutable exact combat root from which caller-numbered replicates are created.
#[derive(Clone, Debug, PartialEq)]
pub struct CombatLearningRootV1 {
    identity: CombatLearningRootIdentityV1,
    context: CombatLearningRootContextV1,
    resources: CombatLearningResourceSnapshotV1,
    potion_uuids: Vec<Option<u32>>,
    session: RunControlSessionCheckpointV1,
    previous_outcome: Option<CombatBaselineOutcomeV1>,
    enemy_start_hp: i32,
    combat_sequence: u64,
}

/// One bounded scan of alternative floor-local RNG seed bases at combat entry.
///
/// Seed bases stay private provenance. The already-realized upstream run and its persistent RNG
/// streams remain fixed. Only checkpoints whose complete public decision equals the source
/// decision are retained, and duplicate exact private states are counted rather than emitted
/// twice.
#[derive(Clone, Debug, PartialEq)]
pub struct CombatEntryFloorChancePopulationV1 {
    checkpoints: Vec<RunControlSessionCheckpointV1>,
    pub accepted_floor_seed_bases: Vec<u64>,
    pub attempted_candidate_count: usize,
    pub public_match_count: usize,
    pub duplicate_private_state_count: usize,
}

impl CombatEntryFloorChancePopulationV1 {
    pub fn checkpoints(&self) -> &[RunControlSessionCheckpointV1] {
        &self.checkpoints
    }

    pub fn into_checkpoints(self) -> Vec<RunControlSessionCheckpointV1> {
        self.checkpoints
    }

    pub fn is_complete(&self, required_particles: usize) -> bool {
        self.checkpoints.len() == required_particles
    }
}

impl CombatLearningRootV1 {
    pub fn from_session(mut session: RunControlSession) -> Result<Self, String> {
        let (position, combat_sequence) = session.rebase_current_combat_outcome_tracking_v1()?;
        let identity = CombatLearningRootIdentityV1 {
            root_id: run_control_session_fingerprint_v3(&session),
            exact_combat_state_hash: combat_exact_state_hash_v2(&position.engine, &position.combat),
        };
        let context = combat_learning_root_context_v1(&session, &position.combat)?;
        let resources = combat_learning_resources_from_combat_v1(&position.combat);
        let potion_uuids = position
            .combat
            .entities
            .potions
            .iter()
            .map(|slot| slot.as_ref().map(|potion| potion.uuid))
            .collect();
        let previous_outcome = session.last_combat_baseline().cloned();
        let enemy_start_hp = super::outcome::combat_enemy_hp(&position.combat);
        if enemy_start_hp <= 0 {
            return Err("combat learning root enemy HP must be positive".to_string());
        }
        Ok(Self {
            identity,
            context,
            resources,
            potion_uuids,
            session: RunControlSessionCheckpointV1::from_session(&session),
            previous_outcome,
            enemy_start_hp,
            combat_sequence,
        })
    }

    pub fn from_checkpoint(checkpoint: RunControlSessionCheckpointV1) -> Result<Self, String> {
        Self::from_session(checkpoint.into_session()?)
    }

    pub fn identity(&self) -> &CombatLearningRootIdentityV1 {
        &self.identity
    }

    pub fn context(&self) -> &CombatLearningRootContextV1 {
        &self.context
    }

    pub fn resources(&self) -> &CombatLearningResourceSnapshotV1 {
        &self.resources
    }

    pub fn potion_uuids(&self) -> &[Option<u32>] {
        &self.potion_uuids
    }

    pub(super) fn session_checkpoint(&self) -> RunControlSessionCheckpointV1 {
        self.session.clone()
    }

    pub fn spawn(&self, replicate_index: u32) -> Result<CombatLearningEnvV1, String> {
        let env = CombatLearningEnvV1 {
            episode: CombatLearningEpisodeIdentityV1 {
                root: self.identity.clone(),
                replicate_index,
            },
            session: self.session.clone().into_session()?,
            root_previous_outcome: self.previous_outcome.clone(),
            enemy_start_hp: self.enemy_start_hp,
            combat_sequence: self.combat_sequence,
        };
        env.observe()?;
        Ok(env)
    }
}

/// Expand one exact source checkpoint into an ordered public-equivalent chance population.
///
/// The optional permutation is an offline coupling instrument: after each
/// independently sampled hidden draw order, output position `i` receives its
/// card from sampled position `permutation[i]`. A bijection preserves a uniform
/// unknown-order marginal while allowing two action branches to share selected
/// later draw windows. It is not evidence about the natural run-seed posterior.
pub fn combat_public_chance_particle_checkpoints_v1(
    source: RunControlSessionCheckpointV1,
    particle_seeds: &[u64],
    draw_order_permutation: Option<&[usize]>,
) -> Result<Vec<RunControlSessionCheckpointV1>, String> {
    let root = CombatLearningRootV1::from_checkpoint(source)?;
    let source_session = root.session.clone().into_session()?;
    let public_root = learning_combat_boundary_v1(&source_session)?;
    let source_position = source_session.current_combat_position_for_actions()?;
    let particles = sample_independent_combat_futures_v1(
        &source_position.engine,
        &source_position.combat,
        particle_seeds,
    )
    .map_err(|error| error.to_string())?;

    if let Some(permutation) = draw_order_permutation {
        if public_root.observation.cards.draw.evidence
            != ObservationEvidenceKindV1::PublicUnorderedCollection
        {
            return Err("draw-order permutation requires a public unordered draw pile".to_owned());
        }
        let draw_count = source_position.combat.zones.draw_pile.len();
        let mut indices = permutation.to_vec();
        indices.sort_unstable();
        if indices != (0..draw_count).collect::<Vec<_>>() {
            return Err(format!(
                "draw-order permutation must contain every index in 0..{draw_count} exactly once"
            ));
        }
    }

    particles
        .into_iter()
        .map(|particle| {
            let mut session = source_session.clone();
            let mut private_position = particle.into_private_position();
            if let Some(permutation) = draw_order_permutation {
                let original = private_position
                    .combat
                    .zones
                    .draw_pile
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                private_position.combat.zones.draw_pile = permutation
                    .iter()
                    .map(|index| original[*index].clone())
                    .collect::<Vec<_>>()
                    .into();
            }
            let active = session.active_combat.as_mut().ok_or_else(|| {
                "combat public-chance sampling requires an active combat".to_owned()
            })?;
            active.engine_state = private_position.engine.clone();
            active.combat_state = private_position.combat;
            session.engine_state = private_position.engine;
            session.run_state.rng_pool = active.combat_state.rng.pool.clone();
            if learning_combat_boundary_v1(&session)? != public_root {
                return Err(
                    "combat public-chance sampling changed the complete learning boundary"
                        .to_owned(),
                );
            }
            Ok(CombatLearningRootV1::from_session(session)?.session)
        })
        .collect()
}

/// Scan alternative floor-local RNG seed bases for public-equivalent natural combat entries.
///
/// This is deliberately limited to visible-intent, potion-empty, first-turn room combats. Each
/// candidate reconstructs the whole combat entry through `build_natural_combat_start`. The exact
/// persistent streams remain conditioned on the already-realized upstream run, while every
/// floor-local stream shares the real `candidate_floor_seed_base + floor` seed. The fixed exact
/// upstream run state is the conditioning boundary. Upstream map/deck history and persistent RNG
/// streams are not regenerated, so this is not a posterior over complete run seeds.
pub fn combat_entry_floor_chance_population_v1(
    source: RunControlSessionCheckpointV1,
    candidate_floor_seed_base_start: u64,
    max_candidates: usize,
    required_particles: usize,
) -> Result<CombatEntryFloorChancePopulationV1, String> {
    if required_particles == 0 {
        return Err("combat-entry floor chance population requires particles".to_owned());
    }
    if max_candidates == 0 {
        return Err("combat-entry floor chance scan requires candidates".to_owned());
    }

    let root = CombatLearningRootV1::from_checkpoint(source)?;
    let source_session = root.session.clone().into_session()?;
    let source_public = validate_combat_entry_floor_chance_source_v1(&source_session)?;

    let reconstructed_source = rebuild_combat_entry_with_floor_seed_base_v1(
        &source_session,
        source_session.run_state.seed,
    )?;
    if learning_combat_boundary_v1(&reconstructed_source)? != source_public {
        return Err(
            "combat entry cannot be reconstructed from its recorded source seed and public run state"
                .to_owned(),
        );
    }
    let reconstructed_root = CombatLearningRootV1::from_session(reconstructed_source)?;
    if reconstructed_root.identity.exact_combat_state_hash != root.identity.exact_combat_state_hash
    {
        return Err(
            "combat entry source floor seed reconstructs the public decision but not the exact combat state"
                .to_owned(),
        );
    }
    if reconstructed_root.identity.root_id != root.identity.root_id {
        return Err(
            "combat entry source floor seed reconstructs the exact combat state but not the exact run-control checkpoint"
                .to_owned(),
        );
    }

    let mut checkpoints = Vec::with_capacity(required_particles);
    let mut accepted_floor_seed_bases = Vec::with_capacity(required_particles);
    let mut exact_root_ids = BTreeSet::new();
    let mut attempted_candidate_count = 0usize;
    let mut public_match_count = 0usize;
    let mut duplicate_private_state_count = 0usize;

    const SCAN_BATCH_SIZE: usize = 8_192;
    while attempted_candidate_count < max_candidates && checkpoints.len() < required_particles {
        let batch_len = SCAN_BATCH_SIZE.min(max_candidates - attempted_candidate_count);
        let batch_start = candidate_floor_seed_base_start
            .checked_add(
                u64::try_from(attempted_candidate_count)
                    .map_err(|_| "combat-entry floor chance offset exceeds u64".to_owned())?,
            )
            .ok_or_else(|| {
                "combat-entry floor chance candidate seed range overflows u64".to_owned()
            })?;
        let public_matches = combat_entry_floor_public_matches_batch_v1(
            &source_session,
            &source_public,
            batch_start,
            batch_len,
        )?;
        attempted_candidate_count += batch_len;
        public_match_count += public_matches.len();
        for (candidate_floor_seed_base, candidate) in public_matches {
            let candidate_root = CombatLearningRootV1::from_session(candidate)?;
            if !exact_root_ids.insert(candidate_root.identity.root_id.clone()) {
                duplicate_private_state_count += 1;
                continue;
            }
            if checkpoints.len() < required_particles {
                accepted_floor_seed_bases.push(candidate_floor_seed_base);
                checkpoints.push(candidate_root.session);
            }
        }
    }

    Ok(CombatEntryFloorChancePopulationV1 {
        checkpoints,
        accepted_floor_seed_bases,
        attempted_candidate_count,
        public_match_count,
        duplicate_private_state_count,
    })
}

fn combat_entry_floor_public_matches_batch_v1(
    source: &RunControlSession,
    source_public: &LearningCombatBoundaryV1,
    candidate_floor_seed_base_start: u64,
    candidate_count: usize,
) -> Result<Vec<(u64, RunControlSession)>, String> {
    let worker_count = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(16)
        .min(candidate_count.max(1));
    let candidates_per_worker = candidate_count.div_ceil(worker_count);
    std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(worker_count);
        for worker_index in 0..worker_count {
            let first_offset = worker_index * candidates_per_worker;
            let end_offset = (first_offset + candidates_per_worker).min(candidate_count);
            if first_offset == end_offset {
                continue;
            }
            workers.push(scope.spawn(move || {
                let mut matches = Vec::new();
                let mut candidate_run_state = source.run_state.clone();
                for offset in first_offset..end_offset {
                    let candidate_floor_seed_base = candidate_floor_seed_base_start
                        .checked_add(u64::try_from(offset).map_err(|_| {
                            "combat-entry floor chance offset exceeds u64".to_owned()
                        })?)
                        .ok_or_else(|| {
                            "combat-entry floor chance candidate seed range overflows u64"
                                .to_owned()
                        })?;
                    let (engine_state, combat_state) = build_combat_entry_with_floor_seed_base_v1(
                        source,
                        &mut candidate_run_state,
                        candidate_floor_seed_base,
                    )?;
                    if combat_entry_candidate_may_match_public_shape_v1(
                        &combat_state,
                        &source_public.observation,
                    ) && combat_entry_candidate_matches_public_boundary_v1(
                        &engine_state,
                        &combat_state,
                        source_public,
                    ) {
                        let candidate = assemble_combat_entry_session_v1(
                            source,
                            candidate_run_state.clone(),
                            engine_state,
                            combat_state,
                        )?;
                        if learning_combat_boundary_v1(&candidate)? != *source_public {
                            return Err(
                                "combat-entry floor chance fast match disagrees with the complete public boundary"
                                    .to_owned(),
                            );
                        }
                        matches.push((candidate_floor_seed_base, candidate));
                    }
                }
                Ok::<_, String>(matches)
            }));
        }

        let mut matches = Vec::new();
        for worker in workers {
            matches.extend(
                worker
                    .join()
                    .map_err(|_| "combat-entry floor chance scan worker panicked".to_owned())??,
            );
        }
        Ok(matches)
    })
}

fn validate_combat_entry_floor_chance_source_v1(
    session: &RunControlSession,
) -> Result<LearningCombatBoundaryV1, String> {
    let boundary = learning_combat_boundary_v1(session)?;
    if !matches!(session.engine_state, EngineState::CombatPlayerTurn) {
        return Err(
            "combat-entry floor chance sampling requires a stable player-turn entry".to_owned(),
        );
    }
    let active = session
        .active_combat
        .as_ref()
        .ok_or_else(|| "combat-entry floor chance sampling requires an active combat".to_owned())?;
    if active.combat_state.turn.turn_count != 0 {
        return Err(
            "combat-entry floor chance sampling currently supports only combat-entry turn 0"
                .to_owned(),
        );
    }
    if active.encounter_id.is_none() {
        return Err(
            "combat-entry floor chance sampling requires a typed encounter identity".to_owned(),
        );
    }
    if !matches!(active.context, CombatContext::Room(_)) {
        return Err(
            "combat-entry floor chance sampling currently supports only room combats".to_owned(),
        );
    }
    if active
        .combat_state
        .entities
        .potions
        .iter()
        .any(Option::is_some)
    {
        return Err(
            "combat-entry floor chance sampling currently requires an empty potion inventory"
                .to_owned(),
        );
    }
    if boundary
        .observation
        .monsters
        .iter()
        .any(|monster| monster.intent.evidence != ObservationEvidenceKindV1::VisibleExact)
    {
        return Err(
            "combat-entry floor chance sampling does not yet support a hidden current intent"
                .to_owned(),
        );
    }
    Ok(boundary)
}

fn rebuild_combat_entry_with_floor_seed_base_v1(
    source: &RunControlSession,
    candidate_floor_seed_base: u64,
) -> Result<RunControlSession, String> {
    let mut run_state = source.run_state.clone();
    let (engine_state, combat_state) = build_combat_entry_with_floor_seed_base_v1(
        source,
        &mut run_state,
        candidate_floor_seed_base,
    )?;
    assemble_combat_entry_session_v1(source, run_state, engine_state, combat_state)
}

fn build_combat_entry_with_floor_seed_base_v1(
    source: &RunControlSession,
    run_state: &mut RunState,
    candidate_floor_seed_base: u64,
) -> Result<(EngineState, CombatState), String> {
    let source_active = source
        .active_combat
        .as_ref()
        .ok_or_else(|| "combat-entry floor chance sampling requires an active combat".to_owned())?;
    let encounter_id = source_active.encounter_id.ok_or_else(|| {
        "combat-entry floor chance sampling requires a typed encounter identity".to_owned()
    })?;
    let CombatContext::Room(room_context) = &source_active.context else {
        return Err(
            "combat-entry floor chance sampling currently supports only room combats".to_owned(),
        );
    };

    run_state.rng_pool = conditioned_combat_entry_floor_rng_pool_v1(
        &source.run_state.rng_pool,
        candidate_floor_seed_base,
        source.run_state.floor_num,
    );
    let (engine_state, mut combat_state) =
        build_natural_combat_start(run_state, encounter_id, room_context.room_type)?;
    combat_state.clear_transition_observations();
    Ok((engine_state, combat_state))
}

fn assemble_combat_entry_session_v1(
    source: &RunControlSession,
    run_state: RunState,
    engine_state: EngineState,
    combat_state: CombatState,
) -> Result<RunControlSession, String> {
    let source_active = source
        .active_combat
        .as_ref()
        .ok_or_else(|| "combat-entry floor chance sampling requires an active combat".to_owned())?;
    let encounter_id = source_active.encounter_id.ok_or_else(|| {
        "combat-entry floor chance sampling requires a typed encounter identity".to_owned()
    })?;

    let mut candidate = source.clone();
    candidate.engine_state = engine_state.clone();
    candidate.run_state = run_state;
    candidate.active_combat = Some(ActiveCombat::new_for_encounter(
        engine_state,
        combat_state,
        encounter_id,
        source_active.context.clone(),
    ));
    Ok(candidate)
}

fn combat_entry_candidate_matches_public_boundary_v1(
    engine_state: &EngineState,
    combat_state: &CombatState,
    source: &LearningCombatBoundaryV1,
) -> bool {
    if public_combat_state_v1(combat_state) != source.observation {
        return false;
    }
    project_public_combat_actions_v1(engine_state, combat_state)
        .is_ok_and(|actions| actions.public == source.public_actions)
}

/// Cheap necessary conditions before projecting the complete learning boundary.
///
/// This intentionally accepts false positives. The full hidden-free observation, legal surface,
/// representative mapping, and complete boundary are still checked before a particle is retained.
fn combat_entry_candidate_may_match_public_shape_v1(
    combat_state: &CombatState,
    source: &PublicCombatStateV1,
) -> bool {
    if combat_state.zones.hand.len() != source.cards.hand.cards.len()
        || !combat_state
            .zones
            .hand
            .iter()
            .zip(&source.cards.hand.cards)
            .all(|(candidate, source)| {
                candidate.id == source.card_id
                    && candidate.upgrades == source.upgrades
                    && candidate.misc_value == source.misc_value
                    && candidate.base_damage_override == source.base_damage_override
                    && candidate.base_block_override == source.base_block_override
                    && candidate.cost_modifier == source.cost_modifier
                    && candidate.cost_for_turn == source.cost_for_turn
                    && candidate.exhaust_override == source.exhaust_override
                    && candidate.retain_override == source.retain_override
                    && candidate.free_to_play_once == source.free_to_play_once
                    && candidate.energy_on_use == source.energy_on_use
            })
    {
        return false;
    }
    combat_state.entities.monsters.len() == source.monsters.len()
        && combat_state
            .entities
            .monsters
            .iter()
            .zip(&source.monsters)
            .all(|(candidate, source)| {
                let same_enemy = match source.enemy {
                    CombatLearningEnemyIdentityV1::Known { enemy_id } => {
                        candidate.enemy_id == enemy_id
                    }
                };
                same_enemy
                    && candidate.slot == source.slot
                    && candidate.current_hp == source.hp
                    && candidate.max_hp == source.max_hp
                    && candidate.block == source.block
                    && candidate.is_alive_for_action() == source.alive
                    && candidate.is_escaped == source.escaped
                    && candidate.is_dying == source.dying
                    && candidate.half_dead == source.half_dead
            })
}

fn conditioned_combat_entry_floor_rng_pool_v1(
    source: &RngPool,
    candidate_floor_seed_base: u64,
    floor: i32,
) -> RngPool {
    let mut pool = source.clone();
    pool.generate_floor_seeds(candidate_floor_seed_base, floor);
    pool
}

fn combat_learning_resources_from_combat_v1(
    combat: &crate::runtime::combat::CombatState,
) -> CombatLearningResourceSnapshotV1 {
    CombatLearningResourceSnapshotV1 {
        hp: combat.entities.player.current_hp,
        max_hp: combat.entities.player.max_hp,
        gold: combat.entities.player.gold,
        potion_ids: combat
            .entities
            .potions
            .iter()
            .map(|slot| slot.as_ref().map(|potion| potion.id))
            .collect(),
    }
}

fn combat_learning_resources_from_run_v1(
    session: &RunControlSession,
) -> CombatLearningResourceSnapshotV1 {
    CombatLearningResourceSnapshotV1 {
        hp: session.run_state.current_hp,
        max_hp: session.run_state.max_hp,
        gold: session.run_state.gold,
        potion_ids: session
            .run_state
            .potions
            .iter()
            .map(|slot| slot.as_ref().map(|potion| potion.id))
            .collect(),
    }
}

pub(super) fn combat_learning_root_context_v1(
    session: &RunControlSession,
    combat: &crate::runtime::combat::CombatState,
) -> Result<CombatLearningRootContextV1, String> {
    let potions = &combat.entities.potions;
    Ok(CombatLearningRootContextV1 {
        act: session.run_state.act_num,
        floor: session.run_state.floor_num,
        ascension_level: combat.meta.ascension_level,
        turn: combat.turn.turn_count,
        is_boss_fight: combat.meta.is_boss_fight,
        is_elite_fight: combat.meta.is_elite_fight,
        monster_count: combat_root_count_v1("monster", combat.entities.monsters.len())?,
        living_monster_count: combat_root_count_v1(
            "living monster",
            combat
                .entities
                .monsters
                .iter()
                .filter(|monster| monster.is_alive_for_action())
                .count(),
        )?,
        potion_slot_count: combat_root_count_v1("potion slot", potions.len())?,
        filled_potion_count: combat_root_count_v1(
            "filled potion",
            potions.iter().filter(|slot| slot.is_some()).count(),
        )?,
        usable_potion_count: combat_root_count_v1(
            "usable potion",
            potions
                .iter()
                .filter_map(Option::as_ref)
                .filter(|potion| {
                    crate::content::potions::potion_can_use_in_combat_like_java(potion, combat)
                })
                .count(),
        )?,
        master_deck_card_count: combat_root_count_v1(
            "master deck card",
            combat.meta.master_deck_snapshot.len(),
        )?,
        relic_count: combat_root_count_v1("relic", combat.entities.player.relics.len())?,
        hand_card_count: combat_root_count_v1("hand card", combat.zones.hand.len())?,
        hp: combat.entities.player.current_hp,
        max_hp: combat.entities.player.max_hp,
    })
}

fn combat_root_count_v1(kind: &str, count: usize) -> Result<u32, String> {
    u32::try_from(count).map_err(|_| format!("combat learning root {kind} count exceeds u32"))
}

#[derive(Clone, Debug)]
pub struct CombatLearningEnvV1 {
    episode: CombatLearningEpisodeIdentityV1,
    session: RunControlSession,
    root_previous_outcome: Option<CombatBaselineOutcomeV1>,
    enemy_start_hp: i32,
    combat_sequence: u64,
}

impl CombatLearningEnvV1 {
    pub fn from_root_session(
        session: RunControlSession,
        replicate_index: u32,
    ) -> Result<Self, String> {
        CombatLearningRootV1::from_session(session)?.spawn(replicate_index)
    }

    pub fn from_root_checkpoint(
        checkpoint: RunControlSessionCheckpointV1,
        replicate_index: u32,
    ) -> Result<Self, String> {
        Self::from_root_session(checkpoint.into_session()?, replicate_index)
    }

    pub fn from_checkpoint(checkpoint: CombatLearningEnvCheckpointV1) -> Result<Self, String> {
        validate_episode_identity_v1(&checkpoint.episode)?;
        let env = Self {
            episode: checkpoint.episode,
            session: checkpoint.session.into_session()?,
            root_previous_outcome: checkpoint.root_previous_outcome,
            enemy_start_hp: checkpoint.enemy_start_hp,
            combat_sequence: checkpoint.combat_sequence,
        };
        env.observe()?;
        Ok(env)
    }

    pub fn episode_identity(&self) -> &CombatLearningEpisodeIdentityV1 {
        &self.episode
    }

    pub fn observe(&self) -> Result<CombatLearningBoundaryV1, String> {
        if self.session.active_combat.is_some() {
            return Ok(CombatLearningBoundaryV1::Decision {
                episode: self.episode.clone(),
                boundary: learning_combat_boundary_v1(&self.session)?,
            });
        }

        let combat = self
            .session
            .last_combat_baseline()
            .filter(|outcome| Some(*outcome) != self.root_previous_outcome.as_ref())
            .cloned()
            .ok_or_else(|| {
                "combat learning episode left its root combat without a new typed outcome"
                    .to_string()
            })?;
        let enemy_final_hp = self
            .session
            .recent_combat_enemy_hp()
            .filter(|progress| progress.combat_sequence == self.combat_sequence)
            .map(|progress| progress.terminal_enemy_hp)
            .ok_or_else(|| {
                "combat learning terminal is missing aligned enemy HP facts".to_string()
            })?;
        Ok(CombatLearningBoundaryV1::Terminal {
            outcome: CombatLearningTerminalOutcomeV1 {
                episode: self.episode.clone(),
                combat,
                resources: combat_learning_resources_from_run_v1(&self.session),
                enemy_start_hp: self.enemy_start_hp,
                enemy_final_hp,
            },
        })
    }

    pub fn step(&mut self, action: LearningActionV1) -> Result<CombatLearningStepV1, String> {
        let input = self.prepare_action(action)?;
        self.step_prepared(input)
    }

    #[doc(hidden)]
    pub fn prepare_action(&self, action: LearningActionV1) -> Result<ClientInput, String> {
        if self.session.active_combat.is_none() {
            return Err("combat learning episode is already terminal".to_string());
        }
        let LearningActionV1::CombatInput { input } = action else {
            return Err("combat learning episode accepts only combat input actions".to_string());
        };
        prepare_learning_combat_input_v1(&self.session, input)
    }

    #[doc(hidden)]
    pub fn step_prepared(&mut self, input: ClientInput) -> Result<CombatLearningStepV1, String> {
        self.session
            .apply_decision_action(RunDecisionAction::Input(input))?;
        let boundary = self.observe()?;
        Ok(CombatLearningStepV1 {
            terminated: boundary.is_terminal(),
            boundary,
        })
    }

    pub fn checkpoint(&self) -> CombatLearningEnvCheckpointV1 {
        CombatLearningEnvCheckpointV1 {
            episode: self.episode.clone(),
            session: RunControlSessionCheckpointV1::from_session(&self.session),
            root_previous_outcome: self.root_previous_outcome.clone(),
            enemy_start_hp: self.enemy_start_hp,
            combat_sequence: self.combat_sequence,
        }
    }

    /// Rebase the current active combat boundary as a new immutable root.
    ///
    /// This deliberately creates a new root identity from the current exact
    /// session instead of reusing the episode's original lineage. Callers that
    /// expose the derived root must retain that parent lineage separately.
    pub fn current_root(&self) -> Result<CombatLearningRootV1, String> {
        CombatLearningRootV1::from_checkpoint(RunControlSessionCheckpointV1::from_session(
            &self.session,
        ))
    }

    pub fn restore(&mut self, checkpoint: CombatLearningEnvCheckpointV1) -> Result<(), String> {
        let restored = Self::from_checkpoint(checkpoint)?;
        if restored.episode != self.episode {
            return Err("combat learning checkpoint belongs to a different episode".to_string());
        }
        *self = restored;
        Ok(())
    }

    pub fn into_session(self) -> RunControlSession {
        self.session
    }
}

fn validate_episode_identity_v1(identity: &CombatLearningEpisodeIdentityV1) -> Result<(), String> {
    if identity.root.root_id.trim().is_empty() {
        return Err("combat learning root_id cannot be empty".to_string());
    }
    if identity.root.exact_combat_state_hash.trim().is_empty() {
        return Err("combat learning exact combat state hash cannot be empty".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::exordium::jaw_worm::JawWorm;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::monsters::EnemyId;
    use crate::content::monsters::MonsterBehavior;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::sim::combat::CombatTerminal;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    #[test]
    fn same_root_replicates_share_root_identity_without_sharing_episode_identity() {
        let root = combat_root_session(20);

        let first = CombatLearningEnvV1::from_root_session(root.clone(), 0)
            .expect("construct first combat replicate");
        let second = CombatLearningEnvV1::from_root_session(root, 1)
            .expect("construct second combat replicate");

        assert_eq!(first.episode.root, second.episode.root);
        assert_ne!(first.episode, second.episode);
    }

    #[test]
    fn root_identity_includes_run_context_not_only_combat_state() {
        let first_root = combat_root_session(20);
        let mut second_root = first_root.clone();
        second_root.run_state.gold += 1;

        let first =
            CombatLearningEnvV1::from_root_session(first_root, 0).expect("construct first root");
        let second = CombatLearningEnvV1::from_root_session(second_root, 0)
            .expect("construct context-distinct root");

        assert_eq!(
            first.episode.root.exact_combat_state_hash,
            second.episode.root.exact_combat_state_hash
        );
        assert_ne!(first.episode.root.root_id, second.episode.root.root_id);
    }

    #[test]
    fn root_context_captures_compact_public_facts_once() {
        let mut session = combat_root_session(20);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 23;
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        combat.meta.ascension_level = 7;
        combat.meta.is_elite_fight = true;
        combat.meta.master_deck_snapshot = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ]
        .into();
        combat.entities.player.current_hp = 61;
        combat.entities.player.max_hp = 85;
        combat.entities.player.relics = vec![RelicState::new(RelicId::BurningBlood)];
        combat.entities.potions = vec![
            Some(Potion::new(PotionId::FirePotion, 1)),
            Some(Potion::new(PotionId::FairyPotion, 2)),
            None,
        ];

        let root = CombatLearningRootV1::from_session(session).expect("construct root");

        assert_eq!(
            *root.context(),
            CombatLearningRootContextV1 {
                act: 2,
                floor: 23,
                ascension_level: 7,
                turn: 1,
                is_boss_fight: false,
                is_elite_fight: true,
                monster_count: 1,
                living_monster_count: 1,
                potion_slot_count: 3,
                filled_potion_count: 2,
                usable_potion_count: 1,
                master_deck_card_count: 2,
                relic_count: 1,
                hand_card_count: 1,
                hp: 61,
                max_hp: 85,
            }
        );
    }

    #[test]
    fn public_chance_particles_change_private_future_without_changing_the_decision() {
        let mut session = combat_root_session(20);
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Bash, 61),
            CombatCard::for_scenario(CardId::Defend, 62),
            CombatCard::for_scenario(CardId::Strike, 63),
        ]
        .into();
        let _ = combat.rng.ai_rng.random(99);
        let _ = combat.rng.potion_rng.random(99);
        let _ = combat.rng.potion_rng.random(99);
        let source = CombatLearningRootV1::from_session(session).expect("construct source root");
        let CombatLearningBoundaryV1::Decision {
            boundary: source_boundary,
            ..
        } = source.spawn(0).unwrap().observe().unwrap()
        else {
            panic!("source must be a combat decision");
        };

        let checkpoints =
            combat_public_chance_particle_checkpoints_v1(source.session.clone(), &[101, 202], None)
                .expect("sample public-equivalent private futures");
        let particles = checkpoints
            .into_iter()
            .map(CombatLearningRootV1::from_checkpoint)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for particle in &particles {
            let CombatLearningBoundaryV1::Decision { boundary, .. } =
                particle.spawn(0).unwrap().observe().unwrap()
            else {
                panic!("particle must be a combat decision");
            };
            assert_eq!(boundary, source_boundary);
            let exact = particle.session.clone().into_session().unwrap();
            let rng = &exact.active_combat.unwrap().combat_state.rng;
            assert_eq!(rng.ai_rng.counter, 1);
            assert_eq!(rng.potion_rng.counter, 2);
        }
        assert_ne!(particles[0].identity(), source.identity());
        assert_ne!(particles[0].identity(), particles[1].identity());
        assert!(combat_public_chance_particle_checkpoints_v1(
            source.session.clone(),
            &[101, 101],
            None
        )
        .is_err());
    }

    #[test]
    fn public_chance_draw_permutation_keeps_each_seed_and_public_boundary() {
        let mut session = combat_root_session(20);
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .zones
            .draw_pile = vec![
            CombatCard::for_scenario(CardId::Bash, 61),
            CombatCard::for_scenario(CardId::Defend, 62),
            CombatCard::for_scenario(CardId::Strike, 63),
        ]
        .into();
        let source = CombatLearningRootV1::from_session(session).unwrap();
        let plain =
            combat_public_chance_particle_checkpoints_v1(source.session.clone(), &[101], None)
                .unwrap();
        let rotated = combat_public_chance_particle_checkpoints_v1(
            source.session.clone(),
            &[101],
            Some(&[2, 0, 1]),
        )
        .unwrap();
        let plain = plain[0].clone().into_session().unwrap();
        let rotated = rotated[0].clone().into_session().unwrap();
        let plain_combat = &plain.active_combat.unwrap().combat_state;
        let rotated_combat = &rotated.active_combat.unwrap().combat_state;
        let plain_cards = plain_combat.zones.draw_pile.iter().collect::<Vec<_>>();
        let rotated_cards = rotated_combat.zones.draw_pile.iter().collect::<Vec<_>>();
        assert_eq!(
            rotated_cards,
            vec![plain_cards[2], plain_cards[0], plain_cards[1]]
        );
        assert_eq!(rotated_combat.rng, plain_combat.rng);

        let error =
            combat_public_chance_particle_checkpoints_v1(source.session, &[101], Some(&[0, 0, 1]))
                .expect_err("duplicate draw position must be rejected");
        assert!(error.contains("every index"));
    }

    #[test]
    fn public_chance_sampling_rejects_a_hidden_current_intent() {
        let mut session = combat_root_session(20);
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .player
            .relics
            .push(RelicState::new(RelicId::RunicDome));
        let source = CombatLearningRootV1::from_session(session).expect("construct source root");

        let error = combat_public_chance_particle_checkpoints_v1(source.session, &[303], None)
            .expect_err("hidden current intent needs its own conditional sampler");

        assert!(error.contains("hidden current intent"));
    }

    #[test]
    fn combat_entry_floor_chance_preserves_consumed_persistent_streams() {
        let seed = 2026081101;
        let source = natural_combat_entry_session(seed, EncounterId::Cultist);
        let source_root =
            CombatLearningRootV1::from_session(source).expect("capture natural combat entry");

        let population =
            combat_entry_floor_chance_population_v1(source_root.session.clone(), seed, 1, 1)
                .expect("reconstruct source seed through production combat start");

        assert!(population.is_complete(1));
        assert_eq!(population.accepted_floor_seed_bases, vec![seed]);
        assert_eq!(population.attempted_candidate_count, 1);
        assert_eq!(population.public_match_count, 1);
        assert_eq!(population.duplicate_private_state_count, 0);
        let reconstructed =
            CombatLearningRootV1::from_checkpoint(population.into_checkpoints().pop().unwrap())
                .expect("restore reconstructed particle");
        assert_eq!(reconstructed.identity(), source_root.identity());
    }

    #[test]
    fn checkpoint_restores_current_decision_and_rejects_cross_episode_restore() {
        let root = combat_root_session(20);
        let first = CombatLearningEnvV1::from_root_session(root.clone(), 0)
            .expect("construct first replicate");
        let checkpoint = first.checkpoint();
        let restored = CombatLearningEnvV1::from_checkpoint(checkpoint.clone())
            .expect("restore combat episode");
        assert_eq!(restored.observe().unwrap(), first.observe().unwrap());

        let mut other =
            CombatLearningEnvV1::from_root_session(root, 1).expect("construct other replicate");
        let before = other.observe().unwrap();
        let error = other
            .restore(checkpoint)
            .expect_err("cross-replicate restore must fail");
        assert!(error.contains("different episode"));
        assert_eq!(other.observe().unwrap(), before);
    }

    #[test]
    fn leaving_combat_returns_typed_combat_terminal_instead_of_run_boundary() {
        let mut env = CombatLearningEnvV1::from_root_session(combat_root_session(1), 7)
            .expect("construct lethal combat episode");

        let step = env
            .step(LearningActionV1::CombatInput {
                input: ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(7),
                },
            })
            .expect("play lethal strike");

        assert!(step.terminated);
        let CombatLearningBoundaryV1::Terminal { outcome } = step.boundary else {
            panic!("combat completion must terminate the combat episode");
        };
        assert_eq!(outcome.episode.replicate_index, 7);
        assert_eq!(outcome.combat.terminal, CombatTerminal::Win);
        assert_eq!(outcome.combat.start_hp, 80);
        assert_eq!(outcome.combat.final_hp, 80);
        assert_eq!(outcome.combat.cards_played, 1);
        assert_eq!(outcome.enemy_start_hp, 1);
        assert_eq!(outcome.enemy_final_hp, 0);
        assert!(env
            .step(LearningActionV1::CombatInput {
                input: ClientInput::EndTurn,
            })
            .is_err());
    }

    #[test]
    fn suffix_root_rebases_terminal_hp_to_its_own_exact_boundary() {
        let mut session = combat_root_session(1);
        session
            .rebase_current_combat_outcome_tracking_v1()
            .expect("capture original combat entry");
        session
            .active_combat
            .as_mut()
            .expect("active combat")
            .combat_state
            .entities
            .player
            .current_hp = 61;
        let mut env = CombatLearningEnvV1::from_root_session(session, 0)
            .expect("construct suffix combat root");

        let step = env
            .step(LearningActionV1::CombatInput {
                input: ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(7),
                },
            })
            .expect("finish suffix combat");
        let CombatLearningBoundaryV1::Terminal { outcome } = step.boundary else {
            panic!("suffix combat must terminate");
        };

        assert_eq!(outcome.combat.start_hp, 61);
        assert_eq!(outcome.combat.final_hp, 61);
        assert_eq!(outcome.combat.hp_loss, 0);
        let lineage = env
            .session
            .recent_combat_attrition()
            .expect("whole-combat attrition survives the suffix rebase");
        assert_eq!(lineage.start_hp, 80);
        assert_eq!(lineage.end_combat_hp, 61);
        assert_eq!(lineage.raw_hp_loss, 19);
    }

    #[test]
    fn suffix_rebase_keeps_prior_potion_use_in_whole_combat_attrition() {
        let mut session = combat_root_session(1);
        session
            .active_combat
            .as_mut()
            .expect("active combat")
            .combat_state
            .entities
            .potions = vec![Some(Potion::new(PotionId::StrengthPotion, 91))];
        let mut first = CombatLearningEnvV1::from_root_session(session, 0)
            .expect("construct combat entry root");
        let use_step = first
            .step(LearningActionV1::CombatInput {
                input: ClientInput::UsePotion {
                    potion_index: 0,
                    target: None,
                },
            })
            .expect("use strength potion");
        assert!(matches!(
            use_step.boundary,
            CombatLearningBoundaryV1::Decision { .. }
        ));

        let mut suffix = CombatLearningEnvV1::from_root_session(first.session, 1)
            .expect("construct post-potion suffix root");
        let lethal = suffix
            .step(LearningActionV1::CombatInput {
                input: ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(7),
                },
            })
            .expect("finish post-potion suffix");
        let CombatLearningBoundaryV1::Terminal { outcome } = lethal.boundary else {
            panic!("post-potion suffix must terminate");
        };

        assert_eq!(outcome.combat.potions_used, 0);
        assert_eq!(
            suffix
                .session
                .recent_combat_attrition()
                .expect("whole-combat potion attrition")
                .potions_used,
            1
        );
    }

    #[test]
    fn non_combat_root_is_rejected() {
        let error =
            CombatLearningEnvV1::from_root_session(RunControlSession::new(Default::default()), 0)
                .expect_err("strategic boundary is not a combat root");

        assert!(error.contains("no active combat"));
    }

    #[test]
    fn root_without_positive_enemy_hp_is_rejected() {
        let error = CombatLearningRootV1::from_session(combat_root_session(0))
            .expect_err("combat learning progress requires a positive enemy-HP denominator");

        assert!(error.contains("enemy HP must be positive"));
    }

    fn combat_root_session(monster_hp: i32) -> RunControlSession {
        let mut session = RunControlSession::new(Default::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 51)];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.current_hp = monster_hp;
        monster.max_hp = monster_hp;
        monster.set_planned_move_id(1);
        let plan = JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        session
    }

    fn natural_combat_entry_session(seed: u64, encounter_id: EncounterId) -> RunControlSession {
        let mut session = RunControlSession::new(crate::eval::run_control::RunControlConfig {
            seed,
            ..Default::default()
        });
        let _ = session.run_state.rng_pool.card_rng.random(17);
        let _ = session.run_state.rng_pool.event_rng.random_boolean();
        session.run_state.floor_num = 1;
        session
            .run_state
            .rng_pool
            .generate_floor_seeds(seed, session.run_state.floor_num);
        let room_type = RoomType::MonsterRoom;
        let (engine_state, mut combat_state) =
            crate::sim::combat_start::build_natural_combat_start(
                &mut session.run_state,
                encounter_id,
                room_type,
            )
            .expect("build natural combat entry");
        combat_state.clear_transition_observations();
        session.engine_state = engine_state.clone();
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            engine_state,
            combat_state,
            encounter_id,
            CombatContext::Room(RoomCombatContext { room_type }),
        ));
        session
    }
}
