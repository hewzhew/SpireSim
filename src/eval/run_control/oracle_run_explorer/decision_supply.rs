use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::ai::deck_mutation_compiler_v1::{
    compile_deck_mutation_decision_v1, DeckMutationCommitmentModeV1, DeckMutationCompilerOutputV1,
    DeckMutationCompilerRequestV1,
};
use crate::state::core::{ClientInput, EngineState};
use crate::state::selection::{SelectionResolution, SelectionScope, SelectionTargetRef};

use super::super::oracle_selection_cursor::LazyUnorderedSelectionCursorV1;
use super::super::{
    build_decision_surface, positive_ranked_run_policy_prior_v1, DecisionCandidateKey,
    DecisionSurface, RunControlSession, RunDecisionAction, RunPolicyCandidateV1,
    RunPolicyPriorFnV1,
};
use super::{
    LazyOracleRunDecisionV1, LazyOracleRunSelectionFamilyV1, OracleRunBoundaryV1,
    OracleRunBranchV1, OracleRunExplorerV1, OracleRunWorkKindV1,
};

#[derive(Serialize)]
struct StableOracleWorkKeyInput<'a> {
    parent_state_fingerprint: &'a str,
    candidate_id: &'a str,
    action: &'a RunDecisionAction,
}

pub(super) fn stable_oracle_work_key(
    parent_state_fingerprint: &str,
    candidate_id: &str,
    action: &RunDecisionAction,
) -> String {
    crate::eval::fingerprint::hash_serializable(&StableOracleWorkKeyInput {
        parent_state_fingerprint,
        candidate_id,
        action,
    })
}

pub(super) fn selection_family_work_key(
    parent_state_fingerprint: &str,
    candidate_id: &str,
    min_count: usize,
    max_count: usize,
) -> String {
    crate::eval::fingerprint::hash_serializable(&(
        "oracle_run_selection_family_v1",
        parent_state_fingerprint,
        candidate_id,
        min_count,
        max_count,
    ))
}

pub(super) struct OracleRunDecisionSupplyV1 {
    pub(super) decisions: Vec<LazyOracleRunDecisionV1>,
    pub(super) selection_family: Option<LazyOracleRunSelectionFamilyV1>,
}

pub(in super::super) struct SelectionMemberReleasePlanV1 {
    completed_work_key: String,
    family_key: Option<String>,
    next_decision: Option<LazyOracleRunDecisionV1>,
    next_family: Option<LazyOracleRunSelectionFamilyV1>,
}

impl OracleRunExplorerV1 {
    pub(super) fn apply_decision_supply(&mut self, mut supply: OracleRunDecisionSupplyV1) {
        supply.decisions.retain(|item| {
            self.registered_work_keys
                .insert(item.stable_work_key.clone())
        });
        self.pending_decisions.extend(supply.decisions);
        if let Some(family) = supply.selection_family {
            if self.registered_work_keys.insert(family.family_key.clone()) {
                self.pending_selection_families.push_back(family);
            }
        }
    }

    pub(in super::super) fn prepare_selection_member_release(
        &self,
        completed_work_key: &str,
    ) -> Result<SelectionMemberReleasePlanV1, String> {
        let Some(source) = self
            .pending_selection_families
            .iter()
            .find(|family| family.outstanding_work_key.as_deref() == Some(completed_work_key))
        else {
            return Ok(SelectionMemberReleasePlanV1 {
                completed_work_key: completed_work_key.to_string(),
                family_key: None,
                next_decision: None,
                next_family: None,
            });
        };
        let mut family = source.clone();
        family.outstanding_work_key = None;
        let Some(action) = selection_family_next_action(&mut family) else {
            return Ok(SelectionMemberReleasePlanV1 {
                completed_work_key: completed_work_key.to_string(),
                family_key: Some(family.family_key),
                next_decision: None,
                next_family: None,
            });
        };
        let decision = selection_family_decision(&mut family, action)?;
        if self
            .registered_work_keys
            .contains(&decision.stable_work_key)
        {
            return Err(format!(
                "selection family '{}' emitted duplicate exact work '{}'",
                family.family_key, decision.stable_work_key
            ));
        }
        let next_family = (!family.cursor.is_exhausted()).then_some(family.clone());
        Ok(SelectionMemberReleasePlanV1 {
            completed_work_key: completed_work_key.to_string(),
            family_key: Some(family.family_key),
            next_decision: Some(decision),
            next_family,
        })
    }

