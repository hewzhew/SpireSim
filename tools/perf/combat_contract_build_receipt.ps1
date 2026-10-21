Set-StrictMode -Version Latest

function Get-StsCombatContractSourceFiles([string] $RepoRoot) {
    $RepoRoot = [IO.Path]::GetFullPath($RepoRoot)
    $Files = [Collections.Generic.List[string]]::new()
    foreach ($Relative in @(
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            ".cargo\config.toml",
            "rust-toolchain.toml",
            "crates\sts_combat_contract\Cargo.toml",
            "crates\sts_combat_knowledge\Cargo.toml",
            "crates\sts_combat_atomic\Cargo.toml",
            "crates\sts_combat_planner\Cargo.toml",
            "crates\sts_combat_strategy\Cargo.toml"
        )) {
        $Path = Join-Path $RepoRoot $Relative
        if (Test-Path -LiteralPath $Path -PathType Leaf) {
            $Files.Add([IO.Path]::GetFullPath($Path))
        }
    }
    foreach ($RelativeRoot in @(
            "src",
            "crates\sts_combat_contract\src",
            "crates\sts_combat_knowledge\src",
            "crates\sts_combat_atomic\src",
            "crates\sts_combat_planner\src",
            "crates\sts_combat_strategy\src"
        )) {
        $Root = Join-Path $RepoRoot $RelativeRoot
        if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
            continue
        }
        foreach ($File in Get-ChildItem -LiteralPath $Root -Recurse -File -Filter "*.rs") {
            $Files.Add($File.FullName)
        }
    }
    return @($Files | Sort-Object -Unique)
}

function Get-StsCombatContractSourceIdentity([string] $RepoRoot) {
    $RepoRoot = [IO.Path]::GetFullPath($RepoRoot).TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    )
    $Files = @(Get-StsCombatContractSourceFiles $RepoRoot)
    if ($Files.Count -eq 0) {
        throw "combat contract source scope below '$RepoRoot' is empty"
    }

    $Hasher = [Security.Cryptography.IncrementalHash]::CreateHash(
        [Security.Cryptography.HashAlgorithmName]::SHA256
    )
    $Encoding = [Text.UTF8Encoding]::new($false)
    $Separator = [byte[]] @(0)
    $Buffer = [byte[]]::new(64KB)
    try {
        foreach ($File in $Files) {
            $Relative = $File.Substring($RepoRoot.Length).TrimStart(
                [IO.Path]::DirectorySeparatorChar,
                [IO.Path]::AltDirectorySeparatorChar
            ).Replace([IO.Path]::DirectorySeparatorChar, '/')
            $Hasher.AppendData($Encoding.GetBytes($Relative))
            $Hasher.AppendData($Separator)

            $Stream = [IO.File]::Open(
                $File,
                [IO.FileMode]::Open,
                [IO.FileAccess]::Read,
                [IO.FileShare]::ReadWrite
            )
            try {
                while (($Read = $Stream.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
                    $Hasher.AppendData($Buffer, 0, $Read)
                }
            }
            finally {
                $Stream.Dispose()
            }
            $Hasher.AppendData($Separator)
        }
        $Digest = [Convert]::ToHexString($Hasher.GetHashAndReset())
    }
    finally {
        $Hasher.Dispose()
    }
    return [pscustomobject]@{
        algorithm = "sha256_path_and_content_v1"
        fingerprint = $Digest
        file_count = $Files.Count
    }
}

function Get-StsCombatContractBuildReceiptPath([string] $Executable) {
    return "$([IO.Path]::GetFullPath($Executable)).build-receipt.json"
}

