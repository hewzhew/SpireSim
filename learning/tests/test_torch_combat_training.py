from __future__ import annotations

import importlib.util
import unittest

from learning.tests.torch_combat_fixtures import combat_group_experience_fixture
from learning.tests.torch_outcome_fixtures import behavior_manifest_fixture
from sts_learning import (
    BehaviorManifestRegistry,
    CombatPolicyUpdateConfig,
    CombatPolicyUpdateRule,
    CombatWinObjectiveConfig,
    SemanticBatchConcatLimits,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_combat_training import (
        CombatWinTrainingStatus,
        SynchronousCombatWinTrainer,
        TorchCombatTrainingError,
    )
    from sts_learning.torch_policy import (
        RaggedCandidateLogits,
        RaggedCategoricalPolicyConfig,
        RaggedMultiActorCriticOutput,
    )


CONCAT_LIMITS = SemanticBatchConcatLimits(
    max_rows=16,
    max_input_array_bytes=1024 * 1024,
)


if _TORCH_AVAILABLE:

    class _VectorScorer(torch.nn.Module):
        def __init__(self, *, zero_gradient: bool = False) -> None:
            super().__init__()
            self.values = torch.nn.Parameter(torch.zeros(7))
            self.zero_gradient = zero_gradient

        def forward(self, payload):
            values = self.values * 0.0 if self.zero_gradient else self.values
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )


    class _SizedActorCritic(torch.nn.Module):
        """A row-count-agnostic scorer for concat-boundary equivalence tests."""

        def __init__(self) -> None:
            super().__init__()
            self.actor_scale = torch.nn.Parameter(torch.zeros(()))
            self.value_columns = torch.nn.Parameter(torch.zeros(3))

        def forward(self, payload):
            row_splits = torch.as_tensor(
                payload["candidate_row_splits"],
                dtype=torch.long,
            )
            candidate_count = int(row_splits[-1])
            features = torch.arange(
                candidate_count,
                dtype=self.actor_scale.dtype,
            )
            return RaggedCandidateLogits(
                values=self.actor_scale * features,
                row_splits=row_splits,
            )

        def actor_critic_multi(self, payload):
            logits = self(payload)
            return RaggedMultiActorCriticOutput(
                logits=logits,
                row_values=self.value_columns.unsqueeze(0).expand(
                    logits.row_count,
                    -1,
                ),
            )


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class SynchronousCombatWinTrainerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.policy_config = RaggedCategoricalPolicyConfig(temperature=1.0)
        self.registry = BehaviorManifestRegistry(capacity=1)
        self.manifest_id = self.registry.register(
            behavior_manifest_fixture(
                behavior_rule=self.policy_config.behavior_rule,
            )
        )

    def test_nonzero_win_gradient_commits_exactly_one_optimizer_step(self) -> None:
        scorer = _VectorScorer()
        before = scorer.values.detach().clone()
        trainer = self._trainer(scorer)

        result = trainer.train(
            (combat_group_experience_fixture(self.manifest_id, wins=(True, False)),)
        )

        self.assertEqual(result.status, CombatWinTrainingStatus.OPTIMIZER_STEP)
        self.assertTrue(result.updated)
        self.assertEqual(result.optimizer_steps_after, 1)
        self.assertFalse(torch.equal(scorer.values.detach(), before))
        snapshot = trainer.snapshot
        self.assertEqual(snapshot.deliveries, 1)
        self.assertEqual(snapshot.optimizer_steps, 1)
        self.assertEqual(snapshot.completed_groups, 1)
        self.assertEqual(snapshot.signal_groups, 1)
        self.assertEqual(snapshot.win_signal_groups, 1)
        self.assertEqual(snapshot.terminal_hp_signal_groups, 0)
        self.assertEqual(snapshot.no_update_deliveries, 0)
        self.assertEqual(snapshot.trained_replicates, 2)
        self.assertEqual(snapshot.trained_decisions, 3)
        self.assertFalse(snapshot.poisoned)

    def test_no_objective_signal_does_not_touch_optimizer_or_claim_a_step(self) -> None:
        scorer = _VectorScorer()
        trainer = self._trainer(scorer)
        before = scorer.values.detach().clone()

        result = trainer.train(
            (
                combat_group_experience_fixture(
                    self.manifest_id,
                    wins=(True, True),
                    final_hps=(70, 70),
                ),
            )
        )

        self.assertEqual(result.status, CombatWinTrainingStatus.NO_OBJECTIVE_SIGNAL)
        self.assertFalse(result.updated)
        self.assertEqual(result.optimizer_steps_after, 0)
        torch.testing.assert_close(scorer.values.detach(), before)
        self.assertEqual(trainer.snapshot.no_update_deliveries, 1)
        self.assertEqual(trainer.snapshot.trained_decisions, 0)

    def test_all_win_hp_signal_commits_an_optimizer_step(self) -> None:
        scorer = _VectorScorer()
        trainer = self._trainer(scorer)

        result = trainer.train(
            (combat_group_experience_fixture(self.manifest_id, wins=(True, True)),)
        )

        self.assertEqual(result.status, CombatWinTrainingStatus.OPTIMIZER_STEP)
        self.assertEqual(result.win_signal_group_count, 0)
        self.assertEqual(result.terminal_hp_signal_group_count, 1)
        self.assertEqual(trainer.snapshot.win_signal_groups, 0)
        self.assertEqual(trainer.snapshot.terminal_hp_signal_groups, 1)

    def test_win_signal_with_zero_policy_gradient_does_not_claim_a_step(self) -> None:
        scorer = _VectorScorer(zero_gradient=True)
        trainer = self._trainer(scorer)

        result = trainer.train(
            (combat_group_experience_fixture(self.manifest_id, wins=(True, False)),)
        )

        self.assertEqual(
            result.status,
            CombatWinTrainingStatus.ZERO_POLICY_GRADIENT,
        )
        self.assertFalse(result.updated)
        self.assertEqual(result.optimizer_steps_after, 0)
        self.assertEqual(trainer.snapshot.signal_groups, 1)
        self.assertEqual(trainer.snapshot.no_update_deliveries, 1)
        self.assertFalse(trainer.snapshot.poisoned)

    def test_delivery_width_is_checked_before_objective_execution(self) -> None:
        scorer = _VectorScorer()
        trainer = SynchronousCombatWinTrainer(
            scorer,
            torch.optim.SGD(scorer.parameters(), lr=0.1),
            self.registry,
            CONCAT_LIMITS,
            self.policy_config,
            CombatWinObjectiveConfig(groups_per_update=2),
        )

        with self.assertRaisesRegex(TorchCombatTrainingError, "groups_per_update"):
            trainer.train(
                (
                    combat_group_experience_fixture(
                        self.manifest_id,
                        wins=(True, False),
                    ),
                )
            )
        self.assertEqual(trainer.snapshot.deliveries, 0)

    def test_ppo_clip_applies_bounded_multiple_steps_to_one_frozen_batch(self) -> None:
        scorer = _VectorScorer()
        trainer = SynchronousCombatWinTrainer(
            scorer,
            torch.optim.SGD(scorer.parameters(), lr=0.01),
            self.registry,
            CONCAT_LIMITS,
            self.policy_config,
            CombatWinObjectiveConfig(
                policy_update=CombatPolicyUpdateConfig(
                    rule=CombatPolicyUpdateRule.PPO_CLIP,
                    epochs=4,
                    clip_coefficient=0.2,
                    entropy_coefficient=0.01,
                    max_grad_norm=0.5,
                    target_kl=None,
                )
            ),
        )

        result = trainer.train(
            (combat_group_experience_fixture(self.manifest_id, wins=(True, False)),)
        )

        self.assertEqual(result.status, CombatWinTrainingStatus.OPTIMIZER_STEP)
        self.assertEqual(result.optimizer_steps_applied, 4)
        self.assertEqual(result.optimizer_steps_after, 4)
        self.assertGreaterEqual(result.approximate_kl, 0.0)
        self.assertGreater(result.entropy, 0.0)
        self.assertEqual(trainer.snapshot.trained_replicates, 8)
        self.assertEqual(trainer.snapshot.trained_decisions, 12)

    def test_value_ppo_microbatches_match_one_materialized_objective(self) -> None:
        objective = CombatWinObjectiveConfig(
            policy_update=CombatPolicyUpdateConfig(
                rule=CombatPolicyUpdateRule.PPO_CLIP_VALUE,
                epochs=2,
                clip_coefficient=0.2,
                entropy_coefficient=0.01,
                max_grad_norm=None,
                target_kl=None,
                value_loss_coefficient=0.5,
            )
        )
        experience = (
            combat_group_experience_fixture(
                self.manifest_id,
                wins=(True, False),
            ),
        )
        full_scorer = _SizedActorCritic()
        micro_scorer = _SizedActorCritic()
        full = SynchronousCombatWinTrainer(
            full_scorer,
            torch.optim.SGD(full_scorer.parameters(), lr=0.01),
            self.registry,
            CONCAT_LIMITS,
            self.policy_config,
            objective,
        ).train(experience)
        micro = SynchronousCombatWinTrainer(
            micro_scorer,
            torch.optim.SGD(micro_scorer.parameters(), lr=0.01),
            self.registry,
            SemanticBatchConcatLimits(
                max_rows=2,
                max_input_array_bytes=1024 * 1024,
            ),
            self.policy_config,
            objective,
        ).train(experience)

        self.assertEqual(full.optimizer_steps_applied, 2)
        self.assertEqual(micro.optimizer_steps_applied, 2)
        self.assertEqual(full.decision_count, micro.decision_count)
        self.assertAlmostEqual(full.loss, micro.loss, places=7)
        self.assertAlmostEqual(full.approximate_kl, micro.approximate_kl, places=7)
        self.assertAlmostEqual(full.clip_fraction, micro.clip_fraction, places=7)
        self.assertAlmostEqual(full.entropy, micro.entropy, places=7)
        self.assertAlmostEqual(full.value_loss, micro.value_loss, places=7)
        torch.testing.assert_close(
            full_scorer.actor_scale,
            micro_scorer.actor_scale,
        )
        torch.testing.assert_close(
            full_scorer.value_columns,
            micro_scorer.value_columns,
        )

    def _trainer(self, scorer: _VectorScorer) -> SynchronousCombatWinTrainer:
        return SynchronousCombatWinTrainer(
            scorer,
            torch.optim.SGD(scorer.parameters(), lr=0.1),
            self.registry,
            CONCAT_LIMITS,
            self.policy_config,
            CombatWinObjectiveConfig(groups_per_update=1),
        )


if __name__ == "__main__":
    unittest.main()
