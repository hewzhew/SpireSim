#!/usr/bin/env python3
"""Train a small behavior-cloning candidate ranker from RLDS-style exports.

This is intentionally a baseline, not an RL agent:
- one row per legal candidate action;
- label is whether the current policy selected that candidate;
- features come from observation_features_v0 and action_features_v0;
- evaluation is grouped top-1 accuracy over decision steps.

The point is to make the learning surface concrete before adding neural models.
"""

from __future__ import annotations

import argparse
import json
import pickle
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
from sklearn.feature_extraction import DictVectorizer
from sklearn.linear_model import LogisticRegression
from sklearn.pipeline import Pipeline


@dataclass
class CandidateExample:
    dataset_path: str
    episode_id: str
    step_index: int
    candidate_index: int
    selected: int
    features: dict[str, Any]
    display: dict[str, Any]


def main() -> int:
    args = parse_args()
    train_inputs, eval_inputs, manifest_ref = resolve_dataset_inputs(args)
    examples = filter_examples_by_boundary(
        collect_examples(train_inputs), args.boundary_kind
    )
    if not examples:
        raise SystemExit("no candidate examples found")

    if eval_inputs:
        train = examples
        eval_ = filter_examples_by_boundary(
            collect_examples(eval_inputs), args.boundary_kind
        )
        split_contract = "explicit_eval_input"
    else:
        train, eval_, split_contract = split_examples(
            examples,
            eval_fraction=args.eval_fraction,
            seed=args.seed,
            split_unit=args.split_unit,
        )
    if not train or not eval_:
        raise SystemExit("not enough grouped examples for train/eval split")
    if len({example.selected for example in train}) < 2:
        raise SystemExit("training examples need both selected and non-selected candidates")

    model = Pipeline(
        [
            ("features", DictVectorizer(sparse=False)),
            (
                "classifier",
                LogisticRegression(
                    max_iter=args.max_iter,
                    class_weight="balanced",
                    solver="liblinear",
                ),
            ),
        ]
    )
    model.fit([example.features for example in train], [example.selected for example in train])

    train_metrics = evaluate_grouped(model, train)
    eval_metrics = evaluate_grouped(model, eval_)
    eval_mistakes = grouped_mistakes(
        model, eval_, candidate_limit=args.mistake_candidate_limit
    )
    report = {
        "schema": "imitation_candidate_ranker_report_v0",
        "model": "sklearn.DictVectorizer+LogisticRegression",
        "manifest": manifest_ref,
        "train_input": [str(path) for path in train_inputs],
        "eval_input": [str(path) for path in eval_inputs] if eval_inputs else None,
        "boundary_filter": args.boundary_kind,
        "split_contract": split_contract,
        "example_count": len(examples),
        "train_example_count": len(train),
        "eval_example_count": len(eval_),
        "train": train_metrics,
        "eval": {
            **eval_metrics,
            "mistake_count": len(eval_mistakes),
            "mistake_summary": summarize_mistakes(eval_mistakes),
        },
        "feature_contract": "observation_features_v0 + action_features_v0; policy score/lane/rank intentionally excluded",
    }
    print(json.dumps(report, indent=2, sort_keys=True))
    if args.report_out:
        write_json(args.report_out, report)
    if args.scored_out:
        write_scored_jsonl(
            args.scored_out,
            model,
            scored_examples(args.scored_split, train, eval_),
        )
    if args.mistakes_out:
        write_jsonl(args.mistakes_out, eval_mistakes)

    if args.model_out:
        args.model_out.parent.mkdir(parents=True, exist_ok=True)
        with args.model_out.open("wb") as handle:
            pickle.dump(model, handle)
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Train a behavior-cloning candidate ranker from RLDS-style JSON exports."
    )
    parser.add_argument(
        "--input",
        type=Path,
        nargs="+",
        help="RLDS dataset JSON file(s) produced by rl_dataset_export.",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        help="Optional rl_dataset_manifest_v0. When present, train/eval inputs are read from it.",
    )
    parser.add_argument(
        "--eval-input",
        type=Path,
        nargs="+",
        help="Optional held-out RLDS dataset JSON file(s). When present, --input is train-only.",
    )
    parser.add_argument("--model-out", type=Path, help="Optional pickle output path.")
    parser.add_argument("--report-out", type=Path, help="Optional JSON report output path.")
    parser.add_argument(
        "--scored-out",
        type=Path,
        help="Optional JSONL output with one scored row per candidate.",
    )
    parser.add_argument(
        "--mistakes-out",
        type=Path,
        help="Optional JSONL output with eval decision points where model top-1 differs from policy.",
    )
    parser.add_argument("--eval-fraction", type=float, default=0.25)
    parser.add_argument(
        "--split-unit",
        choices=["episode", "step"],
        default="episode",
        help="Fallback train/eval split unit when --eval-input is omitted.",
    )
    parser.add_argument(
        "--scored-split",
        choices=["train", "eval", "all"],
        default="eval",
        help="Which split to write to --scored-out.",
    )
    parser.add_argument(
        "--boundary-kind",
        action="append",
        help="Optional boundary kind filter. May be repeated, e.g. --boundary-kind reward --boundary-kind shop.",
    )
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--max-iter", type=int, default=250)
    parser.add_argument(
        "--mistake-candidate-limit",
        type=int,
        default=8,
        help="Maximum model-ranked candidates to include per mistake row; selected candidate is always included.",
    )
    return parser.parse_args()


