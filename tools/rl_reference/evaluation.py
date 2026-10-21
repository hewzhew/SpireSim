"""Read-only exact/empirical diagnostics; no evaluation targets enter PPO."""
import numpy as np
import torch

from .tasks import kernel, make


def policy_table(model, task):
    env = make(task)
    observations = np.arange(env.observation_space.n)
    env.close()
    training = model.policy.training
    try:
        model.policy.set_training_mode(False)
        with torch.no_grad():
            obs, _ = model.policy.obs_to_tensor(observations)
            probabilities = model.policy.get_distribution(obs).distribution.probs.cpu().numpy()
            values = model.policy.predict_values(obs).flatten().cpu().numpy()
        return probabilities.astype(np.float64), values.astype(np.float64)
    finally:
        model.policy.set_training_mode(training)


def exact(task, probabilities, gamma=.99):
    initial, transitions, successes, failures = kernel(task)
    if (probabilities.shape != successes.shape or not np.isfinite(probabilities).all()
            or np.any(probabilities < 0) or not np.allclose(probabilities.sum(-1), 1, atol=1e-6)):
        raise ValueError("invalid reference policy table")
    # Float32 softmax rows may not sum to exactly one after conversion.
    probabilities = probabilities / probabilities.sum(-1, keepdims=True)
    t = np.einsum("sa,san->sn", probabilities, transitions)
    win, lose = (np.sum(probabilities * x, axis=1) for x in (successes, failures))
    v = np.linalg.solve(np.eye(len(initial)) - gamma * t, win)
    optimum = np.zeros_like(initial)
    for _ in range(10000):
        next_optimum = (successes + gamma * np.einsum("san,n->sa", transitions, optimum)).max(-1)
        residual = float(np.max(abs(next_optimum - optimum)))
        optimum = next_optimum
        if residual < 1e-12:
            break
    else:
        raise ValueError("optimal reference calculation did not converge")
    alive, visits = initial.copy(), np.zeros_like(initial)
    p_win = p_lose = 0.
    for _ in range(task.horizon):
        visits += alive
        p_win += float(alive @ win)
        p_lose += float(alive @ lose)
        alive = alive @ t
    return {"discounted_return": float(initial @ v), "optimal_discounted_return": float(initial @ optimum),
            "optimal_fraction": float((initial @ v) / (initial @ optimum)),
            "success_within_limit": p_win, "failure_within_limit": p_lose,
            "unresolved_at_limit": float(alive.sum()), "expected_observed_steps": float(visits.sum()),
            "policy_values": v.tolist(), "visit_weights": (visits / visits.sum()).tolist(),
            "optimal_bellman_residual": residual}


def assessment(model, task):
    probabilities, values = policy_table(model, task)
    sampled = exact(task, probabilities)
    greedy = exact(task, np.eye(probabilities.shape[1])[probabilities.argmax(-1)])
    error = np.abs(values - sampled["policy_values"])
    return {"sampled": sampled, "greedy": greedy, "probabilities": probabilities.tolist(),
            "predicted_values": values.tolist(),
            "visited_value_mae": float(error @ sampled["visit_weights"])}


def empirical(task, probabilities, seeds):
    """Fresh native episodes, with local action RNG and explicit timeout outcomes."""
    env = make(task)
    rows = []
    probabilities = probabilities / probabilities.sum(-1, keepdims=True)
    try:
        for seed in seeds:
            rng = np.random.default_rng(seed + 2**32)
            obs, _ = env.reset(seed=seed)
            trace = []
            for _ in range(task.horizon):
                action = int(rng.choice(probabilities.shape[1], p=probabilities[obs]))
                nxt, reward, terminated, truncated, _ = env.step(action)
                trace.append([int(obs), action, int(nxt), float(reward), bool(terminated), bool(truncated)])
                obs = nxt
                if terminated or truncated:
                    ending = "success" if terminated and reward > 0 else "failure" if terminated else "unresolved"
                    break
            else:
                raise ValueError("environment did not expose its declared time limit")
            rows.append({"seed": seed, "ending": ending, "trace": trace})
    finally:
        env.close()
    return {"episodes": len(rows),
            "counts": {k: sum(r["ending"] == k for r in rows) for k in ("success", "failure", "unresolved")},
            "rows": rows}
