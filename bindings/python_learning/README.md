# SpireSim Python interface

[简体中文](README.zh-CN.md) | English

Run batched Slay the Spire episodes from Python. SpireSim executes the game in Rust; your policy receives public observations and chooses legal candidates. The environment supports all four characters, Ascension, and Act 4.

## Install

Download the wheel for your platform from the [release page](https://github.com/hewzhew/SpireSim/releases), then install it with your training interpreter:

```text
python -m pip install PATH_TO_WHEEL
```

The Python package is named `sts-learning-bridge` and imported as `sts_learning_bridge`. NumPy is installed as a dependency. Training runs without Java or the original game.

The [installation guide](../../docs/getting-started.md) lists supported prebuilt wheels and complete-run verification commands. To build another wheel, see [source builds](docs/building.md).

## Connect a policy

```python
from sts_learning_bridge import LearningBatchEnv, semantic_schema

env = LearningBatchEnv([42, 43], 20, final_act=True,
                       player_class="silent", candidate_surface="legal")
batch = env.decision_batch(semantic=True)
schema = semantic_schema()
```

Implement `choose(batch, schema)` to return one candidate ordinal per current decision row. The maintained [policy loop](examples/policy_loop.py) handles selection prefixes, commits and terminal slots:

```text
python bindings/python_learning/examples/policy_loop.py --policy my_policy:choose --seeds 42 43 --character silent
```

See the [policy walkthrough](../../docs/getting-started.md#connect-a-policy) for scoring and execution limits.

## Reference

| Task | Guide |
| --- | --- |
| Create batches, choose actions, read rewards, restore state | [Batched game API](docs/api.md) |
| Interpret the public NumPy tables and feature coverage | [Observations](docs/observations.md) |
| Compare a recorded trajectory and an installed package | [Trajectory verification](docs/verification.md) |
| Build wheels and check a local installation | [Building](docs/building.md) |
| Use exact-combat, constructed-case or guided-search experiments | [Research interfaces](docs/research.md) |

The package contains `src/` for the native binding, `python/` for Python exports, `examples/` for policy integration, and `tests/` for binding and trajectory checks. Policy optimization and training callers live in [learning/](../../learning/README.md).
