"""Bounded in-memory strategic bootstrap rows from the public baseline."""

from __future__ import annotations

import operator
import hashlib
import json
import time
from collections import Counter
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

import numpy as np
import torch

from .combat_potion_lane import CombatPotionLane
from .decision_progress import BridgeDecisionProgressProvider, DecisionRunProgress
from .evaluate_run import RunPotionLane, resolve_run_potion_lane
from .published_combat_behavior import (
    recover_compatible_combat_scorer,
    recover_published_combat_behavior,
)
from .semantic_batch import select_semantic_decision_rows
from .decision_rows import iter_payload_arrays
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_policy import (
    GreedyTorchPolicy,
    RaggedCategoricalPolicyConfig,
    sample_ragged_categorical,
)
from .torch_session_config import CategoricalSessionBridge


class StrategicDemonstrationError(RuntimeError):
    """A strategic bootstrap corpus could not be collected faithfully."""


class ScopedStrategicSampleRecorder:
    """Public observations and optional attempted choices, not expert labels.

    Captures attempts before native choice, and marks them committed only after
    the enclosing macro succeeds. Search interruptions supply no return target.
    """

    sites = frozenset(("card_reward", "campfire"))

    def __init__(self, *, sites=("card_reward", "campfire"), include_symbolic=False,
                 routine_resolver=None, max_rows=None, max_array_bytes=None):
        self.sites = None if sites is None else frozenset(sites)
        if type(include_symbolic) is not bool:
            raise StrategicDemonstrationError("include_symbolic must be boolean")
        if routine_resolver is not None and not callable(routine_resolver):
            raise StrategicDemonstrationError("routine resolver must be callable")
        self.include_symbolic = include_symbolic
        self.routine_resolver = routine_resolver
        self.routine_rows = 0
        self.max_rows = None if max_rows is None else _positive(max_rows, "max_rows")
        self.max_array_bytes = None if max_array_bytes is None else _positive(max_array_bytes, "max_array_bytes")
        self.array_bytes = 0
        self.rows = []
        self.committed = set()

    @staticmethod
    def _key(decision):
        return (decision["context"]["seed"], decision["slot"], decision["source_token"])

    def __call__(self, batch, decisions):
        if any(key.startswith("legality_bootstrap_") for key in batch):
            raise StrategicDemonstrationError("bootstrap outputs cannot be model inputs")
        if len(decisions) != len(batch["slot_indices"]):
            raise StrategicDemonstrationError("sample decisions are misaligned")
        routines = {} if self.routine_resolver is None else self.routine_resolver(batch)
        for row, decision in enumerate(decisions):
            if row in routines:
                self.routine_rows += 1
                continue
            audit = decision["audit"]
            if audit is None and not self.include_symbolic:
                continue
            if audit is not None and self.sites is not None and audit["decision_site"] not in self.sites:
                continue
            selected = select_semantic_decision_rows(batch, (row,))
            count = int(selected["candidate_counts"][0])
            if audit is not None and count != len(audit["candidates"]):
                raise StrategicDemonstrationError("sample candidates do not match the audit")
            if selected["decision_token_ids"][0] != decision["source_token"]:
                raise StrategicDemonstrationError("sample source token does not match")
            if selected["public_snapshot_ids"][0] != decision["public_snapshot"]:
                raise StrategicDemonstrationError("sample public snapshot does not match")
            ordinal = decision["ordinal"]
            if ordinal is None:
                if decision["owner"] != "observation_only":
                    raise StrategicDemonstrationError("unchosen sample requires observation-only ownership")
            elif (decision["owner"] == "observation_only" or type(ordinal) is not int
                  or not 0 <= ordinal < count):
                raise StrategicDemonstrationError("sample choice is inconsistent with its owner or candidates")
            size = sum(array.nbytes for array in iter_payload_arrays(selected))
            if (self.max_rows is not None and len(self.rows)+1 > self.max_rows
                    or self.max_array_bytes is not None and self.array_bytes+size > self.max_array_bytes):
                raise StrategicDemonstrationError("rule demonstrations exceeded the declared capacity")
            self.array_bytes += size
            self.rows.append((selected, json.loads(json.dumps(decision))))

    def commit(self, decisions):
        if any(decision["ordinal"] is None for decision in decisions):
            raise StrategicDemonstrationError("observation-only sample cannot be committed")
        self.committed.update(self._key(decision) for decision in decisions)

    def summary(self):
        sizes = [sum(array.nbytes for array in iter_payload_arrays(batch))
                 for batch, _ in self.rows]
        return {"attempted_rows": len(self.rows),
                "committed_rows": sum(self._key(decision) in self.committed
                                      for _, decision in self.rows),
                "observation_only_rows": sum(decision["ordinal"] is None for _, decision in self.rows),
                "routine_rows_excluded": self.routine_rows,
                "sites": dict(Counter(decision["audit"]["decision_site"] if decision["audit"] is not None else "symbolic"
                                      for _, decision in self.rows)),
                "array_bytes": sum(sizes), "max_row_array_bytes": max(sizes, default=0),
                "max_candidates": max((int(batch["candidate_counts"][0])
                                       for batch, _ in self.rows), default=0)}

    def save(self, output, *, schema, producer, terminal_outcome):
        output = Path(output)
        output.mkdir(parents=True, exist_ok=False)
        records = []
        for index, (batch, decision) in enumerate(self.rows):
            arrays = {}

            def flatten(value, path):
                if isinstance(value, Mapping):
                    for key, child in value.items():
                        flatten(child, (*path, key))
                else:
                    arrays["|".join(path)] = np.asarray(value)

            flatten(batch, ())
            path = output/f"row-{index:06d}.npz"
            np.savez_compressed(path, **arrays)
            records.append({"file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                            "committed": self._key(decision) in self.committed,
                            "decision": decision,
                            "label_source": ("observation_only" if decision["ordinal"] is None else
                                             "public_baseline" if decision["owner"] == "baseline" else
                                             "compiled_guide" if decision["owner"] == "guide" else "model"),
                            "expert_target": None})
        manifest = {"schema": "sts-scoped-strategic-samples-v3" if self.include_symbolic else "sts-scoped-strategic-samples-v2", "semantic_schema": schema,
                    "include_symbolic": self.include_symbolic,
                    "producer": producer, "summary": self.summary(), "rows": records,
                    "sampling_source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                    "terminal_outcome": terminal_outcome,
                    "scope": "committed actions are observations; adopted actions are not expert targets"}
        (output/"manifest.json").write_text(json.dumps(manifest, indent=2, allow_nan=False)+"\n",
                                           encoding="utf-8")


