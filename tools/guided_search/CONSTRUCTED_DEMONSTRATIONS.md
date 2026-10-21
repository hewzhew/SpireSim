# Assisted supplied-combat demonstrations

This bounded Rust-only collection complements [natural demonstrations](NATURAL_DEMONSTRATIONS.md)
when a particular encounter needs more examples. It records explicitly authorized
Codex choices in fresh [declared combat recipes](ENTITY_LEARNING.md#construction).
It provides no search teacher, automatic actor, historical-state importer, natural
run prefix or Java claim. All supplied encounters run at the constructor's A20 setting.

## Collection and provenance

[`Collection`](constructed_demonstrations.py) accepts a plan and a new directory.
The plan declares `actor: "codex_assisted"`,
`information: "oracle_observation_no_search_no_sl"`,
`purpose: "demonstration_development"`, `max_decisions` (1..512 per case),
`max_seconds` (at most four hours), and 1..128 `cases`. Each case has a distinct
`id`, a `family`, explicit `source` provenance, and the complete construction `spec`.
Exact duplicate specs are rejected. Different recipes may share a seed; retain
all such relatives in the same training or evaluation partition.

Declare cases before examining their draws or enemies. HP, deck, upgrades, relics
and potions are recipe interventions, not claims of natural acquisition. Retain
every case, including losses, interrupted cases and poor play. Actor reasons are
annotations; selected actions carry no optimality, value or behavior-probability label.

The collection owns a resident `GuidedCombatBatch` for each case. `observe(episode)`
returns a `source`, shared structured `reading.combat` and `reading.choices`, and
an actual `outcome` if the case has stopped. A live case has no outcome yet.
`GuidedCombatBatch.readings_json()` projects the same active slots and revision as
`decision_batch()`, using the shared combat observation and `ModelChoice` owners.
No JSON label parsing or engine-entity-ID translation is needed to select a card.

```python
from tools.guided_search.constructed_demonstrations import Collection

collection = Collection(plan, new_directory)
r = collection.observe(0)
# Pick a current legal choice after inspecting its combat context.
chosen = r["reading"]["choices"][chosen_ordinal]
collection.choose(0, r["source"], chosen_ordinal, "Reason for this actual choice")
receipt = collection.finish()
```

Choice ordinals and hand/target references belong to this reading. Refresh after
every action. A stale source, absent choice or missing reason cannot mutate the
environment. The wall budget is checked cooperatively before each choice; reaching
it leaves the collection available for `finish()`, including incomplete cases.

Requests are flushed before execution; trajectory rows retain the actual numerical
input, structured reading, choice, materialized inputs and before/after state hashes.
Virtual selection steps may emit no game input; they still have distinct numeric
choices and replay boundaries. Full final native evidence retains roots, recipes and
successors. Source manifests and the native hash identify the producer.

`finish()` reconstructs every declared recipe and replays every choice, comparing
numerical inputs, structured choices, actual inputs and intermediate/final states.
It verifies losses and partial trajectories as well as wins. It does not import
stored roots. A successful receipt covers producer, trajectory and full evidence;
per-case accounting includes unacted cases and `outcome: null` for unfinished ones.

Core or persistence errors seal collection and retain an execution-failure marker
and available native evidence. A process interruption can leave an unmatched request;
the files are diagnostic evidence, not a resumable session. There is no automatic
retry or crash-recovery claim. `verify(directory)` requires the same native and
source producer; preserve that producer when archiving material.

## Learning admission

`load(directory)` checks attribution, receipt/file integrity, numerical action
alignment, continuity and complete declared-case accounting. It returns selected
actions, including those from losses or incomplete games. A successful replay does
not certify decision quality; review that separately before including a source.

The [imitation experiment](NATURAL_DEMONSTRATIONS.md#bounded-imitation-experiment)
accepts an explicitly marked source with `kind: "constructed_assisted"`, the same
file-hash fields as natural sources, and its `source.json` as the source manifest.
Unmarked sources remain natural and cannot silently accept this contract.
Mixed plans require `sampling: "equal_seed_group_full_pass"`: every seed group
gets equal total weight; decisions from all its recipe variants share that weight.
This is not weighting each constructed combat as an independent natural run.
Groups with no recorded choices remain in source accounting and contribute no
training gradient.

The model, input encoding and cross-entropy objective remain shared. Source kinds
stay in the manifest, not policy inputs. No text reason, win filter, invented
behavior probability or automatic label is introduced. Independent evaluation
uses new seed groups and reports actual enemy composition and resources; neither
collection success nor in-sample fitting proves mechanism understanding.

```powershell
.\scripts\py.ps1 -m pytest tools/guided_search/test_constructed_demonstrations.py -q
```
