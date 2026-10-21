"""Held-out combat execution with no experience or training owners."""

from __future__ import annotations

import json
import operator
from collections import Counter
from collections.abc import Sequence
from dataclasses import dataclass

from .combat_outcomes import (
    CombatGroupOutcomeAccumulator,
    CombatOutcomeError,
    CombatTerminalOutcome,
    CombatTerminalStepBatch,
    CompletedCombatGroup,
)
from .combat_potion_lane import (
    CombatPotionLane,
    CombatPotionLaneRootSource,
    normalize_combat_potion_slots,
)
from .policy import BehaviorManifestId
from .torch_behavior import (
    FrozenCombatGreedyTorchPolicy,
    FrozenGreedyTorchPolicy,
    FrozenReplicateCategoricalTorchPolicy,
)


CombatEvaluationPolicy = (
    FrozenReplicateCategoricalTorchPolicy
    | FrozenCombatGreedyTorchPolicy
    | FrozenGreedyTorchPolicy
)


class CombatEvaluationError(RuntimeError):
    """A held-out combat evaluation is malformed or incomplete."""


@dataclass(frozen=True)
class CombatEvaluationLimits:
    """Hard execution bounds for one held-out root."""

    max_model_rounds: int = 4_096
    max_transitions: int = 4_096

    def __post_init__(self) -> None:
        for name in ("max_model_rounds", "max_transitions"):
            object.__setattr__(
                self,
                name,
                _positive_integer(getattr(self, name), name),
            )


@dataclass(frozen=True)
class CombatObservedResourceFrontier:
    """Pareto order over terminal resource facts that the evaluator observes.

    This is deliberately not continuation value. Exact potion identities are
    compared as multisets; unlike identities and HP/potion tradeoffs remain
    incomparable.
    """

    replicate_count: int
    winning_replicate_indices: tuple[int, ...]
    frontier_replicate_indices: tuple[int, ...]
    dominated_replicate_indices: tuple[int, ...]
    dominators_by_replicate: tuple[tuple[int, ...], ...]
    strict_order_pair_count: int
    equivalent_pair_count: int
    incomparable_pair_count: int

    def __post_init__(self) -> None:
        replicate_count = _positive_integer(
            self.replicate_count,
            "replicate_count",
        )
        winning = _replicate_indices(
            self.winning_replicate_indices,
            replicate_count,
            "winning_replicate_indices",
        )
        frontier = _replicate_indices(
            self.frontier_replicate_indices,
            replicate_count,
            "frontier_replicate_indices",
        )
        dominated = _replicate_indices(
            self.dominated_replicate_indices,
            replicate_count,
            "dominated_replicate_indices",
        )
        if (
            set(frontier) & set(dominated)
            or (set(frontier) | set(dominated)) != set(winning)
        ):
            raise CombatEvaluationError(
                "observed resource frontier must partition winning replicates"
            )
        dominators = tuple(
            _replicate_indices(row, replicate_count, "dominators_by_replicate")
            for row in self.dominators_by_replicate
        )
        if len(dominators) != replicate_count:
            raise CombatEvaluationError(
                "observed resource dominators must align to every replicate"
            )
        for replicate_index, row in enumerate(dominators):
            if replicate_index in row or not set(row) <= set(winning):
                raise CombatEvaluationError(
                    "observed resource dominators are invalid"
                )
            if (replicate_index in dominated) != bool(row):
                raise CombatEvaluationError(
                    "observed resource dominance partition is inconsistent"
                )
        pair_names = (
            "strict_order_pair_count",
            "equivalent_pair_count",
            "incomparable_pair_count",
        )
        pair_counts = tuple(
            _nonnegative_integer(getattr(self, name), name)
            for name in pair_names
        )
        if pair_counts[0] != sum(len(row) for row in dominators):
            raise CombatEvaluationError(
                "observed resource strict pairs disagree with dominators"
            )
        if sum(pair_counts) != len(winning) * (len(winning) - 1) // 2:
            raise CombatEvaluationError(
                "observed resource pair counts do not cover winning pairs"
            )
        object.__setattr__(self, "replicate_count", replicate_count)
        object.__setattr__(self, "winning_replicate_indices", winning)
        object.__setattr__(self, "frontier_replicate_indices", frontier)
        object.__setattr__(self, "dominated_replicate_indices", dominated)
        object.__setattr__(self, "dominators_by_replicate", dominators)
        for name, value in zip(pair_names, pair_counts, strict=True):
            object.__setattr__(self, name, value)


