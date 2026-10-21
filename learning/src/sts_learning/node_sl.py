"""Exact node-restart policy improvement over one combat boundary.

The improver owns tree search, not game mechanics.  It repeatedly restores one
opaque ``LearningEnvV3`` checkpoint, explores complete typed ordinal paths, and
stops when the run leaves combat or terminates.  Old oracle/search owners are
deliberately absent from this module.
"""

from __future__ import annotations

import math
import operator
import random
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Protocol

from .decision_rows import PreparedDecisionRows


class NodeSLError(ValueError):
    """The environment, evaluator, or search result violated the Node-SL contract."""


class NodeSLEnvironment(Protocol):
    """Single-slot subset of the run-level Rust learning bridge."""

    @property
    def slot_count(self) -> int: ...

    @property
    def ready(self) -> bool: ...

    def decision_batch(
        self,
        *,
        semantic: bool = False,
        privileged_node_sl_oracle: bool = False,
    ) -> Mapping[str, object]: ...

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None: ...

    def step(self) -> Mapping[str, object]: ...

    def checkpoint_slots(self, slot_indices: list[int]) -> object: ...

    def restore_slots(self, slot_indices: list[int], checkpoints: object) -> None: ...

    def public_run_contexts(self) -> Sequence[object]: ...


@dataclass(frozen=True)
class NodeSLInference:
    """Policy prior and bounded task-continuation value for one decision row."""

    priors: tuple[float, ...]
    value: float

    def __post_init__(self) -> None:
        priors = tuple(float(prior) for prior in self.priors)
        if not priors or any(not math.isfinite(prior) or prior < 0.0 for prior in priors):
            raise NodeSLError("Node-SL priors must be finite, non-negative, and non-empty")
        total = sum(priors)
        if not math.isfinite(total) or total <= 0.0:
            raise NodeSLError("Node-SL priors must have positive finite mass")
        value = float(self.value)
        if not math.isfinite(value) or not -1.0 <= value <= 1.0:
            raise NodeSLError("Node-SL continuation value must be within [-1, 1]")
        object.__setattr__(self, "priors", tuple(prior / total for prior in priors))
        object.__setattr__(self, "value", value)


NodeSLEvaluator = Callable[[Mapping[str, object]], NodeSLInference]
NodeSLNonCombatActor = Callable[["NodeSLEnvironment"], None]


@dataclass(frozen=True, order=True)
class NodeSLStateKey:
    """Exact lineage identity plus the current public symbolic-selection surface."""

    decision_token_id: str
    public_snapshot_id: str
    phase: int


@dataclass(frozen=True)
class NodeSLResourceSnapshot:
    """Uncombined public resource facts at a combat entry or exit."""

    act: int
    floor: int
    hp: int
    max_hp: int
    gold: int
    potion_ids: tuple[str | None, ...]
    is_combat: bool
    is_terminal: bool


@dataclass(frozen=True)
class NodeSLPolicyTarget:
    """Completed-plan targets, exact solutions, or visits before finding a plan."""

    state_key: NodeSLStateKey
    decision_batch: Mapping[str, object]
    visit_counts: tuple[int, ...]
    probabilities: tuple[float, ...]
    solved_action_values: tuple[float, ...] | None = None
    completed_action_values: tuple[float | None, ...] | None = None

    def __post_init__(self) -> None:
        counts = tuple(_integer(value, "visit count", minimum=0) for value in self.visit_counts)
        probabilities = tuple(float(value) for value in self.probabilities)
        if not counts or len(counts) != len(probabilities):
            raise NodeSLError("Node-SL policy target columns are empty or misaligned")
        if sum(counts) <= 0 and self.completed_action_values is None:
            raise NodeSLError("Node-SL policy target has no visited root edge")
        if any(not math.isfinite(value) or value < 0.0 for value in probabilities):
            raise NodeSLError("Node-SL policy probabilities must be finite and non-negative")
        if not math.isclose(sum(probabilities), 1.0, rel_tol=0.0, abs_tol=1e-9):
            raise NodeSLError("Node-SL policy probabilities must sum to one")
        object.__setattr__(self, "visit_counts", counts)
        object.__setattr__(self, "probabilities", probabilities)
        if self.solved_action_values is not None:
            values = tuple(float(value) for value in self.solved_action_values)
            if len(values) != len(counts) or any(
                not math.isfinite(value) or not -1.0 <= value <= 1.0 for value in values
            ):
                raise NodeSLError("Node-SL solved action values are invalid or misaligned")
            optimum = max(values)
            if any(p > 0.0 and value < optimum for p, value in zip(probabilities, values)):
                raise NodeSLError("Node-SL solved policy assigns mass to a suboptimal action")
            object.__setattr__(self, "solved_action_values", values)
        if self.completed_action_values is not None:
            values = tuple(None if value is None else float(value)
                           for value in self.completed_action_values)
            observed = [value for value in values if value is not None]
            if len(values) != len(counts) or not observed or any(
                not math.isfinite(value) or not -1.0 <= value <= 1.0 for value in observed
            ):
                raise NodeSLError("Node-SL completed action values are invalid or misaligned")
            optimum = max(observed)
            if any(p > 0.0 and value != optimum for p, value in zip(probabilities, values)):
                raise NodeSLError("Node-SL completed policy does not follow its best known plans")
            object.__setattr__(self, "completed_action_values", values)


