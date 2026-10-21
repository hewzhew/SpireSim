"""Bounded online batch execution without trajectory retention."""

from __future__ import annotations

import operator
import time
from collections.abc import Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass
from typing import Protocol

from .decision_progress import DecisionProgressProvider
from .experience import (
    DecisionExperienceBatch,
    DecisionLineage,
    ExperienceSegment,
    ExperienceSegmentBuffer,
    PreparedDecisionBatch,
)
from .outcomes import TerminalStepBatch
from .policy import BatchPolicyChoice, SelectionProbability
from .semantic_batch import select_semantic_decision_rows
from .recovery import (
    EpisodeOutcome,
    RecoveryEvent,
    RecoveryLedger,
    RecoveryMode,
    RecoverySlotStatus,
    RecoverySlotSnapshot,
    TerminalAccountingBatch,
    TerminalAttemptRecord,
    restore_with_accounting,
)
from .seeds import (
    SeedPartition,
    SeedSchedule,
    reset_scheduled_checkpointed_with_accounting,
)


class BatchDriverError(ValueError):
    """A policy, curriculum, or environment batch violated the driver contract."""


def partition_routine_decisions(environment, batch):
    """Separate program-owned rows before inference and experience capture."""
    resolver = getattr(environment, "routine_choice_ordinals", None)
    routines = dict(resolver(batch)) if callable(resolver) else {}
    for row, ordinal in routines.items():
        if (type(row) is not int or not 0 <= row < len(batch["slot_indices"])
                or type(ordinal) is not int or not 0 <= ordinal < int(batch["candidate_counts"][row])):
            raise BatchDriverError("invalid routine decision ordinal")
    rows = tuple(row for row in range(len(batch["slot_indices"])) if row not in routines)
    selected = batch if not routines else (select_semantic_decision_rows(batch, rows) if rows else None)
    return selected, rows, routines


def merge_routine_ordinals(batch, rows, routines, ordinals):
    if len(rows) != len(ordinals):
        raise BatchDriverError("learned decision and ordinal counts are misaligned")
    chosen = dict(routines)
    chosen.update(zip(rows, ordinals, strict=True))
    return [chosen[row] for row in range(len(batch["slot_indices"]))]


class CheckpointBatch(Protocol):
    """Opaque checkpoint collection; contents remain owned by the bridge."""

    def __len__(self) -> int: ...

    def select(self, slot_indices: list[int]) -> CheckpointBatch: ...

    def updated(self, replacements: CheckpointBatch) -> CheckpointBatch: ...

    def checkpoint_bytes(self, *, max_bytes: int) -> bytes: ...


class BatchEnvironment(Protocol):
    """Narrow structural type implemented by the standalone Rust bridge."""

    @property
    def slot_count(self) -> int: ...

    @property
    def terminal_count(self) -> int: ...

    @property
    def ready(self) -> bool: ...

    def decision_batch(self, *, semantic: bool = False) -> Mapping[str, object]: ...

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None: ...

    def step(self) -> Mapping[str, object]: ...

    def checkpoint_slots(self, slot_indices: list[int]) -> CheckpointBatch: ...

    def restore_slots(
        self,
        slot_indices: list[int],
        checkpoints: object,
    ) -> None: ...

    def reset_slots(self, slot_indices: list[int], seeds: list[int]) -> None: ...

    def reset_slots_checkpointed(
        self,
        slot_indices: list[int],
        seeds: list[int],
    ) -> CheckpointBatch: ...

    def checkpoint_bytes(self, *, max_bytes: int) -> bytes: ...


class BatchPolicy(Protocol):
    """One inference call over every row in one ragged decision batch."""

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice: ...


@dataclass(frozen=True)
class RecoveryPlan:
    """Defeated slots to restore; every other defeat completes."""

    slot_indices: tuple[int, ...] = ()

    def __post_init__(self) -> None:
        slots = _normalize_integer_sequence(self.slot_indices, "recovery slots")
        if len(set(slots)) != len(slots):
            raise BatchDriverError("recovery plan contains duplicate slots")
        if any(slot < 0 for slot in slots):
            raise BatchDriverError("recovery slots must be non-negative")
        object.__setattr__(self, "slot_indices", slots)


