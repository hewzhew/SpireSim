"""Typed cross-combat resource evidence for held-out run evaluation."""

from __future__ import annotations

import operator
from collections import Counter
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Any

from .driver import BatchEnvironment


class RunResourceTraceError(ValueError):
    """A bridge context or cross-combat transition is malformed."""


@dataclass(frozen=True)
class RunPublicContext:
    slot_index: int
    boundary_kind: int
    is_combat: bool
    is_terminal: bool
    seed: int
    act: int
    floor: int
    hp: int
    max_hp: int
    gold: int
    potion_ids: tuple[str | None, ...]
    encounter_id: str | None
    monster_ids: tuple[str, ...]

    @classmethod
    def from_bridge_row(cls, row: object) -> RunPublicContext:
        if not isinstance(row, Sequence) or isinstance(row, (str, bytes)):
            raise RunResourceTraceError("public run context row must be a pair")
        if len(row) != 2:
            raise RunResourceTraceError("public run context row must contain two values")
        slot_index = _integer(row[0], "public run context slot", minimum=0)
        view = row[1]
        is_combat = _boolean(_attribute(view, "is_combat"), "is_combat")
        is_terminal = _boolean(_attribute(view, "is_terminal"), "is_terminal")
        if is_combat and is_terminal:
            raise RunResourceTraceError(
                "public run context cannot be combat and terminal together"
            )
        potion_ids = _potion_ids(_attribute(view, "potion_ids"))
        encounter_id = _optional_identity(_attribute(view, "encounter_id"))
        monster_ids = _monster_ids(_attribute(view, "monster_ids"))
        if is_combat != (encounter_id is not None):
            raise RunResourceTraceError(
                "public run context encounter identity disagrees with combat state"
            )
        if is_combat != bool(monster_ids):
            raise RunResourceTraceError(
                "public run context monster identities disagree with combat state"
            )
        hp = _integer(_attribute(view, "hp"), "hp", minimum=0)
        max_hp = _integer(_attribute(view, "max_hp"), "max_hp", minimum=1)
        if hp > max_hp:
            raise RunResourceTraceError("public run context HP exceeds max HP")
        return cls(
            slot_index=slot_index,
            boundary_kind=_integer(
                _attribute(view, "boundary_kind"),
                "boundary_kind",
                minimum=0,
            ),
            is_combat=is_combat,
            is_terminal=is_terminal,
            seed=_integer(_attribute(view, "seed"), "seed", minimum=0),
            act=_integer(_attribute(view, "act"), "act", minimum=0),
            floor=_integer(_attribute(view, "floor"), "floor", minimum=0),
            hp=hp,
            max_hp=max_hp,
            gold=_integer(_attribute(view, "gold"), "gold", minimum=0),
            potion_ids=potion_ids,
            encounter_id=encounter_id,
            monster_ids=monster_ids,
        )


@dataclass(frozen=True)
class RunCombatResourceTransition:
    start: RunPublicContext
    end: RunPublicContext
    terminal_reward: int | None

    def __post_init__(self) -> None:
        if not isinstance(self.start, RunPublicContext) or not isinstance(
            self.end,
            RunPublicContext,
        ):
            raise RunResourceTraceError("combat transition contexts must be typed")
        if not self.start.is_combat or self.end.is_combat:
            raise RunResourceTraceError(
                "combat transition must start in combat and end outside combat"
            )
        if self.start.slot_index != self.end.slot_index:
            raise RunResourceTraceError("combat transition changed slot")
        if self.start.seed != self.end.seed:
            raise RunResourceTraceError("combat transition changed episode seed")
        if self.end.is_terminal:
            if self.terminal_reward not in (-1, 1):
                raise RunResourceTraceError(
                    "terminal combat transition requires a terminal reward"
                )
        elif self.terminal_reward is not None:
            raise RunResourceTraceError(
                "continued-run combat transition cannot have terminal reward"
            )

    @property
    def hp_loss(self) -> int:
        return self.start.hp - self.end.hp