def combat_observed_resource_frontier(
    outcomes: Sequence[CombatTerminalOutcome],
) -> CombatObservedResourceFrontier:
    """Return a no-exchange-rate Pareto order over winning terminal facts."""

    normalized = tuple(outcomes)
    if not normalized or not all(
        isinstance(outcome, CombatTerminalOutcome) for outcome in normalized
    ):
        raise CombatEvaluationError(
            "observed resource frontier requires typed terminal outcomes"
        )
    if tuple(outcome.replicate_index for outcome in normalized) != tuple(
        range(len(normalized))
    ):
        raise CombatEvaluationError(
            "observed resource frontier requires contiguous ordered replicates"
        )
    winning = tuple(outcome.replicate_index for outcome in normalized if outcome.won)
    dominators: list[list[int]] = [[] for _ in normalized]
    strict_pairs = 0
    equivalent_pairs = 0
    incomparable_pairs = 0
    for left_offset, left_index in enumerate(winning):
        for right_index in winning[left_offset + 1 :]:
            relation = _observed_resource_relation(
                normalized[left_index],
                normalized[right_index],
            )
            if relation is None:
                incomparable_pairs += 1
            elif relation > 0:
                dominators[right_index].append(left_index)
                strict_pairs += 1
            elif relation < 0:
                dominators[left_index].append(right_index)
                strict_pairs += 1
            elif relation == 0:
                equivalent_pairs += 1
    dominated = tuple(index for index in winning if dominators[index])
    frontier = tuple(index for index in winning if not dominators[index])
    return CombatObservedResourceFrontier(
        replicate_count=len(normalized),
        winning_replicate_indices=winning,
        frontier_replicate_indices=frontier,
        dominated_replicate_indices=dominated,
        dominators_by_replicate=tuple(tuple(row) for row in dominators),
        strict_order_pair_count=strict_pairs,
        equivalent_pair_count=equivalent_pairs,
        incomparable_pair_count=incomparable_pairs,
    )


def _observed_resource_relation(
    left: CombatTerminalOutcome,
    right: CombatTerminalOutcome,
) -> int | None:
    left_potions = Counter(
        potion for potion in left.final_potion_ids if potion is not None
    )
    right_potions = Counter(
        potion for potion in right.final_potion_ids if potion is not None
    )
    left_no_worse = (
        left.final_hp >= right.final_hp
        and left.final_max_hp >= right.final_max_hp
        and left.final_gold >= right.final_gold
        and not (right_potions - left_potions)
    )
    right_no_worse = (
        right.final_hp >= left.final_hp
        and right.final_max_hp >= left.final_max_hp
        and right.final_gold >= left.final_gold
        and not (left_potions - right_potions)
    )
    if left_no_worse and right_no_worse:
        return 0
    if left_no_worse:
        return 1
    if right_no_worse:
        return -1
    return None


