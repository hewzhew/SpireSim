"""Frozen local model versus independent fixed-root search; no teacher update.

The CLI measures discovery of one complete replayable win without updates.
Its shared collector can explicitly export successful paths for a separate
learning experiment; it does not own supervision or resource ranking.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import gzip
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch
import sts_learning_bridge
from sts_learning_bridge import GuidedCombatBatch

from tools.guided_search.local_train import (
    ROOT, Config, network, cases, check_deadline, hash_file, prepare,
    rollout, write_json,
)


@dataclass(frozen=True)
class EvaluationPanel:
    seed_start: int
    seed_count: int
    partition: str
    deck: tuple[tuple[str, int], ...] | None = None

    def seeds(self):
        return range(self.seed_start, self.seed_start + self.seed_count)

    def validate(self, config):
        if (type(self.seed_start) is not int or type(self.seed_count) is not int
                or not 1 <= self.seed_count <= (32 if config.task == "draw_select" else 64) or self.seed_start < 0
                or self.seed_start + self.seed_count > 2**64):
            raise ValueError("invalid evaluation seed panel")
        if self.partition not in ("development", "final"):
            raise ValueError("invalid evaluation partition")
        if self.deck is not None:
            if config.task != "draw_select":
                raise ValueError("custom evaluation deck requires draw_select")
            if (not isinstance(self.deck, (list, tuple)) or not 1 <= len(self.deck) <= 16
                    or any(not isinstance(card, (list, tuple)) or len(card) != 2 or not isinstance(card[0], str)
                           or type(card[1]) is not int or not 0 <= card[1] <= 1 for card in self.deck)):
                raise ValueError("invalid evaluation deck shape, count or upgrades")
        if set(self.seeds()) & (set(config.train_seeds()) | set(config.eval_seeds())):
            raise ValueError("new evaluation panel overlaps training or development seed groups")


def search(model, panel, config, directory, deadline, services, mode, rollout_steps=0, *, deck=None,
           demonstrations=False):
    if mode not in ("neutral", "policy", "policy_value"):
        raise ValueError("invalid search mode")
    start_wall = time.perf_counter()
    directory.mkdir()
    env = GuidedCombatBatch(panel, config.max_decisions, starting_hp=config.starting_hp,
                             task=config.task, search_services=services, policy_rollout_steps=rollout_steps, deck=deck)
    rows = []
    timings = {"observation": 0., "inference": 0., "search_including_replay": 0.}
    complete = False
    try:
        while True:
            check_deadline(deadline)
            start = time.perf_counter()
            frame = env.decision_batch()
            timings["observation"] += time.perf_counter() - start
            if not len(frame["slots"]):
                complete = True
                break
            start = time.perf_counter()
            data = prepare(frame, config.device)
            with torch.no_grad():
                if mode == "neutral":
                    probabilities = data["mask"].float()
                    probabilities /= probabilities.sum(-1, keepdim=True)
                    estimates = [None] * len(frame["slots"])
                else:
                    logits, value = model(data)
                    probabilities = logits.softmax(-1)
                    estimates = value.sigmoid().cpu().tolist() if mode == "policy_value" else [None] * len(frame["slots"])
            probabilities = probabilities.cpu().numpy()
            counts = np.diff(frame["action_splits"])
            weights = [row[:count].tolist() for row, count in zip(probabilities, counts)]
            rows.append({**{k: v.cpu().numpy() for k, v in data.items()},
                         "slots": np.array(frame["slots"], copy=True),
                         "revision": np.full(len(weights), frame["revision"], dtype=np.uint64),
                         "probabilities": probabilities,
                         "values": np.array([np.nan if v is None else v for v in estimates])})
            timings["inference"] += time.perf_counter() - start
            start = time.perf_counter()
            env.guide(frame["revision"], weights, estimates)
            timings["search_including_replay"] += time.perf_counter() - start
    finally:
        with gzip.open(directory / "evidence.json.gz", "xt", encoding="utf-8") as f:
            f.write(env.evidence_json())
        outcomes = json.loads(env.outcomes_json())
        write_json(directory / "outcomes.json", {"complete": complete, "mode": mode,
                                                 "services": services, "policy_rollout_steps": rollout_steps,
                                                 "outcomes": outcomes})
        if rows:
            np.savez_compressed(directory / "guidance.npz",
                                **{k: np.concatenate([r[k] for r in rows]) for k in rows[0]})
    if not complete or len(outcomes) != len(panel):
        raise RuntimeError("incomplete search panel")
    if any(o["kind"] == "win" and not o["win_replayed"] for o in outcomes):
        raise RuntimeError("search victory lacks exact replay")
    if demonstrations:
        from tools.guided_search.local_demonstrations import export_demonstrations
        start = time.perf_counter()
        export_demonstrations(env, directory)
        timings["demonstration_export"] = time.perf_counter() - start
    timings["cohort_wall"] = time.perf_counter() - start_wall
    write_json(directory / "timing.json", timings)
    return outcomes, timings


def repeated_policy(model, panel, config, directory, deadline, repeats, seed, *, deck=None):
    """Fresh sampled trajectories; first replayed victory ends that root's work."""
    directory.mkdir()
    start = time.perf_counter()
    remaining = list(range(len(panel)))
    spent = [{"engine_steps": 0, "replay_engine_steps": 0, "services": 0,
              "guidance_rows": 0} for _ in panel]
    results = [None] * len(panel)
    generator = torch.Generator(device=config.device).manual_seed(seed)
    for attempt in range(repeats):
        if not remaining:
            break
        _, outcomes, _ = rollout(model, [panel[i] for i in remaining], config, generator,
                                  directory / f"attempt-{attempt}", deadline, deck=deck)
        next_remaining = []
        for o in outcomes:
            slot = remaining[o["slot"]]
            for key in spent[slot]:
                spent[slot][key] += o[key]
            results[slot] = {**o, **spent[slot], "slot": slot, "attempts": attempt + 1}
            if o["kind"] != "win":
                results[slot]["kind"] = "unknown_attempt_limit"
                next_remaining.append(slot)
        remaining = next_remaining
    wall = time.perf_counter() - start
    write_json(directory / "result.json", {"outcomes": results, "wall_seconds": wall})
    return results, {"cohort_wall": wall}


