# Card Lineage Identity

Status: targeted identity reference. This page does not select a parity
workflow or grant an existing tool execution authority.

`CombatCard` identity models original-game card lineage, not the random bits
of Java's `UUID.randomUUID()`.  Simulator code must never compare a Java UUID
string with a Rust-local integer and treat numeric equality as parity.

## Domain contract

Every Rust card lineage has one explicit origin:

- `master_deck`: a persistent card acquisition in one live `RunState`;
- `combat_generated`: a new independent card created during one combat;
- `constructed_scenario`: a named scenario-instance slot in non-natural evidence.

The origin and ordinal together form `CardLineageId`. Its compact in-memory
representation is private. Serialized evidence and diagnostics expose the
typed origin, so an ordinal is never meaningful outside its origin or owning
runtime. A typed acquisition/generation/scenario-recipe witness may use the order named
by that origin; generic code may not treat it as a Java UUID or compare it
across origins.

Lineage follows the original game's observable UUID behavior:

- copying the master deck into a combat preserves lineage;
- Java `makeSameInstanceOf()` preserves lineage;
- Java `makeStatEquivalentCopy()` receives a fresh combat-generated lineage;
- a newly acquired or duplicated master-deck card receives a fresh
  master-deck lineage;
- transforming a master-deck card replaces its runtime UUID and Rust lineage,
  while the selected source-bound logical deck slot is transferred to the
  single replacement proved by that adjacent typed transition;
- UUID-sensitive effects select or mutate cards by lineage, including every
  battle copy for mechanisms that use `GetAllInBattleInstances`.

Both natural allocators are monotonic state.  Removing the highest-numbered
card never permits reuse.  Allocator state is part of exact checkpoint
identity and must survive simulator checkpoint restore.

## Birth boundary

`CombatCard` means that an instance has already been born and therefore must
carry a valid `CardLineageId`.  A card that is only a future generation recipe
is represented by `CardBlueprint`, which intentionally has no identity.
Generation actions keep the blueprint identity-free until the action resolves;
only then may the owning combat allocator materialize it as a `CombatCard`.
Static questions such as printed cost, Innate, or upgrade eligibility use the
identity-free `*_for(card_id, upgrades)` APIs and must not fabricate a card in
scenario slot zero.

Hand-authored fixtures use the deliberately loud constructor
`CombatCard::for_scenario(card_id, recipe_slot)`.  There is no generic
`CombatCard::new(card_id, number)` compatibility alias.  The number is a
scenario-instance recipe slot local to that constructed scenario, never a Java UUID and never a natural
run acquisition ordinal.

## Java parity binding

The four identities remain separate:

```text
JavaCardUuid             original runtime instance lineage
RustCardLineageId        simulator lineage
StableCardBinding        Rust lineage plus typed original-game provenance
RootActivationLease      current Java UUID ↔ stable binding for one JVM epoch
```

A parity root establishes stable bindings from typed provenance, for example a
master-deck acquisition/order observation, a scenario recipe slot, or a
captured generation action. An activation then leases the current Java UUID to
that binding for exactly one runtime epoch. It does not pair repeated cards
merely because their current combat-zone positions happen to match. Successors
carry the stable binding while validating movement and UUID-sensitive
mutations; a fresh JVM must issue fresh leases.

Pandora's Box removes starter cards and independently constructs a batch of
new cards. Its products have `relic_grant` provenance: the producer room,
`pandorasbox`, and the actual construction ordinal. They do not inherit the
removed cards' bindings. Java records the fresh `makeCopy` products inside
`onEquip`; Rust validates the complete typed removal/acquisition emissions and
reverses the confirmation group's obtain order to recover construction order.
The current deck order only checks the resulting objects. Duplicate card names
remain distinct, and the producer room is retained when acquisition advances
to the next Act.

A Java frame captured without a pre-existing Rust natural run is compiled as
`CapturedScenarioRecipe`. Every distinct captured Java UUID directly declares
one new `constructed_scenario` recipe slot; simultaneous
`makeSameInstanceOf()` copies reuse that slot. The compiler rejects repeated
UUIDs whose stable card semantics conflict, and
`CrossRuntimeCardBindings::from_captured_scenario_recipe` records that scoped
fact as `CardBindingEvidence::CapturedScenarioRecipe`.  This is an artificial
experiment root, not an inferred mapping into some independently simulated
run.  Natural runs are forbidden from entering through this constructor; they
must build bindings from acquisition, copying, generation, or another typed
provenance witness.

Successor zone traversal may only validate bindings that already exist.  A
zone/index `zip` is therefore a consistency check, never a first-binding
algorithm.  A newly observed Java card needs transition-specific generation
evidence; without it the comparison is unsupported/`Unknown`, not a request to
guess by card attributes or current position.

Protocol minor 33 introduced that evidence as an optional
`card_birth_witness_v1` on the stable successor frame. The witness binds the
source and successor frame IDs, request and action IDs, and one closed typed
cause (`card_play`, `potion_use`, `relic_trigger`, `power_trigger`, or
`encounter_rule`). Each product records its contiguous generation index, fresh
Java UUID, stable card ID/upgrades, and its observed birth origin.
Java deliberately observes `CardGroup` entry into draw, hand, discard,
exhaust, or limbo rather than constructor UUIDs: `makeSameInstanceOf()` may
allocate a temporary UUID before restoring the original lineage. A fresh UUID
that bypasses the supported birth observations makes the witness incomplete
instead of being reconstructed from successor position.

