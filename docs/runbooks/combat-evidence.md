# Combat Evidence And Offline Laboratories

This runbook covers read-only or isolated exact-combat investigations. None of
these commands changes production policy by itself. Treat a budget-limited
missing witness as unknown and exact-replay every claimed win.

The current schema name and version for each report are owned by its Rust
producer. Keep output paths descriptive and version-independent so a schema
revision cannot silently stale this recipe.

## Batch Combat Evidence Audit

Index local combat artifacts and replay exact case/action relationships:

```powershell
.\ol.ps1 combat-evidence-audit
```

The root defaults to `.oracle-lab`; omitting `--output` creates a fresh ignored
report directory. Source artifacts are never rewritten.

The scanner consumes producer-owned `*combat-evidence-manifest.json` files
first. V2 manifests bind the case's typed replay capability, exact combat root,
optional run-session fingerprint, and optional owner-policy fingerprint. Every
artifact path is relative to the manifest itself and has exactly one resolution
base. On Windows, the manifest and its declared artifacts must therefore live
on the same volume. The scanner then revalidates root identity, action identity
and count, complete consumption, terminal outcome, and final player HP against
the simulator. A manifest is evidence routing, not an override of replay truth.

Legacy V1 manifests and declared trace relationships remain readable through
an explicitly labeled compatibility path. Undeclared action files are not
paired by matching stems or by counting cases in a directory; they remain
explicit unknowns until a producer-owned relationship exists.

The report directory contains:

- `summary.json`: aggregate counts, runtime identity, and source fingerprint;
- `evidence.jsonl`: normalized typed action transitions;
- `fiend-fire-windows.json`: the maintained Fiend Fire diagnostic projection;
- `unresolved.json`: evidence that could not be attributed or replayed.

Every exact card transition records a typed preceding-card bypass when it can
be constructed from exact card and target identity. Missing identity,
illegality, transition limits, and trace-only evidence remain separate
statuses.

### Typed Query Batches

Supply one `CombatEvidenceQueryBatchV1` document from a path or stdin:

```powershell
.\ol.ps1 combat-evidence-audit --query-batch <query.json>
Get-Content -Raw -LiteralPath <query.json> | .\ol.ps1 combat-evidence-audit --query-batch -
```

Queries filter typed record outcome, current and previous same-turn card
identity/type, target before/after state, HP and Block deltas, and the exact
bypass result. Each query is bounded by its declared match limit. The command
stores the normalized query batch and bounded typed results beside the report.
It does not contain card-specific policy conclusions, search, or ranking.

## Potion Expenditure Audit

Compare no-potion, each initial potion, and optionally small combinations from
one unchanged exact combat root:

```powershell
.\ol.ps1 combat-case-potion-expenditure-audit `
  --case <case.json> `
  --max-combination-size 2 `
  --export-witness-actions-dir <fresh-output-directory> `
  --wall-ms-per-lane 10000
```

Every lane receives the same independent allowance. Explicit potion use is
filtered by exact slot without deleting potions from the root; actual
expenditure is replay-attributed by potion UUID. Passive Fairy Potion use is
therefore visible. A passive expenditure outside the lane's allowed slots
marks the witness non-compliant.

Explicit discard is excluded by default because it usually adds a mechanically
irrelevant duplicate search surface. Enable `--include-discard-actions` only
for a concrete slot-generation or revive-priority question.

The report includes final HP, final turn, action count, exact potion identity,
an optional experimental survival reserve, and a Pareto frontier. Missing
budget-limited witnesses do not establish potion value. Forced-rest avoidance,
future elite plans, slot overflow, future shops, and encounter-specific
preservation remain run-level questions unless exact continuation evidence is
present.

When witness export is enabled, each replay-verified lane writes an exact
`<lane-id>.actions.json` plus a producer manifest after lane validation. Feed
those actions directly to replay or trace commands instead of reconstructing
a promising line from display text.

For one-factor production-parity diagnosis:

- `--max-hp-loss <N>` uses production-shaped HP satisfaction;
- `--restore-witness-actions <actions.json>` preloads one exact incumbent in
  every compatible lane;
- `--authorized-root-potion-trial` is a laboratory ablation, never permission
  to spend a potion in production.

Production staging, quality satisfaction, survival floors, and potion
admission are maintained in
[Combat Search Ownership](../architecture/combat-search.md). Audit crossings are
evidence about that owner contract, not standalone spend labels.

## Isolated Loadout Counterfactual

When one exact shop alternative must be tested at a later combat root, create a
fresh isolated case instead of editing JSON or pretending that the result is a
replayable run continuation:

```powershell
.\ol.ps1 combat-case-loadout-counterfactual --case <source.case.json> --output <fresh-counterfactual.case.json> --remove-card-uuid <uuid> --add-relic BronzeScales
```

