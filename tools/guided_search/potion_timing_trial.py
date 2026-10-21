"""Fresh, bounded Block Potion experiment. Card search never uses the model.

Input consists of snapshot triples named train/SEED/[NODE/]{never,now,flexible}
or evaluation/SEED/[NODE/]{never,now,flexible}, with disjoint integer seeds. Each triple
declares the same source, the now prefix and explicit resource constraints.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch

if __package__:
    from .entity_learning import SCHEMA
    from .potion_timing import HP_SCALE, PotionTimingValue, encode_trial, fit, predict, observed_outcomes
else:
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from tools.guided_search.entity_learning import SCHEMA
    from tools.guided_search.potion_timing import HP_SCALE, PotionTimingValue, encode_trial, fit, predict, observed_outcomes


def declared_groups(cases):
    if not cases or len(cases) % 3:
        raise ValueError("input requires complete intervention triples")
    groups, seen, used = [], set(), {"train": set(), "evaluation": set()}
    for start in range(0, len(cases), 3):
        labels = [c["label"].split("/") for c in cases[start:start + 3]]
        if (any(len(label) not in (3, 4) for label in labels)
                or [label[-1] for label in labels] != ["never", "now", "flexible"]
                or any(label[:-1] != labels[0][:-1] for label in labels)):
            raise ValueError("intervention labels do not form one ordered triple")
        partition, seed = labels[0][:2]
        seed = int(seed)
        node = labels[0][2] if len(labels[0]) == 4 else None
        key = (partition, seed, node)
        if partition not in used or seed < 0 or key in seen or node == "":
            raise ValueError("invalid or duplicate partition/seed")
        seen.add(key)
        used[partition].add(seed)
        groups.append((start, partition, seed))
    if not all(used.values()) or used["train"] & used["evaluation"]:
        raise ValueError("training and evaluation require disjoint declared seeds")
    return groups


def best_hp(reports, *, no_use=False):
    return max((p["final_hp"] for r in reports for p in observed_outcomes(r)
                if not no_use or p["total_potion_expenditures"] == 0), default=None)


def current_block_opportunity(row):
    c = row.position.columns
    incoming = c["entity_values"][c["entity_ids"][:, 0] == 3, 13].sum() * 50
    block = c["entity_values"][0, 2] * 50
    potency = c["entity_values"][row.potion_row, 2] * 20
    return min(potency, max(0., incoming - block))


def timing_query_indices(labels, rows, prediction, *, queries_per_battle=2):
    """Nominations from inputs alone, before any new query result is observed."""
    if queries_per_battle < 1 or len(labels) != len(rows) or prediction.shape != (len(rows), 2):
        raise ValueError("invalid timing query inputs or budget")
    if not np.isfinite(prediction).all():
        raise ValueError("nonfinite timing query predictions")
    groups = {}
    for i, label in enumerate(labels):
        groups.setdefault("/".join(label.split("/")[:2]), []).append(i)
    result = []
    for battle, indices in groups.items():
        selected = sorted(indices, key=lambda i: -prediction[i, 0])[:queries_per_battle]
        simple = sorted(indices, key=lambda i: -current_block_opportunity(rows[i]))[:queries_per_battle]
        hybrid = [simple[0]]
        for i in selected:
            if i not in hybrid and len(hybrid) < queries_per_battle:
                hybrid.append(i)
        result.append((battle, indices, selected, simple, hybrid))
    return result


def node_replay_cases(report, max_nodes):
    """Observe spaced exact prefixes of one new, verified no-use trajectory.

    Execute these cases through combat_solver --work 0. The prefix is replayed
    by Rust; Python neither reconstructs intermediate states nor edits RNG.
    """
    if max_nodes < 1 or report["config"]["max_potion_uses"] != 0:
        raise ValueError("node sources require a positive bound and a no-use search")
    partition, seed = report["case"]["label"].split("/")
    plans = [p for p in report["plans"] if p["outcome"]["total_potion_expenditures"] == 0]
    if not plans:
        return []
    plan = max(plans, key=lambda p: p["outcome"]["final_hp"])
    inputs = plan["inputs"]
    controls = [i for i, action in enumerate(inputs)
                if action == "EndTurn" or isinstance(action, dict) and "PlayCard" in action]
    indices = np.linspace(0, len(controls) - 1, min(max_nodes, len(controls)), dtype=int)
    return [{"label": f"{partition}/{seed}/{controls[i]}", "source": "snapshot",
             "producer": f"verified no-use trajectory/{report['implementation_sha256']}/{report['case']['label']}",
             "position": report["root"], "max_potion_uses": 0,
             "prefix": inputs[:controls[i]],
             "plan_proposals": [{"label": "known_no_use", "inputs": inputs[controls[i]:], "player_turns": []}]}
            for i in indices]


def node_intervention_cases(report):
    """Detach an actually replayed node while retaining its winning suffix."""
    root = report["root"]
    slots = [i for i, p in enumerate(root["combat"]["entities"]["potions"])
             if p is not None and p["id"] == "BlockPotion"]
    if (root["engine"] != "CombatPlayerTurn" or len(slots) != 1
            or report["information"] != "full_state_oracle"
            or report["search_work"] != 0
            or best_hp((dict(report, plans=[]),), no_use=True) is None):
        raise ValueError("node requires player control and its replayed no-use victory")
    return [{"label": f"{report['case']['label']}/{arm}", "source": "snapshot",
             "producer": f"replayed node/{report['implementation_sha256']}/{report['case']['label']}",
             "position": root, "max_potion_uses": None if arm == "flexible" else 0,
             "prefix": [{"UsePotion": {"potion_index": slots[0], "target": None}}] if arm == "now" else [],
             "plan_proposals": [] if arm == "now" else report["case"]["plan_proposals"]}
            for arm in ("never", "now", "flexible")]


def query_selection(triples, rows, prediction):
    """Offline query accounting; no branch suppression or additional rollout.

    Compare actual selected-query endpoints, not the richer teacher portfolio.
    Shared no-use routes separate potion benefit from card-search improvement.
    """
    mechanical = [current_block_opportunity(row) for row in rows]
    result = []
    for head, count in [(0, max(1, len(rows) // 4)), (1, max(1, len(rows) // 2))]:
        gains, work, measured = [], [], []
        for never, now, flexible in triples:
            baseline = best_hp((never, flexible), no_use=True)
            endpoint = best_hp(((now, flexible)[head],))
            measured.append(baseline is not None and endpoint is not None)
            gains.append(max(0, endpoint - baseline) if baseline is not None and endpoint is not None else 0)
            stats = (now, flexible)[head]["milestones"][-1]["stats"]
            work.append(sum(stats[k] for k in ("search_transitions", "replay_transitions", "shuffle_work",
                                              "plan_edit_work", "order_family_work", "result_backup_work")))
        order = np.argsort(-prediction[:, head], kind="stable")[:count]
        simple = np.argsort(-np.asarray(mechanical), kind="stable")[:count]
        result.append({"head": ("now", "timing_portfolio")[head], "query_count": count,
                       "evaluation_roots": len(rows), "benefit_cases_captured": sum(gains[i] > 0 for i in order),
                       "measured_comparisons_total": sum(measured),
                       "measured_comparisons_selected": sum(measured[i] for i in order),
                       "benefit_cases_total": sum(g > 0 for g in gains),
                       "observed_hp_gain_captured": sum(gains[i] for i in order),
                       "observed_hp_gain_total": sum(gains),
                       "mechanical_attack_gap_hp_gain": sum(gains[i] for i in simple),
                       "random_expected_hp_gain": sum(gains) * count / len(rows),
                       "selected_extra_work": sum(work[i] for i in order), "all_extra_work": sum(work),
                       "selected_labels": [triples[i][0]["case"]["label"] for i in order]})
    return result


def battle_timing_selection(triples, rows, prediction, *, queries_per_battle=2):
    """Rank actual use-now nodes within each battle, with a shared HP baseline.

    Each battle contributes one final result. Related nodes are not separate
    fights and their HP gains must not be added together. This is an offline
    full-state comparison along the recorded no-use trajectory.
    """
    result = []
    labels = [t[0]["case"]["label"] for t in triples]
    for battle, indices, selected, simple, hybrid in timing_query_indices(
            labels, rows, prediction, queries_per_battle=queries_per_battle):
        baseline = best_hp([r for i in indices for r in (triples[i][0], triples[i][2])], no_use=True)
        # One current-defense nomination and one learned nomination share a
        # fixed query budget. Coincident nominations cost one query, then the
        # remaining slots follow the learned order. Neither authorizes use.
        endpoints = [best_hp([triples[i][1] for i in pool]) for pool in (indices, selected, simple, hybrid)]
        gains = [max(0, hp - baseline) if baseline is not None and hp is not None else None for hp in endpoints]
        work = [sum(triples[i][1]["milestones"][-1]["stats"][k] for k in
                    ("search_transitions", "replay_transitions", "shuffle_work", "plan_edit_work",
                     "order_family_work", "result_backup_work")) for i in indices]
        result.append({"battle": battle, "nodes": len(indices), "selected_queries": len(selected),
                       "no_use_hp": baseline, "all_now_hp": endpoints[0], "selected_now_hp": endpoints[1],
                       "mechanical_now_hp": endpoints[2], "observed_hp_gains": gains,
                       "hybrid_now_hp": endpoints[3],
                       "hybrid_labels": [triples[i][1]["case"]["label"] for i in hybrid],
                       "selected_labels": [triples[i][1]["case"]["label"] for i in selected],
                       "all_query_work": sum(work), "selected_query_work": sum(work[indices.index(i)] for i in selected),
                       "hybrid_query_work": sum(work[indices.index(i)] for i in hybrid)})
    return result


def main():
    import sts_learning_bridge as bridge
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--solver", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--work", type=int, default=8000)
    parser.add_argument("--seconds", type=float, default=.5)
    args = parser.parse_args()
    if args.work < 1 or not np.isfinite(args.seconds) or args.seconds <= 0:
        raise ValueError("declare positive finite search budgets")
    cases = json.loads(args.input.read_text(encoding="utf-8-sig"))
    groups = declared_groups(cases)
    args.output.mkdir(parents=True, exist_ok=False)
    producer_files = {name: (Path(__file__).parent / name).read_bytes()
                      for name in ("potion_timing.py", "potion_timing_trial.py", "entity_learning.py")}
    producer_dir = args.output / "producer"
    producer_dir.mkdir()
    for name, source in producer_files.items():
        (producer_dir / name).write_bytes(source)
    (args.output / "inputs.json").write_text(json.dumps(cases), encoding="utf-8")
    started = time.perf_counter()
    with (args.output / "solver.log").open("w", encoding="utf-8") as log:
        subprocess.run([str(args.solver.resolve()), "--input", str(args.output / "inputs.json"),
                        "--output", str(args.output / "results"), "--work", str(args.work),
                        "--seconds", str(args.seconds)], stdout=log, stderr=subprocess.STDOUT, check=True)
    collection_seconds = time.perf_counter() - started
    splits, reports = {"train": [], "evaluation": []}, {"train": [], "evaluation": []}
    for start, partition, _ in groups:
        triple = [json.loads((args.output / "results" / f"case-{i:03}.json").read_text())
                  for i in range(start, start + 3)]
        splits[partition].append(encode_trial(bridge, *triple))
        reports[partition].append(triple)
    torch.set_num_threads(2)
    torch.manual_seed(20261003)
    model = PotionTimingValue()
    started = time.perf_counter()
    fit(model, splits["train"])
    fit_seconds = time.perf_counter() - started
    evaluation = splits["evaluation"]
    predicted = predict(model, evaluation)
    actual = np.asarray([r.gains for r in evaluation]) * HP_SCALE
    observed = np.asarray([r.observed for r in evaluation])
    summary = {"task": "BlockPotion observed intervention gain; no card policy or solver model",
               "collection_seconds": collection_seconds, "fit_seconds": fit_seconds,
               "work_per_arm": args.work, "seconds_per_arm": args.seconds,
               "train_roots": len(splits["train"]), "evaluation_roots": len(evaluation),
               "train_steps": 600, "entity_schema": SCHEMA, "model_seed": 20261003,
               "observed_targets": observed.sum(0).tolist(),
               "mae_hp": [float(np.abs(predicted[observed[:, j], j] - actual[observed[:, j], j]).mean())
                          if observed[:, j].any() else None for j in range(2)],
               "offline_query_selection": query_selection(reports["evaluation"], evaluation, predicted),
               "within_battle_timing": battle_timing_selection(reports["evaluation"], evaluation, predicted),
               "predictions": [{"label": triple[0]["case"]["label"], "gains": a.tolist(),
                                "observed": mask.tolist(), "prediction": p.tolist()}
                               for triple, a, mask, p in zip(reports["evaluation"], actual, observed, predicted)],
               "scope": "constructed Oracle roots; shared known no-use baseline; query ranking only"}
    torch.save(model.state_dict(), args.output / "model.pt")
    summary["producer"] = {"files_sha256": {
        name: hashlib.sha256(source).hexdigest() for name, source in producer_files.items()},
        "teacher_sha256": sorted({r[0]["implementation_sha256"] for partition in reports.values() for r in partition})}
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps({k: v for k, v in summary.items() if k != "predictions"}))


if __name__ == "__main__":
    main()
