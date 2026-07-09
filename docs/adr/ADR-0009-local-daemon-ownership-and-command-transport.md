# ADR-0009: Local Daemon Ownership and Command Transport

## Status

Accepted

## Context

`muldex` now has:

- runtime driver
- in-memory host
- daemon shell
- file-backed host persistence
- continuity import and export surfaces

What it still lacks is a local ownership boundary for a future long-running daemon process.

Without that boundary, later work on command transport and process lifecycle could drift into unsafe multi-owner behavior.

## Decision

Introduce local daemon ownership and command transport in two layers:

1. a file-backed single-owner lock and daemon state metadata layer
2. a later local command transport layer that assumes one owner per runtime path

The intended rule is:

- one daemon shell owns one runtime path at a time
- ownership is established before local command transport is considered healthy

Current implementation note:

- first half is implemented
- `muldex-runtime` now includes local daemon lock metadata and daemon state metadata primitives
- `RuntimeDaemon` now integrates those primitives during boot, save, and shutdown
- minimal file-based command transport skeleton is now implemented at envelope and file-operation level
- local daemon command processing loop is now implemented in bounded foreground form
- cleanup and retention details still remain narrower than a full production daemon transport

## Transport appendix

The preferred first transport is a minimal local file-based command transport skeleton.

Intended early shape:

- command request envelope written by a client
- command response envelope written by daemon owner
- command directory scoped under daemon runtime root
- ownership lock required before transport is treated as valid

This is not intended as final transport. It is intended to validate:

- command envelope shape
- ownership assumptions
- request and response lifecycle
- later migration path to sockets or named pipes

## Consequences

Positive:

- process ownership semantics are explicit before IPC work begins
- later local transport can assume a stable owner
- crash recovery and operator introspection have a clear metadata anchor

Negative:

- lock and stale-owner policy must be specified carefully
- metadata shape may need revision once transport is added

## Rejected alternatives

### Build local command transport before ownership locking

Rejected because transport without ownership semantics risks multiple writers and ambiguous daemon state.

### Delay daemon metadata until after true background execution exists

Rejected because ownership and discoverability are preconditions for a safe background process model.
