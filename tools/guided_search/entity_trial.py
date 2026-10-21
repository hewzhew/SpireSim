"""Explicit bounded complete-battle study. No search labels or gameplay control."""
import argparse
import ast
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch
import sts_learning_bridge._native as native
from sts_learning_bridge import GuidedCombatBatch

from tools.guided_search.entity_learning import SCHEMA, CaseSampling, EntityNetwork, bounded_update, collect, update
from tools.guided_search.natural_imitation import INPUT_SOURCES, evaluate as evaluate_cases, sha
from tools.guided_search.combat_quality import validate_requirements

ROOT = Path(__file__).resolve().parents[2]


def dump(path, data):
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


def validate(plan):
    """Validate source groups before constructing a game or initializing weights."""
    validate_requirements(plan.get("quality_requirements", []))
    if plan["schema"] != SCHEMA:
        raise ValueError("trial schema mismatch")
    if plan["partition"] != "development":
        raise ValueError("this diagnostic trial uses a development partition")
    if not plan["question"] or not plan["decision_rule"]:
        raise ValueError("declare the question and decision rule")
    train, evaluation = plan["training"], plan["evaluation"]
    if not train or not evaluation:
        raise ValueError("empty trial population")
    if any(not g or len(g) > 128 for g in train):
        raise ValueError("invalid training cohort")
    cases = [c for cohort in train for c in cohort]
    if {c["spec"]["seed"] for c in cases} & {c["spec"]["seed"] for c in evaluation}:
        raise ValueError("training/evaluation seed groups overlap")
    fingerprints = {recipe(c) for c in cases}
    for c in evaluation:
        if c["group"] not in ("new_seed", "new_recipe", "stress"):
            raise ValueError("unknown evaluation group")
        if c["group"] == "new_recipe" and recipe(c) in fingerprints:
            raise ValueError("new recipe repeats a training loadout")
        if c["group"] == "new_seed" and recipe(c) not in fingerprints:
            raise ValueError("new-seed group changed its loadout")
    for c in cases + evaluation:
        if not c["family"] or not c["source"]:
            raise ValueError("unqualified constructed source")
    seeds = plan["model_seeds"]
    if not seeds or len(set(seeds)) != len(seeds) or min(seeds) < 0:
        raise ValueError("invalid model seeds")
    for k in ("max_seconds", "cohort_seconds", "learning_rate", "gradient_limit"):
        if not np.isfinite(plan[k]) or plan[k] <= 0:
            raise ValueError(f"invalid {k}")
    if plan["hidden"] < 4 or plan["hidden"] % 4 or not 1 <= plan["layers"] <= 4:
        raise ValueError("invalid network dimensions")
    if not 1 <= plan["max_decisions"] <= 512 or not 1 <= plan["minibatch_size"] <= 128:
        raise ValueError("invalid trial batch/depth")
    for k in ("value_weight", "entropy_weight"):
        if not np.isfinite(plan[k]) or plan[k] < 0:
            raise ValueError(f"invalid {k}")
    if plan.get("sampling_rng", "shared_batch") not in ("shared_batch", "per_case"):
        raise ValueError("invalid trial sampling RNG")
    if not 1 <= plan.get("evaluation_batch_size", 16) <= 128:
        raise ValueError("invalid evaluation batch size")
    if "max_policy_kl" in plan and (not np.isfinite(plan["max_policy_kl"]) or plan["max_policy_kl"] <= 0):
        raise ValueError("invalid policy change budget")
    if type(plan.get("max_backtracks", 6)) is not int or not 0 <= plan.get("max_backtracks", 6) <= 12:
        raise ValueError("invalid backtracking budget")


def numerical_contract(source):
    # Freeze the complete numerical/model prefix, including constants and helper
    # dependencies. Later collector/update changes do not change frozen features.
    nodes = []
    for node in ast.parse(source).body:
        if isinstance(node, ast.ClassDef) and node.name == "Experience":
            return ast.dump(ast.Module(body=nodes, type_ignores=[]), include_attributes=False)
        nodes.append(node)
    raise ValueError("missing numerical model contract")


