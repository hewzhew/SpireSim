"""Offline two-level linear path prior; full-route cost objective, no gameplay policy.

Rust alone encodes facts, admissible choices and group membership. This module
fits weights and exports plain arrays for the same resident Rust search.
"""
import argparse
import hashlib
import json
import time
from pathlib import Path

import numpy as np
import scipy
from scipy import sparse
from scipy.special import expit, logsumexp

SCHEMA = 1
FEATURE_SCHEMA = 2
FEATURE_COUNT = 16384


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def segment_log_softmax(values, segments):
    maxima = np.full(int(segments.max()) + 1, -np.inf)
    np.maximum.at(maxima, segments, values)
    shifted = values - maxima[segments]
    sums = np.bincount(segments, weights=np.exp(shifted), minlength=len(maxima))
    return shifted - np.log(sums[segments])


class DecisionFeatures:
    """Shared runtime probability calculation; independent of evidence labels."""
    def __init__(self, features):
        row_ptr, indices, values = [0], [], []
        action_group, group_decision, offsets = [], [], [0]
        for decision, f in enumerate(features):
            groups, rows = f["groups"], f["rows"]
            if f["schema"] != FEATURE_SCHEMA or not rows or len(rows) != len(groups):
                raise ValueError("invalid decision features")
            if any(type(g) is not int or g < 0 for g in groups) or set(groups) != set(range(max(groups)+1)):
                raise ValueError("non-dense choice groups")
            base = len(group_decision)
            group_decision.extend([decision] * (max(groups)+1))
            action_group.extend(base+g for g in groups)
            offsets.append(len(action_group))
            for row in rows:
                last = -1
                for i,v in row:
                    if type(i) is not int or not last < i < FEATURE_COUNT or not np.isfinite(v):
                        raise ValueError("invalid sparse feature")
                    indices.append(i); values.append(v); last = i
                row_ptr.append(len(indices))
        if not action_group:
            raise ValueError("no decision features")
        self.x = sparse.csr_matrix((values,indices,row_ptr), shape=(len(action_group),FEATURE_COUNT),dtype=np.float64)
        self.action_group = np.asarray(action_group,dtype=np.int64)
        self.group_decision = np.asarray(group_decision,dtype=np.int64)
        self.action_decision = self.group_decision[self.action_group]
        self.action_offsets = np.asarray(offsets,dtype=np.int64)
        sizes = np.bincount(self.action_group)
        average = sparse.csr_matrix((1.0/sizes[self.action_group],
            (self.action_group,np.arange(len(action_group)))),shape=(len(sizes),len(action_group)))
        self.gx = average @ self.x

    def forward(self, weights):
        weights = np.asarray(weights, dtype=np.float64)
        if weights.shape != (2, FEATURE_COUNT) or not np.isfinite(weights).all():
            raise ValueError("invalid weights")
        group_logs = segment_log_softmax(self.gx @ weights[0], self.group_decision)
        member_logs = segment_log_softmax(self.x @ weights[1], self.action_group)
        action_logs = group_logs[self.action_group] + member_logs
        return group_logs, member_logs, action_logs

    def backward(self, action_coeff, group_logs, member_logs):
        group_coeff = np.bincount(self.action_group, weights=action_coeff, minlength=len(group_logs))
        decision_coeff = np.bincount(self.group_decision, weights=group_coeff)
        dg = group_coeff - np.exp(group_logs) * decision_coeff[self.group_decision]
        dm = action_coeff - np.exp(member_logs) * group_coeff[self.action_group]
        return np.stack([self.gx.T @ dg, self.x.T @ dm])


