"""Reproducible fresh strategic-policy studies over a fixed combat executor."""
from __future__ import annotations

import argparse
import dataclasses
import enum
import hashlib
import json
import math
import time
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import torch

from .fixed_combat import FixedCombatConfig
from .fixed_combat_environment import InstalledFixedCombatRuntime, canonical_json
from .fixed_seed_evaluation import FixedSeedPanel, evaluate_fixed_seed_panel, panel_summary, paired_seed_floor_deltas
from .policy import BatchPolicyChoice, BehaviorManifestId
from .seeds import SeedPartition, SeedSchedule
from .terminal_returns import FloorProgressReturnConfig, OnPolicyObjectiveConfig, RunDecisionScope, RunPolicyUpdateConfig, TerminalAdvantageMode
from .torch_behavior import FrozenReplicateCategoricalTorchPolicy
from .torch_policy import RaggedCandidateLogits, RaggedCandidateScorer, RaggedCategoricalPolicyConfig, RaggedScorerConfig, sample_ragged_categorical_rows
from .torch_provenance import AdamTrainingConfig
from .torch_session import CategoricalOnlineSessionFactory, NoRecoveryCurriculum
from .torch_session_config import CategoricalOnlineProfile, CategoricalOnlineSessionConfig, CategoricalSessionLimits
from .strategic_rollout import StrategicRunController
from .run_resource_trace import RunPublicContext
from .seeds import SeedPartitionSpec
from .strategic_demonstrations import collect_fixed_rule_demonstrations
from .strategic_behavior_cloning import (StrategicBehaviorCloningConfig,
    StrategicDemonstrationSplit, train_strategic_behavior_clone)


@dataclass(frozen=True)
class RuleInitializationSpec:
    training_runs: int = 8
    validation_runs: int = 4
    epochs: int = 32
    learning_rate: float = 0.001
    batch_rows: int = 16
    max_rows: int = 8192
    max_array_bytes: int = 256*1024*1024

    def __post_init__(self):
        for name in ("training_runs", "validation_runs", "epochs", "batch_rows", "max_rows", "max_array_bytes"):
            if type(getattr(self, name)) is not int or getattr(self, name) <= 0:
                raise ValueError(f"{name} must be a positive integer")
        StrategicBehaviorCloningConfig(epochs=self.epochs, learning_rate=self.learning_rate,
                                      batch_rows=self.batch_rows)