def collect_fixed_rule_demonstrations(runtime, seeds, output, *, partition,
                                     ascension, slots, max_steps, max_rows=8192,
                                     max_array_bytes=256*1024*1024, progress=print):
    """Record adopted baseline choices on the full learned strategic surface.

    Uses the maintained controller and fixed executor. Failed runs are retained;
    unresolved macros remain uncommitted and supply no imitation target.
    """
    from .fixed_combat_environment import FixedCombatUnresolved
    from .outcomes import TerminalStepBatch
    from .seeds import SeedPartition, SeedPartitionSpec
    from .semantic_concat import SemanticBatchConcatLimits, concatenate_semantic_decision_batches
    from .strategic_behavior_cloning import StrategicDemonstrationPartition
    from .strategic_rollout import StrategicRunController

    seeds = tuple(seeds)
    seed_partition = SeedPartitionSpec()
    if (not isinstance(partition, SeedPartition) or not seeds
            or len(set(seeds)) != len(seeds)
            or any(seed_partition.classify(seed) is not partition for seed in seeds)):
        raise StrategicDemonstrationError("rule demonstration seeds must belong to their declared partition")
    for name, value in (("slots", slots), ("max_steps", max_steps),
                        ("max_rows", max_rows), ("max_array_bytes", max_array_bytes)):
        _positive(value, name)
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    batches, targets, row_seeds, contexts = [], [], [], []
    outcomes = []
    array_bytes = 0
    started = time.perf_counter()
    for offset in range(0, len(seeds), slots):
        cohort = seeds[offset:offset+slots]
        env = runtime.environment(list(cohort), ascension)
        recorder = ScopedStrategicSampleRecorder(sites=None, include_symbolic=True,
            routine_resolver=env.routine_choice_ordinals,
            max_rows=max_rows-len(targets), max_array_bytes=max_array_bytes-array_bytes)
        def capture(batch, decisions):
            kinds = {int(slot): view.strategic_context_kind
                     for slot, view in env.public_run_contexts()}
            recorder(batch, [dict(decision,
                strategic_context_kind=kinds[decision["slot"]]) for decision in decisions])
        controller = StrategicRunController(env, decision_sink=capture)
        completed = {}
        reason = "step_limit"
        try:
            for _ in range(max_steps):
                result, decisions, _, _ = controller.advance()
                if not result.advanced:
                    reason = "executor_unresolved_batch"
                    raise FixedCombatUnresolved(result)
                recorder.commit(decisions)
                for outcome in TerminalStepBatch.from_bridge_step(
                        result.transitions, slot_count=len(cohort)).attempts:
                    if outcome.slot_index in completed:
                        raise StrategicDemonstrationError("duplicate rule trajectory terminal")
                    completed[outcome.slot_index] = outcome
                if len(completed) == len(cohort):
                    reason = "terminal"
                    break
            if reason != "terminal":
                raise StrategicDemonstrationError("rule demonstration cohort reached step limit")
        finally:
            recorder.save(output/f"cohort-{offset:05d}", schema=runtime.schema,
                producer={"executor_id": runtime.binding.identity,
                          "controller": controller.manifest,
                          "partition": partition.value, "planned_seeds": cohort},
                terminal_outcome={"stop_reason": reason,
                    "outcomes": [outcome.__dict__ for outcome in completed.values()]})
        for batch, decision in recorder.rows:
            if recorder._key(decision) not in recorder.committed:
                continue
            seed = decision["context"]["seed"]
            if decision["owner"] != "baseline" or seed != cohort[decision["slot"]]:
                raise StrategicDemonstrationError("rule demonstration source is not the planned baseline")
            if seed_partition.classify(seed) is not partition:
                raise StrategicDemonstrationError("rule demonstration partition changed")
            batches.append(batch)
            targets.append(decision["ordinal"])
            row_seeds.append(seed)
            # Symbolic phases can lack a strategic root context; code 0 is
            # reserved for those rows in agreement diagnostics.
            contexts.append(decision["strategic_context_kind"] or 0)
            array_bytes += sum(array.nbytes for array in iter_payload_arrays(batch))
            if len(targets) > max_rows or array_bytes > max_array_bytes:
                raise StrategicDemonstrationError("rule demonstrations exceeded the declared capacity")
        outcomes.extend({"seed": cohort[slot], **outcome.__dict__}
                        for slot, outcome in sorted(completed.items()))
        progress(f"Rule demonstrations {partition.value}: {offset+len(cohort)}/{len(seeds)} runs, {len(targets)} committed actor rows")
    partition_rows = StrategicDemonstrationPartition(
        concatenate_semantic_decision_batches(batches,
            SemanticBatchConcatLimits(max_rows, max_array_bytes)),
        tuple(targets), tuple(row_seeds), tuple(contexts))
    summary = {"partition": partition.value, "planned_seeds": seeds,
               "rows": partition_rows.row_count, "contexts": partition_rows.context_counts,
               "array_bytes": array_bytes, "elapsed_seconds": time.perf_counter()-started,
               "outcomes": outcomes, "label_source": "committed_public_baseline",
               "scope": "baseline imitation; all terminal outcomes retained; no optimal/expert claim"}
    (output/"summary.json").write_text(json.dumps(summary, indent=2)+"\n", encoding="utf-8")
    return partition_rows, summary


def load_scoped_strategic_samples(root):
    """Verify saved numerical rows; caller explicitly chooses label authority."""
    root = Path(root).resolve()
    manifest = json.loads((root/"manifest.json").read_text(encoding="utf-8"))
    if manifest["schema"] not in ("sts-scoped-strategic-samples-v1", "sts-scoped-strategic-samples-v2", "sts-scoped-strategic-samples-v3"):
        raise StrategicDemonstrationError("unsupported scoped sample manifest")
    for key in ("categorical_vocabulary_size", "identity_residual_categorical_fields"):
        manifest["semantic_schema"][key] = {
            int(field): value for field, value in manifest["semantic_schema"][key].items()}
    rows = []
    for index, record in enumerate(manifest["rows"]):
        if manifest["schema"] == "sts-scoped-strategic-samples-v1":
            record = dict(record)
            record["decision"] = record.pop("adopted")
        decision = record["decision"]
        ordinal = decision["ordinal"]
        observation_only = ordinal is None
        if observation_only and (manifest["schema"] == "sts-scoped-strategic-samples-v1"
                                 or decision["owner"] != "observation_only" or record["committed"]):
            raise StrategicDemonstrationError("unchosen sample must remain an uncommitted v2 observation")
        if not observation_only and (type(ordinal) is not int or decision["owner"] == "observation_only"):
            raise StrategicDemonstrationError("sample choice is inconsistent with its owner")
        if record["file"] != f"row-{index:06d}.npz":
            raise StrategicDemonstrationError("sample filename is not its declared ordinal")
        path = root/record["file"]
        if hashlib.sha256(path.read_bytes()).hexdigest() != record["sha256"]:
            raise StrategicDemonstrationError("sample hash mismatch")
        batch = {}
        with np.load(path, allow_pickle=False) as archive:
            for name in archive.files:
                keys = name.split("|")
                table = batch
                for key in keys[:-1]:
                    table = table.setdefault(key, {})
                value = archive[name]
                table[keys[-1]] = (value.item() if value.ndim == 0 else
                                  value.tolist() if value.dtype.kind in "US" else value.copy())
        batch = select_semantic_decision_rows(batch, (0,))
        audit = decision["audit"]
        if audit is None and (manifest["schema"] != "sts-scoped-strategic-samples-v3"
                              or manifest.get("include_symbolic") is not True):
            raise StrategicDemonstrationError("symbolic samples require an explicit v3 source")
        if (batch["semantic"]["schema_version"] != manifest["semantic_schema"]["version"]
                or batch["decision_token_ids"][0] != decision["source_token"]
                or batch["public_snapshot_ids"][0] != decision["public_snapshot"]
                or audit is not None and int(batch["candidate_counts"][0]) != len(audit["candidates"])
                or not observation_only and not 0 <= ordinal < int(batch["candidate_counts"][0])):
            raise StrategicDemonstrationError("sample source or candidate alignment mismatch")
        rows.append((batch, record))
    return manifest, tuple(rows)


