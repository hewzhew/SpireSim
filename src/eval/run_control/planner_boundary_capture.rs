use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ai::planner_core::{
    stable_planner_id, CandidateCompletenessBasis, CandidateRepresentationGap,
    CandidateSetCompleteness, LegalCandidate, LegalCandidateSet, PlannerAction,
    PlannerCardObservation, PlannerDecisionContext, PlannerDecisionSite, PlannerMechanicsManifest,
    PlannerObservation, PlannerOfferedCard, PlannerPlayerClass, PlannerPotionObservation,
    PlannerPotionSlotObservation, PlannerPublicHistory, PlannerRelicObservation,
    PlannerRewardDescriptor, PlannerRunGoal, PlannerRunScalars, PlannerShopCardOfferV1,
    PlannerShopInventoryV1, PlannerShopPotionOfferV1, PlannerShopPurchaseBlockReasonV1,
    PlannerShopPurchaseStatusV1, PlannerShopPurgeOfferV1, PlannerShopRelicOfferV1,
    LEGAL_CANDIDATE_SET_SCHEMA_NAME, LEGAL_CANDIDATE_SET_SCHEMA_VERSION, PLANNER_MECHANICS_ID,
    PLANNER_MECHANICS_VERSION, PLANNER_OBSERVATION_SCHEMA_NAME, PLANNER_OBSERVATION_SCHEMA_VERSION,
};
use crate::runtime::combat::CardLineageId;
use crate::state::core::{CampfireChoice, ClientInput, EngineState};
use crate::state::rewards::{RewardItem, RewardState};

use super::view_model::{CandidateAction, DecisionCandidate, DecisionCandidateKey};
use super::{
    build_decision_surface, planner_public_map_v1, RunControlSession, RunDecisionAction,
    RunDecisionBoundaryV1, RunDecisionSelectionSourceV1, RunProgressStepV1,
};