@dataclass(frozen=True)
class NodeSLCompletedLine:
    """A searched combat line that reached a real node exit or run terminal."""

    action_ordinal_paths: tuple[tuple[int, ...], ...]
    search_value: float
    terminal_reward: int | None
    continuation_value: float | None
    start: NodeSLResourceSnapshot
    end: NodeSLResourceSnapshot
    combat_return: float | None = None

    @property
    def exited_combat(self) -> bool:
        return not self.end.is_combat and not self.end.is_terminal


@dataclass(frozen=True)
class NodeSLSearchRecord:
    """Search output retained until the caller's executed task supplies its outcome."""

    policy_targets: tuple[NodeSLPolicyTarget, ...]
    recommended_action_ordinals: tuple[int, ...]
    simulations: int  # Actual tree traversals, including cached exact returns.
    expanded_nodes: int
    completed_lines: int
    best_completed_line: NodeSLCompletedLine | None

    def __post_init__(self) -> None:
        if not self.policy_targets:
            raise NodeSLError("Node-SL search record requires a root policy target")

    @property
    def policy_target(self) -> NodeSLPolicyTarget:
        """Compatibility name for the mandatory root target."""

        return self.policy_targets[0]


@dataclass(frozen=True)
class NodeSLTrainingExample:
    """A root search target joined to the eventual complete-run return."""

    policy_target: NodeSLPolicyTarget
    run_return: float
    terminal_act: int
    terminal_floor: int
    node_line: NodeSLCompletedLine | None

    def __post_init__(self) -> None:
        run_return = float(self.run_return)
        if not math.isfinite(run_return) or not -1.0 <= run_return <= 1.0:
            raise NodeSLError("Node-SL run return must be within [-1, 1]")
        object.__setattr__(self, "run_return", run_return)
        object.__setattr__(self, "terminal_act", _integer(self.terminal_act, "terminal act", minimum=0))
        object.__setattr__(
            self,
            "terminal_floor",
            _integer(self.terminal_floor, "terminal floor", minimum=0),
        )


def combat_outcome_value(end: NodeSLResourceSnapshot) -> float:
    """Combat-local utility: death = -1, surviving exit = HP / maximum HP.

    Exit HP includes automatic post-combat healing. This is deliberately not
    a whole-run value or a price for consuming a potion.
    """

    if end.is_combat and not end.is_terminal:
        raise NodeSLError("combat outcome requires a completed battle")
    return -1.0 if end.hp == 0 else min(1.0, end.hp / end.max_hp)


@dataclass(frozen=True)
class NodeSLCombatExample:
    """Improved search policy supervised by an actually executed combat outcome."""

    policy_target: NodeSLPolicyTarget
    end: NodeSLResourceSnapshot

    def __post_init__(self) -> None:
        combat_outcome_value(self.end)

    @property
    def combat_return(self) -> float:
        return combat_outcome_value(self.end)


@dataclass(frozen=True)
class NodeSLCombatResult:
    """Executed combat facts, retaining a live state when the work limit is hit."""

    searches: tuple[NodeSLSearchRecord, ...]
    start: NodeSLResourceSnapshot
    end: NodeSLResourceSnapshot
    atomic_actions: int

    @property
    def completed(self) -> bool:
        return self.end.is_terminal or not self.end.is_combat

    @property
    def training_examples(self) -> tuple[NodeSLCombatExample, ...]:
        # A work limit is neither death nor a fabricated terminal value label.
        if not self.completed:
            return ()
        return tuple(
            NodeSLCombatExample(target, self.end)
            for record in self.searches
            for target in record.policy_targets
        )


@dataclass(frozen=True)
class NodeSLValueTarget:
    """One visited run state with scalar outcome and uncombined terminal facts."""

    state_key: NodeSLStateKey
    decision_batch: Mapping[str, object]
    state_resources: NodeSLResourceSnapshot
    run_return: float
    terminal_resources: NodeSLResourceSnapshot

    def __post_init__(self) -> None:
        run_return = float(self.run_return)
        if not math.isfinite(run_return) or not -1.0 <= run_return <= 1.0:
            raise NodeSLError("Node-SL value return must be within [-1, 1]")
        if self.state_resources.is_terminal:
            raise NodeSLError("Node-SL value target cannot start at terminal")
        if not self.terminal_resources.is_terminal:
            raise NodeSLError("Node-SL value target must carry terminal run resources")
        object.__setattr__(self, "run_return", run_return)


@dataclass(frozen=True)
class NodeSLEpisodeResult:
    """One complete run generated by network play plus combat Node-SL improvement."""

    searches: tuple[NodeSLSearchRecord, ...]
    training_examples: tuple[NodeSLTrainingExample, ...]
    value_targets: tuple[NodeSLValueTarget, ...]
    atomic_actions: int
    terminal_reward: int
    terminal_act: int
    terminal_floor: int


@dataclass(frozen=True)
class NodeSLConfig:
    """Bounded UCB exploration and completed-plan improvement at an exact node."""

    simulations: int = 64
    exploration: float = 1.5
    max_atomic_actions: int = 256
    max_selection_steps: int = 64
    objective: str = "run"
    leaf_evaluation: str = "rollout"
    rollout_seed: int = 0

    def __post_init__(self) -> None:
        if self.objective not in {"run", "combat_hp"}:
            raise NodeSLError("Node-SL objective must be run or combat_hp")
        if self.leaf_evaluation not in {"rollout", "value"}:
            raise NodeSLError("Node-SL leaf_evaluation must be rollout or value")
        object.__setattr__(self, "simulations", _integer(self.simulations, "simulations", minimum=1))
        object.__setattr__(self, "rollout_seed", _integer(self.rollout_seed, "rollout_seed", minimum=0))
        exploration = float(self.exploration)
        if not math.isfinite(exploration) or exploration < 0.0:
            raise NodeSLError("Node-SL exploration must be finite and non-negative")
        object.__setattr__(self, "exploration", exploration)
        object.__setattr__(
            self,
            "max_atomic_actions",
            _integer(self.max_atomic_actions, "max_atomic_actions", minimum=1),
        )
        object.__setattr__(
            self,
            "max_selection_steps",
            _integer(self.max_selection_steps, "max_selection_steps", minimum=1),
        )


