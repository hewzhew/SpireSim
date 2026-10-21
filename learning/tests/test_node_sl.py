from __future__ import annotations

from copy import deepcopy
from types import SimpleNamespace

import numpy as np
import pytest

from sts_learning.node_sl import (
    NodeSLCombatExample,
    NodeSLConfig,
    NodeSLEpisodeActor,
    NodeSLError,
    NodeSLImprover,
    NodeSLInference,
    finalize_node_sl_episode,
    greedy_network_noncombat_actor,
    privileged_node_sl_noncombat_actor,
    play_node_sl_combat,
    _public_context,
)


def _semantic_batch(*, state: str, phase: int, candidate_count: int) -> dict[str, object]:
    token_count = candidate_count + 1
    return {
        "slot_indices": np.asarray([0], dtype=np.uint64),
        "phase": np.asarray([phase], dtype=np.uint8),
        "candidate_counts": np.asarray([candidate_count], dtype=np.uint64),
        "candidate_row_splits": np.asarray([0, candidate_count], dtype=np.uint64),
        "decision_token_ids": ["combat-root-token" if state != "strategic" else "strategic-token"],
        "public_snapshot_ids": [f"snapshot-{state}"],
        "decision_steps": np.asarray([0 if state != "strategic" else 1], dtype=np.uint64),
        "semantic": {
            "schema_version": 2,
            "completeness": np.asarray([1], dtype=np.uint8),
            "token": {
                "row_splits": np.asarray([0, token_count], dtype=np.uint64),
                "kind": np.asarray([0] + [1] * candidate_count, dtype=np.uint16),
                "scope": np.arange(token_count, dtype=np.uint32),
            },
            "categorical": {
                "token_indices": np.arange(1, token_count, dtype=np.uint64),
                "field": np.ones(candidate_count, dtype=np.uint16),
                "value": np.arange(candidate_count, dtype=np.int64),
            },
            "scalar": {
                "token_indices": np.asarray([], dtype=np.uint64),
                "field": np.asarray([], dtype=np.uint16),
                "value": np.asarray([], dtype=np.float32),
            },
            "relation": {
                "source_token_indices": np.asarray([], dtype=np.uint64),
                "relation": np.asarray([], dtype=np.uint16),
                "target_token_indices": np.asarray([], dtype=np.uint64),
            },
            "candidate_token_indices": np.arange(1, token_count, dtype=np.uint64),
        },
    }


class _CombatNodeEnv:
    slot_count = 1

    def __init__(self) -> None:
        self.state = "root"
        self.pending: tuple[int, ...] = ()
        self.restore_count = 0

    @property
    def ready(self) -> bool:
        return self.state == "ready"

    def checkpoint_slots(self, slot_indices: list[int]) -> object:
        assert slot_indices == [0]
        return (self.state, self.pending)

    def restore_slots(self, slot_indices: list[int], checkpoints: object) -> None:
        assert slot_indices == [0]
        self.state, self.pending = checkpoints  # type: ignore[misc]
        self.restore_count += 1

    def decision_batch(self, *, semantic: bool = False) -> dict[str, object]:
        assert semantic
        if self.state == "root":
            return _semantic_batch(state="root", phase=0, candidate_count=2)
        if self.state == "selection":
            return _semantic_batch(state="selection", phase=1, candidate_count=2)
        if self.state == "strategic":
            return _semantic_batch(state="strategic", phase=0, candidate_count=2)
        raise AssertionError(f"state {self.state} has no decision")

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None:
        assert len(decision_token_ids) == len(ordinals) == 1
        ordinal = ordinals[0]
        if self.state == "root":
            assert decision_token_ids == ["combat-root-token"]
            if ordinal == 0:
                self.pending = (0,)
                self.state = "ready"
            else:
                self.pending = (1,)
                self.state = "selection"
            return
        if self.state == "selection":
            assert decision_token_ids == ["combat-root-token"]
            self.pending += (ordinal,)
            self.state = "ready"
            return
        raise AssertionError(f"cannot choose in {self.state}")

    def step(self) -> dict[str, object]:
        assert self.ready
        terminal = self.pending == (0,) or self.pending == (1, 0)
        self.state = "terminal" if terminal else "strategic"
        self.pending = ()
        return {
            "slot_indices": np.asarray([0], dtype=np.uint64),
            "terminated": np.asarray([terminal], dtype=np.bool_),
            "terminal_reward": np.asarray([-1] if terminal else [], dtype=np.int8),
            "terminal_act": np.asarray([1] if terminal else [], dtype=np.uint8),
            "terminal_floor": np.asarray([3] if terminal else [], dtype=np.int32),
        }

    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        terminal = self.state == "terminal"
        combat = self.state in {"root", "selection", "ready"}
        return [
            (
                0,
                SimpleNamespace(
                    act=1,
                    floor=3,
                    hp=0 if terminal else 70 if self.state == "strategic" else 75,
                    max_hp=80,
                    gold=20,
                    potion_ids=("FirePotion",),
                    is_combat=combat,
                    is_terminal=terminal,
                ),
            )
        ]


