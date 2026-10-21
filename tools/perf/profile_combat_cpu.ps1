<#
.SYNOPSIS
Collects a short, symbolized WPR CPU trace for the canonical combat contract.

.DESCRIPTION
Build and preflight run happen without elevation. The script then launches a
short-lived elevated copy of itself solely to own one uniquely named WPR
recording and the repeated combat workload. It never cancels another WPR
instance and stores all generated artifacts under the ignored `.profiles`
directory.

.EXAMPLE
.\tools\perf\profile_combat_cpu.ps1

.EXAMPLE
.\tools\perf\profile_combat_cpu.ps1 -PrepareOnly
Builds and validates the workload without starting WPR or requesting UAC.

.EXAMPLE
.\tools\perf\profile_combat_cpu.ps1 -CompressTrace
Trades a slower WPR stop for a smaller ETL file.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [string] $Case = ".oracle-lab\cases\seed022-a2f32-collector-full-hp.combat.json",
    [ValidateRange(1, 200)]
    [int] $Iterations = 24,
    [switch] $SkipBuild,
    [switch] $PrepareOnly,
    [switch] $CompressTrace,
    [Parameter(DontShow = $true)]
    [string] $ElevatedRequest
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "combat_contract_workload.ps1")
. (Join-Path $PSScriptRoot "combat_contract_build_receipt.ps1")
. (Join-Path $PSScriptRoot "native_symbol_cache.ps1")