function Write-StsCombatContractBuildReceipt(
    [string] $RepoRoot,
    [string] $Executable
) {
    $RepoRoot = [IO.Path]::GetFullPath($RepoRoot)
    $Executable = [IO.Path]::GetFullPath($Executable)
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "combat contract executable is missing at '$Executable'"
    }
    $Pdb = [IO.Path]::ChangeExtension($Executable, ".pdb")
    if (-not (Test-Path -LiteralPath $Pdb -PathType Leaf)) {
        throw "combat contract PDB is missing at '$Pdb'"
    }

    $Source = Get-StsCombatContractSourceIdentity $RepoRoot
    $GitCommit = (& git -C $RepoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "cannot resolve repository commit for combat build receipt"
    }
    $GitDirty = -not [string]::IsNullOrWhiteSpace(
        ((& git -C $RepoRoot status --porcelain) -join "`n")
    )
    $ExecutableInfo = Get-Item -LiteralPath $Executable
    $PdbInfo = Get-Item -LiteralPath $Pdb
    $Receipt = [ordered]@{
        schema_name = "StsCombatContractBuildReceiptV1"
        schema_version = 1
        recorded_at_utc = [DateTimeOffset]::UtcNow.ToString("O")
        profile = "profiling"
        package = "sts_combat_contract"
        binary = "combat_contract"
        git_commit = $GitCommit
        git_dirty = $GitDirty
        source_algorithm = $Source.algorithm
        source_fingerprint = $Source.fingerprint
        source_file_count = $Source.file_count
        executable_path = $Executable
        executable_bytes = $ExecutableInfo.Length
        executable_sha256 = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash
        pdb_path = $Pdb
        pdb_bytes = $PdbInfo.Length
        pdb_sha256 = (Get-FileHash -LiteralPath $Pdb -Algorithm SHA256).Hash
    }
    $ReceiptPath = Get-StsCombatContractBuildReceiptPath $Executable
    $Receipt | ConvertTo-Json -Depth 5 |
        Set-Content -LiteralPath $ReceiptPath -Encoding utf8
    return [pscustomobject] $Receipt
}

function Assert-StsCombatContractBuildReceipt(
    [string] $RepoRoot,
    [string] $Executable,
    [string] $ReceiptPath = ""
) {
    $RepoRoot = [IO.Path]::GetFullPath($RepoRoot)
    $Executable = [IO.Path]::GetFullPath($Executable)
    if ([string]::IsNullOrWhiteSpace($ReceiptPath)) {
        $ReceiptPath = Get-StsCombatContractBuildReceiptPath $Executable
    }
    else {
        $ReceiptPath = [IO.Path]::GetFullPath($ReceiptPath)
    }
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "combat contract executable is missing at '$Executable'; rerun without -SkipBuild"
    }
    if (-not (Test-Path -LiteralPath $ReceiptPath -PathType Leaf)) {
        throw "combat contract build receipt is missing at '$ReceiptPath'; rerun without -SkipBuild"
    }

    $Receipt = Get-Content -LiteralPath $ReceiptPath -Raw | ConvertFrom-Json
    if ($Receipt.schema_name -ne "StsCombatContractBuildReceiptV1" -or
        [int] $Receipt.schema_version -ne 1) {
        throw "unsupported combat contract build receipt at '$ReceiptPath'; rerun without -SkipBuild"
    }
    if ([IO.Path]::GetFullPath([string] $Receipt.executable_path) -ne $Executable) {
        throw "combat contract build receipt belongs to another executable; rerun without -SkipBuild"
    }

    $ActualExecutableHash = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash
    if ($ActualExecutableHash -ne [string] $Receipt.executable_sha256) {
        throw "combat contract executable changed after its build receipt; rerun without -SkipBuild"
    }
    $Pdb = [IO.Path]::ChangeExtension($Executable, ".pdb")
    if (-not (Test-Path -LiteralPath $Pdb -PathType Leaf)) {
        throw "combat contract PDB is missing at '$Pdb'; rerun without -SkipBuild"
    }
    $ActualPdbHash = (Get-FileHash -LiteralPath $Pdb -Algorithm SHA256).Hash
    if ($ActualPdbHash -ne [string] $Receipt.pdb_sha256) {
        throw "combat contract PDB changed after its build receipt; rerun without -SkipBuild"
    }

    $Source = Get-StsCombatContractSourceIdentity $RepoRoot
    if ($Source.algorithm -ne [string] $Receipt.source_algorithm -or
        $Source.fingerprint -ne [string] $Receipt.source_fingerprint -or
        $Source.file_count -ne [int] $Receipt.source_file_count) {
        throw "combat contract sources changed after this executable was built; rerun without -SkipBuild"
    }
    return $Receipt
}

function Get-StsCombatContractBuildReceiptStatus(
    [string] $RepoRoot,
    [string] $Executable,
    [string] $ReceiptPath = ""
) {
    try {
        $Receipt = Assert-StsCombatContractBuildReceipt `
            $RepoRoot `
            $Executable `
            $ReceiptPath
        return [pscustomobject]@{
            valid = $true
            receipt = $Receipt
            reason = $null
        }
    }
    catch {
        return [pscustomobject]@{
            valid = $false
            receipt = $null
            reason = $_.Exception.Message
        }
    }
}
