"""Experimental resource intervals, not optimal-action labels or a search policy.

For a fixed original-potion condition, predict capped attainable terminal HP.
Zero denotes no qualifying victory; a full [0, cap] interval denotes Unknown.
Only independently replayed terminals raise positive lower bounds. Upper bounds
use observed simulator endpoints and complete admitted coverage, not predictions.
"""
import hashlib
import json
import time

import numpy as np
import torch
from torch import nn

from distance_learning import from_features, KIND_COUNT
from entity_learning import EntityNetwork, Position, pack

SCHEMA = 1


def _positive_int(value):
    return type(value) is int and value > 0


def hp_intervals(data, cap):
    """Bounds for exactly this graph's potion condition; no alternate mask input.

    Cap is an output threshold range, not an upper bound on game HP. No retained
    plan selection, distance lower/upper value, or current-player HP is consulted.
    This validates the transport contract, not the external producer's identity.
    """
    if not _positive_int(cap) or data.get("schema") != 1:
        raise ValueError("invalid resource range or graph schema")
    goal, rows = data["goal"], data["rows"]
    keep = goal["preserve_starting_potion_slots"]
    if (not rows or not _positive_int(goal["min_final_hp"]) or
            type(keep) is not int or not 0 <= keep < 2**64):
        raise ValueError("invalid fixed resource query")
    lower = np.zeros(len(rows), dtype=np.int64)
    upper = np.full(len(rows), cap, dtype=np.int64)
    reached = {0}
    closed = np.zeros(len(rows), dtype=bool)
    for i, row in enumerate(rows):
        e = row.get("outcome_evidence")
        if e is None:
            raise ValueError("missing explicit outcome evidence is not a negative label")
        t, o = e["combat_terminal"], e["observed_terminal_outcome"]
        spent, violated = e["spent_starting_potion_slots"], e["required_original_spent"]
        checked, satisfies = e["terminal_replay_verified"], e["terminal_satisfies_query"]
        if (type(row["complete"]) is not bool or type(checked) is not bool or
                type(violated) is not bool or type(e["combat_smoked"]) is not bool or
                type(spent) is not int or not 0 <= spent < 2**64 or violated != bool(keep & spent) or
                (t is not None and type(t) is not bool) or ((o is None) != (t is None))):
            raise ValueError("inconsistent terminal, coverage or original-potion evidence")
        if e["combat_smoked"] and t is not False:
            raise ValueError("Smoke escape is not an open state or victory")
        if t is None:
            if satisfies is not None or checked:
                raise ValueError("open state cannot carry terminal acceptance or replay")
        else:
            hp = o["final_hp"]
            if (type(hp) is not int or o["starting_potion_expenditure_slots"] != spent or
                    type(satisfies) is not bool or
                    satisfies != (t and hp >= goal["min_final_hp"] and not violated)):
                raise ValueError("terminal resources do not match fixed query")
            if t and hp <= 0:
                raise ValueError("victory requires positive final HP")
            if checked and (not satisfies or row["witnessed_remaining_inputs"] != 0):
                raise ValueError("terminal replay marker lacks qualified terminal witness")
        children = row["children"]
        for edge in children:
            if (len(edge) != 2 or type(edge[0]) is not int or type(edge[1]) is not int or
                    not i < edge[0] < len(rows) or edge[1] not in (0, 1) or
                    edge[0] in reached or rows[edge[0]]["parent"] != i):
                raise ValueError("invalid source-bound successor tree")
            reached.add(edge[0])
        if i == 0 and row["parent"] is not None:
            raise ValueError("graph root has a parent")
        closed[i] = t is not None or violated
        if closed[i]:
            if children:
                raise ValueError("terminal or query-cutoff node has children")
            upper[i] = min(cap, o["final_hp"]) if t is True and not violated else 0
            lower[i] = upper[i] if checked and not violated else 0
        elif row["complete"]:
            features = row["features"]
            if features is None or not children or len(children) != len(features["actions"]):
                raise ValueError("complete open source lacks full admitted successor coverage")
    if len(reached) != len(rows):
        raise ValueError("disconnected successor node")
    for i in range(len(rows)-1, -1, -1):
        if closed[i] or not rows[i]["children"]:
            continue
        children = [j for j, _ in rows[i]["children"]]
        lower[i] = max(lower[children])
        if rows[i]["complete"]:
            upper[i] = max(upper[children])
        if lower[i] > upper[i]:
            raise ValueError("contradictory resource interval")
    return np.column_stack((lower, upper))


def state_input(features):
    """Shared offline/inference transform on a copy; outcome evidence is absent.

    Resource entity16 field0 is the collector's minimum-HP query. Its other facts
    and original-bottle rows are kept. HP thresholds are output coordinates here.
    """
    original = from_features(features)
    columns = {name: np.array(value, copy=True) for name, value in original.columns.items()}
    ids = columns["entity_ids"]
    query = np.flatnonzero((ids[:, 0] == 16) & (ids[:, 1] == 0))
    if len(query) != 1:
        raise ValueError("resource model requires exactly one resource context")
    columns["entity_values"][query[0], 0] = 0
    for value in columns.values():
        value.setflags(write=False)
    return Position(columns)


