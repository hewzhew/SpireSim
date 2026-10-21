"""Schema-9 ragged records and actor/critic. Rust owns actions and transitions."""
from dataclasses import dataclass
import gzip
import json
from pathlib import Path
import time

import numpy as np
import torch
from torch import nn

SCHEMA, WIDTH, ACTION_WIDTH, KINDS, CATEGORIES = 9, 32, 4, 26, 4096
COLUMNS = {"entity_ids": (2, True), "entity_values": (WIDTH, False),
           "entity_refs": (3, True), "action_ids": (2, True),
           "actions": (ACTION_WIDTH, False), "action_refs": (3, True)}


@dataclass(frozen=True)
class Position:
    # Owned arrays retain source-local references, never padded indices.
    columns: dict


def positions(frame, *, kind_count=KINDS):
    if frame["schema"] != SCHEMA:
        raise ValueError("entity schema mismatch")
    n = len(frame["slots"])
    offsets = {}
    for domain, name in (("entity", "entity_values"), ("action", "actions")):
        split = np.asarray(frame[f"{domain}_splits"])
        if (split.shape != (n + 1,) or not np.issubdtype(split.dtype, np.integer)
                or split[0] != 0 or split[-1] != len(frame[name])
                or np.any(split > np.iinfo(np.int64).max)):
            raise ValueError("invalid entity batch offsets")
        split = split.astype(np.int64)
        if np.any(np.diff(split) <= 0):
            raise ValueError("empty or descending entity batch segment")
        offsets[domain] = split
    for name, (width, integer) in COLUMNS.items():
        a = np.asarray(frame[name])
        domain = "entity" if name.startswith("entity_") else "action"
        if (a.shape != (offsets[domain][-1], width) or not np.isfinite(a).all()
                or (integer and not np.issubdtype(a.dtype, np.integer))):
            raise ValueError(f"invalid {name}")
    result = []
    for i in range(n):
        columns = {}
        for name in COLUMNS:
            split = offsets["entity" if name.startswith("entity_") else "action"]
            value = np.array(frame[name][split[i]:split[i + 1]], copy=True)
            value.setflags(write=False)
            columns[name] = value
        _validate(columns, kind_count=kind_count)
        result.append(Position(columns))
    return result


def _validate(c, *, kind_count=KINDS):
    entities = len(c["entity_ids"])
    for name in ("entity_ids", "action_ids"):
        a = c[name]
        if np.any(a < 0) or np.any(a[:, 0] >= kind_count) or np.any(a[:, 1] >= CATEGORIES):
            raise ValueError("entity vocabulary out of range")
    if entities == 0 or c["entity_ids"][0, 0] != 1:
        raise ValueError("entity frame lacks player context")
    for name in ("entity_refs", "action_refs"):
        if np.any(c[name] < -1) or np.any(c[name] >= entities):
            raise ValueError("reference outside its source position")


def pack(items, device="cpu", *, max_attention_cells=4_000_000, kind_count=KINDS):
    if not items:
        raise ValueError("empty entity minibatch")
    n = len(items)
    e = max(len(p.columns["entity_ids"]) for p in items)
    a = max(len(p.columns["action_ids"]) for p in items)
    if n * (e + 1) ** 2 > max_attention_cells:
        raise ValueError("entity minibatch attention budget exceeded")
    data = {name: np.zeros((n, e if name.startswith("entity_") else a, width),
                          dtype=np.int64 if integer else np.float32)
            for name, (width, integer) in COLUMNS.items()}
    data["entity_mask"] = np.zeros((n, e), dtype=bool)
    data["mask"] = np.zeros((n, a), dtype=bool)
    for i, item in enumerate(items):
        _validate(item.columns, kind_count=kind_count)
        for name in COLUMNS:
            rows = item.columns[name]
            data[name][i, :len(rows)] = rows + 1 if name.endswith("_refs") else rows
        data["entity_mask"][i, :len(item.columns["entity_ids"])] = True
        data["mask"][i, :len(item.columns["action_ids"])] = True
    return {key: torch.as_tensor(value, device=device) for key, value in data.items()}


