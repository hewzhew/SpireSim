//! Python projection of one exact same-root combat replicate group.
//!
//! Ordinal decoding and semantic encoding stay in the shared bridge driver. This module owns
//! only combat-group construction and typed terminal columns; it has no policy or objective.

use numpy::PyArray1;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde::Serialize;
use sts_oracle_eval::agent::information::state::{
    CombatLearningCardV1, CombatLearningEnemyIdentityV1, CombatLearningIntentV1,
    CombatLearningPowerV1,
};
use sts_oracle_eval::content::potions::PotionId;
use sts_oracle_eval::sim::combat::CombatTerminal;
use sts_oracle_learning::eval::run_control::{
    CombatLearningBoundaryV1, CombatLearningEnvPoolV1, CombatLearningRootBatchArtifactV1,
    CombatLearningRootContextV1, CombatLearningRootIdentityV1, CombatLearningRootV1,
    LearningActionV1,
};

use super::{
    bridge_states_ready, choose_bridge_ordinals, collect_ready_actions,
    combat_decision_audit_json_from_source, decision_batch_dict, decision_snapshot_from_source,
    runtime_error, semantic_snapshot_from_source, states_from_source, usize_array, value_error,
    BridgeSlotState,
};

pub(super) const COMBAT_TERMINAL_WIN: u8 = 0;
pub(super) const COMBAT_TERMINAL_LOSS: u8 = 1;
pub(super) const COMBAT_TERMINAL_UNRESOLVED: u8 = 2;

const READY_ACTION_TRACE_SCHEMA_NAME: &str = "CombatLearningReadyActionTrace";
const READY_ACTION_TRACE_SCHEMA_VERSION: u32 = 2;

/// Compact public combat state aligned to one current model decision row.
///
/// This is trajectory data, not a reward or a second observation encoding.  Repeated
/// selection rows for the same unchanged simulator boundary intentionally repeat it.
#[pyclass(frozen)]
pub(super) struct CombatLearningDecisionProgressV1 {
    replicate_index: usize,
    turn: u32,
    player_hp: i32,
    player_max_hp: i32,
    enemy_hp: i32,
    enemy_max_hp: i32,
    potion_uuids: Vec<Option<u32>>,
    potion_ids: Vec<Option<String>>,
}

#[pymethods]
impl CombatLearningDecisionProgressV1 {
    #[getter]
    fn replicate_index(&self) -> usize {
        self.replicate_index
    }

    #[getter]
    fn turn(&self) -> u32 {
        self.turn
    }

    #[getter]
    fn player_hp(&self) -> i32 {
        self.player_hp
    }

    #[getter]
    fn player_max_hp(&self) -> i32 {
        self.player_max_hp
    }

    #[getter]
    fn enemy_hp(&self) -> i32 {
        self.enemy_hp
    }

    #[getter]
    fn enemy_max_hp(&self) -> i32 {
        self.enemy_max_hp
    }

    #[getter]
    fn potion_uuids(&self) -> Vec<Option<u32>> {
        self.potion_uuids.clone()
    }

