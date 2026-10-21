"""Bounded semantic experience for one exact same-root combat group."""

from __future__ import annotations

import math
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass

from .combat_outcomes import (
    CombatGroupedAdvantages,
    CombatOutcomeError,
    CompletedCombatGroup,
    validate_combat_digest,
)
from .combat_signals import (
    CombatGroupSignalSummary,
    summarize_combat_group_signals,
)
from .decision_rows import (
    DecisionRowError,
    PreparedDecisionRows,
    normalize_decision_choice,
)
from .policy import (
    BatchPolicyChoice,
    BehaviorManifestId,
    SelectionProbability,
)


class CombatExperienceError(RuntimeError):
    """A combat-group experience transition or resource bound was invalid."""


@dataclass(frozen=True)
class CombatExperienceLimits:
    max_decisions: int
    max_payload_bytes: int
    max_model_rounds: int
    max_transitions: int

    def __post_init__(self) -> None:
        for name in (
            "max_decisions",
            "max_payload_bytes",
            "max_model_rounds",
            "max_transitions",
        ):
            object.__setattr__(
                self,
                name,
                _positive_integer(getattr(self, name), name),
            )


@dataclass(frozen=True)
class CombatDecisionProgress:
    """Compact public combat state aligned to one policy decision row."""

    replicate_index: int
    turn: int
    player_hp: int
    player_max_hp: int
    enemy_hp: int
    enemy_max_hp: int
    potion_uuids: tuple[int | None, ...]
    potion_ids: tuple[str | None, ...]

    def __post_init__(self) -> None:
        replicate_index = _nonnegative_integer(
            self.replicate_index,
            "replicate_index",
        )
        turn = _nonnegative_integer(self.turn, "combat turn")
        player_hp = _nonnegative_integer(self.player_hp, "player_hp")
        player_max_hp = _positive_integer(self.player_max_hp, "player_max_hp")
        enemy_hp = _nonnegative_integer(self.enemy_hp, "enemy_hp")
        enemy_max_hp = _positive_integer(self.enemy_max_hp, "enemy_max_hp")
        if player_hp > player_max_hp:
            raise CombatExperienceError("player_hp exceeds player_max_hp")
        if enemy_hp > enemy_max_hp:
            raise CombatExperienceError("enemy_hp exceeds enemy_max_hp")
        potion_uuids = tuple(
            None
            if value is None
            else _nonnegative_integer(value, "potion_uuid")
            for value in self.potion_uuids
        )
        potion_ids = tuple(self.potion_ids)
        if len(potion_uuids) != len(potion_ids):
            raise CombatExperienceError("combat progress potion columns are misaligned")
        for potion_uuid, potion_id in zip(potion_uuids, potion_ids, strict=True):
            if (potion_uuid is None) != (potion_id is None):
                raise CombatExperienceError(
                    "combat progress potion identity columns disagree"
                )
            if potion_id is not None and (
                not isinstance(potion_id, str) or not potion_id
            ):
                raise CombatExperienceError("potion_id must be a non-empty string")
        object.__setattr__(self, "replicate_index", replicate_index)
        object.__setattr__(self, "turn", turn)
        object.__setattr__(self, "player_hp", player_hp)
        object.__setattr__(self, "player_max_hp", player_max_hp)
        object.__setattr__(self, "enemy_hp", enemy_hp)
        object.__setattr__(self, "enemy_max_hp", enemy_max_hp)
        object.__setattr__(self, "potion_uuids", potion_uuids)
        object.__setattr__(self, "potion_ids", potion_ids)


@dataclass(frozen=True)
class CombatChronologicalDecisionStep:
    """One replicate-local policy step recovered from batch chronology."""

    sequence_index: int
    progress: CombatDecisionProgress
    selected_ordinal: int
    selection_probability: SelectionProbability

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "sequence_index",
            _nonnegative_integer(self.sequence_index, "sequence_index"),
        )
        if not isinstance(self.progress, CombatDecisionProgress):
            raise CombatExperienceError(
                "chronological combat step requires typed progress"
            )
        object.__setattr__(
            self,
            "selected_ordinal",
            _nonnegative_integer(self.selected_ordinal, "selected_ordinal"),
        )
        if not isinstance(self.selection_probability, SelectionProbability):
            raise CombatExperienceError(
                "chronological combat step requires typed probability"
            )


