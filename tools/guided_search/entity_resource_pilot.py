"""Small fresh-source resource prediction pilot, outside the production beam.

Train on actual fixed-work continuation receipts. Freeze predictions before any
held-out continuation is run. Kept/omitted and source IDs never enter the model.
No action imitation, optimal-value claims, or invented loss labels.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch
from torch import nn

import entity_learning
import sts_learning_bridge
from entity_learning import EntityEncoder, SCHEMA, pack, positions
from sts_learning_bridge import combat_entity_batch


ROOT_WORK, SUFFIX_WORK, SELECT_COUNT = 6000, 4000, 2
WORK_FIELDS = ("search_transitions", "replay_transitions", "shuffle_work",
               "plan_edit_work", "order_family_work", "result_backup_work")


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def save(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)


def recipes():
    """Explicit constructed loadouts; not reconstructions of a natural run."""
    result = []
    deck = [("Strike", 0)] * 5 + [("Defend", 0)] * 3 + [
        ("Bash", 0), ("AscendersBane", 0), ("Armaments", 0),
        ("BattleTrance", 0), ("Cleave", 0), ("Rage", 0)]
    for split, count, offset in (("train", 12, 0), ("evaluation", 6, 1000)):
        for i in range(count):
            encounter = "Looter" if i % 3 == 2 else "SlimeBoss"
            spec = dict(seed=4100400 + offset + i, encounter=encounter, act=1,
                room="MonsterRoom" if encounter == "Looter" else "MonsterRoomBoss",
                hp=(45, 60)[(i // 3) % 2],
                deck=deck + [("SearingBlow", (1, 3)[(i // 3) % 2]
                              if encounter == "Looter" else (3, 5)[i % 2])],
                relics=["Vajra", "Sundial"], potions=["LiquidBronze"], max_potion_uses=0)
            result.append(dict(label=f"{split}-{i}-{encounter}", split=split, spec=spec))
    return result


def run_solver(exe, cases, folder, work, *, sample=False):
    input_path = folder.with_suffix(".json")
    save(input_path, cases)
    command = [str(exe), "--input", str(input_path), "--output", str(folder),
               "--work", str(work), "--seconds", "5"]
    if sample:
        command += ["--candidate-samples", "2"]
    started = time.perf_counter()
    with folder.with_suffix(".log").open("x", encoding="utf-8") as stream:
        subprocess.run(command, check=True, stdout=stream, stderr=subprocess.STDOUT)
    return time.perf_counter() - started


def collect_candidates(exe, out, sources):
    cases = [dict(label=s["label"], source="constructed", spec=s["spec"]) for s in sources]
    elapsed = run_solver(exe, cases, out / "sampling", ROOT_WORK, sample=True)
    rows = []
    for si, source in enumerate(sources):
        path = out / "sampling" / f"case-{si:03}-candidates.json"
        if not path.exists():
            continue
        document = load(path)
        for gi, group in enumerate(document["samples"]["groups"]):
            for ci, candidate in enumerate(group["candidates"]):
                if candidate["potion_uses"] or candidate["expenditures"]["explicit_count"]:
                    raise ValueError("preserved-potion candidate spent a potion")
                rows.append(dict(source=si, split=source["split"], group=gi, candidate=ci,
                    encounter=source["spec"]["encounter"], pool=group["pool"],
                    kept=candidate["kept"], side_index=candidate["side_index"],
                    position=candidate["position"], prefix=candidate["inputs"],
                    source_file=str(path.relative_to(out))))
    return rows, elapsed


def gold_available(outcome):
    """Wallet plus pending stolen gold; ordinary postcombat rewards excluded."""
    return outcome["final_gold"] + sum(
        reward["StolenGold"]["amount"] for reward in outcome["pending_rewards"]
        if "StolenGold" in reward)


def outcome_key(outcome, encounter):
    # Fixed resource objective for this pilot, with no HP-to-gold exchange rate.
    if encounter == "Looter":
        return (gold_available(outcome), outcome["final_hp"])
    return (outcome["final_hp"], gold_available(outcome))


def continue_rows(exe, out, rows, name):
    cases = []
    for row in rows:
        source = load(out / row["source_file"])
        cases.append(dict(label=f'{row["source"]}-{row["group"]}-{row["candidate"]}',
            source="snapshot", producer=f'fresh candidate from {row["source_file"]}',
            position=source["root"], prefix=row["prefix"], max_potion_uses=0))
    elapsed = run_solver(exe, cases, out / name, SUFFIX_WORK)
    for i, row in enumerate(rows):
        path = out / name / f"case-{i:03}.json"
        result = load(path)
        if result["root"] != row["position"]:
            raise ValueError("source-relative replay differs from sampled position")
        milestone = result["milestones"][-1]
        if milestone["stop"] == "TimeBudget":
            raise ValueError("time censoring prevents a fixed-work comparison")
        if result["root_entity_admission"]["rejected"]:
            raise ValueError("candidate was not admitted by the entity owner")
        best = max((p["outcome"] for p in result["plans"]),
                   key=lambda o: outcome_key(o, row["encounter"]), default=None)
        if best is not None and best["total_potion_expenditures"]:
            raise ValueError("continuation did not preserve potions")
        row.update(outcome=best, stop=milestone["stop"],
            work=sum(milestone["stats"][key] for key in WORK_FIELDS),
            search_ms=milestone["elapsed_ms"], receipt=str(path.relative_to(out)))
    return elapsed


def encode_rows(rows):
    started = time.perf_counter()
    items = []
    for start in range(0, len(rows), 64):
        frame = combat_entity_batch(json.dumps([r["position"] for r in rows[start:start + 64]]),
                                    max_potion_uses=0)
        items.extend(positions(frame))
    return items, time.perf_counter() - started


class ResourceNetwork(EntityEncoder):
    def __init__(self):
        super().__init__(hidden=32, layers=1)
        self.resources = nn.Linear(32, 4)

    def forward(self, batch):
        return self.resources(self.encode_context(batch)[:, 0])


def targets(rows):
    # Head 0 predicts this solver's budgeted completion, NOT probability of death.
    values = [[float(r["outcome"] is not None),
        (r["outcome"]["final_hp"] / 75 if r["outcome"] else 0),
        (gold_available(r["outcome"]) / 200 if r["outcome"] else 0),
        float(not r["outcome"]["combat_mugged"]) if r["outcome"] else 0] for r in rows]
    return torch.tensor(values, dtype=torch.float32)


def resource_loss(prediction, truth):
    loss = nn.functional.binary_cross_entropy_with_logits(prediction[:, 0], truth[:, 0])
    complete = truth[:, 0].bool()
    if complete.any():
        loss = loss + nn.functional.mse_loss(prediction[complete, 1:3].sigmoid(), truth[complete, 1:3])
        loss = loss + nn.functional.binary_cross_entropy_with_logits(prediction[complete, 3], truth[complete, 3])
    return loss


def fit(rows, items):
    started = time.perf_counter()
    torch.manual_seed(1704)
    model = ResourceNetwork()
    optimizer = torch.optim.AdamW(model.parameters(), lr=0.002)
    y = targets(rows)
    sources = sorted({r["source"] for r in rows})
    cohorts = [[i for i, r in enumerate(rows) if r["source"] == s] for s in sources]
    rng = np.random.default_rng(1704)
    losses = []
    model.train()
    for step in range(240):
        indices = [int(rng.choice(cohorts[int(rng.integers(len(cohorts)))])) for _ in range(32)]
        batch = pack([items[i] for i in indices])
        truth = y[indices]
        prediction = model(batch)
        loss = resource_loss(prediction, truth)
        optimizer.zero_grad()
        loss.backward()
        nn.utils.clip_grad_norm_(model.parameters(), 1.)
        optimizer.step()
        if step % 40 == 0:
            losses.append(dict(step=step, loss=float(loss.detach())))
    model.eval()
    return model, dict(seconds=time.perf_counter() - started, steps=240, losses=losses,
                       parameters=sum(p.numel() for p in model.parameters()))


def predict(model, items):
    started = time.perf_counter()
    with torch.no_grad():
        result = torch.cat([model(pack(items[start:start + 32])).sigmoid()
                            for start in range(0, len(items), 32)]).numpy()
    return result, time.perf_counter() - started


def selections(rows, predictions):
    result = []
    for source, group in sorted({(r["source"], r["group"]) for r in rows}):
        ids = [i for i, r in enumerate(rows) if (r["source"], r["group"]) == (source, group)]
        resource_column = 2 if rows[ids[0]]["encounter"] == "Looter" else 1
        expected = lambda i: float(predictions[i, 0] * predictions[i, resource_column])
        kept_ordered = sorted((i for i in ids if rows[i]["kept"]), key=lambda i: rows[i]["side_index"])
        count = min(SELECT_COUNT, len(kept_ordered))
        if not count:
            raise ValueError("sampled group lacks a kept-side control")
        model = sorted(ids, key=lambda i: (-expected(i), i))[:count]
        kept = kept_ordered[:count]
        # HP-only control exposes whether the learned model adds to an easy proxy.
        hp = sorted(ids, key=lambda i: (-rows[i]["position"]["combat"]["entities"]["player"]["current_hp"], i))[:count]
        result.append(dict(source=source, group=group, pool=rows[ids[0]]["pool"],
            encounter=rows[ids[0]]["encounter"], model=model, kept=kept, current_hp=hp,
            scores={str(i):expected(i) for i in ids}))
    return result


def assess(rows, selected):
    result = []
    for selection in selected:
        record = dict(selection)
        for method in ("model", "kept", "current_hp"):
            ids = selection[method]
            best = max((rows[i]["outcome"] for i in ids if rows[i]["outcome"] is not None),
                key=lambda o: outcome_key(o, selection["encounter"]), default=None)
            record[method + "_result"] = dict(
                completed=best is not None, hp=best["final_hp"] if best else None,
                gold=gold_available(best) if best else None,
                mugged=best["combat_mugged"] if best else None,
                work=sum(rows[i]["work"] for i in ids),
                search_ms=sum(rows[i]["search_ms"] for i in ids))
        result.append(record)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--solver", type=Path, default=Path("target/dev-opt/examples/combat_solver.exe"))
    args = parser.parse_args()
    out, exe = args.output.resolve(), args.solver.resolve()
    out.mkdir(parents=True, exist_ok=False)
    with (out / "producer.py").open("xb") as stream:
        stream.write(Path(__file__).read_bytes())
    torch.set_num_threads(2)
    source_recipes = recipes()
    train_seeds = {s["spec"]["seed"] for s in source_recipes if s["split"] == "train"}
    eval_seeds = {s["spec"]["seed"] for s in source_recipes if s["split"] == "evaluation"}
    if train_seeds & eval_seeds:
        raise ValueError("source leakage")
    save(out / "manifest.json", dict(schema=1, entity_schema=SCHEMA,
        objective="finite_solver_completion_and_conditional_resources_v1",
        information="full_state_oracle", sources=source_recipes,
        budgets=dict(root_work=ROOT_WORK, suffix_work=SUFFIX_WORK, selected=SELECT_COUNT),
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        encoder_sha256=hashlib.sha256(Path(entity_learning.__file__).read_bytes()).hexdigest(),
        bridge_binaries={p.name:hashlib.sha256(p.read_bytes()).hexdigest()
            for p in Path(sts_learning_bridge.__file__).parent.glob("*.pyd")},
        runtime=dict(torch=torch.__version__, numpy=np.__version__),
        solver_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),
        git_head=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        limits="Constructed pilot; fresh relic counters; potions preserved. No optimality, NoSL, production beam or whole-run claim. Gold excludes ordinary postcombat rewards."))
    rows, sampling_seconds = collect_candidates(exe, out, source_recipes)
    train = [r for r in rows if r["split"] == "train"]
    evaluation = [r for r in rows if r["split"] == "evaluation"]
    print(f"fresh candidates: train={len(train)}, held-out={len(evaluation)}", flush=True)
    training_collection_seconds = continue_rows(exe, out, train, "training-continuations")
    save(out / "training-rows.json", train)
    print(f'training completed continuations: {sum(r["outcome"] is not None for r in train)}/{len(train)}', flush=True)
    train_items, train_encoding_seconds = encode_rows(train)
    model, training = fit(train, train_items)
    torch.save(dict(schema=SCHEMA, objective="finite_solver_completion_and_conditional_resources_v1",
                    state_dict=model.state_dict()), out / "model.pt")
    eval_items, encoding_seconds = encode_rows(evaluation)
    predictions, inference_seconds = predict(model, eval_items)
    chosen = selections(evaluation, predictions)
    # Commit prediction and candidate selections to disk BEFORE any eval outcomes.
    save(out / "frozen-selections.json", dict(predictions=predictions.tolist(), groups=chosen,
         encoding_seconds=encoding_seconds, inference_seconds=inference_seconds))
    print(f"model frozen; held-out groups selected: {len(chosen)}", flush=True)
    evaluation_collection_seconds = continue_rows(exe, out, evaluation, "evaluation-continuations")
    save(out / "evaluation-rows.json", evaluation)
    result = dict(training=training, groups=assess(evaluation, chosen),
        counts=dict(train_sources=len({r["source"] for r in train}),
                    evaluation_sources=len({r["source"] for r in evaluation}),
                    train_rows=len(train), evaluation_rows=len(evaluation),
                    train_completed=sum(r["outcome"] is not None for r in train),
                    evaluation_completed=sum(r["outcome"] is not None for r in evaluation)),
        costs=dict(sampling_seconds=sampling_seconds, training_collection_seconds=training_collection_seconds,
            train_encoding_seconds=train_encoding_seconds, encoding_seconds=encoding_seconds,
            inference_seconds=inference_seconds, evaluation_collection_seconds=evaluation_collection_seconds),
        limits="Held-out pool allocation comparison, not production beam integration. All suffixes run offline to audit choices; each method only credited its two frozen selections. Encoding/inference charged separately, not hidden in search work. Missing completed plan is not a death label.")
    save(out / "results.json", result)
    print(json.dumps(dict(counts=result["counts"], training_seconds=training["seconds"],
        costs=result["costs"], groups=[{k:g[k] for k in (
            "source", "group", "encounter", "model_result", "kept_result", "current_hp_result")}
            for g in result["groups"]]), indent=2))


if __name__ == "__main__":
    main()