@dataclass(frozen=True)
class ScopedStrategicPreferenceRow:
    decision_batch: Mapping[str, object]
    source_index: int
    row_index: int
    episode_seed: int
    context_kind: int
    committed: bool
    preferred_ordinals: tuple[int, ...]
    disfavored_ordinals: tuple[int, ...]
    rationale: str
    supervision_basis: str = "committed_observation"

    @property
    def eligible_for_supervision(self):
        return self.committed or self.supervision_basis == "public_observation"

    @property
    def preferred_pairs(self) -> tuple[tuple[int, int], ...]:
        return tuple((preferred, disfavored)
                     for preferred in self.preferred_ordinals
                     for disfavored in self.disfavored_ordinals)


@dataclass(frozen=True)
class ScopedStrategicPreferences:
    rows: tuple[ScopedStrategicPreferenceRow, ...]
    semantic_schema: Mapping[str, object]
    producer: Mapping[str, object]
    overlay: Mapping[str, object]
    overlay_sha256: str


@dataclass(frozen=True)
class ScopedStrategicDecisionRow:
    decision_batch: Mapping[str, object]
    source_index: int
    row_index: int
    episode_seed: int
    context_kind: int
    committed: bool
    acceptable_ordinals: tuple[int, ...] | None
    rationale: str
    supervision_basis: str = "committed_observation"
    preferred_ordinals: tuple[int, ...] | None = None

    @property
    def choice_target_ordinals(self):
        return self.preferred_ordinals if self.preferred_ordinals is not None else self.acceptable_ordinals

    @property
    def eligible_for_supervision(self):
        return self.committed or self.supervision_basis == "public_observation"


@dataclass(frozen=True)
class ScopedStrategicDecisions:
    rows: tuple[ScopedStrategicDecisionRow, ...]
    semantic_schema: Mapping[str, object]
    producer: Mapping[str, object]
    overlay: Mapping[str, object]
    overlay_sha256: str


def _scoped_annotation_sources(sample_roots):
    sources = []
    for root in sample_roots:
        root = Path(root).resolve()
        sources.append({"root": str(root), "manifest_sha256": hashlib.sha256(
            (root / "manifest.json").read_bytes()).hexdigest()})
    return sources


def _scoped_source_producer(manifests, sources, protocol, kind):
    producers = [manifest["producer"] for manifest in manifests]
    if any(not isinstance(producer, Mapping) for producer in producers):
        raise StrategicDemonstrationError(f"{kind} sample producer must be a mapping")
    declaration = protocol.get("source_compatibility")
    if declaration is None:
        if any(producer != producers[0] for producer in producers[1:]):
            raise StrategicDemonstrationError(f"{kind} sample producers or schemas differ")
        return producers[0]
    if (not isinstance(declaration, Mapping)
            or declaration.get("schema") != "sts-scoped-source-compatibility-v1"
            or not isinstance(declaration.get("rationale"), str)
            or not declaration["rationale"].strip()):
        raise StrategicDemonstrationError(f"{kind} source compatibility must be explicitly justified")
    native_digest = declaration.get("native_sha256")
    if (not isinstance(native_digest, str) or len(native_digest) != 64
            or any(character not in "0123456789abcdef" for character in native_digest)):
        raise StrategicDemonstrationError(f"{kind} source compatibility requires native_sha256")
    permissions = declaration.get("sources")
    if not isinstance(permissions, list) or len(permissions) != len(manifests):
        raise StrategicDemonstrationError(f"{kind} source permissions must cover every source")
    records = []
    for index, (manifest, source, producer, permission) in enumerate(
            zip(manifests, sources, producers, permissions, strict=True)):
        if (not isinstance(permission, Mapping)
                or type(permission.get("source_index")) is not int
                or permission["source_index"] != index
                or permission.get("input_access") != "public_semantic_only"
                or permission.get("origin") not in ("constructed", "natural_run", "fixture")
                or permission.get("continuation_access") not in (
                    "public_only", "fixed_state_oracle", "constructed_transition")
                or not isinstance(permission.get("rationale"), str)
                or not permission["rationale"].strip()):
            raise StrategicDemonstrationError(f"{kind} source permissions are incomplete or unsupported")
        sampling_digest = permission.get("sampling_source_sha256")
        if (not isinstance(sampling_digest, str) or len(sampling_digest) != 64
                or any(character not in "0123456789abcdef" for character in sampling_digest)):
            raise StrategicDemonstrationError(f"{kind} source permissions require sampling_source_sha256")
        native_digests = [producer.get("native_sha256")]
        binding = producer.get("executor_binding")
        if isinstance(binding, Mapping):
            native_digests.append(binding.get("native_sha256"))
        native_digests = [digest for digest in native_digests if digest is not None]
        if (not native_digests
                or any(digest != declaration["native_sha256"] for digest in native_digests)
                or manifest.get("sampling_source_sha256") != sampling_digest):
            raise StrategicDemonstrationError(f"{kind} source numerical producers differ from compatibility")
        records.append({"manifest_sha256": source["manifest_sha256"],
                        "producer": producer, "permissions": dict(permission)})
    return {"schema": "sts-scoped-multi-source-producer-v1",
            "native_sha256": declaration["native_sha256"],
            "compatibility": dict(declaration), "sources": records}


def _load_scoped_annotation_sources(sources, kind, protocol):
    loaded = []
    manifests = []
    schema = None
    seen = set()
    for source in sources:
        root = Path(source["root"]).resolve()
        if hashlib.sha256((root / "manifest.json").read_bytes()).hexdigest() != source["manifest_sha256"]:
            raise StrategicDemonstrationError(f"{kind} manifest hash mismatch")
        manifest, rows = load_scoped_strategic_samples(root)
        if schema is None:
            schema = manifest["semantic_schema"]
        if schema != manifest["semantic_schema"]:
            raise StrategicDemonstrationError(f"{kind} sample producers or schemas differ")
        for _, record in rows:
            decision = record["decision"]
            if decision["audit"]["decision_site"] not in ScopedStrategicSampleRecorder.sites:
                raise StrategicDemonstrationError(f"{kind} sample escapes declared scope")
            identity = (decision["context"]["seed"], decision["source_token"])
            if identity in seen:
                raise StrategicDemonstrationError(f"{kind} repeats a source decision")
            seen.add(identity)
        loaded.append(rows)
        manifests.append(manifest)
    if not any(loaded):
        raise StrategicDemonstrationError(f"{kind} overlay has no sample rows")
    producer = _scoped_source_producer(manifests, sources, protocol, kind)
    return loaded, schema, producer