@dataclass(frozen=True)
class CombatEvaluationRootContext:
    """Public run context needed to interpret one combat resource outcome."""

    seed: int
    encounter_id: str
    monster_ids: tuple[str, ...]
    act: int
    floor: int
    ascension_level: int
    turn: int
    is_boss_fight: bool
    is_elite_fight: bool
    monster_count: int
    living_monster_count: int
    potion_slot_count: int
    filled_potion_count: int
    usable_potion_count: int
    master_deck_card_count: int
    relic_count: int
    hand_card_count: int
    hp: int
    max_hp: int
    gold: int
    potion_ids: tuple[str | None, ...]

    @classmethod
    def from_environment(
        cls,
        env: object,
        public_context: object,
    ) -> CombatEvaluationRootContext:
        try:
            context = env.root_context
            public_potions = tuple(public_context.potion_ids)
            root_potions = tuple(env.root_potion_ids)
            if not public_context.is_combat:
                raise CombatEvaluationError(
                    "combat evaluation root context is not a combat boundary"
                )
            if (
                public_context.act != context.act
                or public_context.floor != context.floor
                or public_context.hp != context.hp
                or public_context.max_hp != context.max_hp
                or public_context.gold != env.root_gold
                or public_potions != root_potions
            ):
                raise CombatEvaluationError(
                    "combat evaluation public context disagrees with exact root"
                )
            return cls(
                seed=public_context.seed,
                encounter_id=public_context.encounter_id,
                monster_ids=tuple(public_context.monster_ids),
                act=context.act,
                floor=context.floor,
                ascension_level=context.ascension_level,
                turn=context.turn,
                is_boss_fight=context.is_boss_fight,
                is_elite_fight=context.is_elite_fight,
                monster_count=context.monster_count,
                living_monster_count=context.living_monster_count,
                potion_slot_count=context.potion_slot_count,
                filled_potion_count=context.filled_potion_count,
                usable_potion_count=context.usable_potion_count,
                master_deck_card_count=context.master_deck_card_count,
                relic_count=context.relic_count,
                hand_card_count=context.hand_card_count,
                hp=context.hp,
                max_hp=context.max_hp,
                gold=env.root_gold,
                potion_ids=root_potions,
            )
        except AttributeError as error:
            raise CombatEvaluationError(
                "combat evaluation source omitted exact root context"
            ) from error

    def __post_init__(self) -> None:
        for name in (
            "seed",
            "act",
            "floor",
            "ascension_level",
            "turn",
            "monster_count",
            "living_monster_count",
            "potion_slot_count",
            "filled_potion_count",
            "usable_potion_count",
            "master_deck_card_count",
            "relic_count",
            "hand_card_count",
            "hp",
            "max_hp",
            "gold",
        ):
            object.__setattr__(
                self,
                name,
                _nonnegative_integer(getattr(self, name), name),
            )
        for name in ("is_boss_fight", "is_elite_fight"):
            if not isinstance(getattr(self, name), bool):
                raise CombatEvaluationError(f"{name} must be boolean")
        if self.hp == 0 or self.max_hp == 0 or self.hp > self.max_hp:
            raise CombatEvaluationError("root hp must be in 1..max_hp")
        if not isinstance(self.encounter_id, str) or not self.encounter_id:
            raise CombatEvaluationError(
                "combat evaluation root requires an encounter identity"
            )
        monster_ids = tuple(self.monster_ids)
        if not monster_ids or not all(
            isinstance(monster, str) and monster for monster in monster_ids
        ):
            raise CombatEvaluationError(
                "combat evaluation root requires monster identities"
            )
        if len(monster_ids) != self.monster_count:
            raise CombatEvaluationError(
                "combat evaluation monster identities do not match monster_count"
            )
        potion_ids = _potion_ids(self.potion_ids, "root potion_ids")
        if len(potion_ids) != self.potion_slot_count:
            raise CombatEvaluationError(
                "root potion ids do not match potion_slot_count"
            )
        if sum(potion is not None for potion in potion_ids) != self.filled_potion_count:
            raise CombatEvaluationError(
                "root potion ids do not match filled_potion_count"
            )
        object.__setattr__(self, "potion_ids", potion_ids)
        object.__setattr__(self, "monster_ids", monster_ids)


