$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location (Split-Path $PSScriptRoot -Parent)

npm run check:version
npm run package -- --ci --bundles nsis -- --locked

$version = (Get-Content -LiteralPath package.json -Raw | ConvertFrom-Json).version
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
    Copy-Item -LiteralPath target/release/radio-replay-desktop.exe -Destination (Join-Path $stagingDirectory "Radio-Replay-$version-Windows-x64.exe")
    foreach ($file in @('Install-AudioRuntime.ps1','mpv-windows-x64.json','README.txt')) {
        Copy-Item -LiteralPath (Join-Path 'runtime' $file) -Destination (Join-Path $stagingDirectory 'runtime')
    }
    Copy-Item -LiteralPath runtime/Install-AudioRuntime.cmd -Destination $stagingDirectory
    Compress-Archive -Path (Join-Path $stagingDirectory '*') -DestinationPath $portable -Force
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
