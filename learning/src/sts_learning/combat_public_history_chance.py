"""Exact run-seed populations conditioned on a captured public combat history.

Unlike floor-local RNG reseeding, every retained particle starts a fresh production
run from one complete run seed.  Earlier public decision snapshots must match the
captured prefix exactly, the same public candidate identity is replayed at each
matching boundary, and the combat-entry snapshot must also match exactly.

The scan is exact only for its declared finite seed frame.  A frame that contains
no alternative match is a measured degenerate posterior, not permission to invent
independent RNG streams or to reuse the realized private future as a teacher.
"""

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

from .decision_progress import BridgeDecisionProgressProvider, PublicDecisionSnapshot
from .public_decision_prefix import PublicDecisionPrefixStepV1
from .seeds import SeedPartition, SeedPartitionSpec, SeedSchedule
from .torch_session_config import CategoricalSessionBridge


class CombatPublicHistoryChanceError(RuntimeError):
    """The target history, seed frame, or production replay was malformed."""


class _PublicHistoryEnvironment(Protocol):
    slot_count: int

    def decision_batch(self) -> Mapping[str, object]: ...

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
class PublicCombatEntryHistoryV1:
    """One source combat entry described only by captured public identities."""

    source_seed: int
    ascension_level: int
    public_decision_prefix_id: str
    previous_decisions: tuple[PublicDecisionPrefixStepV1, ...]
    current_snapshot_id: str

    def __post_init__(self) -> None:
        object.__setattr__(self, "source_seed", _seed(self.source_seed, "source_seed"))
        ascension = _integer(self.ascension_level, "ascension_level", minimum=0)
        if ascension > 20:
            raise CombatPublicHistoryChanceError("ascension_level must be at most 20")
        object.__setattr__(self, "ascension_level", ascension)
        object.__setattr__(
            self,
            "public_decision_prefix_id",
            _text(self.public_decision_prefix_id, "public_decision_prefix_id"),
        )
        object.__setattr__(
            self,
            "current_snapshot_id",
            _text(self.current_snapshot_id, "current_snapshot_id"),
        )
        previous = tuple(self.previous_decisions)
        if not all(isinstance(step, PublicDecisionPrefixStepV1) for step in previous):
            raise CombatPublicHistoryChanceError(
                "previous_decisions must contain typed public prefix steps"
            )
        object.__setattr__(self, "previous_decisions", previous)


@dataclass(frozen=True)
class PublicHistoryRunSeedScanConfig:
    """A finite prior frame and bounded exact replay contract."""

    target: PublicCombatEntryHistoryV1
    candidate_seed_start: int
    candidate_seed_count: int
    partition: SeedPartition = SeedPartition.TRAINING
    partition_spec: SeedPartitionSpec = SeedPartitionSpec()
    slot_count: int = 32
    max_progress_steps: int = 32
    wall_ms: int = 60_000
    retained_particle_count: int = 8
    sampling_seed: int = 0
    max_artifact_bytes: int = 16 * 1024 * 1024

    def __post_init__(self) -> None:
        if not isinstance(self.target, PublicCombatEntryHistoryV1):
            raise CombatPublicHistoryChanceError("target must be typed")
        object.__setattr__(
            self,
            "candidate_seed_start",
            _seed(self.candidate_seed_start, "candidate_seed_start"),
        )
        object.__setattr__(
            self,
            "sampling_seed",
            _seed(self.sampling_seed, "sampling_seed"),
        )
        for name in (
            "candidate_seed_count",
            "slot_count",
            "max_progress_steps",
            "wall_ms",
            "retained_particle_count",
            "max_artifact_bytes",
        ):
            object.__setattr__(
                self,
                name,
                _integer(getattr(self, name), name, minimum=1),
            )
        if not isinstance(self.partition, SeedPartition):
            raise CombatPublicHistoryChanceError("partition must be typed")
        if not isinstance(self.partition_spec, SeedPartitionSpec):
            raise CombatPublicHistoryChanceError("partition_spec must be typed")


@dataclass(frozen=True)
class RetainedRunSeedParticleV1:
    run_seed: int
    sampling_priority: str
    root_artifact: bytes


