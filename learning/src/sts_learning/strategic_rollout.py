"""Continuous strategic play with a fixed combat executor and scoped policy.

The controller owns reward-processing memory, not game effects. It commits that
memory only after the native macro commits. A search miss retains the ready
choice and produces no invented defeat or learning return.
"""
from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import time
from collections.abc import Mapping
from enum import Enum
from pathlib import Path

from .fixed_combat import FixedCombatConfig
from .decision_rows import PreparedDecisionRows
from .fixed_combat_environment import InstalledFixedCombatRuntime, canonical_json
from .outcomes import TerminalStepBatch
from .policy import BatchPolicyChoice, BehaviorManifestId, DETERMINISTIC_SELECTION
from .run_resource_trace import RunPublicContext
from .semantic_batch import select_semantic_decision_rows
from .strategic_navigation import StrategicRewardMemory
from .strategic_model_scope import StrategicModelApplicability, require_training_fit
from .strategic_guide import CompiledStrategicGuide, FIRST_ATTACK_GUIDE, StrategicGuideUnresolved


class ModelSite(str, Enum):
    CARD_REWARD = "card_reward"
    CAMPFIRE = "campfire"


class StrategicModelOutOfScope(RuntimeError):
    """Current input preserved without scoring, adopting or fallback selection."""
    def __init__(self, batch, decisions):
        self.prepared = PreparedDecisionRows.capture(batch)
        self.decisions = tuple(decisions)
        super().__init__("strategic input outside declared model applicability")


