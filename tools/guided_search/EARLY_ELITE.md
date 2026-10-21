# Early elite learning checkpoint

Status: candidate quality milestone, not yet qualified by an autonomous player.
The [assisted demonstration experiment](NATURAL_DEMONSTRATIONS.md) executes fresh
continuous prefixes with explicit choices; it supplies no learned strategic policy.
The separate [automatic prefix diagnostic](README.md#自动自然前序诊断) uses provisional
setup rules and Oracle combat search. It can execute a fresh prefix, but this does
not establish strategic quality, Seed-SL competence or full-run capability.
The [entity environment](ENTITY_LEARNING.md) supplies constructed combat training;
its victories do not establish this natural-run milestone.

## Objective and permissions

The target is an Ironclad A20 run from Neow through the first early Act 1 elite,
using resources actually acquired along the prefix. Combat success and remaining
resources are measured together. This is a practical checkpoint, not an average
floor target or an A20H capability claim.

The learning system uses dedicated numerical policy/value networks and simulator
outcomes. The user may explicitly authorize Codex-authored demonstrations: preserve
the actor, information access, chosen actions and actual outcomes separately from
autonomous policy evaluation. Such choices are imitation examples, not optimality
or value labels. This does not authorize external LLM providers or an autonomous
LLM teacher pipeline. Legacy search policies and results are not a baseline teacher.
New search is an optional separate
experiment, not a prerequisite or automatically accepted source of labels.

Each experiment declares its information access, replay/SL permissions and total
compute budget. Oracle observation, seed SL and public NoSL results remain distinct.
Reproducible seeds do not grant knowledge of RNG state. All trials and simulation
work used to choose a branch count toward the declared budget.

## Episode boundary and source

The default candidate window is F6–F8, before the fixed Act 1 chest. F7 is not a
guaranteed elite. A different window must be fixed before evaluation.

A fresh initial configuration fixes seed, class, ascension, Heart availability
and Neow eligibility. Every subsequent input is legal in its current source and
recorded. HP, max HP, gold, cards/upgrades, relic counters, potions, RNG streams,
encounter schedule and map position carry through all rooms without replacement.

The success boundary is after defeating the first qualifying elite and settling
its combat effects back into the run, before choosing its rewards. Smoke Bomb or
another escape is not an elite defeat. Resource readings state that they include
actual healing such as Burning Blood; net HP change is not total damage received.

All declared seeds remain accounted for. Distinguish death before the elite,
death during it, defeat of the elite, missed target, no target on the generated
map, budget Unknown and execution/input failure. An avoidable route miss cannot
be renamed map unavailability. Event-room elites and burning elites retain their
actual provenance and modifiers and are reported separately when admitted.

Replays reconstruct a newly produced trajectory from its initial configuration
with the matching producer and actual inputs. They verify exact boundaries and
resources. Historical roots are not imported or given new execution authority.

## Shared execution and neural interfaces

The natural environment should own `RunState`, `EngineState` and `ActiveCombat`,
and use [tick_run_active_with_observer](../../src/engine/run_loop.rs) to preserve
room/event context, real combat settlement and reward generation. It must not
rebuild each room through `ConstructedCombat`, which replaces the deck/resources
and starts separate encounter RNG.

Combat observations and source-bound legal choices should reuse
[sts_guided_search](../../crates/sts_guided_search/src/lib.rs). A native batch
interface can keep episodes resident while Python packs numerical inputs and
updates networks. Research does not require routing every training action through
a terminal, MCP, natural-language command or external provider.

Strategic choices need their own numerical observation/action contract covering
the offered rewards, route, shop, campfire and events. Existing game logic owns
legality and effects. The combat encoder alone cannot make those choices. A fixed
explicit setup policy can help test plumbing, but its assistance must appear in
the report; it cannot be presented as an independently learned Neow-to-elite run.

## Learning and assessment

Constructed contrasts, plausible early loadouts and fresh natural prefixes serve
different purposes. Keep their provenance separate while using them to diagnose
and learn related decisions. Training may oversample rare useful situations;
ordinary-play evaluation uses its own fixed distribution and held-out seed groups.

Complete combat win/loss is a minimum feasibility check. It cannot distinguish
two guaranteed wins that preserve different resources and is not a sufficient
learning objective for this practical checkpoint. Define the resource-quality
objective before optimizing its estimator or expanding training. The
[quality observations](COMBAT_QUALITY.md) expose actual resource differences and
explicit requirement attainment; they do not establish a strategic reward.
Any resource objective must state the tradeoff, use actual outcomes, and retain
survival risk in its assessment.
Immediate block, card count or damage are not default rewards. Potions are ordinary
resources: neither spending one nor retaining one is automatically correct.

The practical report includes all-start success and availability counts, elite
reach rate, conditional elite win rate, per-elite results, entry/exit HP and potions,
resource distributions including poor tails, and inference/simulation costs.
Report uncertainty and all failures/Unknowns. Mean reached floor cannot replace
these measures. Stability thresholds, sample sizes and baseline permissions are
fixed in a versioned experiment plan before evaluating a frozen system.

Supplied-combat results measure tactical competence only. Full checkpoint results
require the complete prefix and explicit ownership of every decision. Simple
diagnostic success, input support and pipeline tests cannot substitute for that
measurement or support an assertion of optimal play.
