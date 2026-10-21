from __future__ import annotations

import unittest
import json
from types import SimpleNamespace

from sts_learning.fixed_combat import FixedCombatStep
from sts_learning.fixed_combat_environment import FixedCombatUnresolved
from sts_learning.fixed_seed_evaluation import FixedSeedPanel, FixedSeedOutcome, _public_choices, evaluate_fixed_seed_panel, paired_seed_floor_deltas, panel_summary
from sts_learning.outcomes import TerminalAttemptOutcome
from sts_learning.policy import BatchPolicyChoice, BehaviorManifestId, SelectionProbability
from sts_learning.run_resource_trace import RunPublicContext
from sts_learning.seeds import SeedPartition, SeedSchedule


def seeds(n):
    return SeedSchedule(SeedPartition.HELD_OUT).plan(range(n))[0].seeds


def outcome(slot, floor):
    return TerminalAttemptOutcome(slot, -1, 1, floor, 0, 75, 0)


class StubEnvironment:
    def __init__(self, seeds, fail=False):
        self.slot_count = len(seeds)
        self.terminal_count = 0
        self.ready = True
        self.steps = 0
        self.fail = fail

    def step(self):
        self.steps += 1
        if self.fail and self.steps == 2:
            raise FixedCombatUnresolved(FixedCombatStep(None, {}, 1))
        slots = [0] if self.steps == 1 else ([] if self.steps < 3 else [1])
        self.terminal_count += len(slots)
        return {name: values for name, values in zip(
            ("terminal_slot_indices", "terminal_reward", "terminal_act", "terminal_floor", "terminal_hp", "terminal_max_hp", "terminal_gold"),
            (slots, [-1]*len(slots), [1]*len(slots), [5]*len(slots), [0]*len(slots), [75]*len(slots), [0]*len(slots)))}


