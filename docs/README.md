# Documentation Guide

## Start here

If you want the current implemented state rather than long-range plans, read these first:

1. `docs/current-baseline.md`
2. `README.md`
3. `docs/implementation-plan.md`

That order gives you:

- what exists now
- why the project exists
- what should happen next

## Current runtime baseline

The current implemented floor is:

- `muldex-core` for protocol, harness, adapters, and orchestration boundaries
- `muldex-runtime` for bounded runtime state transitions
- `muldex-cli` for request ingestion, debugging, and single-step execution

Authoritative baseline document:

- `docs/current-baseline.md`

## Quick navigation

### Project framing

- `docs/problem-statement.md`
- `docs/architecture-boundary.md`
- `docs/adr/README.md`
- `docs/acceptance-criteria.md`

ADR note:

- accepted ADRs define current reference architecture
- proposed ADRs capture the next likely architecture decisions before implementation commits them

### Current implementation state

- `docs/current-baseline.md`
- `docs/codex-tui-compatibility-matrix.md`
- `docs/interactive-shell-guide.md`
- `docs/interactive-shell-validation.md`
- `docs/interactive-shell-release-checklist.md`
- `docs/interactive-shell-trial-handoff.md`
- `docs/release-build-strategy.md`
- `docs/release-dry-run-plan.md`
- `docs/local-release-runbook.md`
- `docs/installing-muldex-cli.md`
- `docs/windows-terminal-performance.md`
- `docs/client-contract-v1.md`
- `docs/runtime-gap-analysis.md`
- `docs/data-structures.md`

### Comparison and reference docs

- `docs/claude-code-comparison.md`
- `docs/jcode-comparison.md`
- `docs/capability-audit.md`
- `docs/agently-integration-options.md`

### Planning and execution

- `docs/implementation-plan.md`
- `docs/workstreams.md`
- `docs/todo.md`
- `docs/real-env-testing.md`

## How to validate the current baseline

From the workspace root:

```powershell
cargo test
```

Useful CLI checks:

```powershell
cargo run -p muldex-cli -- decide-sample --scenario healthy
cargo run -p muldex-cli -- decide-sample --scenario no-progress
cargo run -p muldex-cli -- decide-codex-snapshot examples/codex-bootstrap-snapshot.json
cargo run -p muldex-cli -- decide-codex-snapshot examples/codex-live-snapshot.json
cargo run -p muldex-cli -- demo-approval-resume
cargo run -p muldex-cli -- demo-host-persistence
cargo run -p muldex-cli -- save-host-snapshot --path .gitnexus/../tmp-muldex-host.json
cargo run -p muldex-cli -- load-host-snapshot --path .gitnexus/../tmp-muldex-host.json
cargo run -p muldex-cli -- import-codex-snapshot --path examples/codex-bootstrap-snapshot.json
cargo run -p muldex-cli -- export-session-view --path .gitnexus/../tmp-muldex-host.json --session-id sample-session --mode raw
cargo run -p muldex-cli -- export-session-view --path .gitnexus/../tmp-muldex-host.json --session-id sample-session --mode compressed
cargo run -p muldex-cli -- daemon-boot-empty --path .gitnexus/../tmp-muldex-daemon.json
cargo run -p muldex-cli -- daemon-boot-load --path .gitnexus/../tmp-muldex-daemon.json
cargo run -p muldex-cli -- daemon-status --path .gitnexus/../tmp-muldex-daemon.json
cargo run -p muldex-cli -- daemon-send-command --path .gitnexus/../tmp-muldex-daemon.json --command-id cmd-1 --session-id sample-session --kind advance-sample
cargo run -p muldex-cli -- daemon-read-response --path .gitnexus/../tmp-muldex-daemon.json --command-id cmd-1
cargo run -p muldex-cli -- daemon-serve-once --path .gitnexus/../tmp-muldex-daemon.json
cargo run -p muldex-cli -- daemon-serve-loop --path .gitnexus/../tmp-muldex-daemon.json --iterations 3
cargo run -p muldex-cli -- client-status --path .gitnexus/../tmp-muldex-daemon.json
cargo run -p muldex-cli -- client-send-command --path .gitnexus/../tmp-muldex-daemon.json --command-id cmd-2 --session-id sample-session --kind status
cargo run -p muldex-cli -- client-read-response --path .gitnexus/../tmp-muldex-daemon.json --command-id cmd-2
cargo run -p muldex-cli -- client-list-sessions --path .gitnexus/../tmp-muldex-daemon.json
cargo run -p muldex-cli -- client-inspect-session --path .gitnexus/../tmp-muldex-daemon.json --session-id sample-session --mode compressed
cargo run -p muldex-cli -- client-export-session --path .gitnexus/../tmp-muldex-daemon.json --session-id sample-session
```

These commands exercise the current request -> harness -> runtime-step -> report path.
The last three commands exercise the formal continuity command surface.
Daemon-prefixed commands exercise current daemon shell surface.

## Reading rule for future work

When a planning document and the baseline document disagree, treat `docs/current-baseline.md` as the source of truth for what is currently implemented.