@dataclass(frozen=True)
class CombatDecisionExperienceBatch:
    """One immutable semantic model call aligned to combat replicate indices."""

    sequence_index: int
    root_id: str
    exact_combat_state_hash: str
    replicate_indices: tuple[int, ...]
    decision_progress: tuple[CombatDecisionProgress, ...]
    payload: Mapping[str, object]
    selected_ordinals: tuple[int, ...]
    selection_probabilities: tuple[SelectionProbability, ...]
    behavior_manifest_id: BehaviorManifestId
    decision_count: int
    payload_bytes: int

    def __post_init__(self) -> None:
        _validate_root(self.root_id, self.exact_combat_state_hash)
        sequence_index = _nonnegative_integer(self.sequence_index, "sequence_index")
        replicates = tuple(
            _nonnegative_integer(value, "replicate_index")
            for value in self.replicate_indices
        )
        ordinals = tuple(
            _nonnegative_integer(value, "selected_ordinal")
            for value in self.selected_ordinals
        )
        probabilities = tuple(self.selection_probabilities)
        progress = tuple(self.decision_progress)
        decision_count = _positive_integer(self.decision_count, "decision_count")
        payload_bytes = _positive_integer(self.payload_bytes, "payload_bytes")
        if not isinstance(self.payload, Mapping):
            raise CombatExperienceError("combat decision payload must be a mapping")
        if not isinstance(self.behavior_manifest_id, BehaviorManifestId):
            raise CombatExperienceError(
                "combat decision batch requires a BehaviorManifestId"
            )
        if not all(
            isinstance(probability, SelectionProbability)
            for probability in probabilities
        ):
            raise CombatExperienceError(
                "combat decision probabilities must be typed"
            )
        if not all(isinstance(row, CombatDecisionProgress) for row in progress):
            raise CombatExperienceError("combat decision progress must be typed")
        if not (
            len(replicates)
            == len(progress)
            == len(ordinals)
            == len(probabilities)
            == decision_count
        ):
            raise CombatExperienceError("combat decision batch rows are misaligned")
        if tuple(row.replicate_index for row in progress) != replicates:
            raise CombatExperienceError(
                "combat decision progress disagrees with replicate rows"
            )
        if len(set(replicates)) != len(replicates):
            raise CombatExperienceError("combat decision batch repeats a replicate")
        object.__setattr__(self, "sequence_index", sequence_index)
        object.__setattr__(self, "replicate_indices", replicates)
        object.__setattr__(self, "decision_progress", progress)
        object.__setattr__(self, "selected_ordinals", ordinals)
        object.__setattr__(self, "selection_probabilities", probabilities)
        object.__setattr__(self, "decision_count", decision_count)
        object.__setattr__(self, "payload_bytes", payload_bytes)


@dataclass(frozen=True)
class CombatDecisionAdvantageBatch:
    """Independent advantage columns aligned to one retained model call."""

    sequence_index: int
    replicate_indices: tuple[int, ...]
    win: tuple[float, ...]
    terminal_hp: tuple[float, ...]
    enemy_hp_progress: tuple[float, ...]
    potion_retention: tuple[float, ...]

    def __post_init__(self) -> None:
        sequence_index = _nonnegative_integer(self.sequence_index, "sequence_index")
        replicates = tuple(
            _nonnegative_integer(value, "replicate_index")
            for value in self.replicate_indices
        )
        axes = (
            tuple(self.win),
            tuple(self.terminal_hp),
            tuple(self.enemy_hp_progress),
            tuple(self.potion_retention),
        )
        if not replicates or any(len(axis) != len(replicates) for axis in axes):
            raise CombatExperienceError("combat decision advantage rows are misaligned")
        if not all(math.isfinite(value) for axis in axes for value in axis):
            raise CombatExperienceError("combat decision advantages must be finite")
        object.__setattr__(self, "sequence_index", sequence_index)
        object.__setattr__(self, "replicate_indices", replicates)
        object.__setattr__(self, "win", axes[0])
        object.__setattr__(self, "terminal_hp", axes[1])
        object.__setattr__(self, "enemy_hp_progress", axes[2])
        object.__setattr__(self, "potion_retention", axes[3])


