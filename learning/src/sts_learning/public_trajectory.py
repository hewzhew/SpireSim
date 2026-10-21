"""Neutral complete-attempt trajectories over sanitized public decisions."""

from __future__ import annotations

import operator
from collections.abc import Mapping
from dataclasses import dataclass

from .attempts import CompletedAttemptExperience
from .decision_progress import DecisionRunProgress, PublicDecisionSnapshot
from .decision_rows import (
    DecisionRowError,
    PreparedDecisionRows,
    normalize_integer_sequence,
)
from .experience import DecisionExperienceBatch, DecisionLineage
from .policy import BehaviorManifestId, SelectionProbability
from .recovery import TerminalAttemptRecord
from .semantic_batch import SemanticBatchError, select_semantic_decision_rows


class PublicTrajectoryError(ValueError):
    """Complete experience cannot form one neutral public trajectory."""


@dataclass(frozen=True)
class PublicTrajectoryDecisionV1:
    """One chronological behavior decision with only raw environment reward."""

    chronological_index: int
    lineage: DecisionLineage
    run_progress: DecisionRunProgress
    public_snapshot: PublicDecisionSnapshot
    semantic_payload: Mapping[str, object]
    behavior_manifest_id: BehaviorManifestId
    selected_ordinal: int
    selection_probability: SelectionProbability
    environment_reward: int
    terminated: bool

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "chronological_index",
            _nonnegative_integer(self.chronological_index, "chronological_index"),
        )
        if not isinstance(self.lineage, DecisionLineage):
            raise PublicTrajectoryError("public trajectory lineage must be typed")
        if not isinstance(self.run_progress, DecisionRunProgress):
            raise PublicTrajectoryError("public trajectory progress must be typed")
        if not isinstance(self.public_snapshot, PublicDecisionSnapshot):
            raise PublicTrajectoryError("public trajectory snapshot must be typed")
        if self.run_progress.public_snapshot != self.public_snapshot:
            raise PublicTrajectoryError(
                "public trajectory progress and snapshot disagree"
            )
        if self.run_progress.episode_seed != self.lineage.key.episode_seed:
            raise PublicTrajectoryError(
                "public trajectory progress seed disagrees with lineage"
            )
        object.__setattr__(
            self,
            "semantic_payload",
            _freeze_public_semantic_payload(
                self.semantic_payload,
                self.lineage,
                self.public_snapshot,
            ),
        )
        if not isinstance(self.behavior_manifest_id, BehaviorManifestId):
            raise PublicTrajectoryError(
                "public trajectory behavior manifest identity must be typed"
            )
        object.__setattr__(
            self,
            "selected_ordinal",
            _nonnegative_integer(self.selected_ordinal, "selected_ordinal"),
        )
        if self.selected_ordinal >= len(self.public_snapshot.candidate_ids):
            raise PublicTrajectoryError(
                "public trajectory selected ordinal is outside its candidate surface"
            )
        if not isinstance(self.selection_probability, SelectionProbability):
            raise PublicTrajectoryError(
                "public trajectory selection probability must be typed"
            )
        reward = _integer(self.environment_reward, "environment_reward")
        if type(self.terminated) is not bool:
            raise PublicTrajectoryError("public trajectory terminated must be bool")
        if self.terminated:
            if reward not in (-1, 1):
                raise PublicTrajectoryError(
                    "terminal public trajectory reward must be -1 or 1"
                )
        elif reward != 0:
            raise PublicTrajectoryError(
                "non-terminal public trajectory reward must be zero"
            )
        object.__setattr__(self, "environment_reward", reward)

    @property
    def selected_candidate_id(self) -> str:
        return self.public_snapshot.candidate_ids[self.selected_ordinal]


@dataclass(frozen=True)
class PublicAttemptTrajectoryV1:
    """One terminal attempt without returns, advantages, or teacher labels."""

    lineage: DecisionLineage
    decisions: tuple[PublicTrajectoryDecisionV1, ...]
    terminal: TerminalAttemptRecord

    def __post_init__(self) -> None:
        if not isinstance(self.lineage, DecisionLineage):
            raise PublicTrajectoryError("public attempt lineage must be typed")
        try:
            decisions = tuple(self.decisions)
        except TypeError as error:
            raise PublicTrajectoryError(
                "public attempt decisions must be a sequence"
            ) from error
        object.__setattr__(self, "decisions", decisions)
        if not decisions:
            raise PublicTrajectoryError("public attempt trajectory has no decisions")
        if not all(
            isinstance(decision, PublicTrajectoryDecisionV1)
            for decision in decisions
        ):
            raise PublicTrajectoryError("public attempt decisions must be typed")
        if not isinstance(self.terminal, TerminalAttemptRecord):
            raise PublicTrajectoryError("public attempt terminal must be typed")
        for index, decision in enumerate(decisions):
            if decision.chronological_index != index:
                raise PublicTrajectoryError(
                    "public attempt chronological indices are not contiguous"
                )
            if decision.lineage != self.lineage:
                raise PublicTrajectoryError(
                    "public attempt decision lineage changed within one attempt"
                )
            if decision.terminated != (index + 1 == len(decisions)):
                raise PublicTrajectoryError(
                    "only the final public attempt decision may be terminal"
                )
        _validate_terminal_lineage(self.lineage, self.terminal)
        if decisions[-1].environment_reward != self.terminal.terminal_reward:
            raise PublicTrajectoryError(
                "public attempt terminal reward disagrees with its final decision"
            )


