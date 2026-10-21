"""Opt-in local distance heuristic. Predictions order Rust search, never play cards.

Factual intervals and full immediate-successor coverage come from Rust. Frozen
successor predictions form approximate targets, kept apart from those facts.
"""
import copy
import hashlib
import json
import time

import numpy as np
import torch
from torch import nn

from entity_learning import EntityNetwork, Position, pack, positions, COLUMNS, KINDS, _validate

SCHEMA, KIND_COUNT = 1, KINDS


def model_revision(model):
    digest = hashlib.sha256()
    digest.update(f"distance-v{SCHEMA}:H{model.horizon}".encode())
    for name, tensor in sorted(model.state_dict().items()):
        value = tensor.detach().cpu().contiguous()
        digest.update(name.encode())
        digest.update(str((value.dtype, tuple(value.shape))).encode())
        digest.update(value.numpy().tobytes())
    return digest.hexdigest()


class DistanceNetwork(EntityNetwork):
    def __init__(self, horizon, hidden=32, layers=1):
        if type(horizon) is not int or not 0 < horizon < 2**32-1:
            raise ValueError("invalid distance horizon")
        super().__init__(hidden, layers, kind_count=KIND_COUNT)
        del self.policy
        del self.action_kind
        self.horizon = horizon
        # Initial optimism is a training initialization, never solvability evidence.
        nn.init.constant_(self.value.bias, -2.0)

    def forward(self, data):
        return (self.horizon+1)*self.value(self.encode_context(data)[:, 0]).squeeze(-1).sigmoid()


def from_features(features):
    mapping = {"entity_ids":"ids", "entity_values":"values", "entity_refs":"refs",
               "action_ids":"action_ids", "actions":"actions", "action_refs":"action_refs"}
    c = {name: np.asarray(features[mapping[name]], dtype=np.int64 if integer else np.float32).reshape(-1,width)
         for name,(width,integer) in COLUMNS.items()}
    _validate(c, kind_count=KIND_COUNT)
    if not len(c["actions"]):
        raise ValueError("unresolved distance features need legal choices")
    return Position(c)


def prediction_batch(model, items, device="cpu", batch_size=32):
    output = []
    with torch.no_grad():
        for start in range(0,len(items),batch_size):
            output.extend(model(pack(items[start:start+batch_size],device,kind_count=KIND_COUNT))
                          .detach().cpu().double().tolist())
    return np.asarray(output,dtype=np.float64)