@dataclass
class _Edge:
    visits: int = 0
    value_sum: float = 0.0
    child: NodeSLStateKey | None = None
    solved_value: float | None = None

    @property
    def mean_value(self) -> float:
        return 0.0 if self.visits == 0 else self.value_sum / self.visits


@dataclass
class _Node:
    key: NodeSLStateKey
    priors: tuple[float, ...]
    edges: list[_Edge] = field(init=False)

    def __post_init__(self) -> None:
        self.edges = [_Edge() for _ in self.priors]

    @property
    def visits(self) -> int:
        return sum(edge.visits for edge in self.edges)

    @property
    def solved_value(self) -> float | None:
        values = [edge.solved_value for edge in self.edges if edge.solved_value is not None]
        return max(values) if len(values) == len(self.edges) else None


@dataclass(frozen=True)
class _SimulationResult:
    value: float
    completed: NodeSLCompletedLine | None


class NodeSLImprover:
    """Improve the current combat policy by exact node-local restart search."""

    def __init__(self, evaluator: NodeSLEvaluator, config: NodeSLConfig | None = None) -> None:
        if not callable(evaluator):
            raise NodeSLError("Node-SL evaluator must be callable")
        self.evaluator = evaluator
        self.config = config or NodeSLConfig()
        self._rollout_rng = random.Random(self.config.rollout_seed)

    def improve(
        self,
        env: NodeSLEnvironment,
        *,
        incumbent_action_paths: Sequence[Sequence[int]] | None = None,
    ) -> NodeSLSearchRecord:
        if operator.index(env.slot_count) != 1:
            raise NodeSLError("Node-SL currently requires exactly one environment slot")
        checkpoint = env.checkpoint_slots([0])
        start = _public_context(env)
        if not start.is_combat or start.is_terminal:
            raise NodeSLError("Node-SL entry must be a live combat boundary")

        tree: dict[NodeSLStateKey, _Node] = {}
        root_batch = env.decision_batch(semantic=True)
        root_key, root_count = _decision_identity(root_batch)
        root_inference = _evaluate(self.evaluator, root_batch, root_count)
        tree[root_key] = _Node(root_key, root_inference.priors)
        completed: list[NodeSLCompletedLine] = []
        traversals = 0
        try:
            if incumbent_action_paths is not None:
                line = self._replay_incumbent(env, start, incumbent_action_paths)
                completed.append(line)
                self._backup(
                    [(tree[root_key], line.action_ordinal_paths[0][0])],
                    line.search_value,
                    solved=(len(line.action_ordinal_paths) == 1
                            and len(line.action_ordinal_paths[0]) == 1),
                )
                traversals += 1
            for _ in range(self.config.simulations - traversals):
                if tree[root_key].solved_value is not None:
                    break
                env.restore_slots([0], checkpoint)
                result = self._simulate(env, start, tree)
                traversals += 1
                if result.completed is not None:
                    completed.append(result.completed)
            env.restore_slots([0], checkpoint)
            recommended, targets = self._recommended_action(env, tree, completed)
        finally:
            env.restore_slots([0], checkpoint)

        root = tree[root_key]
        visits = tuple(edge.visits for edge in root.edges)
        total = sum(visits)
        if total <= 0:
            raise NodeSLError("Node-SL completed no root traversal")
        best = max(
            completed,
            key=lambda line: (line.search_value, -len(line.action_ordinal_paths)),
            default=None,
        )
        return NodeSLSearchRecord(
            policy_targets=targets,
            recommended_action_ordinals=recommended,
            simulations=traversals,
            expanded_nodes=len(tree),
            completed_lines=len(completed),
            best_completed_line=best,
        )

    def _replay_incumbent(
        self,
        env: NodeSLEnvironment,
        start: NodeSLResourceSnapshot,
        action_paths: Sequence[Sequence[int]],
    ) -> NodeSLCompletedLine:
        paths = tuple(tuple(path) for path in action_paths)
        if self.config.objective != "combat_hp" or not paths:
            raise NodeSLError("an incumbent requires a non-empty combat_hp plan")
        if len(paths) > self.config.max_atomic_actions or any(
            len(path) > self.config.max_selection_steps for path in paths
        ):
            raise NodeSLError("incumbent plan exceeds the configured traversal limits")
        for index, path in enumerate(paths):
            _execute_ordinal_path(env, path)
            result = self._completed_result(env, start, paths[:index + 1], env.step())
            if result is not None:
                if index != len(paths) - 1 or result.value < 0.0:
                    raise NodeSLError("incumbent is not a complete surviving combat plan")
                return result.completed
        raise NodeSLError("incumbent ended before combat completed")

    def _simulate(
        self,
        env: NodeSLEnvironment,
        start: NodeSLResourceSnapshot,
        tree: dict[NodeSLStateKey, _Node],
    ) -> _SimulationResult:
        trajectory: list[tuple[_Node, int]] = []
        action_paths: list[tuple[int, ...]] = []
        pending_ordinals: list[int] = []
        atomic_actions = 0

        while True:
            batch = env.decision_batch(semantic=True)
            key, candidate_count = _decision_identity(batch)
            node = tree.get(key)
            if node is None:
                inference = _evaluate(self.evaluator, batch, candidate_count)
                tree[key] = _Node(key, inference.priors)
                if self.config.leaf_evaluation == "value":
                    self._backup(trajectory, inference.value)
                    return _SimulationResult(inference.value, None)
                result = self._rollout_leaf(
                    env,
                    start,
                    action_paths,
                    pending_ordinals,
                    atomic_actions,
                    inference,
                )
                self._backup(trajectory, result.value)
                return result

            solved_value = node.solved_value
            if solved_value is not None:
                self._backup(trajectory, solved_value, solved=True)
                return _SimulationResult(solved_value, None)

            ordinal = _select_ucb(
                node, self.config.exploration,
                explore_unsolved=self.config.objective == "combat_hp",
            )
            trajectory.append((node, ordinal))
            edge_value = node.edges[ordinal].solved_value
            if edge_value is not None:
                # Keep allocation/visit targets meaningful while avoiding
                # another environment rollout for an already exact answer.
                self._backup(trajectory, edge_value, solved=True)
                return _SimulationResult(edge_value, None)
            pending_ordinals.append(ordinal)
            token_id = key.decision_token_id
            env.choose([token_id], [ordinal])

            if not env.ready:
                next_batch = env.decision_batch(semantic=True)
                child_key, _ = _decision_identity(next_batch)
                node.edges[ordinal].child = child_key
                if len(pending_ordinals) >= self.config.max_selection_steps:
                    inference = _evaluate(
                        self.evaluator,
                        next_batch,
                        _decision_identity(next_batch)[1],
                    )
                    self._backup(trajectory, inference.value)
                    return _SimulationResult(inference.value, None)
                continue

            step = env.step()
            action_paths.append(tuple(pending_ordinals))
            pending_ordinals.clear()
            atomic_actions += 1
            result = self._completed_result(env, start, action_paths, step)
            if result is not None:
                # Only an actual task terminal closes this edge. A strategic
                # continuation critic or one arbitrary rollout is an estimate,
                # not a solution to all actions under the edge.
                self._backup(
                    trajectory,
                    result.value,
                    solved=result.completed.continuation_value is None,
                )
                return result
            if atomic_actions >= self.config.max_atomic_actions:
                next_batch = env.decision_batch(semantic=True)
                _, next_count = _decision_identity(next_batch)
                value = _evaluate(self.evaluator, next_batch, next_count).value
                self._backup(trajectory, value)
                return _SimulationResult(value, None)

            next_batch = env.decision_batch(semantic=True)
            child_key, _ = _decision_identity(next_batch)
            node.edges[ordinal].child = child_key

    def _rollout_leaf(
        self,
        env: NodeSLEnvironment,
        start: NodeSLResourceSnapshot,
        action_paths: list[tuple[int, ...]],
        pending_ordinals: list[int],
        atomic_actions: int,
        inference: NodeSLInference,
    ) -> _SimulationResult:
        """Sample a continuation from the policy using search-owned randomness."""

        current = inference
        while True:
            if atomic_actions >= self.config.max_atomic_actions:
                return _SimulationResult(current.value, None)
            # Argmax turns tiny, untrained logit differences into a fixed bad
            # continuation at every leaf. Sample actions without changing the
            # simulator's checkpoint/RNG or the tree's allocation priors.
            ordinal = self._rollout_rng.choices(
                range(len(current.priors)), weights=current.priors, k=1,
            )[0]
            batch = env.decision_batch(semantic=True)
            key, candidate_count = _decision_identity(batch)
            if candidate_count != len(current.priors):
                raise NodeSLError("Node-SL leaf inference no longer matches its decision")
            pending_ordinals.append(ordinal)
            env.choose([key.decision_token_id], [ordinal])
            if not env.ready:
                if len(pending_ordinals) >= self.config.max_selection_steps:
                    return _SimulationResult(current.value, None)
                batch = env.decision_batch(semantic=True)
                _, candidate_count = _decision_identity(batch)
                current = _evaluate(self.evaluator, batch, candidate_count)
                continue

            step = env.step()
            action_paths.append(tuple(pending_ordinals))
            pending_ordinals.clear()
            atomic_actions += 1
            result = self._completed_result(env, start, action_paths, step)
            if result is not None:
                return result
            batch = env.decision_batch(semantic=True)
            _, candidate_count = _decision_identity(batch)
            current = _evaluate(self.evaluator, batch, candidate_count)

    def _completed_result(
        self,
        env: NodeSLEnvironment,
        start: NodeSLResourceSnapshot,
        action_paths: Sequence[tuple[int, ...]],
        step: Mapping[str, object],
    ) -> _SimulationResult | None:
        end = _public_context(env)
        if end.is_combat and not end.is_terminal:
            return None
        reward = _terminal_reward(step)
        continuation = None
        combat_return = None
        if self.config.objective == "combat_hp":
            value = combat_return = combat_outcome_value(end)
        elif end.is_terminal:
            value = float(reward if reward is not None else 0)
        else:
            batch = env.decision_batch(semantic=True)
            value = continuation = _evaluate(
                self.evaluator, batch, _decision_identity(batch)[1]
            ).value
        return _SimulationResult(
            value,
            NodeSLCompletedLine(
                action_ordinal_paths=tuple(action_paths),
                search_value=value,
                terminal_reward=reward,
                continuation_value=continuation,
                start=start,
                end=end,
                combat_return=combat_return,
            ),
        )

    @staticmethod
    def _backup(
        trajectory: Sequence[tuple[_Node, int]],
        value: float,
        *,
        solved: bool = False,
    ) -> None:
        for node, ordinal in reversed(trajectory):
            edge = node.edges[ordinal]
            edge.visits += 1
            edge.value_sum += value
            if solved:
                edge.solved_value = value
            optimum = node.solved_value
            solved = optimum is not None
            if optimum is not None:
                value = optimum

    def _recommended_action(
        self,
        env: NodeSLEnvironment,
        tree: dict[NodeSLStateKey, _Node],
        completed: Sequence[NodeSLCompletedLine],
    ) -> tuple[tuple[int, ...], tuple[NodeSLPolicyTarget, ...]]:
        ordinals: list[int] = []
        targets: list[NodeSLPolicyTarget] = []
        # Search effort is not a policy preference. For combat-local planning,
        # execute a fully tested surviving line; an unfinished critic estimate
        # must not displace it. Failed rollouts alone do not justify committing
        # to death while other continuations are still open.
        best = max(completed, key=lambda line: (line.search_value, -len(line.action_ordinal_paths)),
                   default=None)
        plan = (best.action_ordinal_paths[0]
                if self.config.objective == "combat_hp" and best is not None
                and best.search_value >= 0.0 else None)
        for _ in range(self.config.max_selection_steps):
            batch = env.decision_batch(semantic=True)
            key, candidate_count = _decision_identity(batch)
            node = tree.get(key)
            if plan is not None:
                prefix = tuple(ordinals)
                values: list[float | None] = [None] * candidate_count
                for line in completed:
                    path = line.action_ordinal_paths[0]
                    if path[:len(prefix)] == prefix and len(path) > len(prefix):
                        index = path[len(prefix)]
                        previous = values[index]
                        values[index] = (line.search_value if previous is None
                                         else max(previous, line.search_value))
                targets.append(_policy_target(batch, node, completed_values=tuple(values)))
                ordinal = plan[len(ordinals)]
            elif node is None:
                inference = _evaluate(self.evaluator, batch, candidate_count)
                ordinal = max(range(candidate_count), key=lambda index: inference.priors[index])
            else:
                eligible = tuple(range(candidate_count))
                if self.config.objective == "combat_hp":
                    eligible = tuple(i for i in eligible if node.edges[i].solved_value != -1.0) or eligible
                if node.visits > 0:
                    targets.append(_policy_target(batch, node, eligible_actions=eligible))
                solved = node.solved_value is not None
                ordinal = max(
                    eligible,
                    key=lambda index: (
                        (
                            node.edges[index].solved_value if solved
                            else node.edges[index].visits
                        ),
                        node.edges[index].mean_value,
                        node.priors[index],
                        -index,
                    ),
                )
            ordinals.append(ordinal)
            env.choose([key.decision_token_id], [ordinal])
            if env.ready:
                if plan is not None and tuple(ordinals) != plan:
                    raise NodeSLError("completed plan no longer matches the symbolic action")
                if not targets or targets[0].state_key != next(iter(tree)):
                    raise NodeSLError("Node-SL recommendation lost its root policy target")
                return tuple(ordinals), tuple(targets)
        raise NodeSLError("Node-SL recommendation did not finish symbolic selection")