@dataclass(frozen=True)
class CombatEvaluationRootResult:
    """Terminal facts for every replicate of one exact held-out root."""

    group: CompletedCombatGroup
    context: CombatEvaluationRootContext
    model_rounds: int
    transitions: int
    decision_traces: tuple[dict[str, object], ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.group, CompletedCombatGroup):
            raise CombatEvaluationError(
                "combat evaluation root requires a completed combat group"
            )
        if not isinstance(self.context, CombatEvaluationRootContext):
            raise CombatEvaluationError(
                "combat evaluation root requires typed public context"
            )
        if self.context.hp != self.group.outcomes[0].start_hp:
            raise CombatEvaluationError(
                "combat evaluation context and outcome start_hp disagree"
            )
        if any(
            len(outcome.final_potion_ids) != self.context.potion_slot_count
            for outcome in self.group.outcomes
        ):
            raise CombatEvaluationError(
                "combat evaluation outcome changed potion slot count"
            )
        object.__setattr__(
            self,
            "model_rounds",
            _nonnegative_integer(self.model_rounds, "model_rounds"),
        )
        object.__setattr__(
            self,
            "transitions",
            _positive_integer(self.transitions, "transitions"),
        )
        traces = tuple(self.decision_traces)
        if not all(isinstance(trace, dict) for trace in traces):
            raise CombatEvaluationError(
                "combat evaluation decision traces must be mappings"
            )
        object.__setattr__(self, "decision_traces", traces)

    @property
    def wins(self) -> int:
        return sum(outcome.won for outcome in self.group.outcomes)

    @property
    def losses(self) -> int:
        return len(self.group.outcomes) - self.wins


@dataclass(frozen=True)
class CombatHeldOutEvaluationResult:
    """One frozen manifest evaluated once across distinct exact roots."""

    behavior_manifest_id: BehaviorManifestId
    potion_lane: CombatPotionLane
    potion_slots: tuple[int, ...]
    roots: tuple[CombatEvaluationRootResult, ...]

    def __post_init__(self) -> None:
        if not isinstance(self.behavior_manifest_id, BehaviorManifestId):
            raise CombatEvaluationError(
                "combat evaluation requires a typed behavior manifest id"
            )
        if not isinstance(self.potion_lane, CombatPotionLane):
            raise CombatEvaluationError(
                "combat evaluation requires a typed potion lane"
            )
        potion_slots = normalize_combat_potion_slots(
            self.potion_lane,
            self.potion_slots,
        )
        roots = tuple(self.roots)
        if not roots or not all(
            isinstance(root, CombatEvaluationRootResult) for root in roots
        ):
            raise CombatEvaluationError(
                "combat evaluation requires typed root results"
            )
        identities = tuple(
            (root.group.root_id, root.group.exact_combat_state_hash)
            for root in roots
        )
        if len(set(identities)) != len(identities):
            raise CombatEvaluationError(
                "combat evaluation repeated an exact root"
            )
        object.__setattr__(self, "roots", roots)
        object.__setattr__(self, "potion_slots", potion_slots)

    @property
    def wins(self) -> int:
        return sum(root.wins for root in self.roots)

    @property
    def losses(self) -> int:
        return sum(root.losses for root in self.roots)


