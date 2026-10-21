"""Learn P(the declared executor wins and retains at least H HP).

The simulator owns known effects. This monotone resource-goal curve estimates
unexpanded continuation outcomes, not optimal play or a certified success rate.
"""
import argparse
import hashlib
import json
from pathlib import Path
import time

import numpy as np
import torch
from torch import nn

from continuation_learning import Dataset, hand_layout, metrics, validate_split
from retention_learning import RetentionNetwork

OBJECTIVE = "guardian_fixed_executor_resource_curve_v1"


class ResourceCurve(nn.Module):
    def __init__(self, names):
        super().__init__()
        self.source = RetentionNetwork(len(names), hidden=64, hand_layout=hand_layout(names, 16))
        self.slope = nn.Linear(64, 1)

    def components(self, x):
        h = self.source.hidden(self.source.project(x)).relu()
        return self.source.output(h).squeeze(-1), nn.functional.softplus(self.slope(h).squeeze(-1))

    def probability(self, x, ceilings, hp_goal):
        a, b = self.components(x)
        goal = torch.as_tensor(hp_goal, dtype=torch.float64)
        if torch.any(goal < 0) or torch.any(goal != goal.round()):
            raise ValueError("resource goals must be nonnegative integer HP")
        p = torch.sigmoid(a.double() - b.double() * goal / 75.)
        return torch.where(goal > (ceilings.double() * 75).round(), 0., p).float()

    def value(self, x, ceilings):
        a, b = self.components(x)
        a, b, c = [v.double() for v in (a, b, ceilings)]
        d = b * c
        # Stable softplus divided difference, including saturated positive logits.
        upper, lower = nn.functional.softplus(a), nn.functional.softplus(a - d)
        positive = d + torch.log1p(torch.exp(-a.abs())) - torch.log1p(torch.exp(-(a - d).abs()))
        difference = torch.where(a - d > 0, positive, upper - lower)
        area = difference / b.clamp(min=1e-12)
        area = torch.where(d < 1e-6, c * torch.sigmoid(a - d / 2), area)
        # Probability of loss contributes -1; integrating the smooth tail
        # approximates discrete retained-HP return. No arbitrary HP goal is chosen.
        return (torch.sigmoid(a) - 1 + area).clamp(min=-1).minimum(c).float()

    def export(self, names):
        result = self.source.export(names)
        result["objective"] = OBJECTIVE
        result["resource_curve"] = dict(slope_weights=self.slope.weight.detach()[0].tolist(),
                                         slope_bias=float(self.slope.bias.detach()[0]))
        return result


def labels(returns, hp_goals):
    return ((returns >= 0) & ((returns * 75).round() >= hp_goals)).float()


def fit(data, seed=1, steps=1200):
    torch.manual_seed(seed)
    model = ResourceCurve(data.names)
    optimizer = torch.optim.Adam(model.parameters(), lr=.001, weight_decay=1e-4)
    # Keep source draws identical to the old equal-case trainer. Goal sampling
    # uses its own RNG and cannot change which combat sources are trained.
    sources, goals = np.random.default_rng(seed), np.random.default_rng(seed + 10000)
    for _ in range(steps):
        i = np.asarray([sources.choice(data.case_indices[j])
                        for j in sources.integers(len(data.case_indices), size=128)])
        maximum = np.maximum(1, np.rint(data.ceilings[i].numpy() * 75).astype(int))
        goal_hp = torch.tensor(goals.integers(1, maximum + 1))
        a, b = model.components(data.x[i])
        logits = torch.cat((a, a - b * goal_hp / 75.))
        target = torch.cat((labels(data.y[i], 0), labels(data.y[i], goal_hp)))
        loss = nn.functional.binary_cross_entropy_with_logits(logits, target)
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        optimizer.step()
    return model


def evaluate(model, data):
    with torch.no_grad():
        result = metrics(data, model.value(data.x, data.ceilings))
        p = model.probability(data.x, data.ceilings, 0)
        target = labels(data.y, 0)
        def case_mean(v):
            return float(torch.stack([v[i].mean() for i in data.case_indices]).mean())
        result["win_brier"] = case_mean((p - target).square())
        maximum = (data.ceilings * 75).round().long()
        error, count = torch.zeros(len(data.y)), torch.zeros(len(data.y))
        for goal in range(76):
            legal = maximum >= goal
            probability = model.probability(data.x, data.ceilings, goal)
            error += (probability - labels(data.y, goal)).square() * legal
            count += legal
        result["all_feasible_integer_goals_brier"] = case_mean(error / count)
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--train", type=Path, nargs="+", required=True)
    parser.add_argument("--eval", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--steps", type=int, default=1200)
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()
    if args.output.exists() or args.steps <= 0:
        parser.error("fresh output and positive steps required")
    torch.set_num_threads(1)
    docs = [[json.loads(p.read_bytes()) for p in paths] for paths in (args.train, args.eval)]
    train, evaluation = [Dataset(d) for d in docs]
    validate_split(train, evaluation)
    started = time.perf_counter()
    model = fit(train, seed=args.seed, steps=args.steps)
    training_seconds = time.perf_counter() - started
    receipt = dict(objective=OBJECTIVE, source_objective=train.objective, continuation=train.continuation,
        information="full_state_oracle", seed=args.seed, steps=args.steps,
        target="actual declared-executor victory with retained HP goal; no optimality claim",
        sampling="equal-case sources; goal 0 and one uniform positive feasible integer HP goal per source",
        parameter_count=sum(p.numel() for p in model.parameters()), training_seconds=training_seconds,
        train=evaluate(model, train), evaluation=evaluate(model, evaluation),
        trainer_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        inputs={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in args.train + args.eval})
    seen = {json.dumps(d["case"]["spec"]["deck"]) for d in docs[0]}
    for name, wanted in (("seen_decks", True), ("unseen_decks", False)):
        subset = [d for d in docs[1] if (json.dumps(d["case"]["spec"]["deck"]) in seen) == wanted]
        if subset:
            receipt["evaluation_" + name] = evaluate(model, Dataset(subset))
    args.output.mkdir(parents=True)
    (args.output / "model.json").write_text(json.dumps(model.export(train.names)) + "\n", encoding="utf-8")
    (args.output / "fit.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k:receipt[k] for k in ("training_seconds", "parameter_count", "evaluation")}, indent=2))


if __name__ == "__main__":
    main()