def resolve_dataset_inputs(
    args: argparse.Namespace,
) -> tuple[list[Path], list[Path], dict[str, Any] | None]:
    if args.manifest:
        if args.input or args.eval_input:
            raise SystemExit("--manifest cannot be combined with --input or --eval-input")
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        if manifest.get("schema") != "rl_dataset_manifest_v0":
            raise SystemExit(f"unsupported manifest schema: {manifest.get('schema')}")
        train = paths_from_manifest_split(manifest, "train")
        eval_ = paths_from_manifest_split(manifest, "eval")
        if not train:
            raise SystemExit("manifest has no train datasets")
        return train, eval_, {
            "path": str(args.manifest),
            "dataset_id": manifest.get("dataset_id"),
            "repo": manifest.get("repo"),
        }
    if not args.input:
        raise SystemExit("provide --input or --manifest")
    return args.input, args.eval_input or [], None


def paths_from_manifest_split(manifest: dict[str, Any], split: str) -> list[Path]:
    datasets = ((manifest.get("splits") or {}).get(split) or {}).get("datasets") or []
    return [Path(dataset["path"]) for dataset in datasets if dataset.get("path")]


def filter_examples_by_boundary(
    examples: list[CandidateExample],
    boundary_kinds: list[str] | None,
) -> list[CandidateExample]:
    if not boundary_kinds:
        return examples
    allowed = set(boundary_kinds)
    groups = grouped_examples(examples)
    filtered: list[CandidateExample] = []
    for group in groups.values():
        if group_boundary_kind(group) in allowed:
            filtered.extend(group)
    return filtered


def collect_examples(paths: list[Path]) -> list[CandidateExample]:
    examples: list[CandidateExample] = []
    for path in paths:
        data = json.loads(path.read_text(encoding="utf-8"))
        for episode in data.get("episodes", []):
            episode_id = str(episode.get("episode_id", path))
            episode_metadata = episode.get("episode_metadata") or {}
            for step in episode.get("steps", []):
                if step.get("is_last"):
                    continue
                metadata = step.get("step_metadata", {})
                candidates = metadata.get("action_candidates_v0") or []
                if not candidates:
                    continue
                selected_index = (step.get("action") or {}).get("index")
                observation = metadata.get("observation_features_v0") or {}
                group_features = metadata.get("candidate_group_features_v0") or {}
                for candidate in candidates:
                    candidate_index = candidate.get("index")
                    action = candidate.get("features_v0") or {}
                    features = flatten_candidate_features(
                        observation=observation,
                        action=action,
                        group=group_features,
                    )
                    examples.append(
                        CandidateExample(
                            dataset_path=str(path),
                            episode_id=episode_id,
                            step_index=int(metadata.get("t", 0)),
                            candidate_index=int(candidate_index),
                            selected=int(candidate_index == selected_index),
                            features=features,
                            display=candidate_display(
                                observation=observation,
                                action=action,
                                candidate=candidate,
                                episode_metadata=episode_metadata,
                            ),
                        )
                    )
    return examples


