# Release Build Strategy

## Purpose

Define the intended release build topology for `muldex-cli` across supported operating systems and CPU architectures.

This document reflects the current planned release shape:

- GitHub release or tag triggers orchestration
- Windows x64 artifacts are built on the self-hosted Windows runner at `192.168.1.52`
- Linux artifacts are built on GitHub-hosted Ubuntu runners with explicit GNU toolchains
- Windows ARM64 is built on a GitHub-hosted Windows image with the ARM64 MSVC toolchain
- macOS artifacts are built on GitHub-hosted macOS runners

## Supported target matrix

Current intended target matrix:

- `x86_64-pc-windows-msvc`
- `aarch64-pc-windows-msvc`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

## Runner allocation

### Windows

Windows x64 release builds run on self-hosted infrastructure that uses the maintained local Visual Studio installation.

Current intended build host:

- `192.168.1.52`

GitHub Actions should orchestrate the release build, while Windows x64 compilation remains anchored to the known `.52` runner.

Important current interpretation:

- `.52` is a Windows self-hosted runner host
- Linux is intentionally built on hosted Ubuntu runners because the `.52` runner cannot reliably reach the external container registry required by its Podman cross-build image
- Windows ARM64 is also hosted because the `.52` image does not include the ARM64 MSVC libraries

Why:

- tighter control over toolchain drift
- consistent caches and system dependencies
- ability to preserve the already-proven local Windows x64 build environment

Current runner expectation:

- Windows x64 through the self-hosted Windows runner on `.52`
- Linux x64 and arm64 through GitHub-hosted Ubuntu runners, using the native x64 toolchain and `gcc-aarch64-linux-gnu` for ARM64
- Windows arm64 through the GitHub-hosted `windows-2025` runner, which includes the ARM64 MSVC libraries missing from `.52`

Operational expectation:

- GitHub Actions selects explicit self-hosted labels that map to the build image or runner fleet anchored at `192.168.1.52`
- release validation should confirm that Windows x64 uses `.52` and Linux jobs use the declared Ubuntu runner

### Linux

Linux release builds use GitHub-hosted Ubuntu runners. The x64 target uses the runner's native GNU toolchain; the ARM64 target installs `gcc-aarch64-linux-gnu` and `libc6-dev-arm64-cross`. This avoids a network-sensitive Podman image dependency while keeping the target and C toolchain explicit in the workflow.

### macOS

macOS release builds are intended to run on GitHub-hosted macOS runners.

Why:

- simpler Darwin build maintenance
- no requirement to operate a dedicated local macOS build image

## Artifact shape

Each target should eventually emit one packaged release artifact.

Suggested naming:

- `muldex-x86_64-pc-windows-msvc.zip`
- `muldex-aarch64-pc-windows-msvc.zip`
- `muldex-x86_64-unknown-linux-gnu.tar.gz`
- `muldex-aarch64-unknown-linux-gnu.tar.gz`
- `muldex-x86_64-apple-darwin.tar.gz`
- `muldex-aarch64-apple-darwin.tar.gz`

Suggested package contents:

- `muldex` or `muldex.exe`
- install script
- uninstall script
- pointer to shell usage and validation docs

Current packaging direction:

- Windows artifacts packaged as `.zip`
- Linux and macOS artifacts packaged as `.tar.gz`

Installer expectation:

- installation flow should ask whether to install and use `llm-router`
- installation flow should explain that `muldex` uses an OpenAI-compatible request shape
- installation flow should explain that many providers are not fully compatible with that shape and that `llm-router` is the recommended compatibility shim

## Validation gate before release packaging

Minimum current gate before release publication:

1. `cargo test -p muldex-cli`
2. `cargo test`
3. `scripts\validate-interactive-shell.ps1`
4. `scripts\prepare-interactive-shell-trial.ps1`

## Current release maturity

What exists now:

- interactive shell trial packaging script
- interactive shell validation script
- compatibility matrix and release checklist
- PTY-backed shell smoke plus scripted-key shell smoke

What is still not complete:

- final packaging per target
- install and uninstall scripts per target
- PATH registration flow per platform
- release artifact upload and aggregation into a formal GitHub release

## Workflow direction

The release workflow should eventually:

1. trigger on tag push or manual dispatch
2. run validation jobs first
3. build target artifacts on the correct runner classes
4. verify packaged artifact contents before upload
5. upload artifacts
6. aggregate artifacts into a GitHub release

Current workflow direction now includes a release publication step for `v*` tags that downloads packaged artifacts and attaches them to the corresponding GitHub Release.
