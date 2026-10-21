"""Observed combat resources and explicit requirements, without reward weights.

These observations stop before run settlement. They do not rank whole-run value
or turn sampled policy continuations into optimal-action labels.
"""
from collections import Counter
import copy
import hashlib
import json
from pathlib import Path


CONTRACT = "combat_quality_v1"


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def resources(position):
    """Read evidence fields; never duplicate the run owner's settlement rules."""
    combat = position["combat"]
    player, meta, runtime = combat["entities"]["player"], combat["meta"], combat["runtime"]
    hp, maximum = player["current_hp"], player["max_hp"]
    if type(hp) is not int or type(maximum) is not int or not 0 <= hp <= maximum or maximum <= 0:
        raise ValueError("invalid resource HP")
    return copy.deepcopy(dict(hp=hp, max_hp=maximum, gold=player["gold"],
        potion_slots=[p["id"] if p is not None else None for p in combat["entities"]["potions"]],
        relics=player["relics"], energy_master=player["energy_master"],
        master_deck_snapshot=meta["master_deck_snapshot"],
        pending_meta_changes=meta["meta_changes"],
        pending_rewards=runtime["pending_rewards"],
        room_flags=dict(mugged=runtime["combat_mugged"], smoked=runtime["combat_smoked"])))


def project_episode(episode):
    initial, ending = resources(episode["root"]), resources(episode["successor"])
    outcome = episode["outcome"]
    kind = outcome["kind"] if outcome else "unknown_not_finished"
    if outcome:
        if (outcome["seed"] != episode["seed"] or outcome["hp"] != ending["hp"]
                or outcome["starting_hp"] != initial["hp"]
                or outcome["potions"] != ending["potion_slots"]):
            raise ValueError("resource evidence disagrees with outcome")
        if kind == "win" and (not outcome["win_replayed"] or ending["hp"] <= 0):
            raise ValueError("resource victory lacks replay or surviving HP")
    return dict(contract=CONTRACT, kind=kind, boundary="combat_terminal_before_run_settlement"
                if kind in ("win", "loss") else "unfinished_combat",
        root_sha256=digest(episode["root"]), initial=initial, observed_end=ending,
        rng_sha256=digest(episode["successor"]["combat"]["rng"]),
        continuation_context="unobserved")


def validate_requirements(requirements):
    ids = set()
    for r in requirements:
        if not isinstance(r.get("id"), str) or not r["id"] or r["id"] in ids:
            raise ValueError("requirements need distinct IDs")
        ids.add(r["id"])
        if type(r.get("min_hp")) is not int or r["min_hp"] < 0:
            raise ValueError("invalid minimum HP requirement")
        reserve = r.get("reserve")
        if isinstance(reserve, str):
            if reserve not in ("unrestricted", "starting_inventory"):
                raise ValueError("unknown inventory requirement")
        elif not isinstance(reserve, dict) or any(
                not isinstance(k, str) or not k or type(v) is not int or v <= 0
                for k, v in reserve.items()):
            raise ValueError("inventory requirement needs potion types and positive counts")


def meets(quality, requirement):
    """One validated requirement on an observation, never a policy guarantee."""
    if quality is None:
        return None
    if quality["contract"] != CONTRACT:
        raise ValueError("quality contract mismatch")
    if quality["kind"] not in ("win", "loss"):
        return None
    if quality["kind"] == "loss":
        return False
    end = quality["observed_end"]
    if end["hp"] < requirement["min_hp"]:
        return False
    reserve = requirement["reserve"]
    if reserve == "unrestricted":
        return True
    required = (Counter(p for p in quality["initial"]["potion_slots"] if p is not None)
                if reserve == "starting_inventory" else Counter(reserve))
    remaining = Counter(p for p in end["potion_slots"] if p is not None)
    return all(remaining[p] >= n for p, n in required.items())