def play_node_sl_combat(
    env: NodeSLEnvironment,
    evaluator: NodeSLEvaluator,
    search_config: NodeSLConfig | None = None,
    *,
    max_atomic_actions: int = 256,
) -> NodeSLCombatResult:
    """Execute a combat suffix; omit search_config for model-only evaluation.

    Both modes consume the same public network inputs. Search additionally
    restarts the exact node (Seed-SL), retaining its private trial outcomes.
    """

    limit = _integer(max_atomic_actions, "max_atomic_actions", minimum=1)
    if operator.index(env.slot_count) != 1:
        raise NodeSLError("combat actor requires exactly one environment slot")
    start = _public_context(env)
    if not start.is_combat or start.is_terminal:
        raise NodeSLError("combat actor requires a live combat entry")
    if search_config is not None and search_config.objective != "combat_hp":
        raise NodeSLError("combat actor requires the combat_hp search objective")
    improver = NodeSLImprover(evaluator, search_config) if search_config else None
    searches: list[NodeSLSearchRecord] = []
    incumbent = None
    for step_index in range(limit):
        if improver is None:
            _execute_greedy_network_action(env, evaluator, 64)
        else:
            record = improver.improve(env, incumbent_action_paths=incumbent)
            searches.append(record)
            line = record.best_completed_line
            incumbent = (line.action_ordinal_paths[1:]
                         if line is not None and line.search_value >= 0.0 else None)
            _execute_ordinal_path(env, record.recommended_action_ordinals)
        env.step()
        end = _public_context(env)
        if end.is_terminal or not end.is_combat:
            return NodeSLCombatResult(tuple(searches), start, end, step_index + 1)
    return NodeSLCombatResult(tuple(searches), start, end, limit)