def build_public_attempt_trajectory(
    attempt: CompletedAttemptExperience,
) -> PublicAttemptTrajectoryV1:
    """Freeze one complete behavior attempt without assigning policy credit."""

    if not isinstance(attempt, CompletedAttemptExperience):
        raise PublicTrajectoryError(
            "public trajectory requires CompletedAttemptExperience"
        )
    if not attempt.batches:
        raise PublicTrajectoryError("complete attempt has no policy decisions")
    if attempt.decision_count != len(attempt.batches):
        raise PublicTrajectoryError(
            "complete attempt must contain one chronological row per batch"
        )
    if attempt.payload_bytes != sum(batch.payload_bytes for batch in attempt.batches):
        raise PublicTrajectoryError(
            "complete attempt payload bytes disagree with its batches"
        )
    _validate_terminal_lineage(attempt.lineage, attempt.terminal)

    decisions = tuple(
        _build_public_decision(
            attempt,
            batch,
            chronological_index=index,
            terminated=index + 1 == len(attempt.batches),
        )
        for index, batch in enumerate(attempt.batches)
    )
    return PublicAttemptTrajectoryV1(
        lineage=attempt.lineage,
        decisions=decisions,
        terminal=attempt.terminal,
    )


def _build_public_decision(
    attempt: CompletedAttemptExperience,
    batch: DecisionExperienceBatch,
    *,
    chronological_index: int,
    terminated: bool,
) -> PublicTrajectoryDecisionV1:
    if not isinstance(batch, DecisionExperienceBatch):
        raise PublicTrajectoryError("public trajectory batch must be typed")
    if batch.decision_count != 1:
        raise PublicTrajectoryError(
            "one attempt may have only one row in one environment step"
        )
    if batch.lineages != (attempt.lineage,):
        raise PublicTrajectoryError("decision row disagrees with attempt lineage")
    if batch.run_progress is None or len(batch.run_progress) != 1:
        raise PublicTrajectoryError(
            "public trajectory requires one decision-time progress row per batch"
        )
    progress = batch.run_progress[0]
    if progress.episode_seed != attempt.lineage.key.episode_seed:
        raise PublicTrajectoryError(
            "decision progress seed changed within one attempt"
        )
    snapshot = progress.public_snapshot
    if snapshot is None:
        raise PublicTrajectoryError(
            "public trajectory requires a sanitized snapshot for every decision"
        )

    if len(batch.selected_ordinals) != 1:
        raise PublicTrajectoryError("decision selected ordinal is misaligned")
    selected_ordinal = batch.selected_ordinals[0]
    if len(batch.selection_probabilities) != 1:
        raise PublicTrajectoryError(
            "decision selection probability is misaligned"
        )
    selection_probability = batch.selection_probabilities[0]
    if not isinstance(selection_probability, SelectionProbability):
        raise PublicTrajectoryError("selection probability must be typed")
    if not isinstance(batch.behavior_manifest_id, BehaviorManifestId):
        raise PublicTrajectoryError("behavior manifest identity must be typed")

    return PublicTrajectoryDecisionV1(
        chronological_index=chronological_index,
        lineage=attempt.lineage,
        run_progress=progress,
        public_snapshot=snapshot,
        semantic_payload=batch.payload,
        behavior_manifest_id=batch.behavior_manifest_id,
        selected_ordinal=selected_ordinal,
        selection_probability=selection_probability,
        environment_reward=(attempt.terminal.terminal_reward if terminated else 0),
        terminated=terminated,
    )


def _freeze_public_semantic_payload(
    payload: Mapping[str, object],
    lineage: DecisionLineage,
    snapshot: PublicDecisionSnapshot,
) -> Mapping[str, object]:
    if not isinstance(payload, Mapping):
        raise PublicTrajectoryError("public trajectory payload must be a mapping")
    try:
        rows = PreparedDecisionRows.capture(payload)
        if rows.decision_count != 1:
            raise PublicTrajectoryError(
                "attempt-local payload must contain one decision row"
            )
        selected = select_semantic_decision_rows(rows.payload, [0])
        prepared = PreparedDecisionRows.capture(selected)
        phases = normalize_integer_sequence(prepared.payload["phase"], "phase")
    except (KeyError, DecisionRowError, SemanticBatchError) as error:
        raise PublicTrajectoryError("decision payload is malformed") from error
    if prepared.slot_indices != (lineage.key.slot_index,):
        raise PublicTrajectoryError("decision payload slot disagrees with lineage")
    if len(snapshot.candidate_ids) != prepared.candidate_counts[0]:
        raise PublicTrajectoryError(
            "public snapshot candidates disagree with decision payload"
        )
    if phases != (snapshot.phase,):
        raise PublicTrajectoryError(
            "public snapshot phase disagrees with decision payload"
        )
    return prepared.payload


def _validate_terminal_lineage(
    lineage: DecisionLineage,
    terminal: TerminalAttemptRecord,
) -> None:
    if not isinstance(lineage, DecisionLineage):
        raise PublicTrajectoryError("public trajectory lineage must be typed")
    if not isinstance(terminal, TerminalAttemptRecord):
        raise PublicTrajectoryError("public trajectory terminal must be typed")
    key = lineage.key
    if (
        terminal.slot_index != key.slot_index
        or terminal.episode_seed != key.episode_seed
        or terminal.episode_generation != key.episode_generation
        or terminal.attempt_index != key.attempt_index
        or terminal.recoveries_used != lineage.recoveries_used
    ):
        raise PublicTrajectoryError(
            "terminal record disagrees with public trajectory lineage"
        )


def _nonnegative_integer(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized < 0:
        raise PublicTrajectoryError(f"{name} must be non-negative")
    return normalized


def _integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise PublicTrajectoryError(f"{name} must be an integer, not bool")
    try:
        return operator.index(value)
    except TypeError as error:
        raise PublicTrajectoryError(f"{name} must be an integer") from error