def _evaluator(batch: dict[str, object]) -> NodeSLInference:
    snapshot = batch["public_snapshot_ids"]
    assert isinstance(snapshot, list)
    if snapshot == ["snapshot-root"]:
        return NodeSLInference((0.5, 0.5), 0.0)
    if snapshot == ["snapshot-selection"]:
        return NodeSLInference((0.2, 0.8), 0.0)
    if snapshot == ["snapshot-strategic"]:
        return NodeSLInference((0.5, 0.5), 0.8)
    raise AssertionError(snapshot)


@pytest.mark.parametrize("leaf_evaluation", ["rollout", "value"])
def test_combat_objective_uses_actual_exit_hp_and_never_evaluates_reward_page(
    leaf_evaluation: str,
) -> None:
    env = _CombatNodeEnv()

    def combat_only(batch):
        assert batch["public_snapshot_ids"] != ["snapshot-strategic"]
        return _evaluator(batch)

    config = NodeSLConfig(simulations=24, objective="combat_hp", leaf_evaluation=leaf_evaluation)
    checkpoint = env.checkpoint_slots([0])
    record = NodeSLImprover(combat_only, config).improve(env)
    assert env.checkpoint_slots([0]) == checkpoint
    assert record.best_completed_line is not None
    assert record.best_completed_line.combat_return == pytest.approx(70 / 80)
    assert record.best_completed_line.continuation_value is None
    result = play_node_sl_combat(env, combat_only, config)
    assert result.completed and result.end.hp == 70
    assert result.training_examples
    assert all(example.combat_return == pytest.approx(70 / 80)
               for example in result.training_examples)
    with pytest.raises(NodeSLError, match="completed battle"):
        NodeSLCombatExample(record.policy_target, result.start)


def test_combat_death_supplies_actual_negative_outcome() -> None:
    env = _CombatNodeEnv()
    result = play_node_sl_combat(env, lambda batch: NodeSLInference((1.0, 0.0), 0.0))
    assert result.completed and result.end.hp == 0
    target = NodeSLImprover(_evaluator, NodeSLConfig(simulations=8)).improve(_CombatNodeEnv())
    assert NodeSLCombatExample(target.policy_target, _public_context(env)).combat_return == -1.0


@pytest.mark.parametrize("budget", [8, 256])
@pytest.mark.parametrize("exit_hps", [(64, 70), (70, 64), (70, 70)])
def test_resolved_root_uses_exact_outcomes_instead_of_prior_driven_visits(budget, exit_hps) -> None:
    # A positive favorite rollout must not suppress an untried action merely
    # because its initial Q=0 and the network gave it almost no prior mass.
    class SurvivingEnv(_CombatNodeEnv):
        def choose(self, decision_token_ids, ordinals):
            self.pending = tuple(ordinals)
            self.state = "ready"

        def step(self):
            self.exit_hp = exit_hps[self.pending[0]]
            self.state = "strategic"
            return {"terminated": [False]}

        def public_run_contexts(self):
            rows = super().public_run_contexts()
            if self.state == "strategic":
                rows[0][1].hp = self.exit_hp
            return rows

    config = NodeSLConfig(simulations=budget, objective="combat_hp")
    record = NodeSLImprover(
        lambda batch: NodeSLInference((0.99999, 0.00001), 0.8), config
    ).improve(SurvivingEnv())
    assert all(count > 0 for count in record.policy_target.visit_counts)
    best_hp = max(exit_hps)
    optimal_count = exit_hps.count(best_hp)
    assert exit_hps[record.recommended_action_ordinals[0]] == best_hp
    assert record.policy_target.probabilities == tuple(
        1 / optimal_count if hp == best_hp else 0.0 for hp in exit_hps
    )
    assert record.policy_target.solved_action_values == tuple(hp / 80 for hp in exit_hps)
    assert record.simulations == sum(record.policy_target.visit_counts) == 2
    assert record.completed_lines == 2
    assert record.best_completed_line.end.hp == best_hp


