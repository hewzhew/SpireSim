from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

from sts_learning.fixed_combat import FixedCombatConfig, FixedCombatStep
from sts_learning.fixed_combat_environment import InstalledFixedCombatRuntime
from sts_learning.policy import BatchPolicyChoice, BehaviorManifestId
from sts_learning.strategic_rollout import ModelSite, StrategicRunController, run_rollout
from learning.tests.strategic_scope_fixtures import interface_scopes


class RewardProgressTests(unittest.TestCase):
    def test_narrow_pending_potion_menu_keeps_card_indices_from_the_public_collection(self):
        controller = StrategicRunController.__new__(StrategicRunController)
        controller._reward_card_items = {0: [1]}
        controller._skipped_rewards = {0: {1}}
        controller.environment = Mock()
        controller.environment.strategic_decision_audit_json.return_value = json.dumps({
            "decision_site": "reward", "reward_card_item_indices": [1],
            "candidates": [{"kind": "claim_reward", "reward_item_index": 0}]})
        controller._refresh_reward_indices([{"slot": 0, "ordinal": 0,
            "audit": {"candidates": [{"kind": "discard_run_potion", "slot": 2}]}}])
        self.assertEqual(controller._skipped_rewards[0], {1})
        self.assertEqual(controller._reward_card_items[0], [1])

    def test_skipped_reward_does_not_hide_other_rewards(self):
        controller = StrategicRunController.__new__(StrategicRunController)
        controller._skipped_rewards = {0: {2}}
        skipped = {"kind": "open_card_reward", "reward_item_index": 2}
        proceed = {"kind": "proceed"}
        for candidates, expected in (
            ([skipped, proceed], 1),
            ([skipped, {"kind": "claim_reward", "reward_item_index": 1}, proceed], 1),
            ([skipped, {"kind": "open_card_reward", "reward_item_index": 3}, proceed], 1),
        ):
            audit = {"decision_site": "reward", "candidates": candidates}
            self.assertEqual(controller._baseline_ordinal(0, audit, 0), expected)
        # Unskipped rewards continue to use the native owner's ranking.
        self.assertEqual(controller._baseline_ordinal(1, audit, 0), 0)

    def test_claims_remap_skipped_card_rewards_from_the_observed_item_order(self):
        for previous, skipped, consumed, following, expected in (
            ([1, 3], {3}, None, [0, 2], {2}),
            # Claiming a linked relic/key can remove two preceding items.
            ([2, 4], {4}, None, [0, 2], {2}),
            ([1, 3], {3}, 1, [2], {2}),
            # A policy can revisit a skipped item after another resource choice.
            ([1, 3], {1}, 1, [2], set()),
        ):
            controller = StrategicRunController.__new__(StrategicRunController)
            controller._reward_card_items = {0: previous}
            controller._skipped_rewards = {0: skipped}
            controller.environment = Mock()
            controller.environment.strategic_decision_audit_json.return_value = json.dumps({
                "decision_site": "reward", "candidates": [
                    {"kind": "open_card_reward", "reward_item_index": index} for index in following]})
            chosen = {"kind": "claim_reward", "reward_item_index": 0} if consumed is None else {
                "kind": "take_card", "reward_item_index": consumed}
            controller._refresh_reward_indices([
                {"slot": 0, "ordinal": 0, "audit": {"candidates": [chosen]}}])
            self.assertEqual(controller._skipped_rewards[0], expected)
            self.assertEqual(controller._reward_card_items[0], following)

    def test_entering_a_new_room_cannot_inherit_a_skipped_reward_or_navigation_hint(self):
        controller = StrategicRunController.__new__(StrategicRunController)
        controller._rooms = {0: (17, 1, 5)}
        controller._skipped_rewards = {0: {1}}
        controller._reward_card_items = {0: [1]}
        controller._proceeded_rewards = {0}
        controller.environment = Mock()
        controller._refresh_reward_indices([
            {"slot": 0, "audit": {"decision_site": "map", "candidates": []}}],
            {0: SimpleNamespace(seed=17, act=1, floor=6)})
        self.assertEqual(controller._rooms[0], (17, 1, 6))
        self.assertEqual(controller._skipped_rewards, {})
        self.assertEqual(controller._reward_card_items, {})
        self.assertEqual(controller._proceeded_rewards, set())
        controller.environment.strategic_decision_audit_json.assert_not_called()

    def test_leaving_a_collection_clears_skips_and_only_the_next_commit_consumes_its_hint(self):
        controller = StrategicRunController.__new__(StrategicRunController)
        controller._rooms = {0: (17, 3, 40)}
        controller._skipped_rewards = {0: {2}}
        controller._reward_card_items = {0: [2]}
        controller._proceeded_rewards = set()
        controller._model_slots = set()
        controller._model_slot_sites = {}
        controller._pending = [{"slot": 0, "ordinal": 0, "audit": {
            "decision_site": "reward", "candidates": [{"kind": "proceed", "site": "reward"}]}}]
        context = {0: SimpleNamespace(seed=17, act=3, floor=40)}
        controller._contexts = Mock(return_value=context)
        committed = FixedCombatStep({}, {"combats": []}, None)
        controller.environment = SimpleNamespace(
            ready=True, fixed_step=Mock(return_value=committed),
            strategic_decision_audit_json=Mock(return_value=None))
        controller.advance()
        self.assertEqual(controller._skipped_rewards, {})
        self.assertEqual(controller._reward_card_items, {})
        self.assertEqual(controller._proceeded_rewards, {0})

        # The next root can return to another reward collection on this floor.
        controller._pending = [{"slot": 0, "ordinal": 0, "audit": {
            "decision_site": "event", "candidates": [{"kind": "choose_event_option"}]}}]
        controller.environment.fixed_step.return_value = FixedCombatStep(None, {"combats": []}, 0)
        controller.advance()
        self.assertEqual(controller._proceeded_rewards, {0})
        self.assertTrue(controller._pending)

        controller.environment.fixed_step.return_value = committed
        controller.environment.strategic_decision_audit_json.return_value = json.dumps({
            "decision_site": "reward", "candidates": [
                {"kind": "open_card_reward", "reward_item_index": 2}]})
        controller.advance()
        self.assertEqual(controller._proceeded_rewards, set())
        self.assertFalse(controller._skipped_rewards.get(0))
        audit = json.loads(controller.environment.strategic_decision_audit_json.return_value)
        self.assertEqual(controller._baseline_ordinal(0, audit, 0), 0)