def load_scoped_model(directory, *, schema, model_sites):
    """Restore verified complete-decision weights for explicitly owned sites.

    The live bridge supplies the numeric schema; JSON's stringified enum keys
    are compared as JSON, never used to construct the scorer. No labels or
    training optimizer are loaded into the policy.
    """
    sites = tuple(model_sites)
    if (not sites or any(not isinstance(site, ModelSite) for site in sites)
            or len(set(sites)) != len(sites)):
        raise ValueError("saved model requires distinct typed model sites")
    root = Path(directory).resolve()
    contract_bytes = (root/"contract.json").read_bytes()
    summary_bytes = (root/"summary.json").read_bytes()
    contract, summary = json.loads(contract_bytes), json.loads(summary_bytes)
    if (not isinstance(contract, dict) or not isinstance(summary, dict)
            or contract.get("schema") not in {"sts-scoped-decision-clone-v1", "sts-scoped-field-classifier-v1"}
            or contract.get("semantic_schema") != json.loads(canonical_json(schema))):
        raise ValueError("saved model semantic schema or decision contract differs")
    declared_sites = contract.get("model_sites")
    if (not isinstance(declared_sites, list)
            or not declared_sites or len(set(declared_sites)) != len(declared_sites)
            or any(site not in {item.value for item in ModelSite} for site in declared_sites)
            or not {site.value for site in sites} <= set(declared_sites)):
        raise ValueError("requested model sites escape the saved model scope")
    applicability = StrategicModelApplicability.from_contract(
        contract.get("applicability"), schema=schema, model_sites=declared_sites)
    if not {site.value for site in sites} <= set(applicability.contract()["sites"]):
        raise ValueError("requested model sites lack applicability declarations")
    field_classifier = contract["schema"] == "sts-scoped-field-classifier-v1"
    architecture = contract.get("decision_architecture")
    payload = (root/("model.lgb" if field_classifier else "model.ststorch")).read_bytes()
    model_sha = hashlib.sha256(payload).hexdigest()
    if (summary.get("model_sha256") != model_sha
            or summary.get("validation", {}).get("model_round_trip") is not True):
        raise ValueError("saved model weights are not verified")
    require_training_fit(contract, summary)
    ancestor = contract.get("initialization", {})
    if not isinstance(ancestor, dict):
        raise ValueError("saved model initialization provenance is malformed")
    source_seeds = {}
    for key in ("training_seeds", "evaluation_seeds"):
        direct, inherited = contract.get(key), ancestor.get("source_"+key, [])
        if (not isinstance(direct, list) or not isinstance(inherited, list)
                or any(type(seed) is not int or not 0 <= seed < 2**64 for seed in direct+inherited)):
            raise ValueError("saved model seed provenance is malformed")
        source_seeds[key] = sorted(set(direct+inherited))
    if field_classifier:
        import lightgbm as lgb
        from .strategic_field_policy import (
            FIELD_CHOICE_TARGET_RELEVANCE, FIELD_READOUT, FIELD_READOUTS,
            GreedyStrategicFieldPolicy, StrategicFieldEncoder, validate_field_relevance,
        )
        readout = contract.get("field_readout")
        if architecture != "field-membership" or not isinstance(readout, str) or readout not in FIELD_READOUTS:
            raise ValueError("saved model scorer architecture differs")
        if contract.get("training", {}).get("readout", FIELD_READOUT) != readout:
            raise ValueError("saved model readout contract differs")
        names_bytes = (root/"feature-names.json").read_bytes()
        names_sha = hashlib.sha256(names_bytes).hexdigest()
        if (contract.get("feature_names_sha256") != names_sha
                or summary.get("feature_names_sha256") != names_sha):
            raise ValueError("saved field classifier vocabulary is not verified")
        encoder = StrategicFieldEncoder(schema, json.loads(names_bytes), readout=readout)
        if type(contract.get("feature_count")) is not int or contract["feature_count"] != len(encoder.feature_names):
            raise ValueError("saved field classifier feature count differs")
        try:
            scorer = lgb.Booster(model_str=payload.decode("utf-8"))
        except (UnicodeError, lgb.basic.LightGBMError) as error:
            raise ValueError("saved model weights are incompatible") from error
        # Validate immediately, before constructing an identity or playing.
        from .strategic_field_policy import validate_field_booster
        objective = contract.get("field_objective", "binary")
        if contract.get("training", {}).get("objective", "binary") != objective:
            raise ValueError("saved model objective contract differs")
        relevance = contract.get("field_relevance", FIELD_CHOICE_TARGET_RELEVANCE)
        validate_field_relevance(relevance, objective)
        if contract.get("training", {}).get("relevance", FIELD_CHOICE_TARGET_RELEVANCE) != relevance:
            raise ValueError("saved model relevance contract differs")
        validate_field_booster(scorer, encoder, objective=objective)
        frozen_qualification = ancestor.get("kind") == "frozen-field-qualification"
        if frozen_qualification or contract["training"].get("mode") == "frozen-qualification":
            source_manifest = ancestor.get("manifest")
            digests = ("model_sha256", "feature_names_sha256", "contract_sha256", "summary_sha256", "behavior_id")
            if (not frozen_qualification or contract["training"].get("mode") != "frozen-qualification"
                    or type(summary.get("updates")) is not int or summary["updates"] != 0
                    or summary["validation"].get("frozen_source_unchanged") is not True
                    or any(not isinstance(ancestor.get(key), str) or len(ancestor[key]) != 64
                           or any(c not in "0123456789abcdef" for c in ancestor[key]) for key in digests)
                    or ancestor["model_sha256"] != model_sha or ancestor["feature_names_sha256"] != names_sha
                    or not isinstance(source_manifest, dict)
                    or ancestor["behavior_id"] != hashlib.sha256(canonical_json(source_manifest).encode()).hexdigest()
                    or any(source_manifest.get(key) != ancestor[key] for key in digests[:-1])
                    or source_manifest.get("field_readout") != readout
                    or source_manifest.get("field_objective") != objective
                    or source_manifest.get("field_relevance", FIELD_CHOICE_TARGET_RELEVANCE) != relevance
                    or source_manifest.get("source_seeds") != {
                        key: ancestor.get("source_"+key) for key in ("training_seeds", "evaluation_seeds")}):
                raise ValueError("saved frozen field qualification provenance differs")
        backend_manifest = {"field_readout": readout, "field_objective": objective, "feature_names_sha256": names_sha,
                            "feature_count": len(encoder.feature_names), "lightgbm_version": lgb.__version__,
                            "field_policy_sha256": hashlib.sha256(
                                (Path(__file__).parent/"strategic_field_policy.py").read_bytes()).hexdigest()}
        if "field_relevance" in contract:
            backend_manifest["field_relevance"] = relevance
    else:
        import torch
        from .torch_checkpoints import decode_state_dict
        from .torch_policy import GreedyTorchPolicy, RaggedCandidateScorer, RaggedScorerConfig
        config = RaggedScorerConfig(**contract["scorer"])
        normalized_config = {"family_pooling": "vector-max", "observation_pooling": "mean", **contract["scorer"]}
        expected_family = schema["categorical_field"].get("ActionKind") if architecture == "family-target" else None
        if (architecture not in ("flat", "family-target") or dataclasses.asdict(config) != normalized_config
                or config.family_field != expected_family
                or architecture == "family-target" and expected_family is None):
            raise ValueError("saved model scorer architecture differs")
        with torch.random.fork_rng(devices=[]):
            scorer = RaggedCandidateScorer.from_bridge_schema(schema, config)
        try:
            scorer.load_state_dict(decode_state_dict(payload), strict=True)
        except (RuntimeError, ValueError) as error:
            raise ValueError("saved model weights are incompatible") from error
        if any(not bool(torch.isfinite(tensor).all()) for tensor in scorer.state_dict().values()):
            raise ValueError("saved model weights must be finite")
        scorer.eval().requires_grad_(False)
        backend_manifest = {"family_pooling": config.family_pooling, "observation_pooling": config.observation_pooling,
                            "torch_policy_sha256": hashlib.sha256(
                                (Path(__file__).parent/"torch_policy.py").read_bytes()).hexdigest(),
                            "torch_version": torch.__version__}
    manifest = {
        "policy": "scoped-field-membership-greedy-v1" if field_classifier else "scoped-complete-decision-greedy-v1",
        "model_sha256": model_sha,
        "contract_sha256": hashlib.sha256(contract_bytes).hexdigest(),
        "summary_sha256": hashlib.sha256(summary_bytes).hexdigest(),
        "model_sites": sorted(site.value for site in sites),
        "applicability": applicability.contract(),
        "decision_architecture": architecture,
        "inference": "full legal greedy argmax",
        "source_seeds": source_seeds,
        "loader_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "applicability_owner_sha256": hashlib.sha256(
            (Path(__file__).parent/"strategic_model_scope.py").read_bytes()).hexdigest(),
        **backend_manifest,
    }
    if "choice_target" in contract:
        manifest["choice_target"] = contract["choice_target"]
    identity = BehaviorManifestId(hashlib.sha256(canonical_json(manifest).encode()).digest())
    policy = (GreedyStrategicFieldPolicy(scorer, encoder, identity, objective=objective) if field_classifier
              else GreedyTorchPolicy(scorer, identity))
    return policy, {"root": str(root), "manifest": manifest, "behavior_id": identity.digest.hex()}


