from __future__ import annotations

import math
from dataclasses import replace

import pytest

from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from learning.tests.torch_outcome_fixtures import (
    decision_batch_fixture,
    public_attempt_trajectory_fixture,
    with_run_progress_fixture,
)
from sts_learning import (
    CompletedAttemptExperience,
    FloorProgressReturnConfig,
    PublicAttemptTrajectoryV1,
    RunRolloutConfig,
    RunRolloutError,
    TerminalAttemptOutcome,
    TerminalAttemptRecord,
    build_complete_run_rollout,
    compute_complete_run_gae,
    floor_progress_terminal_return,
)
from sts_learning.run_rollout import decision_progress_potential


def _attempt(
    *,
    slot: int,
    decision_floors: tuple[int, ...],
    candidate_counts: tuple[int, ...] | None = None,
    terminal_floor: int,
    reward: int,
    decision_acts: tuple[int, ...] | None = None,
    terminal_act: int = 3,
) -> PublicAttemptTrajectoryV1:
    if decision_acts is None:
        decision_acts = (1,) * len(decision_floors)
    if candidate_counts is None:
        candidate_counts = (2,) * len(decision_floors)
    assert len(candidate_counts) == len(decision_floors)
    batches = []
    for decision_index, (floor, candidate_count) in enumerate(
        zip(decision_floors, candidate_counts, strict=True)
    ):
        batch = decision_batch_fixture(
            slot=slot,
            semantic_row=1 if candidate_count == 3 else 0,
            selected_ordinal=0,
            manifest_id=BEHAVIOR_MANIFEST_ID,
        )
        if candidate_count == 1:
            payload = dict(batch.payload)
            counts = payload["candidate_counts"].copy()
            counts[0] = 1
            splits = payload["candidate_row_splits"].copy()
            splits[1] = 1
            semantic = dict(payload["semantic"])
            semantic["candidate_token_indices"] = semantic[
                "candidate_token_indices"
            ][:1].copy()
            tokens = dict(semantic["token"])
            scopes = tokens["scope"].copy()
            scopes[scopes > 1] = 0
            tokens["scope"] = scopes
            semantic["token"] = tokens
            payload["candidate_counts"] = counts
            payload["candidate_row_splits"] = splits
            payload["semantic"] = semantic
            batch = replace(batch, payload=payload)
        batch = with_run_progress_fixture(
            batch,
            act=decision_acts[decision_index],
            floor=floor,
            is_combat=True,
            strategic_context_kind=None,
            identity_suffix=f"decision-{decision_index}",
        )
        batches.append(batch)
    lineage = batches[0].lineages[0]
    terminal = TerminalAttemptRecord(
        episode_seed=lineage.key.episode_seed,
        episode_generation=lineage.key.episode_generation,
        attempt_index=lineage.key.attempt_index,
        recoveries_used=lineage.recoveries_used,
        terminal=TerminalAttemptOutcome(
            slot_index=slot,
            terminal_reward=reward,
            terminal_act=terminal_act,
            terminal_floor=terminal_floor,
            terminal_hp=20 if reward == 1 else 0,
            terminal_max_hp=80,
            terminal_gold=50,
        ),
    )
    completed = CompletedAttemptExperience(
        lineage=lineage,
        batches=tuple(batches),
        terminal=terminal,
        decision_count=len(batches),
        payload_bytes=len(batches),
    )
    return public_attempt_trajectory_fixture(completed)


@pytest.mark.parametrize("metric", ("floor_progress", "act_completion"))
@pytest.mark.parametrize("reward", (-1, 1))
def test_public_potential_translates_every_future_return_to_final_score(metric, reward):
    attempt = _attempt(slot=1, decision_floors=(8, 16, 34, 60),
                       decision_acts=(1, 2, 3, 4), terminal_floor=61,
                       terminal_act=4, reward=reward)
    config = FloorProgressReturnConfig(target_floor=54, metric=metric)
    rollout = build_complete_run_rollout((attempt,), config).attempts[0]
    for row in rollout.rows:
        assert row.return_to_go + decision_progress_potential(row.progress, config) == pytest.approx(
            rollout.terminal_return
        )


@pytest.mark.parametrize(
    "acts,floors,final_act,final_floor,win,rewards,prefix,returns",
    [
        ((1, 1, 1), (0, 8, 15), 1, 16, -1, (0, 0, 0), 0, (0, 0, 0)),
        ((1, 1, 2), (0, 16, 20), 3, 40, -1, (0, .25, .25), 0, (.5, .5, .25)),
        ((3, 4), (40, 53), 4, 55, 1, (.25, .25), .5, (.5, .25)),
    ],
)
def test_act_completion_credits_actual_crossings_without_repeating_prefix(
    acts, floors, final_act, final_floor, win, rewards, prefix, returns,
) -> None:
    attempt = _attempt(slot=1, decision_floors=floors, decision_acts=acts,
                       terminal_act=final_act, terminal_floor=final_floor, reward=win)
    config = FloorProgressReturnConfig(metric="act_completion")
    rollout = build_complete_run_rollout((attempt,), config).attempts[0]
    assert tuple(row.reward for row in rollout.rows) == pytest.approx(rewards)
    assert tuple(row.return_to_go for row in rollout.rows) == pytest.approx(returns)
    assert rollout.prefix_reward == prefix
    assert rollout.total_reward == floor_progress_terminal_return(attempt.terminal, config)


