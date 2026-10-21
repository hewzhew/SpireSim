#requires -Version 7.0

Set-StrictMode -Version Latest

function Get-CargoDepfileTokens {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Text
    )

    $tokens = [System.Collections.Generic.List[string]]::new()
    foreach ($line in ($Text -split "\r?\n")) {
        $separator = $line.IndexOf(': ')
        if ($separator -lt 0) {
            continue
        }
        $payload = $line.Substring($separator + 2)
        $token = [System.Text.StringBuilder]::new()
        for ($index = 0; $index -lt $payload.Length; $index++) {
            $character = $payload[$index]
            if ([char]::IsWhiteSpace($character)) {
                if ($token.Length -ne 0) {
                    $tokens.Add($token.ToString())
                    $null = $token.Clear()
                }
                continue
            }
            if (
                $character -eq '\' -and
                $index + 1 -lt $payload.Length -and
                [char]::IsWhiteSpace($payload[$index + 1])
            ) {
                $index++
                $null = $token.Append($payload[$index])
                continue
            }
            $null = $token.Append($character)
        }
        if ($token.Length -ne 0) {
            $tokens.Add($token.ToString())
        }
    }
    return $tokens
}

function Get-CargoPackageResolutionIdentity {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Repository,

        [Parameter(Mandatory)]
        [string] $CargoPackage
    )

    if ([string]::IsNullOrWhiteSpace($CargoPackage)) {
        throw 'Cargo package resolution requires a non-empty package name.'
    }
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = 'cargo'
    $startInfo.WorkingDirectory = [IO.Path]::GetFullPath($Repository)
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in @(
        'tree', '--locked', '--package', $CargoPackage,
        '--edges', 'normal,build', '--prefix', 'depth',
        '--charset', 'ascii', '--format', '{p}|{f}'
    )) {
        [void] $startInfo.ArgumentList.Add($argument)
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw "Unable to start cargo tree for package '$CargoPackage'."
        }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) {
            throw "cargo tree failed for package '$CargoPackage': $($stderr.Trim())"
        }
    }
    finally {
        $process.Dispose()
    }
    $normalized = (($stdout -replace "`r`n", "`n").TrimEnd("`n")) + "`n"
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($normalized)
    $sha256 = [Security.Cryptography.SHA256]::HashData($bytes)
    return [ordered]@{
        package = $CargoPackage
        sha256 = [Convert]::ToHexString($sha256).ToLowerInvariant()
        line_count = @($normalized -split "`n" | Where-Object { $_.Length -gt 0 }).Count
    }
}