class BatchCurriculum(Protocol):
    """Caller-owned retry choice over one complete terminal batch."""

    def plan_recovery(
        self,
        accounting: TerminalAccountingBatch,
        snapshots: tuple[RecoverySlotSnapshot, ...],
    ) -> RecoveryPlan: ...


class ExperienceSegmentSink(Protocol):
    """Synchronous consumer; the driver never queues sealed segments."""

    def __call__(self, segment: ExperienceSegment) -> None: ...


@dataclass(frozen=True)
class InitialPopulation:
    """Seed-aligned environment, ledger, cursor, and episode-root checkpoints."""

    env: BatchEnvironment
    ledger: RecoveryLedger
    schedule: SeedSchedule
    checkpoint_bank: CheckpointBatch


@dataclass(frozen=True)
class BatchDriverResumeBoundary:
    """Exact caller facts admitted at a quiescent driver boundary."""

    slot_count: int
    schedule: SeedSchedule
    recovery_mode: RecoveryMode
    max_recoveries_per_episode: int
    ledger_snapshots: tuple[RecoverySlotSnapshot, ...]
    checkpoint_slots: int
    experience_next_sequence_index: int | None


@dataclass(frozen=True)
class BatchStepResult:
    """Bounded facts from one vector environment transition."""

    decision_rounds: int
    slot_steps: int
    attempts: tuple[TerminalAttemptRecord, ...]
    completed_episodes: tuple[EpisodeOutcome, ...]
    recoveries: tuple[RecoveryEvent, ...]
    emitted_experience_segments: int
    emitted_experience_decisions: int
    emitted_experience_payload_bytes: int


@dataclass(frozen=True)
class TerminalProgressAggregate:
    """Bounded terminal progress facts without retaining trajectories."""

    attempts: int
    floor_sum: int
    min_floor: int | None
    max_floor: int | None
    floor_counts: tuple[tuple[int, int], ...]
    act_counts: tuple[tuple[int, int], ...]

    def __post_init__(self) -> None:
        for name in ("attempts", "floor_sum"):
            value = getattr(self, name)
            if type(value) is not int or value < 0:
                raise BatchDriverError(f"terminal progress {name} must be non-negative")
        if self.attempts == 0:
            if (
                self.floor_sum != 0
                or self.min_floor is not None
                or self.max_floor is not None
                or self.floor_counts
                or self.act_counts
            ):
                raise BatchDriverError("empty terminal progress must contain no outcomes")
            return
        if (
            type(self.min_floor) is not int
            or type(self.max_floor) is not int
            or self.min_floor < 0
            or self.max_floor < self.min_floor
        ):
            raise BatchDriverError("terminal progress floor range is invalid")
        if self.floor_sum < self.min_floor * self.attempts:
            raise BatchDriverError("terminal progress floor sum is below its minimum")
        if self.floor_sum > self.max_floor * self.attempts:
            raise BatchDriverError("terminal progress floor sum exceeds its maximum")
        if tuple(sorted(self.floor_counts)) != self.floor_counts:
            raise BatchDriverError("terminal progress floor counts must be sorted")
        if len({floor for floor, _ in self.floor_counts}) != len(
            self.floor_counts
        ):
            raise BatchDriverError("terminal progress floor counts repeat a floor")
        if any(
            type(floor) is not int
            or floor < 0
            or type(count) is not int
            or count <= 0
            for floor, count in self.floor_counts
        ):
            raise BatchDriverError("terminal progress floor counts are invalid")
        if sum(count for _, count in self.floor_counts) != self.attempts:
            raise BatchDriverError(
                "terminal progress floor counts disagree with attempts"
            )
        if sum(floor * count for floor, count in self.floor_counts) != self.floor_sum:
            raise BatchDriverError(
                "terminal progress floor counts disagree with floor sum"
            )
        if (
            self.floor_counts[0][0] != self.min_floor
            or self.floor_counts[-1][0] != self.max_floor
        ):
            raise BatchDriverError(
                "terminal progress floor counts disagree with floor range"
            )
        if tuple(sorted(self.act_counts)) != self.act_counts:
            raise BatchDriverError("terminal progress act counts must be sorted")
        if len({act for act, _ in self.act_counts}) != len(self.act_counts):
            raise BatchDriverError("terminal progress act counts repeat an act")
        if any(
            type(act) is not int
            or act < 0
            or type(count) is not int
            or count <= 0
            for act, count in self.act_counts
        ):
            raise BatchDriverError("terminal progress act counts are invalid")
        if sum(count for _, count in self.act_counts) != self.attempts:
            raise BatchDriverError("terminal progress act counts disagree with attempts")

    @property
    def mean_floor(self) -> float | None:
        if self.attempts == 0:
            return None
        return self.floor_sum / self.attempts


