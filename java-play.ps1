#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = 'Stop'
$repository = $PSScriptRoot
$liveBinary = Join-Path $repository 'target\debug\sts_java_parity_live.exe'
$liveBuildReceipt = Join-Path $repository 'target\debug\sts_java_parity_live.build-receipt.json'
$compareBinary = Join-Path $repository 'target\debug\sts_java_parity_compare.exe'
$compareBuildReceipt = Join-Path $repository 'target\debug\sts_java_parity_compare.build-receipt.json'
$suiteBinary = Join-Path $repository 'target\debug\sts_java_parity_suite.exe'
$suiteBuildReceipt = Join-Path $repository 'target\debug\sts_java_parity_suite.build-receipt.json'
$liveOutput = Join-Path $repository '.oracle-lab\java-live'
$launchOutput = Join-Path $repository '.oracle-lab\java-launch'
$probeOutput = Join-Path $repository '.oracle-lab\java-probes'
$scenarioOutput = Join-Path $repository '.oracle-lab\java-scenarios'
$scenarioCaseCatalog = Join-Path $repository 'fixtures\java_parity_cases'
$scenarioProbeCaseCatalog = Join-Path $repository 'fixtures\java_parity_probe_cases'
$scenarioTrajectoryCaseCatalog = Join-Path $repository 'fixtures\java_parity_trajectory_cases'
$scenarioSuiteOutput = Join-Path $repository '.oracle-lab\java-suites'
$scenarioSuiteCatalog = Join-Path $repository 'fixtures\java_parity_suites'
$scenarioSharedRuntimeSuiteCatalog = Join-Path $repository 'fixtures\java_parity_shared_runtime_suites'
$scenarioSharedRuntimeTrajectorySuiteCatalog = Join-Path $repository 'fixtures\java_parity_shared_runtime_trajectory_suites'
$pairedRootOutput = Join-Path $repository '.oracle-lab\paired-root-attempts'
$pairedRootStore = Join-Path $repository '.oracle-lab\paired-root-store'
$pairedRootResidentOutput = Join-Path $repository '.oracle-lab\paired-root-residents'
$runPlayRoot = Join-Path $repository '.oracle-lab\run-play'
$runPlayWorkbench = Join-Path $repository 'target\dev-opt\run_play.exe'
$communicationRepository = Join-Path (Split-Path $repository -Parent) 'CommunicationMod'
$communicationWrapper = Join-Path $communicationRepository 'mod.ps1'
$communicationConfig = Join-Path $env:LOCALAPPDATA 'ModTheSpire\CommunicationMod\config.properties'
$gameDirectory = 'C:\Program Files (x86)\Steam\steamapps\common\SlayTheSpire'
$communicationModJar = Join-Path $gameDirectory 'mods\CommunicationMod.jar'
$superFastModeJar = Join-Path $gameDirectory 'mods\SuperFastMode.jar'
$workshopDirectory = 'C:\Program Files (x86)\Steam\steamapps\workshop\content\646570'
$superFastModeConfig = Join-Path $env:LOCALAPPDATA 'ModTheSpire\SuperFastMode\SuperFastModeConfig.properties'
$modTheSpireJar = Join-Path (Split-Path $repository -Parent) 'lib\ModTheSpire.jar'
$gameJava = Join-Path $gameDirectory 'jre\bin\javaw.exe'
$isolatedStorageParent = Join-Path $gameDirectory '.sts-java-play\isolated-storage'
$isolatedStartupTimeoutSeconds = 60
. (Join-Path $repository 'tools\cargo_artifact_contract.ps1')

function Show-JavaPlayHelp {
    @'
Usage: .\java-play.ps1 <command> ...

  .\java-play.ps1 build
      Build the typed live bridge, shared protocol, offline comparator, and
      fresh-runtime suite owner.

  .\java-play.ps1 install
      Build the Rust bridge and CommunicationMod, deploy the audited Mod JAR,
      back up the CommunicationMod config, and select the live bridge command.
      Slay the Spire and ModTheSpire must be closed.

  .\java-play.ps1 launch [--profile strict|accelerated] [--control combat|run]
      Launch without the ModTheSpire selection window. strict (the default)
      loads BaseMod + CommunicationMod. accelerated additionally loads the
      exact installed SuperFastMode and records its JAR and config identity.
      combat (the default) preserves the narrow parity baseline. run grants
      typed main-menu, Neow/map, and combat control without mouse automation.
      The game remains visible and uses installed_game storage; no mouse
      automation is used. Constructed scenarios use the separate managed
      command below so their saves and preferences stay isolated.

  .\java-play.ps1 status [--json]
      Show the newest running live session and frame-bound fN:aN actions.

  .\java-play.ps1 act <fN:aN|exact-action-id> [--timeout-seconds N]
      Submit exactly one action from the latest Java-owned action space and
      wait for its create-new result/successor receipt.

  .\java-play.ps1 start-run --character IRONCLAD --ascension 20 [--seed 0]
      At a typed main-menu frame, start one run with structured parameters.

  .\java-play.ps1 continue-run [--content-profile current_profile|all_unlocked]
      At a typed main-menu frame, apply the explicit in-memory content profile
      and invoke the original Continue button without editing native storage.

  .\java-play.ps1 hand-select [--card-uuid UUID ...] [--timeout-seconds N]
      At a typed HAND_SELECT frame, atomically submit the ordered UUID set.
      Omit --card-uuid for a legal zero-card confirmation such as Elixir.

  .\java-play.ps1 grid-select --card-uuid UUID [--timeout-seconds N]
      At the capability-closed GRID_SELECT frame, submit one advertised UUID.

  .\java-play.ps1 probe elixir-hand-select
      Launch two isolated accelerated run-control sessions, naturally replay
      the zero-card and first-Strike Elixir variants, prove typed invariants,
      create lifecycle receipts plus a workflow-bound proof bundle, and close
      only the managed processes. Passing UUID sandboxes are removed.

  .\java-play.ps1 probe natural-base-strike
      Launch one isolated accelerated run-control session, naturally travel
      from a fresh main menu to seed-0 A20 Ironclad's first combat, select the
      minimum-hand-index Strike occurrence from the Java action space, compare
      its adjacent transition with Rust, and promote natural reachability only
      in a lifecycle-bound workflow. This is not strategy or training evidence.

  .\java-play.ps1 mirror plan --run-play-head <head.json> --output <fresh-plan.json> --seed <decimal-engine-u64> [--character IRONCLAD] [--ascension 20] [--scope first-reward|active-prefix]
      Compile the authoritative active path of one retained run-play head that
      has completed its first combat reward into a read-only current V4 mirror
      plan. The default scope stops at the following Map; active-prefix continues
      through every currently supported retained action to the head's current
      decision and carries a typed before-state for every combat action. The seed
      is the simulator's decimal unsigned Settings.seed bit pattern, not the
      game's base-35 display text. The plan does not mutate the Rust session or
      claim parity.

  .\java-play.ps1 mirror run --plan <mirror-plan.json>
      Launch one isolated accelerated run-control session and execute only the
      compiled continuous prefix. Resolve every action from the current Java
      action space and stop at the first typed mismatch.

  .\java-play.ps1 mirror checkpoint --plan <mirror-plan.json>
      Execute one continuous prefix ending at a combat root and preserve the
      exact native ENTER_ROOM autosave plus a lifecycle-bound manifest.

  .\java-play.ps1 mirror suffix --checkpoint <checkpoint.json> --plan <extended-plan.json>
      Restore one validated native combat checkpoint in fresh isolated storage
      and execute only the target plan's exact suffix. A non-empty suffix that
      reaches a new room's combat root emits a parent-bound next checkpoint.

  .\java-play.ps1 mirror reuse-probe --checkpoint <checkpoint.json> --plan <extended-plan.json>
      Compatibility sentinel for batch with the same typed plan supplied as
      two independently reported members.

  .\java-play.ps1 mirror batch --checkpoint <checkpoint.json> --plan <plan.json> --plan <plan.json> [...]
      Preflight two to sixty-four non-empty combat suffix members against one
      native checkpoint, then execute each in one isolated JVM with a typed,
      save-preserving reset between members. Every member remains a separate
      execution artifact; inputs are closed before launch.

  .\java-play.ps1 paired-root prepare --checkpoint <checkpoint.json> --plan <mirror-plan.json> --session <run-play-session> --at <retained-selector> [--store <artifact-store>]
      Restore one validated native checkpoint in fresh isolated storage, stop
      at its stable combat frame, and run the first probe-only PairedRoot
      preflight against one retained Rust decision. The source run-play head
      is guarded by a before/after inventory hash; the Java sandbox is closed
      after a ready or typed preflight_stopped report.

  .\java-play.ps1 paired-root actions --root <root-id> [--store <artifact-store>]
      Read the immutable root's exact Java/Rust action-space and combat
      projections, verify they still agree, and print stable typed handles.
      This is artifact-only and does not launch or require the game.

  .\java-play.ps1 paired-root preview --checkpoint <checkpoint.json> --plan <mirror-plan.json> --session <run-play-session> --at <retained-selector> --action <paired-action-id> [--store <artifact-store>]
      In one isolated Java runtime, prepare the paired root, resolve one
      source-bound stable typed handle, execute it once in Java and Rust, and
      write either an exact child-root receipt or a typed divergence. It never
      moves the source run-play head.

  .\java-play.ps1 paired-root resume-preview --target-root <root-id> --action <paired-action-id> [--action <paired-action-id> ...] [--store <artifact-store>] [--output text|json]
      Resolve the target root's immutable native launch payload and retained
      Rust checkpoint from the artifact store, automatically reuse a compatible
      neutral resident or open one, replay the stable commitment chain, and
      independently preview one or more typed actions. Every member gets a fresh
      activation and receipt and typed-resets the resident for later calls.
      Callers do not supply checkpoint, mirror plan, resident ID, session, reset,
      or retained selector; incompatible builds or semantic contracts stop
      before a target activation is issued. JSON output is the typed worker
      boundary and contains no human preamble.

  .\java-play.ps1 paired-root resume-witness --target-root <root-id> --witness-plan <typed-plan.json> [--store <artifact-store>] [--output text|json]
      Internal sequential-witness transaction. Continue and recover the initial
      root once, re-resolve each typed intent at its Exact child, compare every
      Java/Rust successor, then reset once. The plan is closed typed evidence;
      this command does not accept action handles, Java UUIDs, or run sessions.

  .\java-play.ps1 paired-root resident --mode open --target-root <root-id> [--store <artifact-store>]
  .\java-play.ps1 paired-root resident --mode preview --resident <resident-id> --target-root <root-id> --action <action-id>
  .\java-play.ps1 paired-root resident --mode status|close --resident <resident-id>
      Development-only lifecycle proof for a cross-invocation isolated Java
      resident. The immutable manifest binds exact process start identities,
      launch plan, native bundle, session/connection, sandbox, autosave and
      source-storage guard. Preview independently activates one compatible root,
      executes one stable action handle, compares Java/Rust, and returns to the
      neutral Continue boundary. An OS-owned exclusive lease serializes each
      cross-invocation operation without a stale busy flag. A clean close stops
      only those exact processes and removes only its verified isolated sandbox.

  .\java-play.ps1 scenario create-combat (--case <exact-case-id> | --draft <draft.json>) [--profile strict|accelerated]
      Launch one isolated scenario-control session, turn a tracked exact case
      or explicit ad-hoc draft into a fresh immutable plan, construct the
      combat at Neow, prove the stable root as constructed_validated, record
      lifecycle/storage evidence, and close managed processes. Failed
      sandboxes are retained for diagnosis.

  .\java-play.ps1 scenario registry
      Show the supported orchestration recipe, closed typed capabilities,
      dependencies, cardinality, and commit phase without launching the game.

  .\java-play.ps1 scenario cases
      Validate and show tracked scenario drafts by exact case ID and SHA-256.

  .\java-play.ps1 scenario probe-cases
      Validate and show tracked root-plus-transition probe drafts by exact ID.

  .\java-play.ps1 scenario trajectories
      Validate and show tracked root-plus-ordered-transition drafts by exact ID
      without launching the game.

  .\java-play.ps1 scenario suites
      Validate and show tracked fresh-runtime suite manifests by exact ID.

  .\java-play.ps1 scenario shared-suites
      Validate and show tracked shared-runtime suite manifests by exact ID.

  .\java-play.ps1 scenario shared-trajectory-suites
      Validate and show tracked shared-runtime trajectory suite manifests.

  .\java-play.ps1 scenario probe --probe-case <exact-probe-case-id> [--profile strict|accelerated]
      Compile a root-and-transition-bound immutable probe plan before launch,
      construct its isolated combat root, submit the closed typed transition,
      and preserve the adjacent original-game receipt without claiming a Rust
      comparison.

  .\java-play.ps1 scenario trajectory --trajectory-case <exact-trajectory-case-id> [--profile strict|accelerated] [--step-index <zero-based-index>]...
      Compile one exact root and its complete ordered typed transition list
      before launch, then resolve and execute each step from successive Java
      action spaces. With no step index, compare every transition and bind the
      clean managed lifecycle into one scoped trajectory workflow. With one or
      more step indices, compare only those transitions and publish an honest
      selected-step attempt without first failing an all-step comparison.

  .\java-play.ps1 scenario verify --probe-case <exact-probe-case-id> [--profile strict|accelerated]
      Run the same isolated typed probe, compare its exact adjacent receipt
      with the Rust production stepper, and create a workflow report binding
      the still-separate root, capture, and scoped-comparison evidence.

  .\java-play.ps1 scenario recompare --probe-report <probe-report.json>
  .\java-play.ps1 scenario recompare-trajectory --trajectory-report <trajectory-report.json> --step-index <zero-based-index>
      Re-run only the Rust comparator from one completed, plan-bound
      constructed probe. Create a fresh comparison/workflow directory without
      launching Java, parsing an action, or modifying the original capture.

  .\java-play.ps1 scenario reuse-probe (--case <exact-case-id> | --draft <draft.json> | --probe-case <exact-probe-case-id>) [--profile strict|accelerated]
      In one isolated shared JVM, construct one exact root, perform a typed
      reset to a proven fresh main menu, and construct a second independently
      identified copy of the same root. With --probe-case, capture the same
      typed transition on both roots, reset from the first action successor,
      then compare both immutable captures offline. This is prerequisite
      lifecycle evidence, not yet a production warm suite.

  .\java-play.ps1 scenario suite --suite <exact-suite-id>
      Run every typed suite member sequentially with its own fresh process and
      isolated storage, then bind the member workflows into one aggregate.
      This reduces supervision; it does not reuse a JVM or hide startup cost.

  .\java-play.ps1 scenario shared-suite --suite <exact-suite-id>
      Compile all member UUID plans before launch, capture every member and
      inter-member reset in one isolated JVM, then shut down and bind every
      offline comparison plus the managed lifecycle into one aggregate.
      This reduces repeated startup cost while keeping per-member evidence.

  .\java-play.ps1 scenario shared-trajectory-suite --suite <exact-suite-id>
      Compile every complete trajectory plan before launch, capture all
      ordered steps and typed inter-member resets in one isolated JVM, then
      compare each immutable trajectory offline and bind one managed lifecycle.

  .\java-play.ps1 doctor
      Inspect bridge freshness, installed Mod identity, config, and processes.

The live session, frames, requests, receipts, and transcript are written below
.oracle-lab/java-live. A session directory is never reused or overwritten.
'@
}

function Assert-LiveBridgeFresh {
    $freshnessArguments = @{
        Executable = $liveBinary
        Repository = $repository
        Label = 'Java parity live bridge'
        RebuildCommand = '.\java-play.ps1 build'
        CargoReceipt = $liveBuildReceipt
        CargoPackage = 'sts_java_parity_live'
        AdditionalInputs = @(
            'crates/sts_java_parity_live/Cargo.toml',
            'crates/sts_java_parity_protocol/Cargo.toml'
        )
    }
    Assert-CargoArtifactFresh @freshnessArguments
}

function Assert-JavaParityComparatorFresh {
    $freshnessArguments = @{
        Executable = $compareBinary
        Repository = $repository
        Label = 'Java parity offline comparator'
        RebuildCommand = '.\java-play.ps1 build'
        CargoReceipt = $compareBuildReceipt
        CargoPackage = 'sts_java_parity_compare'
        AdditionalInputs = @(
            'crates/sts_java_parity_compare/Cargo.toml',
            'crates/sts_java_parity_live/Cargo.toml',
            'crates/sts_java_parity_protocol/Cargo.toml'
        )
    }
    Assert-CargoArtifactFresh @freshnessArguments
}

function Assert-JavaParitySuiteFresh {
    $freshnessArguments = @{
        Executable = $suiteBinary
        Repository = $repository
        Label = 'Java parity suite owner'
        RebuildCommand = '.\java-play.ps1 build'
        CargoReceipt = $suiteBuildReceipt
        CargoPackage = 'sts_java_parity_compare'
        AdditionalInputs = @(
            'crates/sts_java_parity_compare/Cargo.toml',
            'crates/sts_java_parity_live/Cargo.toml',
            'crates/sts_java_parity_protocol/Cargo.toml'
        )
    }
    Assert-CargoArtifactFresh @freshnessArguments
}

function Invoke-LiveBridge {
    param([Parameter(Mandatory)] [string[]] $CommandArguments)
    if (-not (Test-Path -LiteralPath $liveBinary -PathType Leaf)) {
        throw "Java parity live bridge is missing: $liveBinary. Run .\java-play.ps1 build."
    }
    & $liveBinary @CommandArguments
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

function Invoke-JavaPlayBuild {
    Push-Location $repository
    try {
        & cargo build --locked -p sts_java_parity_live -p sts_java_parity_compare
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
        Write-CargoArtifactBuildReceipt -Executable $liveBinary -Repository $repository -Receipt $liveBuildReceipt -CargoPackage 'sts_java_parity_live' -AdditionalInputs @(
            'crates/sts_java_parity_live/Cargo.toml',
            'crates/sts_java_parity_protocol/Cargo.toml'
        )
        Write-Output "Java parity live build receipt refreshed: $liveBuildReceipt"
        Write-CargoArtifactBuildReceipt -Executable $compareBinary -Repository $repository -Receipt $compareBuildReceipt -CargoPackage 'sts_java_parity_compare' -AdditionalInputs @(
            'crates/sts_java_parity_compare/Cargo.toml',
            'crates/sts_java_parity_live/Cargo.toml',
            'crates/sts_java_parity_protocol/Cargo.toml'
        )
        Write-Output "Java parity comparator build receipt refreshed: $compareBuildReceipt"
        Write-CargoArtifactBuildReceipt -Executable $suiteBinary -Repository $repository -Receipt $suiteBuildReceipt -CargoPackage 'sts_java_parity_compare' -AdditionalInputs @(
            'crates/sts_java_parity_compare/Cargo.toml',
            'crates/sts_java_parity_live/Cargo.toml',
            'crates/sts_java_parity_protocol/Cargo.toml'
        )
        Write-Output "Java parity suite build receipt refreshed: $suiteBuildReceipt"
    } finally {
        Pop-Location
    }
}

function Get-RunningSlayTheSpireProcesses {
    return @(Get-Process -Name 'SlayTheSpire','ModTheSpire','java','javaw' -ErrorAction SilentlyContinue | Where-Object {
        $_.ProcessName -in @('SlayTheSpire','ModTheSpire') -or
        $_.Path -eq $gameJava -or
        $_.MainWindowTitle -like '*Slay the Spire*'
    })
}

function Assert-GameClosed {
    $running = @(Get-RunningSlayTheSpireProcesses)
    if ($running.Count -gt 0) {
        $detail = ($running | ForEach-Object { "$($_.ProcessName):$($_.Id)" }) -join ', '
        throw "Close Slay the Spire and ModTheSpire before installing or launching ($detail)."
    }
}

function Get-JavaPlayFileIdentity([string] $Path) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
        throw "Required launch artifact does not exist: $fullPath"
    }
    $item = Get-Item -LiteralPath $fullPath
    return [ordered]@{
        path = $fullPath
        length = $item.Length
        sha256 = (Get-FileHash -LiteralPath $fullPath -Algorithm SHA256).Hash
    }
}

function Resolve-JavaPlayModJar([string] $FileName) {
    $local = Join-Path $gameDirectory "mods\$FileName"
    if (Test-Path -LiteralPath $local -PathType Leaf) {
        return [IO.Path]::GetFullPath($local)
    }
    $candidates = @(
        Get-ChildItem -LiteralPath $workshopDirectory -Filter $FileName -File -Recurse -ErrorAction SilentlyContinue |
            Select-Object -ExpandProperty FullName
    )
    if ($candidates.Count -ne 1) {
        throw "Expected one installed $FileName across the local mods and Slay the Spire Workshop directories; found $($candidates.Count)."
    }
    return [IO.Path]::GetFullPath($candidates[0])
}

function Get-JavaPlayModIdentity([string] $Path) {
    $file = Get-JavaPlayFileIdentity $Path
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($file.path)
    try {
        $entry = $archive.GetEntry('ModTheSpire.json')
        if ($null -eq $entry) {
            throw "Mod JAR has no ModTheSpire.json: $($file.path)"
        }
        $stream = $entry.Open()
        $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::UTF8)
        try {
            $metadata = $reader.ReadToEnd() | ConvertFrom-Json
        } finally {
            $reader.Dispose()
            $stream.Dispose()
        }
    } finally {
        $archive.Dispose()
    }
    if ([string]::IsNullOrWhiteSpace($metadata.modid) -or [string]::IsNullOrWhiteSpace($metadata.version)) {
        throw "Mod JAR metadata is missing modid or version: $($file.path)"
    }
    return [ordered]@{
        id = [string] $metadata.modid
        version = [string] $metadata.version
        jar = $file
    }
}

function Get-JavaPlayStorageInventory([string] $Root) {
    $fullRoot = [IO.Path]::GetFullPath($Root)
    $entries = [Collections.Generic.List[object]]::new()
    foreach ($relativeRoot in @('preferences','betaPreferences','saves','runs','sendToDevs','communication_mod_errors.log')) {
        $candidate = Join-Path $fullRoot $relativeRoot
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            $files = @(Get-Item -LiteralPath $candidate)
        } elseif (Test-Path -LiteralPath $candidate -PathType Container) {
            $files = @(Get-ChildItem -LiteralPath $candidate -File -Recurse -Force | Sort-Object FullName)
        } else {
            continue
        }
        foreach ($file in $files) {
            $entries.Add([ordered]@{
                path = [IO.Path]::GetRelativePath($fullRoot, $file.FullName).Replace('\','/')
                length = $file.Length
                sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            })
        }
    }
    $entryArray = @($entries.ToArray() | Sort-Object { $_['path'] })
    $totalBytes = [long]0
    foreach ($entry in $entryArray) {
        $totalBytes += [long]$entry['length']
    }
    $canonical = ConvertTo-Json -InputObject $entryArray -Depth 5 -Compress
    $hash = [Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($canonical))
    return [ordered]@{
        root = $fullRoot
        fingerprint_sha256 = [Convert]::ToHexString($hash).ToLowerInvariant()
        file_count = $entryArray.Count
        total_bytes = $totalBytes
        entries = $entryArray
    }
}

function Get-JavaPlayStorageInventoryWithRetry([string] $Root, [int] $TimeoutSeconds = 5) {
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            return Get-JavaPlayStorageInventory $Root
        } catch {
            if ([DateTimeOffset]::UtcNow -ge $deadline) {
                throw "Timed out waiting for a stable storage inventory at ${Root}: $($_.Exception.Message)"
            }
            Start-Sleep -Milliseconds 100
        }
    } while ($true)
}

function New-JavaPlayIsolatedStorage([string] $SeedStorageRoot) {
    [IO.Directory]::CreateDirectory($isolatedStorageParent) | Out-Null
    $isolationId = [Guid]::NewGuid().ToString()
    $root = Join-Path $isolatedStorageParent $isolationId
    [IO.Directory]::CreateDirectory($root) | Out-Null
    try {
        New-Item -ItemType HardLink -Path (Join-Path $root 'desktop-1.0.jar') -Target (Join-Path $gameDirectory 'desktop-1.0.jar') | Out-Null
        New-Item -ItemType Junction -Path (Join-Path $root 'mods') -Target (Join-Path $gameDirectory 'mods') | Out-Null
        New-Item -ItemType Junction -Path (Join-Path $root 'jre') -Target (Join-Path $gameDirectory 'jre') | Out-Null
        $seedRoot = if ([string]::IsNullOrWhiteSpace($SeedStorageRoot)) {
            $null
        } else {
            [IO.Path]::GetFullPath($SeedStorageRoot)
        }
        foreach ($directory in @('preferences','betaPreferences')) {
            $source = Join-Path $(if ($null -eq $seedRoot) { $gameDirectory } else { $seedRoot }) $directory
            if (Test-Path -LiteralPath $source -PathType Container) {
                Copy-Item -LiteralPath $source -Destination $root -Recurse
            } else {
                [IO.Directory]::CreateDirectory((Join-Path $root $directory)) | Out-Null
            }
        }
        if ($null -ne $seedRoot -and (Test-Path -LiteralPath (Join-Path $seedRoot 'saves') -PathType Container)) {
            Copy-Item -LiteralPath (Join-Path $seedRoot 'saves') -Destination $root -Recurse
        } else {
            [IO.Directory]::CreateDirectory((Join-Path $root 'saves')) | Out-Null
        }
        foreach ($directory in @('runs','sendToDevs')) {
            [IO.Directory]::CreateDirectory((Join-Path $root $directory)) | Out-Null
        }
        foreach ($file in @('config.json','info.displayconfig','steam_appid.txt','twitchconfig.txt')) {
            $source = Join-Path $gameDirectory $file
            if (Test-Path -LiteralPath $source -PathType Leaf) {
                Copy-Item -LiteralPath $source -Destination (Join-Path $root $file)
            }
        }
    } catch {
        throw "Failed to prepare isolated Java game storage at ${root}: $($_.Exception.Message)"
    }
    return [pscustomobject]@{
        Class = 'isolated_working_directory_v1'
        WorkingDirectory = [IO.Path]::GetFullPath($root)
        SourceGameDirectory = [IO.Path]::GetFullPath($gameDirectory)
        IsolationId = $isolationId
    }
}

