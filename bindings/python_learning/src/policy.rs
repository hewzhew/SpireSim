//! Numerical bridge for the finite-search combat actor.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use sts_guided_search::local::{ConstructedCombat, NumericBatch};
use sts_guided_search::policy_search as core;

fn specifications(value: &Bound<'_, PyAny>) -> PyResult<Vec<ConstructedCombat>> {
    let raw: String = value
        .py()
        .import("json")?
        .getattr("dumps")?
        .call1((value,))?
        .extract()?;
    serde_json::from_str(&raw).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyclass]
pub struct PolicySearchBatch {
    core: core::PolicySearchBatch,
}
#[pymethods]
impl PolicySearchBatch {
    #[new]
    fn new(specs: &Bound<'_, PyList>, simulations: usize, max_decisions: usize) -> PyResult<Self> {
        Ok(Self {
            core: core::PolicySearchBatch::new(&specifications(specs)?, simulations, max_decisions)
                .map_err(PyValueError::new_err)?,
        })
    }
    fn evaluation_batch<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        crate::guided::entity_batch(py, self.core.evaluations().map_err(PyValueError::new_err)?)
    }
    fn evaluate(
        &mut self,
        revision: u64,
        weights: Vec<Vec<f64>>,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.core
            .evaluate(revision, &weights, &values)
            .map_err(PyValueError::new_err)
    }
    fn decision_batch<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = self.core.decisions().map_err(PyValueError::new_err)?;
        let result = crate::guided::entity_batch(py, d.frame)?;
        result.set_item("visits", d.visits)?;
        result.set_item("priors", d.priors)?;
        result.set_item("qvalues", d.qvalues)?;
        result.set_item("root_values", d.root_values)?;
        result.set_item("regularization", d.regularization)?;
        result.set_item("terminal_visits", d.terminal_visits)?;
        result.set_item("terminal_values", d.terminal_values)?;
        Ok(result)
    }
    fn advance(&mut self, revision: u64, ordinals: Vec<usize>) -> PyResult<()> {
        self.core
            .advance(revision, &ordinals)
            .map_err(PyValueError::new_err)
    }
    fn outcomes_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.core.outcomes())
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn evidence_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.core.evidence())
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn leaf_rollouts(
        &self,
        revision: u64,
        slots: Vec<usize>,
        simulations: usize,
        max_decisions: usize,
    ) -> PyResult<Self> {
        Ok(Self {
            core: self
                .core
                .leaf_rollouts(revision, &slots, simulations, max_decisions)
                .map_err(PyValueError::new_err)?,
        })
    }
}

#[pyfunction]
#[pyo3(signature = (spec, work, prefix_choices=None))]
pub fn policy_demonstration<'py>(
    py: Python<'py>,
    spec: &Bound<'py, PyDict>,
    work: usize,
    prefix_choices: Option<Vec<usize>>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let list = PyList::new(py, [spec])?;
    let spec = specifications(&list)?.remove(0);
    let Some((positions, choices, value)) =
        core::demonstrate(&spec, work, prefix_choices.as_deref().unwrap_or(&[]))
            .map_err(PyValueError::new_err)?
    else {
        return Ok(None);
    };
    let result = crate::guided::entity_batch(
        py,
        NumericBatch {
            revision: 0,
            slots: vec![0; positions.len()],
            positions,
        },
    )?;
    result.set_item("choices", choices)?;
    result.set_item("value", value)?;
    result.set_item("objective", core::OBJECTIVE)?;
    Ok(Some(result))
}
