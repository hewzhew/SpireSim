"""Measure natural combat-entry identity at model and public-history boundaries."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
import time
from collections import Counter
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

import numpy as np

from .decision_progress import BridgeDecisionProgressProvider, PublicDecisionSnapshot
from .public_decision_prefix import (
    PublicDecisionPrefixStepV1,
    public_combat_entry_prefix_id_v1,
    selected_public_prefix_step_v1,
)
from .run_resource_trace import RunPublicContext
from .seeds import SeedPartition, SeedPartitionSpec, SeedSchedule
from .semantic_information_state import semantic_policy_row_identity
from .torch_session_config import CategoricalSessionBridge


class CombatInformationCensusError(RuntimeError):
    """Natural-root census state or bridge output was malformed."""


class _CensusEnvironment(Protocol):
    slot_count: int

    def decision_batch(
        self,
        *,
        semantic: bool = False,
        legality_bootstrap: bool = False,
    ) -> Mapping[str, object]: ...

    def public_run_contexts(self) -> Sequence[object]: ...

    def public_information_snapshots(self) -> Sequence[object]: ...

    def combat_root_artifact_bytes(
        self,
        slot_indices: list[int],
        *,
        max_bytes: int,
    ) -> bytes: ...

    def choose(self, ordinals: list[int]) -> None: ...

    def step(self) -> Mapping[str, object]: ...


@dataclass(frozen=True)
class CombatInformationCensusConfig:
    output: Path
    seed_start: int
    seed_count: int
    ascension_level: int
    slot_count: int = 32
    partition: SeedPartition = SeedPartition.TRAINING
    partition_spec: SeedPartitionSpec = SeedPartitionSpec()
    max_progress_steps: int = 16
    wall_ms: int = 60_000
    selection_particles: int = 4
    evaluation_particles: int = 4
    retained_seeds_per_group: int = 16
    relation_layers: int = 2
    root_artifact: Path | None = None
    root_count: int | None = None
    max_artifact_bytes: int = 16 * 1024 * 1024

    def __post_init__(self) -> None:
        output = Path(self.output).resolve()
        if output.exists():
            raise CombatInformationCensusError("census output must be a fresh file")
        if not output.parent.is_dir():
            raise CombatInformationCensusError("census output parent must exist")
        object.__setattr__(self, "output", output)
        root_artifact = (
            None if self.root_artifact is None else Path(self.root_artifact).resolve()
        )
        if (root_artifact is None) != (self.root_count is None):
            raise CombatInformationCensusError(
                "root_artifact and root_count must be provided together"
            )
        if root_artifact is not None:
            if root_artifact.exists():
                raise CombatInformationCensusError(
                    "root artifact output must be a fresh file"
                )
            if not root_artifact.parent.is_dir():
                raise CombatInformationCensusError(
                    "root artifact output parent must exist"
                )
            if root_artifact == output:
                raise CombatInformationCensusError(
                    "census summary and root artifact outputs must differ"
                )
            object.__setattr__(
                self,
                "root_count",
                _positive(self.root_count, "root_count"),
            )
        object.__setattr__(self, "root_artifact", root_artifact)
        object.__setattr__(self, "seed_start", _seed(self.seed_start, "seed_start"))
        object.__setattr__(self, "seed_count", _positive(self.seed_count, "seed_count"))
        if self.root_count is not None and self.root_count > self.seed_count:
            raise CombatInformationCensusError(
                "root_count must not exceed seed_count"
            )
        ascension = _nonnegative(self.ascension_level, "ascension_level")
        if ascension > 20:
            raise CombatInformationCensusError("ascension_level must be at most 20")
        object.__setattr__(self, "ascension_level", ascension)
        for name in (
            "slot_count",
            "max_progress_steps",
            "wall_ms",
            "selection_particles",
            "evaluation_particles",
            "retained_seeds_per_group",
            "max_artifact_bytes",
        ):
            object.__setattr__(self, name, _positive(getattr(self, name), name))
        object.__setattr__(
            self,
            "relation_layers",
            _nonnegative(self.relation_layers, "relation_layers"),
        )


@dataclass
class _InformationGroup:
    count: int
    retained_seeds: list[int]
    candidate_ids: tuple[str, ...]
    encounters: Counter[str]
    monster_sets: Counter[tuple[str, ...]]
    hp_values: Counter[tuple[int, int]]
    public_snapshot_ids: set[str]
    public_history_snapshot_ids: set[str]
    public_decision_prefix_ids: set[str]

    @classmethod
    def first(
        cls,
        seed: int,
        context: RunPublicContext,
        candidate_ids: tuple[str, ...],
        public_snapshot: PublicDecisionSnapshot,
        public_decision_prefix_id: str,
    ) -> _InformationGroup:
        return cls(
            count=1,
            retained_seeds=[seed],
            candidate_ids=tuple(sorted(candidate_ids)),
            encounters=Counter([context.encounter_id or "unavailable"]),
            monster_sets=Counter([context.monster_ids]),
            hp_values=Counter([(context.hp, context.max_hp)]),
            public_snapshot_ids={public_snapshot.snapshot_id},
            public_history_snapshot_ids={public_snapshot.history_snapshot_id},
            public_decision_prefix_ids={public_decision_prefix_id},
        )

    def add(
        self,
        seed: int,
        context: RunPublicContext,
        candidate_ids: tuple[str, ...],
        retained_limit: int,
        public_snapshot: PublicDecisionSnapshot,
        public_decision_prefix_id: str,
    ) -> None:
        if tuple(sorted(candidate_ids)) != self.candidate_ids:
            raise CombatInformationCensusError(
                "one model information identity changed its candidate multiset"
            )
        self.count += 1
        if len(self.retained_seeds) < retained_limit:
            self.retained_seeds.append(seed)
        self.encounters[context.encounter_id or "unavailable"] += 1
        self.monster_sets[context.monster_ids] += 1
        self.hp_values[(context.hp, context.max_hp)] += 1
        self.public_snapshot_ids.add(public_snapshot.snapshot_id)
        self.public_history_snapshot_ids.add(public_snapshot.history_snapshot_id)
        self.public_decision_prefix_ids.add(public_decision_prefix_id)


@dataclass(frozen=True)
class _RecordedCombatEntry:
    seed: int
    information_state_id: str
    public_decision_prefix_id: str
    public_snapshot: PublicDecisionSnapshot
    previous_decisions: tuple[PublicDecisionPrefixStepV1, ...]
    encounter_id: str
    act: int
    floor: int
    hp: int
    max_hp: int


@dataclass(frozen=True)
class _CapturedNaturalRoot:
    entry: _RecordedCombatEntry
    single_root_artifact_sha256: str


def run_combat_information_census(
    config: CombatInformationCensusConfig,
    *,
    environment_constructor: Callable[[list[int], int], _CensusEnvironment] | None = None,
    artifact_merger: Callable[..., object] | None = None,
) -> dict[str, object]:
    """Collect one first combat entry per complete run seed and group public rows."""

    if not isinstance(config, CombatInformationCensusConfig):
        raise CombatInformationCensusError("census config must be typed")
    constructor = environment_constructor
    semantic_schema: int | None = None
    if constructor is None:
        bridge = CategoricalSessionBridge.installed()
        constructor = bridge.environment_without_combat_potions
        semantic_schema = operator.index(bridge.semantic_schema["version"])
        if config.root_artifact is not None and artifact_merger is None:
            artifact_merger = getattr(
                bridge.environment,
                "merge_combat_root_artifact_bytes",
                None,
            )
    if config.root_artifact is not None and not callable(artifact_merger):
        raise CombatInformationCensusError(
            "root artifact export requires the opaque bridge artifact merger"
        )

    schedule = SeedSchedule(
        config.partition,
        spec=config.partition_spec,
        next_candidate=config.seed_start,
    )
    completed_requested = 0
    unknown_requested = 0
    unknown_reasons: Counter[str] = Counter()
    groups: dict[str, _InformationGroup] = {}
    public_prefix_counts: Counter[str] = Counter()
    public_prefix_signatures: dict[str, tuple[str, str, str]] = {}
    root_payloads: list[bytes] = []
    captured_roots: list[_CapturedNaturalRoot] = []
    started = time.perf_counter()
    driver_iterations = 0

    while completed_requested < config.seed_count:
        if (time.perf_counter() - started) * 1000 >= config.wall_ms:
            break
        chunk_size = min(config.slot_count, config.seed_count - completed_requested)
        planned, schedule = schedule.plan(tuple(range(chunk_size)))
        slot_seeds = dict(zip(planned.slot_indices, planned.seeds, strict=True))
        pending = set(planned.slot_indices)
        env = constructor(list(planned.seeds), config.ascension_level)
        if operator.index(env.slot_count) != chunk_size:
            raise CombatInformationCensusError("census environment changed its slot count")
        public_prefix_by_slot: dict[int, list[PublicDecisionPrefixStepV1]] = {
            slot_index: [] for slot_index in planned.slot_indices
        }
        capture_this_chunk = (
            config.root_count is not None and len(root_payloads) < config.root_count
        )
        chunk_captured_roots: list[tuple[int, bytes, _CapturedNaturalRoot]] = []
        progress_provider = BridgeDecisionProgressProvider(env)
        chunk_progress_steps = 0

        while pending and chunk_progress_steps <= config.max_progress_steps:
            if (time.perf_counter() - started) * 1000 >= config.wall_ms:
                break
            driver_iterations += 1
            batch = env.decision_batch(semantic=True, legality_bootstrap=True)
            row_slots = _integer_array(batch, "slot_indices")
            phases = _integer_array(batch, "phase")
            counts = _integer_array(batch, "candidate_counts")
            available = _boolean_array(batch, "legality_bootstrap_available")
            ordinals = _integer_array(batch, "legality_bootstrap_ordinals")
            if not (
                row_slots.size
                == phases.size
                == counts.size
                == available.size
                == ordinals.size
            ):
                raise CombatInformationCensusError(
                    "decision and deterministic baseline columns misalign"
                )

            contexts = {
                context.slot_index: context
                for context in (
                    RunPublicContext.from_bridge_row(row)
                    for row in env.public_run_contexts()
                )
            }
            active_row_by_slot = {
                int(slot_index): row for row, slot_index in enumerate(row_slots)
            }
            for slot_index in sorted(pending - active_row_by_slot.keys()):
                pending.remove(slot_index)
                completed_requested += 1
                unknown_requested += 1
                unknown_reasons["terminated_before_combat"] += 1

            ordered_pending = tuple(sorted(pending))
            progress_by_slot = dict(
                zip(
                    ordered_pending,
                    progress_provider.capture(ordered_pending),
                    strict=True,
                )
            )
            for slot_index in ordered_pending:
                row = active_row_by_slot[slot_index]
                phase = int(phases[row])
                if phase == 1:
                    entry = _record_combat_entry(
                        config,
                        batch,
                        row,
                        slot_index,
                        slot_seeds,
                        contexts,
                        progress_by_slot,
                        public_prefix_by_slot,
                        groups,
                        public_prefix_counts,
                        public_prefix_signatures,
                    )
                    if capture_this_chunk:
                        payload = bytes(
                            env.combat_root_artifact_bytes(
                                [slot_index],
                                max_bytes=config.max_artifact_bytes,
                            )
                        )
                        if not payload or len(payload) > config.max_artifact_bytes:
                            raise CombatInformationCensusError(
                                "captured natural root violates its byte bound"
                            )
                        chunk_captured_roots.append(
                            (
                                slot_index,
                                payload,
                                _CapturedNaturalRoot(
                                    entry=entry,
                                    single_root_artifact_sha256=(
                                        hashlib.sha256(payload).hexdigest()
                                    ),
                                ),
                            )
                        )
                    pending.remove(slot_index)
                    completed_requested += 1
                elif phase != 0 or not bool(available[row]):
                    pending.remove(slot_index)
                    completed_requested += 1
                    unknown_requested += 1
                    unknown_reasons[
                        "unsupported_decision_phase"
                        if phase != 0
                        else "production_owner_unavailable"
                    ] += 1

            if not pending:
                break
            chosen: list[int] = []
            for row, raw_slot_index in enumerate(row_slots):
                slot_index = int(raw_slot_index)
                if int(counts[row]) == 0:
                    raise CombatInformationCensusError(
                        "active decision row has no candidates"
                    )
                if slot_index not in pending:
                    chosen.append(0)
                    continue
                selected_ordinal = int(ordinals[row])
                progress = progress_by_slot.get(slot_index)
                if progress is None or progress.public_snapshot is None:
                    raise CombatInformationCensusError(
                        "strategic row lacks its public information snapshot"
                    )
                if progress.public_snapshot.phase != int(phases[row]):
                    raise CombatInformationCensusError(
                        "strategic row and public information snapshot disagree"
                    )
                public_prefix_by_slot[slot_index].append(
                    selected_public_prefix_step_v1(
                        progress.public_snapshot,
                        selected_ordinal,
                    )
                )
                chosen.append(selected_ordinal)
            env.choose(chosen)
            env.step()
            chunk_progress_steps += 1

        if config.root_count is not None:
            remaining = config.root_count - len(root_payloads)
            for _, payload, root in sorted(
                chunk_captured_roots,
                key=lambda captured: captured[0],
            )[:remaining]:
                root_payloads.append(payload)
                captured_roots.append(root)

        if pending:
            reason = (
                "wall_deadline_reached"
                if (time.perf_counter() - started) * 1000 >= config.wall_ms
                else "progress_budget_exhausted"
            )
            completed_requested += len(pending)
            unknown_requested += len(pending)
            unknown_reasons[reason] += len(pending)

    root_artifact_summary = _finish_root_artifact_export(
        config,
        root_payloads,
        captured_roots,
        unknown_reasons,
        artifact_merger,
    )
    summary = _census_summary(
        config,
        schedule,
        completed_requested,
        unknown_requested,
        unknown_reasons,
        groups,
        public_prefix_counts,
        semantic_schema,
        driver_iterations,
        time.perf_counter() - started,
        root_artifact_summary,
    )
    with config.output.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    receipt_keys = (
        "complete",
        "ascension_level",
        "requested_seed_count",
        "completed_seed_count",
        "successful_root_count",
        "unknown_seed_count",
        "unique_information_states",
        "repeated_information_states",
        "qualified_group_count",
        "max_group_size",
        "unique_public_decision_prefixes",
        "repeated_public_decision_prefixes",
        "qualified_public_decision_prefix_group_count",
        "max_public_decision_prefix_group_size",
        "elapsed_seconds",
    )
    receipt = {
        "schema": "sts-learning-combat-information-census-receipt-v2",
        "artifact": str(config.output),
        **{key: summary[key] for key in receipt_keys},
    }
    print(json.dumps(receipt, separators=(",", ":"), sort_keys=True), flush=True)
    return summary


def _record_combat_entry(
    config: CombatInformationCensusConfig,
    batch: Mapping[str, object],
    row: int,
    slot_index: int,
    slot_seeds: Mapping[int, int],
    contexts: Mapping[int, RunPublicContext],
    progress_by_slot: Mapping[int, object],
    public_prefix_by_slot: Mapping[int, list[PublicDecisionPrefixStepV1]],
    groups: dict[str, _InformationGroup],
    public_prefix_counts: Counter[str],
    public_prefix_signatures: dict[str, tuple[str, str, str]],
) -> _RecordedCombatEntry:
    context = contexts.get(slot_index)
    if context is None or not context.is_combat:
        raise CombatInformationCensusError(
            "combat semantic row lacks an aligned public context"
        )
    progress = progress_by_slot.get(slot_index)
    public_snapshot = getattr(progress, "public_snapshot", None)
    if not isinstance(public_snapshot, PublicDecisionSnapshot):
        raise CombatInformationCensusError(
            "combat row lacks its typed public information snapshot"
        )
    if public_snapshot.phase != 1 or not public_snapshot.is_combat:
        raise CombatInformationCensusError(
            "combat row and public information snapshot disagree"
        )
    identity = semantic_policy_row_identity(
        batch,
        row,
        relation_layers=config.relation_layers,
    )
    public_prefix_id = public_combat_entry_prefix_id_v1(
        public_prefix_by_slot[slot_index],
        public_snapshot,
    )
    signature = (
        public_snapshot.snapshot_id,
        public_snapshot.history_snapshot_id,
        public_snapshot.candidate_surface_id,
    )
    previous = public_prefix_signatures.setdefault(public_prefix_id, signature)
    if previous != signature:
        raise CombatInformationCensusError(
            "one public decision-prefix identity changed its current snapshot"
        )
    public_prefix_counts[public_prefix_id] += 1

    information_id = identity.information_state_id
    group = groups.get(information_id)
    if group is None:
        groups[information_id] = _InformationGroup.first(
            slot_seeds[slot_index],
            context,
            identity.candidate_ids,
            public_snapshot,
            public_prefix_id,
        )
    else:
        group.add(
            slot_seeds[slot_index],
            context,
            identity.candidate_ids,
            config.retained_seeds_per_group,
            public_snapshot,
            public_prefix_id,
        )
    return _RecordedCombatEntry(
        seed=slot_seeds[slot_index],
        information_state_id=information_id,
        public_decision_prefix_id=public_prefix_id,
        public_snapshot=public_snapshot,
        previous_decisions=tuple(public_prefix_by_slot[slot_index]),
        encounter_id=context.encounter_id or "unavailable",
        act=context.act,
        floor=context.floor,
        hp=context.hp,
        max_hp=context.max_hp,
    )


def _finish_root_artifact_export(
    config: CombatInformationCensusConfig,
    root_payloads: Sequence[bytes],
    captured_roots: Sequence[_CapturedNaturalRoot],
    unknown_reasons: Mapping[str, int],
    artifact_merger: Callable[..., object] | None,
) -> dict[str, object] | None:
    if config.root_artifact is None or config.root_count is None:
        return None
    censored_seed_count = sum(
        count
        for reason, count in unknown_reasons.items()
        if reason != "terminated_before_combat"
    )
    complete = (
        len(root_payloads) == config.root_count and censored_seed_count == 0
    )
    merged_payload: bytes | None = None
    if complete:
        if not callable(artifact_merger):
            raise CombatInformationCensusError(
                "root artifact export lost its opaque artifact merger"
            )
        merged_payload = bytes(
            artifact_merger(
                list(root_payloads),
                max_bytes=config.max_artifact_bytes,
            )
        )
        if not merged_payload or len(merged_payload) > config.max_artifact_bytes:
            raise CombatInformationCensusError(
                "merged natural root artifact violates its byte bound"
            )
        with config.root_artifact.open("xb") as destination:
            destination.write(merged_payload)
    return {
        "schema": "sts-learning-natural-combat-root-export-v1",
        "selection_rule": (
            "first_n_successful_natural_combat_entries_in_declared_seed_partition"
        ),
        "outcome_filter": "none",
        "encounter_filter": "none",
        "requested_root_count": config.root_count,
        "captured_root_count": len(captured_roots),
        "censored_seed_count": censored_seed_count,
        "complete": complete,
        "artifact": str(config.root_artifact),
        "artifact_sha256": (
            None if merged_payload is None else hashlib.sha256(merged_payload).hexdigest()
        ),
        "artifact_bytes": None if merged_payload is None else len(merged_payload),
        "roots": tuple(_natural_root_summary(root) for root in captured_roots),
    }


def _natural_root_summary(root: _CapturedNaturalRoot) -> dict[str, object]:
    entry = root.entry
    snapshot = entry.public_snapshot
    return {
        "seed": entry.seed,
        "act": entry.act,
        "floor": entry.floor,
        "encounter_id": entry.encounter_id,
        "hp": entry.hp,
        "max_hp": entry.max_hp,
        "information_state_id": entry.information_state_id,
        "public_decision_prefix_id": entry.public_decision_prefix_id,
        "current_snapshot_id": snapshot.snapshot_id,
        "current_observation_id": snapshot.observation_id,
        "current_history_snapshot_id": snapshot.history_snapshot_id,
        "current_candidate_surface_id": snapshot.candidate_surface_id,
        "previous_decisions": tuple(
            {
                "snapshot_id": step.snapshot_id,
                "selected_candidate_id": step.selected_candidate_id,
            }
            for step in entry.previous_decisions
        ),
        "single_root_artifact_sha256": root.single_root_artifact_sha256,
    }


def _census_summary(
    config: CombatInformationCensusConfig,
    schedule: SeedSchedule,
    completed_requested: int,
    unknown_requested: int,
    unknown_reasons: Counter[str],
    groups: Mapping[str, _InformationGroup],
    public_prefix_counts: Counter[str],
    semantic_schema: int | None,
    driver_iterations: int,
    elapsed_seconds: float,
    root_artifact_export: Mapping[str, object] | None,
) -> dict[str, object]:
    group_sizes = Counter(group.count for group in groups.values())
    public_prefix_group_sizes = Counter(public_prefix_counts.values())
    required_group_size = config.selection_particles + config.evaluation_particles
    repeated = sorted(
        (
            (information_id, group)
            for information_id, group in groups.items()
            if group.count >= 2
        ),
        key=lambda row: (-row[1].count, row[0]),
    )
    return {
        "schema": "sts-learning-combat-information-census-v2",
        "semantic_schema_version": semantic_schema,
        "information_identity": "blake2b_256_relation_invariant_policy_row_v2",
        "public_conditioning_identity": (
            "blake2b_256_sanitized_decision_prefix_and_combat_snapshot_v1"
        ),
        "public_conditioning_limit": (
            "decision_boundaries_only_not_complete_public_event_transcript"
        ),
        "relation_layers": config.relation_layers,
        "ascension_level": config.ascension_level,
        "partition": config.partition.value,
        "partition_held_out_numerator": config.partition_spec.held_out_numerator,
        "partition_denominator": config.partition_spec.denominator,
        "seed_start": config.seed_start,
        "seed_end": schedule.next_candidate,
        "requested_seed_count": config.seed_count,
        "completed_seed_count": completed_requested,
        "successful_root_count": completed_requested - unknown_requested,
        "unknown_seed_count": unknown_requested,
        "unknown_reasons": dict(sorted(unknown_reasons.items())),
        "complete": completed_requested == config.seed_count,
        "slot_count": min(config.slot_count, config.seed_count),
        "driver_iterations": driver_iterations,
        "elapsed_seconds": elapsed_seconds,
        "root_artifact_export": root_artifact_export,
        "unique_information_states": len(groups),
        "singleton_information_states": group_sizes.get(1, 0),
        "repeated_information_states": len(repeated),
        "max_group_size": max(group_sizes, default=0),
        "group_size_histogram": {
            str(size): count for size, count in sorted(group_sizes.items())
        },
        "selection_particles": config.selection_particles,
        "evaluation_particles": config.evaluation_particles,
        "required_group_size": required_group_size,
        "qualified_group_count": sum(
            group.count >= required_group_size for group in groups.values()
        ),
        "qualified_unique_candidate_group_count": sum(
            group.count >= required_group_size
            and len(group.candidate_ids) == len(set(group.candidate_ids))
            for group in groups.values()
        ),
        "unique_public_decision_prefixes": len(public_prefix_counts),
        "repeated_public_decision_prefixes": sum(
            count >= 2 for count in public_prefix_counts.values()
        ),
        "max_public_decision_prefix_group_size": max(
            public_prefix_group_sizes,
            default=0,
        ),
        "qualified_public_decision_prefix_group_count": sum(
            count >= required_group_size for count in public_prefix_counts.values()
        ),
        "public_decision_prefix_group_size_histogram": {
            str(size): count
            for size, count in sorted(public_prefix_group_sizes.items())
        },
        "repeated_groups": tuple(
            {
                "information_state_id": information_id,
                "count": group.count,
                "retained_seeds": tuple(group.retained_seeds),
                "candidate_count": len(group.candidate_ids),
                "candidate_semantic_ids": group.candidate_ids,
                "candidate_semantics_unique": len(group.candidate_ids)
                == len(set(group.candidate_ids)),
                "encounters": dict(sorted(group.encounters.items())),
                "monster_sets": tuple(
                    {"monster_ids": monster_ids, "count": count}
                    for monster_ids, count in sorted(group.monster_sets.items())
                ),
                "hp_values": tuple(
                    {"hp": hp, "max_hp": max_hp, "count": count}
                    for (hp, max_hp), count in sorted(group.hp_values.items())
                ),
                "distinct_public_snapshot_count": len(group.public_snapshot_ids),
                "distinct_public_history_snapshot_count": len(
                    group.public_history_snapshot_ids
                ),
                "distinct_public_decision_prefix_count": len(
                    group.public_decision_prefix_ids
                ),
            }
            for information_id, group in repeated
        ),
    }


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seed-start", type=int, required=True)
    parser.add_argument("--seed-count", type=int, required=True)
    parser.add_argument("--ascension", type=int, required=True)
    parser.add_argument("--slots", type=int, default=32)
    parser.add_argument(
        "--partition",
        choices=tuple(partition.value for partition in SeedPartition),
        default=SeedPartition.TRAINING.value,
    )
    parser.add_argument("--held-out-numerator", type=int, default=1)
    parser.add_argument("--partition-denominator", type=int, default=10)
    parser.add_argument("--max-progress-steps", type=int, default=16)
    parser.add_argument("--wall-ms", type=int, default=60_000)
    parser.add_argument("--selection-particles", type=int, default=4)
    parser.add_argument("--evaluation-particles", type=int, default=4)
    parser.add_argument("--relation-layers", type=int, default=2)
    parser.add_argument("--root-artifact", type=Path)
    parser.add_argument("--root-count", type=int)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    arguments = parser.parse_args(argv)
    run_combat_information_census(
        CombatInformationCensusConfig(
            output=arguments.output,
            seed_start=arguments.seed_start,
            seed_count=arguments.seed_count,
            ascension_level=arguments.ascension,
            slot_count=arguments.slots,
            partition=SeedPartition(arguments.partition),
            partition_spec=SeedPartitionSpec(
                held_out_numerator=arguments.held_out_numerator,
                denominator=arguments.partition_denominator,
            ),
            max_progress_steps=arguments.max_progress_steps,
            wall_ms=arguments.wall_ms,
            selection_particles=arguments.selection_particles,
            evaluation_particles=arguments.evaluation_particles,
            relation_layers=arguments.relation_layers,
            root_artifact=arguments.root_artifact,
            root_count=arguments.root_count,
            max_artifact_bytes=arguments.max_artifact_bytes,
        )
    )
    return 0


def _integer_array(source: Mapping[str, object], key: str) -> np.ndarray:
    if key not in source:
        raise CombatInformationCensusError(f"decision batch is missing {key}")
    value = np.asarray(source[key], dtype=np.uint64)
    if value.ndim != 1:
        raise CombatInformationCensusError(
            f"decision batch {key} must be one-dimensional"
        )
    return value


def _boolean_array(source: Mapping[str, object], key: str) -> np.ndarray:
    if key not in source:
        raise CombatInformationCensusError(f"decision batch is missing {key}")
    value = np.asarray(source[key], dtype=np.bool_)
    if value.ndim != 1:
        raise CombatInformationCensusError(
            f"decision batch {key} must be one-dimensional"
        )
    return value


def _positive(value: object, name: str) -> int:
    normalized = operator.index(value)
    if normalized <= 0:
        raise CombatInformationCensusError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    normalized = operator.index(value)
    if normalized < 0:
        raise CombatInformationCensusError(f"{name} must be non-negative")
    return normalized


def _seed(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized >= 1 << 64:
        raise CombatInformationCensusError(f"{name} must be below 2^64")
    return normalized


if __name__ == "__main__":
    raise SystemExit(main())
