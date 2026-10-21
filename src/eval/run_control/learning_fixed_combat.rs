//! Opt-in strategic transitions under a declared, bounded oracle combat policy.
//! Search witnesses are executor evidence, never model action labels. Missing
//! witnesses either roll back or enter an explicitly configured public rollout.

use super::*;
use crate::agent::task::{
    task_research_plan_non_dominated_candidate_ordinals_v1, TaskResearchPlanAttemptV1,
    TaskResearchPlanCandidateV1, TaskResearchPlanOutcomeV1,
};
use crate::eval::run_control::{
    run_control_session_combat_research_portfolio_with_options_v1,
    run_control_session_whole_combat_portfolio_v1, CombatResearchLaneScheduleV1,
    CombatResearchPlanOptionsV1, CombatResearchPortfolioV1, RunControlSession,
    WholeCombatResearchV1,
};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::sync::Arc;
use std::time::Instant;

pub const FIXED_COMBAT_EXECUTOR_REVISION_V1: u32 = 2;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixedCombatSearchBackendV1 {
    #[default]
    Portfolio,
    WholeCombat,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixedCombatOptionsV1 {
    #[serde(default)]
    pub backend: FixedCombatSearchBackendV1,
    pub search: CombatResearchPlanOptionsV1,
    /// None stops on a search miss. Some(N) actually plays at most N actions
    /// with the public legality baseline; reaching the limit remains Unknown.
    pub fallback_action_limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run_control::{
        public_deterministic_baseline_decision_v1,
        public_deterministic_baseline_selection_decision_v1,
    };

    fn paths(pool: &LearningEnvPoolV3) -> Vec<LearningDecisionPathV3> {
        let batch = pool.active_model_batch().unwrap();
        batch
            .decision_tokens
            .iter()
            .zip(&batch.model_batch.decisions)
            .map(|(source, decision)| {
                let ordinal = public_deterministic_baseline_decision_v1(decision).candidate_ordinal;
                let mut ordinals = vec![ordinal];
                if let LearningModelChoiceV1::DecodeSelection(mut draft) =
                    decision.choose(ordinal).unwrap()
                {
                    loop {
                        let ordinal = public_deterministic_baseline_selection_decision_v1(
                            decision.observation,
                            &draft,
                        )
                        .candidate_ordinal;
                        ordinals.push(ordinal);
                        if matches!(
                            draft.choose(ordinal).unwrap(),
                            LearningSelectionStepV1::Apply(_)
                        ) {
                            break;
                        }
                    }
                }
                LearningDecisionPathV3 {
                    source: source.clone(),
                    ordinals,
                }
            })
            .collect()
    }

    fn before_first_combat() -> LearningEnvPoolV3 {
        before_first_combat_with_config(RunControlConfig {
            seed: 17,
            ..RunControlConfig::default()
        })
    }

    fn before_first_combat_with_config(config: RunControlConfig) -> LearningEnvPoolV3 {
        let mut pool = LearningEnvPoolV3::from_configs([config]).unwrap();
        for _ in 0..30 {
            let mut next = pool.clone();
            next.step_active_paths_v3(paths(&pool)).unwrap();
            if matches!(next.slots[0].boundary, LearningBoundaryV1::Combat { .. }) {
                return pool;
            }
            pool = next;
        }
        panic!("natural prefix did not enter combat");
    }

    #[test]
    fn incremental_research_reuses_budget_preserves_old_choices_and_matches_one_shot() {
        let mut pool = before_first_combat_with_config(RunControlConfig {
            seed: 2026100201,
            ascension_level: 20,
            final_act: true,
            ..RunControlConfig::default()
        });
        pool.step_active_paths_v3(paths(&pool)).unwrap();
        let mut pool = pool.with_candidate_surface(LearningCandidateSurfaceV1::Legal);
        let before = checkpoints(&pool);
        let mut options = test_options();
        options.backend = FixedCombatSearchBackendV1::WholeCombat;
        options.search.max_generation_work = 10_000;
        let mut research = pool
            .start_fixed_combat_research_v1(0, options.clone())
            .unwrap();
        let empty = research.advance_to(1).unwrap();
        assert!(empty.choices.is_empty());
        let early = research.advance_to(1_000).unwrap();
        assert!(!early.choices.is_empty());
        let early_attempt = early.attempt.clone();
        let old_choice = early.choices[0].clone();
        let old_exit = checkpoints(&early.environments[0]);
        assert!(research.advance_to(999).is_err());
        assert!(research.advance_to(10_001).is_err());
        let late = research.advance_to(10_000).unwrap();
        let one_shot = pool.fixed_combat_branches_v1(0, options).unwrap();
        assert_eq!(late.attempt, one_shot.attempt);
        assert_eq!(late.attempt.lanes[0].work_consumed, 10_000);
        assert_eq!(early.attempt, early_attempt);
        for (left, right) in late.environments.iter().zip(&one_shot.environments) {
            assert_eq!(checkpoints(left), checkpoints(right));
            assert_eq!(left.candidate_surface(), LearningCandidateSurfaceV1::Legal);
            assert_eq!(right.candidate_surface(), LearningCandidateSurfaceV1::Legal);
        }
        let same = research.advance_to(10_000).unwrap();
        assert_eq!(same.attempt, late.attempt);
        assert_eq!(checkpoints(&pool), before);
        pool.adopt_fixed_combat_branch_v1(&old_choice).unwrap();
        assert_eq!(checkpoints(&pool), old_exit);
        assert!(pool.adopt_fixed_combat_branch_v1(&late.choices[0]).is_err());
    }

    #[test]
    fn incremental_research_rejects_other_backends_and_fallback() {
        let mut pool = before_first_combat();
        pool.step_active_paths_v3(paths(&pool)).unwrap();
        let before = checkpoints(&pool);
        let mut options = test_options();
        assert!(pool
            .start_fixed_combat_research_v1(0, options.clone())
            .is_err());
        options.backend = FixedCombatSearchBackendV1::WholeCombat;
        options.fallback_action_limit = Some(10);
        assert!(pool.start_fixed_combat_research_v1(0, options).is_err());
        assert_eq!(checkpoints(&pool), before);
    }

    #[test]
    fn combat_branches_keep_resource_alternatives_and_leave_source_unchanged() {
        let mut pool = before_first_combat_with_config(RunControlConfig {
            seed: 2026100201,
            ascension_level: 20,
            final_act: true,
            ..RunControlConfig::default()
        });
        pool.step_active_paths_v3(paths(&pool)).unwrap();
        let before = checkpoints(&pool);
        let token = pool.decision_token(0).unwrap();
        let mut options = test_options();
        options.backend = FixedCombatSearchBackendV1::WholeCombat;
        options.search.max_generation_work = 30_000;
        let branches = pool.fixed_combat_branches_v1(0, options.clone()).unwrap();
        assert_eq!(branches.source, token);
        assert_eq!(
            branches.candidate_ordinals,
            task_research_plan_non_dominated_candidate_ordinals_v1(&branches.attempt.candidates)
        );
        assert!(branches.candidate_ordinals.len() >= 2);
        assert_eq!(
            branches.environments.len(),
            branches.candidate_ordinals.len()
        );
        let mut potion_spend = BTreeSet::new();
        for (branch_pool, &ordinal) in branches
            .environments
            .iter()
            .zip(&branches.candidate_ordinals)
        {
            assert_eq!(branch_pool.slot_count(), 1);
            let outcome = &branches.attempt.candidates[ordinal].outcome;
            let context = branch_pool.public_run_context(0).unwrap();
            assert_eq!(
                (context.hp, context.max_hp, context.gold),
                (
                    outcome.final_hp,
                    outcome.persistent.max_hp,
                    outcome.final_gold
                )
            );
            assert_ne!(
                context.boundary_kind,
                crate::eval::run_control::LearningBoundaryKindV1::Combat
            );
            potion_spend.insert(outcome.total_potions_consumed);
            let LearningBoundaryV1::Strategic { boundary } = branch_pool.boundary(0).unwrap()
            else {
                panic!("witness must stop at real reward boundary");
            };
            assert_eq!(
                boundary.observation.run_goal,
                crate::ai::planner_core::PlannerRunGoal::HeartVictory
            );
        }
        assert!(potion_spend.len() >= 2);
        assert_eq!(checkpoints(&pool), before);
        assert_eq!(pool.decision_token(0).unwrap(), token);
        assert_eq!(branches.options, options);
    }

    #[test]
    fn combat_branches_reject_invalid_sources_and_keep_misses_as_empty_frontiers() {
        let mut pool = before_first_combat();
        let before = checkpoints(&pool);
        assert!(matches!(
            pool.fixed_combat_branches_v1(0, test_options()),
            Err(FixedCombatStepErrorV1::CombatSourceRequired { slot_index: 0 })
        ));
        assert_eq!(checkpoints(&pool), before);
        pool.step_active_paths_v3(paths(&pool)).unwrap();
        let before = checkpoints(&pool);
        let mut options = test_options();
        options.backend = FixedCombatSearchBackendV1::WholeCombat;
        options.search.max_generation_work = 1;
        let branches = pool.fixed_combat_branches_v1(0, options.clone()).unwrap();
        assert!(branches.attempt.candidates.is_empty());
        assert!(branches.candidate_ordinals.is_empty());
        assert!(branches.environments.is_empty());
        assert!(branches.choices.is_empty());
        assert_eq!(checkpoints(&pool), before);
        options.fallback_action_limit = Some(10);
        assert!(matches!(
            pool.fixed_combat_branches_v1(0, options),
            Err(FixedCombatStepErrorV1::InvalidOptions(_))
        ));
        assert_eq!(checkpoints(&pool), before);
    }

    #[test]
    fn combat_branches_preserve_potion_restrictions_in_continuations() {
        let mut root = before_first_combat_with_config(RunControlConfig {
            seed: 2026100201,
            ascension_level: 20,
            final_act: true,
            ..RunControlConfig::default()
        });
        root.step_active_paths_v3(paths(&root)).unwrap();
        let pool = LearningEnvPoolV3::from_envs_with_potion_policy(
            [root.slots[0].env.clone()],
            CombatLearningPotionPolicyV1::never(),
        )
        .unwrap();
        let before = checkpoints(&pool);
        let mut options = test_options();
        options.backend = FixedCombatSearchBackendV1::WholeCombat;
        options.search.max_generation_work = 30_000;
        assert!(matches!(
            pool.fixed_combat_branches_v1(0, options.clone()),
            Err(FixedCombatStepErrorV1::InvalidOptions(_))
        ));
        let unrestricted = options.clone();
        options.search.potion_policy =
            crate::ai::combat_witness_contract::CombatWitnessPotionPolicyV1::Never;
        let branches = pool.fixed_combat_branches_v1(0, options.clone()).unwrap();
        assert!(!branches.environments.is_empty());
        for (mut branch, ordinal) in branches
            .environments
            .into_iter()
            .zip(branches.candidate_ordinals)
        {
            assert_eq!(
                branches.attempt.candidates[ordinal]
                    .outcome
                    .total_potions_consumed,
                0
            );
            let branch_before = checkpoints(&branch);
            assert!(matches!(
                branch.step_with_fixed_combat_v1(paths(&branch), unrestricted.clone()),
                Err(FixedCombatStepErrorV1::InvalidOptions(_))
            ));
            assert_eq!(checkpoints(&branch), branch_before);
            assert!(matches!(
                branch.step_with_fixed_combat_v1(paths(&branch), options.clone()),
                Ok(FixedCombatStepV1::Advanced { .. })
            ));
            assert!(branch.public_run_context(0).unwrap().gold > branches.entry.gold);
        }
        assert_eq!(checkpoints(&pool), before);
    }

    #[test]
    fn branch_adoption_replays_only_source_combat_and_keeps_other_slots() {
        let mut root = before_first_combat_with_config(RunControlConfig {
            seed: 2026100201,
            ascension_level: 20,
            final_act: true,
            ..RunControlConfig::default()
        });
        root.step_active_paths_v3(paths(&root)).unwrap();
        let other = before_first_combat();
        let mut pool =
            LearningEnvPoolV3::from_envs([root.slots[0].env.clone(), other.slots[0].env.clone()])
                .unwrap();
        let other_before = pool.checkpoint_slot(1).unwrap();
        let mut options = test_options();
        options.backend = FixedCombatSearchBackendV1::WholeCombat;
        options.search.max_generation_work = 30_000;
        let mut branches = pool.fixed_combat_branches_v1(0, options).unwrap();
        let index = branches.choices.len() - 1;
        let choice = branches.choices[index].clone();
        let expected = branches.environments[index].checkpoint_slot(0).unwrap();
        let research = &mut branches.environments[index];
        research.step_active_paths_v3(paths(research)).unwrap();
        assert_ne!(research.checkpoint_slot(0).unwrap(), expected);
        let receipt = pool.adopt_fixed_combat_branch_v1(&choice).unwrap();
        assert_eq!(
            receipt.selected_candidate_ordinal,
            Some(choice.candidate_ordinal())
        );
        assert_eq!(pool.checkpoint_slot(0).unwrap(), expected);
        assert_eq!(pool.checkpoint_slot(1).unwrap(), other_before);
        let before = checkpoints(&pool);
        assert!(matches!(
            pool.adopt_fixed_combat_branch_v1(&choice),
            Err(FixedCombatStepErrorV1::Pool(
                LearningEnvPoolError::StaleDecisionToken { .. }
            ))
        ));
        assert_eq!(checkpoints(&pool), before);
    }

    #[test]
    fn branch_adoption_rolls_back_partial_replay_errors_and_rejects_stale_sources() {
        let mut pool = before_first_combat();
        pool.step_active_paths_v3(paths(&pool)).unwrap();
        let branches = pool.fixed_combat_branches_v1(0, test_options()).unwrap();
        let mut broken = branches.choices[0].clone();
        let lines =
            &mut Arc::make_mut(&mut broken.witnesses).witness_input_lines[broken.candidate_ordinal];
        assert!(lines.len() > 1);
        *lines.last_mut().unwrap() = crate::state::core::ClientInput::ClaimReward(usize::MAX);
        let before = checkpoints(&pool);
        assert!(matches!(
            pool.adopt_fixed_combat_branch_v1(&broken),
            Err(FixedCombatStepErrorV1::Combat { .. })
        ));
        assert_eq!(checkpoints(&pool), before);
        pool.step_active_paths_v3(paths(&pool)).unwrap();
        let before = checkpoints(&pool);
        assert!(matches!(
            pool.adopt_fixed_combat_branch_v1(&branches.choices[0]),
            Err(FixedCombatStepErrorV1::Pool(
                LearningEnvPoolError::StaleDecisionToken { .. }
            ))
        ));
        assert_eq!(checkpoints(&pool), before);
    }

    #[test]
    fn whole_combat_executor_replays_a_natural_fight_and_rolls_back_a_miss() {
        let mut pool = before_first_combat();
        let before = checkpoints(&pool);
        let selected = paths(&pool);
        let mut options = test_options();
        options.backend = FixedCombatSearchBackendV1::WholeCombat;
        options.search.max_generation_work = 1;
        let FixedCombatStepV1::Unresolved { evidence, .. } = pool
            .step_with_fixed_combat_v1(selected.clone(), options.clone())
            .unwrap()
        else {
            panic!("one transition cannot resolve this fight")
        };
        assert_eq!(checkpoints(&pool), before);
        assert!(evidence.combats[0].attempt.candidates.is_empty());
        assert!(evidence.combats[0].fallback.is_none());
        options.search.max_generation_work = 30_000;
        let FixedCombatStepV1::Advanced { evidence, .. } =
            pool.step_with_fixed_combat_v1(selected, options).unwrap()
        else {
            panic!("expected a completed natural combat")
        };
        let receipt = &evidence.combats[0];
        assert!(receipt.selected_candidate_ordinal.is_some());
        assert!(receipt.exit.as_ref().unwrap().hp > 0);
        assert!(receipt.fallback.is_none());
        assert_eq!(
            receipt.attempt.lanes[0].provenance.producer_name,
            "whole_combat_solver"
        );
        assert_eq!(
            receipt.attempt.lanes[0].work_unit,
            crate::agent::task::TaskResearchPlanWorkUnitV1::WholeCombatTransitions
        );
    }

    fn checkpoints(pool: &LearningEnvPoolV3) -> Vec<LearningEnvCheckpointV3> {
        pool.slots
            .iter()
            .map(|slot| slot.env.checkpoint())
            .collect()
    }

    fn looter_candidates() -> [TaskResearchPlanCandidateV1; 2] {
        use crate::agent::task::{
            TaskPublicChoiceV1, TaskResearchPlanPendingRewardV1,
            TaskResearchPlanPersistentOutcomeV1, TaskResearchPlanStepV1, TaskResearchPlanV1,
        };
        let escaped = TaskResearchPlanCandidateV1 {
            plan: TaskResearchPlanV1 {
                root_public_snapshot_id: "root".into(),
                steps: vec![TaskResearchPlanStepV1 {
                    source_public_snapshot_id: "root".into(),
                    choice: TaskPublicChoiceV1::one(0),
                }],
            },
            outcome: TaskResearchPlanOutcomeV1 {
                final_hp: 75,
                final_gold: 113,
                persistent: TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 75,
                    recoverable_gold_delta: -40,
                    unrecovered_stolen_gold: 40,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                consumed_entry_potion_slots: Vec::new(),
                total_potions_consumed: 0,
                generated_potions: Vec::new(),
                final_relics: Vec::new(),
                meta_changes: Vec::new(),
                pending_rewards: vec![TaskResearchPlanPendingRewardV1::Card { cards: Vec::new() }],
                combat_mugged: true,
            },
            satisfies_requested_quality: false,
            supporting_provenances: Vec::new(),
        };
        let mut rewarded = escaped.clone();
        rewarded.plan.steps.push(rewarded.plan.steps[0].clone());
        rewarded.outcome.combat_mugged = false;
        rewarded.outcome.persistent.recoverable_gold_delta = 0;
        rewarded.outcome.persistent.unrecovered_stolen_gold = 0;
        rewarded.outcome.pending_rewards.extend([
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 40 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 10 },
        ]);
        [escaped, rewarded]
    }

    #[test]
    fn fixed_selection_recovers_gold_before_shorter_mugged_exit() {
        let candidates = looter_candidates();
        let before = candidates.clone();
        // Both remain available to callers requesting exact outcome branches.
        assert_eq!(
            task_research_plan_non_dominated_candidate_ordinals_v1(&candidates),
            vec![0, 1]
        );
        assert_eq!(select_fixed_combat_candidate_v1(&candidates), Some(1));
        assert_eq!(candidates, before);
        assert_eq!(
            select_fixed_combat_candidate_v1(&[candidates[1].clone(), candidates[0].clone()]),
            Some(0)
        );
    }

    #[test]
    fn fixed_selection_preserves_hp_tradeoff_and_quality_is_only_evidence() {
        let [escaped, mut rewarded] = looter_candidates();
        rewarded.outcome.final_hp -= 1;
        rewarded.satisfies_requested_quality = true;
        assert_eq!(
            select_fixed_combat_candidate_v1(&[escaped, rewarded]),
            Some(0)
        );
        let mut alone = looter_candidates()[0].clone();
        alone.satisfies_requested_quality = false;
        assert_eq!(select_fixed_combat_candidate_v1(&[alone]), Some(0));
        assert_eq!(select_fixed_combat_candidate_v1(&[]), None);
    }

    fn test_options() -> FixedCombatOptionsV1 {
        let mut options = FixedCombatOptionsV1::default();
        options.search.atomic_challenger = Some(
            crate::eval::run_control::AtomicCombatResearchPlanChallengerOptionsV1 {
                max_nodes: 4096,
                wall_ms: 10_000,
            },
        );
        options.search.wall_ms = 10_000;
        options
    }

    #[test]
    fn no_combat_preserves_ordinary_transition_and_lineage() {
        let mut ordinary = LearningEnvPoolV3::from_configs([RunControlConfig::default()]).unwrap();
        let mut macro_pool = ordinary.clone();
        let paths = paths(&ordinary);
        let expected = ordinary.step_active_paths_v3(paths.clone()).unwrap();
        let FixedCombatStepV1::Advanced {
            transitions,
            evidence,
        } = macro_pool
            .step_with_fixed_combat_v1(paths, test_options())
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(transitions, expected);
        assert_eq!(checkpoints(&ordinary), checkpoints(&macro_pool));
        assert!(evidence.combats.is_empty());
    }

    #[test]
    fn invalid_options_and_stale_source_do_not_mutate() {
        let mut pool = before_first_combat();
        let before = checkpoints(&pool);
        let mut options = test_options();
        options.search.wall_ms = 0;
        assert!(matches!(
            pool.step_with_fixed_combat_v1(paths(&pool), options),
            Err(FixedCombatStepErrorV1::InvalidOptions(_))
        ));
        let mut bad = paths(&pool);
        bad[0].source.token_id.push_str("-stale");
        assert!(matches!(
            pool.step_with_fixed_combat_v1(bad, test_options()),
            Err(FixedCombatStepErrorV1::Pool(
                LearningEnvPoolError::StaleDecisionToken { .. }
            ))
        ));
        assert_eq!(checkpoints(&pool), before);
        assert!(!pool.is_poisoned());
    }

    #[test]
    fn natural_combat_returns_real_reward_boundary_and_exact_replay() {
        let mut pool = before_first_combat();
        let mut replay = pool.clone();
        let selected = paths(&pool);
        replay.step_active_paths_v3(selected.clone()).unwrap();
        let portfolio = run_control_session_combat_research_portfolio_with_options_v1(
            &replay.slots[0].env.clone().into_session(),
            Some(test_options().search),
        )
        .unwrap();
        let FixedCombatStepV1::Advanced {
            transitions,
            evidence,
        } = pool
            .step_with_combat_search(selected.clone(), test_options(), |_, _| {
                Ok(portfolio.clone())
            })
            .unwrap()
        else {
            panic!("first combat must have witness")
        };
        let receipt = &evidence.combats[0];
        for input in &portfolio.witness_input_lines[receipt.selected_candidate_ordinal.unwrap()] {
            let action = replay.slots[0]
                .env
                .prepare_action(LearningActionV1::CombatInput {
                    input: input.clone(),
                })
                .unwrap();
            replay.slots[0].boundary = replay.slots[0].env.step_prepared(action).unwrap().boundary;
        }
        assert_eq!(checkpoints(&pool), checkpoints(&replay));
        assert_eq!(
            pool.public_run_context(0).unwrap().strategic_context_kind,
            Some(crate::eval::run_control::LearningStrategicContextKindV1::Reward)
        );
        assert_eq!(transitions[0].source, selected[0].source);
        assert_eq!(transitions[0].action_ordinals, selected[0].ordinals);
        assert_eq!(transitions[0].next, Some(pool.decision_token(0).unwrap()));
        assert!(
            transitions[0].next.as_ref().unwrap().decision_step
                > transitions[0].source.decision_step + 1
        );
        assert!(transitions[0].terminal_outcome.is_none());
        assert_eq!(transitions[0].reward, 0);
    }

    #[test]
    fn later_unknown_or_error_rolls_back_already_settled_slots() {
        let root = before_first_combat();
        let mut entering = root.clone();
        entering.step_active_paths_v3(paths(&root)).unwrap();
        let portfolio = run_control_session_combat_research_portfolio_with_options_v1(
            &entering.slots[0].env.clone().into_session(),
            Some(test_options().search),
        )
        .unwrap();
        assert!(!portfolio.attempt.candidates.is_empty());
        let mut missing = portfolio.clone();
        missing.attempt.candidates.clear();
        missing.witness_input_lines.clear();
        for lane in &mut missing.attempt.lanes {
            lane.exit = crate::agent::task::TaskResearchPlanAttemptExitV1::WallTimeExhausted;
        }
        for failure in [false, true] {
            let mut pool = LearningEnvPoolV3::from_envs([
                root.slots[0].env.clone(),
                root.slots[0].env.clone(),
            ])
            .unwrap();
            let before = checkpoints(&pool);
            let tokens = paths(&pool);
            let mut calls = 0;
            let result = pool.step_with_combat_search(tokens.clone(), test_options(), |_, _| {
                calls += 1;
                if calls == 1 {
                    Ok(portfolio.clone())
                } else if failure {
                    Err("injected replay failure".into())
                } else {
                    Ok(missing.clone())
                }
            });
            if failure {
                assert!(matches!(
                    result,
                    Err(FixedCombatStepErrorV1::Combat { slot_index: 1, .. })
                ));
            } else {
                let FixedCombatStepV1::Unresolved {
                    slot_index,
                    evidence,
                } = result.unwrap()
                else {
                    panic!()
                };
                assert_eq!(slot_index, 1);
                assert_eq!(evidence.combats.len(), 2);
                assert!(evidence.combats[0].exit.is_some());
                assert!(evidence.combats[1].selected_candidate_ordinal.is_none());
            }
            assert_eq!(calls, 2);
            assert_eq!(checkpoints(&pool), before);
            assert_eq!(paths(&pool), tokens);
            assert!(!pool.is_poisoned());
        }
    }
}

