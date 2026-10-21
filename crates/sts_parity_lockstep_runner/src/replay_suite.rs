//! Acceptance cases reference existing raw evidence. Coverage is derived only
//! from transitions that the current state and RNG replay has verified exact.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use sts_parity_lockstep::{
    DecisionSurfaceV1, NaturalRunBootstrapV1, ParitySnapshotV1, SemanticActionV1,
};

use crate::OfflineReplayReportV1;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayTaskV1 {
    pub player_class: String,
    pub ascension: u8,
    pub final_act: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayFlowV1 {
    pub before: String,
    pub action: String,
    pub surface: Option<String>,
    pub after: String,
    pub from_act: u64,
    pub to_act: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReplayFlowWitnessV1 {
    pub flow: ReplayFlowV1,
    pub first_action: usize,
    pub occurrences: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReplayBoundaryV1 {
    pub kind: String,
    pub act: u64,
    pub floor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReplayCoverageV1 {
    pub task: ReplayTaskV1,
    pub seed: String,
    pub initial: ReplayBoundaryV1,
    pub end: ReplayBoundaryV1,
    pub starts_at_neow: bool,
    pub complete_heart_run: bool,
    pub flows: Vec<ReplayFlowWitnessV1>,
}

pub(crate) struct ReplayCoverageBuilderV1 {
    report: ReplayCoverageV1,
    flows: BTreeMap<ReplayFlowV1, ReplayFlowWitnessV1>,
    acts: BTreeSet<u64>,
    last_observation: String,
    actions: usize,
}

impl ReplayCoverageBuilderV1 {
    pub(crate) fn new(
        snapshot: &ParitySnapshotV1,
        bootstrap: &NaturalRunBootstrapV1,
    ) -> Result<Self, String> {
        let initial = boundary(snapshot)?;
        Ok(Self {
            report: ReplayCoverageV1 {
                task: ReplayTaskV1 {
                    player_class: bootstrap.player_class.clone(),
                    ascension: bootstrap.ascension,
                    final_act: bootstrap.final_act,
                },
                seed: bootstrap.seed.clone(),
                initial: initial.clone(),
                end: initial.clone(),
                starts_at_neow: crate::act::require_act_entrance(snapshot, 1).is_ok(),
                complete_heart_run: false,
                flows: Vec::new(),
            },
            flows: BTreeMap::new(),
            acts: BTreeSet::from([initial.act]),
            last_observation: snapshot.observation_id.clone(),
            actions: 0,
        })
    }

    pub(crate) fn observe_exact(
        &mut self,
        before: &ParitySnapshotV1,
        action: &SemanticActionV1,
        after: &ParitySnapshotV1,
    ) -> Result<(), String> {
        if before.observation_id != self.last_observation {
            return Err("replay coverage received a discontinuous transition".to_owned());
        }
        let from = boundary(before)?;
        let to = boundary(after)?;
        if to.act < from.act || to.act > from.act + 1 || to.floor < from.floor {
            return Err(
                "replay coverage received a backwards or skipped Act transition".to_owned(),
            );
        }
        let flow = ReplayFlowV1 {
            before: from.kind,
            action: action.kind_name().to_owned(),
            surface: action_surface(action)
                .map(serde_json::to_value)
                .transpose()
                .map_err(|e| e.to_string())?
                .map(|value| {
                    value
                        .as_str()
                        .expect("surface serializes as a string")
                        .to_owned()
                }),
            after: to.kind.clone(),
            from_act: from.act,
            to_act: to.act,
        };
        self.actions += 1;
        let witness = self
            .flows
            .entry(flow.clone())
            .or_insert(ReplayFlowWitnessV1 {
                flow,
                first_action: self.actions,
                occurrences: 0,
            });
        witness.occurrences += 1;
        self.acts.insert(to.act);
        self.report.end = to;
        self.report.complete_heart_run = self.report.starts_at_neow
            && self.report.task.final_act
            && self.acts == BTreeSet::from([1, 2, 3, 4])
            && crate::act::require_act_terminal(after, 4).is_ok();
        self.last_observation = after.observation_id.clone();
        Ok(())
    }

    pub(crate) fn finish(mut self) -> ReplayCoverageV1 {
        self.report.flows = self.flows.into_values().collect();
        self.report
    }
}

fn action_surface(action: &SemanticActionV1) -> Option<DecisionSurfaceV1> {
    match action {
        SemanticActionV1::Select { surface, .. }
        | SemanticActionV1::SubmitSelection { surface, .. }
        | SemanticActionV1::Proceed { surface }
        | SemanticActionV1::Cancel { surface } => Some(*surface),
        SemanticActionV1::PlayCard { .. }
        | SemanticActionV1::UsePotion { .. }
        | SemanticActionV1::DiscardPotion { .. }
        | SemanticActionV1::EndTurn => None,
    }
}

fn boundary(snapshot: &ParitySnapshotV1) -> Result<ReplayBoundaryV1, String> {
    let run = crate::act::run(snapshot)?;
    Ok(ReplayBoundaryV1 {
        kind: snapshot.boundary_kind.clone(),
        act: run.act,
        floor: run.floor,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayCaseRoleV1 {
    Baseline,
    Supplement,
    Regression,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayCaseV1 {
    pub id: String,
    pub purpose: String,
    pub role: ReplayCaseRoleV1,
    pub evidence: PathBuf,
    pub required_flows: Vec<ReplayFlowV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaySuiteV1 {
    pub schema: String,
    pub task: ReplayTaskV1,
    pub cases: Vec<ReplayCaseV1>,
    pub required_flows: Vec<ReplayFlowV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReplayCaseReportV1 {
    pub id: String,
    pub purpose: String,
    pub role: ReplayCaseRoleV1,
    pub evidence: PathBuf,
    pub replay: Option<OfflineReplayReportV1>,
    pub failures: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReplaySuiteFlowV1 {
    pub flow: ReplayFlowV1,
    pub witnesses: Vec<(String, usize)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReplaySuiteReportV1 {
    pub schema: String,
    pub passed: bool,
    pub cases: Vec<ReplayCaseReportV1>,
    pub required_flows: Vec<ReplaySuiteFlowV1>,
}

/// Read-only: each case runs through the existing raw-Java/current-Rust replay.
/// Repeated flows share one row. A supplement never substitutes for a baseline.
pub fn replay_suite_v1(path: &Path) -> Result<ReplaySuiteReportV1, String> {
    let suite: ReplaySuiteV1 = serde_json::from_slice(
        &fs::read(path).map_err(|e| format!("cannot read replay suite: {e}"))?,
    )
    .map_err(|e| format!("invalid replay suite: {e}"))?;
    validate_suite(&suite)?;
    let absolute = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let root = absolute.parent().ok_or("suite path has no parent")?;
    let mut reports = Vec::with_capacity(suite.cases.len());
    let mut sources = BTreeSet::new();
    for case in &suite.cases {
        let unresolved = root.join(&case.evidence);
        let (evidence, result) = match fs::canonicalize(&unresolved) {
            Ok(evidence) => {
                if !sources.insert(evidence.clone()) {
                    return Err(format!("case {} repeats an evidence directory", case.id));
                }
                let result = crate::replay_recorded_evidence_v1(&evidence);
                (evidence, result)
            }
            Err(error) => (unresolved, Err(format!("cannot open evidence: {error}"))),
        };
        let mut failures = Vec::new();
        let replay = match result {
            Ok(replay) => {
                failures = case_failures(&suite.task, case, &replay);
                Some(replay)
            }
            Err(error) => {
                failures.push(error);
                None
            }
        };
        reports.push(ReplayCaseReportV1 {
            id: case.id.clone(),
            purpose: case.purpose.clone(),
            role: case.role,
            evidence,
            replay,
            failures,
        });
    }
    let required_flows = suite
        .required_flows
        .iter()
        .map(|flow| {
            let witnesses = reports
                .iter()
                .filter(|case| case.failures.is_empty())
                .filter_map(|case| {
                    case.replay
                        .as_ref()?
                        .coverage
                        .flows
                        .iter()
                        .find(|w| &w.flow == flow)
                        .map(|w| (case.id.clone(), w.first_action))
                })
                .collect::<Vec<_>>();
            ReplaySuiteFlowV1 {
                flow: flow.clone(),
                witnesses,
            }
        })
        .collect::<Vec<_>>();
    Ok(ReplaySuiteReportV1 {
        schema: "sts-parity-replay-suite-report-v1".to_owned(),
        passed: reports.iter().all(|case| case.failures.is_empty())
            && required_flows.iter().all(|flow| !flow.witnesses.is_empty()),
        cases: reports,
        required_flows,
    })
}

fn case_failures(
    task: &ReplayTaskV1,
    case: &ReplayCaseV1,
    replay: &OfflineReplayReportV1,
) -> Vec<String> {
    let mut failures = Vec::new();
    if &replay.coverage.task != task {
        failures.push("source task differs from the suite task".to_owned());
    }
    if case.role == ReplayCaseRoleV1::Baseline && !replay.coverage.complete_heart_run {
        failures.push(format!(
            "baseline must run from Neow through formal Heart victory; observed {} A{} F{} -> {} A{} F{}",
            replay.coverage.initial.kind, replay.coverage.initial.act, replay.coverage.initial.floor,
            replay.coverage.end.kind, replay.coverage.end.act, replay.coverage.end.floor,
        ));
    }
    for required in &case.required_flows {
        if !replay.coverage.flows.iter().any(|w| &w.flow == required) {
            failures.push(format!(
                "required flow absent: {}",
                serde_json::to_string(required).unwrap()
            ));
        }
    }
    failures
}

fn validate_suite(suite: &ReplaySuiteV1) -> Result<(), String> {
    if suite.schema != "sts-parity-replay-suite-v1" {
        return Err("unsupported replay suite schema".to_owned());
    }
    if suite.task.player_class.is_empty() || suite.task.ascension > 20 || !suite.task.final_act {
        return Err(
            "complete Heart-run suite requires a character, ascension 0..20 and final_act=true"
                .to_owned(),
        );
    }
    if suite.cases.is_empty() || suite.cases.len() > 128 {
        return Err("replay suite requires 1..128 explicit cases".to_owned());
    }
    if !suite
        .cases
        .iter()
        .any(|case| case.role == ReplayCaseRoleV1::Baseline)
    {
        return Err("replay suite requires a complete-run baseline".to_owned());
    }
    let mut ids = BTreeSet::new();
    for case in &suite.cases {
        if case.id.is_empty()
            || case.purpose.trim().is_empty()
            || !case
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !ids.insert(&case.id)
        {
            return Err("replay suite case IDs must be unique lowercase slugs".to_owned());
        }
        validate_flows(&case.required_flows)?;
    }
    validate_flows(&suite.required_flows)
}

fn validate_flows(flows: &[ReplayFlowV1]) -> Result<(), String> {
    if flows.len() > 512 || flows.iter().collect::<BTreeSet<_>>().len() != flows.len() {
        return Err("replay requirements must be distinct and bounded to 512 flows".to_owned());
    }
    for flow in flows {
        if flow.before.is_empty()
            || flow.after.is_empty()
            || !(1..=4).contains(&flow.from_act)
            || !(1..=4).contains(&flow.to_act)
        {
            return Err("replay flow requires boundary names and Acts 1..4".to_owned());
        }
        if ![
            "select",
            "submit_selection",
            "play_card",
            "use_potion",
            "discard_potion",
            "end_turn",
            "proceed",
            "cancel",
        ]
        .contains(&flow.action.as_str())
        {
            return Err(format!("unknown semantic action kind {}", flow.action));
        }
        if let Some(surface) = &flow.surface {
            serde_json::from_value::<DecisionSurfaceV1>(serde_json::Value::String(surface.clone()))
                .map_err(|e| format!("invalid flow surface: {e}"))?;
        }
        let needs_surface =
            ["select", "submit_selection", "proceed", "cancel"].contains(&flow.action.as_str());
        if needs_surface != flow.surface.is_some() {
            return Err(
                "flow surface must match the semantic action's surface ownership".to_owned(),
            );
        }
    }
    Ok(())
}
