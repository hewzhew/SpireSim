"""Bounded Gymnasium transport for the current Rust entity combat contract."""
import copy
import hashlib
import json

import gymnasium as gym
import numpy as np
from sts_learning_bridge import GuidedCombatBatch

from tools.guided_search.entity_learning import COLUMNS, positions


def observation_space(entities=192, actions=32):
    fields = {}
    for name, (width, integer) in COLUMNS.items():
        rows = entities if name.startswith("entity_") else actions
        fields[name] = gym.spaces.Box(
            0 if integer else -np.inf, 4096 if integer else np.inf,
            (rows, width), np.int64 if integer else np.float32)
    fields["entity_mask"] = gym.spaces.MultiBinary(entities)
    fields["mask"] = gym.spaces.MultiBinary(actions)
    return gym.spaces.Dict(fields)


def padded(position, space):
    """All rows survive; references shift by one for the absent-reference token."""
    result = {k: np.zeros(s.shape, dtype=s.dtype) for k, s in space.spaces.items()}
    for name in COLUMNS:
        rows = position.columns[name]
        if len(rows) > len(result[name]):
            raise ValueError(f"combat capacity exceeded: {name} {len(rows)} > {len(result[name])}")
        result[name][:len(rows)] = rows + 1 if name.endswith("_refs") else rows
    result["entity_mask"][:len(position.columns["entity_ids"])] = 1
    result["mask"][:len(position.columns["action_ids"])] = 1
    return result


def fingerprint(obs):
    digest = hashlib.sha256()
    for name, values in sorted(obs.items()):
        digest.update(name.encode())
        digest.update(str(values.shape).encode())
        digest.update(values.tobytes())
    return digest.hexdigest()


def terminal_reward(outcome):
    # The terminal HP includes real, capped victory healing. There is no bonus
    # for playing a particular card, damaging the enemy, or ending a turn.
    if outcome["kind"] == "win" and outcome["win_replayed"]:
        return outcome["hp"] / 75.
    if outcome["kind"] == "loss":
        return -1.
    raise ValueError("Unknown cannot supply a terminal training reward")


class CombatEnv(gym.Env):
    metadata = {"render_modes": []}

    def __init__(self, recipes, *, game_seeds=(10000, 20000), limit=64,
                 entities=192, actions=32, sink=None):
        if not recipes or not 1 <= limit < 512 or not 0 <= game_seeds[0] < game_seeds[1]:
            raise ValueError("invalid combat task range")
        self.recipes = copy.deepcopy(recipes)
        self.game_seeds, self.limit, self.sink = game_seeds, limit, sink
        self.observation_space = observation_space(entities, actions)
        self.action_space = gym.spaces.Discrete(actions)
        self.core = None
        self.done = True

    def _observe(self):
        frame = self.core.decision_batch()
        if list(frame["slots"]) != [0]:
            raise ValueError("active combat lacks one numerical decision")
        position, = positions(frame)
        self.revision = frame["revision"]
        self.obs = padded(position, self.observation_space)
        return self.obs

    def _archive(self, status, outcome=None):
        if self.core is None or self.done:
            return
        evidence = json.loads(self.core.evidence_json())
        if self.sink is not None:
            self.sink({"case": self.case, "status": status, "outcome": outcome,
                       "trace": self.trace, "evidence": evidence})
        self.done = True

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        self._archive("unknown_reset")
        if options is not None:
            if set(options) != {"case"}:
                raise ValueError("reset options require an explicit case")
            self.case = copy.deepcopy(options["case"])
        else:
            self.case = copy.deepcopy(self.recipes[int(self.np_random.integers(len(self.recipes)))])
            self.case["spec"]["seed"] = int(self.np_random.integers(*self.game_seeds))
        self.trace = []
        self.core = None
        self.done = False
        try:
            self.core = GuidedCombatBatch.from_specifications([self.case["spec"]], 512)
            return self._observe(), {}
        except BaseException:
            self._archive("unknown_reset_error")
            raise

    def step(self, action):
        if self.done:
            raise ValueError("combat requires reset")
        if not self.action_space.contains(action) or not self.obs["mask"][action]:
            raise ValueError("action outside the current source mask")
        choice = int(action)
        record = {"observation": fingerprint(self.obs), "ordinal": choice}
        try:
            self.core.advance(self.revision, [choice])
            self.trace.append(record)
            outcomes = json.loads(self.core.outcomes_json())
            if outcomes:
                outcome, = outcomes
                reward = terminal_reward(outcome)  # Core failure/Unknown aborts collection.
                info = {"outcome": outcome, "status": outcome["kind"]}
                self._archive(outcome["kind"], outcome)
                # SB3 requires an observation after termination. This absorbing
                # placeholder cannot be stepped and is never used for bootstrap.
                obs = {k: np.zeros(s.shape, dtype=s.dtype) for k, s in self.observation_space.spaces.items()}
                obs["entity_ids"][0, 0] = 1
                obs["entity_mask"][0] = obs["mask"][0] = 1
                return obs, reward, True, False, info
            obs = self._observe()
            truncated = len(self.trace) >= self.limit
            if truncated:
                self._archive("unknown_time_limit")
            return obs, 0., False, truncated, {"status": "unknown_time_limit" if truncated else "active"}
        except BaseException:
            self._archive("unknown_step_error")
            raise

    def reading(self):
        if self.done:
            raise ValueError("no live combat reading")
        return json.loads(self.core.readings_json())["readings"][0]

    def close(self):
        self._archive("unknown_collection_end")