class EntityEncoder(nn.Module):
    def __init__(self, hidden=64, layers=2, *, kind_count=KINDS):
        super().__init__()
        self.kind = nn.Embedding(kind_count, hidden)
        self.category = nn.Embedding(CATEGORIES, hidden)
        self.facts = nn.Linear(WIDTH, hidden)
        self.relations = nn.Linear(hidden * 4, hidden)
        layer = nn.TransformerEncoderLayer(hidden, 4, hidden * 2, dropout=0,
                                            batch_first=True, activation="gelu")
        self.encoder = nn.TransformerEncoder(layer, layers, enable_nested_tensor=False)

    @staticmethod
    def references(encoded, indices):
        # The explicit zero token represents an absent reference, not the player.
        table = torch.cat((torch.zeros_like(encoded[:, :1]), encoded), dim=1)
        return [table.gather(1, indices[:, :, j, None].expand(-1, -1, encoded.shape[-1]))
                for j in range(indices.shape[-1])]

    def encode_context(self, data):
        ids = data["entity_ids"]
        base = self.kind(ids[:, :, 0]) + self.category(ids[:, :, 1]) + self.facts(data["entity_values"])
        linked = self.relations(torch.cat((base, *self.references(base, data["entity_refs"])), -1))
        encoded = self.encoder(linked, src_key_padding_mask=~data["entity_mask"])
        return encoded


class EntityNetwork(EntityEncoder):
    def __init__(self, hidden=64, layers=2, *, kind_count=KINDS):
        super().__init__(hidden, layers, kind_count=kind_count)
        self.action_kind = nn.Embedding(KINDS, hidden)
        self.policy = nn.Sequential(nn.Linear(hidden * 5 + ACTION_WIDTH, hidden),
                                    nn.SiLU(), nn.Linear(hidden, 1))
        self.value = nn.Linear(hidden, 1)

    def forward(self, data):
        encoded = self.encode_context(data)
        context = encoded[:, 0]
        actions = torch.cat((context[:, None].expand(-1, data["mask"].shape[1], -1),
            self.action_kind(data["action_ids"][:, :, 0]),
            *self.references(encoded, data["action_refs"]), data["actions"]), -1)
        logits = self.policy(actions).squeeze(-1).masked_fill(~data["mask"], -torch.inf)
        return logits, self.value(context).squeeze(-1)


@dataclass(frozen=True)
class Experience:
    position: Position
    slot: int
    revision: int
    choice: int
    logp: float
    value: float


@dataclass(frozen=True)
class Cohort:
    records: list
    outcomes: list


def save_records(path, records):
    payload = {}
    for name, (width, integer) in COLUMNS.items():
        payload[name] = np.concatenate([r.position.columns[name] for r in records]) if records else np.empty(
            (0, width), dtype=np.int64 if integer else np.float32)
    for domain in ("entity", "action"):
        payload[f"{domain}_splits"] = np.cumsum([0] + [len(r.position.columns[f"{domain}_ids"]) for r in records])
    payload["schema"] = SCHEMA
    for name in ("slot", "revision", "choice", "logp", "value"):
        payload[name] = np.array([getattr(r, name) for r in records])
    np.savez_compressed(path, **payload)


class CaseSampling:
    """One action RNG per construction slot, independent of other active cases."""

    def __init__(self, seeds, device="cpu"):
        seeds = list(seeds)
        if (not seeds or any(type(s) is not int or not 0 <= s < 2**64 for s in seeds)
                or len(set(seeds)) != len(seeds)):
            raise ValueError("declare distinct valid case sampling seeds")
        self.generators = [torch.Generator(device=device).manual_seed(s) for s in seeds]

    def sample(self, probabilities, slots, action_counts):
        if any(not isinstance(s, (int, np.integer)) or isinstance(s, (bool, np.bool_)) for s in slots):
            raise ValueError("invalid case sampling slots")
        slots = [int(s) for s in slots]
        if (not slots or probabilities.ndim != 2 or len(slots) != len(probabilities)
                or len(slots) != len(action_counts) or len(set(slots)) != len(slots)
                or any(not 0 <= s < len(self.generators) for s in slots)):
            raise ValueError("invalid case sampling slots")
        # Validate the complete active frame before consuming any case's RNG.
        for i, count in enumerate(action_counts):
            if type(count) is not int or not 0 < count <= probabilities.shape[1]:
                raise ValueError("invalid case action count")
            row = probabilities[i, :count]
            if (not row.is_floating_point() or not torch.isfinite(row).all() or (row < 0).any()
                    or not torch.isfinite(row.sum()) or row.sum() <= 0
                    or self.generators[slots[i]].device != probabilities.device):
                raise ValueError("invalid case sampling probabilities or device")
        # Trim padding: multinomial can consume RNG in proportion to row width.
        return torch.stack([torch.multinomial(probabilities[i, :count], 1,
            generator=self.generators[slot]).squeeze(0)
            for i, (slot, count) in enumerate(zip(slots, action_counts))])


