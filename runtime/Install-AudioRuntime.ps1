# Windows PowerShell 5.1 and PowerShell 7; no Node.js is required.
param(
    [string]$Destination = (Join-Path (Split-Path $PSScriptRoot -Parent) 'audio'),
    [string]$ArchivePath = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'mpv-windows-x64.json') -Raw | ConvertFrom-Json
if (-not [Environment]::Is64BitProcess) { throw 'Run this setup with 64-bit PowerShell.' }
$Destination = [IO.Path]::GetFullPath($Destination)
$library = Join-Path $Destination $manifest.library
$license = Join-Path $Destination 'LICENSE.LGPL'
if ((Test-Path -LiteralPath $library) -and (Test-Path -LiteralPath $license)) {
    if ((Get-Sha256 $library) -eq $manifest.librarySha256) {
        Write-Output "Verified libmpv $($manifest.version): $Destination"
        exit 0
    }
}
$temporaryArchive = [string]::IsNullOrEmpty($ArchivePath)
if ($temporaryArchive) { $ArchivePath = Join-Path ([IO.Path]::GetTempPath()) ('radio-replay-mpv-' + [guid]::NewGuid().ToString('N') + '.zip') }
$stagedFiles = @()
try {
    if (-not (Test-Path -LiteralPath $ArchivePath)) {
        $archiveParent = Split-Path ([IO.Path]::GetFullPath($ArchivePath)) -Parent
        [IO.Directory]::CreateDirectory($archiveParent) | Out-Null
        $download = $ArchivePath + '.download'
        try {
            Write-Output "Downloading the official libmpv $($manifest.version) audio runtime (about 54 MB)..."
            [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
            $ProgressPreference = 'SilentlyContinue'
            Invoke-WebRequest -Uri $manifest.url -OutFile $download -UseBasicParsing -TimeoutSec 180
            if ((Get-Sha256 $download) -ne $manifest.sha256) { throw 'Audio runtime archive checksum mismatch. Nothing was installed.' }
            Move-Item -LiteralPath $download -Destination $ArchivePath -Force
        } finally {
            if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download -Force }
        }
    }
    if ((Get-Sha256 $ArchivePath) -ne $manifest.sha256) { throw "Audio runtime checksum mismatch. Remove $ArchivePath and retry. Nothing was installed." }
    [IO.Directory]::CreateDirectory($Destination) | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        # Extract exact required entries, never arbitrary ZIP paths.
        foreach ($name in @($manifest.library, 'LICENSE.LGPL')) {
            $entry = $archive.GetEntry($name)
            if ($null -eq $entry) { throw "Verified archive is missing $name" }
            $staged = Join-Path $Destination ($name + '.' + [guid]::NewGuid().ToString('N') + '.part')
            $stagedFiles += $staged
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $staged)
        }
    } finally { $archive.Dispose() }
    if ((Get-Sha256 $stagedFiles[0]) -ne $manifest.librarySha256) { throw 'Audio DLL checksum mismatch. Nothing was installed.' }
    Move-Item -LiteralPath $stagedFiles[1] -Destination $license -Force
    Move-Item -LiteralPath $stagedFiles[0] -Destination $library -Force
    Write-Output "Verified libmpv $($manifest.version): $Destination"
} finally {
    foreach ($file in $stagedFiles) { if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file -Force } }
    if ($temporaryArchive -and (Test-Path -LiteralPath $ArchivePath)) { Remove-Item -LiteralPath $ArchivePath -Force }
}