class CombatHeldOutEvaluator:
    """Evaluate fresh groups without retaining decisions or mutating behavior."""

    def __init__(
        self,
        source: object,
        *,
        slot_indices: Sequence[int],
        replicate_count: int,
        policies: Sequence[CombatEvaluationPolicy],
        max_roots: int,
        limits: CombatEvaluationLimits | None = None,
        potion_lane: CombatPotionLane = CombatPotionLane.ALL,
        potion_slots: Sequence[int] = (),
        trace_replicates_per_root: int = 0,
    ) -> None:
        if not callable(getattr(source, "combat_group", None)):
            raise CombatEvaluationError(
                "combat evaluation requires a combat-root source"
            )
        if not isinstance(potion_lane, CombatPotionLane):
            raise CombatEvaluationError(
                "combat evaluation requires a typed potion lane"
            )
        slots = tuple(
            _nonnegative_integer(slot, f"slot_indices[{index}]")
            for index, slot in enumerate(slot_indices)
        )
        if not slots or len(set(slots)) != len(slots):
            raise CombatEvaluationError(
                "combat evaluation requires distinct root slots"
            )
        root_bound = _positive_integer(max_roots, "max_roots")
        if len(slots) > root_bound:
            raise CombatEvaluationError(
                "combat evaluation roots exceed max_roots"
            )
        replicates = _positive_integer(replicate_count, "replicate_count")
        if replicates < 2:
            raise CombatEvaluationError(
                "combat evaluation requires at least two replicates"
            )
        traced_replicates = _nonnegative_integer(
            trace_replicates_per_root,
            "trace_replicates_per_root",
        )
        if traced_replicates > replicates:
            raise CombatEvaluationError(
                "combat evaluation cannot trace more replicates than it runs"
            )
        frozen_policies = tuple(policies)
        if len(frozen_policies) != len(slots) or not all(
            isinstance(
                policy,
                (
                    FrozenReplicateCategoricalTorchPolicy,
                    FrozenCombatGreedyTorchPolicy,
                    FrozenGreedyTorchPolicy,
                ),
            )
            for policy in frozen_policies
        ):
            raise CombatEvaluationError(
                "combat evaluation requires one frozen policy per root"
            )
        if any(
            isinstance(policy, FrozenCombatGreedyTorchPolicy)
            and not policy.is_combat_only
            for policy in frozen_policies
        ):
            raise CombatEvaluationError(
                "mixed combat evaluation policies must be combat-only"
            )
        greedy_modes = {
            isinstance(
                policy,
                (FrozenCombatGreedyTorchPolicy, FrozenGreedyTorchPolicy),
            )
            for policy in frozen_policies
        }
        if len(greedy_modes) != 1:
            raise CombatEvaluationError(
                "combat evaluation cannot mix sampled and greedy policies"
            )
        manifest_ids = {
            policy.behavior_manifest_id for policy in frozen_policies
        }
        if len(manifest_ids) != 1:
            raise CombatEvaluationError(
                "combat evaluation policies must share one frozen manifest"
            )
        sampled = not next(iter(greedy_modes))
        if sampled:
            sampled_policies = tuple(
                policy
                for policy in frozen_policies
                if isinstance(policy, FrozenReplicateCategoricalTorchPolicy)
            )
            if any(
                len(policy.generators) != replicates
                for policy in sampled_policies
            ):
                raise CombatEvaluationError(
                    "combat evaluation requires one policy RNG stream per replicate"
                )
            generators = tuple(
                generator
                for policy in sampled_policies
                for generator in policy.generators
            )
            if len({id(generator) for generator in generators}) != len(generators):
                raise CombatEvaluationError(
                    "combat evaluation requires independent policy RNG streams"
                )
        active_limits = CombatEvaluationLimits() if limits is None else limits
        if not isinstance(active_limits, CombatEvaluationLimits):
            raise CombatEvaluationError(
                "combat evaluation limits must be typed"
            )
        normalized_potion_slots = normalize_combat_potion_slots(
            potion_lane,
            potion_slots,
        )
        public_contexts = _public_root_contexts(source, slots)
        self.source = CombatPotionLaneRootSource(
            source,
            potion_lane,
            normalized_potion_slots,
        )
        self.slot_indices = slots
        self.replicate_count = replicates
        self.policies = frozen_policies
        self.max_roots = root_bound
        self.limits = active_limits
        self.potion_lane = potion_lane
        self.potion_slots = normalized_potion_slots
        self.trace_replicates_per_root = traced_replicates
        self.behavior_manifest_id = next(iter(manifest_ids))
        self.sampled = sampled
        self.public_contexts = public_contexts
        self._started = False

    def evaluate(self) -> CombatHeldOutEvaluationResult:
        """Consume every declared root exactly once under the frozen manifest."""

        if self._started:
            raise CombatEvaluationError(
                "combat held-out evaluation is single-use"
            )
        self._started = True
        generator_states = (
            tuple(
                tuple(
                    generator.get_state().clone()
                    for generator in policy.generators
                )
                for policy in self.policies
                if isinstance(policy, FrozenReplicateCategoricalTorchPolicy)
            )
            if self.sampled
            else ()
        )
        roots: list[CombatEvaluationRootResult] = []
        observed_identities: set[tuple[object, object]] = set()
        try:
            for slot_index, policy, public_context in zip(
                self.slot_indices,
                self.policies,
                self.public_contexts,
                strict=True,
            ):
                group = self.source.combat_group(
                    slot_index,
                    self.replicate_count,
                )
                if getattr(group, "replicate_count", None) != self.replicate_count:
                    raise CombatEvaluationError(
                        "combat evaluation source changed the replicate count"
                    )
                identity = (
                    getattr(group, "root_id", None),
                    getattr(group, "exact_combat_state_hash", None),
                )
                if identity in observed_identities:
                    raise CombatEvaluationError(
                        "combat evaluation source repeated an exact root"
                    )
                observed_identities.add(identity)
                root = _evaluate_group(
                    group,
                    policy,
                    self.limits,
                    public_context,
                    self.trace_replicates_per_root,
                )
                if self.potion_lane is CombatPotionLane.NEVER and any(
                    outcome.potions_used or outcome.potions_discarded
                    for outcome in root.group.outcomes
                ):
                    raise CombatEvaluationError(
                        "no-potion evaluation observed a potion action"
                    )
                roots.append(root)
        except Exception:
            if self.sampled:
                for policy, states in zip(
                    self.policies,
                    generator_states,
                    strict=True,
                ):
                    if not isinstance(
                        policy,
                        FrozenReplicateCategoricalTorchPolicy,
                    ):
                        raise AssertionError("sampled policy type changed")
                    for generator, state in zip(
                        policy.generators,
                        states,
                        strict=True,
                    ):
                        generator.set_state(state)
            raise
        return CombatHeldOutEvaluationResult(
            self.behavior_manifest_id,
            self.potion_lane,
            self.potion_slots,
            tuple(roots),
        )


