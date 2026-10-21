from __future__ import annotations

from dataclasses import replace

import pytest

torch = pytest.importorskip("torch")

from learning.tests.torch_outcome_fixtures import (  # noqa: E402
    behavior_manifest_fixture,
    completed_attempt_fixture,
    decision_batch_fixture,
    public_attempt_trajectory_fixture,
    with_run_progress_fixture,
)
from sts_learning import (  # noqa: E402
    BehaviorManifestId,
    BehaviorManifestRegistry,
    FloorProgressReturnConfig,
    RunDecisionScope,
    SelectionProbability,
    SemanticBatchConcatLimits,
    TerminalAdvantageMode,
)
from sts_learning.torch_outcomes import on_policy_terminal_loss  # noqa: E402
from sts_learning.torch_policy import (  # noqa: E402
    RaggedCandidateLogits,
    RaggedCategoricalPolicyConfig,
)


def _attempt(
    manifest_id: BehaviorManifestId,
    *,
    slot: int,
    terminal_floor: int,
    selected_ordinal: int,
    context: int,
    episode_seed: int | None = None,
):
    seed = 100 + slot if episode_seed is None else episode_seed
    batch = decision_batch_fixture(
        slot=slot,
        semantic_row=0,
        selected_ordinal=selected_ordinal,
        manifest_id=manifest_id,
        selection_probability=SelectionProbability.known(0.5),
    )
    lineage = replace(
        batch.lineages[0],
        key=replace(batch.lineages[0].key, episode_seed=seed),
    )
    batch = replace(batch, lineages=(lineage,))
    batch = with_run_progress_fixture(
        batch,
        act=1,
        floor=0,
        is_combat=False,
        strategic_context_kind=context,
        identity_suffix=f"context-{context}",
    )
    attempt = completed_attempt_fixture(slot=slot, batches=(batch,), reward=-1)
    completed = replace(
        attempt,
        terminal=replace(
            attempt.terminal,
            terminal=replace(
                attempt.terminal.terminal,
                terminal_floor=terminal_floor,
            ),
        ),
    )
    return public_attempt_trajectory_fixture(completed)


def test_floor_context_objective_does_not_borrow_from_an_unlike_site() -> None:
    policy = RaggedCategoricalPolicyConfig(temperature=1.0)
    registry = BehaviorManifestRegistry(capacity=1)
    manifest_id = registry.register(
        behavior_manifest_fixture(behavior_rule=policy.behavior_rule)
    )
    values = torch.nn.Parameter(torch.zeros(6))

    def scorer(payload):
        return RaggedCandidateLogits(
            values=values,
            row_splits=torch.as_tensor(
                payload["candidate_row_splits"],
                dtype=torch.long,
            ),
        )

    # Three attempts are the minimum that proves one matched pair learns while
    # an unlike context at the same floor remains unsupported.
    attempts = tuple(
        _attempt(
            manifest_id,
            slot=slot,
            terminal_floor=floor,
            selected_ordinal=ordinal,
            context=context,
        )
        for slot, floor, ordinal, context in (
            (1, 10, 0, 1),
            (2, 20, 1, 1),
            (3, 30, 0, 2),
        )
    )
    result = on_policy_terminal_loss(
        scorer,
        attempts,
        registry,
        SemanticBatchConcatLimits(
            max_rows=16,
            max_input_array_bytes=1024 * 1024,
        ),
        policy,
        FloorProgressReturnConfig(target_floor=100),
        TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT,
        RunDecisionScope.STRATEGIC,
    )
    result.value.backward()

    assert float(result.value.detach()) == pytest.approx(0.0)
    assert values.grad is not None
    assert values.grad[0].item() > 0.0
    assert values.grad[3].item() < 0.0
    torch.testing.assert_close(values.grad[4:], torch.zeros(2))


def test_episode_context_objective_does_not_borrow_from_another_root() -> None:
    policy = RaggedCategoricalPolicyConfig(temperature=1.0)
    registry = BehaviorManifestRegistry(capacity=1)
    manifest_id = registry.register(
        behavior_manifest_fixture(behavior_rule=policy.behavior_rule)
    )
    values = torch.nn.Parameter(torch.zeros(6))

    def scorer(payload):
        return RaggedCandidateLogits(
            values=values,
            row_splits=torch.as_tensor(
                payload["candidate_row_splits"],
                dtype=torch.long,
            ),
        )

    attempts = tuple(
        _attempt(
            manifest_id,
            slot=slot,
            terminal_floor=floor,
            selected_ordinal=ordinal,
            context=1,
            episode_seed=seed,
        )
        for slot, floor, ordinal, seed in (
            (1, 10, 0, 10),
            (2, 20, 1, 10),
            (3, 30, 0, 20),
        )
    )
    result = on_policy_terminal_loss(
        scorer,
        attempts,
        registry,
        SemanticBatchConcatLimits(
            max_rows=16,
            max_input_array_bytes=1024 * 1024,
        ),
        policy,
        FloorProgressReturnConfig(target_floor=100),
        TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT,
        RunDecisionScope.STRATEGIC,
    )
    result.value.backward()

    assert float(result.value.detach()) == pytest.approx(0.0)
    assert values.grad is not None
    assert values.grad[0].item() > 0.0
    assert values.grad[3].item() < 0.0
    torch.testing.assert_close(values.grad[4:], torch.zeros(2))
