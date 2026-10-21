# Constructed Java parity probe cases

Probe cases compose one exact scenario-root case with one closed typed
transition selector. They never store a runtime card UUID or parse an action
label. Card plays select an exact card ID, upgrade count, and occurrence in
Java hand order; targeted plays additionally select an exact live monster ID
and occurrence in monster order. The compiled plan resolves those selectors
against the typed action space and requires one exact match with empty action
parameters.

Inspect the catalog without launching the game:

```powershell
.\scripts\java-play.ps1 scenario probe-cases
```

Run a complete comparison by exact probe-case ID:

```powershell
.\scripts\java-play.ps1 scenario verify --probe-case cultist-second-strike --profile accelerated
```

These fixtures are `constructed_validated` mechanics evidence only. They are
not natural reachability, strategy, or training evidence.

`cultist-whirlwind-full-energy` exercises the ordinary X-cost contract: the
typed play has no caller-supplied energy parameter, so an unmodified
Whirlwind reads all three current energy, deals three hits, and leaves zero
energy. It does not cover Chemical X or a free-to-play copy.

`cultist-chemical-x-whirlwind-full-energy` is the paired positive composition:
the exact root replaces the relic list with one fresh Chemical X, while the
same parameter-free play still commits all three energy and gains two effect
repetitions. The adjacent-frame report covers total damage and energy, not the
individual hit event/animation sequence.

`cultist-block-potion-use` selects slot 0 and the exact `Block Potion` ID. The
runtime action space supplies potency and the executable action identity; the
fixture supplies neither.

`cultist-fire-potion-use` additionally selects the exact `Cultist` occurrence.
The runtime action space supplies the live monster instance, potency, and
executable action identity. Its comparator covers an unmodified, unblocked,
nonlethal 20-damage transition and does not generalize that result to potion
modifiers or enemy damage-reduction powers.

`three-sentries-second-fire-potion-use` selects occurrence `1` from three live
monsters with the same `Sentry` ID. The plan still contains no runtime index or
instance ID; Java resolves both from the typed action space and the report
proves that only the selected middle Sentry changed.