The current typed scope is deliberately narrow: one exact master-deck card is
removed from its current combat zone, `BronzeScales` is added, and the real
battle-start hook supplies `Thorns:3`. The command rejects queued/limbo roots,
existing Bronze Scales, unknown UUIDs, and every other relic until its complete
battle-start semantics have an owning reconstruction. It clears production
context and prior search/path evidence, refreshes derived counts, writes only a
fresh output, reloads it, and requires `IsolatedProjection`. The result can
answer a combat-local loadout ablation; it cannot establish that the earlier
shop branch would have reached the same run state or RNG.

## Policy Discrepancy Complete Wins

Export a complete policy-discrepancy win with:

```powershell
.\ol.ps1 combat-case-policy-discrepancy `
  --case <case.json> `
  --export-witness-actions <fresh-output>.actions.json
```

When search finds a complete exact-replayed win, the command writes the action
array and a sibling producer manifest. The manifest names the caller-supplied
case as its root, freezes the exact root and action identities, and records the
replayed terminal outcome and final player HP. The command report returns both
paths. If no witness is found, neither export is claimed.

This contract applies only to the complete winning witness. A descendant
`*.prefix.actions.json` still describes actions from an earlier original case
to the adjacent descendant case; it is not a witness starting at that
descendant and receives no manifest from this producer.

## Local-Graph Complete Wins

`.\ol.ps1 contract combat` is the maintained complete-win surface. It admits
the exact root to the explicit V2 case catalog, runs one typed bounded request,
and atomically publishes `manifest.json`, `report.json`, and
`witness.actions.json` when any replay-exact candidate exists. The compact
result distinguishes a candidate witness from a witness satisfying every HP,
potion, and stolen-gold contract.

V2 summary and rerun never parse legacy local-graph reports. Re-import the
exact `CombatCase` and generate a fresh artifact. Counterfactual roots must
first be exported as their own exact cases; the compact contract has no
full-health mutation flag.

The contract defaults to zero potion expenditures and omits explicit potion
discard. Pass a positive potion budget only for an explicit bounded rescue
question. Concrete slot identity, discard, full-health counterfactuals, and
descendant exports remain separate typed laboratories; they are intentionally
absent from the compact contract rather than exposed as another all-flags
surface.

## Historical Combat Witness Exports

`export-historical-combat-witness` replays the journal to the selected combat
root and writes a producer manifest beside the exported actions. This gives a
later evidence audit exact case/action identity without filename inference.
Its command report also carries the source line identity and the selected
combat-root identity. Node ids remain workspace-local and must not be used to
join reports from different workspace files.

Do not join an independently saved case to a run witness by seed, floor,
workspace-local node id, or filename. Use
`export-historical-combat-witness` so the case, actions, producer manifest,
exact combat-state hash, and run-session identity originate from one replayed
journal boundary.

A local-graph `*.prefix.actions.json` beside a descendant case describes the
path from the original root to that descendant. It must not be paired with the
descendant case as though it started there. Such artifacts remain legacy
evidence until their producer carries the original-case identity.

## Combat Laboratory V1

The Combat Laboratory is an offline mode of the dedicated fixed-root
`AtomicExactV2` worker. It does not invoke the production resident portfolio.
Run the maintained seed006-derived Reptomancer pilot with:

```powershell
.\cs.ps1 --lab-spec fixtures/combat_lab/seed006_reptomancer_8x2.lab.json --lab-output artifacts/runs/combat-lab-seed006-pilot --lab-samples 8
```

Rerun the same command and output directory to resume journaled cells. Increase
only `--lab-samples` to extend the deterministic schedule. Resume rejects
changes to the scenario, schedule, profiles, budget, schema, or source
identity.

Each laboratory directory contains:

- `manifest.json`: immutable resolved experiment and provenance;
- `cells.jsonl`: append-only raw evidence and evidence authority;
- `checkpoint.json`: rebuildable resume accelerator;
- `summary.json`: reproducible aggregate.

Exact-replayed wins and losses are distinct from coverage limits and
infrastructure errors. Read outcome rates with their coverage denominators.
The pilot is explicitly seed006-derived and exact-state-oracle: it does not
restore consumed campaign RNG or model a human-visible-information policy.

## Campfire Threat Panel V1

The offline Campfire panel compares every alignable exact candidate against a
declared public encounter pool with matched analysis RNG and shuffle samples.
It never reads the live hidden encounter queue or changes Campfire policy.

```powershell
.\cs.ps1 --threat-panel-spec fixtures/campfire_threat_panel/seed006_pre_transient_reconstructed.panel.json --threat-panel-output artifacts/runs/campfire-threat-panel-seed006-pilot --threat-panel-samples 1
```

The fixture is reconstructed from recorded public deck and resource state. It
does not claim the original hidden RNG or route map. Repeating the same command
resumes completed cells; increasing only the sample target extends the fixed
shuffle schedule.

Read the two lenses separately:

- `actual_consequence` retains each candidate's real post-Campfire resources;
- `root_hp_capability` resets only current HP to isolate equal-starting-HP
  mechanical capability.

Direction reversals are encounter-specific evidence, not a global Campfire
score. Coverage-limited rows may contain replayed candidates but do not prove
search optimality. Historical artifacts remain readable; rerunning a removed
profile requires the Git commit recorded in its manifest.
