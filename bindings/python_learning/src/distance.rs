//! Local numerical estimate transport. No game decisions or proof semantics here.
use pyo3::{exceptions::PyValueError, prelude::*, types::PyDict};
use std::time::{Duration, Instant};
use sts_guided_search::{
    continuation::{ContinuationTaskSpec, ValidatedContinuationTask},
    distance_search::{
        route_neighborhood, DistanceBudget, DistanceOrdering, DistanceSearch, EvaluationToken,
    },
    local::NumericBatch,
    path_search::routes::RouteLimits,
};
use sts_oracle_eval::sim::combat::{CombatPosition, CombatStepLimits};

fn step(seconds: f64) -> PyResult<CombatStepLimits> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(PyValueError::new_err("positive seconds required"));
    }
    let duration = Duration::try_from_secs_f64(seconds)
        .map_err(|_| PyValueError::new_err("invalid duration"))?;
    Ok(CombatStepLimits {
        max_engine_steps: 512,
        deadline: Some(
            Instant::now()
                .checked_add(duration)
                .ok_or_else(|| PyValueError::new_err("deadline overflow"))?,
        ),
    })
}

#[pyclass]
pub struct DistanceCombatSearch {
    core: DistanceSearch,
}

#[pymethods]
impl DistanceCombatSearch {
    #[new]
    #[pyo3(signature = (task_json, horizon, model_revision, batch_size=32, lookahead_inputs=0, advance_forced=false, probe_immediate=false, ordering="total_inputs", probe_forced_boundaries=false, estimate_probe_boundaries=false))]
    fn new(
        task_json: &str,
        horizon: u32,
        model_revision: String,
        batch_size: usize,
        lookahead_inputs: usize,
        advance_forced: bool,
        probe_immediate: bool,
        ordering: &str,
        probe_forced_boundaries: bool,
        estimate_probe_boundaries: bool,
    ) -> PyResult<Self> {
        let ordering = match ordering {
            "total_inputs" => DistanceOrdering::TotalInputs,
            "remaining_inputs" => DistanceOrdering::RemainingInputs,
            _ => {
                return Err(PyValueError::new_err(
                    "ordering must be total_inputs or remaining_inputs",
                ))
            }
        };
        let spec: ContinuationTaskSpec =
            serde_json::from_str(task_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
        if !spec.prefix.is_empty() {
            return Err(PyValueError::new_err(
                "distance search requires a full root",
            ));
        }
        let task =
            ValidatedContinuationTask::new(spec, step(30.)?).map_err(PyValueError::new_err)?;
        Ok(Self {
            core: DistanceSearch::new(
                task.anchor().clone(),
                task.spec().requirement.clone(),
                horizon,
                model_revision,
                batch_size,
            )
            .and_then(|core| core.with_short_lookahead(lookahead_inputs))
            .and_then(|core| core.with_forced_advance(advance_forced))
            .and_then(|core| core.with_immediate_probe(probe_immediate))
            .and_then(|core| core.with_probe_forced_boundaries(probe_forced_boundaries))
            .and_then(|core| core.with_probe_boundary_estimates(estimate_probe_boundaries))
            .and_then(|core| core.with_ordering(ordering))
            .map_err(PyValueError::new_err)?,
        })
    }
    fn advance(
        &mut self,
        work: usize,
        max_nodes: usize,
        max_depth: usize,
        seconds: f64,
    ) -> PyResult<String> {
        self.core
            .advance(DistanceBudget {
                work,
                max_nodes,
                max_depth,
                step: step(seconds)?,
            })
            .map(|s| format!("{s:?}"))
            .map_err(PyValueError::new_err)
    }
    fn evaluations<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let b = self.core.evaluations().map_err(PyValueError::new_err)?;
        let result = crate::guided::entity_batch(
            py,
            NumericBatch {
                revision: b.token.1,
                slots: b.nodes,
                positions: b.positions,
            },
        )?;
        result.set_item("distance_schema", 1)?;
        result.set_item("token", (b.token.0, b.token.1))?;
        result.set_item("kind_count", 18)?;
        Ok(result)
    }
    fn accept_estimates(
        &mut self,
        token: (u64, u64),
        revision: &str,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.core
            .accept_estimates(EvaluationToken(token.0, token.1), revision, &values)
            .map_err(PyValueError::new_err)
    }
    #[pyo3(signature = (include_plans=true))]
    fn report_json(&self, include_plans: bool) -> PyResult<String> {
        let mut report = serde_json::json!({"stats":self.core.stats(),
            "retained_nodes":self.core.retained_nodes(), "model_revision":self.core.model_revision(),
            "ordering":self.core.ordering(),
            "short_check_pending":self.core.short_check_pending(),
            "forced_pending":self.core.forced_pending(),
            "immediate_pending":self.core.immediate_pending()});
        if include_plans {
            report["immediate_checks"] = serde_json::to_value(self.core.immediate_checks())
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            report["short_checks"] = serde_json::to_value(self.core.short_checks())
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            report["plans"] = serde_json::to_value(self.core.plans())
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
        }
        serde_json::to_string(&report).map_err(|e| PyValueError::new_err(e.to_string()))
    }
    #[pyo3(signature = (include_outcomes=false))]
    fn data_json(&self, include_outcomes: bool) -> PyResult<String> {
        let data = if include_outcomes {
            self.core.data_with_outcomes()
        } else {
            self.core.data()
        };
        serde_json::to_string(&data).map_err(|e| PyValueError::new_err(e.to_string()))
    }
}

#[pyfunction]
pub fn distance_route_data(
    task_json: &str,
    inputs_json: &str,
    successor_json: &str,
    horizon: u32,
    seconds: f64,
) -> PyResult<String> {
    let spec: ContinuationTaskSpec =
        serde_json::from_str(task_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    if !spec.prefix.is_empty() {
        return Err(PyValueError::new_err("reference requires a full root"));
    }
    let inputs: Vec<sts_oracle_eval::state::core::ClientInput> =
        serde_json::from_str(inputs_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let successor: CombatPosition =
        serde_json::from_str(successor_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let limits = step(seconds)?;
    let data = route_neighborhood(
        &spec.origin,
        &spec.requirement,
        &inputs,
        &successor,
        horizon,
        RouteLimits {
            max_decisions: 4096,
            step: limits,
        },
    )
    .map_err(PyValueError::new_err)?;
    serde_json::to_string(&data).map_err(|e| PyValueError::new_err(e.to_string()))
}
