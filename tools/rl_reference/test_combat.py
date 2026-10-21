import copy
import json

import numpy as np
import pytest
import torch
from stable_baselines3 import PPO
from stable_baselines3.common.vec_env import DummyVecEnv
from sts_learning_bridge import GuidedCombatBatch

from tools.rl_reference.combat_env import CombatEnv, fingerprint, terminal_reward
from tools.rl_reference.combat_policy import CombatPolicy


def case(deck=None, hp=68):
    return {"family": "test_only", "spec": {"seed": 73, "encounter": "JawWorm", "act": 1,
        "room": "MonsterRoom", "hp": hp, "deck": deck or [["Strike", 0], ["Defend", 0]],
        "relics": [], "potions": [], "max_potion_uses": None}}


@pytest.fixture(autouse=True)
def single_thread():
    torch.set_num_threads(1)


def model(env, **kw):
    return PPO(CombatPolicy, env, n_steps=8, batch_size=4, n_epochs=1,
               gamma=1., seed=11, device="cpu", **kw)


def test_seeded_source_binding_mask_capacity_and_replay():
    records = []
    env = CombatEnv([case()], limit=5, sink=records.append)
    obs, _ = env.reset(options={"case": case()})
    assert env.observation_space.contains(obs)
    source = env.revision
    with pytest.raises(ValueError, match="source mask"):
        env.step(31)
    for _ in range(5):
        choice = int(np.flatnonzero(env.obs["action_ids"][:, 0] == 2)[0])
        env.step(choice)
    assert records[0]["status"] == "unknown_time_limit"
    assert env.observation_space.contains(env.obs)
    assert env.obs["entity_values"][0, 5] > obs["entity_values"][0, 5]
    with pytest.raises(ValueError, match="reset"):
        env.step(0)
    replay = CombatEnv([case()], limit=5)
    replay.reset(options={"case": case()})
    for step in records[0]["trace"]:
        assert fingerprint(replay.obs) == step["observation"]
        replay.step(step["ordinal"])
    assert json.loads(replay.core.evidence_json()) == records[0]["evidence"]
    with pytest.raises(ValueError, match="stale"):
        replay.core.advance(source, [0])
    overflow = CombatEnv([case()], entities=1)
    with pytest.raises(ValueError, match="capacity exceeded"):
        overflow.reset(seed=3)
    env.close(); replay.close(); overflow.close()


def test_true_terminal_hp_reward_and_unknown_rejection():
    # This extreme card is a reporting unit test only, never a task recipe.
    recipe = case([["SearingBlow", 10]])
    env = CombatEnv([recipe])
    obs, _ = env.reset(seed=31)
    action = int(np.flatnonzero(obs["action_ids"][:, 0] == 1)[0])
    after, reward, terminated, truncated, info = env.step(action)
    assert terminated and not truncated and reward == 74 / 75
    assert info["outcome"]["win_replayed"]
    assert env.observation_space.contains(after)
    assert terminal_reward({"kind": "loss"}) == -1
    with pytest.raises(ValueError, match="Unknown"):
        terminal_reward({"kind": "unknown_decision_limit"})
    records = []
    env = CombatEnv([case()], sink=records.append)
    env.reset(seed=31)
    env.core = GuidedCombatBatch.from_specifications([env.case["spec"]], 1)
    obs = env._observe()
    with pytest.raises(ValueError, match="Unknown"):
        env.step(int(np.flatnonzero(obs["action_ids"][:, 0] == 2)[0]))
    assert records[0]["status"] == "unknown_step_error"


def test_masked_sampling_recompute_variable_rows_and_action_permutation(tmp_path):
    env = CombatEnv([case([["ShrugItOff", 0], ["Strike", 0], ["Defend", 0]] * 2)])
    policy = model(env)
    obs, _ = env.reset(seed=29)
    tensor, _ = policy.policy.obs_to_tensor(obs)
    with torch.no_grad():
        action, values, logp = policy.policy(tensor)
        recomputed_values, recomputed_logp, entropy = policy.policy.evaluate_actions(tensor, action)
        assert obs["mask"][int(action)]
        torch.testing.assert_close(logp, recomputed_logp)
        torch.testing.assert_close(values, recomputed_values)
        assert torch.isfinite(entropy).all()
        probs = policy.policy.get_distribution(tensor).distribution.probs.clone()
        assert (probs[0, ~tensor["mask"][0].bool()] == 0).all()
        perm = torch.arange(int(tensor["mask"].sum()) - 1, -1, -1)
        shuffled = {k: v.clone() for k, v in tensor.items()}
        for key in ("action_ids", "actions", "action_refs"):
            shuffled[key][:, :len(perm)] = shuffled[key][:, perm]
        changed = policy.policy.get_distribution(shuffled).distribution.probs
        torch.testing.assert_close(changed[:, :len(perm)], probs[:, perm])

    class CheckedPPO(PPO):
        def train(self):
            self.policy.set_training_mode(True)
            for batch in self.rollout_buffer.get(4):
                with torch.no_grad():
                    _, actual, _ = self.policy.evaluate_actions(batch.observations, batch.actions.flatten())
                torch.testing.assert_close(actual, batch.old_log_prob, atol=2e-5, rtol=2e-5)
            super().train()

    policy = CheckedPPO(CombatPolicy, env, n_steps=16, batch_size=4, n_epochs=1,
                        gamma=1., seed=11, device="cpu")
    before = copy.deepcopy(policy.policy.state_dict())
    policy.learn(total_timesteps=32)
    assert any(not torch.equal(before[k], v) for k, v in policy.policy.state_dict().items())
    policy.save(tmp_path / "model")
    restored = PPO.load(tmp_path / "model", device="cpu")
    restored.policy.set_training_mode(False)
    policy.policy.set_training_mode(False)
    with torch.no_grad():
        p = policy.policy.get_distribution(tensor).distribution.probs.clone()
        q = restored.policy.get_distribution(tensor).distribution.probs
    torch.testing.assert_close(p, q, atol=0, rtol=0)
    env.close()


def test_sb3_bootstraps_real_successor_on_external_time_limit():
    records = []
    env = CombatEnv([case()], limit=1, sink=records.append)
    wrapped = DummyVecEnv([lambda: env])
    policy = model(wrapped)
    forward = policy.policy.forward

    def end_turn(obs, deterministic=False):
        _, values, _ = forward(obs, deterministic)
        choices = (obs["action_ids"][:, :, 0] == 2).long().argmax(1)
        return choices, values, policy.policy.get_distribution(obs).log_prob(choices)

    policy.policy.forward = end_turn
    # The true successor is turn 2, whereas DummyVecEnv has already reset to 1.
    policy.policy.predict_values = lambda obs: obs["entity_values"][:, 0, 5:6] * 20
    policy.learn(total_timesteps=8)
    np.testing.assert_allclose(policy.rollout_buffer.rewards, 2.)
    assert all(r["status"] == "unknown_time_limit" for r in records)
    assert all(r["evidence"]["episodes"][0]["outcome"] is None for r in records)
    wrapped.close()
