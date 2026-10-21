use serde::{Deserialize, Serialize};

use crate::ai::route_window_facts::{
    build_route_window_facts, RouteWindowFacts, RouteWindowFactsConfig,
};
use crate::content::monsters::factory::EncounterId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicState;
use crate::engine::campfire_candidates::{legal_campfire_candidates, CampfireCandidate};
use crate::eval::campfire_projection::{
    project_campfire_candidate, CampfireProjection, CampfireProjectionError,
};
use crate::eval::fingerprint::{hash_serializable, FINGERPRINT_ALGORITHM_JSON};
use crate::runtime::combat::CombatCard;
use crate::state::run::RunState;

mod growth;
mod run_feasibility;

use growth::{assess_growth, build_growth_facts};
pub use growth::{
    CampfireGrowth, CampfireGrowthError, CampfireLiftGrowth, CampfireSmithGrowth,
    CampfireTokeGrowth,
};
use run_feasibility::assess_run_feasibility;
pub use run_feasibility::{CampfireRubyKeyObligation, CampfireRunFeasibility};

pub const CAMPFIRE_EVALUATION_CONTEXT_SCHEMA_NAME: &str = "CampfireEvaluationContextV1";
pub const CAMPFIRE_EVALUATION_CONTEXT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireRunGoal {
    Act3Victory,
    HeartEligibility,
    HeartVictory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CampfireEvaluationHorizon {
    UntilNextCampfireOrActTerminal { route_horizon_nodes: usize },
}

impl CampfireEvaluationHorizon {
    pub fn route_horizon_nodes(self) -> usize {
        match self {
            Self::UntilNextCampfireOrActTerminal {
                route_horizon_nodes,
            } => route_horizon_nodes,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireContinuationProfile {
    /// Identifies one frozen serialization of combat, route, reward, event,
    /// shop, and other continuation-policy configuration.
    pub profile_id: String,
    /// Identifies the source tree that implements the frozen profile. Callers
    /// provide this explicitly; evaluation never shells out to Git.
    pub source_identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireEvaluationSpec {
    pub run_goal: CampfireRunGoal,
    pub horizon: CampfireEvaluationHorizon,
    pub route_path_budget: usize,
    pub continuation_profile: CampfireContinuationProfile,
    pub public_scenario_distribution_id: String,
    pub mechanics_version: String,
}

impl CampfireEvaluationSpec {
    fn validate(&self) -> Result<(), CampfireEvaluationSpecError> {
        if self.horizon.route_horizon_nodes() == 0 {
            return Err(CampfireEvaluationSpecError::ZeroRouteHorizon);
        }
        if self.route_path_budget == 0 {
            return Err(CampfireEvaluationSpecError::ZeroRoutePathBudget);
        }
        if self.continuation_profile.profile_id.trim().is_empty() {
            return Err(CampfireEvaluationSpecError::EmptyContinuationProfileId);
        }
        if self.continuation_profile.source_identity.trim().is_empty() {
            return Err(CampfireEvaluationSpecError::EmptySourceIdentity);
        }
        if self.public_scenario_distribution_id.trim().is_empty() {
            return Err(CampfireEvaluationSpecError::EmptyScenarioDistributionId);
        }
        if self.mechanics_version.trim().is_empty() {
            return Err(CampfireEvaluationSpecError::EmptyMechanicsVersion);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampfireEvaluationSpecError {
    ZeroRouteHorizon,
    ZeroRoutePathBudget,
    EmptyContinuationProfileId,
    EmptySourceIdentity,
    EmptyScenarioDistributionId,
    EmptyMechanicsVersion,
}

/// The Campfire-local public information cutoff. Hidden RNG streams, seed,
/// ordered reward pools, and future encounter queues are deliberately absent.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfirePublicRootObservationV1 {
    pub ascension_level: u8,
    pub is_daily_run: bool,
    pub act_num: u8,
    pub floor_num: i32,
    pub player_class: String,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub shop_purge_count: i32,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<CampfirePublicPotionObservationV1>>,
    pub keys: [bool; 3],
    pub is_final_act_available: bool,
    pub master_deck: Vec<CombatCard>,
    pub card_upgraded_chance: f32,
    pub boss_key: Option<EncounterId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfirePublicPotionObservationV1 {
    pub id: PotionId,
    pub can_use: bool,
    pub can_discard: bool,
    pub requires_target: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireEvaluationContext {
    pub schema_name: String,
    pub schema_version: u32,
    pub fingerprint_algorithm: String,
    pub public_root: CampfirePublicRootObservationV1,
    pub root_public_fingerprint: String,
    pub route_window_config: RouteWindowFactsConfig,
    pub route_window_facts: RouteWindowFacts,
    pub route_window_fingerprint: String,
    pub spec: CampfireEvaluationSpec,
    pub context_fingerprint: String,
}

#[derive(Serialize)]
struct CampfireEvaluationContextFingerprintInput<'a> {
    schema_name: &'static str,
    schema_version: u32,
    root_public_fingerprint: &'a str,
    route_window_config: &'a RouteWindowFactsConfig,
    route_window_fingerprint: &'a str,
    spec: &'a CampfireEvaluationSpec,
}

pub fn build_campfire_evaluation_context(
    root: &RunState,
    spec: CampfireEvaluationSpec,
) -> Result<CampfireEvaluationContext, CampfireEvaluationSpecError> {
    spec.validate()?;
    let public_root = public_root_observation(root);
    let root_public_fingerprint = hash_serializable(&public_root);
    let route_window_config = RouteWindowFactsConfig {
        horizon_nodes: spec.horizon.route_horizon_nodes(),
        path_budget: spec.route_path_budget,
    };
    let route_window_facts = build_route_window_facts(root, route_window_config.clone());
    let route_window_fingerprint = hash_serializable(&route_window_facts);
    let context_fingerprint = hash_serializable(&CampfireEvaluationContextFingerprintInput {
        schema_name: CAMPFIRE_EVALUATION_CONTEXT_SCHEMA_NAME,
        schema_version: CAMPFIRE_EVALUATION_CONTEXT_SCHEMA_VERSION,
        root_public_fingerprint: &root_public_fingerprint,
        route_window_config: &route_window_config,
        route_window_fingerprint: &route_window_fingerprint,
        spec: &spec,
    });

    Ok(CampfireEvaluationContext {
        schema_name: CAMPFIRE_EVALUATION_CONTEXT_SCHEMA_NAME.to_string(),
        schema_version: CAMPFIRE_EVALUATION_CONTEXT_SCHEMA_VERSION,
        fingerprint_algorithm: FINGERPRINT_ALGORITHM_JSON.to_string(),
        public_root,
        root_public_fingerprint,
        route_window_config,
        route_window_facts,
        route_window_fingerprint,
        spec,
        context_fingerprint,
    })
}

fn public_root_observation(root: &RunState) -> CampfirePublicRootObservationV1 {
    CampfirePublicRootObservationV1 {
        ascension_level: root.ascension_level,
        is_daily_run: root.is_daily_run,
        act_num: root.act_num,
        floor_num: root.floor_num,
        player_class: root.player_class.to_string(),
        current_hp: root.current_hp,
        max_hp: root.max_hp,
        gold: root.gold,
        shop_purge_count: root.shop_purge_count,
        relics: root.relics.clone(),
        potions: root
            .potions
            .iter()
            .map(|slot| {
                slot.as_ref()
                    .map(|potion| CampfirePublicPotionObservationV1 {
                        id: potion.id,
                        can_use: potion.can_use,
                        can_discard: potion.can_discard,
                        requires_target: potion.requires_target,
                    })
            })
            .collect(),
        keys: root.keys,
        is_final_act_available: root.is_final_act_available,
        master_deck: root.master_deck.clone(),
        card_upgraded_chance: root.card_upgraded_chance,
        boss_key: root.boss_key,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireProspectField {
    ImmediateHp,
    RunFeasibility,
    SurvivalDistribution,
    ThreatResolutionDistribution,
    GrowthDistribution,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireEvidenceStatus {
    Exact,
    Calibrated,
    Partial,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireEvidenceProvenance {
    PublicRootObservation,
    EngineTransition,
    EngineTransitionAndUpgradePlanner,
    EngineTransitionAndDeckMutationCompiler,
    EngineTransitionAndRelicMechanics,
    PublicRootAndRouteWindowFacts,
    NoProducer,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireEvidenceLimitation {
    OtherHeartKeysNotEvaluated,
    VisibleDeadlineNotProven,
    FutureRecallDecisionNotEvaluated,
    ChanceOutcomeCouldChangeRouteAccess,
    SurvivalWindowNotEvaluated,
    ThreatTimingNotEvaluated,
    GrowthNotEvaluated,
    DownstreamGrowthDistributionNotEvaluated,
    ChanceOutcomeNotEstimated,
    PostRevealRecourseNotEvaluated,
}

/// Coverage metadata only. Numeric or distributional values live in their
/// typed prospect fields, so `Unsupported` cannot silently carry a zero value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireFieldEvidence {
    pub field: CampfireProspectField,
    pub status: CampfireEvidenceStatus,
    pub provenance: CampfireEvidenceProvenance,
    pub limitations: Vec<CampfireEvidenceLimitation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireImmediateHp {
    pub before: i32,
    pub after: i32,
}

impl CampfireImmediateHp {
    pub fn delta(self) -> i32 {
        self.after - self.before
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireCandidateEvaluation {
    pub candidate: CampfireCandidate,
    pub projection: CampfireProjection,
    pub immediate_hp: CampfireImmediateHp,
    pub run_feasibility: CampfireRunFeasibility,
    pub growth: CampfireGrowth,
    pub field_evidence: Vec<CampfireFieldEvidence>,
}

impl CampfireCandidateEvaluation {
    pub fn evidence_for(&self, field: CampfireProspectField) -> Option<&CampfireFieldEvidence> {
        self.field_evidence
            .iter()
            .find(|evidence| evidence.field == field)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireEvaluationBatch {
    pub context: CampfireEvaluationContext,
    pub candidates: Vec<CampfireCandidateEvaluation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampfireEvaluationError {
    InvalidSpec(CampfireEvaluationSpecError),
    Projection {
        candidate: CampfireCandidate,
        source: CampfireProjectionError,
    },
    Growth {
        candidate: CampfireCandidate,
        source: CampfireGrowthError,
    },
}

impl From<CampfireEvaluationSpecError> for CampfireEvaluationError {
    fn from(value: CampfireEvaluationSpecError) -> Self {
        Self::InvalidSpec(value)
    }
}

pub fn build_campfire_evaluation_batch(
    root: &RunState,
    spec: CampfireEvaluationSpec,
) -> Result<CampfireEvaluationBatch, CampfireEvaluationError> {
    let context = build_campfire_evaluation_context(root, spec)?;
    let growth_facts = build_growth_facts(root);
    let mut candidates = Vec::new();
    for candidate in legal_campfire_candidates(root) {
        let projection = project_campfire_candidate(root, candidate)
            .map_err(|source| CampfireEvaluationError::Projection { candidate, source })?;
        let immediate_hp = immediate_hp(root, &projection);
        let run_feasibility = assess_run_feasibility(root, &context, candidate, &projection);
        let growth = assess_growth(root, &growth_facts, candidate, &projection)
            .map_err(|source| CampfireEvaluationError::Growth { candidate, source })?;
        let field_evidence = field_evidence(&projection, run_feasibility.evidence, growth.evidence);
        candidates.push(CampfireCandidateEvaluation {
            candidate,
            projection,
            immediate_hp,
            run_feasibility: run_feasibility.value,
            growth: growth.value,
            field_evidence,
        });
    }
    Ok(CampfireEvaluationBatch {
        context,
        candidates,
    })
}

fn immediate_hp(root: &RunState, projection: &CampfireProjection) -> CampfireImmediateHp {
    let after = match projection {
        CampfireProjection::Exact(exact) => exact.run_state.current_hp,
        CampfireProjection::Chance(chance) => chance.exact_prefix.hp_after,
        CampfireProjection::ChanceThenDecision(recourse) => recourse.exact_prefix.hp_after,
    };
    CampfireImmediateHp {
        before: root.current_hp,
        after,
    }
}

fn field_evidence(
    projection: &CampfireProjection,
    run_feasibility: CampfireFieldEvidence,
    growth: CampfireFieldEvidence,
) -> Vec<CampfireFieldEvidence> {
    let mut future_limitations = Vec::new();
    match projection {
        CampfireProjection::Exact(_) => {}
        CampfireProjection::Chance(_) => {
            future_limitations.push(CampfireEvidenceLimitation::ChanceOutcomeNotEstimated);
        }
        CampfireProjection::ChanceThenDecision(_) => {
            future_limitations.push(CampfireEvidenceLimitation::ChanceOutcomeNotEstimated);
            future_limitations.push(CampfireEvidenceLimitation::PostRevealRecourseNotEvaluated);
        }
    }

    vec![
        CampfireFieldEvidence {
            field: CampfireProspectField::ImmediateHp,
            status: CampfireEvidenceStatus::Exact,
            provenance: CampfireEvidenceProvenance::EngineTransition,
            limitations: Vec::new(),
        },
        run_feasibility,
        unsupported_evidence(
            CampfireProspectField::SurvivalDistribution,
            with_future_limitations(
                CampfireEvidenceLimitation::SurvivalWindowNotEvaluated,
                &future_limitations,
            ),
        ),
        unsupported_evidence(
            CampfireProspectField::ThreatResolutionDistribution,
            with_future_limitations(
                CampfireEvidenceLimitation::ThreatTimingNotEvaluated,
                &future_limitations,
            ),
        ),
        growth,
    ]
}

fn unsupported_evidence(
    field: CampfireProspectField,
    limitations: Vec<CampfireEvidenceLimitation>,
) -> CampfireFieldEvidence {
    CampfireFieldEvidence {
        field,
        status: CampfireEvidenceStatus::Unsupported,
        provenance: CampfireEvidenceProvenance::NoProducer,
        limitations,
    }
}

fn with_future_limitations(
    primary: CampfireEvidenceLimitation,
    future: &[CampfireEvidenceLimitation],
) -> Vec<CampfireEvidenceLimitation> {
    let mut limitations = vec![primary];
    limitations.extend_from_slice(future);
    limitations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::deck_mutation_compiler_v1::{
        DeckMutationTargetClassV1, DeckMutationTargetLossTierV1,
    };
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::campfire_candidates::{legal_campfire_candidates, CampfireCandidate};
    use crate::runtime::combat::CombatCard;
    use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};
    use crate::state::map::state::MapState;
    use crate::state::run::RunState;

    fn evaluation_spec() -> CampfireEvaluationSpec {
        CampfireEvaluationSpec {
            run_goal: CampfireRunGoal::HeartVictory,
            horizon: CampfireEvaluationHorizon::UntilNextCampfireOrActTerminal {
                route_horizon_nodes: 5,
            },
            route_path_budget: 2_000,
            continuation_profile: CampfireContinuationProfile {
                profile_id: "bounded-mainline-v1".to_string(),
                source_identity: "test-source".to_string(),
            },
            public_scenario_distribution_id: "public-eligible-outcomes-v1".to_string(),
            mechanics_version: "sts-simulator-test-v1".to_string(),
        }
    }

    fn candidate_run() -> RunState {
        let mut run = RunState::new(17, 0, true, "Ironclad");
        run.current_hp = 20;
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 101),
            CombatCard::for_scenario(CardId::Defend, 102),
            CombatCard::for_scenario(CardId::AscendersBane, 103),
        ];
        run.relics = vec![
            RelicState::new(RelicId::DreamCatcher),
            RelicState::new(RelicId::Girya),
            RelicState::new(RelicId::Shovel),
            RelicState::new(RelicId::PeacePipe),
        ];
        run.potions = vec![Some(Potion::new(PotionId::DexterityPotion, 401))];
        run.keys[0] = false;
        run
    }

    fn run_with_next_rooms(next_rooms: &[RoomType]) -> RunState {
        let mut run = candidate_run();
        run.act_num = 3;
        let mut current = MapRoomNode::new(0, 0);
        current.class = Some(RoomType::RestRoom);
        let next_row = next_rooms
            .iter()
            .enumerate()
            .map(|(x, room)| {
                current.edges.insert(MapEdge::new(0, 0, x as i32, 1));
                let mut node = MapRoomNode::new(x as i32, 1);
                node.class = Some(*room);
                node
            })
            .collect::<Vec<_>>();
        run.map = MapState::new(vec![vec![current], next_row]);
        run.map.current_x = 0;
        run.map.current_y = 0;
        run
    }

    fn run_at_last_visible_fire(act_num: u8) -> RunState {
        let mut run = candidate_run();
        run.act_num = act_num;
        let mut graph = (0..=14)
            .map(|y| {
                let mut node = MapRoomNode::new(0, y);
                node.class = Some(RoomType::MonsterRoom);
                vec![node]
            })
            .collect::<Vec<_>>();
        graph[14][0].class = Some(RoomType::RestRoom);
        run.map = MapState::new(graph);
        run.map.current_x = 0;
        run.map.current_y = 14;
        run
    }

    #[test]
    fn context_fingerprint_ignores_hidden_rng_and_hidden_future_order() {
        let root = candidate_run();
        let mut hidden_variant = root.clone();
        hidden_variant.rng_pool.card_rng.random(999);
        hidden_variant.rng_pool.relic_rng.random(999);
        if let Some(rng) = hidden_variant.neow_rng.as_mut() {
            rng.random(999);
        }
        hidden_variant.common_relic_pool.reverse();
        hidden_variant.uncommon_relic_pool.reverse();
        hidden_variant.monster_list.reverse();
        hidden_variant.elite_monster_list.reverse();
        hidden_variant.boss_list.reverse();
        hidden_variant.potions[0].as_mut().unwrap().uuid = 999_999;

        let first = build_campfire_evaluation_context(&root, evaluation_spec()).unwrap();
        let second = build_campfire_evaluation_context(&hidden_variant, evaluation_spec()).unwrap();

        assert_eq!(first.public_root, second.public_root);
        assert_eq!(
            first.root_public_fingerprint,
            second.root_public_fingerprint
        );
        assert_eq!(
            first.route_window_fingerprint,
            second.route_window_fingerprint
        );
        assert_eq!(first.context_fingerprint, second.context_fingerprint);
    }

    #[test]
    fn context_fingerprint_changes_with_public_hp_or_deck_state() {
        let root = candidate_run();
        let baseline = build_campfire_evaluation_context(&root, evaluation_spec()).unwrap();

        let mut lower_hp = root.clone();
        lower_hp.current_hp -= 1;
        let lower_hp = build_campfire_evaluation_context(&lower_hp, evaluation_spec()).unwrap();

        let mut upgraded = root.clone();
        upgraded.master_deck[0].upgrades += 1;
        let upgraded = build_campfire_evaluation_context(&upgraded, evaluation_spec()).unwrap();

        assert_ne!(
            baseline.root_public_fingerprint,
            lower_hp.root_public_fingerprint
        );
        assert_ne!(baseline.context_fingerprint, lower_hp.context_fingerprint);
        assert_ne!(
            baseline.root_public_fingerprint,
            upgraded.root_public_fingerprint
        );
        assert_ne!(baseline.context_fingerprint, upgraded.context_fingerprint);
    }

    #[test]
    fn batch_projects_every_legal_candidate_once_under_one_context() {
        let root = candidate_run();
        let expected = legal_campfire_candidates(&root);

        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let actual = batch
            .candidates
            .iter()
            .map(|candidate| candidate.candidate)
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
        assert_eq!(batch.candidates.len(), expected.len());
        assert!(!batch.context.context_fingerprint.is_empty());
    }

    #[test]
    fn batch_records_exact_immediate_hp_but_no_imaginary_future_values() {
        let root = candidate_run();
        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let rest = batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();

        assert_eq!(rest.immediate_hp.before, 20);
        assert_eq!(rest.immediate_hp.after, 44);
        assert_eq!(rest.immediate_hp.delta(), 24);
        assert_eq!(
            rest.evidence_for(CampfireProspectField::ImmediateHp)
                .unwrap()
                .status,
            CampfireEvidenceStatus::Exact
        );
        for field in [
            CampfireProspectField::SurvivalDistribution,
            CampfireProspectField::ThreatResolutionDistribution,
            CampfireProspectField::GrowthDistribution,
        ] {
            assert_eq!(
                rest.evidence_for(field).unwrap().status,
                CampfireEvidenceStatus::Unsupported
            );
        }
        let growth = rest
            .evidence_for(CampfireProspectField::GrowthDistribution)
            .unwrap();
        assert!(growth
            .limitations
            .contains(&CampfireEvidenceLimitation::ChanceOutcomeNotEstimated));
        assert!(growth
            .limitations
            .contains(&CampfireEvidenceLimitation::PostRevealRecourseNotEvaluated));
    }

    #[test]
    fn smith_growth_binds_typed_planner_facts_to_exact_lineage_projection() {
        let root = candidate_run();
        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let smith = batch
            .candidates
            .iter()
            .find(|candidate| {
                candidate.candidate
                    == CampfireCandidate::Smith {
                        card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(
                            101,
                        ),
                    }
            })
            .expect("Strike should be a legal Smith candidate");
        let growth = smith
            .growth
            .smith
            .as_ref()
            .expect("Smith should expose typed growth facts");

        assert_eq!(
            growth.card_lineage,
            crate::test_support::scenario_lineage(101)
        );
        assert_eq!(growth.card, CardId::Strike);
        assert_eq!(growth.upgrades_before, 0);
        assert_eq!(growth.upgrades_after, 1);
        assert_eq!(growth.mechanical_delta.damage_delta, 3);
        let typed_copy = CampfireSmithGrowth {
            card_lineage: growth.card_lineage,
            card: growth.card,
            upgrades_before: growth.upgrades_before,
            upgrades_after: growth.upgrades_after,
            mechanical_delta: growth.mechanical_delta.clone(),
            roles: growth.roles.clone(),
            pays_debts: growth.pays_debts.clone(),
            opportunity_costs: growth.opportunity_costs.clone(),
            urgency: growth.urgency,
            verdict: growth.verdict,
        };
        assert_eq!(&typed_copy, growth);
        let evidence = smith
            .evidence_for(CampfireProspectField::GrowthDistribution)
            .unwrap();
        assert_eq!(evidence.status, CampfireEvidenceStatus::Partial);
        assert_eq!(
            evidence.provenance,
            CampfireEvidenceProvenance::EngineTransitionAndUpgradePlanner
        );
        assert!(evidence
            .limitations
            .contains(&CampfireEvidenceLimitation::DownstreamGrowthDistributionNotEvaluated));

        let rest = batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();
        assert!(rest.growth.smith.is_none());
        assert_eq!(
            rest.evidence_for(CampfireProspectField::GrowthDistribution)
                .unwrap()
                .status,
            CampfireEvidenceStatus::Unsupported
        );
    }

    #[test]
    fn toke_growth_binds_typed_removal_facts_to_exact_lineage_projection() {
        let root = candidate_run();
        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let toke = batch
            .candidates
            .iter()
            .find(|candidate| {
                candidate.candidate
                    == CampfireCandidate::Toke {
                        card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(
                            101,
                        ),
                    }
            })
            .expect("Strike should be a legal Toke candidate");
        let growth = toke
            .growth
            .toke
            .as_ref()
            .expect("Toke should expose typed removal facts");

        assert_eq!(
            growth.card_lineage,
            crate::test_support::scenario_lineage(101)
        );
        assert_eq!(growth.card, CardId::Strike);
        assert_eq!(growth.upgrades, 0);
        assert_eq!(growth.deck_size_before, 3);
        assert_eq!(growth.deck_size_after, 2);
        assert_eq!(
            growth.target_class,
            DeckMutationTargetClassV1::StarterStrike
        );
        assert_eq!(
            growth.target_loss.tier,
            DeckMutationTargetLossTierV1::LowValue
        );
        let evidence = toke
            .evidence_for(CampfireProspectField::GrowthDistribution)
            .unwrap();
        assert_eq!(evidence.status, CampfireEvidenceStatus::Partial);
        assert_eq!(
            evidence.provenance,
            CampfireEvidenceProvenance::EngineTransitionAndDeckMutationCompiler
        );
        assert!(evidence
            .limitations
            .contains(&CampfireEvidenceLimitation::DownstreamGrowthDistributionNotEvaluated));
    }

    #[test]
    fn lift_growth_binds_girya_strength_to_exact_projection() {
        let root = candidate_run();
        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let candidate = batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Lift)
            .expect("Girya should make Lift a legal candidate");
        let growth = candidate
            .growth
            .lift
            .as_ref()
            .expect("Lift should expose typed Girya growth facts");

        assert_eq!(growth.girya_counter_before, 0);
        assert_eq!(growth.girya_counter_after, 1);
        assert_eq!(growth.combat_start_strength_before, 0);
        assert_eq!(growth.combat_start_strength_after, 1);
        assert_eq!(growth.combat_start_strength_delta, 1);
        let typed_copy = CampfireLiftGrowth {
            girya_counter_before: growth.girya_counter_before,
            girya_counter_after: growth.girya_counter_after,
            combat_start_strength_before: growth.combat_start_strength_before,
            combat_start_strength_after: growth.combat_start_strength_after,
            combat_start_strength_delta: growth.combat_start_strength_delta,
        };
        assert_eq!(&typed_copy, growth);

        let evidence = candidate
            .evidence_for(CampfireProspectField::GrowthDistribution)
            .unwrap();
        assert_eq!(evidence.status, CampfireEvidenceStatus::Partial);
        assert_eq!(
            evidence.provenance,
            CampfireEvidenceProvenance::EngineTransitionAndRelicMechanics
        );
        assert!(evidence
            .limitations
            .contains(&CampfireEvidenceLimitation::DownstreamGrowthDistributionNotEvaluated));
    }

    #[test]
    fn immediate_ruby_key_obligations_follow_declared_goal_and_projection() {
        let root = candidate_run();
        let mut act3_spec = evaluation_spec();
        act3_spec.run_goal = CampfireRunGoal::Act3Victory;
        let act3_batch = build_campfire_evaluation_batch(&root, act3_spec).unwrap();
        let act3_rest = act3_batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();
        assert_eq!(
            act3_rest.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::NotRequired
        );
        assert_eq!(
            act3_rest
                .evidence_for(CampfireProspectField::RunFeasibility)
                .unwrap()
                .status,
            CampfireEvidenceStatus::Exact
        );

        let heart_batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let recall = heart_batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Recall)
            .unwrap();
        assert_eq!(
            recall.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::SatisfiedByCandidate
        );
        let CampfireProjection::Exact(recall_projection) = &recall.projection else {
            panic!("Recall must be an exact projection");
        };
        assert!(recall_projection.run_state.keys[0]);
        assert_eq!(
            recall
                .evidence_for(CampfireProspectField::RunFeasibility)
                .unwrap()
                .status,
            CampfireEvidenceStatus::Partial
        );

        let mut all_keys_held = root.clone();
        all_keys_held.keys = [true, true, true];
        let held_batch =
            build_campfire_evaluation_batch(&all_keys_held, evaluation_spec()).unwrap();
        let held_rest = held_batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();
        assert_eq!(
            held_rest.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::AlreadySatisfied
        );
        assert_eq!(
            held_rest
                .evidence_for(CampfireProspectField::RunFeasibility)
                .unwrap()
                .status,
            CampfireEvidenceStatus::Exact
        );
    }

    #[test]
    fn future_campfire_modality_distinguishes_every_path_from_some_paths() {
        let every_root = run_with_next_rooms(&[RoomType::RestRoom]);
        let every_batch = build_campfire_evaluation_batch(&every_root, evaluation_spec()).unwrap();
        let every_rest = every_batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();
        assert_eq!(
            every_rest.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::DeferrableOnEveryVisiblePath
        );

        let some_root = run_with_next_rooms(&[RoomType::RestRoom, RoomType::MonsterRoom]);
        let some_batch = build_campfire_evaluation_batch(&some_root, evaluation_spec()).unwrap();
        let some_rest = some_batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();
        assert_eq!(
            some_rest.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::DeferrableOnSomeVisiblePath
        );
        assert!(some_rest
            .evidence_for(CampfireProspectField::RunFeasibility)
            .unwrap()
            .limitations
            .contains(&CampfireEvidenceLimitation::FutureRecallDecisionNotEvaluated));
    }

    #[test]
    fn visible_act3_boss_deadline_proves_deterministic_skip_but_not_dig() {
        let root = run_at_last_visible_fire(3);
        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let rest = batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();
        assert_eq!(
            rest.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::ViolatedAtVisibleAct3BossDeadline
        );
        assert_eq!(
            rest.evidence_for(CampfireProspectField::RunFeasibility)
                .unwrap()
                .status,
            CampfireEvidenceStatus::Exact
        );

        let dig = batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Dig)
            .unwrap();
        assert_eq!(
            dig.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::UnresolvedByChanceOutcome
        );
        assert!(dig
            .evidence_for(CampfireProspectField::RunFeasibility)
            .unwrap()
            .limitations
            .contains(&CampfireEvidenceLimitation::ChanceOutcomeCouldChangeRouteAccess));
    }

    #[test]
    fn earlier_act_boss_is_not_the_ruby_key_deadline() {
        let root = run_at_last_visible_fire(2);
        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let rest = batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();

        assert_eq!(
            rest.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::UnresolvedBeyondVisibleWindow
        );
    }

    #[test]
    fn unavailable_map_keeps_ruby_key_deadline_unresolved() {
        let mut root = candidate_run();
        root.act_num = 3;
        root.map = MapState::checkpoint_externalized_placeholder();
        let batch = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let rest = batch
            .candidates
            .iter()
            .find(|candidate| candidate.candidate == CampfireCandidate::Rest)
            .unwrap();

        assert_eq!(
            rest.run_feasibility.ruby_key,
            CampfireRubyKeyObligation::UnresolvedBeyondVisibleWindow
        );
        assert!(rest
            .evidence_for(CampfireProspectField::RunFeasibility)
            .unwrap()
            .limitations
            .contains(&CampfireEvidenceLimitation::VisibleDeadlineNotProven));
    }
}