@dataclass(frozen=True)
class StrategicStudySpec:
    model_seeds: tuple[int, ...] = (1001, 1002, 1003)
    generations: int = 8
    episodes_per_generation: int = 8
    slots: int = 4
    ascension: int = 20
    final_act: bool = True
    training_seed_start: int = 2026094000
    evaluation_seed_start: int = 2026094000
    evaluation_seeds: int = 16
    evaluation_replicas: int = 2
    evaluation_generations: tuple[int, ...] = ()
    final_evaluation_seed_start: int | None = None
    final_evaluation_seeds: int = 64
    evidence_max_bytes: int = 256*1024*1024
    max_steps: int = 512
    training_seconds_per_model: int = 300
    hidden_dim: int = 32
    learning_rate: float = 0.0003
    reward_metric: str = "act_completion"
    value_prediction_target: str = "return_to_go"
    cpu_threads: int = 2
    include_rule_baseline: bool = False
    compact_executor_evidence: bool = False
    strategic_navigation: bool = False
    canonical_behavior_rows: bool = False
    rule_initialization: RuleInitializationSpec | None = None
    executor: FixedCombatConfig = field(default_factory=lambda: FixedCombatConfig(
        backend="whole_combat", generation_work=100_000, wall_ms=3_000,
        atomic_nodes=None, fallback_action_limit=None))

    def __post_init__(self):
        for name in ("include_rule_baseline", "compact_executor_evidence", "strategic_navigation", "canonical_behavior_rows"):
            if type(getattr(self, name)) is not bool:
                raise ValueError(f"{name} must be boolean")
        if self.reward_metric not in ("floor_progress", "act_completion"):
            raise ValueError("unknown strategic reward metric")
        if self.value_prediction_target not in ("return_to_go", "terminal_score"):
            raise ValueError("unknown strategic value prediction target")
        object.__setattr__(self, "model_seeds", tuple(self.model_seeds))
        if not self.model_seeds or len(set(self.model_seeds)) != len(self.model_seeds):
            raise ValueError("model seeds must be nonempty and unique")
        for value in (*self.model_seeds, self.training_seed_start, self.evaluation_seed_start):
            if type(value) is not int or not 0 <= value < 2**63:
                raise ValueError("study seed must be an integer in 0..2^63")
        if self.final_evaluation_seed_start is not None and (
                type(self.final_evaluation_seed_start) is not int
                or not 0 <= self.final_evaluation_seed_start < 2**63):
            raise ValueError("final evaluation seed must be an integer in 0..2^63")
        for name in ("generations", "episodes_per_generation", "slots", "evaluation_seeds",
                     "evaluation_replicas", "final_evaluation_seeds", "evidence_max_bytes",
                     "max_steps", "training_seconds_per_model", "hidden_dim", "cpu_threads"):
            if type(getattr(self, name)) is not int or getattr(self, name) <= 0:
                raise ValueError(f"{name} must be a positive integer")
        milestones = tuple(self.evaluation_generations)
        if (any(type(value) is not int or not 0 < value < self.generations for value in milestones)
                or milestones != tuple(sorted(set(milestones)))):
            raise ValueError("evaluation generations must be increasing unique interior generations")
        object.__setattr__(self, "evaluation_generations", milestones)
        if type(self.ascension) is not int or not 0 <= self.ascension <= 20:
            raise ValueError("ascension must be in 0..20")
        if type(self.final_act) is not bool:
            raise ValueError("final_act must be boolean")
        if type(self.learning_rate) not in (float, int) or not math.isfinite(self.learning_rate) or self.learning_rate <= 0:
            raise ValueError("learning rate must be finite and positive")
        if not isinstance(self.executor, FixedCombatConfig):
            raise ValueError("executor must be a FixedCombatConfig")
        if self.rule_initialization is not None:
            if not isinstance(self.rule_initialization, RuleInitializationSpec):
                raise ValueError("rule_initialization must be a RuleInitializationSpec")
            if not self.strategic_navigation:
                raise ValueError("rule initialization requires strategic navigation")
            if self.rule_initialization.validation_runs > self.evaluation_seeds:
                raise ValueError("rule validation runs exceed the held-out panel")

    @classmethod
    def from_dict(cls, value):
        value = dict(value)
        if "executor" in value:
            value["executor"] = FixedCombatConfig(**value["executor"])
        if value.get("rule_initialization") is not None:
            value["rule_initialization"] = RuleInitializationSpec(**value["rule_initialization"])
        return cls(**value)

    @classmethod
    def a0_rollout(cls):
        """Bounded whole-run learning probe with actual combat continuation."""
        return cls(model_seeds=(101, 102), generations=16,
                   episodes_per_generation=16, slots=4, ascension=0,
                   final_act=False, evaluation_seeds=16, evaluation_replicas=1,
                   evaluation_generations=(8,), max_steps=1024,
                   training_seconds_per_model=600, reward_metric="floor_progress",
                   include_rule_baseline=True, compact_executor_evidence=True, strategic_navigation=True,
                   executor=FixedCombatConfig(backend="whole_combat",
                       generation_work=10_000, wall_ms=1000, atomic_nodes=None,
                       fallback_action_limit=4096))


