# STS Simulator Tools

`tools/` is the offline tooling layer. Runtime behavior belongs in Rust binaries
and library modules; Python and PowerShell here should launch, inspect, export,
or analyze artifacts.

## Maintained Tool Groups

| Tool | Purpose |
| --- | --- |
| `frozen_case_panel.py` | Run fixed combat/search cases for review panels. |
| `combat_resource_frontier_review.py` | Render a saved exact resource-frontier report as candidate, divergence, turn, action, and cutpoint views without rerunning search. |
| `build_rl_dataset_manifest.py` | Build dataset manifests from exported artifacts. |
| `label_rl_outcomes.py` | Attach outcome labels to exported decision samples. |
| `analyze_imitation_disagreements.py` | Inspect imitation-model disagreements against behavior-policy samples. |
| `train_imitation_candidate_ranker.py` | Train the current offline candidate-ranker baseline. |
| `wiki/` | Resumable MediaWiki API/browser acquisition, complete-bundle verification, normalization, and provenance-preserving SQLite search. |

`tools/ml/` contains combat/search trace extraction and baseline scripts:

| Tool | Purpose |
| --- | --- |
| `combat_tactical_trace_extract.py` | Extract tactical combat traces. |
| `combat_first_action_ranking_baseline.py` | Baseline first-action ranking experiment. |
| `run_tactical_trace_extract.ps1` | Local trace-extraction launcher. |
| `run_tactical_trace_batch.ps1` | Batch trace-extraction launcher. |
| `run_turn_plan_baseline.ps1` | Turn-plan baseline launcher. |
| `run_turn_plan_policy_compare.ps1` | Turn-plan policy comparison launcher. |

## Output Rules

- generated reports and datasets belong under `tools/artifacts/`;
- root-level one-off snapshots belong under `tools/artifacts/root_snapshots/`;
- `__pycache__/`, generated panels, model outputs, and scratch data must stay
  ignored;
- long-lived schemas or tiny sample fixtures should be committed only when they
  are intentional interfaces.