def test_act_completion_rejects_regressing_act_even_if_floor_increases() -> None:
    attempt = _attempt(slot=1, decision_floors=(30, 40), decision_acts=(3, 2),
                       terminal_act=3, terminal_floor=41, reward=-1)
    with pytest.raises(RunRolloutError, match="acts are not chronological"):
        build_complete_run_rollout((attempt,), FloorProgressReturnConfig(metric="act_completion"))


def test_defeat_rewards_credit_only_the_transition_that_reaches_a_floor() -> None:
    config = FloorProgressReturnConfig(target_floor=52)
    attempt = _attempt(
        slot=1,
        decision_floors=(0, 10, 10),
        terminal_floor=20,
        reward=-1,
    )

    rollout = build_complete_run_rollout((attempt,), config).attempts[0]

    assert tuple(row.reward for row in rollout.rows) == pytest.approx(
        (20 / 52, 0.0, -32 / 52)
    )
    assert tuple(row.return_to_go for row in rollout.rows) == pytest.approx(
        (-12 / 52, -32 / 52, -32 / 52)
    )
    assert rollout.prefix_reward == 0.0
    assert rollout.total_reward == pytest.approx(
        floor_progress_terminal_return(attempt.terminal, config)
    )


def test_unobserved_prefix_is_conserved_but_never_credited_to_an_action() -> None:
    config = FloorProgressReturnConfig(target_floor=52)
    attempt = _attempt(
        slot=1,
        decision_floors=(5, 51),
        terminal_floor=55,
        reward=1,
    )

    rollout = build_complete_run_rollout((attempt,), config).attempts[0]

    assert rollout.prefix_reward == pytest.approx(10 / 52)
    assert tuple(row.reward for row in rollout.rows) == pytest.approx(
        (92 / 52, -50 / 52)
    )
    assert tuple(row.return_to_go for row in rollout.rows) == pytest.approx(
        (42 / 52, -50 / 52)
    )
    assert rollout.total_reward == pytest.approx(1.0)


def test_actor_sums_per_attempt_while_value_averages_and_forced_rows_have_no_actor_weight() -> None:
    short = _attempt(
        slot=1,
        decision_floors=(0,),
        candidate_counts=(2,),
        terminal_floor=0,
        reward=-1,
    )
    long = _attempt(
        slot=2,
        decision_floors=(0, 0, 0),
        candidate_counts=(1, 3, 2),
        terminal_floor=0,
        reward=-1,
    )

    rollout = build_complete_run_rollout(
        (short, long),
        FloorProgressReturnConfig(),
    )

    assert rollout.decision_count == 4
    assert rollout.actor_decision_count == 3
    assert math.fsum(row.value_weight for row in rollout.attempts[0].rows) == 0.5
    assert math.fsum(row.value_weight for row in rollout.attempts[1].rows) == 0.5
    assert math.fsum(row.actor_weight for row in rollout.attempts[0].rows) == 0.5
    assert math.fsum(row.actor_weight for row in rollout.attempts[1].rows) == 1.0
    assert rollout.attempts[1].rows[0].actor_eligible is False
    assert rollout.attempts[1].rows[0].actor_weight == 0.0


def test_monte_carlo_gae_equals_decision_local_return_to_go() -> None:
    attempt = _attempt(
        slot=1,
        decision_floors=(0, 10, 10),
        terminal_floor=20,
        reward=-1,
    )
    rollout = build_complete_run_rollout(
        (attempt,),
        FloorProgressReturnConfig(target_floor=52),
    )

    evaluated = compute_complete_run_gae(
        rollout,
        ((0.1, 0.2, 0.3),),
    )

    expected_returns = tuple(row.return_to_go for row in rollout.attempts[0].rows)
    assert evaluated.returns[0] == pytest.approx(expected_returns)
    assert evaluated.advantages[0] == pytest.approx(
        tuple(
            target - value
            for target, value in zip(
                expected_returns,
                evaluated.value_predictions[0],
                strict=True,
            )
        )
    )


def test_floor_cap_preserves_the_historical_reserved_victory_return() -> None:
    config = FloorProgressReturnConfig(target_floor=52)
    defeat = _attempt(
        slot=1,
        decision_floors=(0, 51),
        terminal_floor=999,
        reward=-1,
    )
    victory = _attempt(
        slot=2,
        decision_floors=(0, 51),
        terminal_floor=999,
        reward=1,
    )

    rollouts = build_complete_run_rollout((defeat, victory), config).attempts

    assert rollouts[0].total_reward == pytest.approx(1.0 - 2.0 / 52)
    assert rollouts[1].total_reward == pytest.approx(1.0)


def test_malformed_order_and_invented_action_discount_fail_closed() -> None:
    decreasing = _attempt(
        slot=1,
        decision_floors=(0, 10, 9),
        terminal_floor=20,
        reward=-1,
    )
    terminal_before_decision = _attempt(
        slot=2,
        decision_floors=(0, 20),
        terminal_floor=10,
        reward=-1,
    )

    with pytest.raises(RunRolloutError, match="chronological"):
        build_complete_run_rollout((decreasing,), FloorProgressReturnConfig())
    with pytest.raises(RunRolloutError, match="precedes"):
        build_complete_run_rollout(
            (terminal_before_decision,),
            FloorProgressReturnConfig(),
        )
    with pytest.raises(RunRolloutError, match="gamma=1"):
        RunRolloutConfig(gamma=0.99)