@dataclass(frozen=True)
class BatchRunSummary:
    """Compact aggregate from a bounded number of vector transitions."""

    mode: RecoveryMode
    active_slots: int
    batch_steps: int
    slot_steps: int
    decision_rounds: int
    terminal_attempts: int
    victories: int
    defeats: int
    terminal_progress: TerminalProgressAggregate
    completed_episodes: int
    recoveries: int
    emitted_experience_segments: int
    emitted_experience_decisions: int
    emitted_experience_payload_bytes: int
    open_experience_decisions: int
    open_experience_payload_bytes: int
    elapsed_seconds: float

    @property
    def steps_per_second(self) -> float:
        if self.elapsed_seconds == 0.0:
            return 0.0
        return self.slot_steps / self.elapsed_seconds


@dataclass(frozen=True)
class TerminalTargetRunResult:
    """Exact bounded prefix stopped by a terminal target or a step limit."""

    summary: BatchRunSummary
    terminal_attempt_target: int
    batch_step_limit: int

    @property
    def target_reached(self) -> bool:
        return self.summary.terminal_attempts >= self.terminal_attempt_target

    @property
    def step_limit_reached(self) -> bool:
        return (
            not self.target_reached
            and self.summary.batch_steps == self.batch_step_limit
        )


def initialize_population(
    env_factory: Callable[[list[int]], BatchEnvironment],
    *,
    slot_count: int,
    schedule: SeedSchedule,
    max_recoveries_per_episode: int,
) -> InitialPopulation:
    """Create all initial slot owners from one immutable seed plan."""

    normalized_slot_count = _normalize_count(
        slot_count,
        "slot_count",
        allow_zero=False,
    )
    slots = tuple(range(normalized_slot_count))
    recovery_limit = _normalize_count(
        max_recoveries_per_episode,
        "max_recoveries_per_episode",
        allow_zero=True,
    )
    initial, next_schedule = schedule.plan(slots)
    env = env_factory(list(initial.seeds))
    actual_slot_count = operator.index(env.slot_count)
    if actual_slot_count != len(slots):
        raise BatchDriverError(
            f"environment created {actual_slot_count} slots, expected {len(slots)}"
        )
    if operator.index(env.terminal_count) != 0:
        raise BatchDriverError("initial environment contains terminal slots")

    if schedule.partition is SeedPartition.TRAINING:
        ledger = RecoveryLedger.training(
            initial.seeds,
            max_recoveries_per_episode=recovery_limit,
        )
    else:
        if recovery_limit != 0:
            raise BatchDriverError("held-out population requires zero recoveries")
        ledger = RecoveryLedger.held_out(initial.seeds)

    checkpoint_bank = env.checkpoint_slots(list(slots))
    if len(checkpoint_bank) != len(slots):
        raise BatchDriverError(
            f"checkpoint bank contains {len(checkpoint_bank)} slots, expected {len(slots)}"
        )
    return InitialPopulation(
        env=env,
        ledger=ledger,
        schedule=next_schedule,
        checkpoint_bank=checkpoint_bank,
    )