function Get-JavaPlayAutosaveSummary([string] $Path) {
    $encoded = [IO.File]::ReadAllText([IO.Path]::GetFullPath($Path))
    $bytes = [Convert]::FromBase64String($encoded)
    $key = [Text.Encoding]::UTF8.GetBytes('key')
    for ($index = 0; $index -lt $bytes.Length; $index++) {
        $bytes[$index] = $bytes[$index] -bxor $key[$index % $key.Length]
    }
    $save = [Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json -Depth 100
    return [ordered]@{
        seed = [string]$save.seed
        act = [int]$save.act_num
        floor = [int]$save.floor_num
        current_hp = [int]$save.current_health
        max_hp = [int]$save.max_health
        gold = [int]$save.gold
        room = [string]$save.current_room
        room_x = [int]$save.room_x
        room_y = [int]$save.room_y
        post_combat = [bool]$save.post_combat
    }
}

function Export-JavaPlayNativeCombatCheckpoint {
    param(
        [Parameter(Mandatory)] [object] $Launch,
        [Parameter(Mandatory)] [object] $Execution,
        [Parameter(Mandatory)] [object] $Plan,
        [Parameter(Mandatory)] [string] $CheckpointDirectory
    )
    if ($Plan.scope.expected_successor_site -ne 'combat' -or
        -not $Execution.passed -or
        $Execution.completed_plan_steps -ne $Execution.planned_steps) {
        throw 'Native checkpoint export currently requires one completed full-prefix plan ending at combat.'
    }
    $statusJson = & $liveBinary status --output-dir $liveOutput --json 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $statusJson) {
        throw 'Could not read the live Java combat root before checkpoint capture.'
    }
    $view = ($statusJson -join "`n") | ConvertFrom-Json -Depth 100
    $boundaryProperty = $Plan.scope.PSObject.Properties['expected_successor_combat_boundary']
    $expectedBoundary = if ($null -eq $boundaryProperty -or
        [string]::IsNullOrWhiteSpace([string]$boundaryProperty.Value)) {
        'player_decision'
    } else {
        [string]$boundaryProperty.Value
    }
    $boundaryMatches = switch ($expectedBoundary) {
        'player_decision' {
            [string]$view.frame.boundary.kind -eq 'combat_player_decision'
        }
        'toolbox_card_reward' {
            $reward = $view.frame.public_base.decision.card_reward
            $rewardCards = @($reward.cards)
            $chooseActions = @($view.actions | Where-Object { [string]$_.kind -eq 'choose_card_reward' })
            $unrelatedActions = @($view.actions | Where-Object { [string]$_.kind -notin @('choose_card_reward','discard_potion') })
            $choiceIndices = @($chooseActions | ForEach-Object { [int]$_.reward_index } | Sort-Object -Unique)
            [string]$view.frame.boundary.kind -eq 'run_decision' -and
                [string]$view.frame.boundary.evidence.room_phase -eq 'COMBAT' -and
                [string]$view.frame.public_base.decision.kind -eq 'card_reward' -and
                [string]$reward.source_kind -eq 'combat_discovery' -and
                $reward.skip_available -eq $false -and
                $rewardCards.Count -eq 3 -and
                @($view.frame.public_base.combat.hand).Count -eq 0 -and
                @($view.frame.parity_internal.relics | Where-Object { [string]$_.relic_id -eq 'Toolbox' }).Count -eq 1 -and
                $chooseActions.Count -eq 3 -and
                $unrelatedActions.Count -eq 0 -and
                ($choiceIndices -join ',') -eq '0,1,2'
        }
        default {
            throw "Native checkpoint export does not recognize combat boundary: $expectedBoundary"
        }
    }
    if ([string]$view.manifest.session_id -ne [string]$Execution.session.session_id -or
        -not $boundaryMatches) {
        throw 'Live Java checkpoint root is not the completed mirror combat successor.'
    }
    [IO.Directory]::CreateDirectory($CheckpointDirectory) | Out-Null
    $storageRoot = Join-Path $CheckpointDirectory 'storage'
    [IO.Directory]::CreateDirectory($storageRoot) | Out-Null
    foreach ($directory in @('preferences','betaPreferences','saves')) {
        $source = Join-Path $Launch.Storage.WorkingDirectory $directory
        if (-not (Test-Path -LiteralPath $source -PathType Container)) {
            throw "Checkpoint source omitted native storage directory: $source"
        }
        Copy-Item -LiteralPath $source -Destination $storageRoot -Recurse
    }
    $autosaves = @(Get-ChildItem -LiteralPath (Join-Path $storageRoot 'saves') -File |
        Where-Object { $_.Name -match '(^|_)IRONCLAD\.autosave$' })
    if ($autosaves.Count -ne 1) {
        throw "Expected one Ironclad native autosave at the combat root; found $($autosaves.Count)."
    }
    $rootFrame = Join-Path $CheckpointDirectory 'root-frame.json'
    [IO.File]::Copy([IO.Path]::GetFullPath([string]$view.frame_path), $rootFrame, $false)
    return [ordered]@{
        storage_root = [IO.Path]::GetFullPath($storageRoot)
        storage_inventory = Get-JavaPlayStorageInventory $storageRoot
        autosave = Get-JavaPlayFileIdentity $autosaves[0].FullName
        autosave_summary = Get-JavaPlayAutosaveSummary $autosaves[0].FullName
        root_frame = Get-JavaPlayFileIdentity $rootFrame
        root_decision = [ordered]@{
            kind = 'combat'
            boundary = $expectedBoundary
            act = [int]$view.frame.public_base.run.act
            floor = [int]$view.frame.public_base.run.floor
            current_hp = [int]$view.frame.public_base.combat.player.current_hp
            max_hp = [int]$view.frame.public_base.combat.player.max_hp
            monsters = @($view.frame.public_base.combat.monsters | ForEach-Object { [string]$_.id })
        }
    }
}

function Read-JavaPlayNativeCheckpoint([string] $ManifestPath) {
    $fullManifest = [IO.Path]::GetFullPath($ManifestPath)
    $manifest = Get-Content -LiteralPath $fullManifest -Raw | ConvertFrom-Json -Depth 100
    if ($manifest.schema -ne 'sts-java-parity-native-combat-checkpoint-v1' -or
        $manifest.provenance -notin @('natural_full_prefix_native_autosave','natural_checkpoint_suffix_native_autosave') -or
        $manifest.native_scope -ne 'enter_room_combat' -or
        $manifest.storage_edited) {
        throw "Unsupported or constructed Java checkpoint: $fullManifest"
    }
    $checkpointDirectory = [IO.Directory]::GetParent($fullManifest).FullName
    $storageRoot = [IO.Path]::GetFullPath((Join-Path $checkpointDirectory 'storage'))
    if ([IO.Path]::GetFullPath([string]$manifest.storage.root) -ne $storageRoot) {
        throw 'Checkpoint storage root does not match its manifest directory.'
    }
    $inventory = Get-JavaPlayStorageInventory $storageRoot
    if ($inventory.fingerprint_sha256 -ne [string]$manifest.storage.fingerprint_sha256) {
        throw 'Checkpoint storage inventory changed after capture.'
    }
    $boundFiles = @($manifest.source_plan,$manifest.source_execution,$manifest.source_lifecycle,$manifest.root_frame,$manifest.autosave)
    if ($null -ne $manifest.PSObject.Properties['parent_checkpoint'] -and
        $null -ne $manifest.parent_checkpoint) {
        $boundFiles += $manifest.parent_checkpoint
    }
    foreach ($identity in $boundFiles) {
        $actual = Get-JavaPlayFileIdentity ([string]$identity.path)
        if ($actual.length -ne [long]$identity.length -or $actual.sha256 -ne [string]$identity.sha256) {
            throw "Checkpoint-bound file changed: $($identity.path)"
        }
    }
    if ($manifest.provenance -eq 'natural_checkpoint_suffix_native_autosave') {
        $sourcePlan = Get-Content -LiteralPath ([string]$manifest.source_plan.path) -Raw | ConvertFrom-Json -Depth 100
        $lastSourceStep = if (@($sourcePlan.steps).Count -eq 0) { $null } else { @($sourcePlan.steps)[-1] }
        if ([string]$lastSourceStep.action_kind -ne 'choose_route_node') {
            throw 'Chained native checkpoint was not captured directly after a typed map route entered a new combat room.'
        }
    }
    return [pscustomobject]@{
        ManifestPath = $fullManifest
        ManifestIdentity = Get-JavaPlayFileIdentity $fullManifest
        Manifest = $manifest
        StorageRoot = $storageRoot
    }
}

