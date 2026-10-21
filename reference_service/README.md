# STS reference service

This is an experimental, evidence-only source research service for Slay the Spire.
Daily strategy reading follows the [curated knowledge design](DESIGN.md) and
[selective knowledge reader](../reference/expert/curated/README.md).
It accepts exact simulator identities for cards, relics, potions, enemies,
encounters, and events. It does not rank candidates, choose a winner, or reuse
the former local-judgment recommendation layer.

## Evidence boundary

- The simulator or another typed caller supplies the complete decision surface,
  stable entity IDs, public context, and typed facts. Labels never create
  candidate identity.
- Simulator facts, Wiki mechanics/reference, revision-bound Wiki strategy
  sections, and attributed expert strategy remain separate lanes.
- `matched`, `no_match`, `unknown`, and `error` are retrieval states, not action
  scores.
- Entityless actions such as skipping a reward remain present with an explicit
  reason. They are never assigned a guessed entity.
- Citation locators bind source identity, revision or reviewed passage locator,
  entity identity, and a content hash. Citation lookup verifies that binding
  before returning complete text.
- Historical run sessions and old narrow reference databases are not strategy or
  training evidence. The service reads the current full Wiki/entity graph and
  reviewed expert library directly.

The installed local assets are:

```text
.oracle-lab/external-research/wiki.sqlite3
.oracle-lab/external-research/entities.sqlite3
reference/entities.jsonl
reference/expert/
reference/game-knowledge/
```

## Local setup

Use the repository's isolated Python environment. No separate Neow database
build is required:

```text
.\py.ps1 -Python <python-3.12.exe> -m venv .oracle-lab/reference-service-venv
.\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m pip install -e "reference_service[test]"
```

The retriever and its request types live in this package; there is no LLM
controller dependency. The maintained interface is the local CLI.

## Experimental source-research interface

This typed API is a separate source-research surface; it does not implement
the curated knowledge design.

The CLI supports exact `entity`, complete typed `decision`, hash-bound
`citation`, and lane-specific `diagnose` requests. A strategic run-play head can
export a complete reference request without moving its state. Combat uses its
own exact-search surface; callers can still research explicit typed entities.
CLI output is compact reading text by default; add `--json` to any subcommand
for the complete typed response, including citation locators and diagnostics.

Entity research uses the explicit kind and stable ID throughout lookup and
citation expansion. Display labels cannot redirect it.
A broken reference generation returns explicit `error` coverage while keeping
the candidate identities, public context and caller-supplied facts readable;
it does not fall back to stale indexes. Expert citation expansion verifies the
quoted excerpt and record locator. Missing content pins or changed excerpts or
records require fresh entity research, not a silently rebound old citation.

## Maintainer checks

Research one entity:

```text
.\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m sts_reference.cli entity --kind card --id BattleTrance
```

Export a complete request from a durable strategic head without changing it:

```text
target/dev-opt/run_play.exe reference-request --session .oracle-lab/run-play/<session>
```

Run the maintained narrow suite:

```text
.\py.ps1 -Python .oracle-lab/reference-service-venv/Scripts/python.exe -m pytest reference_service/tests -q
```

Acquisition remains deliberate and provenance-preserving. Add or refresh raw
captures through the maintained Wiki and expert tools, review identity bindings
and evidence class, rebuild the disposable corpus/entity derivatives, and rerun
representative entity cases. Do not patch a missing query with a local winner
table.