def _evaluate_group(
    env: object,
    policy: CombatEvaluationPolicy,
    limits: CombatEvaluationLimits,
    public_context: object,
    trace_replicates_per_root: int,
) -> CombatEvaluationRootResult:
    if getattr(env, "terminal_count", None) != 0:
        raise CombatEvaluationError(
            "combat evaluation requires a fresh group"
        )
    context = CombatEvaluationRootContext.from_environment(env, public_context)
    try:
        outcomes = CombatGroupOutcomeAccumulator(
            root_id=env.root_id,
            exact_combat_state_hash=env.exact_combat_state_hash,
            replicate_count=env.replicate_count,
        )
    except (AttributeError, CombatOutcomeError) as error:
        raise CombatEvaluationError(
            "combat evaluation source exposed an invalid root identity"
        ) from error
    model_rounds = 0
    transitions = 0
    decision_traces: list[dict[str, object]] = []
    while outcomes.terminal_count < env.replicate_count:
        while not env.ready:
            if model_rounds >= limits.max_model_rounds:
                raise CombatEvaluationError(
                    "combat evaluation exceeded its model-round limit"
                )
            decision_batch = env.decision_batch(semantic=True)
            choice = policy.choose(decision_batch)
            if choice.behavior_manifest_id != policy.behavior_manifest_id:
                raise CombatEvaluationError(
                    "combat evaluation policy changed behavior manifest"
                )
            env.choose(list(choice.ordinals))
            if trace_replicates_per_root:
                decision_traces.extend(
                    _ready_action_traces(
                        env,
                        decision_batch,
                        choice,
                        model_round_index=model_rounds,
                        replicate_bound=trace_replicates_per_root,
                    )
                )
            model_rounds += 1
        if transitions >= limits.max_transitions:
            raise CombatEvaluationError(
                "combat evaluation exceeded its transition limit"
            )
        step = env.step()
        try:
            outcomes.record(
                CombatTerminalStepBatch.from_bridge_step(
                    step,
                    replicate_count=env.replicate_count,
                )
            )
        except CombatOutcomeError as error:
            raise CombatEvaluationError(str(error)) from error
        transitions += 1
        if outcomes.terminal_count != env.terminal_count:
            raise CombatEvaluationError(
                "bridge and caller combat terminal counts diverged"
            )
    return CombatEvaluationRootResult(
        group=outcomes.finish(),
        context=context,
        model_rounds=model_rounds,
        transitions=transitions,
        decision_traces=tuple(decision_traces),
    )


