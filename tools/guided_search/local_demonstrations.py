"""Successful-path policy supervision, separate from on-policy returns.

Only the live core exports paths; stored numerical evidence never executes roots.
One demonstrated action is a feasible example, not an optimality/negative label.
"""
from __future__ import annotations

from dataclasses import dataclass
import gzip
import json

import numpy as np
import torch
from torch import nn

from tools.guided_search.local_train import hash_file, prepare, write_json


def export_demonstrations(env, directory):
    with gzip.open(directory / "evidence.json.gz", "rt", encoding="utf-8") as f:
        if f.read() != env.evidence_json():
            raise ValueError("saved evidence differs from the live demonstration source")
    outcomes = json.loads(env.outcomes_json())
    records, episodes = [], []
    for o in outcomes:
        if o["kind"] != "win":
            continue
        if not o["win_replayed"]:
            raise ValueError("demonstration requires replayed victory")
        frame = env.demonstration(o["slot"])
        if frame["kind"] != "replayed_win_demonstration" or "revision" in frame:
            raise ValueError("unexpected demonstration authority")
        data = {k: v.numpy() for k, v in prepare(frame).items()}
        data.update(slots=np.asarray(frame["slots"], dtype=np.int64),
                    choices=np.asarray(frame["choices"], dtype=np.int64))
        if not len(data["choices"]) or np.any(data["slots"] != o["slot"]):
            raise ValueError("invalid demonstration source")
        records.append(data)
        episodes.append({"slot": o["slot"], "rows": len(data["choices"])})
    payload_hash = None
    if records:
        payload = {k: np.concatenate([r[k] for r in records]) for k in records[0]}
        path = directory / "demonstrations.npz"
        with path.open("xb") as f:
            np.savez_compressed(f, **payload)
        payload_hash = hash_file(path)
    write_json(directory / "demonstrations.json", {
        "kind": "replayed_win_demonstration", "episodes": episodes,
        "evidence_sha256": hash_file(directory / "evidence.json.gz"),
        "payload_sha256": payload_hash,
        "excluded": [{"slot": o["slot"], "kind": o["kind"]} for o in outcomes if o["kind"] != "win"],
        "target": "observed successful action; no value or optimality target",
    })


def load_demonstrations(directory, config):
    receipt = json.loads((directory / "demonstrations.json").read_text())
    if (receipt["kind"] != "replayed_win_demonstration"
            or receipt["evidence_sha256"] != hash_file(directory / "evidence.json.gz")):
        raise ValueError("demonstration evidence binding changed")
    with gzip.open(directory / "evidence.json.gz", "rt", encoding="utf-8") as f:
        evidence = json.load(f)
    if evidence["poisoned"] or evidence["schema"] != (2 if config.task == "draw_select" else 1):
        raise ValueError("invalid demonstration producer or schema")
    winners = [i for i, e in enumerate(evidence["episodes"])
               if e["outcome"] and e["outcome"]["kind"] == "win" and e["outcome"]["win_replayed"]]
    if [e["slot"] for e in receipt["episodes"]] != winners:
        raise ValueError("demonstration slots do not match replayed victories")
    if not winners:
        if receipt["payload_sha256"] is not None:
            raise ValueError("empty demonstration has a payload")
        return None
    if receipt["payload_sha256"] != hash_file(directory / "demonstrations.npz"):
        raise ValueError("demonstration payload binding changed")
    with np.load(directory / "demonstrations.npz", allow_pickle=False) as f:
        data = {k: torch.as_tensor(f[k], device=config.device) for k in f.files}
    if "logp" in data or "old_value" in data:
        raise ValueError("demonstrations are not behavior-policy records")
    offset = 0
    for e in receipt["episodes"]:
        slot, rows = e["slot"], e["rows"]
        path = evidence["episodes"][slot]["decision_ordinals"]
        if (rows <= 0 or rows != len(path)
                or data["slots"][offset:offset+rows].tolist() != [slot] * rows
                or data["choices"][offset:offset+rows].tolist() != path):
            raise ValueError("demonstration decisions differ from the replayed path")
        offset += rows
    if any(len(v) != offset for v in data.values()):
        raise ValueError("demonstration row counts disagree")
    return data


