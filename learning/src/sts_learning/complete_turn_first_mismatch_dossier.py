"""Human-readable first-mismatch audits for complete-turn proposal scorers.

The dossier replays a coherent captured run under one frozen proposal model.
It stops each combat at the first exact teacher-successor miss and explains the
last verified positive that fell outside the model's bounded reservation. The
unselected candidates remain unknown; the report is a proposal-recall audit,
not a combat-value label set.
"""

from __future__ import annotations

import argparse
import json
import operator
from collections import Counter
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any

import torch

from .complete_turn_prefix_batch import (
    complete_turn_completed_endpoint_decision_batch,
    complete_turn_prefix_decision_batch,
)
from .complete_turn_prefix_learning import topk_prefix_reservations
from .complete_turn_run_capture import CompleteTurnRunCapture, load_complete_turn_run_capture
from .complete_turn_teacher_aggregation import (
    CompleteTurnTeacherAggregation,
    CompleteTurnTeacherRow,
    aggregate_complete_turn_teacher_carriage,
)
from .torch_policy import RaggedCandidateScorer, RaggedScorerConfig


REPORT_SCHEMA = "complete-turn-first-mismatch-dossier-v1"


class CompleteTurnFirstMismatchDossierError(ValueError):
    """Frozen scorer evidence could not be reconstructed consistently."""


def build_complete_turn_first_mismatch_dossier(
    capture: CompleteTurnRunCapture,
    *,
    scorer: RaggedCandidateScorer,
    checkpoint_metadata: Mapping[str, object],
    allow_potions: bool,
    max_inner_nodes: int,
    max_partial_frontier_states: int,
    max_end_states: int,
    per_bucket_limit: int,
    top_candidate_limit: int = 8,
    model_reservation_limit: int | None = None,
) -> dict[str, object]:
    """Run one frozen scorer and summarize each combat's first exact miss."""

    if not isinstance(capture, CompleteTurnRunCapture):
        raise CompleteTurnFirstMismatchDossierError(
            "capture must be a CompleteTurnRunCapture"
        )
    if not isinstance(scorer, RaggedCandidateScorer):
        raise CompleteTurnFirstMismatchDossierError(
            "scorer must be a RaggedCandidateScorer"
        )
    limit = _positive_int(top_candidate_limit, "top_candidate_limit")
    reservation_limit = (
        None
        if model_reservation_limit is None
        else _positive_int(model_reservation_limit, "model_reservation_limit")
    )
    scorer.eval()

    def reservation_policy(stage: str, batch: Mapping[str, object]) -> Sequence[int]:
        decision_batch = _decision_batch(stage, batch)
        candidate_count = _candidate_count(batch)
        capacity = min(
            _positive_int(batch.get("max_retained_states"), "max_retained_states"),
            candidate_count,
        )
        if reservation_limit is not None:
            capacity = min(capacity, reservation_limit)
        with torch.no_grad():
            logits = scorer(decision_batch)
        return topk_prefix_reservations(logits, [capacity])[0]

    aggregation = aggregate_complete_turn_teacher_carriage(
        capture,
        reservation_policy=reservation_policy,
        allow_potions=allow_potions,
        max_inner_nodes=max_inner_nodes,
        max_partial_frontier_states=max_partial_frontier_states,
        max_end_states=max_end_states,
        per_bucket_limit=per_bucket_limit,
        include_public_views=True,
    )
    misses = _first_mismatches(
        capture,
        aggregation,
        scorer,
        top_candidate_limit=limit,
    )
    gaps = [float(miss["positive_score_minus_cutoff"]) for miss in misses]
    return {
        "schema": REPORT_SCHEMA,
        "source_manifest": str(capture.manifest_path),
        "run_seed": capture.seed,
        "source_curriculum_eligible": capture.curriculum_eligible,
        "source_curriculum_ineligibility": capture.curriculum_ineligibility,
        "checkpoint": dict(checkpoint_metadata),
        "proposal_config": {
            "allow_potions": allow_potions,
            "max_inner_nodes": max_inner_nodes,
            "max_partial_frontier_states": max_partial_frontier_states,
            "max_end_states": max_end_states,
            "per_bucket_limit": per_bucket_limit,
            "model_reservation_limit": reservation_limit,
        },
        "combat_count": len(capture.combats),
        "completed_combat_count": len(aggregation.completed_combat_ordinals),
        "failure_count": len(aggregation.failures),
        "first_mismatch_count": len(misses),
        "first_mismatch_stage_counts": dict(
            sorted(Counter(str(miss["first_absent_stage"]) for miss in misses).items())
        ),
        "score_gap_summary": _number_summary(gaps),
        "misses": misses,
    }


