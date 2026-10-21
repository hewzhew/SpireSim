"""Complete-run panel for the experimental public deterministic bootstrap."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
import time
from collections import Counter
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

import numpy as np

from .run_resource_trace import RunPublicContext


BASELINE_EVALUATION_SCHEMA = "sts-learning-public-baseline-evaluation-v2"


class PublicBaselineEvaluationError(RuntimeError):
    """A baseline panel could not be executed faithfully."""


class _BaselineEnvironment(Protocol):
    @property
    def slot_count(self) -> int: ...

    @property
    def terminal_count(self) -> int: ...

    @property
    def ready(self) -> bool: ...

    def decision_batch(
        self, *, legality_bootstrap: bool = False
    ) -> Mapping[str, object]: ...

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None: ...

    def step(self) -> Mapping[str, object]: ...


@dataclass(frozen=True)
class PublicBaselineEvaluationConfig:
    output: Path
    ascension_level: int
    seed_start: int
    attempts: int
    slot_count: int = 16
    max_batch_steps_per_cohort: int = 4_096
    max_selection_rounds_per_step: int = 256

    def __post_init__(self) -> None:
        output = Path(self.output).resolve()
        if output.exists() and (not output.is_dir() or any(output.iterdir())):
            raise PublicBaselineEvaluationError(
                "baseline evaluation output must be absent or empty"
            )
        object.__setattr__(self, "output", output)
        ascension = _integer(self.ascension_level, "ascension_level", minimum=0)
        if ascension > 20:
            raise PublicBaselineEvaluationError("ascension_level must be at most 20")
        object.__setattr__(self, "ascension_level", ascension)
        object.__setattr__(self, "seed_start", _seed(self.seed_start, "seed_start"))
        object.__setattr__(self, "attempts", _positive(self.attempts, "attempts"))
        object.__setattr__(
            self, "slot_count", min(_positive(self.slot_count, "slot_count"), self.attempts)
        )
        object.__setattr__(
            self,
            "max_batch_steps_per_cohort",
            _positive(self.max_batch_steps_per_cohort, "max_batch_steps_per_cohort"),
        )
        object.__setattr__(
            self,
            "max_selection_rounds_per_step",
            _positive(self.max_selection_rounds_per_step, "max_selection_rounds_per_step"),
        )


def run_public_baseline_evaluation(
    config: PublicBaselineEvaluationConfig,
    *,
    environment_factory: Callable[[list[int], int], _BaselineEnvironment] | None = None,
) -> dict[str, object]:
    """Run complete public-baseline episodes without retries or search."""

    if not isinstance(config, PublicBaselineEvaluationConfig):
        raise PublicBaselineEvaluationError("baseline evaluation config must be typed")
    if environment_factory is None:
        try:
            import sts_learning_bridge._native as native
            from sts_learning_bridge import (
                LearningBatchEnv,
                PUBLIC_LEGALITY_BOOTSTRAP_NAME,
                PUBLIC_LEGALITY_BOOTSTRAP_REVISION,
                public_legality_bootstrap_manifest,
            )
        except ImportError as error:
            raise PublicBaselineEvaluationError(
                "installed bridge does not expose the public deterministic baseline; "
                "run .\\learning\\dev.ps1 refresh-bridge"
            ) from error
        environment_factory = LearningBatchEnv
        baseline_name = str(PUBLIC_LEGALITY_BOOTSTRAP_NAME)
        baseline_revision = operator.index(PUBLIC_LEGALITY_BOOTSTRAP_REVISION)
        baseline_manifest: Mapping[str, object] | None = (
            public_legality_bootstrap_manifest()
        )
        with Path(native.__file__).open("rb") as source:
            producer = {"bridge_sha256": hashlib.file_digest(source, "sha256").hexdigest()}
    else:
        baseline_name = "public_legality_bootstrap"
        baseline_revision = 0
        baseline_manifest = None
        producer = {"environment": "injected"}

    started = time.perf_counter()
    with Path(__file__).open("rb") as source:
        producer["evaluator_sha256"] = hashlib.file_digest(source, "sha256").hexdigest()
    outcomes: list[dict[str, object]] = []
    batch_steps = 0
    decision_rounds = 0
    for offset in range(0, config.attempts, config.slot_count):
        cohort_size = min(config.slot_count, config.attempts - offset)
        seeds = list(range(config.seed_start + offset, config.seed_start + offset + cohort_size))
        env = environment_factory(seeds, config.ascension_level)
        cohort_steps = 0
        completed: dict[int, dict[str, object]] = {}
        censored_reason: str | None = None
        selection_rounds = 0
        while env.terminal_count < env.slot_count:
            if cohort_steps >= config.max_batch_steps_per_cohort:
                censored_reason = "step_limit"
                break
            if selection_rounds >= config.max_selection_rounds_per_step:
                censored_reason = "selection_limit"
                break
            batch = env.decision_batch(legality_bootstrap=True)
            slots = _integer_array(batch, "slot_indices")
            available = _boolean_array(batch, "legality_bootstrap_available")
            ordinals = _integer_array(batch, "legality_bootstrap_ordinals")
            counts = _integer_array(batch, "candidate_counts")
            tokens = list(batch.get("decision_token_ids", ()))
            if not (
                slots.size
                == available.size
                == ordinals.size
                == counts.size
                == len(tokens)
            ):
                raise PublicBaselineEvaluationError("baseline decision columns are misaligned")
            if not np.all(available):
                raise PublicBaselineEvaluationError("baseline is unavailable at a live decision")
            if np.any(ordinals < 0) or np.any(ordinals >= counts):
                raise PublicBaselineEvaluationError("baseline selected an illegal ordinal")
            env.choose(tokens, [int(ordinal) for ordinal in ordinals])
            decision_rounds += int(slots.size)
            selection_rounds += 1
            if not env.ready:
                continue
            selection_rounds = 0
            step = env.step()
            terminal_slots = _integer_array(step, "terminal_slot_indices")
            rewards = _integer_array(step, "terminal_reward")
            acts = _integer_array(step, "terminal_act")
            floors = _integer_array(step, "terminal_floor")
            hps = _integer_array(step, "terminal_hp")
            max_hps = _integer_array(step, "terminal_max_hp")
            golds = _integer_array(step, "terminal_gold")
            if not (
                terminal_slots.size
                == rewards.size
                == acts.size
                == floors.size
                == hps.size
                == max_hps.size
                == golds.size
            ):
                raise PublicBaselineEvaluationError("terminal outcome columns are misaligned")
            for index, slot in enumerate(terminal_slots):
                slot_index = int(slot)
                if not 0 <= slot_index < len(seeds):
                    raise PublicBaselineEvaluationError("terminal outcome references an unknown slot")
                if slot_index in completed:
                    raise PublicBaselineEvaluationError("terminal outcome repeats a slot")
                completed[slot_index] = {
                    "seed": seeds[slot_index],
                    "status": "completed",
                    "censored_reason": None,
                    "won": int(rewards[index]) > 0,
                    "act": int(acts[index]),
                    "floor": int(floors[index]),
                    "hp": int(hps[index]),
                    "max_hp": int(max_hps[index]),
                    "gold": int(golds[index]),
                }
            batch_steps += 1
            cohort_steps += 1

        contexts = _public_contexts(env, seeds)
        for slot, seed in enumerate(seeds):
            context = contexts[slot]
            row = completed.get(slot)
            if row is None:
                if censored_reason is None:
                    raise PublicBaselineEvaluationError("nonterminal slot has no censor reason")
                row = {
                    "seed": seed,
                    "status": "censored",
                    "censored_reason": censored_reason,
                    "won": None,
                    "act": context.act,
                    "floor": context.floor,
                    "hp": context.hp,
                    "max_hp": context.max_hp,
                    "gold": context.gold,
                }
            row["potion_ids"] = list(context.potion_ids)
            if row["status"] == "censored":
                audit_source = getattr(env, "strategic_decision_audit_json", None)
                audit_json = audit_source(slot) if callable(audit_source) else None
                row["last_decision_site"] = (
                    json.loads(audit_json)["decision_site"] if audit_json else None
                )
            outcomes.append(row)

    outcomes.sort(key=lambda row: int(row["seed"]))
    terminal_rows = [row for row in outcomes if row["status"] == "completed"]
    censored = config.attempts - len(terminal_rows)
    victories = sum(bool(row["won"]) for row in terminal_rows)
    floor_counts = Counter((int(row["act"]), int(row["floor"])) for row in terminal_rows)
    summary: dict[str, object] = {
        "schema": BASELINE_EVALUATION_SCHEMA,
        "baseline_name": baseline_name,
        "baseline_revision": baseline_revision,
        "baseline_manifest": baseline_manifest,
        "producer": producer,
        "information_access": "public_semantic_views_only",
        "deterministic": True,
        "ascension_level": config.ascension_level,
        "seed_start": config.seed_start,
        "attempts": config.attempts,
        "slot_count": config.slot_count,
        "max_batch_steps_per_cohort": config.max_batch_steps_per_cohort,
        "max_selection_rounds_per_step": config.max_selection_rounds_per_step,
        "completed": len(terminal_rows),
        "censored": censored,
        "censor_reasons": dict(
            sorted(
                Counter(
                    str(row["censored_reason"])
                    for row in outcomes
                    if row["status"] == "censored"
                ).items()
            )
        ),
        "victories": victories,
        "defeats": len(terminal_rows) - victories,
        "floor_sum": sum(int(row["floor"]) for row in terminal_rows),
        "floor_min": min((int(row["floor"]) for row in terminal_rows), default=None),
        "floor_max": max((int(row["floor"]) for row in terminal_rows), default=None),
        "floor_counts": {
            f"A{act}F{floor}": count
            for (act, floor), count in sorted(floor_counts.items())
        },
        "batch_steps": batch_steps,
        "decision_rounds": decision_rounds,
        "elapsed_seconds": time.perf_counter() - started,
        "outcomes": outcomes,
    }
    config.output.mkdir(parents=True, exist_ok=True)
    with (config.output / "evaluation.json").open(
        "x", encoding="utf-8", newline="\n"
    ) as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    print(
        f"public_baseline_evaluation_complete={str(censored == 0).lower()} "
        f"baseline={baseline_name}@{baseline_revision} "
        f"ascension={config.ascension_level} attempts={config.attempts} "
        f"victories={victories} defeats={len(terminal_rows) - victories} censored={censored} "
        f"floor_min={summary['floor_min']} floor_max={summary['floor_max']} "
        f"floor_counts={','.join(f'{key}:{value}' for key, value in summary['floor_counts'].items())} "
        f"seconds={summary['elapsed_seconds']:.3f} output={config.output}",
        flush=True,
    )
    return summary


def _public_contexts(env: object, seeds: list[int]) -> dict[int, RunPublicContext]:
    source = getattr(env, "public_run_contexts", None)
    if not callable(source):
        raise PublicBaselineEvaluationError("baseline environment has no public contexts")
    contexts = tuple(RunPublicContext.from_bridge_row(row) for row in source())
    by_slot = {context.slot_index: context for context in contexts}
    if len(by_slot) != len(contexts) or set(by_slot) != set(range(len(seeds))):
        raise PublicBaselineEvaluationError("public contexts do not cover the cohort once")
    if any(context.seed != seeds[slot] for slot, context in by_slot.items()):
        raise PublicBaselineEvaluationError("public context seed does not match its slot")
    return by_slot


def _integer_array(source: Mapping[str, object], name: str) -> np.ndarray:
    value = source.get(name)
    if not isinstance(value, np.ndarray) or value.ndim != 1 or value.dtype.kind not in "iu":
        raise PublicBaselineEvaluationError(f"{name} must be a one-dimensional integer array")
    return value.astype(np.int64, copy=False)


def _boolean_array(source: Mapping[str, object], name: str) -> np.ndarray:
    value = source.get(name)
    if not isinstance(value, np.ndarray) or value.ndim != 1 or value.dtype != np.bool_:
        raise PublicBaselineEvaluationError(f"{name} must be a one-dimensional Boolean array")
    return value


def _integer(value: object, name: str, *, minimum: int) -> int:
    if isinstance(value, bool):
        raise PublicBaselineEvaluationError(f"{name} must be an integer, not bool")
    try:
        result = operator.index(value)
    except TypeError as error:
        raise PublicBaselineEvaluationError(f"{name} must be an integer") from error
    if result < minimum:
        raise PublicBaselineEvaluationError(f"{name} must be at least {minimum}")
    return result


def _positive(value: object, name: str) -> int:
    return _integer(value, name, minimum=1)


def _seed(value: object, name: str) -> int:
    result = _integer(value, name, minimum=0)
    if result >= 1 << 63:
        raise PublicBaselineEvaluationError(f"{name} must be below 2^63")
    return result


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ascension", type=int, required=True)
    parser.add_argument("--seed-start", type=int, default=0)
    parser.add_argument("--attempts", type=int, default=16)
    parser.add_argument("--slots", type=int, default=16)
    parser.add_argument("--max-batch-steps", type=int, default=4_096)
    return parser


def main() -> None:
    arguments = _parser().parse_args()
    run_public_baseline_evaluation(
        PublicBaselineEvaluationConfig(
            output=arguments.output,
            ascension_level=arguments.ascension,
            seed_start=arguments.seed_start,
            attempts=arguments.attempts,
            slot_count=arguments.slots,
            max_batch_steps_per_cohort=arguments.max_batch_steps,
        )
    )


if __name__ == "__main__":
    main()
