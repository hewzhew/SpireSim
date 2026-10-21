use crate::ai::noncombat_decision_v1::{
    render_noncombat_decision_record_validation_errors, validate_noncombat_decision_record_v1,
    CandidateDescriptorV1, DataRoleV1, DecisionSiteKindV1, EvidenceBundleV1, EvidenceItemV1,
    EvidenceKindV1, InformationBoundaryV1, InformationClassV1, NonCombatDecisionRecordV1,
    PolicyProvenanceV1, PolicySelectionStatusV1, PolicySelectionV1, PublicActionPlanV1,
    ValueEstimateV1, NONCOMBAT_DECISION_RECORD_SCHEMA_NAME,
    NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
};
use crate::state::core::EngineState;

use super::session::RunControlSession;
use super::trace_annotation::RunControlTraceAnnotationV1;
use super::view_model::{
    build_run_control_view_model, CandidateAction, CandidateResolution, DecisionCandidate,
    RunControlViewModel,
};

pub(super) fn noncombat_human_boundary_annotation(
    session: &RunControlSession,
    reason: &str,
) -> Result<Option<RunControlTraceAnnotationV1>, String> {
    let Some(record) = build_noncombat_human_boundary_record_v1(session, reason) else {
        return Ok(None);
    };
    validate_noncombat_decision_record_v1(&record).map_err(|errors| {
        format!(
            "human boundary produced invalid NonCombatDecisionRecordV1: {}",
            render_noncombat_decision_record_validation_errors(&errors)
        )
    })?;
    Ok(Some(RunControlTraceAnnotationV1::NonCombatHumanBoundary {
        record,
    }))
}

fn build_noncombat_human_boundary_record_v1(
    session: &RunControlSession,
    reason: &str,
) -> Option<NonCombatDecisionRecordV1> {
    let site = decision_site_kind(session)?;
    let view = build_run_control_view_model(session);
    let candidates = view
        .candidates
        .iter()
        .map(|candidate| candidate_descriptor(site, candidate))
        .collect::<Vec<_>>();
    let mut evidence_items = view
        .candidates
        .iter()
        .map(|candidate| candidate_evidence(site, candidate))
        .collect::<Vec<_>>();
    if matches!(
        site,
        DecisionSiteKindV1::CardReward
            | DecisionSiteKindV1::Campfire
            | DecisionSiteKindV1::Event
            | DecisionSiteKindV1::BossRelic
            | DecisionSiteKindV1::RunChoice
            | DecisionSiteKindV1::Shop
    ) {
        evidence_items.extend(strategy_package_evidence_items(session));
    }
    let allowed_inputs = allowed_information_classes(&view, &evidence_items);

    Some(NonCombatDecisionRecordV1 {
        schema_name: NONCOMBAT_DECISION_RECORD_SCHEMA_NAME.to_string(),
        schema_version: NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
        site,
        data_role: DataRoleV1::HumanBoundaryNotTeacher,
        information_boundary: InformationBoundaryV1::hidden_free(allowed_inputs),
        provenance: PolicyProvenanceV1 {
            source_policy: "run_control_human_boundary_v1".to_string(),
            source_schema_name: "RunControlViewModel".to_string(),
            source_schema_version: 1,
        },
        candidates,
        evidence: EvidenceBundleV1 {
            items: evidence_items,
            assumptions: vec![
                "recorded candidates come from the current run-control public view model"
                    .to_string(),
                "this record marks a human decision boundary and does not choose an action"
                    .to_string(),
            ],
            warnings: view.warnings.clone(),
        },
        values: Vec::<ValueEstimateV1>::new(),
        selection: PolicySelectionV1 {
            status: if view.candidates.is_empty() {
                PolicySelectionStatusV1::NoCandidates
            } else {
                PolicySelectionStatusV1::Stopped
            },
            selected_candidate_id: None,
            reason: reason.to_string(),
            confidence: 0.0,
            selection_mode: "human_required_boundary".to_string(),
        },
    })
}

fn decision_site_kind(session: &RunControlSession) -> Option<DecisionSiteKindV1> {
    match &session.engine_state {
        EngineState::EventRoom => {
            let event = session.run_state.event_state.as_ref()?;
            if event.id == crate::state::events::EventId::Neow {
                Some(DecisionSiteKindV1::Neow)
            } else {
                Some(DecisionSiteKindV1::Event)
            }
        }
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => {
            Some(DecisionSiteKindV1::Map)
        }
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            Some(DecisionSiteKindV1::CardReward)
        }
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_some() =>
        {
            Some(DecisionSiteKindV1::CardReward)
        }
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => {
            Some(DecisionSiteKindV1::Reward)
        }
        EngineState::Shop(_) => Some(DecisionSiteKindV1::Shop),
        EngineState::Campfire => Some(DecisionSiteKindV1::Campfire),
        EngineState::BossRelicSelect(_) => Some(DecisionSiteKindV1::BossRelic),
        EngineState::RunPendingChoice(_) => Some(DecisionSiteKindV1::RunChoice),
        EngineState::TreasureRoom(_)
        | EngineState::DoubleBossComplete
        | EngineState::Act3VictoryComplete
        | EngineState::TrueVictoryComplete
        | EngineState::CombatStart(_)
        | EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_)
        | EngineState::GameOver(_) => None,
    }
}

