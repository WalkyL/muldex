# Local Release Runbook

## Purpose

Describe the intended operator flow for running `muldex` release builds when GitHub Actions orchestrates the pipeline, Windows x64 compilation runs on the self-hosted build host at `192.168.1.52`, Linux compilation runs on hosted Ubuntu images, and Windows ARM64 compilation runs on a hosted Windows image.

This is a practical runbook, not a policy manifesto.

## Current topology

### GitHub-hosted

- workflow orchestration
- validation gate kickoff
- macOS build jobs
- release artifact attachment to GitHub Release

### Self-hosted Windows build host

- Windows x64 build job
- package generation for the Windows x64 target

### GitHub-hosted Linux jobs

- Linux x64 and ARM64 build jobs
- package generation and artifact verification for those targets

### GitHub-hosted Windows ARM64 job

- Windows ARM64 build job
- package generation for the ARM64 target

Current intended self-hosted build host:

- `192.168.1.52`

Expected labels for the self-hosted Windows path:

- `self-hosted`
- `muldex`
- `build-host-192-168-1-52`

plus one platform label such as:

- `windows-x64`

## Two release entry modes

### 1. Tag-driven release

Push a tag such as:

```powershell
git tag v0.0.0-dryrun.1
git push origin v0.0.0-dryrun.1
```

The workflow should infer `RELEASE_TAG` from the Git ref.

### 2. Manual workflow dispatch

Use GitHub Actions `workflow_dispatch` and provide:

- `release_tag`
- optional `publish_release=false` when you only want build and packaging without release attachment

## Recommended dry-run order

1. local validation
2. workflow dispatch with a dry-run tag
3. verify self-hosted routing
4. verify package and artifact checks
5. verify release attachment if enabled

## Local validation first

Before triggering Actions:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate-interactive-shell.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\prepare-interactive-shell-trial.ps1
```

## What to inspect in the workflow

### Validate job

Expect:

- `cargo test -p muldex-cli` passes
- `cargo test` passes

### Windows x64 job

Expect:

- the job lands on the self-hosted runner set that maps to `192.168.1.52`
- the Windows package script runs
- artifact verification passes

### Linux jobs

Expect:

- jobs land on the declared GitHub-hosted Ubuntu runner
- the ARM64 job reports the GNU cross linker preflight
- package scripts run
- artifact verification passes

### Windows ARM64 job

Expect:

- the job runs on the GitHub-hosted `windows-2025` runner
- the ARM64 MSVC toolchain preflight passes
- the ARM64 package and artifact verification pass

### macOS jobs

Expect:

- jobs land on GitHub-hosted macOS runners
- package scripts run
- artifact verification passes

### Publish job

Expect:

- packaged artifacts download correctly
- `gh release upload` attaches them to the target release tag

## Current artifact expectation

Windows:

- `.zip`

Linux/macOS:

- `.tar.gz`

Each package should contain:

- binary
- install script
- uninstall script
- `README.txt`

## If the release run fails

### Routing failure

Check:

- self-hosted runner online state
- exact labels on the runner
- workflow `runs-on` label list

### Packaging failure

Check:

- `scripts/package-release-windows.ps1`
- `scripts/package-release-unix.sh`

### Artifact verification failure

Check:

- `scripts/verify-release-artifact.ps1`
- actual package contents

### Publish failure

Check:

- release tag existence
- `GH_TOKEN` permissions
- artifact download path and glob behavior

## Related docs

- `docs/release-build-strategy.md`
- `docs/release-dry-run-plan.md`
- `docs/interactive-shell-release-checklist.md`
