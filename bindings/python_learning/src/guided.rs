//! Thin NumPy adapter for independent local-combat learning experiments.
use numpy::ndarray::{Array2, Array3};
use numpy::{IntoPyArray, PyArray1};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple, PyType};
use sts_guided_search::local::{
    LocalCombatBatch, LocalEnemy, LocalTask, NumericBatch, NumericPosition, CARD_COUNT, CARD_WIDTH,
    CONTEXT_WIDTH, SCHEMA,
};

/// JSON only at the exact-state boundary; inference uses the ordinary NumPy
/// entity frame. This projects detached Rust states and grants no run authority.
#[pyfunction]
#[pyo3(signature = (positions_json, *, potion_uses=None, max_potion_uses=None))]
pub fn combat_entity_batch<'py>(
    py: Python<'py>,
    positions_json: &str,
    potion_uses: Option<Vec<usize>>,
    max_potion_uses: Option<usize>,
) -> PyResult<Bound<'py, PyDict>> {
    let roots: Vec<sts_oracle_eval::sim::combat::CombatPosition> =
        serde_json::from_str(positions_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    if roots.is_empty() || roots.len() > 128 {
        return Err(PyValueError::new_err(
            "exact entity batch requires 1..128 positions",
        ));
    }
    let uses = potion_uses.unwrap_or_else(|| vec![0; roots.len()]);
    if uses.len() != roots.len() {
        return Err(PyValueError::new_err(
            "potion-use counts must match exact positions",
        ));
    }
    let positions = roots
        .into_iter()
        .zip(uses)
        .enumerate()
        .map(|(i, (root, uses))| {
            sts_guided_search::entities::encode_position(root, uses, max_potion_uses)
                .map_err(|e| PyValueError::new_err(format!("exact position {i}: {e}")))
        })
        .collect::<PyResult<Vec<_>>>()?;
    entity_batch(
        py,
        NumericBatch {
            revision: 0,
            slots: (0..positions.len()).collect(),
            positions,
        },
    )
}

#[pyclass(name = "GuidedCombatBatch")]
pub struct GuidedCombatBatch {
    core: LocalCombatBatch,
}

#[pymethods]
impl GuidedCombatBatch {
    /// Per-case construction data, converted once at setup; no JSON in step IO.
    #[classmethod]
    #[pyo3(signature = (specifications, max_decisions, *, search_services=None, policy_rollout_steps=0))]
    fn from_specifications(
        _cls: &Bound<'_, PyType>,
        specifications: &Bound<'_, PyList>,
        max_decisions: usize,
        search_services: Option<usize>,
        policy_rollout_steps: usize,
    ) -> PyResult<Self> {
        let py = specifications.py();
        let raw: String = py
            .import("json")?
            .getattr("dumps")?
            .call1((specifications,))?
            .extract()?;
        let specs: Vec<sts_guided_search::local::ConstructedCombat> =
            serde_json::from_str(&raw).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let core = LocalCombatBatch::from_specifications(
            &specs,
            max_decisions,
            search_services,
            policy_rollout_steps,
        )
        .map_err(PyValueError::new_err)?;
        Ok(Self { core })
    }