@dataclass(frozen=True)
class PublicHistoryRunSeedScanResultV1:
    """Complete receipt for one finite seed-frame posterior scan."""

    target: PublicCombatEntryHistoryV1
    partition: SeedPartition
    candidate_seed_start: int
    candidate_seed_end: int
    requested_candidate_count: int
    scanned_candidate_count: int
    accepted_candidate_count: int
    retained_particles: tuple[RetainedRunSeedParticleV1, ...]
    rejection_counts: tuple[tuple[str, int], ...]
    source_seed_in_frame: bool
    source_seed_reconstructed: bool
    complete: bool
    elapsed_seconds: float

    def summary(
        self,
        *,
        merged_artifact: Path | None = None,
        merged_payload: bytes | None = None,
    ) -> dict[str, object]:
        if (merged_artifact is None) != (merged_payload is None):
            raise CombatPublicHistoryChanceError(
                "merged artifact path and payload must be present together"
            )
        return {
            "schema": "sts-learning-public-history-run-seed-population-v1",
            "conditioning": (
                "exact_captured_public_decision_prefix_and_combat_snapshot"
            ),
            "seed_semantics": "fresh_production_run_from_complete_run_seed",
            "finite_frame_exact": self.complete,
            "teacher_valid": False,
            "target": {
                "source_seed": self.target.source_seed,
                "ascension_level": self.target.ascension_level,
                "public_decision_prefix_id": self.target.public_decision_prefix_id,
                "previous_decision_count": len(self.target.previous_decisions),
                "current_snapshot_id": self.target.current_snapshot_id,
            },
            "partition": self.partition.value,
            "candidate_seed_start": self.candidate_seed_start,
            "candidate_seed_end": self.candidate_seed_end,
            "requested_candidate_count": self.requested_candidate_count,
            "scanned_candidate_count": self.scanned_candidate_count,
            "accepted_candidate_count": self.accepted_candidate_count,
            "retained_particle_count": len(self.retained_particles),
            "retained_run_seeds": tuple(
                particle.run_seed for particle in self.retained_particles
            ),
            "rejection_counts": dict(self.rejection_counts),
            "source_seed_in_frame": self.source_seed_in_frame,
            "source_seed_reconstructed": self.source_seed_reconstructed,
            "posterior_degenerate_in_frame": (
                self.complete and self.accepted_candidate_count <= 1
            ),
            "complete": self.complete,
            "elapsed_seconds": self.elapsed_seconds,
            "artifact": None if merged_artifact is None else str(merged_artifact),
            "artifact_bytes": None if merged_payload is None else len(merged_payload),
            "artifact_sha256": (
                None
                if merged_payload is None
                else hashlib.sha256(merged_payload).hexdigest()
            ),
        }


def load_public_combat_entry_history_v1(
    census: Mapping[str, object],
    root_slot: int,
) -> PublicCombatEntryHistoryV1:
    """Load one public target from the natural-root census sidecar."""

    if census.get("schema") != "sts-learning-combat-information-census-v2":
        raise CombatPublicHistoryChanceError("unsupported combat census schema")
    slot = _integer(root_slot, "root_slot", minimum=0)
    export = census.get("root_artifact_export")
    if not isinstance(export, Mapping) or not export.get("complete"):
        raise CombatPublicHistoryChanceError(
            "combat census lacks a complete natural-root export"
        )
    roots = export.get("roots")
    if not isinstance(roots, Sequence) or isinstance(roots, (str, bytes)):
        raise CombatPublicHistoryChanceError("natural-root export roots are malformed")
    if slot >= len(roots):
        raise CombatPublicHistoryChanceError("root_slot is outside the natural-root export")
    root = roots[slot]
    if not isinstance(root, Mapping):
        raise CombatPublicHistoryChanceError("natural-root summary is malformed")
    previous_raw = root.get("previous_decisions")
    if not isinstance(previous_raw, Sequence) or isinstance(previous_raw, (str, bytes)):
        raise CombatPublicHistoryChanceError("public decision prefix is malformed")
    previous: list[PublicDecisionPrefixStepV1] = []
    for step in previous_raw:
        if not isinstance(step, Mapping):
            raise CombatPublicHistoryChanceError("public prefix step is malformed")
        previous.append(
            PublicDecisionPrefixStepV1(
                snapshot_id=step.get("snapshot_id"),
                selected_candidate_id=step.get("selected_candidate_id"),
            )
        )
    return PublicCombatEntryHistoryV1(
        source_seed=root.get("seed"),
        ascension_level=census.get("ascension_level"),
        public_decision_prefix_id=root.get("public_decision_prefix_id"),
        previous_decisions=tuple(previous),
        current_snapshot_id=root.get("current_snapshot_id"),
    )


