# ADR Index

This directory records architecture decision records for `muldex`.

These ADRs are meant to capture decisions that are stable enough to reference repeatedly while the runtime evolves.

## Current ADRs

### Accepted

- [ADR-0001: Rust-First Kernel Authority](ADR-0001-rust-first-kernel-authority.md)
- [ADR-0002: Runtime Layering and Session Host](ADR-0002-runtime-layering-and-session-host.md)
- [ADR-0003: Normalized Runtime Command Boundary](ADR-0003-normalized-runtime-command-boundary.md)
- [ADR-0004: External Agent Runtime Through a Sidecar Seam](ADR-0004-external-agent-runtime-sidecar-seam.md)
- [ADR-0005: Multimodal Context Uses Bounded Derived Artifacts](ADR-0005-multimodal-context-bounded-derived-artifacts.md)
- [ADR-0006: File-Backed Runtime Host Persistence](ADR-0006-file-backed-runtime-host-persistence.md)
- [ADR-0007: Long-Running Runtime Daemon Model](ADR-0007-long-running-runtime-daemon-model.md)
- [ADR-0008: Resume, Import, and Export Contract](ADR-0008-resume-import-export-contract.md)
- [ADR-0009: Local Daemon Ownership and Command Transport](ADR-0009-local-daemon-ownership-and-command-transport.md)
- [ADR-0013: Default Interactive Shell and Codex Migration Surface](ADR-0013-default-interactive-shell-and-codex-migration-surface.md)
- [ADR-0014: Operator-Managed LLM Router Configuration](ADR-0014-operator-managed-llm-router-configuration.md)

### Proposed

- [ADR-0010: Harness-Safe Context Compression and Retention](ADR-0010-harness-safe-context-compression-and-retention.md)
- [ADR-0011: Daemon Lease, Heartbeat, and Stale-Owner Recovery](ADR-0011-daemon-lease-heartbeat-and-stale-owner-recovery.md)
- [ADR-0012: Server-Client Runtime Architecture](ADR-0012-server-client-runtime-architecture.md)


## Reading order

If you are new to the project, read in this order:

1. `docs/current-baseline.md`
2. `docs/architecture-boundary.md`
3. `docs/adr/ADR-0001-rust-first-kernel-authority.md`
4. `docs/adr/ADR-0002-runtime-layering-and-session-host.md`
5. `docs/adr/ADR-0003-normalized-runtime-command-boundary.md`

## Scope rule

An ADR should capture:

- the decision
- why it was chosen
- what alternatives were rejected
- the consequences of keeping it

It should not become a generic design notebook.

## Status rule

- `Accepted` means the decision should be treated as the current reference architecture.
- `Proposed` means the decision is under active consideration and should guide discussion, not override the current baseline.
