"""Bounded imitation of attributed choices, assessed in fresh supplied combats."""
import argparse
from collections import Counter
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch

from tools.guided_search.entity_learning import SCHEMA, CaseSampling, EntityNetwork, collect, pack
from tools.guided_search.natural_demonstrations import load
from tools.guided_search.combat_quality import project_episode, report as quality_report, validate_requirements

ROOT = Path(__file__).resolve().parents[2]
# A matching schema number alone cannot certify historical feature meanings.
INPUT_SOURCES = (
    "crates/sts_guided_search/src/entities.rs",
    "crates/sts_guided_search/src/observation.rs",
    "crates/sts_guided_search/src/observation/monster.rs",
    "crates/sts_guided_search/src/selection.rs",
    "crates/sts_guided_search/src/combat.rs",
    "src/content/cards/mod.rs",
    "src/content/monsters/mod.rs",
    "src/content/potions/mod.rs",
    "src/content/powers/mod.rs",
    "src/content/relics/mod.rs",
)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dump(path, data):
    Path(path).write_text(json.dumps(data, indent=2, allow_nan=False), encoding="utf-8")


def read_material(sources):
    examples, used = [], set()
    for source in sources:
        directory = Path(source["directory"])
        for name in ("producer.json", "trajectory.jsonl", "verification.json"):
            if sha(directory / name) != source["sha256"][name]:
                raise ValueError("demonstration differs from declared material")
        manifest = Path(source["source_manifest"])
        if sha(manifest) != source["source_manifest_sha256"]:
            raise ValueError("demonstration source manifest changed")
        files = {f["path"]: f["sha256"] for f in json.loads(manifest.read_text(encoding="utf-8"))["files"]}
        if any(files.get(p) != sha(ROOT / p) for p in INPUT_SOURCES):
            raise ValueError("demonstration input sources require a separate compatibility review")
        kind = source.get("kind", "natural_assisted")
        producer = json.loads((directory / "producer.json").read_text(encoding="utf-8"))
        if kind == "natural_assisted":
            records = load(directory)
            seeds = set(producer["plan"]["seeds"])
        elif kind == "constructed_assisted":
            from tools.guided_search.constructed_demonstrations import load as load_constructed
            if source["source_manifest_sha256"] != producer["source_sha256"]:
                raise ValueError("constructed source manifest differs from its producer")
            records = load_constructed(directory)
            seeds = set(producer["seeds"])
        else:
            raise ValueError("unadmitted demonstration material kind")
        if used & seeds:
            raise ValueError("duplicate run seed across demonstration batches")
        used.update(seeds)
        examples.extend(records)
    if not examples:
        raise ValueError("empty imitation material")
    return examples, used


def validate(plan, source_seeds):
    validate_requirements(plan.get("quality_requirements", []))
    if (plan["schema"] != SCHEMA or plan["partition"] != "development"
            or plan["information"] != "oracle_observation_no_search_no_sl"
            or plan["sampling"] not in ("equal_run_seed_full_pass", "equal_seed_group_full_pass")
            or plan["evaluation_policy"] != "sampled_temperature_1"):
        raise ValueError("unadmitted imitation contract")
    if (any(s.get("kind") == "constructed_assisted" for s in plan.get("demonstrations", []))
            and plan["sampling"] != "equal_seed_group_full_pass"):
        raise ValueError("constructed material requires explicit seed-group weighting")
    if not plan["question"] or not plan["decision_rule"]:
        raise ValueError("declare question and decision rule")
    if plan.get("evaluation_rng", "shared_batch") not in ("shared_batch", "per_case"):
        raise ValueError("unadmitted evaluation RNG contract")
    cases = plan["evaluation"]
    if not cases or len({c["id"] for c in cases}) != len(cases):
        raise ValueError("empty or duplicate evaluation case IDs")
    if source_seeds & {c["spec"]["seed"] for c in cases}:
        raise ValueError("demonstration/evaluation seed groups overlap")
    if any(not c["source"] or not c["family"] for c in cases):
        raise ValueError("unqualified supplied-combat source")
    seeds = plan["model_seeds"]
    if not seeds or len(set(seeds)) != len(seeds) or any(type(s) is not int or s < 0 for s in seeds):
        raise ValueError("invalid model seeds")
    for key in ("max_seconds", "cohort_seconds", "learning_rate", "gradient_limit"):
        if not np.isfinite(plan[key]) or plan[key] <= 0:
            raise ValueError(f"invalid {key}")
    for key, low, high in (("epochs", 1, 1000), ("minibatch_size", 1, 128),
                           ("evaluation_batch_size", 1, 128), ("max_decisions", 1, 512),
                           ("layers", 1, 4), ("hidden", 4, 256)):
        if type(plan[key]) is not int or not low <= plan[key] <= high:
            raise ValueError(f"invalid {key}")
    if plan["hidden"] % 4:
        raise ValueError("hidden width must be divisible by four")
    for key in ("shuffle_seed", "evaluation_behavior_seed"):
        if type(plan[key]) is not int or plan[key] < 0:
            raise ValueError(f"invalid {key}")
    if plan["evaluation_behavior_seed"] + len(cases) > 2**64:
        raise ValueError("evaluation case seed overflow")