class DistanceDataset:
    def __init__(self, documents):
        self.rows, self.items, self.item_rows, self.groups = [], [], [], []
        horizons = set()
        sources = {}
        for document in documents:
            group, data = document["group"], document["data"]
            if not isinstance(group,str) or not group or data["schema"] != SCHEMA:
                raise ValueError("missing source group or invalid distance schema")
            root = json.dumps(data["root"],sort_keys=True,separators=(",",":"))
            if sources.setdefault(root,group) != group:
                raise ValueError("same root assigned to different source groups")
            h = data["horizon"]
            if type(h) is not int or not 0 < h < 2**32-1:
                raise ValueError("invalid horizon")
            horizons.add(h)
            base = len(self.rows)
            local = data["rows"]
            for i, row in enumerate(local):
                lo,hi = row["lower"],row["upper"]
                if type(lo) is not int or type(hi) is not int or not 0 <= lo <= hi <= h+1:
                    raise ValueError("invalid factual distance interval")
                if type(row["complete"]) is not bool:
                    raise ValueError("invalid successor coverage")
                children = row["children"]
                if len({j for j,c in children}) != len(children) or any(
                    type(j) is not int or type(c) is not int or not i < j < len(local) or c not in (0,1)
                    for j,c in children):
                    raise ValueError("invalid forward successor edges")
                if row["complete"] and not children and lo != hi:
                    raise ValueError("unresolved complete source without successors")
                self.rows.append(dict(row, children=[(base+j,c) for j,c in children]))
                self.groups.append(group)
                if row["features"] is not None:
                    self.items.append(from_features(row["features"]))
                    self.item_rows.append(base+i)
        if len(horizons) != 1 or not self.items:
            raise ValueError("require one horizon and nonempty state features")
        self.horizon = horizons.pop()
        self.item_rows = np.asarray(self.item_rows,dtype=np.int64)

    def values(self, model, device="cpu", batch_size=32):
        if model.horizon != self.horizon:
            raise ValueError("model/data range mismatch")
        result = np.full(len(self.rows),np.nan)
        result[self.item_rows] = prediction_batch(model,self.items,device,batch_size)
        for i,r in enumerate(self.rows):
            if r["lower"] == r["upper"]:
                result[i] = r["lower"]
            elif np.isfinite(result[i]):
                result[i] = np.clip(result[i],r["lower"],r["upper"])
        return result

    def targets(self, values, *, bootstrap=True):
        """Point targets only for facts/bootstrap; intervals carry no point target."""
        if type(bootstrap) is not bool:
            raise ValueError("bootstrap must be bool")
        values = np.asarray(values,dtype=np.float64)
        if values.shape != (len(self.rows),) or np.isinf(values).any():
            raise ValueError("invalid target predictions")
        finite = values[np.isfinite(values)]
        if np.any(finite < 0) or np.any(finite > self.horizon+1):
            raise ValueError("out-of-range target predictions")
        targets = np.full(len(self.rows),np.nan)
        kinds = ["unknown"]*len(self.rows)
        for i,r in enumerate(self.rows):
            lo,hi = r["lower"],r["upper"]
            if lo == hi:
                targets[i],kinds[i] = lo,"fact"
            elif bootstrap and r["complete"] and r["children"] and all(np.isfinite(values[j]) for j,c in r["children"]):
                targets[i] = np.clip(min(c+np.clip(values[j],self.rows[j]["lower"],self.rows[j]["upper"])
                                          for j,c in r["children"]),lo,hi)
                kinds[i] = "bootstrap"
            elif lo > 0 or hi < self.horizon+1:
                kinds[i] = "interval_only"
        return targets,kinds


def _target_losses(prediction, targets, lower, upper, interval_only):
    # A current prediction anywhere inside a factual interval has zero loss.
    # Detach the projection so predictions outside it are pulled to the boundary.
    truth = torch.where(interval_only, prediction.detach().clamp(min=lower,max=upper), targets)
    return nn.functional.huber_loss(prediction,truth,reduction="none")


def _fact_strata(dataset, selected):
    """Equal source, then present fact class, then occurrence; no outcome pricing."""
    strata = []
    counts = {}
    for i in selected:
        row = dataset.item_rows[i]
        fact = dataset.rows[row]
        kind = ("capped" if fact["lower"] == dataset.horizon+1 else
                "finite_upper" if fact["upper"] < dataset.horizon+1 else "lower_only")
        if fact["lower"] == 0 and fact["upper"] == dataset.horizon+1:
            raise ValueError("unknown interval has no factual sampling stratum")
        key = (dataset.groups[row],kind)
        strata.append(key)
        counts[key] = counts.get(key,0)+1
    groups = {}
    for group,kind in counts:
        groups[group] = groups.get(group,0)+1
    probabilities = np.asarray([1/(len(groups)*groups[g]*counts[g,k]) for g,k in strata])
    return strata,probabilities


