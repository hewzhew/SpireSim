"""Typed behavior-policy identity and one batched policy choice."""

from __future__ import annotations

import math
import operator
from collections.abc import Sequence
from dataclasses import dataclass
from numbers import Real


class PolicyChoiceError(ValueError):
    """A behavior manifest id or batched choice is malformed."""


@dataclass(frozen=True, order=True)
class BehaviorManifestId:
    """Caller-owned SHA-256 identity of one exact behavior-policy manifest."""

    digest: bytes

    def __post_init__(self) -> None:
        if not isinstance(self.digest, bytes):
            raise PolicyChoiceError("behavior manifest digest must be immutable bytes")
        if len(self.digest) != 32:
            raise PolicyChoiceError("behavior manifest digest must contain 32 bytes")


@dataclass(frozen=True)
class SelectionProbability:
    """Behavior propensity recorded at selection time, or explicit unknown."""

    value: float | None

    def __post_init__(self) -> None:
        if self.value is None:
            return
        if isinstance(self.value, bool) or not isinstance(self.value, Real):
            raise PolicyChoiceError(
                "selection probability must be a real number, not bool or text"
            )
        normalized = float(self.value)
        if not math.isfinite(normalized) or not 0.0 < normalized <= 1.0:
            raise PolicyChoiceError("known selection probability must be in (0, 1]")
        object.__setattr__(self, "value", normalized)

    @classmethod
    def known(cls, value: float) -> SelectionProbability:
        return cls(value)

    @classmethod
    def unknown(cls) -> SelectionProbability:
        return cls(None)


DETERMINISTIC_SELECTION = SelectionProbability.known(1.0)


@dataclass(frozen=True)
class BatchPolicyChoice:
    """One model call's aligned ordinals and exact behavior provenance."""

    ordinals: tuple[int, ...]
    behavior_manifest_id: BehaviorManifestId
    selection_probabilities: tuple[SelectionProbability, ...]

    def __post_init__(self) -> None:
        if not isinstance(self.behavior_manifest_id, BehaviorManifestId):
            raise PolicyChoiceError(
                "batch policy choice requires a BehaviorManifestId"
            )
        try:
            raw_ordinals = tuple(self.ordinals)
        except TypeError as error:
            raise PolicyChoiceError("policy ordinals must be a sequence") from error
        normalized: list[int] = []
        for value in raw_ordinals:
            try:
                normalized.append(operator.index(value))
            except TypeError as error:
                raise PolicyChoiceError("policy ordinal must be an integer") from error
        object.__setattr__(self, "ordinals", tuple(normalized))
        try:
            probabilities = tuple(self.selection_probabilities)
        except TypeError as error:
            raise PolicyChoiceError(
                "selection probabilities must be a sequence"
            ) from error
        if len(probabilities) != len(normalized):
            raise PolicyChoiceError(
                "selection probabilities must contain one value per ordinal"
            )
        if not all(
            isinstance(probability, SelectionProbability)
            for probability in probabilities
        ):
            raise PolicyChoiceError(
                "selection probabilities must be typed SelectionProbability values"
            )
        object.__setattr__(self, "selection_probabilities", probabilities)

    @classmethod
    def create(
        cls,
        ordinals: Sequence[int],
        behavior_manifest_id: BehaviorManifestId,
        selection_probabilities: Sequence[SelectionProbability],
    ) -> BatchPolicyChoice:
        try:
            probabilities = tuple(selection_probabilities)
        except TypeError as error:
            raise PolicyChoiceError(
                "selection probabilities must be a sequence"
            ) from error
        return cls(
            tuple(ordinals),
            behavior_manifest_id,
            probabilities,
        )

    @classmethod
    def deterministic(
        cls,
        ordinals: Sequence[int],
        behavior_manifest_id: BehaviorManifestId,
    ) -> BatchPolicyChoice:
        normalized = tuple(ordinals)
        return cls(
            normalized,
            behavior_manifest_id,
            (DETERMINISTIC_SELECTION,) * len(normalized),
        )
