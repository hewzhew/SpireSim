Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Read-ReferenceJsonLines {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "JSONL file does not exist: $Path"
    }

    $lineNumber = 0
    foreach ($line in [System.IO.File]::ReadLines($Path)) {
        $lineNumber += 1
        if (-not $line.Trim()) {
            continue
        }
        try {
            $line | ConvertFrom-Json
        }
        catch {
            throw "Invalid JSONL at ${Path}:$lineNumber`: $($_.Exception.Message)"
        }
    }
}

function Resolve-ReferenceEntity {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [object[]] $Entities,

        [Parameter(Mandatory)]
        [string] $Name
    )

    $matches = @(
        $Entities | Where-Object {
            $_.id -ieq $Name -or
            $_.canonical_name -ieq $Name -or
            @($_.aliases) -icontains $Name
        }
    )
    if ($matches.Count -eq 0) {
        throw "Unknown reference entity or alias '$Name'. Inspect reference/entities.jsonl."
    }
    if ($matches.Count -gt 1) {
        $ids = ($matches.id | Sort-Object -Unique) -join ', '
        throw "Ambiguous reference entity alias '$Name': $ids"
    }
    return $matches[0]
}

function Test-ReferenceContainsIgnoreCase {
    [CmdletBinding()]
    param(
        [object[]] $Values,
        [string] $Expected
    )

    return @($Values | Where-Object { [string] $_ -ieq $Expected }).Count -gt 0
}

Export-ModuleMember -Function @(
    'Read-ReferenceJsonLines',
    'Resolve-ReferenceEntity',
    'Test-ReferenceContainsIgnoreCase'
)