function Remove-JavaPlayIsolatedStorage([object] $Storage) {
    if ($null -eq $Storage -or $Storage.Class -ne 'isolated_working_directory_v1') {
        throw 'Only typed isolated Java storage may be removed.'
    }
    $root = [IO.Path]::GetFullPath([string]$Storage.WorkingDirectory)
    $parent = [IO.Path]::GetFullPath($isolatedStorageParent)
    $parsedIsolationId = [Guid]::Empty
    if ([IO.Directory]::GetParent($root).FullName -ne $parent -or
        [IO.Path]::GetFileName($root) -ne [string]$Storage.IsolationId -or
        -not [Guid]::TryParseExact([string]$Storage.IsolationId, 'D', [ref]$parsedIsolationId)) {
        throw "Refusing to remove unverified Java storage path: $root"
    }
    if (Test-Path -LiteralPath $root -PathType Container) {
        foreach ($linkSpec in @(
            @{ Name = 'mods'; Target = (Join-Path $gameDirectory 'mods') },
            @{ Name = 'jre'; Target = (Join-Path $gameDirectory 'jre') }
        )) {
            $linkPath = Join-Path $root $linkSpec.Name
            if (-not (Test-Path -LiteralPath $linkPath)) {
                continue
            }
            $link = Get-Item -LiteralPath $linkPath -Force
            $target = [IO.Path]::GetFullPath([string]@($link.Target)[0])
            if ($link.LinkType -ne 'Junction' -or $target -ne [IO.Path]::GetFullPath($linkSpec.Target)) {
                throw "Refusing to remove isolated storage with unexpected link: $linkPath"
            }
            Remove-Item -LiteralPath $linkPath -Force
        }
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}

function Get-SuperFastModeSettings {
    $file = Get-JavaPlayFileIdentity $superFastModeConfig
    $properties = @{}
    foreach ($line in [IO.File]::ReadAllLines($file.path)) {
        $trimmed = $line.Trim()
        if ($trimmed.Length -eq 0 -or $trimmed.StartsWith('#') -or $trimmed.StartsWith('!')) {
            continue
        }
        $separator = $trimmed.IndexOf('=')
        if ($separator -lt 1) {
            continue
        }
        $properties[$trimmed.Substring(0, $separator).Trim()] = $trimmed.Substring($separator + 1).Trim()
    }
    foreach ($required in @('isDeltaMultiplied','deltaMultiplier','isInstantLerp')) {
        if (-not $properties.ContainsKey($required)) {
            throw "SuperFastMode config is missing ${required}: $($file.path)"
        }
    }
    $deltaEnabled = $properties['isDeltaMultiplied'].ToLowerInvariant()
    $instantLerp = $properties['isInstantLerp'].ToLowerInvariant()
    if ($deltaEnabled -notin @('true','false') -or $instantLerp -notin @('true','false')) {
        throw "SuperFastMode boolean settings are invalid: $($file.path)"
    }
    $multiplier = 0.0
    if (-not [double]::TryParse(
        $properties['deltaMultiplier'],
        [Globalization.NumberStyles]::Float,
        [Globalization.CultureInfo]::InvariantCulture,
        [ref] $multiplier
    )) {
        throw "SuperFastMode deltaMultiplier is invalid: $($properties['deltaMultiplier'])"
    }
    if ($deltaEnabled -ne 'true' -or $multiplier -le 1.0) {
        throw 'The accelerated profile requires isDeltaMultiplied=true and deltaMultiplier greater than 1.'
    }
    return [ordered]@{
        config = $file
        values = [ordered]@{
            is_delta_multiplied = ($deltaEnabled -eq 'true')
            delta_multiplier = $multiplier
            is_instant_lerp = ($instantLerp -eq 'true')
        }
    }
}

function Resolve-JavaPlayLaunchOptions([string[]] $CommandArguments) {
    if (($CommandArguments.Count % 2) -ne 0) {
        throw 'Launch options must be --profile strict|accelerated and/or --control combat|run|scenario.'
    }
    $profile = 'strict'
    $control = 'parity_combat_control'
    $seen = @{}
    for ($index = 0; $index -lt $CommandArguments.Count; $index += 2) {
        $name = $CommandArguments[$index]
        $value = $CommandArguments[$index + 1].ToLowerInvariant()
        if ($seen.ContainsKey($name)) {
            throw "Duplicate Java launch option: $name"
        }
        $seen[$name] = $true
        switch ($name) {
            '--profile' {
                if ($value -notin @('strict','accelerated')) {
                    throw "Unknown Java launch profile: $value"
                }
                $profile = $value
            }
            '--control' {
                if ($value -notin @('combat','run','scenario')) {
                    throw "Unknown Java control profile: $value"
                }
                $control = switch ($value) {
                    'run' { 'parity_run_control' }
                    'scenario' { 'parity_scenario_control' }
                    default { 'parity_combat_control' }
                }
            }
            default {
                throw "Unknown Java launch option: $name"
            }
        }
    }
    return [pscustomobject]@{
        Profile = $profile
        ControlProfile = $control
    }
}

function New-JavaPlayLaunchPlan([string] $Profile, [string] $ControlProfile, [object] $Storage) {
    $selectedModIds = [Collections.Generic.List[string]]::new()
    $selectedMods = [Collections.Generic.List[object]]::new()
    foreach ($entry in @(
        @{ Id = 'basemod'; Path = (Resolve-JavaPlayModJar 'BaseMod.jar') },
        @{ Id = 'CommunicationMod'; Path = $communicationModJar }
    )) {
        $identity = Get-JavaPlayModIdentity $entry.Path
        if ($identity.id -ne $entry.Id) {
            throw "Expected Mod id $($entry.Id), found $($identity.id) in $($entry.Path)"
        }
        $selectedModIds.Add($entry.Id)
        $selectedMods.Add($identity)
    }

    $acceleration = $null
    if ($Profile -eq 'accelerated') {
        $identity = Get-JavaPlayModIdentity $superFastModeJar
        if ($identity.id -ne 'superfastmode') {
            throw "Expected Mod id superfastmode, found $($identity.id) in $superFastModeJar"
        }
        $settings = Get-SuperFastModeSettings
        $selectedModIds.Insert(1, 'superfastmode')
        $selectedMods.Insert(1, $identity)
        $acceleration = [ordered]@{
            mod = $identity
            settings = $settings
        }
    }

    [IO.Directory]::CreateDirectory($launchOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $nonce = [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $path = Join-Path $launchOutput "launch-plan-$stamp-$nonce.json"
    $plan = [ordered]@{
        schema = 'sts_java_play_launch_plan_v3'
        created_at = [DateTimeOffset]::UtcNow.ToString('o')
        profile = $Profile
        control_profile = $ControlProfile
        selected_mod_ids = @($selectedModIds)
        selected_mods = @($selectedMods)
        game_java = Get-JavaPlayFileIdentity $gameJava
        mod_the_spire = Get-JavaPlayFileIdentity $modTheSpireJar
        acceleration = $acceleration
        storage = [ordered]@{
            class = [string]$Storage.Class
            profile_substrate = if ([string]$Storage.Class -eq 'installed_game') {
                'installed_profile_v1'
            } else {
                'copied_source_profile_v1'
            }
            tutorial_policy = 'observe_and_acknowledge_v1'
            working_directory = [IO.Path]::GetFullPath([string]$Storage.WorkingDirectory)
            source_game_directory = [IO.Path]::GetFullPath([string]$Storage.SourceGameDirectory)
            isolation_id = if ($null -eq $Storage.IsolationId) { $null } else { [string]$Storage.IsolationId }
        }
    }
    $json = $plan | ConvertTo-Json -Depth 12
    $stream = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    $writer = [IO.StreamWriter]::new($stream, [Text.UTF8Encoding]::new($false))
    try {
        $writer.WriteLine($json)
    } finally {
        $writer.Dispose()
        $stream.Dispose()
    }
    return [pscustomobject]@{
        Path = [IO.Path]::GetFullPath($path)
        SelectedModIds = @($selectedModIds)
        Acceleration = $acceleration
    }
}

function Set-CommunicationLiveConfig {
    if (-not (Test-Path -LiteralPath $communicationConfig -PathType Leaf)) {
        throw "CommunicationMod config does not exist: $communicationConfig"
    }
    $backupDirectory = Join-Path $communicationRepository '.oracle-lab\config-backups'
    [IO.Directory]::CreateDirectory($backupDirectory) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $backup = Join-Path $backupDirectory "config-before-live-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8)).properties"
    [IO.File]::Copy($communicationConfig, $backup, $false)

    $command = ($liveBinary -replace '\\', '/') + ' serve --output-dir ' + ($liveOutput -replace '\\', '/')
    $lines = @([IO.File]::ReadAllLines($communicationConfig))
    $updated = [Collections.Generic.List[string]]::new()
    $sawCommand = $false
    $sawRunAtStart = $false
    foreach ($line in $lines) {
        if ($line -match '^command=') {
            $updated.Add("command=$command")
            $sawCommand = $true
        } elseif ($line -match '^runAtGameStart=') {
            $updated.Add('runAtGameStart=true')
            $sawRunAtStart = $true
        } else {
            $updated.Add($line)
        }
    }
    if (-not $sawCommand) {
        $updated.Add("command=$command")
    }
    if (-not $sawRunAtStart) {
        $updated.Add('runAtGameStart=true')
    }
    [IO.File]::WriteAllLines($communicationConfig, $updated, [Text.UTF8Encoding]::new($false))
    Write-Output "CommunicationMod live config selected; backup=$backup"
}

function Invoke-JavaPlayInstall {
    Assert-GameClosed
    Invoke-JavaPlayBuild
    Push-Location $communicationRepository
    try {
        & $communicationWrapper build
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
        & $communicationWrapper deploy -ConfirmDeploy
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    } finally {
        Pop-Location
    }
    Set-CommunicationLiveConfig
}

function Start-JavaPlayLaunch([string[]] $CommandArguments, [switch] $Hidden, [object] $Storage) {
    $options = Resolve-JavaPlayLaunchOptions $CommandArguments
    $profile = $options.Profile
    if ($null -eq $Storage) {
        $Storage = [pscustomobject]@{
            Class = 'installed_game'
            WorkingDirectory = [IO.Path]::GetFullPath($gameDirectory)
            SourceGameDirectory = [IO.Path]::GetFullPath($gameDirectory)
            IsolationId = $null
        }
    }
    if ($options.ControlProfile -eq 'parity_scenario_control' -and
        $Storage.Class -ne 'isolated_working_directory_v1') {
        throw 'Scenario control requires managed isolated storage. Use .\java-play.ps1 scenario create-combat instead of interactive launch.'
    }
    Assert-LiveBridgeFresh
    Assert-GameClosed
    $plan = New-JavaPlayLaunchPlan $profile $options.ControlProfile $Storage
    $start = @{
        FilePath = $gameJava
        WorkingDirectory = $Storage.WorkingDirectory
        ArgumentList = @(
            '-jar',
            $modTheSpireJar,
            '--skip-launcher',
            '--skip-intro',
            '--mods',
            ($plan.SelectedModIds -join ',')
        )
        Environment = @{
        STS_JAVA_PLAY_LAUNCH_PROFILE = $profile
        STS_JAVA_PLAY_LAUNCH_PLAN = $plan.Path
        STS_JAVA_PLAY_CONTROL_PROFILE = $options.ControlProfile
        }
        PassThru = $true
    }
    if ($Hidden) {
        $start.WindowStyle = 'Hidden'
    }
    $bridgePidsBefore = @(Get-Process -Name 'sts_java_parity_live' -ErrorAction SilentlyContinue | ForEach-Object Id)
    $process = Start-Process @start
    return [pscustomobject]@{
        Process = $process
        Plan = $plan
        Profile = $profile
        ControlProfile = $options.ControlProfile
        BridgePidsBefore = $bridgePidsBefore
        Storage = $Storage
    }
}

function Invoke-JavaPlayLaunch([string[]] $CommandArguments) {
    $launch = Start-JavaPlayLaunch $CommandArguments
    $process = $launch.Process
    $plan = $launch.Plan
    $profile = $launch.Profile
    Write-Output "Launched ModTheSpire profile=$profile control=$($launch.ControlProfile) process_id=$($process.Id) launch_plan=$($plan.Path)"
    if ($profile -eq 'accelerated') {
        Write-Output "SuperFastMode delta_multiplier=$($plan.Acceleration.settings.values.delta_multiplier) instant_lerp=$($plan.Acceleration.settings.values.is_instant_lerp)"
    }
    Write-Output 'Run .\java-play.ps1 status at the next typed stable decision.'
}

function Wait-JavaPlayProbeSession([object] $Launch, [int] $TimeoutSeconds = 30) {
    $launchPlan = [string]$Launch.Plan.Path
    $bridgeSeen = $false
    $errorLog = Join-Path ([string]$Launch.Storage.WorkingDirectory) 'communication_mod_errors.log'
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        $statusJson = & $liveBinary status --output-dir $liveOutput --json 2>$null
        if ($LASTEXITCODE -eq 0 -and $statusJson) {
            try {
                $view = ($statusJson -join "`n") | ConvertFrom-Json -Depth 100
                if ([IO.Path]::GetFullPath($view.manifest.launch.plan_path) -eq [IO.Path]::GetFullPath($LaunchPlan)) {
                    return $view
                }
            } catch {
                # The producer may still be publishing its first create-new frame.
            }
        }
        $newBridges = @(Get-Process -Name 'sts_java_parity_live' -ErrorAction SilentlyContinue | Where-Object {
            $_.Id -notin $Launch.BridgePidsBefore
        })
        if ($newBridges.Count -gt 0) {
            $bridgeSeen = $true
        }
        if (Test-Path -LiteralPath $errorLog -PathType Leaf) {
            $errorValue = Get-Content -LiteralPath $errorLog -Raw -ErrorAction SilentlyContinue
            $errorText = if ($null -eq $errorValue) { '' } else { ([string]$errorValue).Trim() }
            if (-not [string]::IsNullOrWhiteSpace($errorText)) {
                throw "Java live bridge rejected startup for launch plan ${launchPlan}: $errorText"
            }
        }
        $gameProcessId = [int]$Launch.Process.Id
        if ($gameProcessId -le 0) {
            throw "Java launch omitted its process identity for launch plan $launchPlan"
        }
        if ($null -eq (Get-Process -Id $gameProcessId -ErrorAction SilentlyContinue)) {
            throw "Java process $gameProcessId exited before publishing a session for launch plan $launchPlan"
        }
        if ($bridgeSeen -and $newBridges.Count -eq 0) {
            throw "Java live bridge exited before publishing a session for launch plan $launchPlan"
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Timed out waiting for the fresh Java probe session bound to launch plan $LaunchPlan"
}

function Stop-JavaPlayProbeLaunch([object] $Launch, [object] $BridgeProcessId) {
    $trackedIds = [Collections.Generic.HashSet[int]]::new()
    [void]$trackedIds.Add([int]$Launch.Process.Id)
    if ($null -ne $BridgeProcessId) {
        [void]$trackedIds.Add([int]$BridgeProcessId)
    }
    $newBridges = @(Get-Process -Name 'sts_java_parity_live' -ErrorAction SilentlyContinue | Where-Object {
        $_.Id -notin $Launch.BridgePidsBefore
    })
    foreach ($bridge in $newBridges) {
        [void]$trackedIds.Add([int]$bridge.Id)
    }
    $gameProcess = Get-Process -Id $Launch.Process.Id -ErrorAction SilentlyContinue
    if ($null -ne $gameProcess) {
        Stop-Process -Id $gameProcess.Id -Force
    }
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds(10)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        $remaining = @($trackedIds | Where-Object {
            $null -ne (Get-Process -Id $_ -ErrorAction SilentlyContinue)
        })
        if ($remaining.Count -eq 0) {
            break
        }
        Start-Sleep -Milliseconds 100
    }
    $remaining = @($trackedIds | Where-Object {
        $null -ne (Get-Process -Id $_ -ErrorAction SilentlyContinue)
    })
    foreach ($processId in $remaining) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
    $forceDeadline = [DateTimeOffset]::UtcNow.AddSeconds(5)
    while ([DateTimeOffset]::UtcNow -lt $forceDeadline) {
        $remaining = @($trackedIds | Where-Object {
            $null -ne (Get-Process -Id $_ -ErrorAction SilentlyContinue)
        })
        if ($remaining.Count -eq 0) {
            return
        }
        Start-Sleep -Milliseconds 100
    }
    if ($remaining.Count -ne 0) {
        throw "Managed Java processes did not exit: $($remaining -join ',')"
    }
}

function Get-JavaPlayProcessIdentity([Diagnostics.Process] $Process) {
    $Process.Refresh()
    $path = [string]$Process.Path
    if ([string]::IsNullOrWhiteSpace($path)) {
        throw "Process $($Process.Id) omitted its executable path."
    }
    return [ordered]@{
        process_id = [int]$Process.Id
        executable = Get-JavaPlayFileIdentity $path
        start_time_utc_ticks = [long]$Process.StartTime.ToUniversalTime().Ticks
    }
}

function Get-JavaPlayProcessByIdentity([object] $Identity, [string] $ExpectedExecutable, [string] $Role) {
    if ($null -eq $Identity -or [int]$Identity.process_id -le 0 -or [long]$Identity.start_time_utc_ticks -le 0) {
        throw "Paired-root resident omitted its $Role process identity."
    }
    $process = Get-Process -Id ([int]$Identity.process_id) -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        throw "Paired-root resident $Role process is no longer running: $($Identity.process_id)"
    }
    $actual = Get-JavaPlayProcessIdentity $process
    $expectedPath = [IO.Path]::GetFullPath($ExpectedExecutable)
    if ([long]$actual.start_time_utc_ticks -ne [long]$Identity.start_time_utc_ticks -or
        -not [string]::Equals([string]$actual.executable.path, $expectedPath, [StringComparison]::OrdinalIgnoreCase) -or
        [string]$actual.executable.sha256 -ne [string]$Identity.executable.sha256) {
        throw "Paired-root resident $Role PID was reused or its executable identity changed."
    }
    return $process
}

function Resolve-JavaPlayPairedRootResidentDirectory([string] $ResidentId) {
    if ($ResidentId -notmatch '^paired-root-resident-v1-[0-9a-f]{32}$') {
        throw "Invalid paired-root resident ID: $ResidentId"
    }
    $parent = [IO.Path]::GetFullPath($pairedRootResidentOutput)
    $directory = [IO.Path]::GetFullPath((Join-Path $parent $ResidentId))
    if ([IO.Directory]::GetParent($directory).FullName -ne $parent -or
        [IO.Path]::GetFileName($directory) -ne $ResidentId) {
        throw "Paired-root resident path escaped its managed root: $ResidentId"
    }
    return $directory
}

function Read-JavaPlayPairedRootResident(
    [string] $ResidentId,
    [switch] $AllowNonNeutral
) {
    $directory = Resolve-JavaPlayPairedRootResidentDirectory $ResidentId
    $manifestPath = Join-Path $directory 'resident.json'
    $closePath = Join-Path $directory 'close.json'
    $retiredPath = Join-Path $directory 'retired.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "Paired-root resident manifest does not exist: $manifestPath"
    }
    if (Test-Path -LiteralPath $closePath -PathType Leaf) {
        throw "Paired-root resident is already closed: $ResidentId"
    }
    if (Test-Path -LiteralPath $retiredPath -PathType Leaf) {
        throw "Paired-root resident is retired after a non-clean boundary: $ResidentId"
    }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -Depth 100
    if ($manifest.schema -ne 'sts-java-play-paired-root-resident-v1' -or
        [string]$manifest.resident_id -ne $ResidentId -or
        [string]::IsNullOrWhiteSpace([string]$manifest.native_root_id) -or
        [string]::IsNullOrWhiteSpace([string]$manifest.java_bundle_id) -or
        [string]::IsNullOrWhiteSpace([string]$manifest.content_profile) -or
        [string]::IsNullOrWhiteSpace([string]$manifest.session.session_id) -or
        [string]::IsNullOrWhiteSpace([string]$manifest.session.connection_id)) {
        throw "Paired-root resident manifest is invalid: $manifestPath"
    }
    $directoryFull = [IO.Path]::GetFullPath($directory)
    if ([IO.Path]::GetFullPath([string]$manifest.resident_directory) -ne $directoryFull) {
        throw 'Paired-root resident manifest disagrees with its managed directory.'
    }
    $storage = [pscustomobject]@{
        Class = [string]$manifest.storage.class
        WorkingDirectory = [IO.Path]::GetFullPath([string]$manifest.storage.working_directory)
        SourceGameDirectory = [IO.Path]::GetFullPath([string]$manifest.storage.source_game_directory)
        IsolationId = [string]$manifest.storage.isolation_id
    }
    if ($storage.Class -ne 'isolated_working_directory_v1' -or
        $storage.SourceGameDirectory -ne [IO.Path]::GetFullPath($gameDirectory)) {
        throw 'Paired-root resident does not own typed isolated storage.'
    }
    $gameProcess = Get-JavaPlayProcessByIdentity $manifest.game_process $gameJava 'game'
    $bridgeProcess = Get-JavaPlayProcessByIdentity $manifest.bridge_process $liveBinary 'bridge'
    $currentLiveBinary = Get-JavaPlayFileIdentity $liveBinary
    if ([string]$currentLiveBinary.sha256 -ne [string]$manifest.live_binary.sha256) {
        throw 'Paired-root resident bridge build differs from the current fresh bridge.'
    }
    $statusRaw = & $liveBinary status --output-dir $liveOutput --json 2>&1
    if ($LASTEXITCODE -ne 0 -or -not $statusRaw) {
        throw "Paired-root resident live status failed: $($statusRaw -join "`n")"
    }
    $view = ($statusRaw -join "`n") | ConvertFrom-Json -Depth 100
    $actions = @($view.actions)
    if ([string]$view.manifest.session_id -ne [string]$manifest.session.session_id -or
        [string]$view.manifest.connection_id -ne [string]$manifest.session.connection_id -or
        [int]$view.manifest.process_id -ne [int]$manifest.bridge_process.process_id -or
        [IO.Path]::GetFullPath([string]$view.manifest.launch.plan_path) -ne [IO.Path]::GetFullPath([string]$manifest.launch_plan.path) -or
        [IO.Path]::GetFullPath([string]$view.manifest.launch.working_directory) -ne $storage.WorkingDirectory) {
        throw 'Paired-root resident live runtime identity differs from its manifest.'
    }
    $neutral = [string]$view.frame.boundary.kind -eq 'main_menu_ready' -and
        [string]$view.frame.public_base.decision.kind -eq 'main_menu_ready' -and
        $actions.Count -eq 1 -and
        [string]$actions[0].kind -eq 'continue_run' -and
        [string]$actions[0].action_id -eq 'continue_run'
    if (-not $neutral -and -not $AllowNonNeutral) {
        throw 'Paired-root resident is not the recorded neutral live runtime.'
    }
    $workingAutosave = [IO.Path]::GetFullPath((Join-Path $storage.WorkingDirectory ([string]$manifest.autosave.relative_path)))
    $storagePrefix = "$($storage.WorkingDirectory)$([IO.Path]::DirectorySeparatorChar)"
    if (-not $workingAutosave.StartsWith($storagePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Paired-root resident autosave escaped its isolated storage.'
    }
    $autosave = Get-JavaPlayFileIdentity $workingAutosave
    if ([long]$autosave.length -ne [long]$manifest.autosave.length -or
        [string]$autosave.sha256 -ne [string]$manifest.autosave.sha256) {
        throw 'Paired-root resident native autosave changed while neutral.'
    }
    return [pscustomobject]@{
        ResidentId = $ResidentId
        Directory = $directoryFull
        ManifestPath = [IO.Path]::GetFullPath($manifestPath)
        Manifest = $manifest
        Storage = $storage
        GameProcess = $gameProcess
        BridgeProcess = $bridgeProcess
        View = $view
        Autosave = $autosave
        Neutral = $neutral
    }
}

function Stop-JavaPlayPairedRootResidentProcesses([object] $Resident) {
    foreach ($process in @($Resident.GameProcess,$Resident.BridgeProcess)) {
        try {
            $process.Refresh()
            if (-not $process.HasExited) {
                $process.Kill()
            }
        } catch [InvalidOperationException] {
            # The exact process object already exited; never reacquire by PID.
        }
    }
    $remaining = @()
    foreach ($process in @($Resident.GameProcess,$Resident.BridgeProcess)) {
        try {
            if (-not $process.WaitForExit(10000)) {
                $remaining += [int]$process.Id
            }
        } catch [InvalidOperationException] {
            # The exact process is already gone.
        }
    }
    if ($remaining.Count -gt 0) {
        throw "Paired-root resident processes did not exit: $($remaining -join ',')"
    }
}

function Enter-JavaPlayPairedRootResidentLease([string] $ResidentId, [string] $Operation) {
    $directory = Resolve-JavaPlayPairedRootResidentDirectory $ResidentId
    $leasePath = Join-Path $directory 'lease.lock'
    $stream = $null
    try {
        $stream = [IO.File]::Open(
            $leasePath,
            [IO.FileMode]::OpenOrCreate,
            [IO.FileAccess]::ReadWrite,
            [IO.FileShare]::None
        )
    } catch [IO.IOException] {
        throw "Paired-root resident is busy in another invocation: $ResidentId"
    }
    try {
        $stream.SetLength(0)
        $leaseRecord = [ordered]@{
            schema = 'sts-java-play-paired-root-resident-lease-v1'
            resident_id = $ResidentId
            operation = $Operation
            owner_process_id = [int]$PID
            acquired_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        } | ConvertTo-Json -Depth 10 -Compress
        $writer = [IO.StreamWriter]::new($stream, [Text.UTF8Encoding]::new($false), 1024, $true)
        try {
            $writer.Write($leaseRecord)
            $writer.Flush()
            $stream.Flush($true)
        } finally {
            $writer.Dispose()
        }
        return [pscustomobject]@{
            Path = [IO.Path]::GetFullPath($leasePath)
            Stream = $stream
        }
    } catch {
        $stream.Dispose()
        throw
    }
}

function Exit-JavaPlayPairedRootResidentLease([object] $Lease) {
    if ($null -ne $Lease -and $null -ne $Lease.Stream) {
        $Lease.Stream.Dispose()
    }
}

function Find-JavaPlayCompatiblePairedRootResident([string] $StorePath, [object] $Materialized) {
    if (-not (Test-Path -LiteralPath $pairedRootResidentOutput -PathType Container)) {
        return $null
    }
    $busy = [Collections.Generic.List[string]]::new()
    $invalid = [Collections.Generic.List[string]]::new()
    $candidates = @(Get-ChildItem -LiteralPath $pairedRootResidentOutput -Directory |
        Where-Object { $_.Name -match '^paired-root-resident-v1-[0-9a-f]{32}$' } |
        Sort-Object Name -Descending)
    foreach ($candidate in $candidates) {
        $manifestPath = Join-Path $candidate.FullName 'resident.json'
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf) -or
            (Test-Path -LiteralPath (Join-Path $candidate.FullName 'close.json') -PathType Leaf) -or
            (Test-Path -LiteralPath (Join-Path $candidate.FullName 'retired.json') -PathType Leaf)) {
            continue
        }
        try {
            $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -Depth 100
            $compatible = $manifest.schema -eq 'sts-java-play-paired-root-resident-v1' -and
                [string]$manifest.resident_id -eq $candidate.Name -and
                [IO.Path]::GetFullPath([string]$manifest.store_root) -eq [IO.Path]::GetFullPath($StorePath) -and
                [string]$manifest.native_root_id -eq [string]$Materialized.native_root_id -and
                [string]$manifest.java_bundle_id -eq [string]$Materialized.java_bundle_id -and
                [string]$manifest.content_profile -eq [string]$Materialized.content_profile
        } catch {
            continue
        }
        if (-not $compatible) {
            continue
        }
        try {
            $lease = Enter-JavaPlayPairedRootResidentLease $candidate.Name 'routine_preview'
        } catch {
            $busy.Add($candidate.Name)
            continue
        }
        try {
            $resident = Read-JavaPlayPairedRootResident $candidate.Name
            return [pscustomobject]@{
                Resident = $resident
                Lease = $lease
                Selection = 'reused'
            }
        } catch {
            Exit-JavaPlayPairedRootResidentLease $lease
            $invalid.Add("$($candidate.Name): $($_.Exception.Message)")
        }
    }
    if ($busy.Count -gt 0) {
        throw "A compatible paired-root resident is busy: $($busy -join ',')"
    }
    if ($invalid.Count -gt 0) {
        throw "Compatible paired-root resident recovery failed; inspect before opening another runtime: $($invalid -join '; ')"
    }
    return $null
}

function Retire-JavaPlayPairedRootResident([object] $Resident, [string] $Reason, [string] $Detail, [string] $AttemptReport) {
    $shutdownFailure = $null
    try {
        Stop-JavaPlayPairedRootResidentProcesses $Resident
    } catch {
        $shutdownFailure = $_.Exception.Message
    }
    $retiredPath = Join-Path $Resident.Directory 'retired.json'
    if (-not (Test-Path -LiteralPath $retiredPath -PathType Leaf)) {
        Write-JavaPlayJsonCreateNew $retiredPath ([ordered]@{
            schema = 'sts-java-play-paired-root-resident-retired-v1'
            resident_id = $Resident.ResidentId
            created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
            reason = $Reason
            detail = $Detail
            attempt_report = if ([string]::IsNullOrWhiteSpace($AttemptReport)) { $null } else { [IO.Path]::GetFullPath($AttemptReport) }
            shutdown_failure = $shutdownFailure
            isolated_storage_retained = $true
            isolated_storage = $Resident.Storage.WorkingDirectory
        })
    }
    return [IO.Path]::GetFullPath($retiredPath)
}

function Invoke-JavaPlayPairedRootResident(
    [string[]] $CommandArguments,
    [object] $AcquiredLease = $null,
    [switch] $RetainLease,
    [switch] $PassThru
) {
    if ($CommandArguments.Count -lt 2 -or ($CommandArguments.Count % 2) -ne 0) {
        throw 'Usage: paired-root resident --mode open --target-root <root-id> [--store <store>]; --mode preview --resident <resident-id> --target-root <root-id> --action <action-id>; or --mode status|close --resident <resident-id>.'
    }
    $options = @{}
    for ($index = 0; $index -lt $CommandArguments.Count; $index += 2) {
        $name = $CommandArguments[$index]
        $value = $CommandArguments[$index + 1]
        if ($name -notin @('--mode','--target-root','--store','--resident','--action') -or
            [string]::IsNullOrWhiteSpace($value) -or $options.ContainsKey($name)) {
            throw "Invalid or duplicate paired-root resident option: $name"
        }
        $options[$name] = $value
    }
    if (-not $options.ContainsKey('--mode')) {
        throw 'Paired-root resident requires --mode open|preview|status|close.'
    }
    $mode = ([string]$options['--mode']).ToLowerInvariant()
    if ($mode -notin @('open','preview','status','close')) {
        throw "Unknown paired-root resident mode: $mode"
    }
    if ($mode -ne 'close') {
        Assert-LiveBridgeFresh
    }

    if ($mode -eq 'preview') {
        if (-not $options.ContainsKey('--resident') -or
            -not $options.ContainsKey('--target-root') -or
            -not $options.ContainsKey('--action') -or
            @($options.Keys | Where-Object { $_ -notin @('--mode','--resident','--target-root','--action') }).Count -ne 0) {
            throw 'Paired-root resident preview requires --resident, --target-root and --action.'
        }
        if (-not (Test-Path -LiteralPath $runPlayWorkbench -PathType Leaf)) {
            throw 'Run-play workbench is missing. Run .\run-play.ps1 build first.'
        }
        $lease = $AcquiredLease
        if ($null -eq $lease) {
            $lease = Enter-JavaPlayPairedRootResidentLease ([string]$options['--resident']) 'preview'
        } else {
            $expectedLeasePath = [IO.Path]::GetFullPath((Join-Path (Resolve-JavaPlayPairedRootResidentDirectory ([string]$options['--resident'])) 'lease.lock'))
            if ([IO.Path]::GetFullPath([string]$lease.Path) -ne $expectedLeasePath) {
                throw 'Acquired paired-root resident lease belongs to a different resident.'
            }
        }
        try {
        $resident = Read-JavaPlayPairedRootResident ([string]$options['--resident'])
        $storePath = [IO.Path]::GetFullPath([string]$resident.Manifest.store_root)
        $targetRoot = [string]$options['--target-root']
        $actionId = [string]$options['--action']
        [IO.Directory]::CreateDirectory($pairedRootOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $runDirectory = Join-Path $pairedRootOutput "resident-preview-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
        $actionObservationReport = Join-Path $runDirectory 'actions.json'
        $nativePayload = Join-Path $runDirectory 'native-payload'
        $materializationReport = Join-Path $runDirectory 'native-materialization.json'
        $activationReport = Join-Path $runDirectory 'activation.json'
        $recoveryAttempt = Join-Path $runDirectory 'recovery-work'
        $recoveryReport = Join-Path $runDirectory 'recovery.json'
        $previewAttempt = Join-Path $runDirectory 'preview-work'
        $previewReport = Join-Path $runDirectory 'paired-preview.json'
        $resetReport = Join-Path $runDirectory 'reset.json'
        $attemptReport = Join-Path $runDirectory 'resident-preview.json'
        $failureReport = Join-Path $runDirectory 'failure.json'

        $observeRaw = & $runPlayWorkbench paired-root-observe-actions --root $targetRoot --store $storePath --report $actionObservationReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Paired-root resident action preflight failed: $($observeRaw -join "`n")"
        }
        $observed = Get-Content -LiteralPath $actionObservationReport -Raw | ConvertFrom-Json -Depth 100
        Assert-JavaPlayPairedRootActionObservation $observed $targetRoot @($actionId) -RequireVerified
        $materializeRaw = & $runPlayWorkbench paired-root-materialize-native --target-root $targetRoot --store $storePath --output $nativePayload --report $materializationReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Paired-root resident target preflight failed: $($materializeRaw -join "`n")"
        }
        $materialized = Get-Content -LiteralPath $materializationReport -Raw | ConvertFrom-Json -Depth 100
        if ($materialized.schema -ne 'sts-paired-root-native-materialization-report-v1' -or
            [string]$materialized.requested_target_root_id -ne $targetRoot -or
            [string]$materialized.native_root_id -ne [string]$resident.Manifest.native_root_id -or
            [string]$materialized.java_bundle_id -ne [string]$resident.Manifest.java_bundle_id -or
            [string]$materialized.content_profile -ne [string]$resident.Manifest.content_profile) {
            throw 'Paired-root target requires a different native resident payload.'
        }

        $clock = [Diagnostics.Stopwatch]::StartNew()
        $continueCompleted = $null
        $orchestrateCompleted = $null
        $resetCompleted = $null
        try {
            $continueRaw = & $liveBinary continue-run --output-dir $liveOutput --content-profile ([string]$resident.Manifest.content_profile) 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Paired-root resident Continue failed: $($continueRaw -join "`n")"
            }
            $continueCompleted = $clock.ElapsedMilliseconds
            $orchestrateRaw = & $runPlayWorkbench paired-root-resume-preview --target-root $targetRoot --action $actionId --run-root $runPlayRoot --java-live-output $liveOutput --store $storePath --recovery-attempt $recoveryAttempt --preview-attempt $previewAttempt --activation-report $activationReport --recovery-report $recoveryReport --preview-report $previewReport 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Paired-root resident activate/recover/preview failed: $($orchestrateRaw -join "`n")"
            }
            $orchestrateCompleted = $clock.ElapsedMilliseconds
            $activated = Get-Content -LiteralPath $activationReport -Raw | ConvertFrom-Json -Depth 100
            $recovered = Get-Content -LiteralPath $recoveryReport -Raw | ConvertFrom-Json -Depth 100
            $previewed = Get-Content -LiteralPath $previewReport -Raw | ConvertFrom-Json -Depth 100
            $versionCompatibility = @($activated.version_compatibility)
            if ($activated.schema -ne 'sts-paired-root-native-activation-report-v1' -or
                [string]$activated.requested_target_root_id -ne $targetRoot -or
                [string]$activated.native_root_id -ne [string]$resident.Manifest.native_root_id -or
                -not $activated.source_head_guard.unchanged -or
                $versionCompatibility.Count -ne 6 -or
                @($versionCompatibility | Where-Object { $_.status -notin @('exact','producer_changed_semantics_compatible') }).Count -ne 0 -or
                $recovered.schema -ne 'sts-paired-root-recovery-report-v1' -or
                $recovered.status -ne 'ready' -or
                -not $recovered.source_head_guard.unchanged -or
                [string]$recovered.target_root_id -ne $targetRoot -or
                $previewed.schema -ne 'sts-paired-root-preview-report-v1' -or
                $previewed.status -notin @('ready','diverged','successor_boundary_unsupported') -or
                -not $previewed.source_head_guard.unchanged -or
                [string]$previewed.source_root_id -ne $targetRoot -or
                [string]$previewed.action.handle_id -ne $actionId) {
                throw 'Paired-root resident preview artifacts failed their scoped validation.'
            }
            $successorRootId = if ($null -eq $previewed.PSObject.Properties['successor_root_id']) {
                $null
            } else {
                [string]$previewed.successor_root_id
            }
            $runtimeSettlement = if ([string]$previewed.status -eq 'ready') {
                Get-JavaPlayPairedRootRuntimeSettlement $storePath $successorRootId
            } else {
                $null
            }
            $statusRaw = & $liveBinary status --output-dir $liveOutput --json 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Paired-root resident successor status failed: $($statusRaw -join "`n")"
            }
            $afterPreviewView = ($statusRaw -join "`n") | ConvertFrom-Json -Depth 100
            if ([string]$afterPreviewView.manifest.session_id -ne [string]$resident.Manifest.session.session_id -or
                [string]$afterPreviewView.manifest.connection_id -ne [string]$resident.Manifest.session.connection_id) {
                throw 'Paired-root resident preview changed its live runtime identity.'
            }
            $autosavePath = [string]$resident.Autosave.path
            $afterPreviewAutosave = if (Test-Path -LiteralPath $autosavePath -PathType Leaf) {
                Get-JavaPlayFileIdentity $autosavePath
            } else {
                [pscustomobject]@{
                    path = $autosavePath
                    exists = $false
                    length = $null
                    sha256 = $null
                }
            }
            $autosaveUnchanged = $null -ne $afterPreviewAutosave.length -and
                [long]$afterPreviewAutosave.length -eq [long]$resident.Manifest.autosave.length -and
                [string]$afterPreviewAutosave.sha256 -eq [string]$resident.Manifest.autosave.sha256
            $autosaveMayDiffer = $null -ne $runtimeSettlement -and
                [string]$runtimeSettlement.native_autosave -eq 'may_differ_from_native_root'
            if (-not $autosaveUnchanged -and
                -not $autosaveMayDiffer -and
                [string]$previewed.status -ne 'successor_boundary_unsupported') {
                throw 'Paired-root resident preview changed the native autosave.'
            }

            $returnedToNeutral = $false
            $resetIdentity = $null
            $retireReason = $null
            if ($previewed.status -eq 'ready' -and
                [string]$afterPreviewView.frame.boundary.kind -eq 'combat_player_decision') {
                $resetRaw = & $liveBinary reset-natural-run --output-dir $liveOutput --report-path $resetReport 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "Paired-root resident reset failed: $($resetRaw -join "`n")"
                }
                $reset = ($resetRaw -join "`n") | ConvertFrom-Json -Depth 100
                if (-not $reset.passed -or
                    -not $reset.preserved_save_main_menu_verified -or
                    [string]$reset.session_id -ne [string]$resident.Manifest.session.session_id -or
                    [string]$reset.connection_id -ne [string]$resident.Manifest.session.connection_id) {
                    throw "Paired-root resident reset returned invalid evidence: $resetReport"
                }
                $resetCompleted = $clock.ElapsedMilliseconds
                $resetIdentity = Get-JavaPlayFileIdentity $resetReport
                $neutral = Read-JavaPlayPairedRootResident $resident.ResidentId
                $returnedToNeutral = $true
                $resident = $neutral
            } elseif ($previewed.status -eq 'diverged') {
                $retireReason = 'parity_divergence'
            } elseif ($previewed.status -eq 'successor_boundary_unsupported') {
                $boundaryKind = [string]$previewed.successor_boundary.kind
                $screenType = [string]$previewed.successor_boundary.screen_type
                $retireReason = "successor_boundary_unsupported:${boundaryKind}:${screenType}"
            } elseif ($null -ne $runtimeSettlement -and
                [string]$runtimeSettlement.resident_completion -eq 'retire') {
                $retireReason = 'typed_runtime_settlement:retire'
            } else {
                $retireReason = "reset_unsupported:$([string]$afterPreviewView.frame.boundary.kind)"
            }

            $result = [ordered]@{
                schema = 'sts-java-play-paired-root-resident-preview-v1'
                created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() - [long]$clock.ElapsedMilliseconds
                completed_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                status = [string]$previewed.status
                resident_id = $resident.ResidentId
                target_root_id = $targetRoot
                action_handle_id = $actionId
                resident_manifest = Get-JavaPlayFileIdentity $resident.ManifestPath
                action_observation = Get-JavaPlayFileIdentity $actionObservationReport
                native_materialization = Get-JavaPlayFileIdentity $materializationReport
                activation = Get-JavaPlayFileIdentity $activationReport
                recovery = Get-JavaPlayFileIdentity $recoveryReport
                preview = Get-JavaPlayFileIdentity $previewReport
                reset = $resetIdentity
                result = [ordered]@{
                    successor_root_id = $successorRootId
                    successor_activation_id = if ($null -eq $previewed.PSObject.Properties['successor_activation_id']) { $null } else { [string]$previewed.successor_activation_id }
                    receipt_id = if ($null -eq $previewed.PSObject.Properties['receipt_id']) { $null } else { [string]$previewed.receipt_id }
                    divergence_id = if ($null -eq $previewed.PSObject.Properties['divergence_id']) { $null } else { [string]$previewed.divergence_id }
                    successor_boundary = if ($null -eq $previewed.PSObject.Properties['successor_boundary']) { $null } else { $previewed.successor_boundary }
                    java_evidence = if ($null -eq $previewed.PSObject.Properties['java_evidence']) { $null } else { $previewed.java_evidence }
                    rust_evidence = if ($null -eq $previewed.PSObject.Properties['rust_evidence']) { $null } else { $previewed.rust_evidence }
                    runtime_settlement = $runtimeSettlement
                }
                autosave = [ordered]@{
                    unchanged = $autosaveUnchanged
                    before = $resident.Autosave
                    after_preview = $afterPreviewAutosave
                }
                source_head_guard = [ordered]@{
                    unchanged = [bool]$activated.source_head_guard.unchanged -and [bool]$recovered.source_head_guard.unchanged -and [bool]$previewed.source_head_guard.unchanged
                }
                resident_reusable = $returnedToNeutral
                retire_reason = $retireReason
                timings_ms = [ordered]@{
                    continue = [long]$continueCompleted
                    activate_recover_preview = [long]$orchestrateCompleted - [long]$continueCompleted
                    reset = if ($null -eq $resetCompleted) { $null } else { [long]$resetCompleted - [long]$orchestrateCompleted }
                    total = [long]$clock.ElapsedMilliseconds
                }
                evidence_scope = [ordered]@{
                    java_rust_successor_compared = [string]$previewed.status -in @('ready','diverged')
                    cross_invocation_resident = $true
                    strategy_evidence = $false
                    training_eligible = $false
                }
            }
            Write-JavaPlayJsonCreateNew $attemptReport $result
            $retiredPath = $null
            if (-not $returnedToNeutral) {
                $retiredPath = Retire-JavaPlayPairedRootResident $resident $retireReason 'Resident could not return to its neutral reusable boundary.' $attemptReport
            }
            $outcome = [pscustomobject]@{
                Result = [pscustomobject]$result
                ReportPath = [IO.Path]::GetFullPath($attemptReport)
                RetiredPath = $retiredPath
            }
            if ($PassThru) {
                return $outcome
            }
            Write-Output "PairedRoot resident preview status=$($result.status) resident=$($result.resident_id) target_root=$targetRoot action=$actionId successor_root=$($result.result.successor_root_id) receipt=$($result.result.receipt_id) divergence=$($result.result.divergence_id) reusable=$($result.resident_reusable)"
            Write-Output "timings_ms continue=$($result.timings_ms.continue) orchestrate=$($result.timings_ms.activate_recover_preview) reset=$($result.timings_ms.reset) total=$($result.timings_ms.total)"
            Write-Output "report=$([IO.Path]::GetFullPath($attemptReport)) retired=$(if($null -eq $retiredPath){'none'}else{$retiredPath})"
            return
        } catch {
            $failure = $_.Exception.Message
            if (-not (Test-Path -LiteralPath $failureReport -PathType Leaf)) {
                Write-JavaPlayJsonCreateNew $failureReport ([ordered]@{
                    schema = 'sts-java-play-paired-root-resident-preview-failure-v1'
                    resident_id = $resident.ResidentId
                    target_root_id = $targetRoot
                    action_handle_id = $actionId
                    created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                    failure = $failure
                    isolated_storage_retained = $true
                })
            }
            $retiredPath = Retire-JavaPlayPairedRootResident $resident 'runtime_failure' $failure $failureReport
            throw "Paired-root resident preview failed and retired its runtime; report=${failureReport} retired=${retiredPath}: $failure"
        }
        } finally {
            if (-not $RetainLease) {
                Exit-JavaPlayPairedRootResidentLease $lease
            }
        }
    }

    if ($mode -in @('status','close')) {
        if (-not $options.ContainsKey('--resident') -or
            @($options.Keys | Where-Object { $_ -notin @('--mode','--resident') }).Count -ne 0) {
            throw "Paired-root resident $mode accepts only --resident."
        }
        $lease = Enter-JavaPlayPairedRootResidentLease ([string]$options['--resident']) $mode
        try {
        $resident = Read-JavaPlayPairedRootResident ([string]$options['--resident']) -AllowNonNeutral
        if ($mode -eq 'status') {
            $residentStatus = if ($resident.Neutral) { 'ready' } else { 'non_neutral' }
            Write-Output "PairedRoot resident status=$residentStatus id=$($resident.ResidentId) native_root=$($resident.Manifest.native_root_id) session=$($resident.Manifest.session.session_id) game_pid=$($resident.GameProcess.Id) bridge_pid=$($resident.BridgeProcess.Id) boundary=$($resident.View.frame.boundary.kind) autosave=$($resident.Autosave.sha256)"
            Write-Output "manifest=$($resident.ManifestPath) storage=$($resident.Storage.WorkingDirectory)"
            return
        }

        if (-not $resident.Neutral) {
            $retiredPath = Retire-JavaPlayPairedRootResident `
                $resident `
                'explicit_close_non_neutral_boundary' `
                'Resident was no longer at its recorded neutral boundary when close was requested.' `
                $null
            Write-Output "PairedRoot resident status=retired id=$($resident.ResidentId) source_unchanged=unknown storage_retained=true"
            Write-Output "retired=$retiredPath"
            return
        }

        $closeStartedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        Stop-JavaPlayPairedRootResidentProcesses $resident
        $isolatedFinal = Get-JavaPlayStorageInventoryWithRetry $resident.Storage.WorkingDirectory
        $sourceAfter = Get-JavaPlayStorageInventoryWithRetry $gameDirectory
        $sourceUnchanged = [string]$sourceAfter.fingerprint_sha256 -eq [string]$resident.Manifest.source_game_initial.fingerprint_sha256
        $storageRetained = $true
        $cleanupFailure = $null
        if ($sourceUnchanged) {
            try {
                Remove-JavaPlayIsolatedStorage $resident.Storage
                $storageRetained = $false
            } catch {
                $cleanupFailure = $_.Exception.Message
            }
        }
        $close = [ordered]@{
            schema = 'sts-java-play-paired-root-resident-close-v1'
            resident_id = $resident.ResidentId
            created_at_unix_ms = $closeStartedAt
            completed_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
            passed = $sourceUnchanged -and -not $storageRetained -and $null -eq $cleanupFailure
            source_game_unchanged = $sourceUnchanged
            isolated_storage_retained = $storageRetained
            cleanup_failure = $cleanupFailure
            isolated_final = $isolatedFinal
            source_game_final = $sourceAfter
        }
        $closePath = Join-Path $resident.Directory 'close.json'
        Write-JavaPlayJsonCreateNew $closePath $close
        if (-not $close.passed) {
            throw "Paired-root resident stopped but clean close failed; report=$closePath"
        }
        Write-Output "PairedRoot resident status=closed id=$($resident.ResidentId) source_unchanged=$sourceUnchanged storage_retained=$storageRetained"
        Write-Output "close=$([IO.Path]::GetFullPath($closePath))"
        return
        } finally {
            Exit-JavaPlayPairedRootResidentLease $lease
        }
    }

    if (-not $options.ContainsKey('--target-root') -or $options.ContainsKey('--resident') -or
        @($options.Keys | Where-Object { $_ -notin @('--mode','--target-root','--store') }).Count -ne 0) {
        throw 'Paired-root resident open requires --target-root and optional --store.'
    }
    Assert-GameClosed
    if (-not (Test-Path -LiteralPath $runPlayWorkbench -PathType Leaf)) {
        throw 'Run-play workbench is missing. Run .\run-play.ps1 build first.'
    }
    $storePath = Resolve-JavaPlayPairedRootStore $options
    [IO.Directory]::CreateDirectory($pairedRootResidentOutput) | Out-Null
    $residentId = "paired-root-resident-v1-$([Guid]::NewGuid().ToString('N'))"
    $residentDirectory = Resolve-JavaPlayPairedRootResidentDirectory $residentId
    [IO.Directory]::CreateDirectory($residentDirectory) | Out-Null
    $nativePayload = Join-Path $residentDirectory 'native-payload'
    $materializationReport = Join-Path $residentDirectory 'native-materialization.json'
    $failurePath = Join-Path $residentDirectory 'open-failure.json'
    $sourceBefore = Get-JavaPlayStorageInventory $gameDirectory
    $storage = $null
    $launch = $null
    $bridgeProcessId = $null
    try {
        $materializeRaw = & $runPlayWorkbench paired-root-materialize-native --target-root $options['--target-root'] --store $storePath --output $nativePayload --report $materializationReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Paired-root resident materialization failed: $($materializeRaw -join "`n")"
        }
        $materialized = Get-Content -LiteralPath $materializationReport -Raw | ConvertFrom-Json -Depth 100
        if ($materialized.schema -ne 'sts-paired-root-native-materialization-report-v1' -or
            [string]$materialized.requested_target_root_id -ne [string]$options['--target-root'] -or
            [string]::IsNullOrWhiteSpace([string]$materialized.native_root_id) -or
            [string]::IsNullOrWhiteSpace([string]$materialized.java_bundle_id) -or
            [string]::IsNullOrWhiteSpace([string]$materialized.content_profile)) {
            throw "Paired-root resident materialization returned invalid evidence: $materializationReport"
        }
        $storage = New-JavaPlayIsolatedStorage ([string]$materialized.storage_directory)
        $isolatedInitial = Get-JavaPlayStorageInventory $storage.WorkingDirectory
        $autosaves = @(Get-ChildItem -LiteralPath (Join-Path $storage.WorkingDirectory 'saves') -Filter '*.autosave' -File)
        if ($autosaves.Count -ne 1) {
            throw "Paired-root resident requires exactly one active autosave; found $($autosaves.Count)."
        }
        $autosave = Get-JavaPlayFileIdentity $autosaves[0].FullName
        $relativeAutosave = [IO.Path]::GetRelativePath($storage.WorkingDirectory, $autosave.path)
        $launch = Start-JavaPlayLaunch @('--profile','accelerated','--control','run') -Hidden -Storage $storage
        $view = Wait-JavaPlayProbeSession $launch $isolatedStartupTimeoutSeconds
        $bridgeProcessId = [int]$view.manifest.process_id
        $actions = @($view.actions)
        if ([string]$view.frame.boundary.kind -ne 'main_menu_ready' -or
            [string]$view.frame.public_base.decision.kind -ne 'main_menu_ready' -or
            $actions.Count -ne 1 -or [string]$actions[0].kind -ne 'continue_run' -or
            [string]$actions[0].action_id -ne 'continue_run' -or
            [IO.Path]::GetFullPath([string]$view.manifest.launch.plan_path) -ne [IO.Path]::GetFullPath([string]$launch.Plan.Path)) {
            throw 'Paired-root resident did not reach one neutral Continue decision.'
        }
        $bridgeProcess = Get-Process -Id $bridgeProcessId -ErrorAction Stop
        $manifest = [ordered]@{
            schema = 'sts-java-play-paired-root-resident-v1'
            resident_id = $residentId
            created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
            resident_directory = [IO.Path]::GetFullPath($residentDirectory)
            requested_target_root_id = [string]$options['--target-root']
            native_root_id = [string]$materialized.native_root_id
            java_bundle_id = [string]$materialized.java_bundle_id
            content_profile = [string]$materialized.content_profile
            store_root = $storePath
            materialization = Get-JavaPlayFileIdentity $materializationReport
            launch_plan = Get-JavaPlayFileIdentity $launch.Plan.Path
            live_binary = Get-JavaPlayFileIdentity $liveBinary
            run_play_workbench = Get-JavaPlayFileIdentity $runPlayWorkbench
            session = [ordered]@{
                session_id = [string]$view.manifest.session_id
                connection_id = [string]$view.manifest.connection_id
            }
            game_process = Get-JavaPlayProcessIdentity $launch.Process
            bridge_process = Get-JavaPlayProcessIdentity $bridgeProcess
            storage = [ordered]@{
                class = $storage.Class
                working_directory = $storage.WorkingDirectory
                source_game_directory = $storage.SourceGameDirectory
                isolation_id = $storage.IsolationId
            }
            autosave = [ordered]@{
                relative_path = $relativeAutosave
                length = [long]$autosave.length
                sha256 = [string]$autosave.sha256
            }
            source_game_initial = $sourceBefore
            isolated_initial = $isolatedInitial
        }
        $manifestPath = Join-Path $residentDirectory 'resident.json'
        Write-JavaPlayJsonCreateNew $manifestPath $manifest
        if ($PassThru) {
            return [pscustomobject]@{
                ResidentId = $residentId
                ManifestPath = [IO.Path]::GetFullPath($manifestPath)
                Created = $true
            }
        }
        Write-Output "PairedRoot resident status=ready id=$residentId native_root=$($manifest.native_root_id) session=$($manifest.session.session_id) game_pid=$($manifest.game_process.process_id) bridge_pid=$($manifest.bridge_process.process_id)"
        Write-Output "manifest=$([IO.Path]::GetFullPath($manifestPath)) storage=$($storage.WorkingDirectory)"
    } catch {
        $failure = $_.Exception.Message
        if ($null -ne $launch) {
            try {
                Stop-JavaPlayProbeLaunch $launch $bridgeProcessId
            } catch {
                $failure = "$failure shutdown_failure=$($_.Exception.Message)"
            }
        }
        if (-not (Test-Path -LiteralPath $failurePath -PathType Leaf)) {
            Write-JavaPlayJsonCreateNew $failurePath ([ordered]@{
                schema = 'sts-java-play-paired-root-resident-open-failure-v1'
                resident_id = $residentId
                created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                failure = $failure
                isolated_storage_retained = $null -ne $storage
                isolated_storage = if ($null -eq $storage) { $null } else { $storage.WorkingDirectory }
            })
        }
        throw "Paired-root resident open failed; report=${failurePath}: $failure"
    }
}