def _plain(value):
    if dataclasses.is_dataclass(value):
        return {k: _plain(v) for k, v in dataclasses.asdict(value).items()}
    if isinstance(value, enum.Enum):
        return value.value
    if isinstance(value, dict):
        return {str(k): _plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    if isinstance(value, np.ndarray):
        return value.tolist()
    if isinstance(value, bytes):
        return value.hex()
    if isinstance(value, Path):
        return str(value)
    return value


def _write(path, value):
    path.write_text(json.dumps(_plain(value), indent=2, allow_nan=False)+"\n", encoding="utf-8")


def _digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def _streams(seeds, replica):
    # Seed and replica own the stream, independent of cohort position, policy,
    # training initialization and completion order of neighboring episodes.
    return [torch.Generator().manual_seed(int.from_bytes(hashlib.sha256(
        f"strategic-study-eval-v1:{seed}:{replica}".encode()).digest()[:8], "little") % 2**63)
        for seed in seeds]


class _UniformPolicy:
    behavior_manifest_id = BehaviorManifestId(hashlib.sha256(b"strategic-uniform-legal-v1").digest())

    def __init__(self, generators):
        self.generators = generators

    def candidate_probability_rows(self, batch):
        splits = tuple(int(value) for value in batch["candidate_row_splits"])
        return tuple(tuple(1.0/(end-start) for _ in range(start, end))
                     for start, end in zip(splits[:-1], splits[1:], strict=True))

    def choose(self, batch):
        splits = torch.as_tensor(batch["candidate_row_splits"], dtype=torch.long)
        logits = RaggedCandidateLogits(torch.zeros(int(splits[-1])), splits)
        sample = sample_ragged_categorical_rows(logits, RaggedCategoricalPolicyConfig(),
            tuple(self.generators[int(slot)] for slot in batch["slot_indices"]))
        return BatchPolicyChoice.create(sample.ordinals, self.behavior_manifest_id, sample.selection_probabilities)


class _Evidence:
    def __init__(self, root, *, compact=False, max_bytes=256*1024*1024):
        self.root = root
        self.compact = compact
        self.max_bytes = max_bytes
        self.stage = "setup"
        self.counts = {}
        self.bytes = 0
        self.decision_bytes = 0

    def failure(self, environment, error):
        """Keep the unchanged ready source so a native failure is reproducible."""
        payload = environment.checkpoint_bytes(max_bytes=32*1024*1024)
        (self.root/"executor-failure.checkpoint").write_bytes(payload)
        _write(self.root/"executor-failure.json", {
            "stage": self.stage, "type": type(error).__name__, "message": str(error),
            "executor_id": environment.binding.identity,
            "checkpoint_sha256": hashlib.sha256(payload).hexdigest(),
            "public_contexts": [RunPublicContext.from_bridge_row(row)
                                for row in environment.public_run_contexts()],
        })

    def __call__(self, result):
        combats = result.evidence["combats"]
        count = self.counts.setdefault(self.stage, {"attempted_witnesses": 0, "attempted_fallbacks": 0,
                                                  "committed_witnesses": 0, "committed_fallbacks": 0,
                                                  "attempted_fallback_terminal_zero_hp": 0,
                                                  "committed_fallback_terminal_zero_hp": 0,
                                                  "attempted_wall_limited_combats": 0,
                                                  "committed_wall_limited_combats": 0,
                                                  "attempted_combat_elapsed_ms": 0,
                                                  "committed_combat_elapsed_ms": 0,
                                                  "attempted_generation_work": 0,
                                                  "committed_generation_work": 0,
                                                  "unresolved_batches": 0})
        count["unresolved_batches"] += not result.advanced
        wall_limited = sum(any(lane["exit"] in {"wall_time_exhausted", "deadline_reached", "witness_at_wall_limit"}
                               for lane in combat["attempt"]["lanes"]) for combat in combats)
        count["attempted_wall_limited_combats"] += wall_limited
        count["committed_wall_limited_combats"] += wall_limited if result.advanced else 0
        for key, total in (("witnesses",sum(c["selected_candidate_ordinal"] is not None for c in combats)),
                           ("fallbacks",sum(c["fallback"] is not None for c in combats))):
            count["attempted_"+key] += total
            count["committed_"+key] += total if result.advanced else 0
        fallback_terminal_zero_hp = sum(
            combat["fallback"] is not None
            and combat.get("exit") is not None
            and combat["exit"]["boundary_kind"] == "terminal"
            and combat["exit"]["hp"] == 0
            for combat in combats)
        count["attempted_fallback_terminal_zero_hp"] += fallback_terminal_zero_hp
        count["committed_fallback_terminal_zero_hp"] += fallback_terminal_zero_hp if result.advanced else 0
        elapsed_ms = sum(combat.get("elapsed_ms", 0) for combat in combats)
        work = sum(lane.get("work_consumed", 0) for combat in combats
                   for lane in combat["attempt"]["lanes"])
        count["attempted_combat_elapsed_ms"] += elapsed_ms
        count["committed_combat_elapsed_ms"] += elapsed_ms if result.advanced else 0
        count["attempted_generation_work"] += work
        count["committed_generation_work"] += work if result.advanced else 0
        if combats:
            evidence = result.evidence
            if self.compact:
                evidence = {**evidence, "combats": [{
                    **{key: value for key, value in combat.items()
                       if key not in {"attempt", "fallback"}},
                    "candidate_count": len(combat["attempt"].get("candidates", ())),
                    "search_lanes": combat["attempt"]["lanes"],
                    "fallback": None if combat["fallback"] is None else {
                        **{key: value for key, value in combat["fallback"].items()
                           if key != "steps"},
                        "action_count": len(combat["fallback"].get("steps", ()))},
                } for combat in combats]}
            row = canonical_json({"stage": self.stage, "committed": result.advanced,
                                  "evidence_mode": "summary" if self.compact else "full",
                                  "evidence": evidence})+"\n"
            self.bytes += len(row.encode())
            if self.bytes > self.max_bytes:
                raise RuntimeError(f"study executor evidence exceeded {self.max_bytes} bytes")
            with (self.root/"executor-evidence.jsonl").open("a", encoding="utf-8") as destination:
                destination.write(row)

    def decision(self, event):
        row = canonical_json({"stage": self.stage, **event}) + "\n"
        self.decision_bytes += len(row.encode())
        if self.decision_bytes > self.max_bytes:
            raise RuntimeError(f"study strategic evidence exceeded {self.max_bytes} bytes")
        with (self.root/"strategic-evidence.jsonl").open("a", encoding="utf-8") as destination:
            destination.write(row)


def _session_config(spec):
    limits = CategoricalSessionLimits(owner_capacity=max(16, len(spec.evaluation_generations) + 2))
    # A larger episode batch needs a correspondingly larger retained trajectory
    # budget. Both collection and tensor concatenation use this declared bound.
    update_bytes = max(limits.attempt_updates.max_payload_bytes_per_update,
                       spec.episodes_per_generation*limits.experience.max_payload_bytes)
    limits = dataclasses.replace(limits,
        attempt_updates=dataclasses.replace(limits.attempt_updates,
            max_payload_bytes_per_update=update_bytes),
        concat=dataclasses.replace(limits.concat, max_input_array_bytes=update_bytes))
    return CategoricalOnlineSessionConfig(
        ascension_level=spec.ascension, slot_count=spec.slots,
        # Live promotions replace the head; checkpoints and interruption diagnostics are durable.
        limits=limits,
        schedule=SeedSchedule(SeedPartition.TRAINING, next_candidate=spec.training_seed_start),
        profile=CategoricalOnlineProfile(
            behavior=RaggedCategoricalPolicyConfig(canonical_row_scoring=spec.canonical_behavior_rows),
            scorer=RaggedScorerConfig(hidden_dim=spec.hidden_dim, relation_layers=1, value_head=True),
            optimizer=AdamTrainingConfig(learning_rate=spec.learning_rate),
            objective=OnPolicyObjectiveConfig(
                terminal_return=FloorProgressReturnConfig(target_floor=54, metric=spec.reward_metric),
                attempts_per_update=spec.episodes_per_generation,
                advantage_mode=TerminalAdvantageMode.DECISION_LOCAL_GAE,
                decision_scope=RunDecisionScope.STRATEGIC,
                policy_update=dataclasses.replace(RunPolicyUpdateConfig.ppo_clip_value(),
                    value_prediction_target=spec.value_prediction_target))))


def _evaluation_panels(spec):
    def panel(start, count):
        planned, _ = SeedSchedule(SeedPartition.HELD_OUT, next_candidate=start).plan(range(count))
        return FixedSeedPanel(planned.seeds, replicas=spec.evaluation_replicas,
                              slots=spec.slots, max_steps=spec.max_steps)
    development = panel(spec.evaluation_seed_start, spec.evaluation_seeds)
    final = (None if spec.final_evaluation_seed_start is None else
             panel(spec.final_evaluation_seed_start, spec.final_evaluation_seeds))
    if final is not None and set(development.seeds) & set(final.seeds):
        raise ValueError("development and final evaluation seeds overlap")
    return development, final


def _frozen_policy_factory(factory, identity):
    def policy_factory(seeds, replica):
        streams = _streams(seeds, replica)
        policy = factory.recover_behavior(identity, behavior_seed=streams[0].initial_seed())
        streams[0] = policy.generator
        return FrozenReplicateCategoricalTorchPolicy.from_categorical(policy, streams)
    return policy_factory


def _final_evaluation(output, spec, panel, models, progress):
    """Evaluate every initial/final pair once, after all training has stopped."""
    root = output/"final-test"
    root.mkdir()
    evidence = _Evidence(root, compact=spec.compact_executor_evidence, max_bytes=spec.evidence_max_bytes)
    runtime = InstalledFixedCombatRuntime(spec.executor, evidence, final_act=spec.final_act,
                                          strategic_navigation=spec.strategic_navigation)
    environment = lambda seeds: runtime.environment(seeds, spec.ascension)
    started = time.perf_counter()
    try:
        rules = None
        if spec.include_rule_baseline:
            evidence.stage = "rules"
            rules = evaluate_fixed_seed_panel(environment, None, panel,
                lambda rows: _write(root/"rules-panel.json", rows), evidence.decision,
                controller_factory=StrategicRunController)
            progress(f"Final test rules: {panel_summary(rules)}")
        evidence.stage = "uniform"
        uniform = evaluate_fixed_seed_panel(environment,
            lambda seeds, replica: _UniformPolicy(_streams(seeds, replica)), panel,
            lambda rows: _write(root/"uniform-panel.json", rows), evidence.decision)
        progress(f"Final test uniform: {panel_summary(uniform)}")
        runs = []
        for model_seed, initial, final in models:
            factory = CategoricalOnlineSessionFactory(output/f"model-{model_seed}"/"session",
                runtime.session_bridge(), _session_config(spec), NoRecoveryCurriculum())
            panels = {}
            for name, identity in (("initial", initial), ("final", final)):
                evidence.stage = f"model-{model_seed}/{name}"
                panels[name] = evaluate_fixed_seed_panel(environment,
                    _frozen_policy_factory(factory, identity), panel,
                    lambda rows: _write(root/f"model-{model_seed}-{name}-panel.json", rows), evidence.decision)
                progress(f"Final test model-{model_seed} {name}: {panel_summary(panels[name])}")
            deltas = paired_seed_floor_deltas(panels["initial"], panels["final"])
            rule_deltas = paired_seed_floor_deltas(rules, panels["final"]) if rules is not None else None
            runs.append({"model_seed": model_seed,
                "initial_manifest": initial, "final_manifest": final,
                "initial": panel_summary(panels["initial"]), "final": panel_summary(panels["final"]),
                "paired_seed_floor_deltas": deltas,
                "paired_mean_delta": sum(deltas.values())/len(deltas) if deltas is not None else None,
                "vs_rules_mean_delta": sum(rule_deltas.values())/len(rule_deltas) if rule_deltas is not None else None})
            _write(root/"runs.json", runs)
        summary = {"status": "complete", "panel": panel,
            "rules": panel_summary(rules) if rules is not None else None,
            "uniform": panel_summary(uniform), "runs": runs,
            "elapsed_seconds": time.perf_counter()-started,
            "selection": "all declared initial/final models; no checkpoint selection or further updates"}
        _write(root/"summary.json", summary)
        return summary
    except Exception as error:
        _write(root/"failure.json", {"stage": evidence.stage, "type": type(error).__name__, "message": str(error)})
        raise
    finally:
        _write(root/"executor-counts.json", evidence.counts)


def run_study(output: Path, spec: StrategicStudySpec, progress=print):
    """Freeze protocol first, then run every declared model; no best-model pick."""
    output = Path(output).resolve()
    torch.set_num_threads(spec.cpu_threads)
    panel, final_panel = _evaluation_panels(spec)
    evidence = _Evidence(output, compact=spec.compact_executor_evidence, max_bytes=spec.evidence_max_bytes)
    runtime = InstalledFixedCombatRuntime(spec.executor, evidence, final_act=spec.final_act,
                                          strategic_navigation=spec.strategic_navigation)
    config = _session_config(spec)
    sources = {str(p.relative_to(Path(__file__).parent)): _digest(p)
               for p in sorted(Path(__file__).parent.rglob("*.py"))}
    # Nothing executes until the immutable planned protocol has been written.
    output.mkdir(parents=True, exist_ok=False)
    contract = {"schema": 2, "spec": _plain(spec), "session_config": _plain(config), "panel": _plain(panel),
                "final_panel": _plain(final_panel),
                "final_evaluation": "after all training; initial/final pairs for every initialization, uniform and enabled rules; no selection" if final_panel else None,
                "executor_binding": _plain(runtime.binding), "executor_id": runtime.binding.identity,
                "native_build_profile": runtime.native.NATIVE_BUILD_PROFILE,
                "learning_source_sha256": sources, "torch_version": torch.__version__,
                "goal": "heart" if spec.final_act else "act_three",
                "information": "public strategic model with oracle combat search",
                "weak_combat_fallback": spec.executor.fallback_action_limit is not None,
                "executor_evidence": "summary" if spec.compact_executor_evidence else "full",
                "rule_baseline": runtime.baseline_manifest if spec.include_rule_baseline else None,
                "initialization": "random" if spec.rule_initialization is None else {
                    "source": "committed public-rule trajectories, including losing runs",
                    "routine_rows": "excluded using the PPO navigation owner",
                    "symbolic_choices": "included with their actual selected ordinal",
                    "validation": "first declared held-out seeds; diagnostics only, fixed epochs",
                    "ppo": "fresh optimizer and critic; cloned actor weights; no demonstration PPO samples",
                    "control": "same model seed, random network with zero PPO updates"},
                "evaluation": "sampled temperature 1, fixed seed/replica panel, all initializations retained",
                "objective": f"decision-local {spec.reward_metric} return; gamma = lambda = 1",
                "evaluation_generations": [0, *spec.evaluation_generations, spec.generations],
                "checkpoint_selection": "predeclared generations and last completed update at the time budget; evaluated only after training stops",
                "recovery": "none"}
    _write(output/"contract.json", contract)
    started = time.perf_counter()
    runs = []
    final_models = []
    session = None
    rules = None
    rules_seconds = None
    try:
        demonstrations = None
        initialization = spec.rule_initialization
        if initialization is not None:
            training_plan, _ = SeedSchedule(SeedPartition.TRAINING,
                next_candidate=spec.training_seed_start).plan(range(initialization.training_runs))
            partitions = {}
            for name, seeds, partition in (
                    ("training", training_plan.seeds, SeedPartition.TRAINING),
                    ("validation", panel.seeds[:initialization.validation_runs], SeedPartition.HELD_OUT)):
                evidence.stage = "demonstrations/"+name
                partitions[name], _ = collect_fixed_rule_demonstrations(runtime, seeds,
                    output/("demonstrations-"+name), partition=partition, ascension=spec.ascension,
                    slots=spec.slots, max_steps=spec.max_steps, max_rows=initialization.max_rows,
                    max_array_bytes=initialization.max_array_bytes, progress=progress)
            demonstrations = StrategicDemonstrationSplit(
                partitions["training"], partitions["validation"], SeedPartitionSpec())
        if spec.include_rule_baseline:
            evidence.stage = "rules"
            rules_started = time.perf_counter()
            rules = evaluate_fixed_seed_panel(lambda seeds: runtime.environment(seeds, spec.ascension),
                None, panel,
                lambda rows: _write(output/"rules-panel.json", rows), evidence.decision,
                controller_factory=StrategicRunController)
            rules_seconds = time.perf_counter()-rules_started
            progress(f"Rule baseline ({rules_seconds:.1f}s): {panel_summary(rules)}")
        evidence.stage = "uniform"
        uniform_started = time.perf_counter()
        baseline = evaluate_fixed_seed_panel(lambda seeds: runtime.environment(seeds, spec.ascension),
            lambda seeds, replica: _UniformPolicy(_streams(seeds, replica)), panel,
            lambda rows: _write(output/"uniform-panel.json", rows), evidence.decision)
        progress(f"Uniform baseline: {panel_summary(baseline)}")
        uniform_seconds = time.perf_counter()-uniform_started
        for model_seed in spec.model_seeds:
            label = f"model-{model_seed}"
            root = output/label
            root.mkdir()
            factory = CategoricalOnlineSessionFactory(root/"session", runtime.session_bridge(), config, NoRecoveryCurriculum())
            random_factory = None
            random_checkpoint = None
            warm_start = None
            if demonstrations is not None:
                evidence.stage = label+"/initialization"
                random_factory = CategoricalOnlineSessionFactory(root/"random-session", runtime.session_bridge(), config, NoRecoveryCurriculum())
                random_session = random_factory.new(model_seed=model_seed, behavior_seed=model_seed ^ 0x535453)
                random_resume = random_session.publish()
                random_checkpoint = ("random", 0, random_session.active_behavior_manifest_id, random_resume.manifest_id)
                with torch.random.fork_rng(devices=[]):
                    torch.manual_seed(model_seed)
                    anchor = RaggedCandidateScorer.from_bridge_schema(runtime.schema, config.profile.scorer)
                anchor.eval().requires_grad_(False)
                clone_config = StrategicBehaviorCloningConfig(epochs=initialization.epochs,
                    learning_rate=initialization.learning_rate, batch_rows=initialization.batch_rows,
                    shuffle_seed=model_seed)
                clone = train_strategic_behavior_clone(anchor, demonstrations, clone_config)
                warm_start = clone.scorer
                weights_path = root/"rule-initialization.pt"
                torch.save(warm_start.state_dict(), weights_path)
                _write(root/"rule-initialization.json", {
                    "label_source": "committed_public_baseline", "model_seed": model_seed,
                    "config": clone_config, "weights_sha256": _digest(weights_path),
                    "training_samples": "../demonstrations-training",
                    "validation_samples": "../demonstrations-validation",
                    **{field.name: getattr(clone, field.name) for field in dataclasses.fields(clone)
                       if field.name != "scorer"}})
                progress(f"{label} rule clone: train {clone.final_training.agreement:.3f}, held-out {clone.final_held_out.agreement:.3f}, {clone.optimizer_steps} optimizer steps")
            session = factory.new(model_seed=model_seed, behavior_seed=model_seed ^ 0x535453,
                                  initial_scorer=warm_start, initial_scorer_actor_only=warm_start is not None)
            initial = session.active_behavior_manifest_id
            initial_resume = session.publish()
            # Restore through the same maintained owner graph and binding gate.
            session = factory.restore(initial_resume.manifest_id)
            checkpoints = [("initial", 0, initial, initial_resume.manifest_id)]
            evidence.stage = label+"/training"
            updates = []
            training_stop = "generation_limit"
            train_start = time.perf_counter()
            for generation in range(spec.generations):
                generation_started = time.perf_counter()
                result = session.advance_generation(max_batch_steps=spec.max_steps)
                generation_seconds = time.perf_counter()-generation_started
                updates.append({"generation": generation+1, "progress": result,
                                "learning": session.runner.trainer.last_result,
                                "rollout_payload_bytes": session.runner.update_batcher.last_update_payload_bytes,
                                "generation_seconds": generation_seconds,
                                "episodes_per_second": result.terminal_attempts/generation_seconds,
                                "elapsed_seconds": time.perf_counter()-train_start})
                _write(root/"updates.json", updates)
                progress(f"{label}: generation {generation+1}/{spec.generations}, episodes {result.terminal_attempts}, promoted {result.promoted}, {generation_seconds:.1f}s")
                if not result.promoted:
                    raise RuntimeError(f"{label}: incomplete training generation")
                if generation + 1 in spec.evaluation_generations:
                    milestone_resume = session.publish()
                    checkpoints.append((f"generation-{generation+1}", generation+1,
                                        session.active_behavior_manifest_id, milestone_resume.manifest_id))
                if time.perf_counter()-train_start > spec.training_seconds_per_model and generation+1 < spec.generations:
                    training_stop = "time_budget"
                    progress(f"{label}: training time budget reached after {len(updates)} completed generations; saving and evaluating")
                    break
            final = session.active_behavior_manifest_id
            final_resume = session.publish()
            completed_generations = len(updates)
            if checkpoints[-1][1] == completed_generations:
                checkpoints.pop()
            checkpoints.append(("final", completed_generations, final, final_resume.manifest_id))
            if random_checkpoint is not None:
                checkpoints.insert(0, random_checkpoint)
            _write(root/"models.json", {"initial_manifest": initial, "final_manifest": final,
                "initial_resume": initial_resume.manifest_id, "final_resume": final_resume.manifest_id,
                "evaluation_checkpoints": [{"name": name, "generation": generation, "manifest": identity,
                                            "resume_manifest": resume}
                                           for name, generation, identity, resume in checkpoints],
                "training_stop": training_stop,
                "initialization": "random" if warm_start is None else "rule_clone_actor",
                "random_control": random_checkpoint,
                "executor_id": runtime.binding.identity})
            panels = {}
            curve = []
            for name, generation, identity, _ in checkpoints:
                evidence.stage = label+"/"+name
                panels[name] = evaluate_fixed_seed_panel(lambda seeds: runtime.environment(seeds, spec.ascension),
                    _frozen_policy_factory(random_factory if name == "random" else factory, identity),
                    panel, lambda rows: _write(root/(name+"-panel.json"), rows),
                    evidence.decision)
                progress(f"{label} {name}: {panel_summary(panels[name])}")
                paired = (paired_seed_floor_deltas(panels["initial"], panels[name])
                          if "initial" in panels else None)
                fallback_free_paired = (paired_seed_floor_deltas(
                    panels["initial"], panels[name], require_fallback_free=True)
                    if "initial" in panels else None)
                random_delta = (paired_seed_floor_deltas(panels["random"], panels[name])
                                if "random" in panels else None)
                uniform = paired_seed_floor_deltas(baseline, panels[name])
                rule_delta = paired_seed_floor_deltas(rules, panels[name]) if rules is not None else None
                curve.append({"name": name, "generation": generation, "training_episodes": generation*spec.episodes_per_generation,
                              "manifest": identity, "panel": panel_summary(panels[name]),
                              "paired_seed_floor_deltas": paired,
                              "paired_mean_delta": sum(paired.values())/len(paired) if paired is not None else None,
                              "fallback_free_paired_mean_delta": (
                                  sum(fallback_free_paired.values())/len(fallback_free_paired)
                                  if fallback_free_paired is not None else None),
                              "vs_uniform_mean_delta": sum(uniform.values())/len(uniform) if uniform is not None else None,
                              "vs_random_mean_delta": sum(random_delta.values())/len(random_delta) if random_delta is not None else None,
                              "vs_rules_mean_delta": sum(rule_delta.values())/len(rule_delta) if rule_delta is not None else None})
                _write(root/"learning-curve.json", curve)
            deltas = paired_seed_floor_deltas(panels["initial"], panels["final"])
            fallback_free_deltas = paired_seed_floor_deltas(
                panels["initial"], panels["final"], require_fallback_free=True)
            baseline_deltas = paired_seed_floor_deltas(baseline, panels["final"])
            run = {"model_seed": model_seed, "initial": panel_summary(panels["initial"]), "final": panel_summary(panels["final"]),
                   "random": panel_summary(panels["random"]) if "random" in panels else None,
                   "initialization": "random" if warm_start is None else "rule_clone_actor",
                   "paired_seed_floor_deltas": deltas,
                   "paired_mean_delta": sum(deltas.values())/len(deltas) if deltas is not None else None,
                   "fallback_free_paired_mean_delta": (
                       sum(fallback_free_deltas.values())/len(fallback_free_deltas)
                       if fallback_free_deltas is not None else None),
                   "vs_uniform_mean_delta": sum(baseline_deltas.values())/len(baseline_deltas) if baseline_deltas is not None else None,
                   "learning_curve": curve,
                   "training_episodes": sum(update["progress"].terminal_attempts for update in updates),
                   "planned_training_episodes": spec.generations*spec.episodes_per_generation,
                   "training_stop": training_stop,
                   "training_episodes_per_second": sum(update["progress"].terminal_attempts for update in updates)/updates[-1]["elapsed_seconds"],
                   "training_seconds": updates[-1]["elapsed_seconds"]}
            runs.append(run)
            final_models.append((model_seed, initial, final))
            _write(output/"runs.json", runs)
        final_test = None
        if final_panel is not None:
            evidence.stage = "final-test"
            final_test = _final_evaluation(output, spec, final_panel, final_models, progress)
        summary = {"status": "complete", "uniform": panel_summary(baseline), "runs": runs,
                   "final_test": final_test,
                   "rules": panel_summary(rules) if rules is not None else None,
                   "baseline_seconds": {"rules": rules_seconds, "uniform": uniform_seconds},
                   "elapsed_seconds": time.perf_counter()-started,
                   "executor_counts": evidence.counts,
                   "strategic_evidence": "strategic-evidence.jsonl",
                   "conclusion_scope": "per-initialization paired results; replicas and initializations share held-out seeds, not independent game samples"}
        _write(output/"summary.json", summary)
        return summary
    except Exception as error:
        if session is not None and evidence.stage.endswith("/training"):
            try:
                session.runner.controller.publish_active()
                _write(root/"interrupted-model.json", {
                    "manifest": session.active_behavior_manifest_id,
                    "completed_generations": len(updates),
                    "resumable_session": False,
                    "source_error": type(error).__name__,
                })
            except Exception as diagnostic_error:
                error.add_note(f"Interrupted model capture failed: {diagnostic_error}")
        _write(output/"failure.json", {"type": type(error).__name__, "message": str(error),
                                     "stage": evidence.stage, "completed_model_runs": len(runs)})
        raise
    finally:
        _write(output/"executor-counts.json", evidence.counts)
        _write(output/"artifact-hashes.json", {str(p.relative_to(output)): _digest(p)
            for p in sorted(output.rglob("*")) if p.is_file() and p.name != "artifact-hashes.json"})


def verify_study(output: Path):
    """Read-only integrity and installed-executor compatibility checks."""
    output = Path(output).resolve()
    hashes = json.loads((output/"artifact-hashes.json").read_text(encoding="utf-8"))
    inventory = {str(p.relative_to(output)) for p in output.rglob("*") if p.is_file() and p.name != "artifact-hashes.json"}
    if set(hashes) != inventory or not {"contract.json", "executor-counts.json"} <= inventory:
        raise ValueError("study artifact inventory mismatch")
    for relative, digest in hashes.items():
        path = (output/relative).resolve()
        if not path.is_relative_to(output) or not path.is_file() or _digest(path) != digest:
            raise ValueError(f"study artifact mismatch: {relative}")
    contract = json.loads((output/"contract.json").read_text(encoding="utf-8"))
    spec = StrategicStudySpec.from_dict(contract["spec"])
    runtime = InstalledFixedCombatRuntime(spec.executor, final_act=spec.final_act,
                                          strategic_navigation=spec.strategic_navigation)
    if runtime.binding.identity != contract["executor_id"]:
        raise ValueError("installed executor differs from the study contract")
    current_sources = {str(p.relative_to(Path(__file__).parent)): _digest(p)
                       for p in Path(__file__).parent.rglob("*.py")}
    if current_sources != contract["learning_source_sha256"]:
        raise ValueError("installed learning source differs from the study contract")
    return {"status": "verified", "files": len(hashes), "study_complete": (output/"summary.json").exists()}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--spec", type=Path)
    parser.add_argument("--preset", choices=("a0-rollout",))
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args(argv)
    if args.verify:
        if args.spec is not None or args.preset is not None:
            parser.error("--verify reads the saved contract; do not pass --spec/--preset")
        print(verify_study(args.output))
        return
    if args.spec is not None and args.preset is not None:
        parser.error("choose --spec or --preset")
    spec = (StrategicStudySpec.from_dict(json.loads(args.spec.read_text(encoding="utf-8"))) if args.spec
            else StrategicStudySpec.a0_rollout() if args.preset else StrategicStudySpec())
    result = run_study(args.output, spec)
    print(f"Study complete: {len(result['runs'])} initializations, {result['elapsed_seconds']:.1f}s")


if __name__ == "__main__":
    main()
