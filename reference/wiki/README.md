# Slay the Spire Wiki corpus

This is the maintained catalog and query contract for complete MediaWiki text
captures. It is separate from `reference/expert/`: a Wiki page is mechanics or
reference evidence, while an expert guide is attributed strategic testimony.
Neither becomes a policy label merely because it is locally searchable.

## Sources and scope

`sources.jsonl` records stable source IDs and acquisition capabilities for the
current English wiki.gg Wiki, the Chinese Huiji Wiki, and the legacy English
Fandom Wiki. A full text capture covers every non-negative MediaWiki namespace:
articles, redirects, templates, categories, modules, file-description pages,
project pages, and public discussions. Binary image bodies are a separate
future asset layer and are not mixed into the first text corpus.

Complete bodies live below the ignored directory
`.oracle-lab/external-research/sources/<source-id>/`. Tracked files contain
only tools, schemas, catalog metadata, and instructions.

## One export protocol, interchangeable transports

Both transports produce `mediawiki-text-export-v1` JSONL:

- `direct-mediawiki-api` is used when the official API accepts an ordinary
  HTTP client, currently wiki.gg and Fandom;
- `browser-mediawiki-api` is the same-origin browser transport for a site such as
  Huiji where Cloudflare accepts the normal browser session but rejects the
  direct client.

The browser script is [mediawiki-export.user.js](../../tools/wiki/mediawiki-export.user.js).
It calls the same-origin `/api.php`; it does not scrape visible DOM. It keeps
page batches and continuation state in IndexedDB, permits pause/resume and an
explicit request interval, and refuses to export a `complete` trailer until
every requested namespace has ended normally.

Tampermonkey is the persistent user-facing host. The same audited file may be
temporarily injected into a controlled in-app browser developer session; that
is the same transport and bundle contract, not a third scraper.

The browser transport enumerates page identity separately from page metadata
and current revision text. A MediaWiki semantic API error is never retried as
if it were a network failure. Batch-specific metadata failures are bisected to
one page; current text is then recovered by explicit page ID when possible.
Any remaining warning or text gap is stored on that page and counted by the
verifier instead of being hidden or aborting unrelated namespaces.

## Direct acquisition

All commands are PowerShell 7 and use Python's standard library. They never
build Rust.

```powershell
.\wiki.ps1 probe --api-url https://slaythespire.wiki.gg/api.php

.\wiki.ps1 crawl `
  --source-id slaythespire-wikigg-en `
  --api-url https://slaythespire.wiki.gg/api.php `
  --capture-name 2026-08-20-full
```

If a direct capture is interrupted, repeat the same command with `--resume`.
The partial capture and continuation are durable. Repeated response pages are
deduplicated by MediaWiki page ID.

## Browser acquisition

1. Install `tools/wiki/mediawiki-export.user.js` in Tampermonkey.
2. Open the Wiki normally. The exporter panel never starts automatically.
3. Keep the stable source ID from `sources.jsonl`, choose a request interval,
   and click `新建并开始`. Pause or resume at any batch boundary.
4. When status is `complete`, click `导出`. Modern Chromium exports gzip;
   other browsers may export plain JSONL.
5. Install the downloaded file into a fresh capture path:

```powershell
.\wiki.ps1 import-browser `
  --bundle C:\Users\<user>\Downloads\slay-the-spire-huiji-zh-....jsonl.gz `
  --capture-name 2026-08-20-full
```

The importer re-verifies the header, every unique page ID, namespace counts,
completion trailer, page-ID digest, and full-file SHA-256 before installation.

## Normalize and query

Normalize one verified capture, then rebuild the cross-source index atomically:

```powershell
.\wiki.ps1 normalize `
  --bundle .oracle-lab/external-research/sources/slaythespire-wikigg-en/captures/2026-08-20-full/export.jsonl.gz `
  --source-root .oracle-lab/external-research/sources/slaythespire-wikigg-en