class OnlineBatchDriver:
    """Execute aligned policy batches and resolve terminal slot lifecycles."""

    def __init__(
        self,
        population: InitialPopulation,
        *,
        policy: BatchPolicy,
        curriculum: BatchCurriculum,
        max_decision_rounds_per_step: int = 256,
        experience_buffer: ExperienceSegmentBuffer | None = None,
        experience_sink: ExperienceSegmentSink | None = None,
        decision_progress_provider: DecisionProgressProvider | None = None,
    ) -> None:
        if (experience_buffer is None) != (experience_sink is None):
            raise BatchDriverError(
                "experience_buffer and experience_sink must be configured together"
            )
        if decision_progress_provider is not None and not callable(
            getattr(decision_progress_provider, "capture", None)
        ):
            raise BatchDriverError(
                "decision_progress_provider must expose capture()"
            )
        self.env = population.env
        self.ledger = population.ledger
        self.schedule = population.schedule
        self._checkpoint_bank = population.checkpoint_bank
        self.policy = policy
        self.curriculum = curriculum
        self._experience_buffer = experience_buffer
        self._experience_sink = experience_sink
        self._experience_sink_failed = False
        self._decision_progress_provider = decision_progress_provider
        self.max_decision_rounds_per_step = _normalize_count(
            max_decision_rounds_per_step,
            "max_decision_rounds_per_step",
            allow_zero=False,
        )

    @property
    def experience_sink(self) -> ExperienceSegmentSink | None:
        """Return the configured synchronous sink owner without exposing a queue."""

        return self._experience_sink

    @property
    def checkpoint_bank(self) -> CheckpointBatch:
        """Return the opaque full episode-root bank to the resume owner."""

        return self._checkpoint_bank

    def require_resume_boundary(self) -> BatchDriverResumeBoundary:
        """Reject mutable half-states before any process-resume publication."""

        if self._experience_sink_failed:
            raise BatchDriverError("experience sink previously failed")
        slot_count = self.ledger.slot_count
        if operator.index(self.env.slot_count) != slot_count:
            raise BatchDriverError("environment and recovery ledger slot counts differ")
        if operator.index(self.env.terminal_count) != 0:
            raise BatchDriverError("resume boundary contains terminal environment slots")
        if bool(self.env.ready):
            raise BatchDriverError("resume boundary contains prepared environment actions")
        snapshots = self.ledger.snapshots()
        for snapshot in snapshots:
            if (
                snapshot.status is not RecoverySlotStatus.ACTIVE
                or snapshot.pending_terminal is not None
            ):
                raise BatchDriverError(
                    f"resume boundary slot {snapshot.slot_index} has unfinished terminal accounting"
                )
        if len(self._checkpoint_bank) != slot_count:
            raise BatchDriverError(
                "episode-root checkpoint bank does not cover every environment slot"
            )
        if self.schedule.partition is SeedPartition.TRAINING:
            expected_mode = RecoveryMode.TRAINING
        else:
            expected_mode = RecoveryMode.HELD_OUT_ZERO_RECOVERY
        if self.ledger.mode is not expected_mode:
            raise BatchDriverError("seed schedule and recovery ledger modes differ")

        experience_sequence = None
        if self._experience_buffer is not None:
            if not self._experience_buffer.empty:
                raise BatchDriverError("resume boundary requires a flushed experience buffer")
            experience_sequence = self._experience_buffer.next_sequence_index
        return BatchDriverResumeBoundary(
            slot_count=slot_count,
            schedule=self.schedule,
            recovery_mode=self.ledger.mode,
            max_recoveries_per_episode=self.ledger.max_recoveries_per_episode,
            ledger_snapshots=snapshots,
            checkpoint_slots=len(self._checkpoint_bank),
            experience_next_sequence_index=experience_sequence,
        )

    def advance(self, *, refill_completed: bool = True) -> BatchStepResult:
        """Resolve one step, optionally parking completed slots at a cohort barrier."""

        if self._experience_sink_failed:
            raise BatchDriverError("experience sink previously failed")
        if not isinstance(refill_completed, bool):
            raise BatchDriverError("refill_completed must be bool")
        decision_rounds = 0
        experience_segments = 0
        experience_decisions = 0
        experience_payload_bytes = 0
        while not self.env.ready:
            if decision_rounds >= self.max_decision_rounds_per_step:
                raise BatchDriverError(
                    "policy did not finish symbolic selection within "
                    f"{self.max_decision_rounds_per_step} rounds"
                )
            source_batch = self.env.decision_batch(semantic=True)
            decision_batch, learned_rows, routines = partition_routine_decisions(self.env, source_batch)
            if decision_batch is None:
                self.env.choose(list(source_batch["decision_token_ids"]),
                                merge_routine_ordinals(source_batch, learned_rows, routines, ()))
                decision_rounds += 1
                continue
            slots = _mapping_integer_sequence(
                decision_batch,
                "slot_indices",
                "decision slot indices",
            )
            candidate_counts = _mapping_integer_sequence(
                decision_batch,
                "candidate_counts",
                "candidate counts",
            )
            if len(slots) != len(candidate_counts):
                raise BatchDriverError("decision slot and candidate columns are misaligned")
            decision_token_ids = _mapping_string_sequence(
                decision_batch,
                "decision_token_ids",
                "decision token ids",
            )
            if len(decision_token_ids) != len(slots):
                raise BatchDriverError("decision tokens and decision rows are misaligned")
            prepared_experience: PreparedDecisionBatch | None = None
            if self._experience_buffer is not None:
                run_progress = (
                    None
                    if self._decision_progress_provider is None
                    else self._decision_progress_provider.capture(slots)
                )
                prepared_experience = self._experience_buffer.prepare(
                    decision_batch,
                    tuple(self.ledger.snapshot(slot) for slot in slots),
                    run_progress,
                )
            choice = self.policy.choose(decision_batch)
            if not isinstance(choice, BatchPolicyChoice):
                raise BatchDriverError("policy must return BatchPolicyChoice")
            ordinals = choice.ordinals
            if len(ordinals) != len(slots):
                raise BatchDriverError(
                    f"policy returned {len(ordinals)} ordinals for {len(slots)} rows"
                )
            probabilities = choice.selection_probabilities
            if len(probabilities) != len(slots):
                raise BatchDriverError(
                    "policy selection probabilities must contain one value per row"
                )
            if not all(
                isinstance(probability, SelectionProbability)
                for probability in probabilities
            ):
                raise BatchDriverError(
                    "policy selection probabilities must be typed"
                )
            for slot, ordinal, count in zip(
                slots,
                ordinals,
                candidate_counts,
                strict=True,
            ):
                if count <= 0:
                    raise BatchDriverError(f"slot {slot} has no legal candidates")
                if not 0 <= ordinal < count:
                    raise BatchDriverError(
                        f"slot {slot} candidate ordinal {ordinal} is outside 0..{count}"
                    )
            if prepared_experience is not None:
                experience_batch = DecisionExperienceBatch.from_prepared(
                    prepared_experience,
                    ordinals,
                    probabilities,
                    choice.behavior_manifest_id,
                )
                emitted = self._experience_buffer.rotate_before(experience_batch)
                segments, decisions, payload_bytes = self._consume_experience(emitted)
                experience_segments += segments
                experience_decisions += decisions
                experience_payload_bytes += payload_bytes
            self.env.choose(list(source_batch["decision_token_ids"]),
                            merge_routine_ordinals(source_batch, learned_rows, routines, ordinals))
            if prepared_experience is not None:
                self._experience_buffer.commit(experience_batch)
            decision_rounds += 1

        terminal_lineages = {
            snapshot.slot_index: DecisionLineage.from_snapshot(snapshot)
            for snapshot in self.ledger.snapshots() if snapshot.status is RecoverySlotStatus.ACTIVE
        } if self._experience_buffer is not None else {}
        bridge_step = self.env.step()
        slot_steps = len(
            _mapping_integer_sequence(
                bridge_step,
                "slot_indices",
                "step slot indices",
            )
        )
        terminal = TerminalStepBatch.from_bridge_step(
            bridge_step,
            slot_count=self.ledger.slot_count,
        )
        if not terminal.attempts:
            return BatchStepResult(
                decision_rounds=decision_rounds,
                slot_steps=slot_steps,
                attempts=(),
                completed_episodes=(),
                recoveries=(),
                emitted_experience_segments=experience_segments,
                emitted_experience_decisions=experience_decisions,
                emitted_experience_payload_bytes=experience_payload_bytes,
            )

        accounting = self.ledger.record_terminal(terminal)
        if self._experience_buffer is not None:
            self._experience_buffer.record_terminals(accounting.attempts,
                continuation_lineages=tuple(terminal_lineages[record.slot_index] for record in accounting.attempts))
        snapshots = tuple(
            self.ledger.snapshot(slot) for slot in terminal.slot_indices
        )
        plan = self.curriculum.plan_recovery(accounting, snapshots)
        if not isinstance(plan, RecoveryPlan):
            raise BatchDriverError("curriculum must return a RecoveryPlan")
        defeat_slots = tuple(
            attempt.slot_index
            for attempt in accounting.attempts
            if attempt.terminal_reward == -1
        )
        unexpected = set(plan.slot_indices).difference(defeat_slots)
        if unexpected:
            raise BatchDriverError(
                "recovery plan contains non-defeat slots: "
                + ", ".join(str(slot) for slot in sorted(unexpected))
            )

        recoveries: tuple[RecoveryEvent, ...] = ()
        if plan.slot_indices:
            checkpoints = self._checkpoint_bank.select(list(plan.slot_indices))
            recoveries = restore_with_accounting(
                self.env,
                plan.slot_indices,
                checkpoints,
                self.ledger,
            )

        recover_set = set(plan.slot_indices)
        completed_defeat_slots = tuple(
            slot for slot in defeat_slots if slot not in recover_set
        )
        completed_defeats = self.ledger.complete_defeats(completed_defeat_slots)
        completed_by_slot = {
            outcome.slot_index: outcome
            for outcome in accounting.completed_episodes + completed_defeats
        }
        completed = tuple(
            completed_by_slot[attempt.slot_index]
            for attempt in accounting.attempts
            if attempt.slot_index in completed_by_slot
        )
        if completed and refill_completed:
            reset_slots = tuple(outcome.slot_index for outcome in completed)
            (
                _,
                next_schedule,
                replacements,
            ) = reset_scheduled_checkpointed_with_accounting(
                self.env,
                reset_slots,
                self.ledger,
                self.schedule,
            )
            self._checkpoint_bank = self._checkpoint_bank.updated(replacements)
            self.schedule = next_schedule

        return BatchStepResult(
            decision_rounds=decision_rounds,
            slot_steps=slot_steps,
            attempts=accounting.attempts,
            completed_episodes=completed,
            recoveries=recoveries,
            emitted_experience_segments=experience_segments,
            emitted_experience_decisions=experience_decisions,
            emitted_experience_payload_bytes=experience_payload_bytes,
        )

    def refill_completed(self) -> tuple[int, ...]:
        """Reset every currently parked terminal slot as one scheduled cohort."""

        if self._experience_sink_failed:
            raise BatchDriverError("experience sink previously failed")
        terminal_count = operator.index(self.env.terminal_count)
        completed = {
            RecoverySlotStatus.VICTORY_COMPLETE,
            RecoverySlotStatus.DEFEAT_COMPLETE,
        }
        reset_slots = tuple(
            snapshot.slot_index
            for snapshot in self.ledger.snapshots()
            if snapshot.status in completed
        )
        if len(reset_slots) != terminal_count:
            raise BatchDriverError(
                "terminal environment slots and completed ledger slots differ"
            )
        if not reset_slots:
            raise BatchDriverError("no completed slots are parked for refill")
        if self._experience_buffer is not None and not self._experience_buffer.empty:
            raise BatchDriverError("completed-slot refill requires flushed experience")
        (
            _,
            next_schedule,
            replacements,
        ) = reset_scheduled_checkpointed_with_accounting(
            self.env,
            reset_slots,
            self.ledger,
            self.schedule,
        )
        self._checkpoint_bank = self._checkpoint_bank.updated(replacements)
        self.schedule = next_schedule
        return reset_slots

    def flush_experience(self) -> ExperienceSegment | None:
        """Seal and synchronously consume the current bounded segment."""

        if self._experience_sink_failed:
            raise BatchDriverError("experience sink previously failed")
        if self._experience_buffer is None:
            return None
        segment = self._experience_buffer.flush()
        if segment is not None:
            self._consume_experience((segment,))
        return segment

    def run(self, *, batch_steps: int) -> BatchRunSummary:
        """Run a fixed number of vector transitions without retaining results."""

        requested_steps = _normalize_count(batch_steps, "batch_steps", allow_zero=True)
        return self._run_normalized(
            batch_step_limit=requested_steps,
            terminal_attempt_target=None,
        )

    def run_until_terminal_attempts(
        self,
        *,
        terminal_attempts: int,
        max_batch_steps: int,
    ) -> TerminalTargetRunResult:
        """Run an atomic step prefix until its terminal target or explicit limit."""

        target = _normalize_count(
            terminal_attempts,
            "terminal_attempts",
            allow_zero=True,
        )
        step_limit = _normalize_count(
            max_batch_steps,
            "max_batch_steps",
            allow_zero=True,
        )
        summary = self._run_normalized(
            batch_step_limit=step_limit,
            terminal_attempt_target=target,
        )
        return TerminalTargetRunResult(
            summary=summary,
            terminal_attempt_target=target,
            batch_step_limit=step_limit,
        )

    def _run_normalized(
        self,
        *,
        batch_step_limit: int,
        terminal_attempt_target: int | None,
    ) -> BatchRunSummary:
        slot_steps = 0
        decision_rounds = 0
        terminal_attempts = 0
        victories = 0
        defeats = 0
        terminal_floor_sum = 0
        min_terminal_floor: int | None = None
        max_terminal_floor: int | None = None
        terminal_floor_counts: dict[int, int] = {}
        terminal_act_counts: dict[int, int] = {}
        completed_episodes = 0
        recoveries = 0
        experience_segments = 0
        experience_decisions = 0
        experience_payload_bytes = 0
        batch_steps = 0
        started = time.perf_counter()
        for _ in range(batch_step_limit):
            if (
                terminal_attempt_target is not None
                and terminal_attempts >= terminal_attempt_target
            ):
                break
            result = self.advance()
            batch_steps += 1
            slot_steps += result.slot_steps
            decision_rounds += result.decision_rounds
            terminal_attempts += len(result.attempts)
            victories += sum(
                attempt.terminal_reward == 1 for attempt in result.attempts
            )
            defeats += sum(
                attempt.terminal_reward == -1 for attempt in result.attempts
            )
            for attempt in result.attempts:
                floor = attempt.terminal.terminal_floor
                terminal_floor_sum += floor
                min_terminal_floor = (
                    floor
                    if min_terminal_floor is None
                    else min(min_terminal_floor, floor)
                )
                max_terminal_floor = (
                    floor
                    if max_terminal_floor is None
                    else max(max_terminal_floor, floor)
                )
                terminal_floor_counts[floor] = (
                    terminal_floor_counts.get(floor, 0) + 1
                )
                act = attempt.terminal.terminal_act
                terminal_act_counts[act] = terminal_act_counts.get(act, 0) + 1
            completed_episodes += len(result.completed_episodes)
            recoveries += len(result.recoveries)
            experience_segments += result.emitted_experience_segments
            experience_decisions += result.emitted_experience_decisions
            experience_payload_bytes += result.emitted_experience_payload_bytes
        elapsed = time.perf_counter() - started
        open_experience_decisions = (
            self._experience_buffer.decision_count
            if self._experience_buffer is not None
            else 0
        )
        open_experience_payload_bytes = (
            self._experience_buffer.payload_bytes
            if self._experience_buffer is not None
            else 0
        )
        return BatchRunSummary(
            mode=self.ledger.mode,
            active_slots=self.ledger.slot_count - operator.index(self.env.terminal_count),
            batch_steps=batch_steps,
            slot_steps=slot_steps,
            decision_rounds=decision_rounds,
            terminal_attempts=terminal_attempts,
            victories=victories,
            defeats=defeats,
            terminal_progress=TerminalProgressAggregate(
                attempts=terminal_attempts,
                floor_sum=terminal_floor_sum,
                min_floor=min_terminal_floor,
                max_floor=max_terminal_floor,
                floor_counts=tuple(sorted(terminal_floor_counts.items())),
                act_counts=tuple(sorted(terminal_act_counts.items())),
            ),
            completed_episodes=completed_episodes,
            recoveries=recoveries,
            emitted_experience_segments=experience_segments,
            emitted_experience_decisions=experience_decisions,
            emitted_experience_payload_bytes=experience_payload_bytes,
            open_experience_decisions=open_experience_decisions,
            open_experience_payload_bytes=open_experience_payload_bytes,
            elapsed_seconds=elapsed,
        )

    def _consume_experience(
        self,
        segments: Sequence[ExperienceSegment],
    ) -> tuple[int, int, int]:
        if not segments:
            return 0, 0, 0
        sink = self._experience_sink
        if sink is None:
            raise AssertionError("experience segments exist without a sink")
        decisions = 0
        payload_bytes = 0
        try:
            for segment in segments:
                sink(segment)
                decisions += segment.decision_count
                payload_bytes += segment.payload_bytes
        except Exception:
            self._experience_sink_failed = True
            raise
        return len(segments), decisions, payload_bytes