@pytest.mark.parametrize("leaf_evaluation", ["rollout", "value"])
def test_one_rollout_does_not_solve_a_branch_and_exact_children_propagate(leaf_evaluation):
    class SurvivingSelectionEnv(_CombatNodeEnv):
        def step(self):
            self.exit_hp = 70 if self.pending == (1, 1) else 64
            self.state = "strategic"
            return {"terminated": [False]}

        def public_run_contexts(self):
            rows = super().public_run_contexts()
            if self.state == "strategic":
                rows[0][1].hp = self.exit_hp
            return rows

    # The first rollout follows the mediocre route and the scalar critic is
    # overoptimistic. Neither is a proof of this branch's optimal outcome.
    evaluator = lambda batch: NodeSLInference((1.0, 0.0), 1.0)
    config = dict(objective="combat_hp", leaf_evaluation=leaf_evaluation)
    partial = NodeSLImprover(evaluator, NodeSLConfig(simulations=2, **config)).improve(
        SurvivingSelectionEnv()
    )
    assert partial.policy_target.solved_action_values is None
    complete = NodeSLImprover(evaluator, NodeSLConfig(simulations=64, **config)).improve(
        SurvivingSelectionEnv()
    )
    assert complete.recommended_action_ordinals == (1, 1)
    assert complete.simulations == 4
    assert len(complete.policy_targets) == 2
    for target in complete.policy_targets:
        assert target.solved_action_values == (64 / 80, 70 / 80)
        assert target.probabilities == (0.0, 1.0)
    assert complete.best_completed_line.end.hp == 70


class _FiniteCombatEnv(_CombatNodeEnv):
    """A small fully enumerable action tree; leaves are actual exit HP."""

    def __init__(self, graph, *, start_hp=75):
        super().__init__()
        self.graph = graph
        self.start_hp = start_hp
        self.transitions = 0

    def decision_batch(self, *, semantic=False):
        assert semantic and self.state in self.graph
        return _semantic_batch(state=self.state, phase=0, candidate_count=len(self.graph[self.state]))

    def choose(self, decision_token_ids, ordinals):
        assert decision_token_ids == ["combat-root-token"]
        assert len(ordinals) == 1 and 0 <= ordinals[0] < len(self.graph[self.state])
        self.pending = (self.state, ordinals[0])
        self.state = "ready"

    def step(self):
        assert self.ready
        self.transitions += 1
        state, ordinal = self.pending
        child = self.graph[state][ordinal]
        self.pending = ()
        if isinstance(child, str):
            self.state = child
            return {"terminated": [False]}
        self.exit_hp = child
        self.state = "strategic" if child else "terminal"
        return {"terminated": [not child], "terminal_reward": [-1] if not child else []}

    def public_run_contexts(self):
        combat = self.state in self.graph or self.ready
        return [(0, SimpleNamespace(
            act=1, floor=3, hp=self.start_hp if combat else self.exit_hp, max_hp=80,
            gold=20, potion_ids=(), is_combat=combat, is_terminal=self.state == "terminal",
        ))]


def test_exploring_a_branch_does_not_make_it_the_training_answer():
    env = _FiniteCombatEnv({"root": (70, "tail"), "tail": (64, 60)})
    record = NodeSLImprover(
        lambda batch: NodeSLInference((1.0, 0.0), 0.0),
        NodeSLConfig(simulations=3, objective="combat_hp"),
    ).improve(env)
    # The inferior branch received more work because the superior one was
    # already resolved. Allocation counts must not become preference labels.
    assert record.policy_target.visit_counts == (1, 2)
    assert record.policy_target.solved_action_values is None
    assert record.policy_target.completed_action_values == (70 / 80, 64 / 80)
    assert record.policy_target.probabilities == (1.0, 0.0)
    assert record.recommended_action_ordinals == (0,)
    assert env.state == "root"


