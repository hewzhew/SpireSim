"""Live complete-turn proposal carriage over coherent captured combat runs.

Every aligned teacher successor is replay-verified by Rust. Unselected
candidates remain unknown: this module records a positive ordinal when one is
present but never manufactures negative labels or an execution preference.
"""

from __future__ import annotations

import json
import operator
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Any

from .complete_turn_run_capture import CompleteTurnRunCapture


class CompleteTurnTeacherAggregationError(ValueError):
    """A live carriage environment violated the complete-turn contract."""


@dataclass(frozen=True)
class CompleteTurnTeacherRow:
    combat_ordinal: int
    turn_ordinal: int
    stage: str
    atomic_depth: int | None
    candidate_count: int
    max_retained_states: int
    positive_ordinal: int | None
    reserved_ordinals: tuple[int, ...]
    batch: Mapping[str, object]
    public_view: Mapping[str, object] | None = None

    @property
    def positive_was_reserved(self) -> bool | None:
        if self.positive_ordinal is None:
            return None
        return self.positive_ordinal in self.reserved_ordinals


@dataclass(frozen=True)
class CompleteTurnTeacherCarriageFailure:
    combat_ordinal: int
    turn_ordinal: int
    stage: str
    atomic_depth: int | None
    reason: str


@dataclass(frozen=True)
class CompleteTurnTeacherAggregation:
    run_seed: int
    learner_controlled: bool
    rows: tuple[CompleteTurnTeacherRow, ...]
    completed_combat_ordinals: tuple[int, ...]
    failures: tuple[CompleteTurnTeacherCarriageFailure, ...]

    @property
    def all_combats_closed(self) -> bool:
        return not self.failures


CompleteTurnEnvFactory = Callable[..., Any]
ReservationPolicy = Callable[[str, Mapping[str, object]], Sequence[int]]