class NodeSLEpisodeActor:
    """Generate one full run and retain only combat search targets plus its outcome."""

    def __init__(
        self,
        evaluator: NodeSLEvaluator,
        non_combat_actor: NodeSLNonCombatActor,
        config: NodeSLConfig | None = None,
        *,
        max_episode_atomic_actions: int = 4096,
    ) -> None:
        self.evaluator = evaluator
        if not callable(non_combat_actor):
            raise NodeSLError("Node-SL requires an explicit non-combat actor")
        self.non_combat_actor = non_combat_actor
        self.improver = NodeSLImprover(evaluator, config)
        if self.improver.config.objective != "run":
            raise NodeSLError("full-run actor requires the run search objective")
        self.max_episode_atomic_actions = _integer(
            max_episode_atomic_actions,
            "max_episode_atomic_actions",
            minimum=1,
        )

    def run(self, env: NodeSLEnvironment) -> NodeSLEpisodeResult:
        if operator.index(env.slot_count) != 1:
            raise NodeSLError("Node-SL episode actor requires exactly one environment slot")
        searches: list[NodeSLSearchRecord] = []
        visited_states: list[
            tuple[NodeSLStateKey, Mapping[str, object], NodeSLResourceSnapshot]
        ] = []
        atomic_actions = 0

        while atomic_actions < self.max_episode_atomic_actions:
            context = _public_context(env)
            if context.is_terminal:
                raise NodeSLError("Node-SL episode actor started or resumed after terminal")
            root_batch = env.decision_batch(semantic=True)
            root_key, _ = _decision_identity(root_batch)
            visited_states.append(
                (root_key, PreparedDecisionRows.capture(root_batch).payload, context)
            )
            if context.is_combat:
                search = self.improver.improve(env)
                searches.append(search)
                _execute_ordinal_path(env, search.recommended_action_ordinals)
            else:
                self.non_combat_actor(env)
                if not env.ready:
                    raise NodeSLError(
                        "explicit Node-SL non-combat actor did not finish the action"
                    )
            step = env.step()
            atomic_actions += 1
            reward = _terminal_reward(step)
            if reward is None:
                continue
            terminal_act, terminal_floor = _terminal_location(step)
            terminal_resources = _public_context(env)
            examples = finalize_node_sl_episode(
                searches,
                terminal_reward=reward,
                terminal_act=terminal_act,
                terminal_floor=terminal_floor,
            )
            value_targets = tuple(
                NodeSLValueTarget(
                    state_key=key,
                    decision_batch=batch,
                    state_resources=resources,
                    run_return=float(reward),
                    terminal_resources=terminal_resources,
                )
                for key, batch, resources in visited_states
            )
            return NodeSLEpisodeResult(
                searches=tuple(searches),
                training_examples=examples,
                value_targets=value_targets,
                atomic_actions=atomic_actions,
                terminal_reward=reward,
                terminal_act=terminal_act,
                terminal_floor=terminal_floor,
            )
        raise NodeSLError(
            "Node-SL episode exceeded max_episode_atomic_actions before terminal"
        )


