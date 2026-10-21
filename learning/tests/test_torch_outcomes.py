from __future__ import annotations

from dataclasses import replace
import importlib.util
import math
import unittest

from learning.tests.torch_outcome_fixtures import (
    behavior_manifest_fixture,
    completed_attempt_fixture,
    decision_batch_fixture,
    public_attempt_trajectory_fixture,
    with_run_progress_fixture,
)
from sts_learning import (
    BehaviorManifestRegistry,
    FloorProgressReturnConfig,
    RunDecisionScope,
    RunPolicyUpdateConfig,
    SelectionProbability,
    SemanticBatchConcatLimits,
    TerminalAdvantageMode,
    floor_progress_terminal_return,
    terminal_return_advantages,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
    from sts_learning.torch_outcomes import (
        TorchOutcomeError,
        _selected_log_probabilities,
        _require_matching_propensities,
        on_policy_terminal_loss,
    )
    from sts_learning.torch_policy import (
        RaggedCandidateLogits,
        RaggedCandidateScorer,
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )


CONCAT_LIMITS = SemanticBatchConcatLimits(
    max_rows=16,
    max_input_array_bytes=1024 * 1024,
)


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class OnPolicyTerminalLossTests(unittest.TestCase):
    def test_canonical_behavior_scores_match_standalone_rows_and_reject_wrong_probabilities(self):
        from sts_learning.torch_policy import score_categorical_behavior
        from sts_learning.semantic_batch import select_semantic_decision_rows
        torch.manual_seed(47)
        scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture())
        config = RaggedCategoricalPolicyConfig(canonical_row_scoring=True)
        batch = semantic_batch_fixture()
        logits = score_categorical_behavior(scorer, batch, config)
        records = []
        splits = logits.row_splits.tolist()
        for row, (start, end) in enumerate(zip(splits[:-1], splits[1:], strict=True)):
            solo = scorer(select_semantic_decision_rows(batch, (row,)))
            self.assertTrue(torch.equal(logits.values[start:end], solo.values))
            records.append(SelectionProbability.known(float(solo.values.detach().double().softmax(0)[0])))
        ordinals = (0,) * logits.row_count
        _require_matching_propensities(scorer(batch), ordinals, records, config,
                                      scorer=scorer, decision_batch=batch)
        changed = (SelectionProbability.known(records[0].value * (1 + 4e-6)), *records[1:])
        with self.assertRaisesRegex(TorchOutcomeError, "off-policy"):
            _require_matching_propensities(logits, ordinals, changed, config,
                                          scorer=scorer, decision_batch=batch)
        self.assertNotEqual(config.behavior_rule, RaggedCategoricalPolicyConfig().behavior_rule)

    def test_terminal_score_zero_head_does_not_penalize_later_decisions(self):
        torch.manual_seed(45)
        scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=10, relation_layers=1, value_head=True))
        batches = tuple(self._with_run_progress(
            self._on_policy_batch(scorer, slot=1, row=0, ordinal=0), floor=floor)
            for floor in (0, 8, 19, 26))
        attempt = completed_attempt_fixture(slot=1, batches=batches, reward=-1)
        update = replace(RunPolicyUpdateConfig.ppo_clip_value(),
            value_prediction_target="terminal_score", entropy_coefficient=0.0)
        result = on_policy_terminal_loss(scorer, self._public(attempt), self.registry,
            CONCAT_LIMITS, self.config, self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE, update_config=update)
        # Every action in this one outcome has the same final score. Global
        # normalization therefore supplies no spurious within-run preference.
        self.assertEqual(result.actor_advantages, (0.0,) * 4)
        self.assertAlmostEqual(result.critic_predictions[2], -2 * 19 / self.return_config.target_floor)
        result.value.backward()
        self.assertTrue(all(parameter.grad is None or torch.count_nonzero(parameter.grad) == 0
            for name, parameter in scorer.named_parameters() if not name.startswith("value_head.")))
        self.assertGreater(torch.count_nonzero(scorer.value_head[-1].bias.grad), 0)

        raw = on_policy_terminal_loss(scorer, self._public(attempt), self.registry,
            CONCAT_LIMITS, self.config, self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE,
            update_config=replace(update, normalize_advantage=False))
        expected = floor_progress_terminal_return(attempt.terminal, self.return_config)
        for advantage in raw.actor_advantages:
            self.assertAlmostEqual(advantage, expected)
        shorter = completed_attempt_fixture(slot=2, batches=(
            self._with_run_progress(self._on_policy_batch(scorer, slot=2, row=0, ordinal=0), floor=26),),
            reward=-1)
        shorter = replace(shorter, terminal=replace(shorter.terminal,
            terminal=replace(shorter.terminal.terminal,
                terminal_floor=attempt.terminal.terminal.terminal_floor)))
        mixed_lengths = on_policy_terminal_loss(scorer, self._public(attempt, shorter), self.registry,
            CONCAT_LIMITS, self.config, self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE, update_config=update)
        self.assertEqual(mixed_lengths.actor_advantages, (0.0,) * 5)

    def test_terminal_score_value_translation_retains_frozen_epoch_contract(self):
        torch.manual_seed(46)
        scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=10, relation_layers=1, value_head=True))
        attempts = self._public(completed_attempt_fixture(slot=1, batches=(
            self._with_run_progress(self._on_policy_batch(scorer, slot=1, row=0, ordinal=0), floor=7),
            self._with_run_progress(self._on_policy_batch(scorer, slot=1, row=1, ordinal=1), floor=19)), reward=1))
        update = replace(RunPolicyUpdateConfig.ppo_clip_value(), value_prediction_target="terminal_score")
        def objective(**kw):
            return on_policy_terminal_loss(scorer, attempts, self.registry, CONCAT_LIMITS,
                self.config, self.return_config, TerminalAdvantageMode.DECISION_LOCAL_GAE,
                update_config=update, **kw)
        initial = objective()
        with torch.no_grad(): scorer.value_head[-1].bias.add_(0.4)
        # Actor probabilities still match; a changed value head cannot bypass
        # the original frozen value clip or recalculate advantages mid-epoch.
        next_epoch = objective(require_matching_propensities=False,
            fixed_actor_advantages=initial.actor_advantages,
            fixed_value_predictions=initial.critic_predictions)
        self.assertEqual(next_epoch.actor_advantages, initial.actor_advantages)
        self.assertAlmostEqual(next_epoch.value_clip_fraction, 1.0)
        next_epoch.value.backward()
        self.assertTrue(torch.isfinite(scorer.value_head[-1].bias.grad).all())

    def test_small_batch_rounding_requires_strict_single_row_propensity_replay(self):
        torch.manual_seed(8)
        scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture())
        batch = semantic_batch_fixture()
        logits = scorer(batch)
        splits = logits.row_splits.tolist()
        ordinals = tuple(0 for _ in range(logits.row_count))
        recorded = tuple(SelectionProbability.known(float(
            (logits.values[start:end].detach().double()).softmax(0)[0]))
            for start, end in zip(splits[:-1], splits[1:]))
        config = RaggedCategoricalPolicyConfig()
        changed = logits.values.detach().clone()
        changed[0] += 8e-6
        rounded = RaggedCandidateLogits(changed, logits.row_splits)
        with self.assertRaisesRegex(TorchOutcomeError, "off-policy"):
            _require_matching_propensities(rounded, ordinals, recorded, config)
        _require_matching_propensities(rounded, ordinals, recorded, config,
                                      scorer=scorer, decision_batch=batch)
        # A similar small change in the recorded behavior is not rounding:
        # exact-row replay must still match it at the original tolerance.
        wrong = (SelectionProbability.known(recorded[0].value*(1+4e-6)), *recorded[1:])
        with self.assertRaisesRegex(TorchOutcomeError, "off-policy"):
            _require_matching_propensities(logits, ordinals, wrong, config,
                                          scorer=scorer, decision_batch=batch)
        changed[0] += 0.01
        with self.assertRaisesRegex(TorchOutcomeError, "off-policy"):
            _require_matching_propensities(RaggedCandidateLogits(changed, logits.row_splits),
                ordinals, recorded, config, scorer=scorer, decision_batch=batch)

    def setUp(self) -> None:
        self.config = RaggedCategoricalPolicyConfig(temperature=0.8)
        self.return_config = FloorProgressReturnConfig(target_floor=100)
        self.registry = BehaviorManifestRegistry(capacity=1)
        self.manifest_id = self.registry.register(
            behavior_manifest_fixture(behavior_rule=self.config.behavior_rule)
        )

    def test_single_forward_loss_and_gradients_equal_per_attempt_reference(self) -> None:
        torch.manual_seed(42)
        reference = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=10, relation_layers=1),
        )
        vectorized = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=10, relation_layers=1),
        )
        vectorized.load_state_dict(reference.state_dict())
        short = completed_attempt_fixture(
            slot=1,
            batches=(
                self._on_policy_batch(reference, slot=1, row=0, ordinal=1),
            ),
            reward=1,
        )
        long = completed_attempt_fixture(
            slot=2,
            batches=(
                self._on_policy_batch(reference, slot=2, row=1, ordinal=2),
                self._on_policy_batch(reference, slot=2, row=0, ordinal=0),
            ),
            reward=-1,
        )

        attempts = (short, long)
        advantages = terminal_return_advantages(
            tuple(
                floor_progress_terminal_return(attempt.terminal, self.return_config)
                for attempt in attempts
            ),
            TerminalAdvantageMode.LEAVE_ONE_OUT,
        )
        reference_attempt_losses = []
        for attempt, advantage in zip(attempts, advantages, strict=True):
            terms = []
            for batch in attempt.batches:
                logits = reference(batch.payload)
                selected = batch.selected_ordinals[0]
                log_probability = torch.log_softmax(
                    logits.values / self.config.temperature,
                    dim=0,
                )[selected]
                terms.append(-advantage * log_probability)
            reference_attempt_losses.append(torch.stack(terms).sum())
        reference_loss = torch.stack(reference_attempt_losses).mean()
        reference_loss.backward()

        calls = 0

        def counted_scorer(payload):
            nonlocal calls
            calls += 1
            return vectorized(payload)

        result = on_policy_terminal_loss(
            counted_scorer,
            self._public(short, long),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.LEAVE_ONE_OUT,
        )
        result.value.backward()

        self.assertEqual(calls, 1)
        torch.testing.assert_close(result.value, reference_loss)
        for actual, expected in zip(
            vectorized.parameters(),
            reference.parameters(),
            strict=True,
        ):
            self.assertIsNotNone(actual.grad)
            self.assertIsNotNone(expected.grad)
            torch.testing.assert_close(actual.grad, expected.grad)

    def test_positive_and_negative_returns_move_probability_oppositely(self) -> None:
        values = torch.nn.Parameter(torch.zeros(8))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        short = completed_attempt_fixture(
            slot=1,
            batches=(self._fixed_probability_batch(slot=1),),
            reward=1,
        )
        long = completed_attempt_fixture(
            slot=2,
            batches=tuple(self._fixed_probability_batch(slot=2) for _ in range(3)),
            reward=-1,
        )

        result = on_policy_terminal_loss(
            scorer,
            self._public(short, long),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.RAW_RETURN,
        )
        result.value.backward()

        self.assertAlmostEqual(
            float(result.value.detach()),
            0.2 * math.log(2.0),
            places=6,
        )
        self.assertEqual(result.attempt_count, 2)
        self.assertEqual(result.decision_count, 4)
        self.assertAlmostEqual(values.grad[0].item(), -0.3125, places=6)
        self.assertAlmostEqual(values.grad[1].item(), 0.3125, places=6)
        for start in (2, 4, 6):
            self.assertAlmostEqual(values.grad[start].item(), 1.0 / 16.0, places=6)
            self.assertAlmostEqual(values.grad[start + 1].item(), -1.0 / 16.0, places=6)

    def test_matched_floor_objective_compares_only_attempts_at_that_floor(self) -> None:
        values = torch.nn.Parameter(torch.zeros(4))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        attempts = []
        for slot, terminal_floor, ordinal in ((1, 10, 0), (2, 20, 1)):
            batch = with_run_progress_fixture(
                decision_batch_fixture(
                    slot=slot,
                    semantic_row=0,
                    selected_ordinal=ordinal,
                    manifest_id=self.manifest_id,
                    selection_probability=SelectionProbability.known(0.5),
                ),
                act=1,
                floor=0,
                is_combat=True,
                strategic_context_kind=None,
            )
            attempt = completed_attempt_fixture(
                slot=slot,
                batches=(batch,),
                reward=-1,
            )
            attempts.append(
                replace(
                    attempt,
                    terminal=replace(
                        attempt.terminal,
                        terminal=replace(
                            attempt.terminal.terminal,
                            terminal_floor=terminal_floor,
                        ),
                    ),
                )
            )

        result = on_policy_terminal_loss(
            scorer,
            self._public(*attempts),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.MATCHED_FLOOR_LEAVE_ONE_OUT,
        )
        result.value.backward()

        self.assertAlmostEqual(float(result.value.detach()), 0.0, places=6)
        self.assertGreater(values.grad[0].item(), 0.0)
        self.assertLess(values.grad[3].item(), 0.0)

    def test_strategic_scope_excludes_combat_rows_and_renormalizes_attempt(self) -> None:
        values = torch.nn.Parameter(torch.zeros(2))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        batches = tuple(
            with_run_progress_fixture(
                self._fixed_probability_batch(slot=1),
                act=1,
                floor=1,
                is_combat=is_combat,
                strategic_context_kind=None if is_combat else 1,
                identity_suffix=f"scope-{int(is_combat)}",
            )
            for is_combat in (True, False)
        )
        attempt = completed_attempt_fixture(
            slot=1,
            batches=batches,
            reward=1,
        )

        result = on_policy_terminal_loss(
            scorer,
            self._public(attempt),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.RAW_RETURN,
            RunDecisionScope.STRATEGIC,
        )
        result.value.backward()

        self.assertEqual(result.decision_count, 1)
        self.assertAlmostEqual(float(result.value.detach()), math.log(2.0), places=6)
        self.assertLess(values.grad[0].item(), 0.0)
        self.assertGreater(values.grad[1].item(), 0.0)

    def test_floor_progress_reserves_the_unique_maximum_for_victory(self) -> None:
        config = FloorProgressReturnConfig(target_floor=52)
        defeat = completed_attempt_fixture(
            slot=1,
            batches=(self._fixed_probability_batch(slot=1),),
            reward=-1,
        ).terminal

        for floor, expected in (
            (0, -1.0),
            (26, 0.0),
            (51, 1.0 - 2.0 / 52.0),
            (999, 1.0 - 2.0 / 52.0),
        ):
            with self.subTest(floor=floor):
                record = replace(
                    defeat,
                    terminal=replace(defeat.terminal, terminal_floor=floor),
                )
                self.assertAlmostEqual(
                    floor_progress_terminal_return(record, config),
                    expected,
                )

        victory = completed_attempt_fixture(
            slot=1,
            batches=(self._fixed_probability_batch(slot=1),),
            reward=1,
        ).terminal
        victory = replace(
            victory,
            terminal=replace(victory.terminal, terminal_floor=0),
        )
        self.assertEqual(floor_progress_terminal_return(victory, config), 1.0)

    def test_forced_candidate_has_exactly_zero_policy_gradient(self) -> None:
        values = torch.nn.Parameter(torch.tensor([2.0, 3.0, 4.0]))
        selected = _selected_log_probabilities(
            RaggedCandidateLogits(
                values=values,
                row_splits=torch.tensor([0, 1, 3], dtype=torch.long),
            ),
            (0, 1),
            self.config,
        )

        selected.sum().backward()

        self.assertEqual(values.grad[0].item(), 0.0)
        self.assertNotEqual(values.grad[1].item(), 0.0)
        self.assertNotEqual(values.grad[2].item(), 0.0)

    def test_forced_only_value_ppo_batch_still_trains_the_critic(self) -> None:
        torch.manual_seed(42)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(
                hidden_dim=10,
                relation_layers=1,
                value_head=True,
            ),
        )
        batch = decision_batch_fixture(
            slot=1,
            semantic_row=0,
            selected_ordinal=0,
            manifest_id=self.manifest_id,
            selection_probability=SelectionProbability.known(1.0),
        )
        payload = dict(batch.payload)
        candidate_counts = payload["candidate_counts"].copy()
        candidate_counts[0] = 1
        candidate_splits = payload["candidate_row_splits"].copy()
        candidate_splits[1] = 1
        semantic = dict(payload["semantic"])
        semantic["candidate_token_indices"] = semantic[
            "candidate_token_indices"
        ][:1].copy()
        tokens = dict(semantic["token"])
        scopes = tokens["scope"].copy()
        scopes[scopes > 1] = 0
        tokens["scope"] = scopes
        semantic["token"] = tokens
        payload["candidate_counts"] = candidate_counts
        payload["candidate_row_splits"] = candidate_splits
        payload["semantic"] = semantic
        batch = self._with_run_progress(replace(batch, payload=payload), floor=0)
        attempt = completed_attempt_fixture(
            slot=1,
            batches=(batch,),
            reward=1,
        )

        result = on_policy_terminal_loss(
            scorer,
            self._public(attempt),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE,
            update_config=RunPolicyUpdateConfig.ppo_clip_value(),
        )
        result.value.backward()

        diagnostics = result.value_diagnostics
        assert diagnostics is not None
        assert scorer.value_head is not None
        self.assertEqual(result.actor_decision_count, 0)
        self.assertEqual(result.actor_advantages, (0.0,))
        self.assertEqual(result.approximate_kl, 0.0)
        self.assertEqual(result.clip_fraction, 0.0)
        self.assertEqual(result.entropy, 0.0)
        self.assertIsNone(diagnostics.pre_normalization_actor_advantage)
        self.assertIsNone(diagnostics.actor_advantage)
        self.assertFalse(diagnostics.advantage_normalization_applied)
        self.assertEqual(diagnostics.advantage_normalization_sign_changes, 0)
        self.assertEqual(diagnostics.actor_decision_count, 0)
        self.assertEqual(diagnostics.forced_decision_count, 1)
        self.assertTrue(
            any(
                parameter.grad is not None
                and bool(torch.any(parameter.grad != 0))
                for parameter in scorer.value_head.parameters()
            )
        )

    def test_run_value_ppo_trains_local_value_and_freezes_rollout_columns(
        self,
    ) -> None:
        torch.manual_seed(43)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(
                hidden_dim=10,
                relation_layers=1,
                value_head=True,
            ),
        )
        attempts = (
            completed_attempt_fixture(
                slot=1,
                batches=(
                    self._with_run_progress(
                        self._on_policy_batch(
                            scorer,
                            slot=1,
                            row=0,
                            ordinal=1,
                        ),
                        floor=0,
                    ),
                ),
                reward=1,
            ),
            completed_attempt_fixture(
                slot=2,
                batches=(
                    self._with_run_progress(
                        self._on_policy_batch(
                            scorer,
                            slot=2,
                            row=1,
                            ordinal=2,
                        ),
                        floor=0,
                    ),
                ),
                reward=-1,
            ),
        )
        update = RunPolicyUpdateConfig.ppo_clip_value()

        first = on_policy_terminal_loss(
            scorer,
            self._public(*attempts),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE,
            update_config=update,
        )
        frozen = first.actor_advantages
        frozen_values = first.critic_predictions
        assert frozen_values is not None
        self.assertAlmostEqual(frozen[0], 1.0)
        self.assertAlmostEqual(frozen[1], -1.0)
        self.assertGreater(first.value_loss, 0.0)
        self.assertEqual(first.approximate_kl, 0.0)
        optimizer = torch.optim.SGD(scorer.parameters(), lr=0.01)
        optimizer.zero_grad(set_to_none=True)
        first.value.backward()
        assert scorer.value_head is not None
        self.assertTrue(
            any(
                parameter.grad is not None
                and bool(torch.any(parameter.grad != 0))
                for parameter in scorer.value_head.parameters()
            )
        )
        optimizer.step()
        with torch.no_grad():
            scorer.value_head[-1].bias.add_(1.0)

        second = on_policy_terminal_loss(
            scorer,
            self._public(*attempts),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE,
            update_config=update,
            require_matching_propensities=False,
            fixed_actor_advantages=frozen,
            fixed_value_predictions=frozen_values,
        )

        self.assertEqual(second.actor_advantages, frozen)
        self.assertGreaterEqual(second.approximate_kl, 0.0)
        self.assertGreater(second.value_clip_fraction, 0.0)
        self.assertIsNone(second.value_diagnostics)

    def test_run_diagnostics_use_actor_row_weights_and_attempt_mean_value_weights(
        self,
    ) -> None:
        torch.manual_seed(44)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(
                hidden_dim=10,
                relation_layers=1,
                value_head=True,
            ),
        )
        short = completed_attempt_fixture(
            slot=1,
            batches=(
                self._with_run_progress(
                    self._on_policy_batch(scorer, slot=1, row=0, ordinal=1),
                    floor=0,
                ),
            ),
            reward=1,
        )
        long = completed_attempt_fixture(
            slot=2,
            batches=(
                self._with_run_progress(
                    self._on_policy_batch(scorer, slot=2, row=1, ordinal=2),
                    floor=0,
                ),
                self._with_run_progress(
                    self._on_policy_batch(scorer, slot=2, row=0, ordinal=0),
                    floor=10,
                ),
                self._with_run_progress(
                    self._on_policy_batch(scorer, slot=2, row=1, ordinal=1),
                    floor=20,
                ),
            ),
            reward=-1,
        )

        result = on_policy_terminal_loss(
            scorer,
            self._public(short, long),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE,
            update_config=RunPolicyUpdateConfig.ppo_clip_value(),
        )

        diagnostics = result.value_diagnostics
        assert diagnostics is not None
        pre_normalization = diagnostics.pre_normalization_actor_advantage
        assert pre_normalization is not None
        advantage = diagnostics.actor_advantage
        assert advantage is not None
        self.assertEqual(pre_normalization.decision_count, 4)
        self.assertEqual(pre_normalization.positive_decisions, 1)
        self.assertEqual(pre_normalization.negative_decisions, 3)
        self.assertAlmostEqual(pre_normalization.weighted_mean, -0.05)
        self.assertAlmostEqual(pre_normalization.minimum, -0.6)
        self.assertAlmostEqual(pre_normalization.maximum, 1.0)
        self.assertTrue(diagnostics.advantage_normalization_applied)
        self.assertEqual(diagnostics.advantage_normalization_sign_changes, 0)
        self.assertEqual(
            diagnostics.advantage_normalization_positive_from_nonpositive,
            0,
        )
        self.assertEqual(
            diagnostics.advantage_normalization_negative_from_nonnegative,
            0,
        )
        self.assertEqual(advantage.decision_count, 4)
        self.assertEqual(advantage.positive_decisions, 1)
        self.assertEqual(advantage.negative_decisions, 3)
        self.assertAlmostEqual(advantage.positive_weight, 0.25)
        self.assertAlmostEqual(advantage.negative_weight, 0.75)
        self.assertAlmostEqual(advantage.weighted_mean, 0.0)
        self.assertAlmostEqual(advantage.weighted_standard_deviation, 1.0, places=6)
        self.assertAlmostEqual(diagnostics.critic_prediction.weighted_mean, 0.0)
        self.assertAlmostEqual(diagnostics.return_to_go_target.weighted_mean, 0.3)
        self.assertAlmostEqual(diagnostics.critic_residual.weighted_mean, 0.3)
        self.assertEqual(diagnostics.actor_decision_count, 4)
        self.assertEqual(diagnostics.forced_decision_count, 0)
        self.assertAlmostEqual(diagnostics.explained_variance, 0.0)

    def test_value_diagnostics_identify_normalization_created_direction(self) -> None:
        torch.manual_seed(45)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(
                hidden_dim=10,
                relation_layers=1,
                value_head=True,
            ),
        )
        attempts = []
        for slot, terminal_floor in ((1, 10), (2, 20)):
            attempt = completed_attempt_fixture(
                slot=slot,
                batches=(
                    self._with_run_progress(
                        self._on_policy_batch(
                            scorer,
                            slot=slot,
                            row=0,
                            ordinal=0,
                        ),
                        floor=0,
                    ),
                ),
                reward=-1,
            )
            attempts.append(
                replace(
                    attempt,
                    terminal=replace(
                        attempt.terminal,
                        terminal=replace(
                            attempt.terminal.terminal,
                            terminal_floor=terminal_floor,
                        ),
                    ),
                )
            )

        result = on_policy_terminal_loss(
            scorer,
            self._public(*attempts),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE,
            update_config=RunPolicyUpdateConfig.ppo_clip_value(),
        )

        diagnostics = result.value_diagnostics
        assert diagnostics is not None
        raw = diagnostics.pre_normalization_actor_advantage
        normalized = diagnostics.actor_advantage
        assert raw is not None
        assert normalized is not None
        self.assertEqual(raw.negative_decisions, 2)
        self.assertEqual(raw.positive_decisions, 0)
        self.assertEqual(normalized.negative_decisions, 1)
        self.assertEqual(normalized.positive_decisions, 1)
        self.assertTrue(diagnostics.advantage_normalization_applied)
        self.assertEqual(diagnostics.advantage_normalization_sign_changes, 1)
        self.assertEqual(
            diagnostics.advantage_normalization_positive_from_nonpositive,
            1,
        )
        self.assertEqual(
            diagnostics.advantage_normalization_negative_from_nonnegative,
            0,
        )

    def test_value_ppo_optimizes_decision_local_returns(
        self,
    ) -> None:
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(
                hidden_dim=10,
                relation_layers=1,
                value_head=True,
            ),
        )
        batches = tuple(
            with_run_progress_fixture(
                self._on_policy_batch(
                    scorer,
                    slot=1,
                    row=index,
                    ordinal=0,
                ),
                act=1,
                floor=floor,
                is_combat=True,
                strategic_context_kind=None,
                identity_suffix=f"value-{index}",
            )
            for index, floor in ((0, 0), (1, 10))
        )
        attempt = completed_attempt_fixture(
            slot=1,
            batches=batches,
            reward=-1,
        )

        result = on_policy_terminal_loss(
            scorer,
            self._public(attempt),
            self.registry,
            CONCAT_LIMITS,
            self.config,
            self.return_config,
            TerminalAdvantageMode.DECISION_LOCAL_GAE,
            update_config=RunPolicyUpdateConfig.ppo_clip_value(),
        )

        diagnostics = result.value_diagnostics
        assert diagnostics is not None
        self.assertAlmostEqual(diagnostics.return_to_go_target.minimum, -0.4)
        self.assertAlmostEqual(diagnostics.return_to_go_target.maximum, -0.2)
        self.assertAlmostEqual(diagnostics.critic_residual.minimum, -0.4)
        self.assertAlmostEqual(diagnostics.critic_residual.maximum, -0.2)

    def test_unknown_or_mismatched_propensity_is_rejected(self) -> None:
        values = torch.nn.Parameter(torch.zeros(2))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=values,
                row_splits=torch.as_tensor(
                    payload["candidate_row_splits"],
                    dtype=torch.long,
                ),
            )

        base = self._with_run_progress(
            decision_batch_fixture(
                slot=1,
                semantic_row=0,
                selected_ordinal=0,
                manifest_id=self.manifest_id,
            ),
            floor=0,
        )
        for evidence in (
            SelectionProbability.unknown(),
            SelectionProbability.known(0.2),
        ):
            attempt = completed_attempt_fixture(
                slot=1,
                batches=(replace(base, selection_probabilities=(evidence,)),),
                reward=1,
            )
            with self.assertRaisesRegex(
                TorchOutcomeError,
                "known selection|off-policy",
            ):
                on_policy_terminal_loss(
                    scorer,
                    self._public(attempt),
                    self.registry,
                    CONCAT_LIMITS,
                    self.config,
                    self.return_config,
                    TerminalAdvantageMode.RAW_RETURN,
                )

    def test_unknown_manifest_and_non_complete_input_fail_before_training(self) -> None:
        unregistered = behavior_manifest_fixture(
            behavior_rule=self.config.behavior_rule
        ).identity
        batch = self._with_run_progress(
            decision_batch_fixture(
                slot=1,
                semantic_row=0,
                selected_ordinal=0,
                manifest_id=unregistered,
                selection_probability=SelectionProbability.known(0.5),
            ),
            floor=0,
        )

        with self.assertRaisesRegex(TorchOutcomeError, "unknown behavior"):
            on_policy_terminal_loss(
                lambda payload: RaggedCandidateLogits(
                    values=torch.zeros(2),
                    row_splits=torch.tensor([0, 2]),
                ),
                self._public(
                    completed_attempt_fixture(slot=1, batches=(batch,), reward=1)
                ),
                BehaviorManifestRegistry(capacity=1),
                CONCAT_LIMITS,
                self.config,
                self.return_config,
                TerminalAdvantageMode.RAW_RETURN,
            )
        with self.assertRaisesRegex(TorchOutcomeError, "at least one"):
            on_policy_terminal_loss(
                lambda payload: None,
                (),
                self.registry,
                CONCAT_LIMITS,
                self.config,
                self.return_config,
                TerminalAdvantageMode.RAW_RETURN,
            )
        with self.assertRaisesRegex(TorchOutcomeError, "only public"):
            on_policy_terminal_loss(
                lambda payload: None,
                (object(),),  # type: ignore[arg-type]
                self.registry,
                CONCAT_LIMITS,
                self.config,
                self.return_config,
                TerminalAdvantageMode.RAW_RETURN,
            )

    def _on_policy_batch(self, scorer, *, slot: int, row: int, ordinal: int):
        batch = decision_batch_fixture(
            slot=slot,
            semantic_row=row,
            selected_ordinal=ordinal,
            manifest_id=self.manifest_id,
        )
        logits = scorer(batch.payload)
        probability = float(
            torch.softmax(
                logits.values.detach().to(dtype=torch.float64)
                / self.config.temperature,
                dim=0,
            )[ordinal].item()
        )
        return self._with_run_progress(
            replace(
                batch,
                selection_probabilities=(SelectionProbability.known(probability),),
            ),
            floor=0,
        )

    def _fixed_probability_batch(self, *, slot: int):
        return self._with_run_progress(
            decision_batch_fixture(
                slot=slot,
                semantic_row=0,
                selected_ordinal=0,
                manifest_id=self.manifest_id,
                selection_probability=SelectionProbability.known(0.5),
            ),
            floor=0,
        )

    def _with_run_progress(self, batch, *, floor: int):
        return with_run_progress_fixture(
            batch,
            act=1,
            floor=floor,
            is_combat=True,
            strategic_context_kind=None,
            identity_suffix=f"floor-{floor}",
        )

    def _public(self, *attempts):
        return tuple(public_attempt_trajectory_fixture(attempt) for attempt in attempts)


if __name__ == "__main__":
    unittest.main()
