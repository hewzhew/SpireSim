# Performance Investigation

Measure the smallest production-shaped surface that can answer the question.
Timing is admissible only when source, executable, symbols, workload, exact
counters, and witness identity are all controlled.

Architecture direction and current tool roles are maintained in
[the architecture](../ARCHITECTURE.md) and [the runbook](../RUNBOOK.md). This
page owns commands and acceptance rules, not historical machine timings or the
parity workflow.

## Native CPU Trace

Record a symbolized trace of the narrow combat contract with:

```powershell
.\tools\perf\profile_combat_cpu.ps1
```

Analyze an existing trace without recording again or requesting elevation:

```powershell
.\tools\perf\summarize_combat_cpu.ps1 -Trace .profiles\combat-cpu-<id>.etl
```

The summary normalizes samples across only the recorded `combat_contract`
processes. The recorder publishes the matching Rust PDB into an ignored,
identity-keyed local symbol cache before WPR starts so an older trace remains
symbolizable after later builds. The summarizer rejects insufficient symbol
resolution instead of producing a hotspot table dominated by unknown frames.

PerfView is an ignored local analysis dependency below `.profiles/tools`; it
is not a repository artifact.

## Exact Contract Benchmark

Use the canonical workload without WPR or elevation:

```powershell
.\tools\perf\benchmark_combat_contract.ps1
```

The script builds the narrow target once, warms it, reports batched process
wall time, and rejects iterations whose exact counters or witness differ. The
benchmark and profiler source the same workload definition.

Both tools write a content-addressed build receipt beside the profiling
executable. `-SkipBuild` verifies the complete Rust/Cargo source scope plus the
executable and PDB. Rebuild without `-SkipBuild` after any relevant source
change.

## Turn Guidance Experiment

Compare uniform enumeration with an experimental depth guide through the same
generator and replay core. Targeted selection trials also compare an explicit
Sentinel-exhaust preference, including a counterexample that needs its block.

Sparse multi-selection trials compare replacing Wounds with replacing the whole
hand. The counterexample needs to retain its finisher while drawing energy.
These explicit probe policies read the current hand, not the witness or draw order.

```powershell
cargo run --locked -p sts_combat_planner --release --example turn_guidance_probe -- .oracle-lab/diagnostics/<new-directory>
```

The output directory must be new and its parent must exist. The probe validates
an explicit witness for each constructed input before searching, records the
first matching outcome and work-budget curve, replays every produced option,
checks split versus one-shot budgets, and compares complete successor sets when
both schedules finish. Missing bounded results remain unknown. Hand layouts and
shuffle inputs exercise sensitivity; they are correlated diagnostic cases, not
independent game seeds or a strategy win-rate benchmark.

Full inputs, preparation transitions, witnesses and producer hashes stay in the
output directory. Keep the matching source commit/diff and build log with it.
Search timings exclude fixture preparation, replay verification, serialization
and build time. This diagnostic does not change the default policy, operate a
run, or invoke Java. Fixtures and the two experimental policies are owned by
[the probe](../../crates/sts_combat_planner/examples/turn_guidance_probe.rs).

## Identity-Locked Combat Panel

Before changing state ownership or another cross-cutting hot path, run:

```powershell
.\tools\perf\benchmark_combat_panel.ps1 -SkipBuild
```

The panel interleaves maintained light, long, large-state, and
replay-verified combat cases under one fixed work contract. Timing is
observational; deterministic counters and witness identity must match the
panel specification before accepting a result.

To diagnose transition ownership costs with sparse sampling:

```powershell
.\tools\perf\benchmark_combat_panel.ps1 -SkipBuild -ProfileTransitionCloneCost
```

The diagnostic reports engine clone, combat clone, and transition execution
costs. Its wall time is instrumented and must not be used for performance
acceptance; use the ordinary panel for before/after timing.

## Build Boundaries

Oracle work uses one canonical incremental `release` artifact. The narrow
`.\ol-contract.ps1` surface excludes the run explorer and resident runtime, so
planner changes do not require linking the full oracle host. Use
`release-final` only for a deliberately requested fully optimized deployment
artifact.

Splitting a Rust source file into modules changes ownership and review scope,
not the Cargo compilation unit. Claim rebuild improvements only after a
measured crate-boundary change with matched source invalidation.

Do not configure a project-wide `sccache` wrapper for the canonical iterative
build. That profile deliberately uses incremental compilation, which sccache
does not cache. Reconsider it only for a separately measured non-incremental CI
or same-path recovery workflow.

Cargo's incremental directories are disposable build products, but this large
workspace can accumulate many generations across `dev`, `dev-opt`, and
`release`. Inspect the Cargo-metadata-resolved target without deleting anything:

