"""Conditional resource tasks. Separate contract/weights from single-combat learning."""
from dataclasses import dataclass
import hashlib
import json
import time

import numpy as np
import torch
from torch import nn

from tools.guided_search import entity_learning as combat

SCHEMA, CONTEXT_WIDTH, CONTEXT_KINDS = 1, 24, 7


@dataclass(frozen=True)
class Position:
    combat: combat.Position
    context: dict


def position(frame):
    if frame.get("segment_schema") != SCHEMA:
        raise ValueError("segment schema mismatch")
    base, = combat.positions(frame["combat"])
    context = {}
    for name, width, integer in (("context_ids", 2, True), ("context_values", CONTEXT_WIDTH, False),
                                 ("context_refs", 3, True)):
        a = np.array(frame[name], copy=True)
        if (a.ndim != 2 or a.shape[1] != width or not np.isfinite(a).all()
                or (integer and not np.issubdtype(a.dtype, np.integer))):
            raise ValueError("invalid segment context columns")
        a.setflags(write=False)
        context[name] = a
    _validate(context)
    return Position(base, context)


def _validate(c):
    for name, width, integer in (("context_ids", 2, True), ("context_values", CONTEXT_WIDTH, False),
                                 ("context_refs", 3, True)):
        a = c[name]
        if (a.ndim != 2 or a.shape[1] != width or not np.isfinite(a).all()
                or (integer and not np.issubdtype(a.dtype, np.integer))):
            raise ValueError("invalid segment context columns")
    ids, values, refs = (c[x] for x in ("context_ids", "context_values", "context_refs"))
    if (len(ids) == 0 or len(values) != len(ids) or len(refs) != len(ids)
            or tuple(ids[0]) != (1, 1)
            or np.any(ids < 0) or np.any(ids[:, 0] >= CONTEXT_KINDS)
            or np.any(ids[:, 1] >= combat.CATEGORIES)
            or np.any(refs < -1) or np.any(refs >= len(ids))):
        raise ValueError("invalid segment context vocabulary or references")


def pack(items, device="cpu", *, max_attention_cells=4_000_000):
    if not items:
        raise ValueError("empty segment batch")
    n, e = len(items), max(len(p.combat.columns["entity_ids"]) for p in items)
    c = max(len(p.context["context_ids"]) for p in items)
    if n * (e + c) ** 2 > max_attention_cells:
        raise ValueError("segment attention budget exceeded")
    result = combat.pack([p.combat for p in items], device, max_attention_cells=max_attention_cells)
    columns = {"context_ids": np.zeros((n, c, 2), dtype=np.int64),
               "context_values": np.zeros((n, c, CONTEXT_WIDTH), dtype=np.float32),
               "context_refs": np.zeros((n, c, 3), dtype=np.int64),
               "context_mask": np.zeros((n, c), dtype=bool)}
    for i, p in enumerate(items):
        _validate(p.context)
        rows = len(p.context["context_ids"])
        for name in p.context:
            columns[name][i, :rows] = p.context[name] + (1 if name.endswith("_refs") else 0)
        columns["context_mask"][i, :rows] = True
    result.update({k: torch.as_tensor(v, device=device) for k, v in columns.items()})
    return result


class SegmentNetwork(nn.Module):
    """Fresh conditional actor/critic. A value logit predicts this task's attainment."""
    def __init__(self, hidden=64, layers=2):
        super().__init__()
        self.combat = combat.EntityNetwork(hidden, layers)
        self.context_kind = nn.Embedding(CONTEXT_KINDS, hidden)
        self.context_category = nn.Embedding(combat.CATEGORIES, hidden)
        self.context_facts = nn.Linear(CONTEXT_WIDTH, hidden)
        self.context_relations = nn.Linear(hidden * 4, hidden)

    def forward(self, data):
        core = self.combat
        ids = data["entity_ids"]
        base = core.kind(ids[:, :, 0]) + core.category(ids[:, :, 1]) + core.facts(data["entity_values"])
        linked = core.relations(torch.cat((base, *core.references(base, data["entity_refs"])), -1))
        ids = data["context_ids"]
        context = (self.context_kind(ids[:, :, 0]) + self.context_category(ids[:, :, 1])
                   + self.context_facts(data["context_values"]))
        context = self.context_relations(torch.cat((context, *core.references(context, data["context_refs"])), -1))
        all_entities = torch.cat((linked, context), 1)
        mask = torch.cat((data["entity_mask"], data["context_mask"]), 1)
        encoded = core.encoder(all_entities, src_key_padding_mask=~mask)
        player = encoded[:, 0]
        actions = torch.cat((player[:, None].expand(-1, data["mask"].shape[1], -1),
            core.action_kind(data["action_ids"][:, :, 0]),
            *core.references(encoded[:, :base.shape[1]], data["action_refs"]), data["actions"]), -1)
        return (core.policy(actions).squeeze(-1).masked_fill(~data["mask"], -torch.inf),
                core.value(player).squeeze(-1))


@dataclass(frozen=True)
class Experience:
    position: Position
    slot: int
    task_key: str
    revision: int
    choice: int
    logp: float
    value: float


@dataclass(frozen=True)
class Cohort:
    records: list
    # Each row carries the native target, exact task, actual ending and verification.
    outcomes: list


