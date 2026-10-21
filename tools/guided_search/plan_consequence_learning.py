"""Learn joint consequences of one concrete Oracle plan, not best-of-action value.

Exact replay remains the authority. Predictions are unconstrained approximations,
not feasible endpoints, policy labels, resource bounds or a deployed search score.
"""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import time

import numpy as np
import torch
from torch import nn

from continuation_learning import hand_layout
from retention_learning import RetentionNetwork, SCHEMA

OBJECTIVE = "guardian_concrete_plan_joint_consequences_v1"


class Dataset:
    def __init__(self, docs):
        if not docs:
            raise ValueError("empty plan collection")
        self.docs = docs
        contract = ("feature_names", "token_names", "target_names", "target_scales", "producer_sha256")
        self.contract = {k: docs[0][k] for k in contract}
        self.names, self.token_names = self.contract["feature_names"], self.contract["token_names"]
        self.target_names, self.scales = self.contract["target_names"], np.asarray(self.contract["target_scales"])
        if len(self.scales) != len(self.target_names) or not np.isfinite(self.scales).all() or (self.scales <= 0).any():
            raise ValueError("invalid target scales")
        self.groups, self.seeds, labels, ids = set(), set(), set(), set()
        self.rows, self.case_indices, self.exact_ms = [], [], 0.
        for d in docs:
            if (d["schema"] != 1 or d["feature_schema"] != SCHEMA or d["objective"] != OBJECTIVE
                    or d["information"] != "full_state_oracle_concrete_plan"
                    or any(d[k] != self.contract[k] for k in contract) or d["case"]["label"] in labels):
                raise ValueError("inconsistent plan consequence contract")
            labels.add(d["case"]["label"])
            self.groups.add(d["case"]["group"])
            self.seeds.add(d["case"]["spec"]["seed"])
            indices = []
            for r in d["rows"]:
                if (r["sample_sha256"] in ids or r["horizon"] not in (1, 2)
                        or not r["inputs"] or not r["plan"] or not r["source_sha256"] or not r["endpoint_sha256"]):
                    raise ValueError("invalid plan provenance or horizon")
                ids.add(r["sample_sha256"])
                for key, width in (("features", len(self.names)), ("targets", len(self.target_names))):
                    if len(r[key]) != width or not np.isfinite(r[key]).all():
                        raise ValueError("invalid consequence feature/target width or value")
                if any(len(t) != len(self.token_names) or not np.isfinite(t).all() for t in r["plan"]):
                    raise ValueError("invalid plan token width or value")
                indices.append(len(self.rows)); self.rows.append(r)
            if indices:
                self.case_indices.append(np.asarray(indices))
            self.exact_ms += d["collection"]["exact_local_replay_ms"]
        if not self.rows:
            raise ValueError("no replayed plan consequences")
        self.x = torch.tensor([r["features"] for r in self.rows], dtype=torch.float32)
        self.y = torch.tensor([r["targets"] for r in self.rows], dtype=torch.float32)
        self.horizon = torch.tensor([[r["horizon"] / 2.] for r in self.rows])
        self.lengths = torch.tensor([len(r["plan"]) for r in self.rows])
        self.plan = torch.zeros(len(self.rows), int(self.lengths.max()), len(self.token_names))
        for i, r in enumerate(self.rows):
            self.plan[i, :len(r["plan"])] = torch.tensor(r["plan"])


def validate_split(train, evaluation):
    if train.groups & evaluation.groups or train.seeds & evaluation.seeds:
        raise ValueError("training and evaluation share a source/seed")
    if train.contract != evaluation.contract:
        raise ValueError("training and evaluation contracts differ")