```powershell
.\tools\maintenance\cargo-cache.ps1
```

Clean one profile only after the report identifies it:

```powershell
.\tools\maintenance\cargo-cache.ps1 -Action CleanProfile -Profile release
```

A full clean is intentionally harder to invoke and still delegates deletion to
Cargo after validating the workspace and target directory:

```powershell
.\tools\maintenance\cargo-cache.ps1 -Action CleanAll -ConfirmAllBuildArtifacts
```

`.oracle-lab`, native checkpoints, source files, and Java isolated storage are
outside Cargo's target and are never part of this command. Keep iterative
incremental settings until a matched build-time and disk-use measurement proves
that changing a specific profile is better; do not disable every profile in
response to one cache incident.

## Reporting Results

For phase timings on an existing lockstep command, set `STS_LOCKSTEP_PROFILE`
to a new absolute JSON file path whose parent already exists, then run the
normal command (for example `sts_lockstep manual-inspect`). The CLI reserves
the output before execution, records its arguments and executable hash, and
saves a diagnostic report on normal success or failure. It does not forward
the output path to spawned hosts. No JVM is needed for offline inspection.

The shared core contributes nested spans for evidence loading, replay, adapter
preparation/execution, projection, comparison and persistence. Manual input
waiting is separate. `total_ns` includes descendant spans; use `self_ns` for
additive accounting. Receipt-reported bridge send-to-stable-successor intervals
are separate observations, not pure Java CPU time or additional additive spans.
Offline replay has no live bridge interval. Missing intervals remain absent.
Capture is thread-local; executable hashing and report writing are outside the
measured command. Reports do not enter journals, affect comparison, or authorize
continuation. Pair timings with the exact source/build and workload evidence.

For Java settlement phases, launch the managed host with
`STS_JAVA_SETTLEMENT_PROFILE=1`. CommunicationMod then writes
`parity_settlement_profile` records to the isolated game's
`sendToDevs/logs/SlayTheSpire.log`. These records link an accepted request and its
source/first emitted stable frame, report the observed original-game fast-mode setting, and
attribute sampled wall time to the current action or transition phase. They are
not Java CPU timings or parity evidence: match the emitted frame to the bridge
receipt before treating the interval as settlement, since an unchanged frame can
be emitted during a transition. Sampling is disabled by default; capture
the log with its launch plan before releasing the owned isolated storage. Compare
the same adopted workload and exact results when accepting a speed improvement.

For a read-only measurement of native recovery preparation, explicitly select
a repository with current manual evidence:

```powershell
$env:STS_NATIVE_PROFILE_REPOSITORY = 'D:\rust\sts_simulator'
cargo test --locked -p sts_parity_lockstep_runner --lib native::tests::profile_selected_native_preparation -- --ignored --nocapture
```

This opt-in probe does not contact Java or alter the active session. It measures
generation selection and combined entry/terminal preparation, then independently
replays the entry and terminal with the same executable. The reference must
agree on the full Rust entry, opaque Java identity bindings, terminal snapshot,
and adopted suffix. Build time is separate from the reported phase timings;
record the actual Cargo profile and executable identity with the results.

To compare native capture against a selected session's recorded save scope:

```powershell
$env:STS_NATIVE_PROFILE_REPOSITORY = 'D:\rust\sts_simulator'
$env:STS_NATIVE_CAPTURE_PROFILE_SESSION = '<explicit-session-id>'
$env:STS_NATIVE_CAPTURE_PROFILE_OUTPUT = '<new-absolute-report-path>'
cargo test --locked -p sts_parity_lockstep_runner --lib native::tests::profile_recorded_native_capture -- --exact --ignored --nocapture
```

The probe reconstructs the selected local prefix and original save bytes in a
temporary repository. It compares full evidence discovery and the live writer's
borrowed anchor, with alternating order after warmup. Source artifacts and Java
remain untouched. Exact source observations and native bytes must agree on every
capture. Timings exclude reconstruction, ancestor reads, build time and Java;
they are not an end-to-end throughput claim. Keep the report's executable and
workload identities with the source commit/diff used to build it.

Report:

- source commit and dirty state;
- exact command and workload identity;
- whether a build was performed or receipt-verified;
- accepted and rejected iteration counts;
- deterministic counter and witness agreement;
- aggregate timing and a short hotspot table;
- environmental caveats that could affect comparison.

Store full traces and logs in ignored artifact roots and surface only
aggregates plus a short failure tail. Machine-specific observations are not CI
thresholds unless a separate maintained contract explicitly promotes them.