def seed_weights(examples):
    counts = Counter(e.seed for e in examples)
    return np.array([1 / (len(counts) * counts[e.seed]) for e in examples], dtype=np.float32)


def imitate(model, examples, *, minibatch_size, optimizer=None, gradient_limit=1., order=None,
            check_budget=lambda: None):
    """One full, equal-run-weighted pass; value head has no target or gradient."""
    if not examples or minibatch_size < 1 or not np.isfinite(gradient_limit) or gradient_limit <= 0:
        raise ValueError("invalid imitation pass")
    if any(type(e.selected) is not int or not 0 <= e.selected < len(e.position.columns["actions"])
           for e in examples):
        raise ValueError("imitation action outside its source")
    if order is None:
        order = np.arange(len(examples))
    if sorted(order) != list(range(len(examples))):
        raise ValueError("imitation pass must cover every example exactly once")
    weights = seed_weights(examples)
    totals = dict(loss=0., exact_action_accuracy=0., target_probability=0.)
    model.eval()  # Dropout is zero; deterministic packing comparisons remain meaningful.
    if optimizer is not None:
        optimizer.zero_grad(set_to_none=True)
    try:
        with torch.set_grad_enabled(optimizer is not None):
            for start in range(0, len(order), minibatch_size):
                check_budget()
                indices = order[start:start + minibatch_size]
                batch = [examples[int(i)] for i in indices]
                logits, _ = model(pack([e.position for e in batch]))
                target = torch.tensor([e.selected for e in batch])
                selected = logits.log_softmax(-1).gather(1, target[:, None]).squeeze(1)
                weight = torch.from_numpy(weights[indices])
                loss = -(selected * weight).sum()
                if not torch.isfinite(loss):
                    raise ValueError("nonfinite imitation loss")
                if optimizer is not None:
                    loss.backward()
                totals["loss"] += float(loss.detach())
                totals["target_probability"] += float((selected.detach().exp() * weight).sum())
                totals["exact_action_accuracy"] += float(((logits.argmax(-1) == target) * weight).sum())
        if optimizer is not None:
            check_budget()
            torch.nn.utils.clip_grad_norm_(model.parameters(), gradient_limit, error_if_nonfinite=True)
            optimizer.step()
    except BaseException:
        if optimizer is not None:
            optimizer.zero_grad(set_to_none=True)
        raise
    return totals


def summarize(rows):
    counts = Counter(r["kind"] if r["kind"] in ("win", "loss") else "unknown" for r in rows)
    wins = [r for r in rows if r["kind"] == "win"]
    def mean(items):
        return float(np.mean(items)) if items else None
    hp = [r["hp"] for r in wins]
    return dict(cases=len(rows), win=counts["win"], loss=counts["loss"], unknown=counts["unknown"],
        mean_winning_hp=mean(hp), p10_winning_hp=float(np.quantile(hp, .1)) if hp else None,
        mean_winning_net_hp_cost=mean([r["starting_hp"] - r["hp"] for r in wins]),
        mean_winning_potion_uses=mean([r["potion_uses"] for r in wins]),
        mean_winning_potions_remaining=mean([sum(p is not None for p in r["potions"]) for r in wins]))


