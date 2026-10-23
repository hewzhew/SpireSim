# Trajectory verification

[Python interface](../README.md)

For downloaded wheels and the four-character bundle, use [Getting started](../../../docs/getting-started.md#install-and-run-the-full-verification). The commands below operate from a source checkout.

## Export from original-game evidence

To verify the installed package against recorded original-game play, run from the repository root:

```powershell
.\scripts\py.ps1 bindings/python_learning/tests/trajectory.py --evidence <java-evidence> --output <new-artifact-directory>
```

For a manual session that includes native recovery or combat retries, select its source explicitly:

```powershell
.\scripts\py.ps1 bindings/python_learning/tests/trajectory.py --session <manual-session-id> --output <new-artifact-directory>
```

`--repository` selects the owning checkout when needed. This uses the native recovery replay's typed parent prefixes, verifies every adopted scope, and executes the resulting continuous trajectory from Neow. Training trajectory format 2 records each scope's producer, exact report and global action offset. Use `--through N` to stop after N adopted actions of that selected session; its parent prefixes retain their recorded limits. This fixes the acceptance scope when later play appends more actions to the same session. Native reload preparation remains Java execution evidence; it does not become an extra policy decision or training reward.

With `--evidence` or `--session`, the command invokes `sts_lockstep replay --learning-output` to derive fresh expectations from the raw Java evidence and current Rust implementation, then executes every actual Python `choose/step`. It checks symbolic prefixes, public snapshot identities and candidate order, continuation state, rewards and termination. Original Smith opening and Astrolabe's ordered grid clicks are grouped with their completed game action; semantic actions, Python choices and Python steps have separate counts. The output keeps the expected trajectory, actual commit journal, first failure or final result, and the installed native module's SHA-256. This path does not launch Java.

## Verify a frozen export

To verify another platform's installed wheel against the same frozen export, pass `--trajectory <expected.json> --trajectory-sha256 <expected_sha256>` instead of `--evidence` or `--session`. Take the digest from the exporting acceptance report. The runner checks it before creating an environment, then uses the same actual `choose/step`, recovery, public observation and terminal checks. Its report retains the export digest, producer version and original source scopes; it labels this as package verification against a frozen replay export. The exporting run owns the fresh raw Java/Rust comparison. No original-game install or replay executable is needed on the consuming platform.

For example, from a Linux checkout with the wheel installed in the active Python environment:

```bash
python bindings/python_learning/tests/trajectory.py \
  --trajectory expected.json --trajectory-sha256 SHA256_FROM_EXPORT_REPORT \
  --require-build-profile release --stagger-commits 0 20 1 \
  --reject-extra-input-once --output .oracle-lab/package-acceptance/linux
```

Replace the export path and digest with the exporting report's values, and use a new output directory for each run. This executes three slots at different progress points, including one starting at Neow. A complete Heart trajectory checks every slot through the formal terminal, with an atomic bad-input rejection followed by valid continuation.

A frozen export records the exporting implementation's public observations and candidate semantics. When those projections change, derive a new export from the original evidence with the current replay executable, then verify the rebuilt wheel against it. Keep the old export and its digest with the package it verified; package version alone does not identify a matching export.

When validating an installed release wheel with `tests/trajectory.py`, pass `--require-build-profile release`; the runner checks the imported native build before replay and records its actual module path and SHA-256 in the report.

## Staggered slots and rejected input

The same acceptance runner supports batches with `--stagger-commits 0 20 1`. Each number is a source commit offset for one slot; zero starts at Neow. The runner creates each checkpoint in its own slot by executing the adopted prefix, then restores those slot-bound checkpoints together. Different offsets require a source ending in an actual game terminal. Repeated zero offsets can compare several slots through the same nonterminal prefix.

Every pending row is checked against its own source commit and selection prefix. Ready and terminal slots leave the decision rows; transition paths, rewards and terminal columns retain their actual slot indices. The report separates prefix preparation from checked batch calls and records each slot's start, executed semantic actions and final resources. These slots are recovery curricula on the same adopted trajectory.

`--reject-extra-input-once` requires multiple slots. It supplies one surplus external input to the last active slot, checks that rejection preserves the whole batch's exact state and prepared choices, then commits the original valid inputs. A source with a recorded external time sample exercises that boundary.

## Recorded external inputs

`step(execution_inputs_json=...)` supplies recorded external inputs as one `{"secret_portal_playtime_bits": [...]}` record per active slot. Values are the original float32 bit patterns. Exact consumption is required; rejection leaves the whole batch and pending choices unchanged. Ordinary `step()` uses the environment's own inputs. `continuation_fingerprints()` is a private diagnostic for acceptance and must stay outside model features.