def validate_rescue_sources(direct_directory, search_directory):
    """Bind a compact search cohort to ALL actual greedy losses of a direct cohort.

    Search slots remain local to their own evidence. No ordinal remapping or root
    execution occurs. A rescue is conditional on this exact failed direct policy,
    not proof of the root's difficulty or of any individual action's optimality.
    """
    def read(directory):
        with gzip.open(directory / "evidence.json.gz", "rt", encoding="utf-8") as f:
            return json.load(f)
    direct, search = read(direct_directory), read(search_directory)
    result = json.loads((direct_directory / "outcomes.json").read_text())
    if (not result["completed"] or result["rule"] != "greedy"
            or direct["mode"] != "direct_policy" or search["mode"] != "fixed_root_search"
            or direct["poisoned"] or search["poisoned"] or direct["schema"] != search["schema"]):
        raise ValueError("rescue requires completed greedy evidence and compatible search")
    episodes = direct["episodes"]
    if any(not e["outcome"] or e["outcome"]["kind"] not in ("win", "loss") for e in episodes):
        raise ValueError("unresolved direct root is not a proven greedy loss")
    if (result["outcomes"] != [e["outcome"] for e in episodes]
            or any(e["outcome"]["slot"] != i for i, e in enumerate(episodes))):
        raise ValueError("direct outcome receipt differs from its evidence")
    lost = [i for i, e in enumerate(episodes) if e["outcome"]["kind"] == "loss"]
    if not lost or len(lost) != len(search["episodes"]):
        raise ValueError("search cohort must cover all and only the direct losses")
    keys = ("source", "task", "seed", "enemy", "starting_hp", "deck_recipe", "root")
    mapping = []
    for slot, (original, e) in enumerate(zip(lost, search["episodes"])):
        o = e["outcome"]
        if (any(episodes[original][key] != e[key] for key in keys)
                or not o or o["slot"] != slot
                or not (o["kind"] == "win" and o["win_replayed"] or o["kind"].startswith("unknown"))):
            raise ValueError("search rescue does not match its exact failed direct root")
        mapping.append({"search_slot": slot, "direct_slot": original, "kind": o["kind"]})
    return {"mapping": mapping, "direct_evidence_sha256": hash_file(direct_directory / "evidence.json.gz"),
            "search_evidence_sha256": hash_file(search_directory / "evidence.json.gz")}


def _validate_actions(data):
    if "logp" in data or "old_value" in data:
        raise ValueError("behavior-policy records cannot be used as demonstrations")
    choices, mask, slots = data["choices"], data["mask"], data["slots"]
    if (choices.dtype != torch.int64 or choices.ndim != 1 or len(choices) == 0
            or mask.ndim != 2 or mask.shape[0] != len(choices) or mask.dtype != torch.bool
            or slots.dtype != torch.int64 or slots.shape != choices.shape or torch.any(slots < 0)
            or torch.any(choices < 0) or torch.any(choices >= mask.shape[1])
            or not mask.gather(1, choices[:, None]).all()):
        raise ValueError("demonstrated action is outside its source")


def imitate(model, optimizer, data, config):
    """One MLE policy update, each complete demonstrated episode equally weighted."""
    if data is None:
        return {"updated": False, "reason": "no replayed demonstrations", "rows": 0, "episodes": 0}
    return imitate_groups(model, optimizer, [(data, 1.0)], config)


@dataclass(frozen=True)
class PolicyAnchor:
    data: dict
    log_probabilities: torch.Tensor