def _annotation_index(value, bound, name, kind):
    if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value < bound:
        raise StrategicDemonstrationError(f"{kind} {name} ordinal is outside its source")
    return value


def _annotation_ordinals(values, count, kind):
    if not isinstance(values, (list, tuple)):
        raise StrategicDemonstrationError(f"{kind} ordinals must be a sequence")
    normalized = tuple(_annotation_index(value, count, "candidate", kind) for value in values)
    if len(set(normalized)) != len(normalized):
        raise StrategicDemonstrationError(f"duplicate {kind} candidate ordinal")
    return normalized


def _index_annotations(annotations, loaded, kind):
    judgments = {}
    for annotation in annotations:
        source = _annotation_index(annotation["source_index"], len(loaded), "source", kind)
        row = _annotation_index(annotation["row_index"], len(loaded[source]), "row", kind)
        if (source, row) in judgments:
            raise StrategicDemonstrationError(f"duplicate {kind} annotation")
        judgments[source, row] = annotation
    return judgments


def _bound_annotation(label, loaded, labels, kind):
    source = _annotation_index(label["source_index"], len(loaded), "source", kind)
    index = _annotation_index(label["row_index"], len(loaded[source]), "row", kind)
    batch, record = loaded[source][index]
    key = (source, index)
    if key in labels:
        raise StrategicDemonstrationError(f"duplicate {kind} annotation")
    if (label["row_sha256"] != record["sha256"]
            or label["source_token"] != record["decision"]["source_token"]):
        raise StrategicDemonstrationError(f"{kind} row source mismatch")
    rationale = label["rationale"]
    if not isinstance(rationale, str) or not rationale.strip():
        raise StrategicDemonstrationError(f"{kind} rationale must be recorded")
    return key, batch, record, rationale


def _supervision_basis(label, kind):
    basis = label.get("basis", "committed_observation")
    if basis not in ("committed_observation", "public_observation"):
        raise StrategicDemonstrationError(f"{kind} supervision basis is unsupported")
    return basis


def save_scoped_strategic_decisions(
    output, sample_roots, annotations, *, author, revision, references, protocol,
):
    """Save separately authored full-surface adoptable sets, never partial preferences.

    An annotation supplies source_index, row_index, acceptable (nonempty ordinal
    list or None) and rationale. Optional preferred is a nonempty subset of that
    complete acceptable set. Unannotated rows have no decision target.
    """
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(output)
    if not isinstance(protocol, Mapping) or not protocol:
        raise StrategicDemonstrationError("decision protocol must be declared")
    sources = _scoped_annotation_sources(sample_roots)
    loaded, _, _ = _load_scoped_annotation_sources(sources, "decision", protocol)
    judgments = _index_annotations(annotations, loaded, "decision")
    preferred_mode = any("preferred" in annotation for annotation in judgments.values())
    labels = []
    for source, rows in enumerate(loaded):
        for index, (_, record) in enumerate(rows):
            annotation = judgments.get((source, index))
            if annotation is not None and "acceptable" not in annotation:
                raise StrategicDemonstrationError("decision annotation requires an explicit acceptable set")
            labels.append({"source_index": source, "row_index": index,
                           "row_sha256": record["sha256"],
                           "source_token": record["decision"]["source_token"],
                           "acceptable": annotation["acceptable"] if annotation else None,
                           "basis": annotation.get("basis", "committed_observation") if annotation else "committed_observation",
                           "rationale": annotation["rationale"] if annotation else
                           "Unknown: no complete decision target has been authored."})
            if preferred_mode:
                labels[-1]["preferred"] = annotation.get("preferred") if annotation else None
    overlay = {"schema": "sts-scoped-strategic-decisions-v2" if preferred_mode else "sts-scoped-strategic-decisions-v1", "author": author,
               "revision": revision, "references": references, "protocol": dict(protocol),
               "sources": sources, "rows": labels,
               "authority": "named teacher's adoptable actions over the complete candidate surface"}
    _decode_scoped_strategic_decisions(overlay, "")
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(overlay, indent=2, ensure_ascii=False, allow_nan=False) + "\n")


def load_scoped_strategic_decisions(path) -> ScopedStrategicDecisions:
    payload = Path(path).read_bytes()
    return _decode_scoped_strategic_decisions(json.loads(payload), hashlib.sha256(payload).hexdigest())


def _decode_scoped_strategic_decisions(overlay, digest):
    if overlay.get("schema") not in {"sts-scoped-strategic-decisions-v1", "sts-scoped-strategic-decisions-v2"}:
        raise StrategicDemonstrationError("unsupported scoped decision overlay")
    preferred_mode = overlay["schema"] == "sts-scoped-strategic-decisions-v2"
    for field in ("author", "revision"):
        if not isinstance(overlay.get(field), str) or not overlay[field].strip():
            raise StrategicDemonstrationError(f"decision {field} must be identified")
    if not isinstance(overlay.get("protocol"), dict) or not overlay["protocol"]:
        raise StrategicDemonstrationError("decision protocol must be declared")
    loaded, schema, producer = _load_scoped_annotation_sources(
        overlay["sources"], "decision", overlay["protocol"])
    labels = {}
    for label in overlay["rows"]:
        key, batch, record, rationale = _bound_annotation(label, loaded, labels, "decision")
        basis = _supervision_basis(label, "decision")
        if "acceptable" not in label:
            raise StrategicDemonstrationError("decision annotation requires an explicit acceptable set")
        acceptable = label["acceptable"]
        if acceptable is not None:
            acceptable = _annotation_ordinals(acceptable, int(batch["candidate_counts"][0]), "decision")
            if not acceptable:
                raise StrategicDemonstrationError("decision acceptable set must be nonempty or null")
            if not record["committed"] and basis != "public_observation":
                raise StrategicDemonstrationError("uncommitted sample cannot receive decision labels")
        if (preferred_mode and "preferred" not in label) or (not preferred_mode and "preferred" in label):
            raise StrategicDemonstrationError("decision preferred field disagrees with overlay schema")
        preferred = label.get("preferred")
        if preferred is not None:
            preferred = _annotation_ordinals(preferred, int(batch["candidate_counts"][0]), "preferred decision")
            if not preferred or acceptable is None or not set(preferred) <= set(acceptable):
                raise StrategicDemonstrationError("decision preferred set requires a nonempty subset of the complete acceptable set")
        labels[key] = (acceptable, rationale, basis, preferred)
    materialized = []
    for source, rows in enumerate(loaded):
        for index, (batch, record) in enumerate(rows):
            decision = record["decision"]
            acceptable, rationale, basis, preferred = labels.get(
                (source, index), (None, "Unknown: no complete decision target has been authored.", "committed_observation", None))
            materialized.append(ScopedStrategicDecisionRow(
                batch, source, index, decision["context"]["seed"],
                0 if decision["audit"]["decision_site"] == "card_reward" else 1,
                record["committed"], acceptable, rationale, basis, preferred))
    return ScopedStrategicDecisions(tuple(materialized), schema, producer, overlay, digest)


