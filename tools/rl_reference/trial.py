"""Bounded SB3 baseline. No StS state, labels, search or model promotion."""
import argparse
import gzip
import hashlib
import importlib.metadata
import inspect
import json
from pathlib import Path
import subprocess
import sys
import time

import gymnasium
import numpy as np
import stable_baselines3
import torch
from stable_baselines3 import PPO
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.logger import configure
from stable_baselines3.common.monitor import Monitor
from stable_baselines3.common.vec_env import DummyVecEnv

from .tasks import TASKS, make
from .evaluation import assessment, empirical

ROOT = Path(__file__).resolve().parents[2]
SEEDS = (17, 29, 43)
PARAMETERS = dict(learning_rate=3e-4, n_steps=128, batch_size=256, n_epochs=4,
                  gamma=.99, gae_lambda=.95, clip_range=.2, ent_coef=.01,
                  vf_coef=.5, max_grad_norm=.5, device="cpu")


def write(path, obj):
    path.write_text(json.dumps(obj, indent=2, allow_nan=False), encoding="utf-8")


class Diagnostics(BaseCallback):
    def __init__(self, task, directory, deadline):
        super().__init__()
        self.task, self.directory, self.deadline = task, directory, deadline
        self.rows = []
        self.interval = task.steps // 8
        self.cut = False

    def record(self):
        row = {"steps": self.model.num_timesteps, **assessment(self.model, self.task)}
        self.rows.append(row)
        write(self.directory / "curve.json", self.rows)
        if row["steps"] in (0, self.task.steps // 2, self.task.steps):
            print(f"{self.directory.name} steps={row['steps']} expected={row['sampled']['discounted_return']:.3f} "
                  f"success={row['sampled']['success_within_limit']:.3f}", flush=True)

    def _on_training_start(self):
        self.record()

    def _on_step(self):
        if time.monotonic() >= self.deadline:
            self.cut = True
            return False
        return True

    def _on_rollout_start(self):
        if self.num_timesteps and self.num_timesteps % self.interval == 0:
            self.record()  # Previous rollout's PPO updates have completed.


def run(output, wall_seconds):
    if not np.isfinite(wall_seconds) or wall_seconds <= 0:
        raise ValueError("invalid trial deadline")
    if stable_baselines3.__version__ != "2.7.0" or gymnasium.__version__ != "1.2.0":
        raise ValueError("reference package versions differ from the declared baseline")
    output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    deadline = started + wall_seconds
    plan = {"contract": "rl_reference_diagnostics_v1", "learner": "stable_baselines3.PPO 2.7.0",
            "tasks": [t.specification() for t in TASKS], "initializations": SEEDS,
            "parameters": PARAMETERS, "parallel_envs": 8, "network": [64, 64], "threads": 1,
            "environment_seeds": "10000 + model_seed * 100 + env_index",
            "evaluation_seeds": "1000000 + task_index * 10000 + [0..511]",
            "wall_seconds": wall_seconds, "evaluation": "exact known-kernel policy evaluation + 512 independent episodes",
            "admission": "each final sampled policy must meet task.minimum_optimal_fraction; no best-checkpoint selection",
            "weights": "random initialization per task/seed; no pretrained weights",
            "search_or_teacher": False, "training_targets": "native rewards and SB3 value estimates only",
            "sts_capability_claim": False, "automatic_hyperparameter_retry": False}
    write(output / "plan.json", plan)
    source_dir = output / "sources"
    source_dir.mkdir()
    modules = (stable_baselines3.ppo.ppo, stable_baselines3.common.on_policy_algorithm,
               stable_baselines3.common.buffers, stable_baselines3.common.vec_env.dummy_vec_env)
    sources = list(Path(__file__).parent.glob("*.py")) + [Path(inspect.getfile(m)) for m in modules]
    hashes = {}
    for i, p in enumerate(sources):
        raw = p.read_bytes()
        hashes[str(p)] = hashlib.sha256(raw).hexdigest()
        (source_dir / f"{i}-{p.name}").write_bytes(raw)
    write(output / "producer.json", {"head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "status": subprocess.check_output(["git", "status", "--short"], cwd=ROOT, text=True),
        "python": sys.version, "executable": sys.executable, "source_sha256": hashes,
        "packages": {n: importlib.metadata.version(n) for n in ("torch", "numpy", "stable-baselines3", "gymnasium", "matplotlib")}})
    (output / "packages.txt").write_bytes(subprocess.check_output([sys.executable, "-m", "pip", "freeze"]))
    torch.set_num_threads(1)
    report = {"cases": [], "attempts": [], "status": "running"}
    try:
        for task_index, task in enumerate(TASKS):
            for seed in SEEDS:
                if time.monotonic() >= deadline:
                    raise TimeoutError("trial budget exhausted; remaining declared cases unattempted")
                directory = output / f"{task.name}-{seed}"
                directory.mkdir()
                report["attempts"].append({"task": task.name, "seed": seed})
                envs = DummyVecEnv([lambda: Monitor(make(task)) for _ in range(8)])
                try:
                    model = PPO("MlpPolicy", envs, seed=seed, verbose=0, **PARAMETERS)
                    envs.seed(10000 + seed * 100)
                    model.set_logger(configure(str(directory), ["csv"]))
                    model.save(directory / "initial")
                    diagnostics = Diagnostics(task, directory, deadline)
                    model.learn(total_timesteps=task.steps, callback=diagnostics)
                    diagnostics.record()
                    model.save(directory / "final")
                    complete = not diagnostics.cut and model.num_timesteps == task.steps
                    final = diagnostics.rows[-1]
                    evidence = empirical(task, np.array(final["probabilities"]),
                                         range(1000000 + task_index * 10000, 1000000 + task_index * 10000 + 512))
                    with gzip.open(directory / "evaluation.json.gz", "xt", encoding="utf-8") as f:
                        json.dump(evidence, f)
                    case = {"task": task.name, "seed": seed, "steps": model.num_timesteps, "complete": complete,
                            "initial": diagnostics.rows[0], "final": final, "empirical": evidence["counts"],
                            "criterion_met": complete and final["sampled"]["optimal_fraction"] >= task.minimum_optimal_fraction}
                    report["cases"].append(case)
                    write(output / "report.json", report)
                    if not complete:
                        raise TimeoutError("partial PPO collection retained; no complete-run claim")
                finally:
                    envs.close()
        report["status"] = "complete"
        report["all_criteria_met"] = all(c["criterion_met"] for c in report["cases"])
    except BaseException as error:
        report["status"] = "interrupted" if isinstance(error, (TimeoutError, KeyboardInterrupt)) else "error"
        report["error"] = repr(error)
        raise
    finally:
        report["seconds"] = time.monotonic() - started
        report["unattempted"] = [{"task": t.name, "seed": s} for t in TASKS for s in SEEDS
                                 if not any(c["task"] == t.name and c["seed"] == s for c in report["attempts"])]
        write(output / "report.json", report)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--wall-seconds", type=float, default=600)
    args = parser.parse_args()
    run(args.output, args.wall_seconds)
