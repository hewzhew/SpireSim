"""Optional PyTorch adapter and update for :mod:`sts_learning.node_sl`."""

from __future__ import annotations

import math
import operator
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass

import torch

from .node_sl import (
    NodeSLCombatExample,
    NodeSLConfig,
    NodeSLEnvironment,
    NodeSLEpisodeActor,
    NodeSLEpisodeResult,
    NodeSLInference,
    NodeSLNonCombatActor,
    NodeSLTrainingExample,
    NodeSLValueTarget,
)
from .torch_policy import RaggedCandidateScorer


class TorchNodeSLError(ValueError):
    """A scorer or optimizer violated the Node-SL PyTorch contract."""


@dataclass(frozen=True)
class TorchNodeSLUpdate:
    """Compact result of one real optimizer mutation."""

    examples: int
    policy_loss: float
    value_loss: float
    total_loss: float


@dataclass(frozen=True)
class TorchNodeSLGeneration:
    """One frozen actor cohort followed by exactly one learner mutation."""

    episode_results: tuple[NodeSLEpisodeResult, ...]
    update: TorchNodeSLUpdate
    victories: int
    defeats: int
    total_searches: int
    total_simulations: int


NodeSLEnvironmentFactory = Callable[[int, int], NodeSLEnvironment]


class TorchNodeSLEvaluator:
    """Expose one scalar actor-critic as policy priors and continuation value."""

    def __init__(self, scorer: RaggedCandidateScorer) -> None:
        if not isinstance(scorer, RaggedCandidateScorer):
            raise TorchNodeSLError("Node-SL evaluator requires RaggedCandidateScorer")
        if scorer.value_head is None or scorer.config.value_head_width != 1:
            raise TorchNodeSLError("Node-SL evaluator requires one scalar value head")
        self.scorer = scorer

    def __call__(self, decision_batch: Mapping[str, object]) -> NodeSLInference:
        with torch.inference_mode():
            output = self.scorer.actor_critic(decision_batch)
            if output.logits.row_count != 1 or output.row_values.numel() != 1:
                raise TorchNodeSLError("Node-SL evaluator requires one decision row")
            splits = output.logits.row_splits.detach().cpu().tolist()
            values = output.logits.values[splits[0] : splits[1]]
            priors = torch.softmax(values, dim=0).detach().cpu().tolist()
            continuation = float(output.row_values[0].clamp(-1.0, 1.0).item())
        return NodeSLInference(tuple(priors), continuation)


def run_node_sl_generation(
    scorer: RaggedCandidateScorer,
    optimizer: torch.optim.Optimizer,
    environment_factory: NodeSLEnvironmentFactory,
    seeds: Sequence[int],
    *,
    ascension_level: int,
    non_combat_actor: NodeSLNonCombatActor,
    search_config: NodeSLConfig | None = None,
    max_episode_atomic_actions: int = 4096,
    value_loss_weight: float = 1.0,
    max_gradient_norm: float | None = 5.0,
) -> TorchNodeSLGeneration:
    """Collect a frozen complete-run cohort, then update the shared scorer once."""

    if not callable(environment_factory):
        raise TorchNodeSLError("Node-SL generation requires an environment factory")
    normalized_seeds = tuple(_integer(seed, "Node-SL seed", minimum=0) for seed in seeds)
    if not normalized_seeds or len(set(normalized_seeds)) != len(normalized_seeds):
        raise TorchNodeSLError("Node-SL generation seeds must be non-empty and distinct")
    ascension = _integer(ascension_level, "Node-SL ascension", minimum=0)
    if ascension > 20:
        raise TorchNodeSLError("Node-SL ascension must be at most 20")

    # Every actor shares the unchanged scorer.  No optimizer mutation occurs
    # until every complete episode in this generation has terminated.
    evaluator = TorchNodeSLEvaluator(scorer)
    actor = NodeSLEpisodeActor(
        evaluator,
        non_combat_actor,
        search_config,
        max_episode_atomic_actions=max_episode_atomic_actions,
    )
    results = []
    for seed in normalized_seeds:
        env = environment_factory(seed, ascension)
        results.append(actor.run(env))
    episodes = tuple(results)
    examples = tuple(
        example for episode in episodes for example in episode.training_examples
    )
    value_targets = tuple(
        target for episode in episodes for target in episode.value_targets
    )
    if not examples:
        raise TorchNodeSLError(
            "Node-SL generation reached no combat policy target; no update was applied"
        )
    update = update_node_sl_scorer(
        scorer,
        optimizer,
        examples,
        value_targets=value_targets,
        value_loss_weight=value_loss_weight,
        max_gradient_norm=max_gradient_norm,
    )
    return TorchNodeSLGeneration(
        episode_results=episodes,
        update=update,
        victories=sum(episode.terminal_reward == 1 for episode in episodes),
        defeats=sum(episode.terminal_reward == -1 for episode in episodes),
        total_searches=sum(len(episode.searches) for episode in episodes),
        total_simulations=sum(
            search.simulations
            for episode in episodes
            for search in episode.searches
        ),
    )


