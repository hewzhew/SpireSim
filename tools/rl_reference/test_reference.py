import unittest

import gymnasium as gym
from gymnasium.utils.env_checker import check_env
import numpy as np
from stable_baselines3 import PPO
import torch

from .tasks import TASKS, Task, make, kernel
from .evaluation import exact, empirical, assessment


class EndProbe(gym.Env):
    observation_space = gym.spaces.Box(0., 1., shape=(1,), dtype=np.float32)
    action_space = gym.spaces.Discrete(2)

    def __init__(self, terminal):
        self.terminal = terminal

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        return np.array([0.], dtype=np.float32), {}

    def step(self, action):
        return np.array([1.], dtype=np.float32), .25, self.terminal, not self.terminal, {}


class ReferenceChecks(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)

    def test_environments_follow_gymnasium_contract_and_seeded_transitions(self):
        for task in TASKS:
            a, b = make(task), make(task)
            try:
                check_env(a.unwrapped, skip_render_check=True)
                self.assertEqual(a.reset(seed=103)[0], b.reset(seed=103)[0])
                for action in (2, 1, 3, 2, 0) if task.name != "cue" else (1,):
                    ra, rb = a.step(action), b.step(action)
                    self.assertEqual(ra, rb)
                    if ra[2] or ra[3]:
                        break
                initial, transitions, wins, losses = kernel(task)
                self.assertAlmostEqual(initial.sum(), 1)
                np.testing.assert_allclose(transitions.sum(-1) + wins + losses, 1)
            finally:
                a.close(); b.close()

    def test_cue_requires_opposite_actions_in_opposite_contexts(self):
        self.assertAlmostEqual(exact(TASKS[0], np.eye(2))["discounted_return"], 1)
        self.assertAlmostEqual(exact(TASKS[0], np.flip(np.eye(2), axis=1))["discounted_return"], 0)
        self.assertAlmostEqual(exact(TASKS[0], np.full((2, 2), .5))["discounted_return"], .5)
        env = make(TASKS[0]); env.reset(seed=1); env.step(0)
        with self.assertRaises(ValueError):
            env.step(0)

    def test_delayed_reward_has_three_step_discount_and_no_intermediate_bonus(self):
        p = np.eye(4)[[2, 2, 1, 0, 0, 0]]
        e = exact(TASKS[1], p)
        self.assertAlmostEqual(e["success_within_limit"], 1)
        self.assertAlmostEqual(e["expected_observed_steps"], 3)
        self.assertAlmostEqual(e["discounted_return"], .99**2)
        self.assertAlmostEqual(e["optimal_discounted_return"], .99**2)
        rows = empirical(TASKS[1], p, [21, 22])
        self.assertEqual(rows["counts"]["success"], 2)
        self.assertEqual([s[3] for s in rows["rows"][0]["trace"]], [0., 0., 1.])

    def test_timeout_is_unresolved_and_not_a_failed_episode(self):
        task = Task("lake", 100, horizon=3)
        p = np.eye(4)[[3] * 6]  # Top-row wall; no actual terminal is reached.
        self.assertAlmostEqual(exact(task, p)["unresolved_at_limit"], 1)
        result = empirical(task, p, [41, 43])
        self.assertEqual(result["counts"], {"success": 0, "failure": 0, "unresolved": 2})

    def test_stochastic_kernel_matches_many_real_episodes(self):
        p = np.full((6, 4), .25)
        computed = exact(TASKS[2], p)
        actual = empirical(TASKS[2], p, range(7000, 11000))
        for field, count in (("success_within_limit", "success"), ("failure_within_limit", "failure"),
                             ("unresolved_at_limit", "unresolved")):
            self.assertLess(abs(computed[field] - actual["counts"][count] / 4000), .025)
        self.assertAlmostEqual(sum(computed[k] for k in
                               ("success_within_limit", "failure_within_limit", "unresolved_at_limit")), 1)

    def test_actual_sb3_bootstraps_timeout_final_observation_but_not_true_terminal(self):
        for terminal in (True, False):
            model = PPO("MlpPolicy", EndProbe(terminal), seed=31, n_steps=2, batch_size=2,
                        n_epochs=1, gamma=.99, device="cpu")
            model.policy.predict_values = lambda obs: obs[:, :1] * 10
            _, callback = model._setup_learn(2)
            model.collect_rollouts(model.env, callback, model.rollout_buffer, n_rollout_steps=2)
            np.testing.assert_allclose(model.rollout_buffer.rewards, .25 if terminal else .25 + .99 * 10)
            np.testing.assert_array_equal(model._last_obs, [[0.]])
            model.env.close()

    def test_diagnostics_do_not_consume_training_rng_or_change_weights(self):
        model = PPO("MlpPolicy", make(TASKS[1]), seed=23, n_steps=2, batch_size=2, device="cpu")
        weights = {k: v.clone() for k, v in model.policy.state_dict().items()}
        rng = torch.get_rng_state().clone()
        a, b = assessment(model, TASKS[1]), assessment(model, TASKS[1])
        self.assertEqual(a, b)
        self.assertTrue(torch.equal(rng, torch.get_rng_state()))
        self.assertTrue(all(torch.equal(v, model.policy.state_dict()[k]) for k, v in weights.items()))
        model.env.close()


if __name__ == "__main__":
    unittest.main()
