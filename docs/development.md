# Development validation

Run from the repository root after `npm ci`:

```powershell
npm run check:version
npm run check
npm run test:playback
npm run test:ui
npm run test:audio
cargo fmt --all -- --check
cargo test --workspace --locked
cargo check -p radio-replay-desktop --features native-smoke --locked
```

Browser integration uses installed Microsoft Edge. If it is missing, run
`npx playwright install msedge`. `RADIO_REPLAY_BROWSER` can select another
installed Playwright-supported channel. The tests serve fixtures locally and do
not edit your real station library.

## Isolated native integration (Windows)

Build the opt-in harness; its extra handler is excluded from ordinary builds:

```powershell
npm exec tauri -- build --debug --no-bundle --features native-smoke --config src-tauri/tauri.smoke.conf.json
```

Run it with an isolated absolute directory named `native-smoke`:

```powershell
$previousSmokeDirectory = $env:RADIO_REPLAY_NATIVE_SMOKE_DIR
try {
    $env:RADIO_REPLAY_NATIVE_SMOKE_DIR = "$PWD\out\native-smoke"
    $playerProcess = Start-Process -FilePath .\target\debug\radio-replay-desktop.exe -WindowStyle Hidden -PassThru
    if (!$playerProcess.WaitForExit(60000)) { throw 'Native smoke did not finish within its watchdog deadline.' }
    $report = Get-Content .\out\native-smoke\report.json -Raw | ConvertFrom-Json
    $report
    if (!$report.passed -or $playerProcess.ExitCode -ne 0) { throw 'Native integration failed.' }
} finally {
    $env:RADIO_REPLAY_NATIVE_SMOKE_DIR = $previousSmokeDirectory
}
```

The harness covers WebView2 audio, file grants, SQLite caching, main/mini IPC,
tray behavior, and exit-on-close. It needs a working native desktop environment;
CI compile-checks this feature but does not claim a hosted native run. Repeat the
run with the same fixtures to check durations cached across process restarts.

For the first large-library optimization measurements and remaining audit work,
see [performance pass 1](performance-pass-1.md).
