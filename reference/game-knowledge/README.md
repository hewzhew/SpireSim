# Game mechanics reference library

This directory stores small, reviewable mechanics facts for Slay the Spire 1. It is separate from
expert strategy claims, runtime policy, evaluator labels, and training data.

## Layout

- `../entities.jsonl`: shared entity IDs and multilingual aliases.
- `sources.jsonl`: provenance, licence, content signals, lineage, and access policy.
- `assertions.jsonl`: one source-local mechanics statement with a stable locator.
- `facts.jsonl`: normalized facts supported by one or more assertions.
- `knowledge.ps1`: an offline query and verification entry point. It never accesses the network.

The library does not contain wiki page bodies. Wiki records retain only factual normalization,
page/revision locators, and short caveats. Decompiled game code remains in the sibling local
`cardcrawl` directory and is referenced by version, path, symbol, and SHA-256; it is not copied.

## Evidence and independence

`wiki_statement` means that a wiki page reported something. `decompiled_game_code` means the
mechanic was inspected in the local decompilation of game version 2.3.4. A normalized fact may be:

- `wiki_reported`: not yet checked against game code;
- `source_verified`: checked against versioned game code;
- `conflict`: sources disagree in a way that has not been resolved;
- `unverified`: retained as a question rather than a conclusion.

The Chinese Huiji site says its material currently comes mainly from the English wiki. Both wiki
sources therefore use the same independence group. Agreement between them is useful for names and
translation, but does not count as two independent confirmations.

## Access policy

The 2026-08-16 source audit found `use=reference` and `ai-train=no` content signals on both sites.
wiki.gg also disallows automated agents and `/api.php` in `robots.txt`. Huiji blocks ordinary API
clients with Cloudflare and disallows automated-agent access. Consequently:

- there is no crawler or wiki sync command in this repository;
- bulk access and raw-body retention are disabled;
- selected pages may be reviewed manually as references;
- wiki material must never be used for model training or fine-tuning;
- access policy must be re-audited before adding any network acquisition tool.

A permission inquiry was sent to `support@huiji.wiki` on 2026-08-16. Until a reply is recorded in
`sources.jsonl`, Huiji remains `permission_pending`; a reply may relax or tighten acquisition, but
does not change the assertion/fact schema.

## Curated vertical slices

The first slice covers `event:Vampires`, `card:Bite`, starter Strike replacement, Blood Vial,
maximum-HP loss, and Molten Egg obtain hooks. It records an important distinction omitted by both
wiki summaries: version 2.3.4 removes every card tagged `STARTER_STRIKE`, and the maximum-HP cost is
`ceil(max_hp * 0.3)` capped at `max_hp - 1`.

The Rust event transition removes cards by the `StarterStrike` tag in reverse deck order. Its public
event-option semantics now expand the same tag match against the current deck into concrete typed
removal effects before the five Bite obtains, so strategic consumers can observe the full exchange
without parsing display text or relying on a Vampires-specific card-name rule.

The second slice covers `event:BigFish`, the three event branches, relic-tier
and screenless-pool consumption, `card:Regret`, and `relic:BlueCandle`. It
preserves the distinction between a pool-cursor fact and a strategy judgment:
choosing Banana or Donut does not consume Big Fish's relic roll or any tier
pool entry, but that alone does not prove which relic a later source will
produce or which branch is best for a particular run.

The third slice covers `event:Neow`'s curse drawback and the ordinary
obtainable curse pool. Source inspection confirms that it consumes one
`cardRng` index rather than always granting Regret. The retained pool order is
the versioned Java `HashMap` iteration used by the unmodded 2.3.4 runtime; it
exists for seeded parity and is not a card-quality ordering.

The fourth slice covers Neow's three-potion reward under A11+ two-slot
capacity. The three generated potions are separate visible reward items, so
the player can choose which two to claim; a full-slot claim fails without
consuming the third item, and Proceed opens the map preview. This establishes
the exact decision surface but deliberately does not rank potion pairs.

## Commands

```powershell
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Entity event:Vampires
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Entity 吸血鬼
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Entity card:Bite
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Entity event:Vampires -Detailed
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Fact vampires-accept-max-hp-loss
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Entity 大鱼
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Entity 遗物池
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Fact big-fish-non-box-preserves-relic-state
pwsh -File .\reference\game-knowledge\knowledge.ps1 -ListSources
pwsh -File .\reference\game-knowledge\knowledge.ps1 -Verify
```

To add a fact, first reuse or add a shared entity. Then add a source-local assertion with the exact
page revision or local source file hash. Only add a normalized fact after reviewing the assertion in
context. Do not promote translations, search snippets, or automatically extracted text directly to
`source_verified`.