function Write-JavaPlayJsonCreateNew([string] $Path, [object] $Value) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    $json = $Value | ConvertTo-Json -Depth 100
    $stream = [IO.FileStream]::new($fullPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    $writer = [IO.StreamWriter]::new($stream, [Text.UTF8Encoding]::new($false))
    try {
        $writer.WriteLine($json)
    } finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

function Invoke-JavaPlayIsolatedCase {
    param(
        [Parameter(Mandatory)] [string] $CaseKind,
        [Parameter(Mandatory)] [string] $Profile,
        [Parameter(Mandatory)] [string] $ControlProfile,
        [Parameter(Mandatory)] [string] $EvidencePath,
        [Parameter(Mandatory)] [string] $LifecyclePath,
        [object] $StorageSeed,
        [Parameter(Mandatory)] [scriptblock] $Body
    )
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $createdAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $sourceBefore = Get-JavaPlayStorageInventory $gameDirectory
    $storage = New-JavaPlayIsolatedStorage $(
        if ($null -eq $StorageSeed) { $null } else { [string]$StorageSeed.StorageRoot }
    )
    $isolatedInitial = Get-JavaPlayStorageInventory $storage.WorkingDirectory
    $preparedAt = $clock.ElapsedMilliseconds
    $launch = $null
    $view = $null
    $bridgeProcessId = $null
    $caseValue = $null
    $casePassed = $false
    $failure = $null
    $shutdownFailure = $null
    $inventoryFailure = $null
    $sessionReadyAt = $null
    $caseCompletedAt = $null
    $shutdownCompletedAt = $null
    try {
        $controlName = switch ($ControlProfile) {
            'parity_run_control' { 'run' }
            'parity_scenario_control' { 'scenario' }
            default { throw "Isolated case does not support control profile: $ControlProfile" }
        }
        $launch = Start-JavaPlayLaunch @('--profile',$Profile,'--control',$controlName) -Hidden -Storage $storage
        $view = Wait-JavaPlayProbeSession $launch $isolatedStartupTimeoutSeconds
        $sessionReadyAt = $clock.ElapsedMilliseconds
        $bridgeProcessId = [int]$view.manifest.process_id
        $caseValue = & $Body $launch $view
        if ($null -eq $caseValue) {
            throw 'Isolated Java case returned no typed result.'
        }
        $casePassed = $true
        $caseCompletedAt = $clock.ElapsedMilliseconds
    } catch {
        $failure = $_.Exception.Message
        if (-not [string]::IsNullOrWhiteSpace($_.ScriptStackTrace)) {
            $failure = "$failure script_stack=$($_.ScriptStackTrace -replace "`r?`n", ' | ')"
        }
        $caseCompletedAt = $clock.ElapsedMilliseconds
    } finally {
        if ($null -ne $launch) {
            try {
                Stop-JavaPlayProbeLaunch $launch $bridgeProcessId
            } catch {
                $shutdownFailure = $_.Exception.Message
                $casePassed = $false
                $failure = if ($null -eq $failure) { $shutdownFailure } else { "$failure $shutdownFailure" }
            }
        }
        $shutdownCompletedAt = $clock.ElapsedMilliseconds
    }

    try {
        $isolatedFinal = Get-JavaPlayStorageInventoryWithRetry $storage.WorkingDirectory
        $sourceAfter = Get-JavaPlayStorageInventoryWithRetry $gameDirectory
    } catch {
        $inventoryFailure = $_.Exception.Message
        $casePassed = $false
        $failure = if ($null -eq $failure) { $inventoryFailure } else { "$failure $inventoryFailure" }
        $isolatedFinal = $null
        $sourceAfter = $null
    }
    $inventoriedAt = $clock.ElapsedMilliseconds
    $sourceUnchanged = $null -ne $sourceAfter -and $sourceBefore.fingerprint_sha256 -eq $sourceAfter.fingerprint_sha256
    if ($null -ne $sourceAfter -and -not $sourceUnchanged) {
        $casePassed = $false
        $failure = if ($null -eq $failure) {
            'Installed game storage changed during an isolated Java case.'
        } else {
            "$failure Installed game storage also changed during the isolated case."
        }
    }

    $storageRetained = $true
    $cleanupFailure = $null
    if ($casePassed) {
        try {
            Remove-JavaPlayIsolatedStorage $storage
            $storageRetained = $false
        } catch {
            $cleanupFailure = $_.Exception.Message
            $casePassed = $false
            $failure = if ($null -eq $failure) { $cleanupFailure } else { "$failure $cleanupFailure" }
        }
    }
    $cleanedAt = $clock.ElapsedMilliseconds
    $evidence = if (Test-Path -LiteralPath $EvidencePath -PathType Leaf) {
        Get-JavaPlayFileIdentity $EvidencePath
    } else {
        $null
    }
    $launchPlan = if ($null -ne $launch) { Get-JavaPlayFileIdentity $launch.Plan.Path } else { $null }
    $lifecycle = [ordered]@{
        schema = 'sts_java_play_isolated_lifecycle_v1'
        case_kind = $CaseKind
        created_at_unix_ms = $createdAt
        completed_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        passed = $casePassed
        failure = $failure
        profile = $Profile
        control_profile = $ControlProfile
        session_id = if ($null -eq $view) { $null } else { [string]$view.manifest.session_id }
        shutdown_failure = $shutdownFailure
        inventory_failure = $inventoryFailure
        launch_plan = $launchPlan
        case_evidence = $evidence
        timings_ms = [ordered]@{
            startup_timeout_budget = [long]$isolatedStartupTimeoutSeconds * 1000
            prepare_storage_and_guard = $preparedAt
            launch_to_session_ready = if ($null -eq $sessionReadyAt) { $null } else { $sessionReadyAt - $preparedAt }
            case_after_session_ready = if ($null -eq $sessionReadyAt) { $null } else { $caseCompletedAt - $sessionReadyAt }
            managed_shutdown = $shutdownCompletedAt - $caseCompletedAt
            final_inventories = $inventoriedAt - $shutdownCompletedAt
            cleanup = $cleanedAt - $inventoriedAt
            total = $cleanedAt
        }
        isolated_storage = [ordered]@{
            class = $storage.Class
            isolation_id = $storage.IsolationId
            working_directory = $storage.WorkingDirectory
            retained = $storageRetained
            cleanup_failure = $cleanupFailure
            initial = $isolatedInitial
            final = $isolatedFinal
        }
        storage_seed = if ($null -eq $StorageSeed) {
            $null
        } else {
            [ordered]@{
                class = if ($null -eq $StorageSeed.PSObject.Properties['Class']) {
                    'validated_native_combat_checkpoint_v1'
                } else {
                    [string]$StorageSeed.Class
                }
                manifest = $StorageSeed.ManifestIdentity
            }
        }
        installed_storage_guard = [ordered]@{
            unchanged = $sourceUnchanged
            before = $sourceBefore
            after = $sourceAfter
        }
    }
    Write-JavaPlayJsonCreateNew $LifecyclePath $lifecycle
    if (-not $casePassed) {
        throw "Isolated Java case failed; lifecycle=$([IO.Path]::GetFullPath($LifecyclePath)) detail=$failure"
    }
    return $caseValue
}

function Invoke-JavaPlayProbe([string[]] $CommandArguments) {
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'natural-base-strike') {
        Assert-LiveBridgeFresh
        Assert-JavaParityComparatorFresh
        Assert-GameClosed
        [IO.Directory]::CreateDirectory($probeOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $probeDirectory = Join-Path $probeOutput "natural-base-strike-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($probeDirectory) | Out-Null
        $report = Join-Path $probeDirectory 'producer-report.json'
        $lifecycle = Join-Path $probeDirectory 'lifecycle.json'
        $comparison = Join-Path $probeDirectory 'comparison.json'
        $workflow = Join-Path $probeDirectory 'workflow.json'
        $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'natural_combat_base_strike' -Profile 'accelerated' -ControlProfile 'parity_run_control' -EvidencePath $report -LifecyclePath $lifecycle -Body {
            param($launch, $view)
            $result = & $liveBinary probe-natural-strike --output-dir $liveOutput --expected-launch-plan $launch.Plan.Path --report-path $report 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Natural base Strike producer failed: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed -or $value.provenance -ne 'natural_replay') {
                throw "Natural base Strike producer returned invalid evidence: $report"
            }
            return $value
        }
        $verify = & $compareBinary --verify-natural-combat $report --lifecycle-receipt $lifecycle --comparison-report $comparison --output $workflow 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Natural base Strike verification failed: $($verify -join "`n")"
        }
        $bound = Get-Content -LiteralPath $workflow -Raw | ConvertFrom-Json -Depth 100
        if (
            -not $bound.passed -or
            $bound.provenance -ne 'natural_replay' -or
            -not $bound.evidence_scope.natural_reachability -or
            $bound.evidence_scope.strategy_evidence -or
            $bound.evidence_scope.training_eligible -or
            $bound.evidence_scope.constructed_root_verified -or
            -not $bound.evidence_scope.original_game_navigation_captured -or
            -not $bound.evidence_scope.original_game_transition_captured -or
            -not $bound.evidence_scope.simulator_comparison -or
            -not $bound.evidence_scope.narrow_mechanics_parity -or
            -not $bound.evidence_scope.managed_lifecycle_verified
        ) {
            throw "Natural base Strike workflow has invalid evidence scope: $workflow"
        }
        Write-Output "Natural base Strike parity passed session=$($parsed.session.session_id) transition=$($bound.action.from_frame_id)->$($bound.action.to_frame_id) lifecycle_total_ms=$($bound.lifecycle_total_ms)"
        Write-Output "producer_report=$([IO.Path]::GetFullPath($report)) comparison=$([IO.Path]::GetFullPath($comparison)) lifecycle=$([IO.Path]::GetFullPath($lifecycle)) workflow=$([IO.Path]::GetFullPath($workflow)) provenance=$($bound.provenance)"
        return
    }
    if ($CommandArguments.Count -ne 1 -or $CommandArguments[0] -ne 'elixir-hand-select') {
        throw 'Probe command must be exactly: probe elixir-hand-select | probe natural-base-strike'
    }
    Assert-LiveBridgeFresh
    Assert-GameClosed
    [IO.Directory]::CreateDirectory($probeOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $probeDirectory = Join-Path $probeOutput "elixir-hand-select-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    [IO.Directory]::CreateDirectory($probeDirectory) | Out-Null
    $reports = [Collections.Generic.List[string]]::new()
    $lifecycles = [Collections.Generic.List[string]]::new()
    $sessionIds = [Collections.Generic.List[string]]::new()
    foreach ($variant in @('zero','first-strike')) {
        $report = Join-Path $probeDirectory "$variant.json"
        $lifecycle = Join-Path $probeDirectory "$variant-lifecycle.json"
        $parsed = Invoke-JavaPlayIsolatedCase -CaseKind "natural_probe_elixir_$variant" -Profile 'accelerated' -ControlProfile 'parity_run_control' -EvidencePath $report -LifecyclePath $lifecycle -Body {
            param($launch, $view)
            $result = & $liveBinary probe-elixir --output-dir $liveOutput --variant $variant --expected-launch-plan $launch.Plan.Path --report-path $report 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Elixir probe variant $variant failed: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed) {
                throw "Elixir probe variant $variant produced a non-passing report: $report"
            }
            return $value
        }
        $reports.Add([IO.Path]::GetFullPath($report))
        $lifecycles.Add([IO.Path]::GetFullPath($lifecycle))
        $sessionIds.Add($parsed.session.session_id)
    }
    $bundle = Join-Path $probeDirectory 'bundle.json'
    $bundleResult = & $liveBinary bundle-elixir-probe --report $reports[0] $reports[1] --output-path $bundle 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Elixir probe bundle failed: $($bundleResult -join "`n")"
    }
    $parsedBundle = ($bundleResult -join "`n") | ConvertFrom-Json -Depth 100
    $workflow = Join-Path $probeDirectory 'workflow.json'
    Write-JavaPlayJsonCreateNew $workflow ([ordered]@{
        schema = 'sts_java_play_natural_probe_workflow_v1'
        created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        passed = $true
        provenance = [string]$parsedBundle.provenance
        proof_bundle = Get-JavaPlayFileIdentity $bundle
        lifecycle_receipts = @($lifecycles | ForEach-Object { Get-JavaPlayFileIdentity $_ })
        session_ids = @($sessionIds)
    })
    Write-Output "Elixir HAND_SELECT natural probe passed variants=zero,first-strike sessions=$($sessionIds -join ',')"
    Write-Output "proof_bundle=$([IO.Path]::GetFullPath($bundle)) workflow=$([IO.Path]::GetFullPath($workflow)) provenance=$($parsedBundle.provenance)"
}

function Resolve-JavaPlayScenarioRunOptions([string[]] $OptionArguments, [bool] $RequireProbeCase) {
    $draftPath = $null
    $caseId = $null
    $probeCaseId = $null
    $profile = 'accelerated'
    for ($index = 0; $index -lt $OptionArguments.Count; $index += 2) {
        if ($index + 1 -ge $OptionArguments.Count) {
            throw 'Scenario options require a value.'
        }
        switch ($OptionArguments[$index]) {
            '--draft' { $draftPath = $OptionArguments[$index + 1] }
            '--case' { $caseId = $OptionArguments[$index + 1] }
            '--probe-case' { $probeCaseId = $OptionArguments[$index + 1] }
            '--profile' {
                $profile = $OptionArguments[$index + 1].ToLowerInvariant()
                if ($profile -notin @('strict','accelerated')) {
                    throw "Unknown scenario launch profile: $profile"
                }
            }
            default { throw "Unknown scenario option: $($OptionArguments[$index])" }
        }
    }
    if ($RequireProbeCase) {
        if ([string]::IsNullOrWhiteSpace($probeCaseId) -or -not [string]::IsNullOrWhiteSpace($draftPath) -or -not [string]::IsNullOrWhiteSpace($caseId)) {
            throw 'Scenario probe/verify requires exactly --probe-case <exact-probe-case-id>.'
        }
    } else {
        if ([string]::IsNullOrWhiteSpace($draftPath) -eq [string]::IsNullOrWhiteSpace($caseId) -or -not [string]::IsNullOrWhiteSpace($probeCaseId)) {
            throw 'Scenario create-combat requires exactly one of --case <exact-case-id> or --draft <draft.json>.'
        }
    }
    return [pscustomobject]@{
        DraftPath = $draftPath
        CaseId = $caseId
        ProbeCaseId = $probeCaseId
        Profile = $profile
    }
}

function Resolve-JavaPlayScenarioTrajectoryOptions([string[]] $OptionArguments) {
    $trajectoryCaseId = $null
    $profile = 'accelerated'
    $stepIndices = [Collections.Generic.List[int]]::new()
    for ($index = 0; $index -lt $OptionArguments.Count; $index += 2) {
        if ($index + 1 -ge $OptionArguments.Count) {
            throw 'Scenario trajectory options require a value.'
        }
        switch ($OptionArguments[$index]) {
            '--trajectory-case' { $trajectoryCaseId = $OptionArguments[$index + 1] }
            '--step-index' {
                $parsedIndex = 0
                if (-not [int]::TryParse($OptionArguments[$index + 1], [ref]$parsedIndex) -or $parsedIndex -lt 0) {
                    throw "Scenario trajectory step index must be a nonnegative integer: $($OptionArguments[$index + 1])"
                }
                $stepIndices.Add($parsedIndex)
            }
            '--profile' {
                $profile = $OptionArguments[$index + 1].ToLowerInvariant()
                if ($profile -notin @('strict','accelerated')) {
                    throw "Unknown scenario trajectory launch profile: $profile"
                }
            }
            default { throw "Unknown scenario trajectory option: $($OptionArguments[$index])" }
        }
    }
    if ([string]::IsNullOrWhiteSpace($trajectoryCaseId)) {
        throw 'Scenario trajectory requires exactly --trajectory-case <exact-trajectory-case-id>.'
    }
    $orderedIndices = @($stepIndices | Sort-Object -Unique)
    if ($orderedIndices.Count -ne $stepIndices.Count) {
        throw 'Scenario trajectory step indices must be unique.'
    }
    return [pscustomobject]@{
        TrajectoryCaseId = $trajectoryCaseId
        Profile = $profile
        StepIndices = $orderedIndices
    }
}

function Invoke-JavaPlaySelectedTrajectoryComparison(
    [string] $TrajectoryReport,
    [int[]] $StepIndices,
    [string] $OutputDirectory
) {
    if ($StepIndices.Count -eq 0) {
        throw 'Selected trajectory comparison requires at least one step index.'
    }
    $orderedIndices = @($StepIndices | Sort-Object -Unique)
    if ($orderedIndices.Count -ne $StepIndices.Count) {
        throw 'Selected trajectory comparison step indices must be unique.'
    }
    $TrajectoryReport = [IO.Path]::GetFullPath($TrajectoryReport)
    if (-not (Test-Path -LiteralPath $TrajectoryReport -PathType Leaf)) {
        throw "Constructed trajectory report does not exist: $TrajectoryReport"
    }
    if (Test-Path -LiteralPath $OutputDirectory) {
        throw "Selected trajectory comparison output already exists: $OutputDirectory"
    }
    [IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
    $comparisonDirectory = Join-Path $OutputDirectory 'comparisons'
    $attemptReport = Join-Path $OutputDirectory 'comparison-attempt.json'
    $arguments = [Collections.Generic.List[string]]::new()
    $arguments.Add('--recompare-constructed-trajectory')
    $arguments.Add($TrajectoryReport)
    foreach ($stepIndex in $orderedIndices) {
        $arguments.Add('--trajectory-step-index')
        $arguments.Add([string]$stepIndex)
    }
    $arguments.Add('--comparison-directory')
    $arguments.Add($comparisonDirectory)
    $arguments.Add('--output')
    $arguments.Add($attemptReport)
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $compareResult = & $compareBinary @arguments 2>&1
    $clock.Stop()
    if ($LASTEXITCODE -ne 0) {
        throw "Constructed trajectory selected-step comparison failed; retained output=$OutputDirectory detail=$($compareResult -join "`n")"
    }
    $attempt = Get-Content -LiteralPath $attemptReport -Raw | ConvertFrom-Json -Depth 100
    $actualIndices = @($attempt.requested_step_indices | ForEach-Object { [int]$_ })
    if (-not $attempt.passed -or $attempt.provenance -ne 'constructed_validated' -or -not $attempt.evidence_scope.selected_steps_constructed_and_matched -or -not $attempt.evidence_scope.narrow_mechanics_parity -or ($actualIndices -join ',') -ne ($orderedIndices -join ',')) {
        throw "Constructed trajectory selected-step comparison did not produce an honest passing attempt: $attemptReport"
    }
    return [pscustomobject]@{
        Attempt = $attempt
        OrderedIndices = $orderedIndices
        AttemptReport = [IO.Path]::GetFullPath($attemptReport)
        ComparisonDirectory = [IO.Path]::GetFullPath($comparisonDirectory)
        ElapsedMs = $clock.ElapsedMilliseconds
    }
}

function New-JavaPlayCompiledScenarioPlan([string] $ScenarioDirectory, [object] $Options) {
    $scenarioPlan = [IO.Path]::GetFullPath((Join-Path $ScenarioDirectory 'scenario-plan.json'))
    if (-not [string]::IsNullOrWhiteSpace($Options.CaseId)) {
        $compileResult = & $liveBinary compile-scenario-case --catalog-directory $scenarioCaseCatalog --case-id $Options.CaseId --output-plan $scenarioPlan 2>&1
    } else {
        $compileResult = & $liveBinary compile-scenario --draft ([IO.Path]::GetFullPath($Options.DraftPath)) --output-plan $scenarioPlan 2>&1
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Scenario draft compilation failed: $($compileResult -join "`n")"
    }
    return $scenarioPlan
}

function Invoke-JavaPlayScenarioTransitionCase(
    [object] $Options,
    [bool] $RunComparison,
    [string] $ScenarioDirectory
) {
    Assert-GameClosed
    [IO.Directory]::CreateDirectory($ScenarioDirectory) | Out-Null
    $scenarioPlan = [IO.Path]::GetFullPath((Join-Path $ScenarioDirectory 'scenario-plan.json'))
    $scenarioReport = Join-Path $ScenarioDirectory 'report.json'
    $lifecycle = Join-Path $ScenarioDirectory 'lifecycle.json'
    $probePlan = Join-Path $ScenarioDirectory 'probe-plan.json'
    $probeReport = Join-Path $ScenarioDirectory 'probe-report.json'
    $compileProbe = & $liveBinary compile-scenario-probe-case --probe-catalog-directory $scenarioProbeCaseCatalog --probe-case-id $Options.ProbeCaseId --scenario-catalog-directory $scenarioCaseCatalog --output-scenario-plan $scenarioPlan --output-probe-plan $probePlan 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Scenario probe plan compilation failed before launch: $($compileProbe -join "`n")"
    }
    $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'constructed_combat_probe' -Profile $Options.Profile -ControlProfile 'parity_scenario_control' -EvidencePath $probeReport -LifecyclePath $lifecycle -Body {
        param($launch, $view)
        $result = & $liveBinary run-scenario-probe --output-dir $liveOutput --probe-plan $probePlan --expected-launch-plan $launch.Plan.Path --scenario-report-path $scenarioReport --probe-report-path $probeReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Constructed combat probe failed: $($result -join "`n")"
        }
        $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
        if (-not $value.passed -or $value.provenance -ne 'constructed_validated' -or $null -eq $value.action) {
            throw "Constructed combat probe did not produce a passing typed report: $probeReport"
        }
        return $value
    }
    $comparisonReport = $null
    $workflowReport = $null
    $workflow = $null
    if ($RunComparison) {
        $comparisonReport = Join-Path $ScenarioDirectory 'comparison.json'
        $workflowReport = Join-Path $ScenarioDirectory 'workflow.json'
        $compareResult = & $compareBinary --live-action-receipt $parsed.action.receipt_path --output $comparisonReport --constructed-probe-report $probeReport --workflow-output $workflowReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Constructed combat comparison failed after a passing probe: $($compareResult -join "`n")"
        }
        $workflow = Get-Content -LiteralPath $workflowReport -Raw | ConvertFrom-Json -Depth 100
        if (-not $workflow.passed -or $workflow.provenance -ne 'constructed_validated' -or -not $workflow.evidence_scope.simulator_comparison) {
            throw "Constructed combat verification did not produce a passing bound workflow: $workflowReport"
        }
    }
    return [pscustomobject]@{
        Parsed = $parsed
        Workflow = $workflow
        ScenarioDirectory = [IO.Path]::GetFullPath($ScenarioDirectory)
        ScenarioPlan = [IO.Path]::GetFullPath($scenarioPlan)
        ProbePlan = [IO.Path]::GetFullPath($probePlan)
        ScenarioReport = [IO.Path]::GetFullPath($scenarioReport)
        ProbeReport = [IO.Path]::GetFullPath($probeReport)
        ComparisonReport = if ($null -eq $comparisonReport) { $null } else { [IO.Path]::GetFullPath($comparisonReport) }
        WorkflowReport = if ($null -eq $workflowReport) { $null } else { [IO.Path]::GetFullPath($workflowReport) }
        Lifecycle = [IO.Path]::GetFullPath($lifecycle)
    }
}