@dataclass(frozen=True)
class CompletedCombatGroupExperience:
    """Bounded chosen rows plus exact outcomes for every same-root replicate."""

    root_id: str
    exact_combat_state_hash: str
    behavior_manifest_id: BehaviorManifestId
    batches: tuple[CombatDecisionExperienceBatch, ...]
    outcomes: CompletedCombatGroup
    decision_count: int
    payload_bytes: int

    def __post_init__(self) -> None:
        _validate_root(self.root_id, self.exact_combat_state_hash)
        if not isinstance(self.behavior_manifest_id, BehaviorManifestId):
            raise CombatExperienceError(
                "completed combat experience requires a BehaviorManifestId"
            )
        batches = tuple(self.batches)
        if not batches or not all(
            isinstance(batch, CombatDecisionExperienceBatch) for batch in batches
        ):
            raise CombatExperienceError(
                "completed combat experience requires typed decision batches"
            )
        if tuple(batch.sequence_index for batch in batches) != tuple(
            range(len(batches))
        ):
            raise CombatExperienceError("combat decision sequence is not contiguous")
        if any(
            batch.root_id != self.root_id
            or batch.exact_combat_state_hash != self.exact_combat_state_hash
            or batch.behavior_manifest_id != self.behavior_manifest_id
            for batch in batches
        ):
            raise CombatExperienceError("combat decision batches disagree on lineage")
        if not isinstance(self.outcomes, CompletedCombatGroup):
            raise CombatExperienceError(
                "completed combat experience requires typed outcomes"
            )
        if (
            self.outcomes.root_id != self.root_id
            or self.outcomes.exact_combat_state_hash
            != self.exact_combat_state_hash
        ):
            raise CombatExperienceError("combat outcomes disagree with experience root")
        replicate_count = len(self.outcomes.outcomes)
        if any(
            replicate >= replicate_count
            for batch in batches
            for replicate in batch.replicate_indices
        ):
            raise CombatExperienceError(
                "combat decision replicate is outside completed outcomes"
            )
        decision_count = _positive_integer(self.decision_count, "decision_count")
        payload_bytes = _positive_integer(self.payload_bytes, "payload_bytes")
        if decision_count != sum(batch.decision_count for batch in batches):
            raise CombatExperienceError("combat decision count is misaligned")
        if payload_bytes != sum(batch.payload_bytes for batch in batches):
            raise CombatExperienceError("combat payload byte count is misaligned")
        object.__setattr__(self, "batches", batches)
        object.__setattr__(self, "decision_count", decision_count)
        object.__setattr__(self, "payload_bytes", payload_bytes)

    def grouped_advantages(self) -> CombatGroupedAdvantages:
        return self.outcomes.grouped_advantages()

    def chronological_steps(
        self,
        replicate_index: int,
    ) -> tuple[CombatChronologicalDecisionStep, ...]:
        """Return one replicate's exact retained policy order."""

        replicate_index = _nonnegative_integer(
            replicate_index,
            "replicate_index",
        )
        if replicate_index >= len(self.outcomes.outcomes):
            raise CombatExperienceError(
                "chronological combat replicate is outside completed outcomes"
            )
        steps: list[CombatChronologicalDecisionStep] = []
        previous_turn = -1
        for batch in self.batches:
            try:
                row_index = batch.replicate_indices.index(replicate_index)
            except ValueError:
                continue
            progress = batch.decision_progress[row_index]
            if progress.turn < previous_turn:
                raise CombatExperienceError(
                    "combat decision turns are not chronological"
                )
            previous_turn = progress.turn
            steps.append(
                CombatChronologicalDecisionStep(
                    sequence_index=batch.sequence_index,
                    progress=progress,
                    selected_ordinal=batch.selected_ordinals[row_index],
                    selection_probability=batch.selection_probabilities[row_index],
                )
            )
        if not steps:
            raise CombatExperienceError("combat replicate has no retained decisions")
        return tuple(steps)

    def decision_advantages(self) -> tuple[CombatDecisionAdvantageBatch, ...]:
        """Project replicate outcomes onto rows without combining reward axes."""

        grouped = self.grouped_advantages()
        return tuple(
            CombatDecisionAdvantageBatch(
                sequence_index=batch.sequence_index,
                replicate_indices=batch.replicate_indices,
                win=tuple(grouped.win[index] for index in batch.replicate_indices),
                terminal_hp=tuple(
                    grouped.terminal_hp[index] for index in batch.replicate_indices
                ),
                enemy_hp_progress=tuple(
                    grouped.enemy_hp_progress[index]
                    for index in batch.replicate_indices
                ),
                potion_retention=tuple(
                    grouped.potion_retention[index]
                    for index in batch.replicate_indices
                ),
            )
            for batch in self.batches
        )

    def signal_summary(self) -> CombatGroupSignalSummary:
        """Count useful rows per axis without retaining a second trajectory."""

        grouped = self.grouped_advantages()
        projected = self.decision_advantages()
        return summarize_combat_group_signals(
            root_id=self.root_id,
            exact_combat_state_hash=self.exact_combat_state_hash,
            grouped=grouped,
            decision_count=self.decision_count,
            win_decisions=tuple(
                value for batch in projected for value in batch.win
            ),
            terminal_hp_decisions=tuple(
                value for batch in projected for value in batch.terminal_hp
            ),
            enemy_hp_progress_decisions=tuple(
                value for batch in projected for value in batch.enemy_hp_progress
            ),
            potion_retention_decisions=tuple(
                value for batch in projected for value in batch.potion_retention
            ),
        )


