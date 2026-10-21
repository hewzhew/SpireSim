"""One-state exact-search supervision for the maintained combat scorer."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import operator
from collections.abc import Mapping, Sequence
from dataclasses import asdict, dataclass
from pathlib import Path

import torch
import torch.nn.functional as functional

from .combat_decision_audit import (
    CombatDecisionAuditError,
    read_combat_decision_audit,
)
from .combat_outcomes import CombatOutcomeError, CombatTerminalStepBatch
from .combat_potion_lane import CombatPotionLane, CombatPotionLaneRootSource
from .combat_root_artifacts import (
    load_combat_root_source,
    read_combat_root_artifact,
)
from .combat_root_audit import CombatRootAuditError, read_combat_root_audit
from .published_combat_behavior import recover_compatible_combat_scorer
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_policy import RaggedCandidateLogits, RaggedCandidateScorer


COUNTERFACTUAL_CORPUS_SCHEMA = "ActionSuccessorReanalysisCorpusV2"
CALIBRATION_SCHEMA = "sts-learning-combat-counterfactual-calibration-v1"


class CounterfactualCombatError(RuntimeError):
    """Exact action evidence and the bridge-owned model surface disagree."""


@dataclass(frozen=True)
class CounterfactualCombatConfig:
    artifact: Path
    corpus: Path
    behavior: Path
    output: Path
    expected_roots: int
    root_slot: int
    max_optimizer_steps: int = 512
    learning_rate: float = 0.005
    target_margin: float = 4.0
    max_grad_norm: float = 1.0

    def __post_init__(self) -> None:
        artifact = Path(self.artifact).resolve()
        corpus = Path(self.corpus).resolve()
        behavior = Path(self.behavior).resolve()
        output = Path(self.output).resolve()
        if not artifact.is_file() or not corpus.is_file():
            raise CounterfactualCombatError(
                "counterfactual artifact and corpus must be files"
            )
        if not behavior.is_dir():
            raise CounterfactualCombatError(
                "counterfactual warm-start behavior must be a directory"
            )
        if output.exists() and (not output.is_dir() or any(output.iterdir())):
            raise CounterfactualCombatError(
                "counterfactual output must be absent or empty"
            )
        expected_roots = _positive(self.expected_roots, "expected_roots")
        root_slot = _nonnegative(self.root_slot, "root_slot")
        if root_slot >= expected_roots:
            raise CounterfactualCombatError(
                "counterfactual root_slot must be below expected_roots"
            )
        max_optimizer_steps = _positive(
            self.max_optimizer_steps,
            "max_optimizer_steps",
        )
        for name in ("learning_rate", "target_margin", "max_grad_norm"):
            value = getattr(self, name)
            if isinstance(value, bool) or not isinstance(value, (int, float)):
                raise CounterfactualCombatError(f"{name} must be a real number")
            if not math.isfinite(float(value)) or float(value) <= 0.0:
                raise CounterfactualCombatError(
                    f"{name} must be finite and positive"
                )
        object.__setattr__(self, "artifact", artifact)
        object.__setattr__(self, "corpus", corpus)
        object.__setattr__(self, "behavior", behavior)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "expected_roots", expected_roots)
        object.__setattr__(self, "root_slot", root_slot)
        object.__setattr__(self, "max_optimizer_steps", max_optimizer_steps)
        object.__setattr__(self, "learning_rate", float(self.learning_rate))
        object.__setattr__(self, "target_margin", float(self.target_margin))
        object.__setattr__(self, "max_grad_norm", float(self.max_grad_norm))


@dataclass(frozen=True)
class CounterfactualActionGroup:
    group_index: int
    ordinals: tuple[int, ...]
    semantics: dict[str, object]
    labels: tuple[str, ...]
    evidence_kind: str
    final_hp: int | None


@dataclass(frozen=True)
class CounterfactualPreferenceProblem:
    groups: tuple[CounterfactualActionGroup, ...]
    pairs: tuple[tuple[int, int], ...]
    candidate_count: int


def build_counterfactual_preference_problem(
    corpus: Mapping[str, object],
    decision_candidates: Sequence[Mapping[str, object]],
    *,
    exact_combat_state_hash: str,
) -> CounterfactualPreferenceProblem:
    """Bind equal-work successor evidence to semantic action groups."""

    if corpus.get("schema_name") != COUNTERFACTUAL_CORPUS_SCHEMA:
        raise CounterfactualCombatError("unsupported counterfactual corpus schema")
    if _integer(corpus.get("schema_version"), "schema_version") != 2:
        raise CounterfactualCombatError("unsupported counterfactual corpus version")
    if corpus.get("root_exact_state_hash") != exact_combat_state_hash:
        raise CounterfactualCombatError(
            "counterfactual corpus and bridge root exact hashes differ"
        )
    surface = _mapping(corpus.get("surface"), "surface")
    learning_surface = _mapping(corpus.get("learning_surface"), "learning_surface")
    candidate_count = len(decision_candidates)
    if not bool(surface.get("complete")) or not bool(learning_surface.get("complete")):
        raise CounterfactualCombatError(
            "counterfactual calibration requires complete exact and learning surfaces"
        )
    if _integer(
        learning_surface.get("candidate_count"),
        "learning_surface.candidate_count",
    ) != candidate_count:
        raise CounterfactualCombatError(
            "counterfactual learning surface disagrees with bridge candidate count"
        )
    raw_candidates = _sequence(corpus.get("candidates"), "candidates")
    if len(raw_candidates) != candidate_count:
        raise CounterfactualCombatError(
            "counterfactual corpus does not cover every bridge candidate"
        )

    by_ordinal: dict[int, tuple[str, int | None, str]] = {}
    for index, raw_candidate in enumerate(raw_candidates):
        candidate = _mapping(raw_candidate, f"candidates[{index}]")
        ordinal = _nonnegative(
            candidate.get("learning_candidate_ordinal"),
            f"candidates[{index}].learning_candidate_ordinal",
        )
        if ordinal >= candidate_count or ordinal in by_ordinal:
            raise CounterfactualCombatError(
                "counterfactual learning ordinals must form one unique surface"
            )
        evidence = _mapping(candidate.get("evidence"), f"candidates[{index}].evidence")
        kind = evidence.get("kind")
        if kind not in {
            "exact_win",
            "exact_refutation",
            "exact_terminal_non_win",
            "budget_unknown",
        }:
            raise CounterfactualCombatError(
                f"counterfactual candidate {index} has unsupported evidence"
            )
        final_hp = (
            _nonnegative(evidence.get("final_hp"), f"candidates[{index}].final_hp")
            if kind == "exact_win"
            else None
        )
        label = candidate.get("label")
        if not isinstance(label, str) or not label:
            raise CounterfactualCombatError(
                f"counterfactual candidate {index} has no readable label"
            )
        by_ordinal[ordinal] = (kind, final_hp, label)
    if set(by_ordinal) != set(range(candidate_count)):
        raise CounterfactualCombatError(
            "counterfactual learning ordinals do not cover the bridge surface"
        )

    grouped: dict[str, list[int]] = {}
    normalized_semantics: dict[str, dict[str, object]] = {}
    for ordinal, raw_semantics in enumerate(decision_candidates):
        semantics = _semantic_equivalence_key(raw_semantics)
        key = json.dumps(semantics, separators=(",", ":"), sort_keys=True)
        grouped.setdefault(key, []).append(ordinal)
        normalized_semantics[key] = semantics

    groups: list[CounterfactualActionGroup] = []
    for key, ordinals in grouped.items():
        evidence_rows = {by_ordinal[ordinal][:2] for ordinal in ordinals}
        if len(evidence_rows) != 1:
            raise CounterfactualCombatError(
                "semantic-equivalent actions received conflicting search evidence"
            )
        evidence_kind, final_hp = next(iter(evidence_rows))
        groups.append(
            CounterfactualActionGroup(
                group_index=len(groups),
                ordinals=tuple(ordinals),
                semantics=normalized_semantics[key],
                labels=tuple(by_ordinal[ordinal][2] for ordinal in ordinals),
                evidence_kind=evidence_kind,
                final_hp=final_hp,
            )
        )

    pairs: list[tuple[int, int]] = []
    for left in groups:
        for right in groups[left.group_index + 1 :]:
            preference = _compare_group_evidence(left, right)
            if preference > 0:
                pairs.append((left.group_index, right.group_index))
            elif preference < 0:
                pairs.append((right.group_index, left.group_index))
    if not pairs:
        raise CounterfactualCombatError(
            "counterfactual corpus contains no observable action preference"
        )
    return CounterfactualPreferenceProblem(
        groups=tuple(groups),
        pairs=tuple(pairs),
        candidate_count=candidate_count,
    )


def counterfactual_pairwise_loss(
    logits: torch.Tensor,
    problem: CounterfactualPreferenceProblem,
) -> tuple[torch.Tensor, torch.Tensor]:
    """Rank semantic action mass only across search-observed preferences."""

    if logits.ndim != 1 or logits.numel() != problem.candidate_count:
        raise CounterfactualCombatError(
            "counterfactual logits disagree with the candidate surface"
        )
    group_scores = torch.stack(
        [torch.logsumexp(logits[list(group.ordinals)], dim=0) for group in problem.groups]
    )
    differences = torch.stack(
        [group_scores[preferred] - group_scores[inferior] for preferred, inferior in problem.pairs]
    )
    return functional.softplus(-differences).mean(), differences


def run_counterfactual_combat_calibration(
    config: CounterfactualCombatConfig,
    *,
    bridge: CombatSessionBridge | None = None,
    print_completion: bool = True,
) -> dict[str, object]:
    """Overfit one exact semantic action table and compare greedy rollouts."""

    if not isinstance(config, CounterfactualCombatConfig):
        raise CounterfactualCombatError("counterfactual config must be typed")
    active_bridge = bridge if bridge is not None else CombatSessionBridge.installed()
    limits = CombatWinSessionLimits()
    artifact = read_combat_root_artifact(
        config.artifact,
        max_bytes=limits.max_artifact_bytes,
    )
    source = load_combat_root_source(
        active_bridge,
        artifact,
        expected_roots=config.expected_roots,
        max_bytes=limits.max_artifact_bytes,
    )
    try:
        root_audit = read_combat_root_audit(source, config.root_slot)
    except CombatRootAuditError as error:
        raise CounterfactualCombatError(str(error)) from error
    lane_source = CombatPotionLaneRootSource(source, CombatPotionLane.NEVER, ())
    group = lane_source.combat_group(config.root_slot, 1)
    try:
        decision = read_combat_decision_audit(group, 0)
    except CombatDecisionAuditError as error:
        raise CounterfactualCombatError(str(error)) from error
    if decision is None or decision.phase != "combat_root":
        raise CounterfactualCombatError(
            "counterfactual root is not an undecoded combat decision"
        )
    batch = group.decision_batch(semantic=True)
    exact_hash = _digest(
        getattr(group, "exact_combat_state_hash", None),
        "exact_combat_state_hash",
    )
    corpus = _load_json_mapping(config.corpus)
    problem = build_counterfactual_preference_problem(
        corpus,
        decision.candidates,
        exact_combat_state_hash=exact_hash,
    )

    warm = recover_compatible_combat_scorer(config.behavior, active_bridge, limits)
    scorer = copy.deepcopy(warm.scorer)
    scorer.train()
    scorer.requires_grad_(True)
    baseline = copy.deepcopy(scorer)
    baseline.eval()
    baseline.requires_grad_(False)

    before_logits = _score_logits(scorer, batch, problem.candidate_count)
    _, before_differences = counterfactual_pairwise_loss(before_logits, problem)
    optimizer = torch.optim.Adam(scorer.parameters(), lr=config.learning_rate)
    optimizer_steps = 0
    final_loss = 0.0
    for _ in range(config.max_optimizer_steps):
        logits = _score_logits(scorer, batch, problem.candidate_count)
        loss, differences = counterfactual_pairwise_loss(logits, problem)
        final_loss = float(loss.detach().item())
        if bool(torch.all(differences.detach() >= config.target_margin)):
            break
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        gradient_norm = torch.nn.utils.clip_grad_norm_(
            tuple(scorer.parameters()),
            config.max_grad_norm,
        )
        if not bool(torch.isfinite(gradient_norm)):
            raise CounterfactualCombatError(
                "counterfactual optimizer produced a non-finite gradient"
            )
        optimizer.step()
        optimizer_steps += 1

    scorer.eval()
    scorer.requires_grad_(False)
    after_logits = _score_logits(scorer, batch, problem.candidate_count)
    _, after_differences = counterfactual_pairwise_loss(after_logits, problem)
    baseline_rollout = _greedy_rollout(
        lane_source,
        config.root_slot,
        baseline,
        limits,
    )
    trained_rollout = _greedy_rollout(
        lane_source,
        config.root_slot,
        scorer,
        limits,
    )

    report = {
        "schema": CALIBRATION_SCHEMA,
        "artifact": str(config.artifact),
        "artifact_sha256": hashlib.sha256(artifact).hexdigest(),
        "corpus": str(config.corpus),
        "corpus_sha256": _file_sha256(config.corpus),
        "source_behavior": str(config.behavior),
        "source_manifest_sha256": warm.source_manifest_id.digest.hex(),
        "source_checkpoint_sha256": warm.checkpoint_id.digest.hex(),
        "source_training_step": warm.training_step,
        "root_slot": config.root_slot,
        "root_exact_combat_state_hash": exact_hash,
        "root": root_audit.as_mapping(),
        "candidate_count": problem.candidate_count,
        "semantic_group_count": len(problem.groups),
        "preference_pair_count": len(problem.pairs),
        "optimizer": {
            "rule": "semantic_group_pairwise_logistic",
            "unknown_actions": "excluded_from_pairs",
            "equal_evidence_actions": "not_ranked",
            "steps": optimizer_steps,
            "max_steps": config.max_optimizer_steps,
            "learning_rate": config.learning_rate,
            "target_margin": config.target_margin,
            "max_grad_norm": config.max_grad_norm,
            "final_loss": final_loss,
        },
        "before": _score_report(before_logits, problem),
        "after": _score_report(after_logits, problem),
        "before_min_pair_margin": float(before_differences.detach().min().item()),
        "after_min_pair_margin": float(after_differences.detach().min().item()),
        "before_pair_accuracy": _pair_accuracy(before_differences),
        "after_pair_accuracy": _pair_accuracy(after_differences),
        "groups": _group_report(before_logits, after_logits, problem),
        "greedy_rollout_before": baseline_rollout,
        "greedy_rollout_after": trained_rollout,
    }
    config.output.mkdir(parents=True, exist_ok=True)
    report_path = config.output / "calibration.json"
    with report_path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(report, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    if print_completion:
        print(
            json.dumps(
                {
                    "report": str(report_path),
                    "semantic_groups": len(problem.groups),
                    "preference_pairs": len(problem.pairs),
                    "optimizer_steps": optimizer_steps,
                    "before_min_pair_margin": report["before_min_pair_margin"],
                    "after_min_pair_margin": report["after_min_pair_margin"],
                    "before_greedy_won": baseline_rollout["won"],
                    "after_greedy_won": trained_rollout["won"],
                },
                separators=(",", ":"),
                sort_keys=True,
            ),
            flush=True,
        )
    return report


def _greedy_rollout(
    source: CombatPotionLaneRootSource,
    root_slot: int,
    scorer: RaggedCandidateScorer,
    limits: CombatWinSessionLimits,
) -> dict[str, object]:
    env = source.combat_group(root_slot, 1)
    selected: list[dict[str, object]] = []
    model_rounds = 0
    transitions = 0
    outcome = None
    while int(getattr(env, "terminal_count", 0)) < 1:
        while not bool(getattr(env, "ready", False)):
            if model_rounds >= limits.experience.max_model_rounds:
                raise CounterfactualCombatError(
                    "counterfactual greedy rollout exceeded its model-round limit"
                )
            try:
                decision = read_combat_decision_audit(env, 0)
            except CombatDecisionAuditError as error:
                raise CounterfactualCombatError(str(error)) from error
            if decision is None:
                raise CounterfactualCombatError(
                    "counterfactual greedy rollout lost its decision surface"
                )
            batch = env.decision_batch(semantic=True)
            logits = _score_logits(scorer, batch, len(decision.candidates))
            ordinal = int(torch.argmax(logits).item())
            selected.append(
                {
                    "model_round": model_rounds,
                    "phase": decision.phase,
                    "ordinal": ordinal,
                    "action": _semantic_equivalence_key(decision.candidates[ordinal]),
                }
            )
            env.choose([ordinal])
            model_rounds += 1
        if transitions >= limits.experience.max_transitions:
            raise CounterfactualCombatError(
                "counterfactual greedy rollout exceeded its transition limit"
            )
        step = env.step()
        try:
            terminal = CombatTerminalStepBatch.from_bridge_step(
                step,
                replicate_count=1,
            )
        except CombatOutcomeError as error:
            raise CounterfactualCombatError(str(error)) from error
        if terminal.outcomes:
            outcome = terminal.outcomes[0]
        transitions += 1
    if outcome is None:
        raise CounterfactualCombatError(
            "counterfactual greedy rollout ended without a terminal outcome"
        )
    return {
        "won": outcome.won,
        "final_hp": outcome.final_hp,
        "hp_loss": outcome.hp_loss,
        "enemy_final_hp": outcome.enemy_final_hp,
        "turns": outcome.turns,
        "cards_played": outcome.cards_played,
        "model_rounds": model_rounds,
        "transitions": transitions,
        "actions": selected,
        "terminal": asdict(outcome),
    }


def _score_logits(
    scorer: RaggedCandidateScorer,
    batch: Mapping[str, object],
    candidate_count: int,
) -> torch.Tensor:
    logits = scorer(batch)
    if not isinstance(logits, RaggedCandidateLogits):
        raise CounterfactualCombatError(
            "counterfactual scorer returned the wrong output type"
        )
    splits = tuple(int(value) for value in logits.row_splits.detach().cpu().tolist())
    if splits != (0, candidate_count):
        raise CounterfactualCombatError(
            "counterfactual scorer did not return exactly one aligned row"
        )
    if not bool(torch.all(torch.isfinite(logits.values))):
        raise CounterfactualCombatError("counterfactual logits must be finite")
    return logits.values


def _score_report(
    logits: torch.Tensor,
    problem: CounterfactualPreferenceProblem,
) -> dict[str, object]:
    values = logits.detach().to(dtype=torch.float64, device="cpu")
    probabilities = torch.softmax(values, dim=0)
    ordering = sorted(
        range(problem.candidate_count),
        key=lambda ordinal: (-float(values[ordinal]), ordinal),
    )
    return {
        "top_ordinal": ordering[0],
        "logits": tuple(float(value) for value in values.tolist()),
        "probabilities": tuple(float(value) for value in probabilities.tolist()),
    }


def _group_report(
    before_logits: torch.Tensor,
    after_logits: torch.Tensor,
    problem: CounterfactualPreferenceProblem,
) -> tuple[dict[str, object], ...]:
    before_probabilities = torch.softmax(before_logits.detach(), dim=0)
    after_probabilities = torch.softmax(after_logits.detach(), dim=0)
    preferred = {left for left, _ in problem.pairs}
    inferior = {right for _, right in problem.pairs}
    return tuple(
        {
            "group_index": group.group_index,
            "ordinals": group.ordinals,
            "semantics": group.semantics,
            "labels": group.labels,
            "evidence_kind": group.evidence_kind,
            "best_observed_final_hp": group.final_hp,
            "preference_role": (
                "preferred"
                if group.group_index in preferred and group.group_index not in inferior
                else "inferior"
                if group.group_index in inferior and group.group_index not in preferred
                else "intermediate"
                if group.group_index in preferred | inferior
                else "unknown_or_tied"
            ),
            "before_probability_mass": float(
                before_probabilities[list(group.ordinals)].sum().item()
            ),
            "after_probability_mass": float(
                after_probabilities[list(group.ordinals)].sum().item()
            ),
        }
        for group in problem.groups
    )


def _semantic_equivalence_key(raw: Mapping[str, object]) -> dict[str, object]:
    semantics = copy.deepcopy(dict(raw))
    kind = semantics.get("kind")
    if kind == "play_card":
        semantics.pop("hand_index", None)
    target = semantics.get("target")
    if isinstance(target, Mapping):
        normalized_target = dict(target)
        normalized_target.pop("monster_index", None)
        semantics["target"] = normalized_target
    return semantics


def _compare_group_evidence(
    left: CounterfactualActionGroup,
    right: CounterfactualActionGroup,
) -> int:
    if "budget_unknown" in {left.evidence_kind, right.evidence_kind}:
        return 0
    left_win = left.evidence_kind == "exact_win"
    right_win = right.evidence_kind == "exact_win"
    if left_win != right_win:
        return 1 if left_win else -1
    if left_win:
        assert left.final_hp is not None and right.final_hp is not None
        return (left.final_hp > right.final_hp) - (left.final_hp < right.final_hp)
    return 0


def _pair_accuracy(differences: torch.Tensor) -> float:
    return float((differences.detach() > 0).to(torch.float64).mean().item())


def _load_json_mapping(path: Path) -> dict[str, object]:
    try:
        with path.open("r", encoding="utf-8") as source:
            payload = json.load(source)
    except (OSError, json.JSONDecodeError) as error:
        raise CounterfactualCombatError(
            f"cannot read counterfactual corpus {path}: {error}"
        ) from error
    if not isinstance(payload, Mapping):
        raise CounterfactualCombatError("counterfactual corpus must be a mapping")
    return dict(payload)


def _file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise CounterfactualCombatError(f"{name} must be a mapping")
    return value


def _sequence(value: object, name: str) -> Sequence[object]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise CounterfactualCombatError(f"{name} must be a sequence")
    return value


def _integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CounterfactualCombatError(f"{name} must be an integer")
    try:
        return operator.index(value)
    except TypeError as error:
        raise CounterfactualCombatError(f"{name} must be an integer") from error


def _positive(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized <= 0:
        raise CounterfactualCombatError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized < 0:
        raise CounterfactualCombatError(f"{name} must be non-negative")
    return normalized


def _digest(value: object, name: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        raise CounterfactualCombatError(f"{name} must be a SHA-256 digest")
    try:
        bytes.fromhex(value)
    except ValueError as error:
        raise CounterfactualCombatError(f"{name} must be hexadecimal") from error
    return value.lower()


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Overfit one exact equal-work combat action table.",
    )
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--roots", type=int, required=True)
    parser.add_argument("--root-slot", type=int, required=True)
    parser.add_argument("--max-optimizer-steps", type=int, default=512)
    parser.add_argument("--learning-rate", type=float, default=0.005)
    parser.add_argument("--target-margin", type=float, default=4.0)
    parser.add_argument("--max-grad-norm", type=float, default=1.0)
    return parser


def main() -> int:
    args = _parser().parse_args()
    run_counterfactual_combat_calibration(
        CounterfactualCombatConfig(
            artifact=args.artifact,
            corpus=args.corpus,
            behavior=args.behavior,
            output=args.output,
            expected_roots=args.roots,
            root_slot=args.root_slot,
            max_optimizer_steps=args.max_optimizer_steps,
            learning_rate=args.learning_rate,
            target_margin=args.target_margin,
            max_grad_norm=args.max_grad_norm,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