@dataclass(frozen=True)
class RunResourceTrace:
    combat_transitions: tuple[RunCombatResourceTransition, ...]
    episode_endpoints: tuple[RunEpisodeResourceEndpoint, ...]

    def __post_init__(self) -> None:
        if not all(
            isinstance(item, RunCombatResourceTransition)
            for item in self.combat_transitions
        ):
            raise RunResourceTraceError("resource trace transitions must be typed")
        if not all(
            isinstance(item, RunEpisodeResourceEndpoint)
            for item in self.episode_endpoints
        ):
            raise RunResourceTraceError("resource trace endpoints must be typed")
        seeds = tuple(endpoint.context.seed for endpoint in self.episode_endpoints)
        if len(set(seeds)) != len(seeds):
            raise RunResourceTraceError("resource trace repeats an episode seed")

    @property
    def hp_loss_sum(self) -> int:
        return sum(transition.hp_loss for transition in self.combat_transitions)

    def completed_combats_before(
        self,
        *,
        seed: int,
        act: int,
        floor: int,
    ) -> tuple[RunCombatResourceTransition, ...]:
        """Return one episode's strictly earlier completed combat transitions."""
        boundary = (
            _integer(act, "history boundary act", minimum=0),
            _integer(floor, "history boundary floor", minimum=0),
        )
        episode_seed = _integer(seed, "history episode seed", minimum=0)
        return tuple(
            transition
            for transition in self.combat_transitions
            if transition.start.seed == episode_seed
            and (transition.start.act, transition.start.floor) < boundary
        )

    @property
    def potion_identity_losses(self) -> tuple[tuple[str, int], ...]:
        lost, _ = _potion_identity_changes(self.combat_transitions)
        return tuple(sorted(lost.items()))

    @property
    def potion_identity_gains(self) -> tuple[tuple[str, int], ...]:
        _, gained = _potion_identity_changes(self.combat_transitions)
        return tuple(sorted(gained.items()))

    @property
    def open_combat_count(self) -> int:
        return sum(endpoint.combat_open for endpoint in self.episode_endpoints)

    @property
    def seed_summaries(self) -> tuple[RunSeedResourceSummary, ...]:
        grouped: dict[int, list[RunCombatResourceTransition]] = {}
        for transition in self.combat_transitions:
            grouped.setdefault(transition.start.seed, []).append(transition)
        return tuple(
            RunSeedResourceSummary.from_evidence(
                endpoint,
                grouped.get(endpoint.context.seed, ()),
            )
            for endpoint in sorted(
                self.episode_endpoints,
                key=lambda item: item.context.seed,
            )
        )


@dataclass(frozen=True)
class RunEpisodeResourceEndpoint:
    context: RunPublicContext
    terminal_reward: int | None
    combat_open: bool

    def __post_init__(self) -> None:
        if not isinstance(self.context, RunPublicContext):
            raise RunResourceTraceError("resource endpoint context must be typed")
        if type(self.combat_open) is not bool:
            raise RunResourceTraceError("resource endpoint combat_open must be bool")
        if self.combat_open != self.context.is_combat:
            raise RunResourceTraceError(
                "resource endpoint open-combat flag disagrees with its context"
            )
        if self.context.is_terminal:
            if self.terminal_reward not in (-1, 1):
                raise RunResourceTraceError(
                    "terminal resource endpoint requires a terminal reward"
                )
        elif self.terminal_reward is not None:
            raise RunResourceTraceError(
                "non-terminal resource endpoint cannot have a terminal reward"
            )


@dataclass(frozen=True)
class RunSeedResourceSummary:
    seed: int
    slot_index: int
    combat_count: int
    hp_loss_sum: int
    last_act: int
    last_floor: int
    last_hp: int
    last_max_hp: int
    last_gold: int
    last_potion_ids: tuple[str | None, ...]
    terminal_reward: int | None
    open_combat: bool
    potion_identity_losses: tuple[tuple[str, int], ...]
    potion_identity_gains: tuple[tuple[str, int], ...]

    @classmethod
    def from_evidence(
        cls,
        endpoint: RunEpisodeResourceEndpoint,
        transitions: Sequence[RunCombatResourceTransition],
    ) -> RunSeedResourceSummary:
        normalized = tuple(transitions)
        seed = endpoint.context.seed
        if any(transition.start.seed != seed for transition in normalized):
            raise RunResourceTraceError("seed resource summary mixed episode seeds")
        slots = {transition.start.slot_index for transition in normalized}
        slots.add(endpoint.context.slot_index)
        if len(slots) != 1:
            raise RunResourceTraceError("one episode seed crossed environment slots")
        transition_rewards = tuple(
            transition.terminal_reward for transition in normalized
            if transition.terminal_reward is not None
        )
        if transition_rewards not in ((), (endpoint.terminal_reward,)):
            raise RunResourceTraceError(
                "combat transition and episode endpoint rewards disagree"
            )
        lost, gained = _potion_identity_changes(normalized)
        last = endpoint.context
        return cls(
            seed=seed,
            slot_index=slots.pop(),
            combat_count=len(normalized),
            hp_loss_sum=sum(transition.hp_loss for transition in normalized),
            last_act=last.act,
            last_floor=last.floor,
            last_hp=last.hp,
            last_max_hp=last.max_hp,
            last_gold=last.gold,
            last_potion_ids=last.potion_ids,
            terminal_reward=endpoint.terminal_reward,
            open_combat=endpoint.combat_open,
            potion_identity_losses=tuple(sorted(lost.items())),
            potion_identity_gains=tuple(sorted(gained.items())),
        )


