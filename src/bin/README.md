# Binary Entrypoints

`src/bin` is only for maintained command entrypoints. Library ownership belongs
in `src/ai`, `src/eval`, `src/testing`, `src/state`, or another library module.
One-off experiments should not grow long-lived binaries here.

These binaries are Cargo targets of the library-free `sts_oracle_tools`
command package, even while their physical source paths remain under
`src/bin`. Run or build them with `-p sts_oracle_tools`; semantic code belongs
in a dedicated capability crate, `sts_oracle_runtime`, or a lower crate.

Use [../../docs/RUNBOOK.md](../../docs/RUNBOOK.md) for command examples. This
file only records binary ownership and boundaries.

## Active Binaries

| Binary | Boundary |
| --- | --- |
| `combat_case_review` | Review ladder for saved fixed-root `CombatCase` artifacts; CLI owns IO, `combat_case_review/review_pipeline.rs` owns probe orchestration. |
| `combat_search_v2_driver` | Thin compatibility adapter for the fixed-root `AtomicExactV2` driver; routine use goes through its lightweight frontend and dedicated worker, not the resident oracle portfolio. |
| `rl_dataset_export` | Offline decision-sample export for imitation/RL experiments, including typed planner capture datasets and coverage reports from `SessionTraceV1`; exported behavior is not truth. |
| `oracle_run` | Bounded exact run explorer and continuation adapter over the runtime-owned oracle-run API. |

## Ownership Rules

- `sts_combat_search_driver` (and the thin standalone compatibility adapter)
  plus `combat_case_review` are combat investigation tools. They do not decide
  non-combat policy. Combat Laboratory and Campfire
  Threat Panel artifacts are descriptive evidence and never feed live policy
  automatically.
- `rl_dataset_export` accepts only the current typed `SessionTraceV1`; it must
  not reinterpret retired branch artifacts or invent policy conclusions.

## File Boundaries

For large binaries, keep parsing, command selection, input conversion,
artifact IO, and dispatch separate:

- parsing/defaults belong near CLI args,
- command selection belongs near command dispatch,
- command handlers should take narrow input structs,
- artifact readers/writers should not own policy,
- `main.rs` should stay dispatch-oriented.

If a binary needs enough internal structure to require a second map, move the
shared logic into a library module and keep this README short.