fn allowed_information_classes(
    view: &RunControlViewModel,
    evidence_items: &[EvidenceItemV1],
) -> Vec<InformationClassV1> {
    let mut classes = vec![InformationClassV1::PublicObservation];
    if view
        .candidates
        .iter()
        .any(|candidate| candidate_resolution_has_known_distribution(candidate.resolution.as_ref()))
    {
        classes.push(InformationClassV1::KnownDistribution);
    }
    if evidence_items
        .iter()
        .any(|item| item.information_class == InformationClassV1::Belief)
        && !classes.contains(&InformationClassV1::Belief)
    {
        classes.push(InformationClassV1::Belief);
    }
    classes
}

fn candidate_resolution_has_known_distribution(resolution: Option<&CandidateResolution>) -> bool {
    resolution.is_some_and(|resolution| !resolution.unresolved_effects.is_empty())
}

fn strategy_package_evidence_items(session: &RunControlSession) -> Vec<EvidenceItemV1> {
    let snapshot = crate::ai::noncombat_strategy_v1::build_run_strategy_snapshot_from_run_state_v2(
        &session.run_state,
    );
    snapshot
        .packages
        .into_iter()
        .map(|package| EvidenceItemV1 {
            kind: EvidenceKindV1::PolicyGate,
            candidate_id: None,
            label: format!("strategy package: {:?}/{:?}", package.domain, package.id),
            information_class: InformationClassV1::Belief,
            components: vec![
                crate::ai::noncombat_decision_v1::ValueComponentV1::new(
                    "support",
                    route_package_support_value(package.support),
                ),
                crate::ai::noncombat_decision_v1::ValueComponentV1::new(
                    "evidence_count",
                    package.evidence.len() as f32,
                ),
                crate::ai::noncombat_decision_v1::ValueComponentV1::new(
                    "risk_count",
                    package.risks.len() as f32,
                ),
            ],
        })
        .collect()
}

fn route_package_support_value(
    support: crate::ai::noncombat_strategy_v1::StrategyPlanSupportV1,
) -> f32 {
    match support {
        crate::ai::noncombat_strategy_v1::StrategyPlanSupportV1::Blocked => 0.0,
        crate::ai::noncombat_strategy_v1::StrategyPlanSupportV1::Weak => 0.25,
        crate::ai::noncombat_strategy_v1::StrategyPlanSupportV1::Plausible => 0.5,
        crate::ai::noncombat_strategy_v1::StrategyPlanSupportV1::Strong => 1.0,
    }
}

fn candidate_descriptor(
    site: DecisionSiteKindV1,
    candidate: &DecisionCandidate,
) -> CandidateDescriptorV1 {
    CandidateDescriptorV1 {
        candidate_id: format!("{}:{}", site_slug(site), candidate.id),
        site,
        label: candidate.label.clone(),
        action_plan: PublicActionPlanV1 {
            summary: candidate.label.clone(),
            command: candidate_command(candidate),
        },
        information_classes: candidate_information_classes(candidate),
        uncertainty_notes: candidate_uncertainty_notes(candidate),
    }
}

fn candidate_command(candidate: &DecisionCandidate) -> Option<String> {
    match &candidate.action {
        CandidateAction::Execute(_) => Some(candidate.action.summary()),
        CandidateAction::Parameterized { template } => Some(template.clone()),
        CandidateAction::Unavailable { .. } => None,
    }
}

fn candidate_information_classes(candidate: &DecisionCandidate) -> Vec<InformationClassV1> {
    let mut classes = vec![InformationClassV1::PublicObservation];
    if candidate_resolution_has_known_distribution(candidate.resolution.as_ref()) {
        classes.push(InformationClassV1::KnownDistribution);
    }
    classes
}

fn candidate_uncertainty_notes(candidate: &DecisionCandidate) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(note) = candidate.note.as_ref() {
        notes.push(note.clone());
    }
    if let CandidateAction::Unavailable { reason } = &candidate.action {
        notes.push(format!("unavailable: {reason}"));
    }
    if let Some(resolution) = candidate.resolution.as_ref() {
        if !resolution.known_effects.is_empty() {
            notes.push(format!("known effects: {:?}", resolution.known_effects));
        }
        if !resolution.unresolved_effects.is_empty() {
            notes.push(format!(
                "known distribution/result hidden until resolved: {:?}",
                resolution.unresolved_effects
            ));
        }
        if let Some(followup) = resolution.followup {
            notes.push(format!("followup boundary: {followup:?}"));
        }
    }
    notes
}

fn candidate_evidence(site: DecisionSiteKindV1, candidate: &DecisionCandidate) -> EvidenceItemV1 {
    let information_class =
        if candidate_resolution_has_known_distribution(candidate.resolution.as_ref()) {
            InformationClassV1::KnownDistribution
        } else {
            InformationClassV1::PublicObservation
        };
    EvidenceItemV1 {
        kind: EvidenceKindV1::CandidateFacts,
        candidate_id: Some(format!("{}:{}", site_slug(site), candidate.id)),
        label: candidate.label.clone(),
        information_class,
        components: Vec::new(),
    }
}

fn site_slug(site: DecisionSiteKindV1) -> &'static str {
    match site {
        DecisionSiteKindV1::Map => "map",
        DecisionSiteKindV1::CardReward => "card_reward",
        DecisionSiteKindV1::Neow => "neow",
        DecisionSiteKindV1::Event => "event",
        DecisionSiteKindV1::Shop => "shop",
        DecisionSiteKindV1::Campfire => "campfire",
        DecisionSiteKindV1::BossRelic => "boss_relic",
        DecisionSiteKindV1::Reward => "reward",
        DecisionSiteKindV1::RunChoice => "run_choice",
    }
}
