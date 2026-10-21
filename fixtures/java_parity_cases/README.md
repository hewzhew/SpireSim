# Java parity cases

These files are typed, reviewable inputs for the maintained constructed Java
scenario runner. They are not natural-run fixtures, policy examples, or
training labels. Every execution compiles the selected draft into a fresh plan
with a new scenario UUID and preserves `constructed_validated` provenance.

Inspect the current closed recipe and capability dependencies before adding or
editing a case:

```powershell
.\java-play.ps1 scenario registry
```

Validate and list this catalog without launching the game:

```powershell
.\java-play.ps1 scenario cases
```

Each JSON filename must be exactly `<case_id>.json`. IDs are lowercase exact
slugs; there is no prefix, alias, or fuzzy resolution. The Rust catalog owner
validates every draft and reports its SHA-256.

Run the smoke case through isolated game storage:

```powershell
.\java-play.ps1 scenario create-combat --case cultist-base-root-smoke --profile accelerated
```

Post-root actions live in the separate exact-ID probe-case catalog. Capture its
registered end-turn transition without claiming simulator parity:

```powershell
.\java-play.ps1 scenario probe --probe-case cultist-base-end-turn --profile accelerated
```

Run the complete root/capture/comparison workflow through the maintained
high-level command:

```powershell
.\java-play.ps1 scenario verify --probe-case cultist-base-end-turn --profile accelerated
```

`cultist-base-root-smoke` intentionally exercises Java's oversized-draw empty
second shuffle. `cultist-five-card-remainder-no-shuffle` is its question-bearing
opposite: five cards remain in the draw pile at turn end, so the same comparator
must match without a `shuffleRng` transition.

`cultist-chemical-x-whirlwind-root` composes the optional
`player.relics.fresh_exact.v1` capability with the existing Whirlwind root. The
payload is a complete ordered relic replacement, not a request to add Chemical
X alongside the Ironclad starter relic, and it cannot write mutable relic
fields.

`cultist-letter-opener-root` uses that same closed fresh-relic capability with
three ordinary Defends. The scenario still sets only the relic identity; the
parity observation independently captures Java's runtime relic counter. Its
paired trajectory, rather than the root, owns the counter/reset question.

`cultist-block-potion-root` uses `player.potions.fresh_exact.v1` to replace the
complete natural A20 potion inventory with one fresh Block Potion and one
explicit `Potion Slot`. The caller cannot provide potency, and this capability
cannot be combined with fresh relic construction until potion/relic interactions
have their own typed composition contract.

`cultist-fire-potion-root` uses that same complete-inventory capability for a
target-required potion. Its paired probe binds the exact Cultist occurrence;
the fixture still cannot provide potency or a runtime monster instance ID.

`three-sentries-fire-potion-root` keeps the same potion question but changes
the natural encounter recipe to three same-ID Sentries. It exists to exercise
runtime occurrence disambiguation, not to claim that an elite is naturally
reachable from the constructor's Neow anchor.

`cultist-power-through-second-wind-root` uses a three-card deck so both skills
and the retained Strike are present at the first stable combat decision. Its
trajectory owns the actual question: generation of two runtime Wounds followed
by ordered mass exhaust. The root alone proves neither transition.

`cultist-corruption-power-through-second-wind-root` adds Corruption to that
minimal deck. The corresponding trajectory owns the modifier question:
power-card removal, one exact player power, zero-cost skill conversion,
generated-card identity, and later exhaustive movement. The constructed root
is mechanics-only evidence and is never strategy or training evidence.

The two `cultist-feed-*` roots share one terminal-settlement question at
different health values. The original root remains below Meat on the Bone's
threshold after Feed+ grows HP; `cultist-feed-crosses-meat-threshold-root`
crosses above the newly enlarged half-health threshold. They are paired cases
for one typed comparator, not separate hard-coded settlement implementations.

Keep cases minimal and question-bearing. Do not add one case per card merely to
inflate coverage, and do not add arbitrary Java field writes, fuzzy IDs, or
natural-language mutation instructions.