def summarize(model_seed, mode, bound, outcomes, timing):
    return {"model": model_seed, "mode": mode, "bound": bound,
            "wins": sum(o["kind"] == "win" for o in outcomes),
            "losses": sum(o["kind"] == "loss" for o in outcomes),
            "unknown": sum(o["kind"].startswith("unknown") for o in outcomes),
            "episodes": len(outcomes), "outcomes": outcomes, "timing_seconds": timing,
            **{key: sum(o[key] for o in outcomes) for key in
               ("engine_steps", "replay_engine_steps", "services", "guidance_rows")}}


def verify_direct_regression(current, reference):
    """Read-only comparison of fresh execution against an explicit baseline.

    This imports no roots, trajectories or action permissions into the core.
    """
    with gzip.open(current / "evidence.json.gz", "rt", encoding="utf-8") as f:
        actual = json.load(f)["episodes"]
    with gzip.open(reference / "evidence.json.gz", "rt", encoding="utf-8") as f:
        expected = json.load(f)["episodes"]
    keys = ("source", "seed", "enemy", "starting_hp", "root", "inputs", "successor")
    if len(actual) != len(expected) or any(
        any(a[k] != b[k] for k in keys) for a, b in zip(actual, expected)
    ):
        raise ValueError("fresh direct trajectory differs from the declared baseline")
    with np.load(current / "experience.npz") as a, np.load(reference / "experience.npz") as b:
        if set(a.files) != set(b.files):
            raise ValueError("fresh direct numerical fields differ from the declared baseline")
        for key in set(a.files) - {"logp", "old_value"}:
            if not np.array_equal(a[key], b[key]):
                raise ValueError(f"fresh direct numerical {key} differs from the declared baseline")
    return {"episodes": len(actual), "inputs": sum(len(e["inputs"]) for e in actual),
            "roots_inputs_successors_and_numeric_actions": "exact",
            "reference_evidence_sha256": hash_file(reference / "evidence.json.gz"),
            "actual_evidence_sha256": hash_file(current / "evidence.json.gz")}