def warm_start_states(plan):
    """Admit pinned imitation endpoints only; never import a saved game state."""
    source = plan.get("warm_start")
    if source is None:
        return None
    if source["kind"] != "attributed_imitation" or source["value_initialization"] != "zero_logit":
        raise ValueError("unadmitted warm-start contract")
    directory = Path(source["directory"])
    required = {"plan.json", "producer.json", "summary.json", "material.json"}
    required.update(f"model-{seed}/final.pt" for seed in plan["model_seeds"])
    if not required <= source["sha256"].keys():
        raise ValueError("warm-start source lacks pinned evidence")
    for name in required:
        if sha(directory / name) != source["sha256"][name]:
            raise ValueError(f"warm-start evidence changed: {name}")
    def read(name):
        return json.loads((directory / name).read_text(encoding="utf-8"))
    original, producer, report, material = (read(n) for n in
        ("plan.json", "producer.json", "summary.json", "material.json"))
    if (not report["complete"] or material["labels"] != "chosen_actions_only"
            or original["schema"] != SCHEMA
            or any(original[k] != plan[k] for k in ("hidden", "layers", "model_seeds"))
            or {m["seed"] for m in report["models"] if m["complete"]} != set(plan["model_seeds"])):
        raise ValueError("warm-start endpoints lack complete matching provenance")
    sources = (*INPUT_SOURCES, "crates/sts_guided_search/src/local.rs", "bindings/python_learning/src/guided.rs")
    if any(producer["sources"].get(p) != (ROOT / p).read_text(encoding="utf-8") for p in sources):
        raise ValueError("warm-start input sources require compatibility review")
    model_source = "tools/guided_search/entity_learning.py"
    if numerical_contract(producer["sources"][model_source]) != numerical_contract((ROOT / model_source).read_text(encoding="utf-8")):
        raise ValueError("warm-start numerical network contract changed")
    if producer["native_sha256"] != sha(native.__file__) or producer["torch"] != torch.__version__:
        raise ValueError("warm-start runtime requires compatibility review")
    used = set(material["all_source_seeds"]) | {c["spec"]["seed"] for c in original["evaluation"]}
    if used & {c["spec"]["seed"] for c in [*plan["evaluation"], *(c for g in plan["training"] for c in g)]}:
        raise ValueError("warm-start source/development seeds overlap new trial")
    states = {}
    for seed in plan["model_seeds"]:
        state = torch.load(directory / f"model-{seed}/final.pt", map_location="cpu", weights_only=True)
        if not all(torch.isfinite(t).all() for t in state.values()):
            raise ValueError("nonfinite warm-start weights")
        states[seed] = state
    return states


def recipe(case):
    # Seed, encounter and current HP are interventions, not distinct loadouts.
    # Rearranging equal cards or slots must not manufacture a held-out recipe.
    return json.dumps({k: sorted(case["spec"][k]) for k in ("deck", "relics", "potions")}, sort_keys=True)


def summarize(rows):
    counts = Counter(r["kind"] if r["kind"] in ("win", "loss") else "unknown" for r in rows)
    wins = [r for r in rows if r["kind"] == "win"]
    return {"episodes":len(rows), "win":counts["win"], "loss":counts["loss"],
            "unknown":counts["unknown"],
            "mean_winning_hp":float(np.mean([r["hp"] for r in wins])) if wins else None,
            "decisions":sum(r["decisions"] for r in rows),
            "engine_steps":sum(r["engine_steps"] for r in rows),
            "replay_engine_steps":sum(r["replay_engine_steps"] for r in rows)}


