# Combat Search Cases

This folder holds small, fixed combat-search cases that are useful for local
search work. A case is not a policy verdict or a campaign artifact. It is a
reproducible combat boundary with enough state to compare search behavior.

## Frozen Case Panel V0a

These cases are fixed inputs for the review-only Frozen Case Panel. They are
combat boundaries, not campaign-policy verdicts.

```text
fixtures/combat_cases/frozen_v0a_awakened_one_1552225675_a3f48.json
fixtures/combat_cases/frozen_v0a_collector_1552225671_a2f32.json
fixtures/combat_cases/frozen_v0a_gremlin_leader_1552225671_a2f29.json
```

The V0a panel compares exactly two explicit search lanes on these three cases:
`baseline` and `key_setup_bias`.

## A2F23 Slavers Threshold Case

Case:

```text
fixtures/combat_cases/seed1700000123_a2f23_slavers_b0034.json
```

Context:

```text
seed=1700000123
ascension=0
act=2 floor=23
enemy=Blue Slaver + Taskmaster + Red Slaver
hp=65/85
deck_size=16
hand=Strike, Defend, Clothesline+, Armaments+, Second Wind+
potions=Blessing of the Forge, Blessing of the Forge, Block Potion
```

Baseline checks:

```powershell
cargo run --quiet -p sts_oracle_tools --bin combat_case_review -- --case fixtures\combat_cases\seed1700000123_a2f23_slavers_b0034.json --ladder --fast-nodes 100000 --fast-ms 1000 --slow-nodes 100000 --slow-ms 2000 --compact
cargo run --quiet -p sts_oracle_tools --bin combat_case_review -- --case fixtures\combat_cases\seed1700000123_a2f23_slavers_b0034.json --ladder --fast-nodes 100000 --fast-ms 1000 --slow-nodes 100000 --slow-ms 2000 --compact --disable-rollout
```

Expected current behavior:

```text
The ladder should show whether the fast no-potion review, slow potion diagnostic,
or rollout-disabled comparison finds a complete win. Use the JSON fields
`classification`, `ladder[*].complete_win`, `ladder[*].hp_loss`,
`ladder[*].nodes_expanded`, and `performance` to compare search changes.
```

Use this case to improve first-win discovery. Do not treat raising the global
budget as progress. A useful search change should make the fast review more
decisive without hiding the slow diagnostic comparison.

Reference directions:

- [Nested Monte-Carlo Search](https://www.ijcai.org/Proceedings/09/Papers/083.pdf):
  useful when there is no strong static evaluator and the immediate need is to
  find any complete action sequence.
- [Nested Rollout Policy Adaptation](https://www.ijcai.org/Proceedings/11/Papers/115.pdf):
  useful later if we start learning local action preferences from found winning
  lines.
- [Beam Monte-Carlo Tree Search](https://dke.maastrichtuniversity.nl/m.winands/documents/CIG2012_paper_32.pdf):
  useful if a bounded width of promising partial lines can replace broad root
  fanout without hiding critical lines.