def test_rollouts_vary_near_tied_actions_without_changing_tree_order_or_checkpoint():
    # Both priors are plausible, but greedy continuation always takes death.
    # Search randomness must explore both while preserving exact game state.
    outcomes = []
    for seed in range(8):
        repeated = []
        for _ in range(2):
            env = _FiniteCombatEnv({"root": ("tail", 0), "tail": (0, 70)})
            result = NodeSLImprover(
                lambda batch: NodeSLInference((0.51, 0.49), 0.0),
                NodeSLConfig(simulations=1, objective="combat_hp", rollout_seed=seed),
            ).improve(env)
            line = result.best_completed_line
            assert line.action_ordinal_paths[0] == (0,)
            assert result.policy_target.visit_counts == (1, 0)
            assert env.state == "root" and env.pending == ()
            repeated.append((line.action_ordinal_paths, line.end.hp))
        assert repeated[0] == repeated[1]
        outcomes.append(repeated[0][1])
    assert set(outcomes) == {0, 70}


def test_incumbent_survives_research_with_one_trial_and_a_wrong_prior():
    env = _FiniteCombatEnv({"root": ("tail", 64), "tail": (0, 70)})
    evaluator = lambda batch: NodeSLInference((1.0, 0.0), 0.0)
    record = NodeSLImprover(evaluator, NodeSLConfig(
        simulations=8, objective="combat_hp",
    )).improve(env)
    assert record.best_completed_line.action_ordinal_paths == ((0,), (1,))
    env.choose(["combat-root-token"], [0])
    env.step()
    checkpoint = env.checkpoint_slots([0])
    suffix = record.best_completed_line.action_ordinal_paths[1:]
    result = NodeSLImprover(evaluator, NodeSLConfig(
        simulations=1, objective="combat_hp",
    )).improve(env, incumbent_action_paths=suffix)
    assert result.simulations == 1
    assert result.recommended_action_ordinals == (1,)
    assert result.best_completed_line.end.hp == 70
    assert result.policy_target.probabilities == (0.0, 1.0)
    assert env.checkpoint_slots([0]) == checkpoint
    with pytest.raises(NodeSLError, match="surviving"):
        NodeSLImprover(evaluator, NodeSLConfig(objective="combat_hp")).improve(
            env, incumbent_action_paths=((0,),),
        )
    assert env.checkpoint_slots([0]) == checkpoint


def test_rollout_selection_can_train_without_a_tree_visit():
    # The winning target selection was only seen inside a completed rollout.
    # Its evidence is the actual completion, not a fabricated tree visit.
    record = NodeSLImprover(
        lambda batch: NodeSLInference((0.0, 1.0), 0.0),
        NodeSLConfig(simulations=1, objective="combat_hp"),
    ).improve(_CombatNodeEnv())
    assert record.recommended_action_ordinals == (1, 1)
    selection = record.policy_targets[1]
    assert selection.visit_counts == (0, 0)
    assert selection.completed_action_values == (None, 70 / 80)
    assert selection.probabilities == (0.0, 1.0)
    assert selection.solved_action_values is None


@pytest.mark.parametrize("budget", [1, 2])
def test_failed_trial_does_not_force_death_while_continuations_are_open(budget):
    env = _FiniteCombatEnv({"root": (0, "tail"), "tail": (0, 70)})
    record = NodeSLImprover(
        lambda batch: NodeSLInference((1.0, 0.0), 0.9),
        NodeSLConfig(simulations=budget, max_atomic_actions=1, objective="combat_hp"),
    ).improve(env)
    assert record.best_completed_line.end.hp == 0
    assert record.recommended_action_ordinals == (1,)
    assert record.policy_target.probabilities == (0.0, 1.0)
    assert record.policy_target.completed_action_values is None
    assert record.policy_target.solved_action_values is None