def run(plan, output):
    validate(plan)
    warm = warm_start_states(plan)
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    deadline = started + plan["max_seconds"]
    dump(output / "plan.json", plan)
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT).decode("utf-8").strip()
    sources = [*INPUT_SOURCES, "crates/sts_guided_search/src/local.rs",
        "bindings/python_learning/src/guided.rs", "tools/guided_search/entity_learning.py",
        "tools/guided_search/entity_trial.py", "tools/guided_search/natural_imitation.py",
        "tools/guided_search/combat_quality.py"]
    dump(output / "producer.json", {"revision":git("rev-parse", "HEAD"),
        "status":git("status", "--porcelain"), "diff":git("diff", "HEAD"),
        "sources":{p:(ROOT / p).read_text(encoding="utf-8") for p in sources},
        "native_sha256":hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest(),
        "torch":torch.__version__, "numpy":np.__version__, "device":"cpu", "threads":1})
    torch.set_num_threads(1)
    results = {"models":[dict(seed=s, complete=False, updates=[]) for s in plan["model_seeds"]],
               "complete":False, "schema":SCHEMA}

    def remaining():
        seconds = min(plan["cohort_seconds"], deadline - time.monotonic())
        if seconds <= 0:
            raise TimeoutError("trial wall budget exhausted")
        return seconds

    def sample(model, cases, directory, seed, training):
        seconds = remaining()
        before = time.monotonic()
        env = GuidedCombatBatch.from_specifications([c["spec"] for c in cases], plan["max_decisions"])
        # Construction time also consumes the cooperative wall budget.
        seconds = min(seconds - (time.monotonic() - before), remaining())
        if seconds <= 0:
            raise TimeoutError("trial budget exhausted during construction")
        sampler = (CaseSampling(range(seed, seed + len(cases))) if plan.get("sampling_rng") == "per_case"
                   else torch.Generator().manual_seed(seed))
        dump(directory.with_suffix(".cases.json"), dict(cases=cases, behavior_seed=seed,
             sampling_rng=plan.get("sampling_rng", "shared_batch")))
        cohort = collect(model, env, sampler, directory,
                         max_seconds=seconds, require_complete=training)
        rows = [dict(o, family=cases[o["slot"]]["family"],
                     encounter=cases[o["slot"]]["spec"]["encounter"],
                     group=cases[o["slot"]].get("group", "training")) for o in cohort.outcomes]
        dump(directory / "cases.json", cases)
        return cohort, rows, time.monotonic() - before

    def evaluate(model, directory):
        evaluation_plan = dict(plan, evaluation_batch_size=plan.get("evaluation_batch_size", 16),
                               evaluation_rng=plan.get("sampling_rng", "shared_batch"))
        cases = [dict(c, id=c.get("id", f"case-{i}")) for i, c in enumerate(plan["evaluation"])]
        report = evaluate_cases(model, cases, evaluation_plan, directory, remaining)
        for row, case in zip(report["rows"], cases):
            row["group"] = case["group"]
        report["groups"] = {k:summarize([r for r in report["rows"] if r["group"] == k])
                            for k in sorted({c["group"] for c in cases})}
        dump(directory / "summary.json", report)
        return report

    try:
        for result in results["models"]:
            seed = result["seed"]
            remaining()
            directory = output / f"model-{seed}"
            directory.mkdir()
            torch.manual_seed(seed)
            model = EntityNetwork(plan["hidden"], plan["layers"]).eval()
            if warm is not None:
                model.load_state_dict(warm[seed], strict=True)
                # Imitation trained the policy only. Start the outcome estimator
                # explicitly at 0.5, not at an inherited random prediction.
                with torch.no_grad():
                    model.value.weight.zero_()
                    model.value.bias.zero_()
            optimizer = torch.optim.Adam(model.parameters(), lr=plan["learning_rate"])
            torch.save(model.state_dict(), directory / "initial.pt")
            result["initial"] = evaluate(model, directory / "initial")
            for generation, cases in enumerate(plan["training"]):
                stride = 128 if plan.get("sampling_rng") == "per_case" else 1
                cohort, rows, seconds = sample(model, cases, directory / f"train-{generation:04}",
                    plan["training_behavior_seed"] + seed * 100_000 + generation * stride, True)
                remaining()
                before = time.monotonic()
                update_fn = bounded_update if "max_policy_kl" in plan else update
                guard = dict(max_policy_kl=plan["max_policy_kl"], max_backtracks=plan.get("max_backtracks", 6)) if "max_policy_kl" in plan else {}
                metrics = update_fn(model, optimizer, cohort, minibatch_size=plan["minibatch_size"],
                    value_weight=plan["value_weight"], entropy_weight=plan["entropy_weight"],
                    gradient_limit=plan["gradient_limit"], **guard)
                result["updates"].append(dict(metrics, collection_seconds=seconds,
                    update_seconds=time.monotonic() - before, outcomes=summarize(rows)))
                dump(output / "summary.json", results)
                if not metrics.get("accepted", True):
                    result["stop"] = "policy_change_budget"
                    break
                if (generation + 1) % 4 == 0:
                    print(json.dumps({"model":seed, "updates":generation + 1,
                        "last_cohort":summarize(rows), "seconds":round(time.monotonic() - started, 1)}), flush=True)
            torch.save(model.state_dict(), directory / "final.pt")
            result["final"] = evaluate(model, directory / "final")
            result["complete"] = len(result["updates"]) == len(plan["training"]) and "stop" not in result
            torch.save({"model":model.state_dict(), "optimizer":optimizer.state_dict(),
                        "schema":SCHEMA, "complete_model":result["complete"]}, directory / "last-state.pt")
            print(json.dumps({"model":seed, "initial":result["initial"]["all"],
                              "final":result["final"]["all"]}), flush=True)
        results["complete"] = all(r["complete"] for r in results["models"])
    except BaseException as e:
        results["error"] = f"{type(e).__name__}: {e}"
        raise
    finally:
        results["wall_seconds"] = time.monotonic() - started
        if "model" in locals():
            torch.save({"model":model.state_dict(), "optimizer":optimizer.state_dict(),
                        "schema":SCHEMA, "complete_trial":results["complete"]},
                       directory / "last-state.pt")
        dump(output / "summary.json", results)
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    run(json.loads(args.plan.read_text(encoding="utf-8")), args.output)