class BoundedCombatGroupExperience:
    """Preflight and retain one group without rotation or partial delivery."""

    def __init__(
        self,
        *,
        root_id: str,
        exact_combat_state_hash: str,
        replicate_count: int,
        limits: CombatExperienceLimits,
    ) -> None:
        try:
            validate_combat_digest(root_id, "root_id")
            validate_combat_digest(
                exact_combat_state_hash,
                "exact_combat_state_hash",
            )
        except CombatOutcomeError as error:
            raise CombatExperienceError(str(error)) from error
        if not isinstance(limits, CombatExperienceLimits):
            raise CombatExperienceError("limits must be CombatExperienceLimits")
        self.root_id = root_id
        self.exact_combat_state_hash = exact_combat_state_hash
        self.replicate_count = _positive_integer(
            replicate_count,
            "replicate_count",
        )
        if self.replicate_count < 2:
            raise CombatExperienceError("combat group requires at least two replicates")
        self.limits = limits
        self._batches: list[CombatDecisionExperienceBatch] = []
        self._decision_count = 0
        self._payload_bytes = 0
        self._behavior_manifest_id: BehaviorManifestId | None = None

    @property
    def decision_count(self) -> int:
        return self._decision_count

    @property
    def payload_bytes(self) -> int:
        return self._payload_bytes

    def prepare(self, decision_batch: Mapping[str, object]) -> PreparedDecisionRows:
        try:
            prepared = PreparedDecisionRows.capture(decision_batch)
        except DecisionRowError as error:
            raise CombatExperienceError(str(error)) from error
        if any(
            replicate < 0 or replicate >= self.replicate_count
            for replicate in prepared.slot_indices
        ):
            raise CombatExperienceError(
                "decision row replicate is outside the combat group"
            )
        if self._decision_count + prepared.decision_count > self.limits.max_decisions:
            raise CombatExperienceError("combat group exceeds the decision limit")
        if self._payload_bytes + prepared.payload_bytes > self.limits.max_payload_bytes:
            raise CombatExperienceError("combat group exceeds the payload byte limit")
        return prepared

    def bind_choice(
        self,
        prepared: PreparedDecisionRows,
        choice: BatchPolicyChoice,
        decision_progress: Sequence[object],
    ) -> CombatDecisionExperienceBatch:
        if not isinstance(choice, BatchPolicyChoice):
            raise CombatExperienceError("policy must return BatchPolicyChoice")
        try:
            ordinals, probabilities = normalize_decision_choice(
                prepared,
                choice.ordinals,
                choice.selection_probabilities,
                choice.behavior_manifest_id,
            )
        except DecisionRowError as error:
            raise CombatExperienceError(str(error)) from error
        if (
            self._behavior_manifest_id is not None
            and choice.behavior_manifest_id != self._behavior_manifest_id
        ):
            raise CombatExperienceError(
                "combat group cannot mix behavior manifest identities"
            )
        progress = _capture_decision_progress(decision_progress)
        if len(progress) != prepared.decision_count:
            raise CombatExperienceError(
                "combat decision progress count disagrees with decision rows"
            )
        if tuple(row.replicate_index for row in progress) != prepared.slot_indices:
            raise CombatExperienceError(
                "combat decision progress order disagrees with decision rows"
            )
        return CombatDecisionExperienceBatch(
            sequence_index=len(self._batches),
            root_id=self.root_id,
            exact_combat_state_hash=self.exact_combat_state_hash,
            replicate_indices=prepared.slot_indices,
            decision_progress=progress,
            payload=prepared.payload,
            selected_ordinals=ordinals,
            selection_probabilities=probabilities,
            behavior_manifest_id=choice.behavior_manifest_id,
            decision_count=prepared.decision_count,
            payload_bytes=prepared.payload_bytes,
        )

    def commit(self, batch: CombatDecisionExperienceBatch) -> None:
        if not isinstance(batch, CombatDecisionExperienceBatch):
            raise CombatExperienceError(
                "combat experience commit requires a typed decision batch"
            )
        if batch.sequence_index != len(self._batches):
            raise CombatExperienceError("combat decision batch is stale or repeated")
        if (
            batch.root_id != self.root_id
            or batch.exact_combat_state_hash != self.exact_combat_state_hash
        ):
            raise CombatExperienceError("combat decision batch belongs to another root")
        if (
            self._behavior_manifest_id is not None
            and batch.behavior_manifest_id != self._behavior_manifest_id
        ):
            raise CombatExperienceError(
                "combat group cannot mix behavior manifest identities"
            )
        if self._decision_count + batch.decision_count > self.limits.max_decisions:
            raise CombatExperienceError("combat group exceeds the decision limit")
        if self._payload_bytes + batch.payload_bytes > self.limits.max_payload_bytes:
            raise CombatExperienceError("combat group exceeds the payload byte limit")
        self._batches.append(batch)
        self._decision_count += batch.decision_count
        self._payload_bytes += batch.payload_bytes
        if self._behavior_manifest_id is None:
            self._behavior_manifest_id = batch.behavior_manifest_id

    def finish(self, outcomes: CompletedCombatGroup) -> CompletedCombatGroupExperience:
        if not isinstance(outcomes, CompletedCombatGroup):
            raise CombatExperienceError("finish requires CompletedCombatGroup outcomes")
        if (
            outcomes.root_id != self.root_id
            or outcomes.exact_combat_state_hash != self.exact_combat_state_hash
        ):
            raise CombatExperienceError("combat outcomes belong to another root")
        if len(outcomes.outcomes) != self.replicate_count:
            raise CombatExperienceError("combat outcome replicate count is misaligned")
        if not self._batches or self._behavior_manifest_id is None:
            raise CombatExperienceError("combat group has no retained policy decisions")
        return CompletedCombatGroupExperience(
            root_id=self.root_id,
            exact_combat_state_hash=self.exact_combat_state_hash,
            behavior_manifest_id=self._behavior_manifest_id,
            batches=tuple(self._batches),
            outcomes=outcomes,
            decision_count=self._decision_count,
            payload_bytes=self._payload_bytes,
        )