class FixedPanelTests(unittest.TestCase):
    def test_controller_evidence_uses_each_source_context_even_for_the_same_slot(self):
        cohort = seeds(1)
        class Env(StubEnvironment):
            def public_run_contexts(self):
                return [(0, SimpleNamespace(boundary_kind=0, is_combat=False,
                    is_terminal=bool(self.terminal_count), seed=cohort[0], act=1,
                    floor=5, hp=0 if self.terminal_count else 75, max_hp=75,
                    gold=0, potion_ids=[], encounter_id=None, monster_ids=[]))]

        class Controller:
            behavior_manifest_id = BehaviorManifestId(b"0"*32)
            def __init__(self, env):
                self.env = env
            def advance(self, **kwargs):
                contexts = [RunPublicContext(0, 0, False, False, cohort[0], 1, 5,
                                            75, 75, gold, (), None, ())
                            for gold in (100, 50)]
                audit = {"decision_site": "event", "candidates": [{"kind": "event_option"}]}
                decisions = [{"slot": 0, "ordinal": 0, "audit": audit,
                              "context": context.__dict__} for context in contexts]
                transitions = {"slot_indices": [0], **self.env.step()}
                return FixedCombatStep(transitions, {}, None), decisions, {0: contexts[0]}, {}

        events = []
        rows = evaluate_fixed_seed_panel(lambda s: Env(s), lambda *_: self.fail("policy should not run"),
            FixedSeedPanel(cohort, replicas=1, slots=1, max_steps=2),
            evidence_sink=events.append, controller_factory=Controller)
        self.assertIsNotNone(rows[0].outcome)
        choices = [event for event in events if event["kind"] == "strategic_choice"]
        self.assertEqual([event["public_context"]["gold"] for event in choices], [100, 50])
        self.assertTrue(all(event["available_candidates"][0]["probability"] == 1.0 for event in choices))

    def test_campfire_choice_retains_public_deck_and_relics(self):
        deck = [{"card": "Bash", "upgrades": 0},
                {"card": "Strike", "upgrades": 1}]
        relics = [{"relic": "BurningBlood", "counter": 0,
                   "used_up": False, "amount": 0}]

        class Env:
            @staticmethod
            def public_run_contexts():
                return [(0, SimpleNamespace(
                    boundary_kind=0, is_combat=False, is_terminal=False,
                    seed=123, act=1, floor=15, hp=28, max_hp=75, gold=140,
                    potion_ids=[None, None], encounter_id=None,
                    monster_ids=[]))]

            @staticmethod
            def strategic_decision_audit_json(slot):
                assert slot == 0
                return json.dumps({
                    "schema": "sts-learning-strategic-decision-audit-v2",
                    "decision_site": "campfire",
                    "candidates": [{"kind": "rest"}, {"kind": "smith", "card": "Bash"}],
                    "route_summaries": [None, None],
                    "public_deck": deck,
                    "public_relics": relics,
                })

        events = _public_choices(Env(), {"slot_indices": [0]},
                                 SimpleNamespace(ordinals=(0,)), [123], 0)
        self.assertEqual(events[0]["site"], "campfire")
        self.assertEqual(events[0]["public_deck"], deck)
        self.assertEqual(events[0]["public_relics"], relics)
        self.assertEqual(events[0]["available_candidates"], [
            {"ordinal": 0, "candidate": {"kind": "rest"}},
            {"ordinal": 1, "candidate": {"kind": "smith", "card": "Bash"}},
        ])

    def test_map_choice_evidence_retains_all_public_routes(self):
        candidates = [
            {"kind": "choose_route_node", "x": 0, "y": 0},
            {"kind": "choose_route_node", "x": 2, "y": 0},
        ]
        routes = [
            {"min_elites": 1, "max_elites": 2},
            {"min_elites": 0, "max_elites": 0},
        ]
        deck = [{"card": "Bash", "upgrades": 0},
                {"card": "Strike", "upgrades": 0}]
        relics = [{"relic": "BurningBlood", "counter": 0,
                   "used_up": False, "amount": 0}]
        class Env:
            @staticmethod
            def public_run_contexts():
                return [(0, SimpleNamespace(
                    boundary_kind=0, is_combat=False, is_terminal=False,
                    seed=123, act=1, floor=8, hp=41, max_hp=75, gold=295,
                    potion_ids=[None, None], encounter_id=None,
                    monster_ids=[]))]

            def strategic_decision_audit_json(self, slot):
                self.assert_slot(slot)
                return json.dumps({"schema": "sts-learning-strategic-decision-audit-v2",
                                   "decision_site": "map", "candidates": candidates,
                                   "route_summaries": routes, "public_deck": deck,
                                   "public_relics": relics})
            @staticmethod
            def assert_slot(slot):
                assert slot == 0
        events = _public_choices(Env(), {"slot_indices": [0]},
                                 SimpleNamespace(ordinals=(1,)), [123], 0)
        self.assertEqual(events[0]["candidate"], candidates[1])
        self.assertEqual(events[0]["public_deck"], deck)
        self.assertEqual(events[0]["public_relics"], relics)
        self.assertEqual(events[0]["public_context"]["floor"], 8)
        self.assertEqual(events[0]["public_context"]["hp"], 41)
        self.assertEqual(events[0]["public_context"]["gold"], 295)
        self.assertEqual(events[0]["available_candidates"], [
            {"ordinal": 0, "candidate": candidates[0], "route_summary": routes[0]},
            {"ordinal": 1, "candidate": candidates[1], "route_summary": routes[1]},
        ])

        valued = _public_choices(Env(), {"slot_indices": [0]},
                                 SimpleNamespace(ordinals=(1,)), [123], 0, (0.25,))
        self.assertEqual(valued[0]["predicted_return"], 0.25)
        with self.assertRaisesRegex(ValueError, "value predictions"):
            _public_choices(Env(), {"slot_indices": [0]},
                            SimpleNamespace(ordinals=(1,)), [123], 0, (float("nan"),))

        sampled = SimpleNamespace(ordinals=(1,),
                                  selection_probabilities=(SelectionProbability.known(0.75),))
        distributed = _public_choices(Env(), {"slot_indices": [0]}, sampled,
                                      [123], 0, (0.25,), ((0.25, 0.75),))
        self.assertEqual([option["probability"] for option in distributed[0]["available_candidates"]],
                         [0.25, 0.75])
        with self.assertRaisesRegex(ValueError, "disagree with sampled choice"):
            _public_choices(Env(), {"slot_indices": [0]}, sampled,
                            [123], 0, None, ((0.75, 0.25),))
        with self.assertRaisesRegex(ValueError, "align with public candidates"):
            _public_choices(Env(), {"slot_indices": [0]}, sampled,
                            [123], 0, None, ((1.0,),))

    def test_fast_terminal_does_not_replace_or_erase_slow_seed(self):
        calls = []
        def create(cohort):
            calls.append(cohort)
            return StubEnvironment(cohort)
        panel = FixedSeedPanel(seeds(2), replicas=2, slots=2, max_steps=3)
        rows = evaluate_fixed_seed_panel(create, lambda *_: None, panel)
        self.assertEqual(len(calls), 2)
        self.assertEqual({(r.seed,r.replica) for r in rows}, {(s,r) for s in panel.seeds for r in range(2)})
        self.assertTrue(all(r.outcome is not None for r in rows))

    def test_unknown_keeps_finished_seed_and_censors_remaining_seed(self):
        rows = evaluate_fixed_seed_panel(lambda s: StubEnvironment(s,True), lambda *_: None,
                                        FixedSeedPanel(seeds(2),replicas=1,slots=2))
        self.assertIsNotNone(rows[0].outcome)
        self.assertIsNone(rows[1].outcome)
        self.assertEqual(rows[1].censored_reason, "executor_unresolved_batch")

    def test_limit_is_censored_and_invalid_pairs_are_not_averaged(self):
        rows = evaluate_fixed_seed_panel(StubEnvironment, lambda *_: None,
                                        FixedSeedPanel(seeds(2),replicas=1,slots=2,max_steps=1))
        self.assertEqual(rows[1].censored_reason,"step_limit")
        self.assertIsNone(paired_seed_floor_deltas(rows, rows))
        with self.assertRaises(ValueError):
            paired_seed_floor_deltas(rows, rows[:1])

    def test_replica_deltas_are_grouped_by_seed(self):
        s = seeds(1)[0]
        left = [FixedSeedOutcome(s,r,outcome(0,f),None) for r,f in enumerate((5,7))]
        right = [FixedSeedOutcome(s,r,outcome(0,f),None) for r,f in enumerate((7,7))]
        self.assertEqual(paired_seed_floor_deltas(left,right),{s:1.0})

    def test_fallback_death_is_attributed_to_its_seed_and_excludes_fallback_free_pair(self):
        class InstrumentedEnvironment(StubEnvironment):
            last_combat_fallbacks = None

            def step(self):
                result = super().step()
                self.last_combat_fallbacks = ((0, True),) if self.steps == 1 else ()
                return result

        rows = evaluate_fixed_seed_panel(InstrumentedEnvironment, lambda *_: None,
                                         FixedSeedPanel(seeds(2), replicas=1, slots=2, max_steps=3))
        self.assertEqual([row.combat_fallbacks for row in rows], [1, 0])
        self.assertEqual([row.fallback_terminal_zero_hp for row in rows], [True, False])
        self.assertEqual(panel_summary(rows)["completed_by_fallback_death"], 1)
        self.assertEqual(paired_seed_floor_deltas(rows, rows), {seed: 0.0 for seed in seeds(2)})
        self.assertIsNone(paired_seed_floor_deltas(rows, rows, require_fallback_free=True))
        clean = [FixedSeedOutcome(row.seed, row.replica, row.outcome, None, 0, False)
                 for row in rows]
        self.assertEqual(paired_seed_floor_deltas(clean, clean, require_fallback_free=True),
                         {seed: 0.0 for seed in seeds(2)})

    def test_act_two_reach_is_reported_separately_from_floor_and_censoring(self):
        a, b, c = seeds(3)
        rows = [
            FixedSeedOutcome(a, 0, TerminalAttemptOutcome(0, -1, 1, 16, 0, 75, 0), None),
            FixedSeedOutcome(b, 0, TerminalAttemptOutcome(0, -1, 2, 18, 0, 75, 0), None),
            FixedSeedOutcome(c, 0, None, "step_limit"),
        ]
        summary = panel_summary(rows)
        self.assertEqual(summary["completed_reached_act2"], 1)
        self.assertEqual(summary["completed_died_act1"], 1)
        self.assertEqual(summary["censored"], 1)

    def test_training_seed_is_rejected(self):
        training = SeedSchedule(SeedPartition.TRAINING).plan([0])[0].seeds
        with self.assertRaises(ValueError):
            FixedSeedPanel(training)

    def test_changing_policy_identity_is_rejected_before_choose(self):
        class ChangingPolicy:
            behavior_manifest_id = BehaviorManifestId(b"0"*32)
            def choose(self, batch):
                self.behavior_manifest_id = BehaviorManifestId(b"1"*32)
                return BatchPolicyChoice.deterministic([0],self.behavior_manifest_id)
        class Env:
            slot_count = 1
            terminal_count = 0
            ready = False
            def decision_batch(self, **kwargs):
                return {"slot_indices":[0],"decision_token_ids":["token"]}
            def choose(self,*args):
                raise AssertionError("must reject policy identity before mutation")
        with self.assertRaisesRegex(ValueError,"identity"):
            evaluate_fixed_seed_panel(lambda _:Env(),lambda *_:ChangingPolicy(),
                                      FixedSeedPanel(seeds(1),replicas=1,slots=1))