def attainment(rows, requirements):
    """All-start empirical counts; bounds describe unresolved observations only."""
    validate_requirements(requirements)
    result = []
    for r in requirements:
        counts = Counter(meets(row.get("quality"), r) for row in rows)
        n, hit, miss, unresolved = len(rows), counts[True], counts[False], counts[None]
        result.append(dict(requirement=r, cases=n, met=hit, missed=miss, unresolved=unresolved,
            observed_fraction_bounds=[hit/n, (hit+unresolved)/n] if n else None))
    return result


def compare_observations(before, after):
    """Describe paired resource changes; do not assert strategic dominance."""
    if (before["contract"] != CONTRACT or after["contract"] != CONTRACT
            or before["root_sha256"] != after["root_sha256"]
            or before["initial"] != after["initial"]):
        raise ValueError("quality comparison needs the same source root")
    if before["kind"] not in ("win", "loss") or after["kind"] not in ("win", "loss"):
        return dict(relation="unresolved", before=before["kind"], after=after["kind"])
    if before["kind"] != after["kind"]:
        return dict(relation="survival_changed", before=before["kind"], after=after["kind"])
    if before["kind"] == "loss":
        return dict(relation="both_lost")
    a, b = before["observed_end"], after["observed_end"]
    changed = [k for k in a if a[k] != b[k]]
    delta = b["hp"] - a["hp"]
    if not changed:
        relation = "same_recorded_resources"
    elif changed == ["hp"]:
        relation = "more_hp_same_other_recorded_resources" if delta > 0 else "less_hp_same_other_recorded_resources"
    else:
        relation = "multiple_resource_changes"
    return dict(relation=relation, hp_change=delta, changed=changed,
        details={k:dict(before=a[k], after=b[k]) for k in changed},
        rng_changed=before["rng_sha256"] != after["rng_sha256"],
        continuation_value="unassessed")


def report(rows, requirements):
    """Separate exact construction contexts and actual enemy compositions."""
    groups = {}
    for row in rows:
        if row.get("quality") and row["quality"]["kind"] != row["kind"]:
            raise ValueError("quality row status disagrees with its observation")
        context = row["quality_context"]
        key = digest(dict(context=context, enemies=row.get("initial_enemies")))
        groups.setdefault(key, []).append(row)
    return dict(contract=CONTRACT,
        scope="Sampled combat outcomes before run settlement; no scalar score or strategy ranking.",
        bounds="Fractions of declared cases; unresolved bounds are not statistical confidence intervals.",
        reserve_semantics="Outgoing potion types and multiplicities, not use-count penalties or instance retention.",
        requirements=requirements,
        groups=[dict(context=rs[0]["quality_context"], enemies=rs[0].get("initial_enemies"),
            cases=len(rs), outcomes=dict(Counter(r["kind"] if r["kind"] in ("win", "loss") else "unknown" for r in rs)),
            attainment=attainment(rs, requirements),
            cases_with_pending_meta_changes=[r["id"] for r in rs if r.get("quality") and
                r["quality"]["observed_end"]["pending_meta_changes"]],
            rows=rs) for rs in groups.values()])