def flatten_candidate_features(
    *,
    observation: dict[str, Any],
    action: dict[str, Any],
    group: dict[str, Any],
) -> dict[str, Any]:
    features: dict[str, Any] = {}
    for key in [
        "act",
        "floor",
        "floors_to_act_boss",
        "hp",
        "max_hp",
        "hp_ratio_bp",
        "gold",
        "deck_size",
        "relic_count",
        "potion_count",
    ]:
        add_numeric(features, f"obs.{key}", observation.get(key))
    add_categorical(features, "obs.boundary_kind", observation.get("boundary_kind"))
    add_categorical(features, "obs.boss", observation.get("boss"))
    for boss_id in observation.get("boss_list") or []:
        add_numeric(features, f"obs.boss_list.{boss_id}", 1)

    for card_id, count in sorted((observation.get("deck_card_counts") or {}).items()):
        add_numeric(features, f"obs.deck.{card_id}", count)
    for card_type, count in sorted((observation.get("deck_type_counts") or {}).items()):
        add_numeric(features, f"obs.deck_type.{card_type}", count)
    for rarity, count in sorted((observation.get("deck_rarity_counts") or {}).items()):
        add_numeric(features, f"obs.deck_rarity.{rarity}", count)
    for tag, count in sorted((observation.get("deck_tag_counts") or {}).items()):
        add_numeric(features, f"obs.deck_tag.{tag}", count)
    for relic_id in observation.get("relic_ids") or []:
        add_numeric(features, f"obs.relic.{relic_id}", 1)
    for potion_id in observation.get("potion_ids") or []:
        add_numeric(features, f"obs.potion.{potion_id}", 1)

    add_categorical(features, "act.kind", action.get("kind"))
    for key in ["card_id", "relic_id", "potion_id", "event_id"]:
        add_categorical(features, f"act.{key}", action.get(key))
    for key in ["option_index", "reward_item_index", "shop_slot", "price", "deck_index", "upgrades"]:
        add_numeric(features, f"act.{key}", action.get(key))
    for key in ["is_skip", "is_buy", "is_remove", "is_pick", "is_leave", "is_event_option"]:
        add_numeric(features, f"act.{key}", int(bool(action.get(key))))
    add_card_definition_features(features, observation=observation, action=action)
    add_group_features(features, action=action, group=group)
    return features


def add_card_definition_features(
    features: dict[str, Any],
    *,
    observation: dict[str, Any],
    action: dict[str, Any],
) -> None:
    card_definition = action.get("card_definition") or {}
    if not isinstance(card_definition, dict):
        return
    add_categorical(features, "act.card_type", card_definition.get("type"))
    add_categorical(features, "act.card_rarity", card_definition.get("rarity"))
    add_categorical(features, "act.card_target", card_definition.get("target"))
    for key in [
        "cost",
        "base_damage",
        "base_block",
        "base_magic",
        "upgrade_damage",
        "upgrade_block",
        "upgrade_magic",
    ]:
        add_numeric(features, f"act.card_{key}", card_definition.get(key))
    for key in ["is_multi_damage", "exhaust", "ethereal", "innate"]:
        add_numeric(features, f"act.card_{key}", int(bool(card_definition.get(key))))
    for tag in card_definition.get("tags") or []:
        if isinstance(tag, str):
            features[f"act.card_tag.{tag}"] = 1

    card_id = action.get("card_id")
    if isinstance(card_id, str):
        add_numeric(
            features,
            "rel.same_card_count_in_deck",
            (observation.get("deck_card_counts") or {}).get(card_id, 0),
        )
        add_numeric(
            features,
            "rel.same_card_upgraded_count_in_deck",
            (observation.get("upgraded_deck_card_counts") or {}).get(card_id, 0),
        )

    card_type = card_definition.get("type")
    if isinstance(card_type, str):
        add_numeric(
            features,
            "rel.same_type_count_in_deck",
            (observation.get("deck_type_counts") or {}).get(card_type, 0),
        )
    rarity = card_definition.get("rarity")
    if isinstance(rarity, str):
        add_numeric(
            features,
            "rel.same_rarity_count_in_deck",
            (observation.get("deck_rarity_counts") or {}).get(rarity, 0),
        )
    for tag in card_definition.get("tags") or []:
        if isinstance(tag, str):
            add_numeric(
                features,
                f"rel.same_tag_count_in_deck.{tag}",
                (observation.get("deck_tag_counts") or {}).get(tag, 0),
            )


