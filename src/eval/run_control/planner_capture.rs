use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ai::planner_core::{
    stable_planner_id, CandidateRepresentationGap, CandidateSetCompleteness, LegalCandidateSet,
    PlannerBehaviorEvent, PlannerDecisionSite, PlannerObservation, PlannerOutcomeAttachment,
    PlannerOutcomeHorizon,
};

use super::session_trace::SessionTraceV1;
use super::trace_annotation::RunControlTraceAnnotationV1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerCaptureDataset {
    pub schema_name: String,
    pub schema_version: u32,
    pub source_trace_schema_name: String,
    pub source_trace_schema_version: u32,
    pub observations: Vec<PlannerObservation>,
    pub legal_candidate_sets: Vec<LegalCandidateSet>,
    pub behavior_events: Vec<PlannerBehaviorEvent>,
    pub outcomes: Vec<PlannerOutcomeAttachment>,
}

pub fn build_planner_capture_dataset(trace: &SessionTraceV1) -> PlannerCaptureDataset {
    PlannerCaptureDataset {
        schema_name: "PlannerCaptureDataset".to_string(),
        schema_version: 1,
        source_trace_schema_name: trace.schema_name.clone(),
        source_trace_schema_version: trace.schema_version,
        observations: trace.planner_observations.clone(),
        legal_candidate_sets: trace.planner_legal_candidate_sets.clone(),
        behavior_events: planner_behavior_events(trace).cloned().collect(),
        outcomes: trace.planner_outcome_attachments.clone(),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerCaptureCoverageReport {
    pub schema_name: String,
    pub schema_version: u32,
    pub total_captures: usize,
    pub total_outcomes: usize,
    pub unresolved_payload_references: usize,
    pub sites: Vec<PlannerDecisionSiteCoverage>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerDecisionSiteCoverage {
    pub site: PlannerDecisionSite,
    pub captures: usize,
    pub complete_candidate_sets: usize,
    pub incomplete_candidate_sets: usize,
    pub selected_candidates_linked: usize,
    pub after_one_floor_outcomes: usize,
    pub terminal_outcomes: usize,
    pub representation_gaps: Vec<CandidateRepresentationGap>,
}

pub fn build_planner_capture_coverage_report(
    trace: &SessionTraceV1,
) -> PlannerCaptureCoverageReport {
    let events = planner_behavior_events(trace).collect::<Vec<_>>();
    let observations = trace
        .planner_observations
        .iter()
        .map(|observation| (observation.observation_id.as_str(), observation))
        .collect::<BTreeMap<_, _>>();
    let candidate_sets = trace
        .planner_legal_candidate_sets
        .iter()
        .map(|set| (set.candidate_set_id.as_str(), set))
        .collect::<BTreeMap<_, _>>();
    let mut rows = BTreeMap::<String, PlannerDecisionSiteCoverage>::new();
    let mut unresolved_payload_references = 0;
    let mut site_by_behavior = BTreeMap::new();
    for event in &events {
        let Some(observation) = observations.get(event.behavior.observation_id.as_str()) else {
            unresolved_payload_references += 1;
            continue;
        };
        let Some(candidate_set) =
            candidate_sets.get(event.behavior.legal_candidate_set_id.as_str())
        else {
            unresolved_payload_references += 1;
            continue;
        };
        let key = format!("{:?}", observation.decision_site);
        let row = rows
            .entry(key.clone())
            .or_insert_with(|| PlannerDecisionSiteCoverage {
                site: observation.decision_site,
                captures: 0,
                complete_candidate_sets: 0,
                incomplete_candidate_sets: 0,
                selected_candidates_linked: 0,
                after_one_floor_outcomes: 0,
                terminal_outcomes: 0,
                representation_gaps: Vec::new(),
            });
        row.captures += 1;
        match &candidate_set.completeness {
            CandidateSetCompleteness::Complete { .. } => row.complete_candidate_sets += 1,
            CandidateSetCompleteness::Incomplete { gaps, .. } => {
                row.incomplete_candidate_sets += 1;
                row.representation_gaps.extend(gaps.iter().cloned());
            }
        }
        if candidate_set
            .candidates
            .iter()
            .any(|candidate| candidate.candidate_id == event.behavior.selected_candidate_id)
        {
            row.selected_candidates_linked += 1;
        }
        site_by_behavior.insert(event.behavior.behavior_id.as_str(), key);
    }
    for outcome in &trace.planner_outcome_attachments {
        let Some(site_key) = site_by_behavior.get(outcome.behavior_id.as_str()) else {
            continue;
        };
        let Some(row) = rows.get_mut(site_key) else {
            continue;
        };
        match outcome.horizon {
            PlannerOutcomeHorizon::AfterOneFloor => row.after_one_floor_outcomes += 1,
            PlannerOutcomeHorizon::RunTerminal => row.terminal_outcomes += 1,
        }
    }
    let mut sites = rows.into_values().collect::<Vec<_>>();
    for row in &mut sites {
        row.representation_gaps.sort();
        row.representation_gaps.dedup();
    }
    PlannerCaptureCoverageReport {
        schema_name: "PlannerCaptureCoverageReport".to_string(),
        schema_version: 1,
        total_captures: events.len(),
        total_outcomes: trace.planner_outcome_attachments.len(),
        unresolved_payload_references,
        sites,
    }
}

pub(super) fn validate_planner_trace_payloads(trace: &SessionTraceV1) -> Result<(), String> {
    let mut observation_ids = BTreeSet::new();
    for observation in &trace.planner_observations {
        let mut payload = observation.clone();
        payload.observation_id.clear();
        if observation.observation_id != stable_planner_id("observation", &payload)? {
            return Err("planner observation id does not match its payload".to_string());
        }
        if !observation_ids.insert(observation.observation_id.as_str()) {
            return Err("duplicate planner observation id".to_string());
        }
    }
    let mut candidate_set_ids = BTreeSet::new();
    for set in &trace.planner_legal_candidate_sets {
        for candidate in &set.candidates {
            if candidate.candidate_id != stable_planner_id("candidate", &candidate.action)? {
                return Err("planner candidate id does not match its typed action".to_string());
            }
        }
        let mut payload = set.clone();
        payload.candidate_set_id.clear();
        if set.candidate_set_id != stable_planner_id("candidate_set", &payload)? {
            return Err("planner candidate set id does not match its payload".to_string());
        }
        if !candidate_set_ids.insert(set.candidate_set_id.as_str()) {
            return Err("duplicate planner candidate set id".to_string());
        }
    }
    for event in planner_behavior_events(trace) {
        let behavior = &event.behavior;
        if !observation_ids.contains(behavior.observation_id.as_str()) {
            return Err("planner behavior references a missing observation".to_string());
        }
        let Some(set) = trace
            .planner_legal_candidate_sets
            .iter()
            .find(|set| set.candidate_set_id == behavior.legal_candidate_set_id)
        else {
            return Err("planner behavior references a missing candidate set".to_string());
        };
        if set.observation_id != behavior.observation_id || set.decision_id != behavior.decision_id
        {
            return Err("planner behavior payload references disagree".to_string());
        }
        if !set
            .candidates
            .iter()
            .any(|candidate| candidate.candidate_id == behavior.selected_candidate_id)
        {
            return Err("planner behavior selected candidate is absent".to_string());
        }
    }
    Ok(())
}

fn planner_behavior_events(trace: &SessionTraceV1) -> impl Iterator<Item = &PlannerBehaviorEvent> {
    trace
        .steps
        .iter()
        .flat_map(|step| step.annotations.iter())
        .chain(
            trace
                .boundary_records
                .iter()
                .flat_map(|boundary| boundary.annotations.iter()),
        )
        .filter_map(|annotation| match annotation {
            RunControlTraceAnnotationV1::PlannerBehaviorDecision { event } => Some(event),
            _ => None,
        })
}
