# Batched game API

[Python interface](../README.md)

## Create an environment

Create an A20 Heart environment with `LearningBatchEnv(seeds, 20, final_act=True, candidate_surface="legal")`. `player_class` accepts `ironclad` (default), `silent`, `defect` and `watcher`; `neow_eligibility` accepts `rich` (default) and `mini`. Slot resets retain these run settings. Role-specific mechanics and original-game coverage are tracked by the acceptance artifacts.

## Choose and commit

`decision_batch()` returns current candidate counts and decision tokens. `choose(tokens, ordinals)` advances a root or symbolic selection; it leaves game state unchanged. Once every active slot is ready, `step()` commits the chosen actions and returns the actual transition, including `reward` and `terminated`. Read the new decision after each commit. Selecting cards can require several `choose` calls before one `step`.

The candidate surface is explicit:

- `legal` exposes every represented legal choice, including reward order,
  potion discards and reversible map navigation.
- `policy` is the existing default for research callers. It applies the task's
  reduced candidate admission, including reward-order restrictions.

Both surfaces retain explicit potion-use restrictions. Candidate admission does not change engine legality. Decision tokens bind the surface; checkpoints preserve it and reject restoration into a different surface. Batch and bank payloads use format 4 and reject earlier formats.

The `policy` strategic candidates omit potion discards with an empty slot, with Sozu, or at a map decision, where discarding cannot make room for an immediate acquisition. Full-slot replacement remains available at reward and shop boundaries. The simulator's legal actions and human run-play controls are unchanged.

See [observations](observations.md) for the NumPy tables and public feature contract.

## Transition and terminal results

Run-level `step()` returns the native V3 transition columns: source decision token ids, ragged action ordinal paths, rewards, next-token availability/ids, next public snapshot ids and decision steps, plus the existing typed terminal facts. These are produced by the Rust mutation owner in the same operation; replay and experience code do not infer transitions by joining adjacent Python batches.

Terminal step batches retain aligned `terminal_slot_indices`, `terminal_reward`, public `terminal_act`, `terminal_floor`, `terminal_hp`, `terminal_max_hp`, and `terminal_gold` columns. These raw outcome facts support progress and lower-tail auxiliary targets without turning HP or gold into a handcrafted reward.

## Checkpoints and reset

`checkpoint_slot(slot_index)` returns an opaque, in-process exact checkpoint; `restore_slot(slot_index, checkpoint)` restores both the simulator and any unfinished symbolic-selection prefix without rerolling RNG. Checkpoints are created only on an explicit caller request and do not form an automatic history. A recovery curriculum therefore owns checkpoint retention, retry counts, and replacement policy without becoming part of environment mechanics.

Cross-process callers may explicitly snapshot the complete fixed batch with `checkpoint_bytes(max_bytes=...)` and construct a fresh owner with `LearningBatchEnv.from_checkpoint_bytes(payload, expected_slots=..., max_bytes=...)`. The versioned opaque payload contains each exact run-control session plus only the candidate-ordinal prefix needed to rebuild bridge-local symbolic selection or a prepared action. Restore requires the exact slot count, checks the caller byte limit before decoding, reconstructs every environment, and replays every ordinal through the current typed decoder before exposing the first slot. A corrupt, stale, or no-longer-legal prefix therefore fails closed. The bridge does not inspect the payload in Python, use pickle, write files, or publish an automatic checkpoint stream.

For vectorized curricula, `checkpoint_slots(slot_indices)` and `restore_slots(slot_indices, checkpoints)` perform one cross-language call for the whole selected subset. Restore validates every target, reconstructs every environment, and observes every replacement before changing the first slot; an invalid batch therefore cannot leave a partially restored pool. An opaque checkpoint batch can select an ordered slot subset and return an updated copy with replacement checkpoints for existing slots. This lets a continuous caller retain one bounded episode-root bank across asynchronous slot resets without inspecting sessions or falling back to per-slot bridge calls. Selection rejects duplicate or missing slots, and update never adds a new slot implicitly. An explicit process-resume owner may serialize that bank with `checkpoint_bytes(max_bytes=...)` and recover it through `LearningCheckpointBatch.from_checkpoint_bytes(payload, expected_slot_indices=..., max_bytes=...)`. Restore validates the exact ordered slot identities and reconstructs every private bridge state before returning the bank. The bank format has its own magic and version, so a current-environment snapshot cannot be mistaken for an episode-root bank. `reset_slots(slot_indices, seeds)` applies the same all-or-nothing rule when a caller starts new episodes in terminal slots. Continuous callers use `reset_slots_checkpointed(slot_indices, seeds)` to receive the exact new root checkpoints from that same atomic bridge operation; there is no interval where new episode seeds are active but only old roots are available. Every recovery checkpoint is bound to the slot that created it. The ordinary restore API rejects cross-slot cloning so recovery counts cannot be attached to the wrong environment lineage.

Policy callbacks and execution limits are shown in the [policy walkthrough](../../../docs/getting-started.md#connect-a-policy).