def finalize_node_sl_episode(
    records: Sequence[NodeSLSearchRecord],
    *,
    terminal_reward: int,
    terminal_act: int,
    terminal_floor: int,
) -> tuple[NodeSLTrainingExample, ...]:
    """Join every combat search record to one eventual complete-run outcome."""

    reward = _integer(terminal_reward, "terminal reward", minimum=-1)
    if reward > 1:
        raise NodeSLError("terminal reward must be within [-1, 1]")
    normalized = tuple(records)
    if not all(isinstance(record, NodeSLSearchRecord) for record in normalized):
        raise NodeSLError("Node-SL episode records must be typed search records")
    return tuple(
        NodeSLTrainingExample(
            policy_target=target,
            run_return=float(reward),
            terminal_act=terminal_act,
            terminal_floor=terminal_floor,
            node_line=record.best_completed_line,
        )
        for record in normalized
        for target in record.policy_targets
    )


def _select_ucb(node: _Node, exploration: float, *, explore_unsolved: bool) -> int:
    candidates = [i for i, edge in enumerate(node.edges)
                  if not explore_unsolved or edge.solved_value is None]
    unvisited = [i for i in candidates if node.edges[i].visits == 0]
    if unvisited:
        return max(unvisited, key=lambda i: (node.priors[i], -i))
    # A low learned probability is not evidence that this branch is understood.
    # Priors order first visits and break ties; actual work controls exploration.
    # This is an allocation bonus, not an IID confidence bound on combat returns.
    log_visits = math.log(max(2, node.visits))
    return max(
        candidates,
        key=lambda ordinal: (
            (node.edges[ordinal].solved_value if node.edges[ordinal].solved_value is not None
             else node.edges[ordinal].mean_value)
            + exploration * math.sqrt(log_visits / node.edges[ordinal].visits),
            node.priors[ordinal],
            -ordinal,
        ),
    )