.\wiki.ps1 rebuild-index
.\wiki.ps1 verify --bundle <export.jsonl.gz>
.\wiki.ps1 issues --bundle <export.jsonl.gz>
.\wiki.ps1 inventory --source-id slay-the-spire-huiji-zh
.\wiki.ps1 list-pages --source-id slaythespire-wikigg-en --namespace 0 --limit 20
.\wiki.ps1 search Brutality --source-id slaythespire-wikigg-en
.\wiki.ps1 search Brutality --content-only
.\wiki.ps1 search Brutality --namespace 0
.\wiki.ps1 page --source-id slay-the-spire-huiji-zh --title 残暴
.\wiki.ps1 page --source-id slaythespire-wikigg-en --title Gremlins --metadata-only
.\wiki.ps1 page-sections --source-id slaythespire-wikigg-en --title Gremlins
.\wiki.ps1 structure --source-id slay-the-spire-huiji-zh
.\wiki.ps1 table --source-id slay-the-spire-huiji-zh --title 'Data:Card data.tabx' --columns 'page,card_id,name_en' --limit 5
```

Search treats its query as a literal phrase by default. Use `--raw-query` only
when MediaWiki-corpus FTS syntax is deliberately required.

Normalized page records preserve source ID, page/revision identity, timestamp,
revision SHA-1, URL, raw wikitext, a search projection, redirects, categories,
templates, internal links, and declared acquisition warnings/errors. Search
takes bounded snippets and always retains source and revision provenance.
`page --metadata-only` omits both page bodies and the internal-link list while
retaining their sizes/count plus templates, categories, redirect, and revision
identity. Full `page` output returns any
declared acquisition issue for the exact page.
`page-sections` returns no page body. It extracts only headings written
explicitly in the captured wikitext and reports their hierarchy, revision-local
character locators, and bounded body-length metadata. Headings produced only by
template expansion are deliberately absent; the result is an auditable source
outline, not a rendered MediaWiki parse.

`structure` summarizes machine-readable Data page families, content models,
JSON keys, tabular fields, and row counts without printing their bodies.
`table` reads selected columns and a bounded row window from one JSON tabular
page while preserving its revision provenance.

## Cross-language entity graph

The graph is a disposable derivative of the installed Wiki database and the
simulator's stable Rust identity owners. Its daily build and query path is
Python/PowerShell only; it does not compile or link Rust:

```powershell
.\wiki.ps1 simulator-entities
.\wiki.ps1 rebuild-entities
.\wiki.ps1 entity-inventory
.\wiki.ps1 mechanics-scope-inventory --example-limit 3
.\wiki.ps1 mechanics-scope-inventory --kind relic --example-limit 3
.\wiki.ps1 entity --kind card --id Brutality
.\wiki.ps1 entity --query 残暴
.\wiki.ps1 entity-search 'At the start of your turn' --kind card --id Brutality
.\wiki.ps1 entity-search 'Weak' --kind encounter --id GremlinGang --include-related
.\wiki.ps1 entity-page-search 'Weak' --kind enemy --id GremlinFat
.\wiki.ps1 entity-sections --kind enemy --id GremlinFat
.\wiki.ps1 entity-section-search 'Weak' --kind enemy --id GremlinFat
.\wiki.ps1 entity --kind relic --id RunicCube --full-evidence
.\wiki.ps1 entity-unmatched --side simulator --kind relic --candidate-state some
.\wiki.ps1 entity-candidate-audit --kind relic
.\wiki.ps1 entity-candidate-audit --kind relic --details --limit 20
.\wiki.ps1 entity-reviews
.\wiki.ps1 entity-conflicts
.\wiki.ps1 entity-issues
```

`entity-page-search` first resolves one exact simulator kind and stable ID through
the entity graph, then restricts FTS to that entity's accepted pages. Candidate
pages are never searched. Encounter members are also excluded by default;
`--include-related` explicitly follows
`encounter_initial_possible_member` to the accepted enemy pages while labeling
every result binding as `direct` or `related`. A grouped Wiki page may bind to
multiple enemies, so one deduplicated page retains all of its
`scope_bindings`. Queries are literal phrases unless `--raw-query` is supplied,
and the bounded snippets retain page and revision provenance. A text match is
mechanics evidence from that page, not a strategy tag or teacher conclusion.
`shared_entity_page` and `page_accepted_entity_keys` identify grouped pages;
their snippets are page-level evidence and require section-level inspection
before a sentence is attributed to one member.
The command fails closed when the entity graph was built from a different Wiki
database; rebuild the graph instead of combining stale identity decisions with
new page text.

`entity-search` is the attributed mechanics lookup. A page accepted by exactly
one entity is searched whole. A page accepted by several entities is searched
only through that entity's `unique` exact section; `none` and `ambiguous` shared
pages are excluded without fallback. `--include-related` is valid only for an
encounter and applies the same rule independently to each typed initial member.
Every result declares `scope_type` (`page` or `section`) and the direct/related
entity bindings that made it eligible. Page and section documents share one
small in-memory FTS ranking space, so the global result limit stays meaningful.

`mechanics-scope-inventory` audits that eligibility rule over the installed
entity graph without returning page bodies. `accepted_entity_pages` and all
scope counters count entity-page pairs, not distinct physical pages. A safe
scope is either a page accepted by only that entity or a unique exact section
on a shared page. `entities_with_accepted_but_no_safe_scope` is the fail-closed
blocking signal; `entities_with_partial_exclusions` still have at least one
safe source and merely retain unusable shared pages as visible exclusions.
`sources` describes only the entity set selected by `--kind` and therefore
reconciles with `totals`.

Long-lived Python consumers use `AttributedMechanicsOwner` rather than invoking
the CLI for each phrase. The owner verifies the graph/corpus hash and loads
exact entity/page ownership once, then reuses revision-keyed shared-page
outlines. It checks both database file identities around every search and fails
closed if an atomic rebuild replaces either file; consumers must open a new
owner for the new generation.

Encounters deliberately have no direct accepted pages. Their separate
`encounters` block audits whether every typed initial-member edge reaches an
enemy with at least one safe mechanics scope. In the 2026-08-20 installed
corpus all 65 encounter groups and all 120 initial-member links are covered;
all 65 enemies, 52 events, and 42 potions have a safe scope. Cards cover
370/371, with `card:Impulse` explicit rather than guessed, and relics cover
140/182, leaving 42 explicit source gaps. The 16 excluded shared entity-page
pairs do not block any entity because each affected entity has another safe
scope. These are a reproducible snapshot, not a reason to promote candidates
or claim that the prose is complete or correct.

`entity-sections` audits whether one exact entity can be bound to a heading on
each accepted page. It uses only simulator-owned display/external names and
name fields carried by accepted structured identity evidence. Matching is NFKC,
case-folded, whitespace-normalized equality; stable-ID similarity, translation,
and candidate evidence are excluded. A page reports `unique`, `none`, or
`ambiguous`. Only `unique` is an exact section anchor. `none` is expected for
single-entity pages and for grouped pages whose division is produced by
templates rather than explicit headings; it is not repaired by guessing title
order.

`entity-section-search` searches only the `unique` section subtrees reported by
`entity-sections`. Revision identity and every character locator are rechecked
against the current corpus before a small in-memory FTS query runs. `none` and
`ambiguous` pages are listed as excluded and never fall back to page-wide text.
Literal phrase mode is the default; `--raw-query` remains explicit. Results
contain bounded snippets and section/page/revision provenance, never the full
subtree body. Use `entity-page-search` when deliberately inspecting page-level
evidence; use `entity-search` for the safe mixed page/section default, and the
section-only command when its exclusion boundary is itself under audit.

`simulator-entities` reads the authoritative gameplay enum owners plus the
versioned `src/content/relics/identity_v1.json` contract. Its no-build parser
understands the explicit `stable_gameplay_id!` syntax and verifies stable ID and
ordinal completeness before extracting the entity kinds represented in the
Wiki graph. It separately verifies card Java IDs, potion and enemy display
names, and relic stable/external identities. An owner syntax or coverage change
fails explicitly.
Source file hashes and source lines are stored with every simulator entity.
For relics, one exact external ID is distinguished from additional exact
protocol aliases; neither is confused with the durable stable ID, localized
display text, or expert vocabulary. Events additionally read the exhaustive
`LIVE_EVENT_NAME_ALIASES` exact inbound owner, the separate explicit event
display-name owner, and the explicit `RETIRED_EVENT_IDS` compatibility set.
Retired serialized variants are absent
from the active catalog, and a duplicate, missing, unknown, or retired live
alias fails the catalog build.

The same no-build catalog extracts the typed
`encounter_initial_possible_member` relation from the exhaustive
`build_encounter` match. Every active `EncounterId` must occur in exactly one
arm with at least one known `EnemyId`; combined arms and local helpers that
return `EnemyId` are expanded from source. The compact summary reports relation,
source, and target counts without printing all edges. This is initial encounter
composition evidence, not a Wiki alias and not a claim about enemies summoned
after combat begins.

The former `tools/compiled_protocol_schema.json` mixed dead monster and power
generators with the one live start-spec potion parser. It has been removed.
Human-authored potion aliases now live on `PotionId`'s
`GameplayIdentitySurface`; ordinary normalized stable IDs need no duplicate
alias, while the three genuinely different input spellings are declared
explicitly. Whole-vocabulary input parsing is shared and rejects ambiguous
display or alias matches. Relic external protocol identity remains owned by
`src/content/relics/identity_v1.json`.

`rebuild-entities` atomically creates the ignored
`.oracle-lab/external-research/entities.sqlite3`. It stores simulator entities,
typed simulator relations, typed Wiki pages, source/revision provenance,
accepted edges, candidate edges,
revision-bound identity reviews, conflicts, issues, unmatched simulator
entities, and unmatched typed Wiki pages. Card BSON records and Huiji tabular
rows provide exact cross-language anchors; redirects remain visible evidence
chains. Enemy identities and encounter-group identities are never collapsed.
An exact page category is the primary page-kind signal, so an event page may
embed an enemy infobox without becoming untyped; multiple exact kind
categories remain ambiguous. A page with only a disambiguation category is not
typed from an embedded infobox.

Accepted edges require exact structural identity evidence or an exact typed
simulator identity/title. An exact relic protocol alias can therefore anchor a
Huiji Data record; removing punctuation, spaces, or case is still only
candidate generation and can never silently accept an edge. Every normalized
typed-title candidate carries its exact page, revision, title-field locator,
and kind, so a tracked review may accept that record without reclassifying it
as structured Data evidence. A conflict means
evidence is contradictory or ambiguous. A source issue is non-contradictory
incompleteness, such as a structured record whose target article is absent or
untyped. Encounters have no equivalent protocol identity owner, so direct page
coverage remains measurable unmatched coverage rather than guessed aliases.
The graph stores `encounter_initial_possible_member` in a separate relation
table: querying an encounter by stable ID returns each target enemy and that
enemy's accepted Wiki pages, but those pages never create an accepted edge for
the encounter itself.

Events have a separate conservative evidence path. On typed wiki.gg event
pages, the builder recognizes only explicit line-level internal-name claims
(`Internally called`, `This event is internally called`, or `The internal name
of this event is`). A typed Huiji event page joins that English page only when
it contains exactly one `英文名` field with the exact wiki.gg title. An internal
name exactly equal to a Rust `EventId` is accepted automatically; a match that
exists only after punctuation/case normalization remains a candidate. Duplicate
internal-name claims or duplicate Huiji English names become conflicts instead
of being selected by order. Exact live event display aliases may also accept an
exact typed English title. A title ending in the matching kind suffix, such as
`Golden Idol (Event)`, creates only a revision-bound candidate after the suffix
is removed; it requires a tracked review and never widens the runtime parser.
If one typed event page is accepted by more than one active `EventId`, every
affected source page produces an explicit conflict.

Human identity decisions live in the tracked
`reference/wiki/entity_identity_reviews.json` ledger. Every `accept`, `reject`,
or `defer` binds the entity key to the source/page/revision, record locator, and
complete record identity, plus a rationale, reviewer, project marker, and date.
Rebuild fails if that exact candidate is absent or ambiguous, if the source
revision/title/content has changed, or if the record was already accepted.
`accept` adds separate `reviewed_identity_override` edges while preserving the
original candidate edges; `reject` and `defer` alter no acceptance. The ledger
schema and SHA-256 are stored in the graph, and `entity-reviews` reports the
decisions applied to that exact build.

`entity-candidate-audit` deduplicates repeated Wiki pages back to their source
Data record and reports one-entity/multiple-record and one-record/multiple-
entity collisions. Its compact output covers every unmatched entity; `--details`
adds bounded proposal and exception rows. A unique proposal remains a candidate,
not an accepted mapping. In the 2026-08-20 installed corpus, exact simulator
identity chains and three revision-bound Wish-option title reviews accept 370
of 371 cards. `Impulse`, an active simulator mechanic excluded from all three
Defect reward pools, remains unmatched because the installed corpus has no
typed card page for it. Exact simulator identity chains accept 140 of 182
relics; of the remainder, 38 have unique structured proposals with no
collisions and 4 have no candidate. The active
event catalog contains 52 entities after excluding one retired compatibility
ID. Exact runtime aliases, structured identity chains, and five event reviews
accept all 52 with no graph conflict. Those reviews cover three normalized
wiki.gg internal-name records and both independently typed `Golden Idol (Event)`
title records; all original candidate evidence remains visible. This is event
identity coverage, not proof that event mechanics or strategy are complete.
Enemy identity accepts all 65 entities after the simulator's English display
owner was corrected from Rust/Java class-style labels to original-game names
such as `Fat Gremlin`, `Mystic`, and `The Champ`. Exact Huiji rows and
independently typed variant/redirect pages remain the evidence; the mixed
wiki.gg `Masked Bandits` event page is not retyped as an enemy. Encounter
direct-page identity remains 0 of 65, while 120 initial-member relations cover
all 65 encounters and 63 initial enemy types; `BronzeOrb` and `TorchHead`
remain outside that relation because their factory owners introduce them only
after combat begins.
The 38 relic proposals are intentionally unresolved because some differences,
such as `Frozen Egg 2` versus `Frozen Egg`, are substantive owner/source
disagreements rather than harmless whitespace.

The default `entity` output groups repeated evidence by source page to remain
small. Add `--full-evidence` to inspect the exact Data page revision, row,
matched field, and redirect chain.

## Completeness boundary

`complete` means that every namespace listed in that bundle was exhausted by
MediaWiki continuation and that the installed bundle passed local structural
verification. It does not mean that the remote Wiki is correct, current,
strategically authoritative, or that binary assets were acquired. Distinct
Wikis and revisions remain separate sources; normalization never silently
merges their claims. `captured_complete_text_with_metadata_warnings` means all
current revision bodies were recovered but some auxiliary page metadata used a
declared fallback. Any missing revision body instead produces an acquisition
error and a source status that explicitly declares text gaps.