    #[new]
    #[pyo3(signature = (cases, max_decisions, *, starting_hp=68, task="starter", search_services=None, policy_rollout_steps=0, deck=None))]
    fn new(
        cases: Vec<(u64, String)>,
        max_decisions: usize,
        starting_hp: i32,
        task: &str,
        search_services: Option<usize>,
        policy_rollout_steps: usize,
        deck: Option<Vec<Bound<'_, PyAny>>>,
    ) -> PyResult<Self> {
        let cases = cases
            .into_iter()
            .map(|(seed, enemy)| {
                Ok((
                    seed,
                    match enemy.as_str() {
                        "JawWorm" => LocalEnemy::JawWorm,
                        "Cultist" => LocalEnemy::Cultist,
                        "TwoLouse" => LocalEnemy::TwoLouse,
                        "ThreeLouse" => LocalEnemy::ThreeLouse,
                        _ => return Err(PyValueError::new_err("unknown local encounter")),
                    },
                ))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let hp = vec![starting_hp; cases.len()];
        let task = match task {
            "starter" => LocalTask::Starter,
            "draw_select" => LocalTask::DrawSelect,
            _ => return Err(PyValueError::new_err("unknown local task")),
        };
        let deck = deck
            .map(|cards| {
                cards
                    .into_iter()
                    .map(|entry| {
                        if !(entry.is_instance_of::<PyTuple>() || entry.is_instance_of::<PyList>())
                            || entry.len()? != 2
                        {
                            return Err(PyValueError::new_err(
                                "deck entry must be a card ID and upgrade pair",
                            ));
                        }
                        let id: String = entry.get_item(0)?.extract()?;
                        let upgrades: u8 = entry.get_item(1)?.extract()?;
                        serde_json::from_value(serde_json::Value::String(id))
                            .map(|card| (card, upgrades))
                            .map_err(|_| PyValueError::new_err("unknown constructed card ID"))
                    })
                    .collect::<PyResult<Vec<_>>>()
            })
            .transpose()?;
        let core = LocalCombatBatch::configured(
            &cases,
            max_decisions,
            &hp,
            task,
            search_services,
            policy_rollout_steps,
            deck.as_deref(),
        )
        .map_err(PyValueError::new_err)?;
        Ok(Self { core })
    }

    fn decision_batch<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        if self.core.task() == LocalTask::Entities {
            return entity_batch(py, self.core.entity_frame().map_err(PyValueError::new_err)?);
        }
        if self.core.task() == LocalTask::DrawSelect {
            return tactical_batch(
                py,
                self.core.tactical_frame().map_err(PyValueError::new_err)?,
            );
        }
        starter_batch(py, self.core.frame().map_err(PyValueError::new_err)?)
    }

    /// Read-only numerical path from the same root and edges already replayed
    /// by the core. Metadata is not a live source or a behavior probability.
    fn demonstration<'py>(&self, py: Python<'py>, slot: usize) -> PyResult<Bound<'py, PyDict>> {
        let (result, choices) = if self.core.task() == LocalTask::DrawSelect {
            let demo = self
                .core
                .tactical_demonstration(slot)
                .map_err(PyValueError::new_err)?;
            let n = demo.positions.len();
            (
                tactical_batch(
                    py,
                    NumericBatch {
                        revision: 0,
                        slots: vec![slot; n],
                        positions: demo.positions,
                    },
                )?,
                demo.choices,
            )
        } else {
            let demo = self
                .core
                .demonstration(slot)
                .map_err(PyValueError::new_err)?;
            let n = demo.positions.len();
            (
                starter_batch(
                    py,
                    NumericBatch {
                        revision: 0,
                        slots: vec![slot; n],
                        positions: demo.positions,
                    },
                )?,
                demo.choices,
            )
        };
        result.del_item("revision")?;
        result.set_item("kind", "replayed_win_demonstration")?;
        result.set_item(
            "choices",
            PyArray1::from_vec(py, choices.into_iter().map(|a| a as u64).collect()),
        )?;
        Ok(result)
    }

    fn advance(&mut self, revision: u64, ordinals: Vec<usize>) -> PyResult<()> {
        self.core
            .advance(revision, &ordinals)
            .map_err(PyValueError::new_err)
    }

    fn guide(
        &mut self,
        revision: u64,
        weights: Vec<Vec<f64>>,
        values: Vec<Option<f64>>,
    ) -> PyResult<()> {
        self.core
            .guide(revision, &weights, &values)
            .map_err(PyValueError::new_err)
    }

    fn outcomes_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.core.outcomes())
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn readings_json(&self) -> PyResult<String> {
        self.core.readings_json().map_err(PyValueError::new_err)
    }

    fn evidence_json(&self) -> PyResult<String> {
        self.core.evidence_json().map_err(PyValueError::new_err)
    }
}

pub(crate) fn entity_batch<'py>(
    py: Python<'py>,
    frame: NumericBatch<sts_guided_search::entities::EntityPosition>,
) -> PyResult<Bound<'py, PyDict>> {
    use sts_guided_search::entities as e;
    let result = PyDict::new(py);
    result.set_item("schema", e::SCHEMA)?;
    result.set_item("revision", frame.revision)?;
    result.set_item(
        "slots",
        PyArray1::from_vec(py, frame.slots.iter().map(|s| *s as u64).collect()),
    )?;
    let mut ids = vec![];
    let mut values = vec![];
    let mut refs = vec![];
    let mut action_ids = vec![];
    let mut actions = vec![];
    let mut action_refs = vec![];
    let mut entity_splits = vec![0u64];
    let mut action_splits = vec![0u64];
    for p in frame.positions {
        ids.extend(p.ids.into_iter().flatten());
        values.extend(p.values.into_iter().flatten());
        refs.extend(p.refs.into_iter().flatten());
        action_ids.extend(p.action_ids.into_iter().flatten());
        actions.extend(p.actions.into_iter().flatten());
        action_refs.extend(p.action_refs.into_iter().flatten());
        entity_splits.push((ids.len() / 2) as u64);
        action_splits.push((action_ids.len() / 2) as u64);
    }
    for (name, width, v) in [
        ("entity_values", e::WIDTH, values),
        ("actions", e::ACTION_WIDTH, actions),
    ] {
        result.set_item(
            name,
            Array2::from_shape_vec((v.len() / width, width), v)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .into_pyarray(py),
        )?;
    }
    for (name, width, v) in [
        ("entity_ids", 2, ids),
        ("entity_refs", 3, refs),
        ("action_ids", 2, action_ids),
        ("action_refs", 3, action_refs),
    ] {
        result.set_item(
            name,
            Array2::from_shape_vec((v.len() / width, width), v)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .into_pyarray(py),
        )?;
    }
    result.set_item("entity_splits", PyArray1::from_vec(py, entity_splits))?;
    result.set_item("action_splits", PyArray1::from_vec(py, action_splits))?;
    Ok(result)
}