def review_trial(source, output, requirements):
    """Read declared endpoints, retaining every case and existing raw evidence."""
    import gzip
    source, output = Path(source), Path(output)
    validate_requirements(requirements)
    plan = json.loads((source / "plan.json").read_text(encoding="utf-8"))
    summary = json.loads((source / "summary.json").read_text(encoding="utf-8"))
    if plan["schema"] != 4 or plan["partition"] != "development":
        raise ValueError("quality review requires schema-4 development evidence")
    if len({c["id"] for c in plan["evaluation"]}) != len(plan["evaluation"]):
        raise ValueError("duplicate quality case IDs")
    model_ids = [m["seed"] for m in summary["models"]]
    if len(model_ids) != len(set(model_ids)) or not set(model_ids) <= set(plan["model_seeds"]):
        raise ValueError("quality model accounting differs from plan")
    output.mkdir(parents=True, exist_ok=False)
    def write(name, data):
        (output / name).write_text(json.dumps(data, indent=2, allow_nan=False), encoding="utf-8")
    def file_sha(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()
    manifest = dict(contract=CONTRACT, requirements=requirements,
        source_directory=str(source.resolve()),
        source_files={p:file_sha(source / p) for p in ("plan.json", "producer.json", "summary.json")},
        parser_sha256=file_sha(Path(__file__)), parser_source=Path(__file__).read_text(encoding="utf-8"),
        artifacts={}, complete=False, models=[])
    write("review.json", manifest)
    try:
        for seed in plan["model_seeds"]:
            model = next((m for m in summary["models"] if m["seed"] == seed), None)
            endpoints = {}
            for endpoint in ("initial", "final"):
                declared = model.get(endpoint, {}).get("rows", []) if model else []
                by_id = {r["id"]:r for r in declared}
                if len(by_id) != len(declared) or not by_id.keys() <= {c["id"] for c in plan["evaluation"]}:
                    raise ValueError("quality endpoint case accounting differs from plan")
                rows = [dict(id=c["id"], family=c["family"], encounter=c["spec"]["encounter"],
                    seed=c["spec"]["seed"], kind="unknown_not_attempted", initial_enemies=None,
                    quality_context={k:v for k,v in c["spec"].items() if k != "seed"}) for c in plan["evaluation"]]
                width = plan.get("evaluation_batch_size", 16)
                if type(width) is not int or not 1 <= width <= 128:
                    raise ValueError("invalid recorded evaluation batch size")
                for start in range(0, len(rows), width):
                    name = f"model-{seed}/{endpoint}/batch-{start:04}/evidence.json.gz"
                    path = source / name
                    current = rows[start:start + width]
                    if not path.exists():
                        if any(by_id.get(r["id"], {}).get("kind") in ("win", "loss") for r in current):
                            raise ValueError("reported terminal lacks quality evidence")
                        continue
                    manifest["artifacts"][name] = file_sha(path)
                    with gzip.open(path, "rt", encoding="utf-8") as stream:
                        evidence = json.load(stream)
                    if evidence["schema"] != 4 or evidence["poisoned"] or len(evidence["episodes"]) != len(current):
                        raise ValueError("invalid quality evidence population")
                    for row, episode in zip(current, evidence["episodes"]):
                        if episode["seed"] != row["seed"]:
                            raise ValueError("quality evidence seed mismatch")
                        spec = dict(row["quality_context"], seed=row["seed"])
                        if episode["construction"] != spec:
                            raise ValueError("quality evidence construction mismatch")
                        quality = project_episode(episode)
                        original = by_id.get(row["id"])
                        def status(kind):
                            return kind if kind in ("win", "loss") else "unknown"
                        observed = episode["outcome"]
                        if original and (status(original["kind"]) != status(quality["kind"]) or
                                (observed and any(original[k] != observed.get(k)
                                 for k in ("hp", "potions", "potion_uses") if k in original))):
                            raise ValueError("quality evidence differs from endpoint report")
                        row.update(kind=quality["kind"], quality=quality,
                            initial_enemies=[m["enemy_id"] for m in episode["root"]["combat"]["entities"]["monsters"]])
                endpoints[endpoint] = rows
                write(f"model-{seed}-{endpoint}.json", report(rows, requirements))
            paired = []
            for before, after in zip(endpoints["initial"], endpoints["final"]):
                comparison = (compare_observations(before["quality"], after["quality"])
                              if before.get("quality") and after.get("quality") else dict(relation="unresolved"))
                paired.append(dict(id=before["id"], family=before["family"], encounter=before["encounter"], **comparison))
            write(f"model-{seed}-paired.json", paired)
            manifest["models"].append(dict(seed=seed, cases=len(paired),
                relations=dict(Counter(r["relation"] for r in paired))))
        manifest["complete"] = True
    except BaseException as exc:
        manifest["error"] = f"{type(exc).__name__}: {exc}"
        raise
    finally:
        write("review.json", manifest)
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--requirements", type=Path, required=True)
    args = parser.parse_args()
    result = review_trial(args.source, args.output, json.loads(args.requirements.read_text(encoding="utf-8")))
    print(f"Quality review: {len(result['models'])} model pairs; full resources and declared requirements saved.")
