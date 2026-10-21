# Constructed Java parity trajectory cases

These tracked drafts bind one exact constructed root to an ordered sequence of
two through sixteen closed typed transitions. The complete sequence is compiled
before Java starts. Runtime action UUIDs, card UUIDs, monster instance IDs,
potion potency, and action strings never belong in a draft; each selector is
resolved from the new Java-owned action space after the previous step settles.

Trajectory cases remain `constructed_validated` mechanics experiments. They do
not prove natural reachability, strategy quality, or training eligibility. A
catalog or compiled plan is not transition evidence. A passing capture retains
every adjacent receipt and initially sets `simulator_comparison=false`. The
workflow independently compares each supported step and binds the clean managed
lifecycle before it can set aggregate simulator comparison true.
When setup steps are valid capture evidence but outside the comparator's
mechanics scope, an offline selected-step recompare may bind the whole captured
chain while comparing only named zero-based indices. Its report must keep
`all_steps_constructed_and_matched=false` unless every step was actually
compared.

List and validate the tracked drafts without launching the game:

```powershell
.\java-play.ps1 scenario trajectories
.\java-play.ps1 scenario trajectory --trajectory-case cultist-two-strikes --profile accelerated
.\java-play.ps1 scenario trajectory --trajectory-case cultist-power-through-second-wind --profile accelerated
.\java-play.ps1 scenario trajectory --trajectory-case cultist-corruption-power-through-second-wind --profile accelerated
.\java-play.ps1 scenario trajectory --trajectory-case cultist-feed-crosses-meat-threshold --profile accelerated --step-index 3
.\java-play.ps1 scenario recompare-trajectory --trajectory-report .oracle-lab/java-scenarios/<trajectory>/trajectory-report.json --step-index 3
```

`cultist-two-strikes` is the smallest successor-selector sentinel: after the
first same-ID card leaves hand, the second selector is resolved again from the
new Java action space. `cultist-power-through-second-wind` is the first
question-bearing mechanics chain. Power Through creates two runtime-owned
Wounds, then Second Wind consumes those exact successor cards. The comparator
pairs newly generated Java and Rust cards only by complete typed zone position
and card semantics; it never equates their UUID values.

`cultist-corruption-power-through-second-wind` adds the first supported player
power to that chain. It verifies Corruption's exact typed power application and
skill cost conversion before following the same generated Wounds into Second
Wind's Corruption-forced exhaust successor. Foreign or duplicate powers reject
the comparison; this case does not authorize a general power reconstruction.

`cultist-letter-opener-reset-and-trigger` plays two skills, crosses one real
Cultist end-turn boundary, then plays three more skills. All six adjacent steps
are compared: Java and production Rust must both expose `0 -> 1 -> 2`, reset the
counter to `0` at the next turn start, and deal exactly 5 all-enemy damage only
on the new turn's third skill. The scenario cannot seed a counter value; each
predecessor counter is observed from the immutable Java frame.

`cultist-feed-terminal-settlement` reaches an exact Feed+ lethal after three
captured setup actions. Selected step 3 proves only the terminal transition:
the production Rust stepper and original game both apply Feed's +4 max HP,
Meat on the Bone's 12 heal, and Burning Blood's 6 heal, ending at 72/191 with
99 gold on the combat reward boundary. The selected comparison does not turn
the Bludgeon or two end-turn setup actions into matched mechanics evidence and
does not compare reward generation.

`cultist-feed-crosses-meat-threshold` uses the same typed action chain from
55/100. Cultist's attack leaves 49/100; Feed+ first changes that to 53/104, so
Meat on the Bone no longer triggers and Burning Blood alone ends at 59/104.
Together the two cases distinguish settlement ordering from one memorized
terminal value.

Keep one-step questions in `java_parity_probe_cases`. Add a trajectory only
when the ordering or interaction between steps is itself the question.