def _positive_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatExperienceError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatExperienceError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise CombatExperienceError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatExperienceError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatExperienceError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CombatExperienceError(f"{name} must be non-negative")
    return normalized


def _validate_root(root_id: object, exact_combat_state_hash: object) -> None:
    try:
        validate_combat_digest(root_id, "root_id")
        validate_combat_digest(
            exact_combat_state_hash,
            "exact_combat_state_hash",
        )
    except CombatOutcomeError as error:
        raise CombatExperienceError(str(error)) from error


def _capture_decision_progress(
    rows: Sequence[object],
) -> tuple[CombatDecisionProgress, ...]:
    if isinstance(rows, (str, bytes)):
        raise CombatExperienceError("combat decision progress must be a sequence")
    try:
        normalized = tuple(rows)
    except TypeError as error:
        raise CombatExperienceError(
            "combat decision progress must be a sequence"
        ) from error
    captured = []
    for row in normalized:
        if isinstance(row, CombatDecisionProgress):
            captured.append(row)
            continue
        try:
            captured.append(
                CombatDecisionProgress(
                    replicate_index=getattr(row, "replicate_index"),
                    turn=getattr(row, "turn"),
                    player_hp=getattr(row, "player_hp"),
                    player_max_hp=getattr(row, "player_max_hp"),
                    enemy_hp=getattr(row, "enemy_hp"),
                    enemy_max_hp=getattr(row, "enemy_max_hp"),
                    potion_uuids=tuple(getattr(row, "potion_uuids")),
                    potion_ids=tuple(getattr(row, "potion_ids")),
                )
            )
        except (AttributeError, TypeError) as error:
            raise CombatExperienceError(
                "combat decision progress row is malformed"
            ) from error
    return tuple(captured)
