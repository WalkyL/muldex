# Release Dry-Run Plan

## Purpose

Describe how to exercise the current `muldex` release workflow end to end before calling it release-ready.

This is a dry-run plan, not a production release SOP.
Its job is to validate the release pipeline shape that now exists in the repository.

## Dry-run goal

Confirm that all of the following are true in one coordinated run:

1. GitHub Actions starts from the intended trigger
2. validation jobs pass
3. Windows and Linux jobs route onto the intended self-hosted infrastructure at `192.168.1.52`
4. macOS jobs run on GitHub-hosted macOS runners
5. packaged artifacts are produced for each target
6. artifact verification succeeds for each target
7. packaged artifacts are attached to the GitHub Release for a `v*` tag build

## Suggested trigger mode

Use a dedicated dry-run tag rather than a final release tag.

Suggested tag form:

- `v0.0.0-dryrun.1`

That keeps the pipeline realistic while avoiding confusion with a real release candidate.

## Preconditions

Before starting the dry run, confirm:

### Repository state

- the release workflow file is committed
- packaging scripts are committed
- install and uninstall scripts are committed
- validation and trial scripts are committed

### Self-hosted runner state

On the Windows/Linux build host at `192.168.1.52`, confirm:

- the self-hosted runner is online
- the runner carries the expected labels
  - `self-hosted`
  - `muldex`
  - `build-host-192-168-1-52`
  - one of:
    - `windows-x64`
    - `windows-arm64`
    - `linux-x64`
    - `linux-arm64`
- the local build image and toolchain are ready

### GitHub-side permissions

- workflow runs are allowed
- artifact upload is allowed
- the workflow token has permission to attach assets to releases

## Dry-run steps

### Step 1: local preflight

From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate-interactive-shell.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\prepare-interactive-shell-trial.ps1
```

Expected:

- both commands succeed
- no local regressions remain before remote workflow execution

### Step 2: create and push dry-run tag

Use a dry-run tag such as:

```powershell
git tag v0.0.0-dryrun.1
git push origin v0.0.0-dryrun.1
```

### Step 3: watch `validate` job

Expected:

- `cargo test -p muldex-cli` passes
- `cargo test` passes

If this fails:

- inspect shell and runtime regressions before looking at release packaging

### Step 4: verify runner routing

Expected:

- Windows/Linux jobs clearly report self-hosted labels that map to `192.168.1.52`
- macOS jobs clearly report GitHub-hosted macOS runners

Failure to route correctly means the dry run is invalid even if builds appear to pass.

### Step 5: verify build and packaging

Expected packaged outputs:

- `muldex-x86_64-pc-windows-msvc.zip`
- `muldex-aarch64-pc-windows-msvc.zip`
- `muldex-x86_64-unknown-linux-gnu.tar.gz`
- `muldex-aarch64-unknown-linux-gnu.tar.gz`
- `muldex-x86_64-apple-darwin.tar.gz`
- `muldex-aarch64-apple-darwin.tar.gz`

Expected package contents:

- binary
- install script
- uninstall script
- `README.txt`

### Step 6: verify artifact-check gate

Each platform job should pass the artifact verification script.

If verification fails:

- inspect whether the package is missing
  - binary
  - install script
  - uninstall script
  - `README.txt`

### Step 7: verify GitHub Release attach

Expected:

- a GitHub Release exists for the dry-run tag
- all packaged artifacts are attached
- no raw build directories are attached in place of packaged artifacts

## Failure inspection points

### If validation fails

Look at:

- shell state-machine tests
- scripted-key smoke
- PTY smoke
- runtime/client contract regressions

### If runner routing fails

Look at:

- workflow `runs-on` labels
- self-hosted runner label registration
- whether GitHub saw the intended runner as available

### If packaging fails

Look at:

- `package-release-windows.ps1`
- `package-release-unix.sh`
- path assumptions for built binaries

### If artifact verification fails

Look at:

- archive contents
- naming mismatch
- install/uninstall script copy steps

### If release attach fails

Look at:

- `GH_TOKEN` permissions
- release existence for the tag
- artifact path glob in the publish job

## Exit criteria for a successful dry run

Treat the dry run as successful only when:

1. validate job passes
2. runner routing is correct
3. all platform packaging jobs pass
4. artifact verification passes everywhere
5. all packaged artifacts attach to the GitHub Release

## After the dry run

If the dry run succeeds, record:

- exact runner routing observed
- artifact names produced
- any manual corrections required

Then update:

- `docs/interactive-shell-release-checklist.md`
- `docs/release-build-strategy.md`

to mark which release gates were proven in practice rather than only designed on paper.
