"""Run small search -> executed continuations -> fit -> guided-search iterations.

Each round learns one frozen executor's actual results on its own corpus. Older
teacher contracts are never relabeled or silently mixed. Complete-combat tests
use separate seeds and include native model inference in their search deadline.
Build continuation_value and combat_solver in release mode before invoking this.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

from continuation_learning import Dataset, validate_split


def read(path):
    return json.loads(path.read_bytes())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def validate_cases(train, validation, combat):
    seeds = []
    for cases in (train, validation, combat):
        if not cases or len({c["label"] for c in cases}) != len(cases):
            raise ValueError("nonempty cases with unique labels required")
        seeds.append({c["spec"]["seed"] for c in cases})
    if any(seeds[i] & seeds[j] for i in range(3) for j in range(i)):
        raise ValueError("training, validation and combat test seeds must be disjoint")
    if {c["group"] for c in train} & {c["group"] for c in validation}:
        raise ValueError("training and validation source groups overlap")


def collection(directory, expected_model):
    docs = [read(p) for p in sorted(directory.glob("case-*.json"))]
    data = Dataset(docs)
    identity = data.continuation["solver_config"]["resource_model"]
    if (identity is None) != (expected_model is None):
        raise ValueError("collector did not use the declared teacher")
    if expected_model is not None and identity["sha256"] != sha(expected_model):
        raise ValueError("collector teacher weights changed")
    evaluations = sum(t["search"]["stats"]["resource_evaluations"]
                      for d in docs for t in d["traces"])
    if expected_model is not None and evaluations == 0:
        raise ValueError("the guided collector never evaluated its teacher model")
    roots = [dict(label=d["case"]["label"],
                  actual_return=d["traces"][0]["terminal_return"],
                  selected_solver_plan=d["traces"][0]["search"]["selected_solver_plan"])
             for d in docs]
    return docs, data, dict(rows=len(data.y), censored=data.censored,
                           cases=len(docs), model_evaluations=evaluations,
                           elapsed_ms=sum(d["elapsed_ms"] for d in docs), roots=roots)


def changed_sources(old_docs, new_docs):
    """Count changed replay prefixes, not independently verified new exact states."""
    old = {d["case"]["label"]:d for d in old_docs}
    new_prefixes = changed_roots = total = 0
    for d in new_docs:
        previous = old[d["case"]["label"]]
        assert d["root"] == previous["root"]
        prefixes = {json.dumps(t["prefix"], sort_keys=True) for t in previous["traces"]}
        new_prefixes += sum(json.dumps(t["prefix"], sort_keys=True) not in prefixes
                            for t in d["traces"])
        changed_roots += d["traces"][0]["inputs"] != previous["traces"][0]["inputs"]
        total += len(d["traces"])
    return dict(changed_root_trajectories=changed_roots,
                new_replay_prefixes=new_prefixes, total_new_traces=total)


def combat_results(directory):
    rows = []
    for p in sorted(directory.glob("case-*.json")):
        d = read(p)
        milestone = d["milestones"][-1]
        if milestone["stop"] in ("Error", "Cancelled"):
            raise ValueError("combat search did not finish normally")
        rows.append(dict(label=d["case"]["label"],
                         hp=max((plan["outcome"]["final_hp"] for plan in d["plans"]), default=None),
                         elapsed_ms=milestone["elapsed_ms"],
                         work=milestone["stats"], stop=milestone["stop"]))
    return dict(rows=rows, verified_wins=sum(r["hp"] is not None for r in rows),
                win_hp_total=sum(r["hp"] or 0 for r in rows),
                elapsed_ms=sum(r["elapsed_ms"] for r in rows))


def compare(base, candidate):
    if [r["label"] for r in base["rows"]] != [r["label"] for r in candidate["rows"]]:
        raise ValueError("complete-combat cases differ")
    pairs = [(b, c) for b, c in zip(base["rows"], candidate["rows"])
             if b["hp"] is not None and c["hp"] is not None]
    return dict(common_wins=len(pairs),
                hp_delta_on_common_wins=sum(c["hp"] - b["hp"] for b, c in pairs),
                better=sum(c["hp"] > b["hp"] for b, c in pairs),
                worse=sum(c["hp"] < b["hp"] for b, c in pairs),
                equal=sum(c["hp"] == b["hp"] for b, c in pairs),
                gained_witnesses=sum(b["hp"] is None and c["hp"] is not None
                                     for b, c in zip(base["rows"], candidate["rows"])),
                lost_witnesses=sum(b["hp"] is not None and c["hp"] is None
                                   for b, c in zip(base["rows"], candidate["rows"])))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("train", "validation", "combat", "output", "collector", "solver"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--bootstrap-model", type=Path)
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("--solver-work", type=int, default=10000)
    parser.add_argument("--seconds", type=float, default=.6)
    parser.add_argument("--steps", type=int, default=1200)
    args = parser.parse_args()
    if args.output.exists() or min(args.rounds, args.solver_work, args.steps, args.seconds) <= 0:
        parser.error("fresh output and positive limits required")
    validate_cases(*[read(p) for p in (args.train, args.validation, args.combat)])
    trainer = Path(__file__).with_name("resource_curve_learning.py")
    frozen = {str(p.resolve()):sha(p) for p in
              (args.train, args.validation, args.combat, args.collector, args.solver, trainer,
               Path(__file__), Path(__file__).with_name("continuation_learning.py"),
               Path(__file__).with_name("retention_learning.py"))}
    if args.bootstrap_model is not None:
        frozen[str(args.bootstrap_model.resolve())] = sha(args.bootstrap_model)
    args.output.mkdir(parents=True)
    receipt = dict(status="running", information="full_state_oracle",
                   solver_work=args.solver_work, combat_seconds=args.seconds,
                   steps=args.steps, initialization_seed=1, frozen_inputs=frozen,
                   rounds=[], commands=[], combat={})
    destination = args.output / "iteration.json"
    started = time.perf_counter()

    def run(label, command):
        if any(sha(Path(p)) != digest for p, digest in frozen.items()):
            raise ValueError("an input, executable or teacher changed during the iteration")
        print(label, flush=True)
        tick = time.perf_counter()
        with (args.output / (label + ".log")).open("w", encoding="utf-8") as log:
            result = subprocess.run([str(v) for v in command], stdout=log, stderr=subprocess.STDOUT)
        receipt["commands"].append(dict(label=label, command=[str(v) for v in command],
                                       seconds=time.perf_counter() - tick, exit_code=result.returncode))
        write(destination, receipt)
        result.check_returncode()

    teacher, models, previous = args.bootstrap_model, [], None
    try:
        for index in range(1, args.rounds + 1):
            folder = args.output / f"round-{index}"
            folder.mkdir()
            for name, source in (("train", args.train), ("validation", args.validation)):
                command = [args.collector, "--input", source, "--output", folder/name,
                           "--solver-work", args.solver_work]
                if teacher is not None:
                    command += ["--resource-model", teacher]
                run(f"round-{index}-{name}", command)
            docs, train, training = collection(folder/"train", teacher)
            _, validation, validating = collection(folder/"validation", teacher)
            validate_split(train, validation)
            record = dict(index=index, teacher_model=str(teacher) if teacher else None,
                          teacher_sha256=sha(teacher) if teacher else None,
                          continuation=train.continuation, train=training, validation=validating)
            if previous is not None:
                record["source_changes"] = changed_sources(previous, docs)
            receipt["rounds"].append(record)
            write(destination, receipt)
            run(f"round-{index}-fit", [sys.executable, trainer,
                "--train", *sorted((folder/"train").glob("case-*.json")),
                "--eval", *sorted((folder/"validation").glob("case-*.json")),
                "--output", folder/"model", "--steps", args.steps, "--seed", 1])
            teacher = folder/"model/model.json"
            frozen[str(teacher.resolve())] = sha(teacher)
            record["student_sha256"] = sha(teacher)
            record["fit"] = read(folder/"model/fit.json")
            models.append((f"round-{index}", teacher))
            previous = docs
            write(destination, receipt)
        # No model selection uses combat-test results. Compare every frozen round.
        variants = [("baseline", None)]
        if args.bootstrap_model is not None:
            variants.append(("bootstrap", args.bootstrap_model))
        variants.extend(models)
        for name, model in variants:
            target = args.output / ("combat-" + name)
            command = [args.solver, "--input", args.combat, "--output", target,
                       "--work", 10000000, "--seconds", args.seconds]
            if model is not None:
                command += ["--resource-model", model]
            run("combat-" + name, command)
            receipt["combat"][name] = combat_results(target)
            write(destination, receipt)
        receipt["versus_baseline"] = {n:compare(receipt["combat"]["baseline"], v)
                                       for n, v in receipt["combat"].items() if n != "baseline"}
        receipt["status"] = "complete"
    except Exception as error:
        receipt["status"], receipt["error"] = "failed", str(error)
        raise
    finally:
        receipt["elapsed_seconds"] = time.perf_counter() - started
        write(destination, receipt)
    print(json.dumps({n:{k:v[k] for k in ("verified_wins", "win_hp_total", "elapsed_ms")}
                      for n,v in receipt["combat"].items()}, indent=2), flush=True)


if __name__ == "__main__":
    main()
