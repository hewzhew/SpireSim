"""One predeclared PPO diagnostic on fresh constructed Rust combats."""
import argparse
from collections import Counter
import copy
import gzip
import hashlib
import inspect
import json
from pathlib import Path
import subprocess
import sys
import time

import gymnasium
import numpy as np
import stable_baselines3
import sts_learning_bridge._native as native
import torch
from stable_baselines3 import PPO
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.logger import configure
from stable_baselines3.common.vec_env import DummyVecEnv

from .combat_env import CombatEnv
from .combat_policy import CombatPolicy
from .trial import PARAMETERS, ROOT, SEEDS, write


DECKS = {
    "compact_bash": ["Bash", "Strike", "Strike", "Defend", "Defend"],
    "compact_bludgeon": ["Bludgeon", "Strike", "Strike", "Defend", "Defend"],
    "compact_draw": ["Bludgeon", "Strike", "Strike", "Defend", "Defend", "ShrugItOff"],
    "held_mix": ["Bash", "Bludgeon", "Strike", "Strike", "Defend", "Defend", "ShrugItOff"],
    "starter_bludgeon_stress": ["Strike"] * 5 + ["Defend"] * 4 + ["Bash", "AscendersBane", "Bludgeon"],
}
PARAMS = {**PARAMETERS, "gamma": 1., "batch_size": 64}
STEPS = 32768


def recipes(names, hps=(24, 48, 68)):
    return [{"family": name, "source": "explicit_compact_construction" if name.startswith("compact") else
             "explicit_held_recipe" if name == "held_mix" else "constructed_starter_plus_one_stress",
             "spec": {"seed": 0, "encounter": enemy, "act": 1, "room": room,
                      "hp": hp, "deck": [[card, 0] for card in DECKS[name]], "relics": [],
                      "potions": [], "max_potion_uses": None}}
            for name in names for enemy, room in (("JawWorm", "MonsterRoom"), ("GremlinNob", "MonsterRoomElite"))
            for hp in hps]


def cases(recipes_, start, repeats):
    result = []
    for i, recipe in enumerate(recipes_):
        for j in range(repeats):
            c = copy.deepcopy(recipe)
            c["spec"]["seed"] = start + i * 100 + j
            c["case_id"] = f"{c['family']}-{c['spec']['encounter']}-hp{c['spec']['hp']}-{j}"
            result.append(c)
    return result


def summarize(rows):
    status = Counter(row["status"] for row in rows)
    wins = [r for r in rows if r["status"] == "win"]
    completed = [r for r in rows if r["status"] in ("win", "loss")]
    hp = [r["outcome"]["hp"] for r in wins]
    damage = [r["case"]["spec"]["hp"] + 6 - r["outcome"]["hp"] for r in wins]
    return {"episodes": len(rows), "status": dict(status),
            "unknown": len(rows) - len(completed),
            "mean_terminal_utility": float(np.mean([r["reward"] for r in completed])) if completed else None,
            "winning_hp_min_median_max": [int(np.min(hp)), float(np.median(hp)), int(np.max(hp))] if hp else None,
            "mean_winning_hp": float(np.mean(hp)) if hp else None,
            # All declared recipes start <=68 HP and have no healing card/potion.
            "mean_winning_combat_damage": float(np.mean(damage)) if damage else None,
            "mean_decisions": float(np.mean([r["decisions"] for r in rows])) if rows else None}


def grouped(rows):
    result = {"all": summarize(rows)}
    for field, key in (("family", lambda r: r["case"]["family"]),
                       ("encounter", lambda r: r["case"]["spec"]["encounter"]),
                       ("starting_hp", lambda r: str(r["case"]["spec"]["hp"]))):
        result[field] = {value: summarize([r for r in rows if key(r) == value]) for value in sorted({key(r) for r in rows})}
    return result


