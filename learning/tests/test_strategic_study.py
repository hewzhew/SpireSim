from __future__ import annotations

import tempfile
import unittest
import importlib.util
import json
import itertools
from types import SimpleNamespace
from unittest.mock import patch
from dataclasses import replace
from pathlib import Path

from sts_learning.fixed_combat import FixedCombatConfig, FixedCombatStep
from sts_learning.fixed_combat_environment import FixedCombatUnresolved
_AVAILABLE = importlib.util.find_spec("torch") is not None and importlib.util.find_spec("sts_learning_bridge") is not None
if _AVAILABLE:
    from sts_learning.strategic_study import StrategicStudySpec, RuleInitializationSpec, _Evidence, _session_config, run_study, verify_study
    from sts_learning.fixed_combat_environment import InstalledFixedCombatRuntime
    from sts_learning.resume_store import ResumeManifestId
    from sts_learning.torch_session import CategoricalOnlineSession, CategoricalOnlineSessionFactory, NoRecoveryCurriculum
    from sts_learning.torch_resume_restore import TorchResumeRestoreError


@unittest.skipUnless(_AVAILABLE, "optional torch/native bridge unavailable")
class StrategicStudyTests(unittest.TestCase):
    def test_final_panel_runs_after_all_training_and_uses_every_initial_final_pair(self):
        from sts_learning.fixed_seed_evaluation import evaluate_fixed_seed_panel
        from sts_learning.strategic_study import _evaluation_panels
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"final-panel"
            spec = StrategicStudySpec(model_seeds=(7, 8), generations=1,
                episodes_per_generation=1, slots=1, evaluation_seeds=1,
                evaluation_replicas=1, final_evaluation_seed_start=3030000000,
                final_evaluation_seeds=1, hidden_dim=8, ascension=0, final_act=False,
                strategic_navigation=True, include_rule_baseline=True,
                executor=FixedCombatConfig(generation_work=1, wall_ms=1,
                    atomic_nodes=None, fallback_action_limit=512))
            development, final = _evaluation_panels(spec)
            observed = []
            def evaluate(*args, **kwargs):
                panel = args[2]
                observed.append(panel.seeds)
                if panel.seeds == final.seeds:
                    for seed in spec.model_seeds:
                        models = json.loads((root/f"model-{seed}"/"models.json").read_text())
                        self.assertEqual(models["training_stop"], "generation_limit")
                        self.assertEqual(len(json.loads((root/f"model-{seed}"/"updates.json").read_text())), 1)
                    self.assertEqual(len(json.loads((root/"runs.json").read_text())), 2)
                return evaluate_fixed_seed_panel(*args, **kwargs)
            with patch("sts_learning.strategic_study.evaluate_fixed_seed_panel", side_effect=evaluate):
                report = run_study(root, spec, progress=lambda _: None)
            self.assertEqual(observed, [development.seeds]*6 + [final.seeds]*6)
            self.assertEqual([run["model_seed"] for run in report["final_test"]["runs"]], [7, 8])
            for run in report["final_test"]["runs"]:
                self.assertEqual(run["initial"]["completed"], 1)
                self.assertEqual(run["final"]["completed"], 1)
                saved = json.loads((root/f"model-{run['model_seed']}"/"models.json").read_text())
                self.assertEqual(run["final_manifest"].digest.hex(), saved["final_manifest"]["digest"])
            contract = json.loads((root/"contract.json").read_text())
            self.assertEqual(contract["final_panel"]["seeds"], list(final.seeds))
            self.assertTrue((root/"final-test"/"executor-counts.json").exists())
            self.assertTrue(verify_study(root)["study_complete"])

    def test_overlapping_final_panel_is_rejected_before_creating_a_study(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"overlap"
            spec = StrategicStudySpec(final_evaluation_seed_start=2026094000)
            with self.assertRaisesRegex(ValueError, "seeds overlap"):
                run_study(root, spec)
            self.assertFalse(root.exists())

    def test_declared_evidence_bound_stops_before_appending_an_oversized_row(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            evidence = _Evidence(root, max_bytes=64)
            evidence.decision({"value": 1})
            saved = (root/"strategic-evidence.jsonl").read_bytes()
            with self.assertRaisesRegex(RuntimeError, "exceeded 64 bytes"):
                evidence.decision({"value": "x"*64})
            self.assertEqual((root/"strategic-evidence.jsonl").read_bytes(), saved)

    def test_uniform_boss_reward_potion_modal_keeps_skipped_card_and_reaches_terminal(self):
        from sts_learning.strategic_study import _UniformPolicy, _streams
        from sts_learning.fixed_seed_evaluation import FixedSeedPanel, evaluate_fixed_seed_panel
        runtime = InstalledFixedCombatRuntime(FixedCombatConfig(backend="whole_combat",
            generation_work=10000, wall_ms=1000, atomic_nodes=None, fallback_action_limit=4096),
            final_act=False, strategic_navigation=True)
        modal_states = []
        def environment(seeds):
            env = runtime.environment(seeds, 0)
            step = env.fixed_step
            def inspect_step():
                result = step()
                if result.advanced:
                    raw = env.strategic_decision_audit_json(0)
                    audit = None if raw is None else json.loads(raw)
                    if (audit is not None and audit["decision_site"] == "reward"
                            and len(audit["candidates"]) == 1
                            and audit["candidates"][0]["kind"] == "claim_reward"
                            and audit["reward_card_item_indices"]):
                        modal_states.append(env._reward_memory.state())
                return result
            env.fixed_step = inspect_step
            return env
        panel = FixedSeedPanel((2026094086,), replicas=1, slots=1, max_steps=1024)
        rows = evaluate_fixed_seed_panel(environment,
            lambda seeds, replica: _UniformPolicy(_streams(seeds, replica)), panel)
        self.assertIsNotNone(rows[0].outcome)
        self.assertTrue(modal_states)
        self.assertTrue(any(state["skipped_rewards"].get("0") == [1] for state in modal_states))

    def test_rule_collection_keeps_unknown_macro_uncommitted_and_never_trains_it(self):
        from sts_learning.strategic_demonstrations import load_scoped_strategic_samples
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"unknown-rule"
            spec = StrategicStudySpec(model_seeds=(7,), generations=1, episodes_per_generation=1,
                slots=1, evaluation_seeds=1, evaluation_replicas=1, hidden_dim=8,
                ascension=0, final_act=False, strategic_navigation=True,
                rule_initialization=RuleInitializationSpec(training_runs=1, validation_runs=1, epochs=1),
                executor=FixedCombatConfig(generation_work=1, wall_ms=1, atomic_nodes=None,
                    fallback_action_limit=None))
            with self.assertRaises(FixedCombatUnresolved):
                run_study(root, spec, progress=lambda _: None)
            manifest, rows = load_scoped_strategic_samples(root/"demonstrations-training"/"cohort-00000")
            self.assertTrue(any(not record["committed"] for _, record in rows))
            self.assertEqual(manifest["terminal_outcome"]["stop_reason"], "executor_unresolved_batch")
            self.assertFalse((root/"model-7").exists())
            self.assertFalse((root/"summary.json").exists())
            self.assertTrue(verify_study(root)["status"] == "verified")

    def test_rule_clone_reaches_real_ppo_and_keeps_random_control_and_seed_isolation(self):
        from sts_learning.seeds import SeedPartition, SeedPartitionSpec
        from sts_learning.strategic_demonstrations import load_scoped_strategic_samples
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"rule-start"
            spec = StrategicStudySpec(model_seeds=(7,), generations=1,
                episodes_per_generation=1, slots=1, evaluation_seeds=1, evaluation_replicas=1,
                hidden_dim=8, ascension=0, final_act=False, strategic_navigation=True,
                rule_initialization=RuleInitializationSpec(training_runs=1, validation_runs=1,
                    epochs=1, batch_rows=8),
                executor=FixedCombatConfig(generation_work=1, wall_ms=1, atomic_nodes=None,
                    fallback_action_limit=512))
            self.assertEqual(spec, StrategicStudySpec.from_dict(json.loads(json.dumps(
                spec.__dict__, default=lambda value: value.__dict__))))
            report = run_study(root, spec, progress=lambda _: None)
            run = report["runs"][0]
            self.assertEqual(run["initialization"], "rule_clone_actor")
            self.assertEqual(run["random"]["completed"], 1)
            self.assertEqual(run["initial"]["completed"], 1)
            self.assertEqual(run["final"]["completed"], 1)
            self.assertEqual([point["name"] for point in run["learning_curve"]], ["random", "initial", "final"])
            partition = SeedPartitionSpec()
            for name, expected in (("training", SeedPartition.TRAINING), ("validation", SeedPartition.HELD_OUT)):
                manifest, rows = load_scoped_strategic_samples(root/f"demonstrations-{name}"/"cohort-00000")
                self.assertGreater(manifest["summary"]["routine_rows_excluded"], 0)
                self.assertTrue(all(record["committed"] for _, record in rows))
                self.assertTrue(all(partition.classify(record["decision"]["context"]["seed"]) is expected
                                    for _, record in rows))
                self.assertTrue(all(int(batch["candidate_counts"][0]) > 1 for batch, _ in rows))
            clone = json.loads((root/"model-7"/"rule-initialization.json").read_text())
            self.assertGreater(clone["optimizer_steps"], 0)
            models = json.loads((root/"model-7"/"models.json").read_text())
            runtime = InstalledFixedCombatRuntime(spec.executor, final_act=False, strategic_navigation=True)
            factory = CategoricalOnlineSessionFactory(root/"model-7"/"session", runtime.session_bridge(),
                _session_config(spec), NoRecoveryCurriculum())
            restored = factory.restore(ResumeManifestId(bytes.fromhex(models["initial_resume"]["digest"])))
            self.assertEqual(restored.active_behavior_manifest_id.digest.hex(), models["initial_manifest"]["digest"])
            self.assertTrue(restored.advance_generation(max_batch_steps=spec.max_steps).promoted)
            self.assertEqual(restored.active_behavior_manifest_id.digest.hex(), models["final_manifest"]["digest"])
            self.assertTrue(verify_study(root)["study_complete"])

    def test_navigation_session_restore_repeats_the_next_real_ppo_update(self):
        with tempfile.TemporaryDirectory() as temp:
            spec = StrategicStudySpec(model_seeds=(7,), generations=2,
                episodes_per_generation=2, slots=2, evaluation_seeds=2, evaluation_replicas=1,
                hidden_dim=8, ascension=0, final_act=False, strategic_navigation=True,
                executor=FixedCombatConfig(generation_work=1, wall_ms=1,
                    atomic_nodes=None, fallback_action_limit=512))
            runtime = InstalledFixedCombatRuntime(spec.executor, strategic_navigation=True)
            factory = CategoricalOnlineSessionFactory(Path(temp)/"session", runtime.session_bridge(),
                _session_config(spec), NoRecoveryCurriculum())
            session = factory.new(model_seed=7, behavior_seed=11)
            self.assertTrue(session.advance_generation(max_batch_steps=512).promoted)
            published = session.publish()
            restored = factory.restore(published.manifest_id)
            self.assertTrue(session.advance_generation(max_batch_steps=512).promoted)
            self.assertTrue(restored.advance_generation(max_batch_steps=512).promoted)
            self.assertEqual(session.active_behavior_manifest_id, restored.active_behavior_manifest_id)
            self.assertEqual(session.runner.trainer.last_result.loss, restored.runner.trainer.last_result.loss)

    def test_terminal_score_session_restores_and_keeps_declared_value_target(self):
        with tempfile.TemporaryDirectory() as temp:
            spec = StrategicStudySpec(model_seeds=(7,), generations=2,
                episodes_per_generation=2, slots=2, hidden_dim=8,
                ascension=0, final_act=False, strategic_navigation=True,
                reward_metric="floor_progress", value_prediction_target="terminal_score",
                canonical_behavior_rows=True,
                executor=FixedCombatConfig(generation_work=1, wall_ms=1,
                    atomic_nodes=None, fallback_action_limit=512))
            self.assertEqual(spec, StrategicStudySpec.from_dict(json.loads(json.dumps(
                spec.__dict__, default=lambda value: value.__dict__))))
            runtime = InstalledFixedCombatRuntime(spec.executor, final_act=False, strategic_navigation=True)
            factory = CategoricalOnlineSessionFactory(Path(temp)/"session", runtime.session_bridge(),
                _session_config(spec), NoRecoveryCurriculum())
            session = factory.new(model_seed=7, behavior_seed=11)
            self.assertTrue(session.advance_generation(max_batch_steps=512).promoted)
            published = session.publish()
            restored = factory.restore(published.manifest_id)
            self.assertEqual(restored.runner.trainer.objective_config.policy_update.value_prediction_target,
                             "terminal_score")
            self.assertTrue(session.advance_generation(max_batch_steps=512).promoted)
            self.assertTrue(restored.advance_generation(max_batch_steps=512).promoted)
            self.assertEqual(session.active_behavior_manifest_id, restored.active_behavior_manifest_id)

    def test_legacy_episode_mean_ppo_session_requires_a_new_training_identity(self):
        with tempfile.TemporaryDirectory() as temp:
            spec = StrategicStudySpec(model_seeds=(7,), ascension=0, final_act=False,
                hidden_dim=8, strategic_navigation=True, canonical_behavior_rows=True,
                executor=FixedCombatConfig(generation_work=1, wall_ms=1,
                    atomic_nodes=None, fallback_action_limit=512))
            runtime = InstalledFixedCombatRuntime(spec.executor, final_act=False, strategic_navigation=True)
            with patch("sts_learning.torch_provenance._RUN_VALUE_PPO_TRAINER_IMPLEMENTATION_VERSION", 4), \
                 patch("sts_learning.torch_provenance._RUN_VALUE_PPO_OBJECTIVE_VERSION", 3):
                old_factory = CategoricalOnlineSessionFactory(Path(temp)/"session", runtime.session_bridge(),
                    _session_config(spec), NoRecoveryCurriculum())
                published = old_factory.new(model_seed=7, behavior_seed=11).publish()
            factory = CategoricalOnlineSessionFactory(Path(temp)/"session", runtime.session_bridge(),
                _session_config(spec), NoRecoveryCurriculum())
            with self.assertRaisesRegex(TorchResumeRestoreError, "runtime provenance"):
                factory.restore(published.manifest_id)

    def test_training_failure_preserves_last_adopted_model_without_claiming_resume(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"interrupted"
            spec = StrategicStudySpec(model_seeds=(7,), generations=2,
                episodes_per_generation=1, slots=1, evaluation_seeds=1,
                evaluation_replicas=1, hidden_dim=8, ascension=0, final_act=False,
                executor=FixedCombatConfig(generation_work=1, wall_ms=1,
                    atomic_nodes=None, fallback_action_limit=512))
            original = CategoricalOnlineSession.advance_generation
            calls = 0
            def fail_second(session, **kwargs):
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise RuntimeError("collection failed")
                return original(session, **kwargs)
            with patch.object(CategoricalOnlineSession, "advance_generation", fail_second):
                with self.assertRaisesRegex(RuntimeError, "collection failed"):
                    run_study(root, spec, progress=lambda _: None)
            saved = json.loads((root/"model-7"/"interrupted-model.json").read_text())
            update = json.loads((root/"model-7"/"updates.json").read_text())[0]
            self.assertEqual(saved["manifest"], update["progress"]["promotion"]["manifest_id"])
            self.assertEqual(saved["completed_generations"], 1)
            self.assertFalse(saved["resumable_session"])
            self.assertFalse((root/"summary.json").exists())

    def test_time_budget_keeps_completed_update_and_finishes_all_initializations(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"budget"
            spec = StrategicStudySpec(model_seeds=(7, 8), generations=2,
                episodes_per_generation=1, slots=1, evaluation_seeds=1,
                evaluation_replicas=1, hidden_dim=8, training_seconds_per_model=1,
                evaluation_generations=(1,), ascension=0, final_act=False, strategic_navigation=True,
                executor=FixedCombatConfig(generation_work=1, wall_ms=1,
                    atomic_nodes=None, fallback_action_limit=512))
            ticks = itertools.count(step=10)
            with patch("sts_learning.strategic_study.time",
                       SimpleNamespace(perf_counter=lambda: next(ticks))):
                report = run_study(root, spec, progress=lambda _: None)
            self.assertEqual(len(report["runs"]), 2)
            for run in report["runs"]:
                self.assertEqual(run["training_stop"], "time_budget")
                self.assertEqual(run["training_episodes"], 1)
                self.assertEqual(run["planned_training_episodes"], 2)
                self.assertEqual([point["generation"] for point in run["learning_curve"]], [0, 1])
                model_root = root/f"model-{run['model_seed']}"
                models = json.loads((model_root/"models.json").read_text())
                self.assertEqual(models["evaluation_checkpoints"][-1]["resume_manifest"], models["final_resume"])
                runtime = InstalledFixedCombatRuntime(spec.executor, final_act=spec.final_act,
                    strategic_navigation=spec.strategic_navigation)
                factory = CategoricalOnlineSessionFactory(model_root/"session", runtime.session_bridge(),
                    _session_config(spec), NoRecoveryCurriculum())
                restored = factory.restore(ResumeManifestId(bytes.fromhex(models["final_resume"]["digest"])))
                self.assertEqual(restored.active_behavior_manifest_id.digest.hex(), models["final_manifest"]["digest"])
            self.assertTrue(verify_study(root)["study_complete"])

    def test_new_protocol_defaults_to_heart_and_stops_on_search_miss(self):
        spec = StrategicStudySpec()
        self.assertTrue(spec.final_act)
        self.assertEqual(spec.executor.backend, "whole_combat")
        self.assertIsNone(spec.executor.fallback_action_limit)
        self.assertIsNone(spec.executor.atomic_nodes)

    def test_executor_receipt_counts_wall_limited_combats(self):
        with tempfile.TemporaryDirectory() as temp:
            evidence = _Evidence(Path(temp))
            evidence.stage = "probe"
            combat = {"selected_candidate_ordinal": None, "fallback": {},
                      "exit": {"boundary_kind": "terminal", "hp": 0},
                      "elapsed_ms": 17,
                      "attempt": {"lanes": [{"exit": "wall_time_exhausted", "work_consumed": 3}]}}
            evidence(FixedCombatStep({}, {"combats": [combat]}, None))
            evidence(FixedCombatStep(None, {"combats": [combat]}, 0))
            counts = evidence.counts["probe"]
            self.assertEqual(counts["attempted_wall_limited_combats"], 2)
            self.assertEqual(counts["committed_wall_limited_combats"], 1)
            self.assertEqual(counts["attempted_fallback_terminal_zero_hp"], 2)
            self.assertEqual(counts["committed_fallback_terminal_zero_hp"], 1)
            self.assertEqual(counts["attempted_combat_elapsed_ms"], 34)
            self.assertEqual(counts["committed_combat_elapsed_ms"], 17)
            self.assertEqual(counts["attempted_generation_work"], 6)
            self.assertEqual(counts["committed_generation_work"], 3)

    def test_rollout_preset_keeps_actual_fallback_and_separate_rule_comparison(self):
        spec = StrategicStudySpec.a0_rollout()
        self.assertEqual(spec.ascension, 0)
        self.assertFalse(spec.final_act)
        self.assertEqual(spec.reward_metric, "floor_progress")
        self.assertTrue(spec.include_rule_baseline)
        self.assertTrue(spec.compact_executor_evidence)
        self.assertGreater(spec.executor.fallback_action_limit, 0)
        self.assertEqual(spec, StrategicStudySpec.from_dict(json.loads(
            json.dumps(replace(spec).__dict__, default=lambda value: value.__dict__))))

    def test_compact_receipt_retains_outcome_costs_and_truncation_without_plan_dump(self):
        with tempfile.TemporaryDirectory() as temp:
            evidence = _Evidence(Path(temp), compact=True)
            combat = {"selected_candidate_ordinal": None, "elapsed_ms": 3,
                      "exit": None, "fallback": {"baseline_revision": 29,
                      "steps": ["private trace"], "action_limit_reached": True},
                      "attempt": {"candidates": ["large plan"], "lanes": []}}
            evidence(FixedCombatStep(None, {"combats": [combat]}, 0))
            row = json.loads((Path(temp)/"executor-evidence.jsonl").read_text())
            self.assertFalse(row["committed"])
            self.assertEqual(row["evidence_mode"], "summary")
            receipt = row["evidence"]["combats"][0]
            self.assertEqual(receipt["candidate_count"], 1)
            self.assertTrue(receipt["fallback"]["action_limit_reached"])
            self.assertEqual(receipt["fallback"]["action_count"], 1)
            self.assertIsNone(receipt["exit"])
            self.assertNotIn("attempt", receipt)
            self.assertNotIn("private trace", json.dumps(row))

    def test_search_interruption_is_not_a_terminal_training_label(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"unresolved"
            spec = StrategicStudySpec(model_seeds=(7,), generations=1,
                episodes_per_generation=1, slots=1, evaluation_seeds=1,
                evaluation_replicas=1, hidden_dim=8, max_steps=30,
                executor=FixedCombatConfig(backend="whole_combat", generation_work=1,
                    wall_ms=1, atomic_nodes=None, fallback_action_limit=None))
            with self.assertRaises(FixedCombatUnresolved):
                run_study(root, spec, progress=lambda _: None)
            rows = json.loads((root/"uniform-panel.json").read_text())
            self.assertIsNone(rows[0]["outcome"])
            self.assertEqual(rows[0]["censored_reason"], "executor_unresolved_batch")
            self.assertFalse((root/"summary.json").exists())
            failure = json.loads((root/"failure.json").read_text())
            self.assertEqual(failure["type"], "FixedCombatUnresolved")

    def test_small_real_study_uses_restore_and_seals_all_arms(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"study"
            spec = StrategicStudySpec(model_seeds=(7,),generations=1,episodes_per_generation=1,
                slots=1,evaluation_seeds=1,evaluation_replicas=1,hidden_dim=8,
                ascension=0, include_rule_baseline=True, compact_executor_evidence=True,
                executor=FixedCombatConfig(generation_work=1,wall_ms=1,atomic_nodes=None,fallback_action_limit=512))
            report = run_study(root,spec,progress=lambda _:None)
            self.assertEqual(report["status"],"complete")
            self.assertEqual(report["uniform"]["episodes"],1)
            self.assertEqual(report["rules"]["episodes"], 1)
            self.assertEqual(report["rules"]["censored"], 0)
            self.assertEqual(report["runs"][0]["final"]["episodes"],1)
            self.assertIn("completed_by_fallback_death", report["runs"][0]["final"])
            self.assertIn("fallback_free_paired_mean_delta", report["runs"][0])
            uniform_rows = json.loads((root/"uniform-panel.json").read_text())
            self.assertEqual(sum(row["fallback_terminal_zero_hp"] is True for row in uniform_rows),
                             report["uniform"]["completed_by_fallback_death"])
            self.assertEqual(report["uniform"]["completed_by_fallback_death"],
                             report["executor_counts"]["uniform"]["committed_fallback_terminal_zero_hp"])
            evidence = [json.loads(line) for line in (root/"strategic-evidence.jsonl").read_text().splitlines()]
            self.assertTrue(any(row["kind"] == "strategic_choice" for row in evidence))
            self.assertTrue(any(row["kind"] == "resource_step" for row in evidence))
            map_choices = [row for row in evidence if row["kind"] == "strategic_choice"
                           and row["site"] == "map"]
            self.assertTrue(map_choices)
            self.assertTrue(all(any(option["candidate"]["kind"] == "choose_route_node"
                                    and option["route_summary"] is not None
                                    for option in row["available_candidates"])
                                for row in map_choices))
            self.assertTrue(all(abs(sum(option["probability"] for option in row["available_candidates"])-1.0) < 1e-9
                                for row in map_choices))
            self.assertIn("executor_counts", report)
            self.assertGreater(report["runs"][0]["training_episodes_per_second"], 0)
            self.assertEqual(report["runs"][0]["training_episodes"], 1)
            self.assertIn("vs_rules_mean_delta", report["runs"][0]["learning_curve"][0])
            self.assertIn("committed_wall_limited_combats", report["executor_counts"]["uniform"])
            self.assertIn("committed_fallback_terminal_zero_hp", report["executor_counts"]["uniform"])
            self.assertIn(json.loads((root/"contract.json").read_text())["native_build_profile"],
                          {"dev", "release"})
            contract = json.loads((root/"contract.json").read_text())
            self.assertEqual(contract["goal"], "heart")
            self.assertTrue(contract["weak_combat_fallback"])
            self.assertTrue(verify_study(root)["study_complete"])
            with self.assertRaises(FileExistsError):
                run_study(root,spec,progress=lambda _:None)
            with (root/"contract.json").open("a") as f:
                f.write(" ")
            with self.assertRaisesRegex(ValueError,"artifact mismatch"):
                verify_study(root)

    def test_protocol_rejects_invalid_or_unknown_configuration(self):
        for value in ({"generations":0},{"model_seeds":[7,7]},{"learning_rate":float("nan")},{"ignored_typo":1},
                      {"evaluation_generations":[0]}, {"evaluation_generations":[8]},
                      {"evaluation_generations":[2,1]}, {"evaluation_generations":[1,1]},
                      {"evaluation_generations":[True]}, {"evaluation_generations":[1.0]},
                      {"include_rule_baseline": 1}, {"compact_executor_evidence": "yes"},
                      {"final_evaluation_seed_start": True}, {"final_evaluation_seed_start": -1},
                      {"final_evaluation_seeds": 0}, {"evidence_max_bytes": 0},
                      {"final_act": 1}):
            with self.assertRaises((ValueError,TypeError)):
                StrategicStudySpec.from_dict(value)

    def test_curve_reserves_capacity_for_every_durable_evaluation_model(self):
        spec = StrategicStudySpec(generations=32, evaluation_generations=tuple(range(1, 32)))
        self.assertGreaterEqual(_session_config(spec).limits.owner_capacity, 33)
        self.assertEqual(_session_config(replace(spec, evaluation_generations=(8,))).limits.owner_capacity, 16)

    def test_rollout_storage_scales_with_the_declared_episode_batch(self):
        config = _session_config(StrategicStudySpec.a0_rollout())
        self.assertEqual(config.limits.attempt_updates.max_payload_bytes_per_update, 256*1024*1024)
        self.assertEqual(config.limits.concat.max_input_array_bytes, 256*1024*1024)

    def test_curve_recovers_exact_milestone_without_changing_training(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/"curve"
            spec = StrategicStudySpec(model_seeds=(7,), generations=2, episodes_per_generation=1,
                slots=1, evaluation_seeds=1, evaluation_replicas=1, hidden_dim=8,
                evaluation_generations=(1,),
                executor=FixedCombatConfig(generation_work=1, wall_ms=1, atomic_nodes=None, fallback_action_limit=512))
            report = run_study(root, spec, progress=lambda _:None)
            model_root = root/"model-7"
            models = json.loads((model_root/"models.json").read_text())
            updates = json.loads((model_root/"updates.json").read_text())
            curve = json.loads((model_root/"learning-curve.json").read_text())
            self.assertEqual(len(report["runs"][0]["learning_curve"]), 3)
            self.assertEqual([point["generation"] for point in curve], [0, 1, 2])
            self.assertEqual([point["training_episodes"] for point in curve], [0, 1, 2])
            self.assertEqual(curve[1]["manifest"]["digest"],
                             updates[0]["progress"]["promotion"]["manifest_id"]["digest"])
            self.assertEqual(models["evaluation_checkpoints"][1]["manifest"]["digest"],
                             curve[1]["manifest"]["digest"])
            self.assertTrue((model_root/"generation-1-panel.json").is_file())
            self.assertTrue(verify_study(root)["study_complete"])
            control = Path(temp)/"control"
            run_study(control, replace(spec, evaluation_generations=()), progress=lambda _:None)
            control_models = json.loads((control/"model-7"/"models.json").read_text())
            self.assertEqual(models["final_manifest"], control_models["final_manifest"])