def test_draw_then_reaper_improves_over_an_immediate_strike_kill():
    # Exhaustive Rust-engine suffix: 5 HP, 2 energy, Jaw Worm at 4 HP with
    # lethal intent; hand Strike/Battle Trance; draw Reaper/Wound/Wound.
    # The engine's action order is EndTurn, Strike, then BattleTrance/Reaper.
    env = _FiniteCombatEnv({"root": (0, 5, "drawn"), "drawn": (0, 5, 9)}, start_hp=5)
    result = play_node_sl_combat(
        env,
        lambda batch: NodeSLInference((0.00001, 0.99999, 0.00001), 0.1),
        NodeSLConfig(simulations=8, objective="combat_hp"),
    )
    assert result.completed and result.end.hp == 9
    assert [record.recommended_action_ordinals for record in result.searches] == [(2,), (2,)]
    assert all(record.policy_target.probabilities == (0.0, 0.0, 1.0)
               for record in result.searches)


def test_combat_action_limit_keeps_live_state_without_a_death_label() -> None:
    class ContinuingEnv(_CombatNodeEnv):
        def step(self):
            self.state, self.pending = "root", ()
            return {"terminated": [False]}

    env = ContinuingEnv()
    result = play_node_sl_combat(
        env, _evaluator,
        NodeSLConfig(simulations=2, max_atomic_actions=2, objective="combat_hp"),
        max_atomic_actions=1,
    )
    assert not result.completed
    assert result.end.is_combat and result.end.hp == 75
    assert result.atomic_actions == 1 and result.searches
    assert result.searches[0].simulations == 2
    assert result.training_examples == ()


def test_combat_node_sl_improves_a_complete_symbolic_action_and_restores_root() -> None:
    env = _CombatNodeEnv()
    improver = NodeSLImprover(
        _evaluator,
        NodeSLConfig(simulations=24, exploration=1.25),
    )

    result = improver.improve(env)

    assert result.recommended_action_ordinals == (1, 1)
    assert result.policy_target.visit_counts[1] > result.policy_target.visit_counts[0]
    assert sum(result.policy_target.probabilities) == pytest.approx(1.0)
    assert result.expanded_nodes == 2
    assert len(result.policy_targets) == 2
    assert result.policy_targets[1].state_key.public_snapshot_id == "snapshot-selection"
    assert result.completed_lines > 0
    assert result.best_completed_line is not None
    assert result.best_completed_line.exited_combat
    assert result.best_completed_line.action_ordinal_paths == ((1, 1),)
    assert result.best_completed_line.start.hp == 75
    assert result.best_completed_line.end.hp == 70
    assert result.best_completed_line.continuation_value == pytest.approx(0.8)
    assert result.policy_target.solved_action_values is None
    assert env.state == "root"
    assert env.restore_count == 26


def test_symbolic_selection_is_not_merged_with_its_unchanged_decision_token() -> None:
    env = _CombatNodeEnv()
    result = NodeSLImprover(
        _evaluator,
        NodeSLConfig(simulations=8, exploration=1.0),
    ).improve(env)

    assert result.policy_target.state_key.decision_token_id == "combat-root-token"
    assert result.policy_target.state_key.public_snapshot_id == "snapshot-root"
    assert result.expanded_nodes == 2


def test_search_record_joins_to_the_eventual_complete_run_outcome() -> None:
    record = NodeSLImprover(
        _evaluator,
        NodeSLConfig(simulations=8, exploration=1.0),
    ).improve(_CombatNodeEnv())

    examples = finalize_node_sl_episode(
        [record],
        terminal_reward=1,
        terminal_act=4,
        terminal_floor=57,
    )

    assert len(examples) == len(record.policy_targets) == 2
    assert all(example.run_return == 1.0 for example in examples)
    assert all(example.terminal_act == 4 for example in examples)
    assert all(example.terminal_floor == 57 for example in examples)
    assert all(example.node_line is record.best_completed_line for example in examples)


def test_node_sl_refuses_a_non_combat_entry() -> None:
    env = _CombatNodeEnv()
    env.state = "strategic"

    with pytest.raises(NodeSLError, match="live combat"):
        NodeSLImprover(_evaluator).improve(env)


