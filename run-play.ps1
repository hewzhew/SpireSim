#requires -Version 7.0
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]] $Arguments
)

$ErrorActionPreference = 'Stop'
$repository = $PSScriptRoot
$workbench = Join-Path $repository 'target\dev-opt\run_play.exe'
. (Join-Path $repository 'tools\cargo_artifact_contract.ps1')

function Show-RunPlayHelp {
    param([Parameter()] [string] $Topic = '')

    $topics = [ordered]@{
        status = @'
.\p.ps1 [status] [--session <path>]
  Show the focused decision and its executable actions. With no focus, pass --session.
'@
        inventory = @'
.\p.ps1 inventory [--session <path>]
  Expand the complete master deck and held relics at any decision, including combat.
'@
        act = @'
.\p.ps1 act [--session <path>] [--at nNODE] ACTION [ACTION ...]
.\p.ps1 play PommelStrike+@1 play Disarm+@0
.\p.ps1 use DexterityPotion
.\p.ps1 select Whirlwind
  Selection rows show executable commands. For multiple cards, use
  select <card-token> <card-token> as prompted by the current selection.
  A leading action verb implies act. One action yields immediately; multiple actions
  are validated and published transactionally. --at branches from an exact
  retained node. Use advance only for one uniquely routine progression step;
  --auto-combat accepts one action.
'@
        preview = @'
.\p.ps1 preview [--session <path>] [--at nNODE] ACTION [ACTION ...]
  Read-only exact rehearsal using the same actions as act. The current head stays put.
  Entering an unrevealed ? room requires act; preview will not reveal its outcome.
'@
        show = @'
.\p.ps1 show [--session <path>]
  Expand the current decision and diagnostic evidence; status is the compact view.
'@
        map = @'
.\p.ps1 map [--session <path>]
.\p.ps1 map full [--session <path>]
.\p.ps1 map elites [--session <path>]
.\p.ps1 map route X Y [--session <path>]
  Read connected route examples; full expands all published nodes.
  Compare elite/key ranges and first-elite timing with elites/route;
  inspect later forks by coordinate. Public connections only; no combat forecast.
'@
        'deck-lab' = @'
.\p.ps1 deck-lab [--session <path>] --list
.\p.ps1 deck-lab [--session <path>] --output <fresh-report>.json `
  --encounter TheHeart --scenarios 3 --work 200000 --ms 8000
  Test the current out-of-combat deck with the live CombatSolver and fixed diagnostic RNG cases.
  The terminal shows witnesses, Unknown, observed HP and cost; full cases go to the report.
  Optional --full-hp or --max-potion-uses N changes the declared test condition.
'@
        info = @'
.\p.ps1 info [--session <path>] card|relic|potion <name>
  Read a named object's known mechanics without advancing the game.
'@
        options = @'
.\p.ps1 options [--session <path>]
  Show every currently executable action without the diagnostic state dump.
'@
        history = @'
.\p.ps1 history [--session <path>] --journey --first 8 --detail
.\p.ps1 history [--session <path>] --combat-detail --at nNODE
  Inspect retained journey or one combat boundary.
'@
        advice = @'
.\p.ps1 advice [--session <path>]
  Show owner advice for the current decision; it does not commit an action.
'@
        focus = @'
.\run-play.ps1 focus show|set|clear|migrate ...
.\run-play.ps1 focus set --session .oracle-lab/run-play/<session>
  Focus is a convenience pointer, not write authority. --session overrides it.
'@
        overview = @'
.\run-play.ps1 overview [--json]
  One recovery view of build identity, focus, handoff and recent sessions.
'@
        session = @'
.\run-play.ps1 session show|describe|park|unpark ...
  Inspect or enforce a session write boundary.
'@
        handoff = @'
.\run-play.ps1 handoff open|resolve|show ...
  Record or resolve a typed handoff for a retained session.
'@
        check = @'
.\run-play.ps1 check
  Check the Rust workbench without starting Java.
'@
        build = @'
.\run-play.ps1 build
  Explicitly build the dev-opt workbench. Other commands never build implicitly.
'@
        undo = @'
.\p.ps1 undo [--session <path>]
.\p.ps1 back --floor <N> [--session <path>]
  Correct one action or return to a retained floor end in a scratch run.
'@
        mark = @'
.\p.ps1 mark --name <label> [--session <path>]
.\p.ps1 mark [--session <path>]
.\p.ps1 mark --drop <label> [--session <path>]
.\p.ps1 back --mark <label> [--session <path>]
  Keep and revisit a few alternatives inside the same scratch session.
'@
        jump = @'
.\p.ps1 jump [--session <path>] <nNODE|source:hN|target:hN>
  Move to an exact retained decision. source:hN and target:hN select the
  decision before or after a recorded controller event at revision hN.
'@
        fork = @'
.\run-play.ps1 fork --session <source> --target <fresh-session> [--at nNODE]
.\run-play.ps1 fork --session <source> --target .oracle-lab/run-play-tmp/<fresh-probe> --disposable [--at nNODE]
  Create a retained or disposable branch from the selected exact node.
'@
        'promote-branch' = @'
.\run-play.ps1 promote-branch --source <verified-probe> --target <session>
  Replay the probe's active suffix from its nearest shared exact ancestor and
  publish only after typed boundaries match.
'@
        'counterfactual-branch' = @'
.\run-play.ps1 counterfactual-branch --session <parked-probe> --output <fresh-evidence>.json
  Record the parked probe's active suffix as read-only, root-bound reachability evidence.
'@
        'snapshot-current' = @'
.\run-play.ps1 snapshot-current --session <source> --output <fresh-capsule>.msgpack
  Save an exact current-state capsule for a later explicit reroot.
'@
        reroot = @'
.\run-play.ps1 reroot --snapshot <capsule>.msgpack --target <fresh-session>
  Create a current-build session from an exact saved capsule.
'@
        'reroot-current' = @'
.\p.ps1 reroot-current
.\p.ps1 reroot-current --session <old-session> [--at nNODE] [--target <fresh-session>] [--focus]
  Verify selected exact state and RNG, then create a current-build session
  without a capsule. Omit --at for the old head. With no arguments, use the
  focused session, generate a fresh name, and focus it. The old session remains unchanged.
'@
        'workspace-gc' = @'
.\run-play.ps1 workspace-gc summary
.\run-play.ps1 workspace-gc plan --output <fresh-plan>.json
.\run-play.ps1 workspace-gc execute --plan <plan>.json --receipt <fresh-execution>.jsonl
  Inspect, plan and explicitly execute run-play workspace cleanup.
'@
        'parity-rng' = @'
.\run-play.ps1 parity-rng --session <session> [--at nNODE]
  Privileged read-only Java/Rust parity view of run and active-combat RNG streams.
'@
        'replay-mirror-prefix' = @'
.\run-play.ps1 replay-mirror-prefix --plan <plan>.json --through-depth <N> --session <fresh-current-session>
  Replay a typed retained prefix in one workbench call, checking source boundaries.
'@
        'combat-research' = @'
.\p.ps1 combat-research [--session <session>] --effort fast-witness --artifact <fresh>.json [--detail]
  fast-witness stops at the first replay-verified win. standard/expanded use
  resident-first frontiers and potion caps. observed_best ranks observed
  verified candidates; it is not an optimality claim.
'@
        'combat-query' = @'
.\p.ps1 combat-query [--session <session>]
    [--eventually-generated-card <CardId>]... [--eventually-lethal-payoff Feed]...
    [--max-potions-used <N>] [--allowed-potion-slot <slot>]... [--output <fresh>.json]
  Read-only bounded product-state search; a miss remains Unknown.
'@
        'combat-damage-challenge' = @'
.\p.ps1 combat-damage-challenge [--session <session>] --artifact <research-or-query>.json --output <fresh>.json
  Reopen each turn-advancing HP loss from a replay-proved source, preserving its potion budget.
'@
        'combat-damage-anchor' = @'
.\p.ps1 combat-damage-anchor [--session <session>] --artifact <damage-challenge>.json --frontier 0
  Reproject and transactionally commit one global resource-frontier member.
'@
        'anchor-combat-plan' = @'
.\p.ps1 anchor-combat-plan [--session <session>] --artifact <same-root>.json
    [--candidate <global-index>] <resident|proposal|atomic>
  Commit one selected combat plan from same-root evidence.
'@
        'construction-history' = @'
.\run-play.ps1 construction-history --session <session>
    [--site card_reward] [--candidate-card IronWave] [--added-card RecklessCharge]
    [--all-boundaries] [--detail] [--json]
  Query retained deck-construction decisions.
'@
        'victory-dossier' = @'
.\run-play.ps1 victory-dossier --session <session> --output <fresh>.json
    [--transcript-source <file-or-flat-directory>]...
  Produce a victory evidence dossier from a retained session.
'@
        shell = @'
.\run-play.ps1 shell --session <session>
target\dev-opt\run_play.exe agent-shell --session <session>
  Long-lived workbench interaction. Build explicitly before launching.
'@
    }

    $aliases = @{
        play = 'act'; take = 'act'; go = 'act'; buy = 'act'; remove = 'act'
        use = 'focus'; resume = 'overview'; back = 'undo'; 'agent-shell' = 'shell'
    }
    $aliasSummary = 'Aliases: ' + (($aliases.Keys | Sort-Object | ForEach-Object {
        "$_ → $($aliases[$_])"
    }) -join ', ')
    $topicKey = $Topic.ToLowerInvariant()
    if ($topicKey -in @('--help', '-h')) { $topicKey = '' }
    if ($aliases.ContainsKey($topicKey)) { $topicKey = $aliases[$topicKey] }

    if ($topicKey -eq 'all') {
        'Usage: .\p.ps1 [command or action ...]'
        '       .\run-play.ps1 [command or action ...]'
        'Help topics:'
        foreach ($key in $topics.Keys) {
            "`n$key"
            $topics[$key]
        }
        $aliasSummary
        @'
All workbench commands use the dev-opt executable. The launcher and executable
validate build freshness before touching a session; only build rebuilds it.
Focus never grants write authority. Session park is the write boundary.
Use an explicit --session to override focus.
'@
    }
    elseif ($topicKey -eq 'topics') {
        'Help topics (help <topic> shows one):'
        $names = @($topics.Keys)
        for ($index = 0; $index -lt $names.Count; $index += 5) {
            $last = [Math]::Min($index + 4, $names.Count - 1)
            '  ' + ($names[$index..$last] -join ' ')
        }
        $aliasSummary
        'Use help all to see every topic and the shared execution rules.'
    }
    elseif ($topicKey -eq '') {
        @'
Run-play: .\p.ps1 uses the focused session; --session <path> overrides it.
  .\p.ps1                         current status and executable actions
  .\p.ps1 play Card@target ...    commit one or more actions
  .\p.ps1 preview ACTION ...      rehearse without changing the head
  .\p.ps1 map                     compact public map
  .\p.ps1 info card <name>        inspect known mechanics
  .\p.ps1 help <command>          exact command usage
  .\p.ps1 help topics             list every help topic
  .\p.ps1 help all                all topics and shared rules
Build and session repair: help build, help focus, help reroot-current.
No help command builds the workbench or changes a game session.
'@
    }
    elseif ($topics.Contains($topicKey)) {
        "Help: $topicKey"
        $topics[$topicKey]
        'Use help all to see every topic.'
    }
    else {
        [Console]::Error.WriteLine("Unknown help topic '$Topic'. Use: .\p.ps1 help topics")
        exit 2
    }
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory)] [string] $Executable,
        [Parameter()] [string[]] $CommandArguments = @()
    )
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.UseShellExecute = $false
    foreach ($argument in $CommandArguments) {
        [void] $startInfo.ArgumentList.Add($argument)
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $exitCode = $null
    $started = $false
    try {
        if (-not $process.Start()) {
            throw "Unable to start child process: $Executable"
        }
        $started = $true
        $process.WaitForExit()
        $exitCode = $process.ExitCode
    }
    finally {
        if ($started -and -not $process.HasExited) {
            $process.Kill($true)
            $process.WaitForExit()
        }
        $process.Dispose()
    }
    if ($exitCode -ne 0) {
        exit $exitCode
    }
}