impl Default for FixedCombatOptionsV1 {
    fn default() -> Self {
        Self {
            backend: FixedCombatSearchBackendV1::Portfolio,
            fallback_action_limit: None,
            search: CombatResearchPlanOptionsV1 {
                lane_schedule: CombatResearchLaneScheduleV1::FirstWitness,
                satisfaction: crate::ai::combat_witness_contract::CombatWitnessSatisfactionV1::FirstCompleteWin,
                ..CombatResearchPlanOptionsV1::default()
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixedCombatReceiptV1 {
    pub slot_index: usize,
    pub entry: LearningPublicRunContextV1,
    pub entry_decision_step: u64,
    pub attempt: TaskResearchPlanAttemptV1,
    /// Ordinal in attempt.candidates. The fixed macro first excludes mugged
    /// exits strictly dominated in resources by a rewarded exit, then selects
    /// highest HP, fewest potions, shortest line and producer order on the
    /// sampled frontier. Explicit adoption records the caller's choice instead.
    pub selected_candidate_ordinal: Option<usize>,
    pub fallback: Option<FixedCombatFallbackReceiptV1>,
    pub exit: Option<LearningPublicRunContextV1>,
    pub exit_decision_step: Option<u64>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixedCombatFallbackStepV1 {
    pub source_public_snapshot_id: String,
    pub action_ordinals: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixedCombatFallbackReceiptV1 {
    pub baseline_revision: u32,
    pub steps: Vec<FixedCombatFallbackStepV1>,
    pub action_limit_reached: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixedCombatBatchEvidenceV1 {
    pub executor_revision: u32,
    pub information_access: String,
    pub options: FixedCombatOptionsV1,
    /// On rollback these are attempted, private executions, not committed steps.
    pub combats: Vec<FixedCombatReceiptV1>,
}

/// Exact counterfactual exits of the sampled nondominated combat frontier.
/// Independent single-slot pools align with candidate_ordinals.
/// No branch is adopted and an empty frontier is not a combat defeat.
#[derive(Clone, Debug)]
pub struct FixedCombatBranchBatchV1 {
    pub source: LearningDecisionTokenV3,
    pub entry: LearningPublicRunContextV1,
    pub options: FixedCombatOptionsV1,
    pub attempt: TaskResearchPlanAttemptV1,
    pub candidate_ordinals: Vec<usize>,
    pub environments: Vec<LearningEnvPoolV3>,
    /// Immutable source-bound choices, aligned with the environments. Advancing
    /// a research environment does not change the witness adopted by a choice.
    pub choices: Vec<FixedCombatBranchChoiceV1>,
    pub elapsed_ms: u64,
}

/// A sampled frontier witness, created only by the combat branch owner.
/// It retains shared immutable evidence, not a mutable continuation checkpoint.
#[derive(Clone, Debug)]
pub struct FixedCombatBranchChoiceV1 {
    witnesses: Arc<FixedCombatBranchWitnessSetV1>,
    candidate_ordinal: usize,
}

#[derive(Clone, Debug)]
struct FixedCombatBranchWitnessSetV1 {
    slot_index: usize,
    source: LearningDecisionTokenV3,
    entry: LearningPublicRunContextV1,
    options: FixedCombatOptionsV1,
    attempt: TaskResearchPlanAttemptV1,
    witness_input_lines: Vec<Vec<crate::state::core::ClientInput>>,
}

/// Process-local incremental research, owning its original source and search.
/// It never changes the source pool. Earlier choices remain immutable witnesses
/// even when later work replaces them in the sampled resource frontier.
pub struct FixedCombatResearchV1 {
    slot_index: usize,
    source: LearningDecisionTokenV3,
    entry: LearningPublicRunContextV1,
    slot: LearningEnvPoolSlotV3,
    potion_policy: CombatLearningPotionPolicyV1,
    candidate_surface: LearningCandidateSurfaceV1,
    options: FixedCombatOptionsV1,
    research: WholeCombatResearchV1,
}

impl FixedCombatResearchV1 {
    pub fn slot_index(&self) -> usize {
        self.slot_index
    }

    pub fn advance_to(
        &mut self,
        work: usize,
    ) -> Result<FixedCombatBranchBatchV1, FixedCombatStepErrorV1> {
        let started = Instant::now();
        let portfolio =
            self.research
                .advance_to(work)
                .map_err(|message| FixedCombatStepErrorV1::Combat {
                    slot_index: self.slot_index,
                    message,
                })?;
        Arc::new(FixedCombatBranchWitnessSetV1 {
            slot_index: self.slot_index,
            source: self.source.clone(),
            entry: self.entry.clone(),
            options: self.options.clone(),
            attempt: portfolio.attempt,
            witness_input_lines: portfolio.witness_input_lines,
        })
        .materialize(
            &self.slot,
            &self.potion_policy,
            self.candidate_surface,
            started,
        )
    }
}

impl FixedCombatBranchWitnessSetV1 {
    fn materialize(
        self: Arc<Self>,
        slot: &LearningEnvPoolSlotV3,
        potion_policy: &CombatLearningPotionPolicyV1,
        candidate_surface: LearningCandidateSurfaceV1,
        started: Instant,
    ) -> Result<FixedCombatBranchBatchV1, FixedCombatStepErrorV1> {
        let failure = |message| FixedCombatStepErrorV1::Combat {
            slot_index: self.slot_index,
            message,
        };
        self.attempt.validate().map_err(failure)?;
        if self.witness_input_lines.len() != self.attempt.candidates.len() {
            return Err(failure(
                "combat portfolio witness alignment mismatch".into(),
            ));
        }
        let candidate_ordinals =
            task_research_plan_non_dominated_candidate_ordinals_v1(&self.attempt.candidates);
        let mut environments = Vec::with_capacity(candidate_ordinals.len());
        for &ordinal in &candidate_ordinals {
            let mut branch = slot.clone();
            let candidate = &self.attempt.candidates[ordinal];
            replay_fixed_combat_candidate(
                &mut branch,
                candidate,
                &self.witness_input_lines[ordinal],
            )
            .map_err(failure)?;
            validated_fixed_combat_exit(&branch, Some(&candidate.outcome)).map_err(failure)?;
            environments.push(
                LearningEnvPoolV3::from_envs_with_potion_policy(
                    [branch.env],
                    potion_policy.clone(),
                )?
                .with_candidate_surface(candidate_surface),
            );
        }
        let choices = candidate_ordinals
            .iter()
            .map(|&candidate_ordinal| FixedCombatBranchChoiceV1 {
                witnesses: self.clone(),
                candidate_ordinal,
            })
            .collect();
        Ok(FixedCombatBranchBatchV1 {
            source: self.source.clone(),
            entry: self.entry.clone(),
            options: self.options.clone(),
            attempt: self.attempt.clone(),
            candidate_ordinals,
            environments,
            choices,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }
}

impl FixedCombatBranchChoiceV1 {
    pub fn slot_index(&self) -> usize {
        self.witnesses.slot_index
    }

    pub fn source(&self) -> &LearningDecisionTokenV3 {
        &self.witnesses.source
    }

    pub fn candidate_ordinal(&self) -> usize {
        self.candidate_ordinal
    }
}

#[derive(Clone, Debug)]
pub enum FixedCombatStepV1 {
    Advanced {
        transitions: Vec<LearningTransitionV3>,
        evidence: FixedCombatBatchEvidenceV1,
    },
    Unresolved {
        slot_index: usize,
        evidence: FixedCombatBatchEvidenceV1,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum FixedCombatStepErrorV1 {
    Pool(LearningEnvPoolError),
    InvalidOptions(String),
    StrategicSourceRequired { slot_index: usize },
    CombatSourceRequired { slot_index: usize },
    Combat { slot_index: usize, message: String },
}

impl fmt::Display for FixedCombatStepErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for FixedCombatStepErrorV1 {}
impl From<LearningEnvPoolError> for FixedCombatStepErrorV1 {
    fn from(error: LearningEnvPoolError) -> Self {
        Self::Pool(error)
    }
}

impl LearningEnvPoolV3 {
    /// Retain the whole-combat solver under one declared total budget. Each
    /// advance publishes actual exits and source-bound choices, with cumulative
    /// work in the attempt receipt. No automatic stopping or ranking is added.
    pub fn start_fixed_combat_research_v1(
        &self,
        slot_index: usize,
        options: FixedCombatOptionsV1,
    ) -> Result<FixedCombatResearchV1, FixedCombatStepErrorV1> {
        if self.is_poisoned() {
            return Err(LearningEnvPoolError::PoolPoisoned.into());
        }
        self.validate_fixed_combat_options(&options)?;
        if options.backend != FixedCombatSearchBackendV1::WholeCombat
            || options.fallback_action_limit.is_some()
        {
            return Err(FixedCombatStepErrorV1::InvalidOptions(
                "incremental combat research requires whole_combat without fallback".into(),
            ));
        }
        let source = self.decision_token(slot_index)?;
        let slot = &self.slots[slot_index];
        if !matches!(slot.boundary, LearningBoundaryV1::Combat { .. }) {
            return Err(FixedCombatStepErrorV1::CombatSourceRequired { slot_index });
        }
        let research = WholeCombatResearchV1::from_session(
            &slot.env.clone().into_session(),
            options.search.clone(),
        )
        .map_err(|message| FixedCombatStepErrorV1::Combat {
            slot_index,
            message,
        })?;
        Ok(FixedCombatResearchV1 {
            slot_index,
            source,
            entry: self.public_run_context(slot_index)?,
            slot: slot.clone(),
            potion_policy: self.potion_policy.clone(),
            candidate_surface: self.candidate_surface(),
            options,
            research,
        })
    }

    /// Searches one current combat without changing any source slot, then
    /// exact-replays every sampled nondominated witness into a separate slot.
    /// The returned pools can use the ordinary strategic controller and fixed
    /// combat executor to study continuation value without an HP exchange rate.
    pub fn fixed_combat_branches_v1(
        &self,
        slot_index: usize,
        options: FixedCombatOptionsV1,
    ) -> Result<FixedCombatBranchBatchV1, FixedCombatStepErrorV1> {
        if self.is_poisoned() {
            return Err(LearningEnvPoolError::PoolPoisoned.into());
        }
        self.validate_fixed_combat_options(&options)?;
        if options.fallback_action_limit.is_some() {
            return Err(FixedCombatStepErrorV1::InvalidOptions(
                "combat branches require witnesses, not fallback rollout".into(),
            ));
        }
        let source = self.decision_token(slot_index)?;
        let slot = &self.slots[slot_index];
        if !matches!(slot.boundary, LearningBoundaryV1::Combat { .. }) {
            return Err(FixedCombatStepErrorV1::CombatSourceRequired { slot_index });
        }
        let failure = |message| FixedCombatStepErrorV1::Combat {
            slot_index,
            message,
        };
        let started = Instant::now();
        let entry = self.public_run_context(slot_index)?;
        let portfolio = fixed_combat_portfolio(
            &slot.env.clone().into_session(),
            options.backend,
            options.search.clone(),
        )
        .map_err(failure)?;
        Arc::new(FixedCombatBranchWitnessSetV1 {
            slot_index,
            source: source.clone(),
            entry: entry.clone(),
            options: options.clone(),
            attempt: portfolio.attempt,
            witness_input_lines: portfolio.witness_input_lines,
        })
        .materialize(slot, &self.potion_policy, self.candidate_surface(), started)
    }

    /// Replays only the chosen source combat, never its researched future.
    /// Exact source identity is required, and every other slot is untouched.
    /// Receipt elapsed_ms measures adoption replay, excluding earlier research.
    pub fn adopt_fixed_combat_branch_v1(
        &mut self,
        choice: &FixedCombatBranchChoiceV1,
    ) -> Result<FixedCombatReceiptV1, FixedCombatStepErrorV1> {
        if self.is_poisoned() {
            return Err(LearningEnvPoolError::PoolPoisoned.into());
        }
        let witnesses = &choice.witnesses;
        let slot_index = witnesses.slot_index;
        self.validate_fixed_combat_options(&witnesses.options)?;
        if self.decision_token(slot_index)? != witnesses.source {
            return Err(LearningEnvPoolError::StaleDecisionToken { slot_index }.into());
        }
        if !matches!(
            self.slots[slot_index].boundary,
            LearningBoundaryV1::Combat { .. }
        ) {
            return Err(FixedCombatStepErrorV1::CombatSourceRequired { slot_index });
        }
        let failure = |message| FixedCombatStepErrorV1::Combat {
            slot_index,
            message,
        };
        let started = Instant::now();
        let ordinal = choice.candidate_ordinal;
        let candidate = &witnesses.attempt.candidates[ordinal];
        let mut branch = self.slots[slot_index].clone();
        replay_fixed_combat_candidate(
            &mut branch,
            candidate,
            &witnesses.witness_input_lines[ordinal],
        )
        .map_err(failure)?;
        let exit =
            validated_fixed_combat_exit(&branch, Some(&candidate.outcome)).map_err(failure)?;
        let receipt = FixedCombatReceiptV1 {
            slot_index,
            entry: witnesses.entry.clone(),
            entry_decision_step: witnesses.source.decision_step,
            attempt: witnesses.attempt.clone(),
            selected_candidate_ordinal: Some(ordinal),
            fallback: None,
            exit: Some(exit),
            exit_decision_step: Some(branch.env.decision_step()),
            elapsed_ms: started.elapsed().as_millis() as u64,
        };
        self.slots[slot_index] = branch;
        Ok(receipt)
    }

    /// Applies exactly one strategic action per active slot, then settles only
    /// the combat it enters. Rewards, maps and subsequent choices stay with the
    /// caller. Config is supplied per call; checkpoints own no executor policy.
    /// Both an error and Unresolved leave every slot and source token unchanged.
    pub fn step_with_fixed_combat_v1(
        &mut self,
        paths: Vec<LearningDecisionPathV3>,
        options: FixedCombatOptionsV1,
    ) -> Result<FixedCombatStepV1, FixedCombatStepErrorV1> {
        let backend = options.backend;
        self.step_with_combat_search(paths, options, |session, options| {
            fixed_combat_portfolio(session, backend, options)
        })
    }

    fn validate_fixed_combat_options(
        &self,
        options: &FixedCombatOptionsV1,
    ) -> Result<(), FixedCombatStepErrorV1> {
        options
            .search
            .validate()
            .map_err(FixedCombatStepErrorV1::InvalidOptions)?;
        if options.fallback_action_limit == Some(0) {
            return Err(FixedCombatStepErrorV1::InvalidOptions(
                "fallback action limit must be positive".into(),
            ));
        }
        if self.potion_policy != CombatLearningPotionPolicyV1::All
            && options.search.potion_policy
                != crate::ai::combat_witness_contract::CombatWitnessPotionPolicyV1::Never
        {
            return Err(FixedCombatStepErrorV1::InvalidOptions(
                "restricted environment requires a never-potion executor".into(),
            ));
        }
        Ok(())
    }

    fn step_with_combat_search(
        &mut self,
        paths: Vec<LearningDecisionPathV3>,
        options: FixedCombatOptionsV1,
        mut search: impl FnMut(
            &RunControlSession,
            CombatResearchPlanOptionsV1,
        ) -> Result<CombatResearchPortfolioV1, String>,
    ) -> Result<FixedCombatStepV1, FixedCombatStepErrorV1> {
        self.validate_fixed_combat_options(&options)?;
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if !matches!(
                slot.boundary,
                LearningBoundaryV1::Strategic { .. } | LearningBoundaryV1::Terminal { .. }
            ) {
                return Err(FixedCombatStepErrorV1::StrategicSourceRequired { slot_index });
            }
        }
        let mut trial = self.clone();
        let mut transitions = trial.step_active_paths_v3(paths)?;
        let mut evidence = FixedCombatBatchEvidenceV1 {
            executor_revision: FIXED_COMBAT_EXECUTOR_REVISION_V1,
            information_access: "oracle_exact_combat".into(),
            options: options.clone(),
            combats: Vec::new(),
        };
        for transition in &mut transitions {
            let slot_index = transition.slot_index;
            if !matches!(
                trial.slots[slot_index].boundary,
                LearningBoundaryV1::Combat { .. }
            ) {
                continue;
            }
            let failure = |message| FixedCombatStepErrorV1::Combat {
                slot_index,
                message,
            };
            let started = Instant::now();
            let entry = trial.public_run_context(slot_index)?;
            let slot = &mut trial.slots[slot_index];
            let entry_decision_step = slot.env.decision_step();
            let portfolio = search(&slot.env.clone().into_session(), options.search.clone())
                .map_err(failure)?;
            portfolio.attempt.validate().map_err(failure)?;
            if portfolio.witness_input_lines.len() != portfolio.attempt.candidates.len() {
                return Err(failure(
                    "combat portfolio witness alignment mismatch".into(),
                ));
            }
            let selected = select_fixed_combat_candidate_v1(&portfolio.attempt.candidates);
            let mut receipt = FixedCombatReceiptV1 {
                slot_index,
                entry,
                entry_decision_step,
                attempt: portfolio.attempt,
                selected_candidate_ordinal: selected,
                fallback: None,
                exit: None,
                exit_decision_step: None,
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
            if selected.is_none() {
                if let Some(limit) = options.fallback_action_limit {
                    let potion_policy = if options.search.potion_policy
                        == crate::ai::combat_witness_contract::CombatWitnessPotionPolicyV1::Never
                    {
                        CombatLearningPotionPolicyV1::never()
                    } else {
                        trial.potion_policy.clone()
                    };
                    receipt.fallback =
                        Some(public_combat_rollout(slot, &potion_policy, limit).map_err(failure)?);
                }
                if receipt
                    .fallback
                    .as_ref()
                    .is_none_or(|fallback| fallback.action_limit_reached)
                {
                    receipt.elapsed_ms = started.elapsed().as_millis() as u64;
                    evidence.combats.push(receipt);
                    return Ok(FixedCombatStepV1::Unresolved {
                        slot_index,
                        evidence,
                    });
                }
            }
            if let Some(ordinal) = selected {
                replay_fixed_combat_candidate(
                    slot,
                    &receipt.attempt.candidates[ordinal],
                    &portfolio.witness_input_lines[ordinal],
                )
                .map_err(failure)?;
            }
            let exit = validated_fixed_combat_exit(
                slot,
                selected.map(|ordinal| &receipt.attempt.candidates[ordinal].outcome),
            )
            .map_err(failure)?;
            receipt.exit = Some(exit);
            receipt.exit_decision_step = Some(slot.env.decision_step());
            receipt.elapsed_ms = started.elapsed().as_millis() as u64;
            transition.reward = slot.boundary.terminal_reward();
            transition.terminal_outcome = match &slot.boundary {
                LearningBoundaryV1::Terminal { outcome } => Some(outcome.clone()),
                _ => None,
            };
            transition.next = if transition.terminal_outcome.is_none() {
                Some(trial.decision_token(slot_index)?)
            } else {
                None
            };
            evidence.combats.push(receipt);
        }
        *self = trial;
        Ok(FixedCombatStepV1::Advanced {
            transitions,
            evidence,
        })
    }
}

fn fixed_combat_portfolio(
    session: &RunControlSession,
    backend: FixedCombatSearchBackendV1,
    options: CombatResearchPlanOptionsV1,
) -> Result<CombatResearchPortfolioV1, String> {
    match backend {
        FixedCombatSearchBackendV1::Portfolio => {
            run_control_session_combat_research_portfolio_with_options_v1(session, Some(options))
        }
        FixedCombatSearchBackendV1::WholeCombat => {
            run_control_session_whole_combat_portfolio_v1(session, options)
        }
    }
}

fn select_fixed_combat_candidate_v1(candidates: &[TaskResearchPlanCandidateV1]) -> Option<usize> {
    task_research_plan_non_dominated_candidate_ordinals_v1(candidates)
        .into_iter()
        .filter(|&ordinal| {
            !candidates.iter().any(|other| {
                other
                    .outcome
                    .rewarded_exit_strictly_dominates_v1(&candidates[ordinal].outcome)
            })
        })
        .min_by_key(|&ordinal| {
            let candidate = &candidates[ordinal];
            (
                Reverse(candidate.outcome.final_hp),
                candidate.outcome.total_potions_consumed,
                candidate.plan.steps.len(),
                ordinal,
            )
        })
}

fn replay_fixed_combat_candidate(
    slot: &mut LearningEnvPoolSlotV3,
    candidate: &TaskResearchPlanCandidateV1,
    inputs: &[crate::state::core::ClientInput],
) -> Result<(), String> {
    if inputs.is_empty() || inputs.len() != candidate.plan.steps.len() {
        return Err("combat portfolio has an invalid complete witness length".into());
    }
    for input in inputs {
        if !matches!(slot.boundary, LearningBoundaryV1::Combat { .. }) {
            return Err("fixed combat witness continued after combat exit".into());
        }
        let action = slot.env.prepare_action(LearningActionV1::CombatInput {
            input: input.clone(),
        })?;
        slot.boundary = slot.env.step_prepared(action)?.boundary;
    }
    Ok(())
}

fn validated_fixed_combat_exit(
    slot: &LearningEnvPoolSlotV3,
    outcome: Option<&TaskResearchPlanOutcomeV1>,
) -> Result<LearningPublicRunContextV1, String> {
    if !matches!(
        slot.boundary,
        LearningBoundaryV1::Strategic { .. } | LearningBoundaryV1::Terminal { .. }
    ) {
        return Err("fixed combat witness did not reach a represented exit".into());
    }
    let exit = slot.env.public_run_context(&slot.boundary)?;
    if let Some(outcome) = outcome {
        if (exit.hp, exit.max_hp, exit.gold)
            != (
                outcome.final_hp,
                outcome.persistent.max_hp,
                outcome.final_gold,
            )
            || slot.boundary.terminal_reward() < 0
        {
            return Err("fixed combat replay disagrees with witnessed resources".into());
        }
    }
    Ok(exit)
}

fn public_combat_rollout(
    slot: &mut LearningEnvPoolSlotV3,
    potion_policy: &CombatLearningPotionPolicyV1,
    limit: usize,
) -> Result<FixedCombatFallbackReceiptV1, String> {
    use crate::eval::run_control::{
        learning_public_information_snapshot_with_potion_policy_v1,
        public_deterministic_baseline_decision_v1,
        public_deterministic_baseline_selection_decision_v1,
        PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
    };
    let mut receipt = FixedCombatFallbackReceiptV1 {
        baseline_revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
        steps: Vec::new(),
        action_limit_reached: false,
    };
    for _ in 0..limit {
        if !matches!(slot.boundary, LearningBoundaryV1::Combat { .. }) {
            return Ok(receipt);
        }
        let snapshot = learning_public_information_snapshot_with_potion_policy_v1(
            &slot.boundary,
            potion_policy,
        )?;
        let decision = LearningModelDecisionV1::from_boundary_with_potion_policy(
            &slot.boundary,
            potion_policy,
        )
        .map_err(|e| e.to_string())?;
        let root = public_deterministic_baseline_decision_v1(&decision).candidate_ordinal;
        let mut ordinals = vec![root];
        if let LearningModelChoiceV1::DecodeSelection(mut draft) =
            decision.choose(root).map_err(|e| e.to_string())?
        {
            for _ in 0..1024 {
                let ordinal = public_deterministic_baseline_selection_decision_v1(
                    decision.observation,
                    &draft,
                )
                .candidate_ordinal;
                ordinals.push(ordinal);
                if matches!(
                    draft.choose(ordinal).map_err(|e| e.to_string())?,
                    LearningSelectionStepV1::Apply(_)
                ) {
                    break;
                }
            }
        }
        // The shared decoder also rejects a selection prefix that hit its cap.
        let action = resolve_decision_path_v3(&decision, &ordinals).map_err(|e| e.to_string())?;
        let prepared = slot.env.prepare_action(action)?;
        slot.boundary = slot.env.step_prepared(prepared)?.boundary;
        receipt.steps.push(FixedCombatFallbackStepV1 {
            source_public_snapshot_id: snapshot.snapshot_id,
            action_ordinals: ordinals,
        });
    }
    receipt.action_limit_reached = matches!(slot.boundary, LearningBoundaryV1::Combat { .. });
    Ok(receipt)
}