class PlanNetwork(nn.Module):
    def __init__(self, names, token_width, outputs, mode):
        super().__init__()
        self.mode = mode
        self.source = RetentionNetwork(len(names), hidden=64, hand_layout=hand_layout(names, 16))
        # Remove the unused scalar output rather than train a second objective.
        self.source.output = nn.Identity()
        self.token = nn.Linear(token_width, 32) if mode != "state_only" else None
        self.sequence = nn.GRU(32, 32, batch_first=True) if mode == "ordered" else None
        self.joint = nn.Sequential(nn.Linear(65 + (32 if self.token else 0), 64), nn.ReLU(), nn.Linear(64, outputs))

    def encode_plan(self, plan, lengths):
        z = self.token(plan).relu()
        if self.mode == "ordered":
            states, _ = self.sequence(z)
            return states[torch.arange(len(z)), lengths - 1]
        mask = torch.arange(z.shape[1])[None, :] < lengths[:, None]
        return (z * mask[:, :, None]).sum(1) / 10.

    def forward(self, x, plan, lengths, horizon):
        parts = [self.source.hidden(self.source.project(x)).relu(), horizon]
        if self.token:
            parts.append(self.encode_plan(plan, lengths))
        return self.joint(torch.cat(parts, dim=1))


def predict(model, data, indices=None):
    if indices is None:
        indices = np.arange(len(data.rows))
    return model(data.x[indices], data.plan[indices], data.lengths[indices], data.horizon[indices])


def metrics(data, prediction):
    error = (prediction.detach() - data.y).numpy()
    def case_mean(values, mask=None):
        entries = [values[i if mask is None else i[mask[i]]] for i in data.case_indices]
        entries = [e for e in entries if len(e)]
        return float(np.mean([e.mean() for e in entries])) if entries else None
    result = dict(rows=len(data.rows), cases=len(data.case_indices), groups=len(data.groups), targets={})
    for j, name in enumerate(data.target_names):
        changed = data.y[:, j].numpy() != 0
        result["targets"][name] = dict(case_mean_mae=case_mean(np.abs(error[:, j])) * float(data.scales[j]),
            changed_rows=int(changed.sum()), changed_mae=(case_mean(np.abs(error[:, j]), changed) * float(data.scales[j]) if changed.any() else None))
    result["horizons"] = {str(h): dict(rows=int((data.horizon[:, 0] == h / 2.).sum()),
        normalized_mae=case_mean(np.abs(error).mean(1), data.horizon[:, 0].numpy() == h / 2.)) for h in (1, 2)}
    # Within-source centering asks whether the model explains different actual
    # plans. It creates no preference labels and assumes no optimal alternative.
    groups, ordered = defaultdict(list), defaultdict(list)
    for i, r in enumerate(data.rows):
        key = (r["source_sha256"], r["horizon"])
        groups[key].append(i)
        ordered[key + (tuple(sorted(tuple(t) for t in r["plan"])),)].append(i)
    def contrasts(mapping):
        sets = [i for i in mapping.values() if len(i) > 1 and np.any(np.ptp(data.y[i].numpy(), axis=0) > 1e-6)]
        if not sets:
            return dict(sources=0, rows=0, centered_mae=None)
        centered = [(error[i] - error[i].mean(0)).__abs__().mean(0) for i in sets]
        return dict(sources=len(sets), rows=sum(map(len, sets)),
            centered_mae={n:float(e * scale) for n, e, scale in zip(data.target_names, np.mean(centered, axis=0), data.scales)})
    result["different_plans_same_source"] = contrasts(groups)
    result["same_action_bag_different_order"] = contrasts(ordered)
    return result


def benchmark(model, data):
    with torch.no_grad():
        predict(model, data, np.arange(min(128, len(data.rows))))
        start = time.perf_counter()
        for _ in range(20):
            for offset in range(0, len(data.rows), 128):
                predict(model, data, np.arange(offset, min(offset + 128, len(data.rows))))
        batch_ms = (time.perf_counter() - start) * 1000. / 20
        start = time.perf_counter()
        for i in range(min(128, len(data.rows))):
            predict(model, data, [i])
        single_ms = (time.perf_counter() - start) * 1000. / min(128, len(data.rows))
    return dict(batch_128_total_ms=batch_ms, batch_128_ms_per_plan=batch_ms / len(data.rows),
        single_plan_ms=single_ms, exact_local_replay_ms_per_plan=data.exact_ms / len(data.rows),
        scope="CPU, preencoded inputs; model excludes candidate generation and encoding")


