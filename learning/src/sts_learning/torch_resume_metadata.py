"""Canonical scalar metadata for one categorical generation resume point."""

from __future__ import annotations

import math
import operator
from dataclasses import dataclass

from ._torch_owner_state_codec import (
    TorchOwnerStateError,
    decode_owner_state,
    encode_owner_state,
)
from .attempts import AttemptAssemblerSnapshot
from .driver import BatchDriverResumeBoundary
from .policy import BehaviorManifestId, SelectionProbability
from .recovery import RecoveryMode, RecoverySlotSnapshot, RecoverySlotStatus
from .seeds import SeedPartition, SeedPartitionSpec, SeedSchedule
from .torch_behavior import CategoricalTorchBehaviorControllerSnapshot
from .torch_generation import CategoricalGenerationResumeBoundary
from .torch_resume import TorchResumeStateError
from .torch_training import SynchronousPolicyTrainerSnapshot


_GENERATION_COMPONENT = "categorical_generation_state"
_COMPONENT_VERSION = 3


@dataclass(frozen=True)
class CategoricalGenerationResumeState:
    """Scalar caller state admitted alongside external binary components."""

    boundary: CategoricalGenerationResumeBoundary
    optimizer_steps_per_generation: int


def encode_generation_resume_state(
    boundary: CategoricalGenerationResumeBoundary,
    *,
    optimizer_steps_per_generation: int,
    max_bytes: int,
) -> bytes:
    """Encode ledger, schedule, controller, trainer, and sequence counters."""

    if not isinstance(boundary, CategoricalGenerationResumeBoundary):
        raise TorchResumeStateError("generation resume boundary must be typed")
    generation_steps = _positive_integer(
        optimizer_steps_per_generation,
        "optimizer_steps_per_generation",
    )
    driver = boundary.driver
    trainer = boundary.trainer
    controller = boundary.controller
    try:
        return encode_owner_state(
            {
                "component": _GENERATION_COMPONENT,
                "version": _COMPONENT_VERSION,
                "optimizer_steps_per_generation": generation_steps,
                "driver": {
                    "slot_count": driver.slot_count,
                    "checkpoint_slots": driver.checkpoint_slots,
                    "experience_next_sequence_index": (
                        driver.experience_next_sequence_index
                    ),
                    "schedule": {
                        "partition": driver.schedule.partition.value,
                        "held_out_numerator": (
                            driver.schedule.spec.held_out_numerator
                        ),
                        "denominator": driver.schedule.spec.denominator,
                        "next_candidate": driver.schedule.next_candidate,
                    },
                    "recovery_mode": driver.recovery_mode.value,
                    "max_recoveries_per_episode": (
                        driver.max_recoveries_per_episode
                    ),
                    "ledger": [
                        {
                            "slot_index": row.slot_index,
                            "episode_seed": row.episode_seed,
                            "episode_generation": row.episode_generation,
                            "attempt_index": row.attempt_index,
                            "recoveries_used": row.recoveries_used,
                            "status": row.status.value,
                        }
                        for row in driver.ledger_snapshots
                    ],
                },
                "assembler": {
                    "next_sequence_index": boundary.assembler.next_sequence_index,
                    "open_attempts": boundary.assembler.open_attempts,
                    "dropped_open_attempts": boundary.assembler.dropped_open_attempts,
                    "retained_decisions": boundary.assembler.retained_decisions,
                    "retained_payload_bytes": (
                        boundary.assembler.retained_payload_bytes
                    ),
                    "completed_attempts": boundary.assembler.completed_attempts,
                    "dropped_attempts": boundary.assembler.dropped_attempts,
                },
                "trainer": {
                    "deliveries": trainer.deliveries,
                    "optimizer_steps": trainer.optimizer_steps,
                    "completed_attempts": trainer.completed_attempts,
                    "dropped_attempts": trainer.dropped_attempts,
                    "trained_decisions": trainer.trained_decisions,
                    "last_loss": trainer.last_loss,
                    "last_behavior_manifest_ids": (
                        None
                        if trainer.last_behavior_manifest_ids is None
                        else [
                            [manifest_id.digest for manifest_id in attempt]
                            for attempt in trainer.last_behavior_manifest_ids
                        ]
                    ),
                    "last_selection_probabilities": (
                        None
                        if trainer.last_selection_probabilities is None
                        else [
                            [probability.value for probability in attempt]
                            for attempt in trainer.last_selection_probabilities
                        ]
                    ),
                    "total_training_seconds": trainer.total_training_seconds,
                    "last_training_seconds": trainer.last_training_seconds,
                    "poisoned": trainer.poisoned,
                },
                "controller": {
                    "active_manifest_id": (
                        None
                        if controller.active_manifest_id is None
                        else controller.active_manifest_id.digest
                    ),
                    "active_training_step": controller.active_training_step,
                    "successful_promotions": controller.successful_promotions,
                },
            },
            max_bytes=max_bytes,
        )
    except TorchOwnerStateError as error:
        raise TorchResumeStateError(str(error)) from error


