"""One-action deviations followed by the same frozen greedy combat policy.

This is a diagnostic intervention, not optimal search or training material.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import time

import numpy as np
from stable_baselines3 import PPO
import torch

from .combat_env import CombatEnv
from .trial import write


def groups(reading):
    combat = reading["combat"]
    attack, defense, lethal = [], [], []
    for i, action in enumerate(reading["choices"]):
        if action["kind"] != "play":
            continue
        card = combat["hand"][action["hand"]]
        if card.get("block", 0) > 0:
            defense.append(i)
        target = action.get("target")
        if target is not None and card.get("damage", 0) > 0:
            attack.append(i)
            enemy = combat["monsters"][target]
            if card["damage_by_target"][target] >= enemy["hp"][0] + enemy.get("block", 0):
                lethal.append(i)  # These recipes contain only single-hit damage.
    incoming = 0
    for monster in combat["monsters"]:
        if not monster["alive"]:
            continue
        intent = monster["intent"]
        if intent["hits"] == 0:
            continue
        if intent["total_damage"] is None:
            raise ValueError("unobserved incoming damage in deviation selection")
        incoming += intent["total_damage"]
    return {"attack": attack, "defense": defense, "single_card_lethal": lethal,
            "incoming_exceeds_block": incoming > combat.get("block", 0)}


def selected_states(row):
    wanted = {"attack_defense", "single_card_lethal"}
    for index, decision in enumerate(row["decisions"]):
        g = groups(decision["reading"])
        matched = []
        if "attack_defense" in wanted and g["attack"] and g["defense"] and g["incoming_exceeds_block"]:
            matched.append("attack_defense")
        if "single_card_lethal" in wanted and g["single_card_lethal"] and g["defense"]:
            matched.append("single_card_lethal")
        if matched:
            yield index, matched, g
            wanted.difference_update(matched)
        if not wanted:
            return


def run(directory, output, wall_seconds=300):
    output.mkdir(parents=True, exist_ok=False)
    plan = json.loads((directory / "plan.json").read_text(encoding="utf-8"))
    write(output / "plan.json", {"producer_trial": str(directory.resolve()), "model_seeds": plan["initializations"],
        "selection": "final greedy held episodes: first attack+defense under incoming damage; first single-card lethal with defense available",
        "intervention": "every legal first action, then unchanged greedy policy to the same 64-decision limit",
        "interpretation": "one-step deviation only; not optimal values, policy labels or a comparison with human play",
        "unknown": "never ranked as loss", "wall_seconds": wall_seconds,
        "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()})
    torch.set_num_threads(1)
    started = time.monotonic()
    report = {"states": [], "status": "running"}
    try:
        for seed in plan["initializations"]:
            model = PPO.load(directory / str(seed) / "final", device="cpu")
            model.policy.set_training_mode(False)
            with gzip.open(directory / str(seed) / "final-evaluation.jsonl.gz", "rt", encoding="utf-8") as source, gzip.open(
                    output / f"{seed}-deviations.jsonl.gz", "xt", encoding="utf-8", compresslevel=1) as evidence:
                for line in source:
                    row = json.loads(line)
                    if row["mode"] != "greedy":
                        continue
                    for index, kinds, g in selected_states(row):
                        reading = row["decisions"][index]["reading"]
                        alternatives = []
                        for ordinal in range(len(reading["choices"])):
                            if time.monotonic() - started >= wall_seconds:
                                raise TimeoutError("deviation budget exhausted")
                            records = []
                            env = CombatEnv([row["case"]], sink=records.append)
                            try:
                                obs, _ = env.reset(options={"case": row["case"]})
                                for decision in row["decisions"][:index]:
                                    obs, _, terminated, truncated, _ = env.step(decision["ordinal"])
                                    assert not terminated and not truncated
                                assert env.reading() == reading
                                action = ordinal
                                reward = 0.
                                while True:
                                    obs, delta, terminated, truncated, info = env.step(action)
                                    reward += delta
                                    if terminated or truncated:
                                        break
                                    action, _ = model.predict(obs, deterministic=True)
                                alternative = {"ordinal": ordinal, "status": info["status"],
                                               "reward": reward if terminated else None, "outcome": info.get("outcome")}
                                alternatives.append(alternative)
                            finally:
                                env.close()
                                evidence.write(json.dumps({"case": row["case"], "index": index, "ordinal": ordinal,
                                                           "records": records}, allow_nan=False) + "\n")
                        selected = row["decisions"][index]["ordinal"]
                        # Replaying the original first action must recover the entire original endpoint.
                        assert alternatives[selected]["outcome"] == row["result"]["outcome"]
                        complete = all(a["reward"] is not None for a in alternatives)
                        result = {"seed": seed, "case": row["case"], "index": index, "kinds": kinds,
                                  "groups": g, "reading": reading, "selected": selected,
                                  "probabilities": row["decisions"][index]["probabilities"],
                                  "alternatives": alternatives, "complete": complete}
                        if complete:
                            result["one_step_gap"] = max(a["reward"] for a in alternatives) - alternatives[selected]["reward"]
                            if "attack_defense" in kinds:
                                result["defense_minus_attack"] = max(alternatives[i]["reward"] for i in g["defense"]) - max(
                                    alternatives[i]["reward"] for i in g["attack"])
                        report["states"].append(result)
            print(f"deviation audit {seed}: {len(report['states'])} cumulative states", flush=True)
        report["status"] = "complete"
    except BaseException as error:
        report["status"] = "interrupted" if isinstance(error, (TimeoutError, KeyboardInterrupt)) else "error"
        report["error"] = repr(error)
        raise
    finally:
        report["seconds"] = time.monotonic() - started
        write(output / "report.json", report)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--wall-seconds", type=float, default=300)
    args = parser.parse_args()
    run(args.directory, args.output, args.wall_seconds)
