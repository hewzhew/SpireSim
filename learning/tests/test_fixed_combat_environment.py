from __future__ import annotations

import importlib.util
import unittest
from dataclasses import replace
from unittest.mock import Mock

from sts_learning.fixed_combat import FixedCombatConfig
from sts_learning.fixed_combat_environment import FixedCombatBinding, FixedCombatEnvironment, InstalledFixedCombatRuntime
from sts_learning.driver import partition_routine_decisions


class BindingTests(unittest.TestCase):
    def test_failure_diagnostics_preserve_original_exception(self):
        native = Mock()
        native.fixed_combat_options_json.return_value = '{"search":{}}'
        config = FixedCombatConfig()
        binding = FixedCombatBinding.create(native, {}, "0"*64, config)
        evidence = Mock()
        env = FixedCombatEnvironment(native, config, binding, evidence_sink=evidence)
        original = RuntimeError("native failure")
        env._controller = Mock()
        env._controller.step.side_effect = original
        for diagnostic_error in (None, OSError("cannot write checkpoint")):
            evidence.failure.side_effect = diagnostic_error
            with self.assertRaises(RuntimeError) as caught:
                env.fixed_step()
            self.assertIs(caught.exception, original)
            evidence.failure.assert_called_with(env, original)
        self.assertIn("cannot write checkpoint", original.__notes__[0])
        evidence.assert_not_called()

    def test_binding_changes_for_budget_binary_and_schema(self):
        native = Mock()
        native.fixed_combat_options_json.return_value = '{"search":{}}'
        config = FixedCombatConfig()
        binding = FixedCombatBinding.create(native, {}, "0"*64, config)
        for changed in (
            FixedCombatBinding.create(native, {}, "0"*64, replace(config, wall_ms=1)),
            FixedCombatBinding.create(native, {}, "0"*64, replace(config, quantum_wall_ms=500)),
            replace(binding, native_sha256="1"*64),
            replace(binding, semantic_schema_sha256="2"*64),
        ):
            self.assertNotEqual(binding.identity, changed.identity)

    def test_mismatch_and_corruption_are_rejected_before_native_decode(self):
        native = Mock()
        native.fixed_combat_options_json.return_value = '{"search":{}}'
        native.checkpoint_bytes.return_value = b"opaque state"
        config = FixedCombatConfig()
        binding = FixedCombatBinding.create(native, {}, "0"*64, config)
        payload = FixedCombatEnvironment(native, config, binding).checkpoint_bytes(max_bytes=4096)
        for data, expected in ((payload, replace(binding, native_sha256="1"*64)),
                               (payload[:-1]+b"!", binding)):
            with self.assertRaises(ValueError):
                FixedCombatEnvironment.from_checkpoint_bytes(data, native_type=native,
                    config=config, binding=expected, expected_slots=1, max_bytes=4096)
        native.from_checkpoint_bytes.assert_not_called()


