# Combat quality observations

The [quality owner](combat_quality.py) reads actual outcomes and resource evidence.
It supports assessment of [supplied combat policies](ENTITY_LEARNING.md), not a
new learner, strategic policy, reward score or continuous-run environment.
Survival is a minimum capability check; it cannot stand in for practical combat
quality. A resource report alone also does not change what a model optimizes.

## Boundary and facts

`project_episode` retains entry and observed-ending HP, max HP, gold, outgoing
potion types by slot, complete relic records, energy master, master-deck snapshot,
pending permanent changes/rewards and room flags. It checks outcome HP, inventory,
source seed and victory replay against the raw evidence. Unknown endpoints remain
unfinished; their present resources cannot be treated as terminal resources.
HP includes actual victory healing. Entry minus exit HP is a net change, not total
damage. Relic records include counters rather than only display names.

This boundary precedes run settlement. In particular, `pending_meta_changes`
are the engine's ordered records, not a Python reconstruction of the final deck.
The actual [run loop](../../src/engine/run_loop.rs) owns player absorption, potion
and RNG handoff, permanent changes, rewards and room/event continuation.
The raw evidence remains available through source hashes. The projection does
not reproduce a complete RunState or establish identical future continuations.

`compare_observations` requires the same complete root and entry resources.
It distinguishes survival changes, unresolved cases, equal recorded resources,
HP changes with other recorded resources equal, and multiple resource changes.
Two potions of different types, a changed relic counter, or a pending permanent
card change cannot silently become “the same resources.” Future RNG differences
are flagged separately; whole-run dominance is not inferred even for a favorable
HP difference. Low-HP synergies and future opportunities can affect preferences.

## Explicit resource requirements

An evaluation plan can supply `quality_requirements`, for example:

```json
[
  {"id": "leave45", "min_hp": 45, "reserve": "unrestricted"},
  {"id": "leave45_with_explosive", "min_hp": 45, "reserve": {"ExplosivePotion": 1}}
]
```

These are questions asked of observed outcomes, not restrictions installed on
the policy and not universal passing thresholds. `starting_inventory` requires
the outgoing inventory to contain at least the starting multiplicity of each
potion type. Explicit dictionaries specify types and multiplicities. They do not
require the same potion instance to survive or penalize uses: using and later
regenerating a potion can meet the outgoing requirement. `unrestricted` imposes
no potion requirement. An empty starting inventory makes retention vacuous.

For each requirement, report met, missed and unresolved over all declared starts.
Losses are misses; unfinished attempts stay unresolved. Fraction bounds span
unresolved observations only; they are not statistical confidence intervals or
proofs of a policy's true success probability. No HP-to-potion exchange rate is
introduced, and conflicting resource requirements do not receive an automatic
overall ranking.

Groups retain the exact construction configuration except seed, including entry
HP, deck order, upgrades, relics, potion types and use limits. Actual enemy
compositions separate pool members. Different entry resources must not disappear
inside an encounter average. Paired observations describe realized continuations;
even matching sampling seeds do not prove a difference in expected policy value.

## Evaluation and read-only review

[The shared evaluator](natural_imitation.py) saves these observations in its rows
and writes `quality.json`. With no declared requirements, it still saves resources
and exact context groups, without inventing thresholds. Existing survival/resource
summary fields remain available as narrow diagnostics.

To inspect a recorded schema-4 development trial, provide a JSON list of explicit
requirements and a fresh output directory:

```powershell
.\py.ps1 -m tools.guided_search.combat_quality <trial-directory> <fresh-output> --requirements <requirements.json>
```

This reads both endpoints for every declared model and case. It checks the
construction, outcome and raw evidence against the existing report, pins source
file hashes and saves its parser source. Missing unattempted endpoints remain
unresolved; a claimed terminal without its evidence is an error. It does not
import old roots, run policies, resample cases, rewrite the experiment plan or
generate teacher labels. Once inspected, these cases are development material.

## Candidate learning connection

An actual ending supplies more than a single victory bit: it indicates which
explicit resource requirements that continuation met. A candidate outcome model
could predict this joint resource distribution, or requirement attainment,
conditional on the permitted observation and the continuation policy. This
would preserve differences between victories at different resource costs.
The single-combat quality reader only assesses evidence; it does not train these
models. The separate conditional segment interface below implements an initial
objective-conditioned actor/critic experiment, without claiming useful learning.

This is not sufficient by itself. A policy still needs a declared preference or
context for selecting among risk/resource tradeoffs. The numerical representation,
exploration and temporal credit assignment require their own checks. Predicting
these outcomes does not prove that policy updates will improve them.

For a continuous short segment, the same run owner must carry HP/max HP, gold,
exact potions, relic states, permanent deck changes, RNG and room context into the
next encounter. Resetting a `ConstructedCombat` and copying two resource fields
does not satisfy that contract. The chosen route, available recovery/rewards and
permitted future information must be explicit. Segment quality remains an
approximation to whole-run value and cannot establish the
[natural early-elite checkpoint](EARLY_ELITE.md) without its complete prefix.

The [supplied continuous segment experiment](CONTINUOUS_SEGMENTS.md) implements
this execution path for declared Act 1 corridors through the same resident run
owner. Assisted collection records actual settlements and explicit reward choices.
Its separate [conditional learning interface](CONTINUOUS_SEGMENTS.md#conditional-learning-interface)
provides future-route/resource context and one sampled policy update against an
assigned endpoint requirement. Between-room choices are fixed and disclosed;
the finite task does not supply a learned strategic objective or establish what
resources should be preferred over the rest of a run.