class _CompleteEpisodeEnv(_CombatNodeEnv):
    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None:
        if self.state == "strategic":
            assert decision_token_ids == ["strategic-token"]
            assert len(ordinals) == 1
            self.pending = (2, ordinals[0])
            self.state = "ready"
            return
        super().choose(decision_token_ids, ordinals)

    def step(self) -> dict[str, object]:
        if self.pending == (1, 1):
            self.state = "strategic"
            self.pending = ()
            return {
                "slot_indices": np.asarray([0], dtype=np.uint64),
                "terminated": np.asarray([False], dtype=np.bool_),
                "terminal_reward": np.asarray([], dtype=np.int8),
                "terminal_act": np.asarray([], dtype=np.uint8),
                "terminal_floor": np.asarray([], dtype=np.int32),
            }
        if self.state == "ready" and self.pending[:1] == (2,):
            self.state = "terminal"
            self.pending = ()
            return {
                "slot_indices": np.asarray([0], dtype=np.uint64),
                "terminated": np.asarray([True], dtype=np.bool_),
                "terminal_reward": np.asarray([1], dtype=np.int8),
                "terminal_act": np.asarray([4], dtype=np.uint8),
                "terminal_floor": np.asarray([57], dtype=np.int32),
            }
        return super().step()


def test_episode_actor_executes_search_then_network_and_emits_training_examples() -> None:
    result = NodeSLEpisodeActor(
        _evaluator,
        greedy_network_noncombat_actor(_evaluator),
        NodeSLConfig(simulations=16, exploration=1.0),
        max_episode_atomic_actions=4,
    ).run(_CompleteEpisodeEnv())

    assert result.atomic_actions == 2
    assert result.terminal_reward == 1
    assert result.terminal_act == 4
    assert result.terminal_floor == 57
    assert len(result.searches) == 1
    assert len(result.training_examples) == len(result.searches[0].policy_targets)
    assert len(result.value_targets) == 2
    assert result.searches[0].recommended_action_ordinals == (1, 1)
    assert result.training_examples[0].run_return == 1.0
    assert [target.state_resources.is_combat for target in result.value_targets] == [
        True,
        False,
    ]
    assert all(target.terminal_resources.is_terminal for target in result.value_targets)
    assert result.value_targets[0].terminal_resources.hp == 0


def test_episode_actor_requires_an_explicit_noncombat_actor() -> None:
    with pytest.raises(NodeSLError, match="explicit non-combat actor"):
        NodeSLEpisodeActor(  # type: ignore[arg-type]
            _evaluator,
            None,
            NodeSLConfig(simulations=4, exploration=1.0),
        )


class _PrivilegedRunSelectionEnv:
    slot_count = 1

    def __init__(self) -> None:
        self.position = 0
        self.chosen: list[int] = []

    @property
    def ready(self) -> bool:
        return self.position == 4

    def decision_batch(
        self,
        *,
        semantic: bool = False,
        privileged_node_sl_oracle: bool = False,
    ) -> dict[str, object]:
        assert semantic
        counts = (1, 3, 2, 1)
        if self.ready:
            raise AssertionError("ready selection has no decision")
        batch = _semantic_batch(
            state="strategic",
            phase=0 if self.position == 0 else 2,
            candidate_count=counts[self.position],
        )
        if privileged_node_sl_oracle:
            batch["privileged_node_sl_oracle_available"] = np.asarray(
                [True], dtype=np.bool_
            )
            batch["privileged_node_sl_oracle_ordinal_paths"] = [[0, 2, 1, 0]]
        return batch

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None:
        assert decision_token_ids == ["strategic-token"]
        assert len(ordinals) == 1
        expected = (0, 2, 1, 0)
        assert ordinals[0] == expected[self.position]
        self.chosen.append(ordinals[0])
        self.position += 1


def test_privileged_noncombat_actor_revalidates_each_ordinal_in_complete_path() -> None:
    env = _PrivilegedRunSelectionEnv()

    privileged_node_sl_noncombat_actor()(env)

    assert env.ready
    assert env.chosen == [0, 2, 1, 0]


def single_row_semantic_batch() -> dict[str, object]:
    return deepcopy(_semantic_batch(state="root", phase=0, candidate_count=2))
