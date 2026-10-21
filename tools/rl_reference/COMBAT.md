# Constructed combat learning diagnostic

This entry connects Stable-Baselines3 PPO 2.7.0 to the current Rust
[`GuidedCombatBatch`](../../bindings/python_learning/src/guided.rs) and
[entity schema 4](../guided_search/ENTITY_LEARNING.md). It runs fresh explicit
constructions; it has no gameplay session, Java control or certification authority.
PPO rollout collection, GAE, clipping and optimization remain upstream. The
small-map baseline's weights and exact value tables are not reused.

## Adapter and policy ownership

[`combat_env.py`](combat_env.py) owns Gymnasium transport, episode selection,
external sampling limits and the declared reward. Rust owns card mechanics,
source-bound action enumeration, execution and terminal observations. The direct
batch internally follows the supplied action through its current execution
component; this experiment requests no search, alternative proposals or labels.

Every observation contains all schema-4 entities and legal action rows. Fixed
Gymnasium capacities pad these rows, with explicit entity/action masks; overflow
is an error rather than truncation. Absent references use the encoder's zero
token, and present references remain local to their own observation. No source
revision, game seed, RNG state or lineage identifier becomes a model feature.
Information remains oracle-scoped, not a certified NoSL or full Markov contract.

[`combat_policy.py`](combat_policy.py) adapts the existing randomly initialized
entity actor/critic to SB3's policy methods. It trims only padding during a batch
forward pass, produces one logit per legal action and assigns zero probability
to padding. Sampling and PPO probability recomputation use the same observation
and mask from the rollout buffer. It does not implement another PPO optimizer.

Real victory/death terminates an episode. The terminal observation returned to
Gymnasium is an absorbing transport placeholder that cannot be stepped; actual
terminal state remains in Rust evidence. The external decision limit instead
returns the live numeric successor and `truncated=True`. SB3 can bootstrap its
value estimate from that successor, while evidence records an unresolved case.
A Rust error, unexpected core limit or encoding overflow aborts collection;
it cannot be converted to a terminal loss. An interrupted rollout is not updated.
Closing/resetting a live episode records its unfinished trajectory.

## Task, feedback and scope

[`combat_trial.py`](combat_trial.py) predeclares recipes, game-seed partitions,
initializations and budgets before creating weights. The diagnostic uses small
Bash/Bludgeon/Strike/Defend/Shrug It Off decks against Jaw Worm and Gremlin Nob,
with different starting HP. Actual combat initialization, shuffle, enemy AI,
Enrage, energy, block, damage and victory healing run through Rust.

These reduced decks are controlled constructions, not examples of naturally
acquired Act 1 resources. A new recipe and a starter-plus-Bludgeon stress group
are reported separately. This task contains no potions and therefore cannot
test potion valuation or retention. It does not train route/deckbuilding choices.

The reward is terminal HP / 75 for a replayed win, -1 for actual death, and zero
between decisions, with gamma 1. Terminal HP includes capped Burning Blood
healing. Card preferences, search scores and demonstrations supply no training
labels. This is one explicit combat objective, not a validated whole-run utility
or a claim that survival and HP have a universal exchange rate.

Training uses one fixed budget per initialization. Periodic development
evaluation neither selects checkpoints nor triggers tuning. Initial/final
evaluation distinguishes greedy and sampled policies, unseen game seeds, held
recipes and starter stress. Independent sampling RNG does not consume the
training action RNG. Outcomes, HP distribution, winning combat damage and
individual choices are retained; a mean over winning episodes has a changing
survivor population and is not by itself evidence of improved efficiency.

The damage summary is specific to these non-healing, potion-free recipes with
starting HP <= 68: a win's damage equals initial HP + 6 - final HP. Do not reuse
that formula for arbitrary loadouts. Unknowns remain a separate count and are
excluded from terminal-utility averages, not silently assigned zero utility.

## Evidence and diagnostic interventions

Every completed or unfinished training episode saves its construction, numeric
observation digests, chosen ordinals, raw root, inputs, successor and outcome.
Evaluation additionally saves full decision readings, action probabilities and
critic estimates. Producer records pin Python sources, upstream implementation,
runtime packages and the installed native binary's hash. They do not attest that
an existing binary was rebuilt from the adjacent working tree.

[`combat_verify.py`](combat_verify.py) requires matching producer identities,
reloads each initial/final model and replays all held evaluation trajectories.
It compares observations, probabilities, values and complete Rust evidence.

[`combat_deviations.py`](combat_deviations.py) is an independent behavior audit.
Its mechanical selection rule takes each greedy evaluation trajectory's first
attack/defense choice under incoming damage and first single-card lethal with
defense available. From each source it tries every legal first action, then
continues with the same frozen greedy policy. Original prefixes are reconstructed
and checked; every alternative's trajectory is saved. An original action must
recover the original endpoint. Alternatives with Unknown outcomes are not ranked.

Differences are **one-action deviations under that policy's continuation**. They
are not optimal action values, human-quality scores, training labels or proof
that a first action is inferior under every possible future strategy. States
visited by the model also omit potentially important states it never reaches.
The audit distinguishes attack-favored, defense-favored and tied continuations;
success counts alone do not admit a model as a competent combat controller.

## Commands

Use the dedicated reference Python environment with the current Rust learning
bridge available. No Java launch or additional model provider is required.

```powershell
.\scripts\py.ps1 -Python <reference-python.exe> -m pytest tools/rl_reference/test_combat.py tools/rl_reference/test_combat_deviations.py -q
.\scripts\py.ps1 -Python <reference-python.exe> -u -m tools.rl_reference.combat_trial <fresh-output-directory> --wall-seconds 1200
.\scripts\py.ps1 -Python <reference-python.exe> -u -m tools.rl_reference.combat_verify <trial-directory>
.\scripts\py.ps1 -Python <reference-python.exe> -u -m tools.rl_reference.combat_deviations <trial-directory> <fresh-audit-directory> --wall-seconds 300
```

This runner does not choose a final architecture or automatically promote a
checkpoint. The separate [continuous-segment interface](../guided_search/CONTINUOUS_SEGMENTS.md)
retains its own learning/update contract.
