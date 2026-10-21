param(
    [string]$Python = "python",
    [string]$MaturinPython = "python",
    [switch]$InstallTarget,
    [switch]$Fast
)

$ErrorActionPreference = "Stop"

$bridgeRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $bridgeRoot "..\..")).Path
$runId = (Get-Date -Format "yyyyMMdd-HHmmss") + "-" + [Guid]::NewGuid().ToString("N").Substring(0, 8)
$runRoot = Join-Path $repositoryRoot ".oracle-lab\python-learning-bridge\$runId"
$wheelRoot = Join-Path $runRoot "wheels"
$venvRoot = Join-Path $runRoot "venv"
$buildLog = Join-Path $runRoot "build.log"
$smokeLog = Join-Path $runRoot "smoke.log"
$callerSmokeLog = Join-Path $runRoot "caller-smoke.log"
$targetInstallLog = Join-Path $runRoot "target-install.log"
$bridgeTarget = Join-Path $bridgeRoot "target"
$totalWatch = [Diagnostics.Stopwatch]::StartNew()
$phaseWatch = [Diagnostics.Stopwatch]::new()
$targetInstallSeconds = 0.0

New-Item -ItemType Directory -Path $wheelRoot -Force | Out-Null

$pythonPath = (& $Python -c "import sys; print(sys.executable)").Trim()
if ($LASTEXITCODE -ne 0 -or -not $pythonPath) {
    throw "failed to resolve target Python executable"
}
$pythonSitePackages = (& $pythonPath -c "import sysconfig; print(sysconfig.get_path('purelib'))").Trim()
if ($LASTEXITCODE -ne 0 -or -not $pythonSitePackages -or -not (Test-Path -LiteralPath $pythonSitePackages -PathType Container)) {
    throw "failed to resolve target Python dependency directory"
}

$env:PYO3_PYTHON = $pythonPath
$savedErrorPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$phaseWatch.Restart()
$maturinArgs = @(
    "-m", "maturin", "build",
    "--manifest-path", (Join-Path $bridgeRoot "Cargo.toml"),
    "--interpreter", $pythonPath,
    "--target-dir", $bridgeTarget,
    "--out", $wheelRoot
)
if (-not $Fast) {
    $maturinArgs += "--release"
}
& $MaturinPython @maturinArgs *> $buildLog
$buildExit = $LASTEXITCODE
$phaseWatch.Stop()
$buildSeconds = $phaseWatch.Elapsed.TotalSeconds
$ErrorActionPreference = $savedErrorPreference
if ($buildExit -ne 0) {
    Get-Content -LiteralPath $buildLog -Tail 80
    throw "Maturin wheel build failed; full log: $buildLog"
}

$wheel = Get-ChildItem -LiteralPath $wheelRoot -Filter "*.whl" | Select-Object -First 1
if (-not $wheel) {
    throw "Maturin completed without producing a wheel"
}

$ErrorActionPreference = "Continue"
$phaseWatch.Restart()
& $pythonPath -m venv --system-site-packages $venvRoot *>> $buildLog
$venvExit = $LASTEXITCODE
$phaseWatch.Stop()
$venvSeconds = $phaseWatch.Elapsed.TotalSeconds
$ErrorActionPreference = $savedErrorPreference
if ($venvExit -ne 0) {
    throw "failed to create isolated Python environment: $venvRoot"
}
$venvPython = Join-Path $venvRoot "Scripts\python.exe"

$ErrorActionPreference = "Continue"
$phaseWatch.Restart()
& $venvPython -m pip install --disable-pip-version-check --ignore-installed --no-deps $wheel.FullName *> $smokeLog
$installExit = $LASTEXITCODE
$phaseWatch.Stop()
$isolatedInstallSeconds = $phaseWatch.Elapsed.TotalSeconds
$ErrorActionPreference = $savedErrorPreference
if ($installExit -ne 0) {
    Get-Content -LiteralPath $smokeLog -Tail 80
    throw "isolated wheel installation failed; full log: $smokeLog"
}

$ErrorActionPreference = "Continue"
$phaseWatch.Restart()
& $venvPython (Join-Path $bridgeRoot "tests\smoke.py") *>> $smokeLog
$smokeExit = $LASTEXITCODE
$phaseWatch.Stop()
$smokeSeconds = $phaseWatch.Elapsed.TotalSeconds
$ErrorActionPreference = $savedErrorPreference
if ($smokeExit -ne 0) {
    Get-Content -LiteralPath $smokeLog -Tail 80
    throw "Python learning bridge smoke failed; full log: $smokeLog"
}

