use serde::{Deserialize, Serialize};

use crate::content::monsters::factory::EncounterId;
use crate::engine::campfire_candidates::CampfireCandidate;
use crate::eval::campfire_evaluation::CampfireEvaluationBatch;
use crate::eval::campfire_projection::CampfireProjection;
use crate::eval::combat_lab_v1::{derive_shuffle_seed_v1, CombatLabShuffleScheduleV1};
use crate::eval::fingerprint::{combat_state_fingerprint_v3, StateFingerprintV3};
use crate::runtime::combat::CombatCard;
use crate::runtime::rng::{RngPool, StsRng};
use crate::sim::combat::CombatPosition;
use crate::sim::combat_start::build_natural_combat_start;
use crate::state::map::node::RoomType;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireSurvivalInformationScope {
    ExactStateOracle,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireSurvivalLens {
    ActualConsequence,
    RootHpCapability,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CampfireSurvivalSubject {
    UnchangedRoot,
    Candidate { candidate: CampfireCandidate },
}

#[derive(Clone, Debug)]
pub struct CampfireSurvivalScenarioSpec {
    pub encounter_id: EncounterId,
    pub room_type: RoomType,
    /// Analysis-only seed for every non-shuffle RNG stream in this sampled
    /// encounter. It must not be copied from the live run RNG pool.
    pub analysis_seed: u64,
    pub schedule: CombatLabShuffleScheduleV1,
    pub sample_index: u64,
    pub lenses: Vec<CampfireSurvivalLens>,
    pub include_unchanged_root: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfireSurvivalScenarioGap {
    ChanceOutcomeNotMaterialized,
    PostRevealRecourseNotMaterialized,
    DeckIdentityChanged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CampfireSurvivalScenarioGapRecord {
    pub candidate: CampfireCandidate,
    pub gap: CampfireSurvivalScenarioGap,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireSurvivalScenarioCell {
    pub subject: CampfireSurvivalSubject,
    pub lens: CampfireSurvivalLens,
    pub analysis_seed: u64,
    pub shuffle_seed: u64,
    pub start: CombatPosition,
    pub state_fingerprint: StateFingerprintV3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireSurvivalScenarioSample {
    pub context_fingerprint: String,
    pub information_scope: CampfireSurvivalInformationScope,
    pub encounter_id: EncounterId,
    pub room_type: RoomType,
    pub sample_index: u64,
    pub analysis_seed: u64,
    pub shuffle_seed: u64,
    pub cells: Vec<CampfireSurvivalScenarioCell>,
    pub gaps: Vec<CampfireSurvivalScenarioGapRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampfireSurvivalScenarioError {
    NonCombatRoomType {
        room_type: RoomType,
    },
    NoLenses,
    RootDoesNotMatchEvaluation,
    CombatStart {
        subject: CampfireSurvivalSubject,
        lens: CampfireSurvivalLens,
        message: String,
    },
}

pub fn compile_aligned_campfire_survival_sample(
    root: &RunState,
    evaluation: &CampfireEvaluationBatch,
    spec: CampfireSurvivalScenarioSpec,
) -> Result<CampfireSurvivalScenarioSample, CampfireSurvivalScenarioError> {
    if !matches!(
        spec.room_type,
        RoomType::MonsterRoom | RoomType::MonsterRoomElite | RoomType::MonsterRoomBoss
    ) {
        return Err(CampfireSurvivalScenarioError::NonCombatRoomType {
            room_type: spec.room_type,
        });
    }
    if spec.lenses.is_empty() {
        return Err(CampfireSurvivalScenarioError::NoLenses);
    }
    let public_root = &evaluation.context.public_root;
    if root.current_hp != public_root.current_hp
        || root.max_hp != public_root.max_hp
        || !same_deck_identity(&root.master_deck, &public_root.master_deck)
    {
        return Err(CampfireSurvivalScenarioError::RootDoesNotMatchEvaluation);
    }

    let shuffle_seed = derive_shuffle_seed_v1(&spec.schedule, spec.sample_index);
    let mut subjects = Vec::new();
    let mut gaps = Vec::new();
    if spec.include_unchanged_root {
        subjects.push((CampfireSurvivalSubject::UnchangedRoot, root.clone()));
    }

    for candidate in &evaluation.candidates {
        let exact = match &candidate.projection {
            CampfireProjection::Exact(exact) => exact,
            CampfireProjection::Chance(_) => {
                gaps.push(CampfireSurvivalScenarioGapRecord {
                    candidate: candidate.candidate,
                    gap: CampfireSurvivalScenarioGap::ChanceOutcomeNotMaterialized,
                });
                continue;
            }
            CampfireProjection::ChanceThenDecision(_) => {
                gaps.push(CampfireSurvivalScenarioGapRecord {
                    candidate: candidate.candidate,
                    gap: CampfireSurvivalScenarioGap::PostRevealRecourseNotMaterialized,
                });
                continue;
            }
        };
        if !same_deck_identity(&root.master_deck, &exact.run_state.master_deck) {
            gaps.push(CampfireSurvivalScenarioGapRecord {
                candidate: candidate.candidate,
                gap: CampfireSurvivalScenarioGap::DeckIdentityChanged,
            });
            continue;
        }
        subjects.push((
            CampfireSurvivalSubject::Candidate {
                candidate: candidate.candidate,
            },
            exact.run_state.clone(),
        ));
    }

    let mut cells = Vec::with_capacity(subjects.len() * spec.lenses.len());
    for (subject, projected) in subjects {
        for lens in spec.lenses.iter().copied() {
            let mut sampled = projected.clone();
            if lens == CampfireSurvivalLens::RootHpCapability {
                sampled.current_hp = root.current_hp;
            }
            sampled.rng_pool = RngPool::new(spec.analysis_seed);
            sampled.rng_pool.shuffle_rng = StsRng::new(shuffle_seed);
            let (engine, combat) =
                build_natural_combat_start(&mut sampled, spec.encounter_id, spec.room_type)
                    .map_err(|message| CampfireSurvivalScenarioError::CombatStart {
                        subject,
                        lens,
                        message,
                    })?;
            let start = CombatPosition::new(engine, combat);
            cells.push(CampfireSurvivalScenarioCell {
                subject,
                lens,
                analysis_seed: spec.analysis_seed,
                shuffle_seed,
                state_fingerprint: combat_state_fingerprint_v3(&start),
                start,
            });
        }
    }

    Ok(CampfireSurvivalScenarioSample {
        context_fingerprint: evaluation.context.context_fingerprint.clone(),
        information_scope: CampfireSurvivalInformationScope::ExactStateOracle,
        encounter_id: spec.encounter_id,
        room_type: spec.room_type,
        sample_index: spec.sample_index,
        analysis_seed: spec.analysis_seed,
        shuffle_seed,
        cells,
        gaps,
    })
}

fn same_deck_identity(root: &[CombatCard], projected: &[CombatCard]) -> bool {
    root.iter()
        .map(|card| card.lineage)
        .eq(projected.iter().map(|card| card.lineage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::engine::campfire_candidates::CampfireCandidate;
    use crate::eval::campfire_evaluation::{
        build_campfire_evaluation_batch, CampfireContinuationProfile, CampfireEvaluationHorizon,
        CampfireEvaluationSpec, CampfireRunGoal,
    };
    use crate::eval::combat_lab_v1::{CombatLabShuffleGeneratorV1, CombatLabShuffleScheduleV1};
    use crate::runtime::combat::CombatCard;
    use crate::state::map::node::RoomType;
    use crate::state::run::RunState;

    fn evaluation_spec() -> CampfireEvaluationSpec {
        CampfireEvaluationSpec {
            run_goal: CampfireRunGoal::Act3Victory,
            horizon: CampfireEvaluationHorizon::UntilNextCampfireOrActTerminal {
                route_horizon_nodes: 5,
            },
            route_path_budget: 2_000,
            continuation_profile: CampfireContinuationProfile {
                profile_id: "survival-scenario-test".to_string(),
                source_identity: "test-source".to_string(),
            },
            public_scenario_distribution_id: "explicit-encounter-test".to_string(),
            mechanics_version: "sts-simulator-test-v1".to_string(),
        }
    }

    fn candidate_run() -> RunState {
        let mut run = RunState::new(17, 0, false, "Ironclad");
        run.current_hp = 20;
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 101),
            CombatCard::for_scenario(CardId::Defend, 102),
        ];
        run.relics = vec![
            RelicState::new(RelicId::Girya),
            RelicState::new(RelicId::Shovel),
            RelicState::new(RelicId::PeacePipe),
        ];
        run
    }

    fn scenario_spec() -> CampfireSurvivalScenarioSpec {
        CampfireSurvivalScenarioSpec {
            encounter_id: EncounterId::JawWorm,
            room_type: RoomType::MonsterRoom,
            analysis_seed: 77,
            schedule: CombatLabShuffleScheduleV1 {
                generator: CombatLabShuffleGeneratorV1::SplitMix64V1,
                seed: 91,
            },
            sample_index: 0,
            lenses: vec![
                CampfireSurvivalLens::ActualConsequence,
                CampfireSurvivalLens::RootHpCapability,
            ],
            include_unchanged_root: true,
        }
    }

    fn cell(
        sample: &CampfireSurvivalScenarioSample,
        subject: CampfireSurvivalSubject,
        lens: CampfireSurvivalLens,
    ) -> &CampfireSurvivalScenarioCell {
        sample
            .cells
            .iter()
            .find(|cell| cell.subject == subject && cell.lens == lens)
            .expect("requested cell should be compiled")
    }

    #[test]
    fn actual_and_root_hp_lenses_separate_healing_from_smith_capability() {
        let root = candidate_run();
        let evaluation = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let sample =
            compile_aligned_campfire_survival_sample(&root, &evaluation, scenario_spec()).unwrap();
        let rest_actual = cell(
            &sample,
            CampfireSurvivalSubject::Candidate {
                candidate: CampfireCandidate::Rest,
            },
            CampfireSurvivalLens::ActualConsequence,
        );
        let rest_capability = cell(
            &sample,
            CampfireSurvivalSubject::Candidate {
                candidate: CampfireCandidate::Rest,
            },
            CampfireSurvivalLens::RootHpCapability,
        );
        let smith_actual = cell(
            &sample,
            CampfireSurvivalSubject::Candidate {
                candidate: CampfireCandidate::Smith {
                    card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(101),
                },
            },
            CampfireSurvivalLens::ActualConsequence,
        );
        let unchanged_actual = cell(
            &sample,
            CampfireSurvivalSubject::UnchangedRoot,
            CampfireSurvivalLens::ActualConsequence,
        );

        assert_eq!(
            sample.information_scope,
            CampfireSurvivalInformationScope::ExactStateOracle
        );
        assert_eq!(rest_actual.start.combat.entities.player.current_hp, 44);
        assert_eq!(rest_capability.start.combat.entities.player.current_hp, 20);
        assert_eq!(smith_actual.start.combat.entities.player.current_hp, 20);
        assert_eq!(unchanged_actual.start.combat.entities.player.current_hp, 20);
        assert_eq!(
            rest_actual.start.combat.entities.monsters,
            smith_actual.start.combat.entities.monsters
        );
        assert_eq!(
            rest_actual
                .start
                .combat
                .zones
                .hand
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            smith_actual
                .start
                .combat
                .zones
                .hand
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>()
        );
        assert!(sample.gaps.iter().any(|gap| {
            gap.gap == CampfireSurvivalScenarioGap::ChanceOutcomeNotMaterialized
                && gap.candidate == CampfireCandidate::Dig
        }));
        assert!(sample.gaps.iter().any(|gap| {
            gap.gap == CampfireSurvivalScenarioGap::DeckIdentityChanged
                && matches!(gap.candidate, CampfireCandidate::Toke { .. })
        }));
    }

    #[test]
    fn analysis_rng_replaces_different_live_rng_states() {
        let first_root = candidate_run();
        let mut second_root = first_root.clone();
        let _ = second_root.rng_pool.monster_hp_rng.random(999);
        let _ = second_root.rng_pool.ai_rng.random(999);
        let first_evaluation =
            build_campfire_evaluation_batch(&first_root, evaluation_spec()).unwrap();
        let second_evaluation =
            build_campfire_evaluation_batch(&second_root, evaluation_spec()).unwrap();

        let first = compile_aligned_campfire_survival_sample(
            &first_root,
            &first_evaluation,
            scenario_spec(),
        )
        .unwrap();
        let second = compile_aligned_campfire_survival_sample(
            &second_root,
            &second_evaluation,
            scenario_spec(),
        )
        .unwrap();

        assert_eq!(first.cells, second.cells);
    }

    #[test]
    fn dream_catcher_rest_stays_a_post_reveal_recourse_gap() {
        let mut root = candidate_run();
        root.relics.push(RelicState::new(RelicId::DreamCatcher));
        let evaluation = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let sample =
            compile_aligned_campfire_survival_sample(&root, &evaluation, scenario_spec()).unwrap();

        assert!(!sample.cells.iter().any(|cell| {
            cell.subject
                == CampfireSurvivalSubject::Candidate {
                    candidate: CampfireCandidate::Rest,
                }
        }));
        assert!(sample.gaps.iter().any(|gap| {
            gap.candidate == CampfireCandidate::Rest
                && gap.gap == CampfireSurvivalScenarioGap::PostRevealRecourseNotMaterialized
        }));
    }

    #[test]
    fn empty_lens_contract_is_rejected() {
        let root = candidate_run();
        let evaluation = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
        let mut spec = scenario_spec();
        spec.lenses.clear();

        assert_eq!(
            compile_aligned_campfire_survival_sample(&root, &evaluation, spec),
            Err(CampfireSurvivalScenarioError::NoLenses)
        );
    }
}