class RouteFeatures(DecisionFeatures):
    """Alternatives stay grouped by exact root/query; duplicate routes add no mass."""
    def __init__(self, records):
        self.records = []
        duplicates, families = {}, {}
        for record in records:
            if record["schema"] != SCHEMA:
                raise ValueError("wrong path feature schema")
            root, family = record["root_id"], record["family"]
            if not root or not family or families.setdefault(root, family) != family:
                raise ValueError("missing or conflicting source family")
            identity = (root, canonical(record["goal"]), record["route_id"])
            content = canonical(record)
            if identity in duplicates:
                if duplicates[identity] != content:
                    raise ValueError("same route identity has different features")
                continue
            duplicates[identity] = content
            self.records.append(record)
        if not self.records:
            raise ValueError("no complete routes; missing routes are not zero loss")
        self.queries = {}
        features, selections = [], []
        route_rows, costs = [], []
        for r, record in enumerate(self.records):
            query = (record["root_id"], canonical(record["goal"]))
            self.queries.setdefault(query, []).append(r)
            decisions = record["decisions"]
            if not decisions:
                raise ValueError("empty route")
            costs.append(len(decisions))  # grammar edges, not raw input count
            for d in decisions:
                selected = d["selected"]
                if type(selected) is not int or not 0 <= selected < len(d["features"]["rows"]):
                    raise ValueError("invalid selected choice")
                route_rows.append(r)
                selections.append(selected)
                features.append(d["features"])
        super().__init__(features)
        selected_actions = self.action_offsets[:-1] + selections
        self.route_selection = sparse.csr_matrix((np.ones(len(route_rows)),
            (route_rows, selected_actions)), shape=(len(self.records), self.x.shape[0]))
        self.decision_route = np.asarray(route_rows, dtype=np.int64)
        self.selected_actions = np.asarray(selected_actions, dtype=np.int64)
        self.log_cost = np.log(np.asarray(costs, dtype=np.float64))
        counts = {}
        for root, _ in self.queries:
            counts[root] = counts.get(root, 0) + 1
        self.query_log_weight = np.asarray([-np.log(len(counts))-np.log(counts[root]) for root, _ in self.queries])


class RouteDataset(RouteFeatures):
    metric = "log_mean_route_bound"
    objective = "log(mean_root mean_query min_known_qualified_route C/P)"

    def __init__(self, records):
        records = list(records)
        for record in records:
            # Legacy exports contain only qualified routes; explicit observed
            # evidence must never be silently accepted through this old entry.
            if (record.get("goal_satisfied", True) is not True or
                record.get("evidence_kind", "known_qualified_complete_route") != "known_qualified_complete_route"):
                raise ValueError("positive learner requires qualified-route evidence")
        super().__init__(records)

    def evaluate(self, weights):
        """Exact gradient of log(mean_root mean_query min_route C/P).

        This is a global monotone log of the declared positive objective, not
        mean log-cost, step CE, or averaging routes from the same query.
        """
        group_logs, member_logs, action_logs = self.forward(weights)
        route_logs = self.route_selection @ action_logs
        log_bounds = self.log_cost - route_logs
        active = np.array([routes[int(np.argmin(log_bounds[routes]))] for routes in self.queries.values()])
        terms = log_bounds[active] + self.query_log_weight
        objective = float(logsumexp(terms))
        route_coeff = np.zeros(len(self.records))
        # Min is nondifferentiable at ties; declared route order chooses one subgradient.
        np.add.at(route_coeff, active, -np.exp(terms-objective))
        action_coeff = self.route_selection.T @ route_coeff
        gradient = self.backward(action_coeff, group_logs, member_logs)
        if not np.isfinite(objective) or not np.isfinite(gradient).all():
            raise ValueError("nonfinite route objective or gradient")
        return objective, gradient, route_logs, active


