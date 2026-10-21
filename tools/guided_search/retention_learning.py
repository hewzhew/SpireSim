"""Fit the scoped Rust retention MLP from witnessed bounds and frozen backups.

Completed continuations give achievable returns, not optimality labels. A failed
continuation cannot label its nonterminal ancestors as doomed. Complete action
sets can tighten resource ceilings and bootstrap from a frozen network; deployed
predictions remain approximations, not certified bounds.
"""
import argparse
from collections import deque
import heapq
import hashlib
import json
from pathlib import Path
import time

import numpy as np
import torch
from torch import nn

SCHEMA = 4
OBJECTIVE = "guardian_no_potions_terminal_hp_v1"
RESPONSE_OBJECTIVE = "guardian_complete_response_resource_loss_v1"


def response_losses(data):
    """Exact best resource loss to this turn's response, not whole-fight value.

    A terminal boundary uses its actual return (including post-combat healing);
    a living boundary uses the physical terminal-HP ceiling. Closed graphs with
    no reachable boundary supply no target. Row identities are local to a case.
    """
    rows = data["rows"]
    targets = np.full(len(rows), np.nan, dtype=np.float32)
    required = ("turn", "pending", "assembling", "terminal_hp_ceiling")
    if not all(name in data["feature_names"] for name in required):
        return targets
    columns = {name: data["feature_names"].index(name) for name in required}

    def feature(i, name):
        return rows[i]["features"][columns[name]]

    def playable(i):
        return (rows[i]["terminal"] is None and not feature(i, "pending")
                and not feature(i, "assembling"))

    by_turn = {}
    for i, row in enumerate(rows):
        if playable(i) and row["children"]:
            by_turn.setdefault(feature(i, "turn"), []).append(i)
    for turn, roots in by_turn.items():
        # Only complete action sets certify coverage. Origins and witness traces
        # can prove reachability, but cannot fill a missing action set here.
        todo, seen, interior, gaps = list(roots), set(), set(), set()
        parents, values = {}, {}
        while todo:
            i = todo.pop()
            if i in seen:
                continue
            seen.add(i)
            terminal = rows[i]["terminal"]
            if terminal is not None or (playable(i) and feature(i, "turn") > turn):
                values[i] = terminal if terminal is not None else feature(i, "terminal_hp_ceiling")
                continue
            if feature(i, "turn") < turn:
                raise ValueError("recorded response moves backwards in turn")
            interior.add(i)
            if rows[i]["children"] is None:
                gaps.add(i)
            else:
                for j in rows[i]["children"]:
                    parents.setdefault(j, set()).add(i)
                    todo.append(j)
        # Any reachable unexpanded decision prevents an exact label, even if
        # another alternative already reaches a good response boundary.
        queue = deque(gaps)
        while queue:
            for i in parents.get(queue.popleft(), ()):
                if i not in gaps:
                    gaps.add(i)
                    queue.append(i)
        queue = deque(values)
        while queue:
            j = queue.popleft()
            for i in parents.get(j, ()):
                if values[j] > values.get(i, -np.inf):
                    values[i] = values[j]
                    queue.append(i)
        for i in interior - gaps:
            # A next-turn boundary never receives the previous turn's label;
            # it can be a root in its own turn if that response is also complete.
            if i in values and playable(i) and feature(i, "turn") == turn:
                loss = feature(i, "terminal_hp_ceiling") - values[i]
                if loss < -1e-6:
                    raise ValueError("response exceeds physical resource ceiling")
                targets[i] = max(0., loss)
    return targets


def backed_returns(observed, terminals, edges):
    """Best witnessed continuation through recorded exact-state transitions.

    Partial action sets suffice for an achievable return, never an optimality
    claim. No prediction travels over these edges. A cycle without a completed
    continuation stays unlabelled; identical features do not create an edge.
    """
    values = np.nan_to_num(observed, nan=-np.inf).copy()
    terminal = np.isfinite(terminals)
    values[terminal] = terminals[terminal]
    parents = [set() for _ in values]
    for i, j in edges:
        if terminal[i]:
            raise ValueError("recorded transition leaves a terminal state")
        parents[j].add(i)
    queue = [(-float(v), i) for i, v in enumerate(values) if np.isfinite(v)]
    heapq.heapify(queue)
    while queue:
        negative, j = heapq.heappop(queue)
        value = -negative
        if value < values[j]:
            continue
        for i in parents[j]:
            if value > values[i]:
                values[i] = value
                heapq.heappush(queue, (-value, i))
    values[~np.isfinite(values)] = np.nan
    return values


