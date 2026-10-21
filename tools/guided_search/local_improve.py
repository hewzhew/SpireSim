"""Bounded search-imitation versus continued direct local learning.

Both arms load identical declared local weights, reset optimizers, and use the
same fresh prescribed training roots. Work is measured, not assumed equal.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass, replace
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch

from tools.guided_search.local_compare import (
    EvaluationPanel, load_training, search, summarize, verify_direct_regression,
)
from tools.guided_search.local_demonstrations import imitate, load_demonstrations
from tools.guided_search.local_train import (
    ROOT, cases, check_deadline, hash_file, network, rollout, save_weights, update, write_json,
)


@dataclass(frozen=True)
class Improvement:
    train_seed_start: int
    train_seed_count: int
    eval_seed_start: int
    eval_seed_count: int
    generations: int = 8
    search_services: int = 256
    rollout_steps: int = 128
    learning_rate: float = 0.0001
    max_wall_seconds: float = 1200

    def validate(self, source):
        training = EvaluationPanel(self.train_seed_start, self.train_seed_count, "development")
        evaluation = EvaluationPanel(self.eval_seed_start, self.eval_seed_count, "development")
        training.validate(source)
        evaluation.validate(source)
        if set(training.seeds()) & set(evaluation.seeds()):
            raise ValueError("improvement training and evaluation seed groups overlap")
        if (type(self.generations) is not int or not 1 <= self.generations <= 128
                or type(self.search_services) is not int or not 1 <= self.search_services <= 16384
                or type(self.rollout_steps) is not int or not 1 <= self.rollout_steps <= 512
                or not np.isfinite(self.learning_rate) or self.learning_rate <= 0
                or not np.isfinite(self.max_wall_seconds) or self.max_wall_seconds <= 0):
            raise ValueError("invalid improvement budget or learning rate")


def run(training, output, settings, regression_baseline=None):
    start_wall = time.perf_counter()
    deadline = time.monotonic() + settings.max_wall_seconds
    training = training.resolve()
    regression_baseline = regression_baseline.resolve() if regression_baseline else None
    source, weight_hashes, binary_hashes, training_binary = load_training(training, regression_baseline)
    settings.validate(source)
    config = replace(source, train_seed_start=settings.train_seed_start,
                     train_seed_count=settings.train_seed_count, eval_seed_start=settings.eval_seed_start,
                     eval_seed_count=settings.eval_seed_count, generations=settings.generations,
                     learning_rate=settings.learning_rate, evaluation_partition="development")
    config.validate()
    output.mkdir(parents=True, exist_ok=False)
    torch.set_num_threads(config.threads)
    write_json(output / "contract.json", {
        "settings": asdict(settings), "config": asdict(config), "source_config": asdict(source),
        "training": str(training), "weights_sha256": weight_hashes,
        "training_cases": cases(config.train_seeds(), config.task),
        "evaluation_cases": cases(config.eval_seeds(), config.task),
        "evaluation_partition": "development", "information": "fixed_oracle",
        "arms": ["initial", "direct_rl", "search_imitation"],
        "optimizers": "fresh Adam per arm; identical learning rate; one update per cohort",
        "search": "first replayed victory, policy-only, bounded continuation in shared tree",
        "imitation": "mean negative demonstrated-action log probability per episode, then mean episodes; no value target",
        "unknown": "retained and excluded from demonstration supervision; not losses",
        "selection": "all initializations, last declared generation, sampled and greedy direct evaluation",
        "compute": "matched roots and update opportunities, measured actual work; not matched compute",
        "regression_baseline": str(regression_baseline) if regression_baseline else None,
    })
    write_json(output / "manifest.json", {
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "status": subprocess.check_output(["git", "status", "--short"], cwd=ROOT, text=True),
        "binary_sha256": binary_hashes, "training_binary_sha256": training_binary,
        "training_contract_sha256": hash_file(training / "contract.json"),
        "torch": torch.__version__, "numpy": np.__version__,
    })
    (output / "producer.patch").write_bytes(subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT))
    producer = output / "sources"
    producer.mkdir()
    for rel in ["tools/guided_search/local_improve.py", "tools/guided_search/local_demonstrations.py",
                "tools/guided_search/local_train.py", "tools/guided_search/local_compare.py",
                "tools/guided_search/local_tactics.py", "crates/sts_guided_search/src/local.rs",
                "crates/sts_guided_search/src/local_tactics.rs", "crates/sts_guided_search/src/tree.rs",
                "bindings/python_learning/src/guided.rs"]:
        (producer / rel.replace("/", "__")).write_bytes((ROOT / rel).read_bytes())
    summary, training_work, regression = [], [], []
    initial_weights, final_weights = {}, {}
    try:
        # All original-binary controls finish before collecting new learning data.
        for seed, path in zip(config.model_seeds, weight_hashes):
            check_deadline(deadline)
            directory = output / f"model-{seed}"
            directory.mkdir()
            model = network(config).to(config.device).eval()
            with np.load(path, allow_pickle=False) as data:
                model.load_state_dict({k: torch.as_tensor(data[k], device=config.device) for k in data.files})
            initial_weights[seed] = {k: v.detach().clone() for k, v in model.state_dict().items()}
            save_weights(directory / "initial.npz", model)
            if regression_baseline:
                check_dir = directory / "regression"
                rollout(model, cases(source.eval_seeds(), source.task), source,
                        torch.Generator(device=config.device).manual_seed(seed), check_dir, deadline, "greedy")
                regression.append({"model": seed, **verify_direct_regression(
                    check_dir, regression_baseline / f"model-{seed}/greedy")})
        write_json(output / "regression.json", regression)
        for seed in config.model_seeds:
            for arm in ("direct_rl", "search_imitation"):
                directory = output / f"model-{seed}" / arm
                directory.mkdir()
                model = network(config).to(config.device).eval()
                model.load_state_dict(initial_weights[seed])
                save_weights(directory / "model-0.npz", model)
                optimizer = torch.optim.Adam(model.parameters(), lr=config.learning_rate)
                generator = torch.Generator(device=config.device).manual_seed(seed + 8000)
                for generation in range(1, config.generations + 1):
                    check_deadline(deadline)
                    cohort = directory / f"train-{generation}"
                    if arm == "direct_rl":
                        records, outcomes, timing = rollout(model, cases(config.train_seeds(), config.task),
                                                            config, generator, cohort, deadline)
                        check_deadline(deadline)
                        start = time.perf_counter()
                        metrics = update(model, optimizer, records, outcomes, config)
                    else:
                        outcomes, timing = search(model, cases(config.train_seeds(), config.task), config,
                            cohort, deadline, settings.search_services, "policy", settings.rollout_steps,
                            demonstrations=True)
                        check_deadline(deadline)
                        start = time.perf_counter()
                        metrics = imitate(model, optimizer, load_demonstrations(cohort, config), config)
                    metrics["update_seconds"] = time.perf_counter() - start
                    write_json(cohort / "update.json", metrics)
                    save_weights(directory / f"model-{generation}.npz", model)
                    row = summarize(seed, arm, generation, outcomes, timing)
                    row["update"] = metrics
                    training_work.append(row)
                    if generation == 1 or generation == config.generations:
                        print(json.dumps({"model": seed, "arm": arm, "generation": generation,
                                          "wins": row["wins"], "unknown": row["unknown"]}), flush=True)
                final_weights[(seed, arm)] = {k: v.detach().clone() for k, v in model.state_dict().items()}
        # No evaluation outcome can choose an update, checkpoint, or training root.
        for seed in config.model_seeds:
            model = network(config).to(config.device).eval()
            for arm in ("initial", "direct_rl", "search_imitation"):
                model.load_state_dict(initial_weights[seed] if arm == "initial" else final_weights[(seed, arm)])
                for rule in ("sampled", "greedy"):
                    generator = torch.Generator(device=config.device).manual_seed(seed + 90000)
                    directory = output / f"model-{seed}" / f"eval-{arm}-{rule}"
                    _, outcomes, timing = rollout(model, cases(config.eval_seeds(), config.task), config,
                                                  generator, directory, deadline, rule)
                    row = summarize(seed, arm, config.max_decisions, outcomes, timing)
                    row["rule"] = rule
                    summary.append(row)
                    print(json.dumps({"model": seed, "arm": arm, "rule": rule,
                                      "wins": row["wins"], "episodes": row["episodes"]}), flush=True)
        if any(hash_file(path) != digest for path, digest in weight_hashes.items()):
            raise ValueError("source checkpoint changed during experiment")
        write_json(output / "result.json", {"status": "complete", "evaluation": summary,
            "training": training_work, "wall_seconds": time.perf_counter() - start_wall})
    except Exception as error:
        write_json(output / "failure.json", {"type": type(error).__name__, "message": str(error),
                                             "training": training_work, "evaluation": summary})
        raise


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--training", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--regression-baseline", type=Path)
    args = parser.parse_args()
    run(args.training, args.output, Improvement(**json.loads(args.config.read_text(encoding="utf-8"))),
        args.regression_baseline)


if __name__ == "__main__":
    main()
