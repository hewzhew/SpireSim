"""Resource consequences of finite whole-combat continuations of sampled states.

Teacher-found predicts search coverage, never death probability. Resource heads
are supervised only by replay-checked wins, all from the same retained plan.
This module supplies no retention score or policy imitation target.
"""
from dataclasses import dataclass
import json

import numpy as np
import torch
from torch import nn
from torch.nn import functional as F

try:
    from .entity_learning import EntityEncoder, Position, pack, positions
except ImportError:
    from entity_learning import EntityEncoder, Position, pack, positions


@dataclass(frozen=True)
class CandidateReturn:
    position: Position
    root: str
    group: int
    kept: bool
    teacher_found: bool
    # Delta HP / 75, max HP / 75, gold / 200. These are three distinct outcomes.
    resources: tuple[float, float, float]
    potion_rows: tuple[int, ...]
    potion_retained: tuple[float, ...]


def encode_return(bridge, sample, report, *, root, group, kept, potion_limit):
    """Use the exact candidate and its teacher result; never repair old records."""
    source = sample["position"]
    if report["root"] != source:
        raise ValueError("teacher root differs from sampled candidate")
    if report["config"]["max_potion_uses"] != potion_limit:
        raise ValueError("teacher resource constraint differs from sampled family")
    # The frame's allowance is cumulative explicit uses; the detached teacher's
    # constraint is relative to its new root. Initial pilot supports 0 or unlimited.
    if potion_limit not in (0, None) or (potion_limit == 0 and sample["potion_uses"] != 0):
        raise ValueError("candidate value pilot requires card-only or unlimited potions")
    frame = bridge.combat_entity_batch(json.dumps([source]),
        potion_uses=[sample["potion_uses"]], max_potion_uses=potion_limit)
    position = positions(frame)[0]
    columns = position.columns
    rows = np.flatnonzero((columns["entity_ids"][:, 0] == 6)
                         & (columns["entity_values"][:, 1] == 1))
    found = bool(report["plans"])
    resources, retained = (0., 0., 0.), []
    if found:
        # Same retained plan supplies all heads: no independently cherry-picked
        # best HP and best potion labels that no single route can achieve.
        plan = report["plans"][0]
        before = source["combat"]["entities"]["player"]
        after = plan["successor"]["combat"]["entities"]["player"]
        resources = ((after["current_hp"] - before["current_hp"]) / 75.,
                     (after["max_hp"] - before["max_hp"]) / 75.,
                     (after["gold"] - before["gold"]) / 200.)
        spent = plan["outcome"]["starting_potion_expenditure_slots"]
        for row in rows:
            slot = int(columns["entity_values"][row, 0])
            retained.append(float(not (spent & (1 << slot))))
    else:
        # Placeholder targets are masked, not negative HP/reserve evidence.
        retained = [0.] * len(rows)
    return CandidateReturn(position, root, group, kept, found, resources,
                           tuple(map(int, rows)), tuple(retained))


class EntityContinuationValue(EntityEncoder):
    def __init__(self, hidden=64, layers=2):
        super().__init__(hidden, layers)
        self.teacher_found = nn.Linear(hidden, 1)
        self.resources = nn.Linear(hidden, 3)
        self.potion_retained = nn.Sequential(nn.Linear(hidden * 2, hidden),
                                             nn.SiLU(), nn.Linear(hidden, 1))

    def forward(self, data):
        encoded = self.encode_context(data)
        context = encoded[:, 0]
        potions = self.potion_retained(torch.cat((encoded,
            context[:, None].expand_as(encoded)), -1)).squeeze(-1)
        return {"teacher_found": self.teacher_found(context).squeeze(-1),
                "resources": self.resources(context), "potion_retained": potions}


def targets(rows, data):
    device = data["entity_values"].device
    found = torch.tensor([r.teacher_found for r in rows], device=device, dtype=torch.bool)
    resources = torch.tensor([r.resources for r in rows], device=device, dtype=torch.float32)
    reserve = torch.zeros_like(data["entity_values"][:, :, 0])
    mask = torch.zeros_like(reserve, dtype=torch.bool)
    for i, row in enumerate(rows):
        reserve[i, list(row.potion_rows)] = torch.tensor(row.potion_retained, device=device)
        if row.teacher_found:
            mask[i, list(row.potion_rows)] = True
    return found, resources, reserve, mask


def objective(prediction, target):
    found, resources, reserve, mask = target
    coverage = F.binary_cross_entropy_with_logits(prediction["teacher_found"], found.float())
    value = (F.mse_loss(prediction["resources"][found], resources[found])
             if found.any() else prediction["resources"].sum() * 0)
    potion = (F.binary_cross_entropy_with_logits(prediction["potion_retained"][mask], reserve[mask])
              if mask.any() else prediction["potion_retained"].sum() * 0)
    return value + .25 * coverage + .25 * potion


def fit(model, rows, *, steps=300, minibatch_size=32, seed=0):
    if not rows or steps < 1 or minibatch_size < 1:
        raise ValueError("empty or invalid continuation fit")
    optimizer = torch.optim.AdamW(model.parameters(), lr=1e-3)
    generator = np.random.default_rng(seed)
    model.train()
    for _ in range(steps):
        batch = [rows[i] for i in generator.choice(len(rows),
                 min(len(rows), minibatch_size), replace=False)]
        data = pack([r.position for r in batch])
        loss = objective(model(data), targets(batch, data))
        if not torch.isfinite(loss):
            raise ValueError("nonfinite continuation objective")
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        nn.utils.clip_grad_norm_(model.parameters(), 1.)
        optimizer.step()


def predict(model, rows, *, minibatch_size=32):
    result = []
    model.eval()
    with torch.no_grad():
        for start in range(0, len(rows), minibatch_size):
            batch = rows[start:start + minibatch_size]
            prediction = model(pack([r.position for r in batch]))
            for i, row in enumerate(batch):
                result.append({"teacher_found":float(prediction["teacher_found"][i].sigmoid()),
                    "resources":prediction["resources"][i].tolist(),
                    "potion_retained":prediction["potion_retained"][i, list(row.potion_rows)].sigmoid().tolist()})
    return result
