# Developer commands

These are source-checkout tools. Installing and using the Python wheel does not require them; start with the [installation guide](../docs/getting-started.md).

Run PowerShell scripts with PowerShell 7 from the repository root. The entry points resolve the repository from their own location; individual commands may require Rust, Python, Node.js, local research data, or an original-game installation as documented by their owner.

| Task | Entry point | Usage and owner |
| --- | --- | --- |
| Rust play and inspection | `./scripts/run-play.ps1`, `./scripts/p.ps1` | [Run-play](../tools/run_play/README.md) |
| Node workbench and dsh player inspection | `node scripts/p.mjs` | [Run-play](../tools/run_play/README.md), [dsh plugin](../tools/dsh_player/README.md) |
| Local dsh server | `./scripts/dsh.ps1` | [dsh plugin](../tools/dsh_player/README.md); local launch configuration required |
| Original-game control and diagnostics | `./scripts/java-play.ps1` | [Runbook](../docs/RUNBOOK.md); includes local Windows game and mod paths |
| Exact combat research | `./scripts/ol.ps1`, `./scripts/ol-live.ps1`, `./scripts/ol-contract.ps1`, `./scripts/cs.ps1` | [Combat evidence](../docs/runbooks/combat-evidence.md) |
| Python developer runner | `./scripts/py.ps1` | UTF-8 stdio, explicit interpreter selection, and optional local environment configuration |
| Mechanism and source lookup | `./scripts/wiki.ps1`, `./scripts/reference-service.ps1` | [Reference index](../reference/README.md), [source service setup](../reference_service/README.md) |

## Optional local shortcuts

To keep short root commands such as `./py.ps1`, `./run-play.ps1`, and `node ./p`, run once:

```powershell
./scripts/setup-local.ps1
```

The helper creates small ignored forwarding files. Implementations remain in `scripts/`; the shortcuts contain no game, build, or environment logic. Re-running refreshes generated shortcuts and preserves unrelated files. `./scripts/setup-local.ps1 -Check` checks their contents without writing. The helper starts no game, server, or model.

A fresh checkout can use the documented `scripts/` paths immediately. Creating shortcuts is optional and does not change what gets published.
