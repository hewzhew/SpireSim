param(
    [ValidateSet('start', 'restart', 'status')][string]$Action = 'start',
    [ValidateSet('en', 'zh-CN')][string]$Locale = 'zh-CN',
    [switch]$Help
)
$webArguments = @($Action, '--locale', $Locale)
if ($Help) { $webArguments += '--help' }
& node (Join-Path (Split-Path -Parent $PSScriptRoot) 'tools/dsh_player/web.mjs') @webArguments
exit $LASTEXITCODE
