"""Verify saved frozen policies and replay their actual evaluation actions."""
import argparse
import gzip
import json
from pathlib import Path

import numpy as np
from stable_baselines3 import PPO
import torch

from .tasks import TASKS, make
from .evaluation import assessment


def verify(directory):
    plan = json.loads((directory / "plan.json").read_text())
    report = json.loads((directory / "report.json").read_text())
    if plan["tasks"] != [t.specification() for t in TASKS]:
        raise ValueError("task declarations changed; do not reinterpret old evidence")
    # CPU thread count can change floating-point accumulation. Reuse the producer setting.
    torch.set_num_threads(plan["threads"])
    checks = []
    for case in report["cases"]:
        task = next(t for t in TASKS if t.name == case["task"])
        folder = directory / f"{task.name}-{case['seed']}"
        model = PPO.load(folder / "final", device="cpu")
        calculated = assessment(model, task)
        for field in ("probabilities", "predicted_values"):
            np.testing.assert_array_equal(calculated[field], case["final"][field])
        if calculated != {k: v for k, v in case["final"].items() if k != "steps"}:
            raise ValueError("saved exact policy assessment changed")
        with gzip.open(folder / "evaluation.json.gz", "rt", encoding="utf-8") as f:
            evidence = json.load(f)
        env = make(task)
        steps = 0
        counts = dict(success=0, failure=0, unresolved=0)
        try:
            for row in evidence["rows"]:
                obs, _ = env.reset(seed=row["seed"])
                terminated = truncated = False
                for s, action, nxt, reward, term, trunc in row["trace"]:
                    if terminated or truncated or obs != s:
                        raise ValueError("evaluation trace source changed")
                    actual = env.step(action)
                    if list(actual[:4]) != [nxt, reward, term, trunc]:
                        raise ValueError("evaluation transition changed")
                    obs, reward, terminated, truncated = actual[:4]
                    steps += 1
                if not terminated and not truncated:
                    raise ValueError("evaluation trace has no endpoint")
                ending = "success" if terminated and reward > 0 else "failure" if terminated else "unresolved"
                if row["ending"] != ending:
                    raise ValueError("evaluation endpoint changed")
                counts[ending] += 1
        finally:
            env.close()
        if counts != evidence["counts"] or counts != case["empirical"] or sum(counts.values()) != evidence["episodes"]:
            raise ValueError("evaluation counts changed")
        checks.append({"task": task.name, "seed": case["seed"], "loaded_policy_and_value_match": True,
                       "evaluation_episodes_replayed": len(evidence["rows"]), "steps_replayed": steps})
    path = directory / "verification.json"
    if path.exists():
        raise ValueError("verification receipt already exists")
    path.write_text(json.dumps(checks, indent=2), encoding="utf-8")
    return checks


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    checks = verify(parser.parse_args().directory)
    print(f"{len(checks)} frozen policies; {sum(c['evaluation_episodes_replayed'] for c in checks)} episode replays")