def _task_key(evidence):
    """Association metadata only; never a model feature or a replay certificate."""
    raw = json.dumps({"specification": evidence["specification"],
                      "max_actions": evidence["max_actions"]},
                     sort_keys=True, separators=(",", ":"), allow_nan=False)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def collect(envs, model, sampling, *, device="cpu", max_batches=1024, wall_seconds=60):
    """One sampled continuation per resident task; no branch search or successful-only filter.

    Callers declare every task and persist evidence even if this function raises.
    Native action budgets include fixed intermission choices; Python budgets bound
    inference batches cooperatively. Unfinished tasks remain target=None.
    """
    if (not envs or type(max_batches) is not int or max_batches < 1
            or not np.isfinite(wall_seconds) or wall_seconds <= 0):
        raise ValueError("invalid segment collection budget")
    task_keys = [_task_key(json.loads(e.evidence_json())) for e in envs]
    model.eval()
    records = []
    deadline = time.monotonic() + wall_seconds
    for _ in range(max_batches):
        if time.monotonic() >= deadline:
            break
        frames = [(i, e.decision()) for i, e in enumerate(envs)]
        active = [(i, f) for i, f in frames if f is not None]
        if not active:
            break
        items = [position(f) for _, f in active]
        batch = pack(items, device)
        with torch.no_grad():
            logits, value_logits = model(batch)
            probs = logits.softmax(-1)
            counts = [len(p.combat.columns["actions"]) for p in items]
            choices = sampling.sample(probs, [i for i, _ in active], counts)
            selected_logps = logits.log_softmax(-1).gather(1, choices[:, None]).squeeze(1)
            values = value_logits.sigmoid()
        for row, ((slot, frame), p) in enumerate(zip(active, items)):
            choice = int(choices[row])
            envs[slot].advance(frame["revision"], choice)
            records.append(Experience(p, slot, task_keys[slot], int(frame["revision"]),
                                      choice, float(selected_logps[row]), float(values[row])))
    outcomes = []
    for i, e in enumerate(envs):
        e.verify() # Verify misses and unfinished traces too; this does not relabel Unknown.
        evidence = json.loads(e.evidence_json())
        if _task_key(evidence) != task_keys[i]:
            raise ValueError("segment task changed during collection")
        outcomes.append({"slot": i, "attained": e.attained(), "verified": True,
                         "max_actions": evidence["max_actions"],
                         "specification": evidence["specification"], "successor": evidence["successor"],
                         "resources": evidence["resources"]})
    return Cohort(records, outcomes)


def update(model, optimizer, cohort, *, device="cpu", batch_size=32):
    """One undiscounted whole-segment Monte Carlo actor/critic update.

    Only the assigned conditional task supplies the return. No step shaping,
    HP/potion conversion, hindsight goal relabeling or intermediate win credit.
    This is an experimental update rule, not evidence of useful learning.
    """
    outcomes, records = cohort.outcomes, cohort.records
    if (not outcomes or not records or batch_size < 1
            or any(o.get("slot") != i or o.get("verified") is not True
                   or type(o.get("attained")) is not bool for i, o in enumerate(outcomes))):
        raise ValueError("segment update requires every verified terminal task; Unknown cannot train")
    if any(type(r.slot) is not int or not 0 <= r.slot < len(outcomes) for r in records):
        raise ValueError("segment experience slot outside cohort")
    if set(r.slot for r in records) != set(range(len(outcomes))):
        raise ValueError("segment task has no recorded combat decisions")
    task_keys = [_task_key(o) for o in outcomes]
    if any(r.task_key != task_keys[r.slot] for r in records):
        raise ValueError("segment experience does not match its assigned task")
    for slot in range(len(outcomes)):
        revisions = [r.revision for r in records if r.slot == slot]
        if any(type(r) is not int or r < 0 for r in revisions) or any(
                b <= a for a, b in zip(revisions, revisions[1:])):
            raise ValueError("segment experience source order changed")
    if any(type(r.choice) is not int or not 0 <= r.choice < len(r.position.combat.columns["actions"])
           for r in records):
        raise ValueError("segment action outside its source")
    model.eval()
    optimizer.zero_grad(set_to_none=True)
    loss_total = 0.
    try:
        for start in range(0, len(records), batch_size):
            rs = records[start:start + batch_size]
            batch = pack([r.position for r in rs], device)
            logits, value_logits = model(batch)
            choices = torch.tensor([r.choice for r in rs], device=device)
            selected = logits.log_softmax(-1).gather(1, choices[:, None]).squeeze(1)
            old_logp = torch.tensor([r.logp for r in rs], device=device)
            old_value = torch.tensor([r.value for r in rs], device=device)
            if (not torch.isfinite(selected).all() or not torch.isfinite(value_logits).all()
                    or not torch.allclose(selected.detach(), old_logp, atol=2e-5, rtol=2e-5)
                    or not torch.allclose(value_logits.detach().sigmoid(), old_value, atol=2e-5, rtol=2e-5)):
                raise ValueError("segment behavior probabilities or values changed")
            target = torch.tensor([float(outcomes[r.slot]["attained"]) for r in rs], device=device)
            policy = -(selected * (target - old_value)).sum() / len(outcomes)
            value = nn.functional.binary_cross_entropy_with_logits(value_logits, target, reduction="sum") / len(records)
            loss = policy + value
            loss.backward()
            loss_total += float(loss.detach())
        if not all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()):
            raise ValueError("nonfinite segment gradients")
        optimizer.step()
    except Exception:
        optimizer.zero_grad(set_to_none=True)
        raise
    return {"loss": loss_total, "episodes": len(outcomes), "decisions": len(records),
            "attained": sum(o["attained"] for o in outcomes)}