class ResourceDataset:
    def __init__(self, documents, cap):
        self.cap, self.items, self.coordinates, self.query_indices = cap, [], [], []
        self.graph_bounds, self.queries, intervals, identities = [], [], [], {}
        families = {}
        for document in documents:
            group, graph = document["group"], document["data"]
            if not isinstance(group, str) or not group:
                raise ValueError("missing source family")
            root = json.dumps(graph["root"], sort_keys=True, separators=(",", ":"))
            if families.setdefault(root, group) != group:
                raise ValueError("one root assigned to different families")
            key = (root, graph["goal"]["preserve_starting_potion_slots"])
            if key in identities:
                raise ValueError("duplicate root/potion query; combine evidence explicitly")
            q = len(self.queries); identities[key] = q
            bounds = hp_intervals(graph, cap)
            self.graph_bounds.append(bounds)
            self.queries.append(dict(group=group, key=key))
            for node, row in enumerate(graph["rows"]):
                if row["features"] is not None:
                    self.items.append(state_input(row["features"]))
                    self.coordinates.append((q, node))
                    self.query_indices.append(q)
                    intervals.append(bounds[node])
        if not self.items:
            raise ValueError("resource dataset needs actual prepared states")
        self.intervals = np.asarray(intervals, dtype=np.int64)
        self.query_indices = np.asarray(self.query_indices, dtype=np.int64)
        self.eligible = (self.intervals[:, 0] > 0) | (self.intervals[:, 1] < cap)
        self.pools = [np.flatnonzero(self.eligible & (self.query_indices == q))
                      for q in range(len(self.queries))]


class ResourceNetwork(EntityNetwork):
    def __init__(self, cap, hidden=32, layers=1):
        if not _positive_int(cap):
            raise ValueError("invalid resource range")
        super().__init__(hidden, layers, kind_count=KIND_COUNT)
        del self.policy, self.action_kind, self.value
        self.cap = cap
        self.resources = nn.Linear(hidden, cap+1)

    def forward(self, data):
        return self.resources(self.encode_context(data)[:, 0])

    def tails(self, data):
        # Column q-1 estimates attainability of final HP >= q. Monotone by shape,
        # not a calibrated success probability or an exact feasibility bound.
        mass = self(data).softmax(-1)
        return mass[:, 1:].flip(-1).cumsum(-1).flip(-1).clamp(max=1)


def model_revision(model):
    digest = hashlib.sha256(f"resource-interval-v{SCHEMA}:cap{model.cap}".encode())
    for name, tensor in sorted(model.state_dict().items()):
        value = tensor.detach().cpu().contiguous()
        digest.update(name.encode())
        digest.update(str((value.dtype, tuple(value.shape))).encode())
        digest.update(value.numpy().tobytes())
    return digest.hexdigest()


def interval_loss(logits, bounds):
    """Constraint surrogate: place mass anywhere in the factual interval.

    It neither regresses to the best witnessed route nor assumes an unbiased
    censoring process. Unknown contributes exactly zero loss and gradient.
    """
    if (logits.ndim != 2 or logits.shape[1] < 2 or bounds.shape != (len(logits), 2) or
            bounds.dtype not in (torch.int32, torch.int64) or
            not torch.isfinite(logits).all() or
            (bounds[:, 0] < 0).any() or (bounds[:, 0] > bounds[:, 1]).any() or
            (bounds[:, 1] >= logits.shape[1]).any()):
        raise ValueError("invalid resource logits or bounds")
    axis = torch.arange(logits.shape[1], device=logits.device)
    mask = (axis >= bounds[:, :1]) & (axis <= bounds[:, 1:])
    loss = -torch.logsumexp(logits.log_softmax(-1).masked_fill(~mask, -torch.inf), -1)
    unknown = (bounds[:, 0] == 0) & (bounds[:, 1] == logits.shape[1]-1)
    return torch.where(unknown, torch.zeros_like(loss), loss.clamp_min(0))


def fit(model, dataset, *, steps=600, batch_size=64, learning_rate=0.001, seed=1103, device="cpu"):
    """Fixed schedule, uniform queries then eligible rows; no bootstrap targets."""
    if (model.cap != dataset.cap or not _positive_int(steps) or not _positive_int(batch_size) or
            not np.isfinite(learning_rate) or learning_rate <= 0 or
            type(seed) is not int or seed < 0):
        raise ValueError("invalid resource fitting schedule")
    pools = [p for p in dataset.pools if len(p)]
    if not pools:
        raise ValueError("no factual intervals; Unknown is not a negative training example")
    rng = np.random.default_rng(seed)
    optimizer = torch.optim.Adam(model.parameters(), lr=learning_rate)
    history=[]; seen=set(); started=time.perf_counter()
    model.to(device).train()
    for step in range(steps):
        indices = [int(rng.choice(pools[int(q)])) for q in rng.integers(len(pools), size=batch_size)]
        data = pack([dataset.items[i] for i in indices], device, kind_count=KIND_COUNT)
        bounds = torch.as_tensor(dataset.intervals[indices], device=device)
        optimizer.zero_grad(set_to_none=True)
        loss = interval_loss(model(data), bounds).mean()
        loss.backward()
        nn.utils.clip_grad_norm_(model.parameters(), 1.0, error_if_nonfinite=True)
        optimizer.step(); seen.update(indices)
        if step == 0 or (step+1) % 50 == 0 or step+1 == steps:
            history.append(dict(step=step+1, batch_loss=float(loss.detach()), unique_rows=len(seen)))
    model.eval()
    return dict(steps=steps,batch_size=batch_size,queries_with_facts=len(pools),
                eligible_rows=int(dataset.eligible.sum()),unique_rows_seen=len(seen),
                seconds=time.perf_counter()-started,history=history)
