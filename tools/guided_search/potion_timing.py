"""Block Potion intervention values, independent of card-action policies.

Targets are observed HP gains over the shared known no-use continuations. The timing portfolio
includes the already executed never/now witnesses, so limited flexible search
cannot erase a known option. Gains do not price the consumed potion or measure
unknown-draw win probability. No prediction is connected to solver admission.
"""
from dataclasses import dataclass
import json

import numpy as np
import torch
from torch import nn
from torch.nn import functional as F

from .entity_learning import EntityEncoder, Position, pack, positions

HP_SCALE = 25.


@dataclass(frozen=True)
class PotionTimingSample:
    position: Position
    potion_row: int
    gains: tuple[float, float]  # now, observed free-timing portfolio
    observed: tuple[bool, bool]


def observed_outcomes(report):
    """Actual winning endpoints, including supplied and replayed continuations.

    A recorded suffix can supply the known no-use option without asking each
    detached search to rediscover it. Pending, loss and escape frames do not
    supply a winning HP baseline.
    """
    outcomes = [p["outcome"] for p in report["plans"]]
    for replay in report.get("plan_consequences", []):
        frames = replay["frames"]
        if frames and frames[-1]["boundary"]["kind"] == "victory":
            outcomes.append(frames[-1]["resources"])
    return outcomes


def intervention_targets(never, now, flexible):
    """Match fresh intervention reports before deriving any training target."""
    reports = (never, now, flexible)
    case = never["case"]
    source = case["position"]
    if (case["source"] != "snapshot" or case.get("prefix", [])
            or never["root"] != source or flexible["root"] != source):
        raise ValueError("timing pilot needs the unchanged common snapshot root")
    filled = [(i, p) for i, p in enumerate(source["combat"]["entities"]["potions"])
              if p is not None]
    if len(filled) != 1 or filled[0][1]["id"] != "BlockPotion":
        raise ValueError("timing pilot requires exactly one BlockPotion")
    slot = filled[0][0]
    expected_prefix = [{"UsePotion": {"potion_index": slot, "target": None}}]
    for report, limit, prefix in zip(reports, (0, 0, None), ([], expected_prefix, [])):
        other = report["case"]
        if (other["source"] != "snapshot" or other["position"] != source
                or other["producer"] != case["producer"]
                or other.get("prefix", []) != prefix
                or other["max_potion_uses"] != limit
                or report["config"]["max_potion_uses"] != limit
                or report["implementation_sha256"] != never["implementation_sha256"]
                or report["information"] != "full_state_oracle"):
            raise ValueError("mismatched potion intervention source or teacher")
        if any(p["total_potion_expenditures"] != 0
               for p in observed_outcomes(report)) and limit == 0:
            raise ValueError("post-prefix continuation spent a potion")
    hp = [max((p["final_hp"] for p in observed_outcomes(r)), default=None)
          for r in reports]
    # Flexible search also contains card-only continuations. Sharing those
    # prevents a newly discovered no-potion tactic being mislabeled as a gain
    # from potion use. The now report's zero expenditures are post-prefix and
    # therefore MUST NOT contribute to this no-use baseline.
    no_use = [p["final_hp"] for r in (never, flexible) for p in observed_outcomes(r)
              if p["total_potion_expenditures"] == 0]
    baseline = max(no_use, default=None)
    # All three successful routes are executable from the common source: the
    # now route carries its recorded prefix. This is an observed portfolio,
    # never a counterfactual best-HP/best-reserve synthetic endpoint.
    known = [v for v in hp if v is not None]
    masks = (baseline is not None and hp[1] is not None,
             baseline is not None and bool(known))
    gains = ((hp[1] - baseline) / HP_SCALE if masks[0] else 0.,
             (max(known) - baseline) / HP_SCALE if masks[1] else 0.)
    return source, slot, gains, masks


def encode_source(bridge, source):
    """Inference input needs an actual node, never intervention outcomes."""
    filled = [(i, p) for i, p in enumerate(source["combat"]["entities"]["potions"]) if p is not None]
    if source["engine"] != "CombatPlayerTurn" or len(filled) != 1 or filled[0][1]["id"] != "BlockPotion":
        raise ValueError("timing input needs player control and exactly one BlockPotion")
    slot = filled[0][0]
    position = positions(bridge.combat_entity_batch(json.dumps([source])))[0]
    c = position.columns
    rows = np.flatnonzero((c["entity_ids"][:, 0] == 6)
                         & (c["entity_values"][:, 0] == slot)
                         & (c["entity_values"][:, 1] == 1))
    if len(rows) != 1:
        raise ValueError("selected potion lacks its exact entity row")
    return PotionTimingSample(position, int(rows[0]), (0., 0.), (False, False))


def encode_trial(bridge, never, now, flexible):
    source, _, gains, observed = intervention_targets(never, now, flexible)
    encoded = encode_source(bridge, source)
    return PotionTimingSample(encoded.position, encoded.potion_row, gains, observed)


class PotionTimingValue(EntityEncoder):
    """Two intervention gain heads; no logits for playing cards or EndTurn."""
    def __init__(self, hidden=32, layers=1):
        super().__init__(hidden, layers)
        self.gains = nn.Sequential(nn.Linear(hidden * 2, hidden), nn.SiLU(), nn.Linear(hidden, 2))

    def forward(self, data, potion_rows):
        encoded = self.encode_context(data)
        potion = encoded[torch.arange(len(potion_rows), device=encoded.device), potion_rows]
        return self.gains(torch.cat((encoded[:, 0], potion), -1))


def objective(prediction, gains, observed):
    if prediction.shape != gains.shape or observed.shape != gains.shape:
        raise ValueError("timing target shape mismatch")
    if not observed.any():
        return prediction.sum() * 0
    return F.smooth_l1_loss(prediction[observed], gains[observed])


def fit(model, rows, *, steps=600, minibatch_size=24, seed=0):
    if not rows or steps < 1 or minibatch_size < 1:
        raise ValueError("empty or invalid potion timing fit")
    if not any(any(r.observed) for r in rows):
        raise ValueError("timing fit has no executed resource targets")
    optimizer = torch.optim.AdamW(model.parameters(), lr=1e-3)
    rng = np.random.default_rng(seed)
    model.train()
    for _ in range(steps):
        batch = [rows[i] for i in rng.choice(len(rows), min(len(rows), minibatch_size), replace=False)]
        predicted = model(pack([r.position for r in batch]), torch.tensor([r.potion_row for r in batch]))
        loss = objective(predicted, torch.tensor([r.gains for r in batch]),
                         torch.tensor([r.observed for r in batch]))
        if not torch.isfinite(loss):
            raise ValueError("nonfinite timing objective")
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        nn.utils.clip_grad_norm_(model.parameters(), 1.)
        optimizer.step()


def predict(model, rows, *, minibatch_size=24):
    """Observed gain estimates in HP units, without a spend recommendation."""
    output = []
    model.eval()
    with torch.no_grad():
        for start in range(0, len(rows), minibatch_size):
            batch = rows[start:start + minibatch_size]
            value = model(pack([r.position for r in batch]), torch.tensor([r.potion_row for r in batch]))
            output.extend((value * HP_SCALE).tolist())
    return np.asarray(output, dtype=np.float32).reshape(-1, 2)