def save_scoped_strategic_preferences(
    output, sample_roots, annotations, *, reviewer, references, protocol,
):
    """Bind independent reviewer judgments to immutable public sample sources.

    Each annotation supplies source_index, row_index, preferred, disfavored and
    rationale. Unannotated rows and candidates remain Unknown. Preferred sets
    are unordered; disfavored means worse in these comparisons, not unusable.
    """
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(output)
    if not isinstance(reviewer, str) or not reviewer.strip():
        raise StrategicDemonstrationError("preference reviewer must be identified")
    if not isinstance(protocol, Mapping) or not protocol:
        raise StrategicDemonstrationError("preference protocol must be declared")
    sources = _scoped_annotation_sources(sample_roots)
    loaded, _, _ = _load_scoped_annotation_sources(sources, "preference", protocol)
    judgments = _index_annotations(annotations, loaded, "preference")
    labels = []
    for source, rows in enumerate(loaded):
        for index, (_, record) in enumerate(rows):
            annotation = judgments.get((source, index), {})
            candidates = record["decision"]["audit"]["candidates"]
            preferred = _annotation_ordinals(annotation.get("preferred", []), len(candidates), "preference")
            disfavored = _annotation_ordinals(annotation.get("disfavored", []), len(candidates), "preference")
            rationale = annotation.get("rationale", "Unknown: this row has not been reviewed.")
            labels.append({"source_index": source, "row_index": index,
                           "row_sha256": record["sha256"],
                           "source_token": record["decision"]["source_token"],
                           "basis": annotation.get("basis", "committed_observation"),
                           "preferred": list(preferred), "disfavored": list(disfavored),
                           "unknown": sorted(set(range(len(candidates)))
                                             - set(preferred) - set(disfavored)),
                           "rationale": rationale})
    overlay = {"schema": "sts-scoped-strategic-preferences-v1", "reviewer": reviewer,
               "references": references, "protocol": dict(protocol),
               "sources": sources, "rows": labels,
               "authority": "reviewer assessment; not original guide-author labels or outcome proof"}
    # Validate before publishing, including overlaps and labels on pending actions.
    _decode_scoped_strategic_preferences(overlay, "")
    payload = json.dumps(overlay, indent=2, ensure_ascii=False, allow_nan=False) + "\n"
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        stream.write(payload)


def load_scoped_strategic_preferences(path) -> ScopedStrategicPreferences:
    payload = Path(path).read_bytes()
    return _decode_scoped_strategic_preferences(
        json.loads(payload), hashlib.sha256(payload).hexdigest())


def _decode_scoped_strategic_preferences(overlay, digest):
    if overlay.get("schema") != "sts-scoped-strategic-preferences-v1":
        raise StrategicDemonstrationError("unsupported scoped preference overlay")
    if not isinstance(overlay.get("reviewer"), str) or not overlay["reviewer"].strip():
        raise StrategicDemonstrationError("preference reviewer must be identified")
    if not isinstance(overlay.get("protocol"), dict) or not overlay["protocol"]:
        raise StrategicDemonstrationError("preference protocol must be declared")
    loaded, schema, producer = _load_scoped_annotation_sources(
        overlay["sources"], "preference", overlay["protocol"])
    labels = {}
    for label in overlay["rows"]:
        key, batch, record, rationale = _bound_annotation(label, loaded, labels, "preference")
        basis = _supervision_basis(label, "preference")
        count = int(batch["candidate_counts"][0])
        preferred = _annotation_ordinals(label["preferred"], count, "preference")
        disfavored = _annotation_ordinals(label["disfavored"], count, "preference")
        unknown = _annotation_ordinals(label["unknown"], count, "preference")
        groups = tuple(map(set, (preferred, disfavored, unknown)))
        if (groups[0] & groups[1] or groups[0] & groups[2] or groups[1] & groups[2]
                or set.union(*groups) != set(range(count))):
            raise StrategicDemonstrationError("preference labels conflict or omit candidates")
        if not record["committed"] and basis != "public_observation" and (preferred or disfavored):
            raise StrategicDemonstrationError("uncommitted sample cannot receive preference labels")
        labels[key] = (preferred, disfavored, rationale, basis)
    materialized = []
    for source, rows in enumerate(loaded):
        for index, (batch, record) in enumerate(rows):
            decision = record["decision"]
            preferred, disfavored, rationale, basis = labels.get(
                (source, index), ((), (), "Unknown: this row has not been reviewed.", "committed_observation"))
            materialized.append(ScopedStrategicPreferenceRow(
                batch, source, index, decision["context"]["seed"],
                0 if decision["audit"]["decision_site"] == "card_reward" else 1,
                record["committed"], preferred, disfavored, rationale, basis))
    if not materialized:
        raise StrategicDemonstrationError("preference overlay has no sample rows")
    return ScopedStrategicPreferences(tuple(materialized), schema, producer, overlay, digest)


class CombatAnchorMode(Enum):
    """How the collector obtains immutable combat scorer weights."""

    STRICT_PUBLICATION = "strict_publication"
    COMPATIBLE_WEIGHT_IMPORT = "compatible_weight_import"


@dataclass(frozen=True)
class CombatRetryCoverageConfig:
    """Bounded combat-root retries used only to widen corpus coverage."""

    max_retries_per_combat: int
    sampling_seed: int = 300_000
    temperature: float = 1.0

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "max_retries_per_combat",
            _positive(self.max_retries_per_combat, "max_retries_per_combat"),
        )
        sampling_seed = _nonnegative(self.sampling_seed, "sampling_seed")
        if sampling_seed >= 1 << 63:
            raise StrategicDemonstrationError("sampling_seed must be below 2^63")
        object.__setattr__(self, "sampling_seed", sampling_seed)
        try:
            policy = RaggedCategoricalPolicyConfig(self.temperature)
        except RuntimeError as error:
            raise StrategicDemonstrationError(str(error)) from error
        object.__setattr__(self, "temperature", policy.temperature)