def update_node_sl_scorer(
    scorer: RaggedCandidateScorer,
    optimizer: torch.optim.Optimizer,
    examples: Sequence[NodeSLTrainingExample | NodeSLCombatExample],
    *,
    value_targets: Sequence[NodeSLValueTarget] | None = None,
    value_loss_weight: float = 1.0,
    max_gradient_norm: float | None = 5.0,
) -> TorchNodeSLUpdate:
    """Fit search visits and actual outcomes, with explicit combat/run scope."""

    if not isinstance(scorer, RaggedCandidateScorer):
        raise TorchNodeSLError("Node-SL update requires RaggedCandidateScorer")
    if scorer.value_head is None or scorer.config.value_head_width != 1:
        raise TorchNodeSLError("Node-SL update requires one scalar value head")
    if not isinstance(optimizer, torch.optim.Optimizer):
        raise TorchNodeSLError("Node-SL update requires a torch optimizer")
    normalized = tuple(examples)
    if not normalized or not all(
        isinstance(row, (NodeSLTrainingExample, NodeSLCombatExample)) for row in normalized
    ):
        raise TorchNodeSLError("Node-SL update requires typed training examples")
    combat_scoped = isinstance(normalized[0], NodeSLCombatExample)
    if any(isinstance(row, NodeSLCombatExample) != combat_scoped for row in normalized):
        raise TorchNodeSLError("cannot mix combat and full-run value objectives")
    if combat_scoped and value_targets is not None:
        raise TorchNodeSLError("combat examples cannot use full-run value targets")
    weight = float(value_loss_weight)
    if not math.isfinite(weight) or weight < 0.0:
        raise TorchNodeSLError("Node-SL value loss weight must be finite and non-negative")
    if max_gradient_norm is not None:
        max_gradient_norm = float(max_gradient_norm)
        if not math.isfinite(max_gradient_norm) or max_gradient_norm <= 0.0:
            raise TorchNodeSLError("Node-SL max gradient norm must be finite and positive")

    policy_losses = []
    value_losses = []
    for example in normalized:
        output = scorer.actor_critic(example.policy_target.decision_batch)
        if output.logits.row_count != 1 or output.row_values.numel() != 1:
            raise TorchNodeSLError("Node-SL training example must contain one decision row")
        splits = output.logits.row_splits.detach().cpu().tolist()
        logits = output.logits.values[splits[0] : splits[1]]
        target = torch.as_tensor(
            example.policy_target.probabilities,
            dtype=logits.dtype,
            device=logits.device,
        )
        if target.numel() != logits.numel():
            raise TorchNodeSLError("Node-SL target does not match scorer candidates")
        policy_losses.append(-(target * torch.log_softmax(logits, dim=0)).sum())
        if value_targets is None:
            return_value = (
                example.combat_return if isinstance(example, NodeSLCombatExample)
                else example.run_return
            )
            value_losses.append(torch.nn.functional.mse_loss(
                output.row_values[0], output.row_values.new_tensor(return_value)
            ))
    if value_targets is None:
        value_rows = ()
    else:
        normalized_values = tuple(value_targets)
        if not normalized_values or not all(
            isinstance(target, NodeSLValueTarget) for target in normalized_values
        ):
            raise TorchNodeSLError("Node-SL value update requires typed value targets")
        value_rows = tuple(
            (target.decision_batch, target.run_return) for target in normalized_values
        )
    for decision_batch, return_value in value_rows:
        output = scorer.actor_critic(decision_batch)
        if output.logits.row_count != 1 or output.row_values.numel() != 1:
            raise TorchNodeSLError("Node-SL value target must contain one decision row")
        run_return = torch.tensor(
            return_value,
            dtype=output.row_values.dtype,
            device=output.row_values.device,
        )
        value_losses.append(torch.nn.functional.mse_loss(output.row_values[0], run_return))

    policy_loss = torch.stack(policy_losses).mean()
    value_loss = torch.stack(value_losses).mean()
    total_loss = policy_loss + weight * value_loss
    if not bool(torch.isfinite(total_loss)):
        raise TorchNodeSLError("Node-SL loss is non-finite")
    optimizer.zero_grad(set_to_none=True)
    total_loss.backward()
    if max_gradient_norm is not None:
        torch.nn.utils.clip_grad_norm_(scorer.parameters(), max_gradient_norm)
    optimizer.step()
    return TorchNodeSLUpdate(
        examples=len(normalized),
        policy_loss=float(policy_loss.detach().cpu().item()),
        value_loss=float(value_loss.detach().cpu().item()),
        total_loss=float(total_loss.detach().cpu().item()),
    )


def _integer(value: object, name: str, *, minimum: int) -> int:
    if isinstance(value, bool):
        raise TorchNodeSLError(f"{name} must not be bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchNodeSLError(f"{name} must be an integer") from error
    if normalized < minimum:
        raise TorchNodeSLError(f"{name} must be at least {minimum}")
    return normalized