def aggregate_complete_turn_teacher_carriage(
    capture: CompleteTurnRunCapture,
    *,
    environment_factory: CompleteTurnEnvFactory | None = None,
    reservation_policy: ReservationPolicy | None = None,
    allow_potions: bool = True,
    max_inner_nodes: int = 256,
    max_partial_frontier_states: int = 32,
    max_end_states: int = 24,
    per_bucket_limit: int = 24,
    include_public_views: bool = False,
) -> CompleteTurnTeacherAggregation:
    """Align one coherent run's winning turns on live bounded surfaces.

    With no reservation policy, the verified positive is reserved at each
    layer to measure teacher carriage. With a policy, its reservations are
    applied without revealing the positive; the returned positive rows and
    first missing successor form learner-induced aggregation evidence.
    """

    if not isinstance(capture, CompleteTurnRunCapture):
        raise CompleteTurnTeacherAggregationError(
            "capture must be a CompleteTurnRunCapture"
        )
    factory = environment_factory or _native_environment_factory()
    rows: list[CompleteTurnTeacherRow] = []
    failures: list[CompleteTurnTeacherCarriageFailure] = []
    completed_combats: list[int] = []
    for combat in capture.combats:
        try:
            env = factory(
                combat.case_payload,
                allow_potions=allow_potions,
                max_inner_nodes=max_inner_nodes,
                max_partial_frontier_states=max_partial_frontier_states,
                max_end_states=max_end_states,
                per_bucket_limit=per_bucket_limit,
            )
        except Exception as error:
            raise CompleteTurnTeacherAggregationError(
                f"combat {combat.ordinal} environment construction failed: {error}"
            ) from error
        combat_failed = False
        for turn_ordinal, turn in enumerate(combat.turns):
            while True:
                batch = env.prefix_layer_batch()
                if batch is None:
                    break
                depth = _positive_int(batch.get("atomic_depth"), "atomic_depth")
                candidate_count = _one_row_candidate_count(batch)
                capacity = _positive_int(
                    batch.get("max_retained_states"), "max_retained_states"
                )
                positive = None
                # The full turn is removed from the pending-prefix layer and
                # appears in the completed-endpoint layer below. Only strict
                # prefixes carry a positive in this stage.
                if depth < len(turn.input_prefixes_json):
                    try:
                        positive = env.matching_prefix_successor_ordinal(
                            turn.input_prefixes_json[depth - 1]
                        )
                    except Exception as error:
                        raise CompleteTurnTeacherAggregationError(
                            f"combat {combat.ordinal} turn {turn_ordinal} prefix alignment failed: {error}"
                        ) from error
                    if positive is None:
                        failures.append(
                            CompleteTurnTeacherCarriageFailure(
                                combat_ordinal=combat.ordinal,
                                turn_ordinal=turn_ordinal,
                                stage="prefix",
                                atomic_depth=depth,
                                reason="verified successor is absent from the generated prefix layer",
                            )
                        )
                        combat_failed = True
                        break
                reservations = _reservations(
                    reservation_policy,
                    "prefix",
                    batch,
                    positive,
                    candidate_count,
                    capacity,
                )
                rows.append(
                    CompleteTurnTeacherRow(
                        combat_ordinal=combat.ordinal,
                        turn_ordinal=turn_ordinal,
                        stage="prefix",
                        atomic_depth=depth,
                        candidate_count=candidate_count,
                        max_retained_states=capacity,
                        positive_ordinal=positive,
                        reserved_ordinals=reservations,
                        batch=batch,
                        public_view=_public_view(env, "prefix")
                        if include_public_views and positive is not None
                        else None,
                    )
                )
                try:
                    env.retain_prefix_layer_with_default_fill(list(reservations))
                except Exception as error:
                    raise CompleteTurnTeacherAggregationError(
                        f"combat {combat.ordinal} turn {turn_ordinal} prefix retention failed: {error}"
                    ) from error
            if combat_failed:
                break

            endpoint_batch = env.completed_endpoint_batch()
            endpoint_count = _one_row_candidate_count(endpoint_batch)
            endpoint_capacity = _positive_int(
                endpoint_batch.get("max_retained_states"), "max_retained_states"
            )
            try:
                endpoint_positive = env.matching_completed_endpoint_successor_ordinal(
                    turn.inputs_json
                )
            except Exception as error:
                raise CompleteTurnTeacherAggregationError(
                    f"combat {combat.ordinal} turn {turn_ordinal} endpoint alignment failed: {error}"
                ) from error
            if endpoint_positive is None:
                failures.append(
                    CompleteTurnTeacherCarriageFailure(
                        combat_ordinal=combat.ordinal,
                        turn_ordinal=turn_ordinal,
                        stage="completed_endpoint",
                        atomic_depth=None,
                        reason="verified successor is absent before endpoint selection",
                    )
                )
                combat_failed = True
                break
            endpoint_reservations = _reservations(
                reservation_policy,
                "completed_endpoint",
                endpoint_batch,
                endpoint_positive,
                endpoint_count,
                endpoint_capacity,
            )
            rows.append(
                CompleteTurnTeacherRow(
                    combat_ordinal=combat.ordinal,
                    turn_ordinal=turn_ordinal,
                    stage="completed_endpoint",
                    atomic_depth=None,
                    candidate_count=endpoint_count,
                    max_retained_states=endpoint_capacity,
                    positive_ordinal=endpoint_positive,
                    reserved_ordinals=endpoint_reservations,
                    batch=endpoint_batch,
                    public_view=_public_view(env, "completed_endpoint")
                    if include_public_views
                    else None,
                )
            )
            try:
                env.retain_completed_endpoints_with_default_fill(
                    list(endpoint_reservations)
                )
                env.decision_batch()
                execution_ordinal = env.matching_successor_ordinal(turn.inputs_json)
            except Exception as error:
                raise CompleteTurnTeacherAggregationError(
                    f"combat {combat.ordinal} turn {turn_ordinal} execution-surface alignment failed: {error}"
                ) from error
            if execution_ordinal is None:
                failures.append(
                    CompleteTurnTeacherCarriageFailure(
                        combat_ordinal=combat.ordinal,
                        turn_ordinal=turn_ordinal,
                        stage="execution_surface",
                        atomic_depth=None,
                        reason="verified successor did not survive bounded endpoint selection",
                    )
                )
                combat_failed = True
                break
            try:
                env.choose(execution_ordinal)
            except Exception as error:
                raise CompleteTurnTeacherAggregationError(
                    f"combat {combat.ordinal} turn {turn_ordinal} exact choice failed: {error}"
                ) from error
        if combat_failed:
            continue
        if not env.is_terminal or env.terminal != "win":
            failures.append(
                CompleteTurnTeacherCarriageFailure(
                    combat_ordinal=combat.ordinal,
                    turn_ordinal=len(combat.turns),
                    stage="terminal",
                    atomic_depth=None,
                    reason=f"teacher line ended at {env.terminal}, expected win",
                )
            )
            continue
        completed_combats.append(combat.ordinal)
    return CompleteTurnTeacherAggregation(
        run_seed=capture.seed,
        learner_controlled=reservation_policy is not None,
        rows=tuple(rows),
        completed_combat_ordinals=tuple(completed_combats),
        failures=tuple(failures),
    )


