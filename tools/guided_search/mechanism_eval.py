"""Frozen-policy diagnosis on Rust-exported bounded continuation graphs.

Python only packs existing numeric observations and integrates probabilities.
All actions, transitions, terminal outcomes and replay checks come from Rust.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

import numpy as np
import torch

from tools.guided_search.entity_learning import SCHEMA, EntityNetwork, pack, positions

ROOT = Path(__file__).resolve().parents[2]
LEAVES = {"win", "loss", "unknown_budget", "unknown_depth"}
TRAINING_SOURCES = (
    "crates/sts_guided_search/src/observation.rs",
    "crates/sts_guided_search/src/observation/monster.rs",
    "crates/sts_guided_search/src/entities.rs",
    "crates/sts_guided_search/src/selection.rs",
    "crates/sts_guided_search/src/local.rs",
    "bindings/python_learning/src/guided.rs",
    "tools/guided_search/entity_learning.py",
    "tools/guided_search/entity_trial.py",
)


def numeric_position(value, schema):
    return positions(dict(schema=schema, slots=[0],
        entity_splits=[0, len(value["ids"])], action_splits=[0, len(value["actions"])],
        entity_ids=value["ids"], entity_values=value["values"], entity_refs=value["refs"],
        action_ids=value["action_ids"], actions=value["actions"], action_refs=value["action_refs"]))[0]


def validate_case(case, schema=SCHEMA):
    """Reject malformed/incomplete action tables; cuts remain explicit leaves."""
    if schema != SCHEMA:
        raise ValueError("mechanism schema mismatch")
    nodes = case["nodes"]
    if not nodes:
        raise ValueError("empty continuation graph")
    numeric = {}
    for i, node in enumerate(nodes):
        kind = node["kind"]
        if type(node["hp"]) is not int or node["hp"] < 0:
            raise ValueError("invalid terminal HP")
        if kind == "decision":
            n = len(node["edges"])
            if not n or len(node["choices"]) != n or len(node["inputs"]) != n:
                raise ValueError("incomplete diagnostic action table")
            numeric[i] = numeric_position(node["input"], schema)
            if len(numeric[i].columns["actions"]) != n:
                raise ValueError("numeric actions differ from graph edges")
            for child in node["edges"]:
                if type(child) is not int or not 0 <= child < len(nodes):
                    raise ValueError("edge outside continuation graph")
                # The Rust enumerator may reuse one already allocated cutoff.
                if child <= i and nodes[child]["kind"] != "unknown_budget":
                    raise ValueError("non-forward continuation edge")
        elif kind in LEAVES:
            if node["edges"] or node["choices"] or node["inputs"] or "input" in node:
                raise ValueError("terminal has decision data")
            if kind == "win" and node["hp"] == 0:
                raise ValueError("win has zero HP")
        else:
            raise ValueError("unknown continuation node kind")
    reachable, pending = set(), [0]
    while pending:
        i = pending.pop()
        if i not in reachable:
            reachable.add(i)
            pending.extend(nodes[i]["edges"])
    if len(reachable) != len(nodes):
        raise ValueError("unreachable continuation nodes")
    closed = all(n["kind"] in ("decision", "win", "loss") for n in nodes)
    if type(case["closed"]) is not bool or case["closed"] != closed:
        raise ValueError("false graph closure declaration")
    if case["terminals_replayed"] != sum(n["kind"] in ("win", "loss") for n in nodes):
        raise ValueError("incomplete terminal replay accounting")
    return numeric


def integrate(case, probabilities):
    """Evaluate a validated graph, keeping unresolved probability mass separate."""
    nodes = case["nodes"]
    decisions = {i for i, n in enumerate(nodes) if n["kind"] == "decision"}
    if set(probabilities) != decisions:
        raise ValueError("policy does not cover every decision")
    # Columns: win mass, loss mass, unknown mass, HP weighted by winning mass.
    mass = np.zeros((len(nodes), 4), dtype=np.float64)
    reachable_win = [None] * len(nodes)
    best_hp = [None] * len(nodes)
    greedy = [None] * len(nodes)
    for i, n in enumerate(nodes):
        if n["kind"] in LEAVES:
            win = n["kind"] == "win"
            unknown = n["kind"].startswith("unknown_")
            mass[i] = [float(win), float(n["kind"] == "loss"), float(unknown), n["hp"] if win else 0]
            reachable_win[i] = None if unknown else win
            best_hp[i] = n["hp"] if win else None
            greedy[i] = "unknown" if unknown else n["kind"]
    for i in range(len(nodes) - 1, -1, -1):
        n = nodes[i]
        if n["kind"] != "decision":
            continue
        p = np.asarray(probabilities[i], dtype=np.float64)
        edges = n["edges"]
        if (p.shape != (len(edges),) or not np.isfinite(p).all()
                or np.any(p < 0) or not np.isclose(p.sum(), 1., atol=1e-6, rtol=0)):
            raise ValueError("invalid action probabilities")
        p = p / p.sum()
        mass[i] = p @ mass[edges]
        outcomes = [reachable_win[e] for e in edges]
        # Do not manufacture an optimal label from a partially expanded subtree.
        reachable_win[i] = any(outcomes) if all(x is not None for x in outcomes) else None
        hps = [best_hp[e] for e in edges if best_hp[e] is not None]
        best_hp[i] = max(hps) if hps and reachable_win[i] is not None else None
        greedy[i] = greedy[edges[int(p.argmax())]]
    win, loss, unknown, weighted_hp = map(float, mass[0])
    root = nodes[0]
    best = [reachable_win[e] for e in root["edges"]]
    necessary = None
    if best and all(x is not None for x in best) and any(best) and not all(best):
        necessary = [i for i, x in enumerate(best) if x]
    return dict(closed=case["closed"], win_probability=win if case["closed"] else None,
        win_probability_bounds=[win, min(1., win + unknown)],
        known_win_probability=win, known_loss_probability=loss, unknown_probability=unknown,
        expected_hp_given_win=weighted_hp / win if case["closed"] and win > 0 else None,
        best_reachable_win=reachable_win[0], best_winning_hp=best_hp[0],
        greedy_outcome=greedy[0], root_probabilities=probabilities.get(0, []),
        root_reachable_win=best, root_best_winning_hp=[best_hp[e] for e in root["edges"]],
        necessary_win_actions=necessary,
        necessary_action_mass=sum(probabilities[0][i] for i in necessary) if necessary is not None else None)


def infer(model, numeric, batch_size=32):
    ids = list(numeric)
    result = {}
    with torch.inference_mode():
        for start in range(0, len(ids), batch_size):
            selected = ids[start:start + batch_size]
            logits, _ = model(pack([numeric[i] for i in selected]))
            probabilities = logits.softmax(-1).cpu().numpy()
            for j, i in enumerate(selected):
                p = probabilities[j, :len(numeric[i].columns["actions"])].astype(np.float64)
                result[i] = (p / p.sum()).tolist()
    return result


def verify_training_sources(producer, root=ROOT):
    for name in TRAINING_SOURCES:
        saved = producer["sources"].get(name)
        if saved is None or saved != (root / name).read_text(encoding="utf-8"):
            raise ValueError(f"training source differs: {name}")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, value):
    with path.open("x", encoding="utf-8") as f:
        json.dump(value, f, indent=2, allow_nan=False)


def run(panel, trial, output):
    panel, trial, output = map(Path, (panel, trial, output))
    panel_plan = json.loads((panel / "plan.json").read_text(encoding="utf-8"))
    train_plan = json.loads((trial / "plan.json").read_text(encoding="utf-8"))
    producer = json.loads((trial / "producer.json").read_text(encoding="utf-8"))
    if panel_plan["schema"] != SCHEMA or train_plan["schema"] != SCHEMA:
        raise ValueError("schema mismatch")
    verify_training_sources(producer)
    files = sorted(panel.glob("case-*.json"))
    if len(files) != len(panel_plan["recipes"]):
        raise ValueError("panel population incomplete")
    output.mkdir(parents=True, exist_ok=False)
    models = [(seed, phase, trial / f"model-{seed}" / f"{phase}.pt")
              for seed in train_plan["model_seeds"] for phase in ("initial", "final")]
    artifacts = [panel / "plan.json", trial / "plan.json", trial / "producer.json", *files,
                 *(path for _, _, path in models)]
    evaluator_path = Path(__file__).resolve()
    dump(output / "producer.json", dict(
        revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
        evaluator_source=evaluator_path.read_text(encoding="utf-8"),
        evaluator_sha256=digest(evaluator_path),
        checked_training_sources={k:digest(ROOT / k) for k in TRAINING_SOURCES},
        inputs={str(p.resolve()):digest(p) for p in artifacts},
        torch=torch.__version__, numpy=np.__version__, device="cpu", threads=1,
        scope="frozen numeric policies; constructed development diagnostics; no parameter updates"))
    torch.set_num_threads(1)
    summary = []
    for seed, phase, path in models:
        model = EntityNetwork(train_plan["hidden"], train_plan["layers"]).eval()
        model.load_state_dict(torch.load(path, map_location="cpu", weights_only=True))
        rows = []
        with gzip.open(output / f"model-{seed}-{phase}.json.gz", "xt", encoding="utf-8") as evidence:
            for i, file in enumerate(files):
                case = json.loads(file.read_text(encoding="utf-8"))
                if case["recipe"] != panel_plan["recipes"][i]:
                    raise ValueError("panel recipe differs from plan")
                numeric = validate_case(case, panel_plan["schema"])
                probabilities = infer(model, numeric)
                result = integrate(case, probabilities)
                row = dict(case=file.name, recipe=case["recipe"], **result)
                rows.append(row)
                evidence.write(json.dumps(dict(**row, node_probabilities=probabilities), allow_nan=False) + "\n")
        summary.append(dict(seed=seed, phase=phase, cases=rows))
        print(f"mechanism evaluation | model {seed} {phase} | {len(rows)} cases", flush=True)
    # Recheck immutable model and panel inputs after inference.
    manifest = json.loads((output / "producer.json").read_text(encoding="utf-8"))
    if any(digest(Path(p)) != h for p, h in manifest["inputs"].items()):
        raise ValueError("input changed during evaluation")
    dump(output / "results.json", dict(schema=SCHEMA, models=summary, complete=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("panel")
    parser.add_argument("trial")
    parser.add_argument("output")
    args = parser.parse_args()
    run(args.panel, args.trial, args.output)