def render_complete_turn_first_mismatch_markdown(report: Mapping[str, object]) -> str:
    """Render the compact audit intended for joint human/model review."""

    if report.get("schema") != REPORT_SCHEMA:
        raise CompleteTurnFirstMismatchDossierError(
            f"expected {REPORT_SCHEMA} report"
        )
    misses = _sequence(report.get("misses"), "misses")
    stage_counts = _mapping(report.get("first_mismatch_stage_counts"), "stage counts")
    lines = [
        "# Complete-turn first-mismatch dossier",
        "",
        f"- Run seed: `{report.get('run_seed')}`",
        f"- Frozen scorer closed combats: `{report.get('completed_combat_count')}/{report.get('combat_count')}`",
        f"- First exact misses: `{report.get('first_mismatch_count')}` ({', '.join(f'{key}={value}' for key, value in stage_counts.items())})",
        "- Interpretation: every listed teacher prefix is replay-verified; every unchosen candidate remains unknown.",
        "",
        "| # | Encounter | Floor | Turn | Lost row | First absent | Rank / model slots | Score gap |",
        "|---:|---|---:|---:|---|---|---:|---:|",
    ]
    for raw_miss in misses:
        miss = _mapping(raw_miss, "miss")
        depth = miss.get("lost_atomic_depth")
        lost_row = str(miss.get("lost_stage")) + (f" d{depth}" if depth is not None else "")
        absent_depth = miss.get("first_absent_atomic_depth")
        absent = str(miss.get("first_absent_stage")) + (
            f" d{absent_depth}" if absent_depth is not None else ""
        )
        lines.append(
            f"| {miss.get('combat_ordinal')} | {miss.get('encounter_id')} | "
            f"{miss.get('act')}/{miss.get('floor')} | {miss.get('turn_ordinal')} | "
            f"{lost_row} | {absent} | {miss.get('positive_rank')}/{miss.get('model_reservation_count')} | "
            f"{float(miss.get('positive_score_minus_cutoff', 0.0)):+.4f} |"
        )

    for raw_miss in misses:
        miss = _mapping(raw_miss, "miss")
        lines.extend(
            [
                "",
                f"## {miss.get('combat_ordinal')}. {miss.get('encounter_id')} — Act/Floor {miss.get('act')}/{miss.get('floor')}, turn {miss.get('turn_ordinal')}",
                "",
                f"- Root: {miss.get('root_summary')}",
                f"- Teacher: `{miss.get('teacher_actions')}`",
                f"- Teacher successor: {miss.get('teacher_result')}",
                f"- First divergence: teacher was rank {miss.get('positive_rank')} at "
                f"{miss.get('lost_stage')}"
                + (
                    f" depth {miss.get('lost_atomic_depth')}"
                    if miss.get("lost_atomic_depth") is not None
                    else ""
                )
                + f", outside the model's {miss.get('model_reservation_count')} reservations "
                + f"inside frontier K={miss.get('frontier_capacity')}; its exact successor first became absent at {miss.get('first_absent_stage')}"
                + (
                    f" depth {miss.get('first_absent_atomic_depth')}."
                    if miss.get("first_absent_atomic_depth") is not None
                    else "."
                ),
                f"- Scores: teacher={float(miss.get('positive_score', 0.0)):+.4f}, "
                f"cutoff={float(miss.get('reservation_cutoff_score', 0.0)):+.4f}, "
                f"gap={float(miss.get('positive_score_minus_cutoff', 0.0)):+.4f}.",
            ]
        )
        lines.extend(["", "Model top-K:", ""])
        for raw_candidate in _sequence(miss.get("model_top_candidates"), "top candidates"):
            candidate = _mapping(raw_candidate, "top candidate")
            lines.append(
                f"- #{candidate.get('rank')} ordinal {candidate.get('ordinal')} "
                f"score {float(candidate.get('score', 0.0)):+.4f}: "
                f"`{candidate.get('actions')}` → {candidate.get('result')}"
            )
    return "\n".join(lines) + "\n"


