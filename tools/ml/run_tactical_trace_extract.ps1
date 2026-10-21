<#
.SYNOPSIS
Exports CombatTacticalEpisodeV1 JSONL from turn-plan guidance-lab reports.

.EXAMPLE
.\tools\ml\run_tactical_trace_extract.ps1
Discovers tools\artifacts\tmp\*.turn_plan_lab.json and writes one tactical
episode JSONL.

.EXAMPLE
.\tools\ml\run_tactical_trace_extract.ps1 -LabPath tools\artifacts\tmp\tactical_v1_smoke.turn_plan_lab.json -CaseLimit 4
Runs the extractor on a specific lab file and prints a short preview.
#>
param(
    [Alias("Input")]
    [string[]] $LabPath = @(),
    [string] $ProbeRoot = "tools\artifacts\tmp",
    [string] $Output = "tools\artifacts\tmp\combat_tactical_episodes.jsonl",
    [int] $CaseLimit = 12,
    [ValidateSet("compact", "diagnostic")]
    [string] $ReportMode = "compact",
    [switch] $SummaryOnly,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $ExtraArgs
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$ScriptPath = Join-Path $RepoRoot "tools\ml\combat_tactical_trace_extract.py"

if ($LabPath.Count -eq 0) {
    $LabPath = Get-ChildItem -LiteralPath (Join-Path $RepoRoot $ProbeRoot) -Filter "*.turn_plan_lab.json" |
        Sort-Object LastWriteTime -Descending |
        ForEach-Object { $_.FullName }
}

if ($LabPath.Count -eq 0) {
    throw "No *.turn_plan_lab.json files found under $ProbeRoot. Generate turn-plan guidance labs first."
}

$ArgsList = @("$ScriptPath")
$ArgsList += $LabPath
$ArgsList += @("--out-jsonl", $Output, "--case-limit", "$CaseLimit", "--report-mode", $ReportMode)
if ($SummaryOnly) {
    $ArgsList += "--summary-only"
}
if ($ExtraArgs) {
    $ArgsList += $ExtraArgs
}

Write-Host "tactical trace extract: inputs=$($LabPath.Count) output=$Output cases=$CaseLimit report=$ReportMode summary_only=$SummaryOnly"
python @ArgsList
