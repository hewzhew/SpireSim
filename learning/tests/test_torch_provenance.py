from __future__ import annotations

import importlib.util
import unittest
from dataclasses import replace

from learning.tests.semantic_fixtures import semantic_schema_fixture
from sts_learning import (
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatPolicyUpdateConfig,
    CombatWinObjectiveConfig,
    FloorProgressReturnConfig,
    OnPolicyObjectiveConfig,
    RunDecisionScope,
    RunPolicyUpdateConfig,
    TerminalAdvantageMode,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None

if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_policy import (
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )
    from sts_learning.torch_provenance import (
        AdamTrainingConfig,
        TorchProvenanceError,
        combat_win_training_manifest_template,
        combat_win_trainer_implementation,
        categorical_training_manifest_template,
        categorical_trainer_implementation,
    )


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class TorchProvenanceTests(unittest.TestCase):
    def test_reward_metric_changes_run_trainer_identity(self) -> None:
        for update in (RunPolicyUpdateConfig(), RunPolicyUpdateConfig.ppo_clip_value()):
            with self.subTest(rule=update.rule.name):
                original = OnPolicyObjectiveConfig(
                    policy_update=update,
                    advantage_mode=(TerminalAdvantageMode.DECISION_LOCAL_GAE
                                    if update.uses_value_baseline else TerminalAdvantageMode.RAW_RETURN),
                )
                act = replace(original, terminal_return=FloorProgressReturnConfig(metric="act_completion"))
                self.assertNotEqual(categorical_trainer_implementation(original),
                                    categorical_trainer_implementation(act))

    def test_combat_win_trainer_has_distinct_exact_provenance(self) -> None:
        schema = semantic_schema_fixture()
        scorer = RaggedScorerConfig(hidden_dim=4, relation_layers=1)
        behavior = RaggedCategoricalPolicyConfig(temperature=0.8)
        optimizer = AdamTrainingConfig(learning_rate=0.002)
        combat = CombatWinObjectiveConfig(groups_per_update=1)
        terminal = OnPolicyObjectiveConfig(
            terminal_return=FloorProgressReturnConfig(target_floor=52),
            attempts_per_update=1,
        )

        combat_template = combat_win_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            combat,
            device_type="cpu",
        )
        wider = combat_win_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            CombatWinObjectiveConfig(groups_per_update=2),
            device_type="cpu",
        )
        win_only = combat_win_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            CombatWinObjectiveConfig(all_win_axis=CombatAllWinAxis.NONE),
            device_type="cpu",
        )
        ppo = combat_win_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            CombatWinObjectiveConfig(
                policy_update=CombatPolicyUpdateConfig.ppo_clip(),
            ),
            device_type="cpu",
        )
        terminal_template = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            terminal,
            device_type="cpu",
        )

        self.assertEqual(
            combat_template.model_definition,
            terminal_template.model_definition,
        )
        self.assertEqual(combat_template.model_config, terminal_template.model_config)
        self.assertEqual(combat_template.behavior_rule, terminal_template.behavior_rule)
        self.assertEqual(
            combat_template.semantic_schema,
            terminal_template.semantic_schema,
        )
        self.assertEqual(
            combat_template.optimizer_config,
            terminal_template.optimizer_config,
        )
        self.assertEqual(
            combat_template.trainer_implementation,
            combat_win_trainer_implementation(combat),
        )
        self.assertEqual(
            terminal_template.trainer_implementation,
            categorical_trainer_implementation(terminal),
        )
        self.assertNotEqual(
            combat_template.trainer_implementation,
            terminal_template.trainer_implementation,
        )
        self.assertNotEqual(
            wider.trainer_implementation,
            combat_template.trainer_implementation,
        )
        self.assertNotEqual(
            win_only.trainer_implementation,
            combat_template.trainer_implementation,
        )
        self.assertNotEqual(
            ppo.trainer_implementation,
            combat_template.trainer_implementation,
        )

    def test_all_loss_axis_preserves_default_identity_and_forks_each_update_rule(
        self,
    ) -> None:
        updates = (
            CombatPolicyUpdateConfig(),
            CombatPolicyUpdateConfig.ppo_clip(),
            CombatPolicyUpdateConfig.ppo_clip_value(),
        )
        for update in updates:
            with self.subTest(rule=update.rule.name):
                default = CombatWinObjectiveConfig(policy_update=update)
                explicit_none = CombatWinObjectiveConfig(
                    policy_update=update,
                    all_loss_axis=CombatAllLossAxis.NONE,
                )
                enabled = CombatWinObjectiveConfig(
                    policy_update=update,
                    all_loss_axis=CombatAllLossAxis.ENEMY_HP_PROGRESS,
                )

                self.assertEqual(
                    combat_win_trainer_implementation(default),
                    combat_win_trainer_implementation(explicit_none),
                )
                self.assertNotEqual(
                    combat_win_trainer_implementation(default),
                    combat_win_trainer_implementation(enabled),
                )

    def test_run_value_ppo_has_distinct_model_and_trainer_identity(self) -> None:
        schema = semantic_schema_fixture()
        behavior = RaggedCategoricalPolicyConfig()
        optimizer = AdamTrainingConfig()
        reinforce = OnPolicyObjectiveConfig(attempts_per_update=2)
        value = replace(
            reinforce,
            advantage_mode=TerminalAdvantageMode.DECISION_LOCAL_GAE,
            policy_update=RunPolicyUpdateConfig.ppo_clip_value(),
        )
        unnormalized_value = replace(
            value,
            policy_update=replace(
                value.policy_update,
                normalize_advantage=False,
            ),
        )
        unclipped_value = replace(
            value,
            policy_update=replace(
                value.policy_update,
                value_clip_coefficient=None,
            ),
        )
        calibration = replace(
            value,
            policy_update=RunPolicyUpdateConfig.critic_calibration(),
        )
        terminal_score = replace(value, policy_update=replace(
            value.policy_update, value_prediction_target="terminal_score"))

        policy_template = categorical_training_manifest_template(
            schema,
            RaggedScorerConfig(hidden_dim=4, relation_layers=1),
            behavior,
            optimizer,
            reinforce,
            device_type="cpu",
        )
        value_template = categorical_training_manifest_template(
            schema,
            RaggedScorerConfig(
                hidden_dim=4,
                relation_layers=1,
                value_head=True,
            ),
            behavior,
            optimizer,
            value,
            device_type="cpu",
        )
        multi_value_template = categorical_training_manifest_template(
            schema,
            RaggedScorerConfig(
                hidden_dim=4,
                relation_layers=1,
                value_head=True,
                value_head_width=3,
            ),
            behavior,
            optimizer,
            value,
            device_type="cpu",
        )
        unnormalized_value_template = categorical_training_manifest_template(
            schema,
            RaggedScorerConfig(
                hidden_dim=4,
                relation_layers=1,
                value_head=True,
            ),
            behavior,
            optimizer,
            unnormalized_value,
            device_type="cpu",
        )
        unclipped_value_template = categorical_training_manifest_template(
            schema,
            RaggedScorerConfig(
                hidden_dim=4,
                relation_layers=1,
                value_head=True,
            ),
            behavior,
            optimizer,
            unclipped_value,
            device_type="cpu",
        )
        calibration_template = categorical_training_manifest_template(
            schema,
            RaggedScorerConfig(
                hidden_dim=4,
                relation_layers=1,
                value_head=True,
            ),
            behavior,
            optimizer,
            calibration,
            device_type="cpu",
        )
        terminal_score_template = categorical_training_manifest_template(
            schema, RaggedScorerConfig(hidden_dim=4, relation_layers=1, value_head=True),
            behavior, optimizer, terminal_score, device_type="cpu")
        self.assertEqual(terminal_score_template.model_config, value_template.model_config)
        self.assertNotEqual(terminal_score_template.trainer_implementation,
                            value_template.trainer_implementation)

        self.assertNotEqual(
            value_template.model_definition,
            policy_template.model_definition,
        )
        self.assertNotEqual(
            value_template.model_config,
            policy_template.model_config,
        )
        self.assertNotEqual(
            multi_value_template.model_definition,
            value_template.model_definition,
        )
        self.assertNotEqual(
            multi_value_template.model_config,
            value_template.model_config,
        )
        self.assertNotEqual(
            value_template.trainer_implementation,
            policy_template.trainer_implementation,
        )
        self.assertNotEqual(
            unnormalized_value_template.trainer_implementation,
            value_template.trainer_implementation,
        )
        self.assertNotEqual(
            unclipped_value_template.trainer_implementation,
            value_template.trainer_implementation,
        )
        self.assertEqual(
            calibration_template.model_definition,
            value_template.model_definition,
        )
        self.assertEqual(
            calibration_template.model_config,
            value_template.model_config,
        )
        self.assertNotEqual(
            calibration_template.trainer_implementation,
            value_template.trainer_implementation,
        )

    def test_template_is_canonical_and_changes_with_runtime_profile(self) -> None:
        schema = semantic_schema_fixture()
        reversed_schema = {
            key: (
                dict(reversed(tuple(value.items())))
                if isinstance(value, dict)
                else value
            )
            for key, value in reversed(tuple(schema.items()))
        }
        scorer = RaggedScorerConfig(hidden_dim=4, relation_layers=0)
        behavior = RaggedCategoricalPolicyConfig(temperature=0.8)
        optimizer = AdamTrainingConfig(learning_rate=0.002)
        terminal_return = FloorProgressReturnConfig(target_floor=52)
        objective = OnPolicyObjectiveConfig(
            terminal_return=terminal_return,
            attempts_per_update=8,
        )

        template = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            objective,
            device_type="cpu",
        )
        reordered = categorical_training_manifest_template(
            reversed_schema,
            scorer,
            behavior,
            optimizer,
            objective,
            device_type="cpu",
        )
        changed_model = categorical_training_manifest_template(
            schema,
            replace(scorer, hidden_dim=8),
            behavior,
            optimizer,
            objective,
            device_type="cpu",
        )
        changed_optimizer = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            replace(optimizer, learning_rate=0.003),
            objective,
            device_type="cpu",
        )
        changed_return = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            replace(
                objective,
                terminal_return=replace(terminal_return, target_floor=51),
            ),
            device_type="cpu",
        )
        changed_attempt_batch = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            replace(objective, attempts_per_update=4),
            device_type="cpu",
        )
        changed_advantage = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            replace(
                objective,
                advantage_mode=TerminalAdvantageMode.LEAVE_ONE_OUT,
            ),
            device_type="cpu",
        )
        changed_context_advantage = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            replace(
                objective,
                advantage_mode=(
                    TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT
                ),
            ),
            device_type="cpu",
        )
        changed_scope = categorical_training_manifest_template(
            schema,
            scorer,
            behavior,
            optimizer,
            replace(
                objective,
                decision_scope=RunDecisionScope.STRATEGIC,
            ),
            device_type="cpu",
        )

        self.assertEqual(reordered, template)
        self.assertNotEqual(changed_model.model_config, template.model_config)
        self.assertNotEqual(
            changed_optimizer.optimizer_config,
            template.optimizer_config,
        )
        self.assertNotEqual(
            changed_return.trainer_implementation,
            template.trainer_implementation,
        )
        self.assertNotEqual(
            changed_attempt_batch.trainer_implementation,
            template.trainer_implementation,
        )
        self.assertNotEqual(
            changed_advantage.trainer_implementation,
            template.trainer_implementation,
        )
        self.assertNotEqual(
            changed_context_advantage.trainer_implementation,
            changed_advantage.trainer_implementation,
        )
        self.assertNotEqual(
            changed_scope.trainer_implementation,
            template.trainer_implementation,
        )

    def test_adam_factory_matches_the_provenance_configuration(self) -> None:
        config = AdamTrainingConfig(
            learning_rate=0.002,
            beta1=0.8,
            beta2=0.95,
            epsilon=1e-7,
            weight_decay=0.01,
            amsgrad=True,
        )
        parameter = torch.nn.Parameter(torch.tensor([1.0]))
        optimizer = config.create([parameter])
        group = optimizer.param_groups[0]

        self.assertEqual(group["lr"], config.learning_rate)
        self.assertEqual(group["betas"], (config.beta1, config.beta2))
        self.assertEqual(group["eps"], config.epsilon)
        self.assertEqual(group["weight_decay"], config.weight_decay)
        self.assertTrue(group["amsgrad"])
        self.assertFalse(group["foreach"])
        self.assertFalse(group["fused"])

    def test_unsupported_schema_values_fail_closed(self) -> None:
        schema = semantic_schema_fixture()
        schema["foreign"] = [1, 2, 3]

        with self.assertRaisesRegex(TorchProvenanceError, "unsupported list"):
            categorical_training_manifest_template(
                schema,
                RaggedScorerConfig(hidden_dim=4, relation_layers=0),
                RaggedCategoricalPolicyConfig(temperature=0.8),
                AdamTrainingConfig(),
                OnPolicyObjectiveConfig(),
                device_type="cpu",
            )


if __name__ == "__main__":
    unittest.main()
