# AI Module Map

`src/ai` is a shared source tree for retained policy, strategy, combat
contracts, search, and diagnostics. Directory placement is not compilation
ownership: Cargo packages below deliberately compile disjoint parts of this
tree. It is not the learned-agent owner. Public model state and hidden-future
belief belong in [`src/agent`](../agent/README.md).

## Current Layers

| Area | Modules | Boundary |
| --- | --- | --- |
| card semantics (`sts_card_semantics`) | `analysis`, `card_semantics_v1`, `combat_upgrade_coverage_v1` | Upgrade-sensitive typed mechanics above exact simulator content. |
| combat evidence (`sts_combat_evidence`) | `combat_witness_contract`, `combat_persistent_outcome_v1` | Shared request vocabulary and exact persistent outcome facts; no solver orchestration. |
| stable core contracts (`sts_simulator`) | `combat_state_key`, `decision_tags_v1`, `planner_core` | Exact identity and public protocol types kept independent from evolving strategy. |
| shared run facts (`sts_run_strategy`) | `strategic`, `strategy`, `route_window_facts`, `event_resource_budget` | Public-state facts and reusable strategy inputs. These modules should not click buttons or apply actions. |
| deck mutation (`sts_run_strategy`) | `deck_mutation_compiler_v1` | Shared compiler for remove/transform/sacrifice-style deck changes. Event owners should not copy card-target ordering. |
| non-combat policies (`sts_run_strategy`) | `*_policy_v1`, `noncombat_*` | Scene-specific typed policy adapters. Event choice policy is compiled here, not with simulator content. |
| atomic combat search | `combat_auto_policy_v1`, `combat_search_v2` | `AtomicExactV2` fixed-root diagnostics/challengers and legacy owner-audit support; not the resident oracle portfolio. |

The production resident `TurnGraphPortfolioV1` is composed in run control from
the exact kernels in `crates/sts_combat_planner`. The canonical map is
[`docs/architecture/combat-search.md`](../../docs/architecture/combat-search.md).
| boss matchup work | `boss_matchup`, `boss_mechanics_v1` | Boss-specific facts and review-facing matchup analysis. Keep this evidence-oriented unless a separate policy boundary is explicitly introduced. |

## Naming Notes

Some active modules still end in `_v1`. That suffix is historical, not a license
to add parallel replacements. Prefer improving or retiring an existing boundary
over creating another scene-local copy of the same concept.

Stable public names should not include `_v1` once a boundary is intentionally
promoted. `route_window_facts` and `event_resource_budget` are examples of this
direction; their serialized output can still carry `schema_version = 1`.

## Design Rules

- Typed decisions and candidate ids are control surfaces. Display strings are
  only for humans.
- Shared deck, route, resource, and boss facts belong in analysis/strategy
  modules, not inside one event or shop owner.
- Event owners may decide whether to enter an event option. Card target choice
  belongs in the deck mutation compiler or a typed pending-choice bridge.
- Combat diagnostics may expose symptoms. They must not silently mutate
  non-combat policy.
- Offline learning exports record behavior-policy samples. They are not proof
  that a decision was good.

Neow is deliberately not owned by a handcrafted scalar policy. Its complete
typed option surface, public run state, and map remain available to the run
controller; a model or operator must make the high-agency choice. Do not add a
new automatic Neow ranker by accumulating exception scores or unresolved-case
enums.

## Cleanup Direction

When cleaning this tree, prefer this order:

1. document the current boundary;
2. remove duplicate local logic from scene owners;
3. move shared facts into analysis/strategy/compiler modules;
4. delete the old entrypoint once nothing calls it.

Do not keep compatibility layers for convenience after the replacement boundary
is accepted.