function Invoke-JavaPlayScenarioTrajectoryCase([object] $Options, [string] $ScenarioDirectory) {
    Assert-GameClosed
    [IO.Directory]::CreateDirectory($ScenarioDirectory) | Out-Null
    $scenarioPlan = [IO.Path]::GetFullPath((Join-Path $ScenarioDirectory 'scenario-plan.json'))
    $trajectoryPlan = Join-Path $ScenarioDirectory 'trajectory-plan.json'
    $scenarioReport = Join-Path $ScenarioDirectory 'report.json'
    $trajectoryReport = Join-Path $ScenarioDirectory 'trajectory-report.json'
    $lifecycle = Join-Path $ScenarioDirectory 'lifecycle.json'
    $comparisonDirectory = $null
    $comparisonReport = $null
    $comparisonMode = $null
    $compile = & $liveBinary compile-scenario-trajectory-case --trajectory-catalog-directory $scenarioTrajectoryCaseCatalog --trajectory-case-id $Options.TrajectoryCaseId --scenario-catalog-directory $scenarioCaseCatalog --output-scenario-plan $scenarioPlan --output-trajectory-plan $trajectoryPlan 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Scenario trajectory plan compilation failed before launch: $($compile -join "`n")"
    }
    if ($Options.StepIndices.Count -gt 0) {
        $compiledTrajectory = Get-Content -LiteralPath $trajectoryPlan -Raw | ConvertFrom-Json -Depth 100
        foreach ($stepIndex in $Options.StepIndices) {
            if ($stepIndex -ge $compiledTrajectory.steps.Count) {
                throw "Scenario trajectory step index $stepIndex is outside compiled plan step count $($compiledTrajectory.steps.Count)."
            }
        }
    }
    $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'constructed_combat_trajectory' -Profile $Options.Profile -ControlProfile 'parity_scenario_control' -EvidencePath $trajectoryReport -LifecyclePath $lifecycle -Body {
        param($launch, $view)
        $result = & $liveBinary run-scenario-trajectory --output-dir $liveOutput --trajectory-plan $trajectoryPlan --expected-launch-plan $launch.Plan.Path --scenario-report-path $scenarioReport --trajectory-report-path $trajectoryReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Constructed combat trajectory failed: $($result -join "`n")"
        }
        $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
        if (-not $value.passed -or $value.provenance -ne 'constructed_validated' -or $value.completed_steps -ne $value.planned_steps -or -not $value.evidence_scope.original_game_trajectory_captured -or $value.evidence_scope.simulator_comparison) {
            throw "Constructed combat trajectory did not produce a passing capture-only report: $trajectoryReport"
        }
        return $value
    }
    if ($Options.StepIndices.Count -gt 0) {
        $comparisonMode = 'selected_steps'
        $selectedDirectory = Join-Path $ScenarioDirectory 'selected-trajectory-comparison'
        $selected = Invoke-JavaPlaySelectedTrajectoryComparison $trajectoryReport $Options.StepIndices $selectedDirectory
        $bound = $selected.Attempt
        $comparisonDirectory = $selected.ComparisonDirectory
        $comparisonReport = $selected.AttemptReport
    } else {
        $comparisonMode = 'all_steps'
        $comparisonDirectory = Join-Path $ScenarioDirectory 'trajectory-comparisons'
        $comparisonReport = Join-Path $ScenarioDirectory 'trajectory-workflow.json'
        $verify = & $compareBinary --verify-constructed-trajectory $trajectoryReport --lifecycle-receipt $lifecycle --comparison-directory $comparisonDirectory --output $comparisonReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Constructed trajectory comparison failed after a passing capture; retained output=$ScenarioDirectory detail=$($verify -join "`n")"
        }
        $bound = Get-Content -LiteralPath $comparisonReport -Raw | ConvertFrom-Json -Depth 100
        if (-not $bound.passed -or $bound.provenance -ne 'constructed_validated' -or -not $bound.evidence_scope.simulator_comparison -or -not $bound.evidence_scope.all_steps_constructed_and_matched -or -not $bound.evidence_scope.managed_lifecycle_verified -or $bound.evidence_scope.natural_reachability -or $bound.evidence_scope.training_eligible) {
            throw "Constructed trajectory verification did not produce a passing scoped workflow: $comparisonReport"
        }
    }
    return [pscustomobject]@{
        Parsed = $parsed
        Comparison = $bound
        ComparisonMode = $comparisonMode
        ScenarioPlan = [IO.Path]::GetFullPath($scenarioPlan)
        TrajectoryPlan = [IO.Path]::GetFullPath($trajectoryPlan)
        ScenarioReport = [IO.Path]::GetFullPath($scenarioReport)
        TrajectoryReport = [IO.Path]::GetFullPath($trajectoryReport)
        Lifecycle = [IO.Path]::GetFullPath($lifecycle)
        ComparisonDirectory = [IO.Path]::GetFullPath($comparisonDirectory)
        ComparisonReport = [IO.Path]::GetFullPath($comparisonReport)
    }
}

function Invoke-JavaPlayScenario([string[]] $CommandArguments) {
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'registry') {
        Assert-LiveBridgeFresh
        Invoke-LiveBridge @('scenario-registry')
        return
    }
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'cases') {
        Assert-LiveBridgeFresh
        Invoke-LiveBridge @('scenario-case-catalog','--catalog-directory',$scenarioCaseCatalog)
        return
    }
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'probe-cases') {
        Assert-LiveBridgeFresh
        Invoke-LiveBridge @('scenario-probe-case-catalog','--catalog-directory',$scenarioProbeCaseCatalog)
        return
    }
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'trajectories') {
        Assert-LiveBridgeFresh
        Invoke-LiveBridge @('scenario-trajectory-case-catalog','--catalog-directory',$scenarioTrajectoryCaseCatalog)
        return
    }
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'suites') {
        Assert-JavaParitySuiteFresh
        & $suiteBinary catalog --suite-catalog-directory $scenarioSuiteCatalog
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
        return
    }
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'shared-suites') {
        Assert-JavaParitySuiteFresh
        & $suiteBinary catalog-shared --suite-catalog-directory $scenarioSharedRuntimeSuiteCatalog
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
        return
    }
    if ($CommandArguments.Count -eq 1 -and $CommandArguments[0] -eq 'shared-trajectory-suites') {
        Assert-JavaParitySuiteFresh
        & $suiteBinary catalog-shared-trajectory --suite-catalog-directory $scenarioSharedRuntimeTrajectorySuiteCatalog
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'recompare') {
        if ($CommandArguments.Count -ne 3 -or $CommandArguments[1] -ne '--probe-report' -or [string]::IsNullOrWhiteSpace($CommandArguments[2])) {
            throw 'Scenario recompare command must be exactly: scenario recompare --probe-report <probe-report.json>.'
        }
        Assert-JavaParityComparatorFresh
        $probeReport = [IO.Path]::GetFullPath($CommandArguments[2])
        if (-not (Test-Path -LiteralPath $probeReport -PathType Leaf)) {
            throw "Constructed probe report does not exist: $probeReport"
        }
        [IO.Directory]::CreateDirectory($scenarioOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $recompareDirectory = Join-Path $scenarioOutput "recompare-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($recompareDirectory) | Out-Null
        $comparisonReport = Join-Path $recompareDirectory 'comparison.json'
        $workflowReport = Join-Path $recompareDirectory 'workflow.json'
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $compareResult = & $compareBinary --recompare-constructed-probe $probeReport --output $comparisonReport --workflow-output $workflowReport 2>&1
        $clock.Stop()
        if ($LASTEXITCODE -ne 0) {
            throw "Constructed probe recompare failed; retained output=$recompareDirectory detail=$($compareResult -join "`n")"
        }
        $workflow = Get-Content -LiteralPath $workflowReport -Raw | ConvertFrom-Json -Depth 100
        if (-not $workflow.passed -or $workflow.provenance -ne 'constructed_validated' -or -not $workflow.evidence_scope.simulator_comparison) {
            throw "Constructed probe recompare did not produce a passing bound workflow: $workflowReport"
        }
        Write-Output "Constructed probe recompare passed session=$($workflow.session_id) kind=$($workflow.comparison_kind) frames=$($workflow.from_frame_id)->$($workflow.to_frame_id) elapsed_ms=$($clock.ElapsedMilliseconds)"
        Write-Output "probe_report=$probeReport comparison_report=$([IO.Path]::GetFullPath($comparisonReport)) workflow=$([IO.Path]::GetFullPath($workflowReport)) provenance=$($workflow.provenance)"
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'recompare-trajectory') {
        $trajectoryReport = $null
        $stepIndices = [Collections.Generic.List[int]]::new()
        $options = @($CommandArguments | Select-Object -Skip 1)
        for ($index = 0; $index -lt $options.Count; $index += 2) {
            if ($index + 1 -ge $options.Count) {
                throw 'Scenario recompare-trajectory options require a value.'
            }
            switch ($options[$index]) {
                '--trajectory-report' { $trajectoryReport = $options[$index + 1] }
                '--step-index' {
                    $parsedIndex = 0
                    if (-not [int]::TryParse($options[$index + 1], [ref]$parsedIndex) -or $parsedIndex -lt 0) {
                        throw "Scenario trajectory step index must be a nonnegative integer: $($options[$index + 1])"
                    }
                    $stepIndices.Add($parsedIndex)
                }
                default { throw "Unknown scenario recompare-trajectory option: $($options[$index])" }
            }
        }
        if ([string]::IsNullOrWhiteSpace($trajectoryReport) -or $stepIndices.Count -eq 0) {
            throw 'Scenario recompare-trajectory requires --trajectory-report <trajectory-report.json> and at least one --step-index <zero-based-index>.'
        }
        Assert-JavaParityComparatorFresh
        $trajectoryReport = [IO.Path]::GetFullPath($trajectoryReport)
        [IO.Directory]::CreateDirectory($scenarioOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $recompareDirectory = Join-Path $scenarioOutput "recompare-trajectory-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        $selected = Invoke-JavaPlaySelectedTrajectoryComparison $trajectoryReport $stepIndices.ToArray() $recompareDirectory
        $attempt = $selected.Attempt
        Write-Output "Constructed trajectory selected-step recompare passed session=$($attempt.session_id) selected=$($selected.OrderedIndices -join ',') captured=$($attempt.captured_steps) matched=$($attempt.completed_steps) elapsed_ms=$($selected.ElapsedMs)"
        Write-Output "trajectory_report=$trajectoryReport comparison_directory=$($selected.ComparisonDirectory) attempt_report=$($selected.AttemptReport) provenance=$($attempt.provenance) all_steps_matched=$($attempt.evidence_scope.all_steps_constructed_and_matched)"
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'reuse-probe') {
        $probeReuse = $CommandArguments -contains '--probe-case'
        $options = Resolve-JavaPlayScenarioRunOptions @($CommandArguments | Select-Object -Skip 1) $probeReuse
        Assert-LiveBridgeFresh
        if ($probeReuse) {
            Assert-JavaParityComparatorFresh
        }
        Assert-GameClosed
        [IO.Directory]::CreateDirectory($scenarioOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $reuseDirectory = Join-Path $scenarioOutput "reuse-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($reuseDirectory) | Out-Null
        $firstPlanDirectory = Join-Path $reuseDirectory 'first-plan'
        $secondPlanDirectory = Join-Path $reuseDirectory 'second-plan'
        [IO.Directory]::CreateDirectory($firstPlanDirectory) | Out-Null
        [IO.Directory]::CreateDirectory($secondPlanDirectory) | Out-Null
        if ($probeReuse) {
            $firstPlan = Join-Path $firstPlanDirectory 'scenario-plan.json'
            $firstProbePlan = Join-Path $firstPlanDirectory 'probe-plan.json'
            $secondPlan = Join-Path $secondPlanDirectory 'scenario-plan.json'
            $secondProbePlan = Join-Path $secondPlanDirectory 'probe-plan.json'
            foreach ($pair in @(
                [pscustomobject]@{ Scenario = $firstPlan; Probe = $firstProbePlan },
                [pscustomobject]@{ Scenario = $secondPlan; Probe = $secondProbePlan }
            )) {
                $compileProbe = & $liveBinary compile-scenario-probe-case --probe-catalog-directory $scenarioProbeCaseCatalog --probe-case-id $options.ProbeCaseId --scenario-catalog-directory $scenarioCaseCatalog --output-scenario-plan $pair.Scenario --output-probe-plan $pair.Probe 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "Scenario transition reuse plan compilation failed before launch: $($compileProbe -join "`n")"
                }
            }
        } else {
            $firstPlan = New-JavaPlayCompiledScenarioPlan $firstPlanDirectory $options
            $secondPlan = New-JavaPlayCompiledScenarioPlan $secondPlanDirectory $options
        }
        $firstReport = Join-Path $reuseDirectory 'first-root.json'
        $resetReport = Join-Path $reuseDirectory 'reset.json'
        $secondReport = Join-Path $reuseDirectory 'second-root.json'
        $reuseReport = Join-Path $reuseDirectory 'reuse-report.json'
        $lifecycle = Join-Path $reuseDirectory 'lifecycle.json'
        if ($probeReuse) {
            $firstProbeReport = Join-Path $reuseDirectory 'first-probe.json'
            $secondProbeReport = Join-Path $reuseDirectory 'second-probe.json'
            $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'constructed_scenario_transition_reuse_probe' -Profile $options.Profile -ControlProfile 'parity_scenario_control' -EvidencePath $reuseReport -LifecyclePath $lifecycle -Body {
                param($launch, $view)
                $result = & $liveBinary run-scenario-transition-reuse-probe --output-dir $liveOutput --first-probe-plan $firstProbePlan --second-probe-plan $secondProbePlan --expected-launch-plan $launch.Plan.Path --first-root-report-path $firstReport --first-probe-report-path $firstProbeReport --reset-report-path $resetReport --second-root-report-path $secondReport --second-probe-report-path $secondProbeReport --report-path $reuseReport 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "Constructed scenario transition reuse probe failed: $($result -join "`n")"
                }
                $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
                if (-not $value.passed -or -not $value.evidence_scope.original_game_transitions_captured -or -not $value.evidence_scope.reset_boundary_verified -or -not $value.evidence_scope.shared_runtime_reuse -or $value.evidence_scope.fresh_runtime_per_member -or $value.evidence_scope.simulator_comparison) {
                    throw "Constructed scenario transition reuse probe produced an invalid scope: $reuseReport"
                }
                return $value
            }
            $workflows = [Collections.Generic.List[string]]::new()
            foreach ($probeReport in @($firstProbeReport, $secondProbeReport)) {
                $label = if ($probeReport -eq $firstProbeReport) { 'first' } else { 'second' }
                $comparison = Join-Path $reuseDirectory "${label}-comparison.json"
                $workflow = Join-Path $reuseDirectory "${label}-workflow.json"
                $compareResult = & $compareBinary --recompare-constructed-probe $probeReport --output $comparison --workflow-output $workflow 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "Constructed transition reuse comparison failed for $label capture; retained output=$reuseDirectory detail=$($compareResult -join "`n")"
                }
                $workflowValue = Get-Content -LiteralPath $workflow -Raw | ConvertFrom-Json -Depth 100
                if (-not $workflowValue.passed -or $workflowValue.provenance -ne 'constructed_validated' -or -not $workflowValue.evidence_scope.simulator_comparison -or $workflowValue.session_id -ne $parsed.session_id) {
                    throw "Constructed transition reuse comparison produced an invalid workflow: $workflow"
                }
                $workflows.Add([IO.Path]::GetFullPath($workflow))
            }
            Write-Output "Constructed scenario transition reuse probe passed probe=$($parsed.probe_case_id) session=$($parsed.session_id) frames=$($parsed.first_root_frame_id)->$($parsed.first_successor_frame_id)->$($parsed.reset_successor_frame_id)->$($parsed.second_root_frame_id)->$($parsed.second_successor_frame_id) comparisons=2 after_ready_ms=$($parsed.timings_ms.total_after_session_ready_ms)"
            Write-Output "reuse_report=$([IO.Path]::GetFullPath($reuseReport)) first_workflow=$($workflows[0]) second_workflow=$($workflows[1]) lifecycle=$([IO.Path]::GetFullPath($lifecycle)) provenance=$($parsed.provenance)"
            return
        }
        $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'constructed_scenario_reuse_probe' -Profile $options.Profile -ControlProfile 'parity_scenario_control' -EvidencePath $reuseReport -LifecyclePath $lifecycle -Body {
            param($launch, $view)
            $result = & $liveBinary run-scenario-reuse-probe --output-dir $liveOutput --first-scenario-plan $firstPlan --second-scenario-plan $secondPlan --expected-launch-plan $launch.Plan.Path --first-report-path $firstReport --reset-report-path $resetReport --second-report-path $secondReport --report-path $reuseReport 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Constructed scenario reuse probe failed: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed -or -not $value.evidence_scope.reset_boundary_verified -or -not $value.evidence_scope.shared_runtime_reuse -or $value.evidence_scope.fresh_runtime_per_member) {
                throw "Constructed scenario reuse probe produced an invalid scope: $reuseReport"
            }
            return $value
        }
        Write-Output "Constructed scenario reuse probe passed case=$($parsed.case_id) session=$($parsed.session_id) frames=$($parsed.first_root_frame_id)->$($parsed.reset_successor_frame_id)->$($parsed.second_root_frame_id) after_ready_ms=$($parsed.timings_ms.total_after_session_ready_ms)"
        Write-Output "reuse_report=$([IO.Path]::GetFullPath($reuseReport)) lifecycle=$([IO.Path]::GetFullPath($lifecycle)) provenance=$($parsed.provenance)"
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'suite') {
        if ($CommandArguments.Count -ne 3 -or $CommandArguments[1] -ne '--suite' -or [string]::IsNullOrWhiteSpace($CommandArguments[2])) {
            throw 'Scenario suite command must be exactly: scenario suite --suite <exact-suite-id>.'
        }
        Assert-LiveBridgeFresh
        Assert-JavaParityComparatorFresh
        Assert-JavaParitySuiteFresh
        Assert-GameClosed
        [IO.Directory]::CreateDirectory($scenarioSuiteOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $suiteId = $CommandArguments[2]
        $suiteDirectory = Join-Path $scenarioSuiteOutput "suite-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($suiteDirectory) | Out-Null
        $suitePlan = Join-Path $suiteDirectory 'suite-plan.json'
        $compileSuite = & $suiteBinary compile --suite-catalog-directory $scenarioSuiteCatalog --suite-id $suiteId --probe-case-catalog-directory $scenarioProbeCaseCatalog --scenario-case-catalog-directory $scenarioCaseCatalog --output $suitePlan 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Scenario suite compilation failed before launching any member: $($compileSuite -join "`n")"
        }
        $plan = Get-Content -LiteralPath $suitePlan -Raw | ConvertFrom-Json -Depth 100
        $memberWorkflows = [System.Collections.Generic.List[string]]::new()
        for ($index = 0; $index -lt $plan.members.Count; $index++) {
            $member = $plan.members[$index]
            $memberDirectory = Join-Path $suiteDirectory ("member-{0:D2}-{1}-{2}" -f $index,$member.probe_case_id,$member.launch_profile)
            $memberOptions = [pscustomobject]@{
                DraftPath = $null
                CaseId = $null
                ProbeCaseId = [string] $member.probe_case_id
                Profile = [string] $member.launch_profile
            }
            $result = Invoke-JavaPlayScenarioTransitionCase $memberOptions $true $memberDirectory
            $memberWorkflows.Add($result.WorkflowReport)
            Write-Output "Constructed suite member passed index=$index probe_case=$($member.probe_case_id) scenario_case=$($member.case_id) profile=$($member.launch_profile) session=$($result.Parsed.session.session_id)"
        }
        $suiteReport = Join-Path $suiteDirectory 'suite-report.json'
        $bundleArguments = [System.Collections.Generic.List[string]]::new()
        $bundleArguments.Add('bundle')
        $bundleArguments.Add('--plan')
        $bundleArguments.Add([IO.Path]::GetFullPath($suitePlan))
        foreach ($workflowPath in $memberWorkflows) {
            $bundleArguments.Add('--member-workflow')
            $bundleArguments.Add($workflowPath)
        }
        $bundleArguments.Add('--output')
        $bundleArguments.Add([IO.Path]::GetFullPath($suiteReport))
        $bundleResult = & $suiteBinary @bundleArguments 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Scenario suite binding failed after member execution: $($bundleResult -join "`n")"
        }
        $suite = Get-Content -LiteralPath $suiteReport -Raw | ConvertFrom-Json -Depth 100
        if (-not $suite.passed -or $suite.provenance -ne 'constructed_validated' -or -not $suite.evidence_scope.fresh_runtime_per_member -or $suite.evidence_scope.shared_runtime_reuse) {
            throw "Scenario suite did not produce a passing fresh-runtime report: $suiteReport"
        }
        Write-Output "Constructed scenario suite passed suite=$($suite.suite_id) members=$($suite.members.Count) fresh_runtime_per_member=$($suite.evidence_scope.fresh_runtime_per_member)"
        Write-Output "suite_plan=$([IO.Path]::GetFullPath($suitePlan)) suite_report=$([IO.Path]::GetFullPath($suiteReport)) provenance=$($suite.provenance)"
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'shared-suite') {
        if ($CommandArguments.Count -ne 3 -or $CommandArguments[1] -ne '--suite' -or [string]::IsNullOrWhiteSpace($CommandArguments[2])) {
            throw 'Shared-runtime suite must be exactly: scenario shared-suite --suite <exact-suite-id>.'
        }
        Assert-LiveBridgeFresh
        Assert-JavaParitySuiteFresh
        Assert-GameClosed
        [IO.Directory]::CreateDirectory($scenarioSuiteOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $suiteId = $CommandArguments[2]
        $suiteDirectory = Join-Path $scenarioSuiteOutput "shared-suite-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($suiteDirectory) | Out-Null
        $suitePlan = Join-Path $suiteDirectory 'suite-plan.json'
        $compileSuite = & $suiteBinary compile-shared --suite-catalog-directory $scenarioSharedRuntimeSuiteCatalog --suite-id $suiteId --probe-case-catalog-directory $scenarioProbeCaseCatalog --scenario-case-catalog-directory $scenarioCaseCatalog --output $suitePlan 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Shared-runtime suite compilation failed before launch: $($compileSuite -join "`n")"
        }
        $executionPlan = Join-Path $suiteDirectory 'execution-plan.json'
        $memberPlanDirectory = Join-Path $suiteDirectory 'member-plans'
        $prepare = & $suiteBinary prepare-shared --plan $suitePlan --member-plan-directory $memberPlanDirectory --output $executionPlan 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Shared-runtime execution preparation failed before launch: $($prepare -join "`n")"
        }
        $execution = Get-Content -LiteralPath $executionPlan -Raw | ConvertFrom-Json -Depth 100
        $captureDirectory = Join-Path $suiteDirectory 'captures'
        $captureReport = Join-Path $suiteDirectory 'capture-report.json'
        $lifecycle = Join-Path $suiteDirectory 'lifecycle.json'
        $capture = Invoke-JavaPlayIsolatedCase -CaseKind 'constructed_shared_runtime_suite_capture' -Profile $execution.launch_profile -ControlProfile 'parity_scenario_control' -EvidencePath $captureReport -LifecyclePath $lifecycle -Body {
            param($launch, $view)
            $result = & $suiteBinary capture-shared --execution-plan $executionPlan --capture-directory $captureDirectory --live-output-directory $liveOutput --expected-launch-plan $launch.Plan.Path --output $captureReport 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Shared-runtime suite capture failed: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed -or $value.evidence_scope.simulator_comparison -or -not $value.evidence_scope.shared_runtime_reuse -or $value.evidence_scope.captured_members -ne $value.evidence_scope.expected_members -or $value.evidence_scope.captured_resets -ne $value.evidence_scope.expected_resets) {
                throw "Shared-runtime suite capture produced an invalid scope: $captureReport"
            }
            return $value
        }
        $comparisonDirectory = Join-Path $suiteDirectory 'verified-comparisons'
        $suiteReport = Join-Path $suiteDirectory 'shared-suite-report.json'
        $verify = & $suiteBinary verify-shared --execution-plan $executionPlan --capture-report $captureReport --lifecycle-receipt $lifecycle --comparison-directory $comparisonDirectory --output $suiteReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Shared-runtime suite verification failed after capture; retained output=$suiteDirectory detail=$($verify -join "`n")"
        }
        $suite = Get-Content -LiteralPath $suiteReport -Raw | ConvertFrom-Json -Depth 100
        if (-not $suite.passed -or $suite.provenance -ne 'constructed_validated' -or -not $suite.evidence_scope.simulator_comparison -or -not $suite.evidence_scope.all_members_constructed_and_matched -or -not $suite.evidence_scope.all_inter_member_resets_verified -or -not $suite.evidence_scope.managed_lifecycle_verified -or $suite.evidence_scope.fresh_runtime_per_member -or -not $suite.evidence_scope.shared_runtime_reuse) {
            throw "Shared-runtime suite did not produce a passing topology-bound report: $suiteReport"
        }
        Write-Output "Shared-runtime suite passed suite=$($suite.suite_id) members=$($suite.members.Count) resets=$($suite.reset_reports.Count) session=$($suite.session_id) capture_after_ready_ms=$($suite.capture_after_session_ready_ms) lifecycle_total_ms=$($suite.lifecycle_total_ms)"
        Write-Output "suite_plan=$([IO.Path]::GetFullPath($suitePlan)) execution_plan=$([IO.Path]::GetFullPath($executionPlan)) capture_report=$([IO.Path]::GetFullPath($captureReport)) lifecycle=$([IO.Path]::GetFullPath($lifecycle)) suite_report=$([IO.Path]::GetFullPath($suiteReport)) provenance=$($suite.provenance)"
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'shared-trajectory-suite') {
        if ($CommandArguments.Count -ne 3 -or $CommandArguments[1] -ne '--suite' -or [string]::IsNullOrWhiteSpace($CommandArguments[2])) {
            throw 'Shared-runtime trajectory suite must be exactly: scenario shared-trajectory-suite --suite <exact-suite-id>.'
        }
        Assert-LiveBridgeFresh
        Assert-JavaParitySuiteFresh
        Assert-GameClosed
        [IO.Directory]::CreateDirectory($scenarioSuiteOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $suiteId = $CommandArguments[2]
        $suiteDirectory = Join-Path $scenarioSuiteOutput "shared-trajectory-suite-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($suiteDirectory) | Out-Null
        $suitePlan = Join-Path $suiteDirectory 'suite-plan.json'
        $compileSuite = & $suiteBinary compile-shared-trajectory --suite-catalog-directory $scenarioSharedRuntimeTrajectorySuiteCatalog --suite-id $suiteId --trajectory-case-catalog-directory $scenarioTrajectoryCaseCatalog --scenario-case-catalog-directory $scenarioCaseCatalog --output $suitePlan 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Shared-runtime trajectory suite compilation failed before launch: $($compileSuite -join "`n")"
        }
        $executionPlan = Join-Path $suiteDirectory 'execution-plan.json'
        $memberPlanDirectory = Join-Path $suiteDirectory 'member-plans'
        $prepare = & $suiteBinary prepare-shared-trajectory --plan $suitePlan --member-plan-directory $memberPlanDirectory --output $executionPlan 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Shared-runtime trajectory execution preparation failed before launch: $($prepare -join "`n")"
        }
        $execution = Get-Content -LiteralPath $executionPlan -Raw | ConvertFrom-Json -Depth 100
        $captureDirectory = Join-Path $suiteDirectory 'captures'
        $captureReport = Join-Path $suiteDirectory 'capture-report.json'
        $lifecycle = Join-Path $suiteDirectory 'lifecycle.json'
        $capture = Invoke-JavaPlayIsolatedCase -CaseKind 'constructed_shared_runtime_trajectory_suite_capture' -Profile $execution.launch_profile -ControlProfile 'parity_scenario_control' -EvidencePath $captureReport -LifecyclePath $lifecycle -Body {
            param($launch, $view)
            $result = & $suiteBinary capture-shared-trajectory --execution-plan $executionPlan --capture-directory $captureDirectory --live-output-directory $liveOutput --expected-launch-plan $launch.Plan.Path --output $captureReport 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Shared-runtime trajectory suite capture failed: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed -or $value.evidence_scope.simulator_comparison -or -not $value.evidence_scope.shared_runtime_reuse -or $value.evidence_scope.captured_members -ne $value.evidence_scope.expected_members -or $value.evidence_scope.captured_resets -ne $value.evidence_scope.expected_resets) {
                throw "Shared-runtime trajectory suite capture produced an invalid scope: $captureReport"
            }
            return $value
        }
        $comparisonDirectory = Join-Path $suiteDirectory 'verified-comparisons'
        $suiteReport = Join-Path $suiteDirectory 'shared-trajectory-suite-report.json'
        $verify = & $suiteBinary verify-shared-trajectory --execution-plan $executionPlan --capture-report $captureReport --lifecycle-receipt $lifecycle --comparison-directory $comparisonDirectory --output $suiteReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Shared-runtime trajectory suite verification failed after capture; retained output=$suiteDirectory detail=$($verify -join "`n")"
        }
        $suite = Get-Content -LiteralPath $suiteReport -Raw | ConvertFrom-Json -Depth 100
        if (-not $suite.passed -or $suite.provenance -ne 'constructed_validated' -or -not $suite.evidence_scope.simulator_comparison -or -not $suite.evidence_scope.all_members_constructed_and_matched -or -not $suite.evidence_scope.all_inter_member_resets_verified -or -not $suite.evidence_scope.managed_lifecycle_verified -or $suite.evidence_scope.fresh_runtime_per_member -or -not $suite.evidence_scope.shared_runtime_reuse -or $suite.evidence_scope.training_eligible) {
            throw "Shared-runtime trajectory suite did not produce a passing topology-bound report: $suiteReport"
        }
        Write-Output "Shared-runtime trajectory suite passed suite=$($suite.suite_id) members=$($suite.members.Count) resets=$($suite.reset_reports.Count) session=$($suite.session_id) capture_after_ready_ms=$($suite.capture_after_session_ready_ms) lifecycle_total_ms=$($suite.lifecycle_total_ms)"
        Write-Output "suite_plan=$([IO.Path]::GetFullPath($suitePlan)) execution_plan=$([IO.Path]::GetFullPath($executionPlan)) capture_report=$([IO.Path]::GetFullPath($captureReport)) lifecycle=$([IO.Path]::GetFullPath($lifecycle)) suite_report=$([IO.Path]::GetFullPath($suiteReport)) provenance=$($suite.provenance)"
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'trajectory') {
        $options = Resolve-JavaPlayScenarioTrajectoryOptions @($CommandArguments | Select-Object -Skip 1)
        Assert-LiveBridgeFresh
        Assert-JavaParityComparatorFresh
        [IO.Directory]::CreateDirectory($scenarioOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $scenarioDirectory = Join-Path $scenarioOutput "trajectory-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        $result = Invoke-JavaPlayScenarioTrajectoryCase $options $scenarioDirectory
        $parsed = $result.Parsed
        if ($result.ComparisonMode -eq 'selected_steps') {
            Write-Output "Constructed combat trajectory selected-step comparison passed session=$($parsed.session.session_id) selected=$($result.Comparison.requested_step_indices -join ',') captured=$($result.Comparison.captured_steps) matched=$($result.Comparison.completed_steps) all_steps_matched=$($result.Comparison.evidence_scope.all_steps_constructed_and_matched)"
            Write-Output "scenario_plan=$($result.ScenarioPlan) trajectory_plan=$($result.TrajectoryPlan) scenario_report=$($result.ScenarioReport) trajectory_report=$($result.TrajectoryReport) comparisons=$($result.ComparisonDirectory) attempt=$($result.ComparisonReport) lifecycle=$($result.Lifecycle) provenance=$($parsed.provenance)"
        } else {
            Write-Output "Constructed combat trajectory verified session=$($parsed.session.session_id) steps=$($parsed.completed_steps)/$($parsed.planned_steps) frames=$($result.Comparison.root_frame_id)->$($result.Comparison.final_frame_id) strict_runtime=$($result.Comparison.evidence_scope.strict_runtime_profile)"
            Write-Output "scenario_plan=$($result.ScenarioPlan) trajectory_plan=$($result.TrajectoryPlan) scenario_report=$($result.ScenarioReport) trajectory_report=$($result.TrajectoryReport) comparisons=$($result.ComparisonDirectory) workflow=$($result.ComparisonReport) lifecycle=$($result.Lifecycle) provenance=$($parsed.provenance)"
        }
        return
    }
    if ($CommandArguments.Count -lt 3 -or $CommandArguments[0] -notin @('create-combat','probe','verify')) {
        throw 'Scenario command must be `scenario registry`, `scenario cases`, `scenario probe-cases`, `scenario trajectories`, `scenario suites`, `scenario shared-suites`, `scenario shared-trajectory-suites`, `scenario create-combat (...)`, `scenario probe --probe-case <id>`, `scenario trajectory --trajectory-case <id> [--step-index <index>]...`, `scenario verify --probe-case <id>`, `scenario recompare --probe-report <path>`, `scenario recompare-trajectory --trajectory-report <path> --step-index <index>`, `scenario reuse-probe (...)`, `scenario suite --suite <exact-suite-id>`, `scenario shared-suite --suite <exact-suite-id>`, or `scenario shared-trajectory-suite --suite <exact-suite-id>`.'
    }
    $hasTransition = $CommandArguments[0] -in @('probe','verify')
    $runComparison = $CommandArguments[0] -eq 'verify'
    $options = Resolve-JavaPlayScenarioRunOptions @($CommandArguments | Select-Object -Skip 1) $hasTransition

    Assert-LiveBridgeFresh
    if ($runComparison) {
        Assert-JavaParityComparatorFresh
    }
    [IO.Directory]::CreateDirectory($scenarioOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $prefix = if ($runComparison) { 'verify' } elseif ($hasTransition) { 'probe' } else { 'combat' }
    $scenarioDirectory = Join-Path $scenarioOutput "$prefix-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    if ($hasTransition) {
        $result = Invoke-JavaPlayScenarioTransitionCase $options $runComparison $scenarioDirectory
        $parsed = $result.Parsed
        if ($runComparison) {
            $workflow = $result.Workflow
            Write-Output "Constructed combat verification passed session=$($parsed.session.session_id) transition=$($parsed.transition_id) frames=$($workflow.from_frame_id)->$($workflow.to_frame_id) launch_profile=$($workflow.launch_profile) strict_runtime=$($workflow.evidence_scope.strict_runtime_profile)"
            Write-Output "scenario_plan=$($result.ScenarioPlan) probe_plan=$($result.ProbePlan) scenario_report=$($result.ScenarioReport) probe_report=$($result.ProbeReport) comparison_report=$($result.ComparisonReport) workflow=$($result.WorkflowReport) action_receipt=$($parsed.action.receipt_path) lifecycle=$($result.Lifecycle) provenance=$($parsed.provenance)"
            return
        }
        Write-Output "Constructed combat probe passed session=$($parsed.session.session_id) transition=$($parsed.transition_id) successor_frame=$($parsed.successor.frame_id)"
        Write-Output "scenario_plan=$($result.ScenarioPlan) probe_plan=$($result.ProbePlan) scenario_report=$($result.ScenarioReport) probe_report=$($result.ProbeReport) action_receipt=$($parsed.action.receipt_path) lifecycle=$($result.Lifecycle) provenance=$($parsed.provenance)"
        return
    }
    Assert-GameClosed
    [IO.Directory]::CreateDirectory($scenarioDirectory) | Out-Null
    $scenarioPlan = New-JavaPlayCompiledScenarioPlan $scenarioDirectory $options
    $report = Join-Path $scenarioDirectory 'report.json'
    $lifecycle = Join-Path $scenarioDirectory 'lifecycle.json'
    $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'constructed_combat_scenario' -Profile $options.Profile -ControlProfile 'parity_scenario_control' -EvidencePath $report -LifecyclePath $lifecycle -Body {
        param($launch, $view)
        $result = & $liveBinary run-scenario --output-dir $liveOutput --scenario-plan $scenarioPlan --expected-launch-plan $launch.Plan.Path --report-path $report 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Constructed combat scenario failed: $($result -join "`n")"
        }
        $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
        if (-not $value.passed -or $value.provenance -ne 'constructed_validated') {
            throw "Constructed combat scenario did not produce a passing typed report: $report"
        }
        return $value
    }
    Write-Output "Constructed combat scenario passed session=$($parsed.session.session_id) root_frame=$($parsed.root.frame_id)"
    Write-Output "scenario_plan=$scenarioPlan report=$([IO.Path]::GetFullPath($report)) lifecycle=$([IO.Path]::GetFullPath($lifecycle)) provenance=$($parsed.provenance)"
}

