"""Learn the return of a frozen, actually executed continuation executor.

This point-regression task does not use optimal-value labels, intervals,
Bellman max backups or action preferences. A censored trajectory supplies no target.
The exported model reuses the scoped Rust evaluator and its hand encoder.
"""
import argparse
import hashlib
import json
from pathlib import Path
import time

import numpy as np
import torch
from torch import nn

from retention_learning import RetentionNetwork, SCHEMA

OBJECTIVE = "guardian_retained_tactical_argmax_return_v1"
SEARCH_OBJECTIVE = "guardian_solver_plus_tactical_return_v1"
WITNESS_OBJECTIVE = "guardian_solver_plus_replayed_witness_return_v1"


class Dataset:
    def __init__(self, documents):
        if not documents:
            raise ValueError("empty continuation corpus")
        self.names = documents[0]["feature_names"]
        self.objective = documents[0]["objective"]
        self.continuation = documents[0]["continuation"]
        self.groups, self.seeds, self.labels, self.censored = set(), set(), set(), 0
        self.case_indices, x, y = [], [], []
        for doc in documents:
            case = doc["case"]
            if (doc["schema"] != 1 or doc["feature_schema"] != SCHEMA
                    or self.objective not in (OBJECTIVE, SEARCH_OBJECTIVE, WITNESS_OBJECTIVE)
                    or doc["objective"] != self.objective or doc["feature_names"] != self.names
                    or doc["continuation"] != self.continuation
                    or case["label"] in self.labels or not case["group"].strip()):
                raise ValueError("inconsistent continuation contract or duplicate case")
            self.groups.add(case["group"])
            self.seeds.add(case["spec"]["seed"])
            self.labels.add(case["label"])
            traces = doc["traces"]
            for trace in traces:
                value = trace["terminal_return"]
                if value is not None and (not np.isfinite(value) or not (value == -1 or 0 <= value <= 1)):
                    raise ValueError("invalid actual terminal return")
                if self.objective in (SEARCH_OBJECTIVE, WITNESS_OBJECTIVE):
                    control, search = trace.get("tactical_control"), trace.get("search")
                    if control is None or search is None or search["stop"] == "TimeBudget":
                        raise ValueError("missing fixed-work search/control evidence")
                    old = control["terminal_return"]
                    if old is not None and (not np.isfinite(old) or not (old == -1 or 0 <= old <= 1)):
                        raise ValueError("invalid actual control return")
                    if old is not None and (value is None or value < old - 1e-6):
                        raise ValueError("adopted search return is worse than its executed control")
                    if self.objective == WITNESS_OBJECTIVE:
                        witness = trace.get("witness_control")
                        chosen = search.get("selected_witness_control")
                        if type(chosen) is not bool or (chosen and (witness is None or search.get("selected_solver_plan"))):
                            raise ValueError("missing or contradictory witness selection evidence")
                        if witness is not None:
                            won = witness["terminal_return"]
                            if (not isinstance(witness.get("source_prefix"), list)
                                    or not isinstance(witness.get("inputs"), list)
                                    or won is None or not np.isfinite(won) or not 0 <= won <= 1):
                                raise ValueError("invalid independently replayed winning control")
                            if value is None or value < won - 1e-6:
                                raise ValueError("adopted return is worse than its replayed winning control")
                            if chosen and abs(value - won) > 1e-6:
                                raise ValueError("chosen witness return does not match actual target")
            indices = []
            for row in doc["rows"]:
                i, offset = row["trace"], row["offset"]
                if (type(i) is not int or not 0 <= i < len(traces)
                        or type(offset) is not int or not 0 <= offset <= len(traces[i]["inputs"])):
                    raise ValueError("invalid continuation row provenance")
                features = row["features"]
                if len(features) != len(self.names) or not np.isfinite(features).all():
                    raise ValueError("invalid continuation features")
                target = traces[i]["terminal_return"]
                if self.objective in (SEARCH_OBJECTIVE, WITNESS_OBJECTIVE) and offset != 0:
                    raise ValueError("search continuation requires an independent source per target")
                if target is not None and offset == len(traces[i]["inputs"]):
                    raise ValueError("terminal endpoint is not a continuation decision row")
                if target is None:
                    self.censored += 1
                    continue
                indices.append(len(x))
                x.append(features)
                y.append(target)
            if indices:
                self.case_indices.append(np.asarray(indices))
        if not x:
            raise ValueError("no completed continuation targets")
        self.x = torch.tensor(x, dtype=torch.float32)
        self.y = torch.tensor(y, dtype=torch.float32)
        self.ceilings = self.x[:, self.names.index("terminal_hp_ceiling")]
        if torch.any(self.y > self.ceilings + 1e-6):
            raise ValueError("actual return exceeds scoped resource ceiling")


def validate_split(train, evaluation):
    if train.groups & evaluation.groups or train.seeds & evaluation.seeds:
        raise ValueError("training and evaluation share a source/seed group")
    if train.objective != evaluation.objective or train.names != evaluation.names or train.continuation != evaluation.continuation:
        raise ValueError("training and evaluation continuation contracts differ")