    pub(in super::super) fn apply_selection_member_release(
        &mut self,
        plan: SelectionMemberReleasePlanV1,
    ) {
        let Some(family_key) = plan.family_key else {
            return;
        };
        let index = self
            .pending_selection_families
            .iter()
            .position(|family| {
                family.family_key == family_key
                    && family.outstanding_work_key.as_deref()
                        == Some(plan.completed_work_key.as_str())
            })
            .expect("prepared selection family must remain unchanged until commit");
        self.pending_selection_families
            .remove(index)
            .expect("prepared selection family must remain present");
        if let Some(decision) = plan.next_decision {
            assert!(
                self.registered_work_keys
                    .insert(decision.stable_work_key.clone()),
                "validated selection work must remain unique until commit"
            );
            self.pending_decisions.push_back(decision);
        }
        if let Some(family) = plan.next_family {
            self.pending_selection_families.push_back(family);
        }
    }
}

pub(super) fn decision_supply_for_branch(
    branch: &OracleRunBranchV1,
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> Result<OracleRunDecisionSupplyV1, String> {
    let kind = work_kind(branch.boundary)?;
    let surface = build_decision_surface(&branch.session);
    let has_symbolic_selection = matches!(
        branch.session.engine_state,
        EngineState::RunPendingChoice(_)
    ) && surface.view.candidates.iter().any(|candidate| {
        matches!(
            candidate.key,
            Some(DecisionCandidateKey::SelectionSubmit { .. })
        ) && candidate.action.executable_action().is_none()
    });
    if has_symbolic_selection {
        let (decision, selection_family) =
            run_choice_family_for_branch(branch, kind, &surface, decision_prior)?;
        return Ok(OracleRunDecisionSupplyV1 {
            decisions: vec![decision],
            selection_family,
        });
    }

    let mut work = Vec::new();
    for candidate in surface.view.candidates {
        let Some(action) = candidate.action.executable_action() else {
            continue;
        };
        if should_normalize_navigation_away(&branch.session, &action) {
            continue;
        }
        work.push(lazy_decision(
            branch,
            kind,
            candidate.id,
            candidate.label,
            action,
        ));
    }
    if work.is_empty() {
        return Err(format!(
            "oracle {:?} branch {} exposed no executable strategic action",
            branch.boundary, branch.branch_id
        ));
    }
    apply_decision_policy(branch, &mut work, decision_prior)?;
    Ok(OracleRunDecisionSupplyV1 {
        decisions: work,
        selection_family: None,
    })
}

pub(super) fn apply_decision_policy(
    branch: &OracleRunBranchV1,
    work: &mut [LazyOracleRunDecisionV1],
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> Result<(), String> {
    let prior = {
        let legal = work
            .iter()
            .map(|candidate| RunPolicyCandidateV1 {
                candidate_id: &candidate.candidate_id,
                label: &candidate.label,
                action: &candidate.action,
            })
            .collect::<Vec<_>>();
        let prior = match decision_prior {
            Some(policy) => policy(&branch.session, &legal)?,
            None => positive_ranked_run_policy_prior_v1(&legal, std::iter::empty())?,
        };
        prior.validate_for(&legal)?;
        prior
    };

    let work_indices = work
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.candidate_id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    for (rank, entry) in prior.entries.into_iter().enumerate() {
        let index = work_indices
            .get(&entry.candidate_id)
            .copied()
            .expect("validated policy prior must reference one legal candidate");
        work[index].path_negative_log_policy =
            branch.path_negative_log_policy - entry.probability.ln();
        work[index].path_discrepancy = branch.path_discrepancy.saturating_add(rank as u64);
        work[index].path_depth = branch.path_depth.saturating_add(1);
    }
    Ok(())
}

const RUN_SELECTION_PREFERRED_PREFIX: usize = 4;

fn run_choice_family_for_branch(
    branch: &OracleRunBranchV1,
    kind: OracleRunWorkKindV1,
    surface: &DecisionSurface,
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> Result<
    (
        LazyOracleRunDecisionV1,
        Option<LazyOracleRunSelectionFamilyV1>,
    ),
    String,
> {
    let EngineState::RunPendingChoice(choice) = &branch.session.engine_state else {
        unreachable!("run choice work requires RunPendingChoice")
    };
    let request = choice.selection_request(&branch.session.run_state);
    let candidate = surface
        .view
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.key,
                Some(DecisionCandidateKey::SelectionSubmit { .. })
            )
        })
        .ok_or_else(|| "run choice has no bindable decision-surface candidate".to_string())?;
    let preferred = preferred_run_choice_selections(branch, choice);
    let cursor = LazyUnorderedSelectionCursorV1::new(
        request.targets,
        choice.min_choices,
        choice.max_choices,
        preferred,
    )?;
    let total_count = cursor.total_count();
    if total_count == 0 {
        return Err("run choice parameterized family contains no legal selections".to_string());
    }

    let family_key = selection_family_work_key(
        &branch.state_fingerprint,
        &candidate.id,
        choice.min_choices,
        choice.max_choices,
    );
    let mut family = LazyOracleRunSelectionFamilyV1 {
        family_key,
        parent_branch_id: branch.branch_id,
        parent_state_fingerprint: branch.state_fingerprint.clone(),
        neow_root_candidate_id: branch.neow_root_candidate_id.clone(),
        kind,
        candidate_id: candidate.id.clone(),
        label: candidate.label.clone(),
        path_negative_log_policy: branch.path_negative_log_policy,
        path_discrepancy: branch.path_discrepancy,
        path_depth: branch.path_depth.saturating_add(1),
        parent_act: branch.session.run_state.act_num,
        parent_floor: branch.session.run_state.floor_num,
        public_probability: 1.0,
        cursor,
        outstanding_work_key: None,
    };
    let first_action = selection_family_next_action(&mut family)
        .ok_or_else(|| "run choice selection cursor did not emit its first member".to_string())?;
    let legal = [RunPolicyCandidateV1 {
        candidate_id: &family.candidate_id,
        label: &family.label,
        action: &first_action,
    }];
    let prior = match decision_prior {
        Some(policy) => policy(&branch.session, &legal)?,
        None => positive_ranked_run_policy_prior_v1(&legal, std::iter::empty())?,
    };
    prior.validate_for(&legal)?;
    family.public_probability = prior.entries[0].probability;
    let first = selection_family_decision(&mut family, first_action)?;
    let remaining_family = (!family.cursor.is_exhausted()).then_some(family);
    Ok((first, remaining_family))
}

