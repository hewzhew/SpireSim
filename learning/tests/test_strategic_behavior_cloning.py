from __future__ import annotations

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from learning.tests.semantic_fixtures import (
    semantic_batch_fixture,
    semantic_schema_fixture,
)
from sts_learning.seeds import SeedPartitionSpec
from sts_learning.semantic_batch import select_semantic_decision_rows
from sts_learning.semantic_concat import SemanticBatchConcatLimits
from sts_learning.strategic_behavior_cloning import (
    StrategicBehaviorCloningConfig,
    StrategicDemonstrationPartition, StrategicDemonstrationSplit, StrategicBehaviorCloningError,
    split_strategic_demonstrations,
    train_strategic_behavior_clone,
)
from sts_learning.strategic_demonstrations import (
    CombatAnchorMode,
    StrategicDemonstrationBatch,
    StrategicDemonstrationCorpus,
)
from sts_learning.torch_policy import RaggedCandidateScorer


def _one_row_batch(seed: int) -> StrategicDemonstrationBatch:
    selected = select_semantic_decision_rows(semantic_batch_fixture(), (0,))
    return StrategicDemonstrationBatch(
        decision_batch=selected,
        target_ordinals=(1,),
        episode_seeds=(seed,),
        acts=(1,),
        floors=(3,),
        context_kinds=(2,),
        array_bytes=sum(
            value.nbytes
            for table in selected.values()
            if isinstance(table, np.ndarray)
            for value in (table,)
        ),
    )


def test_behavior_clone_splits_by_seed_and_does_not_mutate_combat_anchor() -> None:
    corpus = StrategicDemonstrationCorpus(
        batches=(_one_row_batch(40_000), _one_row_batch(40_004)),
        requested_runs=2,
        completed_runs=2,
        victories=0,
        defeats=2,
        batch_steps=1,
        decision_rounds=2,
        teacher_rows=2,
        combat_rows=0,
        strategic_selection_rows=0,
        unavailable_strategic_root_rows=0,
        array_bytes=2_048,
        elapsed_seconds=0.01,
        stop_reason="completed_runs",
        combat_anchor_mode=CombatAnchorMode.STRICT_PUBLICATION,
        combat_anchor_provenance_mismatches=(),
        terminal_episode_seeds=(40_000, 40_004),
        terminal_rewards=(-1, -1),
        terminal_acts=(1, 1),
        terminal_floors=(3, 3),
        terminal_hps=(0, 0),
        terminal_max_hps=(80, 80),
        combat_retries=0,
        rescued_combats=0,
        terminal_combat_retries=(0, 0),
    )
    split = split_strategic_demonstrations(
        corpus,
        SeedPartitionSpec(held_out_numerator=1, denominator=2),
        SemanticBatchConcatLimits(max_rows=2, max_input_array_bytes=1_048_576),
    )
    assert split.training.episode_seeds == (40_000,)
    assert split.held_out.episode_seeds == (40_004,)

    torch.manual_seed(7)
    anchor = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture())
    anchor.eval()
    anchor.requires_grad_(False)
    before = {
        name: value.detach().clone()
        for name, value in anchor.state_dict().items()
    }
    result = train_strategic_behavior_clone(
        anchor,
        split,
        StrategicBehaviorCloningConfig(
            epochs=16,
            learning_rate=1e-2,
            max_grad_norm=1.0,
        ),
    )

    assert result.final_training.cross_entropy < result.initial_training.cross_entropy
    assert result.final_held_out.cross_entropy < result.initial_held_out.cross_entropy
    assert not result.scorer.training
    assert not any(parameter.requires_grad for parameter in result.scorer.parameters())
    assert all(
        torch.equal(before[name], value)
        for name, value in anchor.state_dict().items()
    )


def test_minibatch_clone_is_reproducible_and_keeps_held_out_targets_out_of_fit():
    training = StrategicDemonstrationPartition(semantic_batch_fixture(), (1, 2), (40000, 40000), (1, 2))
    held_out = StrategicDemonstrationPartition(semantic_batch_fixture(), (0, 0), (40004, 40004), (1, 2))
    alternate = StrategicDemonstrationPartition(semantic_batch_fixture(), (1, 1), (40004, 40004), (1, 2))
    split = StrategicDemonstrationSplit(training, held_out, SeedPartitionSpec(1, 2))
    alternate_split = StrategicDemonstrationSplit(training, alternate, SeedPartitionSpec(1, 2))
    torch.manual_seed(8)
    initial = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture()).eval().requires_grad_(False)
    config = StrategicBehaviorCloningConfig(epochs=3, batch_rows=1, shuffle_seed=7)
    first = train_strategic_behavior_clone(initial, split, config)
    second = train_strategic_behavior_clone(initial, alternate_split, config)
    assert first.optimizer_steps == 6
    assert all(torch.equal(tensor, second.scorer.state_dict()[name])
               for name, tensor in first.scorer.state_dict().items())
    with pytest.raises(StrategicBehaviorCloningError, match="share a run seed"):
        StrategicDemonstrationSplit(training, training, SeedPartitionSpec(1, 2))
