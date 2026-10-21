"""Plot recorded learning curves without running policies or changing results."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt


def plot(directory):
    report = json.loads((directory / "report.json").read_text())
    plan = json.loads((directory / "plan.json").read_text())
    destination = directory / "learning-curves.png"
    if destination.exists():
        raise ValueError("plot already exists")
    fig, axes = plt.subplots(1, 3, figsize=(12.6, 3.7), sharey=True, layout="constrained")
    titles = ("Visible cue", "Delayed terminal reward", "Stochastic transitions")
    for ax, task, title in zip(axes, plan["tasks"], titles):
        for seed in plan["initializations"]:
            path = directory / f"{task['name']}-{seed}" / "curve.json"
            if path.exists():
                rows = json.loads(path.read_text())
                ax.plot([r["steps"] / 1000 for r in rows],
                        [r["sampled"]["optimal_fraction"] for r in rows], marker=".", label=f"init {seed}")
        ax.axhline(task["minimum_optimal_fraction"], color="#999999", linestyle="--", linewidth=1)
        ax.set(title=title, xlabel="Environment transitions (thousands)", ylim=(0, 1.05))
        ax.grid(alpha=.2)
    axes[0].set_ylabel("Exact policy return / optimal return")
    axes[-1].legend(loc="lower right", frameon=False)
    fig.suptitle("SB3 PPO from random initialization | small diagnostic tasks", fontsize=13)
    fig.savefig(destination, dpi=160)
    plt.close(fig)
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    print(plot(parser.parse_args().directory))
