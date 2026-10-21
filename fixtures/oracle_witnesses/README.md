# Fixed-root combat witnesses

This directory contains exact combat roots and action witnesses for bounded,
combat-local regression. A checked-in action file proves only that its paired
exact combat root can replay that line under the maintained simulator.

It does not prove that the current run owner reaches the root, that the route
or deck history which once produced it is still valid, or that bounded search
must rediscover the line.

## Run-level evidence boundary

The pre-2026-08-05 full-run continuations, workspaces, replay reports, and
seed006--015 suite were removed after commit `97581909` restored Java's
per-floor RNG lifecycle. Those runs were generated while room travel advanced
`floor_num` without reseeding the floor RNG streams, so even their first combat
opening hands no longer replay under the corrected mechanics.

Do not reconstruct a run-level baseline from these remaining combat fixtures.
A new run-witness suite must be generated from fresh floor-zero runs after the
RNG correction and must pass exact replay before it is described as current.

## Remaining fixture groups

- `seed20260713006_a0_awakened_one.*`: one Awakened One root and exact action
  witness.
- `seed20260713007_a0_awakened_one*`: several fixed-root Awakened One
  witnesses retained for combat-search comparison.
- `seed20260713007_a0_relevant_capability_awakened_one*`: a fixed boss root,
  donor/action variants, and the combat-guidance corpus entry that references
  them. Their provenance labels are historical; runtime policy must not read
  these files.
- `seed20260713008_a0_*donu_deca*`: two Donu and Deca roots with exact local
  graph, proof-cache, or policy-discrepancy action witnesses.
- `seed20260713009_a0_slime_boss*` and
  `seed20260713009_a0_time_eater*`: fixed boss roots with exact local-turn
  graph witnesses.

Use current combat-case import/replay tooling for these roots. If a fixture no
longer replays exactly, treat it as stale evidence and either regenerate it
from an unchanged exact root or delete it; do not add compatibility guesses.
