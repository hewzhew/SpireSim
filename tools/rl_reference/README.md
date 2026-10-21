# Reference learning diagnostics

The original small-task experiment exercises the published Stable-Baselines3 PPO
implementation before adapting a learner to card combat. It does not use StS
states, search, an external model, demonstrations or policy labels. No weights
from these tasks are promoted to gameplay or continuous-segment learning.

A separate [constructed combat diagnostic](COMBAT.md) connects the same upstream
learner to real Rust combat, with entity observations and legal action masks.
Its results and admission boundaries are independent of the small-map baseline.

## Reference and ownership

PPO sampling, GAE, optimization, clipping and timeout bootstrap are owned by
`stable-baselines3==2.7.0`. The pinned implementation is used directly, not a local
rewrite of its loss. This is a small diagnostic application, not a reproduction
of a published Atari score, the whole bsuite benchmark or a claim that PPO is the
right final StS algorithm.

- [SB3 PPO](https://stable-baselines3.readthedocs.io/en/v2.7.0/modules/ppo.html)
- [RL environment and implementation advice](https://stable-baselines3.readthedocs.io/en/v2.7.0/guide/rl_tips.html)
- [PPO implementation details](https://iclr-blog-track.github.io/2022/03/25/ppo-implementation-details/)
- [Behaviour Suite methodology](https://github.com/google-deepmind/bsuite)

[`tasks.py`](tasks.py) owns the declared diagnostics. A one-step binary-cue task
rewards matching the visible cue. Two Gymnasium FrozenLake variants use the same
six-cell map, with deterministic movement or stochastic slipping:

```text
S F F
F H G
```

S is the start, F safe ice, H a terminal hole, and G the terminal goal. FrozenLake
mechanics remain owned by `gymnasium==1.2.0`; only its documented layout,
slipperiness and time-limit parameters are set. FrozenLake provides only the
terminal goal reward. These tasks diagnose context-dependent choice, delayed
reward and stochastic transitions; they do not reproduce combat mechanics.

## Objective, boundaries and assessment

Training optimizes expected discounted terminal reward with gamma 0.99. Time
limits are external sampling cuts, not task failures. SB3 bootstraps from the
final pre-reset observation at a truncation and cuts continuation at a true
terminal. A targeted integration test checks the installed library's actual
rollout buffer, using deliberately different terminal and reset observations.
This handling does not change the existing StS endpoint-evidence contract.

[`evaluation.py`](evaluation.py) obtains each frozen policy's full probability
table and uses the same Gymnasium transition kernel to calculate:

- Expected discounted return, independent of a lucky set of sampled episodes.
- Success, failure and still-unresolved probability at the declared time limit.
- Current-policy state values and visitation-weighted critic error.
- A converged optimal discounted reference, with its Bellman residual.

The optimal reference and policy evaluation stay outside training. They never
provide labels, rewards, gradient terms or actions to the learner. A separate
set of actual episodes retains every source/action/successor and timeout.
Evaluation uses local RNG and cannot consume the training action RNG.

The map is fixed: evaluation episode seeds test repeated stochastic executions,
not unseen layouts or unseen mechanics. Multiple initializations test training
variability on these small tasks. Neither exact small-state evaluation nor a
high completion rate implies StS skill or broad generalization. A deterministic
greedy controller and the sampled training policy are reported separately.

## Running a bounded trial

Use a dedicated Python environment to avoid changing the simulator's dependencies.
The exact resolved packages and interpreter are recorded by each run. Install the
pinned packages in [requirements.txt](requirements.txt), with PyTorch already
available in that environment:

```powershell
.\py.ps1 -Python <reference-python.exe> -m pip install -r tools/rl_reference/requirements.txt
.\py.ps1 -Python <reference-python.exe> -m unittest tools.rl_reference.test_reference -v
.\py.ps1 -Python <reference-python.exe> -u -m tools.rl_reference.trial <fresh-output-directory> --wall-seconds 600
.\py.ps1 -Python <reference-python.exe> -m tools.rl_reference.plot <trial-directory>
.\py.ps1 -Python <reference-python.exe> -m tools.rl_reference.verify <trial-directory>
```

The runner writes its entire task/budget/seed/criterion plan before model creation,
pins local and upstream learner sources, and records dependency versions. It uses
fresh weights for each declared task and initialization, fixed hyperparameters,
fixed sampling budgets, periodic exact diagnostics and final held-out episodes.
Diagnostic curves do not choose checkpoints or trigger hyperparameter retries.
Acceptance uses the final sampled policy and the task's predeclared threshold;
these are local sanity criteria, not published benchmark standards.

Training CSVs retain losses, entropy, KL, clipping and value diagnostics. Initial
and final model files, policy tables, critic estimates and all evaluation traces
remain in the fresh artifact directory. Every declared case is attempted once;
interruptions retain completed cases, partial progress, attempts and unattempted
cases separately. The wall limit cooperatively stops rollout collection; it is
not a hard process deadline or an excuse to count unresolved episodes as losses.

The small-map diagnostics do not test ragged entity inputs, dynamic action masks, long
sparse combat, potion/resource valuation or composition across rooms. Adapting
the same upstream learner to those conditions requires separate admission and
evaluation; the existing [conditional segment interface](../guided_search/CONTINUOUS_SEGMENTS.md)
does not acquire PPO behavior just because this reference trial succeeds.