def scan_public_history_run_seed_population_v1(
    config: PublicHistoryRunSeedScanConfig,
    *,
    environment_constructor: Callable[
        [list[int], int], _PublicHistoryEnvironment
    ]
    | None = None,
) -> PublicHistoryRunSeedScanResultV1:
    """Replay one complete finite seed frame and retain exact public matches."""

    if not isinstance(config, PublicHistoryRunSeedScanConfig):
        raise CombatPublicHistoryChanceError("scan config must be typed")
    constructor = environment_constructor
    if constructor is None:
        constructor = CategoricalSessionBridge.installed().environment_without_combat_potions

    schedule = SeedSchedule(
        config.partition,
        spec=config.partition_spec,
        next_candidate=config.candidate_seed_start,
    )
    started = time.perf_counter()
    scanned = 0
    accepted = 0
    retained: list[RetainedRunSeedParticleV1] = []
    rejections: Counter[str] = Counter()
    source_seed_in_frame = False
    source_seed_reconstructed = False
    deadline_reached = False

    while scanned < config.candidate_seed_count:
        if (time.perf_counter() - started) * 1000 >= config.wall_ms:
            deadline_reached = True
            break
        chunk_size = min(config.slot_count, config.candidate_seed_count - scanned)
        planned, schedule = schedule.plan(tuple(range(chunk_size)))
        seeds_by_slot = dict(zip(planned.slot_indices, planned.seeds, strict=True))
        source_seed_in_frame |= config.target.source_seed in planned.seeds
        env = constructor(list(planned.seeds), config.target.ascension_level)
        if operator.index(env.slot_count) != chunk_size:
            raise CombatPublicHistoryChanceError(
                "public-history environment changed its slot count"
            )
        chunk_result = _scan_public_history_chunk_v1(config, env, seeds_by_slot)
        scanned += chunk_size
        accepted += chunk_result.accepted_count
        rejections.update(dict(chunk_result.rejections))
        source_seed_reconstructed |= chunk_result.source_seed_reconstructed
        for particle in chunk_result.particles:
            _retain_priority_particle_v1(
                retained,
                particle,
                config.retained_particle_count,
            )

    retained.sort(key=lambda particle: (particle.sampling_priority, particle.run_seed))
    return PublicHistoryRunSeedScanResultV1(
        target=config.target,
        partition=config.partition,
        candidate_seed_start=config.candidate_seed_start,
        candidate_seed_end=schedule.next_candidate,
        requested_candidate_count=config.candidate_seed_count,
        scanned_candidate_count=scanned,
        accepted_candidate_count=accepted,
        retained_particles=tuple(retained),
        rejection_counts=tuple(sorted(rejections.items())),
        source_seed_in_frame=source_seed_in_frame,
        source_seed_reconstructed=source_seed_reconstructed,
        complete=not deadline_reached and scanned == config.candidate_seed_count,
        elapsed_seconds=time.perf_counter() - started,
    )


@dataclass(frozen=True)
class _ChunkResult:
    accepted_count: int
    particles: tuple[RetainedRunSeedParticleV1, ...]
    rejections: tuple[tuple[str, int], ...]
    source_seed_reconstructed: bool