class RunResourceTraceAccumulator:
    """Observe exact before/after bridge contexts without retaining sessions."""

    def __init__(self) -> None:
        self._open: dict[int, RunPublicContext] = {}
        self._completed: list[RunCombatResourceTransition] = []
        self._latest: dict[int, RunPublicContext] = {}
        self._terminal_rewards: dict[int, int] = {}

    def record(
        self,
        before: Mapping[int, RunPublicContext],
        after: Mapping[int, RunPublicContext],
        bridge_step: Mapping[str, object],
    ) -> None:
        stepped = _integer_sequence(bridge_step, "slot_indices")
        terminal_slots = _integer_sequence(bridge_step, "terminal_slot_indices")
        terminal_rewards = _integer_sequence(bridge_step, "terminal_reward")
        if len(terminal_slots) != len(terminal_rewards):
            raise RunResourceTraceError("terminal slot and reward rows are misaligned")
        if len(set(terminal_slots)) != len(terminal_slots):
            raise RunResourceTraceError("terminal resource rows repeat a slot")
        rewards = dict(zip(terminal_slots, terminal_rewards, strict=True))

        for slot in stepped:
            try:
                left = before[slot]
                right = after[slot]
            except KeyError as error:
                raise RunResourceTraceError(
                    f"step slot {slot} is missing a public run context"
                ) from error
            if left.slot_index != slot or right.slot_index != slot:
                raise RunResourceTraceError("public run context changed its slot")
            if left.seed != right.seed:
                raise RunResourceTraceError("one environment step changed episode seed")
            self._latest[right.seed] = right
            if left.is_combat:
                start = self._open.setdefault(slot, left)
                if start.seed != left.seed:
                    raise RunResourceTraceError(
                        "open combat context changed episode seed"
                    )
                if not right.is_combat:
                    reward = rewards.get(slot) if right.is_terminal else None
                    self._completed.append(
                        RunCombatResourceTransition(start, right, reward)
                    )
                    del self._open[slot]
            elif slot in self._open:
                raise RunResourceTraceError(
                    "open combat disappeared before its transition was recorded"
                )
            elif right.is_combat:
                self._open[slot] = right

            reward = rewards.get(slot)
            if reward is not None:
                previous = self._terminal_rewards.setdefault(right.seed, reward)
                if previous != reward:
                    raise RunResourceTraceError(
                        "one episode seed received conflicting terminal rewards"
                    )

        unexpected_rewards = set(rewards).difference(stepped)
        if unexpected_rewards:
            raise RunResourceTraceError(
                "terminal resource rows contain an unstepped slot"
            )

    def finish(self) -> RunResourceTrace:
        open_seeds = {context.seed for context in self._open.values()}
        return RunResourceTrace(
            combat_transitions=tuple(self._completed),
            episode_endpoints=tuple(
                RunEpisodeResourceEndpoint(
                    context=context,
                    terminal_reward=self._terminal_rewards.get(seed),
                    combat_open=seed in open_seeds,
                )
                for seed, context in sorted(self._latest.items())
            ),
        )