def add_group_features(
    features: dict[str, Any],
    *,
    action: dict[str, Any],
    group: dict[str, Any],
) -> None:
    for key in [
        "candidate_count",
        "auto_expand_count",
        "inspect_only_count",
        "affordable_buy_count",
        "price_count",
        "price_min",
        "price_max",
        "price_mean",
    ]:
        add_numeric(features, f"group.{key}", group.get(key))
    for key in ["has_skip", "has_leave", "has_remove", "has_buy", "has_pick"]:
        add_numeric(features, f"group.{key}", int(bool(group.get(key))))

    kind_counts = group.get("kind_counts") or {}
    action_kind = action.get("kind")
    if isinstance(action_kind, str):
        kind_count = kind_counts.get(action_kind)
        add_numeric(features, "group.action_kind_count", kind_count)
        if isinstance(kind_count, int):
            features["group.action_kind_is_unique"] = int(kind_count == 1)

    price = action.get("price")
    if isinstance(price, (int, float)):
        price_min = group.get("price_min")
        price_mean = group.get("price_mean")
        if isinstance(price_min, (int, float)):
            features["rel.price_minus_group_min"] = price - price_min
        if isinstance(price_mean, (int, float)):
            features["rel.price_minus_group_mean"] = price - price_mean


def add_numeric(features: dict[str, Any], key: str, value: Any) -> None:
    if value is None:
        return
    if isinstance(value, bool):
        features[key] = int(value)
        return
    if isinstance(value, (int, float)):
        features[key] = value


def add_categorical(features: dict[str, Any], key: str, value: Any) -> None:
    if value is None:
        return
    if isinstance(value, str):
        features[key] = value


def candidate_display(
    *,
    observation: dict[str, Any],
    action: dict[str, Any],
    candidate: dict[str, Any],
    episode_metadata: dict[str, Any],
) -> dict[str, Any]:
    card_definition = action.get("card_definition") or {}
    if not isinstance(card_definition, dict):
        card_definition = {}
    final_outcome = episode_metadata.get("final_outcome") or {}
    if not isinstance(final_outcome, dict):
        final_outcome = {}
    final_status = final_outcome.get("status") or {}
    if not isinstance(final_status, dict):
        final_status = {}
    combat_summary = episode_metadata.get("episode_combat_summary") or {}
    if not isinstance(combat_summary, dict):
        combat_summary = {}
    return {
        "context": {
            "act": observation.get("act"),
            "floor": observation.get("floor"),
            "hp": observation.get("hp"),
            "max_hp": observation.get("max_hp"),
            "gold": observation.get("gold"),
            "deck_size": observation.get("deck_size"),
            "boundary_kind": observation.get("boundary_kind"),
            "boss": observation.get("boss"),
            "final_act": final_outcome.get("act"),
            "final_floor": final_outcome.get("floor"),
            "final_hp": final_outcome.get("hp"),
            "final_max_hp": final_outcome.get("max_hp"),
            "final_blocker_kind": final_outcome.get("blocker_kind"),
            "final_status_kind": final_status.get("kind"),
            "high_hp_loss_attempt_count": len(
                combat_summary.get("high_hp_loss_attempts") or []
            ),
        },
        "candidate": {
            "label": candidate.get("label"),
            "kind": action.get("kind"),
            "card_id": action.get("card_id"),
            "card_type": card_definition.get("type"),
            "card_rarity": card_definition.get("rarity"),
            "card_cost": card_definition.get("cost"),
            "relic_id": action.get("relic_id"),
            "potion_id": action.get("potion_id"),
            "event_id": action.get("event_id"),
            "price": action.get("price"),
            "option_index": action.get("option_index"),
            "rank": candidate.get("rank"),
            "lane": candidate.get("lane"),
            "policy_score": candidate.get("score"),
            "auto_expand": candidate.get("auto_expand"),
            "inspect_only": candidate.get("inspect_only"),
        },
    }


def split_examples(
    examples: list[CandidateExample],
    *,
    eval_fraction: float,
    seed: int,
    split_unit: str,
) -> tuple[list[CandidateExample], list[CandidateExample], str]:
    if split_unit == "episode" and len({example.episode_id for example in examples}) >= 2:
        train, eval_ = split_by_group_key(
            examples,
            eval_fraction=eval_fraction,
            seed=seed,
            group_key=lambda example: (example.dataset_path, example.episode_id),
        )
        return train, eval_, "episode_holdout_random"
    train, eval_ = split_by_group_key(
        examples,
        eval_fraction=eval_fraction,
        seed=seed,
            group_key=lambda example: (
                example.dataset_path,
                example.episode_id,
                str(example.step_index),
            ),
    )
    return train, eval_, "step_holdout_random"