def fit(model, dataset, *, updates=4, epochs=2, steps_per_update=None, batch_size=32,
        learning_rate=0.001, seed=0, device="cpu", bootstrap=True, sampling="uniform"):
    """Fixed update schedule, no outcome-based checkpoint selection."""
    positive_int = lambda value: type(value) is int and value > 0
    if (type(bootstrap) is not bool or not positive_int(updates) or not positive_int(batch_size)
            or not np.isfinite(learning_rate) or learning_rate <= 0
            or (steps_per_update is None and not positive_int(epochs))
            or (steps_per_update is not None and (epochs is not None or not positive_int(steps_per_update)))):
        raise ValueError("invalid fitting schedule")
    if sampling not in ("uniform","source_fact") or (sampling == "source_fact" and steps_per_update is None):
        raise ValueError("source_fact sampling requires fixed-step fitting")
    if model.horizon != dataset.horizon:
        raise ValueError("model/data range mismatch")
    model.to(device)
    optimizer = torch.optim.Adam(model.parameters(),lr=learning_rate)
    generator = np.random.default_rng(seed)
    history = []
    for update in range(updates):
        if bootstrap:
            target = copy.deepcopy(model).eval()
            target_revision = model_revision(target)
            estimates = dataset.values(target,device,batch_size)
        else:
            target_revision = None
            estimates = np.full(len(dataset.rows),np.nan)
        targets,kinds = dataset.targets(estimates,bootstrap=bootstrap)
        # Source/fact sampling is defined by evidence, independent of whether a
        # complete source also has a frozen-successor target. A guessed target
        # alone never gives an unconstrained row a factual sampling stratum.
        selected = [i for i,row in enumerate(dataset.item_rows) if kinds[row] != "unknown"
                    and (sampling != "source_fact" or dataset.rows[row]["lower"] > 0
                         or dataset.rows[row]["upper"] < dataset.horizon+1)]
        counts = {}
        for i in selected:
            g = dataset.groups[dataset.item_rows[i]]
            counts[g] = counts.get(g,0)+1
        if not selected:
            raise ValueError("no usable targets; missing evidence is not zero loss")
        strata,probabilities = _fact_strata(dataset,selected) if sampling == "source_fact" else (None,None)
        losses, fitted = [], []
        model.train()
        def batches():
            if steps_per_update is not None:
                # Equal optimizer work across differently sized data sets. Draws
                # use replacement; the same source-group weights still apply.
                for _ in range(steps_per_update):
                    yield generator.choice(selected,size=batch_size,replace=True,p=probabilities)
            else:
                for _ in range(epochs):
                    order = generator.permutation(selected)
                    for start in range(0,len(order),batch_size):
                        yield order[start:start+batch_size]
        for batch in batches():
            row_ids = dataset.item_rows[batch]
            prediction = model(pack([dataset.items[i] for i in batch],device,kind_count=KIND_COUNT))
            truth = torch.tensor(targets[row_ids],device=device,dtype=prediction.dtype)
            lower = torch.tensor([dataset.rows[r]["lower"] for r in row_ids],device=device,dtype=prediction.dtype)
            upper = torch.tensor([dataset.rows[r]["upper"] for r in row_ids],device=device,dtype=prediction.dtype)
            interval_only = torch.tensor([kinds[r] == "interval_only" for r in row_ids],device=device)
            weights = (torch.ones_like(prediction) if sampling == "source_fact" else
                       torch.tensor([len(selected)/(len(counts)*counts[dataset.groups[r]]) for r in row_ids],
                                    device=device,dtype=prediction.dtype))
            loss = (_target_losses(prediction,truth,lower,upper,interval_only)*weights).mean()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(),1.0)
            optimizer.step()
            losses.append(float(loss.detach()))
            fitted.extend(int(i) for i in batch)
        model.eval()
        unique_fitted = set(fitted)
        record = {"update":update,"target_revision":target_revision,"bootstrap":bootstrap,"sampling":sampling,
                        "mean_fit_loss":float(np.mean(losses)),
                        "target_kinds":{k:kinds.count(k) for k in sorted(set(kinds))},
                        "eligible_feature_states":len(selected),"optimizer_steps":len(losses),
                        "fit_examples":len(fitted),"fit_unique_states":len(unique_fitted),
                        "fit_target_kinds":{k:sum(kinds[dataset.item_rows[i]] == k for i in unique_fitted)
                                            for k in sorted({kinds[dataset.item_rows[i]] for i in unique_fitted})}}
        if strata is not None:
            by_item = dict(zip(selected,strata))
            record["fact_sampling"] = [dict(group=g,kind=k,eligible=sum(s == (g,k) for s in strata),
                expected_probability=float(sum(p for s,p in zip(strata,probabilities) if s == (g,k))),
                draws=sum(by_item[i] == (g,k) for i in fitted)) for g,k in sorted(set(strata))]
        history.append(record)
    return history