def evaluate(model, cases, plan, directory, remaining):
    from sts_learning_bridge import GuidedCombatBatch
    requirements = plan.get("quality_requirements", [])
    validate_requirements(requirements)
    directory.mkdir()
    rows = [dict(id=c["id"], family=c["family"], encounter=c["spec"]["encounter"],
                 seed=c["spec"]["seed"], kind="unknown_not_attempted", initial_enemies=None,
                 quality_context={k:v for k,v in c["spec"].items() if k != "seed"}) for c in cases]
    complete, error = False, None
    started = time.monotonic()
    try:
        for start in range(0, len(cases), plan["evaluation_batch_size"]):
            current = cases[start:start + plan["evaluation_batch_size"]]
            remaining()
            before = time.monotonic()
            env = GuidedCombatBatch.from_specifications([c["spec"] for c in current], plan["max_decisions"])
            seconds = min(remaining(), plan["cohort_seconds"] - (time.monotonic() - before))
            if seconds <= 0:
                raise TimeoutError("evaluation construction exhausted cohort budget")
            batch_dir = directory / f"batch-{start:04}"
            try:
                behavior_seed = plan["evaluation_behavior_seed"] + start
                sampler = (CaseSampling(range(behavior_seed, behavior_seed + len(current)))
                           if plan.get("evaluation_rng", "shared_batch") == "per_case"
                           else torch.Generator().manual_seed(behavior_seed))
                collect(model, env, sampler,
                        batch_dir, max_seconds=seconds, require_complete=False)
            finally:
                # Preserve finished cases even when another member of this batch fails.
                for o in json.loads(env.outcomes_json()):
                    if o["kind"] == "win" and not o["win_replayed"]:
                        raise ValueError("evaluation victory lacks exact replay")
                    rows[start + o["slot"]].update(o)
                # Encounter IDs can name pools. Read actual initial enemies from
                # the evidence collect already saved, without another simulation.
                evidence_path = batch_dir / "evidence.json.gz"
                if evidence_path.exists():
                    with gzip.open(evidence_path, "rt", encoding="utf-8") as f:
                        episodes = json.load(f)["episodes"]
                    if len(episodes) != len(current):
                        raise ValueError("evaluation evidence has incomplete episode accounting")
                    for i, episode in enumerate(episodes):
                        if episode["seed"] != current[i]["spec"]["seed"]:
                            raise ValueError("evaluation evidence seed differs from its case")
                        rows[start + i]["initial_enemies"] = [
                            m["enemy_id"] for m in episode["root"]["combat"]["entities"]["monsters"]]
                        rows[start + i]["quality"] = project_episode(episode)
            completed = start + len(current)
            if completed == len(cases) or completed // 20 > start // 20:
                print(f"Evaluation {directory.name}: {completed}/{len(cases)}", flush=True)
        complete = True
    except BaseException as exc:
        error = f"{type(exc).__name__}: {exc}"
        raise
    finally:
        # A sorted multiset groups compositions, retaining duplicate monsters.
        compositions = {tuple(sorted(r["initial_enemies"])) for r in rows if r["initial_enemies"] is not None}
        report = dict(complete=complete, error=error,
            evaluation_rng=plan.get("evaluation_rng", "shared_batch"),
            evaluation_behavior_seed=plan["evaluation_behavior_seed"],
            all=summarize(rows), rows=rows,
            encounters={e:summarize([r for r in rows if r["encounter"] == e]) for e in sorted({r["encounter"] for r in rows})},
            families={e:summarize([r for r in rows if r["family"] == e]) for e in sorted({r["family"] for r in rows})},
            enemy_compositions=[dict(enemies=list(e), **summarize([
                r for r in rows if r["initial_enemies"] is not None and tuple(sorted(r["initial_enemies"])) == e]))
                for e in sorted(compositions)],
            unidentified_enemies=summarize([r for r in rows if r["initial_enemies"] is None]),
            wall_seconds=time.monotonic() - started)
        dump(directory / "summary.json", report)
        dump(directory / "quality.json", quality_report(rows, requirements))
    return report