def _first_mismatches(
    capture: CompleteTurnRunCapture,
    aggregation: CompleteTurnTeacherAggregation,
    scorer: RaggedCandidateScorer,
    *,
    top_candidate_limit: int,
) -> list[dict[str, object]]:
    combats = {combat.ordinal: combat for combat in capture.combats}
    misses: list[dict[str, object]] = []
    for failure in aggregation.failures:
        combat = combats.get(failure.combat_ordinal)
        if combat is None:
            raise CompleteTurnFirstMismatchDossierError(
                f"failure references unknown combat {failure.combat_ordinal}"
            )
        candidate_rows = [
            row
            for row in aggregation.rows
            if row.combat_ordinal == failure.combat_ordinal
            and row.positive_was_reserved is False
        ]
        if not candidate_rows:
            raise CompleteTurnFirstMismatchDossierError(
                f"combat {failure.combat_ordinal} failed without an unreserved verified positive"
            )
        lost = candidate_rows[-1]
        scored = _score_row(scorer, lost)
        public_view = _mapping(lost.public_view, "lost-row public view")
        view_candidates = _sequence(public_view.get("candidates"), "view candidates")
        if len(view_candidates) != lost.candidate_count:
            raise CompleteTurnFirstMismatchDossierError(
                f"combat {failure.combat_ordinal} public view candidate count drifted"
            )
        assert lost.positive_ordinal is not None
        teacher_candidate = _mapping(
            view_candidates[lost.positive_ordinal], "teacher candidate"
        )
        root = _root_state(public_view, lost.stage)
        top_rows = []
        for rank, ordinal in enumerate(scored["ordered_ordinals"], start=1):
            if rank > top_candidate_limit or ordinal not in lost.reserved_ordinals:
                continue
            candidate = _mapping(view_candidates[ordinal], "top candidate")
            top_rows.append(
                {
                    "rank": rank,
                    "ordinal": ordinal,
                    "score": scored["scores"][ordinal],
                    "actions": _actions_summary(candidate),
                    "result": _candidate_result_summary(candidate, lost.stage),
                }
            )
        misses.append(
            {
                "combat_ordinal": combat.ordinal,
                "encounter_id": combat.encounter_id,
                "act": combat.act,
                "floor": combat.floor,
                "turn_ordinal": failure.turn_ordinal,
                "lost_stage": lost.stage,
                "lost_atomic_depth": lost.atomic_depth,
                "first_absent_stage": failure.stage,
                "first_absent_atomic_depth": failure.atomic_depth,
                "first_absent_reason": failure.reason,
                "candidate_count": lost.candidate_count,
                "frontier_capacity": lost.max_retained_states,
                "model_reservation_count": len(lost.reserved_ordinals),
                "positive_ordinal": lost.positive_ordinal,
                "positive_rank": scored["positive_rank"],
                "positive_score": scored["positive_score"],
                "reservation_cutoff_score": scored["cutoff_score"],
                "positive_score_minus_cutoff": scored["score_gap"],
                "root_summary": _state_summary(root),
                "teacher_actions": _actions_summary(teacher_candidate),
                "teacher_result": _candidate_result_summary(teacher_candidate, lost.stage),
                "model_top_candidates": top_rows,
            }
        )
    return misses


def _score_row(
    scorer: RaggedCandidateScorer, row: CompleteTurnTeacherRow
) -> dict[str, Any]:
    with torch.no_grad():
        logits = scorer(_decision_batch(row.stage, row.batch))
    scores = [float(value) for value in logits.values.detach().cpu().tolist()]
    if len(scores) != row.candidate_count:
        raise CompleteTurnFirstMismatchDossierError("scorer candidate count drifted")
    assert row.positive_ordinal is not None
    ordered = sorted(range(len(scores)), key=lambda ordinal: (-scores[ordinal], ordinal))
    rank = ordered.index(row.positive_ordinal) + 1
    if not row.reserved_ordinals:
        raise CompleteTurnFirstMismatchDossierError(
            "learner row has no reservations and therefore no cutoff"
        )
    cutoff = min(scores[ordinal] for ordinal in row.reserved_ordinals)
    positive_score = scores[row.positive_ordinal]
    return {
        "scores": scores,
        "ordered_ordinals": ordered,
        "positive_rank": rank,
        "positive_score": positive_score,
        "cutoff_score": cutoff,
        "score_gap": positive_score - cutoff,
    }