def split_by_group_key(
    examples: list[CandidateExample],
    *,
    eval_fraction: float,
    seed: int,
    group_key,
) -> tuple[list[CandidateExample], list[CandidateExample]]:
    rng = np.random.default_rng(seed)
    groups: dict[tuple[str, ...], list[CandidateExample]] = {}
    for example in examples:
        groups.setdefault(group_key(example), []).append(example)
    keys = list(groups)
    rng.shuffle(keys)
    eval_count = max(1, int(round(len(keys) * eval_fraction)))
    eval_keys = set(keys[:eval_count])
    train: list[CandidateExample] = []
    eval_: list[CandidateExample] = []
    for key, group in groups.items():
        (eval_ if key in eval_keys else train).extend(group)
    return train, eval_


def scored_examples(
    scored_split: str,
    train: list[CandidateExample],
    eval_: list[CandidateExample],
) -> list[CandidateExample]:
    if scored_split == "train":
        return train
    if scored_split == "all":
        return [*train, *eval_]
    return eval_


def evaluate_grouped(model: Pipeline, examples: list[CandidateExample]) -> dict[str, Any]:
    groups = grouped_examples(examples)

    correct = 0
    scored_steps = 0
    for group in groups.values():
        group_correct = group_top1_correct(model, group)
        if group_correct is None:
            continue
        correct += int(group_correct)
        scored_steps += 1

    return {
        "decision_step_count": scored_steps,
        "candidate_count": len(examples),
        "top1_accuracy": (correct / scored_steps) if scored_steps else None,
        "by_boundary_kind": evaluate_by_boundary_kind(model, examples),
    }


def group_top1_correct(model: Pipeline, group: list[CandidateExample]) -> bool | None:
    if not any(example.selected for example in group):
        return None
    scores = model.predict_proba([example.features for example in group])[:, 1]
    chosen = group[int(np.argmax(scores))]
    return chosen.selected == 1


def evaluate_by_boundary_kind(
    model: Pipeline, examples: list[CandidateExample]
) -> list[dict[str, Any]]:
    groups_by_boundary: dict[str, list[list[CandidateExample]]] = {}
    for group in grouped_examples(examples).values():
        boundary = group_boundary_kind(group)
        groups_by_boundary.setdefault(boundary, []).append(group)

    rows = []
    for boundary, groups in sorted(groups_by_boundary.items()):
        correct = 0
        scored_steps = 0
        candidate_count = 0
        for group in groups:
            candidate_count += len(group)
            group_correct = group_top1_correct(model, group)
            if group_correct is None:
                continue
            correct += int(group_correct)
            scored_steps += 1
        rows.append(
            {
                "boundary_kind": boundary,
                "decision_step_count": scored_steps,
                "candidate_count": candidate_count,
                "top1_accuracy": (correct / scored_steps) if scored_steps else None,
            }
        )
    rows.sort(key=lambda row: row["decision_step_count"], reverse=True)
    return rows


def group_boundary_kind(group: list[CandidateExample]) -> str:
    selected = next((example for example in group if example.selected), group[0])
    context = selected.display.get("context") or {}
    return str(context.get("boundary_kind"))


def grouped_mistakes(
    model: Pipeline,
    examples: list[CandidateExample],
    *,
    candidate_limit: int,
) -> list[dict[str, Any]]:
    mistakes: list[dict[str, Any]] = []
    for key, group in grouped_examples(examples).items():
        if not any(example.selected for example in group):
            continue
        scored = score_group(model, group)
        predicted = scored[0]
        selected = next(row for row in scored if row["selected"])
        if predicted["selected"]:
            continue
        mistakes.append(
            {
                "dataset_path": key[0],
                "episode_id": key[1],
                "step_index": key[2],
                "context": selected["display"]["context"],
                "selected": selected,
                "predicted": predicted,
                "candidates_by_model_score": limited_scored_candidates(
                    scored, candidate_limit
                ),
            }
        )
    return mistakes