def run(train, evaluation, output, steps=600, seed=1):
    validate_split(train, evaluation)
    seen_decks = {json.dumps(d["case"]["spec"]["deck"]) for d in train.docs}
    subsets = {}
    for label, seen in (("seen_decks", True), ("unseen_decks", False)):
        docs = [d for d in evaluation.docs if (json.dumps(d["case"]["spec"]["deck"]) in seen_decks) == seen]
        if docs:
            subsets[label] = Dataset(docs)
    target_std = train.y.std(0, unbiased=False)
    target_std = torch.where(target_std > 1e-6, target_std, 1.)
    rng = np.random.default_rng(seed)
    batches = [np.asarray([rng.choice(train.case_indices[i]) for i in rng.integers(len(train.case_indices), size=128)]) for _ in range(steps)]
    receipt = dict(objective=OBJECTIVE, information="full_state_oracle_concrete_plan", steps=steps, seed=seed,
        target="one executed endpoint per plan; no best-of mixing or optimality claim", models={})
    for mode in ("state_only", "bag", "ordered"):
        torch.manual_seed(seed)
        model = PlanNetwork(train.names, len(train.token_names), len(train.target_names), mode)
        optimizer = torch.optim.Adam(model.parameters(), lr=.001, weight_decay=1e-4)
        start = time.perf_counter()
        for indices in batches:
            loss = ((predict(model, train, indices) - train.y[indices]) / target_std).square().mean()
            optimizer.zero_grad(set_to_none=True); loss.backward(); optimizer.step()
        training_seconds = time.perf_counter() - start
        model.eval()
        with torch.no_grad():
            result = dict(parameter_count=sum(p.numel() for p in model.parameters()), training_seconds=training_seconds,
                train=metrics(train, predict(model, train)), evaluation=metrics(evaluation, predict(model, evaluation)),
                timing=benchmark(model, evaluation))
            for label, data in subsets.items():
                result["evaluation_" + label] = metrics(data, predict(model, data))
        receipt["models"][mode] = result
        torch.save(dict(mode=mode, contract=train.contract, state_dict=model.state_dict()), output / f"{mode}.pt")
    receipt["loss"] = dict(kind="MSE scaled by training target standard deviations; constant targets use 1",
                           standard_deviations=target_std.tolist())
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--train", type=Path, nargs="+", required=True)
    parser.add_argument("--eval", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--steps", type=int, default=600)
    args = parser.parse_args()
    if args.output.exists() or args.steps <= 0:
        parser.error("fresh output directory and positive steps required")
    torch.set_num_threads(1)
    docs = [[json.loads(p.read_bytes()) for p in paths] for paths in (args.train, args.eval)]
    train, evaluation = [Dataset(d) for d in docs]
    validate_split(train, evaluation)
    args.output.mkdir(parents=True)
    receipt = run(train, evaluation, args.output, steps=args.steps)
    receipt["trainer_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    receipt["inputs"] = {str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in args.train + args.eval}
    (args.output / "fit.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({mode: dict(hp_mae=r["evaluation"]["targets"]["hp_delta"]["case_mean_mae"],
        strength_mae=r["evaluation"]["targets"]["strength_delta"]["case_mean_mae"],
        carnage_changed_mae=r["evaluation"]["targets"]["exhaust_delta/Carnage"]["changed_mae"],
        training_seconds=r["training_seconds"], timing=r["timing"])
        for mode,r in receipt["models"].items()}, indent=2))


if __name__ == "__main__":
    main()