@dataclass(frozen=True)
class StrategicDemonstrationConfig:
    behavior: Path
    ascension_level: int
    training_seed_start: int
    run_count: int
    slot_count: int = 16
    behavior_seed: int = 200_000
    max_batch_steps_per_cohort: int = 4_096
    max_decision_rounds_per_cohort: int = 32_768
    max_rows: int = 4_096
    max_array_bytes: int = 64 * 1024 * 1024
    wall_ms: int = 60_000
    potion_lane: RunPotionLane = RunPotionLane.TRAINED
    combat_anchor_mode: CombatAnchorMode = CombatAnchorMode.STRICT_PUBLICATION
    combat_retry_coverage: CombatRetryCoverageConfig | None = None

    def __post_init__(self) -> None:
        behavior = Path(self.behavior).resolve()
        if not behavior.is_dir():
            raise StrategicDemonstrationError(
                "demonstration behavior must be a published directory"
            )
        object.__setattr__(self, "behavior", behavior)
        for name in (
            "run_count",
            "slot_count",
            "max_batch_steps_per_cohort",
            "max_decision_rounds_per_cohort",
            "max_rows",
            "max_array_bytes",
            "wall_ms",
        ):
            object.__setattr__(self, name, _positive(getattr(self, name), name))
        if self.slot_count > self.run_count:
            object.__setattr__(self, "slot_count", self.run_count)
        ascension = _nonnegative(self.ascension_level, "ascension_level")
        if ascension > 20:
            raise StrategicDemonstrationError(
                "ascension_level must be at most 20"
            )
        object.__setattr__(self, "ascension_level", ascension)
        for name in ("training_seed_start", "behavior_seed"):
            value = _nonnegative(getattr(self, name), name)
            if value >= 1 << 63:
                raise StrategicDemonstrationError(f"{name} must be below 2^63")
            object.__setattr__(self, name, value)
        if not isinstance(self.potion_lane, RunPotionLane):
            raise StrategicDemonstrationError("potion_lane must be typed")
        if not isinstance(self.combat_anchor_mode, CombatAnchorMode):
            raise StrategicDemonstrationError("combat_anchor_mode must be typed")
        if self.combat_retry_coverage is not None and not isinstance(
            self.combat_retry_coverage,
            CombatRetryCoverageConfig,
        ):
            raise StrategicDemonstrationError("combat_retry_coverage must be typed")


@dataclass(frozen=True)
class StrategicDemonstrationBatch:
    decision_batch: Mapping[str, object]
    target_ordinals: tuple[int, ...]
    episode_seeds: tuple[int, ...]
    acts: tuple[int, ...]
    floors: tuple[int, ...]
    context_kinds: tuple[int, ...]
    array_bytes: int

    @property
    def row_count(self) -> int:
        return len(self.target_ordinals)


@dataclass(frozen=True)
class StrategicDemonstrationCorpus:
    batches: tuple[StrategicDemonstrationBatch, ...]
    requested_runs: int
    completed_runs: int
    victories: int
    defeats: int
    batch_steps: int
    decision_rounds: int
    teacher_rows: int
    combat_rows: int
    strategic_selection_rows: int
    unavailable_strategic_root_rows: int
    array_bytes: int
    elapsed_seconds: float
    stop_reason: str
    combat_anchor_mode: CombatAnchorMode
    combat_anchor_provenance_mismatches: tuple[str, ...]
    terminal_episode_seeds: tuple[int, ...]
    terminal_rewards: tuple[int, ...]
    terminal_acts: tuple[int, ...]
    terminal_floors: tuple[int, ...]
    terminal_hps: tuple[int, ...]
    terminal_max_hps: tuple[int, ...]
    combat_retries: int
    rescued_combats: int
    terminal_combat_retries: tuple[int, ...]

    def __post_init__(self) -> None:
        terminal_columns = (
            self.terminal_episode_seeds,
            self.terminal_rewards,
            self.terminal_acts,
            self.terminal_floors,
            self.terminal_hps,
            self.terminal_max_hps,
        )
        if any(len(column) != self.completed_runs for column in terminal_columns):
            raise StrategicDemonstrationError(
                "terminal outcome columns are misaligned with completed runs"
            )
        if len(set(self.terminal_episode_seeds)) != self.completed_runs:
            raise StrategicDemonstrationError("terminal run seeds are not unique")
        if any(reward not in {-1, 1} for reward in self.terminal_rewards):
            raise StrategicDemonstrationError("terminal rewards must be exact outcomes")
        if self.victories != sum(reward > 0 for reward in self.terminal_rewards):
            raise StrategicDemonstrationError("terminal victory accounting disagrees")
        if self.defeats != sum(reward < 0 for reward in self.terminal_rewards):
            raise StrategicDemonstrationError("terminal defeat accounting disagrees")
        object.__setattr__(
            self,
            "combat_retries",
            _nonnegative(self.combat_retries, "combat_retries"),
        )
        object.__setattr__(
            self,
            "rescued_combats",
            _nonnegative(self.rescued_combats, "rescued_combats"),
        )
        if len(self.terminal_combat_retries) != self.completed_runs:
            raise StrategicDemonstrationError(
                "terminal combat-retry counts are misaligned"
            )
        if any(retries < 0 for retries in self.terminal_combat_retries):
            raise StrategicDemonstrationError(
                "terminal combat-retry counts must be non-negative"
            )
        if sum(self.terminal_combat_retries) > self.combat_retries:
            raise StrategicDemonstrationError(
                "terminal retry accounting exceeds all combat retries"
            )

    @property
    def context_counts(self) -> dict[int, int]:
        counts = Counter(
            context
            for batch in self.batches
            for context in batch.context_kinds
        )
        return dict(sorted(counts.items()))

    @property
    def decisions_per_second(self) -> float:
        if self.elapsed_seconds <= 0.0:
            return 0.0
        return self.decision_rounds / self.elapsed_seconds

    @property
    def terminal_floor_counts(self) -> dict[tuple[int, int], int]:
        return dict(sorted(Counter(zip(self.terminal_acts, self.terminal_floors)).items()))