def evaluate(model, panel, directory, name, *, modes=("greedy", "sampled"), deadline=float("inf")):
    model.policy.set_training_mode(False)
    rows = []
    with gzip.open(directory / f"{name}.jsonl.gz", "xt", encoding="utf-8", compresslevel=3) as stream:
        for mode in modes:
            for index, c in enumerate(panel):
                if time.monotonic() >= deadline:
                    raise TimeoutError("evaluation incomplete; partial evidence retained")
                records = []
                env = CombatEnv([c], sink=records.append)
                trace = []
                total = 0.
                row = None
                try:
                    obs, _ = env.reset(options={"case": c})
                    rng = np.random.default_rng(9000000 + index)
                    while True:
                        with torch.no_grad():
                            tensor, _ = model.policy.obs_to_tensor(obs)
                            distribution, value = model.policy._distribution_value(tensor)
                            probabilities = distribution.distribution.probs[0].cpu().numpy().astype(float)
                        count = int(obs["mask"].sum())
                        action = int(probabilities.argmax() if mode == "greedy" else
                                     rng.choice(len(probabilities), p=probabilities / probabilities.sum()))
                        trace.append({"reading": env.reading(), "probabilities": probabilities[:count].tolist(),
                                      "value": float(value.item()), "ordinal": action})
                        obs, reward, terminated, truncated, info = env.step(action)
                        total += reward
                        if terminated or truncated:
                            row = {"case": c, "mode": mode, "status": info["status"], "reward": total,
                                   "outcome": info.get("outcome"), "decisions": len(trace)}
                            rows.append(row)
                            break
                finally:
                    env.close()
                    stream.write(json.dumps({"result": row, "case": c, "mode": mode,
                                             "decisions": trace, "records": records}, allow_nan=False) + "\n")
    result = {mode: grouped([r for r in rows if r["mode"] == mode]) for mode in modes}
    write(directory / f"{name}-summary.json", result)
    return result


class TrainingLog:
    def __init__(self, path):
        self.stream = gzip.open(path, "xt", encoding="utf-8", compresslevel=1)
        self.rows = []

    def __call__(self, row):
        self.stream.write(json.dumps(row, allow_nan=False) + "\n")
        outcome = row["outcome"]
        self.rows.append({"case": row["case"], "status": row["status"], "outcome": outcome,
                          "reward": outcome["hp"] / 75 if row["status"] == "win" else -1. if row["status"] == "loss" else None,
                          "decisions": len(row["trace"])})

    def close(self):
        self.stream.close()


class Diagnostics(BaseCallback):
    def __init__(self, directory, development, log, deadline):
        super().__init__()
        self.directory, self.development, self.log, self.deadline = directory, development, log, deadline
        self.cut = False
        self.curve = []

    def _on_step(self):
        if time.monotonic() >= self.deadline:
            self.cut = True
            return False
        return True

    def record(self):
        steps = self.model.num_timesteps
        assessment = evaluate(self.model, self.development, self.directory, f"development-{steps}",
                              modes=("greedy",), deadline=self.deadline)
        row = {"steps": steps, "development": assessment, "training": grouped(self.log.rows)}
        self.curve.append(row)
        write(self.directory / "curve.json", self.curve)
        data = assessment["greedy"]["all"]
        print(f"{self.directory.name} steps={steps} outcomes={data['status']} "
              f"winning_damage={data['mean_winning_combat_damage']}", flush=True)

    def _on_rollout_start(self):
        if self.num_timesteps and self.num_timesteps % 8192 == 0:
            self.record()