def collect(model, env, generator, directory, *, device="cpu", max_seconds=60,
            require_complete=True):
    """One fresh direct cohort; preserve evidence even when it cannot be trained."""
    if not np.isfinite(max_seconds) or max_seconds <= 0:
        raise ValueError("invalid collection deadline")
    directory = Path(directory)
    directory.mkdir()
    started = time.monotonic()
    records = []
    completed = False
    try:
        while True:
            if time.monotonic() - started >= max_seconds:
                raise TimeoutError("unfinished entity collection is Unknown")
            frame = env.decision_batch()
            if not len(frame["slots"]):
                completed = True
                break
            items = positions(frame)
            with torch.no_grad():
                logits, values = model(pack(items, device))
                logp = logits.log_softmax(-1)
                if isinstance(generator, CaseSampling):
                    choices = generator.sample(logp.exp(), frame["slots"],
                                               [len(p.columns["actions"]) for p in items])
                else:
                    choices = torch.multinomial(logp.exp(), 1, generator=generator).squeeze(1)
                selected = logp.gather(1, choices[:, None]).squeeze(1)
                if not torch.isfinite(selected).all():
                    raise ValueError("nonfinite sampled entity action")
            for i, item in enumerate(items):
                records.append(Experience(item, int(frame["slots"][i]), int(frame["revision"]),
                    int(choices[i]), float(selected[i]), float(values[i].sigmoid())))
            env.advance(frame["revision"], choices.cpu().tolist())
    finally:
        outcomes = json.loads(env.outcomes_json())
        with gzip.open(directory / "evidence.json.gz", "xt", encoding="utf-8") as f:
            f.write(env.evidence_json())
        save_records(directory / "experience.npz", records)
        (directory / "outcomes.json").write_text(json.dumps({"complete":completed,
            "wall_seconds":time.monotonic() - started, "outcomes":outcomes}), encoding="utf-8")
    if require_complete and (not completed or not outcomes or any(o["kind"] not in ("win", "loss") for o in outcomes)
            or any(o["kind"] == "win" and not o["win_replayed"] for o in outcomes)):
        raise ValueError("entity cohort lacks complete actual outcomes")
    return Cohort(records, outcomes)


