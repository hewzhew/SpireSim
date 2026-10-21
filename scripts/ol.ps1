#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
. (Join-Path $repository 'tools\cargo_artifact_contract.ps1')

function Show-OracleHelp {
    @'
Usage: .\scripts\ol.ps1 <command> ...

Fast control commands (never invoke Cargo implicitly):
  .\scripts\ol.ps1 help
  .\scripts\ol.ps1 build help     # lightweight typed V2 help only
  .\scripts\ol.ps1 build report   # dependency-light durable evidence reader
  .\scripts\ol.ps1 build oracle   # canonical heavy host plus help frontend
  .\scripts\ol.ps1 build play
  .\scripts\ol.ps1 build all

Existing evidence inspection (never runs search):
  .\scripts\ol.ps1 report census <report.json> [--episode N] [--attempt N]
      [--encounter ID] [--floor N] [--json]
  .\scripts\ol.ps1 report resident-corridor <report.json> [--json]

Exact combat play, directly from a case (no search artifact required):
  .\scripts\ol.ps1 play new --case <case.json> --session <session.json> [--allow-potions]
  .\scripts\ol.ps1 play show <session.json>
  .\scripts\ol.ps1 play choose <session.json> <plan-index>
  .\scripts\ol.ps1 play back <session.json>
  .\scripts\ol.ps1 play reset <session.json>
  .\scripts\ol.ps1 play history <session.json>
  .\scripts\ol.ps1 play storyboard <session.json> [--json-output <fresh-storyboard.json>]
  .\scripts\ol.ps1 play agenda-corridor <session.json> [--depth N] [--max-states N]
  .\scripts\ol.ps1 play resident-corridor-audit <winning-session.json>
      [--from-turn-index N] [--generation-work N] [--wall-ms N]
      [--satisfaction zero-loss-or-budget] --output <fresh-audit.json>
  .\scripts\ol.ps1 play export-winning-actions <session.json> --output <actions.json>
  .\scripts\ol.ps1 play export-winning-demonstration <session.json> --output <demonstration.json>

All other commands invoke the already-built canonical oracle laboratory. The
interpreted launcher and host reject missing or stale artifacts with an
explicit build instruction; no command starts a build behind your back.
'@
}

function Show-PlayHelp {
    @'
Usage: .\scripts\ol.ps1 play <new|show|choose|back|reset|history|storyboard|agenda-corridor|resident-corridor-audit|export-winning-actions|export-winning-demonstration> ...

  new --case <case.json> --session <session.json> [--allow-potions] [--limit N]
  show <session.json> [--limit N]
  choose <session.json> <plan-index> [--limit N]
  back <session.json>
  reset <session.json>
  history <session.json>
  storyboard <session.json> [--json-output <fresh-storyboard.json>]
  agenda-corridor <session.json> [--depth N] [--max-states N]
  resident-corridor-audit <winning-session.json> [--from-turn-index N]
      [--generation-work N] [--wall-ms N]
      [--satisfaction first-witness|zero-loss-or-budget|budget-or-exhaustion]
      --output <fresh-audit.json>
  export-winning-actions <session.json> --output <actions.json>
  export-winning-demonstration <session.json> --output <demonstration.json>

The session stores only the exact case identity and chosen turn inputs. Every
command reloads the case and exact-replays that short history before acting.
Winning-action export succeeds only after that replay reaches a terminal win.
Winning-demonstration export additionally projects every retained complete-turn
candidate to public successor evidence; unchosen candidates remain unknown.
Storyboard prints exact per-turn state and named actions; optional JSON output
uses create-new semantics and never reruns combat search.
Resident corridor audit runs only the resident turn graph, records membership,
edge rank, and service facts for the verified teacher suffix, and never invokes
an atomic fallback or commits an action.
'@
}

function Show-ReportHelp {
    @'
Usage:
  .\scripts\ol.ps1 report census <report.json> [--episode N] [--attempt N]
      [--encounter ID] [--floor N] [--json]
  .\scripts\ol.ps1 report resident-corridor <report.json> [--json]

The report reader validates and summarizes existing durable evidence. It does
not restore game state, run search, invoke Cargo, or modify the source report.
'@
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory)] [string] $Executable,
        [Parameter()] [string[]] $CommandArguments = @()
    )
    & $Executable @CommandArguments
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

if (-not $Arguments -or $Arguments.Count -eq 0 -or $Arguments[0] -in @('help', '--help', '-h')) {
    Show-OracleHelp
    exit 0
}

