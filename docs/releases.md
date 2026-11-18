# Publishing Radio Replay releases

The **Build and release** workflow in `.github/workflows/build.yml` checks the
workspace and packages Windows x64. Linux/macOS have source checks and no
published installers. See the [changelog](../CHANGELOG.md) and
[audit progress](pre-1.0-audit-progress.md) before preparing a release; unfinished
release prerequisites are tracked there.

## Repository setup

GitHub Actions must permit `actions/*`, `dtolnay/rust-toolchain` and
`Swatinem/rust-cache`. The workflow defaults to read-only repository access and
grants `contents: write` only to the publishing job. Unsigned builds do not need
a personal access token or signing certificate. Repository/organization policy
may still restrict Actions or token permissions.

## Prepare a version

Choose an unused version and update all manifests together:

```powershell
npm run version:set -- 0.6.7
npm run check:version
git diff
```

The version above is an example, not a statement that a release exists. The
setter updates the npm manifest/lockfile, all four workspace Cargo manifests,
`Cargo.lock` and `src-tauri/tauri.conf.json`. It does not commit, tag or publish.
Update the changelog and review the release notes with the code changes. Run the
[development checks](development.md), commit the reviewed changes, and push the
commit before tagging. Keep the stable application identifier unchanged.

After confirming the version is unused, tag the checked commit using the
manifest version:

```powershell
$releaseVersion = (Get-Content package.json -Raw | ConvertFrom-Json).version
npm run check:version
git tag -a "v$releaseVersion" -m "Radio Replay $releaseVersion"
git push origin "v$releaseVersion"
```

The version guard rejects mismatched tags. Use a new version for changed
binaries; do not move a published tag. Stable versions receive GitHub's Latest
marker; prerelease versions do not. Watch the tag run in **Actions → Build and
release**: checks must pass before packaging and publishing.

## Build without publishing

Use **Actions → Build and release → Run workflow** and select the intended
commit's branch. Download **radio-replay-windows-x64** from the successful run's
Artifacts section; artifacts are retained for 14 days. A manual run does not
create a tag or publish a release.

Local packaging uses the same script as CI and requires PowerShell 7.3 or newer:

```powershell
pwsh -File scripts/package-windows.ps1
```

Outputs under `release/<version>/windows-x64/` are:

- `Radio-Replay-<version>-Windows-x64-setup.exe`
- `Radio-Replay-<version>-Windows-x64-portable.zip`
- `SHA256SUMS.txt`

The portable ZIP contains the executable, audio DLL/license and runtime source
information together. A bare executable is not the portable distribution. The
script verifies the ZIP, checks version consistency, builds with the lockfile
locked, and hashes the final installer/ZIP. Publishing verifies those hashes
again after downloading the artifact. Checksums do not provide code signing;
these builds remain unsigned and may trigger SmartScreen.

## Troubleshooting

| Problem | What to check |
| --- | --- |
| No release after pushing a branch | Branch pushes run checks. A matching `v<version>` tag triggers publishing. |
| Tag-version mismatch | Review the version setter's changes before tagging. |
| No Run workflow button | The workflow must exist on the default branch and Actions must be enabled. |
| HTTP 403 or `Resource not accessible by integration` | Check repository/organization Actions policy and publishing token permissions. |
| Packaging never starts | Inspect failed validation jobs first. |
| Rerunning an existing release | The workflow replaces same-named assets where allowed; immutable releases may reject this. Use a new version for changed packages. |

Hosted runner behavior and publishing permissions can only be confirmed by a
GitHub run. Local validation does not publish or verify a hosted release.