fn preferred_run_choice_selections(
    branch: &OracleRunBranchV1,
    choice: &crate::state::core::RunPendingChoiceState,
) -> Vec<Vec<SelectionTargetRef>> {
    let compiled = compile_deck_mutation_decision_v1(
        &branch.session.run_state,
        choice,
        DeckMutationCompilerRequestV1 {
            output: DeckMutationCompilerOutputV1::BranchTopK {
                max_active: RUN_SELECTION_PREFERRED_PREFIX,
            },
            commitment: DeckMutationCommitmentModeV1::CommittedForced,
        },
    );
    let mut seen = BTreeSet::new();
    compiled
        .selected_plan
        .iter()
        .chain(compiled.candidate_plans.iter())
        .filter_map(|plan| {
            let selected = plan
                .step
                .deck_indices
                .iter()
                .map(|index| {
                    branch
                        .session
                        .run_state
                        .master_deck
                        .get(*index)
                        .map(|card| SelectionTargetRef::CardLineage(card.lineage))
                })
                .collect::<Option<Vec<_>>>()?;
            let key = selected
                .iter()
                .map(|target| target.card_lineage())
                .collect::<Vec<_>>();
            seen.insert(key).then_some(selected)
        })
        .take(RUN_SELECTION_PREFERRED_PREFIX)
        .collect()
}

pub(super) fn selection_family_next_action(
    family: &mut LazyOracleRunSelectionFamilyV1,
) -> Option<RunDecisionAction> {
    family.cursor.next_member().map(|member| {
        RunDecisionAction::Input(ClientInput::SubmitSelection(SelectionResolution {
            scope: SelectionScope::Deck,
            selected: member.selected,
        }))
    })
}

