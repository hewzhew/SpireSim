from __future__ import annotations

import json
import unittest

import numpy as np

from sts_learning.fixed_combat import FixedCombatConfig, FixedCombatController

try:
    from sts_learning_bridge import (
        FixedCombatBranchChoice, FixedCombatResearch, LearningBatchEnv, RUN_BOUNDARY_COMBAT,
    )
except ImportError:
    LearningBatchEnv = None


def choose_bootstrap(env):
    for _ in range(64):
        if env.ready:
            return
        batch = env.decision_batch(legality_bootstrap=True)
        assert all(batch["legality_bootstrap_available"])
        env.choose(batch["decision_token_ids"], batch["legality_bootstrap_ordinals"].tolist())
    raise AssertionError("symbolic selection did not finish")


def first_combat_source(seed=17, *, final_act=False):
    env = LearningBatchEnv([seed], 20, final_act=final_act)
    for _ in range(40):
        choose_bootstrap(env)
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        probe = LearningBatchEnv.from_checkpoint_bytes(before, expected_slots=1, max_bytes=16_777_216)
        probe.step()
        if probe.public_run_contexts()[0][1].boundary_kind == RUN_BOUNDARY_COMBAT:
            return env
        env.step()
    raise AssertionError("prefix did not reach first combat")


@unittest.skipIf(LearningBatchEnv is None, "standalone bridge wheel not installed")
class FixedCombatBridgeTests(unittest.TestCase):
    def test_incremental_research_reuses_work_and_keeps_old_source_choices(self):
        env = first_combat_source(2026100201, final_act=True)
        env.step()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        config = FixedCombatConfig(backend="whole_combat", generation_work=10_000,
                                  wall_ms=3000, atomic_nodes=None)
        options = json.dumps(FixedCombatController(env, config).options)
        research = env.start_fixed_combat_research(0, options)
        self.assertIsInstance(research, FixedCombatResearch)
        self.assertEqual(research.advance_to(1)["choices"], [])
        early = research.advance_to(1000)
        self.assertTrue(early["choices"])
        early_exit = early["environments"][0].checkpoint_bytes(max_bytes=16_777_216)
        early_receipt = early["attempt_json"]
        with self.assertRaises(RuntimeError):
            research.advance_to(999)
        with self.assertRaises(RuntimeError):
            research.advance_to(10_001)
        late = research.advance_to(10_000)
        single = env.fixed_combat_branches(0, options)
        self.assertEqual(late["attempt_json"], single["attempt_json"])
        self.assertEqual(json.loads(late["attempt_json"])["lanes"][0]["work_consumed"], 10_000)
        self.assertEqual(early["attempt_json"], early_receipt)
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)
        env.adopt_fixed_combat_branch(early["choices"][0])
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), early_exit)
        with self.assertRaises(RuntimeError):
            env.adopt_fixed_combat_branch(late["choices"][0])

    def test_pass_through_matches_every_ordinary_transition_column(self):
        env = LearningBatchEnv([17, 18], 20)
        choose_bootstrap(env)
        ordinary = LearningBatchEnv.from_checkpoint_bytes(env.checkpoint_bytes(max_bytes=16_777_216), expected_slots=2, max_bytes=16_777_216)
        expected = ordinary.step()
        step = FixedCombatController(env, FixedCombatConfig()).step()
        self.assertTrue(step.advanced)
        self.assertEqual(set(step.transitions), set(expected))
        for key in expected:
            np.testing.assert_array_equal(step.transitions[key], expected[key])
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), ordinary.checkpoint_bytes(max_bytes=16_777_216))
        self.assertEqual(step.evidence["combats"], [])

    def test_actual_combat_is_separate_from_strategic_labels(self):
        for backend in ("portfolio", "whole_combat"):
            with self.subTest(backend=backend):
                self._actual_combat(backend)

    def _actual_combat(self, backend):
        env = first_combat_source()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        step = FixedCombatController(env, FixedCombatConfig(backend=backend)).step()
        self.assertTrue(step.advanced)
        combat, = step.evidence["combats"]
        self.assertGreater(combat["exit_decision_step"], combat["entry_decision_step"])
        self.assertEqual(len(step.transitions["action_ordinals"]), 1)
        self.assertNotEqual(env.public_run_contexts()[0][1].boundary_kind, RUN_BOUNDARY_COMBAT)
        self.assertFalse(env.ready)
        self.assertNotEqual(before, env.checkpoint_bytes(max_bytes=16_777_216))
        if backend == "whole_combat":
            self.assertEqual(combat["attempt"]["lanes"][0]["provenance"]["producer_name"],
                             "whole_combat_solver")

    def test_unknown_preserves_pending_choices_for_explicit_retry(self):
        env = first_combat_source()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        tiny = FixedCombatConfig(generation_work=1, wall_ms=1, atomic_nodes=None)
        step = FixedCombatController(env, tiny).step()
        self.assertFalse(step.advanced)
        self.assertIsNone(step.transitions)
        self.assertEqual(step.unresolved_slot, 0)
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)
        self.assertTrue(env.ready)
        retry = FixedCombatController(env, FixedCombatConfig()).step()
        self.assertTrue(retry.advanced)

    def test_invalid_native_options_leave_ready_state_unchanged(self):
        env = first_combat_source()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        with self.assertRaises(ValueError):
            env.step_with_fixed_combat('{"search": {"unexpected": 1}}')
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)
        self.assertTrue(env.ready)

    def test_explicit_fallback_replays_the_baseline_and_stops_before_rewards(self):
        env = first_combat_source()
        ordinary = LearningBatchEnv.from_checkpoint_bytes(
            env.checkpoint_bytes(max_bytes=16_777_216), expected_slots=1, max_bytes=16_777_216,
        )
        ordinary.step()
        actions = 0
        for _ in range(512):
            if ordinary.public_run_contexts()[0][1].boundary_kind != RUN_BOUNDARY_COMBAT:
                break
            choose_bootstrap(ordinary)
            ordinary.step()
            actions += 1
        step = FixedCombatController(env, FixedCombatConfig(
            generation_work=1, wall_ms=1, atomic_nodes=None, fallback_action_limit=512,
        )).step()
        self.assertTrue(step.advanced)
        combat, = step.evidence["combats"]
        self.assertIsNone(combat["selected_candidate_ordinal"])
        self.assertEqual(len(combat["fallback"]["steps"]), actions)
        self.assertFalse(combat["fallback"]["action_limit_reached"])
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), ordinary.checkpoint_bytes(max_bytes=16_777_216))

    def test_fallback_limit_is_still_unknown_and_rolls_back(self):
        env = first_combat_source()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        step = FixedCombatController(env, FixedCombatConfig(
            generation_work=1, wall_ms=1, atomic_nodes=None, fallback_action_limit=1,
        )).step()
        self.assertFalse(step.advanced)
        self.assertTrue(step.evidence["combats"][0]["fallback"]["action_limit_reached"])
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)
        self.assertTrue(env.ready)

    def test_restricted_environment_requires_explicit_never_potion_executor(self):
        env = LearningBatchEnv.without_combat_potions([17], 20)
        choose_bootstrap(env)
        before = env.public_run_contexts()[0][1]
        with self.assertRaisesRegex(RuntimeError, "restricted environment"):
            FixedCombatController(env, FixedCombatConfig()).step()
        after = env.public_run_contexts()[0][1]
        self.assertEqual((before.floor, before.hp, before.gold), (after.floor, after.hp, after.gold))
        self.assertTrue(env.ready)
        self.assertTrue(FixedCombatController(env, FixedCombatConfig(allow_potions=False)).step().advanced)

    def test_combat_source_cannot_be_mixed_into_strategic_model_labels(self):
        env = first_combat_source()
        env.step()
        choose_bootstrap(env)
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        with self.assertRaisesRegex(RuntimeError, "StrategicSourceRequired"):
            FixedCombatController(env, FixedCombatConfig()).step()
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)

    def test_resource_branches_are_exact_independent_and_can_claim_real_rewards(self):
        env = first_combat_source(2026100201, final_act=True)
        env.step()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        source = env.decision_batch()
        config = FixedCombatConfig(backend="whole_combat", generation_work=30_000,
                                  wall_ms=3000, atomic_nodes=None)
        result = env.fixed_combat_branches(0, json.dumps(FixedCombatController(env, config).options))
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)
        self.assertEqual(result["information_access"], "oracle_exact_combat")
        self.assertEqual(result["source_decision_token_id"], source["decision_token_ids"][0])
        self.assertEqual(result["source_public_snapshot_id"], source["public_snapshot_ids"][0])
        attempt = json.loads(result["attempt_json"])
        entry = json.loads(result["entry_json"])
        branches = result["environments"]
        ordinals = result["candidate_ordinals"]
        self.assertGreaterEqual(len(ordinals), 2)
        self.assertEqual(len(branches), len(ordinals))
        spends = set()
        branch_checkpoints = [branch.checkpoint_bytes(max_bytes=16_777_216) for branch in branches]
        for branch, ordinal in zip(branches, ordinals, strict=True):
            self.assertEqual(branch.slot_count, 1)
            self.assertFalse(branch.ready)
            (slot, context), = branch.public_run_contexts()
            outcome = attempt["candidates"][ordinal]["outcome"]
            self.assertFalse(context.is_combat)
            self.assertEqual((context.hp, context.max_hp, context.gold),
                             (outcome["final_hp"], outcome["persistent"]["max_hp"], outcome["final_gold"]))
            retained = entry["potion_ids"].copy()
            for spent in outcome["consumed_entry_potion_slots"]:
                retained[spent] = None
            for potion in outcome["generated_potions"]:
                retained[potion["slot"]] = potion["potion_id"]
            self.assertEqual(context.potion_ids, retained)
            audit = json.loads(branch.strategic_decision_audit_json(slot))
            self.assertEqual(audit["run_goal"], "heart_victory")
            self.assertEqual(audit["decision_site"], "reward")
            spends.add(outcome["total_potions_consumed"])
        self.assertGreaterEqual(len(spends), 2)
        branch = branches[0]
        gold = branch.public_run_contexts()[0][1].gold
        choose_bootstrap(branch)
        transitions = branch.step()
        self.assertEqual(len(transitions["slot_indices"]), 1)
        self.assertGreater(branch.public_run_contexts()[0][1].gold, gold)
        self.assertEqual([branch.checkpoint_bytes(max_bytes=16_777_216) for branch in branches[1:]],
                         branch_checkpoints[1:])
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)

    def test_resource_branch_miss_and_invalid_requests_leave_source_unchanged(self):
        env = first_combat_source()
        ready_source = env.checkpoint_bytes(max_bytes=16_777_216)
        options = FixedCombatController(env, FixedCombatConfig(backend="whole_combat",
            generation_work=1, wall_ms=1, atomic_nodes=None)).options
        with self.assertRaisesRegex(ValueError, "undecoded combat root"):
            env.fixed_combat_branches(0, json.dumps(options))
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), ready_source)
        env.step()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        result = env.fixed_combat_branches(0, json.dumps(options))
        self.assertEqual(result["candidate_ordinals"], [])
        self.assertEqual(result["environments"], [])
        self.assertEqual(result["choices"], [])
        self.assertEqual(json.loads(result["attempt_json"])["candidates"], [])
        options["fallback_action_limit"] = 512
        with self.assertRaisesRegex(RuntimeError, "require witnesses"):
            env.fixed_combat_branches(0, json.dumps(options))
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)

    def test_branch_adoption_keeps_other_pending_choices_and_ignores_research_progress(self):
        source = first_combat_source(2026100201, final_act=True)
        source.step()
        env = LearningBatchEnv([2026100201, 18], 20, final_act=True)
        choose_bootstrap(env)
        env.restore_slot(0, source.checkpoint_slot(0))
        other = env.checkpoint_slots([1]).checkpoint_bytes(max_bytes=16_777_216)
        config = FixedCombatConfig(backend="whole_combat", generation_work=30_000,
                                  wall_ms=3000, atomic_nodes=None)
        result = env.fixed_combat_branches(0, json.dumps(FixedCombatController(env, config).options))
        branch, choice = result["environments"][-1], result["choices"][-1]
        self.assertIsInstance(choice, FixedCombatBranchChoice)
        self.assertEqual(choice.candidate_ordinal, result["candidate_ordinals"][-1])
        self.assertEqual(choice.source_decision_token_id, result["source_decision_token_id"])
        expected = branch.checkpoint_bytes(max_bytes=16_777_216)
        choose_bootstrap(branch)
        branch.step()
        self.assertNotEqual(branch.checkpoint_bytes(max_bytes=16_777_216), expected)
        adoption = env.adopt_fixed_combat_branch(choice)
        receipt = json.loads(adoption["fixed_combat_receipt_json"])
        self.assertEqual(receipt["selected_candidate_ordinal"], choice.candidate_ordinal)
        self.assertEqual(adoption["information_access"], "oracle_exact_combat")
        self.assertEqual(env.checkpoint_slots([1]).checkpoint_bytes(max_bytes=16_777_216), other)
        self.assertEqual(env.public_run_contexts()[0][1].gold, receipt["exit"]["gold"])
        single = LearningBatchEnv([1], 20)
        single.restore_slot(0, env.checkpoint_slot(0))
        self.assertEqual(single.checkpoint_bytes(max_bytes=16_777_216), expected)
        batch = env.decision_batch()
        self.assertEqual(batch["slot_indices"].tolist(), [0])
        self.assertEqual(batch["decision_token_ids"], [adoption["next_decision_token_id"]])

    def test_branch_adoption_rejects_foreign_and_pending_sources_without_mutation(self):
        env = first_combat_source()
        env.step()
        config = FixedCombatConfig(backend="whole_combat", generation_work=30_000,
                                  wall_ms=3000, atomic_nodes=None)
        result = env.fixed_combat_branches(0, json.dumps(FixedCombatController(env, config).options))
        choice = result["choices"][0]
        foreign = LearningBatchEnv([18], 20)
        before = foreign.checkpoint_bytes(max_bytes=16_777_216)
        with self.assertRaisesRegex(RuntimeError, "StaleDecisionToken"):
            foreign.adopt_fixed_combat_branch(choice)
        self.assertEqual(foreign.checkpoint_bytes(max_bytes=16_777_216), before)
        choose_bootstrap(env)
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        with self.assertRaisesRegex(ValueError, "undecoded combat root"):
            env.adopt_fixed_combat_branch(choice)
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)
        env.step()
        before = env.checkpoint_bytes(max_bytes=16_777_216)
        with self.assertRaisesRegex(RuntimeError, "StaleDecisionToken"):
            env.adopt_fixed_combat_branch(choice)
        self.assertEqual(env.checkpoint_bytes(max_bytes=16_777_216), before)


class FixedCombatConfigTests(unittest.TestCase):
    def test_limits_are_positive_and_typed(self):
        for values in ({"wall_ms": 0}, {"quantum_wall_ms": 0}, {"generation_work": True},
                       {"atomic_nodes": -1}, {"allow_potions": 1}, {"backend": "unknown"}):
            with self.assertRaises(ValueError):
                FixedCombatConfig(**values)