class PreferenceDataset(RouteFeatures):
    """Whole-route labels supplied by Rust; no outcome ranking is recreated here."""
    metric = "mean_pair_loss"
    objective = "mean_root mean_query mean_pair softplus(relative_cost(better)-relative_cost(worse))"

    def __init__(self, records, pairs):
        super().__init__(records)
        if any(r.get("evidence_kind") != "observed_complete_victory" or
               type(r.get("goal_satisfied")) is not bool for r in self.records):
            raise ValueError("preference learner requires explicit observed-victory evidence")
        lookup = {(r["root_id"], canonical(r["goal"]), r["route_id"]): i
                  for i, r in enumerate(self.records)}
        unique = set()
        for p in pairs:
            query = p["root_id"], canonical(p["goal"])
            try:
                a = lookup[(*query, p["better_route_id"])]
                b = lookup[(*query, p["worse_route_id"])]
            except KeyError as e:
                raise ValueError("pair must reference observed routes at the same root/query") from e
            if a == b or (b, a) in unique:
                raise ValueError("self or contradictory preference")
            unique.add((a, b))
        if not unique:
            raise ValueError("no strict resource preferences")
        self.pairs = np.asarray(sorted(unique), dtype=np.int64)
        counts = {}
        for a, _ in self.pairs:
            r = self.records[a]
            query = r["root_id"], canonical(r["goal"])
            counts[query] = counts.get(query, 0) + 1
        roots = {}
        for root, _ in counts:
            roots[root] = roots.get(root, 0) + 1
        self.pair_weights = np.asarray([
            1 / (len(roots) * roots[self.records[a]["root_id"]] *
                 counts[(self.records[a]["root_id"], canonical(self.records[a]["goal"]))])
            for a, _ in self.pairs])
        # Every exported route must participate; do not silently count or retain
        # unused positive examples as preference training material.
        if set(self.pairs.ravel()) != set(range(len(self.records))):
            raise ValueError("unpaired routes in preference feature index")

    def relative_costs(self, action_logs):
        maxima = np.full(len(self.decision_route), -np.inf)
        np.maximum.at(maxima, self.action_decision, action_logs)
        # First admitted row supplies a deterministic subgradient at argmax ties.
        winner = np.full(len(maxima), len(action_logs), dtype=np.int64)
        matches = np.flatnonzero(action_logs == maxima[self.action_decision])
        np.minimum.at(winner, self.action_decision[matches], matches)
        costs = np.bincount(self.decision_route,
            weights=maxima-action_logs[self.selected_actions], minlength=len(self.records))
        return costs, winner

    def evaluate(self, weights):
        group_logs, member_logs, action_logs = self.forward(weights)
        costs, winner = self.relative_costs(action_logs)
        a, b = self.pairs.T
        margin = costs[a] - costs[b]
        loss = float(self.pair_weights @ np.logaddexp(0, margin))
        pair_coeff = self.pair_weights * expit(margin)
        route_coeff = np.zeros(len(self.records))
        np.add.at(route_coeff, a, pair_coeff)
        np.add.at(route_coeff, b, -pair_coeff)
        action_coeff = np.zeros(len(action_logs))
        np.add.at(action_coeff, winner, route_coeff[self.decision_route])
        np.add.at(action_coeff, self.selected_actions, -route_coeff[self.decision_route])
        gradient = self.backward(action_coeff, group_logs, member_logs)
        if not np.isfinite(loss) or not np.isfinite(gradient).all():
            raise ValueError("nonfinite preference objective or gradient")
        return loss, gradient, costs, winner


