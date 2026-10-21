"""Small diagnostic tasks. These are not StS environments or training data."""
from dataclasses import dataclass, asdict

import gymnasium as gym
import numpy as np


class CueChoice(gym.Env):
    """Uniform visible binary cue; matching action earns 1 and ends the episode."""
    observation_space = gym.spaces.Discrete(2)
    action_space = gym.spaces.Discrete(2)

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        self.cue = int(self.np_random.integers(2))
        self.closed = False
        return self.cue, {}

    def step(self, action):
        if self.closed or not self.action_space.contains(action):
            raise ValueError("closed episode or invalid action")
        self.closed = True
        return self.cue, float(action == self.cue), True, False, {}


@dataclass(frozen=True)
class Task:
    name: str
    steps: int
    slippery: bool = False
    horizon: int = 64
    minimum_optimal_fraction: float = .90

    def specification(self):
        return {**asdict(self), "map": ["SFF", "FHG"] if self.name != "cue" else None,
                "objective": "discounted terminal goal reward; gamma=0.99",
                "time_limit": "external sampling truncation; bootstrap from final observation"}


TASKS = (Task("cue", 16384, minimum_optimal_fraction=.98),
         Task("lake", 65536), Task("slippery_lake", 131072, slippery=True,
                                    minimum_optimal_fraction=.85))


def make(task):
    if task.name == "cue":
        return CueChoice()
    if task.name not in ("lake", "slippery_lake"):
        raise ValueError("unknown reference task")
    return gym.make("FrozenLake-v1", desc=["SFF", "FHG"], is_slippery=task.slippery,
                    max_episode_steps=task.horizon)


def kernel(task):
    """Evaluation only. Exact transitions from the same Gymnasium owner."""
    env = make(task)
    try:
        n, a = env.observation_space.n, env.action_space.n
        transition = np.zeros((n, a, n))
        success, failure = np.zeros((n, a)), np.zeros((n, a))
        initial = np.zeros(n)
        if task.name == "cue":
            initial[:] = .5
            success[:] = np.eye(2)
            failure[:] = 1 - success
        else:
            initial[:] = env.unwrapped.initial_state_distrib
            for s in range(n):
                for action in range(a):
                    for probability, successor, reward, done in env.unwrapped.P[s][action]:
                        if done:
                            (success if reward else failure)[s, action] += probability
                        else:
                            transition[s, action, successor] += probability
        assert np.allclose(transition.sum(-1) + success + failure, 1)
        return initial, transition, success, failure
    finally:
        env.close()