function Test-IsAdministrator {
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $Principal = [Security.Principal.WindowsPrincipal] $Identity
    return $Principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-WprCommand(
    [string[]] $Arguments,
    [ValidateRange(1, 300)]
    [int] $TimeoutSeconds
) {
    $StartInfo = [Diagnostics.ProcessStartInfo]::new()
    $StartInfo.FileName = (Get-Command wpr.exe -ErrorAction Stop).Source
    $StartInfo.UseShellExecute = $false
    $StartInfo.CreateNoWindow = $true
    $StartInfo.RedirectStandardInput = $true
    $StartInfo.RedirectStandardOutput = $true
    $StartInfo.RedirectStandardError = $true
    foreach ($Argument in $Arguments) {
        $StartInfo.ArgumentList.Add($Argument)
    }

    $Process = [Diagnostics.Process]::new()
    $Process.StartInfo = $StartInfo
    if (-not $Process.Start()) {
        throw "failed to start wpr.exe"
    }
    $StandardOutput = $Process.StandardOutput.ReadToEndAsync()
    $StandardError = $Process.StandardError.ReadToEndAsync()
    # WPR asks whether it may stop an incompatible existing recording. Closing
    # stdin after an explicit "N" makes the safe answer deterministic instead
    # of leaving an invisible elevated process blocked on a console prompt.
    $Process.StandardInput.WriteLine("N")
    $Process.StandardInput.Close()

    $TimedOut = -not $Process.WaitForExit($TimeoutSeconds * 1000)
    if ($TimedOut) {
        try {
            $Process.Kill($true)
        }
        catch {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
        $Process.WaitForExit()
    }

    return [pscustomobject]@{
        timed_out = $TimedOut
        exit_code = if ($TimedOut) { $null } else { $Process.ExitCode }
        output = @(
            $StandardOutput.GetAwaiter().GetResult(),
            $StandardError.GetAwaiter().GetResult()
        ).Where({ -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine
    }
}

function Invoke-ElevatedCapture([string] $RequestPath) {
    if (-not (Test-IsAdministrator)) {
        throw "the WPR capture child did not receive an elevated Windows token"
    }

    $ScriptRepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
    $RepoRoot = [IO.Path]::GetFullPath($ScriptRepoRoot)
    $ProfileRoot = [IO.Path]::GetFullPath((Join-Path $RepoRoot ".profiles"))
    $WprProfilePath = [IO.Path]::GetFullPath(
        (Join-Path $RepoRoot "tools\perf\sts_combat_cpu.wprp")
    )
    $RequestPath = [IO.Path]::GetFullPath($RequestPath)
    if (-not $RequestPath.StartsWith($ProfileRoot + [IO.Path]::DirectorySeparatorChar,
            [StringComparison]::OrdinalIgnoreCase)) {
        throw "profiling request must remain below '$ProfileRoot'"
    }

    $Request = Get-Content -LiteralPath $RequestPath -Raw | ConvertFrom-Json
    if ($Request.schema_name -ne "StsCombatCpuProfileRequestV1") {
        throw "unsupported profiling request schema '$($Request.schema_name)'"
    }

    if ([IO.Path]::GetFullPath([string] $Request.repo_root) -ne $RepoRoot) {
        throw "profiling request repository does not match this script"
    }
    $Executable = [IO.Path]::GetFullPath([string] $Request.executable)
    $ExpectedExecutable = [IO.Path]::GetFullPath(
        (Join-Path $RepoRoot "target\profiling\combat_contract.exe")
    )
    $RequestedWprProfilePath = [IO.Path]::GetFullPath([string] $Request.wpr_profile_path)
    $CasePath = [IO.Path]::GetFullPath([string] $Request.case_path)
    $TracePath = [IO.Path]::GetFullPath([string] $Request.trace_path)
    $MetadataPath = [IO.Path]::GetFullPath([string] $Request.metadata_path)
    $Iterations = [int] $Request.iterations
    $CompressTrace = [bool] $Request.compress_trace

    if ($Executable -ne $ExpectedExecutable) {
        throw "profiling request may execute only '$ExpectedExecutable'"
    }
    if ($RequestedWprProfilePath -ne $WprProfilePath) {
        throw "profiling request may use only '$WprProfilePath'"
    }
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "symbolized combat contract is missing at '$Executable'"
    }
    $ActualExecutableHash = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash
    if ($ActualExecutableHash -ne [string] $Request.executable_sha256) {
        throw "profiling executable changed after the request was validated"
    }
    if (-not (Test-Path -LiteralPath $WprProfilePath -PathType Leaf)) {
        throw "STS combat WPR profile is missing at '$WprProfilePath'"
    }
    if (-not (Test-Path -LiteralPath $CasePath -PathType Leaf)) {
        throw "combat case is missing at '$CasePath'"
    }
    foreach ($OutputPath in @($TracePath, $MetadataPath)) {
        if (-not $OutputPath.StartsWith($ProfileRoot + [IO.Path]::DirectorySeparatorChar,
                [StringComparison]::OrdinalIgnoreCase)) {
            throw "profiling output must remain below '$ProfileRoot'"
        }
    }
    if ($Iterations -lt 1 -or $Iterations -gt 200) {
        throw "profiling iterations must be between 1 and 200"
    }

    New-Item -ItemType Directory -Path $ProfileRoot -Force | Out-Null
    $InstanceName = "StsSimulatorCombatCpu-$PID"
    $WorkloadArguments = Get-StsCombatContractWorkloadArguments $CasePath
    $RecordingStarted = $false
    $CaptureError = $null
    $StopError = $null
    $StartedAt = [DateTimeOffset]::UtcNow
    $StartDurationMs = $null
    $WorkloadDurationMs = $null
    $StopDurationMs = $null

    try {
        # A short CPU capture fits comfortably in WPR's bounded memory mode;
        # file mode is intentionally avoided because it creates an unbounded
        # temporary trace while the workload runs.
        $StartTimer = [Diagnostics.Stopwatch]::StartNew()
        $StartResult = Invoke-WprCommand `
            -Arguments @(
                "-start", "$WprProfilePath!StsCombatCpu.Verbose",
                "-instancename", $InstanceName
            ) `
            -TimeoutSeconds 20
        $StartTimer.Stop()
        $StartDurationMs = $StartTimer.Elapsed.TotalMilliseconds
        if ($StartResult.timed_out) {
            throw "WPR start exceeded 20 seconds and its process was terminated"
        }
        if ($StartResult.exit_code -ne 0) {
            throw "WPR start failed without replacing any existing recording: $($StartResult.output)"
        }
        $RecordingStarted = $true

        $WorkloadTimer = [Diagnostics.Stopwatch]::StartNew()
        for ($Index = 1; $Index -le $Iterations; $Index++) {
            & $Executable @WorkloadArguments *> $null
            if ($LASTEXITCODE -ne 0) {
                throw "combat contract failed during profiled iteration $Index/$Iterations"
            }
        }
        $WorkloadTimer.Stop()
        $WorkloadDurationMs = $WorkloadTimer.Elapsed.TotalMilliseconds
    }
    catch {
        $CaptureError = $_
    }
    finally {
        if ($RecordingStarted) {
            $StopArguments = [Collections.Generic.List[string]]::new()
            foreach ($Argument in @(
                    "-stop", $TracePath,
                    "sts_simulator_symbolized_combat_CPU_profile"
                )) {
                $StopArguments.Add($Argument)
            }
            if ($CompressTrace) {
                $StopArguments.Add("-compress")
            }
            # Rust PDBs already exist beside the profiled executable. Dynamic
            # NGEN/embedded-PDB generation is irrelevant here and makes WPR's
            # stop phase substantially slower on a desktop with .NET apps.
            $StopArguments.Add("-skipPdbGen")
            $StopArguments.Add("-instancename")
            $StopArguments.Add($InstanceName)

            $StopTimer = [Diagnostics.Stopwatch]::StartNew()
            $StopResult = Invoke-WprCommand `
                -Arguments $StopArguments.ToArray() `
                -TimeoutSeconds 90
            $StopTimer.Stop()
            $StopDurationMs = $StopTimer.Elapsed.TotalMilliseconds
            if ($StopResult.timed_out -or $StopResult.exit_code -ne 0) {
                $StopError = if ($StopResult.timed_out) {
                    "WPR stop exceeded 90 seconds"
                } else {
                    "WPR stop failed: $($StopResult.output)"
                }
                Invoke-WprCommand `
                    -Arguments @("-cancel", "-instancename", $InstanceName) `
                    -TimeoutSeconds 20 | Out-Null
            }
        }
    }

    $Metadata = [ordered]@{
        schema_name = "StsCombatCpuProfileV1"
        schema_version = 1
        recorded_at_utc = $StartedAt.ToString("O")
        duration_ms = [math]::Round(
            ([DateTimeOffset]::UtcNow - $StartedAt).TotalMilliseconds,
            3
        )
        start_duration_ms = if ($null -eq $StartDurationMs) {
            $null
        } else {
            [math]::Round($StartDurationMs, 3)
        }
        workload_duration_ms = if ($null -eq $WorkloadDurationMs) {
            $null
        } else {
            [math]::Round($WorkloadDurationMs, 3)
        }
        stop_duration_ms = if ($null -eq $StopDurationMs) {
            $null
        } else {
            [math]::Round($StopDurationMs, 3)
        }
        trace_path = $TracePath
        trace_bytes = if (Test-Path -LiteralPath $TracePath -PathType Leaf) {
            (Get-Item -LiteralPath $TracePath).Length
        } else {
            $null
        }
        trace_compressed = $CompressTrace
        case_path = $CasePath
        executable = $Executable
        iterations = $Iterations
        wpr_profile = "StsCombatCpu.Verbose"
        wpr_profile_path = $WprProfilePath
        wpr_logging_mode = "memory"
        wpr_instance = $InstanceName
        git_commit = [string] $Request.git_commit
        git_dirty = [bool] $Request.git_dirty
        build_git_commit = [string] $Request.build_git_commit
        build_git_dirty = [bool] $Request.build_git_dirty
        build_source_fingerprint = [string] $Request.build_source_fingerprint
        executable_sha256 = [string] $Request.executable_sha256
        pdb_sha256 = [string] $Request.pdb_sha256
        symbol_key = [string] $Request.symbol_key
        cached_pdb = [string] $Request.cached_pdb
        capture_succeeded = ($null -eq $CaptureError -and $null -eq $StopError)
        capture_error = if ($null -eq $CaptureError) { $null } else { $CaptureError.ToString() }
        stop_error = $StopError
    }
    $Metadata | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $MetadataPath -Encoding utf8

    if ($null -ne $CaptureError) {
        throw $CaptureError
    }
    if ($null -ne $StopError) {
        throw $StopError
    }

    Write-Host "WPR CPU trace: $TracePath"
    Write-Host "Profile metadata: $MetadataPath"
}

if ($ElevatedRequest) {
    Invoke-ElevatedCapture ([IO.Path]::GetFullPath($ElevatedRequest))
    exit 0
}

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$RepoRoot = [IO.Path]::GetFullPath($RepoRoot)
$ProfileRoot = Join-Path $RepoRoot ".profiles"
$WprProfilePath = Join-Path $RepoRoot "tools\perf\sts_combat_cpu.wprp"
New-Item -ItemType Directory -Path $ProfileRoot -Force | Out-Null

$CasePath = if ([IO.Path]::IsPathRooted($Case)) {
    [IO.Path]::GetFullPath($Case)
} else {
    [IO.Path]::GetFullPath((Join-Path $RepoRoot $Case))
}
if (-not (Test-Path -LiteralPath $CasePath -PathType Leaf)) {
    throw "combat case is missing at '$CasePath'"
}
if (-not (Test-Path -LiteralPath $WprProfilePath -PathType Leaf)) {
    throw "STS combat WPR profile is missing at '$WprProfilePath'"
}

Push-Location $RepoRoot
try {
    if (-not $SkipBuild) {
        & cargo build --locked --profile profiling -p sts_combat_contract --bin combat_contract
        if ($LASTEXITCODE -ne 0) {
            throw "symbolized combat contract build failed"
        }
    }

    $Executable = Join-Path $RepoRoot "target\profiling\combat_contract.exe"
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "symbolized combat contract is missing at '$Executable'; rerun without -SkipBuild"
    }
    $BuildReceipt = if ($SkipBuild) {
        Assert-StsCombatContractBuildReceipt $RepoRoot $Executable
    }
    else {
        Write-StsCombatContractBuildReceipt $RepoRoot $Executable
    }

    # Cache this exact build's PDB before another build can replace it. PerfView
    # otherwise asks its GUI-backed PDB matcher to inspect the adjacent file;
    # that helper is not reliable from a headless PowerShell process. The
    # symbol-server key is read from the executable's RSDS record, so every
    # captured build remains independently recoverable.
    $PublishedSymbols = Publish-NativePdbToSymbolCache `
        $Executable (Join-Path $ProfileRoot "symbol-cache")

    $WorkloadArguments = Get-StsCombatContractWorkloadArguments $CasePath
    & $Executable @WorkloadArguments *> $null
    if ($LASTEXITCODE -ne 0) {
        throw "combat contract preflight failed; WPR was not started"
    }

    $GitCommit = (& git rev-parse HEAD).Trim()
    $GitDirty = -not [string]::IsNullOrWhiteSpace((& git status --porcelain) -join "`n")
    $Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $ShortCommit = $GitCommit.Substring(0, [Math]::Min(8, $GitCommit.Length))
    $BaseName = "combat-cpu-$Timestamp-$ShortCommit"
    $TracePath = Join-Path $ProfileRoot "$BaseName.etl"
    $MetadataPath = Join-Path $ProfileRoot "$BaseName.json"
    $RequestPath = Join-Path $ProfileRoot ".$BaseName.request.json"

    $Request = [ordered]@{
        schema_name = "StsCombatCpuProfileRequestV1"
        schema_version = 1
        repo_root = $RepoRoot
        executable = $Executable
        case_path = $CasePath
        iterations = $Iterations
        compress_trace = [bool] $CompressTrace
        wpr_profile_path = $WprProfilePath
        trace_path = $TracePath
        metadata_path = $MetadataPath
        git_commit = $GitCommit
        git_dirty = $GitDirty
        build_git_commit = $BuildReceipt.git_commit
        build_git_dirty = $BuildReceipt.git_dirty
        build_source_fingerprint = $BuildReceipt.source_fingerprint
        executable_sha256 = $BuildReceipt.executable_sha256
        pdb_sha256 = $BuildReceipt.pdb_sha256
        symbol_key = $PublishedSymbols.identity.symbol_key
        cached_pdb = $PublishedSymbols.cached
    }
    $Request | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $RequestPath -Encoding utf8

    Write-Host "symbolized profile workload is ready"
    Write-Host "case: $CasePath"
    Write-Host "iterations: $Iterations"
    Write-Host "trace: $TracePath"
    Write-Host "symbols: $($PublishedSymbols.identity.symbol_key)"
    if ($PrepareOnly) {
        Remove-Item -LiteralPath $RequestPath -Force
        Write-Host "prepare-only: WPR was not started and no UAC request was made"
        exit 0
    }

    $PowerShell = (Get-Process -Id $PID).Path
    $QuotedScript = '"{0}"' -f $PSCommandPath
    $QuotedRequest = '"{0}"' -f $RequestPath
    $Child = Start-Process -FilePath $PowerShell -Verb RunAs -PassThru -Wait `
        -ArgumentList @(
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", $QuotedScript,
            "-ElevatedRequest", $QuotedRequest
        )
    if ($Child.ExitCode -ne 0) {
        throw "elevated profiling child failed with exit code $($Child.ExitCode)"
    }
    Write-Host "capture complete: $TracePath"
}
finally {
    if ($RequestPath -and (Test-Path -LiteralPath $RequestPath)) {
        Remove-Item -LiteralPath $RequestPath -Force
    }
    Pop-Location
}