def load_training(training, regression_baseline=None):
    """Validate declared local weights and the optional original-binary reference.

    A caller changing binaries must still run fresh direct regression before work.
    """
    training = training.resolve()
    contract = json.loads((training / "contract.json").read_text(encoding="utf-8"))
    config = Config(**contract["config"])
    config.validate()
    if config.evaluation_partition != "development":
        raise ValueError("this diagnostic comparison requires a declared development partition")
    if json.loads((training / "result.json").read_text())["status"] != "complete":
        raise ValueError("training must have completed")
    binary_hashes = {p.name: hash_file(p) for p in Path(sts_learning_bridge.__file__).parent.glob("*.pyd")}
    training_binary = json.loads((training / "manifest.json").read_text())["binary_sha256"]
    weights = [training / f"model-{seed}/model-{config.generations}.npz" for seed in config.model_seeds]
    weight_hashes = {str(p): hash_file(p) for p in weights}
    if not binary_hashes or not training_binary:
        raise ValueError("training and comparison require the same native binary or explicit regression evidence")
    if regression_baseline is None:
        if binary_hashes != training_binary:
            raise ValueError("training and comparison require the same native binary or an explicit regression baseline")
    else:
        regression_baseline = regression_baseline.resolve()
        base = json.loads((regression_baseline / "contract.json").read_text())
        producer = json.loads((regression_baseline / "manifest.json").read_text())
        if (json.loads((regression_baseline / "result.json").read_text())["status"] != "complete"
                or base["weights_sha256"] != weight_hashes or base["config"] != contract["config"]
                or producer["training_contract_sha256"] != hash_file(training / "contract.json")
                or producer["binary_sha256"] != training_binary):
            raise ValueError("regression baseline does not bind the declared training study and native binary")
    return config, weight_hashes, binary_hashes, training_binary