def collect_strategic_demonstrations(
    config: StrategicDemonstrationConfig,
    *,
    combat_bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
) -> StrategicDemonstrationCorpus:
    """Follow the public baseline and retain its strategic initialization rows."""

    if not isinstance(config, StrategicDemonstrationConfig):
        raise StrategicDemonstrationError("demonstration config must be typed")
    active_combat_bridge = combat_bridge or CombatSessionBridge.installed()
    active_run_bridge = run_bridge or CategoricalSessionBridge.installed()
    if active_combat_bridge.semantic_schema != active_run_bridge.semantic_schema:
        raise StrategicDemonstrationError(
            "combat behavior and run environment semantic schemas differ"
        )
    if config.combat_anchor_mode is CombatAnchorMode.STRICT_PUBLICATION:
        recovered = recover_published_combat_behavior(
            config.behavior,
            active_combat_bridge,
            CombatWinSessionLimits(),
            (config.behavior_seed,),
        )
        combat_policy = GreedyTorchPolicy(
            recovered.policies[0].frozen_scorer,
            recovered.manifest_id,
        )
        anchor_mismatches: tuple[str, ...] = ()
    else:
        recovered = recover_compatible_combat_scorer(
            config.behavior,
            active_combat_bridge,
            CombatWinSessionLimits(),
        )
        combat_policy = GreedyTorchPolicy(
            recovered.scorer,
            recovered.source_manifest_id,
        )
        anchor_mismatches = recovered.provenance_mismatches
    potion_lane = resolve_run_potion_lane(config.potion_lane, recovered)
    environment_factory = (
        active_run_bridge.environment
        if potion_lane is CombatPotionLane.ALL
        else active_run_bridge.environment_without_combat_potions
    )
    try:
        from sts_learning_bridge import PHASE_SELECTION, PHASE_STRATEGIC_ROOT
    except ImportError as error:
        raise StrategicDemonstrationError(
            "installed bridge does not expose production behavior phases"
        ) from error

    batches: list[StrategicDemonstrationBatch] = []
    completed_runs = 0
    victories = 0
    defeats = 0
    batch_steps = 0
    decision_rounds = 0
    teacher_rows = 0
    combat_rows = 0
    strategic_selection_rows = 0
    unavailable_strategic_root_rows = 0
    array_bytes = 0
    terminal_episode_seeds: list[int] = []
    terminal_rewards: list[int] = []
    terminal_acts: list[int] = []
    terminal_floors: list[int] = []
    terminal_hps: list[int] = []
    terminal_max_hps: list[int] = []
    terminal_combat_retries: list[int] = []
    combat_retries = 0
    rescued_combats = 0
    stop_reason = "completed_runs"
    started = time.perf_counter()
    next_seed = config.training_seed_start
    retry_config = config.combat_retry_coverage
    retry_policy_config = (
        None
        if retry_config is None
        else RaggedCategoricalPolicyConfig(retry_config.temperature)
    )
    retry_generator = (
        None
        if retry_config is None
        else torch.Generator(device="cpu").manual_seed(retry_config.sampling_seed)
    )

    while completed_runs < config.run_count:
        if _deadline_reached(started, config.wall_ms):
            stop_reason = "wall_ms"
            break
        cohort_size = min(config.slot_count, config.run_count - completed_runs)
        seeds = list(range(next_seed, next_seed + cohort_size))
        next_seed += cohort_size
        env = environment_factory(seeds, config.ascension_level)
        provider = BridgeDecisionProgressProvider(env)
        combat_root_keys: dict[int, tuple[int, int, int]] = {}
        combat_root_checkpoints: dict[int, object] = {}
        current_combat_retries = [0] * cohort_size
        run_combat_retries = [0] * cohort_size
        cohort_steps = 0
        cohort_rounds = 0
        while env.terminal_count < env.slot_count:
            if _deadline_reached(started, config.wall_ms):
                stop_reason = "wall_ms"
                break
            if cohort_steps >= config.max_batch_steps_per_cohort:
                stop_reason = "batch_steps"
                break
            if cohort_rounds >= config.max_decision_rounds_per_cohort:
                stop_reason = "decision_rounds"
                break
            batch = env.decision_batch(
                semantic=True,
                legality_bootstrap=True,
            )
            slots = _integer_vector(batch, "slot_indices")
            phases = _integer_vector(batch, "phase")
            available = _bool_vector(batch, "legality_bootstrap_available")
            targets = _integer_vector(batch, "legality_bootstrap_ordinals")
            if not (
                slots.shape == phases.shape == available.shape == targets.shape
            ):
                raise StrategicDemonstrationError(
                    "deterministic baseline columns are misaligned"
                )
            progress = provider.capture(tuple(map(int, slots)))
            if len(progress) != slots.size:
                raise StrategicDemonstrationError(
                    "deterministic baseline progress is misaligned"
                )
            strategic_baseline = available & (phases == PHASE_STRATEGIC_ROOT)
            if retry_config is not None:
                checkpoint_slots = getattr(env, "checkpoint_slots", None)
                restore_slots = getattr(env, "restore_slots", None)
                if not callable(checkpoint_slots) or not callable(restore_slots):
                    raise StrategicDemonstrationError(
                        "combat retry coverage requires checkpoint_slots/restore_slots"
                    )
                for index, row in enumerate(progress):
                    slot = int(slots[index])
                    if not row.is_combat:
                        if slot in combat_root_checkpoints:
                            if current_combat_retries[slot] > 0:
                                rescued_combats += 1
                            combat_root_checkpoints.pop(slot)
                            combat_root_keys.pop(slot)
                            current_combat_retries[slot] = 0
                        continue
                    key = (row.episode_seed, row.act, row.floor)
                    if combat_root_keys.get(slot) == key:
                        continue
                    if slot in combat_root_checkpoints and current_combat_retries[slot] > 0:
                        rescued_combats += 1
                    checkpoint = checkpoint_slots([slot])
                    if len(checkpoint) != 1:
                        raise StrategicDemonstrationError(
                            "combat root checkpoint did not contain exactly one slot"
                        )
                    combat_root_keys[slot] = key
                    combat_root_checkpoints[slot] = checkpoint
                    current_combat_retries[slot] = 0

            available_rows = tuple(
                int(row) for row in np.flatnonzero(strategic_baseline)
            )
            remaining_rows = config.max_rows - teacher_rows
            if available_rows and remaining_rows <= 0:
                stop_reason = "max_rows"
                break
            if len(available_rows) > remaining_rows:
                available_rows = available_rows[:remaining_rows]
                stop_reason = "max_rows"
            if available_rows:
                demonstration = _demonstration_batch(
                    batch,
                    progress,
                    targets,
                    available_rows,
                )
                if array_bytes + demonstration.array_bytes > config.max_array_bytes:
                    stop_reason = "max_array_bytes"
                    break
                batches.append(demonstration)
                teacher_rows += demonstration.row_count
                array_bytes += demonstration.array_bytes

            combat_rows += sum(row.is_combat for row in progress)
            strategic_selection_rows += sum(
                not row.is_combat and int(phases[index]) == PHASE_SELECTION
                for index, row in enumerate(progress)
            )
            unavailable_strategic_root_rows += sum(
                not row.is_combat
                and int(phases[index]) == PHASE_STRATEGIC_ROOT
                and not bool(available[index])
                for index, row in enumerate(progress)
            )

            if retry_config is None:
                ordinals = list(combat_policy.choose(batch).ordinals)
            else:
                assert retry_policy_config is not None
                assert retry_generator is not None
                with torch.inference_mode():
                    logits = combat_policy.scorer(batch)
                    ordinals = logits.greedy_ordinals()
                    if any(
                        row.is_combat and current_combat_retries[int(slots[index])] > 0
                        for index, row in enumerate(progress)
                    ):
                        sampled = sample_ragged_categorical(
                            logits,
                            retry_policy_config,
                            retry_generator,
                        )
                        for index, row in enumerate(progress):
                            if (
                                row.is_combat
                                and current_combat_retries[int(slots[index])] > 0
                            ):
                                ordinals[index] = sampled.ordinals[index]
            for index, row in enumerate(progress):
                if strategic_baseline[index]:
                    ordinals[index] = int(targets[index])
                elif not row.is_combat:
                    ordinals[index] = 0
            env.choose(list(batch["decision_token_ids"]), ordinals)
            decision_rounds += slots.size
            cohort_rounds += slots.size
            if env.ready:
                step = env.step()
                terminal_slots = _integer_vector(step, "terminal_slot_indices")
                rewards = _integer_vector(step, "terminal_reward")
                acts = _integer_vector(step, "terminal_act")
                floors = _integer_vector(step, "terminal_floor")
                hps = _integer_vector(step, "terminal_hp")
                max_hps = _integer_vector(step, "terminal_max_hp")
                if not (
                    terminal_slots.shape
                    == rewards.shape
                    == acts.shape
                    == floors.shape
                    == hps.shape
                    == max_hps.shape
                ):
                    raise StrategicDemonstrationError(
                        "terminal outcome columns are misaligned"
                    )
                if any(int(slot) >= len(seeds) for slot in terminal_slots):
                    raise StrategicDemonstrationError(
                        "terminal outcome references an unknown cohort slot"
                    )
                accepted_positions: list[int] = []
                for position, raw_slot in enumerate(terminal_slots):
                    slot = int(raw_slot)
                    reward = int(rewards[position])
                    if retry_config is not None and reward < 0:
                        checkpoint = combat_root_checkpoints.get(slot)
                        if checkpoint is None:
                            raise StrategicDemonstrationError(
                                "terminal combat defeat has no retry root checkpoint"
                            )
                        if (
                            current_combat_retries[slot]
                            < retry_config.max_retries_per_combat
                        ):
                            env.restore_slots([slot], checkpoint)
                            current_combat_retries[slot] += 1
                            run_combat_retries[slot] += 1
                            combat_retries += 1
                            continue
                    accepted_positions.append(position)
                    if retry_config is not None:
                        if current_combat_retries[slot] > 0 and reward > 0:
                            rescued_combats += 1
                        combat_root_checkpoints.pop(slot, None)
                        combat_root_keys.pop(slot, None)
                        current_combat_retries[slot] = 0
                    terminal_combat_retries.append(run_combat_retries[slot])
                terminal_episode_seeds.extend(
                    seeds[int(terminal_slots[position])]
                    for position in accepted_positions
                )
                terminal_rewards.extend(int(rewards[position]) for position in accepted_positions)
                terminal_acts.extend(int(acts[position]) for position in accepted_positions)
                terminal_floors.extend(int(floors[position]) for position in accepted_positions)
                terminal_hps.extend(int(hps[position]) for position in accepted_positions)
                terminal_max_hps.extend(
                    int(max_hps[position]) for position in accepted_positions
                )
                completed_runs += len(accepted_positions)
                victories += sum(int(rewards[position]) > 0 for position in accepted_positions)
                defeats += sum(int(rewards[position]) < 0 for position in accepted_positions)
                batch_steps += 1
                cohort_steps += 1
            if stop_reason in {"max_rows", "max_array_bytes"}:
                break
        if stop_reason != "completed_runs":
            break

    elapsed = time.perf_counter() - started
    return StrategicDemonstrationCorpus(
        batches=tuple(batches),
        requested_runs=config.run_count,
        completed_runs=completed_runs,
        victories=victories,
        defeats=defeats,
        batch_steps=batch_steps,
        decision_rounds=decision_rounds,
        teacher_rows=teacher_rows,
        combat_rows=combat_rows,
        strategic_selection_rows=strategic_selection_rows,
        unavailable_strategic_root_rows=unavailable_strategic_root_rows,
        array_bytes=array_bytes,
        elapsed_seconds=elapsed,
        stop_reason=stop_reason,
        combat_anchor_mode=config.combat_anchor_mode,
        combat_anchor_provenance_mismatches=anchor_mismatches,
        terminal_episode_seeds=tuple(terminal_episode_seeds),
        terminal_rewards=tuple(terminal_rewards),
        terminal_acts=tuple(terminal_acts),
        terminal_floors=tuple(terminal_floors),
        terminal_hps=tuple(terminal_hps),
        terminal_max_hps=tuple(terminal_max_hps),
        combat_retries=combat_retries,
        rescued_combats=rescued_combats,
        terminal_combat_retries=tuple(terminal_combat_retries),
    )