def _scan_public_history_chunk_v1(
    config: PublicHistoryRunSeedScanConfig,
    env: _PublicHistoryEnvironment,
    seeds_by_slot: Mapping[int, int],
) -> _ChunkResult:
    active = set(seeds_by_slot)
    prefix_index = {slot: 0 for slot in active}
    provider = BridgeDecisionProgressProvider(env)
    particles: list[RetainedRunSeedParticleV1] = []
    rejections: Counter[str] = Counter()
    accepted_count = 0
    source_seed_reconstructed = False
    progress_steps = 0

    while active and progress_steps <= config.max_progress_steps:
        batch = env.decision_batch()
        row_slots = _integer_sequence(batch, "slot_indices")
        candidate_counts = _integer_sequence(batch, "candidate_counts")
        if len(row_slots) != len(candidate_counts):
            raise CombatPublicHistoryChanceError(
                "decision slots and candidate counts are misaligned"
            )
        if len(set(row_slots)) != len(row_slots):
            raise CombatPublicHistoryChanceError("decision batch repeats a slot")
        row_by_slot = {slot: row for row, slot in enumerate(row_slots)}
        for slot in sorted(active - row_by_slot.keys()):
            active.remove(slot)
            rejections["terminated_before_target"] += 1
        if not active:
            break

        snapshots = dict(
            zip(
                sorted(active),
                provider.capture(tuple(sorted(active))),
                strict=True,
            )
        )
        chosen = [0] * len(row_slots)
        matched_prefix_slots: list[int] = []
        for slot in sorted(active):
            row = row_by_slot[slot]
            if candidate_counts[row] <= 0:
                raise CombatPublicHistoryChanceError(
                    "active public decision has no candidates"
                )
            progress = snapshots[slot]
            snapshot = progress.public_snapshot
            if not isinstance(snapshot, PublicDecisionSnapshot):
                raise CombatPublicHistoryChanceError(
                    "active public decision lacks a typed snapshot"
                )
            index = prefix_index[slot]
            if index < len(config.target.previous_decisions):
                expected = config.target.previous_decisions[index]
                if snapshot.snapshot_id != expected.snapshot_id:
                    active.remove(slot)
                    rejections[f"prefix_snapshot_mismatch_{index}"] += 1
                    continue
                try:
                    ordinal = snapshot.candidate_ids.index(expected.selected_candidate_id)
                except ValueError as error:
                    raise CombatPublicHistoryChanceError(
                        "an exact public snapshot lost its selected candidate identity"
                    ) from error
                chosen[row] = ordinal
                matched_prefix_slots.append(slot)
                continue

            if not snapshot.is_combat:
                active.remove(slot)
                rejections["target_boundary_not_combat"] += 1
                continue
            if snapshot.snapshot_id != config.target.current_snapshot_id:
                active.remove(slot)
                rejections["combat_snapshot_mismatch"] += 1
                continue
            payload = bytes(
                env.combat_root_artifact_bytes(
                    [slot],
                    max_bytes=config.max_artifact_bytes,
                )
            )
            if not payload or len(payload) > config.max_artifact_bytes:
                raise CombatPublicHistoryChanceError(
                    "accepted public-history root violates its artifact byte bound"
                )
            run_seed = seeds_by_slot[slot]
            particles.append(
                RetainedRunSeedParticleV1(
                    run_seed=run_seed,
                    sampling_priority=_sampling_priority_v1(
                        config.sampling_seed,
                        run_seed,
                    ),
                    root_artifact=payload,
                )
            )
            accepted_count += 1
            source_seed_reconstructed |= run_seed == config.target.source_seed
            active.remove(slot)

        if not matched_prefix_slots:
            break
        env.choose(chosen)
        env.step()
        progress_steps += 1
        for slot in matched_prefix_slots:
            prefix_index[slot] += 1

    if active:
        rejections["progress_budget_exhausted"] += len(active)
    return _ChunkResult(
        accepted_count=accepted_count,
        particles=tuple(particles),
        rejections=tuple(sorted(rejections.items())),
        source_seed_reconstructed=source_seed_reconstructed,
    )


def merge_retained_run_seed_particles_v1(
    result: PublicHistoryRunSeedScanResultV1,
    *,
    artifact_merger: Callable[..., object] | None = None,
    max_bytes: int,
) -> bytes | None:
    """Merge only the deterministic priority sample, never every accepted seed."""

    if not isinstance(result, PublicHistoryRunSeedScanResultV1):
        raise CombatPublicHistoryChanceError("scan result must be typed")
    limit = _integer(max_bytes, "max_bytes", minimum=1)
    if not result.retained_particles:
        return None
    merger = artifact_merger
    if merger is None:
        merger = getattr(
            CategoricalSessionBridge.installed().environment,
            "merge_combat_root_artifact_bytes",
            None,
        )
    if not callable(merger):
        raise CombatPublicHistoryChanceError(
            "opaque root artifact merger is unavailable"
        )
    payload = bytes(
        merger(
            [particle.root_artifact for particle in result.retained_particles],
            max_bytes=limit,
        )
    )
    if not payload or len(payload) > limit:
        raise CombatPublicHistoryChanceError(
            "merged public-history population violates its byte bound"
        )
    return payload


def _retain_priority_particle_v1(
    retained: list[RetainedRunSeedParticleV1],
    particle: RetainedRunSeedParticleV1,
    limit: int,
) -> None:
    if len(retained) < limit:
        retained.append(particle)
        return
    worst_index = max(
        range(len(retained)),
        key=lambda index: (
            retained[index].sampling_priority,
            retained[index].run_seed,
        ),
    )
    if (particle.sampling_priority, particle.run_seed) < (
        retained[worst_index].sampling_priority,
        retained[worst_index].run_seed,
    ):
        retained[worst_index] = particle


def _sampling_priority_v1(sampling_seed: int, run_seed: int) -> str:
    return hashlib.blake2b(
        sampling_seed.to_bytes(8, "little") + run_seed.to_bytes(8, "little"),
        digest_size=16,
        person=b"sts-run-chance1",
    ).hexdigest()


