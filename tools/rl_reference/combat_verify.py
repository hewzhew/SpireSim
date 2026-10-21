"""Reload frozen policies and replay every retained combat evaluation step."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path

import numpy as np
from stable_baselines3 import PPO
import sts_learning_bridge._native as native
import torch

from .combat_env import CombatEnv, fingerprint
from .trial import write


def verify(directory):
    producer = json.loads((directory / "producer.json").read_text(encoding="utf-8"))
    if hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest() != producer["native_sha256"]:
        raise ValueError("native producer differs; this is not same-producer verification")
    for path, digest in producer["sources"].items():
        if hashlib.sha256(Path(path).read_bytes()).hexdigest() != digest:
            raise ValueError(f"recorded source changed: {path}")
    torch.set_num_threads(1)
    report = {"models": [], "episodes": 0, "steps": 0}
    plan = json.loads((directory / "plan.json").read_text(encoding="utf-8"))
    for seed in plan["initializations"]:
        for checkpoint in ("initial", "final"):
            model = PPO.load(directory / str(seed) / checkpoint, device="cpu")
            model.policy.set_training_mode(False)
            count, steps = 0, 0
            with gzip.open(directory / str(seed) / f"{checkpoint}-evaluation.jsonl.gz", "rt", encoding="utf-8") as stream:
                for line in stream:
                    row = json.loads(line)
                    records = []
                    env = CombatEnv([row["case"]], sink=records.append)
                    obs, _ = env.reset(options={"case": row["case"]})
                    for saved, record in zip(row["decisions"], row["records"][0]["trace"], strict=True):
                        assert env.reading() == saved["reading"]
                        assert fingerprint(obs) == record["observation"]
                        with torch.no_grad():
                            tensor, _ = model.policy.obs_to_tensor(obs)
                            distribution, value = model.policy._distribution_value(tensor)
                            probabilities = distribution.distribution.probs[0].cpu().numpy()
                        np.testing.assert_array_equal(probabilities[:len(saved["probabilities"])], saved["probabilities"])
                        assert float(value.item()) == saved["value"]
                        obs, _, _, _, _ = env.step(saved["ordinal"])
                        steps += 1
                    env.close()
                    assert records == row["records"]
                    count += 1
            report["models"].append({"seed": seed, "checkpoint": checkpoint, "episodes": count, "steps": steps})
            report["episodes"] += count
            report["steps"] += steps
            print(f"verified {seed}/{checkpoint}: {count} episodes, {steps} steps", flush=True)
    write(directory / "verification.json", report)
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    verify(parser.parse_args().directory)
