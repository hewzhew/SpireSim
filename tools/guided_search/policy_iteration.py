"""Student-state solver teaching, finite PUCT, and played-actor value updates.

Run with py.ps1. No corpus imports, historical checkpoints or model-only player.
"""
import argparse
import copy
from dataclasses import dataclass, replace
import gzip
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch
from torch.nn import functional as F
from sts_learning_bridge import PolicySearchBatch, policy_demonstration

from entity_learning import COLUMNS, SCHEMA, EntityNetwork, pack, positions


@dataclass
class Sample:
    position: object
    policy: np.ndarray
    value: float | None
    slot: int
    choice: int
    query: bool = False
    search: dict | None = None


def population(seed, count):
    """Declared constructed loadouts, not naturally reached floors or an A20H run."""
    starter = [("Strike", 0)] * 5 + [("Defend", 0)] * 4 + [("Bash", 0), ("AscendersBane", 0)]
    elite = starter + [("Carnage", 0), ("ShrugItOff", 0), ("PommelStrike", 0)]
    boss = [("Strike", 0)] * 3 + [("Defend", 0)] * 3 + [
        ("Bash", 1), ("AscendersBane", 0), ("Carnage", 0), ("PommelStrike", 1),
        ("ShrugItOff", 1), ("Disarm", 1), ("Impervious", 0), ("Inflame", 0),
        ("Armaments", 0), ("TrueGrit", 1)]
    recipes = [("JawWorm", "MonsterRoom", starter),
               ("GremlinNob", "MonsterRoomElite", elite),
               ("Lagavulin", "MonsterRoomElite", elite),
               ("TheGuardian", "MonsterRoomBoss", boss)]
    return [dict(seed=seed+i, encounter=recipes[i % 4][0], room=recipes[i % 4][1], act=1,
                 hp=60, deck=list(recipes[i % 4][2]), relics=[], potions=[], max_potion_uses=0)
            for i in range(count)]


