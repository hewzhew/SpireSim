Set-StrictMode -Version Latest

function Get-PeCodeViewIdentity([string] $Executable) {
    $ExecutablePath = [IO.Path]::GetFullPath($Executable)
    $Stream = [IO.File]::Open(
        $ExecutablePath,
        [IO.FileMode]::Open,
        [IO.FileAccess]::Read,
        [IO.FileShare]::ReadWrite
    )
    $Reader = [IO.BinaryReader]::new($Stream)
    try {
        $Stream.Position = 0x3c
        $PeOffset = $Reader.ReadUInt32()
        $Stream.Position = $PeOffset
        if ($Reader.ReadUInt32() -ne 0x00004550) {
            throw "'$ExecutablePath' is not a PE executable"
        }

        $Stream.Position = $PeOffset + 6
        $SectionCount = $Reader.ReadUInt16()
        $Stream.Position = $PeOffset + 20
        $OptionalHeaderSize = $Reader.ReadUInt16()
        $OptionalHeaderOffset = $PeOffset + 24
        $Stream.Position = $OptionalHeaderOffset
        $Magic = $Reader.ReadUInt16()
        $DataDirectoryOffset = switch ($Magic) {
            0x10b { $OptionalHeaderOffset + 96 }
            0x20b { $OptionalHeaderOffset + 112 }
            default { throw "unsupported PE optional-header magic 0x$($Magic.ToString('x'))" }
        }

        # IMAGE_DIRECTORY_ENTRY_DEBUG is data-directory slot 6.
        $Stream.Position = $DataDirectoryOffset + (6 * 8)
        $DebugRva = $Reader.ReadUInt32()
        $DebugSize = $Reader.ReadUInt32()
        if ($DebugRva -eq 0 -or $DebugSize -lt 28) {
            throw "'$ExecutablePath' contains no PE debug directory"
        }

        $Sections = @()
        $Stream.Position = $OptionalHeaderOffset + $OptionalHeaderSize
        for ($Index = 0; $Index -lt $SectionCount; $Index++) {
            $SectionStart = $Stream.Position
            $Reader.ReadBytes(8) | Out-Null
            $VirtualSize = $Reader.ReadUInt32()
            $VirtualAddress = $Reader.ReadUInt32()
            $RawSize = $Reader.ReadUInt32()
            $RawPointer = $Reader.ReadUInt32()
            $Sections += [pscustomobject]@{
                virtual_size = $VirtualSize
                virtual_address = $VirtualAddress
                raw_size = $RawSize
                raw_pointer = $RawPointer
            }
            $Stream.Position = $SectionStart + 40
        }

        $DebugSection = $Sections | Where-Object {
            $Span = [math]::Max([uint64] $_.virtual_size, [uint64] $_.raw_size)
            [uint64] $DebugRva -ge [uint64] $_.virtual_address -and
            [uint64] $DebugRva -lt ([uint64] $_.virtual_address + $Span)
        } | Select-Object -First 1
        if ($null -eq $DebugSection) {
            throw "PE debug directory RVA does not map to a file section"
        }
        $DebugOffset = [uint64] $DebugSection.raw_pointer +
            ([uint64] $DebugRva - [uint64] $DebugSection.virtual_address)

        for ($Offset = 0; $Offset + 28 -le $DebugSize; $Offset += 28) {
            $Stream.Position = $DebugOffset + $Offset + 12
            $DebugType = $Reader.ReadUInt32()
            $DataSize = $Reader.ReadUInt32()
            $Reader.ReadUInt32() | Out-Null # AddressOfRawData
            $DataPointer = $Reader.ReadUInt32()
            if ($DebugType -ne 2 -or $DataSize -lt 25 -or $DataPointer -eq 0) {
                continue
            }

            $Stream.Position = $DataPointer
            $Signature = [Text.Encoding]::ASCII.GetString($Reader.ReadBytes(4))
            if ($Signature -ne "RSDS") {
                continue
            }
            $Guid = [Guid]::new($Reader.ReadBytes(16))
            $Age = $Reader.ReadUInt32()
            $PathBytes = $Reader.ReadBytes([int] $DataSize - 24)
            $Terminator = [Array]::IndexOf($PathBytes, [byte] 0)
            if ($Terminator -lt 0) {
                $Terminator = $PathBytes.Length
            }
            $PdbPath = [Text.Encoding]::UTF8.GetString($PathBytes, 0, $Terminator)
            $PdbName = [IO.Path]::GetFileName($PdbPath)
            if ([string]::IsNullOrWhiteSpace($PdbName)) {
                throw "PE CodeView record contains no PDB filename"
            }
            return [pscustomobject]@{
                guid = $Guid.ToString("N").ToUpperInvariant()
                age = [uint32] $Age
                symbol_key = "$($Guid.ToString('N').ToUpperInvariant())$Age"
                pdb_name = $PdbName
                recorded_pdb_path = $PdbPath
            }
        }
        throw "'$ExecutablePath' contains no RSDS CodeView record"
    }
    finally {
        $Reader.Dispose()
        $Stream.Dispose()
    }
}

function Publish-NativePdbToSymbolCache(
    [string] $Executable,
    [string] $SymbolCacheRoot
) {
    $ExecutablePath = [IO.Path]::GetFullPath($Executable)
    $Identity = Get-PeCodeViewIdentity $ExecutablePath
    $PdbPath = Join-Path ([IO.Path]::GetDirectoryName($ExecutablePath)) $Identity.pdb_name
    if (-not (Test-Path -LiteralPath $PdbPath -PathType Leaf)) {
        throw "matching PDB is missing beside '$ExecutablePath'"
    }

    $ExecutableInfo = Get-Item -LiteralPath $ExecutablePath
    $PdbInfo = Get-Item -LiteralPath $PdbPath
    $TimestampGap = [math]::Abs(
        ($ExecutableInfo.LastWriteTimeUtc - $PdbInfo.LastWriteTimeUtc).TotalSeconds
    )
    if ($TimestampGap -gt 5) {
        throw "executable and PDB timestamps differ by $([math]::Round($TimestampGap, 3)) seconds"
    }

    $CacheRoot = [IO.Path]::GetFullPath($SymbolCacheRoot)
    $Destination = Join-Path $CacheRoot (
        "$($Identity.pdb_name)\$($Identity.symbol_key)\$($Identity.pdb_name)"
    )
    New-Item -ItemType Directory -Path (Split-Path -Parent $Destination) -Force |
        Out-Null
    Copy-Item -LiteralPath $PdbPath -Destination $Destination -Force
    return [pscustomobject]@{
        identity = $Identity
        source = $PdbPath
        cached = $Destination
    }
}
