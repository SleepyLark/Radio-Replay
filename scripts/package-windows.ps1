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
$portable = Join-Path $outputDirectory "Radio-Replay-$version-Windows-x64.exe"
Copy-Item -LiteralPath $installers[0].FullName -Destination $installer
Copy-Item -LiteralPath target/release/radio-replay-desktop.exe -Destination $portable
$checksums = @($installer, $portable) | Sort-Object | ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($_))"
}
[IO.File]::WriteAllText((Join-Path $outputDirectory 'SHA256SUMS.txt'), ($checksums -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
Write-Output "Release files: $outputDirectory"