def _normalize_count(value: int, name: str, *, allow_zero: bool) -> int:
    if isinstance(value, bool):
        raise BatchDriverError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise BatchDriverError(f"{name} must be an integer") from error
    lower = 0 if allow_zero else 1
    if normalized < lower:
        relation = "non-negative" if allow_zero else "positive"
        raise BatchDriverError(f"{name} must be {relation}")
    return normalized


def _mapping_integer_sequence(
    mapping: Mapping[str, object],
    key: str,
    name: str,
) -> tuple[int, ...]:
    try:
        raw = mapping[key]
    except KeyError as error:
        raise BatchDriverError(f"batch is missing {key}") from error
    return _normalize_integer_sequence(raw, name)


def _mapping_string_sequence(
    mapping: Mapping[str, object],
    key: str,
    name: str,
) -> tuple[str, ...]:
    try:
        raw = mapping[key]
    except KeyError as error:
        raise BatchDriverError(f"batch is missing {key}") from error
    if isinstance(raw, (str, bytes)) or not isinstance(raw, Iterable):
        raise BatchDriverError(f"{name} must be an iterable of strings")
    values = tuple(raw)
    if not all(isinstance(value, str) and value for value in values):
        raise BatchDriverError(f"{name} must contain only non-empty strings")
    return values


def _normalize_integer_sequence(raw: object, name: str) -> tuple[int, ...]:
    if isinstance(raw, (str, bytes)) or not isinstance(raw, Iterable):
        raise BatchDriverError(f"{name} must be an iterable of integers")
    normalized = []
    for value in raw:
        if isinstance(value, bool):
            raise BatchDriverError(f"{name} must not contain bool")
        try:
            normalized.append(operator.index(value))
        except TypeError as error:
            raise BatchDriverError(f"{name} must contain only integers") from error
    return tuple(normalized)