def decode_generation_resume_state(
    payload: bytes,
    *,
    max_bytes: int,
) -> CategoricalGenerationResumeState:
    """Rebuild typed scalar owners without accepting partial or extra fields."""

    try:
        raw = decode_owner_state(payload, max_bytes=max_bytes)
        root = _exact_mapping(
            raw,
            {
                "component",
                "version",
                "optimizer_steps_per_generation",
                "driver",
                "assembler",
                "trainer",
                "controller",
            },
            "generation resume root",
        )
        if root["component"] != _GENERATION_COMPONENT:
            raise TorchResumeStateError(
                "resume component is not categorical generation state"
            )
        if root["version"] != _COMPONENT_VERSION:
            raise TorchResumeStateError("resume component version is unsupported")
        generation_steps = _positive_integer(
            root["optimizer_steps_per_generation"],
            "optimizer_steps_per_generation",
        )
        driver = _decode_driver_boundary(root["driver"])
        assembler = _decode_assembler_snapshot(root["assembler"])
        trainer = _decode_trainer_snapshot(root["trainer"])
        controller = _decode_controller_snapshot(root["controller"])
    except (TorchOwnerStateError, ValueError, TypeError, KeyError) as error:
        if isinstance(error, TorchResumeStateError):
            raise
        raise TorchResumeStateError(str(error)) from error
    if assembler.next_sequence_index != driver.experience_next_sequence_index:
        raise TorchResumeStateError(
            "experience buffer and attempt assembler sequence indices differ"
        )
    if (
        assembler.open_attempts != 0
        or assembler.dropped_open_attempts != 0
        or assembler.retained_decisions != 0
        or assembler.retained_payload_bytes != 0
    ):
        raise TorchResumeStateError("generation resume contains open attempt state")
    if trainer.poisoned:
        raise TorchResumeStateError("generation resume trainer is poisoned")
    if (
        controller.active_manifest_id is None
        or controller.active_training_step is None
    ):
        raise TorchResumeStateError("generation resume has no active behavior")
    if trainer.optimizer_steps < controller.active_training_step:
        raise TorchResumeStateError(
            "generation resume optimizer is behind active behavior"
        )
    return CategoricalGenerationResumeState(
        boundary=CategoricalGenerationResumeBoundary(
            driver=driver,
            assembler=assembler,
            trainer=trainer,
            controller=controller,
        ),
        optimizer_steps_per_generation=generation_steps,
    )