class StrategicRunController(StrategicRewardMemory):
    """One resident run cohort; model ownership lasts through symbolic choices.

    The native legality baseline handles all other sites. Campfire ownership
    includes rest and upgrade choices. This routing seam does not fit a model.
    Supply a frozen BatchPolicyChoice policy to opt into model-controlled sites.
    A typed ModelSite-to-policy mapping keeps independently trained sites apart.
    """
    def __init__(self, environment, *, model_policy=None,
                 model_sites=(ModelSite.CARD_REWARD, ModelSite.CAMPFIRE),
                 model_applicability=None, decision_sink=None, card_reward_guide=None):
        if environment.ready or environment.terminal_count:
            raise ValueError("controller requires fresh undecoded strategic slots")
        self._configure(environment, model_policy, model_sites, model_applicability, card_reward_guide)
        if decision_sink is not None and not callable(decision_sink):
            raise ValueError("decision sink must be callable")
        self.decision_sink = decision_sink
        self._rooms = {}
        self._skipped_rewards = {}
        self._reward_card_items = {}
        self._proceeded_rewards = set()
        self._model_slots = set()
        self._pending = []

    def _configure(self, environment, model_policy, model_sites, model_applicability, card_reward_guide=None):
        sites = frozenset(model_sites)
        if any(not isinstance(site, ModelSite) for site in sites):
            raise ValueError("model sites must be typed ModelSite values")
        self.environment = environment
        self.model_policy = model_policy
        split_policies = isinstance(model_policy, Mapping)
        if split_policies and (not model_policy or any(
                not isinstance(site, ModelSite) or site not in sites for site in model_policy)):
            raise ValueError("site policies require nonempty typed ModelSite keys within model sites")
        self._model_policies = (dict(model_policy) if split_policies else
                                {site: model_policy for site in sites if model_policy is not None})
        self.model_sites = frozenset(self._model_policies)
        self._model_identity = (None if model_policy is None or split_policies else
                                model_policy.behavior_manifest_id)
        self._model_identities = {site: getattr(policy, "behavior_manifest_id", None)
                                  for site, policy in self._model_policies.items()}
        if any(not isinstance(identity, BehaviorManifestId) for identity in self._model_identities.values()):
            raise ValueError("model requires a typed behavior identity")
        scopes = {} if model_applicability is None else model_applicability
        if (not isinstance(scopes, Mapping) or set(scopes) != self.model_sites
                or any(not isinstance(scope, StrategicModelApplicability)
                       or site.value not in scope.contract()["sites"]
                       or scope.schema_sha256 != environment.binding.semantic_schema_sha256
                       for site, scope in scopes.items())):
            raise ValueError("model requires matching typed applicability for every owned site")
        self._model_applicability = dict(scopes)
        if card_reward_guide is not None and (
                not isinstance(card_reward_guide, CompiledStrategicGuide)
                or card_reward_guide.schema_sha256 != environment.binding.semantic_schema_sha256
                or ModelSite.CARD_REWARD in self.model_sites):
            raise ValueError("guide requires matching schema and separate card reward ownership")
        self.card_reward_guide = card_reward_guide
        self._guide_identity = (None if card_reward_guide is None else card_reward_guide.behavior_manifest_id)
        self._model_slot_sites = {}
        self.manifest = json.loads(canonical_json({
            "controller": "scoped-strategic-run-v1",
            "controller_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "reward_memory_sha256": hashlib.sha256(Path(__file__).with_name("strategic_navigation.py").read_bytes()).hexdigest(),
            "executor_id": environment.binding.identity,
            "model_id": None if self._model_identity is None else self._model_identity.digest.hex(),
            "model_sites": sorted(site.value for site in self.model_sites),
            "site_model_ids": {site.value: identity.digest.hex()
                               for site, identity in self._model_identities.items()},
            "site_applicability": {site.value: scope.contract()
                                   for site, scope in self._model_applicability.items()},
            "baseline": environment.baseline_manifest,
            "card_reward_guide": None if card_reward_guide is None else {
                "behavior_id": self._guide_identity.digest.hex(), "manifest": card_reward_guide.manifest()},
        }))
        self.behavior_manifest_id = BehaviorManifestId(
            hashlib.sha256(canonical_json(self.manifest).encode()).digest())

    def _contexts(self):
        return {slot: RunPublicContext.from_bridge_row((slot, view))
                for slot, view in self.environment.public_run_contexts()}

    def _choose(self):
        env = self.environment
        contexts = self._contexts()
        self.observe_rooms(contexts)
        batch = env.decision_batch(semantic=True, legality_bootstrap=True,
                                   reward_exit_slots=sorted(self._proceeded_rewards))
        audits = []
        ordinals = []
        model_rows = []
        guide_rows = []
        site_rows = {}
        for row, slot in enumerate(batch["slot_indices"]):
            slot = int(slot)
            context = contexts[slot]
            if context.is_combat:
                raise RuntimeError("strategic controller received a combat decision")
            raw = env.strategic_decision_audit_json(slot)
            audit = None if raw is None else json.loads(raw)
            if audit is not None and (
                audit.get("schema") != "sts-learning-strategic-decision-audit-v2"
                or len(audit["candidates"]) != int(batch["candidate_counts"][row])
            ):
                raise ValueError("strategic audit does not match the live candidate surface")
            audits.append(audit)
            if audit is not None and audit["decision_site"] == "reward":
                self._reward_card_items[slot] = self._card_reward_indices(audit)
            if not batch["legality_bootstrap_available"][row]:
                raise RuntimeError("strategic baseline unavailable")
            ordinals.append(self._baseline_ordinal(
                slot, audit, int(batch["legality_bootstrap_ordinals"][row])))
            if audit is not None:
                self._model_slots.discard(slot)
                self._model_slot_sites.pop(slot, None)
                if audit["decision_site"] in self.model_sites:
                    self._model_slots.add(slot)
                    self._model_slot_sites[slot] = ModelSite(audit["decision_site"])
                if self.card_reward_guide is not None and audit["decision_site"] == "card_reward":
                    guide_rows.append(row)
            if slot in self._model_slots:
                model_rows.append(row)
                site_rows.setdefault(self._model_slot_sites[slot], []).append(row)
        probabilities = [DETERMINISTIC_SELECTION] * len(ordinals)
        guide_results = {}
        if guide_rows:
            guide = self.card_reward_guide
            if guide.behavior_manifest_id != self._guide_identity:
                raise ValueError("guide changed during a fixed-controller rollout")
            guide_batch = select_semantic_decision_rows(
                {key: value for key, value in batch.items() if not key.startswith("legality_bootstrap_")},
                guide_rows)
            results = guide.evaluate(guide_batch, [audits[row] for row in guide_rows])
            guide_results = dict(zip(guide_rows, results, strict=True))
            failures = [{"slot": int(batch["slot_indices"][row]), "site": "card_reward",
                         "source_token": batch["decision_token_ids"][row],
                         "public_snapshot": batch["public_snapshot_ids"][row],
                         "context": dataclasses.asdict(contexts[int(batch["slot_indices"][row])]),
                         "audit": audits[row], "guide": result.record(), "adopted": False}
                        for row, result in guide_results.items() if result.ordinal is None]
            if failures:
                # Preserve every guide row in this cohort, including resolved
                # rows: none is adopted when another still requires comparison.
                raise StrategicGuideUnresolved(guide_batch, failures)
            for row, result in guide_results.items():
                ordinals[row] = result.ordinal
        if model_rows:
            if (self._model_identity is not None and
                    self.model_policy.behavior_manifest_id != self._model_identity):
                raise ValueError("model changed during a fixed-controller rollout")
            # Strip bootstrap columns; compact sparse token tables using the
            # existing row algebra, without exposing other decision sites.
            inputs = {}
            failures = []
            failed_rows = []
            # Validate every owned row before any policy receives a batch. A
            # mixed cohort cannot adopt its supported rows while losing another.
            for site, rows in site_rows.items():
                inputs[site] = select_semantic_decision_rows(
                    {key: value for key, value in batch.items()
                     if not key.startswith("legality_bootstrap_")}, rows)
                for row, issues in zip(rows, self._model_applicability[site].issues(site.value, inputs[site]), strict=True):
                    if issues:
                        slot = int(batch["slot_indices"][row])
                        failed_rows.append(row)
                        failures.append({"slot": slot, "site": site.value,
                            "source_token": batch["decision_token_ids"][row],
                            "public_snapshot": batch["public_snapshot_ids"][row],
                            "context": dataclasses.asdict(contexts[slot]), "audit": audits[row],
                            "model_behavior_id": self._model_identities[site].digest.hex(),
                            "issues": issues, "adopted": False})
            if failures:
                raise StrategicModelOutOfScope(select_semantic_decision_rows(
                    {key: value for key, value in batch.items()
                     if not key.startswith("legality_bootstrap_")}, failed_rows), failures)
            for site, rows in site_rows.items():
                policy, identity = self._model_policies[site], self._model_identities[site]
                if policy.behavior_manifest_id != identity:
                    raise ValueError("model changed during a fixed-controller rollout")
                choice = policy.choose(inputs[site])
                if (not isinstance(choice, BatchPolicyChoice)
                        or choice.behavior_manifest_id != identity
                        or policy.behavior_manifest_id != identity
                        or len(choice.ordinals) != len(rows)):
                    raise ValueError("invalid scoped model choice")
                for row, ordinal, probability in zip(rows, choice.ordinals,
                                                      choice.selection_probabilities, strict=True):
                    ordinals[row] = ordinal
                    probabilities[row] = probability
        for row, ordinal in enumerate(ordinals):
            if not 0 <= ordinal < int(batch["candidate_counts"][row]):
                raise ValueError("policy ordinal outside the live candidate surface")
        choice = BatchPolicyChoice.create(ordinals, self.behavior_manifest_id, probabilities)
        selected_decisions = []
        for row, slot in enumerate(batch["slot_indices"]):
            slot = int(slot)
            audit = audits[row]
            selected_decisions.append({
                "slot": slot, "source_token": batch["decision_token_ids"][row],
                "public_snapshot": batch["public_snapshot_ids"][row],
                "phase": int(batch["phase"][row]),
                "context": dataclasses.asdict(contexts[slot]),
                "owner": "guide" if row in guide_results else "model" if row in model_rows else "baseline",
                "model_behavior_id": (self._model_identities[self._model_slot_sites[slot]].digest.hex()
                                      if row in model_rows else None),
                "ordinal": choice.ordinals[row],
                "selection_probability": choice.selection_probabilities[row].value,
                "audit": audit,
            })
            if row in guide_results:
                selected_decisions[-1]["guide_behavior_id"] = self._guide_identity.digest.hex()
                selected_decisions[-1]["guide"] = guide_results[row].record()
        if self.decision_sink is not None:
            public_batch = {key: value for key, value in batch.items()
                            if not key.startswith("legality_bootstrap_")}
            self.decision_sink(PreparedDecisionRows.capture(public_batch).payload,
                               json.loads(canonical_json(selected_decisions)))
        env.choose(batch["decision_token_ids"], list(choice.ordinals))
        self._pending.extend(selected_decisions)

    def advance(self, *, max_selection_rounds=256):
        """Return native result and source decisions; unresolved keeps both pending.

        Call again to retry the same ready choices. Neither retry nor interruption
        resamples the model or commits reward-processing memory.
        """
        if type(max_selection_rounds) is not int or max_selection_rounds <= 0:
            raise ValueError("selection limit must be a positive integer")
        for _ in range(max_selection_rounds):
            if self.environment.ready:
                break
            self._choose()
        if not self.environment.ready:
            raise RuntimeError("symbolic selection budget exhausted; choices remain pending")
        before = self._contexts()
        result = self.environment.fixed_step()
        decisions = tuple(self._pending)
        after = self._contexts()
        if result.advanced:
            self.commit_rewards(decisions, after)
            self._pending = []
            self._model_slots.clear()
            self._model_slot_sites.clear()
        return result, decisions, before, after

    def memory(self):
        """Persist alongside the opaque executor-bound environment checkpoint."""
        return {"behavior_id": self.behavior_manifest_id.digest.hex(),
                "environment_sha256": hashlib.sha256(self.environment.checkpoint_bytes(
                    max_bytes=64*1024*1024)).hexdigest(),
                "rooms": {str(slot): room for slot, room in self._rooms.items()},
                "skipped_rewards": {str(slot): sorted(items) for slot, items in self._skipped_rewards.items()},
                "reward_card_items": {str(slot): items for slot, items in self._reward_card_items.items()},
                "proceeded_rewards": sorted(self._proceeded_rewards),
                "model_slots": sorted(self._model_slots),
                "model_slot_sites": {str(slot): site.value for slot, site in self._model_slot_sites.items()},
                "pending_decisions": self._pending}

    @classmethod
    def from_memory(cls, environment, memory, *, model_policy=None,
                    model_sites=(ModelSite.CARD_REWARD, ModelSite.CAMPFIRE), model_applicability=None,
                    card_reward_guide=None):
        """Explicit restore, bound to this exact environment and fixed policy.

        A model caller separately owns restoration of its sampling RNG. Ready
        pending choices are already decoded and are not resampled here.
        """
        controller = cls.__new__(cls)
        controller._configure(environment, model_policy, model_sites, model_applicability, card_reward_guide)
        controller.decision_sink = None
        if memory["behavior_id"] != controller.behavior_manifest_id.digest.hex():
            raise ValueError("controller memory policy binding mismatch")
        actual = hashlib.sha256(environment.checkpoint_bytes(max_bytes=64*1024*1024)).hexdigest()
        if memory["environment_sha256"] != actual:
            raise ValueError("controller memory environment mismatch")
        controller._rooms = {int(slot): tuple(room) for slot, room in memory["rooms"].items()}
        controller._skipped_rewards = {int(slot): set(items) for slot, items in memory["skipped_rewards"].items()}
        controller._reward_card_items = {int(slot): list(items) for slot, items in memory["reward_card_items"].items()}
        controller._proceeded_rewards = set(memory["proceeded_rewards"])
        controller._model_slots = set(memory["model_slots"])
        controller._model_slot_sites = {int(slot): ModelSite(site) for slot, site in memory["model_slot_sites"].items()}
        if (set(controller._model_slot_sites) != controller._model_slots or
                not set(controller._model_slot_sites.values()) <= controller.model_sites):
            raise ValueError("controller memory model-site ownership mismatch")
        controller._pending = json.loads(canonical_json(memory["pending_decisions"]))
        for decision in controller._pending:
            if decision["owner"] == "guide":
                if (controller._guide_identity is None
                        or decision.get("guide_behavior_id") != controller._guide_identity.digest.hex()
                        or decision["audit"]["decision_site"] != "card_reward"):
                    raise ValueError("controller memory pending guide ownership mismatch")
                continue
            if decision["owner"] != "model":
                continue
            site = controller._model_slot_sites.get(decision["slot"])
            audit = decision["audit"]
            if (site is None or
                    decision["model_behavior_id"] != controller._model_identities[site].digest.hex() or
                    audit is not None and audit["decision_site"] != site.value):
                raise ValueError("controller memory pending model ownership mismatch")
        return controller