pub const PLANNER_BOUNDARY_CAPTURE_SEGMENT_SCHEMA_NAME: &str = "PlannerBoundaryCaptureSegment";
pub const PLANNER_BOUNDARY_CAPTURE_SEGMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerBoundaryYieldKindV1 {
    CallbackStop,
    ProgressBudgetExhausted,
    WallDeadlineReached,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerBoundaryMutationKindV1 {
    ForcedTransition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerBoundaryVisitOutcomeV1 {
    Selected {
        selection_source: RunDecisionSelectionSourceV1,
        run_candidate_id: String,
        planner_candidate_id: String,
    },
    SelectionNotRepresented {
        selection_source: RunDecisionSelectionSourceV1,
        run_candidate_id: String,
    },
    Yielded {
        yield_kind: PlannerBoundaryYieldKindV1,
    },
    MutationWithoutSelection {
        mutation_kind: PlannerBoundaryMutationKindV1,
    },
    ExecutionFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerBoundaryCandidateLinkV1 {
    pub run_candidate_id: String,
    pub planner_candidate_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerBoundaryVisitV1 {
    pub visit_id: String,
    pub decision_step: u64,
    pub observation: PlannerObservation,
    pub legal_candidate_set: LegalCandidateSet,
    pub candidate_links: Vec<PlannerBoundaryCandidateLinkV1>,
    pub outcome: PlannerBoundaryVisitOutcomeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerBoundaryCaptureSegmentV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub visits: Vec<PlannerBoundaryVisitV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerBoundaryCaptureCoverageReportV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub total_visits: usize,
    pub selected_visits: usize,
    pub yielded_visits: usize,
    pub mutation_without_selection_visits: usize,
    pub unrepresented_selection_visits: usize,
    pub execution_failed_visits: usize,
    pub duplicate_visit_ids: usize,
    pub sites: Vec<PlannerBoundarySiteCoverageV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerBoundarySiteCoverageV1 {
    pub site: PlannerDecisionSite,
    pub visits: usize,
    pub selected_candidates_linked: usize,
    pub complete_candidate_sets: usize,
    pub incomplete_candidate_sets: usize,
    pub representation_gaps: Vec<CandidateRepresentationGap>,
}

#[derive(Clone, Debug)]
pub struct PlannerBoundaryCaptureTicketV1 {
    pending: PendingPlannerBoundaryCaptureV1,
}

impl PlannerBoundaryCaptureTicketV1 {
    pub fn finish_for_progress(
        self,
        progress_steps: &[RunProgressStepV1],
    ) -> PlannerBoundaryCaptureSegmentV1 {
        let mut segment = PlannerBoundaryCaptureSegmentV1::default();
        segment.push(
            self.pending
                .finish_for_progress(progress_steps, PlannerBoundaryYieldKindV1::CallbackStop),
        );
        segment
    }

    pub fn finish_failed(self) -> PlannerBoundaryCaptureSegmentV1 {
        let mut segment = PlannerBoundaryCaptureSegmentV1::default();
        segment.push(
            self.pending
                .finish(PlannerBoundaryVisitOutcomeV1::ExecutionFailed),
        );
        segment
    }
}

impl Default for PlannerBoundaryCaptureSegmentV1 {
    fn default() -> Self {
        Self {
            schema_name: PLANNER_BOUNDARY_CAPTURE_SEGMENT_SCHEMA_NAME.to_string(),
            schema_version: PLANNER_BOUNDARY_CAPTURE_SEGMENT_SCHEMA_VERSION,
            visits: Vec::new(),
        }
    }
}

impl PlannerBoundaryCaptureSegmentV1 {
    pub fn append(&mut self, mut other: Self) {
        self.visits.append(&mut other.visits);
    }

    pub fn push(&mut self, visit: PlannerBoundaryVisitV1) {
        self.visits.push(visit);
    }

    pub fn is_empty(&self) -> bool {
        self.visits.is_empty()
    }
}

pub fn build_planner_boundary_capture_coverage_report_v1(
    segment: &PlannerBoundaryCaptureSegmentV1,
) -> PlannerBoundaryCaptureCoverageReportV1 {
    use std::collections::BTreeMap;

    let mut seen = BTreeSet::new();
    let mut duplicate_visit_ids = 0;
    let mut selected_visits = 0;
    let mut yielded_visits = 0;
    let mut mutation_without_selection_visits = 0;
    let mut unrepresented_selection_visits = 0;
    let mut execution_failed_visits = 0;
    let mut sites = BTreeMap::<String, PlannerBoundarySiteCoverageV1>::new();
    for visit in &segment.visits {
        if !seen.insert(visit.visit_id.as_str()) {
            duplicate_visit_ids += 1;
            continue;
        }
        let row = sites
            .entry(format!("{:?}", visit.observation.decision_site))
            .or_insert_with(|| PlannerBoundarySiteCoverageV1 {
                site: visit.observation.decision_site,
                visits: 0,
                selected_candidates_linked: 0,
                complete_candidate_sets: 0,
                incomplete_candidate_sets: 0,
                representation_gaps: Vec::new(),
            });
        row.visits += 1;
        match &visit.legal_candidate_set.completeness {
            CandidateSetCompleteness::Complete { .. } => row.complete_candidate_sets += 1,
            CandidateSetCompleteness::Incomplete { gaps, .. } => {
                row.incomplete_candidate_sets += 1;
                row.representation_gaps.extend(gaps.iter().cloned());
            }
        }
        match &visit.outcome {
            PlannerBoundaryVisitOutcomeV1::Selected {
                run_candidate_id,
                planner_candidate_id,
                ..
            } => {
                selected_visits += 1;
                let candidate_is_present = visit
                    .legal_candidate_set
                    .candidates
                    .iter()
                    .any(|candidate| candidate.candidate_id == *planner_candidate_id);
                let link_is_present = visit.candidate_links.iter().any(|link| {
                    link.run_candidate_id == *run_candidate_id
                        && link.planner_candidate_id == *planner_candidate_id
                });
                if candidate_is_present && link_is_present {
                    row.selected_candidates_linked += 1;
                }
            }
            PlannerBoundaryVisitOutcomeV1::SelectionNotRepresented { .. } => {
                unrepresented_selection_visits += 1;
            }
            PlannerBoundaryVisitOutcomeV1::Yielded { .. } => yielded_visits += 1,
            PlannerBoundaryVisitOutcomeV1::MutationWithoutSelection { .. } => {
                mutation_without_selection_visits += 1;
            }
            PlannerBoundaryVisitOutcomeV1::ExecutionFailed => execution_failed_visits += 1,
        }
    }
    let mut sites = sites.into_values().collect::<Vec<_>>();
    for row in &mut sites {
        row.representation_gaps.sort();
        row.representation_gaps.dedup();
    }
    PlannerBoundaryCaptureCoverageReportV1 {
        schema_name: "PlannerBoundaryCaptureCoverageReport".to_string(),
        schema_version: 1,
        total_visits: seen.len(),
        selected_visits,
        yielded_visits,
        mutation_without_selection_visits,
        unrepresented_selection_visits,
        execution_failed_visits,
        duplicate_visit_ids,
        sites,
    }
}

#[derive(Clone, Debug)]
pub(in crate::eval::run_control) struct PendingPlannerBoundaryCaptureV1 {
    visit_id: String,
    decision_step: u64,
    observation: PlannerObservation,
    legal_candidate_set: LegalCandidateSet,
    candidate_links: Vec<PlannerBoundaryCandidateLinkV1>,
    run_boundary: RunDecisionBoundaryV1,
}

impl PendingPlannerBoundaryCaptureV1 {
    pub(in crate::eval::run_control) fn finish(
        self,
        outcome: PlannerBoundaryVisitOutcomeV1,
    ) -> PlannerBoundaryVisitV1 {
        PlannerBoundaryVisitV1 {
            visit_id: self.visit_id,
            decision_step: self.decision_step,
            observation: self.observation,
            legal_candidate_set: self.legal_candidate_set,
            candidate_links: self.candidate_links,
            outcome,
        }
    }

    pub(in crate::eval::run_control) fn finish_for_progress(
        self,
        progress_steps: &[RunProgressStepV1],
        yield_kind: PlannerBoundaryYieldKindV1,
    ) -> PlannerBoundaryVisitV1 {
        let outcome = match progress_steps {
            [RunProgressStepV1::Decision(transaction)]
                if transaction.before == self.run_boundary =>
            {
                let run_candidate_id = transaction.selection.candidate_id.clone();
                match self
                    .candidate_links
                    .iter()
                    .find(|link| link.run_candidate_id == run_candidate_id)
                {
                    Some(link) => PlannerBoundaryVisitOutcomeV1::Selected {
                        selection_source: transaction.selection.source,
                        run_candidate_id,
                        planner_candidate_id: link.planner_candidate_id.clone(),
                    },
                    None => PlannerBoundaryVisitOutcomeV1::SelectionNotRepresented {
                        selection_source: transaction.selection.source,
                        run_candidate_id,
                    },
                }
            }
            [RunProgressStepV1::Decision(_)] => PlannerBoundaryVisitOutcomeV1::ExecutionFailed,
            [RunProgressStepV1::ForcedTransition(transition)]
                if transition.before == self.run_boundary =>
            {
                PlannerBoundaryVisitOutcomeV1::MutationWithoutSelection {
                    mutation_kind: PlannerBoundaryMutationKindV1::ForcedTransition,
                }
            }
            [RunProgressStepV1::ForcedTransition(_)] => {
                PlannerBoundaryVisitOutcomeV1::ExecutionFailed
            }
            [] => PlannerBoundaryVisitOutcomeV1::Yielded { yield_kind },
            _ => PlannerBoundaryVisitOutcomeV1::ExecutionFailed,
        };
        self.finish(outcome)
    }
}

pub(in crate::eval::run_control) fn capture_planner_boundary_v1(
    session: &RunControlSession,
) -> Result<Option<PendingPlannerBoundaryCaptureV1>, String> {
    let Some((site, context)) = planner_site_and_context(session)? else {
        return Ok(None);
    };
    let mechanics = planner_mechanics();
    let mut observation = PlannerObservation {
        schema_name: PLANNER_OBSERVATION_SCHEMA_NAME.to_string(),
        schema_version: PLANNER_OBSERVATION_SCHEMA_VERSION,
        observation_id: String::new(),
        mechanics: mechanics.clone(),
        run_goal: if session.run_state.is_final_act_available {
            PlannerRunGoal::HeartVictory
        } else {
            PlannerRunGoal::ActThreeVictory
        },
        decision_site: site,
        run: PlannerRunScalars {
            player_class: planner_player_class(session.run_state.player_class)?,
            ascension_level: session.run_state.ascension_level,
            act: session.run_state.act_num,
            floor: session.run_state.floor_num,
            current_hp: session.run_state.current_hp,
            max_hp: session.run_state.max_hp,
            gold: session.run_state.gold,
            keys: session.run_state.keys,
            potion_capacity: session.run_state.potions.len(),
            pending_act_entry: if session.run_state.act_num <= 2
                && (session.run_state.pending_boss_reward
                    || session.run_state.pending_boss_act_transition
                    || matches!(session.engine_state, EngineState::BossRelicSelect(_)))
            {
                Some(session.run_state.act_num + 1)
            } else if matches!(session.engine_state, EngineState::Act3VictoryComplete)
                || (session.run_state.act_num == 3
                    && session.run_state.is_final_act_available
                    && session.run_state.keys.iter().all(|key| *key)
                    && matches!(
                        context,
                        PlannerDecisionContext::Event {
                            event: crate::state::events::EventId::SpireHeart,
                            ..
                        }
                    ))
            {
                Some(4)
            } else {
                None
            },
        },
        cards: session
            .run_state
            .master_deck
            .iter()
            .map(|card| PlannerCardObservation {
                card_lineage: card.lineage,
                card: card.id,
                upgrades: card.upgrades,
                misc_value: card.misc_value,
                base_damage_override: card.base_damage_override,
                base_block_override: card.base_block_override,
                cost_modifier: card.cost_modifier,
            })
            .collect(),
        relics: session
            .run_state
            .relics
            .iter()
            .map(|relic| PlannerRelicObservation {
                relic: relic.id,
                counter: relic.counter,
                used_up: relic.used_up,
                amount: relic.amount,
            })
            .collect(),
        potions: session
            .run_state
            .potions
            .iter()
            .enumerate()
            .map(|(slot, potion)| PlannerPotionSlotObservation {
                slot,
                potion: potion.as_ref().map(|potion| PlannerPotionObservation {
                    potion: potion.id,
                    potion_uuid: potion.uuid,
                    can_use: potion.can_use,
                    can_discard: potion.can_discard,
                    requires_target: potion.requires_target,
                }),
            })
            .collect(),
        public_map: planner_public_map_v1(session),
        context,
        public_history: PlannerPublicHistory {
            shop_purge_count: session.run_state.shop_purge_count,
            recent_combat_hp_loss: session
                .recent_combat_attrition()
                .map(|attrition| attrition.raw_hp_loss),
            recent_combat_turns: session
                .recent_combat_attrition()
                .map(|attrition| attrition.turns),
            recent_combat_potions_used: session
                .recent_combat_attrition()
                .map(|attrition| attrition.potions_used),
        },
    };
    observation.observation_id = stable_planner_id("observation", &observation)?;

    let surface = build_decision_surface(session);
    let run_boundary = RunDecisionBoundaryV1::capture(session);
    let mut candidates = Vec::new();
    let mut candidate_links = Vec::new();
    let mut gaps = BTreeSet::new();
    for candidate in &surface.view.candidates {
        match planner_action_for_candidate(session, site, candidate)? {
            CandidateProjection::Legal(action) => {
                let candidate_id = stable_planner_id("candidate", &action)?;
                if candidates
                    .iter()
                    .any(|existing: &LegalCandidate| existing.candidate_id == candidate_id)
                {
                    gaps.insert(CandidateRepresentationGap::DuplicateTypedIdentity);
                    continue;
                }
                candidate_links.push(PlannerBoundaryCandidateLinkV1 {
                    run_candidate_id: candidate.id.clone(),
                    planner_candidate_id: candidate_id.clone(),
                });
                candidates.push(LegalCandidate {
                    candidate_id,
                    action,
                    mechanics: mechanics.clone(),
                });
            }
            CandidateProjection::Gap(gap) => {
                gaps.insert(gap);
            }
            CandidateProjection::Unavailable => {}
        }
    }
    let visible_legal_count = surface
        .view
        .candidates
        .iter()
        .filter(|candidate| !matches!(candidate.action, CandidateAction::Unavailable { .. }))
        .count();
    if visible_legal_count > 0 && candidates.is_empty() {
        gaps.insert(CandidateRepresentationGap::NoRepresentedLegalCandidate);
    }
    let basis = CandidateCompletenessBasis::RunControlBoundaryEnumerator;
    let completeness = if gaps.is_empty() {
        CandidateSetCompleteness::Complete { basis }
    } else {
        CandidateSetCompleteness::Incomplete {
            basis,
            gaps: gaps.into_iter().collect(),
        }
    };
    let decision_id = stable_planner_id(
        "decision",
        &(
            session.decision_step,
            observation.observation_id.as_str(),
            site,
        ),
    )?;
    let mut legal_candidate_set = LegalCandidateSet {
        schema_name: LEGAL_CANDIDATE_SET_SCHEMA_NAME.to_string(),
        schema_version: LEGAL_CANDIDATE_SET_SCHEMA_VERSION,
        candidate_set_id: String::new(),
        decision_id,
        observation_id: observation.observation_id.clone(),
        site,
        candidates,
        completeness,
    };
    legal_candidate_set.candidate_set_id =
        stable_planner_id("candidate_set", &legal_candidate_set)?;
    let visit_id = stable_planner_id(
        "boundary_visit",
        &(
            session.decision_step,
            observation.observation_id.as_str(),
            legal_candidate_set.candidate_set_id.as_str(),
        ),
    )?;

    Ok(Some(PendingPlannerBoundaryCaptureV1 {
        visit_id,
        decision_step: session.decision_step,
        observation,
        legal_candidate_set,
        candidate_links,
        run_boundary,
    }))
}

pub fn capture_planner_boundary_ticket_v1(
    session: &RunControlSession,
) -> Result<Option<PlannerBoundaryCaptureTicketV1>, String> {
    Ok(capture_planner_boundary_v1(session)?
        .map(|pending| PlannerBoundaryCaptureTicketV1 { pending }))
}

pub fn capture_planner_boundary_yield_v1(
    session: &RunControlSession,
    yield_kind: PlannerBoundaryYieldKindV1,
) -> Result<PlannerBoundaryCaptureSegmentV1, String> {
    let mut segment = PlannerBoundaryCaptureSegmentV1::default();
    if let Some(pending) = capture_planner_boundary_v1(session)? {
        segment.push(pending.finish(PlannerBoundaryVisitOutcomeV1::Yielded { yield_kind }));
    }
    Ok(segment)
}

enum CandidateProjection {
    Legal(PlannerAction),
    Gap(CandidateRepresentationGap),
    Unavailable,
}

fn planner_action_for_candidate(
    session: &RunControlSession,
    site: PlannerDecisionSite,
    candidate: &DecisionCandidate,
) -> Result<CandidateProjection, String> {
    if matches!(candidate.action, CandidateAction::Unavailable { .. }) {
        return Ok(CandidateProjection::Unavailable);
    }
    if matches!(candidate.action, CandidateAction::Parameterized { .. }) {
        if let Some(DecisionCandidateKey::SelectionSubmit {
            scope,
            reason,
            min_choices,
            max_choices,
            item_count,
        }) = candidate.key.as_ref()
        {
            if !matches!(session.engine_state, EngineState::RunPendingChoice(_)) {
                return Ok(CandidateProjection::Gap(
                    CandidateRepresentationGap::ParameterizedActionFamily,
                ));
            }
            let Some(surface) = super::selection_surface::active_selection_surface(session) else {
                return Ok(CandidateProjection::Gap(
                    CandidateRepresentationGap::UnsupportedBoundaryAction,
                ));
            };
            if surface.scope != *scope
                || surface.reason != *reason
                || surface.min_choices != *min_choices
                || surface.max_choices != *max_choices
                || surface.item_count != *item_count
                || surface.items.len() != *item_count
            {
                return Ok(CandidateProjection::Gap(
                    CandidateRepresentationGap::UnsupportedBoundaryAction,
                ));
            }
            return Ok(CandidateProjection::Legal(
                PlannerAction::BeginRunCardSelection {
                    scope: *scope,
                    reason: *reason,
                    min_choices: *min_choices,
                    max_choices: *max_choices,
                    selectable_card_lineages: surface
                        .items
                        .iter()
                        .map(|item| item.target.card_lineage())
                        .collect(),
                },
            ));
        }
        return Ok(CandidateProjection::Gap(
            CandidateRepresentationGap::ParameterizedActionFamily,
        ));
    }
    let action = match candidate.key.as_ref() {
        Some(DecisionCandidateKey::EventOption {
            event_id,
            screen,
            option_index,
            action,
        }) => {
            let options = crate::engine::event_handler::get_event_options(&session.run_state);
            let public_semantics = options
                .get(*option_index)
                .map(|option| {
                    planner_event_option_public_semantics_v1(&session.run_state, option)
                })
                .ok_or_else(|| {
                    format!(
                        "event candidate option {option_index} is absent from the public option surface"
                    )
                })?;
            Some(PlannerAction::ChooseEventOption {
                event: *event_id,
                screen: *screen,
                option_index: *option_index,
                action: *action,
                public_semantics,
            })
        }
        Some(DecisionCandidateKey::CardRewardPick {
            reward_item_index,
            option_index,
            card,
            upgrades,
        }) => Some(PlannerAction::TakeCard {
            reward_item_index: Some(*reward_item_index),
            option_index: *option_index,
            card: *card,
            upgrades: *upgrades,
        }),
        Some(DecisionCandidateKey::CardRewardOpen { reward_item_index }) => {
            Some(PlannerAction::OpenCardReward {
                reward_item_index: *reward_item_index,
            })
        }
        Some(DecisionCandidateKey::CardRewardSingingBowl {
            reward_item_index, ..
        }) => Some(PlannerAction::SingingBowl {
            reward_item_index: Some(*reward_item_index),
        }),
        Some(DecisionCandidateKey::CardRewardSkip { reward_item_index }) => {
            Some(PlannerAction::SkipCardReward {
                reward_item_index: *reward_item_index,
            })
        }
        Some(DecisionCandidateKey::BossRelicPick {
            option_index,
            relic,
        }) => Some(PlannerAction::TakeBossRelic {
            option_index: *option_index,
            relic: *relic,
        }),
        Some(DecisionCandidateKey::BossRelicSkip) => Some(PlannerAction::SkipBossRelic),
        Some(DecisionCandidateKey::RunPotionUse { slot, potion, uuid }) => {
            Some(PlannerAction::UseRunPotion {
                slot: *slot,
                potion: *potion,
                potion_uuid: *uuid,
            })
        }
        Some(DecisionCandidateKey::RunPotionDiscard { slot, potion, uuid }) => {
            Some(PlannerAction::DiscardRunPotion {
                slot: *slot,
                potion: *potion,
                potion_uuid: *uuid,
            })
        }
        Some(
            DecisionCandidateKey::RouteSelect { .. }
            | DecisionCandidateKey::RouteCancel
            | DecisionCandidateKey::RewardPotionClaim { .. }
            | DecisionCandidateKey::CampfireRest
            | DecisionCandidateKey::CampfireSmith { .. }
            | DecisionCandidateKey::CampfireDig
            | DecisionCandidateKey::CampfireLift
            | DecisionCandidateKey::CampfireToke { .. }
            | DecisionCandidateKey::CampfireRecall,
        ) => planner_action_from_execution(session, site, candidate)?,
        Some(DecisionCandidateKey::ShopPurgeCard {
            deck_index,
            card,
            upgrades,
        }) => session
            .run_state
            .master_deck
            .get(*deck_index)
            .map(|deck_card| PlannerAction::RemoveCard {
                card_lineage: deck_card.lineage,
                card: *card,
                upgrades: *upgrades,
                price: current_shop(session).map_or(0, |shop| shop.purge_cost),
            }),
        Some(DecisionCandidateKey::ShopBuyCard {
            shop_slot,
            card,
            upgrades,
            price,
        }) => Some(PlannerAction::BuyCard {
            shop_slot: *shop_slot,
            card: *card,
            upgrades: *upgrades,
            price: *price,
        }),
        Some(DecisionCandidateKey::ShopBuyRelic {
            shop_slot,
            relic,
            price,
        }) => Some(PlannerAction::BuyRelic {
            shop_slot: *shop_slot,
            relic: *relic,
            price: *price,
        }),
        Some(DecisionCandidateKey::ShopBuyPotion {
            shop_slot,
            potion,
            price,
        }) => Some(PlannerAction::BuyPotion {
            shop_slot: *shop_slot,
            potion: *potion,
            price: *price,
        }),
        Some(DecisionCandidateKey::ShopOpenRewards) => Some(PlannerAction::OpenPendingRewards),
        Some(DecisionCandidateKey::SelectionSubmit { .. }) => None,
        Some(DecisionCandidateKey::ShopLeave) => Some(PlannerAction::LeaveShop),
        None => planner_action_from_execution(session, site, candidate)?,
    };
    Ok(match action {
        Some(action) => CandidateProjection::Legal(action),
        None => CandidateProjection::Gap(CandidateRepresentationGap::UnsupportedBoundaryAction),
    })
}

fn planner_event_card_pool_v1(
    run_state: &crate::state::run::RunState,
    kind: crate::state::events::EventCardKind,
) -> crate::ai::planner_core::PlannerEventCardPoolV1 {
    use crate::ai::planner_core::PlannerEventCardPoolV1;
    use crate::state::events::EventCardKind;

    match kind {
        EventCardKind::Unknown => PlannerEventCardPoolV1::Unknown,
        EventCardKind::Specific(card) => PlannerEventCardPoolV1::Specific {
            card,
            upgrades: run_state.preview_obtain_card_upgrades(card, 0),
        },
        EventCardKind::RandomCurse => PlannerEventCardPoolV1::RandomCurse,
        EventCardKind::RandomClassCard => PlannerEventCardPoolV1::RandomClassCard,
        EventCardKind::RandomClassCommonOrUncommon => {
            PlannerEventCardPoolV1::RandomClassCommonOrUncommon
        }
        EventCardKind::RandomClassRare => PlannerEventCardPoolV1::RandomClassRare,
        EventCardKind::RandomColorless => PlannerEventCardPoolV1::RandomColorless,
        EventCardKind::RandomColorlessUncommon => PlannerEventCardPoolV1::RandomColorlessUncommon,
        EventCardKind::RandomColorlessRare => PlannerEventCardPoolV1::RandomColorlessRare,
    }
}

fn planner_event_relic_pool_v1(
    kind: crate::state::events::EventRelicKind,
) -> crate::ai::planner_core::PlannerEventRelicPoolV1 {
    use crate::ai::planner_core::PlannerEventRelicPoolV1;
    use crate::state::events::EventRelicKind;

    match kind {
        EventRelicKind::Unknown => PlannerEventRelicPoolV1::Unknown,
        EventRelicKind::Specific(relic) => PlannerEventRelicPoolV1::Specific { relic },
        EventRelicKind::RandomCommonRelic => PlannerEventRelicPoolV1::RandomCommonRelic,
        EventRelicKind::RandomUncommonRelic => PlannerEventRelicPoolV1::RandomUncommonRelic,
        EventRelicKind::RandomRareRelic => PlannerEventRelicPoolV1::RandomRareRelic,
        EventRelicKind::RandomShopRelic => PlannerEventRelicPoolV1::RandomShopRelic,
        EventRelicKind::RandomBossRelic => PlannerEventRelicPoolV1::RandomBossRelic,
        EventRelicKind::RandomRelic => PlannerEventRelicPoolV1::RandomRelic,
        EventRelicKind::RandomBook => PlannerEventRelicPoolV1::RandomBook,
        EventRelicKind::RandomFace => PlannerEventRelicPoolV1::RandomFace,
    }
}

fn planner_event_card_observation_v1(
    run_state: &crate::state::run::RunState,
    target_lineage: CardLineageId,
) -> Option<crate::ai::planner_core::PlannerCardObservation> {
    run_state
        .master_deck
        .iter()
        .find(|card| card.lineage == target_lineage)
        .map(|card| crate::ai::planner_core::PlannerCardObservation {
            card_lineage: card.lineage,
            card: card.id,
            upgrades: card.upgrades,
            misc_value: card.misc_value,
            base_damage_override: card.base_damage_override,
            base_block_override: card.base_block_override,
            cost_modifier: card.cost_modifier,
        })
}

fn planner_event_public_effect_v1(
    run_state: &crate::state::run::RunState,
    effect: &crate::state::events::EventEffect,
) -> crate::ai::planner_core::PlannerEventPublicEffectV1 {
    use crate::ai::planner_core::{
        PlannerEventPublicEffectV1 as Public, PlannerEventRandomOutcomeV1 as Random,
    };
    use crate::state::events::{EventEffect, EventRandomOutcomeKind};

    match effect {
        EventEffect::GainGold(amount) => Public::GainGold { amount: *amount },
        EventEffect::GainGoldRange { min, max } => Public::GainGoldRange {
            min: *min,
            max: *max,
        },
        EventEffect::LoseGold(amount) => Public::LoseGold { amount: *amount },
        EventEffect::LoseHp(amount) => Public::LoseHp { amount: *amount },
        EventEffect::LoseMaxHp(amount) => Public::LoseMaxHp { amount: *amount },
        EventEffect::Heal(amount) => Public::Heal { amount: *amount },
        EventEffect::GainMaxHp(amount) => Public::GainMaxHp { amount: *amount },
        EventEffect::ObtainRelic { count, kind } => Public::ObtainRelic {
            count: *count,
            pool: planner_event_relic_pool_v1(*kind),
        },
        EventEffect::ObtainPotion { count } => Public::ObtainRandomPotions { count: *count },
        EventEffect::ObtainCard { count, kind } => Public::ObtainCard {
            count: *count,
            pool: planner_event_card_pool_v1(run_state, *kind),
        },
        EventEffect::ObtainColorlessCard { count, kind } => Public::ObtainColorlessCard {
            count: *count,
            pool: planner_event_card_pool_v1(run_state, *kind),
        },
        EventEffect::OfferCards { count, kind } => Public::OfferCards {
            count: *count,
            pool: planner_event_card_pool_v1(run_state, *kind),
        },
        EventEffect::ObtainCurse { count, kind } => Public::ObtainCurse {
            count: *count,
            pool: planner_event_card_pool_v1(run_state, *kind),
        },
        EventEffect::RemoveCard {
            count,
            target_lineage,
            ..
        } => {
            let target = target_lineage.and_then(|target_lineage| {
                planner_event_card_observation_v1(run_state, target_lineage)
            });
            // Removal concerns the existing instance, not acquisition rules
            // (including Egg relics). A missing target has unknown upgrades.
            let pool = target.as_ref().map_or(
                crate::ai::planner_core::PlannerEventCardPoolV1::Unknown,
                |card| crate::ai::planner_core::PlannerEventCardPoolV1::Specific {
                    card: card.card,
                    upgrades: card.upgrades,
                },
            );
            Public::RemoveCard {
                count: *count,
                target,
                pool,
            }
        }
        EventEffect::UpgradeCard { count } => Public::UpgradeCard { count: *count },
        EventEffect::UpgradeAllCards => Public::UpgradeAllCards,
        EventEffect::TransformCard { count } => Public::TransformCard { count: *count },
        EventEffect::DuplicateCard { count } => Public::DuplicateCard { count: *count },
        EventEffect::LoseRelic {
            specific,
            starter_only,
        } => Public::LoseRelic {
            specific: *specific,
            starter_only: *starter_only,
        },
        EventEffect::LoseStarterRelic { specific } => Public::LoseStarterRelic {
            specific: *specific,
        },
        EventEffect::RandomOutcome {
            kind,
            chance_percent,
        } => Public::RandomOutcome {
            outcome: match kind {
                EventRandomOutcomeKind::GremlinWheel => Random::GremlinWheel,
                EventRandomOutcomeKind::JoustOwnerWin => Random::JoustOwnerWin,
                EventRandomOutcomeKind::DeadAdventurerSearch => Random::DeadAdventurerSearch,
                EventRandomOutcomeKind::MatchAndKeepFlip => Random::MatchAndKeepFlip,
                EventRandomOutcomeKind::ScrapOozeRelicRoll => Random::ScrapOozeRelicRoll,
                EventRandomOutcomeKind::MausoleumCurseRoll => Random::MausoleumCurseRoll,
            },
            chance_percent: *chance_percent,
        },
        EventEffect::StartCombat => Public::StartCombat,
    }
}

fn planner_event_transition_v1(
    transition: crate::state::events::EventOptionTransition,
) -> crate::ai::planner_core::PlannerEventTransitionV1 {
    use crate::ai::planner_core::{PlannerEventSelectionV1 as Selection, PlannerEventTransitionV1};
    use crate::state::events::{EventOptionTransition, EventSelectionKind};

    match transition {
        EventOptionTransition::None => PlannerEventTransitionV1::None,
        EventOptionTransition::AdvanceScreen => PlannerEventTransitionV1::AdvanceScreen,
        EventOptionTransition::Complete => PlannerEventTransitionV1::Complete,
        EventOptionTransition::OpenSelection(selection) => {
            PlannerEventTransitionV1::OpenSelection {
                selection: match selection {
                    EventSelectionKind::None => Selection::None,
                    EventSelectionKind::RemoveCard => Selection::RemoveCard,
                    EventSelectionKind::UpgradeCard => Selection::UpgradeCard,
                    EventSelectionKind::TransformCard => Selection::TransformCard,
                    EventSelectionKind::DuplicateCard => Selection::DuplicateCard,
                    EventSelectionKind::OfferCard => Selection::OfferCard,
                },
            }
        }
        EventOptionTransition::OpenReward => PlannerEventTransitionV1::OpenReward,
        EventOptionTransition::StartCombat => PlannerEventTransitionV1::StartCombat,
    }
}

fn planner_event_option_public_semantics_v1(
    run_state: &crate::state::run::RunState,
    option: &crate::state::events::EventOption,
) -> crate::ai::planner_core::PlannerEventOptionPublicSemanticsV1 {
    use crate::state::events::EventEffect;

    let offered_upgrades = |card| match option.ui.public_detail {
        Some(crate::state::events::EventOptionPublicDetail::OfferedCard {
            card: offered,
            upgrades,
        }) if offered == card => run_state.preview_obtain_card_upgrades(card, upgrades),
        _ => run_state.preview_obtain_card_upgrades(card, 0),
    };

    let mut public = crate::ai::planner_core::PlannerEventOptionPublicSemanticsV1 {
        public_detail: option.ui.public_detail,
        public_label: Some(option.ui.text.clone()),
        structured_semantics: Some(
            crate::ai::planner_core::PlannerEventOptionStructuredSemanticsV1 {
                effects: option
                    .semantics
                    .effects
                    .iter()
                    .map(|effect| {
                        let mut projected = planner_event_public_effect_v1(run_state, effect);
                        use crate::ai::planner_core::{
                            PlannerEventCardPoolV1 as Pool, PlannerEventPublicEffectV1 as Public,
                        };
                        if let Public::ObtainCard {
                            pool: Pool::Specific { card, upgrades },
                            ..
                        }
                        | Public::ObtainColorlessCard {
                            pool: Pool::Specific { card, upgrades },
                            ..
                        } = &mut projected
                        {
                            *upgrades = offered_upgrades(*card);
                        }
                        projected
                    })
                    .collect(),
                transition: planner_event_transition_v1(option.semantics.transition),
            },
        ),
        repeatable: option.semantics.repeatable,
        terminal: option.semantics.terminal,
        ..Default::default()
    };
    let mut projected_hp = run_state.current_hp;
    let mut projected_max_hp = run_state.max_hp;
    let healing_blocked = run_state
        .relics
        .iter()
        .any(|relic| relic.id == crate::content::relics::RelicId::MarkOfTheBloom);
    for effect in &option.semantics.effects {
        match effect {
            EventEffect::GainGold(amount) => {
                public.gold_delta_min = public.gold_delta_min.saturating_add(*amount);
                public.gold_delta_max = public.gold_delta_max.saturating_add(*amount);
            }
            EventEffect::GainGoldRange { min, max } => {
                public.gold_delta_min = public.gold_delta_min.saturating_add(*min);
                public.gold_delta_max = public.gold_delta_max.saturating_add(*max);
                public.random_outcome = true;
            }
            EventEffect::LoseGold(amount) => {
                public.gold_delta_min = public.gold_delta_min.saturating_sub(*amount);
                public.gold_delta_max = public.gold_delta_max.saturating_sub(*amount);
            }
            EventEffect::LoseHp(amount) => {
                projected_hp = projected_hp
                    .saturating_sub(*amount)
                    .clamp(0, projected_max_hp)
            }
            EventEffect::LoseMaxHp(amount) => {
                projected_max_hp = projected_max_hp.saturating_sub(*amount).max(1);
                projected_hp = projected_hp.min(projected_max_hp);
            }
            EventEffect::Heal(amount) => {
                if !healing_blocked {
                    projected_hp = projected_hp
                        .saturating_add(*amount)
                        .clamp(0, projected_max_hp);
                }
            }
            EventEffect::GainMaxHp(amount) => {
                projected_max_hp = projected_max_hp.saturating_add(*amount).max(1);
                if !healing_blocked {
                    projected_hp = projected_hp
                        .saturating_add(*amount)
                        .clamp(0, projected_max_hp);
                }
            }
            EventEffect::ObtainRelic { count, kind } => {
                public.relics_gained = public.relics_gained.saturating_add(*count);
                if !matches!(kind, crate::state::events::EventRelicKind::Specific(_)) {
                    public.random_outcome = true;
                }
            }
            EventEffect::ObtainPotion { count } => {
                public.potions_gained = public.potions_gained.saturating_add(*count);
                public.random_outcome = true;
            }
            EventEffect::ObtainCard { count, kind }
            | EventEffect::ObtainColorlessCard { count, kind } => {
                public.cards_gained = public.cards_gained.saturating_add(*count);
                if let crate::state::events::EventCardKind::Specific(card) = kind {
                    public.specific_cards_gained.push(PlannerOfferedCard {
                        card: *card,
                        upgrades: offered_upgrades(*card),
                    });
                }
                if !matches!(kind, crate::state::events::EventCardKind::Specific(_)) {
                    public.random_outcome = true;
                }
            }
            EventEffect::OfferCards { count, kind } => {
                public.cards_offered = public.cards_offered.saturating_add(*count);
                if !matches!(kind, crate::state::events::EventCardKind::Specific(_)) {
                    public.random_outcome = true;
                }
            }
            EventEffect::ObtainCurse { count, kind } => {
                public.curses_gained = public.curses_gained.saturating_add(*count);
                if !matches!(kind, crate::state::events::EventCardKind::Specific(_)) {
                    public.random_outcome = true;
                }
            }
            EventEffect::RemoveCard {
                count,
                target_lineage,
                ..
            } => {
                public.cards_removed = public.cards_removed.saturating_add(*count);
                if let Some(card) = target_lineage.and_then(|target_lineage| {
                    run_state
                        .master_deck
                        .iter()
                        .find(|card| card.lineage == target_lineage)
                }) {
                    public.specific_cards_removed.push(PlannerCardObservation {
                        card_lineage: card.lineage,
                        card: card.id,
                        upgrades: card.upgrades,
                        misc_value: card.misc_value,
                        base_damage_override: card.base_damage_override,
                        base_block_override: card.base_block_override,
                        cost_modifier: card.cost_modifier,
                    });
                }
            }
            EventEffect::UpgradeCard { count } => {
                public.cards_upgraded = public.cards_upgraded.saturating_add(*count)
            }
            EventEffect::UpgradeAllCards => public.upgrades_all_cards = true,
            EventEffect::TransformCard { count } => {
                public.cards_transformed = public.cards_transformed.saturating_add(*count);
                public.random_outcome = true;
            }
            EventEffect::DuplicateCard { count } => {
                public.cards_duplicated = public.cards_duplicated.saturating_add(*count)
            }
            EventEffect::LoseRelic { .. } | EventEffect::LoseStarterRelic { .. } => {
                public.relics_lost = public.relics_lost.saturating_add(1)
            }
            EventEffect::RandomOutcome { chance_percent, .. } => {
                public.random_outcome = true;
                public.random_outcome_chance_percent = *chance_percent;
            }
            EventEffect::StartCombat => public.starts_combat = true,
        }
    }
    public.hp_delta = projected_hp.saturating_sub(run_state.current_hp);
    public.max_hp_delta = projected_max_hp.saturating_sub(run_state.max_hp);
    public
}

fn planner_action_from_execution(
    session: &RunControlSession,
    site: PlannerDecisionSite,
    candidate: &DecisionCandidate,
) -> Result<Option<PlannerAction>, String> {
    let CandidateAction::Execute(action) = &candidate.action else {
        return Ok(None);
    };
    let route_question_mark_outcomes = match &candidate.key {
        Some(DecisionCandidateKey::RouteSelect {
            question_mark_outcomes,
            ..
        }) => question_mark_outcomes.clone(),
        _ => Vec::new(),
    };
    let planner_action = match action {
        RunDecisionAction::SkipCardReward { reward_item_index } => {
            Some(PlannerAction::SkipCardReward {
                reward_item_index: *reward_item_index,
            })
        }
        RunDecisionAction::SingingBowlCardReward { reward_item_index } => {
            Some(PlannerAction::SingingBowl {
                reward_item_index: Some(*reward_item_index),
            })
        }
        RunDecisionAction::Input(input) => match input {
            ClientInput::SelectMapNode(x) => Some(PlannerAction::ChooseRouteNode {
                x: *x as i32,
                y: next_map_y(session),
                flight: false,
                question_mark_outcomes: route_question_mark_outcomes.clone(),
            }),
            ClientInput::FlyToNode(x, y) => Some(PlannerAction::ChooseRouteNode {
                x: *x as i32,
                y: *y as i32,
                flight: true,
                question_mark_outcomes: route_question_mark_outcomes.clone(),
            }),
            ClientInput::ClaimReward(reward_item_index) => {
                let reward = current_reward(session)
                    .and_then(|reward| reward.items.get(*reward_item_index))
                    .map(planner_reward_descriptor);
                reward.map(|reward| PlannerAction::ClaimReward {
                    reward_item_index: *reward_item_index,
                    reward,
                })
            }
            ClientInput::SelectCard(option_index) => current_reward(session)
                .and_then(|reward| {
                    reward
                        .pending_card_choice
                        .as_ref()
                        .map(|cards| (reward, cards))
                })
                .and_then(|(reward, cards)| {
                    cards
                        .get(*option_index)
                        .map(|card| PlannerAction::TakeCard {
                            reward_item_index: reward.pending_card_reward_index,
                            option_index: *option_index,
                            card: card.id,
                            upgrades: card.upgrades,
                        })
                }),
            ClientInput::CampfireOption(choice) => planner_campfire_action(session, *choice),
            ClientInput::SubmitSelection(resolution) => Some(PlannerAction::SubmitRunSelection {
                scope: resolution.scope,
                selected_card_lineages: resolution.selected_card_lineages(),
            }),
            ClientInput::OpenRewardOverlay => Some(PlannerAction::OpenPendingRewards),
            ClientInput::OpenChest => Some(PlannerAction::OpenChest),
            ClientInput::Proceed => Some(PlannerAction::Proceed { site }),
            ClientInput::Cancel => Some(PlannerAction::Cancel { site }),
            _ => None,
        },
    };
    Ok(planner_action)
}

fn planner_campfire_action(
    session: &RunControlSession,
    choice: CampfireChoice,
) -> Option<PlannerAction> {
    match choice {
        CampfireChoice::Rest => Some(PlannerAction::Rest),
        CampfireChoice::Smith(index) => {
            session
                .run_state
                .master_deck
                .get(index)
                .map(|card| PlannerAction::Smith {
                    card_lineage: card.lineage,
                    card: card.id,
                    upgrades: card.upgrades,
                })
        }
        CampfireChoice::Dig => Some(PlannerAction::Dig),
        CampfireChoice::Lift => Some(PlannerAction::Lift),
        CampfireChoice::Toke(index) => {
            session
                .run_state
                .master_deck
                .get(index)
                .map(|card| PlannerAction::Toke {
                    card_lineage: card.lineage,
                    card: card.id,
                    upgrades: card.upgrades,
                })
        }
        CampfireChoice::Recall => Some(PlannerAction::Recall),
    }
}

fn planner_site_and_context(
    session: &RunControlSession,
) -> Result<Option<(PlannerDecisionSite, PlannerDecisionContext)>, String> {
    let value = match &session.engine_state {
        EngineState::MapNavigation => Some((
            PlannerDecisionSite::Map,
            PlannerDecisionContext::Map { overlay: false },
        )),
        EngineState::MapOverlay { .. } => Some((
            PlannerDecisionSite::Map,
            PlannerDecisionContext::Map { overlay: true },
        )),
        EngineState::EventRoom => {
            let event = session
                .run_state
                .event_state
                .as_ref()
                .ok_or_else(|| "event boundary is missing public event state".to_string())?;
            let site = if event.id == crate::state::events::EventId::Neow {
                PlannerDecisionSite::Neow
            } else {
                PlannerDecisionSite::Event
            };
            Some((
                site,
                PlannerDecisionContext::Event {
                    event: event.id,
                    screen: event.current_screen,
                    face_trader_result:
                        crate::content::events::face_trader::FaceTraderResult::from_event_state(
                            event,
                        ),
                },
            ))
        }
        EngineState::RewardScreen(reward)
        | EngineState::RewardOverlay {
            reward_state: reward,
            ..
        } if reward.pending_card_choice.is_some() => Some((
            PlannerDecisionSite::CardReward,
            PlannerDecisionContext::CardReward {
                reward_item_index: reward.pending_card_reward_index,
                pending_rewards: reward.items.iter().map(planner_reward_descriptor).collect(),
            },
        )),
        EngineState::RewardScreen(reward)
        | EngineState::RewardOverlay {
            reward_state: reward,
            ..
        } => Some((
            PlannerDecisionSite::Reward,
            PlannerDecisionContext::Reward {
                pending_rewards: reward.items.iter().map(planner_reward_descriptor).collect(),
            },
        )),
        EngineState::TreasureRoom(_) => Some((
            PlannerDecisionSite::Treasure,
            PlannerDecisionContext::Treasure,
        )),
        EngineState::Campfire => Some((
            PlannerDecisionSite::Campfire,
            PlannerDecisionContext::Campfire,
        )),
        EngineState::Shop(shop) => Some((
            PlannerDecisionSite::Shop,
            PlannerDecisionContext::Shop {
                purge_cost: shop.purge_cost,
                purge_available: shop.purge_available,
                inventory: planner_shop_inventory_v1(session, shop),
            },
        )),
        EngineState::RunPendingChoice(choice) => {
            let surface =
                super::selection_surface::active_selection_surface(session).ok_or_else(|| {
                    "run choice boundary is missing its selection surface".to_string()
                })?;
            Some((
                PlannerDecisionSite::RunChoice,
                PlannerDecisionContext::RunChoice {
                    scope: surface.scope,
                    reason: surface.reason,
                    source: Some(choice.source),
                    min_choices: surface.min_choices,
                    max_choices: surface.max_choices,
                },
            ))
        }
        EngineState::BossRelicSelect(_) => Some((
            PlannerDecisionSite::BossRelic,
            PlannerDecisionContext::BossRelic,
        )),
        EngineState::DoubleBossComplete => Some((
            PlannerDecisionSite::RunTransition,
            PlannerDecisionContext::RunTransition {
                stage: crate::ai::planner_core::PlannerRunTransitionV1::SecondBoss,
            },
        )),
        EngineState::Act3VictoryComplete => Some((
            PlannerDecisionSite::RunTransition,
            PlannerDecisionContext::RunTransition {
                stage: crate::ai::planner_core::PlannerRunTransitionV1::FinalAct,
            },
        )),
        EngineState::TrueVictoryComplete => Some((
            PlannerDecisionSite::RunTransition,
            PlannerDecisionContext::RunTransition {
                stage: crate::ai::planner_core::PlannerRunTransitionV1::VictoryScreen,
            },
        )),
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::CombatStart(_)
        | EngineState::PendingChoice(_)
        | EngineState::GameOver(_) => None,
    };
    Ok(value)
}

fn planner_shop_purchase_status_v1(
    blocked_reason: Option<PlannerShopPurchaseBlockReasonV1>,
) -> PlannerShopPurchaseStatusV1 {
    match blocked_reason {
        Some(reason) => PlannerShopPurchaseStatusV1::Blocked { reason },
        None => PlannerShopPurchaseStatusV1::Purchasable,
    }
}

/// Read all current stock, including offers that are not executable purchases.
pub fn planner_shop_inventory_v1(
    session: &RunControlSession,
    shop: &crate::state::shop::ShopState,
) -> PlannerShopInventoryV1 {
    let gold = session.run_state.gold;
    PlannerShopInventoryV1 {
        cards: shop
            .cards
            .iter()
            .enumerate()
            .map(|(slot, card)| PlannerShopCardOfferV1 {
                slot,
                card: card.card_id,
                upgrades: card.upgrades,
                price: card.price,
                affordable: gold >= card.price,
                purchase_status: planner_shop_purchase_status_v1(
                    super::shop_merchandise_purchase_block_reason_v1(
                        &session.run_state,
                        card.can_buy,
                        card.blocked_reason.as_deref(),
                        card.price,
                    ),
                ),
            })
            .collect(),
        relics: shop
            .relics
            .iter()
            .enumerate()
            .map(|(slot, relic)| PlannerShopRelicOfferV1 {
                slot,
                relic: relic.relic_id,
                price: relic.price,
                affordable: gold >= relic.price,
                purchase_status: planner_shop_purchase_status_v1(
                    super::shop_merchandise_purchase_block_reason_v1(
                        &session.run_state,
                        relic.can_buy,
                        relic.blocked_reason.as_deref(),
                        relic.price,
                    ),
                ),
            })
            .collect(),
        potions: shop
            .potions
            .iter()
            .enumerate()
            .map(|(slot, potion)| PlannerShopPotionOfferV1 {
                slot,
                potion: potion.potion_id,
                price: potion.price,
                affordable: gold >= potion.price,
                purchase_status: planner_shop_purchase_status_v1(
                    super::shop_potion_purchase_block_reason_v1(&session.run_state, potion),
                ),
            })
            .collect(),
        purge: Some(PlannerShopPurgeOfferV1 {
            price: shop.purge_cost,
            affordable: gold >= shop.purge_cost,
            purchase_status: planner_shop_purchase_status_v1(
                super::shop_purge_purchase_block_reason_v1(&session.run_state, shop),
            ),
        }),
    }
}

fn planner_player_class(player_class: &str) -> Result<PlannerPlayerClass, String> {
    match player_class {
        "Ironclad" => Ok(PlannerPlayerClass::Ironclad),
        "Silent" => Ok(PlannerPlayerClass::Silent),
        "Defect" => Ok(PlannerPlayerClass::Defect),
        "Watcher" => Ok(PlannerPlayerClass::Watcher),
        other => Err(format!("unsupported planner player class: {other}")),
    }
}

fn planner_mechanics() -> PlannerMechanicsManifest {
    PlannerMechanicsManifest {
        mechanics_id: PLANNER_MECHANICS_ID.to_string(),
        mechanics_version: PLANNER_MECHANICS_VERSION,
    }
}

fn planner_reward_descriptor(reward: &RewardItem) -> PlannerRewardDescriptor {
    match reward {
        RewardItem::Gold { amount } => PlannerRewardDescriptor::Gold { amount: *amount },
        RewardItem::StolenGold { amount } => {
            PlannerRewardDescriptor::StolenGold { amount: *amount }
        }
        RewardItem::Card { cards } => PlannerRewardDescriptor::CardReward {
            cards: cards
                .iter()
                .map(|card| PlannerOfferedCard {
                    card: card.id,
                    upgrades: card.upgrades,
                })
                .collect(),
        },
        RewardItem::Relic { relic_id } => PlannerRewardDescriptor::Relic { relic: *relic_id },
        RewardItem::Potion { potion_id } => PlannerRewardDescriptor::Potion { potion: *potion_id },
        RewardItem::EmeraldKey => PlannerRewardDescriptor::EmeraldKey,
        RewardItem::SapphireKey => PlannerRewardDescriptor::SapphireKey,
    }
}

fn current_reward(session: &RunControlSession) -> Option<&RewardState> {
    match &session.engine_state {
        EngineState::RewardScreen(reward) => Some(reward),
        EngineState::RewardOverlay { reward_state, .. } => Some(reward_state),
        _ => None,
    }
}

fn current_shop(session: &RunControlSession) -> Option<&crate::state::shop::ShopState> {
    match &session.engine_state {
        EngineState::Shop(shop) => Some(shop),
        _ => None,
    }
}

fn next_map_y(session: &RunControlSession) -> i32 {
    if session.run_state.map.current_y == -1 {
        0
    } else {
        session.run_state.map.current_y + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::planner_core::{PlannerEventCardPoolV1, PlannerMapRoom};
    use crate::content::events::face_trader::FaceTraderResult;
    use crate::content::potions::{Potion, PotionId};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{RunPendingChoiceReason, RunPendingChoiceState};
    use crate::state::events::{EventCardKind, EventId, EventState};
    use crate::state::map::node::{MapRoomNode, RoomType};
    use crate::state::selection::{DomainEventSource, SelectionReason, SelectionScope};

    #[test]
    fn completed_boss_preparation_tracks_rewards_relics_and_deferred_entry() {
        let mut session = RunControlSession::new(Default::default());
        let capture = |session: &RunControlSession| {
            capture_planner_boundary_v1(session)
                .unwrap()
                .unwrap()
                .observation
                .run
                .pending_act_entry
        };
        session.engine_state = EngineState::RewardScreen(RewardState::new());
        assert_eq!(capture(&session), None);
        session.run_state.pending_boss_reward = true;
        assert_eq!(capture(&session), Some(2));
        session.run_state.pending_boss_reward = false;
        session.engine_state =
            EngineState::BossRelicSelect(crate::state::rewards::BossRelicChoiceState::new(vec![]));
        assert_eq!(capture(&session), Some(2));
        session.engine_state = EngineState::RewardScreen(RewardState::new());
        session.run_state.pending_boss_act_transition = true;
        assert_eq!(capture(&session), Some(2));
        session.run_state.advance_act();
        assert_eq!(capture(&session), None);
        session.run_state.pending_boss_reward = true;
        assert_eq!(capture(&session), Some(3));
        session.run_state.act_num = 3;
        session.run_state.pending_boss_reward = false;
        session.engine_state = EngineState::DoubleBossComplete;
        assert_eq!(capture(&session), None);
        session.engine_state = EngineState::Act3VictoryComplete;
        assert_eq!(capture(&session), Some(4));
        session.engine_state = EngineState::EventRoom;
        session.run_state.event_state = Some(EventState::new(EventId::SpireHeart));
        session.run_state.is_final_act_available = true;
        session.run_state.keys = [true; 3];
        assert_eq!(capture(&session), Some(4));
    }

    #[test]
    fn post_boss_transitions_expose_typed_proceed_candidates() {
        let cases = [
            (
                EngineState::DoubleBossComplete,
                crate::ai::planner_core::PlannerRunTransitionV1::SecondBoss,
            ),
            (
                EngineState::Act3VictoryComplete,
                crate::ai::planner_core::PlannerRunTransitionV1::FinalAct,
            ),
            (
                EngineState::TrueVictoryComplete,
                crate::ai::planner_core::PlannerRunTransitionV1::VictoryScreen,
            ),
        ];
        for (engine_state, stage) in cases {
            let mut session = RunControlSession::new(Default::default());
            session.engine_state = engine_state;
            let boundary = capture_planner_boundary_v1(&session)
                .expect("capture transition")
                .expect("transition boundary");
            assert_eq!(
                boundary.observation.decision_site,
                PlannerDecisionSite::RunTransition
            );
            assert!(matches!(
                boundary.observation.context,
                PlannerDecisionContext::RunTransition { stage: actual } if actual == stage
            ));
            assert!(boundary
                .legal_candidate_set
                .candidates
                .iter()
                .any(|candidate| matches!(
                    candidate.action,
                    PlannerAction::Proceed {
                        site: PlannerDecisionSite::RunTransition
                    }
                )));
            assert!(matches!(
                boundary.legal_candidate_set.completeness,
                CandidateSetCompleteness::Complete { .. }
            ));
        }
    }

    #[test]
    fn face_trader_result_reaches_the_public_event_context() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        let mut event = EventState::new(EventId::FaceTrader);
        event.current_screen = 1;
        session.run_state.event_state = Some(event);
        crate::content::events::face_trader::handle_choice(
            &mut session.engine_state,
            &mut session.run_state,
            2,
        );

        let (_, context) = planner_site_and_context(&session)
            .expect("event context")
            .expect("event boundary");
        assert!(matches!(
            context,
            PlannerDecisionContext::Event {
                event: EventId::FaceTrader,
                screen: 2,
                face_trader_result: Some(FaceTraderResult::Leave),
            }
        ));
    }

    #[test]
    fn hidden_rng_and_encounter_queues_do_not_change_public_capture_ids() {
        let mut left = RunControlSession::new(Default::default());
        left.engine_state = EngineState::EventRoom;
        left.run_state.event_state = Some(EventState::new(EventId::Neow));
        let mut right = left.clone();
        right.run_state.monster_list.reverse();
        right.run_state.elite_monster_list.reverse();
        right.run_state.boss_list.reverse();
        right.run_state.rng_pool.misc_rng.random(10_000);

        let left = capture_planner_boundary_v1(&left)
            .expect("capture left")
            .expect("planner boundary");
        let right = capture_planner_boundary_v1(&right)
            .expect("capture right")
            .expect("planner boundary");

        assert_eq!(left.visit_id, right.visit_id);
        assert_eq!(left.observation, right.observation);
        assert_eq!(left.legal_candidate_set, right.legal_candidate_set);
    }

    #[test]
    fn random_curse_pool_remains_typed_in_the_planner_boundary() {
        let run_state = crate::state::run::RunState::new(1, 20, true, "Ironclad");

        assert_eq!(
            planner_event_card_pool_v1(&run_state, EventCardKind::RandomCurse),
            PlannerEventCardPoolV1::RandomCurse
        );
    }

    #[test]
    fn visible_candidates_are_represented_or_report_a_typed_gap() {
        let session = RunControlSession::new(Default::default());
        let pending = capture_planner_boundary_v1(&session)
            .expect("capture")
            .expect("initial Neow boundary");
        let visible = build_decision_surface(&session)
            .view
            .candidates
            .into_iter()
            .filter(|candidate| !matches!(candidate.action, CandidateAction::Unavailable { .. }))
            .count();
        let represented = pending.candidate_links.len();
        let gap_count = match &pending.legal_candidate_set.completeness {
            CandidateSetCompleteness::Complete { .. } => 0,
            CandidateSetCompleteness::Incomplete { gaps, .. } => gaps.len(),
        };

        assert!(represented == visible || gap_count > 0);
    }

    #[test]
    fn shop_capture_keeps_public_unpurchasable_inventory_outside_legal_actions() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.gold = 40;
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::BlockPotion, 1)),
            Some(Potion::new(PotionId::StrengthPotion, 2)),
            Some(Potion::new(PotionId::DexterityPotion, 3)),
        ];
        let mut shop = crate::state::shop::ShopState::new();
        shop.cards.push(crate::state::shop::ShopCard {
            card_id: crate::content::cards::CardId::Armaments,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        shop.relics.push(crate::state::shop::ShopRelic {
            relic_id: crate::content::relics::RelicId::Anchor,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        shop.potions.push(crate::state::shop::ShopPotion {
            potion_id: PotionId::FirePotion,
            price: 30,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let pending = capture_planner_boundary_v1(&session)
            .expect("capture")
            .expect("shop boundary");
        let PlannerDecisionContext::Shop { inventory, .. } = &pending.observation.context else {
            panic!("shop context");
        };

        assert!(matches!(
            inventory.cards[0].purchase_status,
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NotEnoughGold {
                    gold: 40,
                    price: 50
                }
            }
        ));
        assert!(matches!(
            inventory.relics[0].purchase_status,
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NotEnoughGold {
                    gold: 40,
                    price: 150
                }
            }
        ));
        assert!(matches!(
            inventory.potions[0].purchase_status,
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot
            }
        ));
        assert!(!pending
            .legal_candidate_set
            .candidates
            .iter()
            .any(|candidate| {
                matches!(
                    candidate.action,
                    PlannerAction::BuyCard { .. }
                        | PlannerAction::BuyRelic { .. }
                        | PlannerAction::BuyPotion { .. }
                )
            }));
    }

    #[test]
    fn event_removal_preserves_target_instance_instead_of_acquisition_upgrades() {
        use crate::ai::planner_core::{PlannerEventCardPoolV1, PlannerEventPublicEffectV1};
        use crate::content::cards::CardId;
        use crate::content::relics::{RelicId, RelicState};
        use crate::state::events::{EventCardKind, EventEffect};

        let mut run_state = crate::state::run::RunState::new(1, 20, false, "Ironclad");
        let mut upgraded = CombatCard::for_scenario(CardId::DarkEmbrace, 101);
        upgraded.upgrades = 1;
        run_state.master_deck = vec![CombatCard::for_scenario(CardId::DarkEmbrace, 102), upgraded];
        for egg in [None, Some(RelicId::FrozenEgg)] {
            run_state.relics = egg.into_iter().map(RelicState::new).collect();
            for (lineage, upgrades) in [(101, 1), (102, 0)] {
                let effect = EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: Some(crate::test_support::scenario_lineage(lineage)),
                    kind: EventCardKind::Specific(CardId::DarkEmbrace),
                };
                let PlannerEventPublicEffectV1::RemoveCard { target, pool, .. } =
                    planner_event_public_effect_v1(&run_state, &effect)
                else {
                    panic!("removal effect");
                };
                let target = target.expect("the exact instance exists");
                assert_eq!(
                    target.card_lineage,
                    crate::test_support::scenario_lineage(lineage)
                );
                assert_eq!(target.upgrades, upgrades);
                assert_eq!(
                    pool,
                    PlannerEventCardPoolV1::Specific {
                        card: CardId::DarkEmbrace,
                        upgrades,
                    }
                );
            }
        }
        for target_lineage in [None, Some(crate::test_support::scenario_lineage(999))] {
            let effect = EventEffect::RemoveCard {
                count: 1,
                target_lineage,
                kind: EventCardKind::Specific(CardId::DarkEmbrace),
            };
            assert!(matches!(
                planner_event_public_effect_v1(&run_state, &effect),
                PlannerEventPublicEffectV1::RemoveCard {
                    target: None,
                    pool: PlannerEventCardPoolV1::Unknown,
                    ..
                }
            ));
        }
    }

    #[test]
    fn vampires_option_projects_each_dynamic_starter_strike_removal() {
        use crate::content::cards::CardId;

        let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
        run_state.max_hp = 80;
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 101),
            CombatCard::for_scenario(CardId::Defend, 102),
            CombatCard::for_scenario(CardId::Strike, 103),
            CombatCard::for_scenario(CardId::StrikeG, 104),
        ];
        let event_state = EventState::new(EventId::Vampires);
        let options = crate::content::events::vampires::get_options(&run_state, &event_state);

        let public = planner_event_option_public_semantics_v1(&run_state, &options[0]);

        assert_eq!(public.hp_delta, -24);
        assert_eq!(public.max_hp_delta, -24);
        assert_eq!(public.cards_removed, 3);
        assert_eq!(public.cards_gained, 5);
        assert_eq!(
            public
                .specific_cards_removed
                .iter()
                .map(|card| (card.card_lineage, card.card))
                .collect::<Vec<_>>(),
            vec![
                (crate::test_support::scenario_lineage(104), CardId::StrikeG),
                (crate::test_support::scenario_lineage(103), CardId::Strike),
                (crate::test_support::scenario_lineage(101), CardId::Strike),
            ]
        );
    }

    #[test]
    fn library_card_options_preserve_public_card_identity_and_preview_upgrade() {
        use crate::content::cards::CardId;
        use crate::content::relics::{RelicId, RelicState};

        let mut run_state = crate::state::run::RunState::new(1, 20, false, "Ironclad");
        run_state.relics.push(RelicState::new(RelicId::ToxicEgg));
        let event_state = EventState {
            id: EventId::TheLibrary,
            current_screen: 1,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            prepared_encounter: None,
            card_payload: vec![crate::state::events::EventCardState {
                card_id: CardId::ShrugItOff,
                upgrades: 1,
            }],
            extra_data: Vec::new(),
        };
        let options = crate::content::events::the_library::get_options(&run_state, &event_state);

        let public = planner_event_option_public_semantics_v1(&run_state, &options[0]);

        assert_eq!(
            public.public_label.as_deref(),
            Some(options[0].ui.text.as_str())
        );
        assert_eq!(public.cards_gained, 1);
        assert_eq!(
            public.specific_cards_gained,
            vec![PlannerOfferedCard {
                card: CardId::ShrugItOff,
                upgrades: 1,
            }]
        );
        let structured = public
            .structured_semantics
            .expect("current event capture must retain typed mechanics");
        assert!(matches!(
            structured.effects.as_slice(),
            [
                crate::ai::planner_core::PlannerEventPublicEffectV1::ObtainCard {
                    count: 1,
                    pool: crate::ai::planner_core::PlannerEventCardPoolV1::Specific {
                        card: CardId::ShrugItOff,
                        upgrades: 1,
                    },
                }
            ]
        ));
    }

    #[test]
    fn neow_capture_preserves_random_pool_and_resolution_shape() {
        use crate::ai::planner_core::{
            PlannerEventCardPoolV1, PlannerEventPublicEffectV1, PlannerEventTransitionV1,
        };

        let mut run_state = crate::state::run::RunState::new(2, 20, false, "Ironclad");
        let mut engine_state = crate::state::core::EngineState::EventRoom;
        crate::content::events::neow::handle_choice(&mut engine_state, &mut run_state, 0);
        let event_state = run_state.event_state.clone().expect("Neow state");
        let options = crate::content::events::neow::get_options(&run_state, &event_state);

        let rare_public = planner_event_option_public_semantics_v1(&run_state, &options[0]);
        assert!(rare_public.random_outcome);
        let rare = rare_public
            .structured_semantics
            .expect("rare option mechanics");
        assert_eq!(
            rare.effects,
            vec![PlannerEventPublicEffectV1::ObtainCard {
                count: 1,
                pool: PlannerEventCardPoolV1::RandomClassRare,
            }]
        );
        assert_eq!(rare.transition, PlannerEventTransitionV1::Complete);

        let potion_public = planner_event_option_public_semantics_v1(&run_state, &options[1]);
        assert!(potion_public.random_outcome);
        let potions = potion_public
            .structured_semantics
            .expect("potion option mechanics");
        assert_eq!(
            potions.effects,
            vec![PlannerEventPublicEffectV1::ObtainRandomPotions { count: 3 }]
        );
        assert_eq!(potions.transition, PlannerEventTransitionV1::OpenReward);
    }

    #[test]
    fn big_fish_projection_reports_actual_heal_and_max_hp_changes() {
        let mut full = crate::state::run::RunState::new(1, 0, false, "Ironclad");
        full.current_hp = 80;
        full.max_hp = 80;
        let mut full_event = EventState::new(EventId::BigFish);
        full_event.internal_state = crate::content::events::big_fish::init_big_fish_state(&full);
        let full_options = crate::content::events::big_fish::get_options(&full, &full_event);

        let banana = planner_event_option_public_semantics_v1(&full, &full_options[0]);
        let donut = planner_event_option_public_semantics_v1(&full, &full_options[1]);

        assert_eq!((banana.hp_delta, banana.max_hp_delta), (0, 0));
        assert_eq!((donut.hp_delta, donut.max_hp_delta), (5, 5));

        let mut damaged = full;
        damaged.current_hp = 20;
        damaged.max_hp = 81;
        let mut damaged_event = EventState::new(EventId::BigFish);
        damaged_event.internal_state =
            crate::content::events::big_fish::init_big_fish_state(&damaged);
        let damaged_options =
            crate::content::events::big_fish::get_options(&damaged, &damaged_event);
        let banana = planner_event_option_public_semantics_v1(&damaged, &damaged_options[0]);

        assert_eq!((banana.hp_delta, banana.max_hp_delta), (27, 0));
    }

    #[test]
    fn scrap_ooze_projection_keeps_relic_roll_random_and_hp_loss_certain() {
        let mut run_state = crate::state::run::RunState::new(1, 20, false, "Ironclad");
        run_state.current_hp = 42;
        run_state.max_hp = 68;
        let event_state = EventState::new(EventId::ScrapOoze);
        let options = crate::content::events::scrap_ooze::get_options(&run_state, &event_state);

        let reach = planner_event_option_public_semantics_v1(&run_state, &options[0]);

        assert_eq!(reach.hp_delta, -5);
        assert_eq!(reach.relics_gained, 0);
        assert!(reach.random_outcome);
        assert_eq!(reach.random_outcome_chance_percent, Some(25));
        assert!(reach.repeatable);
    }

    #[test]
    fn gain_max_hp_projection_respects_mark_of_the_bloom_heal_block() {
        let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state
            .relics
            .push(crate::content::relics::RelicState::new(
                crate::content::relics::RelicId::MarkOfTheBloom,
            ));
        let mut event_state = EventState::new(EventId::BigFish);
        event_state.internal_state =
            crate::content::events::big_fish::init_big_fish_state(&run_state);
        let options = crate::content::events::big_fish::get_options(&run_state, &event_state);

        let donut = planner_event_option_public_semantics_v1(&run_state, &options[1]);

        assert_eq!((donut.hp_delta, donut.max_hp_delta), (0, 5));
    }

    #[test]
    fn boss_route_candidate_targets_a_published_synthetic_map_node() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        session.run_state.map.graph = vec![Vec::new(); 14];
        let mut final_campfire = MapRoomNode::new(0, 14);
        final_campfire.class = Some(RoomType::RestRoom);
        session.run_state.map.graph.push(vec![final_campfire]);
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 14;

        let pending = capture_planner_boundary_v1(&session)
            .expect("capture")
            .expect("boss route boundary");

        assert!(pending
            .observation
            .public_map
            .nodes
            .iter()
            .any(|node| { node.x == 0 && node.y == 15 && node.room == PlannerMapRoom::Boss }));
        assert!(pending
            .legal_candidate_set
            .candidates
            .iter()
            .any(|candidate| {
                candidate.action
                    == (PlannerAction::ChooseRouteNode {
                        x: 0,
                        y: 15,
                        flight: false,
                        question_mark_outcomes: Vec::new(),
                    })
            }));
    }

    #[test]
    fn public_map_publishes_question_marks_and_omits_absent_matrix_cells() {
        let mut session = RunControlSession::new(Default::default());
        let mut question_mark = MapRoomNode::new(0, 0);
        question_mark.class = Some(RoomType::EventRoom);
        question_mark
            .edges
            .insert(crate::state::map::MapEdge::new(0, 0, 0, 1));
        let absent = MapRoomNode::new(1, 0);
        session.run_state.map.graph = vec![vec![question_mark, absent]];

        let map = planner_public_map_v1(&session);

        assert_eq!(map.nodes.len(), 1);
        assert_eq!(map.nodes[0].room, PlannerMapRoom::QuestionMark);
    }

    #[test]
    fn map_fruit_juice_is_a_complete_typed_planner_action() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        session.run_state.potions = vec![Some(Potion::new(PotionId::FruitJuice, 10))];

        let pending = capture_planner_boundary_v1(&session)
            .expect("capture")
            .expect("map boundary");

        assert!(matches!(
            pending.legal_candidate_set.completeness,
            CandidateSetCompleteness::Complete { .. }
        ));
        assert!(pending
            .legal_candidate_set
            .candidates
            .iter()
            .any(|candidate| {
                candidate.action
                    == (PlannerAction::UseRunPotion {
                        slot: 0,
                        potion: PotionId::FruitJuice,
                        potion_uuid: 10,
                    })
            }));
    }

    #[test]
    fn card_choice_preserves_collection_through_back_claim_and_reopen() {
        use crate::content::cards::CardId;
        use crate::state::rewards::RewardCard;
        let mut session = RunControlSession::new(Default::default());
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Gold { amount: 27 },
            RewardItem::Potion {
                potion_id: PotionId::FirePotion,
            },
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::Anger, 1)],
            },
            RewardItem::Relic {
                relic_id: crate::content::relics::RelicId::Vajra,
            },
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::Flex, 0)],
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let before = capture_planner_boundary_v1(&session)
            .unwrap()
            .unwrap()
            .observation
            .context;
        assert!(crate::engine::tick_run_active(
            &mut session.engine_state,
            &mut session.run_state,
            &mut session.active_combat,
            Some(ClientInput::ClaimReward(2))
        ));
        let opened = capture_planner_boundary_v1(&session)
            .unwrap()
            .unwrap()
            .observation
            .context;
        let PlannerDecisionContext::Reward {
            pending_rewards: expected,
        } = before
        else {
            panic!("reward")
        };
        assert_eq!(
            opened,
            PlannerDecisionContext::CardReward {
                reward_item_index: Some(2),
                pending_rewards: expected.clone()
            }
        );
        assert!(crate::engine::tick_run_active(
            &mut session.engine_state,
            &mut session.run_state,
            &mut session.active_combat,
            Some(ClientInput::Cancel)
        ));
        assert!(crate::engine::tick_run_active(
            &mut session.engine_state,
            &mut session.run_state,
            &mut session.active_combat,
            Some(ClientInput::ClaimReward(0))
        ));
        assert!(crate::engine::tick_run_active(
            &mut session.engine_state,
            &mut session.run_state,
            &mut session.active_combat,
            Some(ClientInput::ClaimReward(1))
        ));
        assert_eq!(
            capture_planner_boundary_v1(&session)
                .unwrap()
                .unwrap()
                .observation
                .context,
            PlannerDecisionContext::CardReward {
                reward_item_index: Some(1),
                pending_rewards: expected[1..].to_vec()
            }
        );
    }

    #[test]
    fn full_belt_discard_is_a_complete_typed_planner_action() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::Elixir, 10)),
            Some(Potion::new(PotionId::FairyPotion, 20)),
            Some(Potion::new(PotionId::GamblersBrew, 30)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::CultistPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let pending = capture_planner_boundary_v1(&session)
            .expect("capture")
            .expect("reward boundary");

        assert!(matches!(
            &pending.observation.context,
            PlannerDecisionContext::Reward { pending_rewards }
                if pending_rewards == &[PlannerRewardDescriptor::Potion {
                    potion: PotionId::CultistPotion,
                }]
        ));
        assert!(matches!(
            pending.legal_candidate_set.completeness,
            CandidateSetCompleteness::Complete { .. }
        ));
        assert!(pending
            .legal_candidate_set
            .candidates
            .iter()
            .any(|candidate| {
                candidate.action
                    == (PlannerAction::DiscardRunPotion {
                        slot: 0,
                        potion: PotionId::Elixir,
                        potion_uuid: 10,
                    })
            }));
    }

    #[test]
    fn multi_card_run_choice_is_a_complete_typed_selection_family() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(crate::content::cards::CardId::Strike, 11),
            CombatCard::for_scenario(crate::content::cards::CardId::Defend, 22),
            CombatCard::for_scenario(crate::content::cards::CardId::Bash, 33),
        ];
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 2,
            max_choices: 2,
            reason: RunPendingChoiceReason::Transform,
            source: DomainEventSource::Selection(SelectionReason::Transform),
            return_state: Box::new(EngineState::MapNavigation),
        });

        let pending = capture_planner_boundary_v1(&session)
            .expect("capture")
            .expect("run-choice boundary");

        assert!(matches!(
            pending.legal_candidate_set.completeness,
            CandidateSetCompleteness::Complete { .. }
        ));
        assert_eq!(pending.legal_candidate_set.candidates.len(), 1);
        assert_eq!(
            pending.legal_candidate_set.candidates[0].action,
            PlannerAction::BeginRunCardSelection {
                scope: SelectionScope::Deck,
                reason: SelectionReason::Transform,
                min_choices: 2,
                max_choices: 2,
                selectable_card_lineages: crate::test_support::scenario_lineages([11, 22, 33]),
            }
        );
    }

    #[test]
    fn coverage_denominator_counts_yielded_visits_without_behavior_events() {
        let session = RunControlSession::new(Default::default());
        let visit = capture_planner_boundary_v1(&session)
            .expect("capture")
            .expect("planner boundary")
            .finish(PlannerBoundaryVisitOutcomeV1::Yielded {
                yield_kind: PlannerBoundaryYieldKindV1::ProgressBudgetExhausted,
            });
        let mut segment = PlannerBoundaryCaptureSegmentV1::default();
        segment.push(visit);

        let report = build_planner_boundary_capture_coverage_report_v1(&segment);

        assert_eq!(report.total_visits, 1);
        assert_eq!(report.yielded_visits, 1);
        assert_eq!(report.selected_visits, 0);
        assert_eq!(report.sites[0].visits, 1);
    }

    #[test]
    fn capture_ticket_rejects_a_transaction_from_another_boundary() {
        let session = RunControlSession::new(Default::default());
        let ticket = capture_planner_boundary_ticket_v1(&session)
            .expect("capture")
            .expect("planner boundary");
        let mut other = session.clone();
        other.decision_step += 1;
        let candidate_id = build_decision_surface(&other).view.candidates[0].id.clone();
        let outcome = other
            .apply_candidate_id(&candidate_id)
            .expect("other transaction");

        let segment = ticket.finish_for_progress(&outcome.progress_steps);

        assert!(matches!(
            segment.visits[0].outcome,
            PlannerBoundaryVisitOutcomeV1::ExecutionFailed
        ));
    }
}