def run(training, output, budgets=(256, 1024), repeats=8, max_wall_seconds=900,
        rollout_steps=(), regression_baseline=None, evaluation_panel=None):
    wall_start = time.perf_counter()
    deadline = time.monotonic() + max_wall_seconds
    if not budgets or any(not 1 <= b <= 16384 for b in budgets) or not 1 <= repeats <= 32:
        raise ValueError("invalid comparison bounds")
    if not np.isfinite(max_wall_seconds) or max_wall_seconds <= 0:
        raise ValueError("invalid wall budget")
    if len(set(rollout_steps)) != len(rollout_steps) or any(not 1 <= s <= 512 for s in rollout_steps):
        raise ValueError("invalid policy rollout limits")
    training = training.resolve()
    regression_baseline = regression_baseline.resolve() if regression_baseline else None
    config, weight_hashes, binary_hashes, training_binary = load_training(training, regression_baseline)
    weights = [Path(p) for p in weight_hashes]
    if evaluation_panel is not None:
        evaluation_panel.validate(config)
    output.mkdir(parents=True, exist_ok=False)
    torch.set_num_threads(config.threads)
    control_panel = cases(config.eval_seeds(), config.task)
    panel = cases(evaluation_panel.seeds(), config.task) if evaluation_panel else control_panel
    deck = evaluation_panel.deck if evaluation_panel else None
    write_json(output / "contract.json", {
        "config": asdict(config), "cases": panel,
        "partition": evaluation_panel.partition if evaluation_panel else "development",
        "evaluation_panel": asdict(evaluation_panel) if evaluation_panel else None,
        "regression_cases": control_panel if regression_baseline else [],
        "training": str(training), "budgets": list(budgets), "sampled_attempts": repeats,
        "modes": ["greedy", "sampled_restarts", "neutral", "policy", "policy_value"],
        "additional_policy_rollout_limits": list(rollout_steps),
        "regression_baseline": str(regression_baseline) if regression_baseline else None,
        "regression_scope": "fresh direct roots, inputs, successors and numerical actions for every declared model; not general simulator equivalence",
        "tree_config": {"exploration": 1.5, "uniform_fraction": .1, "neutral_value": .5,
                        "max_pending_guidance": 1, "max_engine_steps_per_transition": 256},
        "objective": "first complete replayed victory; work exhausted is Unknown",
        "selection": "last declared generation for all initializations; no weight updates",
        "replay_reserve_engine_steps_per_root": config.max_decisions * 256,
        "max_wall_seconds": max_wall_seconds,
        "weights_sha256": weight_hashes,
    })
    write_json(output / "manifest.json", {
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "status": subprocess.check_output(["git", "status", "--short"], cwd=ROOT, text=True),
        "torch": torch.__version__, "numpy": np.__version__,
        "binary_sha256": binary_hashes,
        "training_binary_sha256": training_binary,
        "training_contract_sha256": hash_file(training / "contract.json"),
    })
    (output / "producer.patch").write_bytes(subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT))
    source_dir = output / "sources"
    source_dir.mkdir()
    for rel in ["tools/guided_search/local_compare.py", "tools/guided_search/local_train.py",
                "tools/guided_search/local_tactics.py", "crates/sts_guided_search/src/local_tactics.rs",
                "crates/sts_guided_search/src/local.rs", "crates/sts_guided_search/src/tree.rs",
                "crates/sts_guided_search/src/combat.rs", "bindings/python_learning/src/guided.rs"]:
        (source_dir / rel.replace("/", "__")).write_bytes((ROOT / rel).read_bytes())
    summary = []

    def record(seed, mode, bound, result):
        row = summarize(seed, mode, bound, *result)
        summary.append(row)
        print(json.dumps({k: row[k] for k in ("model", "mode", "bound", "wins", "losses", "unknown", "services")}), flush=True)

    try:
        models = {}
        regression = []
        # Verify on the original development controls, before opening any new
        # evaluation roots. Regression rows never enter a new panel's metrics.
        for seed, path in zip(config.model_seeds, weights):
            check_deadline(deadline)
            model = network(config).to(config.device).eval()
            with np.load(path, allow_pickle=False) as data:
                model.load_state_dict({k: torch.as_tensor(data[k], device=config.device) for k in data.files})
            models[seed] = model
            directory = output / f"model-{seed}"
            directory.mkdir()
            if regression_baseline:
                check_dir = directory / ("regression" if evaluation_panel else "greedy")
                _, outcomes, timing = rollout(model, control_panel, config,
                    torch.Generator(device=config.device).manual_seed(seed), check_dir, deadline, "greedy")
                regression.append({"model": seed, **verify_direct_regression(
                    check_dir, regression_baseline / f"model-{seed}/greedy")})
                if evaluation_panel is None:
                    record(seed, "greedy", config.max_decisions, (outcomes, timing))
        if regression_baseline:
            write_json(output / "regression.json", regression)
        if evaluation_panel is not None or regression_baseline is None:
            for seed, model in models.items():
                _, outcomes, timing = rollout(model, panel, config,
                    torch.Generator(device=config.device).manual_seed(seed),
                    output / f"model-{seed}/greedy", deadline, "greedy", deck=deck)
                record(seed, "greedy", config.max_decisions, (outcomes, timing))
        # Neutral search does not depend on model initialization: run it once.
        for bound in budgets:
            record(None, "neutral", bound, search(None, panel, config, output / f"neutral-{bound}",
                                                   deadline, bound, "neutral", deck=deck))
        for seed, model in models.items():
            directory = output / f"model-{seed}"
            record(seed, "sampled_restarts", repeats,
                   repeated_policy(model, panel, config, directory / "sampled", deadline, repeats, 70000 + seed, deck=deck))
            for bound in budgets:
                for mode in ("policy", "policy_value"):
                    record(seed, mode, bound, search(model, panel, config, directory / f"{mode}-{bound}",
                                                     deadline, bound, mode, deck=deck))
                    for steps in rollout_steps:
                        name = f"{mode}_rollout_{steps}"
                        record(seed, name, bound, search(model, panel, config, directory / f"{name}-{bound}",
                                                         deadline, bound, mode, steps, deck=deck))
        write_json(output / "result.json", {"status": "complete", "evaluation": summary,
                                             "wall_seconds": time.perf_counter() - wall_start})
    except Exception as error:
        write_json(output / "failure.json", {"type": type(error).__name__, "message": str(error),
                                              "completed": summary})
        raise


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--training", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--budgets", type=int, nargs="+", default=[256, 1024])
    parser.add_argument("--repeats", type=int, default=8)
    parser.add_argument("--max-wall-seconds", type=float, default=900)
    parser.add_argument("--rollout-steps", type=int, nargs="*", default=[])
    parser.add_argument("--regression-baseline", type=Path)
    parser.add_argument("--panel", type=Path, help="new evaluation seed panel JSON, separate from training configuration")
    args = parser.parse_args()
    run(args.training, args.output, tuple(args.budgets), args.repeats, args.max_wall_seconds,
        tuple(args.rollout_steps), args.regression_baseline,
        EvaluationPanel(**json.loads(args.panel.read_text(encoding="utf-8"))) if args.panel else None)


if __name__ == "__main__":
    main()