def _policy_target(
    batch: Mapping[str, object],
    node: _Node | None,
    *,
    completed_values: tuple[float | None, ...] | None = None,
    eligible_actions: tuple[int, ...] | None = None,
) -> NodeSLPolicyTarget:
    key, candidate_count = _decision_identity(batch)
    if node is not None and (key != node.key or candidate_count != len(node.edges)):
        raise NodeSLError("Node-SL tree node no longer matches its public decision row")
    visits = (tuple(edge.visits for edge in node.edges) if node is not None
              else (0,) * candidate_count)
    total = sum(visits)
    if total <= 0 and completed_values is None:
        raise NodeSLError("Node-SL cannot train from an unvisited tree node")
    solved_values = None
    probabilities = tuple(visit / total for visit in visits) if total else ()
    if eligible_actions is not None and node is not None:
        # A proven death is not a preference label just because it consumed
        # the only trial. Condition visits (or the prior) on the remaining
        # actions; this does not assert that an untried continuation survives.
        weights = tuple(visits[i] if i in eligible_actions else 0 for i in range(candidate_count))
        if not any(weights):
            weights = tuple(node.priors[i] if i in eligible_actions else 0.0
                            for i in range(candidate_count))
        if not any(weights):
            weights = tuple(1.0 if i in eligible_actions else 0.0 for i in range(candidate_count))
        mass = sum(weights)
        probabilities = tuple(weight / mass for weight in weights)
    optimum = node.solved_value if node is not None else None
    if optimum is not None:
        solved_values = tuple(
            edge.solved_value for edge in node.edges if edge.solved_value is not None
        )
        optimal_count = sum(value == optimum for value in solved_values)
        probabilities = tuple(
            1.0 / optimal_count if value == optimum else 0.0 for value in solved_values
        )
    if completed_values is not None:
        best = max(value for value in completed_values if value is not None)
        count = sum(value == best for value in completed_values)
        probabilities = tuple(1.0 / count if value == best else 0.0 for value in completed_values)
    return NodeSLPolicyTarget(
        state_key=key,
        decision_batch=PreparedDecisionRows.capture(batch).payload,
        visit_counts=visits,
        probabilities=probabilities,
        solved_action_values=solved_values,
        completed_action_values=completed_values,
    )


def _evaluate(
    evaluator: NodeSLEvaluator,
    batch: Mapping[str, object],
    candidate_count: int,
) -> NodeSLInference:
    inference = evaluator(batch)
    if not isinstance(inference, NodeSLInference):
        raise NodeSLError("Node-SL evaluator must return NodeSLInference")
    if len(inference.priors) != candidate_count:
        raise NodeSLError(
            "Node-SL evaluator prior count does not match the legal candidates"
        )
    return inference


def _decision_identity(batch: Mapping[str, object]) -> tuple[NodeSLStateKey, int]:
    if not isinstance(batch, Mapping):
        raise NodeSLError("Node-SL decision batch must be a mapping")
    slots = _column(batch, "slot_indices")
    counts = _column(batch, "candidate_counts")
    phases = _column(batch, "phase")
    tokens = _text_column(batch, "decision_token_ids")
    snapshots = _text_column(batch, "public_snapshot_ids")
    if not all(len(column) == 1 for column in (slots, counts, phases, tokens, snapshots)):
        raise NodeSLError("Node-SL decision batch must contain exactly one aligned row")
    if slots[0] != 0:
        raise NodeSLError("Node-SL single decision row must belong to slot zero")
    if counts[0] <= 0:
        raise NodeSLError("Node-SL decision row has no legal candidate")
    return NodeSLStateKey(tokens[0], snapshots[0], phases[0]), counts[0]


def _public_context(env: NodeSLEnvironment) -> NodeSLResourceSnapshot:
    rows = tuple(env.public_run_contexts())
    if len(rows) != 1 or not isinstance(rows[0], Sequence) or len(rows[0]) != 2:
        raise NodeSLError("Node-SL requires one slot-aligned public run context")
    slot, context = rows[0]
    if _integer(slot, "public context slot", minimum=0) != 0:
        raise NodeSLError("Node-SL public run context belongs to another slot")
    potions = tuple(getattr(context, "potion_ids"))
    if any(potion is not None and (not isinstance(potion, str) or not potion) for potion in potions):
        raise NodeSLError("Node-SL public potion identities are malformed")
    return NodeSLResourceSnapshot(
        act=_integer(getattr(context, "act"), "public act", minimum=0),
        floor=_integer(getattr(context, "floor"), "public floor", minimum=0),
        hp=_integer(getattr(context, "hp"), "public hp", minimum=0),
        max_hp=_integer(getattr(context, "max_hp"), "public max hp", minimum=1),
        gold=_integer(getattr(context, "gold"), "public gold", minimum=0),
        potion_ids=potions,
        is_combat=bool(getattr(context, "is_combat")),
        is_terminal=bool(getattr(context, "is_terminal")),
    )


def _terminal_reward(step: Mapping[str, object]) -> int | None:
    terminated = _bool_column(step, "terminated")
    if len(terminated) != 1:
        raise NodeSLError("Node-SL step must contain exactly one transition")
    if not terminated[0]:
        return None
    rewards = _column(step, "terminal_reward")
    if len(rewards) != 1:
        raise NodeSLError("terminal Node-SL step must contain one terminal reward")
    reward = rewards[0]
    if reward < -1 or reward > 1:
        raise NodeSLError("terminal Node-SL reward must be within [-1, 1]")
    return reward


def _terminal_location(step: Mapping[str, object]) -> tuple[int, int]:
    acts = _column(step, "terminal_act")
    floors = _column(step, "terminal_floor")
    if len(acts) != 1 or len(floors) != 1:
        raise NodeSLError("terminal Node-SL step must contain one terminal location")
    return acts[0], floors[0]


def _execute_ordinal_path(env: NodeSLEnvironment, ordinals: Sequence[int]) -> None:
    normalized = tuple(_integer(value, "action ordinal", minimum=0) for value in ordinals)
    if not normalized:
        raise NodeSLError("Node-SL action path must not be empty")
    for ordinal in normalized:
        batch = env.decision_batch(semantic=True)
        key, count = _decision_identity(batch)
        if ordinal >= count:
            raise NodeSLError("Node-SL action path ordinal is no longer legal")
        env.choose([key.decision_token_id], [ordinal])
    if not env.ready:
        raise NodeSLError("Node-SL action path ended before symbolic selection completed")


