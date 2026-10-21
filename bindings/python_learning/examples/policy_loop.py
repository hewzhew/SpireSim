"""Connect a policy callback to public batched game decisions.

Run with --policy your_module:choose. The callback receives the current public
NumPy batch and semantic_schema(), and returns one ordinal per decision row.
The environment is kept outside the callback so private diagnostics and search
are not part of this example's policy input.
"""
from __future__ import annotations

import argparse
import importlib
import json
from collections.abc import Callable, Sequence
from numbers import Integral

from sts_learning_bridge import LearningBatchEnv, semantic_schema


def run_policy(policy: Callable, seeds: Sequence[int], *, player_class: str = "ironclad",
               max_commits: int = 10000, max_choices: int = 100000, emit: Callable = print) -> dict:
    if not seeds or min(max_commits, max_choices) < 1:
        raise ValueError("provide at least one seed and positive execution limits")
    env = LearningBatchEnv(list(seeds), 20, final_act=True,
                           candidate_surface="legal", player_class=player_class)
    schema = semantic_schema()
    terminal = set()
    rewards = [0] * len(seeds)
    choices = 0
    commits = 0
    while len(terminal) < len(seeds) and commits < max_commits and choices < max_choices:
        while not env.ready:
            if choices >= max_choices:
                break
            batch = env.decision_batch(semantic=True)
            ordinals = list(policy(batch, schema))
            counts = batch["candidate_counts"].tolist()
            if len(ordinals) != len(counts):
                raise ValueError("policy must return one ordinal per current decision row")
            if any(isinstance(o, bool) or not isinstance(o, Integral) or not 0 <= o < n
                   for o, n in zip(ordinals, counts)):
                raise ValueError("policy returned an ordinal outside the current candidate row")
            env.choose(list(batch["decision_token_ids"]), [int(o) for o in ordinals])
            choices += 1
        if not env.ready:
            break
        transition = env.step()
        commits += 1
        for row, slot in enumerate(transition["slot_indices"].tolist()):
            rewards[slot] += int(transition["reward"][row])
        terminal.update(transition["terminal_slot_indices"].tolist())
        emit(json.dumps({"commit": commits, "terminal_slots": sorted(terminal),
                         "reward": transition["reward"].tolist()}))
    status = "terminated" if len(terminal) == len(seeds) else "choice_limit" if choices >= max_choices else "commit_limit"
    result = {"status": status,
              "commits": commits, "choose_calls": choices, "rewards": rewards,
              "terminal_slots": sorted(terminal)}
    emit(json.dumps(result))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", required=True, help="Importable module:function callback")
    parser.add_argument("--seeds", type=int, nargs="+", required=True)
    parser.add_argument("--character", choices=["ironclad", "silent", "defect", "watcher"], default="ironclad")
    parser.add_argument("--max-commits", type=int, default=10000)
    parser.add_argument("--max-choices", type=int, default=100000)
    args = parser.parse_args()
    module, separator, name = args.policy.partition(":")
    if not separator or not module or not name:
        parser.error("--policy must have the form module:function")
    policy = getattr(importlib.import_module(module), name)
    result = run_policy(policy, args.seeds, player_class=args.character,
                        max_commits=args.max_commits, max_choices=args.max_choices)
    return 0 if result["status"] == "terminated" else 2


if __name__ == "__main__":
    raise SystemExit(main())
