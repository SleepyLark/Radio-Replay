param([Parameter(Mandatory)][string]$Archive)
$ErrorActionPreference = 'Stop'
$workspaceRoot = Split-Path $PSScriptRoot -Parent
$manifest = Get-Content -LiteralPath (Join-Path $workspaceRoot 'runtime/mpv-windows-x64.json') -Raw | ConvertFrom-Json
$version = (Get-Content -LiteralPath (Join-Path $workspaceRoot 'package.json') -Raw | ConvertFrom-Json).version
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($Archive))
try {
    $required = @("Radio-Replay-$version-Windows-x64.exe", 'audio/libmpv-2.dll', 'audio/LICENSE.LGPL', 'runtime/README.txt', 'runtime/SOURCES.txt', 'runtime/mpv-windows-x64.json', 'runtime/LICENSE.Radio-Replay')
    foreach ($name in $required) {
        $entry = $zip.GetEntry($name)
        if ($null -eq $entry -or $entry.Length -eq 0) { throw "Portable release is missing $name" }
    }
    foreach ($name in @('Install-AudioRuntime.cmd','runtime/Install-AudioRuntime.ps1')) {
        if ($null -ne $zip.GetEntry($name)) { throw "Obsolete end-user runtime downloader in portable release: $name" }
    }
    $stream = $zip.GetEntry('audio/libmpv-2.dll').Open()
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose(); $stream.Dispose() }
    if ($hash -ne $manifest.librarySha256) { throw 'Bundled audio DLL checksum mismatch' }
    Write-Output 'Portable ZIP verified: executable, bundled audio DLL, licenses, source information, and no runtime setup step.'
} finally { $zip.Dispose() }