def run_rollout(output, *, seed, ascension=20, final_act=True, executor=None,
                max_steps=512, capture_samples=False, model_directory=None,
                model_sites=None, site_model_directories=None, progress=print, card_reward_guide=None):
    """Fresh run; optional verified frozen model with explicit site ownership."""
    output = Path(output).resolve()
    if type(max_steps) is not int or max_steps <= 0:
        raise ValueError("step limit must be a positive integer")
    if (model_directory is None) != (model_sites is None):
        raise ValueError("model directory and model sites must be supplied together")
    if site_model_directories is not None:
        if (model_directory is not None or not isinstance(site_model_directories, Mapping)
                or not site_model_directories or any(not isinstance(site, ModelSite) for site in site_model_directories)):
            raise ValueError("site model directories require a nonempty typed mapping, separate from a shared model")
        site_model_directories = dict(site_model_directories)
    if model_sites is not None:
        model_sites = tuple(model_sites)
    executor = executor or FixedCombatConfig(
        backend="whole_combat", generation_work=30_000, wall_ms=1_000,
        atomic_nodes=None, fallback_action_limit=None)
    runtime = InstalledFixedCombatRuntime(executor, final_act=final_act)
    model_policy = model_source = None
    if model_directory is not None:
        model_policy, model_source = load_scoped_model(
            model_directory, schema=runtime.schema, model_sites=model_sites)
        if "torch_version" in model_source["manifest"]:
            import torch
            torch.set_num_threads(2)
    elif site_model_directories is not None:
        model_policy, model_source = {}, {}
        model_sites = tuple(site_model_directories)
        for site, directory in site_model_directories.items():
            model_policy[site], model_source[site.value] = load_scoped_model(
                directory, schema=runtime.schema, model_sites=(site,))
        if any("torch_version" in source["manifest"] for source in model_source.values()):
            import torch
            torch.set_num_threads(2)
    env = runtime.environment([seed], ascension)
    guide = (None if card_reward_guide is None else
             CompiledStrategicGuide(runtime.schema, name=card_reward_guide))
    applicability = None
    if model_policy is not None:
        applicability = {}
        for site in model_sites:
            source = model_source[site.value] if site_model_directories is not None else model_source
            applicability[site] = StrategicModelApplicability.from_contract(
                source["manifest"]["applicability"], schema=runtime.schema,
                model_sites=source["manifest"]["applicability"]["sites"])
    recorder = None
    if capture_samples:
        from .strategic_demonstrations import ScopedStrategicSampleRecorder
        recorder = ScopedStrategicSampleRecorder()
    controller = StrategicRunController(env, decision_sink=recorder, model_policy=model_policy,
        model_applicability=applicability,
        card_reward_guide=guide,
        model_sites=(ModelSite.CARD_REWARD, ModelSite.CAMPFIRE) if model_sites is None else model_sites)
    output.mkdir(parents=True, exist_ok=False)
    def write(name, value):
        (output/name).write_text(json.dumps(value, indent=2, allow_nan=False)+"\n", encoding="utf-8")
    write("contract.json", {"seed": seed, "ascension": ascension,
                           "goal": "heart" if final_act else "act_three",
                           "max_steps": max_steps,
                           "executor": dataclasses.asdict(executor),
                           "native_build_profile": runtime.native.NATIVE_BUILD_PROFILE,
                           "executor_binding": dataclasses.asdict(runtime.binding),
                           "capture_samples": capture_samples,
                           "model_source": model_source,
                           "controller": controller.manifest})
    started = time.perf_counter()
    reason = "step_limit"
    act_entries = []
    combats = 0
    fallback_combats = 0
    steps = 0
    model_decisions = 0
    guide_decisions = 0
    last_decision_loadout = None
    terminal_outcome = None
    scope_interruption = None
    try:
        for _ in range(max_steps):
            result, decisions, before, after = controller.advance()
            for decision in decisions:
                if decision["audit"] is not None:
                    last_decision_loadout = {
                        "context": decision["context"],
                        "deck": decision["audit"]["public_deck"],
                        "relics": decision["audit"]["public_relics"],
                    }
            event = {"committed": result.advanced, "decisions": decisions,
                     "before": dataclasses.asdict(before[0]),
                     "after": dataclasses.asdict(after[0]), "combat": result.evidence}
            with (output/"steps.jsonl").open("a", encoding="utf-8") as target:
                target.write(canonical_json(event)+"\n")
            if not result.advanced:
                reason = "combat_search_interruption"
                attempted = result.evidence["combats"][-1]["entry"]
                progress(f"F{attempted['floor']}: search interruption; pending choice preserved "
                         f"at F{after[0].floor}")
                break
            steps += 1
            if recorder is not None:
                recorder.commit(decisions)
            for decision in decisions:
                if decision["owner"] == "guide":
                    guide_decisions += 1
                    selected = decision["audit"]["candidates"][decision["ordinal"]]
                    progress(f"Guide {selected['kind']} {selected.get('card') or ''} "
                             f"at F{decision['context']['floor']}")
                if decision["owner"] == "model":
                    model_decisions += 1
                    audit = decision["audit"]
                    selected = (audit["candidates"][decision["ordinal"]]
                                if audit is not None else {"kind": "symbolic_selection"})
                    progress(f"Model {selected['kind']} {selected.get('card') or ''} "
                             f"at F{decision['context']['floor']}: "
                             f"HP {before[0].hp} -> {after[0].hp}")
            combats += len(result.evidence["combats"])
            fallback_combats += sum(c["fallback"] is not None for c in result.evidence["combats"])
            if after[0].act > before[0].act:
                raw = env.strategic_decision_audit_json(0)
                audit = None if raw is None else json.loads(raw)
                act_entries.append({"context": dataclasses.asdict(after[0]), "loadout": audit})
                (output/f"act-{after[0].act}-entry.bin").write_bytes(
                    env.checkpoint_bytes(max_bytes=64*1024*1024))
                write(f"act-{after[0].act}-memory.json", controller.memory())
            if before[0].floor != after[0].floor or result.evidence["combats"]:
                progress(f"Act {after[0].act} F{after[0].floor}: HP {after[0].hp}/{after[0].max_hp}; "
                         f"potions={after[0].potion_ids}; combats={combats}")
            if env.terminal_count:
                reason = "terminal"
                terminal_outcome = dataclasses.asdict(TerminalStepBatch.from_bridge_step(
                    result.transitions, slot_count=1).attempts[0])
                break
    except StrategicModelOutOfScope as error:
        reason = "model_out_of_scope"
        scope_interruption = {"schema": "sts-strategic-scope-interruption-v1",
                              "decisions": error.decisions, "adopted": False}
        # Plain numeric public inputs are diagnostic evidence, not adopted rows
        # or teacher labels. The exact executor-bound endpoint is saved below.
        def plain(value):
            if isinstance(value, Mapping):
                return {key: plain(child) for key, child in value.items()}
            if isinstance(value, (list, tuple)):
                return [plain(child) for child in value]
            return value.tolist() if hasattr(value, "tolist") else value
        write("out-of-scope.json", {**scope_interruption, "input": plain(error.prepared.payload),
                                    "semantic_schema": runtime.schema,
                                    "controller": controller.manifest})
        if recorder is not None:
            observations = [{"slot": decision["slot"], "source_token": decision["source_token"],
                             "public_snapshot": decision["public_snapshot"],
                             "ordinal": None, "owner": "observation_only",
                             "context": decision["context"], "audit": decision["audit"]}
                            for decision in error.decisions]
            recorder(error.prepared.payload, observations)
        progress("Model input outside declared scope; current decision preserved without adoption")
    except StrategicGuideUnresolved as error:
        reason = "guide_unresolved"
        scope_interruption = {"schema": "sts-strategic-guide-interruption-v1",
                              "decisions": error.decisions, "adopted": False}
        write("guide-unresolved.json", scope_interruption)
        if recorder is not None:
            by_slot = {decision["slot"]: decision for decision in error.decisions}
            # The recorder expects an audit for every preserved numeric row.
            # Only unresolved rows are observations; resolved cohort peers have
            # no adopted action either and are not recorded as successes.
            rows = [i for i, slot in enumerate(error.prepared.payload["slot_indices"])
                    if int(slot) in by_slot]
            public_batch = select_semantic_decision_rows(error.prepared.payload, rows)
            recorder(public_batch, [{"slot": decision["slot"], "source_token": decision["source_token"],
                     "public_snapshot": decision["public_snapshot"], "ordinal": None,
                     "owner": "observation_only", "context": decision["context"], "audit": decision["audit"]}
                     for decision in error.decisions])
        progress("Guide leaves unresolved comparisons; full menu preserved without adoption")
    finally:
        # Also keep exact pending state on an execution error. It is diagnostic
        # state, not an instruction to automatically resume this artifact.
        (output/"endpoint.bin").write_bytes(env.checkpoint_bytes(max_bytes=64*1024*1024))
        write("controller-memory.json", controller.memory())
        if recorder is not None:
            recorder.save(output/"samples", schema=runtime.schema,
                          producer={"controller": controller.manifest,
                                    "executor_binding": dataclasses.asdict(runtime.binding),
                                    "baseline": runtime.baseline_manifest},
                          terminal_outcome=terminal_outcome)
    endpoint = controller._contexts()[0]
    summary = {"stop": reason, "committed_steps": steps, "combats": combats,
               "committed_model_decisions": model_decisions,
               "committed_guide_decisions": guide_decisions,
               "fallback_combats": fallback_combats,
               "act1_completed": any(entry["context"]["act"] >= 2 for entry in act_entries),
               "act_entries": act_entries, "endpoint": dataclasses.asdict(endpoint),
               "terminal_outcome": terminal_outcome,
               "goal_completed": terminal_outcome is not None
                   and terminal_outcome["terminal_reward"] == 1
                   and (not final_act or terminal_outcome["terminal_act"] >= 4),
               "last_decision_loadout": last_decision_loadout,
               "scope_interruption": scope_interruption,
               "elapsed_seconds": time.perf_counter()-started}
    if recorder is not None:
        summary["samples"] = recorder.summary()
    write("summary.json", summary)
    return summary


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--ascension", type=int, default=20, choices=range(21))
    parser.add_argument("--goal", choices=("heart", "act_three"), default="heart")
    parser.add_argument("--executor", type=Path, help="explicit FixedCombatConfig JSON")
    parser.add_argument("--max-steps", type=int, default=512, help="committed macro-step limit")
    parser.add_argument("--capture-samples", action="store_true",
                        help="save public numerical card-reward/campfire rows separately from labels")
    parser.add_argument("--model-directory", type=Path,
                        help="verified complete-decision model directory; no training during play")
    parser.add_argument("--model-sites", nargs="+", choices=[site.value for site in ModelSite],
                        help="explicit sites owned by the saved model; other sites use the baseline")
    parser.add_argument("--site-model", action="append", default=[], metavar="SITE=DIRECTORY",
                        help="separate verified frozen model for one site; repeat for distinct sites")
    parser.add_argument("--card-reward-guide", choices=(FIRST_ATTACK_GUIDE,),
                        help="bounded compiled teacher guide; unresolved menus stop before adoption")
    args = parser.parse_args(argv)
    if (args.model_directory is None) != (args.model_sites is None):
        parser.error("--model-directory and --model-sites are required together")
    site_directories = None
    if args.site_model:
        if args.model_directory is not None:
            parser.error("--site-model and the shared --model-directory are mutually exclusive")
        site_directories = {}
        for specification in args.site_model:
            name, separator, directory = specification.partition("=")
            if not separator or not directory or name not in {site.value for site in ModelSite}:
                parser.error("--site-model requires SITE=DIRECTORY")
            site = ModelSite(name)
            if site in site_directories:
                parser.error("--site-model must name distinct sites")
            site_directories[site] = Path(directory)
    executor = None if args.executor is None else FixedCombatConfig(**json.loads(args.executor.read_text()))
    result = run_rollout(args.output, seed=args.seed, ascension=args.ascension,
                         final_act=args.goal == "heart", executor=executor,
                         max_steps=args.max_steps, capture_samples=args.capture_samples,
                         model_directory=args.model_directory,
                         model_sites=None if args.model_sites is None else
                             tuple(ModelSite(site) for site in args.model_sites),
                         site_model_directories=site_directories, card_reward_guide=args.card_reward_guide)
    print(f"{result['stop']}; Act1 completed={result['act1_completed']}; "
          f"combats={result['combats']}; elapsed={result['elapsed_seconds']:.2f}s")


if __name__ == "__main__":
    main()