function Invoke-JavaPlayDoctor {
    if (Test-Path -LiteralPath $liveBinary -PathType Leaf) {
        try {
            Assert-LiveBridgeFresh
            Write-Output "live_bridge=fresh path=$liveBinary"
        } catch {
            Write-Output "live_bridge=stale detail=$($_.Exception.Message)"
        }
    } else {
        Write-Output "live_bridge=missing path=$liveBinary"
    }
    foreach ($owner in @(
        [pscustomobject]@{ Name = 'offline_comparator'; Path = $compareBinary; Check = { Assert-JavaParityComparatorFresh } },
        [pscustomobject]@{ Name = 'suite_owner'; Path = $suiteBinary; Check = { Assert-JavaParitySuiteFresh } }
    )) {
        if (Test-Path -LiteralPath $owner.Path -PathType Leaf) {
            try {
                & $owner.Check
                Write-Output "$($owner.Name)=fresh path=$($owner.Path)"
            } catch {
                Write-Output "$($owner.Name)=stale detail=$($_.Exception.Message)"
            }
        } else {
            Write-Output "$($owner.Name)=missing path=$($owner.Path)"
        }
    }
    Push-Location $communicationRepository
    try {
        & $communicationWrapper identity
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    } finally {
        Pop-Location
    }
    Write-Output "config=$communicationConfig"
    if (Test-Path -LiteralPath $communicationConfig -PathType Leaf) {
        Get-Content -LiteralPath $communicationConfig | Where-Object {
            $_ -match '^(command|runAtGameStart)='
        }
    }
    try {
        $fastIdentity = Get-JavaPlayModIdentity $superFastModeJar
        $fastSettings = Get-SuperFastModeSettings
        Write-Output "accelerated_profile=ready mod_version=$($fastIdentity.version) jar_sha256=$($fastIdentity.jar.sha256) delta_multiplier=$($fastSettings.values.delta_multiplier) instant_lerp=$($fastSettings.values.is_instant_lerp)"
        Write-Output "accelerated_config=$($fastSettings.config.path) config_sha256=$($fastSettings.config.sha256)"
    } catch {
        Write-Output "accelerated_profile=unavailable detail=$($_.Exception.Message)"
    }
    Get-Process -Name 'SlayTheSpire','ModTheSpire','java','javaw','sts_java_parity_live' -ErrorAction SilentlyContinue |
        Select-Object Id,ProcessName,MainWindowTitle
}