def summarize_mistakes(mistakes: list[dict[str, Any]]) -> dict[str, Any]:
    by_boundary = Counter()
    by_kind_pair = Counter()
    by_label_pair = Counter()
    by_final_blocker = Counter()
    predicted_kind = Counter()
    selected_kind = Counter()
    skip_pressure = Counter()
    for row in mistakes:
        context = row.get("context") or {}
        selected = ((row.get("selected") or {}).get("display") or {}).get("candidate") or {}
        predicted = ((row.get("predicted") or {}).get("display") or {}).get("candidate") or {}
        by_boundary[str(context.get("boundary_kind"))] += 1
        by_final_blocker[str(context.get("final_blocker_kind"))] += 1
        selected_kind[str(selected.get("kind"))] += 1
        predicted_kind[str(predicted.get("kind"))] += 1
        by_kind_pair[f"{selected.get('kind')} -> {predicted.get('kind')}"] += 1
        by_label_pair[f"{selected.get('label')} -> {predicted.get('label')}"] += 1
        if predicted.get("kind") in {"CardRewardSkip", "ShopLeave"}:
            skip_pressure[str(selected.get("label"))] += 1
    return {
        "by_boundary_kind": counter_top(by_boundary),
        "by_final_blocker_kind": counter_top(by_final_blocker),
        "selected_kind": counter_top(selected_kind),
        "predicted_kind": counter_top(predicted_kind),
        "selected_to_predicted_kind": counter_top(by_kind_pair),
        "selected_to_predicted_label": counter_top(by_label_pair, limit=12),
        "skip_pressure_selected_label": counter_top(skip_pressure, limit=12),
        "high_confidence_mistakes": high_confidence_mistakes(mistakes, limit=12),
    }


def counter_top(counter: Counter, limit: int = 10) -> list[dict[str, Any]]:
    return [
        {"key": key, "count": count}
        for key, count in counter.most_common(limit)
    ]


def high_confidence_mistakes(
    mistakes: list[dict[str, Any]], *, limit: int
) -> list[dict[str, Any]]:
    rows = []
    for row in mistakes:
        selected = row.get("selected") or {}
        predicted = row.get("predicted") or {}
        selected_display = (selected.get("display") or {}).get("candidate") or {}
        predicted_display = (predicted.get("display") or {}).get("candidate") or {}
        predicted_prob = float(predicted.get("model_prob_selected") or 0.0)
        selected_prob = float(selected.get("model_prob_selected") or 0.0)
        rows.append(
            {
                "episode_id": row.get("episode_id"),
                "step_index": row.get("step_index"),
                "context": row.get("context"),
                "selected_label": selected_display.get("label"),
                "selected_kind": selected_display.get("kind"),
                "predicted_label": predicted_display.get("label"),
                "predicted_kind": predicted_display.get("kind"),
                "predicted_prob": predicted_prob,
                "selected_prob": selected_prob,
                "prob_margin": predicted_prob - selected_prob,
            }
        )
    rows.sort(key=lambda item: item["prob_margin"], reverse=True)
    return rows[:limit]


def limited_scored_candidates(
    scored: list[dict[str, Any]], candidate_limit: int
) -> list[dict[str, Any]]:
    if candidate_limit <= 0:
        candidate_limit = 1
    limited = scored[:candidate_limit]
    selected = next((row for row in scored if row["selected"]), None)
    if selected is not None and all(
        row["candidate_index"] != selected["candidate_index"] for row in limited
    ):
        limited.append(selected)
    return limited


def write_scored_jsonl(path: Path, model: Pipeline, examples: list[CandidateExample]) -> None:
    rows: list[dict[str, Any]] = []
    for group in grouped_examples(examples).values():
        rows.extend(score_group(model, group))
    write_jsonl(path, rows)


def score_group(model: Pipeline, group: list[CandidateExample]) -> list[dict[str, Any]]:
    probabilities = model.predict_proba([example.features for example in group])[:, 1]
    rows = []
    for example, probability in zip(group, probabilities):
        rows.append(
            {
                "episode_id": example.episode_id,
                "dataset_path": example.dataset_path,
                "step_index": example.step_index,
                "candidate_index": example.candidate_index,
                "selected": bool(example.selected),
                "model_prob_selected": float(probability),
                "display": example.display,
            }
        )
    rows.sort(key=lambda row: row["model_prob_selected"], reverse=True)
    return rows


def grouped_examples(
    examples: list[CandidateExample],
) -> dict[tuple[str, str, int], list[CandidateExample]]:
    groups: dict[tuple[str, str, int], list[CandidateExample]] = {}
    for example in examples:
        groups.setdefault(
            (example.dataset_path, example.episode_id, example.step_index), []
        ).append(example)
    return groups


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