@unittest.skipUnless(importlib.util.find_spec("sts_learning_bridge"), "optional native bridge unavailable")
class RealBindingTests(unittest.TestCase):
    def test_unresolved_macro_preserves_pending_navigation(self):
        from sts_learning.fixed_combat import FixedCombatStep
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig(), strategic_navigation=True)
        env = runtime.environment([17], 0)
        batch = env.decision_batch(legality_bootstrap=True)
        env.choose(batch["decision_token_ids"], batch["legality_bootstrap_ordinals"].tolist())
        original = env.checkpoint_bytes(max_bytes=16_777_216)
        env._controller = Mock()
        env._controller.step.return_value = FixedCombatStep(None, {"combats": []}, 0)
        self.assertFalse(env.fixed_step().advanced)
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), original)

    def test_navigation_bank_reset_restore_and_ready_choice_are_exact(self):
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig(), strategic_navigation=True)
        env = runtime.environment([17, 19], 0)
        env._reward_memory._rooms = {0: (17, 1, 1), 1: (19, 1, 2)}
        env._reward_memory._skipped_rewards = {0: {3}, 1: {2}}
        env._reward_memory._reward_card_items = {0: [3], 1: [2]}
        original = env.checkpoint_slots([0, 1])
        encoded = original.checkpoint_bytes(max_bytes=16_777_216)
        bank = runtime.checkpoint_bank_from_checkpoint(encoded, expected_slot_indices=[0, 1], max_bytes=16_777_216)
        self.assertEqual(bank.checkpoint_bytes(max_bytes=16_777_216), encoded)
        with self.assertRaisesRegex(ValueError, "not terminal"):
            env.reset_slots_checkpointed([0], [23])
        self.assertEqual(env._reward_memory._skipped_rewards, {0: {3}, 1: {2}})
        # Replacement of an opaque slot bank also replaces its memory.
        env._reward_memory.clear_slots([0])
        replacement = env.checkpoint_slots([0])
        self.assertNotIn(0, env._reward_memory._skipped_rewards)
        updated = bank.updated(replacement)
        env.restore_slots([0, 1], updated)
        self.assertEqual(env._reward_memory._skipped_rewards, {1: {2}})
        env.restore_slots([0], bank.select([0]))
        self.assertEqual(env._reward_memory._skipped_rewards, {0: {3}, 1: {2}})
        # A real prepared root action and its pending collection update survive.
        batch = env.decision_batch(legality_bootstrap=True)
        env.choose(batch["decision_token_ids"], batch["legality_bootstrap_ordinals"].tolist())
        payload = env.checkpoint_bytes(max_bytes=16_777_216)
        restored = runtime.environment_from_checkpoint(payload, expected_slots=2, max_bytes=16_777_216)
        self.assertTrue(restored.ready)
        self.assertEqual(restored.checkpoint_bytes(max_bytes=16_777_216), payload)
        env.step()
        restored.step()
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), restored.checkpoint_bytes(max_bytes=16_777_216))
        with self.assertRaises(ValueError):
            InstalledFixedCombatRuntime(FixedCombatConfig()).environment_from_checkpoint(
                payload, expected_slots=2, max_bytes=16_777_216)

    def test_navigation_skips_a_closed_card_item_but_keeps_resource_tradeoffs(self):
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig(), strategic_navigation=True)
        env = runtime.environment([17], 0)
        env._reward_memory._skipped_rewards = {0: {2}}
        audit = {"decision_site": "reward", "candidates": [
            {"kind": "open_card_reward", "reward_item_index": 2},
            {"kind": "open_card_reward", "reward_item_index": 3}, {"kind": "proceed"}]}
        env._decision_audits = {"source": (0, audit)}
        batch = {"decision_token_ids": ["source"], "candidate_counts": [3]}
        self.assertEqual(env.routine_choice_ordinals(batch), {0: 1})
        env._reward_memory._skipped_rewards[0].add(3)
        self.assertEqual(env.routine_choice_ordinals(batch), {0: 2})
        for kind in ("claim_reward", "discard_run_potion"):
            audit["candidates"].append({"kind": kind})
            batch["candidate_counts"] = [len(audit["candidates"])]
            self.assertEqual(env.routine_choice_ordinals(batch), {})

    def test_routine_rows_do_not_reach_policy_or_its_semantic_payload(self):
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig(), strategic_navigation=True)
        env = runtime.environment([17, 19], 0)
        source = env.decision_batch(semantic=True)
        # Mixed ownership in a real native numerical batch, without changing legality.
        env.routine_choice_ordinals = lambda batch: {0: 0}
        learned, rows, routines = partition_routine_decisions(env, source)
        self.assertEqual(rows, (1,))
        self.assertEqual(learned["slot_indices"].tolist(), [1])
        self.assertEqual(learned["decision_token_ids"], [source["decision_token_ids"][1]])
        env.routine_choice_ordinals = lambda batch: {0: 0, 1: 0}
        self.assertIsNone(partition_routine_decisions(env, source)[0])

    def test_online_driver_records_only_the_actually_sampled_rows(self):
        from sts_learning.driver import initialize_population, OnlineBatchDriver
        from sts_learning.experience import ExperienceLimits, ExperienceSegmentBuffer
        from sts_learning.policy import BatchPolicyChoice, BehaviorManifestId, DETERMINISTIC_SELECTION
        from sts_learning.seeds import SeedSchedule, SeedPartition
        from sts_learning.torch_session import NoRecoveryCurriculum
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig(), strategic_navigation=True)
        population = initialize_population(lambda seeds: runtime.environment(seeds, 0),
            slot_count=2, schedule=SeedSchedule(SeedPartition.TRAINING), max_recoveries_per_episode=0)
        env = population.env
        env.routine_choice_ordinals = lambda batch: {
            row: 0 for row, slot in enumerate(batch["slot_indices"]) if int(slot) == 0}
        seen = []
        class Policy:
            def choose(self, batch):
                seen.extend(int(slot) for slot in batch["slot_indices"])
                return BatchPolicyChoice.create(tuple(0 for _ in batch["slot_indices"]),
                    BehaviorManifestId(bytes(32)), tuple(DETERMINISTIC_SELECTION for _ in batch["slot_indices"]))
        buffer = ExperienceSegmentBuffer(ExperienceLimits(max_decisions=64, max_payload_bytes=16*1024*1024))
        driver = OnlineBatchDriver(population, policy=Policy(), curriculum=NoRecoveryCurriculum(),
            experience_buffer=buffer, experience_sink=lambda segment: None)
        driver.advance()
        self.assertTrue(seen)
        self.assertEqual(set(seen), {1})
        self.assertEqual(buffer.decision_count, len(seen))
        segment = buffer.flush()
        self.assertIsNotNone(segment)
        self.assertEqual({lineage.key.slot_index for batch in segment.batches for lineage in batch.lineages}, {1})

    def test_ready_choice_survives_envelope_restore(self):
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig())
        env = runtime.environment([17], 20)
        batch = env.decision_batch(legality_bootstrap=True)
        env.choose(batch["decision_token_ids"], batch["legality_bootstrap_ordinals"].tolist())
        payload = env.checkpoint_bytes(max_bytes=16_777_216)
        restored = runtime.environment_from_checkpoint(payload, expected_slots=1, max_bytes=16_777_216)
        self.assertTrue(restored.ready)
        self.assertEqual(restored.checkpoint_bytes(max_bytes=16_777_216), payload)
        env.step()
        restored.step()
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), restored.checkpoint_bytes(max_bytes=16_777_216))

    def test_changed_options_cannot_restore_state(self):
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig())
        payload = runtime.environment([17],20).checkpoint_bytes(max_bytes=16_777_216)
        changed = InstalledFixedCombatRuntime(FixedCombatConfig(wall_ms=1))
        with self.assertRaisesRegex(ValueError, "binding mismatch"):
            changed.environment_from_checkpoint(payload, expected_slots=1, max_bytes=16_777_216)

    def test_real_combat_successor_restores_without_replaying_combat(self):
        from learning.tests.test_bridge_fixed_combat import first_combat_source, choose_bootstrap
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig())
        env = FixedCombatEnvironment(first_combat_source(), runtime.config, runtime.binding)
        env.step()
        payload = env.checkpoint_bytes(max_bytes=16_777_216)
        restored = runtime.environment_from_checkpoint(payload, expected_slots=1, max_bytes=16_777_216)
        self.assertEqual(restored.checkpoint_bytes(max_bytes=16_777_216), payload)
        choose_bootstrap(env)
        choose_bootstrap(restored)
        env.step()
        restored.step()
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), restored.checkpoint_bytes(max_bytes=16_777_216))
