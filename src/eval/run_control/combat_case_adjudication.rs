use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{
    CombatSearchAcceptancePluginId, CombatSearchV2Config, CombatSearchV2TrajectoryReport,
};
use crate::eval::combat_case::CombatCase;
use crate::state::core::{ActiveCombat, CombatContext, RoomCombatContext};
use crate::state::map::node::RoomType;

use super::combat_candidate_line::CombatCandidateLine;
use super::combat_line_adjudication::{
    CombatLineAcceptancePolicy, CombatLineAdjudicationV1, CombatLineObservedOutcomeV1,
};
use super::combat_line_outcome::evaluate_combat_candidate_line_outcome;
use super::session::{canonical_player_class, RunControlConfig, RunControlSession};

pub const COMBAT_CASE_PROJECTION_TRUST_V1: &str = "combat_case_projected_run_context_v1";

const PROBE_POLICIES: [CombatSearchAcceptancePluginId; 2] = [
    CombatSearchAcceptancePluginId::AcceptedLineOnly,
    CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CombatCaseAdjudicationProbeV1 {
    NoCompleteLine,
    ProjectionFailed {
        source_review: String,
        error: String,
    },
    ReplayFailed {
        source_review: String,
        projection_trust: String,
        action_count: usize,
        adjudications: Vec<CombatLineAdjudicationV1>,
    },
    Adjudicated {
        source_review: String,
        projection_trust: String,
        action_count: usize,
        observed_outcome: CombatLineObservedOutcomeV1,
        adjudications: Vec<CombatLineAdjudicationV1>,
    },
}

pub fn adjudicate_combat_case_line_v1(
    source_review: impl Into<String>,
    case: &CombatCase,
    config: &CombatSearchV2Config,
    trajectory: &CombatSearchV2TrajectoryReport,
) -> CombatCaseAdjudicationProbeV1 {
    let source_review = source_review.into();
    let session = match project_combat_case_session(case) {
        Ok(session) => session,
        Err(error) => {
            return CombatCaseAdjudicationProbeV1::ProjectionFailed {
                source_review,
                error,
            };
        }
    };
    let line = match CombatCandidateLine::try_from_search_trajectory(trajectory) {
        Ok(line) => line,
        Err(error) => {
            return CombatCaseAdjudicationProbeV1::ReplayFailed {
                source_review,
                projection_trust: COMBAT_CASE_PROJECTION_TRUST_V1.to_string(),
                action_count: trajectory.actions.len(),
                adjudications: replay_failures(error),
            };
        }
    };
    match evaluate_combat_candidate_line_outcome(&session, &case.core.position, config, line) {
        Ok(evaluation) => {
            let observed_outcome = evaluation.outcome;
            CombatCaseAdjudicationProbeV1::Adjudicated {
                source_review,
                projection_trust: COMBAT_CASE_PROJECTION_TRUST_V1.to_string(),
                action_count: trajectory.actions.len(),
                adjudications: adjudicate_observed_outcome(observed_outcome.clone()),
                observed_outcome,
            }
        }
        Err(error) => CombatCaseAdjudicationProbeV1::ReplayFailed {
            source_review,
            projection_trust: COMBAT_CASE_PROJECTION_TRUST_V1.to_string(),
            action_count: trajectory.actions.len(),
            adjudications: replay_failures(error),
        },
    }
}

pub(super) fn project_combat_case_session(case: &CombatCase) -> Result<RunControlSession, String> {
    let player_class = canonical_player_class(&case.core.position.combat.meta.player_class)?;
    let mut session = RunControlSession::new(RunControlConfig {
        seed: case.core.source.seed,
        ascension_level: case.core.source.ascension,
        final_act: case.core.run.act >= 4,
        player_class,
        ..RunControlConfig::default()
    });
    session.run_state.act_num = case.core.run.act;
    session.run_state.floor_num = case.core.run.floor;
    session.run_state.current_hp = case.core.run.hp;
    session.run_state.max_hp = case.core.run.max_hp;
    session.run_state.gold = case.core.run.gold;
    session.run_state.master_deck = case.core.position.combat.meta.master_deck_snapshot.to_vec();
    session.run_state.relics = case.core.position.combat.entities.player.relics.clone();
    session.run_state.potions = case.core.position.combat.entities.potions.clone();
    session.run_state.rng_pool = case.core.position.combat.rng.pool.clone();
    session.engine_state = case.core.position.engine.clone();
    session.active_combat = Some(ActiveCombat::new(
        case.core.position.engine.clone(),
        case.core.position.combat.clone(),
        CombatContext::Room(RoomCombatContext {
            room_type: projected_room_type(case),
        }),
    ));
    Ok(session)
}

fn projected_room_type(case: &CombatCase) -> RoomType {
    if case.core.position.combat.meta.is_boss_fight {
        RoomType::MonsterRoomBoss
    } else if case.core.position.combat.meta.is_elite_fight {
        RoomType::MonsterRoomElite
    } else {
        RoomType::MonsterRoom
    }
}

pub(super) fn adjudicate_observed_outcome(
    outcome: CombatLineObservedOutcomeV1,
) -> Vec<CombatLineAdjudicationV1> {
    PROBE_POLICIES
        .into_iter()
        .map(|plugin| CombatLineAcceptancePolicy::from_plugin(plugin).adjudicate(outcome.clone()))
        .collect()
}

fn replay_failures(error: String) -> Vec<CombatLineAdjudicationV1> {
    PROBE_POLICIES
        .into_iter()
        .map(|policy| CombatLineAdjudicationV1::ReplayFailed {
            policy,
            error: error.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::ai::combat_search_v2::CombatSearchAcceptancePluginId;
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::eval::combat_case::{
        CombatCase, CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary, CombatCaseSource,
        CombatCaseWitnessBudgetV1,
    };
    use crate::eval::run_control::combat_line_adjudication::{
        CombatLineAdjudicationV1, CombatLineCleanlinessV1, CombatLineObservedOutcomeV1,
    };
    use crate::eval::run_control::transition_report::CardSnapshot;
    use crate::runtime::combat::CombatCard;
    use crate::sim::combat::{CombatPosition, CombatTerminal};
    use crate::state::core::EngineState;

    fn projected_case() -> CombatCase {
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.player_class = "Ironclad".to_string();
        combat.meta.master_deck_snapshot =
            vec![CombatCard::for_scenario(CardId::Strike, 41)].into();
        combat.entities.player.current_hp = 37;
        combat.entities.player.max_hp = 61;
        combat.entities.player.gold = 123;
        combat.entities.player.relics = vec![RelicState::new(RelicId::Mango)];
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 7)), None];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        CombatCase::new(
            CombatCaseSource {
                seed: 99,
                ascension: 3,
                generation: 4,
                branch_id: 5,
                parent_id: Some(3),
            },
            CombatCaseGap {
                boundary: "Combat".to_string(),
                reason: "test".to_string(),
                witness_budget: CombatCaseWitnessBudgetV1::AtomicExactV2 {
                    primary_nodes: 10,
                    primary_wall_ms: 20,
                    rescue_nodes: 30,
                    rescue_wall_ms: 40,
                },
            },
            CombatCaseRunSummary {
                act: 3,
                floor: 42,
                hp: 37,
                max_hp: 61,
                gold: 123,
                deck_size: 1,
                relic_count: 1,
                potion_slots: 2,
            },
            Vec::new(),
            None,
            Vec::new(),
            CombatCaseRngSummary::from_pool(&position.combat.rng.pool),
            position,
        )
    }

    #[test]
    fn projected_session_uses_combat_case_context_without_becoming_checkpoint() {
        let case = projected_case();
        let session = super::project_combat_case_session(&case).expect("project session");

        assert_eq!(session.run_state.seed, 99);
        assert_eq!(session.run_state.ascension_level, 3);
        assert_eq!(session.run_state.act_num, 3);
        assert_eq!(session.run_state.floor_num, 42);
        assert_eq!(session.run_state.current_hp, 37);
        assert_eq!(session.run_state.max_hp, 61);
        assert_eq!(session.run_state.gold, 123);
        assert_eq!(
            session.run_state.master_deck,
            case.core.position.combat.meta.master_deck_snapshot.as_ref()
        );
        assert_eq!(
            session.run_state.relics,
            case.core.position.combat.entities.player.relics
        );
        assert_eq!(
            session.run_state.potions,
            case.core.position.combat.entities.potions
        );
        assert_eq!(
            session
                .active_combat
                .as_ref()
                .map(|active| &active.combat_state),
            Some(&case.core.position.combat)
        );
    }

    #[test]
    fn dual_policy_results_share_one_observed_dirty_outcome() {
        let outcome = CombatLineObservedOutcomeV1 {
            terminal: CombatTerminal::Win,
            final_hp: 44,
            hp_loss: 0,
            potions_used: 0,
            action_count: 32,
            gold_delta: 0,
            ritual_dagger_growth: 0,
            gained_curses: vec![CardSnapshot {
                id: CardId::Parasite,
                lineage: crate::test_support::scenario_lineage(9001),
                upgrades: 0,
            }],
        };

        let results = super::adjudicate_observed_outcome(outcome.clone());

        assert_eq!(results.len(), 2);
        let CombatLineAdjudicationV1::Accepted {
            policy: CombatSearchAcceptancePluginId::AcceptedLineOnly,
            cleanliness: CombatLineCleanlinessV1::Dirty,
            observed_outcome: ordinary_outcome,
        } = &results[0]
        else {
            panic!("ordinary policy should accept the dirty outcome")
        };
        assert_eq!(ordinary_outcome, &outcome);
        let CombatLineAdjudicationV1::Rejected {
            policy: CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
            observed_outcome: clean_only_outcome,
            ..
        } = &results[1]
        else {
            panic!("clean-only policy should reject the dirty outcome")
        };
        assert_eq!(clean_only_outcome, &outcome);
    }
}