def _demonstration_batch(
    batch: Mapping[str, object],
    progress: Sequence[DecisionRunProgress],
    targets: np.ndarray,
    rows: tuple[int, ...],
) -> StrategicDemonstrationBatch:
    model_batch = {
        key: batch[key]
        for key in (
            "slot_indices",
            "phase",
            "candidate_counts",
            "candidate_row_splits",
            "semantic",
        )
    }
    if "dense_action_mask" in batch:
        model_batch["dense_action_mask"] = batch["dense_action_mask"]
    selected = select_semantic_decision_rows(model_batch, rows)
    target_ordinals = tuple(int(targets[row]) for row in rows)
    candidate_counts = _integer_vector(selected, "candidate_counts")
    if any(
        target < 0 or target >= int(candidate_counts[index])
        for index, target in enumerate(target_ordinals)
    ):
        raise StrategicDemonstrationError(
            "production behavior ordinal is outside its candidate row"
        )
    selected_progress = tuple(progress[row] for row in rows)
    contexts = tuple(row.strategic_context_kind for row in selected_progress)
    if any(context is None for context in contexts):
        raise StrategicDemonstrationError(
            "teacher row has no strategic context kind"
        )
    return StrategicDemonstrationBatch(
        decision_batch=selected,
        target_ordinals=target_ordinals,
        episode_seeds=tuple(row.episode_seed for row in selected_progress),
        acts=tuple(row.act for row in selected_progress),
        floors=tuple(row.floor for row in selected_progress),
        context_kinds=tuple(int(context) for context in contexts),
        array_bytes=_array_bytes(selected),
    )


def _array_bytes(value: object) -> int:
    if isinstance(value, np.ndarray):
        return value.nbytes
    if isinstance(value, Mapping):
        return sum(_array_bytes(item) for item in value.values())
    return 0


def _integer_vector(source: Mapping[str, object], key: str) -> np.ndarray:
    value = source.get(key)
    if not isinstance(value, np.ndarray) or value.ndim != 1:
        raise StrategicDemonstrationError(f"{key} must be a NumPy vector")
    if value.dtype.kind not in "iu":
        raise StrategicDemonstrationError(f"{key} must contain integers")
    return value


def _bool_vector(source: Mapping[str, object], key: str) -> np.ndarray:
    value = source.get(key)
    if not isinstance(value, np.ndarray) or value.ndim != 1:
        raise StrategicDemonstrationError(f"{key} must be a NumPy vector")
    if value.dtype != np.bool_:
        raise StrategicDemonstrationError(f"{key} must contain booleans")
    return value


def _deadline_reached(started: float, wall_ms: int) -> bool:
    return (time.perf_counter() - started) * 1_000 >= wall_ms


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise StrategicDemonstrationError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise StrategicDemonstrationError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise StrategicDemonstrationError(f"{name} must be an integer") from error
    if normalized < 0:
        raise StrategicDemonstrationError(f"{name} must be non-negative")
    return normalized
