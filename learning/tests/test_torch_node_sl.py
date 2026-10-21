from __future__ import annotations

import importlib.util

import pytest

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.test_node_sl import _CombatNodeEnv, _evaluator
from sts_learning.node_sl import (
    NodeSLCombatExample,
    NodeSLConfig,
    NodeSLImprover,
    NodeSLInference,
    finalize_node_sl_episode,
    greedy_network_noncombat_actor,
    _public_context,
)


torch = pytest.importorskip("torch") if importlib.util.find_spec("torch") else None

if torch is not None:
    from sts_learning.torch_node_sl import (
        TorchNodeSLEvaluator,
        run_node_sl_generation,
        update_node_sl_scorer,
    )
    from sts_learning.torch_policy import RaggedCandidateScorer, RaggedScorerConfig


@pytest.mark.skipif(torch is None, reason="optional PyTorch dependency is not installed")
def test_node_sl_target_updates_actor_and_full_run_value() -> None:
    assert torch is not None
    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(),
        RaggedScorerConfig(
            hidden_dim=24,
            relation_layers=1,
            value_head=True,
        ),
    )
    record = NodeSLImprover(
        _evaluator,
        NodeSLConfig(simulations=16, exploration=1.0),
    ).improve(_CombatNodeEnv())
    examples = finalize_node_sl_episode(
        [record],
        terminal_reward=1,
        terminal_act=4,
        terminal_floor=57,
    )
    optimizer = torch.optim.Adam(scorer.parameters(), lr=0.01)
    actor_before = scorer.scorer[-1].weight.detach().clone()
    value_before = scorer.value_head[-1].bias.detach().clone()

    update = update_node_sl_scorer(scorer, optimizer, examples)

    assert update.examples == len(examples) == 2
    assert update.policy_loss > 0.0
    assert update.value_loss == pytest.approx(1.0)
    assert not torch.equal(actor_before, scorer.scorer[-1].weight.detach())
    assert not torch.equal(value_before, scorer.value_head[-1].bias.detach())
    inference = TorchNodeSLEvaluator(scorer)(record.policy_target.decision_batch)
    assert len(inference.priors) == 2
    assert -1.0 <= inference.value <= 1.0


@pytest.mark.skipif(torch is None, reason="optional PyTorch dependency is not installed")
def test_completed_rollout_target_trains_an_unvisited_symbolic_choice() -> None:
    assert torch is not None
    torch.manual_seed(7)
    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(),
        RaggedScorerConfig(hidden_dim=24, relation_layers=1, value_head=True),
    )
    record = NodeSLImprover(
        lambda batch: NodeSLInference((0.0, 1.0), 0.0),
        NodeSLConfig(simulations=1, objective="combat_hp"),
    ).improve(_CombatNodeEnv())
    target = record.policy_targets[1]
    assert target.visit_counts == (0, 0)
    assert target.probabilities == (0.0, 1.0)
    before = TorchNodeSLEvaluator(scorer)(target.decision_batch).priors[1]
    optimizer = torch.optim.Adam(scorer.parameters(), lr=0.01)
    update = update_node_sl_scorer(
        scorer, optimizer,
        [NodeSLCombatExample(target, record.best_completed_line.end)],
        value_loss_weight=0.0,
    )
    assert update.examples == 1 and update.policy_loss > 0.0
    assert TorchNodeSLEvaluator(scorer)(target.decision_batch).priors[1] > before


@pytest.mark.skipif(torch is None, reason="optional PyTorch dependency is not installed")
def test_generation_collects_a_frozen_cohort_before_one_update() -> None:
    assert torch is not None
    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(),
        RaggedScorerConfig(
            hidden_dim=24,
            relation_layers=1,
            value_head=True,
        ),
    )
    optimizer = torch.optim.Adam(scorer.parameters(), lr=0.01)
    optimizer_steps = 0
    original_step = optimizer.step

    def counted_step(*args, **kwargs):
        nonlocal optimizer_steps
        optimizer_steps += 1
        return original_step(*args, **kwargs)

    optimizer.step = counted_step  # type: ignore[method-assign]
    created: list[tuple[int, int]] = []

    def factory(seed: int, ascension: int) -> _CombatNodeEnv:
        created.append((seed, ascension))
        from learning.tests.test_node_sl import _CompleteEpisodeEnv

        return _CompleteEpisodeEnv()

    generation = run_node_sl_generation(
        scorer,
        optimizer,
        factory,
        [11, 12],
        ascension_level=0,
        non_combat_actor=greedy_network_noncombat_actor(
            TorchNodeSLEvaluator(scorer)
        ),
        search_config=NodeSLConfig(simulations=8, exploration=1.0),
        max_episode_atomic_actions=4,
    )

    assert created == [(11, 0), (12, 0)]
    assert optimizer_steps == 1
    assert len(generation.episode_results) == 2
    assert generation.victories == 2
    assert generation.defeats == 0
    assert generation.total_searches == 2
    assert generation.total_simulations == 16
    assert generation.update.examples == sum(
        len(episode.training_examples) for episode in generation.episode_results
    )


@pytest.mark.skipif(torch is None, reason="optional PyTorch dependency is not installed")
def test_all_defeat_combat_examples_still_train_value_and_reject_mixed_objectives() -> None:
    from sts_learning.torch_node_sl import TorchNodeSLError

    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(), RaggedScorerConfig(hidden_dim=24, relation_layers=1, value_head=True)
    )
    env = _CombatNodeEnv()
    record = NodeSLImprover(_evaluator, NodeSLConfig(simulations=8)).improve(env)
    env.choose(["combat-root-token"], [0])
    env.step()
    examples = [NodeSLCombatExample(record.policy_target, _public_context(env))]
    optimizer = torch.optim.Adam(scorer.parameters(), lr=0.01)
    before = scorer.value_head[-1].bias.detach().clone()
    update = update_node_sl_scorer(scorer, optimizer, examples)
    assert update.value_loss == pytest.approx(1.0)
    assert not torch.equal(before, scorer.value_head[-1].bias.detach())
    run_examples = finalize_node_sl_episode(
        [record], terminal_reward=-1, terminal_act=1, terminal_floor=3
    )
    with pytest.raises(TorchNodeSLError, match="cannot mix"):
        update_node_sl_scorer(scorer, optimizer, examples + list(run_examples))