def hand_layout(names, hidden):
    start = names.index("hand/0/present")
    width, remainder = divmod(len(names) - start, 10)
    suffix = [n.removeprefix("hand/0/") for n in names[start:start + width]]
    if remainder or not width or names[start:] != [f"hand/{i}/{n}" for i in range(10) for n in suffix]:
        raise ValueError("invalid shared hand feature layout")
    return start, width, hidden


def fit(data, *, seed=1, steps=1200):
    torch.manual_seed(seed)
    network = RetentionNetwork(len(data.names), hidden=64, hand_layout=hand_layout(data.names, 16))
    linear = nn.Linear(len(data.names), 1)
    optimizers = [torch.optim.Adam(m.parameters(), lr=.001, weight_decay=1e-4) for m in (network, linear)]
    rng = np.random.default_rng(seed)
    for _ in range(steps):
        indices = np.asarray([rng.choice(data.case_indices[i])
                              for i in rng.integers(len(data.case_indices), size=128)])
        for model, optimizer in zip((network, linear), optimizers):
            prediction = model(data.x[indices]).reshape(-1)
            loss = nn.functional.mse_loss(prediction, data.y[indices])
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()
    return network, linear


def metrics(data, prediction):
    prediction = prediction.detach().reshape(-1).clamp(min=-1).minimum(data.ceilings)
    error = prediction - data.y
    def mean(values):
        return float(torch.stack([values[i].mean() for i in data.case_indices]).mean())
    wins = data.y >= 0
    # The simulator counter starts at zero: public turns 1..3 are counts 0..2.
    early = data.x[:, data.names.index("turn")] < 3. / 20. if "turn" in data.names else torch.ones(len(data.y), dtype=torch.bool)
    early_cases = [i[early[i].numpy()] for i in data.case_indices]
    early_cases = [i for i in early_cases if len(i)]
    result = dict(rows=len(data.y), cases=len(data.case_indices), groups=len(data.groups),
                censored_rows=data.censored, winning_rows=int(wins.sum()),
                case_mean_mae=mean(error.abs()), case_mean_mse=mean(error.square()),
                first_three_turns_case_mae=float(torch.stack([error[i].abs().mean() for i in early_cases]).mean()) if early_cases else None,
                winning_return_mae=float(error[wins].abs().mean()) if wins.any() else None,
                losing_return_mae=float(error[~wins].abs().mean()) if (~wins).any() else None)
    if "pending" in data.names:
        pending = data.x[:, data.names.index("pending")] != 0
        result["decision_phases"] = {}
        for name, mask in (("pending", pending), ("player_turn", ~pending)):
            cases = [i[mask[i].numpy()] for i in data.case_indices]
            cases = [i for i in cases if len(i)]
            result["decision_phases"][name] = dict(
                rows=int(mask.sum()), cases=len(cases), winning_rows=int((mask & wins).sum()),
                case_mean_mae=float(torch.stack([error[i].abs().mean() for i in cases]).mean()) if cases else None)
    return result


def evaluate(data, network, linear, mean_return):
    with torch.no_grad():
        return dict(network=metrics(data, network(data.x)), linear=metrics(data, linear(data.x)),
                    constant=metrics(data, torch.full_like(data.y, mean_return)),
                    hp_ceiling=metrics(data, data.ceilings))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--train", type=Path, nargs="+", required=True)
    parser.add_argument("--eval", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--steps", type=int, default=1200)
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()
    if args.steps <= 0 or args.output.exists():
        parser.error("positive steps and a fresh output directory are required")
    torch.set_num_threads(1)
    started = time.perf_counter()
    train_docs, eval_docs = [[json.loads(p.read_bytes()) for p in paths] for paths in (args.train, args.eval)]
    train, evaluation = Dataset(train_docs), Dataset(eval_docs)
    validate_split(train, evaluation)
    network, linear = fit(train, seed=args.seed, steps=args.steps)
    mean_return = float(torch.stack([train.y[i].mean() for i in train.case_indices]).mean())
    result = network.export(train.names)
    result["objective"] = train.objective
    args.output.mkdir(parents=True)
    (args.output / "model.json").write_text(json.dumps(result) + "\n", encoding="utf-8")
    receipt = dict(objective=train.objective, continuation=train.continuation, steps=args.steps, seed=args.seed,
        information="full_state_oracle", target="actual declared-executor terminal return; no optimality claim",
        parameter_count=sum(p.numel() for p in network.parameters()),
        train=evaluate(train, network, linear, mean_return),
        evaluation=evaluate(evaluation, network, linear, mean_return),
        elapsed_seconds=time.perf_counter() - started,
        trainer_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        inputs={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in args.train + args.eval})
    seen_decks = {json.dumps(d["case"]["spec"]["deck"]) for d in train_docs}
    for name, seen in (("seen_decks", True), ("unseen_decks", False)):
        subset = [d for d in eval_docs if (json.dumps(d["case"]["spec"]["deck"]) in seen_decks) == seen]
        if subset:
            receipt["evaluation_" + name] = evaluate(Dataset(subset), network, linear, mean_return)
    (args.output / "fit.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in receipt.items() if k != "inputs"}, indent=2))


if __name__ == "__main__":
    main()
