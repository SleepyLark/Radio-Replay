# Publishing Radio Replay releases

The workflow is `.github/workflows/build.yml`, named **Build and release**.
It builds Windows x64 packages. Linux and macOS currently have source checks,
not published installers.

## One-time GitHub setup

1. Push the workflow and scripts to `main` in `SleepyLark/Radio-Replay`.
2. In the repository's **Settings → Actions → General**, make sure Actions are
   enabled and the workflow's actions are permitted: `actions/*`,
   `dtolnay/rust-toolchain`, and `Swatinem/rust-cache`.
3. Open **Actions → Build and release** and confirm the first branch run passes.

No personal access token, repository secret, or code-signing certificate is
required for unsigned builds. The workflow defaults to read-only repository
access and grants `contents: write` only to the publishing job. An organization
policy can still restrict Actions or token permissions; an administrator must
resolve those restrictions if GitHub reports them.

## First Rust release: 0.6.5

After committing the README/workflow changes, run from the repository root:

```powershell
npm run check:version
git push origin main
git tag -a v0.6.5 -m "Radio Replay 0.6.5"
git push origin v0.6.5
```

The tag must point to the commit containing the new workflow. Do not tag an older
Electron commit. The version guard rejects a tag that disagrees with the project
manifests, including a tag that reuses the Electron version number by mistake.

Watch the tag run in **Actions → Build and release**. All platform checks must
pass before Windows packaging begins. The publishing job then creates the
release with its installer, standalone executable, and `SHA256SUMS.txt`.

The existing Electron tag `v0.5.0` is historical. Leave it intact. Stable Rust
releases explicitly set GitHub's Latest marker. Prerelease versions such as `0.7.0-beta.1` do not become Latest.

## Build without publishing

Use **Actions → Build and release → Run workflow**, select `main`, and run it.
After it succeeds, download **radio-replay-windows-x64** under the run's Artifacts
section. Artifacts are kept for 14 days. A manual run does not create a tag or
publish a GitHub Release.

## Subsequent releases

For example, to prepare version 0.6.6:

```powershell
npm run version:set -- 0.6.6
npm run check:version
git diff
```

The setter updates `package.json`, `package-lock.json`, both package Cargo
manifests, `Cargo.lock`, and `src-tauri/tauri.conf.json`. It does not commit, tag,
push, or publish anything. Run the relevant checks, review the changes, and then:

```powershell
git add package.json package-lock.json crates/core/Cargo.toml src-tauri/Cargo.toml Cargo.lock src-tauri/tauri.conf.json
git commit -m "Prepare Radio Replay 0.6.6"
git push origin main
git tag -a v0.6.6 -m "Radio Replay 0.6.6"
git push origin v0.6.6
```

Include any other release changes in the commit before tagging. Use a new
version for changed binaries instead of moving an already published tag.

## Local packaging

The same packaging script used in CI requires PowerShell 7.3 or newer:

```powershell
pwsh -File scripts/package-windows.ps1
```

Outputs are written to `release/<version>/windows-x64/`:

- `Radio-Replay-<version>-Windows-x64-setup.exe`
- `Radio-Replay-<version>-Windows-x64.exe`
- `SHA256SUMS.txt`

The script checks version consistency, builds with `Cargo.lock` locked, and
calculates hashes of the final files. The publishing job verifies those hashes
again after downloading the workflow artifact. These hashes detect changed
downloads; they are not a code-signing certificate.

## Troubleshooting

| Problem | What to check |
| --- | --- |
| No release after pushing `main` | Branch pushes only run checks. Push a matching `v<version>` tag to publish. |
| Tag-version mismatch | Use `npm run version:set` and review all version files before tagging. Do not overwrite existing release tags. |
| No Run workflow button | The workflow must first be present on the default branch, and Actions must be enabled. |
| `Resource not accessible by integration` or HTTP 403 | Check repository/organization Actions policy and the publish job's `contents: write` permission. |
| Packaging never starts | A validation job failed; inspect its log before retrying the build. |
| A rerun finds an existing release | The workflow replaces same-named assets and completes publishing. Immutable releases may reject replacement; use a new version for changed packages. |
| Windows shows SmartScreen | These builds are unsigned. Signing can be configured later with a certificate and protected secrets. |

Workflow configuration is validated locally, but hosted runners, repository
policy, and publishing permissions can only be confirmed by a GitHub run after
the changes are pushed.