Push-Location -LiteralPath $repository
try {
    if ($Arguments[0] -eq 'build') {
        $target = if ($Arguments.Count -ge 2) { $Arguments[1] } else { 'all' }
        if ($target -notin @('help', 'report', 'oracle', 'play', 'all')) {
            throw "Unknown build target '$target'. Expected help, report, oracle, play, or all."
        }
        if ($target -in @('help', 'oracle', 'all')) {
            Invoke-Checked cargo @(
                'build', '--locked', '-p', 'sts_oracle_v2_cli', '--bin', 'oracle_v2_help'
            )
        }
        if ($target -in @('report', 'all')) {
            Invoke-Checked cargo @(
                'build', '--locked', '-p', 'sts_oracle_report', '--bin', 'oracle_report'
            )
        }
        if ($target -in @('oracle', 'all')) {
            Invoke-Checked cargo @(
                'build', '--locked', '--release', '-p', 'sts_oracle_lab',
                '--features', 'canonical-oracle-artifacts', '--bin', 'oracle_lab'
            )
        }
        if ($target -in @('play', 'all')) {
            Invoke-Checked cargo @(
                'build', '--locked', '--profile', 'dev-opt', '-p', 'sts_combat_play', '--bin', 'combat_play'
            )
        }
        exit 0
    }

    if ($Arguments[0] -eq 'play') {
        if ($Arguments.Count -eq 1 -or $Arguments[1] -in @('help', '--help', '-h')) {
            Show-PlayHelp
            exit 0
        }
        $play = Join-Path $repository 'target\dev-opt\combat_play.exe'
        if (-not (Test-Path -LiteralPath $play -PathType Leaf)) {
            throw 'Exact combat play is not built. Run: .\scripts\ol.ps1 build play'
        }
        Assert-CargoArtifactFresh -Executable $play -Repository $repository `
            -Label 'Exact combat play' -RebuildCommand '.\scripts\ol.ps1 build play' `
            -AdditionalInputs @('crates/sts_combat_play/Cargo.toml')
        Invoke-Checked $play $Arguments[1..($Arguments.Count - 1)]
        exit 0
    }

    if ($Arguments[0] -eq 'report') {
        if ($Arguments.Count -eq 1 -or $Arguments[1] -in @('help', '--help', '-h')) {
            Show-ReportHelp
            exit 0
        }
        $report = Join-Path $repository 'target\debug\oracle_report.exe'
        if (-not (Test-Path -LiteralPath $report -PathType Leaf)) {
            throw 'Durable evidence reader is not built. Run: .\scripts\ol.ps1 build report'
        }
        Assert-CargoArtifactFresh -Executable $report -Repository $repository `
            -Label 'Durable evidence reader' -RebuildCommand '.\scripts\ol.ps1 build report' `
            -AdditionalInputs @('crates/sts_oracle_report/Cargo.toml')
        Invoke-Checked $report $Arguments[1..($Arguments.Count - 1)]
        exit 0
    }

    if ($Arguments[0] -in @('contract', 'artifact', 'case')) {
        $typedHelp = Join-Path $repository 'target\debug\oracle_v2_help.exe'
        if (-not (Test-Path -LiteralPath $typedHelp -PathType Leaf)) {
            throw 'Routine V2 help frontend is not built. Run: .\scripts\ol.ps1 build help'
        }
        Assert-CargoArtifactFresh -Executable $typedHelp -Repository $repository `
            -Label 'Routine V2 help frontend' -RebuildCommand '.\scripts\ol.ps1 build help' `
            -AdditionalInputs @('crates/sts_oracle_v2_cli/Cargo.toml')
        $isHelpRequest = $Arguments | Where-Object { $_ -in @('--help', '-h') }
        Invoke-Checked $typedHelp $Arguments
        if ($isHelpRequest) {
            exit 0
        }
    }

    $oracle = Join-Path $repository 'target\release\oracle_lab.exe'
    if (-not (Test-Path -LiteralPath $oracle -PathType Leaf)) {
        throw 'Canonical oracle laboratory is not built. Run: .\scripts\ol.ps1 build oracle'
    }
    Assert-CargoArtifactFresh -Executable $oracle -Repository $repository `
        -Label 'Canonical oracle laboratory' -RebuildCommand '.\scripts\ol.ps1 build oracle' `
        -AdditionalInputs @('crates/sts_oracle_lab/Cargo.toml') `
        -SelfGuardInputs @(
            'crates/sts_oracle_lab/src/bin/oracle_lab.rs',
            'crates/sts_oracle_lab/src/bin/canonical_launch.rs',
            'crates/oracle_artifact_contract/src/lib.rs',
            'crates/oracle_artifact_contract/build-inputs/oracle-host.txt'
        )
    Invoke-Checked $oracle (@('--canonical-oracle') + $Arguments)
}
finally {
    Pop-Location
}
