"""Typed whole-run sampling curricula above exact episode-root recovery."""

from __future__ import annotations

import operator
from enum import Enum

from .driver import RecoveryPlan
from .recovery import (
    RecoverySlotSnapshot,
    RecoverySlotStatus,
    TerminalAccountingBatch,
)


class RunSamplingError(ValueError):
    """A whole-run sampling curriculum lost its update boundary."""


class RunSamplingMode(Enum):
    """How complete attempts are sampled before one run-policy update."""

    INDEPENDENT_COHORTS = "independent-cohorts"
    EPISODE_ROOT_RETRIES = "episode-root-retries"


class EpisodeRootRetryCurriculum:
    """Sample bounded retries from exact roots within one fixed update.

    This first paired-root curriculum is deliberately single-slot. Victories
    and the configured per-episode attempt cap complete an episode; later
    attempts may come from the next scheduled root, so the objective must
    retain episode identity when matching baselines. The final defeat at an
    update boundary is also completed instead of restored, ensuring no live
    episode crosses behavior promotion.
    """

    def __init__(
        self,
        attempts_per_update: int,
        attempts_per_episode: int,
    ) -> None:
        if isinstance(attempts_per_update, bool) or isinstance(
            attempts_per_episode,
            bool,
        ):
            raise RunSamplingError(
                "retry attempt counts must be integers, not bool"
            )
        try:
            update_attempts = operator.index(attempts_per_update)
            episode_attempts = operator.index(attempts_per_episode)
        except TypeError as error:
            raise RunSamplingError(
                "retry attempt counts must be integers"
            ) from error
        if update_attempts < 2:
            raise RunSamplingError(
                "episode-root retries require at least two update attempts"
            )
        if episode_attempts < 2:
            raise RunSamplingError(
                "episode-root retries require at least two attempts per episode"
            )
        if episode_attempts > update_attempts:
            raise RunSamplingError(
                "episode-root attempts cannot exceed the update batch"
            )
        self.attempts_per_update = update_attempts
        self.attempts_per_episode = episode_attempts
        self._attempts_in_update = 0
        self._attempts_in_episode = 0

    @property
    def attempts_in_update(self) -> int:
        return self._attempts_in_update

    @property
    def attempts_in_episode(self) -> int:
        return self._attempts_in_episode

    def plan_recovery(
        self,
        accounting: TerminalAccountingBatch,
        snapshots: tuple[RecoverySlotSnapshot, ...],
    ) -> RecoveryPlan:
        if not isinstance(accounting, TerminalAccountingBatch):
            raise RunSamplingError("retry curriculum requires typed accounting")
        if len(accounting.attempts) != 1 or len(snapshots) != 1:
            raise RunSamplingError("episode-root retries require exactly one slot")
        attempt = accounting.attempts[0]
        snapshot = snapshots[0]
        if snapshot.slot_index != attempt.slot_index:
            raise RunSamplingError("retry snapshot and terminal slot disagree")
        if snapshot.episode_seed != attempt.episode_seed:
            raise RunSamplingError("retry snapshot and terminal seed disagree")
        if snapshot.episode_generation != attempt.episode_generation:
            raise RunSamplingError("retry snapshot and terminal generation disagree")
        if snapshot.attempt_index != attempt.attempt_index:
            raise RunSamplingError("retry snapshot and terminal attempt disagree")
        if snapshot.recoveries_used != attempt.recoveries_used:
            raise RunSamplingError("retry snapshot and terminal recovery count disagree")
        expected_status = (
            RecoverySlotStatus.VICTORY_COMPLETE
            if attempt.terminal_reward == 1
            else RecoverySlotStatus.DEFEAT_PENDING
        )
        if snapshot.status is not expected_status:
            raise RunSamplingError("retry snapshot and terminal status disagree")

        next_count = self._attempts_in_update + 1
        if next_count > self.attempts_per_update:
            raise RunSamplingError("retry curriculum crossed its update boundary")
        next_episode_count = self._attempts_in_episode + 1
        if next_episode_count > self.attempts_per_episode:
            raise RunSamplingError("retry curriculum crossed its episode boundary")
        at_update_boundary = next_count == self.attempts_per_update
        at_episode_boundary = next_episode_count == self.attempts_per_episode
        episode_complete = (
            attempt.terminal_reward == 1
            or at_episode_boundary
            or at_update_boundary
        )
        self._attempts_in_update = 0 if at_update_boundary else next_count
        self._attempts_in_episode = (
            0 if episode_complete else next_episode_count
        )

        if episode_complete:
            return RecoveryPlan()
        return RecoveryPlan((snapshot.slot_index,))