class RecordingModel:
    behavior_manifest_id = BehaviorManifestId(b"m"*32)

    def __init__(self):
        self.batches = []

    def choose(self, batch):
        self.batches.append(batch)
        return BatchPolicyChoice.deterministic(
            [int(count)-1 for count in batch["candidate_counts"]], self.behavior_manifest_id)


@unittest.skipUnless(importlib.util.find_spec("sts_learning_bridge"), "native bridge unavailable")
class RealControllerTests(unittest.TestCase):
    def runtime(self):
        return InstalledFixedCombatRuntime(FixedCombatConfig(
            backend="whole_combat", generation_work=30_000, wall_ms=3_000,
            atomic_nodes=None, fallback_action_limit=None))

    def test_run_goal_is_explicit_and_survives_exact_restore(self):
        config = FixedCombatConfig(backend="whole_combat", atomic_nodes=None)
        for final_act, expected in ((False, "act_three_victory"), (True, "heart_victory")):
            runtime = InstalledFixedCombatRuntime(config, final_act=final_act)
            for create in (runtime.environment, runtime.environment_without_combat_potions):
                env = create([17], 20)
                audit = json.loads(env.strategic_decision_audit_json(0))
                self.assertEqual(audit["run_goal"], expected)
                # The ordinary environment's checkpoint keeps its task goal;
                # the combat execution binding stays separate from game state.
                if create == runtime.environment:
                    payload = env.checkpoint_bytes(max_bytes=64*1024*1024)
                    restored = runtime.environment_from_checkpoint(
                        payload, expected_slots=1, max_bytes=64*1024*1024)
                    self.assertEqual(json.loads(restored.strategic_decision_audit_json(0))["run_goal"], expected)

    def test_scoped_model_receives_only_reward_rows_and_skip_advances(self):
        runtime = self.runtime()
        env = runtime.environment([17, 18], 20)
        model = RecordingModel()
        controller = StrategicRunController(env, model_policy=model,
            model_sites=(ModelSite.CARD_REWARD,),
            model_applicability=interface_scopes(runtime.schema, (ModelSite.CARD_REWARD,)))
        visited = set()
        skipped = set()
        passed_skipped = set()
        for _ in range(50):
            result, decisions, _, after = controller.advance()
            self.assertTrue(result.advanced)
            for decision in decisions:
                audit = decision["audit"]
                if audit is None:
                    continue
                visited.add(audit["decision_site"])
                self.assertEqual(decision["owner"] == "model",
                                 audit["decision_site"] == "card_reward")
                if audit["candidates"][decision["ordinal"]]["kind"] == "skip_card_reward":
                    skipped.add((decision["slot"], decision["context"]["floor"]))
            for slot, context in after.items():
                for skipped_slot, floor in skipped:
                    if slot == skipped_slot and context.floor > floor:
                        passed_skipped.add(slot)
            if len(passed_skipped) == 2:
                break
        self.assertEqual(passed_skipped, {0, 1})
        self.assertTrue({"neow", "map", "reward", "card_reward"} <= visited)
        self.assertTrue(model.batches)
        for batch in model.batches:
            self.assertFalse(any(key.startswith("legality_bootstrap_") for key in batch))
            self.assertEqual(len(batch["candidate_row_splits"]), len(batch["slot_indices"])+1)
            self.assertEqual(len(batch["semantic"]["token"]["row_splits"]), len(batch["slot_indices"])+1)

    def test_interruption_retry_and_restore_do_not_rechoose_or_commit_memory(self):
        runtime = self.runtime()
        env = runtime.environment([17], 20)
        sink = Mock()
        controller = StrategicRunController(env, decision_sink=sink)
        actual = env.fixed_step
        env.fixed_step = Mock(return_value=FixedCombatStep(None, {"combats": []}, 0))
        env.choose = Mock(wraps=env.choose)
        result, decisions, _, _ = controller.advance()
        self.assertFalse(result.advanced)
        self.assertTrue(env.ready)
        self.assertEqual(controller._skipped_rewards, {})
        choice_calls = env.choose.call_count
        capture_calls = sink.call_count
        self.assertFalse(any(key.startswith("legality_bootstrap_")
                             for key in sink.call_args.args[0]))
        self.assertFalse(sink.call_args.args[0]["slot_indices"].flags.writeable)
        payload = env.checkpoint_bytes(max_bytes=64*1024*1024)
        memory = json.loads(json.dumps(controller.memory()))
        restored_env = runtime.environment_from_checkpoint(payload, expected_slots=1, max_bytes=64*1024*1024)
        restored = StrategicRunController.from_memory(restored_env, memory)
        env.fixed_step = actual
        result, retry_decisions, _, _ = controller.advance()
        self.assertTrue(result.advanced)
        self.assertEqual(decisions, retry_decisions)
        self.assertEqual(env.choose.call_count, choice_calls)
        self.assertEqual(sink.call_count, capture_calls)
        self.assertTrue(restored.advance()[0].advanced)
        self.assertEqual(env.checkpoint_bytes(max_bytes=64*1024*1024),
                         restored_env.checkpoint_bytes(max_bytes=64*1024*1024))
        with self.assertRaisesRegex(ValueError, "environment mismatch"):
            StrategicRunController.from_memory(restored_env, memory)
        changed = dict(memory, behavior_id="0"*64)
        with self.assertRaisesRegex(ValueError, "policy binding mismatch"):
            StrategicRunController.from_memory(restored_env, changed)

    def test_mutated_model_and_invalid_scope_fail_before_environment_choice(self):
        runtime = self.runtime()
        env = runtime.environment([17], 20)
        with self.assertRaisesRegex(ValueError, "typed ModelSite"):
            StrategicRunController(env, model_sites=("card_reward",))
        with self.assertRaisesRegex(ValueError, "typed behavior identity"):
            StrategicRunController(env, model_policy=SimpleNamespace(behavior_manifest_id=None))
        model = RecordingModel()
        controller = StrategicRunController(env, model_policy=model,
            model_applicability=interface_scopes(runtime.schema, tuple(ModelSite)))
        # At a non-model site, the model is not called at all.
        self.assertTrue(controller.advance()[0].advanced)
        self.assertEqual(model.batches, [])
        controller._model_identity = BehaviorManifestId(b"x"*32)
        for _ in range(40):
            before = env.checkpoint_bytes(max_bytes=64*1024*1024)
            try:
                result = controller.advance()[0]
            except ValueError as error:
                self.assertIn("model changed", str(error))
                self.assertEqual(env.checkpoint_bytes(max_bytes=64*1024*1024), before)
                break
            self.assertTrue(result.advanced)
        else:
            self.fail("did not reach a scoped model site")

    def test_smoke_artifact_keeps_interruption_and_resources_without_training(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"run"
            result = run_rollout(root, seed=17, max_steps=30, progress=lambda _: None,
                executor=FixedCombatConfig(backend="whole_combat", generation_work=1,
                                          wall_ms=1, atomic_nodes=None))
            self.assertEqual(result["stop"], "combat_search_interruption")
            self.assertFalse(result["act1_completed"])
            self.assertEqual(result["fallback_combats"], 0)
            self.assertFalse(result["endpoint"]["is_terminal"])
            self.assertIsNone(result["terminal_outcome"])
            self.assertEqual(json.loads((root/"contract.json").read_text())["goal"], "heart")
            rows = [json.loads(line) for line in (root/"steps.jsonl").read_text().splitlines()]
            self.assertFalse(rows[-1]["committed"])
            self.assertEqual(rows[-1]["before"], rows[-1]["after"])
            self.assertTrue((root/"endpoint.bin").stat().st_size > 0)
            self.assertTrue(json.loads((root/"controller-memory.json").read_text())["pending_decisions"])
            with self.assertRaises(FileExistsError):
                run_rollout(root, seed=17, max_steps=1, progress=lambda _: None)

    def test_reward_exit_memory_is_explicit_and_invalid_slots_do_not_change_state(self):
        runtime = self.runtime()
        env = runtime.environment([17, 18], 20)
        before = env.checkpoint_bytes(max_bytes=64*1024*1024)
        for kwargs in ({"reward_exit_slots": [0]},
                       {"legality_bootstrap": True, "reward_exit_slots": [0, 0]},
                       {"legality_bootstrap": True, "reward_exit_slots": [2]}):
            with self.assertRaisesRegex(ValueError, "reward-exit memory slots"):
                env.decision_batch(**kwargs)
        batch = env.decision_batch(legality_bootstrap=True, reward_exit_slots=[0])
        # A remembered Reward/Proceed has no effect at Neow or symbolic selection.
        self.assertEqual(batch["legality_bootstrap_ordinals"].tolist(), [0, 0])
        self.assertEqual(env.checkpoint_bytes(max_bytes=64*1024*1024), before)
