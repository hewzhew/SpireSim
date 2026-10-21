param([ValidateSet('start', 'restart', 'status')][string]$Action = 'start')
& node (Join-Path (Split-Path -Parent $PSScriptRoot) 'tools/dsh_player/web.mjs') $Action
exit $LASTEXITCODE
