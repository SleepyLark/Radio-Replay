#Requires -Version 7.3
[CmdletBinding()]
param(
    [switch]$Repeat,
    [switch]$MissingRuntime,
    [switch]$NullOutput,
    [switch]$Offline,
    [string]$OutputDirectory = 'out/native-integration',
    [ValidateRange(5, 60)][int]$TimeoutSeconds = 45
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if (!$IsWindows) { throw 'The desktop integration runner requires Windows and WebView2.' }
$repoRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$outRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'out')) + [IO.Path]::DirectorySeparatorChar
$outputRoot = [IO.Path]::GetFullPath($(if ([IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory } else { Join-Path $repoRoot $OutputDirectory }))
if (!$outputRoot.StartsWith($outRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Integration output must be a directory under this repository/out.'
}
$runDirectory = Join-Path $outputRoot ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
$summary = [ordered]@{ passed = $false; nullOutput = [bool]$NullOutput; runs = @(); error = $null }
$environmentNames = @('RADIO_REPLAY_NATIVE_SMOKE_DIR', 'RADIO_REPLAY_MPV_LIBRARY', 'RADIO_REPLAY_NETWORK_SMOKE_URL', 'RADIO_REPLAY_NATIVE_SMOKE_MISSING_RUNTIME', 'RADIO_REPLAY_AUDIO_TEST_NULL_OUTPUT')
$previousEnvironment = @{}
foreach ($name in $environmentNames) { $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
Push-Location -LiteralPath $repoRoot
try {
    $manifest = Get-Content runtime/mpv-windows-x64.json -Raw | ConvertFrom-Json
    $runtime = Join-Path $repoRoot ('vendor/mpv/' + $manifest.library)
    if (!$Offline) { node scripts/setup-mpv.mjs --ensure }
    if (!(Test-Path -LiteralPath $runtime) -or (Get-FileHash -LiteralPath $runtime -Algorithm SHA256).Hash -ne $manifest.librarySha256) {
        throw 'Verified cached libmpv is required; run npm run setup:mpv first.'
    }
    # Always build the explicit harness; never launch an unverified existing app.
    $buildArguments = @('exec', 'tauri', '--', 'build', '--debug', '--no-bundle', '--features', 'native-smoke', '--config', 'src-tauri/tauri.smoke.conf.json', '--', '--locked')
    if ($Offline) { $buildArguments += '--offline' }
    & npm @buildArguments
    if ($LASTEXITCODE -ne 0) { throw 'Native integration build failed.' }
    $appDirectory = Join-Path $runDirectory 'app'
    New-Item -ItemType Directory -Path (Join-Path $appDirectory 'audio') -Force | Out-Null
    $executable = Join-Path $appDirectory 'radio-replay-desktop.exe'
    Copy-Item -LiteralPath target/debug/radio-replay-desktop.exe -Destination $executable
    Copy-Item -LiteralPath $runtime -Destination (Join-Path $appDirectory 'audio/libmpv-2.dll')
    $env:RADIO_REPLAY_MPV_LIBRARY = $null
    $env:RADIO_REPLAY_NETWORK_SMOKE_URL = $null
    $env:RADIO_REPLAY_NATIVE_SMOKE_MISSING_RUNTIME = $null
    $env:RADIO_REPLAY_AUDIO_TEST_NULL_OUTPUT = if ($NullOutput) { '1' } else { $null }

    function Invoke-IntegrationCase([string]$Name, [string]$fixtureDirectory, [bool]$ExpectMissing, [double]$InitialVolume) {
        Write-Host "Running isolated native integration: $Name"
        $env:RADIO_REPLAY_NATIVE_SMOKE_DIR = $fixtureDirectory
        $env:RADIO_REPLAY_NATIVE_SMOKE_MISSING_RUNTIME = if ($ExpectMissing) { '1' } else { $null }
        $env:RADIO_REPLAY_MPV_LIBRARY = if ($ExpectMissing) { Join-Path $runDirectory 'absent/libmpv-2.dll' } else { $null }
        $process = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runDirectory "$Name.stdout.log") -RedirectStandardError (Join-Path $runDirectory "$Name.stderr.log")
        if (!$process.WaitForExit($TimeoutSeconds * 1000)) {
            Stop-Process -Id $process.Id -Force
            $process.WaitForExit(5000) | Out-Null
            throw "$Name exceeded its $TimeoutSeconds second deadline."
        }
        $reportPath = Join-Path $fixtureDirectory 'report.json'
        if (!(Test-Path -LiteralPath $reportPath)) { throw "$Name exited without a report (exit $($process.ExitCode))." }
        Copy-Item -LiteralPath $reportPath -Destination (Join-Path $runDirectory "$Name.report.json")
        $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        if (!$report.passed -or $process.ExitCode -ne 0 -or $report.audioBackend -ne 'mpv') { throw "$Name failed: $($report.message)" }
        if ($report.nullOutputRequested -ne [bool]$NullOutput) { throw "$Name did not use the requested test-output mode." }
        if ($ExpectMissing) {
            if (!($report.phases -match 'runtimeMissing')) { throw 'Missing-runtime case did not reach its expected checks.' }
        } else {
            if (!$report.miniCompleted -or !$report.trayReady -or !$report.nativeMediaReady) { throw "$Name did not complete desktop checks." }
            if (!($report.phases -match 'grantInvalidation')) { throw "$Name did not verify grant invalidation." }
            $initial = $report.phases | Where-Object { $_ -match '^main: audioPreferencesStartup: ' } | Select-Object -First 1
            if (!$initial) { throw "$Name did not report restored levels." }
            $levels = ($initial -replace '^main: audioPreferencesStartup: ', '') | ConvertFrom-Json
            if ($levels.volume -ne $InitialVolume -or !$levels.muted) { throw "$Name did not restore expected volume/mute." }
            $saved = Get-Content (Join-Path $fixtureDirectory 'audio-preferences.json') -Raw | ConvertFrom-Json
            if ($saved.volume -ne 0.37 -or !$saved.muted) { throw "$Name did not flush final volume/mute on exit." }
        }
        [pscustomobject]@{ name = $Name; passed = $true; report = "$Name.report.json" }
    }

    $fixtureDirectory = Join-Path $runDirectory 'native-smoke'
    New-Item -ItemType Directory -Path $fixtureDirectory | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixtureDirectory 'audio-preferences.json'), '{"version":1,"volume":0.31,"muted":true}')
    $summary.runs += Invoke-IntegrationCase 'first' $fixtureDirectory $false 0.31
    if ($Repeat) { $summary.runs += Invoke-IntegrationCase 'repeat' $fixtureDirectory $false 0.37 }
    if ($MissingRuntime) {
        $summary.runs += Invoke-IntegrationCase 'missing' (Join-Path $runDirectory 'missing/native-smoke') $true 0
    }
    $summary.passed = $true
    Write-Host "Native integration passed. Reports: $runDirectory"
} catch {
    $summary.error = $_.Exception.Message
    throw
} finally {
    $summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $runDirectory 'summary.json') -Encoding utf8
    foreach ($name in $environmentNames) { [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process') }
    Pop-Location
}