def _root_state(view: Mapping[str, object], stage: str) -> Mapping[str, object]:
    root = _mapping(view.get("root"), "public view root")
    if stage == "prefix":
        return _mapping(root.get("observation"), "prefix root observation")
    return root


def _actions_summary(candidate: Mapping[str, object]) -> str:
    actions = _sequence(candidate.get("actions"), "candidate actions")
    return " -> ".join(_action_label(_mapping(action, "candidate action")) for action in actions)


def _action_label(action: Mapping[str, object]) -> str:
    outer_kind = str(action.get("kind", "unknown"))
    if outer_kind == "selection":
        family = _mapping(action.get("family"), "selection family")
        domain = _sequence(family.get("domain"), "selection domain")
        selected = _sequence(
            action.get("selected_domain_ordinals"), "selected domain ordinals"
        )
        labels: list[str] = []
        for raw_ordinal in selected:
            ordinal = _non_negative_int(raw_ordinal, "selected domain ordinal")
            if ordinal >= len(domain):
                raise CompleteTurnFirstMismatchDossierError(
                    "selected domain ordinal is out of range"
                )
            item = _mapping(domain[ordinal], "selected domain item")
            labels.append(
                _card_label(item)
                if item.get("kind") == "card"
                else str(item.get("kind", "item")) + f"#{ordinal}"
            )
        reason = family.get("reason")
        reason_label = None
        if isinstance(reason, Mapping):
            reason_label = reason.get("value") or reason.get("kind")
        return f"Select[{','.join(labels)}]" + (
            f"({reason_label})" if reason_label else ""
        )
    if outer_kind != "atomic":
        return outer_kind + ":" + json.dumps(action, sort_keys=True, separators=(",", ":"))
    atomic = _mapping(action.get("action"), "atomic action")
    kind = str(atomic.get("kind", "unknown"))
    target = action.get("target")
    target_label = None
    if isinstance(target, Mapping):
        enemy = target.get("enemy")
        if isinstance(enemy, Mapping):
            target_label = enemy.get("enemy_id")
        target_label = target_label or f"slot{target.get('slot')}"
    if kind == "play_card":
        card = action.get("card")
        label = _card_label(card) if isinstance(card, Mapping) else "unknown_card"
        return f"{label}->{target_label}" if target_label else label
    if kind in {"use_potion", "discard_potion"}:
        potion = action.get("potion")
        potion_id = potion.get("potion_id") if isinstance(potion, Mapping) else "unknown_potion"
        label = f"{kind}:{potion_id}"
        return f"{label}->{target_label}" if target_label else label
    if kind == "end_turn":
        return "EndTurn"
    details = [f"{key}={value}" for key, value in atomic.items() if key != "kind"]
    return kind + ("(" + ",".join(details) + ")" if details else "")


def _candidate_result_summary(candidate: Mapping[str, object], stage: str) -> str:
    if stage == "prefix":
        current = _mapping(candidate.get("current"), "prefix current")
        state = _mapping(current.get("observation"), "prefix current observation")
        boundary = _mapping(current.get("boundary"), "prefix current boundary").get("kind")
    else:
        state = _mapping(candidate.get("successor"), "endpoint successor")
        boundary = candidate.get("boundary")
    return f"{_state_result_summary(state)}; boundary={boundary}"


def _state_summary(state: Mapping[str, object]) -> str:
    turn = _mapping(state.get("turn"), "combat turn")
    player = _mapping(state.get("player"), "combat player")
    cards = _mapping(state.get("cards"), "combat cards")
    hand = _card_collection(cards.get("hand"))
    master = _card_collection(cards.get("master_deck"))
    deck_counts = Counter(_card_label(card) for card in master)
    deck = ", ".join(
        f"{label}x{count}" if count > 1 else label
        for label, count in sorted(deck_counts.items())
    )
    relics = ", ".join(
        str(_mapping(relic, "relic").get("id"))
        for relic in _sequence(player.get("relics"), "relics")
    )
    potions = ", ".join(
        "empty" if potion is None else str(_mapping(potion, "potion").get("potion_id"))
        for potion in _sequence(state.get("potions"), "potions")
    )
    monsters = "; ".join(
        _monster_summary(_mapping(monster, "monster"))
        for monster in _sequence(state.get("monsters"), "monsters")
    )
    return (
        f"HP {player.get('hp')}/{player.get('max_hp')}, block {player.get('block')}, "
        f"energy {turn.get('energy')}; hand [{', '.join(_card_label(card) for card in hand)}]; "
        f"enemies [{monsters}]; potions [{potions}]; relics [{relics}]; deck [{deck}]"
    )