function Write-CargoArtifactBuildReceipt {
    <#
    .SYNOPSIS
    Records the exact artifact and Cargo metadata accepted by a successful build.

    .DESCRIPTION
    Cargo does not rewrite a depfile for every successful no-op build.  A
    content-bound receipt therefore records that Cargo accepted the current
    manifests, target dependency resolution, and configuration even when it
    correctly kept the existing executable and depfile.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Executable,

        [Parameter(Mandatory)]
        [string] $Repository,

        [Parameter(Mandatory)]
        [string] $Receipt,

        [Parameter()]
        [string[]] $AdditionalInputs = @(),

        [Parameter()]
        [string] $CargoPackage = ''
    )

    $executablePath = [IO.Path]::GetFullPath($Executable)
    $repositoryPath = [IO.Path]::GetFullPath($Repository)
    $depfilePath = [IO.Path]::ChangeExtension($executablePath, '.d')
    foreach ($required in @($executablePath, $depfilePath)) {
        if (-not [IO.File]::Exists($required)) {
            throw "Cannot write Cargo artifact receipt because '$required' does not exist."
        }
    }

    $inputPaths = [System.Collections.Generic.Dictionary[string, string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    $commonInputs = @('Cargo.toml', '.cargo/config.toml')
    if ([string]::IsNullOrWhiteSpace($CargoPackage)) {
        $commonInputs += 'Cargo.lock'
    }
    foreach ($input in @($AdditionalInputs) + $commonInputs) {
        $path = if ([IO.Path]::IsPathRooted($input)) {
            [IO.Path]::GetFullPath($input)
        }
        else {
            [IO.Path]::GetFullPath((Join-Path $repositoryPath $input))
        }
        $inputPaths[$path] = $path
    }

    $inputs = [System.Collections.Generic.List[object]]::new()
    foreach ($path in ($inputPaths.Keys | Sort-Object)) {
        if (-not [IO.File]::Exists($path)) {
            throw "Cannot write Cargo artifact receipt because metadata input '$path' is not a file."
        }
        $item = [IO.FileInfo]::new($path)
        $inputs.Add([ordered]@{
            path = $path
            length = $item.Length
            sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
        })
    }

    $executableItem = [IO.FileInfo]::new($executablePath)
    $depfileItem = [IO.FileInfo]::new($depfilePath)
    $payload = [ordered]@{
        schema = if ([string]::IsNullOrWhiteSpace($CargoPackage)) {
            'cargo_artifact_build_receipt_v1'
        } else {
            'cargo_artifact_build_receipt_v2'
        }
        created_at = [DateTimeOffset]::UtcNow.ToString('o')
        executable = [ordered]@{
            path = $executablePath
            length = $executableItem.Length
            sha256 = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash
        }
        depfile = [ordered]@{
            path = $depfilePath
            length = $depfileItem.Length
            sha256 = (Get-FileHash -LiteralPath $depfilePath -Algorithm SHA256).Hash
        }
        cargo_inputs = @($inputs)
    }
    if (-not [string]::IsNullOrWhiteSpace($CargoPackage)) {
        $payload['cargo_resolution'] = Get-CargoPackageResolutionIdentity `
            -Repository $repositoryPath -CargoPackage $CargoPackage
    }

    $receiptPath = [IO.Path]::GetFullPath($Receipt)
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($receiptPath)) | Out-Null
    $temporaryPath = "$receiptPath.tmp-$([Guid]::NewGuid().ToString('N'))"
    try {
        [IO.File]::WriteAllText(
            $temporaryPath,
            (($payload | ConvertTo-Json -Depth 6) + [Environment]::NewLine),
            [Text.UTF8Encoding]::new($false)
        )
        Move-Item -LiteralPath $temporaryPath -Destination $receiptPath -Force
    }
    finally {
        if ([IO.File]::Exists($temporaryPath)) {
            Remove-Item -LiteralPath $temporaryPath -Force
        }
    }
}

function Assert-CargoArtifactFresh {
    <#
    .SYNOPSIS
    Refuses to execute a missing, unverifiable, or stale Cargo artifact.

    .DESCRIPTION
    This check deliberately lives in the interpreted launcher. An executable
    cannot prove that it contains a freshness guard added after it was built.
    The Cargo depfile is the owner of the transitive source-input set. Source
    inputs remain tied to the executable. Manifests and configuration that
    rustc does not record are tied either to a content-bound receipt written
    after a successful Cargo invocation or, for legacy callers, to the depfile
    modification time.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Executable,

        [Parameter(Mandatory)]
        [string] $Repository,

        [Parameter(Mandatory)]
        [string] $Label,

        [Parameter(Mandatory)]
        [string] $RebuildCommand,

        [Parameter()]
        [string[]] $AdditionalInputs = @(),

        # A wrapper-owned receipt may prove a successful no-op Cargo build.
        # Without one, retain the older depfile-mtime contract.
        [Parameter()]
        [string] $CargoReceipt = '',

        # When the executable contains the Rust depfile guard, the launcher
        # only has to prove that this guard itself is current. The executable
        # then checks the full transitive depfile without duplicating thousands
        # of filesystem probes in PowerShell.
        [Parameter()]
        [string[]] $SelfGuardInputs = @(),

        # A receipt can bind the target package's resolved normal/build
        # dependency closure instead of invalidating on unrelated lockfile edits.
        [Parameter()]
        [string] $CargoPackage = ''
    )

    $executablePath = [IO.Path]::GetFullPath($Executable)
    $repositoryPath = [IO.Path]::GetFullPath($Repository)
    if (-not [IO.File]::Exists($executablePath)) {
        throw "$Label is not built. Run: $RebuildCommand"
    }

    $depfilePath = [IO.Path]::ChangeExtension($executablePath, '.d')
    if (-not [IO.File]::Exists($depfilePath)) {
        throw "$Label dependency manifest is missing at '$depfilePath'. Rebuild with: $RebuildCommand"
    }

    $artifactInputs = [System.Collections.Generic.Dictionary[string, string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    if ($SelfGuardInputs.Count -eq 0) {
        $depfileText = [IO.File]::ReadAllText($depfilePath)
        foreach ($dependency in (Get-CargoDepfileTokens -Text $depfileText)) {
            $path = if ([IO.Path]::IsPathRooted($dependency)) {
                [IO.Path]::GetFullPath($dependency)
            }
            else {
                [IO.Path]::GetFullPath((Join-Path $repositoryPath $dependency))
            }
            $artifactInputs[$path] = $path
        }
    }
    else {
        foreach ($input in $SelfGuardInputs) {
            $path = if ([IO.Path]::IsPathRooted($input)) {
                [IO.Path]::GetFullPath($input)
            }
            else {
                [IO.Path]::GetFullPath((Join-Path $repositoryPath $input))
            }
            $artifactInputs[$path] = $path
        }
    }

    $cargoInputs = [System.Collections.Generic.Dictionary[string, string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($input in $AdditionalInputs) {
        $path = if ([IO.Path]::IsPathRooted($input)) {
            [IO.Path]::GetFullPath($input)
        }
        else {
            [IO.Path]::GetFullPath((Join-Path $repositoryPath $input))
        }
        $cargoInputs[$path] = $path
    }

    $commonInputs = @('Cargo.toml', '.cargo/config.toml')
    if ([string]::IsNullOrWhiteSpace($CargoPackage)) {
        $commonInputs += 'Cargo.lock'
    }
    foreach ($commonInput in $commonInputs) {
        $path = [IO.Path]::GetFullPath((Join-Path $repositoryPath $commonInput))
        $cargoInputs[$path] = $path
    }

    function Assert-InputsNotNewerThan {
        param(
            [Parameter(Mandatory)] [string] $ReferencePath,
            [Parameter(Mandatory)] $Inputs
        )
        $referenceTime = [IO.File]::GetLastWriteTimeUtc($ReferencePath)
        foreach ($inputPath in $Inputs.Keys) {
            # Cargo may deliberately record a directory for rerun-if-changed.
            # Match the Rust artifact contract: any filesystem entry is a valid
            # input, and its own modification time participates in freshness.
            $isFile = [IO.File]::Exists($inputPath)
            $isDirectory = [IO.Directory]::Exists($inputPath)
            if (-not $isFile -and -not $isDirectory) {
                throw "$Label build input '$inputPath' is unavailable. Rebuild with: $RebuildCommand"
            }
            $inputTime = if ($isFile) {
                [IO.File]::GetLastWriteTimeUtc($inputPath)
            }
            else {
                [IO.Directory]::GetLastWriteTimeUtc($inputPath)
            }
            if ($inputTime -gt $referenceTime) {
                throw "$Label is stale: '$inputPath' is newer than '$ReferencePath'. Rebuild with: $RebuildCommand"
            }
        }
    }

    Assert-InputsNotNewerThan -ReferencePath $executablePath -Inputs $artifactInputs
    if ([string]::IsNullOrWhiteSpace($CargoReceipt)) {
        Assert-InputsNotNewerThan -ReferencePath $depfilePath -Inputs $cargoInputs
        return
    }

    $receiptPath = [IO.Path]::GetFullPath($CargoReceipt)
    if (-not [IO.File]::Exists($receiptPath)) {
        throw "$Label build receipt is missing at '$receiptPath'. Rebuild with: $RebuildCommand"
    }
    try {
        $receipt = [IO.File]::ReadAllText($receiptPath) | ConvertFrom-Json
    }
    catch {
        throw "$Label build receipt is invalid at '$receiptPath'. Rebuild with: $RebuildCommand"
    }
    $expectedReceiptSchema = if ([string]::IsNullOrWhiteSpace($CargoPackage)) {
        'cargo_artifact_build_receipt_v1'
    } else {
        'cargo_artifact_build_receipt_v2'
    }
    if ($receipt.schema -ne $expectedReceiptSchema) {
        throw "$Label build receipt has unsupported schema '$($receipt.schema)'; expected '$expectedReceiptSchema'. Rebuild with: $RebuildCommand"
    }
    if (-not [string]::IsNullOrWhiteSpace($CargoPackage)) {
        $resolution = Get-CargoPackageResolutionIdentity `
            -Repository $repositoryPath -CargoPackage $CargoPackage
        if (
            [string]$receipt.cargo_resolution.package -ne $CargoPackage -or
            [string]$receipt.cargo_resolution.sha256 -ne [string]$resolution.sha256
        ) {
            throw "$Label target package dependency resolution changed. Rebuild with: $RebuildCommand"
        }
    }

    function Assert-ReceiptFileIdentity {
        param(
            [Parameter(Mandatory)] $Recorded,
            [Parameter(Mandatory)] [string] $ExpectedPath,
            [Parameter(Mandatory)] [string] $Kind
        )
        if (-not [string]::Equals(
            [IO.Path]::GetFullPath([string] $Recorded.path),
            [IO.Path]::GetFullPath($ExpectedPath),
            [StringComparison]::OrdinalIgnoreCase
        )) {
            throw "$Label build receipt $Kind path does not match '$ExpectedPath'. Rebuild with: $RebuildCommand"
        }
        if (-not [IO.File]::Exists($ExpectedPath)) {
            throw "$Label build receipt $Kind '$ExpectedPath' is unavailable. Rebuild with: $RebuildCommand"
        }
        $item = [IO.FileInfo]::new($ExpectedPath)
        $hash = (Get-FileHash -LiteralPath $ExpectedPath -Algorithm SHA256).Hash
        if ([long] $Recorded.length -ne $item.Length -or [string] $Recorded.sha256 -ne $hash) {
            throw "$Label build receipt $Kind '$ExpectedPath' does not match current content. Rebuild with: $RebuildCommand"
        }
    }

    Assert-ReceiptFileIdentity -Recorded $receipt.executable -ExpectedPath $executablePath -Kind 'executable'
    Assert-ReceiptFileIdentity -Recorded $receipt.depfile -ExpectedPath $depfilePath -Kind 'depfile'

    $recordedInputs = [System.Collections.Generic.Dictionary[string, object]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($recorded in @($receipt.cargo_inputs)) {
        $path = [IO.Path]::GetFullPath([string] $recorded.path)
        if ($recordedInputs.ContainsKey($path)) {
            throw "$Label build receipt contains duplicate metadata input '$path'. Rebuild with: $RebuildCommand"
        }
        $recordedInputs[$path] = $recorded
    }
    if ($recordedInputs.Count -ne $cargoInputs.Count) {
        throw "$Label build receipt metadata input set changed. Rebuild with: $RebuildCommand"
    }
    foreach ($inputPath in $cargoInputs.Keys) {
        if (-not $recordedInputs.ContainsKey($inputPath)) {
            throw "$Label build receipt does not cover metadata input '$inputPath'. Rebuild with: $RebuildCommand"
        }
        Assert-ReceiptFileIdentity -Recorded $recordedInputs[$inputPath] -ExpectedPath $inputPath -Kind 'metadata input'
    }
}