def _ready_action_traces(
    env: object,
    decision_batch: object,
    choice: object,
    *,
    model_round_index: int,
    replicate_bound: int,
) -> tuple[dict[str, object], ...]:
    provider = getattr(env, "ready_action_trace_json", None)
    if not callable(provider):
        raise CombatEvaluationError(
            "combat evaluation trace requested from an unsupported bridge"
        )
    try:
        raw_slots = decision_batch["slot_indices"]  # type: ignore[index]
        slots = tuple(
            _nonnegative_integer(slot, f"decision slot_indices[{index}]")
            for index, slot in enumerate(raw_slots)
        )
        ordinals = tuple(choice.ordinals)
        probabilities = tuple(choice.selection_probabilities)
    except (KeyError, TypeError, AttributeError) as error:
        raise CombatEvaluationError(
            "combat evaluation trace rows are unavailable"
        ) from error
    if not len(slots) == len(ordinals) == len(probabilities):
        raise CombatEvaluationError(
            "combat evaluation trace rows are misaligned"
        )

    traces: list[dict[str, object]] = []
    for replicate_index, ordinal, probability in zip(
        slots,
        ordinals,
        probabilities,
        strict=True,
    ):
        if replicate_index >= replicate_bound:
            continue
        raw_trace = provider(replicate_index)
        if raw_trace is None:
            continue
        if not isinstance(raw_trace, str):
            raise CombatEvaluationError(
                "combat evaluation bridge returned a non-text action trace"
            )
        try:
            trace = json.loads(raw_trace)
        except json.JSONDecodeError as error:
            raise CombatEvaluationError(
                "combat evaluation bridge returned invalid action trace JSON"
            ) from error
        if not isinstance(trace, dict):
            raise CombatEvaluationError(
                "combat evaluation bridge action trace must be a mapping"
            )
        if trace.get("replicate_index") != replicate_index:
            raise CombatEvaluationError(
                "combat evaluation bridge action trace changed replicate identity"
            )
        try:
            selected_ordinal = _nonnegative_integer(
                ordinal,
                "selected_ordinal",
            )
            selection_probability = probability.value
        except AttributeError as error:
            raise CombatEvaluationError(
                "combat evaluation trace probability is untyped"
            ) from error
        trace["model_round_index"] = model_round_index
        trace["selected_ordinal"] = selected_ordinal
        trace["selection_probability"] = selection_probability
        traces.append(trace)
    return tuple(traces)


def _public_root_contexts(
    source: object,
    slot_indices: tuple[int, ...],
) -> tuple[object, ...]:
    provider = getattr(source, "public_run_contexts", None)
    if not callable(provider):
        raise CombatEvaluationError(
            "combat evaluation source omitted public run contexts"
        )
    try:
        rows = tuple(provider())
    except Exception as error:
        raise CombatEvaluationError(
            "combat evaluation could not read public run contexts"
        ) from error
    by_slot: dict[int, object] = {}
    for raw_slot, context in rows:
        slot = _nonnegative_integer(raw_slot, "public context slot")
        if slot in by_slot:
            raise CombatEvaluationError(
                "combat evaluation source repeated a public context slot"
            )
        by_slot[slot] = context
    try:
        return tuple(by_slot[slot] for slot in slot_indices)
    except KeyError as error:
        raise CombatEvaluationError(
            "combat evaluation source omitted a selected root context"
        ) from error


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise CombatEvaluationError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatEvaluationError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatEvaluationError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CombatEvaluationError(f"{name} must be non-negative")
    return normalized


def _replicate_indices(
    values: object,
    replicate_count: int,
    name: str,
) -> tuple[int, ...]:
    try:
        normalized = tuple(
            _nonnegative_integer(value, name) for value in values
        )
    except TypeError as error:
        raise CombatEvaluationError(f"{name} must be iterable") from error
    if normalized != tuple(sorted(set(normalized))):
        raise CombatEvaluationError(f"{name} must be sorted and unique")
    if any(value >= replicate_count for value in normalized):
        raise CombatEvaluationError(f"{name} contains an out-of-range replicate")
    return normalized


def _potion_ids(value: object, name: str) -> tuple[str | None, ...]:
    try:
        potion_ids = tuple(value)
    except TypeError as error:
        raise CombatEvaluationError(f"{name} must be iterable") from error
    if not all(
        potion is None or isinstance(potion, str) and potion
        for potion in potion_ids
    ):
        raise CombatEvaluationError(
            f"{name} must contain non-empty ids or empty slots"
        )
    return potion_ids
