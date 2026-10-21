//! Numerical adapter only; the resident Rust task owns execution and targets.
use numpy::ndarray::Array2;
use numpy::IntoPyArray;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use sts_guided_search::segment_learning::{SegmentTask, SegmentTaskSpec, CONTEXT_WIDTH, SCHEMA};

#[pyclass(name = "GuidedSegmentTask")]
pub struct GuidedSegmentTask {
    core: SegmentTask,
}

#[pymethods]
impl GuidedSegmentTask {
    #[new]
    fn new(specification: &Bound<'_, PyDict>, max_actions: usize) -> PyResult<Self> {
        let raw: String = specification
            .py()
            .import("json")?
            .getattr("dumps")?
            .call1((specification,))?
            .extract()?;
        let spec: SegmentTaskSpec =
            serde_json::from_str(&raw).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            core: SegmentTask::new(spec, max_actions).map_err(PyValueError::new_err)?,
        })
    }
    fn decision<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyDict>>> {
        let Some(p) = self.core.decision().map_err(PyValueError::new_err)? else {
            return Ok(None);
        };
        let result = PyDict::new(py);
        result.set_item("segment_schema", SCHEMA)?;
        result.set_item("revision", self.core.source())?;
        result.set_item(
            "combat",
            crate::guided::entity_batch(
                py,
                sts_guided_search::local::NumericBatch {
                    revision: self.core.source(),
                    slots: vec![0],
                    positions: vec![p.combat],
                },
            )?,
        )?;
        for (name, v) in [
            (
                "context_ids",
                p.context_ids.into_iter().flatten().collect::<Vec<_>>(),
            ),
            (
                "context_refs",
                p.context_refs.into_iter().flatten().collect(),
            ),
        ] {
            let width = if name == "context_ids" { 2 } else { 3 };
            result.set_item(
                name,
                Array2::from_shape_vec((v.len() / width, width), v)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?
                    .into_pyarray(py),
            )?;
        }
        let v = p.context_values.into_iter().flatten().collect::<Vec<_>>();
        result.set_item(
            "context_values",
            Array2::from_shape_vec((v.len() / CONTEXT_WIDTH, CONTEXT_WIDTH), v)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .into_pyarray(py),
        )?;
        Ok(Some(result))
    }
    fn advance(&mut self, revision: u64, ordinal: usize) -> PyResult<()> {
        self.core
            .advance(revision, ordinal)
            .map_err(PyValueError::new_err)
    }
    fn attained(&self) -> PyResult<Option<bool>> {
        self.core.target().map_err(PyValueError::new_err)
    }
    fn verify(&self) -> PyResult<()> {
        self.core.verify().map_err(PyValueError::new_err)
    }
    fn evidence_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.core.evidence())
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn reading_json(&self) -> PyResult<String> {
        let e = self.core.episode();
        Ok(serde_json::json!({"view": e.view().map_err(PyValueError::new_err)?, "source":e.source(),
            "stop":e.stop(), "options":e.options().map_err(PyValueError::new_err)?,
            "combat":e.combat_observation().map_err(PyValueError::new_err)?, "results":e.results()}).to_string())
    }
}