def backed_ceilings(ceilings, terminals, children):
    """Tighten only over fully enumerated action sets, including virtual choices."""
    upper = np.asarray(ceilings, dtype=np.float32).copy()
    known = np.isfinite(terminals)
    upper[known] = terminals[known]
    parents = [set() for _ in upper]
    pending = deque(i for i, branch in enumerate(children) if branch is not None)
    queued = set(pending)
    for i in pending:
        for j in children[i]:
            parents[j].add(i)
    while pending:
        i = pending.popleft()
        queued.remove(i)
        value = max(upper[j] for j in children[i])
        if value < upper[i]:
            upper[i] = value
            for parent in sorted(parents[i] - queued):
                pending.append(parent)
                queued.add(parent)
    return upper


class Dataset:
    def __init__(self, documents):
        self.names = documents[0]["data"]["feature_names"]
        self.features, self.observed, self.terminals, self.children, self.groups = [], [], [], [], []
        self.response_losses = []
        labels = set()
        edges = set()
        for group, document in enumerate(documents):
            data = document["data"]
            label = document["case"]["label"]
            if (data["schema"] != SCHEMA or data["objective"] != OBJECTIVE
                    or data["feature_names"] != self.names or label in labels):
                raise ValueError("inconsistent value data schema or duplicate source")
            labels.add(label)
            offset = len(self.features)
            rows = data["rows"]
            def index(value):
                if type(value) is not int or not 0 <= value < len(rows):
                    raise ValueError("invalid recorded state index")
                return offset + value

            for i, row in enumerate(rows):
                x = row["features"]
                if len(x) != len(self.names) or not np.isfinite(x).all():
                    raise ValueError("bad feature width or value")
                for key in ("terminal", "observed_return"):
                    if row[key] is not None and not -1 <= row[key] <= 1:
                        raise ValueError("invalid resource return")
                children = row["children"]
                if children is not None and (not children or any(type(j) is not int or not 0 <= j < len(rows) for j in children)):
                    raise ValueError("invalid complete source edges")
                for j in children or []:
                    edges.add((offset + i, index(j)))
                origin = row.get("origin")
                if origin is not None:
                    edges.add((index(origin[0]), offset + i))
                self.features.append(x)
                self.observed.append(np.nan if row["observed_return"] is None else row["observed_return"])
                self.terminals.append(np.nan if row["terminal"] is None else row["terminal"])
                self.children.append(None if children is None else [offset + j for j in children])
                self.groups.append(group)
            for trace in data.get("traces", []):
                path = [index(i) for i in trace["rows"]]
                edges.update(zip(path, path[1:]))
            self.response_losses.extend(response_losses(data))
        self.x = torch.tensor(self.features, dtype=torch.float32)
        self.observed = np.asarray(self.observed, dtype=np.float32)
        self.terminals = np.asarray(self.terminals, dtype=np.float32)
        self.groups = np.asarray(self.groups)
        self.response_losses = np.asarray(self.response_losses, dtype=np.float32)
        labelled = np.isfinite(self.response_losses)
        self.response_stats = dict(rows=int(labelled.sum()),
            positive_loss_rows=int((self.response_losses > 1e-6).sum()),
            groups=int(len(np.unique(self.groups[labelled]))))
        self.ceilings = self.x[:, self.names.index("terminal_hp_ceiling")].numpy() if "terminal_hp_ceiling" in self.names else np.ones(len(self.x))
        self.backed = backed_returns(self.observed, self.terminals, edges)
        self.lower = np.nan_to_num(self.backed, nan=-1.)
        self.upper = backed_ceilings(self.ceilings, self.terminals, self.children)
        if np.any(self.lower > self.upper + 1e-6):
            raise ValueError("witnessed return exceeds complete-action resource ceiling")
        # Account only for floating-point encoding roundoff, not label uncertainty.
        self.upper = np.maximum(self.upper, self.lower)
        self.backup_stats = dict(
            recorded_edges=len(edges),
            improved_observed=int((np.isfinite(self.observed) & (self.backed > self.observed + 1e-6)).sum()),
            loss_to_win=int(((self.observed == -1) & (self.backed >= 0)).sum()),
            newly_connected=int((~np.isfinite(self.observed) & np.isfinite(self.backed)).sum()),
            exact_rows=int((self.lower == self.upper).sum()),
            tightened_ceilings=int((self.upper < self.ceilings).sum()),
            nonterminal_loss_only=int(((self.backed == -1) & ~np.isfinite(self.terminals)).sum()),
        )

    def targets(self, predictions=None):
        # Nonterminal trajectory results constrain a range, not a point. Only
        # coincident bounds or a complete-action frozen backup supply point targets.
        result = np.full(len(self.x), np.nan, dtype=np.float32)
        exact = self.lower == self.upper
        result[exact] = self.lower[exact]
        if predictions is not None:
            values = np.clip(np.asarray(predictions), self.lower, self.upper)
            for i, children in enumerate(self.children):
                if children is not None:
                    # No discount: extra inputs are not intrinsically worse.
                    result[i] = np.clip(values[children].max(), self.lower[i], self.upper[i])
        return result

    def supervision_summary(self, predictions=None, *, estimates=None):
        """Separate grounded targets, frozen backups and interval constraints.

        Counts describe this recorded graph, not identifiability, independent
        examples or learning difficulty. In particular a completed losing route
        does not make an unresolved nonterminal state a negative point target.
        """
        for values in (predictions, estimates):
            if values is not None and (np.shape(values) != self.lower.shape or not np.isfinite(values).all()):
                raise ValueError("supervision predictions must be finite and match rows")
        targets = self.targets(predictions)
        exact = self.lower == self.upper
        point = np.isfinite(targets)
        terminal = np.isfinite(self.terminals)
        complete = np.asarray([branch is not None for branch in self.children])
        scopes = {"all": np.ones(len(self.x), dtype=bool), "nonterminal": ~terminal}
        if "pending" in self.names and "assembling" in self.names:
            scopes["player_decisions"] = (~terminal
                & (self.x[:, self.names.index("pending")].numpy() == 0)
                & (self.x[:, self.names.index("assembling")].numpy() == 0))
        if estimates is not None:
            estimates = np.asarray(estimates, dtype=np.float32)
            violation = np.abs(estimates - np.clip(estimates, self.lower, self.upper))

        result = {}
        for name, scope in scopes.items():
            selected = lambda mask: scope & mask
            count = lambda mask: int(selected(mask).sum())
            widths = (self.upper - self.lower)[scope]
            stats = dict(rows=int(scope.sum()),
                grounded_point_rows=count(exact),
                frozen_backup_point_rows=count(point & ~exact),
                interval_only_rows=count(~point),
                complete_action_rows=count(complete),
                winning_continuation_rows=count(np.isfinite(self.backed) & (self.backed >= 0)),
                loss_only_continuation_rows=count(self.backed == -1),
                no_completed_continuation_rows=count(~np.isfinite(self.backed)),
                mean_interval_width=float(widths.mean()) if len(widths) else None,
                median_interval_width=float(np.median(widths)) if len(widths) else None)
            if estimates is not None:
                def error(mask):
                    indices = selected(mask)
                    return float(np.abs(estimates[indices] - targets[indices]).mean()) if indices.any() else None
                stats.update(grounded_point_mae=error(exact),
                    frozen_backup_point_mae=error(point & ~exact),
                    active_interval_constraints=count(~point & (violation > 0)),
                    inactive_interval_constraints=count(~point & (violation == 0)))
            result[name] = stats
        return result