Omniscience's additional stat-equivalent copies have an explicit `autoplay_copy`
origin instead of a fictitious zone entry. Both owners observe the copy at its
actual creation, retain its already-bound producer identity, and account for its
allocation before any cards generated by the first play. Java observes the
original `UseCardAction` purge branch; Rust observes the matching after-use
disposal. A copy must either remain held by an actual pending play or have that
explicit purge record. Missing copies and allocator gaps still fail completeness.
Ordinary `makeSameInstanceOf()` replay remains an existing lineage, not a birth.

Rust records a transition-only receipt from the combat-generated lineage
allocator and the same five owning zone entrances. The receipt is not part of
checkpoint or search identity. Its ordered products must agree with Java in
count, card semantics, and first zone before a cross-runtime binding is added.
Replaying an existing lineage allocates nothing and therefore produces an
empty receipt on both sides. A direct card play binds the exact producer Java
UUID and Rust lineage. Protocol minor 35 adds `end_turn_resolution`, a second
maintained cause that owns the complete birth set between one exact EndTurn and
its stable successor. It intentionally does not guess whether an individual
product came from an enemy, relic, power, or other nested producer. Other typed
causes remain closed protocol vocabulary and return `Unknown` until both
runtimes expose the corresponding scoped producer evidence. This replaces the
former Power Through/card-name-specific Wound binder without authorizing a
generic card-name, position, UUID, or top-level-action guess.

The maintained sentinel now has fresh-JVM evidence for both supported causes:
an Anger copy created by `card_play` and a Slimed status created during
`end_turn_resolution` coexist as distinct `CapturedBirth` bindings in one
9/9-Exact successor root. This proves only the scoped transition evidence; it
does not infer the nested enemy producer for Slimed.

Combat hand/grid selection can resume the paused action queue and create cards
after the original play boundary (for example Hex after Secret Technique).
`combat_selection_resolution` names that adjacent typed selection scope, not
the selected card or an inferred nested power. Both adapters observe generation
and first zone entry while resolving the selection. Only a nonempty birth set
allocates a selection birth ordinal; ordinary movement of already bound cards
does not. A missing witness never authorizes a fresh successor UUID. A present
witness must pass the same adjacency, completeness, cause, product-order and
first-zone checks as other birth scopes. Run-event grids do not enter this scope.

Consequently parity still checks that the bound card enters the correct zone,
that persistent mutation reaches the correct master-deck lineage, and that
same-lineage battle copies respond together.  It no longer repeatedly guesses
that an arbitrary Java UUID corresponds to a magic Rust number such as `101`.

## Combat entry and native reload

Watcher combat entry can follow a map choice or the A20 first-boss
`DoubleBoss` proceed action. Both transitions observe `combat_entry_resolution`
while the new combat is initialized. The preceding completed combat owns no
active leases; the new entry binds its actual creations before projection.
The second boss is not treated as movement of the first boss's Miracles.

The original native save format stores card ID, upgrades, and misc data, but
not Java UUID. Loading therefore creates a new Java runtime identity epoch.
UUID leases from the previous JVM/card-object graph end at that boundary. A
stable binding may survive only because its saved-card provenance is verified
again; a post-load activation establishes fresh leases from ordered typed
observations and must not claim UUID continuity. Thus the same checkpoint can
honestly produce the same logical `PairedRoot` and a different
`RootActivation`.

Native Continue also reconstructs independent combat-entry cards, such as
Holy Water's three Miracles. The producer records their actual zone entries
under `combat_entry_resolution`; it excludes loaded master-deck copies by
their exact Java UUIDs. The recovery owner retains the original menu,
completed Continue receipt and adjacent raw successor in `native-reload.json`.
Live capture and offline replay validate this execution before reusing the
ordinary combat-birth binder at ordinal zero. A witness alone cannot authorize
the new UUID leases, and Continue is preparation rather than a committed
player action. Sources without these reconstruction facts cannot bind fresh
combat-generated objects.

Ordinary reward acquisition also needs this distinction. CommunicationMod's
card `source` tag lives in a SpireField and is allocated from the tags visible
in the current Java deck; native reload erases those tags. The Java adapter
therefore maintains the durable reward ordinal from its own observed source
history, retaining it across card removal and reload. It proves each new
binding from the frame-bound selected offer UUID, its completed receipt, and
the exact single-card addition to the deck. It separately validates and keeps
the raw Java tag. Identical map coordinates in later acts, or another reward
after reload, cannot reuse a durable source merely because Java's local tag
starts at zero. This state is reconstructed from raw evidence, not migrated
from a serialized head or inferred from card names and deck positions.

Shop purchases have a separate acquisition source family. The offered card's
UUID is retained by the original `FastCardObtainEffect`; the Java adapter binds
that exact purchase and its adjacent receipt to the sole new deck card. Rust
independently binds the corresponding shop input's new master-deck lineage.
Removing a purchased card does not reset either adapter's source ordinal.

## Evidence and migration

The lineage wire form is intentionally incompatible with historical bare
`u32` card UUIDs.  Exact checkpoints, cases, hashes, and sessions created with
the old representation remain files of historical interest, but are not
silently upgraded or accepted as current exact evidence.  Constructed scenario
lineage is always labeled as constructed and cannot acquire natural-run or
training provenance merely by comparing successfully.

This is also a naming boundary. Rust-owned card state, actions, selections,
search keys, diagnostics, and artifacts use `lineage`/`CardLineage`; Java-facing
frames and actions use `card_uuid` because those values are original-game UUIDs.
`uuid` is otherwise reserved for identities that are actually UUIDs, chiefly
`JavaCardUuid` and the original game's potion UUID. The Rust-owned schema
migration is deliberately breaking: historical Rust `card_uuid`, bare card
`uuid`, and `CardUuid` fields or variants have no aliases. This does not rename
the Java protocol's truthful `card_uuid` wire field to `card_lineage`, and no
numeric equality is inferred between the two representations.