def run(plan, output):
    examples, seeds = read_material(plan["demonstrations"])
    validate(plan, seeds)
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    dump(output / "plan.json", plan)
    import sts_learning_bridge._native as native
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT).decode("utf-8").strip()
    sources = (*INPUT_SOURCES, "crates/sts_guided_search/src/local.rs", "bindings/python_learning/src/guided.rs",
        "tools/guided_search/entity_learning.py", "tools/guided_search/natural_demonstrations.py",
        "tools/guided_search/natural_imitation.py", "tools/guided_search/constructed_demonstrations.py",
        "tools/guided_search/combat_quality.py")
    dump(output / "producer.json", dict(revision=git("rev-parse", "HEAD"), status=git("status", "--porcelain"),
        diff=git("diff", "HEAD"), sources={p:(ROOT / p).read_text(encoding="utf-8") for p in sources},
        native_sha256=sha(native.__file__), torch=torch.__version__, numpy=np.__version__, device="cpu", threads=1))
    dump(output / "material.json", dict(examples=len(examples), by_seed=dict(Counter(e.seed for e in examples)),
        labels="chosen_actions_only", weighting=plan["sampling"], all_source_seeds=sorted(seeds),
        source_kinds=[s.get("kind", "natural_assisted") for s in plan["demonstrations"]]))
    torch.set_num_threads(1)
    started = time.monotonic()
    def remaining():
        seconds = plan["max_seconds"] - (time.monotonic() - started)
        if seconds <= 0:
            raise TimeoutError("imitation study wall budget exhausted")
        return seconds
    result = dict(complete=False, models=[dict(seed=s, complete=False, epochs=[]) for s in plan["model_seeds"]])
    try:
        for entry in result["models"]:
            remaining()
            seed = entry["seed"]
            directory = output / f"model-{seed}"
            directory.mkdir()
            torch.manual_seed(seed)
            model = EntityNetwork(plan["hidden"], plan["layers"]).eval()
            optimizer = torch.optim.Adam(model.parameters(), lr=plan["learning_rate"])
            torch.save(model.state_dict(), directory / "initial.pt")
            args = dict(minibatch_size=plan["minibatch_size"], check_budget=remaining)
            entry["initial_fit"] = imitate(model, examples, **args)
            entry["initial"] = evaluate(model, plan["evaluation"], plan, directory / "initial", remaining)
            rng = np.random.default_rng(plan["shuffle_seed"])
            for epoch in range(plan["epochs"]):
                metrics = imitate(model, examples, optimizer=optimizer, gradient_limit=plan["gradient_limit"],
                                  order=rng.permutation(len(examples)), **args)
                entry["epochs"].append(metrics)
                if (epoch + 1) % 20 == 0:
                    print(f"Model {seed}: epoch {epoch + 1}/{plan['epochs']}, imitation loss {metrics['loss']:.3f}", flush=True)
            torch.save(model.state_dict(), directory / "final.pt")
            entry["final_fit"] = imitate(model, examples, **args)
            entry["final"] = evaluate(model, plan["evaluation"], plan, directory / "final", remaining)
            entry["complete"] = True
            print(f"Model {seed}: wins {entry['initial']['all']['win']} -> {entry['final']['all']['win']}/{len(plan['evaluation'])}", flush=True)
            dump(output / "summary.json", result)
        result["complete"] = True
    except BaseException as exc:
        result["error"] = f"{type(exc).__name__}: {exc}"
        raise
    finally:
        result["wall_seconds"] = time.monotonic() - started
        if "model" in locals():
            torch.save(dict(model=model.state_dict(), optimizer=optimizer.state_dict(), schema=SCHEMA,
                            complete_trial=result["complete"]), directory / "last-state.pt")
        dump(output / "summary.json", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    run(json.loads(args.plan.read_text(encoding="utf-8")), args.output)