$savedPythonPath = [Environment]::GetEnvironmentVariable("PYTHONPATH", "Process")
$learningSource = (Resolve-Path -LiteralPath (Join-Path $repositoryRoot "learning\src")).Path
$venvSite = (& $venvPython -c "import sysconfig; print(sysconfig.get_path('purelib'))").Trim()
if ($LASTEXITCODE -ne 0 -or -not $venvSite) {
    throw "failed to resolve isolated wheel site-packages"
}
# Keep the isolated wheel first so this verification cannot import the
# already-installed target bridge. The caller smoke requires only NumPy and the
# repository's pure-Python online batch driver; the full pytest suite belongs to
# `learning/dev.ps1 test` and `verify`.
$env:PYTHONPATH = "$venvSite;$pythonSitePackages;$learningSource;$repositoryRoot"
$ErrorActionPreference = "Continue"
$phaseWatch.Restart()
& $venvPython (Join-Path $bridgeRoot "tests\caller_smoke.py") $venvRoot *> $callerSmokeLog
$callerSmokeExit = $LASTEXITCODE
$phaseWatch.Stop()
$callerSmokeSeconds = $phaseWatch.Elapsed.TotalSeconds
$ErrorActionPreference = $savedErrorPreference
[Environment]::SetEnvironmentVariable("PYTHONPATH", $savedPythonPath, "Process")
if ($callerSmokeExit -ne 0) {
    Get-Content -LiteralPath $callerSmokeLog -Tail 80
    throw "Python learning caller smoke failed; full log: $callerSmokeLog"
}

if ($InstallTarget) {
    $ErrorActionPreference = "Continue"
    $phaseWatch.Restart()
    & $pythonPath -m pip install `
        --disable-pip-version-check `
        --force-reinstall `
        --no-deps `
        $wheel.FullName *> $targetInstallLog
    $targetInstallExit = $LASTEXITCODE
    $phaseWatch.Stop()
    $targetInstallSeconds = $phaseWatch.Elapsed.TotalSeconds
    $ErrorActionPreference = $savedErrorPreference
    if ($targetInstallExit -ne 0) {
        Get-Content -LiteralPath $targetInstallLog -Tail 80
        throw "target wheel refresh failed; full log: $targetInstallLog"
    }
}

$summary = Get-Content -LiteralPath $smokeLog |
    Where-Object { $_ -match "^python_learning_bridge_smoke " } |
    Select-Object -Last 1
if (-not $summary) {
    throw "smoke completed without its compact summary; full log: $smokeLog"
}

Write-Output $summary
Write-Output ("python=" + $pythonPath)
Write-Output ("wheel=" + $wheel.Name)
if ($Fast) {
    Write-Output "build_profile=dev"
}
else {
    Write-Output "build_profile=release"
}
Write-Output "rust_contracts=owned_by_workspace_crates"
Write-Output "isolated_caller_smoke=passed"
if ($InstallTarget) {
    Write-Output "target_install=refreshed"
}
$totalWatch.Stop()
Write-Output (
    "timing_seconds=" +
    "build:" + [math]::Round($buildSeconds, 3) + "," +
    "venv:" + [math]::Round($venvSeconds, 3) + "," +
    "isolated_install:" + [math]::Round($isolatedInstallSeconds, 3) + "," +
    "smoke:" + [math]::Round($smokeSeconds, 3) + "," +
    "caller_smoke:" + [math]::Round($callerSmokeSeconds, 3) + "," +
    "target_install:" + [math]::Round($targetInstallSeconds, 3) + "," +
    "total:" + [math]::Round($totalWatch.Elapsed.TotalSeconds, 3)
)
$resolvedRunRoot = (Resolve-Path -LiteralPath $runRoot).Path
$artifactParent = [IO.Path]::GetFullPath((Join-Path $repositoryRoot '.oracle-lab\python-learning-bridge')) + [IO.Path]::DirectorySeparatorChar
if (-not $resolvedRunRoot.StartsWith($artifactParent, [StringComparison]::OrdinalIgnoreCase)) {
    throw "verification cleanup escaped its artifact directory: $resolvedRunRoot"
}
Remove-Item -LiteralPath $resolvedRunRoot -Recurse -Force
Write-Output "success_artifacts=cleaned"
