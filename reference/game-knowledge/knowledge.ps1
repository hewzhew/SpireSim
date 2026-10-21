[CmdletBinding(DefaultParameterSetName = 'Entity')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Entity')]
    [ValidateNotNullOrEmpty()]
    [string] $Entity,

    [Parameter(ParameterSetName = 'Entity')]
    [switch] $Detailed,

    [Parameter(Mandatory, ParameterSetName = 'Fact')]
    [ValidateNotNullOrEmpty()]
    [string] $Fact,

    [Parameter(Mandatory, ParameterSetName = 'List')]
    [switch] $ListSources,

    [Parameter(Mandatory, ParameterSetName = 'Verify')]
    [switch] $Verify
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$referenceRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$workspaceRoot = (Resolve-Path -LiteralPath (Join-Path $referenceRoot '..')).Path
Import-Module (Join-Path $referenceRoot 'ReferenceIndex.psm1') -Force

$entityPath = Join-Path $referenceRoot 'entities.jsonl'
$sourcePath = Join-Path $PSScriptRoot 'sources.jsonl'
$assertionPath = Join-Path $PSScriptRoot 'assertions.jsonl'
$factPath = Join-Path $PSScriptRoot 'facts.jsonl'

$entities = @(Read-ReferenceJsonLines -Path $entityPath)
$sources = @(Read-ReferenceJsonLines -Path $sourcePath)
$assertions = @(Read-ReferenceJsonLines -Path $assertionPath)
$facts = @(Read-ReferenceJsonLines -Path $factPath)

function ConvertTo-CompactJson {
    param([object] $Value)
    return $Value | ConvertTo-Json -Depth 12 -Compress
}

function Write-FactReport {
    param(
        [object[]] $SelectedFacts,
        [switch] $IncludeEvidence
    )

    $sourceById = @{}
    foreach ($source in $sources) {
        $sourceById[$source.id] = $source
    }
    $assertionById = @{}
    foreach ($assertion in $assertions) {
        $assertionById[$assertion.id] = $assertion
    }

    foreach ($item in $SelectedFacts) {
        if (-not $IncludeEvidence) {
            $support = @($item.supporting_assertion_ids | ForEach-Object { $assertionById[$_] })
            $sourceIds = @($support.source_id | Sort-Object -Unique)
            $independenceGroups = @(
                $sourceIds | ForEach-Object { $sourceById[$_].independence_group } | Sort-Object -Unique
            )
            Write-Output "  - $($item.id) [$($item.status)]"
            Write-Output "    $($item.predicate) = $(ConvertTo-CompactJson $item.value)"
            Write-Output "    sources=$($sourceIds -join ', ') independence_groups=$($independenceGroups -join ', ')"
            if ($null -ne $item.PSObject.Properties['caveats']) {
                Write-Output "    caveat: $(@($item.caveats)[0])"
            }
            continue
        }
        Write-Output "FACT $($item.id)"
        Write-Output "  predicate: $($item.predicate)"
        Write-Output "  value: $(ConvertTo-CompactJson $item.value)"
        Write-Output "  status: $($item.status)"
        Write-Output "  scope: $(ConvertTo-CompactJson $item.scope)"
        foreach ($assertionId in @($item.supporting_assertion_ids)) {
            $assertion = $assertionById[$assertionId]
            $source = $sourceById[$assertion.source_id]
            $locator = ConvertTo-CompactJson $assertion.locator
            Write-Output "  support: $($assertion.id)"
            Write-Output "    source=$($source.id) kind=$($assertion.evidence_kind) independence=$($source.independence_group)"
            Write-Output "    locator=$locator"
            if ($null -ne $assertion.PSObject.Properties['caveats']) {
                Write-Output "    caveats=$(@($assertion.caveats) -join '; ')"
            }
        }
        if ($null -ne $item.PSObject.Properties['caveats']) {
            Write-Output "  fact caveats: $(@($item.caveats) -join '; ')"
        }
        Write-Output ''
    }
    Write-Output "FACT SUMMARY count=$($SelectedFacts.Count) evidence=$(if ($IncludeEvidence) { 'detailed' } else { 'compact' })"
}

if ($ListSources) {
    foreach ($source in $sources) {
        [pscustomobject]@{
            Id = $source.id
            Kind = $source.kind
            Language = $source.language
            Status = $source.audit.current_status
            Independence = $source.independence_group
            NetworkSync = $source.access_policy.network_sync
            BulkAccess = $source.access_policy.bulk_access
            TrainingUse = $source.access_policy.training_use
        }
    }
    exit 0
}

if ($Verify) {
    $failed = $false
    $results = @()

    $entityById = @{}
    $aliasOwner = @{}
    foreach ($item in $entities) {
        $ok = $item.schema -eq 'reference-entity-v1' -and -not $entityById.ContainsKey($item.id)
        $entityById[$item.id] = $item
        foreach ($alias in @($item.id, $item.canonical_name) + @($item.aliases)) {
            $normalized = ([string] $alias).ToLowerInvariant()
            if ($aliasOwner.ContainsKey($normalized) -and $aliasOwner[$normalized] -ne $item.id) {
                $ok = $false
            }
            $aliasOwner[$normalized] = $item.id
        }
        if (-not $ok) {
            $failed = $true
            $results += [pscustomobject]@{ Area = 'entity'; Id = $item.id; Status = 'invalid schema, duplicate id, or alias conflict' }
        }
    }
    foreach ($item in $entities) {
        foreach ($edge in @($item.related)) {
            if (-not $entityById.ContainsKey($edge.entity_id)) {
                $failed = $true
                $results += [pscustomobject]@{ Area = 'entity-edge'; Id = "$($item.id)->$($edge.entity_id)"; Status = 'missing target' }
            }
        }
    }

    $sourceById = @{}
    foreach ($source in $sources) {
        $ok = (
            $source.schema -eq 'mechanics-source-v1' -and
            -not $sourceById.ContainsKey($source.id) -and
            $source.access_policy.training_use -eq $false
        )
        if ($source.kind -eq 'community_wiki') {
            if (
                $source.access_policy.network_sync -ne $false -or
                $source.access_policy.raw_body_retention -ne $false
            ) {
                $ok = $false
            }
        }
        $sourceById[$source.id] = $source
        if (-not $ok) {
            $failed = $true
            $results += [pscustomobject]@{ Area = 'source'; Id = $source.id; Status = 'invalid schema, duplicate id, or unsafe access policy' }
        }
    }

    $assertionById = @{}
    $hashCache = @{}
    foreach ($assertion in $assertions) {
        $ok = (
            $assertion.schema -eq 'mechanics-assertion-v1' -and
            -not $assertionById.ContainsKey($assertion.id) -and
            $sourceById.ContainsKey($assertion.source_id)
        )
        $assertionById[$assertion.id] = $assertion
        foreach ($entityId in @($assertion.entity_ids)) {
            if (-not $entityById.ContainsKey($entityId)) {
                $ok = $false
            }
        }
        if (-not $entityById.ContainsKey($assertion.subject_id)) {
            $ok = $false
        }

        $source = if ($sourceById.ContainsKey($assertion.source_id)) { $sourceById[$assertion.source_id] } else { $null }
        if ($null -ne $source -and $source.kind -eq 'community_wiki') {
            $hasStablePageLocator = (
                $null -ne $assertion.locator.PSObject.Properties['url'] -and
                (
                    $null -ne $assertion.locator.PSObject.Properties['revision_id'] -or
                    $null -ne $assertion.locator.PSObject.Properties['wrapper_revision_id']
                )
            )
            if (-not $hasStablePageLocator) {
                $ok = $false
            }
        }

        if ($null -ne $source -and $source.kind -eq 'decompiled_game_code') {
            $sourceRoot = [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $source.local_root)).TrimEnd('\')
            foreach ($file in @($assertion.locator.files)) {
                $candidate = [System.IO.Path]::GetFullPath((Join-Path $sourceRoot $file.path))
                if (-not $candidate.StartsWith("$sourceRoot\", [System.StringComparison]::OrdinalIgnoreCase)) {
                    $ok = $false
                    continue
                }
                if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
                    $ok = $false
                    continue
                }
                if (-not $hashCache.ContainsKey($candidate)) {
                    $hashCache[$candidate] = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
                }
                if ($hashCache[$candidate] -ne ([string] $file.sha256).ToLowerInvariant()) {
                    $ok = $false
                }
            }
        }

        if (-not $ok) {
            $failed = $true
        }
        $results += [pscustomobject]@{
            Area = 'assertion'
            Id = $assertion.id
            Status = if ($ok) { 'ok' } else { 'invalid source, entity, locator, or local hash' }
        }
    }

    $factIds = @{}
    foreach ($item in $facts) {
        $ok = (
            $item.schema -eq 'mechanics-fact-v1' -and
            -not $factIds.ContainsKey($item.id) -and
            @('wiki_reported', 'source_verified', 'conflict', 'unverified') -contains $item.status
        )
        $factIds[$item.id] = $true
        foreach ($entityId in @($item.entity_ids)) {
            if (-not $entityById.ContainsKey($entityId)) {
                $ok = $false
            }
        }
        if (-not $entityById.ContainsKey($item.subject_id)) {
            $ok = $false
        }
        $support = @()
        foreach ($assertionId in @($item.supporting_assertion_ids)) {
            if (-not $assertionById.ContainsKey($assertionId)) {
                $ok = $false
            }
            else {
                $support += $assertionById[$assertionId]
            }
        }
        if ($item.status -eq 'source_verified') {
            $hasGameCode = @($support | Where-Object { $_.evidence_kind -eq 'decompiled_game_code' }).Count -gt 0
            if (-not $hasGameCode) {
                $ok = $false
            }
        }
        if (-not $ok) {
            $failed = $true
        }
        $results += [pscustomobject]@{
            Area = 'fact'
            Id = $item.id
            Status = if ($ok) { 'ok' } else { 'invalid entity, support, status, or verification level' }
        }
    }

    if (@($results | Where-Object { $_.Area -in @('entity', 'entity-edge', 'source') }).Count -eq 0) {
        $results += [pscustomobject]@{ Area = 'catalog'; Id = 'entities/sources'; Status = "ok ($($entities.Count) entities, $($sources.Count) sources)" }
    }
    $results | Format-Table -AutoSize
    if ($failed) {
        exit 1
    }
    exit 0
}

if ($PSCmdlet.ParameterSetName -eq 'Fact') {
    $selected = @($facts | Where-Object { $_.id -eq $Fact })
    if ($selected.Count -ne 1) {
        throw "Unknown or duplicate mechanics fact '$Fact'."
    }
    Write-FactReport -SelectedFacts $selected -IncludeEvidence
    exit 0
}

$resolved = Resolve-ReferenceEntity -Entities $entities -Name $Entity
$selected = @(
    $facts | Where-Object {
        Test-ReferenceContainsIgnoreCase -Values $_.entity_ids -Expected $resolved.id
    }
)
Write-Output "Entity: $($resolved.id) ($($resolved.canonical_name))"
Write-Output "Aliases: $(@($resolved.aliases) -join ', ')"
Write-Output ''
Write-FactReport -SelectedFacts $selected -IncludeEvidence:$Detailed
