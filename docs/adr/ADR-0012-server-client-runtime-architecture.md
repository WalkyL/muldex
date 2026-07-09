# ADR-0012: Server-Client Runtime Architecture

## Status

Proposed

## Context

`muldex` now has:

- runtime driver
- session host
- daemon shell
- local ownership and lock metadata
- local file-based command transport skeleton

This is enough to justify a clearer future architecture decision:

- should runtime state continue to live inside whichever CLI process is active?
- or should one runtime owner serve many clients, including future mobile clients?

The project already wants long-running continuity and remote-friendly access patterns.

## Proposed decision

`muldex` should evolve into a server-client architecture.

The intended model is:

- one server or daemon process owns runtime state, host state, and persistence
- clients attach to that server over a transport boundary
- terminal, desktop, remote, and mobile surfaces are all clients, not runtime owners

## Intended responsibilities

### Server

- owns `RuntimeHost`
- owns `RuntimeDaemon`
- owns snapshot persistence
- owns command processing
- owns session continuity
- owns approval and safety state

### Client

- submits commands
- reads responses and state views
- presents operator interface
- may disconnect and reconnect without becoming session owner

## Current attach semantics

Current implementation supports a minimal attach-style client surface through CLI commands.

Current client capabilities include:

- inspect daemon status
- send commands
- read responses
- list sessions
- inspect one session view
- export one session snapshot

Current implementation note:

- `muldex-runtime` now includes explicit client view schemas for daemon status and session listing
- client inspection path can emit stable session views and compressed report payloads
- these views are suitable as early read-mostly client contracts for future mobile or remote surfaces
- client-facing daemon and session views now carry an explicit schema version and read-only capability allowlist
- `docs/client-contract-v1.md` now records concrete JSON examples for the current read-mostly client contract
- CLI client send path now enforces a minimal read-only access mode by default, allowing only read-safe command kinds unless access mode is explicitly widened
- daemon command and response envelopes now carry explicit schema version and payload kind tags

This is not yet a live attached session in the richer sense.

What is still missing:

- blocking attach stream
- push updates
- reconnect protocol
- concurrent client coordination rules

## Why this is preferred

- long-running work should not die with one UI surface
- mobile and remote attachment require detached ownership
- server-owned state matches current daemon and host direction
- multiple clients become possible without duplicating runtime state

## Consequences

Positive:

- mobile client support gains a natural architecture path
- continuity and resume become more robust
- CLI becomes one client among many instead of special owner

Negative:

- transport and authentication concerns grow
- multi-client concurrency and state visibility rules must be specified

## Rejected alternatives

### Keep CLI process as primary runtime owner forever

Rejected because it weakens detached continuity and blocks clean mobile attachment.

### Build mobile support as direct local file access over snapshots

Rejected because client surfaces should not directly mutate runtime persistence without server ownership.
