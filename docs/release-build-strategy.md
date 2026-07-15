# Release Build Strategy

## Purpose

Define the intended release build topology for `muldex-cli` across supported operating systems and CPU architectures.

This document reflects the current planned release shape:

- GitHub release or tag triggers orchestration
- Windows x64 and Linux artifacts are built through self-hosted or locally controlled build images
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

### Windows and Linux

Windows x64 and Linux release builds are intended to run on self-hosted infrastructure that uses the maintained local build images.

Current intended build host:

- `192.168.1.52`

GitHub Actions should orchestrate release builds, but Windows x64 and Linux compilation should resolve onto the build image or runner hosted on that machine rather than drift onto arbitrary public runners.

Important current interpretation:

- `.52` is a Windows self-hosted runner host
- the Podman image on `.52` is the Linux build environment, not a separately registered Linux GitHub runner
- Windows x64 and Linux release jobs should therefore route to the same `.52` Windows runner and let the local build image determine the target build environment
- Windows ARM64 is the explicit exception because the `.52` image does not include the ARM64 MSVC libraries

Why:

- tighter control over toolchain drift
- consistent caches and system dependencies
- ability to preserve already-proven local build environment behavior

Current runner expectation:

- Windows x64 through the self-hosted Windows runner on `.52`
- Linux x64 and arm64 through the same `.52` Windows runner, using the local Podman build image as the Linux build environment
- Windows arm64 through the GitHub-hosted `windows-2025` runner, which includes the ARM64 MSVC libraries missing from `.52`

Operational expectation:

- GitHub Actions selects explicit self-hosted labels that map to the build image or runner fleet anchored at `192.168.1.52`
- release validation should confirm that Windows and Linux builds did not silently fall back to unrelated runners

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