def _decode_driver_boundary(value: object) -> BatchDriverResumeBoundary:
    raw = _exact_mapping(
        value,
        {
            "slot_count",
            "checkpoint_slots",
            "experience_next_sequence_index",
            "schedule",
            "recovery_mode",
            "max_recoveries_per_episode",
            "ledger",
        },
        "driver resume state",
    )
    slot_count = _positive_integer(raw["slot_count"], "slot_count")
    checkpoint_slots = _positive_integer(
        raw["checkpoint_slots"],
        "checkpoint_slots",
    )
    if checkpoint_slots != slot_count:
        raise TorchResumeStateError("checkpoint bank does not cover every slot")
    experience_sequence = _nonnegative_integer(
        raw["experience_next_sequence_index"],
        "experience_next_sequence_index",
    )
    schedule_raw = _exact_mapping(
        raw["schedule"],
        {"partition", "held_out_numerator", "denominator", "next_candidate"},
        "seed schedule",
    )
    schedule = SeedSchedule(
        SeedPartition(_string(schedule_raw["partition"], "seed partition")),
        SeedPartitionSpec(
            held_out_numerator=_nonnegative_integer(
                schedule_raw["held_out_numerator"],
                "held_out_numerator",
            ),
            denominator=_positive_integer(
                schedule_raw["denominator"],
                "seed denominator",
            ),
        ),
        next_candidate=_u64_integer(
            schedule_raw["next_candidate"],
            "next seed candidate",
            inclusive_upper=True,
        ),
    )
    recovery_mode = RecoveryMode(
        _string(raw["recovery_mode"], "recovery mode")
    )
    max_recoveries = _nonnegative_integer(
        raw["max_recoveries_per_episode"],
        "max_recoveries_per_episode",
    )
    if (
        schedule.partition is SeedPartition.TRAINING
        and recovery_mode is not RecoveryMode.TRAINING
    ) or (
        schedule.partition is SeedPartition.HELD_OUT
        and recovery_mode is not RecoveryMode.HELD_OUT_ZERO_RECOVERY
    ):
        raise TorchResumeStateError("seed schedule and recovery mode differ")
    ledger_raw = raw["ledger"]
    if not isinstance(ledger_raw, list) or len(ledger_raw) != slot_count:
        raise TorchResumeStateError("resume ledger does not cover every slot")
    snapshots = []
    for expected_slot, row_value in enumerate(ledger_raw):
        row = _exact_mapping(
            row_value,
            {
                "slot_index",
                "episode_seed",
                "episode_generation",
                "attempt_index",
                "recoveries_used",
                "status",
            },
            "resume ledger row",
        )
        slot_index = _nonnegative_integer(row["slot_index"], "ledger slot_index")
        if slot_index != expected_slot:
            raise TorchResumeStateError("resume ledger slots are not contiguous")
        status = RecoverySlotStatus(_string(row["status"], "ledger status"))
        if status is not RecoverySlotStatus.ACTIVE:
            raise TorchResumeStateError("resume ledger contains terminal accounting")
        snapshots.append(
            RecoverySlotSnapshot(
                slot_index=slot_index,
                episode_seed=_u64_integer(row["episode_seed"], "episode seed"),
                episode_generation=_u64_integer(
                    row["episode_generation"],
                    "episode generation",
                ),
                attempt_index=_positive_integer(
                    row["attempt_index"],
                    "attempt index",
                ),
                recoveries_used=_nonnegative_integer(
                    row["recoveries_used"],
                    "recoveries used",
                ),
                status=status,
                pending_terminal=None,
            )
        )
    return BatchDriverResumeBoundary(
        slot_count=slot_count,
        schedule=schedule,
        recovery_mode=recovery_mode,
        max_recoveries_per_episode=max_recoveries,
        ledger_snapshots=tuple(snapshots),
        checkpoint_slots=checkpoint_slots,
        experience_next_sequence_index=experience_sequence,
    )


def _decode_assembler_snapshot(value: object) -> AttemptAssemblerSnapshot:
    raw = _exact_mapping(
        value,
        {
            "next_sequence_index",
            "open_attempts",
            "dropped_open_attempts",
            "retained_decisions",
            "retained_payload_bytes",
            "completed_attempts",
            "dropped_attempts",
        },
        "attempt assembler state",
    )
    return AttemptAssemblerSnapshot(
        **{
            name: _nonnegative_integer(raw[name], f"assembler {name}")
            for name in raw
        }
    )


def _decode_trainer_snapshot(value: object) -> SynchronousPolicyTrainerSnapshot:
    raw = _exact_mapping(
        value,
        {
            "deliveries",
            "optimizer_steps",
            "completed_attempts",
            "dropped_attempts",
            "trained_decisions",
            "last_loss",
            "last_behavior_manifest_ids",
            "last_selection_probabilities",
            "total_training_seconds",
            "last_training_seconds",
            "poisoned",
        },
        "trainer state",
    )
    manifest_rows = _decode_manifest_rows(raw["last_behavior_manifest_ids"])
    probability_rows = _decode_probability_rows(raw["last_selection_probabilities"])
    poisoned = raw["poisoned"]
    if type(poisoned) is not bool:
        raise TorchResumeStateError("trainer poisoned flag must be bool")
    return SynchronousPolicyTrainerSnapshot(
        deliveries=_nonnegative_integer(raw["deliveries"], "trainer deliveries"),
        optimizer_steps=_nonnegative_integer(
            raw["optimizer_steps"],
            "trainer optimizer_steps",
        ),
        completed_attempts=_nonnegative_integer(
            raw["completed_attempts"],
            "trainer completed_attempts",
        ),
        dropped_attempts=_nonnegative_integer(
            raw["dropped_attempts"],
            "trainer dropped_attempts",
        ),
        trained_decisions=_nonnegative_integer(
            raw["trained_decisions"],
            "trainer trained_decisions",
        ),
        last_loss=_optional_finite_float(raw["last_loss"], "trainer last_loss"),
        last_behavior_manifest_ids=manifest_rows,
        last_selection_probabilities=probability_rows,
        total_training_seconds=_finite_float(
            raw["total_training_seconds"],
            "trainer total_training_seconds",
        ),
        last_training_seconds=_optional_finite_float(
            raw["last_training_seconds"],
            "trainer last_training_seconds",
        ),
        poisoned=poisoned,
    )