def run(output, wall_seconds):
    if not np.isfinite(wall_seconds) or wall_seconds <= 0:
        raise ValueError("invalid wall limit")
    if stable_baselines3.__version__ != "2.7.0" or gymnasium.__version__ != "1.2.0":
        raise ValueError("reference versions changed")
    output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    deadline = started + wall_seconds
    training = recipes(tuple(DECKS)[:3])
    development = cases(training, 500000, 1)
    held = cases(training, 1000000, 4) + cases(recipes(("held_mix",), (48, 68)), 2000000, 4) + cases(
        recipes(("starter_bludgeon_stress",), (48, 68)), 3000000, 4)
    plan = {"contract": "sb3_entity_combat_diagnostic_v1", "training": training,
            "training_game_seeds": [10000, 20000], "development": development, "held_evaluation": held,
            "initializations": SEEDS, "parameters": PARAMS, "steps_per_initialization": STEPS,
            "parallel_envs": 4, "threads": 1, "network": {"hidden": 32, "layers": 1, "initialization": "fresh"},
            "capacity": {"entities": 192, "actions": 32, "overflow": "error"},
            "external_decision_limit": 64, "core_limit": 512, "wall_seconds": wall_seconds,
            "reward": "true win: terminal HP / 75 including Burning Blood; true loss: -1; otherwise 0",
            "information": "entity schema4 oracle-scoped; RNG/seed/source identities excluded",
            "evaluation": "fixed final checkpoint; greedy and sampled; new seeds, held recipe, starter stress separate",
            "decision_rule": "diagnostic only: compare initial/final outcomes AND HP cost AND individual behavior; no automatic promotion",
            "teacher_or_search_labels": False, "llm": False, "hyperparameter_retry": False,
            "claim_limits": "constructed small decks; no natural Act1, NoSL, resource/potion strategy or Java parity claim"}
    write(output / "plan.json", plan)
    sources = list(Path(__file__).parent.glob("*.py")) + [ROOT / "tools/guided_search/entity_learning.py",
        Path(inspect.getfile(stable_baselines3.ppo.ppo)), Path(inspect.getfile(stable_baselines3.common.on_policy_algorithm)),
        Path(inspect.getfile(stable_baselines3.common.buffers)), Path(inspect.getfile(stable_baselines3.common.policies))]
    source_dir = output / "sources"
    source_dir.mkdir()
    hashes = {}
    for i, path in enumerate(sources):
        raw = path.read_bytes()
        hashes[str(path)] = hashlib.sha256(raw).hexdigest()
        (source_dir / f"{i}-{path.name}").write_bytes(raw)
    write(output / "producer.json", {"head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "status": subprocess.check_output(["git", "status", "--short"], text=True), "python": sys.version,
        "executable": sys.executable, "sources": hashes, "native": native.__file__,
        "native_sha256": hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest(),
        "native_source_attestation": "binary identity recorded; no new build or source equivalence claim",
        "torch": torch.__version__})
    (output / "packages.txt").write_bytes(subprocess.check_output([sys.executable, "-m", "pip", "freeze"]))
    torch.set_num_threads(1)
    report = {"status": "running", "attempted": [], "models": []}
    try:
        for seed in SEEDS:
            if time.monotonic() >= deadline:
                raise TimeoutError("declared trial budget exhausted")
            directory = output / str(seed)
            directory.mkdir()
            report["attempted"].append(seed)
            log = TrainingLog(directory / "training.jsonl.gz")
            envs = DummyVecEnv([lambda: CombatEnv(training, sink=log) for _ in range(4)])
            model = None
            try:
                model = PPO(CombatPolicy, envs, seed=seed, verbose=0, **PARAMS)
                envs.seed(1000 + seed * 100)
                model.set_logger(configure(str(directory), ["csv"]))
                model.save(directory / "initial")
                initial = evaluate(model, held, directory, "initial-evaluation", deadline=deadline)
                diagnostics = Diagnostics(directory, development, log, deadline)
                diagnostics.model = model
                diagnostics.record()
                model.learn(total_timesteps=STEPS, callback=diagnostics)
                model.save(directory / "final")
                if diagnostics.cut or model.num_timesteps != STEPS:
                    raise TimeoutError("incomplete PPO budget; partial weights retained")
                diagnostics.record()
                final = evaluate(model, held, directory, "final-evaluation", deadline=deadline)
                report["models"].append({"seed": seed, "steps": model.num_timesteps, "initial": initial, "final": final})
                write(output / "report.json", report)
            finally:
                if model is not None:
                    model.save(directory / "last")
                envs.close()
                log.close()
                write(directory / "training-summary.json", grouped(log.rows))
        report["status"] = "complete"
    except BaseException as error:
        report["status"] = "interrupted" if isinstance(error, (TimeoutError, KeyboardInterrupt)) else "error"
        report["error"] = repr(error)
        raise
    finally:
        report["seconds"] = time.monotonic() - started
        report["unattempted"] = [s for s in SEEDS if s not in report["attempted"]]
        write(output / "report.json", report)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--wall-seconds", type=float, default=1200)
    args = parser.parse_args()
    run(args.output, args.wall_seconds)