fn starter_batch<'py>(
    py: Python<'py>,
    frame: NumericBatch<NumericPosition>,
) -> PyResult<Bound<'py, PyDict>> {
    let n = frame.slots.len();
    let mut context = Vec::with_capacity(n * CONTEXT_WIDTH);
    let mut cards = Vec::with_capacity(n * CARD_COUNT * CARD_WIDTH);
    let mut action_cards = Vec::new();
    let mut splits = vec![0u64];
    for position in frame.positions {
        context.extend_from_slice(&position.context);
        for card in position.cards {
            cards.extend_from_slice(&card);
        }
        action_cards.extend(position.action_cards);
        splits.push(action_cards.len() as u64);
    }
    let result = PyDict::new(py);
    result.set_item("schema", SCHEMA)?;
    result.set_item("revision", frame.revision)?;
    result.set_item(
        "slots",
        PyArray1::from_vec(py, frame.slots.iter().map(|&s| s as u64).collect()),
    )?;
    result.set_item(
        "context",
        Array2::from_shape_vec((n, CONTEXT_WIDTH), context)
            .map_err(|e| PyValueError::new_err(e.to_string()))?
            .into_pyarray(py),
    )?;
    result.set_item(
        "cards",
        Array3::from_shape_vec((n, CARD_COUNT, CARD_WIDTH), cards)
            .map_err(|e| PyValueError::new_err(e.to_string()))?
            .into_pyarray(py),
    )?;
    result.set_item("action_cards", PyArray1::from_vec(py, action_cards))?;
    result.set_item("action_splits", PyArray1::from_vec(py, splits))?;
    Ok(result)
}

fn tactical_batch<'py>(
    py: Python<'py>,
    frame: NumericBatch<sts_guided_search::local_tactics::TacticalPosition>,
) -> PyResult<Bound<'py, PyDict>> {
    use sts_guided_search::local_tactics as t;
    let result = PyDict::new(py);
    result.set_item("schema", t::SCHEMA)?;
    result.set_item("revision", frame.revision)?;
    result.set_item(
        "slots",
        PyArray1::from_vec(py, frame.slots.iter().map(|&s| s as u64).collect()),
    )?;
    let mut context = Vec::new();
    let mut cards = Vec::new();
    let mut monsters = Vec::new();
    let mut actions = Vec::new();
    let mut refs = Vec::new();
    let mut card_splits = vec![0u64];
    let mut monster_splits = vec![0u64];
    let mut action_splits = vec![0u64];
    for p in frame.positions {
        context.extend(p.context);
        cards.extend(p.cards.into_iter().flatten());
        monsters.extend(p.monsters.into_iter().flatten());
        actions.extend(p.actions.into_iter().flatten());
        refs.extend(p.action_refs.into_iter().flatten());
        card_splits.push((cards.len() / t::CARD_WIDTH) as u64);
        monster_splits.push((monsters.len() / t::MONSTER_WIDTH) as u64);
        action_splits.push((actions.len() / t::ACTION_WIDTH) as u64);
    }
    for (name, width, values) in [
        ("context", t::CONTEXT_WIDTH, context),
        ("cards", t::CARD_WIDTH, cards),
        ("monsters", t::MONSTER_WIDTH, monsters),
        ("actions", t::ACTION_WIDTH, actions),
    ] {
        result.set_item(
            name,
            Array2::from_shape_vec((values.len() / width, width), values)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .into_pyarray(py),
        )?;
    }
    result.set_item(
        "action_refs",
        Array2::from_shape_vec((refs.len() / 2, 2), refs)
            .map_err(|e| PyValueError::new_err(e.to_string()))?
            .into_pyarray(py),
    )?;
    for (name, values) in [
        ("card_splits", card_splits),
        ("monster_splits", monster_splits),
        ("action_splits", action_splits),
    ] {
        result.set_item(name, PyArray1::from_vec(py, values))?;
    }
    Ok(result)
}
