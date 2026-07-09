# ADR-0007: Long-Running Runtime Daemon Model

## Status

Accepted

## Context

The current architecture now has:

- `RuntimeDriver` for one session-scoped runtime
- `RuntimeHost` for multi-session in-memory hosting
- normalized runtime commands

This is close to a daemon-facing architecture, but there is not yet a process model.

## Decision

Build the daemon model around `RuntimeHost` as the process-owned authority.

The intended shape is:

- one long-running host process owns the active `RuntimeHost`
- clients send normalized runtime commands to that host
- session state, reports, and host snapshots are owned by the host process
- the CLI becomes a client surface when the daemon path exists

Current implementation note:

- `muldex-runtime` now includes a daemon shell object that owns `RuntimeHost`, lifecycle state, and snapshot path management
- IPC and true background process hosting are still future work

## Consequences

Positive:

- process ownership aligns with the current session-host boundary
- there is a direct path from in-memory host to daemon host
- client and daemon responsibilities remain distinct

Negative:

- daemon lifecycle and locking become first-class concerns
- command transport and host-state persistence must be specified cleanly

## Rejected alternatives

### Make each CLI invocation own its own runtime process forever

Rejected because it weakens long-running continuity and multi-session coordination.

### Build a daemon without a stable host/session model

Rejected because the current host boundary is the cleanest place to anchor a long-running process.