def solve(task, model, *, horizon, work=10000, max_nodes=2000, max_depth=128,
          seconds=10.0, batch_size=32, device="cpu", export_data=False, lookahead_inputs=0,
          advance_forced=False, probe_immediate=False, ordering="total_inputs",
          probe_forced_boundaries=False, estimate_probe_boundaries=False, include_outcomes=False):
    from sts_learning_bridge._native import DistanceCombatSearch
    if type(include_outcomes) is not bool or (include_outcomes and not export_data):
        raise ValueError("outcome evidence requires explicit data export")
    if work < 1 or max_nodes < 1 or max_depth < 1 or not np.isfinite(seconds) or seconds <= 0:
        raise ValueError("invalid search budget")
    if model is not None and model.horizon != horizon:
        raise ValueError("model/search range mismatch")
    if model is not None:
        model.eval()
    revision = "distance-neutral-zero" if model is None else model_revision(model)
    setup = time.perf_counter()
    env = DistanceCombatSearch(json.dumps(task),horizon,revision,batch_size,lookahead_inputs,advance_forced,probe_immediate,ordering,probe_forced_boundaries,estimate_probe_boundaries)
    setup_seconds = time.perf_counter()-setup
    started = time.perf_counter()
    prediction_seconds, transfer_seconds = 0.,0.
    # The caller carries one deadline across every native advance and inference.
    while True:
        summary = json.loads(env.report_json(False))
        stats = summary["stats"]
        used = sum(stats[k] for k in ("prepared","transitions","replay_attempts","evaluated_states"))
        remaining = seconds-(time.perf_counter()-started)
        if remaining <= 0: stop = "Deadline"; break
        if used >= work: stop = "WorkBudget"; break
        stop = env.advance(work-used,max_nodes,max_depth,remaining)
        if stop != "NeedsEvaluation": break
        begin = time.perf_counter()
        frame = env.evaluations()
        items = positions(frame,kind_count=KIND_COUNT)
        transfer_seconds += time.perf_counter()-begin
        summary = json.loads(env.report_json(False))
        stats = summary["stats"]
        used = sum(stats[k] for k in ("prepared","transitions","replay_attempts","evaluated_states"))
        if used+len(items) > work: stop = "WorkBudget"; break
        if time.perf_counter()-started >= seconds: stop = "Deadline"; break
        begin = time.perf_counter()
        values = np.zeros(len(items)) if model is None else prediction_batch(model,items,device,batch_size)
        prediction_seconds += time.perf_counter()-begin
        # Inference can overrun by one batch; preserve that actual cost, then stop.
        env.accept_estimates(frame["token"],revision,values.tolist())
    elapsed = time.perf_counter()-started
    summary = json.loads(env.report_json())
    if model is not None and model_revision(model) != revision:
        raise ValueError("model changed during search")
    summary.update(task_input=task, stop=stop, search_seconds=elapsed, setup_seconds=setup_seconds,
                   prediction_seconds=prediction_seconds, numeric_transfer_seconds=transfer_seconds,
                   limits=dict(horizon=horizon,work=work,max_nodes=max_nodes,max_depth=max_depth,
                               seconds=seconds,batch_size=batch_size,lookahead_inputs=lookahead_inputs,
                               advance_forced=advance_forced,probe_immediate=probe_immediate,ordering=ordering,
                               probe_forced_boundaries=probe_forced_boundaries,
                               estimate_probe_boundaries=estimate_probe_boundaries))
    export_started = time.perf_counter()
    data = (json.loads(env.data_json(include_outcomes=True) if include_outcomes else env.data_json())
            if export_data else None)
    if include_outcomes:
        summary["outcome_export_seconds"] = time.perf_counter()-export_started
    return summary,data