def update(model, optimizer, cohort, *, device="cpu", minibatch_size=32,
           value_weight=.5, entropy_weight=.01, gradient_limit=1.):
    """One on-policy update accumulated over freshly packed variable-size batches."""
    if (not all(np.isfinite(x) for x in (value_weight, entropy_weight, gradient_limit))
            or value_weight < 0 or entropy_weight < 0 or gradient_limit <= 0):
        raise ValueError("invalid entity update parameters")
    records, outcomes = cohort.records, sorted(cohort.outcomes, key=lambda o: o["slot"])
    if (not records or not outcomes or minibatch_size < 1 or
            [o["slot"] for o in outcomes] != list(range(len(outcomes))) or
            any(o["kind"] not in ("win", "loss") for o in outcomes) or
            any(o["kind"] == "win" and not o["win_replayed"] for o in outcomes)):
        raise ValueError("Unknown or incomplete entity cohort cannot train")
    if {r.slot for r in records} != set(range(len(outcomes))):
        raise ValueError("entity experience does not cover the terminal cohort")
    optimizer.zero_grad(set_to_none=True)
    loss_total = 0.
    try:
        for start in range(0, len(records), minibatch_size):
            rs = records[start:start + minibatch_size]
            data = pack([r.position for r in rs], device)
            choices = torch.tensor([r.choice for r in rs], device=device)
            if any(r.choice < 0 or r.choice >= len(r.position.columns["actions"]) for r in rs):
                raise ValueError("recorded action outside its source")
            logits, values = model(data)
            logp = logits.log_softmax(-1)
            chosen = logp.gather(1, choices[:, None]).squeeze(1)
            behavior = torch.tensor([r.logp for r in rs], device=device)
            if not torch.allclose(chosen.detach(), behavior, atol=1e-4, rtol=1e-4):
                raise ValueError("entity behavior probabilities changed before update")
            returns = torch.tensor([float(outcomes[r.slot]["kind"] == "win") for r in rs], device=device)
            old_value = torch.tensor([r.value for r in rs], device=device)
            policy = -(chosen * (returns - old_value)).sum() / len(outcomes)
            value = nn.functional.binary_cross_entropy_with_logits(values, returns, reduction="sum") / len(records)
            entropy = -(logp.exp() * logp.masked_fill(~data["mask"], 0)).sum() / len(records)
            loss = policy + value_weight * value - entropy_weight * entropy
            if not torch.isfinite(loss):
                raise ValueError("nonfinite entity loss")
            loss.backward()
            loss_total += float(loss.detach())
        nn.utils.clip_grad_norm_(model.parameters(), gradient_limit, error_if_nonfinite=True)
        optimizer.step()
    except BaseException:
        optimizer.zero_grad(set_to_none=True)
        raise
    return {"loss":loss_total, "episodes":len(outcomes), "decisions":len(records)}


def bounded_update(model, optimizer, cohort, *, max_policy_kl, max_backtracks=6, **kwargs):
    """Backtrack one real-outcome update on its observed states, never on evaluation.

    This local mean KL limit is not a guarantee about unseen states or win rate.
    Rejected attempts restore both parameters and optimizer moments.
    """
    import copy
    if (not np.isfinite(max_policy_kl) or max_policy_kl <= 0
            or type(max_backtracks) is not int or not 0 <= max_backtracks <= 12):
        raise ValueError("invalid policy change budget")
    size = kwargs.get("minibatch_size", 32)
    device = kwargs.get("device", "cpu")
    if size < 1 or not cohort.records:
        raise ValueError("empty or invalid policy change batch")

    def distributions():
        result = []
        with torch.no_grad():
            for start in range(0, len(cohort.records), size):
                records = cohort.records[start:start + size]
                logits, _ = model(pack([r.position for r in records], device))
                logp = logits.log_softmax(-1)
                result.extend(row[:len(r.position.columns["actions"])].clone()
                              for row, r in zip(logp, records))
        if any(not torch.isfinite(row).all() for row in result):
            raise ValueError("nonfinite policy change distribution")
        return result

    old = distributions()
    weights, state = copy.deepcopy(model.state_dict()), copy.deepcopy(optimizer.state_dict())
    rates = [group["lr"] for group in optimizer.param_groups]
    attempts = []

    def restore():
        model.load_state_dict(weights)
        optimizer.load_state_dict(copy.deepcopy(state))
        optimizer.zero_grad(set_to_none=True)

    try:
        for attempt in range(max_backtracks + 1):
            if attempt:
                restore()
            scale = .5 ** attempt
            for group, rate in zip(optimizer.param_groups, rates):
                group["lr"] = rate * scale
            metrics = update(model, optimizer, cohort, **kwargs)
            current = distributions()
            kl = [float((a.exp() * (a - b)).sum()) for a, b in zip(old, current)]
            mean_kl = max(0., float(np.mean(kl)))
            attempts.append(dict(rate_scale=scale, mean_policy_kl=mean_kl,
                                 max_state_kl=max(0., max(kl))))
            if mean_kl <= max_policy_kl:
                for group, rate in zip(optimizer.param_groups, rates):
                    group["lr"] = rate
                return dict(metrics, accepted=True, attempts=attempts,
                    behavior_entropy=float(np.mean([float(-(p.exp() * p).sum()) for p in old])),
                    behavior_mean_max_probability=float(np.mean([float(p.exp().max()) for p in old])))
    except BaseException:
        restore()
        raise
    restore()
    return dict(accepted=False, attempts=attempts, episodes=len(cohort.outcomes),
                decisions=len(cohort.records))