def freeze_policy(model, data):
    """Freeze a reference distribution AND its numerical source, outside autograd.

    This records a policy tendency, not a return target or a search visit count.
    The experiment caller records checkpoint and source provenance on disk.
    """
    _validate_actions(data)
    if model.training:
        raise ValueError("reference policy must be in evaluation mode")
    with torch.no_grad():
        logp = model(data)[0].log_softmax(-1)
    if logp.shape != data["mask"].shape or not torch.isfinite(logp[data["mask"]]).all():
        raise ValueError("invalid reference action probabilities")
    return PolicyAnchor({k: v.detach().clone() for k, v in data.items()}, logp.detach().clone())


def _validate_anchor(data, reference):
    if (not isinstance(reference, PolicyAnchor) or set(reference.data) != set(data)
            or any(not torch.equal(reference.data[k], v) for k, v in data.items())):
        raise ValueError("policy anchor belongs to a different numerical source")
    logp, mask = reference.log_probabilities, data["mask"]
    if (logp.requires_grad or logp.shape != mask.shape or not torch.isfinite(logp[mask]).all()
            or not torch.isneginf(logp[~mask]).all()
            or not torch.allclose(logp.exp().sum(-1), torch.ones(len(mask), device=mask.device), atol=1e-5, rtol=1e-5)):
        raise ValueError("invalid detached anchor probabilities or padding")


def _episode_mean(values, slots):
    episodes, inverse, counts = slots.unique(return_inverse=True, return_counts=True)
    return (values / counts[inverse]).sum() / len(episodes)


def imitate_groups(model, optimizer, groups, config, *, anchors=()):
    """One policy update with explicit relative mass per source-bound cohort.

    Each cohort averages over its own episodes, then losses combine by weight.
    Equal numeric slots from different sources never merge. The entire request
    validates before updates; no implicit empty-group dropping or loss labels.
    Value has no target, but shared features can still change its predictions.
    """
    if not groups or any(not np.isfinite(weight) or weight <= 0 or data is None for data, weight in groups):
        raise ValueError("groups require nonempty demonstrations and positive finite weights")
    total = sum(weight for _, weight in groups)
    if not np.isfinite(total):
        raise ValueError("invalid total demonstration weight")
    for data, _ in groups:
        _validate_actions(data)
    seen = set()
    for group, reference, strength in anchors:
        if (type(group) is not int or group < 0 or group >= len(groups) or group in seen
                or not np.isfinite(strength) or strength <= 0):
            raise ValueError("invalid anchor group or strength")
        _validate_anchor(groups[group][0], reference)
        seen.add(group)
    losses, metrics, probabilities, anchor_metrics = [], [], [], []
    for data, weight in groups:
        logits, _ = model(data)
        log_probs = logits.log_softmax(-1)
        probabilities.append(log_probs)
        logp = log_probs.gather(1, data["choices"][:, None]).squeeze(1)
        group_loss = -_episode_mean(logp, data["slots"])
        losses.append(group_loss * (weight / total))
        metrics.append({"weight": weight / total, "policy_loss": float(group_loss.detach()),
                        "rows": len(logp), "episodes": len(data["slots"].unique())})
    for group, reference, strength in anchors:
        data, _ = groups[group]
        mask = data["mask"]
        current = probabilities[group].masked_fill(~mask, 0)
        original = reference.log_probabilities.masked_fill(~mask, 0)
        mass = reference.log_probabilities.exp().masked_fill(~mask, 0)
        divergence = _episode_mean((mass * (original - current)).sum(-1), data["slots"])
        losses.append(strength * divergence)
        anchor_metrics.append({"group": group, "strength": strength, "kl": float(divergence.detach())})
    loss = torch.stack(losses).sum()
    if not torch.isfinite(loss):
        raise ValueError("nonfinite imitation loss")
    optimizer.zero_grad(set_to_none=True)
    loss.backward()
    gradient = nn.utils.clip_grad_norm_(model.parameters(), config.gradient_limit, error_if_nonfinite=True)
    optimizer.step()
    return {"updated": True, "policy_loss": float(loss.detach()), "gradient_norm": float(gradient),
            "rows": sum(m["rows"] for m in metrics), "episodes": sum(m["episodes"] for m in metrics),
            "groups": metrics, "anchors": anchor_metrics}