def _integer_sequence(batch: Mapping[str, object], name: str) -> tuple[int, ...]:
    try:
        raw = batch[name]
    except KeyError as error:
        raise CombatPublicHistoryChanceError(f"decision batch lacks {name}") from error
    if isinstance(raw, (str, bytes)):
        raise CombatPublicHistoryChanceError(f"decision batch {name} is malformed")
    try:
        return tuple(operator.index(value) for value in raw)
    except (TypeError, ValueError) as error:
        raise CombatPublicHistoryChanceError(
            f"decision batch {name} must contain integers"
        ) from error


def _seed(value: object, name: str) -> int:
    normalized = _integer(value, name, minimum=0)
    if normalized >= 1 << 64:
        raise CombatPublicHistoryChanceError(f"{name} must be below 2^64")
    return normalized


def _integer(value: object, name: str, *, minimum: int) -> int:
    if isinstance(value, bool):
        raise CombatPublicHistoryChanceError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatPublicHistoryChanceError(f"{name} must be an integer") from error
    if normalized < minimum:
        raise CombatPublicHistoryChanceError(f"{name} must be at least {minimum}")
    return normalized


def _text(value: object, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise CombatPublicHistoryChanceError(f"{name} must be non-empty text")
    return value


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Scan complete run seeds for one exact public combat-entry history"
    )
    parser.add_argument("--census", type=Path, required=True)
    parser.add_argument("--root-slot", type=int, required=True)
    parser.add_argument("--candidate-seed-start", type=int, required=True)
    parser.add_argument("--candidate-seed-count", type=int, required=True)
    parser.add_argument(
        "--partition",
        choices=tuple(partition.value for partition in SeedPartition),
        default=SeedPartition.TRAINING.value,
    )
    parser.add_argument("--slot-count", type=int, default=32)
    parser.add_argument("--max-progress-steps", type=int, default=32)
    parser.add_argument("--wall-ms", type=int, default=60_000)
    parser.add_argument("--retained-particles", type=int, default=8)
    parser.add_argument("--sampling-seed", type=int, default=0)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    parser.add_argument("--root-artifact", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    for name in ("census",):
        path = getattr(args, name).resolve()
        if not path.is_file():
            parser.error(f"--{name.replace('_', '-')} must be an existing file")
        setattr(args, name, path)
    for name in ("output", "root_artifact"):
        path = getattr(args, name)
        if path is None:
            continue
        path = path.resolve()
        if path.exists():
            parser.error(f"--{name.replace('_', '-')} must be a fresh path")
        if not path.parent.is_dir():
            parser.error(f"--{name.replace('_', '-')} parent must exist")
        setattr(args, name, path)
    return args


def main(argv: Sequence[str] | None = None) -> int:
    args = _parse_args(argv)
    with args.census.open("r", encoding="utf-8") as source:
        census = json.load(source)
    if not isinstance(census, Mapping):
        raise CombatPublicHistoryChanceError("combat census root must be an object")
    target = load_public_combat_entry_history_v1(census, args.root_slot)
    config = PublicHistoryRunSeedScanConfig(
        target=target,
        candidate_seed_start=args.candidate_seed_start,
        candidate_seed_count=args.candidate_seed_count,
        partition=SeedPartition(args.partition),
        slot_count=args.slot_count,
        max_progress_steps=args.max_progress_steps,
        wall_ms=args.wall_ms,
        retained_particle_count=args.retained_particles,
        sampling_seed=args.sampling_seed,
        max_artifact_bytes=args.max_artifact_bytes,
    )
    result = scan_public_history_run_seed_population_v1(config)
    merged = (
        None
        if args.root_artifact is None
        else merge_retained_run_seed_particles_v1(
            result,
            max_bytes=args.max_artifact_bytes,
        )
    )
    if merged is not None:
        with args.root_artifact.open("xb") as destination:
            destination.write(merged)
    summary = result.summary(
        merged_artifact=args.root_artifact if merged is not None else None,
        merged_payload=merged,
    )
    with args.output.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    receipt = {
        key: summary[key]
        for key in (
            "schema",
            "partition",
            "scanned_candidate_count",
            "accepted_candidate_count",
            "retained_particle_count",
            "source_seed_reconstructed",
            "posterior_degenerate_in_frame",
            "complete",
            "elapsed_seconds",
            "artifact",
        )
    }
    print(json.dumps(receipt, separators=(",", ":"), sort_keys=True), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
