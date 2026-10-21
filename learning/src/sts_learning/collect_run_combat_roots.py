"""Capture typed later-combat roots while a frozen policy advances real runs."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
import time
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Protocol, cast

from .combat_potion_lane import CombatPotionLane
from .combat_root_audit import (
    CombatRootAudit,
    CombatRootAuditError,
    read_combat_root_audit,
)
from .decision_progress import BridgeDecisionProgressProvider
from .driver import (
    BatchEnvironment,
    OnlineBatchDriver,
    RecoveryPlan,
    initialize_population,
)
from .evaluate_run import RunPotionLane, resolve_run_potion_lane
from .published_combat_behavior import (
    PublishedCombatBehavior,
    recover_published_combat_behavior,
)
from .published_run_behavior import (
    PublishedRunBehavior,
    is_run_training_publication,
    recover_published_run_behavior,
)
from .recovery import RecoverySlotSnapshot, TerminalAccountingBatch
from .run_resource_trace import RunPublicContext
from .run_resource_trace import (
    ResourceTracingEnvironmentFactory,
    RunResourceTrace,
)
from .seeds import SeedPartition, SeedPartitionSpec, SeedSchedule
from .strategic_decision_audit import (
    StrategicDecisionAudit,
    StrategicDecisionAuditError,
    read_strategic_decision_audit,
)
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_behavior import (
    FrozenCombatAnchor,
    FrozenCombatGreedyTorchPolicy,
    FrozenDecisionRule,
)
from .torch_session_config import CategoricalSessionBridge


class RunCombatRootCollectionError(RuntimeError):
    """A bounded root collection lost identity, provenance, or a resource bound."""


class CombatFightClass(str, Enum):
    ANY = "any"
    ORDINARY = "ordinary"
    ELITE = "elite"
    BOSS = "boss"


class _RootExportEnvironment(BatchEnvironment, Protocol):
    def public_run_contexts(self) -> Sequence[object]: ...

    def public_information_snapshots(self) -> Sequence[object]: ...

    def combat_root_contexts(self) -> Sequence[object]: ...

    def combat_root_audit(self, slot_index: int) -> object: ...

    def strategic_decision_audit_json(self, slot_index: int) -> str | None: ...

    def combat_root_artifact_bytes(
        self,
        slot_indices: list[int],
        *,
        max_bytes: int,
    ) -> bytes: ...


@dataclass(frozen=True)
class RequiredPotionSlot:
    slot_index: int
    potion_id: str

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "slot_index",
            _nonnegative(self.slot_index, "required potion slot"),
        )
        if not isinstance(self.potion_id, str) or not self.potion_id:
            raise RunCombatRootCollectionError(
                "required potion id must be a non-empty string"
            )


@dataclass(frozen=True)
class EncounterQuota:
    encounter_id: str
    root_count: int

    def __post_init__(self) -> None:
        if not isinstance(self.encounter_id, str) or not self.encounter_id:
            raise RunCombatRootCollectionError(
                "encounter quota id must be non-empty text"
            )
        object.__setattr__(
            self,
            "root_count",
            _positive(self.root_count, "encounter quota root count"),
        )


@dataclass(frozen=True)
class RunCombatRootCollectionConfig:
    behavior: Path
    output: Path
    root_count: int
    max_batch_steps: int
    wall_ms: int
    behavior_seed: int
    seed_start: int
    ascension_level: int
    seed_partition: SeedPartition = SeedPartition.TRAINING
    seed_partition_spec: SeedPartitionSpec = SeedPartitionSpec()
    combat_decision_rule: FrozenDecisionRule = FrozenDecisionRule.GREEDY
    min_floor: int = 2
    max_floor: int | None = None
    required_prior_combat_count: int | None = None
    min_hp_percent: int = 0
    min_usable_potions: int = 1
    potion_lane: RunPotionLane = RunPotionLane.TRAINED
    max_artifact_bytes: int = 16 * 1024 * 1024
    required_potion: RequiredPotionSlot | None = None
    required_encounter_id: str | None = None
    distinct_encounters: bool = False
    encounter_quotas: tuple[EncounterQuota, ...] = ()
    strategic_behavior: Path | None = None
    fight_class: CombatFightClass = CombatFightClass.ANY

    def __post_init__(self) -> None:
        behavior = Path(self.behavior).resolve()
        output = Path(self.output).resolve()
        if not behavior.is_dir():
            raise RunCombatRootCollectionError(
                "root collection behavior is not a directory"
            )
        if output.exists():
            raise RunCombatRootCollectionError(
                "root collection output must be a fresh file"
            )
        strategic_behavior = (
            None
            if self.strategic_behavior is None
            else Path(self.strategic_behavior).resolve()
        )
        if strategic_behavior is not None and not strategic_behavior.is_dir():
            raise RunCombatRootCollectionError(
                "root collection strategic behavior is not a directory"
            )
        behavior_directories = (
            (behavior,)
            if strategic_behavior is None
            else (behavior, strategic_behavior)
        )
        if any(
            output == source or source in output.parents
            for source in behavior_directories
        ):
            raise RunCombatRootCollectionError(
                "root collection output must stay outside the behavior directory"
            )
        if not output.parent.is_dir():
            raise RunCombatRootCollectionError(
                "root collection output parent does not exist"
            )
        if not isinstance(self.potion_lane, RunPotionLane):
            raise RunCombatRootCollectionError(
                "root collection potion lane must be typed"
            )
        if not isinstance(self.combat_decision_rule, FrozenDecisionRule):
            raise RunCombatRootCollectionError(
                "root collection combat decision rule must be typed"
            )
        if not isinstance(self.fight_class, CombatFightClass):
            raise RunCombatRootCollectionError(
                "root collection fight class must be typed"
            )
        if (
            strategic_behavior is not None
            and self.combat_decision_rule is not FrozenDecisionRule.GREEDY
        ):
            raise RunCombatRootCollectionError(
                "scoped root collection requires greedy combat decisions"
            )
        if (
            strategic_behavior is not None
            and self.potion_lane is RunPotionLane.TRAINED
        ):
            raise RunCombatRootCollectionError(
                "scoped root collection requires an explicit whole-run potion lane"
            )
        if not isinstance(self.seed_partition, SeedPartition):
            raise RunCombatRootCollectionError(
                "root collection seed partition must be typed"
            )
        if not isinstance(self.seed_partition_spec, SeedPartitionSpec):
            raise RunCombatRootCollectionError(
                "root collection seed partition spec must be typed"
            )
        if self.required_potion is not None and not isinstance(
            self.required_potion,
            RequiredPotionSlot,
        ):
            raise RunCombatRootCollectionError(
                "required potion selector must be typed"
            )
        if not isinstance(self.distinct_encounters, bool):
            raise RunCombatRootCollectionError(
                "distinct encounter selector must be boolean"
            )
        if self.required_encounter_id is not None and (
            not isinstance(self.required_encounter_id, str)
            or not self.required_encounter_id
        ):
            raise RunCombatRootCollectionError(
                "required encounter id must be non-empty text"
            )
        if (
            not isinstance(self.encounter_quotas, tuple)
            or any(
                not isinstance(quota, EncounterQuota)
                for quota in self.encounter_quotas
            )
        ):
            raise RunCombatRootCollectionError(
                "encounter quotas must be a typed tuple"
            )
        quota_ids = tuple(quota.encounter_id for quota in self.encounter_quotas)
        if len(set(quota_ids)) != len(quota_ids):
            raise RunCombatRootCollectionError(
                "encounter quota ids must be distinct"
            )
        if self.encounter_quotas and (
            self.required_encounter_id is not None or self.distinct_encounters
        ):
            raise RunCombatRootCollectionError(
                "encounter quotas cannot be combined with another encounter selector"
            )
        object.__setattr__(self, "behavior", behavior)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "strategic_behavior", strategic_behavior)
        root_count = _positive(self.root_count, "root_count")
        if root_count > 64:
            raise RunCombatRootCollectionError("root_count must be at most 64")
        if (
            self.distinct_encounters
            and self.required_encounter_id is not None
            and root_count > 1
        ):
            raise RunCombatRootCollectionError(
                "distinct encounter collection cannot request multiple roots "
                "for one exact required encounter"
            )
        quota_root_count = sum(
            quota.root_count for quota in self.encounter_quotas
        )
        if self.encounter_quotas and root_count != quota_root_count:
            raise RunCombatRootCollectionError(
                "root_count must equal the encounter quota total: "
                f"root_count={root_count} quota_total={quota_root_count}"
            )
        object.__setattr__(self, "root_count", root_count)
        for name in ("max_batch_steps", "wall_ms", "max_artifact_bytes"):
            object.__setattr__(self, name, _positive(getattr(self, name), name))
        min_floor = _nonnegative(self.min_floor, "min_floor")
        object.__setattr__(self, "min_floor", min_floor)
        if self.max_floor is not None:
            max_floor = _nonnegative(self.max_floor, "max_floor")
            if max_floor < min_floor:
                raise RunCombatRootCollectionError(
                    "max_floor must be at least min_floor"
                )
            object.__setattr__(self, "max_floor", max_floor)
        if self.required_prior_combat_count is not None:
            object.__setattr__(
                self,
                "required_prior_combat_count",
                _nonnegative(
                    self.required_prior_combat_count,
                    "required_prior_combat_count",
                ),
            )
        min_hp_percent = _nonnegative(self.min_hp_percent, "min_hp_percent")
        if min_hp_percent > 100:
            raise RunCombatRootCollectionError(
                "min_hp_percent must be at most 100"
            )
        object.__setattr__(self, "min_hp_percent", min_hp_percent)
        object.__setattr__(
            self,
            "min_usable_potions",
            _nonnegative(self.min_usable_potions, "min_usable_potions"),
        )
        object.__setattr__(
            self,
            "behavior_seed",
            _seed(self.behavior_seed, "behavior_seed"),
        )
        object.__setattr__(
            self,
            "seed_start",
            _seed(self.seed_start, "seed_start"),
        )
        ascension_level = _nonnegative(self.ascension_level, "ascension_level")
        if ascension_level > 20:
            raise RunCombatRootCollectionError(
                "ascension_level must be at most 20"
            )
        object.__setattr__(self, "ascension_level", ascension_level)


@dataclass(frozen=True)
class CapturedRunCombatRoot:
    seed: int
    act: int
    floor: int
    ascension_level: int
    hp: int
    max_hp: int
    potion_ids: tuple[str | None, ...]
    encounter_id: str
    monster_ids: tuple[str, ...]
    filled_potion_count: int
    usable_potion_count: int
    prior_combat_count: int
    audit: CombatRootAudit
    prior_strategic_decisions: tuple[dict[str, object], ...]


class _RootCaptureSink:
    def __init__(
        self,
        config: RunCombatRootCollectionConfig,
        required_encounter_id: str | None,
        encounter_quotas: tuple[EncounterQuota, ...],
        prior_combat_count: Callable[[int, int, int], int],
    ) -> None:
        self.config = config
        self.required_encounter_id = required_encounter_id
        self.encounter_quotas = {
            quota.encounter_id: quota.root_count for quota in encounter_quotas
        }
        if not callable(prior_combat_count):
            raise RunCombatRootCollectionError(
                "prior combat count source must be callable"
            )
        self._prior_combat_count = prior_combat_count
        self.payloads: list[bytes] = []
        self.roots: list[CapturedRunCombatRoot] = []
        self._captured_seeds: set[int] = set()
        self._captured_encounters: set[str] = set()
        self._captured_encounter_counts = {
            encounter_id: 0 for encounter_id in self.encounter_quotas
        }
        self._strategic_decisions: dict[int, list[dict[str, object]]] = {}

    @property
    def complete(self) -> bool:
        if self.encounter_quotas:
            return all(
                self._captured_encounter_counts[encounter_id] >= root_count
                for encounter_id, root_count in self.encounter_quotas.items()
            )
        return len(self.roots) >= self.config.root_count

    @property
    def encounter_quota_progress(self) -> tuple[dict[str, object], ...]:
        return tuple(
            {
                "encounter_id": encounter_id,
                "requested_roots": requested_roots,
                "captured_roots": self._captured_encounter_counts[encounter_id],
            }
            for encounter_id, requested_roots in self.encounter_quotas.items()
        )

    def record_strategic_decision(
        self,
        context: RunPublicContext,
        audit: StrategicDecisionAudit,
        selected_ordinal: object,
    ) -> None:
        row = audit.selected_mapping(selected_ordinal)
        row.update(
            {
                "act": context.act,
                "floor": context.floor,
                "hp": context.hp,
                "max_hp": context.max_hp,
                "gold": context.gold,
                "potion_ids": context.potion_ids,
            }
        )
        self._strategic_decisions.setdefault(context.seed, []).append(row)

    def observe(self, env: _RootExportEnvironment) -> None:
        if self.complete:
            return
        public = {
            context.slot_index: context
            for context in (
                RunPublicContext.from_bridge_row(row)
                for row in env.public_run_contexts()
            )
        }
        for row in env.combat_root_contexts():
            if self.complete:
                break
            slot, root = _root_context_row(row)
            context = public.get(slot)
            if context is None:
                raise RunCombatRootCollectionError(
                    "combat root has no aligned public run context"
                )
            if not context.is_combat or context.is_terminal:
                raise RunCombatRootCollectionError(
                    "combat root public context is not an active combat"
                )
            if context.seed in self._captured_seeds:
                continue
            floor = _root_integer(root, "floor", minimum=0)
            act = _root_integer(root, "act", minimum=0)
            ascension_level = _root_integer(root, "ascension_level", minimum=0)
            hp = _root_integer(root, "hp", minimum=0)
            max_hp = _root_integer(root, "max_hp", minimum=1)
            filled = _root_integer(root, "filled_potion_count", minimum=0)
            usable = _root_integer(root, "usable_potion_count", minimum=0)
            if (act, floor, hp, max_hp) != (
                context.act,
                context.floor,
                context.hp,
                context.max_hp,
            ):
                raise RunCombatRootCollectionError(
                    "combat root and public run context disagree: "
                    f"slot={slot} seed={context.seed} "
                    f"root=(act={act},floor={floor},hp={hp},max_hp={max_hp}) "
                    f"public=(act={context.act},floor={context.floor},"
                    f"hp={context.hp},max_hp={context.max_hp})"
                )
            if ascension_level != self.config.ascension_level:
                raise RunCombatRootCollectionError(
                    "combat root changed the requested ascension level: "
                    f"slot={slot} seed={context.seed} "
                    f"root={ascension_level} requested={self.config.ascension_level}"
                )
            if filled != sum(potion is not None for potion in context.potion_ids):
                raise RunCombatRootCollectionError(
                    "combat root and public potion inventory disagree"
                )
            if usable > filled:
                raise RunCombatRootCollectionError(
                    "usable potion count exceeds filled inventory"
                )
            if floor < self.config.min_floor:
                continue
            if self.config.max_floor is not None and floor > self.config.max_floor:
                continue
            prior_combat_count = _nonnegative(
                self._prior_combat_count(context.seed, act, floor),
                "prior combat count",
            )
            if (
                self.config.required_prior_combat_count is not None
                and prior_combat_count
                != self.config.required_prior_combat_count
            ):
                continue
            if 100 * hp < self.config.min_hp_percent * max_hp:
                continue
            if usable < self.config.min_usable_potions:
                continue
            required = self.config.required_potion
            if required is not None and (
                required.slot_index >= len(context.potion_ids)
                or context.potion_ids[required.slot_index] != required.potion_id
            ):
                continue
            encounter = context.encounter_id
            if encounter is None:
                raise RunCombatRootCollectionError(
                    "combat root is missing its typed encounter identity"
                )
            if self.required_encounter_id is not None and (
                encounter != self.required_encounter_id
            ):
                continue
            if self.encounter_quotas and (
                encounter not in self.encounter_quotas
                or self._captured_encounter_counts[encounter]
                >= self.encounter_quotas[encounter]
            ):
                continue
            if (
                self.config.distinct_encounters
                and encounter in self._captured_encounters
            ):
                continue
            try:
                audit = read_combat_root_audit(env, slot)
            except CombatRootAuditError as error:
                raise RunCombatRootCollectionError(str(error)) from error
            if audit.ascension_level != ascension_level:
                raise RunCombatRootCollectionError(
                    "combat root audit disagrees with root ascension"
                )
            if not _matches_fight_class(audit, self.config.fight_class):
                continue
            master_deck_card_count = _root_integer(
                root,
                "master_deck_card_count",
                minimum=0,
            )
            relic_count = _root_integer(root, "relic_count", minimum=0)
            if audit.deck_card_count != master_deck_card_count:
                raise RunCombatRootCollectionError(
                    "combat root audit deck count disagrees with root context"
                )
            if len(audit.relic_ids) != relic_count:
                raise RunCombatRootCollectionError(
                    "combat root audit relic count disagrees with root context"
                )
            payload = bytes(
                env.combat_root_artifact_bytes(
                    [slot],
                    max_bytes=self.config.max_artifact_bytes,
                )
            )
            if not payload or len(payload) > self.config.max_artifact_bytes:
                raise RunCombatRootCollectionError(
                    "captured combat root payload violates its byte bound"
                )
            self.payloads.append(payload)
            self.roots.append(
                CapturedRunCombatRoot(
                    seed=context.seed,
                    act=act,
                    floor=floor,
                    ascension_level=ascension_level,
                    hp=hp,
                    max_hp=max_hp,
                    potion_ids=context.potion_ids,
                    encounter_id=encounter,
                    monster_ids=context.monster_ids,
                    filled_potion_count=filled,
                    usable_potion_count=usable,
                    prior_combat_count=prior_combat_count,
                    audit=audit,
                    prior_strategic_decisions=tuple(
                        self._strategic_decisions.get(context.seed, ())
                    ),
                )
            )
            self._captured_seeds.add(context.seed)
            self._captured_encounters.add(encounter)
            if encounter in self._captured_encounter_counts:
                self._captured_encounter_counts[encounter] += 1


class _CapturingEnvironment:
    def __init__(
        self,
        env: _RootExportEnvironment,
        sink: _RootCaptureSink,
    ) -> None:
        self.env = env
        self.sink = sink
        self._pending_decisions: tuple[
            tuple[RunPublicContext, StrategicDecisionAudit | None], ...
        ] | None = None
        self._pending_decision_token_ids: list[str] | None = None

    @property
    def slot_count(self) -> int:
        return self.env.slot_count

    @property
    def terminal_count(self) -> int:
        return self.env.terminal_count

    @property
    def ready(self) -> bool:
        return self.env.ready

    def decision_batch(self, *, semantic: bool = False) -> Mapping[str, object]:
        if self._pending_decisions is not None:
            raise RunCombatRootCollectionError(
                "a strategic decision audit is still waiting for its chosen ordinal"
            )
        self.sink.observe(self.env)
        batch = self.env.decision_batch(semantic=semantic)
        slots = _decision_integer_sequence(batch, "slot_indices")
        counts = _decision_integer_sequence(batch, "candidate_counts")
        if len(slots) != len(counts):
            raise RunCombatRootCollectionError(
                "decision audit slot and candidate-count columns are misaligned"
            )
        public = {
            context.slot_index: context
            for context in (
                RunPublicContext.from_bridge_row(row)
                for row in self.env.public_run_contexts()
            )
        }
        pending: list[tuple[RunPublicContext, StrategicDecisionAudit | None]] = []
        for slot, candidate_count in zip(slots, counts, strict=True):
            context = public.get(slot)
            if context is None:
                raise RunCombatRootCollectionError(
                    "decision audit has no aligned public run context"
                )
            try:
                audit = read_strategic_decision_audit(self.env, slot)
            except StrategicDecisionAuditError as error:
                raise RunCombatRootCollectionError(str(error)) from error
            if audit is not None and len(audit.candidates) != candidate_count:
                raise RunCombatRootCollectionError(
                    "strategic decision audit candidate count disagrees with the decision batch"
                )
            pending.append((context, audit))
        self._pending_decisions = tuple(pending)
        raw_token_ids = batch.get("decision_token_ids")
        if not isinstance(raw_token_ids, Sequence) or isinstance(
            raw_token_ids, (str, bytes)
        ):
            raise RunCombatRootCollectionError(
                "decision batch has no aligned V3 decision token ids"
            )
        token_ids = list(raw_token_ids)
        if len(token_ids) != len(pending) or not all(
            isinstance(token_id, str) and token_id for token_id in token_ids
        ):
            raise RunCombatRootCollectionError(
                "decision batch V3 token ids are malformed or misaligned"
            )
        self._pending_decision_token_ids = token_ids
        return batch

    def public_run_contexts(self) -> Sequence[object]:
        return self.env.public_run_contexts()

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None:
        pending = self._pending_decisions
        if pending is None:
            raise RunCombatRootCollectionError(
                "chosen ordinals have no pending strategic decision audit"
            )
        if len(ordinals) != len(pending):
            raise RunCombatRootCollectionError(
                "chosen ordinals and strategic decision audits are misaligned"
            )
        token_ids = self._pending_decision_token_ids
        if token_ids is None:
            raise RunCombatRootCollectionError("chosen ordinals have no V3 decision tokens")
        if decision_token_ids != token_ids:
            raise RunCombatRootCollectionError(
                "chosen V3 decision tokens disagree with the audited decision batch"
            )
        self.env.choose(decision_token_ids, ordinals)
        for (context, audit), ordinal in zip(pending, ordinals, strict=True):
            if audit is not None:
                self.sink.record_strategic_decision(context, audit, ordinal)
        self._pending_decisions = None
        self._pending_decision_token_ids = None

    def step(self) -> Mapping[str, object]:
        return self.env.step()

    def public_information_snapshots(self) -> Sequence[object]:
        return self.env.public_information_snapshots()

    def checkpoint_slots(self, slot_indices: list[int]):
        return self.env.checkpoint_slots(slot_indices)

    def restore_slots(self, slot_indices: list[int], checkpoints: object) -> None:
        self.env.restore_slots(slot_indices, checkpoints)

    def reset_slots(self, slot_indices: list[int], seeds: list[int]) -> None:
        self.env.reset_slots(slot_indices, seeds)

    def reset_slots_checkpointed(
        self,
        slot_indices: list[int],
        seeds: list[int],
    ):
        return self.env.reset_slots_checkpointed(slot_indices, seeds)

    def checkpoint_bytes(self, *, max_bytes: int) -> bytes:
        return self.env.checkpoint_bytes(max_bytes=max_bytes)


class _NoRecovery:
    def plan_recovery(
        self,
        accounting: TerminalAccountingBatch,
        snapshots: tuple[RecoverySlotSnapshot, ...],
    ) -> RecoveryPlan:
        return RecoveryPlan()


def run_run_combat_root_collection(
    config: RunCombatRootCollectionConfig,
    *,
    combat_bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
    artifact_merger: Callable[..., bytes] | None = None,
) -> dict[str, object]:
    """Capture one qualifying exact combat root from each distinct run seed."""

    if not isinstance(config, RunCombatRootCollectionConfig):
        raise RunCombatRootCollectionError("root collection config must be typed")
    active_combat_bridge = (
        combat_bridge if combat_bridge is not None else CombatSessionBridge.installed()
    )
    active_run_bridge = (
        run_bridge if run_bridge is not None else CategoricalSessionBridge.installed()
    )
    if not isinstance(active_combat_bridge, CombatSessionBridge):
        raise RunCombatRootCollectionError("combat bridge must be typed")
    if not isinstance(active_run_bridge, CategoricalSessionBridge):
        raise RunCombatRootCollectionError("run bridge must be typed")
    if active_combat_bridge.semantic_schema != active_run_bridge.semantic_schema:
        raise RunCombatRootCollectionError(
            "combat behavior and run environment semantic schemas differ"
        )
    if config.required_potion is not None:
        supported_source = getattr(
            active_run_bridge.environment,
            "supported_potion_ids",
            None,
        )
        if not callable(supported_source):
            raise RunCombatRootCollectionError(
                "run bridge does not expose supported potion identities"
            )
        supported = supported_source()
        if (
            not isinstance(supported, Sequence)
            or isinstance(supported, (str, bytes))
            or not supported
            or any(not isinstance(potion, str) or not potion for potion in supported)
            or len(set(supported)) != len(supported)
        ):
            raise RunCombatRootCollectionError(
                "run bridge returned malformed supported potion identities"
            )
        if config.required_potion.potion_id not in supported:
            raise RunCombatRootCollectionError(
                "required potion id is not supported by the installed bridge: "
                f"{config.required_potion.potion_id}"
            )
    required_encounter_id = config.required_encounter_id
    encounter_quotas = config.encounter_quotas
    if required_encounter_id is not None or encounter_quotas:
        canonical_source = getattr(
            active_run_bridge.environment,
            "canonical_encounter_id",
            None,
        )
        if not callable(canonical_source):
            raise RunCombatRootCollectionError(
                "run bridge does not expose canonical encounter identity validation"
            )
        if required_encounter_id is not None:
            required_encounter_id = _canonical_encounter_id(
                canonical_source,
                required_encounter_id,
            )
        encounter_quotas = tuple(
            EncounterQuota(
                _canonical_encounter_id(canonical_source, quota.encounter_id),
                quota.root_count,
            )
            for quota in encounter_quotas
        )
        canonical_quota_ids = tuple(
            quota.encounter_id for quota in encounter_quotas
        )
        if len(set(canonical_quota_ids)) != len(canonical_quota_ids):
            raise RunCombatRootCollectionError(
                "encounter quota ids must remain distinct after canonicalization"
            )

    recovered: PublishedCombatBehavior | PublishedRunBehavior
    if is_run_training_publication(config.behavior):
        recovered = recover_published_run_behavior(
            config.behavior,
            active_run_bridge,
            (config.behavior_seed,),
        )
        behavior_kind = "run"
    else:
        recovered = recover_published_combat_behavior(
            config.behavior,
            active_combat_bridge,
            CombatWinSessionLimits(),
            (config.behavior_seed,),
        )
        behavior_kind = "combat"
    strategic_recovered: PublishedCombatBehavior | None = None
    if config.strategic_behavior is not None:
        if is_run_training_publication(config.behavior):
            raise RunCombatRootCollectionError(
                "scoped root collection combat anchor must be combat-trained"
            )
        if is_run_training_publication(config.strategic_behavior):
            raise RunCombatRootCollectionError(
                "scoped root collection strategic source must be combat-trained"
            )
        strategic_recovered = recover_published_combat_behavior(
            config.strategic_behavior,
            active_combat_bridge,
            CombatWinSessionLimits(),
            (config.behavior_seed,),
        )
    potion_lane = resolve_run_potion_lane(config.potion_lane, recovered)
    environment_constructor = (
        active_run_bridge.environment
        if potion_lane is CombatPotionLane.ALL
        else active_run_bridge.environment_without_combat_potions
    )
    merger = artifact_merger or getattr(
        active_run_bridge.environment,
        "merge_combat_root_artifact_bytes",
        None,
    )
    if not callable(merger):
        raise RunCombatRootCollectionError(
            "run bridge does not provide opaque combat-root merging"
        )

    tracing_factory = ResourceTracingEnvironmentFactory(
        lambda seeds: environment_constructor(seeds, config.ascension_level)
    )
    sink = _RootCaptureSink(
        config,
        required_encounter_id,
        encounter_quotas,
        lambda seed, act, floor: len(
            tracing_factory.trace.completed_combats_before(
                seed=seed,
                act=act,
                floor=floor,
            )
        ),
    )

    def capturing_factory(seeds: list[int]) -> _CapturingEnvironment:
        env = tracing_factory(seeds)
        for name in (
            "public_run_contexts",
            "combat_root_contexts",
            "combat_root_audit",
            "strategic_decision_audit_json",
            "combat_root_artifact_bytes",
        ):
            if not callable(getattr(env, name, None)):
                raise RunCombatRootCollectionError(
                    f"run environment lacks {name}()"
                )
        return _CapturingEnvironment(cast(_RootExportEnvironment, env), sink)

    schedule_start = SeedSchedule(
        config.seed_partition,
        spec=config.seed_partition_spec,
        next_candidate=config.seed_start,
    )
    population = initialize_population(
        capturing_factory,
        slot_count=1,
        schedule=schedule_start,
        max_recoveries_per_episode=0,
    )
    progress_provider = BridgeDecisionProgressProvider(population.env)
    collection_policy = recovered.policies[0]
    if strategic_recovered is not None:
        anchor = FrozenCombatAnchor.from_behavior(recovered.policies[0])
        collection_policy = FrozenCombatGreedyTorchPolicy.from_categorical(
            strategic_recovered.policies[0],
            progress_provider,
            anchor,
        )
    elif isinstance(collection_policy, FrozenCombatGreedyTorchPolicy):
        collection_policy = collection_policy.bind_progress_provider(
            progress_provider
        )
    elif config.combat_decision_rule is FrozenDecisionRule.GREEDY:
        collection_policy = FrozenCombatGreedyTorchPolicy.from_categorical(
            collection_policy,
            progress_provider,
        )
    driver = OnlineBatchDriver(
        population,
        policy=collection_policy,
        curriculum=_NoRecovery(),
    )
    started = time.perf_counter()
    batch_steps = 0
    terminal_attempts = 0
    while not sink.complete and batch_steps < config.max_batch_steps:
        if (time.perf_counter() - started) * 1000 >= config.wall_ms:
            break
        result = driver.advance()
        batch_steps += 1
        terminal_attempts += len(result.attempts)
    elapsed = time.perf_counter() - started
    if not sink.complete:
        quota_progress = ""
        if sink.encounter_quota_progress:
            quota_progress = " quotas=" + ",".join(
                f"{row['encounter_id']}="
                f"{row['captured_roots']}/{row['requested_roots']}"
                for row in sink.encounter_quota_progress
            )
        raise RunCombatRootCollectionError(
            "root collection did not reach its target within the step/deadline bounds: "
            f"roots={len(sink.roots)}/{config.root_count} "
            f"batch_steps={batch_steps}/{config.max_batch_steps}"
            f"{quota_progress}"
        )

    payload = bytes(merger(sink.payloads, max_bytes=config.max_artifact_bytes))
    if not payload or len(payload) > config.max_artifact_bytes:
        raise RunCombatRootCollectionError(
            "merged combat-root artifact violates its byte bound"
        )
    with config.output.open("xb") as destination:
        destination.write(payload)
    resource_trace = tracing_factory.trace

    summary: dict[str, object] = {
        "schema": "sts-learning-run-combat-root-collection-v7",
        "behavior": str(config.behavior),
        "behavior_training_kind": behavior_kind,
        "behavior_manifest_id": recovered.manifest_id.digest.hex(),
        "collection_scope": (
            "single_publication"
            if strategic_recovered is None
            else "combat_anchor_greedy_strategic_source_sampled"
        ),
        "strategic_behavior": (
            None
            if config.strategic_behavior is None
            else str(config.strategic_behavior)
        ),
        "strategic_source_manifest_id": (
            recovered.manifest_id.digest.hex()
            if strategic_recovered is None
            else strategic_recovered.manifest_id.digest.hex()
        ),
        "strategic_source_checkpoint_id": (
            recovered.checkpoint_id.digest.hex()
            if strategic_recovered is None
            else strategic_recovered.checkpoint_id.digest.hex()
        ),
        "combat_anchor_manifest_id": (
            None
            if strategic_recovered is None
            else recovered.manifest_id.digest.hex()
        ),
        "combat_anchor_checkpoint_id": (
            None
            if strategic_recovered is None
            else recovered.checkpoint_id.digest.hex()
        ),
        "combat_decision_rule": config.combat_decision_rule.value,
        "collection_manifest_id": (
            collection_policy.behavior_manifest_id.digest.hex()
        ),
        "behavior_checkpoint_id": recovered.checkpoint_id.digest.hex(),
        "behavior_seed": config.behavior_seed,
        "ascension_level": config.ascension_level,
        "requested_run_potion_lane": config.potion_lane.value,
        "run_potion_lane": potion_lane.value,
        "seed_partition": config.seed_partition.value,
        "seed_partition_held_out_numerator": (
            config.seed_partition_spec.held_out_numerator
        ),
        "seed_partition_denominator": config.seed_partition_spec.denominator,
        "seed_start": config.seed_start,
        "seed_end": driver.schedule.next_candidate,
        "min_floor": config.min_floor,
        "max_floor": config.max_floor,
        "required_prior_combat_count": config.required_prior_combat_count,
        "min_hp_percent": config.min_hp_percent,
        "min_usable_potions": config.min_usable_potions,
        "fight_class": config.fight_class.value,
        "distinct_encounters": config.distinct_encounters,
        "encounter_quotas": sink.encounter_quota_progress,
        "required_potion_id": (
            None
            if config.required_potion is None
            else config.required_potion.potion_id
        ),
        "required_potion_slot": (
            None
            if config.required_potion is None
            else config.required_potion.slot_index
        ),
        "required_encounter_id": required_encounter_id,
        "root_count": len(sink.roots),
        "terminal_attempts": terminal_attempts,
        "batch_steps": batch_steps,
        "elapsed_seconds": elapsed,
        "artifact": str(config.output),
        "artifact_bytes": len(payload),
        "artifact_sha256": hashlib.sha256(payload).hexdigest(),
        "roots": tuple(
            {
                "seed": root.seed,
                "act": root.act,
                "floor": root.floor,
                "ascension_level": root.ascension_level,
                "hp": root.hp,
                "max_hp": root.max_hp,
                "potion_ids": root.potion_ids,
                "encounter_id": root.encounter_id,
                "monster_ids": root.monster_ids,
                "is_elite_fight": root.audit.is_elite_fight,
                "is_boss_fight": root.audit.is_boss_fight,
                "filled_potion_count": root.filled_potion_count,
                "usable_potion_count": root.usable_potion_count,
                "prior_combat_count": root.prior_combat_count,
                "deck": root.audit.as_mapping()["deck"],
                "relic_ids": root.audit.relic_ids,
                "prior_combats": _prior_combat_rows(
                    root,
                    resource_trace,
                ),
                "prior_strategic_decisions": root.prior_strategic_decisions,
            }
            for root in sink.roots
        ),
    }
    print(json.dumps(summary, separators=(",", ":"), sort_keys=True), flush=True)
    return summary


def _canonical_encounter_id(
    canonical_source: Callable[[str], object],
    encounter_id: str,
) -> str:
    try:
        canonical = canonical_source(encounter_id)
    except (TypeError, ValueError) as error:
        raise RunCombatRootCollectionError(
            "required encounter id is unsupported by the installed bridge: "
            f"{encounter_id}"
        ) from error
    if not isinstance(canonical, str) or not canonical:
        raise RunCombatRootCollectionError(
            "run bridge returned a malformed canonical encounter identity"
        )
    return canonical


def _matches_fight_class(
    audit: CombatRootAudit,
    expected: CombatFightClass,
) -> bool:
    if expected is CombatFightClass.ANY:
        return True
    if expected is CombatFightClass.BOSS:
        return audit.is_boss_fight
    if expected is CombatFightClass.ELITE:
        return audit.is_elite_fight and not audit.is_boss_fight
    return not audit.is_elite_fight and not audit.is_boss_fight


def _prior_combat_rows(
    root: CapturedRunCombatRoot,
    resource_trace: RunResourceTrace,
) -> tuple[dict[str, object], ...]:
    return tuple(
        {
            "act": transition.start.act,
            "floor": transition.start.floor,
            "start_hp": transition.start.hp,
            "end_hp": transition.end.hp,
            "max_hp": transition.end.max_hp,
            "hp_loss": transition.hp_loss,
            "start_gold": transition.start.gold,
            "end_gold": transition.end.gold,
            "start_potion_ids": transition.start.potion_ids,
            "end_potion_ids": transition.end.potion_ids,
            "encounter_id": transition.start.encounter_id,
            "monster_ids": transition.start.monster_ids,
            "terminal_reward": transition.terminal_reward,
        }
        for transition in resource_trace.completed_combats_before(
            seed=root.seed,
            act=root.act,
            floor=root.floor,
        )
    )


def _root_context_row(row: object) -> tuple[int, object]:
    if not isinstance(row, Sequence) or isinstance(row, (str, bytes)):
        raise RunCombatRootCollectionError("combat root context row must be a pair")
    if len(row) != 2:
        raise RunCombatRootCollectionError(
            "combat root context row must contain two values"
        )
    return _nonnegative(row[0], "combat root slot"), row[1]


def _root_integer(root: object, name: str, *, minimum: int) -> int:
    try:
        value = getattr(root, name)
    except AttributeError as error:
        raise RunCombatRootCollectionError(
            f"combat root context lacks {name}"
        ) from error
    normalized = _nonnegative(value, name)
    if normalized < minimum:
        raise RunCombatRootCollectionError(f"{name} must be at least {minimum}")
    return normalized


def _decision_integer_sequence(
    batch: Mapping[str, object],
    name: str,
) -> tuple[int, ...]:
    try:
        raw = batch[name]
    except KeyError as error:
        raise RunCombatRootCollectionError(
            f"decision batch lacks {name}"
        ) from error
    if not isinstance(raw, Sequence) and not hasattr(raw, "__iter__"):
        raise RunCombatRootCollectionError(
            f"decision batch {name} must be iterable"
        )
    try:
        return tuple(_nonnegative(value, f"decision batch {name}") for value in raw)
    except TypeError as error:
        raise RunCombatRootCollectionError(
            f"decision batch {name} must be iterable"
        ) from error


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise RunCombatRootCollectionError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise RunCombatRootCollectionError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise RunCombatRootCollectionError(f"{name} must be an integer") from error
    if normalized < 0:
        raise RunCombatRootCollectionError(f"{name} must be non-negative")
    return normalized


def _seed(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized >= 1 << 63:
        raise RunCombatRootCollectionError(f"{name} must be below 2^63")
    return normalized


def _encounter_quota_argument(raw: str) -> EncounterQuota:
    encounter_id, separator, raw_count = raw.partition("=")
    if not separator or not encounter_id or not raw_count:
        raise argparse.ArgumentTypeError(
            "encounter quota must use ENCOUNTER_ID=ROOT_COUNT"
        )
    try:
        root_count = int(raw_count)
        return EncounterQuota(encounter_id, root_count)
    except (ValueError, RunCombatRootCollectionError) as error:
        raise argparse.ArgumentTypeError(str(error)) from error


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Capture exact run-derived combat roots with typed resource context while "
            "a frozen behavior advances one explicit seed partition."
        )
    )
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--strategic-behavior", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--roots",
        type=int,
        help=(
            "target root count; omit when encounter quotas should determine "
            "the batch width"
        ),
    )
    parser.add_argument("--max-batch-steps", type=int, default=4096)
    parser.add_argument("--wall-ms", type=int, default=60_000)
    parser.add_argument("--behavior-seed", type=int, default=10_000)
    parser.add_argument("--seed-start", type=int, default=10_000_000)
    parser.add_argument(
        "--seed-partition",
        choices=tuple(partition.value for partition in SeedPartition),
        default=SeedPartition.TRAINING.value,
    )
    parser.add_argument("--held-out-numerator", type=int, default=1)
    parser.add_argument("--partition-denominator", type=int, default=10)
    parser.add_argument(
        "--ascension",
        type=int,
        choices=range(21),
        required=True,
    )
    parser.add_argument(
        "--combat-decision-rule",
        choices=tuple(rule.value for rule in FrozenDecisionRule),
        default=FrozenDecisionRule.GREEDY.value,
    )
    parser.add_argument("--min-floor", type=int, default=2)
    parser.add_argument("--max-floor", type=int)
    parser.add_argument("--required-prior-combats", type=int)
    parser.add_argument("--min-hp-percent", type=int, choices=range(101), default=0)
    parser.add_argument("--min-usable-potions", type=int, default=1)
    parser.add_argument(
        "--fight-class",
        choices=tuple(kind.value for kind in CombatFightClass),
        default=CombatFightClass.ANY.value,
    )
    parser.add_argument("--required-potion-id")
    parser.add_argument("--required-potion-slot", type=int)
    encounter_selector = parser.add_mutually_exclusive_group()
    encounter_selector.add_argument(
        "--required-encounter-id",
        help="capture only one canonical encounter identity",
    )
    encounter_selector.add_argument(
        "--distinct-encounters",
        action="store_true",
        help="capture at most one root per canonical encounter identity",
    )
    encounter_selector.add_argument(
        "--encounter-quota",
        action="append",
        default=[],
        type=_encounter_quota_argument,
        metavar="ENCOUNTER_ID=ROOT_COUNT",
        help=(
            "fixed target for one encounter; repeat for a mixed curriculum "
            "and omit --roots to derive the total"
        ),
    )
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in RunPotionLane),
        default=RunPotionLane.TRAINED.value,
    )
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    return parser


def main() -> int:
    parser = _parser()
    arguments = parser.parse_args()
    if (arguments.required_potion_id is None) != (
        arguments.required_potion_slot is None
    ):
        parser.error(
            "--required-potion-id and --required-potion-slot must be supplied together"
        )
    required_potion = (
        None
        if arguments.required_potion_id is None
        else RequiredPotionSlot(
            arguments.required_potion_slot,
            arguments.required_potion_id,
        )
    )
    encounter_quotas = tuple(arguments.encounter_quota)
    quota_root_count = sum(quota.root_count for quota in encounter_quotas)
    if arguments.roots is None and not encounter_quotas:
        parser.error("--roots is required unless --encounter-quota is supplied")
    if (
        arguments.roots is not None
        and encounter_quotas
        and arguments.roots != quota_root_count
    ):
        parser.error("--roots must equal the --encounter-quota total")
    root_count = (
        quota_root_count if arguments.roots is None else arguments.roots
    )
    run_run_combat_root_collection(
        RunCombatRootCollectionConfig(
            behavior=arguments.behavior,
            output=arguments.output,
            root_count=root_count,
            max_batch_steps=arguments.max_batch_steps,
            wall_ms=arguments.wall_ms,
            behavior_seed=arguments.behavior_seed,
            seed_start=arguments.seed_start,
            ascension_level=arguments.ascension,
            seed_partition=SeedPartition(arguments.seed_partition),
            seed_partition_spec=SeedPartitionSpec(
                held_out_numerator=arguments.held_out_numerator,
                denominator=arguments.partition_denominator,
            ),
            combat_decision_rule=FrozenDecisionRule(
                arguments.combat_decision_rule
            ),
            min_floor=arguments.min_floor,
            max_floor=arguments.max_floor,
            required_prior_combat_count=arguments.required_prior_combats,
            min_hp_percent=arguments.min_hp_percent,
            min_usable_potions=arguments.min_usable_potions,
            fight_class=CombatFightClass(arguments.fight_class),
            potion_lane=RunPotionLane(arguments.potion_lane),
            max_artifact_bytes=arguments.max_artifact_bytes,
            required_potion=required_potion,
            required_encounter_id=arguments.required_encounter_id,
            distinct_encounters=arguments.distinct_encounters,
            encounter_quotas=encounter_quotas,
            strategic_behavior=arguments.strategic_behavior,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
