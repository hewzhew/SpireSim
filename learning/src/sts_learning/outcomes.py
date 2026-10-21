"""Compact typed terminal batches extracted from one bridge step."""

from __future__ import annotations

import operator
from collections.abc import Iterable, Mapping
from dataclasses import dataclass


class TerminalBatchError(ValueError):
    """A bridge terminal batch is missing, misaligned, or invalid."""


@dataclass(frozen=True)
class TerminalAttemptOutcome:
    """Public terminal facts for one episode attempt in one environment slot."""

    slot_index: int
    terminal_reward: int
    terminal_act: int
    terminal_floor: int
    terminal_hp: int
    terminal_max_hp: int
    terminal_gold: int

    def __post_init__(self) -> None:
        for field_name in (
            "slot_index",
            "terminal_reward",
            "terminal_act",
            "terminal_floor",
            "terminal_hp",
            "terminal_max_hp",
            "terminal_gold",
        ):
            raw = getattr(self, field_name)
            if isinstance(raw, bool):
                raise TerminalBatchError(f"{field_name} must be an integer, not bool")
            try:
                value = operator.index(raw)
            except TypeError as error:
                raise TerminalBatchError(f"{field_name} must be an integer") from error
            object.__setattr__(self, field_name, value)
        if self.slot_index < 0:
            raise TerminalBatchError("slot_index must be non-negative")
        if self.terminal_reward not in (-1, 1):
            raise TerminalBatchError("terminal_reward must be -1 or 1")
        if not 1 <= self.terminal_act <= 255:
            raise TerminalBatchError("terminal_act must be in 1..255")
        if self.terminal_floor < 0:
            raise TerminalBatchError("terminal_floor must be non-negative")
        if self.terminal_max_hp <= 0:
            raise TerminalBatchError("terminal_max_hp must be positive")
        if not 0 <= self.terminal_hp <= self.terminal_max_hp:
            raise TerminalBatchError("terminal_hp must be in 0..terminal_max_hp")
        if self.terminal_gold < 0:
            raise TerminalBatchError("terminal_gold must be non-negative")


@dataclass(frozen=True)
class TerminalStepBatch:
    """Terminal rows from one vector step, with no retained tensor payload."""

    attempts: tuple[TerminalAttemptOutcome, ...]

    def __post_init__(self) -> None:
        attempts = tuple(self.attempts)
        if not all(isinstance(attempt, TerminalAttemptOutcome) for attempt in attempts):
            raise TerminalBatchError(
                "attempts must contain only TerminalAttemptOutcome values"
            )
        slots = tuple(attempt.slot_index for attempt in attempts)
        if len(set(slots)) != len(slots):
            raise TerminalBatchError("terminal batch contains duplicate slots")
        object.__setattr__(self, "attempts", attempts)

    @classmethod
    def from_bridge_step(
        cls,
        step: Mapping[str, object],
        *,
        slot_count: int,
    ) -> TerminalStepBatch:
        """Copy only compact terminal integer columns from a bridge result."""

        normalized_slot_count = operator.index(slot_count)
        if normalized_slot_count < 0:
            raise TerminalBatchError("slot_count must be non-negative")
        columns = {
            name: _integer_column(step, name)
            for name in (
                "terminal_slot_indices",
                "terminal_reward",
                "terminal_act",
                "terminal_floor",
                "terminal_hp",
                "terminal_max_hp",
                "terminal_gold",
            )
        }
        row_count = len(columns["terminal_slot_indices"])
        for name, values in columns.items():
            if len(values) != row_count:
                raise TerminalBatchError(
                    f"terminal column {name} has {len(values)} rows, expected {row_count}"
                )
        if row_count > normalized_slot_count:
            raise TerminalBatchError(
                f"terminal batch has {row_count} rows for {normalized_slot_count} slots"
            )

        attempts = tuple(
            TerminalAttemptOutcome(
                slot_index=columns["terminal_slot_indices"][row],
                terminal_reward=columns["terminal_reward"][row],
                terminal_act=columns["terminal_act"][row],
                terminal_floor=columns["terminal_floor"][row],
                terminal_hp=columns["terminal_hp"][row],
                terminal_max_hp=columns["terminal_max_hp"][row],
                terminal_gold=columns["terminal_gold"][row],
            )
            for row in range(row_count)
        )
        for attempt in attempts:
            if attempt.slot_index >= normalized_slot_count:
                raise TerminalBatchError(
                    f"slot {attempt.slot_index} is outside 0..{normalized_slot_count}"
                )
        return cls(attempts)

    @property
    def slot_indices(self) -> tuple[int, ...]:
        return tuple(attempt.slot_index for attempt in self.attempts)


def _integer_column(step: Mapping[str, object], name: str) -> tuple[int, ...]:
    try:
        raw = step[name]
    except KeyError as error:
        raise TerminalBatchError(f"bridge step is missing {name}") from error
    if isinstance(raw, (str, bytes)) or not isinstance(raw, Iterable):
        raise TerminalBatchError(f"terminal column {name} is not iterable")
    try:
        return tuple(operator.index(value) for value in raw)
    except TypeError as error:
        raise TerminalBatchError(f"terminal column {name} is not integral") from error
