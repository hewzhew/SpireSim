#!/usr/bin/env python3
"""Fit a diagnostic complete-turn successor ranker.

The corpus keeps exact wins, exact refutations, terminal non-wins, and bounded
unknowns separate. Only exact wins versus exact non-wins form training pairs;
BudgetUnknown candidates are scored for ranking but never become negatives.

The optional verified-corridor imitation objective instead learns the weaker
statement "prefer the demonstrated replay-verified corridor edge". Other
candidates remain outcome-unknown; imitation never relabels them as losses.

This is deliberately an offline representation and supervision test. It does
not emit a Rust runtime policy and it does not authorize production search
changes.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

import numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler


@dataclass(frozen=True)
class Candidate:
    group_id: str
    split: str
    policy_rank: int
    exact_hash: str
    evidence: str
    is_verified_successor: bool
    features: np.ndarray

    @property
    def exact_label(self) -> int | None:
        if self.evidence == "exact_win":
            return 1
        if self.evidence in {"exact_refutation", "exact_terminal_non_win"}:
            return 0
        return None


@dataclass
class Group:
    group_id: str
    split: str
    verified_hash: str
    verified_policy_rank: int | None
    candidates: list[Candidate]


def main() -> int:
    args = parse_args()
    corpus = json.loads(args.input.read_text(encoding="utf-8"))
    if corpus.get("schema_name") not in {
        "BoundarySuccessorCorpusV1",
        "BoundarySuccessorCorpusV2",
    }:
        raise SystemExit(f"unsupported corpus schema: {corpus.get('schema_name')}")
    feature_set = args.feature_set
    if feature_set == "auto":
        feature_set = (
            "semantic"
            if corpus.get("semantic_feature_schema")
            else "guide"
        )
    groups = load_groups(corpus, feature_set)
    train_groups = [group for group in groups if group.split == "train"]
    eval_groups = [group for group in groups if group.split == "eval"]
    x, y, pair_count = pairwise_examples(train_groups, args.training_objective)
    if pair_count == 0:
        raise SystemExit(
            "training groups contain no pairs for "
            f"{args.training_objective}"
        )

    model = fit_model(x, y, args.c)
    training_rankings = [rank_group(model, group) for group in train_groups]
    leave_one_out = leave_one_group_out(
        train_groups, args.c, args.training_objective
    )
    evaluation_rankings = [rank_group(model, group) for group in eval_groups]
    report = {
        "schema_name": "BoundarySuccessorRankerReportV1",
        "schema_version": 1,
        "authority": "offline_diagnostic_only",
        "input": str(args.input),
        "source_identity": corpus.get("source_identity"),
        "input_fingerprint": corpus.get("input_fingerprint"),
        "corpus_config": corpus.get("config"),
        "feature_set": feature_set,
        "feature_schema": (
            corpus.get("semantic_feature_schema")
            if feature_set == "semantic"
            else corpus.get("guide_feature_schema", corpus.get("feature_schema"))
        ),
        "training_objective": args.training_objective,
        "training_contract": training_contract(args.training_objective),
        "training": {
            "groups": len(train_groups),
            "pair_count": pair_count,
            "candidate_evidence": evidence_counts(train_groups),
            "group_rankings": training_rankings,
            "leave_one_group_out": leave_one_out,
        },
        "evaluation": {
            "groups": len(eval_groups),
            "candidate_evidence": evidence_counts(eval_groups),
            "group_rankings": evaluation_rankings,
        },
        "offline_gate": offline_gate(
            leave_one_out,
            evaluation_rankings,
            args.training_objective,
        ),
        "limitations": [
            (
                "semantic features retain ordered card zones and concrete combat facts but "
                "exclude raw RNG identity"
                if feature_set == "semantic"
                else "guide features are composed from existing handwritten guide components"
            ),
            "a few adjacent boundaries from one exact fight are a representation probe, not generalization evidence",
            "unresolved successors may also be winning and are never scored as incorrect labels",
            (
                "verified-corridor imitation expresses a demonstrated expansion preference; "
                "it does not prove that non-demonstrated successors lose or that the "
                "demonstration is optimal"
                if args.training_objective == "verified-corridor-imitation"
                else "exact-outcome pairs remain sparse at early boundaries where bounded "
                "search cannot refute alternative successors"
            ),
            "the model is not loadable by production search",
        ],
    }
    printed = summary_report(report) if args.summary_only else report
    print(json.dumps(printed, indent=2, sort_keys=True))
    if args.report_out:
        args.report_out.parent.mkdir(parents=True, exist_ok=True)
        args.report_out.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Train an offline pairwise complete-turn successor ranker."
    )
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--report-out", type=Path)
    parser.add_argument(
        "--summary-only",
        action="store_true",
        help="Print only gates and compact boundary ranks; --report-out remains complete.",
    )
    parser.add_argument(
        "--feature-set",
        choices=("auto", "guide", "semantic"),
        default="auto",
        help="State representation used by the offline diagnostic ranker.",
    )
    parser.add_argument(
        "--training-objective",
        choices=("exact-outcome", "verified-corridor-imitation"),
        default="exact-outcome",
        help=(
            "exact-outcome learns only exact win/non-win pairs; "
            "verified-corridor-imitation learns the demonstrated edge over "
            "other sampled candidates without changing their outcome evidence"
        ),
    )
    parser.add_argument(
        "--c",
        type=float,
        default=0.1,
        help="Inverse L2 regularization strength for the diagnostic linear model.",
    )
    return parser.parse_args()


def summary_report(report: dict[str, Any]) -> dict[str, Any]:
    def compact(ranking: dict[str, Any]) -> dict[str, Any]:
        return {
            "group_id": ranking.get("group_id"),
            "verified_policy_rank": ranking.get("verified_policy_rank"),
            "verified_model_rank": ranking.get("verified_model_rank"),
            "known_pair_count": ranking.get("known_pair_count"),
            "model_pairwise_accuracy": ranking.get("model_pairwise_accuracy"),
            "policy_pairwise_accuracy": ranking.get("policy_pairwise_accuracy"),
            "known_ordering_pass": ranking.get("known_ordering_pass"),
            "demonstrated_pair_count": ranking.get(
                "demonstrated_pair_count"
            ),
            "model_demonstrated_accuracy": ranking.get(
                "model_demonstrated_accuracy"
            ),
            "policy_demonstrated_accuracy": ranking.get(
                "policy_demonstrated_accuracy"
            ),
            "demonstrated_ordering_pass": ranking.get(
                "demonstrated_ordering_pass"
            ),
        }

    leave_one_out = []
    for item in report["training"]["leave_one_group_out"]:
        compact_item: dict[str, Any] = {
            "held_out_group": item["held_out_group"],
            "status": item["status"],
        }
        if item.get("ranking"):
            compact_item["ranking"] = compact(item["ranking"])
        leave_one_out.append(compact_item)
    return {
        "schema_name": "BoundarySuccessorRankerSummaryV1",
        "schema_version": 1,
        "feature_set": report["feature_set"],
        "feature_schema": report["feature_schema"],
        "training_objective": report["training_objective"],
        "offline_gate": report["offline_gate"],
        "training": {
            "groups": report["training"]["groups"],
            "pair_count": report["training"]["pair_count"],
            "leave_one_group_out": leave_one_out,
        },
        "evaluation": {
            "groups": report["evaluation"]["groups"],
            "group_rankings": [
                compact(ranking)
                for ranking in report["evaluation"]["group_rankings"]
            ],
        },
    }


def load_groups(corpus: dict[str, Any], feature_set: str) -> list[Group]:
    groups: list[Group] = []
    semantic_names = (
        sorted(
            {
                str(feature["name"])
                for raw_group in corpus.get("groups", [])
                for raw_candidate in raw_group.get("candidates", [])
                for feature in raw_candidate.get(
                    "successor_semantic_features", []
                )
            }
        )
        if feature_set == "semantic"
        else []
    )
    if feature_set == "semantic" and not semantic_names:
        raise SystemExit(
            "semantic feature set requested but corpus has no successor_semantic_features"
        )
    semantic_index = {name: index for index, name in enumerate(semantic_names)}
    feature_width: int | None = (
        len(semantic_names) if feature_set == "semantic" else None
    )
    for raw_group in corpus.get("groups", []):
        group_id = str(raw_group["id"])
        split = str(raw_group["split"])
        candidates: list[Candidate] = []
        for raw_candidate in raw_group.get("candidates", []):
            if feature_set == "semantic":
                features = np.zeros((len(semantic_names),), dtype=float)
                for feature in raw_candidate["successor_semantic_features"]:
                    features[semantic_index[str(feature["name"])]] = float(
                        feature["value"]
                    )
                features = squash(features)
            else:
                features = squash(
                    np.asarray(raw_candidate["successor_features"], dtype=float)
                )
            if feature_width is None:
                feature_width = int(features.shape[0])
            if features.shape != (feature_width,):
                raise SystemExit(
                    f"candidate {group_id}/{raw_candidate['policy_rank']} has "
                    f"feature width {features.shape}, expected {(feature_width,)}"
                )
            candidates.append(
                Candidate(
                    group_id=group_id,
                    split=split,
                    policy_rank=int(raw_candidate["policy_rank"]),
                    exact_hash=str(raw_candidate["exact_successor_hash"]),
                    evidence=str(raw_candidate["evidence"]["kind"]),
                    is_verified_successor=(
                        str(raw_candidate["exact_successor_hash"])
                        == str(
                            raw_group[
                                "verified_successor_exact_state_hash"
                            ]
                        )
                    ),
                    features=features,
                )
            )
        groups.append(
            Group(
                group_id=group_id,
                split=split,
                verified_hash=str(
                    raw_group["verified_successor_exact_state_hash"]
                ),
                verified_policy_rank=raw_group.get(
                    "verified_successor_policy_rank"
                ),
                candidates=candidates,
            )
        )
    if not groups:
        raise SystemExit("corpus has no groups")
    return groups


def squash(features: np.ndarray) -> np.ndarray:
    return np.clip(np.arcsinh(features) / 8.0, -1.0, 1.0)


def pairwise_examples(
    groups: Iterable[Group],
    objective: str,
) -> tuple[np.ndarray, np.ndarray, int]:
    rows: list[np.ndarray] = []
    labels: list[int] = []
    pairs = 0
    for group in groups:
        positives, negatives = preference_sides(group, objective)
        for positive in positives:
            for negative in negatives:
                difference = positive.features - negative.features
                rows.extend([difference, -difference])
                labels.extend([1, 0])
                pairs += 1
    if not rows:
        width = next(
            (
                candidate.features.shape[0]
                for group in groups
                for candidate in group.candidates
            ),
            0,
        )
        return np.empty((0, width), dtype=float), np.empty((0,), dtype=int), 0
    return np.stack(rows), np.asarray(labels, dtype=int), pairs


def preference_sides(
    group: Group, objective: str
) -> tuple[list[Candidate], list[Candidate]]:
    if objective == "exact-outcome":
        return (
            [
                candidate
                for candidate in group.candidates
                if candidate.exact_label == 1
            ],
            [
                candidate
                for candidate in group.candidates
                if candidate.exact_label == 0
            ],
        )
    if objective == "verified-corridor-imitation":
        return (
            [
                candidate
                for candidate in group.candidates
                if candidate.is_verified_successor
            ],
            [
                candidate
                for candidate in group.candidates
                if not candidate.is_verified_successor
            ],
        )
    raise ValueError(f"unsupported training objective: {objective}")


def training_contract(objective: str) -> str:
    if objective == "exact-outcome":
        return (
            "pairwise exact-win versus exact-refutation-or-terminal-non-win; "
            "BudgetUnknown excluded from labels"
        )
    if objective == "verified-corridor-imitation":
        return (
            "pairwise replay-verified corridor successor versus other sampled "
            "successors; alternatives retain their original outcome evidence "
            "and are not asserted to lose"
        )
    raise ValueError(f"unsupported training objective: {objective}")


def fit_model(x: np.ndarray, y: np.ndarray, c: float) -> Pipeline:
    if c <= 0.0:
        raise SystemExit("--c must be positive")
    model = Pipeline(
        [
            ("scale", StandardScaler()),
            (
                "ranker",
                LogisticRegression(
                    C=c,
                    max_iter=1_000,
                    solver="liblinear",
                    random_state=0,
                ),
            ),
        ]
    )
    model.fit(x, y)
    return model


def rank_group(model: Pipeline, group: Group) -> dict[str, Any]:
    if not group.candidates:
        return {
            "group_id": group.group_id,
            "verified_policy_rank": group.verified_policy_rank,
            "verified_model_rank": None,
            "candidates": [],
        }
    scores = model.decision_function(
        np.stack([candidate.features for candidate in group.candidates])
    )
    ranked = sorted(
        zip(group.candidates, scores, strict=True),
        key=lambda pair: (-float(pair[1]), pair[0].policy_rank),
    )
    verified_model_rank = next(
        (
            index
            for index, (candidate, _) in enumerate(ranked, start=1)
            if candidate.exact_hash == group.verified_hash
        ),
        None,
    )
    score_by_hash = {
        candidate.exact_hash: float(score)
        for candidate, score in zip(group.candidates, scores, strict=True)
    }
    positives = [
        candidate for candidate in group.candidates if candidate.exact_label == 1
    ]
    negatives = [
        candidate for candidate in group.candidates if candidate.exact_label == 0
    ]
    known_pairs = [(positive, negative) for positive in positives for negative in negatives]
    model_pairwise_accuracy = pairwise_accuracy(
        known_pairs,
        lambda candidate: score_by_hash[candidate.exact_hash],
        higher_is_better=True,
    )
    policy_pairwise_accuracy = pairwise_accuracy(
        known_pairs,
        lambda candidate: float(candidate.policy_rank),
        higher_is_better=False,
    )
    demonstrated = [
        candidate
        for candidate in group.candidates
        if candidate.is_verified_successor
    ]
    alternatives = [
        candidate
        for candidate in group.candidates
        if not candidate.is_verified_successor
    ]
    demonstrated_pairs = [
        (positive, negative)
        for positive in demonstrated
        for negative in alternatives
    ]
    model_demonstrated_accuracy = pairwise_accuracy(
        demonstrated_pairs,
        lambda candidate: score_by_hash[candidate.exact_hash],
        higher_is_better=True,
    )
    policy_demonstrated_accuracy = pairwise_accuracy(
        demonstrated_pairs,
        lambda candidate: float(candidate.policy_rank),
        higher_is_better=False,
    )
    return {
        "group_id": group.group_id,
        "verified_policy_rank": group.verified_policy_rank,
        "verified_model_rank": verified_model_rank,
        "known_candidate_count": len(group.candidates),
        "known_pair_count": len(known_pairs),
        "model_pairwise_accuracy": model_pairwise_accuracy,
        "policy_pairwise_accuracy": policy_pairwise_accuracy,
        "known_ordering_pass": (
            None
            if not known_pairs
            else all(
                score_by_hash[positive.exact_hash]
                > score_by_hash[negative.exact_hash]
                for positive, negative in known_pairs
            )
        ),
        "demonstrated_pair_count": len(demonstrated_pairs),
        "model_demonstrated_accuracy": model_demonstrated_accuracy,
        "policy_demonstrated_accuracy": policy_demonstrated_accuracy,
        "demonstrated_ordering_pass": (
            None
            if not demonstrated_pairs
            else all(
                score_by_hash[positive.exact_hash]
                > score_by_hash[negative.exact_hash]
                for positive, negative in demonstrated_pairs
            )
        ),
        "ranked_candidates": [
            {
                "model_rank": index,
                "policy_rank": candidate.policy_rank,
                "evidence": candidate.evidence,
                "is_verified_successor": candidate.exact_hash == group.verified_hash,
                "score": float(score),
            }
            for index, (candidate, score) in enumerate(ranked, start=1)
        ],
    }


def pairwise_accuracy(
    pairs: list[tuple[Candidate, Candidate]],
    value: Any,
    *,
    higher_is_better: bool,
) -> float | None:
    if not pairs:
        return None
    credit = 0.0
    for positive, negative in pairs:
        positive_value = float(value(positive))
        negative_value = float(value(negative))
        if positive_value == negative_value:
            credit += 0.5
        elif (positive_value > negative_value) == higher_is_better:
            credit += 1.0
    return credit / len(pairs)


def offline_gate(
    leave_one_out: list[dict[str, Any]],
    evaluation_rankings: list[dict[str, Any]],
    objective: str,
) -> dict[str, Any]:
    held_out_rankings = [
        item["ranking"]
        for item in leave_one_out
        if item.get("status") == "scored" and item.get("ranking")
    ]
    held_out_complete = len(held_out_rankings) == len(leave_one_out)
    if objective == "exact-outcome":
        held_out_pass = held_out_complete and all(
            ranking.get("known_ordering_pass") is True
            and ranking.get("model_pairwise_accuracy", 0.0)
            >= ranking.get("policy_pairwise_accuracy", 1.0)
            for ranking in held_out_rankings
        )
        evaluation_has_pairs = bool(evaluation_rankings) and all(
            ranking.get("known_pair_count", 0) > 0
            for ranking in evaluation_rankings
        )
        evaluation_pass = evaluation_has_pairs and all(
            ranking.get("known_ordering_pass") is True
            and ranking.get("model_pairwise_accuracy", 0.0)
            >= ranking.get("policy_pairwise_accuracy", 1.0)
            for ranking in evaluation_rankings
        )
        pair_requirement = "exact_win_non_win"
        missing_evaluation_status = "blocked_missing_exact_eval_pairs"
        requirements = {
            "all_training_boundaries_scored_when_held_out": held_out_complete,
            "all_held_out_known_win_non_win_pairs_ordered_correctly": held_out_pass,
            "all_evaluation_boundaries_have_exact_win_non_win_pairs": (
                evaluation_has_pairs
            ),
            "all_evaluation_known_pairs_ordered_correctly": evaluation_pass,
        }
    elif objective == "verified-corridor-imitation":
        held_out_pass = held_out_complete and all(
            ranking.get("demonstrated_ordering_pass") is True
            and ranking.get("model_demonstrated_accuracy", 0.0)
            >= ranking.get("policy_demonstrated_accuracy", 1.0)
            for ranking in held_out_rankings
        )
        evaluation_has_pairs = bool(evaluation_rankings) and all(
            ranking.get("demonstrated_pair_count", 0) > 0
            for ranking in evaluation_rankings
        )
        evaluation_pass = evaluation_has_pairs and all(
            ranking.get("demonstrated_ordering_pass") is True
            and ranking.get("model_demonstrated_accuracy", 0.0)
            >= ranking.get("policy_demonstrated_accuracy", 1.0)
            for ranking in evaluation_rankings
        )
        pair_requirement = "verified_corridor_alternative"
        missing_evaluation_status = "blocked_missing_evaluation_pairs"
        requirements = {
            "all_training_boundaries_scored_when_held_out": held_out_complete,
            "all_held_out_preference_pairs_ordered_correctly": held_out_pass,
            "all_evaluation_boundaries_have_required_pairs": evaluation_has_pairs,
            "all_evaluation_preference_pairs_ordered_correctly": evaluation_pass,
        }
    else:
        raise ValueError(f"unsupported training objective: {objective}")
    if not held_out_pass:
        status = "fail_leave_one_boundary_out"
    elif not evaluation_has_pairs:
        status = missing_evaluation_status
    elif not evaluation_pass:
        status = "fail_held_out_evaluation"
    else:
        status = "pass"
    return {
        "status": status,
        "eligible_for_shadow_search": status == "pass",
        "training_objective": objective,
        "pair_requirement": pair_requirement,
        "requirements": requirements,
    }


def leave_one_group_out(
    groups: list[Group], c: float, objective: str
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for held_out in groups:
        remaining = [group for group in groups if group.group_id != held_out.group_id]
        x, y, pair_count = pairwise_examples(remaining, objective)
        if pair_count == 0:
            results.append(
                {
                    "held_out_group": held_out.group_id,
                    "status": "insufficient_exact_pairs",
                }
            )
            continue
        model = fit_model(x, y, c)
        results.append(
            {
                "held_out_group": held_out.group_id,
                "status": "scored",
                "training_pair_count": pair_count,
                "ranking": rank_group(model, held_out),
            }
        )
    return results


def evidence_counts(groups: Iterable[Group]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for group in groups:
        for candidate in group.candidates:
            counts[candidate.evidence] = counts.get(candidate.evidence, 0) + 1
    return dict(sorted(counts.items()))


if __name__ == "__main__":
    raise SystemExit(main())