class RetentionNetwork(nn.Module):
    def __init__(self, width, hidden=32, response_head=False, hand_layout=None):
        super().__init__()
        self.hand_layout = hand_layout
        self.cards = nn.Linear(hand_layout[1], hand_layout[2]) if hand_layout else None
        self.hidden = nn.Linear(hand_layout[0] + 2 * hand_layout[2] if hand_layout else width, hidden)
        self.output = nn.Linear(hidden, 1)
        # Created after the deployed layers so enabling it preserves their init.
        self.response = nn.Linear(hidden, 1) if response_head else None

    def forward(self, x):
        # Do not clamp training gradients. Clipping is only for frozen targets
        # and deployed inference, with the same semantics on both runtimes.
        return self.output(self.hidden(self.project(x)).relu()).squeeze(-1)

    def response_prediction(self, x):
        return self.response(self.hidden(self.project(x)).relu()).squeeze(-1)

    def project(self, x):
        if self.cards is None:
            return x
        start, width, _ = self.hand_layout
        tokens = x[:, start:].reshape(-1, 10, width)
        z = self.cards(tokens).relu() * tokens[:, :, :1]
        return torch.cat((x[:, :start], z.sum(1) / 10, z.max(1).values), dim=1)

    def export(self, names, masked=(), *, response=False):
        weights = self.hidden.weight.detach().clone()
        encoder = None
        if self.cards is not None:
            start, width, _ = self.hand_layout
            weights[:, [i for i in masked if i < start]] = 0
            token_mask = [{i - start - slot * width for i in masked
                           if start + slot * width <= i < start + (slot + 1) * width}
                          for slot in range(10)]
            if any(m != token_mask[0] for m in token_mask) or 0 in token_mask[0]:
                raise ValueError("hand masks must preserve presence and be shared across instances")
            card_weights = self.cards.weight.detach().clone()
            card_weights[:, list(token_mask[0])] = 0
            encoder = dict(start=start, width=width, weights=card_weights.tolist(),
                           bias=self.cards.bias.detach().tolist())
        else:
            weights[:, list(masked)] = 0
        output = self.response if response else self.output
        result = dict(schema=SCHEMA, objective=RESPONSE_OBJECTIVE if response else OBJECTIVE, feature_names=names,
                    hidden_weights=weights.tolist(), hidden_bias=self.hidden.bias.detach().tolist(),
                    output_weights=output.weight.detach()[0].tolist(),
                    output_bias=float(output.bias.detach()[0]))
        if encoder is not None:
            result['hand_encoder'] = encoder
        return result