def fit(data, steps, learning_rate, initial=None):
    if steps < 1 or not np.isfinite(learning_rate) or learning_rate <= 0:
        raise ValueError("positive training limits required")
    weights = np.zeros((2, FEATURE_COUNT)) if initial is None else np.array(initial, dtype=np.float64, copy=True)
    m, v = np.zeros_like(weights), np.zeros_like(weights)
    best, best_key, history = weights.copy(), None, []
    for step in range(steps + 1):
        loss, gradient, route_logs, active = data.evaluate(weights)
        key = data.checkpoint_key(loss, active) if hasattr(data, "checkpoint_key") else (loss,)
        if best_key is None or key < best_key:
            best, best_key = weights.copy(), key
        if step % 10 == 0 or step == steps:
            row = {"step":step, data.metric:loss, "max_abs_weight":float(np.abs(weights).max())}
            if isinstance(data, RouteDataset):
                row["active_routes"] = active.tolist()
            if hasattr(data,"checkpoint_key"):
                row["mean_sampled_queue_rank"] = float(key[0])
            history.append(row)
        if step == steps:
            break
        m = .9*m + .1*gradient
        v = .999*v + .001*gradient*gradient
        weights -= learning_rate * (m/(1-.9**(step+1))) / (np.sqrt(v/(1-.999**(step+1)))+1e-8)
    return best, history


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--steps", type=int, default=120)
    parser.add_argument("--learning-rate", type=float, default=.01)
    parser.add_argument("--objective", choices=("qualified", "preference", "queue"), default="qualified")
    parser.add_argument("--initial-model", type=Path)
    args = parser.parse_args()
    started = time.perf_counter()
    index = json.loads(args.index.read_text(encoding="utf-8"))
    if args.objective in ("preference","queue"):
        label = "witnessed_queue" if args.objective=="queue" else "strict_materialized_resource_dominance"
        if not isinstance(index, dict) or index.get("schema") != 1 or index.get("label") != label:
            raise ValueError("wrong explicit evidence index")
        files = index["features"]
    else:
        if not isinstance(index, list):
            raise ValueError("expected qualified-route feature index")
        files = index
    records, sources = [], []
    for file in files:
        path = args.index.parent / file
        content = path.read_bytes()
        records.append(json.loads(content))
        sources.append({"file":file, "sha256":hashlib.sha256(content).hexdigest()})
    if args.objective=="queue":
        from queue_learning import QueueDataset
        data=QueueDataset(records)
    else:
        data = PreferenceDataset(records, index["pairs"]) if args.objective == "preference" else RouteDataset(records)
    initial, initialization = None, "all_zero"
    if args.initial_model:
        raw = args.initial_model.read_bytes()
        model = json.loads(raw)
        if model["schema"] != FEATURE_SCHEMA:
            raise ValueError("wrong initial model schema")
        initial = np.asarray([model["group"], model["member"]], dtype=np.float64)
        data.forward(initial)  # validate before creating any training artifacts
        initialization = {"file":str(args.initial_model.resolve()), "sha256":hashlib.sha256(raw).hexdigest()}
    preparation_seconds = time.perf_counter() - started
    args.output.mkdir(parents=True, exist_ok=False)
    started = time.perf_counter()
    weights, history = fit(data, args.steps, args.learning_rate, initial)
    training_seconds = time.perf_counter() - started
    model = {"schema":FEATURE_SCHEMA, "group":weights[0].tolist(), "member":weights[1].tolist()}
    model_bytes = canonical(model).encode("utf-8")
    (args.output/"model.json").write_bytes(model_bytes)
    objective, _, route_logs, active = data.evaluate(weights)
    receipt = {"objective":data.objective,
        "producer_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "input_index":str(args.index.resolve()),
        "input_index_sha256":hashlib.sha256(args.index.read_bytes()).hexdigest(),
        "encoder_producer":json.loads((args.index.parent/"producer.json").read_text(encoding="utf-8")),
        "model_sha256":hashlib.sha256(model_bytes).hexdigest(), "sources":sources,
        "steps":args.steps, "learning_rate":args.learning_rate, "initialization":initialization,
        "checkpoint_rule":"minimum_mean_sampled_queue_rank_then_loss" if args.objective=="queue" else "minimum_training_objective", "history":history,
        data.metric:objective,
        "roots":len({r["root_id"] for r in data.records}), "queries":len(data.queries),
        "examples":len(data.records), "families":sorted({r["family"] for r in data.records}),
        "numpy":np.__version__, "scipy":scipy.__version__,
        "preparation_seconds":preparation_seconds, "training_seconds":training_seconds}
    if args.objective=="queue":
        receipt["queue_learner_sha256"]=hashlib.sha256(Path(__file__).with_name("queue_learning.py").read_bytes()).hexdigest()
        receipt["mean_sampled_queue_rank"]=float(data.checkpoint_key(objective,active)[0])
        receipt["sample_predictions"]=[{"snapshot_id":r["snapshot_id"],"rank":int(rank)} for r,rank in zip(data.records,active)]
        receipt["edge_costs"]=route_logs.tolist()
    else:
        receipt["routes"]=len(data.records)
        receipt["route_predictions"]=[{"root_id":r["root_id"],"goal":r["goal"],"route_id":r["route_id"],
            ("relative_prior_cost" if args.objective=="preference" else "log_probability"):float(lp)} for r,lp in zip(data.records,route_logs)]
    if args.objective=="preference":
        receipt["pairs"] = len(data.pairs)
        receipt["pair_predictions"] = [{"better":int(a), "worse":int(b),
            "cost_difference":float(route_logs[a]-route_logs[b]), "weight":float(w)}
            for (a,b),w in zip(data.pairs,data.pair_weights)]
    elif args.objective=="qualified":
        receipt["active_routes"] = active.tolist()
    (args.output/"training.json").write_text(json.dumps(receipt, indent=2, allow_nan=False)+"\n",encoding="utf-8")
    print(f"{receipt['roots']} roots, {receipt['queries']} queries, {receipt['examples']} examples; "
          f"{data.metric} {history[0][data.metric]:.3f} -> {objective:.3f}")


if __name__ == "__main__":
    main()