    #[getter]
    fn potion_ids(&self) -> Vec<Option<String>> {
        self.potion_ids.clone()
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ReadyActionTraceV1<'a> {
    schema_name: &'static str,
    schema_version: u32,
    replicate_index: usize,
    decision_ordinals: &'a [usize],
    turn: u32,
    energy: u8,
    player_hp: i32,
    player_max_hp: i32,
    player_block: i32,
    player_powers: &'a [CombatLearningPowerV1],
    hand: Vec<ReadyActionCardV1<'a>>,
    draw_count: usize,
    discard_count: usize,
    exhaust_count: usize,
    potions: Vec<Option<PotionId>>,
    monsters: Vec<ReadyActionMonsterV1<'a>>,
    action: &'a LearningActionV1,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ReadyActionCardV1<'a> {
    card_id: sts_oracle_eval::content::cards::CardId,
    upgrades: u8,
    effective_cost: i32,
    damage_by_monster_order: &'a [i32],
}

impl<'a> ReadyActionCardV1<'a> {
    fn from_card(card: &'a CombatLearningCardV1) -> Self {
        Self {
            card_id: card.card_id,
            upgrades: card.upgrades,
            effective_cost: card.effective_cost,
            damage_by_monster_order: &card.damage_by_monster_order,
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ReadyActionMonsterV1<'a> {
    slot: u8,
    enemy: CombatLearningEnemyIdentityV1,
    hp: i32,
    max_hp: i32,
    block: i32,
    intent: &'a CombatLearningIntentV1,
    powers: &'a [CombatLearningPowerV1],
}

/// Read-only Python view of the compact public context captured with one exact root.
#[pyclass(frozen, name = "CombatLearningRootContextV1")]
pub(super) struct PyCombatLearningRootContextV1 {
    inner: CombatLearningRootContextV1,
}

impl PyCombatLearningRootContextV1 {
    pub(super) fn from_context(inner: CombatLearningRootContextV1) -> Self {
        Self { inner }
    }
}

/// Opaque in-process recovery root with explicit parent episode lineage.
///
/// Capturing this object clones exactly one caller-selected current session.
/// The bridge keeps no automatic history and exposes no raw session payload.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub(super) struct CombatLearningRecoveryRoot {
    root: CombatLearningRootV1,
    source_root: CombatLearningRootIdentityV1,
    source_replicate_index: u32,
}

#[pymethods]
impl CombatLearningRecoveryRoot {
    #[getter]
    fn root_id(&self) -> String {
        self.root.identity().root_id.clone()
    }

    #[getter]
    fn exact_combat_state_hash(&self) -> String {
        self.root.identity().exact_combat_state_hash.clone()
    }

    #[getter]
    fn source_root_id(&self) -> String {
        self.source_root.root_id.clone()
    }

    #[getter]
    fn source_exact_combat_state_hash(&self) -> String {
        self.source_root.exact_combat_state_hash.clone()
    }

    #[getter]
    fn source_replicate_index(&self) -> u32 {
        self.source_replicate_index
    }

    #[getter]
    fn root_context(&self) -> PyCombatLearningRootContextV1 {
        PyCombatLearningRootContextV1::from_context(*self.root.context())
    }

    /// Persist this caller-selected exact decision boundary as one opaque root artifact.
    ///
    /// Python receives only the canonical bytes accepted by
    /// ``LearningBatchEnv.from_combat_root_artifact_bytes``; runtime entity ids and the
    /// run-control checkpoint remain private to Rust.
    #[pyo3(signature = (*, max_bytes))]
    fn combat_root_artifact_bytes<'py>(
        &self,
        py: Python<'py>,
        max_bytes: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let payload = self
            .encode_combat_root_artifact(max_bytes)
            .map_err(value_error)?;
        Ok(PyBytes::new(py, &payload))
    }

    #[pyo3(signature = (replicate_count, potion_slots=None))]
    fn spawn_group(
        &self,
        replicate_count: usize,
        potion_slots: Option<Vec<usize>>,
    ) -> PyResult<CombatLearningBatchEnv> {
        CombatLearningBatchEnv::from_root_with_potion_slots(
            &self.root,
            replicate_count,
            potion_slots,
        )
        .map_err(value_error)
    }
}

impl CombatLearningRecoveryRoot {
    fn encode_combat_root_artifact(&self, max_bytes: usize) -> Result<Vec<u8>, String> {
        CombatLearningRootBatchArtifactV1::from_roots([&self.root])?.encode(max_bytes)
    }
}

#[pymethods]
impl PyCombatLearningRootContextV1 {
    #[getter]
    fn act(&self) -> u8 {
        self.inner.act
    }

    #[getter]
    fn floor(&self) -> i32 {
        self.inner.floor
    }

    #[getter]
    fn ascension_level(&self) -> u8 {
        self.inner.ascension_level
    }

    #[getter]
    fn turn(&self) -> u32 {
        self.inner.turn
    }

    #[getter]
    fn is_boss_fight(&self) -> bool {
        self.inner.is_boss_fight
    }

    #[getter]
    fn is_elite_fight(&self) -> bool {
        self.inner.is_elite_fight
    }

    #[getter]
    fn monster_count(&self) -> u32 {
        self.inner.monster_count
    }

    #[getter]
    fn living_monster_count(&self) -> u32 {
        self.inner.living_monster_count
    }

    #[getter]
    fn potion_slot_count(&self) -> u32 {
        self.inner.potion_slot_count
    }

    #[getter]
    fn filled_potion_count(&self) -> u32 {
        self.inner.filled_potion_count
    }

    #[getter]
    fn usable_potion_count(&self) -> u32 {
        self.inner.usable_potion_count
    }

    #[getter]
    fn master_deck_card_count(&self) -> u32 {
        self.inner.master_deck_card_count
    }

    #[getter]
    fn relic_count(&self) -> u32 {
        self.inner.relic_count
    }

    #[getter]
    fn hand_card_count(&self) -> u32 {
        self.inner.hand_card_count
    }

    #[getter]
    fn hp(&self) -> i32 {
        self.inner.hp
    }

    #[getter]
    fn max_hp(&self) -> i32 {
        self.inner.max_hp
    }
}

/// Same-root combat replicates created from one exact live run-control slot.
///
/// This class deliberately has no public constructor: a combat group must be derived from a
/// typed `LearningBatchEnv` combat boundary so Python never reconstructs simulator state.
#[pyclass]
pub(super) struct CombatLearningBatchEnv {
    pool: CombatLearningEnvPoolV1,
    states: Vec<BridgeSlotState>,
}

#[pymethods]
impl CombatLearningBatchEnv {
    #[getter]
    fn root_id(&self) -> String {
        self.pool.root_identity().root_id.clone()
    }

    #[getter]
    fn exact_combat_state_hash(&self) -> String {
        self.pool.root_identity().exact_combat_state_hash.clone()
    }

    #[getter]
    fn root_context(&self) -> PyCombatLearningRootContextV1 {
        PyCombatLearningRootContextV1::from_context(*self.pool.root_context())
    }

    #[getter]
    fn root_gold(&self) -> i32 {
        self.pool.root_resources().gold
    }

    #[getter]
    fn root_potion_ids(&self) -> Vec<Option<String>> {
        potion_id_names(&self.pool.root_resources().potion_ids)
    }

    #[getter]
    fn replicate_count(&self) -> usize {
        self.pool.replicate_count()
    }

    #[getter]
    fn potion_slots(&self) -> Option<Vec<usize>> {
        self.pool
            .potion_policy()
            .root_slots()
            .map(<[usize]>::to_vec)
    }

    #[getter]
    fn terminal_count(&self) -> usize {
        self.pool.terminal_count()
    }

    #[getter]
    fn ready(&self) -> bool {
        bridge_states_ready(&self.states)
    }

    #[pyo3(signature = (dense_mask=false, semantic=false))]
    fn decision_batch<'py>(
        &self,
        py: Python<'py>,
        dense_mask: bool,
        semantic: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let snapshot =
            decision_snapshot_from_source(&self.pool, &self.states).map_err(runtime_error)?;
        let semantic = semantic
            .then(|| semantic_snapshot_from_source(&self.pool, &self.states))
            .transpose()
            .map_err(runtime_error)?;
        decision_batch_dict(py, snapshot, dense_mask, semantic, None, None)
    }

    /// Return compact public progress in exactly the current decision-row order.
    fn decision_progress(&self) -> PyResult<Vec<CombatLearningDecisionProgressV1>> {
        let snapshot =
            decision_snapshot_from_source(&self.pool, &self.states).map_err(runtime_error)?;
        snapshot
            .slot_indices
            .into_iter()
            .map(|replicate_index| {
                let replicate_index_u32 = u32::try_from(replicate_index)
                    .map_err(|_| PyValueError::new_err("combat replicate index exceeds u32"))?;
                let Some(CombatLearningBoundaryV1::Decision { boundary, .. }) =
                    self.pool.boundary(replicate_index_u32)
                else {
                    return Err(PyValueError::new_err(format!(
                        "combat decision row {replicate_index} has no active boundary"
                    )));
                };
                let observation = &boundary.observation;
                let enemy_hp = observation
                    .monsters
                    .iter()
                    .filter(|monster| monster.alive)
                    .fold(0i32, |total, monster| total.saturating_add(monster.hp));
                let enemy_max_hp = observation
                    .monsters
                    .iter()
                    .fold(0i32, |total, monster| total.saturating_add(monster.max_hp));
                Ok(CombatLearningDecisionProgressV1 {
                    replicate_index,
                    turn: observation.turn.turn_count,
                    player_hp: observation.player.hp,
                    player_max_hp: observation.player.max_hp,
                    enemy_hp,
                    enemy_max_hp,
                    potion_uuids: boundary.private_resolution.potion_uuids_by_slot.clone(),
                    potion_ids: observation
                        .potions
                        .iter()
                        .map(|potion| {
                            potion
                                .as_ref()
                                .map(|potion| potion.potion_id.stable_id().to_string())
                        })
                        .collect(),
                })
            })
            .collect()
    }

    /// Return the complete typed candidate set for one current combat decision.
    ///
    /// This diagnostic view is read-only, excluded from model input, and returns
    /// ``None`` after an action has been fully decoded or the replicate is terminal.
    fn combat_decision_audit_json(&self, replicate_index: usize) -> PyResult<Option<String>> {
        combat_decision_audit_json_from_source(&self.pool, &self.states, replicate_index)
            .map_err(runtime_error)
    }

    fn choose(&mut self, ordinals: Vec<usize>) -> PyResult<()> {
        choose_bridge_ordinals(&self.pool, &mut self.states, ordinals).map_err(value_error)
    }

    /// Return one compact, typed pre-action record after ordinal decoding.
    ///
    /// This diagnostic view is optional and never participates in policy
    /// inference. A symbolic selection that is not yet ready returns ``None``.
    fn ready_action_trace_json(&self, replicate_index: usize) -> PyResult<Option<String>> {
        let Some(state) = self.states.get(replicate_index) else {
            return Err(PyValueError::new_err(format!(
                "combat replicate index {replicate_index} is out of range"
            )));
        };
        let BridgeSlotState::Ready {
            action,
            decision_ordinals,
            ..
        } = state
        else {
            return Ok(None);
        };
        let replicate_index_u32 = u32::try_from(replicate_index)
            .map_err(|_| PyValueError::new_err("combat replicate index exceeds u32"))?;
        let Some(CombatLearningBoundaryV1::Decision { boundary, .. }) =
            self.pool.boundary(replicate_index_u32)
        else {
            return Err(PyValueError::new_err(
                "ready combat action has no decision boundary",
            ));
        };
        let observation = &boundary.observation;
        let trace = ReadyActionTraceV1 {
            schema_name: READY_ACTION_TRACE_SCHEMA_NAME,
            schema_version: READY_ACTION_TRACE_SCHEMA_VERSION,
            replicate_index,
            decision_ordinals,
            turn: observation.turn.turn_count,
            energy: observation.turn.energy,
            player_hp: observation.player.hp,
            player_max_hp: observation.player.max_hp,
            player_block: observation.player.block,
            player_powers: &observation.player.powers,
            hand: observation
                .cards
                .hand
                .cards
                .iter()
                .map(ReadyActionCardV1::from_card)
                .collect(),
            draw_count: observation.cards.draw.cards.len(),
            discard_count: observation.cards.discard.cards.len(),
            exhaust_count: observation.cards.exhaust.cards.len(),
            potions: observation
                .potions
                .iter()
                .map(|potion| potion.as_ref().map(|potion| potion.potion_id))
                .collect(),
            monsters: observation
                .monsters
                .iter()
                .map(|monster| ReadyActionMonsterV1 {
                    slot: monster.slot,
                    enemy: monster.enemy,
                    hp: monster.hp,
                    max_hp: monster.max_hp,
                    block: monster.block,
                    intent: &monster.intent,
                    powers: &monster.powers,
                })
                .collect(),
            action,
        };
        serde_json::to_string(&trace).map(Some).map_err(|error| {
            PyValueError::new_err(format!(
                "failed to encode ready combat action trace: {error}"
            ))
        })
    }

    /// Capture one undecoded active replicate as an opaque recovery root.
    ///
    /// A partially decoded symbolic action is intentionally rejected: recovery
    /// roots are simulator boundaries, not bridge-local ordinal prefixes.
    fn capture_recovery_root(
        &self,
        replicate_index: usize,
    ) -> PyResult<CombatLearningRecoveryRoot> {
        if !matches!(
            self.states.get(replicate_index),
            Some(BridgeSlotState::Root)
        ) {
            return Err(PyValueError::new_err(format!(
                "combat replicate {replicate_index} must be at an undecoded active decision"
            )));
        }
        let replicate_index = u32::try_from(replicate_index)
            .map_err(|_| PyValueError::new_err("combat replicate index exceeds u32"))?;
        let root = self
            .pool
            .current_root(replicate_index)
            .map_err(runtime_error)?;
        Ok(CombatLearningRecoveryRoot {
            root,
            source_root: self.pool.root_identity().clone(),
            source_replicate_index: replicate_index,
        })
    }

    fn step<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        if !self.ready() {
            return Err(PyValueError::new_err(
                "all active combat replicates must finish root and selection decisions before step",
            ));
        }
        let actions = collect_ready_actions(&self.states).map_err(runtime_error)?;
        let step = self.pool.step_active(actions).map_err(runtime_error)?;
        self.states = states_from_source(&self.pool).map_err(runtime_error)?;

        let result = PyDict::new(py);
        result.set_item("root_id", self.pool.root_identity().root_id.as_str())?;
        result.set_item(
            "exact_combat_state_hash",
            self.pool.root_identity().exact_combat_state_hash.as_str(),
        )?;
        result.set_item(
            "slot_indices",
            usize_array(
                py,
                step.slots
                    .iter()
                    .map(|slot| slot.replicate_index as usize)
                    .collect(),
            ),
        )?;
        result.set_item(
            "terminated",
            PyArray1::from_vec(py, step.slots.iter().map(|slot| slot.terminated).collect()),
        )?;
        let terminal_slots = step
            .slots
            .iter()
            .filter_map(|slot| {
                slot.terminal_outcome
                    .as_ref()
                    .map(|outcome| (slot, outcome))
            })
            .collect::<Vec<_>>();
        result.set_item(
            "terminal_slot_indices",
            usize_array(
                py,
                terminal_slots
                    .iter()
                    .map(|(slot, _)| slot.replicate_index as usize)
                    .collect(),
            ),
        )?;
        result.set_item(
            "terminal_kind",
            PyArray1::from_vec(
                py,
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| combat_terminal_code(outcome.combat.terminal))
                    .collect(),
            ),
        )?;
        result.set_item(
            "terminal_won",
            PyArray1::from_vec(
                py,
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.terminal == CombatTerminal::Win)
                    .collect(),
            ),
        )?;
        for (key, values) in [
            (
                "terminal_start_hp",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.start_hp)
                    .collect(),
            ),
            (
                "terminal_final_hp",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.final_hp)
                    .collect(),
            ),
            (
                "terminal_hp_loss",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.hp_loss)
                    .collect(),
            ),
            (
                "terminal_enemy_start_hp",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.enemy_start_hp)
                    .collect(),
            ),
            (
                "terminal_enemy_final_hp",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.enemy_final_hp)
                    .collect(),
            ),
        ] {
            result.set_item(key, PyArray1::from_vec(py, values))?;
        }
        for (key, values) in [
            (
                "terminal_turns",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.turns)
                    .collect(),
            ),
            (
                "terminal_potions_used",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.potions_used)
                    .collect(),
            ),
            (
                "terminal_potions_discarded",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.potions_discarded)
                    .collect(),
            ),
            (
                "terminal_cards_played",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.combat.cards_played)
                    .collect(),
            ),
        ] {
            result.set_item(key, PyArray1::from_vec(py, values))?;
        }
        for (key, values) in [
            (
                "terminal_final_max_hp",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.resources.max_hp)
                    .collect(),
            ),
            (
                "terminal_final_gold",
                terminal_slots
                    .iter()
                    .map(|(_, outcome)| outcome.resources.gold)
                    .collect(),
            ),
        ] {
            result.set_item(key, PyArray1::from_vec(py, values))?;
        }
        result.set_item(
            "terminal_potion_ids",
            terminal_slots
                .iter()
                .map(|(_, outcome)| potion_id_names(&outcome.resources.potion_ids))
                .collect::<Vec<_>>(),
        )?;
        Ok(result)
    }
}

pub(super) fn potion_id_names(
    potion_ids: &[Option<sts_oracle_eval::content::potions::PotionId>],
) -> Vec<Option<String>> {
    potion_ids
        .iter()
        .map(|potion| potion.map(|potion| potion.stable_id().to_string()))
        .collect()
}

impl CombatLearningBatchEnv {
    pub(super) fn from_root_with_potion_slots(
        root: &CombatLearningRootV1,
        replicate_count: usize,
        potion_slots: Option<Vec<usize>>,
    ) -> Result<Self, String> {
        let pool = CombatLearningEnvPoolV1::from_root_with_potion_slots(
            root,
            replicate_count,
            potion_slots,
        )
        .map_err(|error| error.to_string())?;
        let states = states_from_source(&pool)?;
        Ok(Self { pool, states })
    }
}

fn combat_terminal_code(terminal: CombatTerminal) -> u8 {
    match terminal {
        CombatTerminal::Win => COMBAT_TERMINAL_WIN,
        CombatTerminal::Loss => COMBAT_TERMINAL_LOSS,
        CombatTerminal::Unresolved => COMBAT_TERMINAL_UNRESOLVED,
    }
}