def _native_environment_factory() -> CompleteTurnEnvFactory:
    try:
        from sts_learning_bridge import CompleteTurnExactEnv
    except ImportError as error:
        raise CompleteTurnTeacherAggregationError(
            "the installed sts_learning_bridge is required for live teacher carriage"
        ) from error
    return CompleteTurnExactEnv.from_case_bytes


def _reservations(
    policy: ReservationPolicy | None,
    stage: str,
    batch: Mapping[str, object],
    positive: int | None,
    candidate_count: int,
    capacity: int,
) -> tuple[int, ...]:
    raw = [positive] if policy is None and positive is not None else []
    if policy is not None:
        raw = list(policy(stage, batch))
    reservations = tuple(
        _non_negative_int(value, f"{stage} reservation ordinal") for value in raw
    )
    if len(set(reservations)) != len(reservations):
        raise CompleteTurnTeacherAggregationError(
            f"{stage} reservation ordinals must be unique"
        )
    if len(reservations) > capacity:
        raise CompleteTurnTeacherAggregationError(
            f"{stage} reservations exceed the retained capacity"
        )
    if any(ordinal >= candidate_count for ordinal in reservations):
        raise CompleteTurnTeacherAggregationError(
            f"{stage} reservation ordinal is outside the candidate row"
        )
    return reservations


def _public_view(env: Any, stage: str) -> Mapping[str, object]:
    method_name = {
        "prefix": "prefix_layer_public_view_json",
        "completed_endpoint": "completed_endpoint_public_view_json",
    }.get(stage)
    if method_name is None:
        raise CompleteTurnTeacherAggregationError(
            f"unsupported complete-turn public-view stage {stage}"
        )
    method = getattr(env, method_name, None)
    if not callable(method):
        raise CompleteTurnTeacherAggregationError(
            f"complete-turn environment does not expose {method_name}"
        )
    try:
        encoded = method()
    except Exception as error:
        raise CompleteTurnTeacherAggregationError(
            f"{stage} public view failed: {error}"
        ) from error
    if not isinstance(encoded, bytes) or not encoded:
        raise CompleteTurnTeacherAggregationError(
            f"{stage} public view must be non-empty JSON bytes"
        )
    try:
        decoded = json.loads(encoded)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise CompleteTurnTeacherAggregationError(
            f"{stage} public view is not valid JSON: {error}"
        ) from error
    if not isinstance(decoded, Mapping):
        raise CompleteTurnTeacherAggregationError(
            f"{stage} public view must decode to an object"
        )
    return decoded


def _one_row_candidate_count(batch: Mapping[str, object]) -> int:
    counts = batch.get("candidate_counts")
    if counts is None:
        raise CompleteTurnTeacherAggregationError(
            "complete-turn batch is missing candidate_counts"
        )
    try:
        if len(counts) != 1:  # type: ignore[arg-type]
            raise CompleteTurnTeacherAggregationError(
                "complete-turn batch must contain exactly one row"
            )
        return _positive_int(counts[0], "candidate_count")  # type: ignore[index]
    except TypeError as error:
        raise CompleteTurnTeacherAggregationError(
            "complete-turn candidate_counts must be an indexable sequence"
        ) from error


def _non_negative_int(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CompleteTurnTeacherAggregationError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CompleteTurnTeacherAggregationError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CompleteTurnTeacherAggregationError(f"{name} must be non-negative")
    return normalized


def _positive_int(value: object, name: str) -> int:
    normalized = _non_negative_int(value, name)
    if normalized == 0:
        raise CompleteTurnTeacherAggregationError(f"{name} must be positive")
    return normalized