def greedy_network_noncombat_actor(
    evaluator: NodeSLEvaluator,
    *,
    max_selection_steps: int = 64,
) -> NodeSLNonCombatActor:
    """Build the former network-greedy actor only when a caller names it explicitly."""

    if not callable(evaluator):
        raise NodeSLError("greedy non-combat actor requires an evaluator")
    limit = _integer(max_selection_steps, "max_selection_steps", minimum=1)

    def act(env: NodeSLEnvironment) -> None:
        _execute_greedy_network_action(env, evaluator, limit)

    return act


def privileged_node_sl_noncombat_actor(
    *,
    max_selection_steps: int = 64,
) -> NodeSLNonCombatActor:
    """Use the bridge's exact strategic owner without concealing coverage gaps."""

    limit = _integer(max_selection_steps, "max_selection_steps", minimum=1)

    def act(env: NodeSLEnvironment) -> None:
        for _ in range(limit):
            try:
                batch = env.decision_batch(
                    semantic=True,
                    privileged_node_sl_oracle=True,
                )
            except TypeError as error:
                raise NodeSLError(
                    "Node-SL environment does not expose the privileged strategic oracle"
                ) from error
            key, count = _decision_identity(batch)
            available = _bool_column(batch, "privileged_node_sl_oracle_available")
            paths = _ordinal_path_column(
                batch,
                "privileged_node_sl_oracle_ordinal_paths",
            )
            if len(available) != 1 or len(paths) != 1:
                raise NodeSLError("privileged Node-SL oracle columns are misaligned")
            if not available[0]:
                raise NodeSLError(
                    "privileged Node-SL oracle has no exact action for this non-combat prefix"
                )
            path = paths[0]
            if not path or path[0] >= count:
                raise NodeSLError("privileged Node-SL oracle selected an illegal ordinal path")
            _execute_ordinal_path(env, path)
            return
        raise NodeSLError("privileged Node-SL oracle did not finish symbolic selection")

    return act


def _execute_greedy_network_action(
    env: NodeSLEnvironment,
    evaluator: NodeSLEvaluator,
    max_selection_steps: int,
) -> None:
    for _ in range(max_selection_steps):
        batch = env.decision_batch(semantic=True)
        key, count = _decision_identity(batch)
        inference = _evaluate(evaluator, batch, count)
        ordinal = max(range(count), key=lambda index: (inference.priors[index], -index))
        env.choose([key.decision_token_id], [ordinal])
        if env.ready:
            return
    raise NodeSLError("network action did not finish symbolic selection")


def _column(mapping: Mapping[str, object], name: str) -> tuple[int, ...]:
    try:
        raw = mapping[name]
    except KeyError as error:
        raise NodeSLError(f"Node-SL payload is missing {name}") from error
    if isinstance(raw, (str, bytes)):
        raise NodeSLError(f"Node-SL {name} must be an integer sequence")
    try:
        return tuple(_integer(value, name) for value in raw)  # type: ignore[union-attr]
    except TypeError as error:
        raise NodeSLError(f"Node-SL {name} must be an integer sequence") from error


def _ordinal_path_column(
    mapping: Mapping[str, object],
    name: str,
) -> tuple[tuple[int, ...], ...]:
    try:
        raw = mapping[name]
    except KeyError as error:
        raise NodeSLError(f"Node-SL payload is missing {name}") from error
    if isinstance(raw, (str, bytes)):
        raise NodeSLError(f"Node-SL {name} must contain ordinal paths")
    try:
        return tuple(
            tuple(_integer(value, name, minimum=0) for value in path)
            for path in raw  # type: ignore[union-attr]
        )
    except TypeError as error:
        raise NodeSLError(f"Node-SL {name} must contain ordinal paths") from error


def _bool_column(mapping: Mapping[str, object], name: str) -> tuple[bool, ...]:
    try:
        raw = mapping[name]
    except KeyError as error:
        raise NodeSLError(f"Node-SL payload is missing {name}") from error
    if isinstance(raw, (str, bytes)):
        raise NodeSLError(f"Node-SL {name} must be a boolean sequence")
    try:
        values = tuple(raw)  # type: ignore[arg-type]
    except TypeError as error:
        raise NodeSLError(f"Node-SL {name} must be a boolean sequence") from error
    normalized = []
    for value in values:
        if isinstance(value, bool):
            normalized.append(value)
            continue
        # NumPy booleans intentionally have no ``operator.index`` conversion.
        if type(value).__module__ == "numpy" and type(value).__name__ == "bool":
            normalized.append(bool(value))
            continue
        raise NodeSLError(f"Node-SL {name} must contain only booleans")
    return tuple(normalized)


def _text_column(mapping: Mapping[str, object], name: str) -> tuple[str, ...]:
    try:
        raw = mapping[name]
    except KeyError as error:
        raise NodeSLError(f"Node-SL payload is missing {name}") from error
    if isinstance(raw, (str, bytes)):
        raise NodeSLError(f"Node-SL {name} must be a text sequence")
    try:
        values = tuple(raw)  # type: ignore[arg-type]
    except TypeError as error:
        raise NodeSLError(f"Node-SL {name} must be a text sequence") from error
    if any(not isinstance(value, str) or not value for value in values):
        raise NodeSLError(f"Node-SL {name} contains an invalid identity")
    return values


def _integer(value: object, name: str, minimum: int | None = None) -> int:
    if isinstance(value, bool):
        raise NodeSLError(f"{name} must not be bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise NodeSLError(f"{name} must be an integer") from error
    if minimum is not None and normalized < minimum:
        raise NodeSLError(f"{name} must be at least {minimum}")
    return normalized
