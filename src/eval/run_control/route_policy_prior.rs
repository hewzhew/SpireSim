use std::cmp::Ordering;

use serde::Serialize;

use crate::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, StrategyCapabilityCoverageV1,
    StrategyPackageIdV2, StrategyPlanSupportV1,
};
use crate::ai::planner_core::{PlannerMapRoom, PlannerQuestionMarkOutcome};
use crate::ai::route_window_facts::{
    build_route_path_family_from_target, summarize_route_path_family, RouteFirstEliteSegment,
    RouteWindowCoverageKind, RouteWindowFactsConfig, RouteWindowPath, RouteWindowPathFamily,
};
use crate::ai::strategy::boss_encounter_readiness::{
    boss_encounter_readiness_v1, BossEncounterPreparationBandV1, BossEncounterReadinessV1,
};
use crate::content::relics::RelicId;
use crate::state::map::node::RoomType;

use super::{
    exact_run_policy_decision_v1, owned_run_policy_actions_v1, positive_ranked_run_policy_prior_v1,
    DecisionCandidateKey, ExactRunPolicyActionSuccessorV1, ExactRunPolicyDecisionV1,
    RunControlSession, RunPolicyCandidateV1, RunPolicyPriorV1,
};

const ROUTE_PATH_BUDGET_V1: usize = 2_000;
const RECENT_COMBAT_HIGH_ATTRITION_MAX_HP_DENOMINATOR_V1: i32 = 6;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutePolicyBandV1 {
    PreservePendingRewards,
    ForcedBoss,
    HeartKeyDeadline,
    CriticalRecovery,
    RecoveryOption,
    LiquidityConversion,
    EliteGrowth,
    FlexibleGrowth,
    Ordinary,
    ForcedPressure,
    ContinueAfterRewards,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutePolicyArrivalV1 {
    Combat,
    QuestionMark,
    Campfire,
    Shop,
    Treasure,
    BossRelic,
    Map,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePolicyContextV1 {
    pub act: u8,
    pub ascension: u8,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub critical_recovery: bool,
    pub recovery_pressure: bool,
    pub shop_conversion_support: StrategyPlanSupportV1,
    pub recent_combat_hp_loss: Option<i32>,
    pub boss_encounter_readiness: BossEncounterReadinessV1,
    pub returned_rewards_complete: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePolicyPathEvidenceV1 {
    pub coverage: Option<RouteWindowCoverageKind>,
    pub observed_path_count: usize,
    pub min_early_pressure: usize,
    pub max_early_pressure: usize,
    pub first_shop_floor: Option<i32>,
    pub first_campfire_floor: Option<i32>,
    pub first_elite: RouteFirstEliteSegment,
    pub min_damage_rooms_before_recovery: usize,
    pub max_damage_rooms_before_recovery: usize,
    /// Fewest damage rooms on one selectable continuation that actually
    /// reaches a campfire. `None` means the observed continuation set has no
    /// recovery outlet. Unlike path counts, this is a capability of the
    /// player's controllable option set rather than a probability estimate.
    pub min_damage_rooms_to_recovery: Option<usize>,
    pub paths_with_recovery: usize,
    pub paths_with_recovery_before_damage: usize,
    /// Exact HP after choosing Rest from the current run state. `None` means
    /// Rest is unavailable (for example Coffee Dripper), so a visible
    /// campfire must not be treated as recovery backing.
    pub projected_hp_after_rest: Option<i32>,
    pub min_elites: usize,
    pub max_elites: usize,
    pub min_campfires: usize,
    pub max_campfires: usize,
    pub min_shops: usize,
    pub max_shops: usize,
    pub min_treasures: usize,
    pub max_treasures: usize,
    pub min_unknowns: usize,
    pub max_unknowns: usize,
    pub emerald_key_reachable: bool,
    pub emerald_key_forced: bool,
    pub emerald_key_required: bool,
}

impl RoutePolicyPathEvidenceV1 {
    fn complete(&self) -> bool {
        self.coverage == Some(RouteWindowCoverageKind::CompleteWithinHorizon)
    }

    fn some_path_recovers_before_damage(&self) -> bool {
        self.projected_hp_after_rest.is_some() && self.min_damage_rooms_to_recovery == Some(0)
    }

    fn usable_recovery_distance(&self) -> Option<usize> {
        self.projected_hp_after_rest
            .and(self.min_damage_rooms_to_recovery)
    }

    fn optional_elite(&self) -> bool {
        self.min_elites == 0 && self.max_elites > 0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RoutePolicyActionV1 {
    Select {
        x: i32,
        y: i32,
        room_type: Option<PlannerMapRoom>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        question_mark_outcomes: Vec<PlannerQuestionMarkOutcome>,
        uses_wing_boots: bool,
        has_emerald_key: bool,
        actual_wing_boots_spent: i32,
        arrival: RoutePolicyArrivalV1,
        path: RoutePolicyPathEvidenceV1,
    },
    CancelToPendingRewards,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePolicyActionEvidenceV1 {
    pub candidate_id: String,
    pub candidate_key: DecisionCandidateKey,
    pub action: RoutePolicyActionV1,
    pub band: RoutePolicyBandV1,
    surface_index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePolicyAuditCandidateV1 {
    pub owner_rank: usize,
    pub candidate_id: String,
    pub label: String,
    pub candidate_key: DecisionCandidateKey,
    pub action: RoutePolicyActionV1,
    pub band: RoutePolicyBandV1,
    pub surface_index: usize,
    pub prior_probability: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactRoutePolicyAuditV1 {
    pub context: RoutePolicyContextV1,
    pub candidates: Vec<RoutePolicyAuditCandidateV1>,
}

#[derive(Clone, Debug)]
pub struct ExactRoutePolicyDecisionV1 {
    pub exact: ExactRunPolicyDecisionV1,
    pub context: RoutePolicyContextV1,
    pub evidence: Vec<RoutePolicyActionEvidenceV1>,
    pub prior: RunPolicyPriorV1,
}

impl ExactRoutePolicyDecisionV1 {
    pub fn audit(
        &self,
        legal: &[RunPolicyCandidateV1<'_>],
    ) -> Result<ExactRoutePolicyAuditV1, String> {
        let candidates = self
            .evidence
            .iter()
            .enumerate()
            .map(|(owner_rank, evidence)| {
                let legal_candidate = legal
                    .iter()
                    .find(|candidate| candidate.candidate_id == evidence.candidate_id)
                    .ok_or_else(|| {
                        format!(
                            "route policy audit could not find legal candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                let prior_probability = self
                    .prior
                    .entries
                    .iter()
                    .find(|entry| entry.candidate_id == evidence.candidate_id)
                    .map(|entry| entry.probability)
                    .ok_or_else(|| {
                        format!(
                            "route policy audit could not find prior for candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                Ok(RoutePolicyAuditCandidateV1 {
                    owner_rank,
                    candidate_id: evidence.candidate_id.clone(),
                    label: legal_candidate.label.to_string(),
                    candidate_key: evidence.candidate_key.clone(),
                    action: evidence.action.clone(),
                    band: evidence.band,
                    surface_index: evidence.surface_index,
                    prior_probability,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ExactRoutePolicyAuditV1 {
            context: self.context.clone(),
            candidates,
        })
    }
}

pub fn exact_route_policy_audit_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactRoutePolicyAuditV1, String> {
    exact_route_policy_decision_v1(session, legal)?.audit(legal)
}

pub fn exact_route_policy_prior_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    Ok(exact_route_policy_decision_v1(session, legal)?.prior)
}

pub fn exact_route_policy_decision_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactRoutePolicyDecisionV1, String> {
    if !session.engine_state.is_map_surface() {
        return Err("exact route policy requires a map boundary".to_string());
    }

    let exact = exact_run_policy_decision_v1(session)?;
    let owned = owned_run_policy_actions_v1(&exact, legal, "route", is_route_key)?;
    let context = route_policy_context_v1(session)?;
    let mut evidence = owned
        .into_iter()
        .map(|(surface_index, successor)| {
            route_action_evidence_v1(session, successor, surface_index, &context)
        })
        .collect::<Result<Vec<_>, _>>()?;
    evidence.sort_by(|left, right| compare_route_evidence(left, right, &context));
    let prior = positive_ranked_run_policy_prior_v1(
        legal,
        evidence
            .iter()
            .map(|candidate| candidate.candidate_id.clone()),
    )?;

    Ok(ExactRoutePolicyDecisionV1 {
        exact,
        context,
        evidence,
        prior,
    })
}

fn is_route_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::RouteSelect { .. } | DecisionCandidateKey::RouteCancel
    )
}

fn route_policy_context_v1(session: &RunControlSession) -> Result<RoutePolicyContextV1, String> {
    let current_hp = session.run_state.current_hp.max(0);
    let max_hp = session.run_state.max_hp.max(0);
    let strategy = build_run_strategy_snapshot_from_run_state_v2(&session.run_state);
    Ok(RoutePolicyContextV1 {
        act: session.run_state.act_num,
        ascension: session.run_state.ascension_level,
        current_hp,
        max_hp,
        gold: session.run_state.gold,
        critical_recovery: max_hp > 0 && current_hp.saturating_mul(3) <= max_hp,
        recovery_pressure: max_hp > 0 && current_hp.saturating_mul(2) <= max_hp,
        shop_conversion_support: strategy.support(StrategyPackageIdV2::GoldPlan),
        recent_combat_hp_loss: session
            .recent_combat_attrition()
            .map(|attrition| attrition.raw_hp_loss),
        boss_encounter_readiness: boss_encounter_readiness_v1(&session.run_state),
        returned_rewards_complete: map_overlay_returned_rewards_complete(session)?,
    })
}

fn map_overlay_returned_rewards_complete(session: &RunControlSession) -> Result<bool, String> {
    use crate::state::core::EngineState;
    let EngineState::MapOverlay { return_state } = &session.engine_state else {
        return Ok(false);
    };
    if !matches!(
        return_state.as_ref(),
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. }
    ) {
        return Ok(false);
    }
    let mut returned = session.clone();
    returned.engine_state = return_state.as_ref().clone();
    Ok(super::reward_policy_progress_v1(&returned)?.is_complete())
}

fn route_action_evidence_v1(
    parent: &RunControlSession,
    successor: &ExactRunPolicyActionSuccessorV1,
    surface_index: usize,
    context: &RoutePolicyContextV1,
) -> Result<RoutePolicyActionEvidenceV1, String> {
    let candidate_key = successor.candidate_key.clone().ok_or_else(|| {
        format!(
            "route candidate '{}' has no typed key",
            successor.candidate_id
        )
    })?;
    let action = match candidate_key {
        DecisionCandidateKey::RouteSelect {
            x,
            y,
            room_type,
            ref question_mark_outcomes,
            uses_wing_boots,
            has_emerald_key,
        } => {
            let child = &successor.exact.session;
            if child.run_state.map.current_x != x || child.run_state.map.current_y != y {
                return Err(format!(
                    "route candidate '{}' selected ({x},{y}) but exact successor arrived at ({},{})",
                    successor.candidate_id,
                    child.run_state.map.current_x,
                    child.run_state.map.current_y
                ));
            }
            let before_charges = wing_boots_charges(parent);
            let after_charges = wing_boots_charges(child);
            let actual_wing_boots_spent = before_charges.saturating_sub(after_charges);
            if uses_wing_boots && actual_wing_boots_spent <= 0 {
                return Err(format!(
                    "route candidate '{}' claims a Wing Boots jump without consuming a charge",
                    successor.candidate_id
                ));
            }
            let arrival = route_arrival_v1(room_type.as_ref());
            RoutePolicyActionV1::Select {
                x,
                y,
                room_type,
                question_mark_outcomes: question_mark_outcomes.clone(),
                uses_wing_boots,
                has_emerald_key,
                actual_wing_boots_spent,
                arrival,
                path: route_path_evidence_v1(parent, x, y),
            }
        }
        DecisionCandidateKey::RouteCancel => RoutePolicyActionV1::CancelToPendingRewards,
        ref other => {
            return Err(format!(
                "exact route policy received non-route candidate key {other:?}"
            ))
        }
    };
    let band = route_policy_band_v1(&action, context);
    Ok(RoutePolicyActionEvidenceV1 {
        candidate_id: successor.candidate_id.clone(),
        candidate_key,
        action,
        band,
        surface_index,
    })
}

fn wing_boots_charges(session: &RunControlSession) -> i32 {
    session
        .run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::WingBoots && !relic.used_up)
        .map(|relic| relic.counter.max(0))
        .unwrap_or_default()
}

fn route_arrival_v1(room_type: Option<&PlannerMapRoom>) -> RoutePolicyArrivalV1 {
    // Applying a route choice first commits the selected coordinates. Its exact
    // successor may still carry the room we just left until normal execution
    // advances the new node. Arrival evidence is public map topology, so it
    // must come from the selected candidate rather than that transient state.
    match room_type {
        Some(PlannerMapRoom::Monster | PlannerMapRoom::Elite | PlannerMapRoom::Boss) => {
            RoutePolicyArrivalV1::Combat
        }
        Some(PlannerMapRoom::QuestionMark) => RoutePolicyArrivalV1::QuestionMark,
        Some(PlannerMapRoom::Rest) => RoutePolicyArrivalV1::Campfire,
        Some(PlannerMapRoom::Shop) => RoutePolicyArrivalV1::Shop,
        Some(PlannerMapRoom::Treasure) => RoutePolicyArrivalV1::Treasure,
        Some(PlannerMapRoom::Victory) | None => RoutePolicyArrivalV1::Other,
    }
}

fn route_path_evidence_v1(
    session: &RunControlSession,
    x: i32,
    y: i32,
) -> RoutePolicyPathEvidenceV1 {
    let horizon_nodes = 16_usize.saturating_sub(y.max(0) as usize).max(1);
    let family = build_route_path_family_from_target(
        &session.run_state,
        x,
        y,
        RouteWindowFactsConfig {
            horizon_nodes,
            path_budget: ROUTE_PATH_BUDGET_V1,
        },
    );
    let mut evidence = summarize_path_family(&family, &session.run_state);
    evidence.emerald_key_required = session.run_state.is_final_act_available
        && session.run_state.act_num == 3
        && !session.run_state.keys[2]
        && evidence.emerald_key_reachable;
    if evidence.min_damage_rooms_to_recovery.is_some() {
        evidence.projected_hp_after_rest = projected_hp_after_rest(&session.run_state);
    }
    evidence
}

fn projected_hp_after_rest(run_state: &crate::state::run::RunState) -> Option<i32> {
    use crate::state::core::CampfireChoice;

    if !crate::engine::campfire_handler::campfire_choice_is_available(
        run_state,
        CampfireChoice::Rest,
    ) {
        return None;
    }
    let mut trial = run_state.clone();
    crate::engine::campfire_handler::apply_campfire_rest_healing(&mut trial);
    Some(trial.current_hp)
}

#[derive(Clone, Copy, Debug, Default)]
struct PathStats {
    damage_before_recovery: usize,
    recovered: bool,
    recovery_before_damage: bool,
    elites: usize,
    campfires: usize,
    shops: usize,
    treasures: usize,
    unknowns: usize,
}

fn summarize_path_family(
    family: &RouteWindowPathFamily,
    run_state: &crate::state::run::RunState,
) -> RoutePolicyPathEvidenceV1 {
    let summary = summarize_route_path_family(family);
    let stats = family.paths.iter().map(summarize_path).collect::<Vec<_>>();
    if stats.is_empty() {
        return RoutePolicyPathEvidenceV1 {
            coverage: Some(family.coverage.kind),
            min_early_pressure: summary.min_early_pressure,
            max_early_pressure: summary.max_early_pressure,
            first_shop_floor: summary.first_shop_floor,
            first_campfire_floor: summary.first_campfire_floor,
            first_elite: summary.first_elite,
            ..Default::default()
        };
    }
    RoutePolicyPathEvidenceV1 {
        coverage: Some(family.coverage.kind),
        observed_path_count: summary.path_count,
        min_early_pressure: summary.min_early_pressure,
        max_early_pressure: summary.max_early_pressure,
        first_shop_floor: summary.first_shop_floor,
        first_campfire_floor: summary.first_campfire_floor,
        first_elite: summary.first_elite,
        min_damage_rooms_before_recovery: summary.min_damage_rooms_before_recovery,
        max_damage_rooms_before_recovery: summary.max_damage_rooms_before_recovery,
        min_damage_rooms_to_recovery: stats
            .iter()
            .filter(|item| item.recovered)
            .map(|item| item.damage_before_recovery)
            .min(),
        paths_with_recovery: stats.iter().filter(|item| item.recovered).count(),
        paths_with_recovery_before_damage: summary.paths_with_recovery_before_damage,
        projected_hp_after_rest: None,
        min_elites: summary.min_elites,
        max_elites: summary.max_elites,
        min_campfires: summary.min_campfires,
        max_campfires: summary.max_campfires,
        min_shops: summary.min_shops,
        max_shops: summary.max_shops,
        min_treasures: summary.min_treasures,
        max_treasures: summary.max_treasures,
        min_unknowns: summary.min_unknowns,
        max_unknowns: summary.max_unknowns,
        emerald_key_reachable: family.paths.iter().any(|path| {
            path.nodes.iter().any(|node| {
                run_state
                    .map
                    .graph
                    .get(node.y.max(0) as usize)
                    .and_then(|row| row.get(node.x.max(0) as usize))
                    .is_some_and(|map_node| map_node.has_emerald_key)
            })
        }),
        emerald_key_forced: family.paths.iter().all(|path| {
            path.nodes.iter().any(|node| {
                run_state
                    .map
                    .graph
                    .get(node.y.max(0) as usize)
                    .and_then(|row| row.get(node.x.max(0) as usize))
                    .is_some_and(|map_node| map_node.has_emerald_key)
            })
        }),
        emerald_key_required: false,
    }
}

fn summarize_path(path: &RouteWindowPath) -> PathStats {
    let mut stats = PathStats::default();
    for node in &path.nodes {
        match node.room_type {
            Some(
                RoomType::MonsterRoom | RoomType::MonsterRoomElite | RoomType::MonsterRoomBoss,
            ) => {
                if !stats.recovered {
                    stats.damage_before_recovery = stats.damage_before_recovery.saturating_add(1);
                }
                if node.room_type == Some(RoomType::MonsterRoomElite) {
                    stats.elites = stats.elites.saturating_add(1);
                }
            }
            Some(RoomType::RestRoom) => {
                stats.campfires = stats.campfires.saturating_add(1);
                if !stats.recovered {
                    stats.recovery_before_damage = stats.damage_before_recovery == 0;
                    stats.recovered = true;
                }
            }
            Some(RoomType::ShopRoom) => {
                stats.shops = stats.shops.saturating_add(1);
            }
            Some(RoomType::TreasureRoom) => {
                stats.treasures = stats.treasures.saturating_add(1);
            }
            Some(RoomType::EventRoom) => {
                stats.unknowns = stats.unknowns.saturating_add(1);
            }
            _ => {}
        }
    }
    stats
}

fn route_policy_band_v1(
    action: &RoutePolicyActionV1,
    context: &RoutePolicyContextV1,
) -> RoutePolicyBandV1 {
    let RoutePolicyActionV1::Select {
        room_type, path, ..
    } = action
    else {
        return if context.returned_rewards_complete {
            RoutePolicyBandV1::ContinueAfterRewards
        } else {
            RoutePolicyBandV1::PreservePendingRewards
        };
    };
    if *room_type == Some(PlannerMapRoom::Boss) {
        return RoutePolicyBandV1::ForcedBoss;
    }
    if path.emerald_key_required {
        return RoutePolicyBandV1::HeartKeyDeadline;
    }
    if context.critical_recovery && path.some_path_recovers_before_damage() {
        return RoutePolicyBandV1::CriticalRecovery;
    }
    if context.recovery_pressure && path.some_path_recovers_before_damage() {
        return RoutePolicyBandV1::RecoveryOption;
    }
    if *room_type == Some(PlannerMapRoom::Shop)
        && path.min_elites == 0
        && shop_conversion_is_supported(context)
    {
        return RoutePolicyBandV1::LiquidityConversion;
    }
    if *room_type == Some(PlannerMapRoom::Elite)
        && path.min_elites == 1
        && a0_act1_elite_growth_is_supported(context)
    {
        return RoutePolicyBandV1::EliteGrowth;
    }
    if recovery_backed_a0_act1_elite_growth_is_supported(path, context) {
        return RoutePolicyBandV1::EliteGrowth;
    }
    if recovery_backed_a0_elite_growth_is_supported(path, context) {
        return RoutePolicyBandV1::EliteGrowth;
    }
    if path.min_elites == 0
        && (path.optional_elite()
            || path.max_campfires > path.min_campfires
            || path.max_shops > path.min_shops)
    {
        return RoutePolicyBandV1::FlexibleGrowth;
    }
    if context.critical_recovery && !path.some_path_recovers_before_damage() {
        return RoutePolicyBandV1::ForcedPressure;
    }
    RoutePolicyBandV1::Ordinary
}

fn recovery_backed_a0_act1_elite_growth_is_supported(
    path: &RoutePolicyPathEvidenceV1,
    context: &RoutePolicyContextV1,
) -> bool {
    context.act == 1
        && context.ascension == 0
        && context.max_hp > 0
        && path
            .projected_hp_after_rest
            .is_some_and(|hp| hp.saturating_mul(4) >= context.max_hp.saturating_mul(3))
        && !recent_combat_attrition_is_high(context)
        && !context
            .boss_encounter_readiness
            .preparation
            .requires_resource_preservation()
        && context.boss_encounter_readiness.timed_damage_race
            >= StrategyCapabilityCoverageV1::Supported
        && path.complete()
        && path.observed_path_count > 0
        && path.paths_with_recovery_before_damage == path.observed_path_count
        && path.first_elite.forced
        && path.first_elite.min_campfires_before >= 1
        && path.first_elite.max_hallway_fights_before == 0
        && path.first_elite.max_unknowns_before == 0
        && path.min_elites >= 1
}

fn a0_act1_elite_growth_is_supported(context: &RoutePolicyContextV1) -> bool {
    context.act == 1
        && context.ascension == 0
        && context.max_hp > 0
        && context.current_hp.saturating_mul(4) >= context.max_hp.saturating_mul(3)
        && !recent_combat_attrition_is_high(context)
        && !context
            .boss_encounter_readiness
            .preparation
            .requires_resource_preservation()
}

fn recovery_backed_a0_elite_growth_is_supported(
    path: &RoutePolicyPathEvidenceV1,
    context: &RoutePolicyContextV1,
) -> bool {
    context.ascension == 0
        && context.max_hp > 0
        && context.current_hp.saturating_mul(4) >= context.max_hp.saturating_mul(3)
        && !recent_combat_attrition_is_high(context)
        && !context
            .boss_encounter_readiness
            .preparation
            .requires_resource_preservation()
        && context.boss_encounter_readiness.timed_damage_race
            >= StrategyCapabilityCoverageV1::Supported
        && context.boss_encounter_readiness.sustained_defense
            >= StrategyCapabilityCoverageV1::Supported
        && context.boss_encounter_readiness.draw_energy_consistency
            >= StrategyCapabilityCoverageV1::Supported
        && path.complete()
        && path.observed_path_count > 0
        && path.paths_with_recovery_before_damage == path.observed_path_count
        && path.min_elites == 1
        && path.max_elites == 1
}

fn recent_combat_attrition_is_high(context: &RoutePolicyContextV1) -> bool {
    context.max_hp > 0
        && context.recent_combat_hp_loss.is_some_and(|loss| {
            loss.max(0)
                .saturating_mul(RECENT_COMBAT_HIGH_ATTRITION_MAX_HP_DENOMINATOR_V1)
                >= context.max_hp
        })
}

fn shop_conversion_is_supported(context: &RoutePolicyContextV1) -> bool {
    matches!(
        context.shop_conversion_support,
        StrategyPlanSupportV1::Plausible | StrategyPlanSupportV1::Strong
    )
}

fn compare_route_evidence(
    left: &RoutePolicyActionEvidenceV1,
    right: &RoutePolicyActionEvidenceV1,
    context: &RoutePolicyContextV1,
) -> Ordering {
    left.band
        .cmp(&right.band)
        .then_with(|| compare_route_actions(&left.action, &right.action, context))
        .then_with(|| left.surface_index.cmp(&right.surface_index))
}

fn compare_route_actions(
    left: &RoutePolicyActionV1,
    right: &RoutePolicyActionV1,
    context: &RoutePolicyContextV1,
) -> Ordering {
    let (
        RoutePolicyActionV1::Select {
            room_type: left_room,
            uses_wing_boots: left_wing,
            path: left_path,
            ..
        },
        RoutePolicyActionV1::Select {
            room_type: right_room,
            uses_wing_boots: right_wing,
            path: right_path,
            ..
        },
    ) = (left, right)
    else {
        return Ordering::Equal;
    };

    // Route suffixes are future player choices, not sampled worlds. Compare
    // the best selectable recovery outlet; never treat the number or fraction
    // of enumerated suffixes as a probability.
    let survival = compare_selectable_recovery(left_path, right_path);
    if context.recovery_pressure && survival != Ordering::Equal {
        return survival;
    }

    let left_supported_hallway_development =
        supports_immediate_hallway_development(*left_room, left_path, context);
    let right_supported_hallway_development =
        supports_immediate_hallway_development(*right_room, right_path, context);

    let boss_preparation = if matches!(
        context.boss_encounter_readiness.preparation,
        BossEncounterPreparationBandV1::Exposed | BossEncounterPreparationBandV1::PotionBacked
    ) {
        right_path
            .max_campfires
            .cmp(&left_path.max_campfires)
            .then_with(|| compare_selectable_recovery(left_path, right_path))
            .then_with(|| left_path.min_elites.cmp(&right_path.min_elites))
            .then_with(|| left_path.max_elites.cmp(&right_path.max_elites))
    } else {
        Ordering::Equal
    };
    if boss_preparation != Ordering::Equal {
        return boss_preparation;
    }

    let shop = if shop_conversion_is_supported(context) {
        right_path.max_shops.cmp(&left_path.max_shops)
    } else {
        Ordering::Equal
    };
    shop.then_with(|| left_path.min_elites.cmp(&right_path.min_elites))
        .then_with(|| right_path.max_elites.cmp(&left_path.max_elites))
        .then_with(|| right_path.max_campfires.cmp(&left_path.max_campfires))
        .then_with(|| right_supported_hallway_development.cmp(&left_supported_hallway_development))
        .then_with(|| right_path.max_treasures.cmp(&left_path.max_treasures))
        .then_with(|| left_wing.cmp(right_wing))
        .then_with(|| right_path.complete().cmp(&left_path.complete()))
}

fn compare_selectable_recovery(
    left: &RoutePolicyPathEvidenceV1,
    right: &RoutePolicyPathEvidenceV1,
) -> Ordering {
    match (
        left.usable_recovery_distance(),
        right.usable_recovery_distance(),
    ) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn supports_immediate_hallway_development(
    room_type: Option<PlannerMapRoom>,
    path: &RoutePolicyPathEvidenceV1,
    context: &RoutePolicyContextV1,
) -> bool {
    room_type == Some(PlannerMapRoom::Monster)
        && context.act == 1
        && context.ascension == 0
        && !context.critical_recovery
        && !context.recovery_pressure
        && context.boss_encounter_readiness.boss_threat_gap_count > 0
        && context.recent_combat_hp_loss.is_some()
        && !recent_combat_attrition_is_high(context)
        && path
            .usable_recovery_distance()
            .is_some_and(|damage_rooms| damage_rooms <= 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run_control::{build_decision_surface, RunControlConfig, RunDecisionAction};
    use crate::state::core::{ClientInput, EngineState};
    use crate::state::map::node::{MapEdge, MapRoomNode};
    use crate::state::map::state::MapState;

    fn policy_candidates<'a>(
        surface: &'a super::super::DecisionSurface,
    ) -> Vec<RunPolicyCandidateV1<'a>> {
        surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect()
    }

    fn node(x: i32, y: i32, room_type: RoomType) -> MapRoomNode {
        let mut node = MapRoomNode::new(x, y);
        node.class = Some(room_type);
        node
    }

    fn route_action(
        room_type: RoomType,
        arrival: RoutePolicyArrivalV1,
        path: RoutePolicyPathEvidenceV1,
    ) -> RoutePolicyActionV1 {
        RoutePolicyActionV1::Select {
            x: 0,
            y: 0,
            room_type: Some(room_type.into()),
            question_mark_outcomes: Vec::new(),
            uses_wing_boots: false,
            has_emerald_key: false,
            actual_wing_boots_spent: 0,
            arrival,
            path,
        }
    }

    #[test]
    fn route_arrival_uses_the_selected_public_room_type() {
        assert_eq!(
            route_arrival_v1(Some(&PlannerMapRoom::QuestionMark)),
            RoutePolicyArrivalV1::QuestionMark
        );
        assert_eq!(
            route_arrival_v1(Some(&PlannerMapRoom::Elite)),
            RoutePolicyArrivalV1::Combat
        );
        assert_eq!(
            route_arrival_v1(Some(&PlannerMapRoom::Rest)),
            RoutePolicyArrivalV1::Campfire
        );
        assert_eq!(
            route_arrival_v1(Some(&PlannerMapRoom::Shop)),
            RoutePolicyArrivalV1::Shop
        );
        assert_eq!(
            route_arrival_v1(Some(&PlannerMapRoom::Treasure)),
            RoutePolicyArrivalV1::Treasure
        );
    }

    fn two_route_session() -> RunControlSession {
        let mut combat = node(0, 0, RoomType::MonsterRoom);
        combat.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut rest = node(1, 0, RoomType::RestRoom);
        rest.edges.insert(MapEdge::new(1, 0, 1, 1));
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        session.run_state.map = MapState::new(vec![
            vec![combat, rest],
            vec![
                node(0, 1, RoomType::RestRoom),
                node(1, 1, RoomType::MonsterRoom),
            ],
        ]);
        session.run_state.current_hp = 10;
        session.run_state.max_hp = 80;
        session.engine_state = EngineState::MapNavigation;
        session
    }

    fn shop_or_hallway_session(gold: i32) -> RunControlSession {
        let mut shop = node(0, 0, RoomType::ShopRoom);
        shop.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut hallway = node(1, 0, RoomType::MonsterRoom);
        hallway.edges.insert(MapEdge::new(1, 0, 1, 1));
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        session.run_state.map = MapState::new(vec![
            vec![shop, hallway],
            vec![
                node(0, 1, RoomType::MonsterRoom),
                node(1, 1, RoomType::MonsterRoom),
            ],
        ]);
        session.run_state.current_hp = 80;
        session.run_state.max_hp = 80;
        session.run_state.gold = gold;
        session.engine_state = EngineState::MapNavigation;
        session
    }

    #[test]
    fn every_exact_route_action_has_a_typed_key_and_positive_support() {
        let session = two_route_session();
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_route_policy_decision_v1(&session, &legal).expect("exact route decision");

        assert_eq!(decision.evidence.len(), legal.len());
        assert_eq!(decision.prior.entries.len(), legal.len());
        assert!(decision
            .evidence
            .iter()
            .all(|entry| is_route_key(&entry.candidate_key)));
        assert!(decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability.is_finite() && entry.probability > 0.0));

        let audit = decision.audit(&legal).expect("typed route audit");
        assert_eq!(audit.candidates.len(), legal.len());
        assert_eq!(
            audit
                .candidates
                .iter()
                .map(|candidate| candidate.owner_rank)
                .collect::<Vec<_>>(),
            (0..legal.len()).collect::<Vec<_>>()
        );
        assert!(audit
            .candidates
            .iter()
            .all(|candidate| candidate.prior_probability.is_finite()
                && candidate.prior_probability > 0.0));
        serde_json::to_value(audit).expect("serialize typed route audit");
    }

    #[test]
    fn route_prior_lifts_ranked_routes_over_auxiliary_map_actions() {
        let session = two_route_session();
        let surface = build_decision_surface(&session);
        let mut legal = policy_candidates(&surface);
        let auxiliary_action = RunDecisionAction::Input(ClientInput::DiscardPotion(0));
        legal.push(RunPolicyCandidateV1 {
            candidate_id: "auxiliary-discard-potion",
            label: "discard potion",
            action: &auxiliary_action,
        });

        let prior = exact_route_policy_prior_v1(&session, &legal)
            .expect("route prior accepts a complete map surface");

        assert_eq!(prior.entries.len(), legal.len());
        assert_eq!(
            prior
                .entries
                .last()
                .map(|entry| entry.candidate_id.as_str()),
            Some("auxiliary-discard-potion")
        );
        prior.validate_for(&legal).expect("complete positive prior");

        let audit = exact_route_policy_audit_v1(&session, &legal)
            .expect("route audit accepts a complete map surface");
        assert_eq!(audit.candidates.len(), legal.len() - 1);
        assert!(audit
            .candidates
            .iter()
            .all(|candidate| candidate.candidate_id != "auxiliary-discard-potion"));
    }

    #[test]
    fn critical_hp_prefers_a_proven_recovery_before_damage() {
        let session = two_route_session();
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_route_policy_decision_v1(&session, &legal).expect("exact route decision");

        let first = decision.evidence.first().expect("preferred route");
        assert!(matches!(
            first.action,
            RoutePolicyActionV1::Select {
                room_type: Some(PlannerMapRoom::Rest),
                ..
            }
        ));
        assert_eq!(first.band, RoutePolicyBandV1::CriticalRecovery);
    }

    #[test]
    fn coffee_dripper_campfire_is_not_a_recovery_outlet() {
        let mut session = two_route_session();
        session.run_state.relics = [RelicId::CoffeeDripper]
            .into_iter()
            .map(crate::content::relics::RelicState::new)
            .collect();
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_route_policy_decision_v1(&session, &legal).unwrap();
        let rest = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.action,
                    RoutePolicyActionV1::Select {
                        room_type: Some(PlannerMapRoom::Rest),
                        ..
                    }
                )
            })
            .unwrap();

        let RoutePolicyActionV1::Select { path, .. } = &rest.action else {
            unreachable!();
        };
        assert_eq!(path.min_damage_rooms_to_recovery, Some(0));
        assert_eq!(path.projected_hp_after_rest, None);
        assert!(!path.some_path_recovers_before_damage());
        assert!(!matches!(
            rest.band,
            RoutePolicyBandV1::CriticalRecovery | RoutePolicyBandV1::RecoveryOption
        ));
    }

    #[test]
    fn act_three_heart_route_preserves_the_last_emerald_key_path() {
        let mut key_path = node(0, 0, RoomType::MonsterRoom);
        key_path.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut misses_key = node(1, 0, RoomType::MonsterRoom);
        misses_key.edges.insert(MapEdge::new(1, 0, 1, 1));
        let mut burning_elite = node(0, 1, RoomType::MonsterRoomElite);
        burning_elite.has_emerald_key = true;
        let mut session = RunControlSession::new(RunControlConfig {
            final_act: true,
            ..RunControlConfig::default()
        });
        session.run_state.act_num = 3;
        session.run_state.keys[2] = false;
        session.run_state.event_state = None;
        session.run_state.current_hp = session.run_state.max_hp;
        session.run_state.map = MapState::new(vec![
            vec![key_path, misses_key],
            vec![burning_elite, node(1, 1, RoomType::EventRoom)],
        ]);
        session.engine_state = EngineState::MapNavigation;

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_route_policy_decision_v1(&session, &legal).expect("Act 3 Heart route");
        let first = decision.evidence.first().expect("preferred route");

        assert_eq!(first.band, RoutePolicyBandV1::HeartKeyDeadline);
        assert!(matches!(
            &first.action,
            RoutePolicyActionV1::Select { x: 0, path, .. }
                if path.emerald_key_reachable && path.emerald_key_required
        ));
    }

    #[test]
    fn mandatory_elite_is_not_relabelled_as_flexible_by_later_room_variation() {
        let action = RoutePolicyActionV1::Select {
            x: 1,
            y: 1,
            room_type: Some(PlannerMapRoom::Elite),
            question_mark_outcomes: Vec::new(),
            uses_wing_boots: false,
            has_emerald_key: false,
            actual_wing_boots_spent: 0,
            arrival: RoutePolicyArrivalV1::Combat,
            path: RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::CompleteWithinHorizon),
                observed_path_count: 2,
                min_elites: 1,
                max_elites: 1,
                min_campfires: 1,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        };

        assert_eq!(
            route_policy_band_v1(
                &action,
                &RoutePolicyContextV1 {
                    act: 2,
                    ascension: 0,
                    current_hp: 70,
                    max_hp: 80,
                    gold: 0,
                    critical_recovery: false,
                    recovery_pressure: false,
                    shop_conversion_support: StrategyPlanSupportV1::Blocked,
                    recent_combat_hp_loss: None,
                    boss_encounter_readiness: BossEncounterReadinessV1::default(),
                    returned_rewards_complete: false,
                }
            ),
            RoutePolicyBandV1::Ordinary
        );
    }

    #[test]
    fn healthy_a0_act1_forced_double_elite_does_not_outrank_optional_growth() {
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 84,
            max_hp: 85,
            gold: 37,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: None,
            boss_encounter_readiness: BossEncounterReadinessV1::default(),
            returned_rewards_complete: false,
        };
        let elite = route_action(
            RoomType::MonsterRoomElite,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                min_elites: 2,
                max_elites: 2,
                min_campfires: 1,
                max_campfires: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let event = route_action(
            RoomType::EventRoom,
            RoutePolicyArrivalV1::QuestionMark,
            RoutePolicyPathEvidenceV1 {
                min_elites: 0,
                max_elites: 1,
                min_campfires: 1,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );

        let elite_band = route_policy_band_v1(&elite, &context);
        let event_band = route_policy_band_v1(&event, &context);
        assert_eq!(elite_band, RoutePolicyBandV1::Ordinary);
        assert_eq!(event_band, RoutePolicyBandV1::FlexibleGrowth);
        assert!(event_band < elite_band);
    }

    #[test]
    fn a0_act1_immediate_rest_backing_admits_the_first_forced_elite() {
        let recovery_then_elite = route_action(
            RoomType::RestRoom,
            RoutePolicyArrivalV1::Campfire,
            RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::CompleteWithinHorizon),
                observed_path_count: 16,
                min_damage_rooms_before_recovery: 0,
                max_damage_rooms_before_recovery: 0,
                min_damage_rooms_to_recovery: Some(0),
                paths_with_recovery: 16,
                paths_with_recovery_before_damage: 16,
                projected_hp_after_rest: Some(85),
                first_elite: RouteFirstEliteSegment {
                    paths_with_first_elite: 16,
                    forced: true,
                    min_campfires_before: 1,
                    max_campfires_before: 1,
                    ..RouteFirstEliteSegment::default()
                },
                min_elites: 1,
                max_elites: 2,
                min_campfires: 2,
                max_campfires: 3,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let optional_growth = route_action(
            RoomType::EventRoom,
            RoutePolicyArrivalV1::QuestionMark,
            RoutePolicyPathEvidenceV1 {
                min_elites: 0,
                max_elites: 1,
                min_campfires: 1,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 71,
            max_hp: 85,
            gold: 115,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Plausible,
            recent_combat_hp_loss: Some(8),
            boss_encounter_readiness: BossEncounterReadinessV1 {
                preparation: BossEncounterPreparationBandV1::Established,
                timed_damage_race: StrategyCapabilityCoverageV1::Supported,
                sustained_defense: StrategyCapabilityCoverageV1::Thin,
                draw_energy_consistency: StrategyCapabilityCoverageV1::Missing,
                ..BossEncounterReadinessV1::default()
            },
            returned_rewards_complete: false,
        };

        let elite_band = route_policy_band_v1(&recovery_then_elite, &context);
        let optional_band = route_policy_band_v1(&optional_growth, &context);
        assert_eq!(elite_band, RoutePolicyBandV1::EliteGrowth);
        assert_eq!(optional_band, RoutePolicyBandV1::FlexibleGrowth);
        assert!(elite_band < optional_band);

        let mut rest_locked = recovery_then_elite;
        let RoutePolicyActionV1::Select { path, .. } = &mut rest_locked else {
            unreachable!()
        };
        path.projected_hp_after_rest = None;
        assert_eq!(
            route_policy_band_v1(&rest_locked, &context),
            RoutePolicyBandV1::Ordinary,
            "a visible campfire is not recovery backing when Rest is unavailable"
        );
    }

    #[test]
    fn healthy_a0_act1_single_forced_elite_keeps_growth_priority() {
        let action = route_action(
            RoomType::MonsterRoomElite,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                min_elites: 1,
                max_elites: 2,
                min_campfires: 1,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 84,
            max_hp: 85,
            gold: 37,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: None,
            boss_encounter_readiness: BossEncounterReadinessV1::default(),
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&action, &context),
            RoutePolicyBandV1::EliteGrowth
        );
    }

    #[test]
    fn healthy_a0_later_act_recovery_backed_elite_keeps_growth_priority() {
        let action = route_action(
            RoomType::TreasureRoom,
            RoutePolicyArrivalV1::Treasure,
            RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::CompleteWithinHorizon),
                observed_path_count: 3,
                min_damage_rooms_before_recovery: 0,
                max_damage_rooms_before_recovery: 0,
                min_damage_rooms_to_recovery: Some(0),
                paths_with_recovery: 3,
                paths_with_recovery_before_damage: 3,
                min_elites: 1,
                max_elites: 1,
                min_campfires: 2,
                max_campfires: 2,
                min_treasures: 1,
                max_treasures: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 2,
            ascension: 0,
            current_hp: 76,
            max_hp: 76,
            gold: 105,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Plausible,
            recent_combat_hp_loss: Some(0),
            boss_encounter_readiness: BossEncounterReadinessV1 {
                timed_damage_race: StrategyCapabilityCoverageV1::Strong,
                sustained_defense: StrategyCapabilityCoverageV1::Strong,
                draw_energy_consistency: StrategyCapabilityCoverageV1::Strong,
                ..BossEncounterReadinessV1::default()
            },
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&action, &context),
            RoutePolicyBandV1::EliteGrowth
        );
    }

    #[test]
    fn later_act_elite_growth_requires_complete_recovery_backing() {
        let action = route_action(
            RoomType::TreasureRoom,
            RoutePolicyArrivalV1::Treasure,
            RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::PartialPathBudget),
                observed_path_count: 3,
                paths_with_recovery: 3,
                paths_with_recovery_before_damage: 3,
                min_elites: 1,
                max_elites: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 2,
            ascension: 0,
            current_hp: 76,
            max_hp: 76,
            gold: 105,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: Some(0),
            boss_encounter_readiness: BossEncounterReadinessV1 {
                timed_damage_race: StrategyCapabilityCoverageV1::Strong,
                sustained_defense: StrategyCapabilityCoverageV1::Strong,
                draw_energy_consistency: StrategyCapabilityCoverageV1::Strong,
                ..BossEncounterReadinessV1::default()
            },
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&action, &context),
            RoutePolicyBandV1::Ordinary
        );
    }

    #[test]
    fn a0_act1_direct_elite_growth_requires_three_quarters_hp() {
        let action = route_action(
            RoomType::MonsterRoomElite,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                min_elites: 1,
                max_elites: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 63,
            max_hp: 85,
            gold: 0,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: None,
            boss_encounter_readiness: BossEncounterReadinessV1::default(),
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&action, &context),
            RoutePolicyBandV1::Ordinary
        );
    }

    #[test]
    fn potion_backed_slime_boss_plan_does_not_fund_direct_elite_growth() {
        let action = route_action(
            RoomType::MonsterRoomElite,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                min_elites: 1,
                max_elites: 1,
                min_campfires: 1,
                max_campfires: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 65,
            max_hp: 85,
            gold: 41,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: Some(7),
            boss_encounter_readiness: BossEncounterReadinessV1 {
                boss: Some(crate::content::monsters::factory::EncounterId::SlimeBoss),
                preparation: BossEncounterPreparationBandV1::PotionBacked,
                ..BossEncounterReadinessV1::default()
            },
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&action, &context),
            RoutePolicyBandV1::Ordinary
        );
    }

    #[test]
    fn established_slime_boss_plan_keeps_the_bounded_elite_growth_prior() {
        let action = route_action(
            RoomType::MonsterRoomElite,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                min_elites: 1,
                max_elites: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 65,
            max_hp: 85,
            gold: 41,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: Some(7),
            boss_encounter_readiness: BossEncounterReadinessV1 {
                boss: Some(crate::content::monsters::factory::EncounterId::SlimeBoss),
                preparation: BossEncounterPreparationBandV1::Established,
                ..BossEncounterReadinessV1::default()
            },
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&action, &context),
            RoutePolicyBandV1::EliteGrowth
        );
    }

    #[test]
    fn exposed_boss_plan_prefers_campfire_scope_over_optional_elite_count() {
        let monster = route_action(
            RoomType::MonsterRoom,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                min_elites: 0,
                max_elites: 2,
                min_campfires: 1,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let campfire = route_action(
            RoomType::RestRoom,
            RoutePolicyArrivalV1::Campfire,
            RoutePolicyPathEvidenceV1 {
                min_elites: 0,
                max_elites: 1,
                min_campfires: 3,
                max_campfires: 3,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 73,
            max_hp: 85,
            gold: 29,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: Some(10),
            boss_encounter_readiness: BossEncounterReadinessV1 {
                boss: Some(crate::content::monsters::factory::EncounterId::SlimeBoss),
                preparation: BossEncounterPreparationBandV1::Exposed,
                ..BossEncounterReadinessV1::default()
            },
            returned_rewards_complete: false,
        };

        assert_eq!(
            compare_route_actions(&campfire, &monster, &context),
            Ordering::Less
        );
    }

    #[test]
    fn forced_campfire_and_path_cardinality_do_not_rank_controllable_suffixes() {
        let monster = route_action(
            RoomType::MonsterRoom,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                observed_path_count: 21,
                min_damage_rooms_before_recovery: 2,
                max_damage_rooms_before_recovery: 8,
                min_elites: 0,
                max_elites: 2,
                min_campfires: 1,
                max_campfires: 3,
                min_treasures: 1,
                max_treasures: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let event = route_action(
            RoomType::EventRoom,
            RoutePolicyArrivalV1::QuestionMark,
            RoutePolicyPathEvidenceV1 {
                observed_path_count: 13,
                min_damage_rooms_before_recovery: 1,
                max_damage_rooms_before_recovery: 1,
                min_elites: 0,
                max_elites: 2,
                min_campfires: 2,
                max_campfires: 3,
                min_treasures: 1,
                max_treasures: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 2,
            ascension: 0,
            current_hp: 66,
            max_hp: 85,
            gold: 195,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Strong,
            recent_combat_hp_loss: Some(5),
            boss_encounter_readiness: BossEncounterReadinessV1::default(),
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&monster, &context),
            RoutePolicyBandV1::FlexibleGrowth
        );
        assert_eq!(
            route_policy_band_v1(&event, &context),
            RoutePolicyBandV1::FlexibleGrowth
        );
        assert_eq!(
            compare_route_actions(&event, &monster, &context),
            Ordering::Equal,
            "future suffixes are player choices: neither a forced campfire nor representation cardinality is a probability or intrinsic reward"
        );
    }

    #[test]
    fn supported_act1_hallway_development_beats_a_forced_extra_campfire() {
        let hallway = route_action(
            RoomType::MonsterRoom,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::CompleteWithinHorizon),
                observed_path_count: 11,
                min_damage_rooms_before_recovery: 1,
                max_damage_rooms_before_recovery: 5,
                min_damage_rooms_to_recovery: Some(1),
                paths_with_recovery: 11,
                projected_hp_after_rest: Some(82),
                min_elites: 0,
                max_elites: 1,
                min_campfires: 1,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let event = route_action(
            RoomType::EventRoom,
            RoutePolicyArrivalV1::QuestionMark,
            RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::CompleteWithinHorizon),
                observed_path_count: 5,
                min_damage_rooms_before_recovery: 0,
                max_damage_rooms_before_recovery: 0,
                min_damage_rooms_to_recovery: Some(0),
                paths_with_recovery: 5,
                paths_with_recovery_before_damage: 5,
                projected_hp_after_rest: Some(82),
                min_elites: 0,
                max_elites: 1,
                min_campfires: 2,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 58,
            max_hp: 82,
            gold: 123,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: Some(0),
            boss_encounter_readiness: BossEncounterReadinessV1 {
                boss: Some(crate::content::monsters::factory::EncounterId::SlimeBoss),
                preparation: BossEncounterPreparationBandV1::Established,
                boss_threat_gap_count: 3,
                ..BossEncounterReadinessV1::default()
            },
            returned_rewards_complete: false,
        };

        assert_eq!(
            compare_route_actions(&hallway, &event, &context),
            Ordering::Less,
            "the hallway has exact low-attrition support, fills public capability gaps, and retains a selectable next-fire recovery outlet"
        );
    }

    #[test]
    fn high_recent_raw_attrition_blocks_the_elite_growth_label() {
        let action = route_action(
            RoomType::MonsterRoomElite,
            RoutePolicyArrivalV1::Combat,
            RoutePolicyPathEvidenceV1 {
                min_elites: 1,
                max_elites: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let context = RoutePolicyContextV1 {
            act: 1,
            ascension: 0,
            current_hp: 84,
            max_hp: 85,
            gold: 0,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Blocked,
            recent_combat_hp_loss: Some(15),
            boss_encounter_readiness: BossEncounterReadinessV1::default(),
            returned_rewards_complete: false,
        };

        assert_eq!(
            route_policy_band_v1(&action, &context),
            RoutePolicyBandV1::Ordinary
        );
    }

    #[test]
    fn map_returns_only_when_reward_policy_has_remaining_work() {
        use crate::content::cards::CardId;
        use crate::state::rewards::{RewardCard, RewardItem, RewardState};

        for (items, should_return) in [
            (vec![], false),
            (
                vec![RewardItem::Card {
                    cards: vec![RewardCard::new(CardId::Injury, 0)],
                }],
                false,
            ),
            (vec![RewardItem::Gold { amount: 11 }], true),
            (
                vec![RewardItem::Card {
                    cards: vec![RewardCard::new(CardId::ShrugItOff, 0)],
                }],
                true,
            ),
        ] {
            let mut session = two_route_session();
            let mut reward = RewardState::new();
            reward.items = items;
            session.engine_state = EngineState::map_overlay(EngineState::RewardScreen(reward));
            let surface = build_decision_surface(&session);
            let legal = policy_candidates(&surface);
            let decision = exact_route_policy_decision_v1(&session, &legal).unwrap();
            assert_eq!(
                matches!(
                    decision.evidence[0].action,
                    RoutePolicyActionV1::CancelToPendingRewards
                ),
                should_return
            );
            assert!(decision.prior.entries.iter().all(|e| e.probability > 0.0));
            assert!(surface
                .view
                .candidates
                .iter()
                .any(|c| c.key == Some(DecisionCandidateKey::RouteCancel)));
        }
    }

    #[test]
    fn full_potion_belt_does_not_loop_back_to_an_unclaimable_reward() {
        use crate::content::potions::{Potion, PotionId};
        use crate::state::rewards::{RewardItem, RewardState};

        let mut session = two_route_session();
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::WeakenPotion, 1)),
            Some(Potion::new(PotionId::FearPotion, 2)),
            Some(Potion::new(PotionId::DexterityPotion, 3)),
        ];
        let mut reward = RewardState::new();
        reward.items.push(RewardItem::Potion {
            potion_id: PotionId::FirePotion,
        });
        session.engine_state = EngineState::map_overlay(EngineState::RewardScreen(reward));
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);

        let audit = exact_route_policy_audit_v1(&session, &legal)
            .expect("public route audit accepts the complete mixed map surface");

        assert!(matches!(
            audit.candidates.first().map(|evidence| &evidence.action),
            Some(RoutePolicyActionV1::Select { .. })
        ));
        assert_eq!(
            audit.candidates.last().map(|evidence| evidence.band),
            Some(RoutePolicyBandV1::ContinueAfterRewards)
        );
    }

    #[test]
    fn funded_shop_is_a_real_liquidity_conversion_not_ordinary_path_noise() {
        let session = shop_or_hallway_session(237);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_route_policy_decision_v1(&session, &legal).expect("exact route decision");

        assert_eq!(
            decision.context.shop_conversion_support,
            StrategyPlanSupportV1::Strong
        );
        assert!(matches!(
            decision.evidence.first(),
            Some(RoutePolicyActionEvidenceV1 {
                action: RoutePolicyActionV1::Select {
                    room_type: Some(PlannerMapRoom::Shop),
                    ..
                },
                band: RoutePolicyBandV1::LiquidityConversion,
                ..
            })
        ));
    }

    #[test]
    fn funded_shop_with_a_forced_elite_does_not_outrank_a_flexible_safe_route() {
        let context = RoutePolicyContextV1 {
            act: 2,
            ascension: 0,
            current_hp: 72,
            max_hp: 90,
            gold: 129,
            critical_recovery: false,
            recovery_pressure: false,
            shop_conversion_support: StrategyPlanSupportV1::Plausible,
            recent_combat_hp_loss: None,
            boss_encounter_readiness: BossEncounterReadinessV1::default(),
            returned_rewards_complete: false,
        };
        let shop = route_action(
            RoomType::ShopRoom,
            RoutePolicyArrivalV1::Shop,
            RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::CompleteWithinHorizon),
                observed_path_count: 1,
                min_elites: 1,
                max_elites: 1,
                min_shops: 1,
                max_shops: 1,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );
        let flexible = route_action(
            RoomType::EventRoom,
            RoutePolicyArrivalV1::QuestionMark,
            RoutePolicyPathEvidenceV1 {
                coverage: Some(RouteWindowCoverageKind::CompleteWithinHorizon),
                observed_path_count: 2,
                min_elites: 0,
                max_elites: 1,
                min_campfires: 1,
                max_campfires: 2,
                ..RoutePolicyPathEvidenceV1::default()
            },
        );

        let shop_band = route_policy_band_v1(&shop, &context);
        let flexible_band = route_policy_band_v1(&flexible, &context);

        assert_eq!(shop_band, RoutePolicyBandV1::Ordinary);
        assert_eq!(flexible_band, RoutePolicyBandV1::FlexibleGrowth);
        assert!(flexible_band < shop_band);
    }

    #[test]
    fn shop_is_not_reported_as_guaranteed_recovery() {
        let stats = summarize_path(&RouteWindowPath {
            nodes: vec![
                crate::ai::route_window_facts::RouteWindowNode {
                    x: 0,
                    y: 0,
                    room_type: Some(RoomType::ShopRoom),
                },
                crate::ai::route_window_facts::RouteWindowNode {
                    x: 0,
                    y: 1,
                    room_type: Some(RoomType::MonsterRoomElite),
                },
            ],
        });

        assert_eq!(stats.shops, 1);
        assert_eq!(stats.elites, 1);
        assert_eq!(stats.damage_before_recovery, 1);
        assert!(!stats.recovered);
        assert!(!stats.recovery_before_damage);
    }

    #[test]
    fn unfunded_shop_does_not_receive_liquidity_conversion_priority() {
        let session = shop_or_hallway_session(0);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_route_policy_decision_v1(&session, &legal).expect("exact route decision");
        let shop = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.action,
                    RoutePolicyActionV1::Select {
                        room_type: Some(PlannerMapRoom::Shop),
                        ..
                    }
                )
            })
            .expect("shop route");

        assert_eq!(
            decision.context.shop_conversion_support,
            StrategyPlanSupportV1::Blocked
        );
        assert_ne!(shop.band, RoutePolicyBandV1::LiquidityConversion);
    }
}