def _state_result_summary(state: Mapping[str, object]) -> str:
    turn = _mapping(state.get("turn"), "combat turn")
    player = _mapping(state.get("player"), "combat player")
    monsters = ", ".join(
        f"{_enemy_id(_mapping(monster, 'monster'))}:{_mapping(monster, 'monster').get('hp')}"
        for monster in _sequence(state.get("monsters"), "monsters")
        if _mapping(monster, "monster").get("alive")
    )
    return (
        f"HP {player.get('hp')}, block {player.get('block')}, energy {turn.get('energy')}, "
        f"alive [{monsters}]"
    )


def _monster_summary(monster: Mapping[str, object]) -> str:
    intent_container = _mapping(monster.get("intent"), "monster intent")
    intent = intent_container.get("intent")
    if isinstance(intent, Mapping) and len(intent) == 1:
        name, payload = next(iter(intent.items()))
        if isinstance(payload, Mapping) and "damage" in payload:
            hits = payload.get("hits", 1)
            intent_label = f"{name} {payload.get('damage')}x{hits}"
        else:
            intent_label = str(name)
    else:
        intent_label = str(intent)
    return (
        f"{_enemy_id(monster)} {monster.get('hp')}/{monster.get('max_hp')} "
        f"block {monster.get('block')} intent {intent_label}"
    )


def _enemy_id(monster: Mapping[str, object]) -> object:
    enemy = monster.get("enemy")
    return enemy.get("enemy_id") if isinstance(enemy, Mapping) else f"slot{monster.get('slot')}"


def _card_collection(value: object) -> list[Mapping[str, object]]:
    collection = _mapping(value, "card collection")
    return [_mapping(card, "card") for card in _sequence(collection.get("cards"), "cards")]


def _card_label(card: Mapping[str, object]) -> str:
    upgrades = _non_negative_int(card.get("upgrades", 0), "card upgrades")
    return str(card.get("card_id")) + (f"+{upgrades}" if upgrades else "")


def _decision_batch(stage: str, batch: Mapping[str, object]) -> dict[str, object]:
    if stage == "prefix":
        return complete_turn_prefix_decision_batch(batch)
    if stage == "completed_endpoint":
        return complete_turn_completed_endpoint_decision_batch(batch)
    raise CompleteTurnFirstMismatchDossierError(f"unsupported scorer stage {stage}")


def _candidate_count(batch: Mapping[str, object]) -> int:
    counts = batch.get("candidate_counts")
    try:
        if len(counts) != 1:  # type: ignore[arg-type]
            raise CompleteTurnFirstMismatchDossierError(
                "complete-turn batch must contain one candidate count"
            )
        value = counts[0]  # type: ignore[index]
    except TypeError as error:
        raise CompleteTurnFirstMismatchDossierError(
            "complete-turn batch must contain one candidate count"
        ) from error
    return _positive_int(value, "candidate_count")


