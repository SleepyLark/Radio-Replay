param([switch]$BundleOnly)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location (Split-Path $PSScriptRoot -Parent)

npm run check:version
$version = (Get-Content -LiteralPath package.json -Raw | ConvertFrom-Json).version
if ($BundleOnly) {
    # For packaging-only changes: requires an existing matching release executable.
    $existingBinary = Join-Path (Get-Location) 'target/release/radio-replay-desktop.exe'
    if (-not (Test-Path -LiteralPath $existingBinary)) { throw 'Build the release executable before using -BundleOnly.' }
    $binaryVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($existingBinary).ProductVersion
    if ($binaryVersion -ne $version) { throw "Existing executable version $binaryVersion does not match $version. Run a normal build." }
    npm run setup:mpv
    npm exec tauri -- bundle --ci --bundles nsis
} else {
    npm run package -- --ci --bundles nsis -- --locked
}

$outputDirectory = Join-Path (Get-Location) "release/$version/windows-x64"
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
$installers = @(Get-ChildItem -LiteralPath target/release/bundle/nsis -Filter "*_${version}_x64-setup.exe")
if ($installers.Count -ne 1) { throw "Expected one NSIS installer; found $($installers.Count)." }
$installer = Join-Path $outputDirectory "Radio-Replay-$version-Windows-x64-setup.exe"
$portable = Join-Path $outputDirectory "Radio-Replay-$version-Windows-x64-portable.zip"
Copy-Item -LiteralPath $installers[0].FullName -Destination $installer
$buildRoot = [IO.Path]::GetFullPath((Join-Path (Get-Location) 'target'))
$stagingDirectory = Join-Path $buildRoot ('portable-' + [guid]::NewGuid().ToString('N'))
try {
    New-Item -ItemType Directory -Path (Join-Path $stagingDirectory 'runtime') -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $stagingDirectory 'audio') -Force | Out-Null
    Copy-Item -LiteralPath target/release/radio-replay-desktop.exe -Destination (Join-Path $stagingDirectory "Radio-Replay-$version-Windows-x64.exe")
    foreach ($file in @('mpv-windows-x64.json','README.txt','SOURCES.txt')) {
        Copy-Item -LiteralPath (Join-Path 'runtime' $file) -Destination (Join-Path $stagingDirectory 'runtime')
    }
    Copy-Item -LiteralPath LICENSE -Destination (Join-Path $stagingDirectory 'runtime/LICENSE.Radio-Replay')
    foreach ($file in @('libmpv-2.dll','LICENSE.LGPL')) {
        Copy-Item -LiteralPath (Join-Path 'vendor/mpv' $file) -Destination (Join-Path $stagingDirectory 'audio')
    }
    Compress-Archive -Path (Join-Path $stagingDirectory '*') -DestinationPath $portable -Force
    & (Join-Path $PSScriptRoot 'verify-portable.ps1') -Archive $portable
} finally {
    $resolvedStaging = [IO.Path]::GetFullPath($stagingDirectory)
    if (-not $resolvedStaging.StartsWith($buildRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing to clean a directory outside target.' }
    if (Test-Path -LiteralPath $resolvedStaging) { Remove-Item -LiteralPath $resolvedStaging -Recurse -Force }
}
# Do not publish an obsolete bare executable left by the previous packaging format.
$oldPortable = Join-Path $outputDirectory "Radio-Replay-$version-Windows-x64.exe"
if (Test-Path -LiteralPath $oldPortable) { Remove-Item -LiteralPath $oldPortable -Force }
$checksums = @($installer, $portable) | Sort-Object | ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($_))"
}
[IO.File]::WriteAllText((Join-Path $outputDirectory 'SHA256SUMS.txt'), ($checksums -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
Write-Output "Release files: $outputDirectory"
