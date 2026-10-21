from __future__ import annotations

import importlib.util
import math
import unittest

from learning.tests.torch_combat_fixtures import combat_group_experience_fixture
from learning.tests.torch_outcome_fixtures import behavior_manifest_fixture
from sts_learning import (
    BehaviorManifestId,
    BehaviorManifestRegistry,
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatPolicyUpdateConfig,
    CombatPolicyUpdateRule,
    CombatWinObjectiveConfig,
    SelectionProbability,
    SemanticBatchConcatLimits,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_outcomes import (
        TorchOutcomeError,
        on_policy_combat_win_loss,
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


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class OnPolicyCombatWinLossTests(unittest.TestCase):
    def setUp(self) -> None:
        self.config = RaggedCategoricalPolicyConfig(temperature=1.0)
        self.objective = CombatWinObjectiveConfig()
        self.registry = BehaviorManifestRegistry(capacity=1)
        self.manifest_id = self.registry.register(
            behavior_manifest_fixture(behavior_rule=self.config.behavior_rule)
        )

    def test_one_forward_weights_replicates_equally_not_decisions(self) -> None:
        values = torch.nn.Parameter(torch.zeros(7))
        calls = 0

        def scorer(payload):
            nonlocal calls
            calls += 1
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        result = on_policy_combat_win_loss(
            scorer,
            (combat_group_experience_fixture(self.manifest_id, wins=(True, False)),),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.objective,
        )
        result.value.backward()

        self.assertEqual(calls, 1)
        self.assertEqual(result.group_count, 1)
        self.assertEqual(result.signal_group_count, 1)
        self.assertEqual(result.win_signal_group_count, 1)
        self.assertEqual(result.terminal_hp_signal_group_count, 0)
        self.assertEqual(result.enemy_hp_progress_signal_group_count, 0)
        self.assertEqual(result.replicate_count, 2)
        self.assertEqual(result.decision_count, 3)
        self.assertAlmostEqual(
            float(result.value.detach()),
            0.25 * (math.log(2.0) - math.log(3.0)),
            places=6,
        )
        expected = torch.tensor(
            [-0.25, 0.25, 1.0 / 6.0, -1.0 / 12.0, -1.0 / 12.0, 0.125, -0.125]
        )
        torch.testing.assert_close(values.grad, expected)

    def test_all_win_group_uses_hp_after_win_axis_is_solved(self) -> None:
        values = torch.nn.Parameter(torch.zeros(7))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        result = on_policy_combat_win_loss(
            scorer,
            (combat_group_experience_fixture(self.manifest_id, wins=(True, True)),),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.objective,
        )
        result.value.backward()

        self.assertEqual(result.signal_group_count, 1)
        self.assertEqual(result.win_signal_group_count, 0)
        self.assertEqual(result.terminal_hp_signal_group_count, 1)
        self.assertEqual(result.enemy_hp_progress_signal_group_count, 0)
        self.assertAlmostEqual(
            float(result.value.detach()),
            0.1875 * (math.log(2.0) - math.log(3.0)),
            places=6,
        )

    def test_all_win_axis_can_explicitly_disable_hp_learning(self) -> None:
        values = torch.nn.Parameter(torch.zeros(7))

        result = on_policy_combat_win_loss(
            lambda payload: RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            ),
            (combat_group_experience_fixture(self.manifest_id, wins=(True, True)),),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            CombatWinObjectiveConfig(all_win_axis=CombatAllWinAxis.NONE),
        )
        result.value.backward()

        self.assertEqual(result.signal_group_count, 0)
        self.assertEqual(result.win_signal_group_count, 0)
        self.assertEqual(result.terminal_hp_signal_group_count, 0)
        self.assertEqual(result.enemy_hp_progress_signal_group_count, 0)
        torch.testing.assert_close(values.grad, torch.zeros_like(values))

    def test_mixed_outcomes_use_win_only_even_when_hp_also_varies(self) -> None:
        result = on_policy_combat_win_loss(
            lambda payload: RaggedCandidateLogits(
                values=torch.zeros(7, requires_grad=True),
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            ),
            (combat_group_experience_fixture(self.manifest_id, wins=(True, False)),),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.objective,
        )

        self.assertEqual(result.win_signal_group_count, 1)
        self.assertEqual(result.terminal_hp_signal_group_count, 0)
        self.assertEqual(result.enemy_hp_progress_signal_group_count, 0)

    def test_all_loss_and_potion_only_variation_have_no_signal(self) -> None:
        for wins, final_hps in (
            ((False, False), (0, 0)),
            ((True, True), (70, 70)),
        ):
            with self.subTest(wins=wins):
                values = torch.nn.Parameter(torch.zeros(7))

                def scorer(payload):
                    return RaggedCandidateLogits(
                        values=values,
                        row_splits=torch.as_tensor(
                            payload["candidate_row_splits"],
                            dtype=torch.long,
                        ),
                    )

                result = on_policy_combat_win_loss(
                    scorer,
                    (
                        combat_group_experience_fixture(
                            self.manifest_id,
                            wins=wins,
                            final_hps=final_hps,
                            enemy_final_hps=(30, 10),
                            potions_used=(0, 1),
                        ),
                    ),
                    self.registry,
                    CONCAT_LIMITS,
                    self.config,
                    self.objective,
                )
                result.value.backward()

                self.assertEqual(result.signal_group_count, 0)
                self.assertEqual(result.win_signal_group_count, 0)
                self.assertEqual(result.terminal_hp_signal_group_count, 0)
                self.assertEqual(result.enemy_hp_progress_signal_group_count, 0)
                self.assertEqual(float(result.value.detach()), 0.0)
                torch.testing.assert_close(values.grad, torch.zeros_like(values))

    def test_all_loss_enemy_hp_progress_requires_explicit_opt_in(self) -> None:
        values = torch.nn.Parameter(torch.zeros(7))
        result = on_policy_combat_win_loss(
            lambda payload: RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            ),
            (
                combat_group_experience_fixture(
                    self.manifest_id,
                    wins=(False, False),
                    enemy_final_hps=(30, 10),
                ),
            ),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            CombatWinObjectiveConfig(
                all_loss_axis=CombatAllLossAxis.ENEMY_HP_PROGRESS,
            ),
        )
        result.value.backward()

        self.assertEqual(result.signal_group_count, 1)
        self.assertEqual(result.win_signal_group_count, 0)
        self.assertEqual(result.terminal_hp_signal_group_count, 0)
        self.assertEqual(result.enemy_hp_progress_signal_group_count, 1)
        self.assertTrue(bool(torch.any(values.grad != 0)))

    def test_unresolved_escapes_never_use_the_all_loss_axis(self) -> None:
        values = torch.nn.Parameter(torch.zeros(7))
        result = on_policy_combat_win_loss(
            lambda payload: RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            ),
            (
                combat_group_experience_fixture(
                    self.manifest_id,
                    wins=(False, False),
                    enemy_final_hps=(30, 10),
                    terminal_kinds=(2, 2),
                ),
            ),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            CombatWinObjectiveConfig(
                all_loss_axis=CombatAllLossAxis.ENEMY_HP_PROGRESS,
            ),
        )
        result.value.backward()

        self.assertEqual(result.signal_group_count, 0)
        self.assertEqual(result.enemy_hp_progress_signal_group_count, 0)
        torch.testing.assert_close(values.grad, torch.zeros_like(values))

    def test_unknown_behavior_and_off_policy_probability_fail_closed(self) -> None:
        def scorer(payload):
            return RaggedCandidateLogits(
                values=torch.zeros(7),
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        unknown = BehaviorManifestId(b"\xff" * 32)
        with self.assertRaisesRegex(TorchOutcomeError, "unknown behavior"):
            on_policy_combat_win_loss(
                scorer,
                (combat_group_experience_fixture(unknown, wins=(True, False)),),
                self.registry,
                CONCAT_LIMITS,
                self.config,
                self.objective,
            )
        with self.assertRaisesRegex(TorchOutcomeError, "off-policy"):
            on_policy_combat_win_loss(
                scorer,
                (
                    combat_group_experience_fixture(
                        self.manifest_id,
                        wins=(True, False),
                        first_probability=SelectionProbability.known(0.4),
                    ),
                ),
                self.registry,
                CONCAT_LIMITS,
                self.config,
                self.objective,
            )

    def test_ppo_clip_reuses_recorded_probabilities_with_drift_diagnostics(self) -> None:
        values = torch.nn.Parameter(torch.zeros(7))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        objective = CombatWinObjectiveConfig(
            policy_update=CombatPolicyUpdateConfig(
                rule=CombatPolicyUpdateRule.PPO_CLIP,
                epochs=2,
                clip_coefficient=0.2,
                entropy_coefficient=0.0,
                max_grad_norm=0.5,
                target_kl=None,
            )
        )
        experience = combat_group_experience_fixture(
            self.manifest_id,
            wins=(True, False),
        )
        initial = on_policy_combat_win_loss(
            scorer,
            (experience,),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            objective,
        )
        self.assertAlmostEqual(initial.approximate_kl, 0.0, places=7)
        self.assertAlmostEqual(initial.clip_fraction, 0.0, places=7)
        self.assertGreater(initial.entropy, 0.0)

        with torch.no_grad():
            values[0] = 3.0
        reused = on_policy_combat_win_loss(
            scorer,
            (experience,),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            objective,
            require_matching_propensities=False,
        )
        self.assertGreater(reused.approximate_kl, 0.0)
        self.assertGreater(reused.clip_fraction, 0.0)
        self.assertTrue(torch.isfinite(reused.value))

    def test_value_ppo_trains_a_neutral_critic_without_changing_first_actor_axis(
        self,
    ) -> None:
        class _ActorCritic(torch.nn.Module):
            def __init__(self) -> None:
                super().__init__()
                self.logits = torch.nn.Parameter(torch.zeros(7))
                self.values = torch.nn.Parameter(torch.zeros((3, 3)))

            def forward(self, payload):
                return RaggedCandidateLogits(
                    values=self.logits,
                    row_splits=torch.as_tensor(
                        payload["candidate_row_splits"],
                        dtype=torch.long,
                    ),
                )

            def actor_critic_multi(self, payload):
                return RaggedMultiActorCriticOutput(
                    logits=self(payload),
                    row_values=self.values,
                )

        scorer = _ActorCritic()
        objective = CombatWinObjectiveConfig(
            policy_update=CombatPolicyUpdateConfig.ppo_clip_value(),
        )

        result = on_policy_combat_win_loss(
            scorer,
            (
                combat_group_experience_fixture(
                    self.manifest_id,
                    wins=(True, False),
                ),
            ),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            objective,
        )
        result.value.backward()

        self.assertEqual(result.actor_advantages, (1.0, 0.0, 0.0))
        self.assertGreater(result.value_loss, 0.0)
        self.assertTrue(bool(torch.any(scorer.values.grad[:, 0] != 0)))
        self.assertTrue(bool(torch.all(scorer.values.grad[:, 1:] == 0)))
        self.assertTrue(bool(torch.any(scorer.logits.grad != 0)))

        with torch.no_grad():
            scorer.values[:, 0] = torch.tensor((0.5, -0.25, 0.75))
        reused = on_policy_combat_win_loss(
            scorer,
            (
                combat_group_experience_fixture(
                    self.manifest_id,
                    wins=(True, False),
                ),
            ),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            objective,
            require_matching_propensities=False,
            fixed_actor_advantages=result.actor_advantages,
        )
        self.assertEqual(reused.actor_advantages, result.actor_advantages)

    def test_value_ppo_fallbacks_use_only_their_fixed_value_column(self) -> None:
        class _MultiCritic(torch.nn.Module):
            def __init__(self) -> None:
                super().__init__()
                self.logits = torch.nn.Parameter(torch.zeros(7))
                self.values = torch.nn.Parameter(torch.zeros((3, 3)))

            def forward(self, payload):
                return RaggedCandidateLogits(
                    values=self.logits,
                    row_splits=torch.as_tensor(
                        payload["candidate_row_splits"],
                        dtype=torch.long,
                    ),
                )

            def actor_critic_multi(self, payload):
                return RaggedMultiActorCriticOutput(
                    logits=self(payload),
                    row_values=self.values,
                )

        cases = (
            (
                CombatWinObjectiveConfig(
                    policy_update=CombatPolicyUpdateConfig.ppo_clip_value(),
                ),
                combat_group_experience_fixture(
                    self.manifest_id,
                    wins=(True, True),
                    final_hps=(70, 10),
                ),
                1,
                (-0.125, -0.875, -0.8125),
            ),
            (
                CombatWinObjectiveConfig(
                    all_loss_axis=CombatAllLossAxis.ENEMY_HP_PROGRESS,
                    policy_update=CombatPolicyUpdateConfig.ppo_clip_value(),
                ),
                combat_group_experience_fixture(
                    self.manifest_id,
                    wins=(False, False),
                    enemy_final_hps=(30, 20),
                ),
                2,
                (0.25, 0.5, 0.125),
            ),
        )

        for objective, experience, selected_column, expected in cases:
            scorer = _MultiCritic()
            result = on_policy_combat_win_loss(
                scorer,
                (experience,),
                self.registry,
                CONCAT_LIMITS,
                self.config,
                objective,
            )
            result.value.backward()

            self.assertEqual(result.actor_advantages, expected)
            assert scorer.values.grad is not None
            for column in range(3):
                has_gradient = bool(torch.any(scorer.values.grad[:, column] != 0))
                self.assertEqual(has_gradient, column == selected_column)

if __name__ == "__main__":
    unittest.main()