function Invoke-JavaPlayMirrorCheckpointBatch {
    param(
        [Parameter(Mandatory)] [string] $CheckpointArgument,
        [Parameter(Mandatory)] [string[]] $PlanArguments,
        [Parameter(Mandatory)] [string] $InvocationMode
    )
    Assert-GameClosed
    $checkpoint = Read-JavaPlayNativeCheckpoint $CheckpointArgument
    $resolvedPlans = [Collections.Generic.List[string]]::new()
    foreach ($planArgument in $PlanArguments) {
        $resolvedPlans.Add($(if ([IO.Path]::IsPathRooted($planArgument)) {
            [IO.Path]::GetFullPath($planArgument)
        } else {
            [IO.Path]::GetFullPath((Join-Path $repository $planArgument))
        }))
    }

    [IO.Directory]::CreateDirectory($probeOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $runDirectory = Join-Path $probeOutput "mirror-batch-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
    $batchPlanPath = Join-Path $runDirectory 'batch-plan.json'
    $aggregateReport = Join-Path $runDirectory 'batch-execution.json'
    $lifecycle = Join-Path $runDirectory 'lifecycle.json'
    $compileArguments = [Collections.Generic.List[string]]::new()
    foreach ($argument in @(
        'compile-mirror-suffix-batch',
        '--checkpoint-manifest',
        $checkpoint.ManifestPath
    )) {
        $compileArguments.Add($argument)
    }
    foreach ($planPath in $resolvedPlans) {
        $compileArguments.Add('--mirror-plan')
        $compileArguments.Add($planPath)
    }
    $compileArguments.Add('--output-plan')
    $compileArguments.Add($batchPlanPath)
    $compileRaw = & $liveBinary @($compileArguments.ToArray()) 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Mirror checkpoint batch preflight failed before Java launch: $($compileRaw -join "`n")"
    }
    $batch = ($compileRaw -join "`n") | ConvertFrom-Json -Depth 100
    if ($batch.schema -ne 'sts-java-parity-mirror-suffix-batch-plan-v1' -or
        $batch.provenance -ne 'natural_checkpoint_batch_plan' -or
        @($batch.members).Count -ne $resolvedPlans.Count -or
        [string]$batch.checkpoint_manifest.sha256 -ne [string]$checkpoint.ManifestIdentity.sha256 -or
        $batch.evidence_scope.continuous_from_neow -or
        $batch.evidence_scope.simulator_comparison -or
        $batch.evidence_scope.strategy_evidence -or
        $batch.evidence_scope.training_eligible) {
        throw "Mirror checkpoint batch compiler returned an invalid closed plan: $batchPlanPath"
    }

    $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'natural_mirror_checkpoint_batch' -Profile 'accelerated' -ControlProfile 'parity_run_control' -EvidencePath $aggregateReport -LifecyclePath $lifecycle -StorageSeed $checkpoint -Body {
        param($launch, $view)
        $batchStartedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $checkpointAutosave = [IO.Path]::GetFullPath([string]$checkpoint.Manifest.autosave.path)
        $relativeAutosave = [IO.Path]::GetRelativePath($checkpoint.StorageRoot, $checkpointAutosave)
        if ([IO.Path]::IsPathRooted($relativeAutosave) -or
            $relativeAutosave -eq '..' -or
            $relativeAutosave.StartsWith("..$([IO.Path]::DirectorySeparatorChar)")) {
            throw 'Checkpoint autosave is outside its validated storage root.'
        }
        $workingAutosave = [IO.Path]::GetFullPath((Join-Path $launch.Storage.WorkingDirectory $relativeAutosave))
        $workingRoot = [IO.Path]::GetFullPath([string]$launch.Storage.WorkingDirectory)
        if (-not $workingAutosave.StartsWith("$workingRoot$([IO.Path]::DirectorySeparatorChar)", [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Resolved batch autosave escaped the isolated working directory.'
        }
        $autosaveSnapshots = [Collections.Generic.List[object]]::new()
        $captureAutosave = {
            param([string] $Stage, [object] $MemberId)
            $identity = Get-JavaPlayFileIdentity $workingAutosave
            return [ordered]@{
                stage = $Stage
                member_id = $MemberId
                relative_path = $relativeAutosave
                length = [long]$identity.length
                sha256 = [string]$identity.sha256
            }
        }
        $initialAutosave = & $captureAutosave 'initial' $null
        $autosaveSnapshots.Add($initialAutosave)

        $memberResults = [Collections.Generic.List[object]]::new()
        $expectedSessionId = $null
        $expectedConnectionId = $null
        $previousResetSuccessor = $null
        $members = @($batch.members)
        for ($index = 0; $index -lt $members.Count; $index++) {
            $member = $members[$index]
            $memberId = [string]$member.member_id
            $memberPlan = Get-Content -LiteralPath ([string]$member.mirror_plan.path) -Raw | ConvertFrom-Json -Depth 100
            $expectsSuccessorCombatProjection = (
                $null -ne $memberPlan.scope.PSObject.Properties['expected_successor_scalars'] -and
                $null -ne $memberPlan.scope.PSObject.Properties['expected_successor_combat']
            )
            $executionPath = Join-Path $runDirectory "$memberId-execution.json"
            $executionRaw = & $liveBinary run-mirror-suffix --output-dir $liveOutput --mirror-plan ([string]$member.mirror_plan.path) --checkpoint-manifest $checkpoint.ManifestPath --expected-launch-plan $launch.Plan.Path --report-path $executionPath 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Mirror batch member $memberId stopped: $($executionRaw -join "`n")"
            }
            $execution = ($executionRaw -join "`n") | ConvertFrom-Json -Depth 100
            $steps = @($execution.steps)
            if (-not $execution.passed -or
                -not $execution.evidence_scope.native_checkpoint_restored -or
                -not $execution.evidence_scope.checkpoint_suffix_captured -or
                $execution.evidence_scope.continuous_from_neow -or
                $execution.evidence_scope.simulator_comparison -or
                $execution.evidence_scope.strategy_evidence -or
                $execution.evidence_scope.training_eligible -or
                [string]$execution.plan_sha256 -ne [string]$member.mirror_plan.sha256 -or
                [string]$execution.resume.checkpoint_manifest_sha256 -ne [string]$checkpoint.ManifestIdentity.sha256 -or
                [int]$execution.resume.source_depth -ne [int]$batch.source_depth -or
                [int]$execution.planned_steps -ne [int]$member.suffix_steps -or
                [int]$execution.completed_plan_steps -ne [int]$member.suffix_steps -or
                ($expectsSuccessorCombatProjection -and
                    -not $execution.evidence_scope.successor_combat_projection_compared) -or
                $steps.Count -lt 2) {
                throw "Mirror batch member returned invalid scoped evidence: $executionPath"
            }
            if ($null -eq $expectedSessionId) {
                $expectedSessionId = [string]$execution.session.session_id
                $expectedConnectionId = [string]$execution.session.connection_id
            }
            if ([string]$execution.session.session_id -ne $expectedSessionId -or
                [string]$execution.session.connection_id -ne $expectedConnectionId -or
                ($null -ne $previousResetSuccessor -and
                    [long]$steps[0].frame_id -ne [long]$previousResetSuccessor)) {
                throw "Mirror batch member $memberId did not continue the same reset-bound runtime."
            }
            $afterMember = & $captureAutosave 'after_member' $memberId
            $autosaveSnapshots.Add($afterMember)
            if ($afterMember.length -ne $initialAutosave.length -or
                $afterMember.sha256 -ne $initialAutosave.sha256) {
                throw "Mirror batch member $memberId changed the native autosave."
            }

            $resetIdentity = $null
            $resetTiming = $null
            $endpointFrameId = [long]$steps[-1].successor_frame_id
            if ($index + 1 -lt $members.Count) {
                $resetPath = Join-Path $runDirectory "$memberId-reset.json"
                $resetRaw = & $liveBinary reset-natural-run --output-dir $liveOutput --report-path $resetPath 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "Mirror batch reset after $memberId failed: $($resetRaw -join "`n")"
                }
                $reset = ($resetRaw -join "`n") | ConvertFrom-Json -Depth 100
                if (-not $reset.passed -or
                    -not $reset.preserved_save_main_menu_verified -or
                    $reset.provenance -ne 'natural_native_checkpoint' -or
                    [string]$reset.session_id -ne $expectedSessionId -or
                    [string]$reset.connection_id -ne $expectedConnectionId -or
                    [long]$reset.from_frame_id -ne $endpointFrameId) {
                    throw "Mirror batch reset after $memberId returned invalid evidence: $resetPath"
                }
                $previousResetSuccessor = [long]$reset.successor_frame_id
                $resetIdentity = Get-JavaPlayFileIdentity $resetPath
                $resetTiming = [long]$reset.completed_at_unix_ms - [long]$reset.created_at_unix_ms
                $afterReset = & $captureAutosave 'after_reset' $memberId
                $autosaveSnapshots.Add($afterReset)
                if ($afterReset.length -ne $initialAutosave.length -or
                    $afterReset.sha256 -ne $initialAutosave.sha256) {
                    throw "Mirror batch reset after $memberId changed the native autosave."
                }
            }

            $memberResults.Add([ordered]@{
                member_id = $memberId
                mirror_plan = $member.mirror_plan
                suffix_steps = [int]$member.suffix_steps
                execution = Get-JavaPlayFileIdentity $executionPath
                endpoint_frame_id = $endpointFrameId
                successor_combat_projection_compared = [bool]$execution.evidence_scope.successor_combat_projection_compared
                reset_after = $resetIdentity
                timings_ms = [ordered]@{
                    suffix = [long]$execution.completed_at_unix_ms - [long]$execution.created_at_unix_ms
                    reset_after = $resetTiming
                }
            })
        }

        $allSuccessorCombatProjectionsCompared = @(
            $memberResults.ToArray() | Where-Object { -not $_.successor_combat_projection_compared }
        ).Count -eq 0

        $aggregate = [ordered]@{
            schema = 'sts-java-parity-native-checkpoint-batch-execution-v1'
            provenance = 'natural_checkpoint_batch'
            invocation_mode = $InvocationMode
            created_at_unix_ms = $batchStartedAt
            completed_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
            passed = $true
            batch_plan = Get-JavaPlayFileIdentity $batchPlanPath
            checkpoint = $checkpoint.ManifestIdentity
            session = [ordered]@{
                session_id = $expectedSessionId
                connection_id = $expectedConnectionId
            }
            members = $memberResults.ToArray()
            autosave = [ordered]@{
                unchanged = $true
                initial_sha256 = [string]$initialAutosave.sha256
                snapshots = $autosaveSnapshots.ToArray()
            }
            evidence_scope = [ordered]@{
                continuous_from_neow = $false
                original_game_native_autosave = $true
                native_checkpoint_restored = $true
                same_runtime_reused = $true
                successor_combat_projection_compared = $allSuccessorCombatProjectionsCompared
                simulator_comparison = $false
                strategy_evidence = $false
                training_eligible = $false
            }
        }
        Write-JavaPlayJsonCreateNew $aggregateReport $aggregate
        return $aggregate
    }
    $parsedLifecycle = Get-Content -LiteralPath $lifecycle -Raw | ConvertFrom-Json -Depth 100
    $suffixMs = @($parsed.members | ForEach-Object { [long]$_.timings_ms.suffix } | Measure-Object -Sum).Sum
    $resetMs = @($parsed.members | ForEach-Object { if ($null -ne $_.timings_ms.reset_after) { [long]$_.timings_ms.reset_after } } | Measure-Object -Sum).Sum
    Write-Output "Java native checkpoint batch passed session=$($parsed.session.session_id) members=$(@($parsed.members).Count) autosave_unchanged=$($parsed.autosave.unchanged) successor_combat_projection_compared=$($parsed.evidence_scope.successor_combat_projection_compared)"
    Write-Output "timings_ms launch_to_ready=$($parsedLifecycle.timings_ms.launch_to_session_ready) suffix_sum=$suffixMs reset_sum=$resetMs case=$($parsedLifecycle.timings_ms.case_after_session_ready) total=$($parsedLifecycle.timings_ms.total)"
    Write-Output "evidence continuous_from_neow=$($parsed.evidence_scope.continuous_from_neow) simulator_comparison=$($parsed.evidence_scope.simulator_comparison) strategy_evidence=$($parsed.evidence_scope.strategy_evidence) training_eligible=$($parsed.evidence_scope.training_eligible)"
    Write-Output "checkpoint=$($checkpoint.ManifestPath) batch_plan=$([IO.Path]::GetFullPath($batchPlanPath)) execution=$([IO.Path]::GetFullPath($aggregateReport)) lifecycle=$([IO.Path]::GetFullPath($lifecycle))"
}

function Resolve-JavaPlayPairedRootStore([hashtable] $Options) {
    return [IO.Path]::GetFullPath($(if ($Options.ContainsKey('--store')) {
        if ([IO.Path]::IsPathRooted($Options['--store'])) {
            $Options['--store']
        } else {
            Join-Path $repository $Options['--store']
        }
    } else {
        $pairedRootStore
    }))
}

function Get-JavaPlayPairedRootRuntimeSettlement(
    [string] $StorePath,
    [string] $RootId
) {
    if ([string]::IsNullOrWhiteSpace($RootId)) {
        return $null
    }
    if ($RootId -notmatch '^state-root-v2-[0-9a-f]{64}$') {
        throw "Invalid paired-root identity for runtime settlement: $RootId"
    }
    $resolvedStore = [IO.Path]::GetFullPath($StorePath)
    $rootPath = [IO.Path]::GetFullPath((Join-Path $resolvedStore "roots\$RootId.json"))
    if (-not $rootPath.StartsWith($resolvedStore + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        -not (Test-Path -LiteralPath $rootPath -PathType Leaf)) {
        throw "Paired-root runtime settlement root is unavailable: $RootId"
    }
    $artifact = Get-Content -LiteralPath $rootPath -Raw | ConvertFrom-Json -Depth 100
    if ([string]$artifact.schema -ne 'sts-state-root-v2' -or
        [string]$artifact.root_id -ne $RootId) {
        throw "Paired-root runtime settlement artifact is invalid: $RootId"
    }
    $components = @($artifact.root.preflight.components | Where-Object {
        [string]$_.domain -eq 'runtime_settlement'
    })
    if ($components.Count -eq 0) {
        return $null
    }
    if ($components.Count -ne 1 -or
        [string]$components[0].status -ne 'exact' -or
        [string]$components[0].projection_schema -notin @(
            'sts-paired-combat-reward-runtime-settlement-v1',
            'sts-paired-card-reward-runtime-settlement-v1',
            'sts-paired-map-runtime-settlement-v1'
        ) -or
        [string]$components[0].java.sha256 -ne [string]$components[0].rust.sha256) {
        throw "Paired-root runtime settlement projection is not one exact typed contract: $RootId"
    }
    $digest = [string]$components[0].java.sha256
    if ($digest -notmatch '^[0-9a-f]{64}$') {
        throw "Paired-root runtime settlement blob identity is invalid: $RootId"
    }
    $blobPath = [IO.Path]::GetFullPath((Join-Path $resolvedStore "blobs\sha256\$digest"))
    if (-not $blobPath.StartsWith($resolvedStore + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        -not (Test-Path -LiteralPath $blobPath -PathType Leaf)) {
        throw "Paired-root runtime settlement blob is unavailable: $digest"
    }
    $settlement = Get-Content -LiteralPath $blobPath -Raw | ConvertFrom-Json -Depth 20
    if ([string]$settlement.native_autosave -ne 'may_differ_from_native_root' -or
        [string]$settlement.resident_completion -ne 'retire') {
        throw "Paired-root runtime settlement contract is unsupported: $RootId"
    }
    return $settlement
}

function Assert-JavaPlayPairedRootActionObservation(
    [object] $Observed,
    [string] $RootId,
    [string[]] $RequestedActionIds = @(),
    [switch] $RequireVerified
) {
    $entries = @($Observed.actions)
    $handles = @($entries | ForEach-Object { [string]$_.handle.handle_id })
    if ([string]$Observed.schema -ne 'sts-paired-root-action-observation-v2' -or
        [string]$Observed.root_id -ne $RootId -or
        [string]::IsNullOrWhiteSpace([string]$Observed.action_space_id) -or
        @($handles | Sort-Object -Unique).Count -ne $handles.Count) {
        throw "Paired-root action observation is not one valid v2 typed surface for root: $RootId"
    }
    foreach ($entry in $entries) {
        $handleId = [string]$entry.handle.handle_id
        $capability = [string]$entry.capability.capability
        $status = [string]$entry.capability.status.status
        if ($handleId -notmatch '^paired-action-v1-[0-9a-f]{64}$' -or
            [string]::IsNullOrWhiteSpace($capability) -or
            $status -notin @('verified','unknown','unsupported')) {
            throw "Paired-root action observation contains invalid capability evidence: $handleId"
        }
        if (($status -eq 'verified' -and [string]$entry.capability.status.proof.sha256 -notmatch '^[0-9a-f]{64}$') -or
            ($status -eq 'unknown' -and [string]::IsNullOrWhiteSpace([string]$entry.capability.status.gap_id)) -or
            ($status -eq 'unsupported' -and [string]::IsNullOrWhiteSpace([string]$entry.capability.status.reason))) {
            throw "Paired-root action observation contains incomplete $status capability evidence: $handleId"
        }
    }
    foreach ($actionId in $RequestedActionIds) {
        $matches = @($entries | Where-Object { [string]$_.handle.handle_id -eq $actionId })
        if ($matches.Count -ne 1) {
            throw "Paired-root action preflight rejected a handle outside the target root: $actionId"
        }
        if ($RequireVerified -and [string]$matches[0].capability.status.status -ne 'verified') {
            $capability = [string]$matches[0].capability.capability
            $status = [string]$matches[0].capability.status.status
            $detail = if ($status -eq 'unknown') {
                [string]$matches[0].capability.status.gap_id
            } else {
                [string]$matches[0].capability.status.reason
            }
            throw "Paired-root action preflight rejected non-verified capability before native materialization: handle=$actionId capability=$capability status=$status detail=$detail"
        }
    }
}

function Invoke-JavaPlayPairedRootActions([hashtable] $Options) {
    if (-not (Test-Path -LiteralPath $runPlayWorkbench -PathType Leaf)) {
        throw 'Run-play workbench is missing. Run .\run-play.ps1 build first.'
    }
    $storePath = Resolve-JavaPlayPairedRootStore $Options
    [IO.Directory]::CreateDirectory($pairedRootOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $runDirectory = Join-Path $pairedRootOutput "actions-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
    $report = Join-Path $runDirectory 'actions.json'
    $raw = & $runPlayWorkbench paired-root-observe-actions --root $Options['--root'] --store $storePath --report $report 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Paired-root action observation failed: $($raw -join "`n")"
    }
    $observed = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json -Depth 100
    Assert-JavaPlayPairedRootActionObservation $observed ([string]$Options['--root'])
    Write-Output $raw
}

function Invoke-JavaPlayPairedRootResidentBatch(
    [hashtable] $Options,
    [string[]] $ActionIds,
    [string] $StorePath,
    [object] $Observed,
    [object] $Materialized,
    [string] $ActionObservationReport,
    [string] $MaterializationReport,
    [string] $AggregateReport
) {
    $batchStartedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $selection = Find-JavaPlayCompatiblePairedRootResident $StorePath $Materialized
    if ($null -eq $selection) {
        $openArgs = @(
            '--mode','open',
            '--target-root',[string]$Options['--target-root'],
            '--store',$StorePath
        )
        $opened = Invoke-JavaPlayPairedRootResident -CommandArguments $openArgs -PassThru
        $lease = Enter-JavaPlayPairedRootResidentLease $opened.ResidentId 'routine_preview'
        try {
            $resident = Read-JavaPlayPairedRootResident $opened.ResidentId
            $selection = [pscustomobject]@{
                Resident = $resident
                Lease = $lease
                Selection = 'opened'
            }
        } catch {
            Exit-JavaPlayPairedRootResidentLease $lease
            throw
        }
    }
    $runtimeReady = $clock.ElapsedMilliseconds
    $memberResults = [Collections.Generic.List[object]]::new()
    $autosaveSnapshots = [Collections.Generic.List[object]]::new()
    $aggregateStatus = 'ready'
    $stopReason = $null
    $allSourceHeadsUnchanged = $true
    $allAutosavesUnchanged = $true
    try {
        for ($index = 0; $index -lt $ActionIds.Count; $index++) {
            $actionId = [string]$ActionIds[$index]
            $previewArgs = @(
                '--mode','preview',
                '--resident',$selection.Resident.ResidentId,
                '--target-root',[string]$Options['--target-root'],
                '--action',$actionId
            )
            $outcome = Invoke-JavaPlayPairedRootResident -CommandArguments $previewArgs -AcquiredLease $selection.Lease -RetainLease -PassThru
            $result = $outcome.Result
            $pairedPreview = Get-Content -LiteralPath ([string]$result.preview.path) -Raw | ConvertFrom-Json -Depth 100
            $comparisonComponents = @($pairedPreview.components)
            $comparisonDomains = @($comparisonComponents | ForEach-Object { [string]$_.domain })
            $boundaryUnsupported = [string]$pairedPreview.status -eq 'successor_boundary_unsupported'
            if ([string]$pairedPreview.schema -ne 'sts-paired-root-preview-report-v1' -or
                [string]$pairedPreview.status -ne [string]$result.status -or
                [string]$pairedPreview.source_root_id -ne [string]$Options['--target-root'] -or
                [string]$pairedPreview.action.handle_id -ne $actionId -or
                (-not $boundaryUnsupported -and $comparisonComponents.Count -eq 0) -or
                ($boundaryUnsupported -and ($comparisonComponents.Count -ne 0 -or
                    $null -eq $pairedPreview.PSObject.Properties['successor_boundary'] -or
                    -not [bool]$pairedPreview.successor_boundary.stable)) -or
                @($comparisonDomains | Sort-Object -Unique).Count -ne $comparisonComponents.Count -or
                @($comparisonComponents | Where-Object { [string]$_.status -notin @('exact','diverged','unknown') }).Count -ne 0 -or
                ([string]$result.status -eq 'ready' -and
                    @($comparisonComponents | Where-Object { [string]$_.status -ne 'exact' }).Count -ne 0)) {
                throw "Paired-root resident preview returned invalid comparison evidence: $([string]$result.preview.path)"
            }
            $comparisonProjection = @($comparisonComponents | ForEach-Object {
                [ordered]@{
                    domain = [string]$_.domain
                    schema = [string]$_.schema
                    status = [string]$_.status
                    first_difference = if ($null -eq $_.PSObject.Properties['first_difference']) {
                        $null
                    } else {
                        $_.first_difference
                    }
                }
            })
            $allSourceHeadsUnchanged = $allSourceHeadsUnchanged -and [bool]$result.source_head_guard.unchanged
            $allAutosavesUnchanged = $allAutosavesUnchanged -and [bool]$result.autosave.unchanged
            $autosaveSnapshots.Add($result.autosave)
            $memberResults.Add([ordered]@{
                index = $index
                action_handle_id = $actionId
                resident_preview = Get-JavaPlayFileIdentity $outcome.ReportPath
                result = [ordered]@{
                    status = [string]$result.status
                    successor_root_id = [string]$result.result.successor_root_id
                    successor_activation_id = [string]$result.result.successor_activation_id
                    receipt_id = [string]$result.result.receipt_id
                    divergence_id = [string]$result.result.divergence_id
                    successor_boundary = $result.result.successor_boundary
                    java_evidence = $result.result.java_evidence
                    rust_evidence = $result.result.rust_evidence
                    runtime_settlement = $result.result.runtime_settlement
                }
                comparison = [ordered]@{
                    exact_count = @($comparisonProjection | Where-Object { $_.status -eq 'exact' }).Count
                    total_count = $comparisonProjection.Count
                    components = $comparisonProjection
                }
                reset_after = $result.reset
                resident_reusable = [bool]$result.resident_reusable
                timings_ms = $result.timings_ms
            })
            if ($result.status -eq 'diverged') {
                $aggregateStatus = 'diverged'
                $stopReason = 'parity_divergence'
                break
            }
            if ($result.status -eq 'successor_boundary_unsupported') {
                $aggregateStatus = 'successor_boundary_unsupported'
                $stopReason = [string]$result.retire_reason
                break
            }
            if (-not $result.resident_reusable) {
                if ($index -lt ($ActionIds.Count - 1)) {
                    $aggregateStatus = 'reset_unsupported'
                    $stopReason = [string]$result.retire_reason
                }
                break
            }
        }
    } finally {
        Exit-JavaPlayPairedRootResidentLease $selection.Lease
    }
    $aggregate = [ordered]@{
        schema = 'sts-paired-root-preview-batch-v2'
        provenance = 'paired_root_probe'
        created_at_unix_ms = $batchStartedAt
        completed_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        status = $aggregateStatus
        stop_reason = $stopReason
        target_root_id = [string]$Options['--target-root']
        action_space_id = [string]$Observed.action_space_id
        requested_action_ids = @($ActionIds)
        action_observation = Get-JavaPlayFileIdentity $ActionObservationReport
        native_materialization = Get-JavaPlayFileIdentity $MaterializationReport
        runtime = [ordered]@{
            selection = [string]$selection.Selection
            manifest = Get-JavaPlayFileIdentity $selection.Resident.ManifestPath
        }
        members = $memberResults.ToArray()
        autosave = [ordered]@{
            unchanged = $allAutosavesUnchanged
            initial_sha256 = [string]$selection.Resident.Manifest.autosave.sha256
            snapshots = $autosaveSnapshots.ToArray()
        }
        source_head_guard = [ordered]@{
            unchanged = $allSourceHeadsUnchanged
        }
        evidence_scope = [ordered]@{
            java_rust_successors_compared = $memberResults.Count -gt 0 -and
                @($memberResults | Where-Object {
                    [string]$_.result.status -in @('ready','diverged')
                }).Count -eq $memberResults.Count
            same_runtime_reused = $memberResults.Count -gt 1 -or $selection.Selection -eq 'reused'
            cross_invocation_resident = $true
            original_game_native_autosave = $true
            strategy_evidence = $false
            training_eligible = $false
        }
        timings_ms = [ordered]@{
            runtime_ready = [long]$runtimeReady
            previews = [long]$clock.ElapsedMilliseconds - [long]$runtimeReady
            total = [long]$clock.ElapsedMilliseconds
        }
    }
    Write-JavaPlayJsonCreateNew $AggregateReport $aggregate
    return $aggregate
}

function Invoke-JavaPlayPairedRootResumePreview([hashtable] $Options, [string[]] $ActionIds) {
    Assert-LiveBridgeFresh
    if (-not (Test-Path -LiteralPath $runPlayWorkbench -PathType Leaf)) {
        throw 'Run-play workbench is missing. Run .\run-play.ps1 build first.'
    }
    if ($ActionIds.Count -lt 1 -or $ActionIds.Count -gt 64) {
        throw 'Paired-root resume-preview requires between one and sixty-four actions.'
    }
    if (@($ActionIds | Sort-Object -Unique).Count -ne $ActionIds.Count) {
        throw 'Paired-root resume-preview action handles must be unique.'
    }
    $storePath = Resolve-JavaPlayPairedRootStore $Options
    [IO.Directory]::CreateDirectory($pairedRootOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $runDirectory = Join-Path $pairedRootOutput "resume-preview-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
    $actionObservationReport = Join-Path $runDirectory 'actions.json'
    $nativePayload = Join-Path $runDirectory 'native-payload'
    $materializationReport = Join-Path $runDirectory 'native-materialization.json'
    $aggregateReport = Join-Path $runDirectory 'preview-batch.json'

    $observeRaw = & $runPlayWorkbench paired-root-observe-actions --root $Options['--target-root'] --store $storePath --report $actionObservationReport 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Paired-root action preflight failed: $($observeRaw -join "`n")"
    }
    $observed = Get-Content -LiteralPath $actionObservationReport -Raw | ConvertFrom-Json -Depth 100
    Assert-JavaPlayPairedRootActionObservation $observed ([string]$Options['--target-root']) $ActionIds -RequireVerified

    $materializeRaw = & $runPlayWorkbench paired-root-materialize-native --target-root $Options['--target-root'] --store $storePath --output $nativePayload --report $materializationReport 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Paired-root native materialization failed: $($materializeRaw -join "`n")"
    }
    $materialized = Get-Content -LiteralPath $materializationReport -Raw | ConvertFrom-Json -Depth 100
    if ($materialized.schema -ne 'sts-paired-root-native-materialization-report-v1' -or
        [string]$materialized.requested_target_root_id -ne [string]$Options['--target-root'] -or
        [string]::IsNullOrWhiteSpace([string]$materialized.native_root_id) -or
        [string]::IsNullOrWhiteSpace([string]$materialized.java_bundle_id) -or
        [string]::IsNullOrWhiteSpace([string]$materialized.content_profile) -or
        -not (Test-Path -LiteralPath ([string]$materialized.storage_directory) -PathType Container)) {
        throw "Paired-root native materialization returned invalid evidence: $materializationReport"
    }
    $result = Invoke-JavaPlayPairedRootResidentBatch $Options $ActionIds $storePath $observed $materialized $actionObservationReport $materializationReport $aggregateReport
    $outputFormat = if ($Options.ContainsKey('--output')) { [string]$Options['--output'] } else { 'text' }
    if ($outputFormat -eq 'json') {
        Write-Output ($result | ConvertTo-Json -Depth 100 -Compress)
        return
    }
    $memberCount = @($result.members).Count
    $resetCount = @($result.members | Where-Object { $null -ne $_.reset_after }).Count
    $residentReusable = $memberCount -gt 0 -and [bool]$result.members[-1].resident_reusable
    Write-Output "PairedRoot preview batch status=$($result.status) target_root=$($result.target_root_id) requested=$(@($result.requested_action_ids).Count) completed=$memberCount resets=$resetCount runtime=$($result.runtime.selection) reusable=$residentReusable"
    foreach ($member in @($result.members)) {
        Write-Output "paired_preview_member index=$($member.index) status=$($member.result.status) action=$($member.action_handle_id) successor_root=$($member.result.successor_root_id) receipt=$($member.result.receipt_id) divergence=$($member.result.divergence_id)"
    }
    Write-Output "native_files=$($materialized.file_count) autosave_unchanged=$($result.autosave.unchanged) source_head_unchanged=$($result.source_head_guard.unchanged)"
    Write-Output "timings_ms runtime_ready=$($result.timings_ms.runtime_ready) previews=$($result.timings_ms.previews) total=$($result.timings_ms.total)"
    Write-Output "report=$([IO.Path]::GetFullPath($aggregateReport)) actions=$([IO.Path]::GetFullPath($actionObservationReport)) materialization=$([IO.Path]::GetFullPath($materializationReport)) store=$storePath"
    return
}

function Invoke-JavaPlayPairedRootResumeWitness([hashtable] $Options) {
    Assert-LiveBridgeFresh
    if (-not (Test-Path -LiteralPath $runPlayWorkbench -PathType Leaf)) {
        throw 'Run-play workbench is missing. Run .\run-play.ps1 build first.'
    }
    $planPath = [IO.Path]::GetFullPath($(if ([IO.Path]::IsPathRooted([string]$Options['--witness-plan'])) {
        [string]$Options['--witness-plan']
    } else {
        Join-Path $repository ([string]$Options['--witness-plan'])
    }))
    if (-not (Test-Path -LiteralPath $planPath -PathType Leaf)) {
        throw "Paired-root witness plan does not exist: $planPath"
    }
    $plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json -Depth 100
    $intents = @($plan.intents)
    if ($plan.schema -ne 'sts-paired-root-witness-transaction-plan-v1' -or
        [string]$plan.initial_root_id -ne [string]$Options['--target-root'] -or
        [string]$plan.witness_id -notmatch '^paired-search-witness-v1-[0-9a-f]{64}$' -or
        $intents.Count -lt 1 -or $intents.Count -gt 64) {
        throw 'Paired-root witness plan failed its closed typed preflight.'
    }

    $storePath = Resolve-JavaPlayPairedRootStore $Options
    [IO.Directory]::CreateDirectory($pairedRootOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $runDirectory = Join-Path $pairedRootOutput "resume-witness-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
    $nativePayload = Join-Path $runDirectory 'native-payload'
    $materializationReport = Join-Path $runDirectory 'native-materialization.json'
    $activationReport = Join-Path $runDirectory 'activation.json'
    $recoveryAttempt = Join-Path $runDirectory 'recovery-work'
    $recoveryReport = Join-Path $runDirectory 'recovery.json'
    $transactionAttempt = Join-Path $runDirectory 'transaction-work'
    $transactionReport = Join-Path $runDirectory 'witness-transaction.json'
    $attemptReport = Join-Path $runDirectory 'witness-preview.json'
    $failureReport = Join-Path $runDirectory 'failure.json'

    $materializeRaw = & $runPlayWorkbench paired-root-materialize-native --target-root $Options['--target-root'] --store $storePath --output $nativePayload --report $materializationReport 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Paired-root witness materialization failed: $($materializeRaw -join "`n")"
    }
    $materialized = Get-Content -LiteralPath $materializationReport -Raw | ConvertFrom-Json -Depth 100
    if ($materialized.schema -ne 'sts-paired-root-native-materialization-report-v1' -or
        [string]$materialized.requested_target_root_id -ne [string]$Options['--target-root'] -or
        [string]::IsNullOrWhiteSpace([string]$materialized.native_root_id) -or
        [string]::IsNullOrWhiteSpace([string]$materialized.java_bundle_id) -or
        [string]::IsNullOrWhiteSpace([string]$materialized.content_profile)) {
        throw 'Paired-root witness materialization returned invalid evidence.'
    }

    $clock = [Diagnostics.Stopwatch]::StartNew()
    $selection = Find-JavaPlayCompatiblePairedRootResident $storePath $materialized
    if ($null -eq $selection) {
        $opened = Invoke-JavaPlayPairedRootResident -CommandArguments @(
            '--mode','open','--target-root',[string]$Options['--target-root'],'--store',$storePath
        ) -PassThru
        $lease = Enter-JavaPlayPairedRootResidentLease $opened.ResidentId 'witness_transaction'
        try {
            $selection = [pscustomobject]@{
                Resident = Read-JavaPlayPairedRootResident $opened.ResidentId
                Lease = $lease
                Selection = 'opened'
            }
        } catch {
            Exit-JavaPlayPairedRootResidentLease $lease
            throw
        }
    }
    $runtimeReady = $clock.ElapsedMilliseconds
    $resident = $selection.Resident
    try {
        try {
            $continueRaw = & $liveBinary continue-run --output-dir $liveOutput --content-profile ([string]$resident.Manifest.content_profile) 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Paired-root witness Continue failed: $($continueRaw -join "`n")"
            }
            $continueCompleted = $clock.ElapsedMilliseconds
            $transactionRaw = & $runPlayWorkbench paired-root-resume-witness --target-root $Options['--target-root'] --witness-plan $planPath --run-root $runPlayRoot --java-live-output $liveOutput --store $storePath --recovery-attempt $recoveryAttempt --transaction-attempt $transactionAttempt --activation-report $activationReport --recovery-report $recoveryReport --transaction-report $transactionReport 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Paired-root witness transaction failed: $($transactionRaw -join "`n")"
            }
            $transactionCompleted = $clock.ElapsedMilliseconds
            $activated = Get-Content -LiteralPath $activationReport -Raw | ConvertFrom-Json -Depth 100
            $recovered = Get-Content -LiteralPath $recoveryReport -Raw | ConvertFrom-Json -Depth 100
            $transaction = Get-Content -LiteralPath $transactionReport -Raw | ConvertFrom-Json -Depth 100
            $steps = @($transaction.steps)
            if ($activated.schema -ne 'sts-paired-root-native-activation-report-v1' -or
                [string]$activated.requested_target_root_id -ne [string]$Options['--target-root'] -or
                -not $activated.source_head_guard.unchanged -or
                $recovered.schema -ne 'sts-paired-root-recovery-report-v1' -or
                $recovered.status -ne 'ready' -or
                [string]$recovered.target_root_id -ne [string]$Options['--target-root'] -or
                -not $recovered.source_head_guard.unchanged -or
                $transaction.schema -ne 'sts-paired-root-witness-transaction-report-v1' -or
                [string]$transaction.witness_id -ne [string]$plan.witness_id -or
                [string]$transaction.initial_root_id -ne [string]$Options['--target-root'] -or
                [string]$transaction.status -notin @('ready','diverged','intent_unavailable','successor_boundary_unsupported') -or
                -not $transaction.source_head_guard.unchanged -or
                $steps.Count -lt 1 -or $steps.Count -gt $intents.Count -or
                ([string]$transaction.status -eq 'ready' -and $steps.Count -ne $intents.Count)) {
                throw 'Paired-root witness transaction returned invalid scoped evidence.'
            }
            for ($index = 0; $index -lt $steps.Count; $index++) {
                $step = $steps[$index]
                if ([long]$step.step_ordinal -ne $index -or
                    [string]$step.resolution.witness_id -ne [string]$plan.witness_id -or
                    [long]$step.resolution.step_ordinal -ne $index -or
                    $step.resolution.intent.kind -ne $intents[$index].kind) {
                    throw "Paired-root witness step $index is not bound to its typed intent."
                }
            }
            $runtimeSettlement = if ([string]$transaction.status -in @('ready','intent_unavailable')) {
                Get-JavaPlayPairedRootRuntimeSettlement $storePath ([string]$transaction.reached_root_id)
            } else {
                $null
            }

            $statusRaw = & $liveBinary status --output-dir $liveOutput --json 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Paired-root witness successor status failed: $($statusRaw -join "`n")"
            }
            $afterView = ($statusRaw -join "`n") | ConvertFrom-Json -Depth 100
            if ([string]$afterView.manifest.session_id -ne [string]$resident.Manifest.session.session_id -or
                [string]$afterView.manifest.connection_id -ne [string]$resident.Manifest.session.connection_id) {
                throw 'Paired-root witness transaction changed its live runtime identity.'
            }
            $afterAutosave = if (Test-Path -LiteralPath ([string]$resident.Autosave.path) -PathType Leaf) {
                Get-JavaPlayFileIdentity ([string]$resident.Autosave.path)
            } else {
                [pscustomobject]@{ path = [string]$resident.Autosave.path; exists = $false; length = $null; sha256 = $null }
            }
            $autosaveUnchanged = $null -ne $afterAutosave.length -and
                [long]$afterAutosave.length -eq [long]$resident.Manifest.autosave.length -and
                [string]$afterAutosave.sha256 -eq [string]$resident.Manifest.autosave.sha256
            $autosaveMayDiffer = $null -ne $runtimeSettlement -and
                [string]$runtimeSettlement.native_autosave -eq 'may_differ_from_native_root'
            if (-not $autosaveUnchanged -and
                -not $autosaveMayDiffer -and
                [string]$transaction.status -ne 'successor_boundary_unsupported') {
                throw 'Paired-root witness transaction changed the native autosave.'
            }

            $resetIdentity = $null
            $returnedToNeutral = $false
            $retireReason = $null
            if ([string]$transaction.status -in @('ready','intent_unavailable') -and
                [string]$afterView.frame.boundary.kind -eq 'combat_player_decision') {
                $resetReport = Join-Path $runDirectory 'reset.json'
                $resetRaw = & $liveBinary reset-natural-run --output-dir $liveOutput --report-path $resetReport 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "Paired-root witness reset failed: $($resetRaw -join "`n")"
                }
                $reset = ($resetRaw -join "`n") | ConvertFrom-Json -Depth 100
                if (-not $reset.passed -or -not $reset.preserved_save_main_menu_verified -or
                    [string]$reset.session_id -ne [string]$resident.Manifest.session.session_id -or
                    [string]$reset.connection_id -ne [string]$resident.Manifest.session.connection_id) {
                    throw 'Paired-root witness reset returned invalid evidence.'
                }
                $resetIdentity = Get-JavaPlayFileIdentity $resetReport
                $resident = Read-JavaPlayPairedRootResident $resident.ResidentId
                $returnedToNeutral = $true
            } elseif ([string]$transaction.status -eq 'diverged') {
                $retireReason = 'parity_divergence'
            } elseif ([string]$transaction.status -eq 'successor_boundary_unsupported') {
                $retireReason = 'successor_boundary_unsupported'
            } elseif ($null -ne $runtimeSettlement -and
                [string]$runtimeSettlement.resident_completion -eq 'retire') {
                $retireReason = 'typed_runtime_settlement:retire'
            } else {
                $retireReason = "reset_unsupported:$([string]$afterView.frame.boundary.kind)"
            }
            $completedAt = $clock.ElapsedMilliseconds
            $result = [ordered]@{
                schema = 'sts-paired-root-witness-preview-v1'
                created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() - [long]$completedAt
                completed_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                status = [string]$transaction.status
                witness_id = [string]$transaction.witness_id
                initial_root_id = [string]$transaction.initial_root_id
                reached_root_id = [string]$transaction.reached_root_id
                stopped_step_ordinal = if ($null -eq $transaction.PSObject.Properties['stopped_step_ordinal']) { $null } else { $transaction.stopped_step_ordinal }
                stop_reason = if ($null -eq $transaction.PSObject.Properties['stop_reason']) { $null } else { [string]$transaction.stop_reason }
                runtime = [ordered]@{
                    selection = [string]$selection.Selection
                    manifest = Get-JavaPlayFileIdentity $resident.ManifestPath
                }
                native_materialization = Get-JavaPlayFileIdentity $materializationReport
                activation = Get-JavaPlayFileIdentity $activationReport
                recovery = Get-JavaPlayFileIdentity $recoveryReport
                transaction = Get-JavaPlayFileIdentity $transactionReport
                reset = $resetIdentity
                runtime_settlement = $runtimeSettlement
                resident_reusable = $returnedToNeutral
                retire_reason = $retireReason
                autosave = [ordered]@{ unchanged = $autosaveUnchanged; before = $selection.Resident.Autosave; after_preview = $afterAutosave }
                source_head_guard = [ordered]@{ unchanged = [bool]$activated.source_head_guard.unchanged -and [bool]$recovered.source_head_guard.unchanged -and [bool]$transaction.source_head_guard.unchanged }
                timings_ms = [ordered]@{
                    runtime_ready = [long]$runtimeReady
                    continue = [long]$continueCompleted - [long]$runtimeReady
                    transaction = [long]$transactionCompleted - [long]$continueCompleted
                    finalize = [long]$completedAt - [long]$transactionCompleted
                    total = [long]$completedAt
                }
                evidence_scope = [ordered]@{
                    sequential_same_runtime = $true
                    per_step_java_rust_comparison = $true
                    original_game_native_autosave = $true
                    strategy_evidence = $false
                    training_eligible = $false
                }
            }
            Write-JavaPlayJsonCreateNew $attemptReport $result
            if (-not $returnedToNeutral) {
                Retire-JavaPlayPairedRootResident $resident $retireReason 'Witness transaction could not return to its neutral reusable boundary.' $attemptReport | Out-Null
            }
            $outputFormat = if ($Options.ContainsKey('--output')) { [string]$Options['--output'] } else { 'text' }
            if ($outputFormat -eq 'json') {
                Write-Output ($result | ConvertTo-Json -Depth 100 -Compress)
                return
            }
            Write-Output "PairedRoot witness status=$($result.status) witness=$($result.witness_id) steps=$($steps.Count) reached_root=$($result.reached_root_id) runtime=$($result.runtime.selection) reusable=$($result.resident_reusable)"
            Write-Output "timings_ms runtime_ready=$($result.timings_ms.runtime_ready) continue=$($result.timings_ms.continue) transaction=$($result.timings_ms.transaction) finalize=$($result.timings_ms.finalize) total=$($result.timings_ms.total)"
            Write-Output "report=$([IO.Path]::GetFullPath($attemptReport)) transaction=$([IO.Path]::GetFullPath($transactionReport))"
            return
        } catch {
            $failure = $_.Exception.Message
            if (-not (Test-Path -LiteralPath $failureReport -PathType Leaf)) {
                Write-JavaPlayJsonCreateNew $failureReport ([ordered]@{
                    schema = 'sts-java-play-paired-root-witness-preview-failure-v1'
                    resident_id = $resident.ResidentId
                    witness_id = [string]$plan.witness_id
                    created_at_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                    failure = $failure
                    isolated_storage_retained = $true
                })
            }
            $retired = Retire-JavaPlayPairedRootResident $resident 'runtime_failure' $failure $failureReport
            throw "Paired-root witness failed and retired its runtime; report=${failureReport} retired=${retired}: $failure"
        }
    } finally {
        Exit-JavaPlayPairedRootResidentLease $selection.Lease
    }
}