def training_population(seed, count):
    """Cross starting HP and one-card attack/block changes within each encounter."""
    specs = population(seed, count)
    for i, spec in enumerate(specs):
        spec["hp"] = (45, 60, 75)[(i // 4) % 3]
        edit = (i // 12) % 3
        if edit:
            old, new = ("Strike", "Defend") if edit == 1 else ("Defend", "Strike")
            spec["deck"][spec["deck"].index((old, 0))] = (new, 0)
    return specs


def save_samples(path, rows):
    payload = {name: np.concatenate([r.position.columns[name] for r in rows])
               if rows else np.empty((0, width), dtype=np.int64 if integer else np.float32)
               for name, (width, integer) in COLUMNS.items()}
    for domain in ("entity", "action"):
        payload[f"{domain}_splits"] = np.cumsum([0] + [len(r.position.columns[f"{domain}_ids"]) for r in rows])
    payload.update(schema=SCHEMA, policy=np.concatenate([r.policy for r in rows]) if rows else [],
                   value=[np.nan if r.value is None else r.value for r in rows],
                   slot=[r.slot for r in rows], choice=[r.choice for r in rows],
                   query=np.asarray([r.query for r in rows], dtype=bool))
    np.savez_compressed(path, **payload)


def bootstrap(specs, work, directory, *, prefixes=None):
    started = time.monotonic()
    prefixes = [[] for _ in specs] if prefixes is None else prefixes
    if len(prefixes) != len(specs):
        raise ValueError("one demonstration prefix is required per construction")
    rows, results = [], []
    for slot, (spec, prefix) in enumerate(zip(specs, prefixes)):
        frame = policy_demonstration(spec, work, prefix)
        results.append(dict(slot=slot, found=frame is not None, value=None if frame is None else frame["value"]))
        if frame is not None:
            for p, choice in zip(positions(frame), frame["choices"]):
                target = np.zeros(len(p.columns["actions"]), dtype=np.float32)
                target[choice] = 1
                rows.append(Sample(p, target, frame["value"], slot, choice))
        if (slot+1) % 8 == 0 or frame is None or slot+1 == len(specs):
            print(json.dumps(dict(stage="demonstration", case=slot+1, total=len(specs),
                                  found=sum(r["found"] for r in results), seconds=round(time.monotonic()-started, 2))), flush=True)
    directory.mkdir()
    (directory / "sources.json").write_text(json.dumps(dict(specifications=specs, prefix_choices=prefixes,
        work=work, results=results)), encoding="utf-8")
    save_samples(directory / "samples.npz", rows)
    return rows


def student_demonstrations(specs, played, work, directory, rng):
    """Teach ordinary decisions and rare mandatory selections from played states."""
    traces = [[] for _ in specs]
    for row in played:
        if row.query or not 0 <= row.slot < len(specs):
            raise ValueError("teaching needs played root decisions from this cohort")
        traces[row.slot].append(row)
    selected, prefixes, sources, queries = [], [], [], []
    family_counts = {}
    for slot, trace in enumerate(traces):
        if not trace:
            raise ValueError("student fight has no recorded decision")
        # BeginSelection (5) is the simulator's PendingChoice boundary. Member
        # selection and submission (6/7) are virtual assembly, not a restart root.
        candidates = [i for i, row in enumerate(trace)
                      if not np.isin(row.position.columns["action_ids"][:, 0], [6, 7]).any()
                      and (len(row.policy) > 1 or (row.position.columns["action_ids"][:, 0] == 5).any())]
        ordinary, families = [], {}
        for index in candidates:
            c = trace[index].position.columns
            if (c["action_ids"][:, 0] == 5).any():
                # Engine reason IDs identify the attached task (e.g. exhaust or
                # upgrade), without defining which card is desirable. Query one
                # such family per fight, balancing reasons across this cohort.
                family = tuple(sorted(c["entity_ids"][c["entity_ids"][:, 0] == 7, 1].tolist()))
                families.setdefault(family, []).append(index)
            else:
                ordinary.append(index)
        indices = [(int(rng.choice(ordinary)), "ordinary")] if ordinary else []
        if families:
            least = min(family_counts.get(key, 0) for key in families)
            eligible = [key for key in families if family_counts.get(key, 0) == least]
            family = eligible[int(rng.integers(len(eligible)))]
            indices.append((int(rng.choice(families[family])), "selection"))
            family_counts[family] = family_counts.get(family, 0) + 1
        if not indices:
            indices = [(0, "forced")]
        for index, kind in indices:
            selected.append(trace[index])
            prefixes.append([r.choice for r in trace[:index]])
            queries.append(specs[slot])
            sources.append(dict(slot=slot, decision=index, kind=kind))
    directory.mkdir()
    rows = bootstrap(queries, work, directory / "solver", prefixes=prefixes)
    first = {}
    for row in rows:
        first.setdefault(row.slot, row)
    for slot, row in first.items():
        if any(not np.array_equal(row.position.columns[k], selected[slot].position.columns[k]) for k in COLUMNS):
            raise ValueError("solver teaching did not restart at the recorded student state")
    adopted, comparisons = [], []
    for i, (row, source_info) in enumerate(zip(selected, sources)):
        slot, index = source_info["slot"], source_info["decision"]
        teacher = first.get(i)
        if teacher is not None and (row.value is None or teacher.value > row.value):
            source = "solver"
            adopted.extend(replace(r, slot=slot) for r in rows if r.slot == i)
        elif row.value is not None and row.value > 0:
            source = "student"
            adopted.extend(traces[slot][index:])
        else:
            source = "none"
        comparisons.append(dict(query=i, **source_info, student_choice=row.choice,
                                student_return=row.value, teacher_choice=teacher.choice if teacher else None,
                                teacher_return=teacher.value if teacher else None, adopted=source))
    save_samples(directory / "samples.npz", adopted)
    (directory / "corrections.json").write_text(json.dumps(comparisons), encoding="utf-8")
    return adopted


def collect(model, specs, simulations, directory, rng, *, training, leaf_observer=None, max_seconds=None):
    if max_seconds is not None and (not np.isfinite(max_seconds) or max_seconds <= 0):
        raise ValueError("declare a positive finite collection time budget")
    started = time.monotonic()
    env = PolicySearchBatch(specs, simulations, 256)
    construction_seconds = time.monotonic() - started
    rows, summary = play(model, env, simulations, rng, training=training,
                         leaf_observer=leaf_observer, progress_label=directory.name,
                         max_seconds=None if max_seconds is None else max(0., max_seconds-construction_seconds))
    summary["construction_seconds"] = construction_seconds
    summary["seconds"] += construction_seconds
    summary["time_budget_seconds"] = max_seconds
    for i, result in enumerate(summary["outcomes"]):
        result["encounter"] = specs[i]["encounter"]
    directory.mkdir()
    with gzip.open(directory / "evidence.json.gz", "wt", encoding="utf-8") as f:
        f.write(env.evidence_json())
    save_samples(directory / "samples.npz", rows)
    with gzip.open(directory / "search.json.gz", "wt", encoding="utf-8") as f:
        json.dump([r.search for r in rows], f)
    (directory / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(dict(stage=directory.name, **{k:v for k,v in summary.items() if k != "outcomes"})), flush=True)
    return rows, summary


def play(model, env, simulations, rng, *, training=False, leaf_observer=None, progress_label=None, max_seconds=None):
    """One execution policy for constructed roots and cloned search queries."""
    rows, evaluations, calls = [], 0, 0
    started = time.monotonic()
    deadline = None if max_seconds is None else started + max_seconds
    stop = "complete"
    next_update = started + 20
    root_request = True
    model.eval()
    while True:
        if deadline is not None and time.monotonic() >= deadline:
            stop = "time_budget"
            break
        frame = env.evaluation_batch()
        if len(frame["slots"]):
            items = positions(frame)
            if leaf_observer is not None:
                leaf_observer(env, frame, items)
            with torch.no_grad():
                logits, values = model(pack(items))
                probabilities = logits.softmax(-1).numpy()
                values = values.sigmoid().numpy().astype(float).tolist()
            weights = [probabilities[i, :len(p.columns["actions"])].astype(float).tolist() for i, p in enumerate(items)]
            if training and root_request:
                # Optional exploratory actor for diagnostics. Its return labels
                # describe this noisy actor, not deterministic deployment.
                weights = [(0.75*np.asarray(w) + 0.25*rng.dirichlet(np.full(len(w), 0.3))).tolist()
                           for w in weights]
            env.evaluate(frame["revision"], weights, values)
            root_request = False
            evaluations += len(items)
            calls += 1
        else:
            frame = env.decision_batch()
            if not len(frame["slots"]):
                break
            choices = []
            for i, (slot, p, visits) in enumerate(zip(frame["slots"], positions(frame), frame["visits"])):
                visit_policy = np.asarray(visits, dtype=np.float32)
                if visit_policy.sum() != simulations:
                    raise ValueError("root visits do not account for simulations")
                visit_policy /= visit_policy.sum()
                if training:
                    choice = int(rng.choice(len(visit_policy), p=visit_policy))
                else:
                    # Small searches often tie on visits. Use the backed-up
                    # return among visited leaders before source-local order.
                    tied = np.flatnonzero(visit_policy == visit_policy.max())
                    choice = int(max(tied, key=lambda a: frame["qvalues"][i][a]))
                # Record the actor's adopted decision. Visits describe its
                # exploration, including actions it tried and then rejected.
                target = np.zeros_like(visit_policy)
                target[choice] = 1.
                statistics = {name: frame[name][i] for name in (
                    "visits", "priors", "qvalues", "root_values", "regularization",
                    "terminal_visits", "terminal_values")}
                rows.append(Sample(p, target, None, int(slot), choice, search=statistics))
                choices.append(choice)
            env.advance(frame["revision"], choices)
            root_request = True
        if progress_label and time.monotonic() >= next_update:
            print(json.dumps(dict(stage=progress_label, decisions=len(rows), evaluations=evaluations,
                                  seconds=round(time.monotonic()-started, 2))), flush=True)
            next_update = time.monotonic() + 20
    outcomes = json.loads(env.outcomes_json())
    for row in rows:
        row.value = outcomes[row.slot]["value"]
    counts = {}
    for i, result in enumerate(outcomes):
        # Diagnostic only: having a playable card is not proof EndTurn is wrong.
        result["end_with_playable"] = int(sum(r.slot == i and r.position.columns["action_ids"][r.choice, 0] == 2
            and np.any(r.position.columns["action_ids"][:, 0] == 1) for r in rows))
        counts[result["kind"]] = counts.get(result["kind"], 0) + 1
    actor = dict(simulations=simulations,
                 action_selection="sample_visits" if training else "argmax_visits_then_q",
                 root_noise=dict(fraction=0.25, dirichlet_alpha=0.3) if training else None)
    summary = dict(outcomes=outcomes, counts=counts, actor=actor, decisions=len(rows), evaluations=evaluations,
                   inference_batches=calls, seconds=time.monotonic()-started, stop=stop)
    return rows, summary


def policy_cross_entropy(logits, data, rows, *, weights=None):
    targets = torch.zeros_like(logits)
    for i, row in enumerate(rows):
        targets[i, :len(row.policy)] = torch.from_numpy(row.policy)
    # Padding is zeroed before multiplication to avoid 0 * -inf.
    losses = -(targets * logits.log_softmax(-1).masked_fill(~data["mask"], 0)).sum(-1)
    if weights is not None:
        losses = losses * torch.as_tensor(weights, dtype=losses.dtype, device=losses.device)
    return losses.mean()


def update(model, optimizer, rows, steps, rng):
    model.train()
    losses = []
    for _ in range(steps):
        chosen = rng.choice(len(rows), min(32, len(rows)), replace=False)
        batch = [rows[int(i)] for i in chosen]
        data = pack([r.position for r in batch])
        logits, values = model(data)
        policy_loss = policy_cross_entropy(logits, data, batch)
        observed = torch.tensor([r.value is not None for r in batch])
        returns = torch.tensor([r.value or 0. for r in batch], dtype=torch.float32)
        value_loss = F.binary_cross_entropy_with_logits(values[observed], returns[observed]) if observed.any() else values.sum()*0
        loss = policy_loss + value_loss
        if not torch.isfinite(loss):
            raise ValueError("nonfinite policy/value update")
        optimizer.zero_grad()
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.)
        optimizer.step()
        losses.append([float(policy_loss.detach()), float(value_loss.detach())])
    return np.mean(losses[-min(20, steps):], axis=0).tolist()


class FrozenPolicyValue(torch.nn.Module):
    """Combine a phase-frozen action network with a separately fit value."""
    def __init__(self, policy, critic):
        super().__init__()
        self.policy = policy
        self.critic = critic

    def forward(self, data):
        logits, _ = self.policy(data)
        _, value = self.critic(data)
        return logits, value


def append_policy_examples(pool, rows):
    """Keep each cohort's fights distinct without altering saved local slots."""
    offset = max((r.slot for r in pool), default=-1) + 1
    pool.extend(replace(r, slot=offset+r.slot) for r in rows)


def improve_policy(policy, optimizer, rows, demonstrations, steps, rng):
    """Fit accumulated accepted continuations and this run's initial demonstrations."""
    if steps <= 0 or not demonstrations or any(r.policy.shape != (len(r.position.columns["actions"]),)
                       or not np.isclose(r.policy.sum(), 1.) or np.any(r.policy < 0.)
                       or not np.isfinite(r.policy).all() for group in (rows, demonstrations) for r in group):
        raise ValueError("policy improvement needs normalized action targets")
    # A normalized target over one legal action has identically zero action
    # loss. Keep such states in evidence/value data, not policy minibatches.
    groups = [group for group in (rows, demonstrations) if group]
    fights = []
    for group in groups:
        by_slot = {}
        for row in group:
            by_slot.setdefault(row.slot, []).append(row)
        choices = [(selected, len(selected)/len(fight)) for fight in by_slot.values()
                   if (selected := [r for r in fight if len(r.policy) > 1])]
        if choices:
            # Conditioning on a choice must not change the original equal-fight,
            # uniform-decision objective. Forced-only fights have zero loss.
            fights.append([(selected, mass*len(choices)/len(by_slot)) for selected,mass in choices])
    if not fights:
        policy.eval().requires_grad_(False)
        return 0.
    pool_mass = len(fights)/len(groups)
    policy.train().requires_grad_(True)
    losses = []
    try:
        for _ in range(steps):
            # Both cohorts contain executed wins. The student actions alone are
            # observations, not automatically better teaching targets.
            # Equal fight mass prevents long plans and extra attached-choice
            # queries from silently changing the encounter mixture.
            batch, weights = [], []
            for cases in fights:
                for i in rng.choice(len(cases), 32 // len(fights), replace=True):
                    fight, mass = cases[int(i)]
                    batch.append(fight[int(rng.integers(len(fight)))])
                    weights.append(pool_mass*mass)
            data = pack([r.position for r in batch])
            logits, _ = policy(data)
            loss = policy_cross_entropy(logits, data, batch, weights=weights)
            if not torch.isfinite(loss):
                raise ValueError("nonfinite search distillation loss")
            optimizer.zero_grad()
            loss.backward()
            torch.nn.utils.clip_grad_norm_(policy.parameters(), 1.)
            optimizer.step()
            losses.append(float(loss.detach()))
    finally:
        policy.eval().requires_grad_(False)
    return float(np.mean(losses[-min(20, steps):]))


class LeafReturns:
    """Queried states continued by the frozen deployment search actor."""
    def __init__(self, policy, critic, rng, simulations, *, query_limit=16):
        if not 1 <= simulations <= 4096 or query_limit < 1:
            raise ValueError("declare positive search/leaf sampling budgets")
        self.policy, self.critic, self.rng = policy, critic, rng
        self.simulations, self.query_limit = simulations, query_limit
        self.rows, self.queries, self.evidence = [], [], []
        self.seconds = self.transitions = self.evaluations = 0

    def __call__(self, parent, frame, items):
        selected = [i for i in range(len(items)) if self.rng.integers(64) == 0]
        selected = selected[:max(0, self.query_limit - len(self.queries))]
        if not selected:
            return
        started = time.monotonic()
        slots = [int(frame["slots"][i]) for i in selected]
        env = parent.leaf_rollouts(frame["revision"], slots, self.simulations, 256)
        with torch.no_grad():
            _, raw = self.critic(pack([items[i] for i in selected]))
            estimates = raw.sigmoid().numpy()
        rows, summary = play(FrozenPolicyValue(self.policy, self.critic), env,
                             self.simulations, self.rng)
        outcomes = summary["outcomes"]
        first = {}
        for i, row in enumerate(rows):
            first.setdefault(row.slot, i)
            # Continuations teach value only; rejected-search moves are not
            # turned into action-imitation labels.
            row.policy.fill(0.)
        for row in rows:
            row.query = rows[first[row.slot]] is row
        for i, item in enumerate(selected):
            for key in items[item].columns:
                if not np.array_equal(rows[first[i]].position.columns[key], items[item].columns[key]):
                    raise ValueError("search continuation differs from its queried source")
        first_sample = len(self.rows)
        self.rows.extend(rows)
        for i, outcome in enumerate(outcomes):
            self.queries.append(dict(prediction=float(estimates[i]), sample_index=first_sample+first[i],
                                     parent_slot=slots[i], **outcome))
        self.evidence.append(json.loads(env.evidence_json()))
        self.seconds += time.monotonic() - started
        self.transitions += sum(o["search_transitions"] for o in outcomes)
        self.evaluations += summary["evaluations"] + len(selected)

    def save(self, directory):
        directory.mkdir()
        save_samples(directory / "samples.npz", self.rows)
        observed = [q for q in self.queries if q["value"] is not None]
        mse = float(np.mean([(q["prediction"]-q["value"])**2 for q in observed])) if observed else None
        summary = dict(queries=len(self.queries), query_limit=self.query_limit,
                       simulations=self.simulations, action_selection="argmax_visits_then_q",
                       samples=len(self.rows), mse=mse, seconds=self.seconds,
                       search_transitions=self.transitions, evaluations=self.evaluations,
                       wins=sum(q["kind"] == "win" for q in self.queries),
                       losses=sum(q["kind"] == "loss" for q in self.queries),
                       truncated=sum(q["kind"] == "truncated" for q in self.queries))
        (directory / "queries.json").write_text(json.dumps(self.queries), encoding="utf-8")
        (directory / "summary.json").write_text(json.dumps(summary), encoding="utf-8")
        with gzip.open(directory / "evidence.json.gz", "wt", encoding="utf-8") as f:
            json.dump(self.evidence, f)
        return summary


def update_value(critic, optimizer, rows, steps, rng):
    rows = [r for r in rows if r.value is not None]
    if not rows:
        raise ValueError("value update needs completed policy continuations")
    queries = [r for r in rows if r.query]
    suffixes = [r for r in rows if not r.query]
    groups = [g for g in (queries, suffixes) if g]
    critic.train()
    losses = []
    for _ in range(steps):
        # Give the queried states and their continuation states equal learning
        # mass. A long continuation must not drown out the search query it labels.
        batch = [g[int(i)] for g in groups
                 for i in rng.choice(len(g), 32 // len(groups), replace=True)]
        _, values = critic(pack([r.position for r in batch]))
        targets = torch.tensor([r.value for r in batch], dtype=torch.float32)
        loss = F.binary_cross_entropy_with_logits(values, targets)
        if not torch.isfinite(loss):
            raise ValueError("nonfinite continuation value loss")
        optimizer.zero_grad()
        loss.backward()
        torch.nn.utils.clip_grad_norm_(critic.parameters(), 1.)
        optimizer.step()
        losses.append(float(loss.detach()))
    critic.eval()
    return float(np.mean(losses[-min(20, steps):]))


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cases", type=int, default=16)
    parser.add_argument("--simulations", type=int, default=24)
    parser.add_argument("--teacher-work", type=int, default=6000)
    parser.add_argument("--updates", type=int, default=200)
    parser.add_argument("--iterations", type=int, default=2)
    parser.add_argument("--leaf-queries", type=int, default=16)
    args = parser.parse_args()
    if not 4 <= args.cases <= 128 or args.cases % 4 or not 1 <= args.simulations <= 4096 or not 1 <= args.teacher_work <= 100_000 or min(args.updates, args.iterations, args.leaf_queries) <= 0:
        parser.error("use supported positive budgets and 4..128 cases divisible by four")
    args.output.mkdir(parents=True, exist_ok=False)
    torch.set_num_threads(4)
    torch.manual_seed(1307)
    rng = np.random.default_rng(1307)
    model = EntityNetwork(hidden=64, layers=2)
    optimizer = torch.optim.AdamW(model.parameters(), lr=3e-4, weight_decay=1e-4)
    heldout = population(1130000, 8)
    config = dict(vars(args), output=str(args.output), entity_schema=SCHEMA,
                  objective="combat_win_plus_retained_hp_v1", information="oracle",
                  hidden=64, layers=2, seed=1307, heldout=heldout,
                  value_target="actual_deployed_search_actor_return",
                  value_sampling="equal_search_queries_and_current_actor_decisions",
                  policy_query_sampling="ordinary_plus_balanced_attached_selection",
                  policy_minibatch_sampling="choices_with_original_equal_fight_decision_mass",
                  policy_target="equal_accumulated_continuations_and_initial_demonstrations",
                  policy_replay="accepted_continuations_this_run",
                  value_replay="current_actor_generation",
                  producer_head=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip())
    (args.output / "configuration.json").write_text(json.dumps(config, indent=2), encoding="utf-8")
    (args.output / "producer.patch").write_bytes(subprocess.check_output(["git", "diff", "HEAD"]))
    # Also retain the currently untracked implementation sources in the artifact.
    sources = args.output / "sources"
    sources.mkdir()
    for file in [Path(__file__), Path("crates/sts_guided_search/src/policy_search.rs"), Path("crates/sts_guided_search/src/policy_search/rollout.rs"), Path("crates/sts_guided_search/src/entities/effects.rs"), Path("bindings/python_learning/src/policy.rs")]:
        (sources / file.name).write_bytes(file.read_bytes())
    _, before = collect(model, heldout, args.simulations, args.output / "initial", rng, training=False)
    teachers = bootstrap(population(1010000, args.cases), args.teacher_work, args.output / "teacher")
    if not teachers:
        raise RuntimeError("no fresh demonstrations; inspect these encounters before initializing imitation")
    warm_loss = update(model, optimizer, teachers, args.updates, rng)
    torch.save(dict(model=model.state_dict(), config=config), args.output / "warm.pt")
    _, warm = collect(model, heldout, args.simulations, args.output / "warm", rng, training=False)
    policy = copy.deepcopy(model).eval().requires_grad_(False)
    critic = copy.deepcopy(model)
    policy_optimizer = torch.optim.AdamW(policy.parameters(), lr=3e-4, weight_decay=1e-4)
    value_optimizer = torch.optim.AdamW(critic.parameters(), lr=3e-4, weight_decay=1e-4)
    player = FrozenPolicyValue(policy, critic)
    history = []
    policy_examples = []
    for iteration in range(args.iterations):
        started = time.monotonic()
        specs = training_population(1210000+iteration*1000, args.cases)
        student, source = collect(player, specs, args.simulations,
                                  args.output / f"student-source-{iteration}", rng, training=False)
        teacher_directory = args.output / f"policy-teacher-{iteration}"
        searched = student_demonstrations(specs, student, args.teacher_work, teacher_directory, rng)
        queries = json.loads((teacher_directory / "corrections.json").read_text(encoding="utf-8"))
        # Executed winning action sequences remain valid teaching examples under
        # this run's unchanged simulator/schema. Their old actor returns are not
        # reused for critic training.
        append_policy_examples(policy_examples, searched)
        teaching = dict(kind="improved_student_continuations", cases=args.cases,
                        queries=len(queries), selection_queries=sum(q["kind"] == "selection" for q in queries),
                        taught_cases=len({r.slot for r in searched}), samples=len(searched),
                        accumulated_samples=len(policy_examples),
                        work_per_query=args.teacher_work, seconds=time.monotonic()-started,
                        student_source=source)
        policy_loss = improve_policy(policy, policy_optimizer, policy_examples, teachers, args.updates, rng)
        leaves = LeafReturns(policy, critic, rng, args.simulations, query_limit=args.leaf_queries)
        played, summary = collect(player, training_population(1020000+iteration*1000, args.cases), args.simulations,
                                  args.output / f"selfplay-{iteration}", rng, training=False, leaf_observer=leaves)
        # Match deployment's priors and maximum visits with Q tie-breaks throughout the
        # fight. Exploratory mistakes must not become this actor's return labels.
        leaf_summary = leaves.save(args.output / f"leaves-{iteration}")
        loss = update_value(critic, value_optimizer, [*played, *leaves.rows], args.updates, rng)
        torch.save(dict(policy=policy.state_dict(), critic=critic.state_dict(), generation=iteration+1, config=config), args.output / f"iteration-{iteration}.pt")
        _, evaluation = collect(player, heldout, args.simulations, args.output / f"evaluation-{iteration}", rng, training=False)
        history.append(dict(iteration=iteration, policy_loss=policy_loss, policy_teacher=teaching,
                            loss=loss, collection=summary, leaves=leaf_summary, evaluation=evaluation))
    (args.output / "result.json").write_text(json.dumps(dict(initial=before, warm=warm, warm_loss=warm_loss, iterations=history), indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