function Assert-RunPlayWorkbenchFresh {
    $freshnessArguments = @{
        Executable = $workbench
        Repository = $repository
        Label = 'Run-play workbench'
        RebuildCommand = '.\run-play.ps1 build'
        AdditionalInputs = @('crates/sts_run_play/Cargo.toml')
        SelfGuardInputs = @(
            'crates/sts_run_play/src/main.rs',
            'crates/oracle_artifact_contract/src/lib.rs'
        )
    }
    Assert-CargoArtifactFresh @freshnessArguments
}

function Get-RunPlayWorkbenchSha256 {
    (Get-FileHash -LiteralPath $workbench -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Show-RunPlayWorkbenchIdentity {
    $artifact = Get-Item -LiteralPath $workbench
    $sha256 = Get-RunPlayWorkbenchSha256
    Write-Output (
        'run_play artifact=fresh modified_utc={0:o} sha256={1} path={2}' -f
        $artifact.LastWriteTimeUtc,
        $sha256,
        $artifact.FullName
    )
}

function Get-RepositoryCommit {
    $lines = @(& git rev-parse HEAD)
    if ($LASTEXITCODE -ne 0 -or $lines.Count -ne 1) {
        throw 'Unable to read the repository commit.'
    }
    $lines[0].Trim()
}

function Test-RepositoryDirty {
    $lines = @(& git status --porcelain=v1 --untracked-files=normal)
    if ($LASTEXITCODE -ne 0) {
        throw 'Unable to inspect the repository worktree.'
    }
    $lines.Count -gt 0
}

function Invoke-RunPlayOverview {
    param(
        [Parameter()] [string[]] $OverviewArguments = @()
    )

    $commandArguments = @(
        'overview',
        '--repository-commit', (Get-RepositoryCommit),
        '--workbench-sha256', (Get-RunPlayWorkbenchSha256)
    )
    if (Test-RepositoryDirty) {
        $commandArguments += '--repository-dirty'
    }
    $commandArguments += $OverviewArguments
    Invoke-Checked $workbench $commandArguments
}

if (-not $Arguments -or $Arguments.Count -eq 0) {
    $Arguments = @('status')
}

if ($Arguments[0] -in @('help', '--help', '-h') -or
    ($Arguments.Count -eq 2 -and $Arguments[1] -in @('--help', '-h'))) {
    if ($Arguments.Count -gt 2) {
        [Console]::Error.WriteLine('Usage: .\p.ps1 help [topic|topics|all]')
        exit 2
    }
    $topic = if ($Arguments[0] -in @('help', '--help', '-h')) {
        if ($Arguments.Count -eq 2) { $Arguments[1] } else { '' }
    } else {
        $Arguments[0]
    }
    Show-RunPlayHelp $topic
    exit 0
}

Push-Location -LiteralPath $repository
try {
    if ($Arguments[0] -eq 'check') {
        if ($Arguments.Count -ne 1) {
            throw 'Usage: .\run-play.ps1 check (see: .\p.ps1 help check)'
        }
        Invoke-Checked cargo @('check', '--locked', '-p', 'sts_run_play')
        exit 0
    }

    if ($Arguments[0] -eq 'build') {
        if ($Arguments.Count -ne 1) {
            throw 'Usage: .\run-play.ps1 build (see: .\p.ps1 help build)'
        }
        Invoke-Checked cargo @(
            'build', '--locked', '--profile', 'dev-opt',
            '-p', 'sts_run_play', '--bin', 'run_play'
        )
        Assert-RunPlayWorkbenchFresh
        Show-RunPlayWorkbenchIdentity
        exit 0
    }

    if (-not (Test-Path -LiteralPath $workbench -PathType Leaf)) {
        throw 'Run-play workbench is not built. Run: .\run-play.ps1 build'
    }
    Assert-RunPlayWorkbenchFresh

    # A leading action is the ordinary one-step/multi-step play gesture.
    # The Rust parser already accepts both `play Card@1` and `play:Card@1`.
    $actionVerbs = @(
        'play', 'use', 'end', 'go', 'take', 'claim', 'buy', 'remove', 'smith', 'toke',
        'choose', 'select', 'discard-potion', 'boss-relic', 'advance',
        'proceed', 'cancel', 'skip-card', 'open-card', 'singing-bowl',
        'rest', 'dig', 'lift', 'recall', 'leave', 'open-rewards',
        'skip-boss-relic', 'open-chest'
    )
    $actionPrefix = '^(play|use-potion|take|claim|buy|remove|smith|toke|choose|select|discard-potion|boss-relic|skip-card|open-card|singing-bowl|route|event|ordinal):'
    $indexedRewardAction = '^(open-card|skip-card)#[0-9]+$'
    if (($Arguments[0] -in $actionVerbs -and -not ($Arguments[0] -eq 'use' -and $Arguments.Count -gt 1 -and $Arguments[1] -eq '--session')) -or
        $Arguments[0] -match $actionPrefix -or $Arguments[0] -match $indexedRewardAction) {
        $Arguments = @('act') + $Arguments
    }

    # Focus is a convenience pointer, resolved only when no session was
    # supplied. The executable still validates the selected session itself.
    $focusedCommands = @(
        'status', 'inventory', 'show', 'map', 'deck-lab', 'info', 'options', 'act', 'preview', 'history',
        'undo', 'back', 'mark', 'jump', 'advice', 'combat-research', 'combat-query',
        'combat-damage-challenge', 'combat-damage-anchor', 'anchor-combat-plan'
    )
    if ($Arguments[0] -in $focusedCommands -and '--session' -notin $Arguments) {
        $focusJson = @(& $workbench focus show --json)
        if ($LASTEXITCODE -ne 0) {
            throw 'Unable to read run-play focus.'
        }
        $focus = ($focusJson -join "`n") | ConvertFrom-Json
        if (-not $focus.session_path) {
            throw 'No run-play session is focused. Use: .\run-play.ps1 focus set --session <path>'
        }
        $Arguments = @($Arguments[0], '--session', [string]$focus.session_path) + @($Arguments | Select-Object -Skip 1)
    }

    if ($Arguments[0] -eq 'use') {
        if ($Arguments.Count -ne 3 -or $Arguments[1] -ne '--session') {
            throw 'Usage: .\run-play.ps1 use --session .oracle-lab/run-play/<session> (see: .\p.ps1 help focus)'
        }
        Invoke-Checked $workbench @('focus', 'set', '--session', $Arguments[2])
        exit 0
    }

    if ($Arguments[0] -eq 'resume') {
        if ($Arguments.Count -ne 1) {
            throw 'Usage: .\run-play.ps1 resume (see: .\p.ps1 help overview)'
        }
        Invoke-RunPlayOverview
        exit 0
    }

    if ($Arguments[0] -eq 'overview') {
        Invoke-RunPlayOverview -OverviewArguments @($Arguments | Select-Object -Skip 1)
        exit 0
    }

    if ($Arguments[0] -eq 'handoff' -and '--repository-commit' -notin $Arguments) {
        $Arguments += @('--repository-commit', (Get-RepositoryCommit))
    }
    if ($Arguments[0] -in @('status', 'act', 'preview')) {
        $sessionIndex = [Array]::IndexOf($Arguments, '--session')
        if ($sessionIndex -ge 0 -and $sessionIndex + 1 -lt $Arguments.Count) {
            $sessionName = [IO.Path]::GetFileName($Arguments[$sessionIndex + 1].TrimEnd('\', '/'))
            Write-Output "Session: $sessionName"
        }
    }
    Invoke-Checked $workbench $Arguments
}
finally {
    Pop-Location
}