class ResourceTracingEnvironmentFactory:
    """Wrap one held-out environment factory and intercept only its step boundary."""

    def __init__(self, environment: Callable[[list[int]], BatchEnvironment]) -> None:
        if not callable(environment):
            raise RunResourceTraceError("resource trace environment must be callable")
        self._environment = environment
        self._accumulator = RunResourceTraceAccumulator()
        self._created = False

    def __call__(self, seeds: list[int]) -> BatchEnvironment:
        if self._created:
            raise RunResourceTraceError(
                "resource trace factory supports one evaluation population"
            )
        self._created = True
        return _ResourceTracingEnvironment(
            self._environment(seeds),
            self._accumulator,
        )

    @property
    def trace(self) -> RunResourceTrace:
        if not self._created:
            raise RunResourceTraceError("resource trace population was not created")
        return self._accumulator.finish()


class _ResourceTracingEnvironment:
    def __init__(
        self,
        inner: BatchEnvironment,
        accumulator: RunResourceTraceAccumulator,
    ) -> None:
        self._inner = inner
        self._accumulator = accumulator

    def __getattr__(self, name: str) -> Any:
        return getattr(self._inner, name)

    def step(self) -> Mapping[str, object]:
        before = _public_contexts(self._inner)
        result = self._inner.step()
        if not isinstance(result, Mapping):
            raise RunResourceTraceError("bridge step must be a mapping")
        after = _public_contexts(self._inner)
        self._accumulator.record(before, after, result)
        return result


def _public_contexts(env: object) -> dict[int, RunPublicContext]:
    source = getattr(env, "public_run_contexts", None)
    if not callable(source):
        raise RunResourceTraceError(
            "evaluation environment does not expose public_run_contexts()"
        )
    rows = source()
    if not isinstance(rows, Sequence) or isinstance(rows, (str, bytes)):
        raise RunResourceTraceError("public_run_contexts() must return a sequence")
    contexts = tuple(RunPublicContext.from_bridge_row(row) for row in rows)
    by_slot = {context.slot_index: context for context in contexts}
    if len(by_slot) != len(contexts):
        raise RunResourceTraceError("public run contexts repeat a slot")
    return by_slot


def _potion_identity_changes(
    transitions: Sequence[RunCombatResourceTransition],
) -> tuple[Counter[str], Counter[str]]:
    lost: Counter[str] = Counter()
    gained: Counter[str] = Counter()
    for transition in transitions:
        start = Counter(potion for potion in transition.start.potion_ids if potion)
        end = Counter(potion for potion in transition.end.potion_ids if potion)
        lost.update(start - end)
        gained.update(end - start)
    return lost, gained


def _potion_ids(value: object) -> tuple[str | None, ...]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise RunResourceTraceError("potion_ids must be a sequence")
    normalized: list[str | None] = []
    for potion in value:
        if potion is not None and (not isinstance(potion, str) or not potion):
            raise RunResourceTraceError("potion identity must be non-empty text or null")
        normalized.append(potion)
    return tuple(normalized)


def _monster_ids(value: object) -> tuple[str, ...]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise RunResourceTraceError("monster_ids must be a sequence")
    normalized = tuple(value)
    if any(not isinstance(monster, str) or not monster for monster in normalized):
        raise RunResourceTraceError("monster identity must be non-empty text")
    return normalized


def _optional_identity(value: object) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str) or not value:
        raise RunResourceTraceError("encounter identity must be non-empty text")
    return value


def _integer_sequence(source: Mapping[str, object], name: str) -> tuple[int, ...]:
    try:
        values = source[name]
    except KeyError as error:
        raise RunResourceTraceError(f"bridge step is missing {name}") from error
    try:
        return tuple(_integer(value, name) for value in values)  # type: ignore[union-attr]
    except TypeError as error:
        raise RunResourceTraceError(f"bridge step {name} is not iterable") from error


def _attribute(source: object, name: str) -> object:
    try:
        return getattr(source, name)
    except AttributeError as error:
        raise RunResourceTraceError(
            f"public run context is missing {name}"
        ) from error


def _integer(value: object, name: str, *, minimum: int | None = None) -> int:
    if isinstance(value, bool):
        raise RunResourceTraceError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise RunResourceTraceError(f"{name} must be an integer") from error
    if minimum is not None and normalized < minimum:
        raise RunResourceTraceError(f"{name} must be at least {minimum}")
    return normalized


def _boolean(value: object, name: str) -> bool:
    if type(value) is not bool:
        raise RunResourceTraceError(f"{name} must be bool")
    return value