function Invoke-JavaPlayPairedRoot([string[]] $CommandArguments) {
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'resident') {
        return Invoke-JavaPlayPairedRootResident @($CommandArguments | Select-Object -Skip 1)
    }
    if ($CommandArguments.Count -lt 1 -or $CommandArguments[0] -notin @('actions','prepare','preview','resume-preview','resume-witness') -or
        (($CommandArguments.Count - 1) % 2) -ne 0) {
        throw 'Usage: .\java-play.ps1 paired-root actions --root <root-id> [--store <artifact-store>]; prepare --checkpoint <checkpoint.json> --plan <mirror-plan.json> --session <run-play-session> --at <retained-selector> [--store <artifact-store>]; preview with the same source inputs plus one --action <paired-action-id>; resume-preview --target-root <root-id> --action <paired-action-id> [--action <paired-action-id> ...] [--store <artifact-store>] [--output text|json]; or resume-witness --target-root <root-id> --witness-plan <typed-plan.json> [--store <artifact-store>] [--output text|json]'
    }
    $operation = $CommandArguments[0]
    $options = @{}
    $actionIds = [Collections.Generic.List[string]]::new()
    for ($index = 1; $index -lt $CommandArguments.Count; $index += 2) {
        $name = $CommandArguments[$index]
        $value = $CommandArguments[$index + 1]
        if ($name -notin @('--checkpoint','--plan','--session','--at','--store','--root','--target-root','--action','--output','--witness-plan') -or
            [string]::IsNullOrWhiteSpace($value)) {
            throw "Invalid or duplicate paired-root $operation option: $name"
        }
        if ($name -eq '--action') {
            $actionIds.Add($value)
            continue
        }
        if ($options.ContainsKey($name)) {
            throw "Invalid or duplicate paired-root $operation option: $name"
        }
        $options[$name] = $value
    }
    if ($operation -eq 'actions') {
        if (-not $options.ContainsKey('--root')) {
            throw 'Paired-root actions requires --root.'
        }
        if (@($options.Keys | Where-Object { $_ -notin @('--root','--store') }).Count -ne 0) {
            throw 'Paired-root actions accepts only --root and optional --store.'
        }
        return Invoke-JavaPlayPairedRootActions $options
    }
    if ($operation -eq 'resume-preview') {
        if (-not $options.ContainsKey('--target-root') -or $actionIds.Count -eq 0) {
            throw 'Paired-root resume-preview requires --target-root and --action.'
        }
        foreach ($retired in @('--checkpoint','--plan','--session','--at','--root')) {
            if ($options.ContainsKey($retired)) {
                throw "Paired-root resume-preview derives $retired from its target root and does not accept it."
            }
        }
        if ($options.ContainsKey('--output') -and [string]$options['--output'] -notin @('text','json')) {
            throw 'Paired-root resume-preview --output must be text or json.'
        }
        return Invoke-JavaPlayPairedRootResumePreview $options $actionIds.ToArray()
    }
    if ($operation -eq 'resume-witness') {
        if (-not $options.ContainsKey('--target-root') -or
            -not $options.ContainsKey('--witness-plan') -or
            $actionIds.Count -ne 0) {
            throw 'Paired-root resume-witness requires --target-root and --witness-plan and does not accept --action.'
        }
        if (@($options.Keys | Where-Object { $_ -notin @('--target-root','--witness-plan','--store','--output') }).Count -ne 0) {
            throw 'Paired-root resume-witness accepts only --target-root, --witness-plan, optional --store, and optional --output.'
        }
        if ($options.ContainsKey('--output') -and [string]$options['--output'] -notin @('text','json')) {
            throw 'Paired-root resume-witness --output must be text or json.'
        }
        return Invoke-JavaPlayPairedRootResumeWitness $options
    }
    if ($options.ContainsKey('--output')) {
        throw "Paired-root $operation does not accept --output."
    }
    foreach ($required in @('--checkpoint','--plan','--session','--at')) {
        if (-not $options.ContainsKey($required)) {
            throw "Paired-root $operation requires $required."
        }
    }
    if ($operation -eq 'prepare' -and
        ($options.ContainsKey('--root') -or $options.ContainsKey('--target-root') -or $actionIds.Count -ne 0)) {
        throw 'Paired-root prepare does not accept root or action selection options.'
    }
    if ($operation -eq 'preview' -and
        ($options.ContainsKey('--root') -or $options.ContainsKey('--target-root') -or $actionIds.Count -ne 1)) {
        throw 'Paired-root preview requires --action and does not accept --root or --target-root.'
    }
    if ($operation -eq 'preview') {
        $options['--action'] = $actionIds[0]
    }

    Assert-LiveBridgeFresh
    Assert-GameClosed
    if (-not (Test-Path -LiteralPath $runPlayWorkbench -PathType Leaf)) {
        throw 'Run-play workbench is missing. Run .\run-play.ps1 build first.'
    }
    $checkpoint = Read-JavaPlayNativeCheckpoint $options['--checkpoint']
    $planPath = [IO.Path]::GetFullPath($(if ([IO.Path]::IsPathRooted($options['--plan'])) {
        $options['--plan']
    } else {
        Join-Path $repository $options['--plan']
    }))
    $sessionPath = [IO.Path]::GetFullPath($(if ([IO.Path]::IsPathRooted($options['--session'])) {
        $options['--session']
    } else {
        Join-Path $repository $options['--session']
    }))
    $storePath = Resolve-JavaPlayPairedRootStore $options
    if (-not (Test-Path -LiteralPath $planPath -PathType Leaf)) {
        throw "Paired-root mirror plan does not exist: $planPath"
    }
    if (-not (Test-Path -LiteralPath $sessionPath -PathType Container)) {
        throw "Paired-root run-play session does not exist: $sessionPath"
    }
    $sourcePreflightRaw = & $runPlayWorkbench paired-root-prepare-preflight --session $sessionPath 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Paired-root source preflight failed before Java startup or artifact creation: $($sourcePreflightRaw -join "`n")"
    }
    try {
        $sourcePreflight = ($sourcePreflightRaw -join "`n") | ConvertFrom-Json -Depth 20
    } catch {
        throw "Paired-root source preflight returned invalid JSON: $($sourcePreflightRaw -join "`n")"
    }
    if ($sourcePreflight.schema -ne 'sts-paired-root-source-preflight-v1' -or
        $sourcePreflight.status -ne 'ready') {
        throw "Paired-root source preflight returned an invalid readiness result."
    }
    $plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json -Depth 100
    $contentProfile = [string]$plan.run_start.content_profile
    if ([string]::IsNullOrWhiteSpace($contentProfile)) {
        throw "Paired-root mirror plan omitted run_start.content_profile: $planPath"
    }

    [IO.Directory]::CreateDirectory($pairedRootOutput) | Out-Null
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $runDirectory = Join-Path $pairedRootOutput "$operation-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
    $prepareAttempt = Join-Path $runDirectory 'prepare-work'
    $previewAttempt = Join-Path $runDirectory 'preview-work'
    $prepareReport = Join-Path $runDirectory 'prepare.json'
    $previewReport = Join-Path $runDirectory 'preview.json'
    $report = if ($operation -eq 'prepare') { $prepareReport } else { $previewReport }
    $lifecycle = Join-Path $runDirectory 'lifecycle.json'

    $result = Invoke-JavaPlayIsolatedCase -CaseKind "paired_root_$operation" -Profile 'accelerated' -ControlProfile 'parity_run_control' -EvidencePath $report -LifecyclePath $lifecycle -StorageSeed $checkpoint -Body {
        param($launch, $view)
        $continueRaw = & $liveBinary continue-run --output-dir $liveOutput --content-profile $contentProfile 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Paired-root checkpoint Continue failed: $($continueRaw -join "`n")"
        }
        $prepareRaw = & $runPlayWorkbench paired-root-prepare --session $sessionPath --at $options['--at'] --java-checkpoint $checkpoint.ManifestPath --java-plan $planPath --java-live-output $liveOutput --store $storePath --attempt $prepareAttempt --report $prepareReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Paired-root Rust preparation failed: $($prepareRaw -join "`n")"
        }
        $prepared = Get-Content -LiteralPath $prepareReport -Raw | ConvertFrom-Json -Depth 100
        if ($prepared.schema -ne 'sts-paired-root-prepare-report-v1' -or
            $prepared.status -notin @('ready','preflight_stopped') -or
            -not $prepared.source_head_guard.unchanged) {
            throw "Paired-root preparation returned invalid scoped evidence: $prepareReport"
        }
        if ($operation -eq 'prepare') {
            return $prepared
        }
        if ($prepared.status -ne 'ready' -or
            $null -eq $prepared.PSObject.Properties['root_id'] -or
            [string]::IsNullOrWhiteSpace([string]$prepared.root_id) -or
            $null -eq $prepared.PSObject.Properties['activation_id'] -or
            [string]::IsNullOrWhiteSpace([string]$prepared.activation_id)) {
            throw "Paired-root preview source did not pass preflight: $prepareReport"
        }
        $activeRootId = [string]$prepared.root_id
        $activeActivationId = [string]$prepared.activation_id
        $previewRaw = & $runPlayWorkbench paired-root-preview-action --session $sessionPath --root $activeRootId --activation $activeActivationId --action $options['--action'] --java-live-output $liveOutput --store $storePath --attempt $previewAttempt --report $previewReport 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Paired-root typed preview failed: $($previewRaw -join "`n")"
        }
        $previewed = Get-Content -LiteralPath $previewReport -Raw | ConvertFrom-Json -Depth 100
        if ($previewed.schema -ne 'sts-paired-root-preview-report-v1' -or
            $previewed.status -notin @('ready','diverged','successor_boundary_unsupported') -or
            -not $previewed.source_head_guard.unchanged) {
            throw "Paired-root preview returned invalid scoped evidence: $previewReport"
        }
        return $previewed
    }

    $parsedLifecycle = Get-Content -LiteralPath $lifecycle -Raw | ConvertFrom-Json -Depth 100
    if ($operation -eq 'prepare') {
        $rootId = if ($null -eq $result.PSObject.Properties['root_id']) { 'none' } else { [string]$result.root_id }
        $activationId = if ($null -eq $result.PSObject.Properties['activation_id']) { 'none' } else { [string]$result.activation_id }
        Write-Output "PairedRoot prepare status=$($result.status) source_node=$($result.source_archive_node_id) root_id=$rootId activation_id=$activationId"
    } else {
        $successorId = if ($null -eq $result.PSObject.Properties['successor_root_id']) { 'none' } else { [string]$result.successor_root_id }
        $successorActivationId = if ($null -eq $result.PSObject.Properties['successor_activation_id']) { 'none' } else { [string]$result.successor_activation_id }
        $receiptId = if ($null -eq $result.PSObject.Properties['receipt_id']) { 'none' } else { [string]$result.receipt_id }
        $divergenceId = if ($null -eq $result.PSObject.Properties['divergence_id']) { 'none' } else { [string]$result.divergence_id }
        Write-Output "PairedRoot preview status=$($result.status) source_root=$($result.source_root_id) source_activation=$($result.source_activation_id) action=$($result.action.handle_id) successor_root=$successorId successor_activation=$successorActivationId receipt=$receiptId divergence=$divergenceId"
        Write-Output "timings_ms rust=$($result.timings.rust_execution_ms) java=$($result.timings.java_execution_ms) compare=$($result.timings.comparison_ms)"
    }
    Write-Output "source_head_unchanged=$($result.source_head_guard.unchanged) head_files=$($result.source_head_guard.file_count) java_session=$($parsedLifecycle.session_id)"
    Write-Output "report=$([IO.Path]::GetFullPath($report)) prepare=$([IO.Path]::GetFullPath($prepareReport)) lifecycle=$([IO.Path]::GetFullPath($lifecycle)) store=$storePath"
}

function Invoke-JavaPlayMirror([string[]] $CommandArguments) {
    Assert-LiveBridgeFresh
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'plan') {
        $forward = @($CommandArguments | Select-Object -Skip 1)
        $rewritten = [System.Collections.Generic.List[string]]::new()
        for ($index = 0; $index -lt $forward.Count; $index++) {
            if ($forward[$index] -eq '--output') {
                $rewritten.Add('--output-plan')
            } else {
                $rewritten.Add($forward[$index])
            }
        }
        $outputIndex = $rewritten.IndexOf('--output-plan')
        if ($outputIndex -lt 0 -or $outputIndex + 1 -ge $rewritten.Count) {
            throw 'mirror plan requires --output <fresh-plan.json>'
        }
        $planPath = if ([IO.Path]::IsPathRooted($rewritten[$outputIndex + 1])) {
            [IO.Path]::GetFullPath($rewritten[$outputIndex + 1])
        } else {
            [IO.Path]::GetFullPath((Join-Path $repository $rewritten[$outputIndex + 1]))
        }
        $bridgeArguments = @('compile-mirror-plan') + $rewritten.ToArray()
        $result = & $liveBinary @bridgeArguments 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Java mirror plan compilation failed: $($result -join "`n")"
        }
        $plan = ($result -join "`n") | ConvertFrom-Json -Depth 100
        $invalidSchema = $plan.schema -ne 'sts-java-parity-mirror-plan-v4'
        $invalidDepth = @($plan.steps).Count -ne [int]$plan.source.total_active_path_steps
        $unsupportedScope = -not $plan.coverage.selected_scope_supported_by_current_run_profile
        $hasGap = $null -ne $plan.coverage.first_current_profile_gap
        if ($invalidSchema -or $invalidDepth -or $unsupportedScope -or $hasGap) {
            throw "Java mirror plan compiler returned an invalid current plan: $planPath"
        }
        $boundaryProperty = $plan.scope.PSObject.Properties['expected_successor_combat_boundary']
        $successorBoundary = if ($null -eq $boundaryProperty) {
            'none'
        } else {
            [string]$boundaryProperty.Value
        }
        $successorProjection = (
            $null -ne $plan.scope.PSObject.Properties['expected_successor_scalars'] -and
            $null -ne $plan.scope.PSObject.Properties['expected_successor_combat']
        )
        Write-Output "Java mirror plan compiled schema=$($plan.schema) head=h$($plan.source.head_revision) state=$($plan.source.state_revision) steps=$(@($plan.steps).Count) successor=$($plan.scope.expected_successor_site) boundary=$successorBoundary successor_projection=$successorProjection"
        Write-Output "coverage current_profile_supported=$($plan.coverage.selected_scope_supported_by_current_run_profile) first_gap=none required_action_kinds=$(@($plan.coverage.required_action_kinds).Count)"
        Write-Output "plan=$planPath"
        return
    }
    if ($CommandArguments.Count -eq 3 -and $CommandArguments[0] -eq 'run' -and $CommandArguments[1] -eq '--plan') {
        Assert-GameClosed
        [IO.Directory]::CreateDirectory($probeOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $runDirectory = Join-Path $probeOutput "mirror-prefix-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
        $planArgument = $CommandArguments[2]
        $plan = if ([IO.Path]::IsPathRooted($planArgument)) {
            [IO.Path]::GetFullPath($planArgument)
        } else {
            [IO.Path]::GetFullPath((Join-Path $repository $planArgument))
        }
        $report = Join-Path $runDirectory 'execution.json'
        $lifecycle = Join-Path $runDirectory 'lifecycle.json'
        $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'natural_mirror_prefix' -Profile 'accelerated' -ControlProfile 'parity_run_control' -EvidencePath $report -LifecyclePath $lifecycle -Body {
            param($launch, $view)
            $result = & $liveBinary run-mirror-plan --output-dir $liveOutput --mirror-plan $plan --expected-launch-plan $launch.Plan.Path --report-path $report 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Java mirror run stopped: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed -or -not $value.evidence_scope.planned_prefix_completed -or -not $value.evidence_scope.expected_successor_reached -or -not $value.evidence_scope.first_combat_victory_reached -or -not $value.evidence_scope.first_reward_completed -or -not $value.evidence_scope.post_reward_map_reached -or $value.evidence_scope.simulator_comparison -or $value.evidence_scope.training_eligible) {
                throw "Java mirror run returned invalid scoped evidence: $report"
            }
            return $value
        }
        $parsedLifecycle = Get-Content -LiteralPath $lifecycle -Raw | ConvertFrom-Json -Depth 100
        $mirrorExecutionMs = [long]$parsed.completed_at_unix_ms - [long]$parsed.created_at_unix_ms
        Write-Output "Java mirror prefix passed session=$($parsed.session.session_id) actions=$($parsed.completed_plan_steps)/$($parsed.planned_steps) receipts=$(@($parsed.steps).Count)"
        Write-Output "milestones first_combat_victory=$($parsed.evidence_scope.first_combat_victory_reached) first_reward_completed=$($parsed.evidence_scope.first_reward_completed) post_reward_map=$($parsed.evidence_scope.post_reward_map_reached)"
        Write-Output "evidence scalar_pilot_compared=$($parsed.evidence_scope.scalar_pilot_compared) complete_rust_semantic_fingerprint_compared=$($parsed.evidence_scope.complete_rust_semantic_fingerprint_compared) simulator_comparison=$($parsed.evidence_scope.simulator_comparison) strategy_evidence=$($parsed.evidence_scope.strategy_evidence) training_eligible=$($parsed.evidence_scope.training_eligible)"
        Write-Output "lifecycle installed_storage_unchanged=$($parsedLifecycle.installed_storage_guard.unchanged) isolated_storage_retained=$($parsedLifecycle.isolated_storage.retained) timings_ms mirror_execution=$mirrorExecutionMs prepare_storage=$($parsedLifecycle.timings_ms.prepare_storage_and_guard) launch_to_ready=$($parsedLifecycle.timings_ms.launch_to_session_ready) case=$($parsedLifecycle.timings_ms.case_after_session_ready) shutdown=$($parsedLifecycle.timings_ms.managed_shutdown) cleanup=$($parsedLifecycle.timings_ms.cleanup) total=$($parsedLifecycle.timings_ms.total)"
        Write-Output "plan=$plan execution=$([IO.Path]::GetFullPath($report)) lifecycle=$([IO.Path]::GetFullPath($lifecycle))"
        return
    }
    if ($CommandArguments.Count -eq 3 -and $CommandArguments[0] -eq 'checkpoint' -and $CommandArguments[1] -eq '--plan') {
        Assert-GameClosed
        [IO.Directory]::CreateDirectory($probeOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $runDirectory = Join-Path $probeOutput "mirror-checkpoint-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
        $planArgument = $CommandArguments[2]
        $planPath = if ([IO.Path]::IsPathRooted($planArgument)) {
            [IO.Path]::GetFullPath($planArgument)
        } else {
            [IO.Path]::GetFullPath((Join-Path $repository $planArgument))
        }
        $plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json -Depth 100
        $report = Join-Path $runDirectory 'execution.json'
        $lifecycle = Join-Path $runDirectory 'lifecycle.json'
        $checkpointDirectory = Join-Path $runDirectory 'checkpoint'
        $capturePath = Join-Path $checkpointDirectory 'capture.json'
        $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'natural_mirror_checkpoint' -Profile 'accelerated' -ControlProfile 'parity_run_control' -EvidencePath $report -LifecyclePath $lifecycle -Body {
            param($launch, $view)
            $result = & $liveBinary run-mirror-plan --output-dir $liveOutput --mirror-plan $planPath --expected-launch-plan $launch.Plan.Path --report-path $report 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Java mirror checkpoint run stopped: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed -or -not $value.evidence_scope.continuous_from_neow -or -not $value.evidence_scope.expected_successor_reached -or $value.evidence_scope.native_checkpoint_restored) {
                throw "Java mirror checkpoint source returned invalid full-prefix evidence: $report"
            }
            $capture = Export-JavaPlayNativeCombatCheckpoint -Launch $launch -Execution $value -Plan $plan -CheckpointDirectory $checkpointDirectory
            Write-JavaPlayJsonCreateNew $capturePath $capture
            return $value
        }
        $capture = Get-Content -LiteralPath $capturePath -Raw | ConvertFrom-Json -Depth 100
        $manifestPath = Join-Path $checkpointDirectory 'checkpoint.json'
        $manifest = [ordered]@{
            schema = 'sts-java-parity-native-combat-checkpoint-v1'
            provenance = 'natural_full_prefix_native_autosave'
            native_scope = 'enter_room_combat'
            storage_edited = $false
            source_depth = [int]$parsed.completed_plan_steps
            source_plan = Get-JavaPlayFileIdentity $planPath
            source_execution = Get-JavaPlayFileIdentity $report
            source_lifecycle = Get-JavaPlayFileIdentity $lifecycle
            root_frame = $capture.root_frame
            autosave = $capture.autosave
            storage = $capture.storage_inventory
            autosave_summary = $capture.autosave_summary
            root_decision = $capture.root_decision
            evidence_scope = [ordered]@{
                continuous_from_neow = $true
                original_game_native_autosave = $true
                simulator_comparison = $false
                strategy_evidence = $false
                training_eligible = $false
            }
        }
        Write-JavaPlayJsonCreateNew $manifestPath $manifest
        $parsedLifecycle = Get-Content -LiteralPath $lifecycle -Raw | ConvertFrom-Json -Depth 100
        Write-Output "Java native combat checkpoint passed session=$($parsed.session.session_id) depth=$($parsed.completed_plan_steps) autosave=$($capture.autosave_summary.room)"
        Write-Output "lifecycle installed_storage_unchanged=$($parsedLifecycle.installed_storage_guard.unchanged) isolated_storage_retained=$($parsedLifecycle.isolated_storage.retained) timings_ms total=$($parsedLifecycle.timings_ms.total)"
        Write-Output "checkpoint=$([IO.Path]::GetFullPath($manifestPath)) execution=$([IO.Path]::GetFullPath($report)) lifecycle=$([IO.Path]::GetFullPath($lifecycle))"
        return
    }
    if ($CommandArguments.Count -eq 5 -and $CommandArguments[0] -eq 'suffix' -and $CommandArguments[1] -eq '--checkpoint' -and $CommandArguments[3] -eq '--plan') {
        Assert-GameClosed
        $checkpoint = Read-JavaPlayNativeCheckpoint $CommandArguments[2]
        $planArgument = $CommandArguments[4]
        $planPath = if ([IO.Path]::IsPathRooted($planArgument)) {
            [IO.Path]::GetFullPath($planArgument)
        } else {
            [IO.Path]::GetFullPath((Join-Path $repository $planArgument))
        }
        $plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json -Depth 100
        $expectsSuccessorCombatProjection = (
            $null -ne $plan.scope.PSObject.Properties['expected_successor_scalars'] -and
            $null -ne $plan.scope.PSObject.Properties['expected_successor_combat']
        )
        $suffixStepCount = @($plan.steps).Count - [int]$checkpoint.Manifest.source_depth
        $lastPlanStep = if (@($plan.steps).Count -eq 0) { $null } else { @($plan.steps)[-1] }
        $mayReachNextCombatRoom = (
            [string]$plan.scope.expected_successor_site -eq 'combat' -and
            $suffixStepCount -gt 0 -and
            [string]$lastPlanStep.action_kind -eq 'choose_route_node'
        )
        [IO.Directory]::CreateDirectory($probeOutput) | Out-Null
        $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
        $runDirectory = Join-Path $probeOutput "mirror-suffix-$stamp-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
        [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
        $report = Join-Path $runDirectory 'execution.json'
        $lifecycle = Join-Path $runDirectory 'lifecycle.json'
        $checkpointDirectory = Join-Path $runDirectory 'checkpoint'
        $capturePath = Join-Path $checkpointDirectory 'capture.json'
        $parsed = Invoke-JavaPlayIsolatedCase -CaseKind 'natural_mirror_checkpoint_suffix' -Profile 'accelerated' -ControlProfile 'parity_run_control' -EvidencePath $report -LifecyclePath $lifecycle -StorageSeed $checkpoint -Body {
            param($launch, $view)
            $result = & $liveBinary run-mirror-suffix --output-dir $liveOutput --mirror-plan $planPath --checkpoint-manifest $checkpoint.ManifestPath --expected-launch-plan $launch.Plan.Path --report-path $report 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "Java mirror suffix stopped: $($result -join "`n")"
            }
            $value = ($result -join "`n") | ConvertFrom-Json -Depth 100
            if (-not $value.passed -or -not $value.evidence_scope.native_checkpoint_restored -or -not $value.evidence_scope.checkpoint_suffix_captured -or ($expectsSuccessorCombatProjection -and -not $value.evidence_scope.successor_combat_projection_compared) -or $value.evidence_scope.continuous_from_neow -or $value.evidence_scope.simulator_comparison -or $value.evidence_scope.training_eligible) {
                throw "Java mirror suffix returned invalid scoped evidence: $report"
            }
            if ($mayReachNextCombatRoom) {
                $statusJson = & $liveBinary status --output-dir $liveOutput --json 2>$null
                if ($LASTEXITCODE -ne 0 -or -not $statusJson) {
                    throw 'Could not read the live Java suffix endpoint before child-checkpoint classification.'
                }
                $endpoint = ($statusJson -join "`n") | ConvertFrom-Json -Depth 100
                $endpointAct = [int]$endpoint.frame.public_base.run.act
                $endpointFloor = [int]$endpoint.frame.public_base.run.floor
                $parentAct = [int]$checkpoint.Manifest.root_decision.act
                $parentFloor = [int]$checkpoint.Manifest.root_decision.floor
                if ($endpointAct -ne $parentAct -or $endpointFloor -ne $parentFloor) {
                    $capture = Export-JavaPlayNativeCombatCheckpoint -Launch $launch -Execution $value -Plan $plan -CheckpointDirectory $checkpointDirectory
                    Write-JavaPlayJsonCreateNew $capturePath $capture
                }
            }
            return $value
        }
        $nextCheckpoint = $null
        if (Test-Path -LiteralPath $capturePath -PathType Leaf) {
            $capture = Get-Content -LiteralPath $capturePath -Raw | ConvertFrom-Json -Depth 100
            $nextCheckpoint = Join-Path $checkpointDirectory 'checkpoint.json'
            $manifest = [ordered]@{
                schema = 'sts-java-parity-native-combat-checkpoint-v1'
                provenance = 'natural_checkpoint_suffix_native_autosave'
                native_scope = 'enter_room_combat'
                storage_edited = $false
                source_depth = @($plan.steps).Count
                source_plan = Get-JavaPlayFileIdentity $planPath
                source_execution = Get-JavaPlayFileIdentity $report
                source_lifecycle = Get-JavaPlayFileIdentity $lifecycle
                parent_checkpoint = $checkpoint.ManifestIdentity
                root_frame = $capture.root_frame
                autosave = $capture.autosave
                storage = $capture.storage_inventory
                autosave_summary = $capture.autosave_summary
                root_decision = $capture.root_decision
                evidence_scope = [ordered]@{
                    continuous_from_neow = $false
                    original_game_native_autosave = $true
                    native_checkpoint_restored = $true
                    checkpoint_suffix_captured = $true
                    simulator_comparison = $false
                    strategy_evidence = $false
                    training_eligible = $false
                }
            }
            Write-JavaPlayJsonCreateNew $nextCheckpoint $manifest
        }
        $parsedLifecycle = Get-Content -LiteralPath $lifecycle -Raw | ConvertFrom-Json -Depth 100
        Write-Output "Java mirror suffix passed session=$($parsed.session.session_id) source_depth=$($parsed.resume.source_depth) actions=$($parsed.completed_plan_steps)/$($parsed.planned_steps) receipts=$(@($parsed.steps).Count)"
        Write-Output "evidence native_checkpoint_restored=$($parsed.evidence_scope.native_checkpoint_restored) successor_combat_projection_compared=$($parsed.evidence_scope.successor_combat_projection_compared) continuous_from_neow=$($parsed.evidence_scope.continuous_from_neow) simulator_comparison=$($parsed.evidence_scope.simulator_comparison) training_eligible=$($parsed.evidence_scope.training_eligible)"
        Write-Output "lifecycle installed_storage_unchanged=$($parsedLifecycle.installed_storage_guard.unchanged) isolated_storage_retained=$($parsedLifecycle.isolated_storage.retained) timings_ms launch_to_ready=$($parsedLifecycle.timings_ms.launch_to_session_ready) case=$($parsedLifecycle.timings_ms.case_after_session_ready) total=$($parsedLifecycle.timings_ms.total)"
        Write-Output "parent_checkpoint=$($checkpoint.ManifestPath) next_checkpoint=$(if($null -eq $nextCheckpoint){'none'}else{[IO.Path]::GetFullPath($nextCheckpoint)}) plan=$planPath execution=$([IO.Path]::GetFullPath($report)) lifecycle=$([IO.Path]::GetFullPath($lifecycle))"
        return
    }
    if ($CommandArguments.Count -eq 5 -and $CommandArguments[0] -eq 'reuse-probe' -and $CommandArguments[1] -eq '--checkpoint' -and $CommandArguments[3] -eq '--plan') {
        Invoke-JavaPlayMirrorCheckpointBatch -CheckpointArgument $CommandArguments[2] -PlanArguments @($CommandArguments[4],$CommandArguments[4]) -InvocationMode 'compatibility_reuse_probe'
        return
    }
    if ($CommandArguments.Count -gt 0 -and $CommandArguments[0] -eq 'batch') {
        if ($CommandArguments.Count -lt 7 -or
            $CommandArguments[1] -ne '--checkpoint' -or
            (($CommandArguments.Count - 3) % 2) -ne 0) {
            throw 'Mirror batch requires --checkpoint <checkpoint.json> followed by two to sixty-four --plan <extended-plan.json> members.'
        }
        $planArguments = [Collections.Generic.List[string]]::new()
        for ($index = 3; $index -lt $CommandArguments.Count; $index += 2) {
            if ($CommandArguments[$index] -ne '--plan' -or
                [string]::IsNullOrWhiteSpace($CommandArguments[$index + 1])) {
                throw 'Every mirror batch member must be one ordered --plan <extended-plan.json> pair.'
            }
            $planArguments.Add($CommandArguments[$index + 1])
        }
        Invoke-JavaPlayMirrorCheckpointBatch -CheckpointArgument $CommandArguments[2] -PlanArguments $planArguments.ToArray() -InvocationMode 'batch'
        return
    }
    throw 'Usage: .\java-play.ps1 mirror plan ... | .\java-play.ps1 mirror run --plan <plan> | .\java-play.ps1 mirror checkpoint --plan <plan> | .\java-play.ps1 mirror suffix --checkpoint <checkpoint.json> --plan <extended-plan> | .\java-play.ps1 mirror batch --checkpoint <checkpoint.json> --plan <extended-plan> --plan <extended-plan> [...]'
}

if ($Arguments.Count -eq 0 -or $Arguments[0] -in @('help','--help','-h')) {
    Show-JavaPlayHelp
    exit 0
}

$command = $Arguments[0].ToLowerInvariant()
$rest = @($Arguments | Select-Object -Skip 1)
switch ($command) {
    'build' { Invoke-JavaPlayBuild }
    'install' { Invoke-JavaPlayInstall }
    'launch' { Invoke-JavaPlayLaunch $rest }
    'status' { Invoke-LiveBridge (@('status','--output-dir',$liveOutput) + $rest) }
    'act' { Invoke-LiveBridge (@('act','--output-dir',$liveOutput) + $rest) }
    'start-run' { Invoke-LiveBridge (@('start-run','--output-dir',$liveOutput) + $rest) }
    'continue-run' { Invoke-LiveBridge (@('continue-run','--output-dir',$liveOutput) + $rest) }
    'hand-select' { Invoke-LiveBridge (@('hand-select','--output-dir',$liveOutput) + $rest) }
    'grid-select' { Invoke-LiveBridge (@('grid-select','--output-dir',$liveOutput) + $rest) }
    'probe' { Invoke-JavaPlayProbe $rest }
    'mirror' { Invoke-JavaPlayMirror $rest }
    'paired-root' { Invoke-JavaPlayPairedRoot $rest }
    'scenario' { Invoke-JavaPlayScenario $rest }
    'doctor' { Invoke-JavaPlayDoctor }
    default {
        Write-Error "Unknown java-play command: $command"
        Show-JavaPlayHelp
        exit 2
    }
}