def mechanism_columns(names):
    return [i for i, name in enumerate(names) if name.startswith("guardian/")
            or name.startswith("enemy/move/") or name in ("enemy/SharpHide", "enemy/ModeShift")
            or name.startswith("zone/") and name.endswith("/numbers/3")
            or name.startswith("hand/") and name.rsplit('/', 1)[-1] in ('mode_remaining', 'sharp_hide')]


def fit(data, *, seed=1, hidden=32, steps=500, backup_rounds=8, mask_mechanisms=False,
        response_weight=0., response_data=None, hand_hidden=0):
    if not np.isfinite(response_weight) or response_weight < 0:
        raise ValueError("response weight must be finite and nonnegative")
    if response_data is not None and not response_weight:
        raise ValueError("separate response data requires a positive response weight")
    response_data = data if response_data is None else response_data
    if response_data.names != data.names:
        raise ValueError("response and terminal feature schemas differ")
    torch.manual_seed(seed)
    rng = np.random.default_rng(seed)
    response_rng = np.random.default_rng(seed)  # Do not perturb the terminal batches.
    layout = None
    if hand_hidden:
        if not 1 <= hand_hidden <= 64 or "hand/0/present" not in data.names:
            raise ValueError("hand encoder requires instance features and 1..64 hidden units")
        start = data.names.index("hand/0/present")
        width, remainder = divmod(len(data.names) - start, 10)
        suffix = [name.removeprefix('hand/0/') for name in data.names[start:start + width]]
        if not width or remainder or data.names[start:] != [f'hand/{i}/{n}' for i in range(10) for n in suffix]:
            raise ValueError("inconsistent hand feature layout")
        layout = (start, width, hand_hidden)
    model = RetentionNetwork(len(data.names), hidden, response_head=response_weight > 0, hand_layout=layout)
    x = data.x.clone()
    masked = mechanism_columns(data.names) if mask_mechanisms else []
    x[:, masked] = 0
    response_x = x if response_data is data else response_data.x.clone()
    response_x[:, masked] = 0
    optimizer = torch.optim.Adam(model.parameters(), lr=0.001)
    pools = [np.flatnonzero(data.groups == group) for group in np.unique(data.groups)]
    response_pools = [np.flatnonzero((response_data.groups == group)
        & np.isfinite(response_data.response_losses)) for group in np.unique(response_data.groups)]
    response_pools = [pool for pool in response_pools if len(pool)]
    if response_weight and not response_pools:
        raise ValueError("no complete response targets for auxiliary training")
    if not np.isfinite(data.observed).any():
        raise ValueError("no actual completed continuations for initialization")
    history = []
    warm = None
    for stage in range(backup_rounds + 1):
        with torch.no_grad():
            predictions = model(x).numpy() if stage else None
        y = data.targets(predictions)
        for _ in range(steps if stage == 0 else max(1, steps // 4)):
            indices = np.asarray([rng.choice(pools[g]) for g in rng.integers(len(pools), size=128)])
            prediction = model(x[indices])
            target = training_targets(prediction, torch.from_numpy(y[indices]),
                                      torch.from_numpy(data.lower[indices]), torch.from_numpy(data.upper[indices]))
            loss = nn.functional.smooth_l1_loss(prediction, target)
            if response_weight:
                auxiliary = np.asarray([response_rng.choice(response_pools[g])
                    for g in response_rng.integers(len(response_pools), size=128)])
                response_loss = nn.functional.smooth_l1_loss(model.response_prediction(response_x[auxiliary]),
                    torch.from_numpy(response_data.response_losses[auxiliary]))
                loss = loss + response_weight * response_loss
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.)
            optimizer.step()
        with torch.no_grad():
            eligible = np.flatnonzero(np.isfinite(y))
            raw = model(x).numpy()
            fitted = np.clip(raw[eligible], -1, data.ceilings[eligible])
            mae = float(np.abs(fitted - y[eligible]).mean()) if len(eligible) else None
            bound_error = float(np.abs(raw - np.clip(raw, data.lower, data.upper)).mean())
        metrics = dict(stage=stage, target_rows=len(eligible), target_mae=mae, mean_bound_error=bound_error,
            # Use this stage's frozen targets and the fitted network separately.
            supervision=data.supervision_summary(predictions, estimates=raw))
        if response_weight:
            with torch.no_grad():
                indices = np.flatnonzero(np.isfinite(response_data.response_losses))
                error = np.abs(model.response_prediction(response_x[indices]).numpy() - response_data.response_losses[indices])
                positive = response_data.response_losses[indices] > 1e-6
                metrics.update(response_mae=float(error.mean()),
                    positive_response_mae=float(error[positive].mean()) if positive.any() else None)
        history.append(metrics)
        if stage == 0:
            warm = model.export(data.names, masked)
    auxiliary = model.export(data.names, masked, response=True) if response_weight else None
    return model.export(data.names, masked), warm, history, auxiliary


def training_targets(prediction, point, lower, upper):
    # Projection gives zero gradient inside an observed interval. It does not
    # drag every state on a bad rollout down to the rollout's terminal return.
    projected = prediction.detach().clamp(lower, upper)
    return torch.where(torch.isfinite(point), point, projected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--steps", type=int, default=500)
    parser.add_argument("--backup-rounds", type=int, default=8)
    parser.add_argument("--hidden", type=int, default=32)
    parser.add_argument("--hand-hidden", type=int, default=0,
                        help="optional shared hand encoder width (1..64); zero uses flat inputs")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--mask-mechanisms", action="store_true")
    parser.add_argument("--response-weight", type=float, default=0.,
                        help="training-only complete-response loss weight; zero disables")
    parser.add_argument("--response-input", type=Path, nargs="+",
                        help="separate auxiliary cases; preserves terminal rows and sampling")
    args = parser.parse_args()
    if args.steps < 1 or args.backup_rounds < 0 or not 1 <= args.hidden <= 128 or not 0 <= args.hand_hidden <= 64:
        parser.error("invalid fitting dimensions or schedule")
    if not np.isfinite(args.response_weight) or args.response_weight < 0:
        parser.error("response weight must be finite and nonnegative")
    if args.response_input and not args.response_weight:
        parser.error("--response-input requires a positive --response-weight")
    args.output.mkdir(parents=True, exist_ok=False)
    torch.set_num_threads(1)
    started = time.perf_counter()
    data = Dataset([json.loads(p.read_text(encoding="utf-8")) for p in args.input])
    response_data = Dataset([json.loads(p.read_text(encoding="utf-8")) for p in args.response_input]) if args.response_input else None
    model, warm, history, auxiliary = fit(data, seed=args.seed, hidden=args.hidden, steps=args.steps,
        backup_rounds=args.backup_rounds, mask_mechanisms=args.mask_mechanisms,
        response_weight=args.response_weight, response_data=response_data, hand_hidden=args.hand_hidden)
    for name, value in (("model.json", model), ("warm-model.json", warm)):
        (args.output / name).write_text(json.dumps(value), encoding="utf-8")
    if auxiliary is not None:
        (args.output / "response-head.json").write_text(json.dumps(auxiliary), encoding="utf-8")
    receipt = dict(seconds=time.perf_counter()-started, rows=len(data.x), sources=len(args.input),
                   feature_count=len(data.names), seed=args.seed, hidden=args.hidden, hand_hidden=args.hand_hidden,
                   mask_mechanisms=args.mask_mechanisms, steps=args.steps, backup_rounds=args.backup_rounds,
                   witnessed_backups=data.backup_stats,
                   data_supervision=data.supervision_summary(),
                   response_auxiliary=dict(weight=args.response_weight, **(response_data or data).response_stats,
                       target_method="complete_recorded_response_graph_v1",
                       sampling="separate uniform-case 128-row batch per update; independent RNG"),
                   trainer_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                   inputs=[dict(path=str(p.resolve()), sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in args.input],
                   response_inputs=[dict(path=str(p.resolve()), sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in args.response_input] if args.response_input else None,
                   target_method="witnessed_intervals_complete_action_backups_v1",
                   history=history, limitation="Grounded training bounds and approximate frozen backups; deployed predictions are not bounds, calibrated win probabilities or optimality claims.")
    (args.output / "fit.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
    print(json.dumps({k:v for k,v in receipt.items() if k not in ("inputs", "response_inputs")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