def _load_checkpoint(
    checkpoint_path: Path,
) -> tuple[RaggedCandidateScorer, dict[str, object], dict[str, int]]:
    try:
        payload = torch.load(checkpoint_path, map_location="cpu", weights_only=True)
    except (OSError, RuntimeError, ValueError) as error:
        raise CompleteTurnFirstMismatchDossierError(
            f"failed to load checkpoint {checkpoint_path}: {error}"
        ) from error
    checkpoint = _mapping(payload, "checkpoint")
    hidden_dim = _positive_int(checkpoint.get("hidden_dim"), "hidden_dim")
    relation_layers = _non_negative_int(
        checkpoint.get("relation_layers"), "relation_layers"
    )
    limits = _mapping(checkpoint.get("proposal_limits"), "proposal_limits")
    normalized_limits = {
        name: _positive_int(limits.get(name), name)
        for name in (
            "max_inner_nodes",
            "max_partial_frontier_states",
            "max_end_states",
            "per_bucket_limit",
        )
    }
    try:
        from sts_learning_bridge import semantic_schema
    except ImportError as error:
        raise CompleteTurnFirstMismatchDossierError(
            "the installed sts_learning_bridge is required"
        ) from error
    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema(),
        RaggedScorerConfig(hidden_dim=hidden_dim, relation_layers=relation_layers),
    )
    state_dict = checkpoint.get("state_dict")
    if not isinstance(state_dict, Mapping):
        raise CompleteTurnFirstMismatchDossierError("checkpoint state_dict is missing")
    try:
        scorer.load_state_dict(state_dict, strict=True)
    except RuntimeError as error:
        raise CompleteTurnFirstMismatchDossierError(
            f"checkpoint does not match its declared scorer: {error}"
        ) from error
    metadata = {
        key: value
        for key, value in checkpoint.items()
        if key != "state_dict" and isinstance(value, (str, int, float, bool, list, dict, type(None)))
    }
    metadata["path"] = str(checkpoint_path.resolve())
    return scorer, metadata, normalized_limits


def _number_summary(values: Sequence[float]) -> dict[str, float | int | None]:
    if not values:
        return {"count": 0, "min": None, "median": None, "max": None}
    ordered = sorted(values)
    middle = len(ordered) // 2
    median = (
        ordered[middle]
        if len(ordered) % 2
        else (ordered[middle - 1] + ordered[middle]) / 2.0
    )
    return {
        "count": len(ordered),
        "min": ordered[0],
        "median": median,
        "max": ordered[-1],
    }


def _mapping(value: object, name: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise CompleteTurnFirstMismatchDossierError(f"{name} must be an object")
    return value


def _sequence(value: object, name: str) -> Sequence[Any]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes, bytearray)):
        raise CompleteTurnFirstMismatchDossierError(f"{name} must be a sequence")
    return value


def _non_negative_int(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CompleteTurnFirstMismatchDossierError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CompleteTurnFirstMismatchDossierError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CompleteTurnFirstMismatchDossierError(f"{name} must be non-negative")
    return normalized


def _positive_int(value: object, name: str) -> int:
    normalized = _non_negative_int(value, name)
    if normalized == 0:
        raise CompleteTurnFirstMismatchDossierError(f"{name} must be positive")
    return normalized


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Explain the first complete-turn proposal miss in each captured combat."
    )
    parser.add_argument("--capture-manifest", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--markdown-output", type=Path)
    parser.add_argument("--allow-potions", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--top-candidates", type=int, default=8)
    parser.add_argument(
        "--model-reservations",
        type=int,
        help="reserve at most this many model top scores and leave the remaining frontier to typed default fill",
    )
    return parser.parse_args()


def main() -> None:
    args = _parse_args()
    scorer, checkpoint_metadata, limits = _load_checkpoint(args.checkpoint)
    capture = load_complete_turn_run_capture(args.capture_manifest)
    report = build_complete_turn_first_mismatch_dossier(
        capture,
        scorer=scorer,
        checkpoint_metadata=checkpoint_metadata,
        allow_potions=args.allow_potions,
        top_candidate_limit=args.top_candidates,
        model_reservation_limit=args.model_reservations,
        **limits,
    )
    _write_new_text(
        args.output,
        json.dumps(report, indent=2, ensure_ascii=False) + "\n",
    )
    if args.markdown_output is not None:
        _write_new_text(
            args.markdown_output,
            render_complete_turn_first_mismatch_markdown(report),
        )
    print(
        f"first mismatches: {report['first_mismatch_count']}; "
        f"closed combats: {report['completed_combat_count']}/{report['combat_count']}"
    )
    print(f"report: {args.output.resolve()}")
    if args.markdown_output is not None:
        print(f"markdown: {args.markdown_output.resolve()}")


def _write_new_text(path: Path, content: str) -> None:
    try:
        with path.open("x", encoding="utf-8", newline="\n") as output:
            output.write(content)
    except FileExistsError as error:
        raise CompleteTurnFirstMismatchDossierError(
            f"refusing to overwrite existing report {path}"
        ) from error


if __name__ == "__main__":
    main()