def _decode_controller_snapshot(
    value: object,
) -> CategoricalTorchBehaviorControllerSnapshot:
    raw = _exact_mapping(
        value,
        {"active_manifest_id", "active_training_step", "successful_promotions"},
        "controller state",
    )
    manifest_raw = raw["active_manifest_id"]
    manifest_id = None
    if manifest_raw is not None:
        if not isinstance(manifest_raw, bytes):
            raise TorchResumeStateError("controller manifest id must be bytes")
        manifest_id = BehaviorManifestId(manifest_raw)
    step_raw = raw["active_training_step"]
    training_step = (
        None
        if step_raw is None
        else _nonnegative_integer(step_raw, "controller active_training_step")
    )
    return CategoricalTorchBehaviorControllerSnapshot(
        active_manifest_id=manifest_id,
        active_training_step=training_step,
        successful_promotions=_positive_integer(
            raw["successful_promotions"],
            "controller successful_promotions",
        ),
    )


def _decode_manifest_rows(
    value: object,
) -> tuple[tuple[BehaviorManifestId, ...], ...] | None:
    if value is None:
        return None
    if not isinstance(value, list):
        raise TorchResumeStateError("trainer manifest evidence must be a list")
    rows = []
    for row in value:
        if not isinstance(row, list):
            raise TorchResumeStateError("trainer manifest attempt must be a list")
        if not all(isinstance(digest, bytes) for digest in row):
            raise TorchResumeStateError("trainer manifest digest must be bytes")
        rows.append(tuple(BehaviorManifestId(digest) for digest in row))
    return tuple(rows)


def _decode_probability_rows(
    value: object,
) -> tuple[tuple[SelectionProbability, ...], ...] | None:
    if value is None:
        return None
    if not isinstance(value, list):
        raise TorchResumeStateError("trainer probability evidence must be a list")
    rows = []
    for row in value:
        if not isinstance(row, list):
            raise TorchResumeStateError("trainer probability attempt must be a list")
        rows.append(tuple(SelectionProbability(probability) for probability in row))
    return tuple(rows)


def _exact_mapping(
    value: object,
    fields: set[str],
    name: str,
) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != fields:
        raise TorchResumeStateError(f"{name} fields are unsupported")
    if not all(isinstance(key, str) for key in value):
        raise TorchResumeStateError(f"{name} keys must be strings")
    return value


def _string(value: object, name: str) -> str:
    if not isinstance(value, str):
        raise TorchResumeStateError(f"{name} must be a string")
    return value


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise TorchResumeStateError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchResumeStateError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchResumeStateError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchResumeStateError(f"{name} must be non-negative")
    return normalized


def _u64_integer(
    value: object,
    name: str,
    *,
    inclusive_upper: bool = False,
) -> int:
    normalized = _nonnegative_integer(value, name)
    upper = (1 << 64) + int(inclusive_upper)
    if normalized >= upper:
        raise TorchResumeStateError(f"{name} is outside its unsigned 64-bit range")
    return normalized


def _finite_float(value: object, name: str) -> float:
    if type(value) is not float or not math.isfinite(value) or value < 0.0:
        raise TorchResumeStateError(f"{name} must be a finite non-negative float")
    return value


def _optional_finite_float(value: object, name: str) -> float | None:
    if value is None:
        return None
    if type(value) is not float or not math.isfinite(value):
        raise TorchResumeStateError(f"{name} must be a finite float")
    return value
