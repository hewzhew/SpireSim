"""Schema-2 entity/action encoder and fresh local network; no game execution."""
import numpy as np
import torch
from torch import nn

CONTEXT, CARD_WIDTH, MONSTER_WIDTH, ACTION_WIDTH = 32, 24, 50, 6
MAX_CARDS, MAX_MONSTERS, MAX_ACTIONS = 16, 3, 32


def prepare(batch, device="cpu"):
    if batch["schema"] != 2:
        raise ValueError("unsupported tactical numeric schema")
    context = np.asarray(batch["context"])
    n = len(context)
    if context.shape != (n, CONTEXT) or not np.isfinite(context).all():
        raise ValueError("invalid tactical context")
    data = {"context": context}
    counts_by_name = {}
    for name, width, maximum, split_name in (
        ("cards", CARD_WIDTH, MAX_CARDS, "card_splits"),
        ("monsters", MONSTER_WIDTH, MAX_MONSTERS, "monster_splits"),
        ("actions", ACTION_WIDTH, MAX_ACTIONS, "action_splits"),
    ):
        rows = np.asarray(batch[name])
        splits = np.asarray(batch[split_name])
        if (rows.ndim != 2 or rows.shape[1] != width or not np.isfinite(rows).all()
                or not np.issubdtype(splits.dtype, np.integer) or splits.shape != (n+1,)
                or splits[0] != 0 or splits[-1] != len(rows)):
            raise ValueError(f"invalid tactical {name} rows or offsets")
        # Cast before differencing unsigned offsets to detect descending splits.
        counts = np.diff(splits.astype(np.int64))
        if np.any(counts < 1) or np.any(counts > maximum):
            raise ValueError(f"invalid tactical {name} capacity")
        padded = np.zeros((n, maximum, width), dtype=np.float32)
        mask = np.zeros((n, maximum), dtype=bool)
        for i, count in enumerate(counts):
            padded[i, :count] = rows[int(splits[i]):int(splits[i+1])]
            mask[i, :count] = True
        data[name] = padded
        data["mask" if name == "actions" else f"{name}_mask"] = mask
        counts_by_name[name] = counts
    refs = np.asarray(batch["action_refs"])
    if (refs.shape != (len(batch["actions"]), 2)
            or not np.issubdtype(refs.dtype, np.integer)):
        raise ValueError("invalid tactical action references")
    indices = np.zeros((n, MAX_ACTIONS, 2), dtype=np.int64)
    for i, count in enumerate(counts_by_name["actions"]):
        start, end = map(int, batch["action_splits"][i:i+2])
        r = refs[start:end].astype(np.int64)
        if (np.any(r < -1) or np.any(r[:, 0] >= counts_by_name["cards"][i])
                or np.any(r[:, 1] >= counts_by_name["monsters"][i])):
            raise ValueError("action references an absent entity")
        # Token zero is context. Absent references point there; action kind
        # distinguishes EndTurn, assembly and FinishSelection.
        indices[i, :count, 0] = r[:, 0] + 1
        indices[i, :count, 1] = np.where(r[:, 1] < 0, 0, r[:, 1] + 1 + MAX_CARDS)
    data["indices"] = indices
    return {k: torch.as_tensor(v, device=device) for k, v in data.items()}


class TacticalNetwork(nn.Module):
    def __init__(self, config):
        super().__init__()
        h = config.hidden
        self.context = nn.Linear(CONTEXT, h)
        self.card = nn.Linear(CARD_WIDTH, h)
        self.monster = nn.Linear(MONSTER_WIDTH, h)
        layer = nn.TransformerEncoderLayer(h, 4, h * 2, dropout=0, batch_first=True, activation="gelu")
        self.encoder = nn.TransformerEncoder(layer, config.layers, enable_nested_tensor=False)
        self.policy = nn.Sequential(nn.Linear(h * 3 + ACTION_WIDTH, h), nn.SiLU(), nn.Linear(h, 1))
        self.value = nn.Linear(h, 1)
        nn.init.zeros_(self.value.weight)
        nn.init.zeros_(self.value.bias)

    def forward(self, data):
        tokens = torch.cat((self.context(data["context"]).unsqueeze(1),
                            self.card(data["cards"]), self.monster(data["monsters"])), dim=1)
        present = torch.cat((torch.ones_like(data["cards_mask"][:, :1]),
                             data["cards_mask"], data["monsters_mask"]), dim=1)
        encoded = self.encoder(tokens, src_key_padding_mask=~present)
        global_state = encoded[:, 0]
        entities = [encoded.gather(1, data["indices"][:, :, column].unsqueeze(-1)
                                  .expand(-1, -1, encoded.shape[-1])) for column in (0, 1)]
        paired = torch.cat((global_state.unsqueeze(1).expand(-1, MAX_ACTIONS, -1),
                            *entities, data["actions"]), dim=-1)
        logits = self.policy(paired).squeeze(-1).masked_fill(~data["mask"], -torch.inf)
        return logits, self.value(global_state).squeeze(-1)