pub(super) fn selection_family_decision(
    family: &mut LazyOracleRunSelectionFamilyV1,
    action: RunDecisionAction,
) -> Result<LazyOracleRunDecisionV1, String> {
    let exact_count = family.cursor.total_count() as f64;
    let exact_probability = family.public_probability / exact_count;
    if !exact_probability.is_finite() || exact_probability <= 0.0 {
        return Err(format!(
            "selection family '{}' produced invalid exact probability {exact_probability}",
            family.family_key
        ));
    }
    let rank = family.cursor.emitted_count().saturating_sub(1);
    let stable_work_key = stable_oracle_work_key(
        &family.parent_state_fingerprint,
        &family.candidate_id,
        &action,
    );
    family.outstanding_work_key = Some(stable_work_key.clone());
    Ok(LazyOracleRunDecisionV1 {
        parent_branch_id: family.parent_branch_id,
        parent_state_fingerprint: family.parent_state_fingerprint.clone(),
        neow_root_candidate_id: family.neow_root_candidate_id.clone(),
        kind: family.kind,
        candidate_id: family.candidate_id.clone(),
        label: family.label.clone(),
        action,
        stable_work_key,
        path_negative_log_policy: family.path_negative_log_policy - exact_probability.ln(),
        path_discrepancy: family.path_discrepancy.saturating_add(rank),
        path_depth: family.path_depth,
        parent_act: family.parent_act,
        parent_floor: family.parent_floor,
        combat_edge_probe: None,
    })
}

fn lazy_decision(
    branch: &OracleRunBranchV1,
    kind: OracleRunWorkKindV1,
    candidate_id: String,
    label: String,
    action: RunDecisionAction,
) -> LazyOracleRunDecisionV1 {
    let stable_work_key = stable_oracle_work_key(&branch.state_fingerprint, &candidate_id, &action);
    LazyOracleRunDecisionV1 {
        parent_branch_id: branch.branch_id,
        parent_state_fingerprint: branch.state_fingerprint.clone(),
        neow_root_candidate_id: branch.neow_root_candidate_id.clone(),
        kind,
        candidate_id,
        label,
        action,
        stable_work_key,
        path_negative_log_policy: branch.path_negative_log_policy,
        path_discrepancy: branch.path_discrepancy,
        path_depth: branch.path_depth.saturating_add(1),
        parent_act: branch.session.run_state.act_num,
        parent_floor: branch.session.run_state.floor_num,
        combat_edge_probe: None,
    }
}

fn work_kind(boundary: OracleRunBoundaryV1) -> Result<OracleRunWorkKindV1, String> {
    match boundary {
        OracleRunBoundaryV1::MapDecision => Ok(OracleRunWorkKindV1::MapTravel),
        OracleRunBoundaryV1::Reward => Ok(OracleRunWorkKindV1::RewardAction),
        OracleRunBoundaryV1::Event => Ok(OracleRunWorkKindV1::EventOption),
        OracleRunBoundaryV1::Shop => Ok(OracleRunWorkKindV1::ShopAction),
        OracleRunBoundaryV1::Campfire => Ok(OracleRunWorkKindV1::CampfireAction),
        OracleRunBoundaryV1::RunChoice => Ok(OracleRunWorkKindV1::RunChoice),
        OracleRunBoundaryV1::Treasure => Ok(OracleRunWorkKindV1::TreasureAction),
        OracleRunBoundaryV1::BossRelic => Ok(OracleRunWorkKindV1::BossRelicChoice),
        unsupported => Err(format!(
            "oracle boundary {unsupported:?} does not own a noncombat action surface"
        )),
    }
}

fn should_normalize_navigation_away(
    session: &RunControlSession,
    action: &RunDecisionAction,
) -> bool {
    if !matches!(action, RunDecisionAction::Input(ClientInput::Cancel)) {
        return false;
    }
    matches!(
        session.engine_state,
        EngineState::RewardScreen(ref reward) if reward.pending_card_choice.is_some()
    ) || matches!(
        session.engine_state,
        EngineState::RewardOverlay {
            ref reward_state,
            ..
        } if reward_state.pending_card_choice.is_some()
    )
}
